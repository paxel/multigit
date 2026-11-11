//! What a command line or button runs: git arguments, or a shell line when it
//! starts with `!`.

use std::ffi::OsString;

/// A command ready for the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    /// Arguments after `git`.
    Git(Vec<String>),
    /// A line for `<shell> -c`.
    Shell(String),
}

/// The text after a leading `!`, trimmed; `None` for a git line.
pub fn shell_line(line: &str) -> Option<&str> {
    line.trim_start().strip_prefix('!').map(str::trim)
}

/// How the output panel names a command: `git status`, or the shell line as typed.
pub fn display(command: &str) -> String {
    if shell_line(command).is_some() {
        command.trim().to_owned()
    } else {
        format!("git {command}")
    }
}

/// The shell for `!` lines: the login shell from `$SHELL`, `sh` when unset or empty.
pub fn shell_program(env_shell: Option<OsString>) -> OsString {
    env_shell
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| OsString::from("sh"))
}

/// Short shell name for the UI (`/usr/bin/fish` → `fish`).
pub fn shell_name(program: &OsString) -> String {
    std::path::Path::new(program)
        .file_name()
        .map_or_else(|| program.to_string_lossy(), |name| name.to_string_lossy())
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bang_lines_are_shell_lines() {
        assert_eq!(shell_line("!ls -la"), Some("ls -la"));
        assert_eq!(shell_line("  ! make test "), Some("make test"));
        assert_eq!(shell_line("!"), Some(""));
        assert_eq!(shell_line("status"), None);
        assert_eq!(shell_line("log --grep=!x"), None);
    }

    #[test]
    fn display_names_git_and_shell_commands() {
        assert_eq!(display("status -s"), "git status -s");
        assert_eq!(display("!ls | wc -l"), "!ls | wc -l");
    }

    #[test]
    fn shell_comes_from_env_with_sh_fallback() {
        assert_eq!(
            shell_program(Some("/usr/bin/fish".into())),
            OsString::from("/usr/bin/fish")
        );
        assert_eq!(shell_program(Some("".into())), OsString::from("sh"));
        assert_eq!(shell_program(None), OsString::from("sh"));
        assert_eq!(shell_name(&"/usr/bin/fish".into()), "fish");
        assert_eq!(shell_name(&"sh".into()), "sh");
    }
}
