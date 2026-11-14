//! The corkboard: the items of a folder as index cards.

use egui::{Align2, Color32, CornerRadius, Rect, RichText, Sense, Stroke, StrokeKind, Ui, vec2};
use sue_core::{EntityId, Value};
use sue_model::manuscript;
use sue_wm::{AppletContext, Theme};

use crate::project;
use crate::studio::{SueWriter, View};

use super::rgb;
use super::widgets::desk;

impl SueWriter {
    pub(super) fn corkboard_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
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
                                    project::status_text(status).to_uppercase(),
                                    egui::FontId::proportional(9.5),
                                    Color32::from_rgba_unmultiplied(0xb0, 0x30, 0x30, 0xa0),
                                );
                            }
                            let words = manuscript::total_words(cx.arena(), *id);
                            painter.text(
                                rect.left_bottom() + vec2(10.0, -8.0),
                                Align2::LEFT_BOTTOM,
                                tr!("word-count", count = words),
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
                        RichText::new(tr!("no-cards-here-yet-new-documents")).color(theme.muted),
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
}
