# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.4.0]

### Added

- The Kawaii music has three themes, each with its own instruments, songs and breaks: Japanese traditional lofi, retro gaming lofi and Babymetal-style lofi. After each song it stays in the theme or, one time in four, changes to another with a longer break between the two; one of those breaks is a modem dialling in.
- Bunnies and cats live on the cards in the Kawaii theme. They blink, wiggle their ears, hop from card to card and sleep while nothing happens; they look up at a new remote commit, dance for a local one, let their ears hang over a failed fetch, look at the pointer and jump with a heart when clicked.
- In the Kawaii theme a card throws a burst of hearts and stars when a command on it ends well.
- Every card shows the remote its branch tracks, as a short URL with the protocol first (`ssh github.com/paxel/multigit`); a click copies the URL. A branch that pushes to another remote says so.
- A right click on a card's remote lists the remotes of the repository; picking one makes the branch track the branch of the same name there.
- A right click on text in the repository view (commit message, diff, file content) and on the output of a command opens a menu: `Copy` copies what is marked and leaves it marked, `Copy all` copies the whole text, the lines that are not on screen included.

### Changed

- The repository view has three tabs, Files, Commits and Graph, with Back and Forward through what was looked at. It opens on Files when the repository has changes and on Commits when it is clean.
- The Files tab of the repository view shows the changed files as a tree with folders that fold, each file once and colored by what happened to it since the last commit, staged or not. A picked file shows its content when it is new, what is gone when it was deleted, and its diff otherwise.
- The Commits tab of the repository view lists the history in compact rows (the author's initials in the author's color, short hash, the tags on the commit, title, age) and reads on when scrolled to its end. A picked commit shows its hash, author, date, parents, whole message and the branches and tags on it above the files it changed with the lines added and removed; a click on a parent goes to it, a click on the hash, the author, the mail address, the date or a name copies it, and a click on a file shows what the commit did to it.
- The Graph tab of the repository view draws all local and remote branches as a graph that shows only the commits that matter (branch tips, tags, merges, and where branches part) and folds the runs of commits between them into one row that unfolds on a click. Remote branches nobody has locally and that are older than 90 days are left out behind a line that shows them.
- The Kawaii music plays songs of about a minute that vary each time they play, with short and long breaks between them, instead of ten short passages.

### Fixed

- The volume of the Kawaii music changes as soon as the slider is let go, not only after the music was switched off and on.
- The app starts with the output panel as it was left: hidden stays hidden.
- (+) on a folder no longer adds folders whose `.git` is empty or incomplete, like a repository copied without its content.

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
