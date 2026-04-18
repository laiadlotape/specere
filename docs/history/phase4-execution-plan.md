# Phase 4 execution plan — auto-mode sequential delivery of issues #40–#43

**Status.** Drafted 2026-04-18 post-Phase-3 full close (main at `111d394`; PR #38 merged). Governs sequential delivery of Phase 4's four sub-issues under parent [#39](https://github.com/laiadlotape/specere/issues/39).
**Authority.** `docs/contributing-via-issues.md` (pipeline) · `docs/specere_v1.md §5 Phase 4` (scope) · `.specify/memory/constitution.md` (rules).
**Predecessors.** `docs/history/phase{2,3}-execution-plan.md` — patterns carry over; Phase 3 calibration (32 tests delivered vs 19 projected, 1.68× band trip but CI green) informs Phase 4 estimates with a deliberately wider test cushion.

## 1. Context

Phase 3 turned the pipeline on: events flow into SQLite via HTTP (:4318) + gRPC (:4317), workflow spans land on every `/speckit-*` verb. **Phase 4 consumes that stream** — a Rust port of ReSearch's `prototype/mini_specs/filter.py` that writes a live per-spec posterior at `.specere/posterior.toml`. This is the belief surface the rest of the v1.0 vision hangs on.

| Issue | Title | FRs |
|---|---|---|
| [#40](https://github.com/laiadlotape/specere/issues/40) | `specere-filter` crate scaffold + PerSpecHMM forward recursion | FR-P4 precondition |
| [#41](https://github.com/laiadlotape/specere/issues/41) | FactorGraphBP + coupling graph loader | FR-P4-006 |
| [#42](https://github.com/laiadlotape/specere/issues/42) | RBPF escape valve + Gate-A parity test | FR-P4-002 |
| [#43](https://github.com/laiadlotape/specere/issues/43) | `specere filter run / status` CLI | FR-P4-001, 003, 004, 005 |

## 2. Auto-mode contract

Same as Phase 3's. Issue body is the spec; `/speckit-implement` runs normally; divergence-adjudication at PR-merge time. Human gates (core_theory §4): **spec authorship** = issue bodies landed pre-implementation; **contract authoring** = at scaffold time for `Filter` trait; **prior setting** = hyperparameters ported verbatim from ReSearch prototype (not re-derived); **divergence adjudication** = on PR review + on any tail-accuracy miss in Gate-A parity.

**Phase 4 adjustments:**

- Numerical code — fixed seed everywhere. No `rand::thread_rng`, no `SystemTime::now()` in test paths. A single `rand::rngs::StdRng::seed_from_u64(0xSPECERE)` for the whole filter suite.
- Golden-file tests — posterior TOML format stability (FR-P4-004) requires a committed golden file. Regenerating it is a deliberate human step, not an implicit fixup.
- Python parity tests — need a one-time export of the ReSearch prototype's output on the Gate-A fixture. That export is checked in as test data under `crates/specere-filter/tests/fixtures/gate_a/`.

## 3. Sequence + dependency graph

```
#40 (PerSpecHMM scaffold)  ──►  #41 (FactorGraphBP + loader)  ──►  #42 (RBPF + Gate-A parity)  ──►  #43 (CLI)
```

**Strictly sequential.** #41 depends on the HMM's per-spec posterior being well-formed; #42 depends on BP for the non-RBPF paths; #43 depends on all three filters being callable.

## 4. Per-sub-issue recipe

Same 20-step recipe as Phase 2 + 3. Notable deltas:

- **TDD red on numerics**: start each filter with a hand-computed 2- or 3-event posterior as the first test. Make the assertion exact (within `1e-9`). That single test gates the entire implementation.
- **Golden-file generation protocol**: for FR-P4-004's posterior TOML lock-in, the first green run writes the golden file; subsequent runs must byte-match. Regeneration requires a deliberate `cargo test -- --ignored regen_golden` escape hatch.
- **Prototype parity**: before implementing an RBPF step, export the prototype's canonical output on a fixed seed + fixed trace once. Don't iterate against a live prototype — iterate against the frozen export.

## 5. Re-planning triggers (refined from Phase 3)

Trigger a pause + re-plan on any of these:

- **Test-count deviation**: > 1.5× or < 0.5× estimate. *Phase 3 tripped 1.68×; expected for numeric code with parity + determinism tests — hold fire unless > 2×.*
- **Scope growth**: sub-issue PR exceeds **500 LoC** of new non-test code (tighter than Phase 3's 600 — numeric code shouldn't balloon).
- **CI retries**: > 3 same-PR retries.
- **Numerical divergence**: filter output deviates from prototype by > 2 pp on Gate-A. Don't fudge tolerances — investigate.
- **Golden file unstable**: posterior TOML byte-drift on a clean re-run is an FR-P4-004 violation; pause and root-cause before landing.
- **Cycle-detection corner case**: if the coupling-graph loader finds a real-repo graph with cycles, that's a data issue not a code issue — escalate to adjudication.
- **Review-queue drain surfaces novel item**: unchanged.

## 6. Escalation-to-user triggers

Same five as Phase 2/3: same-PR CI 3× consecutive fails, required credential, breaking downstream contract, spec-level disagreement, user interrupt. Phase-4-specific additions:

- **Prototype numerics changed**: if the ReSearch prototype's Gate-A fixture output has moved since last export, the human decides whether we re-anchor or keep the old export.
- **Cycles in sensor-map.toml**: if loader rejects a graph the human intended to author, that's an adjudication call about whether coupling should tolerate cycles (RBPF route) or the sensor-map itself needs refactor.
- **Throughput regression**: `< 1000 events/s` (FR-P4-005) on CI hardware — flag the profile rather than dropping the threshold.

## 7. Phase 4 exit criteria

Phase 4 closes when **all** of:

- [ ] #40, #41, #42, #43 merged to `main`; parent #39 closed.
- [ ] `cargo test --workspace --all-targets` ≥ 125 (projection: ~130 — 101 today + ~29 new).
- [ ] `specere filter run` produces `.specere/posterior.toml` on a dogfood repo (ReSearch); `specere filter status` renders a plausible entropy-sorted table.
- [ ] Gate-A parity within 2 pp of the Python prototype on the same event stream.
- [ ] `docs/upcoming.md` shows `phase-4-filter-engine` under `## Recently closed`; Phase 5 (motion-model calibration) becomes priority 1.
- [ ] `README.md` phase-status table marks Phase 4 ✅ Shipped.
- [ ] This plan moves to `docs/history/phase4-execution-plan.md` at close.
- [ ] Optional: cut v0.5.0 release — first release with a filter engine (per `docs/specere_v1.md §5 Phase 4` Release line).

## 8. Estimates

Per-sub-issue sizing, calibrated against Phase 3's delivery (32 tests / ~800 impl LoC / 4 CI retries / high-risk async):

| Issue | Est. LoC (impl) | Est. LoC (tests) | Est. tests | CI retries | Risk |
|---|---|---|---|---|---|
| #40 PerSpecHMM scaffold | 220 | 180 | 5 | 0 | low — pure numeric, hand-computed tests |
| #41 FactorGraphBP + loader | 300 | 280 | 7 | 1 | med — convergence corner cases + cycle detection |
| #42 RBPF + Gate-A parity | 260 | 260 | 6 | 2 | **high** — stochastic sampling + prototype parity |
| #43 filter CLI | 200 | 240 | 6 | 1 | med — atomic posterior writes + throughput test |
| **Total** | **~980** | **~960** | **~24** | **~4** | |

Test cushion: ~29 tests projected (24 core + ~5 determinism/golden). Phase 3's 1.68× overshoot means plan for 35–45 actual.

## 9. Deferred to Phase 5

- Motion-model calibration from git history (`specere calibrate from-git`).
- Per-spec motion parameters fit from historical (diff, test-delta) pairs.
- Escape valve routing hints from coupling-graph topology analysis.

## 10. Living document

Updated in place on re-planning; moves to `docs/history/phase4-execution-plan.md` at Phase 4 close. Same pattern as Phase 2/3.
