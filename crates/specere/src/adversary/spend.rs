//! FR-EQ-021 — hard $20/month ceiling with deferral.
//!
//! The ledger at `.specere/adversary-budget.toml` tracks `month` +
//! `spent_usd` + `cap_usd`. On month rollover, a fresh row is written.
//! `allow(cost)` returns `Err(SpendError::CapExceeded)` when the next
//! debit would cross the cap — callers surface this to the user and stop
//! the loop rather than hit the provider.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const DEFAULT_CAP_USD: f64 = 20.0;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Ledger {
    pub month: String,
    #[serde(default)]
    pub spent_usd: f64,
    #[serde(default = "default_cap")]
    pub cap_usd: f64,
}

fn default_cap() -> f64 {
    DEFAULT_CAP_USD
}

#[derive(Debug)]
pub enum SpendError {
    CapExceeded { spent: f64, cap: f64, wants: f64 },
}

impl std::fmt::Display for SpendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpendError::CapExceeded { spent, cap, wants } => write!(
                f,
                "adversary monthly cap exceeded: spent ${spent:.2}, cap ${cap:.2}, \
                 next debit ${wants:.2}"
            ),
        }
    }
}

impl std::error::Error for SpendError {}

pub fn ledger_path(repo: &Path) -> PathBuf {
    repo.join(".specere/adversary-budget.toml")
}

pub fn current_month_utc() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days_since_epoch = now / 86400;
    let (mut year, mut month) = (1970_i64, 1_u32);
    let mut remaining = days_since_epoch as i64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }
    for m in 1..=12u32 {
        let dim = days_in_month(year, m);
        if remaining < dim as i64 {
            month = m;
            break;
        }
        remaining -= dim as i64;
    }
    format!("{year:04}-{month:02}")
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => unreachable!(),
    }
}

pub fn load_or_init(path: &Path, cap_usd: Option<f64>) -> Result<Ledger> {
    let month = current_month_utc();
    if !path.exists() {
        return Ok(Ledger {
            month,
            spent_usd: 0.0,
            cap_usd: cap_usd.unwrap_or(DEFAULT_CAP_USD),
        });
    }
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut led: Ledger =
        toml::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    if led.month != month {
        led.month = month;
        led.spent_usd = 0.0;
    }
    if let Some(c) = cap_usd {
        led.cap_usd = c;
    }
    if led.cap_usd == 0.0 {
        led.cap_usd = DEFAULT_CAP_USD;
    }
    Ok(led)
}

pub fn save(path: &Path, led: &Ledger) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("mkdir -p {}", parent.display()))?;
    }
    let body = toml::to_string_pretty(led).context("serialize ledger")?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, body).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

/// Reserve `cost_usd`. Returns `CapExceeded` if the new total would cross
/// the cap. Ledger is updated + written on success.
pub fn charge(path: &Path, led: &mut Ledger, cost_usd: f64) -> Result<()> {
    let new_total = led.spent_usd + cost_usd;
    if new_total > led.cap_usd + 1e-9 {
        return Err(SpendError::CapExceeded {
            spent: led.spent_usd,
            cap: led.cap_usd,
            wants: cost_usd,
        }
        .into());
    }
    led.spent_usd = new_total;
    save(path, led)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn new_ledger_defaults_to_20_cap() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("b.toml");
        let led = load_or_init(&p, None).unwrap();
        assert_eq!(led.cap_usd, 20.0);
        assert_eq!(led.spent_usd, 0.0);
    }

    #[test]
    fn charge_within_cap_updates_ledger() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("b.toml");
        let mut led = load_or_init(&p, None).unwrap();
        charge(&p, &mut led, 3.5).unwrap();
        let reloaded = load_or_init(&p, None).unwrap();
        assert!((reloaded.spent_usd - 3.5).abs() < 1e-9);
    }

    #[test]
    fn charge_past_cap_returns_cap_exceeded() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("b.toml");
        let mut led = load_or_init(&p, Some(5.0)).unwrap();
        charge(&p, &mut led, 4.0).unwrap();
        let err = charge(&p, &mut led, 2.0).unwrap_err();
        let root = err.root_cause();
        assert!(
            root.to_string().contains("cap exceeded"),
            "expected cap error, got: {err}"
        );
    }

    #[test]
    fn month_rollover_resets_spent() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("b.toml");
        let stale = Ledger {
            month: "2020-01".into(),
            spent_usd: 15.0,
            cap_usd: 20.0,
        };
        save(&p, &stale).unwrap();
        let fresh = load_or_init(&p, None).unwrap();
        assert_eq!(fresh.spent_usd, 0.0);
        assert_ne!(fresh.month, "2020-01");
    }
}
