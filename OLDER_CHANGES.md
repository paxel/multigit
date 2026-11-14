# Older changes

Released versions of multigit before the current one. The current version is in [CHANGELOG.md](CHANGELOG.md).

## [0.3.0] - 2026-11-12

### Added

- Kawaii music, switched on in Settings: lofi beats made by multigit itself play while the Kawaii theme is on.
- Sounds, switched on in Settings: a beep when a remote commit appears on a card, a boop when a local one does. Volume and a sound file of your own per sound can be set.
- Read-only repository view: a double click on a card, or "Open" in its right-click menu, shows the repository's changed files, last commits and branches, and the diff of a picked file or commit.
- "Hide" in a card's right-click menu takes the card off the board but remembers the repository, so adding its parent folder again does not bring it back. Settings lists the hidden repositories and unhides them.
- "ahead" beside the toolbar's mark buttons marks the repositories with unpushed commits, and "ahead first" is a sort key.

### Changed

- Cards are removed with "Remove from board" in the card's right-click menu, for all marked cards at once when the card is marked; the × on the card and the toolbar's "Remove marked" are gone.
- A card shows its favorites as chips under its name, each in the favorite's color with its emoji and name, instead of stripes at the left edge and emojis before the name. A click on a chip shows only that favorite's cards.
- The Sort button opens a list of the sort keys instead of stepping to the next one with every click.

### Fixed

- The tooltip of a card's commit line shows the commit message once, not twice.

## [0.2.0] - 2026-11-11

### Added

- Favorites: named groups of repositories with a color and an emoji. Each has a toolbar button that shows only its cards; cards and favorite buttons have right-click menus to fill and change them.
- About page, opened from Settings: version, links to the source code, the issue tracker, a mail to the developer and the donation page, and the licences of the crates in multigit.
- Filter field in the toolbar: only cards whose folder name contains the text are shown; hidden cards are skipped by commands, marking and "Remove marked".
- Commit line on the card: short hash and subject of the checked-out commit; a click copies the full hash.
- Right-click menu on a card's branch: switch to a local or remote branch of that repository, or create a new branch.
- Right-click menu on a card's tag line: switch that repository to one of its tags.
- Terminal button on every card: opens a terminal window in that repository; the terminal command can be set in Settings.
- "Open in editor" on a command and on each repository's section of the output panel opens that output as a text file.
- "Output" button beside the command line hides the output panel completely and shows it again; while hidden it marks a running, succeeded or failed command.
- Theme setting: System, Light, Dark or Kawaii. Kawaii is round, pastel and animated; without focus it stands still, except for five seconds after a repository changed.
- Sort button in the toolbar: orders the cards by name, path, branch, tag, dirty first or behind first.

### Changed

- A click on a card copies only names: the folder name, the branch name or sha, and the tag. A click on any other text marks the card.
- Every card shows the folders above the repository under its name; a click on that line copies the repository's full path.
- A failed background fetch takes one line on the card, `Error: …`; a click on it copies everything git printed and removes the line until a fetch fails again.
- The tag line lists every tag of the tagged commit, not only one.
- The status refresh of a repository that did not change runs one git process instead of several; a branch or tag made outside multigit shows within a minute.

### Fixed

- A background fetch that fails in part shows git's error line on the card, not the first line git printed.

### Security

- The automatic status refresh and background fetch do not run programs named in a repository's own configuration: `core.fsmonitor`, hooks, `ext::` remotes and the `uploadpack` of non-SSH remotes.

## [0.1.0] - 2026-11-11

### Added

- Board of repository cards showing branch, nearest reachable tag, staged/modified/untracked/conflict counts, rebase or merge state, and unpushed/unpulled commits.
- Status refresh of all cards every 10 seconds and a background `git fetch --all --prune` every minute (configurable).
- (+) adds a repository, or every repository up to two folder levels below a chosen folder; × and "Remove marked" take cards off the board without touching files.
- Card buttons, by default `pull --ff-only`, `pull --rebase`, `push` and `fetch --all --prune`, editable in Settings with presets.
- Marking cards (a click on the card's empty space, or all / none / dirty / behind) and running toolbar buttons on all marked repositories.
- Command line for any git command on the marked repositories (a typed leading `git` is ignored), with Tab completion of subcommands, options, branches, remotes, tags and files, and a clickable suggestion popup.
- Shell lines: a command line or button starting with `!` runs through the login shell in each repository, after a confirmation.
- A click on any text of a card copies it to the clipboard.
- Command history with inline suggestions, filtered Up/Down and Ctrl+R fuzzy search, kept across restarts.
- Output panel with one section per repository and exit code for the last 50 commands.
- Confirmation before destructive commands (such as `reset --hard` or `push --force`) run in more than one repository.
- git runs without prompts, pager or editor; commands that need an editor fail with a clear message and failed logins show as "auth failed" on the card.
- Application menu entry and icon on Linux, registered on every start and by the release tarball's `install.sh`.
- macOS support: releases include `multigit.app` as a universal (Apple silicon and Intel) zip.
- Homebrew install from `paxel/tap`: a formula for Linux and a cask for macOS.
