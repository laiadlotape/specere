# Contributing to SpecERE

SpecERE is early; contributions are welcome but the design is still solidifying. **Start here for non-trivial contributions:** [`docs/contributing-via-issues.md`](docs/contributing-via-issues.md) — the bug/flaw/feature → issue → sub-issues → PR pipeline. Please open an issue before large changes, and read [`docs/specere_v1.md`](docs/specere_v1.md) for the phase plan before proposing anything outside Phase N.

## The governing rule: compose, never clone

Every feature proposal answers one question before it lands: *is this something [SpecKit](https://github.com/github/spec-kit) or [OTel](https://opentelemetry.io/docs/specs/semconv/gen-ai/) already does?*

- **If yes → WRAP.** Shell out, delegate, consume. Do not reimplement. `specere add speckit` calling `uvx specify-cli init` is the archetype.
- **If no → EXTEND.** Overlay the capability without editing upstream files. SpecERE's manifest (`.specere/manifest.toml`), its sensor map (`.specere/sensor-map.toml`), its filter engine, and its cross-session persistence are all extensions because upstream has no equivalent.
- **If it's fluff → IGNORE.** Explicitly.

Full capability matrix and 10-rule composition pattern: [`docs/research/09_speckit_capabilities.md`](docs/research/09_speckit_capabilities.md) §§12-13. The matrix has 22 WRAP / 4 IGNORE / 15 EXTEND entries today; new rows get added as new capabilities are proposed.

**Enforcement.** PRs are expected to cite a matrix row (or add one). Any write path landing inside `.specify/templates/` that isn't `.specify/templates/overrides/` is a violation; use the override stack instead.

## Conventions worth knowing

- **Dot-vs-hyphen in slash commands.** SpecKit authors hook commands in dot form in YAML (`speckit.constitution`, `specere.observe.implement`). Claude Code skills forbid dots in identifiers, so the commands render as hyphens at install time (`/speckit-constitution`, `/specere-observe-implement`). Both forms are correct in their layer — YAML: dots; skill filenames + slash names: hyphens. Don't "fix" one to match the other.
- **Marker fences** are the only way SpecERE edits files it co-owns with someone else (e.g. `CLAUDE.md`, `.gitignore`). Use `<!-- specere:begin {unit-id} -->` / `<!-- specere:end {unit-id} -->` pairs, one pair per installed unit. Content outside markers is untouchable. (SpecKit adopts the same convention in [PR #2259](https://github.com/github/spec-kit/pull/2259); our pattern is convergent, not novel.)
- **Namespacing.** All SpecERE slash commands are `specere-*`. Never reuse or extend `speckit-*`.
- **Hooks over prompt embedding.** SpecERE's runtime behavior on `/speckit-*` boundaries lives in `.specify/extensions.yml`. Never embed dispatch logic into slash-command prompts (that's SpecKit's drift tax; don't inherit it).
- **Parse narrowly.** SpecERE only parses `.specify/extensions.yml` (YAML) and `.specere/*.toml` (TOML). Every other SpecKit file is opaque.

## Local development

```bash
rustup toolchain install stable
git clone https://github.com/laiadlotape/specere
cd specere
cargo build
cargo test
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

`uvx` (from [`uv`](https://github.com/astral-sh/uv)) is a runtime dependency for integration tests that exercise `specere add speckit`. Install it before running the full test suite.

## Adding a new `add` unit

1. Decide **native** or **wrapper** first:
   - **Native** = SpecERE owns every file, records each with SHA256 in the manifest, removes exactly what it installed.
   - **Wrapper** = shells out to upstream; manifest records only `(id, pinned_version, install_config, installed_at)`; `remove` delegates to upstream's removal verbs first, falls back to a scoped wipe.
2. Implement the `AddUnit` trait (see `specere-core`) in a new module under `crates/specere-units/src/`.
3. Register the unit in `specere-units::lookup`.
4. Add an integration test under `crates/specere-units/tests/` that runs `add` → `status` → `remove` and asserts the tree returns to its pre-install state (modulo documented postflight effects).
5. Document the unit under `docs/roadmap/31_specere_scaffolding.md` §6 (add a row to the MVP table) or under a new `docs/units/<your-unit>.md` page.
6. Add the capability as a new row in [`docs/research/09_speckit_capabilities.md`](docs/research/09_speckit_capabilities.md) §12.

## Adding a new deployer (new agent harness)

v1.0 ships Claude Code only. Post-v1, additional harnesses (Cursor, Aider, OpenCode, Codex) plug into the `Deploy` trait in `specere-units::deploy`. To add one:

1. Implement the `Deploy` trait with the harness-specific skill-directory and hook-config conventions.
2. Create a new unit type (`<harness>-deploy`, e.g. `cursor-deploy`).
3. Register the unit in `specere-units::lookup`.
4. Add an integration test that exercises at least one skill install + remove.

## Release process

Releases are driven by [`cargo-dist`](https://opensource.axo.dev/cargo-dist/). Tag a commit `v0.X.Y` on `main`; the release workflow publishes to crates.io and GitHub releases (macOS arm64/x86_64, Linux x86_64/aarch64, Windows x86_64).

Semver:
- **Major** — breaking CLI change (flag rename, subcommand removal, manifest format break).
- **Minor** — new unit, new command, new flag.
- **Patch** — bug fixes.

Every release ships a CHANGELOG diff, and breaking changes ship a migration note.

## Where to read next

- [`docs/specere_v1.md`](docs/specere_v1.md) — the 7-phase master plan.
- [`docs/roadmap/31_specere_scaffolding.md`](docs/roadmap/31_specere_scaffolding.md) — the scaffolding design.
- [`docs/research/09_speckit_capabilities.md`](docs/research/09_speckit_capabilities.md) — the governing composition reference.
