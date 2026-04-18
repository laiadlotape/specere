# 31 — SpecERE scaffolding design

> **Status.** Design lock pending user validation. Informed by `docs/research/08_speckit_deepdive.md` (SpecKit v0.7.3 analysis, 2026-04-18).
>
> **Scope.** Phase 0 of `30_long_term_tool.md`. Everything here is pre-Gate-C plumbing: no inference math, only the integration surface.

---

## 1. Goal

`specere` is a Rust CLI that scaffolds Repo SLAM into an existing repository through **composable, idempotent, reversible `add` units**. Each unit installs one capability (SpecKit, OTel hooks, Claude Code hook wiring, filter-state directory, EARS linter, …) and each one can be cleanly removed. The mechanism is the *only* day-one product; the filter engine comes later (Phase 3) on top of the telemetry this scaffold captures.

SpecERE's positive differentiator versus SpecKit, Cursor rules, Aider, and Kiro: **a repo-level manifest with first-class uninstall**. SpecKit v0.7.3 has none of this — `deepdive §7`.

---

## 2. Top-level CLI surface

```
specere init [--profile research|prod]        # one-shot: add speckit + filter-state + hooks
specere add    <unit> [unit-flags]             # install one unit, idempotent
specere remove <unit> [--dry-run] [--force]    # reverse one unit using manifest
specere status                                  # list installed units + drift from manifest
specere verify                                  # SHA256-check every manifest entry
specere doctor                                  # preflight/postflight diagnostics
specere observe                                 # invoked by hooks; writes telemetry records
specere version
```

Binary is a single static Rust executable distributed via:

- `cargo install specere` (crates.io)
- `curl -sSfL https://install.specere.dev | sh` (release tarball)
- `nix run github:laiadlotape/specere`
- GitHub release assets (macOS arm64/x86_64, Linux x86_64/aarch64, Windows x86_64)

No Python, no Node. Counter-positioning versus SpecKit's `uv`+Python stack is intentional (`deepdive §10`).

---

## 3. The `add` unit contract

Every `add` unit is the same six-tuple, enforced by a Rust trait:

```rust
pub trait AddUnit {
    fn id(&self) -> &'static str;                             // "speckit", "otel-hooks", …
    fn pinned_version(&self) -> &'static str;                  // specifies upstream tag / internal semver
    fn preflight(&self, ctx: &Ctx) -> Result<Plan>;            // detect existing state, compute ops
    fn install(&self, ctx: &Ctx, plan: &Plan) -> Result<Record>;
    fn postflight(&self, ctx: &Ctx, record: &Record) -> Result<()>;
    fn remove(&self, ctx: &Ctx, record: &Record) -> Result<()>;// must restore repo to pre-install state
}
```

- **`preflight`** is read-only, returns a `Plan` describing operations. `--dry-run` prints it.
- **`install`** performs the plan and returns a `Record` listing every file/dir touched, with SHA256 pre/post.
- **`postflight`** is effects that should not be rolled back on reinstall (e.g., opening a one-time editor).
- **`remove`** consults the `Record` to restore state. "Installed-by-me-unchanged" is reversed; "user-edited-after-install" is flagged and preserved unless `--force`.

**Idempotence invariant.** `add X` followed by `add X` is a no-op; `add X && remove X` restores the working tree to pre-install (modulo postflight side effects, which are documented per unit).

---

## 4. Manifest format

```toml
# .specere/manifest.toml — the source of truth for what SpecERE installed here.
[meta]
specere_version = "0.1.0"
schema_version  = 1
created_at      = "2026-04-18T14:03:00Z"

[[units]]
id              = "speckit"
version         = "v0.7.3"                # upstream tag we pinned
installed_at    = "2026-04-18T14:03:01Z"
install_config  = { integration = "claude", script = "sh", branch_numbering = "sequential" }

  [[units.files]]
  path = ".specify/memory/constitution.md"
  sha256_post = "…"
  owner = "speckit"            # speckit | specere | user-edited-after-install
  role  = "template-output"

  [[units.markers]]             # marker-fenced sections in shared files
  path    = "CLAUDE.md"
  begin   = "<!-- specere:begin speckit -->"
  end     = "<!-- specere:end speckit -->"
  sha256  = "…"
```

**Why TOML, not JSON/YAML.** Rust-native (`toml` crate), human-diffable, comment-friendly, no ambiguous types. Matches `Cargo.toml` ergonomics.

**Drift detection.** `specere verify` re-hashes every `files.path` and compares to `sha256_post`. Mismatches flip `owner` to `user-edited-after-install`.

**Signing (deferred to 0.2).** Day one: unsigned. Post-1.0: sigstore/cosign bundle signatures for catalogued community units, per `deepdive §10`'s "unsigned extension catalogue" anti-pattern.

---

## 5. Marker-based shared-file editing

For every file that SpecERE co-owns with SpecKit, an agent harness, or the user:

```markdown
<!-- specere:begin {unit-id} [{block-id}] -->
… SpecERE-owned content …
<!-- specere:end   {unit-id} [{block-id}] -->
```

`remove` strips the fenced block and any surrounding whitespace it introduced. Content outside markers is **never** touched.

This matches SpecKit 0.7.3 PR #2259's own convention for agent-context upserts (`deepdive §9`); collisions are impossible because unit IDs are our namespace.

---

## 6. Day-one `add` units (MVP)

Five units ship in `specere 0.1.0`. Each follows the §3 contract:

| Unit id              | What it installs                                                           | Pins                          | Removes                                |
|----------------------|----------------------------------------------------------------------------|-------------------------------|----------------------------------------|
| `speckit`            | `.specify/`, `specs/`, agent commands, marker-fenced `CLAUDE.md` block     | SpecKit upstream tag (`v0.7.3`)| Marker-fenced block + `.specify/` if SpecERE created it |
| `filter-state`       | `.specere/` directory, `manifest.toml`, `.gitignore` entries, telemetry sink dir | n/a                     | Full `.specere/` tree (with confirmation) |
| `claude-code-hooks`  | Claude Code hook config (`.claude/hooks/specere.json`) emitting OTLP to the local collector | Claude Code hook schema v1 | Hook entry only                        |
| `otel-collector`     | Local OTel collector scaffolded *by SpecERE*: config, receivers (OTLP gRPC + HTTP), file/SQLite persister at `.specere/telemetry/`, OS-specific run recipe (systemd user unit on Linux, launchd agent on macOS, NSSM/task on Windows) | OTel semconv `gen_ai.*` 2026-04 + collector backend pin | Collector config + run recipe; leaves on-disk telemetry by default (flag `--purge` removes it) |
| `ears-linter`        | `.specere/lint/ears.toml` rules + `pre-commit` hook entry (marker-fenced)  | Our EARS grammar v0.1         | Lint config + `pre-commit` marker      |

**Rationale for the five.**

- `speckit` and `filter-state` are the substrate the paper needs; without them we can't show an end-to-end sensor flow.
- `claude-code-hooks` wires the first concrete telemetry channel (per `01_agent_telemetry.md`) — emits OTLP *to* the collector scaffolded by the next unit.
- `otel-collector` is the **user-explicit decision (2026-04-18)**: the collector is not an external prerequisite; SpecERE scaffolds it. This is the honest multi-harness story from `deepdive §8` and removes the single biggest integration tax end users would otherwise pay.
- `ears-linter` addresses `deepdive §10`'s anti-pattern "no requirement-syntax enforcement" — SpecERE's whole thesis is belief over specs; we cannot leave spec input unconstrained.

**Collector implementation choice.** `specere add otel-collector` ships with two pluggable backends behind a `--backend` flag:

- `embedded` (default) — a minimal OTLP receiver implemented in `specere-telemetry` (tonic gRPC + axum HTTP), writing to `.specere/telemetry/events.sqlite` + JSONL mirror. Single Rust binary, matches our counter-positioning (no Python, no Node, no 150-MB upstream download).
- `contrib` — wraps upstream `opentelemetry-collector-contrib` with a SpecERE-authored config; for users who already run a full collector ecosystem.

The SQLite file is the filter engine's downstream input (Phase 3); JSONL is the human-inspectable parallel. Either way the storage layer is local and offline-capable.

Everything else (`cursor-rules`, `aider-conventions`, `github-actions-telemetry`, `daikon-invariants`, `mutation-testing`, `dashboard`) is post-MVP, built on the same contract.

---

## 7. SpecERE repo layout (the tool scaffolding itself)

```
laiadlotape/specere/
├── Cargo.toml                    # workspace root
├── Cargo.lock
├── rust-toolchain.toml           # pin stable channel
├── LICENSE                       # Apache-2.0
├── README.md
├── CHANGELOG.md                  # keepachangelog.com
├── CONTRIBUTING.md
├── CODE_OF_CONDUCT.md
├── SECURITY.md
├── .github/
│   ├── workflows/
│   │   ├── ci.yml                # fmt + clippy + test + cross-compile
│   │   ├── release.yml           # tag → binary assets + crates.io publish
│   │   └── docs.yml              # mdbook → gh-pages
│   ├── dependabot.yml
│   └── ISSUE_TEMPLATE/
├── crates/
│   ├── specere/                  # the binary: CLI, commands, orchestration
│   ├── specere-core/             # AddUnit trait, Manifest, Ctx, Plan, Record
│   ├── specere-units/            # the five day-one units, each behind a feature flag
│   ├── specere-manifest/         # toml schema, load/save, SHA256 helpers
│   ├── specere-markers/          # marker-fence parser/writer
│   └── specere-telemetry/        # OTel emitter + `specere observe`
├── docs/                         # mdbook source
│   ├── book.toml
│   └── src/
│       ├── intro.md
│       ├── add-unit-contract.md
│       ├── manifest.md
│       ├── markers.md
│       └── units/
│           ├── speckit.md
│           ├── filter-state.md
│           ├── claude-code-hooks.md
│           ├── otel-hooks.md
│           └── ears-linter.md
├── examples/
│   └── dogfood-research/         # integration test: install on a fixture resembling ReSearch
└── xtask/                        # cargo-xtask for release engineering
```

**Professional project abstraction** (from the reboot brief):

- CI must pass fmt + clippy + test + cross-compile before merge to `main`.
- `release-plz` or `cargo-dist` drives releases; tags are the single source of truth.
- mdBook docs auto-deployed to `specere.dev` (GitHub Pages) on every release.
- `cargo-xtask` owns all non-build chores (`cargo xtask docs`, `cargo xtask fixtures`).
- Every public API has `#[deny(missing_docs)]` before 1.0.

---

## 8. Scalability, compatibility, configurability (reboot-brief constraints)

**Scalability.** A new unit is one file in `crates/specere-units/src/<unit>.rs` + one manifest schema entry; no changes to the binary dispatcher (driven by the `AddUnit` trait registry). Community units are possible via a catalogue (post-1.0, signed).

**Compatibility.** Every unit must:
- detect existing non-SpecERE installs (e.g., a hand-rolled `.specify/`) and attach non-destructively;
- never mutate a file outside its marker block;
- tolerate partial prior installs (from a crashed run);
- produce a `remove` that is a true inverse.

Integration tests enforce this on fixtures in `examples/`.

**Configurability.** Each unit takes a small strongly-typed config struct, serialisable to/from `manifest.toml`. Defaults chosen so `specere add <unit>` with no flags is correct on a greenfield repo. A `specere init --profile research` shortcut applies the ReSearch-flavoured defaults.

---

## 9. Validation loop on ReSearch (Definition of Done)

Per user's DoD:

1. **Create** the SpecERE repo under `laiadlotape` (public, Apache-2.0).
2. **Ship** `specere 0.1.0` with at minimum `add speckit` and `remove speckit` working end-to-end.
3. **Run** `specere add speckit` inside `/home/lotape6/Projects/ReSearch` (branch off first).
4. **Ask** the user (interactive question) to validate the result — paths, diffs, whether `CLAUDE.md` block looks right, whether `.specify/` state is what they expected.
5. **Iterate** via `specere remove speckit` and re-run; the uninstall mechanism *is* the human-validation loop tool.
6. **Repeat** for `add filter-state`, `add claude-code-hooks`, `add otel-hooks`, `add ears-linter`.
7. **Commit** the validated state to ReSearch's `main` only after user approval.

The uninstall mechanism is load-bearing for this loop; that is why it is §3/§4 of the design, not an afterthought.

---

## 10. Decisions (2026-04-18)

All blocking decisions resolved via interactive questionnaire:

- **MVP units.** All 5 (speckit, filter-state, claude-code-hooks, otel-collector, ears-linter). `otel-collector` replaces the originally proposed `otel-hooks` per user note — "3 OTel collector and add it to the scaffold mechanism."
- **Layout.** Multi-crate workspace as designed in §7.
- **Distribution.** `cargo-dist` full matrix — crates.io + GitHub releases (macOS arm64/x86_64, Linux x86_64/aarch64, Windows x86_64) + `install.sh` one-liner.
- **Telemetry sink default.** Local OTel collector, scaffolded by SpecERE itself (not a user prerequisite). Embedded backend in `specere-telemetry`; upstream `otelcol-contrib` available behind a flag.

Remaining open (non-blocking for repo creation):

- **`specere init` meta-command.** One-shot `init --profile research` that composes the five `add` units. Proposed default = call all five with research defaults; user can override per-unit. Implement in 0.1.0 as a thin wrapper.
- **Signed catalogue.** Deferred to 0.2.0 per §4.

---

## 11. Post-dogfood corrections (2026-04-18, after first install on ReSearch)

Two critiques from the user after the first `specere add speckit` on ReSearch; both are adopted.

### 11.1 Don't duplicate SpecKit's bookkeeping

**Problem.** The 0.1.0-dev manifest recorded all 26 files SpecKit installed. SpecKit *already* tracks its integration state in `.specify/integrations/integration.json` (and per-component manifests documented in `AGENTS.md`). SpecERE storing a parallel file list is wheel-reinvention and will drift every time SpecKit's internal bookkeeping changes.

**Correction.** Two unit categories going forward:

- **Native units** — filter-state, otel-collector, ears-linter, claude-code-hooks (SpecERE-specific parts). Full manifest per §3–§4: file list, SHA256, marker blocks, first-class `remove`.
- **Wrapper units** — speckit, and any future upstream-tool bindings. Manifest entry is **minimal**: only `(unit_id, pinned_version, installed_at, install_config)`. No file list. `remove` delegates to upstream removal verbs (e.g. `specify integration uninstall <agent>`), with an explicit fallback to a directory wipe behind a confirmation flag.

Trait shape stays the same; the wrapper units implement `install` / `remove` by shelling out instead of tracking files.

### 11.2 Adoption skill — translate existing projects into SpecERE's stack

**Problem.** Scaffolding only works well for greenfield repos. For an existing project (like ReSearch itself), what the user needs is an *agent capability* that reads the repo — code, README, tests, existing docs — and produces a first-pass SpecKit constitution, spec set, and tasks that reflect the project's actual state. Rust scaffolding can't do this alone; it's an LLM job.

**Correction.** Introduce an adoption skill, delivered through the `claude-code-hooks` unit:

- `specere add claude-code-hooks` installs a Claude Code skill at `.claude/skills/specere-adopt/SKILL.md`. The skill prompt reads README, source tree, test suite, existing ADRs/docs; emits a draft constitution, a `specs/000-baseline/spec.md` with FR-NNN IDs, and a `tasks.md` with `[P]` flags — all in SpecKit syntax.
- Invoked from Claude Code by `/specere-adopt`. Output is a draft the human edits before `/speckit.clarify` and `/speckit.plan`.
- The skill is the bridge between "we just installed the scaffold" and "we have a useful SDD starting point that reflects the actual repo."
- Future: analogous `/specere-adopt` skills for other agents (Cursor, Aider) when SpecERE adds those integrations.

This changes `claude-code-hooks` from a pure-plumbing unit (hook config) to a thin plumbing + one shipped skill. It does **not** change the `AddUnit` contract — the skill file is just another entry in the manifest's `files` list.

### 11.3 Consequence for the implementation plan

Both corrections are lightweight to fold in:

- Refactor `crates/specere-units/src/speckit.rs` to the wrapper-unit shape (no file-list recording; delegate `remove` to `specify` + directory-wipe fallback).
- Add `crates/specere-units/src/claude_code_hooks.rs` with the adoption skill prompt embedded as a template.
- Update `specere-core::AddUnit` docs to describe both unit categories (trait stays unchanged).

No breaking changes to the `AddUnit` trait or the `Ctx` / `Plan` / `Record` types.
