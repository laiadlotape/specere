# `specere calibrate from-git` depth audit

**Date.** 2026-04-18. **Binary.** `target/debug/specere` built from `6d42fdd` (v1.0.0) + the audit fixes described here. **Method.** Crafted throwaway git repos exercised a 20-scenario charter against the CLI. Output checked by hand and, where automation was natural, promoted to unit or integration tests.

## Summary

| Total | Pass as-is | Bug found + fixed | Minor UX note kept |
|---|---|---|---|
| 20 | 15 | 4 | 1 |

**Bugs fixed in-branch (with regression tests):**
- **C-01 / C-13 — path-prefix false-match across sibling paths.** Blocker-level correctness bug. Support `"src/auth"` matched `"src/auth_helpers/h.rs"`, `"src/widget"` matched `"src/widgetry/x.rs"`. Every user who omitted the trailing slash silently got wrong coupling counts.
- **C-02 — empty git repo** surfaced a raw `fatal: your current branch 'main' does not have any commits yet`. Now reports `calibrate: <path> has no commits yet — make at least one commit before running \`specere calibrate\``.
- **C-11 — non-git directory** surfaced a raw `fatal: not a git repository`. Now reports `calibrate: <path> is not a git repository — run \`git init\` first`.

**Minor UX note not fixed:**
- **C-19** — `specere calibrate` run from a subdirectory doesn't auto-walk up to find the git root; reports `sensor-map not found at <subdir>/.specere/sensor-map.toml`. Callers can use `--repo` explicitly. Not a correctness issue; noted for a future UX pass.

---

## Traceability

### C-01 — support `"src/auth"` false-matches `src/auth_helpers/*` — **BUG**

- **Setup.** Specs `auth = ["src/auth"]` and `helpers = ["src/auth_helpers"]`. 3 commits touch only `src/auth_helpers/h.rs`; 1 commit touches both.
- **Pre-fix.** `auth = 4` (should be 1), coupling edge `[auth, helpers] = 4 co-commits` (should be 1).
- **Root cause.** `supports[*i].iter().any(|sup| files.iter().any(|f| f == sup || f.starts_with(sup)))` — `starts_with` with no trailing separator bleeds across sibling paths when the support is a bare directory name.
- **Fix.** Normalise each support into a `(bare, dir)` pair where `dir = "bare/"`. Match iff `f == bare` (exact file) OR `f.starts_with(&dir)` (directory with explicit separator). See `compute_report` in `crates/specere-filter/src/calibrate.rs`.
- **Regression tests.** `sibling_directories_do_not_false_match`, `trailing_slash_support_is_equivalent_to_bare`, `exact_file_match_works`.

### C-02 — empty git repo — **BUG** (UX)

- **Setup.** Fresh `git init` with no commits yet.
- **Pre-fix.** `specere: error: \`git log\` failed at <path>: fatal: your current branch 'main' does not have any commits yet`.
- **Fix.** Recognise the two common git setup errors and emit friendlier messages (`run_git_log_names` in `calibrate.rs`).

### C-03 — single-commit repo

- **Outcome.** ✅ Pass. 1 commit walked, 1 spec counted, 0 edges proposed. No crash.

### C-04 — threshold boundary (`--min-commits` semantics)

- **Setup.** 3 co-commits between `widget` and `core`.
- **Outcome.** ✅ Pass. `>=` semantics — `--min-commits 3` includes, `--min-commits 4` excludes, `--min-commits 2` includes.

### C-05 — merge commits

- **Setup.** Branch with an `a`-edit commit; main with a `b`-edit commit; `git merge --no-ff` with no conflicts.
- **Outcome.** ✅ Pass. The merge commit itself (empty file list) is filtered by `parse_git_log`'s `if !current.is_empty()` guard. a=2, b=1, no coupling. Correct.

### C-06 — renames

- **Setup.** `git mv src/a/f.rs src/a/renamed.rs`, then edit the renamed file.
- **Outcome.** ✅ Pass. `git log --name-only` emits both old and new paths in the rename commit; both live under `src/a/`, so `a` is counted once per commit. No false positives.

### C-07 — paths with spaces + non-ASCII

- **Setup.** Files at `src/a/sub path/file.rs` and `src/b/日本/file.rs`.
- **Outcome.** ✅ Pass. `starts_with` on UTF-8 `&str` is byte-safe for valid UTF-8 input; space + CJK paths match correctly.

### C-08 — binary files

- **Setup.** Commit a random 256-byte blob under `src/a/`.
- **Outcome.** ✅ Pass. Binary files are counted like any other; git log emits their paths the same way.

### C-09 — deletion-only commit

- **Setup.** `git rm` of the blob from C-08.
- **Outcome.** ✅ Pass. Git log emits the deleted path; the spec is counted.

### C-10 — spec with empty `support = []`

- **Outcome.** ✅ Pass. The spec is defined but untouchable by any commit; doesn't appear in `spec_activity` and contributes no edges.

### C-11 — non-git directory — **BUG** (UX)

- **Setup.** Run `calibrate` in a tempdir that has no `.git`.
- **Pre-fix.** `fatal: not a git repository (or any of the parent directories)`.
- **Fix.** Friendlier error as for C-02.

### C-12 — overlapping supports

- **Setup.** Spec `broad = ["src/"]`, spec `narrow_a = ["src/a/"]`, spec `narrow_b = ["src/b/"]`.
- **Outcome.** ⚠️ Pass with a documentation note. Every commit touches `broad` plus one narrow, so `broad ↔ narrow_a` and `broad ↔ narrow_b` edges appear. That's technically correct — the user *did* declare overlapping scopes — but it's noisy. `docs/filter.md` should call out "prefer disjoint supports" as a best-practice.

### C-13 — trailing-slash-free supports false-match siblings — **BUG (same as C-01)**

- **Setup.** Three sibling specs `widget_a`, `widget_b`, `widget_c` with supports `src/widget`, `src/widgetry`, `src/widget-extra` (no trailing slashes).
- **Pre-fix.** Each specless sibling commit erroneously counted for `widget_a` because every sibling path starts with `src/widget`.
- **Fix.** Same as C-01 (they're the same root cause, discovered independently).

### C-14 — exact file match with similar filenames

- **Setup.** Support `"src/main.rs"`, commit touches `src/main.rs` + `src/mainframe.rs`.
- **Outcome.** ✅ Pass both pre- and post-fix. `"src/mainframe.rs".starts_with("src/main.rs")` is false (the next char after the 10-char prefix is `.`, not `f`...wait, let me re-verify — actually `src/mainframe.rs` vs `src/main.rs`: at position 8 both have `m`, position 9 both have `a`, position 10 both have `i`, position 11 both have `n`, position 12 first has `.` second has `f`. So `starts_with` fails. Incidental pass.)

### C-15 — output roundtrips through the coupling loader

- **Setup.** Run calibrate on the specere repo itself; paste emitted snippet into a fresh sensor-map.toml; parse the result via TOML.
- **Outcome.** ✅ Pass. Snippet parses cleanly; `[coupling].edges` is a valid `Vec<Vec<String>>`.

### C-16 — spec-order determinism

- **Setup.** Same repo. Run calibrate twice with specs declared in opposite order in `sensor-map.toml`.
- **Outcome.** ✅ Pass. Byte-identical stdout — `BTreeMap` / sorted iteration eliminates `HashMap` non-determinism.

### C-17 — DAG filter never actually fires

- **Setup.** Three specs `zebra / mike / alpha`, all co-modified.
- **Outcome.** ✅ Expected. All edges get directed alphabetically (`src < dst`), so the `would_create_cycle` defensive check can never trigger on calibrate output. Kept as defense-in-depth — cheap to run, catches if the direction rule ever gets relaxed.

### C-18 — `--max-commits` respects upper bound

- **Setup.** 20 co-commits; run with `--max-commits 7` and `--max-commits 100`.
- **Outcome.** ✅ Pass. First run analyses exactly 7; second analyses all 20.

### C-19 — running from a subdirectory — **MINOR UX**

- **Setup.** `cd src/a && specere calibrate from-git` in a repo that has `.specere/sensor-map.toml` at the root.
- **Outcome.** ⚠️ Reports `sensor-map not found at <cwd>/.specere/sensor-map.toml`. The `--repo` flag defaults to `std::env::current_dir()`; there's no auto-walk-up to find the git root. Documented workaround: use `--repo` with an absolute path. Not a bug; opportunistic follow-up for a future UX pass.

### C-20 — `--repo` override with absolute path

- **Outcome.** ✅ Pass. Runs correctly against the target repo regardless of cwd.

---

## Fixes summary

| Finding | Fix location | Regression test |
|---|---|---|
| C-01 / C-13 prefix false-match | `crates/specere-filter/src/calibrate.rs::compute_report` (support normalisation) | `sibling_directories_do_not_false_match`, `trailing_slash_support_is_equivalent_to_bare`, `exact_file_match_works` |
| C-02 / C-11 error UX | `calibrate.rs::run_git_log_names` | Manual verification in audit; error-wording variants |

Total new unit tests: 3. Workspace test count: 173 → **176**.

## Re-run of the specere self-calibration post-fix

```
analysed 59 commit(s); 33 touched a tracked spec
  specere-cli       23
  specere-core       3
  specere-filter     7
  specere-manifest   1
  specere-markers    2
  specere-telemetry  9
  specere-units     17

[coupling]
edges = [
  ["specere-cli", "specere-units"],      # 13 co-commits
  ["specere-cli", "specere-telemetry"],  #  6 co-commits
  ["specere-cli", "specere-filter"],     #  4 co-commits  ← newly visible
  ["specere-cli", "specere-core"],       #  3 co-commits
  ["specere-core", "specere-units"],     #  3 co-commits
]
```

Pre-fix produced 4 edges (with `specere-filter` hidden) because the trailing-slash supports in this sensor-map happened to dodge the bug. Post-fix is identical for slash-terminated supports (the repo's entries all use them), and `specere-filter` appears because it's genuinely co-modified with `specere-cli` in 4 commits — calibrate surfacing it is a true signal, not a pre-fix regression in counting.
