<!-- SPECKIT START -->
For additional context about technologies to be used, project structure,
shell commands, and other important information, read the current plan
<!-- SPECKIT END -->

<!-- specere:begin harness -->
## SpecERE harness

This repo is its own dogfood target. The `specere-observe` workflow wraps
`/speckit-*` commands with OTel GenAI spans (via `.specify/extensions.yml`
hooks) and a post-implement review-queue drain.

**Authoritative docs**
- `.specify/memory/constitution.md` — the 10-rule composition pattern + the
  human-in-the-loop discipline from `docs/analysis/core_theory.md` §4
  (in the ReSearch repo).
- `docs/specere_v1.md` — the 7-phase / 36-FR master plan.
- `docs/research/09_speckit_capabilities.md` — the 22 WRAP / 4 IGNORE /
  15 EXTEND capability matrix.

**Human-in-the-loop rule.** You are **not** in the per-tool-call loop.
Questions go to the human only at the four core_theory §4 layers:
spec authorship, contract authoring, prior setting, divergence adjudication.
The workflow gates (`review-spec`, `review-plan`, `divergence-adjudication`)
are the implementation of this rule.

**Harness self-extension rule (constitution V).** When you observe a new
write surface, hook verb, or OTel span not covered by `.specere/sensor-map.toml`
or `.specere/manifest.toml`, append to `.specere/review-queue.md`. Do not
drain silently.

**Current branch.** `000-harness` — harness scaffold work, not feature work.
<!-- specere:end harness -->

