# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.4.0]

### Added

- The Kawaii music has three themes, each with its own instruments, songs and breaks: Japanese traditional lofi, retro gaming lofi and Babymetal-style lofi. It changes theme about every five minutes, with a longer break between two themes; one of them is a modem dialling in.

### Changed

- The repository view has three tabs, Files, Commits and Graph, with Back and Forward through what was looked at. It opens on Files when the repository has changes and on Commits when it is clean.
- The Files tab of the repository view shows the changed files as a tree with folders that fold, each file once and colored by what happened to it since the last commit, staged or not. A picked file shows its content when it is new, what is gone when it was deleted, and its diff otherwise.
- The Commits tab of the repository view lists the history in compact rows (the author's initials in the author's color, short hash, title, age) and reads on when scrolled to its end. A picked commit shows its hash, author, date, whole message and the branches and tags on it above the files it changed; a click on a file shows what the commit did to it.
- The Kawaii music plays songs of about a minute that vary each time they play, with short and long breaks between them, instead of ten short passages.

### Fixed

- The volume of the Kawaii music changes as soon as the slider is let go, not only after the music was switched off and on.

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
