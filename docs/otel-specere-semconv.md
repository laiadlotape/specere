# SpecERE Supplementary OpenTelemetry Semantic Convention

**Scope.** This document formalises the `specere.*` attribute namespace used across the SpecERE toolchain — evidence-quality events, workflow spans, and harness-manager events. It is a **Supplementary Semantic Convention** per [OpenTelemetry's semconv contribution guide](https://opentelemetry.io/docs/specs/semconv/) — not yet an upstream convention. We publish it in-repo so that downstream consumers (the filter, the GUI, third-party OTLP backends) have a stable attribute contract to rely on.

**Upstream RFC path.** Once two or more consumers of this convention exist, we will propose the `test.*` and `harness.*` portions upstream to the [CI/CD SIG](https://github.com/open-telemetry/semantic-conventions/tree/main/docs/cicd). Until then, SpecERE's vendor-prefixed variants are the authoritative source.

**Related upstream conventions.**

- [`gen_ai.*`](https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-spans/) — GenAI agent spans. SpecERE hosts these alongside its own attributes; they are NOT redefined.
- [`cicd.*`](https://opentelemetry.io/docs/specs/semconv/cicd/cicd-spans/) — CI/CD pipeline runs. When emitted from a CI context, SpecERE events SHOULD carry these upstream attributes in addition to `specere.*`.
- [`code.*`](https://opentelemetry.io/docs/specs/semconv/attributes-registry/code/) — source-code location. SpecERE uses `code.filepath` + `code.function.name` verbatim.

---

## 1. Namespaces

| Prefix | Owner | Shape |
|---|---|---|
| `specere.*` | This document | SpecERE-global attributes applicable across any event kind. |
| `specere.harness.*` | This document | Harness-manager-specific (FR-HM-*). |
| `specere.workflow_*` | [Phase-3 workflow-span convention](./specere_v1.md#phase-3) | `/speckit-*` verb telemetry. |
| `specere.filter.*` | Phase-4 filter convention | Per-spec posterior + calibration. |

---

## 2. Event kinds (the `attrs.event_kind` axis)

Every event SpecERE emits carries a single `event_kind` string attribute. This is the primary discriminator downstream consumers dispatch on.

| `event_kind` | Emitted by | Purpose |
|---|---|---|
| `workflow_span` | `specere observe record` (from `before_<verb>` / `after_<verb>` hooks) | Per-`/speckit-*`-verb span. |
| `test_outcome` | User-supplied hooks; `specere filter` drive | Single test pass/fail result. |
| `files_touched` | User-supplied hooks | Files modified within a span. |
| `mutation_result` | `specere evaluate mutations` (FR-EQ-001) | One event per cargo-mutants mutant. |
| `test_smell_detected` | `specere lint tests` (FR-EQ-003) | Advisory lint finding. |
| `bug_reported` | `specere observe watch-issues` (v1.0.6 roadmap) | Issue-tracker-derived event. |
| `counterexample_found` | LLM adversary (v1.1.0 roadmap) | Adversarial test discovery. |
| `adversary_budget_exhausted` | LLM adversary (v1.1.0 roadmap) | Spend cap hit. |
| `harness_scan_completed` | `specere harness scan` (FR-HM-001) | Classification run summary. |
| `harness_provenance_completed` | `specere harness provenance` (FR-HM-010) | Provenance join summary. |
| `harness_history_completed` | `specere harness history` (FR-HM-020) | Git-metrics + PPMI summary. |
| `harness_coverage_completed` | `specere harness coverage` (FR-HM-030) | Per-test coverage Jaccard summary. |
| `harness_flaky_completed` | `specere harness flaky` (FR-HM-040) | Flakiness + co-failure summary. |
| `harness_cluster_completed` | `specere harness cluster` (FR-HM-050) | Louvain clustering summary. |

**Forward compatibility.** Unknown `event_kind` values MUST be ignored by the filter (tracked as `skipped`). This mirrors FR-EQ-007 / FR-P4-001.

---

## 3. Global `specere.*` attributes

Applicable to any event kind.

| Attribute | Type | Cardinality | Notes |
|---|---|---|---|
| `specere.schema_version` | int | 1 | Currently `1`. Bump only on breaking attribute changes. |
| `specere.cli_version` | string | 1 | e.g. `1.2.0`. Lets consumers reject events from newer tool versions they don't understand. |
| `specere.feature_dir` | string | 0..1 | Absolute or repo-relative path of the active speckit feature directory when applicable. |
| `specere.fr_ids` | string[] | 0..* | FR identifiers the event concerns (comma-separated wire format). |
| `specere.repo_root` | string | 0..1 | Absolute repo root. |

---

## 4. Workflow-span attributes (`specere.workflow_*`)

Phase-3 defined. Reproduced here for completeness.

| Attribute | Type | Cardinality | Notes |
|---|---|---|---|
| `specere.workflow_step` | string | 1 | `specify` / `clarify` / `plan` / `tasks` / `analyze` / `checklist` / `implement`. |
| `specere.workflow_phase` | string | 1 | `before` / `after`. |
| `specere.tasks_flipped` | int | 0..1 | Number of `[ ]`→`[X]` in tasks.md during an implement span. |
| `specere.duration_ms` | int | 0..1 | End-to-end duration. |

**Companion upstream**: populate `gen_ai.system`, `gen_ai.request.model`, `gen_ai.agent.id`, and `gen_ai.operation.name` on the same span when the work was driven by an LLM.

---

## 5. Harness-manager attributes (`specere.harness.*`)

Applicable to the five `harness_*_completed` event kinds and to any future per-file events we emit.

### 5.1 Node-level (one instance per harness file described by the event)

| Attribute | Type | Cardinality | Notes |
|---|---|---|---|
| `specere.harness.id` | string | 1 | 16-char hex path-hash ID (see `HarnessFile::id`). |
| `specere.harness.path` | string | 1 | Repo-relative, forward-slash path. Windows backslashes are normalised before emission. |
| `specere.harness.kind` | string | 1 | One of `unit` / `integration` / `property` / `fuzz` / `bench` / `snapshot` / `golden` / `mock` / `fixture` / `workflow` / `production`. |
| `specere.harness.category_confidence` | double | 0..1 | `[0.0, 1.0]`. |
| `specere.harness.crate` | string | 0..1 | Cargo crate name when present in a workspace layout. |
| `specere.harness.test_names` | string[] | 0..* | Comma-separated extracted test-fn names (per FR-HM-002). |
| `specere.harness.coverage_digest` | string | 0..1 | 16-char hex SHA-256 of the test's line-hit bitvector (per FR-HM-032). |
| `specere.harness.flakiness_score` | double | 0..1 | `P(fail)`, in `[0.0, 1.0]`. Set only when `n_runs >= min_runs`. |
| `specere.harness.cluster_id` | string | 0..1 | Louvain cluster label, e.g. `C03` (per FR-HM-050). |

### 5.2 Provenance (subset — see §5.3 for the `provenance.speckit_unit` alias)

| Attribute | Type | Cardinality | Notes |
|---|---|---|---|
| `specere.harness.provenance.speckit_unit` | string | 0..1 | `specify` / `plan` / `implement` / etc. — the `/speckit-*` verb that created the file. |
| `specere.harness.provenance.agent` | string | 0..1 | `gen_ai.system` of the creator span (`claude-code`, `cursor`, …). |
| `specere.harness.provenance.commit` | string | 0..1 | 40-char git SHA of the introducing commit. |
| `specere.harness.provenance.human_email` | string | 0..1 | Committer email. Human vs. agent attribution is intentionally dual. |
| `specere.harness.provenance.divergence_detected` | boolean | 0..1 | `true` when agent-authored AND later human commit (advisory). |

### 5.3 Run-summary attributes (one set per `harness_*_completed` event)

These describe the aggregate result of the CLI invocation.

| Attribute | Applies to | Type | Notes |
|---|---|---|---|
| `specere.harness.n_files` | all `harness_*_completed` | int | Total nodes in the graph after the run. |
| `specere.harness.n_files_enriched` | provenance / history / coverage / flaky | int | How many nodes the run populated. |
| `specere.harness.n_edges.direct_use` | scan / cluster | int | Edge-count per type. |
| `specere.harness.n_edges.comod` | history / cluster | int | |
| `specere.harness.n_edges.cov_cooccur` | coverage / cluster | int | |
| `specere.harness.n_edges.cofail` | flaky / cluster | int | |
| `specere.harness.n_clusters` | cluster | int | |
| `specere.harness.total_modularity` | cluster | double | Louvain modularity after convergence. |
| `specere.harness.cluster_seed` | cluster | int | Seed used — required for replay. |
| `specere.harness.flakes_flagged` | flaky | int | Tests with `flakiness_score > flake_threshold`. |
| `specere.harness.insufficient_history` | flaky | boolean | `true` when `n_runs < min_runs`. |

---

## 6. Attribute naming rules

SpecERE follows [OpenTelemetry's general naming guide](https://opentelemetry.io/docs/specs/semconv/general/naming/) with one local addition: **arrays are wire-encoded as comma-separated strings**. Rationale: the event-store format is JSONL for portability, and the upstream `string[]` type often loses fidelity through OTLP/HTTP translation. SpecERE consumers split on `,` and strip whitespace.

Attribute casing: `snake_case` segments separated by `.`. No camelCase. No hyphens inside attribute names (the `-` in `co-modification` becomes `comod`).

---

## 7. Contract tests

Every SpecERE CLI verb that emits events MUST have a contract test that:

1. Spawns the verb in a TempRepo.
2. Reads `.specere/events.jsonl` after the run.
3. Asserts `event_kind` is one of the declared values in §2.
4. Asserts `specere.schema_version == 1`.
5. Asserts verb-specific attributes from §5 are present + have the correct type.

These tests live alongside each FR's integration tests under `crates/specere/tests/fr_*.rs`.

---

## 8. Versioning policy

- **SpecERE patch versions** (e.g. 1.2.0 → 1.2.1) MAY add new attributes, new `event_kind` values, and new optional fields. Never remove.
- **SpecERE minor versions** MAY remove or rename attributes — but MUST keep the old wire name recognised by the event parser for one minor cycle (deprecation window).
- **SpecERE major versions** MAY bump `specere.schema_version`. Consumers rejecting unknown schema versions SHOULD surface a friendly "upgrade SpecERE" message, not crash.

---

## 9. Relation to the event-store schema

SpecERE's on-disk event format (`.specere/events.jsonl`) is a minimally-framed JSONL of `{ts, source, signal, attrs}` records. This document pins the `attrs` side only. The framing itself is defined by `specere-telemetry::Event`.

Upstream OTLP payloads MAY use richer shapes (spans with nested events, links, baggage); SpecERE consumers always reduce to the `{ts, source, signal, attrs}` view before processing.

---

## 10. Interop with upstream `test.*` (future)

The OTel CI/CD SIG is working on a `test.*` namespace ([agenda](https://github.com/open-telemetry/semantic-conventions/issues/951)). Once it lands, SpecERE will:

1. Add upstream-compliant `test.case.name`, `test.case.result.status`, etc. to `harness_flaky_completed` events alongside the existing `specere.harness.flakiness_score`.
2. Deprecate the `specere.harness.flakiness_score` attribute in favour of upstream, keeping backward-compat parsing for one minor cycle.
3. Submit this document (or a distilled subset) as an RFC to the CI/CD SIG.

Until that happens, **`specere.harness.*` is the stable contract** — consumers MAY rely on every attribute in §5.
