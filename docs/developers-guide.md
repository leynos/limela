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

Limela targets Rust Edition 2024. It is an application, so it declares no
minimum supported Rust version: every build uses the nightly pinned below.

The repository pins the active toolchain in `rust-toolchain.toml`:

```toml
[toolchain]
channel = "nightly-2026-11-13"
components = [
  "clippy",
  "llvm-tools-preview",
  "rust-analyzer",
  "rustc-codegen-cranelift-preview",
  "rustfmt",
]
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
`setup-rust` exports one). The `lint` recipe's Whitaker command is the one
deliberate exception among the gate recipes: Whitaker runs on its
installer-managed toolchain, which must not inherit the repository's
development flags, so it assigns `RUSTFLAGS="-D warnings"` alone, keeps none of
the caller's flags and takes neither `-Zthreads=8` nor `mold`, while Clippy in
the same recipe keeps the standard's flags. Two builds are also deliberately
excluded: coverage assigns `RUSTFLAGS` without the fast flags, because a
measurement should not depend on them, and the release recipe and workflow keep
the platform linker, because they assign `RUSTFLAGS` (even an empty value
displaces the configuration). Cargo has no per-profile `rustflags`, so a direct
`cargo build --release` takes the configuration's flags unless `RUSTFLAGS` is
assigned too.

On Linux, install `mold` before building: the configuration names it, so a
build without it fails at link time. CI installs it through `setup-rust`'s
`install-mold` input. `tests/build_standard_contract.rs` holds the standard. It
reads the configuration sources, the commands `make -n` prints for each
development target on a Linux host and a macOS host (each keeping the caller's
own `RUSTFLAGS`) and for each coverage and release target on a Linux host, and
the `setup-rust` steps of the CI workflows (each must pass `install-mold`), so
a flag lost through a recipe or workflow edit fails there. For Whitaker the
contract runs the other way: the `lint` target must run a Whitaker command that
assigns the warning policy, inherits nothing and names neither flag, and a
Whitaker command that carries a development flag, inherits the caller's flags,
drops `-D warnings` or assigns nothing fails.

Cognitive complexity is not gated by Clippy. Clippy's `cognitive_complexity`
scores code after macro expansion, so it counts branching a macro generated,
and neither `Cargo.toml` nor `clippy.toml` enables it. Whitaker's experimental
`macro_aware_cognitive_complexity` gates it at `max_complexity = 9`,
CodeScene's ceiling. `dylint.toml` opts in with
`[whitaker] experimental = ["macro_aware_cognitive_complexity"]`. Fix a finding
by extracting named helpers, never by raising the threshold.

### Cranelift

The estate standard is LLVM, so LLVM is the default backend here. The Cranelift
selection in `.cargo/config.toml` is commented out; the
`[unstable] codegen-backend` key and the `rustc-codegen-cranelift-preview`
component stay for an opt-in. See the
[concordat decision record](https://github.com/leynos/concordat/blob/main/docs/adrs/adr-0004-cranelift-is-not-part-of-the-rust-build-standard.md),
with review due 14 May 2027. An inherited `CARGO_PROFILE_*_CODEGEN_BACKEND`
variable can still select Cranelift, and Cranelift does not run destructors
while unwinding, so under an opt-in no test may rely on a caught panic
releasing a guard. The release workflow builds on the pinned nightly. The
`lint` recipe runs Whitaker with the LLVM backend, since its installer-managed
toolchain does not take a Cranelift opt-in, and coverage selects LLVM too,
since Cranelift cannot instrument coverage.

## CodeScene coverage and the CV-005 contract

Main owns coverage publication (estate rule CV-005). A pull request measures
coverage for its own ratchet check only, in the `build-test` job of `ci.yml`
(`with-ratchet: 'true'`, `publish-artefact: 'false'`); it never contacts
CodeScene. `.github/workflows/coverage-main.yml` runs on every push to `main`
in the `codescene` environment, generates coverage again and uploads it with
`mode: upload`, guarded on `github.ref == 'refs/heads/main'` alone. The token
reaches the uploader only through its `access-token` input, never an `env`.
When the repository has no `CS_ACCESS_TOKEN` (no CodeScene project yet) the
uploader records a `CodeScene upload skipped` notice and a step-summary line
and the run still succeeds.

`make test-workflow-contracts` checks that shape. It runs the shared
`cv005-contracts check --repository .` from the commit named by
`CV005_CONTRACTS_REF` in the `Makefile` (a bump of that pin is a reviewed
change, never a silent upgrade), then the local wiring tests in
`tests/workflow_contracts`. It needs `uv`, Python 3.13 for the checker, and
`pytest` and `pyyaml`, which `uv run --with` supplies. Both runs go through
`UV` and `UV_ENV`, so a `uv` outside `PATH` can be injected. `make all` and CI
run the target; `.github/cv005.toml` holds the repository name and nothing
else. A Dependabot pull request merged by the automerge workflow fires no push
event, so that merge publishes nothing until the next push to `main`
(shared-actions #518 tracks the fix).

## YAML lint

`make lint` runs `make yamllint` and `make actionlint` as prerequisites.
`make yamllint` runs `yamllint --strict` over every tracked `.yml` and `.yaml`
file, reading `.yamllint.yml`, which extends the estate base
`.yamllint-estate.yml` (a verbatim copy of the estate canon; do not edit it).
The base ignores Helm chart templates (`**/charts/**/templates/`), so a tracked
file there is passed to yamllint but not checked, and a local `ignore` in
`.yamllint.yml` replaces that entry rather than adding to it. Run it from the
repository root, because yamllint resolves `extends` against the current
directory. `make actionlint` checks the GitHub Actions workflows, accepting the
runner labels in `.github/actionlint.yaml`, and runs ShellCheck over their Bash
and `sh` `run:` scripts when it is on `PATH` (it skips PowerShell steps).

Install the tools locally at the versions CI uses:
`uv tool install yamllint==1.38.0`, and the `actionlint` 1.7.12 and
`shellcheck` 0.11.0 release binaries (a package manager's builds can differ and
change what the lint reports). CI installs the same pinned versions with the
`install-yaml-lint` action of `leynos/shared-actions` before it runs
`make lint`. The `YAMLLINT` and `ACTIONLINT` variables name the commands when
they are not on `PATH`.
