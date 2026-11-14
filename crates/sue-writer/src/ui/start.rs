//! The start screen and its template gallery.

use egui::{
    Align2, Color32, Painter, Rect, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2,
};
use sue_core::EntityId;
use sue_model::class;
use sue_model::manuscript;
use sue_wm::{AppletContext, Theme, widgets};

use crate::studio::SueWriter;
use crate::templates::{Group, Template};

impl SueWriter {
    pub(super) fn start_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter().rect_filled(rect, 0.0, theme.void);
        // A phone's start page keeps a thumb's gutter; a desk's centers a
        // column, as a word processor's start screen does.
        let gutter = if rect.width() < 600.0 { 16.0 } else { 36.0 };
        let width = (rect.width() - 2.0 * gutter).min(900.0);
        let area = Rect::from_min_max(
            pos2(rect.center().x - width / 2.0, rect.top() + gutter),
            pos2(rect.center().x + width / 2.0, rect.bottom()),
        );
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(area)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let mut chosen: Option<String> = None;
        egui::ScrollArea::vertical()
            .id_salt("writer-start")
            .auto_shrink([false, false])
            .show(&mut child, |ui| {
                ui.set_width(width);
                ui.label(
                    RichText::new("Sue Writer")
                        .size(theme.heading_size * 1.4)
                        .color(theme.text),
                );
                ui.label(
                    RichText::new(tr!("start-something-new-open-project-sue"))
                        .size(theme.small_size + 1.0)
                        .color(theme.muted),
                );
                ui.add_space(14.0);
                // New, with the presets for long work in a list at its side.
                ui.horizontal(|ui| {
                    widgets::heading(ui, &tr!("new-2"), theme);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("writer-presets")
                            .selected_text(tr!("presets"))
                            .width(170.0)
                            .show_ui(ui, |ui| {
                                ui.set_min_width(240.0);
                                for template in self.templates.group(Group::Preset) {
                                    if ui
                                        .selectable_label(false, template.name())
                                        .on_hover_text(template.about())
                                        .clicked()
                                    {
                                        chosen = Some(template.key.clone());
                                    }
                                }
                            })
                            .response
                            .on_hover_text(tr!("presets-about"));
                    });
                });
                ui.add_space(6.0);
                // Each template as a picture of its first page.
                let gap = 12.0;
                let least = if width < 420.0 { 100.0 } else { 132.0 };
                let columns = ((width + gap) / (least + gap)).floor().max(2.0) as usize;
                let card_width = (width - gap * (columns as f32 - 1.0)) / columns as f32;
                let paper = vec2(card_width, card_width * 1.25);
                let templates: Vec<&Template> = self.templates.group(Group::Template).collect();
                for row in templates.chunks(columns) {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for template in row {
                            let (card, response) = ui.allocate_exact_size(
                                vec2(card_width, paper.y + 44.0),
                                Sense::click(),
                            );
                            let page = Rect::from_min_size(card.min, paper);
                            template_thumb(ui.painter(), page, template, response.hovered(), theme);
                            ui.painter().text(
                                pos2(page.left() + 2.0, page.bottom() + 8.0),
                                Align2::LEFT_TOP,
                                template.name(),
                                theme.font(theme.ui_size),
                                theme.text,
                            );
                            let response = response.on_hover_text(template.about());
                            if response.clicked() {
                                chosen = Some(template.key.clone());
                            }
                        }
                    });
                    ui.add_space(gap * 0.5);
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
                    widgets::heading(ui, &tr!("in-this-project"), theme);
                    for (id, name, words) in projects {
                        if ui
                            .button(tr!("words", name = name, words = words))
                            .clicked()
                        {
                            self.open_project(cx, id, None);
                        }
                    }
                }
                ui.add_space(14.0);
                ui.horizontal_wrapped(|ui| {
                    if widgets::pill_button(ui, &tr!("import-a-document-ellipsis"), false, theme)
                        .on_hover_text(tr!("word-markdown-html-plain-text"))
                        .clicked()
                    {
                        self.run("file.import", cx);
                    }
                    if widgets::pill_button(ui, &tr!("templates-folder"), false, theme)
                        .on_hover_text(tr!("templates-folder-about"))
                        .clicked()
                    {
                        self.run("file.templates_folder", cx);
                    }
                });
                ui.add_space(gutter);
            });
        if let Some(key) = chosen {
            let label = self
                .templates
                .get(&key)
                .map_or_else(|| tr!("untitled"), Template::name);
            self.new_project(cx, &key, &label);
        }
    }

    // ----- Corkboard and outliner -----------------------------------------------------
}

/// The color around pages: the theme's darkest, a little lighter.
/// A template's first page in small: its paragraphs as bars, sized and
/// placed by their styles, as a word processor's template gallery shows.
fn template_thumb(
    painter: &Painter,
    page: Rect,
    template: &Template,
    hovered: bool,
    theme: &Theme,
) {
    painter.rect_filled(page, 3.0, Color32::from_gray(0xfa));
    painter.rect_stroke(
        page,
        3.0,
        if hovered {
            Stroke::new(2.0, theme.text)
        } else {
            theme.hairline()
        },
        StrokeKind::Outside,
    );
    let margin = page.width() * 0.12;
    let inner = page.shrink(margin);
    let unit = page.width() / 100.0;
    let mut y = inner.top();
    let preview = template.preview();
    if preview.iter().all(|paragraph| paragraph.text.is_empty()) {
        // A blank page: just the caret's line.
        painter.line_segment(
            [pos2(inner.left(), y), pos2(inner.left(), y + 6.0 * unit)],
            Stroke::new(unit.max(1.0), Color32::from_gray(0x50)),
        );
        return;
    }
    for paragraph in preview {
        let (height, ink, before) = match paragraph.style.as_str() {
            "title" => (5.0, 0x30, 0.0),
            "subtitle" => (3.2, 0x70, 1.0),
            "heading1" | "chapter" => (3.4, 0x40, 4.0),
            "heading2" => (3.0, 0x48, 3.0),
            "heading3" | "scene" => (2.6, 0x55, 2.5),
            "quote" => (2.0, 0x90, 2.0),
            _ => (1.6, 0xa8, 1.0),
        };
        let height = height * unit;
        y += before * unit;
        if paragraph.mods.iter().any(|one| one == "break") {
            y += 4.0 * unit;
        }
        if y + height > inner.bottom() {
            break;
        }
        // The bar is as long as the text, at most the line; an empty body
        // paragraph is a few lines of room to write in.
        let chars = paragraph.text.chars().count() as f32;
        let lines = if paragraph.text.is_empty() && paragraph.style == sue_type::BODY {
            2
        } else {
            1
        };
        for line in 0..lines {
            let full = if paragraph.text.is_empty() {
                if line + 1 == lines { 0.6 } else { 1.0 }
            } else {
                (chars * height * 0.55 / inner.width()).clamp(0.15, 1.0)
            };
            let mut left = inner.left();
            if paragraph
                .mods
                .iter()
                .any(|one| one == "bullet" || one == "number")
            {
                painter.circle_filled(
                    pos2(left + unit, y + height / 2.0),
                    unit * 0.8,
                    Color32::from_gray(ink),
                );
                left += unit * 4.0;
            }
            let length = (inner.right() - left) * full;
            let x = if paragraph.mods.iter().any(|one| one == "center") {
                inner.center().x - length / 2.0
            } else if paragraph.mods.iter().any(|one| one == "right") {
                inner.right() - length
            } else {
                left
            };
            painter.rect_filled(
                Rect::from_min_size(pos2(x, y), vec2(length, height * 0.7)),
                height * 0.2,
                Color32::from_gray(ink),
            );
            y += height + unit;
        }
    }
}
