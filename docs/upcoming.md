# Upcoming specs — the SpecERE work queue

> **Purpose.** Lightweight running list of the next feature specs in priority order. Updated when a spec lands, when a phase closes, or when a divergence-adjudication decision queues new work.
>
> Each entry points at the [`docs/specere_v1.md`](specere_v1.md) phase it implements, plus any carry-over items from prior `.specere/decisions.log` entries.

## Priority queue (highest first)

### 1. `phase-3-observe-pipeline` — `specere serve` + persisted events

- **Why it's next.** Builds on Phase 2's `otel-collector` unit (shipped via PR #21) to stand up a real embedded OTLP receiver. The \`.specere/otel-config.yml\` scaffolded today is waiting for a binary that honours it.
- **Deliverables.** `crates/specere-telemetry` gains a `tonic` gRPC server on `localhost:4317`, an `axum` HTTP server on `:4318`, SQLite + JSONL event store, `specere serve` + `specere observe record` + `specere observe query` commands, and the `specere-observe` workflow's OTel-span-around-each-step wrapping.
- **Phase mapping.** `docs/specere_v1.md §5.P3` (FR-P3-001 … FR-P3-006).
- **Workflow.** Per `docs/contributing-via-issues.md`, open a parent issue + sub-issues (likely one per deliverable above) when work starts.

## Beyond the immediate queue

Phases 4–7 (filter engine, motion-model calibration, cross-session persistence, v1.0.0 dogfood) remain as in the master plan. They are not queued here because Phases 2 and 3 gate them.

## Recently closed

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
