//! The Whitaker isolation checks fed canned `make -n` text through the injected runner, so no
//! process runs: an isolated command passes, and every way of breaking the isolation is refused.

use super::{
    config::{Pin, THREADS_FLAG},
    injected::{canned, ensure, linker_flag},
    make::{Host, MakeRunner, Target, development_problems},
    whitaker::whitaker_route_problems,
};

/// Renders a `lint` recipe: a compliant Clippy command beside the given Whitaker command line.
fn lint_with(host: Host, whitaker: &str) -> Result<String, String> {
    let linker = linker_flag(host);
    canned(format_args!(
        "RUSTFLAGS=\"${{RUSTFLAGS:+$RUSTFLAGS }}-D warnings {THREADS_FLAG}{linker}\" cargo clippy --all-targets\n{whitaker}\n"
    ))
}

/// A fake runner whose Whitaker command is isolated: it assigns only the warning policy.
fn isolated_whitaker_make(_target: Target<'_>, host: Host) -> Result<String, String> {
    lint_with(host, "RUSTFLAGS=\"-D warnings\" whitaker --all")
}

/// A fake runner whose Whitaker command carries the development flags.
fn contaminated_whitaker_make(_target: Target<'_>, host: Host) -> Result<String, String> {
    let flags = format!("-D warnings {THREADS_FLAG}{}", linker_flag(host));
    lint_with(host, &format!("RUSTFLAGS=\"{flags}\" whitaker --all"))
}

/// A fake runner whose Whitaker command inherits the caller's `RUSTFLAGS`.
fn inheriting_whitaker_make(_target: Target<'_>, host: Host) -> Result<String, String> {
    lint_with(
        host,
        "RUSTFLAGS=\"${RUSTFLAGS:+$RUSTFLAGS }-D warnings\" whitaker --all",
    )
}

/// A fake runner whose Whitaker command assigns an empty `RUSTFLAGS`, dropping the warning policy.
fn empty_whitaker_make(_target: Target<'_>, host: Host) -> Result<String, String> {
    lint_with(host, "RUSTFLAGS=\"\" whitaker --all")
}

/// A fake runner whose Whitaker command assigns nothing.
fn bare_whitaker_make(_target: Target<'_>, host: Host) -> Result<String, String> {
    lint_with(host, "whitaker --all")
}

/// A fake runner whose `lint` recipe runs no Whitaker command at all.
fn no_whitaker_make(_target: Target<'_>, host: Host) -> Result<String, String> {
    let linker = linker_flag(host);
    canned(format_args!(
        "RUSTFLAGS=\"${{RUSTFLAGS:+$RUSTFLAGS }}-D warnings {THREADS_FLAG}{linker}\" cargo clippy --all-targets\n"
    ))
}

/// A fake runner whose Cargo test filter names Whitaker, beside a compliant Whitaker command.
fn whitaker_test_filter_make(_target: Target<'_>, host: Host) -> Result<String, String> {
    let linker = linker_flag(host);
    canned(format_args!(
        "RUSTFLAGS=\"${{RUSTFLAGS:+$RUSTFLAGS }}-D warnings {THREADS_FLAG}{linker}\" cargo test whitaker\nRUSTFLAGS=\"-D warnings\" whitaker --all\n"
    ))
}

/// Scenario: a direct Whitaker command, whose installer-managed toolchain must not inherit the
/// repository's development flags (Concordat `rust-build-defaults` BD-008).
///
/// Invariant: an isolated command that assigns only the warning policy raises nothing; a command that
/// carries the frontend or linker flag, inherits the caller's flags, drops the warning policy or
/// assigns nothing is refused; a recipe with no Whitaker route is refused; and a Cargo test filter
/// that names Whitaker is still held to the standard's flags.
#[test]
fn a_direct_whitaker_command_must_stay_isolated_from_the_development_flags() -> Result<(), String> {
    let pin = Pin::Nightly;
    for host in [Host::Linux, Host::Darwin] {
        let (isolated, _) = development_problems(isolated_whitaker_make, host, pin)?;
        ensure(
            isolated.is_empty(),
            &format!("an isolated Whitaker command raised {isolated:?}"),
        )?;
        let route = whitaker_route_problems(isolated_whitaker_make, host)?;
        ensure(
            route.is_empty(),
            &format!("an isolated Whitaker route raised {route:?}"),
        )?;
        let (filtered, _) = development_problems(whitaker_test_filter_make, host, pin)?;
        ensure(
            filtered.is_empty(),
            &format!("a Cargo test filter naming Whitaker raised {filtered:?}"),
        )?;
        let missing = whitaker_route_problems(no_whitaker_make, host)?;
        ensure(!missing.is_empty(), "a lint recipe without Whitaker passed")?;
        let bare = whitaker_route_problems(bare_whitaker_make, host)?;
        ensure(
            !bare.is_empty(),
            "a bare Whitaker command passed the route check",
        )?;
    }
    let refused = [
        (
            contaminated_whitaker_make as MakeRunner,
            "development flags",
        ),
        (inheriting_whitaker_make, "inherit the caller's RUSTFLAGS"),
        (empty_whitaker_make, "without -D warnings"),
    ];
    for (runner, reason) in refused {
        let (problems, _) = development_problems(runner, Host::Linux, pin)?;
        ensure(
            problems.iter().any(|problem| problem.contains(reason)),
            &format!("a Whitaker command with `{reason}` passed: {problems:?}"),
        )?;
    }
    Ok(())
}
