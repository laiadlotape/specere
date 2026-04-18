# Roadmap — long-term tool

> Horizon: 12-18 months. Contingent on Gate C (paper received external validation). Do not invest serious engineering effort before Gate C.
>
> **Repo identity.** The tool lives in a separate repo `laiadlotape/specere` (public, Apache-2.0). `SpecERE` = *Spec Entropy Regulation Engine*; name from Latin `specere`, "to look / to observe" — the tool observes agent and repo activity to maintain a posterior over spec satisfaction.
>
> **Monorepo separation.** ReSearch itself is the **research monorepo** (investigation, analysis, paper, prototype). SpecERE is the **engineering monorepo**. They cross-reference but do not co-commit. ReSearch is the source of truth for *theory*; SpecERE is the source of truth for *implementation*.

## Vision

A filter-as-a-service that attaches to a coding agent harness (Claude Code primary, OpenCode and Cline next) via its hook system, maintains a persistent per-repo posterior over specification satisfaction, surfaces confidence and drift alerts to the developer, and provides information-gain-driven test prioritisation.

## Phase 0 — Scaffolding mechanism (pre-Gate-C; runway work)

> The one piece that can legitimately start before Gate C: a scaffolding CLI so that when the math *is* trusted, the integration surface already exists. Low risk — it is pure plumbing, no inference math.

**Decision lock (2026-04-18).**

- **Language:** Rust (single binary, matches RTK philosophy, clean cross-platform install).
- **License / visibility:** Apache-2.0, public from day one.
- **UX:** composable `add` commands, each idempotent, each with a reverse `remove` (for the human validation loop). The `add` surface initially ships with:
  - `specere add speckit` — install SpecKit scaffolding into the target repo (via the mechanism documented in `docs/research/08_speckit_deepdive.md`).
  - `specere add otel-hooks` — wire OTel GenAI semantic conventions into the repo's harness config.
  - `specere add filter-state` — create the `.specere/` state directory and initial config.
  - `specere add claude-code-hooks` — write the Claude Code hook payload capture.
  - (More to be enumerated after the SpecKit deep-dive lands.)
- **Uninstall.** First-class, not an afterthought. Every `add X` must register what it touched so `remove X` reverses it cleanly; supports the iterative human-validation loop the user called out explicitly.
- **Target validation.** ReSearch itself is the first dogfood target: `specere add speckit && specere add otel-hooks` against `/home/lotape6/Projects/ReSearch`, validated by human review, uninstalled, re-run until clean.

**Design constraints (from the reboot brief).**

1. **Scalability** — the scaffolding mechanism must accept new `add` targets without touching core; new components are declarative manifests, not hardcoded branches.
2. **Compatibility** — must not mutate files it does not own; must not conflict with SpecKit, Cursor rules, Aider conventions, or agent-harness state when those already exist.
3. **Configurability** — every `add` target takes a small, documented set of knobs; defaults chosen so `specere add X` with no flags does the right thing on a greenfield repo.
4. **Professional project abstraction** — the scaffolding is itself scaffolded: `cargo new`-grade init for the SpecERE repo, with CI, release automation, docs-as-code, and examples from day one.

Concrete design document follows the SpecKit deep-dive (see `docs/research/08_speckit_deepdive.md` when it lands).

## Component sketch

```
┌──────────────────────────────────────────────────────────────┐
│  Coding agent harness (Claude Code / OpenCode / Cline)       │
│  - emits hook events (PreToolUse / PostToolUse / Stop / ...)  │
└────────────────────┬─────────────────────────────────────────┘
                     │ OTel GenAI semantic conventions
                     ▼
┌──────────────────────────────────────────────────────────────┐
│  Telemetry collector (OTel + hook adapters per harness)       │
└────────────────────┬─────────────────────────────────────────┘
                     │ normalised event stream
                     ▼
┌──────────────────────────────────────────────────────────────┐
│  Filter engine                                                │
│  - per-spec HMM + factor-graph BP + RBPF escape + iSAM2 smoother │
│  - calibrated motion/sensor params from git history           │
└────────────────────┬─────────────────────────────────────────┘
                     │ posterior snapshots (append-only log)
                     ▼
┌──────────────────────────────────────────────────────────────┐
│  Views                                                        │
│  - confidence dashboard per spec                              │
│  - agent-behaviour fingerprints                               │
│  - test prioritisation list                                   │
│  - drift alerts (webhook / slack / email)                     │
└──────────────────────────────────────────────────────────────┘
```

## Phases

**Phase 1 — Observability foundation (2 months).**
- OTel collector with adapters for Claude Code hooks, OpenCode events, Cline notifications (see `docs/research/01_agent_telemetry.md` §1-§4).
- Append-only event log with a clean schema.
- Gate: live hook events from 1 harness flowing into the collector in a dev repo.

**Phase 2 — Offline calibration (2 months).**
- Git-history walker that reconstructs (diff, test-delta) pairs and fits per-spec motion transition $T_i$, collateral rate $\eta_i$, test sensitivity $\alpha_t$, flake rate $\beta_t$.
- Surfaces "this test has near-zero kill-rate" warnings that are useful standalone.
- Gate: calibration pipeline runs on 3 real repos and produces plausible parameter estimates.

**Phase 3 — Online filter engine (3 months).**
- pgmax or GTSAM-based factor-graph implementation.
- Spec dependency graph loader (human-authored DAG).
- Live posterior updates on each hook event.
- Gate: on a real agent session of 4 hours, the filter surface posterior updates within 500ms of hook events.

**Phase 4 — Human-facing surfaces (3 months).**
- Confidence dashboard (a web app reading the posterior log).
- Test prioritisation: `filter-cli suggest-tests` returns top-K information-gain tests.
- Drift alerts.
- Adjudication workflow — the human resolves divergences, online-updating $T, \eta, \alpha, \beta$.
- Gate: used by at least one external team on a real project for 1 month.

**Phase 5 — Multi-harness + polish (2+ months).**
- Second harness adapter.
- Multi-agent joint-state (if Phase 4 surfaces demand).
- Public release, docs, examples.

## Explicit non-goals

- **Not another TaskSLAM.** Do not recreate chemistry/atoms/molecules vocabulary, do not build a GUI before the numbers are trusted, do not build orchestration or sub-agent spawning. Scope-creep killed prior attempts in this problem space — see `~/Documents/Town-Notes/ReSearch.md` for the historical record.
- **Not a replacement for CI or test runners.** It reads their outputs, it doesn't execute them.
- **Not a spec authoring tool.** It consumes specs from whatever system the team uses (Spec Kit, Kiro, plain Markdown, OpenAPI — see `docs/research/02_sdd_contracts.md`).
- **Not an agent harness.** It attaches to existing ones.

## Anti-patterns to avoid (from prior lineage)

1. Building a CLI before the math is trusted on a prototype.
2. Building a GUI before the CLI is usable.
3. Defining new domain vocabulary (atoms/molecules/worlds/maps) before it adds real expressive power.
4. Attempting C++ rewrites before a working Python version exists.
5. Designing for "millions of tasks" before handling 10 specs.

The shortest path to value is: toy prototype → paper → calibration pipeline (valuable standalone) → filter engine. Everything else is risk.
