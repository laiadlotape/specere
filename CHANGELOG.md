# Changelog

All notable changes to SpecERE will be documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project aims to adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once 0.1.0 ships.

## [Unreleased]

### Added
- Initial Rust workspace skeleton: `specere`, `specere-core`, `specere-units`, `specere-manifest`, `specere-markers`, `specere-telemetry`.
- CLI surface stubs: `add`, `remove`, `status`, `verify`, `doctor`, `observe`, `version`.
- `AddUnit` trait in `specere-core` with the six-tuple contract (preflight, install, postflight, remove, manifest, idempotency).
- TOML manifest schema in `specere-manifest`.
- Marker-fenced shared-file editing in `specere-markers`.
- CI (fmt, clippy, test), dependabot, Apache-2.0 LICENSE.

### Decided
- `specere add speckit` wraps upstream `github/spec-kit` via `uvx`, pinned per SpecERE release.
- `specere add otel-collector` scaffolds a local OTel collector (embedded Rust backend default; `contrib` upstream backend optional) — no external prerequisite.
- Manifest at `.specere/manifest.toml` records every file/dir installed, SHA256 pre/post, install config, and marker blocks. First-class `remove` reverses only what we installed.
