# Phase 3 execution plan — auto-mode sequential delivery of issues #27–#31

**Status.** Drafted 2026-04-18 post-Phase-2 close (main at `ebc2d30`; PR #26's ears-lint fix on top). Governs sequential delivery of Phase 3's four sub-issues.
**Authority.** `docs/contributing-via-issues.md` (pipeline) · `docs/specere_v1.md §5 Phase 3` (scope) · `.specify/memory/constitution.md` (rules).
**Predecessor.** `docs/history/phase2-execution-plan.md` — the pattern and estimate calibration (21 tests delivered vs 23 projected, ±50% band never tripped) carries over.

## 1. Context

Phase 2 shipped 5 native units (filter-state, otel-collector, ears-linter, claude-code-deploy, speckit wrapper) + `specere init` + orphan detector. Post-Phase-2 fix (PR #26 / issue #25) added `specere lint ears`. Now **Phase 3 turns the pipeline on**: OTLP receivers, persistent event store, CLI to record/query, and workflow span emission. Parent: [#27](https://github.com/laiadlotape/specere/issues/27).

| Issue | Title | FR |
|---|---|---|
| [#28](https://github.com/laiadlotape/specere/issues/28) | Event store + `specere observe record/query` (JSONL backend) | FR-P3-004 partial |
| [#29](https://github.com/laiadlotape/specere/issues/29) | SQLite event store + WAL + query latency | FR-P3-003, FR-P3-004 complete, FR-P3-005 |
| [#30](https://github.com/laiadlotape/specere/issues/30) | OTLP receivers (gRPC + HTTP) + `specere serve` | FR-P3-001, FR-P3-005 |
| [#31](https://github.com/laiadlotape/specere/issues/31) | `specere-observe` workflow emits gen_ai.* spans | FR-P3-002, FR-P3-006 |

## 2. Auto-mode contract

Same as Phase 2's (see `docs/history/phase2-execution-plan.md §2`). In short: the issue body is the spec; `/speckit-implement` runs normally; review gate = divergence-adjudication at PR-merge time.

**Adjustments for Phase 3's larger scope:**

- Sub-issues #29 and #30 are each estimated > Phase 2's biggest sub-issue. Re-plan thresholds tighten: if either exceeds 600 LoC of impl or 3 CI retries, pause and reassess.
- Phase 3 introduces async + network; tests need `tokio::test` + ephemeral-port allocation. Tests that bind to fixed ports 4317/4318 will race across the CI matrix — use `tokio::net::TcpListener::bind("127.0.0.1:0")` and read the assigned port back.
- SIGINT testing needs `std::process::Command::new + kill -INT`. Use `nix` or spawn via `tokio::process::Command` and signal via `child.id()`. Platform-portable variants noted in #30.

## 3. Sequence + dependency graph

```
#28 (event store + CLI)  ──►  #29 (SQLite upgrade)  ──►  #30 (receivers + serve)  ──►  #31 (workflow spans)
```

**Strictly sequential.** Each sub-issue consumes the previous's surface. No parallel tracks this phase.

## 4. Per-sub-issue recipe

Same 20-step recipe as Phase 2 (`docs/history/phase2-execution-plan.md §4`). Notable delta: **TDD red for async surfaces** — write the test with `#[tokio::test]`, start with an assertion that compiles against the target API (even if the function stub returns a placeholder Err), watch it fail the real assertion, then implement.

## 5. Re-planning triggers (tightened from Phase 2)

Trigger a pause + re-plan check when **any** of these fires:

- **Test-count deviation**: > 1.5× or < 0.5× estimate (unchanged).
- **Scope growth**: sub-issue PR exceeds **600 LoC** of new non-test code (was 500 in Phase 2; Phase 3's network + async code is denser).
- **CI retries**: > 3 retries on the same PR (was 3 total — now explicitly same-PR).
- **New FR surfaces**: unchanged.
- **Cross-sub-issue contract changes**: `EventStore` trait added in #28 may need refinement in #29 (JSONL → SQLite switch) — that's expected; re-plan only if the refactor breaks downstream sub-issues' acceptance criteria.
- **Review-queue drain surfaces novel item**: unchanged (the review queue already has empty state; new items need explicit adjudication).
- **Port binding conflict on CI**: receivers tests that bind ports must use ephemeral allocation. If we see port conflicts in CI, fix in the current sub-issue rather than deferring.

## 6. Escalation-to-user triggers

Same five as Phase 2 (`§6` of the history doc): CI same-PR 3× consecutive fails, required credential needed, breaking downstream contract, spec-level disagreement, user interrupt. Added:

- **Async deadlock or CI hang > 30 min**: network receivers stuck in tests. Pause; investigate rather than retry.
- **Port binding collision on a CI runner**: likely GitHub Actions parallel-job interference. Pause; verify ephemeral-port usage.

## 7. Phase 3 exit criteria

Phase 3 closes when **all** of:

- [ ] #28, #29, #30, #31 merged to `main`; parent #27 closed.
- [ ] `cargo test --workspace --all-targets` ≥ 85 (projection: ~88 — 69 today + ~19 new).
- [ ] `specere serve` starts + binds localhost:4317 + :4318 simultaneously; `specere observe query` round-trips events; `specify workflow run specere-observe` emits ≥ 1 span per step.
- [ ] `docs/upcoming.md` shows `phase-3-observe-pipeline` under `## Recently closed`; Phase 4 (filter engine) becomes priority 1.
- [ ] `README.md` phase-status table marks Phase 3 ✅ Shipped.
- [ ] This plan moves to `docs/history/phase3-execution-plan.md` at close (same pattern as Phase 2).
- [ ] Optional: cut v0.4.0 release (release infra ready; user decision).

## 8. Estimates

Per-sub-issue sizing, calibrated against Phase 2's delivery (impl LoC / test LoC / # tests / CI retries / risk):

| Issue | Est. LoC (impl) | Est. LoC (tests) | Est. tests | CI retries | Risk |
|---|---|---|---|---|---|
| #28 event store JSONL + CLI | 180 | 200 | 5 | 0 | low — pure-local file I/O |
| #29 SQLite backend | 250 | 250 | 5 | 1 | med — WAL + migration corner cases |
| #30 receivers + serve | 400 | 350 | 6 | 2 | **high** — async + network + SIGINT + deps (tonic/axum/tokio) |
| #31 workflow spans | 120 | 180 | 3 | 1 | med — hook integration across 7 verbs |
| **Total** | **~950** | **~980** | **~19** | **~4** | |

Post-Phase-3 test count projection: 69 + 19 ≈ **88**.

Compared to Phase 2's ~670 impl / ~950 test / 23 tests / 3 retries — Phase 3 is 40% more impl LoC but with fewer tests (network + async are heavier code-per-test).

## 9. Deferred to Phase 4

- Filter engine (HMM / FGBP / RBPF in `specere-filter` crate).
- Motion-model calibration from git history.
- Posterior TOML + `specere filter run / status` commands.

## 10. Living document

Same as Phase 2 §9. Updated in place on re-planning; moves to `docs/history/phase3-execution-plan.md` at Phase 3 close.
