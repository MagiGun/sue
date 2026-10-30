//! Windows of their own for egui's immediate viewports: a dialog or a
//! floating palette shown with `show_viewport_immediate` opens as a real
//! system window that can be moved anywhere (outside the main window too)
//! and resized.
//!
//! A viewport is drawn while the main window's frame runs, but its window
//! is made between frames (only the event loop can make windows), so a new
//! viewport's first frame draws nowhere and the window appears on the next.
//! What every viewport draws is kept and painted when the frame is done,
//! before the main window, so textures arrive in the order egui made them.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use egui::{ViewportBuilder, ViewportId};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use crate::gpu::{GpuContext, WindowSurface};
use crate::paint::EguiPainter;

/// One viewport's window.
struct Child {
    window: Arc<Window>,
    surface: Option<WindowSurface>,
    state: egui_winit::State,
    /// Its close button was pressed: egui hears of it next frame.
    close: bool,
}

/// What one viewport drew this frame.
struct Drawn {
    id: ViewportId,
    shapes: Vec<egui::epaint::ClippedShape>,
    textures: egui::TexturesDelta,
    pixels_per_point: f32,
}

/// Every viewport window, shared between the host and egui's renderer.
#[derive(Default)]
pub(crate) struct Viewports {
    children: HashMap<ViewportId, Child>,
    /// Viewports shown that have no window yet.
    wanted: HashMap<ViewportId, ViewportBuilder>,
    drawn: Vec<Drawn>,
    /// Viewports shown this frame; the others' windows close.
    seen: HashSet<ViewportId>,
}

pub(crate) type Shared = Rc<RefCell<Viewports>>;

/// Lets `ctx` show immediate viewports as windows of their own.
pub(crate) fn install(ctx: &egui::Context, shared: &Shared) {
    ctx.set_embed_viewports(false);
    let shared = Rc::clone(shared);
    egui::Context::set_immediate_viewport_renderer(move |ctx, viewport| {
        let id = viewport.ids.this;
        let raw = {
            let mut viewports = shared.borrow_mut();
            viewports.seen.insert(id);
            match viewports.children.get_mut(&id) {
                Some(child) => {
                    let mut raw = child.state.take_egui_input(&child.window);
                    let info = raw.viewports.entry(id).or_default();
                    egui_winit::update_viewport_info(info, ctx, &child.window, false);
                    if std::mem::take(&mut child.close) {
                        info.events.push(egui::ViewportEvent::Close);
                        // One more frame closes its window if it goes.
                        ctx.request_repaint_of(egui::ViewportId::ROOT);
                    }
                    raw
                }
                None => {
                    viewports.wanted.insert(id, viewport.builder.clone());
                    // Drawn at the size asked for, to keep egui in step.
                    let size = viewport
                        .builder
                        .inner_size
                        .unwrap_or(egui::vec2(400.0, 300.0));
                    let mut raw = egui::RawInput {
                        viewport_id: id,
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    };
                    raw.viewports.insert(
                        id,
                        egui::ViewportInfo {
                            native_pixels_per_point: Some(ctx.pixels_per_point()),
                            ..Default::default()
                        },
                    );
                    raw
                }
            }
        };
        let mut show = viewport.viewport_ui_cb;
        let output = ctx.run_ui(raw, |ui| show(ui));
        let mut viewports = shared.borrow_mut();
        if let Some(child) = viewports.children.get_mut(&id) {
            child
                .state
                .handle_platform_output(&child.window, output.platform_output);
        }
        viewports.drawn.push(Drawn {
            id,
            shapes: output.shapes,
            textures: output.textures_delta,
            pixels_per_point: output.pixels_per_point,
        });
    });
}

impl Viewports {
    /// Makes the windows of viewports shown without one. True when any
    /// was made (draw again, so it shows).
    pub(crate) fn open_wanted(
        &mut self,
        event_loop: &ActiveEventLoop,
        ctx: &egui::Context,
        gpu: &GpuContext,
        max_texture_side: usize,
    ) -> bool {
        let wanted: Vec<_> = self.wanted.drain().collect();
        let mut opened = false;
        for (id, builder) in wanted {
            if self.children.contains_key(&id) {
                continue;
            }
            let Ok(window) = egui_winit::create_window(ctx, event_loop, &builder) else {
                continue;
            };
            let window = Arc::new(window);
            let size = window.inner_size();
            let surface = gpu
                .instance
                .create_surface(Arc::clone(&window))
                .ok()
                .and_then(|surface| {
                    WindowSurface::new(surface, gpu, size.width.max(1), size.height.max(1)).ok()
                });
            let state = egui_winit::State::new(
                ctx.clone(),
                id,
                event_loop,
                Some(window.scale_factor() as f32),
                window.theme(),
                Some(max_texture_side),
            );
            self.children.insert(
                id,
                Child {
                    window,
                    surface,
                    state,
                    close: false,
                },
            );
            opened = true;
        }
        opened
    }

    /// Hands a window's event to its viewport. False when the window is
    /// not a viewport's (the main window).
    pub(crate) fn window_event(
        &mut self,
        gpu: &GpuContext,
        window_id: WindowId,
        event: &WindowEvent,
    ) -> bool {
        let Some(child) = self
            .children
            .values_mut()
            .find(|child| child.window.id() == window_id)
        else {
            return false;
        };
        let _ = child.state.on_window_event(&child.window, event);
        match event {
            WindowEvent::CloseRequested => child.close = true,
            WindowEvent::Resized(size) => {
                if let Some(surface) = child.surface.as_mut() {
                    surface.resize(gpu, size.width, size.height);
                }
            }
            _ => {}
        }
        true
    }

    /// Paints what every viewport drew this frame into its window, and
    /// closes the windows of viewports no longer shown.
    pub(crate) fn paint(
        &mut self,
        gpu: &GpuContext,
        painter: &mut EguiPainter,
        ctx: &egui::Context,
        clear: egui::Color32,
    ) {
        for drawn in std::mem::take(&mut self.drawn) {
            let target = self.children.get_mut(&drawn.id).and_then(|child| {
                let surface = child.surface.as_mut()?;
                let size = surface.size();
                surface
                    .acquire(gpu)
                    .map(|frame| (frame, size, Arc::clone(&child.window)))
            });
            match target {
                Some((frame, (width, height), window)) => {
                    let view = frame
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    painter.paint(
                        ctx,
                        &view,
                        [width, height],
                        clear,
                        drawn.shapes,
                        drawn.textures,
                        drawn.pixels_per_point,
                    );
                    window.pre_present_notify();
                    gpu.queue.present(frame);
                }
                None => painter.apply_textures(drawn.textures),
            }
        }
        let seen = std::mem::take(&mut self.seen);
        self.children.retain(|id, _| seen.contains(id));
        self.wanted.retain(|id, _| seen.contains(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_viewport_asks_for_its_window_and_still_draws() {
        let ctx = egui::Context::default();
        let shared = Shared::default();
        install(&ctx, &shared);
        assert!(!ctx.embed_viewports(), "viewports get windows of their own");
        let id = ViewportId::from_hash_of("a dialog");
        let mut drew = false;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.ctx().show_viewport_immediate(
                id,
                ViewportBuilder::default().with_inner_size(egui::vec2(320.0, 200.0)),
                |ui, class| {
                    assert!(matches!(class, egui::ViewportClass::Immediate));
                    ui.label("inside");
                    drew = true;
                },
            );
        });
        // No GPU here to take the textures.
        output.textures_delta.clear();
        assert!(drew, "the viewport's own code ran before its window exists");
        let mut viewports = shared.borrow_mut();
        assert!(viewports.wanted.contains_key(&id), "its window is wanted");
        assert!(viewports.seen.contains(&id));
        assert_eq!(viewports.drawn.len(), 1, "what it drew waits to be painted");
        assert!(!viewports.drawn[0].shapes.is_empty());
        viewports.drawn[0].textures.clear();
    }
}
