//! Throughput harness for the flows mapper.
//!
//! Runs the real `FlowsMapper` actor against an in-memory MQTT peer
//! (no broker, no cloud), loads `FLOWS` independent JavaScript flows
//! and pushes `MSGS` messages through each of them concurrently.
//!
//! ```sh
//! FLOWS=4 MSGS=20000 WORK=200 cargo run --release -p tedge_flows --example flows_throughput
//! ```
//!
//! - `FLOWS`: number of independent flows, each with its own input topic (default 4)
//! - `MSGS`: messages pushed per flow (default 20000)
//! - `WORK`: extra loop iterations per message in the script, to emulate heavier logic (default 200)
//! - `TOKIO_WORKERS`: tokio worker threads (default: number of cores)
//! - `JS_WORKERS`: number of JS runtimes (default 1)
//! - `FLOW_WORKERS`: number of copies of each flow, capped by `JS_WORKERS` (default 1)
//! - `MAPPER`: use of `context.mapper` by the flows (default `none`)
//!   - `none`: no flow uses it
//!   - `read`: all the flows read it
//!   - `mixed`: the first flow writes it, the others read it
//!
//! Run it under `/usr/bin/time` (or sample threads with `ps -M` / `top -H`)
//! to see how many cores the process actually uses.

use camino::Utf8Path;
use futures::channel::mpsc;
use futures::StreamExt;
use std::collections::HashMap;
use std::time::Duration;
use std::time::Instant;
use tedge_actors::Actor as _;
use tedge_actors::Builder;
use tedge_actors::CloneSender as _;
use tedge_actors::DynSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::Sender as _;
use tedge_flows::ConnectedFlowRegistry;
use tedge_flows::FlowsMapperBuilder;
use tedge_flows::FlowsMapperConfig;
use tedge_mqtt_ext::DynSubscriptions;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::MqttRequest;
use tedge_mqtt_ext::Topic;
use tedge_utils::paths::TedgePaths;

/// The script run by each flow, with `MAPPER_ACCESS` replaced by its use of `context.mapper`
const SCRIPT: &str = r#"
const utf8 = new TextDecoder();

export function onMessage(message, context) {
    const payload = JSON.parse(utf8.decode(message.payload));
    MAPPER_ACCESS
    const work = context.config.work;
    let acc = 0;
    for (let i = 0; i < work; i++) {
        acc += Math.sqrt(i * payload.temperature);
    }
    const c8y = {
        type: "ThinEdgeMeasurement",
        time: payload.time,
        temperature: { value: { value: payload.temperature, unit: "C" } },
        humidity: { value: { value: payload.humidity, unit: "%" } },
        checksum: acc,
    };
    return [{ topic: message.topic.replace("in", "out"), payload: JSON.stringify(c8y) }];
}
"#;

fn env_or(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let flows = env_or("FLOWS", 4);
    let msgs = env_or("MSGS", 20000);
    let work = env_or("WORK", 200);
    let workers = env_or(
        "TOKIO_WORKERS",
        std::thread::available_parallelism().map_or(1, |n| n.get()),
    );
    let js_workers = env_or("JS_WORKERS", 1);
    let flow_workers = env_or("FLOW_WORKERS", 1);
    let mapper = std::env::var("MAPPER").unwrap_or_else(|_| "none".to_string());

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
        .expect("tokio runtime");
    let settings = Settings {
        flows,
        msgs,
        work,
        workers,
        js_workers,
        flow_workers,
        mapper,
    };
    runtime.block_on(run(settings));
}

struct Settings {
    flows: usize,
    msgs: usize,
    work: usize,
    workers: usize,
    js_workers: usize,
    flow_workers: usize,
    mapper: String,
}

async fn run(settings: Settings) {
    let Settings {
        flows,
        msgs,
        work,
        workers,
        js_workers,
        flow_workers,
        mapper,
    } = settings;
    let temp_dir = tempfile::TempDir::new().expect("temp dir");
    let config_dir = temp_dir.path();
    let script = |access: &str| SCRIPT.replace("MAPPER_ACCESS", access);
    std::fs::write(config_dir.join("bench.js"), script("")).expect("write script");
    std::fs::write(
        config_dir.join("reader.js"),
        script(r#"const last = context.mapper.get("bench/last");"#),
    )
    .expect("write script");
    std::fs::write(
        config_dir.join("writer.js"),
        script(r#"context.mapper.set("bench/last", payload.temperature);"#),
    )
    .expect("write script");
    for i in 0..flows {
        let script = match (mapper.as_str(), i) {
            ("read", _) | ("mixed", 1..) => "reader.js",
            ("mixed", 0) => "writer.js",
            _ => "bench.js",
        };
        let config = format!(
            r#"
            workers = {flow_workers}
            input.mqtt.topics = ["bench/in/{i}"]
            steps = [{{ script = "{script}", config = {{ work = {work} }} }}]
            output.mqtt.topic = "bench/out/{i}"
            "#
        );
        std::fs::write(config_dir.join(format!("flow{i}.toml")), config).expect("write flow");
    }

    let flows_path = Utf8Path::from_path(config_dir).expect("utf8 path");
    let flows_dir = TedgePaths::from_root_with_defaults(flows_path, "", "").root_dir();
    let registry = ConnectedFlowRegistry::new(HashMap::new(), flows_dir).expect("registry");
    let mut builder = FlowsMapperBuilder::try_new(
        registry,
        FlowsMapperConfig::default().with_js_workers(js_workers),
    )
    .await
    .expect("flows mapper");

    let (requests_tx, mut requests_rx) = mpsc::unbounded::<MqttRequest>();
    let mut mqtt = BenchMqtt {
        input: None,
        requests: requests_tx,
    };
    builder.connect(&mut mqtt);
    let input = mqtt.input.take().expect("flows mapper connected to mqtt");
    let actor = builder.build();
    tokio::spawn(async move { actor.run().await });

    let expected = flows * msgs;
    let started = Instant::now();
    for i in 0..flows {
        let mut input: DynSender<MqttMessage> = input.sender_clone();
        let topic = Topic::new_unchecked(&format!("bench/in/{i}"));
        tokio::spawn(async move {
            for n in 0..msgs {
                let payload = format!(
                    r#"{{"time":"2026-10-28T10:00:00Z","temperature":{},"humidity":{}}}"#,
                    20 + n % 10,
                    40 + n % 20
                );
                input
                    .send(MqttMessage::new(&topic, payload))
                    .await
                    .expect("flows mapper alive");
            }
        });
    }

    let mut received = 0;
    while received < expected {
        match tokio::time::timeout(Duration::from_secs(60), requests_rx.next()).await {
            Ok(Some(MqttRequest::Publish(msg))) if msg.topic.name.starts_with("bench/out/") => {
                received += 1
            }
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => panic!("stalled after {received}/{expected} messages"),
        }
    }
    let elapsed = started.elapsed();

    println!(
        "flows={flows} msgs_per_flow={msgs} work={work} tokio_workers={workers} js_workers={js_workers} \
         flow_workers={flow_workers} mapper={mapper} total={expected} \
         elapsed={:.3}s throughput={:.0} msg/s",
        elapsed.as_secs_f64(),
        expected as f64 / elapsed.as_secs_f64()
    );
}

/// In-memory MQTT peer: hands out the flows mapper input sender and collects published messages.
struct BenchMqtt {
    input: Option<DynSender<MqttMessage>>,
    requests: mpsc::UnboundedSender<MqttRequest>,
}

impl MessageSource<MqttMessage, &mut DynSubscriptions> for BenchMqtt {
    fn connect_sink(
        &mut self,
        config: &mut DynSubscriptions,
        sink: &impl MessageSink<MqttMessage>,
    ) {
        config.set_client_id_usize(0);
        self.input = Some(sink.get_sender());
    }
}

impl MessageSink<MqttRequest> for BenchMqtt {
    fn get_sender(&self) -> DynSender<MqttRequest> {
        Box::new(self.requests.clone())
    }
}
