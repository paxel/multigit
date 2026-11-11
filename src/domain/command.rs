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

/// Stands for the repository's folder in a terminal command.
pub const DIR_PLACEHOLDER: &str = "{dir}";

/// Terminals tried in this order when none is configured, each started in
/// the repository's folder; the ones that need it get the folder as an
/// argument too.
const KNOWN_TERMINALS: &[&[&str]] = &[
    &["xdg-terminal-exec"],
    &["konsole", "--workdir", DIR_PLACEHOLDER],
    &["gnome-terminal", "--working-directory={dir}"],
    &["xfce4-terminal", "--working-directory={dir}"],
    &["kitty", "--directory", DIR_PLACEHOLDER],
    &["alacritty", "--working-directory", DIR_PLACEHOLDER],
    &["foot", "--working-directory={dir}"],
    &["wezterm", "start", "--cwd", DIR_PLACEHOLDER],
    &["xterm"],
];

/// The command lines to try, in order, to open a terminal in `dir`: the
/// first whose program exists is the one. `configured` is the command from
/// the settings and the only one tried when it is not blank; otherwise
/// Terminal.app on macOS, and elsewhere `$TERMINAL` and the known terminals.
/// `{dir}` in an argument stands for the folder.
pub fn terminal_commands(
    configured: &str,
    env_terminal: Option<&str>,
    macos: bool,
    dir: &str,
) -> Result<Vec<Vec<String>>, String> {
    let fill = |args: Vec<String>| -> Vec<String> {
        args.iter()
            .map(|a| a.replace(DIR_PLACEHOLDER, dir))
            .collect()
    };
    let split = |line: &str| {
        shell_words::split(line).map_err(|e| format!("Cannot parse terminal command: {e}"))
    };
    if !configured.trim().is_empty() {
        return Ok(vec![fill(split(configured)?)]);
    }
    if macos {
        let open = ["open", "-a", "Terminal", DIR_PLACEHOLDER];
        return Ok(vec![fill(open.map(str::to_owned).to_vec())]);
    }
    let mut commands = Vec::new();
    if let Some(terminal) = env_terminal.filter(|t| !t.trim().is_empty()) {
        commands.push(fill(split(terminal)?));
    }
    for known in KNOWN_TERMINALS {
        commands.push(fill(known.iter().map(|s| (*s).to_owned()).collect()));
    }
    commands.retain(|c| !c.is_empty());
    Ok(commands)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_terminal_is_the_only_one_tried() {
        let commands =
            terminal_commands(" foot -D '{dir}' ", Some("kitty"), false, "/my repo").unwrap();
        assert_eq!(commands, vec![vec!["foot", "-D", "/my repo"]]);
        assert!(terminal_commands("foot 'x", None, false, "/r").is_err());
    }

    #[test]
    fn without_a_setting_the_terminal_is_detected() {
        let mac = terminal_commands("", Some("kitty"), true, "/r").unwrap();
        assert_eq!(mac, vec![vec!["open", "-a", "Terminal", "/r"]]);

        let linux = terminal_commands("  ", Some("my-term --here"), false, "/r").unwrap();
        assert_eq!(linux[0], ["my-term", "--here"]);
        assert_eq!(linux[1], ["xdg-terminal-exec"]);
        assert!(linux.contains(&vec!["konsole".into(), "--workdir".into(), "/r".into()]));
        assert!(linux.contains(&vec![
            "gnome-terminal".to_owned(),
            "--working-directory=/r".to_owned()
        ]));
        assert_eq!(linux.last().unwrap(), &["xterm"]);

        let none = terminal_commands("", None, false, "/r").unwrap();
        assert_eq!(none[0], ["xdg-terminal-exec"]);
        assert_eq!(none.len(), linux.len() - 1);
        assert!(terminal_commands("", Some(" "), false, "/r").unwrap().len() == none.len());
    }

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
