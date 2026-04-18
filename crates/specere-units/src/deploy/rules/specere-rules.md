## SpecERE rules (session-durable)

These rules govern every change agents make in this repo. They duplicate `.specify/memory/constitution.md` by design — rules loaded into every session context cannot be drowned out by a long conversation.

### The 10 composition rules (NON-NEGOTIABLE)

1. **Installer detects ambient git-kind.** On a git repo, never pass `--no-git`; auto-create a feature branch (`000-baseline` default). Never `--force` without a SHA-diff step.
2. **Hook registration is the only runtime attach point.** Hooks live in `.specify/extensions.yml` only — never embed dispatch in slash-command prompts.
3. **Template overrides go only in `.specify/templates/overrides/`.** Never edit files under `.specify/templates/` directly.
4. **Context-file ownership uses marker-fenced blocks.** `<!-- specere:begin {unit-id} -->` … `<!-- specere:end {unit-id} -->`, one pair per unit. Content outside the fence is never touched.
5. **`.specere/sensor-map.toml` is SpecERE-native.** Nothing else reads or writes it.
6. **One SpecKit-registered workflow: `specere-observe`.** No parallel orchestrator.
7. **Namespace: SpecERE slash commands are `specere-*`.** Never reuse or rename `speckit-*`.
8. **Uninstall consults `.specere/manifest.toml`.** SHA256 match required; preserves user-edited files; delegates SpecKit core removal to `specify integration uninstall`.
9. **Update is user-confirmed.** `specere update speckit` probes + prompts; never auto-upgrades.
10. **Parse narrowly.** SpecERE parses YAML (`.specify/extensions.yml`), TOML (`.specere/*.toml`), JSON (`.specify/workflows/workflow-registry.json`), plain text (`.gitignore`). All other files are opaque.

### NEVER list

- **Never** re-implement what SpecKit or OTel GenAI semconv already does. Wrap, ignore, or extend — never clone. See `docs/research/09_speckit_capabilities.md` §13 for the 22 WRAP / 4 IGNORE / 15 EXTEND matrix.
- **Never** edit `.specify/templates/*` directly. Overrides go in `.specify/templates/overrides/`.
- **Never** write outside a marker-fenced block in a co-owned file (`CLAUDE.md`, `.gitignore`, `.specify/extensions.yml`, any future shared file).
- **Never** `--force` on a re-install without first running the SHA-diff gate (FR-P1-003).
- **Never** push to `main` directly. Every non-trivial change is a PR with `rustfmt` + `clippy` + `test × 3 OS` + `docs-sync` green.
- **Never** tag a release without version-bump + CHANGELOG-section + `release-guards.yml` green. See `docs/release.md`.
- **Never** block the user in the per-tool-call loop. Human-in-the-loop gates are only at `review-spec`, `review-plan`, and `divergence-adjudication` per `core_theory.md` §4.
- **Never** silently drop a review-queue item; constitution principle V requires every `.specere/review-queue.md` entry to be drained via explicit decision (EXTEND / IGNORE / ALLOWLIST / ADJUDICATE) logged in `.specere/decisions.log`.

### When in doubt

Read `.specify/memory/constitution.md` — it is authoritative. This block is a session-time summary, not a replacement.
