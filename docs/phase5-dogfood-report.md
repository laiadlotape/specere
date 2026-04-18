# v0.5.0 dogfood report — clean-install walkthrough

**Date.** 2026-04-18. **Binary under test.** `target/release/specere` built from `d6dd88e` (v0.4.0) + the fixes described below. **Sandbox.** Fresh `git init` Rust project in `mktemp -d`.

## Goal

Take the tool off the shelf, install it on a brand-new repo, walk end-to-end: `init` → populate sensor-map → `observe record` → `filter run` → `filter status`. Record every friction point and fix any blocker before cutting v0.5.0.

## Findings

### D-01 — `specere --help` still mentions `v0.2.0` (caching only)

Local `target/release/specere` was built before the v0.4.0 version bump. Not a bug — rebuild gives v0.4.0. Noted for release-engineering awareness.

### D-02 — post-init `specere status` shows units at their own pinned versions

Native units carry their own version constants (`0.2.0` at the time of observation, now `0.4.0` after v0.4.0 built). These are **unit-schema** versions, not the binary's. Correct behaviour — leaving a note in case a future user confuses the two.

### D-03 — clean-install leaves a dense but sensible `.specere/` / `.specify/` / `.claude/` footprint

6 files in `.specere/` (events.sqlite, lint/, manifest.toml, otel-config.yml, posterior.toml, sensor-map.toml), all expected. 20 skills + 1 agent under `.claude/`. Nothing surprising.

### D-04 — **BLOCKER**: first `filter run` always fails because `[specs]` is missing

**Root cause.** `filter-state`'s `SENSOR_MAP_TOML_CONTENT` seeded a `[channels]` section but no `[specs]` section. A brand-new user running `specere init && specere filter run` hits:

> `[specs] section empty or missing in sensor-map.toml — add entries like "FR-001" = { support = ["src/a.rs"] }`

The error pointed at `docs/filter.md` — which didn't exist.

**Fix.** Two commits in one PR:

1. `crates/specere-units/src/filter_state.rs` — seed an empty `[specs]` block with a quick-start comment block and a pointer to `docs/filter.md`.
2. `docs/filter.md` — written from scratch; covers sensor-map format, event-attr contract, `filter run` / `status` flags, sensor calibration, troubleshooting.

### D-05 — **BLOCKER**: `filter run` crashes because the pre-seeded `posterior.toml` is not a valid `Posterior`

**Root cause.** `filter-state`'s `POSTERIOR_TOML_CONTENT` wrote only a comment header + `schema_version = 1`. The `Posterior` deserialiser required `entries`, so load-or-default errored on any brand-new repo:

> `parse posterior.toml: TOML parse error at line 1, column 1 | missing field \`entries\``

**Fix, two-pronged (belt and braces):**

1. `crates/specere-units/src/filter_state.rs` — seed `entries = []` in the placeholder so fresh installs load cleanly.
2. `crates/specere-filter/src/posterior.rs` — `#[serde(default)]` on `Posterior.entries` + `Posterior.cursor`, default `schema_version` so even older placeholder shapes (pre-v0.5.0) deserialise without error. Regression test `filter_run_tolerates_pre_existing_placeholder_posterior`.

### D-06 — happy path works after D-04 + D-05 fixes

```sh
$BIN init
$EDITOR .specere/sensor-map.toml  # populate [specs]
$BIN observe record --source X --attr event_kind=test_outcome --attr spec_id=auth_login --attr outcome=pass
$BIN observe record --source X --attr event_kind=test_outcome --attr spec_id=billing_charge --attr outcome=fail
$BIN filter run
$BIN filter status
```

Produces the expected per-spec table with `billing_charge` leaning VIO and `auth_login` leaning SAT. End-to-end green.

### D-07 — calibrate from-git on the specere repo itself surfaces sensible coupling

Ran `specere calibrate from-git --sensor-map /tmp/specere-self-sensor-map.toml` (7 specs covering the 7 crates + CLI). The suggester analysed 55 commits, found 31 touched a tracked spec, and proposed 4 coupling edges:

- `specere-cli → specere-units` (11 co-commits) — CLI additions require unit wiring.
- `specere-cli → specere-telemetry` (6 co-commits) — observe/serve wiring.
- `specere-cli → specere-core` (3 co-commits).
- `specere-core → specere-units` (3 co-commits).

Architecturally exactly what a reviewer would expect. The suggester is useful for real repos, not just synthetic fixtures.

## v0.5.0 ships with

- D-04 + D-05 fixes (above).
- Issue #50 resolved: advisory exclusive file lock on `.specere/filter.lock` serialises concurrent `filter run`. Regression test `filter_run_serialises_concurrent_invocations`.
- `docs/filter.md` end-user guide.
- `specere calibrate from-git` — coupling-edge suggester from git-log co-modification.
- All existing Phase 4 tests still green (156 → 169 total).

## Deferred to v0.6.0+

- Full FR-P5 motion-matrix fit via (diff, test-delta) pairs — needs a durable test-history source that v0.5.0 doesn't yet carry.
- D-01 / D-02 minor UX clarity around version strings in `status` output.
- Long spec-ID table alignment (M-16 from the phase-4 manual-test report).
