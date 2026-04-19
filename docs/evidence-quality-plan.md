# Evidence-quality execution plan (v1.0.5 → v1.1.0)

**Status.** Drafted 2026-04-19. Scope + direction approved via interactive questionnaire on `docs/proposals/v2-evidence-quality.md`. No code touched yet — this plan is what lands first, reviewed, then implementation follows.

**Release shape.**

| Release | Slice | Scope | FRs |
|---|---|---|---|
| **v1.0.5** | Mutation-calibrated sensors + test-smell detector + motion-matrix-fit + suspicious-SAT review queue | ~900 LoC, no external APIs or paid services | FR-EQ-001 … FR-EQ-007 |
| **v1.0.6** | Bug-tracker bridge (GitHub + Gitea) | ~600 LoC, requires credentials, light LLM spend for triage (embeddings) | FR-EQ-010 … FR-EQ-013 |
| **v1.1.0** | LLM adversary agent with hard $20/mo budget | ~800 LoC + ongoing LLM spend | FR-EQ-020 … FR-EQ-024 |

Each release is patch-compatible except **v1.1.0** which introduces a new CLI verb surface and bumps the minor. Patches only add event kinds + channels; the filter's public interface is unchanged.

**Authority.** This plan supersedes `docs/upcoming.md` priority queue items #1–#4 (calibrate motion-fit tail, MarkerEntry compat, RBPF CLI routing, long spec-ID alignment). Those carry-over items are re-prioritised in § 10.

---

## 1. v1.0.5 — mutation + smells + motion + review-queue

### FR-EQ-001 — `specere evaluate mutations` subcommand

**Acceptance criteria.**

- New `specere evaluate mutations` verb that shells out to `cargo-mutants --json --in-diff <ref>` (or `--file <paths>`) with flags for scope control.
- Parses cargo-mutants' JSON output and emits one event per mutant to `.specere/events.jsonl`:
  ```json
  {
    "ts": "<rfc3339>",
    "source": "cargo-mutants",
    "signal": "traces",
    "attrs": {
      "event_kind": "mutation_result",
      "spec_id": "<fr-id>",
      "outcome": "caught | missed | timeout | unviable",
      "operator": "replace_if | flip_comparison | ...",
      "file": "crates/.../lib.rs",
      "line": "142"
    }
  }
  ```
- `spec_id` resolved by intersecting the mutated file's path with `[specs]` entries' `support` lists in sensor-map.toml. Same directory-boundary semantics as `specere calibrate from-git` (no cross-sibling false matches, v1.0.1 fix).
- `--scope <fr-id>` flag to run mutation only on files supporting one FR.
- `--jobs N` forwarded to cargo-mutants.
- Exit 0 on success; nonzero only on infrastructure failure (missing cargo-mutants, malformed JSON).
- Advisory per specere convention: a 40% kill rate is not an error.

**Out of scope.** Installing cargo-mutants itself — assume present on PATH; clear error if missing.

**Test plan.** 4 integration tests.
- End-to-end run against a tiny Rust crate with 2 specs and ~10 mutants.
- Scope flag correctly restricts mutants.
- Spec-ID attribution matches expected sensor-map mapping.
- Handles missing cargo-mutants gracefully.

### FR-EQ-002 — sensor-calibration formula integrates mutation kill rate

**Acceptance criteria.**

- `PerSpecHMM` / `FactorGraphBP` / `RBPF` accept a new `Calibration` struct:
  ```rust
  pub struct Calibration {
      pub quality: f64,        // ∈ [0.3, 1.0]
      pub alpha_sat: f64,
      pub alpha_vio: f64,
      pub alpha_unk: f64,
  }
  ```
- `Calibration::default()` returns prototype alphas (0.92 / 0.90 / 0.55) and `quality=1.0`. Backwards-compatible with v1.0.4 behaviour.
- A new `Calibration::from_evidence(kill_rate, smell_penalty)` implements the formula in `docs/proposals/v2-evidence-quality.md §5`:
  ```
  q = clamp(0.3, kill_rate * smell_penalty, 1.0)
  α_sat = 0.55 + q * 0.37
  α_vio = 0.45 + q * 0.45
  α_unk = 0.55  # unchanged
  ```
- `DefaultTestSensor::log_likelihood` becomes `CalibratedTestSensor` parameterised on `Calibration`. Old `DefaultTestSensor` preserved as an alias returning prototype alphas.
- Gate-A parity fixture regenerated with `quality=1.0` to preserve bit-identical parity (FR-P4-002 anchor must still pass).

**Test plan.** 3 unit tests + 1 parity refresh.
- Prototype defaults → `quality=1.0` → alphas unchanged.
- `kill_rate=0.3` → alphas compress: `α_sat ≈ 0.66`, `α_vio ≈ 0.59`.
- `kill_rate=0` with `smell_penalty=0.5` → `q = 0.3` (clamped), alphas at the floor.
- Gate-A parity test still bit-identical under the new code path.

### FR-EQ-003 — `specere lint tests` detects test smells

**Acceptance criteria.**

- New `specere lint tests` verb that walks `src/**/*.rs` + `tests/**/*.rs`, parses with `syn`, applies a rule set:
  - `tautological-assert` — `assert_eq!(x, x)`, `assert_ne!(x, y)` where `x == y` literally, `assert!(true)`.
  - `no-assertion` — test fn body contains no `assert*!` / `.unwrap_err()` / `? == Err` discriminant.
  - `mock-only` — test body is 90%+ `mock_*` macro calls, no real subject.
  - `single-fixture` — all tests for a fn use the same input constant (happy-path only).
- Emits `test_smell_detected` events:
  ```json
  {
    "attrs": {
      "event_kind": "test_smell_detected",
      "spec_id": "<inferred>",
      "smell_kind": "tautological-assert",
      "severity": "info",
      "test_fn": "tests/foo::smoke",
      "file": "tests/foo.rs",
      "line": "42"
    }
  }
  ```
- Severity is always `info` per user's choice (§ 4 answer: degrade posterior but proceed).
- `spec_id` inferred from the test file's path via sensor-map's support sets, OR from a `// FR-NNN` comment in the test body (opt-in annotation), OR `unknown` if neither matches.

**Test plan.** 6 unit tests, one per smell, + 2 integration tests covering the full pipeline on a fixture crate.

### FR-EQ-004 — `specere calibrate motion-from-evidence`

**Acceptance criteria.**

- New subcommand `specere calibrate motion-from-evidence` (distinct from the existing `specere calibrate from-git` which produces coupling edges).
- For each spec in `[specs]`:
  - Walk events.jsonl + any history from `events.sqlite`.
  - Identify `test_outcome` events with a preceding `files_touched` or `mutation_result` event.
  - Estimate per-spec transition matrix using the Laplace-smoothed MLE described in `docs/specere_v1.md §5.P5`.
  - Requires at least 20 events per spec to emit a fit; otherwise reports `insufficient history`.
- Writes proposed per-spec `[motion]` table to stdout for user to paste into sensor-map.toml:
  ```toml
  [motion."FR-001"]
  t_good = [[0.10, 0.80, 0.10], [...], [...]]  # or whatever the fit produces
  t_bad  = [[0.10, 0.10, 0.80], [...], [...]]
  t_leak = [[...], [...], [...]]
  ```
- `Motion::prototype_defaults()` remains the default; per-spec motion overrides it when present in sensor-map.
- Writes a companion `[calibration]` section with `quality` from current mutation kill rate (per FR-EQ-002) so filter pick-up is one-step.

**Test plan.** 3 integration tests.
- 20+ synthetic events → per-spec motion emitted.
- < 20 events → "insufficient history" message, no output.
- End-to-end: `cargo-mutants` → `calibrate motion-from-evidence` → `filter run` uses fitted alphas.

### FR-EQ-005 — filter-run integration

**Acceptance criteria.**

- `run_filter_run` loads `Calibration` per spec from the aggregation of:
  - Mutation kill rate (aggregate all `mutation_result` events for this spec in window, default: last 30 days).
  - Smell penalty (aggregate `test_smell_detected` — each smell compresses `smell_penalty` by 0.15, floor 0.3).
  - Per-spec motion from sensor-map if present; else `Motion::prototype_defaults()`.
- Emits a one-line summary per spec on `filter run`:
  ```
  specere filter: processed 12 event(s); calibration:
    FR-001  kill=0.87 smells=0 q=0.87 α_sat=0.87 α_vio=0.84
    FR-002  kill=0.45 smells=2 q=0.32 α_sat=0.67 α_vio=0.59 ← suspicious
  ```
- No existing tests break (FR-P4-001/003/004/006 regression anchors still green).
- Backwards compatibility: a repo with zero mutation/smell events uses `q=1.0` (prototype defaults) — identical to v1.0.4 behaviour.

**Test plan.** 3 integration tests.
- Zero evidence events → prototype-default alphas.
- Mutation events present → alphas compress correctly.
- Filter run output contains calibration summary.

### FR-EQ-006 — `specere doctor --suspicious` review-queue flagging

**Acceptance criteria.**

- `specere doctor --suspicious` scans posterior.toml + the calibration output of a hypothetical next `filter run`.
- For each spec where `p_sat > 0.95` AND `quality < 0.5` (default thresholds; configurable in sensor-map):
  - Append an entry to `.specere/review-queue.md`:
    ```markdown
    ## Suspicious high-confidence SAT — FR-001 (auto-flagged 2026-04-19)
    
    - **Posterior**: p_sat = 0.97, p_vio = 0.01, p_unk = 0.02
    - **Calibration**: quality = 0.34 (mutation kill 0.40, smell penalty 0.85)
    - **Recommendation**: Human sanity-check before trusting this SAT — either the test suite is too weak to discriminate, or smells are dragging calibration down.
    ```
- Never auto-removes entries — manual review is the adjudication (constitution V).
- Thresholds configurable in `sensor-map.toml`:
  ```toml
  [review]
  suspicious_p_sat_min = 0.95
  suspicious_quality_max = 0.50
  ```

**Test plan.** 2 integration tests (suspicious spec flagged, confident spec ignored).

### FR-EQ-007 — additive event schema

**Acceptance criteria.**

- Events with new `event_kind` values (`mutation_result`, `test_smell_detected`, `bug_reported`, `counterexample_found`, `adversary_budget_exhausted`) parse cleanly in the existing event-store pipeline.
- Unknown-to-filter event kinds continue to be counted in `skipped` (existing behaviour preserved).
- Cursor semantics unchanged — new events advance the cursor exactly like old ones.
- Forward-compat fixture test: a posterior.toml written by v1.0.5 loads cleanly in v1.0.4 (and vice versa) via `#[serde(default)]` + `#[serde(alias)]` as already established.

**Test plan.** 1 cross-version round-trip test.

---

## 2. v1.0.6 — bug-tracker bridge

### FR-EQ-010 — `specere observe watch-issues`

**Acceptance criteria.**

- New verb `specere observe watch-issues --provider {github,gitea} --repo <owner/name>` that polls the issue endpoint every 10 minutes (configurable via `--interval`).
- Credentials via env: `GITHUB_TOKEN`, `GITEA_TOKEN`; error cleanly if absent.
- For each new issue since the cursor in `.specere/issues-cursor.toml`:
  - Skip if label in `{question, docs, duplicate, not-planned}` or state is `closed` without a linked PR.
  - Otherwise emit a `bug_reported` event (see FR-EQ-012 for attrs).
- Gitea's `GET /repos/{owner}/{repo}/issues?state=open&since=<ts>` is compatible with GitHub's shape — implementation shares code.
- Backgrounded via `--daemon` or one-shot via `--once` (default: `--once` for CI use).

**Test plan.** 3 integration tests, each with a mock HTTP server (`mockito` or `wiremock-rs`) serving canned GitHub/Gitea responses.

### FR-EQ-011 — LLM issue-to-spec triage

**Acceptance criteria.**

- A new library module `specere-filter::triage` that takes an issue body + sensor-map's `[specs]` and returns the most likely `spec_id`.
- Approach: embed issue body with a local model (or remote API), cosine-match against each spec's `description` / `support` file contents, LLM-rerank top 3.
- Configurable via `sensor-map.toml`:
  ```toml
  [triage]
  embedder = "openai:text-embedding-3-small"  # or "local:<path>"
  reranker = "anthropic:claude-3-5-haiku"
  min_confidence = 0.60  # if highest match < this, emit spec_id="unknown"
  ```
- No triage if LLM APIs aren't configured — falls back to heuristic (stack-trace file path parse + CODEOWNERS match) with lower confidence.
- Cost cap: configurable `max_monthly_spend_usd` (default $5 — triage is cheap).

**Test plan.** 3 tests — fixtures with known-good triage, heuristic-only fallback, over-budget handling.

### FR-EQ-012 — `bug_reported` event feeds posterior with decay

**Acceptance criteria.**

- Event attrs: `spec_id`, `issue_url`, `severity=critical|major|minor`, `age_days`, `state=open|closed`.
- Filter-run treats each open `bug_reported` as a VIO injection with magnitude scaled by severity: critical=0.3, major=0.15, minor=0.05.
- Exponential decay with 50-day half-life (per Kim '07 empirical optimum) — an event from day 100 counts 1/4 as much as a fresh one.
- Closed bugs (state=closed + linked PR merged) decay faster (25-day half-life) — the fix itself is the adjudication.
- No double-counting: a bug reported AND fixed AND closed produces one "VIO then SAT" trajectory, not two independent events.

**Test plan.** 3 integration tests covering decay math, severity scaling, and open→closed state change.

### FR-EQ-013 — cursor handles filesystem + remote events

**Acceptance criteria.**

- `posterior.toml` gains a new optional `cursors` table:
  ```toml
  cursor = "2026-04-19T14:00:00Z"              # legacy single cursor
  [cursors]
  events_jsonl = "2026-04-19T14:00:00Z"
  github_issues = "2026-04-19T13:55:00Z"
  gitea_issues = "2026-04-19T14:01:00Z"
  ```
- Single-cursor repos upgrade-in-place: on first read, the legacy cursor becomes `cursors.events_jsonl`.
- `specere filter run` advances all cursors to their respective source's latest ts.

**Test plan.** 2 round-trip tests (v1.0.5 → v1.0.6 upgrade; mixed-source event consumption).

---

## 3. v1.1.0 — LLM adversary agent

### FR-EQ-020 — `specere adversary run --spec FR-NNN`

**Acceptance criteria.**

- New verb `specere adversary run --spec FR-NNN` (or `--all` for batch mode).
- Reads the FR text from `specs/<feature>/spec.md`, the spec's support files, and the test files that exercise them.
- Iterative loop:
  1. Send `(spec text, support files, existing tests)` to an LLM with a prompt asking for "one input / scenario that would violate this spec if the implementation were naive or buggy."
  2. Extract the proposed test case.
  3. Run it in a sandbox (see FR-EQ-024).
  4. If it fails → minimize (delta-debug) → emit `counterexample_found` event.
  5. If it passes → go to step 1 with a different angle (up to N iterations).
- Budget tracked in `.specere/adversary-budget.toml`:
  ```toml
  month = "2026-04"
  spent_usd = 3.47
  cap_usd = 20.0
  ```
- On cap hit: emit `adversary_budget_exhausted` for the current spec + any skipped specs; exit 0 with a clear message.

**Test plan.** 5 integration tests — covering a synthetic broken spec (adversary finds it), a correct spec (adversary exhausts iterations, emits budget_exhausted), budget cap enforcement, model output parsing edge cases, minimization correctness.

### FR-EQ-021 — hard $20/month ceiling with deferral

**Acceptance criteria.**

- Monthly cap of $20 default, configurable in `sensor-map.toml`:
  ```toml
  [adversary]
  cap_usd_per_month = 20.0
  model = "anthropic:claude-sonnet-4-6"
  max_iterations = 5
  ```
- `spent_usd` incremented after each LLM call using the API's `usage.total_cost` or a local computation from token counts.
- Budget rolls over on the 1st of each month (local TZ).
- `specere adversary status` prints current `spent_usd / cap_usd` + est iterations remaining.
- Attempting to run when `spent_usd >= cap_usd` exits with a clear message naming the cap; exit code 2 (distinct from other errors).

**Test plan.** 3 integration tests (cap-not-hit, cap-hit, roll-over).

### FR-EQ-022 — ≥ 3-iteration rule before posterior update

**Acceptance criteria.**

- A counterexample is only written to events.jsonl if:
  - The falsification loop ran ≥ 3 iterations before finding it (one-shot findings are suspect per Liu '24).
  - The counterexample reproduces deterministically (the test runs twice with the same input + gets the same failure).
- Single-iteration findings are recorded as `counterexample_candidate` (not `counterexample_found`) and surfaced to review queue for human judgment, not posterior update.

**Test plan.** 2 integration tests covering first-iter-find (candidate only) vs third-iter-find (posterior update).

### FR-EQ-023 — counter-example minimization

**Acceptance criteria.**

- Delta-debugging loop on a found counterexample to produce the minimal input that still fails.
- Record both the original and minimized form in the event attrs.
- Minimization is time-boxed (30 s default) — if it doesn't converge, record the original.

**Test plan.** 2 integration tests on a small fixture.

### FR-EQ-024 — sandbox for LLM-generated test code

**Acceptance criteria.**

- LLM-generated test code runs in a container or process-isolated subprocess with:
  - No network access.
  - Read-only access to the repo files.
  - Write access to a scratch dir only.
  - 30 s wall-clock timeout.
  - Resource limits (256 MB memory, 1 CPU).
- On escape attempt or timeout: emit `adversary_sandbox_violation` event (distinct from a successful counter-example); do NOT update posterior.

**Test plan.** 4 integration tests — normal run, timeout, memory limit, network attempt.

---

## 4. Dependency graph

```
v1.0.5                v1.0.6                v1.1.0
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│ mutation +    │───▶│ bug-tracker   │───▶│ adversary     │
│ smells +      │    │ bridge        │    │ agent         │
│ motion fit +  │    │ (GitHub,      │    │ ($20 cap,     │
│ review queue  │    │  Gitea local) │    │  sandbox)     │
└───────────────┘    └───────────────┘    └───────────────┘
   Calibration          Independent          Active falsification;
   machinery.           evidence channel;    consumes v1.0.5's
   Enables weighted     decay-weighted;      calibration to weigh
   posterior under      works with or        its own output.
   weak tests.          without LLM triage.
```

Each slice depends on its left neighbour:
- v1.0.6 reuses v1.0.5's `Calibration` struct + event schema.
- v1.1.0 needs v1.0.5 to damp its hallucination-prone output AND v1.0.6 to cross-check its findings ("did this counterexample also trigger a real bug report?").

## 5. Re-planning triggers

- **Mutation runtime > 30 min** on a medium crate. Pause, move to nightly-only mode for FR-EQ-001.
- **LLM triage false-positive rate > 40%** measured on a labelled sample. Pause FR-EQ-011, fall back to heuristics until we can improve.
- **Adversary budget burn > $5 in first week** on a typical repo. Pause FR-EQ-020, tighten `max_iterations` default to 3, re-evaluate.
- **Bit-identical Gate-A parity breaks** when refactoring `DefaultTestSensor` → `CalibratedTestSensor`. Hard stop; the FR-P4-002 anchor is non-negotiable.
- **Review-queue noise** (> 5 false-alarm suspicious flags per session). Adjust thresholds; if that doesn't fix it, the calibration formula needs revision.

## 6. Exit criteria

**v1.0.5 ships when:**
- All FR-EQ-001 … FR-EQ-007 acceptance criteria met.
- Workspace tests +25 minimum.
- `specere evaluate mutations` runs clean on specere's own repo and produces per-spec kill rates.
- Gate-A parity (FR-P4-002) bit-identical under the new `CalibratedTestSensor` code path.
- CHANGELOG rolled; self-dogfood guide updated with a new part exercising `evaluate mutations` + `lint tests` + the review queue.

**v1.0.6 ships when:**
- FR-EQ-010 … FR-EQ-013 met.
- Mock-HTTP integration tests for both GitHub + Gitea.
- Credential handling is secret-safe (no tokens in CHANGELOG, test fixtures, or logs).

**v1.1.0 ships when:**
- FR-EQ-020 … FR-EQ-024 met.
- Sandbox fuzzed against a test-suite of malicious-LLM-output scenarios (attempted `curl`, `rm -rf /`, fork-bomb, network probe) with all blocked.
- $20 cap verified via a stubbed API returning high-cost responses until cap hits.

## 7. Test surface estimates

- v1.0.5: ~25 new tests (6 mutation-verb, 3 sensor calibration, 8 smell rules, 3 motion-fit, 3 filter-run integration, 2 review-queue).
- v1.0.6: ~11 new tests (3 watch-issues, 3 triage, 3 decay, 2 cursor-compat).
- v1.1.0: ~16 new tests (5 adversary-run, 3 budget, 2 iteration rule, 2 minimization, 4 sandbox).

Total projected: ~52 new tests. Current baseline: 188. Target post-v1.1.0: ~240.

## 8. External dependencies

- **v1.0.5**: `cargo-mutants` (run-time only, not a cargo dep). `syn` (already an indirect dep).
- **v1.0.6**: `reqwest` (already a dep). Possibly `wiremock` (test-only). Optional LLM client (`anthropic-sdk`?).
- **v1.1.0**: LLM client (`anthropic-sdk` or `reqwest` + manual). `nix` or `bollard` for sandboxing (investigate both; `nsjail`/`bubblewrap` are Linux-only, need macOS/Windows story).

## 9. User-facing docs per slice

- v1.0.5: new section in `docs/filter.md` on "sensor calibration" + a new `docs/test-plans/` scenario set for the mutation/smell pipeline.
- v1.0.6: new doc `docs/bug-tracker-integration.md` covering providers, credentials, triage config.
- v1.1.0: new doc `docs/adversary-agent.md` covering budget, sandbox model, how to interpret counterexamples, and the ≥ 3-iteration damping rule.

## 10. Re-prioritisation of prior carry-overs

Current `docs/upcoming.md` items get re-sorted:

| Old priority | Item | New placement |
|---|---|---|
| #1 | MarkerEntry backwards-compat | Parked — affects only one old committed manifest; low value vs v1.0.5 |
| #2 | Motion-matrix-fit from (diff, test-delta) | **Subsumed by FR-EQ-004** in v1.0.5 |
| #3 | RBPF CLI routing | Parked — no user requested it yet |
| #4 | Long spec-ID table alignment | Parked — cosmetic, JSON output works |

After v1.1.0 ships, revisit whether any of these are still worth the effort given real-world evidence-quality data.

## 11. Immediate next steps

1. **Review + merge this plan + the proposal doc** (one PR). No code touched.
2. **File three tracking issues** — one per release slice — each referencing this plan and listing its FRs.
3. **v1.0.5 sub-issues** — FR-EQ-001 through FR-EQ-007, each a sub-issue of the v1.0.5 tracker.
4. **Start FR-EQ-001** as the first PR. Every subsequent FR-EQ-NNN PR closes its issue and references this plan.

---

*This plan lives at `docs/evidence-quality-plan.md`. Paired with `docs/proposals/v2-evidence-quality.md` (the research + design doc). Both land as one PR for reviewer context.*
