//! The live GPU window: the runtime driven by the native desktop host.

use std::time::Instant;

use sue_platform::{
    DesktopApp, DesktopHost, DesktopOptions, FramePacing, GpuPreference, PlatformHost,
    PlatformPaths, audio_output_devices, run_desktop, write_atomic,
};
use sue_rice::{DEFAULT_RICE, RiceSchema};

use crate::engine::{BootOptions, Engine, RICE_FILE};
use crate::runtime::Runtime;

/// The window title (house style: exactly `Sue`).
pub const TITLE: &str = "Sue";

#[derive(Default)]
struct DesktopShell {
    runtime: Option<Runtime>,
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
            gpu_kind: Some(host.gpu().adapter_kind().to_owned()),
            gpus: host.gpu().available().to_vec(),
        };
        // Sue starts in the system's language when it has it.
        sue_lang::set_system(&sue_platform::system_languages());
        let engine = Engine::boot_with(&*host, options)?;
        sue_lang::set_language(&engine.config.language);
        let runtime = Runtime::new(engine)?.interactive(true);
        host.set_clear_color(runtime.theme().void);
        self.runtime = Some(runtime);
        Ok(())
    }

    fn ui(&mut self, ui: &mut egui::Ui, host: &mut DesktopHost) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        runtime.ui(ui, host.textures(), Instant::now());
        host.set_clear_color(runtime.theme().void);
        if runtime.wants_exit() {
            host.request_exit();
        }
    }

    fn timer(&mut self, host: &mut DesktopHost) -> bool {
        self.runtime
            .as_mut()
            .is_some_and(|runtime| runtime.timer(host.textures(), Instant::now()))
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
            if runtime.wants_restart()
                && let Ok(exe) = std::env::current_exe()
            {
                // A fresh Sue picks up what only applies at startup (the GPU).
                let _ = std::process::Command::new(exe)
                    .args(std::env::args_os().skip(1))
                    .spawn();
            }
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
            gpu: gpu_preference(),
            icon: sue_wm::brand::pixels(sue_wm::brand::APP_ICON, 256),
            app_id: sue_wm::brand::APP_ID.to_owned(),
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
