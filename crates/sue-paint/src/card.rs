//! Pictures on Sue's canvas. Every picture made in Sue Paint also sits on
//! the infinite canvas as a card: the very same entity, shown as a preview
//! that follows its layers, with a button that opens it in Sue Paint.

use std::collections::HashMap;

use egui::{Color32, Pos2, Rect, Ui, pos2, vec2};
use sue_core::{Entity, EntityId};
use sue_model::picture;
use sue_wm::{
    AppletContext, AppletId, AppletManifest, AppletRole, NodeId, ShellRequest, SueApplet, widgets,
};

use crate::composite;
use crate::studio::{ID, texture_key};

/// The applet id of the card.
pub const CARD_ID: &str = "illustration";

/// The largest side of a card's preview, in pixels.
const PREVIEW_SIDE: u32 = 640;

/// A card's preview and the layer revisions it shows.
#[derive(Debug)]
struct Preview {
    seen: Vec<(EntityId, u64)>,
    pixels: Vec<u8>,
    size: [u32; 2],
    version: u64,
}

/// Sue Paint pictures on the canvas.
#[derive(Debug, Default)]
pub struct PictureCard {
    previews: HashMap<EntityId, Preview>,
}

impl SueApplet for PictureCard {
    fn manifest(&self) -> AppletManifest {
        let mut manifest =
            AppletManifest::new(CARD_ID, "Illustration", AppletRole::Node, "\u{1F5BC}");
        manifest.summary = "A Sue Paint picture on the canvas.".to_owned();
        manifest.order = 21;
        manifest
    }

    fn accepts(&self, entity: &Entity) -> bool {
        picture::is_picture(entity)
    }

    fn node_size(&self, entity: &Entity) -> egui::Vec2 {
        let [width, height] = picture::size(entity).unwrap_or([4, 3]);
        let scale = (480.0 / width.max(1) as f32).min(360.0 / height.max(1) as f32);
        vec2(
            width as f32 * scale,
            height as f32 * scale + sue_wm::NODE_TITLE,
        )
    }

    fn node_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {
        let area = ui.max_rect();
        ui.painter().rect_filled(area, 0.0, cx.theme.void);
        let Some(entity) = cx.entity(node) else {
            return;
        };
        let id = entity.id();
        let seen = composite::signature(&cx.world.arena, id);
        let fresh = self
            .previews
            .get(&id)
            .is_some_and(|preview| preview.seen == seen);
        if !fresh
            && let Some((pixels, size)) =
                composite::preview(&cx.world.arena, id, [PREVIEW_SIDE, PREVIEW_SIDE])
        {
            let version = self
                .previews
                .get(&id)
                .map_or(0, |preview| preview.version + 1);
            self.previews.insert(
                id,
                Preview {
                    seen,
                    pixels,
                    size,
                    version,
                },
            );
        }
        let Some(preview) = self.previews.get(&id) else {
            return;
        };
        let [width, height] = preview.size;
        let aspect = width as f32 / height.max(1) as f32;
        let fitted = if area.width() / area.height().max(1.0) > aspect {
            vec2(area.height() * aspect, area.height())
        } else {
            vec2(area.width(), area.width() / aspect)
        };
        let image = Rect::from_center_size(area.center(), fitted);
        match cx.textures.texture(
            texture_key(id, 0x4341_5244),
            preview.version,
            preview.size,
            4,
            &preview.pixels,
            false,
        ) {
            Some(texture) => {
                ui.painter().image(
                    texture,
                    image,
                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                ui.painter()
                    .rect_filled(image, 0.0, Color32::from_gray(235));
            }
        }
        if ui
            .interact(
                area,
                egui::Id::unique(("sue-paint-card", node.0)),
                egui::Sense::click(),
            )
            .double_clicked()
        {
            open_studio(cx);
        }
    }

    fn pill_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        let facts = cx.entity(node).and_then(|entity| {
            let [width, height] = picture::size(entity)?;
            let layers = picture::layers(&cx.world.arena, entity.id()).len();
            Some(format!("{width} x {height} px, {layers} layers"))
        });
        if let Some(facts) = facts {
            ui.label(
                egui::RichText::new(facts)
                    .size(theme.small_size)
                    .color(theme.muted),
            );
            widgets::pill_divider(ui, &theme);
        }
        if widgets::pill_button(ui, "Open in Sue Paint", false, &theme)
            .on_hover_text("Paint on it (double-click the card does the same)")
            .clicked()
        {
            open_studio(cx);
        }
    }
}

/// Opens Sue Paint, which shows the selected picture.
fn open_studio(cx: &mut AppletContext<'_>) {
    if let Ok(id) = AppletId::new(ID) {
        cx.request(ShellRequest::OpenStudio(id));
    }
}
