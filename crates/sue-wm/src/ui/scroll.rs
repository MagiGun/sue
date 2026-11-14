//! The scroll area: thin bars that lie over the content, and edges that
//! fade where there is more.
//!
//! A [`ScrollView`] is egui's scroll area in Sue's look. Its bars take no
//! room: they are a thin line over the content while the pointer is in the
//! area, widen under the pointer, and are gone otherwise. That there is
//! more to see is said by the edges: the side the content runs out of
//! fades into the surface it lies on, and stops fading at its end.
//!
//! On a touch layout nothing hovers, so the bar stays faintly visible.

use std::ops::Range;
use std::sync::Arc;

use egui::containers::scroll_area::{ScrollAreaOutput, ScrollBarVisibility};
use egui::style::{ScrollFadeStyle, ScrollStyle};
use egui::{
    Color32, CornerRadius, Direction, Id, Margin, Rect, ScrollArea, Shape, Style, Ui, Vec2,
};

use super::list::{ground, salted};
use super::tokens::{Density, Radius, Space};
use crate::Theme;

/// An area that scrolls what does not fit in it.
#[derive(Clone, Copy, Debug)]
#[must_use = "a scroll view draws nothing until it is shown"]
pub struct ScrollView {
    id: Id,
    axes: [bool; 2],
    max_size: [Option<f32>; 2],
    auto_shrink: [bool; 2],
    stick_to_bottom: bool,
    fades: bool,
    on: Option<Color32>,
    always_bars: bool,
}

impl ScrollView {
    fn new(salt: impl std::hash::Hash + std::fmt::Debug, axes: [bool; 2]) -> Self {
        Self {
            id: salted("sue-scroll-view", salt),
            axes,
            max_size: [None, None],
            auto_shrink: [false, false],
            stick_to_bottom: false,
            fades: true,
            on: None,
            always_bars: false,
        }
    }

    /// An area that scrolls up and down. `salt` names it among what is
    /// drawn in the same `Ui` and stays the same from frame to frame, so
    /// it keeps its place when what is around it changes.
    pub fn vertical(salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self::new(salt, [false, true])
    }

    /// An area that scrolls left and right.
    pub fn horizontal(salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self::new(salt, [true, false])
    }

    /// An area that scrolls both ways.
    pub fn both(salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self::new(salt, [true, true])
    }

    /// The most it is wide, whatever room there is.
    pub const fn max_width(mut self, width: f32) -> Self {
        self.max_size[0] = Some(width);
        self
    }

    /// The most it is high, whatever room there is.
    pub const fn max_height(mut self, height: f32) -> Self {
        self.max_size[1] = Some(height);
        self
    }

    /// Whether it is only as wide and as high as what it holds, when that
    /// is less than the room there is. It fills the room by default.
    pub const fn auto_shrink(mut self, width: bool, height: bool) -> Self {
        self.auto_shrink = [width, height];
        self
    }

    /// Stays at its end while content is added there (a log, a chat),
    /// until someone scrolls away from it.
    pub const fn stick_to_bottom(mut self, stick: bool) -> Self {
        self.stick_to_bottom = stick;
        self
    }

    /// Whether its edges fade where there is more to see. They do by
    /// default.
    pub const fn fades(mut self, fades: bool) -> Self {
        self.fades = fades;
        self
    }

    /// The fill of the surface it lies on, which its edges fade into.
    /// Without one it is what the frames around it are filled with, or a
    /// panel's fill when they say nothing: an area on a hand-painted
    /// surface has to say.
    pub const fn on(mut self, surface: Color32) -> Self {
        self.on = Some(surface);
        self
    }

    /// Keeps its bars visible even when the pointer is elsewhere.
    pub const fn always_bars(mut self, always: bool) -> Self {
        self.always_bars = always;
        self
    }

    /// How far from an end the fading reaches, in points.
    #[must_use]
    pub fn fade_reach(theme: &Theme) -> f32 {
        theme.space(Space::S6)
    }

    /// How scroll bars are drawn at `density`: thin, over the content,
    /// shown while the pointer is in the area (always, faintly, on touch
    /// layouts and when `always` asks).
    #[must_use]
    pub fn style(theme: &Theme, density: Density, always: bool) -> ScrollStyle {
        let touch = density == Density::Touch;
        let step = theme.space(Space::S1);
        ScrollStyle {
            floating: true,
            content_margin: Margin::ZERO,
            bar_width: step * 4.0,
            handle_min_length: density.min_hit(),
            bar_inner_margin: step,
            bar_outer_margin: step,
            floating_width: step * 1.5,
            floating_allocated_width: 0.0,
            foreground_color: true,
            dormant_background_opacity: 0.0,
            active_background_opacity: 0.0,
            interact_background_opacity: 0.0,
            dormant_handle_opacity: if touch || always { 0.35 } else { 0.0 },
            active_handle_opacity: 0.5,
            interact_handle_opacity: 0.8,
            // The view paints its own fades, into the surface it is told.
            fade: ScrollFadeStyle {
                strength: 0.0,
                size: 0.0,
            },
            ..ScrollStyle::floating()
        }
    }

    /// How opaque the fade at the start and at the end of one axis is:
    /// nothing at an end the content does not run out of, full once it
    /// runs out by `reach` or more.
    #[must_use]
    pub fn fade_strength(offset: f32, content: f32, view: f32, reach: f32) -> [f32; 2] {
        if !(reach > 0.0 && content > view) {
            return [0.0, 0.0];
        }
        let hidden_after = content - view - offset;
        [
            (offset / reach).clamp(0.0, 1.0),
            (hidden_after / reach).clamp(0.0, 1.0),
        ]
    }

    /// Draws the area around what `add_contents` draws.
    pub fn show<R>(
        self,
        ui: &mut Ui,
        theme: &Theme,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> ScrollAreaOutput<R> {
        self.run(ui, theme, |area, ui, inside| {
            area.show(ui, |ui| {
                ui.set_style(inside);
                add_contents(ui)
            })
        })
    }

    /// Draws the area around `total_rows` rows of one height
    /// (`row_height`, without the spacing between them), of which
    /// `add_contents` draws only the ones in view: a list of any length
    /// costs what its visible rows cost.
    pub fn show_rows<R>(
        self,
        ui: &mut Ui,
        theme: &Theme,
        row_height: f32,
        total_rows: usize,
        add_contents: impl FnOnce(&mut Ui, Range<usize>) -> R,
    ) -> ScrollAreaOutput<R> {
        self.run(ui, theme, |area, ui, inside| {
            area.show_rows(ui, row_height, total_rows, |ui, rows| {
                ui.set_style(inside);
                add_contents(ui, rows)
            })
        })
    }

    /// Runs the egui scroll area `show` shows, in Sue's look, and paints
    /// the fades over it. `show` gets the style what is inside goes back
    /// to: the bars' look is the area's own.
    fn run<R>(
        self,
        ui: &mut Ui,
        theme: &Theme,
        show: impl FnOnce(ScrollArea, &mut Ui, Arc<Style>) -> ScrollAreaOutput<R>,
    ) -> ScrollAreaOutput<R> {
        let density = Density::of(ui);
        let surface = self.on.unwrap_or_else(|| ground(ui, theme));
        let outside = ui.style().clone();
        {
            let style = ui.style_mut();
            style.spacing.scroll = Self::style(theme, density, self.always_bars);
            // The handle is a pill, whatever buttons look like.
            let round = theme.corner(Radius::Full);
            for look in [
                &mut style.visuals.widgets.inactive,
                &mut style.visuals.widgets.hovered,
                &mut style.visuals.widgets.active,
            ] {
                look.corner_radius = round;
                look.fg_stroke.color = theme.text;
            }
        }
        let mut area = ScrollArea::new(self.axes)
            .id_salt(self.id)
            .auto_shrink(self.auto_shrink)
            .stick_to_bottom(self.stick_to_bottom);
        if self.always_bars {
            area = area.scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible);
        }
        if let Some(width) = self.max_size[0] {
            area = area.max_width(width);
        }
        if let Some(height) = self.max_size[1] {
            area = area.max_height(height);
        }
        let output = show(area, ui, outside.clone());
        ui.set_style(outside);
        if self.fades {
            let offset = output.state.clamped_offset();
            paint_fades(
                ui,
                output.inner_rect,
                (output.content_size, offset),
                surface,
                Self::fade_reach(theme),
            );
        }
        output
    }
}

/// Paints the fades of an area that shows `view` of content
/// `content_size` large, scrolled by `offset`, into `surface`.
fn paint_fades(
    ui: &Ui,
    view: Rect,
    (content_size, offset): (Vec2, Vec2),
    surface: Color32,
    reach: f32,
) {
    if !ui.is_rect_visible(view) {
        return;
    }
    let painter = ui.painter().with_clip_rect(view.intersect(ui.clip_rect()));
    let clear = Color32::TRANSPARENT;
    let tint = |strength: f32| surface.gamma_multiply(strength);
    let [top, bottom] = ScrollView::fade_strength(offset.y, content_size.y, view.height(), reach);
    let [left, right] = ScrollView::fade_strength(offset.x, content_size.x, view.width(), reach);
    let band = |rect: Rect, direction: Direction, colors: [Color32; 2]| {
        let mut shape = Shape::gradient_rect(rect, direction, colors);
        if let Shape::Rect(rect) = &mut shape {
            rect.corner_radius = CornerRadius::ZERO;
        }
        painter.add(shape);
    };
    if top > 0.0 {
        let rect = Rect::from_min_max(view.min, egui::pos2(view.right(), view.top() + reach));
        band(rect, Direction::TopDown, [tint(top), clear]);
    }
    if bottom > 0.0 {
        let rect = Rect::from_min_max(egui::pos2(view.left(), view.bottom() - reach), view.max);
        band(rect, Direction::TopDown, [clear, tint(bottom)]);
    }
    if left > 0.0 {
        let rect = Rect::from_min_max(view.min, egui::pos2(view.left() + reach, view.bottom()));
        band(rect, Direction::LeftToRight, [tint(left), clear]);
    }
    if right > 0.0 {
        let rect = Rect::from_min_max(egui::pos2(view.right() - reach, view.top()), view.max);
        band(rect, Direction::LeftToRight, [clear, tint(right)]);
    }
}
