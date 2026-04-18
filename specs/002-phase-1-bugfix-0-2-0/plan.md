# Implementation Plan: Phase 1 Bugfix Release (0.2.0)

**Branch**: `002-phase-1-bugfix-0-2-0` | **Date**: 2026-04-18 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/002-phase-1-bugfix-0-2-0/spec.md`

## Summary

Close the four confirmed dogfood bugs (`docs/specere_v1.md §4`) in the `speckit` wrapper unit and the `claude-code-deploy` unit. Technical approach: extend the existing `AddUnit` trait (`crates/specere-core/src/lib.rs`) with a SHA-diff preflight gate; teach the `speckit` unit to detect ambient git state and auto-create `000-baseline` (overridable); introduce a minimal `claude-code-deploy` unit that owns the `.claude/` tree, the `.gitignore` marker block for credentials, and the first real `after_implement` hook in `.specify/extensions.yml`. All file mutations happen inside marker-fenced blocks so `remove` is a byte-identical inverse. Regression tests for each bug live in `crates/specere-units/tests/` using ephemeral `tempfile::TempDir` fixtures with live `git init`.

## Technical Context

**Language/Version**: Rust stable (pinned to 1.78 via `rust-toolchain.toml`)
**Primary Dependencies**: `anyhow`, `thiserror`, `serde` (derive), `toml`, `serde_json`, `serde_yaml` (for `.specify/extensions.yml` read-safe parse), `sha2` (SHA256), `time`, `tracing`, `tempfile` (dev), `assert_cmd` + `predicates` (dev — CLI integration tests). All already in-workspace except `serde_yaml` which is added by this plan.
**Storage**: Plain-file TOML for `.specere/manifest.toml`; YAML for `.specify/extensions.yml` (read-safe, never round-trip-generated — we parse and mutate-in-place via marker blocks). No database.
**Testing**: `cargo test` + `assert_cmd`-driven CLI tests + ephemeral-fixture integration tests under `crates/specere-units/tests/`. No mocks for git or filesystem — tests run real `git init` + `git checkout` inside a temp dir.
**Target Platform**: Linux (primary, CI), macOS (secondary, CI), Windows (tertiary, smoke-test in CI).
**Project Type**: CLI tool (single-binary `specere` distributed via `cargo-dist`).
**Performance Goals**: SC-003 (refuse-on-edit ≤ 200 ms), SC-007 (integration-test suite ≤ 5 min wall-clock on a laptop). `specere add speckit` end-to-end is dominated by `uvx specify init` (network + Python startup) — out of our scope. Our Rust code budget: < 100 ms per unit install, excluding sub-process wait.
**Constraints**: single static binary; no Python or Node in shipped artifacts; `uvx` sub-process is the one allowed exception (invoked by `speckit` wrapper unit). Cross-platform path handling mandatory — no hard-coded `/`.
**Scale/Scope**: v0.2.0 ships two units (`speckit`, `claude-code-deploy`). Manifest supports ≤ 20 units per repo without perf concern (linear scans are fine at that size). The `.specify/extensions.yml` mutation is idempotent under 10⁴ repeated invocations (tested).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Evaluated against `.specify/memory/constitution.md` (2026-04-18 ratification):

| Principle | Status | Notes |
|---|---|---|
| I. Compose, Never Clone | **PASS** | No new scaffolder re-implementation; `speckit` unit still shells out to `specify-cli`. No OTel SDK re-implementation. |
| II. Ten Composition Rules | **PASS** | Rule 1 (installer detects git-kind) = exactly what FR-P1-001/002 implement. Rule 2 (hooks only in `extensions.yml`) = FR-P1-005. Rule 4 (marker-fenced blocks) = FR-P1-004 and the after_implement hook entry wrapping. Rule 8 (uninstall via manifest) = FR-P1-006. Rule 10 (parse narrowly) = FR-P1-008. Rules 3, 5, 6, 7, 9 untouched. |
| III. Reversible Units | **PASS** | FR-P1-006 promotes this to a blocking test (SC-004). |
| IV. Human-in-the-Loop | **PASS** | No new gates. `review-spec` + `review-plan` + `divergence-adjudication` already scaffolded; this plan adds no per-tool-call interactivity. |
| V. Harness Self-Extension Detection | **PASS** | Already one open queue item (recursive-claude state leak). Plan adds no new write surface requiring sensor-map extension. |
| Engineering: single static Rust binary | **PASS** | No Python/Node added. `serde_yaml` and `sha2` are pure-Rust. |
| Engineering: SpecKit v0.7.3 pin | **PASS** | `PINNED_SPECKIT_TAG` constant unchanged; touched only to drop `--no-git`. |
| Engineering: EARS-style FRs | **PASS** | Every FR-P1-NNN uses MUST/SHOULD phrasing; `ears-linter` unit (not yet implemented) would pass. |
| Engineering: no `--no-git` on git repos | **PASS** | FR-P1-001 is this principle. |

**Gate: PASS. No violations. Complexity Tracking table empty.**

## Project Structure

### Documentation (this feature)

```text
specs/002-phase-1-bugfix-0-2-0/
├── plan.md              # This file
├── research.md          # Phase 0 output — trivial (no unresolved clarifications)
├── data-model.md        # Phase 1 output — manifest + hook-entry schemas
├── quickstart.md        # Phase 1 output — Phase-1-user onboarding walk-through
├── contracts/
│   ├── cli.md           # `specere add / remove` command contract (flags, exit codes)
│   ├── manifest.md      # `.specere/manifest.toml` TOML schema v1 (what Phase 1 extends)
│   └── extensions-mutation.md  # Safe-mutation protocol for `.specify/extensions.yml`
├── spec.md              # Clarified spec (input to this plan)
├── checklists/
│   └── requirements.md  # Passed in /speckit-clarify
└── tasks.md             # Phase 2 output — /speckit-tasks will create
```

### Source Code (repository root)

The repo is a multi-crate Rust workspace; Phase 1 touches the following paths:

```text
crates/
├── specere/              # top-level binary — CLI entry (clap)
│   └── src/main.rs           # adds `--adopt-edits` flag, `--branch` flag, `--delete-branch` flag
├── specere-core/         # traits, Ctx, Plan, Record, Error
│   └── src/lib.rs            # extends Error with AlreadyInstalledMismatch, ParseFailure, DeletedOwnedFile
├── specere-units/        # the concrete units
│   └── src/
│       ├── lib.rs            # registry (speckit + claude-code-deploy)
│       ├── speckit.rs        # drop --no-git; auto-create branch; record in manifest
│       ├── deploy/           # the claude-code-deploy unit
│       │   ├── mod.rs        # unit implementation
│       │   ├── gitignore.rs  # marker-fenced .gitignore mutation
│       │   ├── extensions.rs # marker-fenced extensions.yml hook add/remove
│       │   └── skills/       # bundled skill files (embed via include_str!)
│       │       └── specere-observe-implement.md
│       └── tests/            # integration tests — one per FR
│           ├── fr_p1_001_no_no_git.rs
│           ├── fr_p1_002_branch_auto_create.rs
│           ├── fr_p1_003_sha_diff_gate.rs
│           ├── fr_p1_004_gitignore_marker.rs
│           ├── fr_p1_005_hook_registration.rs
│           ├── fr_p1_006_remove_round_trip.rs
│           ├── fr_p1_007_manifest_branch_record.rs
│           ├── fr_p1_008_malformed_file_refuse.rs
│           └── common/        # shared `TempRepo` fixture + helpers
│               └── mod.rs
├── specere-manifest/     # TOML schema load/save + SHA256 helpers
│   └── src/lib.rs            # extends schema with install_config.branch_name + branch_was_created_by_specere
├── specere-markers/      # marker-fence parser/writer
│   └── src/lib.rs            # already handles the generic `<!-- specere:begin X -->` pattern; no change expected
└── specere-telemetry/    # OTel emitter stub — untouched by Phase 1

examples/
└── dogfood-research/     # existing fixture — add `.specere/` snapshot assertions

```

**Structure Decision**: single Rust workspace (Option 1 equivalent — CLI tool, not web / mobile). Unit-specific code lives under `crates/specere-units/src/<unit>/` (new `deploy/` subdir for `claude-code-deploy` since it's multi-file). Integration tests live alongside the unit under `crates/specere-units/tests/fr_*.rs` — one test file per FR so failures are traceable to the bug they fix. Shared test helpers under `tests/common/`.

## Complexity Tracking

> **Empty** — Constitution Check passed with no violations.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| (none) | — | — |

## Priors set by this plan

Enumerated here so the `review-plan` gate (core_theory §4 prior-setting touchpoint) has a single page to review:

1. **Error-type discipline** — one `Error` enum in `specere-core::Error`, variants per failure mode. Per-unit errors flow through `#[from]` on this enum. Rejected alternative: per-unit error types (more boilerplate, worse diagnostics when errors cross unit boundaries).
2. **Fixture strategy** — `tempfile::TempDir` + live `git init` per test. Rejected alternative: pre-baked tarball fixture (loses signal on cross-platform path quirks; faster but less realistic).
3. **SHA256 implementation** — `sha2` crate. Rejected alternative: `blake3` (faster but not the standard SpecKit already uses; `sha2` matches the convention in `.specify/integrations/*.manifest.json`).
4. **YAML mutation of `extensions.yml`** — parse with `serde_yaml`, locate our marker block, mutate in-place, preserve all other content byte-for-byte. Rejected alternative: parse + re-serialize (would reformat the file and destroy git extension's entries).
5. **Branch handling on `remove`** — the branch is deleted only if `--delete-branch` is passed AND the branch is clean (no uncommitted changes). If dirty, refuse with pointer to `git stash`. Rejected alternative: force-delete (violates principle III reversibility — user could lose uncommitted work).
6. **`specere_observe.implement` registration** — hook entry is written **even though the observe command body is a Phase-3 stub**. The hook's `prompt` field references `specere observe record --source=implement`; if the binary doesn't support `observe` yet, it exits with a friendly "coming in v0.4.0" message and the workflow proceeds. Rejected alternative: defer hook registration to Phase 3 (then Phase 3 has to do both plumbing + filter work; Phase 1 hook wiring is the lighter half).
7. **CLI flag surface** — `specere add <unit>` gains `--adopt-edits`, `--branch <name>`, `--force`. `specere remove <unit>` gains `--delete-branch`, `--force`. No other flags in Phase 1. Rejected alternative: a single mega-`--mode` flag (less discoverable).

## Post-Phase-1 re-check

After writing research.md, data-model.md, contracts/, and quickstart.md, I re-evaluated the Constitution Check. No new violations introduced by Phase-1 artifacts — all still PASS. The data-model.md schema change (adding `install_config.branch_name` + `branch_was_created_by_specere`) is a strictly-additive TOML-schema extension; existing manifests parse unchanged. Contracts/cli.md flags match the priors-set list above. Quickstart.md walks a new user through installer + remove round-trip and explicitly exercises FR-P1-001 through FR-P1-006.
