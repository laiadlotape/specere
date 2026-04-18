# SpecERE documentation

All SpecERE planning, design, and capability-reference material.

## Start here

- **[`specere_v1.md`](specere_v1.md)** — the master plan: 7 phases, 36 FRs, 7 SCs, 20-step dogfood protocol. Governs all pre-1.0 work.

## Roadmap

- **[`roadmap/31_specere_scaffolding.md`](roadmap/31_specere_scaffolding.md)** — scaffolding design: `AddUnit` contract, manifest, marker fences, MVP unit list, validation loop.
- **[`roadmap/30_long_term_tool.md`](roadmap/30_long_term_tool.md)** — long-term vision beyond v1.0 (multi-harness, advanced calibration, filter-as-a-service).

## Research

Reference material SpecERE is built *on top of*. Both are deep-dives into [github/spec-kit](https://github.com/github/spec-kit).

- **[`research/08_speckit_deepdive.md`](research/08_speckit_deepdive.md)** — the first-pass capability tour (~2400 words). Why SpecERE wraps SpecKit, where the upstream gaps are.
- **[`research/09_speckit_capabilities.md`](research/09_speckit_capabilities.md)** — the exhaustive follow-up (~3500 words). Capability matrix (22 WRAP / 4 IGNORE / 15 EXTEND) and the 10-rule composition pattern that governs SpecERE's implementation choices.

## What's *not* here

Theory — Repo SLAM framing, the spec-belief filter formulation, the SRGM × LLM calibration story, the three novelty claims — lives in the [ReSearch](https://github.com/laiadlotape/ReSearch) monorepo under `docs/analysis/`, `docs/research/01-07`, and `prototype/`. SpecERE consumes it; SpecERE does not re-derive it.

Contribution and release conventions — see [`../CONTRIBUTING.md`](../CONTRIBUTING.md), [`../CHANGELOG.md`](../CHANGELOG.md), and [`../SECURITY.md`](../SECURITY.md).
