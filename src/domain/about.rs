//! What the About page says.

/// Under the app's name.
pub const TAGLINE: &str = "A desktop dashboard for many git repositories";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const LICENSE: &str = "MIT license";
pub const REPOSITORY_URL: &str = "https://github.com/paxel/multigit";
pub const ISSUES_URL: &str = "https://github.com/paxel/multigit/issues";
pub const DEVELOPER_MAIL: &str = "taum@tuta.io";
pub const DONATE_URL: &str = "https://ko-fi.com/paxel7";

/// The crates in the binary, one per line: name and version, a tab, the
/// licence. Written by `tool/licenses.sh`.
const CRATE_LICENSES: &str = include_str!("../../assets/licenses.txt");

/// A row of the About page that opens something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub title: &'static str,
    pub subtitle: &'static str,
    pub url: &'static str,
}

/// The rows that open a web page or a mail.
pub const LINKS: [Link; 4] = [
    Link {
        title: "Source code",
        subtitle: "https://github.com/paxel/multigit \u{2014} MIT license",
        url: REPOSITORY_URL,
    },
    Link {
        title: "Report a problem or idea",
        subtitle: "GitHub issues",
        url: ISSUES_URL,
    },
    Link {
        title: "Write the developer",
        subtitle: DEVELOPER_MAIL,
        url: "mailto:taum@tuta.io?subject=multigit%20feedback",
    },
    Link {
        title: "Buy the developer a coffee",
        subtitle: "The app stays free. Even if I don't get a coffee :)",
        url: DONATE_URL,
    },
];

/// A crate in the binary and its licence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrateLicense {
    /// Name and version.
    pub krate: &'static str,
    pub license: &'static str,
}

/// Every crate in the binary with its licence, by name.
pub fn crate_licenses() -> Vec<CrateLicense> {
    parse_licenses(CRATE_LICENSES)
}

fn parse_licenses(text: &'static str) -> Vec<CrateLicense> {
    text.lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(krate, license)| CrateLicense { krate, license })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_point_at_the_project_and_its_developer() {
        assert_eq!(LINKS.len(), 4);
        assert!(LINKS[0].subtitle.starts_with(REPOSITORY_URL));
        assert!(LINKS[0].subtitle.ends_with(LICENSE));
        assert!(ISSUES_URL.starts_with(REPOSITORY_URL));
        assert!(
            LINKS[2]
                .url
                .starts_with(&format!("mailto:{DEVELOPER_MAIL}"))
        );
        assert_eq!(LINKS[3].url, DONATE_URL);
        assert!(!TAGLINE.is_empty() && VERSION.contains('.'));
    }

    #[test]
    fn crate_licenses_are_read_from_the_generated_list() {
        let parsed = parse_licenses("a 1.0\tMIT\nbroken line\nb 2.1\tMIT OR Apache-2.0\n");
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            (parsed[1].krate, parsed[1].license),
            ("b 2.1", "MIT OR Apache-2.0")
        );
        let all = crate_licenses();
        assert!(all.len() > 100, "{}", all.len());
        assert!(all.iter().any(|c| c.krate.starts_with("egui ")));
        assert!(
            all.iter()
                .all(|c| !c.license.is_empty() && c.krate.contains(' '))
        );
    }
}
