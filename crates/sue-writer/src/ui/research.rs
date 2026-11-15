//! A research item shown in place of the text: a picture, as large as
//! fits and never larger than it is, under its name.

use egui::{Align2, CornerRadius, Rect, Ui, UiBuilder, Vec2};
use sue_core::{Entity, EntityId};
use sue_wm::icons::Icon;
use sue_wm::ui::{self, Radius, Space, TextRole};
use sue_wm::{AppletContext, Theme};

use crate::studio::{Figure, SueWriter};

use super::widgets::texture_key;

impl SueWriter {
    pub(super) fn research_item_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        id: EntityId,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let seen = cx.arena().get_by_id(id).map_or(0, Entity::revision);
        if self
            .figures
            .get(&id)
            .is_none_or(|figure| figure.seen != seen)
            && let Some((pixels, [width, height])) =
                crate::compile::picture_pixels(cx.arena(), id, 1600)
        {
            self.figures.insert(
                id,
                Figure {
                    texture: None,
                    size: Vec2::new(width as f32, height as f32),
                    seen,
                    pixels,
                    pixel_size: [width, height],
                },
            );
        }
        if let Some(figure) = self.figures.get_mut(&id) {
            figure.texture = cx.textures.texture(
                texture_key(id),
                figure.seen,
                figure.pixel_size,
                4,
                &figure.pixels,
                false,
            );
        }
        ui.painter()
            .rect_filled(rect, CornerRadius::ZERO, theme.void);
        let name = cx
            .arena()
            .get_by_id(id)
            .map_or_else(|_| String::new(), |entity| entity.name().to_owned());
        let margin = theme.space(Space::S9);
        if let Some(figure) = self.figures.get(&id) {
            // As large as fits, never larger than it is.
            let room = rect.shrink(margin);
            let size = figure.size;
            let scale = (room.width() / size.x).min(room.height() / size.y).min(1.0);
            let shown = Rect::from_center_size(room.center(), size * scale);
            let corner = theme.corner(Radius::Sm);
            match figure.texture {
                Some(texture) => {
                    egui::Image::new((texture, shown.size()))
                        .corner_radius(corner)
                        .paint_at(ui, shown);
                }
                // Its pixels are on their way to the screen.
                None => ui::paint_skeleton(ui, shown, corner, theme),
            }
        } else {
            let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
            ui::empty_state(("writer-research-empty", id), &tr!("research-empty"))
                .icon(Icon::Picture)
                .detail(&tr!("research-empty-detail"))
                .show(&mut child, theme);
        }
        let pad = theme.space(Space::S6);
        ui.painter().text(
            rect.left_top() + Vec2::new(pad, theme.space(Space::S5)),
            Align2::LEFT_TOP,
            name,
            theme.text_font(TextRole::Title),
            theme.text_muted(),
        );
    }
}
