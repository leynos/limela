# Developers guide

This guide records the local development baseline for contributors working on
Limela.

## Spelling policy

Run `make spelling` to enforce en-GB-oxendict prose spelling. The gate
regenerates `typos.toml` from the live shared dictionary and the
`typos.local.toml` overlay on every run, so `typos.toml` is never drift checked
in CI. Put narrow repository-specific exceptions in `typos.local.toml`; never
edit generated entries by hand.

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

## The build standard

Development, test, lint, and typecheck builds use the parallel `rustc` frontend
(`-Zthreads=8`) and, on Linux, the `mold` linker (`-Clink-arg=-fuse-ld=mold`).
These are defaults in `.cargo/config.toml`, which Cargo discovers on its own,
so a bare `cargo build` gets them. `mold` ships for Linux only, so the linker
flag lives in a Linux-only table and macOS and Windows keep their platform
linker. Cargo selects one `rustflags` source rather than merging them, so every
source repeats the same flags apart from the linker.

An assigned `RUSTFLAGS` replaces the configuration's flags, so the Makefile
recipes that set it compose the standard's flags onto any inherited value (CI's
`setup-rust` exports one). Two builds are deliberately excluded: coverage
assigns `RUSTFLAGS` without the fast flags, because a measurement should not
depend on them, and the release recipe and workflow keep the platform linker,
because they assign `RUSTFLAGS` (even an empty value displaces the
configuration). Cargo has no per-profile `rustflags`, so a direct
`cargo build --release` takes the configuration's flags unless `RUSTFLAGS` is
assigned too.

On Linux, install `mold` before building: the configuration names it, so a
build without it fails at link time. CI installs it through `setup-rust`'s
`install-mold` input. `tests/build_standard_contract.rs` holds the standard. It
reads the configuration sources, the commands `make -n` prints for each
development target on a Linux host and a macOS host (each keeping the caller's
own `RUSTFLAGS`) and for each coverage and release target on a Linux host, and
the `setup-rust` steps of the CI workflows (each must pass `install-mold`), so
a flag lost through a recipe or workflow edit fails there.

### Cranelift

Exception: Cranelift is not the development-profile backend. The repository pins
`nightly-2026-05-28`, but the release workflow
(`cross +stable build --release`) builds on a stable toolchain against
`.cargo/config.toml`, and stable Cargo refuses a
`[profile.dev] codegen-backend` key ("config profile `dev` is not valid"), so
selecting the backend would break that build (recorded 2026-09-29). That build
assigns `RUSTFLAGS`, so the nightly-only `-Zthreads` flag never reaches it.
Revisit if that build moves to the pinned nightly.
