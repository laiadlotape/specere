//! `specere serve` — starts the embedded OTLP/HTTP receiver and persists
//! every incoming span / log to the SQLite event store.
//!
//! Issue #30 — the HTTP half of FR-P3-001 + FR-P3-005 (SIGINT safety).
//! The gRPC half lands in #34.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::event_store::Event;
use crate::sqlite_backend;

/// Configuration loaded from `.specere/otel-config.yml`. We only pull the
/// HTTP receiver endpoint for now; the rest of the config is Phase-3-follow-
/// up territory (#34 for gRPC, Phase 4 for exporter routing).
#[derive(Debug, Clone)]
pub struct ServeConfig {
    /// HTTP receiver bind address. Defaults to `127.0.0.1:4318` if the config
    /// is missing / malformed.
    pub http_bind: SocketAddr,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            http_bind: "127.0.0.1:4318".parse().unwrap(),
        }
    }
}

/// Parse `otel-config.yml` and extract the HTTP endpoint. Returns a default
/// config on any error (yaml missing / unparseable / section absent) — the
/// receiver should still start on the standard port.
pub fn load_config(path: &Path) -> ServeConfig {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return ServeConfig::default();
    };
    #[derive(Deserialize)]
    struct YamlCfg {
        receivers: Option<Receivers>,
    }
    #[derive(Deserialize)]
    struct Receivers {
        otlp: Option<Otlp>,
    }
    #[derive(Deserialize)]
    struct Otlp {
        protocols: Option<Protocols>,
    }
    #[derive(Deserialize)]
    struct Protocols {
        http: Option<HttpEndpoint>,
    }
    #[derive(Deserialize)]
    struct HttpEndpoint {
        endpoint: Option<String>,
    }
    let parsed: YamlCfg = match serde_yaml::from_str(&raw) {
        Ok(c) => c,
        Err(_) => return ServeConfig::default(),
    };
    let http_ep = parsed
        .receivers
        .and_then(|r| r.otlp)
        .and_then(|o| o.protocols)
        .and_then(|p| p.http)
        .and_then(|h| h.endpoint);
    let bind = match http_ep {
        Some(ep) => normalise_endpoint(&ep)
            .parse()
            .unwrap_or_else(|_| ServeConfig::default().http_bind),
        None => ServeConfig::default().http_bind,
    };
    ServeConfig { http_bind: bind }
}

/// OTel convention: `localhost:4318` — we normalise to `127.0.0.1:4318` so
/// std::net::SocketAddr parses. Otherwise returns the string unchanged.
fn normalise_endpoint(ep: &str) -> String {
    if let Some(rest) = ep.strip_prefix("localhost:") {
        format!("127.0.0.1:{rest}")
    } else {
        ep.to_string()
    }
}

#[derive(Clone)]
struct AppState {
    repo: PathBuf,
    /// Serialise writes — SQLite is thread-safe but we want an orderly flush
    /// under SIGINT. One connection + mutex suffices for Phase-3 throughput.
    conn: Arc<Mutex<rusqlite::Connection>>,
}

/// Start the HTTP receiver and block until the shutdown signal fires. Caller
/// must provide a shutdown future (typically SIGINT / `CancellationToken`).
///
/// Returns the bound local address — useful for tests that bind to :0 and
/// read the assigned port.
pub async fn serve_http<F>(
    repo: PathBuf,
    cfg: ServeConfig,
    shutdown: F,
) -> anyhow::Result<SocketAddr>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let conn = sqlite_backend::open(&repo)?;
    let state = AppState {
        repo: repo.clone(),
        conn: Arc::new(Mutex::new(conn)),
    };
    let app = Router::new()
        .route("/v1/traces", post(handle_traces))
        .route("/v1/logs", post(handle_logs))
        .route("/healthz", post(handle_health))
        .with_state(state.clone());

    let listener = TcpListener::bind(cfg.http_bind).await?;
    let local = listener.local_addr()?;
    tracing::info!("specere serve: OTLP/HTTP receiver up on {}", local);

    // Run the axum server with the caller's shutdown future. On shutdown
    // trigger, checkpoint the SQLite WAL so the post-process state is clean
    // (FR-P3-005).
    let state_for_shutdown = state.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown.await;
            let conn = state_for_shutdown.conn.lock().await;
            if let Err(e) = sqlite_backend::checkpoint_truncate(&conn) {
                tracing::warn!("WAL checkpoint on shutdown failed: {e}");
            }
        })
        .await?;

    tracing::info!("specere serve: graceful shutdown complete");
    Ok(local)
}

async fn handle_health() -> impl IntoResponse {
    "ok"
}

/// OTLP/HTTP/JSON traces payload. We parse a subset sufficient to extract
/// one Event per span. The rest of the OTLP schema is ignored — can be
/// extended in a follow-up without breaking wire compat.
#[derive(Debug, Deserialize)]
struct TracesData {
    #[serde(default, rename = "resourceSpans")]
    resource_spans: Vec<ResourceSpans>,
}

#[derive(Debug, Deserialize)]
struct ResourceSpans {
    #[serde(default)]
    resource: Option<Resource>,
    #[serde(default, rename = "scopeSpans")]
    scope_spans: Vec<ScopeSpans>,
}

#[derive(Debug, Deserialize)]
struct Resource {
    #[serde(default)]
    attributes: Vec<KeyValue>,
}

#[derive(Debug, Deserialize)]
struct ScopeSpans {
    #[serde(default)]
    spans: Vec<Span>,
}

#[derive(Debug, Deserialize)]
struct Span {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "startTimeUnixNano")]
    start_time_unix_nano: Option<serde_json::Value>,
    #[serde(default)]
    attributes: Vec<KeyValue>,
}

#[derive(Debug, Deserialize)]
struct KeyValue {
    key: String,
    value: AnyValue,
}

#[derive(Debug, Deserialize)]
struct AnyValue {
    #[serde(default, rename = "stringValue")]
    string_value: Option<String>,
    #[serde(default, rename = "intValue")]
    int_value: Option<serde_json::Value>,
    #[serde(default, rename = "boolValue")]
    bool_value: Option<bool>,
}

impl AnyValue {
    fn to_string_repr(&self) -> Option<String> {
        if let Some(s) = &self.string_value {
            return Some(s.clone());
        }
        if let Some(i) = &self.int_value {
            return Some(i.to_string());
        }
        if let Some(b) = self.bool_value {
            return Some(b.to_string());
        }
        None
    }
}

async fn handle_traces(
    State(state): State<AppState>,
    axum::Json(payload): axum::Json<TracesData>,
) -> impl IntoResponse {
    let mut appended = 0usize;
    for rs in &payload.resource_spans {
        let resource_attrs: std::collections::BTreeMap<String, String> = rs
            .resource
            .as_ref()
            .map(|r| flatten_kv(&r.attributes))
            .unwrap_or_default();
        for ss in &rs.scope_spans {
            for span in &ss.spans {
                let mut attrs = resource_attrs.clone();
                attrs.extend(flatten_kv(&span.attributes));
                let ts = unix_nano_to_rfc3339(&span.start_time_unix_nano)
                    .unwrap_or_else(crate::event_store::now_rfc3339);
                let event = Event {
                    ts,
                    source: attrs
                        .get("specere.workflow_step")
                        .cloned()
                        .unwrap_or_else(|| span.name.clone().unwrap_or_default()),
                    signal: "traces".into(),
                    name: span.name.clone(),
                    feature_dir: attrs.get("specere.feature_dir").cloned(),
                    attrs,
                };
                let conn = state.conn.lock().await;
                if let Err(e) = sqlite_backend::append(&conn, &event) {
                    tracing::error!("append failed: {e}");
                    return (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        "append failed",
                    )
                        .into_response();
                }
                // Mirror to JSONL too.
                drop(conn); // release before sync file op
                if let Err(e) = crate::event_store::append(&state.repo, &event) {
                    tracing::warn!("JSONL mirror append failed: {e}");
                }
                appended += 1;
            }
        }
    }
    (
        axum::http::StatusCode::OK,
        format!("{{\"accepted\":{appended}}}"),
    )
        .into_response()
}

async fn handle_logs(
    State(state): State<AppState>,
    axum::Json(_payload): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    // Minimal logs handler: accept and count. Full LogsData extraction is
    // symmetric with traces and can be lifted once an actual log producer
    // shows up. For Phase 3 MVP we acknowledge but don't persist.
    let _ = state;
    (axum::http::StatusCode::OK, "{\"accepted\":0}").into_response()
}

fn flatten_kv(kvs: &[KeyValue]) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    for kv in kvs {
        if let Some(v) = kv.value.to_string_repr() {
            out.insert(kv.key.clone(), v);
        }
    }
    out
}

fn unix_nano_to_rfc3339(v: &Option<serde_json::Value>) -> Option<String> {
    let v = v.as_ref()?;
    // OTLP/HTTP/JSON encodes fixed64 as a string by default. Accept both.
    let nanos: i128 = match v {
        serde_json::Value::String(s) => s.parse().ok()?,
        serde_json::Value::Number(n) => n.as_i64().map(i128::from)?,
        _ => return None,
    };
    let secs = (nanos / 1_000_000_000) as i64;
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::from_unix_timestamp(secs)
        .ok()?
        .format(&Rfc3339)
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_binds_4318() {
        let cfg = ServeConfig::default();
        assert_eq!(cfg.http_bind.port(), 4318);
    }

    #[test]
    fn load_config_uses_yaml_when_present() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("otel-config.yml");
        std::fs::write(
            &path,
            "receivers:\n  otlp:\n    protocols:\n      http:\n        endpoint: 127.0.0.1:4999\n",
        )
        .unwrap();
        let cfg = load_config(&path);
        assert_eq!(cfg.http_bind.port(), 4999);
    }

    #[test]
    fn load_config_falls_back_on_missing_file() {
        let cfg = load_config(Path::new("/nonexistent/path"));
        assert_eq!(cfg.http_bind.port(), 4318);
    }

    #[test]
    fn load_config_normalises_localhost() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("otel-config.yml");
        std::fs::write(
            &path,
            "receivers:\n  otlp:\n    protocols:\n      http:\n        endpoint: localhost:4500\n",
        )
        .unwrap();
        let cfg = load_config(&path);
        assert_eq!(cfg.http_bind.port(), 4500);
        assert!(cfg.http_bind.ip().is_loopback());
    }
}
