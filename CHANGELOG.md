# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.7.0]

### Added

- The file lists of the repository view switch between a tree of folders and a flat list of paths, with the Tree button of a list or `` ` `` in the terminal; the choice is saved, and the files of a commit fold by folder too ([mgit-1026](https://github.com/paxel/multigit/issues/101)).
- The Files tab of the repository view stages, unstages and discards files and folders: a checkbox and a right-click menu in the window, Space, `d` and a right click in the terminal; each file shows its staged and unstaged letters in the colors lazygit uses, and a discard asks first ([mgit-1027](https://github.com/paxel/multigit/issues/102)).
- Commit what is staged from the Files tab with the Commit button or `c`: one box whose first line is the subject, Ctrl+Enter (Ctrl+S in the terminal) or Enter three times at the end commits, the empty line before the body is added, nothing staged offers to stage everything, and git's error stays in the box ([mgit-1030](https://github.com/paxel/multigit/issues/105)).
- The commit box starts with the ticket key of the branch, or of the last commit, written the way the last commit wrote it, or as `[KEY] ` ([mgit-1031](https://github.com/paxel/multigit/issues/106)).
- A file or folder of the Files tab is stashed from its menu, untracked files too, with a message; the new Stash tab (`4`) lists the stashes with their changes and applies, pops or drops one (Space, `P`, `d` in the terminal), and a drop asks first ([mgit-1028](https://github.com/paxel/multigit/issues/103)).
- "Edit" on a file of the repository view (`e` in the terminal) opens its copy in the working tree in the editor command of the settings, or the system's default program; the terminal hands itself to `$VISUAL` or `$EDITOR` until the editor ends ([mgit-1029](https://github.com/paxel/multigit/issues/104)).
- The Reflog tab (`5`) of the repository view lists where HEAD has been, each step with its commit, what moved HEAD there and its message; a picked step shows its commit ([mgit-1034](https://github.com/paxel/multigit/issues/109)).
- A row of the Graph tab checks out one of its branches or its commit from its right-click menu (`o` in the terminal); a remote branch gets its local tracking branch, local changes in the way can be stashed first, and the view says when HEAD is detached ([mgit-1033](https://github.com/paxel/multigit/issues/108)).
- "Apply to repositories…" on a commit of the Commits, Graph or Reflog tab (`a` in the terminal) takes it to the board, where the repositories it goes to are marked; Apply makes it a commit there with the same message and author, and where it does not apply the repository stays as it was and the output says why ([mgit-1032](https://github.com/paxel/multigit/issues/107)).

### Changed

- The fold triangles of the repository view in the window have three equal sides instead of a stretched shape ([mgit-1025](https://github.com/paxel/multigit/issues/100)).

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
