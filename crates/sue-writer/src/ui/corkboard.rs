//! The corkboard: the items of a folder as index cards.
//!
//! A card is a raised surface of the theme with the item's label as a
//! strip along its top, its title, its synopsis (or the start of its
//! text) and, at its foot, its words and its status. A click chooses a
//! card, a double click or Enter opens it, and a drag puts it elsewhere
//! in the folder.

use egui::{
    Align2, CornerRadius, Key, Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, Vec2,
    WidgetInfo, pos2,
};
use sue_core::{EntityId, Value};
use sue_model::manuscript;
use sue_wm::icons::Icon;
use sue_wm::ui::{self, Elevation, Radius, Space, TextRole, Tip};
use sue_wm::{AppletContext, Theme};

use crate::project;
use crate::studio::{SueWriter, View};

use super::binder::{key_text, keys_of};
use super::rgb;
use super::widgets::desk;

/// How wide a card is, in steps of the largest space: about the width of
/// a paper index card's line of text.
const CARD_WIDTH: f32 = 5.5;
/// How tall a card is, in the same steps: an index card's proportions.
const CARD_HEIGHT: f32 = 3.75;
/// How much of a document's text stands in for a synopsis it lacks.
const PREVIEW: usize = 220;

/// The size of a card in `theme`.
fn card_size(theme: &Theme) -> Vec2 {
    let step = theme.space(Space::S9);
    Vec2::new(step * CARD_WIDTH, step * CARD_HEIGHT)
}

/// What a card shows.
struct Card {
    id: EntityId,
    title: String,
    body: String,
    label: Option<[u8; 3]>,
    status: Option<String>,
    words: usize,
}

impl SueWriter {
    pub(super) fn corkboard_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter()
            .rect_filled(rect, CornerRadius::ZERO, desk(theme));
        let Some(folder) = self.board_folder(cx) else {
            return;
        };
        let cards: Vec<Card> = manuscript::children(cx.arena(), folder)
            .into_iter()
            .filter_map(|id| {
                let entity = cx.arena().get_by_id(id).ok()?;
                let body = entity
                    .property(manuscript::SYNOPSIS)
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                    .map(str::to_owned)
                    .or_else(|| {
                        entity
                            .string_buffer()
                            .map(|text| text.as_str().chars().take(PREVIEW).collect())
                    })
                    .unwrap_or_default();
                Some(Card {
                    id,
                    title: entity.name().to_owned(),
                    body,
                    label: entity
                        .property(manuscript::LABEL)
                        .and_then(Value::as_str)
                        .and_then(project::label_color),
                    status: entity
                        .property(manuscript::STATUS)
                        .and_then(Value::as_str)
                        .map(project::status_text),
                    words: manuscript::total_words(cx.arena(), id),
                })
            })
            .collect();
        if cards.is_empty() {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            let mut add = false;
            ui::empty_state("writer-corkboard-empty", &tr!("corkboard-empty"))
                .icon(Icon::Corkboard)
                .detail(&tr!("corkboard-empty-detail"))
                .show_actions(&mut child, theme, |ui| {
                    let name = tr!("new-document");
                    let keys = keys_of("binder.new_doc", cx);
                    let response = ui::button("corkboard-new", &name)
                        .icon(Icon::FileNew)
                        .show(ui, theme);
                    let response = ui::tooltip(response, &Tip::new(&name).shortcut(&keys), theme);
                    add = response.clicked();
                });
            if add {
                self.selected = Some(folder);
                self.run("binder.new_doc", cx);
            }
            return;
        }
        let size = card_size(theme);
        let gap = theme.space(Space::S6);
        let mut moves = Vec::new();
        let mut open = None;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        ui::ScrollView::vertical("writer-corkboard")
            .on(desk(theme))
            .show(&mut child, theme, |ui| {
                let width = ui.available_width();
                let columns = ((width - gap) / (size.x + gap)).floor().max(1.0) as usize;
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.add_space(gap);
                for (row, chunk) in cards.chunks(columns).enumerate() {
                    ui.horizontal(|ui| {
                        ui.add_space(gap);
                        for (column, card) in chunk.iter().enumerate() {
                            let index = row * columns + column;
                            let response = self.card_ui(ui, card, size, theme);
                            if response.double_clicked()
                                || (response.has_focus()
                                    && ui.input(|input| input.key_pressed(Key::Enter)))
                            {
                                open = Some(card.id);
                            }
                            if let Some(dragged) = response.dnd_release_payload::<EntityId>()
                                && *dragged != card.id
                            {
                                moves.push((*dragged, folder, index));
                            }
                            if response
                                .dnd_hover_payload::<EntityId>()
                                .is_some_and(|dragged| *dragged != card.id)
                            {
                                // Where the card dropped here goes: before
                                // this one.
                                let x = response.rect.left() - gap / 2.0;
                                ui.painter().vline(
                                    x,
                                    response.rect.y_range(),
                                    theme.focus_stroke(),
                                );
                            }
                            ui.add_space(gap);
                        }
                    });
                    ui.add_space(gap);
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

    /// Draws one card of `size` and returns its response.
    fn card_ui(&mut self, ui: &mut Ui, card: &Card, size: Vec2, theme: &Theme) -> Response {
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let selected = self.selected == Some(card.id);
        response.widget_info(|| WidgetInfo::selected(Role::ListItem, true, selected, &card.title));
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let corner = theme.corner(Radius::Lg);
            let lifted = if response.hovered() || selected {
                Elevation::Floating
            } else {
                Elevation::Raised
            };
            painter.add(theme.elevation(lifted).as_shape(rect, corner));
            painter.rect(
                rect,
                corner,
                theme.surface(2),
                theme.hairline(),
                StrokeKind::Inside,
            );
            let pad = theme.card_padding();
            let inner = rect.shrink(pad);
            if let Some(color) = card.label {
                let strip =
                    Rect::from_min_size(rect.min, Vec2::new(rect.width(), theme.space(Space::S2)));
                let round = corner.nw;
                let top = CornerRadius {
                    nw: round,
                    ne: round,
                    sw: 0,
                    se: 0,
                };
                painter.rect_filled(strip, top, rgb(color));
            }
            let title = painter.layout(
                card.title.clone(),
                theme.text_font(TextRole::Title),
                theme.text,
                inner.width(),
            );
            let title_bottom = inner.top() + title.size().y;
            painter
                .with_clip_rect(inner)
                .galley(inner.left_top(), title, theme.text);
            let rule = title_bottom + theme.space(Space::S2);
            painter.hline(inner.x_range(), rule, theme.hairline());
            let foot = theme.text_size(TextRole::Caption) + theme.space(Space::S3);
            let room = Rect::from_min_max(
                pos2(inner.left(), rule + theme.space(Space::S2)),
                pos2(inner.right(), inner.bottom() - foot),
            );
            let muted = theme.text_muted_on(theme.surface(2));
            let body = painter.layout(
                card.body.clone(),
                theme.text_font(TextRole::Body),
                muted,
                room.width(),
            );
            painter
                .with_clip_rect(room)
                .galley(room.left_top(), body, muted);
            let caption = theme.text_font(TextRole::Caption);
            painter.text(
                inner.left_bottom(),
                Align2::LEFT_BOTTOM,
                tr!("word-count", count = card.words),
                caption.clone(),
                muted,
            );
            if let Some(status) = &card.status {
                painter.text(
                    inner.right_bottom(),
                    Align2::RIGHT_BOTTOM,
                    status,
                    caption,
                    muted,
                );
            }
            if selected {
                let stroke: Stroke = theme.selected_stroke();
                painter.rect_stroke(rect, corner, stroke, StrokeKind::Outside);
            }
            ui::focus_ring(ui, &response, theme.radius(Radius::Lg), theme);
        }
        response.dnd_set_drag_payload(card.id);
        let enter = key_text(Key::Enter);
        let help = tr!("corkboard-card-help");
        let response = ui::tooltip(
            response,
            &Tip::new(&card.title).shortcut(&enter).help(&help),
            theme,
        );
        if response.clicked() {
            self.selected = Some(card.id);
        }
        response
    }
}
