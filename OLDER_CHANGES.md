# Older changes

Released versions of multigit before the current one. The current version is in [CHANGELOG.md](CHANGELOG.md).

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
