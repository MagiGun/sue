//! The pen's sample stream: what every pointing device that can draw
//! reported, one [`PenSample`] for each packet of the device, in order,
//! each with where the pen was, when (on the device's own clock where it
//! has one, else Sue's monotonic one), how hard it pressed and how it
//! leaned.
//!
//! Every source feeds the same stream:
//!
//! - a WinTab tablet on Windows (tablet drivers with Windows Ink turned
//!   off, common among artists), a pen tablet's XInput2 device under X11
//!   and XWayland, and a pen in a browser, where winit treats pens as
//!   mice. Their packets say everything but where the pen is in Sue's
//!   window (the pen moves the system's pointer instead), so the host
//!   lays each frame's packets along the path the pointer took
//!   (`route.rs`);
//! - a pen under Wayland's tablet-v2, whose packets Sue turns into
//!   pointer events itself;
//! - the window's touch events, which carry a pen's force with Windows
//!   Ink, on macOS and on phones, and a finger's where the screen feels
//!   it;
//! - a mouse, which says of itself that it presses fully and always the
//!   same: a constant, not a fallback.
//!
//! [`take_pen_samples`] hands a frame's samples to whoever draws with
//! them. A release ([`PenKind::Up`]) says only where the pen left: it
//! carries no pressure, so nothing can draw a last point with it.
//!
//! The older calls ([`pressure`], [`tilt`], [`eraser`], [`source`] and
//! [`take_samples`]) are readers of the same stream, kept until the last
//! code that draws from "the pen's latest pressure" is gone. A pen's
//! driver says that the tip has lifted before the window hands over the
//! button's release, often within one frame. So for those readers the
//! pressure the pen had as it lifted stays readable until that frame has
//! run: the stroke's last points are drawn with it, not at full size.

mod clock;
mod route;

use std::collections::VecDeque;
use std::sync::Mutex;

#[cfg_attr(
    not(any(
        target_os = "windows",
        target_arch = "wasm32",
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly",
        test
    )),
    allow(unused_imports)
)]
pub(crate) use clock::DeviceClock;
#[cfg_attr(
    not(any(
        target_os = "windows",
        target_arch = "wasm32",
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly",
        test
    )),
    allow(unused_imports)
)]
pub(crate) use route::{Reading, Router, queue_reading};

/// Where a [`PenSample`] comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    /// The window's touch events: a pen with Windows Ink, on macOS and on
    /// phones and tablets, or a finger.
    Touch,
    /// A mouse (or anything the system hands over as one). It presses
    /// fully and always the same.
    Mouse,
}

/// How many [`PenSource`]s there are.
const SOURCES: usize = 6;

impl PenSource {
    /// Its place in the stream's per-source state.
    fn index(self) -> usize {
        match self {
            Self::WinTab => 0,
            Self::Browser => 1,
            Self::XInput => 2,
            Self::WaylandTablet => 3,
            Self::Touch => 4,
            Self::Mouse => 5,
        }
    }

    /// `true` for a pen whose pressure does not come with the window's
    /// own events: the pens [`pressure`] and [`source`] speak of.
    #[must_use]
    pub fn is_pen(self) -> bool {
        !matches!(self, Self::Touch | Self::Mouse)
    }

    /// `true` for a pen whose packets do not say where it is in the
    /// window: the host pairs them with the pointer's path.
    fn follows_the_pointer(self) -> bool {
        matches!(self, Self::WinTab | Self::Browser | Self::XInput)
    }
}

/// What a [`PenSample`] says happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PenKind {
    /// The tip (or the mouse's main button, or a finger) touched here.
    Down,
    /// It moved here, or stayed and pressed or leaned differently:
    /// touching when the last sample of its source before it was a
    /// [`PenKind::Down`] or a touching move, hovering otherwise.
    Move,
    /// It lifted here. A release is where a line ends, never one more
    /// point of it: it carries a position and a time and nothing else.
    Up,
}

/// What a pointing device reported in one of its packets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenSample {
    /// Where it was, in egui points in Sue's main window.
    pub pos: [f32; 2],
    /// When, in seconds on the stream's clock: the device's own time
    /// where it stamps its packets (the steps between two samples are
    /// then the device's), else the monotonic time the event reached Sue.
    /// It never goes backwards from one sample of the stream to the next.
    pub time: f64,
    /// How hard it pressed, 0 to 1, while touching; `None` while it
    /// hovers, on [`PenKind::Up`], and for a touch whose screen does not
    /// feel force. A mouse says 1.
    pub pressure: Option<f32>,
    /// How far it leans, `[x, y]`, each from -1 to 1: x to the right, y
    /// toward the person, 0 upright and 1 flat (or the farthest the
    /// source reports). `None` where the source does not say.
    pub tilt: Option<[f32; 2]>,
    /// How far it is turned about its own axis, in degrees clockwise,
    /// 0 to 360. `None` where the source does not say.
    pub twist: Option<f32>,
    /// What happened.
    pub kind: PenKind,
    /// Which device.
    pub source: PenSource,
    /// The pen's eraser end (or an eraser tool) is the one in use.
    pub eraser: bool,
}

impl PenSample {
    /// A sample of `source` at `pos` and `time` that says nothing else: a
    /// hovering move.
    #[must_use]
    pub fn at(source: PenSource, pos: [f32; 2], time: f64) -> Self {
        Self {
            pos,
            time,
            pressure: None,
            tilt: None,
            twist: None,
            kind: PenKind::Move,
            source,
            eraser: false,
        }
    }
}

/// What a pen last reported, for [`pressure`]: its pressure while its tip
/// touches, and once it has lifted the pressure it had, until a frame has
/// run.
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

    /// This after the pen reports `pressure` (`None`: it does not touch).
    /// A tip that lifts leaves its pressure behind.
    fn report(self, pressure: Option<f32>) -> Self {
        let pressure = pressure.map(|value| value.clamp(0.0, 1.0));
        let lift = match (pressure, self.touching) {
            (Some(_), _) => None,
            (None, Some(last)) => Some(last),
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
}

/// How many samples wait at most (a frame that never comes keeps the
/// latest).
const SAMPLES_KEPT: usize = 512;

/// How many [`PenSample`]s wait at most: several seconds of the fastest
/// tablets.
const STREAM_KEPT: usize = 4096;

/// The stream: the samples nobody has taken yet, and what the samples so
/// far add up to for the older readers.
#[derive(Debug)]
pub(crate) struct Stream {
    /// The samples since the last frame, oldest first.
    queue: VecDeque<PenSample>,
    /// The older readers' per-event readings (see [`Sample`]).
    paired: VecDeque<Sample>,
    /// The latest sample's time.
    time: f64,
    /// Whether each source touches.
    touching: [bool; SOURCES],
    /// Each source's latest pressure while it touches.
    held: [Option<f32>; SOURCES],
    /// Where each source's latest sample was.
    last_pos: [Option<[f32; 2]>; SOURCES],
    /// The latest pen's contact, for [`pressure`].
    contact: Contact,
    /// The pen ([`PenSource::is_pen`]) that spoke last.
    source: Option<PenSource>,
    /// When it did.
    pen_time: Option<f64>,
    /// Its lean and its end in use, for [`tilt`] and [`eraser`].
    tilt: Option<[f32; 2]>,
    eraser: bool,
    /// The latest touches the system named a pen or a finger, by touch
    /// id, newest last ([`touch_is_pen`]).
    touch_kinds: VecDeque<(u64, bool)>,
}

/// How many touches [`touch_is_pen`] remembers: more fingers than a hand
/// has, and the pen.
const TOUCHES_KEPT: usize = 16;

impl Stream {
    pub(crate) const fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            paired: VecDeque::new(),
            time: 0.0,
            touching: [false; SOURCES],
            held: [None; SOURCES],
            last_pos: [None; SOURCES],
            contact: Contact {
                touching: None,
                lift: None,
            },
            source: None,
            pen_time: None,
            tilt: None,
            eraser: false,
            touch_kinds: VecDeque::new(),
        }
    }

    /// Notes that the system says the touch `id` is a pen (`true`) or a
    /// finger (`false`).
    pub(crate) fn mark_touch(&mut self, id: u64, pen: bool) {
        self.touch_kinds.retain(|(known, _)| *known != id);
        if self.touch_kinds.len() >= TOUCHES_KEPT {
            self.touch_kinds.pop_front();
        }
        self.touch_kinds.push_back((id, pen));
    }

    /// What [`touch_is_pen`] answers.
    pub(crate) fn touch_kind(&self, id: u64) -> Option<bool> {
        self.touch_kinds
            .iter()
            .rev()
            .find(|(known, _)| *known == id)
            .map(|(_, pen)| *pen)
    }

    /// Whether `source` touches, as its samples so far say.
    pub(crate) fn touches(&self, source: PenSource) -> bool {
        self.touching[source.index()]
    }

    /// The pressure `source` last had while it touches.
    pub(crate) fn held(&self, source: PenSource) -> Option<f32> {
        self.held[source.index()]
    }

    /// The pen that spoke last, and when (the stream's clock).
    pub(crate) fn pen(&self) -> Option<(PenSource, f64)> {
        self.source.zip(self.pen_time)
    }

    /// What [`pressure`] answers.
    pub(crate) fn pressure(&self) -> Option<f32> {
        self.contact.read()
    }

    /// The latest pen's lean and whether its eraser end is in use.
    pub(crate) fn tool(&self) -> (Option<[f32; 2]>, bool) {
        (self.tilt, self.eraser)
    }

    /// Queues `sample`, made sound first, and returns it as queued (`None`
    /// when it says nothing usable: a position that is no number).
    ///
    /// What "sound" means, whatever a device or a driver sends: a source
    /// goes down once before it moves touching and up once after; an
    /// [`PenKind::Up`] carries only its position; nothing but a touching
    /// sample has a pressure; pressure, lean and twist stay in range; and
    /// time never goes backwards.
    pub(crate) fn push(&mut self, mut sample: PenSample) -> Option<PenSample> {
        if !(sample.pos[0].is_finite() && sample.pos[1].is_finite()) {
            return None;
        }
        let at = sample.source.index();
        let was = self.touching[at];
        sample.kind = match (sample.kind, was) {
            (PenKind::Down, false) => PenKind::Down,
            (PenKind::Up, true) => PenKind::Up,
            // Down twice is a move; up while hovering, a hovering move.
            _ => PenKind::Move,
        };
        let touching = match sample.kind {
            PenKind::Down => true,
            PenKind::Up => false,
            PenKind::Move => was,
        };
        let number = |value: f32| value.is_finite();
        sample.pressure = sample
            .pressure
            .filter(|value| touching && number(*value))
            .map(|value| value.clamp(0.0, 1.0));
        sample.tilt = sample
            .tilt
            .filter(|lean| lean.iter().all(|value| number(*value)))
            .map(|lean| lean.map(|value| value.clamp(-1.0, 1.0)));
        sample.twist = sample
            .twist
            .filter(|value| number(*value))
            .map(|value| value.rem_euclid(360.0));
        if sample.kind == PenKind::Up {
            sample.tilt = None;
            sample.twist = None;
        }
        if !sample.time.is_finite() || sample.time < self.time {
            sample.time = self.time;
        }
        self.time = sample.time;
        // What the pen pressed with just before, for the older readers.
        let held = self.held[at];
        self.touching[at] = touching;
        self.held[at] = if touching {
            sample.pressure.or(held)
        } else {
            None
        };
        self.last_pos[at] = Some(sample.pos);
        if sample.source.is_pen() {
            self.note(
                sample.source,
                sample.time,
                self.held[at],
                (sample.kind != PenKind::Up).then_some((sample.tilt, sample.eraser)),
            );
        }
        if self.queue.len() >= STREAM_KEPT {
            self.queue.pop_front();
        }
        self.queue.push_back(sample);
        Some(sample)
    }

    /// Queues `sample` for a source that says in each packet whether it
    /// touches rather than when it goes down and up: the kind follows
    /// from `touching` and what the source did before.
    pub(crate) fn push_state(
        &mut self,
        mut sample: PenSample,
        touching: bool,
    ) -> Option<PenSample> {
        sample.kind = match (touching, self.touches(sample.source)) {
            (true, false) => PenKind::Down,
            (false, true) => PenKind::Up,
            _ => PenKind::Move,
        };
        self.push(sample)
    }

    /// Takes a pen's packet that cannot be placed in the window (the
    /// pointer is not over it): the older readers still learn its
    /// pressure, lean and end, and a tip that lifts out there ends its
    /// contact where the stream last saw it.
    pub(crate) fn push_unplaced(
        &mut self,
        source: PenSource,
        time: f64,
        pressure: Option<f32>,
        tool: (Option<[f32; 2]>, bool),
    ) {
        let at = source.index();
        if pressure.is_none()
            && self.touching[at]
            && let Some(pos) = self.last_pos[at]
        {
            let mut up = PenSample::at(source, pos, time);
            up.kind = PenKind::Up;
            up.eraser = tool.1;
            self.push(up);
        }
        if source.is_pen() {
            let time = if time.is_finite() {
                time.max(self.time)
            } else {
                self.time
            };
            self.note(source, time, pressure, Some(tool));
        }
    }

    /// Keeps what a pen just said for the older readers.
    fn note(
        &mut self,
        source: PenSource,
        time: f64,
        pressure: Option<f32>,
        tool: Option<(Option<[f32; 2]>, bool)>,
    ) {
        self.source = Some(source);
        self.pen_time = Some(time);
        self.contact = self.contact.report(pressure);
        if let Some((tilt, eraser)) = tool {
            self.tilt = tilt.map(|lean| lean.map(|value| value.clamp(-1.0, 1.0)));
            self.eraser = eraser;
        }
    }

    /// Queues the older readers' reading for a pointer event of this
    /// frame.
    pub(crate) fn push_paired(&mut self, mut sample: Sample) {
        sample.pressure = sample.pressure.map(|value| value.clamp(0.0, 1.0));
        sample.tilt = sample
            .tilt
            .map(|lean| lean.map(|value| value.clamp(-1.0, 1.0)));
        if self.paired.len() >= SAMPLES_KEPT {
            self.paired.pop_front();
        }
        self.paired.push_back(sample);
    }

    /// A frame has run: the samples nobody took go with it, and so does
    /// the pressure of a pen that lifted before it.
    fn end_frame(&mut self) {
        self.contact = self.contact.frame_done();
        self.queue.clear();
        self.paired.clear();
    }

    /// Takes the samples waiting, oldest first.
    pub(crate) fn take(&mut self) -> Vec<PenSample> {
        self.queue.drain(..).collect()
    }
}

/// The real devices' stream.
static STREAM: Mutex<Stream> = Mutex::new(Stream::new());

thread_local! {
    /// A test's own stream on its own thread (see [`set_for_test`] and
    /// [`push_for_test`]), read instead of the real one.
    static TEST_STREAM: std::cell::RefCell<Option<Stream>> =
        const { std::cell::RefCell::new(None) };
}

/// Runs `with` on the real stream.
pub(crate) fn real<R>(with: impl FnOnce(&mut Stream) -> R) -> R {
    let mut stream = STREAM
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    with(&mut stream)
}

/// Runs `with` on the stream this thread reads: a test's own when it has
/// one, else the real one.
fn read<R>(with: impl FnOnce(&mut Stream) -> R) -> R {
    TEST_STREAM.with_borrow_mut(|own| match own.as_mut() {
        Some(stream) => with(stream),
        None => real(with),
    })
}

/// The samples since the last frame, oldest first, from every device;
/// whoever takes them has them (the rest of the frame finds none). What
/// nobody takes goes when the frame has run.
#[must_use]
pub fn take_pen_samples() -> Vec<PenSample> {
    read(Stream::take)
}

/// The pen's pressure, 0 to 1, while it touches, and the pressure it had
/// as it lifted until the frame after has run (a stroke's last points are
/// drawn in that frame); `None` when no such pen touches. Only pens whose
/// pressure does not come with the window's touch events count
/// ([`PenSource::is_pen`]).
///
/// A reader of the stream's latest state, kept for code that does not
/// take the samples themselves yet.
#[must_use]
pub fn pressure() -> Option<f32> {
    read(|stream| stream.pressure())
}

/// For tests that drive a studio as a pen of `source` would, without the
/// tablet: the pen reports `pressure` as a driver would (`None`: it does
/// not touch), and from now on this thread reads its own stream (and
/// `source`) instead of the real devices', until [`forget_for_test`].
/// Other threads, and so other tests, see nothing of it. No frame of the
/// host ends for it, so a lifted pen's pressure stays until it is
/// forgotten.
#[doc(hidden)]
pub fn set_for_test(source: PenSource, pressure: Option<f32>) {
    TEST_STREAM.with_borrow_mut(|stream| {
        let stream = stream.get_or_insert_with(Stream::new);
        let mut sample = PenSample::at(source, [0.0, 0.0], stream.time);
        sample.pressure = pressure;
        stream.push_state(sample, pressure.is_some());
    });
}

/// For tests that script a whole stream: queues `sample` on this thread's
/// own stream (made on first use), which this thread then reads instead
/// of the real devices' until [`forget_for_test`]. Returns the sample as
/// queued (see [`PenSample`] for what the stream keeps sound).
#[doc(hidden)]
pub fn push_for_test(sample: PenSample) -> Option<PenSample> {
    TEST_STREAM.with_borrow_mut(|stream| stream.get_or_insert_with(Stream::new).push(sample))
}

/// Ends [`set_for_test`] and [`push_for_test`]: this thread reads the
/// real devices again.
#[doc(hidden)]
pub fn forget_for_test() {
    TEST_STREAM.set(None);
}

/// Whether the window's touch `id` (egui's `TouchId`) is a pen (`true`)
/// or a finger (`false`), as the system said when it went down; `None`
/// where the system does not tell them apart, or for a touch it never
/// named. On Windows a pen is a touch with Windows Ink pen packets.
#[must_use]
pub fn touch_is_pen(id: u64) -> Option<bool> {
    read(|stream| stream.touch_kind(id))
}

/// For tests that drive a studio with fingers and a pen: this thread's
/// own stream (made on first use) says the touch `id` is a pen or a
/// finger, as the system would, until [`forget_for_test`].
#[doc(hidden)]
pub fn mark_touch_for_test(id: u64, pen: bool) {
    TEST_STREAM
        .with_borrow_mut(|stream| stream.get_or_insert_with(Stream::new).mark_touch(id, pen));
}

/// Where pen pressure last came from, once a pen has been seen: one of
/// the pens [`PenSource::is_pen`] names, never a mouse or a touch.
#[must_use]
pub fn source() -> Option<PenSource> {
    read(|stream| stream.source)
}

/// How far the pen leans, `[x, y]`, each from -1 to 1: x to the right, y
/// toward the person, 0 upright and 1 the farthest the source reports (90
/// degrees under Wayland and WinTab, the axis's end under XInput2).
/// `None` when the pen's source does not report tilt.
#[must_use]
pub fn tilt() -> Option<[f32; 2]> {
    read(|stream| stream.tilt)
}

/// `true` while the pen is turned over to its eraser end (or an eraser
/// tool is in use), where the source tells the ends apart.
#[must_use]
pub fn eraser() -> bool {
    read(|stream| stream.eraser)
}

/// Tells the stream of a real pen's packet that has no place in the main
/// window (see [`Stream::push_unplaced`]).
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
pub(crate) fn push_unplaced(
    source: PenSource,
    time: f64,
    pressure: Option<f32>,
    tool: (Option<[f32; 2]>, bool),
) {
    real(|stream| stream.push_unplaced(source, time, pressure, tool));
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

/// What the pen read at one of its pointer events in this frame: the
/// older, per-event view of the stream, for code that draws from egui's
/// pointer events and asks what the pen read at each.
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

/// The readings queued for this frame's pointer events, in their order;
/// whoever takes them has them (the rest of the frame finds none).
#[must_use]
pub fn take_samples() -> VecDeque<Sample> {
    read(|stream| std::mem::take(&mut stream.paired))
}

/// Queues a real pen's sample together with the older readers' reading
/// for the pointer event of `kind` the host makes from it, at the same
/// place: the pen's pressure there, and on the tip's release the
/// pressure it had just before.
#[cfg_attr(
    not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly",
        test
    )),
    allow(dead_code)
)]
pub(crate) fn push_with_event(sample: PenSample, kind: SampleKind) {
    real(|stream| stream.push_with_event(sample, kind));
}

impl Stream {
    /// See [`push_with_event`].
    pub(crate) fn push_with_event(&mut self, sample: PenSample, kind: SampleKind) {
        let before = self.held(sample.source);
        let Some(queued) = self.push(sample) else {
            return;
        };
        self.push_paired(Sample {
            kind,
            pos: queued.pos,
            pressure: match queued.kind {
                PenKind::Up => before,
                _ => queued.pressure,
            },
            tilt: sample.tilt,
            eraser: queued.eraser,
        });
    }
}

/// A frame has run: the samples nobody took go with it, and so does the
/// pressure of a pen that lifted before it. Always the real devices'
/// frame: no frame of the host ends for a test's own stream.
pub(crate) fn end_frame() {
    real(Stream::end_frame);
}

/// The older readers' readings queued on `stream`, for the tests of the
/// sources.
#[cfg(test)]
pub(crate) fn tests_paired(stream: &mut Stream) -> Vec<Sample> {
    std::mem::take(&mut stream.paired).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Held by each test that reads or ends a frame of the real stream,
    /// which all tests share: one at a time.
    static REAL_PEN: Mutex<()> = Mutex::new(());

    fn alone() -> std::sync::MutexGuard<'static, ()> {
        REAL_PEN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn pen(kind: PenKind, x: f32, time: f64, pressure: Option<f32>) -> PenSample {
        PenSample {
            kind,
            pressure,
            ..PenSample::at(PenSource::WaylandTablet, [x, 2.0], time)
        }
    }

    #[test]
    fn samples_queue_in_order_until_taken_or_the_frame_ends() {
        let _alone = alone();
        end_frame();
        let lean = |sample: PenSample| PenSample {
            tilt: Some([0.5, -3.0]),
            ..sample
        };
        push_with_event(
            lean(pen(PenKind::Down, 1.0, 0.0, Some(0.25))),
            SampleKind::Press,
        );
        push_with_event(
            lean(pen(PenKind::Move, 1.5, 0.0, Some(2.0))),
            SampleKind::Move,
        );
        push_with_event(
            lean(pen(PenKind::Move, 1.5, 0.0, Some(0.75))),
            SampleKind::Move,
        );
        push_with_event(lean(pen(PenKind::Up, 1.5, 0.0, None)), SampleKind::Release);
        let mut taken = take_samples();
        assert!(take_samples().is_empty(), "taken once");
        assert_eq!(taken.len(), 4);
        let first = taken.pop_front().unwrap();
        assert!(first.is_for(SampleKind::Press, [1.0, 2.0]));
        assert!(!first.is_for(SampleKind::Move, [1.0, 2.0]));
        assert!(!first.is_for(SampleKind::Press, [1.1, 2.0]));
        assert_eq!(first.pressure, Some(0.25));
        assert_eq!(first.tilt, Some([0.5, -1.0]), "clamped");
        assert_eq!(taken[0].pressure, Some(1.0), "clamped");
        assert_eq!(
            taken[2].pressure,
            Some(0.75),
            "a release keeps the pressure the tip had"
        );
        // The stream itself has the same four, the release bare.
        let stream = take_pen_samples();
        assert!(take_pen_samples().is_empty(), "taken once");
        assert_eq!(
            stream.iter().map(|sample| sample.kind).collect::<Vec<_>>(),
            [PenKind::Down, PenKind::Move, PenKind::Move, PenKind::Up]
        );
        assert_eq!(stream[3].pressure, None);
        assert_eq!(stream[3].tilt, None);
        assert_eq!(stream[3].pos, [1.5, 2.0]);
        push_with_event(pen(PenKind::Move, 0.0, 0.0, None), SampleKind::Move);
        end_frame();
        assert!(take_samples().is_empty(), "dropped with its frame");
        assert!(take_pen_samples().is_empty(), "dropped with its frame");
        for x in 0..SAMPLES_KEPT + 10 {
            push_with_event(pen(PenKind::Move, x as f32, 0.0, None), SampleKind::Move);
        }
        let kept = take_samples();
        assert_eq!(kept.len(), SAMPLES_KEPT);
        assert_eq!(kept[0].pos[0], 10.0, "the latest are kept");
        end_frame();
    }

    #[test]
    fn the_pens_pressure_reads_back_until_it_lifts() {
        let _alone = alone();
        end_frame();
        let report = |source, pressure: Option<f32>| {
            real(|stream| {
                let mut sample = PenSample::at(source, [0.0, 0.0], 0.0);
                sample.pressure = pressure;
                stream.push_state(sample, pressure.is_some());
            });
        };
        report(PenSource::WinTab, Some(0.42));
        assert_eq!(pressure(), Some(0.42));
        assert_eq!(source(), Some(PenSource::WinTab));
        report(PenSource::WinTab, None);
        report(PenSource::Browser, Some(3.0));
        assert_eq!(pressure(), Some(1.0), "clamped");
        report(PenSource::Browser, None);
        assert_eq!(pressure(), Some(1.0), "lifted: kept for the frame");
        assert_eq!(source(), Some(PenSource::Browser));
        end_frame();
        assert_eq!(pressure(), None, "the frame has run");
        end_frame();
        assert_eq!(pressure(), None);
        report(PenSource::XInput, Some(0.5));
        assert_eq!(source(), Some(PenSource::XInput));
        end_frame();
        assert_eq!(
            pressure(),
            Some(0.5),
            "a frame takes nothing while it touches"
        );
        report(PenSource::XInput, None);
        assert_eq!(pressure(), Some(0.5));
        end_frame();
        assert_eq!(pressure(), None);
        // Wayland's tablet is a source like the others now.
        report(PenSource::WaylandTablet, Some(0.6));
        report(PenSource::WaylandTablet, None);
        assert_eq!(pressure(), Some(0.6));
        end_frame();
        assert_eq!(pressure(), None);
        // A mouse and a touch are in the stream, and none of this.
        report(PenSource::Mouse, Some(1.0));
        report(PenSource::Touch, Some(0.3));
        assert_eq!(pressure(), None);
        assert_eq!(source(), Some(PenSource::WaylandTablet));
        report(PenSource::Mouse, None);
        report(PenSource::Touch, None);
        end_frame();
    }

    #[test]
    fn a_lifted_pens_pressure_stays_for_one_frame() {
        let pen = Contact::default();
        assert_eq!(pen.read(), None);
        let down = pen.report(Some(0.4));
        assert_eq!(down.read(), Some(0.4));
        // Lifted: the stroke's last frame still reads what it had.
        let up = down.report(None);
        assert_eq!(up.read(), Some(0.4));
        // Hovering on in that frame changes nothing.
        assert_eq!(up.report(None).read(), Some(0.4));
        // The frame has run: gone.
        assert_eq!(up.frame_done().read(), None);
        // A frame's end while it touches takes nothing away.
        assert_eq!(down.frame_done().read(), Some(0.4));
        // Touching again before that frame: the new pressure.
        assert_eq!(up.report(Some(0.7)).read(), Some(0.7));
        assert_eq!(up.report(Some(0.7)).frame_done().lift, None);
        // A touch and a lift between two frames: the touch's pressure.
        let tap = pen.report(Some(0.2)).report(None);
        assert_eq!(tap.read(), Some(0.2));
    }

    #[test]
    fn a_tests_own_pen_is_read_on_its_thread_only() {
        set_for_test(PenSource::WinTab, Some(0.3));
        assert_eq!(pressure(), Some(0.3));
        assert_eq!(source(), Some(PenSource::WinTab));
        let elsewhere = std::thread::spawn(|| TEST_STREAM.with_borrow(Option::is_some))
            .join()
            .unwrap();
        assert!(!elsewhere, "another thread reads the real pen");
        set_for_test(PenSource::WinTab, None);
        assert_eq!(pressure(), Some(0.3), "lifted, kept for the last frame");
        // Its samples are there to take, as a real pen's would be.
        let kinds: Vec<_> = take_pen_samples()
            .iter()
            .map(|sample| sample.kind)
            .collect();
        assert_eq!(kinds, [PenKind::Down, PenKind::Up]);
        forget_for_test();
        assert!(TEST_STREAM.with_borrow(Option::is_none));
    }

    #[test]
    fn a_scripted_stream_comes_back_as_it_was_pushed() {
        let script = [
            pen(PenKind::Move, 0.0, 1.000, None),
            pen(PenKind::Down, 1.0, 1.005, Some(0.2)),
            pen(PenKind::Move, 2.0, 1.010, Some(0.6)),
            pen(PenKind::Move, 3.0, 1.015, Some(0.4)),
            pen(PenKind::Up, 3.5, 1.020, None),
            pen(PenKind::Move, 5.0, 1.025, None),
        ];
        for sample in script {
            assert_eq!(push_for_test(sample), Some(sample), "sound as it is");
        }
        assert_eq!(take_pen_samples(), script);
        assert_eq!(pressure(), Some(0.4), "what the tip had as it lifted");
        forget_for_test();
    }

    #[test]
    fn tilt_and_the_eraser_read_back() {
        let mut stream = Stream::new();
        let mut sample = PenSample::at(PenSource::XInput, [1.0, 1.0], 0.0);
        sample.tilt = Some([0.25, -2.0]);
        sample.twist = Some(-90.0);
        sample.eraser = true;
        let queued = stream.push(sample).unwrap();
        assert_eq!(queued.tilt, Some([0.25, -1.0]), "clamped");
        assert_eq!(queued.twist, Some(270.0), "within one turn");
        assert_eq!(stream.tool(), (Some([0.25, -1.0]), true));
        // A mouse says nothing about the pen's lean or end.
        stream.push(PenSample::at(PenSource::Mouse, [1.0, 1.0], 0.0));
        assert_eq!(stream.tool(), (Some([0.25, -1.0]), true));
        stream.push(PenSample::at(PenSource::XInput, [1.0, 1.0], 0.0));
        assert_eq!(stream.tool(), (None, false));
    }

    #[test]
    fn the_stream_keeps_every_source_sound() {
        let mut stream = Stream::new();
        let sample = |kind, time, pressure| PenSample {
            kind,
            pressure,
            tilt: Some([0.1, 0.1]),
            twist: Some(10.0),
            ..PenSample::at(PenSource::Touch, [4.0, 4.0], time)
        };
        // Hovering never has a pressure, whatever the device says.
        let hover = stream.push(sample(PenKind::Move, 1.0, Some(0.9))).unwrap();
        assert_eq!((hover.kind, hover.pressure), (PenKind::Move, None));
        // Up without a down is a hovering move.
        let stray = stream.push(sample(PenKind::Up, 1.1, None)).unwrap();
        assert_eq!(stray.kind, PenKind::Move);
        let down = stream.push(sample(PenKind::Down, 1.2, Some(0.5))).unwrap();
        assert_eq!((down.kind, down.pressure), (PenKind::Down, Some(0.5)));
        // Down twice is a touching move.
        let again = stream.push(sample(PenKind::Down, 1.3, Some(0.6))).unwrap();
        assert_eq!((again.kind, again.pressure), (PenKind::Move, Some(0.6)));
        // Time that runs backwards, or is no number, stands still.
        let late = stream.push(sample(PenKind::Move, 0.5, Some(0.6))).unwrap();
        assert_eq!(late.time, 1.3);
        let broken = stream
            .push(sample(PenKind::Move, f64::NAN, Some(f32::NAN)))
            .unwrap();
        assert_eq!((broken.time, broken.pressure), (1.3, None));
        assert_eq!(stream.held(PenSource::Touch), Some(0.6), "the last stays");
        // An up is a place and a time, nothing more.
        let up = stream.push(sample(PenKind::Up, 1.4, Some(1.0))).unwrap();
        assert_eq!(
            up,
            PenSample {
                kind: PenKind::Up,
                ..PenSample::at(PenSource::Touch, [4.0, 4.0], 1.4)
            }
        );
        // A position that is no number is no sample.
        let mut nowhere = sample(PenKind::Move, 1.5, None);
        nowhere.pos = [f32::NAN, 0.0];
        assert_eq!(stream.push(nowhere), None);
        // Two sources keep their own contact.
        let mouse = PenSample {
            kind: PenKind::Down,
            pressure: Some(1.0),
            ..PenSample::at(PenSource::Mouse, [0.0, 0.0], 2.0)
        };
        assert_eq!(stream.push(mouse).unwrap().kind, PenKind::Down);
        assert!(stream.touches(PenSource::Mouse));
        assert!(!stream.touches(PenSource::Touch));
        assert_eq!(stream.take().len(), 8);
    }

    #[test]
    fn a_pen_outside_the_window_still_tells_the_older_readers() {
        let mut stream = Stream::new();
        let mut down = PenSample::at(PenSource::WinTab, [7.0, 8.0], 1.0);
        down.pressure = Some(0.5);
        stream.push_state(down, true);
        // It leaves the window pressed, then lifts out there.
        stream.push_unplaced(PenSource::WinTab, 1.1, Some(0.7), (None, false));
        assert_eq!(stream.pressure(), Some(0.7));
        stream.push_unplaced(PenSource::WinTab, 1.2, None, (None, false));
        assert_eq!(stream.pressure(), Some(0.7), "kept for the frame");
        let taken = stream.take();
        assert_eq!(taken.len(), 2);
        assert_eq!(taken[1].kind, PenKind::Up);
        assert_eq!(taken[1].pos, [7.0, 8.0], "where the stream last saw it");
        assert_eq!(stream.pen(), Some((PenSource::WinTab, 1.2)));
    }

    #[test]
    fn a_touch_is_a_pen_or_a_finger_as_the_system_said() {
        let mut stream = Stream::new();
        assert_eq!(stream.touch_kind(1), None, "never named");
        stream.mark_touch(1, true);
        stream.mark_touch(2, false);
        assert_eq!(stream.touch_kind(1), Some(true));
        assert_eq!(stream.touch_kind(2), Some(false));
        // An id the system uses again is what it said last.
        stream.mark_touch(1, false);
        assert_eq!(stream.touch_kind(1), Some(false));
        // Only the latest touches are kept.
        for id in 10..10 + TOUCHES_KEPT as u64 {
            stream.mark_touch(id, true);
        }
        assert_eq!(stream.touch_kind(2), None);
        assert_eq!(stream.touch_kind(10), Some(true));
        // A test's own stream answers on its thread only.
        mark_touch_for_test(77, false);
        assert_eq!(touch_is_pen(77), Some(false));
        let other = std::thread::spawn(|| touch_is_pen(77)).join().unwrap();
        assert_eq!(other, None);
        forget_for_test();
        assert_eq!(touch_is_pen(77), None);
    }
}
