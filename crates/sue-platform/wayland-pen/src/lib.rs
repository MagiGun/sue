//! Pen pressure on Wayland. winit gives a graphics tablet's pen to the
//! window as a plain mouse, so the pressure never arrives. This crate
//! listens to the compositor's `zwp_tablet_v2` protocol on winit's own
//! connection and reports the pen's pressure while its tip touches.
#![cfg(target_os = "linux")]

use raw_window_handle::{DisplayHandle, RawDisplayHandle};
use wayland_backend::client::Backend;
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, event_created_child,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_registry::WlRegistry, wl_seat::WlSeat},
};
use wayland_protocols::wp::tablet::zv2::client::{
    zwp_tablet_manager_v2::ZwpTabletManagerV2,
    zwp_tablet_pad_group_v2::{self, ZwpTabletPadGroupV2},
    zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2,
    zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2,
    zwp_tablet_pad_v2::{self, ZwpTabletPadV2},
    zwp_tablet_seat_v2::{self, ZwpTabletSeatV2},
    zwp_tablet_tool_v2::{self, ZwpTabletToolV2},
    zwp_tablet_v2::ZwpTabletV2,
};

/// One thing the pen did, for the host to turn into the window's input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// The pen's position on the surface, in surface-local (logical) pixels.
    Move { x: f64, y: f64 },
    /// The tip touched the surface.
    Down,
    /// The tip lifted.
    Up,
    /// The pen left the tablet's range above this surface.
    Leave,
    /// The pen's pressure, 0 to 1, while its tip touches; `None` otherwise.
    Pressure(Option<f32>),
}

type Report = Box<dyn Fn(Event) + Send>;

struct State {
    report: Report,
    x: f64,
    y: f64,
    down: bool,
    pressure: f32,
    inside: bool,
    sent_position: Option<(f64, f64)>,
    sent_down: bool,
    sent_pressure: Option<f32>,
}

impl State {
    /// Reports what changed since the last frame, in the order the host must
    /// apply it: position, then the tip, then the pressure.
    fn flush(&mut self) {
        if !self.inside {
            if self.sent_down {
                self.sent_down = false;
                (self.report)(Event::Up);
            }
            if self.sent_position.is_some() {
                self.sent_position = None;
                (self.report)(Event::Leave);
            }
            if self.sent_pressure.is_some() {
                self.sent_pressure = None;
                (self.report)(Event::Pressure(None));
            }
            return;
        }
        if self.sent_position != Some((self.x, self.y)) {
            self.sent_position = Some((self.x, self.y));
            (self.report)(Event::Move {
                x: self.x,
                y: self.y,
            });
        }
        if self.sent_down != self.down {
            self.sent_down = self.down;
            (self.report)(if self.down { Event::Down } else { Event::Up });
        }
        let pressure = self.down.then_some(self.pressure);
        if pressure != self.sent_pressure {
            self.sent_pressure = pressure;
            (self.report)(Event::Pressure(pressure));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn state() -> (State, Arc<Mutex<Vec<Event>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let report = Arc::clone(&seen);
        let state = State {
            report: Box::new(move |event| report.lock().unwrap().push(event)),
            x: 0.0,
            y: 0.0,
            down: false,
            pressure: 0.0,
            inside: false,
            sent_position: None,
            sent_down: false,
            sent_pressure: None,
        };
        (state, seen)
    }

    #[test]
    fn a_stroke_reports_position_tip_and_pressure_in_order() {
        let (mut state, seen) = state();
        state.inside = true;
        state.x = 10.0;
        state.y = 20.0;
        state.down = true;
        state.pressure = 0.5;
        state.flush();
        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                Event::Move { x: 10.0, y: 20.0 },
                Event::Down,
                Event::Pressure(Some(0.5)),
            ]
        );
        seen.lock().unwrap().clear();
        state.flush();
        assert!(
            seen.lock().unwrap().is_empty(),
            "nothing changed, nothing said"
        );
        state.down = false;
        state.flush();
        assert_eq!(
            *seen.lock().unwrap(),
            vec![Event::Up, Event::Pressure(None)]
        );
        seen.lock().unwrap().clear();
        state.inside = false;
        state.flush();
        assert_eq!(*seen.lock().unwrap(), vec![Event::Leave]);
    }
}

macro_rules! ignore {
    ($($ty:ty),*) => {$(
        impl Dispatch<$ty, ()> for State {
            fn event(
                _: &mut Self,
                _: &$ty,
                _: <$ty as Proxy>::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        }
    )*};
}
ignore!(
    WlSeat,
    ZwpTabletManagerV2,
    ZwpTabletV2,
    ZwpTabletPadRingV2,
    ZwpTabletPadStripV2
);

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpTabletSeatV2, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwpTabletSeatV2,
        _: zwp_tablet_seat_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
    event_created_child!(State, ZwpTabletSeatV2, [
        zwp_tablet_seat_v2::EVT_TABLET_ADDED_OPCODE => (ZwpTabletV2, ()),
        zwp_tablet_seat_v2::EVT_TOOL_ADDED_OPCODE => (ZwpTabletToolV2, ()),
        zwp_tablet_seat_v2::EVT_PAD_ADDED_OPCODE => (ZwpTabletPadV2, ()),
    ]);
}

impl Dispatch<ZwpTabletPadV2, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwpTabletPadV2,
        _: zwp_tablet_pad_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
    event_created_child!(State, ZwpTabletPadV2, [
        zwp_tablet_pad_v2::EVT_GROUP_OPCODE => (ZwpTabletPadGroupV2, ()),
    ]);
}

impl Dispatch<ZwpTabletPadGroupV2, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwpTabletPadGroupV2,
        _: zwp_tablet_pad_group_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
    event_created_child!(State, ZwpTabletPadGroupV2, [
        zwp_tablet_pad_group_v2::EVT_RING_OPCODE => (ZwpTabletPadRingV2, ()),
        zwp_tablet_pad_group_v2::EVT_STRIP_OPCODE => (ZwpTabletPadStripV2, ()),
    ]);
}

impl Dispatch<ZwpTabletToolV2, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZwpTabletToolV2,
        event: zwp_tablet_tool_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwp_tablet_tool_v2::Event::{
            Down, Frame, Motion, Pressure, ProximityIn, ProximityOut, Up,
        };
        match event {
            ProximityIn { .. } => state.inside = true,
            ProximityOut => {
                state.inside = false;
                state.down = false;
                state.pressure = 0.0;
            }
            Motion { x, y } => {
                state.x = x;
                state.y = y;
            }
            Down { .. } => state.down = true,
            Up => {
                state.down = false;
                state.pressure = 0.0;
            }
            // The protocol's range is 0 to 65535.
            Pressure { pressure } => state.pressure = pressure as f32 / 65535.0,
            // A frame groups the events that happened together.
            Frame { .. } => state.flush(),
            _ => {}
        }
    }
}

/// Starts listening to the pen on the connection `display` belongs to and
/// calls `report` (from another thread) with what the pen does there. Does
/// nothing on a display that is not Wayland (X11 and XWayland), or when the
/// compositor offers no tablet protocol.
///
/// The listener lives as long as the display does, which for a window's
/// event loop is until the program ends.
pub fn start(display: DisplayHandle<'_>, report: impl Fn(Event) + Send + 'static) {
    let RawDisplayHandle::Wayland(wayland) = display.as_raw() else {
        return;
    };
    #[allow(unsafe_code)]
    // SAFETY: the pointer is the live `wl_display` of the window's event
    // loop, which `display` borrows for this call. The listener runs on a
    // thread that is only used while that loop runs, which in Sue is the
    // whole life of the program; the loop is never dropped before exit.
    let backend = unsafe { Backend::from_foreign_display(wayland.display.as_ptr().cast()) };
    let connection = Connection::from_backend(backend);
    let Ok((globals, mut queue)) = registry_queue_init::<State>(&connection) else {
        return;
    };
    let handle = queue.handle();
    // Version 1 of the protocol is enough for pressure and keeps the
    // newer pad dials out of the way.
    let Ok(seat) = globals.bind::<WlSeat, _, _>(&handle, 1..=1, ()) else {
        return;
    };
    let Ok(manager) = globals.bind::<ZwpTabletManagerV2, _, _>(&handle, 1..=1, ()) else {
        return;
    };
    let _tablet_seat = manager.get_tablet_seat(&seat, &handle, ());
    let mut state = State {
        report: Box::new(report),
        x: 0.0,
        y: 0.0,
        down: false,
        pressure: 0.0,
        inside: false,
        sent_position: None,
        sent_down: false,
        sent_pressure: None,
    };
    std::thread::spawn(move || {
        let _keep = (connection, manager, seat, _tablet_seat);
        while queue.blocking_dispatch(&mut state).is_ok() {}
    });
}
