//! Progress: the bar, the spinner and the skeleton.
//!
//! - [`ProgressBar`]: how far a piece of work is, or, when nobody knows
//!   how far, that it is going on.
//! - [`Spinner`]: something small is being waited for.
//! - [`Skeleton`]: the shape of what is still loading, where it will be.
//!
//! All three are monochrome and say what they say by shape. The ones that
//! move ask for new frames only while they are on screen, and stand still
//! when `motion.instant_transitions` is on (or the hardware profile
//! forbids motion): a still bar, a still ring and a still block say the
//! same thing without a frame being drawn for them.

use std::f32::consts::TAU;

use egui::{
    Color32, CornerRadius, Pos2, Rect, Response, Role, Sense, Stroke, Ui, Vec2, WidgetInfo, pos2,
    vec2,
};

use super::list::blend;
use super::motion::Motion;
use super::tokens::{Density, Radius, Space, TextRole};
use crate::Theme;

/// How long the moving part of a bar that knows no fraction takes to
/// cross it, at `motion.animation_speed` 1, in seconds.
const SWEEP_SECONDS: f64 = 1.4;

/// How long a spinner takes to turn once, in seconds.
const TURN_SECONDS: f64 = 0.9;

/// How long a skeleton takes to breathe in and out once, in seconds.
const BREATH_SECONDS: f64 = 1.6;

/// Where a movement that repeats every `seconds` is in its round, from 0
/// to 1, or `None` when nothing may move. It asks for the next frame.
fn round(ui: &Ui, seconds: f64) -> Option<f32> {
    let motion = Motion::of(ui.ctx());
    if motion.instant {
        return None;
    }
    let period = seconds / f64::from(motion.speed.max(0.01));
    let time = ui.input(|input| input.time);
    ui.ctx().request_repaint();
    Some((time.rem_euclid(period) / period) as f32)
}

/// A bar that fills from its start as work gets done.
#[derive(Clone, Copy, Debug)]
#[must_use = "a progress bar draws nothing until it is shown"]
pub struct ProgressBar<'a> {
    fraction: Option<f32>,
    width: Option<f32>,
    label: &'a str,
}

impl<'a> ProgressBar<'a> {
    /// A bar `fraction` full, from 0 (nothing done) to 1 (all of it).
    pub const fn new(fraction: f32) -> Self {
        Self {
            fraction: Some(fraction),
            width: None,
            label: "",
        }
    }

    /// A bar for work whose end nobody knows: a part of it travels along
    /// it for as long as it is shown.
    pub const fn indeterminate() -> Self {
        Self {
            fraction: None,
            width: None,
            label: "",
        }
    }

    /// Its width; without one it fills the width there is.
    pub const fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// What the work is, for a screen reader.
    pub const fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// How thick a bar is at `density`.
    #[must_use]
    pub fn thickness(theme: &Theme, density: Density) -> f32 {
        theme.space(match density {
            Density::Compact => Space::S2,
            Density::Regular => Space::S3,
            Density::Touch => Space::S4,
        })
    }

    /// The part of `track` that is filled when `fraction` is done. A
    /// fraction that is not a number counts as nothing done.
    #[must_use]
    pub fn filled(track: Rect, fraction: f32) -> Rect {
        let done = if fraction.is_nan() {
            0.0
        } else {
            fraction.clamp(0.0, 1.0)
        };
        Rect::from_min_size(track.min, vec2(track.width() * done, track.height()))
    }

    /// Draws the bar.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> Response {
        let thick = Self::thickness(theme, Density::of(ui));
        let width = self.width.unwrap_or_else(|| ui.available_width());
        let (rect, response) = ui.allocate_exact_size(vec2(width, thick), Sense::hover());
        let (label, fraction) = (self.label, self.fraction);
        response.widget_info(|| {
            let mut info = WidgetInfo::labeled(Role::ProgressIndicator, true, label);
            info.value = fraction.map(|done| f64::from(done.clamp(0.0, 1.0)));
            info
        });
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let round_corners = theme.radius(Radius::Full);
        let painter = ui.painter();
        painter.rect_filled(rect, round_corners, theme.fill_pressed());
        match self.fraction {
            Some(fraction) => {
                let done = Self::filled(rect, fraction);
                if done.width() > 0.0 {
                    // Never narrower than it is thick, so the first
                    // percent shows as a dot and not as a sliver.
                    let done = Rect::from_min_size(
                        done.min,
                        vec2(done.width().max(thick).min(rect.width()), thick),
                    );
                    painter.rect_filled(done, round_corners, theme.text);
                }
            }
            None => match round(ui, SWEEP_SECONDS) {
                Some(at) => {
                    // A third of the bar that enters at the start and
                    // leaves at the end.
                    let part = rect.width() / 3.0;
                    let left = rect.left() - part + (rect.width() + part) * at;
                    let moving = Rect::from_min_max(
                        pos2(left.max(rect.left()), rect.top()),
                        pos2((left + part).min(rect.right()), rect.bottom()),
                    );
                    if moving.width() > 0.0 {
                        painter.rect_filled(moving, round_corners, theme.text);
                    }
                }
                // Standing still, a part of the bar would read as that
                // much done: the whole bar in the muted color does not.
                None => {
                    painter.rect_filled(rect, round_corners, theme.text_muted());
                }
            },
        }
        response
    }
}

/// A ring that turns while something is being waited for.
#[derive(Clone, Copy, Debug, Default)]
#[must_use = "a spinner draws nothing until it is shown"]
pub struct Spinner<'a> {
    size: Option<f32>,
    label: &'a str,
}

impl<'a> Spinner<'a> {
    /// A spinner as large as an icon of the density it is drawn at.
    pub const fn new() -> Self {
        Self {
            size: None,
            label: "",
        }
    }

    /// How large it is across, in points.
    pub const fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// What is being waited for, for a screen reader.
    pub const fn label(mut self, label: &'a str) -> Self {
        self.label = label;
        self
    }

    /// Draws the spinner.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> Response {
        let size = self.size.unwrap_or_else(|| Density::of(ui).icon());
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
        let label = self.label;
        response.widget_info(|| WidgetInfo::labeled(Role::ProgressIndicator, true, label));
        if ui.is_rect_visible(rect) {
            let thick = theme.icon_stroke(size).max(1.0);
            let radius = (size - thick) / 2.0;
            let center = rect.center();
            let painter = ui.painter();
            painter.circle_stroke(center, radius, Stroke::new(thick, theme.fill_pressed()));
            // A third of the ring, starting where the round is.
            let start = round(ui, TURN_SECONDS).unwrap_or(0.0) * TAU - TAU / 4.0;
            let steps = ((radius * 1.5).ceil() as usize).clamp(12, 96);
            let points: Vec<Pos2> = (0..=steps)
                .map(|step| {
                    let angle = start + TAU / 3.0 * step as f32 / steps as f32;
                    center + vec2(angle.cos(), angle.sin()) * radius
                })
                .collect();
            painter.line(points, Stroke::new(thick, theme.text));
        }
        response
    }
}

/// The shape of something that is still loading, in its place.
#[derive(Clone, Copy, Debug)]
#[must_use = "a skeleton draws nothing until it is shown"]
pub struct Skeleton {
    size: Vec2,
    radius: Radius,
    /// The part of the height that is drawn: a line of text is thinner
    /// than the line it stands in.
    share: f32,
}

impl Skeleton {
    /// A block `size` large: a picture, a card, a button.
    pub const fn block(size: Vec2) -> Self {
        Self {
            size,
            radius: Radius::Md,
            share: 1.0,
        }
    }

    /// A line of text of `role`, `width` wide.
    pub fn text(theme: &Theme, role: TextRole, width: f32) -> Self {
        Self {
            size: vec2(width, theme.text_size(role)),
            radius: Radius::Full,
            share: 0.7,
        }
    }

    /// How round its corners are.
    pub const fn radius(mut self, radius: Radius) -> Self {
        self.radius = radius;
        self
    }

    /// Draws the skeleton.
    pub fn show(self, ui: &mut Ui, theme: &Theme) -> Response {
        let (rect, response) = ui.allocate_exact_size(self.size, Sense::hover());
        if ui.is_rect_visible(rect) {
            let drawn = Rect::from_center_size(
                rect.center(),
                vec2(rect.width(), rect.height() * self.share),
            );
            paint_skeleton(ui, drawn, theme.radius(self.radius), theme);
        }
        response
    }
}

/// The fill of a skeleton now: between the hover and the pressed fill,
/// breathing; the hover fill when nothing may move.
fn skeleton_fill(ui: &Ui, theme: &Theme) -> Color32 {
    match round(ui, BREATH_SECONDS) {
        Some(at) => blend(
            theme.fill_hover(),
            theme.fill_pressed(),
            0.5 - 0.5 * (at * TAU).cos(),
        ),
        None => theme.fill_hover(),
    }
}

/// Paints a skeleton over `rect`, whose corners have `radius`: for a
/// component that shows one inside itself while its content loads.
pub fn paint_skeleton(ui: &Ui, rect: Rect, radius: impl Into<CornerRadius>, theme: &Theme) {
    if ui.is_rect_visible(rect) {
        ui.painter()
            .rect_filled(rect, radius, skeleton_fill(ui, theme));
    }
}

/// A [`ProgressBar`] `fraction` full.
pub const fn progress_bar<'a>(fraction: f32) -> ProgressBar<'a> {
    ProgressBar::new(fraction)
}

/// A [`Spinner`] as large as an icon.
pub const fn spinner<'a>() -> Spinner<'a> {
    Spinner::new()
}

/// A [`Skeleton`] block `size` large.
pub const fn skeleton(size: Vec2) -> Skeleton {
    Skeleton::block(size)
}

#[cfg(test)]
mod tests {
    use egui::{Context, RawInput};
    use sue_rice::RiceSchema;

    use super::*;

    fn theme() -> Theme {
        Theme::from_rice(&RiceSchema::default())
    }

    /// One frame of `add`; returns whether the frame asked for another.
    fn frame(ctx: &Context, time: f64, add: impl FnMut(&mut Ui)) -> bool {
        let input = RawInput {
            time: Some(time),
            ..Default::default()
        };
        let mut done = ctx.run_ui(input, add);
        done.textures_delta.clear();
        done.viewport_output
            .values()
            .any(|viewport| viewport.repaint_delay.is_zero())
    }

    #[test]
    fn the_filled_part_follows_the_fraction_and_never_leaves_the_track() {
        let track = Rect::from_min_size(pos2(10.0, 20.0), vec2(200.0, 6.0));
        assert_eq!(ProgressBar::filled(track, 0.0).width(), 0.0);
        assert_eq!(ProgressBar::filled(track, 0.25).width(), 50.0);
        assert_eq!(ProgressBar::filled(track, 1.0), track);
        // Out of range, and not a number.
        assert_eq!(ProgressBar::filled(track, 7.0), track);
        assert_eq!(ProgressBar::filled(track, -1.0).width(), 0.0);
        assert_eq!(ProgressBar::filled(track, f32::NAN).width(), 0.0);
        assert_eq!(ProgressBar::filled(track, f32::INFINITY), track);
    }

    #[test]
    fn a_bar_is_thicker_where_fingers_are() {
        let theme = theme();
        let sizes = Density::ALL.map(|density| ProgressBar::thickness(&theme, density));
        assert_eq!(sizes, [4.0, 6.0, 8.0]);
    }

    #[test]
    fn a_bar_that_knows_its_fraction_asks_for_no_frames() {
        let mut run = super::super::list::harness::Run::new();
        let still = |ui: &mut Ui, theme: &Theme| {
            ProgressBar::new(0.4).label("Saving").show(ui, theme);
        };
        run.settle(still);
        for _ in 0..3 {
            assert!(!run.frame(Vec::new(), still).1, "a still bar draws nothing");
        }
    }

    #[test]
    fn what_moves_asks_for_frames_and_stands_still_when_motion_is_off() {
        let theme = theme();
        let moving = |ui: &mut Ui| {
            ProgressBar::indeterminate().show(ui, &theme);
            Spinner::new().label("Loading").show(ui, &theme);
            Skeleton::block(vec2(80.0, 40.0)).show(ui, &theme);
            Skeleton::text(&theme, TextRole::Body, 120.0).show(ui, &theme);
        };
        let ctx = Context::default();
        theme.motion.store(&ctx);
        let _ = frame(&ctx, 0.0, moving);
        assert!(frame(&ctx, 0.1, moving), "it moves, so it asks");
        // The same with motion switched off.
        let ctx = Context::default();
        Motion {
            instant: true,
            ..theme.motion
        }
        .store(&ctx);
        let _ = frame(&ctx, 0.0, moving);
        let _ = frame(&ctx, 0.1, moving);
        assert!(!frame(&ctx, 0.2, moving), "nothing moves, nothing is drawn");
    }

    #[test]
    fn a_round_follows_the_clock_and_the_speed_setting() {
        let theme = theme();
        let at = |speed: f32, time: f64| {
            let ctx = Context::default();
            Motion {
                speed,
                ..theme.motion
            }
            .store(&ctx);
            let mut seen = None;
            let _ = frame(&ctx, time, |ui| seen = round(ui, 2.0));
            seen.unwrap()
        };
        assert!((at(1.0, 0.5) - 0.25).abs() < 1e-4);
        assert!((at(1.0, 2.5) - 0.25).abs() < 1e-4, "it repeats");
        assert!((at(2.0, 0.5) - 0.5).abs() < 1e-4, "twice as fast");
    }
}
