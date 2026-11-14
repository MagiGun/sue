//! A research item shown in place of the text.

use egui::{Align2, Color32, Rect, Ui, pos2, vec2};
use sue_core::{Entity, EntityId};
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
                    size: vec2(width as f32, height as f32),
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
        ui.painter().rect_filled(rect, 0.0, theme.void);
        if let Some(figure) = self.figures.get(&id)
            && let Some(texture) = figure.texture
        {
            let room = rect.shrink(40.0);
            let scale = (room.width() / figure.size.x)
                .min(room.height() / figure.size.y)
                .min(1.0);
            let shown = Rect::from_center_size(room.center(), figure.size * scale);
            ui.painter().image(
                texture,
                shown,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let name = cx
            .arena()
            .get_by_id(id)
            .map_or_else(|_| String::new(), |entity| entity.name().to_owned());
        ui.painter().text(
            rect.left_top() + vec2(16.0, 14.0),
            Align2::LEFT_TOP,
            name,
            theme.font(theme.ui_size),
            theme.muted,
        );
    }
}
