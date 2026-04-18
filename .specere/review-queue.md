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

_(migrated from `.specere/decisions.log` on drain.)_
