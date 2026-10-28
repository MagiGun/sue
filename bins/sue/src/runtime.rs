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
use sue_io::{IngestOptions, IngestView, NativeJxlPipeline, ingest_bytes, meta};
use sue_platform::{
    AudioConfig, AudioOutput, AudioSink, FileFilter, FramePacing, TextureUpload, write_atomic,
};
use sue_rice::{RiceSchema, SetRiceValues};
use sue_wm::{Applets, RiceEdit, Shell, ShellIo, ShellRequest, Theme, Transport, Typing, widgets};

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
    interactive: bool,
    booted: bool,
    note_saved: Option<u64>,
    next_autosave: Option<Instant>,
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
            interactive: false,
            booted: false,
            note_saved: note,
            next_autosave: None,
        })
    }

    /// Lets the runtime use the OS: native dialogs, the file manager, audio.
    #[must_use]
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
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
                    Some(_) | None => {}
                }
            }
        }
    }

    fn run_command(&mut self, command: &str) {
        match command {
            "file.open" => self.pick_and_open(),
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
            "ai.rescan" => {
                self.engine.rebuild_catalog();
                let found = self
                    .engine
                    .catalog
                    .tables
                    .get("models")
                    .map_or(0, |table| table.rows.len());
                self.shell
                    .notify(format!("Found {found} model files"), false);
            }
            "ai.reveal" => {
                let dir = self.engine.model_dir();
                let _ = std::fs::create_dir_all(&dir);
                self.reveal(&dir);
            }
            "audio.start" => {
                if self.engine.audio.is_none() {
                    self.open_audio();
                }
            }
            other => match other.strip_prefix("reveal:") {
                Some(path) => self.reveal(Path::new(path)),
                None => self.shell.notify(format!("Unknown command {other}"), true),
            },
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

    fn pick_and_open(&mut self) {
        if !self.interactive {
            self.shell
                .notify("Choosing files needs the desktop window", false);
            return;
        }
        let start = self.engine.paths.home_dir.clone();
        if let Some(path) = sue_platform::pick_open_file("Open File", Some(&start), &[]) {
            self.engine.requests.push(ShellRequest::OpenFile(path));
        }
    }

    /// Reads a file, puts it in the arena and places it on the canvas in the
    /// applet that shows its content.
    pub fn open_path(&mut self, path: &Path, textures: &mut dyn TextureUpload, now: Instant) {
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
        if !self.interactive {
            self.shell
                .notify("Exporting needs the desktop window", false);
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
                Ok(()) => self
                    .shell
                    .notify(format!("Exported the rice to {}", path.display()), false),
                Err(error) => self.shell.notify(format!("Export failed: {error}"), true),
            }
        }
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
            ShellRequest::Spawn(AppletId::new("paint").unwrap()),
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
