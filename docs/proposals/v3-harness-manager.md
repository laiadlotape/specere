# SpecERE v3 — Harness Manager & Inspector

**Proposal status**: Draft — awaiting go/no-go + prioritisation. **Next action**: user reviews §1 premise + §10 questionnaire, picks a scope, then we cut FR numbers and begin Slice 1.

**Paired document**: `docs/harness-manager-plan.md` (FR-numbered execution plan; to be written after go/no-go).

---

## 1 · Premise — what's missing today

SpecERE already observes an agentic workflow (OTel spans, SpecKit verbs) and maintains a per-spec Bayesian belief over `UNK/SAT/VIO` with evidence-quality calibration (v1.1 work, see `docs/evidence-quality-plan.md`). What it does **not** do is treat the *test surface itself* as a first-class object.

Today, every `#[test]`, every `tests/*.rs`, every `benches/`, every CI YAML is a file path in the sensor-map at best. The user cannot ask:

- "Which tests are *actually testing* this spec — not just declared to?"
- "Which tests co-fail consistently — are they really testing two things or one?"
- "Who/what created this test — a human in issue #42, or `/speckit-implement` on spec `002-phase-1-bugfix`?"
- "This helper fixture is shared by 18 tests across 4 specs — what happens if I edit it?"
- "Our property-based tests under `proptest!` last ran 6 months ago — has the code they exercise changed since?"

A **harness manager** promotes the test surface from an opaque input into a **typed, provenanced, clustered graph** over which SpecERE's belief engine can reason. It is the natural v3 move after v1.0/v1.1's evidence-quality work: the evidence-quality layer fixed "when do we trust a test's verdict"; the harness manager fixes "*which tests do we even have, and how do they relate?*"

---

## 2 · Scope — what is a "harness file"?

Nine categories, each detectable statically (cheap):

| Category | Detection | Example |
|---|---|---|
| `unit` | `src/**/*.rs` with `#[cfg(test)]` + `#[test]` | Inline module tests |
| `integration` | `tests/*.rs` (Cargo convention) | `tests/fr_eq_003_lint_tests.rs` |
| `property` | `proptest!{}` / `#[quickcheck]` / `bolero::check!` | Property tests |
| `fuzz` | `fuzz/fuzz_targets/**` / `libfuzzer_sys::fuzz_target!` | Fuzz corpora |
| `bench` | `benches/**` / `criterion_group!` / `#[bench]` | Criterion benches |
| `snapshot` | Files matching `insta::assert_*!` + `*.snap` | Snapshot tests |
| `golden` | Files named `*.expected.*`, `fixtures/**/*.json` | Golden data |
| `mock` | `mockall!` macros, files named `mock_*.rs` | Mocks |
| `fixture` | `tests/common/**`, `tests/fixtures/**` | Shared helpers |
| `workflow` | `.github/workflows/*.yml`, `.gitlab-ci.yml`, `justfile`, `xtask/**` | CI/orchestration |

---

## 3 · Relation taxonomy — what edges SpecERE should track

Six relation families. Every family has a **precise definition**, a **Rust-ecosystem detection recipe**, and a **noise source**:

### 3.1 Direct linkage (zero-cost, high-signal)
- **Definition**: `a → b` iff `a`'s source text contains a resolvable `use`/`mod`/`include_str!`/macro invocation whose target is `b`.
- **Source**: `rustc --emit=dep-info` produces `.d` files per target on every build — free. Optionally augmented by SCIP (rust-analyzer's cross-reference index) for symbol-level edges.
- **Noise**: near-zero.

### 3.2 Indirect linkage (shared fixture/helper/mock)
- **Definition**: `a ↔ b` iff they share a third node `h` that is not ubiquitous.
- **Source**: graph query `common_neighbors(a, b)` over the direct graph, filtered by an IDF weighting on `h` (helpers used by 90% of tests contribute ~0; helpers used by 3 contribute strongly).
- **Noise**: low, after IDF filter.

### 3.3 Co-execution (coverage footprint)
- **Definition**: `J_cov(a, b) = |C(a) ∩ C(b)| / |C(a) ∪ C(b)|` where `C(t)` is the set of production lines `t` covered.
- **Source**: `cargo llvm-cov --no-report` + per-test profraw files, merged via `llvm-profdata`. ~2-3× test suite overhead; opt-in.
- **Noise**: medium — feature-flag permutations produce different coverage, must key by `(test, features, target)`.

### 3.4 Co-failure (CI history)
- **Definition**: `PPMI_fail(a, b) = max(0, log₂(p(a,b) / (p(a)·p(b))))` over the last N CI runs.
- **Source**: nextest JUnit XML or libtest JSON output, persisted per run in SpecERE's event store.
- **Noise**: **HIGH** unless DeFlaker-style coverage filter + Meta's probabilistic flakiness score are applied first. Flaky tests co-fail by chance; must be de-biased.

### 3.5 Co-modification (git history)
- **Definition**: Same PPMI formula applied to commit co-membership.
- **Source**: Reuses `specere calibrate from-git` pipeline verbatim — the algorithm is already in the codebase, just needs a second consumer.
- **Noise**: medium; churn-heavy files dominate without care.

### 3.6 Provenance (who/what created this)
- **Definition**: Each harness file has a `(creator_agent, creator_verb, creator_spec, creator_commit, creator_human, created_at)` tuple.
- **Source**:
  - **Agent provenance**: SpecERE's existing workflow-span hooks (`after_implement`, `after_specify`). Join `files_created` attrs to the harness node — SpecERE **already has this data**; no new collection needed.
  - **Human provenance**: `git log --follow --diff-filter=A` for creation commit; `git blame` + `.mailmap` for line-level authorship.
  - **Divergence**: agent wrote file but a human patched it → record as `modified_by` edges, which is itself a signal.
- **This is SpecERE's unique surface**: no academic or industry tool fuses agent-span provenance with test-dependency graphs. Cursor's Agent-Trace RFC is the closest analogue and publishes only a JSON attribution side-channel.

### 3.7 Version / lineage (for "is this test rotting?")
- **Metrics**: `(age_days, total_commits, distinct_authors, churn_rate, last_touched, bus_factor)`.
- **Source**: subprocess `code-maat -a entity-churn -a age -a main-dev` (Adam Tornhill's Clojure tool) or a thin Rust reimplementation over `git log --numstat`. Reuses existing `calibrate from-git` walkers.
- **Noise**: low for large repos.

---

## 4 · Formal "used likely along with" score

For each pair `(a, b)`, SpecERE computes three raw sub-scores and a composite:

```
J_cov(a, b)    = |C(a) ∩ C(b)| / |C(a) ∪ C(b)|            ∈ [0, 1]
PPMI_fail(a, b) = max(0, log₂(p_fail(a,b) / (p_fail(a)·p_fail(b))))
PPMI_mod(a, b)  = max(0, log₂(p_mod(a,b) / (p_mod(a)·p_mod(b))))
w_indirect(a, b) = Σ_h [h ∈ N(a) ∩ N(b)] · log(|H| / df(h))
S(a, b) = 0.4·J_cov + 0.3·σ(PPMI_fail) + 0.2·σ(PPMI_mod) + 0.1·σ(w_indirect)
```

where `σ(x) = 1 − exp(−x)` is a saturating normaliser. **`S` is a sort key for the UI only; downstream queries consume the raw sub-scores separately.** Collapsing them too early erases information (coverage + co-failure tell you "tests of the same thing"; co-modification tells you "tests that happen to get edited together" — different conclusions).

Significance gates: require `n_joint_failures ≥ 5` before a PPMI_fail is reported (Hoeffding-style); require `n_joint_commits ≥ 3` for PPMI_mod (matches existing `min_commits` default in `calibrate from-git`).

---

## 5 · Novel SpecERE surface vs. prior art

Four items are genuinely new:

| # | Novel surface | Why no one else has it |
|---|---|---|
| 1 | **Bayesian cluster-belief updates over agentic spans** | Meta's Predictive Test Selection (ICSE 2019) is a GBDT on commits; SpecERE fuses OTel GenAI spans + coverage + clustering + BBN into a live posterior. |
| 2 | **Harness-provenance graph** linking each test to the `/speckit-*` verb that produced it | Every academic paper treats tests as ahistorical. SpecERE's workflow spans carry authoritative lineage. |
| 3 | **Heterogeneous harness-class awareness** — cluster within + across unit/property/fuzz/bench/snapshot | Academic tools assume one test kind. Real Rust repos mix nine. |
| 4 | **Supplementary `specere.harness.*` semantic convention** | OTel 1.40 has `cicd.*` + `gen_ai.*` but **no `test.*` namespace** (SIG hasn't landed it). SpecERE can slot in a vendor-prefixed convention and RFC upstream later. |

Everything else — coverage collection, test sharding, mutation testing, clustering algorithms, graph rendering — is **composition over existing OSS** (see §7).

---

## 6 · Data model sketch

### 6.1 Node: `HarnessFile` (one per test/fixture/workflow source)
```toml
id                 = "01HW..."           # ULID, stable across git moves
path               = "tests/fr_eq_003_lint_tests.rs"
prior_paths        = []                  # follow renames via `git log --follow`
category           = "integration"        # one of nine
category_confidence = 1.0
crate              = "specere"
test_names         = ["emits_one_event_per_smell_with_spec_attribution", ...]
provenance = { creator_agent = "claude-4.7",
               creator_verb = "/speckit-implement",
               creator_spec = "043-fr-eq-003-smell-detector",
               creator_commit = "2378809...",
               creator_human = "laiadlotape",
               created_at = "2026-04-19T22:40:00Z" }
version_metrics = { age_days = 1, commits = 3, authors = 1, churn_rate = 0.0,
                    last_touched = "2026-04-19T23:00:00Z", bus_factor = 1 }
flakiness_score    = 0.0                 # 0 if unknown
coverage_hash      = "blake3:..."         # digest of production lines it covers
platform_tags      = ["cfg(unix)", "cfg(feature = \"syn\")"]
tier               = "fast"               # fast | slow | serial (from nextest + timing)
```

### 6.2 Edges (nine types)
| Type | Direction | Weight source | Confidence |
|---|---|---|---|
| `direct_use` | a → b | 1.0 | 1.0 |
| `shared_helper` | a ↔ b | TF-IDF on shared h | 0.9 |
| `dep_overlap` | a ↔ b | Jaccard on crate deps | 0.7 |
| `cov_cooccur` | a ↔ b | Jaccard on coverage bitvector | 0.8 |
| `cofail` | a ↔ b | PPMI on CI run matrix | 0.5 |
| `comod` | a ↔ b | PPMI on commit matrix | 0.6 |
| `created_by` | a → WorkflowSpan | 1.0 | 1.0 |
| `modified_by` | a → WorkflowSpan | #lines touched | 0.9 |
| `authored_by` | a → Human | fraction of lines | 0.8 |

Weights are **not comparable across types**. Edges are *stored raw* and *composed per query*.

### 6.3 Persistence
- Node table → `.specere/harness-graph.toml` (sorted by `id`, deterministic output, TOML fits existing manifest conventions).
- Edge tables → `.specere/harness-graph.sqlite` (rusqlite, same crate SpecERE already uses for events).
- Derived cluster IDs → `.specere/harness-clusters.toml` (pastable into sensor-map for filter priors).

---

## 7 · Tech stack — compose-never-clone scorecard

### 7.1 Rust crates to pull in (Tier A, must-have)
| Crate | Role |
|---|---|
| `petgraph` | Core graph ADT. Already supports everything we need. |
| `walkdir` | Filesystem enumeration (already a dep from FR-EQ-003). |
| `syn` | AST walking for category/test-name extraction (already a dep). |
| `graphina` or `single-clustering` | Louvain + Leiden community detection. |
| `linfa-clustering` | HDBSCAN over coverage embeddings for outlier detection. |
| `tree-sitter-stack-graphs` | Cross-crate reference edges (complements SCIP). |
| `rusqlite` | Edge store (already a dep). |

### 7.2 External CLIs to subprocess (Tier B)
| Tool | Role |
|---|---|
| `cargo-nextest` | Test enumeration, JSON output, groups, partition. **Do not reimplement.** |
| `cargo-llvm-cov` | Per-test coverage (profraw). **Do not reimplement.** |
| `code-maat` | Churn/age/coupling analyses. Subprocess-only; don't port. |
| `cargo-mutants` | Already wired (FR-EQ-001); reuse its spec-attribution for sensitivity scores. |

### 7.3 Explicit non-build list (what we will NOT do)
- Do **not** reimplement Ekstazi/STARTS — leverage dep-info + llvm-cov instead.
- Do **not** write our own Louvain/Leiden — crates exist.
- Do **not** train or fine-tune embedding models — defer until v4 if ever.
- Do **not** implement cryptographic provenance (SLSA/in-toto) — premature for the use case.

---

## 8 · GUI — future-proofing the data pipe

The user flagged that SpecERE will get a GUI. The data model in §6 is designed to serve both CLI and GUI from the same REST endpoints exposed by the existing `specere serve http` Axum server.

### 8.1 Recommended stack (research §GUI)
- **Shell**: Tauri v2 — native, 600KB binaries, reuses `specere serve http` as the backend.
- **Graph rendering**: Sigma.js + Graphology (WebGL, 10k+ node fluid), layouts via ForceAtlas2 in a WebWorker.
- **Inspector panels**: React Flow (DOM-level node customisation, context menus, edge labels).
- **Fallback** (no-JS path): egui + `egui_graphs` single-binary Rust GUI for <1k-node repos.
- **Companion TUI**: ratatui — parallel track, not a replacement.

### 8.2 Core screens (v2 GUI)
1. **Harness Graph** — force-directed, coloured by cluster, sized by posterior entropy.
2. **Spec Dashboard** — per-spec UNK/SAT/VIO simplex + timeline sparkline.
3. **Review Queue** — markdown-backed Kanban (read `.specere/review-queue.md`, write back on state change).
4. **Event Timeline** — OTel waterfall of workflow spans (Perfetto-style).
5. **Relation Inspector** — "click a file, see all its edges" deep-link modal.
6. **Calibration View** — scatter of predicted vs. empirical posterior per spec.

### 8.3 New read-side Axum endpoints (CLI-first, GUI-ready)
- `GET /api/v1/harness/graph?format=graphology` — full graph as Graphology JSON.
- `GET /api/v1/harness/files/{path}/relations` — 2-hop neighbourhood + edge types.
- `GET /api/v1/harness/clusters?algo=leiden` — pre-computed cluster assignments.
- `GET /api/v1/specs/{id}/harness` — which harness files test this spec.
- `GET /api/v1/events/ws` — WebSocket stream of live span arrivals.

### 8.4 CLI-first rule
Every GUI-facing endpoint must be reachable via CLI first. Example: `specere harness graph --format json` before we write the graph view. GUI is a second consumer, never the only one.

---

## 9 · Implementation slices (priority order)

Six slices, each independently useful, ship-by-ship:

| Slice | Scope | Cost | Ship gate |
|---|---|---|---|
| **S1 · Enumerate + categorise** | Walk `src/`/`tests/`/`benches/`/`fuzz/`; classify into nine categories; extract `#[test]` names. New CLI: `specere harness scan`. | ~500 LoC | Direct linkage from `dep-info`. Categories cover 95% of repos. |
| **S2 · Provenance join** | Read existing workflow spans; match `files_created` to S1 nodes. New CLI: `specere harness provenance`. | ~300 LoC | Every node has a `creator_span_id` when available. |
| **S3 · Git version + co-modification** | Add code-maat-style churn + PPMI on commit matrix. Reuses `calibrate from-git`. New CLI: `specere harness history`. | ~400 LoC | Hotspot report matches Tornhill "Your Code as a Crime Scene" output. |
| **S4 · Coverage co-execution** | Opt-in `cargo-llvm-cov` integration; Jaccard on coverage bitvectors. New CLI: `specere harness coverage --with-nextest`. | ~600 LoC | At least one real-world repo's `J_cov > 0.8` pairs all reflect genuine coupling. |
| **S5 · Co-failure + flakiness** | Persist per-run JUnit into event store; PPMI + DeFlaker coverage-join + Meta prob-flakiness. New CLI: `specere harness flaky`. | ~500 LoC | Classifies 70%+ of known flakes in a seed project. |
| **S6 · Cluster + filter wiring** | Leiden on the combined edge graph; emit `[harness_cluster]` section into sensor-map; wire cluster belief into existing BBN. New CLI: `specere harness cluster`. | ~400 LoC | Cluster IDs become priors over FR groups; filter picks them up without code change. |

**S1–S3 ship together as v1.2.0** (~1200 LoC, no new heavy deps, pure value-add).
**S4–S5 ship as v1.3.0** (coverage is opt-in; flakiness needs historical data).
**S6 ships as v1.4.0** (the Bayesian integration that closes the loop).
**GUI is v2.0.0** (6-screen MVP on Tauri).

---

## 10 · Questionnaire for go/no-go

Before we write the FR-numbered plan, please answer:

1. **Scope of first release.** v1.2.0 = S1+S2+S3 (harness scan + provenance + history). Right?
2. **Release cadence.** Do we cut v1.2.0 as a patch-level release like v1.0.5/6, or do we target crates.io publication for v1.2? (Reminder: SpecERE is not on crates.io today per `MEMORY.md` pivot notes.)
3. **Coverage collection cost.** S4 doubles test-suite wall-clock time. Opt-in (`specere harness coverage` only when user runs it) or always-on behind a feature flag?
4. **Flakiness minimum data.** S5 needs ≥50 CI runs of history. Do we build it now and wait for data, or defer until a real SpecERE consumer has that history?
5. **GUI timing.** Tauri + Sigma.js MVP is ~4-6 weeks. Ship *before* S6 (so users can see the data as we build it), *after* S6 (so all the data exists to visualise), or *in parallel* on a separate track?
6. **TUI companion.** Is a `specere harness tui` (ratatui) worth a night-and-weekend track? Power users would love it; it also serves as the CLI→GUI bridge.
7. **Supplementary OTel semconv.** Do we formalise `specere.harness.*` attributes and RFC upstream to the OTel CI/CD SIG, or stay vendor-private?
8. **Novel Bayesian surface priority.** Is the "cluster-belief-update over agentic spans" (the genuinely novel §5-#1 item) valuable enough to be the headline feature, or is the provenance graph (§5-#2) the more defensible story for users?

Your answers pin down v1.2.x/v1.3.x/v1.4.x/v2.0 release timing and what each contains. Then I cut FR-HM-001..NNN and begin S1.

---

## 11 · Risks

- **Coverage overhead**. 2-3× test-suite time. S4 must be opt-in and well-documented.
- **Flakiness filter quality**. Without DeFlaker-style coverage-join, co-failure edges will be mostly noise. Ship order (S4 before S5) is non-negotiable.
- **Provenance divergence**. If an agent writes a file and a human immediately rewrites it, the creator_agent field misleads. We record both; the reviewer decides. Must document in S2.
- **Cluster instability**. Leiden is deterministic only with a fixed seed. Document seed policy before S6.
- **GUI scope creep**. The MVP is 6 screens. Resist adding a 7th until users ask.
- **Monorepo scaling**. 10k+ harness files per repo exist. Sigma.js + FA2 handles 10k; past that, hierarchical collapse + LOD rendering kicks in.
- **Compose risk**. `code-maat` is Clojure; `cargo-llvm-cov` wraps llvm-profdata. Both add subprocess dependencies. Fall-back plans: pure-Rust git log walker (already built); accept tarpaulin as macOS fallback where llvm-cov is slower.

---

## 12 · Research sources

See the three deep-research briefs attached:
- **§SoA** — 30+ URLs on RTS, predictive test selection, flakiness detection, code-graph protocols.
- **§Relation taxonomy** — formal PPMI/Jaccard derivations, Ekstazi/STARTS/DeFlaker/FlakeFlagger citations.
- **§GUI** — Tauri v2 / Sigma.js / egui_graphs benchmark comparisons, 6-screen UX sketch.

Full URL bibliography kept in the agent research transcripts at `.claude/projects/.../tasks/{aa54daca,a76089a1,a1a9c4ad}.output`.

---

*End of proposal. Reviewer: answer §10 questionnaire; reply "go" + choices; I will cut FR numbers and start S1.*
