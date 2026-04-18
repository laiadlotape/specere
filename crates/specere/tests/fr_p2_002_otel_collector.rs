//! FR-P2-002 / issue #13 — `otel-collector` unit writes `.specere/otel-config.yml`
//! and, with `--service`, a platform-specific service artifact.

mod common;

use common::TempRepo;

#[test]
fn default_install_writes_otel_config_only() {
    let repo = TempRepo::new();
    let out = repo
        .run_specere(&["add", "otel-collector"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "install failed — exit {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );

    let cfg = repo.abs(".specere/otel-config.yml");
    assert!(cfg.exists(), "otel-config.yml not written");
    let text = std::fs::read_to_string(&cfg).unwrap();
    assert!(
        text.contains("receivers:") && text.contains("otlp:"),
        "otel-config.yml missing OTLP receiver section:\n{text}"
    );
    assert!(
        text.contains("4317") && text.contains("4318"),
        "otel-config.yml missing gRPC/HTTP ports:\n{text}"
    );
    // gen_ai.* tuning note at minimum documented in the file.
    assert!(
        text.contains("gen_ai"),
        "otel-config.yml should reference gen_ai.* semconv:\n{text}"
    );

    // Without --service, no service artifact.
    assert!(
        !repo.abs(".specere/services").exists(),
        "--service opt-in but services dir was written"
    );
}

#[test]
fn service_flag_writes_platform_artifact() {
    let repo = TempRepo::new();
    let out = repo
        .run_specere(&["add", "otel-collector", "--service"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "install --service failed — stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let services = repo.abs(".specere/services");
    assert!(services.is_dir(), "services dir not created");

    let entries: Vec<_> = std::fs::read_dir(&services)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().into_string().unwrap())
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "expected exactly one service artifact; got {entries:?}"
    );

    let name = &entries[0];
    match std::env::consts::OS {
        "linux" => assert!(
            name.ends_with(".service"),
            "on Linux expected *.service (systemd); got {name}"
        ),
        "macos" => assert!(
            name.ends_with(".plist"),
            "on macOS expected *.plist (launchd); got {name}"
        ),
        "windows" => assert!(
            name.ends_with(".md") || name.ends_with(".txt"),
            "on Windows expected *.md/*.txt documentation; got {name}"
        ),
        _ => {}
    }
}

#[test]
fn round_trip_is_clean() {
    let repo = TempRepo::new();
    assert!(repo
        .run_specere(&["add", "otel-collector"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(repo
        .run_specere(&["remove", "otel-collector"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(
        !repo.abs(".specere/otel-config.yml").exists(),
        "otel-config.yml leaked on remove"
    );
}

#[test]
fn reinstall_is_idempotent() {
    let repo = TempRepo::new();
    assert!(repo
        .run_specere(&["add", "otel-collector"])
        .output()
        .unwrap()
        .status
        .success());
    let sha_before = sha_of(&repo.abs(".specere/otel-config.yml"));
    let out = repo
        .run_specere(&["add", "otel-collector"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "re-install should be a no-op; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let sha_after = sha_of(&repo.abs(".specere/otel-config.yml"));
    assert_eq!(sha_before, sha_after, "re-install mutated otel-config.yml");
}

fn sha_of(path: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).unwrap_or_default();
    let mut h = Sha256::new();
    h.update(&bytes);
    hex::encode(h.finalize())
}
