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

**v1.0.3 on `main`** — all seven master-plan phases shipped. `specere calibrate from-git` surfaces real architectural coupling on live repos. Full Gate-A parity with the ReSearch Python prototype (bit-identical for PerSpecHMM + FactorGraphBP; tail-MAP-within-2-pp for RBPF).

Not on crates.io yet; install from the [GitHub Release](https://github.com/laiadlotape/specere/releases/latest) shell / powershell installer.

| Phase | What ships | Status |
|---|---|---|
| Phase 0 — doc rectification | README / CONTRIBUTING / CHANGELOG aligned to pivot | ✅ Shipped (2026-04-18) |
| Phase 1 — bugfix release | Drop `--no-git`, SHA-diff gate, first `after_implement` hook, marker-fenced `.gitignore`, bit-identical remove, parse-safety | ✅ v0.2.0 (2026-04-18) — 9 FRs, 37/37 tests |
| Phase 2 — native units | All 5 MVP units implemented end-to-end | ✅ Shipped (2026-04-18) — 5 units real; `specere init` composes the full scaffold; 65/65 tests |
| Phase 3 — observe pipeline | Embedded OTLP receiver + `specere-observe` workflow | ✅ v0.4.0 (2026-04-18) — OTLP HTTP + gRPC + SQLite event store + 13 workflow-span hooks; FR-P3-001 through FR-P3-006 closed |
| Phase 4 — filter engine | Rust port of the ReSearch prototype's three Bayesian filters | ✅ v0.4.0 / v0.4.0 follow-ups — PerSpecHMM + FactorGraphBP + RBPF + `specere filter run/status` CLI; FR-P4-001 through FR-P4-006 closed; Python-prototype parity bit-identical on Gate-A for PerSpecHMM + BP |
| Phase 5 — motion-model calibration | `specere calibrate from-git` | ✅ v0.5.0 (partial) — coupling-edge suggester from git log co-modification; full motion-matrix fit deferred (needs test-history source) |
| Phase 6 — cross-session persistence | Posterior survives across sessions | ✅ v1.0.0 — posterior bit-identical across process restarts; FR-P6 regression caught + fixed |
| Phase 7 — v1.0.0 release | Final tear-down-and-rebuild dogfood on ReSearch | ✅ v1.0.0 (2026-04-18); v1.0.1 calibrate path-prefix fix; v1.0.2 RBPF/BP parity closes #42; v1.0.3 ears-lint parser hardening closes #61 |

**Release:** current stable is **v1.0.3**. See [CHANGELOG.md](./CHANGELOG.md) for the full history.

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
