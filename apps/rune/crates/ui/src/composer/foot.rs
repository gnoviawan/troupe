//! The working copy under a composer's card: a running chat's read-only label, the
//! launcher's checkout pick and branch in the desktop foot row, and the phone launcher's
//! worktree strip.

use rune::{El, icon, row, text};
use theme::Tokens;
use theme::icons as i;

use crate::type_::sans;

/// The working copy a composer shows under its card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Foot {
    /// A running chat's, read-only (`ComposerWorktreeLabel`): "Current checkout · branch".
    Chat { branch: Option<String> },
    /// The launcher's context strip (`LauncherContextStrip`): the checkout pick, then the
    /// project's branch; on a phone it is the 40-point strip under the card (Paper "2.6g").
    Launcher { branch: Option<String> },
}

/// The desktop foot row's working copy: a chat's read-only label, or the launcher's
/// "Current checkout" pick (`ROW_TRIGGER_CLASS`) and its branch (`ContextRowBranch`).
pub(super) fn worktree(t: &Tokens, foot: &Foot) -> El {
    let c = &t.colors;
    match foot {
        Foot::Chat { branch } => {
            let label = match branch {
                Some(b) => format!("Current checkout · {b}"),
                None => "Current checkout".to_owned(),
            };
            row()
                .key("worktree")
                .h(28.0)
                .px(6.0)
                .gap(8.0)
                .items_center()
                .min_w(0.0)
                .child(icon(&i::FOLDER, 14.0).color(c.muted_foreground))
                .child(text(label).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).truncate())
        }
        Foot::Launcher { branch } => {
            let pick = row()
                .key("worktree-pick")
                .h(24.0)
                .px(8.0)
                .gap(6.0)
                .items_center()
                .child(icon(&i::FOLDER, 12.0).color(c.secondary_foreground))
                .child(text("Current checkout").font(sans(12.0, 500.0, 16.0)).color(c.secondary_foreground))
                .child(icon(&i::CHEVRON_DOWN, 12.0).color(c.muted_foreground));
            let mut strip = row().key("worktree").h(28.0).gap(2.0).items_center().min_w(0.0).child(pick);
            if let Some(b) = branch {
                strip = strip.child(
                    row()
                        .key("worktree-branch")
                        .h(24.0)
                        .px(8.0)
                        .gap(6.0)
                        .items_center()
                        .min_w(0.0)
                        .child(icon(&i::GIT_BRANCH, 12.0).color(c.muted_foreground))
                        .child(text(b.clone()).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).truncate()),
                );
            }
            strip
        }
    }
}

/// The phone launcher's worktree strip (`PHONE_CONTEXT_ROW_CLASS`): "Current checkout" at the
/// start and the branch at the end, 40 points tall, 14-point text and 16-point icons.
pub(super) fn phone_strip(t: &Tokens, key: &str, branch: Option<&str>) -> El {
    let c = &t.colors;
    let item = |key: String, lead: &'static rune::Icon, label: &str, chevron: bool| {
        let mut r = row()
            .key(key)
            .h(40.0)
            .px(8.0)
            .gap(8.0)
            .items_center()
            .min_w(0.0)
            .child(icon(lead, 16.0).color(c.muted_foreground))
            .child(text(label.to_owned()).font(sans(14.0, 400.0, 20.0)).color(c.muted_foreground).truncate());
        if chevron {
            r = r.child(icon(&i::CHEVRON_DOWN, 12.0).color(c.muted_foreground));
        }
        r
    };
    let mut strip = row()
        .key(format!("{key}-strip"))
        .w_full()
        .h(40.0)
        .px(6.0)
        .items_center()
        .justify_between()
        .child(item(format!("{key}-strip-checkout"), &i::FOLDER, "Current checkout", true));
    if let Some(b) = branch {
        strip = strip.child(item(format!("{key}-strip-branch"), &i::GIT_BRANCH, b, false));
    }
    strip
}
