//! The runtime shared by the live window and offscreen snapshots: one egui
//! frame of the shell, timers, engine commands, file opening and saving.
//!
//! Pacing: frames keep coming while a spring moves (the shell asks
//! egui for them with `request_repaint`) and for a short riced grace period
//! after any change. Then the window sleeps at 0 FPS, waking only for timed
//! work: the audio queue while something plays, a status message to clear,
//! a typing burst to close into one undo step, a pending autosave.

use std::collections::VecDeque;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Align2, Id, Order, RichText, Sense};
use sue_core::{SueFile, UndoStack};
use sue_io::{IngestOptions, IngestView, NativeJxlPipeline, ingest_bytes, meta};
use sue_platform::{AudioConfig, AudioOutput, AudioSink, FramePacing, TextureUpload, write_atomic};
use sue_rice::{RiceSchema, SetRiceValues};
use sue_wm::{
    Applets, FilePick, RiceEdit, Shell, ShellIo, ShellRequest, Theme, Transport, Typing, widgets,
};

use crate::document::{KIND, KIND_CODE};
use crate::engine::{Engine, NOTE_NAME};
use crate::util::{file_name, human_bytes};

/// How often the audio queue is topped up while something plays.
const FEED_INTERVAL: Duration = Duration::from_millis(40);

/// Extensions opened as code in Code / Hex rather than as documents.
const CODE_EXTENSIONS: [&str; 16] = [
    "rs", "py", "js", "ts", "c", "h", "cpp", "hpp", "java", "go", "toml", "json", "wgsl", "sh",
    "lua", "cs",
];

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

fn text_hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Values whose change means the Settings catalog must be rebuilt.
fn catalog_fingerprint(rice: &RiceSchema) -> String {
    [
        "packages.require_signatures",
        "display.gpu_preference",
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

/// Borrows everything a shell frame touches, field by field.
macro_rules! shell_io {
    ($runtime:expr, $textures:expr) => {
        ShellIo {
            world: &mut $runtime.engine.world,
            history: &mut $runtime.engine.history,
            typing: &mut $runtime.typing,
            applets: &mut $runtime.applets,
            catalog: &$runtime.engine.catalog,
            textures: &mut *$textures,
            audio: $runtime.engine.audio.as_deref_mut(),
            transport: &mut $runtime.transport,
            requests: &mut $runtime.engine.requests,
            fps: ($runtime.telemetry.fps() > 0).then(|| $runtime.telemetry.fps()),
        }
    };
}

/// The engine plus everything the shell needs between frames.
pub struct Runtime {
    /// Every subsystem.
    pub engine: Engine,
    /// The shell.
    pub shell: Shell,
    /// The live applets.
    pub applets: Applets,
    /// Typing waiting to become one undo step.
    pub typing: Typing,
    /// The audio transport.
    pub transport: Transport,
    telemetry: Telemetry,
    applied_revision: u64,
    saved_revision: u64,
    catalog_fingerprint: String,
    audio_config: Option<AudioConfig>,
    next_feed: Option<Instant>,
    next_tick: Option<Instant>,
    asked_for_audio: bool,
    confirm_quit: bool,
    quit: bool,
    restart: bool,
    interactive: bool,
    booted: bool,
    note_saved: Option<u64>,
    next_autosave: Option<Instant>,
    /// Where the open project lives, once saved or opened.
    project: Option<PathBuf>,
    /// The open project file, so chunks this build does not know survive.
    project_file: SueFile,
}

impl Runtime {
    /// Wraps a booted engine.
    ///
    /// # Errors
    /// Never today; kept for start-up checks.
    pub fn new(engine: Engine) -> Result<Self, String> {
        let theme = Theme::from_rice(&engine.world.rice);
        let shell = Shell::new(
            engine.config.clone(),
            theme,
            engine.profile,
            engine.capabilities.display_refresh_hz,
            engine.form_factor,
        );
        let applets = engine.launchpad.instantiate();
        let revision = engine.world.rice.revision();
        let fingerprint = catalog_fingerprint(&engine.world.rice);
        let note = engine
            .world
            .arena
            .iter()
            .find(|(_, entity)| entity.name() == NOTE_NAME)
            .and_then(|(_, entity)| entity.string_buffer())
            .map(|text| text_hash(text.as_str()));
        Ok(Self {
            engine,
            shell,
            applets,
            typing: Typing::default(),
            transport: Transport::default(),
            telemetry: Telemetry::default(),
            applied_revision: revision,
            saved_revision: revision,
            catalog_fingerprint: fingerprint,
            audio_config: None,
            next_feed: None,
            next_tick: None,
            asked_for_audio: false,
            confirm_quit: false,
            quit: false,
            restart: false,
            interactive: false,
            booted: false,
            note_saved: note,
            next_autosave: None,
            project: None,
            project_file: SueFile::new(),
        })
    }

    /// Lets the runtime use the OS: native dialogs, the file manager, audio.
    #[must_use]
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self.shell.set_interactive(interactive);
        self
    }

    /// The current theme.
    pub fn theme(&self) -> &Theme {
        self.shell.theme()
    }

    /// `true` once the user confirmed quitting.
    #[must_use]
    pub fn wants_exit(&self) -> bool {
        self.quit
    }

    /// `true` when Sue should start again after closing (Restart Sue).
    #[must_use]
    pub fn wants_restart(&self) -> bool {
        self.restart
    }

    /// Builds one frame of the whole window into the root `ui`.
    pub fn ui(&mut self, ui: &mut egui::Ui, textures: &mut dyn TextureUpload, now: Instant) {
        if !self.booted {
            self.booted = true;
            self.shell
                .set_canvas(ui.max_rect().shrink2(egui::vec2(36.0, 11.0)));
            let mut io = shell_io!(self, textures);
            self.shell.boot(&mut io, now);
        }
        {
            let mut io = shell_io!(self, textures);
            self.shell.ui(ui, &mut io, now);
        }
        if self.confirm_quit {
            self.quit_dialog(ui.ctx());
        }
        self.serve_requests(textures, now);
        self.refresh_rice();
        if self.shell.is_animating() {
            self.telemetry.record(now);
        } else {
            self.telemetry.reset();
        }
        self.track_note(now);
        if !ui.ctx().input(|input| input.pointer.any_down())
            && let Some(Err(error)) = self.persist()
        {
            self.shell
                .notify(format!("Could not save the rice: {error}"), true);
        }
    }

    fn quit_dialog(&mut self, ctx: &egui::Context) {
        let theme = self.shell.theme().clone();
        let screen = ctx.content_rect();
        egui::Area::new(Id::unique("sue-quit-scrim"))
            .order(Order::Middle)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                ui.allocate_rect(screen, Sense::click());
                ui.painter()
                    .rect_filled(screen, 0.0, theme.shadow.gamma_multiply(0.5));
            });
        let mut answer = None;
        egui::Area::new(Id::unique("sue-quit"))
            .order(Order::Foreground)
            .pivot(Align2::CENTER_CENTER)
            .fixed_pos(screen.center())
            .show(ctx, |ui| {
                widgets::window_frame(&theme).show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(16))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("Quit Sue?")
                                    .size(theme.heading_size)
                                    .color(theme.text),
                            );
                            ui.label(
                                RichText::new("Your note and settings are saved first.")
                                    .color(theme.muted),
                            );
                            ui.add_space(theme.padding);
                            ui.horizontal(|ui| {
                                if widgets::pill_button(ui, "Quit", true, &theme).clicked() {
                                    answer = Some(true);
                                }
                                if widgets::pill_button(ui, "Cancel", false, &theme).clicked() {
                                    answer = Some(false);
                                }
                            });
                        });
                });
            });
        let (enter, escape) = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::Enter),
                input.key_pressed(egui::Key::Escape),
            )
        });
        if enter {
            answer = Some(true);
        }
        if escape {
            answer = Some(false);
        }
        if let Some(yes) = answer {
            self.confirm_quit = false;
            if yes {
                self.shutdown();
                self.quit = true;
            }
            self.shell.touch();
        }
    }

    /// Re-applies config, profile and theme after any rice change (an edit,
    /// a live slider preview, an undo or a redo).
    fn refresh_rice(&mut self) {
        let revision = self.engine.world.rice.revision();
        if revision == self.applied_revision {
            return;
        }
        self.applied_revision = revision;
        if let Err(error) = self.engine.refresh_config() {
            self.shell.notify(format!("Settings: {error}"), true);
            return;
        }
        let theme = Theme::from_rice(&self.engine.world.rice);
        self.shell
            .apply_config(self.engine.config.clone(), theme, self.engine.profile);
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

    /// What the loop should do next.
    pub fn pacing(&self, now: Instant) -> FramePacing {
        if self.quit {
            return FramePacing::Sleep;
        }
        let shell = self.shell.pacing(now);
        if matches!(shell, FramePacing::Animate { .. }) {
            return shell;
        }
        let shell_wake = match shell {
            FramePacing::WakeAt(at) => Some(at),
            _ => None,
        };
        [
            shell_wake,
            self.transport
                .is_playing()
                .then_some(self.next_feed.unwrap_or(now)),
            self.next_autosave,
            self.typing.deadline(),
            self.next_tick,
        ]
        .into_iter()
        .flatten()
        .min()
        .map_or(FramePacing::Sleep, FramePacing::WakeAt)
    }

    /// Runs timed work that is due. Returns `true` if the window must be
    /// redrawn.
    pub fn timer(&mut self, textures: &mut dyn TextureUpload, now: Instant) -> bool {
        let mut redraw = self.shell.expire(now);
        if self.next_autosave.is_some_and(|due| due <= now) {
            self.save_note();
        }
        if self.typing.flush_if_paused(&mut self.engine.history, now) {
            redraw = true;
        }
        if self.transport.is_playing() && self.next_feed.is_none_or(|due| due <= now) {
            let volume = self
                .engine
                .world
                .rice
                .get_f64("audio.volume")
                .unwrap_or(0.8) as f32;
            match self.engine.audio.as_deref_mut() {
                Some(sink) => {
                    self.asked_for_audio = false;
                    self.transport.pump(&self.engine.world, sink, volume);
                }
                None if !self.asked_for_audio => {
                    self.asked_for_audio = true;
                    self.open_audio();
                }
                None => {}
            }
            self.next_feed = Some(now + FEED_INTERVAL);
            redraw = true;
        }
        if !self.transport.is_playing() {
            self.next_feed = None;
        }
        if self.next_tick.is_none_or(|due| due <= now) {
            let tick = {
                let mut io = shell_io!(self, textures);
                self.shell.tick(&mut io, now)
            };
            self.next_tick = tick.wake;
            redraw |= tick.repaint || tick.animating;
            self.serve_requests(textures, now);
        }
        redraw
    }

    /// Notices note edits and schedules an autosave.
    fn track_note(&mut self, now: Instant) {
        let seconds = self
            .engine
            .world
            .rice
            .get_i64("general.autosave_seconds")
            .unwrap_or(30);
        let current = self.note_hash();
        if current != self.note_saved && self.next_autosave.is_none() && seconds > 0 {
            self.next_autosave =
                Some(now + Duration::from_secs(u64::try_from(seconds).unwrap_or(30)));
        }
    }

    fn note_hash(&self) -> Option<u64> {
        self.engine
            .world
            .arena
            .iter()
            .find(|(_, entity)| entity.name() == NOTE_NAME)
            .and_then(|(_, entity)| entity.string_buffer())
            .map(|text| text_hash(text.as_str()))
    }

    /// Writes the note if it changed since the last save.
    pub fn save_note(&mut self) {
        self.next_autosave = None;
        let hash = self.note_hash();
        if hash == self.note_saved {
            return;
        }
        let Some(text) = self
            .engine
            .world
            .arena
            .iter()
            .find(|(_, entity)| entity.name() == NOTE_NAME)
            .and_then(|(_, entity)| entity.string_buffer())
        else {
            return;
        };
        let path = self.engine.note_path();
        match write_atomic(&path, text.as_str().as_bytes()) {
            Ok(()) => self.note_saved = hash,
            Err(error) => self
                .shell
                .notify(format!("Could not save the note: {error}"), true),
        }
    }

    /// Asks before closing when the rice says so. Returns `true` to close.
    pub fn close_requested(&mut self) -> bool {
        if self.quit {
            return true;
        }
        if self
            .engine
            .world
            .rice
            .get_bool("general.confirm_on_exit")
            .unwrap_or(true)
        {
            self.confirm_quit = true;
            self.shell.touch();
            return false;
        }
        self.shutdown();
        true
    }

    /// Saves what must survive: the note, the open typing group and the rice.
    pub fn shutdown(&mut self) {
        self.typing.flush(&mut self.engine.history);
        self.save_note();
        if let Some(Err(error)) = self.persist() {
            eprintln!("sue: could not save the rice: {error}");
        }
    }

    /// Writes the rice back to its file if it changed. Untouched lines are
    /// written byte for byte.
    pub fn persist(&mut self) -> Option<std::io::Result<PathBuf>> {
        let revision = self.engine.world.rice.revision();
        if revision == self.saved_revision {
            return None;
        }
        self.saved_revision = revision;
        let path = self.engine.rice_path();
        Some(write_atomic(&path, self.engine.world.rice.to_suerice().as_bytes()).map(|()| path))
    }

    /// Carries out what applets and the shell asked the engine for.
    fn serve_requests(&mut self, textures: &mut dyn TextureUpload, now: Instant) {
        for _ in 0..8 {
            let requests = std::mem::take(&mut self.engine.requests);
            if requests.is_empty() {
                return;
            }
            for request in requests {
                let leftover = {
                    let mut io = shell_io!(self, textures);
                    self.shell.handle(request, &mut io, now)
                };
                match leftover {
                    Some(ShellRequest::Notify(text)) => self.shell.notify(text, false),
                    Some(ShellRequest::Fail(text)) => self.shell.notify(text, true),
                    Some(ShellRequest::OpenFile(path)) => self.open_path(&path, textures, now),
                    Some(ShellRequest::Command(command)) => self.run_command(&command),
                    Some(ShellRequest::Picked { pick, path }) => {
                        self.picked(&pick, &path, textures, now);
                    }
                    Some(ShellRequest::PickFile(pick)) => self.pick_with_system_dialog(pick),
                    Some(_) | None => {}
                }
            }
        }
    }

    fn run_command(&mut self, command: &str) {
        match command {
            "file.open" => self.pick_and_open(),
            "file.save" => self.save_project(false),
            "file.save_as" => self.save_project(true),
            "app.quit" => {
                if self.close_requested() {
                    self.quit = true;
                }
            }
            "rice.reload" => self.reload_rice(),
            "rice.export" => self.export_rice(),
            "rice.reveal" => {
                let dir = self.engine.paths.rice_dir();
                self.reveal(&dir);
            }
            "app.restart" => {
                self.shutdown();
                self.restart = true;
                self.quit = true;
            }
            "audio.start" => {
                if self.engine.audio.is_none() {
                    self.open_audio();
                }
            }
            other => {
                if let Some(path) = other.strip_prefix("reveal:") {
                    self.reveal(Path::new(path));
                } else if let Some(path) = other.strip_prefix("reveal-file:") {
                    self.reveal_file(Path::new(path));
                } else {
                    self.shell.notify(format!("Unknown command {other}"), true);
                }
            }
        }
    }

    fn reveal(&mut self, dir: &Path) {
        if !self.interactive {
            self.shell
                .notify(format!("Folder: {}", dir.display()), false);
            return;
        }
        if let Err(error) = sue_platform::reveal_folder(dir) {
            self.shell
                .notify(format!("Cannot open the folder: {error}"), true);
        }
    }

    /// Shows a file selected in the system file manager.
    fn reveal_file(&mut self, path: &Path) {
        if !self.interactive {
            self.shell
                .notify(format!("File: {}", path.display()), false);
            return;
        }
        if let Err(error) = sue_platform::reveal_file(path) {
            self.shell
                .notify(format!("Cannot show the file: {error}"), true);
        }
    }

    /// Asks the File Manager for a file to open on the canvas.
    fn pick_and_open(&mut self) {
        self.engine
            .requests
            .push(ShellRequest::PickFile(FilePick::open(
                None,
                "open",
                "Open File",
            )));
    }

    /// Carries out a file the File Manager picked for the shell or engine.
    fn picked(
        &mut self,
        pick: &FilePick,
        path: &Path,
        textures: &mut dyn TextureUpload,
        now: Instant,
    ) {
        match pick.purpose.as_str() {
            "open" => self.open_path(path, textures, now),
            "save-project" => {
                if let Err(error) = self.save_project_to(path) {
                    self.shell
                        .notify(format!("Cannot save {}: {error}", file_name(path)), true);
                }
            }
            "export-rice" => {
                let text = self.engine.world.rice.to_suerice();
                match write_atomic(path, text.as_bytes()) {
                    Ok(()) => self.shell.notify_saved(path, "rice"),
                    Err(error) => self.shell.notify(format!("Export failed: {error}"), true),
                }
            }
            other => self
                .shell
                .notify(format!("Nothing asked for a file to {other}"), true),
        }
    }

    /// No applet picks files: the system dialog answers instead.
    fn pick_with_system_dialog(&mut self, pick: FilePick) {
        if !self.interactive {
            self.shell
                .notify("Choosing files needs the desktop window", false);
            return;
        }
        let filters = crate::files::filters_of(&pick);
        let start = pick
            .start
            .clone()
            .unwrap_or_else(|| self.engine.paths.home_dir.clone());
        let path = match &pick.save_as {
            Some(name) => sue_platform::pick_save_file(&pick.title, Some(&start), name, &filters),
            None => sue_platform::pick_open_file(&pick.title, Some(&start), &filters),
        };
        if let Some(path) = path {
            self.engine
                .requests
                .push(ShellRequest::Picked { pick, path });
        }
    }

    /// The open project's path, if it has one.
    #[must_use]
    pub fn project(&self) -> Option<&Path> {
        self.project.as_deref()
    }

    /// Saves the project to its file, asking where first if it has none
    /// (or when `choose` is set).
    pub fn save_project(&mut self, choose: bool) {
        let path = match (&self.project, choose) {
            (Some(path), false) => path.clone(),
            _ => {
                // The File Manager asks where; `picked` saves.
                let start = self
                    .project
                    .as_ref()
                    .and_then(|path| path.parent().map(Path::to_path_buf))
                    .unwrap_or_else(|| self.engine.paths.home_dir.clone());
                self.engine.requests.push(ShellRequest::PickFile(
                    FilePick::save(None, "save-project", "Save Project", "Untitled.sue")
                        .types(vec![sue_wm::FileType::new("Sue project", &["sue"])])
                        .starting_in(Some(start)),
                ));
                return;
            }
        };
        if let Err(error) = self.save_project_to(&path) {
            self.shell
                .notify(format!("Cannot save {}: {error}", file_name(&path)), true);
        }
    }

    /// Writes the project to `path` (atomically) and remembers the path.
    ///
    /// # Errors
    /// The file system's message.
    pub fn save_project_to(&mut self, path: &Path) -> Result<(), String> {
        self.typing.flush(&mut self.engine.history);
        self.engine.world.write_into(&mut self.project_file);
        write_atomic(path, &self.project_file.to_bytes()).map_err(|error| error.to_string())?;
        self.project = Some(path.to_path_buf());
        self.shell.notify_saved(path, "project");
        Ok(())
    }

    /// Replaces the open document with the project in `bytes`.
    ///
    /// # Errors
    /// Why the project could not be read; nothing changes then.
    pub fn open_project(&mut self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        let file = SueFile::from_bytes(bytes).map_err(|error| error.to_string())?;
        self.typing.flush(&mut self.engine.history);
        self.transport
            .set_playing(false, self.engine.audio.as_deref_mut());
        self.engine
            .world
            .load_file(&file, &sue_model::registry())
            .map_err(|error| error.to_string())?;
        // Old steps name entities that are gone.
        self.engine.history = UndoStack::new();
        self.typing = Typing::default();
        self.shell.select(None, &mut self.engine.world);
        self.shell.frame_all(&self.engine.world);
        self.project = Some(path.to_path_buf());
        self.project_file = file;
        self.shell
            .notify(format!("Opened {}", file_name(path)), false);
        Ok(())
    }

    /// Reads a file, puts it in the arena and places it on the canvas in the
    /// applet that shows its content. A `.sue` project replaces the document.
    /// A file an applet takes itself (a Word document, for Sue Writer) goes
    /// to that applet instead.
    pub fn open_path(&mut self, path: &Path, textures: &mut dyn TextureUpload, now: Instant) {
        let taker = self
            .applets
            .iter()
            .find(|(_, applet)| applet.opens_file(path))
            .map(|(manifest, _)| manifest.id.clone());
        if let Some(to) = taker {
            self.request(
                ShellRequest::SendFile {
                    to,
                    purpose: "import".to_owned(),
                    path: path.to_path_buf(),
                },
                textures,
                now,
            );
            return;
        }
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
            Ok(metadata) if metadata.is_dir() => Err("folders open in the Files drawer".to_owned()),
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
                self.shell
                    .notify(format!("Cannot open {name}: {reason}"), true);
                return;
            }
        };
        if SueFile::sniff(&bytes) {
            if let Err(reason) = self.open_project(path, &bytes) {
                self.shell
                    .notify(format!("Cannot open {name}: {reason}"), true);
            }
            self.serve_requests(textures, now);
            return;
        }
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
                self.shell
                    .notify(format!("Cannot open {name}: {error}"), true);
                return;
            }
        };
        let code = ingested.view == IngestView::Text
            && path.extension().is_some_and(|extension| {
                CODE_EXTENSIONS.contains(&extension.to_string_lossy().to_ascii_lowercase().as_str())
            });
        if let Ok(entity) = self.engine.world.arena.get_mut(ingested.handle) {
            entity.set_meta(meta::SOURCE, path.display().to_string());
            if code {
                entity.set_meta(KIND, KIND_CODE);
            }
        }
        let placed = {
            let mut io = shell_io!(self, textures);
            self.shell.place(ingested.handle, true, &mut io)
        };
        if placed.is_some()
            && let Some(note) = ingested.note
        {
            self.shell.notify(format!("{name}: {note}"), false);
        }
        self.serve_requests(textures, now);
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
                // The queue of the old stream is gone: start the clip again.
                let position = self.transport.position();
                self.transport
                    .seek(position, self.engine.audio.as_deref_mut());
                let playing = self.transport.is_playing();
                self.transport
                    .set_playing(playing, self.engine.audio.as_deref_mut());
            }
            Err(error) => {
                self.shell.notify(format!("Audio: {error}"), true);
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
                self.shell
                    .notify(format!("Cannot read {}: {error}", path.display()), true);
                return;
            }
        };
        let disk = match RiceSchema::parse(&text) {
            Ok(disk) => disk,
            Err(error) => {
                self.shell
                    .notify(format!("The rice file has an error: {error}"), true);
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
            self.shell
                .notify("The rice file matches what Sue shows", false);
            return;
        }
        let count = changes.len();
        let txn = SetRiceValues::new("Reload default.suerice", changes);
        self.typing.flush(&mut self.engine.history);
        match self
            .engine
            .history
            .apply(&mut self.engine.world, RiceEdit(txn))
        {
            Ok(_) => self
                .shell
                .notify(format!("Reloaded {count} values from disk"), false),
            Err(error) => self.shell.notify(format!("Reload failed: {error}"), true),
        }
        self.refresh_rice();
    }

    fn export_rice(&mut self) {
        // The File Manager asks where; `picked` writes it.
        let start = self.engine.paths.home_dir.clone();
        self.engine.requests.push(ShellRequest::PickFile(
            FilePick::save(None, "export-rice", "Export Rice", "my.suerice")
                .extensions(&["suerice"])
                .starting_in(Some(start)),
        ));
    }

    /// Runs `work` with an applet context, as an applet would (snapshots,
    /// scripted runs and tests), then serves what it asked for.
    pub fn with_context<R>(
        &mut self,
        textures: &mut dyn TextureUpload,
        now: Instant,
        work: impl FnOnce(&mut sue_wm::AppletContext<'_>) -> R,
    ) -> R {
        let result = {
            let mut io = shell_io!(self, textures);
            self.shell.with_context(&mut io, now, work)
        };
        self.serve_requests(textures, now);
        result
    }

    /// Runs a shell request as if an applet had asked (snapshots, scripted
    /// runs and tests).
    pub fn request(
        &mut self,
        request: ShellRequest,
        textures: &mut dyn TextureUpload,
        now: Instant,
    ) {
        self.engine.requests.push(request);
        self.serve_requests(textures, now);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::engine::tests::temp_host;
    use egui::{Pos2, vec2};
    use sue_platform::NoTextures;
    use sue_wm::AppletId;

    pub(crate) fn runtime(tag: &str) -> (Runtime, PathBuf) {
        let (host, root) = temp_host(tag);
        let runtime = Runtime::new(Engine::boot(&host).unwrap()).unwrap();
        (runtime, root)
    }

    /// Runs frames of `size` until the shell sleeps, at a fixed frame time.
    pub(crate) fn frames(
        runtime: &mut Runtime,
        ctx: &egui::Context,
        size: egui::Vec2,
        start: Instant,
        mut input: impl FnMut(usize) -> Vec<egui::Event>,
    ) -> usize {
        let mut count = 0;
        let mut now = start;
        loop {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
                events: input(count),
                time: Some(now.duration_since(start).as_secs_f64()),
                ..egui::RawInput::default()
            };
            ctx.run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now))
                .drop_without_applying_deltas();
            count += 1;
            now += Duration::from_millis(8);
            let asleep = !matches!(runtime.pacing(now), FramePacing::Animate { .. });
            if (asleep && count > 2) || count > 2_000 {
                return count;
            }
        }
    }

    #[test]
    fn a_mode_switch_glides_the_camera_and_then_sleeps() {
        let (mut runtime, root) = runtime("mode");
        let ctx = egui::Context::default();
        let start = Instant::now();
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |_| {
            Vec::new()
        });
        let before = runtime.shell.camera().center();
        runtime.request(
            ShellRequest::Mode("document".to_owned()),
            &mut NoTextures,
            start,
        );
        let count = frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |_| {
            Vec::new()
        });
        assert!(count > 10, "the camera must animate ({count} frames)");
        assert!(runtime.shell.camera().center().distance(before) > 1.0);
        assert_eq!(runtime.shell.mode(), "document");
        assert!(runtime.shell.selected().is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// One frame of `size` with `events`, at `now`.
    fn frame_with(
        runtime: &mut Runtime,
        ctx: &egui::Context,
        now: Instant,
        events: Vec<egui::Event>,
    ) {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events,
            ..egui::RawInput::default()
        };
        ctx.run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now))
            .drop_without_applying_deltas();
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn sue_paint_makes_a_picture_draws_with_a_pen_and_undoes() {
        let (mut runtime, root) = runtime("paint-studio");
        let ctx = egui::Context::default();
        let mut now = Instant::now();
        runtime.request(
            ShellRequest::Mode(sue_paint::ID.to_owned()),
            &mut NoTextures,
            now,
        );
        // The studio's panels slide in first.
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        assert!(runtime.shell.studio_reveal() > 0.999);
        let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
            frame_with(runtime, &ctx, now, events);
            now += Duration::from_millis(8);
        };
        assert_eq!(
            runtime.shell.studio().map(AppletId::as_str),
            Some(sue_paint::ID)
        );
        step(&mut runtime, Vec::new());
        // Ctrl+N opens New, Enter makes the picture with the riced size.
        step(
            &mut runtime,
            vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
        );
        step(
            &mut runtime,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        step(&mut runtime, Vec::new());
        let arena = &runtime.engine.world.arena;
        let picture = arena
            .iter()
            .find(|(_, entity)| sue_model::picture::is_picture(entity))
            .map(|(_, entity)| entity.id())
            .expect("New made a picture");
        assert_eq!(
            sue_model::picture::size(arena.get_by_id(picture).unwrap()),
            Some([1600, 1200])
        );
        let layer = sue_paint::doc::layers(arena, picture).last().unwrap().id;
        let card = runtime
            .engine
            .world
            .scene
            .iter()
            .find(|(_, node)| node.applet.as_str() == sue_paint::CARD_ID)
            .map(|(_, node)| node.entity);
        assert_eq!(
            card.and_then(|handle| arena.get(handle).ok())
                .map(sue_core::Entity::id),
            Some(picture),
            "the new picture also sits on the canvas"
        );
        let alpha = |runtime: &Runtime, x: u32, y: u32| {
            let raster = sue_paint::doc::raster(&runtime.engine.world.arena, layer).unwrap();
            raster.bytes().unwrap()[((y * raster.width() + x) * 4 + 3) as usize]
        };
        let depth = runtime.engine.history.depth();

        // A pen stroke from the middle of the canvas to the right, with pen
        // pressure. The picture is fitted, so the middle is pixel (800, 600).
        // The studio sits between the side rails, and its palettes leave the
        // canvas 17 points right of and 50 points above the area's middle.
        let viewport_center = runtime.shell.canvas_rect().center() + vec2(17.0, -50.0);
        let to = viewport_center + vec2(100.0, 0.0);
        let touch = |phase, pos| egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(1),
            phase,
            pos,
            force: Some(0.6),
        };
        step(
            &mut runtime,
            vec![
                touch(egui::TouchPhase::Start, viewport_center),
                egui::Event::PointerMoved(viewport_center),
                egui::Event::PointerButton {
                    pos: viewport_center,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        step(
            &mut runtime,
            vec![
                touch(egui::TouchPhase::Move, to),
                egui::Event::PointerMoved(viewport_center + vec2(50.0, 0.0)),
                egui::Event::PointerMoved(to),
            ],
        );
        // A pen's touch ends before its button does.
        step(
            &mut runtime,
            vec![
                touch(egui::TouchPhase::End, to),
                egui::Event::PointerButton {
                    pos: to,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(
            runtime.engine.history.depth(),
            depth + 1,
            "one stroke, one undo step"
        );
        let across = |runtime: &Runtime| (590..610).map(|y| alpha(runtime, 900, y)).max();
        assert!(across(&runtime) > Some(200), "the stroke crossed x = 900");
        assert_eq!(alpha(&runtime, 900, 700), 0, "and stayed thin");
        // Pressure 0.6 of an 8 px G-pen: about 5 px wide, not 8.
        let wide = (580..620)
            .filter(|y| alpha(&runtime, 900, *y) > 128)
            .count();
        assert!((3..=6).contains(&wide), "{wide} px wide");
        // The G-pen tapers into its end instead of leaving a blob.
        let ink = |x: u32| {
            (580..620)
                .map(|y| u32::from(alpha(&runtime, x, y)))
                .sum::<u32>()
        };
        let end = (900..1300).rev().find(|x| ink(*x) > 0).unwrap();
        assert!(end > 990, "the stroke reached x = {end}");
        assert!(ink(end - 4) < ink(900), "it thins toward its end");

        // Ctrl+Shift+N adds a layer above; Ctrl+Z takes it away again, and
        // a second Ctrl+Z the stroke.
        step(
            &mut runtime,
            vec![key(
                egui::Key::N,
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            )],
        );
        assert_eq!(
            sue_paint::doc::layers(&runtime.engine.world.arena, picture).len(),
            3
        );
        step(
            &mut runtime,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
        step(
            &mut runtime,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
        assert_eq!(
            sue_paint::doc::layers(&runtime.engine.world.arena, picture).len(),
            2
        );
        assert_eq!(across(&runtime), Some(0), "undo took the stroke away");

        // The picture lives in the project: it saves and comes back.
        let path = root.join("paint.sue");
        runtime.save_project_to(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        runtime.open_project(&path, &bytes).unwrap();
        assert!(runtime.engine.world.arena.get_by_id(picture).is_ok());
        assert_eq!(
            sue_paint::doc::layers(&runtime.engine.world.arena, picture).len(),
            2
        );
        step(&mut runtime, Vec::new());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_puppet_warp_bends_a_stroke_in_one_undo_step() {
        let (mut runtime, root) = runtime("paint-warp");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Mode(sue_paint::ID.to_owned()),
            &mut NoTextures,
            now,
        );
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
            frame_with(runtime, &ctx, now, events);
            now += Duration::from_millis(8);
        };
        step(
            &mut runtime,
            vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
        );
        step(
            &mut runtime,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        step(&mut runtime, Vec::new());
        let arena = &runtime.engine.world.arena;
        let picture = arena
            .iter()
            .find(|(_, entity)| sue_model::picture::is_picture(entity))
            .map(|(_, entity)| entity.id())
            .unwrap();
        let layer = sue_paint::doc::layers(arena, picture).last().unwrap().id;
        let ink = |runtime: &Runtime, x: u32, rows: std::ops::Range<u32>| {
            let raster = sue_paint::doc::raster(&runtime.engine.world.arena, layer).unwrap();
            rows.map(|y| {
                u32::from(raster.bytes().unwrap()[((y * raster.width() + x) * 4 + 3) as usize])
            })
            .sum::<u32>()
        };
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        // A mouse stroke across the middle, from x = 800 to past 990.
        let from = runtime.shell.canvas_rect().center() + vec2(17.0, -50.0);
        let to = from + vec2(100.0, 0.0);
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(from), press(from, true)],
        );
        step(
            &mut runtime,
            vec![
                egui::Event::PointerMoved(from + vec2(50.0, 0.0)),
                egui::Event::PointerMoved(to),
            ],
        );
        step(&mut runtime, vec![press(to, false)]);
        assert!(ink(&runtime, 960, 580..620) > 0, "the stroke is there");
        let depth = runtime.engine.history.depth();

        // Puppet warp: pin the start, drag the end up.
        let paint = AppletId::new(sue_paint::ID).unwrap();
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint,
                command: "edit.transform.puppet".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        step(&mut runtime, Vec::new());
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(from), press(from, true)],
        );
        step(&mut runtime, vec![press(from, false)]);
        let end = to - vec2(8.0, 0.0);
        let up = end + vec2(0.0, -60.0);
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(end), press(end, true)],
        );
        step(
            &mut runtime,
            vec![
                egui::Event::PointerMoved(end + vec2(0.0, -30.0)),
                egui::Event::PointerMoved(up),
            ],
        );
        step(&mut runtime, vec![press(up, false)]);
        assert_eq!(
            runtime.engine.history.depth(),
            depth,
            "a preview, not history yet"
        );
        step(
            &mut runtime,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        step(&mut runtime, Vec::new());
        assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");
        assert_eq!(ink(&runtime, 960, 580..620), 0, "the end left the middle");
        assert!(ink(&runtime, 960, 440..580) > 0, "and went up");
        assert!(ink(&runtime, 810, 580..620) > 0, "the pinned start stayed");

        // Undo puts the stroke back as it was.
        step(
            &mut runtime,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
        assert!(ink(&runtime, 960, 580..620) > 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_stories_add_step_through_delete_and_export_pages() {
        let (mut runtime, root) = runtime("paint-story");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Mode(sue_paint::ID.to_owned()),
            &mut NoTextures,
            now,
        );
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        let paint = AppletId::new(sue_paint::ID).unwrap();
        let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
            frame_with(runtime, &ctx, now, events);
            now += Duration::from_millis(8);
        };
        step(
            &mut runtime,
            vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
        );
        step(
            &mut runtime,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        step(&mut runtime, Vec::new());
        let command = |runtime: &mut Runtime, name: &str| {
            runtime.request(
                ShellRequest::AppletCommand {
                    applet: paint.clone(),
                    command: name.to_owned(),
                },
                &mut NoTextures,
                start,
            );
        };
        let current = |runtime: &Runtime| {
            runtime
                .applets
                .get(&paint)
                .and_then(|applet| applet.studio_focus())
        };
        let first = current(&runtime).unwrap();
        let pages = |runtime: &Runtime| {
            let arena = &runtime.engine.world.arena;
            sue_paint::story::story_of(arena, first)
                .map(|story| sue_paint::story::pages(arena, story))
                .unwrap_or_default()
        };

        // Add Page makes the picture a story and opens the new second page.
        command(&mut runtime, "story.add");
        step(&mut runtime, Vec::new());
        assert_eq!(pages(&runtime).len(), 2);
        assert_eq!(current(&runtime), Some(pages(&runtime)[1]));
        // Insert Page goes before the current one.
        command(&mut runtime, "story.insert");
        step(&mut runtime, Vec::new());
        let now_pages = pages(&runtime);
        assert_eq!(now_pages.len(), 3);
        assert_eq!(current(&runtime), Some(now_pages[1]));
        // Stepping through.
        command(&mut runtime, "story.last");
        assert_eq!(current(&runtime), Some(now_pages[2]));
        command(&mut runtime, "story.first");
        assert_eq!(current(&runtime), Some(first));
        command(&mut runtime, "story.next");
        assert_eq!(current(&runtime), Some(now_pages[1]));
        // Delete Page opens the next one; undo brings the page back.
        let depth = runtime.engine.history.depth();
        command(&mut runtime, "story.delete");
        step(&mut runtime, Vec::new());
        assert_eq!(pages(&runtime).len(), 2);
        assert_eq!(current(&runtime), Some(now_pages[2]));
        assert_eq!(runtime.engine.history.depth(), depth + 1);
        step(
            &mut runtime,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
        assert_eq!(pages(&runtime), now_pages);

        // Export All Pages writes one numbered file per page.
        std::fs::create_dir_all(&root).unwrap();
        runtime.request(
            ShellRequest::Picked {
                pick: FilePick::save(
                    Some(paint.clone()),
                    "story-pages",
                    "Export All Pages",
                    "Book.png",
                ),
                path: root.join("Book.png"),
            },
            &mut NoTextures,
            start,
        );
        for number in 1..=3 {
            let path = root.join(format!("Book-{number:03}.png"));
            assert!(path.exists(), "{} was written", path.display());
        }

        // The story lives in the project, its pages in order.
        let project = root.join("story.sue");
        runtime.save_project_to(&project).unwrap();
        let bytes = std::fs::read(&project).unwrap();
        runtime.open_project(&project, &bytes).unwrap();
        assert_eq!(pages(&runtime), now_pages);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_makes_manga_frames_balloons_and_focus_lines() {
        let (mut runtime, root) = runtime("paint-manga");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Mode(sue_paint::ID.to_owned()),
            &mut NoTextures,
            now,
        );
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        let paint = AppletId::new(sue_paint::ID).unwrap();
        let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
            frame_with(runtime, &ctx, now, events);
            now += Duration::from_millis(8);
        };
        let plain = egui::Modifiers::NONE;
        step(
            &mut runtime,
            vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
        );
        step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
        step(&mut runtime, Vec::new());
        let command = |runtime: &mut Runtime, name: &str| {
            runtime.request(
                ShellRequest::AppletCommand {
                    applet: paint.clone(),
                    command: name.to_owned(),
                },
                &mut NoTextures,
                start,
            );
        };
        let picture = runtime
            .applets
            .get(&paint)
            .and_then(|applet| applet.studio_focus())
            .unwrap();
        let layer_of = |runtime: &Runtime, class: &str| {
            let arena = &runtime.engine.world.arena;
            sue_paint::doc::layers(arena, picture)
                .into_iter()
                .find(|info| {
                    arena
                        .get_by_id(info.id)
                        .is_ok_and(|entity| entity.class() == class)
                })
                .map(|info| info.id)
        };
        let text_of = |runtime: &Runtime, layer, key| {
            runtime
                .engine
                .world
                .arena
                .get_by_id(layer)
                .ok()
                .and_then(|entity| {
                    entity
                        .property(key)
                        .and_then(sue_core::Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_default()
        };
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let drag = |step: &mut dyn FnMut(&mut Runtime, Vec<egui::Event>),
                    runtime: &mut Runtime,
                    from,
                    to| {
            step(
                runtime,
                vec![egui::Event::PointerMoved(from), press(from, true)],
            );
            step(runtime, vec![egui::Event::PointerMoved(to)]);
            step(runtime, vec![press(to, false)]);
        };
        let middle = runtime.shell.canvas_rect().center() + vec2(17.0, -50.0);

        // A frame border folder of the manga layout: five panels.
        command(&mut runtime, "layer.new.frame");
        step(&mut runtime, Vec::new());
        step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
        step(&mut runtime, Vec::new());
        let frame = layer_of(&runtime, sue_paint::manga::FRAME).expect("a frame layer");
        let panels = |runtime: &Runtime| {
            sue_paint::manga::decode_panels(&text_of(runtime, frame, sue_paint::manga::PANELS))
                .len()
        };
        assert_eq!(panels(&runtime), 5);
        // The Frame Border tool's division cuts the middle panel in two.
        step(&mut runtime, vec![key(egui::Key::U, plain)]);
        step(&mut runtime, vec![key(egui::Key::U, plain)]);
        step(&mut runtime, vec![key(egui::Key::Period, plain)]);
        let depth = runtime.engine.history.depth();
        drag(
            &mut step,
            &mut runtime,
            middle - vec2(0.0, 30.0),
            middle + vec2(0.0, 30.0),
        );
        assert_eq!(panels(&runtime), 6, "divided");
        assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");
        step(
            &mut runtime,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
        assert_eq!(panels(&runtime), 5, "undone");

        // A balloon and its tail on a balloon layer.
        step(&mut runtime, vec![key(egui::Key::T, plain)]);
        step(&mut runtime, vec![key(egui::Key::T, plain)]);
        drag(
            &mut step,
            &mut runtime,
            middle - vec2(40.0, 30.0),
            middle + vec2(40.0, 30.0),
        );
        let balloon = layer_of(&runtime, sue_paint::manga::BALLOON).expect("a balloon layer");
        let shapes = |runtime: &Runtime| {
            sue_paint::manga::decode_shapes(&text_of(runtime, balloon, sue_paint::manga::SHAPES))
                .len()
        };
        assert_eq!(shapes(&runtime), 1);
        step(&mut runtime, vec![key(egui::Key::Period, plain)]);
        drag(&mut step, &mut runtime, middle, middle + vec2(70.0, 60.0));
        assert_eq!(shapes(&runtime), 2, "a tail");

        // Focus lines on a new layer, applied as one step.
        command(&mut runtime, "layer.new");
        step(&mut runtime, Vec::new());
        let depth = runtime.engine.history.depth();
        command(&mut runtime, "filter.focus_lines");
        step(&mut runtime, Vec::new());
        step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
        step(&mut runtime, Vec::new());
        assert_eq!(runtime.engine.history.depth(), depth + 1);
        let arena = &runtime.engine.world.arena;
        let top = sue_paint::doc::layers(arena, picture).last().unwrap().id;
        let raster = sue_paint::doc::raster(arena, top).unwrap();
        let inked = raster
            .bytes()
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 128)
            .count();
        assert!(inked > 10_000, "focus lines: {inked} pixels");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_animates_cels_on_a_timeline_and_exports_them() {
        let (mut runtime, root) = runtime("paint-anim");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Mode(sue_paint::ID.to_owned()),
            &mut NoTextures,
            now,
        );
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        let paint = AppletId::new(sue_paint::ID).unwrap();
        let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
            frame_with(runtime, &ctx, now, events);
            now += Duration::from_millis(8);
        };
        step(
            &mut runtime,
            vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
        );
        step(
            &mut runtime,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        step(&mut runtime, Vec::new());
        let command = |runtime: &mut Runtime, name: &str| {
            runtime.request(
                ShellRequest::AppletCommand {
                    applet: paint.clone(),
                    command: name.to_owned(),
                },
                &mut NoTextures,
                start,
            );
        };
        let picture = runtime
            .applets
            .get(&paint)
            .and_then(|applet| applet.studio_focus())
            .unwrap();
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let middle = runtime.shell.canvas_rect().center() + vec2(17.0, -50.0);
        let mut stroke = |runtime: &mut Runtime, y: f32| {
            let (from, to) = (middle + vec2(-60.0, y), middle + vec2(60.0, y));
            step(
                runtime,
                vec![egui::Event::PointerMoved(from), press(from, true)],
            );
            step(
                runtime,
                vec![
                    egui::Event::PointerMoved(middle + vec2(0.0, y)),
                    egui::Event::PointerMoved(to),
                ],
            );
            step(runtime, vec![press(to, false)]);
        };

        // A cel at frame 1 drawn high, another from frame 4 drawn low.
        command(&mut runtime, "anim.cel.new");
        stroke(&mut runtime, -40.0);
        for _ in 0..3 {
            command(&mut runtime, "anim.next");
        }
        command(&mut runtime, "anim.cel.new");
        stroke(&mut runtime, 40.0);
        let arena = &runtime.engine.world.arena;
        let sequence = sue_paint::anim::timeline(arena, picture).expect("a timeline");
        let keys = sue_paint::anim::keys(&sequence.tracks[0], sequence.rate);
        assert_eq!(
            keys.iter().map(|(frame, _)| *frame).collect::<Vec<_>>(),
            vec![0, 3]
        );

        // Onion skin and undo.
        command(&mut runtime, "anim.onion");
        let depth = runtime.engine.history.depth();
        command(&mut runtime, "anim.cel.new");
        assert_eq!(
            runtime.engine.history.depth(),
            depth + 1,
            "a cel is one step"
        );
        step(
            &mut runtime,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
        assert_eq!(runtime.engine.history.depth(), depth);

        // The image sequence shows one cel per frame.
        std::fs::create_dir_all(&root).unwrap();
        runtime.request(
            ShellRequest::Picked {
                pick: FilePick::save(
                    Some(paint.clone()),
                    "anim-sequence",
                    "Export Animation",
                    "Walk.png",
                ),
                path: root.join("Walk.png"),
            },
            &mut NoTextures,
            start,
        );
        let ink = |frame: u32, upper: bool| {
            let bytes = std::fs::read(root.join(format!("Walk-{frame:04}.png"))).unwrap();
            let tile = sue_io::decode_png(&bytes).unwrap().tile;
            let (width, height) = (tile.width(), tile.height());
            let pixels = tile.bytes().unwrap();
            let rows = if upper {
                0..height / 2
            } else {
                height / 2..height
            };
            rows.flat_map(|y| (0..width).map(move |x| (x, y)))
                .filter(|(x, y)| pixels[((y * width + x) * 4) as usize] < 100)
                .count()
        };
        assert!(
            ink(2, true) > 50 && ink(2, false) == 0,
            "frame 2 shows the first cel"
        );
        assert!(
            ink(5, false) > 50 && ink(5, true) == 0,
            "frame 5 shows the second cel"
        );
        assert!(root.join("Walk-0024.png").exists(), "every frame");
        runtime.request(
            ShellRequest::Picked {
                pick: FilePick::save(
                    Some(paint.clone()),
                    "anim-gif",
                    "Export Animation",
                    "Walk.gif",
                ),
                path: root.join("Walk.gif"),
            },
            &mut NoTextures,
            start,
        );
        assert!(
            std::fs::read(root.join("Walk.gif"))
                .unwrap()
                .starts_with(b"GIF8")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_scale_rotate_moves_a_stroke_in_one_undo_step() {
        let (mut runtime, root) = runtime("paint-transform");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Mode(sue_paint::ID.to_owned()),
            &mut NoTextures,
            now,
        );
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        let paint = AppletId::new(sue_paint::ID).unwrap();
        let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
            frame_with(runtime, &ctx, now, events);
            now += Duration::from_millis(8);
        };
        step(
            &mut runtime,
            vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
        );
        step(
            &mut runtime,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        step(&mut runtime, Vec::new());
        let picture = runtime
            .applets
            .get(&paint)
            .and_then(|applet| applet.studio_focus())
            .unwrap();
        let layer = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
            .last()
            .unwrap()
            .id;
        let ink = |runtime: &Runtime, rows: std::ops::Range<u32>| {
            let raster = sue_paint::doc::raster(&runtime.engine.world.arena, layer).unwrap();
            rows.map(|y| {
                u32::from(raster.bytes().unwrap()[((y * raster.width() + 800) * 4 + 3) as usize])
            })
            .sum::<u32>()
        };
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let middle = runtime.shell.canvas_rect().center() + vec2(17.0, -50.0);
        let (from, to) = (middle - vec2(60.0, 0.0), middle + vec2(60.0, 0.0));
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(from), press(from, true)],
        );
        step(
            &mut runtime,
            vec![
                egui::Event::PointerMoved(middle),
                egui::Event::PointerMoved(to),
            ],
        );
        step(&mut runtime, vec![press(to, false)]);
        assert!(ink(&runtime, 580..620) > 0);
        let depth = runtime.engine.history.depth();

        // Ctrl+T, drag from inside the box down, Enter.
        step(
            &mut runtime,
            vec![key(egui::Key::T, egui::Modifiers::COMMAND)],
        );
        step(&mut runtime, Vec::new());
        let down = middle + vec2(0.0, 80.0);
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(middle), press(middle, true)],
        );
        step(
            &mut runtime,
            vec![
                egui::Event::PointerMoved(middle + vec2(0.0, 40.0)),
                egui::Event::PointerMoved(down),
            ],
        );
        step(&mut runtime, vec![press(down, false)]);
        assert_eq!(
            runtime.engine.history.depth(),
            depth,
            "a preview, not history yet"
        );
        step(
            &mut runtime,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        step(&mut runtime, Vec::new());
        assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");
        assert_eq!(ink(&runtime, 580..620), 0, "the stroke left its place");
        assert!(ink(&runtime, 700..820) > 0, "and moved down");
        step(
            &mut runtime,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
        assert!(ink(&runtime, 580..620) > 0, "undo puts it back");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_saves_as_a_project_or_any_picture_type() {
        let (mut runtime, root) = runtime("paint-save-as");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Mode(sue_paint::ID.to_owned()),
            &mut NoTextures,
            now,
        );
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
            frame_with(runtime, &ctx, now, events);
            now += Duration::from_millis(8);
        };
        step(
            &mut runtime,
            vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
        );
        step(
            &mut runtime,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        step(&mut runtime, Vec::new());
        let paint = AppletId::new(sue_paint::ID).unwrap();
        let pick = FilePick::save(Some(paint), "save-as", "Save As", "Picture.sue");
        std::fs::create_dir_all(&root).unwrap();

        // A picture type writes the flattened picture in that format.
        for (name, format) in [
            ("Picture.qoi", sue_io::PictureFormat::Qoi),
            ("Picture.webp", sue_io::PictureFormat::WebP),
        ] {
            let path = root.join(name);
            runtime.request(
                ShellRequest::Picked {
                    pick: pick.clone(),
                    path: path.clone(),
                },
                &mut NoTextures,
                start,
            );
            step(&mut runtime, Vec::new());
            let bytes = std::fs::read(&path).unwrap_or_else(|_| panic!("{name} was written"));
            let decoded = sue_io::decode_picture(&bytes, Some(format)).unwrap();
            assert_eq!([decoded.width(), decoded.height()], [1600, 1200]);
        }
        assert_eq!(runtime.project(), None, "a picture type is not the project");

        // The Sue project type saves the whole project there.
        let project = root.join("Picture.sue");
        runtime.request(
            ShellRequest::Picked {
                pick,
                path: project.clone(),
            },
            &mut NoTextures,
            start,
        );
        step(&mut runtime, Vec::new());
        step(&mut runtime, Vec::new());
        assert_eq!(runtime.project(), Some(project.as_path()));
        assert!(sue_core::SueFile::from_bytes(&std::fs::read(&project).unwrap()).is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn studios_zoom_in_slide_in_and_glide_back_out() {
        let (mut runtime, root) = runtime("studio-motion");
        let ctx = egui::Context::default();
        let size = vec2(1440.0, 900.0);
        let start = Instant::now();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        let overview = runtime.shell.camera().zoom();
        let paint = AppletId::new(sue_paint::ID).unwrap();
        // New File, Picture, Square Post: a card appears and Sue Paint
        // opens on it, first zooming into the card.
        runtime.request(
            ShellRequest::Create {
                applet: paint.clone(),
                key: "Square Post".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        assert_eq!(runtime.shell.studio(), Some(&paint));
        assert!(
            runtime.shell.studio_reveal() < 0.01,
            "the camera goes first"
        );
        let card = runtime
            .engine
            .world
            .scene
            .iter()
            .any(|(_, node)| node.applet.as_str() == sue_paint::CARD_ID);
        assert!(card, "the new picture sits on the canvas");
        let opening = frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        assert!(
            opening > 20,
            "zooming and sliding in take a moment ({opening} frames)"
        );
        assert!(runtime.shell.studio_reveal() > 0.999);
        // The card's picture fills the studio's area, 24 points from its
        // edges, as Sue Paint then shows it.
        let content = runtime
            .engine
            .world
            .scene
            .iter()
            .find(|(_, node)| node.applet.as_str() == sue_paint::CARD_ID)
            .map(|(_, node)| node.rect.size() - vec2(0.0, sue_wm::NODE_TITLE))
            .unwrap();
        let area = runtime.shell.canvas_rect().size() - vec2(48.0, 48.0);
        let filled = (area.x / content.x).min(area.y / content.y);
        let zoom = runtime.shell.camera().zoom();
        assert!(
            (zoom / filled - 1.0).abs() < 0.05 && (zoom / overview - 1.0).abs() > 0.05,
            "zoomed into the card: {zoom} for {filled}, from {overview}"
        );

        runtime.request(ShellRequest::CloseStudio, &mut NoTextures, start);
        assert_eq!(runtime.shell.studio(), None, "leaving starts at once");
        let closing = frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        assert!(
            closing > 20,
            "sliding out and zooming out take a moment ({closing})"
        );
        assert!(runtime.shell.studio_reveal() < 0.001);

        // With instant transitions the same trip takes a frame or two.
        runtime
            .engine
            .history
            .apply(
                &mut runtime.engine.world,
                RiceEdit(sue_rice::SetRiceValue::new(
                    "motion.instant_transitions",
                    sue_rice::RiceValue::Bool(true),
                )),
            )
            .unwrap();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        runtime.request(ShellRequest::OpenStudio(paint), &mut NoTextures, start);
        for frame in 0..2 {
            frame_with(
                &mut runtime,
                &ctx,
                start + Duration::from_millis(8 * frame),
                Vec::new(),
            );
        }
        assert!(
            runtime.shell.studio_reveal() > 0.999,
            "instant transitions snap in a frame or two"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn saved_files_say_where_they_went_and_show_on_a_click() {
        let (mut runtime, root) = runtime("saved");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("out.jxl");
        std::fs::write(&path, b"jxl").unwrap();
        let start = Instant::now();
        runtime.request(
            ShellRequest::Saved {
                path: path.clone(),
                note: "JPEG XL".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        let (text, error) = runtime.shell.message().unwrap();
        assert!(!error);
        assert!(text.contains("out.jxl") && text.contains(&root.display().to_string()));
        assert_eq!(runtime.shell.message_path(), Some(path.as_path()));
        // Headless, showing the file names it instead of opening Explorer.
        runtime.request(
            ShellRequest::Command(format!("reveal-file:{}", path.display())),
            &mut NoTextures,
            start,
        );
        assert!(runtime.shell.message().unwrap().0.starts_with("File: "));
        assert_eq!(runtime.shell.message_path(), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn studios_and_the_canvas_open_files_through_the_file_manager() {
        let (mut runtime, root) = runtime("pick");
        std::fs::create_dir_all(&root).unwrap();
        let picture = root.join("photo.png");
        let pixels = [255_u8, 0, 0, 255].repeat(16);
        std::fs::write(&picture, sue_io::encode_png_rgba(4, 4, &pixels).unwrap()).unwrap();
        let ctx = egui::Context::default();
        let start = Instant::now();
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |_| {
            Vec::new()
        });
        let pictures = |runtime: &Runtime| {
            runtime
                .engine
                .world
                .arena
                .iter()
                .filter(|(_, entity)| sue_model::picture::is_picture(entity))
                .count()
        };

        // Sue Paint asks; the File Manager's window answers.
        let paint = AppletId::new(sue_paint::ID).unwrap();
        let pick = FilePick::open(Some(paint), "open", "Open Picture").extensions(&["png"]);
        runtime.request(ShellRequest::PickFile(pick.clone()), &mut NoTextures, start);
        assert_eq!(
            runtime.shell.window().map(AppletId::as_str),
            Some(crate::files::ID)
        );
        let before = pictures(&runtime);
        runtime.request(
            ShellRequest::Picked {
                pick,
                path: picture.clone(),
            },
            &mut NoTextures,
            start,
        );
        assert_eq!(pictures(&runtime), before + 1, "Sue Paint opened it");
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |_| {
            Vec::new()
        });
        assert_eq!(runtime.shell.window(), None, "the window closed");

        // Ctrl+O on the canvas goes the same way.
        let nodes = runtime.engine.world.scene.len();
        runtime.request(
            ShellRequest::Command("file.open".to_owned()),
            &mut NoTextures,
            start,
        );
        assert_eq!(
            runtime.shell.window().map(AppletId::as_str),
            Some(crate::files::ID)
        );
        runtime.request(
            ShellRequest::Picked {
                pick: FilePick::open(None, "open", "Open File"),
                path: picture,
            },
            &mut NoTextures,
            start,
        );
        assert_eq!(
            runtime.engine.world.scene.len(),
            nodes + 1,
            "it is on the canvas"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_player_adds_a_folder_of_songs_and_plays_through_them() {
        let (mut runtime, root) = runtime("player");
        let music = root.join("music");
        std::fs::create_dir_all(music.join("disc")).unwrap();
        for (name, value) in [("one.wav", 0.1_f32), ("disc/two.wav", 0.2)] {
            let pcm = sue_core::PCMFloat1D::new(8_000, 1, vec![value; 8_000]).unwrap();
            std::fs::write(music.join(name), sue_io::encode_wav(&pcm)).unwrap();
        }
        let mut now = Instant::now();
        let player = AppletId::new(crate::player::ID).unwrap();
        // The File Manager's Add to Player, on a whole folder.
        runtime.request(
            ShellRequest::SendFile {
                to: player.clone(),
                purpose: "import".to_owned(),
                path: music.clone(),
            },
            &mut NoTextures,
            now,
        );
        let library = runtime.engine.catalog.folders["player"].join("library.txt");
        let songs = move || {
            std::fs::read_to_string(&library)
                .map(|text| {
                    text.lines()
                        .filter(|line| line.starts_with("song\t"))
                        .count()
                })
                .unwrap_or(0)
        };
        let mut wait = |runtime: &mut Runtime, until: &dyn Fn(&Runtime) -> bool| {
            for _ in 0..400 {
                if until(runtime) {
                    return true;
                }
                runtime.timer(&mut NoTextures, now);
                now += Duration::from_millis(50);
                std::thread::sleep(Duration::from_millis(5));
            }
            false
        };
        assert!(
            wait(&mut runtime, &|_| songs() == 2),
            "both songs are in the library"
        );

        // Play: the song decodes in the background, then plays from its
        // shared buffer, not from the arena.
        let entities = runtime.engine.world.arena.len();
        let command = |name: &str| ShellRequest::AppletCommand {
            applet: player.clone(),
            command: name.to_owned(),
        };
        runtime.request(command("player.toggle"), &mut NoTextures, Instant::now());
        let playing_stream = |runtime: &Runtime| {
            runtime.transport.is_playing()
                && runtime.transport.clip().is_some_and(|clip| {
                    clip.owner == "player" && matches!(clip.source, sue_wm::ClipSource::Stream(_))
                })
        };
        assert!(wait(&mut runtime, &playing_stream), "the first song plays");
        assert_eq!(
            runtime.engine.world.arena.len(),
            entities,
            "nothing went into the arena"
        );
        let frames_of = |runtime: &Runtime| match runtime.transport.clip().map(|clip| &clip.source)
        {
            Some(sue_wm::ClipSource::Stream(pcm)) => pcm.samples()[0],
            _ => 0.0,
        };
        let first = frames_of(&runtime);

        // Next: the other song, already decoded ahead.
        runtime.request(command("player.next"), &mut NoTextures, Instant::now());
        assert!(wait(&mut runtime, &|runtime| playing_stream(runtime)
            && frames_of(runtime) != first));
        // Pause, and play again.
        runtime.request(command("player.toggle"), &mut NoTextures, Instant::now());
        assert!(!runtime.transport.is_playing());
        runtime.request(command("player.toggle"), &mut NoTextures, Instant::now());
        assert!(runtime.transport.is_playing());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_converter_turns_a_picture_a_song_and_text_into_new_files() {
        let (mut runtime, root) = runtime("convert");
        std::fs::create_dir_all(&root).unwrap();
        let picture = root.join("card.png");
        let pixels = [40_u8, 80, 120, 255].repeat(6 * 4);
        std::fs::write(&picture, sue_io::encode_png_rgba(6, 4, &pixels).unwrap()).unwrap();
        let song = root.join("tone.wav");
        let pcm = sue_core::PCMFloat1D::new(8_000, 1, vec![0.2; 4_000]).unwrap();
        std::fs::write(&song, sue_io::encode_wav(&pcm)).unwrap();
        let notes = root.join("notes.md");
        std::fs::write(&notes, "# Hello\n\nSome *words*.\n").unwrap();
        let mut now = Instant::now();
        let convert = AppletId::new(crate::convert::ID).unwrap();
        for path in [&picture, &song, &notes] {
            runtime.request(
                ShellRequest::SendFile {
                    to: convert.clone(),
                    purpose: "add".to_owned(),
                    path: path.clone(),
                },
                &mut NoTextures,
                now,
            );
        }
        // The usual targets: a PNG stays PNG (a new file beside it), the
        // sound becomes FLAC and the text is tidied.
        runtime.request(
            ShellRequest::AppletCommand {
                applet: convert,
                command: "convert.run".to_owned(),
            },
            &mut NoTextures,
            now,
        );
        let outputs = [
            root.join("card (2).png"),
            root.join("tone.flac"),
            root.join("notes.txt"),
        ];
        for _ in 0..400 {
            if outputs.iter().all(|path| path.is_file()) {
                break;
            }
            runtime.timer(&mut NoTextures, now);
            now += Duration::from_millis(50);
            std::thread::sleep(Duration::from_millis(5));
        }
        for path in &outputs {
            assert!(path.is_file(), "{} was written", path.display());
        }
        let (flac, _) =
            sue_io::decode_audio(std::fs::read(&outputs[1]).unwrap(), Some("flac")).unwrap();
        assert_eq!(flac.pcm.frames(), 4_000);
        assert!(
            std::fs::read(&picture).unwrap().starts_with(b"\x89PNG"),
            "the source is untouched"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn notes_open_and_save_files_and_undo_brings_the_old_note_back() {
        let (mut runtime, root) = runtime("notes-files");
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("in.txt");
        std::fs::write(&file, "from a file").unwrap();
        let note = |runtime: &Runtime| {
            runtime
                .engine
                .world
                .arena
                .iter()
                .find(|(_, entity)| entity.name() == NOTE_NAME)
                .and_then(|(_, entity)| entity.string_buffer().map(|text| text.as_str().to_owned()))
                .unwrap_or_default()
        };
        let before = note(&runtime);
        let notes = AppletId::new(crate::notes::ID).unwrap();
        let send = |purpose: &str, path: &Path| ShellRequest::SendFile {
            to: notes.clone(),
            purpose: purpose.to_owned(),
            path: path.to_path_buf(),
        };
        runtime.request(send("open", &file), &mut NoTextures, Instant::now());
        assert_eq!(note(&runtime), "from a file");
        let out = root.join("out.txt");
        runtime.request(send("save", &out), &mut NoTextures, Instant::now());
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "from a file");
        runtime
            .engine
            .history
            .undo(&mut runtime.engine.world)
            .unwrap();
        assert_eq!(note(&runtime), before, "one undo brings the old note back");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn studios_keep_the_keys_they_reserve() {
        let (mut runtime, root) = runtime("reserved-keys");
        let ctx = egui::Context::default();
        let start = Instant::now();
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |_| {
            Vec::new()
        });
        runtime.request(ShellRequest::Hub(false), &mut NoTextures, start);
        // On the canvas, Tab opens the command palette.
        frame_with(
            &mut runtime,
            &ctx,
            start,
            vec![key(egui::Key::Tab, egui::Modifiers::NONE)],
        );
        assert!(runtime.shell.palette_open());
        runtime.shell.dismiss_overlays();
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |_| {
            Vec::new()
        });
        // In Sue Paint, Tab hides the palettes, as in Clip Studio Paint.
        runtime.request(
            ShellRequest::Mode(sue_paint::ID.to_owned()),
            &mut NoTextures,
            start,
        );
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |_| {
            Vec::new()
        });
        assert!(runtime.shell.studio_reveal() > 0.999);
        frame_with(
            &mut runtime,
            &ctx,
            start,
            vec![key(egui::Key::Tab, egui::Modifiers::NONE)],
        );
        assert!(!runtime.shell.palette_open(), "Tab stayed with Sue Paint");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_sue_menu_follows_the_context_and_sends_work_between_studios() {
        let (mut runtime, root) = runtime("sue-menu");
        let ctx = egui::Context::default();
        let size = vec2(1440.0, 900.0);
        let start = Instant::now();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        runtime.request(ShellRequest::Hub(false), &mut NoTextures, start);

        // On the canvas: projects, files and the studios.
        runtime.request(ShellRequest::SueMenu, &mut NoTextures, start);
        assert!(runtime.shell.radial_open());
        let first = runtime.shell.sue_menu_labels();
        for wheel in [
            "File \u{203a}",
            "Project \u{203a}",
            "Studios \u{203a}",
            "Command Palette",
        ] {
            assert!(
                first.iter().any(|label| label == wheel),
                "{wheel} in {first:?}"
            );
        }
        runtime.shell.sue_menu_open("File");
        assert!(
            runtime
                .shell
                .sue_menu_labels()
                .iter()
                .any(|label| label == "File Manager")
        );
        runtime.shell.dismiss_overlays();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());

        // In Sue Paint, on a new picture: its own wheel first, and Send To.
        let paint = AppletId::new(sue_paint::ID).unwrap();
        runtime.request(
            ShellRequest::Create {
                applet: paint.clone(),
                key: "Square Post".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        assert_eq!(runtime.shell.studio(), Some(&paint));
        runtime.request(ShellRequest::SueMenu, &mut NoTextures, start);
        let first = runtime.shell.sue_menu_labels();
        assert_eq!(
            first[0], "Sue Paint \u{203a}",
            "the open studio's wheel first: {first:?}"
        );
        runtime.shell.sue_menu_open("Send To");
        let send = runtime.shell.sue_menu_labels();
        assert!(
            send.iter().any(|label| label == "Canvas")
                && send.iter().any(|label| label == "Convert"),
            "{send:?}"
        );
        runtime.shell.dismiss_overlays();

        // The picture goes to the Converter, flattened into a PNG.
        let picture = runtime
            .engine
            .world
            .arena
            .iter()
            .find(|(_, entity)| sue_model::picture::is_picture(entity))
            .map(|(_, entity)| entity.id())
            .unwrap();
        let convert = AppletId::new(crate::convert::ID).unwrap();
        runtime.request(
            ShellRequest::SendEntity {
                to: convert.clone(),
                purpose: "add".to_owned(),
                entity: picture,
            },
            &mut NoTextures,
            start,
        );
        runtime.request(
            ShellRequest::AppletCommand {
                applet: convert,
                command: "convert.run".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        let home = runtime.engine.catalog.folders["home"].clone();
        let written = std::fs::read_dir(&home)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.extension().is_some_and(|extension| extension == "png"))
            .expect("the flattened picture was written");
        let png = sue_io::decode_png(&std::fs::read(&written).unwrap()).unwrap();
        let pixels =
            sue_model::picture::size(runtime.engine.world.arena.get_by_id(picture).unwrap())
                .unwrap();
        assert_eq!(
            [png.tile.width(), png.tile.height()],
            pixels,
            "the whole picture, flattened"
        );

        // Send To Canvas leaves the studio; Open In goes back to it.
        let handle = runtime.engine.world.arena.handle_of(picture).unwrap();
        runtime.request(
            ShellRequest::Place {
                entity: handle,
                focus: true,
            },
            &mut NoTextures,
            start,
        );
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        assert_eq!(runtime.shell.studio(), None);
        runtime.request(
            ShellRequest::OpenIn {
                entity: picture,
                applet: paint.clone(),
            },
            &mut NoTextures,
            start,
        );
        assert_eq!(runtime.shell.studio(), Some(&paint));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_writer_makes_a_novel_types_undoes_takes_a_picture_and_compiles() {
        let (mut runtime, root) = runtime("sue-writer");
        let ctx = egui::Context::default();
        let size = vec2(1440.0, 900.0);
        let start = Instant::now();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        runtime.request(ShellRequest::Hub(false), &mut NoTextures, start);
        let writer = AppletId::new(sue_writer::ID).unwrap();
        // A novel from the New File wheel: made, put on the canvas, opened.
        runtime.request(
            ShellRequest::Create {
                applet: writer.clone(),
                key: "novel".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        assert_eq!(runtime.shell.studio(), Some(&writer));
        let arena = &runtime.engine.world.arena;
        let novel = arena
            .iter()
            .find(|(_, entity)| sue_model::manuscript::is_manuscript(entity))
            .map(|(_, entity)| entity.id())
            .expect("a manuscript");
        assert!(
            runtime
                .engine
                .world
                .scene
                .iter()
                .any(|(_, node)| node.applet.as_str() == sue_writer::CARD_ID),
            "its card is on the canvas"
        );
        let draft = sue_model::manuscript::folder(arena, novel, sue_model::manuscript::role::DRAFT)
            .unwrap();
        let scene = sue_model::manuscript::walk(arena, draft)
            .into_iter()
            .map(|(id, _)| id)
            .find(|id| {
                arena
                    .get_by_id(*id)
                    .is_ok_and(sue_model::manuscript::is_document)
            })
            .unwrap();
        let text = |runtime: &Runtime| {
            runtime
                .engine
                .world
                .arena
                .get_by_id(scene)
                .unwrap()
                .string_buffer()
                .unwrap()
                .as_str()
                .to_owned()
        };

        // Typing goes into the first scene (the editor has the keyboard).
        frames(&mut runtime, &ctx, size, start, |frame| {
            if frame == 1 {
                vec![egui::Event::Text("It was a dark night.".to_owned())]
            } else {
                Vec::new()
            }
        });
        assert_eq!(text(&runtime), "It was a dark night.\n");
        // A pause ends the burst of typing.
        runtime.typing.flush(&mut runtime.engine.history);
        // Ctrl+B with nothing selected: what is typed next is bold.
        let key = |key: egui::Key, modifiers: egui::Modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        frames(&mut runtime, &ctx, size, start, |frame| match frame {
            1 => vec![key(egui::Key::B, egui::Modifiers::COMMAND)],
            2 => vec![egui::Event::Text(" Bold.".to_owned())],
            _ => Vec::new(),
        });
        assert_eq!(text(&runtime), "It was a dark night. Bold.\n");
        let marks = runtime
            .engine
            .world
            .arena
            .get_by_id(scene)
            .unwrap()
            .ext::<sue_model::TextMarks>()
            .cloned()
            .unwrap();
        assert!(
            marks.covers(21..26, sue_model::mark_kind::BOLD),
            "{marks:?}"
        );
        // Undo takes the burst back, its formatting too.
        runtime.typing.flush(&mut runtime.engine.history);
        frames(&mut runtime, &ctx, size, start, |frame| {
            if frame == 1 {
                vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]
            } else {
                Vec::new()
            }
        });
        assert_eq!(text(&runtime), "It was a dark night.\n");

        // A Sue Paint picture sent to Sue Writer goes into the text.
        let picture = runtime
            .with_context(&mut NoTextures, start, |cx| {
                sue_paint::sample_picture(cx, 64, 48)
            })
            .unwrap();
        runtime.request(
            ShellRequest::SendEntity {
                to: writer.clone(),
                purpose: "figure".to_owned(),
                entity: picture,
            },
            &mut NoTextures,
            start,
        );
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        let marks = runtime
            .engine
            .world
            .arena
            .get_by_id(scene)
            .unwrap()
            .ext::<sue_model::TextMarks>()
            .cloned()
            .unwrap();
        assert!(
            marks
                .marks()
                .iter()
                .any(|mark| mark.kind == sue_model::mark_kind::EMBED
                    && mark.value == sue_core::Value::Entity(picture)),
            "the picture is in the text"
        );

        // The Sue menu has Sue Writer's own wheel first.
        runtime.request(ShellRequest::SueMenu, &mut NoTextures, start);
        let first = runtime.shell.sue_menu_labels();
        assert_eq!(first[0], "Sue Writer \u{203a}", "{first:?}");
        runtime.shell.dismiss_overlays();

        // Compile to Markdown and to PDF, as the File Manager answers.
        for (format, ext) in [("md", "md"), ("pdf", "pdf"), ("docx", "docx")] {
            let home = runtime.engine.catalog.folders["home"].clone();
            std::fs::create_dir_all(&home).unwrap();
            let path = home.join(format!("novel.{ext}"));
            runtime.request(
                ShellRequest::Picked {
                    pick: FilePick::save(
                        Some(writer.clone()),
                        &format!("compile.{format}"),
                        "Export",
                        "novel",
                    ),
                    path: path.clone(),
                },
                &mut NoTextures,
                start,
            );
            let bytes = std::fs::read(&path)
                .unwrap_or_else(|_| panic!("{format} was written: {:?}", runtime.shell.message()));
            if format == "md" {
                let markdown = String::from_utf8(bytes).unwrap();
                assert!(markdown.contains("Chapter 1"), "{markdown}");
                assert!(markdown.contains("It was a dark night."));
            } else if format == "pdf" {
                assert!(bytes.starts_with(b"%PDF"));
            }
        }
        // Opening a Word file anywhere in Sue brings it into Sue Writer.
        let docs = |runtime: &Runtime| {
            runtime
                .engine
                .world
                .arena
                .iter()
                .filter(|(_, entity)| sue_model::manuscript::is_document(entity))
                .count()
        };
        let before = docs(&runtime);
        let word = runtime.engine.catalog.folders["home"].join("novel.docx");
        runtime.open_path(&word, &mut NoTextures, start);
        assert_eq!(
            docs(&runtime),
            before + 1,
            "the Word file is a document now"
        );
        assert!(
            runtime.engine.world.arena.iter().any(|(_, entity)| entity
                .string_buffer()
                .is_some_and(|text| text.as_str().contains("It was a dark night."))
                && entity.name() == "novel"),
            "with its text"
        );
        assert_eq!(runtime.shell.studio(), Some(&writer));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn spawned_entities_undo_and_redo_with_their_buffers() {
        let (mut runtime, root) = runtime("spawn");
        let ctx = egui::Context::default();
        let start = Instant::now();
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |_| {
            Vec::new()
        });
        let before = runtime.engine.world.scene.len();
        runtime.request(
            ShellRequest::Spawn(AppletId::new(crate::document::ID).unwrap()),
            &mut NoTextures,
            start,
        );
        assert_eq!(runtime.engine.world.scene.len(), before + 1);
        runtime
            .engine
            .history
            .undo(&mut runtime.engine.world)
            .unwrap();
        assert_eq!(runtime.engine.world.scene.len(), before);
        // Undoing a spawn also takes the entity out of the arena.
        runtime
            .engine
            .history
            .redo(&mut runtime.engine.world)
            .unwrap();
        assert_eq!(runtime.engine.world.scene.len(), before + 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}
