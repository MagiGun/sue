//! Player: a music player for the songs on this computer, in the left
//! drawer and, larger, in a window. Like the streaming players people know:
//! a library of songs, albums (from the tags, or made by hand), artists,
//! liked songs and playlists; shuffle, previous, play and pause, next and
//! repeat; a queue; and the song keeps playing whatever Sue is doing.
//!
//! Songs are added from the File Manager (Add to Player) or with Add Music.
//! The library is the listener's own: it is kept in Sue's data folder, not
//! in projects, and the songs are decoded on a background thread into
//! shared buffers the transport plays from, so nothing is copied into the
//! project or the undo history.

pub(crate) mod library;
mod work;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{
    Align, Align2, Color32, Layout, Rect, RichText, ScrollArea, Sense, Stroke, Ui, pos2, vec2,
};
use sue_core::{PCMFloat1D, Shared};
use sue_platform::write_atomic;
use sue_rice::{RiceDeclaration, RiceParameter, RiceValue, SetRiceValue};
use sue_wm::{
    AppletContext, AppletId, AppletManifest, AppletRole, Clip, ClipSource, FilePick, ShellRequest,
    SueApplet, Theme, Tick, widgets,
};

use self::library::{AUDIO, Collection, Kind, Library};
use self::work::{Done, Worker};
use crate::files::thumbs::Thumbs;
use crate::util::clock;

/// The applet id.
pub const ID: &str = "player";

/// The transport owner name of the Player.
const OWNER: &str = "player";

/// The library file in the Player's folder.
const LIBRARY_FILE: &str = "library.txt";

/// Picture files a folder of songs keeps its cover in.
const COVERS: [&str; 8] = [
    "cover.jpg",
    "cover.png",
    "folder.jpg",
    "folder.png",
    "front.jpg",
    "front.png",
    "album.jpg",
    "albumart.jpg",
];

/// What happens at the end of the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Repeat {
    Off,
    All,
    One,
}

impl Repeat {
    fn from_rice(value: &str) -> Self {
        match value {
            "all" => Self::All,
            "one" => Self::One,
            _ => Self::Off,
        }
    }

    fn next(self) -> &'static str {
        match self {
            Self::Off => "all",
            Self::All => "one",
            Self::One => "off",
        }
    }
}

/// Which part of the library is showing.
#[derive(Clone, Debug, Default, PartialEq)]
enum Page {
    #[default]
    Songs,
    Albums,
    Artists,
    Liked,
    Playlists,
    Queue,
    Album(String, String),
    Artist(String),
    Playlist(usize),
}

impl Page {
    const TABS: [(&'static str, Page); 6] = [
        ("Songs", Page::Songs),
        ("Albums", Page::Albums),
        ("Artists", Page::Artists),
        ("Liked", Page::Liked),
        ("Playlists", Page::Playlists),
        ("Queue", Page::Queue),
    ];

    fn tab(&self) -> &'static str {
        match self {
            Self::Songs => "Songs",
            Self::Albums | Self::Album(..) => "Albums",
            Self::Artists | Self::Artist(_) => "Artists",
            Self::Liked => "Liked",
            Self::Playlists | Self::Playlist(_) => "Playlists",
            Self::Queue => "Queue",
        }
    }
}

/// A decoded song, ready for the transport.
#[derive(Debug, Clone)]
struct Loaded {
    path: PathBuf,
    pcm: Shared<PCMFloat1D>,
    gain: f32,
}

/// What a song row asked for.
enum RowAction {
    Play(usize),
    Queue(PathBuf, bool),
    AddTo(PathBuf, usize),
    NewPlaylist(PathBuf),
    Like(PathBuf),
    Show(PathBuf),
    Remove(PathBuf),
    Move(usize, bool),
    Drop(usize),
}

/// The Player.
#[derive(Debug, Default)]
pub struct Player {
    library: Library,
    loaded: bool,
    queue: Vec<PathBuf>,
    at: Option<usize>,
    current: Option<Loaded>,
    next: Option<Loaded>,
    loading: Option<PathBuf>,
    preloading: Option<PathBuf>,
    start_at: f64,
    play_when_loaded: bool,
    playing: bool,
    paused_at: f64,
    page: Page,
    search: String,
    reading: usize,
    read_count: usize,
    skipped: Vec<PathBuf>,
    volume_drag: Option<f64>,
    renaming: Option<(usize, String)>,
    covers: Thumbs,
    seed: u64,
    worker: Worker,
}

fn rice_bool(cx: &AppletContext<'_>, path: &str, default: bool) -> bool {
    cx.rice().get_bool(path).unwrap_or(default)
}

fn set_rice(cx: &mut AppletContext<'_>, path: &str, value: RiceValue) {
    if let Err(error) = cx.edit_rice(SetRiceValue::new(path, value)) {
        cx.fail(format!("Could not change {path}: {error}"));
    }
}

/// A small round icon button; `on` draws it lit.
fn icon_button(ui: &mut Ui, icon: &str, on: bool, size: f32, tip: &str, theme: &Theme) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter()
            .circle_filled(rect.center(), size / 2.0, theme.hover);
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        theme.font(size * 0.5),
        if on || hovered {
            theme.text
        } else {
            theme.muted
        },
    );
    if on {
        ui.painter()
            .circle_filled(pos2(rect.center().x, rect.bottom() - 2.0), 1.6, theme.text);
    }
    response.on_hover_text(tip).clicked()
}

impl Player {
    // ----- The library file ---------------------------------------------------

    fn folder(cx: &AppletContext<'_>) -> Option<PathBuf> {
        cx.catalog.folders.get("player").cloned()
    }

    fn load_library(&mut self, cx: &AppletContext<'_>) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        if let Some(text) = Self::folder(cx)
            .and_then(|folder| std::fs::read_to_string(folder.join(LIBRARY_FILE)).ok())
        {
            self.library = Library::parse(&text);
        }
    }

    fn save(&self, cx: &mut AppletContext<'_>) {
        let Some(folder) = Self::folder(cx) else {
            return;
        };
        let _ = std::fs::create_dir_all(&folder);
        if let Err(error) = write_atomic(
            &folder.join(LIBRARY_FILE),
            self.library.to_text().as_bytes(),
        ) {
            cx.fail(format!("Could not save the music library: {error}"));
        }
    }

    /// Adds a song, or every song under a folder, in the background.
    fn import(&mut self, path: PathBuf, cx: &mut AppletContext<'_>) {
        self.load_library(cx);
        self.reading += 1;
        self.worker.read(vec![path]);
    }

    // ----- Playing ------------------------------------------------------------

    fn ours(cx: &AppletContext<'_>) -> bool {
        cx.transport.clip().is_some_and(|clip| clip.owner == OWNER)
    }

    fn current_path(&self) -> Option<&PathBuf> {
        self.at.and_then(|at| self.queue.get(at))
    }

    /// Seconds into the song being heard.
    fn position(&self, cx: &AppletContext<'_>) -> f64 {
        match (&self.current, Self::ours(cx)) {
            (Some(loaded), true) => {
                cx.transport.position() / f64::from(loaded.pcm.sample_rate().max(1))
            }
            _ => self.paused_at,
        }
    }

    /// The song's length in seconds, from its sound or the library.
    fn length(&self) -> f64 {
        if let Some(loaded) = &self.current {
            return loaded.pcm.duration_seconds();
        }
        self.current_path()
            .and_then(|path| self.library.song(path))
            .map_or(0.0, |song| song.seconds)
    }

    fn random(&mut self) -> u64 {
        if self.seed == 0 {
            self.seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0x9E37_79B9, |since| since.as_nanos() as u64)
                | 1;
        }
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        self.seed
    }

    /// Plays `list` from song `first`; shuffled, the chosen song still
    /// comes first.
    fn play_list(&mut self, mut list: Vec<PathBuf>, first: usize, cx: &mut AppletContext<'_>) {
        if list.is_empty() {
            return;
        }
        let first = first.min(list.len() - 1);
        if rice_bool(cx, "player.shuffle", false) {
            let chosen = list.remove(first);
            for index in (1..list.len()).rev() {
                let other = (self.random() % (index as u64 + 1)) as usize;
                list.swap(index, other);
            }
            list.insert(0, chosen);
            self.at = Some(0);
        } else {
            self.at = Some(first);
        }
        self.queue = list;
        self.next = None;
        self.play_current(cx, 0.0);
    }

    /// Starts the queue's current song at `from` seconds, decoding it first
    /// if it is not ready.
    fn play_current(&mut self, cx: &mut AppletContext<'_>, from: f64) {
        let Some(path) = self.current_path().cloned() else {
            return;
        };
        if self
            .current
            .as_ref()
            .is_some_and(|loaded| loaded.path == path)
        {
            self.begin(cx, from);
            return;
        }
        if let Some(ready) = self.next.take().filter(|loaded| loaded.path == path) {
            self.current = Some(ready);
            self.begin(cx, from);
            return;
        }
        cx.transport.stop(cx.audio.as_deref_mut());
        self.current = None;
        self.loading = Some(path.clone());
        self.play_when_loaded = true;
        self.start_at = from;
        self.paused_at = from;
        self.worker.decode(path);
    }

    /// Loads the decoded current song into the transport and plays it.
    fn begin(&mut self, cx: &mut AppletContext<'_>, from: f64) {
        let Some(loaded) = self.current.clone() else {
            return;
        };
        if cx.audio.is_none() {
            cx.request(ShellRequest::Command("audio.start".to_owned()));
        }
        let gain = if rice_bool(cx, "player.normalize", true) {
            loaded.gain
        } else {
            1.0
        };
        let rate = f64::from(loaded.pcm.sample_rate().max(1));
        cx.transport.load(
            Clip {
                source: ClipSource::Stream(loaded.pcm.clone()),
                start: 0,
                end: loaded.pcm.frames(),
                gain,
                owner: OWNER.to_owned(),
            },
            cx.audio.as_deref_mut(),
        );
        if from > 0.0 {
            cx.transport.seek(from * rate, cx.audio.as_deref_mut());
        }
        cx.transport.set_playing(true, cx.audio.as_deref_mut());
        self.playing = true;
        self.preload(cx);
    }

    /// Decodes the song after this one early, so it starts at once.
    fn preload(&mut self, cx: &AppletContext<'_>) {
        if !rice_bool(cx, "player.preload", true) {
            return;
        }
        let Some(at) = self.at else {
            return;
        };
        let next = self
            .queue
            .get(at + 1)
            .or_else(|| {
                (Repeat::from_rice(cx.rice().get_str("player.repeat").unwrap_or("off"))
                    == Repeat::All)
                    .then(|| self.queue.first())
                    .flatten()
            })
            .cloned();
        if let Some(next) = next
            && self.next.as_ref().is_none_or(|loaded| loaded.path != next)
            && self.preloading.as_ref() != Some(&next)
            && self
                .current
                .as_ref()
                .is_none_or(|loaded| loaded.path != next)
        {
            self.preloading = Some(next.clone());
            self.worker.decode(next);
        }
    }

    fn toggle(&mut self, cx: &mut AppletContext<'_>) {
        if Self::ours(cx) && cx.transport.is_playing() {
            self.paused_at = self.position(cx);
            cx.transport.set_playing(false, cx.audio.as_deref_mut());
            self.playing = false;
        } else if Self::ours(cx) {
            cx.transport.set_playing(true, cx.audio.as_deref_mut());
            self.playing = true;
        } else if self.current_path().is_some() {
            let from = self.paused_at;
            self.play_current(cx, from);
        } else {
            let all = self
                .library
                .songs
                .iter()
                .map(|song| song.path.clone())
                .collect();
            self.play_list(all, 0, cx);
        }
    }

    /// Moves along the queue. `by_hand` is a click on Previous or Next;
    /// otherwise the song ended.
    fn step(&mut self, forward: bool, by_hand: bool, cx: &mut AppletContext<'_>) {
        let Some(at) = self.at else {
            return;
        };
        let repeat = Repeat::from_rice(cx.rice().get_str("player.repeat").unwrap_or("off"));
        if !by_hand && repeat == Repeat::One {
            self.play_current(cx, 0.0);
            return;
        }
        if !forward && self.position(cx) > 3.0 {
            self.play_current(cx, 0.0);
            return;
        }
        let len = self.queue.len();
        let target = if forward {
            if at + 1 < len {
                Some(at + 1)
            } else if repeat != Repeat::Off || by_hand {
                Some(0)
            } else {
                None
            }
        } else {
            Some(
                at.checked_sub(1)
                    .unwrap_or(if repeat == Repeat::All { len - 1 } else { 0 }),
            )
        };
        match target {
            Some(index) => {
                self.at = Some(index);
                self.paused_at = 0.0;
                self.play_current(cx, 0.0);
            }
            None => {
                cx.transport.stop(cx.audio.as_deref_mut());
                self.playing = false;
                self.paused_at = 0.0;
            }
        }
    }

    fn seek(&mut self, fraction: f64, cx: &mut AppletContext<'_>) {
        let seconds = fraction.clamp(0.0, 1.0) * self.length();
        if Self::ours(cx)
            && let Some(loaded) = &self.current
        {
            let rate = f64::from(loaded.pcm.sample_rate().max(1));
            cx.transport.seek(seconds * rate, cx.audio.as_deref_mut());
        } else {
            self.paused_at = seconds;
        }
    }

    /// Takes in the background work and moves on at the end of a song.
    fn advance(&mut self, cx: &mut AppletContext<'_>) -> bool {
        let mut changed = false;
        let mut library_changed = false;
        for done in self.worker.collect() {
            changed = true;
            match done {
                Done::Decoded { path, pcm, gain } => {
                    let loaded = Loaded {
                        path: path.clone(),
                        pcm,
                        gain,
                    };
                    if self.loading.as_ref() == Some(&path) {
                        self.loading = None;
                        self.current = Some(loaded);
                        if self.play_when_loaded {
                            self.play_when_loaded = false;
                            let from = self.start_at;
                            self.begin(cx, from);
                        }
                    } else if self.preloading.as_ref() == Some(&path) {
                        self.preloading = None;
                        self.next = Some(loaded);
                    }
                }
                Done::Failed { path, error } => {
                    if self.loading.as_ref() == Some(&path) {
                        self.loading = None;
                        cx.fail(format!(
                            "Cannot play {}: {error}",
                            crate::util::file_name(&path)
                        ));
                    }
                    if self.preloading.as_ref() == Some(&path) {
                        self.preloading = None;
                    }
                }
                Done::Read(song) => {
                    self.read_count += 1;
                    self.library.add(song);
                    library_changed = true;
                }
                Done::Skipped(path) => self.skipped.push(path),
                Done::Finished => {
                    self.reading = self.reading.saturating_sub(1);
                    if self.reading == 0 {
                        let skipped = match self.skipped.first() {
                            None => String::new(),
                            Some(first) => format!(
                                " ({} could not be read, like {})",
                                self.skipped.len(),
                                crate::util::file_name(first)
                            ),
                        };
                        cx.notify(format!(
                            "Added {} songs to the Player{skipped}",
                            self.read_count
                        ));
                        self.read_count = 0;
                        self.skipped.clear();
                    }
                }
            }
        }
        if library_changed && self.reading == 0 {
            self.save(cx);
        }
        // The end of a song.
        if self.playing && Self::ours(cx) && !cx.transport.is_playing() {
            let end = cx.transport.clip().map_or(0, |clip| clip.end) as f64;
            if cx.transport.position() >= end - 2.0 {
                self.playing = false;
                changed = true;
                if rice_bool(cx, "player.autoplay", true) {
                    self.step(true, false, cx);
                }
            }
        }
        // Something else took the transport (the timeline, an audio card).
        if self.playing && !Self::ours(cx) && self.loading.is_none() {
            self.playing = false;
            changed = true;
        }
        changed
    }

    // ----- Pictures -----------------------------------------------------------

    /// The cover of a song's album: the collection's own picture, else a
    /// cover picture in the song's folder.
    fn cover_file(
        &self,
        song: Option<&Path>,
        cover: Option<&Path>,
        cx: &AppletContext<'_>,
    ) -> Option<PathBuf> {
        if let Some(cover) = cover.filter(|cover| cover.is_file()) {
            return Some(cover.to_path_buf());
        }
        if !rice_bool(cx, "player.covers", true) {
            return None;
        }
        let folder = song?.parent()?;
        COVERS
            .iter()
            .map(|name| folder.join(name))
            .find(|path| path.is_file())
    }

    /// Draws a cover into `rect`: the picture, or the album's first letter.
    fn cover_ui(
        &mut self,
        ui: &Ui,
        rect: Rect,
        file: Option<&Path>,
        name: &str,
        cx: &mut AppletContext<'_>,
    ) {
        let theme = cx.theme.clone();
        ui.painter()
            .rect_filled(rect, theme.control_radius, theme.hover);
        if let Some(file) = file {
            let meta = std::fs::metadata(file).ok();
            let version = crate::files::thumbs::version_of(
                meta.as_ref().and_then(|meta| meta.modified().ok()),
                meta.as_ref().map_or(0, std::fs::Metadata::len),
            );
            let size = meta.as_ref().map_or(0, std::fs::Metadata::len);
            if let Some(thumb) = self.covers.get(file, version, size) {
                let mut hasher = DefaultHasher::new();
                file.hash(&mut hasher);
                if let Some(texture) = cx.textures.texture(
                    hasher.finish() ^ 0xC0FE,
                    version,
                    thumb.size,
                    4,
                    &thumb.pixels,
                    false,
                ) {
                    ui.painter().image(
                        texture,
                        rect,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    return;
                }
            }
        }
        let letter = name.chars().next().map_or_else(
            || "\u{1F3B5}".to_owned(),
            |first| first.to_uppercase().to_string(),
        );
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            letter,
            theme.font(rect.height() * 0.42),
            theme.muted,
        );
    }

    // ----- The parts of the Player --------------------------------------------

    /// The song playing: its cover, title, artist and the like button.
    fn now_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>, side: f32) {
        let theme = cx.theme.clone();
        let path = self.current_path().cloned();
        let song = path
            .as_ref()
            .and_then(|path| self.library.song(path))
            .cloned();
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
            let cover = self.cover_file(path.as_deref(), None, cx);
            let album = song
                .as_ref()
                .map_or("", |song| song.album.as_str())
                .to_owned();
            self.cover_ui(ui, rect, cover.as_deref(), &album, cx);
            ui.vertical(|ui| {
                let (title, by) = match (&song, &path) {
                    (Some(song), _) => (song.title.clone(), song.by().to_owned()),
                    (None, Some(path)) => (crate::util::file_name(path), String::new()),
                    (None, None) => ("Nothing playing".to_owned(), "Pick a song below".to_owned()),
                };
                ui.label(
                    RichText::new(title)
                        .size(theme.ui_size * 1.1)
                        .color(theme.text),
                );
                ui.label(RichText::new(by).size(theme.small_size).color(theme.muted));
                if self.loading.is_some() {
                    ui.label(
                        RichText::new("Loading...")
                            .size(theme.small_size)
                            .color(theme.muted),
                    );
                }
            });
            if let Some(path) = path {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let liked = self.library.is_liked(&path);
                    if icon_button(
                        ui,
                        "\u{2764}",
                        liked,
                        28.0,
                        if liked { "Unlike" } else { "Like" },
                        &theme,
                    ) {
                        self.library.toggle_like(&path);
                        self.save(cx);
                    }
                });
            }
        });
    }

    /// Shuffle, previous, play or pause, next and repeat, then the bar.
    fn controls_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        let shuffle = rice_bool(cx, "player.shuffle", false);
        let repeat = Repeat::from_rice(cx.rice().get_str("player.repeat").unwrap_or("off"));
        let playing = Self::ours(cx) && cx.transport.is_playing();
        let width = 30.0 * 4.0 + 42.0 + 6.0 * 4.0;
        ui.horizontal(|ui| {
            ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
            ui.spacing_mut().item_spacing.x = 6.0;
            if icon_button(ui, "\u{1F500}", shuffle, 30.0, "Shuffle", &theme) {
                set_rice(cx, "player.shuffle", RiceValue::Bool(!shuffle));
            }
            if icon_button(ui, "\u{23EE}", false, 30.0, "Previous", &theme) {
                self.step(false, true, cx);
            }
            // Play and pause: a big round button, like the players people know.
            let (rect, response) = ui.allocate_exact_size(vec2(42.0, 42.0), Sense::click());
            ui.painter().circle_filled(
                rect.center(),
                20.0,
                if response.hovered() {
                    theme.text
                } else {
                    theme.border_active
                },
            );
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                if playing { "\u{23F8}" } else { "\u{25B6}" },
                theme.font(18.0),
                theme.void,
            );
            if response
                .on_hover_text(if playing { "Pause" } else { "Play" })
                .clicked()
            {
                self.toggle(cx);
            }
            if icon_button(ui, "\u{23ED}", false, 30.0, "Next", &theme) {
                self.step(true, true, cx);
            }
            let (icon, tip) = match repeat {
                Repeat::One => ("\u{1F502}", "Repeat this song"),
                Repeat::All => ("\u{1F501}", "Repeat the queue"),
                Repeat::Off => ("\u{1F501}", "Repeat"),
            };
            if icon_button(ui, icon, repeat != Repeat::Off, 30.0, tip, &theme) {
                set_rice(
                    cx,
                    "player.repeat",
                    RiceValue::Text(repeat.next().to_owned()),
                );
            }
        });
        // The bar: elapsed, a line you click or drag, the length.
        let length = self.length();
        let at = self.position(cx).min(length);
        ui.horizontal(|ui| {
            let small = theme.mono(theme.small_size);
            ui.label(
                RichText::new(clock(at))
                    .font(small.clone())
                    .color(theme.muted),
            );
            let bar_width = (ui.available_width() - 44.0).max(40.0);
            let (rect, response) =
                ui.allocate_exact_size(vec2(bar_width, 14.0), Sense::click_and_drag());
            let line = Rect::from_center_size(rect.center(), vec2(rect.width(), 4.0));
            ui.painter().rect_filled(line, 2.0, theme.hover);
            let fraction = if length > 0.0 {
                (at / length) as f32
            } else {
                0.0
            };
            let filled =
                Rect::from_min_size(line.min, vec2(line.width() * fraction, line.height()));
            let active = response.hovered() || response.dragged();
            ui.painter().rect_filled(
                filled,
                2.0,
                if active {
                    theme.text
                } else {
                    theme.border_active
                },
            );
            if active {
                ui.painter()
                    .circle_filled(pos2(filled.right(), line.center().y), 6.0, theme.text);
            }
            if (response.clicked() || response.drag_stopped())
                && let Some(pointer) = response.interact_pointer_pos()
            {
                let t = f64::from((pointer.x - rect.left()) / rect.width());
                self.seek(t, cx);
            }
            ui.label(RichText::new(clock(length)).font(small).color(theme.muted));
        });
        if playing || self.loading.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(250));
        }
    }

    /// The volume, live while dragging, one undo step on release.
    fn volume_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        let stored = cx.rice().get_f64("audio.volume").unwrap_or(0.8);
        let mut volume = self.volume_drag.unwrap_or(stored);
        ui.horizontal(|ui| {
            let icon = if volume <= 0.001 {
                "\u{1F507}"
            } else {
                "\u{1F50A}"
            };
            if icon_button(
                ui,
                icon,
                false,
                26.0,
                if volume <= 0.001 { "Unmute" } else { "Mute" },
                &theme,
            ) {
                let target = if volume <= 0.001 { 0.8 } else { 0.0 };
                set_rice(cx, "audio.volume", RiceValue::Float(target));
                return;
            }
            let response = ui.add(egui::Slider::new(&mut volume, 0.0..=1.0).show_value(false));
            if response.dragged() || response.changed() {
                self.volume_drag = Some(volume);
                if let Some(sink) = cx.audio.as_deref_mut() {
                    sink.set_volume(volume as f32);
                }
            }
            if (response.drag_stopped() || (response.changed() && !response.dragged()))
                && let Some(volume) = self.volume_drag.take()
                && (volume - stored).abs() > f64::EPSILON
            {
                set_rice(
                    cx,
                    "audio.volume",
                    RiceValue::Float((volume * 100.0).round() / 100.0),
                );
            }
        });
    }

    fn tabs_ui(&mut self, ui: &mut Ui, theme: &Theme) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 4.0);
            for (label, page) in Page::TABS {
                if widgets::pill_button(ui, label, self.page.tab() == label, theme).clicked() {
                    self.page = page;
                }
            }
        });
    }

    /// The songs of `paths` as rows: number, title and artist, album, length.
    #[allow(clippy::too_many_lines, reason = "one row and its menu")]
    fn song_rows(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        paths: &[PathBuf],
        editing: bool,
    ) -> Option<RowAction> {
        let theme = cx.theme.clone();
        let wide = ui.available_width() > 460.0;
        let mut action = None;
        let playing_path = self.current_path().cloned();
        let playlists: Vec<(usize, String)> = self
            .library
            .collections
            .iter()
            .enumerate()
            .filter(|(_, collection)| collection.kind == Kind::Playlist)
            .map(|(index, collection)| (index, collection.name.clone()))
            .collect();
        for (index, path) in paths.iter().enumerate() {
            let song = self.library.song(path);
            let title =
                song.map_or_else(|| crate::util::file_name(path), |song| song.title.clone());
            let by = song.map_or("", |song| song.by()).to_owned();
            let album = song.map_or("", |song| song.album.as_str()).to_owned();
            let seconds = song.map_or(0.0, |song| song.seconds);
            let row_height = theme.ui_size * 2.6;
            let (rect, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), row_height), Sense::click());
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let here = playing_path.as_ref() == Some(path);
            if here {
                ui.painter().rect_filled(rect, 4.0, theme.active);
            } else if response.hovered() {
                ui.painter().rect_filled(rect, 4.0, theme.hover);
            }
            let painter = ui.painter().with_clip_rect(rect);
            let left = rect.left() + 8.0;
            painter.text(
                pos2(left, rect.center().y),
                Align2::LEFT_CENTER,
                if here {
                    "\u{25B6}".to_owned()
                } else {
                    (index + 1).to_string()
                },
                theme.mono(theme.small_size),
                theme.muted,
            );
            let mut right = rect.right() - 34.0;
            let placed = painter.text(
                pos2(right, rect.center().y),
                Align2::RIGHT_CENTER,
                if seconds > 0.0 {
                    clock(seconds)
                } else {
                    String::new()
                },
                theme.mono(theme.small_size),
                theme.muted,
            );
            right = placed.left() - 10.0;
            if wide && !album.is_empty() {
                let column = rect.left() + rect.width() * 0.55;
                ui.painter()
                    .with_clip_rect(Rect::from_min_max(
                        pos2(column, rect.top()),
                        pos2(right, rect.bottom()),
                    ))
                    .text(
                        pos2(column, rect.center().y),
                        Align2::LEFT_CENTER,
                        &album,
                        theme.font(theme.small_size),
                        theme.muted,
                    );
                right = column - 10.0;
            }
            let text_clip =
                Rect::from_min_max(pos2(left + 26.0, rect.top()), pos2(right, rect.bottom()));
            let clipped = ui.painter().with_clip_rect(text_clip.intersect(rect));
            clipped.text(
                pos2(left + 28.0, rect.center().y - theme.small_size * 0.6),
                Align2::LEFT_CENTER,
                &title,
                theme.font(theme.ui_size),
                theme.text,
            );
            clipped.text(
                pos2(left + 28.0, rect.center().y + theme.ui_size * 0.55),
                Align2::LEFT_CENTER,
                &by,
                theme.font(theme.small_size),
                theme.muted,
            );
            // The song's menu, on the + button or a right-click.
            let menu = |ui: &mut Ui, action: &mut Option<RowAction>| {
                if ui.button("Play Next").clicked() {
                    *action = Some(RowAction::Queue(path.clone(), true));
                    ui.close();
                }
                if ui.button("Add to Queue").clicked() {
                    *action = Some(RowAction::Queue(path.clone(), false));
                    ui.close();
                }
                ui.menu_button("Add to Playlist", |ui| {
                    for (collection, name) in &playlists {
                        if ui.button(name).clicked() {
                            *action = Some(RowAction::AddTo(path.clone(), *collection));
                            ui.close();
                        }
                    }
                    if ui.button("New Playlist").clicked() {
                        *action = Some(RowAction::NewPlaylist(path.clone()));
                        ui.close();
                    }
                });
                let liked = self.library.is_liked(path);
                if ui.button(if liked { "Unlike" } else { "Like" }).clicked() {
                    *action = Some(RowAction::Like(path.clone()));
                    ui.close();
                }
                if ui.button("Show in Explorer").clicked() {
                    *action = Some(RowAction::Show(path.clone()));
                    ui.close();
                }
                if editing {
                    if ui.button("Move Up").clicked() {
                        *action = Some(RowAction::Move(index, true));
                        ui.close();
                    }
                    if ui.button("Move Down").clicked() {
                        *action = Some(RowAction::Move(index, false));
                        ui.close();
                    }
                    if ui.button("Take Out of This List").clicked() {
                        *action = Some(RowAction::Drop(index));
                        ui.close();
                    }
                }
                if ui.button("Remove from Library").clicked() {
                    *action = Some(RowAction::Remove(path.clone()));
                    ui.close();
                }
            };
            let plus = Rect::from_center_size(
                pos2(rect.right() - 16.0, rect.center().y),
                vec2(24.0, 24.0),
            );
            let mut plus_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(plus)
                    .layout(Layout::left_to_right(Align::Center)),
            );
            plus_ui.menu_button(RichText::new("+").color(theme.muted), |ui| {
                menu(ui, &mut action)
            });
            response.context_menu(|ui| menu(ui, &mut action));
            if response.clicked() {
                action = Some(RowAction::Play(index));
            }
        }
        action
    }

    fn act(
        &mut self,
        action: Option<RowAction>,
        list: &[PathBuf],
        collection: Option<usize>,
        cx: &mut AppletContext<'_>,
    ) {
        let Some(action) = action else {
            return;
        };
        match action {
            RowAction::Play(index) => {
                if self.page == Page::Queue {
                    self.at = Some(index);
                    self.play_current(cx, 0.0);
                } else {
                    self.play_list(list.to_vec(), index, cx);
                }
            }
            RowAction::Queue(path, next) => {
                let at = self.at.map_or(self.queue.len(), |at| at + 1);
                if next {
                    self.queue.insert(at.min(self.queue.len()), path);
                } else {
                    self.queue.push(path);
                }
                if self.at.is_none() {
                    self.at = Some(0);
                }
                cx.notify(if next {
                    "Plays next"
                } else {
                    "Added to the queue"
                });
            }
            RowAction::AddTo(path, index) => {
                if let Some(collection) = self.library.collections.get_mut(index) {
                    collection.songs.push(path);
                    cx.notify(format!("Added to {}", collection.name));
                }
                self.save(cx);
            }
            RowAction::NewPlaylist(path) => {
                let name = self.library.fresh_name("New Playlist");
                self.library.collections.push(Collection {
                    kind: Kind::Playlist,
                    name,
                    artist: String::new(),
                    cover: None,
                    songs: vec![path],
                });
                self.page = Page::Playlist(self.library.collections.len() - 1);
                self.save(cx);
            }
            RowAction::Like(path) => {
                self.library.toggle_like(&path);
                self.save(cx);
            }
            RowAction::Show(path) => {
                cx.request(ShellRequest::Command(format!(
                    "reveal-file:{}",
                    path.display()
                )));
            }
            RowAction::Remove(path) => {
                self.library.remove(&path);
                self.save(cx);
            }
            RowAction::Move(index, up) => {
                let target = if up {
                    index.checked_sub(1)
                } else {
                    Some(index + 1)
                };
                if let Some(target) = target {
                    match (self.page.clone(), collection) {
                        (Page::Queue, _) if target < self.queue.len() => {
                            self.queue.swap(index, target);
                            if self.at == Some(index) {
                                self.at = Some(target);
                            } else if self.at == Some(target) {
                                self.at = Some(index);
                            }
                        }
                        (_, Some(collection)) => {
                            if let Some(collection) = self.library.collections.get_mut(collection)
                                && target < collection.songs.len()
                            {
                                collection.songs.swap(index, target);
                            }
                            self.save(cx);
                        }
                        _ => {}
                    }
                }
            }
            RowAction::Drop(index) => match (self.page.clone(), collection) {
                (Page::Queue, _) if index < self.queue.len() => {
                    self.queue.remove(index);
                    match self.at {
                        Some(at) if at > index => self.at = Some(at - 1),
                        Some(at) if at == index => {
                            self.at =
                                (!self.queue.is_empty()).then_some(at.min(self.queue.len() - 1))
                        }
                        _ => {}
                    }
                }
                (_, Some(collection)) => {
                    if let Some(collection) = self.library.collections.get_mut(collection)
                        && index < collection.songs.len()
                    {
                        collection.songs.remove(index);
                    }
                    self.save(cx);
                }
                _ => {}
            },
        }
    }

    /// Album or playlist cards in a grid; returns the one clicked.
    fn cards(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        items: &[(String, String, Option<PathBuf>, Option<PathBuf>)],
    ) -> Option<usize> {
        let theme = cx.theme.clone();
        let gap = 8.0;
        let side = 132.0_f32.min(ui.available_width());
        let columns = (((ui.available_width() + gap) / (side + gap)).floor() as usize).max(1);
        let mut picked = None;
        for (row_index, row) in items.chunks(columns).enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (offset, (name, by, cover, song)) in row.iter().enumerate() {
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(side, side + 40.0), Sense::click());
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    if response.hovered() {
                        ui.painter().rect_filled(
                            rect.expand(3.0),
                            theme.control_radius,
                            theme.hover,
                        );
                    }
                    let art = Rect::from_min_size(rect.min, vec2(side, side));
                    let file = self.cover_file(song.as_deref(), cover.as_deref(), cx);
                    self.cover_ui(ui, art, file.as_deref(), name, cx);
                    let text = ui.painter().with_clip_rect(rect);
                    text.text(
                        pos2(rect.left(), art.bottom() + 6.0),
                        Align2::LEFT_TOP,
                        name,
                        theme.font(theme.ui_size),
                        theme.text,
                    );
                    text.text(
                        pos2(rect.left(), art.bottom() + 6.0 + theme.ui_size * 1.3),
                        Align2::LEFT_TOP,
                        by,
                        theme.font(theme.small_size),
                        theme.muted,
                    );
                    if response.clicked() {
                        picked = Some(row_index * columns + offset);
                    }
                }
            });
            ui.add_space(gap);
        }
        picked
    }

    /// A playlist's or an album's header: its name, what is in it, and
    /// Play, Shuffle and (for made ones) Rename, Cover and Delete.
    fn header_ui(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        title: &str,
        by: &str,
        songs: &[PathBuf],
        made: Option<usize>,
    ) {
        let theme = cx.theme.clone();
        let seconds: f64 = songs
            .iter()
            .filter_map(|path| self.library.song(path))
            .map(|song| song.seconds)
            .sum();
        if let Some((index, name)) = self.renaming.as_mut()
            && Some(*index) == made
        {
            let mut done = None;
            ui.horizontal(|ui| {
                let response = ui.add(egui::TextEdit::singleline(name).desired_width(220.0));
                if response.lost_focus() {
                    done = Some(ui.input(|input| input.key_pressed(egui::Key::Enter)));
                }
                if widgets::pill_button(ui, "Rename", true, &theme).clicked() {
                    done = Some(true);
                }
            });
            match done {
                Some(true) => {
                    let (index, name) = self.renaming.take().unwrap_or_default();
                    if let Some(collection) = self.library.collections.get_mut(index)
                        && !name.trim().is_empty()
                    {
                        collection.name = name.trim().to_owned();
                    }
                    self.save(cx);
                }
                Some(false) => self.renaming = None,
                None => {}
            }
        } else {
            ui.label(
                RichText::new(title)
                    .size(theme.heading_size)
                    .color(theme.text),
            );
        }
        ui.label(
            RichText::new(format!(
                "{}{} songs, {}",
                if by.is_empty() {
                    String::new()
                } else {
                    format!("{by}, ")
                },
                songs.len(),
                clock(seconds)
            ))
            .size(theme.small_size)
            .color(theme.muted),
        );
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            if widgets::pill_button(ui, "\u{25B6} Play", true, &theme).clicked() {
                if rice_bool(cx, "player.shuffle", false) {
                    set_rice(cx, "player.shuffle", RiceValue::Bool(false));
                }
                self.play_list(songs.to_vec(), 0, cx);
            }
            if widgets::pill_button(ui, "\u{1F500} Shuffle", false, &theme).clicked() {
                set_rice(cx, "player.shuffle", RiceValue::Bool(true));
                let first = (self.random() as usize) % songs.len().max(1);
                self.play_list(songs.to_vec(), first, cx);
            }
            if let Some(index) = made {
                if widgets::pill_button(ui, "Rename", false, &theme).clicked() {
                    self.renaming = Some((index, title.to_owned()));
                }
                if widgets::pill_button(ui, "Cover", false, &theme)
                    .on_hover_text("Pick a picture for its cover")
                    .clicked()
                {
                    cx.request(ShellRequest::PickFile(
                        FilePick::open(
                            Some(AppletId::new(ID).expect("a valid id")),
                            &format!("cover:{index}"),
                            "Choose a Cover",
                        )
                        .extensions(&["png", "jpg", "jpeg", "jxl"]),
                    ));
                }
                if widgets::pill_button(ui, "Delete", false, &theme).clicked() {
                    self.library.collections.remove(index);
                    self.page = Page::Playlists;
                    self.save(cx);
                }
            }
        });
        ui.add_space(theme.padding);
    }

    /// The page the tabs or the sidebar chose.
    #[allow(clippy::too_many_lines, reason = "one match over the pages")]
    fn page_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        if self.reading > 0 {
            ui.label(
                RichText::new(format!("Adding songs... {} read", self.read_count))
                    .size(theme.small_size)
                    .color(theme.muted),
            );
        }
        let search = self.search.trim().to_lowercase();
        let found = |song: &library::Song| {
            search.is_empty()
                || search.split_whitespace().all(|word| {
                    song.title.to_lowercase().contains(word)
                        || song.artist.to_lowercase().contains(word)
                        || song.album.to_lowercase().contains(word)
                        || song.genre.to_lowercase().contains(word)
                })
        };
        match self.page.clone() {
            Page::Songs | Page::Liked => {
                let liked_only = self.page == Page::Liked;
                let list: Vec<PathBuf> = self
                    .library
                    .songs
                    .iter()
                    .filter(|song| found(song))
                    .filter(|song| !liked_only || self.library.is_liked(&song.path))
                    .map(|song| song.path.clone())
                    .collect();
                if liked_only {
                    self.header_ui(ui, cx, "Liked Songs", "", &list, None);
                }
                if list.is_empty() {
                    let hint = if self.library.songs.is_empty() {
                        "No songs yet. Click Add Music, or pick a song or a folder in the File Manager and click Add to Player."
                    } else if liked_only {
                        "Songs you like (the heart) gather here."
                    } else {
                        "Nothing matches"
                    };
                    ui.add(egui::Label::new(RichText::new(hint).color(theme.muted)).wrap());
                }
                let action = self.song_rows(ui, cx, &list, false);
                self.act(action, &list, None, cx);
            }
            Page::Albums => {
                let albums = self.library.albums();
                if albums.is_empty() {
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                "Albums appear from the songs' tags, or make one with New Album.",
                            )
                            .color(theme.muted),
                        )
                        .wrap(),
                    );
                }
                let items: Vec<_> = albums
                    .iter()
                    .map(|album| {
                        (
                            album.name.clone(),
                            album.artist.clone(),
                            album.cover.clone(),
                            album.songs.first().cloned(),
                        )
                    })
                    .collect();
                if let Some(index) = self.cards(ui, cx, &items) {
                    let album = &albums[index];
                    self.page = Page::Album(album.name.clone(), album.artist.clone());
                }
            }
            Page::Album(name, artist) => {
                let Some(album) = self
                    .library
                    .albums()
                    .into_iter()
                    .find(|album| album.name == name && album.artist == artist)
                else {
                    self.page = Page::Albums;
                    return;
                };
                if widgets::pill_button(ui, "\u{2039} Albums", false, &theme).clicked() {
                    self.page = Page::Albums;
                }
                self.header_ui(ui, cx, &album.name, &album.artist, &album.songs, album.made);
                let action = self.song_rows(ui, cx, &album.songs, album.made.is_some());
                self.act(action, &album.songs, album.made, cx);
            }
            Page::Artists => {
                for (name, songs) in self.library.artists() {
                    let (rect, response) = ui.allocate_exact_size(
                        vec2(ui.available_width(), theme.ui_size * 2.2),
                        Sense::click(),
                    );
                    if response.hovered() {
                        ui.painter().rect_filled(rect, 4.0, theme.hover);
                    }
                    ui.painter().text(
                        rect.left_center() + vec2(8.0, 0.0),
                        Align2::LEFT_CENTER,
                        &name,
                        theme.font(theme.ui_size),
                        theme.text,
                    );
                    ui.painter().text(
                        rect.right_center() - vec2(8.0, 0.0),
                        Align2::RIGHT_CENTER,
                        format!("{} songs", songs.len()),
                        theme.font(theme.small_size),
                        theme.muted,
                    );
                    if response.clicked() {
                        self.page = Page::Artist(name.clone());
                    }
                }
            }
            Page::Artist(name) => {
                if widgets::pill_button(ui, "\u{2039} Artists", false, &theme).clicked() {
                    self.page = Page::Artists;
                }
                let songs = self
                    .library
                    .artists()
                    .into_iter()
                    .find(|(artist, _)| *artist == name)
                    .map(|(_, songs)| songs)
                    .unwrap_or_default();
                self.header_ui(ui, cx, &name, "", &songs, None);
                let action = self.song_rows(ui, cx, &songs, false);
                self.act(action, &songs, None, cx);
            }
            Page::Playlists => {
                ui.horizontal_wrapped(|ui| {
                    if widgets::pill_button(ui, "New Playlist", false, &theme).clicked() {
                        let name = self.library.fresh_name("New Playlist");
                        self.library.collections.push(Collection {
                            kind: Kind::Playlist,
                            name: name.clone(),
                            artist: String::new(),
                            cover: None,
                            songs: Vec::new(),
                        });
                        let index = self.library.collections.len() - 1;
                        self.renaming = Some((index, name));
                        self.page = Page::Playlist(index);
                        self.save(cx);
                    }
                    if widgets::pill_button(ui, "New Album", false, &theme).clicked() {
                        let name = self.library.fresh_name("New Album");
                        self.library.collections.push(Collection {
                            kind: Kind::Album,
                            name: name.clone(),
                            artist: String::new(),
                            cover: None,
                            songs: Vec::new(),
                        });
                        let index = self.library.collections.len() - 1;
                        self.renaming = Some((index, name));
                        self.page = Page::Playlist(index);
                        self.save(cx);
                    }
                });
                ui.add_space(theme.padding * 0.5);
                let items: Vec<_> = self
                    .library
                    .collections
                    .iter()
                    .map(|collection| {
                        let kind = match collection.kind {
                            Kind::Playlist => format!("Playlist, {} songs", collection.songs.len()),
                            Kind::Album => format!("Album, {} songs", collection.songs.len()),
                        };
                        (
                            collection.name.clone(),
                            kind,
                            collection.cover.clone(),
                            collection.songs.first().cloned(),
                        )
                    })
                    .collect();
                if items.is_empty() {
                    ui.label(
                        RichText::new("Make a playlist, then add songs with their + button.")
                            .color(theme.muted),
                    );
                }
                if let Some(index) = self.cards(ui, cx, &items) {
                    self.page = Page::Playlist(index);
                }
            }
            Page::Playlist(index) => {
                let Some(collection) = self.library.collections.get(index).cloned() else {
                    self.page = Page::Playlists;
                    return;
                };
                if widgets::pill_button(ui, "\u{2039} Playlists", false, &theme).clicked() {
                    self.page = Page::Playlists;
                }
                self.header_ui(
                    ui,
                    cx,
                    &collection.name,
                    &collection.artist,
                    &collection.songs,
                    Some(index),
                );
                if collection.songs.is_empty() {
                    ui.label(
                        RichText::new("Add songs with their + button, from Songs or Albums.")
                            .color(theme.muted),
                    );
                }
                let action = self.song_rows(ui, cx, &collection.songs, true);
                self.act(action, &collection.songs, Some(index), cx);
            }
            Page::Queue => {
                if self.queue.is_empty() {
                    ui.label(
                        RichText::new("The queue is empty. Play a song, or use Add to Queue.")
                            .color(theme.muted),
                    );
                    return;
                }
                if widgets::pill_button(ui, "Clear the Queue", false, &theme).clicked() {
                    let keep = self.current_path().cloned();
                    self.queue = keep.into_iter().collect();
                    self.at = (!self.queue.is_empty()).then_some(0);
                    return;
                }
                let queue = self.queue.clone();
                let action = self.song_rows(ui, cx, &queue, true);
                self.act(action, &queue, None, cx);
            }
        }
    }

    fn add_music_button(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        if widgets::pill_button(ui, "Add Music", false, &theme)
            .on_hover_text("Choose a song, or a folder of them, in the File Manager")
            .clicked()
        {
            let start = cx
                .catalog
                .folders
                .get("home")
                .map(|home| home.join("Music"))
                .filter(|music| music.is_dir());
            cx.request(ShellRequest::PickFile(
                FilePick::open(
                    Some(AppletId::new(ID).expect("a valid id")),
                    "import",
                    "Add Music",
                )
                .extensions(&AUDIO)
                .or_folder()
                .starting_in(start),
            ));
        }
    }
}

impl SueApplet for Player {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "Player", AppletRole::LeftDrawer, "\u{25B6}");
        manifest.summary =
            "Your music: songs, albums, artists and playlists, playing while you work.".to_owned();
        manifest.order = 20;
        manifest
    }

    fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        let toggle = |value| RiceParameter::Bool { value };
        vec![
            RiceDeclaration::new(
                "player.shuffle",
                toggle(false),
                "Play songs in a random order",
            ),
            RiceDeclaration::new(
                "player.repeat",
                RiceParameter::EnumChoice {
                    value: "off".to_owned(),
                    options: ["off", "all", "one"].map(String::from).to_vec(),
                },
                "Repeat the queue, one song, or nothing",
            ),
            RiceDeclaration::new(
                "player.autoplay",
                toggle(true),
                "Go on to the next song when one ends",
            ),
            RiceDeclaration::new(
                "player.normalize",
                toggle(true),
                "Even out loudness between songs",
            ),
            RiceDeclaration::new(
                "player.preload",
                toggle(true),
                "Decode the next song early so it starts at once",
            ),
            RiceDeclaration::new(
                "player.covers",
                toggle(true),
                "Show cover pictures kept next to the songs",
            ),
        ]
    }

    fn drawer_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.load_library(cx);
        let theme = cx.theme.clone();
        self.now_ui(ui, cx, 56.0);
        ui.add_space(theme.padding * 0.5);
        self.controls_ui(ui, cx);
        self.volume_ui(ui, cx);
        ui.add_space(theme.padding * 0.5);
        ui.horizontal_wrapped(|ui| {
            self.add_music_button(ui, cx);
            if widgets::pill_button(ui, "Open the Large Player", false, &theme).clicked() {
                cx.request(ShellRequest::OpenWindow(
                    AppletId::new(ID).expect("a valid id"),
                ));
            }
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .desired_width(ui.available_width())
                .hint_text("Search songs, artists, albums"),
        );
        self.tabs_ui(ui, &theme);
        ui.separator();
        self.page_ui(ui, cx);
    }

    fn window_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.load_library(cx);
        let theme = cx.theme.clone();
        let area = ui.max_rect().shrink(theme.padding * 1.5);
        let bar_height = 92.0;
        let bar = Rect::from_min_max(pos2(area.left(), area.bottom() - bar_height), area.max);
        let body = Rect::from_min_max(area.min, pos2(area.right(), bar.top() - theme.padding));
        let sidebar = Rect::from_min_size(body.min, vec2(180.0, body.height()));
        let main = Rect::from_min_max(
            pos2(sidebar.right() + theme.padding * 1.5, body.top()),
            body.max,
        );

        // The sidebar: the library's parts, then the playlists.
        let mut side = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(sidebar)
                .layout(Layout::top_down(Align::Min)),
        );
        ScrollArea::vertical()
            .id_salt("sue-player-side")
            .auto_shrink([false, false])
            .show(&mut side, |ui| {
                widgets::heading(ui, "Library", &theme);
                for (label, page) in Page::TABS {
                    if widgets::pill_button(
                        ui,
                        label,
                        self.page.tab() == label && !matches!(self.page, Page::Playlist(_)),
                        &theme,
                    )
                    .clicked()
                    {
                        self.page = page;
                    }
                }
                ui.add_space(theme.padding);
                widgets::heading(ui, "Playlists", &theme);
                let names: Vec<(usize, String)> = self
                    .library
                    .collections
                    .iter()
                    .enumerate()
                    .map(|(index, collection)| (index, collection.name.clone()))
                    .collect();
                for (index, name) in names {
                    if widgets::pill_button(ui, &name, self.page == Page::Playlist(index), &theme)
                        .clicked()
                    {
                        self.page = Page::Playlist(index);
                    }
                }
                ui.add_space(theme.padding);
                self.add_music_button(ui, cx);
            });
        ui.painter().vline(
            sidebar.right() + theme.padding * 0.75,
            sidebar.y_range(),
            theme.hairline(),
        );

        // The page.
        let mut page = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(main)
                .layout(Layout::top_down(Align::Min)),
        );
        page.add(
            egui::TextEdit::singleline(&mut self.search)
                .desired_width(main.width())
                .hint_text("Search songs, artists, albums"),
        );
        page.add_space(theme.padding * 0.5);
        ScrollArea::vertical()
            .id_salt("sue-player-page")
            .auto_shrink([false, false])
            .show(&mut page, |ui| self.page_ui(ui, cx));

        // The bar at the bottom: the song, the controls, the volume.
        ui.painter().hline(
            bar.x_range(),
            bar.top() - theme.padding * 0.5,
            theme.hairline(),
        );
        let third = bar.width() / 3.0;
        let mut left = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_size(bar.min, vec2(third, bar.height())))
                .layout(Layout::left_to_right(Align::Center)),
        );
        left.vertical(|ui| self.now_ui(ui, cx, 60.0));
        let mut middle = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_size(
                    bar.min + vec2(third, 0.0),
                    vec2(third, bar.height()),
                ))
                .layout(Layout::top_down(Align::Min)),
        );
        self.controls_ui(&mut middle, cx);
        let mut right = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_size(
                    bar.min + vec2(third * 2.0 + theme.padding * 2.0, bar.height() * 0.3),
                    vec2(third - theme.padding * 2.0, bar.height()),
                ))
                .layout(Layout::top_down(Align::Min)),
        );
        self.volume_ui(&mut right, cx);
        ui.painter()
            .rect_stroke(bar.expand(2.0), 0.0, Stroke::NONE, egui::StrokeKind::Inside);
    }

    fn tick(&mut self, cx: &mut AppletContext<'_>) -> Tick {
        let changed = self.advance(cx);
        let arrived = self.covers.collect();
        let busy =
            self.playing || self.loading.is_some() || self.reading > 0 || self.covers.pending();
        Tick {
            repaint: changed || arrived,
            wake: busy.then(|| std::time::Instant::now() + Duration::from_millis(200)),
            ..Tick::default()
        }
    }

    fn picked(&mut self, pick: &FilePick, path: &Path, cx: &mut AppletContext<'_>) {
        if pick.purpose == "import" {
            self.import(path.to_path_buf(), cx);
            cx.notify(format!(
                "Adding {} to the Player",
                crate::util::file_name(path)
            ));
        } else if let Some(index) = pick
            .purpose
            .strip_prefix("cover:")
            .and_then(|index| index.parse::<usize>().ok())
        {
            self.load_library(cx);
            if let Some(collection) = self.library.collections.get_mut(index) {
                collection.cover = Some(path.to_path_buf());
            }
            self.save(cx);
        }
    }

    fn window_title(&self) -> Option<String> {
        Some("Player".to_owned())
    }

    fn commands(&self) -> Vec<sue_wm::PaletteEntry> {
        [
            ("Player: Play or Pause", "player.toggle"),
            ("Player: Next Song", "player.next"),
            ("Player: Previous Song", "player.previous"),
            ("Player: Shuffle On or Off", "player.shuffle"),
            ("Player: Add Music", "player.add"),
            ("Player: Open the Large Player", "player.window"),
        ]
        .into_iter()
        .map(|(label, command)| sue_wm::PaletteEntry {
            label: label.to_owned(),
            command: command.to_owned(),
            hint: String::new(),
        })
        .collect()
    }

    fn command(&mut self, name: &str, cx: &mut AppletContext<'_>) -> bool {
        self.load_library(cx);
        match name {
            "player.toggle" => self.toggle(cx),
            "player.next" => self.step(true, true, cx),
            "player.previous" => self.step(false, true, cx),
            "player.shuffle" => {
                let shuffle = rice_bool(cx, "player.shuffle", false);
                set_rice(cx, "player.shuffle", RiceValue::Bool(!shuffle));
            }
            "player.add" => {
                let start = cx
                    .catalog
                    .folders
                    .get("home")
                    .map(|home| home.join("Music"))
                    .filter(|music| music.is_dir());
                cx.request(ShellRequest::PickFile(
                    FilePick::open(
                        Some(AppletId::new(ID).expect("a valid id")),
                        "import",
                        "Add Music",
                    )
                    .extensions(&AUDIO)
                    .or_folder()
                    .starting_in(start),
                ));
            }
            "player.window" => {
                cx.request(ShellRequest::OpenWindow(
                    AppletId::new(ID).expect("a valid id"),
                ));
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_cycles_off_all_one() {
        assert_eq!(Repeat::from_rice(Repeat::Off.next()), Repeat::All);
        assert_eq!(Repeat::from_rice(Repeat::All.next()), Repeat::One);
        assert_eq!(Repeat::from_rice(Repeat::One.next()), Repeat::Off);
        assert_eq!(Page::Album("A".into(), "B".into()).tab(), "Albums");
    }
}
