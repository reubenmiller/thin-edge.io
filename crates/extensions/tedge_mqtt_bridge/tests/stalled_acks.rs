//! Reproduction harness for a silent local->cloud stall of the built-in bridge.
//!
//! Field symptom: QoS 1 telemetry stops reaching the cloud while the cloud connection stays up
//! and QoS 0 JWT requests still work. The bridge acknowledges a local QoS 1 delivery only once
//! the cloud has acknowledged the forwarded copy, so if the cloud stops acknowledging, the
//! bridge accumulates unacknowledged local deliveries. With mosquitto as the local broker, that
//! stalls QoS 1 delivery to the bridge as soon as `max_inflight_messages` (20) deliveries are
//! outstanding, while QoS 0 keeps flowing and the bridge reports itself as up.
//!
//! This harness puts a packet-inspecting TCP proxy between the bridge and each broker:
//! - the local proxy keeps a ledger of PUBLISH (broker -> bridge) vs PUBACK (bridge -> broker),
//!   so any delivery the bridge never acknowledges shows up as unacknowledged
//! - the cloud proxy can cut connections, refuse connections (outage) and drop PUBACKs
//!
//! The local broker is a real mosquitto (spawned as a subprocess), so its inflight window
//! semantics apply exactly as on a device. The tests are skipped when mosquitto is not installed.

use bytes::BytesMut;
use rumqttc::mqttbytes::Error as PacketError;
use rumqttc::AsyncClient;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::MqttOptions;
use rumqttc::Packet;
use rumqttc::QoS;
use rumqttd::Broker;
use rumqttd::Config;
use rumqttd::ConnectionSettings;
use rumqttd::ServerSettings;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::Range;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;
use tedge_config::TEdgeConfig;
use tedge_mqtt_bridge::BridgeConfig;
use tedge_mqtt_bridge::MqttBridgeActorBuilder;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio::time::sleep;

const MAX_PACKET: usize = 268435455;
const SERVICE_NAME: &str = "tedge-mapper-test";
const MOSQUITTO_MAX_INFLIGHT: usize = 20; // mosquitto default
/// After this long without any acknowledgement progress the bridge is expected to reconnect
const ACK_TIMEOUT_SECS: u64 = 5;

// ---------------------------------------------------------------------------------------------
// Packet-inspecting chaos proxy
// ---------------------------------------------------------------------------------------------

#[derive(Default, Debug)]
struct Ledger {
    /// QoS>0 PUBLISH packets sent by the "publisher" side and not yet acknowledged (pkid -> topic)
    inflight: HashMap<u16, String>,
    delivered: usize,
    redelivered: usize,
    acked: usize,
    unknown_acks: usize,
    qos0_delivered: usize,
    connections: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dir {
    /// bridge -> broker
    ToBroker,
    /// broker -> bridge
    ToClient,
}

#[derive(Clone)]
struct ChaosProxy {
    name: &'static str,
    port: u16,
    /// Direction in which PUBLISH packets are accounted (the opposite carries the PUBACKs)
    publish_dir: Dir,
    stop_tx: Arc<watch::Sender<()>>,
    ledger: Arc<Mutex<Ledger>>,
    /// Drop every n-th PUBACK travelling in the ack direction (0 = never drop)
    drop_puback_every: Arc<AtomicUsize>,
    puback_counter: Arc<AtomicUsize>,
    /// Refuse new connections (broker unreachable)
    refuse: Arc<AtomicBool>,
    /// Stop dropping PUBACKs as soon as the bridge establishes a new connection
    drop_pubacks_until_reconnect: Arc<AtomicBool>,
    /// Drop only this many PUBACKs, then let the rest through (0 = no limit)
    drop_at_most: Arc<AtomicUsize>,
    dropped: Arc<AtomicUsize>,
}

impl ChaosProxy {
    async fn start(name: &'static str, target_port: u16, publish_dir: Dir) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (stop_tx, _) = watch::channel(());
        let proxy = Self {
            name,
            port,
            publish_dir,
            stop_tx: Arc::new(stop_tx),
            ledger: Default::default(),
            drop_puback_every: Default::default(),
            puback_counter: Default::default(),
            refuse: Default::default(),
            drop_pubacks_until_reconnect: Default::default(),
            drop_at_most: Default::default(),
            dropped: Default::default(),
        };
        let p = proxy.clone();
        tokio::spawn(async move {
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    continue;
                };
                if p.refuse.load(Ordering::Relaxed) {
                    drop(socket);
                    continue;
                }
                let target = loop {
                    match TcpStream::connect(("127.0.0.1", target_port)).await {
                        Ok(c) => break c,
                        Err(_) => sleep(Duration::from_millis(20)).await,
                    }
                };
                if p.ledger.lock().unwrap().connections > 0
                    && p.drop_pubacks_until_reconnect
                        .swap(false, Ordering::Relaxed)
                {
                    p.drop_puback_every.store(0, Ordering::Relaxed);
                }
                p.ledger.lock().unwrap().connections += 1;
                let stop = p.stop_tx.subscribe();
                let (client_rd, client_wr) = socket.into_split();
                let (broker_rd, broker_wr) = target.into_split();
                let p1 = p.clone();
                let p2 = p.clone();
                tokio::spawn(async move {
                    tokio::select! {
                        _ = pump(client_rd, broker_wr, stop.clone(), move |pkt| p1.inspect(Dir::ToBroker, pkt)) => (),
                        _ = pump(broker_rd, client_wr, stop, move |pkt| p2.inspect(Dir::ToClient, pkt)) => (),
                    }
                    // dropping the halves closes both sockets
                });
            }
        });
        proxy
    }

    /// Returns whether the packet should be forwarded
    fn inspect(&self, dir: Dir, packet: &Packet) -> bool {
        let mut ledger = self.ledger.lock().unwrap();
        match packet {
            Packet::Publish(publish) if dir == self.publish_dir => {
                if publish.qos == QoS::AtMostOnce {
                    ledger.qos0_delivered += 1;
                } else {
                    ledger.delivered += 1;
                    if publish.dup {
                        ledger.redelivered += 1;
                    }
                    ledger.inflight.insert(publish.pkid, publish.topic.clone());
                }
                true
            }
            Packet::PubAck(ack) if dir != self.publish_dir => {
                let every = self.drop_puback_every.load(Ordering::Relaxed);
                let at_most = self.drop_at_most.load(Ordering::Relaxed);
                if every > 0 && (at_most == 0 || self.dropped.load(Ordering::Relaxed) < at_most) {
                    let n = self.puback_counter.fetch_add(1, Ordering::Relaxed) + 1;
                    if n.is_multiple_of(every) {
                        self.dropped.fetch_add(1, Ordering::Relaxed);
                        return false; // drop the ack: the publisher never learns it was received
                    }
                }
                if ledger.inflight.remove(&ack.pkid).is_some() {
                    ledger.acked += 1;
                } else {
                    ledger.unknown_acks += 1;
                }
                true
            }
            _ => true,
        }
    }

    fn interrupt_connections(&self) {
        let _ = self.stop_tx.send(());
    }

    fn set_outage(&self, outage: bool) {
        self.refuse.store(outage, Ordering::Relaxed);
        if outage {
            self.interrupt_connections();
        }
    }

    fn drop_puback_every(&self, n: usize) {
        self.drop_puback_every.store(n, Ordering::Relaxed);
    }

    /// Drops every n-th PUBACK on the current connection; a new connection acknowledges everything
    fn drop_puback_every_until_reconnect(&self, n: usize) {
        self.drop_pubacks_until_reconnect
            .store(true, Ordering::Relaxed);
        self.drop_puback_every(n);
    }

    /// Drops only the next PUBACK on the current connection, then acknowledges normally
    fn drop_next_puback_until_reconnect(&self) {
        self.drop_at_most.store(1, Ordering::Relaxed);
        self.drop_puback_every_until_reconnect(1);
    }

    fn connections(&self) -> usize {
        self.ledger.lock().unwrap().connections
    }

    fn snapshot(&self) -> String {
        let l = self.ledger.lock().unwrap();
        let mut pkids: Vec<_> = l.inflight.keys().copied().collect();
        pkids.sort();
        format!(
            "[{}] connections={} qos1 delivered={} (redelivered={}) acked={} unknown_acks={} qos0={} UNACKED={} pkids={:?}",
            self.name,
            l.connections,
            l.delivered,
            l.redelivered,
            l.acked,
            l.unknown_acks,
            l.qos0_delivered,
            l.inflight.len(),
            pkids
        )
    }

    fn unacked(&self) -> usize {
        self.ledger.lock().unwrap().inflight.len()
    }
}

async fn pump(
    mut from: OwnedReadHalf,
    mut to: OwnedWriteHalf,
    mut stop: watch::Receiver<()>,
    mut inspect: impl FnMut(&Packet) -> bool,
) -> std::io::Result<()> {
    let mut buf = BytesMut::with_capacity(64 * 1024);
    loop {
        let n = tokio::select! {
            n = from.read_buf(&mut buf) => n?,
            _ = stop.changed() => return Ok(()),
        };
        if n == 0 {
            return Ok(());
        }
        loop {
            let snapshot = buf.clone();
            match Packet::read(&mut buf, MAX_PACKET) {
                Ok(packet) => {
                    let consumed = snapshot.len() - buf.len();
                    if inspect(&packet) {
                        to.write_all(&snapshot[..consumed]).await?;
                    }
                }
                Err(PacketError::InsufficientBytes(_)) => break,
                Err(e) => panic!("proxy failed to parse MQTT packet: {e:?}"),
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Brokers
// ---------------------------------------------------------------------------------------------

struct Mosquitto {
    child: Child,
    port: u16,
    log_path: std::path::PathBuf,
}

impl Mosquitto {
    async fn start(max_inflight: usize) -> Option<Self> {
        if Command::new("mosquitto")
            .arg("-h")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_err()
        {
            return None;
        }
        let port = free_port().await;
        let dir = std::env::temp_dir().join(format!("tedge-bridge-mosq-{port}"));
        std::fs::create_dir_all(&dir).unwrap();
        let log_path = dir.join("mosquitto.log");
        let cfg = dir.join("mosquitto.conf");
        std::fs::write(
            &cfg,
            format!(
                "listener {port} 127.0.0.1\nallow_anonymous true\npersistence false\n\
                 max_inflight_messages {max_inflight}\nlog_dest file {}\nlog_type all\n",
                log_path.display()
            ),
        )
        .unwrap();
        let child = Command::new("mosquitto")
            .arg("-c")
            .arg(&cfg)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        wait_until_port_listening(port).await;
        Some(Self {
            child,
            port,
            log_path,
        })
    }

    fn log_lines_matching(&self, needle: &str) -> Vec<String> {
        std::fs::read_to_string(&self.log_path)
            .unwrap_or_default()
            .lines()
            .filter(|l| l.contains(needle))
            .map(|s| s.to_owned())
            .collect()
    }
}

impl Drop for Mosquitto {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_rumqttd(port: u16) {
    let router_config = rumqttd::RouterConfig {
        max_segment_size: 10240,
        max_segment_count: 10,
        max_connections: 10,
        initialized_filters: None,
        ..Default::default()
    };
    let connections_settings = ConnectionSettings {
        connection_timeout_ms: 1000,
        max_payload_size: MAX_PACKET,
        max_inflight_count: 200,
        auth: None,
        external_auth: None,
        dynamic_filters: false,
    };
    let server_config = ServerSettings {
        name: "cloud".to_owned(),
        listen: ([127, 0, 0, 1], port).into(),
        tls: None,
        next_connection_delay_ms: 1,
        connections: connections_settings,
    };
    let mut servers = HashMap::new();
    servers.insert("cloud".to_owned(), server_config);
    let config = Config {
        id: 0,
        router: router_config,
        cluster: None,
        console: None,
        v4: Some(servers),
        v5: None,
        ws: None,
        bridge: None,
        prometheus: None,
        metrics: None,
    };
    let mut broker = Broker::new(config);
    std::thread::Builder::new()
        .name("cloud broker".into())
        .spawn(move || broker.start().unwrap())
        .unwrap();
}

async fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn wait_until_port_listening(port: u16) {
    for _ in 0..500 {
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return;
        }
        sleep(Duration::from_millis(10)).await;
    }
    panic!("port {port} never started listening");
}

fn tedge_mqtt_config(mqtt_port: u16) -> TEdgeConfig {
    TEdgeConfig::load_toml_str(&format!(
        "
    mqtt.client.port = {mqtt_port}
    mqtt.bridge.reconnect_policy.initial_interval = \"0s\"
    mqtt.bridge.ack_timeout = \"{ACK_TIMEOUT_SECS}s\"
    "
    ))
}

// ---------------------------------------------------------------------------------------------
// Test rig
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct CloudReceived {
    distinct: HashSet<String>,
    duplicates: usize,
    qos0: usize,
}

struct Rig {
    mosquitto: Mosquitto,
    local_proxy: ChaosProxy,
    cloud_proxy: ChaosProxy,
    /// Publisher connected straight to mosquitto (plays the role of tedge-mapper-c8y)
    local: AsyncClient,
    cloud_received: Arc<Mutex<CloudReceived>>,
    health: Arc<Mutex<Option<String>>>,
    started: Instant,
}

impl Rig {
    async fn start() -> Option<Self> {
        let _ = env_logger::builder()
            .parse_filters(&std::env::var("RUST_LOG").unwrap_or("tedge_mqtt_bridge=info".into()))
            .is_test(false)
            .try_init();
        let mosquitto = Mosquitto::start(MOSQUITTO_MAX_INFLIGHT).await?;
        let cloud_port = free_port().await;
        start_rumqttd(cloud_port);
        wait_until_port_listening(cloud_port).await;

        let local_proxy = ChaosProxy::start("local", mosquitto.port, Dir::ToClient).await;
        let cloud_proxy = ChaosProxy::start("cloud", cloud_port, Dir::ToBroker).await;

        let mut rules = BridgeConfig::new();
        rules.forward_from_local("s/us", "c8y/", "").unwrap();
        rules.forward_from_local("s/uat", "c8y/", "").unwrap();
        rules.forward_from_remote("s/ds", "c8y/", "").unwrap();

        let cloud_config = MqttOptions::new("a-device-id", "127.0.0.1", cloud_proxy.port);
        let health_topic = format!("te/device/main/service/{SERVICE_NAME}/status/health")
            .as_str()
            .try_into()
            .unwrap();
        MqttBridgeActorBuilder::new(
            &tedge_mqtt_config(local_proxy.port),
            SERVICE_NAME,
            &health_topic,
            rules,
            cloud_config,
            None,
            MAX_PACKET,
        )
        .await;

        // Cloud-side subscriber (plays Cumulocity's consumer), straight to the cloud broker
        let mut opts = MqttOptions::new("cloud-subscriber", "127.0.0.1", cloud_port);
        opts.set_max_packet_size(MAX_PACKET, MAX_PACKET);
        let (cloud, mut ev_cloud) = AsyncClient::new(opts, 100);
        cloud.subscribe("s/us", QoS::AtLeastOnce).await.unwrap();
        cloud.subscribe("s/uat", QoS::AtLeastOnce).await.unwrap();
        let cloud_received: Arc<Mutex<CloudReceived>> = Default::default();
        let received = cloud_received.clone();
        tokio::spawn(async move {
            loop {
                if let Ok(Event::Incoming(Incoming::Publish(p))) = ev_cloud.poll().await {
                    let mut r = received.lock().unwrap();
                    if p.topic == "s/uat" {
                        r.qos0 += 1;
                    } else if !r
                        .distinct
                        .insert(String::from_utf8_lossy(&p.payload).to_string())
                    {
                        r.duplicates += 1;
                    }
                }
            }
        });

        // Local publisher + health watcher, straight to mosquitto
        let mut opts = MqttOptions::new("tedge-mapper-c8y", "127.0.0.1", mosquitto.port);
        opts.set_max_packet_size(MAX_PACKET, MAX_PACKET);
        let (local, mut ev_local) = AsyncClient::new(opts, 100);
        local
            .subscribe(
                format!("te/device/main/service/{SERVICE_NAME}/status/health"),
                QoS::AtLeastOnce,
            )
            .await
            .unwrap();
        let health: Arc<Mutex<Option<String>>> = Default::default();
        let h = health.clone();
        tokio::spawn(async move {
            loop {
                if let Ok(Event::Incoming(Incoming::Publish(p))) = ev_local.poll().await {
                    let json: serde_json::Value =
                        serde_json::from_slice(&p.payload).unwrap_or_default();
                    *h.lock().unwrap() = json["status"].as_str().map(|s| s.to_owned());
                }
            }
        });

        let rig = Self {
            mosquitto,
            local_proxy,
            cloud_proxy,
            local,
            cloud_received,
            health,
            started: Instant::now(),
        };
        rig.wait_health("up", Duration::from_secs(10)).await;
        Some(rig)
    }

    async fn wait_health(&self, expected: &str, timeout: Duration) {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if self.health.lock().unwrap().as_deref() == Some(expected) {
                return;
            }
            sleep(Duration::from_millis(20)).await;
        }
        panic!(
            "bridge health never became {expected:?} (last: {:?})",
            self.health.lock().unwrap()
        );
    }

    /// Publishes QoS 1 messages `range` on c8y/s/us with `gap` between them
    async fn publish_qos1(&self, range: Range<usize>, gap: Duration) {
        for i in range {
            self.local
                .publish("c8y/s/us", QoS::AtLeastOnce, false, format!("{i}"))
                .await
                .unwrap();
            if !gap.is_zero() {
                sleep(gap).await;
            }
        }
    }

    /// Waits until the cloud has received `expected` distinct messages, returning how many it got
    async fn wait_cloud_received(&self, expected: usize, timeout: Duration) -> usize {
        let start = Instant::now();
        loop {
            let got = self.cloud_received.lock().unwrap().distinct.len();
            if got >= expected || start.elapsed() > timeout {
                return got;
            }
            sleep(Duration::from_millis(50)).await;
        }
    }

    fn report(&self, label: &str) {
        let r = self.cloud_received.lock().unwrap();
        eprintln!(
            "--- {label} (t+{:.1}s)\n    cloud received: distinct={} duplicates={} qos0={}\n    {}\n    {}\n    health={:?}",
            self.started.elapsed().as_secs_f32(),
            r.distinct.len(),
            r.duplicates,
            r.qos0,
            self.local_proxy.snapshot(),
            self.cloud_proxy.snapshot(),
            self.health.lock().unwrap(),
        );
        for line in self.mosquitto.log_lines_matching("dropped") {
            eprintln!("    mosquitto: {line}");
        }
    }

    /// Proves the local->cloud QoS 1 path is (still) alive
    async fn probe_qos1(&self, id: usize) -> bool {
        self.publish_qos1(id..id + 1, Duration::ZERO).await;
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if self
                .cloud_received
                .lock()
                .unwrap()
                .distinct
                .contains(&id.to_string())
            {
                return true;
            }
            sleep(Duration::from_millis(50)).await;
        }
        false
    }

    /// Proves the local->cloud QoS 0 path is alive (the JWT request path)
    async fn probe_qos0(&self) -> bool {
        let before = self.cloud_received.lock().unwrap().qos0;
        self.local
            .publish("c8y/s/uat", QoS::AtMostOnce, false, "")
            .await
            .unwrap();
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if self.cloud_received.lock().unwrap().qos0 > before {
                return true;
            }
            sleep(Duration::from_millis(50)).await;
        }
        false
    }
}

/// Cuts the proxy's connections at random intervals within `every` for `duration`
fn flap(
    proxy: ChaosProxy,
    every: Range<u64>,
    duration: Duration,
) -> tokio::task::JoinHandle<usize> {
    static SEED: AtomicU64 = AtomicU64::new(0x9E3779B97F4A7C15);
    tokio::spawn(async move {
        let start = Instant::now();
        let mut cuts = 0;
        while start.elapsed() < duration {
            let mut x = SEED.load(Ordering::Relaxed);
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            SEED.store(x, Ordering::Relaxed);
            let wait = every.start + x % (every.end - every.start);
            sleep(Duration::from_millis(wait)).await;
            proxy.interrupt_connections();
            cuts += 1;
        }
        cuts
    })
}

macro_rules! rig {
    () => {
        match Rig::start().await {
            Some(rig) => rig,
            None => {
                eprintln!("mosquitto not found in PATH, skipping");
                return;
            }
        }
    };
}

// ---------------------------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------------------------

/// Baseline: the QoS 1 acknowledgement path is airtight when nothing goes wrong
#[tokio::test]
async fn baseline_all_local_deliveries_are_acked() {
    let rig = rig!();
    rig.publish_qos1(0..500, Duration::from_millis(1)).await;
    let got = rig.wait_cloud_received(500, Duration::from_secs(20)).await;
    sleep(Duration::from_secs(1)).await;
    rig.report("baseline");
    assert_eq!(got, 500);
    assert_eq!(
        rig.local_proxy.unacked(),
        0,
        "{}",
        rig.local_proxy.snapshot()
    );
}

/// The cloud connection is cut repeatedly while QoS 1 telemetry flows
#[tokio::test]
async fn cloud_flapping_under_qos1_load() {
    let rig = rig!();
    let chaos = flap(rig.cloud_proxy.clone(), 50..300, Duration::from_secs(8));
    rig.publish_qos1(0..1500, Duration::from_millis(4)).await;
    let cuts = chaos.await.unwrap();
    rig.wait_health("up", Duration::from_secs(20)).await;
    let got = rig.wait_cloud_received(1500, Duration::from_secs(30)).await;
    sleep(Duration::from_secs(2)).await;
    rig.report(&format!("after {cuts} cloud cuts"));
    assert!(rig.probe_qos1(9_000).await, "QoS 1 path is stalled");
    assert_eq!(got, 1500, "messages lost");
    assert_eq!(
        rig.local_proxy.unacked(),
        0,
        "leaked local acks: {}",
        rig.local_proxy.snapshot()
    );
}

/// The local (mosquitto) connection is cut repeatedly while QoS 1 telemetry flows
#[tokio::test]
async fn local_flapping_under_qos1_load() {
    let rig = rig!();
    let chaos = flap(rig.local_proxy.clone(), 50..300, Duration::from_secs(8));
    rig.publish_qos1(0..1500, Duration::from_millis(4)).await;
    let cuts = chaos.await.unwrap();
    rig.wait_health("up", Duration::from_secs(20)).await;
    let got = rig.wait_cloud_received(1500, Duration::from_secs(30)).await;
    sleep(Duration::from_secs(2)).await;
    rig.report(&format!("after {cuts} local cuts"));
    assert!(rig.probe_qos1(9_000).await, "QoS 1 path is stalled");
    assert_eq!(got, 1500, "messages lost");
    assert_eq!(
        rig.local_proxy.unacked(),
        0,
        "leaked local acks: {}",
        rig.local_proxy.snapshot()
    );
}

/// Both connections are cut, independently, while QoS 1 telemetry flows
#[tokio::test]
async fn both_connections_flapping_under_qos1_load() {
    let rig = rig!();
    let chaos_cloud = flap(rig.cloud_proxy.clone(), 50..400, Duration::from_secs(8));
    let chaos_local = flap(rig.local_proxy.clone(), 100..600, Duration::from_secs(8));
    rig.publish_qos1(0..1500, Duration::from_millis(4)).await;
    let cuts = chaos_cloud.await.unwrap() + chaos_local.await.unwrap();
    rig.wait_health("up", Duration::from_secs(20)).await;
    let got = rig.wait_cloud_received(1500, Duration::from_secs(30)).await;
    sleep(Duration::from_secs(2)).await;
    rig.report(&format!("after {cuts} cuts"));
    assert!(rig.probe_qos1(9_000).await, "QoS 1 path is stalled");
    assert_eq!(got, 1500, "messages lost");
    assert_eq!(
        rig.local_proxy.unacked(),
        0,
        "leaked local acks: {}",
        rig.local_proxy.snapshot()
    );
}

/// The cloud is unreachable for a while (DNS/network outage) and a backlog builds up locally,
/// with the outage interrupted by a few short-lived connections
#[tokio::test]
async fn cloud_outage_with_local_backlog() {
    let rig = rig!();
    rig.cloud_proxy.set_outage(true);
    rig.publish_qos1(0..300, Duration::from_millis(2)).await;
    // brief windows where the cloud is reachable again, cut immediately
    for _ in 0..3 {
        rig.cloud_proxy.set_outage(false);
        sleep(Duration::from_millis(150)).await;
        rig.cloud_proxy.set_outage(true);
        rig.publish_qos1(300..400, Duration::from_millis(2)).await;
    }
    rig.report("during outage");
    rig.cloud_proxy.set_outage(false);
    rig.wait_health("up", Duration::from_secs(20)).await;
    let got = rig.wait_cloud_received(400, Duration::from_secs(30)).await;
    sleep(Duration::from_secs(2)).await;
    rig.report("after outage");
    assert!(rig.probe_qos1(9_000).await, "QoS 1 path is stalled");
    assert_eq!(got, 400, "messages lost");
    assert_eq!(
        rig.local_proxy.unacked(),
        0,
        "leaked local acks: {}",
        rig.local_proxy.snapshot()
    );
}

/// The cloud broker silently drops some PUBACKs; a reconnection later makes the bridge resend
/// the unacknowledged messages, after which nothing may remain unacknowledged locally
#[tokio::test]
async fn cloud_drops_pubacks_until_a_reconnection() {
    let rig = rig!();
    rig.cloud_proxy.drop_puback_every(3);
    rig.publish_qos1(0..200, Duration::from_millis(2)).await;
    sleep(Duration::from_secs(3)).await;
    rig.report("while the cloud drops every 3rd PUBACK");
    let stalled = !rig.probe_qos1(9_000).await;
    eprintln!("    QoS 1 path stalled while acks are being dropped: {stalled}");
    assert!(rig.probe_qos0().await, "QoS 0 path should still work");

    rig.cloud_proxy.drop_puback_every(0);
    rig.cloud_proxy.interrupt_connections();
    rig.wait_health("up", Duration::from_secs(20)).await;
    let got = rig.wait_cloud_received(201, Duration::from_secs(30)).await;
    sleep(Duration::from_secs(2)).await;
    rig.report("after the cloud reconnected and acks all messages");
    assert!(
        rig.probe_qos1(9_001).await,
        "QoS 1 path is stalled after reconnection"
    );
    assert_eq!(got, 201, "messages lost");
    assert_eq!(
        rig.local_proxy.unacked(),
        0,
        "leaked local acks: {}",
        rig.local_proxy.snapshot()
    );
}

/// A single message the cloud never acknowledges, while it keeps acknowledging everything
/// else, must not be left behind: it would occupy one slot of the mosquitto inflight window
/// for the life of the connection, and 20 such messages over time would stall the device.
#[tokio::test]
async fn bridge_recovers_a_single_message_the_cloud_never_acknowledges() {
    let rig = rig!();
    rig.cloud_proxy.drop_next_puback_until_reconnect();
    // Steady traffic for longer than the ack timeout: every other message is acknowledged
    rig.publish_qos1(0..60, Duration::from_millis(120)).await;
    let got = rig
        .wait_cloud_received(60, Duration::from_secs(ACK_TIMEOUT_SECS * 4))
        .await;
    sleep(Duration::from_secs(1)).await;
    rig.report("after steady traffic with one withheld PUBACK");
    assert!(
        rig.cloud_proxy.connections() >= 2,
        "the bridge never reconnected although one message was never acknowledged"
    );
    assert_eq!(got, 60, "messages never reached the cloud");
    assert_eq!(
        rig.local_proxy.unacked(),
        0,
        "leaked local acks: {}",
        rig.local_proxy.snapshot()
    );
}

/// Fingerprint of the field report, and the recovery expected from the bridge.
///
/// The cloud accepts the connection and the publishes but stops acknowledging them. Nothing
/// ever fails, so without a watchdog the bridge sits there forever: mosquitto holds back QoS 1
/// once 20 deliveries are unacknowledged, QoS 0 (the JWT request) still flows, health stays up.
/// The bridge is expected to notice that acknowledgements stopped, reconnect, and resend.
#[tokio::test]
async fn bridge_recovers_when_the_cloud_stops_acknowledging() {
    let rig = rig!();
    rig.cloud_proxy.drop_puback_every_until_reconnect(1);
    rig.publish_qos1(0..60, Duration::from_millis(2)).await;
    sleep(Duration::from_millis(1500)).await;
    let qos0_alive = rig.probe_qos0().await;
    rig.report("cloud stopped acknowledging");
    eprintln!(
        "    fingerprint: unacked={} qos0_alive={qos0_alive} cloud_connections={} health={:?}",
        rig.local_proxy.unacked(),
        rig.cloud_proxy.connections(),
        rig.health.lock().unwrap()
    );
    assert_eq!(
        rig.local_proxy.unacked(),
        MOSQUITTO_MAX_INFLIGHT,
        "mosquitto inflight window not exhausted"
    );
    assert!(
        qos0_alive,
        "QoS 0 should bypass the mosquitto inflight window"
    );

    // The bridge must reconnect on its own and resend what was never acknowledged
    let got = rig
        .wait_cloud_received(60, Duration::from_secs(ACK_TIMEOUT_SECS * 4))
        .await;
    sleep(Duration::from_secs(1)).await;
    rig.report("after the expected recovery");
    let qos1_alive = rig.probe_qos1(9_000).await;
    rig.report("after probing QoS 1");
    assert!(
        rig.cloud_proxy.connections() >= 2,
        "the bridge never reconnected although acknowledgements stopped for more than {ACK_TIMEOUT_SECS}s"
    );
    assert_eq!(got, 60, "messages never reached the cloud");
    assert!(qos1_alive, "QoS 1 path is still stalled");
    assert_eq!(
        rig.local_proxy.unacked(),
        0,
        "leaked local acks: {}",
        rig.local_proxy.snapshot()
    );
}
