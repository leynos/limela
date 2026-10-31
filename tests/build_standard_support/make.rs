//! Readers for the Makefile half of the build standard: the commands `make -n`
//! prints for each development, coverage and release target, judged against a
//! toolchain pin and a host.

use std::process::Command;

use super::{
    config::{Flags, LINKER_FLAG, Pin, Problems, THREADS_FLAG},
    shell::{compiles, shell_commands, without_leading_keywords},
};

/// Makefile targets that build for development. A command in one either assigns
/// `RUSTFLAGS` with the standard flags or assigns none and so takes the
/// configuration's. The list is this repository's own, and a target that stops
/// being defined fails the contract rather than dropping out of it.
const DEVELOPMENT_TARGETS: &[&str] = &["test", "typecheck", "lint", "build"];
/// Makefile targets that measure or ship, so every command assigns `RUSTFLAGS`
/// and none carries a standard flag.
const HELD_OUT_TARGETS: &[&str] = &["release"];

/// The host `make` is told it runs on, through `BUILD_HOST_OS`.
#[derive(Clone, Copy)]
pub enum Host {
    Linux,
    Darwin,
}

impl Host {
    /// Returns the value `uname -s` reports for the host.
    const fn make_value(self) -> &'static str {
        match self {
            Self::Linux => "Linux",
            Self::Darwin => "Darwin",
        }
    }

    /// Returns whether the host takes mold, which ships for Linux alone.
    pub const fn takes_linker_flag(self) -> bool {
        matches!(self, Self::Linux)
    }
}

/// What one `make -n` command assigns to `RUSTFLAGS`.
#[derive(Debug, PartialEq, Eq)]
pub enum Assignment {
    /// A command that assigns nothing and takes the configuration's flags, and runs no
    /// build, test or lint tool (a formatter, a metadata probe, a documentation build).
    Unassigned,
    /// A build, test or lint command that assigns nothing. Development recipes must
    /// assign `RUSTFLAGS` there, so the caller's flags and the warning policy reach it.
    Bare(String),
    /// An assignment, and whether it keeps the caller's own `RUSTFLAGS`.
    Flags(Flags, bool),
    /// An assignment on a direct Whitaker command, and whether it keeps the caller's own
    /// `RUSTFLAGS`. Whitaker runs on its installer-managed toolchain, which must not inherit the
    /// development flags, so it assigns the warning policy alone, keeps nothing of the caller's
    /// flags, and the standard's flags are refused there instead of required.
    Isolated(Flags, bool),
}

/// Reads the `RUSTFLAGS` a `make -n` output line assigns. An unreadable form is
/// an error, because it still replaces the configuration's sources and so must
/// not pass.
///
/// ```text
/// assigned_rustflags("RUSTFLAGS=\"-Zthreads=8\" cargo test") -> Flags(["-Zthreads=8"], inherits: false)
/// assigned_rustflags("RUSTFLAGS=\"${RUSTFLAGS:+$RUSTFLAGS }-Zthreads=8\" cargo test") -> inherits: true
/// assigned_rustflags("cargo test")                           -> Unassigned
/// assigned_rustflags("RUSTFLAGS=-Zthreads=8 cargo test")     -> Err
/// assigned_rustflags("RUSTFLAGS=\"${RUSTFLAGS-}-Zthreads=8\" cargo test") -> Err (glued)
/// ```
///
/// # Errors
///
/// Returns the reason when an assignment is unquoted, unterminated, or glues the
/// caller's flags to the next one.
pub fn assigned_rustflags(line: &str) -> Result<Assignment, String> {
    let Some((_, rest)) = line.split_once("RUSTFLAGS=\"") else {
        if line.contains("RUSTFLAGS=") {
            return Err(format!("unreadable RUSTFLAGS assignment in `{line}`"));
        }
        return Ok(Assignment::Unassigned);
    };
    let (assigned, _) = rest
        .split_once('"')
        .ok_or_else(|| format!("unterminated RUSTFLAGS in `{line}`"))?;
    // The recipes prepend the caller's own flags with these expansions; they are
    // not standard flags. `${RUSTFLAGS-}` adds no separator, so glued to the next
    // word it makes one token with it (`-Dwarnings-Zthreads=8`) and hides the flag.
    let glued = assigned
        .split("${RUSTFLAGS-}")
        .skip(1)
        .any(|after| !after.is_empty() && !after.starts_with(' '));
    if glued {
        return Err(format!(
            "inherited RUSTFLAGS glued to the next flag in `{line}`"
        ));
    }
    let inherits =
        assigned.contains("${RUSTFLAGS:+$RUSTFLAGS }") || assigned.contains("${RUSTFLAGS-}");
    let own = assigned
        .replace("${RUSTFLAGS:+$RUSTFLAGS }", " ")
        .replace("${RUSTFLAGS-}", " ");
    Ok(Assignment::Flags(
        Flags::from_words(own.split_whitespace()),
        inherits,
    ))
}

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
fn runs_whitaker(command: &str) -> bool {
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

/// Reads the assignment of each cargo or whitaker command `make -n` printed. A line that chains
/// commands is read one command at a time.
///
/// # Errors
///
/// Returns the reason when a command assigns `RUSTFLAGS` in an unreadable form.
pub fn commands_from(stdout: &str) -> Result<Vec<Assignment>, String> {
    // A recipe continued with a trailing backslash is one logical line.
    let joined = stdout.replace("\\\n", " ");
    joined
        .lines()
        .flat_map(shell_commands)
        .map(|command| without_leading_keywords(&command).to_owned())
        .filter(|command| !command.starts_with("echo"))
        // A tool-availability probe names Cargo but runs no build.
        .filter(|command| !command.starts_with("command -v"))
        .filter(|command| command.contains("cargo") || command.contains("whitaker"))
        .map(|command| match assigned_rustflags(&command)? {
            Assignment::Unassigned if compiles(&command) => Ok(Assignment::Bare(command)),
            Assignment::Flags(flags, inherits) if runs_whitaker(&command) => {
                Ok(Assignment::Isolated(flags, inherits))
            }
            other => Ok(other),
        })
        .collect()
}

/// A Makefile target name, typed so a runner takes a target and not just any text.
#[derive(Clone, Copy, Debug)]
pub struct Target<'a>(pub &'a str);

impl<'a> Target<'a> {
    /// Returns the target's name.
    pub const fn name(self) -> &'a str {
        self.0
    }
}

/// Runs `make -n` for a target on a host and returns what it printed. The tests
/// that drive real `make` use [`real_make`]; a test of the parsing path passes a
/// function that returns canned text instead, so no process runs.
pub type MakeRunner = fn(Target<'_>, Host) -> Result<String, String>;

/// The integration adapter: runs the real `make -n` in the crate's directory and
/// reports a spawn failure or an undefined target as an error.
///
/// # Errors
///
/// Returns the reason when `make` cannot run or the target is not defined.
pub fn real_make(target: Target<'_>, host: Host) -> Result<String, String> {
    let name = target.name();
    let output = Command::new("make")
        .args([
            "-n",
            "-B",
            &format!("BUILD_HOST_OS={}", host.make_value()),
            name,
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .map_err(|error| format!("running make: {error}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!(
            "`make -n {name}` failed, so it is not defined: {stderr}"
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Reads the commands a runner reports for a target on a host.
fn make_commands(runner: MakeRunner, target: &str, host: Host) -> Result<Vec<Assignment>, String> {
    commands_from(&runner(Target(target), host)?)
}

/// Returns the complaint about a development command that assigns no `RUSTFLAGS`.
fn bare_problem(target: &str, host: Host, command: &str) -> String {
    format!(
        "`make {target}` on {} runs `{command}` without assigning RUSTFLAGS",
        host.make_value()
    )
}

/// Returns the complaint about a direct Whitaker command, if any: it must assign the warning
/// policy and nothing of the caller's `RUSTFLAGS`, and must carry neither the frontend flag nor the
/// linker flag.
fn isolated_problem(target: &str, host: Host, flags: &Flags, inherits: bool) -> Option<String> {
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

/// Returns the complaint about one development command, if any: an assigned
/// `RUSTFLAGS` keeps the caller's own flags and restates the frontend flag on a
/// nightly pin, and mold on Linux. A direct Whitaker command is the exception: its
/// installer-managed toolchain must not inherit the development flags.
pub fn development_problem(
    target: &str,
    host: Host,
    pin: Pin,
    assignment: &Assignment,
) -> Option<String> {
    let (flags, inherits) = match assignment {
        Assignment::Flags(flags, inherits) => (flags, inherits),
        Assignment::Bare(command) => return Some(bare_problem(target, host, command)),
        Assignment::Isolated(flags, inherits) => {
            return isolated_problem(target, host, flags, *inherits);
        }
        Assignment::Unassigned => return None,
    };
    if !inherits {
        return Some(format!(
            "`make {target}` on {} drops the caller's RUSTFLAGS",
            host.make_value()
        ));
    }
    let reason = flags.meets(pin, host.takes_linker_flag()).err()?;
    Some(format!("`make {target}` on {} {reason}", host.make_value()))
}

/// Returns the complaint when the `test` target assigns `RUSTFLAGS` in no command, or
/// keeps `-D warnings` in none of its assigned commands: a recipe may run other commands (a version probe, a
/// prerequisite build) that never carried the policy, but dropping `$(RUST_FLAGS)`
/// from the command that runs the tests drops it from all of them.
pub fn test_policy_problem(target: &str, host: Host, commands: &[Assignment]) -> Option<String> {
    let assigned = commands.iter().filter_map(|command| match command {
        Assignment::Flags(flags, _) => Some(flags),
        Assignment::Unassigned | Assignment::Bare(_) | Assignment::Isolated(..) => None,
    });
    let keeps_the_policy = assigned.clone().any(Flags::denies_warnings);
    (target == "test" && !keeps_the_policy).then(|| {
        format!(
            "`make {target}` on {} keeps -D warnings in none of its commands",
            host.make_value()
        )
    })
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

/// Returns every complaint about the development targets on one host, and how
/// many assignments it read, so a test can refuse to pass over nothing.
///
/// # Errors
///
/// Returns the reason when a listed target is not defined or unreadable.
pub fn development_problems(
    runner: MakeRunner,
    host: Host,
    pin: Pin,
) -> Result<(Problems, usize), String> {
    let mut problems = Vec::new();
    let mut read = 0;
    for target in DEVELOPMENT_TARGETS {
        let commands = make_commands(runner, target, host)?;
        read += commands
            .iter()
            .filter(|command| matches!(command, Assignment::Flags(..) | Assignment::Isolated(..)))
            .count();
        problems.extend(
            commands
                .iter()
                .filter_map(|command| development_problem(target, host, pin, command)),
        );
        problems.extend(test_policy_problem(target, host, &commands));
    }
    Ok((problems, read))
}

/// Returns every complaint about one held-out command: it assigns nothing, so
/// it takes the configuration's flags, or the assignment names a standard flag.
fn held_out_command_problems(target: &str, assignment: &Assignment) -> Problems {
    let (flags, inherits) = match assignment {
        Assignment::Flags(flags, inherits) => (flags, *inherits),
        Assignment::Isolated(flags, _) => (flags, true),
        Assignment::Bare(command) => {
            return vec![format!(
                "`make {target}` runs `{command}`, which takes the configuration's flags"
            )];
        }
        Assignment::Unassigned => return Vec::new(),
    };
    let named = [
        (flags.names_threads(), THREADS_FLAG),
        (flags.names_linker(), LINKER_FLAG),
    ];
    let mut problems: Problems = named
        .into_iter()
        .filter(|(is_named, _)| *is_named)
        .map(|(_, flag)| format!("`make {target}` takes {flag}"))
        .collect();
    // A release build keeps the caller's own flags (a sanitizer, a target feature) while it drops the
    // standard's; only the coverage build, a measurement, ignores them.
    if target == "release" && !inherits {
        problems.push(format!("`make {target}` drops the caller's RUSTFLAGS"));
    }
    problems
}

/// Returns every complaint about the held-out targets, and how many commands it
/// read: each assigns `RUSTFLAGS`, since only an assignment displaces the
/// configuration's sources.
///
/// # Errors
///
/// Returns the reason when a listed target is not defined or unreadable.
pub fn held_out_problems(runner: MakeRunner) -> Result<(Problems, usize), String> {
    held_out_problems_for(runner, HELD_OUT_TARGETS)
}

/// Returns the complaints about a given list of held-out targets, so a test can name a
/// synthetic target and exercise the check in a repository that defines none.
///
/// # Errors
///
/// Returns the reason when a named target is not defined or unreadable.
pub fn held_out_problems_for(
    runner: MakeRunner,
    targets: &[&str],
) -> Result<(Problems, usize), String> {
    let mut problems = Vec::new();
    let mut read = 0;
    for target in targets {
        let commands = make_commands(runner, target, Host::Linux)?;
        read += commands.len();
        problems.extend(
            commands
                .iter()
                .flat_map(|command| held_out_command_problems(target, command)),
        );
    }
    Ok((problems, read))
}

/// Returns the number of held-out targets the repository defines.
pub const fn held_out_target_count() -> usize {
    HELD_OUT_TARGETS.len()
}
