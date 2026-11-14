//! A pen on a WinTab tablet driver on Windows: Huion, XP-Pen, Wacom and
//! others always offer WinTab, while Windows Ink (which reaches Sue
//! through winit's touch events) is often turned off in their settings.
//!
//! A system context on Sue's window is opened through the `wintab` crate,
//! which loads the driver's `wintab32.dll` from System32 and keeps its own
//! calls in it (this crate stays free of `unsafe`). Its packet queue is
//! drained every frame and every packet goes to the pen's stream
//! ([`crate::stylus`]) as one reading, in its order: the pressure, lean
//! and twist the pen had in that packet, at the time the driver stamped
//! it with. The pen still moves the system pointer, so where it is in the
//! window comes with the window's pointer events; the stream places each
//! packet along the pointer's path by how far the pen had travelled on
//! the tablet (`stylus/route.rs`).

use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::stylus::{self, DeviceClock, PenSource, Reading};

wintab::packet! {
    struct Packet { time, buttons, x, y, normal_pressure, orientation }
}

/// What the tablet says of its axes.
#[derive(Clone, Copy, Debug, Default)]
struct Axes {
    /// The tip's pressure range.
    pressure: Option<wintab::Axis>,
    /// The ranges of azimuth, altitude and twist, where the pen reports
    /// how it is held.
    orientation: Option<[wintab::Axis; 3]>,
}

/// An open WinTab context on Sue's window.
pub(crate) struct Tablet {
    context: wintab::Context<Packet>,
    axes: Axes,
    /// The driver's packet clock on Sue's.
    clock: DeviceClock,
    packets: Vec<Packet>,
}

impl std::fmt::Debug for Tablet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tablet").finish_non_exhaustive()
    }
}

impl Tablet {
    /// The tablet, when a WinTab driver is installed and answers.
    pub(crate) fn open(window: &Window) -> Option<Self> {
        let RawWindowHandle::Win32(handle) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        let hwnd = windows::Win32::Foundation::HWND(handle.hwnd.get() as *mut std::ffi::c_void);
        let wintab = wintab::Wintab::load().ok()?;
        let device = wintab.device(0);
        let axes = Axes {
            pressure: device.as_ref().and_then(|device| device.pressure),
            orientation: device.as_ref().and_then(|device| device.orientation),
        };
        let options = wintab::ContextOptions::new()
            .system(true)
            .queue_size(Some(128));
        let context = wintab.open::<Packet>(hwnd, &options).ok()?;
        Some(Self {
            context,
            axes,
            clock: DeviceClock::default(),
            packets: Vec::new(),
        })
    }

    /// Takes the packets since the last frame, read at `now` (seconds on
    /// the stream's clock), and queues each as a reading in its turn: not
    /// only a frame's last, which would give every point of the frame one
    /// pressure and lose a whole tap that began and ended between two
    /// frames.
    pub(crate) fn poll(&mut self, now: f64) {
        self.packets.clear();
        if self.context.drain(&mut self.packets) == 0 {
            return;
        }
        for reading in readings(&self.packets, self.axes, &mut self.clock, now) {
            stylus::queue_reading(reading);
        }
    }
}

/// The readings of `packets` (oldest first, drained at `now`), each at
/// the driver's own time carried over to Sue's clock by `clock`.
fn readings(packets: &[Packet], axes: Axes, clock: &mut DeviceClock, now: f64) -> Vec<Reading> {
    let Some(newest) = packets.last() else {
        return Vec::new();
    };
    clock.sync(newest.time, now);
    packets
        .iter()
        .map(|packet| {
            // By value: the packet is packed as the driver writes it.
            let buttons = { packet.buttons };
            let orientation = { packet.orientation };
            let held = axes.orientation.map(|[azimuth, altitude, twist]| {
                (
                    azimuth
                        .degrees(orientation.azimuth)
                        .zip(altitude.degrees(orientation.altitude))
                        .and_then(|(azimuth, altitude)| lean(azimuth, altitude)),
                    twist
                        .degrees(orientation.twist)
                        .map(|degrees| degrees as f32),
                )
            });
            Reading {
                source: PenSource::WinTab,
                time: clock.seconds(packet.time),
                pressure: touching(packet.normal_pressure, buttons.is_pressed(0), axes.pressure),
                tilt: held.and_then(|(lean, _)| lean),
                twist: held.and_then(|(_, twist)| twist),
                eraser: false,
                at: Some([f64::from(packet.x), f64::from(packet.y)]),
            }
        })
        .collect()
}

/// The pressure, 0 to 1, of a packet with tip pressure `raw` (the tip
/// button held or not) on an axis `range`: `None` when the pen does not
/// touch.
fn touching(raw: u32, tip: bool, range: Option<wintab::Axis>) -> Option<f32> {
    if raw == 0 && !tip {
        return None;
    }
    let value = i32::try_from(raw).unwrap_or(i32::MAX);
    Some(range.map_or_else(
        // Without a range, the common 1024 or 8192 levels.
        || (raw as f32 / if raw > 1024 { 8191.0 } else { 1023.0 }).clamp(0.0, 1.0),
        |axis| axis.normalize(value),
    ))
}

/// How far a pen leans, `[x, y]` as [`crate::stylus::PenSample::tilt`]
/// has it (x to the right, y toward the person, 1 for 90 degrees), from
/// WinTab's orientation in degrees: `azimuth` the way the pen's top
/// points, clockwise from straight away from the person, and `altitude`
/// how far the pen stands up from the tablet, 90 upright (negative when
/// it is turned over). `None` for a pen that says it lies flat in the
/// tablet, which is what a driver without the reading reports.
fn lean(azimuth: f64, altitude: f64) -> Option<[f32; 2]> {
    let altitude = altitude.abs().min(90.0);
    if !azimuth.is_finite() || !altitude.is_finite() || altitude < 0.5 {
        return None;
    }
    let rise = altitude.to_radians().tan();
    let way = azimuth.to_radians();
    let right = (way.sin() / rise).atan().to_degrees();
    let away = (way.cos() / rise).atan().to_degrees();
    Some([(right / 90.0) as f32, (-away / 90.0) as f32])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stylus::{PenKind, Router, Stream};

    /// A tablet with 8192 pressure levels and orientation in tenths of a
    /// degree.
    fn axes() -> Axes {
        let axis = |max, units, resolution| wintab::Axis {
            min: 0,
            max,
            units,
            resolution,
        };
        Axes {
            pressure: Some(axis(8191, wintab::Units::None, 0.0)),
            orientation: Some([
                axis(3600, wintab::Units::Circle, 3600.0),
                axis(900, wintab::Units::Circle, 3600.0),
                axis(3600, wintab::Units::Circle, 3600.0),
            ]),
        }
    }

    fn packet(time: u32, tip: bool, x: i32, pressure: u32) -> Packet {
        Packet {
            time,
            buttons: wintab::Buttons(u32::from(tip)),
            x,
            y: 4000,
            normal_pressure: pressure,
            orientation: wintab::Orientation {
                azimuth: 900,
                altitude: 450,
                twist: 1800,
            },
        }
    }

    #[test]
    fn a_lifted_pen_has_no_pressure_and_a_pressed_one_its_share() {
        assert_eq!(touching(0, false, None), None);
        assert_eq!(touching(0, true, None), Some(0.0), "touching, lightly");
        assert!((touching(512, true, None).unwrap() - 0.5).abs() < 0.01);
        assert!((touching(8191, true, None).unwrap() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn orientation_becomes_a_lean_to_the_right_and_toward_the_person() {
        let near = |lean: Option<[f32; 2]>, x: f32, y: f32| {
            let [a, b] = lean.unwrap();
            assert!((a - x).abs() < 1e-4 && (b - y).abs() < 1e-4, "{a} {b}");
        };
        near(lean(0.0, 90.0), 0.0, 0.0);
        // Its top points right, half way down: it leans right.
        near(lean(90.0, 45.0), 0.5, 0.0);
        near(lean(270.0, 45.0), -0.5, 0.0);
        // Its top points away from the person: it leans away.
        near(lean(0.0, 45.0), 0.0, -0.5);
        near(lean(180.0, 45.0), 0.0, 0.5);
        // Turned over (the eraser end) it leans the same way.
        near(lean(90.0, -45.0), 0.5, 0.0);
        // A driver without the reading says 0 and 0: no lean, not flat.
        assert_eq!(lean(0.0, 0.0), None);
        assert_eq!(lean(f64::NAN, 45.0), None);
    }

    #[test]
    fn every_packet_is_a_reading_at_the_drivers_own_time() {
        let mut clock = DeviceClock::default();
        // Five packets 5 ms apart, drained together at 2 s: hovering,
        // then down, pressing harder, lighter, and up.
        let packets = [
            packet(70_000, false, 100, 0),
            packet(70_005, true, 110, 2048),
            packet(70_010, true, 120, 8191),
            packet(70_015, true, 130, 1024),
            packet(70_020, false, 140, 0),
        ];
        let readings = readings(&packets, axes(), &mut clock, 2.0);
        assert_eq!(readings.len(), packets.len(), "one for each packet");
        let pressures: Vec<_> = readings.iter().map(|reading| reading.pressure).collect();
        assert_eq!(pressures[0], None);
        assert!((pressures[1].unwrap() - 0.25).abs() < 1e-3);
        assert_eq!(pressures[2], Some(1.0));
        assert!((pressures[3].unwrap() - 0.125).abs() < 1e-3);
        assert_eq!(pressures[4], None);
        for (index, reading) in readings.iter().enumerate() {
            let expected = 2.0 - 0.005 * (4 - index) as f64;
            assert!((reading.time - expected).abs() < 1e-9, "{index}");
            assert_eq!(reading.source, PenSource::WinTab);
            assert_eq!(reading.at, Some([100.0 + 10.0 * index as f64, 4000.0]));
            let [x, y] = reading.tilt.unwrap();
            assert!((x - 0.5).abs() < 1e-4 && y.abs() < 1e-4);
            assert_eq!(reading.twist, Some(180.0));
        }
        // A tablet that reports no orientation: no lean and no twist.
        let plain = Axes {
            orientation: None,
            ..axes()
        };
        let bare = super::readings(&packets, plain, &mut clock, 2.0);
        assert!(bare.iter().all(|r| r.tilt.is_none() && r.twist.is_none()));
        assert!(super::readings(&[], plain, &mut clock, 2.0).is_empty());
    }

    #[test]
    fn a_scripted_packet_list_comes_out_as_the_same_sample_list() {
        let mut clock = DeviceClock::default();
        let mut router = Router::default();
        let mut stream = Stream::new();
        // The pointer is at 100, 50 when the frame begins.
        router.moved([100.0, 50.0], 1.9);
        router.flush(&mut stream, &[]);
        stream.take();
        // One frame: five packets, while the window got two pointer
        // moves (the system merged the rest), the press and the release.
        let packets = [
            packet(70_000, false, 100, 0),
            packet(70_005, true, 110, 2048),
            packet(70_010, true, 120, 8191),
            packet(70_015, true, 130, 1024),
            packet(70_020, false, 140, 0),
        ];
        router.moved([110.0, 50.0], 2.0);
        router.button(true, true, 2.0);
        router.moved([140.0, 50.0], 2.0);
        router.button(true, false, 2.0);
        router.flush(&mut stream, &readings(&packets, axes(), &mut clock, 2.0));
        let samples = stream.take();
        assert_eq!(samples.len(), packets.len(), "one sample for each packet");
        let kinds: Vec<_> = samples.iter().map(|sample| sample.kind).collect();
        assert_eq!(
            kinds,
            [
                PenKind::Move,
                PenKind::Down,
                PenKind::Move,
                PenKind::Move,
                PenKind::Up
            ]
        );
        // Each where the pen had travelled to, at the driver's time,
        // with its own pressure.
        let xs: Vec<f32> = samples.iter().map(|sample| sample.pos[0]).collect();
        assert_eq!(xs, [100.0, 110.0, 120.0, 130.0, 140.0]);
        for pair in samples.windows(2) {
            assert!((pair[1].time - pair[0].time - 0.005).abs() < 1e-9);
        }
        let pressures: Vec<_> = samples
            .iter()
            .map(|sample| sample.pressure.map(|value| (value * 1000.0).round()))
            .collect();
        assert_eq!(
            pressures,
            [None, Some(250.0), Some(1000.0), Some(125.0), None]
        );
        // The up carries its place and nothing else.
        assert_eq!(samples[4].tilt, None);
        assert_eq!(samples[4].twist, None);
        assert_eq!(samples[3].twist, Some(180.0));
        // The older readers: the pressure the tip had as it lifted stays
        // for this frame, and each pointer event has the reading of the
        // packet made where it is.
        assert!((stream.pressure().unwrap() - 0.125).abs() < 1e-3);
        let paired = crate::stylus::tests_paired(&mut stream);
        let read: Vec<_> = paired
            .iter()
            .map(|sample| sample.pressure.map(|value| (value * 1000.0).round()))
            .collect();
        assert_eq!(read, [Some(250.0), Some(250.0), None, Some(125.0)]);
    }
}
