# Changelog

All notable changes to SpecERE will be documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project aims to adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once 0.1.0 ships.

## [Unreleased]

### Added (v1.0.5 prep — evidence-quality)

- **`specere calibrate motion-from-evidence` subcommand** (FR-EQ-004, [#73](https://github.com/laiadlotape/specere/issues/73)). Fits per-spec 3×3 motion transition matrices from the event store's `mutation_result` + `test_outcome` history. Each event maps to `(write_class, observed_state)`: `caught`/`pass` → (Good, SAT), `missed`/`timeout`/`fail` → (Bad, VIO). Consecutive-pair counts are Laplace-smoothed (`t[i][j] = (count[i][j] + 1) / (Σ_k count[i][k] + 3)`) and rendered as ready-to-paste `[motion."<id>"]` + `[calibration."<id>"]` TOML tables. Specs with < `--min-events` (default 20) report `insufficient history` — no fabrication. Zero-observation classes fall back to `Motion::prototype_defaults()`; `t_leak` always falls back (requires gap analysis we don't yet do). Kill-rate is re-derived from the mutation subset for the companion `quality` field. 7 unit tests in `specere-filter::motion_fit` + 4 integration tests driving the CLI against seeded events.jsonl fixtures.
- **`specere lint tests` subcommand** (FR-EQ-003, [#72](https://github.com/laiadlotape/specere/issues/72)). Static analysis of `src/**/*.rs` + `tests/**/*.rs` via `syn` AST visitor, flagging three test smells that degrade sensor calibration: `tautological-assert` (`assert!(true)`, `assert_eq!(x, x)`), `no-assertion` (test body has no `assert*!`, no `.unwrap_err`/`.expect_err`, no `?` on a `Result`-returning sig, no `#[should_panic]`), and `mock-only` (≥3 total calls, ≥90% mockall builder methods — `expect_*`, `returning`, `return_const`, `with`, `times`, `in_sequence`, plus `mock_*` / `Mock::new`). Detects `#[test]`, `#[tokio::test]`, `#[async_std::test]`, `#[rstest]` (last-segment "test"/"rstest" match). Both `Expr::Macro` and `Stmt::Macro` visited because syn 2 represents top-level `assert_eq!(...);` as `StmtMacro`, not wrapped. Advisory-only — emits `test_smell_detected` events at INFO severity (per v1 questionnaire), always exits 0. Consumed by `run_filter_run` via the `smell_penalty = clamp(1 − 0.15·n_smells, 0.3, 1.0)` factor in `Calibration::from_evidence`. 10 unit tests + 4 integration tests; syn/proc-macro2/quote/walkdir added as workspace deps.
- **Calibration struct + filter integration** (FR-EQ-002+005+007, [#89](https://github.com/laiadlotape/specere/pull/89)). New `Calibration { quality, alpha_sat, alpha_vio, alpha_unk }` in `specere-filter::state` computed from mutation kill rate × smell penalty. `CalibratedTestSensor` and `PerSpecTestSensor` wrap it. `run_filter_run` aggregates `mutation_result` + `test_smell_detected` events per spec and prints a per-spec calibration summary with `← low evidence` flags when `quality < 0.5`. Repos without evidence events see no change (prototype alphas, bit-identical to v1.0.4). Gate-A parity (FR-P4-002) anchored: `CalibratedTestSensor::new(Calibration::prototype())` produces numerically-identical log-likelihoods to the v1.0.4 `DefaultTestSensor`.
- **`specere evaluate mutations` subcommand** ([#70](https://github.com/laiadlotape/specere/issues/70)). First slice of the evidence-quality upgrade (tracker #69). Wraps `cargo mutants --json` with per-spec scoping (`--scope FR-NNN` or `--in-diff <REF>`) and emits one `mutation_result` event per mutant into `.specere/events.jsonl`. Attribution uses the same directory-boundary semantics as the calibrate path (v1.0.1 fix — `src/auth` doesn't false-match `src/auth_helpers/*`). Tolerant JSON parser handles cargo-mutants v25–v27 schema drift via optional fields + polymorphic `scenario` (string for baseline, `{"Mutant": {...}}` for mutants). 10 new tests: 6 unit tests on the parser + 4 integration tests driving the CLI against fixture outcomes.json files (tests don't require `cargo-mutants` to be installed on CI). Hidden `--from-outcomes` flag enables fixture-driven testing.

## [1.0.4] - 2026-04-19

Three bugs caught during the self-dogfood guide's extended run on real repos. All three now have regression tests and default-rule updates.

### Fixed
- **`specere lint ears` no longer panics on multi-byte UTF-8 in FR lines** (closes #63). `truncate` in `ears_lint.rs` previously sliced a `&str` at a byte offset that could land inside a UTF-8 codepoint (`≥`, `→`, `€`, em-dashes, smart-quotes — all common in technical specs). The panic leaked through the advisory-only contract (exit 0) and silently dropped every would-be finding for the affected spec. `truncate` now snaps `max` back to the nearest char boundary. 4 new `truncate` unit tests + 1 end-to-end `lint_ears_tolerates_multibyte_utf8_in_fr_line` integration test exercising `≥ → ≠ ≤ —`.
- **`specere remove speckit` sweeps orphan `.claude/skills/speckit-git-*`** (closes #64). The upstream `specify integration uninstall` hook doesn't always enumerate the `speckit-git-{commit,feature,initialize,remote,validate}` skill dirs it installed, and they aren't recorded in specere's manifest (speckit is a wrapper unit). Remove now best-effort deletes any `speckit-git-*` directory under `.claude/skills/`. Deliberately does NOT sweep other `speckit-*` skills (`speckit-plan`, `speckit-implement`, …) — those belong to `claude-code-deploy` and are tracked in its manifest. New integration tests in `crates/specere/tests/issue_064_speckit_orphan_skills.rs` verify both the sweep and the preservation of non-speckit-git skills.
- **Default EARS rules accept canonical `SHALL` / `MAY` and domain-prefixed FR IDs** (closes #65). Pre-fix: `ears-fr-prefix` rejected `FR-AUTH-001` / `FR-EDITOR-018` (common convention); `ears-must-should` rejected `SHALL` and `MAY` (canonical EARS imperatives per Mavin et al.). Post-fix: `ears-fr-prefix` pattern is `^\s*-\s*\*\*FR(-P\d+)?(-[A-Z][A-Z0-9]+)?-\d{3,}\*\*:`; `ears-must-should` is `\b(MUST|SHALL|SHOULD|MAY)\b`. New regression `lint_ears_accepts_ears_canonical_shall_and_domain_prefixed_ids`.

### Test count

188 workspace tests (180 → +8: 4 truncate units, 1 UTF-8 integration, 2 speckit-orphan integrations, 1 EARS-canonical regression).

## [1.0.3] - 2026-04-19

Dogfood finding from the self-dogfood guide's T-31 scenario → real bug caught + fixed. Plus a new test plan for interactive agent integration and project-wide status refresh.

### Fixed
- **`specere lint ears` feature.json parser hardened** (closes #61). Replaced the hand-rolled string search with proper `serde_json` deserialisation. The parser now accepts both the speckit convention (`feature_directory`) and a shorter `feature_dir` alias via `#[serde(alias)]`. Pre-fix, the `feature_dir` key errored with `could not parse feature_directory` — surfaced during the self-dogfood guide's T-31 execution. Same treatment applied to `specere-units::orphan::parse_feature_directory` for consistency. 2 new regression tests: `lint_ears_accepts_feature_dir_alias`, `lint_ears_rejects_malformed_feature_json`. Error messages now surface the full `serde_json` chain with line/column for malformed input.

### Added
- **`docs/test-plans/agentic-integration-plan.md`** — new human-walkable plan exercising specere transparently under a live Claude Code session. Covers hooks firing on all 7 `/speckit-*` verbs, span attrs contract, filter feedback loop, calibrate-then-refine cycle. 14-scenario checklist + 4 appendices (hook-wiring reference, debugging missing hooks, mid-test cleanup, known gaps).
- **`docs/test-plans/self-dogfood-guide.md` — T-31 observation recorded** with a pointer to issue #61, plus a new "Observed run log" table tracking which versions pass which scenarios.

### Changed
- **`README.md` phase-status table refreshed** to reflect v1.0.3 shipped. All seven phases now marked ✅; the "not on crates.io" line points at the GitHub Release installer instead.
- **`docs/upcoming.md` priority queue trimmed** to post-v1.0 polish items only: MarkerEntry backwards-compat, Phase 5 motion-matrix-fit tail, CLI RBPF routing, long spec-ID table alignment. Master-plan phase queue closed — v1.x is bug-fix + follow-ups.

## [1.0.2] - 2026-04-19

Closes Phase 4 parity gap. FR-P4-002 now fully satisfied across all three filter paths.

### Added (v1.0.2 prep)
- **FactorGraphBP Gate-A parity** (closes #42). `crates/specere-filter/tests/gate_a_parity.rs::rust_factor_graph_bp_matches_gate_a_fixture_within_2pp` replays the fixture trace through Rust BP and asserts per-cell agreement with the Python prototype's `FactorGraphBP.all_marginals()`. Observed max cell diff: **0.000000** (bit-identical).
- **RBPF Gate-A tail-MAP parity** (closes #42 remainder). `rust_rbpf_matches_gate_a_fixture_tail_map_within_2pp` asserts Rust RBPF's tail-MAP accuracy vs ground truth matches the prototype's within 2 pp (both are 5/8 on the Gate-A fixture). Per-cell RBPF probabilities diverge by more than 2 pp across languages because NumPy `default_rng` and Rust `StdRng` produce different uniform sequences from matched seeds — the divergence is PRNG drift, not algorithmic. FR-P4-002's 2 pp bound is a tail-MAP bound (what the spec actually says), not a per-cell bound.
- **`scripts/export_gate_a_posterior.py` extended** to dump all three filter posteriors (`expected_per_spec_hmm`, `expected_factor_graph_bp`, `expected_rbpf`) on a shared 324-event trace. Fixture now also embeds `[coupling]` and `cluster` so Rust replays identical topology.

## [1.0.1] - 2026-04-18

Depth-audit release for `specere calibrate from-git`. One correctness bug fixed + two UX improvements. See `docs/phase5-calibrate-audit.md` for the full 20-scenario traceability.

### Fixed
- **Path-prefix false-match across sibling directories** (audit C-01 / C-13). Support `"src/auth"` erroneously matched commits touching only `"src/auth_helpers/*"` because `str::starts_with` has no notion of path boundaries. Every user who omitted the trailing slash silently got wrong per-spec touch counts and inflated coupling edges. Fixed by normalising each support entry into `(bare, bare+"/")` and matching against both — exact file equality OR directory-with-separator prefix. Regression tests: `sibling_directories_do_not_false_match`, `trailing_slash_support_is_equivalent_to_bare`, `exact_file_match_works`.
- **Empty-repo UX** (audit C-02). `calibrate from-git` on a repo with no commits yet used to print `fatal: your current branch 'main' does not have any commits yet`. Now: `calibrate: <path> has no commits yet — make at least one commit before running \`specere calibrate\``.
- **Non-git-dir UX** (audit C-11). Running outside a git repository used to print `fatal: not a git repository`. Now: `calibrate: <path> is not a git repository — run \`git init\` first`.

### Validated (no change — documenting for the public record)
- Threshold boundary semantics (`--min-commits` uses `>=`).
- Merge-commit handling (empty merges excluded via `parse_git_log`'s empty-list guard).
- Renames, deletions, binary files, UTF-8 paths with spaces + CJK.
- Output roundtrips cleanly through the `[coupling]` loader.
- Spec-ordering determinism in `sensor-map.toml` does not affect calibrate output.
- `--max-commits` caps exactly, `--repo` override works with absolute paths.

## [1.0.0] - 2026-04-18

**First stable release.** All seven phases of the master plan shipped. Production-ready end-to-end pipeline validated against a 2.2GB real-world target (`memaso`).

### Added (Phase 6 + Phase 7 — v1.0.0 candidate)
- **Cross-session posterior resume** (FR-P6). `run_filter_run` now seeds the filter's belief matrix from the persisted `posterior.toml` before processing new events; previously every invocation reset to uniform, which meant belief never accumulated across processes. New `PerSpecHMM::set_belief` and `FactorGraphBP::set_belief` mutators. 5 new regression tests at `crates/specere/tests/fr_p6_persistence.rs` — bit-identical posterior across restarts, cursor resume, forward-compat with unknown TOML fields, 8-event append sequence across processes.
- **Phase 7 real-world dogfood on memaso** (`docs/phase7-memaso-dogfood.md`). 18-scenario install → calibrate → populate → observe → run → status → verify → remove → re-install round-trip against a 2.2 GB Kotlin/Android/TS project with 80 commits of history. End-to-end pipeline clean. Calibrate surfaced architecturally-meaningful coupling edges on memaso's real history.

### Fixed (Phase 7 findings)
- **FR-P6 blocker** — `run_filter_run` was re-initialising the filter to uniform on every invocation, breaking cross-session belief accumulation. Fixed as above; without this, the persistent-posterior story was fiction.
- **P-15 filter.lock orphan** — `filter-state::remove` now best-effort sweeps `.specere/filter.lock` (ephemeral advisory-lock sidecar from issue #50) so uninstall leaves a clean `.specere/`. Tracked via `EPHEMERAL_SIDECARS` constant.

## [0.5.0] - 2026-04-18

Production-readiness release. First-run experience works end-to-end on a brand-new repo. Delivers Phase 5 partial (coupling-edge calibration from git log), closes issue #50 (advisory file-lock on `filter run`), ships `docs/filter.md`.

### Added (Phase 5 partial + v0.5.0 production polish)
- **`specere calibrate from-git`** (Phase 5 partial). New subcommand that walks `git log`, tallies per-spec co-modification counts, and emits a ready-to-paste `[coupling]` TOML snippet for `.specere/sensor-map.toml`. Configurable via `--max-commits` (default 500) and `--min-commits` (default 3 — co-modifications below this are filtered as coincidences). Greedy DAG filter rejects proposed edges that would close a cycle. New crate module `specere-filter::calibrate` with 7 unit tests + 3 integration tests in `crates/specere/tests/fr_p5_calibrate_from_git.rs`. Dogfood on the specere repo surfaces the expected `cli ↔ units / telemetry / core` coupling. Motion-matrix fit (full FR-P5) deferred — needs a durable test-history source.
- **Advisory file lock on `filter run`** (issue #50 closure). New workspace dep `fs2 = 0.4`. `run_filter_run` now acquires an exclusive lock on `.specere/filter.lock` before loading or writing the posterior; concurrent invocations queue instead of one losing the atomic-write race. Regression test `filter_run_serialises_concurrent_invocations`.
- **`docs/filter.md`** — end-user guide for the filter subcommand. Covers sensor-map schema, event-attr contract, run / status flags, sensor calibration, troubleshooting. The "missing sensor-map" error now points at a real document.

### Fixed (v0.5.0 dogfood pass — docs/phase5-dogfood-report.md)
- **D-04 blocker**: `specere init` now scaffolds a `[specs]` block (with a quick-start comment) so the first `specere filter run` after a clean install doesn't immediately error with "missing [specs]".
- **D-05 blocker**: `filter-state`'s placeholder `posterior.toml` now includes `entries = []`, and `Posterior` deserialisation applies `#[serde(default)]` to `cursor`, `schema_version`, and `entries` so pre-existing placeholder shapes (from pre-v0.5.0 installs) still load cleanly. Regression test `filter_run_tolerates_pre_existing_placeholder_posterior`.

## [0.4.0] - 2026-04-18

First release with a live filter engine. Closes Phase 3 (observe pipeline) and Phase 4 (filter engine) main tracks plus the phase-4-follow-ups (Python-prototype parity + throughput).

### Added (Phase 4 follow-ups — FR-P4-002 + FR-P4-005)
- **Gate-A Python-prototype parity test** (FR-P4-002 closure). New `scripts/export_gate_a_posterior.py` dumps a 324-event trace + expected beliefs from the ReSearch prototype into `crates/specere-filter/tests/fixtures/gate_a/posterior.toml` (one-time export; commit and re-pin only when algorithmic priors change). New integration test `crates/specere-filter/tests/gate_a_parity.rs` replays the trace through Rust `PerSpecHMM` and asserts per-cell absolute difference < 0.02 vs the prototype's final beliefs. Observed max cell diff on this laptop: **0.000000** — Rust output is bit-identical to Python at the fixture's precision.
- **`Motion::prototype_defaults` + `DefaultTestSensor` aligned to prototype** (FR-P4-002 prerequisite). Transition matrices now match `ReSearch/prototype/mini_specs/world.py::build_demo_world` (`t_good`, `t_bad`, `t_leak`) verbatim. `DefaultTestSensor` uses the prototype's `alpha_sat=0.92, alpha_vio=0.90, alpha_unk=0.55` constants with the same 1e-6 log-floor. Hand-computed test expectations in `perspec_hmm_hand_computed.rs` updated to the new matrices.
- **FR-P4-005 throughput test** (`#[ignore]`-gated at `crates/specere/tests/fr_p4_005_throughput.rs`). 10 000-event JSONL benchmark; observed **15 166 events/s** on this laptop (15× the 1000 events/s floor).

### Fixed (manual-test findings, docs/phase4-manual-test-report.md)
- **Cursor advance on out-of-order JSONL** (manual-test M-21, FR-P4-001 regression). `run_filter_run` previously set the cursor to the *last-iterated* event's ts; if events arrived out of order (e.g. a back-dated append), the cursor could retreat and the next re-run would re-process valid events. Now tracks the MAX observed ts. Regression test `filter_run_cursor_advances_to_max_not_last_iteration_ts`.
- **CLI error chain collapsed to top-level message** (manual-test M-04 / M-19). Switched the fallthrough error-print in `main.rs` from `{e}` to `{e:#}` so `anyhow::Context` layers (e.g. TOML parse line/col, rename failure detail) surface to the user.
- **`filter status` on empty-but-existing posterior** (M-07-B). Now prints an actionable "posterior has no entries — no events processed yet" hint instead of a header-only table. Regression test `filter_status_hints_on_empty_posterior`.
- **`filter status --format <unknown>`** (M-15). Previously silently defaulted to table; now errors with `unknown --format` + enumeration of valid values. Regression test `filter_status_rejects_unknown_format`.
- **`filter status --sort <field>,<bad-direction>`** (M-15-B). Previously any non-`asc` direction became `desc`; now errors with `--sort direction must be \`asc\` or \`desc\``. Regression test `filter_status_rejects_bad_sort_direction`.

### Planning
- `docs/phase4-followups-execution-plan.md` — plan covering §2.1 alignment, §2.2 fixture export, §2.3 parity test, §2.4 throughput test, §2.5 manual-test charter, §2.6 release.
- `docs/phase4-manual-test-report.md` — traceability for 24 corner-case manual tests: 19 pass, 3 minor-UX notes, 2 bugs (both fixed in this PR).

### Added (Phase 4)
- **`specere filter run / status` CLI** (issue #43 / FR-P4-001, FR-P4-003, FR-P4-004). New `filter` subcommand tree. `specere filter run` loads specs from `.specere/sensor-map.toml`'s `[specs]` table, reads unconsumed events from the event store (cursor-gated for FR-P4-001 idempotency), drives a `PerSpecHMM` (upgrades to `FactorGraphBP` when `[coupling].edges` is non-empty), and writes `.specere/posterior.toml` atomically via write-then-rename. Posterior entries sorted by spec_id for deterministic serialisation. `specere filter status` reads the posterior and prints a table sorted by entropy desc by default; supports `--sort entropy|p_sat|p_vio|p_unk|spec_id,asc|desc` and `--format table|json`. Event-attr contract (`event_kind=test_outcome|files_touched`, `spec_id`, `outcome`/`paths`) documented in `drive.rs`. 7 new integration tests in `crates/specere/tests/fr_p4_filter_cli.rs` exercise posterior structure, idempotent re-run, entropy-sort default, sort override, JSON output, empty-repo hint, and determinism across invocations. Three new modules in the filter crate: `specs.rs` (sensor-map `[specs]` loader), `posterior.rs` (atomic TOML IO + Entry + Shannon entropy), `drive.rs` (`DefaultTestSensor` + path-list parser).
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
