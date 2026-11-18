//! The media pool: what the project holds, as thumbnails or as a list.
//! A click picks an item, a double click shows it in the source monitor
//! (or a sequence on the timeline), and dragging one onto the timeline
//! places it.

use std::sync::PoisonError;

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};
use sue_core::{EntityId, Time};
use sue_wm::icons::{self, Icon};
use sue_wm::ui::{Space, TextRole};
use sue_wm::{AppletContext, Theme};

use super::{SQUARE, font, timecode};
use crate::media::{Need, Quality};
use crate::model::{self, SourceKind};
use crate::studio::SueVe;

/// What is dragged from the pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolItem(pub EntityId);

/// A thumbnail's width as a share of a step of spacing.
const CARD_STEPS: f32 = 3.5;
/// Thumbnails are 16 by 9.
const ASPECT: f32 = 9.0 / 16.0;

/// The icon of a kind of source.
#[must_use]
pub fn kind_icon(kind: &SourceKind) -> Icon {
    match kind {
        SourceKind::Movie => Icon::Film,
        SourceKind::Sound => Icon::Audio,
        SourceKind::Still | SourceKind::ImageSequence => Icon::MaterialImage,
        SourceKind::Picture => Icon::AppPaint,
        SourceKind::Title => Icon::ToolText,
        SourceKind::Solid => Icon::FillSelection,
        SourceKind::Shape => Icon::ToolFigure,
        SourceKind::Adjustment => Icon::Effects,
        SourceKind::Sequence => Icon::PaletteTimeline,
        SourceKind::Unknown => Icon::Document,
    }
}

/// The name of a kind of source.
#[must_use]
pub fn kind_name(kind: &SourceKind) -> String {
    match kind {
        SourceKind::Movie => tr!("kind-movie"),
        SourceKind::Sound => tr!("kind-sound"),
        SourceKind::Still => tr!("kind-still"),
        SourceKind::Picture => tr!("kind-picture"),
        SourceKind::ImageSequence => tr!("kind-image-sequence"),
        SourceKind::Title => tr!("kind-title"),
        SourceKind::Solid => tr!("kind-solid"),
        SourceKind::Shape => tr!("kind-shape"),
        SourceKind::Adjustment => tr!("kind-adjustment"),
        SourceKind::Sequence => tr!("kind-sequence"),
        SourceKind::Unknown => tr!("kind-unknown"),
    }
}

/// Draws the picture of pool item `id` in `rect`: a movie's first
/// thumbnail, a still or a picture, a solid's color, else its icon.
pub(crate) fn picture(
    studio: &mut SueVe,
    ui: &Ui,
    rect: Rect,
    id: EntityId,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, SQUARE, theme.void);
    let Ok(entity) = cx.arena().get_by_id(id) else {
        return;
    };
    let kind = SourceKind::of(entity);
    let frame = match kind {
        SourceKind::Movie => studio.hub.thumbs.strip(id).and_then(|strip| {
            strip
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .frames
                .first()
                .cloned()
        }),
        SourceKind::Still | SourceKind::Picture | SourceKind::ImageSequence | SourceKind::Solid => {
            studio
                .hub
                .frame(
                    cx.arena(),
                    id,
                    &kind,
                    Time::ZERO,
                    (256, 144),
                    Quality::Quarter,
                    Need::Exact,
                )
                .map(|found| found.frame)
        }
        _ => None,
    };
    if let Some(frame) = frame {
        let key = texture_key(("sve-pool", id));
        if let Some(texture) = cx.textures.texture(
            key,
            frame.serial,
            [frame.width, frame.height],
            4,
            &frame.rgba,
            false,
        ) {
            let (w, h) = (frame.width as f32, frame.height as f32);
            let fit = (rect.width() / w).min(rect.height() / h);
            let shown = Rect::from_center_size(rect.center(), vec2(w * fit, h * fit));
            if kind == SourceKind::Solid {
                painter.image(
                    texture,
                    rect,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    theme.text,
                );
            } else {
                painter.image(
                    texture,
                    shown,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            return;
        }
    }
    let side = sue_wm::ui::Density::Regular.icon() * 2.0;
    icons::paint(
        &painter,
        Rect::from_center_size(rect.center(), vec2(side, side)),
        kind_icon(&kind),
        theme.text_muted(),
    );
}

/// A stable texture key for `salt`.
pub(crate) fn texture_key(salt: impl std::hash::Hash) -> u64 {
    use std::hash::Hasher as _;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    salt.hash(&mut hasher);
    hasher.finish()
}

/// The pool palette.
pub fn pool_ui(studio: &mut SueVe, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
    let Some(project) = studio.project else {
        return;
    };
    let rect = ui.max_rect();
    let pad = theme.space(Space::S3);
    let bar = sue_wm::ui::Density::Compact.icon_button();
    // The bar: import, a new bin of generated sources, the view.
    let mut x = rect.left() + pad;
    let top = rect.top() + pad;
    let mut clicked = Vec::new();
    for (icon, command, on) in [
        (Icon::Import, "file.import", false),
        (Icon::ToolText, "file.new_title", false),
        (Icon::FillSelection, "file.new_solid", false),
        (Icon::ViewGrid, "pool.thumbs", !studio.list),
        (Icon::ViewList, "pool.list", studio.list),
    ] {
        let place = Rect::from_min_size(pos2(x, top), vec2(bar, bar));
        x += bar + theme.space(Space::S1);
        let name = match command {
            "pool.thumbs" => tr!("pool-thumbnails"),
            "pool.list" => tr!("pool-list"),
            other => studio.tip(other).0,
        };
        if sue_wm::ui::icon_button(("sve-pool-bar", command), icon, &name)
            .selected(on)
            .show_at(ui, place, theme)
            .clicked()
        {
            clicked.push(command);
        }
    }
    for command in clicked {
        match command {
            "pool.thumbs" => studio.list = false,
            "pool.list" => studio.list = true,
            other => {
                studio.run(other, cx);
            }
        }
    }
    let items: Vec<(EntityId, usize)> = model::pool(cx.arena(), project)
        .into_iter()
        .chain(
            cx.arena()
                .relations()
                .targets(project, sue_core::REF)
                .into_iter()
                .map(|id| (id, 0)),
        )
        .collect();
    let area = Rect::from_min_max(pos2(rect.left(), top + bar + pad), rect.max);
    if items.is_empty() {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
        sue_wm::ui::empty_state("sve-pool-empty", &tr!("pool-empty"))
            .icon(Icon::Import)
            .detail(&tr!("pool-empty-detail"))
            .show(&mut child, theme);
        return;
    }
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
    egui::ScrollArea::vertical()
        .id_salt("sve-pool-scroll")
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            if studio.list {
                list(studio, ui, &items, cx, theme);
            } else {
                grid(studio, ui, &items, cx, theme);
            }
        });
}

/// What a click on an item does: pick it; a double click opens it.
fn clicked_item(
    studio: &mut SueVe,
    id: EntityId,
    response: &egui::Response,
    cx: &mut AppletContext<'_>,
) {
    if response.clicked() {
        studio.picked = Some(id);
    }
    if response.double_clicked() {
        if model::sequence(cx.arena(), id).is_some() {
            studio.sequence = Some(id);
            studio.selection.clear();
            studio.clock = crate::playback::Clock::default();
        } else {
            studio.show_source(cx, id);
        }
    }
    if response.drag_started() {
        response.dnd_set_drag_payload(PoolItem(id));
    }
}

fn grid(
    studio: &mut SueVe,
    ui: &mut Ui,
    items: &[(EntityId, usize)],
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let pad = theme.space(Space::S3);
    let width = theme.space(Space::S9) * CARD_STEPS;
    let label = theme.text_size(TextRole::Caption) * 2.0;
    let height = width * ASPECT + label;
    let room = ui.available_width() - pad;
    let columns = ((room / (width + pad)).floor() as usize).max(1);
    let rows = items.len().div_ceil(columns);
    let (area, _) = ui.allocate_exact_size(
        vec2(room, rows as f32 * (height + pad) + pad),
        Sense::hover(),
    );
    for (index, (id, _)) in items.iter().enumerate() {
        let (row, column) = (index / columns, index % columns);
        let min = pos2(
            area.left() + pad + column as f32 * (width + pad),
            area.top() + pad + row as f32 * (height + pad),
        );
        let card = Rect::from_min_size(min, vec2(width, height));
        let response = ui.interact(
            card,
            egui::Id::unique(("sve-pool-card", *id)),
            Sense::click_and_drag(),
        );
        let picked = studio.picked == Some(*id);
        if picked {
            ui.painter().rect_filled(
                card.expand(theme.space(Space::S1)),
                theme.corner(sue_wm::ui::Radius::Sm),
                theme.fill_selected(),
            );
        } else if response.hovered() {
            ui.painter().rect_filled(
                card.expand(theme.space(Space::S1)),
                theme.corner(sue_wm::ui::Radius::Sm),
                theme.fill_hover(),
            );
        }
        let thumb = Rect::from_min_size(min, vec2(width, width * ASPECT));
        picture(studio, ui, thumb, *id, cx, theme);
        let Ok(entity) = cx.arena().get_by_id(*id) else {
            continue;
        };
        let name = entity.name().to_owned();
        let kind = SourceKind::of(entity);
        let painter = ui.painter_at(card);
        painter.text(
            pos2(card.left(), thumb.bottom() + theme.space(Space::S1)),
            Align2::LEFT_TOP,
            name,
            font(theme, TextRole::Caption),
            theme.text,
        );
        if let Some(length) = model::length_of(cx.arena(), *id) {
            let rate = model::rate_of(entity).unwrap_or(sue_core::Rate::FPS_30);
            let badge = timecode(length, rate);
            let at = pos2(
                thumb.right() - theme.space(Space::S1),
                thumb.bottom() - theme.space(Space::S1),
            );
            let galley = painter.layout_no_wrap(badge, font(theme, TextRole::Micro), theme.text);
            let back = Rect::from_min_size(at - galley.size(), galley.size())
                .expand(theme.space(Space::S1));
            painter.rect_filled(back, theme.corner(sue_wm::ui::Radius::Sm), theme.scrim());
            painter.galley(at - galley.size(), galley, theme.text);
        }
        let side = sue_wm::ui::Density::Compact.icon();
        icons::paint(
            &painter,
            Rect::from_min_size(
                thumb.min + vec2(theme.space(Space::S1), theme.space(Space::S1)),
                vec2(side, side),
            ),
            kind_icon(&kind),
            theme.text,
        );
        if let Some(error) = studio
            .hub
            .error(*id)
            .or_else(|| studio.hub.sounds.error(*id).map(str::to_owned))
        {
            response.clone().on_hover_text(error);
        }
        clicked_item(studio, *id, &response, cx);
    }
}

fn list(
    studio: &mut SueVe,
    ui: &mut Ui,
    items: &[(EntityId, usize)],
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let row = sue_wm::ui::Density::Compact.row();
    let pad = theme.space(Space::S3);
    let room = ui.available_width();
    for (id, depth) in items {
        let Ok(entity) = cx.arena().get_by_id(*id) else {
            continue;
        };
        let kind = SourceKind::of(entity);
        let name = entity.name().to_owned();
        let detail = crate::media::import::summary(entity);
        let length = model::length_of(cx.arena(), *id).map(|length| {
            timecode(
                length,
                model::rate_of(entity).unwrap_or(sue_core::Rate::FPS_30),
            )
        });
        let (rect, response) = ui.allocate_exact_size(vec2(room, row), Sense::click_and_drag());
        if studio.picked == Some(*id) {
            ui.painter()
                .rect_filled(rect, SQUARE, theme.fill_selected());
        } else if response.hovered() {
            ui.painter().rect_filled(rect, SQUARE, theme.fill_hover());
        }
        let indent = pad + *depth as f32 * theme.space(Space::S5);
        let side = sue_wm::ui::Density::Compact.icon();
        icons::paint(
            ui.painter(),
            Rect::from_min_size(
                pos2(rect.left() + indent, rect.center().y - side / 2.0),
                vec2(side, side),
            ),
            kind_icon(&kind),
            theme.text,
        );
        let painter = ui.painter_at(rect);
        painter.text(
            pos2(rect.left() + indent + side + pad, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            font(theme, TextRole::Body),
            theme.text,
        );
        // The length at the right; the rest of what is known on hover.
        let right = length.unwrap_or_default();
        let response = if detail.is_empty() {
            response
        } else {
            response.on_hover_text(detail)
        };
        painter.text(
            pos2(rect.right() - pad, rect.center().y),
            Align2::RIGHT_CENTER,
            right,
            font(theme, TextRole::Caption),
            theme.text_muted(),
        );
        clicked_item(studio, *id, &response, cx);
    }
}
