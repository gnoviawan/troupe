//! The chat composer's own controls: the mode chip right of `+` with its menu (`ModeChip`,
//! `ModeMenuList`) and the context ring on the field's first line with its card
//! (`ContextUsageIndicator`, `ContextUsageDetails`).

use std::rc::Rc;

use rune::{El, col, el, icon, row, text};
use theme::Tokens;
use theme::icons as i;

use crate::overlay::{Placement, Popover};
use crate::type_::sans;

/// How much a mode lets the agent do without asking (`ModeRisk`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Risk {
    Ask,
    Act,
    Danger,
    Other,
}

/// `MODE_KINDS`: the known modes' risk and glyph.
pub fn mode_kind(id: &str) -> (Risk, &'static rune::Icon) {
    match id {
        "default" => (Risk::Ask, &i::HAND),
        "plan" => (Risk::Ask, &i::MAPS),
        "ask" => (Risk::Ask, &i::MESSAGE_QUESTION),
        "read-only" => (Risk::Ask, &i::EYE),
        "acceptEdits" => (Risk::Act, &i::FILE_EDIT),
        "auto" => (Risk::Act, &i::SPARKLES),
        "agent" => (Risk::Act, &i::BOT),
        "workspace-write" => (Risk::Act, &i::FOLDER_EDIT),
        "bypassPermissions" | "agent-full-access" => (Risk::Danger, &i::SHIELD_ALERT),
        _ => (Risk::Other, &i::BOT),
    }
}

/// One mode of the menu.
pub struct ModeRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

/// The chat's modes, the current one, and whether the menu is open.
pub struct ModeView {
    pub current: String,
    pub modes: Vec<ModeRow>,
    pub agent_name: Option<String>,
    pub open: bool,
}

/// The context window the agent reports (`SessionUsage`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UsageView {
    pub used: f64,
    pub size: f64,
    pub baseline: f64,
    pub open: bool,
}

/// A press on a control: open or close a surface, or pick a mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlAct {
    ModeMenu(bool),
    Mode(String),
    Usage(bool),
}

pub type OnControl<S> = Rc<dyn Fn(&mut S, ControlAct)>;

/// `modeChipLabel`.
fn chip_label(id: &str, name: &str) -> String {
    if id == "default" { "Ask first".into() } else { name.to_owned() }
}

fn group_label(risk: Risk, agent: Option<&str>) -> String {
    match risk {
        Risk::Ask => "Ask first".into(),
        Risk::Act => agent.map_or_else(|| "Let the agent act".into(), |a| format!("Let {a} act")),
        Risk::Danger => "Use with care".into(),
        Risk::Other => "Other modes".into(),
    }
}

/// `ModeChip` with `ModeMenuList`: the current mode's glyph and label; the menu groups the
/// modes by risk, each with its glyph, name and description, the picked one tinted.
pub fn mode_chip<S: 'static>(t: &'static Tokens, key: &str, view: &ModeView, on: &OnControl<S>, layer: &mut Vec<El>) -> El {
    let c = &t.colors;
    let current = view.modes.iter().find(|m| m.id == view.current);
    let (risk, glyph) = mode_kind(&view.current);
    let label = chip_label(&view.current, current.map_or("Agent", |m| m.name.as_str()));
    let danger = risk == Risk::Danger;
    let toggle = on.clone();
    let open = view.open;
    let mut chip = row()
        .key(format!("{key}-mode"))
        .role("button")
        .label(label.clone())
        .h(28.0)
        .px(if danger { 10.0 } else { 6.0 })
        .gap(6.0)
        .items_center()
        .radius(8.0)
        .hover(c.foreground.alpha(0.06))
        .child(icon(glyph, 14.0).color(if danger { c.warning } else { c.muted_foreground }))
        .child(text(label).font(sans(12.0, 400.0, 16.0)).color(if danger { c.warning } else { c.muted_foreground }))
        .on_press(move |s: &mut S| toggle(s, ControlAct::ModeMenu(!open)));
    if danger {
        chip = chip.fill(c.warning.alpha(0.12));
    }
    let groups = [Risk::Ask, Risk::Act, Risk::Danger, Risk::Other];
    let present: Vec<Risk> = groups.into_iter().filter(|r| view.modes.iter().any(|m| mode_kind(&m.id).0 == *r)).collect();
    let mut list = col().key(format!("{key}-mode-menu")).w_full().role("menu").label("Mode");
    for (n, group) in present.iter().enumerate() {
        if *group == Risk::Danger && n > 0 {
            list = list.child(el().h(0.5).mx(4.0).my(4.0).fill(c.border));
        }
        if present.len() > 1 && *group != Risk::Danger {
            list = list.child(
                text(group_label(*group, view.agent_name.as_deref()))
                    .font(sans(12.0, 500.0, 16.0))
                    .color(c.muted_foreground)
                    .px(8.0)
                    .pt(6.0)
                    .pb(4.0),
            );
        }
        for mode in view.modes.iter().filter(|m| mode_kind(&m.id).0 == *group) {
            let (_, glyph) = mode_kind(&mode.id);
            let picked = mode.id == view.current;
            let description = match (group, mode.description.as_deref().map(str::trim).filter(|d| !d.is_empty())) {
                (Risk::Danger, None) => Some("Use with care.".to_owned()),
                (Risk::Danger, Some(d)) => {
                    Some(format!("{d}{} Use with care.", if d.ends_with(['.', '!', '?']) { "" } else { "." }))
                }
                (_, d) => d.map(str::to_owned),
            };
            let mut words = col().grow(1.0).min_w(0.0).child(
                text(mode.name.clone()).font(sans(12.0, 400.0, 16.0)).color(c.foreground).single_line().truncate(),
            );
            if let Some(d) = description {
                words = words.child(text(d).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).wrap());
            }
            let (pick, id) = (on.clone(), mode.id.clone());
            let mut item = row()
                .key(format!("{key}-mode-{}", mode.id))
                .role("menuitemradio")
                .label(mode.name.clone())
                .w_full()
                .gap(8.0)
                .px(8.0)
                .py(6.0)
                .items_start()
                .radius(6.0)
                .hover(c.foreground.alpha(0.06))
                .focusable(true)
                .child(icon(glyph, 16.0).color(if *group == Risk::Danger { c.warning } else { c.muted_foreground }))
                .child(words)
                .on_press(move |s: &mut S| pick(s, ControlAct::Mode(id.clone())));
            if picked {
                item = item.fill(c.foreground.alpha(0.1));
            }
            list = list.child(item);
        }
    }
    let close = on.clone();
    Popover::new(format!("{key}-mode-popover"), chip)
        .label("Mode")
        .child(list)
        .open(view.open)
        .placement(Placement::TopStart)
        .width(300.0)
        .padding(4.0)
        .on_close(move |s: &mut S| close(s, ControlAct::ModeMenu(false)))
        .build(t, layer)
}

/// `formatTokenCount`: whole tokens under a thousand, else one decimal with K or M.
pub fn token_count(value: f64) -> String {
    if !value.is_finite() || value < 0.0 {
        return "0".into();
    }
    if value < 1_000.0 {
        return format!("{}", value.round());
    }
    let (n, unit) = if value < 1_000_000.0 { (value / 1_000.0, "K") } else { (value / 1_000_000.0, "M") };
    let n = (n * 10.0).round() / 10.0;
    if n.fract() == 0.0 { format!("{n:.0}{unit}") } else { format!("{n:.1}{unit}") }
}

impl UsageView {
    /// `displayedUsagePercent`.
    pub fn percent(&self) -> f64 {
        if self.size > 0.0 { (self.used.max(0.0) / self.size * 100.0).clamp(0.0, 100.0) } else { 0.0 }
    }

    /// `shouldShowSessionUsage` past the user-message check: a real report whose
    /// conversation part rounds to one percent or more.
    pub fn shown(&self) -> bool {
        if !(self.used.is_finite() && self.size.is_finite() && self.baseline.is_finite()) || self.size <= 0.0 || self.used <= 0.0 {
            return false;
        }
        let baseline = self.baseline.min(self.used).min(self.size);
        let (used, size) = ((self.used - baseline).max(0.0), (self.size - baseline).max(0.0));
        used > 0.0 && size > 0.0 && (used / size * 100.0).round() >= 1.0
    }
}

/// `ContextUsageIndicator`: a 16-point ring (the share used, clockwise from the top) in a
/// 32-point button; it opens the context card.
pub fn context_ring<S: 'static>(t: &'static Tokens, key: &str, usage: &UsageView, on: &OnControl<S>, layer: &mut Vec<El>) -> El {
    let c = &t.colors;
    let percent = usage.percent();
    let warning = percent.round() >= 80.0;
    let ink = if warning { c.warning } else { c.muted_foreground };
    let circumference = std::f32::consts::PI * 14.0;
    let filled = (percent as f32 / 100.0 * circumference).clamp(0.01, circumference - 0.01);
    let ring = el()
        .size(14.0, 14.0)
        .radius(7.0)
        .border(2.0, ink.alpha(0.22))
        .child(
            el()
                .size(14.0, 14.0)
                .radius(7.0)
                .border(2.0, ink)
                .border_dash(filled, circumference - filled)
                .pin(Some(0.0), Some(0.0), None, None),
        );
    let size = usage.size.max(0.0);
    let used = usage.used.max(0.0).min(size);
    let label = format!("Context {}%, {} of {} tokens{}", percent.round(), token_count(used), token_count(size), if warning { ", nearing the limit" } else { "" });
    let (toggle, open) = (on.clone(), usage.open);
    let button = row()
        .key(format!("{key}-context"))
        .role("button")
        .label(label)
        .size(32.0, 32.0)
        .center()
        .child(ring)
        .on_press(move |s: &mut S| toggle(s, ControlAct::Usage(!open)));
    let setup = usage.baseline.max(0.0).min(used);
    let conversation = used - setup;
    let fraction = |n: f64| if size > 0.0 { (n / size) as f32 } else { 0.0 };
    let bar = row()
        .w_full()
        .h(6.0)
        .gap(2.0)
        .radius(3.0)
        .fill(c.muted)
        .child(el().h_full().w_frac(fraction(setup)).radius(3.0).fill(c.foreground.alpha(0.35)))
        .child(el().h_full().w_frac(fraction(conversation)).radius(3.0).fill(if warning { c.warning } else { c.foreground }));
    let line = |dot: rune::Color, name: &str, n: f64| {
        row()
            .w_full()
            .gap(8.0)
            .items_center()
            .child(el().size(6.0, 6.0).radius(3.0).fill(dot))
            .child(text(name.to_owned()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).grow(1.0))
            .child(text(token_count(n)).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground))
    };
    let card = col()
        .key(format!("{key}-context-card"))
        .w_full()
        .gap(10.0)
        .child(
            row()
                .w_full()
                .items_center()
                .child(text("Context window").font(sans(12.0, 500.0, 16.0)).color(c.foreground).grow(1.0))
                .child(text(format!("{}% used", percent.round())).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground)),
        )
        .child(bar)
        .child(
            col()
                .w_full()
                .gap(4.0)
                .child(line(c.foreground.alpha(0.35), "Setup", setup))
                .child(line(if warning { c.warning } else { c.foreground }, "Conversation", conversation)),
        )
        .child(
            text(format!("{} left", token_count(size - used)))
                .font(sans(12.0, 400.0, 16.0))
                .color(c.muted_foreground),
        );
    let close = on.clone();
    Popover::new(format!("{key}-context-popover"), button)
        .label("Context window")
        .child(card)
        .open(usage.open)
        .placement(Placement::BottomStart)
        .width(224.0)
        .padding(12.0)
        .on_close(move |s: &mut S| close(s, ControlAct::Usage(false)))
        .build(t, layer)
}
