//! The start screen and its template gallery.

use egui::{
    Align2, Color32, Painter, Rect, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2,
};
use sue_core::EntityId;
use sue_model::class;
use sue_model::manuscript;
use sue_wm::icons::Icon;
use sue_wm::ui::{self, Density, Radius, Space, TextRole, Tip};
use sue_wm::{AppletContext, Theme};

use crate::studio::SueWriter;
use crate::templates::{Group, Template};

/// The widest a start page's column gets, in the largest steps of the
/// spacing scale.
const COLUMN_STEPS: f32 = 22.5;
/// Below this width (in the largest steps of the spacing scale) the page
/// is a phone's: a thumb's gutter, smaller cards.
const NARROW_STEPS: f32 = 15.0;
/// The least width of a template's card, in rows of the density, on a
/// narrow page and on a wide one.
const CARD_ROWS: (f32, f32) = (3.5, 4.5);
/// A page is this many times as tall as it is wide.
const PAGE_RATIO: f32 = 1.25;

impl SueWriter {
    /// The start page: what to begin from, as pictures of first pages
    /// (the templates, then the presets for long work), the projects that
    /// are already in this Sue project, and importing a document.
    pub(super) fn start_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::ZERO, theme.void);
        // A phone's start page keeps a thumb's gutter; a desk's centers a
        // column, as a word processor's start screen does.
        let narrow = rect.width() < theme.space(Space::S9) * NARROW_STEPS;
        let gutter = theme.space(if narrow { Space::S6 } else { Space::S8 });
        let width = (rect.width() - 2.0 * gutter).min(theme.space(Space::S9) * COLUMN_STEPS);
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
                // The cards keep clear of the scroll bar at the right.
                let width = width - theme.space(Space::S5);
                ui.label(
                    RichText::new(tr!("start-title"))
                        .font(theme.text_font(TextRole::Display))
                        .color(theme.text),
                );
                ui.label(
                    RichText::new(tr!("start-something-new-open-project-sue"))
                        .font(theme.text_font(TextRole::Body))
                        .color(theme.text_muted()),
                );
                ui.add_space(theme.space(Space::S6));
                let least = Density::of(ui).row() * if narrow { CARD_ROWS.0 } else { CARD_ROWS.1 };
                ui::heading(ui, &tr!("new-2"), theme);
                ui.add_space(theme.space(Space::S2));
                let templates: Vec<&Template> = self.templates.group(Group::Template).collect();
                if let Some(key) = gallery(ui, "writer-template", &templates, width, least, theme) {
                    chosen = Some(key);
                }
                ui.add_space(theme.space(Space::S5));
                ui::heading(ui, &tr!("presets"), theme);
                ui.label(
                    RichText::new(tr!("presets-about"))
                        .font(theme.text_font(TextRole::Caption))
                        .color(theme.text_muted()),
                );
                ui.add_space(theme.space(Space::S2));
                let presets: Vec<&Template> = self.templates.group(Group::Preset).collect();
                if let Some(key) = gallery(ui, "writer-preset", &presets, width, least, theme) {
                    chosen = Some(key);
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
                    ui.add_space(theme.space(Space::S5));
                    ui::heading(ui, &tr!("in-this-project"), theme);
                    for (id, name, words) in projects {
                        let count = tr!("word-count", count = words);
                        let row = ui::list_row(("writer-start-project", id), &name)
                            .icon(Icon::Book)
                            .note(&count)
                            .show(ui, theme);
                        if row.response.clicked() {
                            self.open_project(cx, id, None);
                        }
                    }
                }
                ui.add_space(theme.space(Space::S6));
                ui.horizontal_wrapped(|ui| {
                    let import = tr!("import-a-document-ellipsis");
                    let help = tr!("word-markdown-html-plain-text");
                    let button = ui::button("writer-start-import", &import)
                        .icon(Icon::Import)
                        .show(ui, theme);
                    if ui::tooltip(button, &Tip::new(&import).help(&help), theme).clicked() {
                        self.run("file.import", cx);
                    }
                    let folder = tr!("templates-folder");
                    let help = tr!("templates-folder-about");
                    let button = ui::button("writer-start-templates", &folder)
                        .ghost()
                        .icon(Icon::FolderReveal)
                        .show(ui, theme);
                    if ui::tooltip(button, &Tip::new(&folder).help(&help), theme).clicked() {
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
}

/// `templates` as cards of their first pages in rows as wide as `width`,
/// each card at least `least` wide. Returns the key of the one clicked
/// (or chosen with Enter or Space while it holds focus).
fn gallery(
    ui: &mut Ui,
    salt: &str,
    templates: &[&Template],
    width: f32,
    least: f32,
    theme: &Theme,
) -> Option<String> {
    let gap = theme.space(Space::S5);
    let columns = ((width + gap) / (least + gap)).floor().max(2.0) as usize;
    let card_width = (width - gap * (columns as f32 - 1.0)) / columns as f32;
    let page_height = card_width * PAGE_RATIO;
    let name_font = theme.text_font(TextRole::Body);
    let name_height = theme.text_size(TextRole::Body) + theme.space(Space::S5);
    let radius = theme.radius(Radius::Sm);
    let mut chosen = None;
    for (row, members) in templates.chunks(columns).enumerate() {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (column, template) in members.iter().enumerate() {
                let (card, _) = ui.allocate_exact_size(
                    vec2(card_width, page_height + name_height),
                    Sense::hover(),
                );
                let id = ui.scope_id().with((salt, row * columns + column));
                let response = ui.interact(card, id, Sense::click());
                let name = template.name();
                response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
                let page = Rect::from_min_size(card.min, vec2(card_width, page_height));
                template_thumb(ui.painter(), page, template);
                let outline = if response.hovered() || response.has_focus() {
                    theme.selected_stroke()
                } else {
                    theme.hairline()
                };
                ui.painter()
                    .rect_stroke(page, radius, outline, StrokeKind::Outside);
                ui::focus_ring(ui, &response, radius, theme);
                ui.painter().text(
                    pos2(page.left(), page.bottom() + theme.space(Space::S3)),
                    Align2::LEFT_TOP,
                    &name,
                    name_font.clone(),
                    theme.text,
                );
                let about = template.about();
                if ui::tooltip(response, &Tip::new(&name).help(&about), theme).clicked() {
                    chosen = Some(template.key.clone());
                }
            }
        });
        ui.add_space(theme.space(Space::S3));
    }
    chosen
}

/// A template's first page in small: its paragraphs as bars, sized and
/// placed by their styles, as a word processor's template gallery shows.
/// It is a picture of paper, so it stays light in every theme.
fn template_thumb(painter: &Painter, page: Rect, template: &Template) {
    painter.rect_filled(page, page.width() * 0.02, Color32::from_gray(0xfa));
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
