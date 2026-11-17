//! Where the Timeline palette looks: zoom, pan and scroll, and its
//! scrollbars.

use egui::{CornerRadius, Rect, Sense, Ui, pos2, vec2};
use sue_wm::Theme;
use sue_wm::ui::Radius;

/// Where the Timeline palette looks, as Clip Studio Paint's: how wide a
/// frame is, the frame at the left edge (fractional while it pans) and
/// how far the tracks are scrolled down. Over the frames the wheel zooms
/// around the pointer, Shift and the wheel pan sideways, Alt and the wheel
/// scroll the tracks, and Space and a drag or the middle button pan; the
/// scrollbars along the bottom and right edges scroll it too.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct TimelineView {
    /// Points per frame once people zoomed; `None` fits the frames.
    pub step: Option<f32>,
    /// The frame at the left edge.
    pub first: f32,
    /// How far the tracks are scrolled down, in points.
    pub down: f32,
    /// The current frame the view last made sure to show.
    seen: Option<u32>,
    /// Points per frame the frames were last drawn at while they fit, so
    /// the zoom buttons start from what people see.
    pub fitted: Option<f32>,
}

impl TimelineView {
    /// The narrowest a frame gets.
    pub const MIN_STEP: f32 = 4.0;
    /// The widest a frame gets.
    pub const MAX_STEP: f32 = 60.0;
    /// How many frames before the first the view may show at most (and
    /// never more than a third of what shows), as Clip Studio Paint's
    /// ruler goes on left of frame 1 (numbered 0, -1 and on, shaded):
    /// room to drag the playback range's first handle to the very start.
    pub const BEFORE: f32 = 10.0;

    /// Points per frame: the zoom people chose, or `length` frames fitting
    /// `width`.
    #[must_use]
    pub fn step(&self, width: f32, length: u32) -> f32 {
        self.step
            .unwrap_or_else(|| ((width - 8.0) / length.max(1) as f32).clamp(10.0, 26.0))
    }

    /// Zooms from `step` by `factor`, keeping the frame `at` points right of
    /// the frames' left edge under the pointer. Returns the new step.
    pub fn zoom_at(&mut self, step: f32, at: f32, factor: f32) -> f32 {
        let anchor = self.first + at / step;
        let zoomed = (step * factor).clamp(Self::MIN_STEP, Self::MAX_STEP);
        self.step = Some(zoomed);
        self.first = anchor - at / zoomed;
        zoomed
    }

    /// Moves the frames `points` to the right (earlier frames come in).
    pub fn pan(&mut self, points: f32, step: f32) {
        self.first -= points / step.max(f32::EPSILON);
    }

    /// Moves the tracks `points` down (the ones above come in).
    pub fn scroll(&mut self, points: f32) {
        self.down -= points;
    }

    /// The wheel over the frames, as Clip Studio Paint takes it: `delta`
    /// zooms around `at` (points from the frames' left edge), with Shift
    /// it pans and with Alt it scrolls the tracks. Returns the step.
    pub fn wheel(&mut self, step: f32, at: f32, delta: egui::Vec2, shift: bool, alt: bool) -> f32 {
        if alt && delta.y != 0.0 {
            self.scroll(delta.y);
            step
        } else if shift {
            self.pan(delta.x + delta.y, step);
            step
        } else if delta.y + delta.x != 0.0 {
            self.zoom_at(step, at, ((delta.y + delta.x) * 0.004).exp())
        } else {
            step
        }
    }

    /// Keeps the view on the timeline (and the few frames before it) and
    /// its tracks, and brings `frame`
    /// into view when it changed (playing, stepping). `width` and
    /// `visible` are the frames' width and the tracks' height, `content`
    /// the height of every track. Returns how many frames show.
    pub fn settle(
        &mut self,
        frame: u32,
        length: u32,
        width: f32,
        step: f32,
        content: f32,
        visible: f32,
    ) -> u32 {
        let shown = ((width / step).ceil() as u32).max(1);
        let most = length.saturating_sub(shown.saturating_sub(1)) as f32;
        if self.seen != Some(frame) {
            self.seen = Some(frame);
            let now = self.first.max(0.0);
            let end = now + shown.saturating_sub(1).max(1) as f32;
            if (frame as f32) < now.floor() || frame as f32 >= end {
                self.first = frame.saturating_sub(shown.saturating_sub(4)) as f32;
            }
        }
        // Never so far left that the frames leave most of the width.
        let before = Self::BEFORE.min((shown as f32 / 3.0).floor().max(1.0));
        self.first = self.first.clamp(-before, most);
        self.down = self.down.clamp(0.0, (content - visible).max(0.0));
        shown
    }
}

/// The Timeline's scrollbars, as Clip Studio Paint's: along the bottom of
/// `rows` (the frames under the ruler) when not every frame shows, and
/// along its right edge when not every track does. `frames` is the
/// timeline's length and how many frames show; `heights` every track's
/// height and how much of it shows.
pub(super) fn timeline_scrollbars(
    ui: &Ui,
    view: &mut TimelineView,
    rows: Rect,
    (length, shown): (u32, u32),
    (content, visible): (f32, f32),
    theme: &Theme,
) {
    const THICK: f32 = 9.0;
    let tall = content > visible + 0.5;
    let painter = ui.painter().with_clip_rect(rows);
    let bar = |track: Rect, thumb: Rect, id: &str| {
        let response = ui.interact(track, ui.scope_id().with(id), Sense::drag());
        painter.rect_filled(track, CornerRadius::ZERO, theme.surface(0));
        let lit = response.hovered() || response.dragged();
        let round = CornerRadius::same(theme.radius(Radius::Sm) as u8);
        painter.rect_filled(
            thumb.shrink(1.5),
            round,
            if lit { theme.text } else { theme.text_muted() },
        );
        response.drag_delta()
    };
    if length > shown {
        let most = length.saturating_sub(shown.saturating_sub(1)).max(1) as f32;
        let track = Rect::from_min_max(
            pos2(rows.left(), rows.bottom() - THICK),
            pos2(rows.right() - if tall { THICK } else { 0.0 }, rows.bottom()),
        );
        let size = (track.width() * shown as f32 / length as f32).clamp(24.0, track.width());
        let room = (track.width() - size).max(1.0);
        let thumb = Rect::from_min_size(
            pos2(
                track.left() + room * view.first.max(0.0) / most,
                track.top(),
            ),
            vec2(size, THICK),
        );
        let moved = bar(track, thumb, "timeline-hbar");
        if moved.x != 0.0 {
            view.first = (view.first.max(0.0) + moved.x * most / room).clamp(0.0, most);
        }
    }
    if tall {
        let most = (content - visible).max(1.0);
        let track = Rect::from_min_max(
            pos2(rows.right() - THICK, rows.top()),
            pos2(rows.right(), rows.bottom() - THICK),
        );
        let size = (track.height() * visible / content).clamp(24.0, track.height().max(24.0));
        let room = (track.height() - size).max(1.0);
        let thumb = Rect::from_min_size(
            pos2(track.left(), track.top() + room * view.down / most),
            vec2(THICK, size),
        );
        let moved = bar(track, thumb, "timeline-vbar");
        view.down = (view.down + moved.y * most / room).clamp(0.0, most);
    }
}

#[cfg(test)]
mod tests {
    use super::TimelineView;
    use egui::vec2;

    #[test]
    fn the_wheel_zooms_the_timeline_around_the_pointer() {
        let mut view = TimelineView {
            first: 10.0,
            ..TimelineView::default()
        };
        // 20 points a frame, the pointer 100 points in: frame 15 is there.
        let step = view.wheel(20.0, 100.0, vec2(0.0, 50.0), false, false);
        assert!(step > 20.0, "the wheel up zooms in: {step}");
        assert!(
            (view.first + 100.0 / step - 15.0).abs() < 1e-3,
            "frame 15 stays under the pointer"
        );
        let back = view.wheel(step, 100.0, vec2(0.0, -50.0), false, false);
        assert!((back - 20.0).abs() < 1e-3, "and out again: {back}");
        assert!((view.first - 10.0).abs() < 1e-3);
        // Never past its narrowest or widest.
        for _ in 0..40 {
            view.wheel(view.step.unwrap(), 0.0, vec2(0.0, 200.0), false, false);
        }
        assert_eq!(view.step, Some(TimelineView::MAX_STEP));
        for _ in 0..40 {
            view.wheel(view.step.unwrap(), 0.0, vec2(0.0, -200.0), false, false);
        }
        assert_eq!(view.step, Some(TimelineView::MIN_STEP));
    }

    #[test]
    fn shift_pans_alt_scrolls_and_drags_pan() {
        let mut view = TimelineView {
            first: 10.0,
            ..TimelineView::default()
        };
        // Shift and the wheel: egui hands it on as a sideways scroll.
        let step = view.wheel(20.0, 50.0, vec2(-40.0, 0.0), true, false);
        assert_eq!(step, 20.0, "no zoom");
        assert_eq!(view.first, 12.0, "two frames later");
        assert_eq!(view.step, None);
        // Alt and the wheel scroll the tracks.
        view.wheel(20.0, 50.0, vec2(0.0, -30.0), false, true);
        assert_eq!(view.down, 30.0);
        assert_eq!(view.first, 12.0);
        // A drag (Space or the middle button) moves the frames with it.
        view.pan(60.0, 20.0);
        view.scroll(10.0);
        assert_eq!((view.first, view.down), (9.0, 20.0));
    }

    #[test]
    fn the_view_stays_on_the_timeline_and_follows_the_frame() {
        let mut view = TimelineView::default();
        // 120 frames, 10 points each, 300 points wide: 30 show.
        let shown = view.settle(0, 120, 300.0, 10.0, 400.0, 200.0);
        assert_eq!(shown, 30);
        assert_eq!(view.first, 0.0);
        // Panned far to the left: no further than ten frames before the
        // first (a third of the 30 that show), as the ruler goes on left
        // of frame 1.
        view.pan(5000.0, 10.0);
        view.settle(0, 120, 300.0, 10.0, 400.0, 200.0);
        assert_eq!(view.first, -TimelineView::BEFORE);
        view.pan(-5000.0, 10.0);
        view.scroll(-5000.0);
        view.settle(0, 120, 300.0, 10.0, 400.0, 200.0);
        assert_eq!(view.first, 91.0, "no further than the last frames");
        assert_eq!(view.down, 200.0, "no further than the last track");
        // The frame moving out of view brings the view to it once.
        view.settle(40, 120, 300.0, 10.0, 400.0, 200.0);
        assert_eq!(view.first, 14.0);
        // Panning away from a frame that stays put keeps the pan.
        view.pan(-200.0, 10.0);
        view.settle(40, 120, 300.0, 10.0, 400.0, 200.0);
        assert_eq!(view.first, 34.0);
        // Everything fits: nothing to scroll.
        let mut view = TimelineView::default();
        view.scroll(-50.0);
        view.pan(30.0, 24.0);
        view.settle(0, 12, 300.0, 24.0, 100.0, 200.0);
        assert_eq!((view.first, view.down), (-1.25, 0.0), "only the pan");
        assert_eq!(view.step(300.0, 12), 24.333_334);
    }
}
