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

_No items. Queue is empty — harness is up-to-date against observed state._



## Closed items

### [2026-04-18 → drained 2026-04-18] `specify workflow run` recursive-claude subprocess leaks state on abort
- closed_at: 2026-04-18T00:00:00Z
- decision: **EXTEND**
- rationale: file as speckit::preflight orphan detector; target v0.3.0 (Phase 2 native-units completion). The workflow runner itself is upstream-owned (IGNORE per 10-rule #10), but orphan detection on our side is in-scope.
- surface: `specify workflow run specere-observe` spawns `claude -p "/speckit-specify …"` as a subprocess. If that subprocess is killed before completing, partial state is left behind: (a) a ghost feature branch with no commits, (b) a `specs/NNN-.../` dir containing only the unfilled template, (c) `.specify/feature.json` still pointing at the ghost dir, (d) an orphan `.specify/workflows/runs/<run-id>/` artifact.
