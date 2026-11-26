# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.5.0]

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

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
