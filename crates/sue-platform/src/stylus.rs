//! A pen's pressure where the window's touch events do not carry it: a
//! WinTab tablet on Windows (tablet drivers with Windows Ink turned off,
//! common among artists), a pen tablet on Linux and the BSDs (Wayland's
//! tablet-v2, or XInput2 under X11 and XWayland), and a pen in a browser,
//! where winit treats pens as mice. With Windows Ink, on macOS and phones
//! the pressure arrives with egui's touch events instead.
//!
//! The host updates it every frame; studios read [`pressure`] when a
//! stroke's touch events bring none, and [`tilt`] and [`eraser`] where a
//! source reports them. Where the host makes a pen's pointer events itself
//! (Wayland's tablet-v2) it also queues one [`Sample`] for each of them,
//! so a stroke can give every point the pressure and tilt the pen had
//! there ([`take_samples`]) rather than the frame's last.
//!
//! A pen's driver says that the tip has lifted before the window hands
//! over the button's release, often within one frame. So the pressure the
//! pen had as it lifted stays readable until that frame has run: the
//! stroke's last points are drawn with it, not at full size.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

/// Where [`pressure`] comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PenSource {
    /// A WinTab tablet driver (Windows).
    WinTab,
    /// A pen's pointer events in a browser.
    Browser,
    /// A pen tablet's XInput2 device (X11 on Linux and the BSDs, or
    /// XWayland under a Wayland compositor).
    XInput,
    /// A pen through Wayland's tablet-v2 protocol (Linux and the BSDs).
    WaylandTablet,
}

/// The pressure's bits, or `u32::MAX` while no pen touches.
static PRESSURE: AtomicU32 = AtomicU32::new(u32::MAX);
/// The bits of the pressure a pen had as it lifted, until a frame has run
/// since; `u32::MAX` otherwise.
static LIFT: AtomicU32 = AtomicU32::new(u32::MAX);
/// The last [`PenSource`] seen: 0 none, 1 WinTab, 2 a browser, 3 XInput2,
/// 4 Wayland's tablet-v2.
static SOURCE: AtomicU8 = AtomicU8::new(0);
/// The pen's lean to the right and toward the person, each a bit pattern
/// of -1 to 1, or `u32::MAX` when the source does not say.
static TILT_X: AtomicU32 = AtomicU32::new(u32::MAX);
static TILT_Y: AtomicU32 = AtomicU32::new(u32::MAX);
/// `true` while the pen's eraser end is the one in use.
static ERASER: AtomicBool = AtomicBool::new(false);

/// What a pen last reported: its pressure while its tip touches, and once
/// it has lifted the pressure it had, until a frame has run.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Contact {
    /// The pressure, 0 to 1, while the tip touches.
    touching: Option<f32>,
    /// After the tip lifted: the pressure it had, for the frame that
    /// draws the stroke's end.
    lift: Option<f32>,
}

impl Contact {
    /// What [`pressure`] answers.
    fn read(self) -> Option<f32> {
        self.touching.or(self.lift)
    }

    /// This after the pen of `from` reports `pressure` (`None`: it does
    /// not touch). A tip that lifts leaves its pressure behind; Wayland's
    /// tablet keeps its own (`wayland_pen.rs`), so none is kept for it.
    fn report(self, from: PenSource, pressure: Option<f32>) -> Self {
        let pressure = pressure.map(|value| value.clamp(0.0, 1.0));
        let lift = match (pressure, self.touching) {
            (Some(_), _) => None,
            (None, Some(last)) if from != PenSource::WaylandTablet => Some(last),
            (None, Some(_)) => None,
            (None, None) => self.lift,
        };
        Self {
            touching: pressure,
            lift,
        }
    }

    /// This once a frame has run: a lifted tip's pressure goes.
    fn frame_done(self) -> Self {
        Self { lift: None, ..self }
    }

    /// The real pen's, as the statics have it.
    fn load() -> Self {
        let read = |bits: u32| (bits != u32::MAX).then(|| f32::from_bits(bits));
        Self {
            touching: read(PRESSURE.load(Ordering::Relaxed)),
            lift: read(LIFT.load(Ordering::Relaxed)),
        }
    }

    /// Makes this the real pen's.
    fn store(self) {
        let bits = |value: Option<f32>| value.map_or(u32::MAX, f32::to_bits);
        PRESSURE.store(bits(self.touching), Ordering::Relaxed);
        LIFT.store(bits(self.lift), Ordering::Relaxed);
    }
}

/// The pen's pressure, 0 to 1, while it touches, and the pressure it had
/// as it lifted until the frame after has run (a stroke's last points are
/// drawn in that frame); `None` when no such pen touches.
#[must_use]
pub fn pressure() -> Option<f32> {
    if let Some((_, pen)) = TEST_PEN.get() {
        return pen.read();
    }
    Contact::load().read()
}

thread_local! {
    /// A test's own pen on its own thread (see [`set_for_test`]): where it
    /// comes from and what it last reported, read instead of the real
    /// pen's.
    static TEST_PEN: std::cell::Cell<Option<(PenSource, Contact)>> =
        const { std::cell::Cell::new(None) };
}

/// For tests that drive a studio as a pen of `source` would, without the
/// tablet: the pen reports `pressure` as a driver would, and from now on
/// this thread reads it (and `source`) instead of the real pen's, until
/// [`forget_for_test`]. Other threads, and so other tests, see nothing of
/// it. No frame of the host ends for it, so a lifted pen's pressure stays
/// until it is forgotten.
#[doc(hidden)]
pub fn set_for_test(source: PenSource, pressure: Option<f32>) {
    let pen = TEST_PEN.get().map_or_else(Contact::default, |(_, pen)| pen);
    TEST_PEN.set(Some((source, pen.report(source, pressure))));
}

/// Ends [`set_for_test`]: this thread reads the real pen again.
#[doc(hidden)]
pub fn forget_for_test() {
    TEST_PEN.set(None);
}

/// Where pen pressure last came from, once a pen has been seen.
#[must_use]
pub fn source() -> Option<PenSource> {
    if let Some((source, _)) = TEST_PEN.get() {
        return Some(source);
    }
    match SOURCE.load(Ordering::Relaxed) {
        1 => Some(PenSource::WinTab),
        2 => Some(PenSource::Browser),
        3 => Some(PenSource::XInput),
        4 => Some(PenSource::WaylandTablet),
        _ => None,
    }
}

/// How far the pen leans, `[x, y]`, each from -1 to 1: x to the right, y
/// toward the person, 0 upright and 1 the farthest the source reports (90
/// degrees under Wayland, the axis's end under XInput2).
/// `None` when the pen's source does not report tilt.
#[must_use]
pub fn tilt() -> Option<[f32; 2]> {
    let x = TILT_X.load(Ordering::Relaxed);
    let y = TILT_Y.load(Ordering::Relaxed);
    (x != u32::MAX && y != u32::MAX).then(|| [f32::from_bits(x), f32::from_bits(y)])
}

/// `true` while the pen is turned over to its eraser end (or an eraser
/// tool is in use), where the source tells the ends apart.
#[must_use]
pub fn eraser() -> bool {
    ERASER.load(Ordering::Relaxed)
}

/// Records what the pen does now: its pressure while it touches, else
/// `None`. Only Windows' tablet driver, XInput2 and the browser report
/// pressure this way. Called once for each reading in its order (not only
/// a frame's last), a touch and a lift between two frames still leave the
/// pressure of the touch for the frame that draws it.
#[cfg_attr(
    not(any(
        target_os = "windows",
        target_arch = "wasm32",
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )),
    allow(dead_code)
)]
pub(crate) fn set(from: PenSource, pressure: Option<f32>) {
    SOURCE.store(
        match from {
            PenSource::WinTab => 1,
            PenSource::Browser => 2,
            PenSource::XInput => 3,
            PenSource::WaylandTablet => 4,
        },
        Ordering::Relaxed,
    );
    Contact::load().report(from, pressure).store();
}

/// Records which end of the pen is in use and how far it leans (`None`
/// when the source does not say). Only XInput2 and Wayland's tablet-v2
/// report these so far.
#[cfg_attr(
    not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )),
    allow(dead_code)
)]
pub(crate) fn set_tool(tilt: Option<[f32; 2]>, eraser: bool) {
    let [x, y] = tilt.map_or([u32::MAX; 2], |[x, y]| {
        [x.clamp(-1.0, 1.0).to_bits(), y.clamp(-1.0, 1.0).to_bits()]
    });
    TILT_X.store(x, Ordering::Relaxed);
    TILT_Y.store(y, Ordering::Relaxed);
    ERASER.store(eraser, Ordering::Relaxed);
}

/// Which pointer event a [`Sample`] goes with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleKind {
    /// `egui::Event::PointerMoved`.
    Move,
    /// `egui::Event::PointerButton` going down.
    Press,
    /// `egui::Event::PointerButton` going up.
    Release,
}

/// What the pen read at one of its pointer events in this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    /// The event it goes with.
    pub kind: SampleKind,
    /// That event's position, in egui points.
    pub pos: [f32; 2],
    /// The pressure, 0 to 1, while the tip touches (on the tip's release,
    /// the pressure it had just before); `None` while it hovers.
    pub pressure: Option<f32>,
    /// The lean, as [`tilt`] has it, where the source says.
    pub tilt: Option<[f32; 2]>,
    /// The eraser end is the one in use.
    pub eraser: bool,
}

impl Sample {
    /// `true` when this is the reading for the event of `kind` at `pos`
    /// (egui points).
    #[must_use]
    pub fn is_for(&self, kind: SampleKind, pos: [f32; 2]) -> bool {
        self.kind == kind
            && (self.pos[0] - pos[0]).abs() <= 1e-3
            && (self.pos[1] - pos[1]).abs() <= 1e-3
    }
}

/// This frame's samples, oldest first.
static SAMPLES: Mutex<VecDeque<Sample>> = Mutex::new(VecDeque::new());

/// How many samples wait at most (a frame that never comes keeps the
/// latest).
const SAMPLES_KEPT: usize = 512;

/// The samples queued for this frame's pointer events, in their order;
/// whoever takes them has them (the rest of the frame finds none).
#[must_use]
pub fn take_samples() -> VecDeque<Sample> {
    SAMPLES
        .lock()
        .map(|mut samples| std::mem::take(&mut *samples))
        .unwrap_or_default()
}

/// Queues the reading for a pointer event the host adds to this frame.
#[cfg_attr(
    not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )),
    allow(dead_code)
)]
pub(crate) fn push_sample(mut sample: Sample) {
    sample.pressure = sample.pressure.map(|value| value.clamp(0.0, 1.0));
    sample.tilt = sample
        .tilt
        .map(|lean| lean.map(|value| value.clamp(-1.0, 1.0)));
    if let Ok(mut samples) = SAMPLES.lock() {
        if samples.len() >= SAMPLES_KEPT {
            samples.pop_front();
        }
        samples.push_back(sample);
    }
}

/// A frame has run: the samples nobody took go with it, and so does the
/// pressure of a pen that lifted before it.
pub(crate) fn end_frame() {
    Contact::load().frame_done().store();
    if let Ok(mut samples) = SAMPLES.lock()
        && !samples.is_empty()
    {
        samples.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_queue_in_order_until_taken_or_the_frame_ends() {
        let sample = |kind, x: f32, pressure| Sample {
            kind,
            pos: [x, 2.0],
            pressure,
            tilt: Some([0.5, -3.0]),
            eraser: false,
        };
        end_frame();
        push_sample(sample(SampleKind::Press, 1.0, Some(0.25)));
        push_sample(sample(SampleKind::Move, 1.5, Some(2.0)));
        push_sample(sample(SampleKind::Release, 1.5, Some(0.75)));
        let mut taken = take_samples();
        assert!(take_samples().is_empty(), "taken once");
        assert_eq!(taken.len(), 3);
        let first = taken.pop_front().unwrap();
        assert!(first.is_for(SampleKind::Press, [1.0, 2.0]));
        assert!(!first.is_for(SampleKind::Move, [1.0, 2.0]));
        assert!(!first.is_for(SampleKind::Press, [1.1, 2.0]));
        assert_eq!(first.pressure, Some(0.25));
        assert_eq!(first.tilt, Some([0.5, -1.0]), "clamped");
        assert_eq!(taken[0].pressure, Some(1.0), "clamped");
        assert_eq!(
            taken[1].pressure,
            Some(0.75),
            "a release keeps its pressure"
        );
        push_sample(sample(SampleKind::Move, 0.0, None));
        end_frame();
        assert!(take_samples().is_empty(), "dropped with its frame");
        for x in 0..SAMPLES_KEPT + 10 {
            push_sample(sample(SampleKind::Move, x as f32, None));
        }
        let kept = take_samples();
        assert_eq!(kept.len(), SAMPLES_KEPT);
        assert_eq!(kept[0].pos[0], 10.0, "the latest are kept");
    }

    #[test]
    fn the_pens_pressure_reads_back_until_it_lifts() {
        set(PenSource::WinTab, Some(0.42));
        assert_eq!(pressure(), Some(0.42));
        assert_eq!(source(), Some(PenSource::WinTab));
        set(PenSource::Browser, Some(3.0));
        assert_eq!(pressure(), Some(1.0), "clamped");
        set(PenSource::Browser, None);
        assert_eq!(source(), Some(PenSource::Browser));
        set(PenSource::XInput, Some(0.5));
        assert_eq!(source(), Some(PenSource::XInput));
        set(PenSource::XInput, None);
    }

    #[test]
    fn a_lifted_pens_pressure_stays_for_one_frame() {
        let pen = Contact::default();
        assert_eq!(pen.read(), None);
        for from in [PenSource::WinTab, PenSource::XInput, PenSource::Browser] {
            let down = pen.report(from, Some(0.4));
            assert_eq!(down.read(), Some(0.4));
            // Lifted: the stroke's last frame still reads what it had.
            let up = down.report(from, None);
            assert_eq!(up.read(), Some(0.4), "{from:?}");
            // Hovering on in that frame changes nothing.
            assert_eq!(up.report(from, None).read(), Some(0.4), "{from:?}");
            // The frame has run: gone.
            assert_eq!(up.frame_done().read(), None, "{from:?}");
            // A frame's end while it touches takes nothing away.
            assert_eq!(down.frame_done().read(), Some(0.4));
            // Touching again before that frame: the new pressure.
            assert_eq!(up.report(from, Some(0.7)).read(), Some(0.7));
            assert_eq!(up.report(from, Some(0.7)).frame_done().lift, None);
            // A touch and a lift between two frames: the touch's pressure.
            let tap = pen.report(from, Some(0.2)).report(from, None);
            assert_eq!(tap.read(), Some(0.2), "{from:?}");
        }
        // Wayland's tablet holds its own lift, and says when it is over.
        let wayland = PenSource::WaylandTablet;
        let up = pen.report(wayland, Some(0.4)).report(wayland, None);
        assert_eq!(up.read(), None);
    }

    #[test]
    fn a_tests_own_pen_is_read_on_its_thread_only() {
        set_for_test(PenSource::WinTab, Some(0.3));
        assert_eq!(pressure(), Some(0.3));
        assert_eq!(source(), Some(PenSource::WinTab));
        let elsewhere = std::thread::spawn(|| TEST_PEN.get().is_some())
            .join()
            .unwrap();
        assert!(!elsewhere, "another thread reads the real pen");
        set_for_test(PenSource::WinTab, None);
        assert_eq!(pressure(), Some(0.3), "lifted, kept for the last frame");
        forget_for_test();
        assert!(TEST_PEN.get().is_none());
    }

    #[test]
    fn tilt_and_the_eraser_read_back() {
        set_tool(Some([0.25, -2.0]), true);
        assert_eq!(tilt(), Some([0.25, -1.0]), "clamped");
        assert!(eraser());
        set_tool(None, false);
        assert_eq!(tilt(), None);
        assert!(!eraser());
    }
}
