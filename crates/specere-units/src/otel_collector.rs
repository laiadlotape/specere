//! `specere add otel-collector` — native unit that writes the SpecERE-tuned
//! OTLP collector config and (opt-in via `--service`) a platform-specific
//! service file. Does NOT start a receiver — that's Phase 3's `specere serve`.
//!
//! Issue #13 / FR-P2-002.

use std::path::PathBuf;

use specere_core::{AddUnit, Ctx, FileEntry, Owner, Plan, PlanOp, Record, Result};

const UNIT_ID: &str = "otel-collector";

/// Minimal OTLP receiver config tuned for `gen_ai.*` semconv. Values chosen
/// to match `specere serve`'s defaults in Phase 3 (localhost binding,
/// file-exporter sink into `.specere/events.jsonl`). Embedded here so the
/// scaffold is self-contained; Phase-3's receiver reads it at startup.
const OTEL_CONFIG_YML: &str = r#"# SpecERE OTLP collector config. Consumed by `specere serve` in Phase 3.
# Tuned for OpenTelemetry gen_ai.* semantic conventions — every span / log
# emitted by a Claude Code session is routed through this pipeline.

receivers:
  otlp:
    protocols:
      grpc:
        endpoint: localhost:4317
      http:
        endpoint: localhost:4318

processors:
  batch:
    timeout: 1s
    send_batch_size: 1024

exporters:
  file:
    path: .specere/events.jsonl
    rotation:
      max_megabytes: 50
      max_days: 7

service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [batch]
      exporters: [file]
    logs:
      receivers: [otlp]
      processors: [batch]
      exporters: [file]
"#;

const LINUX_SYSTEMD_UNIT: &str = r#"# SpecERE OTLP receiver — systemd user unit.
# Install:  cp .specere/services/specere-serve.service ~/.config/systemd/user/
# Enable:   systemctl --user daemon-reload && systemctl --user enable --now specere-serve
# (Phase 3's `specere serve` binary implements the ExecStart target.)

[Unit]
Description=SpecERE OTLP receiver (gen_ai.* telemetry sink)
After=network.target

[Service]
Type=simple
ExecStart=/usr/bin/env specere serve --config .specere/otel-config.yml
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
"#;

const MACOS_LAUNCHD_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- SpecERE OTLP receiver - launchd user agent.
     Install: cp .specere/services/dev.specere.serve.plist ~/Library/LaunchAgents/
     Load:    launchctl load ~/Library/LaunchAgents/dev.specere.serve.plist
     (Phase 3's `specere serve` binary implements the ProgramArguments target.) -->
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>dev.specere.serve</string>
  <key>ProgramArguments</key>
  <array>
    <string>specere</string>
    <string>serve</string>
    <string>--config</string>
    <string>.specere/otel-config.yml</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
</dict>
</plist>
"#;

const WINDOWS_README: &str = r#"# SpecERE serve on Windows

No native service file shipped; run the receiver in a long-running shell or via Task Scheduler.

## Interactive
```
specere serve --config .specere\otel-config.yml
```

## Background via Task Scheduler (one-time setup)
```
schtasks /create ^
  /tn "SpecERE serve" ^
  /tr "specere serve --config .specere\otel-config.yml" ^
  /sc onlogon ^
  /rl limited
```

To remove:
```
schtasks /delete /tn "SpecERE serve" /f
```
"#;

pub struct OtelCollector {
    pub with_service: bool,
}

impl OtelCollector {
    pub fn new(with_service: bool) -> Self {
        Self { with_service }
    }
}

impl AddUnit for OtelCollector {
    fn id(&self) -> &'static str {
        UNIT_ID
    }

    fn pinned_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn preflight(&self, _ctx: &Ctx) -> Result<Plan> {
        let mut plan = Plan::default();
        plan.ops.push(PlanOp::CreateDir {
            path: PathBuf::from(".specere"),
        });
        plan.ops.push(PlanOp::WriteFile {
            path: PathBuf::from(".specere/otel-config.yml"),
            summary: "SpecERE OTLP receiver config".into(),
        });
        if self.with_service {
            plan.ops.push(PlanOp::CreateDir {
                path: PathBuf::from(".specere/services"),
            });
            plan.ops.push(PlanOp::WriteFile {
                path: service_file_rel(),
                summary: format!("platform service artifact ({})", std::env::consts::OS),
            });
        }
        Ok(plan)
    }

    fn install(&self, ctx: &Ctx, _plan: &Plan) -> Result<Record> {
        let specere_dir = ctx.repo().join(".specere");
        std::fs::create_dir_all(&specere_dir)
            .map_err(|e| specere_core::Error::Install(format!("create .specere/: {e}")))?;

        let mut record = Record::default();
        record.dirs.push(PathBuf::from(".specere"));

        // 1. otel-config.yml
        let cfg_path = specere_dir.join("otel-config.yml");
        std::fs::write(&cfg_path, OTEL_CONFIG_YML)
            .map_err(|e| specere_core::Error::Install(format!("write otel-config.yml: {e}")))?;
        record.files.push(FileEntry {
            path: PathBuf::from(".specere/otel-config.yml"),
            sha256_post: specere_manifest::sha256_bytes(OTEL_CONFIG_YML.as_bytes()),
            owner: Owner::Specere,
            role: "otel-collector-config".into(),
        });

        // 2. Optional per-platform service artifact
        if self.with_service {
            let services_dir = specere_dir.join("services");
            std::fs::create_dir_all(&services_dir)
                .map_err(|e| specere_core::Error::Install(format!("create services/: {e}")))?;
            record.dirs.push(PathBuf::from(".specere/services"));

            let rel = service_file_rel();
            let abs = ctx.repo().join(&rel);
            let body = service_file_body();
            std::fs::write(&abs, body).map_err(|e| {
                specere_core::Error::Install(format!("write {}: {e}", abs.display()))
            })?;
            record.files.push(FileEntry {
                path: rel,
                sha256_post: specere_manifest::sha256_bytes(body.as_bytes()),
                owner: Owner::Specere,
                role: format!("otel-collector-service-{}", std::env::consts::OS),
            });
        }

        record.notes.push(format!(
            "otel-collector installed (with_service={})",
            self.with_service
        ));
        Ok(record)
    }

    fn remove(&self, ctx: &Ctx, record: &Record) -> Result<()> {
        for f in &record.files {
            let abs = ctx.repo().join(&f.path);
            if !abs.exists() {
                continue;
            }
            if f.owner == Owner::UserEditedAfterInstall {
                tracing::warn!(
                    "otel-collector: `{}` user-edited; preserving",
                    f.path.display()
                );
                continue;
            }
            let actual = specere_manifest::sha256_file(&abs).map_err(|e| {
                specere_core::Error::Remove(format!("sha256 {}: {e}", abs.display()))
            })?;
            if actual != f.sha256_post {
                tracing::warn!(
                    "otel-collector: `{}` edited after install; preserving",
                    f.path.display()
                );
                continue;
            }
            std::fs::remove_file(&abs).map_err(|e| {
                specere_core::Error::Remove(format!("remove {}: {e}", abs.display()))
            })?;
        }
        // GC: empty services/ and empty .specere/ get removed by the dispatcher
        // when manifest.units is empty. But services/ is this unit's alone, so
        // remove it if empty right here.
        let services = ctx.repo().join(".specere/services");
        if services.is_dir() {
            if let Ok(mut it) = std::fs::read_dir(&services) {
                if it.next().is_none() {
                    let _ = std::fs::remove_dir(&services);
                }
            }
        }
        Ok(())
    }
}

fn service_file_rel() -> PathBuf {
    match std::env::consts::OS {
        "linux" => PathBuf::from(".specere/services/specere-serve.service"),
        "macos" => PathBuf::from(".specere/services/dev.specere.serve.plist"),
        _ => PathBuf::from(".specere/services/README.md"),
    }
}

fn service_file_body() -> &'static str {
    match std::env::consts::OS {
        "linux" => LINUX_SYSTEMD_UNIT,
        "macos" => MACOS_LAUNCHD_PLIST,
        _ => WINDOWS_README,
    }
}
