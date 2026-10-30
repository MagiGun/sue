//! Palettes that keep work at hand: Auto Actions (menu commands recorded
//! and played back), the Item Bank (sub tools kept to add again), Info
//! (what is under the pointer) and registered workspaces (palette layouts
//! by name), each kept in Sue Paint's data folder.

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
        tr!("color-wheel-color-slider-color-set"),
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

/// `true` for commands an auto action keeps: what changes the picture or
/// the view, not files, windows or the actions themselves.
#[must_use]
pub(crate) fn recordable(command: &str) -> bool {
    !["auto.", "file.", "window.", "help.", "story.editor"]
        .iter()
        .any(|prefix| command.starts_with(prefix))
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

/// A palette layout kept by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Workspace {
    pub name: String,
    pub lines: Vec<String>,
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
    tr!("panels", bits = bits)
}

/// Panels from [`panels_line`].
#[must_use]
pub(crate) fn panels_from(line: &str) -> Option<Panels> {
    let bits: Vec<bool> = line
        .strip_prefix("panels ")?
        .chars()
        .map(|c| c == '1')
        .collect();
    (bits.len() == 10).then(|| Panels {
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

fn read(folder: Option<&Path>, file: &str) -> Option<String> {
    sue_platform::fs::read_to_string(folder?.join(file)).ok()
}

fn write(folder: Option<&Path>, file: &str, text: &str) -> Result<(), String> {
    let folder = folder.ok_or(tr!("sue-paint-has-no-data-folder"))?;
    sue_platform::fs::create_dir_all(folder)
        .and_then(|()| sue_platform::fs::write(folder.join(file), text))
        .map_err(|error| error.to_string())
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
        if let Some(text) = read(folder.as_deref(), "command_bar.txt") {
            for line in text.lines() {
                if let Some(command) = line.strip_prefix("hide ") {
                    self.bar_hidden.insert(command.to_owned());
                } else if let Some(command) = line.strip_prefix("add ") {
                    self.bar_extra.push(command.to_owned());
                }
            }
        }
        self.workspaces = read(folder.as_deref(), "workspaces.txt")
            .map(|text| {
                sections(&text)
                    .into_iter()
                    .map(|(name, lines)| Workspace { name, lines })
                    .collect()
            })
            .unwrap_or_default();
        self.refresh_workspace_menu();
    }

    /// Writes the auto actions, the item bank and the workspaces.
    pub(crate) fn save_extras(&mut self, cx: &mut AppletContext<'_>) {
        let folder = crate::colorsets::folder(cx);
        let bank: String = self
            .item_bank
            .iter()
            .map(|item| item.to_line() + "\n")
            .collect();
        let spaces = write_sections(
            self.workspaces
                .iter()
                .map(|space| (space.name.as_str(), space.lines.clone())),
        );
        let mut bar: Vec<String> = self
            .bar_hidden
            .iter()
            .map(|command| tr!("hide", command = command))
            .collect();
        bar.sort();
        bar.extend(
            self.bar_extra
                .iter()
                .map(|command| tr!("add-3", command = command)),
        );
        let written = write(
            folder.as_deref(),
            "auto_actions.txt",
            &AutoActions::write(&self.auto.sets),
        )
        .and_then(|()| write(folder.as_deref(), "command_bar.txt", &bar.concat()))
        .and_then(|()| write(folder.as_deref(), "item_bank.txt", &bank))
        .and_then(|()| write(folder.as_deref(), "workspaces.txt", &spaces));
        if let Err(error) = written {
            cx.fail(tr!("cannot-keep-the-palettes-settings", error = error));
        }
        self.refresh_workspace_menu();
    }

    /// Shows the registered workspaces in Window, Workspace.
    pub(crate) fn refresh_workspace_menu(&mut self) {
        let names: Vec<String> = self
            .workspaces
            .iter()
            .map(|space| space.name.clone())
            .collect();
        crate::menus::set_workspaces(&mut self.menus, &names);
    }

    /// The current palette layout as a workspace named `name`.
    pub(crate) fn workspace_now(&self, name: &str) -> Workspace {
        let mut lines = self.dock.to_lines();
        lines.push(panels_line(&self.panels));
        lines.push(tr!("menus", value = u8::from(!self.menus_hidden)));
        Workspace {
            name: name.to_owned(),
            lines,
        }
    }

    /// Lays the palettes out as workspace `index` says.
    pub(crate) fn use_workspace(&mut self, index: usize) {
        let Some(space) = self.workspaces.get(index).cloned() else {
            return;
        };
        if let Some(dock) = crate::dock::Dock::from_lines(&space.lines) {
            self.dock = dock;
        }
        for line in &space.lines {
            if let Some(panels) = panels_from(line) {
                self.panels = panels;
            }
            if let Some(shown) = line.strip_prefix("menus ") {
                self.menus_hidden = shown == "0";
            }
        }
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
            && recordable(command)
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
        assert!(
            recordable("canvas.flip_h") && !recordable("file.open") && !recordable("auto.play")
        );
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
}
