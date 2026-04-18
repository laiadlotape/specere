# Phase 7 — memaso dogfood report (v1.0.0 candidate)

**Date.** 2026-04-18. **Binary.** `target/debug/specere` built from `96452ea` (v0.5.0) + v1.0.0 candidate fixes. **Target.** Copy of `$HOME/Projects/memaso` (branch `impl/rewrite-v2`) at `$HOME/Projects/tmp/memaso-dogfood-<timestamp>`. The original memaso working copy was not touched.

## Why memaso

memaso is a real 2.2 GB multi-language project (Kotlin/Android + TS/Electron + design-system) with ~80 commits on its active branch. It's as unlike specere's test fixtures as a target can reasonably be — Android app structure, mixed languages, design-system package-layout, assets everywhere. If `specere` breaks under a real-world install, memaso will find it.

## Findings

### P-01 — pre-state

Branch `impl/rewrite-v2` at `e21afe2`. Working tree had uncommitted Kotlin edits + uncommitted `.claude/mailbox/`. Intentionally did not touch the original repo; copied to `$HOME/Projects/tmp/memaso-dogfood-<ts>/` for isolation.

### P-02 — `specere init` is idempotent on a repo with an existing install

Status: ✅ Pass. The memaso copy carried `.specere/manifest.toml` from a prior dogfood. `specere init` correctly detected each unit as already installed and no-op'd all 5. No state corruption, no duplicate install. Exit 0.

### P-03 / P-04 — pre-v0.5.0 state still loads

The existing sensor-map.toml was in the **old** format (no `[specs]` section); the existing posterior.toml was in the **old** format (no `entries` field). `specere status` listed all 5 units at their pre-upgrade version (0.2.0). This confirms the v0.5.0 `#[serde(default)]` tolerance works for in-place upgrades — the new binary reads an old install without erroring.

### P-05 — `filter run` without `[specs]` gives an actionable error

Status: ✅ Pass. `[specs] section empty or missing in sensor-map.toml — add entries like ...`. Exit 1. This is the error `docs/filter.md` documents.

### P-06 — `calibrate from-git` surfaces the right architectural coupling for a real repo

Status: ✅ Pass and **genuinely useful**. On memaso's 79-commit history with 6 hand-authored specs (app/core/design/desktop/onboarding/auth_gate), the suggester analysed 52 spec-touching commits and proposed 5 edges:

| Edge | Co-commits |
|---|---|
| `app_layer ↔ core_layer` | **23** |
| `app_layer ↔ onboarding` | 6 |
| `core_layer ↔ onboarding` | 6 |
| `app_layer ↔ desktop_ui` | 4 |
| `core_layer ↔ desktop_ui` | 4 |

An architect reviewing these would confirm: yes, memaso's Android app layer genuinely co-evolves with core; design-system is correctly isolated (no edges — 1 co-commit filtered by the min-3 threshold).

### P-07..P-11 — full filter pipeline works end-to-end with BP coupling

Synthesised 10 events (5 pass on `core_layer`, 3 fail on `auth_gate`, one `files_touched` with core+app paths, one pass on `onboarding`). After `filter run`:

| spec | p_unk | p_sat | p_vio | comment |
|---|---|---|---|---|
| `core_layer` | 0.08 | **0.86** | 0.05 | 5 passes → heavily SAT |
| `auth_gate` | 0.12 | 0.03 | **0.85** | 3 fails → VIO |
| `onboarding` | 0.32 | 0.58 | 0.10 | 1 pass pulled toward SAT |
| `desktop_ui` | 0.27 | 0.29 | 0.44 | BP-lifted via app_layer's p_v |
| `app_layer` | 0.31 | 0.34 | 0.35 | one files_touched predict step |
| `design_system` | 0.31 | 0.34 | 0.35 | (no direct events; drifted via identity-leak) |

Cross-session resume (P-11): added a fourth `auth_gate` fail event in a new process, re-ran filter. Result: `auth_gate` p_vio climbed 0.85 → **0.93**. This is the FR-P6 cross-session-persistence invariant.

### P-FR-P6 — **CRITICAL BUG CAUGHT**: cross-session belief was not accumulating

While writing the Phase 6 test suite (before running the dogfood), caught that `run_filter_run` was re-initialising the filter to uniform on every invocation instead of seeding from the persisted posterior. That meant repeated `filter run` calls effectively replayed events against a fresh uniform prior every time — belief never accumulated across processes.

Fix: new `PerSpecHMM::set_belief` / `FactorGraphBP::set_belief`; `run_filter_run` now seeds the backend's belief matrix from `existing.entries` before processing new events. Regression test `cursor_resumes_across_processes_consuming_only_new_events` asserts the accumulation.

Without this fix, the entire cross-session posterior story was a fiction. Caught before memaso ran, validated by memaso (P-11 auth_gate 0.85 → 0.93 progression).

### P-12 — `specere verify` correctly detects runtime-edited files as drifted

Status: ✅ Pass. After `filter run` + `observe record`, `verify` flagged `.specere/events.sqlite`, `posterior.toml`, `sensor-map.toml` as drifted. Correct — those files are legitimately edited by runtime operations.

### P-13 — full uninstall preserves user edits, reports what was preserved

Status: ✅ Pass. Remove ran through the 5 units in reverse install order. Preserved user-edited files with actionable warnings (posterior, sensor-map, CLAUDE.md). The conservative "don't clobber anything the user might want" stance is exactly right for a production tool.

### P-14 — post-uninstall footprint is reasonable

Status: ✅ Pass with a minor-finding-then-fix. `.specere/manifest.toml` removed; `.specify/` removed; specere-owned skills removed from `.claude/`. User-edited files preserved. Non-specere `.claude/` content (memaso's own agents, commands, rules) untouched.

### P-15 — **FINDING FIXED**: `.specere/filter.lock` orphaned after uninstall

Status: ⚠️ found → ✅ fixed in-branch.

Issue #50's advisory-lock sidecar (`.specere/filter.lock`) was created by `filter run` but not tracked by any unit's manifest, so `remove filter-state` left a 0-byte orphan behind. Not harmful — it's inside `.specere/*` and gitignored — but inelegant and surprising.

Fix: `filter-state::remove` now best-effort deletes a documented list of "ephemeral sidecars" (`EPHEMERAL_SIDECARS = [".specere/filter.lock"]`). The file is ephemeral runtime state with no user content, so unconditional sweep is safe. Verified with P-18 re-run: `filter.lock` is gone after `remove filter-state`.

### P-16 — `specere doctor` on post-uninstall repo reports sensibly

Status: ✅ Pass. Reports `manifest: absent`, tool prereqs green.

### P-17 — full round-trip install → use → uninstall → re-install is clean

Status: ✅ Pass. Re-install after uninstall produces a pristine install — `specere verify` reports `No drift.` immediately. No leftover state confuses the second install.

### P-18 — filter.lock sweep verified after the in-branch fix

Status: ✅ Pass. Hand-seeded filter.lock → `remove filter-state --force` → file gone.

## Bugs fixed in this dogfood cycle

| ID | Severity | Fix location | Regression test |
|---|---|---|---|
| FR-P6 cross-session belief reset | **Blocker** (breaks FR-P6) | `crates/specere/src/main.rs::run_filter_run` + `PerSpecHMM::set_belief` + `FactorGraphBP::set_belief` | `cursor_resumes_across_processes_consuming_only_new_events` + 4 other Phase 6 tests |
| P-15 filter.lock orphan | Minor | `crates/specere-units/src/filter_state.rs::remove` + `EPHEMERAL_SIDECARS` | Manual sweep in dogfood P-18 |

## Shipped as v1.0.0 after this PR

- Phase 6 cross-session persistence — validated end-to-end; FR-P6 blocker fixed.
- Phase 7 real-world dogfood — memaso install-use-uninstall round-trip is clean.
- Test count: 168 → 178 (+5 Phase 6 regression tests; +5 retained from prior phases).
- All CI gates green.

## Deferred (not blockers for v1.0.0)

- Full FR-P5 motion-matrix fit via (diff, test-delta) pairs — waiting on durable test-history source.
- RBPF routing from CLI for repos with cyclic coupling — RBPF is wired in-library, CLI selector is deferred.
- Long spec-ID table alignment (phase-4 M-16) — JSON output is the workaround.
