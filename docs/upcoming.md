# Upcoming specs — the SpecERE work queue

> **Purpose.** Lightweight running list of the next feature specs in priority order. Updated when a spec lands, when a phase closes, or when a divergence-adjudication decision queues new work.
>
> Each entry points at the [`docs/specere_v1.md`](specere_v1.md) phase it implements, plus any carry-over items from prior `.specere/decisions.log` entries.

## Priority queue (highest first)

### 1. `phase-4-filter-engine` — per-spec posterior over agent telemetry

- **Why it's next.** Phase 3 shipped the event stream; Phase 4 consumes it. Rust port of ReSearch's `prototype/mini_specs/filter.py` (per-spec HMM + factor-graph BP + RBPF escape valve). Produces `.specere/posterior.toml` — the live spec-belief surface that the rest of the v1.0 vision hangs on.
- **Deliverables.** New `specere-filter` crate. `specere filter run` consumes SQLite event store → advances filter → writes posterior. `specere filter status` reads posterior and prints per-spec belief table (sorted by entropy). Per-spec coupling graph loaded from `.specere/sensor-map.toml` (no auto-inference in v1).
- **Phase mapping.** `docs/specere_v1.md §5.P4` (FR-P4-001 … FR-P4-006).
- **Workflow.** Per `docs/contributing-via-issues.md`. Sub-issues likely split per filter family (PerSpecHMM → FactorGraphBP → RBPF) so each lands testable against the ReSearch prototype's Gate-A scenario.

### 2. `phase-3-follow-up-grpc` — OTLP/gRPC receiver (#34)

- **Tracked at:** [issue #34](https://github.com/laiadlotape/specere/issues/34) (split from #30 during Phase 3 re-plan).
- **Deliverables.** Add `tonic` gRPC server on :4317 via `opentelemetry-proto` generated types. `specere serve` starts both receivers concurrently via `tokio::try_join!`.
- **Why deferred.** Phase 3 scope-growth trigger fired when adding tonic + opentelemetry-proto to the same PR as axum HTTP; clean split to keep `#30`'s scope under the 600 LoC ceiling. HTTP half is live; gRPC is the remaining half of FR-P3-001.
- **Workflow.** Single issue → single PR; no sub-issues needed.

## Beyond the immediate queue

Phases 5–7 (motion-model calibration, cross-session persistence, v1.0.0 dogfood) remain as in the master plan.

## Recently closed

- **phase-3-observe-pipeline main track** (2026-04-18, parent [#27](https://github.com/laiadlotape/specere/issues/27)) — event pipeline live; execution plan archived at [`docs/history/phase3-execution-plan.md`](history/phase3-execution-plan.md).
  - [#28](https://github.com/laiadlotape/specere/issues/28) event store JSONL + CLI (PR #32) — `specere observe record/query`.
  - [#29](https://github.com/laiadlotape/specere/issues/29) SQLite backend + WAL (PR #33) — primary store; JSONL mirror.
  - [#30](https://github.com/laiadlotape/specere/issues/30) OTLP/HTTP receiver + `specere serve` (PR #35).
  - [#31](https://github.com/laiadlotape/specere/issues/31) workflow-span hooks for all 7 verbs (PR #36).
  - gRPC receiver queued as [#34](https://github.com/laiadlotape/specere/issues/34) (Phase-3-follow-up priority 2).
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
