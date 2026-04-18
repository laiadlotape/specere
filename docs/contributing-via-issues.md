# Contributing via issues

The canonical pipeline for non-trivial SpecERE work: **bug / flaw / feature** → **parent issue** → *(optional)* **sub-issues** → **branch(es)** → **PR(s)** → **CI green** → **merge**. This is how the harness itself got built — PRs #2 through #6 followed exactly this shape.

Read this before opening an issue or a PR larger than a typo fix.

## The 10 steps

1. **Spot.** You notice a bug, a design flaw, or a missing capability. First check `docs/upcoming.md` — if it's already queued, either pick it up or wait. If not, proceed.
2. **Open a parent issue.** Use https://github.com/laiadlotape/specere/issues/new. Body template:
   ```markdown
   ## Problem
   <one paragraph, concrete, links where the gap is visible>

   ## Why it matters
   <impact, who is blocked, what breaks>

   ## Proposed fix (shape)
   <bullets; scope-bounded; cite the constitution rule or phase if applicable>

   ## Non-goals
   <explicit out-of-scope bullets so scope creep has a place to land>

   ## Acceptance
   <bullets naming files, commands, tests, or SCs that should pass>
   ```
   Issue #6 (this feature's own parent) follows that template — re-read it as a reference.
3. **Decide scope.** Heuristic: if the fix is ≤ 200 LoC across ≤ 5 files with one cohesive concern, **skip sub-issues** — one PR is enough. Otherwise open a sub-issue per independently-deliverable piece. Each sub-issue's body starts with `Parent: #<N>` and uses the same template.
4. **Link sub-issues.** GitHub's native sub-issue feature GA'd in 2025; after creating each sub-issue, link it:
   ```sh
   SUB_ID=$(gh api repos/laiadlotape/specere/issues/<sub-number> --jq .id)
   gh api "repos/laiadlotape/specere/issues/<parent-number>/sub_issues" -X POST -F "sub_issue_id=$SUB_ID"
   ```
   The parent's issue page then renders the child list with a progress bar.
5. **Branch naming.** `NNN-short-slug` — `NNN` is the next free 3-digit counter matching `specs/` (e.g. the last was `005-release-infra`, so next is `006-…`). The slug is 2-5 words of the parent issue's topic.
6. **Optionally run `/speckit-specify`.** FR-bearing work (anything adding a constitution rule, a test-able FR, or a user-facing CLI surface) runs the full `/speckit-observe` workflow — `/speckit-specify → /speckit-clarify → /speckit-plan → /speckit-tasks → /speckit-implement` — scaffolding `specs/NNN-.../`. Pure-docs / pure-infra issues can skip and go straight to a PR.
7. **PR per sub-issue** by default. Open the PR as soon as the branch has a coherent first commit. The body template:
   ```markdown
   ## Summary
   <one paragraph + bullet list>

   Fixes #<parent>
   Closes #<sub1>, #<sub2>, #<sub3>   <!-- if single PR closes multiple sub-issues -->

   ## Test plan
   - [ ] <per-check boxes>

   ## Post-merge
   <housekeeping; upcoming.md updates; follow-up issues>
   ```
   The `Fixes` + `Closes` lines auto-close the referenced issues on merge.
8. **CI gates (authoritative).** `rustfmt`, `clippy`, `test (ubuntu-latest)`, `test (macos-latest)`, `test (windows-latest)`, `docs-sync`. Plus the advisory `review` job (Claude auto-review — see [`auto-review.md`](auto-review.md)). Fix red X's before asking for review; the review is not a substitute for the required checks.
9. **Merge via merge-commit.** `gh pr merge <n> --merge --delete-branch`. Keep the PR's commit history — `/speckit-*` workflow trails are themselves documentation. Do **not** squash unless the history is genuinely noisy (rebase-and-dance, many fixup commits).
10. **Post-merge cleanup.**
    - Strike the parent issue off the priority queue in [`docs/upcoming.md`](upcoming.md); add a one-line entry under `## Recently closed`.
    - If the merge cut a release tag, update the `## Status` table in [`README.md`](../README.md) — the `docs-sync` CI enforces this, but the update is your responsibility.
    - If the work surfaced any follow-ups you deferred, open them as fresh issues immediately — don't let them live in your head.

## Examples (from the git log)

| PR | Parent issue | Sub-issues | Notes |
|---|---|---|---|
| #2 | n/a (scaffold phase) | — | Phase 1 bugfix release, 9 FRs, 37 tests. Full `/speckit-observe` workflow. |
| #3 | n/a (drift fix) | — | README sync + `docs-sync` CI gate added. Self-validating. |
| #4 | n/a (follow-up) | — | Auto-review CI job + subsumes Dependabot #1. |
| #5 | n/a (follow-up) | — | Release infra — closed `docs/upcoming.md` priority 1. |
| #6 | #6 (meta) | #7, #8, #9 | Harness-gap for subagents + rules + this doc. |

## Anti-patterns

- **Single PR bundling three unrelated issues.** Breaks merge-commit story-telling; makes bisect useless. Open three PRs instead.
- **Feature branch off `main` without an issue.** Fine for a typo fix; not fine for anything that could generate follow-up discussion. If reviewers will want to discuss *why*, the discussion belongs on an issue, not scattered across PR comments.
- **Skipping `docs-sync`** via `[skip-docs]` without justification. The escape hatch is for pure refactors / renames / CI-only changes, not for "I'll update docs in a follow-up PR" — which never happens.
- **Merging your own PR with a red `review` check** you didn't read. The job is advisory but the Claude review output is often a free round of critique; skim it before merge.

## When *not* to use this flow

- Typo fix in a comment or doc — just PR it. No issue needed.
- Obvious one-line bug fix with a regression test — PR it; issue only if there's user discussion to capture.
- Dependabot / Renovate automated bumps — they open their own PRs; review + merge directly.
