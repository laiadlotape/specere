# Upcoming specs — the SpecERE work queue

> **Purpose.** Lightweight running list of the next feature specs in priority order. Updated when a spec lands, when a phase closes, or when a divergence-adjudication decision queues new work.
>
> Each entry points at the [`docs/specere_v1.md`](specere_v1.md) phase it implements, plus any carry-over items from prior `.specere/decisions.log` entries.

## Priority queue (highest first)

**v1.2.0 harness-manager feature-complete on `main`.** All 30 FR-HM + 7 FR-EQ landed (PRs #88–#101). Queue below is for the release-cut + v2.0.0 GUI work.

### 0. Cut v1.0.5 + v1.2.0 release tag

- **What.** First tag since v1.0.4. Combines the evidence-quality slice (FR-EQ-001..007) + harness-manager slice (FR-HM-001..072). Both accumulate under `[Unreleased]` in CHANGELOG.md today.
- **Why deferred.** User chose "mega-release packaging" in the §10 harness-manager questionnaire — means no patch-level tag until every slice landed. That condition is now met.
- **Action.** Bump workspace version in Cargo.toml from `1.0.4` → `1.2.0`; split the Unreleased section into `[1.2.0]` headers; tag + push; cargo-dist publishes.

### 1. v2.0.0 GUI scaffolding (Tauri v2 + Sigma.js)

- **Scope.** FR-HM-080..085 — six-screen MVP: Harness Graph, Spec Dashboard, Review Queue, Event Timeline, Relation Inspector, Calibration View.
- **Stack.** Tauri v2 shell + `@sigma/core` + `graphology` for 10k+-node WebGL graph; React Flow for edge-inspector panels; reuses existing Axum `serve http` endpoints.
- **Blocker.** Adds JS/TS build toolchain to the repo for the first time — worth user check-in before starting.
- **Estimated size.** ~500 LoC Rust (new REST endpoints) + ~3000 LoC frontend.

### 2. v1.0.6 bug-tracker bridge (FR-EQ-010..013)

- **Scope.** `specere observe watch-issues` polls GitHub + Gitea; emits `bug_reported` events that feed the posterior with decay. LLM issue-to-spec triage via text-embedding-3-small.
- **Size.** ~600 LoC; adds `octocrab` + a Gitea client.
- **Blocker.** Needs user credentials — config surface to design.

### 3. v1.1.0 LLM adversary agent (FR-EQ-020..024)

- **Scope.** Budgeted ($20/mo cap) counter-test generator.
- **Size.** ~800 LoC + ongoing LLM spend.

### 4. Long spec-ID table alignment

- Cosmetic — table column width fixed at 11 chars; JSON output is the programmatic path. Noted in self-dogfood phase-4 manual-test report M-16.

## Beyond the immediate queue

Nothing in the v1.0 master plan is open. v1.0.x line is bug-fix + evidence-quality; v1.2.0 is the harness manager (above); v2.0.0 GUI requires a deliberate JS toolchain decision; post-v2 queue is bug-tracker + LLM adversary.

## Recently closed

- **v1.2.0 harness manager & inspector** (2026-04-20) — 30 FR-HM spread across 8 PRs; 358 workspace tests total. Plan at [`docs/harness-manager-plan.md`](./harness-manager-plan.md); proposal at [`docs/proposals/v3-harness-manager.md`](./proposals/v3-harness-manager.md).
  - [PR #94](https://github.com/laiadlotape/specere/pull/94) S1+S2: `specere harness scan` + `specere harness provenance`.
  - [PR #96](https://github.com/laiadlotape/specere/pull/96) S3: `specere harness history` — churn, age, hotspot, co-modification PPMI.
  - [PR #97](https://github.com/laiadlotape/specere/pull/97) S4: `specere harness coverage` — LCOV → Jaccard → cov_cooccur edges.
  - [PR #98](https://github.com/laiadlotape/specere/pull/98) S5: `specere harness flaky` — CI co-failure PPMI + Meta-style flakiness + DeFlaker filter.
  - [PR #99](https://github.com/laiadlotape/specere/pull/99) S6: `specere harness cluster` — Louvain on the composite-edge graph.
  - [PR #100](https://github.com/laiadlotape/specere/pull/100) FR-HM-060..061: `specere.harness.*` OTel supplementary semantic convention + completion events per verb.
  - [PR #101](https://github.com/laiadlotape/specere/pull/101) FR-HM-070..072: `specere harness tui` — ratatui companion.
- **v1.0.5 evidence-quality** (2026-04-19..20) — 7 FR-EQ across 5 PRs (#88–#92). Plan at [`docs/evidence-quality-plan.md`](./evidence-quality-plan.md); proposal at [`docs/proposals/v2-evidence-quality.md`](./proposals/v2-evidence-quality.md).
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
