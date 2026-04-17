# Security policy

## Reporting a vulnerability

Please email security reports to the maintainer listed in `Cargo.toml`. Do not open a public issue for security matters.

Expect an acknowledgement within 72 hours.

## Supported versions

During pre-1.0 development, only the latest published release is supported.

## Scope

SpecERE writes files to user repositories and shells out to upstream tooling (notably `uvx` for `specere add speckit`). In-scope:

- Path traversal through unit flags.
- Command injection via forwarded shell arguments.
- Manifest tampering that would let `remove` delete unintended files.
- Marker-block parser exploits.

Out of scope: vulnerabilities in third-party tools SpecERE scaffolds (report those upstream).
