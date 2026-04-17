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
            let m: Manifest = toml::from_str(&text)?;
            if m.meta.schema_version != SCHEMA_VERSION {
                anyhow::bail!(
                    "manifest schema version {} not supported (expected {})",
                    m.meta.schema_version,
                    SCHEMA_VERSION
                );
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
