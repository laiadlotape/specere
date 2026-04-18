//! Issue #30 — `specere serve` OTLP/HTTP receiver. Spawns the receiver on
//! an ephemeral port in a tokio task, POSTs a synthetic OTLP/HTTP/JSON
//! traces payload, asserts the event lands in the SQLite store.
//!
//! gRPC half is deferred to issue #34.

use std::path::PathBuf;
use std::sync::Arc;

use tempfile::TempDir;
use tokio::sync::Notify;

#[tokio::test(flavor = "multi_thread")]
async fn serve_receives_otlp_traces_on_ephemeral_port() {
    let tmp = TempDir::new().unwrap();
    let repo: PathBuf = tmp.path().into();
    std::fs::create_dir_all(repo.join(".specere")).unwrap();

    // Bind to :0 so the OS picks an open port — avoids cross-job collisions.
    let cfg = specere_telemetry::ServeConfig {
        http_bind: "127.0.0.1:0".parse().unwrap(),
    };

    let shutdown = Arc::new(Notify::new());
    let shutdown_handle = shutdown.clone();

    // Channel the bound port out of the server task via oneshot.
    let (port_tx, port_rx) = tokio::sync::oneshot::channel::<u16>();
    let repo_for_server = repo.clone();

    let server_task = tokio::spawn(async move {
        // We can't directly read the port from the existing serve_http signature
        // because it returns after shutdown. Instead, bind manually, grab the
        // port, then hand off to serve_http via a pre-bound listener.
        // TODO(#30-follow-up): expose bound-addr callback. For now we bind a
        // quick probe, read the port, release it, and pass to serve_http which
        // will rebind. The brief release window is safe inside a single-test
        // tokio runtime.
        let probe = tokio::net::TcpListener::bind(cfg.http_bind).await.unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let _ = port_tx.send(port);

        let real_cfg = specere_telemetry::ServeConfig {
            http_bind: format!("127.0.0.1:{port}").parse().unwrap(),
        };
        let wait = shutdown_handle.clone();
        specere_telemetry::serve_http(repo_for_server, real_cfg, async move {
            wait.notified().await;
        })
        .await
    });

    let port = port_rx.await.unwrap();

    // Retry briefly — the re-bind window may be < 10ms.
    for _ in 0..20 {
        let probe = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/healthz"))
            .send()
            .await;
        if probe.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    // Post a minimal OTLP/HTTP/JSON TracesData payload.
    let payload = serde_json::json!({
        "resourceSpans": [{
            "resource": {
                "attributes": [
                    {"key": "gen_ai.system", "value": {"stringValue": "claude-code"}}
                ]
            },
            "scopeSpans": [{
                "spans": [{
                    "name": "specere.observe.implement",
                    "startTimeUnixNano": "1752800000000000000",
                    "attributes": [
                        {"key": "specere.workflow_step", "value": {"stringValue": "implement"}},
                        {"key": "specere.feature_dir", "value": {"stringValue": "specs/002-phase-1-bugfix-0-2-0"}}
                    ]
                }]
            }]
        }]
    });
    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/v1/traces"))
        .json(&payload)
        .send()
        .await
        .expect("send OTLP traces");
    assert!(
        resp.status().is_success(),
        "receiver did not accept traces: status {}",
        resp.status()
    );

    // Give the handler a moment to persist.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Verify via the SQLite backend that exactly one event landed.
    let conn = specere_telemetry::sqlite_backend::open(&repo).unwrap();
    let events = specere_telemetry::sqlite_backend::query(
        &conn,
        &specere_telemetry::QueryFilters::default(),
    )
    .unwrap();
    assert_eq!(events.len(), 1, "expected one event persisted");
    assert_eq!(events[0].source, "implement");
    assert_eq!(
        events[0].attrs.get("gen_ai.system").map(String::as_str),
        Some("claude-code")
    );
    assert_eq!(
        events[0].feature_dir.as_deref(),
        Some("specs/002-phase-1-bugfix-0-2-0")
    );

    // Verify JSONL mirror also grew.
    let jsonl = std::fs::read_to_string(repo.join(".specere/events.jsonl")).unwrap();
    assert_eq!(jsonl.lines().count(), 1);

    shutdown.notify_one();
    let _ = server_task.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn serve_shutdown_is_graceful() {
    let tmp = TempDir::new().unwrap();
    let repo: PathBuf = tmp.path().into();
    std::fs::create_dir_all(repo.join(".specere")).unwrap();

    let cfg = specere_telemetry::ServeConfig {
        http_bind: "127.0.0.1:0".parse().unwrap(),
    };
    let shutdown = Arc::new(Notify::new());
    let s_clone = shutdown.clone();
    let repo_c = repo.clone();
    let task = tokio::spawn(async move {
        specere_telemetry::serve_http(repo_c, cfg, async move {
            s_clone.notified().await;
        })
        .await
    });

    // Trigger shutdown immediately; the server should exit Ok within a timeout.
    shutdown.notify_one();
    let res = tokio::time::timeout(std::time::Duration::from_secs(5), task).await;
    assert!(
        res.is_ok(),
        "serve_http did not shut down within 5s of shutdown signal"
    );
    assert!(res.unwrap().is_ok(), "server task panicked");
}
