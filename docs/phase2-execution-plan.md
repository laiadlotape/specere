# Phase 2 execution plan — auto-mode sequential delivery of issues #11–#16

**Status.** Drafted 2026-04-18, after v0.2.0 release and the harness / auto-review / rules work. Governs the sequential delivery of Phase 2's five sub-issues.
**Authority.** `docs/contributing-via-issues.md` (pipeline) · `docs/specere_v1.md §5 Phase 2` (scope) · `.specify/memory/constitution.md` (rules).

This doc is meant to be read cold. A session resuming Phase 2 work should need nothing else to pick up the next sub-issue.

## 1. Context

Phase 2 of the `docs/specere_v1.md` master plan promotes three stub units (`filter-state`, `otel-collector`, `ears-linter`) to real implementations, adds a `specere init` meta-command, and lands the `speckit::preflight` orphan-detector carry-over from the 2026-04-18 review-queue drain. Five GitHub sub-issues track the work:

| Issue | Title | FR |
|---|---|---|
| [#12](https://github.com/laiadlotape/specere/issues/12) | filter-state unit | FR-P2-001 |
| [#13](https://github.com/laiadlotape/specere/issues/13) | otel-collector unit | FR-P2-002 |
| [#14](https://github.com/laiadlotape/specere/issues/14) | ears-linter unit | FR-P2-003 |
| [#15](https://github.com/laiadlotape/specere/issues/15) | `specere init` meta-command | FR-P2-005 |
| [#16](https://github.com/laiadlotape/specere/issues/16) | speckit orphan detector | decisions.log carry-over |

Parent: [#11](https://github.com/laiadlotape/specere/issues/11).

## 2. Auto-mode contract

**"Auto mode" means:** the assistant proceeds through the sequence without interactive gates *unless* one of the re-planning or escalation triggers (§5, §6) fires. Every sub-issue still flows through the issue-driven pipeline (`docs/contributing-via-issues.md`): feature branch → implementation → PR → CI → merge. Auto mode does not skip the pipeline; it just removes the per-verb questionnaires.

**Abbreviations relative to the full `/speckit-observe` workflow:**

- `/speckit-specify` / `/speckit-clarify` are skipped. Each sub-issue's body already satisfies both — acceptance criteria are explicit and ambiguity was resolved when the issue was filed.
- `/speckit-plan` is compressed to a one-paragraph "approach" section in the PR body; no separate `specs/NNN-*/plan.md` unless the scope surprises.
- `/speckit-tasks` is skipped when the implementation is ≤ ~200 LoC (all of #12–#16 fit). The commit chain is the task log.
- `/speckit-implement` runs normally: TDD (red → green), lint clean, fmt clean, docs-sync satisfied.
- `review-spec` / `review-plan` gates are not applicable (no separate spec/plan docs this time). The `divergence-adjudication` gate is replaced by the post-implement review-queue-drain check.
- The CI `review` job remains advisory. Its output is read but not gated on.

## 3. Sequence + dependency graph

```
                               ┌─► #13 otel-collector ─┐
                               │                        │
#12 filter-state ─► #16 orphan ┤                        ├─► #15 specere init ─► DONE
                               │                        │
                               └─► #14 ears-linter ─────┘
```

**Linear delivery order** (auto mode runs one PR at a time; nothing parallel):

1. **#12 filter-state** — first. Smallest surface; everything below assumes `.specere/` skeleton exists.
2. **#16 orphan detector** — second. Independent, but strengthens `speckit::preflight` before later PRs exercise `speckit` again via `specere init`.
3. **#13 otel-collector** — third. Needs `.specere/` from #12.
4. **#14 ears-linter** — fourth. Needs `.specify/` (already present via the `speckit` wrapper unit).
5. **#15 `specere init`** — last. Composes #12, #13, #14 (and the existing `speckit` + `claude-code-deploy`).

Rationale for putting #16 second: it touches `crates/specere-units/src/speckit.rs` — the same file #15 will indirectly exercise through `specere init`. Landing orphan detection early means #15's end-to-end `specere init` test catches orphan-state bugs rather than silently succeeding.

## 4. Per-sub-issue recipe

For each issue, follow this recipe unless a re-planning trigger (§5) fires.

```
#  Action                                                         Notes
1  git checkout main && git pull --ff-only                        clean base per sub-issue
2  git checkout -b <NNN>-<short-slug>                             NNN = next free dir counter; slug = ≤ 5 words
3  Read issue body end-to-end                                     acceptance criteria are authoritative
4  Write the test file first (TDD red)                            one file per sub-issue: fr_p2_*.rs or issue_*.rs
5  Run the failing test to confirm red                            cargo test -p specere --test <name>
6  Implement                                                      stay inside the scope in the issue body
7  cargo fmt --all && cargo clippy -- -D warnings                 zero warnings
8  cargo test --workspace --all-targets                           must be green end-to-end
9  Update CHANGELOG [Unreleased]                                  include the FR + one-line summary
10 Update docs/upcoming.md if the sub-issue changes queue state   usually: nothing (queue is per-issue)
11 git add -A && git commit (HEREDOC body)                        title: feat/<unit>: …; body cites Fixes #N
12 git push -u origin <branch>                                    tracking
13 gh pr create with "Fixes #N" in body                           no manual links needed; GH auto-closes
14 gh pr checks <pr> --watch                                      waits for all checks to terminate
15 On failure: diagnose + push fix (max 3 retries)                escalate if still failing (§6)
16 On green: gh pr merge <pr> --merge --delete-branch             merge-commit preserves history
17 git checkout main && git pull --ff-only                        sync post-merge
18 Verify sub-issue auto-closed                                   "Fixes" should close it
19 Re-plan check (§5)                                             any triggers? pause and reassess
20 Proceed to next sub-issue OR stop                              per §5/§6
```

## 5. Re-planning triggers

Between sub-issues, re-plan (update this doc + reorder if needed) when **any** of these fire:

- **Test-count deviation** — the sub-issue delivers > 1.5× or < 0.5× the estimated test count (§8). Signals scope surprise; review the remaining estimates.
- **New FR surfaces** — constitution V: an implementation reveals a missed invariant worth a new FR. Add it to the CURRENT sub-issue's PR if trivial; otherwise open a new sub-issue, linked as a new child of #11.
- **Scope growth** — the sub-issue's PR exceeds 500 LoC of new code (tests excluded). Pause, reassess: is this truly one sub-issue, or should it be split?
- **Cross-sub-issue contract change** — implementing #12 changes the `AddUnit` trait shape that #13/#14/#15 rely on. Pause; update the downstream issues' acceptance criteria before proceeding.
- **Review-queue drain surfaces a novel item** — not a pre-queued EXTEND. The post-PR drain is a normal part of the recipe; a *novel* item (one that needs real adjudication) halts auto mode.
- **CI wall-clock regression** — test suite takes > 2 min locally, or > 5 min on any CI runner. Investigate before continuing.

## 6. Escalation-to-user triggers

Stop auto mode and ask the user when **any** of these fire:

- **CI fails on the same PR > 3 times** in a row with different root causes. Pattern suggests the sub-issue's acceptance criteria are inconsistent with the existing code.
- **A required credential or GitHub App** surfaces (e.g. crates.io token for a publish step). The assistant can't install these; the user must.
- **A breaking change to a downstream contract** lands in a Phase-2 PR's blast radius — e.g. renaming `AddUnit::install` signature. The user may want the change but should see the rename explicitly.
- **A spec-level disagreement with the issue body** — during implementation, the acceptance criteria turn out to be internally inconsistent or conflict with the constitution. Pause; the user authors the spec, not the assistant.
- **User interrupts** with a course correction or a new task. Stop Phase 2 immediately; do not resume without explicit go-ahead.

## 7. Phase 2 exit criteria

Phase 2 closes when **all** of:

- [ ] #12, #13, #14, #15, #16 all merged to `main` (auto-close from their PRs).
- [ ] Parent #11 closed (auto-close via the final child's "Fixes" keyword, OR manually after all children close).
- [ ] `cargo test --workspace --all-targets` green on `main` with test count 60+ (up from 44 at v0.2.0).
- [ ] `specere init` on a fresh git repo (e.g. a temp dir, no SpecKit prior) completes successfully and produces all five unit manifest entries.
- [ ] `docs/upcoming.md` shows `phase-2-native-units` under `## Recently closed`, with `phase-3-observe-pipeline` as priority 1.
- [ ] `README.md`'s phase-status table marks Phase 2 ✅ Shipped with a date and PR/merge-commit reference.
- [ ] A release tag cut for v0.3.0 (optional: may be deferred to a release-infra follow-up). If cut: all Phase-2 CHANGELOG entries move from `[Unreleased]` to `[0.3.0] - <date>`.

## 8. Estimates

Per-sub-issue, rough sizing (calibrated on PR #2's FR-per-test density and PR #10's deploy-extension density):

| Issue | Est. LoC (impl) | Est. LoC (tests) | Est. tests | Est. CI retries | Risk |
|---|---|---|---|---|---|
| #12 filter-state | 120 | 180 | 5 | 0 | low — pure file creation |
| #16 orphan detector | 90 | 150 | 4 | 1 (cross-platform git) | med — git state detection |
| #13 otel-collector | 160 | 200 | 5 | 0–1 | med — per-OS service files |
| #14 ears-linter | 180 | 220 | 5 | 0 | low — pattern matching |
| #15 `specere init` | 120 | 200 | 4 | 1 | med — idempotency across units |
| **Total** | **~670** | **~950** | **~23** | **~3** | |

Post-Phase-2 test total projection: 44 + 23 ≈ **67 tests**.

## 9. Living document

This plan is re-written in place when any of §5's re-planning triggers fires. Update the sequence (§3) + estimates (§8) + any rationale that changed. The git history preserves prior versions; do not rename the file.

When Phase 2 closes, move this doc to `docs/history/phase2-execution-plan.md` so `docs/` stays current-focused. Keep it around — the estimate calibration is valuable input to Phase 3's plan.
