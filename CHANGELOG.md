# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.0]

### Added

- Filter field in the toolbar: only cards whose folder name contains the text are shown; hidden cards are skipped by commands, marking and "Remove marked".

### Changed

- A click on a card copies only names: the folder name, the branch name or sha, and the tag. A click on any other text marks the card.
- Every card shows the folders above the repository under its name; a click on that line copies the repository's full path.
- A failed background fetch takes one line on the card, `Error: …`; a click on it copies everything git printed and removes the line until a fetch fails again.

### Fixed

- A background fetch that fails in part shows git's error line on the card, not the first line git printed.

### Security

- The automatic status refresh and background fetch do not run programs named in a repository's own configuration: `core.fsmonitor`, hooks, `ext::` remotes and the `uploadpack` of non-SSH remotes.

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
