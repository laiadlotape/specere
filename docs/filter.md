# `specere filter` — per-spec belief engine

The filter subcommand turns the event store into a live per-spec posterior — one 3-state distribution `{Unk, Sat, Vio}` per requirement, written to `.specere/posterior.toml`. This is the SpecERE surface that every agent workflow ultimately feeds.

## TL;DR

```sh
# 1. init scaffolds sensor-map.toml + posterior.toml + events.sqlite.
specere init

# 2. Hand-author (or `specere-adopt`) a [specs] section with the
#    requirements the filter should track.
$EDITOR .specere/sensor-map.toml

# 3. Agents / hooks call `specere observe record` to emit events.
specere observe record --source test_runner \
  --attr event_kind=test_outcome --attr spec_id=FR-001 --attr outcome=pass

# 4. Advance the posterior.
specere filter run

# 5. Read it.
specere filter status
```

## `.specere/sensor-map.toml`

The filter consumes two sections from the sensor-map. Both live at top level.

### `[specs]` (required)

```toml
[specs]
"FR-001" = { support = ["src/auth/login.rs"] }
"FR-002" = { support = ["src/auth/login.rs", "src/auth/token.rs"] }
"FR-003" = { support = ["src/billing/charge.rs"] }
```

- **Key** — the requirement ID. Any non-empty string. Convention: `FR-NNN` or `<module>_<behaviour>`.
- **Value.support** — a list of file paths that, when edited, advance this spec's belief via the motion step.

Without a `[specs]` section `specere filter run` exits with:

> `[specs] section empty or missing in sensor-map.toml — add entries like "FR-001" = { support = ["src/a.rs"] }`

### `[coupling]` (optional)

```toml
[coupling]
edges = [
  ["FR-002", "FR-003"],  # FR-002 and FR-003 are coupled — a VIO on FR-002
  ["FR-001", "FR-002"],  # biases FR-003's belief via loopy BP.
]
```

Each edge is a **directed** pair: `[src, dst]` means VIO evidence on `src` propagates toward `dst`. The graph **must be a DAG** — loops are rejected with an actionable error listing the cycle chain.

When `[coupling]` is present, `filter run` dispatches to `FactorGraphBP` instead of the default `PerSpecHMM`. When it's absent or empty, BP is skipped entirely and the filter runs as independent per-spec HMMs.

Cycles intentionally route to a separate `RBPF` path, but that path is not wired into the CLI in v0.5.0 — the coupling loader rejects them so you know before you run.

## Event-attr contract

Events in `.specere/events.jsonl` drive the filter. Hook authors populate three OTel attributes:

| attr | values | meaning |
|---|---|---|
| `event_kind` | `"test_outcome"` or `"files_touched"` | dispatch discriminator |
| `spec_id` | the requirement ID | required for `test_outcome` |
| `outcome` | `"pass"` or `"fail"` | required for `test_outcome` |
| `paths` | comma-separated file paths | required for `files_touched` |

Events with neither `event_kind` in the set above are silently **skipped** and counted in the `skipped` column of `filter run`'s summary — malformed events don't crash the run.

### Examples

Record a test outcome:

```sh
specere observe record \
  --source cargo-test \
  --attr event_kind=test_outcome \
  --attr spec_id=FR-003 \
  --attr outcome=fail
```

Record an agent write (motion step):

```sh
specere observe record \
  --source claude-code \
  --attr event_kind=files_touched \
  --attr paths="src/auth/login.rs,src/auth/token.rs"
```

### Unknown outcomes

If `outcome` is anything other than `"pass"` or `"fail"`, the default test sensor returns a uniform log-likelihood — the posterior is unchanged (Bayes with a flat emission preserves the prior). Useful as a "no-op" placeholder for hooks that haven't been fully wired.

## `specere filter run`

Consumes every event past the last-processed cursor, advances the filter, writes `.specere/posterior.toml` atomically.

```sh
specere filter run                          # default paths
specere filter run --sensor-map /path/to/map.toml
specere filter run --posterior /tmp/test-posterior.toml
```

- **Cursor semantics.** The posterior stores a `cursor` field with the **max** event timestamp processed. Out-of-order JSONL appends are handled correctly — a late-dated event appearing after newer events does not roll the cursor back.
- **Idempotency.** A second `run` with no new events is a byte-level no-op on the file.
- **Concurrency.** An advisory exclusive file lock at `.specere/filter.lock` serialises concurrent `filter run` invocations. The second run blocks until the first finishes, then processes only events the first hasn't yet.
- **Atomic writes.** The posterior is written to `.specere/posterior.toml.tmp` then renamed into place. No partial-file corruption on crash.

## `specere filter status`

Reads `.specere/posterior.toml` and prints the per-spec belief table.

```sh
specere filter status                          # default: entropy,desc
specere filter status --sort p_vio,desc        # highest-VIO specs first
specere filter status --sort spec_id,asc       # alphabetical
specere filter status --format json            # machine-parseable
```

Valid `--sort` fields: `entropy`, `p_sat`, `p_vio`, `p_unk`, `spec_id`. Directions: `asc`, `desc`. Unknown values error with an enumeration of what's valid.

Valid `--format` values: `table` (default, human-readable), `json` (an array of `{spec_id, p_unk, p_sat, p_vio, entropy, last_updated}` objects).

### Empty states

| state | message |
|---|---|
| `.specere/posterior.toml` does not exist | `no posterior yet — run \`specere filter run\` first` |
| `posterior.toml` exists but has no entries | `posterior has no entries — no events processed yet. Add [specs] + seed events, then specere filter run.` |

## Sensor calibration

The default emission model (`DefaultTestSensor`) matches the ReSearch prototype:

- `P(pass | SAT) = 0.92`
- `P(fail | VIO) = 0.90`
- `P(pass | UNK) = 0.55`

The motion matrices (`t_good`, `t_bad`, `t_leak`) are likewise ported verbatim from `ReSearch/prototype/mini_specs/world.py`. Gate-A parity with the Python prototype is verified in `crates/specere-filter/tests/gate_a_parity.rs`; observed per-cell divergence is **0** (bit-identical).

Phase 5 (planned) will learn per-spec motion matrices from git history via `specere calibrate from-git`. Until then, the prototype defaults are in effect for every install.

## Troubleshooting

| symptom | likely cause |
|---|---|
| `sensor-map not found` | Run `specere init` or create `.specere/sensor-map.toml` by hand. |
| `[specs] section empty or missing` | Add a `[specs]` block per the schema above. |
| `coupling graph has a cycle` | Break the cycle. If you *need* coupling with loops, wait for RBPF CLI (post v0.5.0). |
| `unknown spec id` in a `skipped` event | The event's `spec_id` isn't in `[specs]` — either typo'd or you removed the spec after the event landed. Events are kept for audit; the filter skips them. |
| `posterior has no entries` after `filter run` | No events with known `event_kind` + `spec_id` matched your specs. Check `specere observe query` to confirm events are arriving. |
| Very high VIO after a single `fail` | Expected — the default sensor has `P(fail | VIO) = 0.90`. Either your tests are very reliable (good!) or you want to recalibrate (Phase 5). |

## Further reading

- `docs/specere_v1.md §5.P4` — FR-P4-001 through FR-P4-006, the filter-engine requirements.
- `docs/phase4-followups-execution-plan.md` — the execution log for the Phase 4 follow-ups PR (Gate-A parity + throughput).
- `docs/phase4-manual-test-report.md` — 24-scenario manual-test traceability; every edge case called out above was hit by hand at least once.
- `crates/specere-filter/src/` — the filter engine source. Start at `lib.rs` for the module map.
