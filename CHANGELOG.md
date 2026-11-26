# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.6.0]

### Added

- On Windows the config is kept in `%APPDATA%\multigit` and the history in `%LOCALAPPDATA%\multigit` ([mgit-1015](https://github.com/paxel/multigit/issues/89)).
- On Windows `!` lines and the terminal board's shell run in PowerShell when `SHELL` is not set: `pwsh`, or `powershell` without PowerShell 7 ([mgit-1016](https://github.com/paxel/multigit/issues/90)).
- On Windows the terminal button opens Windows Terminal in the repository, or a PowerShell console without it ([mgit-1017](https://github.com/paxel/multigit/issues/91)).
- On Windows files and links open with their default program ([mgit-1018](https://github.com/paxel/multigit/issues/92)).
- On Windows sounds and the Kawaii music play through `ffplay` when installed, otherwise through PowerShell ([mgit-1019](https://github.com/paxel/multigit/issues/93)).

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
