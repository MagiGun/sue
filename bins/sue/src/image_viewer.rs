//! Image Viewer: shows a decoded arena raster with pan, zoom and metadata.

use sue_core::{AssetHandle, ComponentKind};
use sue_io::meta;
use sue_platform::{CursorShape, InputEvent, Point};
use sue_wm::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, BoxStyle, FontFace,
    InputResponse, MediaClass, Rect, SueApplet, Ui, World, Zone, widget_id,
};

use crate::drop_zone::human_bytes;

/// The applet id.
pub const ID: &str = "image-viewer";

/// Pan and zoom over one raster entity.
#[derive(Debug)]
pub struct ImageViewer {
    image: Option<AssetHandle>,
    zoom: f32,
    pan: Point,
    last_pointer: Option<Point>,
}

impl Default for ImageViewer {
    fn default() -> Self {
        Self {
            image: None,
            zoom: 1.0,
            pan: Point::default(),
            last_pointer: None,
        }
    }
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
            category: AppletCategory::Utility,
            home_zone: Zone::CenterStage,
            uses_timeline: false,
            summary: "Decoded JPEG XL with pan, zoom and metadata.".to_owned(),
            opens: vec![MediaClass::Image],
            accepts_drops: false,
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
        let entity = world.arena.get(self.image?).ok()?;
        Some(format!("Image Viewer — {}", entity.name()))
    }

    fn draw(&mut self, ui: &mut Ui<'_>, area: Rect, ctx: &mut AppletContext<'_>) {
        let theme = ui.theme;
        let Some((handle, width, height)) = self.image.and_then(|handle| {
            let tile = ctx
                .arena()
                .get(handle)
                .ok()?
                .component(ComponentKind::RasterTile2D)?
                .as_raster()?;
            Some((handle, tile.width() as f32, tile.height() as f32))
        }) else {
            ui.label_centered(
                area,
                "Drop a .jxl file onto Sue to view it here",
                theme.muted,
            );
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
        let scale = fit * self.zoom;
        let rect = Rect::new(
            area.x + (area.width - width * scale) / 2.0 + self.pan.x,
            area.y + (area.height - height * scale) / 2.0 + self.pan.y,
            width * scale,
            height * scale,
        );
        ui.canvas.draw_image(rect, handle, scale < 2.0);

        // Metadata card.
        let entity = ctx.arena().get(handle).ok();
        let lookup = |key: &str| {
            entity
                .and_then(|entity| entity.meta(key))
                .unwrap_or("?")
                .to_owned()
        };
        let name = entity.map_or_else(String::new, |entity| entity.name().to_owned());
        let bytes = entity
            .and_then(|entity| entity.meta(meta::BYTES))
            .and_then(|bytes| bytes.parse::<u64>().ok())
            .map_or_else(|| "?".to_owned(), human_bytes);
        let millis = entity
            .and_then(|entity| entity.meta(meta::MICROS))
            .and_then(|micros| micros.parse::<f64>().ok())
            .map_or_else(
                || "?".to_owned(),
                |micros| format!("{:.1} ms", micros / 1000.0),
            );
        let lines = [
            name,
            format!(
                "{} × {} · {} · {}-bit",
                lookup(meta::WIDTH),
                lookup(meta::HEIGHT),
                lookup(meta::COLOR),
                lookup(meta::BITS)
            ),
            format!("{bytes} · decoded in {millis} · zoom {:.0}%", scale * 100.0),
            lookup(meta::ROUTE),
        ];
        let line = ui.row_height() * 0.85;
        let pad = theme.padding();
        let card_width = lines
            .iter()
            .map(|text| ui.fonts.measure(FontFace::Mono, theme.mono_size, text))
            .fold(0.0_f32, f32::max)
            + pad * 2.0;
        let card = Rect::new(
            area.x + pad,
            area.bottom() - pad - line * 4.0 - pad,
            card_width.min(area.width - pad * 2.0),
            line * 4.0 + pad,
        );
        ui.canvas.draw_box(
            card,
            BoxStyle::filled(theme.surface.fade(0.92), theme.radius),
        );
        for (index, text) in lines.iter().enumerate() {
            let color = if index == 0 { theme.text } else { theme.muted };
            ui.text(
                Point::new(card.x + pad, card.y + pad / 2.0 + line * index as f32),
                text,
                FontFace::Mono,
                theme.mono_size,
                color,
            );
        }
        let fit_button = Rect::new(
            area.right() - pad - line * 2.5,
            area.y + pad,
            line * 2.5,
            line,
        );
        if ui.button(widget_id("image-viewer", "fit"), fit_button, "Fit") {
            self.reset_view();
        }
    }
}
