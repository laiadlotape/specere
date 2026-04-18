# Changelog

All notable changes to SpecERE will be documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project aims to adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once 0.1.0 ships.

## [Unreleased]

### Added
- Initial Rust workspace skeleton: `specere`, `specere-core`, `specere-units`, `specere-manifest`, `specere-markers`, `specere-telemetry`.
- CLI surface stubs: `add`, `remove`, `status`, `verify`, `doctor`, `observe`, `version`.
- `AddUnit` trait in `specere-core` with the six-tuple contract (preflight, install, postflight, remove, manifest, idempotency).
- TOML manifest schema in `specere-manifest`.
- Marker-fenced shared-file editing in `specere-markers`.
- CI (fmt, clippy, test), dependabot, Apache-2.0 LICENSE.
- `Deploy` trait in `specere-units::deploy` and `claude-code-deploy` concrete implementation; embedded `/specere-adopt` skill.
- Wrapper-unit shape for `speckit` (minimal manifest; delegates file tracking to SpecKit's `.specify/integrations/`).

### Changed
- Renamed `claude-code-hooks` → `claude-code-deploy` to reflect its role as a per-harness deployer, not a single-purpose hook installer.
- Manifest distinguishes `[wrapper]` vs `[native]` unit shapes in `specere status` output.

### Decided (2026-04-18 pivot)
- **SpecERE is the primary deliverable.** Prior framing of "companion tool to ReSearch" is retired; the master plan lives at [`docs/specere_v1.md`](docs/specere_v1.md). Seven phases to v1.0.0 over 20-24 weeks.
- **Compose, never clone.** Every capability decides WRAP/IGNORE/EXTEND against SpecKit + OTel. Rule and 10-rule composition pattern at [`docs/research/09_speckit_capabilities.md`](docs/research/09_speckit_capabilities.md) §§12-13 govern all implementation choices.
- **Claude Code only for v1.0.** Cursor / Aider / OpenCode / Codex deployers deferred to v1.x.
- **ReSearch is the dogfood target.** The paper and foundational booklet over in [`laiadlotape/ReSearch`](https://github.com/laiadlotape/ReSearch) are held pending SpecERE v1.0; SpecERE will be tear-down-and-rebuild-verified on that repo before tagging v1.0.0.

### Moved in (2026-04-18)
- `docs/roadmap/30_long_term_tool.md` — long-term vision (was in ReSearch).
- `docs/roadmap/31_specere_scaffolding.md` — scaffolding design (was in ReSearch).
- `docs/research/08_speckit_deepdive.md` — first SpecKit capability survey (was in ReSearch).
- `docs/research/09_speckit_capabilities.md` — exhaustive capability reference (was in ReSearch).

### Planned for v0.2.0 (Phase 1 bugfix release)
- Drop `--no-git` from `specere add speckit` on git repos; auto-create feature branch `000-baseline`.
- SHA-diff gate on re-install (refuse to overwrite user-edited files unless `--adopt-edits`).
- Gitignore `.claude/settings.local.json` via marker-fenced block.
- First real `after_implement` hook in `.specify/extensions.yml` pointing at `specere.observe.implement`.

See [`docs/specere_v1.md`](docs/specere_v1.md) §5.P1 for the full Phase 1 scope and FRs.
