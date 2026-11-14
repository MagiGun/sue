//! The design tokens: every radius, gap, size, stroke, shadow, layer and
//! color role the interface is drawn from.
//!
//! A token is a name for a decision, so a studio writes
//! `theme.radius(Radius::Md)` where it used to write `4.0`. The values
//! come from the [`Theme`], which reads them from the rice: the knobs
//! people already have (`layout.padding`, `layout.control_radius`,
//! `typography.ui_size` and the colors) move every token derived from
//! them.

use egui::epaint::Shadow;
use egui::{Color32, CornerRadius, FontId, Stroke, Ui, UiBuilder, UiStackInfo};

use crate::Theme;

/// The least contrast of text against what it lies on (WCAG 2, AA).
pub const TEXT_CONTRAST: f32 = 4.5;

/// The least contrast of a control's edge and of the focus ring against
/// the surface around them (WCAG 2, non-text contrast).
pub const EDGE_CONTRAST: f32 = 3.0;

/// No text role is smaller than this, unless the rice asks for smaller
/// captions.
pub const MIN_TEXT_SIZE: f32 = 10.0;

/// The corner radius steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Radius {
    /// Swatches, marks, slider tracks (2 by default).
    Xs,
    /// Chips inside strips, tabs, menu rows (4).
    Sm,
    /// Buttons, fields, icon buttons: `layout.control_radius` (6).
    Md,
    /// Cards, list rows, toasts, palettes that float:
    /// `layout.card_radius` (10).
    Lg,
    /// Dialogs, sheets, menus, the command palette: `layout.pill_radius`
    /// (14).
    Xl,
    /// Toggles, pills, progress bars: half the height, whatever it is.
    Full,
}

/// The radius of something that sits `gap` inside a corner of radius
/// `outer`, so both curves share a center: `outer - gap`, never under 2
/// (or under `outer`, when that is sharper still).
#[must_use]
pub fn nested_radius(outer: f32, gap: f32) -> f32 {
    (outer - gap).max(2.0_f32.min(outer)).max(0.0)
}

/// The steps of the spacing scale, a 4 pt grid at the default
/// `layout.padding` of 8.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Space {
    /// 2.
    S1,
    /// 4.
    S2,
    /// 6.
    S3,
    /// 8: `layout.padding` itself.
    S4,
    /// 12.
    S5,
    /// 16.
    S6,
    /// 24.
    S7,
    /// 32.
    S8,
    /// 40.
    S9,
}

impl Space {
    /// Every step, smallest first.
    pub const ALL: [Self; 9] = [
        Self::S1,
        Self::S2,
        Self::S3,
        Self::S4,
        Self::S5,
        Self::S6,
        Self::S7,
        Self::S8,
        Self::S9,
    ];

    /// The step as a share of `layout.padding`.
    const fn share(self) -> f32 {
        match self {
            Self::S1 => 0.25,
            Self::S2 => 0.5,
            Self::S3 => 0.75,
            Self::S4 => 1.0,
            Self::S5 => 1.5,
            Self::S6 => 2.0,
            Self::S7 => 3.0,
            Self::S8 => 4.0,
            Self::S9 => 5.0,
        }
    }
}

/// How large controls are: small in palettes, roomy in dialogs, finger
/// sized on touch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Density {
    /// Palettes and tool strips.
    Compact,
    /// Dialogs, Settings and the Hub.
    #[default]
    Regular,
    /// Touch layouts.
    Touch,
}

/// The tag a [`Density::scope`] leaves on its `Ui`.
const DENSITY_TAG: &str = "sue-density";

impl Density {
    /// Every density.
    pub const ALL: [Self; 3] = [Self::Compact, Self::Regular, Self::Touch];

    /// The density `ui` draws at: [`Touch`](Self::Touch) on a touch
    /// layout, else what the nearest [`scope`](Self::scope) around it
    /// said, else [`Regular`](Self::Regular).
    #[must_use]
    pub fn of(ui: &Ui) -> Self {
        if crate::widgets::is_touch(ui.ctx()) {
            return Self::Touch;
        }
        ui.stack()
            .iter()
            .find_map(|node| node.tags().get_downcast::<Self>(DENSITY_TAG).copied())
            .unwrap_or_default()
    }

    /// Draws `add_contents` at this density: every [`Density::of`] inside
    /// answers with it (a touch layout stays [`Touch`](Self::Touch)).
    pub fn scope<R>(self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
        let info = UiStackInfo::default().with_tag_value(DENSITY_TAG, self);
        ui.scope_builder(UiBuilder::new().ui_stack_info(info), add_contents)
            .inner
    }

    /// Height of buttons and fields.
    #[must_use]
    pub const fn control_height(self) -> f32 {
        match self {
            Self::Compact => 24.0,
            Self::Regular => 30.0,
            Self::Touch => 44.0,
        }
    }

    /// Side of a square icon button.
    #[must_use]
    pub const fn icon_button(self) -> f32 {
        match self {
            Self::Compact => 26.0,
            Self::Regular => 32.0,
            Self::Touch => 44.0,
        }
    }

    /// Side of an icon.
    #[must_use]
    pub const fn icon(self) -> f32 {
        match self {
            Self::Compact => 16.0,
            Self::Regular => 18.0,
            Self::Touch => 22.0,
        }
    }

    /// Height of a list row and of a menu row.
    #[must_use]
    pub const fn row(self) -> f32 {
        match self {
            Self::Compact => 24.0,
            Self::Regular => 30.0,
            Self::Touch => 44.0,
        }
    }

    /// Height of a tab strip.
    #[must_use]
    pub const fn tab_strip(self) -> f32 {
        match self {
            Self::Compact => 28.0,
            Self::Regular => 34.0,
            Self::Touch => 44.0,
        }
    }

    /// The least side of anything that takes a click or a tap.
    #[must_use]
    pub const fn min_hit(self) -> f32 {
        match self {
            Self::Compact => 24.0,
            Self::Regular => 28.0,
            Self::Touch => 44.0,
        }
    }
}

/// How far a surface floats above the page: its shadow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Elevation {
    /// Docked panels, cards at rest: no shadow.
    Flat,
    /// Hovered cards, floating palettes.
    Raised,
    /// Menus, popovers, tooltips, toasts.
    Floating,
    /// Dialogs, sheets, the command palette.
    Modal,
}

impl Elevation {
    /// Every level, lowest first.
    pub const ALL: [Self; 4] = [Self::Flat, Self::Raised, Self::Floating, Self::Modal];

    /// Blur, offset down and opacity at the default `layout.shadow_blur`.
    const fn shape(self) -> (f32, f32, f32) {
        match self {
            Self::Flat => (0.0, 0.0, 0.0),
            Self::Raised => (8.0, 2.0, 0.25),
            Self::Floating => (18.0, 6.0, 0.45),
            Self::Modal => (32.0, 12.0, 0.55),
        }
    }
}

/// What lies over what, lowest first. The one place that maps Sue's
/// layers to egui's [`Order`](egui::Order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Layer {
    /// The page: bars, docked palettes, the canvas.
    Page,
    /// Palettes pulled out of the dock.
    FloatingPalette,
    /// Menus, dropdown lists, pickers.
    Popover,
    /// Dialogs and sheets, over their scrim.
    Dialog,
    /// Notices. They can hold a button, so they take clicks.
    Toast,
    /// Tooltips. Nothing in them takes a click.
    Tooltip,
    /// What is being dragged, and where it would land.
    Drag,
}

impl Layer {
    /// Every layer, lowest first.
    pub const ALL: [Self; 7] = [
        Self::Page,
        Self::FloatingPalette,
        Self::Popover,
        Self::Dialog,
        Self::Toast,
        Self::Tooltip,
        Self::Drag,
    ];

    /// The egui order of this layer. egui has five orders for Sue's seven
    /// layers: within one order, what was opened or touched last is on
    /// top, which is what popovers, dialogs and toasts want of each other.
    #[must_use]
    pub const fn order(self) -> egui::Order {
        match self {
            Self::Page => egui::Order::Background,
            Self::FloatingPalette => egui::Order::Middle,
            Self::Popover | Self::Dialog | Self::Toast => egui::Order::Foreground,
            Self::Tooltip => egui::Order::Tooltip,
            Self::Drag => egui::Order::Debug,
        }
    }

    /// The egui layer of this order named `id`.
    #[must_use]
    pub fn id(self, id: egui::Id) -> egui::LayerId {
        egui::LayerId::new(self.order(), id)
    }
}

/// The steps of the type scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextRole {
    /// The smallest text: badges, axis marks (10).
    Micro,
    /// Captions and the status bar: `typography.small_size` (11).
    Caption,
    /// Interface text: `typography.ui_size` (13).
    Body,
    /// Titles of cards, dialogs and sections: `typography.title_size`
    /// (15).
    Title,
    /// Headings: `typography.heading_size` (18).
    Heading,
    /// The largest text: a page's name (24).
    Display,
    /// Monospace text: `typography.mono_size` (12).
    Mono,
}

impl TextRole {
    /// Every role.
    pub const ALL: [Self; 7] = [
        Self::Micro,
        Self::Caption,
        Self::Body,
        Self::Title,
        Self::Heading,
        Self::Display,
        Self::Mono,
    ];
}

/// Relative luminance of `color` (WCAG 2), from 0 for black to 1 for
/// white. Opacity is not looked at.
fn luminance(color: Color32) -> f32 {
    let linear = |channel: u8| {
        let value = f32::from(channel) / 255.0;
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
}

/// The contrast ratio of two colors (WCAG 2): 1 for the same color, 21 for
/// black on white.
#[must_use]
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// `from` moved the share `t` of the way to `to`, channel by channel.
fn mix(from: Color32, to: Color32, t: f32) -> Color32 {
    let channel = |a: u8, b: u8| {
        let (a, b) = (f32::from(a), f32::from(b));
        (a + (b - a) * t.clamp(0.0, 1.0)).round() as u8
    };
    Color32::from_rgb(
        channel(from.r(), to.r()),
        channel(from.g(), to.g()),
        channel(from.b(), to.b()),
    )
}

/// How finely [`toward`] steps from one color to the other.
const MIX_STEPS: u16 = 64;

/// The color nearest `from`, on the way to `to` and no nearer than the
/// share `start`, that `passes`; `to` itself when none does.
fn toward(from: Color32, to: Color32, start: f32, passes: impl Fn(Color32) -> bool) -> Color32 {
    (0..=MIX_STEPS)
        .map(|step| f32::from(step) / f32::from(MIX_STEPS))
        .filter(|share| *share >= start)
        .map(|share| mix(from, to, share))
        .find(|color| passes(*color))
        .unwrap_or(to)
}

impl Theme {
    /// Width of the focus ring and of the outline of what is chosen.
    pub const STRONG_STROKE: f32 = 2.0;

    /// A corner radius step, in points. [`Radius::Full`] is as round as a
    /// corner can be: the painter holds it to half the short side.
    #[must_use]
    pub fn radius(&self, step: Radius) -> f32 {
        match step {
            Radius::Xs => self.control_radius / 3.0,
            Radius::Sm => self.control_radius * 2.0 / 3.0,
            Radius::Md => self.control_radius,
            Radius::Lg => self.card_radius,
            Radius::Xl => self.pill_radius,
            Radius::Full => 255.0,
        }
        .max(0.0)
    }

    /// A corner radius step, as egui takes it.
    #[must_use]
    pub fn corner(&self, step: Radius) -> CornerRadius {
        CornerRadius::same(self.radius(step).round().clamp(0.0, 255.0) as u8)
    }

    /// A step of the spacing scale, in points.
    #[must_use]
    pub fn space(&self, step: Space) -> f32 {
        self.padding * step.share()
    }

    /// Space between a dialog's edge and what it holds.
    #[must_use]
    pub fn dialog_margin(&self) -> f32 {
        self.space(Space::S6)
    }

    /// Space between a card's edge and what it holds.
    #[must_use]
    pub fn card_padding(&self) -> f32 {
        self.space(Space::S5)
    }

    /// Space between the rows of a form.
    #[must_use]
    pub fn form_row_gap(&self) -> f32 {
        self.space(Space::S4)
    }

    /// Space between a label and its control.
    #[must_use]
    pub fn label_gap(&self) -> f32 {
        self.space(Space::S5)
    }

    /// The size of a text role, in points.
    #[must_use]
    pub fn text_size(&self, role: TextRole) -> f32 {
        match role {
            // A step under captions, but not under the floor, and never
            // over the captions when the rice makes those smaller still.
            TextRole::Micro => (self.small_size - 1.0)
                .max(MIN_TEXT_SIZE)
                .min(self.small_size),
            TextRole::Caption => self.small_size,
            TextRole::Body => self.ui_size,
            TextRole::Title => self.title_size,
            TextRole::Heading => self.heading_size,
            TextRole::Display => self.heading_size * 4.0 / 3.0,
            TextRole::Mono => self.mono_size,
        }
    }

    /// The font of a text role.
    #[must_use]
    pub fn text_font(&self, role: TextRole) -> FontId {
        let size = self.text_size(role);
        match role {
            TextRole::Mono => self.mono(size),
            _ => self.font(size),
        }
    }

    /// How thick the lines of an icon `size` points across are: 1.5 for
    /// icons of 16 to 20 points, in step with the size outside that.
    #[must_use]
    pub fn icon_stroke(&self, size: f32) -> f32 {
        1.5 * (size / size.clamp(16.0, 20.0)).max(0.0)
    }

    /// The stroke of the focus ring.
    #[must_use]
    pub fn focus_stroke(&self) -> Stroke {
        Stroke::new(Self::STRONG_STROKE, self.focus())
    }

    /// The outline of what is chosen.
    #[must_use]
    pub fn selected_stroke(&self) -> Stroke {
        Stroke::new(Self::STRONG_STROKE, self.border_active)
    }

    /// The shadow of an elevation level. `layout.shadow_blur` scales every
    /// level, and 0 takes all shadows away.
    #[must_use]
    pub fn elevation(&self, level: Elevation) -> Shadow {
        let (blur, down, opacity) = level.shape();
        let scale = self.shadow_blur.max(0.0) / 18.0;
        Shadow {
            offset: [0, (down * scale).round().clamp(0.0, 127.0) as i8],
            blur: (blur * scale).round().clamp(0.0, 255.0) as u8,
            spread: 0,
            color: self.shadow.gamma_multiply(opacity),
        }
    }

    /// The fill of a surface: 0 the page, 1 bars and panels, 2 cards and
    /// fields, 3 and up popovers (as far above the cards as those are
    /// above the panels).
    #[must_use]
    pub fn surface(&self, level: u8) -> Color32 {
        match level {
            0 => self.void,
            1 => self.panel,
            2 => self.pill,
            _ => {
                let step = |pill: u8, panel: u8| {
                    (i16::from(pill) * 2 - i16::from(panel)).clamp(0, 255) as u8
                };
                Color32::from_rgb(
                    step(self.pill.r(), self.panel.r()),
                    step(self.pill.g(), self.panel.g()),
                    step(self.pill.b(), self.panel.b()),
                )
            }
        }
    }

    /// The fill of a control under the pointer.
    #[must_use]
    pub fn fill_hover(&self) -> Color32 {
        self.hover
    }

    /// The fill of a control being pressed.
    #[must_use]
    pub fn fill_pressed(&self) -> Color32 {
        self.active
    }

    /// The fill of what is chosen. Text on it is [`Theme::text`]:
    /// secondary text need not read on it (see
    /// [`text_muted_on`](Self::text_muted_on)).
    #[must_use]
    pub fn fill_selected(&self) -> Color32 {
        self.active
    }

    /// Secondary text. It reads (4.5 to 1) on every surface and on a
    /// hovered control.
    #[must_use]
    pub fn text_muted(&self) -> Color32 {
        self.muted
    }

    /// Secondary text on `fill`: the muted color where that reads, else
    /// as much nearer the text color as it takes.
    #[must_use]
    pub fn text_muted_on(&self, fill: Color32) -> Color32 {
        toward(self.muted, self.text, 0.0, |color| {
            contrast(color, fill) >= TEXT_CONTRAST
        })
    }

    /// The text and icons of what cannot be used now, on `surface`: the
    /// text color at 38 percent over it, or as much more as it takes to
    /// reach 3 to 1. The one rule for every disabled thing.
    #[must_use]
    pub fn text_disabled(&self, surface: Color32) -> Color32 {
        toward(surface, self.text, 0.38, |color| {
            contrast(color, surface) >= EDGE_CONTRAST
        })
    }

    /// The outline of fields and cards on `surface`: the border color
    /// moved toward the text color until it reaches 3 to 1.
    #[must_use]
    pub fn border_strong_on(&self, surface: Color32) -> Color32 {
        toward(self.border, self.text, 0.0, |color| {
            contrast(color, surface) >= EDGE_CONTRAST
        })
    }

    /// The outline of fields and cards, wherever they lie: it reaches 3 to
    /// 1 on the page, on panels and on cards alike.
    #[must_use]
    pub fn border_strong(&self) -> Color32 {
        toward(self.border, self.text, 0.0, |color| {
            (0..=2).all(|level| contrast(color, self.surface(level)) >= EDGE_CONTRAST)
        })
    }

    /// The color of the focus ring.
    #[must_use]
    pub fn focus(&self) -> Color32 {
        self.border_active
    }

    /// The dimmed backdrop behind dialogs and sheets.
    #[must_use]
    pub fn scrim(&self) -> Color32 {
        self.shadow.gamma_multiply(0.45)
    }

    /// What deletes or cannot be undone, and errors. The text color in the
    /// shipped themes, which stay monochrome: such things are always
    /// drawn with a glyph too, never told apart by color alone.
    #[must_use]
    pub fn danger(&self) -> Color32 {
        self.text
    }

    /// Warnings. See [`danger`](Self::danger).
    #[must_use]
    pub fn warning(&self) -> Color32 {
        self.text
    }

    /// What went well. See [`danger`](Self::danger).
    #[must_use]
    pub fn success(&self) -> Color32 {
        self.text
    }

    /// Notes. See [`danger`](Self::danger).
    #[must_use]
    pub fn info(&self) -> Color32 {
        self.text
    }

    /// Links. See [`danger`](Self::danger).
    #[must_use]
    pub fn link(&self) -> Color32 {
        self.text
    }
}

#[cfg(test)]
mod tests {
    use sue_rice::RiceSchema;

    use super::*;

    /// Sue's theme with each built-in preset's colors.
    fn presets() -> Vec<(String, Theme)> {
        let mut rice = RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
        crate::register_rice(&mut rice).unwrap();
        let sue = Theme::from_rice(&rice);
        sue_rice::builtin_presets()
            .iter()
            .map(|preset| (preset.id.clone(), sue.with_preset(preset)))
            .collect()
    }

    fn default_theme() -> Theme {
        let mut rice = RiceSchema::new();
        crate::register_rice(&mut rice).unwrap();
        Theme::from_rice(&rice)
    }

    #[test]
    fn contrast_is_the_wcag_ratio() {
        let (black, white) = (Color32::BLACK, Color32::WHITE);
        assert!((contrast(black, white) - 21.0).abs() < 1e-3);
        assert!((contrast(white, black) - 21.0).abs() < 1e-3);
        assert!((contrast(white, white) - 1.0).abs() < 1e-6);
        // The gray that is 4.5 to 1 on white, as the standard's own
        // examples have it.
        assert!(contrast(Color32::from_gray(0x76), white) >= 4.5);
        assert!(contrast(Color32::from_gray(0x78), white) < 4.5);
    }

    #[test]
    fn every_preset_reads() {
        let presets = presets();
        assert_eq!(presets.len(), 4);
        for (id, theme) in &presets {
            let surfaces: Vec<Color32> = (0..=3).map(|level| theme.surface(level)).collect();
            let fills = [
                theme.fill_hover(),
                theme.fill_pressed(),
                theme.fill_selected(),
            ];
            // Text reads on every surface and on every state of a control.
            for ground in surfaces.iter().chain(&fills) {
                let ratio = contrast(theme.text, *ground);
                assert!(ratio >= TEXT_CONTRAST, "{id}: text on {ground:?} {ratio}");
                for role in [
                    theme.danger(),
                    theme.warning(),
                    theme.success(),
                    theme.info(),
                    theme.link(),
                ] {
                    assert!(contrast(role, *ground) >= TEXT_CONTRAST, "{id} {ground:?}");
                }
                // Secondary text too, once it is asked what it lies on.
                let muted = contrast(theme.text_muted_on(*ground), *ground);
                assert!(muted >= TEXT_CONTRAST, "{id}: muted on {ground:?} {muted}");
            }
            // Secondary text as it is reads on every surface and on a
            // hovered control.
            for ground in surfaces.iter().chain([&theme.fill_hover()]) {
                let ratio = contrast(theme.text_muted(), *ground);
                assert!(ratio >= TEXT_CONTRAST, "{id}: muted on {ground:?} {ratio}");
                assert_eq!(theme.text_muted_on(*ground), theme.muted, "{id}");
            }
            // Outlines, the focus ring and disabled text reach 3 to 1.
            for ground in &surfaces {
                let focus = contrast(theme.focus(), *ground);
                assert!(focus >= EDGE_CONTRAST, "{id}: focus on {ground:?} {focus}");
                let edge = contrast(theme.border_strong_on(*ground), *ground);
                assert!(edge >= EDGE_CONTRAST, "{id}: edge on {ground:?} {edge}");
                let off = contrast(theme.text_disabled(*ground), *ground);
                assert!(off >= EDGE_CONTRAST, "{id}: disabled on {ground:?} {off}");
                // Disabled text is fainter than secondary text.
                assert!(
                    off < contrast(theme.text_muted(), *ground),
                    "{id} {ground:?}"
                );
            }
            for ground in &surfaces[..3] {
                let edge = contrast(theme.border_strong(), *ground);
                assert!(edge >= EDGE_CONTRAST, "{id}: edge on {ground:?} {edge}");
            }
            for ground in &fills {
                let focus = contrast(theme.focus(), *ground);
                assert!(focus >= EDGE_CONTRAST, "{id}: focus on {ground:?} {focus}");
            }
            // A hairline is its own gray: neither the hover fill nor the
            // surface it is drawn on.
            assert_ne!(theme.border, theme.hover, "{id}");
            assert!(surfaces[..3].iter().all(|ground| *ground != theme.border));
            // Every role is a gray.
            for color in [
                theme.surface(3),
                theme.border_strong(),
                theme.text_disabled(theme.panel),
                theme.text_muted_on(theme.active),
                theme.scrim(),
            ] {
                assert!(color.r() == color.g() && color.g() == color.b(), "{id}");
            }
        }
    }

    #[test]
    fn radii_follow_the_rice() {
        let theme = default_theme();
        assert_eq!(theme.radius(Radius::Xs), 2.0);
        assert_eq!(theme.radius(Radius::Sm), 4.0);
        assert_eq!(theme.radius(Radius::Md), 6.0);
        assert_eq!(theme.radius(Radius::Lg), 10.0);
        assert_eq!(theme.radius(Radius::Xl), 14.0);
        assert_eq!(theme.corner(Radius::Md), CornerRadius::same(6));
        assert_eq!(theme.corner(Radius::Full), CornerRadius::same(255));
        // A sharp theme is sharp all the way down.
        let sharp = Theme {
            control_radius: 0.0,
            card_radius: 0.0,
            pill_radius: 0.0,
            ..theme
        };
        for step in [Radius::Xs, Radius::Sm, Radius::Md, Radius::Lg, Radius::Xl] {
            assert_eq!(sharp.radius(step), 0.0);
        }
        // Inner corners share the outer corner's center, down to 2.
        assert_eq!(nested_radius(14.0, 8.0), 6.0);
        assert_eq!(nested_radius(6.0, 8.0), 2.0);
        assert_eq!(nested_radius(1.0, 8.0), 1.0);
        assert_eq!(nested_radius(0.0, 4.0), 0.0);
    }

    #[test]
    fn spacing_is_a_four_point_grid_that_follows_the_padding() {
        let theme = default_theme();
        let steps: Vec<f32> = Space::ALL.iter().map(|step| theme.space(*step)).collect();
        assert_eq!(steps, [2.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 32.0, 40.0]);
        assert_eq!(theme.dialog_margin(), 16.0);
        assert_eq!(theme.card_padding(), 12.0);
        assert_eq!(theme.form_row_gap(), 8.0);
        assert_eq!(theme.label_gap(), 12.0);
        let roomy = Theme {
            padding: 12.0,
            ..theme
        };
        assert_eq!(roomy.space(Space::S4), 12.0);
        assert_eq!(roomy.space(Space::S6), 24.0);
    }

    #[test]
    fn the_type_scale_has_seven_steps_and_a_floor() {
        let theme = default_theme();
        let sizes: Vec<f32> = TextRole::ALL
            .iter()
            .map(|role| theme.text_size(*role))
            .collect();
        assert_eq!(sizes, [10.0, 11.0, 13.0, 15.0, 18.0, 24.0, 12.0]);
        assert!(sizes.iter().all(|size| *size >= MIN_TEXT_SIZE));
        assert_eq!(
            theme.text_font(TextRole::Mono).family,
            egui::FontFamily::Monospace
        );
        assert_eq!(
            theme.text_font(TextRole::Title),
            FontId::new(15.0, egui::FontFamily::Proportional)
        );
        // Captions at the floor: the smallest text does not go under it.
        let small = Theme {
            small_size: 10.0,
            ..theme.clone()
        };
        assert_eq!(small.text_size(TextRole::Micro), 10.0);
        // Captions the rice makes smaller still: nothing is drawn larger
        // than the captions for being "micro".
        let tiny = Theme {
            small_size: 8.0,
            ..theme
        };
        assert_eq!(tiny.text_size(TextRole::Micro), 8.0);
    }

    #[test]
    fn densities_grow_from_palettes_to_touch() {
        let sizes = |density: Density| {
            [
                density.control_height(),
                density.icon_button(),
                density.icon(),
                density.row(),
                density.tab_strip(),
                density.min_hit(),
            ]
        };
        assert_eq!(
            sizes(Density::Compact),
            [24.0, 26.0, 16.0, 24.0, 28.0, 24.0]
        );
        assert_eq!(
            sizes(Density::Regular),
            [30.0, 32.0, 18.0, 30.0, 34.0, 28.0]
        );
        assert_eq!(sizes(Density::Touch), [44.0, 44.0, 22.0, 44.0, 44.0, 44.0]);
        for density in Density::ALL {
            // Nothing that takes a click is smaller than the hit area.
            assert!(density.control_height() >= density.min_hit());
            assert!(density.icon_button() >= density.min_hit());
            assert!(density.row() >= density.min_hit());
            assert!(density.icon() < density.icon_button());
        }
    }

    #[test]
    fn a_density_reaches_everything_drawn_inside_its_scope() {
        let ctx = egui::Context::default();
        let mut seen = Vec::new();
        let mut done = ctx.run_ui(egui::RawInput::default(), |ui| {
            seen.push(Density::of(ui));
            Density::Compact.scope(ui, |ui| {
                seen.push(Density::of(ui));
                ui.horizontal(|ui| {
                    ui.vertical(|ui| seen.push(Density::of(ui)));
                    // The nearest scope wins.
                    Density::Regular.scope(ui, |ui| seen.push(Density::of(ui)));
                });
            });
            seen.push(Density::of(ui));
        });
        done.textures_delta.clear();
        assert_eq!(
            seen,
            [
                Density::Regular,
                Density::Compact,
                Density::Compact,
                Density::Regular,
                Density::Regular
            ]
        );
        // A touch layout is touch sized whatever a scope says.
        ctx.data_mut(|data| data.insert_temp(egui::Id::NULL.with("sue-touch"), true));
        let mut touch = None;
        let mut done = ctx.run_ui(egui::RawInput::default(), |ui| {
            Density::Compact.scope(ui, |ui| touch = Some(Density::of(ui)));
        });
        done.textures_delta.clear();
        assert_eq!(touch, Some(Density::Touch));
    }

    #[test]
    fn shadows_deepen_with_the_elevation_and_follow_the_blur_setting() {
        let theme = default_theme();
        assert_eq!(theme.elevation(Elevation::Flat).blur, 0);
        assert_eq!(theme.elevation(Elevation::Flat).color, Color32::TRANSPARENT);
        let shapes: Vec<(u8, i8)> = Elevation::ALL
            .iter()
            .map(|level| {
                let shadow = theme.elevation(*level);
                (shadow.blur, shadow.offset[1])
            })
            .collect();
        assert_eq!(shapes, [(0, 0), (8, 2), (18, 6), (32, 12)]);
        let opacity: Vec<u8> = Elevation::ALL
            .iter()
            .map(|level| theme.elevation(*level).color.a())
            .collect();
        assert!(opacity.is_sorted(), "{opacity:?}");
        let flat = Theme {
            shadow_blur: 0.0,
            ..theme.clone()
        };
        assert!(
            Elevation::ALL
                .iter()
                .all(|level| flat.elevation(*level).blur == 0)
        );
        let deep = Theme {
            shadow_blur: 36.0,
            ..theme
        };
        assert_eq!(deep.elevation(Elevation::Raised).blur, 16);
    }

    #[test]
    fn layers_never_go_down_in_eguis_order() {
        let orders: Vec<egui::Order> = Layer::ALL.iter().map(|layer| layer.order()).collect();
        assert!(orders.is_sorted(), "{orders:?}");
        assert!(Layer::ALL.is_sorted());
        // What is dragged is over the tooltips, and those over the rest.
        assert!(Layer::Drag.order() > Layer::Tooltip.order());
        assert!(Layer::Tooltip.order() > Layer::Toast.order());
        // Toasts can hold a button: egui's tooltip order takes no clicks.
        assert!(Layer::Toast.order().allow_interaction());
        assert_eq!(
            Layer::Dialog.id(egui::Id::unique("x")).order,
            egui::Order::Foreground
        );
    }

    #[test]
    fn strokes_have_one_rule() {
        let theme = default_theme();
        assert_eq!(theme.hairline().width, 1.0);
        assert_eq!(theme.focus_stroke(), Stroke::new(2.0, theme.border_active));
        assert_eq!(theme.selected_stroke().width, 2.0);
        for size in [16.0, 18.0, 20.0] {
            assert_eq!(theme.icon_stroke(size), 1.5);
        }
        assert_eq!(theme.icon_stroke(8.0), 0.75);
        assert_eq!(theme.icon_stroke(40.0), 3.0);
    }

    #[test]
    fn surfaces_step_up_and_the_scrim_dims() {
        let theme = default_theme();
        assert_eq!(theme.surface(0), Color32::from_gray(0x0a));
        assert_eq!(theme.surface(1), Color32::from_gray(0x11));
        assert_eq!(theme.surface(2), Color32::from_gray(0x18));
        assert_eq!(theme.surface(3), Color32::from_gray(0x1f));
        assert_eq!(theme.surface(9), theme.surface(3));
        assert_eq!(theme.scrim(), theme.shadow.gamma_multiply(0.45));
        assert!(theme.scrim().a() > 0 && theme.scrim().a() < 255);
        // Disabled text starts at 38 percent of the way to the text.
        assert_eq!(theme.text_disabled(theme.panel), Color32::from_gray(0x6e));
    }
}
