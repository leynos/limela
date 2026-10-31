//! The Whitaker half of the Makefile reader. Whitaker runs on its installer-managed toolchain, which
//! must not inherit the repository's development flags, so a direct Whitaker command is held to the
//! opposite of the standard: it assigns the warning policy alone.

use super::{
    config::{Flags, Problems},
    make::{Assignment, Host, MakeRunner, make_commands},
};

/// Returns whether a word is a leading `NAME=value` environment assignment.
fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// Returns whether a command runs Whitaker directly: the executable is `whitaker`, past any
/// leading environment assignments, so a Cargo test filter that merely names it does not count.
///
/// ```text
/// runs_whitaker("RUSTFLAGS=\"-D warnings\" whitaker --all") == true
/// runs_whitaker("RUSTFLAGS=\"-D warnings\" cargo test whitaker") == false
/// ```
pub(super) fn runs_whitaker(command: &str) -> bool {
    // A quoted value may hold spaces (`RUSTFLAGS="-D warnings"`), so drop each quoted span first.
    let mut unquoted = String::new();
    let mut quote = false;
    for c in command.chars() {
        match (quote, c) {
            (_, '"') => quote = !quote,
            (false, _) => unquoted.push(c),
            (true, _) => {}
        }
    }
    unquoted
        .split_whitespace()
        .find(|word| !is_assignment(word))
        .is_some_and(|word| word.rsplit('/').next() == Some("whitaker"))
}

/// Returns the complaint about a direct Whitaker command, if any: it must assign the warning
/// policy and nothing of the caller's `RUSTFLAGS`, and must carry neither the frontend flag nor the
/// linker flag.
pub(super) fn isolated_problem(
    target: &str,
    host: Host,
    flags: &Flags,
    inherits: bool,
) -> Option<String> {
    let on = host.make_value();
    if inherits {
        return Some(format!(
            "`make {target}` on {on} lets Whitaker inherit the caller's RUSTFLAGS"
        ));
    }
    if flags.names_threads() || flags.names_linker() {
        return Some(format!(
            "`make {target}` on {on} gives Whitaker the development flags: {flags:?}"
        ));
    }
    (!flags.denies_warnings())
        .then(|| format!("`make {target}` on {on} runs Whitaker without -D warnings"))
}

/// Returns the complaint when the `lint` target runs no isolated Whitaker command: the suite is the
/// lint gate's second half, and a bare, inheriting or missing Whitaker route must not pass for it.
///
/// # Errors
///
/// Returns the reason when the `lint` target is not defined or unreadable.
pub fn whitaker_route_problems(runner: MakeRunner, host: Host) -> Result<Problems, String> {
    let commands = make_commands(runner, "lint", host)?;
    let isolated = commands
        .iter()
        .any(|command| matches!(command, Assignment::Isolated(..)));
    let mut problems = Problems::new();
    if !isolated {
        problems.push(format!(
            "`make lint` on {} runs no Whitaker command that assigns RUSTFLAGS",
            host.make_value()
        ));
    }
    Ok(problems)
}
