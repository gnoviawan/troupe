//! The phone composer's Add to chat sheet (`ComposerAddSheet`, Paper "2.6d"): three attach
//! tiles, Mention and Commands, then Troupe tools (a sub-view) and Connectors.

use rune::{El, Tx, col, icon, row, text};
use theme::Tokens;
use theme::icons as i;
use ui::composer::view::tool_icon;
use ui::composer::wire::TOOLS;
use ui::type_::sans;

use crate::State;
use crate::composer::Host;

/// The sheet's overlay key, and its Troupe tools sub-view's above it.
pub const ADD_SHEET: &str = "composer-add-sheet";
pub const ADD_TOOLS: &str = "composer-add-tools";

impl State {
    /// The phone's +: the sheet for `host`'s composer.
    pub fn open_add_sheet(&mut self, host: &Host) {
        if self.composer_locked(host) {
            return;
        }
        self.composer.sheet_host = Some(host.clone());
        self.overlays.open(ADD_SHEET);
    }

    /// A sheet row that writes into the draft: close the sheet, then act as the desktop menu.
    fn sheet_add(&mut self, what: &str, tx: &mut Tx) {
        self.close(ADD_SHEET);
        if let Some(host) = self.composer.sheet_host.clone() {
            self.composer_add(&host, what, tx);
        }
    }
}

/// `SheetIconWell`: an 18-point muted glyph.
pub(crate) fn well(t: &Tokens, glyph: &'static rune::Icon) -> El {
    row().w(18.0).h(18.0).center().child(icon(glyph, 18.0).color(t.colors.muted_foreground))
}

/// `SheetIconWell` around a character (`@`, `/`).
fn well_text(t: &Tokens, c: &str) -> El {
    row()
        .w(18.0)
        .h(18.0)
        .center()
        .child(text(c.to_owned()).font(sans(16.0, 400.0, 24.0)).color(t.colors.muted_foreground))
}

/// `SheetRow`: the well, a 15-point title over a 14-point subtitle, and a chevron for a sub-view.
fn sheet_row(t: &Tokens, key: &str, well: El, title: &str, subtitle: &str, chevron: bool) -> El {
    let c = &t.colors;
    let mut e = row()
        .key(key.to_owned())
        .role("button")
        .label(title.to_owned())
        .w_full()
        .min_h(60.0)
        .py(8.0)
        .pl(14.0)
        .pr(10.0)
        .gap(12.0)
        .items_center()
        .hover(c.foreground.alpha(0.04))
        .child(well)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(text(title.to_owned()).font(sans(15.0, 400.0, 20.0)).color(c.foreground).truncate())
                .child(text(subtitle.to_owned()).font(sans(14.0, 400.0, 18.0)).color(c.muted_foreground).truncate()),
        );
    if chevron {
        e = e.child(icon(&i::CHEVRON_RIGHT, 14.0).color(c.muted_foreground));
    }
    e
}

/// `SheetGroup`: a rounded muted card, rows split by a hairline after the icon column.
pub(crate) fn group(t: &Tokens, rows: Vec<El>) -> El {
    let c = &t.colors;
    let mut card = col().w_full().radius(12.0).fill(c.muted).clip_rounded(12.0);
    for (n, r) in rows.into_iter().enumerate() {
        if n > 0 {
            card = card.child(row().w_full().pl(44.0).child(row().w_full().h(0.5).fill(c.border)));
        }
        card = card.child(r);
    }
    ui::paint::shadows(card, &t.shadows.shadow_control)
}

/// `AttachTile`: an 18-point glyph over a 15-point label on a 76-point muted tile.
fn tile(t: &Tokens, glyph: &'static rune::Icon, label: &'static str) -> El {
    let c = &t.colors;
    let tile = col()
        .key(format!("add-tile-{}", label.to_lowercase()))
        .role("button")
        .label(label)
        .grow(1.0)
        .h(76.0)
        .gap(8.0)
        .center()
        .radius(12.0)
        .fill(c.muted)
        .child(well(t, glyph))
        .child(text(label).font(sans(15.0, 400.0, 20.0)).color(c.foreground))
        .on_press(|s: &mut State| {
            s.close(ADD_SHEET);
            s.toast_later("Attaching files from the picker");
        });
    ui::paint::shadows(tile, &t.shadows.shadow_control)
}

fn root_view(t: &Tokens) -> Vec<El> {
    let trigger = |key: &str, c: &'static str, title: &str, subtitle: &str| {
        sheet_row(t, key, well_text(t, c), title, subtitle, false)
            .on_press_tx(move |s: &mut State, tx: &mut Tx| s.sheet_add(c, tx))
    };
    vec![
        row()
            .w_full()
            .gap(8.0)
            .child(tile(t, &i::CAMERA, "Camera"))
            .child(tile(t, &i::IMAGE_ICON, "Photos"))
            .child(tile(t, &i::FILE_TEXT, "Files")),
        group(
            t,
            vec![
                trigger("add-mention", "@", "Mention a file or conversation", "Attach project files or another chat as context"),
                trigger("add-commands", "/", "Commands", "Agent commands and skills"),
            ],
        ),
        group(
            t,
            vec![
                sheet_row(t, "add-tools", well(t, &i::WRENCH), "Troupe tools", "Browser, canvas, automations and orchestration", true)
                    .on_press(|s: &mut State| s.overlays.open(ADD_TOOLS)),
                sheet_row(t, "add-connectors", well(t, &i::SERVER), "Connectors", "None added", true)
                    .on_press(|s: &mut State| s.toast_later("Connectors")),
            ],
        ),
    ]
}

fn tools_view(t: &Tokens) -> Vec<El> {
    let rows = TOOLS
        .iter()
        .map(|tool| {
            let id = tool.id;
            sheet_row(t, &format!("add-tool-{id}"), well(t, tool_icon(id)), tool.label, tool.description, false)
                .on_press_tx(move |s: &mut State, tx: &mut Tx| s.sheet_add(id, tx))
        })
        .collect();
    vec![group(t, rows)]
}

/// The sheet, when it is open.
pub fn add_sheet(s: &State, t: &'static Tokens, window: (f32, f32), layer: &mut Vec<El>) {
    let tools = s.overlays.is_open(ADD_TOOLS);
    let (title, depth, body) = if tools {
        ("Troupe tools", 1, tools_view(t))
    } else {
        ("Add to chat", 0, root_view(t))
    };
    let mut sheet = crate::phone::sheet(ADD_SHEET, title.to_owned(), s)
        .depth(depth)
        .on_back(|s: &mut State| s.close(ADD_TOOLS))
        .on_close(move |s: &mut State| s.close(if tools { ADD_TOOLS } else { ADD_SHEET }));
    if tools {
        // The swipe dismisses the whole sheet, sub-view and all, as the web drawer does; the
        // card follows the finger at either depth.
        sheet = sheet.drag(s.phone.drag);
    }
    for e in body {
        sheet = sheet.child(e);
    }
    sheet.build(t, window, layer);
}
