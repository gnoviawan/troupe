//! The colour theme picker (`ThemePicker.tsx`): a panel beside the rail listing every bundled
//! theme by family, dark then light. Hovering or arrowing onto a row previews it on the whole
//! window; Return or a press applies it to `settings/app` (`colorTheme`, `appearanceMode`);
//! Escape, the ✕ or a press outside put the saved theme back.

use rune::{El, TextEdit, col, el, icon, row, text, text_input};
use serde_json::Value;
use theme::Tokens;
use ui::shell::COLLAPSED_RAIL;
use ui::type_::sans;

use crate::sync::Shared;
use crate::{Edge, State};

pub const KEY: &str = "theme-picker";
const RAIL_GUTTER: f32 = 8.0;

#[derive(Default)]
pub struct Picker {
    pub query: TextEdit,
    /// The highlighted row among the filtered ones.
    pub focus: usize,
}

/// One picker row: a bundled theme.
struct Row {
    tokens: &'static Tokens,
    family: &'static str,
}

/// `THEME_PICKER_ROWS` filtered by `query` (label, family or id), grouped by family in the
/// web client's order: each family's dark theme, then its light one.
fn rows(query: &str) -> Vec<Row> {
    let q = query.trim().to_lowercase();
    let all = theme::all();
    let mut out = Vec::new();
    for dark in all.iter().filter(|t| t.dark) {
        let light = all.iter().find(|t| !t.dark && t.family == dark.family);
        for tokens in std::iter::once(dark).chain(light) {
            let hit = q.is_empty()
                || tokens.name.to_lowercase().contains(&q)
                || tokens.family.to_lowercase().contains(&q)
                || tokens.id.contains(q.as_str());
            if hit {
                out.push(Row {
                    tokens,
                    family: dark.name.as_str(),
                });
            }
        }
    }
    out
}

fn swatches(t: &Tokens, of: &Tokens) -> El {
    let c = &of.colors;
    row()
        .gap(2.0)
        .a11y_hidden()
        .children([c.background, c.primary, c.accent, c.success].map(|fill| {
            el().size(10.0, 10.0)
                .radius(5.0)
                .fill(fill)
                .border(1.0, t.colors.border.alpha(0.6))
        }))
}

pub fn build(s: &State, t: &'static Tokens, phone: bool, layer: &mut Vec<El>) {
    if !s.overlays.is_open(KEY) {
        return;
    }
    let c = &t.colors;
    let applied = theme::theme_id(s.theme, s.dark);
    let x = theme::icons::named("X").expect("bundled icon");
    let palette = theme::icons::named("Palette").expect("bundled icon");
    let search = theme::icons::named("Search").expect("bundled icon");
    let check = theme::icons::named("Check").expect("bundled icon");
    let header = row()
        .key("picker-header")
        .w_full()
        .px(12.0)
        .py(10.0)
        .gap(8.0)
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(icon(palette, 16.0).color(c.primary).a11y_hidden())
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .child(
                    text("Color Themes")
                        .font(sans(14.0, 500.0, 14.0))
                        .color(c.foreground)
                        .role("heading"),
                )
                .child(
                    text(if phone {
                        "Tap a theme to apply it"
                    } else {
                        "Hover to preview · Enter to apply"
                    })
                    .font(sans(12.0, 400.0, 16.0))
                    .color(c.muted_foreground)
                    .mt(2.0),
                ),
        )
        .child(
            row()
                .key("picker-close")
                .role("button")
                .label("Close theme picker")
                .size(28.0, 28.0)
                .center()
                .radius(6.0)
                .hover(c.secondary)
                .child(icon(x, 14.0).color(c.muted_foreground))
                .on_press(|s: &mut State| s.close(KEY)),
        );
    let field = text_input(&s.picker.query, sans(14.0, 400.0, 20.0))
        .single_line()
        .key("picker-search")
        .label("Search themes")
        .placeholder("Search themes…".to_owned(), c.muted_foreground)
        .color(c.foreground)
        .w_full()
        .pl(32.0)
        .pr(12.0)
        .py(7.0)
        .radius(6.0)
        .fill(c.secondary.alpha(0.5))
        .border(1.0, c.border)
        .focus_ring(1.0, 0.0, c.focus_ring)
        .autofocus()
        .on_edit(|s: &mut State| {
            s.picker.focus = 0;
            &mut s.picker.query
        });
    let search_band = col()
        .key("picker-search-band")
        .w_full()
        .px(12.0)
        .py(8.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(
            el().w_full().child(field).child(
                el().absolute(10.0, 10.0)
                    .child(icon(search, 14.0).color(c.muted_foreground))
                    .a11y_hidden(),
            ),
        );
    let rows = rows(&s.picker.query.text);
    let mut list = col()
        .key("picker-list")
        .role("listbox")
        .label("Themes")
        .w_full()
        .padding(8.0);
    if rows.is_empty() {
        list = list.child(
            text("No themes match.")
                .font(sans(14.0, 400.0, 20.0))
                .color(c.muted_foreground)
                .w_full()
                .py(24.0)
                .align(rune::Align::Center),
        );
    }
    let mut family = "";
    for (i, r) in rows.iter().enumerate() {
        if r.family != family {
            family = r.family;
            list = list.child(
                text(family.to_owned())
                    .font(sans(12.0, 500.0, 16.0))
                    .color(c.muted_foreground)
                    .px(8.0)
                    .pb(4.0)
                    .mt(if i == 0 { 0.0 } else { 12.0 }),
            );
        }
        let id = r.tokens.id;
        let lit = i == s.picker.focus.min(rows.len().saturating_sub(1));
        let mut item = row()
            .key(format!("theme-{id}"))
            .role("option")
            .label(r.tokens.name.clone())
            .focus_selected(lit)
            .w_full()
            .min_h(if phone { 44.0 } else { 0.0 })
            .px(10.0)
            .py(8.0)
            .gap(10.0)
            .items_center()
            .radius(6.0)
            .child(swatches(t, r.tokens))
            .child(
                text(r.tokens.name.clone())
                    .font(sans(14.0, 500.0, 20.0))
                    .color(c.foreground)
                    .grow(1.0)
                    .truncate(),
            );
        if lit {
            item = item.fill(c.foreground.alpha(0.06));
        }
        if id == applied {
            item = item.child(
                icon(check, 14.0)
                    .color(c.primary)
                    .label("Currently applied"),
            );
        }
        list = list.child(
            item.on_hover_change(move |s: &mut State, on: bool, _tx: &mut rune::Tx| {
                if on {
                    s.picker.focus = i;
                    s.preview = Some(id);
                }
            })
            .on_press(move |s: &mut State| s.apply_theme(id)),
        );
    }
    let mut panel = col()
        .key("theme-picker-panel")
        .role("dialog")
        .label("Color theme picker")
        .radius(12.0)
        .clip_rounded(12.0)
        .fill(c.popover.alpha(0.95))
        .border(1.0, c.border)
        .child(header)
        .child(search_band)
        .child(
            col()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .child(list)
                .scroll(rune::ScrollSpec {
                    stick_bottom: false,
                }),
        );
    if !phone {
        panel = panel.child(
            text("Esc cancel · Enter apply")
                .font(sans(12.0, 400.0, 16.0))
                .color(c.muted_foreground)
                .w_full()
                .px(12.0)
                .py(8.0)
                .border_sides([1.0, 0.0, 0.0, 0.0], c.border),
        );
    }
    let panel = ui::paint::shadows(panel, &t.shadows.elevation_2xl)
        .modal(true, |s: &mut State| s.close(KEY));
    let left = if s.layout.record.rail_expanded {
        s.width(Edge::Rail) as f32
    } else {
        COLLAPSED_RAIL
    } + RAIL_GUTTER;
    let placed = if phone {
        panel.pin(Some(16.0), Some(16.0), Some(16.0), Some(16.0))
    } else {
        panel.pin(Some(left), Some(16.0), None, Some(16.0)).w(320.0)
    };
    layer.push(
        el().key("theme-picker-layer")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .child(placed),
    );
}

impl State {
    pub fn open_theme_picker(&mut self) {
        self.picker = Picker::default();
        let applied = theme::theme_id(self.theme, self.dark);
        self.picker.focus = rows("")
            .iter()
            .position(|r| r.tokens.id == applied)
            .unwrap_or(0);
        self.overlays.open(KEY);
    }

    /// The picker closed without applying: the saved theme shows again.
    pub(crate) fn picker_closed(&mut self) {
        self.preview = None;
        self.picker.query = TextEdit::new("");
    }

    /// Up or Down moves the highlight and previews it; Return applies it.
    pub(crate) fn picker_key(&mut self, step: Option<isize>) {
        let rows = rows(&self.picker.query.text);
        if rows.is_empty() {
            return;
        }
        let at = self.picker.focus.min(rows.len() - 1);
        match step {
            Some(d) => {
                let next = (at as isize + d).clamp(0, rows.len() as isize - 1) as usize;
                self.picker.focus = next;
                self.preview = Some(rows[next].tokens.id);
            }
            None => self.apply_theme(rows[at].tokens.id),
        }
    }

    /// `getPickerApplySettings`: a `-light` id saves its family with the light appearance.
    pub fn apply_theme(&mut self, id: &str) {
        let Some(tokens) = theme::by_id(id) else {
            return;
        };
        self.theme = tokens.family.as_str();
        self.dark = tokens.dark;
        self.close(KEY);
        let settings = &mut self.sync.app_settings;
        settings.insert("colorTheme".into(), Value::from(tokens.family.clone()));
        settings.insert(
            "appearanceMode".into(),
            Value::from(if tokens.dark { "dark" } else { "light" }),
        );
        let data = Value::Object(settings.clone());
        self.write_shared(Shared::AppSettings, data);
    }

    /// A shared key was read: the theme from `settings/app`, the bindings from
    /// `settings/keyboard-shortcuts`.
    pub(crate) fn shared_loaded(&mut self, shared: Shared, value: Option<Value>) {
        match shared {
            Shared::AppSettings => {
                let map = match value {
                    Some(Value::Object(map)) => map,
                    _ => serde_json::Map::new(),
                };
                if let Some(family) = map.get("colorTheme").and_then(Value::as_str) {
                    let tokens = theme::tokens(family, true);
                    self.theme = tokens.family.as_str();
                }
                self.dark = map.get("appearanceMode").and_then(Value::as_str) != Some("light");
                if let Some(k) = map.get("uiZoomLevel").and_then(Value::as_f64) {
                    self.zoom = ui::Zoom::new(k as f32);
                }
                self.sync.app_settings = map;
            }
            Shared::Shortcuts => self.shortcuts_loaded(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::rows;

    #[test]
    fn the_rows_pair_each_family_dark_then_light() {
        let all = rows("");
        assert_eq!(all.len(), 20);
        assert_eq!(
            (all[0].tokens.id, all[1].tokens.id),
            ("termul", "termul-light")
        );
        assert_eq!(all[1].family, "Troupe");
        let nord: Vec<&str> = rows("nord")
            .iter()
            .map(|r| r.tokens.name.as_str())
            .collect();
        assert_eq!(nord, ["Nord", "Nord Light"]);
    }
}
