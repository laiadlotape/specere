# Harness Manager execution plan (v1.2.0 + v2.0.0)

**Status.** Scope + direction approved via §10 questionnaire on `docs/proposals/v3-harness-manager.md` (2026-04-20).

**v1.2.0 feature-complete on main (2026-04-20):** S1–S6 all shipped (PRs #94, #96, #97, #98, #99) + OTel semconv formalised (PR #100) + ratatui TUI companion shipped (PR #101). 358 workspace tests green. Release tag not yet cut — deferred per user's "mega-release packaging" choice so v1.2.0 ships with any final GUI-API endpoints or polish items.

**Release shape (decided + current status).**

| Release | Contents | Size | Heavy deps | Status |
|---|---|---|---|---|
| **v1.2.0** | S1–S6 (scan + provenance + history + coverage + flakiness + clustering) + semconv + TUI | ~2400 LoC + ~600 LoC TUI + ~550 LoC semconv/event-emit | `cargo-llvm-cov` subprocess (opt-in), `ratatui`, `crossterm`, `syn`, `sha2`, `hex` | ✅ **Landed on main** |
| **v2.0.0** | Tauri v2 + Sigma.js GUI; 6-screen MVP | ~frontend + ~500 LoC Rust API layer | `@sigma/core`, `graphology`, `react-flow` via Tauri shell | 📋 Not yet started — substantial frontend scoping |
| **v1.0.6** (post-v2) | Bug-tracker bridge (FR-EQ-010..013) | ~600 LoC | `octocrab`, `gitea-sdk` | ⏸ Queued |
| **v1.1.0** (post-v2) | LLM adversary (FR-EQ-020..024) | ~800 LoC | paid LLM spend | ⏸ Queued |

**Policy decisions from questionnaire.**
- **Coverage mode**: feature-flagged always-on (opt-in *per repo* via `[specere.coverage] enabled = true` in sensor-map). When flagged on, S4 runs llvm-cov alongside nextest on every test invocation; when flagged off (default), S4's CLI still works on-demand.
- **Flakiness cold start**: ship S5 now; the per-run matrix populates from day 1. Reports surface the data we have and flag `insufficient history` under `n_runs < 50` (same pattern as FR-EQ-004).
- **OTel semconv**: use `specere.harness.*` prefix, document in-repo as *Supplementary Semantic Convention*, propose upstream after 2+ consumers exist.
- **GUI timing**: parallel track from S1 onwards; MVP ships as v2.0.0 alongside S6.
- **TUI**: parallel track; share REST endpoints with GUI. Ships whenever ready.
- **Headline story**: dual — **(1) provenance graph** (S2, demoable early) + **(2) "used-likely-along-with" coupling detection** (S4+S5 deliver it, S6 clusters it). v1.2.0 ships both.
- **Post-v2 queue**: v1.0.6 bug-tracker bridge next.

---

## 1. FR numbering map

| FR range | Slice | Ships in | Status |
|---|---|---|---|
| FR-HM-001..004 | S1 — enumerate + categorise | v1.2.0 | ✅ merged PR #94 |
| FR-HM-010..012 | S2 — provenance join | v1.2.0 | ✅ merged PR #94 |
| FR-HM-020..022 | S3 — git version + co-modification | v1.2.0 | ✅ merged PR #96 |
| FR-HM-030..033 | S4 — coverage co-execution | v1.2.0 | ✅ merged PR #97 |
| FR-HM-040..043 | S5 — co-failure + flakiness | v1.2.0 | ✅ merged PR #98 |
| FR-HM-050..052 | S6 — cluster + filter wiring | v1.2.0 | ✅ merged PR #99 |
| FR-HM-060..061 | `specere.harness.*` OTel semconv | v1.2.0 (cross-cutting) | ✅ merged PR #100 |
| FR-HM-070..072 | TUI companion | parallel | ✅ merged PR #101 |
| FR-HM-080..085 | GUI v2.0.0 (6 screens) | parallel, v2.0.0 | ⏸ not yet started |

---

## 2. Slice S1 — enumerate + categorise (FR-HM-001..004)

**Goal.** Every file under `src/`, `tests/`, `benches/`, `fuzz/`, `.github/workflows/`, `xtask/`, `justfile` is classified into one of nine harness categories (or explicitly `production`), with `#[test]`/`#[bench]`/`proptest!{}`/`fuzz_target!`/`insta::assert_*!` name extraction, and rendered as a harness-graph node table.

**Deliverables.**
- New CLI verb: `specere harness scan [--format toml|json]`.
- Output: `.specere/harness-graph.toml` — sorted-by-ULID node table (TOML, matches manifest convention).
- Output: `.specere/harness-graph.sqlite` — edge table (direct_use edges only in S1).
- Parser for `rustc --emit=dep-info` `.d` files (read from `target/debug/deps/*.d`).

**FR list.**

### FR-HM-001 — `specere harness scan` enumerates all nine categories
- **AC.** Walks `src/`, `tests/`, `benches/`, `fuzz/`, `xtask/`, `.github/workflows/`, and any file matching `justfile` or `*.just`. Classifies each `.rs` file by syn AST walker + path conventions; classifies `.yml`/`.yaml` under `.github/workflows/` as `workflow`; classifies `justfile` / `xtask/**` as `workflow`.
- **Test plan.** 9 unit tests (one per category) using fixture repos; 2 integration tests (full-repo scan; scan with `--format json`).

### FR-HM-002 — Test-name extraction per harness file
- **AC.** For each categorised file, extract `#[test]` / `#[tokio::test]` / `#[rstest]` function names; for property files, extract `proptest!{}` identifiers; for fuzz, extract `fuzz_target!` args; for benches, extract `criterion_group!`/`#[bench]` names; for snapshots, extract `insta::assert_*!` macro call sites.
- **Test plan.** 5 unit tests (one per harness class with test-name idioms); 1 integration test verifying the node table lists all expected test names.

### FR-HM-003 — Direct-edge extraction from `rustc --emit=dep-info`
- **AC.** Parses `target/debug/deps/*.d` files (Make-style dependency lists), filters to repo-relative paths, emits `direct_use` edges into the graph SQLite store. No rebuild required if target dir is fresh.
- **Test plan.** 3 unit tests on `.d` parser (simple, multi-line-continuation, include_str!-derived edges); 1 integration test end-to-end against a fixture crate.

### FR-HM-004 — `HarnessFile` node model + TOML serialisation
- **AC.** The `HarnessFile` struct (see `docs/proposals/v3-harness-manager.md` §6.1) serialises stably; round-trip through `harness-graph.toml` produces bit-identical output.
- **Test plan.** 2 unit tests (round-trip; ordering determinism); `harness-graph.toml` output is byte-identical on repeated scans (pin via `sha256` assertion in test).

**Estimated LoC.** ~500. **Heavy deps.** None new (syn+walkdir already added in FR-EQ-003).

---

## 3. Slice S2 — provenance join (FR-HM-010..012)

**Goal.** Link each harness node to the `/speckit-*` verb that created it, via existing workflow_span events.

### FR-HM-010 — Workflow-span schema augmentation
- **AC.** `.specify/extensions.yml` hooks emit `files_created` + `files_modified` in workflow spans (they already do for `after_implement`; extend to `after_specify`, `after_plan`, `after_tasks`, `after_analyze`, `after_checklist`, `after_clarify` — all 7 verbs). Attribute: comma-separated repo-rel paths.
- **Test plan.** 7 integration tests (one per verb) verifying `files_created` attribute appears in events.jsonl after each hook fires.

### FR-HM-011 — `specere harness provenance` subcommand
- **AC.** Reads `.specere/events.jsonl`, matches `files_created`/`files_modified` to harness-graph nodes, writes `created_by` + `modified_by` edges. Populates `provenance.{creator_agent, creator_verb, creator_spec, creator_commit, creator_human, created_at}` on each node. Where agent provenance is missing (pre-harness-manager files), falls back to `git log --follow --diff-filter=A`.
- **Test plan.** 3 integration tests (agent-created file, human-created file, divergent file where agent created but human patched).

### FR-HM-012 — Divergence detection (agent vs. human edits)
- **AC.** When a file has `created_by=WorkflowSpan` but `authored_by=Human` lines > 50%, emit `provenance_divergence_detected` advisory event (INFO severity, per v1 questionnaire).
- **Test plan.** 2 integration tests (no divergence; heavy divergence).

**Estimated LoC.** ~300. **Heavy deps.** None.

---

## 4. Slice S3 — git version + co-modification (FR-HM-020..022)

**Goal.** Per-node churn/age/authors metrics + PPMI-on-commit-matrix edges. Reuses `specere calibrate from-git` walker.

### FR-HM-020 — `git log` churn/age walker (Rust-native, no code-maat subprocess)
- **AC.** Reimplement the subset of code-maat analyses needed: `entity-churn`, `age`, `main-dev`, `coupling`. All four emit metrics into the node table. Git log parsed via the existing `git log --name-only --numstat --follow --pretty=format:...` machinery in `calibrate/mod.rs`.
- **Test plan.** 4 unit tests (one per analysis); 1 integration test on a fixture git repo with known commit history.

### FR-HM-021 — PPMI on commit matrix → `comod` edges
- **AC.** Same PPMI formula as in `calibrate from-git` (but applied to harness-file pairs, not spec pairs). Min-count gate: `n_joint_commits ≥ 3`. Writes `comod` edges to SQLite.
- **Test plan.** 3 unit tests (strong coupling; below-threshold pair; single-commit spurious-coupling rejection).

### FR-HM-022 — Hotspot scoring (churn × complexity)
- **AC.** Per-node `hotspot_score = churn_rate × cyclomatic_complexity` (complexity via `syn` function-nesting walker). Rendered as top-N list in the scan output. Surfaces test-rot candidates.
- **Test plan.** 2 unit tests + 1 integration test on a fixture with known hotspot.

**Estimated LoC.** ~400. **Heavy deps.** None (reuses existing git log pipeline).

---

## 5. Slice S4 — coverage co-execution (FR-HM-030..033)

**Goal.** Per-test coverage bitvectors → Jaccard → `cov_cooccur` edges. Feature-flagged always-on via `[specere.coverage] enabled = true`.

### FR-HM-030 — `cargo-llvm-cov` driver
- **AC.** Subprocess `cargo llvm-cov nextest --no-report --profraw-only` with per-test profraw output enabled. Collects one `.profraw` file per test binary × test name. Handles `--features` + `--target` permutations (keys each coverage vector by `(test, features, target)`).
- **Test plan.** 2 integration tests (single-target run; feature-flag permutation). Tests gated behind `cfg(feature = "cov-integration")` so CI without llvm tools still passes.

### FR-HM-031 — `llvm-profdata` merge + bitvector extraction
- **AC.** Subprocess `llvm-profdata merge` + `llvm-cov export --format=lcov --instr-profile=...`. Parses LCOV to extract `covered_line_bitvector` per test. Stores bitvectors in SQLite keyed by `(test, features, target)`.
- **Test plan.** 3 unit tests on LCOV parser; 1 integration test that bitvector round-trips.

### FR-HM-032 — Jaccard similarity → `cov_cooccur` edges
- **AC.** For every pair `(a, b)` with non-empty bitvectors, compute `J_cov = |C(a)∩C(b)| / |C(a)∪C(b)|`. Emit `cov_cooccur` edge with weight=J_cov when J_cov > 0.1 (pruning threshold). Runtime scales O(n² · avg_vector_len); use roaring bitmaps for large repos.
- **Test plan.** 3 unit tests (disjoint=0; identical=1; partial overlap).

### FR-HM-033 — `[specere.coverage] enabled` sensor-map flag
- **AC.** When `true`, `specere harness scan` automatically invokes S4 at the end of S3. When `false` (default), `specere harness coverage` is only run on-demand.
- **Test plan.** 2 integration tests (flag on; flag off).

**Estimated LoC.** ~600. **Heavy deps.** `cargo-llvm-cov` + `llvm-profdata` subprocess; `roaring` crate for compact bitvectors (optional — v1.2.0 uses `BitVec` if repo <1000 tests).

---

## 6. Slice S5 — co-failure + flakiness (FR-HM-040..043)

**Goal.** Per-run JUnit matrices → PPMI-on-failures edges, with DeFlaker-style coverage filter + Meta prob-flakiness gating.

### FR-HM-040 — Per-run JUnit ingestion
- **AC.** New hook in `.specify/extensions.yml`: `after_test_run` reads the nextest-emitted JUnit XML at `target/nextest/*/junit.xml`, persists per-test pass/fail outcome as event-store rows.
- **Test plan.** 2 integration tests (first run; accumulating runs).

### FR-HM-041 — PPMI on `test × run` matrix → `cofail` edges
- **AC.** Accumulates matrix in SQLite; `cofail` edges emitted only when `n_joint_failures ≥ 5` (Hoeffding gate). Edges are signed: `PPMI_fail > 0` means co-coupled, negative truncated to 0.
- **Test plan.** 3 unit tests (coupled pair; anti-correlated pair; below-threshold pair).

### FR-HM-042 — DeFlaker-style flakiness filter
- **AC.** For each failing test, check whether its coverage bitvector (from S4) intersects the diff of the failing commit. If no intersection → mark `probable_flake`. Discount flaky tests' contribution to `cofail` PPMI by `(1 - P_flake)`.
- **Test plan.** 3 unit tests (genuine fail; pure flake; ambiguous).

### FR-HM-043 — Meta probabilistic-flakiness score
- **AC.** Per-test `P(fail | good_state)` estimated from recent history. Stored on `HarnessFile.flakiness_score`. Surfaced in `specere harness flaky` report with `insufficient history` warning when `n_runs < 50`.
- **Test plan.** 2 unit tests (stable test; known-flake).

**Estimated LoC.** ~500. **Heavy deps.** `junit-parser` crate (~200 LoC alternative, pure-Rust). Requires S4 for DeFlaker filter (dependency gate).

---

## 7. Slice S6 — cluster + filter wiring (FR-HM-050..052)

**Goal.** Run Leiden on the combined edge graph; emit `[harness_cluster]` into sensor-map; wire cluster belief into the existing BBN.

### FR-HM-050 — Leiden community detection
- **AC.** Constructs weighted undirected graph G over harness nodes with composite weight = `0.4·J_cov + 0.3·σ(PPMI_fail) + 0.2·σ(PPMI_mod) + 0.1·σ(w_indirect)` (sub-scores stored raw in SQLite; composite computed on demand). Runs `graphina::leiden` (or `single-clustering::leiden`) with deterministic seed (read from sensor-map `[harness_cluster] seed`). Assigns `cluster_id` to every node.
- **Test plan.** 3 unit tests (two tightly coupled clusters produce two clusters; disconnected nodes produce singletons; empty graph yields empty output).

### FR-HM-051 — Sensor-map `[harness_cluster]` export
- **AC.** Writes `.specere/sensor-map.toml`'s `[harness_cluster]` section:
  ```toml
  [harness_cluster]
  algo = "leiden"
  seed = 42
  auto_emit = true

  [harness_cluster.clusters."C01"]
  members = ["tests/fr_eq_003_lint_tests.rs", ...]
  centroid_spec = "FR-EQ-003"
  ```
- **Test plan.** 2 integration tests (new run writes cluster table; re-run with same seed produces byte-identical output).

### FR-HM-052 — Cluster-belief integration with BBN
- **AC.** Extend `PerSpecTestSensor` to accept a `cluster_id` → `Calibration` map. When a harness file in cluster C fires a test outcome, the calibration for the *cluster* is used as a prior on the per-spec calibration. Fallback: prototype alphas when cluster data absent.
- **Test plan.** 2 unit tests (cluster-calibration applied; missing-cluster fallback) + 1 integration test (two specs share a cluster → their posteriors move together).

**Estimated LoC.** ~400. **Heavy deps.** `graphina` (Louvain+Leiden, pure Rust) OR `single-clustering` (Leiden alone) — pick based on v1.2.0 implementation audit. Wire into existing BBN via the FR-EQ-002 `Calibration` struct.

---

## 8. Cross-cutting — `specere.harness.*` OTel semconv (FR-HM-060..061)

### FR-HM-060 — Attribute set definition
- **AC.** Document the following supplementary attributes in `docs/otel-specere-semconv.md`:
  - `specere.harness.kind` — one of nine categories
  - `specere.harness.file` — repo-rel path
  - `specere.harness.test_name` — extracted test name
  - `specere.harness.cluster_id` — post-S6 cluster assignment
  - `specere.harness.coverage_digest` — blake3 of production lines
  - `specere.harness.provenance.speckit_unit` — creator `/speckit-*` verb
  - `specere.harness.flakiness_score` — [0, 1]
- **Test plan.** 1 contract test in `crates/specere-telemetry/tests/` that every emitted harness-event populates these attributes correctly.

### FR-HM-061 — All S1–S6 subcommands emit these attributes
- **AC.** Every new event kind (`harness_scan`, `harness_provenance`, `harness_history`, `harness_coverage`, `harness_flaky`, `harness_cluster`) carries the supplementary attribute set. Additive-event-schema rule (FR-EQ-007) enforced: unknown kinds still parse, cursor still advances.
- **Test plan.** 6 integration tests (one per subcommand) verifying attribute presence.

**Estimated LoC.** ~100 (documentation + attribute plumbing in existing event_store). **Heavy deps.** None.

---

## 9. TUI parallel track (FR-HM-070..072)

### FR-HM-070 — `specere harness tui` main shell
- **AC.** ratatui shell with sidebar (file tree + cluster list) + main pane (node detail); reads from SQLite + REST endpoints exposed by `specere serve http`. Entrypoint: `specere harness tui`.
- **Test plan.** 2 unit tests (nav keybindings; layout rendering).

### FR-HM-071 — Relation inspector mini-view
- **AC.** Press Enter on a node → modal listing all incoming/outgoing edges grouped by type. Press Esc to return. Keyboard-first (j/k navigation).
- **Test plan.** 2 integration tests (render; select-and-return).

### FR-HM-072 — Event timeline TUI
- **AC.** Bottom pane shows live OTel span stream filtered by verb. Auto-refreshes every 1s.
- **Test plan.** 1 integration test (mock stream populates pane).

**Estimated LoC.** ~600. **Heavy deps.** `ratatui`, `crossterm`.

---

## 10. GUI parallel track v2.0.0 (FR-HM-080..085)

Six screens in a Tauri v2 shell. Frontend: Sigma.js + Graphology + React Flow (per research report §GUI).

### FR-HM-080 — Tauri shell + Axum API extension
- **AC.** `specere gui` launches a Tauri shell hosting the existing `specere serve http` Axum server as its backend. New endpoints:
  - `GET /api/v1/harness/graph?format=graphology`
  - `GET /api/v1/harness/files/{path}/relations`
  - `GET /api/v1/harness/clusters?algo=leiden`
  - `GET /api/v1/specs/{id}/harness`
  - `GET /api/v1/events/ws` (WebSocket for live updates)
- **Test plan.** REST contract tests in `crates/specere/tests/fr_hm_080_*.rs`.

### FR-HM-081 — Harness Graph screen (Sigma.js + FA2-in-worker)
- **AC.** Renders `/api/v1/harness/graph` via Sigma.js WebGL renderer; force-directed layout via Graphology's `graphology-layout-forceatlas2` in a WebWorker. Colour by cluster; size by posterior entropy.
- **Test plan.** Playwright E2E test renders 100-node fixture graph.

### FR-HM-082 — Spec Dashboard
- **AC.** Per-spec UNK/SAT/VIO stacked-bar simplex + timeline sparkline. Reads `/api/v1/specs` + `/api/v1/specs/{id}/timeline`.
- **Test plan.** Component test with fixture data.

### FR-HM-083 — Review Queue (markdown-backed Kanban)
- **AC.** Three-column Kanban parsed from `.specere/review-queue.md`; drag-and-drop state changes write back to markdown preserving plaintext source-of-truth. Keyboard-first: J/K navigate, E extend, I ignore, A allowlist, D adjudicate.
- **Test plan.** E2E test verifies markdown round-trip on state change.

### FR-HM-084 — Event Timeline (Perfetto-style waterfall)
- **AC.** Chronological span viewer with row-per-trace-ID. Click span → attribute panel.
- **Test plan.** E2E test with fixture OTel trace.

### FR-HM-085 — Relation Inspector (deep-link modal)
- **AC.** Click a node anywhere → split-pane opens with 2-hop neighbourhood (React Flow) + tabbed detail pane (Relations, Specs, Spans, Review items). Deep-linkable via `specere://file/<path>` custom scheme.
- **Test plan.** E2E test verifies deep-link opens correct node.

**Estimated LoC.** ~500 Rust (API layer) + ~3000 JS/TS (frontend). **Heavy deps (frontend)**: `@sigma/core`, `graphology`, `react-flow`, `@tauri-apps/api`.

---

## 11. Dependency graph

```
S1 (FR-HM-001..004) → S2 (FR-HM-010..012) → S3 (FR-HM-020..022) → S4 (FR-HM-030..033) → S5 (FR-HM-040..043) → S6 (FR-HM-050..052)
                                                                       ↘ (S5 needs S4 for DeFlaker)
FR-HM-060..061 cross-cuts all six.
TUI (FR-HM-070..072) can begin after S3.
GUI (FR-HM-080..085) API layer can begin at S1; screens populate as slices land.
```

S1 → S2 → S3 is hard-sequential (S2 needs S1 nodes; S3 needs S1 nodes but is independent of S2).
S4 is independent of S2/S3 but most valuable after them.
S5 depends on S4 (DeFlaker coverage join).
S6 needs all edge types in place.

---

## 12. Exit criteria for v1.2.0

- All 6 slices landed on main with integration tests.
- `specere harness scan` on ReSearch (dogfood target) produces a classified inventory of ≥ 50 harness files within 60 seconds.
- `specere harness provenance` attributes ≥ 80% of existing harness files to a `/speckit-*` verb (self-dogfood test).
- `specere harness flaky` on ≥50-run CI history from SpecERE itself produces at least one correctly-identified flake pair.
- Cluster output at `.specere/sensor-map.toml` is byte-identical on repeated runs (same seed).
- Full workspace `cargo test` green; clippy clean.
- CHANGELOG entry + README updated.

## 13. Exit criteria for v2.0.0 (GUI)

- Tauri shell launches on linux/macOS/windows.
- All 6 screens render fixture data.
- 10k-node graph test renders at ≥30fps (Sigma.js + FA2 worker benchmark).
- Markdown round-trip on review-queue state change preserves byte-identity of untouched entries.
- REST endpoints covered by contract tests.

---

## 14. Re-planning triggers

If any of these occur, pause implementation and revisit the plan:
- `cargo-llvm-cov` integration cost exceeds 3× test-suite wall-clock (not 2-3× as research estimated).
- Leiden clustering produces ≥50% singleton clusters on ReSearch (signal-to-noise too low).
- Agent-provenance attribution in S2 drops below 30% coverage (workflow-span hooks missing data).
- GUI MVP blocks on a missing OSS library (Sigma.js perf cliff, React Flow regression).
- User requests scope change.

## 15. Not in this plan (defer)

- SLSA/in-toto cryptographic provenance — overkill for use case.
- FlakeFlagger-class ML-based flakiness prediction — requires training pipeline.
- Cross-repo SCIP indexing — only after multi-repo SpecERE users exist.
- Upstream OTel `test.*` semconv RFC — after 2+ consumers, per questionnaire Q7.
- Predictive test selection (Meta-style GBDT) — future phase; would reuse v1.2.0's data pipeline.

---

*End of plan. Next: begin S1 (FR-HM-001..004). First PR: `docs/harness-manager-plan.md` + scaffold `crates/specere/src/harness/mod.rs`.*
