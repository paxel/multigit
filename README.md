# multigit

<p>
  <a href="https://github.com/paxel/multigit/actions/workflows/ci.yml"><img src="https://github.com/paxel/multigit/actions/workflows/ci.yml/badge.svg?branch=summer" alt="CI"/></a>
  <a href="https://github.com/paxel/multigit/actions/workflows/security.yml"><img src="https://github.com/paxel/multigit/actions/workflows/security.yml/badge.svg?branch=summer" alt="Security"/></a>
  <a href="https://github.com/paxel/multigit/actions/workflows/codeql.yml"><img src="https://github.com/paxel/multigit/actions/workflows/codeql.yml/badge.svg?branch=summer" alt="CodeQL"/></a>
  <a href="https://sonarcloud.io/summary/new_code?id=paxel_multigit"><img src="https://sonarcloud.io/api/project_badges/measure?project=paxel_multigit&metric=alert_status" alt="Quality gate"/></a>
  <a href="https://sonarcloud.io/component_measures?id=paxel_multigit&metric=coverage"><img src="https://sonarcloud.io/api/project_badges/measure?project=paxel_multigit&metric=coverage" alt="Coverage"/></a>
  <a href="https://sonarcloud.io/summary/overall?id=paxel_multigit"><img src="https://sonarcloud.io/api/project_badges/measure?project=paxel_multigit&metric=ncloc" alt="Lines of code"/></a>
  <a href="https://github.com/paxel/multigit/releases/latest"><img src="https://img.shields.io/github/v/release/paxel/multigit" alt="Release"/></a>
  <img src="https://img.shields.io/badge/platforms-Linux%20%7C%20macOS-informational" alt="Platforms"/>
  <img src="https://img.shields.io/github/languages/top/paxel/multigit" alt="Top language"/>
  <img src="https://img.shields.io/github/last-commit/paxel/multigit/summer" alt="Last commit"/>
</p>

<img src="assets/multigit.svg" alt="multigit icon" width="96" align="right">

A desktop dashboard for many git repositories, for Linux and macOS. Every
repository is a card that shows where it stands; buttons and a command line run
git in one or many repositories at once.

## The board

Each card shows:

- the folder name, and under it the folders above it (as many as fit; the
  full path is in the tooltip)
- the branch, or `detached @ <sha>`
- the nearest tag reachable from the checked-out commit (lightweight tags
  included) and every other tag on the same commit, highest version first,
  or `no tag`
- the checked-out commit: short hash and subject
- the working tree: `clean`, or `+staged ~modified ?untracked !conflicts`
- `rebasing` or `merging` while one is in progress
- unpushed and unpulled commits, `no upstream`, or `upstream gone`
- the result of the last command run on it
- a failed background fetch as one red line (`Error: auth failed`, for
  example); a click on it copies everything git printed and takes the line
  off the card until a fetch fails again

Cards are sorted by folder name. The **Sort** button in the toolbar switches
to the next sort key with every click: name, path, branch, tag, dirty first,
behind first. Cards without a branch or tag come last, equal cards are in name
order, and a card moves as soon as its status changes. The status of every card refreshes every
10 seconds; a background `git fetch --all --prune` runs every minute so the
unpulled count stays current. A repository with a running command is skipped by
both until the command is done, and is refreshed right after it.

The filter field at the right end of the toolbar shows only the cards whose
folder name contains the typed text (case ignored), and how many those are. A
hidden card keeps its mark, but nothing touches it: commands, the toolbar's
mark buttons and **Remove marked** act on the shown cards only.

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

- **Terminal**: the `>_` button in a card's top row opens a terminal window
  with a shell in that repository. Without a terminal command in Settings,
  multigit takes `$TERMINAL`, then `xdg-terminal-exec`, then the first of
  konsole, gnome-terminal, xfce4-terminal, kitty, alacritty, foot, wezterm and
  xterm that is installed; on macOS it opens Terminal.app. In a terminal
  command of your own, `{dir}` stands for the repository's folder.
- **Card buttons** run in that one repository. The defaults are
  `pull --ff-only`, `pull --rebase`, `push` and `fetch --all --prune`.
- **Marking**: a click on a card marks it, a second click unmarks it; marked
  cards get a coloured border. The toolbar marks all, none, dirty (changes or
  an operation in progress) or behind (unpulled commits). The toolbar repeats the card buttons
  for all marked repositories.
- **Copying**: a click on a card's folder name, branch name (or the sha of a
  detached HEAD) or tag copies it to the clipboard instead of marking the
  card; a click on the path line copies the repository's full path, and a
  click on the commit line the full commit hash.
- **Command line** at the bottom: type git arguments after the fixed `git`
  label (typing `git` first anyway is fine; it is ignored); Enter runs them in
  all marked repositories. It is disabled while nothing is marked.
- **Shell lines**: a line starting with `!` runs through your login shell
  (`$SHELL -c`, or `sh -c` when `$SHELL` is empty) in each marked repository's
  folder, for example `!ls | wc -l`. The label in front of the line then shows
  the shell's name. A button whose arguments start with `!` does the same.
  Shell lines always ask for confirmation first, even for one repository
  (Enter or Run confirms, Esc cancels), and get no Tab completion.

Commands run in parallel, four at a time by default. Destructive commands
(`reset --hard`, `push --force`, `clean -f`, `branch -D`, `checkout -- <paths>`,
`restore`, `stash drop`, `stash clear`) ask for confirmation before they run in
more than one repository.

The output panel above the command line keeps the last 50 commands, one
collapsible section per repository with its exit code. **Open in editor**, on
a command or on one repository's section, writes that output to a text file
and opens it with the system's default program for text files (`xdg-open` on
Linux, `open` on macOS); the files are in the temp folder and are removed when
multigit exits. Drag its upper edge to
make it bigger. The **Output** button left of the command line hides the panel
completely and brings it back; while it is hidden, a mark on the button tells
about the last command: running, then green or red by its result.

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

git and shell lines run without a terminal and without input: no password or passphrase prompts (use
ssh-agent or a credential helper), no pager, and no editor. A command that
needs an editor, such as `commit` without `-m` or `rebase -i`, fails with a
message saying so.

### Add only folders you trust

multigit runs git in every repository on the board by itself: a status query
every 10 seconds and a fetch every minute. A repository's own `.git/config` and
`.git/hooks` can name programs that git runs. A cloned repository does not
bring those along, but a folder that arrives with its `.git` directory does (an
unpacked archive, a USB stick, a shared folder), and **+** on a folder such as
`~/Downloads` adds every repository below it.

For what it runs by itself, multigit switches off `core.fsmonitor`, hooks,
`ext::` remotes, and `remote.<name>.uploadpack` for remotes that are not
reached over SSH (over SSH it names a program on the server). Other settings
that make git run a program stay in effect, because switching them off breaks
legitimate setups such as git-lfs or a key per repository. Among them:

- clean filters (`filter.<driver>.clean`), which can run during the status query
- `core.sshCommand`, `credential.helper`, `core.gitProxy` and
  `core.alternateRefsCommand`, which can run during the fetch

So the board is no sandbox: add only folders whose origin you know.

Buttons and the command line run git as you would in a terminal, hooks and all.

## Settings and files

**Settings** in the toolbar changes the fetch interval, the number of parallel
git processes, the folder scan depth, the terminal command, the theme (System,
Light or Dark), Tab's
single-match behaviour, and the card buttons (add, remove, reorder, or pick a
preset).

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

### Homebrew (Linux x86_64 and macOS)

```sh
brew install paxel/tap/multigit           # Linux
brew install --cask paxel/tap/multigit    # macOS
```

On Linux the formula also puts the menu entry and icon into
`~/.local/share`; `multigit-install-icon` writes them again, and
`multigit-install-icon --uninstall` removes them before `brew uninstall`. On
macOS the app is not signed: on first start, choose **Open Anyway** in System
Settings, Privacy & Security.

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
packaging/macos/bundle.sh target/release/multigit 0.2.0 .
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
tool/coverage.sh    # needs cargo-llvm-cov; COVERAGE_JOBS=1 for a small machine
```

Clippy denies `unsafe`, and `unwrap`, `expect`, `panic` and indexing outside
tests (`Cargo.toml`, `clippy.toml`).

CI runs these on Linux and macOS on every push and pull request, together with
CodeQL (Rust and the workflows) and OSV-Scanner on `Cargo.lock`. A notice that a
crate is unmaintained only fails the weekly Security run; accepted ones are in
`osv-scanner.toml`, each with a reason and a review date. SonarQube Cloud
analyses `summer` after every push. Pushing a
tag `vX.Y.Z` that matches the version in `Cargo.toml` builds the Linux tarball
and the macOS zip and publishes a GitHub release with that version's `CHANGELOG.md` section as release notes.
The release stops first on a failing check, a known vulnerability, an open high
or critical CodeQL alert, or line coverage below the floor in
`tool/coverage-floor.txt`.
