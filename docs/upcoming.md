# Upcoming specs — the SpecERE work queue

> **Purpose.** Lightweight running list of the next feature specs in priority order. Updated when a spec lands, when a phase closes, or when a divergence-adjudication decision queues new work.
>
> Each entry points at the [`docs/specere_v1.md`](specere_v1.md) phase it implements, plus any carry-over items from prior `.specere/decisions.log` entries.

## Priority queue (highest first)

**Nothing blocking a release.** All seven master-plan phases shipped through v1.0.3. The items below are polish-level follow-ups.

### 1. MarkerEntry schema backwards-compat

- **Why.** The specere repo's own committed `.specere/manifest.toml` uses an early pre-`unit_id` MarkerEntry schema. `specere status` / `verify` on a fresh clone of the upstream repo errors with `missing field unit_id`. The self-dogfood guide's Setup block works around this by deleting `.specere/` first — but a real user upgrading from a very early SpecERE install would also hit it.
- **Fix.** `#[serde(default)]` on `MarkerEntry.unit_id`; infer from the containing `[[units]].unit_id` during deserialisation.
- **Scope.** Small — ~20 LoC in `specere-core` + a regression test that loads the old-schema manifest cleanly.

### 2. Motion-matrix fit from `(diff, test-delta)` pairs (Phase 5 tail)

- **Why.** v0.5.0 shipped the coupling-edge suggester half of Phase 5; the motion-matrix fit half was deferred because it needs a durable per-commit test-history source (CI run records, not just event JSONL).
- **Blocker.** No CI-result ingestion yet. A new `specere calibrate from-ci <path-to-junit.xml>` subcommand would unlock this.

### 3. RBPF CLI routing

- **Why.** Library supports it; CLI picks PerSpecHMM or FactorGraphBP only. Users with cyclic coupling graphs get a "DAG required" error from the loader rather than auto-routing to RBPF.
- **Fix.** Read a `[rbpf]` section from sensor-map.toml with cluster + particle config, branch `run_filter_run` accordingly.

### 4. Long spec-ID table alignment

- Cosmetic — table column width fixed at 11 chars; JSON output is the programmatic path. Noted in self-dogfood phase-4 manual-test report M-16.

## Beyond the immediate queue

Nothing in the master plan is open. v1.x is bug-fix + follow-ups only; v2.0 would be a deliberate schema-breaking re-plan.

## Recently closed

- **phase-4-filter-engine main track** (2026-04-18, parent [#39](https://github.com/laiadlotape/specere/issues/39)) — `specere-filter` crate live; `specere filter run/status` wired; FR-P4-001, -003, -004, -006 closed. Execution plan archived at [`docs/history/phase4-execution-plan.md`](history/phase4-execution-plan.md).
  - [#40](https://github.com/laiadlotape/specere/issues/40) PerSpecHMM scaffold (PR #45).
  - [#41](https://github.com/laiadlotape/specere/issues/41) FactorGraphBP + coupling loader + cycle rejection (PR #46).
  - [#42](https://github.com/laiadlotape/specere/issues/42) RBPF escape valve + seeded Gate-A scenario (PR #47).
  - [#43](https://github.com/laiadlotape/specere/issues/43) filter run/status CLI (PR #48).
  - FR-P4-002 (< 2 pp Python parity) + FR-P4-005 (throughput smoke) queued as phase-4-follow-ups above.
- **phase-3-follow-up-grpc** (2026-04-18, [issue #34](https://github.com/laiadlotape/specere/issues/34)) — OTLP/gRPC receiver closes FR-P3-001; `specere serve` now runs HTTP + gRPC concurrently over one SQLite connection.
- **phase-3-observe-pipeline main track** (2026-04-18, parent [#27](https://github.com/laiadlotape/specere/issues/27)) — event pipeline live; execution plan archived at [`docs/history/phase3-execution-plan.md`](history/phase3-execution-plan.md).
  - [#28](https://github.com/laiadlotape/specere/issues/28) event store JSONL + CLI (PR #32) — `specere observe record/query`.
  - [#29](https://github.com/laiadlotape/specere/issues/29) SQLite backend + WAL (PR #33) — primary store; JSONL mirror.
  - [#30](https://github.com/laiadlotape/specere/issues/30) OTLP/HTTP receiver + `specere serve` (PR #35).
  - [#31](https://github.com/laiadlotape/specere/issues/31) workflow-span hooks for all 7 verbs (PR #36).
- **phase-2-native-units** (2026-04-18, parent [#11](https://github.com/laiadlotape/specere/issues/11)) — all 5 MVP units real; execution plan archived at [`docs/history/phase2-execution-plan.md`](history/phase2-execution-plan.md).
  - [#12](https://github.com/laiadlotape/specere/issues/12) filter-state (PR #19) — `.specere/` skeleton + gitignore allowlist.
  - [#16](https://github.com/laiadlotape/specere/issues/16) speckit orphan detector (PR #20) — `Speckit::preflight` + `specere doctor --clean-orphans`.
  - [#13](https://github.com/laiadlotape/specere/issues/13) otel-collector (PR #21) — `.specere/otel-config.yml` + platform service artifacts (opt-in).
  - [#14](https://github.com/laiadlotape/specere/issues/14) ears-linter (PR #22) — advisory lint rules + `before_clarify` hook + skill.
  - [#15](https://github.com/laiadlotape/specere/issues/15) `specere init` (PR #23) — idempotent composition of all 5 units + fix for multi-owner file SHA drift.
- **release-infra** (2026-04-18) — `cargo-dist@0.31` wired via `dist-workspace.toml`; `release.yml` (auto-generated) produces five-target binaries + shell/powershell installers on `v*.*.*` tag push; hand-written `release-guards.yml` validates tag/version match, CHANGELOG section, and main-reachability before artifacts upload. Full tag-cut procedure documented at `docs/release.md`. Spec: `specs/005-release-infra/`.
- **auto-review** (2026-04-18) — `Claude PR review` workflow added at `.github/workflows/claude-review.yml`; enforces the constitution on every PR as advisory review comments. See `docs/auto-review.md` for the GitHub-App-vs-API-key setup. Constitution V's CI-surface companion.

## Queue hygiene

- **Adding.** When a spec surfaces a follow-up (e.g. a review-queue EXTEND decision), add it to the priority-queue section with a one-line link back to its origin (spec id, FR, or decisions.log timestamp).
- **Closing.** When a queued spec lands on `main`, delete its entry here; the CHANGELOG + the phase table in [`README.md`](../README.md) become the authoritative records.
- **Priority.** Reorder only when a real dependency changes. Don't reshuffle on vibes.
