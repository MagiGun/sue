//! Sue on Wayland: joins the Wayland connection winit opened, so that
//! sue-platform can read pen tablets (the tablet-v2 protocol, which winit
//! 0.30 does not speak) on the same connection, where the compositor sends
//! a pen's events for Sue's own windows, with wayland-client's safe API.
//!
//! [`join`] is an exception to Sue's forbidden `unsafe` (backbone rule 6),
//! approved by the maintainer on 2026-11-03 for this one call. Anything
//! more needs its own approval.
//!
//! On every other system this library is empty.

/// A second handle on the Wayland connection behind `display`, a window's
/// display handle; `None` when it is not a Wayland one.
///
/// The connection is a guest: it has its own event queues and never closes
/// the display. Its owner (winit's event loop) closes it when the loop
/// ends, so **the returned `Connection`, every event queue and every
/// object made from it must be dropped before winit's event loop is**
/// (sue-platform drops its pen tablet when the loop says it is exiting,
/// and leaks it rather than drop it late). Requests may be sent on it, but
/// never `wl_display` requests that end or take over the connection, and
/// its events are only ever read through its own queues, never winit's.
#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
#[allow(
    unsafe_code,
    reason = "joining winit's Wayland connection takes its raw wl_display pointer; approved by the maintainer on 2026-11-03 for this call only (backbone rule 6)"
)]
#[must_use]
pub fn join(display: raw_window_handle::DisplayHandle<'_>) -> Option<wayland_client::Connection> {
    let raw_window_handle::RawDisplayHandle::Wayland(handle) = display.as_raw() else {
        return None;
    };
    // SAFETY: `handle.display` is the `wl_display` winit connected with
    // libwayland and handed out through raw-window-handle; it is non-null
    // and stays valid until winit's event loop drops its own connection.
    // `from_foreign_display` makes a guest backend: it creates its own
    // event queue on that display and never disconnects it, so it does not
    // touch winit's queue or close the display. What it needs from us is
    // that the pointer outlives the backend, its clones and every proxy
    // made from it, which the caller keeps by dropping (or leaking) the
    // connection before the event loop ends, as documented above. Nothing
    // here may ever call `wl_display_disconnect` on it or read events into
    // winit's queue.
    let backend = unsafe {
        wayland_backend::client::Backend::from_foreign_display(handle.display.as_ptr().cast())
    };
    Some(wayland_client::Connection::from_backend(backend))
}
