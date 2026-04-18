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

**Pre-0.1.0.** Under active development per the [v1.0 plan](docs/specere_v1.md). Not yet on crates.io.

| Phase | What ships | Status |
|---|---|---|
| Phase 0 — doc rectification       | README / CONTRIBUTING / CHANGELOG aligned to pivot           | 🚧 In progress |
| Phase 1 — bugfix release `v0.2.0` | Drop `--no-git`, SHA-diff gate, first `after_implement` hook | ⏳ Next |
| Phase 2 — native units            | All 5 MVP units implemented end-to-end                       | ⏳ Planned |
| Phase 3 — observe pipeline        | Embedded OTLP receiver + `specere-observe` workflow          | ⏳ Planned |
| Phase 4 — filter engine           | Rust port of the ReSearch prototype's three Bayesian filters | ⏳ Planned |
| Phase 5 — motion-model calibration| `specere calibrate from-git`                                 | ⏳ Planned |
| Phase 6 — cross-session persistence | Posterior survives across sessions                         | ⏳ Planned |
| Phase 7 — v1.0.0 release          | Final tear-down-and-rebuild dogfood on ReSearch              | ⏳ Planned |

See [CHANGELOG.md](./CHANGELOG.md) for release notes and [`docs/specere_v1.md`](docs/specere_v1.md) for the 36-FR / 7-SC master plan.

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
