# Auto-review setup — Claude reviews every PR

The `Claude PR review` CI job at [`.github/workflows/claude-review.yml`](../.github/workflows/claude-review.yml) runs on every `opened` / `synchronize` / `reopened` pull request event and posts findings as a PR review. The review enforces the constitution's 10-rule composition pattern, reversibility, test coverage per FR, cross-platform path safety, doc-sync drift, and the narrow-parse rule.

The workflow is idempotent across re-pushes (concurrency group scoped by PR number; in-flight runs are cancelled).

## First-time setup

Pick **one** of the following. The GitHub App route is preferred for this public repo — no static secret in the repo's secret store.

### Option A — Claude GitHub App (preferred)

From any `claude` session on this repo:

```bash
claude /install-github-app
```

This walks the repo owner through the OAuth install, wires the action's auth against the App's identity, and requires no `ANTHROPIC_API_KEY` secret. The workflow picks up the App credential automatically.

Verify on the next pull request: the `review` job should run without `ANTHROPIC_API_KEY`-related errors in the logs.

### Option B — API key secret

1. [Create an API key](https://console.anthropic.com/) under your Anthropic org.
2. Repo → **Settings → Secrets and variables → Actions → New repository secret**.
3. Name: `ANTHROPIC_API_KEY`. Value: the key.
4. Open any PR; the workflow picks it up on next `pull_request` event.

API-key path is fine for private repos but exposes a static credential; rotate on schedule.

## Scope + opt-outs

- **Forks are skipped** — GitHub's fork token policy prevents the workflow from posting review comments across the fork boundary. The `if:` gate on the job detects this.
- **No blocking** — the review job is advisory. The authoritative CI gates are `rustfmt`, `clippy`, `test`, and `docs-sync`. A Claude review requesting changes will surface as a PR review but does not set required-check status.
- **Escape hatch** — to skip review on a trivial PR (typo fix, dependabot metadata), leave a PR comment starting with `@claude skip`. The action respects it on the next run. Prefer this over disabling the workflow file.

## What the review checks

Lifted verbatim from `.github/workflows/claude-review.yml`'s prompt, in priority order:

1. **Constitution compliance** — all 10 composition rules; rules 1, 2, 4, 8, 10 flagged as blocking on violation.
2. **Reversibility** — every new install path has a matching `remove`.
3. **Test coverage per FR** — regression tests for every FR named in the diff.
4. **Cross-platform** — path separators, `CARGO_BIN_EXE_*`, shell assumptions.
5. **Doc-sync** — `crates/**/*.rs` changes imply a docs touch; corroborates the `docs-sync` CI job.
6. **Narrow parse surface** — new file formats must extend FR-P1-008's declared-format list.

## Relation to the harness's self-extension principle (constitution V)

Auto-review is the CI-surface companion of the `specere review check / drain` skills inside the repo. Both emerge from constitution principle V (the harness notifies humans when coverage is insufficient). The in-repo review queue catches *runtime* surface drift; this CI job catches *PR-time* surface drift.
