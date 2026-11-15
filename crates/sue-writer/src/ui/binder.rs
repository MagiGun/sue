//! The binder: the tree of the project's items.
//!
//! Every row is the shared tree row (`sue_wm::ui::TreeRow`): its icon
//! says what the item is, its mark its label, its note that it is left
//! out of the export. A click opens, a double click or F2 renames,
//! Delete moves to the Trash, a right click offers the rest, and a drag
//! moves the item above, below or into another.

use egui::{
    CornerRadius, Key, Modifiers, Rect, Response, Role, Sense, Ui, UiBuilder, WidgetInfo, pos2,
};
use sue_core::{EntityId, Value};
use sue_model::manuscript::{self, role};
use sue_model::picture;
use sue_wm::icons::Icon;
use sue_wm::ui::{self, Density, Radius, Space, Tip};
use sue_wm::{AppletContext, AppletId, ArenaEdit, Facts, ItemState, MenuItem, ShellRequest, Theme};

use crate::project::{self, LABELS, STATUSES};
use crate::studio::{KEYS_SECTION, SueWriter, View, shortcut_table};

use super::rgb;
use super::widgets::{Zone, drop_zone};

/// What the binder's right-click menu runs that is not a command of the
/// registry: it is done here.
const RENAME: &str = "binder.rename";
/// Opens a picture of Research in Sue Paint.
const OPEN_IN_PAINT: &str = "binder.open_in_paint";

/// The keys that run `command` now, as the settings say (`Ctrl+N`), or
/// nothing when it has none: what a tooltip shows after a control's name.
pub(super) fn keys_of(command: &str, cx: &AppletContext<'_>) -> String {
    shortcut_table()
        .iter()
        .find(|shortcut| shortcut.command == command)
        .map(|shortcut| sue_wm::shortcuts::chords(cx.rice(), KEYS_SECTION, shortcut))
        .and_then(|chords| chords.chords().first().map(ToString::to_string))
        .unwrap_or_default()
}

/// The text of a key alone, such as `F2`, for a tooltip or a menu.
pub(super) fn key_text(key: Key) -> String {
    sue_wm::shortcuts::chord_text(Modifiers::NONE, key)
}

/// A quiet icon button whose tooltip says its `name` and its `keys`.
pub(super) fn tool_button(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    icon: Icon,
    name: &str,
    keys: &str,
    theme: &Theme,
) -> Response {
    // The button is shown without a name of its own, so that the one
    // tooltip is the rich one, with the keys; what assistive technology
    // reads is its name all the same.
    let response = ui::icon_button(salt, icon, "").quiet().show(ui, theme);
    let enabled = response.enabled();
    response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, name));
    ui::tooltip(response, &Tip::new(name).shortcut(keys), theme)
}

/// What a binder row is drawn with.
struct Item {
    name: String,
    folder: bool,
    document: bool,
    image: bool,
    /// Draft, Research or Trash: a place of the project that is not
    /// renamed, moved or thrown away.
    role: Option<String>,
    included: bool,
    label: Option<[u8; 3]>,
    synopsis: String,
    empty: bool,
}

impl Item {
    /// The icon of the item: the Draft is the binder's book, the Trash its
    /// bin, a folder a folder (open while its rows show), a picture a
    /// picture and a document a page (an empty one while nothing is
    /// written in it).
    fn icon(&self, open: bool) -> Icon {
        match self.role.as_deref() {
            Some(role::TRASH) => Icon::Delete,
            Some(role::DRAFT) => Icon::Binder,
            _ if self.folder && open => Icon::FolderOpen,
            _ if self.folder => Icon::Folder,
            _ if self.image => Icon::Picture,
            _ if self.empty => Icon::PageEmpty,
            _ => Icon::Page,
        }
    }
}

impl SueWriter {
    pub(super) fn binder_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter()
            .rect_filled(rect, CornerRadius::ZERO, theme.panel);
        let Some(root) = self.project else {
            return;
        };
        let mut child = ui.new_child(
            UiBuilder::new()
                .id_salt("writer-binder")
                .max_rect(rect.shrink(theme.space(Space::S3)))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        let ui = &mut child;
        let mut run: Option<String> = None;
        Density::Compact.scope(ui, |ui| {
            ui.horizontal(|ui| {
                ui::heading(ui, &tr!("binder"), theme);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let folder = tr!("new-folder");
                    let keys = keys_of("binder.new_folder", cx);
                    if tool_button(ui, "new-folder", Icon::FolderNew, &folder, &keys, theme)
                        .clicked()
                    {
                        run = Some("binder.new_folder".to_owned());
                    }
                    let document = tr!("new-document");
                    let keys = keys_of("binder.new_doc", cx);
                    if tool_button(ui, "new-document", Icon::FileNew, &document, &keys, theme)
                        .clicked()
                    {
                        run = Some("binder.new_doc".to_owned());
                    }
                });
            });
            ui.add_space(theme.space(Space::S2));
            let mut moves: Vec<(EntityId, EntityId, usize)> = Vec::new();
            ui::ScrollView::vertical("writer-binder")
                .on(theme.panel)
                .show(ui, theme, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
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
        });
        if let Some(command) = run {
            self.binder_command(&command, cx);
        }
    }

    /// Runs what the binder asked for: its own actions here, the rest
    /// through the registry.
    fn binder_command(&mut self, command: &str, cx: &mut AppletContext<'_>) {
        match command {
            RENAME => {
                if let Some(id) = self.selected
                    && let Ok(entity) = cx.arena().get_by_id(id)
                {
                    self.renaming = Some((id, entity.name().to_owned()));
                }
            }
            OPEN_IN_PAINT => {
                if let (Some(id), Ok(paint)) = (self.selected, AppletId::new("sue-paint")) {
                    cx.request(ShellRequest::OpenIn {
                        entity: id,
                        applet: paint,
                    });
                }
            }
            command => {
                self.run(command, cx);
            }
        }
    }

    /// What the binder knows of the item `id`.
    fn binder_item(cx: &AppletContext<'_>, id: EntityId) -> Option<Item> {
        let entity = cx.arena().get_by_id(id).ok()?;
        let document = manuscript::is_document(entity);
        Some(Item {
            name: entity.name().to_owned(),
            folder: manuscript::is_folder(entity),
            document,
            image: picture::is_picture(entity)
                || entity
                    .component(sue_core::ComponentKind::RasterTile2D)
                    .is_some(),
            role: entity
                .property(manuscript::ROLE)
                .and_then(Value::as_str)
                .map(str::to_owned),
            included: manuscript::included(entity),
            label: entity
                .property(manuscript::LABEL)
                .and_then(Value::as_str)
                .and_then(project::label_color),
            synopsis: entity
                .property(manuscript::SYNOPSIS)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            empty: document
                && entity
                    .string_buffer()
                    .is_some_and(|text| text.as_str().trim().is_empty()),
        })
    }

    /// The right-click menu of a binder row.
    fn binder_menu(item: &Item) -> Vec<MenuItem> {
        let command = |command: &str| {
            let label = crate::commands::registry()
                .label(command)
                .unwrap_or_else(|| command.to_owned());
            MenuItem::action(label, command)
        };
        let keyed = |label: String, command: &str, key: Key| MenuItem::Action {
            label,
            command: command.to_owned(),
            shortcut: key_text(key),
            note: String::new(),
        };
        let mut items = vec![command("binder.new_doc"), command("binder.new_folder")];
        if item.role.is_none() {
            items.push(MenuItem::Separator);
            items.push(keyed(tr!("rename"), RENAME, Key::F2));
            items.push(command("project.duplicate"));
            items.push(MenuItem::action(
                if item.included {
                    tr!("leave-out-of-export")
                } else {
                    tr!("include-in-export")
                },
                "project.include",
            ));
            if item.document {
                items.push(command("project.snapshot"));
                items.push(command("project.merge"));
            }
            let mut labels: Vec<MenuItem> = LABELS
                .iter()
                .map(|(label, _)| {
                    MenuItem::action(project::label_text(label), format!("label.{label}"))
                })
                .collect();
            labels.push(MenuItem::Separator);
            labels.push(MenuItem::action(tr!("no-label"), "label.none"));
            items.push(MenuItem::sub(tr!("label"), labels));
            let mut statuses: Vec<MenuItem> = STATUSES
                .iter()
                .map(|status| {
                    MenuItem::action(project::status_text(status), format!("status.{status}"))
                })
                .collect();
            statuses.push(MenuItem::Separator);
            statuses.push(MenuItem::action(tr!("no-status"), "status.none"));
            items.push(MenuItem::sub(tr!("status"), statuses));
            if item.image {
                items.push(MenuItem::action(tr!("open-in-sue-paint"), OPEN_IN_PAINT));
            }
            items.push(MenuItem::Separator);
            items.push(keyed(tr!("move-to-trash"), "project.trash", Key::Delete));
        }
        if item.role.as_deref() == Some(role::TRASH) {
            items.push(MenuItem::Separator);
            items.push(command("project.empty_trash"));
        }
        items
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
        let Some(item) = Self::binder_item(cx, id) else {
            return;
        };
        let children = manuscript::children(cx.arena(), id);
        let open = self.expanded.contains(&id);
        let renaming = self
            .renaming
            .as_ref()
            .filter(|(known, _)| *known == id)
            .map(|(_, text)| text.clone());
        let left_out = tr!("binder-left-out");
        let title = if renaming.is_some() { "" } else { &item.name };
        let mut tree = ui::tree_row(("writer-binder-row", id), title)
            .depth(depth)
            .icon(item.icon(open))
            .selected(self.selected == Some(id));
        if !children.is_empty() {
            tree = tree.expanded(open);
        }
        if let Some(color) = item.label {
            tree = tree.row(|row| row.mark(rgb(color)));
        }
        if !item.included {
            tree = tree.row(|row| row.note(&left_out));
        }
        let shown = tree.show(ui, theme);
        let response = shown.response;
        let rect = response.rect;
        if shown.toggled {
            if open {
                self.expanded.remove(&id);
            } else {
                self.expanded.insert(id);
            }
        }
        if let Some(mut text) = renaming {
            // The name is typed where it is drawn: past the row's chevron
            // and icon, as the tree row lays them out.
            let density = Density::of(ui);
            let pad = theme.space(Space::S4);
            let gap = theme.space(Space::S2);
            let left = rect.left()
                + pad
                + depth as f32 * theme.space(Space::S6)
                + density.icon()
                + gap
                + density.icon()
                + pad
                - gap;
            let field = Rect::from_min_max(pos2(left, rect.top()), rect.max);
            let edit = ui::text_field(("writer-rename", id), &mut text)
                .label(&tr!("rename"))
                .show_at(ui, field, theme);
            if !edit.has_focus() && !edit.lost_focus() {
                edit.request_focus();
            }
            if edit.lost_focus() {
                self.renaming = None;
                let escape = ui.input(|input| input.key_pressed(Key::Escape));
                let name = text.trim();
                if !escape && !name.is_empty() && name != item.name {
                    let _ = cx.apply(ArenaEdit(project::rename(id, name)));
                }
            } else {
                self.renaming = Some((id, text));
            }
        } else {
            // The whole name, for one cut short, and what happens in it.
            let synopsis: String = item.synopsis.chars().take(160).collect();
            ui::tooltip(
                response.clone(),
                &Tip::new(&item.name).help(&synopsis),
                theme,
            );
        }
        // A click selects and opens; a double click or F2 renames; Delete
        // moves to the Trash.
        if response.clicked() && !shown.toggled {
            self.selected = Some(id);
            self.open = Some(id);
            if item.document {
                if self.view == View::Document || self.view == View::Corkboard {
                    self.view = View::Document;
                }
                self.editor.select(id, 0..0);
                // A phone's binder covers the text: back to writing.
                self.phone_binder = false;
            }
        }
        if response.secondary_clicked() {
            self.selected = Some(id);
        }
        let fixed = item.role.is_some();
        if !fixed && response.double_clicked() {
            self.renaming = Some((id, item.name.clone()));
        }
        if !fixed && response.has_focus() && self.renaming.is_none() {
            let (rename, trash) = ui.input_mut(|input| {
                (
                    input.consume_key(Modifiers::NONE, Key::F2),
                    input.consume_key(Modifiers::NONE, Key::Delete),
                )
            });
            if rename {
                self.selected = Some(id);
                self.renaming = Some((id, item.name.clone()));
            } else if trash {
                self.selected = Some(id);
                *run = Some("project.trash".to_owned());
            }
        }
        let menu = Self::binder_menu(&item);
        let picked = {
            let facts = Facts::of(cx);
            let registry = crate::commands::registry();
            let me: &Self = self;
            let state = |command: &str| {
                if command == RENAME || command == OPEN_IN_PAINT {
                    ItemState::on()
                } else {
                    registry.state(me, &facts, command)
                }
            };
            ui::context_menu(ui, &response, &menu, &state, theme)
        };
        if let Some(command) = picked {
            self.selected = Some(id);
            *run = Some(command);
        }
        // Dragging an item onto another moves it: above or below it, or
        // into a folder. The drag has a sense of its own over the row, so
        // that clicks stay the row's.
        if !fixed {
            ui.interact(rect, response.id.with("writer-binder-drag"), Sense::drag())
                .dnd_set_drag_payload(id);
        }
        if let (Some(dragged), Some(pointer)) = (
            response.dnd_hover_payload::<EntityId>(),
            ui.input(|input| input.pointer.hover_pos()),
        ) && *dragged != id
        {
            let stroke = theme.focus_stroke();
            match drop_zone(rect, pointer, item.folder) {
                Zone::Into => {
                    ui.painter().rect_stroke(
                        rect,
                        theme.radius(Radius::Lg),
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
                Zone::Before => {
                    ui.painter().hline(rect.x_range(), rect.top(), stroke);
                }
                Zone::After => {
                    ui.painter().hline(rect.x_range(), rect.bottom(), stroke);
                }
            }
        }
        if let (Some(dragged), Some(pointer)) = (
            response.dnd_release_payload::<EntityId>(),
            ui.input(|input| input.pointer.interact_pos()),
        ) {
            let dragged = *dragged;
            if dragged != id {
                match drop_zone(rect, pointer, item.folder) {
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
}
