# SpecERE Constitution

**Status.** Active from 2026-04-18, branch `000-harness`.
**Source of truth.** `docs/research/09_speckit_capabilities.md` §13 (the 10-rule composition pattern) + `docs/analysis/core_theory.md` §4 (human-in-the-loop layer).
**Amendment rule.** Any change requires a `/speckit-constitution` pass plus a new entry in `CHANGELOG.md` under `[Unreleased] → Constitution`.

## Core Principles

### I. Compose, Never Clone (NON-NEGOTIABLE)
Every capability answers one question before implementation: *is this something SpecKit or OpenTelemetry GenAI semconv already does?* **If yes: WRAP.** **If not: EXTEND** without editing the upstream-owned file. **If it's fluff: IGNORE.** Reimplementation is a bug, not a feature. The 22 WRAP / 4 IGNORE / 15 EXTEND capability matrix is the authoritative decision reference.

### II. The Ten Composition Rules (NON-NEGOTIABLE)
These govern every line of code SpecERE ships. A tool call that violates any one of them is a bug.

1. **Installer detects ambient git-kind.** On a git repo, never pass `--no-git`; auto-create a feature branch (`000-baseline` by default, overridable). Never `--force` without a SHA-diff step.
2. **Hook registration is the only runtime attach point.** SpecERE never embeds dispatch logic into slash-command prompts. All hooks live in `.specify/extensions.yml`.
3. **Template overrides go only in `.specify/templates/overrides/`.** SpecERE never edits files under `.specify/templates/` directly.
4. **Context-file ownership uses marker-fenced blocks.** `<!-- specere:begin {unit-id} --> … <!-- specere:end {unit-id} -->`, one pair per unit, in any file SpecERE co-owns with SpecKit or the user. Content outside the fence is never touched.
5. **`.specere/sensor-map.toml` is SpecERE-native.** Nothing else reads or writes it.
6. **One SpecKit-registered workflow.** `specere-observe`, registered via `specify workflow add`. No parallel orchestrator.
7. **Namespacing.** SpecERE slash commands are `specere-*`. Never reuse or rename `speckit-*`.
8. **Uninstall consults `.specere/manifest.toml`.** SHA256 match required; preserves user-edited files; delegates SpecKit core removal to `specify integration uninstall`.
9. **Update is user-confirmed.** `specere update speckit` probes the pinned version and invokes `uv tool upgrade specify-cli` + `specify integration update <key>` only after explicit confirmation.
10. **Parse narrowly.** SpecERE parses `.specify/extensions.yml` (YAML) and `.specere/*.toml` (TOML). Every other SpecKit file is opaque.

### III. Reversible Units (NON-NEGOTIABLE)
Every `add` unit — wrapper or native — ships with a `remove` that is a true inverse. Tests MUST round-trip: `add X && remove X` produces a bit-identical tree (modulo files marked `Owner::UserEditedAfterInstall`). No half-finished removals.

### IV. Human-in-the-Loop Discipline
Per `docs/analysis/core_theory.md` §4, humans participate at four layers and nowhere else:
- **Authoring specs** — defines the state space; no filter fabricates this.
- **Authoring contracts, tests, invariants** — defines the sensor array.
- **Setting priors** — initial `p(s₀)`, critical-spec weights, flake-tolerance per test.
- **Adjudicating divergences** — when posterior disagrees with reality.

The human is **explicitly not** in the per-tool-call loop. The filter does that. Workflow gates exist at `plan` (priors) and post-`implement` (divergence adjudication), not between every verb. Asking a human at a non-§4 layer is a UX bug.

### V. Harness Self-Extension Detection (NON-NEGOTIABLE)
Any observed write surface, slash-command verb, OTel span, or test outcome not covered by `.specere/sensor-map.toml` or `.specere/manifest.toml` MUST be recorded in `.specere/review-queue.md` with `{surface, first_seen, last_seen, sample_payload, suggested_action}`. The post-`implement` workflow gate blocks on a non-empty queue. Decisions (EXTEND / IGNORE / ALLOWLIST / ADJUDICATE) log to `.specere/decisions.log`. The harness notifies the human; the human adjudicates; the harness learns.

## Engineering Constraints

- **Single static Rust binary.** No Python, no Node in shipped artifacts. `uvx`-installed `specify-cli` is the one exception, invoked via sub-process.
- **SpecKit v0.7.3 pin.** Documented in `.specere/manifest.toml → units.speckit.version`. Upgrades are user-confirmed.
- **OTel GenAI semantic conventions.** All telemetry emits `gen_ai.*` attributes per the 2026-04 spec. Custom attributes are prefixed `specere.*`.
- **EARS-style requirements.** Every `FR-NNN` uses the When/While/Where/If pattern. The `ears-linter` unit runs advisory in v1 (warn, don't block).
- **No `--no-git` on git repos.** Ever. §6 of `docs/research/09_speckit_capabilities.md`.

## Governance

- **Workflow.** `specere-observe` is the canonical run (`specify workflow run specere-observe`). Gates at `review-spec`, `review-plan`, and the new `divergence-adjudication` step (post-implement).
- **Release cadence.** `cargo-dist` drives releases; tags are the single source of truth. CI must pass fmt + clippy + test + cross-compile before merge to `main`.
- **Phase discipline.** The 7-phase plan in `docs/specere_v1.md` governs all pre-v1.0 work. Phase closure is signaled by a CHANGELOG entry + the FR table's rightmost column turning green.

**Version:** 1.0.0 | **Ratified:** 2026-04-18 | **Last Amended:** 2026-04-18
