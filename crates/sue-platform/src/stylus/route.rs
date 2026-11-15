//! Who has the pointer: turns the main window's mouse and touch events,
//! and the packets of pens that do not say where they are, into the
//! stream's samples.
//!
//! A WinTab tablet, an XInput2 pen and a pen in a browser each report
//! pressure, lean and time in packets of their own ([`Reading`]) while
//! the pen moves the system's pointer, whose moves reach the window as
//! mouse events. The two arrive side by side, several packets to a
//! frame and often more packets than pointer events (the system merges
//! mouse moves it has not handed over yet). [`Router::flush`] joins
//! them once a frame: every packet becomes one sample, placed along the
//! path the pointer took in that frame, as far along it as the pen had
//! travelled on the tablet when the packet was made (or, where a packet
//! does not say, evenly, since a tablet reports at a steady rate). The
//! pressure, the lean and the time of each sample are the packet's own.
//!
//! While no pen speaks, the same pointer events are a mouse's: one
//! sample each, at a constant full pressure while its main button is
//! held. Touch events are samples as they come.

use std::sync::Mutex;

use super::{PenKind, PenSample, PenSource, Sample, SampleKind, Stream};

/// How long after a pen's last packet the pointer still counts as that
/// pen's, in seconds: a driver sends the button's release, and the system
/// the pointer's last moves, a little after the packets they belong to.
const PEN_HOLD: f64 = 0.25;

/// A packet of a pen that does not say where it is in the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Reading {
    /// Whose packet.
    pub(crate) source: PenSource,
    /// When, in seconds on the stream's clock (see
    /// [`super::DeviceClock`]).
    pub(crate) time: f64,
    /// How hard the tip presses, 0 to 1; `None` while it does not touch.
    pub(crate) pressure: Option<f32>,
    /// Its lean, as [`PenSample::tilt`] has it.
    pub(crate) tilt: Option<[f32; 2]>,
    /// Its twist, as [`PenSample::twist`] has it.
    pub(crate) twist: Option<f32>,
    /// Its eraser end is the one in use.
    pub(crate) eraser: bool,
    /// Where the pen is on its own device, in whatever units and
    /// directions the device counts: only how far it travelled from one
    /// packet to the next is read from it. `None` where a packet does not
    /// say.
    pub(crate) at: Option<[f64; 2]>,
}

impl Reading {
    /// A browser's pen event as a reading: `time` on the stream's clock,
    /// `touching` whether the tip is down after it, `pressure` as the
    /// event has it, `tilt` in degrees to the right and toward the
    /// person, `twist` in degrees clockwise, `client` the event's place
    /// on the page.
    #[cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]
    pub(crate) fn of_browser_pen(
        time: f64,
        touching: bool,
        pressure: f32,
        tilt: [i32; 2],
        twist: i32,
        client: [i32; 2],
    ) -> Self {
        Self {
            source: PenSource::Browser,
            time,
            pressure: touching.then_some(pressure),
            tilt: Some(tilt.map(|degrees| (degrees as f32 / 90.0).clamp(-1.0, 1.0))),
            twist: Some(twist as f32),
            eraser: false,
            at: Some(client.map(f64::from)),
        }
    }
}

/// The packets since the last frame, oldest first.
static READINGS: Mutex<Vec<Reading>> = Mutex::new(Vec::new());

/// How many packets wait at most (a frame that never comes keeps the
/// latest).
const READINGS_KEPT: usize = 4096;

/// How many pointer events wait at most before further moves replace the
/// latest one.
const POINTERS_KEPT: usize = 4096;

/// Queues a pen's packet for the next frame's [`Router::flush`].
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
    allow(dead_code)
)]
pub(crate) fn queue_reading(reading: Reading) {
    if let Ok(mut readings) = READINGS.lock() {
        if readings.len() >= READINGS_KEPT {
            readings.remove(0);
        }
        readings.push(reading);
    }
}

/// The packets queued since the last frame.
fn take_readings() -> Vec<Reading> {
    READINGS
        .lock()
        .map(|mut readings| std::mem::take(&mut *readings))
        .unwrap_or_default()
}

/// One of the main window's mouse events, as egui will see it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pointer {
    kind: SampleKind,
    /// In egui points.
    pos: [f32; 2],
    /// When it reached Sue, on the stream's clock.
    time: f64,
    /// A button event of the main button.
    primary: bool,
}

/// The path the pointer took in a frame.
struct Path {
    points: Vec<[f32; 2]>,
    /// How far along the path each point is.
    lengths: Vec<f32>,
}

impl Path {
    /// `points` must not be empty.
    fn new(points: Vec<[f32; 2]>) -> Self {
        let mut lengths = Vec::with_capacity(points.len());
        let mut length = 0.0;
        for (index, point) in points.iter().enumerate() {
            if index > 0 {
                let before = points[index - 1];
                length += (point[0] - before[0]).hypot(point[1] - before[1]);
            }
            lengths.push(length);
        }
        Self { points, lengths }
    }

    fn total(&self) -> f32 {
        self.lengths.last().copied().unwrap_or(0.0)
    }

    /// How far along the path point `index` is, 0 to 1.
    fn share(&self, index: usize) -> f32 {
        let total = self.total();
        if total > 0.0 {
            self.lengths[index] / total
        } else {
            1.0
        }
    }

    /// The place `share` (0 to 1) of the way along the path.
    fn at(&self, share: f32) -> [f32; 2] {
        let last = self.points[self.points.len() - 1];
        let total = self.total();
        if total <= 0.0 || share.is_nan() {
            return last;
        }
        let target = share.clamp(0.0, 1.0) * total;
        for index in 1..self.points.len() {
            let (from, to) = (self.lengths[index - 1], self.lengths[index]);
            if target <= to {
                let step = to - from;
                let along = if step > 0.0 {
                    (target - from) / step
                } else {
                    1.0
                };
                let (a, b) = (self.points[index - 1], self.points[index]);
                return [a[0] + (b[0] - a[0]) * along, a[1] + (b[1] - a[1]) * along];
            }
        }
        last
    }
}

/// How far along a frame's pointer path each of its packets belongs, 0
/// to 1, the last at the end: by how far the pen had travelled on its
/// device since `carry` (where the last frame's last packet was) when
/// every packet says where it was, else evenly.
fn shares(readings: &[Reading], carry: Option<[f64; 2]>) -> Vec<f32> {
    let count = readings.len();
    let evenly = || {
        (0..count)
            .map(|index| (index + 1) as f32 / count as f32)
            .collect()
    };
    let Some(places) = readings
        .iter()
        .map(|reading| reading.at)
        .collect::<Option<Vec<_>>>()
    else {
        return evenly();
    };
    let mut before = carry.or(places.first().copied()).unwrap_or([0.0; 2]);
    let mut travelled = 0.0;
    let mut run = Vec::with_capacity(count);
    for place in places {
        travelled += (place[0] - before[0]).hypot(place[1] - before[1]);
        run.push(travelled);
        before = place;
    }
    if !(travelled.is_finite() && travelled > 0.0) {
        return evenly();
    }
    run.into_iter()
        .map(|so_far| (so_far / travelled) as f32)
        .collect()
}

/// Joins the main window's pointer and touch events with the pens'
/// packets, frame by frame. The host owns one.
#[derive(Debug, Default)]
pub(crate) struct Router {
    /// The mouse events since the last frame.
    pointers: Vec<Pointer>,
    /// Where the pointer was when the last frame took its events.
    start: Option<[f32; 2]>,
    /// Where it is now, while it is over the window.
    pos: Option<[f32; 2]>,
    /// The main button is held.
    primary_held: bool,
    /// A mouse's stroke is open in the stream.
    mouse_down: bool,
    /// The main button went down while a pen had the pointer: its
    /// release is that pen's too, however late it comes.
    pen_press: bool,
    /// The last per-event reading had a pressure: whoever follows it by
    /// events is told when the pen has gone.
    stale: bool,
    /// The touch that stands for the pointer (the first to land).
    touch: Option<u64>,
    /// Where the last packet said the pen was on its device.
    device: Option<[f64; 2]>,
}

impl Router {
    /// The pointer moved to `pos` (egui points) at `time`.
    pub(crate) fn moved(&mut self, pos: [f32; 2], time: f64) {
        self.pos = Some(pos);
        let moved = Pointer {
            kind: SampleKind::Move,
            pos,
            time,
            primary: false,
        };
        // A frame that never comes (the window is paused) keeps the
        // buttons and the latest place, not every move on the way.
        let full = self.pointers.len() >= POINTERS_KEPT;
        match self.pointers.last_mut() {
            Some(last) if full && last.kind == SampleKind::Move => *last = moved,
            _ => self.pointers.push(moved),
        }
    }

    /// A mouse button went down or up at `time`, where the pointer is.
    pub(crate) fn button(&mut self, primary: bool, pressed: bool, time: f64) {
        let Some(pos) = self.pos else {
            return;
        };
        self.pointers.push(Pointer {
            kind: if pressed {
                SampleKind::Press
            } else {
                SampleKind::Release
            },
            pos,
            time,
            primary,
        });
    }

    /// The pointer left the window.
    pub(crate) fn left(&mut self) {
        self.pos = None;
    }

    /// A touch event of the window: a finger, or a pen whose force comes
    /// with it. The first touch to land is the one that draws, as it is
    /// the one that moves the pointer; the others belong to gestures.
    #[cfg(test)]
    pub(crate) fn touch(
        &mut self,
        stream: &mut Stream,
        id: u64,
        phase: egui::TouchPhase,
        pos: [f32; 2],
        force: Option<f32>,
        time: f64,
    ) {
        let mut sample = PenSample::at(PenSource::Touch, pos, time);
        sample.pressure = force;
        self.touch_sample(stream, id, phase, sample);
    }

    /// [`Self::touch`] for a touch whose packet says more than winit's
    /// event: `sample` (of [`PenSource::Touch`]) with the packet's own
    /// pressure, lean, twist, end and time (a Windows Ink pen).
    pub(crate) fn touch_sample(
        &mut self,
        stream: &mut Stream,
        id: u64,
        phase: egui::TouchPhase,
        mut sample: PenSample,
    ) {
        let pos = sample.pos;
        match phase {
            egui::TouchPhase::Start => {
                if self.touch.is_some() {
                    return;
                }
                self.touch = Some(id);
                self.pos = Some(pos);
                sample.kind = PenKind::Down;
            }
            egui::TouchPhase::Move => {
                if self.touch != Some(id) {
                    return;
                }
                self.pos = Some(pos);
            }
            egui::TouchPhase::End | egui::TouchPhase::Cancel => {
                if self.touch != Some(id) {
                    return;
                }
                self.touch = None;
                // The pointer goes with the touch.
                self.pos = None;
                sample.kind = PenKind::Up;
            }
        }
        stream.push(sample);
    }

    /// A frame begins: everything since the last one goes to the real
    /// stream, the pens' queued packets with it.
    pub(crate) fn flush_frame(&mut self) {
        let readings = take_readings();
        super::real(|stream| self.flush(stream, &readings));
    }

    /// Sends everything since the last frame to `stream`: `readings`
    /// along the pointer's path when there are any, else the pointer's
    /// events as a mouse's (or, just after a pen spoke, as that pen's).
    pub(crate) fn flush(&mut self, stream: &mut Stream, readings: &[Reading]) {
        let pointers = std::mem::take(&mut self.pointers);
        if readings.is_empty() {
            self.device = None;
            for pointer in pointers {
                self.alone(stream, pointer);
            }
        } else {
            self.with_pen(stream, readings, &pointers);
        }
        self.start = self.pos;
    }

    /// Ends a mouse's stroke that a pen cut short.
    fn end_mouse(&mut self, stream: &mut Stream, pos: [f32; 2], time: f64) {
        if self.mouse_down {
            self.mouse_down = false;
            let mut up = PenSample::at(PenSource::Mouse, pos, time);
            up.kind = PenKind::Up;
            stream.push(up);
        }
    }

    /// Follows the main button through a pen's pointer event.
    fn follow_pen_button(&mut self, pointer: Pointer) {
        if pointer.primary {
            let pressed = pointer.kind == SampleKind::Press;
            self.primary_held = pressed;
            self.pen_press = pressed;
        }
    }

    /// A frame with packets: each is a sample along the pointer's path,
    /// and each pointer event gets the reading of the packet made there.
    fn with_pen(&mut self, stream: &mut Stream, readings: &[Reading], pointers: &[Pointer]) {
        let mut points: Vec<[f32; 2]> = self.start.into_iter().collect();
        let first = points.len();
        points.extend(pointers.iter().map(|pointer| pointer.pos));
        let shares = shares(readings, self.device);
        self.device = readings.last().and_then(|reading| reading.at);
        if points.is_empty() || self.touch.is_some() {
            // The pen is not over the window, or the window hears it as
            // a touch as well (Windows Ink beside WinTab): the touch
            // events are its samples, and say where it is.
            for reading in readings {
                stream.push_unplaced(
                    reading.source,
                    reading.time,
                    reading.pressure,
                    (reading.tilt, reading.eraser),
                );
            }
            return;
        }
        self.end_mouse(stream, points[0], readings[0].time);
        let path = Path::new(points);
        // What the tip pressed with coming into this frame, lifted or not.
        let before = stream
            .held(readings[0].source)
            .or_else(|| stream.pressure());
        for (reading, share) in readings.iter().zip(&shares) {
            let sample = PenSample {
                pos: path.at(*share),
                time: reading.time,
                pressure: reading.pressure,
                tilt: reading.tilt,
                twist: reading.twist,
                kind: PenKind::Move,
                source: reading.source,
                eraser: reading.eraser,
            };
            stream.push_state(sample, reading.pressure.is_some());
        }
        let last = readings.len() - 1;
        let moved = path.total() > 0.0;
        for (index, pointer) in pointers.iter().enumerate() {
            let press = pointer.primary && pointer.kind == SampleKind::Press;
            let release = pointer.primary && pointer.kind == SampleKind::Release;
            // The packet made where this event is: the first one at or
            // past it. Where the pointer did not move the last one counts,
            // and for the tip going down the first that touches.
            let at = if moved {
                let share = path.share(first + index);
                shares
                    .iter()
                    .position(|made| *made >= share - 1e-6)
                    .unwrap_or(last)
            } else if press {
                readings
                    .iter()
                    .position(|reading| reading.pressure.is_some())
                    .unwrap_or(last)
            } else {
                last
            };
            let reading = readings[at];
            let pressure = if press {
                // The button can come a packet before the pressure.
                reading
                    .pressure
                    .or_else(|| readings[at..].iter().find_map(|later| later.pressure))
            } else if release {
                // What the tip had just before it lifted.
                readings[..=at]
                    .iter()
                    .rev()
                    .find_map(|earlier| earlier.pressure)
                    .or(before)
            } else {
                reading.pressure
            };
            stream.push_paired(Sample {
                kind: pointer.kind,
                pos: pointer.pos,
                pressure,
                tilt: reading.tilt,
                eraser: reading.eraser,
            });
            self.stale = pressure.is_some() && !release;
            self.follow_pen_button(*pointer);
        }
    }

    /// A pointer event of a frame without packets: a mouse's, unless a
    /// pen spoke a moment ago, touches still, or pressed the button this
    /// event lets go.
    fn alone(&mut self, stream: &mut Stream, pointer: Pointer) {
        let pen = stream.pen().filter(|(source, spoke)| {
            self.pen_press || stream.touches(*source) || pointer.time <= spoke + PEN_HOLD
        });
        if let Some((source, _)) = pen {
            self.end_mouse(stream, pointer.pos, pointer.time);
            if source.follows_the_pointer() {
                let release = pointer.primary && pointer.kind == SampleKind::Release;
                let touching = stream.held(source);
                let pressure = if release {
                    touching.or_else(|| stream.pressure())
                } else {
                    touching
                };
                let (tilt, eraser) = stream.tool();
                stream.push_paired(Sample {
                    kind: pointer.kind,
                    pos: pointer.pos,
                    pressure,
                    tilt,
                    eraser,
                });
                self.stale = pressure.is_some() && !release;
            }
            self.follow_pen_button(pointer);
            return;
        }
        if self.stale {
            // The pen has gone: whoever kept its last reading drops it.
            self.stale = false;
            stream.push_paired(Sample {
                kind: pointer.kind,
                pos: pointer.pos,
                pressure: None,
                tilt: None,
                eraser: false,
            });
        }
        let mut sample = PenSample::at(PenSource::Mouse, pointer.pos, pointer.time);
        match (pointer.kind, pointer.primary) {
            (SampleKind::Press, true) => {
                self.primary_held = true;
                if self.mouse_down {
                    return;
                }
                self.mouse_down = true;
                sample.kind = PenKind::Down;
                sample.pressure = Some(1.0);
            }
            (SampleKind::Release, true) => {
                self.primary_held = false;
                if !self.mouse_down {
                    return;
                }
                self.mouse_down = false;
                sample.kind = PenKind::Up;
            }
            (SampleKind::Move, _) => {
                if self.primary_held && !self.mouse_down {
                    // Its press was taken for a pen's: it goes down here.
                    self.mouse_down = true;
                    sample.kind = PenKind::Down;
                }
                if self.mouse_down {
                    sample.pressure = Some(1.0);
                }
            }
            // The other buttons draw nothing.
            _ => return,
        }
        stream.push(sample);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(time: f64, pressure: Option<f32>) -> Reading {
        Reading {
            source: PenSource::WinTab,
            time,
            pressure,
            tilt: None,
            twist: None,
            eraser: false,
            at: None,
        }
    }

    fn kinds(samples: &[PenSample]) -> Vec<PenKind> {
        samples.iter().map(|sample| sample.kind).collect()
    }

    /// What every stream must hold, whatever went in: time never goes
    /// backwards, each source goes down, moves and comes up in turn, an
    /// up carries only its place, and nothing after a source's last
    /// touching sample presses at all, let alone harder than it.
    pub(crate) fn assert_sound(samples: &[PenSample]) {
        let mut time = f64::NEG_INFINITY;
        let mut touching = [false; super::super::SOURCES];
        for sample in samples {
            assert!(sample.time >= time, "time went backwards: {sample:?}");
            time = sample.time;
            let down = &mut touching[sample.source.index()];
            match sample.kind {
                PenKind::Down => {
                    assert!(!*down, "down twice: {sample:?}");
                    *down = true;
                }
                PenKind::Up => {
                    assert!(*down, "up without a down: {sample:?}");
                    *down = false;
                    assert_eq!(sample.pressure, None, "an up presses: {sample:?}");
                    assert_eq!(sample.tilt, None);
                    assert_eq!(sample.twist, None);
                }
                PenKind::Move => {}
            }
            if !*down {
                assert_eq!(sample.pressure, None, "pressure in the air: {sample:?}");
            }
            if let Some(pressure) = sample.pressure {
                assert!((0.0..=1.0).contains(&pressure));
            }
        }
    }

    #[test]
    fn a_mouse_is_a_source_with_a_constant_pressure() {
        let mut router = Router::default();
        let mut stream = Stream::new();
        router.moved([1.0, 1.0], 0.10);
        router.button(true, true, 0.11);
        router.moved([2.0, 1.0], 0.12);
        router.moved([3.0, 1.0], 0.13);
        // The other buttons draw nothing.
        router.button(false, true, 0.135);
        router.button(false, false, 0.136);
        router.button(true, false, 0.14);
        router.moved([4.0, 1.0], 0.15);
        router.flush(&mut stream, &[]);
        let mouse = |kind, x: f32, time, pressure| PenSample {
            kind,
            pressure,
            ..PenSample::at(PenSource::Mouse, [x, 1.0], time)
        };
        let samples = stream.take();
        assert_eq!(
            samples,
            [
                mouse(PenKind::Move, 1.0, 0.10, None),
                mouse(PenKind::Down, 1.0, 0.11, Some(1.0)),
                mouse(PenKind::Move, 2.0, 0.12, Some(1.0)),
                mouse(PenKind::Move, 3.0, 0.13, Some(1.0)),
                mouse(PenKind::Up, 3.0, 0.14, None),
                mouse(PenKind::Move, 4.0, 0.15, None),
            ]
        );
        assert_sound(&samples);
        // Nothing of it is a pen's.
        assert_eq!(stream.pressure(), None);
        assert_eq!(stream.pen(), None);
        assert!(std::mem::take(&mut stream.paired).is_empty());
        // A button with the pointer outside the window is nobody's.
        router.left();
        router.button(true, true, 0.2);
        router.flush(&mut stream, &[]);
        assert!(stream.take().is_empty());
    }

    #[test]
    fn touch_events_are_samples_as_they_come() {
        let mut router = Router::default();
        let mut stream = Stream::new();
        use egui::TouchPhase::{Cancel, End, Move, Start};
        // A pen with Windows Ink: force with every event.
        router.touch(&mut stream, 7, Start, [5.0, 5.0], Some(0.2), 1.00);
        router.touch(&mut stream, 7, Move, [6.0, 5.0], Some(0.5), 1.01);
        // A second finger lands: a gesture's, not the line's.
        router.touch(&mut stream, 8, Start, [50.0, 50.0], None, 1.015);
        router.touch(&mut stream, 8, Move, [51.0, 50.0], None, 1.016);
        router.touch(&mut stream, 7, Move, [7.0, 5.0], Some(0.3), 1.02);
        router.touch(&mut stream, 8, End, [51.0, 50.0], None, 1.025);
        router.touch(&mut stream, 7, End, [7.5, 5.0], Some(0.0), 1.03);
        let touch = |kind, x: f32, time, pressure| PenSample {
            kind,
            pressure,
            ..PenSample::at(PenSource::Touch, [x, 5.0], time)
        };
        let samples = stream.take();
        assert_eq!(
            samples,
            [
                touch(PenKind::Down, 5.0, 1.00, Some(0.2)),
                touch(PenKind::Move, 6.0, 1.01, Some(0.5)),
                touch(PenKind::Move, 7.0, 1.02, Some(0.3)),
                touch(PenKind::Up, 7.5, 1.03, None),
            ]
        );
        assert_sound(&samples);
        // A finger on a screen that feels no force: no pressure, and no
        // made-up one.
        router.touch(&mut stream, 9, Start, [1.0, 5.0], None, 2.0);
        router.touch(&mut stream, 9, Move, [2.0, 5.0], None, 2.1);
        router.touch(&mut stream, 9, Cancel, [2.0, 5.0], None, 2.2);
        let samples = stream.take();
        assert_eq!(kinds(&samples), [PenKind::Down, PenKind::Move, PenKind::Up]);
        assert!(samples.iter().all(|sample| sample.pressure.is_none()));
        // The older readers know nothing of a touch.
        assert_eq!(stream.pressure(), None);
        assert_eq!(stream.pen(), None);
    }

    #[test]
    fn each_packet_is_a_sample_along_the_pointers_path() {
        let mut router = Router::default();
        let mut stream = Stream::new();
        router.moved([0.0, 0.0], 0.9);
        router.flush(&mut stream, &[]);
        stream.take();
        // One frame: the pointer made two moves while the tablet sent
        // four packets, the tip going down in the second.
        router.moved([10.0, 0.0], 1.016);
        router.moved([20.0, 0.0], 1.016);
        let packets = [
            reading(1.000, None),
            reading(1.005, Some(0.2)),
            reading(1.010, Some(0.5)),
            reading(1.015, Some(0.4)),
        ];
        router.flush(&mut stream, &packets);
        let samples = stream.take();
        let pen = |kind, x: f32, time, pressure| PenSample {
            kind,
            pressure,
            ..PenSample::at(PenSource::WinTab, [x, 0.0], time)
        };
        assert_eq!(
            samples,
            [
                pen(PenKind::Move, 5.0, 1.000, None),
                pen(PenKind::Down, 10.0, 1.005, Some(0.2)),
                pen(PenKind::Move, 15.0, 1.010, Some(0.5)),
                pen(PenKind::Move, 20.0, 1.015, Some(0.4)),
            ]
        );
        assert_eq!(stream.pressure(), Some(0.4));
        // Each pointer event reads the packet made where it is, not the
        // frame's last.
        let paired: Vec<_> = std::mem::take(&mut stream.paired).into();
        assert_eq!(paired.len(), 2);
        assert!(paired[0].is_for(SampleKind::Move, [10.0, 0.0]));
        assert_eq!(paired[0].pressure, Some(0.2));
        assert!(paired[1].is_for(SampleKind::Move, [20.0, 0.0]));
        assert_eq!(paired[1].pressure, Some(0.4));
        // The next frame: the tip lifts, then the button's release comes.
        router.moved([30.0, 0.0], 1.033);
        router.button(true, false, 1.033);
        router.flush(
            &mut stream,
            &[reading(1.020, Some(0.3)), reading(1.025, None)],
        );
        let more = stream.take();
        assert_eq!(
            more,
            [
                pen(PenKind::Move, 25.0, 1.020, Some(0.3)),
                pen(PenKind::Up, 30.0, 1.025, None),
            ]
        );
        let all: Vec<_> = samples.into_iter().chain(more).collect();
        assert_sound(&all);
        // The release reads what the tip had just before it lifted, and
        // so does the latest pressure until the frame has run.
        let paired: Vec<_> = std::mem::take(&mut stream.paired).into();
        assert_eq!(paired[1].kind, SampleKind::Release);
        assert_eq!(paired[1].pressure, Some(0.3));
        assert_eq!(stream.pressure(), Some(0.3));
    }

    #[test]
    fn packets_sit_where_the_pen_had_travelled_to() {
        let mut router = Router::default();
        let mut stream = Stream::new();
        router.moved([0.0, 0.0], 0.9);
        router.flush(&mut stream, &[]);
        stream.take();
        // The tablet counts upward and ten times finer than the screen:
        // only how far the pen went matters. It went slowly, then fast.
        let at = |time, y: f64| Reading {
            at: Some([300.0, y]),
            ..reading(time, Some(0.5))
        };
        router.moved([100.0, 0.0], 1.016);
        router.flush(
            &mut stream,
            &[at(1.000, 5000.0), at(1.005, 4900.0), at(1.010, 4000.0)],
        );
        let xs: Vec<f32> = stream.take().iter().map(|sample| sample.pos[0]).collect();
        // The first packet is where the frame began (no travel yet).
        assert_eq!(xs, [0.0, 10.0, 100.0]);
        // The next frame goes on from where the last packet was.
        router.moved([200.0, 0.0], 1.033);
        router.flush(&mut stream, &[at(1.015, 3500.0), at(1.020, 3000.0)]);
        let xs: Vec<f32> = stream.take().iter().map(|sample| sample.pos[0]).collect();
        assert_eq!(xs, [150.0, 200.0]);
    }

    #[test]
    fn a_pen_that_presses_in_place_still_reaches_the_stream() {
        let mut router = Router::default();
        let mut stream = Stream::new();
        router.moved([4.0, 4.0], 0.9);
        router.flush(&mut stream, &[]);
        stream.take();
        // No pointer event at all: the pen stands still and presses.
        router.button(true, true, 1.016);
        router.flush(
            &mut stream,
            &[
                reading(1.000, None),
                reading(1.005, Some(0.1)),
                reading(1.010, Some(0.6)),
            ],
        );
        let samples = stream.take();
        assert_eq!(
            kinds(&samples),
            [PenKind::Move, PenKind::Down, PenKind::Move]
        );
        assert!(samples.iter().all(|sample| sample.pos == [4.0, 4.0]));
        // The press reads the first touching packet, not a hovering one.
        let paired: Vec<_> = std::mem::take(&mut stream.paired).into();
        assert_eq!(paired[0].kind, SampleKind::Press);
        assert_eq!(paired[0].pressure, Some(0.1));
        // A frame of packets only.
        router.flush(&mut stream, &[reading(1.020, Some(0.9))]);
        let more = stream.take();
        assert_eq!(more[0].pos, [4.0, 4.0]);
        assert_eq!(more[0].pressure, Some(0.9));
    }

    #[test]
    fn the_pointer_stays_the_pens_until_its_release_has_come() {
        let mut router = Router::default();
        let mut stream = Stream::new();
        router.moved([0.0, 0.0], 0.9);
        router.button(true, true, 1.0);
        router.flush(&mut stream, &[reading(1.0, Some(0.5))]);
        stream.take();
        stream.paired.clear();
        // The tip lifts; the button's release comes two frames and half
        // a second later, without packets.
        router.flush(&mut stream, &[reading(1.1, None)]);
        stream.end_frame();
        router.moved([3.0, 0.0], 1.6);
        router.button(true, false, 1.6);
        router.moved([6.0, 0.0], 1.61);
        router.flush(&mut stream, &[]);
        // The move and the release are the pen's own late events: no
        // sample, and no mouse stroke left open. What follows the
        // release is a mouse's again.
        let samples = stream.take();
        assert_eq!(
            samples,
            [PenSample::at(PenSource::Mouse, [6.0, 0.0], 1.61)],
            "only the move after the release"
        );
        let paired: Vec<_> = std::mem::take(&mut stream.paired).into();
        assert_eq!(paired.len(), 2, "its move and its release");
        assert_eq!(paired[1].kind, SampleKind::Release);
        // Long after, the mouse is a mouse again.
        router.moved([9.0, 0.0], 2.0);
        router.button(true, true, 2.1);
        router.flush(&mut stream, &[]);
        let samples = stream.take();
        assert_eq!(kinds(&samples), [PenKind::Move, PenKind::Down]);
        assert!(
            samples
                .iter()
                .all(|sample| sample.source == PenSource::Mouse)
        );
    }

    #[test]
    fn whoever_kept_the_pens_last_reading_is_told_it_has_gone() {
        let mut router = Router::default();
        let mut stream = Stream::new();
        // A touch too light to click: packets with pressure, moves, no
        // button.
        router.moved([0.0, 0.0], 1.0);
        router.flush(&mut stream, &[reading(1.0, Some(0.05))]);
        let paired: Vec<_> = std::mem::take(&mut stream.paired).into();
        assert_eq!(paired[0].pressure, Some(0.05));
        // It lifts without the pointer moving again, and goes away.
        router.flush(&mut stream, &[reading(1.1, None)]);
        stream.end_frame();
        // Much later a mouse moves: its first event says "no pressure".
        router.moved([5.0, 0.0], 9.0);
        router.moved([6.0, 0.0], 9.1);
        router.flush(&mut stream, &[]);
        let paired: Vec<_> = std::mem::take(&mut stream.paired).into();
        assert_eq!(paired.len(), 1, "told once");
        assert!(paired[0].is_for(SampleKind::Move, [5.0, 0.0]));
        assert_eq!(paired[0].pressure, None);
    }

    #[test]
    fn a_mouse_stroke_a_pen_cuts_short_is_ended() {
        let mut router = Router::default();
        let mut stream = Stream::new();
        router.moved([0.0, 0.0], 1.0);
        router.button(true, true, 1.0);
        router.flush(&mut stream, &[]);
        router.moved([2.0, 0.0], 1.1);
        router.flush(&mut stream, &[reading(1.1, Some(0.5))]);
        let samples = stream.take();
        assert_sound(&samples);
        let mouse: Vec<_> = samples
            .iter()
            .filter(|sample| sample.source == PenSource::Mouse)
            .map(|sample| sample.kind)
            .collect();
        assert_eq!(mouse, [PenKind::Move, PenKind::Down, PenKind::Up]);
    }

    #[test]
    fn a_pen_outside_the_window_makes_no_samples() {
        let mut router = Router::default();
        let mut stream = Stream::new();
        router.flush(&mut stream, &[reading(1.0, Some(0.5))]);
        assert!(stream.take().is_empty());
        assert_eq!(stream.pressure(), Some(0.5), "the older readers hear");
    }

    #[test]
    fn a_pen_heard_as_a_touch_too_is_in_the_stream_once() {
        let mut router = Router::default();
        let mut stream = Stream::new();
        // A mouse was here before.
        router.moved([50.0, 50.0], 0.5);
        router.flush(&mut stream, &[]);
        stream.take();
        // Windows Ink is on and the driver sends WinTab packets as well.
        router.touch(
            &mut stream,
            1,
            egui::TouchPhase::Start,
            [5.0, 5.0],
            Some(0.4),
            1.0,
        );
        router.flush(&mut stream, &[reading(1.0, Some(0.4))]);
        router.touch(
            &mut stream,
            1,
            egui::TouchPhase::Move,
            [6.0, 5.0],
            Some(0.5),
            1.01,
        );
        router.flush(&mut stream, &[reading(1.01, Some(0.5))]);
        let samples = stream.take();
        assert_eq!(kinds(&samples), [PenKind::Down, PenKind::Move]);
        assert!(
            samples
                .iter()
                .all(|sample| sample.source == PenSource::Touch)
        );
        // The older readers still hear the tablet's pressure.
        assert_eq!(stream.pressure(), Some(0.5));
    }

    #[test]
    fn a_browsers_pen_event_is_a_reading() {
        let down = Reading::of_browser_pen(2.0, true, 0.25, [45, -90], 359, [10, 20]);
        assert_eq!(down.source, PenSource::Browser);
        assert_eq!(down.pressure, Some(0.25));
        assert_eq!(down.tilt, Some([0.5, -1.0]));
        assert_eq!(down.twist, Some(359.0));
        assert_eq!(down.at, Some([10.0, 20.0]));
        // A pen that hovers has a pressure of 0 in the event: none here.
        let up = Reading::of_browser_pen(2.1, false, 0.0, [0, 0], 0, [10, 20]);
        assert_eq!(up.pressure, None);
        // Through the router: the page's events, in their order.
        let mut router = Router::default();
        let mut stream = Stream::new();
        router.moved([10.0, 20.0], 2.0);
        router.button(true, true, 2.0);
        router.flush(&mut stream, &[down]);
        router.moved([12.0, 20.0], 2.05);
        router.flush(
            &mut stream,
            &[Reading::of_browser_pen(
                2.05,
                true,
                0.5,
                [0, 0],
                0,
                [12, 20],
            )],
        );
        router.button(true, false, 2.1);
        router.flush(&mut stream, &[up]);
        let samples = stream.take();
        assert_sound(&samples);
        assert_eq!(kinds(&samples), [PenKind::Down, PenKind::Move, PenKind::Up]);
        assert_eq!(
            samples
                .iter()
                .map(|sample| sample.pressure)
                .collect::<Vec<_>>(),
            [Some(0.25), Some(0.5), None]
        );
        assert_eq!(samples[1].pos, [12.0, 20.0]);
        assert!(
            samples
                .iter()
                .all(|sample| sample.source == PenSource::Browser)
        );
    }

    #[test]
    fn whatever_goes_in_the_stream_stays_sound() {
        // A scripted mess, from a seeded generator: packets with times
        // out of order, pointer events, touches, frames ending anywhere.
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = move |below: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % below
        };
        let mut router = Router::default();
        let mut stream = Stream::new();
        let mut all = Vec::new();
        let mut clock = 0.0;
        for frame in 0..600 {
            // Every other stretch of frames the pen is away: the pointer
            // is a mouse's then.
            let pen_away = (frame / 25) % 2 == 1;
            if frame % 25 == 0 {
                clock += 1.0;
            }
            let mut readings = Vec::new();
            for _ in 0..next(6) {
                clock += 0.004;
                let x = next(200) as f32;
                match next(8) {
                    4 | 5 if pen_away => router.moved([x, next(200) as f32], clock),
                    0 => router.button(true, next(2) == 0, clock),
                    1 => router.button(false, next(2) == 0, clock),
                    2 => router.left(),
                    3 => {
                        let phase = match next(4) {
                            0 => egui::TouchPhase::Start,
                            1 => egui::TouchPhase::Move,
                            2 => egui::TouchPhase::End,
                            _ => egui::TouchPhase::Cancel,
                        };
                        let force = (next(2) == 0).then(|| next(100) as f32 / 50.0);
                        router.touch(&mut stream, 1, phase, [x, 1.0], force, clock);
                    }
                    4 | 5 => {
                        let mut packet = reading(
                            // Now and then a packet from the past.
                            clock - next(3) as f64 * 0.01,
                            (next(3) != 0).then(|| next(100) as f32 / 50.0),
                        );
                        if next(2) == 0 {
                            packet.at = Some([next(5000) as f64, next(5000) as f64]);
                        }
                        readings.push(packet);
                    }
                    _ => router.moved([x, next(200) as f32], clock),
                }
            }
            router.flush(&mut stream, &readings);
            if next(3) == 0 {
                clock += 0.3;
            }
            if let Some(latest) = stream.pressure() {
                assert!((0.0..=1.0).contains(&latest));
            }
            let frame = stream.take();
            all.extend(frame);
            stream.end_frame();
        }
        for source in [PenSource::WinTab, PenSource::Mouse, PenSource::Touch] {
            let own = all.iter().filter(|s| s.source == source).count();
            assert!(
                own > 40,
                "the script made a real stream of {source:?}: {own}"
            );
        }
        assert_sound(&all);
        // From each lift to the source's next touch nothing presses
        // harder than the last contact did: nothing presses at all. The
        // script must have lifted each source often for that to count.
        for source in [PenSource::WinTab, PenSource::Mouse, PenSource::Touch] {
            let mut contact: Option<f32> = None;
            let mut lifted = false;
            let mut lifts = 0;
            for sample in all.iter().filter(|s| s.source == source) {
                match sample.kind {
                    PenKind::Down => lifted = false,
                    PenKind::Up => {
                        lifted = true;
                        lifts += 1;
                    }
                    PenKind::Move => {}
                }
                if lifted {
                    let after = sample.pressure.unwrap_or(0.0);
                    assert!(
                        sample.pressure.is_none() && after <= contact.unwrap_or(0.0),
                        "{sample:?} presses after the contact at {contact:?}"
                    );
                } else if sample.pressure.is_some() {
                    contact = sample.pressure;
                }
            }
            assert!(lifts > 5, "{source:?} lifted {lifts} times");
        }
    }

    #[test]
    fn moves_without_a_frame_do_not_pile_up() {
        let mut router = Router::default();
        let mut stream = Stream::new();
        router.moved([0.0, 0.0], 0.0);
        router.button(true, true, 0.0);
        for step in 1..=POINTERS_KEPT + 500 {
            router.moved([step as f32, 0.0], step as f64 * 0.001);
        }
        router.button(true, false, 9.0);
        assert_eq!(router.pointers.len(), POINTERS_KEPT + 1);
        router.flush(&mut stream, &[]);
        let samples = stream.take();
        assert_sound(&samples);
        // The latest place and the release are kept.
        let end = (POINTERS_KEPT + 500) as f32;
        let last = samples[samples.len() - 1];
        assert_eq!((last.kind, last.pos), (PenKind::Up, [end, 0.0]));
        assert_eq!(samples[samples.len() - 2].pos, [end, 0.0]);
    }

    #[test]
    fn the_check_of_a_sound_stream_sees_a_press_after_the_lift() {
        // The check itself can fail: a release that presses, and a sample
        // that presses in the air, are caught.
        let touch = |kind, pressure| PenSample {
            kind,
            pressure,
            ..PenSample::at(PenSource::Touch, [0.0, 0.0], 1.0)
        };
        let down = touch(PenKind::Down, Some(0.2));
        for bad in [
            touch(PenKind::Up, Some(1.0)),
            touch(PenKind::Down, Some(0.5)),
        ] {
            let caught = std::panic::catch_unwind(|| assert_sound(&[down, bad]));
            assert!(caught.is_err(), "{bad:?}");
        }
        let up = touch(PenKind::Up, None);
        let caught =
            std::panic::catch_unwind(|| assert_sound(&[down, up, touch(PenKind::Move, Some(1.0))]));
        assert!(caught.is_err(), "pressure in the air");
        assert_sound(&[down, touch(PenKind::Move, Some(0.9)), up]);
    }
}
