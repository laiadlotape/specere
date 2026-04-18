# Upcoming specs — the SpecERE work queue

> **Purpose.** Lightweight running list of the next feature specs in priority order. Updated when a spec lands, when a phase closes, or when a divergence-adjudication decision queues new work.
>
> Each entry points at the [`docs/specere_v1.md`](specere_v1.md) phase it implements, plus any carry-over items from prior `.specere/decisions.log` entries.

## Priority queue (highest first)

### 1. `release-infra` — cut v0.2.0 (blocks the Phase 1 release tag)

- **Why it's first.** Phase 1 is merged on `main` at `ae7b4c4`, but the workspace is still version-pinned as `0.2.0-dev`. No v0.2.0 tag has been cut because `cargo-dist` + `.github/workflows/release.yml` are not wired (the master plan's "cargo-dist is already configured" assumption is stale — verified 2026-04-18 post-PR-#2).
- **Deliverables.**
  - Add `.github/workflows/release.yml` driven by `cargo-dist` (tag trigger → cross-platform binaries → GitHub Release assets).
  - Add `[workspace.metadata.dist]` in `Cargo.toml` with the four target triples from `docs/roadmap/31_specere_scaffolding.md §2`.
  - Bump workspace version `0.2.0-dev → 0.2.0`; move CHANGELOG `[0.2.0-dev]` → `[0.2.0] — 2026-MM-DD`.
  - Tag `v0.2.0` on the release-infra merge commit.
- **Scope guard.** Pure release plumbing. No FR changes, no crate surface changes.
- **Phase mapping.** Closes out Phase 1 (docs/specere_v1.md §5.P1) — the plan listed cargo-dist as an implicit prereq; this spec makes it explicit.

### 2. `phase-2-native-units` — finish the 5 MVP units

- **Why it's next.** Per `docs/specere_v1.md §5 Phase 2`, five units ship end-to-end before Phase 3's observe pipeline has anything to plug into.
- **Deliverables.**
  - Promote `filter-state`, `otel-collector`, `ears-linter` from stub to real implementations (currently `stub::StubUnit` in `crates/specere-units/src/lib.rs`).
  - Extend `claude-code-deploy` with `specere-observe-implement` / `specere-review-check` / `specere-review-drain` skills registered under the right slash-command surface (these skills' `SKILL.md` files are already bundled; missing piece is `specere init`-time activation).
  - `specere init` meta-command that composes all five units idempotently, per Phase 2 FR-P2-005.
- **Carry-over from v0.2.0 review-queue drain** (`.specere/decisions.log` entry 2026-04-18): `speckit::preflight` orphan detector. The wrapper unit should detect stale `.specify/feature.json` / ghost feature-branch dirs (produced when `specify workflow run` spawns `claude -p` and that subprocess is killed mid-run). Folds into the `speckit` section of the Phase 2 spec.
- **Phase mapping.** `docs/specere_v1.md §5.P2` (FR-P2-001 … FR-P2-007).

### 3. `phase-3-observe-pipeline` — `specere serve` + persisted events

- **Why it's third.** Builds on Phase 2's `otel-collector` unit to stand up a real embedded OTLP receiver.
- **Deliverables.** `crates/specere-telemetry` gains a `tonic` gRPC server on `localhost:4317`, an `axum` HTTP server on `:4318`, SQLite + JSONL event store, `specere serve` + `specere observe record` + `specere observe query` commands, and the `specere-observe` workflow's OTel-span-around-each-step wrapping.
- **Phase mapping.** `docs/specere_v1.md §5.P3` (FR-P3-001 … FR-P3-006).

## Beyond the immediate queue

Phases 4–7 (filter engine, motion-model calibration, cross-session persistence, v1.0.0 dogfood) remain as in the master plan. They are not queued here because Phases 2 and 3 gate them.

## Recently closed

- **auto-review** (2026-04-18) — `Claude PR review` workflow added at `.github/workflows/claude-review.yml`; enforces the constitution on every PR as advisory review comments. See `docs/auto-review.md` for the GitHub-App-vs-API-key setup. Constitution V's CI-surface companion.

## Queue hygiene

- **Adding.** When a spec surfaces a follow-up (e.g. a review-queue EXTEND decision), add it to the priority-queue section with a one-line link back to its origin (spec id, FR, or decisions.log timestamp).
- **Closing.** When a queued spec lands on `main`, delete its entry here; the CHANGELOG + the phase table in [`README.md`](../README.md) become the authoritative records.
- **Priority.** Reorder only when a real dependency changes. Don't reshuffle on vibes.
