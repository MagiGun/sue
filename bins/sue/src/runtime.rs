//! The shell runtime shared by the live window and offscreen snapshots:
//! input folding, frame building and pacing, timers, engine commands, file
//! opening and persistence.
//!
//! Pacing (LAW 4): frames keep coming while a spring moves, while damage is
//! waiting, and for a short riced grace period after any change, so an
//! animation started by a click runs to the end with no further input. Then
//! the scene sleeps at 0 FPS, waking only for timed work: a caret blink, the
//! player's audio queue, a status message to clear, a pending autosave.

use std::collections::VecDeque;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sue_core::StringBuffer;
use sue_io::{IngestOptions, NativeJxlPipeline, ingest_bytes, meta};
use sue_platform::{
    AudioOutput, AudioSink, CursorShape, FileDrop, FileFilter, FramePacing, InputEvent, KeyEvent,
    MouseEvent, NamedKey, SafeAreaInsets, write_atomic,
};
use sue_rice::{RiceSchema, SetRiceValues};
use sue_wm::{
    Canvas, Dialog, Fonts, InputResponse, Rect, RiceEdit, ShellFrame, ShellRequest, Theme, Ui,
    UiInput, UiState, apply_actions, draw_shell,
};

use crate::engine::{Engine, NOTE_NAME};
use crate::util::{file_name, human_bytes, media_class};

/// Frame-rate telemetry for the status bar.
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
}

/// One built frame.
#[derive(Debug)]
pub struct FrameOutput {
    /// What to draw.
    pub canvas: Canvas,
    /// Pointer shape requested by the widgets under the pointer.
    pub cursor: Option<CursorShape>,
}

/// What an open dialog decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Question {
    Quit,
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
    catalog_fingerprint: String,
    audio_config: Option<sue_platform::AudioConfig>,
    last_frame: Option<Instant>,
    animating: bool,
    last_change: Option<Instant>,
    caret_epoch: Instant,
    caret_drawn: bool,
    caret_shown: bool,
    next_tick: Option<Instant>,
    message: Option<(String, bool, Instant)>,
    dialog: Option<(Dialog, Question)>,
    quit: bool,
    interactive: bool,
    note_saved: Option<u64>,
    next_autosave: Option<Instant>,
}

fn text_hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Values whose change means the Settings catalog must be rebuilt.
fn catalog_fingerprint(rice: &RiceSchema) -> String {
    [
        "ai.model_dir",
        "ai.backend",
        "ai.vram_budget_mib",
        "packages.require_signatures",
        "profile.hardware",
        "display.frame_rate_cap",
    ]
    .iter()
    .map(|path| {
        rice.get(path)
            .map_or_else(String::new, |value| value.to_string())
    })
    .collect::<Vec<_>>()
    .join("\u{1F}")
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
        let fingerprint = catalog_fingerprint(&engine.world.rice);
        let note = engine
            .world
            .arena
            .iter()
            .find(|(_, entity)| entity.name() == NOTE_NAME)
            .and_then(|(_, entity)| entity.string_buffer())
            .map(|text| text_hash(text.as_str()));
        let mut state = UiState::default();
        state.caret_visible = true;
        Ok(Self {
            engine,
            fonts,
            theme,
            input: UiInput::default(),
            state,
            telemetry: Telemetry::default(),
            applied_revision: revision,
            saved_revision: revision,
            catalog_fingerprint: fingerprint,
            audio_config: None,
            last_frame: None,
            animating: false,
            last_change: None,
            caret_epoch: Instant::now(),
            caret_drawn: false,
            caret_shown: true,
            next_tick: None,
            message: None,
            dialog: None,
            quit: false,
            interactive: false,
            note_saved: note,
            next_autosave: None,
        })
    }

    /// Lets the runtime use the OS: native dialogs, the browser, audio.
    #[must_use]
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    /// The current theme.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// `true` once the user confirmed quitting.
    #[must_use]
    pub fn wants_exit(&self) -> bool {
        self.quit
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
            self.notify(format!("Settings: {error}"), true);
        }
        self.theme = Theme::from_rice(&self.engine.world.rice);
        self.fonts.set_ui_font(&self.theme.ui_font);
        let fingerprint = catalog_fingerprint(&self.engine.world.rice);
        if fingerprint != self.catalog_fingerprint {
            self.catalog_fingerprint = fingerprint;
            self.engine.rebuild_catalog();
        }
        // An open audio stream follows its settings.
        if self.engine.audio.is_some()
            && self.audio_config.as_ref() != Some(&self.engine.audio_config())
        {
            self.open_audio();
        }
    }

    fn notify(&mut self, text: impl Into<String>, error: bool) {
        let duration = self.engine.workspace.config().message_duration;
        self.message = Some((text.into(), error, Instant::now() + duration));
        self.engine.workspace.invalidate();
    }

    fn caret_phase(&self, now: Instant) -> (bool, Option<Instant>) {
        match self.engine.workspace.config().caret_blink {
            Some(half) => {
                let half_ms = half.as_millis().max(1);
                let elapsed = now.duration_since(self.caret_epoch).as_millis();
                let phase = elapsed / half_ms;
                let next = self.caret_epoch
                    + Duration::from_millis(
                        u64::try_from((phase + 1) * half_ms).unwrap_or(u64::MAX),
                    );
                (phase.is_multiple_of(2), Some(next))
            }
            None => (true, None),
        }
    }

    /// Folds one event in. Returns `true` if the scene must be redrawn; plain
    /// pointer motion only redraws when the hovered widget changes (LAW 4).
    pub fn handle(&mut self, event: &InputEvent) -> bool {
        let now = Instant::now();
        if let Some(answer) = self.dialog_key(event) {
            self.answer(answer);
            self.last_change = Some(now);
            return true;
        }
        let redraw = match event {
            InputEvent::Mouse(MouseEvent::Moved { position }) => {
                let changes = self.state.hover_changes(Some(*position));
                self.input.apply(event);
                changes || self.input.down
            }
            InputEvent::Mouse(_) => {
                self.caret_epoch = now;
                self.input.apply(event)
            }
            InputEvent::Window(_) => true,
            InputEvent::FileDrop(drop) => {
                self.input.apply(event);
                if matches!(drop, FileDrop::Dropped(_)) {
                    self.dispatch(event);
                }
                true
            }
            InputEvent::Keyboard(_) => {
                self.caret_epoch = now;
                self.dispatch(event)
            }
            _ => self.dispatch(event),
        };
        self.serve_requests();
        self.refresh_rice();
        self.track_note(now);
        if redraw {
            self.last_change = Some(now);
        }
        redraw
    }

    fn dispatch(&mut self, event: &InputEvent) -> bool {
        if self.dialog.is_some() {
            return false;
        }
        let (workspace, mut ctx) = self.engine.shell();
        match workspace.dispatch(event, &mut ctx) {
            Ok(response) => response != InputResponse::Ignored,
            Err(error) => {
                self.notify(error.to_string(), true);
                true
            }
        }
    }

    /// Enter and Escape answer an open dialog.
    fn dialog_key(&self, event: &InputEvent) -> Option<bool> {
        self.dialog.as_ref()?;
        match event {
            InputEvent::Keyboard(KeyEvent {
                key: sue_platform::Key::Named(NamedKey::Enter),
                pressed: true,
                ..
            }) => Some(true),
            InputEvent::Keyboard(KeyEvent {
                key: sue_platform::Key::Named(NamedKey::Escape),
                pressed: true,
                ..
            }) => Some(false),
            _ => None,
        }
    }

    fn answer(&mut self, yes: bool) {
        if let Some((_, question)) = self.dialog.take() {
            match (question, yes) {
                (Question::Quit, true) => self.quit = true,
                (Question::Quit, false) => {}
            }
        }
        self.engine.workspace.invalidate();
    }

    /// Asks to quit, with a confirmation when the rice wants one.
    pub fn request_quit(&mut self) {
        let confirm = self
            .engine
            .world
            .rice
            .get_bool("general.confirm_on_exit")
            .unwrap_or(true);
        if confirm {
            self.dialog = Some((
                Dialog {
                    title: "Quit Sue?".to_owned(),
                    body: "Your notes and settings are saved. Open tabs close.".to_owned(),
                    confirm: "Quit".to_owned(),
                    cancel: "Cancel".to_owned(),
                },
                Question::Quit,
            ));
            self.engine.workspace.invalidate();
        } else {
            self.quit = true;
        }
    }

    /// The window's close button: `true` to close now.
    pub fn close_requested(&mut self) -> bool {
        if self.quit {
            return true;
        }
        self.request_quit();
        self.quit
    }

    /// Builds one frame for a `viewport` in logical pixels.
    pub fn frame(
        &mut self,
        viewport: Rect,
        safe_area: SafeAreaInsets,
        now: Instant,
    ) -> FrameOutput {
        let target_hz = self.engine.workspace.target_hz().max(1);
        // Real elapsed time, clamped so a stall never jumps an animation; the
        // first frame after sleeping advances by one nominal frame.
        let dt = match self.last_frame {
            Some(last) if self.animating => {
                now.duration_since(last).as_secs_f32().clamp(0.0001, 0.05)
            }
            _ => 1.0 / target_hz as f32,
        };
        self.last_frame = Some(now);
        let (visible, _) = self.caret_phase(now);
        self.state.caret_visible = visible;
        self.refresh_rice();
        self.expire_message(now);
        self.engine.workspace.begin_frame();
        let demand = {
            let (workspace, mut ctx) = self.engine.shell();
            let tick = workspace.tick(now, &mut ctx);
            self.next_tick = tick.wake;
            workspace.update(dt, &mut ctx)
        };
        let fps = if self.animating {
            self.telemetry.record(now);
            Some(self.telemetry.fps().max(1))
        } else {
            self.telemetry.reset();
            None
        };

        self.state.begin_frame();
        let mut canvas = Canvas::new(viewport);
        let message = self
            .message
            .as_ref()
            .map(|(text, error, _)| (text.clone(), *error));
        let dialog = self.dialog.as_ref().map(|(dialog, _)| dialog.clone());
        let mut answer = None;
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
                fps,
                message: message
                    .as_ref()
                    .map(|(text, error)| (text.as_str(), *error)),
                dialog: dialog.as_ref(),
            };
            let actions = draw_shell(workspace, &mut ui, &mut ctx, &frame);
            let (answers, actions): (Vec<_>, Vec<_>) = actions
                .into_iter()
                .partition(|action| matches!(action, sue_wm::ShellAction::DialogAnswer(_)));
            if let Some(sue_wm::ShellAction::DialogAnswer(yes)) = answers.first() {
                answer = Some(*yes);
            }
            if let Err(error) = apply_actions(workspace, &mut ctx, actions) {
                ctx.requests.push(ShellRequest::Notify(error.to_string()));
            }
        }
        self.state.end_frame(&self.input);
        self.input.end_frame();
        if let Some(yes) = answer {
            self.answer(yes);
        }
        self.serve_requests();
        self.refresh_rice();
        self.track_note(now);
        self.caret_drawn = self.state.caret_drawn;
        self.caret_shown = self.state.caret_visible;
        self.animating = self.engine.workspace.is_animating() || demand.is_active();
        if self.animating || self.engine.workspace.has_damage() {
            self.last_change = Some(now);
        }
        FrameOutput {
            canvas,
            cursor: self.state.cursor,
        }
    }

    /// How the host should pace what comes next.
    #[must_use]
    pub fn pacing(&self, now: Instant) -> FramePacing {
        if self.quit {
            return FramePacing::Sleep;
        }
        let target_hz = self.engine.workspace.target_hz();
        if self.animating || self.engine.workspace.has_damage() {
            return FramePacing::Animate { target_hz };
        }
        let grace = self.engine.workspace.config().settle_grace;
        if self.last_change.is_some_and(|change| now < change + grace) {
            return FramePacing::Animate { target_hz };
        }
        let caret = if self.caret_drawn {
            self.caret_phase(now).1
        } else {
            None
        };
        [
            caret,
            self.message.as_ref().map(|(_, _, until)| *until),
            self.next_tick,
            self.next_autosave,
        ]
        .into_iter()
        .flatten()
        .min()
        .map_or(FramePacing::Sleep, FramePacing::WakeAt)
    }

    /// Runs timed work that is due. Returns `true` if the scene must be
    /// redrawn.
    pub fn timer(&mut self, now: Instant) -> bool {
        let mut redraw = false;
        if self.caret_drawn && self.caret_phase(now).0 != self.caret_shown {
            redraw = true;
        }
        redraw |= self.expire_message(now);
        if self.next_autosave.is_some_and(|due| due <= now) {
            self.save_note();
        }
        if self.next_tick.is_some_and(|due| due <= now) {
            let (workspace, mut ctx) = self.engine.shell();
            let tick = workspace.tick(now, &mut ctx);
            self.next_tick = tick.wake;
            redraw |= tick.redraw;
            self.serve_requests();
        }
        redraw
    }

    fn expire_message(&mut self, now: Instant) -> bool {
        if self
            .message
            .as_ref()
            .is_some_and(|(_, _, until)| *until <= now)
        {
            self.message = None;
            self.engine.workspace.invalidate();
            return true;
        }
        false
    }

    /// Notices note edits and schedules an autosave.
    fn track_note(&mut self, now: Instant) {
        let seconds = self
            .engine
            .world
            .rice
            .get_i64("general.autosave_seconds")
            .unwrap_or(30);
        let current = self.note_text().map(|text| text_hash(&text));
        if current != self.note_saved && self.next_autosave.is_none() && seconds > 0 {
            self.next_autosave =
                Some(now + Duration::from_secs(u64::try_from(seconds).unwrap_or(30)));
        }
    }

    fn note_text(&self) -> Option<String> {
        self.engine
            .world
            .arena
            .iter()
            .find(|(_, entity)| entity.name() == NOTE_NAME)
            .and_then(|(_, entity)| entity.string_buffer())
            .map(|text| StringBuffer::as_str(text).to_owned())
    }

    /// Writes the Quick Note if it changed since the last save.
    pub fn save_note(&mut self) {
        self.next_autosave = None;
        let Some(text) = self.note_text() else {
            return;
        };
        let hash = text_hash(&text);
        if Some(hash) == self.note_saved {
            return;
        }
        let path = self.engine.note_path();
        match write_atomic(&path, text.as_bytes()) {
            Ok(()) => self.note_saved = Some(hash),
            Err(error) => self.notify(format!("Could not save the note: {error}"), true),
        }
    }

    /// Saves what must survive: the note and the rice.
    pub fn shutdown(&mut self) {
        self.save_note();
        if let Some(Err(error)) = self.persist() {
            eprintln!("sue: could not save the rice: {error}");
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
        Some(write_atomic(&path, self.engine.world.rice.to_suerice().as_bytes()).map(|()| path))
    }

    /// Carries out what applets and the shell asked the engine for.
    fn serve_requests(&mut self) {
        for _ in 0..8 {
            let mut requests = self.engine.workspace.take_outbox();
            requests.append(&mut self.engine.requests);
            if requests.is_empty() {
                return;
            }
            for request in requests {
                match request {
                    ShellRequest::OpenFile(path) => self.open_path(&path),
                    ShellRequest::OpenUrl(url) => self.open_url(&url),
                    ShellRequest::Notify(text) => self.notify(text, false),
                    ShellRequest::Command(command) => self.run_command(&command),
                    other => self.engine.requests.push(other),
                }
            }
            // Anything left (an asset to open) goes back through the workspace.
            let (workspace, mut ctx) = self.engine.shell();
            if let Err(error) = workspace.process_requests(&mut ctx) {
                let text = error.to_string();
                self.notify(text, true);
            }
        }
    }

    fn open_url(&mut self, url: &str) {
        if !self.interactive {
            self.notify(format!("Would open {url} in the system browser"), false);
            return;
        }
        match sue_platform::open_url(url) {
            Ok(url) => self.notify(format!("Opened {url} in your browser"), false),
            Err(error) => self.notify(format!("Cannot open {url}: {error}"), true),
        }
    }

    fn run_command(&mut self, command: &str) {
        match command {
            "file.open" => self.pick_and_open(),
            "file.save" => {
                let (workspace, mut ctx) = self.engine.shell();
                match workspace.command_active("save", &mut ctx) {
                    Ok(true) => {}
                    Ok(false) => self.notify("This tab has nothing to save", false),
                    Err(error) => self.notify(error.to_string(), true),
                }
            }
            "file.save_as" => self.save_as(),
            "app.quit" => self.request_quit(),
            "dialog.confirm" => self.answer(true),
            "dialog.cancel" => self.answer(false),
            "rice.reload" => self.reload_rice(),
            "rice.export" => self.export_rice(),
            "rice.reveal" => self.reveal(&self.engine.paths.rice_dir()),
            "ai.rescan" => {
                self.engine.rebuild_catalog();
                let found = self
                    .engine
                    .catalog
                    .tables
                    .get("models")
                    .map_or(0, |table| table.rows.len());
                self.notify(format!("Found {found} model files"), false);
            }
            "ai.reveal" => {
                let dir = self.engine.model_dir();
                let _ = std::fs::create_dir_all(&dir);
                self.reveal(&dir);
            }
            "audio.start" => self.open_audio(),
            other => match other.strip_prefix("reveal:") {
                Some(path) => self.reveal(Path::new(path)),
                None => self.notify(format!("Unknown command {other}"), true),
            },
        }
    }

    fn reveal(&mut self, dir: &Path) {
        if !self.interactive {
            self.notify(format!("Folder: {}", dir.display()), false);
            return;
        }
        if let Err(error) = sue_platform::reveal_folder(dir) {
            self.notify(format!("Cannot open the folder: {error}"), true);
        }
    }

    fn pick_and_open(&mut self) {
        if !self.interactive {
            self.notify("Choosing files needs the desktop window", false);
            return;
        }
        let start = self.engine.paths.home_dir.clone();
        if let Some(path) = sue_platform::pick_open_file("Open File", Some(&start), &[]) {
            self.open_path(&path);
        }
    }

    fn save_as(&mut self) {
        if !self.interactive {
            self.notify("Saving to a new file needs the desktop window", false);
            return;
        }
        let name = self
            .engine
            .workspace
            .tab_title(self.engine.workspace.active_index(), &self.engine.world);
        let start = self.engine.paths.home_dir.clone();
        if let Some(path) = sue_platform::pick_save_file("Save As", Some(&start), &name, &[]) {
            let (workspace, mut ctx) = self.engine.shell();
            if let Err(error) =
                workspace.command_active(&format!("save_as:{}", path.display()), &mut ctx)
            {
                self.notify(error.to_string(), true);
            }
        }
    }

    /// Reads a file and opens it in the applet that fits its content.
    pub fn open_path(&mut self, path: &Path) {
        let name = file_name(path);
        let limit_mib = self
            .engine
            .world
            .rice
            .get_i64("io.max_ingest_mib")
            .unwrap_or(512)
            .max(1);
        let limit = u64::try_from(limit_mib)
            .unwrap_or(u64::MAX)
            .saturating_mul(1024 * 1024);
        let bytes = match std::fs::metadata(path) {
            Ok(metadata) if metadata.is_dir() => Err("folders open in the Files panel".to_owned()),
            Ok(metadata) if metadata.len() > limit => Err(format!(
                "it is {}, over the {limit_mib} MiB limit (io.max_ingest_mib)",
                human_bytes(metadata.len())
            )),
            Ok(_) => std::fs::read(path).map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(reason) => {
                self.notify(format!("Cannot open {name}: {reason}"), true);
                return;
            }
        };
        let width = self
            .engine
            .world
            .rice
            .get_i64("io.byte_map_width")
            .and_then(|width| u32::try_from(width).ok())
            .unwrap_or(64);
        let ingested = match ingest_bytes(
            &mut self.engine.world.arena,
            &name,
            bytes,
            &NativeJxlPipeline,
            IngestOptions {
                byte_map_width: width,
            },
        ) {
            Ok(ingested) => ingested,
            Err(error) => {
                self.notify(format!("Cannot open {name}: {error}"), true);
                return;
            }
        };
        if let Ok(entity) = self.engine.world.arena.get_mut(ingested.handle) {
            entity.set_meta(meta::SOURCE, path.display().to_string());
        }
        let class = media_class(ingested.view);
        let (workspace, mut ctx) = self.engine.shell();
        let opened = workspace.open_asset(ingested.handle, class, &mut ctx);
        match opened {
            Ok(()) => {
                if let Some(note) = ingested.note {
                    self.notify(format!("{name}: {note}"), false);
                }
            }
            Err(error) => self.notify(format!("Cannot open {name}: {error}"), true),
        }
        self.serve_requests();
    }

    /// Opens (or reopens) the audio output with the riced device and format.
    fn open_audio(&mut self) {
        if !self.interactive {
            self.engine.audio_error = Some("there is no audio device in this mode".to_owned());
            self.engine.rebuild_catalog();
            return;
        }
        let config = self.engine.audio_config();
        self.engine.audio = None;
        match AudioOutput::open(&config) {
            Ok(output) => {
                let mut sink: Box<dyn AudioSink> = Box::new(output);
                sink.set_volume(
                    self.engine
                        .world
                        .rice
                        .get_f64("audio.volume")
                        .unwrap_or(0.8) as f32,
                );
                self.engine.audio = Some(sink);
                self.engine.audio_error = None;
            }
            Err(error) => {
                self.notify(format!("Audio: {error}"), true);
                self.engine.audio_error = Some(error.to_string());
            }
        }
        self.audio_config = Some(config);
        self.engine.rebuild_catalog();
    }

    /// Re-reads the rice file and applies what differs as one undoable step.
    fn reload_rice(&mut self) {
        let path = self.engine.rice_path();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                self.notify(format!("Cannot read {}: {error}", path.display()), true);
                return;
            }
        };
        let disk = match RiceSchema::parse(&text) {
            Ok(disk) => disk,
            Err(error) => {
                self.notify(format!("The rice file has an error: {error}"), true);
                return;
            }
        };
        let changes: Vec<(String, sue_rice::RiceValue)> = disk
            .entries()
            .iter()
            .filter_map(|entry| {
                let value = entry.parameter().value();
                let current = self.engine.world.rice.get(entry.path())?;
                (current != value).then(|| (entry.path().to_owned(), value))
            })
            .collect();
        if changes.is_empty() {
            self.notify("The rice file matches what Sue shows", false);
            return;
        }
        let count = changes.len();
        let txn = SetRiceValues::new("Reload default.suerice", changes);
        match self
            .engine
            .history
            .apply(&mut self.engine.world, RiceEdit(txn))
        {
            Ok(_) => self.notify(format!("Reloaded {count} values from disk"), false),
            Err(error) => self.notify(format!("Reload failed: {error}"), true),
        }
        self.refresh_rice();
    }

    fn export_rice(&mut self) {
        if !self.interactive {
            self.notify("Exporting needs the desktop window", false);
            return;
        }
        let filters = [FileFilter {
            name: "Rice files".to_owned(),
            extensions: vec!["suerice".to_owned()],
        }];
        let start = self.engine.paths.home_dir.clone();
        if let Some(path) =
            sue_platform::pick_save_file("Export Rice", Some(&start), "my.suerice", &filters)
        {
            let text = self.engine.world.rice.to_suerice();
            match write_atomic(&path, text.as_bytes()) {
                Ok(()) => self.notify(format!("Exported the rice to {}", path.display()), false),
                Err(error) => self.notify(format!("Export failed: {error}"), true),
            }
        }
    }

    /// Where widget `id` was drawn in the latest frame.
    #[must_use]
    pub fn widget_rect(&self, id: sue_wm::WidgetId) -> Option<Rect> {
        self.state.hit_rect(id)
    }

    /// Shows the dock applet `id` (for snapshots and scripted runs).
    pub fn show_dock(&mut self, id: &str) {
        if let Ok(id) = sue_wm::AppletId::new(id) {
            self.engine.workspace.show_dock_applet(&id);
        }
    }

    /// Opens Settings at `page` (for snapshots and scripted runs).
    pub fn open_settings(&mut self, page: Option<&str>) {
        let (workspace, mut ctx) = self.engine.shell();
        let _ = workspace.open_settings(page, &mut ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::temp_host;
    use sue_platform::{Key, Modifiers, MouseButton, Point};
    use sue_rice::{DEFAULT_RICE, RiceValue, SetRiceValue};
    use sue_wm::{RiceEdit, widget_id};

    fn runtime(tag: &str) -> (Runtime, PathBuf) {
        let (host, root) = temp_host(tag);
        let runtime = Runtime::new(Engine::boot(&host).unwrap()).unwrap();
        (runtime, root)
    }

    const VIEWPORT: Rect = Rect::new(0.0, 0.0, 1280.0, 800.0);

    fn click(runtime: &mut Runtime, point: Point, now: &mut Instant) {
        for pressed in [true, false] {
            runtime.handle(&InputEvent::Mouse(MouseEvent::Button {
                button: MouseButton::Left,
                pressed,
                position: point,
            }));
            *now += Duration::from_millis(16);
            runtime.frame(VIEWPORT, SafeAreaInsets::ZERO, *now);
        }
    }

    #[test]
    fn a_clicked_dock_toggle_animates_to_the_end_without_more_input() {
        let (mut runtime, root) = runtime("pacing");
        runtime
            .engine
            .workspace
            .set_profile(sue_rice::HardwareProfile::Balanced);
        let mut now = Instant::now();
        runtime.frame(VIEWPORT, SafeAreaInsets::ZERO, now);
        let toggle = runtime
            .state
            .hit_rect(widget_id("shell", "toggle-left"))
            .expect("the left dock toggle is drawn");
        let center = Point::new(
            toggle.x + toggle.width / 2.0,
            toggle.y + toggle.height / 2.0,
        );
        runtime.handle(&InputEvent::Mouse(MouseEvent::Moved { position: center }));
        click(&mut runtime, center, &mut now);
        assert!(
            !runtime.engine.workspace.docks_open().0,
            "the click closed the dock"
        );

        // No input from here on: the host only asks for pacing and frames.
        let mut frames = 0;
        while matches!(runtime.pacing(now), FramePacing::Animate { .. }) {
            now += Duration::from_millis(16);
            runtime.frame(VIEWPORT, SafeAreaInsets::ZERO, now);
            frames += 1;
            assert!(frames < 2_000, "the animation never settled");
        }
        assert_eq!(
            runtime.engine.workspace.dock_reveal().0,
            0.0,
            "ran to the end"
        );
        assert!(
            frames > 10,
            "animated over many frames, not one jump ({frames})"
        );
        assert!(!matches!(runtime.pacing(now), FramePacing::Animate { .. }));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn idle_scenes_sleep_after_the_grace_period() {
        let (mut runtime, root) = runtime("idle");
        let now = Instant::now();
        runtime.frame(VIEWPORT, SafeAreaInsets::ZERO, now);
        let later = now + Duration::from_secs(5);
        runtime.frame(VIEWPORT, SafeAreaInsets::ZERO, later);
        assert_eq!(
            runtime.pacing(later + Duration::from_secs(1)),
            FramePacing::Sleep
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rice_edits_persist_byte_cleanly_and_undo_restores_the_file() {
        let (mut runtime, root) = runtime("persist");
        assert!(runtime.persist().is_none(), "nothing changed yet");
        let engine = &mut runtime.engine;
        engine
            .history
            .apply(
                &mut engine.world,
                RiceEdit(SetRiceValue::new("layout.padding", RiceValue::Int(13))),
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
        assert!(changed[0].1.starts_with("padding = 13 "));

        let engine = &mut runtime.engine;
        engine.history.undo(&mut engine.world).unwrap();
        runtime.persist().unwrap().unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dropped_files_open_and_quitting_asks_first() {
        let (mut runtime, root) = runtime("drop");
        let file = root.join("notes.md");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&file, "# Sue\n").unwrap();
        runtime.handle(&InputEvent::FileDrop(FileDrop::Dropped(file.clone())));
        let workspace = &runtime.engine.workspace;
        assert_eq!(
            workspace.tab_title(workspace.active_index(), &runtime.engine.world),
            "notes.md"
        );
        let quit = InputEvent::Keyboard(KeyEvent::press(Key::Character('q'), Modifiers::CTRL));
        runtime.handle(&quit);
        assert!(!runtime.wants_exit(), "a dialog asks first");
        runtime.handle(&InputEvent::Keyboard(KeyEvent::press(
            Key::Named(NamedKey::Escape),
            Modifiers::NONE,
        )));
        assert!(!runtime.wants_exit());
        runtime.handle(&quit);
        runtime.handle(&InputEvent::Keyboard(KeyEvent::press(
            Key::Named(NamedKey::Enter),
            Modifiers::NONE,
        )));
        assert!(runtime.wants_exit());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn notes_autosave_once_they_change() {
        let (mut runtime, root) = runtime("autosave");
        let now = Instant::now();
        runtime.show_dock(crate::quick_notes::ID);
        for ch in "hi".chars() {
            runtime.handle(&InputEvent::Keyboard(KeyEvent::typed(ch)));
        }
        runtime.frame(VIEWPORT, SafeAreaInsets::ZERO, now);
        let FramePacing::WakeAt(due) = runtime.pacing(now + Duration::from_secs(2)) else {
            panic!("an autosave is scheduled");
        };
        assert!(due > now);
        runtime.timer(now + Duration::from_secs(31));
        assert_eq!(
            std::fs::read_to_string(runtime.engine.note_path()).unwrap(),
            "hi"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn telemetry_counts_frames_over_the_last_second() {
        let mut telemetry = Telemetry::default();
        let start = Instant::now();
        for frame in 0..=60 {
            telemetry.record(start + Duration::from_millis(frame * 1000 / 60));
        }
        assert!((59..=61).contains(&telemetry.fps()), "{}", telemetry.fps());
        telemetry.reset();
        assert_eq!(telemetry.fps(), 0);
    }
}
