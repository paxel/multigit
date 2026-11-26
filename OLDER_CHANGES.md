# Older changes

Released versions of multigit before the current one. The current version is in [CHANGELOG.md](CHANGELOG.md).

## [0.5.0] - 2026-11-26

### Added

- `multigit --tui` runs the board in a terminal, for a machine without a display: a table with one row per repository, the whole card of the selected row under it, and statuses and fetches as in the window; no sounds and no music there ([mgit-1001](https://github.com/paxel/multigit/issues/72)).
- In the terminal, `space` and a click mark rows, `a` `d` `b` `p` work like the mark buttons, `D` `B` `P` show only the dirty, behind or ahead rows, `/` filters by name and `s` sorts ([mgit-1002](https://github.com/paxel/multigit/issues/73)).
- The terminal has the command line with Tab completion, history, Ctrl+R and the grey suggestion, and asks before shell lines and destructive commands ([mgit-1003](https://github.com/paxel/multigit/issues/74)).
- The terminal shows the output of the last commands, a section per repository that folds, and opens it in `$PAGER` ([mgit-1004](https://github.com/paxel/multigit/issues/75)).
- In the terminal `1` to `9` run the card buttons, `m` runs one on all marked rows, and `t` opens `$SHELL` in the row's folder ([mgit-1005](https://github.com/paxel/multigit/issues/76)).
- The row's menu in the terminal (`x` or a right click) copies the card's texts to the clipboard of the terminal, also over SSH (OSC 52) ([mgit-1006](https://github.com/paxel/multigit/issues/77)).
- The row's menu in the terminal switches to a branch or a tag, creates a branch and changes the remote the branch tracks ([mgit-1007](https://github.com/paxel/multigit/issues/78)).
- Favorites in the terminal: a line of favorites over the table, `f` to show, edit, fill or delete one, and the row's menu to add rows to one or make a new one ([mgit-1008](https://github.com/paxel/multigit/issues/79)).
- In the terminal `+` adds a typed folder (Tab completes it), and the row's menu hides a repository or takes it off the board ([mgit-1009](https://github.com/paxel/multigit/issues/80)).
- The repository view in the terminal (Enter or a double click), with the Files tab, Back and Forward, Refresh and copying the whole right side ([mgit-1010](https://github.com/paxel/multigit/issues/81)).
- The Commits tab of the repository view in the terminal, with the commit's values to copy, its parents to go to and its files to unfold ([mgit-1011](https://github.com/paxel/multigit/issues/82)).
- The Graph tab of the repository view in the terminal, drawn with line characters ([mgit-1012](https://github.com/paxel/multigit/issues/83)).
- Settings and About in the terminal (`S`): the values, the card buttons and the hidden repositories; a link is copied, as there is no browser ([mgit-1013](https://github.com/paxel/multigit/issues/84)).
- The Kawaii theme in the terminal: pastel colors, bunnies and cats in text in front of the rows, and hearts and stars from a row whose command ended well ([mgit-1014](https://github.com/paxel/multigit/issues/85)).

### Security

- The `url` crate is updated to 2.5.9, which fixes a stack overflow on nested `blob:` URLs (RUSTSEC-2026-0592).

## [0.4.0] - 2026-11-14

### Added

- The Kawaii music has five themes, each with its own instruments, songs and breaks: Japanese traditional lofi, retro gaming lofi, Babymetal-style lofi, Cajun porch lofi (a banjo, a fiddle, a washtub bass, a washboard, a jaw harp) and trip-hop lofi (a theremin over a slow beat on a crackling record, with bombs and a call far away). After each song it stays in the theme or, one time in four, changes to another with a longer break between the two; among those breaks are a modem dialling in and a record scratch.
- Every theme of the Kawaii music has a motto: one short phrase that returns in about every second song and in one of the theme's long breaks, always the same notes, in the key of the song.
- Settings has a checkbox for each theme of the Kawaii music: only the checked themes play, and with a single one checked the music never leaves it.
- Bunnies and cats live on the cards in the Kawaii theme. They blink, wiggle their ears, hop from card to card and sleep while nothing happens; they look up at a new remote commit, dance for a local one, let their ears hang over a failed fetch, look at the pointer and jump with a heart when clicked.
- In the Kawaii theme a card throws a burst of hearts and stars when a command on it ends well.
- Every card shows the remote its branch tracks, as a short URL with the protocol first (`ssh github.com/paxel/multigit`); a click copies the URL. A branch that pushes to another remote says so.
- A right click on a card's remote lists the remotes of the repository; picking one makes the branch track the branch of the same name there.
- A right click on a mark button of the toolbar (dirty, behind, ahead) shows only the cards that fit it, and the toolbar says so; a second right click, or one on "all", shows all again.
- A right click on text in the repository view (commit message, diff, file content) and on the output of a command opens a menu: `Copy` copies what is marked and leaves it marked, `Copy all` copies the whole text, the lines that are not on screen included.

### Changed

- The repository view has three tabs, Files, Commits and Graph, with Back and Forward through what was looked at. It opens on Files when the repository has changes and on Commits when it is clean.
- The Files tab of the repository view shows the changed files as a tree with folders that fold, each file once and colored by what happened to it since the last commit, staged or not. A picked file shows its content when it is new, what is gone when it was deleted, and its diff otherwise.
- The Commits tab of the repository view lists the history in compact rows (the author's initials in the author's color, short hash, the tags on the commit, title, age) and reads on when scrolled to its end. A picked commit shows its hash, author, date, parents, whole message and the branches and tags on it above the files it changed with the lines added and removed; a click on a parent goes to it, a click on the hash, the author, the mail address, the date or a name copies it, and a click on a file shows what the commit did to it.
- The Graph tab of the repository view draws all local and remote branches as a graph that shows only the commits that matter (branch tips, tags, merges, and where branches part) and folds the runs of commits between them into one row that unfolds on a click. Remote branches nobody has locally and that are older than 90 days are left out behind a line that shows them.
- The toolbar's mark buttons stay pressed like the station buttons of a radio: a second click on the pressed button marks the other cards instead, a click on another button releases it. "none" is gone: it is the second click on "all".
- The Kawaii music plays songs of about a minute, each with two themes of its own, that vary each time they play, with short and long breaks between them, instead of ten short passages.

### Fixed

- The volume of the Kawaii music changes as soon as the slider is let go, not only after the music was switched off and on.
- The app starts with the output panel as it was left: hidden stays hidden.
- (+) on a folder no longer adds folders whose `.git` is empty or incomplete, like a repository copied without its content.

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
