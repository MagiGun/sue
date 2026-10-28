//! The native desktop host: one `winit` window with a `wgpu` surface, an
//! event-driven loop that sleeps at 0 FPS, and translation of OS input into
//! [`InputEvent`]s.
//!
//! Pacing is decided in one place: every time the loop is about to wait it
//! asks the application for its [`FramePacing`]. While anything animates the
//! loop requests the next redraw itself (paced by vsync, or by a timer for a
//! lower frame-rate cap), so animations run to the end without any input.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseScrollDelta, StartCause, WindowEvent as OsWindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as OsKey, ModifiersState, NamedKey as OsNamedKey};
use winit::window::{CursorIcon, Window, WindowId};

use crate::gpu::{GpuContext, GpuPreference, WindowSurface};
use crate::{
    FileDrop, FormFactor, HardwareCapabilities, InputEvent, Key, KeyEvent, Modifiers, MouseButton,
    MouseEvent, NamedKey, OsFamily, PlatformError, PlatformHost, PlatformPaths, Point,
    SafeAreaInsets, SurfaceDescriptor, SurfaceId, SurfaceInfo, WindowEvent,
};

/// How the application wants its next frames scheduled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramePacing {
    /// Nothing changes: block until the next input event (0 FPS).
    Sleep,
    /// Nothing moves, but something changes at this instant (a caret blink,
    /// audio to feed, a message to clear): block until then, then call
    /// [`DesktopApp::timer`].
    WakeAt(Instant),
    /// Keep rendering, at most `target_hz` frames per second.
    Animate {
        /// Frame-rate cap.
        target_hz: u32,
    },
}

/// Pointer shapes the application can ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CursorShape {
    /// The arrow.
    Default,
    /// A hand, over clickable things.
    Pointer,
    /// An I-beam, over text.
    Text,
    /// An open hand, over draggable things.
    Grab,
    /// A closed hand, while dragging.
    Grabbing,
    /// Horizontal resize, over sliders.
    ResizeHorizontal,
}

/// Window options.
#[derive(Clone, Debug, PartialEq)]
pub struct DesktopOptions {
    /// Window title.
    pub title: String,
    /// Initial width in logical pixels.
    pub width: f32,
    /// Initial height in logical pixels.
    pub height: f32,
    /// Pixels scrolled per wheel notch (from rice).
    pub wheel_line_px: f32,
    /// Which GPU draws the window (from rice).
    pub gpu: GpuPreference,
}

/// An application driven by the desktop host.
pub trait DesktopApp {
    /// Called once the window and GPU are ready.
    ///
    /// # Errors
    /// A message that aborts start-up.
    fn start(&mut self, host: &mut DesktopHost) -> Result<(), String>;

    /// Handles one input event. Returns `true` if the scene must be redrawn.
    fn input(&mut self, host: &mut DesktopHost, event: InputEvent) -> bool;

    /// A [`FramePacing::WakeAt`] deadline passed. Returns `true` if the scene
    /// must be redrawn.
    fn timer(&mut self, host: &mut DesktopHost) -> bool;

    /// Draws one frame (see [`DesktopHost::acquire_frame`]).
    fn frame(&mut self, host: &mut DesktopHost);

    /// How to pace what comes next. Asked every time the event loop is about
    /// to wait, so it is the single source of truth for frame scheduling.
    fn pacing(&mut self, now: Instant) -> FramePacing;

    /// The user asked to close the window. Returns `true` to close now, or
    /// `false` to keep the window open (for example to ask first).
    fn close_requested(&mut self, host: &mut DesktopHost) -> bool;

    /// The loop is about to end: save what must survive.
    fn exit(&mut self, host: &mut DesktopHost) {
        let _ = host;
    }
}

/// The native desktop [`PlatformHost`]: a single main window with a GPU surface.
pub struct DesktopHost {
    window: Arc<Window>,
    gpu: GpuContext,
    surface: WindowSurface,
    os: OsFamily,
    form_factor: FormFactor,
    capabilities: HardwareCapabilities,
    paths: PlatformPaths,
    info: SurfaceInfo,
    events: VecDeque<InputEvent>,
    modifiers: Modifiers,
    cursor: Point,
    wheel_line_px: f32,
    exit_requested: bool,
}

/// The id of the main window surface.
pub const MAIN_SURFACE: SurfaceId = SurfaceId(1);

impl DesktopHost {
    fn create(
        event_loop: &ActiveEventLoop,
        options: &DesktopOptions,
    ) -> Result<Self, PlatformError> {
        let attributes = Window::default_attributes()
            .with_title(options.title.clone())
            .with_inner_size(LogicalSize::new(options.width, options.height))
            .with_min_inner_size(LogicalSize::new(360.0, 480.0));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|error| PlatformError::Window(error.to_string()))?,
        );
        let descriptor = wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(
            event_loop.owned_display_handle(),
        ));
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance
            .create_surface(Arc::clone(&window))
            .map_err(|error| PlatformError::Gpu(error.to_string()))?;
        let gpu = pollster::block_on(GpuContext::with_instance(
            instance,
            Some(&surface),
            options.gpu,
        ))?;
        let size = window.inner_size();
        let surface = WindowSurface::new(surface, &gpu, size.width, size.height)?;

        let os = OsFamily::current();
        let paths = PlatformPaths::detect()?;
        let refresh_hz = window
            .current_monitor()
            .and_then(|monitor| monitor.refresh_rate_millihertz())
            .map_or(HardwareCapabilities::DEFAULT_REFRESH_HZ, |millihertz| {
                millihertz.div_ceil(1000)
            });
        let capabilities = gpu.describe(HardwareCapabilities {
            display_refresh_hz: refresh_hz,
            ..HardwareCapabilities::probe_portable()
        });
        let scale = window.scale_factor() as f32;
        let (width, height) = (size.width as f32 / scale, size.height as f32 / scale);
        let form_factor = FormFactor::classify(os, width.min(height));
        Ok(Self {
            info: SurfaceInfo {
                id: MAIN_SURFACE,
                title: options.title.clone(),
                width,
                height,
                scale_factor: scale,
                safe_area: SafeAreaInsets::ZERO,
            },
            window,
            gpu,
            surface,
            os,
            form_factor,
            capabilities,
            paths,
            events: VecDeque::new(),
            modifiers: Modifiers::NONE,
            cursor: Point::default(),
            wheel_line_px: options.wheel_line_px,
            exit_requested: false,
        })
    }

    /// The GPU device and queue.
    #[must_use]
    pub fn gpu(&self) -> &GpuContext {
        &self.gpu
    }

    /// The swap-chain texture format.
    #[must_use]
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface.format()
    }

    /// Size of the swap chain in physical pixels.
    #[must_use]
    pub fn physical_size(&self) -> (u32, u32) {
        self.surface.size()
    }

    /// The main window: logical size, scale factor and safe area.
    #[must_use]
    pub fn window_info(&self) -> &SurfaceInfo {
        &self.info
    }

    /// The next swap-chain texture, or `None` to skip this frame.
    pub fn acquire_frame(&mut self) -> Option<wgpu::SurfaceTexture> {
        self.surface.acquire(&self.gpu)
    }

    /// Presents a finished frame.
    pub fn present(&self, frame: wgpu::SurfaceTexture) {
        self.window.pre_present_notify();
        self.gpu.queue.present(frame);
    }

    /// Asks the OS for a pointer shape.
    pub fn set_cursor(&self, shape: CursorShape) {
        let icon = match shape {
            CursorShape::Default => CursorIcon::Default,
            CursorShape::Pointer => CursorIcon::Pointer,
            CursorShape::Text => CursorIcon::Text,
            CursorShape::Grab => CursorIcon::Grab,
            CursorShape::Grabbing => CursorIcon::Grabbing,
            CursorShape::ResizeHorizontal => CursorIcon::EwResize,
        };
        self.window.set_cursor(icon);
    }

    /// Asks the event loop to quit after this callback.
    pub fn request_exit(&mut self) {
        self.exit_requested = true;
    }

    /// Updates the pixels scrolled per wheel notch.
    pub fn set_wheel_line_px(&mut self, pixels: f32) {
        self.wheel_line_px = pixels;
    }

    fn resized(&mut self, width: u32, height: u32) -> InputEvent {
        self.surface.resize(&self.gpu, width, height);
        let scale = self.window.scale_factor() as f32;
        self.info.scale_factor = scale;
        self.info.width = width as f32 / scale;
        self.info.height = height as f32 / scale;
        InputEvent::Window(WindowEvent::Resized {
            width: self.info.width,
            height: self.info.height,
            scale_factor: scale,
        })
    }

    /// Translates one OS window event; `None` for events Sue does not use.
    fn translate(&mut self, event: &OsWindowEvent) -> Option<InputEvent> {
        let scale = self.info.scale_factor.max(f32::EPSILON);
        Some(match event {
            OsWindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                if *is_synthetic {
                    return None;
                }
                let pressed = event.state == ElementState::Pressed;
                let text = event
                    .text
                    .as_ref()
                    .filter(|_| pressed)
                    .map(|text| text.to_string())
                    .filter(|text| !text.chars().any(char::is_control));
                let key = map_key(&event.logical_key)
                    .or_else(|| text.as_ref()?.chars().next().map(Key::Character))?;
                InputEvent::Keyboard(KeyEvent {
                    key,
                    pressed,
                    modifiers: self.modifiers,
                    text,
                })
            }
            OsWindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = map_modifiers(modifiers.state());
                return None;
            }
            OsWindowEvent::CursorMoved { position, .. } => {
                self.cursor = Point::new(position.x as f32 / scale, position.y as f32 / scale);
                InputEvent::Mouse(MouseEvent::Moved {
                    position: self.cursor,
                })
            }
            OsWindowEvent::CursorLeft { .. } => InputEvent::Mouse(MouseEvent::Left),
            OsWindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    winit::event::MouseButton::Left => MouseButton::Left,
                    winit::event::MouseButton::Right => MouseButton::Right,
                    winit::event::MouseButton::Middle => MouseButton::Middle,
                    winit::event::MouseButton::Back => MouseButton::Back,
                    winit::event::MouseButton::Forward => MouseButton::Forward,
                    winit::event::MouseButton::Other(_) => return None,
                };
                InputEvent::Mouse(MouseEvent::Button {
                    button,
                    pressed: *state == ElementState::Pressed,
                    position: self.cursor,
                })
            }
            OsWindowEvent::MouseWheel { delta, .. } => {
                let (delta_x, delta_y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        (x * self.wheel_line_px, y * self.wheel_line_px)
                    }
                    MouseScrollDelta::PixelDelta(position) => {
                        (position.x as f32 / scale, position.y as f32 / scale)
                    }
                };
                InputEvent::Mouse(MouseEvent::Wheel { delta_x, delta_y })
            }
            OsWindowEvent::HoveredFile(path) => {
                InputEvent::FileDrop(FileDrop::Hovered(path.clone()))
            }
            OsWindowEvent::HoveredFileCancelled => InputEvent::FileDrop(FileDrop::Cancelled),
            OsWindowEvent::DroppedFile(path) => {
                InputEvent::FileDrop(FileDrop::Dropped(path.clone()))
            }
            OsWindowEvent::Focused(focused) => InputEvent::Window(WindowEvent::Focused(*focused)),
            _ => return None,
        })
    }
}

impl PlatformHost for DesktopHost {
    fn os(&self) -> OsFamily {
        self.os
    }

    fn form_factor(&self) -> FormFactor {
        self.form_factor
    }

    fn capabilities(&self) -> &HardwareCapabilities {
        &self.capabilities
    }

    fn paths(&self) -> &PlatformPaths {
        &self.paths
    }

    fn create_surface(
        &mut self,
        _descriptor: &SurfaceDescriptor,
    ) -> Result<SurfaceInfo, PlatformError> {
        Err(PlatformError::Unsupported(
            "the desktop host drives a single main window",
        ))
    }

    fn resize_surface(
        &mut self,
        id: SurfaceId,
        width: f32,
        height: f32,
    ) -> Result<SurfaceInfo, PlatformError> {
        if id != MAIN_SURFACE {
            return Err(PlatformError::UnknownSurface(id));
        }
        if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
            return Err(PlatformError::InvalidSurface(
                "size must be positive and finite",
            ));
        }
        // The OS may refuse or adjust the size; `Resized` reports the outcome.
        let _ = self
            .window
            .request_inner_size(LogicalSize::new(width, height));
        Ok(self.info.clone())
    }

    fn destroy_surface(&mut self, id: SurfaceId) -> Result<(), PlatformError> {
        if id != MAIN_SURFACE {
            return Err(PlatformError::UnknownSurface(id));
        }
        self.exit_requested = true;
        Ok(())
    }

    fn surface(&self, id: SurfaceId) -> Option<&SurfaceInfo> {
        (id == MAIN_SURFACE).then_some(&self.info)
    }

    fn poll_events(&mut self) -> Vec<InputEvent> {
        self.events.drain(..).collect()
    }
}

/// Opens the main window and runs `app` until the window closes. Rendering is
/// event-driven: with [`FramePacing::Sleep`] the loop blocks in
/// [`ControlFlow::Wait`] and draws nothing until input arrives (LAW 4).
///
/// # Errors
/// Window, GPU or application start-up failures.
pub fn run_desktop<A: DesktopApp>(app: A, options: DesktopOptions) -> Result<(), PlatformError> {
    let event_loop = EventLoop::new().map_err(|error| PlatformError::Window(error.to_string()))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut runner = Runner {
        app,
        options,
        host: None,
        error: None,
        last_frame: None,
        exiting: false,
    };
    event_loop
        .run_app(&mut runner)
        .map_err(|error| PlatformError::Window(error.to_string()))?;
    runner.error.map_or(Ok(()), Err)
}

struct Runner<A> {
    app: A,
    options: DesktopOptions,
    host: Option<DesktopHost>,
    error: Option<PlatformError>,
    last_frame: Option<Instant>,
    exiting: bool,
}

/// What the loop does next for one pacing decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Block until input.
    Wait,
    /// Block until input or this instant.
    WaitUntil(Instant),
    /// Request a redraw now, then block until it arrives.
    Redraw,
    /// Run the application's timer now.
    Timer,
}

/// Turns a pacing decision into a loop step. Uncapped animation is paced by
/// vsync (presenting blocks), so it redraws at once; a lower cap waits for
/// the next frame slot after `last_frame`.
fn plan_step(
    pacing: FramePacing,
    now: Instant,
    last_frame: Option<Instant>,
    display_hz: u32,
) -> Step {
    match pacing {
        FramePacing::Sleep => Step::Wait,
        FramePacing::WakeAt(at) if at <= now => Step::Timer,
        FramePacing::WakeAt(at) => Step::WaitUntil(at),
        FramePacing::Animate { target_hz } => {
            let interval = Duration::from_secs_f64(1.0 / f64::from(target_hz.max(1)));
            match last_frame.map(|last| last + interval) {
                Some(next) if target_hz < display_hz && next > now => Step::WaitUntil(next),
                _ => Step::Redraw,
            }
        }
    }
}

impl<A: DesktopApp> Runner<A> {
    fn finish(&mut self, event_loop: &ActiveEventLoop) {
        if !self.exiting {
            self.exiting = true;
            if let Some(host) = self.host.as_mut() {
                self.app.exit(host);
            }
        }
        event_loop.exit();
    }
}

impl<A: DesktopApp> ApplicationHandler for Runner<A> {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, _cause: StartCause) {}

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.exiting {
            return;
        }
        let Some(host) = self.host.as_mut() else {
            return;
        };
        let display_hz = host.capabilities.display_refresh_hz;
        let mut step = plan_step(
            self.app.pacing(Instant::now()),
            Instant::now(),
            self.last_frame,
            display_hz,
        );
        if step == Step::Timer {
            if self.app.timer(host) {
                host.window.request_redraw();
            }
            step = match plan_step(
                self.app.pacing(Instant::now()),
                Instant::now(),
                self.last_frame,
                display_hz,
            ) {
                // A deadline that is still due would spin the loop: retry soon.
                Step::Timer => Step::WaitUntil(Instant::now() + Duration::from_millis(1)),
                other => other,
            };
        }
        match step {
            Step::Wait | Step::Timer => event_loop.set_control_flow(ControlFlow::Wait),
            Step::WaitUntil(at) => event_loop.set_control_flow(ControlFlow::WaitUntil(at)),
            Step::Redraw => {
                host.window.request_redraw();
                event_loop.set_control_flow(ControlFlow::Wait);
            }
        }
        if host.exit_requested {
            self.finish(event_loop);
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.host.is_some() {
            return;
        }
        match DesktopHost::create(event_loop, &self.options) {
            Ok(mut host) => match self.app.start(&mut host) {
                Ok(()) => {
                    host.window.request_redraw();
                    self.host = Some(host);
                }
                Err(reason) => {
                    self.error = Some(PlatformError::App(reason));
                    event_loop.exit();
                }
            },
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: OsWindowEvent,
    ) {
        if self.exiting {
            return;
        }
        let Some(host) = self.host.as_mut() else {
            return;
        };
        let translated = match &event {
            OsWindowEvent::CloseRequested => {
                if self.app.close_requested(host) {
                    self.finish(event_loop);
                } else {
                    host.window.request_redraw();
                }
                return;
            }
            OsWindowEvent::RedrawRequested => {
                self.last_frame = Some(Instant::now());
                self.app.frame(host);
                if host.exit_requested {
                    self.finish(event_loop);
                }
                return;
            }
            OsWindowEvent::Resized(size) => Some(host.resized(size.width, size.height)),
            other => host.translate(other),
        };
        if let Some(input) = translated {
            host.events.push_back(input.clone());
            if self.app.input(host, input) {
                host.window.request_redraw();
            }
        }
        if host.exit_requested {
            self.finish(event_loop);
        }
    }
}

fn map_modifiers(state: ModifiersState) -> Modifiers {
    Modifiers {
        ctrl: state.control_key(),
        alt: state.alt_key(),
        shift: state.shift_key(),
        super_key: state.super_key(),
    }
}

/// Maps an OS key to a Sue key; `None` for keys Sue does not name.
fn map_key(key: &OsKey) -> Option<Key> {
    match key {
        OsKey::Character(text) => {
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) => Some(Key::Character(ch)),
                _ => None,
            }
        }
        OsKey::Named(named) => map_named(*named).map(Key::Named),
        _ => None,
    }
}

fn map_named(named: OsNamedKey) -> Option<NamedKey> {
    Some(match named {
        OsNamedKey::Space => NamedKey::Space,
        OsNamedKey::Enter => NamedKey::Enter,
        OsNamedKey::Escape => NamedKey::Escape,
        OsNamedKey::Tab => NamedKey::Tab,
        OsNamedKey::Backspace => NamedKey::Backspace,
        OsNamedKey::Delete => NamedKey::Delete,
        OsNamedKey::ArrowUp => NamedKey::ArrowUp,
        OsNamedKey::ArrowDown => NamedKey::ArrowDown,
        OsNamedKey::ArrowLeft => NamedKey::ArrowLeft,
        OsNamedKey::ArrowRight => NamedKey::ArrowRight,
        OsNamedKey::Home => NamedKey::Home,
        OsNamedKey::End => NamedKey::End,
        OsNamedKey::PageUp => NamedKey::PageUp,
        OsNamedKey::PageDown => NamedKey::PageDown,
        OsNamedKey::F1 => NamedKey::F(1),
        OsNamedKey::F2 => NamedKey::F(2),
        OsNamedKey::F3 => NamedKey::F(3),
        OsNamedKey::F4 => NamedKey::F(4),
        OsNamedKey::F5 => NamedKey::F(5),
        OsNamedKey::F6 => NamedKey::F(6),
        OsNamedKey::F7 => NamedKey::F(7),
        OsNamedKey::F8 => NamedKey::F(8),
        OsNamedKey::F9 => NamedKey::F(9),
        OsNamedKey::F10 => NamedKey::F(10),
        OsNamedKey::F11 => NamedKey::F(11),
        OsNamedKey::F12 => NamedKey::F(12),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_map_to_sue_keys() {
        assert_eq!(
            map_key(&OsKey::Character("a".into())),
            Some(Key::Character('a'))
        );
        assert_eq!(map_key(&OsKey::Character("ab".into())), None);
        assert_eq!(
            map_key(&OsKey::Named(OsNamedKey::Enter)),
            Some(Key::Named(NamedKey::Enter))
        );
        assert_eq!(
            map_key(&OsKey::Named(OsNamedKey::F11)),
            Some(Key::Named(NamedKey::F(11)))
        );
        assert_eq!(map_key(&OsKey::Named(OsNamedKey::CapsLock)), None);
    }

    #[test]
    fn pacing_turns_into_loop_steps() {
        let now = Instant::now();
        assert_eq!(plan_step(FramePacing::Sleep, now, None, 60), Step::Wait);
        let later = now + Duration::from_millis(500);
        assert_eq!(
            plan_step(FramePacing::WakeAt(later), now, None, 60),
            Step::WaitUntil(later)
        );
        assert_eq!(
            plan_step(FramePacing::WakeAt(now), now, None, 60),
            Step::Timer
        );
        // Uncapped animation asks for the next frame at once: vsync paces it.
        let uncapped = FramePacing::Animate { target_hz: 144 };
        assert_eq!(plan_step(uncapped, now, Some(now), 144), Step::Redraw);
        // A 30 Hz cap on a 144 Hz display waits for the next frame slot.
        let capped = FramePacing::Animate { target_hz: 30 };
        assert!(matches!(
            plan_step(capped, now, Some(now), 144),
            Step::WaitUntil(at) if at > now
        ));
        let slot = now + Duration::from_millis(40);
        assert_eq!(plan_step(capped, slot, Some(now), 144), Step::Redraw);
    }

    #[test]
    fn modifiers_map_one_to_one() {
        let state = ModifiersState::CONTROL | ModifiersState::SHIFT;
        let mapped = map_modifiers(state);
        assert!(mapped.ctrl && mapped.shift && !mapped.alt && !mapped.super_key);
    }
}
