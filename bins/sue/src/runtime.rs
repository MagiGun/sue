//! The shell runtime shared by the live window and offscreen snapshots:
//! input folding, frame building, telemetry and rice persistence.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use sue_platform::{CursorShape, FileDrop, InputEvent, MouseEvent, SafeAreaInsets};
use sue_rice::HardwareProfile;
use sue_wm::{
    Canvas, Fonts, InputResponse, Rect, RenderDemand, ShellFrame, Theme, Ui, UiInput, UiState,
    apply_actions, draw_shell,
};

use crate::engine::Engine;

/// Frame-rate telemetry for the top-bar badge.
#[derive(Debug, Default)]
pub struct Telemetry {
    frames: VecDeque<Instant>,
}

impl Telemetry {
    /// Records an animated frame.
    pub fn record(&mut self, now: Instant) {
        self.frames.push_back(now);
        while self.frames.len() > 2
            && self
                .frames
                .front()
                .is_some_and(|first| now.duration_since(*first) > Duration::from_secs(1))
        {
            self.frames.pop_front();
        }
    }

    /// Forgets the frame history (the scene went to sleep).
    pub fn reset(&mut self) {
        self.frames.clear();
    }

    /// Frames per second over the last second of animation.
    #[must_use]
    pub fn fps(&self) -> u32 {
        match (self.frames.front(), self.frames.back()) {
            (Some(first), Some(last)) if self.frames.len() > 1 => {
                let span = last.duration_since(*first).as_secs_f64().max(f64::EPSILON);
                ((self.frames.len() - 1) as f64 / span).round() as u32
            }
            _ => 0,
        }
    }

    /// `Render: SLEEP (0 FPS) · balanced` or `Render: ACTIVE (144 FPS) · …`.
    #[must_use]
    pub fn label(&self, sleeping: bool, profile: HardwareProfile) -> String {
        if sleeping {
            format!("Render: SLEEP (0 FPS) · {profile}")
        } else {
            format!("Render: ACTIVE ({} FPS) · {profile}", self.fps())
        }
    }
}

/// One built frame.
#[derive(Debug)]
pub struct FrameOutput {
    /// What to draw.
    pub canvas: Canvas,
    /// Whether more frames are needed.
    pub next: RenderDemand,
    /// Pointer shape requested by the widgets under the pointer.
    pub cursor: Option<CursorShape>,
}

/// The engine plus everything the shell needs between frames.
pub struct Runtime {
    /// Every subsystem.
    pub engine: Engine,
    /// The bundled fonts.
    pub fonts: Fonts,
    theme: Theme,
    input: UiInput,
    state: UiState,
    telemetry: Telemetry,
    applied_revision: u64,
    saved_revision: u64,
    last_frame: Option<Instant>,
}

impl Runtime {
    /// Wraps a booted engine.
    ///
    /// # Errors
    /// If the bundled fonts fail to load.
    pub fn new(engine: Engine) -> Result<Self, String> {
        let mut fonts = Fonts::new()?;
        let theme = Theme::from_rice(&engine.world.rice);
        fonts.set_ui_font(&theme.ui_font);
        let revision = engine.world.rice.revision();
        Ok(Self {
            engine,
            fonts,
            theme,
            input: UiInput::default(),
            state: UiState::default(),
            telemetry: Telemetry::default(),
            applied_revision: revision,
            saved_revision: revision,
            last_frame: None,
        })
    }

    /// The current theme.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Re-applies config, profile, theme and font after any rice change
    /// (an edit, a live slider preview, an undo or a redo).
    fn refresh_rice(&mut self) {
        let revision = self.engine.world.rice.revision();
        if revision == self.applied_revision {
            return;
        }
        self.applied_revision = revision;
        if let Err(error) = self.engine.refresh_config() {
            eprintln!("sue: {error}");
        }
        self.theme = Theme::from_rice(&self.engine.world.rice);
        self.fonts.set_ui_font(&self.theme.ui_font);
    }

    fn dispatch(&mut self, event: &InputEvent) -> bool {
        let (workspace, mut ctx) = self.engine.shell();
        match workspace.dispatch(event, &mut ctx) {
            Ok(response) => response != InputResponse::Ignored,
            Err(error) => {
                eprintln!("sue: {error}");
                true
            }
        }
    }

    /// Folds one event in. Returns `true` if the scene must be redrawn; plain
    /// pointer motion only redraws when the hovered widget changes (LAW 4).
    pub fn handle(&mut self, event: &InputEvent) -> bool {
        let redraw = match event {
            InputEvent::Mouse(MouseEvent::Moved { position }) => {
                let changes = self.state.hover_changes(Some(*position));
                self.input.apply(event);
                changes || self.input.down
            }
            InputEvent::Mouse(_) => self.input.apply(event),
            InputEvent::Window(_) => true,
            InputEvent::FileDrop(drop) => {
                self.input.apply(event);
                if matches!(drop, FileDrop::Dropped(_)) {
                    self.dispatch(event);
                }
                true
            }
            _ => self.dispatch(event),
        };
        self.refresh_rice();
        redraw
    }

    /// Builds one frame for a `viewport` in logical pixels.
    pub fn frame(
        &mut self,
        viewport: Rect,
        safe_area: SafeAreaInsets,
        now: Instant,
    ) -> FrameOutput {
        let dt = self
            .last_frame
            .map_or(0.0, |last| now.duration_since(last).as_secs_f32());
        self.last_frame = Some(now);
        self.refresh_rice();
        {
            let (workspace, mut ctx) = self.engine.shell();
            workspace.update(dt, &mut ctx);
        }
        let animating = self.engine.workspace.scheduler().animating();
        if animating {
            self.telemetry.record(now);
        } else {
            self.telemetry.reset();
        }
        let label = self
            .telemetry
            .label(!animating, self.engine.workspace.profile());

        self.state.begin_frame();
        let mut canvas = Canvas::new(viewport);
        {
            let (workspace, mut ctx) = self.engine.shell();
            let form_factor = ctx.form_factor;
            let mut ui = Ui {
                canvas: &mut canvas,
                fonts: &self.fonts,
                theme: &self.theme,
                input: &self.input,
                state: &mut self.state,
            };
            let frame = ShellFrame {
                viewport,
                safe_area,
                form_factor,
                telemetry: &label,
                sleeping: !animating,
            };
            let actions = draw_shell(workspace, &mut ui, &mut ctx, &frame);
            if let Err(error) = apply_actions(workspace, &mut ctx, actions) {
                eprintln!("sue: {error}");
            }
        }
        self.state.end_frame(&self.input);
        self.input.end_frame();
        self.refresh_rice();
        self.engine.workspace.begin_frame();
        FrameOutput {
            canvas,
            next: self.engine.workspace.scheduler().demand(),
            cursor: self.state.cursor,
        }
    }

    /// Writes the rice back to its file if it changed and no slider is being
    /// dragged. Untouched lines are written byte for byte.
    pub fn persist(&mut self) -> Option<std::io::Result<PathBuf>> {
        let revision = self.engine.world.rice.revision();
        if revision == self.saved_revision || self.state.active.is_some() {
            return None;
        }
        self.saved_revision = revision;
        let path = self.engine.rice_path();
        Some(
            sue_platform::write_atomic(&path, self.engine.world.rice.to_suerice().as_bytes())
                .map(|()| path),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_platform::{FormFactor, HardwareCapabilities, HeadlessHost, OsFamily, PlatformPaths};
    use sue_rice::{DEFAULT_RICE, RiceValue, SetRiceValue};
    use sue_wm::RiceEdit;

    #[test]
    fn rice_edits_persist_byte_cleanly_and_undo_restores_the_file() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let root =
            std::env::temp_dir().join(format!("sue-runtime-test-{}-{unique}", std::process::id()));
        let paths = PlatformPaths {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
            cache_dir: root.join("cache"),
        };
        let host = HeadlessHost::new(
            OsFamily::current(),
            FormFactor::Desktop,
            HardwareCapabilities::probe_portable(),
            paths,
        );
        let mut runtime = Runtime::new(Engine::boot(&host).unwrap()).unwrap();
        assert!(runtime.persist().is_none(), "nothing changed yet");

        let engine = &mut runtime.engine;
        engine
            .history
            .apply(
                &mut engine.world,
                RiceEdit(SetRiceValue::new("layout.gap", RiceValue::Int(13))),
            )
            .unwrap();
        let path = runtime.persist().unwrap().unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        let original = DEFAULT_RICE.replace("\r\n", "\n");
        let changed: Vec<(&str, &str)> = original
            .lines()
            .zip(written.lines())
            .filter(|(before, after)| before != after)
            .collect();
        assert_eq!(original.lines().count(), written.lines().count());
        assert_eq!(changed.len(), 1, "{changed:?}");
        assert!(changed[0].1.starts_with("gap = 13 "));

        let engine = &mut runtime.engine;
        engine.history.undo(&mut engine.world).unwrap();
        runtime.persist().unwrap().unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn telemetry_reports_sleep_and_frame_rate() {
        let mut telemetry = Telemetry::default();
        assert_eq!(
            telemetry.label(true, HardwareProfile::Balanced),
            "Render: SLEEP (0 FPS) · balanced"
        );
        let start = Instant::now();
        for frame in 0..=60 {
            telemetry.record(start + Duration::from_millis(frame * 1000 / 60));
        }
        assert!((58..=62).contains(&telemetry.fps()), "{}", telemetry.fps());
        assert!(
            telemetry
                .label(false, HardwareProfile::HighRefresh)
                .starts_with("Render: ACTIVE (")
        );
        telemetry.reset();
        assert_eq!(telemetry.fps(), 0);
    }
}
