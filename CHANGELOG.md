# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.0]

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

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
