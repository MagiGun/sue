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
//!
//! A dialog belongs to the window whose frame showed it: the main window,
//! or one of Sue's other windows (`crate::windows`). It is drawn in that
//! window's frames, that window draws again when the dialog has input,
//! and it closes when a frame of that window no longer shows it.

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
    /// What the window was last made or changed to: a frame that asks
    /// for something else (a dialog as tall as what it now holds, a new
    /// title) changes the window to it.
    builder: ViewportBuilder,
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
    /// The viewport whose frame showed each one.
    parents: HashMap<ViewportId, ViewportId>,
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
            viewports.parents.insert(id, viewport.ids.parent);
            let owner = viewports.owner(id);
            match viewports.children.get_mut(&id) {
                Some(child) => {
                    // egui hands an immediate viewport's builder over as
                    // it is; telling what changed since the window was
                    // made is the backend's part. Without it a dialog
                    // stayed the size of its first frame, before it knew
                    // how tall its body is, and scrolled what it holds.
                    let (commands, _) = child.builder.patch(viewport.builder.clone());
                    if !commands.is_empty() {
                        let mut info = egui::ViewportInfo::default();
                        egui_winit::process_viewport_commands(
                            ctx,
                            &mut info,
                            commands,
                            &child.window,
                            &mut Vec::new(),
                        );
                    }
                    let mut raw = child.state.take_egui_input(&child.window);
                    // A Wayland pen over this window (tablet-v2 takes it
                    // away from the mouse).
                    #[cfg(any(
                        target_os = "linux",
                        target_os = "freebsd",
                        target_os = "openbsd",
                        target_os = "netbsd",
                        target_os = "dragonfly"
                    ))]
                    raw.events.extend(crate::wayland_pen::events_for(
                        &child.window,
                        egui_winit::pixels_per_point(ctx, &child.window),
                        ctx.input(|input| input.modifiers),
                    ));
                    let info = raw.viewports.entry(id).or_default();
                    egui_winit::update_viewport_info(info, ctx, &child.window, false);
                    if std::mem::take(&mut child.close) {
                        info.events.push(egui::ViewportEvent::Close);
                        // One more frame of its window closes it if it goes.
                        ctx.request_repaint_of(owner);
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
            // Its own title bar moves, resizes, minimizes and maximizes it.
            if let Some(own) = output.viewport_output.get(&id) {
                let mut info = egui::ViewportInfo::default();
                egui_winit::process_viewport_commands(
                    ctx,
                    &mut info,
                    own.commands.iter().cloned(),
                    &child.window,
                    &mut Vec::new(),
                );
                if info.events.contains(&egui::ViewportEvent::Close) {
                    child.close = true;
                    ctx.request_repaint_of(id);
                }
            }
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
    /// The window whose frames draw viewport `id`: the main window's or
    /// one of Sue's other windows, past any dialog that showed it.
    pub(crate) fn owner(&self, id: ViewportId) -> ViewportId {
        let mut at = id;
        // A chain longer than every viewport known is a loop: stop.
        for _ in 0..=self.parents.len() {
            match self.parents.get(&at) {
                Some(parent) if *parent != at => at = *parent,
                _ => return at,
            }
        }
        at
    }

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
                    builder,
                },
            );
            opened = true;
        }
        opened
    }

    /// Hands a window's event to its viewport, and says which window's
    /// frame draws it (to draw again). `None` when the window is not a
    /// viewport's.
    pub(crate) fn window_event(
        &mut self,
        gpu: &GpuContext,
        window_id: WindowId,
        event: &WindowEvent,
    ) -> Option<ViewportId> {
        let id = self
            .children
            .iter()
            .find(|(_, child)| child.window.id() == window_id)
            .map(|(id, _)| *id)?;
        let owner = self.owner(id);
        let child = self.children.get_mut(&id)?;
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
        Some(owner)
    }

    /// Paints what every viewport drew in a frame of window `ended` into
    /// its window, and closes the windows of that window's viewports the
    /// frame no longer showed. Another window's dialogs stay.
    pub(crate) fn paint(
        &mut self,
        gpu: &GpuContext,
        painter: &mut EguiPainter,
        ctx: &egui::Context,
        clear: egui::Color32,
        ended: ViewportId,
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
        self.forget_unseen(ended);
    }

    /// Lets go of every viewport of window `closed`, which closed.
    pub(crate) fn forget_window(&mut self, closed: ViewportId) {
        let shown: Vec<ViewportId> = self
            .seen
            .iter()
            .copied()
            .filter(|id| self.owner(*id) == closed)
            .collect();
        for id in shown {
            self.seen.remove(&id);
        }
        self.forget_unseen(closed);
    }

    /// Lets go of the viewports of window `ended` its frame did not show.
    fn forget_unseen(&mut self, ended: ViewportId) {
        let gone: Vec<ViewportId> = self
            .children
            .keys()
            .chain(self.wanted.keys())
            .copied()
            .filter(|id| !self.seen.contains(id) && self.owner(*id) == ended)
            .collect();
        for id in &gone {
            self.children.remove(id);
            self.wanted.remove(id);
        }
        let shown: Vec<ViewportId> = self
            .seen
            .iter()
            .copied()
            .filter(|id| self.owner(*id) == ended)
            .collect();
        for id in shown {
            self.seen.remove(&id);
        }
        let Self {
            children,
            wanted,
            seen,
            parents,
            ..
        } = self;
        parents.retain(|id, _| {
            children.contains_key(id) || wanted.contains_key(id) || seen.contains(id)
        });
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
        assert_eq!(viewports.owner(id), ViewportId::ROOT);
    }

    #[test]
    fn a_windows_frame_closes_only_its_own_dialogs() {
        let mut viewports = Viewports::default();
        let second = ViewportId::from_hash_of("second window");
        let main_dialog = ViewportId::from_hash_of("main dialog");
        let second_dialog = ViewportId::from_hash_of("second dialog");
        let nested = ViewportId::from_hash_of("a dialog's dialog");
        for (id, parent) in [
            (main_dialog, ViewportId::ROOT),
            (second_dialog, second),
            (nested, second_dialog),
        ] {
            viewports.wanted.insert(id, ViewportBuilder::default());
            viewports.parents.insert(id, parent);
        }
        assert_eq!(viewports.owner(nested), second, "past the dialog");
        // The second window's frame showed its dialog but not the nested
        // one: the nested one goes, the main window's stays.
        viewports.seen.insert(second_dialog);
        viewports.forget_unseen(second);
        assert!(viewports.wanted.contains_key(&second_dialog));
        assert!(!viewports.wanted.contains_key(&nested));
        assert!(viewports.wanted.contains_key(&main_dialog));
        assert!(viewports.seen.is_empty());
        // The main window's frame showed nothing: its dialog goes, the
        // second window's stays.
        viewports.forget_unseen(ViewportId::ROOT);
        assert!(!viewports.wanted.contains_key(&main_dialog));
        assert!(viewports.wanted.contains_key(&second_dialog));
        assert_eq!(viewports.parents.len(), 1);
        // The second window closes: its dialog goes with it.
        viewports.seen.insert(second_dialog);
        viewports.forget_window(second);
        assert!(viewports.wanted.is_empty());
        assert!(viewports.parents.is_empty());
    }
}
