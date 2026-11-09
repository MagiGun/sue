//! Pen tablets under Wayland, through the tablet-v2 protocol, which winit
//! 0.30 does not speak.
//!
//! A compositor sends a pen's events to the client that owns the surface
//! under it, so they have to be read on winit's own connection: the one
//! audited call in `sue-wayland` joins it (backbone rule 6), and the rest
//! is wayland-client's safe API. Sue's events are read on an event queue
//! of their own, never winit's. winit reads the socket as always; Sue reads
//! it too each frame, but only when `poll` says bytes are waiting, so a
//! read never blocks (libwayland would wait on any other reader of the
//! display, such as the Vulkan driver's presenting thread, until it read).
//! While the loop sleeps a watcher thread waits on the socket (`poll`
//! alone) and wakes the loop when something arrives: winit drops a wake
//! that brought it nothing of its own, which a pen's events alone are
//! (found on a Huion Kamvas 22 Plus under KDE Plasma 6 by mahesh-labs).
//!
//! A pen over the main window becomes its pointer (moving, the tip as the
//! primary button, the barrel buttons) whether or not the compositor also
//! turns it into a mouse, and its pressure, tilt and eraser go to
//! [`crate::stylus`], with one sample for each of its pointer events, so
//! every point of a stroke has its own pressure. Each global is bound at
//! the lower of the version the compositor offers and the one Sue's
//! protocol tables know, never higher.
//!
//! The connection must go before winit's event loop does: the host drops
//! the tablet when the loop says it is exiting and leaks it if it is still
//! there afterwards (see `sue_wayland::join`).

use std::io::Write as _;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::JoinHandle;

use rustix::event::{PollFd, PollFlags, Timespec};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
use wayland_protocols::wp::cursor_shape::v1::client::{
    wp_cursor_shape_device_v1::{Shape, WpCursorShapeDeviceV1},
    wp_cursor_shape_manager_v1::WpCursorShapeManagerV1,
};
use wayland_protocols::wp::tablet::zv2::client::{
    zwp_tablet_manager_v2::ZwpTabletManagerV2,
    zwp_tablet_pad_dial_v2::ZwpTabletPadDialV2,
    zwp_tablet_pad_group_v2::{self, ZwpTabletPadGroupV2},
    zwp_tablet_pad_ring_v2::ZwpTabletPadRingV2,
    zwp_tablet_pad_strip_v2::ZwpTabletPadStripV2,
    zwp_tablet_pad_v2::{self, ZwpTabletPadV2},
    zwp_tablet_seat_v2::{self, ZwpTabletSeatV2},
    zwp_tablet_tool_v2::{self, ZwpTabletToolV2},
    zwp_tablet_v2::{self, ZwpTabletV2},
};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::stylus::{self, PenSource, Sample, SampleKind};
use crate::tablets::{PenEvent, PenPointer, PenTrack, Socket, ToolFrame, Watch, Watched};

/// `true` when the session's compositor offers tablet-v2, asked on a
/// short connection of its own before Sue's window opens.
pub(crate) fn compositor_has_tablets() -> bool {
    let Ok(connection) = Connection::connect_to_env() else {
        return false;
    };
    let mut queue = connection.new_event_queue();
    let _registry = connection.display().get_registry(&queue.handle(), ());
    let mut probe = Probe(false);
    queue.roundtrip(&mut probe).is_ok() && probe.0
}

/// What the probe found: whether tablet-v2 is offered.
struct Probe(bool);

impl Dispatch<wl_registry::WlRegistry, ()> for Probe {
    fn event(
        state: &mut Self,
        _: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { interface, .. } = event
            && interface == ZwpTabletManagerV2::interface().name
        {
            state.0 = true;
        }
    }
}

/// A key for a surface: its libwayland object's address.
fn surface_key(window: &Window) -> Option<usize> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Wayland(handle) => Some(handle.surface.as_ptr() as usize),
        _ => None,
    }
}

/// The lower of the version a compositor offers and the one Sue knows.
fn version(offered: u32, known: u32) -> u32 {
    offered.min(known)
}

/// The pens on Sue's Wayland connection.
pub(crate) struct Tablet {
    /// What wakes the loop while it sleeps: `None` until it first sleeps,
    /// then the watcher, unless its thread could not start.
    waker: Option<Option<Waker>>,
    connection: Connection,
    queue: EventQueue<State>,
    state: State,
    /// The main window's surface.
    main: usize,
    /// Its pointer events, until a frame takes them.
    pending: Vec<PenEvent>,
    /// The pressure a tip had as it lifted, until a frame has drawn it.
    lift: Option<f32>,
    /// The cursor Sue shows for the mouse over the main window.
    cursor: egui::CursorIcon,
}

impl std::fmt::Debug for Tablet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tablet")
            .field("tools", &self.state.tools.len())
            .finish_non_exhaustive()
    }
}

/// One pen (or eraser, or other tool) the compositor told of.
struct Tool {
    proxy: ZwpTabletToolV2,
    frame: ToolFrame,
    track: PenTrack,
    /// Its cursor through cursor-shape-v1, made the first time it is near.
    shape_device: Option<WpCursorShapeDeviceV1>,
    /// While it is near: the serial of its `proximity_in` (which every
    /// cursor request names), whether it is over the main window, and the
    /// cursor it was last given.
    near: Option<Near>,
}

/// A tool in range of one of Sue's surfaces.
struct Near {
    serial: u32,
    main: bool,
    shown: Option<egui::CursorIcon>,
}

/// What the queue's events build up.
#[derive(Default)]
struct State {
    seats: Vec<wl_seat::WlSeat>,
    manager: Option<ZwpTabletManagerV2>,
    /// Named cursors (cursor-shape-v1), when the compositor offers them.
    shapes: Option<WpCursorShapeManagerV1>,
    /// The main window's surface key, to tell its tools from the others'.
    main: usize,
    tools: Vec<Tool>,
    /// Pointer events since the last poll, with their surfaces.
    out: Vec<PenEvent>,
    /// The tool that last finished a frame.
    active: Option<usize>,
    /// The pressure of a tip that lifted since the last poll.
    lifted: Option<f32>,
}

impl State {
    fn tool(&mut self, proxy: &ZwpTabletToolV2) -> Option<usize> {
        self.tools.iter().position(|tool| tool.proxy == *proxy)
    }

    /// Asks for a seat's tablets once both it and the manager are bound.
    fn tablet_seat(&self, seat: &wl_seat::WlSeat, queue: &QueueHandle<Self>) {
        if let Some(manager) = &self.manager {
            let _ = manager.get_tablet_seat(seat, queue, ());
        }
    }
}

impl Tablet {
    /// Joins the window's Wayland connection and asks for its pens; `None`
    /// when the window is not a Wayland one.
    pub(crate) fn open(window: &Window) -> Option<Self> {
        let main = surface_key(window)?;
        let connection = sue_wayland::join(window.display_handle().ok()?)?;
        let queue = connection.new_event_queue();
        let _registry = connection.display().get_registry(&queue.handle(), ());
        // Sent now; the answer comes with the socket's next read.
        connection.flush().ok()?;
        Some(Self {
            waker: None,
            connection,
            queue,
            state: State {
                main,
                ..State::default()
            },
            main,
            pending: Vec::new(),
            lift: None,
            cursor: egui::CursorIcon::Default,
        })
    }

    /// The cursor Sue shows for the mouse over the main window (egui's
    /// cursor of the last frame): pens near it show the same.
    pub(crate) fn set_cursor(&mut self, icon: egui::CursorIcon) {
        self.cursor = icon;
        self.show_cursors();
    }

    /// Gives every tool near one of Sue's surfaces its cursor, when it
    /// changed: over the main window the mouse's, elsewhere the default.
    /// Through cursor-shape-v1 where the compositor offers it; without it,
    /// the compositor's own tablet cursor stays.
    fn show_cursors(&mut self) {
        let Some(shapes) = self.state.shapes.clone() else {
            return;
        };
        let handle = self.queue.handle();
        let mut sent = false;
        for tool in &mut self.state.tools {
            let Some(near) = tool.near.as_mut() else {
                continue;
            };
            let icon = if near.main {
                self.cursor
            } else {
                egui::CursorIcon::Default
            };
            if near.shown == Some(icon) {
                continue;
            }
            near.shown = Some(icon);
            sent = true;
            match cursor_shape(icon) {
                // No cursor: a null surface hides it.
                None => tool.proxy.set_cursor(near.serial, None, 0, 0),
                Some(shape) => tool
                    .shape_device
                    .get_or_insert_with(|| shapes.get_tablet_tool_v2(&tool.proxy, &handle, ()))
                    .set_shape(near.serial, shape),
            }
        }
        if sent {
            let _ = self.connection.flush();
        }
    }

    /// Handles the pens' events that arrived, without waiting for more,
    /// and refreshes the stylus state. `Some(true)` when there were any
    /// (draw a frame), `None` once the connection has failed (drop the
    /// tablet).
    pub(crate) fn poll(&mut self) -> Option<bool> {
        let mut handled = self.queue.dispatch_pending(&mut self.state).ok()?;
        // What reached the socket since winit last read it, without ever
        // waiting: a read is only begun when `poll` says bytes are there
        // (libwayland's read would otherwise wait for every other reader
        // of the display, such as the Vulkan driver's, to read too).
        // A guard dropped unread cancels the read.
        if let Some(guard) = self.queue.prepare_read()
            && readable(guard.connection_fd())
        {
            // A failure shows in the dispatch that follows.
            let _ = guard.read();
            handled += self.queue.dispatch_pending(&mut self.state).ok()?;
        }
        // A tool that just came near gets its cursor.
        self.show_cursors();
        // Requests made while handling them (binding, destroying).
        let _ = self.connection.flush();
        for event in self.state.out.drain(..) {
            if event.surface == self.main {
                self.pending.push(event);
            } else {
                others(event.surface, event.pointer);
            }
        }
        if let Some(pressure) = self.state.lifted.take() {
            self.lift = Some(pressure);
        }
        self.publish();
        Some(handled > 0)
    }

    /// The loop is about to sleep: the watcher (started the first time)
    /// wakes it through `proxy` when the socket has something to read.
    pub(crate) fn arm(&mut self, proxy: &winit::event_loop::EventLoopProxy<()>) {
        let connection = &self.connection;
        let waker = self.waker.get_or_insert_with(|| {
            // A socket of its own (the same connection), so the thread
            // holds nothing of the connection's and never outlives it.
            let socket = connection.as_fd().try_clone_to_owned().ok()?;
            let proxy = proxy.clone();
            Waker::spawn(socket, move || proxy.send_event(()).is_ok())
        });
        if let Some(waker) = waker {
            waker.arm();
        }
    }

    /// The main window's pointer events since the last frame, as egui
    /// events (`scale` egui points per logical pixel); each one's reading
    /// goes to [`crate::stylus`] as a sample, in the same order.
    pub(crate) fn take_events(
        &mut self,
        scale: f32,
        modifiers: egui::Modifiers,
    ) -> Vec<egui::Event> {
        self.pending
            .drain(..)
            .map(|event| {
                let egui = event.pointer.to_egui(scale, modifiers);
                if let Some(sample) = sample(&egui, event) {
                    stylus::push_sample(sample);
                }
                egui
            })
            .collect()
    }

    /// A frame has drawn the events taken: a lifted tip's pressure, kept
    /// for the stroke's last points, goes.
    pub(crate) fn frame_done(&mut self) {
        if self.lift.take().is_some() {
            self.publish();
        }
    }

    /// Puts the active pen's state in [`crate::stylus`]: while the tip
    /// touches, its pressure; after it lifts, the pressure it had, until a
    /// frame has drawn the stroke's end; then none.
    fn publish(&self) {
        let Some(tool) = self
            .state
            .active
            .and_then(|index| self.state.tools.get(index))
        else {
            return;
        };
        let pressure = if tool.track.down {
            Some(tool.track.pressure)
        } else {
            self.lift
        };
        stylus::set(PenSource::WaylandTablet, pressure);
        stylus::set_tool(tool.track.tilt, tool.track.eraser);
    }
}

/// The sample for the egui event made from `event`; none for a pen going
/// away.
fn sample(egui: &egui::Event, event: PenEvent) -> Option<Sample> {
    let (kind, pos) = match *egui {
        egui::Event::PointerMoved(pos) => (SampleKind::Move, pos),
        egui::Event::PointerButton { pos, pressed, .. } => (
            if pressed {
                SampleKind::Press
            } else {
                SampleKind::Release
            },
            pos,
        ),
        _ => return None,
    };
    Some(Sample {
        kind,
        pos: [pos.x, pos.y],
        pressure: event.reading.pressure,
        tilt: event.reading.tilt,
        eraser: event.reading.eraser,
    })
}

/// `true` when `socket` has bytes to read now (a `poll` that never waits).
fn readable(socket: BorrowedFd<'_>) -> bool {
    let mut fds = [PollFd::new(&socket, PollFlags::IN)];
    let now = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    rustix::event::poll(&mut fds, Some(&now)).is_ok_and(|ready| ready > 0)
        && fds[0].revents().contains(PollFlags::IN)
}

/// The watcher thread that wakes the loop while it sleeps (see
/// [`Watch`]): it waits on the Wayland socket with `poll` only, never
/// reading it or calling libwayland, so it cannot take events from winit
/// or Sue's queue, and costs nothing while nothing arrives.
struct Waker {
    shared: Arc<(Mutex<Watch>, Condvar)>,
    /// Written to when the waker goes, which ends the thread's wait.
    stop: UnixStream,
    thread: Option<JoinHandle<()>>,
}

impl Waker {
    /// Starts the thread on `socket` (a duplicate of the connection's),
    /// waking the loop with `wake` (`false` once the loop has gone).
    fn spawn(socket: OwnedFd, wake: impl Fn() -> bool + Send + 'static) -> Option<Self> {
        let (stop, stopped) = UnixStream::pair().ok()?;
        let shared = Arc::new((Mutex::new(Watch::default()), Condvar::new()));
        let theirs = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("sue-pen-waker".to_owned())
            .spawn(move || watch(&theirs, &socket, &stopped, &wake))
            .ok()?;
        Some(Self {
            shared,
            stop,
            thread: Some(thread),
        })
    }

    fn arm(&self) {
        let (watch, told) = &*self.shared;
        let mut watch = watch.lock().unwrap_or_else(PoisonError::into_inner);
        if watch.arm() {
            told.notify_one();
        }
    }
}

impl Drop for Waker {
    fn drop(&mut self) {
        {
            let (watch, told) = &*self.shared;
            watch.lock().unwrap_or_else(PoisonError::into_inner).stop();
            told.notify_one();
        }
        // Ends a wait on the socket.
        let _ = (&self.stop).write_all(&[1]);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The watcher thread's work: each time the loop sleeps, wait on the
/// socket (or the stop signal) and wake the loop once it is readable.
fn watch(
    shared: &(Mutex<Watch>, Condvar),
    socket: &OwnedFd,
    stopped: &UnixStream,
    wake: &dyn Fn() -> bool,
) {
    let (watch, told) = shared;
    loop {
        let mut state = watch.lock().unwrap_or_else(PoisonError::into_inner);
        let go = loop {
            match state.begin() {
                Some(go) => break go,
                None => state = told.wait(state).unwrap_or_else(PoisonError::into_inner),
            }
        };
        drop(state);
        if !go {
            return;
        }
        let mut fds = [
            PollFd::new(socket, PollFlags::IN),
            PollFd::new(stopped, PollFlags::IN),
        ];
        let waited = rustix::event::poll(&mut fds, None);
        let events = fds[0].revents();
        // Interrupted, it waits again; any other failure would fail again
        // at once, so the socket counts as closed.
        let failed = waited.is_err_and(|error| error != rustix::io::Errno::INTR);
        let socket_did =
            if failed || events.intersects(PollFlags::ERR | PollFlags::HUP | PollFlags::NVAL) {
                Socket::Closed
            } else if events.contains(PollFlags::IN) {
                Socket::Readable
            } else {
                Socket::Quiet
            };
        let stop_signal = !fds[1].revents().is_empty();
        let next = watch
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .end(socket_did, stop_signal);
        match next {
            Watched::Wake => {
                if !wake() {
                    return;
                }
            }
            Watched::Again => {}
            Watched::Stop => return,
        }
    }
}

/// Pointer events for Sue's other windows (dialogs and palettes in
/// windows of their own), until they take them with [`events_for`]; a
/// window that never does keeps only the latest.
static OTHERS: Mutex<Vec<(usize, PenPointer)>> = Mutex::new(Vec::new());

/// How many pointer events wait for other windows at most.
const OTHERS_KEPT: usize = 512;

fn others(surface: usize, event: PenPointer) {
    if let Ok(mut others) = OTHERS.lock() {
        if others.len() >= OTHERS_KEPT {
            others.remove(0);
        }
        others.push((surface, event));
    }
}

/// The pen's pointer events over `window`, one of Sue's other windows,
/// as egui events (`pixels_per_point` its egui scale).
pub(crate) fn events_for(
    window: &Window,
    pixels_per_point: f32,
    modifiers: egui::Modifiers,
) -> Vec<egui::Event> {
    let Some(key) = surface_key(window) else {
        return Vec::new();
    };
    let scale = window.scale_factor() as f32 / pixels_per_point;
    let Ok(mut others) = OTHERS.lock() else {
        return Vec::new();
    };
    let mut events = Vec::new();
    others.retain(|(surface, event)| {
        let mine = *surface == key;
        if mine {
            events.push(event.to_egui(scale, modifiers));
        }
        !mine
    });
    events
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        (): &(),
        _: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version: offered,
        } = event
        else {
            return;
        };
        if interface == wl_seat::WlSeat::interface().name {
            // Only to name the seat to the tablet manager: version 1 does.
            let seat: wl_seat::WlSeat = registry.bind(name, version(offered, 1), queue, ());
            state.tablet_seat(&seat, queue);
            state.seats.push(seat);
        } else if interface == ZwpTabletManagerV2::interface().name && state.manager.is_none() {
            let known = ZwpTabletManagerV2::interface().version;
            state.manager = Some(registry.bind(name, version(offered, known), queue, ()));
            for seat in &state.seats {
                state.tablet_seat(seat, queue);
            }
        } else if interface == WpCursorShapeManagerV1::interface().name && state.shapes.is_none() {
            let known = WpCursorShapeManagerV1::interface().version;
            state.shapes = Some(registry.bind(name, version(offered, known), queue, ()));
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpTabletManagerV2, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwpTabletManagerV2,
        _: <ZwpTabletManagerV2 as Proxy>::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpTabletSeatV2, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZwpTabletSeatV2,
        event: zwp_tablet_seat_v2::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_tablet_seat_v2::Event::ToolAdded { id } = event {
            state.tools.push(Tool {
                proxy: id,
                frame: ToolFrame::default(),
                track: PenTrack::default(),
                shape_device: None,
                near: None,
            });
        }
    }

    wayland_client::event_created_child!(State, ZwpTabletSeatV2, [
        zwp_tablet_seat_v2::EVT_TABLET_ADDED_OPCODE => (ZwpTabletV2, ()),
        zwp_tablet_seat_v2::EVT_TOOL_ADDED_OPCODE => (ZwpTabletToolV2, ()),
        zwp_tablet_seat_v2::EVT_PAD_ADDED_OPCODE => (ZwpTabletPadV2, ()),
    ]);
}

impl Dispatch<ZwpTabletV2, ()> for State {
    fn event(
        _: &mut Self,
        tablet: &ZwpTabletV2,
        event: zwp_tablet_v2::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_tablet_v2::Event::Removed = event {
            tablet.destroy();
        }
    }
}

impl Dispatch<ZwpTabletToolV2, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &ZwpTabletToolV2,
        event: zwp_tablet_tool_v2::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwp_tablet_tool_v2::Event;
        let Some(index) = state.tool(proxy) else {
            return;
        };
        let tool = &mut state.tools[index];
        match event {
            Event::Type {
                tool_type: WEnum::Value(kind),
            } => tool.track.eraser = kind == zwp_tablet_tool_v2::Type::Eraser,
            Event::ProximityIn {
                serial, surface, ..
            } => {
                let key = surface.id().as_ptr() as usize;
                tool.frame.enter = Some(key);
                // Its cursor is set (with this serial) once the events are
                // handled.
                tool.near = Some(Near {
                    serial,
                    main: key == state.main,
                    shown: None,
                });
            }
            Event::ProximityOut => {
                tool.frame.leave = true;
                tool.near = None;
            }
            Event::Down { .. } => tool.frame.down = Some(true),
            Event::Up => tool.frame.down = Some(false),
            Event::Motion { x, y } => tool.frame.position = Some([x as f32, y as f32]),
            Event::Pressure { pressure } => tool.frame.pressure = Some(pressure),
            Event::Tilt { tilt_x, tilt_y } => tool.frame.tilt = Some([tilt_x, tilt_y]),
            Event::Button {
                button,
                state: pressed,
                ..
            } => tool.frame.buttons.push((
                button,
                pressed == WEnum::Value(zwp_tablet_tool_v2::ButtonState::Pressed),
            )),
            Event::Frame { .. } => {
                let frame = std::mem::take(&mut tool.frame);
                let touching = tool.track.down.then_some(tool.track.pressure);
                tool.track.apply(&frame, &mut state.out);
                if !tool.track.down
                    && let Some(pressure) = touching
                {
                    state.lifted = Some(pressure);
                }
                state.active = Some(index);
            }
            Event::Removed => {
                if let Some(device) = tool.shape_device.take() {
                    device.destroy();
                }
                proxy.destroy();
                state.tools.remove(index);
                state.active = None;
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpTabletPadV2, ()> for State {
    fn event(
        _: &mut Self,
        pad: &ZwpTabletPadV2,
        event: zwp_tablet_pad_v2::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_tablet_pad_v2::Event::Removed = event {
            pad.destroy();
        }
    }

    wayland_client::event_created_child!(State, ZwpTabletPadV2, [
        zwp_tablet_pad_v2::EVT_GROUP_OPCODE => (ZwpTabletPadGroupV2, ()),
    ]);
}

impl Dispatch<ZwpTabletPadGroupV2, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwpTabletPadGroupV2,
        _: zwp_tablet_pad_group_v2::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }

    wayland_client::event_created_child!(State, ZwpTabletPadGroupV2, [
        zwp_tablet_pad_group_v2::EVT_RING_OPCODE => (ZwpTabletPadRingV2, ()),
        zwp_tablet_pad_group_v2::EVT_STRIP_OPCODE => (ZwpTabletPadStripV2, ()),
        zwp_tablet_pad_group_v2::EVT_DIAL_OPCODE => (ZwpTabletPadDialV2, ()),
    ]);
}

/// The pad's rings, strips and dials: Sue does not use them yet.
macro_rules! ignored {
    ($($proxy:ty),*) => {$(
        impl Dispatch<$proxy, ()> for State {
            fn event(
                _: &mut Self,
                _: &$proxy,
                _: <$proxy as Proxy>::Event,
                (): &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        }
    )*};
}

ignored!(
    ZwpTabletPadRingV2,
    ZwpTabletPadStripV2,
    ZwpTabletPadDialV2,
    WpCursorShapeManagerV1,
    WpCursorShapeDeviceV1
);

/// The named cursor for egui's cursor `icon` (the CSS names both use);
/// `None` for no cursor at all.
fn cursor_shape(icon: egui::CursorIcon) -> Option<Shape> {
    use egui::CursorIcon as Icon;
    Some(match icon {
        Icon::None => return None,
        Icon::Default => Shape::Default,
        Icon::ContextMenu => Shape::ContextMenu,
        Icon::Help => Shape::Help,
        Icon::PointingHand => Shape::Pointer,
        Icon::Progress => Shape::Progress,
        Icon::Wait => Shape::Wait,
        Icon::Cell => Shape::Cell,
        Icon::Crosshair => Shape::Crosshair,
        Icon::Text => Shape::Text,
        Icon::VerticalText => Shape::VerticalText,
        Icon::Alias => Shape::Alias,
        Icon::Copy => Shape::Copy,
        Icon::Move => Shape::Move,
        Icon::NoDrop => Shape::NoDrop,
        Icon::NotAllowed => Shape::NotAllowed,
        Icon::Grab => Shape::Grab,
        Icon::Grabbing => Shape::Grabbing,
        Icon::AllScroll => Shape::AllScroll,
        Icon::ResizeHorizontal => Shape::EwResize,
        Icon::ResizeNeSw => Shape::NeswResize,
        Icon::ResizeNwSe => Shape::NwseResize,
        Icon::ResizeVertical => Shape::NsResize,
        Icon::ResizeEast => Shape::EResize,
        Icon::ResizeSouthEast => Shape::SeResize,
        Icon::ResizeSouth => Shape::SResize,
        Icon::ResizeSouthWest => Shape::SwResize,
        Icon::ResizeWest => Shape::WResize,
        Icon::ResizeNorthWest => Shape::NwResize,
        Icon::ResizeNorth => Shape::NResize,
        Icon::ResizeNorthEast => Shape::NeResize,
        Icon::ResizeColumn => Shape::ColResize,
        Icon::ResizeRow => Shape::RowResize,
        Icon::ZoomIn => Shape::ZoomIn,
        Icon::ZoomOut => Shape::ZoomOut,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globals_bind_at_the_version_both_sides_know() {
        let known = ZwpTabletManagerV2::interface().version;
        assert_eq!(version(known + 3, known), known, "never above Sue's tables");
        assert_eq!(version(1, known), 1);
    }

    #[test]
    fn each_pen_event_becomes_one_sample_for_its_egui_event() {
        use crate::tablets::Reading;
        let reading = Reading {
            pressure: Some(0.4),
            tilt: Some([0.1, 0.2]),
            eraser: true,
        };
        let modifiers = egui::Modifiers::default();
        let pointers = [
            (PenPointer::Move([2.0, 4.0]), Some(SampleKind::Move)),
            (
                PenPointer::Button {
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    at: [2.0, 4.0],
                },
                Some(SampleKind::Press),
            ),
            (
                PenPointer::Button {
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    at: [2.0, 4.0],
                },
                Some(SampleKind::Release),
            ),
            (PenPointer::Gone, None),
        ];
        for (pointer, kind) in pointers {
            let event = PenEvent {
                surface: 1,
                pointer,
                reading,
            };
            let egui = pointer.to_egui(1.5, modifiers);
            let made = sample(&egui, event);
            assert_eq!(made.map(|sample| sample.kind), kind);
            if let Some(made) = made {
                // At the egui event's own position, so a studio can pair
                // them.
                assert!(made.is_for(made.kind, [3.0, 6.0]));
                assert_eq!(made.pressure, Some(0.4));
                assert_eq!(made.tilt, Some([0.1, 0.2]));
                assert!(made.eraser);
            }
        }
    }

    #[test]
    fn the_waker_wakes_once_per_sleep_and_ends_when_dropped() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::{Duration, Instant};
        // A socket pair stands in for the Wayland socket.
        let (display, compositor) = UnixStream::pair().unwrap();
        // A read is begun only when bytes wait.
        assert!(!readable(display.as_fd()));
        (&compositor).write_all(&[0]).unwrap();
        assert!(readable(display.as_fd()));
        let woken = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&woken);
        let waker = Waker::spawn(display.into(), move || {
            counter.fetch_add(1, Ordering::SeqCst);
            true
        })
        .unwrap();
        let wait_for = |count: usize| {
            let start = Instant::now();
            while woken.load(Ordering::SeqCst) < count && start.elapsed() < Duration::from_secs(5) {
                std::thread::yield_now();
            }
            woken.load(Ordering::SeqCst)
        };
        // Bytes arrived while the loop is awake: no wake until it sleeps.
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(woken.load(Ordering::SeqCst), 0);
        waker.arm();
        assert_eq!(wait_for(1), 1);
        // Woken once for that sleep, however long the bytes stay unread.
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(woken.load(Ordering::SeqCst), 1);
        waker.arm();
        assert_eq!(wait_for(2), 2);
        // Dropping it ends the thread, even mid-watch on a quiet socket.
        let (quiet, _keep) = UnixStream::pair().unwrap();
        let idle = Waker::spawn(quiet.into(), || true).unwrap();
        idle.arm();
        drop(idle);
        drop(waker);
    }

    #[test]
    fn the_pen_shows_the_mouses_cursor() {
        assert_eq!(
            cursor_shape(egui::CursorIcon::Default),
            Some(Shape::Default)
        );
        assert_eq!(
            cursor_shape(egui::CursorIcon::Crosshair),
            Some(Shape::Crosshair)
        );
        assert_eq!(
            cursor_shape(egui::CursorIcon::PointingHand),
            Some(Shape::Pointer)
        );
        assert_eq!(
            cursor_shape(egui::CursorIcon::ResizeHorizontal),
            Some(Shape::EwResize)
        );
        assert_eq!(cursor_shape(egui::CursorIcon::None), None, "hidden");
    }
}
