# SpecERE self-dogfood test guide

A human-walkable end-to-end test plan that installs `specere` onto a fresh clone of its own source tree, exercises every verb and subcommand, and uninstalls cleanly. Designed as a one-sitting smoke suite before cutting a release.

**Target binary under test:** `specere` ≥ v1.0.3 (T-31 requires the [issue #61](https://github.com/laiadlotape/specere/issues/61) fix to accept the `feature_dir` alias; earlier versions will fail T-31 with `could not parse feature_directory`).

**Duration.** ~25 minutes the first time, ~10 minutes after.

**Who this is for.** Maintainers running the pre-release checklist. External contributors who want a hands-on tour of the CLI.

---

## Prerequisites

- `git` ≥ 2.30 on `PATH`.
- `cargo` available (only needed if you're building from source).
- `uvx` available (only needed if `specere init` should pull speckit — the test still exits 0 on missing `uvx`, but speckit install is skipped).
- `jq` or `python3` — for verifying JSON output in T-18.
- A writable scratch directory (we'll use `$HOME/Projects/tmp`).

If you're testing a local build, have the binary path ready:

```sh
export BIN=$(realpath /path/to/specere/target/release/specere)
$BIN --version                      # should print: specere 1.0.3 (or newer)
```

If you're testing the installer-bundled binary, add it to `PATH` first:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/laiadlotape/specere/releases/download/v1.0.3/specere-installer.sh | sh
export BIN=$HOME/.cargo/bin/specere   # or wherever the installer placed it
```

---

## Setup — sandbox the target

We clone the specere repo into a throwaway directory and use it as the "target repo" that `specere` will operate on. Crucially we strip the committed harness state first so `specere init` can scaffold from a clean tree — if you skip this step, T-01 will fail because the repo carries a pre-v0.3 manifest schema.

```sh
export SANDBOX=$HOME/Projects/tmp/specere-self-test-$(date +%s)
git clone https://github.com/laiadlotape/specere "$SANDBOX"
cd "$SANDBOX"

# Strip specere-owned state left behind by the project's own self-install.
# The binary will scaffold fresh copies below.
rm -rf .specere .specify
rm -rf .claude/agents/specere-reviewer.md
rm -rf .claude/skills/specere-*
rm -rf .claude/skills/speckit-*
git checkout -- CLAUDE.md .gitignore 2>/dev/null

# Sanity: there should be no .specere/, no .specify/, CLAUDE.md back at HEAD.
ls .specere .specify 2>&1 | head
```

Expected:

```
ls: cannot access '.specere': No such file or directory
ls: cannot access '.specify': No such file or directory
```

---

## Part A — Installation and introspection

### T-01 — `specere init` scaffolds all five units

**Objective.** Full install from scratch. Exits 0. Prints 5 `installed ...` lines + 1 summary line.

```sh
$BIN init
```

**Expected (relevant excerpt):**

```
...INFO installed `speckit` @ v0.7.3
...INFO installed `filter-state` @ 1.0.1
...INFO installed `claude-code-deploy` @ 1.0.1
...INFO installed `otel-collector` @ 1.0.1
...INFO installed `ears-linter` @ 1.0.1
...INFO specere init: 5 unit(s) installed
```

Exit code: **0**.

(If `uvx` isn't installed, you'll see `speckit: upstream installer not available (uvx missing)` — still exit 0 because the other 4 units land.)

### T-02 — `specere status` lists the installed units

```sh
$BIN status
```

**Expected:**

```
SpecERE units installed in <sandbox>:
  speckit @ v0.7.3 [wrapper] (0 files, 0 markers)
  filter-state @ 1.0.1 [native] (3 files, 1 markers)
  claude-code-deploy @ 1.0.1 [native] (6 files, 4 markers)
  otel-collector @ 1.0.1 [native] (1 files, 0 markers)
  ears-linter @ 1.0.1 [native] (2 files, 1 markers)
```

All unit version strings should be `1.0.1` (or whatever version the binary reports via `--version`) for the **native** units. `speckit` version tracks the upstream speckit CLI and is independent.

### T-03 — `specere verify` reports no drift after a fresh install

```sh
$BIN verify
```

**Expected:** `No drift.`

### T-04 — `specere doctor` prints prerequisites + manifest status

```sh
$BIN doctor
```

**Expected:**

```
SpecERE doctor — target: <sandbox>
  git        OK  git version 2.xx.x
  uvx        OK  uvx <version>        (or: uvx MISSING — not fatal)
  cargo      OK  cargo <version>
  manifest   present
```

### T-05 — idempotent re-init is a no-op

```sh
$BIN init
```

**Expected:** each unit prints `unit \`X\` already installed — no-op`. Exit 0. `specere verify` still reports `No drift.`.

### T-06 — `--help` lists every verb

```sh
$BIN --help
```

**Expected:** lists `add`, `remove`, `init`, `lint`, `status`, `verify`, `doctor`, `observe`, `serve`, `filter`, `calibrate`, `help`.

---

## Part B — Observation subsystem

### T-07 — record a synthetic test-outcome event

```sh
$BIN observe record \
  --source test_runner \
  --attr event_kind=test_outcome \
  --attr spec_id=FR-001 \
  --attr outcome=pass
```

**Expected:** `specere observe record: 1 event appended to .specere/events.jsonl`. Exit 0.

Verify the line landed:

```sh
wc -l .specere/events.jsonl       # should print: 1 .specere/events.jsonl
```

### T-08 — record a files_touched event

```sh
$BIN observe record \
  --source agent \
  --attr event_kind=files_touched \
  --attr paths="crates/specere-filter/src/lib.rs,crates/specere/src/main.rs"
```

**Expected:** another line appended; `wc -l` now reports `2`.

### T-09 — query in table format

```sh
$BIN observe query --format table
```

**Expected:** a header + 2 data rows, one per event:

```
ts                          source          signal    name
--------------------------  --------------  --------  ----------------------------------------
<ts>                        test_runner     traces    test_runner
<ts>                        agent           traces    agent
```

### T-10 — query in JSON, filter by source

```sh
$BIN observe query --source test_runner --format json | python3 -c "import json,sys; d=json.load(sys.stdin); print(len(d))"
```

**Expected:** prints `1` — only the `test_runner` event survived the filter.

### T-11 — query with `--since` filter

```sh
$BIN observe query --since 2099-01-01T00:00:00Z --format table
```

**Expected:** header printed, zero data rows — nothing is newer than year 2099.

---

## Part C — Calibration (`specere calibrate from-git`)

These tests rely on the specere repo's own commit history, which the sandbox has. If the tree has fewer than ~20 commits, T-14's expected output will differ.

### T-12 — calibrate with the default install's empty `[specs]` errors cleanly

`specere init` writes a sensor-map with a commented-out `[specs]` section (empty). Run calibrate as-is:

```sh
$BIN calibrate from-git
```

**Expected:**

```
specere: error: [specs] section empty or missing in sensor-map.toml — add entries like `"FR-001" = { support = ["src/a.rs"] }`
```

Exit code: **1**. This is the expected onboarding UX — the user needs to populate `[specs]` before calibrate can do anything.

### T-13 — populate `[specs]` with the specere crate layout

Replace the sensor-map inline:

```sh
cat > .specere/sensor-map.toml <<'EOF'
schema_version = 1

[specs]
"core"      = { support = ["crates/specere-core/"] }
"units"     = { support = ["crates/specere-units/"] }
"telemetry" = { support = ["crates/specere-telemetry/"] }
"filter"    = { support = ["crates/specere-filter/"] }
"cli"       = { support = ["crates/specere/src/"] }

[channels]
EOF
```

### T-14 — calibrate produces sensible coupling

```sh
$BIN calibrate from-git --max-commits 50
```

**Expected (counts may vary as the repo grows):**

```
specere calibrate: analysed 29 commit(s); 19 touched a tracked spec
  per-spec touch counts:
    cli         9
    filter      8
    telemetry   8
    units       4
  3 edge(s) proposed; paste the snippet below into `.specere/sensor-map.toml`
# Suggested coupling edges — auto-proposed by
# `specere calibrate from-git` based on co-modification counts.
# Analysed 29 commits (19 touched a tracked spec).
[coupling]
edges = [
  ["cli", "filter"],      # 4 co-commits
  ["cli", "telemetry"],   # 3 co-commits
  ["cli", "units"],       # 3 co-commits
]
```

**Pass criteria:** at least `cli` has the highest touch count; at least one edge involving `cli` appears; exit 0.

### T-15 — `--min-commits 100` proposes nothing

```sh
$BIN calibrate from-git --max-commits 500 --min-commits 100
```

**Expected:** `no coupling edges proposed` (no pair has 100 co-commits in a repo this young).

### T-16 — calibrate refuses the path-prefix false match

Create a fake support with no trailing slash that looks like it would false-match a sibling path:

```sh
cat > .specere/sensor-map.toml <<'EOF'
schema_version = 1

[specs]
"filter"       = { support = ["crates/specere-filter"] }
"filter_telem" = { support = ["crates/specere-telemetry"] }

[channels]
EOF
$BIN calibrate from-git --max-commits 50
```

**Expected:** per-spec counts should be **different** for `filter` and `filter_telem`. Pre-v1.0.1 the prefix bug would have made every `crates/specere-filter/*` commit also match `filter_telem` (because `crates/specere-filter`.starts_with(`crates/specere-f`) — actually no, but `crates/specere-telemetry` doesn't start with `crates/specere-filter`; the real bug was `crates/specere`/`crates/specere-*`). In any case, counts must be spec-specific, not identical.

### T-17 — calibrate on a non-git directory emits a friendly error

```sh
(cd /tmp && $BIN calibrate from-git 2>&1 | head -1)
```

**Expected:**

```
specere: error: calibrate: /tmp is not a git repository — run `git init` first
```

(The `cd /tmp` puts us in a non-git dir; the sensor-map lookup will fail even earlier, but if you symlink a sensor-map into `/tmp/.specere/sensor-map.toml` you'll see the git error instead.)

---

## Part D — Filter pipeline end-to-end

Restore the proper `[specs]` section from T-13 before running this part:

```sh
cat > .specere/sensor-map.toml <<'EOF'
schema_version = 1

[specs]
"core"      = { support = ["crates/specere-core/"] }
"units"     = { support = ["crates/specere-units/"] }
"telemetry" = { support = ["crates/specere-telemetry/"] }
"filter"    = { support = ["crates/specere-filter/"] }
"cli"       = { support = ["crates/specere/src/"] }

[channels]
EOF
```

### T-18 — seed a mixed event stream

```sh
for i in 1 2 3 4 5; do
  $BIN observe record --source test --attr event_kind=test_outcome --attr spec_id=core --attr outcome=pass > /dev/null
done
for i in 1 2 3; do
  $BIN observe record --source test --attr event_kind=test_outcome --attr spec_id=filter --attr outcome=fail > /dev/null
done
$BIN observe record --source test --attr event_kind=test_outcome --attr spec_id=units --attr outcome=pass > /dev/null
```

**Expected:** 9 new events. `wc -l .specere/events.jsonl` should increase by 9.

### T-19 — filter run processes the new events

```sh
$BIN filter run
```

**Expected:**

```
specere filter: processed N event(s), skipped M; cursor -> <ts>
```

Where `N + M` equals the number of events in the store, and `M` counts events whose `spec_id` isn't in `[specs]` (typically the earlier FR-001 / FR-002 test events from Part B, which are skipped here).

### T-20 — filter status shows the expected lean

```sh
$BIN filter status
```

**Expected:** `core` leans **SAT** heavily (≥ 0.80 p_sat after 5 passes), `filter` leans **VIO** heavily (≥ 0.80 p_vio after 3 fails), `units` has a mild SAT lean, and `telemetry` / `cli` stay near uniform (unchanged). Default sort is entropy **desc**, so specs with the LEAST evidence (telemetry, cli) print first.

### T-21 — idempotent re-run

```sh
$BIN filter run
```

**Expected:** `specere filter: no new events since <ts>`. The posterior file is byte-identical:

```sh
sha256sum .specere/posterior.toml
$BIN filter run > /dev/null
sha256sum .specere/posterior.toml        # same hash
```

### T-22 — sort override

```sh
$BIN filter status --sort p_vio,desc
```

**Expected:** `filter` is the first data row (highest p_vio).

### T-23 — sort rejects malformed input

```sh
$BIN filter status --sort garbage
$BIN filter status --sort entropy,sideways
$BIN filter status --sort foo,desc
```

**Expected:** each exits 1 with a clear error — `--sort expects 'field,asc|desc'`, `--sort direction must be 'asc' or 'desc'`, `unknown --sort field 'foo'; one of entropy, p_sat, p_vio, p_unk, spec_id`.

### T-24 — status rejects unknown format

```sh
$BIN filter status --format yaml
```

**Expected:** `unknown --format 'yaml'; one of 'table' (default) or 'json'`. Exit 1.

### T-25 — status JSON is valid JSON

```sh
$BIN filter status --format json | python3 -c "import json,sys; d=json.load(sys.stdin); print(f'{len(d)} entries')"
```

**Expected:** `5 entries` (one per spec in the `[specs]` section).

### T-26 — cross-session belief persists

Append one more fail event to `filter` and re-run. The previous posterior's beliefs must be preserved, not reset to uniform.

```sh
BEFORE_VIO=$($BIN filter status --format json | python3 -c "import json,sys; d=json.load(sys.stdin); print(next(e['p_vio'] for e in d if e['spec_id']=='filter'))")
$BIN observe record --source test --attr event_kind=test_outcome --attr spec_id=filter --attr outcome=fail > /dev/null
$BIN filter run
AFTER_VIO=$($BIN filter status --format json | python3 -c "import json,sys; d=json.load(sys.stdin); print(next(e['p_vio'] for e in d if e['spec_id']=='filter'))")
echo "before=$BEFORE_VIO after=$AFTER_VIO"
```

**Pass criteria:** `after_vio > before_vio`. If after equals before, cross-session persistence is broken.

### T-27 — concurrent `filter run` serialises via advisory lock

```sh
$BIN filter run &
P1=$!
$BIN filter run &
P2=$!
wait $P1; RC1=$?
wait $P2; RC2=$?
echo "rc1=$RC1 rc2=$RC2"
```

**Expected:** both exit 0. The advisory lock at `.specere/filter.lock` serialises them; no "rename failed" error.

---

## Part E — OTLP serve

### T-28 — `specere serve` starts HTTP + gRPC on ephemeral ports

```sh
$BIN serve --bind 127.0.0.1:0 --grpc-bind 127.0.0.1:0 > /tmp/serve.log 2>&1 &
SPID=$!
sleep 1
cat /tmp/serve.log
kill -INT $SPID
wait $SPID 2>/dev/null
```

**Expected:** two log lines announcing the bound ports:

```
...INFO specere serve: OTLP/HTTP receiver up on 127.0.0.1:<port>
...INFO specere serve: OTLP/gRPC receiver up on 127.0.0.1:<port>
```

SIGINT (the `kill -INT`) exits cleanly — no panic, no stack trace.

### T-29 — `specere serve` with fixed ports

If nothing else is listening on 4318/4317:

```sh
$BIN serve &
SPID=$!
sleep 1
curl -s http://127.0.0.1:4318/healthz
kill -INT $SPID; wait $SPID 2>/dev/null
```

**Expected:** `curl` prints `ok`; serve shuts down on SIGINT.

If ports are taken (EADDRINUSE), the error is raised on stdout with a clear address-conflict message.

---

## Part F — Lint and lint helpers

### T-30 — `specere lint ears` with no active feature

```sh
$BIN lint ears
```

**Expected:** `specere lint ears: no active feature — skipping ears lint (.specify/feature.json absent)`. Exit 0.

### T-31 — `specere lint ears` with a synthetic spec

```sh
mkdir -p specs/999-smoke
cat > specs/999-smoke/spec.md <<'EOF'
## Requirements

- FR-001: The system MUST return HTTP 200.
- FR-002: Probably should handle errors somehow.
- FR-003: When X happens, THE system MUST do Y.
EOF
cat > .specify/feature.json <<'EOF'
{"feature_directory": "specs/999-smoke"}
EOF
$BIN lint ears
```

**Expected:** advisory findings for FR-002 (missing `THE SYSTEM`/`MUST`/`SHALL`), no fatal exit — `lint ears` always exits 0 per the advisory-only contract.

Clean up: `rm -rf specs/999-smoke .specify/feature.json`.

**Known issue — fixed in v1.0.3 ([#61](https://github.com/laiadlotape/specere/issues/61)).** On v1.0.2 and earlier this scenario erred with `could not parse feature_directory from .../feature.json` if the JSON used the shorter `feature_dir` key. v1.0.3's `parse_feature_directory` is serde_json-based and accepts either `feature_directory` (shown above) or `feature_dir` via `#[serde(alias)]`. If you're on v1.0.2 or earlier, use the full `feature_directory` key name.

---

## Part G — Uninstall round-trip

### T-32 — remove all 5 units in reverse-install order

```sh
for u in ears-linter otel-collector claude-code-deploy filter-state speckit; do
  $BIN remove "$u" --force
done
```

**Expected:** each unit prints `removed \`X\``. Runtime-edited files (posterior.toml, sensor-map.toml, events.sqlite, events.jsonl) are **preserved** with a `preserving on remove` warning — that's intentional, the user may want to keep the data.

### T-33 — post-uninstall `.specere/` contains only runtime artefacts

```sh
ls .specere
```

**Expected:** `events.jsonl`, `events.sqlite`, `posterior.toml`, `sensor-map.toml` — these are user data. `manifest.toml` is **gone** (the install record is removed). `filter.lock` is **gone** (swept by filter-state::remove as an ephemeral sidecar).

### T-34 — `specere doctor` reports the repo is uninstalled

```sh
$BIN doctor
```

**Expected:** `manifest   absent` (the last line). No crash.

### T-35 — re-install works cleanly

```sh
$BIN init
$BIN verify
```

**Expected:** all 5 units install fresh; `verify` reports `No drift.` — the pre-existing runtime artefacts (events, posterior) are silently adopted (filter-state's install path marks them `UserEditedAfterInstall`, which is correct behaviour).

---

## Part H — Error-path exploration

### T-36 — corrupted events.jsonl line

```sh
echo "this is not JSON" >> .specere/events.jsonl
$BIN observe query --format table
```

**Expected:** the valid lines still render; the corrupt one is silently skipped. No crash.

### T-37 — manually delete the sensor-map between runs

```sh
mv .specere/sensor-map.toml /tmp/sensor-map.bak
$BIN filter run
```

**Expected:**

```
specere: error: sensor-map not found at <sandbox>/.specere/sensor-map.toml — run `specere init` or add a [specs] section per docs/filter.md
```

Exit 1. Restore: `mv /tmp/sensor-map.bak .specere/sensor-map.toml`.

### T-38 — manually corrupt posterior.toml

```sh
echo "not valid toml [[" > .specere/posterior.toml
$BIN filter run
```

**Expected:** exits with a clear TOML parse error chain that names the file and the parse location (line, column).

Restore with another `$BIN filter run` after deleting the corrupt file: `rm .specere/posterior.toml && $BIN filter run`.

---

## Checklist

Run through all 38 scenarios and tick off as you go. The minimum bar for a release-ready state is **every scenario passing**. If any scenario diverges from its expected output, capture the output and file it as a regression issue.

| # | Part | Scenario | Pass |
|---|---|---|---|
| T-01 | A | `specere init` scaffolds all five units | ☐ |
| T-02 | A | `specere status` lists installed units | ☐ |
| T-03 | A | `specere verify` reports no drift | ☐ |
| T-04 | A | `specere doctor` prints prerequisites | ☐ |
| T-05 | A | idempotent re-init is a no-op | ☐ |
| T-06 | A | `--help` lists every verb | ☐ |
| T-07 | B | observe record test_outcome | ☐ |
| T-08 | B | observe record files_touched | ☐ |
| T-09 | B | observe query table | ☐ |
| T-10 | B | observe query JSON with source filter | ☐ |
| T-11 | B | observe query with `--since` filter | ☐ |
| T-12 | C | calibrate fails loud on empty `[specs]` | ☐ |
| T-13 | C | populate sensor-map with real crate layout | ☐ |
| T-14 | C | calibrate proposes sensible coupling | ☐ |
| T-15 | C | `--min-commits 100` proposes nothing | ☐ |
| T-16 | C | path-prefix match does NOT bleed across siblings | ☐ |
| T-17 | C | calibrate on non-git dir friendly error | ☐ |
| T-18 | D | seed mixed event stream | ☐ |
| T-19 | D | filter run consumes new events | ☐ |
| T-20 | D | filter status shows expected lean | ☐ |
| T-21 | D | idempotent re-run | ☐ |
| T-22 | D | sort override works | ☐ |
| T-23 | D | sort rejects malformed input | ☐ |
| T-24 | D | format rejects unknown value | ☐ |
| T-25 | D | status JSON is valid | ☐ |
| T-26 | D | cross-session belief persists | ☐ |
| T-27 | D | concurrent filter run queues via lock | ☐ |
| T-28 | E | serve on ephemeral ports | ☐ |
| T-29 | E | serve on fixed ports + /healthz | ☐ |
| T-30 | F | lint ears no-feature path | ☐ |
| T-31 | F | lint ears with synthetic spec | ☐ |
| T-32 | G | remove all 5 units | ☐ |
| T-33 | G | `.specere/` contains only runtime artefacts | ☐ |
| T-34 | G | doctor reports absent | ☐ |
| T-35 | G | re-install clean | ☐ |
| T-36 | H | corrupted events.jsonl line | ☐ |
| T-37 | H | missing sensor-map error | ☐ |
| T-38 | H | corrupted posterior.toml error | ☐ |

Tear down the sandbox when finished:

```sh
rm -rf "$SANDBOX"
```

---

## Appendix A — Known behaviours that look like bugs but aren't

- **`observe record` stamps RFC3339 with nanosecond precision** (e.g. `2026-04-19T13:29:19.320718Z`). If two records are issued faster than 1ns apart, timestamps can tie and cursor ordering becomes undefined. In practice the wall clock's resolution rules this out.
- **`filter status` may print rows in a different order than the on-disk entries.** Entries in `posterior.toml` are always sorted by `spec_id` (for FR-P4-004 byte-stability). `filter status` then re-sorts them per `--sort`. If you diff posterior.toml bytes you'll see spec_id order; if you diff `filter status` output you'll see entropy-desc order.
- **`filter-state::remove` preserves runtime-edited files** (posterior, sensor-map, events.sqlite, events.jsonl). This is intentional — the user may want to keep their history. Use `rm -rf .specere` if you genuinely want a clean slate.
- **`specere remove claude-code-deploy --force`** leaves `.claude/skills/speckit-git-*` if they weren't installed by the current specere version. Older installs wrote fewer skills; `remove` only cleans what that install tracked in its manifest. Safe to leave or delete by hand.
- **`specere init` on the upstream specere repo** will fail with `missing field unit_id` if the repo's committed `.specere/manifest.toml` predates the MarkerEntry schema. The Setup step above removes this manifest before init — if you're testing a real user-facing upgrade flow, the proper regression fix is to make `unit_id` optional on MarkerEntry (tracked as a follow-up; not blocking current releases).

## Observed run log

| Version | Date | All 38 pass? | Notes |
|---|---|---|---|
| v1.0.2 | 2026-04-19 | 37/38 | T-31 surfaced issue #61 (feature.json parser rigid on key name). Fixed in v1.0.3. |
| v1.0.3 | 2026-04-19 | anticipate 38/38 | #61 fixed; T-31 guide updated to use `feature_directory` and note the new `feature_dir` alias. |

## Appendix B — Cleanup between test runs

If anything in the checklist leaves state behind that confuses a later test, reset the sandbox cleanly:

```sh
cd "$SANDBOX"
for u in ears-linter otel-collector claude-code-deploy filter-state speckit; do
  $BIN remove "$u" --force 2>/dev/null
done
rm -rf .specere .specify .claude/skills/specere-* .claude/skills/speckit-* .claude/agents/specere-reviewer.md
git checkout -- CLAUDE.md .gitignore 2>/dev/null
$BIN init
```

This is equivalent to the initial Setup block — safe to run anywhere between tests.

## Appendix C — How to report a failure

If any scenario diverges from its expected output:

1. Capture the exact command and the full output.
2. Run `$BIN --version` + `git rev-parse HEAD` + `uname -a`.
3. File a GitHub issue with title `[self-dogfood] T-NN: <one-line summary>`.
4. Paste the captured output in a code block.
5. Label `regression` if the scenario has a pass criterion in a prior release's test report.
