//! Reproduction harness: one Cumulocity HTTP request that never gets an answer must not block
//! the mapper actor for good.
//!
//! The mapper actor awaits its HTTP requests (events over HTTP, internal id lookups, software
//! lists) inline, and those requests go through the local Cumulocity proxy. If the proxy, or
//! Cumulocity behind it, accepts the connection but never answers, nothing bounds the wait and
//! every message the actor handles (registrations, twin updates, operations) queues behind it.
//! Measurements, events and alarms are converted by the flows mapper, a separate actor, and
//! keep flowing regardless.
//!
//! This test wires the real HTTP actor to a server that accepts connections and never responds.
//! The HTTP actor is given a short request timeout; in production it defaults to
//! [tedge_http_ext::DEFAULT_REQUEST_TIMEOUT].

use crate::tests::c8y_mapper_builder_with_http;
use crate::tests::test_mapper_config_with_auth_proxy_port;
use serde_json::json;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::MessageReceiver;
use tedge_actors::Sender;
use tedge_config::TEdgeConfig;
use tedge_http_ext::HttpActor;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_test_utils::fs::TempTedgeDir;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

/// How long the mapper actor may be held up by one unanswered HTTP request
const MAX_ACTOR_DELAY: Duration = Duration::from_secs(45);

/// An HTTP server that accepts connections, reads the requests and never answers
async fn hanging_http_server() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let requests = Arc::new(AtomicUsize::new(0));
    let counter = requests.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                continue;
            };
            let counter = counter.clone();
            tokio::spawn(async move {
                let mut sink = [0u8; 4096];
                while let Ok(n) = socket.read(&mut sink).await {
                    if n == 0 {
                        break;
                    }
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
    });
    (port, requests)
}

/// Waits for a message on `topic`, returning how long it took, or None past `deadline`
async fn wait_for_topic(
    mqtt: &mut impl MessageReceiver<MqttMessage>,
    topic: &str,
    since: Instant,
    deadline: Instant,
) -> Option<Duration> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let Ok(Some(message)) = tokio::time::timeout(remaining, mqtt.recv()).await else {
            return None;
        };
        if message.topic.name == topic {
            return Some(since.elapsed());
        }
    }
}

#[tokio::test]
async fn mapper_actor_is_not_blocked_for_good_by_a_cumulocity_http_request_that_hangs() {
    let ttd = TempTedgeDir::new();
    let (proxy_port, http_requests) = hanging_http_server().await;
    let config = test_mapper_config_with_auth_proxy_port(&ttd, proxy_port);

    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let tls_config = TEdgeConfig::load_toml_str(&format!("http.ca_path = \"{}\"", ttd.utf8_path()))
        .http
        .client_tls_config()
        .unwrap();
    let mut http_builder = HttpActor::new(tls_config)
        .with_request_timeout(Duration::from_secs(5))
        .builder();
    let builders = c8y_mapper_builder_with_http(&ttd, config, true, &mut http_builder).await;
    let http_actor = http_builder.build();
    tokio::spawn(async move { http_actor.run().await });
    let c8y_actor = builders.c8y.build();
    tokio::spawn(async move { c8y_actor.run().await });
    let flows_actor = builders.flows.build();
    tokio::spawn(async move { flows_actor.run().await });
    let mut mqtt = builders.mqtt.build();

    // An event too large for MQTT is sent to Cumulocity over HTTP, through the proxy that hangs
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("c8y/http/events/create/test-device"),
        json!({
            "type": "large_event",
            "text": "an event forwarded over HTTP",
            "time": "2023-01-25T18:41:14.776170774Z",
        })
        .to_string(),
    ))
    .await
    .unwrap();

    // Right after, a measurement (converted by the flows mapper) and a twin update (converted by
    // the mapper actor, behind the HTTP request)
    let sent_at = Instant::now();
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main///m/"),
        json!({ "temperature": 21.5 }).to_string(),
    ))
    .await
    .unwrap();
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main///twin/agent"),
        json!({ "name": "thin-edge.io", "url": "https://thin-edge.io", "version": "x" })
            .to_string(),
    ))
    .await
    .unwrap();

    let deadline = sent_at + MAX_ACTOR_DELAY;
    let measurement = wait_for_topic(
        &mut mqtt,
        "c8y/measurement/measurements/create",
        sent_at,
        deadline,
    )
    .await;
    eprintln!("measurement converted after {measurement:?} (flows mapper, unaffected)");
    assert!(measurement.is_some(), "measurement was not converted");

    let twin = wait_for_topic(
        &mut mqtt,
        "c8y/inventory/managedObjects/update/test-device",
        sent_at,
        deadline,
    )
    .await;
    let requests = http_requests.load(Ordering::Relaxed);
    eprintln!(
        "twin update converted after {twin:?}; unanswered HTTP requests received: {requests}"
    );
    assert!(requests > 0, "the mapper never sent the HTTP request");
    assert!(
        twin.is_some(),
        "the mapper actor is stuck behind the HTTP request that never gets an answer: \
         twin update not converted within {MAX_ACTOR_DELAY:?}"
    );
}
