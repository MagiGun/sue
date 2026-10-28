//! The live GPU window: the shell runtime driven by the native desktop host.

use std::time::Instant;

use sue_platform::{
    CursorShape, DesktopApp, DesktopHost, DesktopOptions, FramePacing, GpuPreference, InputEvent,
    MouseEvent, PlatformHost, PlatformPaths, Point, WindowEvent, audio_output_devices, run_desktop,
    wgpu, write_atomic,
};
use sue_rice::{DEFAULT_RICE, RiceSchema};
use sue_wm::{FrameTarget, Rect, UiRenderer};

use crate::engine::{BootOptions, Engine, RICE_FILE};
use crate::runtime::Runtime;

/// The window title.
pub const TITLE: &str = "Sue";

#[derive(Default)]
struct DesktopShell {
    runtime: Option<Runtime>,
    renderer: Option<UiRenderer>,
}

impl DesktopShell {
    fn ui_scale(runtime: &Runtime) -> f32 {
        runtime.engine.workspace.config().ui_scale.max(0.25)
    }

    /// Maps window coordinates onto the scaled interface.
    fn scale_event(event: InputEvent, scale: f32) -> InputEvent {
        let point = |point: Point| Point::new(point.x / scale, point.y / scale);
        match event {
            InputEvent::Mouse(MouseEvent::Moved { position }) => {
                InputEvent::Mouse(MouseEvent::Moved {
                    position: point(position),
                })
            }
            InputEvent::Mouse(MouseEvent::Button {
                button,
                pressed,
                position,
            }) => InputEvent::Mouse(MouseEvent::Button {
                button,
                pressed,
                position: point(position),
            }),
            InputEvent::Mouse(MouseEvent::Wheel { delta_x, delta_y }) => {
                InputEvent::Mouse(MouseEvent::Wheel {
                    delta_x: delta_x / scale,
                    delta_y: delta_y / scale,
                })
            }
            InputEvent::Window(WindowEvent::Resized {
                width,
                height,
                scale_factor,
            }) => InputEvent::Window(WindowEvent::Resized {
                width: width / scale,
                height: height / scale,
                scale_factor,
            }),
            other => other,
        }
    }

    fn sync(host: &mut DesktopHost, runtime: &Runtime) {
        host.set_wheel_line_px(runtime.engine.workspace.config().wheel_line_px);
        if runtime.wants_exit() {
            host.request_exit();
        }
    }
}

impl DesktopApp for DesktopShell {
    fn start(&mut self, host: &mut DesktopHost) -> Result<(), String> {
        let rice_path = host.paths().rice_dir().join(RICE_FILE);
        if !rice_path.exists() {
            // Seed the live rice file with the shipped defaults, byte for byte.
            write_atomic(&rice_path, DEFAULT_RICE.as_bytes())
                .map_err(|error| format!("cannot create {}: {error}", rice_path.display()))?;
        }
        let options = BootOptions {
            audio_devices: audio_output_devices(),
            gpu_label: Some(host.gpu().adapter_label()),
        };
        let engine = Engine::boot_with(&*host, options)?;
        let runtime = Runtime::new(engine)?.interactive(true);
        eprintln!(
            "sue: {TITLE} on {}, rice {}",
            host.gpu().adapter_label(),
            runtime.engine.rice_source
        );
        self.renderer = Some(UiRenderer::new(&host.gpu().device, host.surface_format()));
        Self::sync(host, &runtime);
        self.runtime = Some(runtime);
        Ok(())
    }

    fn input(&mut self, host: &mut DesktopHost, event: InputEvent) -> bool {
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        let event = Self::scale_event(event, Self::ui_scale(runtime));
        let redraw = runtime.handle(&event);
        Self::sync(host, runtime);
        redraw
    }

    fn timer(&mut self, host: &mut DesktopHost) -> bool {
        let Some(runtime) = self.runtime.as_mut() else {
            return false;
        };
        let redraw = runtime.timer(Instant::now());
        Self::sync(host, runtime);
        redraw
    }

    fn frame(&mut self, host: &mut DesktopHost) {
        let (Some(runtime), Some(renderer)) = (self.runtime.as_mut(), self.renderer.as_mut())
        else {
            return;
        };
        let info = host.window_info().clone();
        let ui_scale = Self::ui_scale(runtime);
        let viewport = Rect::new(0.0, 0.0, info.width / ui_scale, info.height / ui_scale);
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
                    scale: info.scale_factor * ui_scale,
                },
                &output.canvas,
                &runtime.fonts,
                &runtime.engine.world.arena,
                runtime.theme().base,
            );
            host.present(frame);
        }
        if let Some(Err(error)) = runtime.persist() {
            eprintln!("sue: could not save the rice: {error}");
        }
        Self::sync(host, runtime);
    }

    fn pacing(&mut self, now: Instant) -> FramePacing {
        self.runtime
            .as_ref()
            .map_or(FramePacing::Sleep, |runtime| runtime.pacing(now))
    }

    fn close_requested(&mut self, _host: &mut DesktopHost) -> bool {
        self.runtime.as_mut().is_none_or(Runtime::close_requested)
    }

    fn exit(&mut self, _host: &mut DesktopHost) {
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.shutdown();
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
            width: 1440.0,
            height: 900.0,
            wheel_line_px: 48.0,
            gpu: gpu_preference(),
        },
    )
    .map_err(|error| error.to_string())
}

/// `display.gpu_preference` from the live rice file. The GPU is chosen before
/// the engine boots, so this reads the file itself; a missing file or key
/// means `auto`.
fn gpu_preference() -> GpuPreference {
    PlatformPaths::detect()
        .ok()
        .and_then(|paths| std::fs::read_to_string(paths.rice_dir().join(RICE_FILE)).ok())
        .and_then(|text| RiceSchema::parse(&text).ok())
        .and_then(|rice| {
            rice.get_str("display.gpu_preference")
                .map(GpuPreference::from_setting)
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_interface_scale_maps_pointer_coordinates() {
        let event = InputEvent::Mouse(MouseEvent::Moved {
            position: Point::new(300.0, 150.0),
        });
        assert_eq!(
            DesktopShell::scale_event(event, 1.5),
            InputEvent::Mouse(MouseEvent::Moved {
                position: Point::new(200.0, 100.0)
            })
        );
    }
}
