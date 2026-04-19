# Proposal: evidence-quality channels for SpecERE v2

**Status.** Draft, 2026-04-19. Awaiting user direction before we plan execution.
**Author.** Generated from user request + three parallel research passes (mutation testing, LLM adversarial generation, bug-tracker-as-telemetry). See § 8 References for URLs.
**Audience.** Project maintainers. This is a *design* document, not an implementation plan — implementation lands after the open questions in § 7 are resolved.

---

## 1. The problem, stated precisely

The SpecERE filter's posterior is only as trustworthy as the evidence feeding it. v1 has one calibrated sensor channel (Channel A: test pass/fail) with fixed constants:

```
P(pass | SAT) = α_sat = 0.92    # prototype default
P(fail | VIO) = α_vio = 0.90
P(pass | UNK) = α_unk = 0.55
```

These constants assume the test suite has **high discriminative power** — tests pass when the spec is satisfied and fail when it's violated. That assumption fails in three common modes:

| Failure mode | What happens | Effect on posterior |
|---|---|---|
| **Tautological tests** | `assert_eq!(x, x)`, over-mocking, missing assertions | `P(pass\|SAT) ≈ P(pass\|VIO)` → no signal; posterior stays near uniform even under heavy evidence |
| **Happy-path-only coverage** | Only the golden path exercised; edge cases untested | `P(pass\|VIO)` is close to `P(pass\|SAT)` → false confidence (inflated p_sat) |
| **Wrong tests (test bug)** | The test is checking the wrong property | Posterior converges to a satisfying belief for a wrong condition |

Combined with the fact that tests **can be wrong in ways we can't detect from their pass/fail alone**, this creates a systematic gap between the posterior and reality. A spec can reach `p_sat > 0.95` while being completely broken — the exact failure mode the user observed on an agentic-created repo.

Three orthogonal evidence channels the user proposed:

1. **Test-suite quality as sensor calibration.** Weak tests → looser sensor model → less confident posterior.
2. **Bug reports as an independent VIO channel.** A bug filed against code under a spec's support set is strong evidence of violation, regardless of what tests say.
3. **Adversarial agents that try to falsify SAT estimates.** If a "prove-me-wrong" agent can't find a counter-example despite trying hard, that's stronger evidence than a passive passing test suite.

This maps onto the original sensor-channel design in `docs/analysis/core_theory.md §3` (ReSearch): v1 activated channels A (tests), B (reads), C (harness-intrinsic). Channel D was deliberately deferred and documented as "invariants / PBT / mutation." The user's request activates Channel D and adds new channels E (bug reports) and F (adversary agents) that v1 didn't anticipate.

## 2. What the research found

Three parallel research passes. Full summaries + URLs in § 8. Headlines:

### 2.1 Mutation testing

- **`cargo-mutants`** (Martin Pool) is mature, `--json` output, per-file scoping (`--file`, `--re`), `--in-diff` for incremental PR runs. Nightly-full + per-PR-diff is the common pattern.
- **Empirical basis**: Just et al. (FSE 2014) — mutation score correlates with real-fault detection at **r ≈ 0.72**, vs r ≈ 0.44 for statement/branch coverage. Papadakis et al. (2018) confirms across multiple languages.
- **Practical target**: >80% kill rate on changed code; 90%+ for safety-critical. Absolute scores are less meaningful than per-spec deltas.
- **Speed**: `--in-diff` under 5 min typical; `--file <fr-support>` minutes. Full runs hours on medium crates.
- **Zero OTel prior art** — emitting mutation results as OTel spans is greenfield. Closest schema: the cross-tool `mutation-testing-elements` JSON (Stryker ecosystem).

### 2.2 LLM adversarial / counter-test generation

- **Meta TestGen-LLM** (arXiv:2402.09171): 75% correct builds, 57% pass reliably, 25% improve coverage, 73% accepted by Meta engineers. **Java only.**
- **Fuzz4All / TitanFuzz** (ICSE '24, ISSTA '23/'24): LLM-guided grey-box fuzzing found **98 new bugs across 9 systems, 64 CVEs**. The "spec + seed → unusual-but-valid input" pattern.
- **SWE-agent** (NeurIPS '24, arXiv:2405.15793): iterative falsification at **~$1.20–$2.80 per issue** with Claude Sonnet; 65% first-try → 3-iter reproducer generation rate.
- **Hallucination risk**: 18–31% of LLM-generated assertions test properties *not* in the spec (Schäfer et al. TSE '24). Liu et al. (ICSE '24): test validity drops from 73% → 34% under spec paraphrase.
- **No Rust-specific production tool exists.** The composable stack is `proptest` + LLM-generated `Strategy` + SWE-agent-style loop with a hard budget and ≥ 3-iteration rule.

### 2.3 Bug-tracker as telemetry

- **Traceability literature** (Cleland-Huang 2014, CoEST community): defect-to-requirement linkage is a canonical relation; only ~40% of issues link explicitly, the rest need inference (stack-trace parsing, NER, CODEOWNERS matching).
- **Signal-to-noise**: Herzig et al. (ICSE '13) — **33.8% of issues labeled "bug" are misclassified** (feature, refactor, question). 15–30% duplicates typical. Filtering heuristics (repro-step regex, non-question labels, stack-trace presence) help.
- **Time decay**: Kim et al. (ICSE '07) — ~50-day half-life optimal on Eclipse/Mozilla fault-prediction. Bug-tracker evidence should decay exponentially against this scale.
- **LLM triage**: GPT-4 at **82% F1** on bug/feature/question classification (MSR '24). Spec-violation mapping specifically has no public precedent but is structurally identical to solved problems.
- **APIs**: GitHub timeline + `Fixes #N` parsing is cheap; Jira Smart Commits + Linear GraphQL equivalents exist. Polling `since=` every N minutes is simpler than webhooks for hour-scale belief updates.

## 3. Design space — 6 approaches

Each has different scope, cost, and epistemic value. Not all need to ship together.

### Approach A: Mutation-testing Channel D (the already-planned extension)

- **What.** Wrap `cargo-mutants --json --in-diff` behind a `specere evaluate mutations` verb. Aggregate kill rate per FR via the sensor-map. Emit one span per mutant: `event_kind=mutation_result`, attrs: `spec_id`, `outcome=caught|missed|timeout|unviable`, `operator`, `file`, `line`.
- **Sensor integration.** Feed per-spec kill rate into a new `alpha_sat(spec_id)` calibration: high kill rate ⇒ near-default 0.92; low kill rate ⇒ compressed toward 0.55. Exact formula in § 5.
- **Cost.** Dev: ~300 LoC + fixture. Runtime: 5–15 min per PR via `--in-diff`.
- **Blocker**: requires a test suite that runs reliably — not always true for flaky / slow agentic repos.

### Approach B: Property-based testing Channel D

- **What.** Skill `specere-pbt-driver` that discovers `#[proptest]` / `#[quickcheck]` attributes in `tests/`, runs them, records trial count + shrunk counter-example. `event_kind=pbt_result`, attrs: `spec_id`, `trials_ok`, `failure_input`.
- **Sensor integration.** A surviving property → weak SAT evidence (absence-of-counterexample). A shrunk counterexample → strong VIO evidence.
- **Cost.** Dev: ~200 LoC if the repo already uses proptest; ~500 LoC with scaffolding to generate strategies.
- **Blocker**: Rust repos vary wildly in PBT adoption. Many have zero proptest coverage.

### Approach C: LLM adversary agent

- **What.** New agent `specere-adversary` (subagent, not skill — long-running and context-heavy). Inputs: a spec's FR text + its support files + current passing tests. Output: a set of proposed failing inputs / test cases. Pass an iterative falsification loop (up to 5 iterations, hard $2 budget). Each iteration: generate → run in sandbox → if pass, retry with different angle; if fail, minimize + emit `event_kind=counterexample_found`.
- **Sensor integration.** A confirmed counter-example is VIO evidence roughly equivalent to a real bug report. Budget-exhausted-without-finding is weak SAT evidence.
- **Cost.** Dev: ~800 LoC (agent harness + sandbox + minimizer). Runtime: $1–$5 per FR per run. Damping: require ≥ 3 iterations before any posterior update (hallucination guard per Liu '24).
- **Blocker**: sandbox correctness (running LLM-generated code must not escape), hallucination rate (damping only reduces it).

### Approach D: Bug-tracker bridge (Channel E)

- **What.** New subcommand `specere observe watch-issues --provider github --repo owner/name`. Polls `GET /issues?since=<cursor>` every 10 minutes. For each new issue: filter (is-bug-label? has-repro? not-duplicate?), LLM-triage which spec it hits (embeddings + rerank), emit `event_kind=bug_reported`, attrs: `spec_id`, `severity`, `issue_url`, `age_days`.
- **Sensor integration.** Independent VIO channel with exponential decay (50-day half-life default). A closed issue decays faster than an open one. Severity scales the update magnitude.
- **Cost.** Dev: ~600 LoC + credentials story. GitHub only in v1; GitLab/Linear/Jira in v2.
- **Blocker**: false-positive rate (33.8% misclassification + 15–30% dupes). LLM-triage cost: pennies per issue but adds up on high-traffic projects.

### Approach E: Test-smell static analysis

- **What.** A new `specere lint tests` that statically analyzes tests for known smells: tautological assertions, no-assertion tests, over-mocked (all-mock) tests, happy-path-only (single fixture per fn). For each smell detected, degrade the sensor alphas for that spec's tests when the filter runs.
- **Sensor integration.** Per-spec `quality_multiplier` ∈ [0.3, 1.0] applied to `α_sat − α_vio` difference. Low multiplier compresses the sensor toward uninformative.
- **Cost.** Dev: ~400 LoC (rust-syntax-tree walker + rule engine). No runtime cost (static).
- **Blocker**: false-positive smell detection. Some "tautological" tests are intentional (e.g., serialization round-trip).

### Approach F: Human-in-the-loop adjudication

- **What.** Extend the existing review-queue pattern (constitution V) to surface "high-posterior + suspicious evidence" specs for human sanity-check. E.g., `p_sat > 0.95` with `mutation_kill_rate < 0.4` → add to `.specere/review-queue.md` with "passive confidence but weak evidence" flag.
- **Sensor integration.** No direct posterior change — this is a UX gate, not an evidence channel. But the user's sign-off (or "actually, broken") becomes a high-confidence manual event.
- **Cost.** Dev: ~100 LoC.
- **Blocker**: none; this is orthogonal polish.

## 4. Recommended approach — staged integration

The user asked for **one** upgrade. The right shape, combining the three concerns raised (weak tests / bug reports / counter-testing agents) into a coherent v1.1 → v2.0 arc:

**v1.1 — Mutation as calibration (Approach A + E + F).** The foundation. Introduces the "sensor alphas are computed per-spec" machinery that everything else plugs into. Scope: ~900 LoC. Ships a working "weak tests degrade confidence" story without any LLM cost or external API.

**v1.2 — Bug-tracker bridge (Approach D).** An independent evidence channel — high ROI because it activates the existing event pipeline with zero filter-engine changes. Scope: ~600 LoC. Requires GitHub credentials but no paid APIs beyond GitHub.

**v2.0 — Adversary agent (Approach C).** The biggest and most ambitious. Relies on v1.1's calibration machinery to weigh its output correctly. Has real sandbox + hallucination risks. Scope: ~800 LoC + ongoing LLM spend.

Property-based testing (Approach B) is deferred because adoption is uneven across Rust repos; it becomes a Channel D variant users can opt into when they have proptest coverage.

Total: ~2300 LoC across three minor releases over ~6 sessions. Each release is independently useful.

## 5. Sensor calibration formula — concrete proposal

Given a spec `s` at time of filter run, compute per-spec alphas from per-channel evidence:

```
kill_rate(s)        ∈ [0, 1]    # Channel D: mutation kill rate
smell_penalty(s)    ∈ [0, 1]    # Channel E-static: 1.0 = no smells, 0.3 = severe
bug_density(s, t)   ∈ [0, ∞)    # Channel E-runtime: decay-weighted bugs/month
adversary_flag(s)   ∈ {0, 1}    # Channel F: was a counter-example found?

# Baseline alphas from prototype
α_sat_0 = 0.92
α_vio_0 = 0.90
α_unk_0 = 0.55

# Effective test quality — clamp to [0.3, 1.0] so a broken test suite
# doesn't disable evidence entirely (still tracks UNK → SAT drift slowly).
q(s) = clamp(0.3, kill_rate(s) * smell_penalty(s), 1.0)

# Sensor compression: weak tests pull α_sat and α_vio toward α_unk.
α_sat(s) = α_unk_0 + q(s) * (α_sat_0 - α_unk_0)    # 0.55 + q*(0.37)
α_vio(s) = 1 - α_unk_0 + q(s) * (α_vio_0 - (1 - α_unk_0))    # 0.45 + q*(0.45)

# Bug density is a pure VIO injection — independent of test-quality.
# Modeled as a direct posterior nudge, not via the sensor alphas.
# (Decay-weighted per-event update through channel E; see § 6.)

# Adversary counter-example is a high-confidence VIO event —
# same treatment as a bug report with severity=critical.
```

This preserves v1 behaviour exactly when kill rate = 1.0 and no smells (`q = 1.0` ⇒ alphas at prototype defaults). It smoothly compresses toward uninformative as quality drops.

## 6. Event schema additions

Proposed `event_kind` values for the event store:

| `event_kind` | Required attrs | Channel | Treatment |
|---|---|---|---|
| `test_outcome` | `spec_id`, `outcome=pass\|fail` | A | existing |
| `files_touched` | `paths` | motion | existing |
| `mutation_result` | `spec_id`, `outcome=caught\|missed\|timeout\|unviable`, `operator`, `file`, `line` | D | aggregate to `kill_rate` |
| `test_smell_detected` | `spec_id`, `smell_kind`, `severity`, `test_fn` | E-static | aggregate to `smell_penalty` |
| `bug_reported` | `spec_id`, `issue_url`, `severity=critical\|major\|minor`, `age_days` | E-runtime | VIO injection with decay |
| `counterexample_found` | `spec_id`, `input_minimized`, `test_fn`, `iterations`, `cost_usd` | F | VIO injection |
| `adversary_budget_exhausted` | `spec_id`, `iterations`, `cost_usd` | F | weak SAT signal |

All new event kinds are additive. Events with unknown `event_kind` continue to be counted in `skipped` (existing behaviour).

## 7. Open questions — please answer before we plan execution

1. **Scope for the first slice.** Do we tackle v1.1 (mutation + smells) alone as one PR, or bundle in v1.2 (bug-tracker) to ship a bigger story?
2. **LLM cost ceiling.** If we build Approach C later, do you want a hard monthly budget (e.g. "≤ $20/month across all adversary runs"), a per-PR budget, or deferred-until-proven? Free tier is adversary-disabled.
3. **Test-smell severity.** Should `assert_eq!(x, x)` be an ERROR (block `filter run` until the test is fixed) or INFO (degrade posterior but proceed)? I lean INFO — the filter's whole job is "honest about uncertainty."
4. **Bug-tracker provider priority.** GitHub first is obvious. Beyond that: Linear? Jira? GitLab? Which matters most to your real targets (specere itself, ReSearch, memaso, any others)?
5. **Human-in-loop adjudication UX.** The review-queue pattern works for divergence adjudication today. Extend that, or a new `specere doctor --suspicious` verb, or both?
6. **Motion calibration integration.** v0.5.0 shipped coupling-edge calibration from git; the corresponding motion-matrix fit was deferred because "we don't have test-history." Mutation + bug-tracker channels give us that history. Should v1.1 ALSO finally wire up `specere calibrate motion-from-evidence`? It's a natural companion.
7. **Proposal-to-implementation gate.** Once questions 1–6 are answered, do you want me to produce a full FR-numbered execution plan (like Phase 4's) for review before touching code, or go straight from here to an implementation PR?

## 8. References

**Mutation testing**
- Martin Pool, `cargo-mutants`: https://github.com/sourcefrog/cargo-mutants
- Stryker mutation-testing-elements schema: https://github.com/stryker-mutator/mutation-testing-elements
- Just et al., "Are Mutants a Valid Substitute for Real Faults?" FSE 2014: https://people.cs.umass.edu/~rjust/publ/mutants_real_faults_fse_2014.pdf
- Papadakis et al., "Mutation Testing Advances" (2018): https://doi.org/10.1016/bs.adcom.2018.03.015
- Petrović & Ivanković, "State of Mutation Testing at Google" ICSE-SEIP 2018: https://research.google/pubs/pub46584/

**LLM adversarial testing**
- Alshahwan et al., "Automated Unit Test Improvement using LLMs at Meta" (TestGen-LLM): https://arxiv.org/abs/2402.09171
- Yang et al., "SWE-agent" NeurIPS '24: https://arxiv.org/abs/2405.15793
- Xia et al., "Fuzz4All" ICSE '24: https://arxiv.org/abs/2308.04748
- Deng et al., "TitanFuzz" ISSTA '23: https://arxiv.org/abs/2304.02014
- Schäfer et al., "LLM-based test-generation reliability" TSE '24
- Liu et al., "Test validity under spec paraphrase" ICSE '24: https://arxiv.org/abs/2305.01210

**Bug-tracker as telemetry**
- Cleland-Huang et al., "Software Traceability" FOSE 2014
- Herzig et al., "It's Not a Bug, It's a Feature" ICSE 2013: https://doi.org/10.1109/ICSE.2013.6606585
- Zhou et al., "BugLocator" ICSE 2012: https://doi.org/10.1109/ICSE.2012.6227210
- Śliwerski-Zimmermann-Zeller, "SZZ algorithm" MSR 2005: https://doi.org/10.1145/1083142.1083147
- Kim et al., "Predicting Faults from Cached History" ICSE 2007
- Lee et al., "LLM-based Bug Report Classification" MSR 2024
- CoEST community: https://coest.org
- OTel CI/CD Semantic Conventions: https://opentelemetry.io/docs/specs/semconv/cicd/

## 9. What happens next

I'll now use the interactive questionnaire (AskUserQuestion) to walk through the six open questions in § 7. Your answers become the basis for the actual execution plan — mirrors the Phase 4 pattern where the plan doc lands *before* any code.

---

*This proposal is a read-only research artefact. No code has been touched. The document lives at `docs/proposals/v2-evidence-quality.md` (uncommitted until we agree on scope).*
