//! The binder: the tree of the project's items.

use egui::{Id, Rect, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2};
use sue_core::{EntityId, Value};
use sue_model::manuscript::{self, role};
use sue_model::picture;
use sue_wm::{AppletContext, AppletId, ArenaEdit, ShellRequest, Theme, widgets};

use crate::project::{self, LABELS, STATUSES};
use crate::studio::{SueWriter, View};

use super::rgb;
use super::widgets::{
    Zone, book_icon, drop_zone, folder_icon, page_icon, picture_icon, trash_icon,
};

impl SueWriter {
    pub(super) fn binder_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
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
            widgets::heading(ui, &tr!("binder"), theme);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::pill_button(ui, &tr!("folder"), false, theme)
                    .on_hover_text(tr!("new-folder-ctrl-shift-n"))
                    .clicked()
                {
                    run = Some("binder.new_folder".to_owned());
                }
                if widgets::pill_button(ui, &tr!("text-2"), false, theme)
                    .on_hover_text(tr!("new-document-ctrl-n"))
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
                    // A phone's binder covers the text: back to writing.
                    self.phone_binder = false;
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
            item(ui, &tr!("new-document"), "binder.new_doc", run);
            item(ui, &tr!("new-folder"), "binder.new_folder", run);
            ui.separator();
            if folder_role.is_none() {
                if ui.button(tr!("rename")).clicked() {
                    self.renaming = Some((id, name.clone()));
                    ui.close();
                }
                item(ui, &tr!("duplicate"), "project.duplicate", run);
                item(
                    ui,
                    &if included {
                        tr!("leave-out-of-export")
                    } else {
                        tr!("include-in-export")
                    },
                    "project.include",
                    run,
                );
                if document {
                    item(ui, &tr!("take-snapshot"), "project.snapshot", run);
                    item(ui, &tr!("merge-with-next"), "project.merge", run);
                }
                ui.menu_button(tr!("label"), |ui| {
                    for (label, color) in LABELS {
                        if ui
                            .button(
                                RichText::new(format!("\u{25CF} {}", project::label_text(label)))
                                    .color(rgb(*color)),
                            )
                            .clicked()
                        {
                            *run = Some(format!("label.{label}"));
                            ui.close();
                        }
                    }
                    if ui.button(tr!("no-label")).clicked() {
                        *run = Some("label.none".to_owned());
                        ui.close();
                    }
                });
                ui.menu_button(tr!("status"), |ui| {
                    for status in STATUSES {
                        if ui.button(project::status_text(status)).clicked() {
                            *run = Some(format!("status.{status}"));
                            ui.close();
                        }
                    }
                    if ui.button(tr!("no-status")).clicked() {
                        *run = Some("status.none".to_owned());
                        ui.close();
                    }
                });
                if image && ui.button(tr!("open-in-sue-paint")).clicked() {
                    if let Ok(paint) = AppletId::new("sue-paint") {
                        cx.request(ShellRequest::OpenIn {
                            entity: id,
                            applet: paint,
                        });
                    }
                    ui.close();
                }
                ui.separator();
                item(ui, &tr!("move-to-trash"), "project.trash", run);
            }
            if folder_role.as_deref() == Some(role::TRASH) {
                item(ui, &tr!("empty-trash"), "project.empty_trash", run);
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
}
