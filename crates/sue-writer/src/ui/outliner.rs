//! The outliner: the items of a folder as rows.

use egui::{Color32, Rect, RichText, Sense, Ui, UiBuilder, pos2, vec2};
use sue_core::Value;
use sue_model::manuscript;
use sue_wm::{AppletContext, Theme};

use crate::project;
use crate::studio::{SueWriter, View};

use super::rgb;

impl SueWriter {
    pub(super) fn outliner_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
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
            (tr!("title"), 0.34),
            (tr!("synopsis"), 0.34),
            (tr!("label"), 0.1),
            (tr!("status"), 0.1),
            (tr!("words-3"), 0.06),
            (tr!("compile"), 0.06),
        ];
        let width = ui.available_width();
        ui.horizontal(|ui| {
            for (title, share) in &columns {
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
                    cell(
                        columns[2].1,
                        project::label_text(&label),
                        label_color,
                        &mut x,
                        0.0,
                    );
                    cell(
                        columns[3].1,
                        project::status_text(
                            entity
                                .property(manuscript::STATUS)
                                .and_then(Value::as_str)
                                .unwrap_or(""),
                        ),
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
}
