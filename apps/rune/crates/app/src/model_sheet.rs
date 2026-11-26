//! The phone chat's model sheet (`MobileModelSheet`, Paper "2.6e"): the Provider row (the
//! agent the next send runs on, a sub-view of every agent), the Mode row (a sub-view of the
//! chat's modes), and the model list with a check on the current one. Picking another agent
//! arms the switch as the desktop menu does; a busy chat answers with Wait or Cancel turn and
//! switch under the list.

use rune::{El, Tx, col, icon, row, text};
use theme::Tokens;
use theme::icons as i;
use ui::composer::agent_menu::SwitchAct;
use ui::type_::sans;

use crate::State;
use crate::composer::Host;
use crate::composer_sheet::{group, well};

/// The sheet's overlay key, and its two sub-views' above it.
pub const MODEL_SHEET: &str = "composer-model-sheet";
pub const MODEL_PROVIDERS: &str = "composer-model-providers";
pub const MODEL_MODES: &str = "composer-model-modes";

impl State {
    /// The phone chip of `host`'s chat: its model sheet.
    pub(crate) fn open_model_sheet(&mut self, host: &Host) {
        log::info!("[composer] model sheet for {}", host.key());
        self.composer.sheet_host = Some(host.clone());
        self.overlays.open(MODEL_SHEET);
    }

    pub(crate) fn close_model_sheet(&mut self) {
        for key in [MODEL_MODES, MODEL_PROVIDERS, MODEL_SHEET] {
            self.close(key);
        }
    }
}

/// `SheetRow` with a value and an accessory: a check on the picked row, a chevron for a
/// sub-view.
fn choice_row(t: &Tokens, key: String, title: El, label: &str, value: Option<El>, accessory: Option<&'static rune::Icon>) -> El {
    let c = &t.colors;
    let mut e = row()
        .key(key)
        .role("button")
        .label(label.to_owned())
        .w_full()
        .min_h(52.0)
        .py(8.0)
        .pl(14.0)
        .pr(12.0)
        .gap(12.0)
        .items_center()
        .hover(c.foreground.alpha(0.04))
        .child(col().grow(1.0).min_w(0.0).child(title));
    if let Some(value) = value {
        e = e.child(value);
    }
    if let Some(glyph) = accessory {
        e = e.child(icon(glyph, 16.0).color(if std::ptr::eq(glyph, &i::CHECK) { c.foreground } else { c.muted_foreground }));
    }
    e
}

fn title(t: &Tokens, words: &str) -> El {
    text(words.to_owned()).font(sans(15.0, 400.0, 20.0)).color(t.colors.foreground).single_line().truncate()
}

fn hint(t: &Tokens, words: String) -> El {
    row().w_full().px(12.0).child(text(words).font(sans(12.0, 400.0, 16.0)).color(t.colors.muted_foreground))
}

fn switch_act(act: SwitchAct) -> impl Fn(&mut State, &mut Tx) + Clone {
    move |s: &mut State, tx: &mut Tx| {
        if let Some(host) = s.composer.sheet_host.clone() {
            s.close(MODEL_PROVIDERS);
            s.composer_switch(&host, act.clone(), tx);
        }
    }
}

fn root_view(s: &State, t: &'static Tokens, host: &Host) -> Vec<El> {
    let c = &t.colors;
    let mut out = Vec::new();
    let switch = s.switch_view(host);
    if let Some(sw) = &switch {
        let shown = sw.agents.iter().find(|a| a.picked).map_or("Agent", |a| a.name.as_str());
        let provider = col()
            .child(text("Provider").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground))
            .child(title(t, shown));
        let count = format!("{} available", sw.agents.len());
        let many = sw.agents.len() > 1;
        let mut r = row()
            .key("model-sheet-provider")
            .role("button")
            .label(format!("Provider, {shown}"))
            .w_full()
            .min_h(60.0)
            .py(8.0)
            .pl(14.0)
            .pr(12.0)
            .gap(12.0)
            .items_center()
            .child(well(t, &i::BOT))
            .child(provider.grow(1.0).min_w(0.0));
        if many {
            r = r
                .child(text(count).font(sans(15.0, 400.0, 20.0)).color(c.muted_foreground).single_line())
                .child(icon(&i::CHEVRON_RIGHT, 16.0).color(c.muted_foreground));
        }
        if many {
            r = r.on_press(|s: &mut State| s.overlays.open(MODEL_PROVIDERS));
        }
        out.push(group(t, vec![r]));
    }
    if let Some(mode) = s.mode_view(host).filter(|m| !m.modes.is_empty()) {
        let name = mode.modes.iter().find(|m| m.id == mode.current).map_or(mode.current.as_str(), |m| m.name.as_str());
        let current = ui::composer::controls::chip_label(&mode.current, name);
        let (risk, glyph) = ui::composer::controls::mode_kind(&mode.current);
        let tint = if risk == ui::composer::controls::Risk::Danger { c.warning } else { c.muted_foreground };
        let value = row()
            .gap(6.0)
            .items_center()
            .child(icon(glyph, 16.0).color(tint))
            .child(text(current.clone()).font(sans(15.0, 400.0, 20.0)).color(tint).single_line());
        let r = choice_row(t, "model-sheet-mode".into(), title(t, "Mode"), &format!("Mode, {current}"), Some(value), Some(&i::CHEVRON_RIGHT))
            .on_press(|s: &mut State| s.overlays.open(MODEL_MODES));
        out.push(group(t, vec![r]));
    }
    let models = s.agent_choices(host);
    if models.is_empty() {
        out.push(hint(t, "This agent gives no model choice.".to_owned()));
    } else {
        let rows = models
            .into_iter()
            .map(|m| {
                let id = m.id.clone();
                choice_row(t, format!("model-sheet-model-{id}"), title(t, &m.name), &m.name, None, m.picked.then_some(&i::CHECK)).on_press_tx(
                    move |s: &mut State, tx: &mut Tx| {
                        if let Some(host) = s.composer.sheet_host.clone() {
                            s.composer_pick_agent(&host, &id, tx);
                        }
                    },
                )
            })
            .collect();
        out.push(group(t, rows));
    }
    match switch.as_ref().map(|sw| (&sw.busy, &sw.armed)) {
        Some((Some((agent, can_cancel)), _)) => {
            let status = if *can_cancel {
                format!("{agent} is working on a turn.")
            } else {
                "This chat is busy. Wait for the queued prompts or the open request to finish.".to_owned()
            };
            out.push(hint(t, status));
            let mut rows = vec![choice_row(t, "model-sheet-wait".into(), title(t, "Wait"), "Wait", None, None).on_press_tx(switch_act(SwitchAct::Wait))];
            if *can_cancel {
                rows.push(
                    choice_row(t, "model-sheet-cancel-turn".into(), title(t, "Cancel turn and switch"), "Cancel turn and switch", None, None)
                        .on_press_tx(switch_act(SwitchAct::CancelTurnAndSwitch)),
                );
            }
            out.push(group(t, rows));
        }
        Some((None, Some(armed))) => out.push(hint(t, format!("Switches to {armed} on the next send"))),
        _ => {}
    }
    out
}

fn providers_view(s: &State, t: &'static Tokens, host: &Host) -> Vec<El> {
    let Some(sw) = s.switch_view(host) else { return Vec::new() };
    let rows = sw
        .agents
        .into_iter()
        .map(|a| {
            choice_row(t, format!("model-sheet-agent-{}", a.id), title(t, &a.name), &a.name, None, a.picked.then_some(&i::CHECK))
                .on_press_tx(switch_act(SwitchAct::Agent(a.id.clone())))
        })
        .collect();
    vec![group(t, rows)]
}

fn modes_view(s: &State, t: &'static Tokens, host: &Host) -> Vec<El> {
    let Some(mode) = s.mode_view(host) else { return Vec::new() };
    let rows = mode
        .modes
        .iter()
        .map(|m| {
            let id = m.id.clone();
            choice_row(t, format!("model-sheet-mode-{id}"), title(t, &m.name), &m.name, None, (m.id == mode.current).then_some(&i::CHECK)).on_press(
                move |s: &mut State| {
                    s.close(MODEL_MODES);
                    if let Some(host) = s.composer.sheet_host.clone() {
                        s.composer_control(&host, ui::composer::controls::ControlAct::Mode(id.clone()));
                    }
                },
            )
        })
        .collect();
    vec![group(t, rows)]
}

/// The sheet, when it is open.
pub fn model_sheet(s: &State, t: &'static Tokens, window: (f32, f32), layer: &mut Vec<El>) {
    let Some(host) = s.composer.sheet_host.clone() else { return };
    let (title, sub, body) = if s.overlays.is_open(MODEL_PROVIDERS) {
        ("Providers", Some(MODEL_PROVIDERS), providers_view(s, t, &host))
    } else if s.overlays.is_open(MODEL_MODES) {
        ("Mode", Some(MODEL_MODES), modes_view(s, t, &host))
    } else {
        ("Select model", None, root_view(s, t, &host))
    };
    let mut sheet = crate::phone::sheet(MODEL_SHEET, title.to_owned(), s)
        .depth(usize::from(sub.is_some()))
        .on_back(move |s: &mut State| {
            if let Some(key) = sub {
                s.close(key);
            }
        })
        .on_close(|s: &mut State| s.close_model_sheet());
    if sub.is_some() {
        sheet = sheet.drag(s.phone.drag);
    }
    for e in body {
        sheet = sheet.child(e);
    }
    sheet.build(t, window, layer);
}
