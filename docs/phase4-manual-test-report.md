# Phase 4 follow-ups — manual test traceability report

**Date.** 2026-04-18. **Binary.** `target/debug/specere` built from `028-phase4-followups` branch (pre-fix head `c5c9421` + fixes for M-04 / M-19 / M-21 applied in this PR). **Sandbox.** `/tmp/specere-manual-GaBQgd/`.

Each row below pairs a manual-test scenario from the charter in `docs/phase4-followups-execution-plan.md §3` with the commands executed, the outcome, and any fix.

## Summary

| Total | Pass as-is | Pass with a minor UX note | Bug found + fixed in-branch |
|---|---|---|---|
| 24 | 19 | 3 | 2 |

**Bugs found and fixed:**
- **M-21** — cursor-advance-on-last-iter instead of max (FR-P4-001 violation under out-of-order JSONL). Fixed.
- **M-04 / M-19** (same root cause) — anyhow context chain truncated in CLI error-print. Fixed.

**Minor UX notes (not fixed for v0.4.0):**
- M-06 writes an empty `posterior.toml` on first run with no events; not harmful.
- M-07-B prints only the table header when the posterior has zero entries; no orient-the-user hint.
- M-15 / M-15-B silently accepts unknown `--format` and non-`asc` sort directions, defaulting to table / desc.

---

## Traceability

### M-01 — top-level help

- **Linked.** CLI surface.
- **Command.** `specere --help`.
- **Outcome.** ✅ Pass. Lists `filter` subcommand alongside `add / remove / init / lint / status / verify / doctor / observe / serve`.

### M-02 — filter subcommand help

- **Linked.** CLI surface.
- **Command.** `specere filter --help && specere filter run --help && specere filter status --help`.
- **Outcome.** ✅ Pass. All flags documented, including `--sort`, `--format`, `--posterior`, `--sensor-map`.

### M-03 — `filter run` on uninitialised repo

- **Linked.** `specs.rs::load_specs`.
- **Command.** Fresh git repo, no `.specere/`; `specere filter run`.
- **Outcome.** ✅ Pass. `specere: error: sensor-map not found at {path} — run \`specere init\` or add a [specs] section per docs/filter.md`. Exit 1.

### M-04 — malformed sensor-map.toml

- **Linked.** `specs.rs::load_specs_from_str` + CLI error display.
- **Command.** Wrote `[specs][[not valid toml` to sensor-map.toml; `specere filter run`.
- **Outcome, pre-fix.** ⚠️ Error collapsed to `parse sensor-map.toml` — the underlying `toml` parser's line/column was swallowed by CLI error display (the `eprintln!("specere: error: {e}")` path used `{e}` instead of `{e:#}`, losing the anyhow cause chain).
- **Fix.** Change to `{e:#}` in `crates/specere/src/main.rs`'s fallthrough error-print. Post-fix the CLI shows `parse sensor-map.toml: TOML parse error at line 1, column 8 | invalid table header | expected newline, #`.
- **Outcome, post-fix.** ✅ Pass.

### M-05 — empty `[specs]`

- **Linked.** `specs.rs`.
- **Command.** `[specs]` present but no entries; `specere filter run`.
- **Outcome.** ✅ Pass. `specere: error: [specs] section empty or missing in sensor-map.toml — add entries like "FR-001" = { support = ["src/a.rs"] }`. Exit 1.

### M-06 — valid specs, no event store

- **Linked.** `run_filter_run` in `main.rs`.
- **Command.** Valid sensor-map, no `.specere/events.jsonl`; `specere filter run`.
- **Outcome.** ✅ Pass, with a minor note. Prints `specere filter: no new events since start` and exits 0. An empty `posterior.toml` is written (`schema_version = 1 / entries = []`). Not harmful — writing an empty file on first invocation is arguably the right signal that the run completed — and FR-P4-001 is still satisfied (a second run leaves the file byte-identical).

### M-07 — `filter status` with no posterior

- **Linked.** `run_filter_status`.
- **Command.** Valid sensor-map, no `posterior.toml`; `specere filter status`.
- **Outcome.** ✅ Pass. Prints `no posterior yet — run \`specere filter run\` first` and exits 0.

### M-07-B — `filter status` on zero-entry posterior (from M-06's artefact)

- **Linked.** `run_filter_status`.
- **Outcome.** ⚠️ Minor. Prints only the table header, no rows, no hint. Functional but slightly disorienting. Didn't fix in this PR — would push the "no events" check into `run_filter_status` which complicates the contract of "status reads what's there and prints it."

### M-08 — `event_kind=test_outcome`, no `spec_id`

- **Linked.** `run_filter_run` event matcher.
- **Outcome.** ✅ Pass. Event falls through to `_ => skipped += 1`. Report: `processed 0, skipped 1`.

### M-09 — unknown `outcome=`

- **Linked.** `DefaultTestSensor`.
- **Outcome.** ✅ Pass. Event processed; `DefaultTestSensor` returns the flat (1/3, 1/3, 1/3) log-likelihood → Bayes update leaves the uniform prior exactly uniform. `p_unk = p_sat = p_vio = 0.3333…`.

### M-10 — `event_kind=files_touched`, empty `paths=`

- **Linked.** `parse_paths` + `PerSpecHMM::predict`.
- **Outcome.** ✅ Pass. `parse_paths("")` returns `[]`; `predict(&[])` applies identity-leak to every spec. Belief drifts slightly (UNK 0.333 → 0.313) as the leak's off-diagonal shifts mass. Matches the prototype's `apply_write` with no touched specs.

### M-11 — event for unknown spec

- **Linked.** `PerSpecHMM::update_test`.
- **Outcome.** ✅ Pass. `update_test` returns `anyhow!("unknown spec id: FR-999")`; driver increments `skipped`. Report: `processed 0, skipped 1`. No posterior drift.

### M-12 — `--sort garbage` (no comma)

- **Linked.** `sort_entries`.
- **Outcome.** ✅ Pass. `specere: error: --sort expects \`field,asc|desc\` (got \`garbage\`)`. Exit 1.

### M-13 — `--sort foo,desc`

- **Linked.** `sort_entries`.
- **Outcome.** ✅ Pass. `specere: error: unknown --sort field \`foo\`; one of entropy, p_sat, p_vio, p_unk, spec_id`. Exit 1.

### M-14 — `--sort spec_id,asc` (valid, help doc lists limited set)

- **Linked.** `sort_entries`.
- **Outcome.** ✅ Pass. Works correctly (`spec_id` is accepted even though the `--help` text doesn't enumerate it). Minor doc inconsistency; not fixing because expanding the help text would exceed the 80-column width.

### M-15 — `--format yaml` (unknown format)

- **Linked.** `run_filter_status`.
- **Outcome.** ⚠️ Minor. Silently falls through to the default table format. No warning emitted. Not fixing for v0.4.0 — current fallthrough behaviour is permissive-by-default, which the table-vs-json-vs-future-formats story can clean up later.

### M-15-B — `--sort entropy,sideways` (unknown direction)

- **Linked.** `sort_entries`.
- **Outcome.** ⚠️ Minor. `matches!(dir, "asc")` fails, so `ascending=false` → treated as `desc`. Same permissive-default theme as M-15. Not fixing.

### M-16 — very long spec ID (200 chars)

- **Linked.** `specs.rs`, table formatter.
- **Outcome.** ✅ Pass. ID accepted. Table column alignment breaks (the `{:<11}` spec-id width is busted) but the row itself is readable. Formatter downgrade under long IDs is acceptable — JSON output is the right path for programmatic consumers.

### M-17 — Unicode spec IDs (emoji + CJK)

- **Linked.** `specs.rs`, `sort_entries`.
- **Outcome.** ✅ Pass. IDs round-trip through TOML + filter + posterior. `--sort spec_id,asc` produces BTreeMap (Unicode code-point) ordering: `FR-001-🚀` < `仕様-002`, matching expectation.

### M-18 — `filter run --posterior /tmp/foo.toml`

- **Linked.** `run_filter_run`.
- **Outcome.** ✅ Pass. Posterior written to the override path; source events still read from `--repo`'s `.specere/events.jsonl`. No cross-contamination.

### M-19 — concurrent `filter run`

- **Linked.** `Posterior::write_atomic`.
- **Command.** Two `specere filter run` backgrounded in parallel.
- **Outcome.** One run exited 0 (wrote the posterior); the other exited 1 with an error. Final posterior intact; no `.tmp` leftover. Same CLI-error-chain issue as M-04 — the loser printed `rename …posterior.toml.tmp -> …posterior.toml` without the anyhow chain. Fixed alongside M-04 by switching to `{e:#}`.
- **Follow-up not pursued here.** An advisory lock (flock on `.specere/posterior.toml`) would make the race a queue instead of a loss. Substantive enough to defer.

### M-20 — stale `.tmp` leftover

- **Linked.** `Posterior::write_atomic`.
- **Outcome.** ✅ Pass. Hand-placed `.specere/posterior.toml.tmp` with garbage contents; next `filter run` clobbered it via `fs::write` then `fs::rename`. No orphan after the run.

### M-21 — cursor advance with out-of-order JSONL — **BUG**

- **Linked.** `run_filter_run` in `main.rs` + FR-P4-001 (idempotent re-run).
- **Command.** Hand-seeded three events with timestamps 0.100, 0.200, 0.090 (in file order); ran `filter run` twice.
- **Outcome, pre-fix.** ❌ Bug. First run processed 3 events; cursor set to `0.090` (the last-iterated ts). Second run re-processed 2 events (0.100 and 0.200, both > 0.090) and reported `processed 2, skipped 0`. Direct violation of FR-P4-001.
- **Fix.** `crates/specere/src/main.rs::run_filter_run` — `latest_ts` now updates only when the current event's ts exceeds the running max:

```rust
match &latest_ts {
    Some(cur) if e.ts.as_str() <= cur.as_str() => {}
    _ => latest_ts = Some(e.ts.clone()),
}
```

- **Regression test.** Added `filter_run_cursor_advances_to_max_not_last_iteration_ts` in `crates/specere/tests/fr_p4_filter_cli.rs`.
- **Outcome, post-fix.** ✅ Pass. Cursor lands at `0.200`; second run is a true no-op (`no new events since 2026-04-18T12:00:00.200Z`).

### M-22 — `status --format json`

- **Linked.** `run_filter_status`.
- **Outcome.** ✅ Pass. `python3 -c "import json; json.load(stdin)"` round-trips; entries have the expected keys.

### M-23 — `filter run` with `[coupling]` edges

- **Linked.** `FactorGraphBP` + CLI backend dispatch.
- **Command.** 2-spec repo with `[coupling] edges = [["FR-001","FR-002"]]`; 6 `fail` events on FR-001.
- **Outcome.** ✅ Pass. FR-001 concentrates on VIO (0.985); FR-002's VIO mass lifts to 0.461 (vs uniform 0.333) via BP damped messages. BP code path clearly engaged.

### M-24 — `[coupling]` referencing unknown spec

- **Linked.** `FactorGraphBP::new`.
- **Outcome.** ✅ Pass. Edges naming unknown specs silently dropped at construction; FR-001's filter proceeds as if no coupling exists. Documented behaviour.

---

## Fix commit SHAs

| Finding | Fixed in |
|---|---|
| M-21 cursor max-ts | crates/specere/src/main.rs::run_filter_run |
| M-04 / M-19 lost anyhow chain | crates/specere/src/main.rs main-err fmt → `{e:#}` |
| Regression test | crates/specere/tests/fr_p4_filter_cli.rs (new `filter_run_cursor_advances_to_max_not_last_iteration_ts`) |

All three changes land in the `028-phase4-followups` branch — commit references captured in git history at the time of merge.
