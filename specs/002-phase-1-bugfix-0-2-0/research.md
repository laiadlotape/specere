# Phase 0 Research — Phase 1 Bugfix Release (0.2.0)

**Status**: Trivial — the `/speckit-clarify` pass already resolved every ambiguity in-spec. This document exists to satisfy the `/speckit-plan` skill's Phase-0 contract and to record the two technology-choice decisions that the plan made without escalating to a questionnaire.

## Resolved `[NEEDS CLARIFICATION]` markers

None. The clarified spec (`spec.md § Clarifications`) closed every marker; the plan did not introduce new ones.

## Technology choices

### Decision 1 — YAML parser for `.specify/extensions.yml`

- **Decision**: `serde_yaml` (v0.9.x) for read-only parse + marker-fenced region mutation via string manipulation.
- **Rationale**: `serde_yaml` is the de-facto standard in the Rust ecosystem and integrates with the existing `serde` derive macros already in workspace dependencies. Mutating via marker-fenced region (not parse + re-serialize) is mandatory because the git extension's 17 hook entries depend on exact-format preservation — re-serialization would reformat the file and fail `SC-004` (byte-identical remove).
- **Alternatives considered**:
  - `saphyr` (newer, claims better YAML 1.2 compliance) — not yet widely adopted; API volatility risk for a stable release.
  - `yaml-rust2` — no serde integration; more manual parsing code.
  - Hand-rolled line scanner — would work since we never re-serialize, but `serde_yaml` gives us a validated-parse fallback for FR-P1-008.

### Decision 2 — SHA256 implementation

- **Decision**: `sha2::Sha256` (RustCrypto's `sha2` crate).
- **Rationale**: Matches the hash algorithm that SpecKit uses in its own `.specify/integrations/*.manifest.json` files (inspected in the live scaffold). Audited, stable, zero non-Rust deps.
- **Alternatives considered**:
  - `blake3` — faster but not the format SpecKit uses; mixed ecosystems in `.specere/manifest.toml` would be confusing.
  - `ring::digest::SHA256` — pulls in a heavier crypto dep for one hash use.

## Git sub-process policy

- `specere` shells out to `git` for three operations: `status --porcelain`, `checkout -b`, `branch -D`. No other git invocations in Phase 1. We use `std::process::Command` directly — no `git2` crate (adds 150+ kLOC of C bindings for three commands).

## Integration with existing code

- `specere-core::Error` gains three variants: `AlreadyInstalledMismatch { unit, file }`, `ParseFailure { path, format, inner }`, `DeletedOwnedFile { unit, path }`.
- `specere-manifest` gains a `branch_name: Option<String>` + `branch_was_created_by_specere: bool` on `UnitInstallConfig`.
- `specere-markers` is already sufficient — the generic `<!-- specere:begin {unit-id} -->` / `<!-- specere:end {unit-id} -->` parser / writer handles every marker block this plan needs.

## No further research required for Phase 1

- Cross-platform `git` binary location: rely on `$PATH`; document the assumption in quickstart.md.
- Windows path separators: `Path::join` everywhere, no string concatenation.
- `specify-cli` availability: if the `specify` binary is not on `$PATH`, `speckit.rs` fails with a pre-existing `Error::SpecifyCliMissing` variant — no change needed.
