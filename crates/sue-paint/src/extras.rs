//! Palettes that keep work at hand: Auto Actions (menu commands recorded
//! and played back), the Item Bank (sub tools kept to add again), Info
//! (what is under the pointer) and registered workspaces (palette layouts
//! by name), each kept in Sue Paint's data folder, with the palette layout
//! as it was last left.

use std::path::Path;

use egui::{Align2, Rect, Sense, Ui, pos2};
use sue_wm::{AppletContext, Theme};

use crate::brush::Tip;
use crate::studio::{Panels, SuePaint};
use crate::tools::{SubTool, Tool};

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

/// A named list of menu commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AutoAction {
    pub name: String,
    pub steps: Vec<String>,
}

/// The auto actions and what is being recorded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AutoActions {
    pub sets: Vec<AutoAction>,
    pub selected: usize,
    /// The action commands are being recorded into.
    pub recording: Option<usize>,
    pub loaded: bool,
}

/// Named sections of lines: `[name]` then its lines.
fn sections(text: &str) -> Vec<(String, Vec<String>)> {
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

fn write_sections<'a>(items: impl Iterator<Item = (&'a str, Vec<String>)>) -> String {
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

impl AutoActions {
    /// Reads them from `text`.
    #[must_use]
    pub fn parse(text: &str) -> Vec<AutoAction> {
        sections(text)
            .into_iter()
            .map(|(name, steps)| AutoAction { name, steps })
            .collect()
    }

    /// Them as text.
    #[must_use]
    pub fn write(sets: &[AutoAction]) -> String {
        write_sections(
            sets.iter()
                .map(|set| (set.name.as_str(), set.steps.clone())),
        )
    }

    /// The actions a new install starts with.
    #[must_use]
    pub fn builtin() -> Vec<AutoAction> {
        vec![
            AutoAction {
                name: tr!("flip-canvas-and-fit"),
                steps: vec!["canvas.flip_h".to_owned(), "view.fit".to_owned()],
            },
            AutoAction {
                name: tr!("rotate-canvas-right"),
                steps: vec!["canvas.rotate_right".to_owned(), "view.fit".to_owned()],
            },
        ]
    }
}

/// A sub tool kept in the Item Bank.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Item {
    pub tool: Tool,
    pub name: String,
    pub tip: Tip,
    pub stabilization: u32,
}

impl Item {
    /// The item for `sub`.
    #[must_use]
    pub fn of(sub: &SubTool) -> Self {
        Self {
            tool: sub.tool,
            name: sub.name.clone(),
            tip: sub.tip,
            stabilization: sub.stabilization,
        }
    }

    /// One line: tool, name, stabilization and brush, tab separated.
    #[must_use]
    pub fn to_line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}",
            self.tool.label(),
            self.name.replace('\t', " "),
            self.stabilization,
            self.tip.to_text()
        )
    }

    /// An item from [`Self::to_line`].
    #[must_use]
    pub fn from_line(line: &str) -> Option<Self> {
        let mut parts = line.splitn(4, '\t');
        let label = parts.next()?;
        let tool = Tool::ALL.into_iter().find(|tool| tool.label() == label)?;
        Some(Self {
            tool,
            name: parts.next()?.to_owned(),
            stabilization: parts.next()?.parse().ok()?,
            tip: Tip::from_text(parts.next()?),
        })
    }
}

/// The file in Sue Paint's data folder that keeps the palette layout
/// between runs (never inside a document), for the default layout style
/// (another style keeps `layout.<id>.txt`, `layout_style::file_for`).
const LAYOUT_FILE: &str = "layout.txt";

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
        self.auto.sets = read(folder.as_deref(), "auto_actions.txt")
            .map_or_else(AutoActions::builtin, |text| AutoActions::parse(&text));
        self.item_bank = read(folder.as_deref(), "item_bank.txt")
            .map(|text| text.lines().filter_map(Item::from_line).collect())
            .unwrap_or_default();
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
        let written = write(
            folder.as_deref(),
            "auto_actions.txt",
            &AutoActions::write(&self.auto.sets),
        )
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

    /// Plays auto action `index`: each of its commands in turn.
    pub(crate) fn play_auto(&mut self, index: usize, cx: &mut AppletContext<'_>) {
        let Some(steps) = self.auto.sets.get(index).map(|set| set.steps.clone()) else {
            return;
        };
        let recording = self.auto.recording.take();
        for step in steps {
            self.run(&step, cx);
        }
        self.auto.recording = recording;
    }

    /// Keeps `command` in the auto action being recorded.
    pub(crate) fn record_auto(&mut self, command: &str) {
        if let Some(index) = self.auto.recording
            && crate::commands::registry()
                .flags(command)
                .has(sue_wm::Flags::RECORDABLE)
            && let Some(set) = self.auto.sets.get_mut(index)
        {
            set.steps.push(command.to_owned());
        }
    }

    /// The Auto Actions tab.
    pub(crate) fn auto_tab(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(body.shrink(6.0)));
        let mut changed = false;
        let mut play = None;
        inner.horizontal(|ui| {
            let recording = self.auto.recording.is_some();
            if ui
                .button(if recording {
                    tr!("stop")
                } else {
                    tr!("record")
                })
                .clicked()
            {
                self.auto.recording = if recording {
                    changed = true;
                    None
                } else {
                    self.auto
                        .sets
                        .get(self.auto.selected)
                        .map(|_| self.auto.selected)
                };
            }
            if ui.button(tr!("play")).clicked() {
                play = Some(self.auto.selected);
            }
            if ui.button(tr!("new-2")).clicked() {
                let name = tr!("auto-action", value = self.auto.sets.len() + 1);
                self.auto.sets.push(AutoAction {
                    name,
                    steps: Vec::new(),
                });
                self.auto.selected = self.auto.sets.len() - 1;
                self.auto.recording = Some(self.auto.selected);
                changed = true;
            }
            if ui.button(tr!("delete")).clicked() && self.auto.selected < self.auto.sets.len() {
                self.auto.sets.remove(self.auto.selected);
                self.auto.selected = self.auto.selected.saturating_sub(1);
                self.auto.recording = None;
                changed = true;
            }
        });
        if self.auto.recording.is_some() {
            inner.colored_label(theme.text, tr!("recording-run-commands-menus"));
        }
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-auto")
            .show(&mut inner, |ui| {
                for (index, set) in self.auto.sets.iter_mut().enumerate() {
                    let chosen = index == self.auto.selected;
                    let response = ui.selectable_label(
                        chosen,
                        tr!("steps", name = set.name, count = set.steps.len()),
                    );
                    if response.clicked() {
                        self.auto.selected = index;
                    }
                    if response.double_clicked() {
                        play = Some(index);
                    }
                    if chosen {
                        ui.indent("steps", |ui| {
                            if ui.text_edit_singleline(&mut set.name).lost_focus() {
                                changed = true;
                            }
                            let mut remove = None;
                            for (step, command) in set.steps.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    let label = crate::menus::label_of(&self.menus, command)
                                        .unwrap_or(command);
                                    ui.label(label);
                                    if ui.small_button("x").clicked() {
                                        remove = Some(step);
                                    }
                                });
                            }
                            if let Some(step) = remove {
                                set.steps.remove(step);
                                changed = true;
                            }
                        });
                    }
                }
            });
        if let Some(index) = play {
            self.play_auto(index, cx);
        }
        if changed {
            self.save_extras(cx);
        }
    }

    /// The Item Bank tab: sub tools kept to add to their tool again.
    pub(crate) fn item_bank_tab(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(body.shrink(6.0)));
        let mut changed = false;
        if inner.button(tr!("keep-current-sub-tool")).clicked() {
            self.item_bank.push(Item::of(self.toolbox.current()));
            changed = true;
        }
        if self.item_bank.is_empty() {
            inner.colored_label(theme.muted, tr!("kept-sub-tools-show-here-add"));
        }
        let mut add = None;
        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-bank")
            .show(&mut inner, |ui| {
                for (index, item) in self.item_bank.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} ({})", item.name, item.tool.label()));
                        if ui.small_button(tr!("add-2")).clicked() {
                            add = Some(index);
                        }
                        if ui.small_button("x").clicked() {
                            remove = Some(index);
                        }
                    });
                }
            });
        if let Some(index) = add
            && let Some(item) = self.item_bank.get(index).cloned()
        {
            let mut sub =
                SubTool::new(item.tool, &item.name, 0, item.tip).stabilization(item.stabilization);
            sub.name = item.name;
            self.toolbox.add_sub(sub);
        }
        if let Some(index) = remove {
            self.item_bank.remove(index);
            changed = true;
        }
        if changed {
            self.save_extras(cx);
        }
    }

    /// The Info tab: the canvas and what is under the pointer.
    pub(crate) fn info_tab(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let painter = ui.painter().with_clip_rect(body);
        let mut lines = Vec::new();
        let viewport = self.viewport;
        if let Some(doc) = self.doc() {
            let arena = &cx.world.arena;
            let entity = arena.get_by_id(doc.picture).ok();
            let size = entity.and_then(sue_model::picture::size).unwrap_or([0, 0]);
            let dpi = entity
                .and_then(|entity| entity.property(sue_model::picture::DPI))
                .and_then(sue_core::Value::as_f64)
                .unwrap_or(72.0);
            lines.push(tr!(
                "canvas-x-px-dpi",
                value = size[0],
                value2 = size[1],
                dpi = sue_lang::number(dpi, 0)
            ));
            lines.push(tr!(
                "print-size-x-cm",
                value = sue_lang::number(f64::from(size[0]) / dpi * 2.54, 1),
                value2 = sue_lang::number(f64::from(size[1]) / dpi * 2.54, 1)
            ));
            lines.push(tr!(
                "layers-2",
                count = crate::doc::layers(arena, doc.picture).len()
            ));
            lines.push(tr!(
                "zoom-percent-angle",
                value = sue_lang::number(f64::from(doc.view.zoom * 100.0), 0),
                rotation = sue_lang::number(f64::from(doc.view.rotation), 0)
            ));
            if let Some(pointer) = self.pointer.filter(|pointer| viewport.contains(*pointer)) {
                let at = doc.view.to_picture(viewport, size, pointer);
                lines.push(tr!(
                    "pointer",
                    x = sue_lang::number(f64::from(at.x), 0),
                    y = sue_lang::number(f64::from(at.y), 0)
                ));
                let (pixels, shown, _) = doc.compositor.display();
                if at.x >= 0.0
                    && at.y >= 0.0
                    && (at.x as u32) < shown[0]
                    && (at.y as u32) < shown[1]
                {
                    let index = ((at.y as u32 * shown[0] + at.x as u32) * 4) as usize;
                    if let Some(rgba) = pixels.get(index..index + 4) {
                        lines.push(tr!(
                            "color-r-g-b-a",
                            value = rgba[0],
                            value2 = rgba[1],
                            value3 = rgba[2],
                            value4 = rgba[3],
                            value5 = format!("{:02X}", rgba[0]),
                            value6 = format!("{:02X}", rgba[1]),
                            value7 = format!("{:02X}", rgba[2])
                        ));
                    }
                }
            } else {
                lines.push(tr!("pointer-off-the-canvas"));
            }
        } else {
            lines.push(tr!("no-canvas"));
        }
        let font = theme.font(theme.small_size + 0.5);
        for (row, line) in lines.iter().enumerate() {
            painter.text(
                pos2(body.left() + 10.0, body.top() + 10.0 + row as f32 * 18.0),
                Align2::LEFT_TOP,
                line,
                font.clone(),
                theme.text,
            );
        }
        // Keep it current while the pointer moves.
        let _ = ui.interact(body, ui.scope_id().with("sue-paint-info"), Sense::hover());
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        paint.bar_hidden.insert("edit.undo".to_owned());
        paint.bar_extra = vec!["view.fit".to_owned(), "layer.new".to_owned()];
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
            assert_eq!(back.bar_hidden, paint.bar_hidden, "{language}");
            assert_eq!(back.bar_extra, paint.bar_extra, "{language}");
            assert_eq!(back.workspace_now("Inking").lines, lines, "{language}");
        }
        assert!(english.contains(&"panels 1111111011".to_owned()));
        assert!(english.contains(&"menus 0".to_owned()));
        assert!(english.contains(&"style classic".to_owned()));
        assert!(english.contains(&"bar hide edit.undo".to_owned()));
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
