//! Pen pressure and input from a graphics tablet on Wayland. winit hands a
//! pen to the window as a mouse only when the compositor emulates it, and
//! KWin does not: a client that speaks the tablet protocol gets tablet
//! events instead of pointer ones, so the pen would neither press nor draw.
//! The compositor's protocol is listened to on the window's connection (see
//! `sue-wayland-pen`) and turned back into the window events egui reads,
//! with the pressure going to [`crate::stylus`] as WinTab's does on Windows.

use std::sync::{Arc, Mutex, Once};

use winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};
use winit::raw_window_handle::HasDisplayHandle;
use winit::window::Window;

use sue_wayland_pen::Event;

use crate::stylus::{self, PenSource};

/// What the pen did since the host last read it (`apply`).
static EVENTS: Mutex<Vec<Event>> = Mutex::new(Vec::new());

/// Starts listening, once, when the window is on a Wayland display.
pub(crate) fn start(window: &Arc<Window>) {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let Ok(display) = window.display_handle() else {
            return;
        };
        let redraw = Arc::clone(window);
        sue_wayland_pen::start(display, move |event| {
            if let Event::Pressure(pressure) = event {
                stylus::set(PenSource::Wayland, pressure);
            }
            EVENTS.lock().unwrap().push(event);
            // The pen arrives on its own thread, so nothing else would wake
            // the loop for it.
            redraw.request_redraw();
        });
    });
}

/// Turns what the pen did since the last frame into the window events egui
/// reads. KWin sends no pointer events for this pen, so these stand in for
/// them.
pub(crate) fn apply(egui: &mut egui_winit::State, window: &Window) {
    let events = std::mem::take(&mut *EVENTS.lock().unwrap());
    for event in events {
        let device_id = DeviceId::dummy();
        let event = match event {
            Event::Move { x, y } => {
                // winit reports pointer positions in physical pixels and the
                // protocol gives surface-local (logical) ones.
                let position =
                    winit::dpi::LogicalPosition::new(x, y).to_physical(window.scale_factor());
                WindowEvent::CursorMoved {
                    device_id,
                    position,
                }
            }
            Event::Down => WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                device_id,
            },
            Event::Up => WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                device_id,
            },
            Event::Leave => WindowEvent::CursorLeft { device_id },
            Event::Pressure(_) => continue,
        };
        let _ = egui.on_window_event(window, &event);
    }
}
