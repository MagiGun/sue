//! The Material palette and the thumbnail of a material.

use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use sue_wm::{AppletContext, Theme};

use crate::panels::child;
use crate::studio::SuePaint;

/// A material's thumbnail, `side` square, over white.
pub(crate) fn material_thumb(material: &crate::materials::Material, side: u32) -> Vec<u8> {
    let tile = match material.placement {
        crate::materials::Placement::Fit => crate::materials::layer(material, [side, side], [0; 3]),
        crate::materials::Placement::Cover | crate::materials::Placement::Pose => {
            crate::materials::picture(material, [side, side], [0; 3])
        }
    };
    let mut out = vec![255_u8; (side * side * 4) as usize];
    if let Some(bytes) = tile.as_ref().and_then(|tile| tile.bytes()) {
        for (into, from) in out.chunks_mut(4).zip(bytes.chunks(4)) {
            let alpha = u32::from(from[3]);
            for channel in 0..3 {
                into[channel] =
                    ((u32::from(from[channel]) * alpha + 255 * (255 - alpha)) / 255) as u8;
            }
        }
    }
    out
}

impl SuePaint {
    /// The Material palette: Sue Paint's tones and patterns and the
    /// pictures registered, to double-click onto the canvas.
    pub(crate) fn material_panel(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let folder = crate::materials::location(cx);
        let library = self.material_kinds.scan(folder.as_deref());
        let mut inner = child(ui, body, "materials");
        let small = |text: &str| {
            RichText::new(text)
                .size(theme.small_size)
                .color(theme.muted)
        };
        // All, then every kind registered.
        let kinds: Vec<(String, String)> = self
            .material_kinds
            .iter()
            .map(|kind| (kind.id.clone(), crate::localize(&kind.name)))
            .collect();
        inner.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.material_kind, 0, tr!("all"));
            for (index, (_, name)) in kinds.iter().enumerate() {
                ui.selectable_value(&mut self.material_kind, index as u8 + 1, name.as_str());
            }
        });
        let chosen = usize::from(self.material_kind)
            .checked_sub(1)
            .and_then(|index| kinds.get(index))
            .map(|(id, _)| id.as_str());
        let shown: Vec<&crate::materials::Material> = library
            .iter()
            .filter(|material| chosen.is_none_or(|id| material.kind == id))
            .collect();
        let mut paste = None;
        let mut delete = None;
        let side = 52.0;
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-materials")
            .max_height((body.height() - 70.0).max(40.0))
            .show(&mut inner, |ui| {
                let columns = ((ui.available_width() / (side + 10.0)).floor() as usize).max(1);
                for row in shown.chunks(columns) {
                    ui.horizontal(|ui| {
                        for material in row {
                            ui.vertical(|ui| {
                                ui.set_width(side + 4.0);
                                let (rect, response) =
                                    ui.allocate_exact_size(vec2(side, side), Sense::click());
                                let picked =
                                    self.material_pick.as_deref() == Some(material.name.as_str());
                                let key = {
                                    use std::hash::{Hash, Hasher};
                                    let mut hasher =
                                        std::collections::hash_map::DefaultHasher::new();
                                    material.name.hash(&mut hasher);
                                    if let crate::materials::Source::File(path) = &material.source {
                                        path.hash(&mut hasher);
                                    }
                                    hasher.finish()
                                };
                                let thumb = self
                                    .material_thumbs
                                    .entry(key)
                                    .or_insert_with(|| material_thumb(material, side as u32));
                                if let Some(texture) = cx.textures.texture(
                                    key ^ 0x4d41_5445,
                                    1,
                                    [side as u32; 2],
                                    4,
                                    thumb,
                                    false,
                                ) {
                                    ui.painter().image(
                                        texture,
                                        rect,
                                        Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                        Color32::WHITE,
                                    );
                                }
                                ui.painter().rect_stroke(
                                    rect,
                                    0.0,
                                    if picked {
                                        Stroke::new(2.0, theme.border_active)
                                    } else {
                                        theme.hairline()
                                    },
                                    StrokeKind::Outside,
                                );
                                if response.clicked() {
                                    self.material_pick = Some(material.name.clone());
                                }
                                if response.double_clicked() {
                                    paste = Some((*material).clone());
                                }
                                response.context_menu(|ui| {
                                    if ui.button(tr!("paste-to-canvas")).clicked() {
                                        paste = Some((*material).clone());
                                        ui.close();
                                    }
                                    if material.removable()
                                        && ui.button(tr!("delete-material")).clicked()
                                    {
                                        delete = Some((*material).clone());
                                        ui.close();
                                    }
                                });
                                ui.label(small(&crate::localize(&material.name)));
                            });
                        }
                    });
                }
            });
        let picked = shown
            .iter()
            .find(|material| self.material_pick.as_deref() == Some(material.name.as_str()))
            .map(|material| (*material).clone());
        inner.horizontal(|ui| {
            if ui
                .add_enabled(picked.is_some(), egui::Button::new(tr!("paste-to-canvas")))
                .clicked()
            {
                paste = picked.clone();
            }
            if let Some(material) = picked.as_ref().filter(|material| material.removable())
                && ui.button(tr!("delete")).clicked()
            {
                delete = Some(material.clone());
            }
        });
        inner.label(small(&tr!("double-click-paste-edit-register-material")));
        if let Some(material) = paste {
            self.paste_material(cx, &material);
        }
        if let Some(material) = delete
            && let crate::materials::Source::File(path) = &material.source
        {
            match sue_platform::fs::remove_file(path) {
                Ok(()) => cx.notify(tr!("is-no-longer-a-material", name = material.name)),
                Err(error) => cx.fail(tr!("cannot-delete-the-material", error = error)),
            }
            self.material_pick = None;
        }
    }
}
