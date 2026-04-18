//! OTLP/gRPC receiver. Issue #34 — closes the gRPC half of FR-P3-001 split
//! out from `serve.rs`'s HTTP-only receiver.
//!
//! Uses `opentelemetry-proto` v0.31's `gen-tonic` to expose
//! `TraceServiceServer` + `LogsServiceServer` on `localhost:4317` (or the
//! endpoint in `.specere/otel-config.yml`'s `receivers.otlp.protocols.grpc`).
//!
//! Each incoming `ExportTraceServiceRequest` → one Event per span appended to
//! the shared SQLite store; same for logs. The state (`Arc<Mutex<Connection>>`)
//! is threaded from `serve_both` so HTTP and gRPC write to the same database.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use opentelemetry_proto::tonic::collector::logs::v1::{
    logs_service_server::{LogsService, LogsServiceServer},
    ExportLogsServiceRequest, ExportLogsServiceResponse,
};
use opentelemetry_proto::tonic::collector::trace::v1::{
    trace_service_server::{TraceService, TraceServiceServer},
    ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use opentelemetry_proto::tonic::common::v1::{any_value::Value as AnyValueKind, KeyValue};
use serde::Deserialize;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};

use crate::event_store::Event;
use crate::sqlite_backend;

/// Shared state across both OTLP receivers — one SQLite connection guarded
/// by a tokio mutex so gRPC + HTTP serialise their writes.
#[derive(Clone)]
pub struct ReceiverState {
    pub repo: PathBuf,
    pub conn: Arc<Mutex<rusqlite::Connection>>,
}

/// gRPC TraceService impl — each span in the batch becomes one Event.
pub(crate) struct TraceSvc {
    pub(crate) state: ReceiverState,
}

#[tonic::async_trait]
impl TraceService for TraceSvc {
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        let payload = request.into_inner();
        let mut appended = 0u64;
        for rs in &payload.resource_spans {
            let resource_attrs: BTreeMap<String, String> = rs
                .resource
                .as_ref()
                .map(|r| flatten_kv(&r.attributes))
                .unwrap_or_default();
            for ss in &rs.scope_spans {
                for span in &ss.spans {
                    let mut attrs = resource_attrs.clone();
                    attrs.extend(flatten_kv(&span.attributes));
                    let ts = unix_nano_to_rfc3339(span.start_time_unix_nano)
                        .unwrap_or_else(crate::event_store::now_rfc3339);
                    let event = Event {
                        ts,
                        source: attrs
                            .get("specere.workflow_step")
                            .cloned()
                            .unwrap_or_else(|| span.name.clone()),
                        signal: "traces".into(),
                        name: Some(span.name.clone()),
                        feature_dir: attrs.get("specere.feature_dir").cloned(),
                        attrs,
                    };
                    persist(&self.state, &event).await.map_err(|e| {
                        Status::internal(format!("persist span: {e}"))
                    })?;
                    appended += 1;
                }
            }
        }
        tracing::debug!(
            "specere grpc: accepted {} span(s) via ExportTraceServiceRequest",
            appended
        );
        // Partial-success with no rejections is the canonical OK.
        Ok(Response::new(ExportTraceServiceResponse {
            partial_success: None,
        }))
    }
}

/// gRPC LogsService impl — each log record becomes one Event.
pub(crate) struct LogsSvc {
    pub(crate) state: ReceiverState,
}

#[tonic::async_trait]
impl LogsService for LogsSvc {
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
        let payload = request.into_inner();
        let mut appended = 0u64;
        for rl in &payload.resource_logs {
            let resource_attrs: BTreeMap<String, String> = rl
                .resource
                .as_ref()
                .map(|r| flatten_kv(&r.attributes))
                .unwrap_or_default();
            for sl in &rl.scope_logs {
                for rec in &sl.log_records {
                    let mut attrs = resource_attrs.clone();
                    attrs.extend(flatten_kv(&rec.attributes));
                    let ts = unix_nano_to_rfc3339(rec.time_unix_nano)
                        .unwrap_or_else(crate::event_store::now_rfc3339);
                    // LogRecord's body is an AnyValue — stringify if present.
                    let body = rec
                        .body
                        .as_ref()
                        .and_then(|b| b.value.as_ref())
                        .and_then(any_value_to_string);
                    let event = Event {
                        ts,
                        source: attrs
                            .get("specere.workflow_step")
                            .cloned()
                            .unwrap_or_else(|| "log".into()),
                        signal: "logs".into(),
                        name: body,
                        feature_dir: attrs.get("specere.feature_dir").cloned(),
                        attrs,
                    };
                    persist(&self.state, &event).await.map_err(|e| {
                        Status::internal(format!("persist log: {e}"))
                    })?;
                    appended += 1;
                }
            }
        }
        tracing::debug!(
            "specere grpc: accepted {} log(s) via ExportLogsServiceRequest",
            appended
        );
        Ok(Response::new(ExportLogsServiceResponse {
            partial_success: None,
        }))
    }
}

async fn persist(state: &ReceiverState, event: &Event) -> anyhow::Result<()> {
    let conn = state.conn.lock().await;
    sqlite_backend::append(&conn, event)?;
    drop(conn);
    crate::event_store::append(&state.repo, event)?;
    Ok(())
}

fn flatten_kv(kvs: &[KeyValue]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for kv in kvs {
        if let Some(v) = kv.value.as_ref().and_then(|v| v.value.as_ref()).and_then(any_value_to_string) {
            out.insert(kv.key.clone(), v);
        }
    }
    out
}

fn any_value_to_string(v: &AnyValueKind) -> Option<String> {
    Some(match v {
        AnyValueKind::StringValue(s) => s.clone(),
        AnyValueKind::IntValue(i) => i.to_string(),
        AnyValueKind::DoubleValue(d) => d.to_string(),
        AnyValueKind::BoolValue(b) => b.to_string(),
        AnyValueKind::BytesValue(_) => return None,
        AnyValueKind::ArrayValue(_) => return None,
        AnyValueKind::KvlistValue(_) => return None,
    })
}

fn unix_nano_to_rfc3339(nanos: u64) -> Option<String> {
    if nanos == 0 {
        return None;
    }
    let secs = (nanos / 1_000_000_000) as i64;
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::from_unix_timestamp(secs)
        .ok()?
        .format(&Rfc3339)
        .ok()
}

/// Parse `otel-config.yml` and extract the gRPC endpoint. Returns None if
/// the config is missing / malformed / has no gRPC section.
pub fn load_grpc_endpoint(path: &Path) -> Option<SocketAddr> {
    let raw = std::fs::read_to_string(path).ok()?;
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
        grpc: Option<Ep>,
    }
    #[derive(Deserialize)]
    struct Ep {
        endpoint: Option<String>,
    }
    let parsed: YamlCfg = serde_yaml::from_str(&raw).ok()?;
    let ep = parsed
        .receivers?
        .otlp?
        .protocols?
        .grpc?
        .endpoint?;
    normalise_endpoint(&ep).parse().ok()
}

/// Default gRPC bind (`127.0.0.1:4317`). Used when the YAML config is absent.
pub fn default_grpc_bind() -> SocketAddr {
    "127.0.0.1:4317".parse().unwrap()
}

fn normalise_endpoint(ep: &str) -> String {
    if let Some(rest) = ep.strip_prefix("localhost:") {
        format!("127.0.0.1:{rest}")
    } else {
        ep.to_string()
    }
}

/// Start the gRPC server and block until `shutdown` fires. Returns the bound
/// local address (useful for tests that bind :0).
pub async fn serve_grpc<F>(
    state: ReceiverState,
    bind: SocketAddr,
    shutdown: F,
) -> anyhow::Result<SocketAddr>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let listener = tokio::net::TcpListener::bind(bind).await?;
    let local = listener.local_addr()?;
    tracing::info!("specere serve: OTLP/gRPC receiver up on {}", local);

    let trace_svc = TraceServiceServer::new(TraceSvc {
        state: state.clone(),
    });
    let logs_svc = LogsServiceServer::new(LogsSvc {
        state: state.clone(),
    });

    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
    tonic::transport::Server::builder()
        .add_service(trace_svc)
        .add_service(logs_svc)
        .serve_with_incoming_shutdown(incoming, shutdown)
        .await?;

    tracing::info!("specere serve: grpc graceful shutdown complete");
    Ok(local)
}
