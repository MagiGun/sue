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
use sue_core::SueFile;
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
use crate::update::{self, Updater};
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
            sue_history: if $runtime.engine.history_key == crate::engine::HistoryKey::Sue {
                None
            } else {
                Some(
                    $runtime
                        .engine
                        .parked
                        .entry(crate::engine::HistoryKey::Sue)
                        .or_default(),
                )
            },
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
    /// Sue's own updates.
    updater: Updater,
    /// When to read a running update check or download next.
    next_update_poll: Option<Instant>,
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
        let updater = Updater::new(&engine.paths);
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
            updater,
            next_update_poll: None,
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

    /// The applets' views that want windows of their own, for the host
    /// to open (none where the system gives Sue one window).
    #[must_use]
    pub fn windows(&self) -> Vec<sue_platform::WindowSpec> {
        self.shell.os_windows()
    }

    /// Builds one frame of the window of the view named `key` into `ui`,
    /// which covers that window, then serves what it asked for.
    pub fn window_ui(
        &mut self,
        key: &str,
        ui: &mut egui::Ui,
        textures: &mut dyn TextureUpload,
        now: Instant,
    ) {
        let _language = sue_lang::scope(&sue_wm::sue_language(&self.engine.world.rice));
        {
            let mut io = shell_io!(self, textures);
            self.shell.view_window_ui(key, ui, &mut io, now);
        }
        self.serve_requests(textures, now);
        self.refresh_rice();
    }

    /// The window of the view named `key` was closed (its close button,
    /// or the system's).
    pub fn window_closed(&mut self, key: &str, textures: &mut dyn TextureUpload, now: Instant) {
        let mut io = shell_io!(self, textures);
        self.shell.close_view(key, &mut io, now);
    }

    /// Changes whenever what a window shows may have changed: a document
    /// or a setting.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.engine
            .world
            .arena
            .revision()
            .wrapping_add(self.engine.world.rice.revision())
    }

    /// Builds one frame of the whole window into the root `ui`.
    /// Puts in use the history of the studio or tool on the page and its
    /// document, or Sue's own on the Hub; typing still open goes to the
    /// history it was typed in first.
    pub fn follow_history(&mut self) {
        let key = match self.shell.page() {
            Some(page) => crate::engine::HistoryKey::Applet(
                page.clone(),
                self.applets
                    .get(page)
                    .and_then(|applet| applet.history_scope(&self.engine.world.arena)),
            ),
            None => crate::engine::HistoryKey::Sue,
        };
        if key != self.engine.history_key {
            self.typing.flush(&mut self.engine.history);
        }
        self.engine.use_history(key);
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, textures: &mut dyn TextureUpload, now: Instant) {
        let _language = sue_lang::scope(&sue_wm::sue_language(&self.engine.world.rice));
        if !self.booted {
            self.booted = true;
            // The icon pack chosen in Settings, and icons replaced one by
            // one, from the first frame on.
            let icons = sue_wm::icons::pack::IconHome::of(&self.engine.paths);
            if icons.chosen().is_some() || !icons.overrides().is_empty() {
                let _ = sue_wm::icons::pack::refresh(ui.ctx(), &icons);
            }
            let mut io = shell_io!(self, textures);
            self.shell.boot(&mut io);
            if self.interactive
                && self
                    .engine
                    .world
                    .rice
                    .get_bool("updates.check_automatically")
                    .unwrap_or(true)
            {
                self.updater.check_if_due(unix_now());
            }
        }
        self.poll_system_dialogs(textures, now);
        self.follow_history();
        {
            let mut io = shell_io!(self, textures);
            self.shell.ui(ui, &mut io, now);
        }
        if self.confirm_quit {
            self.quit_dialog(ui.ctx());
        }
        self.updates(ui.ctx(), now);
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

    /// Reads a running update check or download and draws the update
    /// window when it shows.
    fn updates(&mut self, ctx: &egui::Context, now: Instant) {
        let polled = self.updater.poll();
        let theme = self.shell.theme().clone();
        let clicked = self.updater.ui(ctx, &theme);
        for action in [polled, clicked].into_iter().flatten() {
            match action {
                update::Action::OpenUrl(link) => self.open_url(&link),
                update::Action::Copy(text) => match sue_platform::copy_text(&text) {
                    Ok(()) => self.shell.notify(tr!("update-command-copied"), false),
                    Err(error) => self.shell.notify(error, true),
                },
                update::Action::Quit => {
                    self.shutdown();
                    self.quit = true;
                }
                update::Action::CheckAgain => self.updater.check(false),
            }
        }
        self.next_update_poll = self.updater.busy().then(|| now + update::POLL);
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
            self.next_update_poll,
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
        // A running update check or download is read, and its window
        // drawn, in the frame.
        if self.next_update_poll.is_some_and(|due| due <= now) {
            self.next_update_poll = None;
            redraw = true;
        }
        self.schedule_autosave(now);
        redraw
    }

    /// Closing: each piece of work not saved is asked about first (the
    /// shell's Save, Do Not Save, Cancel), then "Quit Sue?" when the rice
    /// asks for it. Returns `true` to close now.
    pub fn close_requested(&mut self) -> bool {
        if self.quit {
            return true;
        }
        if !self.interactive {
            return self.quit_checked();
        }
        self.shell.begin_quit();
        false
    }

    /// Every piece of work not saved has been asked about: "Quit Sue?"
    /// when the rice asks for it, else quit. Returns `true` when closing.
    fn quit_checked(&mut self) -> bool {
        if self
            .engine
            .world
            .rice
            .get_bool("general.confirm_on_exit")
            .unwrap_or(false)
        {
            self.confirm_quit = true;
            self.shell.touch();
            return false;
        }
        self.shutdown();
        self.quit = true;
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
        self.engine.forget_histories();
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
            "app.quit.checked" => {
                self.quit_checked();
            }
            "rice.reload" => self.reload_rice(),
            "app.check_updates" => {
                if self.interactive {
                    self.updater.check(false);
                    self.shell.touch();
                } else {
                    self.shell.notify(tr!("checking-for-updates"), false);
                }
            }
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
        self.engine.forget_histories();
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
        // Settings are Sue's: they go to Sue's own history.
        let world = &mut self.engine.world;
        let sue = if self.engine.history_key == crate::engine::HistoryKey::Sue {
            &mut self.engine.history
        } else {
            self.engine
                .parked
                .entry(crate::engine::HistoryKey::Sue)
                .or_default()
        };
        match sue.apply(world, RiceEdit(txn)) {
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
pub(crate) mod tests;
