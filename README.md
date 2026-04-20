# SpecERE

> **Spec Entropy Regulation Engine.** Composable, reversible Repo-SLAM scaffolding for AI coding agents.
>
> Latin `specere` = "to look / to observe." The tool observes agent and repository activity and maintains a posterior over specification satisfaction.

[![crates.io](https://img.shields.io/crates/v/specere.svg)](https://crates.io/crates/specere)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](./LICENSE)

## What this is

`specere` is a Rust CLI that installs *Repo-SLAM scaffolding* into an existing repository. It is the engineering counterpart to the [ReSearch](https://github.com/laiadlotape/ReSearch) research monorepo, where the theory and paper live.

The scaffolding is **composable** — each capability is an `add` unit you install or remove on its own:

```bash
specere add speckit              # wrap github/spec-kit into the repo
specere add filter-state         # create .specere/ with manifest
specere add claude-code-deploy   # install the Claude Code skills + hooks
specere add otel-collector       # local collector, scaffolded (no external deps)
specere add ears-linter          # advisory EARS-style requirement linter
```

Every `add` has a reverse:

```bash
specere remove speckit --dry-run    # preview what `remove` would do
specere remove speckit              # strip exactly what we installed
```

This uninstall-first design is SpecERE's core UX differentiator versus SpecKit, Cursor rules, Aider conventions, and Kiro — none of which ship a repo-level uninstall. See the [SpecKit deep-dive](docs/research/08_speckit_deepdive.md) for the full comparison.

## Status

**v1.2.0 feature-complete on `main`** (not yet tagged) — the **harness manager & inspector** upgrade lands on top of the v1.0.5 evidence-quality work. All seven master-plan phases remain shipped. 358 workspace tests.

Not on crates.io yet; install from the [GitHub Release](https://github.com/laiadlotape/specere/releases/latest) shell / powershell installer.

### Master plan (v0.1 → v1.0.0)

| Phase | What ships | Status |
|---|---|---|
| Phase 0 — doc rectification | README / CONTRIBUTING / CHANGELOG aligned to pivot | ✅ Shipped (2026-04-18) |
| Phase 1 — bugfix release | Drop `--no-git`, SHA-diff gate, first `after_implement` hook, marker-fenced `.gitignore`, bit-identical remove, parse-safety | ✅ v0.2.0 — 9 FRs, 37/37 tests |
| Phase 2 — native units | All 5 MVP units implemented end-to-end | ✅ Shipped — 5 units real; `specere init` composes the full scaffold |
| Phase 3 — observe pipeline | Embedded OTLP receiver + `specere-observe` workflow | ✅ v0.4.0 — OTLP HTTP + gRPC + SQLite event store + 13 workflow-span hooks |
| Phase 4 — filter engine | Rust port of the ReSearch prototype's three Bayesian filters | ✅ PerSpecHMM + FactorGraphBP + RBPF; Python-prototype parity bit-identical on Gate-A for PerSpecHMM + BP |
| Phase 5 — motion-model calibration | `specere calibrate from-git` + motion-from-evidence fit | ✅ v0.5.0 coupling-edge suggester + v1.0.5 `calibrate motion-from-evidence` |
| Phase 6 — cross-session persistence | Posterior survives across sessions | ✅ v1.0.0 — posterior bit-identical across process restarts |
| Phase 7 — v1.0.0 release | Tear-down-and-rebuild dogfood on ReSearch | ✅ v1.0.0; v1.0.1–1.0.4 bugfix follow-ups |

### Post-v1.0 upgrades

| Track | What ships | Status |
|---|---|---|
| **v1.0.5 evidence-quality** | Mutation-calibrated sensors, test-smell detector, motion-from-evidence fit, suspicious-SAT review queue. FR-EQ-001..007. | ✅ All 7 FRs on main (PRs #88–#92) |
| **v1.2.0 harness manager** | Classify + inspect every test/bench/fuzz/mock/workflow file; provenance + git history + coverage + flakiness + Leiden clustering; OTel semconv; ratatui TUI. FR-HM-001..072. | ✅ All 30 FRs on main (PRs #94, #96–#101) |
| v1.0.6 bug-tracker bridge | GitHub + Gitea issue → posterior. FR-EQ-010..013. | ⏸ Queued |
| v1.1.0 LLM adversary | Budgeted counter-test generator. FR-EQ-020..024. | ⏸ Queued |
| v2.0.0 GUI | Tauri v2 + Sigma.js 6-screen inspector. FR-HM-080..085. | ⏸ Not yet started |

### Harness-manager CLI (v1.2.0)

The harness tree is reachable via a single `specere harness` command group:

```
specere harness scan                    # classify every file into nine categories
specere harness provenance              # link files to /speckit-* verbs + git commits
specere harness history                 # churn, age, hotspot score, co-modification PPMI
specere harness coverage --from-lcov-dir <path>   # per-test Jaccard on line hits
specere harness flaky --from-runs <path>          # co-failure PPMI + Meta flakiness
specere harness cluster --emit-to-sensor-map      # Louvain community detection
specere harness tui                     # interactive ratatui inspector
```

Every verb writes into `.specere/harness-graph.toml` and emits a `harness_*_completed` event to `.specere/events.jsonl` per the [OTel supplementary semantic convention](docs/otel-specere-semconv.md).

**Release:** current stable is **v1.0.4** on [GitHub Releases](https://github.com/laiadlotape/specere/releases). v1.0.5 + v1.2.0 entries accumulate under `[Unreleased]` in [CHANGELOG.md](./CHANGELOG.md) and will ship as one tagged release once the user calls the cut.

**Test plans for contributors:**
- [`docs/test-plans/self-dogfood-guide.md`](docs/test-plans/self-dogfood-guide.md) — 38-scenario CLI-driven smoke suite, ~25 min.
- [`docs/test-plans/agentic-integration-plan.md`](docs/test-plans/agentic-integration-plan.md) — interactive Claude Code session validating hooks + filter end-to-end.

See [`docs/specere_v1.md`](docs/specere_v1.md) for the 36-FR / 7-SC master plan.

## Design documents

All SpecERE planning and capability reference material is now in this repo.

- **Master plan:** [`docs/specere_v1.md`](docs/specere_v1.md) — 7 phases, 36 FRs, 20-step dogfood protocol.
- **Scaffolding design:** [`docs/roadmap/31_specere_scaffolding.md`](docs/roadmap/31_specere_scaffolding.md) — the `AddUnit` contract, manifest, marker fences, MVP unit list.
- **Long-term vision:** [`docs/roadmap/30_long_term_tool.md`](docs/roadmap/30_long_term_tool.md) — phases beyond v1.0.
- **SpecKit deep-dive:** [`docs/research/08_speckit_deepdive.md`](docs/research/08_speckit_deepdive.md) — the incumbent we compose over.
- **SpecKit capabilities:** [`docs/research/09_speckit_capabilities.md`](docs/research/09_speckit_capabilities.md) — the 22 WRAP / 4 IGNORE / 15 EXTEND matrix and 10-rule composition pattern.

Theory lives in [ReSearch](https://github.com/laiadlotape/ReSearch) (`docs/analysis/`, `docs/research/01-07`, `prototype/`, `latex/`). SpecERE consumes it; SpecERE does not re-derive it.

## Repo layout

```
specere/
├── crates/
│   ├── specere/              # binary: CLI, command dispatch
│   ├── specere-core/         # AddUnit trait, Ctx, Plan, Record
│   ├── specere-units/        # the five day-one add units
│   ├── specere-manifest/     # .specere/manifest.toml load/save
│   ├── specere-markers/      # marker-fenced shared-file editing
│   └── specere-telemetry/    # OTel receiver + `specere observe`
├── docs/
│   ├── specere_v1.md         # the master plan (7 phases, 36 FRs)
│   ├── roadmap/              # scaffolding design + long-term vision
│   └── research/             # SpecKit deep-dives (08, 09)
└── .github/workflows/        # CI (fmt, clippy, test on linux/mac/windows)
```

## Building

```bash
cargo build --release
./target/release/specere --help
```

## License

Apache-2.0. See [LICENSE](./LICENSE).
