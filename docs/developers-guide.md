# Developers guide

This guide records the local development baseline for contributors working on
Limela.

## Spelling policy

Run `make spelling` to enforce en-GB-oxendict prose spelling. The generated
`typos.toml` starts from the shared estate dictionary, refreshes its untracked
local cache only when the authority is newer, and then applies the narrow
repository policy in `typos.local.toml`. Edit the local policy and regenerate
the configuration rather than changing generated entries by hand.

## Rust baseline

Limela targets Rust Edition 2024 and declares a minimum supported Rust version
(MSRV) of 1.87 in `Cargo.toml`. Keep the README prerequisite and package
metadata aligned whenever the MSRV changes.

The repository pins the active toolchain in `rust-toolchain.toml`:

```toml
[toolchain]
channel = "nightly-2026-05-28"
components = ["rustfmt", "clippy", "rust-analyzer"]
```

Use this pinned nightly toolchain for local development, formatting, linting,
and editor integration.

## Build and quality targets

Prefer the Makefile targets over running Cargo directly. The project quality
gate is:

```bash
make check-fmt
make lint
make typecheck
make test
```

The `typecheck` target runs `cargo check` for all targets and all features with
warnings denied through `RUSTFLAGS="-D warnings"`. Run it before committing
changes alongside formatting, linting, and tests.
