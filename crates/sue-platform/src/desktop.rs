//! The native desktop host: one `winit` window, a `wgpu` surface, and egui
//! (`egui-winit` for input, IME and the clipboard; [`EguiPainter`] for
//! drawing).
//!
//! Rendering is event-driven. After every frame the loop asks two
//! questions: does the application still animate (a spring in motion, which
//! also calls `request_repaint`), and when does egui want its next frame
//! (a hover fade, a caret blink)? With neither, it blocks in
//! [`ControlFlow::Wait`] and draws nothing until input arrives.
//!
//! In the browser the window is the page's canvas and the loop is the
//! browser's own: the GPU and the page's saved files arrive later
//! (nothing may block there), so the host is made once they come.

use std::sync::Arc;
use std::time::Duration;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{StartCause, WindowEvent as OsWindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Icon, Window, WindowId};

use crate::gpu::{GpuContext, GpuPreference, WindowSurface};
use crate::paint::EguiPainter;
use crate::time::Instant;
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

/// The browser draws Sue on the page's `<canvas id="sue">`, which the
/// page sizes, or on a canvas of its own added to the page.
#[cfg(target_arch = "wasm32")]
fn web_attributes(attributes: winit::window::WindowAttributes) -> winit::window::WindowAttributes {
    use wasm_bindgen::JsCast;
    use winit::platform::web::WindowAttributesExtWebSys;
    let canvas = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("sue"))
        .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok());
    attributes
        .with_append(canvas.is_none())
        .with_canvas(canvas)
        .with_prevent_default(true)
        .with_focusable(true)
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
    /// A WinTab tablet's pen pressure (see `crate::stylus`): `None` until
    /// the first frame looks for one, then whether there is one.
    #[cfg(target_os = "windows")]
    tablet: Option<Option<crate::wintab_pen::Tablet>>,
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

    /// Opens the window (in the browser, takes the page's canvas).
    fn open_window(
        event_loop: &ActiveEventLoop,
        options: &DesktopOptions,
    ) -> Result<Arc<Window>, PlatformError> {
        let attributes = Window::default_attributes()
            .with_title(options.title.clone())
            .with_window_icon(pixel_icon(options));
        // The page sizes the canvas; a window sizes itself.
        #[cfg(target_arch = "wasm32")]
        let attributes = web_attributes(attributes);
        #[cfg(not(target_arch = "wasm32"))]
        let attributes = attributes
            .with_inner_size(LogicalSize::new(options.width, options.height))
            .with_min_inner_size(LogicalSize::new(360.0, 480.0));
        let window = Arc::new(
            event_loop
                .create_window(system_attributes(attributes, options))
                .map_err(|error| PlatformError::Window(error.to_string()))?,
        );
        #[cfg(target_os = "windows")]
        resource_icons(&window);
        Ok(window)
    }

    /// Opens the window and its GPU, waiting for the GPU (not in the
    /// browser, where nothing may wait).
    #[cfg(not(target_arch = "wasm32"))]
    fn create(
        event_loop: &ActiveEventLoop,
        options: &DesktopOptions,
    ) -> Result<Self, PlatformError> {
        let window = Self::open_window(event_loop, options)?;
        let (gpu, surface) = pollster::block_on(open_gpu(
            Arc::clone(&window),
            event_loop.owned_display_handle(),
            options.gpu,
        ))?;
        Self::assemble(event_loop, window, gpu, surface, options)
    }

    /// The host for a window whose GPU is open.
    fn assemble(
        event_loop: &ActiveEventLoop,
        window: Arc<Window>,
        gpu: GpuContext,
        surface: wgpu::Surface<'static>,
        options: &DesktopOptions,
    ) -> Result<Self, PlatformError> {
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
        // The browser's own copy, cut and paste events carry the clipboard
        // (`crate::web::page`); the keys alone would paste only Sue's copies.
        #[cfg(target_arch = "wasm32")]
        let egui = {
            let mut egui = egui;
            egui.set_clipboard_shortcuts(false);
            egui
        };

        let os = OsFamily::current();
        let paths = PlatformPaths::detect()?;
        let refresh_hz = window
            .current_monitor()
            .and_then(|monitor| monitor.refresh_rate_millihertz())
            .map_or(HardwareCapabilities::DEFAULT_REFRESH_HZ, |millihertz| {
                millihertz.div_ceil(1000)
            });
        #[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
        let mut capabilities = gpu.describe(HardwareCapabilities {
            display_refresh_hz: refresh_hz,
            ..HardwareCapabilities::probe_portable()
        });
        let scale = window.scale_factor() as f32;
        // Phones and tablets by their shorter side; computers are desktops.
        // A browser is a phone or tablet when touch is how it is used, and
        // a desktop when a phone's browser was asked for the desktop site.
        let shorter_side = size.width.min(size.height) as f32 / scale;
        #[cfg(not(target_arch = "wasm32"))]
        let form_factor = FormFactor::classify(os, shorter_side);
        #[cfg(target_arch = "wasm32")]
        let form_factor = {
            capabilities.touch = crate::web::page::touch_screen();
            // The device's screen, not the page: a phone showing the
            // desktop site lays the page out wider than it is.
            let side = crate::web::page::screen_side()
                .map_or(shorter_side, |screen| screen.min(shorter_side));
            FormFactor::for_browser(
                capabilities.touch,
                FormFactor::mobile_agent(&crate::web::page::user_agent()),
                side,
            )
        };
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
            form_factor,
            capabilities,
            paths,
            info,
            clear: egui::Color32::BLACK,
            repaint_at: None,
            exit_requested: false,
            #[cfg(target_os = "windows")]
            tablet: None,
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
        // A WinTab tablet's pen pressure for this frame. The context opens
        // on the first frame, once the window is shown.
        #[cfg(target_os = "windows")]
        {
            let window = &self.window;
            let tablet = self
                .tablet
                .get_or_insert_with(|| crate::wintab_pen::Tablet::open(window));
            if let Some(tablet) = tablet {
                tablet.poll();
            }
        }
        #[cfg_attr(
            not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")),
            allow(unused_mut)
        )]
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
        // The page's clipboard events, and files dropped on it (staged).
        #[cfg(target_arch = "wasm32")]
        {
            let (events, dropped) = crate::web::page::take_input();
            raw.events.extend(events);
            raw.dropped_files
                .extend(dropped.into_iter().map(crate::web::page::Staged::handle));
        }
        let ctx = self.ctx.clone();
        let output = ctx.run_ui(raw, |ui| app.ui(ui, self));
        #[cfg(target_arch = "wasm32")]
        for command in &output.platform_output.commands {
            match command {
                egui::OutputCommand::CopyText(text) => {
                    let _ = crate::web::page::copy_text(text);
                }
                egui::OutputCommand::OpenUrl(open) => {
                    let _ = crate::web::page::open_url(&open.url);
                }
                _ => {}
            }
        }
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
///
/// In the browser it returns at once: the page's own loop runs `app`, and
/// a failure to start is shown on the page's loading screen.
pub fn run_desktop<A: DesktopApp + 'static>(
    app: A,
    options: DesktopOptions,
) -> Result<(), PlatformError> {
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
    #[cfg(target_arch = "wasm32")]
    crate::web::page::catch_panics();
    #[cfg(target_arch = "wasm32")]
    let event_loop = EventLoop::<Wake>::with_user_event()
        .build()
        .map_err(|error| PlatformError::Window(error.to_string()))?;
    #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
    let event_loop = EventLoop::new().map_err(|error| PlatformError::Window(error.to_string()))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    let mut runner = Runner {
        app,
        options,
        host: None,
        error: None,
        last_frame: None,
        exiting: false,
        paused: false,
        #[cfg(target_arch = "wasm32")]
        proxy: event_loop.create_proxy(),
        #[cfg(target_arch = "wasm32")]
        opening: None,
    };
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::EventLoopExtWebSys;
        event_loop.spawn_app(runner);
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        event_loop
            .run_app(&mut runner)
            .map_err(|error| PlatformError::Window(error.to_string()))?;
        runner.error.map_or(Ok(()), Err)
    }
}

/// Opens a GPU that draws into `window`. An Android window takes one
/// graphics API: try Vulkan alone, then OpenGL ES alone, so a surface of
/// one never blocks the other. A browser offers WebGPU where it works and
/// WebGL2 otherwise (it decides before the canvas is touched, which takes
/// one kind of drawing only). Elsewhere every backend is offered at once.
async fn open_gpu(
    window: Arc<Window>,
    display: winit::event_loop::OwnedDisplayHandle,
    preference: GpuPreference,
) -> Result<(GpuContext, wgpu::Surface<'static>), PlatformError> {
    let attempts: &[Option<wgpu::Backends>] = if cfg!(target_os = "android") {
        &[Some(wgpu::Backends::VULKAN), Some(wgpu::Backends::GL)]
    } else if cfg!(target_arch = "wasm32") {
        &[Some(
            wgpu::Backends::BROWSER_WEBGPU.union(wgpu::Backends::GL),
        )]
    } else {
        &[None]
    };
    let mut last_error = PlatformError::Gpu("no GPU can draw this window".to_owned());
    for backends in attempts {
        let mut descriptor =
            wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(display.clone()));
        if let Some(backends) = backends {
            descriptor.backends = *backends;
        }
        #[cfg(target_arch = "wasm32")]
        let instance = wgpu::util::new_instance_with_webgpu_detection(descriptor).await;
        #[cfg(not(target_arch = "wasm32"))]
        let instance = wgpu::Instance::new(descriptor);
        let surface = match instance.create_surface(Arc::clone(&window)) {
            Ok(surface) => surface,
            Err(error) => {
                last_error = PlatformError::Gpu(error.to_string());
                continue;
            }
        };
        match GpuContext::with_instance(instance, Some(&surface), preference).await {
            Ok(gpu) => return Ok((gpu, surface)),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

/// What the browser hands the loop once the page's files and the GPU
/// are ready.
#[cfg(target_arch = "wasm32")]
enum Wake {
    Ready(Result<(GpuContext, wgpu::Surface<'static>), PlatformError>),
}

#[cfg(not(target_arch = "wasm32"))]
type Wake = ();

struct Runner<A> {
    app: A,
    options: DesktopOptions,
    host: Option<DesktopHost>,
    error: Option<PlatformError>,
    /// Reaches the loop from the browser's start-up.
    #[cfg(target_arch = "wasm32")]
    proxy: winit::event_loop::EventLoopProxy<Wake>,
    /// The canvas, while its GPU is being opened.
    #[cfg(target_arch = "wasm32")]
    opening: Option<Arc<Window>>,
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
    /// Starts the application on a new host.
    fn begin(&mut self, event_loop: &ActiveEventLoop, created: Result<DesktopHost, PlatformError>) {
        match created {
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

    /// Takes the page's canvas, then reads the page's saved files and
    /// opens its GPU, which the browser only does while Sue waits for it;
    /// [`Wake::Ready`] brings them back to the loop.
    #[cfg(target_arch = "wasm32")]
    fn open_in_browser(&mut self, event_loop: &ActiveEventLoop) {
        if self.opening.is_some() {
            return;
        }
        let window = match DesktopHost::open_window(event_loop, &self.options) {
            Ok(window) => window,
            Err(error) => {
                crate::web::page::failed(&error.to_string());
                self.error = Some(error);
                return;
            }
        };
        self.opening = Some(Arc::clone(&window));
        let proxy = self.proxy.clone();
        let display = event_loop.owned_display_handle();
        let preference = self.options.gpu;
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(why) = crate::web::store::load().await {
                // Sue runs without them; nothing will survive a reload.
                web_sys::console::warn_1(&format!("sue: no saved files: {why}").into());
            }
            let ready = open_gpu(window, display, preference).await;
            let _ = proxy.send_event(Wake::Ready(ready));
        });
    }

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

/// Wires the page to the window: its clipboard, dropped files and picker
/// answers draw a frame, and the loading screen goes.
#[cfg(target_arch = "wasm32")]
fn connect_page(host: &DesktopHost) {
    use winit::platform::web::WindowExtWebSys;
    let wake = Arc::clone(&host.window);
    let through = Arc::clone(&host.window);
    crate::web::page::connect(
        move || wake.request_redraw(),
        move |pass| through.set_prevent_default(!pass),
    );
    if let Some(canvas) = host.window.canvas() {
        let _ = canvas.focus();
    }
    crate::web::page::started();
}

impl<A: DesktopApp> ApplicationHandler<Wake> for Runner<A> {
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
        #[cfg(target_arch = "wasm32")]
        self.open_in_browser(event_loop);
        #[cfg(not(target_arch = "wasm32"))]
        self.begin(event_loop, DesktopHost::create(event_loop, &self.options));
    }

    /// The browser's start-up is done: the page's files are read and the
    /// GPU is open (or failed).
    #[cfg(target_arch = "wasm32")]
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Wake) {
        let Wake::Ready(ready) = event;
        let Some(window) = self.opening.take() else {
            return;
        };
        let created = ready.and_then(|(gpu, surface)| {
            DesktopHost::assemble(event_loop, window, gpu, surface, &self.options)
        });
        self.begin(event_loop, created);
        match (&self.host, &self.error) {
            (Some(host), _) => connect_page(host),
            (None, Some(error)) => crate::web::page::failed(&error.to_string()),
            (None, None) => {}
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
