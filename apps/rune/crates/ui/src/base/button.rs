//! `Button` (`components/ui/button.tsx`): seven variants and nine sizes. There is no `primary`
//! variant; `default` is the filled one, and `composer` is the same fill reserved for send, stop
//! and launch.

use rune::{Color, El, Icon, icon, row, text};
use theme::Tokens;

use crate::paint::shadows;
use crate::type_::sans;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    #[default]
    Default,
    Destructive,
    Outline,
    Secondary,
    Ghost,
    Link,
    Composer,
}

impl ButtonVariant {
    pub const ALL: [ButtonVariant; 7] = [
        ButtonVariant::Default,
        ButtonVariant::Secondary,
        ButtonVariant::Outline,
        ButtonVariant::Ghost,
        ButtonVariant::Destructive,
        ButtonVariant::Link,
        ButtonVariant::Composer,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ButtonVariant::Default => "default",
            ButtonVariant::Destructive => "destructive",
            ButtonVariant::Outline => "outline",
            ButtonVariant::Secondary => "secondary",
            ButtonVariant::Ghost => "ghost",
            ButtonVariant::Link => "link",
            ButtonVariant::Composer => "composer",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonSize {
    #[default]
    Default,
    Xs,
    Sm,
    Lg,
    Icon,
    IconXs,
    IconSm,
    IconLg,
    /// The 44 px phone floor; no horizontal padding.
    Touch,
}

impl ButtonSize {
    pub const TEXT: [ButtonSize; 5] = [
        ButtonSize::Xs,
        ButtonSize::Sm,
        ButtonSize::Default,
        ButtonSize::Lg,
        ButtonSize::Touch,
    ];
    pub const ICON: [ButtonSize; 4] = [
        ButtonSize::IconXs,
        ButtonSize::IconSm,
        ButtonSize::Icon,
        ButtonSize::IconLg,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ButtonSize::Default => "default",
            ButtonSize::Xs => "xs",
            ButtonSize::Sm => "sm",
            ButtonSize::Lg => "lg",
            ButtonSize::Icon => "icon",
            ButtonSize::IconXs => "icon-xs",
            ButtonSize::IconSm => "icon-sm",
            ButtonSize::IconLg => "icon-lg",
            ButtonSize::Touch => "touch",
        }
    }

    /// Height, horizontal padding (None for a square icon button), radius and font size.
    fn metrics(self) -> (f32, Option<f32>, f32, f32) {
        match self {
            ButtonSize::Default => (40.0, Some(16.0), 8.0, 14.0),
            ButtonSize::Xs => (28.0, Some(8.0), 6.0, 12.0),
            ButtonSize::Sm => (36.0, Some(12.0), 8.0, 14.0),
            ButtonSize::Lg => (44.0, Some(32.0), 8.0, 14.0),
            ButtonSize::Touch => (44.0, Some(0.0), 8.0, 14.0),
            ButtonSize::Icon | ButtonSize::IconLg => (40.0, None, 8.0, 14.0),
            ButtonSize::IconXs => (24.0, None, 6.0, 14.0),
            ButtonSize::IconSm => (32.0, None, 8.0, 14.0),
        }
    }
}

/// A pointer state drawn at rest, for galleries and docs (`className="bg-primary-fill-hover"`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonPin {
    #[default]
    None,
    Hover,
    Pressed,
}

/// How one variant paints at rest, under the pointer and while pressed.
struct Look {
    fill: Option<Color>,
    hover: Option<Color>,
    press: Option<Color>,
    ink: Color,
    hover_ink: Option<Color>,
    shadow: Vec<rune::Shadow>,
    /// Opacity when disabled; the filled variants swap colours instead (opacity 1).
    disabled_opacity: f32,
    /// The main buttons squeeze to .97 under a press; rows, chips and icons answer with fill.
    press_scale: bool,
}

fn look(t: &Tokens, variant: ButtonVariant, disabled: bool) -> Look {
    let c = &t.colors;
    match variant {
        ButtonVariant::Default | ButtonVariant::Composer if disabled => Look {
            fill: Some(c.muted),
            hover: None,
            press: None,
            ink: c.disabled_foreground,
            hover_ink: None,
            shadow: Vec::new(),
            disabled_opacity: 1.0,
            press_scale: false,
        },
        ButtonVariant::Default | ButtonVariant::Composer => Look {
            fill: Some(c.primary_fill),
            hover: Some(c.primary_fill_hover),
            press: Some(c.primary_fill_active),
            ink: c.primary_foreground,
            hover_ink: None,
            shadow: t.shadows.shadow_emboss.clone(),
            disabled_opacity: 1.0,
            press_scale: true,
        },
        ButtonVariant::Destructive => Look {
            fill: Some(c.destructive_fill),
            // `hover:bg-destructive-fill/90` over the page: a tenth of the background on top.
            hover: Some(c.background.alpha(0.1)),
            press: None,
            ink: c.destructive_foreground,
            hover_ink: None,
            shadow: Vec::new(),
            disabled_opacity: 0.5,
            press_scale: true,
        },
        ButtonVariant::Outline | ButtonVariant::Secondary => Look {
            fill: Some(c.control_fill),
            hover: Some(c.control_fill_hover),
            press: None,
            ink: c.foreground,
            hover_ink: None,
            shadow: t.shadows.shadow_control.clone(),
            disabled_opacity: 0.6,
            press_scale: true,
        },
        ButtonVariant::Ghost => Look {
            fill: None,
            hover: Some(c.secondary),
            press: Some(c.secondary),
            ink: c.foreground,
            hover_ink: Some(c.ghost_hover_foreground),
            shadow: Vec::new(),
            disabled_opacity: 0.5,
            press_scale: false,
        },
        ButtonVariant::Link => Look {
            fill: None,
            hover: None,
            press: None,
            ink: c.primary,
            hover_ink: None,
            shadow: Vec::new(),
            disabled_opacity: 0.5,
            press_scale: false,
        },
    }
}

pub struct Button {
    label: String,
    variant: ButtonVariant,
    size: ButtonSize,
    icon: Option<&'static Icon>,
    leading: Option<El>,
    disabled: bool,
    ring: bool,
    show_label: bool,
    role: &'static str,
    pin: ButtonPin,
    fill: Option<Color>,
    ink: Option<Color>,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Button {
        Button {
            label: label.into(),
            variant: ButtonVariant::Default,
            size: ButtonSize::Default,
            icon: None,
            leading: None,
            disabled: false,
            ring: false,
            show_label: true,
            role: "button",
            pin: ButtonPin::None,
            fill: None,
            ink: None,
        }
    }

    /// A square button that shows only `icon`; `label` is its accessible name.
    pub fn icon_only(icon: &'static Icon, label: impl Into<String>, size: ButtonSize) -> Button {
        Button {
            icon: Some(icon),
            show_label: false,
            size,
            ..Button::new(label)
        }
    }

    pub fn variant(mut self, v: ButtonVariant) -> Self {
        self.variant = v;
        self
    }
    pub fn size(mut self, s: ButtonSize) -> Self {
        self.size = s;
        self
    }
    /// A 16 px glyph before the label.
    pub fn icon(mut self, i: &'static Icon) -> Self {
        self.icon = Some(i);
        self
    }
    /// Any element before the label (a spinner while loading).
    pub fn leading(mut self, e: El) -> Self {
        self.leading = Some(e);
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    /// The 2 px `ring` edge drawn at rest, as `className="ring-2 ring-ring"` forces it.
    pub fn ring(mut self, on: bool) -> Self {
        self.ring = on;
        self
    }
    /// Draw the hover or pressed look at rest.
    pub fn pin(mut self, p: ButtonPin) -> Self {
        self.pin = p;
        self
    }
    /// A call site's own fill (`className="bg-secondary"`).
    pub fn fill(mut self, c: Color) -> Self {
        self.fill = Some(c);
        self
    }
    /// A call site's own ink (`className="text-ghost-hover-foreground"`).
    pub fn ink(mut self, c: Color) -> Self {
        self.ink = Some(c);
        self
    }
    /// `asChild` around an anchor: the same look, read as a link.
    pub fn as_link(mut self) -> Self {
        self.role = "link";
        self
    }

    pub fn build(self, t: &Tokens) -> El {
        let mut look = look(t, self.variant, self.disabled);
        let primary = matches!(self.variant, ButtonVariant::Default | ButtonVariant::Composer);
        match self.pin {
            ButtonPin::None => {}
            ButtonPin::Hover => {
                look.fill = look.hover.or(look.fill);
                if primary {
                    look.shadow = t.shadows.shadow_emboss_hover.clone();
                }
            }
            ButtonPin::Pressed => look.fill = look.press.or(look.fill),
        }
        if let Some(f) = self.fill {
            look.fill = Some(f);
        }
        if let Some(i) = self.ink {
            look.ink = i;
            look.hover_ink = None;
        }
        let (h, px, radius, size) = self.size.metrics();
        let line = if size < 14.0 { 16.0 } else { 20.0 };
        let mut e = row()
            .role(self.role)
            .label(self.label.clone())
            .h(h)
            .gap(8.0)
            .center()
            .radius(radius)
            .focusable(!self.disabled)
            .disabled(self.disabled)
            .focus_ring(2.0, 2.0, t.colors.focus_ring);
        e = match px {
            Some(px) => e.px(px),
            None => e.w(h),
        };
        if let Some(f) = look.fill {
            e = e.fill(f);
        }
        if !self.disabled {
            if let Some(c) = look.hover {
                e = e.hover(c);
            }
            if let Some(c) = look.press {
                e = e.press(c);
            }
            if let Some(c) = look.hover_ink {
                e = e.hover_group();
                e = e.press_color(c);
            }
            if look.press_scale {
                e = e.press_scale(0.97);
            }
        }
        let mut ring = Vec::new();
        if self.ring {
            ring.push(rune::Shadow {
                x: 0.0,
                y: 0.0,
                blur: 0.0,
                spread: 2.0,
                color: t.colors.ring,
                inset: false,
            });
        }
        e = shadows(e, &[ring, look.shadow].concat());
        if self.pin == ButtonPin::Pressed {
            e = e.scale(0.96);
        }
        if self.disabled && look.disabled_opacity < 1.0 {
            e = e.opacity(look.disabled_opacity);
        }
        let ink = |el: El| match look.hover_ink {
            Some(c) if !self.disabled => el.color(look.ink).group_color(c),
            _ => el.color(look.ink),
        };
        if let Some(lead) = self.leading {
            e = e.child(lead);
        }
        if let Some(i) = self.icon {
            e = e.child(ink(icon(i, 16.0)));
        }
        if self.show_label {
            e = e.child(ink(text(self.label).font(sans(size, 500.0, line))));
        }
        e
    }
}
