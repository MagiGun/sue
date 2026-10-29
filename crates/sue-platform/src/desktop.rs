//! The native desktop host: one `winit` window, a `wgpu` surface, and egui
//! (`egui-winit` for input, IME and the clipboard; [`EguiPainter`] for
//! drawing).
//!
//! Rendering is event-driven. After every frame the loop asks two
//! questions: does the application still animate (a spring in motion, which
//! also calls `request_repaint`), and when does egui want its next frame
//! (a hover fade, a caret blink)? With neither, it blocks in
//! [`ControlFlow::Wait`] and draws nothing until input arrives.

use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{StartCause, WindowEvent as OsWindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Icon, Window, WindowId};

use crate::gpu::{GpuContext, GpuPreference, WindowSurface};
use crate::paint::EguiPainter;
use crate::{
    FormFactor, HardwareCapabilities, InputEvent, OsFamily, PlatformError, PlatformHost,
    PlatformPaths, SafeAreaInsets, SurfaceDescriptor, SurfaceId, SurfaceInfo,
};

/// What the application wants from the loop after a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramePacing {
    /// Nothing moves: draw nothing until input arrives (0 FPS).
    Sleep,
    /// Nothing moves, but timed work is due at this instant (an audio queue
    /// to top up, a message to clear, an autosave).
    WakeAt(Instant),
    /// Something moves: keep drawing at up to `target_hz`.
    Animate {
        /// Frame rate cap from the hardware profile.
        target_hz: u32,
    },
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
    /// Which GPU draws the window (from rice).
    pub gpu: GpuPreference,
    /// The window's and the taskbar's icon: square RGBA8 pixels and their
    /// side. On Windows the icon built into the program (its first icon
    /// resource) is used instead when it has one.
    pub icon: Option<(Vec<u8>, u32)>,
    /// The application's id in reverse domain form, such as
    /// `io.github.MagiGun.Sue`. Linux and BSD desktops (Wayland's app id,
    /// X11's `WM_CLASS`) match it to the `.desktop` file of the same name,
    /// which names the icon the window, the taskbar and the switcher show.
    /// Empty leaves it to the system.
    pub app_id: String,
}

/// The icon from the pixels the options give.
fn pixel_icon(options: &DesktopOptions) -> Option<Icon> {
    options
        .icon
        .as_ref()
        .and_then(|(rgba, side)| Icon::from_rgba(rgba.clone(), *side, *side).ok())
}

/// Windows shows two icons: the window icon (the small one, in the title
/// bar) and the taskbar icon (the big one, in the taskbar and Alt+Tab).
/// The window starts hidden and appears once Sue has started and drawn
/// its first frame: the taskbar asks the window for its icon when it
/// appears, gives up on a window too busy to answer (starting the GPU and
/// the engine) and does not ask again, leaving a generic icon.
#[cfg(target_os = "windows")]
fn system_attributes(
    attributes: winit::window::WindowAttributes,
    options: &DesktopOptions,
) -> winit::window::WindowAttributes {
    use winit::platform::windows::WindowAttributesExtWindows;
    attributes
        .with_taskbar_icon(pixel_icon(options))
        .with_visible(false)
}

/// Wayland and X11 desktops find the window's icon through its app id.
#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd"
))]
fn system_attributes(
    attributes: winit::window::WindowAttributes,
    options: &DesktopOptions,
) -> winit::window::WindowAttributes {
    use winit::platform::wayland::WindowAttributesExtWayland;
    if options.app_id.is_empty() {
        attributes
    } else {
        // One name serves both: Wayland's app id and X11's WM_CLASS.
        attributes.with_name(options.app_id.clone(), options.app_id.clone())
    }
}

/// Elsewhere the system takes the icon from the app bundle or package.
#[cfg(not(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd"
)))]
fn system_attributes(
    attributes: winit::window::WindowAttributes,
    _options: &DesktopOptions,
) -> winit::window::WindowAttributes {
    attributes
}

/// The icon built into the program (its first icon resource, which
/// `bins/sue/build.rs` puts there), at the two sizes this window's scale
/// wants: Windows picks the closest size the file holds instead of
/// shrinking one big picture, so both stay sharp. A program without the
/// resource keeps the pixels it was given. The window is still hidden
/// here.
#[cfg(target_os = "windows")]
fn resource_icons(window: &Window) {
    use winit::dpi::PhysicalSize;
    use winit::platform::windows::{IconExtWindows, WindowExtWindows};
    let scale = window.scale_factor();
    let sized = |points: u32| {
        let pixels = (f64::from(points) * scale).round().clamp(16.0, 256.0) as u32;
        PhysicalSize::new(pixels, pixels)
    };
    if let Ok(small) = Icon::from_resource(1, Some(sized(16))) {
        window.set_window_icon(Some(small));
    }
    if let Ok(big) = Icon::from_resource(1, Some(sized(32))) {
        window.set_taskbar_icon(Some(big));
    }
}

/// An application driven by the desktop host.
pub trait DesktopApp {
    /// Called once the window and GPU exist.
    ///
    /// # Errors
    /// A message explaining why the application cannot start.
    fn start(&mut self, host: &mut DesktopHost) -> Result<(), String>;

    /// Builds one frame of the interface into the root `ui`, which covers
    /// the window.
    fn ui(&mut self, ui: &mut egui::Ui, host: &mut DesktopHost);

    /// Runs timed work that [`FramePacing::WakeAt`] asked for. Returns `true`
    /// if the window must be redrawn.
    fn timer(&mut self, host: &mut DesktopHost) -> bool {
        let _ = host;
        false
    }

    /// What the loop should do next.
    fn pacing(&mut self, now: Instant) -> FramePacing;

    /// The user asked to close the window. Return `true` to close now, or
    /// `false` to keep running (for example to ask first, then call
    /// [`DesktopHost::request_exit`]).
    fn close_requested(&mut self, host: &mut DesktopHost) -> bool {
        let _ = host;
        true
    }

    /// The loop is about to end: save what must survive.
    fn exit(&mut self, host: &mut DesktopHost) {
        let _ = host;
    }
}

/// The id of the main window surface.
pub const MAIN_SURFACE: SurfaceId = SurfaceId(0);

/// The live window with its GPU surface and egui state.
pub struct DesktopHost {
    window: Arc<Window>,
    gpu: GpuContext,
    /// `None` while the system has taken the window away.
    surface: Option<WindowSurface>,
    painter: EguiPainter,
    egui: egui_winit::State,
    ctx: egui::Context,
    os: OsFamily,
    form_factor: FormFactor,
    capabilities: HardwareCapabilities,
    paths: PlatformPaths,
    info: SurfaceInfo,
    clear: egui::Color32,
    repaint_at: Option<Instant>,
    exit_requested: bool,
}

impl std::fmt::Debug for DesktopHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopHost")
            .field("gpu", &self.gpu)
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

impl DesktopHost {
    /// What Android's system bars and on-screen keyboard cover of the
    /// window, in points; `None` until Android has said.
    #[cfg(target_os = "android")]
    fn android_insets(&self) -> Option<SafeAreaInsets> {
        let app = crate::android::app()?;
        let content = app.content_rect();
        let size = self.window.inner_size();
        let scale = self.window.scale_factor() as f32;
        if content.right <= content.left || content.bottom <= content.top {
            return None;
        }
        Some(SafeAreaInsets {
            top: content.top.max(0) as f32 / scale,
            left: content.left.max(0) as f32 / scale,
            right: (size.width as i32 - content.right).max(0) as f32 / scale,
            bottom: (size.height as i32 - content.bottom).max(0) as f32 / scale,
        })
    }

    /// `true` when Android moved the content rectangle since the last
    /// frame: the on-screen keyboard came up or went away (the activity
    /// resizes for it), which reaches winit with no event of its own.
    #[cfg(target_os = "android")]
    fn android_content_moved(&self) -> bool {
        self.android_insets()
            .is_some_and(|insets| insets != self.info.safe_area)
    }

    /// The part of the window Android's system bars and on-screen keyboard
    /// leave free, in points, and the insets it means; `None` until Android
    /// has said.
    #[cfg(target_os = "android")]
    fn android_safe_rect(&mut self) -> Option<egui::Rect> {
        let insets = self.android_insets()?;
        let size = self.window.inner_size();
        let scale = self.window.scale_factor() as f32;
        self.info.safe_area = insets;
        Some(egui::Rect::from_min_max(
            egui::pos2(insets.left, insets.top),
            egui::pos2(
                size.width as f32 / scale - insets.right,
                size.height as f32 / scale - insets.bottom,
            ),
        ))
    }

    /// `true` when iOS's on-screen keyboard moved since the last frame: it
    /// came up, went away or changed height, which reaches winit with no
    /// event of its own.
    #[cfg(target_os = "ios")]
    fn ios_keyboard_moved(&self) -> bool {
        (sue_ios::keyboard_inset() - self.info.safe_area.bottom).abs() > 0.5
    }

    /// Makes the window's surface again, for a window the system gave back.
    fn renew_surface(&mut self) -> Result<(), PlatformError> {
        let surface = self
            .gpu
            .instance
            .create_surface(Arc::clone(&self.window))
            .map_err(|error| PlatformError::Gpu(error.to_string()))?;
        let size = self.window.inner_size();
        self.surface = Some(WindowSurface::new(
            surface,
            &self.gpu,
            size.width.max(1),
            size.height.max(1),
        )?);
        Ok(())
    }

    fn create(
        event_loop: &ActiveEventLoop,
        options: &DesktopOptions,
    ) -> Result<Self, PlatformError> {
        let attributes = Window::default_attributes()
            .with_title(options.title.clone())
            .with_inner_size(LogicalSize::new(options.width, options.height))
            .with_min_inner_size(LogicalSize::new(360.0, 480.0))
            .with_window_icon(pixel_icon(options));
        let window = Arc::new(
            event_loop
                .create_window(system_attributes(attributes, options))
                .map_err(|error| PlatformError::Window(error.to_string()))?,
        );
        #[cfg(target_os = "windows")]
        resource_icons(&window);
        // An Android window takes one graphics API: try Vulkan alone, then
        // OpenGL ES alone, so a surface of one never blocks the other.
        // Elsewhere every backend is offered at once.
        let attempts: &[Option<wgpu::Backends>] = if cfg!(target_os = "android") {
            &[Some(wgpu::Backends::VULKAN), Some(wgpu::Backends::GL)]
        } else {
            &[None]
        };
        let mut chosen = None;
        let mut last_error = PlatformError::Gpu("no GPU can draw this window".to_owned());
        for backends in attempts {
            let mut descriptor = wgpu::InstanceDescriptor::new_with_display_handle_from_env(
                Box::new(event_loop.owned_display_handle()),
            );
            if let Some(backends) = backends {
                descriptor.backends = *backends;
            }
            let instance = wgpu::Instance::new(descriptor);
            let surface = match instance.create_surface(Arc::clone(&window)) {
                Ok(surface) => surface,
                Err(error) => {
                    last_error = PlatformError::Gpu(error.to_string());
                    continue;
                }
            };
            match pollster::block_on(GpuContext::with_instance(
                instance,
                Some(&surface),
                options.gpu,
            )) {
                Ok(gpu) => {
                    chosen = Some((gpu, surface));
                    break;
                }
                Err(error) => last_error = error,
            }
        }
        let (gpu, surface) = chosen.ok_or(last_error)?;
        let size = window.inner_size();
        let surface = WindowSurface::new(surface, &gpu, size.width, size.height)?;
        let painter = EguiPainter::new(&gpu, surface.format());

        let ctx = egui::Context::default();
        let egui = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            event_loop,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(painter.max_texture_side()),
        );

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
        let info = SurfaceInfo {
            id: MAIN_SURFACE,
            title: options.title.clone(),
            width: size.width as f32 / scale,
            height: size.height as f32 / scale,
            scale_factor: scale,
            safe_area: SafeAreaInsets::default(),
        };
        Ok(Self {
            window,
            gpu,
            surface: Some(surface),
            painter,
            egui,
            ctx,
            os,
            // Phones and tablets by their shorter side; computers are desktops.
            form_factor: FormFactor::classify(os, size.width.min(size.height) as f32 / scale),
            capabilities,
            paths,
            info,
            clear: egui::Color32::BLACK,
            repaint_at: None,
            exit_requested: false,
        })
    }

    /// The GPU objects.
    #[must_use]
    pub fn gpu(&self) -> &GpuContext {
        &self.gpu
    }

    /// The egui context of the window.
    #[must_use]
    pub fn egui(&self) -> &egui::Context {
        &self.ctx
    }

    /// Uploads images for the interface (see [`crate::TextureUpload`]).
    pub fn textures(&mut self) -> &mut EguiPainter {
        &mut self.painter
    }

    /// The window's logical size and scale.
    #[must_use]
    pub fn window_info(&self) -> &SurfaceInfo {
        &self.info
    }

    /// The color behind everything egui draws.
    pub fn set_clear_color(&mut self, color: egui::Color32) {
        self.clear = color;
    }

    /// Asks the loop to end after this event.
    pub fn request_exit(&mut self) {
        self.exit_requested = true;
    }

    /// Builds, paints and presents one frame.
    fn frame<A: DesktopApp>(&mut self, app: &mut A) {
        #[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(unused_mut))]
        let mut raw = self.egui.take_egui_input(&self.window);
        #[cfg(target_os = "android")]
        if let Some(safe) = self.android_safe_rect() {
            // Lay everything out between the status and navigation bars.
            raw.screen_rect = Some(safe);
        }
        #[cfg(target_os = "ios")]
        if let Some(screen) = raw.screen_rect.as_mut() {
            // Lay everything out above the on-screen keyboard; the safe
            // area's bottom is what it covers, as on Android.
            let keyboard = sue_ios::keyboard_inset();
            self.info.safe_area.bottom = keyboard;
            screen.max.y = (screen.max.y - keyboard).max(screen.min.y);
        }
        let ctx = self.ctx.clone();
        let output = ctx.run_ui(raw, |ui| app.ui(ui, self));
        self.egui
            .handle_platform_output(&self.window, output.platform_output);
        let delay = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(Duration::MAX, |viewport| viewport.repaint_delay);
        // Anything past a day is egui's "never".
        self.repaint_at = (delay < Duration::from_secs(86_400)).then(|| Instant::now() + delay);
        let Some(surface) = self.surface.as_mut() else {
            // No window to draw into: keep egui's textures in step.
            self.painter.apply_textures(output.textures_delta);
            return;
        };
        let (width, height) = surface.size();
        match surface.acquire(&self.gpu) {
            Some(frame) => {
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                self.painter.paint(
                    &ctx,
                    &view,
                    [width, height],
                    self.clear,
                    output.shapes,
                    output.textures_delta,
                    output.pixels_per_point,
                );
                self.window.pre_present_notify();
                self.gpu.queue.present(frame);
            }
            None => {
                // Keep egui's textures in step and try again next frame.
                self.painter.apply_textures(output.textures_delta);
                self.repaint_at = Some(Instant::now() + Duration::from_millis(16));
            }
        }
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
        // egui receives the window's input directly.
        Vec::new()
    }
}

/// Opens the main window and runs `app` until the window closes.
///
/// # Errors
/// Window, GPU or application start-up failures.
pub fn run_desktop<A: DesktopApp>(app: A, options: DesktopOptions) -> Result<(), PlatformError> {
    #[cfg(target_os = "android")]
    let event_loop = {
        use winit::platform::android::EventLoopBuilderExtAndroid;
        let android = crate::android::app().ok_or_else(|| {
            PlatformError::Window("Android did not hand Sue its activity".to_owned())
        })?;
        EventLoop::builder()
            .with_android_app(android)
            .build()
            .map_err(|error| PlatformError::Window(error.to_string()))?
    };
    #[cfg(not(target_os = "android"))]
    let event_loop = EventLoop::new().map_err(|error| PlatformError::Window(error.to_string()))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut runner = Runner {
        app,
        options,
        host: None,
        error: None,
        last_frame: None,
        exiting: false,
        paused: false,
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
    /// The system took the window away (Android in the background): nothing
    /// draws until it is given back.
    paused: bool,
}

/// What the loop does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Block until input.
    Wait,
    /// Block until input or this instant.
    WaitUntil(Instant),
    /// Request a redraw now, then block until it arrives.
    Redraw,
}

/// Combines the application's pacing with egui's next repaint. Uncapped
/// animation is paced by vsync (presenting blocks), so it redraws at once; a
/// lower cap waits for the next frame slot after `last_frame`.
fn plan_step(
    pacing: FramePacing,
    repaint_at: Option<Instant>,
    now: Instant,
    last_frame: Option<Instant>,
    display_hz: u32,
) -> Step {
    let animate = match pacing {
        FramePacing::Animate { target_hz } => Some(target_hz),
        _ => None,
    };
    if animate.is_some() || repaint_at.is_some_and(|at| at <= now) {
        let target_hz = animate.unwrap_or(display_hz).max(1);
        let interval = Duration::from_secs_f64(1.0 / f64::from(target_hz));
        return match last_frame.map(|last| last + interval) {
            Some(next) if target_hz < display_hz && next > now => Step::WaitUntil(next),
            _ => Step::Redraw,
        };
    }
    let timer = match pacing {
        FramePacing::WakeAt(at) => Some(at),
        _ => None,
    };
    [timer, repaint_at]
        .into_iter()
        .flatten()
        .min()
        .map_or(Step::Wait, Step::WaitUntil)
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
        if self.exiting || self.paused {
            return;
        }
        let Some(host) = self.host.as_mut() else {
            return;
        };
        // The window lays out above an on-screen keyboard: draw again as
        // soon as it comes up or goes away.
        #[cfg(target_os = "android")]
        if host.android_content_moved() {
            host.window.request_redraw();
        }
        #[cfg(target_os = "ios")]
        if host.ios_keyboard_moved() {
            host.window.request_redraw();
        }
        let now = Instant::now();
        let mut pacing = self.app.pacing(now);
        if let FramePacing::WakeAt(at) = pacing
            && at <= now
        {
            if self.app.timer(host) {
                host.window.request_redraw();
            }
            pacing = match self.app.pacing(Instant::now()) {
                // A deadline that is still due would spin the loop: retry soon.
                FramePacing::WakeAt(at) if at <= Instant::now() => {
                    FramePacing::WakeAt(Instant::now() + Duration::from_millis(1))
                }
                other => other,
            };
        }
        let display_hz = host.capabilities.display_refresh_hz;
        match plan_step(
            pacing,
            host.repaint_at,
            Instant::now(),
            self.last_frame,
            display_hz,
        ) {
            Step::Wait => event_loop.set_control_flow(ControlFlow::Wait),
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

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.paused = true;
        // Let go of the surface now: Android takes the window away and a
        // new surface cannot connect to it while the old one is still there.
        if let Some(host) = self.host.as_mut() {
            host.surface = None;
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(host) = self.host.as_mut() {
            // Back from the background: the window may be new, so is its
            // surface.
            if self.paused {
                if let Err(error) = host.renew_surface() {
                    self.error = Some(error);
                    event_loop.exit();
                    return;
                }
                host.window.request_redraw();
            }
            self.paused = false;
            return;
        }
        self.paused = false;
        match DesktopHost::create(event_loop, &self.options) {
            Ok(mut host) => match self.app.start(&mut host) {
                Ok(()) => {
                    // On Windows the window is still hidden: draw its first
                    // frame, then show it, whole and with nothing left to do
                    // when the taskbar asks it for its icon.
                    #[cfg(target_os = "windows")]
                    {
                        host.frame(&mut self.app);
                        self.last_frame = Some(Instant::now());
                        host.window.set_visible(true);
                    }
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
        if self.exiting || (self.paused && matches!(event, OsWindowEvent::RedrawRequested)) {
            return;
        }
        let Some(host) = self.host.as_mut() else {
            return;
        };
        let response = host.egui.on_window_event(&host.window, &event);
        // egui asks to repaint after any event, the redraw itself included;
        // answering that one would draw forever.
        if response.repaint && !matches!(event, OsWindowEvent::RedrawRequested) {
            host.window.request_redraw();
        }
        match event {
            OsWindowEvent::CloseRequested => {
                if self.app.close_requested(host) {
                    self.finish(event_loop);
                    return;
                }
                host.window.request_redraw();
            }
            OsWindowEvent::Resized(size) => {
                // While the system has the window (Android in the
                // background), its surface is gone; the size is used when
                // the surface is made again.
                if let Some(surface) = host
                    .surface
                    .as_mut()
                    .filter(|_| size.width > 0 && size.height > 0)
                {
                    surface.resize(&host.gpu, size.width, size.height);
                }
                let scale = host.window.scale_factor() as f32;
                host.info.width = size.width as f32 / scale;
                host.info.height = size.height as f32 / scale;
                host.info.scale_factor = scale;
                host.window.request_redraw();
            }
            OsWindowEvent::RedrawRequested => {
                host.frame(&mut self.app);
                self.last_frame = Some(Instant::now());
            }
            _ => {}
        }
        if host.exit_requested {
            self.finish(event_loop);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pacing_turns_into_loop_steps() {
        let now = Instant::now();
        let soon = now + Duration::from_millis(40);
        assert_eq!(
            plan_step(FramePacing::Sleep, None, now, None, 60),
            Step::Wait
        );
        assert_eq!(
            plan_step(FramePacing::WakeAt(soon), None, now, None, 60),
            Step::WaitUntil(soon)
        );
        // egui asks for a frame later (a caret blink): wait for it.
        assert_eq!(
            plan_step(FramePacing::Sleep, Some(soon), now, None, 60),
            Step::WaitUntil(soon)
        );
        // egui asks for a frame now (a spring called request_repaint).
        assert_eq!(
            plan_step(FramePacing::Sleep, Some(now), now, Some(now), 60),
            Step::Redraw
        );
        // Animating at the display rate redraws at once; vsync paces it.
        assert_eq!(
            plan_step(
                FramePacing::Animate { target_hz: 120 },
                None,
                now,
                Some(now),
                120
            ),
            Step::Redraw
        );
        // A 30 FPS cap on a 60 Hz display waits for the next slot.
        assert!(matches!(
            plan_step(
                FramePacing::Animate { target_hz: 30 },
                None,
                now,
                Some(now),
                60
            ),
            Step::WaitUntil(_)
        ));
    }
}
