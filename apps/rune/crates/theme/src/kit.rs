//! Troupe's tokens on rune-kit's semantic schema. `Kit::from_palette` derives the slots Troupe has
//! no token for from the theme's canvas, ink and brand fill; every slot Troupe does name takes its
//! token. The base components (a later unit) refine this against the web client's computed styles.

use std::sync::OnceLock;

use rune::{Spring, TextStyle};
use rune_kit::{Kit, PaletteSeed, Radius, Type};

use crate::{INTER_FEATURES, Tokens};

/// The kit for `family` in one appearance, built once per theme.
pub fn kit(family: &str, dark: bool) -> &'static Kit {
    static KITS: OnceLock<Vec<Kit>> = OnceLock::new();
    let kits = KITS.get_or_init(|| crate::all().iter().map(build).collect());
    let t = crate::tokens(family, dark);
    let i = crate::all()
        .iter()
        .position(|x| x.id == t.id)
        .expect("tokens come from all()");
    &kits[i]
}

const fn inter(size: f32, weight: f32, line: f32) -> TextStyle {
    TextStyle::new(size, weight)
        .lh(line)
        .features(INTER_FEATURES)
}

/// Seconds, from a duration token in ms.
fn secs(ms: f32) -> f32 {
    ms / 1000.0
}

pub(crate) fn build(t: &Tokens) -> Kit {
    let c = &t.colors;
    let mut k = Kit::from_palette(PaletteSeed {
        bg: c.background,
        fg: c.foreground,
        accent: c.primary_fill,
    });
    let p = &mut k.c;
    p.page = c.background;
    p.bg_base = c.background;
    p.bg_deep = c.surface_darker;
    p.layer_01 = c.card;
    p.layer_02 = c.popover;
    p.layer_03 = c.muted;
    p.bg_accent = c.primary_fill;
    p.bg_button_neutral = c.control_fill;
    p.text_base = c.foreground;
    p.text_strong = c.foreground;
    p.text_weak = c.secondary_foreground;
    p.text_muted = c.muted_foreground;
    p.text_faint = c.disabled_foreground;
    p.text_accent = c.primary;
    p.icon_base = c.foreground;
    p.icon_muted = c.muted_foreground;
    p.icon_faint = c.disabled_foreground;
    p.icon_accent = c.primary;
    p.border_base = c.border;
    p.border_muted = c.border;
    p.border_strong = c.field_border;
    p.border_focus = c.focus_ring;
    p.hover = c.row_hover;
    p.surface_base = c.card;
    p.surface_raised = c.popover;
    p.surface_float = c.popover;
    p.std_border = c.border;
    p.std_text = c.foreground;
    p.danger.fg = c.destructive;
    p.danger.icon = c.destructive;
    p.success.fg = c.success;
    p.success.icon = c.success;
    p.warning.fg = c.warning;
    p.warning.icon = c.warning;
    p.info.fg = c.connection;
    p.info.icon = c.connection;

    let s = &t.shadows;
    k.elevation.raised = s.keycap_shadow.clone();
    k.elevation.floating = s.elevation_lg.clone();
    k.elevation.overlay = s.elevation_xl.clone();
    k.elevation.button_neutral = s.shadow_control.clone();
    k.elevation.button_contrast = s.shadow_emboss.clone();
    k.elevation.md = s.elevation_md.clone();
    k.elevation.knob = s.switch_thumb_shadow.clone();

    // Tailwind's `rounded-*` from `--radius` (tailwind.config.ts).
    let r = t.numbers.radius;
    k.radius = Radius {
        xs: 2.0,
        sm: r - 4.0,
        md: r - 2.0,
        lg: r,
        xl: r + 4.0,
    };
    // Tailwind's text-sm (14/20) and text-xs (12/16), the app's two body sizes.
    k.ty = Type {
        ui: inter(14.0, 400.0, 20.0),
        ui_medium: inter(14.0, 500.0, 20.0),
        ui_compact: inter(14.0, 400.0, 16.0),
        small: inter(12.0, 400.0, 16.0),
        small_medium: inter(12.0, 500.0, 16.0),
        body: inter(14.0, 400.0, 21.0),
        body_medium: inter(14.0, 500.0, 21.0),
        text_small: inter(13.0, 400.0, 19.5),
        text_small_medium: inter(13.0, 500.0, 19.5),
        text_xsmall: inter(12.0, 400.0, 18.0),
        text_xsmall_medium: inter(12.0, 500.0, 18.0),
        mono: TextStyle::new(13.0, 400.0).mono().lh(20.0),
    };
    let n = &t.numbers;
    k.motion.control = Spring::new(secs(n.duration_quick), 0.0);
    k.motion.overlay = Spring::new(secs(n.tt_in_dur), 0.0);
    k.motion.disclosure = Spring::new(secs(n.acc_expand), 0.0);
    k.blend_over = Some(c.background);
    k
}
