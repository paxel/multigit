# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.3.0]

### Added

- "Hide" in a card's right-click menu takes the card off the board but remembers the repository, so adding its parent folder again does not bring it back. Settings lists the hidden repositories and unhides them.
- "ahead" beside the toolbar's mark buttons marks the repositories with unpushed commits, and "ahead first" is a sort key.

### Changed

- Cards are removed with "Remove from board" in the card's right-click menu, for all marked cards at once when the card is marked; the × on the card and the toolbar's "Remove marked" are gone.
- A card shows its favorites as chips under its name, each in the favorite's color with its emoji and name, instead of stripes at the left edge and emojis before the name. A click on a chip shows only that favorite's cards.
- The Sort button opens a list of the sort keys instead of stepping to the next one with every click.

### Fixed

- The tooltip of a card's commit line shows the commit message once, not twice.

---

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
