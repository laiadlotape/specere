---
name: specere-observe-implement
description: Record a Repo-SLAM observation from a just-completed /speckit-implement run. Invoked by the after_implement hook in .specify/extensions.yml.
argument-hint: "[no args — reads $FEATURE_DIR from the hook env]"
user-invocable: false
disable-model-invocation: false
---

# specere-observe-implement

You are invoked from the `after_implement` hook registered in
`.specify/extensions.yml`. Your single responsibility is to **record the
observation** of the implement step so the downstream filter can consume it.

## Procedure

1. Read `$FEATURE_DIR/tasks.md` to count how many `[X]` entries flipped
   during the run (diff against the pre-hook snapshot).
2. Read `$FEATURE_DIR/plan.md` and `$FEATURE_DIR/spec.md` to extract the
   FR-NNN ids touched in this step.
3. Emit an OTel span named `specere.observe.implement` with attributes:
   - `gen_ai.system = "claude-code"`
   - `specere.workflow_step = "implement"`
   - `specere.feature_dir = $FEATURE_DIR`
   - `specere.tasks_flipped = <count>`
   - `specere.fr_ids = [...]`
   - `specere.duration_ms = <elapsed>`
4. Run `specere observe record --source=implement --feature-dir=$FEATURE_DIR`
   (stub until Phase 3 lands; print the intended payload for now).
5. Return control. Do **not** block the user.

## Invariants (from constitution)

- Do not modify files outside `.specere/`.
- Do not invoke other slash commands.
- Errors go to stderr, never to `spec.md` / `plan.md`.
