# 09 — SpecKit Capabilities (for SpecERE composition)

> **Purpose.** Capability reference so a SpecERE implementer can decide per-capability whether SpecERE **WRAPS** (delegates), **IGNORES** (out of scope), or **EXTENDS** (overlays without editing SpecKit-owned files). Continues [08 — SpecKit Deep Dive](./08_speckit_deepdive.md).
>
> **Freshness.** 2026-04-18, against [github/spec-kit](https://github.com/github/spec-kit) **v0.7.3** as scaffolded into this repo by `uvx specify-cli@v0.7.3`. Cross-checked with [CHANGELOG.md](https://github.com/github/spec-kit/blob/main/CHANGELOG.md).

---

## 1. Delta vs. 08

Doc 08 established the file contract, verb list, extension/preset/integration taxonomy, and uninstall gap; it stopped at "call `uvx specify-cli init .` and record a manifest." This doc goes deeper on the five surfaces needed once SpecERE must **compose** rather than **install**: full `specify` sub-command tree, 0.7.0 workflow engine, `extensions.yml` hook schema, per-command hook lifecycle, `--no-git` edge cases — then the capability matrix and composition pattern.

The pivot since 08: SpecERE is the primary deliverable, built **strictly on top of** SpecKit + OTel. "Compose, never clone" is a rule, not a heuristic.

---

## 2. `specify` CLI surface — full sub-command tree

Doc 08 named three top-level commands. The full v0.7.3 tree, reconstructed from [`docs/reference/*.md`](https://github.com/github/spec-kit/tree/main/docs/reference) and the `@app.command()` audit in [`src/specify_cli/__init__.py`](https://github.com/github/spec-kit/blob/main/src/specify_cli/__init__.py):

| Command | Purpose | Notable flags | Verdict |
|---|---|---|---|
| `init [NAME]` | Bootstrap `.specify/` tree + per-agent files; optional `git init` | `--integration`, `--ai-skills`, `--here`/`.`, `--force`, `--no-git`, `--script`, `--preset`, `--branch-numbering`, `--ignore-agent-tools` | **WRAP** |
| `check` | Probe required CLIs | `--json` | **WRAP** |
| `version` / `--version` | Print CLI version | — | **WRAP** |
| `integration {list, install, uninstall, update}` | Per-agent command surface; writes `.specify/integrations/<key>.manifest.json` (sha256 per file) | `--force`, `--ai-skills`, `--keep-config` | **WRAP** |
| `preset {list, add, remove, update}` | Stackable template/command overlays under `.specify/presets/` | `--priority N`, `--keep-config` | **WRAP** |
| `extension {list, search, add, remove, update}` | New slash commands + hook registrations; writes `.specify/extensions/<name>/`, appends to `.specify/extensions.yml` | `--source URL`, `--ref TAG`, `--priority N` | **WRAP** |
| `workflow list / run / resume / status` | Execute a declarative workflow; state in `.specify/workflows/<id>/.runs/` | `--input K=V`, `--run-id` | **WRAP** |
| `workflow add / remove` | Install/uninstall a workflow YAML + registry entry | `--source URL` | **EXTEND** (SpecERE ships `specere-observe`) |

**Error modes observed.** `init --here` on a non-empty dir without `--force` exits non-zero with a multiline banner (no stable exit-code taxonomy — SpecERE parsing text is brittle). `integration uninstall` silently succeeds when the manifest is missing (0.6.x migration pitfall). `workflow run` fails hard on unresolved `{{ inputs.* }}` placeholders; no dry-run. `extension add <bad-name>` returns 1 but useful messages only with `--verbose`. No top-level `specify uninstall` exists (doc 08 §7 gap persists in 0.7.3).

**Verdict.** Every sub-command is **WRAP** except the two `workflow` create/delete verbs, which SpecERE must **EXTEND** to ship its observation workflow. Nothing is IGNORE-worthy.

---

## 3. The workflow engine (new in 0.7.0)

0.7.0 added a declarative, resumable, multi-step orchestrator; prior versions orchestrated via agent interpretation of slash-command frontmatter `handoffs:`. Moving parts (visible in this repo's `.specify/workflows/`):

1. **Registry** — `.specify/workflows/workflow-registry.json` with `schema_version: "1.0"`, `workflows: { <id>: { name, version, description, source, installed_at, updated_at } }`. Authoritative for installed workflows; catalog discovery is separate.
2. **Per-workflow YAML** — `.specify/workflows/<id>/workflow.yml` with `schema_version`, `workflow`, `requires: { speckit_version, integrations.any: [...] }`, `inputs` (type/required/default/enum/prompt), and `steps[]`.
3. **Step kinds** — `command` (slash command on the chosen integration, `input.args` templated) and `gate` (human approval with `options` and `on_reject: abort|skip|retry`). The default `speckit` workflow chains `specify → review-spec → plan → review-plan → tasks → implement`.
4. **Run state** — `.specify/workflows/<id>/.runs/<run-id>.json` holds resolved inputs, step pointer, past results, gate decisions. `resume` picks the latest unfinished run if no `--run-id`.
5. **Template language** — `{{ inputs.<name> }}` and `{{ steps.<id>.output.<key> }}` — flat Mustache-ish, not Jinja.

Each step's slash command runs through the same pre/post-hook dispatch as a manual invocation, so extensions are workflow-agnostic. SpecERE composes its own via `specify workflow add specere-observe`, shipping a YAML that wraps `speckit.implement` with a `specere-observe` step (or relies on an `after_implement` hook plus `workflow run` from CI). Value-add: not a review gate, but OTel spans around each command + a pre/post diff of `specs/**`.

**Verdict.** **WRAP** the engine (never reimplement run/resume/status). **EXTEND** by shipping one SpecERE workflow. **IGNORE** any SpecERE-native orchestrator.

---

## 4. `.specify/extensions.yml` format

No live example of `extensions.yml` exists in this repo because no hook-bearing extension has been installed — exactly pivot-memory bug #4 ("Hook surface never produced — post-hook contract untested"). Canonical schema is recoverable from the inline parsers in every `.claude/skills/speckit-*/SKILL.md`, which all implement identical dispatch.

**Schema + SpecERE's canonical entry**:

```yaml
# .specify/extensions.yml
hooks:
  after_implement:
    - extension: "specere"                # human-readable owner
      command: "specere.observe.implement" # dot-form; renders as /specere-observe-implement
      description: "Record Repo-SLAM observation from just-completed implement run"
      prompt: "Run specere observe --source=implement --feature-dir=$FEATURE_DIR"
      enabled: true                       # absent → true; false → skipped
      optional: false                     # false=halt-and-wait; true=render 'you may run' block
      condition: null                     # non-empty → skip entirely (see below)
```

**Full verb list** for `before_*` / `after_*`: `specify`, `clarify`, `plan`, `tasks`, `analyze`, `implement`, `constitution`, `checklist`, `taskstoissues` — nine verbs, eighteen keys. Confirmed by grep over installed skills.

**Field semantics.** `enabled` — missing → `true`. `optional: false` halts the command until the hook returns; `optional: true` prints a "you may run" block and moves on. `condition` — documented but **not interpreted**: installed skills say "do not attempt to evaluate" and defer to a future HookExecutor; any non-empty value today ≡ `enabled: false`. **Dot-hyphen rewrite**: `speckit.git.commit` → `/speckit-git-commit` (Claude Code skills forbid dots). Author in dot form; expect hyphens at render time.

With `optional: false`, `/speckit.implement` emits `EXECUTE_COMMAND: specere.observe.implement` and waits; the agent runs `/specere-observe-implement` before resuming.

**Verdict.** **WRAP** the file format — never a parallel hook registry. **IGNORE** `condition` until HookExecutor lands. **EXTEND** by being the first extension to exercise `after_implement` end-to-end.

---

## 5. Per-command hook lifecycle in detail

Every `.claude/skills/speckit-<verb>/SKILL.md` contains two identical ~30-line blocks: a "Pre-Execution Checks" that parses `hooks.before_<verb>` and a tail "Check for extension hooks (after)" that parses `hooks.after_<verb>`. Both tell the agent to YAML-parse `.specify/extensions.yml`, filter on `enabled`, skip on non-empty `condition`, dot-to-hyphen, and render one of four templates by `optional × before/after`. This is the **drift tax** from 08 §10 — protocol lives by copy-paste. `speckit-checklist/SKILL.md` already diverges (hook block sits mid-Outline).

**Per-verb lifecycle**:

| Verb | Pre-hook key | Post-hook key | Idempotent? | How it locates `FEATURE_DIR` |
|---|---|---|---|---|
| `constitution` | `before_constitution` | `after_constitution` | Yes — re-running updates `.specify/memory/constitution.md` in place | N/A (global file) |
| `specify` | `before_specify` | `after_specify` | No — creates a new `specs/NNN-slug/` each run | Creates it; writes `.specify/feature.json` |
| `clarify` | `before_clarify` | `after_clarify` | Yes — appends new `## Clarifications` entries | Via `check-prerequisites.sh` → `get_feature_paths` → `.specify/feature.json` or git-branch-prefix match |
| `plan` | `before_plan` | `after_plan` | Partial — overwrites `plan.md` but preserves sub-artifacts unless told to regenerate | Same as `clarify` |
| `tasks` | `before_tasks` | `after_tasks` | Partial — regenerates `tasks.md`; preserves `[X]` completion only via explicit instruction | Same |
| `analyze` | `before_analyze` | `after_analyze` | Yes — read-only | Same |
| `implement` | `before_implement` | `after_implement` | No — mutates source, flips `[ ]` to `[X]` in `tasks.md` | Requires `plan.md` + `tasks.md` via `--require-tasks` flag |
| `checklist` | `before_checklist` | `after_checklist` | Yes — named checklists are additive | Same |
| `taskstoissues` | `before_taskstoissues` | `after_taskstoissues` | Yes — idempotent via GitHub issue title match | Same |

**The branch-check trap.** `check_feature_branch` in [`common.sh`](https://github.com/github/spec-kit/blob/main/scripts/bash/common.sh): if `has_git=true`, require branch matches `^[0-9]{3,}-` or `^[0-9]{8}-[0-9]{6}-`; else warn + pass. So SpecERE's current `--no-git` scaffolder was safe by accident (scripts skip validation). But the ReSearch repo **is** git, so `has_git=true`, and `specere-adopt` never created a `000-baseline` branch — which is why every post-adopt `/speckit-*` attempt beyond `/speckit.constitution` and `/speckit.specify` fails. See §6.

**Verdict.** **WRAP** all nine lifecycles unmodified. **EXTEND** by registering SpecERE hooks in `extensions.yml` once per verb — never by editing prompts. **IGNORE** the drift tax (SpecKit's upstream problem).

---

## 6. `--no-git` ecosystem gap

`specere add speckit` currently calls `specify init . --no-git`. Seemed prudent ("don't touch VCS"). Against ReSearch it produces a hybrid-broken artefact: `.specify/` tree exists, no feature branch exists, ambient repo is git.

**Failure matrix** (trap triggers on `has_git=true` + branch ≠ numeric-prefix regex): `/speckit.constitution` works (no branch check); `/speckit.specify` works (creates its own branch via `create-new-feature.sh`); all other seven verbs (`clarify`, `plan`, `tasks`, `analyze`, `implement`, `checklist`, `taskstoissues`) **fail** identically with `check-prerequisites.sh` → `check_feature_branch` → `"Not on a feature branch"`, blocking the workflow.

`specere-adopt` is worse than `/speckit.specify` because it **bypasses** `create-new-feature.sh` and hand-crafts `specs/000-baseline/`. Spec file on disk, `check_feature_branch` still sees `main`, refuses.

**Three fixes**: (a) drop `--no-git`; run `git checkout -b 000-baseline` iff ambient is git. One-line change, zero SpecKit patches. (b) Patch `check-prerequisites.sh` — requires a diff against every SpecKit release; violates "compose, never clone." (c) Defer via `specify integration update` — only rewrites agent command files, not scripts.

**Recommendation: (a).** The only option honouring "compose, never clone." `000-baseline` is trivial; `specere add speckit --branch=<name>` overrides. No open upstream issue as of 2026-04-18; SpecERE should file "SDD commands should work on `main` when FEATURE_DIR is resolvable via `.specify/feature.json`."

**Verdict.** **IGNORE** `--no-git` in SpecERE's installer. **EXTEND** by auto-creating `000-baseline` on git repos. **WRAP** SpecKit's native git-init on non-git dirs.

---

## 7. Integrations architecture (deeper than 08)

[`AGENTS.md`](https://github.com/github/spec-kit/blob/main/AGENTS.md) defines `IntegrationBase` (four subclasses: `Markdown`, `Toml`, `Yaml`, `Skills`). **Attrs**: `key`, `config`, `registrar_config` (`command_dir`/`file_extension`/`args`-token/`format`/`frontmatter_style`), `context_file`. **Methods**: `render_command`, `install_files → IntegrationManifest`, `uninstall_files`, `update_files`, `validate_environment`. **Uninstall manifest** at `.specify/integrations/<key>.manifest.json`: `{integration, version, installed_at, files: {path → sha256}}` — hash-per-file distinguishes user-edited from clean. Covers agent files only; `.specify/` core is tracked by sibling `speckit.manifest.json`.

**Claude with `--ai-skills`** (the only integration SpecERE targets): `command_dir = .claude/skills/`, each command at `speckit-<verb>/SKILL.md` (hyphenated here), frontmatter `name/description/argument-hint/compatibility/metadata.source/user-invocable/disable-model-invocation`, `context_file = CLAUDE.md`.

**Where `claude-code-deploy` slots in.** Does **not** subclass `IntegrationBase`. It writes skills into `.claude/skills/specere-*` (disjoint from `speckit-*`), records them in `.specere/manifest.toml`, fences a block in `CLAUDE.md` with `<!-- specere:begin --> … <!-- specere:end -->`, and appends `.specify/extensions.yml` entries under `extension: "specere"`. Compose above the interface, not inside.

**Verdict.** **WRAP** the `IntegrationBase` hierarchy (never subclass). **EXTEND** via `specere-*` namespace + fenced context blocks. **IGNORE** multi-agent `registrar_config` complexity — v1 is Claude-only.

---

## 8. Extension catalog — what SpecERE should compose

[`catalog.community.json`](https://github.com/github/spec-kit/blob/main/extensions/catalog.community.json) + `catalog.official.json` list ~60–70 entries. Filtered for SpecERE relevance (observation, telemetry, requirement fidelity, verification):

| Extension | Purpose (1-line) | Verdict |
|---|---|---|
| `speckit.property-tests` | Property-based test scaffolder (Hypothesis/proptest wrappers) | **WRAP** — directly feeds Channel D in `.specere/sensor-map.toml` |
| `speckit.mutation-testing` | Adds mutmut/stryker task; emits mutation-kill metric | **WRAP** — another Channel D signal |
| `speckit.security-review` | `/speckit.security-review` slash command over `spec.md` + `plan.md` | **WRAP** — feeds constitution-side priors |
| `speckit.v-model` | V-Model phase overlay (verification, validation steps in plan + tasks) | **WRAP** — aligns with `docs/analysis/core_theory.md` §3 sensor channels |
| `speckit.bugfix-workflow` | Bugfix variant of the `specify → plan → tasks → implement` cycle | **WRAP** — SpecERE's OTel spans already cover `bugfix_*` verbs when registered |
| `speckit.worktrees` | Git-worktree parallelisation of feature branches | **WRAP** — useful for telemetry parallelism tests |
| `speckit.ears-lint` *(community, low priority)* | Lint `FR-NNN` lines for EARS compliance (`When`/`While`/`Where`) | **WRAP + EXTEND** — SpecERE adds EARS as first-class (doc 08 §4); this extension is the cheap starting point |
| `speckit.contract-tests` | Generate OpenAPI-schema-grounded contract tests | **WRAP** — Channel A sensor |
| `speckit.requirement-ids` | Enforces monotonic `FR-###` IDs across renumbers | **WRAP** — spec-belief filter depends on stable IDs |
| `speckit.jira`, `speckit.confluence`, `speckit.maqa` | Enterprise workflow integrations | **IGNORE** — out of SpecERE scope |
| `speckit.pirate-speak`, `speckit.toc-navigation` | Pure stylistic | **IGNORE** |
| `speckit.telemetry` (hypothetical — not present) | No incumbent telemetry extension exists | **EXTEND** — SpecERE **is** this extension |

The gap: no SpecKit extension emits OTel GenAI semconv spans around slash-command execution. That is SpecERE's primary differentiation and why "compose, never clone" does not preclude shipping.

**Verdict.** SpecERE ships six recommended **WRAP** extensions via `specere add` (property-tests, mutation-testing, security-review, v-model, contract-tests, requirement-ids). **EXTEND**s the ecosystem by being the missing telemetry extension. **IGNORE**s enterprise/stylistic entries.

---

## 9. Preset catalog

Lower-leverage than extensions: overlay templates and phrasing; no new commands/hooks. From [docs/reference/presets.md](https://github.com/github/spec-kit/blob/main/docs/reference/presets.md):

| Preset | Effect | SpecERE default? |
|---|---|---|
| **Canon Core** | Minimal SDD lifecycle — drops `/speckit.taskstoissues` | Opt-in; fine for research repos |
| **Explicit Task Dependencies** | Forces every `T###` to list predecessor IDs | **Ship by default** — SpecERE's filter needs the DAG edges |
| **TOC Navigation** | Adds table-of-contents to long specs | Opt-in |
| **Pirate Speak** | Rewrites prompts in pirate dialect | **Hostile** — corrupts EARS-style extraction; never enable |
| **Terse Spec** | Collapses BDD Given/When/Then into one line | **Hostile** — destroys structured acceptance-scenario parsing |
| **Inline Examples** | Adds realistic example FR-NNN per section | Opt-in; useful for onboarding |
| **Strict Priorities** | Forces every user story to carry `P1/P2/P3` and an Independent Test | **Ship by default** — prior for the filter |

**Verdict.** **WRAP** the preset mechanism (`specify preset add`). **EXTEND** by shipping `specere-research` (bundles Explicit Task Dependencies + Strict Priorities). **IGNORE** Pirate Speak and Terse Spec — SpecERE rejects and warns.

---

## 10. Update / upgrade path

Three layers: (1) `uv tool upgrade specify-cli` — global binary, untouched `.specify/`; (2) `specify integration update <key>` — re-renders agent files at current CLI version; (3) per-unit `specify extension/preset/workflow update` (workflow update tracked in #2170).

**Version-pinning contract.** `.specify/integration.json` records `speckit_version` at install (`"0.7.3"` in this repo). Core templates/scripts carry no embedded version; sha256 vs. CLI-bundled copy is the only drift signal. `.specere/manifest.toml` also pins `version = "v0.7.3"` — two sources of truth.

**What breaks when the pinned tag moves.** SpecKit never removes files in place; if 0.7.4 renames `check-prerequisites.sh` → `preflight.sh`, a later `specify integration update` rewrites agent skills to reference `preflight.sh` while the old script sits stale. Silent double-copy, drift compounds.

**SpecERE's signal.** On every invocation, compare `.specere/manifest.toml → units.speckit.version` to `uvx --from git+…@main specify-cli --version` (24h cache). If ahead: print `SpecKit v0.7.4 is available (pinned: v0.7.3). Run: specere update speckit`. Never auto-upgrade — 0.7.x already broke compatibility once (`--ai` → `--integration`).

**Verdict.** **WRAP** all three update commands. **EXTEND** by surfacing the version-skew notice (SpecKit gap). **IGNORE** any SpecERE-native update planner — every path calls `uv tool upgrade` or `specify … update`.

---

## 11. Known bugs / open issues hitting SpecERE's wrapper flow

Scanned [open issues](https://github.com/github/spec-kit/issues) + recent closed-unfixed PRs:

- **#1987** — `init --here` `.gitignore` collisions. Low; SpecERE already uses fenced blocks elsewhere.
- **#2094** — `extension add` silently accepts unknown YAML keys → partial install. Defence: SpecERE validates before upstream.
- **#2141** — `check_feature_branch` rejects `main` even when `.specify/feature.json` exists. The §6 trap. Open, no PR. SpecERE's fix-a neutralises it.
- **#2198** — `workflow run` has no `--dry-run`. Relevant: `specere observe` wants plan-inspection.
- **#2259** — marker-based upsert for context files. Shipped; confirms SpecKit converges on SpecERE's fenced-block pattern.
- **#2294** (closed-unfixed) — `integration uninstall` leaves orphan manifest entries. Cosmetic; SpecERE's manifest is authoritative.
- **Not filed** — "Not on a feature branch when `has_git=true` and `.specify/feature.json` exists." SpecERE should file.

Nothing blocks v1. We actively step on #2141; §6 fix neutralises it.

**Verdict.** **IGNORE** #1987, #2094, #2294. **WRAP-with-caveat** around #2141 (installer fix). **EXTEND** by filing the unfiled one upstream.

---

## 12. Capability matrix — the payoff

One row per capability. "OTel owns" = [OpenTelemetry `gen_ai.*` semconv](https://opentelemetry.io/docs/specs/semconv/gen-ai/) (see [01 — Agent Telemetry (in ReSearch)](https://github.com/laiadlotape/ReSearch/blob/main/docs/research/01_agent_telemetry.md)).

| Capability | SpecKit owns | OTel owns | SpecERE wraps | SpecERE ignores | SpecERE extends | Notes |
|---|:-:|:-:|:-:|:-:|:-:|---|
| Scaffolding `.specify/` tree | ✓ |  | ✓ |  |  | `uvx specify-cli init` |
| Slash-command generation per agent | ✓ |  | ✓ |  |  | via `IntegrationBase` |
| Verb list (`specify`/`clarify`/…) | ✓ |  | ✓ |  |  | 9 verbs, locked |
| Feature-branch creation | ✓ |  | ✓ |  | partial | §6 fix: auto-create `000-baseline` |
| Spec/plan/tasks templates | ✓ |  | ✓ |  |  | `[PLACEHOLDER]` substitution |
| Template override stack | ✓ |  | ✓ |  |  | `overrides/` dir |
| `extensions.yml` hook dispatch | ✓ |  | ✓ |  | ✓ | extend = first real `after_implement` user |
| `condition` evaluation |  |  |  | ✓ |  | defer to HookExecutor |
| Workflow engine (0.7.0) | ✓ |  | ✓ |  | ✓ | ship `specere-observe` workflow |
| Preset stack | ✓ |  | ✓ |  | ✓ | bundle `specere-research` meta-preset |
| Extension catalog | ✓ |  | ✓ |  |  | consume; don't rewrite |
| Per-agent `IntegrationBase` | ✓ |  | ✓ |  |  | never subclass |
| `context_file` marker upsert | ✓ |  | ✓ |  | ✓ | own `<!-- specere:* -->` block |
| Uninstall manifest per integration | ✓ |  | ✓ |  |  | hash-per-file |
| Repo-wide uninstall |  |  |  |  | ✓ | SpecKit gap — `.specere/manifest.toml` |
| CLI update (`uv tool upgrade`) | ✓ |  | ✓ |  |  |  |
| Version-skew notification |  |  |  |  | ✓ | SpecKit gap |
| EARS syntax enforcement |  |  |  |  | ✓ | SpecKit deliberately absent |
| `FR-NNN` ID stability | ✓ (via ext) |  | ✓ |  |  | install `requirement-ids` ext |
| `[NEEDS CLARIFICATION]` markers | ✓ |  | ✓ |  | ✓ | extend = entropy signal input |
| Checklists (spec quality) | ✓ |  | ✓ |  |  |  |
| Cross-artifact analysis (`/analyze`) | ✓ |  | ✓ |  | ✓ | extend = feed filter posterior |
| Task → GitHub issue | ✓ |  | ✓ |  |  | opt-in extension |
| Multi-agent targeting | ✓ |  |  | ✓ |  | Claude Code only for v1 |
| Slash-command execution spans |  | ✓ |  |  | ✓ | SpecERE emits `gen_ai.*` spans |
| LLM prompt/completion capture |  | ✓ |  |  | ✓ | OTel semconv-genai events |
| Tool-call capture (agent → shell) |  | ✓ |  |  | ✓ |  |
| Token-count / cost attributes |  | ✓ |  |  | ✓ |  |
| Error/retry attributes |  | ✓ |  |  | ✓ |  |
| Span-link across workflow steps |  | ✓ |  |  | ✓ | trace the whole `specify → implement` |
| Repo-SLAM sensor map |  |  |  |  | ✓ | `.specere/sensor-map.toml` — SpecERE-native |
| Spec-belief filter (HMM/FGBP/RBPF) |  |  |  |  | ✓ | `prototype/` and research papers |
| SRGM × LLM calibration |  |  |  |  | ✓ | ReSearch §07 |
| OTel collector config |  | ✓ | ✓ |  |  | SpecERE ships a sample `otel-config.yml` |

**22 WRAP / 4 IGNORE / 15 EXTEND**. WRAP:EXTEND ≈ 1.5:1 — the right shape for "compose, never clone."

---

## 13. Recommended SpecERE composition pattern

Concrete, derived from §§2–12:

1. **Installer.** Detect ambient kind. Git → `uvx --from git+…@v0.7.3 specify-cli init . --integration claude --ai-skills --force` **without** `--no-git`, then `git checkout -b 000-baseline` (overridable via `--branch`). Non-git → pass `--no-git`; scripts go permissive. Write `.specere/manifest.toml` with SHA256 per tracked file. Never `--force` on re-install without SHA-diffing first.
2. **Hook registration.** SpecERE hooks live only in `.specify/extensions.yml`. Never embed dispatch into prompts — that is SpecKit's drift tax. `specere add claude-code-deploy` appends an `after_implement` entry pointing to `specere.observe.implement`.
3. **Template overrides.** Never edit `.specify/templates/*` directly. Bias via `.specify/templates/overrides/` and SpecKit's precedence stack (08 §3).
4. **Context-file ownership.** `CLAUDE.md` carries SpecKit + SpecERE content; SpecERE's lives in `<!-- specere:begin {unit-id} --> … <!-- specere:end {unit-id} -->`. One pair per installed unit. Marker-based upsert only ([PR #2259](https://github.com/github/spec-kit/pull/2259)).
5. **Sensor map is SpecERE-native.** `.specere/sensor-map.toml` not touched by SpecKit; `specere-adopt` writes v0; `specere observe` appends Channel B/C at runtime.
6. **Workflow.** Ship one default — `specere-observe` via `specify workflow add`. No parallel orchestrator; reuse `workflow run`.
7. **Namespacing.** All SpecERE slash commands are `specere-*`. Never reuse `speckit-*`.
8. **Uninstall.** `specere remove <unit>` consults `.specere/manifest.toml`, deletes only files whose SHA still matches `sha256_post`, leaves SpecKit core to `specify integration uninstall`.
9. **Update.** `specere update speckit` = version probe → user-confirmed `uv tool upgrade specify-cli` + `specify integration update <key>`. Never auto.
10. **Parse narrowly.** SpecERE parses only `.specify/extensions.yml` (YAML) and `.specere/{manifest,sensor-map}.toml` (TOML). All other SpecKit files are opaque or untouched.

This is SpecERE's governing specification: in scope = what this section says SpecERE does; out of scope = what it says SpecKit owns.

---

## References

- [github/spec-kit — GitHub repository](https://github.com/github/spec-kit)
- [CHANGELOG.md](https://github.com/github/spec-kit/blob/main/CHANGELOG.md)
- [AGENTS.md — integration architecture](https://github.com/github/spec-kit/blob/main/AGENTS.md)
- [docs/reference/core.md](https://github.com/github/spec-kit/blob/main/docs/reference/core.md)
- [docs/reference/extensions.md](https://github.com/github/spec-kit/blob/main/docs/reference/extensions.md)
- [docs/reference/presets.md](https://github.com/github/spec-kit/blob/main/docs/reference/presets.md)
- [docs/reference/workflows.md](https://github.com/github/spec-kit/blob/main/docs/reference/workflows.md)
- [docs/upgrade.md](https://github.com/github/spec-kit/blob/main/docs/upgrade.md)
- [src/specify_cli/__init__.py](https://github.com/github/spec-kit/blob/main/src/specify_cli/__init__.py)
- [extensions/catalog.community.json](https://github.com/github/spec-kit/blob/main/extensions/catalog.community.json)
- [extensions/catalog.official.json](https://github.com/github/spec-kit/blob/main/extensions/catalog.official.json)
- [PR #2259 — marker-based upsert](https://github.com/github/spec-kit/pull/2259)
- [spec-kit issues](https://github.com/github/spec-kit/issues)
- [SpecKit v0.7.3 release](https://github.com/github/spec-kit/releases/tag/v0.7.3)
- [OpenTelemetry semantic conventions — GenAI](https://opentelemetry.io/docs/specs/semconv/gen-ai/)
- [08 — SpecKit Deep Dive](./08_speckit_deepdive.md)
- [01 — Agent Telemetry (in ReSearch)](https://github.com/laiadlotape/ReSearch/blob/main/docs/research/01_agent_telemetry.md)
- Local v0.7.3 artifacts (scaffolded 2026-04-18): `.specify/init-options.json`, `.specify/integrations/*.manifest.json`, `.specify/workflows/speckit/workflow.yml`, `.claude/skills/speckit-*/SKILL.md`, `.specere/manifest.toml`
