# Upcoming specs — the SpecERE work queue

> **Purpose.** Lightweight running list of the next feature specs in priority order. Updated when a spec lands, when a phase closes, or when a divergence-adjudication decision queues new work.
>
> Each entry points at the [`docs/specere_v1.md`](specere_v1.md) phase it implements, plus any carry-over items from prior `.specere/decisions.log` entries.

## Priority queue (highest first)

### 1. `phase-2-native-units` — finish the 5 MVP units

- **Tracked at:** [issue #11](https://github.com/laiadlotape/specere/issues/11) (parent) with 5 sub-issues queued:
  - [#12](https://github.com/laiadlotape/specere/issues/12) filter-state unit (FR-P2-001)
  - [#13](https://github.com/laiadlotape/specere/issues/13) otel-collector unit (FR-P2-002)
  - [#14](https://github.com/laiadlotape/specere/issues/14) ears-linter unit (FR-P2-003)
  - [#15](https://github.com/laiadlotape/specere/issues/15) `specere init` meta-command (FR-P2-005)
  - [#16](https://github.com/laiadlotape/specere/issues/16) `speckit::preflight` orphan detector (decisions.log carry-over)
- **Why it's next.** Per `docs/specere_v1.md §5 Phase 2`, five units ship end-to-end before Phase 3's observe pipeline has anything to plug into.
- **Workflow.** Per `docs/contributing-via-issues.md`: one sub-issue → one branch `NNN-short-slug` → one PR. Merge order: #12 first (others assume `.specere/` exists), then #13/#14 in parallel, then #15 (depends on #12/#13/#14), then #16 (independent). Pick up any ready sub-issue at session start.
- **Phase mapping.** `docs/specere_v1.md §5.P2` (FR-P2-001 … FR-P2-007).

### 2. `phase-3-observe-pipeline` — `specere serve` + persisted events

- **Why it's third.** Builds on Phase 2's `otel-collector` unit to stand up a real embedded OTLP receiver.
- **Deliverables.** `crates/specere-telemetry` gains a `tonic` gRPC server on `localhost:4317`, an `axum` HTTP server on `:4318`, SQLite + JSONL event store, `specere serve` + `specere observe record` + `specere observe query` commands, and the `specere-observe` workflow's OTel-span-around-each-step wrapping.
- **Phase mapping.** `docs/specere_v1.md §5.P3` (FR-P3-001 … FR-P3-006).

## Beyond the immediate queue

Phases 4–7 (filter engine, motion-model calibration, cross-session persistence, v1.0.0 dogfood) remain as in the master plan. They are not queued here because Phases 2 and 3 gate them.

## Recently closed

- **release-infra** (2026-04-18) — `cargo-dist@0.31` wired via `dist-workspace.toml`; `release.yml` (auto-generated) produces five-target binaries + shell/powershell installers on `v*.*.*` tag push; hand-written `release-guards.yml` validates tag/version match, CHANGELOG section, and main-reachability before artifacts upload. Full tag-cut procedure documented at `docs/release.md`. Spec: `specs/005-release-infra/`.
- **auto-review** (2026-04-18) — `Claude PR review` workflow added at `.github/workflows/claude-review.yml`; enforces the constitution on every PR as advisory review comments. See `docs/auto-review.md` for the GitHub-App-vs-API-key setup. Constitution V's CI-surface companion.

## Queue hygiene

- **Adding.** When a spec surfaces a follow-up (e.g. a review-queue EXTEND decision), add it to the priority-queue section with a one-line link back to its origin (spec id, FR, or decisions.log timestamp).
- **Closing.** When a queued spec lands on `main`, delete its entry here; the CHANGELOG + the phase table in [`README.md`](../README.md) become the authoritative records.
- **Priority.** Reorder only when a real dependency changes. Don't reshuffle on vibes.
