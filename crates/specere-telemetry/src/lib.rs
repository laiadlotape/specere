//! Telemetry entrypoint — `specere observe record` and `specere observe query`.
//!
//! Phase 3 / issue #28 promoted this from a stub. The event store is a
//! JSONL append-only file at `.specere/events.jsonl` (see [`event_store`]).
//! The SQLite backend (issue #29) and OTLP receivers (issue #30) land later
//! in Phase 3.

use specere_core::Ctx;

pub mod event_store;

pub use event_store::{Event, QueryFilters};

/// Legacy stub. Kept for back-compat until the `Observe` CLI migrates entirely
/// to the record / query subcommands.
pub fn observe(_ctx: &Ctx) -> anyhow::Result<()> {
    tracing::warn!(
        "`specere observe` without a subcommand is deprecated; use `specere observe record` or `specere observe query`"
    );
    Ok(())
}

/// Record one event into the store. Timestamp defaults to `now_rfc3339()` if
/// caller left it blank.
pub fn record(ctx: &Ctx, event: Event) -> anyhow::Result<()> {
    let mut event = event;
    if event.ts.is_empty() {
        event.ts = event_store::now_rfc3339();
    }
    event_store::append(ctx.repo(), &event)
}

/// Query the event store. Returns events in chronological order.
pub fn query(ctx: &Ctx, filters: &QueryFilters) -> anyhow::Result<Vec<Event>> {
    event_store::query(ctx.repo(), filters)
}

/// Output format for `query`.
#[derive(Debug, Clone, Copy)]
pub enum QueryFormat {
    Json,
    Toml,
    Table,
}

/// Render events according to `fmt`. Returns the output string.
pub fn format_events(events: &[Event], fmt: QueryFormat) -> anyhow::Result<String> {
    match fmt {
        QueryFormat::Json => Ok(serde_json::to_string_pretty(events)?),
        QueryFormat::Toml => {
            #[derive(serde::Serialize)]
            struct Wrap<'a> {
                events: &'a [Event],
            }
            Ok(toml::to_string_pretty(&Wrap { events })?)
        }
        QueryFormat::Table => {
            let mut s = String::new();
            s.push_str("ts                          source          signal    name\n");
            s.push_str(
                "--------------------------  --------------  --------  ----------------------------------------\n",
            );
            for e in events {
                s.push_str(&format!(
                    "{:<26}  {:<14}  {:<8}  {}\n",
                    trunc(&e.ts, 26),
                    trunc(&e.source, 14),
                    trunc(&e.signal, 8),
                    e.name.as_deref().unwrap_or("-")
                ));
            }
            Ok(s)
        }
    }
}

fn trunc(s: &str, n: usize) -> &str {
    if s.len() <= n {
        s
    } else {
        &s[..n]
    }
}
