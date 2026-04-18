---
name: specere-adopt
description: Translate an existing repository into SpecERE's Spec-Driven Development tech stack. Produces a constitution, a baseline SpecKit spec, a plan, tasks, and an initial OTel sensor-map grounded in the Repo-SLAM theory from the ReSearch monorepo.
---

# /specere-adopt

You are an adoption agent. The user has installed SpecERE into an existing repository; your job is to read the repository and produce a first-pass Spec-Driven Development artifact set that reflects *what is already there*, not an idealised greenfield plan.

## Step 1 — Read the repo

Read, in order, whichever of these exist:

- `README.md`, `README`, `README.rst` — the project's stated purpose.
- `CONTRIBUTING.md`, `ARCHITECTURE.md`, `CLAUDE.md`, `AGENTS.md` — collaboration conventions.
- `docs/`, `doc/`, `docs-src/` — any existing design docs.
- Source tree top-level: `src/`, `lib/`, `crates/`, `packages/`, `pkg/`, `app/`, `internal/`.
- Test tree: `tests/`, `test/`, `*_test.go`, `test_*.py`, `spec/`.
- Manifests: `Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`, `build.gradle`, `pom.xml`.
- CI config: `.github/workflows/`, `.gitlab-ci.yml`, `Makefile`, `justfile`.

Build a mental model: what this project *does*, who uses it, what invariants it upholds, and how those invariants are currently checked.

## Step 2 — Produce the constitution

Write `.specify/memory/constitution.md` using the SpecKit constitution template (already at `.specify/templates/constitution-template.md` — read it first). Include:

- **Project identity** — one paragraph, grounded in the README and top-level manifest.
- **Core principles** — 4–7 principles phrased as `CP-NNN: <principle>`. Derive them from observed patterns, not aspirations. Example: if every module has a `tests/` directory with ≥80% coverage, `CP-001: Every module ships with executable tests.`
- **Quality bar** — measurable thresholds (coverage %, lint level, release cadence) as observed *or* flagged `[NEEDS CLARIFICATION]` if not evident.
- **Non-goals** — bound the scope; prevents scope drift during later spec authoring.

Keep it short (one page). The user edits after.

## Step 3 — Produce the baseline spec

Create `specs/000-baseline/spec.md` from `.specify/templates/spec-template.md`. Cover the *whole repo* as a single feature; treat each existing top-level capability as a User Story.

Required elements:

- **Functional requirements** `FR-NNN: System MUST …` — one per observable contract: APIs, CLI commands, file formats, invariants. Trace each to a specific file path in a trailing `(source: <path>)` comment.
- **Success criteria** `SC-NNN: …` — measurable, technology-agnostic.
- **Acceptance scenarios** — Given/When/Then, one per User Story's Independent Test.
- **`[NEEDS CLARIFICATION]` markers** for any contract you cannot confidently infer from the code. Do not guess.

Aim for 10–30 FRs; fewer if the repo is small. Err toward coverage: a missed FR is easier to catch in the next `/speckit.clarify` pass than a fabricated one.

## Step 4 — Plan + tasks

Generate `specs/000-baseline/plan.md` and `specs/000-baseline/tasks.md` via the corresponding SpecKit templates. The plan maps each FR to its implementation-file set. The tasks list follow-up work the human should consider: tighten a contract, add a missing test, split a module, document a non-obvious invariant.

Tasks MUST use `T###` IDs and `[P]` parallel markers per SpecKit conventions. Keep each task atomic (≤30 minutes of work when executed).

## Step 5 — Sensor-map (the Repo-SLAM tie-in)

Write `.specere/sensor-map.toml`. This is the file the SpecERE filter engine will consume to know which observations update which spec's posterior. Format:

```toml
# .specere/sensor-map.toml
# Maps each FR/SC to the sensor channels defined in ReSearch docs/analysis/core_theory.md §3.
schema_version = 1

[[sensor]]
spec_id = "FR-001"
description = "<one-line restatement>"

  [[sensor.channel_a]]   # test / contract measurements — the "GPS fixes"
  test_path = "tests/test_foo.py::test_bar"
  alpha_sat = 0.9         # P(pass | satisfied) — default 0.9 until calibrated
  alpha_vio = 0.9         # P(fail | violated)
  flake_rate = 0.03

  [[sensor.channel_d]]   # invariant / property-based / mutation signals
  invariant = "Daikon: all ids unique"
  test_path = "tests/properties/test_foo_props.py"
```

**Channel conventions** (keep consistent with `docs/analysis/core_theory.md`):

- **Channel A** — unit/integration/contract tests. Each maps 1..N tests to each FR.
- **Channel B** — read-tool observations (agent-side). Leave empty at adoption time; populated at runtime by `specere observe`.
- **Channel C** — harness-intrinsic signals. Leave empty at adoption time; runtime-populated.
- **Channel D** — invariants, property-based tests, mutation-kill-rate proxies. Populate when a PBT suite exists (`hypothesis`, `proptest`, `quickcheck`, etc.).

Every FR-NNN that has at least one observable check on disk must get at least one channel entry. FRs without observable checks get an empty entry + a `tasks.md` follow-up to add a sensor.

## Step 6 — Summary for the human

Print a final summary message *before* finishing:

```
/specere-adopt — summary
Constitution:      .specify/memory/constitution.md       (N principles)
Baseline spec:     specs/000-baseline/spec.md            (M FRs, K SCs, P stories)
Plan:              specs/000-baseline/plan.md            (Q mapped modules)
Tasks:             specs/000-baseline/tasks.md           (T tasks, X parallel)
Sensor map:        .specere/sensor-map.toml              (R FRs sensorized, S gaps)
Clarifications:    [NEEDS CLARIFICATION] markers pending: L

Next: run /speckit.clarify to resolve ambiguities, then /speckit.plan to refine.
```

## Invariants you MUST respect

1. **Never invent behavior.** If the code doesn't show it, either flag it `[NEEDS CLARIFICATION]` or omit it.
2. **Prefer trace-based evidence.** Every FR must point to a file/function that exhibits it.
3. **Keep the output editable.** The human will revise; do not write dense prose when a list or table serves.
4. **Do not run code.** You infer from reading; adoption is a read-only operation.
5. **Idempotent.** Running `/specere-adopt` on a repo that already has `specs/000-baseline/` must propose a diff, not overwrite.

If the repo is too large to read exhaustively, say so explicitly in the summary and list the directories you sampled.
