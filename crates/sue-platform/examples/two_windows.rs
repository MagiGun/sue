//! Opens a second window beside the main one on a real desktop, checks
//! that each draws on its own, closes the second and checks that the main
//! window goes on, then quits and prints what it saw.
//!
//! `cargo run -p sue-platform --example two_windows`

use std::time::Duration;

use sue_platform::time::Instant;
use sue_platform::{
    DesktopApp, DesktopHost, DesktopOptions, FramePacing, GpuPreference, WindowSpec, run_desktop,
};

const SECOND: &str = "second";

#[derive(Default)]
struct Two {
    started: Option<Instant>,
    main_frames: u32,
    second_frames: u32,
    /// The second window is wanted.
    second_open: bool,
    closed_at: Option<Instant>,
    main_after_close: u32,
    second_scale: Option<f32>,
    second_size: Option<[f32; 2]>,
    /// Frames drawn in the quiet second after both settled.
    quiet: Option<(u32, u32)>,
    screens: usize,
    report: Vec<String>,
    /// How often the timer ran.
    ticks: u32,
    /// Where the second window opens, and where it was.
    place: Option<[i32; 2]>,
    second_at: Option<[i32; 2]>,
}

impl DesktopApp for Two {
    fn start(&mut self, host: &mut DesktopHost) -> Result<(), String> {
        self.started = Some(Instant::now());
        self.second_open = true;
        let screens = host.screens();
        self.screens = screens.len();
        // On the last screen, when there is more than one.
        self.place = screens
            .iter()
            .skip(1)
            .last()
            .map(|screen| screen.place([40.0, 40.0]));
        for screen in &screens {
            self.report.push(format!(
                "screen {:?} at {:?} pixels, {:?} points, scale {}",
                screen.name,
                screen.at,
                screen.size(),
                screen.scale
            ));
        }
        Ok(())
    }

    fn ui(&mut self, ui: &mut egui::Ui, host: &mut DesktopHost) {
        self.main_frames += 1;
        if std::env::var_os("TWO_TRACE").is_some() {
            eprintln!("main frame {}", self.main_frames);
        }
        if self.closed_at.is_some() {
            self.main_after_close += 1;
        }
        ui.label(format!("main window, frame {}", self.main_frames));
        if let Some(info) = host.sue_window(SECOND) {
            self.second_at = host.sue_window_at(SECOND);
            self.second_scale = Some(info.scale_factor);
            self.second_size = Some([info.width, info.height]);
        }
        let elapsed = self.started.map_or(0.0, |at| at.elapsed().as_secs_f32());
        if elapsed > 4.0 && self.closed_at.is_none() {
            // Close the second window, as its close button would.
            self.second_open = false;
            self.closed_at = Some(Instant::now());
        }
        if self
            .closed_at
            .is_some_and(|at| at.elapsed() > Duration::from_secs(1))
        {
            self.report.push(format!(
                "main frames {}, second frames {}, main frames after closing {}, second open now {}",
                self.main_frames,
                self.second_frames,
                self.main_after_close,
                host.sue_window(SECOND).is_some()
            ));
            host.request_exit();
        }
    }

    fn windows(&mut self) -> Vec<WindowSpec> {
        if self.second_open {
            let spec = WindowSpec::new(SECOND, "Second window", [480.0, 320.0]);
            vec![match self.place {
                Some(at) => spec.at(at),
                None => spec,
            }]
        } else {
            Vec::new()
        }
    }

    fn window_ui(&mut self, key: &str, ui: &mut egui::Ui, _host: &mut DesktopHost) {
        assert_eq!(key, SECOND);
        self.second_frames += 1;
        if std::env::var_os("TWO_TRACE").is_some() {
            eprintln!("second frame {}", self.second_frames);
        }
        ui.label(format!("second window, frame {}", self.second_frames));
    }

    fn timer(&mut self, _host: &mut DesktopHost) -> bool {
        self.ticks += 1;
        let elapsed = self.started.map_or(0.0, |at| at.elapsed().as_secs_f32());
        if std::env::var_os("TWO_TRACE").is_some() {
            eprintln!("timer at {elapsed:.2}");
        }
        if elapsed > 2.0 && self.quiet.is_none() {
            self.quiet = Some((self.main_frames, self.second_frames));
        }
        if elapsed > 3.0
            && let Some((main, second)) = self.quiet
            && !self.report.iter().any(|line| line.starts_with("quiet"))
        {
            self.report.push(format!(
                "quiet second: main drew {}, second drew {}",
                self.main_frames - main,
                self.second_frames - second
            ));
        }
        // The main window draws to close the second and to quit.
        elapsed > 4.0
    }

    fn pacing(&mut self, _now: Instant) -> FramePacing {
        // The timer runs every tenth of a second.
        let started = self.started.unwrap_or_else(Instant::now);
        let next = started + Duration::from_millis(100) * (self.ticks + 1);
        FramePacing::WakeAt(next)
    }

    fn window_close_requested(&mut self, key: &str, _host: &mut DesktopHost) {
        if key == SECOND {
            self.second_open = false;
        }
    }

    fn exit(&mut self, _host: &mut DesktopHost) {
        for line in &self.report {
            println!("{line}");
        }
        println!(
            "second window: {:?} points at scale {:?}, asked at {:?}, was at {:?}; screens {}",
            self.second_size, self.second_scale, self.place, self.second_at, self.screens
        );
    }
}

fn main() {
    let options = DesktopOptions {
        title: "Sue".to_owned(),
        width: 640.0,
        height: 480.0,
        gpu: GpuPreference::default(),
        icon: None,
        app_id: String::new(),
        decorations: true,
    };
    if let Err(error) = run_desktop(Two::default(), options) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
