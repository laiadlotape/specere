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
specere add claude-code-hooks    # emit OTLP telemetry on agent tool calls
specere add otel-collector       # local collector, scaffolded (no external deps)
specere add ears-linter          # enforce EARS-style requirement syntax
```

Every `add` has a reverse:

```bash
specere remove speckit --dry-run    # preview what `remove` would do
specere remove speckit              # strip exactly what we installed
```

This uninstall-first design is SpecERE's core UX differentiator versus SpecKit, Cursor rules, Aider conventions, and Kiro — none of which ship a repo-level uninstall. See the [SpecKit deep-dive](https://github.com/laiadlotape/ReSearch/blob/main/docs/research/08_speckit_deepdive.md) for the full comparison.

## Status

**Pre-0.1.0.** Under active scaffolding. Not yet on crates.io.

| Component                      | Status                                  |
|--------------------------------|------------------------------------------|
| Repo skeleton                  | ✅ Up (Rust workspace, CI, docs)          |
| `specere` CLI surface          | 🚧 Stub — `add`/`remove`/`status` wiring  |
| `specere add speckit`          | 🚧 In progress                            |
| `specere add filter-state`     | ⏳ Planned                                |
| `specere add claude-code-hooks`| ⏳ Planned                                |
| `specere add otel-collector`   | ⏳ Planned                                |
| `specere add ears-linter`      | ⏳ Planned                                |

See [CHANGELOG.md](./CHANGELOG.md) for release notes.

## Design

- Design brief: [ReSearch/docs/roadmap/31_specere_scaffolding.md](https://github.com/laiadlotape/ReSearch/blob/main/docs/roadmap/31_specere_scaffolding.md)
- SpecKit deep-dive: [ReSearch/docs/research/08_speckit_deepdive.md](https://github.com/laiadlotape/ReSearch/blob/main/docs/research/08_speckit_deepdive.md)
- Long-term roadmap: [ReSearch/docs/roadmap/30_long_term_tool.md](https://github.com/laiadlotape/ReSearch/blob/main/docs/roadmap/30_long_term_tool.md)

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
├── docs/                     # mdbook
├── examples/                 # dogfood fixtures
└── xtask/                    # release/dev chores
```

## Building

```bash
cargo build --release
./target/release/specere --help
```

## License

Apache-2.0. See [LICENSE](./LICENSE).
