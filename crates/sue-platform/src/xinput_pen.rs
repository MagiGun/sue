//! Pen tablets on Linux and the BSDs, through X11's XInput2.
//!
//! Under X11, and under Wayland through XWayland (see `crate::tablets` for
//! when Sue opens there with a pen tablet connected), a pen is an
//! XInput2 device that also moves the pointer: its motion, tip and barrel
//! buttons reach Sue as the window's mouse events, as before. What those
//! leave out (pressure, tilt and which end is in use) comes from the
//! pen's own device here, read on an X11 connection of Sue's own through
//! the pure Rust `x11rb` (no `unsafe`, no extra permissions): raw XInput2
//! events, selected on the root window, which arrive whatever window the
//! pen is over and while a button is held. Its queue is drained every
//! frame and the latest state goes to [`crate::stylus`].

use winit::event_loop::EventLoopBuilder;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xinput::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{self, ConnectionExt as _};
use x11rb::rust_connection::RustConnection;

use crate::stylus::{self, PenSource};
use crate::tablets::{self, DisplayServer};

/// Picks the display server before the event loop opens: Wayland as winit
/// prefers, where Sue reads a pen itself (`crate::wayland_pen`), but
/// XWayland while a pen tablet is connected and the compositor lacks the
/// tablet-v2 protocol; or what `SUE_DISPLAY_SERVER` asks for.
pub(crate) fn choose_display_server(builder: &mut EventLoopBuilder<()>) {
    use winit::platform::wayland::EventLoopBuilderExtWayland;
    use winit::platform::x11::EventLoopBuilderExtX11;
    let set = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    let wayland = set("WAYLAND_DISPLAY") || set("WAYLAND_SOCKET");
    let x11 = set("DISPLAY");
    let choice = std::env::var(tablets::CHOICE_VARIABLE).ok();
    // The device list is read, and the compositor asked, only when they
    // can change the answer.
    let pen = wayland && x11 && pen_connected();
    let server = tablets::display_server(
        wayland,
        x11,
        pen,
        crate::wayland_pen::compositor_has_tablets,
        choice.as_deref(),
    );
    match server {
        Some(DisplayServer::X11) => {
            if wayland && choice.is_none() {
                eprintln!(
                    "sue: this Wayland compositor does not pass pen tablets on (no tablet-v2), \
                     so Sue opens through XWayland for the pen ({}=wayland keeps Wayland)",
                    tablets::CHOICE_VARIABLE
                );
            }
            builder.with_x11();
        }
        Some(DisplayServer::Wayland) => {
            builder.with_wayland();
        }
        None => {}
    }
}

/// `true` when the kernel lists a pen tablet (Linux; the BSDs have no such
/// list and keep Wayland).
fn pen_connected() -> bool {
    std::fs::read_to_string("/proc/bus/input/devices")
        .is_ok_and(|devices| tablets::lists_pen(&devices, usize::BITS))
}

/// One axis of a device: its valuator's number, range and resolution
/// (for tilt, units per radian when the driver says), and its value when
/// the device was listed.
#[derive(Clone, Copy, Debug)]
struct Axis {
    number: u16,
    min: f64,
    max: f64,
    resolution: u32,
    value: f64,
}

impl Axis {
    /// Its value as a lean from -1 to 1 (see [`tablets::lean`]).
    fn lean(self, value: f64) -> f32 {
        tablets::lean(value, self.min, self.max, self.resolution)
    }
}

/// A pen's XInput2 device.
#[derive(Debug)]
struct Pen {
    id: xinput::DeviceId,
    pressure: Axis,
    tilt: Option<[Axis; 2]>,
    eraser: bool,
    /// The last values seen: raw events carry only the axes that changed.
    last_pressure: f32,
    last_tilt: [f32; 2],
    /// Whether the tip is down, once its button has been seen.
    tip: Option<bool>,
}

/// The atoms XInput2 labels pen axes with.
#[derive(Clone, Copy, Debug)]
struct Labels {
    pressure: xproto::Atom,
    tilt_x: xproto::Atom,
    tilt_y: xproto::Atom,
}

/// An X11 connection watching the pens' devices.
pub(crate) struct Tablet {
    connection: RustConnection,
    labels: Labels,
    pens: Vec<Pen>,
}

impl std::fmt::Debug for Tablet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tablet")
            .field("pens", &self.pens)
            .finish_non_exhaustive()
    }
}

impl Tablet {
    /// Watches the pens, when Sue's window is an X11 one and the server
    /// has XInput 2.
    pub(crate) fn open(window: &Window) -> Option<Self> {
        match window.window_handle().ok()?.as_raw() {
            RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_) => {}
            _ => return None,
        }
        let (connection, screen) = x11rb::connect(None).ok()?;
        let root = connection.setup().roots.get(screen)?.root;
        let version = connection
            .xinput_xi_query_version(2, 2)
            .ok()?
            .reply()
            .ok()?;
        if version.major_version < 2 {
            return None;
        }
        let atom = |name: &[u8]| -> Option<xproto::Atom> {
            Some(connection.intern_atom(false, name).ok()?.reply().ok()?.atom)
        };
        let labels = Labels {
            pressure: atom(b"Abs Pressure")?,
            tilt_x: atom(b"Abs Tilt X")?,
            tilt_y: atom(b"Abs Tilt Y")?,
        };
        // Every device's raw motion and buttons, and word of devices
        // coming, going or changing (XWayland adds a pen's devices when it
        // first comes near).
        let mask = xinput::XIEventMask::RAW_MOTION
            | xinput::XIEventMask::RAW_BUTTON_PRESS
            | xinput::XIEventMask::RAW_BUTTON_RELEASE
            | xinput::XIEventMask::HIERARCHY
            | xinput::XIEventMask::DEVICE_CHANGED;
        connection
            .xinput_xi_select_events(
                root,
                &[xinput::EventMask {
                    deviceid: ALL_DEVICES,
                    mask: vec![mask],
                }],
            )
            .ok()?;
        connection.flush().ok()?;
        let mut tablet = Self {
            connection,
            labels,
            pens: Vec::new(),
        };
        tablet.scan();
        Some(tablet)
    }

    /// Lists the devices with a pressure axis again.
    fn scan(&mut self) {
        let Some(reply) = self
            .connection
            .xinput_xi_query_device(ALL_DEVICES)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
        else {
            return;
        };
        let previous = std::mem::take(&mut self.pens);
        for info in reply.infos {
            if info.type_ != xinput::DeviceType::SLAVE_POINTER
                && info.type_ != xinput::DeviceType::FLOATING_SLAVE
            {
                continue;
            }
            let mut pressure = None;
            let mut tilt = [None, None];
            let mut touch = false;
            for class in &info.classes {
                match &class.data {
                    xinput::DeviceClassData::Valuator(valuator) => {
                        let axis = Axis {
                            number: valuator.number,
                            min: tablets::fixed(valuator.min.integral, valuator.min.frac),
                            max: tablets::fixed(valuator.max.integral, valuator.max.frac),
                            resolution: valuator.resolution,
                            value: tablets::fixed(valuator.value.integral, valuator.value.frac),
                        };
                        if valuator.label == self.labels.pressure {
                            pressure = Some(axis);
                        } else if valuator.label == self.labels.tilt_x {
                            tilt[0] = Some(axis);
                        } else if valuator.label == self.labels.tilt_y {
                            tilt[1] = Some(axis);
                        }
                    }
                    xinput::DeviceClassData::Touch(_) => touch = true,
                    _ => {}
                }
            }
            // Touch screens report finger pressure; they are not pens.
            let (Some(pressure), false) = (pressure, touch) else {
                continue;
            };
            let kept = previous.iter().find(|pen| pen.id == info.deviceid);
            let tilt = tilt[0].zip(tilt[1]).map(|(x, y)| [x, y]);
            // A pen seen for the first time starts from its lean now (raw
            // events carry only the axes that change); not from its
            // pressure, which a driver may leave where the last stroke
            // ended and would read as a touch.
            self.pens.push(Pen {
                id: info.deviceid,
                pressure,
                tilt,
                eraser: tablets::is_eraser(&String::from_utf8_lossy(&info.name)),
                last_pressure: kept.map_or(0.0, |pen| pen.last_pressure),
                last_tilt: kept.map_or_else(
                    || tilt.map_or([0.0; 2], |[x, y]| [x.lean(x.value), y.lean(y.value)]),
                    |pen| pen.last_tilt,
                ),
                tip: kept.and_then(|pen| pen.tip),
            });
        }
    }

    /// Takes the events since the last frame. `false` once the X server
    /// has gone, and the tablet with it.
    pub(crate) fn poll(&mut self) -> bool {
        let mut latest = None;
        loop {
            match self.connection.poll_for_event() {
                Ok(Some(event)) => match event {
                    Event::XinputRawMotion(raw) => latest = self.take(&raw, None).or(latest),
                    Event::XinputRawButtonPress(raw) if raw.detail == 1 => {
                        latest = self.take(&raw, Some(true)).or(latest);
                    }
                    Event::XinputRawButtonRelease(raw) if raw.detail == 1 => {
                        latest = self.take(&raw, Some(false)).or(latest);
                    }
                    // At once, so the new pen's next events are read. A
                    // master switching between its mouse and its pen
                    // changes nothing here.
                    Event::XinputHierarchy(_) => self.scan(),
                    Event::XinputDeviceChanged(changed)
                        if changed.reason == xinput::ChangeReason::DEVICE_CHANGE =>
                    {
                        self.scan();
                    }
                    _ => {}
                },
                Ok(None) => break,
                Err(_) => return false,
            }
        }
        if let Some(id) = latest
            && let Some(pen) = self.pens.iter().find(|pen| pen.id == id)
        {
            stylus::set(
                PenSource::XInput,
                tablets::contact(pen.last_pressure, pen.tip),
            );
            stylus::set_tool(pen.tilt.map(|_| pen.last_tilt), pen.eraser);
        }
        true
    }

    /// Takes one raw event of a pen (`tip` for its tip's button), and says
    /// which pen it was.
    fn take(
        &mut self,
        raw: &xinput::RawButtonPressEvent,
        tip: Option<bool>,
    ) -> Option<xinput::DeviceId> {
        let pen = self
            .pens
            .iter_mut()
            .find(|pen| pen.id == raw.deviceid || pen.id == raw.sourceid)?;
        let values: Vec<f64> = raw
            .axisvalues
            .iter()
            .map(|value| tablets::fixed(value.integral, value.frac))
            .collect();
        let read = |axis: Axis| tablets::valuator(&raw.valuator_mask, &values, axis.number);
        if let Some(value) = read(pen.pressure) {
            pen.last_pressure = tablets::share(value, pen.pressure.min, pen.pressure.max);
        }
        if let Some([x, y]) = pen.tilt {
            if let Some(value) = read(x) {
                pen.last_tilt[0] = x.lean(value);
            }
            if let Some(value) = read(y) {
                pen.last_tilt[1] = y.lean(value);
            }
        }
        if tip.is_some() {
            pen.tip = tip;
        }
        Some(pen.id)
    }
}

/// XInput2's `XIAllDevices`.
const ALL_DEVICES: xinput::DeviceId = 0;
