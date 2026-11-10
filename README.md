# multigit

<img src="assets/multigit.svg" alt="multigit icon" width="96" align="right">

A desktop dashboard for many git repositories, for Linux and macOS. Every
repository is a card that shows where it stands; buttons and a command line run
git in one or many repositories at once.

## The board

Each card shows:

- the folder name (with the shortest distinguishing parent path when two
  repositories share a name; the full path is in the tooltip)
- the branch, or `detached @ <sha>`
- the working tree: `clean`, or `+staged ~modified ?untracked !conflicts`
- `rebasing` or `merging` while one is in progress
- unpushed and unpulled commits, `no upstream`, or `upstream gone`
- the result of the last command run on it, and a failed background fetch
  (`fetch: auth failed`, for example)

Cards are sorted by folder name. The status of every card refreshes every
10 seconds; a background `git fetch --all --prune` runs every minute so the
unpulled count stays current. A repository with a running command is skipped by
both until the command is done, and is refreshed right after it.

A card whose folder is gone or is no longer a repository stays on the board,
greyed out, and comes back by itself when the folder returns.

## Adding and removing repositories

- **+** opens a folder dialog. A repository is added as one card; any other
  folder is scanned for repositories up to two levels deep (configurable),
  without descending into repositories it finds. Repositories already on the
  board are skipped.
- **×** on a card, or **Remove marked** in the toolbar, takes cards off the
  board. Nothing on disk is touched.

## Running git

- **Card buttons** run in that one repository. The defaults are
  `pull --ff-only`, `pull --rebase`, `push` and `fetch --all --prune`.
- **Marking**: the checkbox on a card marks it; marked cards get a coloured
  border. The toolbar marks all, none, dirty (changes or an operation in
  progress) or behind (unpulled commits). The toolbar repeats the card buttons
  for all marked repositories.
- **Command line** at the bottom: type git arguments after the fixed `git`
  label (typing `git` first anyway is fine; it is ignored); Enter runs them in
  all marked repositories. It is disabled while nothing is marked.

Commands run in parallel, four at a time by default. Destructive commands
(`reset --hard`, `push --force`, `clean -f`, `branch -D`, `checkout -- <paths>`,
`restore`, `stash drop`, `stash clear`) ask for confirmation before they run in
more than one repository.

The output panel above the command line keeps the last 50 commands, one
collapsible section per repository with its exit code.

### Command line keys

| Key              | Action                                                                             |
|------------------|------------------------------------------------------------------------------------|
| Tab              | Complete: one match is inserted, several fill the common prefix and open the popup |
| Tab (popup open) | Next entry                                                                         |
| Up / Down        | Popup: move. Otherwise: older / newer history entry containing what you typed      |
| Enter            | Popup: accept. Otherwise: run                                                      |
| Esc              | Close the popup                                                                    |
| Right arrow      | Accept the grey history suggestion                                                 |
| Ctrl+R           | Fuzzy search the history                                                           |

Completion offers git's subcommands and aliases, the options git reports for
a command (with descriptions for about 20 common commands), and branches,
remotes, tags and changed files of the marked repositories. A value that only
some marked repositories have is tagged `in n/m`. "Tab inserts a single
completion directly" can be turned off in Settings; Tab then always opens the
popup.

### No terminal

git runs without a terminal: no password or passphrase prompts (use
ssh-agent or a credential helper), no pager, and no editor. A command that
needs an editor, such as `commit` without `-m` or `rebase -i`, fails with a
message saying so.

## Settings and files

**Settings** in the toolbar changes the fetch interval, the number of parallel
git processes, the folder scan depth, Tab's single-match behaviour, and the card
buttons (add, remove, reorder, or pick a preset).

| File                                                           | Content                                           |
|----------------------------------------------------------------|---------------------------------------------------|
| `~/.config/multigit/config.toml`                               | Repositories, buttons, settings; editable by hand |
| `~/.local/state/multigit/history`                              | Command line history, last 500 entries            |
| `~/.local/share/applications/io.github.paxel.multigit.desktop` | Menu entry, written on every start (Linux only)   |
| `~/.local/share/icons/hicolor/256x256/apps/multigit.png`       | Icon (Linux only)                                 |

`XDG_CONFIG_HOME`, `XDG_STATE_HOME` and `XDG_DATA_HOME` are honoured, on macOS
as well. If the
config file cannot be read, the app starts with defaults and does not save
over it.

## Install

### Release tarball (Linux x86_64)

Download `multigit-<version>-x86_64-linux.tar.gz` from the
[releases](https://github.com/paxel/multigit/releases), then:

```sh
tar -xzf multigit-<version>-x86_64-linux.tar.gz
multigit-<version>-x86_64-linux/install.sh
```

`install.sh` copies the binary to `~/.local/bin` and installs the menu entry
and icon. git must be installed.

### Release zip (macOS, Apple silicon and Intel)

Download `multigit-<version>-macos-universal.zip` from the
[releases](https://github.com/paxel/multigit/releases), unpack it and move
`multigit.app` to `/Applications`. The app is not signed: on first start,
right-click it and choose **Open**, or remove the quarantine flag:

```sh
xattr -dr com.apple.quarantine /Applications/multigit.app
```

An app started from the Finder or the Dock gets the system `PATH`
(`/usr/bin:/bin:/usr/sbin:/sbin`), so it runs `/usr/bin/git` from the Xcode
Command Line Tools (`xcode-select --install`), not a git from Homebrew.

To build the bundle yourself:

```sh
cargo build --release
packaging/macos/bundle.sh target/release/multigit 0.1.0 .
```

### From source

```sh
cargo build --release
target/release/multigit
```

On Linux the app registers its menu entry pointing at the binary it was
started from.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs these on Linux and macOS on every push and pull request. Pushing a
tag `vX.Y.Z` that matches the version in `Cargo.toml` builds the Linux tarball
and the macOS zip and publishes a GitHub release with that version's `CHANGELOG.md` section as release notes.
