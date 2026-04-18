# SpecERE v1.0 — the big plan

> **Status.** Planning, 2026-04-18 pivot. Governs all pre-1.0 work.
>
> **Rule of rules.** Compose, never clone. Every capability this document enumerates answers one question: "is this something [SpecKit](https://github.com/github/spec-kit) or [OTel](https://opentelemetry.io/docs/specs/semconv/gen-ai/) already does?" If yes, SpecERE **wraps** it. If no, SpecERE **extends**. If it's fluff, SpecERE **ignores**. See `laiadlotape/ReSearch` `docs/research/09_speckit_capabilities.md` §12 for the full capability matrix (22 WRAP / 4 IGNORE / 15 EXTEND) and §13 for the 10-rule composition pattern that governs this whole document.
>
> **Source of truth for theory.** The `ReSearch` monorepo (`docs/analysis/`, `docs/roadmap/`, `docs/research/`, `prototype/`). SpecERE does not re-derive theory; it *implements* what ReSearch has already argued for.

---

## 0. Scope in one paragraph

SpecERE v1.0 is a single Rust binary that installs, observes, and filters a Repo-SLAM loop on a Claude-Code-driven repository. It scaffolds SpecKit artifacts via `specify-cli`, registers one after-hook in `.specify/extensions.yml`, ships one SpecKit workflow, captures agent telemetry through an embedded OTel OTLP receiver into a local SQLite+JSONL store, runs a factor-graph/HMM filter over the event stream to produce a persistent per-spec posterior, calibrates the filter's motion model from git history, and survives cross-session restarts. The target test project is ReSearch itself (its LaTeX paper + foundational booklet + Python prototype). Only Claude Code is a first-class deployer. No venue submission, no multi-agent, no GUI.

---

## 1. Vision: what "done" looks like at v1.0.0

A developer clones ReSearch on a fresh machine, runs `curl -sSfL https://install.specere.dev | sh`, then `cd ReSearch && specere init`, and within ten minutes:

1. `.specify/` is scaffolded on a `000-baseline` branch auto-created by SpecKit's own `create-new-feature.sh`. No `--no-git`. `/speckit-*` slash commands all work.
2. `.specere/manifest.toml` records every unit installed, with SHA256 per owned file.
3. `.specify/extensions.yml` has one `after_implement` hook pointing at `specere.observe.implement`.
4. `.specify/workflows/specere-observe/workflow.yml` is registered; `specify workflow run specere-observe` wraps the whole `specify→plan→tasks→implement` sequence with OTel spans.
5. `specere serve` starts an embedded OTLP receiver on `localhost:4317` and writes every Claude-Code tool-call event into `.specere/events.sqlite` (+ mirror JSONL).
6. `specere filter run` consumes the event stream and writes `.specere/posterior.toml` — a live per-spec belief snapshot.
7. `specere calibrate from-git` walks the repo's git history, fits per-spec motion parameters, and writes `.specere/motion.toml`.
8. On a second session (new shell, new day, new machine with the same repo checkout), `specere status` reports the *same* posterior as it left off — cross-session persistence works.
9. `specere remove <unit>` for any installed unit returns the tree to its pre-install state (bit-identical for non-user-edited files; user edits preserved with a warning).
10. `specere update` is the only privileged upgrade path; it never auto-updates SpecKit.

That scenario is the v1.0 acceptance test, dogfooded on ReSearch.

---

## 2. Governing rules (the 10-rule composition pattern)

Borrowed verbatim from ReSearch/09 §13 and treated as non-negotiable:

1. **Installer** detects git vs non-git and behaves accordingly. On a git repo, `specere add speckit` drops `--no-git` and auto-creates a feature branch (default `000-baseline`, overridable). Never `--force` without a SHA-diff step.
2. **Hook registration** is the *only* way SpecERE runs on `/speckit-*` command boundaries. Hooks live in `.specify/extensions.yml`. SpecERE never embeds dispatch logic into slash-command prompts.
3. **Template overrides** go only in `.specify/templates/overrides/`. SpecERE never edits files under `.specify/templates/` directly.
4. **Context-file ownership** uses `<!-- specere:begin {unit-id} --> … <!-- specere:end {unit-id} -->` markers, one pair per unit. No content outside markers is touched. Ever.
5. **Sensor map** (`.specere/sensor-map.toml`) is SpecERE-native; nothing else reads or writes it.
6. **Workflow** ships as a single SpecKit-registered YAML via `specify workflow add`, not a parallel orchestrator.
7. **Namespacing.** All SpecERE slash commands are `specere-*`. Never reuse or rename `speckit-*`.
8. **Uninstall** consults `.specere/manifest.toml`, compares SHA256, preserves user-edited files, delegates SpecKit core removal to `specify integration uninstall`.
9. **Update** is user-confirmed. `specere update speckit` probes the latest pinned version and invokes `uv tool upgrade specify-cli` + `specify integration update <key>` only after confirmation.
10. **Parse narrowly.** SpecERE parses `.specify/extensions.yml` (YAML) and `.specere/*.toml` (TOML). Every other SpecKit file is opaque.

A tool call from SpecERE violating any of these is a bug, not a feature.

---

## 3. Out of scope for v1.0 (explicit)

- **Other agent harnesses.** No Cursor deployer, no Aider, no OpenCode, no Codex. Claude Code only.
- **Multi-agent joint-state estimation.** One agent per session.
- **GUI / dashboard / web UI.** CLI + TOML + Markdown report only.
- **Signed extension catalog.** Deferred to v0.2 / v2.0 depending on pressure.
- **Parallel workflow orchestrator.** Use SpecKit's.
- **Neural amortised inference / fine-tuned models.** Revisit post-v1.
- **iSAM2 smoothing.** Still implemented as the "Plan B" for retroactive correction, but not a v1 requirement unless a concrete use case materializes mid-build.
- **OTel collector `--backend=contrib` mode.** Only the embedded receiver ships in v1; `contrib` flag deferred to v1.1.
- **EARS compile-time enforcement.** The `ears-linter` unit ships but runs advisory-only in v1 (warn, don't block commits).

---

## 4. Bug backlog from the `/specere-adopt` pass

Every bug becomes a test case after fix.

| # | Behaviour observed | Root cause | Fix | Phase |
|---|---|---|---|---|
| 1 | `/speckit-clarify` fails "Not on a feature branch" | `specere add speckit` passes `--no-git`; `specere-adopt` hand-crafted `specs/000-baseline/` without calling `create-new-feature.sh` | Drop `--no-git` on git repos; auto-create feature branch; `/specere-adopt` skill calls `create-new-feature.sh 000-baseline` | P1 |
| 2 | `--force` on re-install silently overwrites user edits | SpecKit has no SHA-diff gate | SpecERE computes SHA-diff of every file it would touch; refuses to re-install if any file's SHA has changed since last install, unless `specere add <unit> --adopt-edits` | P1 |
| 3 | `.claude/settings.local.json` not gitignored | Outside SpecKit's scope; `specere add claude-code-deploy` should own it | `claude-code-deploy` install adds `.claude/settings.local.json` to `.gitignore` (marker-fenced) | P1 |
| 4 | `.specify/extensions.yml` never produced; post-hook contract untested | No hook-bearing extension had been installed | `specere add claude-code-deploy` writes the first `.specify/extensions.yml` with an `after_implement` hook | P2 |
| 5 | `/speckit.*` vs `/speckit-*` naming drift | **Not a bug** — by-design rewrite (Claude Code skills forbid dots) | No fix; document the rule in the SpecERE CONTRIBUTING | P0 (docs) |

---

## 5. Phase plan

Seven phases, ~20-24 weeks of focused work.

### Phase 0 — Documentation rectification (3 days)

**Deliverable:** the existing SpecERE code compiles clean, docs reflect the pivot, CONTRIBUTING.md explains compose-never-clone and the dot-vs-hyphen rewrite rule.

- **T000** Update `README.md` to cite ReSearch/09 §13 as the governing spec.
- **T001** Extend `CONTRIBUTING.md` with: the 10-rule composition pattern, the dot-vs-hyphen rewrite, the "never edit `.specify/templates/` directly" rule, the uninstall contract.
- **T002** Update `CHANGELOG.md` with the pivot entry.
- **T003** Rewrite this document's §5 phase plan as `docs/roadmap.md` pointer so downstream contributors find it from the repo root.
- **Exit:** `cargo build --release` clean; docs link-check passes; `specere --version` reports `0.1.0-dev` still (no release yet).

### Phase 1 — Bugfix release 0.2.0 (2-3 weeks)

**Deliverable:** a SpecERE install that does not trigger the `/speckit-clarify` branch-check failure, does not silently overwrite user edits, and registers a minimal `after_implement` hook that Claude Code will dispatch.

Unit work:

- **Branch auto-creation.** `speckit::install` detects `.git/` in target repo. If present: drop `--no-git`, invoke `specify-cli init . --integration claude --ai-skills --force` without `--no-git`, then `git checkout -b 000-baseline` unless the user passed `--branch <name>`. If absent: keep `--no-git`. Records the branch name in `.specere/manifest.toml` for `remove` to optionally delete.
- **SHA-diff gate on re-install.** `AddUnit::preflight` gains a "are the files I would write already present with different SHA?" check. If yes, refuse with a pointer to `specere add <unit> --adopt-edits` (which recomputes SHA and records the user's version as `Owner::UserEditedAfterInstall`).
- **`settings.local.json` gitignore.** `claude-code-deploy` unit appends `.claude/settings.local.json` to `.gitignore` inside a `<!-- specere:begin claude-code-deploy -->` block. `remove` strips the block.
- **`.specify/extensions.yml` minimal entry.** `claude-code-deploy` writes or updates `.specify/extensions.yml` with one `after_implement` hook pointing at `specere.observe.implement` — even if `specere observe` itself isn't implemented yet, the hook registration must work (the observe command lands in Phase 3). Hook config:
  ```yaml
  hooks:
    after_implement:
      - extension: specere
        command: specere.observe.implement
        description: Record Repo-SLAM observation from the just-completed implement run
        prompt: "Run `specere observe record --source=implement --feature-dir=$FEATURE_DIR`"
        enabled: true
        optional: false
  ```
- **Test suite.** Integration tests for every bug in §4, each: "before fix this test fails; after fix this test passes."

**FRs.**
- FR-P1-001: `specere add speckit` MUST NOT pass `--no-git` when `<repo>/.git/` exists.
- FR-P1-002: On a git repo, `specere add speckit` MUST leave the working tree on a feature branch named `000-baseline` (or `$SPECERE_FEATURE_BRANCH` if set).
- FR-P1-003: `specere add <unit>` MUST refuse to re-install any file whose current SHA256 differs from its manifest-recorded `sha256_post`, unless invoked with `--adopt-edits`.
- FR-P1-004: `specere add claude-code-deploy` MUST append `.claude/settings.local.json` to the target repo's `.gitignore` inside a marker-fenced SpecERE block.
- FR-P1-005: `specere add claude-code-deploy` MUST register exactly one `after_implement` hook in `.specify/extensions.yml`.
- FR-P1-006: `specere remove claude-code-deploy` MUST strip that `.specify/extensions.yml` hook and the `.gitignore` marker block, leaving the rest of each file intact.

**Exit:** Phase 1 dogfood on ReSearch: `specere init && /speckit-clarify` now runs without a branch error; repeated `specere add` doesn't blow away user edits.

**Release:** `v0.2.0` via `cargo-dist`. CHANGELOG notes the breaking change (users must `remove` and `add` units from v0.1.x; SHA manifest format upgrades).

### Phase 2 — Native units completion (3-4 weeks)

**Deliverable:** the five MVP units named in ReSearch/31 §6 are all implemented, none stubbed. `specere init` composes them into a working first-run experience.

Unit work:

- **`filter-state` unit.** Creates `.specere/events.sqlite` (empty schema), `.specere/posterior.toml` (empty — populated by Phase 4), `.specere/sensor-map.toml` (empty — populated by `/specere-adopt` later). Adds the whole `.specere/` dir to `.gitignore` *except* `manifest.toml` and `sensor-map.toml` (which are repo state). Registers a `before_specify` hook that aborts if `.specere/` is missing (sanity check).
- **`otel-collector` unit.** Writes `.specere/otel-config.yml` (SpecERE-authored config for the embedded receiver, tuned for gen_ai.* semconv). Does NOT start the receiver (that's `specere serve` in Phase 3). Writes a systemd user unit on Linux / launchd plist on macOS / documented `specere serve &` incantation on Windows — but opt-in via `specere add otel-collector --service`.
- **`ears-linter` unit.** Writes `.specere/lint/ears.toml` (rules: FR-NNN MUST/SHOULD register + EARS-style "When/While/Where" phrasing). Registers a `before_clarify` hook that runs `specere lint ears` over `specs/**/spec.md` in advisory mode. Reports warnings, does not block the command.
- **`claude-code-deploy` unit extension.** Beyond the Phase 1 skeleton: install the `/specere-adopt` skill (already in `crates/specere-units/src/deploy/skills/specere-adopt.md`), add `/specere-observe-implement` and `/specere-status` skills that Claude Code can invoke. Each skill is a markdown prompt file under `.claude/skills/specere-*/SKILL.md`.
- **`speckit` unit refactor.** Already a wrapper (v0.1.0-dev), but Phase 1's changes need to land cleanly. Verify the composition-pattern §13.1 (no `--force` without diff) is enforced.
- **`specere init` meta-command.** One-shot: `specere add speckit && specere add filter-state && specere add claude-code-deploy && specere add otel-collector && specere add ears-linter`, each idempotent. Fail-fast on the first error; partial installs are manifest-recorded for clean `remove`.

**FRs.**
- FR-P2-001: `specere add filter-state` MUST create `.specere/{events.sqlite, posterior.toml, sensor-map.toml, manifest.toml}` and MUST NOT create any file outside `.specere/` or `.gitignore`.
- FR-P2-002: `specere add otel-collector` MUST write a working `otel-config.yml` that the Phase-3 embedded receiver accepts without modification.
- FR-P2-003: `specere add ears-linter` MUST NOT block any `/speckit-*` command on lint failures in v1 (warn only).
- FR-P2-004: `specere add claude-code-deploy` MUST install at least three skills: `specere-adopt`, `specere-observe-implement`, `specere-status`.
- FR-P2-005: `specere init` MUST be idempotent — re-running with all units already installed MUST be a no-op and exit 0.
- FR-P2-006: Every unit's `remove` MUST leave the target tree bit-identical to its pre-install state, modulo files with `Owner::UserEditedAfterInstall`.
- FR-P2-007: `specere status` MUST distinguish `[wrapper]`, `[native]`, and `[deploy]` unit shapes in its output.

**Exit:** `specere init` on a fresh ReSearch clone completes in under 60 seconds; all five units present; `specere remove <each>` reverses cleanly.

**Release:** `v0.3.0`.

### Phase 3 — Observe pipeline (3-4 weeks)

**Deliverable:** `specere serve` runs an embedded OTLP receiver and persists events; `specere observe record` invoked by Claude Code hooks feeds it; `specere observe query` returns recorded events as JSON.

Code surface:

- **`specere-telemetry` crate** (already exists, currently stub): implement `Receiver` trait with a `tonic` gRPC server on `localhost:4317` and an `axum` HTTP server on `localhost:4318`. Accept OTLP `Traces` + `Logs` payloads conforming to gen_ai semconv. Persist to SQLite (via `rusqlite` or `sqlx`) with one table per OTLP signal type; JSONL mirror optional.
- **`specere serve` command** in the top-level binary: `specere serve [--bind 127.0.0.1:4317] [--state-dir .specere/]`. Blocks. Handles SIGINT for clean shutdown. Emits its own OTel spans so "the observer observes itself" (useful for Phase 4 calibration).
- **`specere observe record` command**: small client that POSTs events to the local receiver. Intended for hook-driven invocation (`after_implement`, `after_plan`, etc). Fields captured: feature_dir, slash-command name, start_ts, end_ts, duration, agent version, number of tool calls, number of tests run, pass/fail counts, any `[NEEDS CLARIFICATION]` markers added.
- **`specere observe query`**: emits last N events as JSONL / TOML / human-readable table. `--since <iso8601>`, `--limit N`, `--signal traces|logs`.
- **Workflow**: `specify workflow add specere-observe` installs `.specify/workflows/specere-observe/workflow.yml` that wraps `/speckit-specify → clarify → plan → tasks → implement`, adding a `specere observe span open/close` wrapper around each step.

**FRs.**
- FR-P3-001: `specere serve` MUST accept OTLP/gRPC and OTLP/HTTP simultaneously without port conflict.
- FR-P3-002: Every slash command invoked by Claude Code on a SpecERE-scaffolded repo MUST produce at least one OTel span with `gen_ai.system="claude-code"` and `specere.workflow_step="<verb>"`.
- FR-P3-003: The SQLite event store MUST support > 10k events per repo without indexing degradation on a laptop.
- FR-P3-004: `specere observe query --since <T>` MUST return results in ≤ 500ms at the 50th percentile.
- FR-P3-005: `specere serve` MUST survive a SIGINT without corrupting the SQLite store (WAL mode, proper checkpointing).
- FR-P3-006: `specify workflow run specere-observe` MUST end-to-end succeed on a minimal SpecKit repo, producing at least one span per workflow step.

**Exit:** Dogfood on ReSearch — run `/speckit-specify` → `/speckit-clarify` → `/speckit-plan` → `/speckit-tasks` → `/speckit-implement`; after each, `specere observe query --limit 5` shows the span. The event store grows.

**Release:** `v0.4.0`.

### Phase 4 — Filter engine (6-8 weeks)

**Deliverable:** a Rust port of ReSearch's `prototype/mini_specs/filter.py` that runs against the SpecERE event store and writes a persistent per-spec posterior.

Code surface:

- **New crate `specere-filter`**: implements per-spec HMM forward recursion first (PerSpecHMM in prototype terms), then factor-graph loopy BP (FactorGraphBP), then RBPF escape valve. Use `ndarray` + `nalgebra`; do not pull in `tch` or `candle` (keep the binary small). Port the prototype's hyperparameters directly — they're the Gate-A-validated starting point.
- **Sensor ingestion**: map OTel spans from the event store onto the four sensor channels (A: test outcomes; B: reads/greps; C: harness-intrinsic logprob/token signals; D: invariants/PBT/mutation kill-rate). Channel D is optional for v1 (no PBT harness integration yet).
- **`specere filter run` command**: consume events since last posterior snapshot; advance the filter; write `.specere/posterior.toml` (one entry per spec ID with `p(sat)`, `p(vio)`, `p(unk)`, `entropy`, `last_updated`).
- **`specere filter status` command**: read the posterior; print a human-readable table sorted by entropy descending (the specs most in need of a new observation first).
- **Per-spec coupling**: loaded from `.specere/sensor-map.toml`'s optional `coupling` section, which `/specere-adopt` can populate from a hand-authored graph. No automatic coupling inference in v1.

**FRs.**
- FR-P4-001: `specere filter run` MUST be idempotent across repeated invocations with no new events (posterior unchanged).
- FR-P4-002: On a synthetic event stream matching ReSearch's Gate-A scenario, the SpecERE-Rust filter MUST achieve tail MAP-accuracy within 2 percentage points of the Python prototype (`PerSpecHMM`, `FactorGraphBP`, `RBPF` all measured).
- FR-P4-003: `specere filter status` MUST sort specs by entropy descending by default; `--sort p_sat,asc` etc. as alternatives.
- FR-P4-004: Posterior file format (`.specere/posterior.toml`) MUST be deterministic under fixed seed and fixed event stream.
- FR-P4-005: Filter engine MUST NOT regress runtime below 1000 events/second throughput on a laptop.
- FR-P4-006: Coupling graph loading MUST reject cycles with an actionable error (cycles break BP convergence guarantees on trees and need explicit RBPF routing).

**Exit:** On ReSearch, run `specere serve` + a Claude Code session that works on a feature; `specere filter run` produces a posterior; `specere filter status` reports plausibly. Compare prototype on the same trace (via a synthetic export) — numbers within 2pp.

**Release:** `v0.5.0` — first release with a filter engine.

### Phase 5 — Motion-model calibration from git history (3-4 weeks)

**Deliverable:** `specere calibrate from-git` walks the repo's git log, reconstructs (diff, test-delta) pairs, and fits per-spec motion parameters.

Code surface:

- **Git walker** using `git2` (libgit2 bindings): enumerate every commit on `main` touching files in the spec support sets (derived from `.specere/sensor-map.toml`). For each (commit, spec) pair where the commit touched the spec's support files, check whether tests flipped at that commit. The (touched, test-flip) pairs feed per-spec motion-matrix estimation.
- **Parameter estimator**: MAP / MLE for the 3×3 transition matrices conditioned on write quality (good/bad — derived from "did tests pass after the commit?"). Collateral rate η_i from unrelated-touched flip rate. Writes `.specere/motion.toml`.
- **`specere calibrate from-git` command**: `--since <rev|date>`, `--dry-run` (prints would-be estimates without persisting), `--only-spec FR-001` for targeted recomputation.
- **Integration with `specere filter run`**: on first run after calibration, the filter swaps its hard-coded hyperparameters for the calibrated ones. Posterior includes `motion_source: "calibrated" | "default"` in its TOML header.

**FRs.**
- FR-P5-001: `specere calibrate from-git` MUST produce a `.specere/motion.toml` whose transition matrices are row-stochastic to within 1e-9.
- FR-P5-002: On ReSearch's own history (post-Phase-4), calibration MUST complete in under 2 minutes.
- FR-P5-003: `specere filter run` after calibration MUST use the calibrated matrices by default; `--motion-source=default` overrides.
- FR-P5-004: Calibration MUST be monotone — adding more commits to the input MUST NOT decrease posterior accuracy on held-out commits (tested via a 80/20 split).
- FR-P5-005: `specere calibrate` MUST NOT write to any file outside `.specere/`.

**Exit:** ReSearch's own git history feeds `specere calibrate from-git`; the filter's motion matrices update; `specere filter run` replays the event store with calibrated parameters; posterior is materially different (measurable entropy delta).

**Release:** `v0.6.0`.

### Phase 6 — Cross-session persistence (2-3 weeks)

**Deliverable:** the filter's posterior survives across sessions; a second clone on a new machine with the same repo and event export resumes the same posterior.

Code surface:

- **Posterior checkpointing**: `specere filter run` writes not just `.specere/posterior.toml` but also `.specere/posterior.lock` (JSON, deterministic) capturing the last event ID processed and the RBPF particle states. Commit these files.
- **Event-log portability**: SQLite event store is a single file committed to `.specere/` by default (gitignored, because it can grow; but can be exported to JSONL which IS commitable). `specere state export` / `specere state import` commands.
- **`specere filter resume` command**: reads the lock, replays any new events since that ID, advances the posterior. This is the happy-path `filter run` mode once cross-session works — in fact, `filter run` becomes `filter resume` under the hood.
- **Cross-machine test**: `git clone && specere state import events.jsonl && specere filter resume` on a machine that never saw the original run, producing the same posterior.

**FRs.**
- FR-P6-001: `specere filter run` MUST write both `posterior.toml` and `posterior.lock`.
- FR-P6-002: On the same repo+event export, `specere filter resume` on two different machines MUST produce bit-identical `posterior.toml` files (given fixed seed).
- FR-P6-003: `specere state export --output events.jsonl` MUST produce a file that round-trips through `specere state import` to a SQLite store with identical contents.
- FR-P6-004: `.specere/posterior.lock` format MUST be stable across minor version bumps; v1.0 → v1.1 reads old locks.
- FR-P6-005: `specere status --check-persistence` MUST report whether posterior is current (last event applied) or stale (new events in store since last run).

**Exit:** The "§6.3 experiment" from ReSearch's 10_research_paper.md, run in practice on ReSearch: two sessions on the same repo, starting one day apart, see the same posterior. This is the distinctive Repo-SLAM demonstration.

**Release:** `v0.7.0`.

### Phase 7 — Dogfood verification + v1.0 release (2 weeks)

**Deliverable:** a tear-down-and-rebuild-from-scratch on ReSearch that passes end-to-end, then a `v1.0.0` release.

Protocol (the final acceptance test):

1. On a clean checkout of ReSearch (post-Phase-6): `specere remove --all`. Tree must be bit-identical to the pre-SpecERE state (no `.specify/`, no `.specere/`, no `CLAUDE.md`, no `.claude/skills/specere-*`).
2. `cargo install specere` fresh. `specere --version` reports `1.0.0-rc.N`.
3. `cd ReSearch && specere init`. Wait < 60 s.
4. Start a Claude Code session; run `/specere-adopt` to regenerate `specs/000-baseline/` from scratch. Compare against the previous adoption pass — diff should be minimal (same FRs).
5. Walk through `/speckit-specify → clarify → plan → tasks → implement` for one small feature (a new `00N_*.tex` chapter of the foundational booklet, say).
6. `specere serve` runs in parallel, `specere observe query` shows spans.
7. `specere filter run` produces a non-trivial posterior.
8. `specere calibrate from-git` updates motion parameters.
9. Save `.specere/posterior.lock` + `.specere/events.jsonl`; exit.
10. Re-clone ReSearch on a different machine; `specere state import events.jsonl`; `specere filter resume`. Posterior matches.

Any step failing means v1.0 doesn't ship.

**FRs.**
- FR-P7-001: The full protocol MUST complete on a fresh Ubuntu 24.04 LTS laptop within 30 minutes total wall-clock.
- FR-P7-002: At least one `/specere-observe-implement` hook MUST fire during step (5) and be reflected in the event store.
- FR-P7-003: Step (9) MUST export fewer than 10 MB of state for ReSearch's size (sub-linear in repo size).

**Release:** `v1.0.0`. CHANGELOG, SECURITY advisory review, GitHub release notes, mdBook docs deployed to `specere.dev` (or equivalent).

---

## 6. Functional requirements — master list

Aggregated from §5 for quick lookup. Phase-prefixed IDs (FR-P1-001, etc.) for traceability; a total of 36 FRs across phases.

| ID | Phase | Summary |
|---|---|---|
| FR-P1-001 | 1 | No `--no-git` on git repos |
| FR-P1-002 | 1 | Auto-create `000-baseline` feature branch |
| FR-P1-003 | 1 | SHA-diff gate on re-install |
| FR-P1-004 | 1 | Gitignore `.claude/settings.local.json` |
| FR-P1-005 | 1 | Register one `after_implement` hook |
| FR-P1-006 | 1 | Remove reverses hook + gitignore block |
| FR-P2-001 | 2 | `filter-state` contained in `.specere/` |
| FR-P2-002 | 2 | `otel-collector` emits a working config |
| FR-P2-003 | 2 | `ears-linter` warns, never blocks |
| FR-P2-004 | 2 | Three `specere-*` skills installed |
| FR-P2-005 | 2 | `specere init` idempotent |
| FR-P2-006 | 2 | Every `remove` bit-identical restore |
| FR-P2-007 | 2 | `status` distinguishes unit shapes |
| FR-P3-001 | 3 | OTLP gRPC + HTTP simultaneously |
| FR-P3-002 | 3 | Every slash command produces a span |
| FR-P3-003 | 3 | >10k events without index degradation |
| FR-P3-004 | 3 | `observe query` <500 ms p50 |
| FR-P3-005 | 3 | SIGINT-safe SQLite |
| FR-P3-006 | 3 | `specere-observe` workflow end-to-end |
| FR-P4-001 | 4 | `filter run` idempotent |
| FR-P4-002 | 4 | Filter within 2pp of Python prototype |
| FR-P4-003 | 4 | Status sorted by entropy by default |
| FR-P4-004 | 4 | Posterior TOML deterministic |
| FR-P4-005 | 4 | ≥1000 events/s throughput |
| FR-P4-006 | 4 | Coupling-graph cycles rejected |
| FR-P5-001 | 5 | Row-stochastic motion matrices |
| FR-P5-002 | 5 | Calibration <2 min on ReSearch |
| FR-P5-003 | 5 | Filter uses calibrated by default |
| FR-P5-004 | 5 | Calibration monotone on held-out |
| FR-P5-005 | 5 | Writes only inside `.specere/` |
| FR-P6-001 | 6 | Posterior + lock files written |
| FR-P6-002 | 6 | Bit-identical posterior across machines |
| FR-P6-003 | 6 | Event export round-trips |
| FR-P6-004 | 6 | Lock format stable across minor versions |
| FR-P6-005 | 6 | `status --check-persistence` works |
| FR-P7-001 | 7 | Full protocol <30 min |
| FR-P7-002 | 7 | `after_implement` hook fires |
| FR-P7-003 | 7 | Export <10 MB for ReSearch |

## 7. Success criteria (SC-NNN)

The measurable outcomes v1.0 is judged on, independent of phase.

- **SC-001**: All 36 FRs in §6 pass their Phase-exit tests.
- **SC-002**: The dogfood protocol §5.P7 completes successfully on a fresh laptop.
- **SC-003**: `specere --help` is understandable to someone who has read only ReSearch's foundational booklet (not the paper, not the roadmap). *(Test: a non-technical reviewer reads the help output and can correctly predict what `specere init` will do.)*
- **SC-004**: `cargo-dist` ships binaries for macOS arm64/x86_64, Linux x86_64/aarch64, Windows x86_64 within 10 minutes of a tag push.
- **SC-005**: `cargo test --workspace` passes on CI for every phase-exit commit; no flaky tests (10-run stability).
- **SC-006**: Total binary size < 15 MB for any target (strip + LTO).
- **SC-007**: Zero files touched outside `.specify/`, `.specere/`, `.claude/`, `specs/`, `CLAUDE.md`, `.gitignore` during any `specere add|init|remove` invocation.

## 8. Architecture notes

### 8.1 Workspace layout evolution

Existing (2026-04-18 post-pivot):

```
crates/specere           # binary: CLI dispatch
crates/specere-core       # AddUnit trait, Ctx, Plan, Record, Owner
crates/specere-manifest   # .specere/manifest.toml load/save, SHA256
crates/specere-markers    # marker-fenced shared-file editing
crates/specere-telemetry  # observe stub
crates/specere-units      # 5 units + deploy trait (claude-code-deploy)
```

Phase 3 adds no new crate (telemetry takes the weight). Phase 4 adds:

```
crates/specere-filter     # PerSpecHMM + FactorGraphBP + RBPF + posterior IO
```

Phase 5 does not add a crate; calibration lives in a `calibrate` module under `specere-filter`. Phase 6 does not add a crate; persistence lives partly in `specere-filter` (checkpointing) and partly in `specere-telemetry` (event export/import).

Final v1.0 workspace: 7 crates.

### 8.2 Dependency discipline

- **Allowed:** `tokio`, `tonic`, `axum`, `serde`, `toml`, `tracing`, `clap`, `sha2`, `hex`, `rusqlite` or `sqlx` (pick one), `ndarray` or `nalgebra` (pick one), `git2`, `time`, `anyhow`, `thiserror`, `walkdir`, `tempfile`.
- **Forbidden:** `tch`, `candle`, `burn`, `torch-*` — anything that pulls a gigabyte of ML framework.
- **Forbidden:** fork or vendor any SpecKit Python code. Compose via `uvx` invocation only.

### 8.3 Compose-never-clone enforcement

- Every PR gets a lint that scans for file writes to `.specify/templates/*` (not `/overrides/`) and blocks if present.
- Every integration test asserts: after a `specere add <unit>`, the set of files in `.specify/` has not been modified by SpecERE itself; any changes must come from the upstream `specify-cli` invocation.

## 9. Testing strategy

Four layers:

1. **Unit tests** — per-crate, per-module. Pure Rust. `cargo test --workspace`.
2. **Integration tests** — per-unit, per-add/remove round-trip. Run on a `tempfile::tempdir` against a fixture repo. Require `uvx` in CI (install `uv` as a preflight step).
3. **End-to-end tests** — exercise the whole `specere init → filter run → filter resume` flow on a copy of ReSearch's skeleton. Slow; run on nightly CI only.
4. **Dogfood tests** — the §5.P7 protocol, run before every release tag.

CI matrix: Ubuntu + macOS + Windows for layers 1 and 2. Ubuntu only for layers 3 and 4 (to keep CI time bounded).

## 10. Release engineering

- `cargo-dist` drives all binary releases.
- Semver: any breaking CLI change (flag rename, subcommand removal, manifest format break) bumps major; any new unit, new command, or new flag bumps minor; bug fixes bump patch.
- Every release ships a CHANGELOG diff and (if needed) a migration note — especially for manifest-format breaks.
- v1.0.0 ships with a SECURITY.md update covering the new attack surfaces (command execution via `uvx`, file writes via install, SQLite DB exposure via `specere serve`).
- Public docs deploy as mdBook to GitHub Pages on every tag.

## 11. Risks

| ID | Risk | Mitigation | Owner phase |
|---|---|---|---|
| R1 | SpecKit v0.7.3 → v0.8.x breaks our manifest format | Pin the version; test against a moving-pin CI job that flags incompatibilities | Phase 1 |
| R2 | OTLP receiver grows scope until SpecERE owns a full collector | Restrict to gen_ai.* + `specere.*` attributes; reject unknown schemas with a log + drop | Phase 3 |
| R3 | Rust filter diverges numerically from Python prototype | Port with fixed-seed parity tests from Phase 4 onward | Phase 4 |
| R4 | Git-history calibration overfits a small repo like ReSearch | Show held-out metric in the calibrate output; block promotion to "default" if monotone check fails | Phase 5 |
| R5 | Cross-session posterior bit-identity breaks on floating-point non-determinism | Use integer-arithmetic log-domain updates where possible; serialize to TOML with fixed decimal precision | Phase 6 |
| R6 | v1.0 acceptance protocol takes too long on a laptop | Add a `--fast` flag that truncates event replay; keep full protocol for CI nightly | Phase 7 |
| R7 | Scope creep — Cursor / Aider deployers added mid-build | §3 is non-negotiable for v1; add to v1.x roadmap explicitly | All |

## 12. Dogfood verification protocol

See §5.P7. Repeated here as a literal checklist for the final acceptance test:

```
[ ] 1. Clean ReSearch clone: `git clone git@github.com:laiadlotape/ReSearch && cd ReSearch`
[ ] 2. Uninstall any prior SpecERE state: `specere remove --all` (must report clean if nothing installed)
[ ] 3. Install SpecERE fresh: `cargo install specere`
[ ] 4. `specere --version` reports 1.0.0 (or rc.N)
[ ] 5. `specere init` completes in < 60 s
[ ] 6. `.specify/`, `.specere/`, `.claude/skills/specere-*`, `CLAUDE.md` all present
[ ] 7. `specere status` reports 5 units installed
[ ] 8. Start Claude Code; run `/specere-adopt`; adopt pass completes
[ ] 9. `specs/000-baseline/spec.md` exists with at least 10 FRs
[ ] 10. `specere serve &` in background
[ ] 11. `/speckit-specify` through `/speckit-implement` on a small feature
[ ] 12. `specere observe query --limit 20` shows spans from each verb
[ ] 13. `specere filter run` produces `.specere/posterior.toml`
[ ] 14. `specere calibrate from-git` produces `.specere/motion.toml`
[ ] 15. `specere state export --output events.jsonl`
[ ] 16. Export < 10 MB
[ ] 17. On a DIFFERENT machine: `git clone && specere init && specere state import events.jsonl && specere filter resume`
[ ] 18. `posterior.toml` on machine 2 is bit-identical to machine 1
[ ] 19. `specere remove --all`
[ ] 20. Working tree returns to step-1 state (verified by `git status`)
```

If all 20 boxes tick, v1.0.0 ships.
