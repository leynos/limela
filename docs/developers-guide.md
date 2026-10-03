# Developers guide

This guide records the local development baseline for contributors working on
Limela.

## Spelling policy

Run `make spelling` to enforce en-GB-oxendict prose spelling. The gate
regenerates `typos.toml` from the live shared dictionary and the
`typos.local.toml` overlay on every run, so `typos.toml` is never drift checked
in CI. Put narrow repository-specific exceptions in `typos.local.toml`; never
edit generated entries by hand.

`TYPOS_CONFIG_BUILDER_VERSION` in the `Makefile` pins the
`typos-config-builder` release the gate runs (currently `v0.1.3`). Raise it
together with the regenerated `typos.toml`, never on its own. The builder
requires Python 3.14 or newer, so the target passes `--python 3.14` and `uv`
fetches that interpreter when the host lacks one.

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
