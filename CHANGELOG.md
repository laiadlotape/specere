# Changelog

All notable changes to SpecERE will be documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project aims to adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once 0.1.0 ships.

## [Unreleased]

### Added (Phase 4)
- **`RBPF` escape valve** (issue #42 / pre-FR-P4-002). New `rbpf.rs` module — Rao-Blackwellised particle filter for coupling clusters BP cannot converge on. Each particle samples a joint discrete assignment over the designated cluster; non-cluster specs use the per-spec HMM backbone. Weights update by the measurement likelihood conditional on the sampled cluster state; systematic categorical resampling triggers when ESS drops below `resample_ess_frac × N` (default 0.3). Seeded `rand::rngs::StdRng` drives every stochastic step — same seed + same stream ⇒ bit-identical posterior (the determinism invariant #43's golden-file lock needs for FR-P4-004). New workspace dep: `rand = 0.8`. 8 new tests: seeded construction deterministic, different seeds diverge, empty cluster tracks the backbone, cluster-spec concentration under fail-stream, sensor-arity validation, Gate-A-style cyclic-cluster recovery of an injected violation, end-to-end seeded reproducibility, mixed-stream non-degenerate cloud. Strict <2 pp Python-prototype parity (FR-P4-002 closure) tracked as follow-up — needs a one-time export of `prototype/mini_specs/filter.py` on a fixed fixture.
- **`FactorGraphBP` + coupling graph loader** (issue #41 / FR-P4-006). New modules `coupling.rs` (TOML loader for `.specere/sensor-map.toml → [coupling].edges` with DAG enforcement via iterative three-colour DFS — cycle errors name the chain and point at RBPF as the escape valve) and `bp.rs` (loopy BP over directed edges; per-sweep log-domain messages with mean-centring and a `damp = 0.3`, `kappa = 1.4`, `n_iter = 1` prototype default; only message-touched rows renormalise to avoid sub-1e-12 drift on sparse graphs). 13 new tests across unit + integration including hand-traced one-hop/two-hop downstream propagation on a chain and cycle rejection on triangles and self-loops. `PerSpecHMM` internals bumped to `pub(crate)` so BP composes without a second allocation.
- **`specere-filter` crate scaffold + `PerSpecHMM` baseline** (issue #40 / pre-FR-P4-001..006). New workspace member `crates/specere-filter/` (dep: `ndarray = 0.16`) with the three-state simplex `Status::{Unk, Sat, Vio}`, a `TestSensor` trait for log-likelihood emissions, a prototype-ported `Motion` model (`t_good`, `t_bad`, `t_leak` + `assumed_good=0.7`), and an independent per-spec forward recursion. `predict()` advances touched specs via the mixture transition and untouched specs via identity-leak; `update_test()` runs Bayes in log space with a log-sum-exp stabiliser. 9 new tests total: 5 unit (motion row-stochasticity, predict simplex invariance, touched-vs-untouched asymmetry, construction prior) + 4 integration (uniform+pass matches closed-form, predict+pass matches hand-computed posterior, unknown-spec rejection, 100-event no-NaN smoke). Phase 4 execution plan at `docs/phase4-execution-plan.md`.

### Added (Phase 3)
- **`specere serve` OTLP/gRPC receiver** (issue #34 / FR-P3-001 closure). Tonic-based gRPC receiver on `127.0.0.1:4317` (port configurable via `.specere/otel-config.yml → receivers.otlp.protocols.grpc.endpoint` or `--grpc-bind`). Implements `TraceServiceServer` + `LogsServiceServer` from `opentelemetry-proto` 0.31 (`gen-tonic` feature); each span / log record becomes one Event written to the shared SQLite connection. New public `serve_both` runs HTTP + gRPC concurrently over one `Arc<Mutex<rusqlite::Connection>>`, with SIGINT fan-out via `tokio::sync::watch`. CLI: `specere serve --grpc-bind 127.0.0.1:4317` parallels `--bind`. Dev-deps bump: `tonic = "0.14"`, `opentelemetry-proto = "0.31"`, `prost = "0.14"`, `tokio-stream = "0.1"`. 2 new integration tests in `crates/specere/tests/fr_p3_005_serve_grpc.rs` (end-to-end `ExportTraceServiceRequest` round-trip + graceful shutdown).

### Added (Phase 3)
- **Workflow span emission** (issue #31 / FR-P3-002 / FR-P3-006). `claude-code-deploy` now registers 13 additional SpecKit hooks — `before_<verb>` for all 7 workflow verbs (`specify`, `clarify`, `plan`, `tasks`, `analyze`, `checklist`, `implement`) and `after_<verb>` for all six excluding `after_implement` (which stays on the pre-existing bespoke `specere.observe.implement` hook to preserve FR-P1-005). All 13 new hooks call the generic `specere.observe.step` command (`optional: true` — advisory, never blocks `/speckit-*`). New bundled skill `specere-observe-step` in the `claude-code-deploy` unit — reads its hook's prompt, extracts verb + phase, runs `specere observe record --source=<verb> --attr phase=<...> --attr gen_ai.system=claude-code --attr specere.workflow_step=<verb> --feature-dir=$FEATURE_DIR`. 3 regression scenarios in `crates/specere/tests/fr_p3_004_workflow_spans.rs`: 13 hooks present with correct block IDs, skill file shipped, remove round-trip leaves `extensions.yml` byte-identical including cleanup of synthesised verb keys.

### Added (Phase 3)
- **`specere serve` OTLP/HTTP receiver** (issue #30 partial + issue #34 filed for gRPC follow-up). Axum-based HTTP receiver on `127.0.0.1:4318` (port configurable via `.specere/otel-config.yml → receivers.otlp.protocols.http.endpoint` or `--bind`). POST `/v1/traces` parses OTLP/HTTP/JSON payloads, extracts each Span, merges resource + span attributes, writes an Event per span to the SQLite store + JSONL mirror. POST `/v1/logs` acknowledges (persistence symmetric; full extraction deferred). GET/POST `/healthz` returns `ok`. Graceful shutdown via `tokio::signal::ctrl_c` — on SIGINT, the WAL is checkpoint-truncated before exit (FR-P3-005 partial). New `serve` module in `crates/specere-telemetry` + `Command::Serve` in the CLI. 6 new tests: 4 unit (config defaults, YAML parsing, localhost normalisation, path fallback) + 2 integration (end-to-end OTLP/HTTP/JSON round-trip on ephemeral port, graceful shutdown within 5s). gRPC receiver on `:4317` filed as issue #34 per re-plan trigger in docs/phase3-execution-plan.md §5.

### Added (Phase 3)
- **SQLite event store** (issue #29 / FR-P3-003 / FR-P3-004 / FR-P3-005). New `sqlite_backend` module in `crates/specere-telemetry` promotes SQLite at `.specere/events.sqlite` to the primary store; JSONL stays as the human-inspectable mirror. Schema: single `events` table with indexes on `ts`, `source`, `signal`. WAL journal mode + NORMAL synchronous (crash-safe writes + concurrent reads). Auto-backfill from JSONL on first `query` call if SQLite is empty but JSONL has content (migrates post-#28 repos transparently). `rusqlite = "0.32"` (bundled SQLite) added to workspace. 5 unit tests + 4 integration tests in `crates/specere/tests/fr_p3_002_sqlite_backend.rs` including a 10k-event indexed-query smoke within a 2s CI ceiling.

### Added (Phase 3)
- **Event store foundation + `specere observe record/query` CLI** (issue #28 / FR-P3-004 partial). New `event_store` module in `crates/specere-telemetry` with JSONL append-only store at `.specere/events.jsonl`. `Event` struct mirrors a flat OTLP span/log record with `ts`, `source`, `signal`, `name`, `feature_dir`, `attrs`. CLI: `specere observe record --source=<verb> [--feature-dir <p>] [--signal traces|logs] [--name <span>] [--attr KEY=VALUE]...` and `specere observe query [--since <iso>] [--signal <s>] [--source <s>] [--limit N] [--format json|toml|table]`. 7 integration scenarios in `crates/specere/tests/fr_p3_001_event_store.rs` + 5 unit tests in the store module itself. SQLite upgrade (issue #29) and OTLP receivers (issue #30) land next in Phase 3.
- **`docs/phase3-execution-plan.md`** — mirrors Phase 2 execution plan shape; governs issues #27-#31.

### Added (post-Phase-2)
- **`specere lint ears` CLI subcommand** (issue #25). Runs the rules from `.specere/lint/ears.toml` against the active feature's `spec.md` and prints findings as `[SEVERITY rule-id] <bullet-excerpt>`. Always exits 0 (advisory per FR-P2-003). Replaces the agent-only runtime path — the lint is now reproducible in CI via the new integration test `crates/specere/tests/issue_025_ears_lint_cli.rs` (4 scenarios: foo feature with 3 bad bullets, compliant spec, missing feature.json, missing rules). Adds `regex` crate to the workspace dep list.

### Fixed (post-Phase-2)
- **`ears-condition-keyword` rule removed** (issue #25). The rule's `condition_only=true` gate + default `bad_match=false` was self-contradictory — the gate's pattern was the same as the enforcement pattern, so the rule could never fire. Left an explanatory comment in `rules.toml` for future condition-casing rules that would need a separate `trigger_pattern` schema field. The lint runtime treats any remaining `condition_only=true + bad_match=false` rules as no-op for forward compatibility.

### Added (Phase 2)
- **`specere init` meta-command** (FR-P2-005 / issue #15) — one idempotent pass installs all five day-one units in order: `speckit` → `filter-state` → `claude-code-deploy` → `otel-collector` → `ears-linter`. Fail-fast on the first unit error; partial installs are manifest-recorded so `specere remove <unit>` can clean up. 3 regression scenarios in `crates/specere/tests/fr_p2_005_init.rs`: fresh init, idempotent re-init, fail-fast on orphan state preserves no partial work.
- **Multi-owner file fix**: `filter-state` and `claude-code-deploy` no longer record whole-file `FileEntry`s for `.gitignore` and `.specify/extensions.yml` — they co-own these files with other units via disjoint marker-fenced blocks, and whole-file SHA tracking caused false-positive SHA-diff failures on `specere init` idempotent re-runs. `MarkerEntry` records remain authoritative for each unit's owned content.

### Added (Phase 2)
- **`ears-linter` unit** promoted from `stub::StubUnit` to real `AddUnit` at `crates/specere-units/src/ears_linter.rs`. FR-P2-003 / issue #14. Install writes `.specere/lint/ears.toml` (4 lint rules: FR-NNN prefix, MUST/SHOULD presence, EARS condition keywords, ambiguous-adjective avoidance), embeds a `specere-lint-ears` skill at `.claude/skills/specere-lint-ears/SKILL.md`, and registers a `before_clarify` hook in `.specify/extensions.yml` with `optional: true` (advisory only, never blocks any `/speckit-*` command). 4 regression scenarios in `crates/specere/tests/fr_p2_003_ears_linter.rs`. Removes cleanly with byte-identical `.specify/extensions.yml` round-trip.
- The legacy `stub::StubUnit` has been removed from `crates/specere-units/src/lib.rs` — all three previously-stubbed units (filter-state, otel-collector, ears-linter) are now real.
- **`otel-collector` unit** promoted from `stub::StubUnit` to real `AddUnit` at `crates/specere-units/src/otel_collector.rs`. FR-P2-002 / issue #13. Install writes `.specere/otel-config.yml` (OTLP gRPC :4317 + HTTP :4318, batch processor, file exporter to `.specere/events.jsonl`, tuned for `gen_ai.*` semconv). `--service` flag (opt-in) writes a platform-specific service artifact: systemd user unit on Linux, launchd plist on macOS, Task Scheduler README on Windows. Does NOT start the receiver — that's Phase 3's `specere serve`. 4 regression scenarios in `crates/specere/tests/fr_p2_002_otel_collector.rs`.
- **`speckit::preflight` orphan-state detector** (issue #16, carry-over from 2026-04-18 `.specere/decisions.log` EXTEND). Detects orphan `.specify/feature.json` + `specs/NNN-*/spec.md` (template-only) left by aborted `specify workflow run` subprocesses; raises `Error::OrphanFeatureDir` with exit code 8 + actionable help pointing at `specere doctor --clean-orphans`. New subcommand flag sweeps filesystem artifacts (spec dir + feature.json + orphan workflow-run dirs). Does not touch git branches. 4 regression scenarios in `crates/specere/tests/fr_p2_orphan_detector.rs`. New module: `crates/specere-units/src/orphan.rs`.
- **`filter-state` unit** promoted from `stub::StubUnit` to real `AddUnit` at `crates/specere-units/src/filter_state.rs`. FR-P2-001 / issue #12. Install creates `.specere/{events.sqlite, posterior.toml, sensor-map.toml}` skeleton and writes a marker-fenced `.gitignore` block with `.specere/*` + allowlist (`!manifest.toml`, `!sensor-map.toml`, `!review-queue.md`, `!decisions.log`, `!posterior.toml`). Remove is byte-identical round-trip. Idempotent via the existing FR-P1-003 SHA-diff gate. 6 regression scenarios in `crates/specere/tests/fr_p2_001_filter_state.rs`.

### Added
- `AgentBundle` in `crates/specere-units/src/deploy/mod.rs` alongside the existing `SkillBundle`. `Deploy` trait gains `agents()` + `agent_dir()` + `agent_rel_path()` with sensible defaults. Issue #7.
- First SpecERE subagent shipped via `claude-code-deploy`: `specere-reviewer` at `.claude/agents/specere-reviewer.md`, a constitution-compliant PR/diff reviewer. Matches the CI `review` job's prompt but usable interactively via the `Agent` tool. Issue #7.
- Second marker-fenced block in `CLAUDE.md`: `rules`. Contains the 10 composition rules + NEVER-do list, session-durable so every agent invocation sees them up-front (not only on-demand via skills). Sourced from `crates/specere-units/src/deploy/rules/specere-rules.md`. Issue #8.
- `docs/contributing-via-issues.md` — canonical bug/flaw/feature → parent issue → sub-issues → PR pipeline. Linked from `CONTRIBUTING.md`. Issue #9.

### Changed
- `claude-code-deploy`'s install record now lists an additional `MarkerEntry` for `CLAUDE.md` block `rules`, and an agent `FileEntry` under `.claude/agents/`. `remove` inverts both cleanly (byte-identical round-trip per FR-P1-006).
- `CONTRIBUTING.md` now links `docs/contributing-via-issues.md` as the start-here doc.

## [0.2.0] - 2026-04-18

### Release infrastructure
- `cargo-dist@0.31` wired via `dist-workspace.toml`. Generated `.github/workflows/release.yml` produces cross-platform binaries for five target triples (Linux x86_64 + aarch64, macOS x86_64 + aarch64, Windows x86_64) plus `shell` and `powershell` installer scripts on every `v*.*.*` tag push.
- Hand-written `.github/workflows/release-guards.yml` enforces three pre-upload invariants on tag push: tag name matches `Cargo.toml` version (FR-RI-003), `CHANGELOG.md` has a `## [<version>]` section (FR-RI-004), tag commit is reachable from `main` (FR-RI-005).
- `docs/release.md` documents the tag-cut procedure, local reproduction via `dist plan` / `dist build`, rollback steps (delete tag + Release → bit-identical to pre-tag), and the three guard-failure modes.
- Workspace version bumped from `0.2.0-dev` to `0.2.0`.

### Phase-1 post-merge (was [Unreleased])
- `docs/upcoming.md` — lightweight priority queue of the next specs (release-infra, Phase 2 native units, Phase 3 observe pipeline) with carry-over items from `.specere/decisions.log`.
- `docs-sync` CI job in `.github/workflows/ci.yml`. Blocks PRs where `crates/**/*.rs` changes without any `README.md` / `CHANGELOG.md` / `CONTRIBUTING.md` / `docs/**/*.md` / `specs/**/*.md` touch. Escape hatch: include `[skip-docs]` in the PR title or body.
- `Claude PR review` CI job at `.github/workflows/claude-review.yml` using `anthropics/claude-code-action@v1`. Runs on every `opened` / `synchronize` / `reopened` PR event, posts findings as a PR review enforcing the constitution's 10-rule composition pattern, reversibility, per-FR test coverage, cross-platform path safety, doc-sync drift, and the narrow-parse rule. Advisory (does not block). Setup documented in `docs/auto-review.md` (GitHub App preferred; API-key secret as fallback). Skipped on fork PRs.
- `README.md` Status table corrected: Phase 0 marked ✅ Shipped, Phase 1 marked ✅ Merged (PR #2, 9 FRs, 37/37 CI tests green).
- All CI jobs upgraded to `actions/checkout@v6` (absorbs Dependabot PR #1).

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

## [0.2.0-dev] - 2026-04-18 (Phase 1 bugfix release — unreleased)

**Governance.** Spec + clarifications + plan + contracts + 37 tasks all under `specs/002-phase-1-bugfix-0-2-0/`. Constitution (`.specify/memory/constitution.md`) gates every install path; constitution principles I–V passed re-check post-design. Full workspace test sweep: 37 passing across 19 suites.

### Added
- **SpecKit harness dogfood.** `.specify/`, `.claude/skills/speckit-*`, `CLAUDE.md` marker block, `.specere/manifest.toml`, `specere-observe` workflow (`review-spec` → `review-plan` → `divergence-adjudication` gates), `.specere/review-queue.md` for self-extension detection — constitution principle V in action.
- **FR-P1-001**: `speckit` installer detects ambient `.git/` and drops `--no-git`.
- **FR-P1-002**: Auto-created feature branch `000-baseline` (overridable via `--branch <name>` CLI flag or `$SPECERE_FEATURE_BRANCH` env var; flag wins).
- **FR-P1-003**: SHA-diff preflight gate on every reinstall. Refuses with exit code 2 naming the diverged file(s); `--adopt-edits` flips the owner to `user-edited-after-install` and updates the manifest without rewriting. Deletion case refuses separately (exit 4).
- **FR-P1-004**: `claude-code-deploy` unit appends `.claude/settings.local.json` to `.gitignore` inside a marker-fenced block (`# <!-- specere:begin claude-code-deploy --> … # <!-- specere:end claude-code-deploy -->`).
- **FR-P1-005**: `claude-code-deploy` registers exactly one `after_implement` hook in `.specify/extensions.yml` (extension: specere, command: `specere.observe.implement`, optional: false).
- **FR-P1-006**: `specere add <unit> && specere remove <unit>` is byte-identical round-trip — `.gitignore` and `.specify/extensions.yml` SHA-match pre- and post-cycle.
- **FR-P1-007**: Manifest records `install_config.branch_name` + `install_config.branch_was_created_by_specere` on the `speckit` unit. `specere remove speckit --delete-branch` refuses with exit 7 if `branch_was_created_by_specere=false` and with exit 6 if the working tree is dirty.
- **FR-P1-008**: Parse-safety gate extended to all declared formats: YAML (`extensions.yml`), TOML (`.specere/*.toml`), JSON (`workflow-registry.json`), plain text (`.gitignore`). Refuses to rewrite any malformed file with exit code 3, naming the file.
- **FR-P1-009**: Every Phase-1 bug has a dedicated regression test in `crates/specere-units/tests/fr_p1_*.rs`.
- New crates dependencies: `serde_yaml`, `serde_json`, `assert_cmd`, `predicates`.
- New `specere-markers` modules: `text_block_fence` (plain-text) and `yaml_block_fence` (YAML line-comment) — 11 unit tests pass.
- Three new `claude-code-deploy`-bundled skills: `specere-observe-implement`, `specere-review-check`, `specere-review-drain` — embedded via `include_str!`, written to `.claude/skills/specere-*/SKILL.md` on install.
- `specere-core::Error` variants: `AlreadyInstalledMismatch`, `ParseFailure`, `DeletedOwnedFile`, `BranchDirty`, `BranchNotOurs` with stable exit-code mapping per `contracts/cli.md`.
- `SC-008` usability check (aspirational, non-blocking) documented at `docs/lessons/0.2.0-usability.md` if performed.

### Changed
- Workspace version bumped to `0.2.0-dev`; cross-crate path-deps track.
- `specere add` / `specere remove` CLI grew typed flags (`--adopt-edits`, `--branch`, `--delete-branch`) replacing the prior trailing-var-args passthrough.
- `specere add speckit`'s `uvx specify init …` arg list conditionally includes `--no-git` based on `.git/` presence; previous unconditional flag dropped.
- `specere-markers` added line-comment YAML fence convention for `.specify/extensions.yml` (contracts/extensions-mutation.md) since HTML comments don't survive inside YAML block-sequence items. All marker mutations are text-splice — never `serde_yaml::to_string` round-trip — so the git extension's 17 hook entries keep byte-exact formatting (SC-004 load-bearing).

### Breaking changes (from v0.1.0-dev)
- CLI: `specere add <unit> <positional flags>` no longer accepts pass-through flags. New typed flags are `--adopt-edits` and `--branch <name>`.
- Manifest schema: new optional fields on `install_config` — no migration needed for v0.1.x manifests (fields are additive).
