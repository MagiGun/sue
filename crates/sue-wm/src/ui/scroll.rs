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
use egui::layers::ShapeIdx;
use egui::style::{ScrollFadeStyle, ScrollStyle};
use egui::{
    Color32, Direction, Id, Margin, Painter, Rect, ScrollArea, Shape, Style, Ui, Vec2, pos2,
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
        self.run(ui, theme, |area, ui, inside, slot| {
            area.show(ui, |ui| {
                ui.set_style(inside);
                let inner = add_contents(ui);
                *slot = Some(FadeSlot::after(ui));
                inner
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
        self.run(ui, theme, |area, ui, inside, slot| {
            area.show_rows(ui, row_height, total_rows, |ui, rows| {
                ui.set_style(inside);
                let inner = add_contents(ui, rows);
                *slot = Some(FadeSlot::after(ui));
                inner
            })
        })
    }

    /// Runs the egui scroll area `show` shows, in Sue's look, and paints
    /// the fades into the slot `show` leaves after the content: over the
    /// content and under the bars, which egui draws after it. `show` gets
    /// the style what is inside goes back to: the bars' look is the
    /// area's own.
    fn run<R>(
        self,
        ui: &mut Ui,
        theme: &Theme,
        show: impl FnOnce(ScrollArea, &mut Ui, Arc<Style>, &mut Option<FadeSlot>) -> ScrollAreaOutput<R>,
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
        let mut slot = None;
        let output = show(area, ui, outside.clone(), &mut slot);
        ui.set_style(outside);
        if self.fades
            && let Some(slot) = slot
        {
            let offset = output.state.clamped_offset();
            let fades = fades(
                output.inner_rect,
                (output.content_size, offset),
                surface,
                Self::fade_reach(theme),
            );
            if !fades.is_empty() {
                slot.painter.set(slot.index, Shape::Vec(fades));
            }
        }
        output
    }
}

/// The place among the shapes of an area where its fades go: after its
/// content, before its bars.
struct FadeSlot {
    painter: Painter,
    index: ShapeIdx,
}

impl FadeSlot {
    /// A slot after what `ui`, the area's content, has drawn so far.
    fn after(ui: &Ui) -> Self {
        let painter = ui.painter().clone();
        let index = painter.add(Shape::Noop);
        Self { painter, index }
    }
}

/// The fades of an area that shows `view` of content `content_size`
/// large, scrolled by `offset`, into `surface`: a band at each edge the
/// content runs out of.
fn fades(
    view: Rect,
    (content_size, offset): (Vec2, Vec2),
    surface: Color32,
    reach: f32,
) -> Vec<Shape> {
    let clear = Color32::TRANSPARENT;
    let tint = |strength: f32| surface.gamma_multiply(strength);
    let [top, bottom] = ScrollView::fade_strength(offset.y, content_size.y, view.height(), reach);
    let [left, right] = ScrollView::fade_strength(offset.x, content_size.x, view.width(), reach);
    let band = |rect: Rect, direction: Direction, colors: [Color32; 2]| {
        Shape::gradient_rect(rect, direction, colors)
    };
    let mut shapes = Vec::new();
    if top > 0.0 {
        let rect = Rect::from_min_max(view.min, pos2(view.right(), view.top() + reach));
        shapes.push(band(rect, Direction::TopDown, [tint(top), clear]));
    }
    if bottom > 0.0 {
        let rect = Rect::from_min_max(pos2(view.left(), view.bottom() - reach), view.max);
        shapes.push(band(rect, Direction::TopDown, [clear, tint(bottom)]));
    }
    if left > 0.0 {
        let rect = Rect::from_min_max(view.min, pos2(view.left() + reach, view.bottom()));
        shapes.push(band(rect, Direction::LeftToRight, [tint(left), clear]));
    }
    if right > 0.0 {
        let rect = Rect::from_min_max(pos2(view.right() - reach, view.top()), view.max);
        shapes.push(band(rect, Direction::LeftToRight, [clear, tint(right)]));
    }
    shapes
}

#[cfg(test)]
mod tests {
    use egui::{Event, MouseWheelSource, MouseWheelUnit, TouchPhase, vec2};

    use super::super::list::harness::Run;
    use super::*;

    #[test]
    fn an_edge_fades_only_where_the_content_runs_out() {
        // Content that fits: no fades at all.
        assert_eq!(
            ScrollView::fade_strength(0.0, 100.0, 200.0, 16.0),
            [0.0, 0.0]
        );
        // At the top of a long list: only the end fades.
        assert_eq!(
            ScrollView::fade_strength(0.0, 1000.0, 200.0, 16.0),
            [0.0, 1.0]
        );
        // A little way in: the start fades in step with the distance.
        assert_eq!(
            ScrollView::fade_strength(4.0, 1000.0, 200.0, 16.0),
            [0.25, 1.0]
        );
        // At the end: only the start fades.
        assert_eq!(
            ScrollView::fade_strength(800.0, 1000.0, 200.0, 16.0),
            [1.0, 0.0]
        );
        // No reach, no fade.
        assert_eq!(
            ScrollView::fade_strength(10.0, 1000.0, 200.0, 0.0),
            [0.0, 0.0]
        );
    }

    #[test]
    fn the_bars_take_no_room_and_show_faintly_only_where_nothing_hovers() {
        let theme = Run::new().theme;
        for density in Density::ALL {
            let style = ScrollView::style(&theme, density, false);
            assert!(style.floating);
            assert_eq!(style.floating_allocated_width, 0.0);
            let resting = style.dormant_handle_opacity;
            if density == Density::Touch {
                assert!(resting > 0.0, "on touch nothing hovers to show the bar");
            } else {
                assert_eq!(resting, 0.0);
            }
            assert!(ScrollView::style(&theme, density, true).dormant_handle_opacity > 0.0);
            assert!(style.handle_min_length >= density.min_hit());
        }
    }

    /// Thirty rows in an area 100 points high; the shapes of the frame.
    fn list(run: &mut Run, events: Vec<Event>) -> (Rect, Vec<Shape>) {
        let draw = |ui: &mut Ui, theme: &Theme| {
            ScrollView::vertical("list")
                .max_height(100.0)
                .always_bars(true)
                .show(ui, theme, |ui| {
                    for row in 0..30 {
                        ui.label(format!("Row {row}"));
                    }
                })
                .inner_rect
        };
        let (view, _) = run.frame(events, draw);
        let theme = run.theme.clone();
        let output = run.ctx.run_ui(egui::RawInput::default(), |ui| {
            let _ = draw(ui, &theme);
        });
        let shapes = output
            .shapes
            .into_iter()
            .map(|clipped| clipped.shape)
            .collect();
        (view, shapes)
    }

    /// The gradient bands among `shapes`, by their top edge.
    fn bands(shapes: &[Shape]) -> Vec<f32> {
        let mut found = Vec::new();
        for shape in shapes {
            match shape {
                Shape::Vec(inner) => found.extend(bands(inner)),
                Shape::Mesh(mesh)
                    if mesh.vertices.len() == 4
                        && mesh.texture_id == egui::TextureId::default() =>
                {
                    let top = mesh
                        .vertices
                        .iter()
                        .map(|vertex| vertex.pos.y)
                        .fold(f32::INFINITY, f32::min);
                    found.push(top);
                }
                _ => {}
            }
        }
        found
    }

    #[test]
    fn a_long_list_fades_at_its_end_and_at_its_start_once_scrolled() {
        let mut run = Run::new();
        let (view, shapes) = list(&mut run, Vec::new());
        let reach = ScrollView::fade_reach(&run.theme);
        assert_eq!(bands(&shapes), [view.bottom() - reach], "only the end");
        let wheel = Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: vec2(0.0, -60.0),
            phase: TouchPhase::Move,
            source: MouseWheelSource::Wheel,
            modifiers: egui::Modifiers::NONE,
        };
        let _ = list(&mut run, vec![Event::PointerMoved(view.center())]);
        let _ = list(&mut run, vec![wheel]);
        let (view, shapes) = run_settled(&mut run);
        let mut seen = bands(&shapes);
        seen.sort_by(f32::total_cmp);
        assert_eq!(seen, [view.top(), view.bottom() - reach], "both ends");
    }

    fn run_settled(run: &mut Run) -> (Rect, Vec<Shape>) {
        let mut last = list(run, Vec::new());
        for _ in 0..20 {
            last = list(run, Vec::new());
        }
        last
    }

    #[test]
    fn the_fades_lie_under_the_bars() {
        let mut run = Run::new();
        let _ = list(&mut run, Vec::new());
        let (view, shapes) = list(&mut run, Vec::new());
        let fade = shapes
            .iter()
            .position(|shape| !bands(std::slice::from_ref(shape)).is_empty())
            .expect("a long list fades");
        // The bar, always shown here, is a plain rectangle along the
        // right edge of the view.
        let bar = shapes
            .iter()
            .rposition(|shape| {
                matches!(shape, Shape::Rect(rect)
                    if rect.fill.a() > 0
                        && rect.rect.left() > view.right() - 12.0
                        && rect.rect.right() <= view.right() + 0.5)
            })
            .expect("the bar");
        assert!(fade < bar, "a fade drawn over the bar dims it");
    }
}
