//! The caret's motion: it glides to its new place instead of jumping, and
//! blinks only while it rests.
//!
//! The approach is WordCraft's (`github.com/storytold/wordcraft`, MIT or
//! Apache 2.0), which measured it from Word for Windows:
//!
//! - **A move glides.** The caret's bar eases from where it was to where it
//!   belongs, its top and its bottom both, so its height changes on the way
//!   too (the taller caret of a heading). A glide takes the same time,
//!   [`GLIDE_MS`] unless the rice says otherwise, for one character and for
//!   a whole line, along Word's curve [`ease`]: more than a third of the way
//!   in the first frame, then settling. Typing, Backspace and Delete, the
//!   arrows, Home and End, a click, and text wrapping to a new line or page
//!   all move it this way.
//! - **Another move before it arrives** sets off from where the caret was
//!   heading, not from where it has got to: fast typing and held keys never
//!   leave it more than one move behind the text, and it never goes back
//!   toward where it started.
//! - **It blinks only at rest.** A gliding caret stays lit; once it lands
//!   the blink starts over, lit first. After [`BLINK_FOR`] seconds without a
//!   move it stays lit, so an idle editor draws nothing.
//! - **It is simply there** when the view changed (zoom, a resized window,
//!   other documents), when it was not drawn the frame before, when a move
//!   starts off screen or goes further than the view is tall (Ctrl+End), and
//!   while the pointer drags a selection.
//!
//! Unlike Word, a click glides too. The glide runs in content space (points
//! from the top left of the scrolled content), so scrolling under a moving
//! caret does not bend its path, and on egui's clock, so it looks the same
//! at any frame rate. Nothing asks for a frame unless the caret moves or its
//! blink turns.

use egui::{Rect, vec2};

/// How long a glide takes unless the rice says otherwise (ms).
pub const GLIDE_MS: i64 = 100;
/// The longest glide the rice can ask for (ms).
pub const MAX_GLIDE_MS: i64 = 400;
/// How long the caret blinks after its last move (seconds); then it stays
/// lit.
pub const BLINK_FOR: f64 = 10.0;
/// Moves shorter than this (points) are not worth a glide.
const MIN_MOVE: f32 = 0.5;
/// How far outside the view a glide may start (points).
const OFFSCREEN_SLACK: f32 = 48.0;

/// How the caret moves and blinks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretMotion {
    /// How long a glide takes (seconds); 0 jumps.
    pub glide: f64,
    /// How long the caret is lit, then dark, in a blink (seconds); `None`
    /// keeps it lit.
    pub blink: Option<(f64, f64)>,
}

impl CaretMotion {
    /// A caret that jumps and never blinks.
    pub const STILL: Self = Self {
        glide: 0.0,
        blink: None,
    };

    /// The motion asked for: `smooth` and `glide_ms` from the rice
    /// (`motion.smooth_caret`, `motion.caret_glide_ms`), `animate` from the
    /// hardware profile (a legacy machine snaps) and the blink from egui's
    /// text cursor style (`motion.caret_blink_ms`).
    #[must_use]
    pub fn new(
        smooth: bool,
        glide_ms: i64,
        animate: bool,
        cursor: &egui::style::TextCursorStyle,
    ) -> Self {
        let glide = if smooth && animate {
            glide_ms.clamp(0, MAX_GLIDE_MS) as f64 / 1000.0
        } else {
            0.0
        };
        let blink =
            (cursor.blink && cursor.on_duration > 0.0 && cursor.off_duration > 0.0).then(|| {
                (
                    f64::from(cursor.on_duration),
                    f64::from(cursor.off_duration),
                )
            });
        Self { glide, blink }
    }
}

/// What the editor knows about the caret in one frame.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// Where the caret belongs, in content space.
    pub target: Rect,
    /// What puts it there: the caret and its document's revision. The
    /// caret moves (and glides) only when this changes; otherwise the text
    /// moved under it and it follows at once.
    pub cause: u64,
    /// How the documents are shown: zoom, width, which documents.
    pub view: u64,
    /// The caret is drawn this frame (the editor has the keyboard).
    pub shown: bool,
    /// The part of content space on screen.
    pub viewport: Rect,
    /// egui's clock, in seconds.
    pub now: f64,
    /// The pointer drags a selection: the caret keeps to it.
    pub dragging: bool,
}

/// How to draw the caret this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Where, in content space.
    pub bar: Rect,
    /// Drawn at all (`false` while the blink has it dark).
    pub lit: bool,
    /// On its way: draw the next frame at once.
    pub moving: bool,
    /// The blink turns in this many seconds: draw a frame then. `None`:
    /// nothing changes until something moves it.
    pub wake: Option<f64>,
}

#[derive(Clone, Copy, Debug)]
struct Glide {
    from: Rect,
    to: Rect,
    started: f64,
}

/// The caret's motion between frames.
#[derive(Clone, Debug, Default)]
pub struct CaretGlide {
    /// Where the caret belonged last frame; `None` when it was not drawn.
    target: Option<Rect>,
    /// Where it was drawn last frame.
    drawn: Option<Rect>,
    glide: Option<Glide>,
    cause: u64,
    view: u64,
    /// When the blink starts over: the end of the last move.
    rested: f64,
}

impl CaretGlide {
    /// Advances to this frame; returns where and how to draw the caret.
    pub fn frame(&mut self, f: Frame, motion: CaretMotion) -> Look {
        let was = self.target;
        let edited = f.cause != self.cause;
        let same_view = f.view == self.view;
        self.cause = f.cause;
        self.view = f.view;
        if !(f.shown && is_finite(f.target) && f.now.is_finite()) {
            self.reset();
            return Look {
                bar: f.target,
                lit: false,
                moving: false,
                wake: None,
            };
        }
        if was.is_none() {
            // Coming back: lit, and the blink starts over.
            self.rested = f.now;
        }
        if !same_view || motion.glide <= 0.0 {
            self.glide = None;
        }
        // Where the caret was heading: its place last frame, or the end of
        // the glide under way.
        let heading = self.glide.map(|glide| glide.to).or(was);
        if edited {
            let moved = heading.is_some_and(|from| distance(from, f.target) >= MIN_MOVE);
            let glide = heading.filter(|from| {
                moved
                    && motion.glide > 0.0
                    && same_view
                    && !f.dragging
                    && within_reach(*from, f.target, f.viewport)
            });
            self.glide = glide.map(|from| Glide {
                from,
                to: f.target,
                started: f.now,
            });
            self.rested = f.now
                + if self.glide.is_some() {
                    motion.glide
                } else {
                    0.0
                };
        }
        // The text moved under a glide with nothing edited (a reflow):
        // carry the glide along.
        if let Some(glide) = &mut self.glide
            && glide.to != f.target
        {
            glide.from = glide.from.translate(f.target.min - glide.to.min);
            glide.to = f.target;
        }
        let bar = match self.glide {
            Some(glide) => match progress(f.now, glide.started, motion.glide) {
                Some(s) => lerp(glide.from, glide.to, ease(s)),
                None => {
                    self.glide = None;
                    f.target
                }
            },
            None => f.target,
        };
        let moving = self.glide.is_some();
        let (lit, wake) = if moving {
            (true, None)
        } else {
            blink(f.now - self.rested, motion.blink)
        };
        self.target = Some(f.target);
        self.drawn = Some(bar);
        Look {
            bar,
            lit,
            moving,
            wake,
        }
    }

    /// The caret is on its way somewhere.
    #[must_use]
    pub fn is_gliding(&self) -> bool {
        self.glide.is_some()
    }

    /// Where the caret was drawn last frame (content space), if it was.
    #[must_use]
    pub fn drawn(&self) -> Option<Rect> {
        self.drawn
    }

    /// Forgets the caret: it appears wherever it belongs next frame.
    pub fn reset(&mut self) {
        self.glide = None;
        self.target = None;
        self.drawn = None;
    }
}

/// How far along a glide that started at `started` and lasts `secs` is at
/// `now`: 0 to 1, or `None` once it is over (or the clock went back).
fn progress(now: f64, started: f64, secs: f64) -> Option<f32> {
    let s = (now - started) / secs;
    (0.0..1.0).contains(&s).then_some(s as f32)
}

/// Word's curve for the glide, `cubic-bezier(0, 0, 0, 1)` as WordCraft
/// measured it: more than a third of the way after 7% of the time, nine
/// tenths after half of it. For these control points the curve has a closed
/// form: time is `u^3` and distance `3u^2 - 2u^3`.
#[must_use]
pub fn ease(s: f32) -> f32 {
    let s = if s.is_finite() {
        s.clamp(0.0, 1.0)
    } else {
        1.0
    };
    (3.0 * s.powf(2.0 / 3.0) - 2.0 * s).clamp(0.0, 1.0)
}

/// Whether a resting caret is lit `since` seconds after its blink started
/// over (lit for `on`, dark for `off`, and again), and in how many seconds
/// that changes. After [`BLINK_FOR`] it stays lit.
#[must_use]
pub fn blink(since: f64, blink: Option<(f64, f64)>) -> (bool, Option<f64>) {
    let Some((on, off)) = blink else {
        return (true, None);
    };
    if !since.is_finite() || since >= BLINK_FOR {
        return (true, None);
    }
    let since = since.max(0.0);
    let phase = since % (on + off);
    let (lit, turn) = if phase < on {
        (true, on - phase)
    } else {
        (false, on + off - phase)
    };
    (lit, Some(turn.min(BLINK_FOR - since)))
}

fn lerp(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::from_min_max(a.min.lerp(b.min, t), a.max.lerp(b.max, t))
}

/// How far the bar's farther corner moves.
fn distance(a: Rect, b: Rect) -> f32 {
    (b.min - a.min).length().max((b.max - a.max).length())
}

/// A glide starts on screen and goes no further than the view is tall (the
/// view scrolls along); a longer jump is simply there.
fn within_reach(from: Rect, to: Rect, viewport: Rect) -> bool {
    let seen = viewport.expand(OFFSCREEN_SLACK);
    let reach = viewport.size().max(vec2(200.0, 200.0));
    let step = (to.center() - from.center()).abs();
    seen.intersects(from) && step.x <= reach.x && step.y <= reach.y
}

fn is_finite(rect: Rect) -> bool {
    rect.min.x.is_finite()
        && rect.min.y.is_finite()
        && rect.max.x.is_finite()
        && rect.max.y.is_finite()
}

#[cfg(test)]
mod tests {
    use egui::pos2;

    use super::*;

    const DT: f64 = 1.0 / 60.0;
    const SMOOTH: CaretMotion = CaretMotion {
        glide: 0.1,
        blink: Some((0.53, 0.53)),
    };

    fn bar(x: f32, y: f32) -> Rect {
        Rect::from_min_max(pos2(x, y), pos2(x + 1.0, y + 16.0))
    }

    /// A caret driven frame by frame, as the editor drives it.
    struct Rig {
        glide: CaretGlide,
        motion: CaretMotion,
        f: Frame,
    }

    impl Rig {
        fn new(at: Rect) -> Self {
            let f = Frame {
                target: at,
                cause: 1,
                view: 1,
                shown: true,
                viewport: Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 800.0)),
                now: 100.0,
                dragging: false,
            };
            let mut rig = Self {
                glide: CaretGlide::default(),
                motion: SMOOTH,
                f,
            };
            let first = rig.glide.frame(rig.f, rig.motion);
            assert_eq!((first.bar, first.lit, first.moving), (at, true, false));
            rig
        }

        /// The next frame, `dt` seconds later.
        fn step(&mut self, dt: f64) -> Look {
            self.f.now += dt;
            self.glide.frame(self.f, self.motion)
        }

        /// A key, a click or wrapping text moves the caret this frame.
        fn move_to(&mut self, to: Rect) -> Look {
            self.f.target = to;
            self.f.cause += 1;
            self.step(DT)
        }

        /// Lets `secs` pass.
        fn rest(&mut self, secs: f64) -> Look {
            let mut look = self.step(DT);
            for _ in 1..(secs / DT).round() as usize {
                look = self.step(DT);
            }
            look
        }
    }

    /// WordCraft's measurements of Word's caret, frame by frame at 144 Hz,
    /// as fractions of the way (the same for moves of 6 and of 372 px).
    #[test]
    fn the_glide_follows_words_curve_and_ends_exactly() {
        let measured = [
            0.369, 0.529, 0.640, 0.724, 0.790, 0.843, 0.885, 0.918, 0.945, 0.965, 0.980, 0.991,
            0.997, 1.0,
        ];
        for (frame, want) in measured.iter().enumerate() {
            let s = (frame + 1) as f32 * (1.0 / 144.0) / 0.1;
            let got = ease(s);
            assert!(
                (got - want).abs() < 0.004,
                "frame {}: {got} vs {want}",
                frame + 1
            );
        }
        assert_eq!((ease(0.0), ease(1.0)), (0.0, 1.0));
        assert!(ease(0.5) > 0.88, "nine tenths of the way in half the time");
        assert_eq!((ease(-3.0), ease(7.0), ease(f32::NAN)), (0.0, 1.0, 1.0));
        let mut last = 0.0;
        for step in 0..=1000 {
            let p = ease(step as f32 / 1000.0);
            assert!(p >= last && p <= 1.0, "never back, never past the end");
            last = p;
        }
    }

    #[test]
    fn a_move_glides_position_and_height_and_lands_on_the_target() {
        let mut r = Rig::new(bar(100.0, 50.0));
        // To a heading's taller caret on the next line.
        let to = Rect::from_min_max(pos2(40.0, 90.0), pos2(41.0, 120.0));
        let first = r.move_to(to);
        assert_eq!(first.bar, bar(100.0, 50.0), "it sets off from where it was");
        assert!(first.moving && first.lit && first.wake.is_none());
        let mut frames = 0;
        let mut last = first.bar;
        while r.glide.is_gliding() {
            let look = r.step(DT);
            let b = look.bar;
            assert!(
                b.min.x <= last.min.x && b.min.x >= 40.0,
                "toward it, never past: {b:?}"
            );
            assert!(b.min.y >= last.min.y && b.min.y <= 90.0, "{b:?}");
            assert!(
                b.height() >= 16.0 && b.height() <= 30.0,
                "its height eases: {b:?}"
            );
            assert!(look.lit, "a gliding caret stays lit");
            last = b;
            frames += 1;
            assert!(frames < 60);
        }
        // 0.1 s at 60 Hz.
        assert!((5..=7).contains(&frames), "{frames} frames");
        assert_eq!(r.glide.drawn(), Some(to), "exactly on the target");
        let rest = r.step(DT);
        assert_eq!((rest.bar, rest.moving), (to, false), "then it rests");
    }

    #[test]
    fn the_glide_is_the_same_at_any_frame_rate() {
        let at = |hz: f64| {
            let mut r = Rig::new(bar(0.0, 0.0));
            r.f.target = bar(300.0, 0.0);
            r.f.cause += 1;
            r.step(1.0 / hz);
            let mut b = bar(0.0, 0.0);
            // 0.05 s in: three frames at 60 Hz, twelve at 240 Hz.
            for _ in 0..(hz / 20.0).round() as usize {
                b = r.step(1.0 / hz).bar;
            }
            b.min.x
        };
        let expected = 300.0 * ease(0.5);
        assert!(
            (at(60.0) - expected).abs() < 0.5,
            "{} vs {expected}",
            at(60.0)
        );
        assert!(
            (at(240.0) - expected).abs() < 0.5,
            "{} vs {expected}",
            at(240.0)
        );
    }

    /// Held keys and fast typing: each move sets off from where the caret
    /// was heading, so it never goes back and never falls behind the text.
    #[test]
    fn a_new_target_during_a_glide_retargets_with_no_jump_back() {
        let mut r = Rig::new(bar(0.0, 0.0));
        r.move_to(bar(10.0, 0.0));
        let mid = r.step(DT).bar;
        assert!(mid.min.x > 0.0 && mid.min.x < 10.0);
        let next = r.move_to(bar(20.0, 0.0)).bar;
        assert_eq!(next, bar(10.0, 0.0), "on from where it was heading");
        assert!(next.min.x >= mid.min.x, "never back toward the start");
        // Typing at key-repeat speed (30 a second, 10 points a character):
        // the caret only goes forward and is never a whole character
        // behind the text.
        let mut last = next.min.x;
        let mut x = 20.0;
        for _ in 0..30 {
            x += 10.0;
            r.f.target = bar(x, 0.0);
            r.f.cause += 1;
            for _ in 0..2 {
                let drawn = r.step(1.0 / 60.0).bar.min.x;
                assert!(drawn >= last, "{drawn} after {last}");
                assert!(x - drawn <= 10.0, "{drawn} lags the text at {x}");
                last = drawn;
            }
        }
        r.rest(0.2);
        assert_eq!(r.glide.drawn(), Some(bar(x, 0.0)));
        assert!(!r.glide.is_gliding());
    }

    #[test]
    fn it_blinks_only_at_rest_and_starts_over_after_each_move() {
        assert_eq!(blink(0.0, SMOOTH.blink), (true, Some(0.53)));
        assert!(!blink(0.6, SMOOTH.blink).0);
        assert!(blink(1.1, SMOOTH.blink).0);
        assert_eq!(blink(3.0, None), (true, None), "a solid caret never wakes");
        assert_eq!(
            blink(BLINK_FOR, SMOOTH.blink),
            (true, None),
            "then it rests lit"
        );
        let (lit, wake) = blink(BLINK_FOR - 0.1, SMOOTH.blink);
        assert!(
            wake.is_some_and(|wake| wake <= 0.1 + 1e-9),
            "{lit} {wake:?}"
        );
        assert_eq!(blink(f64::NAN, SMOOTH.blink), (true, None));

        let mut r = Rig::new(bar(0.0, 0.0));
        assert!(r.rest(0.3).lit);
        assert!(!r.rest(0.4).lit, "dark after 0.53 s");
        // A move while it is dark: lit at once and all the way.
        let look = r.move_to(bar(50.0, 0.0));
        assert!(look.lit && look.moving);
        while r.glide.is_gliding() {
            assert!(r.step(DT).lit);
        }
        // Then the blink starts over, lit first, from where it landed.
        let look = r.rest(0.4);
        assert!(look.lit && !look.moving && look.wake.is_some());
        assert!(!r.rest(0.3).lit);
        // An edit that leaves the caret where it is (Delete) starts it over too.
        r.f.cause += 1;
        let look = r.step(DT);
        assert!(look.lit && !look.moving);
    }

    #[test]
    fn an_idle_caret_asks_for_no_frames() {
        let mut r = Rig::new(bar(0.0, 0.0));
        r.move_to(bar(40.0, 0.0));
        r.rest(0.2);
        let look = r.step(DT);
        assert!(!look.moving, "no frame at once once it has landed");
        assert!(
            look.wake.is_some_and(|wake| wake > 0.2),
            "only the blink, later"
        );
        let look = r.rest(BLINK_FOR + 1.0);
        assert_eq!(
            (look.lit, look.moving, look.wake),
            (true, false, None),
            "after a while it rests lit and nothing wakes"
        );
        // With the blink turned off it never wakes at all.
        r.motion.blink = None;
        r.move_to(bar(80.0, 0.0));
        let look = r.rest(0.2);
        assert_eq!((look.moving, look.wake), (false, None));
    }

    #[test]
    fn view_changes_drags_far_jumps_and_hidden_carets_never_glide() {
        let mut r = Rig::new(bar(100.0, 100.0));
        // Zoom or a resized window.
        r.f.view += 1;
        assert_eq!(r.move_to(bar(180.0, 150.0)).bar, bar(180.0, 150.0));
        assert!(!r.glide.is_gliding());
        // A view change during a glide ends it.
        r.move_to(bar(300.0, 150.0));
        assert!(r.glide.is_gliding());
        r.f.view += 1;
        assert_eq!(r.step(DT).bar, bar(300.0, 150.0));
        assert!(!r.glide.is_gliding());
        // The text moving under a resting caret (a reflow) is followed.
        r.f.target = bar(300.0, 152.0);
        assert_eq!(r.step(DT).bar, bar(300.0, 152.0));
        assert!(!r.glide.is_gliding());
        // Dragging a selection keeps the caret on the pointer.
        r.f.dragging = true;
        assert_eq!(r.move_to(bar(400.0, 200.0)).bar, bar(400.0, 200.0));
        r.f.dragging = false;
        // Ctrl+End, far below the view, and back.
        assert_eq!(r.move_to(bar(200.0, 5000.0)).bar, bar(200.0, 5000.0));
        assert_eq!(r.move_to(bar(200.0, 100.0)).bar, bar(200.0, 100.0));
        assert!(!r.glide.is_gliding());
        // The next page, just below the view, still glides (the view follows).
        r.move_to(bar(60.0, 900.0));
        assert!(r.glide.is_gliding());
        r.rest(0.2);
        // Hidden (the editor lost the keyboard), then back somewhere else.
        r.f.shown = false;
        assert!(!r.move_to(bar(10.0, 10.0)).lit);
        assert_eq!(r.glide.drawn(), None);
        r.f.shown = true;
        let look = r.move_to(bar(20.0, 10.0));
        assert_eq!(
            (look.bar, look.moving, look.lit),
            (bar(20.0, 10.0), false, true)
        );
    }

    #[test]
    fn turned_off_or_hostile_numbers_never_glide_or_panic() {
        let cursor = egui::style::TextCursorStyle::default();
        let off = CaretMotion::new(false, 100, true, &cursor);
        assert_eq!(off.glide, 0.0, "the switch turns it off");
        assert_eq!(
            CaretMotion::new(true, 100, false, &cursor).glide,
            0.0,
            "a legacy machine snaps"
        );
        assert_eq!(CaretMotion::new(true, 250, true, &cursor).glide, 0.25);
        assert_eq!(CaretMotion::new(true, 9_000, true, &cursor).glide, 0.4);
        assert_eq!(CaretMotion::new(true, -5, true, &cursor).glide, 0.0);
        let solid = egui::style::TextCursorStyle {
            blink: false,
            ..cursor
        };
        assert_eq!(CaretMotion::new(true, 100, true, &solid).blink, None);

        let mut r = Rig::new(bar(0.0, 0.0));
        r.motion = off;
        let look = r.move_to(bar(90.0, 0.0));
        assert_eq!((look.bar, look.moving), (bar(90.0, 0.0), false), "it jumps");
        r.motion = SMOOTH;
        let junk = Rect::from_min_max(pos2(f32::NAN, 0.0), pos2(f32::INFINITY, 1.0));
        assert!(!r.move_to(junk).lit);
        assert!(!r.glide.is_gliding() && r.glide.drawn().is_none());
        assert_eq!(
            r.move_to(bar(50.0, 0.0)).bar,
            bar(50.0, 0.0),
            "back from junk: in place"
        );
        // The clock stops or goes back mid-glide.
        r.move_to(bar(150.0, 0.0));
        assert!(is_finite(r.step(0.0).bar));
        assert_eq!(r.step(-50.0).bar, bar(150.0, 0.0));
        r.move_to(bar(250.0, 0.0));
        assert!(
            !r.step(f64::NAN).lit,
            "a bad clock: nothing drawn that frame"
        );
        r.f.now = 200.0;
        let look = r.move_to(bar(260.0, 0.0));
        assert!(is_finite(look.bar));
    }
}
