//! Palettes that keep work at hand: Auto Actions (menu commands recorded
//! and played back), the Item Bank (sub tools kept to add again), Info
//! (what is under the pointer) and registered workspaces (palette layouts
//! by name), each kept in Sue Paint's data folder, with the palette layout
//! as it was last left.

use std::path::Path;

use sue_wm::AppletContext;

/// The golden ledger's tests read Auto Actions by this path.
#[cfg(test)]
pub(crate) use crate::panels::auto_action::AutoAction;
pub(crate) use crate::panels::auto_action::AutoActions;
pub(crate) use crate::panels::item_bank::Item;
use crate::studio::{Panels, SuePaint};

/// The Sue Paint Guide (Help), as headings (`# `) and paragraphs.
#[must_use]
pub(crate) fn guide() -> Vec<String> {
    [
        tr!("starting"),
        tr!("file-new-makes-illustration-comic-webtoon"),
        tr!("tools"),
        tr!("tool-strip-left-holds-tools-sub"),
        tr!("alt-picks-color-while-drawing-space"),
        tr!("layers"),
        tr!("raster-layers-hold-pixels-vector-layers"),
        tr!("colors"),
        tr!("colors-set-the-main-color-x-switches"),
        tr!("selections"),
        tr!("selection-tools-make-selection-launcher-under"),
        tr!("pages-and-books"),
        tr!("comic-story-pages-story-menu-adds"),
        tr!("animation-2"),
        tr!("animation-timeline-new-timeline-starts-animation"),
        tr!("repeating-work"),
        tr!("auto-actions-navigator-palette-records-menu"),
        tr!("exporting"),
        tr!("file-export-writes-png-jpeg-jpeg"),
    ]
    .map(String::from)
    .to_vec()
}

/// What Help, Check for Updates says.
#[must_use]
pub(crate) fn updates() -> Vec<String> {
    vec![
        tr!("this-is-sue", value = sue_core::SUE_VERSION),
        tr!("new-versions-are-published-as-releases"),
        "https://github.com/MagiGun/sue/releases".to_owned(),
        tr!("sue-never-downloads-installs-anything-itself"),
    ]
}

/// Named sections of lines: `[name]` then its lines.
pub(crate) fn sections(text: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            out.push((name.to_owned(), Vec::new()));
        } else if !line.is_empty()
            && let Some((_, lines)) = out.last_mut()
        {
            lines.push(line.to_owned());
        }
    }
    out
}

pub(crate) fn write_sections<'a>(items: impl Iterator<Item = (&'a str, Vec<String>)>) -> String {
    let mut out = String::new();
    for (name, lines) in items {
        out.push('[');
        out.push_str(name);
        out.push_str("]\n");
        for line in lines {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

/// What the Information, Item Bank, Auto Action, Sub View and Material
/// palettes keep besides the older fields of `SuePaint`.
#[derive(Debug, Default)]
pub(crate) struct Extras {
    /// What the Information palette shows.
    pub info: crate::panels::InfoOptions,
    /// The Item Bank's pictures, PNG files in its folder.
    pub pictures: Vec<std::path::PathBuf>,
    /// Thumbnails of the Item Bank's pictures.
    pub bank_thumbs: crate::panels::item_bank::Thumbs,
    /// The frame the Item Bank was last drawn in.
    pub bank_drawn: Option<u64>,
    /// Whether the current layer has pixels, as of its id and revision.
    pub layer_drawn: std::cell::Cell<Option<(sue_core::EntityId, u64, bool)>>,
    /// The Material palette's view: its folder, search, size and picks.
    pub material: crate::panels::MaterialView,
    /// The library keys of the favorite materials, as of the last frame
    /// the Material palette drew.
    pub material_favorites: Vec<String>,
}

/// The file in Sue Paint's data folder that keeps the palette layout
/// between runs (never inside a document), for the default layout style
/// (another style keeps `layout.<id>.txt`, `layout_style::file_for`).
const LAYOUT_FILE: &str = "layout.txt";

/// The file the Auto Action sets are kept in.
pub(crate) const AUTO_FILE: &str = "auto_actions.txt";

/// The file the registered workspaces are kept in, a section each.
const WORKSPACES_FILE: &str = "workspaces.txt";

/// The file the default workspace is kept in (Use Current Workspace as
/// Default), one section as `workspaces.txt` has.
const DEFAULT_FILE: &str = "default_workspace.txt";

/// The rice key: Sue Paint opens with its palettes where they were left.
pub(crate) const REMEMBER_LAYOUT: &str = "sue_paint.remember_layout";

/// How long the layout stays the same before it is written (seconds), so
/// moving a window writes it once.
const LAYOUT_SETTLE: f64 = 0.5;

fn remember_layout(cx: &AppletContext<'_>) -> bool {
    cx.rice().get_bool(REMEMBER_LAYOUT).unwrap_or(true)
}

/// A palette layout kept by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Workspace {
    pub name: String,
    pub lines: Vec<String>,
}

/// A workspace Sue Paint ships, offered in Window, Workspace before the
/// registered ones: its id (saved in the command
/// `window.workspace.preset.<id>`), the language key of its name, and
/// what it changes on the style's own arrangement.
pub(crate) struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub arrange: fn(&mut crate::dock::Dock, &mut Panels),
}

/// The workspaces Sue Paint ships: Illustration (the style's own
/// arrangement) and Animation (the Timeline open under the canvas and
/// Animation Cels beside the layers).
pub(crate) const PRESETS: [Preset; 2] = [
    Preset {
        id: "illustration",
        label: "workspace-illustration",
        arrange: |_, _| {},
    },
    Preset {
        id: "animation",
        label: "workspace-animation",
        arrange: |dock, panels| {
            use crate::dock::Palette;
            panels.timeline = true;
            dock.row_folded[sue_wm::dock::Row::Bottom.index()] = false;
            dock.row_hidden[sue_wm::dock::Row::Bottom.index()] = false;
            dock.bottom_height = dock.bottom_height.max(200.0);
            if dock.find(Palette::Layer).is_some() {
                dock.put(
                    Palette::Cels,
                    sue_wm::dock::Target::Join {
                        anchor: Palette::Layer,
                        before: false,
                    },
                );
                dock.choose(Palette::Layer);
            }
        },
    },
];

// ----- The lines a layout or workspace is kept as -------------------------
//
// These lines are a file format (docs/paint-workspace-files.md), never
// interface text: they are written in English whatever the language, and
// read in English. Sue Paint 0.0.75 and older wrote four of them through
// the language database, so a file written in another language is read
// once with the words below and written again in English.

/// The word before the ten palette switches.
const PANELS_WORD: &str = "panels";
/// The word before whether the menu bar shows (1) or not (0).
const MENUS_WORD: &str = "menus";
/// The word before the layout style a workspace was made in.
const STYLE_WORD: &str = "style";
/// The word before how the strip opens its palettes (`tab` or `popup`).
const STRIP_WORD: &str = "strip";
/// How the lines `panels`, `menus`, `hide` and `add` read in each
/// language Sue Paint 0.0.75 wrote them in: (language, panels, menus,
/// hide, add). A word is written before the value, or, with a leading
/// space, after it.
pub(crate) const OLD_WORDS: [(&str, &str, &str, &str, &str); 9] = [
    ("en", "panels ", "menus ", "hide ", "add "),
    ("ar", "لوحات ", "قوائم ", "إخفاء ", "إضافة "),
    ("de", "Panels ", "Menüs ", " ausblenden", " hinzufügen"),
    ("es", "paneles ", "menús ", "ocultar ", "añadir "),
    ("fr", "cases ", "menus ", "masquer ", "ajouter "),
    ("it", "pannelli ", "menu ", "nascondi ", "aggiungi "),
    ("ja", "パネル ", "メニュー ", "hide ", "add "),
    ("pt-BR", "painéis ", "menus ", "ocultar ", "adicionar "),
    ("zh-Hans", "面板 ", "菜单 ", "隐藏 ", "添加 "),
];

/// The value `word` holds in `line`: after the word, or before it when
/// the word starts with a space.
fn after_word<'a>(line: &'a str, word: &str) -> Option<&'a str> {
    if word.starts_with(' ') {
        line.strip_suffix(word)
    } else {
        line.strip_prefix(word)
    }
}

/// The value of a `panels` line (`panels`) or a `menus` line, in any
/// language 0.0.75 wrote it in.
fn old_value(line: &str, panels: bool) -> Option<&str> {
    OLD_WORDS
        .iter()
        .find_map(|words| after_word(line, if panels { words.1 } else { words.2 }))
}

/// Panels as a line of flags.
#[must_use]
pub(crate) fn panels_line(panels: &Panels) -> String {
    let flags = [
        panels.command_bar,
        panels.tool,
        panels.sub_tool,
        panels.tool_property,
        panels.brush_size,
        panels.color,
        panels.color_set,
        panels.navigator,
        panels.layer,
        panels.timeline,
    ];
    let bits: String = flags.iter().map(|on| if *on { '1' } else { '0' }).collect();
    format!("{PANELS_WORD} {bits}")
}

/// Panels from [`panels_line`] (or from such a line in another language).
#[must_use]
pub(crate) fn panels_from(line: &str) -> Option<Panels> {
    let bits = old_value(line, true)?;
    if bits.len() != 10 || !bits.chars().all(|c| c == '0' || c == '1') {
        return None;
    }
    let bits: Vec<bool> = bits.chars().map(|c| c == '1').collect();
    Some(Panels {
        command_bar: bits[0],
        tool: bits[1],
        sub_tool: bits[2],
        tool_property: bits[3],
        brush_size: bits[4],
        color: bits[5],
        color_set: bits[6],
        navigator: bits[7],
        layer: bits[8],
        timeline: bits[9],
    })
}

/// Whether the menu bar shows, from a `menus` line in any language.
fn menus_from(line: &str) -> Option<bool> {
    match old_value(line, false)? {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

/// `line` as Sue Paint writes it now: a `panels` or `menus` line in
/// another language becomes the English one; any other line is kept.
#[must_use]
pub(crate) fn canonical_line(line: &str) -> String {
    if let Some(panels) = panels_from(line) {
        return panels_line(&panels);
    }
    if let Some(shown) = menus_from(line) {
        return format!("{MENUS_WORD} {}", u8::from(shown));
    }
    line.to_owned()
}

pub(crate) fn read(folder: Option<&Path>, file: &str) -> Option<String> {
    sue_platform::fs::read_to_string(folder?.join(file)).ok()
}

pub(crate) fn write(folder: Option<&Path>, file: &str, text: &str) -> Result<(), String> {
    let folder = folder.ok_or(tr!("sue-paint-has-no-data-folder"))?;
    sue_platform::fs::create_dir_all(folder)
        .and_then(|()| sue_platform::fs::write(folder.join(file), text))
        .map_err(|error| error.to_string())
}

/// Workspaces from a `workspaces.txt` text, their lines as Sue Paint
/// writes them now, and whether any line changed (a file written in
/// another language, to be written again).
fn workspaces_from(text: &str) -> (Vec<Workspace>, bool) {
    let mut changed = false;
    let spaces = sections(text)
        .into_iter()
        .map(|(name, lines)| {
            let lines: Vec<String> = lines
                .into_iter()
                .map(|line| {
                    let tidy = canonical_line(&line);
                    changed |= tidy != line;
                    tidy
                })
                .collect();
            Workspace { name, lines }
        })
        .collect();
    (spaces, changed)
}

fn workspaces_text(spaces: &[Workspace]) -> String {
    write_sections(
        spaces
            .iter()
            .map(|space| (space.name.as_str(), space.lines.clone())),
    )
}

impl SuePaint {
    /// Reads the auto actions, the item bank and the workspaces, once.
    pub(crate) fn load_extras(&mut self, cx: &AppletContext<'_>) {
        if self.auto.loaded {
            return;
        }
        self.auto.loaded = true;
        let folder = crate::colorsets::folder(cx);
        self.auto
            .load(read(folder.as_deref(), AUTO_FILE).as_deref());
        self.item_bank = read(folder.as_deref(), "item_bank.txt")
            .map(|text| text.lines().filter_map(Item::from_line).collect())
            .unwrap_or_default();
        self.extras.info = read(folder.as_deref(), crate::panels::InfoOptions::FILE)
            .map(|text| crate::panels::InfoOptions::from_text(&text))
            .unwrap_or_default();
        self.extras.pictures = crate::panels::bank_pictures(folder.as_deref());
        self.canvas_presets = read(folder.as_deref(), "canvas_presets.txt")
            .map(|text| {
                text.lines()
                    .filter_map(crate::dialogs::CanvasPreset::from_line)
                    .collect()
            })
            .unwrap_or_default();
        // The brush tips made from pictures, in the materials location.
        if let Some(entries) = crate::materials::location(cx)
            .and_then(|folder| sue_platform::list_dir(&folder.join("tips"), true).ok())
        {
            for entry in entries {
                let path = entry.path;
                let name = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if let Some(shape) = sue_platform::fs::read(&path)
                    .ok()
                    .and_then(|bytes| sue_io::decode_png(&bytes).ok())
                    .and_then(|decoded| {
                        let tile = decoded.tile;
                        crate::brush::TipShape::from_rgba(
                            &name,
                            tile.bytes()?,
                            tile.width(),
                            tile.height(),
                        )
                    })
                {
                    crate::brush::register_shape(shape);
                }
            }
        }
        self.load_command_bar(folder.as_deref());
        self.layout_style = crate::layout_style::current(cx.rice()).id;
        self.load_style_files(folder.as_deref(), remember_layout(cx));
    }

    /// Reads the layout style in use's workspaces, its default workspace
    /// and, when `remember`, its palettes where they were left (else where
    /// people chose as the default, else the style's own arrangement).
    /// Files written in another language are written again in English.
    fn load_style_files(&mut self, folder: Option<&Path>, remember: bool) {
        let style = self.layout_style;
        let file = |name: &str| crate::layout_style::file_for(style, name);
        let (spaces, old) = read(folder, &file(WORKSPACES_FILE))
            .map(|text| workspaces_from(&text))
            .unwrap_or_default();
        self.workspaces = spaces;
        if old {
            let _ = write(
                folder,
                &file(WORKSPACES_FILE),
                &workspaces_text(&self.workspaces),
            );
        }
        let (default, old) = read(folder, &file(DEFAULT_FILE))
            .map(|text| workspaces_from(&text))
            .unwrap_or_default();
        self.default_workspace = default.into_iter().next();
        if old && let Some(space) = &self.default_workspace {
            let _ = write(
                folder,
                &file(DEFAULT_FILE),
                &workspaces_text(std::slice::from_ref(space)),
            );
        }
        self.current_workspace = None;
        self.hidden_all = None;
        self.refresh_workspace_menu();
        let kept = remember
            .then(|| read(folder, &file(LAYOUT_FILE)))
            .flatten()
            .map(|text| text.lines().map(str::to_owned).collect::<Vec<String>>());
        if let Some(lines) = kept {
            self.apply_layout(&lines);
        } else if let Some(space) = self.default_workspace.clone() {
            self.apply_layout(&space.lines);
        } else {
            self.dock = self.style_layout();
            self.panels = Panels::default();
        }
        // A file in the old form is written again by `keep_layout`, since
        // its lines differ from these.
        self.layout_kept = read(folder, &file(LAYOUT_FILE))
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_else(|| self.workspace_now("").lines);
    }

    /// Follows the setting `sue_paint.layout_style` (changed in Settings,
    /// by the Window menu or by undo): the palettes as they are now are
    /// kept for the style left, and the new style's come back.
    pub(crate) fn follow_layout_style(&mut self, cx: &mut AppletContext<'_>) {
        let wanted = crate::layout_style::current(cx.rice()).id;
        if wanted != self.layout_style && self.auto.loaded {
            let folder = crate::colorsets::folder(cx);
            self.switch_layout_style(wanted, folder.as_deref(), remember_layout(cx));
        }
    }

    /// Changes to the layout style `id`: writes the palettes as they are
    /// for the style in use, then lays out `id`'s as they were left (kept
    /// for this run, else in its files), else its default workspace, else
    /// its own arrangement; its workspaces fill Window, Workspace.
    pub(crate) fn switch_layout_style(
        &mut self,
        id: &'static str,
        folder: Option<&Path>,
        remember: bool,
    ) {
        if id == self.layout_style {
            return;
        }
        let lines = self.workspace_now("").lines;
        let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
        if remember {
            let _ = write(
                folder,
                &crate::layout_style::file_for(self.layout_style, LAYOUT_FILE),
                &text,
            );
        }
        self.kept_styles
            .retain(|(style, _)| *style != self.layout_style);
        self.kept_styles.push((self.layout_style, lines));
        self.layout_style = id;
        let kept = self
            .kept_styles
            .iter()
            .find(|(style, _)| *style == id)
            .map(|(_, lines)| lines.clone());
        self.load_style_files(folder, remember);
        if let Some(lines) = kept {
            self.apply_layout(&lines);
        }
        self.layout_kept = self.workspace_now("").lines;
    }

    /// Writes the palette layout to the data folder once it has stayed the
    /// same a moment, when Settings say to remember it.
    pub(crate) fn keep_layout(&mut self, ctx: &egui::Context, cx: &mut AppletContext<'_>) {
        if !remember_layout(cx) || self.dock.dragging() {
            return;
        }
        let lines = self.workspace_now("").lines;
        if lines == self.layout_kept {
            self.layout_changed = None;
            return;
        }
        let now = ctx.input(|input| input.time);
        let since = *self.layout_changed.get_or_insert(now);
        if now - since < LAYOUT_SETTLE {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(LAYOUT_SETTLE + 0.1));
            return;
        }
        let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
        let folder = crate::colorsets::folder(cx);
        let file = crate::layout_style::file_for(self.layout_style, LAYOUT_FILE);
        if let Err(error) = write(folder.as_deref(), &file, &text) {
            cx.fail(tr!("cannot-keep-the-palettes-settings", error = error));
        }
        self.layout_kept = lines;
        self.layout_changed = None;
    }

    /// Writes the auto actions, the item bank and the workspaces.
    pub(crate) fn save_extras(&mut self, cx: &mut AppletContext<'_>) {
        let folder = crate::colorsets::folder(cx);
        let bank: String = self
            .item_bank
            .iter()
            .map(|item| item.to_line() + "\n")
            .collect();
        let spaces = workspaces_text(&self.workspaces);
        let spaces_file = crate::layout_style::file_for(self.layout_style, WORKSPACES_FILE);
        let written = write(folder.as_deref(), AUTO_FILE, &self.auto.to_text())
            .and_then(|()| self.save_command_bar(folder.as_deref()))
            .and_then(|()| write(folder.as_deref(), "item_bank.txt", &bank))
            .and_then(|()| {
                let presets: String = self
                    .canvas_presets
                    .iter()
                    .map(crate::dialogs::CanvasPreset::to_line)
                    .collect();
                write(folder.as_deref(), "canvas_presets.txt", &presets)
            })
            .and_then(|()| write(folder.as_deref(), &spaces_file, &spaces));
        if let Err(error) = written {
            cx.fail(tr!("cannot-keep-the-palettes-settings", error = error));
        }
        self.refresh_workspace_menu();
    }

    /// Shows the presets and the registered workspaces in Window,
    /// Workspace, and the one in use in Reload.
    pub(crate) fn refresh_workspace_menu(&mut self) {
        let names: Vec<String> = self
            .workspaces
            .iter()
            .map(|space| space.name.clone())
            .collect();
        crate::menus::set_workspaces_in_use(
            &mut self.menus,
            &names,
            self.current_workspace.as_deref(),
        );
    }

    /// The current palette layout as a workspace named `name`: the dock's
    /// lines, the palette switches, the menu bar, the layout style, how
    /// the strip opens its palettes, and the command bar's items. While
    /// Hide All Palettes hides them, the palettes as they were.
    pub(crate) fn workspace_now(&self, name: &str) -> Workspace {
        let (panels, dock) = self
            .hidden_all
            .as_ref()
            .map_or((&self.panels, &self.dock), |(panels, dock)| (panels, dock));
        let mut lines = dock.to_lines();
        lines.push(panels_line(panels));
        lines.push(format!("{MENUS_WORD} {}", u8::from(!self.menus_hidden)));
        lines.push(format!("{STYLE_WORD} {}", self.layout_style));
        if self.strip_as_tab {
            lines.push(format!("{STRIP_WORD} tab"));
        }
        lines.extend(self.bar_workspace_lines());
        Workspace {
            name: name.to_owned(),
            lines,
        }
    }

    /// Use Current Workspace as Default: the palettes as they are now are
    /// where Reset to Default goes back to, kept in the data folder.
    pub(crate) fn set_default_workspace(&mut self, cx: &mut AppletContext<'_>) {
        let space = self.workspace_now(&tr!("workspace"));
        let text = workspaces_text(std::slice::from_ref(&space));
        let folder = crate::colorsets::folder(cx);
        let file = crate::layout_style::file_for(self.layout_style, DEFAULT_FILE);
        match write(folder.as_deref(), &file, &text) {
            Ok(()) => cx.notify(tr!("default-workspace-saved")),
            Err(error) => cx.fail(tr!("cannot-keep-the-palettes-settings", error = error)),
        }
        // Kept for this run even when the folder cannot be written.
        self.default_workspace = Some(space);
    }

    /// Lays the palettes out as workspace `index` says; it becomes the one
    /// checked in Window, Workspace.
    pub(crate) fn use_workspace(&mut self, index: usize) {
        if let Some(space) = self.workspaces.get(index).cloned() {
            self.apply_layout(&space.lines);
            self.current_workspace = Some(space.name);
            self.refresh_workspace_menu();
        }
    }

    /// Reload: the workspace in use as it was registered.
    pub(crate) fn reload_workspace(&mut self) {
        if let Some(index) = self
            .current_workspace
            .as_ref()
            .and_then(|name| self.workspaces.iter().position(|space| space.name == *name))
        {
            self.use_workspace(index);
        }
    }

    /// Lays the palettes out as the preset workspace `id` says.
    pub(crate) fn use_preset(&mut self, id: &str) {
        if let Some(preset) = PRESETS.iter().find(|preset| preset.id == id) {
            self.hidden_all = None;
            let mut dock = self.style_layout();
            let mut panels = Panels::default();
            (preset.arrange)(&mut dock, &mut panels);
            self.dock = dock;
            self.panels = panels;
            self.menus_hidden = false;
            self.current_workspace = None;
            self.refresh_workspace_menu();
        }
    }

    /// The snapshot tour's routes into these palettes:
    /// `extras/show/<palette key>` shows a palette where the layout has
    /// it.
    pub(crate) fn extras_navigate(&mut self, target: &str) -> bool {
        use sue_wm::dock::DockPanel;
        match target.strip_prefix("extras/show/") {
            Some(key) => match crate::dock::Palette::named(key) {
                Some(palette) => {
                    self.show_palette(palette);
                    true
                }
                None => false,
            },
            None => false,
        }
    }

    /// Lays the palettes out as [`Self::workspace_now`]'s `lines` say (or
    /// such lines in another language); palettes the lines lack go where
    /// the layout style in use has them.
    pub(crate) fn apply_layout(&mut self, lines: &[String]) {
        self.hidden_all = None;
        if let Some(dock) = Self::read_layout(lines, &self.style_layout()) {
            self.dock = dock;
        }
        let mut bar = crate::bar_items::WorkspaceBar::default();
        self.strip_as_tab = false;
        for line in lines {
            if let Some(panels) = panels_from(line) {
                self.panels = panels;
            } else if let Some(shown) = menus_from(line) {
                self.menus_hidden = !shown;
            } else if let Some(how) = line.strip_prefix(&format!("{STRIP_WORD} ")) {
                self.strip_as_tab = how == "tab";
            } else {
                bar.read(line);
            }
        }
        bar.apply(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Tool;

    #[test]
    fn auto_actions_items_and_panels_round_trip_as_text() {
        let sets = AutoActions::builtin();
        assert_eq!(AutoActions::parse(&AutoActions::write(&sets)), sets);
        // Auto Actions keep what the registry flags as recordable.
        let flags = |name| crate::commands::registry().flags(name);
        assert!(flags("canvas.flip_h").has(sue_wm::Flags::RECORDABLE));
        assert!(!flags("file.open").has(sue_wm::Flags::RECORDABLE));
        assert!(!flags("auto.play").has(sue_wm::Flags::RECORDABLE));
        let item = Item::of(&Tool::Pen.sub_tools()[0]);
        assert_eq!(
            Item::from_line(&item.to_line()).map(|back| back.name),
            Some(item.name.clone())
        );
        let panels = Panels {
            timeline: false,
            tool: false,
            ..Panels::default()
        };
        assert_eq!(panels_from(&panels_line(&panels)), Some(panels));
    }

    #[test]
    fn workspace_lines_are_the_same_in_every_language() {
        let mut paint = SuePaint::default();
        paint.panels.navigator = false;
        paint.menus_hidden = true;
        paint
            .bar
            .items
            .push(crate::bar_items::Slot::command("layer.new"));
        let english = {
            let _english = sue_lang::scope(sue_lang::ENGLISH);
            paint.workspace_now("Inking").lines
        };
        for language in ["de", "fr", "ar", "ja"] {
            let _scope = sue_lang::scope(language);
            let lines = paint.workspace_now("Inking").lines;
            assert_eq!(lines, english, "{language}");
            // Read back in that language: the same palettes and bar.
            let mut back = SuePaint::default();
            back.apply_layout(&lines);
            assert_eq!(back.panels, paint.panels, "{language}");
            assert!(back.menus_hidden, "{language}");
            assert_eq!(back.bar, paint.bar, "{language}");
            assert_eq!(back.workspace_now("Inking").lines, lines, "{language}");
        }
        assert!(english.contains(&"panels 1111111011".to_owned()));
        assert!(english.contains(&"menus 0".to_owned()));
        assert!(english.contains(&"style classic".to_owned()));
        assert!(english.contains(&"bar command layer.new".to_owned()));
    }

    #[test]
    fn lines_written_in_another_language_are_read_once_and_kept_in_english() {
        let panels = Panels {
            timeline: false,
            ..Panels::default()
        };
        for line in [
            "Panels 1111111110",
            "cases 1111111110",
            "لوحات 1111111110",
            "パネル 1111111110",
            "panels 1111111110",
        ] {
            assert_eq!(panels_from(line), Some(panels), "{line}");
            assert_eq!(canonical_line(line), "panels 1111111110", "{line}");
        }
        for (line, shown) in [("Menüs 0", false), ("menu 1", true), ("菜单 0", false)] {
            assert_eq!(menus_from(line), Some(shown), "{line}");
        }
        assert_eq!(canonical_line("Menüs 0"), "menus 0");
        // Not a palette line: kept as it is.
        assert_eq!(canonical_line("Panels 12"), "Panels 12");
        assert_eq!(
            canonical_line("column left 46 1:tool"),
            "column left 46 1:tool"
        );
        // A registered workspace written in German.
        let text = "[Inking]\ncolumn left 46 1:tool\nPanels 1111111110\nMenüs 0\n";
        let (spaces, old) = workspaces_from(text);
        assert!(old);
        assert_eq!(
            spaces[0].lines,
            ["column left 46 1:tool", "panels 1111111110", "menus 0"]
        );
        let (_, old) = workspaces_from(&workspaces_text(&spaces));
        assert!(!old, "written again in English, read as it is");
    }

    #[test]
    fn the_old_words_are_what_each_language_wrote() {
        let languages: Vec<String> = sue_lang::languages()
            .into_iter()
            .map(|info| info.tag)
            .collect();
        for (language, panels, menus, hide, add) in OLD_WORDS {
            if !languages.iter().any(|known| known == language) {
                continue;
            }
            let _scope = sue_lang::scope(language);
            // The keys stay while a language still has them; once they are
            // gone the table alone reads old files.
            let wrote = |key: &str, name: &str, value: &str| {
                let text = sue_lang::text_with("paint", key, &[(name, value.into())]);
                (text != key).then_some(text)
            };
            let fits = |text: Option<String>, word: &str, value: &str| {
                text.is_none_or(|text| {
                    let expected = if word.starts_with(' ') {
                        format!("{value}{word}")
                    } else {
                        format!("{word}{value}")
                    };
                    text.trim() == expected.trim()
                })
            };
            assert!(
                fits(wrote("panels", "bits", "1"), panels, "1"),
                "{language}"
            );
            assert!(fits(wrote("menus", "value", "1"), menus, "1"), "{language}");
            assert!(fits(wrote("hide", "command", "x"), hide, "x"), "{language}");
            assert!(fits(wrote("add-3", "command", "x"), add, "x"), "{language}");
        }
    }

    #[test]
    fn an_emptied_workspace_stays_empty() {
        let mut paint = SuePaint::default();
        for &palette in <crate::dock::Palette as sue_wm::dock::DockPanel>::all() {
            paint.close_palette(palette);
        }
        let lines = paint.workspace_now("Empty").lines;
        let mut back = SuePaint::default();
        back.apply_layout(&lines);
        for &palette in <crate::dock::Palette as sue_wm::dock::DockPanel>::all() {
            assert!(!back.palette_showing(palette), "{palette:?} came back");
        }
        assert_eq!(back.workspace_now("Empty").lines, lines);
    }
}
