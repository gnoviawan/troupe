//! The phone's Files sheet (`MobileFileExplorer`) over the explorer state, and its row actions
//! sheet: Rename, Duplicate and Delete, the delete asking first.

use rune::El;
use theme::Tokens;
use ui::explorer::{FILES, PHONE_NAME_KEY, PhoneHooks};
use ui::mobile;

use crate::explorer_host::{get, lens};
use crate::State;

/// A Files row's ⋯ sheet.
pub const FILE_ACTIONS: &str = "phone-file-actions";

fn glyph(name: &str) -> &'static rune::Icon {
    theme::icons::named(name).unwrap_or(&theme::icons::X)
}

impl State {
    /// The More sheet's Files row and the drawer's Files view.
    pub(crate) fn open_files(&mut self) {
        self.explorer.open_phone();
        self.overlays.open(FILES);
        self.run_explorer_effects();
    }
}

fn hooks() -> PhoneHooks<State> {
    PhoneHooks {
        close: |s: &mut State| s.close(FILES),
        actions: |s: &mut State, path: &str| {
            s.phone.file = Some(path.to_owned());
            s.overlays.open(FILE_ACTIONS);
        },
        open_file: |s: &mut State, path: &str| {
            s.close(FILES);
            s.open_file(path);
        },
    }
}

pub fn files_sheet(s: &State, t: &'static Tokens, window: (f32, f32), layer: &mut Vec<El>) {
    ui::explorer::files_sheet(s, get, lens, t, window, hooks(), layer);
    ui::explorer::phone_confirm(s, get, lens, t, layer);
}

pub fn actions_sheet(s: &State, t: &'static Tokens, window: (f32, f32), layer: &mut Vec<El>) {
    let Some(path) = s.phone.file.clone() else { return };
    let Some(entry) = s.explorer.tree.entry(&path).cloned() else { return };
    let (renamed, duplicated, deleted) = (path.clone(), path.clone(), path);
    let rows = vec![
        mobile::sheet_row(t, glyph("Edit"), "Rename", false).key("phone-file-rename").on_press_tx(move |s: &mut State, tx: &mut rune::Tx| {
            s.close(FILE_ACTIONS);
            s.explorer.step(model::explorer::Event::StartRename(renamed.clone()));
            tx.focus(PHONE_NAME_KEY, false);
        }),
        mobile::sheet_row(t, glyph("Copy"), "Duplicate", false).key("phone-file-duplicate").on_press(move |s: &mut State| {
            s.close(FILE_ACTIONS);
            s.explorer.step(model::explorer::Event::Duplicate(duplicated.clone()));
            s.run_explorer_effects();
        }),
        mobile::sheet_row(t, glyph("Trash2"), "Delete", true).key("phone-file-delete").on_press(move |s: &mut State| {
            s.close(FILE_ACTIONS);
            s.explorer.step(model::explorer::Event::RequestDelete(deleted.clone()));
        }),
    ];
    crate::phone::sheet(FILE_ACTIONS, entry.name, s)
        .child(rune::col().w_full().items_stretch().children(rows))
        .build(t, window, layer);
}
