//! What the terminal's menus can ask for, and doing it.

use super::App;
use super::favorites;
use super::folders;
use super::state::Mode;
use super::widgets::{Input, Item, Menu};
use crate::domain::board::SortKey;
use crate::domain::remote::remote_of;
use crate::domain::status::Head;
use crate::domain::switch::{branch_choices, new_branch_command, remote_choices, tag_choices};
use std::path::{Path, PathBuf};

/// An entry's deed.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// A heading or a greyed entry.
    Nothing,
    Sort(SortKey),
    /// Card button `n` on every marked row.
    ButtonOnMarked(usize),
    /// Put this text on the clipboard.
    Copy(String),
    /// Copy the whole fetch error of this row and take it off the row.
    CopyFetchError(PathBuf),
    /// Run these git arguments in this one repository.
    Run(PathBuf, String),
    /// The list of branches to switch this repository to.
    Branches(PathBuf),
    /// Ask for the name of a new branch in this repository.
    NewBranch(PathBuf),
    /// The list of tags to switch this repository to.
    Tags(PathBuf),
    /// The remotes this repository's branch can track.
    Remotes(PathBuf),
    /// What can be done with this favorite.
    Favorite(usize),
    /// Show only this favorite's rows, or all again.
    ToggleFavorite(usize),
    /// The form for this favorite.
    EditFavorite(usize),
    /// The form for a new favorite of these repositories.
    NewFavorite(Vec<PathBuf>),
    AddToFavorite(usize, Vec<PathBuf>),
    RemoveFromFavorite(usize, Vec<PathBuf>),
    DeleteFavorite(usize),
    /// Off the board; nothing on disk is touched.
    Remove(Vec<PathBuf>),
    /// Off the board and remembered, so a scan leaves them out.
    Hide(Vec<PathBuf>),
}

/// What a line typed into a [`Prompt`] is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptFor {
    /// The name of a new branch in this repository.
    NewBranch(PathBuf),
    /// A folder to add: a repository, or one to scan for repositories.
    AddFolder,
}

/// A question that wants a line of text.
#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    pub title: String,
    pub input: Input,
    pub purpose: PromptFor,
}

/// The answer to a prompt.
pub fn answer(app: &mut App, purpose: PromptFor, text: &str) {
    match purpose {
        PromptFor::NewBranch(path) => {
            if let Some(command) = new_branch_command(text) {
                app.start_run(&command, vec![path]);
            }
        }
        PromptFor::AddFolder => {
            if text.trim().is_empty() {
                return;
            }
            let (home, cwd) = places();
            let folder = folders::expand(text, home.as_deref(), &cwd);
            match folder.canonicalize() {
                Ok(folder) if folder.is_dir() => app.add_folder(&folder),
                _ => app.toast(format!("No folder {}", folder.display())),
            }
        }
    }
}

/// The home folder and the folder multigit was started in.
pub fn places() -> (Option<PathBuf>, PathBuf) {
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from);
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    (home, cwd)
}

/// Do what a picked entry asks for.
pub fn perform(app: &mut App, action: Action) {
    match action {
        Action::Nothing => {}
        Action::Sort(key) => app.sort = key,
        Action::ButtonOnMarked(index) => app.run_button_on_marked(index),
        Action::Copy(text) => copy(app, text),
        Action::CopyFetchError(path) => {
            if let Some(text) = app.take_fetch_error(&path) {
                copy(app, text);
            }
        }
        Action::Run(path, command) => app.start_run(&command, vec![path]),
        Action::Branches(path) => {
            let menu = switch_menu(app, &path, true);
            app.gui.mode = Mode::Menu(menu);
        }
        Action::Tags(path) => {
            let menu = switch_menu(app, &path, false);
            app.gui.mode = Mode::Menu(menu);
        }
        Action::NewBranch(path) => {
            app.gui.mode = Mode::Prompt(Prompt {
                title: "new branch from the checked-out commit".to_owned(),
                input: Input::default(),
                purpose: PromptFor::NewBranch(path),
            });
        }
        Action::Remotes(path) => {
            let menu = remote_menu(app, &path);
            app.gui.mode = Mode::Menu(menu);
        }
        Action::Favorite(index) => {
            let menu = favorites::of(app, index);
            app.gui.mode = Mode::Menu(menu);
        }
        Action::ToggleFavorite(index) => app.toggle_favorite(index),
        Action::EditFavorite(index) => {
            app.edit_favorite(index);
            open_form(app);
        }
        Action::NewFavorite(repos) => {
            app.new_favorite(repos);
            open_form(app);
        }
        Action::AddToFavorite(index, repos) => app.add_to_favorite(index, &repos),
        Action::RemoveFromFavorite(index, repos) => app.remove_from_favorite(index, &repos),
        Action::DeleteFavorite(index) => app.delete_favorite(index),
        Action::Remove(repos) => app.remove_cards(&repos),
        Action::Hide(repos) => app.hide_cards(&repos),
    }
}

/// Open the favorite form on what `app.favorite_dialog` holds.
fn open_form(app: &mut App) {
    if let Some(dialog) = &app.favorite_dialog {
        app.gui.mode = Mode::Favorite(favorites::Form {
            field: favorites::Field::Name,
            name: Input::new(dialog.draft.name.clone()),
        });
    }
}

/// The branches (`branches`) or tags to switch `path` to, newest first,
/// with a filter field.
fn switch_menu(app: &App, path: &Path, branches: bool) -> Menu {
    let snap = app
        .cards
        .iter()
        .find(|c| c.path == path)
        .and_then(|c| c.snapshot());
    let choices = match snap {
        Some(snap) if branches => branch_choices(&snap.dated, &snap.refs.remotes),
        Some(snap) => tag_choices(&snap.dated),
        None => Vec::new(),
    };
    let items = choices
        .into_iter()
        .map(|c| Item::new(c.label, Action::Run(path.to_path_buf(), c.command)))
        .collect();
    let (title, none) = if branches {
        ("switch to", "no other branch")
    } else {
        ("switch to tag", "no tags")
    };
    Menu::new(title, items).filtered(none)
}

/// The remotes the branch of `path` can be made to track.
fn remote_menu(app: &App, path: &Path) -> Menu {
    let Some(snap) = app
        .cards
        .iter()
        .find(|c| c.path == path)
        .and_then(|c| c.snapshot())
    else {
        return Menu::new("remotes", vec![Item::disabled("no status yet")]);
    };
    let branch = match &snap.status.head {
        Head::Branch(branch) => Some(branch.as_str()),
        Head::Unborn(_) | Head::Detached(_) => None,
    };
    let tracked = snap
        .status
        .tracking
        .as_deref()
        .and_then(|upstream| remote_of(&snap.remotes, upstream))
        .map(|remote| remote.name.as_str());
    let items: Vec<Item> = remote_choices(branch, tracked, &snap.remotes, &snap.dated)
        .into_iter()
        .map(|choice| {
            let label = if choice.current {
                format!("{} (tracked)", choice.label)
            } else {
                choice.label
            };
            match choice.command {
                Some(command) => Item::new(label, Action::Run(path.to_path_buf(), command)),
                None => Item::disabled(label),
            }
        })
        .collect();
    let title = branch.map_or("remotes".to_owned(), |b| format!("remote of {b}"));
    if items.is_empty() {
        return Menu::new(title, vec![Item::disabled("no remote")]);
    }
    Menu::new(title, items)
}

/// Send `text` to the terminal's clipboard with the next frame.
pub fn copy(app: &mut App, text: String) {
    let shown: String = text
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(40)
        .collect();
    let more = if shown.len() < text.len() {
        "\u{2026}"
    } else {
        ""
    };
    app.toast(format!("Copied {shown}{more}"));
    app.gui.copies.push(text);
}
