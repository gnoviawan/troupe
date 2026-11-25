//! `AgentSwitchSeparator`: the divider an agent switch leaves in the transcript. A hairline
//! rule on each side of an `old → new` chip, and under it the handoff summary in a bordered
//! card, open until the reader folds it.

use rune::{El, col, el, icon, row, text};
use serde_json::Value;
use theme::icons as i;

use super::Ctx;
use crate::type_::sans;

const HANDOFF_PREFIX: &str = "# Conversation handoff\n\n";
const PREAMBLE_START: &str = "You are taking over a conversation previously handled by ";
const PREAMBLE_END: &str = " Summary of the prior conversation:";

/// `stripHandoffHeader`: the summary without its wire headline and preamble line.
pub fn strip_handoff_header(text: &str) -> &str {
    let Some(body) = text.strip_prefix(HANDOFF_PREFIX) else {
        return text;
    };
    let Some((first, rest)) = body.split_once("\n\n") else {
        return body;
    };
    let preamble = first.len() > PREAMBLE_START.len() + PREAMBLE_END.len()
        && first.starts_with(PREAMBLE_START)
        && first.ends_with(PREAMBLE_END);
    if preamble { rest } else { body }
}

/// The name a config id shows: the stored config's name, else the id, else `fallback`.
fn config_name(configs: &[model::acp::types::Object], id: &str, fallback: &str) -> String {
    configs
        .iter()
        .find(|c| c.get("id").and_then(Value::as_str) == Some(id))
        .and_then(|c| c.get("name").and_then(Value::as_str))
        .filter(|name| !name.is_empty())
        .or(Some(id).filter(|id| !id.is_empty()))
        .unwrap_or(fallback)
        .to_owned()
}

/// The separator of one switch marker; `key` is its row's key.
pub(super) fn separator<S: 'static>(ctx: &Ctx<S>, s: &S, key: &str, record: &Value) -> El {
    let t = ctx.t;
    let c = &t.colors;
    let field = |k: &str| record.get(k).and_then(Value::as_str).unwrap_or("");
    let configs = &(ctx.lenses.acp)(s).agent_configs;
    let from = config_name(configs, field("fromConfigId"), "previous agent");
    let to = config_name(configs, field("toConfigId"), "another agent");
    let summary = strip_handoff_header(field("summaryText")).trim().to_owned();
    // The reader is shared by every chat and marker ids repeat across chats.
    let folded_key = format!("switch-folded-{}-{key}", ctx.session);
    let open = !summary.is_empty() && !(ctx.lenses.reader)(s).is_tool_open(&folded_key);
    let small = sans(12.0, 400.0, 16.0);
    let rule = || el().h(1.0).grow(1.0).fill(c.border).a11y_hidden();
    let glyph = || icon(&i::BOT, 12.0).color(c.muted_foreground).a11y_hidden();
    let chip = row()
        .gap(6.0)
        .items_center()
        .min_w(0.0)
        .radius(999.0)
        .border(1.0, c.border.alpha(0.6))
        .fill(c.muted.alpha(0.4))
        .px(10.0)
        .py(2.0)
        .child(glyph())
        .child(text(from.clone()).font(small).color(c.muted_foreground).single_line().truncate().max_w(96.0))
        .child(icon(&i::ARROW_RIGHT, 11.0).color(c.muted_foreground.alpha(0.7)).a11y_hidden())
        .child(glyph())
        .child(text(to.clone()).font(sans(12.0, 500.0, 16.0)).color(c.muted_foreground).single_line().truncate().max_w(96.0));
    let mut trigger = row()
        .key(format!("{key}-toggle"))
        .label(format!("Handoff: {from} to {to}"))
        .w_full()
        .min_h(32.0)
        .gap(12.0)
        .items_center()
        .child(rule())
        .child(chip);
    if !summary.is_empty() {
        // A down chevron, not a rotated right one: a rotated icon stops the CPU canvas (pin).
        let chevron = if open { &i::CHEVRON_DOWN } else { &i::CHEVRON_RIGHT };
        let reader = ctx.lenses.reader_mut;
        trigger = trigger
            .role("button")
            .focusable(true)
            .child(icon(chevron, 13.0).color(c.muted_foreground).a11y_hidden())
            .on_press(move |s: &mut S| reader(s).toggle_tool(&folded_key));
    }
    let mut frame = col().w_full().py(4.0).child(trigger.child(rule()));
    if open {
        frame = frame.child(
            col()
                .key(format!("{key}-summary"))
                .w_full()
                .mt(8.0)
                .max_h(160.0)
                .radius(6.0)
                .border(1.0, c.border.alpha(0.5))
                .fill(c.muted.alpha(0.4))
                .scroll(rune::ScrollSpec { stick_bottom: false })
                .child(col().w_full().px(12.0).py(8.0).child(text(summary).font(small).color(c.muted_foreground).wrap().min_w(0.0))),
        );
    }
    frame
}
