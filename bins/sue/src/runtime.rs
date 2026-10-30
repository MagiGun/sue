//! The runtime shared by the live window and offscreen snapshots: one egui
//! frame of the shell, timers, engine commands, the system's file dialogs,
//! and opening and saving files and projects.
//!
//! Pacing: frames keep coming while a spring moves (the shell asks
//! egui for them with `request_repaint`) and for a short riced grace period
//! after any change. Then the window sleeps at 0 FPS, waking only for timed
//! work: the audio queue while something plays, a status message to clear,
//! a typing burst to close into one undo step, unsaved work to keep.
//!
//! The work Sue remembers ([`WorkList`]): every file opened or saved goes
//! first in the recent list, and work nobody saved is kept by itself
//! (`general.autosave_seconds` after a change, and at quit) for the Hub to
//! recover or discard. Opening another project keeps the unsaved work it
//! replaces the same way.

use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{Align2, Id, Order, RichText, Sense};
use sue_core::{SueFile, UndoStack};
use sue_io::{IngestOptions, NativeJxlPipeline, ingest_bytes, meta};
use sue_platform::time::Instant;
use sue_platform::{
    AudioConfig, AudioOutput, AudioSink, FramePacing, PickNews, Picking, TextureUpload,
    write_atomic,
};
use sue_rice::{RiceSchema, SetRiceValues};
use sue_wm::{
    AppletId, AppletRole, Applets, FilePick, RiceEdit, Shell, ShellIo, ShellRequest, Theme,
    TransferKind, Transfers, Transport, Typing, WorkList, unix_now, widgets,
};

use crate::engine::Engine;
use crate::util::{file_name, human_bytes};

/// How often the audio queue is topped up while something plays.
const FEED_INTERVAL: Duration = Duration::from_millis(40);
/// How often a file dialog open beside Sue (phones and tablets) is asked
/// for its answer.
const PICK_POLL: Duration = Duration::from_millis(200);
/// How often running transfers report their progress.
const TRANSFER_POLL: Duration = Duration::from_millis(100);

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
    // The pages read in Sue's language and name each studio's own.
    .chain(
        rice.entries()
            .iter()
            .filter(|entry| entry.key() == "language")
            .map(|entry| entry.parameter().value().to_string()),
    )
    .chain(std::iter::once(sue_lang::generation().to_string()))
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
            work: &$runtime.work,
            transfers: &mut $runtime.transfers,
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
    /// A file dialog a headless run could not show, kept for scripted runs
    /// to answer ([`Runtime::answer_pick`]).
    pending_pick: Option<FilePick>,
    /// A file dialog open beside Sue (phones and tablets), whose answer
    /// comes later ([`sue_platform::poll_picks`]).
    waiting_pick: Option<FilePick>,
    /// When to ask the system's file pickers for news again, while one is
    /// open or a picked file is being copied.
    next_pick_poll: Option<Instant>,
    /// Where the open project lives, once saved or opened.
    project: Option<PathBuf>,
    /// The open project file, so chunks this build does not know survive.
    project_file: SueFile,
    /// The files of late and the work Sue kept (the Hub's lists).
    work: WorkList,
    /// The files brought in, written out and copied in this run.
    transfers: Transfers,
    /// When to read running transfers' progress next.
    next_transfer_poll: Option<Instant>,
    /// The arena's revision when the work was last looked at for keeping.
    autosave_seen: u64,
    /// When to keep the work next, while a change waits for it.
    next_autosave: Option<Instant>,
    /// What the project was when saved or opened (`None`: unknown, as
    /// after a recovery), so unchanged work is not kept.
    saved_digest: Option<u64>,
    /// What the work kept by this run holds.
    kept_digest: Option<u64>,
    /// The Hub was open in the last frame; its lists are read again each
    /// time it opens.
    hub_was_open: bool,
}

/// Said after opening something over unsaved work, which was kept.
const REPLACED: &str = "; the unsaved work before it is on the Hub, under Unsaved Work";

/// A digest of a project's bytes, to tell whether two are the same.
fn digest(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
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
            pending_pick: None,
            waiting_pick: None,
            next_pick_poll: None,
            project: None,
            project_file: SueFile::new(),
            work: WorkList::default(),
            transfers: Transfers::default(),
            next_transfer_poll: None,
            autosave_seen: 0,
            next_autosave: None,
            saved_digest: None,
            kept_digest: None,
            hub_was_open: false,
        })
    }

    /// Lets the runtime use the OS: native dialogs, the file manager, audio.
    #[must_use]
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self.shell.set_interactive(interactive);
        self.remembering(interactive)
    }

    /// Lets the runtime remember work in Sue's data folder: the files of
    /// late and the work it keeps. Off, nothing is written there (tests and
    /// the self-check run beside people's own lists).
    #[must_use]
    pub fn remembering(mut self, on: bool) -> Self {
        if !on {
            self.work = WorkList::default();
            return self;
        }
        let dir = self.engine.paths.data_dir.join("work");
        let first = !sue_platform::fs::exists(dir.join("recent.txt"));
        self.work = WorkList::open(dir);
        // Sue Paint kept its own recent files before Sue did.
        if first
            && let Ok(text) = sue_platform::fs::read_to_string(
                self.engine.paths.data_dir.join("paint").join("recent.txt"),
            )
        {
            let paint = AppletId::new(sue_paint::ID).ok();
            let now = unix_now();
            let paths: Vec<&str> = text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .collect();
            for (age, path) in paths.iter().enumerate().rev() {
                let when = now.saturating_sub(age as u64);
                let _ = self
                    .work
                    .remember(Path::new(path), false, paint.clone(), when);
            }
        }
        self.work.refresh();
        self.autosave_seen = self.engine.world.arena.revision();
        self
    }

    /// The files of late and the work Sue kept (tests).
    #[cfg(test)]
    #[must_use]
    pub fn work(&self) -> &WorkList {
        &self.work
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
        let _language = sue_lang::scope(&sue_wm::sue_language(&self.engine.world.rice));
        if !self.booted {
            self.booted = true;
            let mut io = shell_io!(self, textures);
            self.shell.boot(&mut io);
        }
        self.poll_system_dialogs(textures, now);
        {
            let mut io = shell_io!(self, textures);
            self.shell.ui(ui, &mut io, now);
        }
        if self.confirm_quit {
            self.quit_dialog(ui.ctx());
        }
        self.serve_requests(textures, now);
        self.refresh_rice();
        // The Hub's lists are read again each time it opens.
        let hub = self.shell.hub_open();
        if hub && !self.hub_was_open {
            self.work.refresh();
        }
        self.hub_was_open = hub;
        self.schedule_autosave(now);
        if self.shell.is_animating() {
            self.telemetry.record(now);
        } else {
            self.telemetry.reset();
        }
        if !ui.ctx().input(|input| input.pointer.any_down())
            && let Some(Err(error)) = self.persist()
        {
            self.shell
                .notify(tr!("could-not-save-the-rice", error = error), true);
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
                                RichText::new(tr!("quit-sue"))
                                    .size(theme.heading_size)
                                    .color(theme.text),
                            );
                            ui.label(
                                RichText::new(tr!("your-settings-are-saved-first"))
                                    .color(theme.muted),
                            );
                            ui.add_space(theme.padding);
                            ui.horizontal(|ui| {
                                if widgets::pill_button(ui, &tr!("quit"), true, &theme).clicked() {
                                    answer = Some(true);
                                }
                                if widgets::pill_button(ui, &tr!("cancel"), false, &theme).clicked()
                                {
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
            self.shell
                .notify(tr!("settings-error", error = error), true);
            return;
        }
        if self.interactive {
            // Threads outside the frame (background jobs) read the global
            // language; frames open a scope of their own.
            sue_lang::set_language(&self.engine.config.language);
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
            self.typing.deadline(),
            self.next_tick,
            self.next_pick_poll,
            self.next_autosave,
            self.next_transfer_poll,
        ]
        .into_iter()
        .flatten()
        .min()
        .map_or(FramePacing::Sleep, FramePacing::WakeAt)
    }

    /// Runs timed work that is due. Returns `true` if the window must be
    /// redrawn.
    pub fn timer(&mut self, textures: &mut dyn TextureUpload, now: Instant) -> bool {
        let _language = sue_lang::scope(&sue_wm::sue_language(&self.engine.world.rice));
        let mut redraw = self.shell.expire(now);
        redraw |= self.poll_system_dialogs(textures, now);
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
        if self.next_autosave.is_some_and(|due| due <= now) {
            self.next_autosave = None;
            self.autosave();
            redraw = true;
        }
        if self.next_transfer_poll.is_none_or(|due| due <= now) {
            let seen = self.transfers.revision();
            self.next_transfer_poll = self.transfers.poll().then_some(now + TRANSFER_POLL);
            if self.transfers.revision() != seen {
                // The start pages show the progress.
                self.shell.touch();
                redraw = true;
            }
        }
        self.schedule_autosave(now);
        redraw
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

    /// Saves what must survive: the open typing group, the rice, and the
    /// work nobody saved (kept for the Hub to offer next time).
    pub fn shutdown(&mut self) {
        self.typing.flush(&mut self.engine.history);
        if let Some(Err(error)) = self.persist() {
            eprintln!("sue: could not save the rice: {error}");
        }
        self.autosave();
    }

    /// How often unsaved work is kept, or `None` when it is not (no folder
    /// to keep it in, or `general.autosave_seconds` is 0).
    fn autosave_interval(&self) -> Option<Duration> {
        self.work.dir()?;
        let seconds = self
            .engine
            .world
            .rice
            .get_i64("general.autosave_seconds")
            .unwrap_or(60);
        u64::try_from(seconds)
            .ok()
            .filter(|seconds| *seconds > 0)
            .map(Duration::from_secs)
    }

    /// After a change, keeps the work once the riced interval has passed.
    fn schedule_autosave(&mut self, now: Instant) {
        if self.next_autosave.is_none()
            && self.engine.world.arena.revision() != self.autosave_seen
            && let Some(interval) = self.autosave_interval()
        {
            self.next_autosave = Some(now + interval);
        }
    }

    /// The project as it would be saved now.
    fn project_bytes(&mut self) -> Vec<u8> {
        self.engine.world.write_into(&mut self.project_file);
        self.project_file.to_bytes()
    }

    /// Keeps the work if it changed and nobody saved it, for the Hub to
    /// offer under Unsaved Work; lets the kept copy go once the work is
    /// saved (or empty) again.
    fn autosave(&mut self) {
        if self.autosave_interval().is_none() {
            return;
        }
        let revision = self.engine.world.arena.revision();
        if revision == self.autosave_seen {
            return;
        }
        self.autosave_seen = revision;
        if self.engine.world.arena.is_empty() && self.project.is_none() {
            self.kept_digest = None;
            let _ = self.work.release();
            return;
        }
        let bytes = self.project_bytes();
        let kept = digest(&bytes);
        if Some(kept) == self.saved_digest {
            self.kept_digest = None;
            let _ = self.work.release();
            return;
        }
        if Some(kept) == self.kept_digest {
            return;
        }
        let (name, page) = self.what_is_open();
        match self
            .work
            .keep(&bytes, self.project.as_deref(), &name, page, unix_now())
        {
            Ok(()) => self.kept_digest = Some(kept),
            Err(error) => self
                .shell
                .notify(tr!("could-not-keep-unsaved-work", error = error), true),
        }
    }

    /// Keeps the work about to be replaced if nobody saved it, so the Hub
    /// offers it; what comes next is kept apart from it. Returns `true`
    /// when something was kept.
    fn set_aside(&mut self) -> bool {
        if self.autosave_interval().is_none() {
            return false;
        }
        // Look again, whether or not the timer came.
        self.autosave_seen = self.engine.world.arena.revision().wrapping_add(1);
        self.autosave();
        let kept = self.kept_digest.is_some();
        self.work.start_over();
        self.kept_digest = None;
        self.next_autosave = None;
        kept
    }

    /// What the page shows, by name, and the page: for the kept work's
    /// note.
    fn what_is_open(&self) -> (String, Option<AppletId>) {
        let page = self.shell.page().cloned();
        let focus = page
            .as_ref()
            .and_then(|id| self.applets.get(id))
            .and_then(|applet| applet.focus())
            .and_then(|id| self.engine.world.arena.get_by_id(id).ok())
            .map(|entity| entity.name().to_owned());
        match focus {
            Some(name) => (name, page),
            None => self.first_page(page.as_ref()).map_or_else(
                || (String::new(), page.clone()),
                |(entity, applet)| {
                    let name = self
                        .engine
                        .world
                        .arena
                        .get_by_id(entity)
                        .map(|entity| entity.name().to_owned())
                        .unwrap_or_default();
                    (name, Some(applet))
                },
            ),
        }
    }

    /// The first thing in the project a studio works on (or else a tool),
    /// and where: `prefer` first, when it takes something.
    fn first_page(&self, prefer: Option<&AppletId>) -> Option<(sue_core::EntityId, AppletId)> {
        let mut studio = None;
        let mut tool = None;
        for (_, entity) in self.engine.world.arena.iter() {
            if let Some(prefer) = prefer
                && self
                    .applets
                    .get(prefer)
                    .is_some_and(|applet| applet.accepts(entity))
            {
                return Some((entity.id(), prefer.clone()));
            }
            let Some(applet) = self.applets.page_for(entity) else {
                continue;
            };
            let role = self.applets.manifest(&applet).map(|manifest| manifest.role);
            if role == Some(AppletRole::Studio) {
                studio.get_or_insert((entity.id(), applet));
            } else {
                tool.get_or_insert((entity.id(), applet));
            }
        }
        studio.or(tool)
    }

    /// Opens what the project holds in its studio (`prefer` first). Returns
    /// the studio or tool, if anything opened.
    fn open_first_page(
        &mut self,
        prefer: Option<&AppletId>,
        textures: &mut dyn TextureUpload,
        now: Instant,
    ) -> Option<AppletId> {
        let (entity, applet) = self.first_page(prefer)?;
        self.request(
            ShellRequest::OpenIn {
                entity,
                applet: applet.clone(),
            },
            textures,
            now,
        );
        Some(applet)
    }

    /// Brings back the work kept as `id` and opens it in its studio. The
    /// work it replaces is kept first if nobody saved it.
    fn recover(&mut self, id: &str, textures: &mut dyn TextureUpload, now: Instant) {
        let Some(kept) = self.work.autosave(id).cloned() else {
            self.shell.notify(tr!("that-unsaved-work-is-gone"), true);
            return;
        };
        let file = sue_platform::fs::read(&kept.file)
            .map_err(|error| error.to_string())
            .and_then(|bytes| SueFile::from_bytes(&bytes).map_err(|error| error.to_string()));
        let file = match file {
            Ok(file) => file,
            Err(error) => {
                self.shell.notify(
                    tr!("cannot-recover", title = kept.title(), error = error),
                    true,
                );
                return;
            }
        };
        let replaced = self.set_aside();
        self.typing.flush(&mut self.engine.history);
        self.transport
            .set_playing(false, self.engine.audio.as_deref_mut());
        if let Err(error) = self.engine.world.load_file(&file, &sue_model::registry()) {
            self.shell.notify(
                tr!("cannot-recover", title = kept.title(), error = error),
                true,
            );
            return;
        }
        self.engine.history = UndoStack::new();
        self.typing = Typing::default();
        self.project.clone_from(&kept.project);
        self.project_file = file;
        // This run keeps it from now on, until it is saved.
        self.work.adopt(&kept.id);
        self.saved_digest = None;
        let bytes = self.project_bytes();
        self.kept_digest = Some(digest(&bytes));
        self.autosave_seen = self.engine.world.arena.revision();
        self.shell.notify(
            tr!(
                "recovered-save-keep",
                title = kept.title(),
                value = if replaced { REPLACED } else { "" }
            ),
            false,
        );
        self.open_first_page(kept.page.as_ref(), textures, now);
    }

    /// Throws away the work kept as `id`.
    fn discard(&mut self, id: &str) {
        let title = self
            .work
            .autosave(id)
            .map_or_else(String::new, sue_wm::Autosave::title);
        match self.work.discard(id) {
            Ok(()) => self.shell.notify(tr!("discarded", title = title), false),
            Err(error) => self
                .shell
                .notify(tr!("cannot-discard", title = title, error = error), true),
        }
    }

    /// Puts `path` first in the files of late.
    fn remember(&mut self, path: &Path, saved: bool, page: Option<AppletId>) {
        if !saved {
            self.transfers.done(
                TransferKind::Import,
                &file_name(path),
                Some(path.to_path_buf()),
            );
        }
        if let Err(error) = self.work.remember(path, saved, page, unix_now()) {
            self.shell
                .notify(tr!("cannot-remember-the-recent-files", error = error), true);
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
                break;
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
                    Some(ShellRequest::Remember { path, saved }) => {
                        let page = self.shell.page().cloned();
                        self.remember(&path, saved, page);
                    }
                    Some(ShellRequest::Forget(path)) => {
                        let _ = self.work.forget(&path);
                    }
                    Some(ShellRequest::Recover(id)) => self.recover(&id, textures, now),
                    Some(ShellRequest::Discard(id)) => self.discard(&id),
                    Some(ShellRequest::Labels(edit)) => {
                        if let Err(error) = self.work.edit_labels(&edit) {
                            self.shell.notify(
                                tr!("cannot-remember-the-recent-files", error = error),
                                true,
                            );
                        }
                    }
                    Some(_) | None => {}
                }
            }
        }
        if self.transfers.running() > 0 {
            self.next_transfer_poll.get_or_insert(now + TRANSFER_POLL);
        }
        // Files just written to paths a dialog chose go on to the place
        // the person chose, where the system keeps it apart (phones and
        // tablets).
        sue_platform::deliver_picked_files();
        if sue_platform::picks_busy() {
            self.next_pick_poll.get_or_insert(now + PICK_POLL);
        } else {
            self.next_pick_poll = None;
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
                } else if let Some(link) = other.strip_prefix("open-url:") {
                    self.open_url(link);
                } else {
                    self.shell
                        .notify(tr!("unknown-command", other = other), true);
                }
            }
        }
    }

    /// Opens a web page in the system's browser.
    fn open_url(&mut self, link: &str) {
        if !self.interactive {
            self.shell.notify(tr!("link", link = link), false);
            return;
        }
        if let Err(error) = sue_platform::open_url(link) {
            self.shell
                .notify(tr!("cannot-open-the-link", error = error), true);
        }
    }

    fn reveal(&mut self, dir: &Path) {
        if !self.interactive {
            self.shell.notify(tr!("folder", dir = dir.display()), false);
            return;
        }
        if let Err(error) = sue_platform::reveal_folder(dir) {
            self.shell
                .notify(tr!("cannot-open-the-folder", error = error), true);
        }
    }

    /// Shows a file selected in the system file manager.
    fn reveal_file(&mut self, path: &Path) {
        if !self.interactive {
            self.shell.notify(tr!("file", path = path.display()), false);
            return;
        }
        if let Err(error) = sue_platform::reveal_file(path) {
            self.shell
                .notify(tr!("cannot-show-the-file", error = error), true);
        }
    }

    /// Asks the system's dialog for a file to open.
    fn pick_and_open(&mut self) {
        self.engine
            .requests
            .push(ShellRequest::PickFile(FilePick::open(
                None,
                "open",
                &tr!("open-file"),
            )));
    }

    /// Carries out a file the system's dialog picked for the shell or
    /// engine.
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
                    self.shell.notify(
                        tr!("cannot-save", value = file_name(path), error = error),
                        true,
                    );
                }
            }
            "export-rice" => {
                let text = self.engine.world.rice.to_suerice();
                match write_atomic(path, text.as_bytes()) {
                    Ok(()) => self.shell.notify_saved(path, "rice"),
                    Err(error) => self.shell.notify(tr!("export-failed", error = error), true),
                }
            }
            other => self
                .shell
                .notify(tr!("nothing-asked-file", other = other), true),
        }
    }

    /// Shows the system's open, save or folder dialog for `pick`; the
    /// chosen path comes back as [`ShellRequest::Picked`], at once on
    /// desktops and later on phones and tablets. A headless run keeps the
    /// pick for a script to answer.
    fn pick_with_system_dialog(&mut self, pick: FilePick) {
        if !self.interactive {
            self.shell
                .notify(tr!("choosing-files-needs-desktop-window"), false);
            self.pending_pick = Some(pick);
            return;
        }
        let filters = pick.filters();
        let start = pick
            .start
            .clone()
            .unwrap_or_else(|| self.engine.paths.home_dir.clone());
        let answer = match &pick.save_as {
            Some(name) => sue_platform::pick_save_file(&pick.title, Some(&start), name, &filters),
            None if pick.folders => sue_platform::pick_folder(&pick.title, Some(&start)),
            None => sue_platform::pick_open_file(&pick.title, Some(&start), &filters),
        };
        self.answered(pick, answer);
    }

    /// Carries out a system dialog's answer to `pick`.
    fn answered(&mut self, pick: FilePick, answer: Picking) {
        match answer {
            Picking::Chosen(path) => self
                .engine
                .requests
                .push(ShellRequest::Picked { pick, path }),
            Picking::Cancelled => {}
            Picking::Waiting => self.waiting_pick = Some(pick),
            Picking::Failed(why) => self
                .shell
                .notify(tr!("cannot-choose-a-file", why = why), true),
        }
    }

    /// Takes the answers of file dialogs open beside Sue (phones and
    /// tablets) and serves them. Returns `true` if any came.
    fn poll_system_dialogs(&mut self, textures: &mut dyn TextureUpload, now: Instant) -> bool {
        if self.next_pick_poll.is_none() {
            return false;
        }
        let news = sue_platform::poll_picks();
        self.next_pick_poll = sue_platform::picks_busy().then_some(now + PICK_POLL);
        if news.is_empty() {
            return false;
        }
        for news in news {
            match news {
                PickNews::Answered(answer) => {
                    if let Some(pick) = self.waiting_pick.take() {
                        self.answered(pick, answer);
                    }
                }
                PickNews::Failed(why) => self.shell.notify(why, true),
            }
        }
        self.serve_requests(textures, now);
        true
    }

    /// The file dialog a headless run could not show, if any (tests).
    #[cfg(test)]
    #[must_use]
    pub fn pending_pick(&self) -> Option<&FilePick> {
        self.pending_pick.as_ref()
    }

    /// Answers the pending file dialog with `path`, as if it had been
    /// chosen there (scripted runs, snapshots and tests). Returns `false`
    /// when no dialog was waiting.
    pub fn answer_pick(
        &mut self,
        path: &Path,
        textures: &mut dyn TextureUpload,
        now: Instant,
    ) -> bool {
        let Some(pick) = self.pending_pick.take() else {
            return false;
        };
        self.request(
            ShellRequest::Picked {
                pick,
                path: path.to_path_buf(),
            },
            textures,
            now,
        );
        true
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
                // The system's dialog asks where; `picked` saves.
                let start = self
                    .project
                    .as_ref()
                    .and_then(|path| path.parent().map(Path::to_path_buf))
                    .unwrap_or_else(|| self.engine.paths.home_dir.clone());
                self.engine.requests.push(ShellRequest::PickFile(
                    FilePick::save(
                        None,
                        "save-project",
                        &tr!("save-project"),
                        &tr!("untitled-sue"),
                    )
                    .types(vec![sue_wm::FileType::new(&tr!("sue-project"), &["sue"])])
                    .starting_in(Some(start)),
                ));
                return;
            }
        };
        if let Err(error) = self.save_project_to(&path) {
            self.shell.notify(
                tr!("cannot-save", value = file_name(&path), error = error),
                true,
            );
        }
    }

    /// Writes the project to `path` (atomically) and remembers the path.
    ///
    /// # Errors
    /// The file system's message.
    pub fn save_project_to(&mut self, path: &Path) -> Result<(), String> {
        self.typing.flush(&mut self.engine.history);
        let bytes = self.project_bytes();
        write_atomic(path, &bytes).map_err(|error| error.to_string())?;
        self.project = Some(path.to_path_buf());
        // Saved: nothing is kept for it any more.
        self.saved_digest = Some(digest(&bytes));
        self.kept_digest = None;
        self.autosave_seen = self.engine.world.arena.revision();
        self.next_autosave = None;
        let _ = self.work.release();
        let page = self.shell.page().cloned();
        self.remember(path, true, page);
        self.transfers.done(
            TransferKind::Export,
            &file_name(path),
            Some(path.to_path_buf()),
        );
        self.shell.notify_saved(path, "project");
        Ok(())
    }

    /// Replaces the open document with the project in `bytes`.
    ///
    /// # Errors
    /// Why the project could not be read; nothing changes then.
    pub fn open_project(&mut self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        let file = SueFile::from_bytes(bytes).map_err(|error| error.to_string())?;
        let replaced = self.set_aside();
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
        self.project = Some(path.to_path_buf());
        self.project_file = file;
        if self.work.dir().is_some() {
            let bytes = self.project_bytes();
            self.saved_digest = Some(digest(&bytes));
        }
        self.kept_digest = None;
        self.autosave_seen = self.engine.world.arena.revision();
        self.shell.notify(
            tr!(
                "opened-2",
                value = file_name(path),
                value2 = if replaced { REPLACED } else { "" }
            ),
            false,
        );
        Ok(())
    }

    /// Reads a file into the arena and opens it in the first studio or tool
    /// that works on it. A `.sue` project replaces the document. A file an
    /// applet takes itself (a Word document or a text file, for Sue Writer)
    /// goes to that applet instead.
    pub fn open_path(&mut self, path: &Path, textures: &mut dyn TextureUpload, now: Instant) {
        let taker = self
            .applets
            .iter()
            .find(|(_, applet)| applet.opens_file(path))
            .map(|(manifest, _)| manifest.id.clone());
        if let Some(to) = taker {
            self.remember(path, false, Some(to.clone()));
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
        let bytes = match sue_platform::fs::metadata(path) {
            Ok(metadata) if metadata.is_dir() => Err(tr!("it-is-a-folder")),
            Ok(metadata) if metadata.len() > limit => Err(tr!(
                "over-mib-limit-io-max-ingest",
                value = human_bytes(metadata.len()),
                limit_mib = limit_mib
            )),
            Ok(_) => sue_platform::fs::read(path).map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(reason) => {
                self.shell
                    .notify(tr!("cannot-open", name = name, reason = reason), true);
                return;
            }
        };
        if SueFile::sniff(&bytes) {
            match self.open_project(path, &bytes) {
                Ok(()) => {
                    // Back in the studio it was saved from, if it takes
                    // what the project holds.
                    let prefer = self
                        .work
                        .recent()
                        .find(|file| file.path == path)
                        .and_then(|file| file.page.clone());
                    let page = self.open_first_page(prefer.as_ref(), textures, now);
                    self.remember(path, false, page);
                }
                Err(reason) => self
                    .shell
                    .notify(tr!("cannot-open", name = name, reason = reason), true),
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
                clock: sue_platform::time::since_start,
            },
        ) {
            Ok(ingested) => ingested,
            Err(error) => {
                self.shell
                    .notify(tr!("cannot-open", name = name, reason = error), true);
                return;
            }
        };
        let Ok(entity) = self.engine.world.arena.get_mut(ingested.handle) else {
            return;
        };
        entity.set_meta(meta::SOURCE, path.display().to_string());
        let id = entity.id();
        let page = self
            .engine
            .world
            .arena
            .get(ingested.handle)
            .ok()
            .and_then(|entity| self.applets.page_for(entity));
        let Some(applet) = page else {
            // Nothing shows it: it does not stay in the project unseen.
            let _ = self.engine.world.arena.despawn(ingested.handle);
            self.shell
                .notify(tr!("no-studio-tool-opens-yet", name = name), true);
            return;
        };
        if let Some(note) = ingested.note {
            self.shell.notify(format!("{name}: {note}"), false);
        }
        self.remember(path, false, Some(applet.clone()));
        self.request(ShellRequest::OpenIn { entity: id, applet }, textures, now);
    }

    /// Opens (or reopens) the audio output with the riced device and format.
    fn open_audio(&mut self) {
        if !self.interactive {
            self.engine.audio_error = Some(tr!("there-no-audio-device-mode"));
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
                self.shell.notify(tr!("audio", error = error), true);
                self.engine.audio_error = Some(error.to_string());
            }
        }
        self.audio_config = Some(config);
        self.engine.rebuild_catalog();
    }

    /// Re-reads the rice file and applies what differs as one undoable step.
    fn reload_rice(&mut self) {
        let path = self.engine.rice_path();
        let text = match sue_platform::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                self.shell.notify(
                    tr!("cannot-read", path = path.display(), error = error),
                    true,
                );
                return;
            }
        };
        let disk = match RiceSchema::parse(&text) {
            Ok(disk) => disk,
            Err(error) => {
                self.shell
                    .notify(tr!("rice-file-has-error", error = error), true);
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
                .notify(tr!("rice-file-matches-what-sue-shows"), false);
            return;
        }
        let count = changes.len();
        let txn = SetRiceValues::new(tr!("reload-default-suerice"), changes);
        self.typing.flush(&mut self.engine.history);
        match self
            .engine
            .history
            .apply(&mut self.engine.world, RiceEdit(txn))
        {
            Ok(_) => self
                .shell
                .notify(tr!("reloaded-values-from-disk", count = count), false),
            Err(error) => self.shell.notify(tr!("reload-failed", error = error), true),
        }
        self.refresh_rice();
    }

    fn export_rice(&mut self) {
        // The system's dialog asks where; `picked` writes it.
        let start = self.engine.paths.home_dir.clone();
        self.engine.requests.push(ShellRequest::PickFile(
            FilePick::save(None, "export-rice", &tr!("export-rice"), "my.suerice")
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

    /// Runs `work` with the shell and everything a frame touches (scripted
    /// runs, snapshots and tests), then serves what it asked for.
    pub fn with_shell<R>(
        &mut self,
        textures: &mut dyn TextureUpload,
        now: Instant,
        work: impl FnOnce(&mut Shell, &mut ShellIo<'_>, Instant) -> R,
    ) -> R {
        let result = {
            let mut io = shell_io!(self, textures);
            work(&mut self.shell, &mut io, now)
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
    use sue_platform::{FormFactor, NoTextures};
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
            // egui's clock goes on from the context's last frame, so calls
            // in a row never turn it back.
            let time = ctx.input(|input| input.time) + 0.008;
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
                events: input(count),
                time: Some(time),
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
    fn a_click_on_a_hub_box_glides_its_studio_in_and_then_sleeps() {
        let (mut runtime, root) = runtime("hub");
        let ctx = egui::Context::default();
        let start = Instant::now();
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |_| {
            Vec::new()
        });
        assert!(runtime.shell.hub_open());
        // Studios first, then tools, each a box with its pin.
        let boxes: Vec<&str> = runtime
            .shell
            .hub_boxes()
            .iter()
            .map(|(id, _, _)| id.as_str())
            .collect();
        assert_eq!(
            boxes,
            [
                sue_paint::ID,
                sue_writer::ID,
                crate::viewer::ID,
                crate::convert::ID
            ]
        );
        let (_, rect, _) = runtime.shell.hub_boxes()[0].clone();
        let at = rect.left_top() + vec2(30.0, 30.0);
        let count = frames(
            &mut runtime,
            &ctx,
            vec2(1440.0, 900.0),
            start,
            |frame| match frame {
                0 => vec![egui::Event::PointerMoved(at)],
                1 => vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
                2 => vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                _ => Vec::new(),
            },
        );
        assert!(count > 10, "the page must glide in ({count} frames)");
        assert_eq!(
            runtime.shell.page().map(AppletId::as_str),
            Some(sue_paint::ID)
        );
        assert!(runtime.shell.reveal() > 0.999);
        assert!(!matches!(
            runtime.pacing(start + Duration::from_secs(60)),
            FramePacing::Animate { .. }
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Where Sue Paint's canvas has its middle in a 1440 by 900 window: the
    /// page's middle, moved by Sue Paint's palettes.
    fn paint_middle(runtime: &Runtime) -> Pos2 {
        runtime.shell.page_rect().center() + vec2(17.0, -63.0)
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
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
            &mut NoTextures,
            now,
        );
        // The studio's panels slide in first.
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        assert!(runtime.shell.reveal() > 0.999);
        let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
            frame_with(runtime, &ctx, now, events);
            now += Duration::from_millis(8);
        };
        assert_eq!(
            runtime.shell.page().map(AppletId::as_str),
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
        let alpha = |runtime: &Runtime, x: u32, y: u32| {
            let raster = sue_paint::doc::raster(&runtime.engine.world.arena, layer).unwrap();
            raster.bytes().unwrap()[((y * raster.width() + x) * 4 + 3) as usize]
        };
        let depth = runtime.engine.history.depth();

        // A pen stroke from the middle of the canvas to the right, with pen
        // pressure. The picture is fitted, so the middle is pixel (800, 600).
        let viewport_center = paint_middle(&runtime);
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
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
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
        let from = paint_middle(&runtime);
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
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
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
                .and_then(|applet| applet.focus())
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
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
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
            .and_then(|applet| applet.focus())
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
        let middle = paint_middle(&runtime);

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
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
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
            .and_then(|applet| applet.focus())
            .unwrap();
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let middle = paint_middle(&runtime);
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

        // File, Import, Audio: a sound on an audio track, from the frame
        // shown; playing the animation plays it, stopping stops it.
        std::fs::create_dir_all(&root).unwrap();
        let tone = sue_core::PCMFloat1D::new(8_000, 1, vec![0.2; 8_000]).unwrap();
        std::fs::write(root.join("Tone.wav"), sue_io::encode_wav(&tone)).unwrap();
        runtime.request(
            ShellRequest::Picked {
                pick: FilePick::open(Some(paint.clone()), "anim-audio", "Import Audio"),
                path: root.join("Tone.wav"),
            },
            &mut NoTextures,
            start,
        );
        let sequence = sue_paint::anim::timeline(&runtime.engine.world.arena, picture).unwrap();
        assert_eq!(
            sue_paint::anim::audio_tracks(sequence).len(),
            1,
            "an audio track"
        );
        command(&mut runtime, "anim.first");
        command(&mut runtime, "anim.play");
        step(&mut runtime, Vec::new());
        assert!(
            runtime.transport.is_playing_for("sue-paint"),
            "the sound plays with it"
        );
        command(&mut runtime, "anim.play");
        step(&mut runtime, Vec::new());
        assert!(
            !runtime.transport.is_playing_for("sue-paint"),
            "and stops with it"
        );
        // Back to the second cel's frame for what follows.
        command(&mut runtime, "anim.first");
        for _ in 0..3 {
            command(&mut runtime, "anim.next");
        }

        // Onion Skin Settings keeps its settings on the picture.
        command(&mut runtime, "anim.onion_settings");
        step(&mut runtime, Vec::new());
        step(
            &mut runtime,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        let arena = &runtime.engine.world.arena;
        assert!(
            arena
                .get_by_id(picture)
                .unwrap()
                .property(sue_paint::anim::ONION)
                .is_some()
        );
        assert_eq!(
            sue_paint::anim::Onion::of(arena, picture),
            sue_paint::anim::Onion::default()
        );

        // The light table: the cel being drawn cannot trace itself; the
        // first cel, picked in the Layer palette, goes on its light table.
        let (first, second) = (keys[0].1.unwrap(), keys[1].1.unwrap());
        command(&mut runtime, "anim.light_table.register");
        assert!(sue_paint::anim::light_table(&runtime.engine.world.arena, second).is_empty());
        command(&mut runtime, "layer.select_below");
        command(&mut runtime, "anim.light_table.register");
        assert_eq!(
            sue_paint::anim::light_table(&runtime.engine.world.arena, second),
            vec![first]
        );
        step(
            &mut runtime,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
        assert!(sue_paint::anim::light_table(&runtime.engine.world.arena, second).is_empty());

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
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
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
            .and_then(|applet| applet.focus())
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
        let middle = paint_middle(&runtime);
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
    fn sue_paint_folders_take_layers_in_and_out_as_csp_does() {
        let (mut runtime, root) = runtime("paint-folders");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
            &mut NoTextures,
            now,
        );
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        let paint = AppletId::new(sue_paint::ID).unwrap();
        let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
            frame_with(runtime, &ctx, now, events);
            now += Duration::from_millis(8);
        };
        let command = egui::Modifiers::COMMAND;
        step(&mut runtime, vec![key(egui::Key::N, command)]);
        step(
            &mut runtime,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        step(&mut runtime, Vec::new());
        let picture = runtime
            .applets
            .get(&paint)
            .and_then(|applet| applet.focus())
            .unwrap();
        step(
            &mut runtime,
            vec![key(egui::Key::N, command | egui::Modifiers::SHIFT)],
        );
        let stack = |runtime: &Runtime| -> Vec<sue_core::EntityId> {
            sue_paint::doc::layers(&runtime.engine.world.arena, picture)
                .into_iter()
                .map(|info| info.id)
                .collect()
        };
        let [_, first, second] = stack(&runtime)[..] else {
            panic!("paper and two layers");
        };
        // Ctrl+G: a folder over the second layer, holding it.
        step(&mut runtime, vec![key(egui::Key::G, command)]);
        let folder = stack(&runtime)[3];
        let folder_of = |runtime: &Runtime, layer| {
            sue_paint::doc::folder_of(&runtime.engine.world.arena, layer)
        };
        assert_eq!(folder_of(&runtime, second), Some(folder));
        // Back to the second layer (Alt+[), then up past the folder: out.
        step(
            &mut runtime,
            vec![key(egui::Key::OpenBracket, egui::Modifiers::ALT)],
        );
        step(&mut runtime, vec![key(egui::Key::CloseBracket, command)]);
        assert_eq!(folder_of(&runtime, second), None);
        assert_eq!(stack(&runtime)[1..], [first, folder, second]);
        // Down onto the folder: in, as its top layer.
        step(&mut runtime, vec![key(egui::Key::OpenBracket, command)]);
        assert_eq!(folder_of(&runtime, second), Some(folder));
        assert_eq!(stack(&runtime)[1..], [first, second, folder]);
        // Down past its bottom: out, staying put.
        step(&mut runtime, vec![key(egui::Key::OpenBracket, command)]);
        assert_eq!(folder_of(&runtime, second), None);
        assert_eq!(stack(&runtime)[1..], [first, second, folder]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_crops_to_the_selection_in_one_undo_step() {
        let (mut runtime, root) = runtime("paint-crop");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
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
            .and_then(|applet| applet.focus())
            .unwrap();
        let size = |runtime: &Runtime| {
            sue_model::picture::size(runtime.engine.world.arena.get_by_id(picture).unwrap())
                .unwrap()
        };
        assert_eq!(size(&runtime), [1600, 1200]);
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let middle = paint_middle(&runtime);
        // M picks the rectangle selection; drag a box around the middle.
        step(&mut runtime, vec![key(egui::Key::M, egui::Modifiers::NONE)]);
        let (a, b) = (middle - vec2(40.0, 30.0), middle + vec2(40.0, 30.0));
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(a), press(a, true)],
        );
        step(&mut runtime, vec![egui::Event::PointerMoved(b)]);
        step(&mut runtime, vec![press(b, false)]);
        let depth = runtime.engine.history.depth();
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: "edit.crop".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        let [width, height] = size(&runtime);
        assert!(
            width < 400 && height < 300 && width > height,
            "cropped to {width} by {height}"
        );
        assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");
        let layer = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
            .last()
            .unwrap()
            .id;
        assert_eq!(
            sue_paint::doc::raster(&runtime.engine.world.arena, layer)
                .unwrap()
                .width(),
            width
        );
        step(
            &mut runtime,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
        assert_eq!(size(&runtime), [1600, 1200]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_symmetry_ruler_mirrors_strokes_until_turned_off() {
        let (mut runtime, root) = runtime("paint-symmetry");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
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
        let picture = runtime
            .applets
            .get(&paint)
            .and_then(|applet| applet.focus())
            .unwrap();
        let layer = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
            .last()
            .unwrap()
            .id;
        let ink = |runtime: &Runtime, columns: std::ops::Range<u32>, rows: std::ops::Range<u32>| {
            let raster = sue_paint::doc::raster(&runtime.engine.world.arena, layer).unwrap();
            let bytes = raster.bytes().unwrap();
            rows.flat_map(|y| columns.clone().map(move |x| (x, y)))
                .map(|(x, y)| u32::from(bytes[((y * raster.width() + x) * 4 + 3) as usize]))
                .sum::<u32>()
        };
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let middle = paint_middle(&runtime);
        // U three times picks the Ruler tool and comma steps back round to
        // the Symmetrical Ruler; a click lays a vertical ruler.
        for _ in 0..3 {
            step(&mut runtime, vec![key(egui::Key::U, plain)]);
        }
        step(&mut runtime, vec![key(egui::Key::Comma, plain)]);
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(middle), press(middle, true)],
        );
        step(&mut runtime, vec![press(middle, false)]);
        // A pen stroke on the left only.
        step(&mut runtime, vec![key(egui::Key::P, plain)]);
        let mut stroke = |runtime: &mut Runtime, y: f32| {
            let (a, b) = (middle + vec2(-80.0, y), middle + vec2(-30.0, y));
            step(runtime, vec![egui::Event::PointerMoved(a), press(a, true)]);
            step(
                runtime,
                vec![
                    egui::Event::PointerMoved(middle + vec2(-55.0, y)),
                    egui::Event::PointerMoved(b),
                ],
            );
            step(runtime, vec![press(b, false)]);
        };
        stroke(&mut runtime, 0.0);
        assert!(ink(&runtime, 650..740, 580..620) > 0, "drawn on the left");
        assert!(
            ink(&runtime, 860..950, 580..620) > 0,
            "mirrored on the right"
        );
        // Snap to Special Ruler (Ctrl+2) turns it off: the next stroke
        // stays on its side.
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: "view.snap_special".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        stroke(&mut runtime, 60.0);
        assert!(ink(&runtime, 650..740, 690..760) > 0);
        assert_eq!(
            ink(&runtime, 860..950, 690..760),
            0,
            "no mirror with the ruler off"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_perspective_rulers_hold_strokes_and_take_new_vanishing_points() {
        let (mut runtime, root) = runtime("paint-perspective");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
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
        let picture = runtime
            .applets
            .get(&paint)
            .and_then(|applet| applet.focus())
            .unwrap();
        let layer = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
            .last()
            .unwrap()
            .id;
        let points = |runtime: &Runtime| {
            sue_paint::rulers::Rulers::of(&runtime.engine.world.arena, picture)
                .perspective
                .map_or(0, |ruler| ruler.points.len())
        };
        let command = |runtime: &mut Runtime, command: &str| {
            runtime.request(
                ShellRequest::AppletCommand {
                    applet: paint.clone(),
                    command: command.to_owned(),
                },
                &mut NoTextures,
                start,
            );
        };
        // Layer, Ruler/Frame, Create Perspective Ruler: 2-point by default.
        command(&mut runtime, "layer.ruler.perspective");
        step(&mut runtime, Vec::new());
        step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
        step(&mut runtime, Vec::new());
        assert_eq!(points(&runtime), 2);
        // How far across the ink on the layer reaches.
        let width = |runtime: &Runtime| {
            let raster = sue_paint::doc::raster(&runtime.engine.world.arena, layer).unwrap();
            let (w, bytes) = (raster.width(), raster.bytes().unwrap());
            let columns: Vec<u32> = (0..raster.height())
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .filter(|(x, y)| bytes[((y * w + x) * 4 + 3) as usize] > 0)
                .map(|(x, _)| x)
                .collect();
            columns.iter().max().unwrap() - columns.iter().min().unwrap()
        };
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let drag = |runtime: &mut Runtime,
                    step: &mut dyn FnMut(&mut Runtime, Vec<egui::Event>),
                    from: egui::Pos2,
                    to: egui::Pos2| {
            step(
                runtime,
                vec![egui::Event::PointerMoved(from), press(from, true)],
            );
            for part in 1..=8 {
                let at = from + (to - from) * (part as f32 / 8.0);
                step(runtime, vec![egui::Event::PointerMoved(at)]);
            }
            step(runtime, vec![press(to, false)]);
            let _ = runtime;
        };
        // A pen stroke heading down but drifting right runs straight up
        // and down, the upright of two-point perspective.
        let middle = runtime.shell.page_rect().center();
        let (top, bottom) = (middle + vec2(-20.0, -150.0), middle + vec2(10.0, 10.0));
        step(&mut runtime, vec![key(egui::Key::P, plain)]);
        drag(&mut runtime, &mut step, top, bottom);
        let snapped = width(&runtime);
        step(
            &mut runtime,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
        // With Snap to Special Ruler off it drifts as drawn.
        command(&mut runtime, "view.snap_special");
        drag(&mut runtime, &mut step, top, bottom);
        let free = width(&runtime);
        assert!(snapped * 2 < free, "{snapped} against {free}");
        command(&mut runtime, "view.snap_special");
        // Back to the Ruler tool, on the Perspective Ruler that Create
        // Perspective Ruler picked: two lines that meet make a third
        // vanishing point.
        for _ in 0..3 {
            step(&mut runtime, vec![key(egui::Key::U, plain)]);
        }
        drag(
            &mut runtime,
            &mut step,
            middle + vec2(-200.0, -100.0),
            middle + vec2(-100.0, -60.0),
        );
        assert_eq!(points(&runtime), 2, "one line is not a point yet");
        drag(
            &mut runtime,
            &mut step,
            middle + vec2(-200.0, 100.0),
            middle + vec2(-100.0, 60.0),
        );
        assert_eq!(points(&runtime), 3);
        // Delete Ruler takes every ruler away, and undo brings them back.
        command(&mut runtime, "layer.ruler.delete");
        assert_eq!(points(&runtime), 0);
        step(
            &mut runtime,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
        assert_eq!(points(&runtime), 3);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_vector_layers_keep_lines_and_erase_up_to_intersections() {
        let (mut runtime, root) = runtime("paint-vector");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
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
        let picture = runtime
            .applets
            .get(&paint)
            .and_then(|applet| applet.focus())
            .unwrap();
        let command = |runtime: &mut Runtime, command: &str| {
            runtime.request(
                ShellRequest::AppletCommand {
                    applet: paint.clone(),
                    command: command.to_owned(),
                },
                &mut NoTextures,
                start,
            );
        };
        // Layer, New Layer, Vector Layer.
        command(&mut runtime, "layer.new.vector");
        let layer = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
            .last()
            .unwrap()
            .id;
        assert!(sue_paint::vector::is_vector(
            &runtime.engine.world.arena,
            layer
        ));
        let lines = |runtime: &Runtime| {
            sue_paint::vector::lines(&sue_paint::vector::geometry(
                &runtime.engine.world.arena,
                layer,
            ))
        };
        // How far right the ink on the layer reaches.
        let right = |runtime: &Runtime| {
            let raster = sue_paint::doc::raster(&runtime.engine.world.arena, layer).unwrap();
            let (w, bytes) = (raster.width(), raster.bytes().unwrap());
            (0..raster.height())
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .filter(|(x, y)| bytes[((y * w + x) * 4 + 3) as usize] > 0)
                .map(|(x, _)| x)
                .max()
                .unwrap_or(0)
        };
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let drag = |runtime: &mut Runtime,
                    step: &mut dyn FnMut(&mut Runtime, Vec<egui::Event>),
                    from: egui::Pos2,
                    to: egui::Pos2| {
            step(
                runtime,
                vec![egui::Event::PointerMoved(from), press(from, true)],
            );
            for part in 1..=8 {
                let at = from + (to - from) * (part as f32 / 8.0);
                step(runtime, vec![egui::Event::PointerMoved(at)]);
            }
            step(runtime, vec![press(to, false)]);
        };
        // A level line crossed by an upright one: two lines, each one step.
        let middle = runtime.shell.page_rect().center();
        step(&mut runtime, vec![key(egui::Key::P, plain)]);
        drag(
            &mut runtime,
            &mut step,
            middle + vec2(-120.0, 0.0),
            middle + vec2(120.0, 0.0),
        );
        drag(
            &mut runtime,
            &mut step,
            middle + vec2(0.0, -120.0),
            middle + vec2(0.0, 120.0),
        );
        assert_eq!(lines(&runtime).len(), 2);
        let full = right(&runtime);
        command(&mut runtime, "edit.undo");
        assert_eq!(lines(&runtime).len(), 1, "undo takes the line away");
        command(&mut runtime, "edit.redo");
        assert_eq!(lines(&runtime).len(), 2);
        // The Vector eraser (up to intersection) on the level line's right
        // end takes it away up to the upright.
        step(&mut runtime, vec![key(egui::Key::E, plain)]);
        for _ in 0..3 {
            step(&mut runtime, vec![key(egui::Key::Period, plain)]);
        }
        let at = middle + vec2(80.0, 0.0);
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(at), press(at, true)],
        );
        step(&mut runtime, vec![press(at, false)]);
        assert_eq!(lines(&runtime).len(), 2, "the left part and the upright");
        let cut = right(&runtime);
        let upright = lines(&runtime)[1].samples[0].at[0] as u32;
        assert!(
            cut < full && cut <= upright + 12,
            "{cut} against {full}, upright at {upright}"
        );
        command(&mut runtime, "edit.undo");
        assert_eq!(right(&runtime), full, "undo brings the line back");
        assert_eq!(lines(&runtime).len(), 2);

        // Correct Line (Y), Pinch vector line: dragging the level line's
        // middle up pulls it into a curve; undo puts it back.
        let drawn = lines(&runtime);
        step(&mut runtime, vec![key(egui::Key::Y, plain)]);
        step(&mut runtime, vec![key(egui::Key::Period, plain)]);
        drag(
            &mut runtime,
            &mut step,
            middle + vec2(-60.0, 0.0),
            middle + vec2(-60.0, -40.0),
        );
        let pulled = lines(&runtime);
        let lift = |line: &sue_paint::vector::Line| {
            line.samples
                .iter()
                .map(|sample| sample.at[1])
                .fold(f32::INFINITY, f32::min)
        };
        assert!(
            lift(&pulled[0]) < lift(&drawn[0]) - 5.0,
            "the line is pulled up"
        );
        assert_eq!(pulled[1], drawn[1], "the other line stays");
        command(&mut runtime, "edit.undo");
        assert_eq!(lines(&runtime), drawn);

        // The Operation tool's Object (O): drag the upright to move it,
        // Delete takes it away, undo brings it back.
        step(&mut runtime, vec![key(egui::Key::O, plain)]);
        drag(
            &mut runtime,
            &mut step,
            middle + vec2(0.0, 80.0),
            middle + vec2(40.0, 80.0),
        );
        let moved = lines(&runtime);
        assert!(
            moved[1].samples[0].at[0] > drawn[1].samples[0].at[0] + 10.0,
            "moved right"
        );
        assert_eq!(moved[0], drawn[0], "the other line stays");
        command(&mut runtime, "edit.clear");
        assert_eq!(lines(&runtime).len(), 1);
        command(&mut runtime, "edit.undo");
        assert_eq!(lines(&runtime), moved);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_records_a_timelapse_and_exports_it_as_a_video() {
        let (mut runtime, root) = runtime("paint-timelapse");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
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
        let picture = runtime
            .applets
            .get(&paint)
            .and_then(|applet| applet.focus())
            .unwrap();
        let command = |runtime: &mut Runtime, command: &str| {
            runtime.request(
                ShellRequest::AppletCommand {
                    applet: paint.clone(),
                    command: command.to_owned(),
                },
                &mut NoTextures,
                start,
            );
        };
        // File, Timelapse, Record Timelapse: the picture keeps that, and a
        // frame goes in for the canvas as it is and after each stroke.
        command(&mut runtime, "file.timelapse.record");
        for _ in 0..3 {
            step(&mut runtime, Vec::new());
        }
        let dir = sue_paint::timelapse::folder(&runtime.engine.catalog.folders["paint"], picture);
        assert_eq!(sue_paint::timelapse::frames(&dir).len(), 1);
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let middle = runtime.shell.page_rect().center();
        step(&mut runtime, vec![key(egui::Key::P, plain)]);
        for y in [-40.0, 40.0] {
            let (a, b) = (middle + vec2(-80.0, y), middle + vec2(80.0, y));
            step(
                &mut runtime,
                vec![egui::Event::PointerMoved(a), press(a, true)],
            );
            step(
                &mut runtime,
                vec![
                    egui::Event::PointerMoved(middle + vec2(0.0, y)),
                    egui::Event::PointerMoved(b),
                ],
            );
            step(&mut runtime, vec![press(b, false)]);
            for _ in 0..3 {
                step(&mut runtime, Vec::new());
            }
        }
        assert_eq!(sue_paint::timelapse::frames(&dir).len(), 3, "one a stroke");
        // Export Timelapse writes a video of them.
        std::fs::create_dir_all(&root).unwrap();
        runtime.request(
            ShellRequest::Picked {
                pick: FilePick::save(
                    Some(paint.clone()),
                    "timelapse",
                    "Export Timelapse",
                    "Drawing.avi",
                ),
                path: root.join("Drawing.avi"),
            },
            &mut NoTextures,
            start,
        );
        let video = std::fs::read(root.join("Drawing.avi")).unwrap();
        assert_eq!(&video[..4], b"RIFF");
        assert_eq!(&video[8..12], b"AVI ");
        // Stopping keeps the frames but adds no more.
        command(&mut runtime, "file.timelapse.record");
        let a = middle + vec2(-80.0, 80.0);
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(a), press(a, true)],
        );
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(a + vec2(60.0, 0.0))],
        );
        step(&mut runtime, vec![press(a + vec2(60.0, 0.0), false)]);
        for _ in 0..3 {
            step(&mut runtime, Vec::new());
        }
        assert_eq!(sue_paint::timelapse::frames(&dir).len(), 3);
        let _ = std::fs::remove_dir_all(&dir);

        // Edit, Register Material, Image keeps the drawing in the Material
        // palette; pasting it makes a new layer with it in the middle.
        let materials = runtime.engine.catalog.folders["paint"].join("materials");
        let _ = std::fs::remove_dir_all(&materials);
        command(&mut runtime, "edit.material.image");
        step(&mut runtime, Vec::new());
        step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
        assert_eq!(
            std::fs::read_dir(&materials).unwrap().count(),
            1,
            "one material file"
        );
        let before = sue_paint::doc::layers(&runtime.engine.world.arena, picture).len();
        command(&mut runtime, "edit.material.paste");
        let stack = sue_paint::doc::layers(&runtime.engine.world.arena, picture);
        assert_eq!(stack.len(), before + 1);
        let pasted =
            sue_paint::doc::raster(&runtime.engine.world.arena, stack.last().unwrap().id).unwrap();
        assert!(
            pasted.bytes().unwrap().chunks(4).any(|pixel| pixel[3] > 0),
            "with the drawing on it"
        );
        let _ = std::fs::remove_dir_all(&materials);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_selections_shrink_become_layers_and_quick_masks() {
        let (mut runtime, root) = runtime("paint-selection");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
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
        let picture = runtime
            .applets
            .get(&paint)
            .and_then(|applet| applet.focus())
            .unwrap();
        let command = |runtime: &mut Runtime, command: &str| {
            runtime.request(
                ShellRequest::AppletCommand {
                    applet: paint.clone(),
                    command: command.to_owned(),
                },
                &mut NoTextures,
                start,
            );
        };
        let named = |runtime: &Runtime, name: &str| {
            sue_paint::doc::layers(&runtime.engine.world.arena, picture)
                .into_iter()
                .find(|info| info.name == name)
        };
        // Select All, Shrink Selected Area by 4, Convert to Selection Layer:
        // the layer holds the selection, 4 pixels in from every edge.
        command(&mut runtime, "select.all");
        command(&mut runtime, "select.shrink");
        step(&mut runtime, Vec::new());
        step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
        command(&mut runtime, "select.to_layer");
        let layer = named(&runtime, "Selection").expect("a selection layer");
        assert!(layer.style.draft, "left out of exports");
        let raster = sue_paint::doc::raster(&runtime.engine.world.arena, layer.id).unwrap();
        let selection = sue_paint::select::Selection::from_alpha(raster);
        let (w, h) = (raster.width(), raster.height());
        assert_eq!(selection.at(4, 4), 1.0);
        assert_eq!(selection.at(w - 5, h - 5), 1.0);
        assert_eq!(selection.at(2, 2), 0.0, "drawn in from the edge");
        assert_eq!(selection.at(w - 3, h / 2), 0.0);
        // Quick Mask: a red layer to paint the selection on, and back.
        command(&mut runtime, "select.quick_mask");
        assert!(named(&runtime, "Quick Mask").is_some());
        command(&mut runtime, "select.quick_mask");
        assert!(named(&runtime, "Quick Mask").is_none(), "gone again");

        // Convert to Lines and Tones: a lines layer over a tone layer.
        command(&mut runtime, "layer.select_below");
        command(&mut runtime, "filter.lines_tones");
        let tones = named(&runtime, "Tones").expect("a tone layer");
        assert!(named(&runtime, "Lines").is_some(), "and a lines layer");
        assert!(
            sue_paint::effects::Effects::of(
                runtime.engine.world.arena.get_by_id(tones.id).unwrap(),
                72.0
            )
            .tone
            .is_some()
        );

        // Export as PDF and as EPUB.
        std::fs::create_dir_all(&root).unwrap();
        for (purpose, file) in [("book-pdf", "Book.pdf"), ("book-epub", "Book.epub")] {
            runtime.request(
                ShellRequest::Picked {
                    pick: FilePick::save(Some(paint.clone()), purpose, "Export", file),
                    path: root.join(file),
                },
                &mut NoTextures,
                start,
            );
        }
        assert!(
            std::fs::read(root.join("Book.pdf"))
                .unwrap()
                .starts_with(b"%PDF")
        );
        let epub = std::fs::read(root.join("Book.epub")).unwrap();
        let files = sue_io::zip::read_zip(&epub).unwrap();
        assert_eq!(files[0].0, "mimetype");
        assert!(
            files
                .iter()
                .any(|(name, _)| name == "OEBPS/page-0001.xhtml")
        );

        // Export Webtoon: OK keeps the cut and width (1280 and 800), then
        // the picture goes out scaled to 800 wide in numbered pieces.
        command(&mut runtime, "file.export.webtoon");
        step(&mut runtime, Vec::new());
        step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
        runtime.request(
            ShellRequest::Picked {
                pick: FilePick::save(
                    Some(paint.clone()),
                    "webtoon",
                    "Export Webtoon",
                    "Strip.png",
                ),
                path: root.join("Strip.png"),
            },
            &mut NoTextures,
            start,
        );
        let piece = std::fs::read(root.join("Strip-001.png")).expect("the first piece");
        let tile = sue_io::decode_png(&piece).unwrap().tile;
        assert_eq!(tile.width(), 800);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_3d_layers_turn_and_become_lines_and_tones() {
        let (mut runtime, root) = runtime("paint-3d");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
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
        let picture = runtime
            .applets
            .get(&paint)
            .and_then(|applet| applet.focus())
            .unwrap();
        let command = |runtime: &mut Runtime, command: &str| {
            runtime.request(
                ShellRequest::AppletCommand {
                    applet: paint.clone(),
                    command: command.to_owned(),
                },
                &mut NoTextures,
                start,
            );
        };
        let three_d = |runtime: &Runtime| {
            sue_paint::doc::layers(&runtime.engine.world.arena, picture)
                .into_iter()
                .find(|info| sue_paint::three::is_3d(&runtime.engine.world.arena, info.id))
                .map(|info| info.id)
        };
        // Layer, New Layer, 3D Layer: a cube, drawn.
        command(&mut runtime, "layer.new.3d");
        step(&mut runtime, Vec::new());
        step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
        let layer = three_d(&runtime).expect("a 3D layer");
        let arena = &runtime.engine.world.arena;
        assert!(sue_paint::three::model_of(arena, layer).is_some());
        let drawn = sue_paint::doc::raster(arena, layer)
            .unwrap()
            .bytes()
            .unwrap()
            .to_vec();
        assert!(drawn.chunks(4).any(|pixel| pixel[3] > 0));
        let yaw = |runtime: &Runtime| {
            sue_paint::three::View3D::of(runtime.engine.world.arena.get_by_id(layer).unwrap()).yaw
        };
        let before = yaw(&runtime);
        // The Object tool (picked for a new 3D layer) turns the camera.
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let middle = runtime.shell.page_rect().center();
        step(&mut runtime, vec![egui::Event::PointerMoved(middle)]);
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(middle), press(middle, true)],
        );
        for part in 1..=4 {
            step(
                &mut runtime,
                vec![egui::Event::PointerMoved(
                    middle + vec2(25.0 * part as f32, 0.0),
                )],
            );
        }
        step(&mut runtime, vec![press(middle + vec2(100.0, 0.0), false)]);
        let after = yaw(&runtime);
        assert!(
            (after - (before - 40.0)).abs() < 1.0,
            "turned by the drag: {before} to {after}"
        );
        assert_ne!(
            sue_paint::doc::raster(&runtime.engine.world.arena, layer)
                .unwrap()
                .bytes()
                .unwrap(),
            &drawn[..]
        );
        command(&mut runtime, "edit.undo");
        assert!((yaw(&runtime) - before).abs() < 1e-3, "one step to undo");
        assert_eq!(
            sue_paint::doc::raster(&runtime.engine.world.arena, layer)
                .unwrap()
                .bytes()
                .unwrap(),
            &drawn[..]
        );
        // Convert to Lines and Tones from the model.
        command(&mut runtime, "filter.lines_tones");
        let names: Vec<String> = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
            .into_iter()
            .map(|info| info.name)
            .collect();
        assert!(
            names.contains(&"Lines".to_owned()) && names.contains(&"Tones".to_owned()),
            "{names:?}"
        );
        let lines = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
            .into_iter()
            .find(|info| info.name == "Lines")
            .unwrap();
        let raster = sue_paint::doc::raster(&runtime.engine.world.arena, lines.id).unwrap();
        assert!(
            raster
                .bytes()
                .unwrap()
                .chunks(4)
                .any(|pixel| pixel[3] > 200),
            "outlines drawn"
        );
        // File, Import, 3D Data: an OBJ model becomes a 3D layer.
        std::fs::create_dir_all(&root).unwrap();
        let obj = root.join("Wedge.obj");
        std::fs::write(
            &obj,
            "v 0 0 0
v 1 0 0
v 0 1 0
v 0 0 1
f 1 2 3
f 1 2 4
f 1 3 4
f 2 3 4
",
        )
        .unwrap();
        runtime.request(
            ShellRequest::Picked {
                pick: FilePick::open(Some(paint.clone()), "model3d", "Import 3D Data"),
                path: obj,
            },
            &mut NoTextures,
            start,
        );
        assert!(
            sue_paint::doc::layers(&runtime.engine.world.arena, picture)
                .iter()
                .any(|info| info.name == "Wedge"
                    && sue_paint::three::is_3d(&runtime.engine.world.arena, info.id))
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_masks_hide_without_erasing_the_layer() {
        let (mut runtime, root) = runtime("paint-mask");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
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
        let picture = runtime
            .applets
            .get(&paint)
            .and_then(|applet| applet.focus())
            .unwrap();
        let layer = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
            .last()
            .unwrap()
            .id;
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let middle = paint_middle(&runtime);
        let mut drag = |runtime: &mut Runtime, before: Vec<egui::Event>, a, b| {
            step(runtime, before);
            step(runtime, vec![egui::Event::PointerMoved(a), press(a, true)]);
            step(runtime, vec![egui::Event::PointerMoved(b)]);
            step(runtime, vec![press(b, false)]);
        };
        drag(
            &mut runtime,
            Vec::new(),
            middle - vec2(60.0, 0.0),
            middle + vec2(60.0, 0.0),
        );
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
        command(&mut runtime, "layer.mask.outside");
        command(&mut runtime, "layer.mask.edit");
        let mask = sue_paint::doc::mask_of(&runtime.engine.world.arena, layer).expect("a mask");
        // The eraser (E) on the mask along the stroke's left half.
        drag(
            &mut runtime,
            vec![key(egui::Key::E, plain)],
            middle - vec2(60.0, 0.0),
            middle - vec2(15.0, 0.0),
        );
        let arena = &runtime.engine.world.arena;
        let alpha_of = |raster: &sue_core::RasterTile2D, x: u32| {
            raster.bytes().unwrap()[((600 * raster.width() + x) * 4 + 3) as usize]
        };
        let options = sue_paint::composite::RenderOptions {
            paper: false,
            drafts: true,
        };
        let shown = sue_paint::composite::flatten(arena, picture, options).unwrap();
        assert!(
            alpha_of(sue_paint::doc::raster(arena, layer).unwrap(), 720) > 200,
            "the layer keeps its pixels"
        );
        assert!(
            alpha_of(sue_paint::doc::raster(arena, mask).unwrap(), 720) < 50,
            "the mask hides them"
        );
        assert!(
            alpha_of(&shown, 720) < 50,
            "so the picture does not show them"
        );
        assert!(alpha_of(&shown, 880) > 200, "the rest shows");
        // The mask off: everything shows again.
        command(&mut runtime, "layer.mask.toggle");
        let shown =
            sue_paint::composite::flatten(&runtime.engine.world.arena, picture, options).unwrap();
        assert!(alpha_of(&shown, 720) > 200);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_text_tool_writes_and_edits_a_text_layer() {
        let (mut runtime, root) = runtime("paint-text");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
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
        let picture = runtime
            .applets
            .get(&paint)
            .and_then(|applet| applet.focus())
            .unwrap();
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let middle = paint_middle(&runtime);
        let text_layers = |runtime: &Runtime| -> Vec<sue_core::EntityId> {
            let arena = &runtime.engine.world.arena;
            sue_paint::doc::layers(arena, picture)
                .into_iter()
                .filter(|info| {
                    arena
                        .get_by_id(info.id)
                        .is_ok_and(|entity| entity.class() == "sue.paint.text")
                })
                .map(|info| info.id)
                .collect()
        };
        // T, a click, the words, Ctrl+Enter.
        step(&mut runtime, vec![key(egui::Key::T, plain)]);
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(middle), press(middle, true)],
        );
        step(&mut runtime, vec![press(middle, false)]);
        step(&mut runtime, Vec::new());
        step(&mut runtime, vec![egui::Event::Text("Hello".to_owned())]);
        step(
            &mut runtime,
            vec![key(egui::Key::Enter, egui::Modifiers::COMMAND)],
        );
        step(&mut runtime, Vec::new());
        let layers = text_layers(&runtime);
        assert_eq!(layers.len(), 1, "a text layer");
        let content = |runtime: &Runtime| {
            runtime
                .engine
                .world
                .arena
                .get_by_id(layers[0])
                .ok()
                .and_then(|entity| {
                    entity
                        .property("text.content")
                        .and_then(sue_core::Value::as_str)
                        .map(str::to_owned)
                })
        };
        assert_eq!(content(&runtime).as_deref(), Some("Hello"));
        let inked = sue_paint::doc::raster(&runtime.engine.world.arena, layers[0])
            .unwrap()
            .bytes()
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 128)
            .count();
        assert!(inked > 200, "the words are drawn: {inked}");
        // A click on the words edits them in the same layer.
        let depth = runtime.engine.history.depth();
        let on = middle + vec2(10.0, 8.0);
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(on), press(on, true)],
        );
        step(&mut runtime, vec![press(on, false)]);
        step(&mut runtime, Vec::new());
        step(
            &mut runtime,
            vec![
                key(egui::Key::End, plain),
                egui::Event::Text(" Sue".to_owned()),
            ],
        );
        step(
            &mut runtime,
            vec![key(egui::Key::Enter, egui::Modifiers::COMMAND)],
        );
        step(&mut runtime, Vec::new());
        assert_eq!(text_layers(&runtime), layers, "the same layer");
        assert_eq!(content(&runtime).as_deref(), Some("Hello Sue"));
        assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_liquify_pushes_a_stroke_in_one_undo_step() {
        let (mut runtime, root) = runtime("paint-liquify");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
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
        let picture = runtime
            .applets
            .get(&paint)
            .and_then(|applet| applet.focus())
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
        let middle = paint_middle(&runtime);
        let mut drag = |runtime: &mut Runtime, before: Vec<egui::Event>, path: &[Pos2]| {
            step(runtime, before);
            step(
                runtime,
                vec![egui::Event::PointerMoved(path[0]), press(path[0], true)],
            );
            step(
                runtime,
                path[1..]
                    .iter()
                    .map(|at| egui::Event::PointerMoved(*at))
                    .collect(),
            );
            step(runtime, vec![press(*path.last().unwrap(), false)]);
        };
        drag(
            &mut runtime,
            Vec::new(),
            &[middle - vec2(60.0, 0.0), middle, middle + vec2(60.0, 0.0)],
        );
        assert!(ink(&runtime, 590..610) > 0);
        assert_eq!(ink(&runtime, 640..700), 0);
        let depth = runtime.engine.history.depth();
        // J for Blend, then two sub tools on: Liquify, Push. Push down.
        let path: Vec<Pos2> = (0..12)
            .map(|index| middle + vec2(0.0, -10.0 + index as f32 * 4.0))
            .collect();
        drag(
            &mut runtime,
            vec![
                key(egui::Key::J, plain),
                key(egui::Key::Period, plain),
                key(egui::Key::Period, plain),
            ],
            &path,
        );
        assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");
        assert!(ink(&runtime, 610..700) > 0, "the stroke was pushed down");
        step(
            &mut runtime,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
        assert_eq!(ink(&runtime, 640..700), 0, "undo puts it back");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_correction_layers_change_what_is_below_and_cancel_cleanly() {
        let (mut runtime, root) = runtime("paint-correction");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
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
        let picture = runtime
            .applets
            .get(&paint)
            .and_then(|applet| applet.focus())
            .unwrap();
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
        let shown = |runtime: &Runtime| {
            let options = sue_paint::composite::RenderOptions {
                paper: true,
                drafts: true,
            };
            sue_paint::composite::render(
                &runtime.engine.world.arena,
                picture,
                [0, 0, 1, 1],
                options,
            )[0]
        };
        let layers =
            |runtime: &Runtime| sue_paint::doc::layers(&runtime.engine.world.arena, picture).len();
        assert_eq!(shown(&runtime), 255, "white paper");
        // Reverse Gradient as a correction layer: the paper shows black.
        command(&mut runtime, "layer.correction.invert");
        assert_eq!(shown(&runtime), 0);
        step(
            &mut runtime,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
        assert_eq!(shown(&runtime), 255, "undone");
        // Level Correction opens its settings; Esc takes the layer away.
        let before = layers(&runtime);
        command(&mut runtime, "layer.correction.levels");
        assert_eq!(layers(&runtime), before + 1);
        step(&mut runtime, Vec::new());
        step(&mut runtime, vec![key(egui::Key::Escape, plain)]);
        step(&mut runtime, Vec::new());
        assert_eq!(layers(&runtime), before, "cancelled, gone");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Frame times of a huge brush on a big picture through the whole app
    /// (`cargo test --release -p sue paint_frame_speed -- --ignored
    /// --nocapture`).
    #[test]
    #[ignore = "a benchmark"]
    fn paint_frame_speed() {
        let (mut runtime, root) = runtime("paint-speed");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        let paint = AppletId::new(sue_paint::ID).unwrap();
        for (key, value) in [("sue_paint.width", 4000), ("sue_paint.height", 3000)] {
            let _ = runtime
                .engine
                .world
                .rice
                .set(key, sue_rice::RiceValue::Int(value));
        }
        runtime.request(ShellRequest::Open(paint.clone()), &mut NoTextures, now);
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
            let begun = Instant::now();
            frame_with(runtime, &ctx, now, events);
            now += Duration::from_millis(8);
            begun.elapsed().as_secs_f64() * 1000.0
        };
        step(
            &mut runtime,
            vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
        );
        step(
            &mut runtime,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        for _ in 0..6 {
            step(
                &mut runtime,
                vec![key(
                    egui::Key::N,
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                )],
            );
        }
        // ] makes the brush bigger: from 8 pixels to the largest.
        for _ in 0..40 {
            step(
                &mut runtime,
                vec![key(egui::Key::CloseBracket, egui::Modifiers::NONE)],
            );
        }
        for _ in 0..5 {
            step(&mut runtime, Vec::new());
        }
        let middle = paint_middle(&runtime);
        let from = middle - vec2(250.0, 0.0);
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut times = Vec::new();
        times.push(step(
            &mut runtime,
            vec![egui::Event::PointerMoved(from), press(from, true)],
        ));
        let mut at = from;
        for _ in 0..120 {
            let a = at + vec2(2.0, 0.3);
            let b = a + vec2(2.0, -0.3);
            at = b;
            times.push(step(
                &mut runtime,
                vec![egui::Event::PointerMoved(a), egui::Event::PointerMoved(b)],
            ));
        }
        let lift = step(&mut runtime, vec![press(at, false)]);
        let after = step(&mut runtime, Vec::new());
        let mut slowest: Vec<(usize, f64)> = times.iter().copied().enumerate().collect();
        slowest.sort_by(|a, b| b.1.total_cmp(&a.1));
        println!("slowest frames: {:?}", &slowest[..5]);
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        let worst = times.iter().copied().fold(0.0, f64::max);
        println!(
            "4000x3000, 8 layers, brush huge: frame mean {mean:.2} ms, worst {worst:.2} ms, lift {lift:.2} ms, next {after:.2} ms"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_a_light_pen_stroke_ends_light() {
        let (mut runtime, root) = runtime("paint-light-end");
        let ctx = egui::Context::default();
        let mut now = Instant::now();
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
            &mut NoTextures,
            now,
        );
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
            frame_with(runtime, &ctx, now, events);
            now += Duration::from_millis(16);
        };
        step(
            &mut runtime,
            vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
        );
        step(
            &mut runtime,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        // A bigger G-pen, so a full-pressure blob would show.
        for _ in 0..8 {
            step(
                &mut runtime,
                vec![key(egui::Key::CloseBracket, egui::Modifiers::NONE)],
            );
        }
        let touch = |phase, pos| egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(1),
            phase,
            pos,
            force: Some(0.15),
        };
        let start = paint_middle(&runtime) - vec2(120.0, 0.0);
        step(
            &mut runtime,
            vec![
                touch(egui::TouchPhase::Start, start),
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let mut at = start;
        for _ in 0..30 {
            at += vec2(8.0, 0.0);
            step(
                &mut runtime,
                vec![
                    touch(egui::TouchPhase::Move, at),
                    egui::Event::PointerMoved(at),
                ],
            );
        }
        // A real pen lifts a little away from its last move.
        let lift = at + vec2(3.0, 0.0);
        step(
            &mut runtime,
            vec![
                touch(egui::TouchPhase::End, lift),
                egui::Event::PointerButton {
                    pos: lift,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        step(&mut runtime, Vec::new());
        let arena = &runtime.engine.world.arena;
        let picture = arena
            .iter()
            .find(|(_, entity)| sue_model::picture::is_picture(entity))
            .map(|(_, entity)| entity.id())
            .unwrap();
        let layer = sue_paint::doc::layers(arena, picture).last().unwrap().id;
        let raster = sue_paint::doc::raster(arena, layer).unwrap();
        let bytes = raster.bytes().unwrap();
        let width = raster.width() as usize;
        // The painted columns, and each one's thickness.
        let thick = |x: usize| {
            (0..raster.height() as usize)
                .filter(|y| bytes[(y * width + x) * 4 + 3] > 128)
                .count()
        };
        let columns: Vec<usize> = (0..width).filter(|x| thick(*x) > 0).collect();
        assert!(columns.len() > 20, "the stroke was painted");
        let middle = thick(columns[columns.len() / 2]);
        let end = columns[columns.len().saturating_sub(12)..]
            .iter()
            .map(|x| thick(*x))
            .max()
            .unwrap();
        assert!(
            end <= middle + 1,
            "the end ({end} px) is no blob over the middle ({middle} px)"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_pinching_takes_back_the_touch_and_undoes_nothing() {
        let (mut runtime, root) = runtime("paint-pinch");
        let ctx = egui::Context::default();
        let mut now = Instant::now();
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
            &mut NoTextures,
            now,
        );
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
            frame_with(runtime, &ctx, now, events);
            now += Duration::from_millis(16);
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
        let pictures = |runtime: &Runtime| {
            runtime
                .engine
                .world
                .arena
                .iter()
                .filter(|(_, entity)| sue_model::picture::is_picture(entity))
                .count()
        };
        let touch = |id: u64, phase, pos| egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(id),
            phase,
            pos,
            force: None,
        };
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let middle = paint_middle(&runtime);
        // A real stroke with one finger first.
        let a = middle - vec2(150.0, 80.0);
        step(
            &mut runtime,
            vec![
                touch(1, egui::TouchPhase::Start, a),
                egui::Event::PointerMoved(a),
                press(a, true),
            ],
        );
        step(
            &mut runtime,
            vec![
                touch(1, egui::TouchPhase::Move, a + vec2(60.0, 0.0)),
                egui::Event::PointerMoved(a + vec2(60.0, 0.0)),
            ],
        );
        step(
            &mut runtime,
            vec![
                touch(1, egui::TouchPhase::End, a + vec2(60.0, 0.0)),
                press(a + vec2(60.0, 0.0), false),
            ],
        );
        let depth = runtime.engine.history.depth();
        let open = pictures(&runtime);
        let layer_bytes = |runtime: &Runtime| {
            let arena = &runtime.engine.world.arena;
            let picture = arena
                .iter()
                .find(|(_, entity)| sue_model::picture::is_picture(entity))
                .map(|(_, entity)| entity.id())
                .unwrap();
            let layer = sue_paint::doc::layers(arena, picture).last().unwrap().id;
            sue_paint::doc::raster(arena, layer)
                .unwrap()
                .bytes()
                .unwrap()
                .to_vec()
        };
        let before = layer_bytes(&runtime);
        // Now a pinch: one finger lands (and starts to paint), a second
        // follows, both spread apart and lift.
        let (one, two) = (middle, middle + vec2(80.0, 40.0));
        step(
            &mut runtime,
            vec![
                touch(1, egui::TouchPhase::Start, one),
                egui::Event::PointerMoved(one),
                press(one, true),
            ],
        );
        step(&mut runtime, vec![touch(2, egui::TouchPhase::Start, two)]);
        for spread in 1..=6 {
            let d = vec2(12.0, 6.0) * spread as f32;
            step(
                &mut runtime,
                vec![
                    touch(1, egui::TouchPhase::Move, one - d),
                    touch(2, egui::TouchPhase::Move, two + d),
                    egui::Event::PointerMoved(one - d),
                ],
            );
        }
        step(
            &mut runtime,
            vec![
                touch(2, egui::TouchPhase::End, two + vec2(72.0, 36.0)),
                touch(1, egui::TouchPhase::End, one - vec2(72.0, 36.0)),
                press(one - vec2(72.0, 36.0), false),
            ],
        );
        step(&mut runtime, Vec::new());
        assert_eq!(
            runtime.engine.history.depth(),
            depth,
            "nothing undone, nothing added"
        );
        assert_eq!(pictures(&runtime), open, "the picture stays open");
        assert_eq!(
            layer_bytes(&runtime),
            before,
            "the pinch left no paint, the stroke stays"
        );
        // Both fingers landing in one frame: no stroke starts either.
        step(
            &mut runtime,
            vec![
                touch(3, egui::TouchPhase::Start, one),
                touch(4, egui::TouchPhase::Start, two),
                egui::Event::PointerMoved(one),
                press(one, true),
            ],
        );
        step(
            &mut runtime,
            vec![
                touch(3, egui::TouchPhase::Move, one - vec2(50.0, 0.0)),
                touch(4, egui::TouchPhase::Move, two + vec2(50.0, 0.0)),
                egui::Event::PointerMoved(one - vec2(50.0, 0.0)),
            ],
        );
        step(
            &mut runtime,
            vec![
                touch(3, egui::TouchPhase::End, one - vec2(50.0, 0.0)),
                touch(4, egui::TouchPhase::End, two + vec2(50.0, 0.0)),
                press(one - vec2(50.0, 0.0), false),
            ],
        );
        step(&mut runtime, Vec::new());
        assert_eq!(runtime.engine.history.depth(), depth);
        assert_eq!(layer_bytes(&runtime), before, "no stroke during a pinch");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_saves_as_a_project_or_any_picture_type() {
        let (mut runtime, root) = runtime("paint-save-as");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut now = start;
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
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
        let pick = FilePick::save(Some(paint.clone()), "save-as", "Save As", "Picture.sue");
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

        // Clip Studio's type writes a .clip with the picture's layers and
        // Sue's own copy of it, and Save writes that file again from then on.
        let clip = root.join("Picture.clip");
        runtime.request(
            ShellRequest::Picked {
                pick: FilePick::save(Some(paint.clone()), "save-as", "Save As", "Picture.clip"),
                path: clip.clone(),
            },
            &mut NoTextures,
            start,
        );
        step(&mut runtime, Vec::new());
        let read = sue_io::clip::decode_clip(&std::fs::read(&clip).unwrap()).unwrap();
        assert_eq!((read.psd.width, read.psd.height), (1600, 1200));
        assert!(read.sue.is_some(), "Sue's copy rides along");
        std::fs::remove_file(&clip).unwrap();
        step(
            &mut runtime,
            vec![key(egui::Key::S, egui::Modifiers::COMMAND)],
        );
        step(&mut runtime, Vec::new());
        assert!(clip.exists(), "Save wrote the .clip again");

        // A .clip made elsewhere opens as a new canvas with its layers.
        let mut layer = sue_io::psd::PsdLayer::pixels(
            "Ink",
            10,
            20,
            30,
            40,
            [12_u8, 34, 56, 255].repeat(30 * 40),
        );
        layer.opacity = 128;
        let foreign = root.join("Elsewhere.clip");
        let psd = sue_io::psd::Psd {
            width: 200,
            height: 100,
            layers: vec![layer],
            merged: Vec::new(),
        };
        std::fs::write(
            &foreign,
            sue_io::clip::encode_clip(&psd, 350.0, None).unwrap(),
        )
        .unwrap();
        let pictures = |runtime: &Runtime| {
            runtime
                .engine
                .world
                .arena
                .iter()
                .filter(|(_, entity)| sue_model::picture::is_picture(entity))
                .count()
        };
        let before = pictures(&runtime);
        runtime.request(
            ShellRequest::Picked {
                pick: FilePick::open(Some(paint), "open", "Open"),
                path: foreign,
            },
            &mut NoTextures,
            start,
        );
        step(&mut runtime, Vec::new());
        assert_eq!(
            pictures(&runtime),
            before + 1,
            "the .clip opened as a canvas"
        );
        let arena = &runtime.engine.world.arena;
        let opened = arena
            .iter()
            .find(|(_, entity)| entity.name() == "Elsewhere")
            .map(|(_, entity)| entity.id())
            .expect("named after its file");
        let layers = sue_model::picture::layers(arena, opened);
        assert!(
            layers
                .iter()
                .filter_map(|id| arena.get_by_id(*id).ok())
                .any(|entity| entity.name() == "Ink"),
            "its layer came along"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pages_slide_in_over_the_hub_and_back_out() {
        let (mut runtime, root) = runtime("page-motion");
        let ctx = egui::Context::default();
        let size = vec2(1440.0, 900.0);
        let start = Instant::now();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        assert!(runtime.shell.hub_open());
        let paint = AppletId::new(sue_paint::ID).unwrap();
        // New, Picture, Square Post: Sue Paint makes it and opens on it,
        // coming in over the Hub.
        runtime.request(
            ShellRequest::Create {
                applet: paint.clone(),
                key: "Square Post".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        assert_eq!(runtime.shell.page(), Some(&paint));
        assert!(runtime.shell.reveal() < 0.01, "it starts on the Hub");
        let opening = frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        assert!(opening > 10, "coming in takes a moment ({opening} frames)");
        assert!(runtime.shell.reveal() > 0.999);
        assert!(
            runtime.shell.hub_boxes().is_empty(),
            "the Hub is not drawn under a page in place"
        );
        assert!(
            runtime
                .engine
                .world
                .arena
                .iter()
                .any(|(_, entity)| sue_model::picture::is_picture(entity)),
            "the picture was made"
        );

        runtime.request(ShellRequest::Hub, &mut NoTextures, start);
        assert!(runtime.shell.hub_open(), "leaving starts at once");
        let closing = frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        assert!(closing > 10, "going out takes a moment ({closing})");
        assert!(runtime.shell.reveal() < 0.001);
        assert_eq!(
            runtime.shell.hub_boxes().len(),
            4,
            "the Hub shows every studio and tool again"
        );

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
        runtime.request(ShellRequest::Open(paint), &mut NoTextures, start);
        for frame in 0..2 {
            frame_with(
                &mut runtime,
                &ctx,
                start + Duration::from_millis(8 * frame),
                Vec::new(),
            );
        }
        assert!(
            runtime.shell.reveal() > 0.999,
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
    fn files_open_through_the_system_dialog_in_the_studio_or_tool_that_takes_them() {
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

        // Sue Paint asks; the system's dialog answers (headless, the
        // script does).
        let paint = AppletId::new(sue_paint::ID).unwrap();
        let pick = FilePick::open(Some(paint.clone()), "open", "Open Picture").extensions(&["png"]);
        runtime.request(ShellRequest::PickFile(pick.clone()), &mut NoTextures, start);
        assert_eq!(runtime.pending_pick(), Some(&pick), "the dialog was asked");
        assert_eq!(runtime.shell.window(), None, "no window of Sue's own");
        let before = pictures(&runtime);
        assert!(runtime.answer_pick(&picture, &mut NoTextures, start));
        assert_eq!(pictures(&runtime), before + 1, "Sue Paint opened it");
        assert_eq!(runtime.shell.page(), Some(&paint));
        assert!(!runtime.answer_pick(&picture, &mut NoTextures, start));

        // Open File (Ctrl+O on the Hub) puts a picture in the Image Viewer.
        runtime.request(ShellRequest::Hub, &mut NoTextures, start);
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |frame| {
            if frame == 1 {
                vec![key(egui::Key::O, egui::Modifiers::COMMAND)]
            } else {
                Vec::new()
            }
        });
        let asked = runtime.pending_pick().cloned().expect("Ctrl+O asked");
        assert_eq!((asked.requester, asked.title.as_str()), (None, "Open File"));
        assert!(runtime.answer_pick(&picture, &mut NoTextures, start));
        let viewer = AppletId::new(crate::viewer::ID).unwrap();
        assert_eq!(runtime.shell.page(), Some(&viewer));
        let shown = runtime
            .applets
            .get(&viewer)
            .and_then(|applet| applet.focus());
        assert_eq!(
            shown
                .and_then(|id| runtime.engine.world.arena.get_by_id(id).ok())
                .and_then(|entity| entity.meta(sue_io::meta::SOURCE)),
            Some(picture.display().to_string().as_str()),
            "the Image Viewer shows the file"
        );

        // Text goes to Sue Writer.
        let notes = root.join("notes.md");
        std::fs::write(&notes, "# Hello\n\nSome *words*.\n").unwrap();
        runtime.open_path(&notes, &mut NoTextures, start);
        assert_eq!(
            runtime.shell.page().map(AppletId::as_str),
            Some(sue_writer::ID)
        );

        // Nothing opens an unknown file, and it does not stay behind.
        let data = root.join("data.bin");
        std::fs::write(&data, [0_u8, 1, 2, 3, 255, 254, 0, 7].repeat(64)).unwrap();
        let entities = runtime.engine.world.arena.len();
        runtime.open_path(&data, &mut NoTextures, start);
        assert_eq!(runtime.engine.world.arena.len(), entities);
        let (message, error) = runtime.shell.message().unwrap();
        assert!(error && message.contains("data.bin"), "{message}");

        // A folder is asked for as a folder.
        let convert = AppletId::new(crate::convert::ID).unwrap();
        runtime.request(
            ShellRequest::AppletCommand {
                applet: convert.clone(),
                command: "convert.folder".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        assert!(runtime.pending_pick().is_some_and(|pick| pick.folders));
        assert_eq!(runtime.shell.page(), Some(&convert));
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
        // A tool, with a page of its own like a studio's.
        assert_eq!(
            runtime
                .applets
                .manifest(&convert)
                .map(|manifest| manifest.role),
            Some(sue_wm::AppletRole::Tool)
        );
        runtime.request(ShellRequest::Open(convert.clone()), &mut NoTextures, now);
        assert_eq!(runtime.shell.page(), Some(&convert));
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
    fn studios_keep_the_keys_they_reserve() {
        let (mut runtime, root) = runtime("reserved-keys");
        let ctx = egui::Context::default();
        let start = Instant::now();
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |_| {
            Vec::new()
        });
        // On the Hub, Tab opens the command palette.
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
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
            &mut NoTextures,
            start,
        );
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |_| {
            Vec::new()
        });
        assert!(runtime.shell.reveal() > 0.999);
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

        // The Sue button sits in the window's top left corner; a click
        // opens the Sue menu.
        let button = runtime.shell.sue_button();
        assert_eq!(button.min, egui::Pos2::ZERO);
        frames(&mut runtime, &ctx, size, start, |frame| {
            let at = button.center();
            match frame {
                0 => vec![egui::Event::PointerMoved(at)],
                1 => vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
                2 => vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                _ => Vec::new(),
            }
        });
        assert!(runtime.shell.sue_menu_open());
        let first = runtime.shell.sue_menu_labels(&[]);
        for wanted in [
            "Hub",
            "Studios",
            "Tools",
            "New",
            "File",
            "Project",
            "Command Palette (Tab)",
            "Settings",
            "Quit Sue",
        ] {
            assert!(
                first.iter().any(|label| label == wanted),
                "{wanted} in {first:?}"
            );
        }
        assert!(
            !first.iter().any(|label| label.ends_with("at Launch")),
            "the Hub has nothing to pin"
        );
        assert_eq!(
            runtime.shell.sue_menu_labels(&["Studios"]),
            ["Sue Paint", "Sue Writer"]
        );
        assert_eq!(
            runtime.shell.sue_menu_labels(&["Tools"]),
            ["Image Viewer", "Converter"]
        );
        assert_eq!(
            runtime.shell.sue_menu_labels(&["New"]),
            ["Picture", "Writing"]
        );
        assert!(
            runtime
                .shell
                .sue_menu_labels(&["File"])
                .iter()
                .any(|label| label == "Open File...")
        );

        // New, Picture, Square Post: into Sue Paint on a new picture.
        let picked = runtime.with_shell(&mut NoTextures, start, |shell, io, now| {
            shell.sue_menu_pick(&["New", "Picture", "Square Post"], io, now)
        });
        assert!(picked);
        assert!(!runtime.shell.sue_menu_open(), "a pick closes the menu");
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        let paint = AppletId::new(sue_paint::ID).unwrap();
        assert_eq!(runtime.shell.page(), Some(&paint));
        let picture = runtime
            .engine
            .world
            .arena
            .iter()
            .find(|(_, entity)| sue_model::picture::is_picture(entity))
            .map(|(_, entity)| entity.id())
            .unwrap();

        // In Sue Paint: Send To offers the others, and the pin.
        runtime.request(ShellRequest::SueMenu, &mut NoTextures, start);
        let send = runtime.shell.sue_menu_labels(&["Send To"]);
        assert!(
            send.iter().any(|label| label == "Converter")
                && send.iter().any(|label| label == "Sue Writer")
                && !send.iter().any(|label| label == "Sue Paint"),
            "{send:?}"
        );
        assert!(
            runtime
                .shell
                .sue_menu_labels(&[])
                .iter()
                .any(|label| label == "Open Sue Paint at Launch")
        );

        // The picture goes to the Converter, flattened into a PNG.
        let sent = runtime.with_shell(&mut NoTextures, start, |shell, io, now| {
            shell.sue_menu_pick(&["Send To", "Converter"], io, now)
        });
        assert!(sent);
        let convert = AppletId::new(crate::convert::ID).unwrap();
        assert_eq!(runtime.shell.page(), Some(&convert));
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

        // Open In goes back to Sue Paint on the very same picture; the Hub
        // item goes home.
        runtime.request(
            ShellRequest::OpenIn {
                entity: picture,
                applet: paint.clone(),
            },
            &mut NoTextures,
            start,
        );
        assert_eq!(runtime.shell.page(), Some(&paint));
        assert_eq!(
            runtime
                .applets
                .get(&paint)
                .and_then(|applet| applet.focus()),
            Some(picture)
        );
        runtime.request(ShellRequest::SueMenu, &mut NoTextures, start);
        let home_picked = runtime.with_shell(&mut NoTextures, start, |shell, io, now| {
            shell.sue_menu_pick(&["Hub"], io, now)
        });
        assert!(home_picked && runtime.shell.hub_open());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A click at `at` over three frames, for [`frames`].
    fn click_at(at: egui::Pos2) -> impl FnMut(usize) -> Vec<egui::Event> {
        move |frame: usize| match frame {
            0 => vec![egui::Event::PointerMoved(at)],
            1 => vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            2 => vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            _ => Vec::new(),
        }
    }

    /// Where the Hub drew the row (or button) that does `row`.
    fn hub_row(runtime: &Runtime, row: &sue_wm::HubRow) -> Option<egui::Rect> {
        runtime
            .shell
            .hub_rows()
            .iter()
            .filter(|(what, _)| what == row)
            .map(|(_, rect)| *rect)
            .max_by(|a, b| a.width().total_cmp(&b.width()))
    }

    #[test]
    fn the_hub_lists_recent_files_saved_projects_and_unsaved_work() {
        use sue_wm::HubRow;
        let (host, root) = temp_host("hub-lists");
        let ctx = egui::Context::default();
        let size = vec2(1440.0, 900.0);
        let start = Instant::now();
        let boot = || {
            Runtime::new(Engine::boot(&host).unwrap())
                .unwrap()
                .remembering(true)
        };
        let paint = AppletId::new(sue_paint::ID).unwrap();
        let pictures = |runtime: &Runtime| {
            runtime
                .engine
                .world
                .arena
                .iter()
                .filter(|(_, entity)| sue_model::picture::is_picture(entity))
                .count()
        };
        let mut runtime = boot();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        assert!(runtime.shell.hub_rows().is_empty(), "nothing yet");

        // A picture opened from a file goes first among the recent files.
        std::fs::create_dir_all(&root).unwrap();
        let photo = root.join("photo.png");
        let pixels = vec![200_u8; 4 * 4 * 4];
        std::fs::write(&photo, sue_io::encode_png_rgba(4, 4, &pixels).unwrap()).unwrap();
        runtime.request(
            ShellRequest::OpenFile(photo.clone()),
            &mut NoTextures,
            start,
        );
        assert_eq!(
            runtime.work().recent().next().map(|file| &file.path),
            Some(&photo)
        );

        // A picture made in Sue Paint and saved as a project is a saved
        // project too.
        runtime.request(
            ShellRequest::Create {
                applet: paint.clone(),
                key: "Square Post".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        let album = root.join("album.sue");
        runtime.save_project_to(&album).unwrap();
        let first: Vec<&Path> = runtime
            .work()
            .recent()
            .map(|file| file.path.as_path())
            .collect();
        assert_eq!(first, [album.as_path(), photo.as_path()]);
        assert_eq!(runtime.work().projects().len(), 1);
        assert_eq!(runtime.work().projects()[0].page.as_ref(), Some(&paint));
        // The Sue menu opens them too.
        runtime.request(ShellRequest::SueMenu, &mut NoTextures, start);
        assert_eq!(
            runtime.shell.sue_menu_labels(&["File", "Open Recent"]),
            ["album.sue", "photo.png"]
        );
        runtime.shell.dismiss_overlays();

        // A second picture, never saved, is kept at quit.
        runtime.request(
            ShellRequest::Create {
                applet: paint.clone(),
                key: "Square Post".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        assert_eq!(pictures(&runtime), 2);
        runtime.shutdown();

        // The next start lists all three on the Hub.
        let mut runtime = boot();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        let kept = runtime.work().unsaved().next().cloned().unwrap();
        assert_eq!(kept.project.as_deref(), Some(album.as_path()));
        assert_eq!(kept.title(), "album");
        for row in [
            HubRow::Open(photo.clone()),
            HubRow::Open(album.clone()),
            HubRow::Recover(kept.id.clone()),
            HubRow::Discard(kept.id.clone()),
        ] {
            assert!(hub_row(&runtime, &row).is_some(), "{row:?} is on the Hub");
        }

        // A click on it brings it back in Sue Paint, both pictures, and the
        // project it belongs to.
        let at = hub_row(&runtime, &HubRow::Recover(kept.id.clone()))
            .unwrap()
            .left_center()
            + vec2(40.0, 0.0);
        frames(&mut runtime, &ctx, size, start, click_at(at));
        assert_eq!(runtime.shell.page(), Some(&paint));
        assert_eq!(pictures(&runtime), 2);
        assert_eq!(runtime.project(), Some(album.as_path()));
        assert_eq!(runtime.work().unsaved().count(), 0, "it is this run's now");
        assert!(kept.file.exists(), "kept until it is saved");
        runtime.save_project_to(&album).unwrap();
        assert!(!kept.file.exists(), "saved, it is let go");

        // Opening another project keeps the unsaved work it replaces.
        runtime.request(
            ShellRequest::Create {
                applet: paint.clone(),
                key: "Square Post".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        assert_eq!(pictures(&runtime), 3);
        runtime.request(
            ShellRequest::OpenFile(album.clone()),
            &mut NoTextures,
            start,
        );
        assert_eq!(pictures(&runtime), 2, "the saved project");
        assert_eq!(
            runtime.shell.page(),
            Some(&paint),
            "in the studio it was saved from"
        );
        let replaced = runtime.work().unsaved().next().cloned().unwrap();

        // Discard asks once more, then throws it away.
        runtime.request(ShellRequest::Hub, &mut NoTextures, start);
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        let discard = HubRow::Discard(replaced.id.clone());
        let button = hub_row(&runtime, &discard).unwrap().center();
        frames(&mut runtime, &ctx, size, start, click_at(button));
        assert!(replaced.file.exists(), "a first click only asks");
        let button = hub_row(&runtime, &discard).unwrap().center();
        frames(&mut runtime, &ctx, size, start, click_at(button));
        assert!(!replaced.file.exists());
        assert_eq!(runtime.work().unsaved().count(), 0);
        assert!(hub_row(&runtime, &discard).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn runs_that_do_not_remember_leave_no_trace() {
        let (mut runtime, root) = runtime("no-trace");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("story.sue");
        runtime.request(
            ShellRequest::Create {
                applet: AppletId::new(sue_paint::ID).unwrap(),
                key: "Square Post".to_owned(),
            },
            &mut NoTextures,
            Instant::now(),
        );
        runtime.save_project_to(&path).unwrap();
        runtime.shutdown();
        assert!(!runtime.engine.paths.data_dir.join("work").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_pinned_studio_opens_at_launch_and_the_pin_undoes() {
        let (host, root) = temp_host("pin");
        let ctx = egui::Context::default();
        let size = vec2(1440.0, 900.0);
        let start = Instant::now();
        let mut runtime = Runtime::new(Engine::boot(&host).unwrap()).unwrap();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        assert!(runtime.shell.hub_open(), "nothing pinned: the Hub");
        let pinned = |runtime: &Runtime| {
            runtime
                .engine
                .world
                .rice
                .get_str(sue_wm::PIN_KEY)
                .unwrap_or_default()
                .to_owned()
        };
        // A click on Sue Writer's pin, at the bottom right of its box.
        let (_, rect, pin) = runtime
            .shell
            .hub_boxes()
            .iter()
            .find(|(id, _, _)| id.as_str() == sue_writer::ID)
            .cloned()
            .unwrap();
        assert!(
            (pin.right() - rect.right()).abs() < 12.0
                && (pin.bottom() - rect.bottom()).abs() < 12.0,
            "the pin sits at the box's bottom right"
        );
        let click = |at: egui::Pos2| {
            move |frame: usize| match frame {
                0 => vec![egui::Event::PointerMoved(at)],
                1 => vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
                2 => vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                _ => Vec::new(),
            }
        };
        frames(&mut runtime, &ctx, size, start, click(pin.center()));
        assert_eq!(pinned(&runtime), sue_writer::ID);
        assert!(runtime.shell.hub_open(), "pinning does not open it");
        // One undo step, like any other change.
        frames(&mut runtime, &ctx, size, start, |frame| {
            if frame == 1 {
                vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]
            } else {
                Vec::new()
            }
        });
        assert_eq!(pinned(&runtime), "");
        frames(&mut runtime, &ctx, size, start, |frame| {
            if frame == 1 {
                vec![key(egui::Key::Y, egui::Modifiers::COMMAND)]
            } else {
                Vec::new()
            }
        });
        assert_eq!(pinned(&runtime), sue_writer::ID);
        // One pin at a time: Sue Paint's pin takes it over.
        let paint_pin = runtime
            .shell
            .hub_boxes()
            .iter()
            .find(|(id, _, _)| id.as_str() == sue_paint::ID)
            .map(|(_, _, pin)| pin.center())
            .unwrap();
        frames(&mut runtime, &ctx, size, start, click(paint_pin));
        assert_eq!(pinned(&runtime), sue_paint::ID);
        frames(&mut runtime, &ctx, size, start, click(pin.center()));
        assert_eq!(pinned(&runtime), sue_writer::ID);
        // The rice file keeps it.
        let rice = std::fs::read_to_string(runtime.engine.rice_path()).unwrap();
        assert!(rice.contains("pinned = \"sue-writer\""), "{rice}");

        // Started again, Sue opens straight into Sue Writer, already in place.
        let mut again = Runtime::new(Engine::boot(&host).unwrap()).unwrap();
        frame_with(&mut again, &ctx, start, Vec::new());
        assert_eq!(
            again.shell.page().map(AppletId::as_str),
            Some(sue_writer::ID)
        );
        assert!(again.shell.reveal() > 0.999, "no trip through the Hub");
        // Its Sue menu takes the pin away; the next start is on the Hub.
        again.request(ShellRequest::SueMenu, &mut NoTextures, start);
        let unpinned = again.with_shell(&mut NoTextures, start, |shell, io, now| {
            shell.sue_menu_pick(&["Open Sue Writer at Launch"], io, now)
        });
        assert!(unpinned);
        frames(&mut again, &ctx, size, start, |_| Vec::new());
        let third = Runtime::new(Engine::boot(&host).unwrap()).unwrap();
        let mut third = third;
        frame_with(&mut third, &ctx, start, Vec::new());
        assert!(third.shell.hub_open());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_writers_caret_glides_after_typing_then_asks_for_no_frames() {
        let (mut runtime, root) = runtime("smooth-caret");
        let ctx = egui::Context::default();
        let size = vec2(1440.0, 900.0);
        let start = Instant::now();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        runtime.request(
            ShellRequest::Create {
                applet: AppletId::new(sue_writer::ID).unwrap(),
                key: "novel".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        // One frame `dt` seconds after the last; how soon egui wants the next.
        let mut now = start;
        let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>, dt: f64| {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
                events,
                time: Some(ctx.input(|input| input.time) + dt),
                ..egui::RawInput::default()
            };
            now += Duration::from_secs_f64(dt);
            let output = ctx.run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
            let delay = output
                .viewport_output
                .values()
                .next()
                .map_or(Duration::MAX, |viewport| viewport.repaint_delay);
            output.drop_without_applying_deltas();
            delay
        };
        // Let the studio come in and egui's own fades finish.
        for _ in 0..300 {
            step(&mut runtime, Vec::new(), 0.008);
        }
        let typed = |text: &str| vec![egui::Event::Text(text.to_owned())];

        // A word typed: egui answers any input with a frame at once; the
        // gliding caret keeps them coming for its 0.1 s, and no longer.
        assert_eq!(step(&mut runtime, typed("Once"), 0.008), Duration::ZERO);
        let mut gliding = 0;
        while step(&mut runtime, Vec::new(), 0.008) == Duration::ZERO {
            gliding += 1;
            assert!(gliding < 100, "the caret never lands");
        }
        assert!((9..=15).contains(&gliding), "{gliding} frames");
        // At rest only its blink wakes egui, later.
        let blink = step(&mut runtime, Vec::new(), 0.008);
        assert!(
            blink > Duration::from_millis(200) && blink <= Duration::from_millis(530),
            "{blink:?}"
        );
        // After a while it rests lit, and nothing asks for frames.
        step(&mut runtime, Vec::new(), sue_type::caret::BLINK_FOR + 1.0);
        let idle = step(&mut runtime, Vec::new(), 0.008);
        assert!(
            idle > Duration::from_secs(60),
            "egui asks for frames every {idle:?} ({:?})",
            ctx.repaint_causes()
        );

        // Smooth Caret off in Settings: the caret jumps, no frames to glide.
        runtime
            .engine
            .world
            .rice
            .set(sue_wm::SMOOTH_CARET_KEY, sue_rice::RiceValue::Bool(false))
            .unwrap();
        step(&mut runtime, Vec::new(), 0.008);
        assert_eq!(step(&mut runtime, typed(" upon"), 0.008), Duration::ZERO);
        // (egui still draws a frame or two as the words count goes up.)
        let mut jumped = 0;
        while step(&mut runtime, Vec::new(), 0.008) == Duration::ZERO {
            jumped += 1;
            assert!(jumped < 100);
        }
        assert!(
            jumped <= 2 && gliding >= jumped + 8,
            "{jumped} and {gliding}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_writer_makes_a_novel_types_undoes_takes_a_picture_and_compiles() {
        let (mut runtime, root) = runtime("sue-writer");
        let ctx = egui::Context::default();
        let size = vec2(1440.0, 900.0);
        let start = Instant::now();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        let writer = AppletId::new(sue_writer::ID).unwrap();
        // A novel from the Sue menu's New: made and opened.
        runtime.request(
            ShellRequest::Create {
                applet: writer.clone(),
                key: "novel".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        assert_eq!(runtime.shell.page(), Some(&writer));
        let arena = &runtime.engine.world.arena;
        let novel = arena
            .iter()
            .find(|(_, entity)| sue_model::manuscript::is_manuscript(entity))
            .map(|(_, entity)| entity.id())
            .expect("a manuscript");
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

        // Sue Writer's menus sit in the top bar after the Sue button; the
        // Sue menu offers the pin.
        runtime.request(ShellRequest::SueMenu, &mut NoTextures, start);
        let first = runtime.shell.sue_menu_labels(&[]);
        assert!(
            first
                .iter()
                .any(|label| label == "Open Sue Writer at Launch"),
            "{first:?}"
        );
        runtime.shell.dismiss_overlays();

        // Compile to Markdown and to PDF, as the system's dialog answers.
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
        assert_eq!(runtime.shell.page(), Some(&writer));
        let _ = std::fs::remove_dir_all(&root);
    }
    #[test]
    fn sue_writer_changes_case_keeping_formatting_and_undoes_it() {
        let (mut runtime, root) = runtime("sue-writer-case");
        let ctx = egui::Context::default();
        let size = vec2(1440.0, 900.0);
        let start = Instant::now();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        let writer = AppletId::new(sue_writer::ID).unwrap();
        runtime.request(
            ShellRequest::Create {
                applet: writer.clone(),
                key: "blank".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        frames(&mut runtime, &ctx, size, start, |frame| {
            if frame == 1 {
                vec![egui::Event::Text("the storm came".to_owned())]
            } else {
                Vec::new()
            }
        });
        runtime.typing.flush(&mut runtime.engine.history);
        let text = |runtime: &Runtime| {
            runtime
                .engine
                .world
                .arena
                .iter()
                .find_map(|(_, entity)| {
                    entity
                        .string_buffer()
                        .map(|text| text.as_str().to_owned())
                        .filter(|text| text.contains("storm"))
                })
                .unwrap_or_default()
        };
        assert_eq!(text(&runtime), "the storm came\n");
        // Nothing selected: the word at the caret changes.
        runtime.request(
            ShellRequest::AppletCommand {
                applet: writer.clone(),
                command: "format.case.upper".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        assert_eq!(text(&runtime), "the storm CAME\n");
        frames(&mut runtime, &ctx, size, start, |frame| {
            if frame == 1 {
                vec![egui::Event::Key {
                    key: egui::Key::Z,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                }]
            } else {
                Vec::new()
            }
        });
        assert_eq!(text(&runtime), "the storm came\n");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_writer_records_changes_and_accepts_them() {
        let (mut runtime, root) = runtime("sue-writer-changes");
        let ctx = egui::Context::default();
        let size = vec2(1440.0, 900.0);
        let start = Instant::now();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        let writer = AppletId::new(sue_writer::ID).unwrap();
        runtime.request(
            ShellRequest::Create {
                applet: writer.clone(),
                key: "blank".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        frames(&mut runtime, &ctx, size, start, |frame| {
            if frame == 1 {
                vec![egui::Event::Text("The cat".to_owned())]
            } else {
                Vec::new()
            }
        });
        runtime.typing.flush(&mut runtime.engine.history);
        let command = |runtime: &mut Runtime, command: &str| {
            runtime.request(
                ShellRequest::AppletCommand {
                    applet: writer.clone(),
                    command: command.to_owned(),
                },
                &mut NoTextures,
                start,
            );
        };
        let key = |key: egui::Key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let doc = |runtime: &Runtime| {
            runtime
                .engine
                .world
                .arena
                .iter()
                .find_map(|(_, entity)| {
                    let text = entity.string_buffer()?.as_str().to_owned();
                    text.starts_with("The")
                        .then(|| (text, entity.ext::<sue_model::TextMarks>().cloned()))
                })
                .unwrap()
        };
        command(&mut runtime, "review.track");
        // Typed while recording: inserted. Backspace takes an insertion out
        // for good, then strikes "t" of the old text through.
        frames(&mut runtime, &ctx, size, start, |frame| match frame {
            1 => vec![egui::Event::Text("s sat".to_owned())],
            2 => vec![key(egui::Key::Backspace), key(egui::Key::Backspace)],
            3 => vec![key(egui::Key::Home)],
            4 => vec![key(egui::Key::Delete)],
            _ => Vec::new(),
        });
        let (text, marks) = doc(&runtime);
        let marks = marks.unwrap();
        assert_eq!(text, "The cats s\n", "{marks:?}");
        let changes = sue_model::changes::changes(&marks);
        assert!(
            changes
                .iter()
                .any(|tracked| tracked.inserted && tracked.range == (7..10))
        );
        assert!(
            changes
                .iter()
                .any(|tracked| !tracked.inserted && tracked.range == (0..1))
        );
        assert_eq!(changes[0].change.author, "Author");
        // Accept All keeps the insertion and drops the struck "T".
        command(&mut runtime, "review.accept_all");
        let (text, marks) = doc_any(&runtime);
        assert_eq!(text, "he cats s\n");
        assert!(sue_model::changes::changes(&marks).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The first document's text and marks.
    fn doc_any(runtime: &Runtime) -> (String, sue_model::TextMarks) {
        runtime
            .engine
            .world
            .arena
            .iter()
            .find_map(|(_, entity)| {
                let text = entity.string_buffer()?.as_str().to_owned();
                text.contains("cat").then(|| {
                    (
                        text,
                        entity
                            .ext::<sue_model::TextMarks>()
                            .cloned()
                            .unwrap_or_default(),
                    )
                })
            })
            .unwrap()
    }

    #[test]
    fn sue_writer_comments_go_on_words_and_come_off_with_undo() {
        let (mut runtime, root) = runtime("sue-writer-comments");
        let ctx = egui::Context::default();
        let size = vec2(1440.0, 900.0);
        let start = Instant::now();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        let writer = AppletId::new(sue_writer::ID).unwrap();
        runtime.request(
            ShellRequest::Create {
                applet: writer.clone(),
                key: "blank".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        frames(&mut runtime, &ctx, size, start, |frame| {
            if frame == 1 {
                vec![egui::Event::Text("A dark and stormy night".to_owned())]
            } else {
                Vec::new()
            }
        });
        runtime.typing.flush(&mut runtime.engine.history);
        let command = |runtime: &mut Runtime, command: &str| {
            runtime.request(
                ShellRequest::AppletCommand {
                    applet: writer.clone(),
                    command: command.to_owned(),
                },
                &mut NoTextures,
                start,
            );
        };
        let key = |key: egui::Key, modifiers: egui::Modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        // New Comment on the word at the caret, typed and sent with
        // Ctrl+Enter.
        command(&mut runtime, "review.comment");
        frames(&mut runtime, &ctx, size, start, |frame| match frame {
            1 => vec![egui::Event::Text("Too much?".to_owned())],
            2 => vec![key(egui::Key::Enter, egui::Modifiers::COMMAND)],
            _ => Vec::new(),
        });
        let comments = |runtime: &Runtime| {
            runtime
                .engine
                .world
                .arena
                .iter()
                .filter(|(_, entity)| entity.class() == sue_model::comments::COMMENT)
                .map(|(_, entity)| entity.id())
                .collect::<Vec<_>>()
        };
        let found = comments(&runtime);
        assert_eq!(found.len(), 1, "a comment was made");
        let thread = sue_model::comments::thread(&runtime.engine.world.arena, found[0]).unwrap();
        assert_eq!(thread.first.text, "Too much?");
        let marked = runtime.engine.world.arena.iter().any(|(_, entity)| {
            entity.ext::<sue_model::TextMarks>().is_some_and(|marks| {
                marks.marks().iter().any(|mark| {
                    mark.kind == sue_model::mark_kind::COMMENT
                        && mark.value == sue_core::Value::Entity(found[0])
                })
            })
        });
        assert!(marked, "its words carry it");
        // Delete it from the Review menu; undo brings it back.
        command(&mut runtime, "review.delete_comment");
        assert!(comments(&runtime).is_empty());
        frames(&mut runtime, &ctx, size, start, |frame| {
            if frame == 1 {
                vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]
            } else {
                Vec::new()
            }
        });
        assert_eq!(comments(&runtime), found);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_writer_tables_take_typing_grow_with_tab_undo_and_export() {
        let (mut runtime, root) = runtime("sue-writer-tables");
        let ctx = egui::Context::default();
        let size = vec2(1440.0, 900.0);
        let start = Instant::now();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        let writer = AppletId::new(sue_writer::ID).unwrap();
        runtime.request(
            ShellRequest::Create {
                applet: writer.clone(),
                key: "novel".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        let command = |runtime: &mut Runtime, command: &str| {
            runtime.request(
                ShellRequest::AppletCommand {
                    applet: writer.clone(),
                    command: command.to_owned(),
                },
                &mut NoTextures,
                start,
            );
        };
        command(&mut runtime, "table.insert.2x2");
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        let table_of = |runtime: &Runtime| {
            runtime
                .engine
                .world
                .arena
                .iter()
                .find(|(_, entity)| entity.class() == sue_model::TEXT_TABLE)
                .and_then(|(_, entity)| entity.ext::<sue_model::Table>().cloned())
                .expect("a table in the arena")
        };
        assert_eq!(table_of(&runtime).size(), (2, 2));
        let key = |key: egui::Key, modifiers: egui::Modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        // Typing goes into the first cell; Tab moves on, and Tab in the last
        // cell adds a row.
        frames(&mut runtime, &ctx, size, start, |frame| match frame {
            1 => vec![egui::Event::Text("Name".to_owned())],
            2 => vec![key(egui::Key::Tab, egui::Modifiers::NONE)],
            3 => vec![egui::Event::Text("Qty".to_owned())],
            4 => vec![key(egui::Key::Tab, egui::Modifiers::NONE)],
            5 => vec![egui::Event::Text("Bolt".to_owned())],
            6 => vec![
                key(egui::Key::Tab, egui::Modifiers::NONE),
                key(egui::Key::Tab, egui::Modifiers::NONE),
            ],
            _ => Vec::new(),
        });
        let table = table_of(&runtime);
        assert_eq!(table.text(0, 0), "Name");
        assert_eq!(table.text(0, 1), "Qty");
        assert_eq!(table.text(1, 0), "Bolt");
        assert_eq!(table.size(), (3, 2), "Tab in the last cell adds a row");
        // Undo takes the new row back, then the typing in the cell before.
        frames(&mut runtime, &ctx, size, start, |frame| {
            if frame == 1 {
                vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]
            } else {
                Vec::new()
            }
        });
        assert_eq!(table_of(&runtime).size(), (2, 2));
        frames(&mut runtime, &ctx, size, start, |frame| {
            if frame == 1 {
                vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]
            } else {
                Vec::new()
            }
        });
        assert_eq!(
            table_of(&runtime).text(1, 0),
            "",
            "{:?}",
            table_of(&runtime)
        );
        assert_eq!(table_of(&runtime).text(0, 1), "Qty");
        // A column after the caret's, from the Table menu.
        command(&mut runtime, "table.column_right");
        assert_eq!(table_of(&runtime).size(), (2, 3));
        // The table goes out in a web page and a Word file.
        for (format, needle) in [("html", "<td>"), ("docx", ""), ("odt", "")] {
            let home = runtime.engine.catalog.folders["home"].clone();
            std::fs::create_dir_all(&home).unwrap();
            let path = home.join(format!("tables.{format}"));
            runtime.request(
                ShellRequest::Picked {
                    pick: FilePick::save(
                        Some(writer.clone()),
                        &format!("compile.{format}"),
                        "Export",
                        "tables",
                    ),
                    path: path.clone(),
                },
                &mut NoTextures,
                start,
            );
            let bytes = std::fs::read(&path).unwrap();
            if format == "html" {
                let html = String::from_utf8(bytes).unwrap();
                assert!(html.contains(needle) && html.contains("Qty"), "{html}");
            } else {
                assert!(bytes.starts_with(b"PK"));
            }
        }
        // The Word file opens with its table a table again.
        let tables = |runtime: &Runtime| {
            runtime
                .engine
                .world
                .arena
                .iter()
                .filter(|(_, entity)| entity.class() == sue_model::TEXT_TABLE)
                .count()
        };
        let before = tables(&runtime);
        let word = runtime.engine.catalog.folders["home"].join("tables.docx");
        runtime.open_path(&word, &mut NoTextures, start);
        assert_eq!(
            tables(&runtime),
            before + 1,
            "the Word file's table came back"
        );
        // So does an OpenDocument file's.
        let odt = runtime.engine.catalog.folders["home"].join("tables.odt");
        runtime.open_path(&odt, &mut NoTextures, start);
        assert_eq!(
            tables(&runtime),
            before + 2,
            "the OpenDocument table came back"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_sue_menus_layout_switches_the_hub_and_sue_paint_and_stays() {
        let (host, root) = temp_host("layout");
        let ctx = egui::Context::default();
        let size = vec2(1440.0, 900.0);
        let start = Instant::now();
        let mut runtime = Runtime::new(Engine::boot(&host).unwrap()).unwrap();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        assert_eq!(runtime.shell.form_factor(), FormFactor::Desktop);
        let desktop_top = runtime.shell.page_rect().top();
        let pick = |runtime: &mut Runtime, layout: &str| {
            runtime.request(ShellRequest::SueMenu, &mut NoTextures, start);
            let picked = runtime.with_shell(&mut NoTextures, start, |shell, io, now| {
                shell.sue_menu_pick(&["Layout", layout], io, now)
            });
            assert!(picked, "{layout}");
            frames(runtime, &ctx, size, start, |_| Vec::new());
        };

        // Phone: the Hub and Sue Paint draw the phone layout, with the
        // phone's taller bar, in the same window.
        pick(&mut runtime, "Phone");
        assert_eq!(runtime.shell.form_factor(), FormFactor::Mobile);
        assert!(runtime.shell.hub_open());
        assert!(runtime.shell.page_rect().top() > desktop_top);
        assert_eq!(runtime.shell.page_rect().width(), size.x);
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
            &mut NoTextures,
            start,
        );
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        assert_eq!(
            runtime.shell.page().map(AppletId::as_str),
            Some(sue_paint::ID)
        );
        assert_eq!(runtime.shell.form_factor(), FormFactor::Mobile);

        // Tablet, then back to the desktop the system detects.
        pick(&mut runtime, "Tablet");
        assert_eq!(runtime.shell.form_factor(), FormFactor::Tablet);
        pick(&mut runtime, "Phone");
        // The rice file keeps it, so it survives a restart.
        let rice = std::fs::read_to_string(runtime.engine.rice_path()).unwrap();
        assert!(rice.contains("interface = \"phone\""), "{rice}");
        let mut again = Runtime::new(Engine::boot(&host).unwrap()).unwrap();
        frames(&mut again, &ctx, size, start, |_| Vec::new());
        assert_eq!(again.shell.form_factor(), FormFactor::Mobile);
        pick(&mut again, "Automatic (Desktop)");
        assert_eq!(again.shell.form_factor(), FormFactor::Desktop);
        assert_eq!(again.shell.page_rect().top(), desktop_top);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn settings_fits_each_screen_and_how_it_is_held() {
        let (mut runtime, root) = runtime("settings-sheets");
        let ctx = egui::Context::default();
        let start = Instant::now();
        let sheet = |runtime: &mut Runtime, interface: &str, w: f32, h: f32| {
            runtime
                .engine
                .world
                .rice
                .set(
                    sue_wm::INTERFACE_KEY,
                    sue_rice::RiceValue::Text(interface.to_owned()),
                )
                .unwrap();
            frames(runtime, &ctx, vec2(w, h), start, |_| Vec::new());
            runtime.request(ShellRequest::OpenSettings(None), &mut NoTextures, start);
            frames(runtime, &ctx, vec2(w, h), start, |_| Vec::new());
            runtime.shell.window_rect().expect("Settings is open")
        };
        // A phone's fills the screen, upright and on its side.
        for (w, h) in [(393.0, 851.0), (851.0, 393.0)] {
            let rect = sheet(&mut runtime, "phone", w, h);
            assert_eq!(rect.size(), vec2(w, h), "{rect:?}");
        }
        // A tablet's: nearly the whole width upright, the whole height on
        // its side, and never past the screen.
        let upright = sheet(&mut runtime, "tablet", 800.0, 1280.0);
        assert!(upright.width() > 700.0 && upright.height() < 1280.0 * 0.9);
        let side = sheet(&mut runtime, "tablet", 1280.0, 800.0);
        assert!(side.height() > 700.0 && side.width() < 1280.0 * 0.9);
        // A desktop's keeps a largest size, and fits a small window.
        let big = sheet(&mut runtime, "desktop", 1920.0, 1080.0);
        assert_eq!(big.size(), vec2(980.0, 720.0));
        let small = sheet(&mut runtime, "desktop", 600.0, 420.0);
        let window = egui::Rect::from_min_size(Pos2::ZERO, vec2(600.0, 420.0));
        assert!(window.contains_rect(small), "{small:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_sue_menu_changes_the_language_and_a_tool_can_keep_its_own() {
        let (mut runtime, root) = runtime("language");
        let ctx = egui::Context::default();
        let size = vec2(1440.0, 900.0);
        let start = Instant::now();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());

        // Headless runs start in English; the Sue menu offers every language.
        runtime.with_shell(&mut NoTextures, start, |shell, io, _| {
            shell.open_sue_menu(io)
        });
        let languages = runtime.shell.sue_menu_labels(&["Language"]);
        let japanese = sue_lang::name("ja");
        assert!(languages.contains(&japanese), "{languages:?}");
        assert!(languages.contains(&sue_lang::name("es")), "{languages:?}");

        // Picking Japanese is an undoable rice edit, and the menu speaks it.
        let picked = runtime.with_shell(&mut NoTextures, start, |shell, io, now| {
            shell.sue_menu_pick(&["Language", &japanese], io, now)
        });
        assert!(picked);
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        assert_eq!(
            runtime.engine.world.rice.get_str(sue_wm::LANGUAGE_KEY),
            Some("ja")
        );
        runtime.with_shell(&mut NoTextures, start, |shell, io, _| {
            shell.open_sue_menu(io)
        });
        let labels = runtime.shell.sue_menu_labels(&[]);
        let settings = {
            let _ja = sue_lang::scope("ja");
            sue_lang::text("wm", "settings")
        };
        assert_ne!(settings, "Settings");
        assert!(labels.contains(&settings), "{labels:?}");
        runtime.shell.dismiss_overlays();

        // The Image Viewer set apart in Spanish: what it says is Spanish,
        // while Sue's own menu stays Japanese.
        let viewer = AppletId::new(crate::viewer::ID).unwrap();
        runtime
            .engine
            .world
            .rice
            .set(
                &sue_wm::language_key(&viewer),
                sue_rice::RiceValue::Text("es".to_owned()),
            )
            .unwrap();
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        runtime.request(
            ShellRequest::AppletCommand {
                applet: viewer,
                command: "no.such.command".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        let spanish = {
            let _es = sue_lang::scope("es");
            sue_lang::tr!("wm", "is-not-available", command = "no.such.command")
        };
        assert_eq!(
            runtime.shell.message().map(|(text, _)| text),
            Some(spanish.as_str())
        );
        runtime.with_shell(&mut NoTextures, start, |shell, io, _| {
            shell.open_sue_menu(io)
        });
        assert!(runtime.shell.sue_menu_labels(&[]).contains(&settings));

        // English again: the menu reads English.
        let _ = runtime.engine.world.rice.set(
            sue_wm::LANGUAGE_KEY,
            sue_rice::RiceValue::Text(sue_lang::ENGLISH.to_owned()),
        );
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        runtime.with_shell(&mut NoTextures, start, |shell, io, _| {
            shell.open_sue_menu(io)
        });
        assert!(
            runtime
                .shell
                .sue_menu_labels(&[])
                .contains(&"Settings".to_owned())
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sue_paint_opens_on_its_start_page_where_draw_opens_the_last_picture_or_a_new_one() {
        let (mut runtime, root) = runtime("paint-start");
        let ctx = egui::Context::default();
        let size = vec2(1440.0, 900.0);
        let start = Instant::now();
        let paint = AppletId::new(sue_paint::ID).unwrap();
        let pictures = |runtime: &Runtime| {
            runtime
                .engine
                .world
                .arena
                .iter()
                .filter(|(_, entity)| sue_model::picture::is_picture(entity))
                .count()
        };
        let click = |at: Pos2| {
            move |frame: usize| {
                let button = |pressed| egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                };
                match frame {
                    0 => vec![egui::Event::PointerMoved(at)],
                    1 => vec![button(true)],
                    2 => vec![button(false)],
                    _ => Vec::new(),
                }
            }
        };
        runtime.request(ShellRequest::Open(paint.clone()), &mut NoTextures, start);
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());
        // The round Draw button at the top of the start page's rail.
        let draw = runtime.shell.page_rect().min + vec2(42.0, 40.0);

        // No canvas and no picture worked on lately: Draw asks for a new one.
        frames(&mut runtime, &ctx, size, start, click(draw));
        assert!(runtime.applets.captures_keys(), "Draw shows the New dialog");
        assert_eq!(pictures(&runtime), 0);
        if let Some(applet) = runtime.applets.get_mut(&paint) {
            applet.navigate("dialog/close");
        }
        frames(&mut runtime, &ctx, size, start, |_| Vec::new());

        // With a picture opened lately, Draw opens it as a canvas.
        std::fs::create_dir_all(&root).unwrap();
        let photo = root.join("photo.png");
        let pixels = vec![120_u8; 8 * 6 * 4];
        std::fs::write(&photo, sue_io::encode_png_rgba(8, 6, &pixels).unwrap()).unwrap();
        runtime.request(
            ShellRequest::Remember {
                path: photo.clone(),
                saved: false,
            },
            &mut NoTextures,
            start,
        );
        frames(&mut runtime, &ctx, size, start, click(draw));
        assert_eq!(pictures(&runtime), 1, "Draw opened the last picture");
        assert!(!runtime.applets.captures_keys());
        assert_eq!(
            runtime.shell.page().map(AppletId::as_str),
            Some(sue_paint::ID)
        );
        // Opening it is a transfer the start page lists.
        assert!(
            runtime
                .transfers
                .list()
                .iter()
                .any(|transfer| transfer.label == "photo.png")
        );
        // The pages and popups open by name, File, Start Page included.
        let applet = runtime.applets.get_mut(&paint).unwrap();
        for target in ["start/materials", "start/help", "start/close"] {
            assert!(applet.navigate(target), "{target}");
        }
        // File, Start Page is a command Sue Paint has.
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint,
                command: "file.start_page".to_owned(),
            },
            &mut NoTextures,
            start,
        );
        assert!(runtime.shell.message().is_none_or(|(_, error)| !error));
        let _ = std::fs::remove_dir_all(&root);
    }
}
