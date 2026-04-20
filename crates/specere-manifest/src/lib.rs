//! The SpecERE manifest lives at `.specere/manifest.toml`. It is the source of
//! truth for "what SpecERE installed into this repo." Every `add` writes an
//! entry; every `remove` consults one. Drift is detected by re-hashing the
//! paths and comparing to `sha256_post`.

use std::fs;
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specere_core::{FileEntry, MarkerEntry, Record};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub meta: Meta,
    #[serde(default, rename = "units")]
    pub units: Vec<UnitEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Meta {
    pub specere_version: String,
    pub schema_version: u32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitEntry {
    pub id: String,
    pub version: String,
    pub installed_at: String,
    #[serde(default)]
    pub install_config: toml::Table,
    #[serde(default)]
    pub files: Vec<FileEntry>,
    #[serde(default)]
    pub markers: Vec<MarkerEntry>,
    #[serde(default)]
    pub dirs: Vec<std::path::PathBuf>,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl Manifest {
    pub fn new(specere_version: impl Into<String>) -> Self {
        Self {
            meta: Meta {
                specere_version: specere_version.into(),
                schema_version: SCHEMA_VERSION,
                created_at: now_rfc3339(),
            },
            units: Vec::new(),
        }
    }

    pub fn load_or_init(path: &Path, specere_version: &str) -> anyhow::Result<Self> {
        if path.exists() {
            let text = fs::read_to_string(path)?;
            let mut m: Manifest = toml::from_str(&text)?;
            if m.meta.schema_version != SCHEMA_VERSION {
                anyhow::bail!(
                    "manifest schema version {} not supported (expected {})",
                    m.meta.schema_version,
                    SCHEMA_VERSION
                );
            }
            // Backwards-compat: pre-v1.0 manifests did not emit `unit_id`
            // on MarkerEntry. We deserialise with `#[serde(default)]` (so
            // the field lands as empty string) and backfill here from the
            // containing unit's id. See `docs/upcoming.md` §5.
            for unit in &mut m.units {
                for marker in &mut unit.markers {
                    if marker.unit_id.is_empty() {
                        marker.unit_id = unit.id.clone();
                    }
                }
            }
            Ok(m)
        } else {
            Ok(Self::new(specere_version))
        }
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        fs::write(path, text)?;
        Ok(())
    }

    pub fn get(&self, unit_id: &str) -> Option<&UnitEntry> {
        self.units.iter().find(|u| u.id == unit_id)
    }

    pub fn upsert(&mut self, entry: UnitEntry) {
        if let Some(pos) = self.units.iter().position(|u| u.id == entry.id) {
            self.units[pos] = entry;
        } else {
            self.units.push(entry);
        }
    }

    pub fn remove(&mut self, unit_id: &str) -> Option<UnitEntry> {
        let pos = self.units.iter().position(|u| u.id == unit_id)?;
        Some(self.units.remove(pos))
    }
}

pub fn record_to_unit_entry(
    unit_id: impl Into<String>,
    version: impl Into<String>,
    install_config: toml::Table,
    record: Record,
) -> UnitEntry {
    UnitEntry {
        id: unit_id.into(),
        version: version.into(),
        installed_at: now_rfc3339(),
        install_config,
        files: record.files,
        markers: record.markers,
        dirs: record.dirs,
        notes: record.notes,
    }
}

pub fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn now_rfc3339() -> String {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Pre-v1.0 manifest shape: no `unit_id` on `MarkerEntry`. The loader
    /// must accept the old schema without erroring and backfill each
    /// marker's unit_id from the containing unit's id.
    #[test]
    fn load_backfills_marker_unit_id_from_parent_unit() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("manifest.toml");
        fs::write(
            &path,
            r#"[meta]
specere_version = "0.1.0"
schema_version = 1
created_at = "2026-04-18T00:00:00Z"

[[units]]
id = "speckit"
version = "1.0"
installed_at = "2026-04-18T00:00:00Z"

[[units.markers]]
path = ".gitignore"
block_id = "speckit-block"
sha256 = "deadbeef"
"#,
        )
        .unwrap();
        let m = Manifest::load_or_init(&path, "1.2.0").expect("old-schema manifest loads");
        assert_eq!(m.units.len(), 1);
        let u = &m.units[0];
        assert_eq!(u.id, "speckit");
        assert_eq!(u.markers.len(), 1);
        // unit_id must be backfilled from parent `id`, not left empty.
        assert_eq!(u.markers[0].unit_id, "speckit");
    }

    /// Round-trip: a manifest written by this version MUST deserialise
    /// without the backfill being needed (so newer markers carry their
    /// own unit_id on disk). Keeps the backfill a no-op on healthy data.
    #[test]
    fn round_trip_preserves_explicit_unit_id() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("manifest.toml");
        let mut m = Manifest::new("1.2.0");
        m.units.push(UnitEntry {
            id: "filter-state".into(),
            version: "1.0".into(),
            installed_at: "2026-04-18T00:00:00Z".into(),
            install_config: toml::Table::new(),
            files: Vec::new(),
            markers: vec![specere_core::MarkerEntry {
                path: ".gitignore".into(),
                unit_id: "filter-state".into(),
                block_id: Some("filter-state-block".into()),
                sha256: "abc123".into(),
            }],
            dirs: Vec::new(),
            notes: Vec::new(),
        });
        m.save(&path).unwrap();
        let loaded = Manifest::load_or_init(&path, "1.2.0").unwrap();
        assert_eq!(loaded.units[0].markers[0].unit_id, "filter-state");
    }
}
