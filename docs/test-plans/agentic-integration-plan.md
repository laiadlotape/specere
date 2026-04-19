# SpecERE agentic-integration test plan

A human-walkable plan that exercises `specere` end-to-end under a **real Claude Code session**. Unlike `self-dogfood-guide.md` (which drives the CLI directly), this plan validates that specere runs transparently when an agent is actually doing work — that hooks fire on `/speckit-*` verbs, spans land in the event store, and the filter reflects the agent's actions in the posterior.

**Target binary under test.** `specere` ≥ v1.0.3.

**Duration.** ~20 minutes for the passive pass (session runs on its own, you just watch), ~40 minutes if you drive an actual feature through the full `/speckit-*` cycle.

**Who this is for.** Maintainers who want to catch broken hooks before release. Contributors validating that their new skill / hook wires up correctly. Anyone debugging why a `/speckit-*` verb didn't emit a span.

---

## Prerequisites

- Everything from `self-dogfood-guide.md` Prerequisites (git, cargo, uvx, python3, jq).
- **Claude Code CLI** installed locally. Verify with `claude --version`.
- `specere` ≥ v1.0.3 on `PATH` or via `$BIN`.
- A writable scratch directory.

---

## Setup — sandbox + install

```sh
export SANDBOX=$HOME/Projects/tmp/specere-agentic-$(date +%s)
git clone https://github.com/laiadlotape/specere "$SANDBOX"
cd "$SANDBOX"

# Strip harness state left by the project's own self-install.
rm -rf .specere .specify
rm -rf .claude/agents/specere-reviewer.md .claude/skills/specere-* .claude/skills/speckit-*
git checkout -- CLAUDE.md .gitignore 2>/dev/null

$BIN init
$BIN verify           # should print: No drift.
```

Verify the harness is wired correctly:

```sh
cat .specify/extensions.yml          # should contain 14 hooks (before_<verb> × 7, after_<verb> × 6, after_implement bespoke)
ls .claude/skills/specere-*          # should list: specere-adopt, specere-lint-ears, specere-observe-implement, specere-observe-step, specere-review-check, specere-review-drain
ls .claude/agents/                   # should include specere-reviewer.md
grep -c "specere:begin" CLAUDE.md    # should print: 1 (the rules block fence)
```

Populate sensor-map so the filter has something to reason about:

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

Cleanse the event store so the agent run starts from a known-empty state:

```sh
rm -f .specere/events.jsonl .specere/events.sqlite
```

---

## Part A — Passive observation pass (workflow runner)

In this pass you don't interact with Claude — you invoke the `specere-observe` workflow runner, which drives the full `/speckit-*` cycle end-to-end without human input. Every verb should emit a `before_<verb>` + `after_<verb>` span, plus the bespoke `after_implement` hook.

### A-01 — launch the passive workflow

In one terminal (with `$BIN` on PATH):

```sh
cd "$SANDBOX"
specify workflow run specere-observe \
  --input '{"feature_title": "agentic-smoke", "feature_branch": "999-agentic-smoke"}'
```

**Expected.** `specify workflow run` spawns a headless Claude session, walks `specify → clarify → plan → tasks → implement`, prints progress for each step, exits 0.

(If you don't have SpecKit's `specify` CLI available — e.g. `uvx` isn't installed — skip to Part B and drive verbs interactively.)

### A-02 — events landed for every verb

```sh
$BIN observe query --format table | head -30
```

**Expected.** At least 13 events (or more — the exact count depends on how many verbs the workflow ran). Each row shows `source = <verb>`, `signal = traces`. Verbs you should see at minimum: `specify`, `clarify`, `plan`, `tasks`, `implement`.

### A-03 — attr contract is honoured

```sh
$BIN observe query --format json | python3 -c "
import json,sys
d=json.load(sys.stdin)
by_source = {}
for e in d:
    by_source.setdefault(e['source'], []).append(e)
for source, events in sorted(by_source.items()):
    sample = events[0]
    print(f'{source:12}  {len(events):3} event(s)  attrs: {sorted(sample[\"attrs\"].keys())}')
"
```

**Expected.** Each unique `source` has one row. Every sample should show at least `gen_ai.system = claude-code`, `specere.workflow_step = <verb>`, `phase = before|after` in its attrs. Missing attrs = broken skill wiring.

### A-04 — feature_dir attribute is populated

```sh
$BIN observe query --format json | python3 -c "
import json,sys
d=json.load(sys.stdin)
with_fd = [e for e in d if e.get('feature_dir')]
print(f'events with feature_dir: {len(with_fd)}/{len(d)}')
if with_fd:
    print('sample:', with_fd[0]['feature_dir'])
"
```

**Expected.** Every event after `/speckit-specify` has set `feature_dir` = `specs/999-agentic-smoke` (or whatever branch-name mapping speckit assigned). Events from before the feature was created may have empty feature_dir.

### A-05 — advance the filter

```sh
$BIN filter run
$BIN filter status
```

**Expected.** The `filter run` output reports a sensible `processed`/`skipped` ratio. Most workflow spans will be `skipped` because they carry `event_kind=workflow_step` (or no `event_kind`), not `test_outcome` or `files_touched`. This is **correct behaviour** — the filter only advances on actual test results, not on workflow milestones.

The posterior may therefore be empty or show only specs touched by write events. If you want to see the filter advance, proceed to Part C.

---

## Part B — Interactive session

In this pass you open Claude Code interactively and run verbs by hand. Useful for debugging a specific hook.

### B-01 — open Claude Code in the sandbox

```sh
cd "$SANDBOX"
claude
```

Inside the Claude session, run:

```
/specere-reviewer  Check the repo is ready to specify a new feature
```

**Expected.** The `specere-reviewer` agent responds (from `.claude/agents/specere-reviewer.md`). This proves the agent surface is wired.

### B-02 — run `/speckit-specify` interactively

Inside the Claude session:

```
/speckit-specify  widget — a toy feature that returns HTTP 200
```

**Expected.** Claude produces `specs/NNN-widget/spec.md`. In a second terminal, check the event store:

```sh
$BIN observe query --source specify --format json | python3 -m json.tool | head -30
```

There should be at least one event with `source=specify`, `attrs.phase` in {before, after}, and a timestamp close to when the verb ran.

### B-03 — run the rest of the cycle

One at a time in Claude:

```
/speckit-clarify
/speckit-plan
/speckit-tasks
/speckit-implement
```

**Expected.** After each verb:

```sh
$BIN observe query --source <verb> | head
```

shows at least one new event. The `/speckit-implement` verb also triggers the bespoke `specere-observe-implement` skill (separate from the generic `specere-observe-step`), which fires on the `after_implement` hook.

### B-04 — drive test outcomes through the filter

To see the filter move, record synthetic test outcomes matching the specs you edited during `/speckit-implement`:

```sh
# E.g. if the feature touched crates/specere-filter/...
$BIN observe record --source cargo-test \
  --attr event_kind=test_outcome --attr spec_id=filter --attr outcome=pass
$BIN observe record --source cargo-test \
  --attr event_kind=test_outcome --attr spec_id=filter --attr outcome=pass

$BIN filter run
$BIN filter status
```

**Expected.** `filter` spec lean toward SAT (`p_sat > 0.7` after 2 passes).

### B-05 — confirm the `specere-observe-step` skill fires with the right attrs

A hook should emit a span with:

- `source` = the verb (e.g., `implement`)
- `attrs.event_kind = workflow_step`
- `attrs.phase` in {before, after}
- `attrs.gen_ai.system = claude-code`
- `attrs.specere.workflow_step = <verb>`

Spot-check:

```sh
$BIN observe query --source implement --format json | python3 -c "
import json,sys
d=json.load(sys.stdin)
if not d:
    print('FAIL: no events with source=implement — implement hook did not fire')
    sys.exit(1)
for e in d:
    attrs = e.get('attrs', {})
    missing = [k for k in ['event_kind','phase','gen_ai.system','specere.workflow_step'] if k not in attrs]
    if missing:
        print(f'FAIL: event at {e[\"ts\"]} missing attrs: {missing}')
    else:
        print(f'OK: {e[\"ts\"]}  phase={attrs[\"phase\"]}')
"
```

**Expected.** Every event prints `OK`. Any `FAIL` indicates a broken hook / skill wiring.

---

## Part C — Full feedback loop (calibrate → run → refine)

The belief surface is most useful when calibrated against the repo's own history. This part closes the loop.

### C-01 — calibrate from git

```sh
$BIN calibrate from-git --max-commits 200 > /tmp/coupling-proposal.toml
cat /tmp/coupling-proposal.toml
```

**Expected.** A `[coupling]` TOML snippet with edges proposed from the real commit history.

### C-02 — paste the coupling into sensor-map + re-run filter

```sh
# Append the `[coupling]` section to sensor-map (manually or with awk/sed).
awk '/^\[coupling\]/,/^$/' /tmp/coupling-proposal.toml >> .specere/sensor-map.toml
cat .specere/sensor-map.toml
$BIN filter run
$BIN filter status
```

**Expected.** With coupling wired, the filter dispatches to `FactorGraphBP` instead of `PerSpecHMM`. Fail events on one spec should now lift adjacent specs' VIO mass via BP messages.

### C-03 — observe a session then re-calibrate

After a few real `/speckit-*` cycles worth of commits, re-run calibrate:

```sh
$BIN calibrate from-git --max-commits 500
```

New coupling proposals may appear if the session produced co-modifying commits. This is the loop the SpecERE vision promises: agent activity → events → belief → calibration → better coupling model.

---

## Part D — Cleanup

```sh
cd "$SANDBOX"
for u in ears-linter otel-collector claude-code-deploy filter-state speckit; do
  $BIN remove "$u" --force
done
cd /
rm -rf "$SANDBOX"
```

---

## Checklist

| # | Part | Scenario | Pass |
|---|---|---|---|
| A-01 | A | `specify workflow run specere-observe` runs end-to-end | ☐ |
| A-02 | A | events landed for every workflow verb | ☐ |
| A-03 | A | attrs contract honoured (gen_ai, workflow_step, phase) | ☐ |
| A-04 | A | feature_dir attribute populated after `/speckit-specify` | ☐ |
| A-05 | A | filter run/status tolerates workflow-only events | ☐ |
| B-01 | B | `/specere-reviewer` agent responds inside Claude Code | ☐ |
| B-02 | B | `/speckit-specify` emits a span | ☐ |
| B-03 | B | clarify / plan / tasks / implement all emit spans | ☐ |
| B-04 | B | filter advances on synthetic test outcomes | ☐ |
| B-05 | B | specere-observe-step skill emits the right attrs | ☐ |
| C-01 | C | `calibrate from-git` proposes coupling | ☐ |
| C-02 | C | coupling pasted + filter dispatches to BP | ☐ |
| C-03 | C | post-session re-calibrate shows new co-modification | ☐ |
| D   | D | full uninstall + sandbox teardown | ☐ |

---

## Appendix A — Hook wiring reference

| Hook name | Registered in `extensions.yml` | Calls |
|---|---|---|
| `before_specify`, `after_specify` | auto | `specere-observe-step` skill |
| `before_clarify`, `after_clarify` | auto | `specere-observe-step` + `ears-linter` (before only) |
| `before_plan`, `after_plan` | auto | `specere-observe-step` |
| `before_tasks`, `after_tasks` | auto | `specere-observe-step` |
| `before_analyze`, `after_analyze` | auto | `specere-observe-step` |
| `before_checklist`, `after_checklist` | auto | `specere-observe-step` |
| `before_implement` | auto | `specere-observe-step` |
| `after_implement` | auto | **bespoke** `specere-observe-implement` (preserves FR-P1-005) |

Total: 14 hooks across 7 verbs. If any verb produces only one event (either before or after but not both), check `.specify/extensions.yml` for that verb's hook stanza.

## Appendix B — Debugging a missing hook

1. **Skill not found.** Check `.claude/skills/specere-observe-step/SKILL.md` exists.
2. **Hook registered but doesn't fire.** Check `.specify/extensions.yml` has the hook. Run `specere verify` — if it reports `drift` on `extensions.yml`, fix with `specere add claude-code-deploy --adopt-edits`.
3. **Fires but no event lands.** Check `specere observe record` is on `PATH` (the skill shells out to the binary). Try manually:
   ```sh
   $BIN observe record --source test --attr event_kind=workflow_step --attr specere.workflow_step=test --attr phase=before --attr gen_ai.system=claude-code
   $BIN observe query --source test | tail
   ```
4. **Event lands but missing attrs.** Inspect the skill file at `.claude/skills/specere-observe-step/SKILL.md`; its prompt tells Claude which attrs to emit.

## Appendix C — Tearing down mid-test

If the session hangs or produces unexpected state:

```sh
# Kill any rogue specere serve processes
pkill -f "specere serve" || true
# Reset the event store
rm -f .specere/events.jsonl .specere/events.sqlite .specere/posterior.toml
# Reset harness state
cd "$SANDBOX"
for u in ears-linter otel-collector claude-code-deploy filter-state speckit; do
  $BIN remove "$u" --force 2>/dev/null
done
rm -rf .specere .specify .claude/skills/specere-* .claude/skills/speckit-* .claude/agents/specere-reviewer.md
git checkout -- CLAUDE.md .gitignore 2>/dev/null
$BIN init
```

## Appendix D — Known gaps

- **Workflow spans don't advance the filter on their own.** By design — `event_kind=workflow_step` is an advisory signal (agent-is-doing-thing), not a test outcome. Only `test_outcome` and `files_touched` events feed the filter. This means Part A's filter output will look empty; Part B + C is where the filter actually moves.
- **No auto-calibration trigger yet.** Users must run `specere calibrate from-git` manually. A future phase may add a post-implement hook that re-calibrates automatically.
- **`specere serve` ingress via Claude Code hooks is not wired.** Hooks currently shell out to `specere observe record` (CLI), not post to `localhost:4318`. Switching the skills to HTTP-POST would unlock richer attrs at the cost of requiring a running serve. This is an intentional v1 simplification.
