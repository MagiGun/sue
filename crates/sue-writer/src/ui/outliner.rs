//! The outliner: the items of a folder as rows, one column for each of
//! what the inspector edits (title, synopsis, label, status, words and
//! whether the item is exported).

use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Color32, CornerRadius, Key, Painter, Rect, Response, Role, Sense, Ui, UiBuilder, Vec2,
    WidgetInfo, pos2,
};
use sue_core::{EntityId, Value};
use sue_model::manuscript;
use sue_wm::icons::{self, Icon};
use sue_wm::ui::{self, Density, Radius, Space, TextRole, Tip};
use sue_wm::{AppletContext, Theme};

use crate::project;
use crate::studio::{SueWriter, View};

use super::binder::key_text;
use super::rgb;

/// The columns, by the share of the width each takes.
const SHARES: [f32; 6] = [0.32, 0.34, 0.11, 0.11, 0.06, 0.06];

/// Draws `text` in one line no wider than `width`, its left end at `left`
/// and its middle on `y`; what does not fit ends in an ellipsis.
fn cell(
    painter: &Painter,
    text: &str,
    (left, y, width): (f32, f32, f32),
    color: Color32,
    theme: &Theme,
) {
    let mut job =
        LayoutJob::simple_singleline(text.to_owned(), theme.text_font(TextRole::Body), color);
    job.wrap = TextWrapping::truncate_at_width(width.max(0.0));
    let galley = painter.layout_job(job);
    let top = y - galley.size().y / 2.0;
    painter.galley(pos2(left, top), galley, color);
}

/// The left end of each column of `rect`, and the width it has.
fn columns(rect: Rect, theme: &Theme) -> [(f32, f32); 6] {
    let gap = theme.space(Space::S4);
    let mut left = rect.left() + gap;
    let room = rect.width() - gap;
    SHARES.map(|share| {
        let width = room * share;
        let column = (left, width - gap);
        left += width;
        column
    })
}

/// What one row shows.
struct Row {
    id: EntityId,
    depth: usize,
    name: String,
    folder: bool,
    synopsis: String,
    label: String,
    label_color: Option<[u8; 3]>,
    status: String,
    words: usize,
    included: bool,
}

impl SueWriter {
    pub(super) fn outliner_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter()
            .rect_filled(rect, CornerRadius::ZERO, theme.void);
        let Some(folder) = self.board_folder(cx) else {
            return;
        };
        let rows: Vec<Row> = manuscript::walk(cx.arena(), folder)
            .into_iter()
            .filter_map(|(id, depth)| {
                let entity = cx.arena().get_by_id(id).ok()?;
                let text = |key: &str| {
                    entity
                        .property(key)
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned()
                };
                let label = text(manuscript::LABEL);
                Some(Row {
                    id,
                    depth,
                    name: entity.name().to_owned(),
                    folder: manuscript::is_folder(entity),
                    synopsis: text(manuscript::SYNOPSIS).replace('\n', " "),
                    label_color: project::label_color(&label),
                    label: project::label_text(&label),
                    status: project::status_text(&text(manuscript::STATUS)),
                    words: manuscript::total_words(cx.arena(), id),
                    included: manuscript::included(entity),
                })
            })
            .collect();
        let mut open = None;
        let mut child = ui.new_child(
            UiBuilder::new()
                .id_salt("writer-outliner")
                .max_rect(rect.shrink(theme.space(Space::S5)))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let ui = &mut child;
        Density::Compact.scope(ui, |ui| {
            if rows.is_empty() {
                ui::empty_state("writer-outliner-empty", &tr!("outliner-empty"))
                    .icon(Icon::Outliner)
                    .detail(&tr!("outliner-empty-detail"))
                    .show(ui, theme);
                return;
            }
            let density = Density::of(ui);
            let width = ui.available_width();
            let (_, head) = ui.allocate_space(Vec2::new(width, density.row()));
            let titles = [
                tr!("title"),
                tr!("synopsis"),
                tr!("label"),
                tr!("status"),
                tr!("words-3"),
                tr!("export"),
            ];
            let muted = theme.text_muted();
            for ((left, room), title) in columns(head, theme).into_iter().zip(&titles) {
                let mut job = LayoutJob::simple_singleline(
                    title.clone(),
                    theme.text_font(TextRole::Caption),
                    muted,
                );
                job.wrap = TextWrapping::truncate_at_width(room.max(0.0));
                let galley = ui.painter().layout_job(job);
                let top = head.center().y - galley.size().y / 2.0;
                ui.painter().galley(pos2(left, top), galley, muted);
            }
            ui::divider(ui, theme);
            ui::ScrollView::vertical("writer-outliner")
                .on(theme.void)
                .show(ui, theme, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    for row in &rows {
                        if self.outliner_row(ui, row, width, theme) {
                            open = Some(row.id);
                        }
                    }
                });
        });
        if let Some(id) = open {
            self.selected = Some(id);
            self.open = Some(id);
            self.view = View::Document;
            if cx.arena().get_by_id(id).is_ok_and(manuscript::is_document) {
                self.editor.select(id, 0..0);
            }
        }
    }

    /// One row of the outliner. A click chooses it; a double click or
    /// Enter opens it. Returns `true` when it was opened.
    fn outliner_row(&mut self, ui: &mut Ui, row: &Row, width: f32, theme: &Theme) -> bool {
        let density = Density::of(ui);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, density.row()), Sense::click());
        let selected = self.selected == Some(row.id);
        response.widget_info(|| WidgetInfo::selected(Role::Row, true, selected, &row.name));
        let radius = theme.radius(Radius::Md);
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let fill = if selected {
                theme.fill_selected()
            } else if response.is_pointer_button_down_on() {
                theme.fill_pressed()
            } else if response.hovered() {
                theme.fill_hover()
            } else {
                Color32::TRANSPARENT
            };
            if fill != Color32::TRANSPARENT {
                painter.rect_filled(rect, radius, fill);
            }
            let muted = if selected {
                theme.text_muted_on(theme.fill_selected())
            } else {
                theme.text_muted()
            };
            let y = rect.center().y;
            let [title, synopsis, label, status, words, export] = columns(rect, theme);
            // The title, set in by its depth, after its icon.
            let side = density.icon();
            let indent = row.depth as f32 * theme.space(Space::S6);
            let icon =
                Rect::from_center_size(pos2(title.0 + indent + side / 2.0, y), Vec2::splat(side));
            let kind = if row.folder { Icon::Folder } else { Icon::Page };
            icons::paint(painter, icon, kind, muted);
            let text_left = icon.right() + theme.space(Space::S3);
            let room = title.0 + title.1 - text_left;
            cell(painter, &row.name, (text_left, y, room), theme.text, theme);
            cell(
                painter,
                &row.synopsis,
                (synopsis.0, y, synopsis.1),
                muted,
                theme,
            );
            // The label: its color as a mark, then its name.
            let mut left = label.0;
            if let Some(color) = row.label_color {
                let dot = theme.space(Space::S3);
                let half = dot / 2.0;
                painter.circle_filled(pos2(left + half, y), half, rgb(color));
                left += dot + theme.space(Space::S2);
            }
            cell(
                painter,
                &row.label,
                (left, y, label.0 + label.1 - left),
                muted,
                theme,
            );
            cell(painter, &row.status, (status.0, y, status.1), muted, theme);
            cell(
                painter,
                &row.words.to_string(),
                (words.0, y, words.1),
                muted,
                theme,
            );
            if row.included {
                let tick = Rect::from_min_size(pos2(export.0, y - side / 2.0), Vec2::splat(side));
                icons::paint(painter, tick, Icon::Check, muted);
            }
            ui::focus_ring(ui, &response, radius, theme);
        }
        let enter = key_text(Key::Enter);
        let help = tr!("outliner-row-help");
        let response: Response = ui::tooltip(
            response,
            &Tip::new(&row.name).shortcut(&enter).help(&help),
            theme,
        );
        if response.clicked() {
            self.selected = Some(row.id);
        }
        let entered = response.has_focus() && ui.input(|input| input.key_pressed(Key::Enter));
        response.double_clicked() || entered
    }
}
