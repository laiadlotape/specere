# 08 — SpecKit Deep Dive (for SpecERE scaffolding)

> **Purpose.** Before building SpecERE (Spec Entropy Regulation Engine), understand the incumbent — [GitHub Spec Kit](https://github.com/github/spec-kit) — well enough to (a) ship a correct `specere add speckit` scaffolder, (b) decide which SpecKit patterns to borrow and which to reject, and (c) identify SpecKit state that SpecERE's Repo-SLAM sensors must read.
>
> **Freshness.** Researched against the live repository on 2026-04-18; reflects release **v0.7.3** (published 2026-04-17).

---

## 1. Origin, scope, status (April 2026)

Spec Kit is an open-source toolkit from GitHub (org `github`) for "Spec-Driven Development" (SDD). The repo [github/spec-kit](https://github.com/github/spec-kit) is **MIT-licensed** — relevant because SpecERE is Apache-2.0: we cannot copy code, but patterns and file contracts are fair game. Created **2025-08-21**, under near-daily release cadence: `v0.7.3` on 2026-04-17, `0.7.2` the day before, `0.7.0` on 2026-04-14 ([CHANGELOG.md](https://github.com/github/spec-kit/blob/main/CHANGELOG.md)). ~89k stars, 7.6k forks; the dominant AI-SDD incumbent.

Problem framing (README): "focus on product scenarios and predictable outcomes instead of vibe coding every piece from scratch." The companion [`spec-driven.md`](https://github.com/github/spec-kit/blob/main/spec-driven.md) is stronger — "specifications become executable, directly generating working implementations rather than just guiding them." Primary docs consulted: [README.md](https://github.com/github/spec-kit/blob/main/README.md), [spec-driven.md](https://github.com/github/spec-kit/blob/main/spec-driven.md), [AGENTS.md](https://github.com/github/spec-kit/blob/main/AGENTS.md), the [docs/reference/](https://github.com/github/spec-kit/tree/main/docs/reference) tree (added 0.7.2), and the [launch blog post](https://github.blog/ai-and-ml/generative-ai/spec-driven-development-with-ai-get-started-with-a-new-open-source-toolkit/).

The design is still moving: 0.7.0 added a workflow engine, 0.7.1 deprecated `--ai` for `--integration`, 0.7.2 reorganised docs. SpecERE should target the **file/directory contract**, not the CLI internals.

---

## 2. The `specify` CLI surface

The CLI is a Python [typer](https://typer.tiangolo.com/) app ([`src/specify_cli/__init__.py`](https://github.com/github/spec-kit/blob/main/src/specify_cli/__init__.py), ~5000 lines). Install: `uv tool install specify-cli --from git+https://github.com/github/spec-kit.git@vX.Y.Z`.

Top-level commands (verified against `@app.command()` decorators and [`docs/reference/core.md`](https://github.com/github/spec-kit/blob/main/docs/reference/core.md)):

| Command | Purpose | Side effects |
|---|---|---|
| `specify init [name]` | Bootstrap SDD scaffolding | Creates `.specify/`, agent command files, optionally `git init` |
| `specify check` | Verify required tools (git + agent CLIs) | Read-only |
| `specify version` / `--version` | Print CLI version | Read-only |

`init` is the only scaffold-writing command. Flags: `--integration <key>` (~30 agents: `claude`, `copilot`, `gemini`, `cursor-agent`, `windsurf`, `codex`, `goose`, `forge`, `qwen`, `kiro-cli`, `mistral-vibe`, …), `--ai <key>` (deprecated alias), `--ai-skills`, `--here`/`.`, `--force`, `--no-git`, `--script sh|ps`, `--preset <id>`, `--branch-numbering sequential|timestamp`, `--ignore-agent-tools`.

**There is no `specify uninstall` / `clean` / `remove` at the top level.** Removal verbs live only inside subcommand groups: `specify integration uninstall <key>`, `specify preset remove <id>`, `specify extension remove <name>`, `specify workflow remove <id>`. The CLI binary is removed via `uv tool uninstall specify-cli` ([docs/upgrade.md](https://github.com/github/spec-kit/blob/main/docs/upgrade.md)), leaving `.specify/` and `specs/` intact. Real gap — §7.

The **prompted workflow** is driven by slash commands installed into the agent, not by the Python CLI. Core slash commands: `/speckit.constitution`, `/speckit.specify`, `/speckit.clarify`, `/speckit.plan`, `/speckit.tasks`, `/speckit.taskstoissues`, `/speckit.analyze`, `/speckit.checklist`, `/speckit.implement`. Each is a markdown prompt in [`templates/commands/`](https://github.com/github/spec-kit/tree/main/templates/commands); running it reads `.specify/templates/` and writes artifacts under `specs/<###-feature>/` and `.specify/memory/`.

Per-phase side-effects:

- `/speckit.constitution` → writes `.specify/memory/constitution.md` (read/updates, keeps it in sync with dependent templates via the command's explicit instruction)
- `/speckit.specify "<desc>"` → creates a git branch (via `scripts/bash/create-new-feature.sh` or the PowerShell equivalent), creates `specs/NNN-slug/spec.md` from `spec-template.md`
- `/speckit.clarify` → appends a `## Clarifications` section to `spec.md`
- `/speckit.plan` → fills `specs/NNN-slug/plan.md`, plus `research.md`, `data-model.md`, `quickstart.md`, `contracts/`
- `/speckit.tasks` → writes `specs/NNN-slug/tasks.md` with `[P]` parallel markers and `T###` task IDs
- `/speckit.analyze` → read-only cross-artifact audit
- `/speckit.implement` → executes tasks; runs local CLIs; mutates source
- `/speckit.taskstoissues` → pushes tasks to GitHub Issues (integration)

---

## 3. Template system

Two tiers: ship-time originals in the repo at [`templates/`](https://github.com/github/spec-kit/tree/main/templates) (`spec-template.md`, `plan-template.md`, `tasks-template.md`, `constitution-template.md`, `checklist-template.md`, `commands/*.md`, `vscode-settings.json`), and install-time copies at the target repo's `.specify/templates/` which agents read at runtime.

Variable substitution is **intentionally textual**, not Jinja/Handlebars:

- `[UPPER_SNAKE_CASE]` placeholders: `[PROJECT_NAME]`, `[FEATURE NAME]`, `[###-feature-name]`, `[DATE]`, `[CONSTITUTION_VERSION]`.
- `<!-- Example: ... -->` HTML comments give inline examples; agent erases them when filling.
- `$ARGUMENTS` (Markdown agents) / `{{args}}` (TOML, YAML — Gemini, Goose); alias is per-integration via `registrar_config["args"]` (AGENTS.md §"Argument Patterns").
- `{SCRIPT}` → helper-script path at install time; `__AGENT__` → agent name.

Runtime precedence (highest first, per README):

1. `.specify/templates/overrides/` — project-local tweaks
2. `.specify/presets/templates/`
3. `.specify/extensions/templates/`
4. `.specify/templates/` — core defaults

Templates resolve at runtime by walking this stack top-down; extension/preset **commands** (not templates) are materialised at install time into the agent's commands dir (e.g. `.claude/commands/`, `.gemini/commands/`) — see [Extensions reference](https://github.com/github/spec-kit/blob/main/docs/reference/extensions.md). A user override is just a same-named file in `overrides/`; no CLI flag. Presets are the packaged, reusable form of the same mechanism.

---

## 4. EARS integration

**SpecKit does not use EARS** ("While X, the system shall Y" / "When X, the system shall Y" / …). Verified by inspecting [`templates/spec-template.md`](https://github.com/github/spec-kit/blob/main/templates/spec-template.md), [`templates/constitution-template.md`](https://github.com/github/spec-kit/blob/main/templates/constitution-template.md), [`docs/reference/core.md`](https://github.com/github/spec-kit/blob/main/docs/reference/core.md), and all command prompts. The only "shall" lines in the repo are governance prose in `spec-driven.md` ("No feature shall be implemented directly within application code…"), not requirement syntax.

What SpecKit uses instead (from `spec-template.md`):

- **Functional requirements** `FR-NNN: System MUST <capability>` — informal MUST/SHOULD register, not EARS.
- **Acceptance scenarios** in BDD `Given / When / Then`.
- **`[NEEDS CLARIFICATION: <q>]`** inline markers — queue for `/speckit.clarify`.
- **`SC-NNN`** success criteria — technology-agnostic, measurable.
- **User stories** with `P1/P2/P3` priorities and an "Independent Test" field.

There is **no parser, linter, or schema check**. Compliance is enforced only by the agent reading HTML comments (`ACTION REQUIRED: …`) and by `/speckit.analyze`. For SpecERE, if we want EARS-style regularity (useful for observation-model fidelity), we add it ourselves — no conflict, because SpecKit has no opinion.

---

## 5. Files SpecKit places in a target repo

Concrete paths, from the README's "Detailed Process" walk-through and verified against the CLI source:

```
<repo>/
├── .specify/
│   ├── memory/
│   │   └── constitution.md              # /speckit.constitution output
│   ├── scripts/
│   │   ├── bash/                        # or powershell/ — chosen by --script
│   │   │   ├── create-new-feature.sh
│   │   │   ├── setup-plan.sh
│   │   │   ├── check-prerequisites.sh
│   │   │   ├── common.sh
│   │   │   └── update-agent-context.sh
│   ├── templates/                       # spec/plan/tasks/constitution/checklist
│   ├── templates/overrides/             # user overrides (optional)
│   ├── presets/                         # installed presets
│   ├── extensions/                      # installed extensions
│   ├── extensions.yml                   # hook registry (referenced by command prompts)
│   └── extension-catalogs.yml           # custom catalog sources
├── specs/
│   └── NNN-feature-slug/
│       ├── spec.md                      # /speckit.specify
│       ├── plan.md                      # /speckit.plan
│       ├── research.md                  # /speckit.plan
│       ├── data-model.md                # /speckit.plan
│       ├── quickstart.md                # /speckit.plan
│       ├── contracts/                   # /speckit.plan
│       └── tasks.md                     # /speckit.tasks
├── <agent-dir>/                         # per integration
│   └── commands/ or skills/ or workflows/ or recipes/
│       └── speckit.*.md|.toml|.yaml
└── <agent-context-file>                 # CLAUDE.md, GEMINI.md, AGENTS.md, .github/copilot-instructions.md
```

Config surfaces: `.specify/extensions.yml` (hook registry — `hooks.before_specify`, `hooks.after_plan`, … — parsed by each command's "Pre-Execution Checks" block) and per-extension `<ext>-config.yml` / `.local.yml` / `.template.yml` triples ([extensions reference](https://github.com/github/spec-kit/blob/main/docs/reference/extensions.md)).

Pre-0.6 releases placed `memory/`, `scripts/`, `templates/` at the repo root; modern layout nests them inside `.specify/` (README §STEP 2). SpecERE detectors must handle both.

---

## 6. Extension / customization points

Four mechanisms, by intrusiveness:

1. **Project-local overrides** — drop a template into `.specify/templates/overrides/`.
2. **Presets** (`specify preset add <id>`) — packaged template+command overrides, stackable by priority; no new capabilities. ~8 catalog entries (Canon Core, Pirate Speak, Explicit Task Dependencies, TOC Navigation, …).
3. **Extensions** (`specify extension add <name>`) — new slash commands and templates. ~60 community entries in [`catalog.community.json`](https://github.com/github/spec-kit/blob/main/extensions/catalog.community.json) (MAQA, Jira, Confluence, V-Model, Worktrees, Bugfix Workflow, Security Review…). Lowest-priority-number wins. Hooks registered in `.specify/extensions.yml` under `hooks.before_*` / `hooks.after_*`.
4. **Integrations** — per-agent Python subpackages in [`src/specify_cli/integrations/`](https://github.com/github/spec-kit/tree/main/src/specify_cli/integrations). Subclass one of `MarkdownIntegration` / `TomlIntegration` / `YamlIntegration` / `SkillsIntegration` / `IntegrationBase`; declare `key`, `config`, `registrar_config`, `context_file`. Full spec in [`AGENTS.md`](https://github.com/github/spec-kit/blob/main/AGENTS.md).

A separate **workflow engine** landed in 0.7.0 (`workflow_app`: `run`, `resume`, `status`, `list`, `add`, `remove`, …) for multi-step orchestration; adjacent to, not part of, SDD slash commands.

---

## 7. Uninstall / removal story

The clearest friction point SpecERE should fix.

Exists: `uv tool uninstall specify-cli` (global binary), `specify extension remove <name>` (one extension; `--keep-config` available), `specify preset remove <id>`, `specify integration uninstall <key>`.

Does **not** exist:

- No `specify uninstall` that removes `.specify/`, `specs/`, agent-context files (`CLAUDE.md` etc.), or scripts from a target repo. No dry-run.
- No repo-level manifest of "files originally installed by `specify init`." A partial per-integration `IntegrationManifest` exists ([`AGENTS.md`](https://github.com/github/spec-kit/blob/main/AGENTS.md)) but does not cover `.specify/` core assets.
- No cross-version removal tracker — a 0.6.x `init` layout may not be fully recognised by a 0.7.3 `integration uninstall`.
- No idempotency contract for re-running `init`; non-empty directory errors unless `--force`, and `--force` does not reconcile prior state across agent switches.

The **deterministic idempotent `add`/`remove` pair** is SpecERE's core UX differentiator, precisely because SpecKit lacks it.

---

## 8. Comparison matrix: scaffolding philosophy

| Tool | What it installs | Uninstall story | Template style | Scope |
|---|---|---|---|---|
| **SpecKit** | `.specify/`, `specs/`, agent command files, context files | Per-component remove; no repo-wide uninstall | `[PLACEHOLDER]` + HTML comments; `$ARGUMENTS` / `{{args}}` | Full SDD lifecycle across agents |
| **AWS Kiro** | IDE-native "specs" and "steering" files under `.kiro/`; EARS-enforced requirements | Tied to the Kiro IDE; remove via IDE UI | EARS (`shall`) enforced syntactically at spec-authoring time | Greenfield + brownfield inside the Kiro IDE, closed-source |
| **Cursor rules** | `.cursor/rules/*.mdc`, `.cursorrules`, `.cursor/commands/` | Manual file deletion; no CLI | Plain markdown + frontmatter; glob-scoped `alwaysApply` rules | Prompt/rule injection into Cursor sessions only |
| **Aider conventions** | `CONVENTIONS.md`, `.aider.conf.yml`, `.aider.model.metadata.json` | Manual deletion; no scaffolder | Plain markdown conventions loaded via `--read` | Single-agent repo-map + coding conventions; no phase model |
| **Claude Code slash commands** | `.claude/commands/*.md`, `CLAUDE.md`, `.claude/skills/` (SpecKit installs these *into* Claude Code) | Manual; Claude Code reads whatever is in `.claude/` | Markdown prompt files with `$ARGUMENTS` | Per-agent command surface; delegated by SpecKit |
| **SpecERE (target)** | TBD — composable `add` units under a single manifest | First-class idempotent `remove`; full manifest | Borrow `[PLACEHOLDER]` + override stack; add schema checks | Telemetry + SDD filter; layers above agent frameworks, not beside them |

Key delta: SpecKit, Cursor, and Aider are **one-shot scaffolders with no uninstall**. Kiro is closed and IDE-bound. SpecERE's opportunity is "composable add, full remove, structured manifest" across the same artifact surface.

---

## 9. Co-existence with SpecERE

`specere add speckit` must do five things:

1. **Detect** — check for `.specify/`; if present, validate version (memory layout, `extensions.yml`, context-file) and attach non-destructively (or no-op).
2. **Scaffold when absent** — invoke `uvx --from git+https://github.com/github/spec-kit.git@<pinned-tag> specify init . --integration <agent> --force`. Wrap upstream; never fork its output. Pin a known-good version per SpecERE release.
3. **Record manifest** — write `.specere/manifest.toml` listing every file/dir with SHA256, so `specere remove speckit` distinguishes "installed-by-me, unchanged" from "user-edited".
4. **Register sensors** — the Repo-SLAM spec-belief filter consumes `specs/*/spec.md` (FR-NNN IDs as latent variables), `specs/*/tasks.md` (T### IDs + `[P]` parallel flags as dependency edges), `.specify/memory/constitution.md` (prior), and `specs/*/plan.md` (tech-context delta) as typed inputs to the telemetry pipeline defined in `docs/research/01_agent_telemetry.md`.
5. **Install an `after_implement` hook** — in `.specify/extensions.yml`, invoking `specere observe` so every `/speckit.implement` run emits a telemetry record.

**File-ownership conflicts**: both tools write to the agent-context file (`CLAUDE.md`, `AGENTS.md`). Resolution: SpecERE owns a block fenced by `<!-- specere:begin --> … <!-- specere:end -->` and never touches content outside. This matches SpecKit 0.7.3's own pivot to marker-based upsert ([#2259](https://github.com/github/spec-kit/pull/2259)) — safe convention.

SpecERE-reads-only-never-writes from SpecKit:

- `specs/NNN-slug/spec.md` — FR-NNN, SC-NNN, P1/P2/P3 priorities, `[NEEDS CLARIFICATION]` markers (entropy signal).
- `specs/NNN-slug/tasks.md` — T### IDs, `[P]` flags, checkpoint markers.
- `.specify/memory/constitution.md` — governance prior.
- `.specify/extensions.yml` — avoid duplicating hooks.

---

## 10. Reusable patterns / anti-patterns

**Borrow:**

- **Minimal textual substitution** (`[PLACEHOLDER]` + `<!-- Example: ... -->`). Legible to humans and LLMs; zero parser risk.
- **Override precedence stack with one `overrides/` directory** — avoids forking a whole preset for a one-line change.
- **Per-agent integration subclasses with fixed metadata** (`key`, `config`, `registrar_config`, `context_file`) — clean for multi-agent telemetry.
- **Marker-based upsert** for shared files — adopted by SpecKit 0.7.3 itself.
- **Frontmatter `handoffs:`** in command prompts — declarative next-step suggestions, useful for Repo-SLAM active-exploration policy.
- **`hooks.before_*` / `hooks.after_*` convention** — clean join point for extensions.

**Avoid:**

- **Prompt-embedded hook dispatch.** Each core command's markdown repeats the "parse `.specify/extensions.yml`, filter `enabled:false`, skip conditions" block. Drift is inevitable. SpecERE should centralise dispatch in code.
- **No repo-wide uninstall/manifest.** The single biggest gap (§7); ship on day one.
- **Churning flag names.** `--ai` → `--integration` mid-0.7.x; `--offline` silently demoted to no-op. Lock SpecERE's public flags before 1.0 with a deprecation window.
- **No requirement-syntax enforcement.** SpecKit leaves requirement quality entirely to the LLM. An entropy-regulation engine cannot — we need a defined input distribution (EARS or similar).
- **Unsigned extension catalogue.** Community extensions are read/write with no cryptographic provenance. SpecERE should sign its bundled `add` units.
- **Heavy Python runtime dep (`uv`, Python 3.11+).** A Rust single static binary is the natural counter-positioning.

---

## 11. Synthesis: what this means for SpecERE scaffolding

1. **`specere add speckit` is a thin wrapper around upstream `specify init`.** Pin a release tag per SpecERE version, call upstream, then record the manifest and install sensors. Do not re-implement SpecKit's scaffolder.
2. **SpecERE's manifest is the primary differentiator.** Every `add` writes a signed entry to `.specere/manifest.toml` capturing (component, version, file path, SHA256, install-time config). Every `remove` consults it. This is the feature SpecKit is missing.
3. **Marker-based shared-file editing is the only acceptable pattern.** For `CLAUDE.md`, `AGENTS.md`, any future `.github/` file SpecERE touches, wrap our block with `<!-- specere:begin {id} -->` / `<!-- specere:end {id} -->` so we can round-trip cleanly and so SpecKit's own upsert does not collide.
4. **Read SpecKit state, do not compete with it.** `specs/**/spec.md`, `tasks.md`, `.specify/memory/constitution.md`, `.specify/extensions.yml` are read-only sensor inputs to the Repo-SLAM filter. Layer on top; do not fork.
5. **Lock the `add` unit contract before writing any Rust.** Each `add` is `(id, pinned-version, preflight, install, manifest-record, postflight hooks, remove)`. Define this six-tuple in the SpecERE design doc before shipping `specere add speckit`; everything else (otel-hooks, ears-linter, telemetry-db) slots into the same shape.

---

## References

- [github/spec-kit — GitHub repository](https://github.com/github/spec-kit)
- [README.md](https://github.com/github/spec-kit/blob/main/README.md)
- [spec-driven.md](https://github.com/github/spec-kit/blob/main/spec-driven.md)
- [AGENTS.md (integration architecture)](https://github.com/github/spec-kit/blob/main/AGENTS.md)
- [CHANGELOG.md](https://github.com/github/spec-kit/blob/main/CHANGELOG.md)
- [src/specify_cli/__init__.py (CLI entrypoint)](https://github.com/github/spec-kit/blob/main/src/specify_cli/__init__.py)
- [templates/spec-template.md](https://github.com/github/spec-kit/blob/main/templates/spec-template.md)
- [templates/plan-template.md](https://github.com/github/spec-kit/blob/main/templates/plan-template.md)
- [templates/tasks-template.md](https://github.com/github/spec-kit/blob/main/templates/tasks-template.md)
- [templates/constitution-template.md](https://github.com/github/spec-kit/blob/main/templates/constitution-template.md)
- [templates/commands/specify.md](https://github.com/github/spec-kit/blob/main/templates/commands/specify.md)
- [templates/commands/constitution.md](https://github.com/github/spec-kit/blob/main/templates/commands/constitution.md)
- [docs/reference/core.md](https://github.com/github/spec-kit/blob/main/docs/reference/core.md)
- [docs/reference/extensions.md](https://github.com/github/spec-kit/blob/main/docs/reference/extensions.md)
- [docs/reference/presets.md](https://github.com/github/spec-kit/blob/main/docs/reference/presets.md)
- [docs/upgrade.md](https://github.com/github/spec-kit/blob/main/docs/upgrade.md)
- [extensions/catalog.community.json](https://github.com/github/spec-kit/blob/main/extensions/catalog.community.json)
- [GitHub blog: Spec-driven development with AI — Get started with a new open source toolkit](https://github.blog/ai-and-ml/generative-ai/spec-driven-development-with-ai-get-started-with-a-new-open-source-toolkit/)
- [PR #2259 — marker-based upsert for context updates](https://github.com/github/spec-kit/pull/2259)
- SpecKit release `v0.7.3` (2026-04-17): [releases](https://github.com/github/spec-kit/releases/tag/v0.7.3)
