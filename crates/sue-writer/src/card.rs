//! Writer projects on Sue's canvas: a card showing the very same entity
//! (its title, how far it has come, and its opening lines), with a button
//! that opens it in Sue Writer.

use egui::{Align2, Color32, Rect, Sense, Ui, pos2, vec2};
use sue_core::{Entity, Value};
use sue_model::manuscript::{self, role};
use sue_wm::{AppletContext, AppletManifest, AppletRole, NodeId, SueApplet, widgets};

/// The card's applet id.
pub const CARD_ID: &str = "manuscript";

/// Writer projects on the canvas.
#[derive(Debug, Default)]
pub struct WriterCard;

impl SueApplet for WriterCard {
    fn manifest(&self) -> AppletManifest {
        let mut manifest =
            AppletManifest::new(CARD_ID, "Manuscript", AppletRole::Node, "\u{1F4D6}");
        manifest.summary = "A Sue Writer project on the canvas.".to_owned();
        manifest.order = 22;
        manifest
    }

    fn accepts(&self, entity: &Entity) -> bool {
        manuscript::is_manuscript(entity)
    }

    fn node_size(&self, _entity: &Entity) -> egui::Vec2 {
        vec2(320.0, 420.0)
    }

    fn node_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {
        let area = ui.max_rect();
        let Some(entity) = cx.entity(node) else {
            return;
        };
        let id = entity.id();
        let title = entity
            .property(manuscript::TITLE)
            .and_then(Value::as_str)
            .filter(|title| !title.is_empty())
            .unwrap_or(entity.name())
            .to_owned();
        let author = entity
            .property(manuscript::AUTHOR)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let target = entity
            .property(manuscript::TARGET)
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let arena = cx.arena();
        let draft = manuscript::folder(arena, id, role::DRAFT).unwrap_or(id);
        let words = manuscript::total_words(arena, draft);
        // The opening lines of the first document.
        let opening: String = manuscript::walk(arena, draft)
            .into_iter()
            .filter_map(|(child, _)| arena.get_by_id(child).ok())
            .filter(|child| manuscript::is_document(child))
            .filter_map(|child| {
                child
                    .string_buffer()
                    .map(|text| text.as_str().trim().to_owned())
            })
            .find(|text| !text.is_empty())
            .unwrap_or_default()
            .chars()
            .take(600)
            .collect();
        // A page: paper with the title on it.
        let painter = ui.painter();
        painter.rect_filled(area, 2.0, Color32::from_rgb(0xfb, 0xfa, 0xf6));
        let ink = Color32::from_gray(0x22);
        let soft = Color32::from_gray(0x77);
        let inner = area.shrink(26.0);
        let heading = painter.layout(title, egui::FontId::proportional(24.0), ink, inner.width());
        let mut y = inner.top() + 30.0;
        painter.galley(
            pos2(inner.center().x - heading.size().x / 2.0, y),
            heading.clone(),
            ink,
        );
        y += heading.size().y + 6.0;
        if !author.is_empty() {
            painter.text(
                pos2(inner.center().x, y),
                Align2::CENTER_TOP,
                author,
                egui::FontId::proportional(13.0),
                soft,
            );
            y += 22.0;
        }
        let progress = if target > 0 {
            format!("{words} of {target} words")
        } else {
            format!("{words} words")
        };
        painter.text(
            pos2(inner.center().x, y + 4.0),
            Align2::CENTER_TOP,
            progress,
            egui::FontId::proportional(11.0),
            soft,
        );
        y += 34.0;
        if !opening.is_empty() {
            let body = painter.layout(
                opening,
                egui::FontId::proportional(11.5),
                Color32::from_gray(0x44),
                inner.width(),
            );
            painter
                .with_clip_rect(Rect::from_min_max(pos2(inner.left(), y), inner.max))
                .galley(pos2(inner.left(), y), body, ink);
        }
        if ui
            .interact(
                area,
                egui::Id::unique(("sue-writer-card", node.0)),
                Sense::click(),
            )
            .double_clicked()
        {
            crate::ui::open_in_writer(cx, id);
        }
    }

    fn pill_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        let Some(id) = cx.entity(node).map(Entity::id) else {
            return;
        };
        let words = manuscript::total_words(cx.arena(), id);
        ui.label(
            egui::RichText::new(format!("{words} words"))
                .size(theme.small_size)
                .color(theme.muted),
        );
        widgets::pill_divider(ui, &theme);
        if widgets::pill_button(ui, "Open in Sue Writer", false, &theme)
            .on_hover_text("Write in it (double-click the card does the same)")
            .clicked()
        {
            crate::ui::open_in_writer(cx, id);
        }
    }
}
