//! Load spec descriptors from `.specere/sensor-map.toml`. Schema extension
//! for issue #43 — the CLI needs to know which specs exist before it can
//! advance their beliefs.
//!
//! Format (added on top of `[coupling]`):
//!
//! ```toml
//! [specs]
//! "FR-001" = { support = ["src/a.rs", "src/b.rs"] }
//! "FR-002" = { support = ["src/b.rs"] }
//! ```
//!
//! Unknown keys are ignored so this extends gracefully with future fields
//! (prior overrides, coupling cluster membership, etc.).

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use crate::hmm::SpecDescriptor;

/// Parse the `[specs]` table from a sensor-map.toml file. Returns the specs
/// sorted by ID for deterministic iteration order (posterior TOML needs this
/// for FR-P4-004).
pub fn load_specs(path: &Path) -> Result<Vec<SpecDescriptor>> {
    if !path.exists() {
        return Err(anyhow!(
            "sensor-map not found at {} — run `specere init` or add a [specs] section per docs/filter.md",
            path.display()
        ));
    }
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    load_specs_from_str(&raw)
}

pub fn load_specs_from_str(raw: &str) -> Result<Vec<SpecDescriptor>> {
    #[derive(Deserialize)]
    struct Root {
        specs: Option<std::collections::BTreeMap<String, SpecEntry>>,
    }
    #[derive(Deserialize)]
    struct SpecEntry {
        support: Option<Vec<String>>,
    }
    let parsed: Root = toml::from_str(raw).context("parse sensor-map.toml")?;
    let specs = parsed.specs.unwrap_or_default();
    if specs.is_empty() {
        return Err(anyhow!(
            "[specs] section empty or missing in sensor-map.toml — \
             add entries like `\"FR-001\" = {{ support = [\"src/a.rs\"] }}`"
        ));
    }
    // BTreeMap already yields sorted keys. Preserve that order.
    let mut out: Vec<SpecDescriptor> = Vec::with_capacity(specs.len());
    for (id, entry) in specs {
        out.push(SpecDescriptor {
            id,
            support: entry.support.unwrap_or_default(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_specs_section() {
        let specs = load_specs_from_str(
            r#"
            [specs]
            "FR-002" = { support = ["src/b.rs"] }
            "FR-001" = { support = ["src/a.rs", "src/b.rs"] }
            "#,
        )
        .unwrap();
        // BTreeMap ordering — FR-001 before FR-002.
        assert_eq!(specs[0].id, "FR-001");
        assert_eq!(specs[1].id, "FR-002");
        assert_eq!(specs[0].support, vec!["src/a.rs", "src/b.rs"]);
    }

    #[test]
    fn rejects_missing_specs_section() {
        let err = load_specs_from_str(
            r#"
            schema_version = 1
            [channels]
            "#,
        )
        .unwrap_err();
        assert!(format!("{err}").contains("[specs]"));
    }

    #[test]
    fn rejects_empty_specs_section() {
        let err = load_specs_from_str(
            r#"
            [specs]
            "#,
        )
        .unwrap_err();
        assert!(format!("{err}").contains("empty or missing"));
    }

    #[test]
    fn allows_missing_support_list() {
        let specs = load_specs_from_str(
            r#"
            [specs]
            "FR-001" = {}
            "#,
        )
        .unwrap();
        assert_eq!(specs.len(), 1);
        assert!(specs[0].support.is_empty());
    }
}
