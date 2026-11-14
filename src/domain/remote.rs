//! The remotes of a repository, and how a card writes one.

/// A remote and where it fetches from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub name: String,
    /// Its URL as git uses it; empty when it has none.
    pub url: String,
}

/// The remotes in the output of `git remote -v`: a line per remote and
/// direction, `name<TAB>url (fetch)`. The URL is the one fetched from.
pub fn parse_remotes(output: &str) -> Vec<Remote> {
    let mut remotes: Vec<Remote> = Vec::new();
    for line in output.lines().filter(|line| !line.is_empty()) {
        let (name, rest) = line.split_once('\t').unwrap_or((line, ""));
        let fetch = rest.strip_suffix(" (fetch)");
        match remotes.iter_mut().find(|remote| remote.name == name) {
            Some(known) => {
                if let Some(url) = fetch {
                    known.url = url.to_owned();
                }
            }
            None => remotes.push(Remote {
                name: name.to_owned(),
                url: fetch.unwrap_or_default().to_owned(),
            }),
        }
    }
    remotes
}

/// The remote the remote branch `branch` (`origin/main`) is on. A remote's
/// name may hold a slash itself: the longest name that fits wins.
pub fn remote_of<'a>(remotes: &'a [Remote], branch: &str) -> Option<&'a Remote> {
    remotes
        .iter()
        .filter(|remote| {
            branch
                .strip_prefix(remote.name.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
        })
        .max_by_key(|remote| remote.name.len())
}

/// `host:path` of an address written like scp does, `git@host:path`;
/// `None` for a path in the file system (a drive letter is no host).
fn scp_like(url: &str) -> Option<(&str, &str)> {
    let (before, path) = url.split_once(':')?;
    if before.contains('/') || before.len() < 2 {
        return None;
    }
    let host = before.rsplit_once('@').map_or(before, |(_, host)| host);
    Some((host, path))
}

/// The end of a URL without what says nothing: a slash and `.git`.
fn trimmed(path: &str) -> &str {
    let path = path.trim_end_matches('/');
    path.strip_suffix(".git").unwrap_or(path)
}

/// `scheme`, and what follows `://` split into who logs in (with the `@`),
/// the host and the path.
fn parts(url: &str) -> Option<(&str, &str, &str, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    let slash = rest.find('/').unwrap_or(rest.len());
    let (authority, path) = rest.split_at(slash);
    let at = authority.rfind('@').map_or(0, |at| at + 1);
    let (user, host) = authority.split_at(at);
    Some((scheme, user, host, path))
}

/// How a card writes a remote's URL: the protocol first, then host and
/// path, as in `ssh github.com/paxel/multigit`. Who logs in, a password or
/// token in the URL and the `.git` at its end are left out.
pub fn short_url(url: &str) -> String {
    if let Some((scheme, _, host, path)) = parts(url) {
        return format!("{scheme} {host}{}", trimmed(path));
    }
    match scp_like(url) {
        Some((host, path)) => format!("ssh {host}/{}", trimmed(path.trim_start_matches('/'))),
        None => format!("file {}", trimmed(url)),
    }
}

/// The URL without a password or token in it, for showing it whole.
pub fn safe_url(url: &str) -> String {
    match parts(url) {
        Some((scheme, user, host, path)) if user.contains(':') => {
            format!("{scheme}://{host}{path}")
        }
        _ => url.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(name: &str, url: &str) -> Remote {
        Remote {
            name: name.into(),
            url: url.into(),
        }
    }

    #[test]
    fn remotes_are_read_with_the_url_they_fetch_from() {
        let out = "origin\tgit@github.com:paxel/multigit.git (fetch)\n\
                   origin\tgit@github.com:paxel/other.git (push)\n\
                   up/stream\thttps://example.org/x.git (fetch)\n\
                   up/stream\thttps://example.org/x.git (push)\n\
                   bare\n";
        assert_eq!(
            parse_remotes(out),
            [
                remote("origin", "git@github.com:paxel/multigit.git"),
                remote("up/stream", "https://example.org/x.git"),
                remote("bare", ""),
            ]
        );
        assert!(parse_remotes("").is_empty());
    }

    #[test]
    fn a_remote_branch_belongs_to_the_longest_remote_name_that_fits() {
        let remotes = [
            remote("up", "a"),
            remote("up/stream", "b"),
            remote("o", "c"),
        ];
        let name = |branch| remote_of(&remotes, branch).map(|r| r.name.as_str());
        assert_eq!(name("up/main"), Some("up"));
        assert_eq!(name("up/stream/main"), Some("up/stream"));
        assert_eq!(name("o/feature/x"), Some("o"));
        assert_eq!(name("origin/main"), None);
        assert_eq!(name("up"), None);
        assert_eq!(name("main"), None);
    }

    #[test]
    fn a_url_is_written_short_with_its_protocol_first() {
        for (url, short) in [
            (
                "git@github.com:paxel/multigit.git",
                "ssh github.com/paxel/multigit",
            ),
            ("github.com:paxel/multigit", "ssh github.com/paxel/multigit"),
            ("git@host:/srv/git/x.git/", "ssh host/srv/git/x"),
            ("ssh://git@host:2222/team/x.git", "ssh host:2222/team/x"),
            (
                "https://github.com/paxel/multigit.git",
                "https github.com/paxel/multigit",
            ),
            ("https://me:s3cret@example.org/a/b", "https example.org/a/b"),
            ("http://example.org/a/", "http example.org/a"),
            ("git://example.org/a.git", "git example.org/a"),
            ("file:///srv/git/x.git", "file /srv/git/x"),
            ("/srv/git/x.git", "file /srv/git/x"),
            ("../x", "file ../x"),
            ("C:/repos/x.git", "file C:/repos/x"),
            ("", "file "),
        ] {
            assert_eq!(short_url(url), short, "{url}");
        }
    }

    #[test]
    fn a_password_in_a_url_is_not_shown() {
        assert_eq!(
            safe_url("https://me:s3cret@example.org/a/b.git"),
            "https://example.org/a/b.git"
        );
        // A name alone is no secret, and nothing else is touched.
        for url in [
            "ssh://git@host/x.git",
            "git@github.com:paxel/multigit.git",
            "https://example.org/a",
            "/srv/x",
        ] {
            assert_eq!(safe_url(url), url);
        }
    }
}
