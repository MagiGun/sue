//! Image Viewer: shows a decoded arena raster with pan and zoom. Its facts
//! (size, depth, channels, zoom) go to the Inspector.

use sue_core::{AssetHandle, ComponentKind};
use sue_io::meta;
use sue_platform::{CursorShape, InputEvent, Point};
use sue_wm::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, BoxStyle, FontFace,
    InputResponse, MediaClass, PropertyGroup, Rect, SueApplet, Ui, World, Zone, widget_id,
};

use crate::util::human_bytes;

/// The applet id.
pub const ID: &str = "image-viewer";

/// Pan and zoom over one raster entity.
#[derive(Debug)]
pub struct ImageViewer {
    image: Option<AssetHandle>,
    zoom: f32,
    pan: Point,
    last_pointer: Option<Point>,
    /// Screen pixels per image pixel in the last frame.
    scale: f32,
}

impl Default for ImageViewer {
    fn default() -> Self {
        Self {
            image: None,
            zoom: 1.0,
            pan: Point::default(),
            last_pointer: None,
            scale: 1.0,
        }
    }
}

/// Channels in a colour model such as `RGB + alpha`.
fn channels(model: &str) -> usize {
    let base = if model.starts_with("Gray") { 1 } else { 3 };
    base + usize::from(model.contains("alpha"))
}

impl ImageViewer {
    fn reset_view(&mut self) {
        self.zoom = 1.0;
        self.pan = Point::default();
    }
}

impl SueApplet for ImageViewer {
    fn manifest(&self) -> AppletManifest {
        AppletManifest {
            id: AppletId::new(ID).expect("the Image Viewer id is valid"),
            name: "Image Viewer".to_owned(),
            category: AppletCategory::Inspector,
            home_zone: Zone::CenterStage,
            uses_timeline: false,
            summary: "Decoded JPEG XL, PNG and JPEG pictures with pan, zoom and fit.".to_owned(),
            opens: vec![MediaClass::Image],
            icon: "\u{1F5BC}".to_owned(),
            singleton: false,
        }
    }

    fn mount(&mut self, _ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        Ok(())
    }

    fn open(&mut self, asset: AssetHandle, ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        ctx.arena()
            .get(asset)?
            .component(ComponentKind::RasterTile2D)
            .ok_or_else(|| AppletError::new("the asset is not a raster"))?;
        self.image = Some(asset);
        self.reset_view();
        Ok(())
    }

    fn handle_input(
        &mut self,
        _event: &InputEvent,
        _ctx: &mut AppletContext<'_>,
    ) -> Result<InputResponse, AppletError> {
        Ok(InputResponse::Ignored)
    }

    fn title(&self, world: &World) -> Option<String> {
        Some(world.arena.get(self.image?).ok()?.name().to_owned())
    }

    fn inspect(&self, world: &World) -> Vec<PropertyGroup> {
        let Some(entity) = self.image.and_then(|image| world.arena.get(image).ok()) else {
            return Vec::new();
        };
        let lookup = |key: &str| entity.meta(key).unwrap_or("?").to_owned();
        let model = lookup(meta::COLOR);
        let mut groups = vec![PropertyGroup::new(
            "Image",
            [
                ("Name", entity.name().to_owned()),
                ("Format", lookup(meta::FORMAT)),
                (
                    "Dimensions",
                    format!("{} x {} px", lookup(meta::WIDTH), lookup(meta::HEIGHT)),
                ),
                ("Bit depth", format!("{}-bit", lookup(meta::BITS))),
                ("Color", model.clone()),
                ("Channels", channels(&model).to_string()),
                ("Zoom", format!("{:.0}%", self.scale * 100.0)),
            ],
        )];
        let size = entity
            .meta(meta::BYTES)
            .and_then(|bytes| bytes.parse::<u64>().ok())
            .map_or_else(|| "?".to_owned(), human_bytes);
        let decoded = entity
            .meta(meta::MICROS)
            .and_then(|micros| micros.parse::<f64>().ok())
            .map_or_else(
                || "?".to_owned(),
                |micros| format!("{:.1} ms", micros / 1000.0),
            );
        groups.push(PropertyGroup::new(
            "File",
            [
                ("Size", size),
                ("Decoded in", decoded),
                ("Route", lookup(meta::ROUTE)),
                (
                    "Path",
                    entity
                        .meta(meta::SOURCE)
                        .unwrap_or("Not from a file")
                        .to_owned(),
                ),
            ],
        ));
        groups
    }

    fn draw(&mut self, ui: &mut Ui<'_>, area: Rect, ctx: &mut AppletContext<'_>) {
        let theme = ui.theme;
        let pad = theme.padding;
        let Some((handle, width, height)) = self.image.and_then(|handle| {
            let tile = ctx
                .arena()
                .get(handle)
                .ok()?
                .component(ComponentKind::RasterTile2D)?
                .as_raster()?;
            Some((handle, tile.width() as f32, tile.height() as f32))
        }) else {
            ui.label_centered(area, "Open or drop a picture to view it here", theme.muted);
            return;
        };
        let response = ui.interact(widget_id("image-viewer", "canvas"), area);
        let wheel = ui.wheel_in(area);
        if wheel != 0.0
            && let Some(pointer) = ui.input.pointer
        {
            let notch = ctx.rice().get_f64("input.wheel_line_px").unwrap_or(48.0) as f32;
            let factor = (wheel / (notch * 4.0)).exp2();
            let new_zoom = (self.zoom * factor).clamp(0.05, 64.0);
            // Keep the point under the pointer fixed while zooming.
            let center = Point::new(
                area.x + area.width / 2.0 + self.pan.x,
                area.y + area.height / 2.0 + self.pan.y,
            );
            let ratio = new_zoom / self.zoom;
            self.pan.x += (pointer.x - center.x) * (1.0 - ratio);
            self.pan.y += (pointer.y - center.y) * (1.0 - ratio);
            self.zoom = new_zoom;
        }
        if response.dragging {
            if let (Some(last), Some(now)) = (self.last_pointer, ui.input.pointer) {
                self.pan.x += now.x - last.x;
                self.pan.y += now.y - last.y;
            }
            self.last_pointer = ui.input.pointer;
            ui.set_cursor(CursorShape::Grabbing);
        } else {
            self.last_pointer = None;
            if response.hovered {
                ui.set_cursor(CursorShape::Grab);
            }
        }
        let fit = (area.width / width).min(area.height / height);
        self.scale = fit * self.zoom;
        let rect = Rect::new(
            area.x + (area.width - width * self.scale) / 2.0 + self.pan.x,
            area.y + (area.height - height * self.scale) / 2.0 + self.pan.y,
            width * self.scale,
            height * self.scale,
        );
        ui.canvas.draw_image(rect, handle, self.scale < 2.0);

        // A small zoom readout and a fit button, top right.
        let row = ui.row_height();
        let readout = format!("{:.0}%", self.scale * 100.0);
        let readout_width = ui.fonts.measure(FontFace::Ui, theme.small_size, &readout) + pad * 2.0;
        let fit_width = ui.fonts.measure(FontFace::Ui, theme.ui_size, "Fit") + pad * 3.0;
        let fit_button = Rect::new(area.right() - pad - fit_width, area.y + pad, fit_width, row);
        let badge = Rect::new(
            fit_button.x - pad / 2.0 - readout_width,
            fit_button.y,
            readout_width,
            row,
        );
        ui.canvas
            .draw_box(badge, BoxStyle::filled(theme.surface, theme.radius));
        ui.label_centered_sized(badge, &readout, theme.small_size, theme.muted);
        if ui.button(widget_id("image-viewer", "fit"), fit_button, "Fit") {
            self.reset_view();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_models_count_their_channels() {
        assert_eq!(channels("Gray"), 1);
        assert_eq!(channels("Gray + alpha"), 2);
        assert_eq!(channels("RGB"), 3);
        assert_eq!(channels("RGB + alpha"), 4);
    }
}
