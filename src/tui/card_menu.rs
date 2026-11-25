//! The card's menu in the terminal: what a right click on a card offers in
//! the window, and the card's texts to copy.

use super::App;
use super::actions::Action;
use super::board::repositories;
use super::widgets::{Item, Menu};
use crate::domain::naming::title;
use crate::domain::status::Head;
use std::path::Path;

/// The menu of the row of `path`. It acts on that row, or on all marked
/// rows when it is marked; copying is about that row alone.
pub fn card_menu(app: &App, path: &Path) -> Menu {
    let targets = app.menu_targets(path);
    let heading = if targets.len() == 1 {
        title(path)
    } else {
        repositories(targets.len())
    };
    let mut items = Vec::new();
    items.extend(copy_items(app, path));
    Menu::new(heading, items)
}

fn copy(label: impl Into<String>, text: impl Into<String>) -> Item {
    Item::new(label, Action::Copy(text.into()))
}

/// The texts a click copies on the card in the window.
fn copy_items(app: &App, path: &Path) -> Vec<Item> {
    let Some(card) = app.cards.iter().find(|c| c.path == path) else {
        return Vec::new();
    };
    let mut items = vec![
        copy("Copy name", title(path)),
        copy("Copy path", path.display().to_string()),
    ];
    if let Some(snap) = card.snapshot() {
        match &snap.status.head {
            Head::Branch(b) | Head::Unborn(b) => items.push(copy("Copy branch", b.clone())),
            Head::Detached(sha) => {
                items.push(copy("Copy commit of the detached HEAD", sha.clone()))
            }
        }
        for tag in &snap.tags {
            items.push(copy(format!("Copy tag {tag}"), tag.clone()));
        }
        if let Some(commit) = &snap.commit {
            items.push(copy("Copy commit hash", commit.hash.clone()));
        }
        if let Some(remote) = snap.remote() {
            items.push(copy("Copy remote URL", remote.url.clone()));
        }
    }
    if card.fetch_error.is_some() {
        items.push(Item::new(
            "Copy the fetch error and take it off",
            Action::CopyFetchError(path.to_path_buf()),
        ));
    }
    items
}
