---
name: specere-observe-step
description: Generic workflow-step observer invoked from every before_<verb> / after_<verb> hook registered by claude-code-deploy. Records a SpecERE telemetry event per hook firing so the filter (Phase 4) has a per-verb sensor stream. Issue #31 / FR-P3-002 / FR-P3-006.
argument-hint: "[no args — reads verb + phase + feature_dir from the hook's prompt]"
user-invocable: false
disable-model-invocation: false
---

# specere-observe-step

You are invoked from a SpecKit workflow hook registered in `.specify/extensions.yml` under `extension: specere`, `command: specere.observe.step`. The hook's `prompt:` field carries the exact shell invocation you should run — it looks like:

```
Record <phase> <verb> span: specere observe record --source=<verb> --attr phase=<phase> --feature-dir=$FEATURE_DIR
```

## Procedure

1. **Extract** the verb (`specify|clarify|plan|tasks|analyze|checklist|implement`) and phase (`before|after`) from the hook's prompt. `$FEATURE_DIR` comes from the SpecKit hook environment.
2. **Run**:
   ```sh
   specere observe record \
     --source=<verb> \
     --attr phase=<phase> \
     --attr gen_ai.system=claude-code \
     --attr specere.workflow_step=<verb> \
     --feature-dir="$FEATURE_DIR"
   ```
3. **Return control** to the SpecKit flow immediately. Do not block, do not prompt, do not comment.

## Invariants

- **Advisory only.** The hook is registered with `optional: true`; you never block the `/speckit-*` command that triggered you.
- **No filesystem writes** outside what `specere observe record` does (the event store in `.specere/`).
- **No errors surface.** If `specere` isn't on `$PATH`, silently skip — the workflow must continue. (Real installations always have `specere` because this skill ships with `claude-code-deploy`.)
- **Idempotent at the span level.** Re-running the same hook twice writes two events — that's fine; the filter's time-series expects exactly this.

## Why this skill exists

Before Phase 3, `claude-code-deploy` only registered one hook (`after_implement`) with a bespoke command (`specere.observe.implement`). That was a scaffolding placeholder. Phase 3 expands to 13 more hooks (`before/after` × `specify|clarify|plan|tasks|analyze|checklist` + `before_implement`) so every workflow step produces ≥ 1 gen_ai.* span — the FR-P3-002 / FR-P3-006 acceptance shape.

The single generic skill + 13 hooks keeps the wire format tiny; adding a new verb in the future means one new hook entry, not a new skill.
