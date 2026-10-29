//! Drawing Sue Writer: the menu bar and toolbar, the binder, the text (or
//! the corkboard, or the outliner), the inspector, the status bar, the find
//! bar, composition mode, the start screen and the dialogs.

use std::collections::HashMap;

use egui::{
    Align2, Color32, CornerRadius, Id, Painter, Pos2, Rect, RichText, Sense, Stroke, StrokeKind,
    Ui, UiBuilder, Vec2, pos2, vec2,
};
use sue_core::{Entity, EntityId, Value};
use sue_model::manuscript::{self, role};
use sue_model::{class, mark_kind as kind, picture};
use sue_type::{EditorView, Ink, ViewMode};
use sue_wm::{AppletContext, AppletId, ArenaEdit, ItemState, ShellRequest, Theme, widgets};

use crate::compile::{FORMATS, Separator, Titles};
use crate::project::{self, LABELS, STATUSES, TEMPLATES};
use crate::studio::{Dialog, Figure, ID, PAGES, Settings, SueWriter, View};

const TOOLBAR_HEIGHT: f32 = 36.0;
const STATUS_HEIGHT: f32 = 24.0;
const FIND_HEIGHT: f32 = 34.0;

fn rgb(color: [u8; 3]) -> Color32 {
    Color32::from_rgb(color[0], color[1], color[2])
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl SueWriter {
    /// Draws the studio.
    pub(crate) fn frame(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.bound.refresh(
            &mut self.menus,
            cx.rice(),
            crate::studio::KEYS_SECTION,
            crate::studio::shortcut_table(),
        );
        let present = cx.view.page.as_ref().map(AppletId::as_str) == Some(ID);
        let reveal = cx.view.reveal.clamp(0.0, 1.0);
        self.present = present;
        self.ready = present && reveal >= 0.999;
        self.reveal = reveal;
        if present && !self.shown {
            self.shown = true;
            if let Some(root) = self
                .project
                .filter(|root| cx.arena().get_by_id(*root).is_ok())
                && self.session.is_none()
            {
                let words = manuscript::total_words(cx.arena(), root);
                self.session = Some((root, words, project::today()));
            }
        }
        // What was open may have been undone away.
        if self
            .project
            .is_some_and(|root| cx.arena().get_by_id(root).is_err())
        {
            self.project = None;
            self.open = None;
        }
        if self
            .open
            .is_some_and(|open| cx.arena().get_by_id(open).is_err())
        {
            self.open = self
                .project
                .and_then(|root| manuscript::folder(cx.arena(), root, role::DRAFT));
        }
        if self
            .selected
            .is_some_and(|id| cx.arena().get_by_id(id).is_err())
        {
            self.selected = self.open;
        }
        let ctx = ui.ctx().clone();
        if self.ready {
            self.keys(&ctx, cx);
        }
        let theme = cx.theme.clone();
        let full = ui.max_rect();
        // Coming in, the page turns opaque first; the bars and panels slide
        // in from the edges around it. Going out, the other way.
        let hide = 1.0 - smooth(reveal);
        let cover = smooth(reveal / 0.35);
        ui.painter()
            .rect_filled(full, 0.0, theme.void.gamma_multiply(cover));
        if self.composition {
            ui.set_opacity(cover.max(0.02));
            self.composition_ui(ui, full, cx, &theme);
            self.dialog_ui(&ctx, cx, &theme);
            return;
        }
        let simple = self.simple(cx);
        let places = self.places(full, simple, self.project.is_some(), self.open.is_some());
        // The text first, so the panels sliding in pass over it.
        {
            let mut center = ui.new_child(UiBuilder::new().max_rect(places.center));
            center.set_clip_rect(places.center.intersect(ui.clip_rect()));
            center.set_opacity(cover.max(0.02));
            self.center_ui(&mut center, places.center, cx, &theme);
        }
        // The menus are in Sue's top bar (`menu_bar`); the toolbar slides
        // down from the page's top.
        let lift = hide * (places.body.top() - full.top());
        let toolbar = Rect::from_min_size(
            pos2(full.left(), full.top() - lift),
            vec2(full.width(), self.toolbar_height),
        );
        self.toolbar(ui, toolbar, cx, &theme);
        if self.find.is_some() {
            let find = Rect::from_min_size(
                pos2(full.left(), toolbar.bottom()),
                vec2(full.width(), FIND_HEIGHT),
            );
            self.find_bar(ui, find, cx, &theme);
        }
        let body = places.body;
        if let Some(rect) = places.binder {
            let slid = rect.translate(vec2(-hide * (rect.width() + 2.0), 0.0));
            self.binder_ui(ui, slid, cx, &theme);
            if hide < 0.01
                && let Some(width) = splitter(
                    ui,
                    "writer-binder-edge",
                    rect.right(),
                    body.y_range(),
                    &theme,
                )
            {
                self.binder_width = (width - body.left()).clamp(150.0, 480.0);
            }
        }
        if let Some(rect) = places.inspector {
            let slid = rect.translate(vec2(hide * (rect.width() + 2.0), 0.0));
            self.inspector_ui(ui, slid, cx, &theme);
            if hide < 0.01
                && let Some(x) = splitter(
                    ui,
                    "writer-inspector-edge",
                    rect.left(),
                    body.y_range(),
                    &theme,
                )
            {
                self.inspector_width = (body.right() - x).clamp(190.0, 520.0);
            }
        }
        let status = places.status.translate(vec2(0.0, hide * STATUS_HEIGHT));
        self.status_ui(ui, status, cx, &theme);
        self.dialog_ui(&ctx, cx, &theme);
    }

    /// Sue Writer's menus, in Sue's top bar after the Sue button (none in
    /// composition mode, which keeps only the text).
    pub(crate) fn menu_bar(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        if self.composition {
            return;
        }
        let theme = cx.theme.clone();
        let rect = ui.max_rect();
        let picked = {
            let mut menubar = std::mem::take(&mut self.menubar);
            let state = |command: &str| self.item_state(command, cx);
            let picked = menubar.ui(ui, rect, &self.menus, &state, &theme);
            self.menubar = menubar;
            picked
        };
        if let Some(command) = picked {
            self.run(&command, cx);
        }
    }

    /// Where everything goes in `full`: the bars, the binder and inspector
    /// (when they fit), and the text between them.
    pub(crate) fn places(
        &self,
        full: Rect,
        simple: bool,
        has_project: bool,
        has_open: bool,
    ) -> Places {
        let top = full.top()
            + self.toolbar_height
            + if self.find.is_some() {
                FIND_HEIGHT
            } else {
                0.0
            };
        let status = Rect::from_min_max(pos2(full.left(), full.bottom() - STATUS_HEIGHT), full.max);
        let body = Rect::from_min_max(pos2(full.left(), top), pos2(full.right(), status.top()));
        let mut center = body;
        let mut binder = None;
        if self.binder && !simple && has_project && body.width() > 520.0 {
            let width = self
                .binder_width
                .clamp(150.0, (body.width() * 0.4).max(150.0));
            let rect = Rect::from_min_max(body.min, pos2(body.left() + width, body.bottom()));
            center.min.x = rect.right() + 1.0;
            binder = Some(rect);
        }
        // The inspector folds away before the text gets too narrow to write in
        // (and a phone's narrow page never turns its bounds around).
        let width = self
            .inspector_width
            .clamp(190.0, (body.width() * 0.35).max(190.0));
        let mut inspector = None;
        if self.inspector && center.width() - width >= 520.0 && (has_project || has_open) {
            let rect = Rect::from_min_max(pos2(body.right() - width, body.top()), body.max);
            center.max.x = rect.left() - 1.0;
            inspector = Some(rect);
        }
        Places {
            body,
            status,
            binder,
            inspector,
            center,
        }
    }

    /// Whether a menu item works, and its check mark.
    fn item_state(&self, command: &str, cx: &AppletContext<'_>) -> ItemState {
        let has_project = self.project.is_some();
        let has_text = self.editor.current().is_some();
        match command {
            "view.page" => ItemState::check(self.paged && self.view == View::Document),
            "view.draft" => ItemState::check(!self.paged && self.view == View::Document),
            "view.document" => ItemState::check(self.view == View::Document),
            "view.corkboard" => ItemState {
                enabled: has_project,
                checked: Some(self.view == View::Corkboard),
            },
            "view.outliner" => ItemState {
                enabled: has_project,
                checked: Some(self.view == View::Outliner),
            },
            "view.composition" => ItemState::check(self.composition),
            "view.typewriter" => ItemState::check(self.typewriter),
            "view.binder" => ItemState {
                enabled: has_project,
                checked: Some(self.binder),
            },
            "view.inspector" => ItemState::check(self.inspector),
            "project.include" => ItemState {
                enabled: self.selected.is_some(),
                checked: self
                    .selected
                    .and_then(|id| cx.arena().get_by_id(id).ok())
                    .map(manuscript::included),
            },
            "binder.new_doc"
            | "binder.new_folder"
            | "project.duplicate"
            | "project.trash"
            | "project.empty_trash"
            | "project.merge"
            | "project.statistics"
            | "project.settings" => {
                if has_project {
                    ItemState::on()
                } else {
                    ItemState::off()
                }
            }
            command if command.starts_with("status.") || command.starts_with("label.") => {
                if has_project {
                    ItemState::on()
                } else {
                    ItemState::off()
                }
            }
            command
                if command.starts_with("format.")
                    || command.starts_with("align.")
                    || command.starts_with("insert.")
                    || command.starts_with("style.")
                    || command.starts_with("list.")
                    || command.starts_with("indent.")
                    || command.starts_with("spacing.")
                    || command == "project.split"
                    || command == "project.snapshot" =>
            {
                if has_text {
                    ItemState::on()
                } else {
                    ItemState::off()
                }
            }
            command if command.starts_with("compile.") => {
                if has_project || has_text {
                    ItemState::on()
                } else {
                    ItemState::off()
                }
            }
            _ => ItemState::on(),
        }
    }

    // ----- Toolbar -------------------------------------------------------------

    #[allow(clippy::too_many_lines, reason = "one row of controls")]
    fn toolbar(&mut self, ui: &mut Ui, rect: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        ui.painter().rect_filled(rect, 0.0, theme.panel);
        ui.painter()
            .hline(rect.x_range(), rect.bottom(), theme.hairline());
        let sheet = self.sheet(cx);
        let (para, chars) = self.editor.look_here(&sheet);
        let text_on = self.editor.current().is_some() && self.view == View::Document;
        let mut run: Option<String> = None;
        let inner = rect.shrink2(vec2(8.0, 4.0));
        // The views at the right first; the formatting scrolls in the rest.
        // The views sit on the first row.
        let first_row = Rect::from_min_size(inner.min, vec2(inner.width(), TOOLBAR_HEIGHT - 8.0));
        let mut right = ui.new_child(
            UiBuilder::new()
                .max_rect(first_row)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        right.set_clip_rect(rect.intersect(ui.clip_rect()));
        {
            let ui = &mut right;
            ui.spacing_mut().item_spacing.x = 4.0;
            let options = ["Page", "Draft", "Cards", "Outline"].map(String::from);
            let chosen = match (self.view, self.paged) {
                (View::Document, true) => 0,
                (View::Document, false) => 1,
                (View::Corkboard, _) => 2,
                (View::Outliner, _) => 3,
            };
            if let Some(index) = widgets::segmented(ui, &options, Some(chosen), theme) {
                run = Some(
                    ["view.page", "view.draft", "view.corkboard", "view.outliner"][index]
                        .to_owned(),
                );
            }
            if widgets::pill_button(ui, "Find", self.find.is_some(), theme)
                .on_hover_text("Find and replace (Ctrl+F)")
                .clicked()
            {
                if self.find.is_some() {
                    self.find = None;
                } else {
                    run = Some("edit.find".to_owned());
                }
            }
            if widgets::pill_button(ui, "Focus", false, theme)
                .on_hover_text("Composition mode: only the text (F11)")
                .clicked()
            {
                run = Some("view.composition".to_owned());
            }
        }
        let taken = right.min_rect().width() + 12.0;
        let left = Rect::from_min_max(
            inner.min,
            pos2((inner.right() - taken).max(inner.left()), inner.bottom()),
        );
        let mut row = ui.new_child(
            UiBuilder::new()
                .max_rect(left)
                .layout(egui::Layout::left_to_right(egui::Align::Min)),
        );
        row.set_clip_rect(
            Rect::from_x_y_ranges(left.x_range(), rect.y_range()).intersect(ui.clip_rect()),
        );
        let ui = &mut row;
        // The formatting wraps onto a second row when the studio is narrow.
        ui.horizontal_wrapped(|ui| {
            {
                {
                    ui.spacing_mut().item_spacing = vec2(4.0, 6.0);
                    ui.add_enabled_ui(text_on, |ui| {
                        // Paragraph style.
                        egui::ComboBox::from_id_salt("writer-style")
                            .selected_text(sheet.get(&para.style).name.clone())
                            .width(132.0)
                            .show_ui(ui, |ui| {
                                for style in &sheet.styles {
                                    let label = RichText::new(&style.name).size(
                                        theme.ui_size
                                            + if style.outline.is_some() { 1.5 } else { 0.0 },
                                    );
                                    if ui
                                        .selectable_label(style.key == para.style, label)
                                        .clicked()
                                    {
                                        run = Some(format!("style.{}", style.key));
                                    }
                                }
                            });
                        // Font.
                        let font = chars.font.clone().unwrap_or_else(|| para.font.clone());
                        let font_id = Id::unique("writer-font-filter");
                        egui::ComboBox::from_id_salt("writer-font")
                            .selected_text(font.clone())
                            .width(150.0)
                            .height(420.0)
                            .show_ui(ui, |ui| {
                                let mut filter: String =
                                    ui.data(|data| data.get_temp(font_id)).unwrap_or_default();
                                ui.add(
                                    egui::TextEdit::singleline(&mut filter)
                                        .hint_text("Search fonts")
                                        .desired_width(200.0),
                                );
                                ui.data_mut(|data| data.insert_temp(font_id, filter.clone()));
                                if !sue_wm::fonts::ready() {
                                    ui.label(
                                        RichText::new("Finding installed fonts...")
                                            .color(theme.muted),
                                    );
                                }
                                if ui
                                    .selectable_label(chars.font.is_none(), "Style's font")
                                    .clicked()
                                {
                                    self.editor.set_char(cx, kind::FONT, None, "Font");
                                }
                                let needle = filter.to_lowercase();
                                for family in sue_wm::fonts::families() {
                                    if !needle.is_empty()
                                        && !family.to_lowercase().contains(&needle)
                                    {
                                        continue;
                                    }
                                    if ui.selectable_label(family == font, &family).clicked() {
                                        self.editor.set_char(
                                            cx,
                                            kind::FONT,
                                            Some(Value::Text(family.clone())),
                                            "Font",
                                        );
                                    }
                                }
                            });
                        // Size.
                        let size = chars.size.unwrap_or(para.size);
                        egui::ComboBox::from_id_salt("writer-size")
                            .selected_text(format!("{size:.0}"))
                            .width(48.0)
                            .show_ui(ui, |ui| {
                                for option in [
                                    8.0_f32, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0,
                                    28.0, 32.0, 36.0, 48.0, 72.0,
                                ] {
                                    if ui
                                        .selectable_label(
                                            (option - size).abs() < 0.1,
                                            format!("{option:.0}"),
                                        )
                                        .clicked()
                                    {
                                        self.editor.set_char(
                                            cx,
                                            kind::SIZE,
                                            Some(Value::Float(f64::from(option))),
                                            "Size",
                                        );
                                    }
                                }
                            });
                        widgets::pill_divider(ui, theme);
                        for (label, command, on, hint) in [
                            ("B", "format.bold", chars.bold || para.bold, "Bold (Ctrl+B)"),
                            (
                                "I",
                                "format.italic",
                                chars.italic || para.italic,
                                "Italic (Ctrl+I)",
                            ),
                            (
                                "U",
                                "format.underline",
                                chars.underline,
                                "Underline (Ctrl+U)",
                            ),
                            (
                                "S",
                                "format.strike",
                                chars.strike,
                                "Strikethrough (Ctrl+Shift+X)",
                            ),
                            (
                                "x\u{00B2}",
                                "format.superscript",
                                chars.script == 1,
                                "Superscript",
                            ),
                            (
                                "x\u{2082}",
                                "format.subscript",
                                chars.script == -1,
                                "Subscript",
                            ),
                        ] {
                            let text = match label {
                                "B" => RichText::new(label).strong(),
                                "I" => RichText::new(label).italics(),
                                "U" => RichText::new(label).underline(),
                                "S" => RichText::new(label).strikethrough(),
                                _ => RichText::new(label),
                            };
                            if toggle(ui, text, on, theme).on_hover_text(hint).clicked() {
                                run = Some(command.to_owned());
                            }
                        }
                        // Text color and highlight.
                        let mut rgba = chars
                            .color
                            .map_or([0x18, 0x18, 0x18, 255], |[r, g, b]| [r, g, b, 255]);
                        let pick =
                            sue_wm::color_button(ui, "writer-color", &mut rgba, false, theme);
                        if pick.released || (pick.changed && !pick.dragging) {
                            let hex = format!("#{:02x}{:02x}{:02x}", rgba[0], rgba[1], rgba[2]);
                            self.editor.set_char(
                                cx,
                                kind::COLOR,
                                Some(Value::Text(hex)),
                                "Text Color",
                            );
                        }
                        ui.menu_button(
                            RichText::new("Highlight").size(theme.small_size + 1.0),
                            |ui| {
                                for (name, color) in [
                                    ("Yellow", "yellow"),
                                    ("Green", "green"),
                                    ("Blue", "blue"),
                                    ("Pink", "pink"),
                                    ("None", "none"),
                                ] {
                                    if ui.button(name).clicked() {
                                        run = Some(format!("format.highlight.{color}"));
                                        ui.close();
                                    }
                                }
                            },
                        );
                        widgets::pill_divider(ui, theme);
                        for (align, command, hint) in [
                            (sue_type::Align::Left, "align.left", "Align left (Ctrl+L)"),
                            (sue_type::Align::Center, "align.center", "Center (Ctrl+E)"),
                            (
                                sue_type::Align::Right,
                                "align.right",
                                "Align right (Ctrl+R)",
                            ),
                            (
                                sue_type::Align::Justify,
                                "align.justify",
                                "Justify (Ctrl+J)",
                            ),
                        ] {
                            if icon(ui, para.align == align, theme, |painter, rect, color| {
                                align_icon(painter, rect, color, align)
                            })
                            .on_hover_text(hint)
                            .clicked()
                            {
                                run = Some(command.to_owned());
                            }
                        }
                        widgets::pill_divider(ui, theme);
                        let bullet = para.list == Some(sue_type::ListKind::Bullet);
                        let number = para.list == Some(sue_type::ListKind::Number);
                        if icon(ui, bullet, theme, |painter, rect, color| {
                            list_icon(painter, rect, color, false)
                        })
                        .on_hover_text("Bulleted list (Ctrl+Shift+L)")
                        .clicked()
                        {
                            run = Some("list.bullet".to_owned());
                        }
                        if icon(ui, number, theme, |painter, rect, color| {
                            list_icon(painter, rect, color, true)
                        })
                        .on_hover_text("Numbered list")
                        .clicked()
                        {
                            run = Some("list.number".to_owned());
                        }
                        if icon(ui, false, theme, |painter, rect, color| {
                            indent_icon(painter, rect, color, false)
                        })
                        .on_hover_text("Decrease indent")
                        .clicked()
                        {
                            run = Some("indent.less".to_owned());
                        }
                        if icon(ui, false, theme, |painter, rect, color| {
                            indent_icon(painter, rect, color, true)
                        })
                        .on_hover_text("Increase indent")
                        .clicked()
                        {
                            run = Some("indent.more".to_owned());
                        }
                        egui::ComboBox::from_id_salt("writer-spacing")
                            .selected_text(
                                format!("{:.2}", para.line_spacing)
                                    .trim_end_matches('0')
                                    .trim_end_matches('.')
                                    .to_owned(),
                            )
                            .width(44.0)
                            .show_ui(ui, |ui| {
                                for (label, value) in [
                                    ("Single", "1"),
                                    ("1.15", "1.15"),
                                    ("1.5", "1.5"),
                                    ("Double", "2"),
                                ] {
                                    if ui.selectable_label(false, label).clicked() {
                                        run = Some(format!("spacing.{value}"));
                                    }
                                }
                            })
                            .response
                            .on_hover_text("Line spacing");
                    });
                }
            }
        });
        self.toolbar_height = (ui.min_rect().bottom() - rect.top() + 5.0).max(TOOLBAR_HEIGHT);
        if let Some(command) = run {
            self.run(&command, cx);
        }
    }

    // ----- Binder ----------------------------------------------------------------

    fn binder_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        ui.painter().rect_filled(rect, 0.0, theme.panel);
        let Some(root) = self.project else {
            return;
        };
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink(6.0))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let ui = &mut child;
        let mut run: Option<String> = None;
        ui.horizontal(|ui| {
            widgets::heading(ui, "Binder", theme);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::pill_button(ui, "+ Folder", false, theme)
                    .on_hover_text("New folder (Ctrl+Shift+N)")
                    .clicked()
                {
                    run = Some("binder.new_folder".to_owned());
                }
                if widgets::pill_button(ui, "+ Text", false, theme)
                    .on_hover_text("New document (Ctrl+N)")
                    .clicked()
                {
                    run = Some("binder.new_doc".to_owned());
                }
            });
        });
        ui.add_space(4.0);
        let mut moves: Vec<(EntityId, EntityId, usize)> = Vec::new();
        egui::ScrollArea::vertical()
            .id_salt("writer-binder")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let top: Vec<EntityId> = manuscript::children(cx.arena(), root);
                for id in top {
                    self.binder_row(ui, id, 0, cx, theme, &mut run, &mut moves);
                }
            });
        for (id, parent, index) in moves {
            if let Some(batch) = project::move_to(cx.arena(), id, parent, index) {
                self.apply(cx, batch);
                self.expanded.insert(parent);
            }
        }
        if let Some(command) = run {
            self.run(&command, cx);
        }
    }

    #[allow(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "one row of the binder, recursively"
    )]
    fn binder_row(
        &mut self,
        ui: &mut Ui,
        id: EntityId,
        depth: usize,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        run: &mut Option<String>,
        moves: &mut Vec<(EntityId, EntityId, usize)>,
    ) {
        let Ok(entity) = cx.arena().get_by_id(id) else {
            return;
        };
        let name = entity.name().to_owned();
        let folder = manuscript::is_folder(entity);
        let document = manuscript::is_document(entity);
        let image = picture::is_picture(entity)
            || entity
                .component(sue_core::ComponentKind::RasterTile2D)
                .is_some();
        let folder_role = entity
            .property(manuscript::ROLE)
            .and_then(Value::as_str)
            .map(str::to_owned);
        let included = manuscript::included(entity);
        let label = entity
            .property(manuscript::LABEL)
            .and_then(Value::as_str)
            .and_then(project::label_color);
        let children = manuscript::children(cx.arena(), id);
        let open = self.expanded.contains(&id);
        let height = 22.0;
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click_and_drag());
        let selected = self.selected == Some(id);
        let painter = ui.painter();
        if selected {
            painter.rect_filled(rect, theme.control_radius, theme.active);
        } else if response.hovered() {
            painter.rect_filled(rect, theme.control_radius, theme.hover);
        }
        let x = rect.left() + 4.0 + depth as f32 * 14.0;
        // The disclosure triangle.
        let arrow = Rect::from_center_size(pos2(x + 5.0, rect.center().y), vec2(10.0, 10.0));
        if !children.is_empty() {
            let color = theme.muted;
            let points = if open {
                vec![
                    arrow.left_top() + vec2(1.0, 2.5),
                    arrow.right_top() + vec2(-1.0, 2.5),
                    arrow.center_bottom() - vec2(0.0, 2.0),
                ]
            } else {
                vec![
                    arrow.left_top() + vec2(2.5, 1.0),
                    arrow.left_bottom() + vec2(2.5, -1.0),
                    arrow.right_center() - vec2(1.5, 0.0),
                ]
            };
            painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
        }
        let glyph = Rect::from_center_size(pos2(x + 20.0, rect.center().y), vec2(13.0, 13.0));
        let ink = if included { theme.text } else { theme.muted };
        match folder_role.as_deref() {
            Some(role::TRASH) => trash_icon(painter, glyph, ink),
            Some(role::DRAFT) => book_icon(painter, glyph, ink),
            _ if folder => folder_icon(painter, glyph, ink),
            _ if image => picture_icon(painter, glyph, ink),
            _ => page_icon(
                painter,
                glyph,
                ink,
                document
                    && entity
                        .string_buffer()
                        .is_some_and(|text| text.as_str().trim().is_empty()),
            ),
        }
        let text_left = x + 32.0;
        let renaming = self
            .renaming
            .as_ref()
            .filter(|(known, _)| *known == id)
            .map(|(_, text)| text.clone());
        if let Some(mut text) = renaming {
            let field = Rect::from_min_max(
                pos2(text_left, rect.top() + 1.0),
                pos2(rect.right() - 4.0, rect.bottom() - 1.0),
            );
            let edit = ui.put(
                field,
                egui::TextEdit::singleline(&mut text).id(Id::unique(("writer-rename", id))),
            );
            if !edit.has_focus() && !edit.lost_focus() {
                edit.request_focus();
            }
            let done = edit.lost_focus();
            let escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
            if done {
                self.renaming = None;
                if !escape && !text.trim().is_empty() && text.trim() != name {
                    let _ = cx.apply(ArenaEdit(project::rename(id, text.trim())));
                }
            } else {
                self.renaming = Some((id, text));
            }
        } else {
            let font = theme.font(theme.ui_size);
            let galley = ui.painter().layout_no_wrap(name.clone(), font, ink);
            let clip = Rect::from_min_max(
                pos2(text_left, rect.top()),
                pos2(rect.right() - 14.0, rect.bottom()),
            );
            ui.painter().with_clip_rect(clip).galley(
                pos2(text_left, rect.center().y - galley.size().y / 2.0),
                galley,
                ink,
            );
        }
        if let Some(color) = label {
            ui.painter()
                .circle_filled(pos2(rect.right() - 8.0, rect.center().y), 3.5, rgb(color));
        }
        // Clicks: the arrow folds, a click selects and opens, a double
        // click renames, a right click offers the rest.
        if response.clicked() {
            let on_arrow = response
                .interact_pointer_pos()
                .is_some_and(|pos| arrow.expand(3.0).contains(pos));
            if on_arrow && !children.is_empty() {
                if open {
                    self.expanded.remove(&id);
                } else {
                    self.expanded.insert(id);
                }
            } else {
                self.selected = Some(id);
                if document {
                    self.open = Some(id);
                    if self.view == View::Document || self.view == View::Corkboard {
                        self.view = View::Document;
                    }
                    self.editor.select(id, 0..0);
                } else {
                    // A folder shows its documents one after another; a
                    // picture in Research shows as itself.
                    self.open = Some(id);
                }
            }
        }
        if response.double_clicked() && folder_role.is_none() {
            self.renaming = Some((id, name.clone()));
        }
        response.context_menu(|ui| {
            self.selected = Some(id);
            fn item(ui: &mut Ui, label: &str, command: &str, run: &mut Option<String>) {
                if ui.button(label).clicked() {
                    *run = Some(command.to_owned());
                    ui.close();
                }
            }
            item(ui, "New Document", "binder.new_doc", run);
            item(ui, "New Folder", "binder.new_folder", run);
            ui.separator();
            if folder_role.is_none() {
                if ui.button("Rename").clicked() {
                    self.renaming = Some((id, name.clone()));
                    ui.close();
                }
                item(ui, "Duplicate", "project.duplicate", run);
                item(
                    ui,
                    if included {
                        "Leave Out of Compile"
                    } else {
                        "Include in Compile"
                    },
                    "project.include",
                    run,
                );
                if document {
                    item(ui, "Take Snapshot", "project.snapshot", run);
                    item(ui, "Merge With Next", "project.merge", run);
                }
                ui.menu_button("Label", |ui| {
                    for (label, color) in LABELS {
                        if ui
                            .button(RichText::new(format!("\u{25CF} {label}")).color(rgb(*color)))
                            .clicked()
                        {
                            *run = Some(format!("label.{label}"));
                            ui.close();
                        }
                    }
                    if ui.button("No Label").clicked() {
                        *run = Some("label.none".to_owned());
                        ui.close();
                    }
                });
                ui.menu_button("Status", |ui| {
                    for status in STATUSES {
                        if ui.button(*status).clicked() {
                            *run = Some(format!("status.{status}"));
                            ui.close();
                        }
                    }
                    if ui.button("No Status").clicked() {
                        *run = Some("status.none".to_owned());
                        ui.close();
                    }
                });
                if image && ui.button("Open in Sue Paint").clicked() {
                    if let Ok(paint) = AppletId::new("sue-paint") {
                        cx.request(ShellRequest::OpenIn {
                            entity: id,
                            applet: paint,
                        });
                    }
                    ui.close();
                }
                ui.separator();
                item(ui, "Move to Trash", "project.trash", run);
            }
            if folder_role.as_deref() == Some(role::TRASH) {
                item(ui, "Empty Trash", "project.empty_trash", run);
            }
        });
        // Dragging items onto others moves them: above or below an item, or
        // into a folder.
        if folder_role.is_none() {
            response.dnd_set_drag_payload(id);
        }
        if let (Some(dragged), Some(pointer)) = (
            response.dnd_hover_payload::<EntityId>(),
            ui.input(|input| input.pointer.hover_pos()),
        ) && *dragged != id
        {
            let zone = drop_zone(rect, pointer, folder);
            let color = theme.border_active;
            match zone {
                Zone::Into => {
                    ui.painter().rect_stroke(
                        rect,
                        theme.control_radius,
                        Stroke::new(1.5, color),
                        StrokeKind::Inside,
                    );
                }
                Zone::Before => {
                    ui.painter()
                        .hline(rect.x_range(), rect.top(), Stroke::new(2.0, color));
                }
                Zone::After => {
                    ui.painter()
                        .hline(rect.x_range(), rect.bottom(), Stroke::new(2.0, color));
                }
            }
        }
        if let (Some(dragged), Some(pointer)) = (
            response.dnd_release_payload::<EntityId>(),
            ui.input(|input| input.pointer.interact_pos()),
        ) {
            let dragged = *dragged;
            if dragged != id {
                match drop_zone(rect, pointer, folder) {
                    Zone::Into => moves.push((dragged, id, children.len())),
                    zone => {
                        if let Some(parent) = manuscript::parent(cx.arena(), id) {
                            let siblings: Vec<EntityId> = manuscript::children(cx.arena(), parent)
                                .into_iter()
                                .filter(|sibling| *sibling != dragged)
                                .collect();
                            let at = siblings
                                .iter()
                                .position(|sibling| *sibling == id)
                                .unwrap_or(0);
                            moves.push((
                                dragged,
                                parent,
                                if zone == Zone::After { at + 1 } else { at },
                            ));
                        }
                    }
                }
            }
        }
        if open {
            for child in children {
                self.binder_row(ui, child, depth + 1, cx, theme, run, moves);
            }
        }
    }

    // ----- The middle --------------------------------------------------------------

    fn center_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let ui = &mut child;
        if self.project.is_none() && self.open.is_none() {
            self.start_ui(ui, rect, cx, theme);
            return;
        }
        // A picture kept in Research shows as itself.
        if let Some(open) = self.open
            && cx.arena().get_by_id(open).is_ok_and(|entity| {
                !manuscript::is_document(entity)
                    && !manuscript::is_folder(entity)
                    && !manuscript::is_manuscript(entity)
                    && entity.string_buffer().is_none()
            })
            && self.view == View::Document
        {
            self.research_item_ui(ui, rect, open, cx, theme);
            return;
        }
        match self.view {
            View::Document => self.text_ui(ui, rect, cx, theme, false),
            View::Corkboard => self.corkboard_ui(ui, rect, cx, theme),
            View::Outliner => self.outliner_ui(ui, rect, cx, theme),
        }
    }

    /// Textures for the pictures the shown documents embed.
    fn figure_table(
        &mut self,
        docs: &[EntityId],
        cx: &mut AppletContext<'_>,
    ) -> HashMap<EntityId, (Option<egui::TextureId>, Vec2)> {
        let mut wanted = Vec::new();
        for doc in docs {
            let marks = sue_type::edit::marks_of(cx.arena(), *doc);
            for mark in marks.marks() {
                if mark.kind == kind::EMBED
                    && let Value::Entity(id) = mark.value
                {
                    wanted.push(id);
                }
            }
        }
        let mut table = HashMap::new();
        for id in wanted {
            let seen = if cx.arena().get_by_id(id).is_ok_and(picture::is_picture) {
                sue_paint::composite::signature(cx.arena(), id)
                    .iter()
                    .map(|(_, revision)| *revision)
                    .sum::<u64>()
            } else {
                cx.arena().get_by_id(id).map_or(0, Entity::revision)
            };
            let fresh = self
                .figures
                .get(&id)
                .is_some_and(|figure| figure.seen == seen);
            if !fresh
                && let Some((pixels, [width, height])) =
                    crate::compile::picture_pixels(cx.arena(), id, 1200)
            {
                self.figures.insert(
                    id,
                    Figure {
                        texture: None,
                        size: vec2(width as f32, height as f32) * 0.75,
                        seen,
                        pixels,
                        pixel_size: [width, height],
                    },
                );
            }
            if let Some(figure) = self.figures.get_mut(&id) {
                // Every frame: an unused texture is let go.
                figure.texture = cx.textures.texture(
                    texture_key(id),
                    figure.seen,
                    figure.pixel_size,
                    4,
                    &figure.pixels,
                    false,
                );
                table.insert(id, (figure.texture, figure.size));
            }
        }
        table
    }

    fn text_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        composing: bool,
    ) {
        let docs = self.shown_docs(cx);
        if docs.is_empty() {
            ui.scope_builder(UiBuilder::new().max_rect(rect.shrink(40.0)), |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    ui.label(
                        RichText::new("Nothing to write in here yet.")
                            .size(theme.heading_size)
                            .color(theme.muted),
                    );
                    ui.add_space(8.0);
                    if widgets::pill_button(ui, "New Document", false, theme).clicked() {
                        self.add_item(cx, false);
                    }
                });
            });
            return;
        }
        let sheet = self.sheet(cx);
        let page = self.page(cx);
        let titles: Vec<String> = docs
            .iter()
            .map(|id| {
                cx.arena()
                    .get_by_id(*id)
                    .map_or_else(|_| String::new(), |entity| entity.name().to_owned())
            })
            .collect();
        let figures = self.figure_table(&docs, cx);
        let lookup = |id: EntityId| figures.get(&id).copied();
        let dark = cx.rice().get_bool("sue_writer.dark_paper").unwrap_or(false) || composing;
        let ink = if dark {
            Ink {
                paper: if composing {
                    Color32::from_gray(0x1c)
                } else {
                    theme.pill
                },
                text: if composing {
                    Color32::from_gray(0xd8)
                } else {
                    theme.text
                },
                soft: theme.muted,
                link: Color32::from_rgb(0x7a, 0xb0, 0xff),
                shade: theme.hover,
                colors: false,
            }
        } else {
            Ink::paper()
        };
        // The page fits the space, until people zoom themselves.
        let (mode, across) = if self.paged && !composing {
            (ViewMode::Page, page.size[0])
        } else if composing {
            (ViewMode::Draft { width: 560.0 }, 600.0)
        } else {
            (
                ViewMode::Draft {
                    width: page.text_width(),
                },
                page.text_width() + 60.0,
            )
        };
        // Never smaller than can be read: past that the page scrolls across.
        let fit = ((rect.width() - 56.0) / (across * sue_type::PX_PER_PT)).clamp(0.6, 3.0);
        let zoom = if self.fit {
            fit.min(if composing { 1.3 } else { 1.0 })
        } else {
            self.zoom
        };
        self.shown_zoom = zoom;
        let smart_quotes = cx
            .rice()
            .get_bool("sue_writer.smart_quotes")
            .unwrap_or(true);
        let view = EditorView {
            mode,
            page: &page,
            zoom,
            sheet: &sheet,
            ink,
            desk: if composing {
                Color32::from_gray(0x12)
            } else if dark {
                theme.void
            } else {
                desk(theme)
            },
            figures: &lookup,
            typewriter: self.typewriter || composing,
            smart_quotes,
            read_only: false,
            titles: &titles,
        };
        let mut area = ui.new_child(UiBuilder::new().max_rect(rect));
        let output = self.editor.show(&mut area, cx, &docs, &view);
        if let Some(factor) = output.zoom {
            self.fit = false;
            self.zoom = (zoom * factor).clamp(0.25, 4.0);
        }
        if let Some(link) = output.link {
            cx.request(ShellRequest::Command(format!("open-url:{link}")));
        }
        self.page_info = (output.page, output.pages);
        if let Some(current) = self.editor.current()
            && self.selected != Some(current)
            && docs.len() > 1
            && output.focused
        {
            self.selected = Some(current);
        }
    }

    fn research_item_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        id: EntityId,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let seen = cx.arena().get_by_id(id).map_or(0, Entity::revision);
        if self
            .figures
            .get(&id)
            .is_none_or(|figure| figure.seen != seen)
            && let Some((pixels, [width, height])) =
                crate::compile::picture_pixels(cx.arena(), id, 1600)
        {
            self.figures.insert(
                id,
                Figure {
                    texture: None,
                    size: vec2(width as f32, height as f32),
                    seen,
                    pixels,
                    pixel_size: [width, height],
                },
            );
        }
        if let Some(figure) = self.figures.get_mut(&id) {
            figure.texture = cx.textures.texture(
                texture_key(id),
                figure.seen,
                figure.pixel_size,
                4,
                &figure.pixels,
                false,
            );
        }
        ui.painter().rect_filled(rect, 0.0, theme.void);
        if let Some(figure) = self.figures.get(&id)
            && let Some(texture) = figure.texture
        {
            let room = rect.shrink(40.0);
            let scale = (room.width() / figure.size.x)
                .min(room.height() / figure.size.y)
                .min(1.0);
            let shown = Rect::from_center_size(room.center(), figure.size * scale);
            ui.painter().image(
                texture,
                shown,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let name = cx
            .arena()
            .get_by_id(id)
            .map_or_else(|_| String::new(), |entity| entity.name().to_owned());
        ui.painter().text(
            rect.left_top() + vec2(16.0, 14.0),
            Align2::LEFT_TOP,
            name,
            theme.font(theme.ui_size),
            theme.muted,
        );
    }

    fn start_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        ui.painter().rect_filled(rect, 0.0, theme.void);
        let width = rect.width().min(860.0);
        let area = Rect::from_min_size(
            pos2(rect.center().x - width / 2.0, rect.top() + 36.0),
            vec2(width, rect.height() - 48.0),
        );
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(area)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let ui = &mut child;
        ui.label(
            RichText::new("Sue Writer")
                .size(theme.heading_size * 1.4)
                .color(theme.text),
        );
        ui.label(
            RichText::new("Start something new, or open a project from this Sue project.")
                .size(theme.small_size + 1.0)
                .color(theme.muted),
        );
        ui.add_space(18.0);
        widgets::heading(ui, "New", theme);
        let card = vec2(196.0, 86.0);
        let columns = ((width + 10.0) / (card.x + 10.0)).floor().max(1.0) as usize;
        let mut chosen = None;
        for row in TEMPLATES.chunks(columns) {
            ui.horizontal(|ui| {
                for (key, label, about) in row {
                    let (rect, response) = ui.allocate_exact_size(card, Sense::click());
                    let fill = if response.hovered() {
                        theme.hover
                    } else {
                        theme.pill
                    };
                    ui.painter()
                        .rect_filled(rect, theme.control_radius * 2.0, fill);
                    ui.painter().rect_stroke(
                        rect,
                        theme.control_radius * 2.0,
                        theme.hairline(),
                        StrokeKind::Inside,
                    );
                    ui.painter().text(
                        rect.left_top() + vec2(12.0, 10.0),
                        Align2::LEFT_TOP,
                        *label,
                        theme.font(theme.ui_size + 1.5),
                        theme.text,
                    );
                    let galley = ui.painter().layout(
                        about.to_string(),
                        theme.font(theme.small_size),
                        theme.muted,
                        card.x - 24.0,
                    );
                    ui.painter()
                        .galley(rect.left_top() + vec2(12.0, 34.0), galley, theme.muted);
                    if response.clicked() {
                        chosen = Some(*key);
                    }
                }
            });
            ui.add_space(6.0);
        }
        if let Some(key) = chosen {
            let label = TEMPLATES
                .iter()
                .find(|(known, ..)| *known == key)
                .map_or("Untitled", |(_, label, _)| *label);
            self.new_project(cx, key, label);
        }
        let projects: Vec<(EntityId, String, usize)> = cx
            .arena()
            .with_class(class::MANUSCRIPT)
            .filter_map(|handle| cx.arena().get(handle).ok())
            .map(|entity| {
                (
                    entity.id(),
                    entity.name().to_owned(),
                    manuscript::total_words(cx.arena(), entity.id()),
                )
            })
            .collect();
        if !projects.is_empty() {
            ui.add_space(14.0);
            widgets::heading(ui, "In this project", theme);
            for (id, name, words) in projects {
                if ui.button(format!("{name}    {words} words")).clicked() {
                    self.open_project(cx, id, None);
                }
            }
        }
        ui.add_space(14.0);
        if widgets::pill_button(ui, "Import a Document...", false, theme)
            .on_hover_text("Word, Markdown, HTML or plain text")
            .clicked()
        {
            self.run("file.import", cx);
        }
    }

    // ----- Corkboard and outliner -----------------------------------------------------

    fn corkboard_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        ui.painter().rect_filled(rect, 0.0, desk(theme));
        let Some(folder) = self.board_folder(cx) else {
            return;
        };
        let items = manuscript::children(cx.arena(), folder);
        let card = vec2(220.0, 150.0);
        let gap = 18.0;
        let mut moves = Vec::new();
        let mut open = None;
        egui::ScrollArea::vertical()
            .id_salt("writer-corkboard")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let width = ui.available_width();
                let columns = ((width - gap) / (card.x + gap)).floor().max(1.0) as usize;
                ui.add_space(gap);
                for (row, chunk) in items.chunks(columns).enumerate() {
                    ui.horizontal(|ui| {
                        ui.add_space(gap);
                        for (column, id) in chunk.iter().enumerate() {
                            let index = row * columns + column;
                            let Ok(entity) = cx.arena().get_by_id(*id) else {
                                continue;
                            };
                            let (rect, response) =
                                ui.allocate_exact_size(card, Sense::click_and_drag());
                            let selected = self.selected == Some(*id);
                            let painter = ui.painter();
                            painter.rect_filled(
                                rect.translate(vec2(0.0, 2.0)),
                                3.0,
                                Color32::from_black_alpha(50),
                            );
                            painter.rect_filled(rect, 3.0, Color32::from_rgb(0xfb, 0xf8, 0xef));
                            if selected {
                                painter.rect_stroke(
                                    rect,
                                    3.0,
                                    Stroke::new(2.0, Color32::from_rgb(0x3a, 0x7b, 0xd5)),
                                    StrokeKind::Outside,
                                );
                            }
                            if let Some(color) = entity
                                .property(manuscript::LABEL)
                                .and_then(Value::as_str)
                                .and_then(project::label_color)
                            {
                                painter.rect_filled(
                                    Rect::from_min_size(rect.min, vec2(card.x, 5.0)),
                                    CornerRadius {
                                        nw: 3,
                                        ne: 3,
                                        sw: 0,
                                        se: 0,
                                    },
                                    rgb(color),
                                );
                            }
                            let ink = Color32::from_gray(0x22);
                            let title = painter.layout(
                                entity.name().to_owned(),
                                egui::FontId::proportional(14.0),
                                ink,
                                card.x - 20.0,
                            );
                            painter.galley(rect.left_top() + vec2(10.0, 12.0), title, ink);
                            painter.hline(
                                rect.left() + 10.0..=rect.right() - 10.0,
                                rect.top() + 34.0,
                                Stroke::new(1.0, Color32::from_rgb(0xe0, 0xb0, 0xb0)),
                            );
                            let synopsis = entity
                                .property(manuscript::SYNOPSIS)
                                .and_then(Value::as_str)
                                .filter(|text| !text.is_empty())
                                .map(str::to_owned)
                                .or_else(|| {
                                    entity
                                        .string_buffer()
                                        .map(|text| text.as_str().chars().take(220).collect())
                                })
                                .unwrap_or_default();
                            let body = painter.layout(
                                synopsis,
                                egui::FontId::proportional(12.0),
                                Color32::from_gray(0x44),
                                card.x - 20.0,
                            );
                            painter
                                .with_clip_rect(Rect::from_min_max(
                                    rect.left_top() + vec2(0.0, 38.0),
                                    rect.right_bottom() - vec2(0.0, 22.0),
                                ))
                                .galley(rect.left_top() + vec2(10.0, 40.0), body, ink);
                            if let Some(status) =
                                entity.property(manuscript::STATUS).and_then(Value::as_str)
                            {
                                painter.text(
                                    rect.right_bottom() - vec2(10.0, 8.0),
                                    Align2::RIGHT_BOTTOM,
                                    status.to_uppercase(),
                                    egui::FontId::proportional(9.5),
                                    Color32::from_rgba_unmultiplied(0xb0, 0x30, 0x30, 0xa0),
                                );
                            }
                            let words = manuscript::total_words(cx.arena(), *id);
                            painter.text(
                                rect.left_bottom() + vec2(10.0, -8.0),
                                Align2::LEFT_BOTTOM,
                                format!("{words} words"),
                                egui::FontId::proportional(9.5),
                                Color32::from_gray(0x88),
                            );
                            if response.clicked() {
                                self.selected = Some(*id);
                            }
                            if response.double_clicked() {
                                open = Some(*id);
                            }
                            response.dnd_set_drag_payload(*id);
                            if let Some(dragged) = response.dnd_release_payload::<EntityId>()
                                && *dragged != *id
                            {
                                moves.push((*dragged, folder, index));
                            }
                            if response
                                .dnd_hover_payload::<EntityId>()
                                .is_some_and(|dragged| *dragged != *id)
                            {
                                ui.painter().vline(
                                    rect.left() - gap / 2.0,
                                    rect.y_range(),
                                    Stroke::new(3.0, theme.border_active),
                                );
                            }
                            ui.add_space(gap - ui.spacing().item_spacing.x);
                        }
                    });
                    ui.add_space(gap);
                }
                if items.is_empty() {
                    ui.label(
                        RichText::new(
                            "No cards here yet. New documents and folders appear as cards.",
                        )
                        .color(theme.muted),
                    );
                }
            });
        for (id, parent, index) in moves {
            if let Some(batch) = project::move_to(cx.arena(), id, parent, index) {
                self.apply(cx, batch);
            }
        }
        if let Some(id) = open {
            self.selected = Some(id);
            self.open = Some(id);
            self.view = View::Document;
            if cx.arena().get_by_id(id).is_ok_and(manuscript::is_document) {
                self.editor.select(id, 0..0);
            }
        }
    }

    fn outliner_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        ui.painter().rect_filled(rect, 0.0, theme.void);
        let Some(folder) = self.board_folder(cx) else {
            return;
        };
        let rows = manuscript::walk(cx.arena(), folder);
        let mut open = None;
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink(10.0))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let ui = &mut child;
        let columns = [
            ("Title", 0.34),
            ("Synopsis", 0.34),
            ("Label", 0.1),
            ("Status", 0.1),
            ("Words", 0.06),
            ("Compile", 0.06),
        ];
        let width = ui.available_width();
        ui.horizontal(|ui| {
            for (title, share) in columns {
                ui.add_sized(
                    vec2(width * share - 4.0, 18.0),
                    egui::Label::new(
                        RichText::new(title)
                            .size(theme.small_size)
                            .color(theme.muted),
                    ),
                );
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("writer-outliner")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (id, depth) in rows {
                    let Ok(entity) = cx.arena().get_by_id(id) else {
                        continue;
                    };
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(width, 22.0), Sense::click());
                    if self.selected == Some(id) {
                        ui.painter().rect_filled(rect, 2.0, theme.active);
                    } else if response.hovered() {
                        ui.painter().rect_filled(rect, 2.0, theme.hover);
                    }
                    let mut x = rect.left();
                    let cell =
                        |share: f32, text: String, color: Color32, x: &mut f32, indent: f32| {
                            let cell = Rect::from_min_size(
                                pos2(*x + indent, rect.top()),
                                vec2(width * share - indent - 6.0, rect.height()),
                            );
                            let galley = ui.painter().layout_no_wrap(
                                text,
                                theme.font(theme.small_size + 0.5),
                                color,
                            );
                            ui.painter().with_clip_rect(cell).galley(
                                pos2(cell.left(), rect.center().y - galley.size().y / 2.0),
                                galley,
                                color,
                            );
                            *x += width * share;
                        };
                    cell(
                        columns[0].1,
                        entity.name().to_owned(),
                        theme.text,
                        &mut x,
                        depth as f32 * 14.0,
                    );
                    let synopsis = entity
                        .property(manuscript::SYNOPSIS)
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .replace('\n', " ");
                    cell(columns[1].1, synopsis, theme.muted, &mut x, 0.0);
                    let label = entity
                        .property(manuscript::LABEL)
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    let label_color = project::label_color(&label).map_or(theme.muted, rgb);
                    cell(columns[2].1, label, label_color, &mut x, 0.0);
                    cell(
                        columns[3].1,
                        entity
                            .property(manuscript::STATUS)
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        theme.muted,
                        &mut x,
                        0.0,
                    );
                    let words = manuscript::total_words(cx.arena(), id);
                    cell(columns[4].1, words.to_string(), theme.muted, &mut x, 0.0);
                    cell(
                        columns[5].1,
                        if manuscript::included(entity) {
                            "\u{2713}".to_owned()
                        } else {
                            String::new()
                        },
                        theme.muted,
                        &mut x,
                        0.0,
                    );
                    if response.clicked() {
                        self.selected = Some(id);
                    }
                    if response.double_clicked() {
                        open = Some(id);
                    }
                }
            });
        if let Some(id) = open {
            self.open = Some(id);
            self.view = View::Document;
            if cx.arena().get_by_id(id).is_ok_and(manuscript::is_document) {
                self.editor.select(id, 0..0);
            }
        }
    }

    // ----- Inspector -----------------------------------------------------------------

    /// A text property edited in place, committed when the field lets go.
    fn property_field(
        &mut self,
        ui: &mut Ui,
        id: EntityId,
        key: &'static str,
        rows: usize,
        hint: &str,
        cx: &mut AppletContext<'_>,
    ) {
        let stored = cx
            .arena()
            .get_by_id(id)
            .ok()
            .and_then(|entity| entity.property(key))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let draft = self
            .drafts
            .entry((id, key))
            .or_insert_with(|| stored.clone());
        let edit = if rows > 1 {
            egui::TextEdit::multiline(draft)
                .desired_rows(rows)
                .desired_width(f32::INFINITY)
                .hint_text(hint)
        } else {
            egui::TextEdit::singleline(draft)
                .desired_width(f32::INFINITY)
                .hint_text(hint)
        };
        let response = ui.add(edit);
        if response.lost_focus() {
            let text = draft.clone();
            self.drafts.remove(&(id, key));
            if text != stored {
                let value = (!text.is_empty()).then_some(Value::Text(text));
                let _ = cx.apply(ArenaEdit(project::set(id, key, value)));
            }
        } else if !response.has_focus() && *draft != stored {
            // Changed elsewhere (undo): show what is stored.
            *draft = stored;
        }
    }

    fn inspector_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        ui.painter().rect_filled(rect, 0.0, theme.panel);
        let target = self.selected.or(self.open);
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink(8.0))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let ui = &mut child;
        let mut run: Option<String> = None;
        egui::ScrollArea::vertical().id_salt("writer-inspector").auto_shrink([false, false]).show(ui, |ui| {
            let Some(id) = target else {
                return;
            };
            let Ok(entity) = cx.arena().get_by_id(id) else {
                return;
            };
            let name = entity.name().to_owned();
            let is_document = entity.string_buffer().is_some();
            let in_project = self.project.is_some();
            ui.label(RichText::new(name).size(theme.heading_size).color(theme.text));
            ui.add_space(6.0);
            if in_project {
                widgets::heading(ui, "Synopsis", theme);
                self.property_field(ui, id, manuscript::SYNOPSIS, 4, "What happens here, in a line or two", cx);
                ui.add_space(6.0);
                let entity = cx.arena().get_by_id(id).ok();
                let status = entity.and_then(|entity| entity.property(manuscript::STATUS)).and_then(Value::as_str).unwrap_or("No Status").to_owned();
                let label = entity.and_then(|entity| entity.property(manuscript::LABEL)).and_then(Value::as_str).unwrap_or("No Label").to_owned();
                let included = entity.is_some_and(manuscript::included);
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt(("writer-label", id)).selected_text(label.clone()).width(100.0).show_ui(ui, |ui| {
                        for (name, color) in LABELS {
                            if ui.selectable_label(label == *name, RichText::new(format!("\u{25CF} {name}")).color(rgb(*color))).clicked() {
                                run = Some(format!("label.{name}"));
                            }
                        }
                        if ui.selectable_label(false, "No Label").clicked() {
                            run = Some("label.none".to_owned());
                        }
                    });
                    egui::ComboBox::from_id_salt(("writer-status", id)).selected_text(status.clone()).width(100.0).show_ui(ui, |ui| {
                        for name in STATUSES {
                            if ui.selectable_label(status == *name, *name).clicked() {
                                run = Some(format!("status.{name}"));
                            }
                        }
                        if ui.selectable_label(false, "No Status").clicked() {
                            run = Some("status.none".to_owned());
                        }
                    });
                });
                let mut include = included;
                if ui.checkbox(&mut include, "Include in compile").changed() {
                    run = Some("project.include".to_owned());
                }
                let target_words = cx.arena().get_by_id(id).ok().and_then(|entity| entity.property(manuscript::TARGET)).and_then(Value::as_i64).unwrap_or(0);
                let mut words_target = target_words;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Word target").color(theme.muted));
                    let response = ui.add(egui::DragValue::new(&mut words_target).range(0..=10_000_000).speed(50));
                    if (response.drag_stopped() || response.lost_focus()) && words_target != target_words {
                        let value = (words_target > 0).then_some(Value::Int(words_target));
                        let _ = cx.apply(ArenaEdit(project::set(id, manuscript::TARGET, value)));
                    }
                });
                if target_words > 0 {
                    let words = manuscript::total_words(cx.arena(), id) as f32;
                    progress(ui, words / target_words as f32, theme);
                }
                ui.add_space(6.0);
                widgets::heading(ui, "Keywords", theme);
                self.property_field(ui, id, manuscript::KEYWORDS, 1, "comma, separated", cx);
            }
            if is_document {
                ui.add_space(8.0);
                widgets::heading(ui, "Statistics", theme);
                let text = cx.arena().get_by_id(id).ok().and_then(Entity::string_buffer).map(|text| text.as_str().to_owned()).unwrap_or_default();
                let words = sue_model::word_count(&text);
                let paragraphs = text.lines().filter(|line| !line.trim().is_empty()).count();
                let selection = self.editor.selected_text();
                ui.label(RichText::new(format!("{words} words, {} characters, {paragraphs} paragraphs", text.chars().filter(|ch| *ch != '\n').count())).color(theme.muted));
                if !selection.is_empty() {
                    ui.label(RichText::new(format!("Selected: {} words", sue_model::word_count(selection))).color(theme.muted));
                }
                ui.label(RichText::new(format!("About {} minutes to read", (words / 230).max(1))).color(theme.muted));
            }
            if in_project {
                ui.add_space(8.0);
                widgets::heading(ui, "Notes", theme);
                self.property_field(ui, id, manuscript::NOTES, 6, "Notes for yourself", cx);
            }
            if is_document && in_project {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    widgets::heading(ui, "Snapshots", theme);
                    if ui.small_button("Take").on_hover_text("Keep a copy of this document as it is now (Ctrl+5)").clicked() {
                        run = Some("project.snapshot".to_owned());
                    }
                });
                let shots = project::snapshots(cx.arena(), id);
                if shots.is_empty() {
                    ui.label(RichText::new("None yet. Take one before a big rewrite.").size(theme.small_size).color(theme.muted));
                }
                let mut restore = None;
                for shot in shots.iter().rev() {
                    let Ok(saved) = cx.arena().get_by_id(*shot) else {
                        continue;
                    };
                    let words = saved.string_buffer().map_or(0, |text| sue_model::word_count(text.as_str()));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}  ({words} words)", saved.name())).size(theme.small_size + 0.5));
                        if ui.small_button("Restore").on_hover_text("Put this version back (what is there now is kept as a snapshot)").clicked() {
                            restore = Some(*shot);
                        }
                    });
                }
                if let Some(shot) = restore
                    && let Some(batch) = project::restore(cx.arena(), id, shot, &project::today())
                {
                    self.apply(cx, batch);
                }
            }
            // The headings of what is shown, to jump to.
            let outline = self.editor.outline();
            if !outline.is_empty() && self.view == View::Document {
                ui.add_space(8.0);
                widgets::heading(ui, "Outline", theme);
                for entry in outline {
                    let indent = f32::from(entry.level.saturating_sub(1)) * 12.0;
                    ui.horizontal(|ui| {
                        ui.add_space(indent);
                        if ui.link(RichText::new(&entry.title).size(theme.small_size + 0.5)).clicked()
                            && let Some(doc) = self.editor.docs().get(entry.at.doc).copied()
                        {
                            self.editor.select(doc, entry.at.offset..entry.at.offset);
                        }
                    });
                }
            }
        });
        if let Some(command) = run {
            let keep = self.selected;
            self.selected = target;
            self.run(&command, cx);
            if keep.is_some() {
                self.selected = keep;
            }
        }
    }

    // ----- Status, find, composition --------------------------------------------------

    fn status_ui(&mut self, ui: &mut Ui, rect: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        ui.painter().rect_filled(rect, 0.0, theme.panel);
        ui.painter()
            .hline(rect.x_range(), rect.top(), theme.hairline());
        let (draft, session) = self.counts(cx);
        let doc_words = self
            .editor
            .current()
            .map_or(0, |doc| manuscript::words(cx.arena(), doc));
        let selected = sue_model::word_count(self.editor.selected_text());
        let mut parts = Vec::new();
        if selected > 0 {
            parts.push(format!("{selected} of {doc_words} words"));
        } else {
            parts.push(format!("{doc_words} words"));
        }
        if self.project.is_some() && !self.simple(cx) {
            let target = self
                .project_entity(cx)
                .and_then(|entity| entity.property(manuscript::TARGET))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            if target > 0 {
                parts.push(format!(
                    "Draft {draft} / {target} ({:.0}%)",
                    draft as f64 * 100.0 / target as f64
                ));
            } else {
                parts.push(format!("Draft {draft} words"));
            }
            let goal = self
                .project_entity(cx)
                .and_then(|entity| entity.property(project::SESSION_TARGET))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            if goal > 0 {
                parts.push(format!("Today {session:+} / {goal}"));
            } else {
                parts.push(format!("Today {session:+}"));
            }
        }
        if self.view == View::Document && self.paged {
            let (page, pages) = self.page_info;
            parts.push(format!("Page {} of {}", page + 1, pages.max(1)));
        }
        let text = parts.join("    |    ");
        ui.painter().text(
            rect.left_center() + vec2(12.0, 0.0),
            Align2::LEFT_CENTER,
            text,
            theme.font(theme.small_size + 0.5),
            theme.muted,
        );
        // Zoom at the right.
        let zoom = Rect::from_min_max(
            pos2(rect.right() - 150.0, rect.top() + 2.0),
            pos2(rect.right() - 8.0, rect.bottom() - 2.0),
        );
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(zoom)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        if child.small_button("+").on_hover_text("Zoom in").clicked() {
            self.run("view.zoom_in", cx);
        }
        let label = if self.fit {
            format!("Fit {:.0}%", self.shown_zoom * 100.0)
        } else {
            format!("{:.0}%", self.shown_zoom * 100.0)
        };
        if child
            .button(RichText::new(label).size(theme.small_size))
            .on_hover_text("Fit the page to the window, or show it at its size")
            .clicked()
        {
            self.run(
                if self.fit {
                    "view.zoom_reset"
                } else {
                    "view.zoom_fit"
                },
                cx,
            );
        }
        if child
            .small_button("\u{2212}")
            .on_hover_text("Zoom out")
            .clicked()
        {
            self.run("view.zoom_out", cx);
        }
    }

    fn find_bar(&mut self, ui: &mut Ui, rect: Rect, cx: &mut AppletContext<'_>, theme: &Theme) {
        ui.painter().rect_filled(rect, 0.0, theme.panel);
        ui.painter()
            .hline(rect.x_range(), rect.bottom(), theme.hairline());
        let Some(mut find) = self.find.clone() else {
            return;
        };
        let mut action: Option<&str> = None;
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink2(vec2(10.0, 4.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let ui = &mut child;
        let query = ui.add(
            egui::TextEdit::singleline(&mut find.query)
                .hint_text("Find")
                .desired_width(200.0)
                .id(Id::unique("writer-find")),
        );
        if find.focus {
            query.request_focus();
            find.focus = false;
        }
        if query.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
            action = Some(if ui.input(|input| input.modifiers.shift) {
                "previous"
            } else {
                "next"
            });
            query.request_focus();
        }
        if ui
            .small_button("\u{2191}")
            .on_hover_text("Previous (Shift+F3)")
            .clicked()
        {
            action = Some("previous");
        }
        if ui
            .small_button("\u{2193}")
            .on_hover_text("Next (F3)")
            .clicked()
        {
            action = Some("next");
        }
        if find.replacing {
            ui.add(
                egui::TextEdit::singleline(&mut find.replace)
                    .hint_text("Replace with")
                    .desired_width(180.0),
            );
            if ui.button("Replace").clicked() {
                action = Some("replace");
            }
            if ui.button("Replace All").clicked() {
                action = Some("all");
            }
        } else if ui.small_button("Replace...").clicked() {
            find.replacing = true;
        }
        ui.checkbox(&mut find.case, "Match case");
        ui.checkbox(&mut find.words, "Whole words");
        if self.project.is_some() {
            ui.checkbox(&mut find.project, "Whole project");
        }
        if !find.message.is_empty() {
            ui.label(RichText::new(&find.message).color(theme.muted));
        }
        let close = ui
            .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.small_button("\u{2715}")
                    .on_hover_text("Close (Esc)")
                    .clicked()
            })
            .inner;
        self.find = Some(find);
        match action {
            Some("next") => self.find_next(cx, false),
            Some("previous") => self.find_next(cx, true),
            Some("replace") => self.replace_one(cx),
            Some("all") => self.replace_all(cx),
            _ => {}
        }
        if close {
            self.find = None;
            self.editor.focus();
        }
    }

    fn composition_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter()
            .rect_filled(rect, 0.0, Color32::from_gray(0x12));
        self.text_ui(ui, rect, cx, theme, true);
        let (_, session) = self.counts(cx);
        let words = self
            .editor
            .current()
            .map_or(0, |doc| manuscript::words(cx.arena(), doc));
        let hint = format!("{words} words    |    today {session:+}    |    Esc to leave");
        ui.painter().text(
            rect.center_bottom() - vec2(0.0, 14.0),
            Align2::CENTER_BOTTOM,
            hint,
            theme.font(theme.small_size),
            Color32::from_gray(0x60),
        );
    }

    // ----- Dialogs ----------------------------------------------------------------------

    #[allow(clippy::too_many_lines, reason = "every dialog of the studio")]
    fn dialog_ui(&mut self, ctx: &egui::Context, cx: &mut AppletContext<'_>, theme: &Theme) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let mut keep = true;
        let mut after: Option<After> = None;
        let keys = matches!(dialog, Dialog::Shortcuts).then(|| {
            sue_wm::shortcuts::rows(
                cx.rice(),
                crate::studio::KEYS_SECTION,
                crate::studio::shortcut_table(),
            )
        });
        let title = match &dialog {
            Dialog::NewProject { .. } => "New Project",
            Dialog::Compile(_) => "Compile",
            Dialog::Settings(_) => "Project Settings",
            Dialog::Link(_) => "Link",
            Dialog::Footnote(_) => "Footnote",
            Dialog::Statistics => "Writing Statistics",
            Dialog::Shortcuts => "Keyboard Shortcuts",
        };
        let escape = ctx.input(|input| input.key_pressed(egui::Key::Escape));
        let id = Id::unique("sue-writer-dialog");
        egui::Modal::new(id)
            .area(egui::Modal::default_area(id).fade_in(false))
            .backdrop_color(theme.shadow.gamma_multiply(0.45))
            .frame(widgets::window_frame(theme).inner_margin(16))
            .show(ctx, |ui| {
                ui.set_min_width(380.0);
                ui.label(
                    RichText::new(title)
                        .size(theme.heading_size)
                        .color(theme.text),
                );
                ui.add_space(8.0);
                match &mut dialog {
                    Dialog::NewProject { template, title } => {
                        for (index, (_, label, about)) in TEMPLATES.iter().enumerate() {
                            ui.radio_value(template, index, RichText::new(*label).strong());
                            ui.label(
                                RichText::new(*about)
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                        }
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label("Title");
                            ui.text_edit_singleline(title);
                        });
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("Create").clicked() {
                                let key = TEMPLATES[(*template).min(TEMPLATES.len() - 1)].0;
                                let title = if title.trim().is_empty() {
                                    "Untitled".to_owned()
                                } else {
                                    title.trim().to_owned()
                                };
                                after = Some(Box::new(move |me: &mut Self, cx| {
                                    me.new_project(cx, key, &title)
                                }));
                                keep = false;
                            }
                            if ui.button("Cancel").clicked() {
                                keep = false;
                            }
                        });
                    }
                    Dialog::Compile(choices) => {
                        widgets::heading(ui, "Format", theme);
                        egui::Grid::new("writer-formats")
                            .num_columns(2)
                            .show(ui, |ui| {
                                for (index, (_, name, ext)) in FORMATS.iter().enumerate() {
                                    ui.radio_value(&mut choices.format, index, *name);
                                    ui.label(RichText::new(format!(".{ext}")).color(theme.muted));
                                    ui.end_row();
                                }
                            });
                        ui.add_space(6.0);
                        widgets::heading(ui, "What", theme);
                        if self.project.is_some() {
                            ui.radio_value(&mut choices.whole, true, "The whole manuscript");
                            ui.radio_value(&mut choices.whole, false, "What is open now");
                        }
                        ui.add_space(6.0);
                        widgets::heading(ui, "Titles", theme);
                        ui.radio_value(
                            &mut choices.titles,
                            Titles::Folders,
                            "Folders become chapter titles",
                        );
                        ui.radio_value(
                            &mut choices.titles,
                            Titles::All,
                            "Folders and documents both",
                        );
                        ui.radio_value(
                            &mut choices.titles,
                            Titles::None,
                            "No titles, just the text",
                        );
                        ui.add_space(6.0);
                        widgets::heading(ui, "Between documents", theme);
                        ui.radio_value(
                            &mut choices.separator,
                            Separator::SceneBreak,
                            "A scene break (#)",
                        );
                        ui.radio_value(&mut choices.separator, Separator::Page, "A new page");
                        ui.radio_value(&mut choices.separator, Separator::None, "Nothing");
                        ui.add_space(6.0);
                        ui.checkbox(&mut choices.title_page, "A title page first");
                        ui.checkbox(&mut choices.chapter_pages, "Each chapter on a new page");
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if ui.button("Export...").clicked() {
                                let picked = choices.clone();
                                after = Some(Box::new(move |me: &mut Self, cx| {
                                    me.ask_compile(cx, picked)
                                }));
                                keep = false;
                            }
                            if ui.button("Cancel").clicked() {
                                keep = false;
                            }
                        });
                    }
                    Dialog::Settings(settings) => {
                        self.settings_ui(ui, settings, theme);
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if ui.button("Apply").clicked() {
                                let settings = settings.clone();
                                after = Some(Box::new(move |me: &mut Self, cx| {
                                    me.apply_settings(cx, &settings)
                                }));
                                keep = false;
                            }
                            if ui.button("Cancel").clicked() {
                                keep = false;
                            }
                        });
                    }
                    Dialog::Link(url) => {
                        ui.label("Web address");
                        ui.add(egui::TextEdit::singleline(url).desired_width(340.0));
                        ui.horizontal(|ui| {
                            if ui.button("Link").clicked() {
                                let url = url.trim().to_owned();
                                after = Some(Box::new(move |me: &mut Self, cx| {
                                    me.editor.set_char(
                                        cx,
                                        kind::LINK,
                                        (!url.is_empty()).then_some(Value::Text(url)),
                                        "Link",
                                    )
                                }));
                                keep = false;
                            }
                            if ui.button("Remove Link").clicked() {
                                after = Some(Box::new(|me: &mut Self, cx| {
                                    me.editor.set_char(cx, kind::LINK, None, "Remove Link")
                                }));
                                keep = false;
                            }
                            if ui.button("Cancel").clicked() {
                                keep = false;
                            }
                        });
                    }
                    Dialog::Footnote(note) => {
                        ui.label("The note, printed at the end");
                        ui.add(
                            egui::TextEdit::multiline(note)
                                .desired_rows(4)
                                .desired_width(340.0),
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Add Footnote").clicked() {
                                let note = note.trim().to_owned();
                                after = Some(Box::new(move |me: &mut Self, cx| {
                                    if !note.is_empty() {
                                        me.editor.set_char(
                                            cx,
                                            kind::FOOTNOTE,
                                            Some(Value::Text(note)),
                                            "Footnote",
                                        );
                                    }
                                }));
                                keep = false;
                            }
                            if ui.button("Cancel").clicked() {
                                keep = false;
                            }
                        });
                    }
                    Dialog::Statistics => {
                        self.statistics_ui(ui, cx, theme);
                        if ui.button("Close").clicked() {
                            keep = false;
                        }
                    }
                    Dialog::Shortcuts => {
                        egui::ScrollArea::vertical()
                            .max_height(460.0)
                            .show(ui, |ui| {
                                let keys = keys.as_deref().unwrap_or_default();
                                sue_wm::shortcuts::grid_ui(ui, "writer-shortcuts", keys, theme);
                            });
                        ui.horizontal(|ui| {
                            if ui.button("Close").clicked() {
                                keep = false;
                            }
                            if ui
                                .button("Change Shortcuts...")
                                .on_hover_text("Settings, Keymaps: any number of keys for each")
                                .clicked()
                            {
                                keep = false;
                                after = Some(Box::new(|_: &mut Self, cx| {
                                    cx.request(ShellRequest::OpenSettings(Some(
                                        "keymaps".to_owned(),
                                    )));
                                }));
                            }
                        });
                    }
                }
            });
        if escape {
            keep = false;
        }
        if keep {
            self.dialog = Some(dialog);
        }
        if let Some(after) = after {
            after(self, cx);
        }
    }

    fn settings_ui(&self, ui: &mut Ui, settings: &mut Settings, theme: &Theme) {
        egui::Grid::new("writer-settings")
            .num_columns(2)
            .spacing(vec2(12.0, 6.0))
            .show(ui, |ui| {
                ui.label("Title");
                ui.text_edit_singleline(&mut settings.title);
                ui.end_row();
                ui.label("Author");
                ui.text_edit_singleline(&mut settings.author);
                ui.end_row();
                ui.label("Styles");
                egui::ComboBox::from_id_salt("writer-sheet")
                    .selected_text(
                        sue_type::SHEETS
                            .iter()
                            .find(|(key, _)| *key == settings.sheet)
                            .map_or("Standard", |(_, name)| *name),
                    )
                    .show_ui(ui, |ui| {
                        for (key, name) in sue_type::SHEETS {
                            ui.selectable_value(&mut settings.sheet, (*key).to_owned(), *name);
                        }
                    });
                ui.end_row();
                ui.label("Page");
                egui::ComboBox::from_id_salt("writer-page")
                    .selected_text(settings.page.clone())
                    .show_ui(ui, |ui| {
                        for page in PAGES {
                            ui.selectable_value(&mut settings.page, (*page).to_owned(), *page);
                        }
                    });
                ui.end_row();
                ui.label("Margins (inches)");
                let mut inches = settings.margin / 72.0;
                if ui
                    .add(
                        egui::DragValue::new(&mut inches)
                            .range(0.25..=3.0)
                            .speed(0.05)
                            .fixed_decimals(2),
                    )
                    .changed()
                {
                    settings.margin = inches * 72.0;
                }
                ui.end_row();
                ui.label("Body font");
                font_picker(
                    ui,
                    "writer-body-font",
                    &mut settings.font,
                    &mut settings.font_filter,
                );
                ui.end_row();
                ui.label("Heading font");
                let mut filter = String::new();
                font_picker(
                    ui,
                    "writer-heading-font",
                    &mut settings.heading_font,
                    &mut filter,
                );
                ui.end_row();
                ui.label("Body size (pt)");
                ui.add(
                    egui::DragValue::new(&mut settings.size)
                        .range(6.0..=48.0)
                        .speed(0.25),
                );
                ui.end_row();
                ui.label("Running head");
                ui.text_edit_singleline(&mut settings.header);
                ui.end_row();
                ui.label("Manuscript target (words)");
                ui.add(
                    egui::DragValue::new(&mut settings.target)
                        .range(0..=10_000_000)
                        .speed(100),
                );
                ui.end_row();
                ui.label("Words a day");
                ui.add(
                    egui::DragValue::new(&mut settings.session)
                        .range(0..=100_000)
                        .speed(10),
                );
                ui.end_row();
            });
        if !sue_wm::fonts::ready() {
            ui.label(
                RichText::new("Still finding installed fonts...")
                    .size(theme.small_size)
                    .color(theme.muted),
            );
        }
    }

    fn apply_settings(&mut self, cx: &mut AppletContext<'_>, settings: &Settings) {
        let Some(root) = self.project else {
            return;
        };
        let mut batch = sue_core::txn::Batch::new("Project Settings");
        let text = |value: &str| (!value.is_empty()).then(|| Value::Text(value.to_owned()));
        batch.push(Box::new(project::set(
            root,
            manuscript::TITLE,
            text(&settings.title),
        )));
        batch.push(Box::new(project::set(
            root,
            manuscript::AUTHOR,
            text(&settings.author),
        )));
        batch.push(Box::new(project::set(
            root,
            project::SHEET,
            text(&settings.sheet),
        )));
        batch.push(Box::new(project::set(
            root,
            project::PAGE,
            text(&settings.page),
        )));
        batch.push(Box::new(project::set(
            root,
            project::MARGIN,
            Some(Value::Float(f64::from(settings.margin))),
        )));
        batch.push(Box::new(project::set(
            root,
            project::FONT,
            text(&settings.font),
        )));
        batch.push(Box::new(project::set(
            root,
            project::HEADING_FONT,
            text(&settings.heading_font),
        )));
        batch.push(Box::new(project::set(
            root,
            project::SIZE,
            Some(Value::Float(f64::from(settings.size))),
        )));
        batch.push(Box::new(project::set(
            root,
            project::HEADER,
            text(&settings.header),
        )));
        batch.push(Box::new(project::set(
            root,
            manuscript::TARGET,
            (settings.target > 0).then_some(Value::Int(settings.target)),
        )));
        batch.push(Box::new(project::set(
            root,
            project::SESSION_TARGET,
            (settings.session > 0).then_some(Value::Int(settings.session)),
        )));
        if !settings.title.trim().is_empty() {
            batch.push(Box::new(project::rename(root, settings.title.trim())));
        }
        self.apply(cx, batch);
    }

    fn statistics_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
        let (draft, session) = self.counts(cx);
        let total = self
            .project
            .map_or(0, |root| manuscript::total_words(cx.arena(), root));
        let target = self
            .project_entity(cx)
            .and_then(|entity| entity.property(manuscript::TARGET))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        egui::Grid::new("writer-stats")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("This sitting");
                ui.label(format!("{session:+} words"));
                ui.end_row();
                ui.label("The manuscript");
                ui.label(format!("{draft} words"));
                ui.end_row();
                ui.label("Everything, with research");
                ui.label(format!("{total} words"));
                ui.end_row();
                if target > 0 {
                    ui.label("Target");
                    ui.label(format!(
                        "{target} words, {:.0}% there",
                        draft as f64 * 100.0 / target as f64
                    ));
                    ui.end_row();
                }
            });
        if target > 0 {
            progress(ui, draft as f32 / target as f32, theme);
        }
        let history = self
            .project_entity(cx)
            .map(project::history)
            .unwrap_or_default();
        if history.is_empty() {
            ui.label(
                RichText::new("Days you write on show here once a sitting ends.")
                    .size(theme.small_size)
                    .color(theme.muted),
            );
            return;
        }
        ui.add_space(8.0);
        widgets::heading(ui, "Words by day", theme);
        let days: Vec<&(String, i64)> = history.iter().rev().take(14).collect();
        let most = days
            .iter()
            .map(|(_, words)| *words)
            .max()
            .unwrap_or(1)
            .max(1);
        let (rect, _) = ui.allocate_exact_size(vec2(380.0, 110.0), Sense::hover());
        let bar = rect.width() / days.len().max(1) as f32;
        for (index, (day, words)) in days.iter().rev().enumerate() {
            let height = (*words as f32 / most as f32) * (rect.height() - 20.0);
            let column = Rect::from_min_max(
                pos2(
                    rect.left() + index as f32 * bar + 2.0,
                    rect.bottom() - 16.0 - height,
                ),
                pos2(
                    rect.left() + (index + 1) as f32 * bar - 2.0,
                    rect.bottom() - 16.0,
                ),
            );
            ui.painter()
                .rect_filled(column, 2.0, theme.text.gamma_multiply(0.7));
            ui.painter().text(
                pos2(column.center().x, rect.bottom()),
                Align2::CENTER_BOTTOM,
                &day[8..],
                theme.font(9.0),
                theme.muted,
            );
        }
    }
}

/// Where the studio's parts go.
pub(crate) struct Places {
    /// Between the bars.
    pub body: Rect,
    /// The status bar.
    pub status: Rect,
    /// The binder, when it shows.
    pub binder: Option<Rect>,
    /// The inspector, when it shows.
    pub inspector: Option<Rect>,
    /// The text, or the corkboard or outliner.
    pub center: Rect,
}

/// What a dialog's button does once the dialog is drawn.
type After = Box<dyn FnOnce(&mut SueWriter, &mut AppletContext<'_>)>;

/// The color around pages: the theme's darkest, a little lighter.
fn desk(theme: &Theme) -> Color32 {
    let [r, g, b, _] = theme.void.to_array();
    let lift = |value: u8| value.saturating_add(0x18);
    Color32::from_rgb(lift(r), lift(g), lift(b))
}

fn texture_key(id: EntityId) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    ("sue-writer-figure", id).hash(&mut hasher);
    hasher.finish()
}

/// A thin bar of `share` (0 to 1).
fn progress(ui: &mut Ui, share: f32, theme: &Theme) {
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width().min(380.0), 6.0), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, theme.pill);
    let done = Rect::from_min_size(
        rect.min,
        vec2(rect.width() * share.clamp(0.0, 1.0), rect.height()),
    );
    ui.painter().rect_filled(
        done,
        3.0,
        if share >= 1.0 {
            Color32::from_rgb(0x50, 0xb0, 0x7a)
        } else {
            theme.text.gamma_multiply(0.75)
        },
    );
}

/// A drag handle between two panels; returns its new x while dragged.
fn splitter(ui: &mut Ui, name: &str, x: f32, span: egui::Rangef, theme: &Theme) -> Option<f32> {
    let rect = Rect::from_x_y_ranges(x - 3.0..=x + 3.0, span);
    let response = ui.interact(
        rect,
        Id::unique(("sue-writer-splitter", name)),
        Sense::drag(),
    );
    let color = if response.hovered() || response.dragged() {
        theme.border_active
    } else {
        theme.border
    };
    ui.painter().vline(x, span, Stroke::new(1.0, color));
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    response
        .dragged()
        .then(|| response.interact_pointer_pos().map_or(x, |pos| pos.x))
}

/// Where a drop lands on a binder row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Zone {
    Before,
    Into,
    After,
}

fn drop_zone(rect: Rect, pointer: Pos2, folder: bool) -> Zone {
    let t = (pointer.y - rect.top()) / rect.height().max(1.0);
    if folder && (0.25..0.75).contains(&t) {
        Zone::Into
    } else if t < 0.5 {
        Zone::Before
    } else {
        Zone::After
    }
}

/// A toggle button showing `text`.
fn toggle(ui: &mut Ui, text: RichText, on: bool, theme: &Theme) -> egui::Response {
    let button = egui::Button::new(text.size(theme.ui_size + 1.0).color(if on {
        theme.text
    } else {
        theme.muted
    }))
    .fill(if on {
        theme.active
    } else {
        Color32::TRANSPARENT
    })
    .stroke(Stroke::NONE)
    .min_size(vec2(24.0, 24.0));
    ui.add(button)
}

/// A square button with a drawn icon.
fn icon(
    ui: &mut Ui,
    on: bool,
    theme: &Theme,
    draw: impl FnOnce(&Painter, Rect, Color32),
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
    if on {
        ui.painter()
            .rect_filled(rect, theme.control_radius, theme.active);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, theme.control_radius, theme.hover);
    }
    let color = if on || response.hovered() {
        theme.text
    } else {
        theme.muted
    };
    draw(ui.painter(), rect.shrink(6.0), color);
    response
}

fn align_icon(painter: &Painter, rect: Rect, color: Color32, align: sue_type::Align) {
    let stroke = Stroke::new(1.5, color);
    for (index, long) in [true, false, true, false].into_iter().enumerate() {
        let y = rect.top() + 1.0 + index as f32 * (rect.height() - 2.0) / 3.0;
        let width = if long || align == sue_type::Align::Justify {
            rect.width()
        } else {
            rect.width() * 0.6
        };
        let x = match align {
            sue_type::Align::Left | sue_type::Align::Justify => rect.left(),
            sue_type::Align::Center => rect.center().x - width / 2.0,
            sue_type::Align::Right => rect.right() - width,
        };
        painter.hline(x..=x + width, y, stroke);
    }
}

fn list_icon(painter: &Painter, rect: Rect, color: Color32, numbered: bool) {
    for index in 0..3 {
        let y = rect.top() + 1.5 + index as f32 * (rect.height() - 3.0) / 2.0;
        if numbered {
            painter.text(
                pos2(rect.left() + 1.5, y),
                Align2::CENTER_CENTER,
                (index + 1).to_string(),
                egui::FontId::proportional(7.0),
                color,
            );
        } else {
            painter.circle_filled(pos2(rect.left() + 1.5, y), 1.4, color);
        }
        painter.hline(rect.left() + 5.0..=rect.right(), y, Stroke::new(1.5, color));
    }
}

fn indent_icon(painter: &Painter, rect: Rect, color: Color32, more: bool) {
    let stroke = Stroke::new(1.5, color);
    for index in 0..4 {
        let y = rect.top() + 1.0 + index as f32 * (rect.height() - 2.0) / 3.0;
        let x = if index == 0 || index == 3 {
            rect.left()
        } else {
            rect.left() + 5.0
        };
        painter.hline(x..=rect.right(), y, stroke);
    }
    let mid = rect.center().y;
    let tip = if more { rect.left() + 3.5 } else { rect.left() };
    let back = if more { rect.left() } else { rect.left() + 3.5 };
    painter.add(egui::Shape::convex_polygon(
        vec![pos2(back, mid - 2.5), pos2(tip, mid), pos2(back, mid + 2.5)],
        color,
        Stroke::NONE,
    ));
}

fn page_icon(painter: &Painter, rect: Rect, color: Color32, empty: bool) {
    let page = rect.shrink2(vec2(2.0, 0.5));
    painter.rect_stroke(page, 1.0, Stroke::new(1.2, color), StrokeKind::Inside);
    if !empty {
        for index in 0..3 {
            let y = page.top() + 3.5 + index as f32 * 2.6;
            painter.hline(
                page.left() + 2.0..=page.right() - 2.0,
                y,
                Stroke::new(1.0, color.gamma_multiply(0.7)),
            );
        }
    }
}

fn folder_icon(painter: &Painter, rect: Rect, color: Color32) {
    let body = Rect::from_min_max(
        pos2(rect.left(), rect.top() + 3.0),
        rect.right_bottom() - vec2(0.0, 1.0),
    );
    painter.rect_stroke(body, 1.5, Stroke::new(1.2, color), StrokeKind::Inside);
    painter.hline(
        rect.left() + 1.0..=rect.left() + 5.5,
        rect.top() + 1.6,
        Stroke::new(1.6, color),
    );
}

fn book_icon(painter: &Painter, rect: Rect, color: Color32) {
    let body = rect.shrink2(vec2(1.5, 0.5));
    painter.rect_stroke(body, 1.0, Stroke::new(1.2, color), StrokeKind::Inside);
    painter.vline(body.left() + 3.0, body.y_range(), Stroke::new(1.2, color));
}

fn trash_icon(painter: &Painter, rect: Rect, color: Color32) {
    let can = Rect::from_min_max(
        pos2(rect.left() + 2.0, rect.top() + 3.5),
        pos2(rect.right() - 2.0, rect.bottom()),
    );
    painter.rect_stroke(can, 1.0, Stroke::new(1.2, color), StrokeKind::Inside);
    painter.hline(
        rect.left() + 0.5..=rect.right() - 0.5,
        rect.top() + 2.0,
        Stroke::new(1.2, color),
    );
}

fn picture_icon(painter: &Painter, rect: Rect, color: Color32) {
    painter.rect_stroke(
        rect.shrink(0.5),
        1.0,
        Stroke::new(1.2, color),
        StrokeKind::Inside,
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(rect.left() + 2.0, rect.bottom() - 2.0),
            pos2(rect.center().x - 1.0, rect.center().y),
            pos2(rect.right() - 2.0, rect.bottom() - 2.0),
        ],
        color.gamma_multiply(0.7),
        Stroke::NONE,
    ));
}

/// A combo box of installed fonts with a search field.
fn font_picker(ui: &mut Ui, salt: &str, font: &mut String, filter: &mut String) {
    egui::ComboBox::from_id_salt(salt.to_owned())
        .selected_text(font.clone())
        .height(360.0)
        .width(180.0)
        .show_ui(ui, |ui| {
            ui.add(egui::TextEdit::singleline(filter).hint_text("Search fonts"));
            let needle = filter.to_lowercase();
            for family in sue_wm::fonts::families() {
                if needle.is_empty() || family.to_lowercase().contains(&needle) {
                    ui.selectable_value(font, family.clone(), family);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_lays_out_on_a_phone_and_on_a_desktop() {
        let writer = SueWriter::default();
        // A phone's page: the text takes it all, the panels fold away.
        let phone = Rect::from_min_size(pos2(0.0, 52.0), vec2(390.0, 760.0));
        let places = writer.places(phone, false, true, true);
        assert!(places.binder.is_none() && places.inspector.is_none());
        assert_eq!(places.center.width(), 390.0);
        // A desktop's page has room for the binder and the inspector.
        let desktop = Rect::from_min_size(pos2(0.0, 28.0), vec2(1440.0, 850.0));
        let places = writer.places(desktop, false, true, true);
        assert!(places.binder.is_some() && places.inspector.is_some());
        assert!(places.center.width() >= 520.0);
    }
}
