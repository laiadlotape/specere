//! Append-only JSONL event store backing `specere observe record / query`.
//!
//! Issue #28 / FR-P3-004 partial. One JSON object per line at
//! `.specere/events.jsonl`. `append` uses `OpenOptions::append(true)` for
//! multi-process O_APPEND semantics; concurrent writers interleave at line
//! boundaries, not mid-record.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One recorded event. Mirrors a flat OTLP span / log record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// RFC3339 UTC timestamp (e.g. "2026-04-18T15:23:00Z").
    pub ts: String,
    /// The slash-command verb or CLI source that generated this event.
    pub source: String,
    /// OTLP signal class. Defaults to "traces".
    #[serde(default = "default_signal")]
    pub signal: String,
    /// Human-readable span / record name. Optional — defaults to `source`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Feature dir (SpecKit workflow step association). Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature_dir: Option<String>,
    /// Flat string attributes — typically OTel GenAI semconv (`gen_ai.system`,
    /// `specere.workflow_step`, …) plus any caller additions.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub attrs: std::collections::BTreeMap<String, String>,
}

fn default_signal() -> String {
    "traces".to_string()
}

/// Resolve the default event-store path for a repo (`<repo>/.specere/events.jsonl`).
pub fn default_path(repo: &Path) -> PathBuf {
    repo.join(".specere").join("events.jsonl")
}

/// Append one event as a single JSON line. Creates `.specere/` + the file if
/// absent. Atomic at the line level under O_APPEND.
pub fn append(repo: &Path, event: &Event) -> anyhow::Result<()> {
    let path = default_path(repo);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    let mut line = serde_json::to_string(event)?;
    line.push('\n');
    file.write_all(line.as_bytes())?;
    Ok(())
}

/// Filter set for [`query`]. All filters are AND'd.
#[derive(Debug, Default, Clone)]
pub struct QueryFilters {
    /// Only return events at or after this RFC3339 timestamp.
    pub since: Option<String>,
    /// Only return events with matching `signal`.
    pub signal: Option<String>,
    /// Only return events with matching `source`.
    pub source: Option<String>,
    /// Cap the number of returned events (most recent first). `None` = no cap.
    pub limit: Option<usize>,
}

/// Tail-read the event store, applying [`QueryFilters`]. Returns events in
/// file order (chronological insertion order). An absent store yields an
/// empty vector.
pub fn query(repo: &Path, filters: &QueryFilters) -> anyhow::Result<Vec<Event>> {
    let path = default_path(repo);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = std::fs::File::open(&path)?;
    let reader = BufReader::new(file);
    let mut all: Vec<Event> = Vec::new();
    for line_result in reader.lines() {
        let line = line_result?;
        if line.trim().is_empty() {
            continue;
        }
        let event: Event = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue, // skip malformed lines; the store is append-only so a partial crash can leak a partial line
        };
        if let Some(since) = &filters.since {
            if event.ts.as_str() < since.as_str() {
                continue;
            }
        }
        if let Some(sig) = &filters.signal {
            if event.signal != *sig {
                continue;
            }
        }
        if let Some(src) = &filters.source {
            if event.source != *src {
                continue;
            }
        }
        all.push(event);
    }
    if let Some(n) = filters.limit {
        // Most-recent-N = take from the tail.
        if all.len() > n {
            let skip = all.len() - n;
            all = all.into_iter().skip(skip).collect();
        }
    }
    Ok(all)
}

/// Emit RFC3339-UTC "now" for event-timestamp defaults.
pub fn now_rfc3339() -> String {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_one_event() {
        let tmp = tempfile::TempDir::new().unwrap();
        let e = Event {
            ts: "2026-04-18T15:00:00Z".into(),
            source: "implement".into(),
            signal: "traces".into(),
            name: Some("specere.observe.implement".into()),
            feature_dir: Some("specs/001-foo".into()),
            attrs: [("gen_ai.system".to_string(), "claude-code".to_string())]
                .into_iter()
                .collect(),
        };
        append(tmp.path(), &e).unwrap();
        let got = query(tmp.path(), &QueryFilters::default()).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].source, "implement");
        assert_eq!(
            got[0].attrs.get("gen_ai.system").map(String::as_str),
            Some("claude-code")
        );
    }

    #[test]
    fn query_limit_keeps_most_recent() {
        let tmp = tempfile::TempDir::new().unwrap();
        for i in 0..5 {
            let e = Event {
                ts: format!("2026-04-18T15:00:0{i}Z"),
                source: format!("verb{i}"),
                signal: "traces".into(),
                name: None,
                feature_dir: None,
                attrs: Default::default(),
            };
            append(tmp.path(), &e).unwrap();
        }
        let got = query(
            tmp.path(),
            &QueryFilters {
                limit: Some(3),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].source, "verb2");
        assert_eq!(got[2].source, "verb4");
    }

    #[test]
    fn query_signal_filter() {
        let tmp = tempfile::TempDir::new().unwrap();
        for (i, sig) in ["traces", "logs", "traces"].iter().enumerate() {
            append(
                tmp.path(),
                &Event {
                    ts: format!("2026-04-18T15:0{i}:00Z"),
                    source: "x".into(),
                    signal: sig.to_string(),
                    name: None,
                    feature_dir: None,
                    attrs: Default::default(),
                },
            )
            .unwrap();
        }
        let got = query(
            tmp.path(),
            &QueryFilters {
                signal: Some("logs".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn query_since_filter_excludes_older() {
        let tmp = tempfile::TempDir::new().unwrap();
        append(
            tmp.path(),
            &Event {
                ts: "2026-01-01T00:00:00Z".into(),
                source: "old".into(),
                signal: "traces".into(),
                name: None,
                feature_dir: None,
                attrs: Default::default(),
            },
        )
        .unwrap();
        append(
            tmp.path(),
            &Event {
                ts: "2026-05-01T00:00:00Z".into(),
                source: "new".into(),
                signal: "traces".into(),
                name: None,
                feature_dir: None,
                attrs: Default::default(),
            },
        )
        .unwrap();
        let got = query(
            tmp.path(),
            &QueryFilters {
                since: Some("2026-03-01T00:00:00Z".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].source, "new");
    }

    #[test]
    fn empty_store_returns_no_events() {
        let tmp = tempfile::TempDir::new().unwrap();
        let got = query(tmp.path(), &QueryFilters::default()).unwrap();
        assert_eq!(got.len(), 0);
    }
}
