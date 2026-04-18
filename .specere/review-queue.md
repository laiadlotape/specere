# SpecERE review queue

> **Dogfeed surface.** Unmapped write surfaces, uninstrumented hook verbs, and
> OTel spans not covered by `.specere/sensor-map.toml` or `.specere/manifest.toml`
> land here. `specere review check` appends; `specere review drain` walks
> interactively (EXTEND / IGNORE / ALLOWLIST / ADJUDICATE) and logs decisions
> to `.specere/decisions.log`.
>
> The post-implement workflow gate (`specere-observe`) blocks on non-empty queue.
> Empty queue = harness coverage is complete for observed state.

## Open items

### [2026-04-18] `specify workflow run` recursive-claude subprocess leaks state on abort
- first_seen: 2026-04-18T00:00:00Z
- last_seen:  2026-04-18T00:00:00Z
- seen_count: 1
- surface: `specify workflow run specere-observe` spawns `claude -p "/speckit-specify …"` as a subprocess. If that subprocess is killed before completing, partial state is left behind: (a) a ghost feature branch with no commits, (b) a `specs/NNN-.../` dir containing only the unfilled template, (c) `.specify/feature.json` still pointing at the ghost dir, (d) an orphan `.specify/workflows/runs/<run-id>/` artifact.
- sample: on this exact scaffold run, branch `001-phase1-bugfix` + `specs/001-phase1-bugfix/spec.md` (128-line template) + orphan `1971c533/` run artifact + stale `feature.json`. Force-cleaned manually.
- suggested_action: **EXTEND** — the `speckit` wrapper unit's `preflight` should detect orphan feature dirs / `.specify/feature.json` mismatches and prompt for cleanup before the next `specify …` verb runs. The workflow runner itself is upstream-owned (IGNORE per 10-rule #10), but orphan detection is in-scope.
- rationale: this is a §1.1 motion-model signal — a write surface (`specify workflow run`'s subprocess spawn) produced side-effects that our sensor map did not cover. Constitution V mandates surfacing it.



## Closed items

_(migrated from `.specere/decisions.log` on drain.)_
