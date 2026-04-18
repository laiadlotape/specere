# Contract: safe mutation of `.specify/extensions.yml`

SpecKit owns `.specify/extensions.yml`; the git extension already writes 17 hook entries into it at scaffold time. SpecERE mutates this file under the following contract — any violation is a bug, not a feature.

## Marker convention (YAML line-comment fences)

HTML-style comments (`<!-- … -->`) don't survive inside a YAML block sequence's individual items. We use YAML line comments as fences, matched by `specere-markers::yaml_block_fence`:

```yaml
hooks:
  after_implement:
  - extension: git
    command: speckit.git.commit
    enabled: true
    optional: true
    prompt: Commit implementation changes?
    description: Auto-commit after implementation
    condition: null
  # >>> specere:begin claude-code-deploy
  - extension: specere
    command: specere.observe.implement
    enabled: true
    optional: false
    prompt: Record Repo-SLAM observation from the just-completed implement run?
    description: SpecERE telemetry + post-implement filter step (FR-P1-005)
    condition: null
  # <<< specere:end claude-code-deploy
```

The begin/end markers are whole lines, indented to match the block-sequence item level (two spaces inside `hooks.<verb>:`).

## Add protocol

Given: unit id, verb (e.g. `after_implement`), and the hook entry object to insert.

1. Read the file; try `serde_yaml::from_str::<serde_yaml::Value>`. On parse failure → `Error::ParseFailure` (FR-P1-008).
2. If a `# >>> specere:begin <unit-id>` line already exists under the target verb → no-op, exit 0 (idempotent).
3. Locate the `hooks.<verb>:` list in the original file's line buffer (not the parsed tree) by line-scanning. If missing, synthesize `  <verb>:\n` after the `hooks:` line.
4. Serialize the hook entry as YAML with the standard two-space indent, prefixed with `  - ` for the first field (`extension: specere`).
5. Insert, preceded by `  # >>> specere:begin <unit-id>` and followed by `  # <<< specere:end <unit-id>`, at the end of the verb's list (or directly after the verb key if the list was empty).
6. Write the file back atomically (write to `<path>.tmp` + `rename`).
7. Record the marker pair in the unit's `[[units.markers]]` manifest entry.

## Remove protocol

Given: unit id.

1. Read the file; `serde_yaml` parse safety check (FR-P1-008).
2. Find the begin marker `# >>> specere:begin <unit-id>`. If absent → no-op, exit 0 (already removed).
3. Find the matching end marker `# <<< specere:end <unit-id>`. If missing → `Error::MarkerUnpaired` (exit 1). Do **not** attempt to repair.
4. Excise the two marker lines and everything between, preserving the surrounding blank-line count: if the removal results in two adjacent blank lines, collapse to one; otherwise preserve exactly.
5. Write atomically.
6. Verify the result still parses as valid YAML.

## Invariants

- **No re-serialization.** The file is mutated as a text buffer. `serde_yaml` is only used for parse validation.
- **No cross-block reflow.** Adding / removing a specere block MUST NOT touch any other block's indentation, comments, or ordering.
- **No `serde_yaml::to_string` round-trip**, ever. It would alphabetize map keys and destroy the git extension's field ordering.
- **Byte-identity**: after `add X && remove X` the file byte-matches its pre-add state (the load-bearing claim behind SC-004/FR-P1-006).

## Tests

- `fr_p1_005_hook_registration.rs` — add-only: verify exactly one entry appears under `after_implement` with `extension: specere`.
- `fr_p1_006_remove_round_trip.rs` — add → remove → compare SHA256 of the file to pre-add snapshot.
- `fr_p1_008_malformed_file_refuse.rs` — corrupt the YAML (mismatched quote), attempt add, expect exit 3.
- Property test (nice-to-have, not blocking v0.2.0): add 10 random specere unit IDs, then remove them all in a random order. File must match pre-add state.
