//! The live GPU window: the shell runtime driven by the native desktop host.

use std::time::Instant;

use sue_platform::{
    CursorShape, DesktopApp, DesktopHost, DesktopOptions, FramePacing, InputEvent, PlatformHost,
    run_desktop, wgpu, write_atomic,
};
use sue_rice::DEFAULT_RICE;
use sue_wm::{FrameTarget, Rect, RenderDemand, UiRenderer};

use crate::engine::Engine;
use crate::runtime::Runtime;

/// The window title.
pub const TITLE: &str = "Sue v1.0 — Faustian Enigma";

#[derive(Default)]
struct DesktopShell {
    runtime: Option<Runtime>,
    renderer: Option<UiRenderer>,
}

impl DesktopShell {
    fn wheel_line_px(runtime: &Runtime) -> f32 {
        runtime
            .engine
            .world
            .rice
            .get_f64("input.wheel_line_px")
            .unwrap_or(48.0) as f32
    }
}

impl DesktopApp for DesktopShell {
    fn start(&mut self, host: &mut DesktopHost) -> Result<(), String> {
        let rice_path = host.paths().rice_dir().join(crate::engine::RICE_FILE);
        if !rice_path.exists() {
            // Seed the live rice file with the shipped defaults, byte for byte.
            write_atomic(&rice_path, DEFAULT_RICE.as_bytes())
                .map_err(|error| format!("cannot create {}: {error}", rice_path.display()))?;
        }
        let engine = Engine::boot(&*host)?;
        let runtime = Runtime::new(engine)?;
        host.set_wheel_line_px(Self::wheel_line_px(&runtime));
        eprintln!(
            "sue: {} on {} · rice {}",
            TITLE,
            host.gpu().adapter_label(),
            runtime.engine.rice_path().display()
        );
        self.renderer = Some(UiRenderer::new(&host.gpu().device, host.surface_format()));
        self.runtime = Some(runtime);
        Ok(())
    }

    fn input(&mut self, host: &mut DesktopHost, event: InputEvent) -> bool {
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        let redraw = runtime.handle(&event);
        host.set_wheel_line_px(Self::wheel_line_px(runtime));
        redraw
    }

    fn frame(&mut self, host: &mut DesktopHost) -> FramePacing {
        let (Some(runtime), Some(renderer)) = (self.runtime.as_mut(), self.renderer.as_mut())
        else {
            return FramePacing::Sleep;
        };
        let info = host.window_info().clone();
        let viewport = Rect::new(0.0, 0.0, info.width, info.height);
        let output = runtime.frame(viewport, info.safe_area, Instant::now());
        host.set_cursor(output.cursor.unwrap_or(CursorShape::Default));
        if let Some(frame) = host.acquire_frame() {
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let (width, height) = host.physical_size();
            let gpu = host.gpu();
            renderer.render(
                &gpu.device,
                &gpu.queue,
                &view,
                FrameTarget {
                    width,
                    height,
                    scale: info.scale_factor,
                },
                &output.canvas,
                &runtime.fonts,
                &runtime.engine.world.arena,
                runtime.theme().background,
            );
            host.present(frame);
        }
        if let Some(Err(error)) = runtime.persist() {
            eprintln!("sue: could not save the rice: {error}");
        }
        match output.next {
            RenderDemand::Sleep => FramePacing::Sleep,
            RenderDemand::ActiveFrame { target_hz } => FramePacing::Animate { target_hz },
        }
    }
}

/// Opens the window and runs until it closes.
///
/// # Errors
/// Window, GPU or boot failures.
pub fn run() -> Result<(), String> {
    run_desktop(
        DesktopShell::default(),
        DesktopOptions {
            title: TITLE.to_owned(),
            width: 1280.0,
            height: 800.0,
            wheel_line_px: 48.0,
        },
    )
    .map_err(|error| error.to_string())
}
