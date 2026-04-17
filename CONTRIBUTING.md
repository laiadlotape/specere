# Contributing to SpecERE

SpecERE is early; contributions are welcome but the design is still solidifying. Please open an issue before large changes.

## Local development

```bash
rustup toolchain install stable
git clone https://github.com/laiadlotape/specere
cd specere
cargo build
cargo test
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Adding a new `add` unit

1. Implement the `AddUnit` trait (see `specere-core`) in a new module under `crates/specere-units/src/`.
2. Register the unit in `specere-units::registry`.
3. Add an integration test under `crates/specere-units/tests/` that runs `add` → `status` → `remove` and asserts the tree returns to its pre-install state (modulo documented postflight effects).
4. Document the unit under `docs/src/units/<your-unit>.md`.

## Release process

Releases are driven by `cargo-dist`. Tag a commit `v0.X.Y` on `main`; the release workflow publishes to crates.io and GitHub releases.
