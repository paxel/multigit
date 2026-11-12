//! User configuration stored in `~/.config/multigit/config.toml`.

use crate::domain::favorites::Favorite;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A card button: label shown on the button, git arguments it runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ButtonDef {
    pub label: String,
    pub args: String,
}

impl ButtonDef {
    pub fn new(label: &str, args: &str) -> Self {
        Self {
            label: label.to_owned(),
            args: args.to_owned(),
        }
    }
}

/// Buttons every new configuration starts with.
pub fn default_buttons() -> Vec<ButtonDef> {
    vec![
        ButtonDef::new("pull --ff-only", "pull --ff-only"),
        ButtonDef::new("pull --rebase", "pull --rebase"),
        ButtonDef::new("push", "push"),
        ButtonDef::new("fetch --all --prune", "fetch --all --prune"),
    ]
}

/// Presets offered in the button editor's dropdown.
pub fn button_presets() -> Vec<ButtonDef> {
    let mut presets = default_buttons();
    presets.extend([
        ButtonDef::new("fetch", "fetch"),
        ButtonDef::new("status", "status"),
        ButtonDef::new("stash", "stash"),
        ButtonDef::new("stash pop", "stash pop"),
        ButtonDef::new("log", "log -5 --oneline"),
        ButtonDef::new("switch -", "switch -"),
    ]);
    presets
}

/// The look of the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Light or dark, as the system says.
    #[default]
    System,
    Light,
    Dark,
    /// Round, pastel and animated.
    Kawaii,
}

impl Theme {
    /// Every theme, in the order Settings offers them.
    pub const ALL: [Self; 4] = [Self::System, Self::Light, Self::Dark, Self::Kawaii];

    /// As Settings names it.
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::Kawaii => "Kawaii",
        }
    }
}

fn default_fetch_interval() -> u64 {
    60
}
fn default_concurrency() -> usize {
    4
}
fn default_scan_depth() -> usize {
    2
}
fn default_true() -> bool {
    true
}

/// Everything the app persists in its config file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// Seconds between automatic `git fetch --all --prune` runs.
    #[serde(default = "default_fetch_interval")]
    pub fetch_interval_secs: u64,
    /// Maximum parallel git processes for commands (and, separately, for polling).
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    /// How many folder levels (+) scans below a parent folder.
    #[serde(default = "default_scan_depth")]
    pub scan_depth: usize,
    /// Tab with exactly one completion inserts it directly.
    #[serde(default = "default_true")]
    pub auto_insert_single_match: bool,
    #[serde(default)]
    pub theme: Theme,
    /// Command that opens a terminal in a repository; `{dir}` stands for the
    /// folder. Empty: the app finds a terminal.
    #[serde(default)]
    pub terminal: String,
    #[serde(default)]
    pub repos: Vec<PathBuf>,
    /// Repositories taken off the board that a later scan of their parent
    /// folder must not bring back.
    #[serde(default)]
    pub hidden: Vec<PathBuf>,
    #[serde(default = "default_buttons")]
    pub buttons: Vec<ButtonDef>,
    /// In the order they were made, which is the order of their buttons.
    #[serde(default)]
    pub favorites: Vec<Favorite>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fetch_interval_secs: default_fetch_interval(),
            concurrency: default_concurrency(),
            scan_depth: default_scan_depth(),
            auto_insert_single_match: true,
            theme: Theme::default(),
            terminal: String::new(),
            repos: Vec::new(),
            hidden: Vec::new(),
            buttons: default_buttons(),
            favorites: Vec::new(),
        }
    }
}

impl Config {
    /// Parse a config file's content; missing keys take their defaults.
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Serialize for writing back to disk.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Concurrency clamped to at least one process.
    pub fn effective_concurrency(&self) -> usize {
        self.concurrency.max(1)
    }

    /// Fetch interval clamped to at least ten seconds.
    pub fn effective_fetch_interval_secs(&self) -> u64 {
        self.fetch_interval_secs.max(10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favorites_survive_the_config_file() {
        let mut config = Config::default();
        config.favorites.push(Favorite {
            name: "ETL".into(),
            color: "teal".into(),
            emoji: Some("\u{1f680}".into()),
            repos: vec!["/x/etl-load".into()],
        });
        config.favorites.push(Favorite {
            name: "empty".into(),
            color: "red".into(),
            emoji: None,
            repos: vec![],
        });
        let text = config.to_toml().unwrap();
        assert!(text.contains("[[favorites]]"), "{text}");
        assert_eq!(Config::from_toml(&text).unwrap(), config);
        // Written by hand with the least there is.
        let by_hand = Config::from_toml("[[favorites]]\nname = \"a\"\ncolor = \"blue\"").unwrap();
        assert_eq!(by_hand.favorites[0].repos, Vec::<PathBuf>::new());
        assert_eq!(by_hand.favorites[0].emoji, None);
    }

    #[test]
    fn hidden_repositories_survive_the_config_file() {
        let mut config = Config::default();
        config.repos.push("/x/shown".into());
        config.hidden.push("/x/hidden".into());
        config.favorites.push(Favorite {
            name: "a".into(),
            color: "red".into(),
            emoji: None,
            repos: vec!["/x/hidden".into()],
        });
        let text = config.to_toml().unwrap();
        assert_eq!(Config::from_toml(&text).unwrap(), config);
        assert!(Config::from_toml("").unwrap().hidden.is_empty());
    }

    #[test]
    fn theme_is_stored_as_a_lowercase_word() {
        let mut config = Config::from_toml("theme = \"dark\"").unwrap();
        assert_eq!(config.theme, Theme::Dark);
        config.theme = Theme::Light;
        let text = config.to_toml().unwrap();
        assert!(text.contains("theme = \"light\""), "{text}");
        assert_eq!(Config::from_toml(&text).unwrap(), config);
        assert!(Config::from_toml("theme = \"pink\"").is_err());
        assert_eq!(
            Theme::ALL.map(Theme::label),
            ["System", "Light", "Dark", "Kawaii"]
        );
        assert_eq!(
            Config::from_toml("theme = \"kawaii\"").unwrap().theme,
            Theme::Kawaii
        );
    }

    #[test]
    fn empty_file_gives_defaults() {
        let config = Config::from_toml("").unwrap();
        assert_eq!(config, Config::default());
        assert_eq!(config.fetch_interval_secs, 60);
        assert_eq!(config.concurrency, 4);
        assert_eq!(config.scan_depth, 2);
        assert!(config.auto_insert_single_match);
        assert_eq!(config.theme, Theme::System);
        assert_eq!(config.terminal, "");
        assert_eq!(
            config
                .buttons
                .iter()
                .map(|b| b.args.as_str())
                .collect::<Vec<_>>(),
            vec![
                "pull --ff-only",
                "pull --rebase",
                "push",
                "fetch --all --prune"
            ]
        );
    }

    #[test]
    fn round_trip() {
        let mut config = Config {
            repos: vec![PathBuf::from("/home/u/dev/a"), PathBuf::from("/x y/b")],
            concurrency: 7,
            ..Config::default()
        };
        config
            .buttons
            .push(ButtonDef::new("log", "log -5 --oneline"));
        let text = config.to_toml().unwrap();
        assert_eq!(Config::from_toml(&text).unwrap(), config);
    }

    #[test]
    fn hand_written_partial_file() {
        let text = r#"
            concurrency = 2
            repos = ["/a"]
            [[buttons]]
            label = "st"
            args = "status -sb"
        "#;
        let config = Config::from_toml(text).unwrap();
        assert_eq!(config.concurrency, 2);
        assert_eq!(config.buttons, vec![ButtonDef::new("st", "status -sb")]);
        assert_eq!(config.fetch_interval_secs, 60);
    }

    #[test]
    fn invalid_file_is_an_error() {
        assert!(Config::from_toml("concurrency = \"many\"").is_err());
    }

    #[test]
    fn clamps() {
        let config = Config {
            concurrency: 0,
            fetch_interval_secs: 1,
            ..Config::default()
        };
        assert_eq!(config.effective_concurrency(), 1);
        assert_eq!(config.effective_fetch_interval_secs(), 10);
    }
}
