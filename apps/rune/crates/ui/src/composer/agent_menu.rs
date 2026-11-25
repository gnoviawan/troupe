//! The composer's agent chip and its menu (`AgentModelSelector`). The launcher's menu picks
//! the agent a new chat starts on. A chat's menu lists the models of the agent its next send
//! runs on, then every agent: picking another one arms the switch (`armAgentSwitch`), and the
//! next send runs it. An armed chip reads `→ name` with a cancel button beside it, and a busy
//! chat answers a pick with Wait or Cancel turn and switch (`BusyBlock`).

use std::rc::Rc;

use rune::{El, Tx, icon, row, text};
use theme::Tokens;
use theme::icons as i;

use super::view::{AgentChoice, Hooks, Place, Spec};
use crate::overlay::{Menu, MenuItem, Placement};
use crate::type_::sans;

pub type OnSwitch<S> = Rc<dyn Fn(&mut S, SwitchAct, &mut Tx)>;

/// A press in a chat's agent menu or on its cancel button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitchAct {
    /// An agent row, by config id.
    Agent(String),
    /// Drop the armed switch: the next send stays with the chat's agent.
    Cancel,
    /// Leave a busy chat's pick for later.
    Wait,
    /// Stop the running turn, then arm the pick.
    CancelTurnAndSwitch,
    /// The phone's chip opens the model sheet instead of the menu.
    OpenSheet,
}

/// A chat's agent switch, as its menu shows it.
pub struct SwitchView {
    /// Every agent; `picked` is the one the next send runs on.
    pub agents: Vec<AgentChoice>,
    /// The armed target's name.
    pub armed: Option<String>,
    /// A pick the busy chat holds: the chat's agent name, and whether cancelling its turn
    /// clears the way (nothing but the turn blocks).
    pub busy: Option<(String, bool)>,
}

fn chip<S: 'static>(t: &'static Tokens, spec: &Spec, label: &str, hooks: &Hooks<S>) -> El {
    let c = &t.colors;
    let agent = hooks.agent.clone();
    let armed = spec.switch.as_ref().and_then(|sw| sw.armed.as_ref()).is_some();
    let ink = if armed { c.foreground } else { c.muted_foreground };
    let mut chip = row()
        .key(format!("{}-agent", spec.key))
        .h(28.0)
        .px(6.0)
        .gap(6.0)
        .items_center()
        .min_w(0.0)
        .radius(8.0)
        .role("button")
        .label(label.to_owned())
        .hover(c.foreground.alpha(0.06))
        .child(icon(&i::BOT, 12.0).color(c.muted_foreground))
        .child(text(label.to_owned()).font(sans(12.0, 400.0, 16.0)).color(ink).single_line().truncate().min_w(0.0));
    if !armed {
        chip = chip.child(icon(&i::CHEVRON_DOWN, 12.0).color(c.muted_foreground));
    }
    if phone_sheet(spec) {
        let switch = hooks.switch.clone();
        return chip.on_press_tx(move |s: &mut S, tx: &mut Tx| switch(s, SwitchAct::OpenSheet, tx));
    }
    chip.on_press(move |s: &mut S| agent(s, true))
}

/// A phone chat's chip opens the model sheet (`MobileModelSheet`), not a menu.
fn phone_sheet(spec: &Spec) -> bool {
    spec.place == Place::Phone && spec.switch.is_some()
}

/// The chip, its menu, and the cancel button of an armed switch.
pub(super) fn agent_chip<S: 'static>(t: &'static Tokens, spec: &Spec, label: &str, hooks: &Hooks<S>, layer: &mut Vec<El>) -> El {
    let trigger = chip(t, spec, label, hooks);
    let menu = if (spec.agents.is_empty() && spec.switch.is_none()) || phone_sheet(spec) {
        trigger
    } else {
        menu(spec, hooks).build(t, layer, trigger)
    };
    if spec.switch.as_ref().and_then(|sw| sw.armed.as_ref()).is_none() {
        return menu;
    }
    let switch = hooks.switch.clone();
    let cancel = row()
        .key(format!("{}-agent-switch-cancel", spec.key))
        .w(22.0)
        .h(22.0)
        .center()
        .radius(4.0)
        .role("button")
        .label("Cancel agent switch")
        .hover(t.colors.foreground.alpha(0.1))
        .child(icon(&i::X, 10.0).color(t.colors.muted_foreground).a11y_hidden())
        .on_press_tx(move |s: &mut S, tx: &mut Tx| switch(s, SwitchAct::Cancel, tx));
    row().gap(2.0).items_center().min_w(0.0).child(menu).child(cancel)
}

fn menu<S: 'static>(spec: &Spec, hooks: &Hooks<S>) -> Menu {
    let mut menu = Menu::new(format!("{}-agent-menu", spec.key))
        .name("Agent")
        .width(240.0)
        .placement(if spec.place == Place::Phone { Placement::TopStart } else { Placement::TopEnd });
    let switch = spec.switch.as_ref();
    if switch.is_some() && !spec.agents.is_empty() {
        menu = menu.label("Models");
    }
    for choice in &spec.agents {
        let (pick, id) = (hooks.pick_agent.clone(), choice.id.clone());
        menu = menu.item(
            MenuItem::new(choice.name.clone())
                .checked(choice.picked)
                .on_select_tx(move |s: &mut S, tx: &mut Tx| pick(s, &id, tx)),
        );
    }
    let close = hooks.agent.clone();
    let menu = menu.on_close(move |s: &mut S| close(s, false)).open(spec.agent_open);
    match switch {
        Some(sw) => switch_rows(menu, sw, !spec.agents.is_empty(), hooks),
        None => menu,
    }
}

fn switch_rows<S: 'static>(mut menu: Menu, sw: &SwitchView, after_models: bool, hooks: &Hooks<S>) -> Menu {
    if after_models {
        menu = menu.separator();
    }
    menu = menu.label("Agents");
    let act = |a: SwitchAct| {
        let switch = hooks.switch.clone();
        move |s: &mut S, tx: &mut Tx| switch(s, a.clone(), tx)
    };
    for agent in &sw.agents {
        menu = menu.item(
            MenuItem::new(agent.name.clone())
                .icon(&i::BOT)
                .checked(agent.picked)
                .on_select_tx(act(SwitchAct::Agent(agent.id.clone()))),
        );
    }
    match (&sw.busy, &sw.armed) {
        (Some((agent, can_cancel)), _) => {
            let status = if *can_cancel {
                format!("{agent} is working on a turn.")
            } else {
                "This chat is busy. Wait for the queued prompts or the open request to finish.".to_owned()
            };
            menu = menu.separator().label(status).item(MenuItem::new("Wait").on_select_tx(act(SwitchAct::Wait)));
            if *can_cancel {
                menu = menu.item(MenuItem::new("Cancel turn and switch").on_select_tx(act(SwitchAct::CancelTurnAndSwitch)));
            }
            menu
        }
        (None, Some(armed)) => menu.separator().label(format!("Switches to {armed} on the next send")),
        (None, None) => menu,
    }
}
