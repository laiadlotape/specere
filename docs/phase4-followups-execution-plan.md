# Phase-4-follow-ups execution plan — FR-P4-002 + FR-P4-005 + manual test pass + v0.4.0 release

**Status.** Drafted 2026-04-18 post-Phase-4-main-track close (main at `62aee60`). Governs the final Phase 4 follow-up PR plus a structured manual-test pass of existing behaviour before cutting `v0.4.0`.
**Authority.** `docs/contributing-via-issues.md` (pipeline) · `docs/specere_v1.md §5 Phase 4` (FR-P4-002, FR-P4-005) · `.specify/memory/constitution.md` · core_theory §4 (human gates — divergence adjudication).

## 1. Context

Phase 4 main track landed all four algorithmic sub-issues (#40 HMM / #41 BP / #42 RBPF / #43 CLI) but deliberately deferred two FRs:

- **FR-P4-002** — < 2 pp tail-MAP accuracy vs Python prototype on Gate-A. Requires a **one-time prototype fixture export** because random-seed divergence between NumPy and Rust's `StdRng` makes live parity impossible.
- **FR-P4-005** — ≥ 1000 events/s throughput on laptop hardware. Requires a dedicated benchmark test that's heavy enough to be meaningful but gated (`#[ignore]`) so CI doesn't pay the cost every run.

Beyond the two FRs, Phase 4 is the first numerically-nontrivial slice of the codebase — enough surface area that automated tests won't catch every corner case. Before releasing `v0.4.0` we add a **structured manual test charter**: drive the binary against scenarios automated tests don't cover, record each test's outcome in a traceability file, and fix anything broken.

## 2. Deliverables

### 2.1 Algorithm alignment to prototype — FR-P4-002 prerequisite

Before parity can work, the Rust `Motion` matrices + `DefaultTestSensor` must match `prototype/mini_specs/world.py` + `sensors.py` verbatim. The prior Rust values were reasonable-looking placeholders; parity means identical priors.

- `crates/specere-filter/src/motion.rs` — overwrite `prototype_defaults()` with the prototype's `transition_good / transition_bad / transition_identity_leak`.
- `crates/specere-filter/src/drive.rs` — overwrite `DefaultTestSensor` with `alpha_sat=0.92, alpha_vio=0.90, alpha_unk=0.55`.
- Update the hand-computed test in `tests/perspec_hmm_hand_computed.rs` to the new expected values.

### 2.2 Gate-A parity fixture export

New script `scripts/export_gate_a_posterior.py` — imports the prototype and produces a committed TOML fixture at `crates/specere-filter/tests/fixtures/gate_a/` containing:

- `seed` + `steps` metadata.
- Observable trace: every `write` event's `files_touched` and every `test` event's `(spec_id, outcome)`. Reads are excluded (prototype uses a Poisson-based ReadSensor that we're not porting in v1 — FR-P4-002's MAP-accuracy is insensitive to reads in the Gate-A configuration).
- Final belief matrix from the Python prototype's `PerSpecHMM.all_marginals()` after the trace.

This is executed **once** and committed. Regenerating requires a deliberate opt-in rebuild.

### 2.3 Parity test

New `crates/specere-filter/tests/gate_a_parity.rs` — load fixture, replay trace through Rust `PerSpecHMM`, assert per-cell absolute difference `< 0.02` on every spec row vs the fixture's expected beliefs.

### 2.4 Throughput test — FR-P4-005

New `crates/specere/tests/fr_p4_005_throughput.rs` — `#[ignore]`-gated integration test. Generates a 10k-event JSONL store, drives `specere filter run`, asserts `duration < 10s` (i.e. ≥ 1000 events/s). `#[ignore]` keeps CI fast; `cargo test --test fr_p4_005_throughput -- --ignored` runs it locally.

### 2.5 Manual test charter + traceability

New `docs/phase4-manual-test-report.md` — a traceability-style report. For each manual test case:

- A unique ID (`M-01`, `M-02`, …).
- One-sentence description of what was tried.
- Linked FR / spec / issue.
- Steps taken (one or two command lines).
- Observed outcome.
- Pass/fail verdict + any code change needed to make it pass.

The charter covers **at least** the following corner-case families (see §3 for the enumerated list):

1. Malformed / missing `.specere/sensor-map.toml`
2. Missing `[specs]` vs missing `[coupling]`
3. Unknown event kinds, missing attrs, empty attrs
4. Unknown `outcome` values (flat-sensor fallback)
5. Unicode and long spec IDs
6. `--sort` edge cases: malformed field, unknown field, missing direction
7. `--format` unknown value
8. Running `filter run` with zero specs and zero events
9. Running `filter status` against a truncated / corrupted posterior
10. Concurrent `filter run` (two processes at once)
11. `posterior.toml.tmp` already present from a prior crash
12. Event with `files_touched` but empty `paths=`
13. Event with `spec_id=` pointing at a spec not in `[specs]`
14. Cursor comparison with lexicographically out-of-order RFC3339
15. The binary itself: `specere --help`, `specere filter --help`, both CLI trees
16. The `run` → `status` round-trip with `--posterior` + `--sensor-map` overrides

### 2.6 v0.4.0 release — gated on user approval of the final report

Once §2.1–§2.5 are done AND the user approves the manual-test report via an interactive questionnaire, cut the release:

- Bump `workspace.package.version` to `0.4.0` in root `Cargo.toml`.
- Move the CHANGELOG `## [Unreleased]` section to `## [0.4.0] — 2026-04-18`.
- New `## [Unreleased]` stub.
- Tag `v0.4.0` locally.
- Push tag → `release.yml` fires → 16 artifacts land.

## 3. Manual-test charter (expanded)

The charter's **discipline**: every scenario gets its own entry in the traceability file. No silent "I tried it and it worked" — if I tried it, it's in the file. No silent "I tried it and it didn't work but I fixed it" — the fix gets its own commit and a cross-reference from the traceability entry.

| ID | Scenario | Linked FR / file | Expected outcome |
|---|---|---|---|
| M-01 | `specere --help` — top-level help | CLI | Shows all subcommands incl. `filter` |
| M-02 | `specere filter --help` + `run --help` + `status --help` | CLI | All flags documented |
| M-03 | `filter run` on an uninitialised repo (no `.specere/`) | `specs.rs` | Actionable error naming sensor-map.toml |
| M-04 | `filter run` with malformed TOML in sensor-map.toml | `specs.rs` | Parse error with context (path) |
| M-05 | `filter run` with `[specs]` present but empty | `specs.rs` | "[specs] section empty or missing" |
| M-06 | `filter run` with a valid `[specs]` but no event store | driver | "no new events since start" |
| M-07 | `filter status` with no posterior | CLI | "no posterior yet — run `specere filter run` first" |
| M-08 | Event with `event_kind=test_outcome` but no `spec_id` | driver | Skipped, counted in `skipped` |
| M-09 | Event with `event_kind=test_outcome`, unknown `outcome=` | driver + DefaultTestSensor | Processed; flat-likelihood → belief unchanged |
| M-10 | Event with `event_kind=files_touched`, empty `paths=` | driver | `parse_paths` yields empty → identity-leak only |
| M-11 | Event with `spec_id=` not in `[specs]` | driver + HMM | Skipped, posterior untouched |
| M-12 | `filter status --sort garbage` (no comma) | `sort_entries` | Actionable error |
| M-13 | `filter status --sort foo,desc` (unknown field) | `sort_entries` | Actionable error listing valid fields |
| M-14 | `filter status --sort spec_id,asc` — valid but undocumented-in-help field | `sort_entries` | Works; entries alphabetically sorted |
| M-15 | `filter status --format yaml` — unknown format | `run_filter_status` | Falls through to table (current behaviour) |
| M-16 | Very long spec ID (200 chars) | schema | Accepted; table wraps or truncates gracefully |
| M-17 | Unicode spec ID (emoji, CJK) | schema | Accepted; sort stable |
| M-18 | `filter run --posterior /tmp/foo.toml` override | CLI | Writes to `/tmp/foo.toml` atomically |
| M-19 | `filter run` twice in quick succession (simulated concurrency) | atomic write | No partial file; no silent data loss |
| M-20 | Pre-existing `posterior.toml.tmp` leftover from a prior crash | atomic write | Gets overwritten on next successful write |
| M-21 | Cursor with timezone-normalised RFC3339 vs bare `Z` | driver cursor comparison | Lexicographic comparison behaves correctly on same-TZ values (current contract) |
| M-22 | `filter status --format json` output is valid JSON | CLI | `jq` round-trip works |
| M-23 | `filter run` on repo with `[coupling]` edges — BP path | CLI branch | BP is invoked; posterior reflects coupling influence |
| M-24 | `filter run` on repo with `[coupling]` referencing unknown spec | BP constructor | Unknown edges silently dropped (documented behaviour) |

If any of M-01 through M-24 surfaces a genuine bug, it gets fixed in a dedicated commit on the same branch, and the traceability entry records the fix commit SHA.

## 4. Sequence

```
§2.1 align Motion + Sensor  →  §2.2 export fixture  →  §2.3 parity test  →  §2.4 throughput test
                                                                        →  §2.5 manual test charter
                                                                                        ↓
                                                                               questionnaire review
                                                                                        ↓
                                                                                  §2.6 release
```

Algorithmic alignment (§2.1) lands first because it's the precondition for parity. Parity export (§2.2) must succeed before the parity test (§2.3). Throughput (§2.4) and manual charter (§2.5) are independent and can proceed in parallel after §2.1 builds.

## 5. Re-planning triggers

- **Parity check fails wider than 2 pp.** Pause. Either the trace-replay contract is wrong, or the Rust `update_test` log-space Bayes has a subtle bug. Investigate before loosening the tolerance.
- **Throughput below 1000 events/s.** Profile before optimising. Likely suspects: log-sum-exp hot loop, TOML serialisation, SQLite query overhead per event.
- **Any manual test reveals a behaviour I'd call "wrong."** Fix in-branch. If the fix needs > 50 LoC of new logic, carve it out as a separate PR on a new branch — this branch should stay close to "parity + benchmark + report".

## 6. Exit criteria

Phase-4-follow-ups closes when **all** of:

- [ ] Motion + Sensor aligned with prototype; hand-computed test updated; all automated tests green.
- [ ] Gate-A fixture committed at `crates/specere-filter/tests/fixtures/gate_a/posterior.toml`.
- [ ] `tests/gate_a_parity.rs` passes with every belief cell < 0.02 off prototype.
- [ ] `tests/fr_p4_005_throughput.rs --ignored` passes under 10 s on a typical laptop (profile included in the traceability file).
- [ ] `docs/phase4-manual-test-report.md` has 24 entries, each with an outcome. Any entries flagged failing also reference the fix commit.
- [ ] All CI gates green cross-platform on the follow-ups PR.
- [ ] User has run the interactive questionnaire and approved.
- [ ] `v0.4.0` tag pushed; release artifacts land.

## 7. Follow-ups after v0.4.0

Nothing for this slice — Phase 5 (motion-model calibration) becomes the next queue head.
