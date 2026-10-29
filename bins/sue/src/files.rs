//! The File Manager: browse folders in tabs, as a list or as cards with
//! picture thumbnails, search a folder or everything under it, keep saved
//! folders, and see, copy or type any path. It lives in the left drawer and,
//! larger, in a window; that window is also how every studio and the shell
//! ask for a file to open or a place to save (`ShellRequest::PickFile`).
//!
//! Dot files and the Windows legacy junctions (`NetHood`, `Start Menu` and
//! the like) are hidden unless `files.show_hidden` is on.

pub(crate) mod thumbs;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, SystemTime};

use egui::{
    Align, Align2, Color32, Key, Layout, Rect, RichText, ScrollArea, Sense, Stroke, StrokeKind, Ui,
    Vec2, pos2, vec2,
};
use sue_platform::{DirEntryInfo, list_dir};
use sue_rice::{RiceDeclaration, RiceParameter, RiceValue, SetRiceValue};
use sue_wm::{
    AppletContext, AppletManifest, AppletRole, FilePick, ShellRequest, SueApplet, Theme, Tick,
    widgets,
};

use crate::util::{file_name, human_bytes};

/// The applet id.
pub const ID: &str = "files";

/// Folders in the home folder offered as places.
const JUMPS: [&str; 6] = [
    "Desktop",
    "Documents",
    "Downloads",
    "Pictures",
    "Music",
    "Videos",
];

/// Most results a search under a folder collects.
const DEEP_RESULTS: usize = 2_000;

/// Most entries a search under a folder looks at.
const DEEP_VISITS: usize = 200_000;

/// What a file is, for its icon, its card and the Kind sort.
fn kind_of(entry: &DirEntryInfo) -> &'static str {
    if entry.is_dir {
        return "Folder";
    }
    let extension = Path::new(&entry.name)
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "jxl" | "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "svg" => {
            "Picture"
        }
        "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "opus" => "Audio",
        "mp4" | "mkv" | "mov" | "webm" | "avi" => "Video",
        "sue" => "Sue Project",
        "txt" | "md" | "pdf" | "doc" | "docx" | "odt" | "rtf" => "Document",
        "rs" | "py" | "js" | "ts" | "html" | "css" | "json" | "toml" | "c" | "cpp" | "h" | "sh"
        | "suerice" => "Code",
        "zip" | "7z" | "rar" | "tar" | "gz" | "xz" => "Archive",
        _ => "File",
    }
}

/// The icon for an entry.
fn icon_for(entry: &DirEntryInfo) -> &'static str {
    match kind_of(entry) {
        "Folder" => "\u{1F4C1}",
        "Picture" => "\u{1F5BC}",
        "Audio" => "\u{1F3B5}",
        "Video" => "\u{1F3AC}",
        "Code" => "\u{2328}",
        "Archive" => "\u{1F4E6}",
        _ => "\u{1F4C4}",
    }
}

/// How long ago `modified` was: `just now`, `5 min ago`, `3 days ago`, or
/// the date (UTC) for anything older than a month.
fn when(modified: Option<SystemTime>, now: SystemTime) -> String {
    let Some(modified) = modified else {
        return String::new();
    };
    let Ok(age) = now.duration_since(modified) else {
        return "just now".to_owned();
    };
    let minutes = age.as_secs() / 60;
    match minutes {
        0 => "just now".to_owned(),
        1..60 => format!("{minutes} min ago"),
        60..1_440 => format!("{} h ago", minutes / 60),
        1_440..2_880 => "yesterday".to_owned(),
        2_880..43_200 => format!("{} days ago", minutes / 1_440),
        _ => {
            let days = modified
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |since| since.as_secs() / 86_400) as i64;
            let (year, month, day) = civil(days);
            format!("{year}-{month:02}-{day:02}")
        }
    }
}

/// Year, month and day of `days` since 1970-01-01.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// `true` when every word of `query` is in `name`, ignoring case.
fn matches(name: &str, query: &str) -> bool {
    let name = name.to_lowercase();
    query
        .split_whitespace()
        .all(|word| name.contains(&word.to_lowercase()))
}

/// How the File Manager shows a folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    /// Rows: icon, name, when, size.
    List,
    /// Rows with columns that sort when clicked.
    Details,
    /// Small thumbnails with their names.
    Icons,
    /// Large thumbnails with their names and sizes.
    Cards,
}

impl View {
    const ALL: [(Self, &'static str, &'static str); 4] = [
        (Self::List, "list", "List"),
        (Self::Details, "details", "Details"),
        (Self::Icons, "icons", "Icons"),
        (Self::Cards, "cards", "Cards"),
    ];
}

/// The filters a pick gives the system's dialog: each kind of file it
/// offers, else its extensions.
pub(crate) fn filters_of(pick: &FilePick) -> Vec<sue_platform::FileFilter> {
    if !pick.types.is_empty() {
        return pick
            .types
            .iter()
            .map(|kind| sue_platform::FileFilter {
                name: kind.name.clone(),
                extensions: kind.extensions.clone(),
            })
            .collect();
    }
    if pick.extensions.is_empty() {
        Vec::new()
    } else {
        vec![sue_platform::FileFilter {
            name: "Files".to_owned(),
            extensions: pick.extensions.clone(),
        }]
    }
}

/// Draws a five-pointed star, filled or as an outline.
fn star(painter: &egui::Painter, center: egui::Pos2, radius: f32, filled: bool, color: Color32) {
    let points: Vec<egui::Pos2> = (0..10)
        .map(|index| {
            let r = if index % 2 == 0 {
                radius
            } else {
                radius * 0.42
            };
            let angle = -std::f32::consts::FRAC_PI_2 + index as f32 * std::f32::consts::PI / 5.0;
            center + vec2(angle.cos(), angle.sin()) * r
        })
        .collect();
    if filled {
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(center, color);
        for point in &points {
            mesh.colored_vertex(*point, color);
        }
        for index in 0..10_u32 {
            mesh.add_triangle(0, 1 + index, 1 + (index + 1) % 10);
        }
        painter.add(egui::Shape::mesh(mesh));
    } else {
        painter.add(egui::Shape::closed_line(points, Stroke::new(1.3, color)));
    }
}

/// A button showing a star: filled when the folder is a favorite.
fn star_button(ui: &mut Ui, on: bool, theme: &Theme) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(28.0, 24.0), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, theme.control_radius, theme.hover);
    }
    let color = if on {
        Color32::from_rgb(0xf2, 0xc0, 0x3a)
    } else if response.hovered() {
        theme.text
    } else {
        theme.muted
    };
    star(ui.painter(), rect.center(), 8.0, on, color);
    response
}

/// What the File Manager sorts by (folders always come first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sort {
    Name,
    Modified,
    Size,
    Kind,
}

impl Sort {
    const ALL: [(Self, &'static str, &'static str); 4] = [
        (Self::Name, "name", "Name"),
        (Self::Modified, "modified", "Date modified"),
        (Self::Size, "size", "Size"),
        (Self::Kind, "kind", "Kind"),
    ];

    fn from_rice(value: &str) -> Self {
        Self::ALL
            .iter()
            .find(|(_, key, _)| *key == value)
            .map_or(Self::Name, |(sort, _, _)| *sort)
    }

    #[cfg(test)]
    fn key(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(sort, _, _)| *sort == self)
            .map_or("name", |(_, key, _)| key)
    }

    fn label(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(sort, _, _)| *sort == self)
            .map_or("Name", |(_, _, label)| label)
    }
}

/// Sorts `entries`: folders first, then by `sort` (newest and largest
/// first), then by name.
fn sort_entries(entries: &mut [DirEntryInfo], sort: Sort) {
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| match sort {
                Sort::Name => std::cmp::Ordering::Equal,
                Sort::Modified => b.modified.cmp(&a.modified),
                Sort::Size => b.size.cmp(&a.size),
                Sort::Kind => kind_of(a).cmp(kind_of(b)),
            })
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// Saved folders as `files.saved` keeps them: paths joined by `|`.
fn saved_folders(rice: &sue_rice::RiceSchema) -> Vec<PathBuf> {
    rice.get_str("files.saved")
        .unwrap_or("")
        .split('|')
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// One open folder, with its own way back and forward.
#[derive(Debug)]
struct Tab {
    dir: PathBuf,
    entries: Vec<DirEntryInfo>,
    error: Option<String>,
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
}

impl Tab {
    fn new(dir: PathBuf, show_hidden: bool) -> Self {
        let mut tab = Self {
            dir: dir.clone(),
            entries: Vec::new(),
            error: None,
            back: Vec::new(),
            forward: Vec::new(),
        };
        tab.load(dir, show_hidden);
        tab
    }

    fn load(&mut self, dir: PathBuf, show_hidden: bool) {
        match list_dir(&dir, show_hidden) {
            Ok(entries) => {
                self.entries = entries;
                self.error = None;
            }
            Err(error) => {
                self.entries.clear();
                self.error = Some(error.to_string());
            }
        }
        self.dir = dir;
    }

    /// Goes to `dir`, remembering where it was.
    fn go(&mut self, dir: PathBuf, show_hidden: bool) {
        if dir != self.dir {
            self.back.push(self.dir.clone());
            self.forward.clear();
        }
        self.load(dir, show_hidden);
    }

    fn title(&self) -> String {
        let name = file_name(&self.dir);
        if name.is_empty() {
            self.dir.display().to_string()
        } else {
            name
        }
    }
}

/// A search through every folder under one, on a background thread.
#[derive(Debug)]
struct DeepSearch {
    root: PathBuf,
    query: String,
    results: Vec<DirEntryInfo>,
    receiver: Receiver<Option<DirEntryInfo>>,
    cancel: Arc<AtomicBool>,
    done: bool,
}

impl DeepSearch {
    fn start(root: PathBuf, query: String, show_hidden: bool) -> Self {
        let (sender, receiver) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&cancel);
        let (from, words) = (root.clone(), query.clone());
        let _ = std::thread::Builder::new()
            .name("sue-file-search".to_owned())
            .spawn(move || {
                let mut queue = std::collections::VecDeque::from([from]);
                let (mut visits, mut found) = (0, 0);
                while let Some(dir) = queue.pop_front() {
                    let Ok(entries) = list_dir(&dir, show_hidden) else {
                        continue;
                    };
                    for entry in entries {
                        visits += 1;
                        if stop.load(Ordering::Relaxed) || visits > DEEP_VISITS {
                            let _ = sender.send(None);
                            return;
                        }
                        if entry.is_dir {
                            queue.push_back(entry.path.clone());
                        }
                        if matches(&entry.name, &words) {
                            found += 1;
                            if sender.send(Some(entry)).is_err() || found >= DEEP_RESULTS {
                                let _ = sender.send(None);
                                return;
                            }
                        }
                    }
                }
                let _ = sender.send(None);
            });
        Self {
            root,
            query,
            results: Vec::new(),
            receiver,
            cancel,
            done: false,
        }
    }

    /// Takes in new results. Returns `true` if any came.
    fn collect(&mut self) -> bool {
        let mut any = false;
        while let Ok(message) = self.receiver.try_recv() {
            any = true;
            match message {
                Some(entry) => self.results.push(entry),
                None => self.done = true,
            }
        }
        any
    }
}

impl Drop for DeepSearch {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// What a click on an entry asked for.
enum Action {
    Select(PathBuf),
    Open(DirEntryInfo),
}

/// The File Manager.
#[derive(Debug, Default)]
pub struct Files {
    tabs: Vec<Tab>,
    current: usize,
    path_text: String,
    path_editing: bool,
    search: String,
    deep: bool,
    deep_search: Option<DeepSearch>,
    selected: Option<PathBuf>,
    renaming: Option<String>,
    new_folder: Option<String>,
    pick: Option<FilePick>,
    save_name: String,
    thumbs: thumbs::Thumbs,
    root_setting: Option<String>,
    show_hidden: bool,
    places_open: bool,
    /// The settings hand file picks to the system's own dialog.
    system_picks: bool,
}

impl Files {
    fn project(cx: &AppletContext<'_>) -> PathBuf {
        let riced = cx
            .rice()
            .get_str("general.workspace_dir")
            .unwrap_or("")
            .trim();
        if riced.is_empty() {
            std::env::current_dir().unwrap_or_else(|_| Self::home(cx))
        } else {
            PathBuf::from(riced)
        }
    }

    fn home(cx: &AppletContext<'_>) -> PathBuf {
        cx.catalog.folders.get("home").cloned().unwrap_or_default()
    }

    fn tab(&mut self) -> &mut Tab {
        let current = self.current.min(self.tabs.len().saturating_sub(1));
        &mut self.tabs[current]
    }

    fn dir(&self) -> Option<&Path> {
        self.tabs.get(self.current).map(|tab| tab.dir.as_path())
    }

    /// Goes to `dir` in the current tab.
    fn go(&mut self, dir: PathBuf) {
        let hidden = self.show_hidden;
        self.tab().go(dir, hidden);
        self.after_move();
    }

    fn after_move(&mut self) {
        self.path_text = self
            .dir()
            .map(|dir| dir.display().to_string())
            .unwrap_or_default();
        self.selected = None;
        self.renaming = None;
        self.deep_search = None;
    }

    fn reload(&mut self) {
        let hidden = self.show_hidden;
        let tab = self.tab();
        let dir = tab.dir.clone();
        tab.load(dir, hidden);
    }

    /// Follows `general.workspace_dir` and `files.show_hidden`.
    fn sync(&mut self, cx: &AppletContext<'_>) {
        let setting = cx
            .rice()
            .get_str("general.workspace_dir")
            .unwrap_or("")
            .trim()
            .to_owned();
        let hidden = cx.rice().get_bool("files.show_hidden").unwrap_or(false);
        if self.tabs.is_empty() || self.root_setting.as_deref() != Some(setting.as_str()) {
            self.root_setting = Some(setting.clone());
            self.show_hidden = hidden;
            let root = if setting.is_empty() {
                Self::home(cx)
            } else {
                PathBuf::from(setting)
            };
            if self.tabs.is_empty() {
                self.tabs.push(Tab::new(root, hidden));
                self.current = 0;
            } else {
                self.tab().go(root, hidden);
            }
            self.after_move();
        } else if hidden != self.show_hidden {
            self.show_hidden = hidden;
            for tab in &mut self.tabs {
                let dir = tab.dir.clone();
                tab.load(dir, hidden);
            }
        }
        if !self.path_editing {
            self.path_text = self
                .dir()
                .map(|dir| dir.display().to_string())
                .unwrap_or_default();
        }
    }

    fn view(cx: &AppletContext<'_>) -> View {
        let key = cx.rice().get_str("files.view").unwrap_or("list");
        View::ALL
            .iter()
            .find(|(_, known, _)| *known == key)
            .map_or(View::List, |(view, _, _)| *view)
    }

    fn set_rice(cx: &mut AppletContext<'_>, path: &str, value: RiceValue) {
        if let Err(error) = cx.edit_rice(SetRiceValue::new(path, value)) {
            cx.fail(format!("Could not change {path}: {error}"));
        }
    }

    /// The places: the project, home and its usual folders, the drives,
    /// then the saved folders.
    fn places(cx: &AppletContext<'_>) -> Vec<(String, PathBuf)> {
        let home = Self::home(cx);
        let mut places = vec![
            ("Project".to_owned(), Self::project(cx)),
            ("Home".to_owned(), home.clone()),
        ];
        places.extend(
            JUMPS
                .iter()
                .map(|name| ((*name).to_owned(), home.join(name)))
                .filter(|(_, path)| path.is_dir()),
        );
        places.extend(
            sue_platform::roots()
                .into_iter()
                .map(|root| (root.display().to_string(), root)),
        );
        places
    }

    /// The entries to show: the folder's, filtered by the search, or what
    /// the search under it found.
    fn shown(&self) -> Vec<DirEntryInfo> {
        if self.deep
            && let Some(search) = &self.deep_search
        {
            return search.results.clone();
        }
        let Some(tab) = self.tabs.get(self.current) else {
            return Vec::new();
        };
        tab.entries
            .iter()
            .filter(|entry| self.search.trim().is_empty() || matches(&entry.name, &self.search))
            .filter(|entry| {
                // While picking, show folders and the files it takes.
                entry.is_dir
                    || self
                        .pick
                        .as_ref()
                        .is_none_or(|pick| pick.takes(&entry.path))
            })
            .cloned()
            .collect()
    }

    // ----- The parts of the File Manager --------------------------------------

    fn tabs_ui(&mut self, ui: &mut Ui, theme: &Theme) {
        let mut pick = None;
        let mut close = None;
        let mut add = false;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 4.0);
            for (index, tab) in self.tabs.iter().enumerate() {
                let here = index == self.current;
                let title = tab.title();
                let short: String = if title.chars().count() > 16 {
                    format!("{}...", title.chars().take(14).collect::<String>())
                } else {
                    title.clone()
                };
                if widgets::pill_button(ui, &short, here, theme)
                    .on_hover_text(tab.dir.display().to_string())
                    .clicked()
                {
                    pick = Some(index);
                }
                if self.tabs.len() > 1
                    && ui
                        .add(
                            egui::Button::new(RichText::new("\u{00D7}").color(theme.muted))
                                .frame(false)
                                .small(),
                        )
                        .on_hover_text("Close this tab")
                        .clicked()
                {
                    close = Some(index);
                }
            }
            if ui
                .add(egui::Button::new(RichText::new("+").color(theme.text)).frame(false))
                .on_hover_text("A new tab on this folder")
                .clicked()
            {
                add = true;
            }
        });
        if let Some(index) = pick {
            self.current = index;
            self.after_move();
        }
        if let Some(index) = close {
            self.tabs.remove(index);
            if self.current >= index && self.current > 0 {
                self.current -= 1;
            }
            self.after_move();
        }
        if add && let Some(dir) = self.dir().map(Path::to_path_buf) {
            self.tabs.push(Tab::new(dir, self.show_hidden));
            self.current = self.tabs.len() - 1;
            self.after_move();
        }
    }

    fn path_ui(&mut self, ui: &mut Ui, theme: &Theme) {
        let Some(tab) = self.tabs.get(self.current) else {
            return;
        };
        let (can_back, can_forward) = (!tab.back.is_empty(), !tab.forward.is_empty());
        let parent = tab.dir.parent().map(Path::to_path_buf);
        let mut go = None;
        let mut step = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let nav = |ui: &mut Ui, text: &str, enabled: bool, tip: &str| {
                ui.add_enabled(
                    enabled,
                    egui::Button::new(RichText::new(text).color(theme.text)).frame(false),
                )
                .on_hover_text(tip)
                .clicked()
            };
            if nav(ui, "\u{2039}", can_back, "Back") {
                step = Some(true);
            }
            if nav(ui, "\u{203A}", can_forward, "Forward") {
                step = Some(false);
            }
            if nav(ui, "Up", parent.is_some(), "Up one folder") {
                go.clone_from(&parent);
            }
            let copy_width = 44.0;
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.path_text)
                    .desired_width((ui.available_width() - copy_width).max(60.0))
                    .font(theme.font(theme.small_size + 1.0))
                    .hint_text("Type a folder and press Enter"),
            );
            self.path_editing = response.has_focus();
            if response.lost_focus() {
                if ui.input(|input| input.key_pressed(Key::Enter)) {
                    let typed = PathBuf::from(self.path_text.trim());
                    if typed.is_dir() {
                        go = Some(typed);
                    } else if typed.is_file() {
                        go = typed.parent().map(Path::to_path_buf);
                        self.selected = Some(typed);
                    } else {
                        self.path_text = tab.dir.display().to_string();
                    }
                } else {
                    self.path_text = tab.dir.display().to_string();
                }
            }
            if ui
                .add(egui::Button::new(RichText::new("Copy").size(theme.small_size)).frame(false))
                .on_hover_text("Copy this folder's path")
                .clicked()
            {
                ui.ctx().copy_text(tab.dir.display().to_string());
            }
        });
        let hidden = self.show_hidden;
        match step {
            Some(back) => {
                let tab = self.tab();
                let (from, to) = if back {
                    (&mut tab.back, &mut tab.forward)
                } else {
                    (&mut tab.forward, &mut tab.back)
                };
                if let Some(dir) = from.pop() {
                    to.push(tab.dir.clone());
                    tab.load(dir, hidden);
                }
                self.after_move();
            }
            None => {
                if let Some(dir) = go {
                    let keep = self.selected.clone();
                    self.go(dir);
                    self.selected = keep;
                }
            }
        }
    }

    fn tools_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
        let view = Self::view(cx);
        let sort = Sort::from_rice(cx.rice().get_str("files.sort").unwrap_or("name"));
        let saved = saved_folders(cx.rice());
        let here = self.dir().map(Path::to_path_buf);
        let is_saved = here.as_ref().is_some_and(|dir| saved.contains(dir));
        let search = ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .desired_width(ui.available_width())
                .hint_text(if self.deep {
                    "Search everything under this folder"
                } else {
                    "Search this folder"
                }),
        );
        let deep_query = self.deep && !self.search.trim().is_empty();
        if deep_query
            && (search.changed() || self.deep_search.is_none())
            && let Some(dir) = here.clone()
        {
            let restart = self
                .deep_search
                .as_ref()
                .is_none_or(|search| search.query != self.search || search.root != dir);
            if restart {
                self.deep_search = Some(DeepSearch::start(
                    dir,
                    self.search.clone(),
                    self.show_hidden,
                ));
            }
        }
        if !deep_query {
            self.deep_search = None;
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            if widgets::pill_button(ui, "Subfolders", self.deep, theme)
                .on_hover_text("Search every folder under this one too")
                .clicked()
            {
                self.deep = !self.deep;
            }
            let options: Vec<String> = View::ALL
                .iter()
                .map(|(_, _, label)| (*label).to_owned())
                .collect();
            let chosen = View::ALL.iter().position(|(known, _, _)| *known == view);
            if let Some(index) = widgets::segmented(ui, &options, chosen, theme) {
                Self::set_rice(
                    cx,
                    "files.view",
                    RiceValue::Text(View::ALL[index].1.to_owned()),
                );
            }
            if matches!(view, View::Icons | View::Cards) {
                let mut side = cx.rice().get_i64("files.card_size").unwrap_or(112) as f32;
                let response = ui
                    .add(egui::Slider::new(&mut side, 48.0..=320.0).show_value(false))
                    .on_hover_text("Thumbnail size");
                if response.changed() {
                    Self::set_rice(cx, "files.card_size", RiceValue::Int(side.round() as i64));
                }
            }
            egui::ComboBox::from_id_salt("sue-files-sort")
                .selected_text(sort.label())
                .width(110.0)
                .show_ui(ui, |ui| {
                    for (option, key, label) in Sort::ALL {
                        if ui
                            .add(egui::Button::selectable(option == sort, label))
                            .clicked()
                        {
                            Self::set_rice(cx, "files.sort", RiceValue::Text(key.to_owned()));
                        }
                    }
                });
            let reverse = cx.rice().get_bool("files.sort_reverse").unwrap_or(false);
            if ui
                .add(egui::Button::new(if reverse { "\u{2191}" } else { "\u{2193}" }).frame(false))
                .on_hover_text(if reverse {
                    "Reversed: tap for the usual order"
                } else {
                    "Tap to reverse the order"
                })
                .clicked()
            {
                Self::set_rice(cx, "files.sort_reverse", RiceValue::Bool(!reverse));
            }
            if star_button(ui, is_saved, theme)
                .on_hover_text(if is_saved {
                    "A favorite: tap to take it out of Favorites"
                } else {
                    "Add this folder to Favorites"
                })
                .clicked()
                && let Some(dir) = here.clone()
            {
                let mut next = saved.clone();
                if is_saved {
                    next.retain(|path| *path != dir);
                } else {
                    next.push(dir);
                }
                let joined = next
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join("|");
                Self::set_rice(cx, "files.saved", RiceValue::Text(joined));
            }
            if widgets::pill_button(ui, "New Folder", self.new_folder.is_some(), theme).clicked() {
                self.new_folder = Some(String::new());
            }
            if let Some(pick) = self.pick.clone() {
                // While choosing a file, the system's own dialog chooses it.
                let label = if pick.save_as.is_some() {
                    "System Save Dialog"
                } else {
                    "System Open Dialog"
                };
                if widgets::pill_button(ui, label, false, theme)
                    .on_hover_text("Choose with the system's own file dialog instead")
                    .clicked()
                {
                    let dir = here.clone().unwrap_or_default();
                    if let Some(path) = self.system_dialog(&pick, &dir, cx) {
                        self.pick = None;
                        cx.request(ShellRequest::Picked { pick, path });
                    }
                }
            } else if widgets::pill_button(ui, "Open in Explorer", false, theme)
                .on_hover_text("Show this folder in the system's file manager")
                .clicked()
                && let Some(dir) = here.clone()
            {
                cx.request(ShellRequest::Command(format!("reveal:{}", dir.display())));
            }
        });
        if let Some(name) = self.new_folder.as_mut() {
            let mut make = false;
            let mut cancel = false;
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(name)
                        .desired_width(160.0)
                        .hint_text("Folder name"),
                );
                response.request_focus();
                if response.lost_focus() {
                    if ui.input(|input| input.key_pressed(Key::Enter)) {
                        make = true;
                    } else {
                        cancel = true;
                    }
                }
                if widgets::pill_button(ui, "Create", false, theme).clicked() {
                    make = true;
                }
            });
            if make {
                let name = name.trim().to_owned();
                if let (false, Some(dir)) = (name.is_empty(), here.clone()) {
                    let path = dir.join(&name);
                    match std::fs::create_dir(&path) {
                        Ok(()) => {
                            self.reload();
                            self.selected = Some(path);
                        }
                        Err(error) => cx.fail(format!("Cannot make {name}: {error}")),
                    }
                }
                self.new_folder = None;
            } else if cancel {
                self.new_folder = None;
            }
        }
    }

    fn places_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme, wide: bool) {
        let places = Self::places(cx);
        let saved = saved_folders(cx.rice());
        let here = self.dir().map(Path::to_path_buf);
        let mut go = None;
        let mut chip = None;
        let mut forget = None;
        let mut row = |ui: &mut Ui, name: &str, path: &Path, removable: bool| {
            let chosen = here.as_deref() == Some(path);
            ui.horizontal(|ui| {
                if widgets::pill_button(ui, name, chosen, theme)
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    go = Some(path.to_path_buf());
                }
                if removable
                    && star_button(ui, true, theme)
                        .on_hover_text("Take it out of Favorites")
                        .clicked()
                {
                    forget = Some(path.to_path_buf());
                }
            });
        };
        if wide {
            widgets::heading(ui, "Places", theme);
            for (name, path) in &places {
                row(ui, name, path, false);
            }
            ui.add_space(theme.padding);
            widgets::heading(ui, "Favorites", theme);
            if saved.is_empty() {
                ui.label(
                    RichText::new("The star by the search keeps the folder you are in here.")
                        .size(theme.small_size)
                        .color(theme.muted),
                );
            }
            for path in &saved {
                row(ui, &file_name(path), path, true);
            }
        } else {
            let open = self.places_open;
            let label = if open {
                "Places and favorites  \u{2212}"
            } else {
                "Places and favorites  +"
            };
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(label)
                            .size(theme.small_size)
                            .color(theme.muted),
                    )
                    .frame(false),
                )
                .clicked()
            {
                self.places_open = !open;
            }
            if open {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                    for (name, path) in &places {
                        if widgets::pill_button(
                            ui,
                            name,
                            here.as_deref() == Some(path.as_path()),
                            theme,
                        )
                        .on_hover_text(path.display().to_string())
                        .clicked()
                        {
                            chip = Some(path.clone());
                        }
                    }
                });
                if !saved.is_empty() {
                    widgets::heading(ui, "Favorites", theme);
                    for path in &saved {
                        row(ui, &file_name(path), path, true);
                    }
                }
            }
        }
        if let Some(path) = forget {
            let next = saved
                .iter()
                .filter(|saved| **saved != path)
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join("|");
            Self::set_rice(cx, "files.saved", RiceValue::Text(next));
        }
        if let Some(dir) = go.or(chip) {
            self.go(dir);
        }
    }

    /// The folder's entries as rows: icon, name, when, size.
    fn list_ui(
        &mut self,
        ui: &mut Ui,
        entries: &[DirEntryInfo],
        theme: &Theme,
        now: SystemTime,
    ) -> Option<Action> {
        let row_height = theme.ui_size * 1.9;
        let mut action = None;
        let wide = ui.available_width() > 420.0;
        for entry in entries {
            let (rect, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), row_height), Sense::click());
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let chosen = self.selected.as_deref() == Some(entry.path.as_path());
            if chosen {
                ui.painter().rect_filled(rect, 4.0, theme.active);
            } else if response.hovered() {
                ui.painter().rect_filled(rect, 4.0, theme.hover);
            }
            let painter = ui.painter().with_clip_rect(rect);
            let y = rect.center().y;
            painter.text(
                pos2(rect.left() + 6.0, y),
                Align2::LEFT_CENTER,
                icon_for(entry),
                theme.font(theme.ui_size),
                theme.muted,
            );
            let mut right = rect.right() - 6.0;
            let small = theme.font(theme.small_size);
            if !entry.is_dir {
                let placed = painter.text(
                    pos2(right, y),
                    Align2::RIGHT_CENTER,
                    human_bytes(entry.size),
                    small.clone(),
                    theme.muted,
                );
                right = placed.left() - 12.0;
            }
            if wide {
                let placed = painter.text(
                    pos2(right, y),
                    Align2::RIGHT_CENTER,
                    when(entry.modified, now),
                    small.clone(),
                    theme.muted,
                );
                right = placed.left() - 12.0;
            }
            let name_clip =
                Rect::from_min_max(pos2(rect.left(), rect.top()), pos2(right, rect.bottom()));
            ui.painter().with_clip_rect(name_clip.intersect(rect)).text(
                pos2(rect.left() + 28.0, y),
                Align2::LEFT_CENTER,
                &entry.name,
                theme.font(theme.ui_size),
                theme.text,
            );
            let response = response.on_hover_text(entry.path.display().to_string());
            if response.double_clicked() {
                action = Some(Action::Open(entry.clone()));
            } else if response.clicked() {
                action = Some(Action::Select(entry.path.clone()));
            }
        }
        action
    }

    /// The folder's entries as cards: a thumbnail or icon, the name and
    /// the size.
    /// The folder's entries in columns: name, kind, size and date, sorted
    /// by a click on a column's title.
    fn details_ui(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        entries: &[DirEntryInfo],
        theme: &Theme,
        now: SystemTime,
        sort: Sort,
    ) -> Option<Action> {
        let width = ui.available_width();
        let columns = [
            (Sort::Name, "Name", 0.46),
            (Sort::Kind, "Kind", 0.18),
            (Sort::Size, "Size", 0.14),
            (Sort::Modified, "Modified", 0.22),
        ];
        let reverse = cx.rice().get_bool("files.sort_reverse").unwrap_or(false);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (column, title, share) in columns {
                let label = if column == sort {
                    format!("{title} {}", if reverse { "\u{2191}" } else { "\u{2193}" })
                } else {
                    title.to_owned()
                };
                let button = egui::Button::new(
                    RichText::new(label)
                        .size(theme.small_size)
                        .color(theme.muted),
                )
                .frame(false);
                if ui.add_sized(vec2(width * share, 20.0), button).clicked() {
                    if column == sort {
                        Self::set_rice(cx, "files.sort_reverse", RiceValue::Bool(!reverse));
                    } else {
                        let key = Sort::ALL
                            .iter()
                            .find(|(known, _, _)| *known == column)
                            .map_or("name", |(_, key, _)| key);
                        Self::set_rice(cx, "files.sort", RiceValue::Text(key.to_owned()));
                    }
                }
            }
        });
        let row_height = theme.ui_size * 1.8;
        let mut action = None;
        for entry in entries {
            let (rect, response) = ui.allocate_exact_size(vec2(width, row_height), Sense::click());
            if !ui.is_rect_visible(rect) {
                continue;
            }
            let chosen = self.selected.as_deref() == Some(entry.path.as_path());
            if chosen {
                ui.painter().rect_filled(rect, 3.0, theme.active);
            } else if response.hovered() {
                ui.painter().rect_filled(rect, 3.0, theme.hover);
            }
            let y = rect.center().y;
            let mut x = rect.left();
            let cells = [
                format!("{}  {}", icon_for(entry), entry.name),
                if entry.is_dir {
                    "Folder".to_owned()
                } else {
                    kind_of(entry).to_owned()
                },
                if entry.is_dir {
                    String::new()
                } else {
                    human_bytes(entry.size)
                },
                when(entry.modified, now),
            ];
            for ((_, _, share), text) in columns.iter().zip(cells) {
                let cell = Rect::from_min_max(
                    pos2(x + 6.0, rect.top()),
                    pos2(x + width * share - 6.0, rect.bottom()),
                );
                let color = if x == rect.left() {
                    theme.text
                } else {
                    theme.muted
                };
                ui.painter().with_clip_rect(cell).text(
                    pos2(cell.left(), y),
                    Align2::LEFT_CENTER,
                    text,
                    theme.font(theme.small_size + 0.5),
                    color,
                );
                x += width * share;
            }
            let response = response.on_hover_text(entry.path.display().to_string());
            if response.double_clicked() {
                action = Some(Action::Open(entry.clone()));
            } else if response.clicked() {
                action = Some(Action::Select(entry.path.clone()));
            }
        }
        action
    }

    /// The folder's entries as cards: a thumbnail or icon, the name and
    /// (unless `compact`) the size.
    #[allow(clippy::too_many_arguments, reason = "one grid of cards")]
    fn cards_ui(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        entries: &[DirEntryInfo],
        theme: &Theme,
        side: f32,
        compact: bool,
    ) -> Option<Action> {
        let mut action = None;
        let gap = 6.0;
        let width = ui.available_width();
        let columns = (((width + gap) / (side + gap)).floor() as usize).max(1);
        let side = ((width + gap) / columns as f32 - gap).max(40.0);
        let lines = if compact { 1.4 } else { 2.6 };
        let card = vec2(side, side + theme.small_size * lines + 10.0);
        for row in entries.chunks(columns) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for entry in row {
                    let (rect, response) = ui.allocate_exact_size(card, Sense::click());
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    let chosen = self.selected.as_deref() == Some(entry.path.as_path());
                    let fill = if chosen {
                        theme.active
                    } else if response.hovered() {
                        theme.hover
                    } else {
                        theme.pill
                    };
                    ui.painter().rect(
                        rect,
                        theme.control_radius,
                        fill,
                        Stroke::new(
                            theme.border_width,
                            if chosen {
                                theme.border_active
                            } else {
                                theme.border
                            },
                        ),
                        StrokeKind::Inside,
                    );
                    let image =
                        Rect::from_min_size(rect.min + vec2(6.0, 6.0), Vec2::splat(side - 12.0));
                    let mut drawn = false;
                    if !entry.is_dir && thumbs::is_picture(&entry.path) {
                        let version = thumbs::version_of(entry.modified, entry.size);
                        if let Some(thumb) = self.thumbs.get(&entry.path, version, entry.size) {
                            let mut hasher = DefaultHasher::new();
                            entry.path.hash(&mut hasher);
                            let key = hasher.finish() ^ 0x5u64.rotate_left(60);
                            if let Some(texture) = cx.textures.texture(
                                key,
                                version,
                                thumb.size,
                                4,
                                &thumb.pixels,
                                false,
                            ) {
                                let aspect = thumb.size[0] as f32 / thumb.size[1].max(1) as f32;
                                let fit = if aspect >= 1.0 {
                                    vec2(image.width(), image.width() / aspect)
                                } else {
                                    vec2(image.height() * aspect, image.height())
                                };
                                let placed = Rect::from_center_size(image.center(), fit);
                                ui.painter().image(
                                    texture,
                                    placed,
                                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                    Color32::WHITE,
                                );
                                drawn = true;
                            }
                        }
                    }
                    if !drawn {
                        ui.painter().text(
                            image.center(),
                            Align2::CENTER_CENTER,
                            icon_for(entry),
                            theme.font(side * 0.34),
                            theme.muted,
                        );
                    }
                    let text = rect.shrink2(vec2(6.0, 0.0));
                    let name_at = pos2(text.left(), image.bottom() + 4.0);
                    let galley = ui.painter().layout(
                        entry.name.clone(),
                        theme.font(theme.small_size),
                        theme.text,
                        text.width(),
                    );
                    ui.painter()
                        .with_clip_rect(Rect::from_min_max(
                            name_at,
                            pos2(text.right(), rect.bottom() - theme.small_size - 4.0),
                        ))
                        .galley(name_at, galley, theme.text);
                    if !compact {
                        let detail = if entry.is_dir {
                            "Folder".to_owned()
                        } else {
                            human_bytes(entry.size)
                        };
                        ui.painter().text(
                            pos2(text.left(), rect.bottom() - 4.0),
                            Align2::LEFT_BOTTOM,
                            detail,
                            theme.font(theme.small_size - 1.0),
                            theme.muted,
                        );
                    }
                    let response = response.on_hover_text(entry.path.display().to_string());
                    if response.double_clicked() {
                        action = Some(Action::Open(entry.clone()));
                    } else if response.clicked() {
                        action = Some(Action::Select(entry.path.clone()));
                    }
                }
            });
            ui.add_space(gap);
        }
        action
    }

    /// The selected entry's full path, with Copy, Rename, Show and Open.
    fn selection_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
        let Some(path) = self.selected.clone() else {
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(path.display().to_string())
                        .size(theme.small_size)
                        .color(theme.text),
                )
                .selectable(true)
                .wrap(),
            );
        });
        if let Some(name) = self.renaming.as_mut() {
            let mut done = None;
            ui.horizontal(|ui| {
                let response = ui.add(egui::TextEdit::singleline(name).desired_width(180.0));
                if response.lost_focus() {
                    done = Some(ui.input(|input| input.key_pressed(Key::Enter)));
                }
                if widgets::pill_button(ui, "Rename", true, theme).clicked() {
                    done = Some(true);
                }
            });
            match done {
                Some(true) => {
                    let name = name.trim().to_owned();
                    let target = path.with_file_name(&name);
                    if !name.is_empty() && target != path {
                        if target.exists() {
                            cx.fail(format!("{name} already exists here"));
                        } else {
                            match std::fs::rename(&path, &target) {
                                Ok(()) => {
                                    self.reload();
                                    self.selected = Some(target);
                                }
                                Err(error) => cx.fail(format!("Cannot rename it: {error}")),
                            }
                        }
                    }
                    self.renaming = None;
                }
                Some(false) => self.renaming = None,
                None => {}
            }
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            if widgets::pill_button(ui, "Copy Path", false, theme).clicked() {
                ui.ctx().copy_text(path.display().to_string());
                cx.notify("Copied the path");
            }
            if widgets::pill_button(ui, "Rename", false, theme).clicked() {
                self.renaming = Some(file_name(&path));
            }
            if widgets::pill_button(ui, "Show in Explorer", false, theme).clicked() {
                cx.request(ShellRequest::Command(format!(
                    "reveal-file:{}",
                    path.display()
                )));
            }
            let playable = path.is_dir() || crate::player::library::is_audio(&path);
            if self.pick.is_none()
                && playable
                && widgets::pill_button(ui, "Add to Player", false, theme)
                    .on_hover_text("Add this song, or every song in this folder, to the Player")
                    .clicked()
            {
                cx.request(ShellRequest::SendFile {
                    to: sue_wm::AppletId::new(crate::player::ID).expect("a valid id"),
                    purpose: "import".to_owned(),
                    path: path.clone(),
                });
            }
            if self.pick.is_none()
                && widgets::pill_button(ui, "Convert", false, theme)
                    .on_hover_text("Send it to the Converter")
                    .clicked()
            {
                let convert = sue_wm::AppletId::new(crate::convert::ID).expect("a valid id");
                cx.request(ShellRequest::SendFile {
                    to: convert.clone(),
                    purpose: "add".to_owned(),
                    path: path.clone(),
                });
                cx.request(ShellRequest::ShowDrawer(convert));
            }
            if self.pick.is_none() && widgets::pill_button(ui, "Open", false, theme).clicked() {
                if path.is_dir() {
                    self.go(path.clone());
                } else {
                    cx.request(ShellRequest::OpenFile(path.clone()));
                }
            }
        });
    }

    /// Open or Save with Cancel and the system dialog, while picking.
    fn pick_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
        let Some(pick) = self.pick.clone() else {
            return;
        };
        let dir = self.dir().map(Path::to_path_buf).unwrap_or_default();
        let mut chosen = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if pick.save_as.is_some() {
                if !pick.types.is_empty() {
                    let current = pick
                        .type_of(Path::new(&self.save_name))
                        .map_or_else(|| pick.types[0].name.clone(), |kind| kind.name.clone());
                    egui::ComboBox::from_id_salt("sue-files-save-type")
                        .selected_text(current.clone())
                        .width(170.0)
                        .show_ui(ui, |ui| {
                            for kind in &pick.types {
                                let label =
                                    format!("{} (.{})", kind.name, kind.extensions.join(", ."));
                                if ui
                                    .add(egui::Button::selectable(kind.name == current, label))
                                    .clicked()
                                    && let Some(ext) = kind.extensions.first()
                                {
                                    let stem = Path::new(&self.save_name)
                                        .file_stem()
                                        .map_or_else(String::new, |stem| {
                                            stem.to_string_lossy().into_owned()
                                        });
                                    self.save_name = format!("{stem}.{ext}");
                                }
                            }
                        })
                        .response
                        .on_hover_text("Save as type");
                }
                ui.label(RichText::new("File name").color(theme.muted));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.save_name)
                        .desired_width((ui.available_width() - 260.0).max(120.0)),
                );
                let enter =
                    response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
                if (widgets::pill_button(ui, "Save", true, theme).clicked() || enter)
                    && !self.save_name.trim().is_empty()
                {
                    let mut name = self.save_name.trim().to_owned();
                    if let Some(extension) = pick.extensions.first()
                        && Path::new(&name).extension().is_none()
                    {
                        name = format!("{name}.{extension}");
                    }
                    chosen = Some(dir.join(name));
                }
            } else {
                if pick.folders
                    && widgets::pill_button(ui, "Choose This Folder", true, theme)
                        .on_hover_text(dir.display().to_string())
                        .clicked()
                {
                    chosen = Some(dir.clone());
                }
                let file = self
                    .selected
                    .clone()
                    .filter(|path| path.is_file() && pick.takes(path));
                let label = file
                    .as_ref()
                    .map_or_else(|| "Choose a file".to_owned(), |path| file_name(path));
                ui.label(RichText::new(label).color(theme.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |_| {});
                if ui
                    .add_enabled(file.is_some(), egui::Button::new("Open"))
                    .clicked()
                {
                    chosen = file;
                }
            }
            if widgets::pill_button(ui, "Cancel", false, theme).clicked() {
                self.pick = None;
                cx.request(ShellRequest::CloseWindow);
            }
            if widgets::pill_button(ui, "System Dialog", false, theme)
                .on_hover_text("Choose with the system's own file dialog")
                .clicked()
            {
                chosen = self.system_dialog(&pick, &dir, cx);
            }
        });
        if let Some(path) = chosen {
            if pick.save_as.is_some() && path.exists() {
                cx.notify(format!("{} will be replaced", file_name(&path)));
            }
            self.pick = None;
            cx.request(ShellRequest::Picked { pick, path });
        }
    }

    fn system_dialog(
        &self,
        pick: &FilePick,
        dir: &Path,
        cx: &mut AppletContext<'_>,
    ) -> Option<PathBuf> {
        if !cx.view.interactive {
            cx.notify("The system dialog needs the desktop window");
            return None;
        }
        let filters = filters_of(pick);
        match &pick.save_as {
            Some(name) => sue_platform::pick_save_file(&pick.title, Some(dir), name, &filters),
            None => sue_platform::pick_open_file(&pick.title, Some(dir), &filters),
        }
    }

    /// Everything but the tabs and places: the entries and, below them, the
    /// selection.
    fn content_ui(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> Option<Action> {
        if let Some(search) = self.deep_search.as_mut() {
            search.collect();
        }
        if let Some(tab) = self.tabs.get(self.current)
            && let Some(error) = &tab.error
        {
            ui.label(RichText::new(format!("Cannot list this folder: {error}")).color(theme.muted));
        }
        let mut entries = self.shown();
        let sort = Sort::from_rice(cx.rice().get_str("files.sort").unwrap_or("name"));
        sort_entries(&mut entries, sort);
        if cx.rice().get_bool("files.sort_reverse").unwrap_or(false) {
            // Folders stay first; the rest turn around.
            let folders = entries.iter().take_while(|entry| entry.is_dir).count();
            entries[..folders].reverse();
            entries[folders..].reverse();
        }
        if let Some(search) = self.deep_search.as_ref().filter(|_| self.deep) {
            let status = if search.done {
                format!(
                    "{} found under {}",
                    search.results.len(),
                    file_name(&search.root)
                )
            } else {
                format!("Searching... {} found so far", search.results.len())
            };
            ui.label(
                RichText::new(status)
                    .size(theme.small_size)
                    .color(theme.muted),
            );
        }
        if entries.is_empty() {
            let text = if self.search.trim().is_empty() {
                "This folder is empty"
            } else {
                "Nothing matches"
            };
            ui.label(RichText::new(text).color(theme.muted));
            return None;
        }
        let side = cx.rice().get_i64("files.card_size").unwrap_or(112) as f32;
        match Self::view(cx) {
            View::List => self.list_ui(ui, &entries, theme, SystemTime::now()),
            View::Details => self.details_ui(ui, cx, &entries, theme, SystemTime::now(), sort),
            View::Icons => self.cards_ui(ui, cx, &entries, theme, (side * 0.6).max(48.0), true),
            View::Cards => self.cards_ui(ui, cx, &entries, theme, side, false),
        }
    }

    fn act(&mut self, action: Option<Action>, cx: &mut AppletContext<'_>) {
        match action {
            Some(Action::Select(path)) => {
                if self.selected.as_ref() != Some(&path) {
                    self.renaming = None;
                }
                if let Some(pick) = &self.pick
                    && pick.save_as.is_some()
                    && path.is_file()
                {
                    self.save_name = file_name(&path);
                }
                self.selected = Some(path);
            }
            Some(Action::Open(entry)) => {
                if entry.is_dir {
                    self.go(entry.path);
                } else if let Some(pick) = self.pick.clone() {
                    if pick.takes(&entry.path) {
                        self.pick = None;
                        cx.request(ShellRequest::Picked {
                            pick,
                            path: entry.path,
                        });
                    }
                } else {
                    cx.request(ShellRequest::OpenFile(entry.path));
                }
            }
            None => {}
        }
    }

    /// Enter opens the selection, Backspace goes up, F2 renames.
    fn keys(&mut self, ui: &Ui, cx: &mut AppletContext<'_>) {
        if ui.ctx().text_edit_focused() {
            return;
        }
        let (enter, up, rename) = ui.input(|input| {
            (
                input.key_pressed(Key::Enter),
                input.key_pressed(Key::Backspace),
                input.key_pressed(Key::F2),
            )
        });
        if enter && let Some(path) = self.selected.clone() {
            let entry = DirEntryInfo {
                name: file_name(&path),
                is_dir: path.is_dir(),
                path,
                size: 0,
                modified: None,
            };
            self.act(Some(Action::Open(entry)), cx);
        }
        if up && let Some(parent) = self.dir().and_then(Path::parent).map(Path::to_path_buf) {
            self.go(parent);
        }
        if rename && let Some(path) = &self.selected {
            self.renaming = Some(file_name(path));
        }
    }
}

impl SueApplet for Files {
    fn manifest(&self) -> AppletManifest {
        let mut manifest =
            AppletManifest::new(ID, "File Manager", AppletRole::LeftDrawer, "\u{1F4C1}");
        manifest.summary =
            "Browse, search and open files; every studio opens and saves through it.".to_owned();
        manifest.order = 10;
        manifest
    }

    fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        let choice = |value: &str, options: &[&str]| RiceParameter::EnumChoice {
            value: value.to_owned(),
            options: options.iter().map(|&option| option.to_owned()).collect(),
        };
        vec![
            RiceDeclaration::new(
                "files.view",
                choice("list", &["list", "details", "icons", "cards"]),
                "How the File Manager shows a folder",
            ),
            RiceDeclaration::new(
                "files.sort_reverse",
                RiceParameter::Bool { value: false },
                "Reverse the File Manager's order (folders stay first)",
            ),
            RiceDeclaration::new(
                "files.dialog",
                choice("sue", &["sue", "system"]),
                "Choose files to open and places to save with Sue's File Manager or the system's own dialog",
            ),
            RiceDeclaration::new(
                "files.sort",
                choice("name", &["name", "modified", "size", "kind"]),
                "What the File Manager sorts by (folders come first)",
            ),
            RiceDeclaration::new(
                "files.card_size",
                RiceParameter::IntRange {
                    value: 112,
                    min: Some(48),
                    max: Some(320),
                },
                "Width of the File Manager's cards (px)",
            ),
            RiceDeclaration::new(
                "files.saved",
                RiceParameter::Text {
                    value: String::new(),
                },
                "Favorite folders, separated by |",
            ),
        ]
    }

    fn drawer_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.sync(cx);
        let theme = cx.theme.clone();
        self.tabs_ui(ui, &theme);
        self.path_ui(ui, &theme);
        self.tools_ui(ui, cx, &theme);
        self.places_ui(ui, cx, &theme, false);
        self.selection_ui(ui, cx, &theme);
        ui.separator();
        let action = self.content_ui(ui, cx, &theme);
        self.act(action, cx);
        self.keys(ui, cx);
        if ui
            .add(
                egui::Button::new(
                    RichText::new("Open the large File Manager")
                        .size(theme.small_size)
                        .color(theme.muted),
                )
                .frame(false),
            )
            .clicked()
        {
            cx.request(ShellRequest::OpenWindow(
                sue_wm::AppletId::new(ID).expect("a valid id"),
            ));
        }
    }

    fn window_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.sync(cx);
        let theme = cx.theme.clone();
        let area = ui.max_rect().shrink(theme.padding * 1.5);
        let sidebar = Rect::from_min_size(area.min, vec2(170.0, area.height()));
        let main = Rect::from_min_max(
            pos2(sidebar.right() + theme.padding * 1.5, area.top()),
            area.max,
        );
        let mut side_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(sidebar)
                .layout(Layout::top_down(Align::Min)),
        );
        ScrollArea::vertical()
            .id_salt("sue-files-places")
            .auto_shrink([false, false])
            .show(&mut side_ui, |ui| self.places_ui(ui, cx, &theme, true));
        ui.painter().vline(
            sidebar.right() + theme.padding * 0.75,
            sidebar.y_range(),
            theme.hairline(),
        );
        let mut main_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(main)
                .layout(Layout::top_down(Align::Min)),
        );
        let ui = &mut main_ui;
        self.tabs_ui(ui, &theme);
        self.path_ui(ui, &theme);
        self.tools_ui(ui, cx, &theme);
        ui.separator();
        let footer = if self.pick.is_some() { 64.0 } else { 56.0 };
        let list_height = (ui.available_height() - footer).max(80.0);
        let action = ScrollArea::vertical()
            .id_salt("sue-files-window")
            .auto_shrink([false, false])
            .max_height(list_height)
            .show(ui, |ui| self.content_ui(ui, cx, &theme))
            .inner;
        self.act(action, cx);
        ui.separator();
        if self.pick.is_some() {
            self.pick_ui(ui, cx, &theme);
        } else {
            self.selection_ui(ui, cx, &theme);
        }
        self.keys(ui, cx);
    }

    fn tick(&mut self, cx: &mut AppletContext<'_>) -> Tick {
        self.system_picks =
            cx.view.interactive && cx.rice().get_str("files.dialog") == Some("system");
        let arrived = self.thumbs.collect();
        let searching = self.deep_search.as_ref().is_some_and(|search| !search.done);
        let waiting = self.thumbs.pending() || searching;
        Tick {
            repaint: arrived || searching,
            wake: waiting.then(|| std::time::Instant::now() + Duration::from_millis(60)),
            ..Tick::default()
        }
    }

    fn begin_pick(&mut self, pick: &FilePick) -> bool {
        // The settings may hand every pick to the system's own dialog.
        if self.system_picks {
            return false;
        }
        self.pick = Some(pick.clone());
        self.save_name = pick.save_as.clone().unwrap_or_default();
        self.selected = None;
        if let Some(start) = pick.start.clone().filter(|start| start.is_dir()) {
            if self.tabs.is_empty() {
                self.tabs.push(Tab::new(start, self.show_hidden));
                self.current = 0;
            } else {
                self.go(start);
            }
        }
        true
    }

    fn sue_actions(
        &self,
        focus: Option<&sue_core::Entity>,
        _current: bool,
    ) -> Vec<sue_wm::SueAction> {
        let me = || sue_wm::AppletId::new(ID).expect("a valid id");
        let mut actions = vec![sue_wm::SueAction::new(
            sue_wm::wheel::FILE,
            "File Manager",
            ShellRequest::OpenWindow(me()),
        )];
        if let Some(path) = focus
            .and_then(|entity| entity.meta(sue_io::meta::SOURCE))
            .map(PathBuf::from)
            .filter(|path| path.exists())
        {
            actions.push(sue_wm::SueAction::new(
                sue_wm::wheel::SEND_TO,
                "File Manager",
                ShellRequest::SendFile {
                    to: me(),
                    purpose: "show".to_owned(),
                    path: path.clone(),
                },
            ));
            actions.push(sue_wm::SueAction::new(
                sue_wm::wheel::FILE,
                "Show in Explorer",
                ShellRequest::Command(format!("reveal-file:{}", path.display())),
            ));
        }
        actions
    }

    fn picked(&mut self, pick: &FilePick, path: &Path, cx: &mut AppletContext<'_>) {
        if pick.purpose == "show" {
            // Show a file, selected in its folder, in the large window.
            self.sync(cx);
            self.pick = None;
            if let Some(folder) = path.parent() {
                self.go(folder.to_path_buf());
            }
            self.selected = Some(path.to_path_buf());
            cx.request(ShellRequest::OpenWindow(
                sue_wm::AppletId::new(ID).expect("a valid id"),
            ));
        }
    }

    fn window_title(&self) -> Option<String> {
        Some(
            self.pick
                .as_ref()
                .map_or_else(|| "File Manager".to_owned(), |pick| pick.title.clone()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, is_dir: bool) -> DirEntryInfo {
        DirEntryInfo {
            name: name.to_owned(),
            path: PathBuf::from(name),
            is_dir,
            size: 0,
            modified: None,
        }
    }

    #[test]
    fn icons_and_kinds_follow_the_kind_of_file() {
        assert_eq!(icon_for(&entry("photos", true)), "\u{1F4C1}");
        assert_eq!(icon_for(&entry("card.jxl", false)), "\u{1F5BC}");
        assert_eq!(icon_for(&entry("song.flac", false)), "\u{1F3B5}");
        assert_eq!(icon_for(&entry("notes.txt", false)), "\u{1F4C4}");
        assert_eq!(kind_of(&entry("main.rs", false)), "Code");
        assert_eq!(kind_of(&entry("work.sue", false)), "Sue Project");
    }

    #[test]
    fn legacy_junctions_and_dot_files_stay_hidden() {
        assert!(sue_platform::hidden_by_os(".git"));
        assert_eq!(sue_platform::hidden_by_os("NetHood"), cfg!(windows));
        assert_eq!(sue_platform::hidden_by_os("Start Menu"), cfg!(windows));
        assert!(!sue_platform::hidden_by_os("Documents"));
    }

    #[test]
    fn searches_match_every_word_and_sorting_keeps_folders_first() {
        assert!(matches("Summer Trip 2026.jpg", "trip summer"));
        assert!(!matches("Summer Trip 2026.jpg", "winter"));
        let mut entries = vec![
            DirEntryInfo {
                size: 10,
                ..entry("b.png", false)
            },
            entry("zeta", true),
            DirEntryInfo {
                size: 900,
                ..entry("a.txt", false)
            },
        ];
        sort_entries(&mut entries, Sort::Size);
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["zeta", "a.txt", "b.png"]);
        sort_entries(&mut entries, Sort::Name);
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["zeta", "a.txt", "b.png"]);
        assert_eq!(Sort::from_rice(Sort::Kind.key()), Sort::Kind);
    }

    #[test]
    fn times_read_like_people_say_them() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let ago = |seconds| Some(now - Duration::from_secs(seconds));
        assert_eq!(when(ago(10), now), "just now");
        assert_eq!(when(ago(300), now), "5 min ago");
        assert_eq!(when(ago(7_200), now), "2 h ago");
        assert_eq!(when(ago(90_000), now), "yesterday");
        assert_eq!(when(ago(5 * 86_400), now), "5 days ago");
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_754), (2026, 10, 28));
        assert_eq!(when(None, now), "");
    }

    #[test]
    fn a_search_under_a_folder_finds_files_in_its_subfolders() {
        let root = std::env::temp_dir().join(format!("sue-files-deep-{}", std::process::id()));
        std::fs::create_dir_all(root.join("a").join("b")).unwrap();
        std::fs::write(root.join("a").join("b").join("holiday.png"), b"x").unwrap();
        std::fs::write(root.join("other.txt"), b"x").unwrap();
        let mut search = DeepSearch::start(root.clone(), "holiday".to_owned(), false);
        let start = std::time::Instant::now();
        while !search.done && start.elapsed().as_secs() < 20 {
            search.collect();
            std::thread::sleep(Duration::from_millis(5));
        }
        let names: Vec<&str> = search
            .results
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(names, ["holiday.png"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn picks_take_only_their_extensions() {
        let pick = FilePick::open(None, "open", "Open").extensions(&["png", "jxl"]);
        assert!(pick.takes(Path::new("a/B.PNG")));
        assert!(!pick.takes(Path::new("a/b.txt")));
        assert!(FilePick::default().takes(Path::new("anything")));
    }
}
