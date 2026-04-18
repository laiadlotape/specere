//! Issue #34 — `specere serve` OTLP/gRPC receiver. Spawns `serve_grpc` on
//! an ephemeral port, connects an in-process tonic `TraceServiceClient`,
//! sends a synthetic `ExportTraceServiceRequest`, asserts the event lands
//! in the SQLite store.

use std::path::PathBuf;
use std::sync::Arc;

use opentelemetry_proto::tonic::collector::trace::v1::{
    trace_service_client::TraceServiceClient, ExportTraceServiceRequest,
};
use opentelemetry_proto::tonic::common::v1::{
    any_value::Value as AnyValueKind, AnyValue, KeyValue,
};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, ScopeSpans, Span};
use tempfile::TempDir;
use tokio::sync::Notify;

fn kv(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.into(),
        value: Some(AnyValue {
            value: Some(AnyValueKind::StringValue(value.into())),
        }),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn grpc_export_trace_persists_to_sqlite() {
    let tmp = TempDir::new().unwrap();
    let repo: PathBuf = tmp.path().into();
    std::fs::create_dir_all(repo.join(".specere")).unwrap();

    // Probe-and-release the ephemeral port — same pattern as fr_p3_003.
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);

    let conn = specere_telemetry::sqlite_backend::open(&repo).unwrap();
    let state = specere_telemetry::grpc::ReceiverState {
        repo: repo.clone(),
        conn: Arc::new(tokio::sync::Mutex::new(conn)),
    };

    let shutdown = Arc::new(Notify::new());
    let s_clone = shutdown.clone();
    let bind = format!("127.0.0.1:{port}").parse().unwrap();
    let server_task = tokio::spawn(async move {
        specere_telemetry::serve_grpc(state, bind, async move {
            s_clone.notified().await;
        })
        .await
    });

    // Wait briefly for the rebind window to close.
    let endpoint = format!("http://127.0.0.1:{port}");
    let mut client = None;
    for _ in 0..40 {
        if let Ok(c) = TraceServiceClient::connect(endpoint.clone()).await {
            client = Some(c);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let mut client = client.expect("tonic client failed to connect within 1s");

    let request = ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![kv("gen_ai.system", "claude-code")],
                dropped_attributes_count: 0,
                entity_refs: vec![],
            }),
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans: vec![Span {
                    trace_id: vec![1u8; 16],
                    span_id: vec![2u8; 8],
                    trace_state: String::new(),
                    parent_span_id: vec![],
                    flags: 0,
                    name: "specere.observe.implement".into(),
                    kind: 1,
                    start_time_unix_nano: 1_752_800_000_000_000_000,
                    end_time_unix_nano: 1_752_800_001_000_000_000,
                    attributes: vec![
                        kv("specere.workflow_step", "implement"),
                        kv("specere.feature_dir", "specs/021-otlp-grpc-receiver"),
                    ],
                    dropped_attributes_count: 0,
                    events: vec![],
                    dropped_events_count: 0,
                    links: vec![],
                    dropped_links_count: 0,
                    status: None,
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    };

    let resp = client
        .export(tonic::Request::new(request))
        .await
        .expect("gRPC export failed");
    assert!(
        resp.into_inner().partial_success.is_none(),
        "expected full success"
    );

    // Give the handler a moment to flush to SQLite + JSONL.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let conn = specere_telemetry::sqlite_backend::open(&repo).unwrap();
    let events = specere_telemetry::sqlite_backend::query(
        &conn,
        &specere_telemetry::QueryFilters::default(),
    )
    .unwrap();
    assert_eq!(events.len(), 1, "expected one span persisted via gRPC");
    assert_eq!(events[0].source, "implement");
    assert_eq!(events[0].signal, "traces");
    assert_eq!(
        events[0].attrs.get("gen_ai.system").map(String::as_str),
        Some("claude-code")
    );
    assert_eq!(
        events[0].feature_dir.as_deref(),
        Some("specs/021-otlp-grpc-receiver")
    );

    // JSONL mirror grew by one record.
    let jsonl = std::fs::read_to_string(repo.join(".specere/events.jsonl")).unwrap();
    assert_eq!(jsonl.lines().count(), 1);

    shutdown.notify_one();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), server_task).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn grpc_shutdown_is_graceful() {
    let tmp = TempDir::new().unwrap();
    let repo: PathBuf = tmp.path().into();
    std::fs::create_dir_all(repo.join(".specere")).unwrap();

    let conn = specere_telemetry::sqlite_backend::open(&repo).unwrap();
    let state = specere_telemetry::grpc::ReceiverState {
        repo: repo.clone(),
        conn: Arc::new(tokio::sync::Mutex::new(conn)),
    };

    let shutdown = Arc::new(Notify::new());
    let s_clone = shutdown.clone();
    let bind = "127.0.0.1:0".parse().unwrap();
    let task = tokio::spawn(async move {
        specere_telemetry::serve_grpc(state, bind, async move {
            s_clone.notified().await;
        })
        .await
    });

    shutdown.notify_one();
    let res = tokio::time::timeout(std::time::Duration::from_secs(5), task).await;
    assert!(
        res.is_ok(),
        "serve_grpc did not shut down within 5s of shutdown signal"
    );
    assert!(res.unwrap().is_ok(), "server task panicked");
}
