//! The Information palette: the canvas, the memory its pixels take, and
//! what is under the pointer (where, in the unit chosen, and its color),
//! with the length of a drag on the canvas while it lasts. Its palette
//! menu chooses what shows and the unit (gap WS-22 of the 0.0.8 plan);
//! the choices are kept in Sue Paint's data folder.

use egui::{Align2, Pos2, Rect, Sense, Ui, pos2};
use sue_wm::ui::{Space, TextRole};
use sue_wm::{AppletContext, Theme};

use crate::studio::SuePaint;

/// The commands of the Information palette's menu.
pub(crate) const MENU_COMMANDS: [&str; 8] = [
    "info.canvas",
    "info.memory",
    "info.coordinates",
    "info.length",
    "info.unit_px",
    "info.unit_mm",
    "info.unit_cm",
    "info.unit_in",
];

/// The unit coordinates and lengths are shown in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Unit {
    /// Picture pixels.
    #[default]
    Px,
    /// Millimeters at the picture's resolution.
    Mm,
    /// Centimeters.
    Cm,
    /// Inches.
    In,
}

impl Unit {
    /// Every unit, in the menu's order.
    pub(crate) const ALL: [Self; 4] = [Self::Px, Self::Mm, Self::Cm, Self::In];

    /// Its word in the file and in its command's name.
    pub(crate) const fn word(self) -> &'static str {
        match self {
            Self::Px => "px",
            Self::Mm => "mm",
            Self::Cm => "cm",
            Self::In => "in",
        }
    }

    /// The unit called `word`.
    pub(crate) fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|unit| unit.word() == word)
    }

    /// `pixels` picture pixels at `dpi`, in this unit, as people read it.
    pub(crate) fn show(self, pixels: f64, dpi: f64) -> String {
        let inches = pixels / dpi.max(1.0);
        match self {
            Self::Px => tr!("info-px", value = sue_lang::number(pixels, 0)),
            Self::Mm => tr!("info-mm", value = sue_lang::number(inches * 25.4, 1)),
            Self::Cm => tr!("info-cm", value = sue_lang::number(inches * 2.54, 2)),
            Self::In => tr!("info-in", value = sue_lang::number(inches, 2)),
        }
    }
}

/// What the Information palette shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InfoOptions {
    /// The canvas: its size, print size, layers, zoom and angle.
    pub canvas: bool,
    /// The memory the picture's pixels take.
    pub memory: bool,
    /// Where the pointer is, and its color.
    pub coordinates: bool,
    /// The length of a drag on the canvas.
    pub length: bool,
    /// The unit of coordinates and lengths.
    pub unit: Unit,
}

impl Default for InfoOptions {
    fn default() -> Self {
        Self {
            canvas: true,
            memory: true,
            coordinates: true,
            length: true,
            unit: Unit::Px,
        }
    }
}

impl InfoOptions {
    /// The file in Sue Paint's data folder that keeps them.
    pub(crate) const FILE: &'static str = "information.txt";

    /// Them as the file's lines (`memory 1`, `unit mm`).
    pub(crate) fn to_text(self) -> String {
        format!(
            "canvas {}\nmemory {}\ncoordinates {}\nlength {}\nunit {}\n",
            u8::from(self.canvas),
            u8::from(self.memory),
            u8::from(self.coordinates),
            u8::from(self.length),
            self.unit.word()
        )
    }

    /// Them from the file's lines; what does not read stays as it was.
    pub(crate) fn from_text(text: &str) -> Self {
        let mut options = Self::default();
        for line in text.lines() {
            let Some((word, value)) = line.trim().split_once(' ') else {
                continue;
            };
            let on = value.trim() == "1";
            match word {
                "canvas" => options.canvas = on,
                "memory" => options.memory = on,
                "coordinates" => options.coordinates = on,
                "length" => options.length = on,
                "unit" => {
                    if let Some(unit) = Unit::from_word(value.trim()) {
                        options.unit = unit;
                    }
                }
                _ => {}
            }
        }
        options
    }
}

/// The memory `bytes` take, as people read it.
fn megabytes(bytes: usize) -> String {
    sue_lang::number(bytes as f64 / (1024.0 * 1024.0), 1)
}

/// The canvas's lines: its size, print size, layers, zoom and angle
/// (`view`).
fn canvas_lines(
    lines: &mut Vec<String>,
    size: [u32; 2],
    dpi: f64,
    layers: usize,
    view: (f32, f32),
) {
    lines.push(tr!(
        "canvas-x-px-dpi",
        value = size[0],
        value2 = size[1],
        dpi = sue_lang::number(dpi, 0)
    ));
    lines.push(tr!(
        "print-size-x-cm",
        value = sue_lang::number(f64::from(size[0]) / dpi * 2.54, 1),
        value2 = sue_lang::number(f64::from(size[1]) / dpi * 2.54, 1)
    ));
    lines.push(tr!("layers-2", count = layers));
    lines.push(tr!(
        "zoom-percent-angle",
        value = sue_lang::number(f64::from(view.0 * 100.0), 0),
        rotation = sue_lang::number(f64::from(view.1), 0)
    ));
}

impl SuePaint {
    /// Turns an Information option on or off, or picks its unit, and
    /// keeps the choice.
    pub(crate) fn set_info(
        &mut self,
        cx: &mut AppletContext<'_>,
        change: impl FnOnce(&mut InfoOptions),
    ) {
        change(&mut self.extras.info);
        let text = self.extras.info.to_text();
        let folder = crate::colorsets::folder(cx);
        if let Err(error) = crate::extras::write(folder.as_deref(), InfoOptions::FILE, &text) {
            cx.fail(tr!("cannot-keep-the-palettes-settings", error = error));
        }
    }

    /// The lines the Information palette shows now.
    pub(crate) fn info_lines(
        &self,
        cx: &AppletContext<'_>,
        pointer: Option<Pos2>,
        pressed: Option<Pos2>,
    ) -> Vec<String> {
        let options = self.extras.info;
        let mut lines = Vec::new();
        let viewport = self.viewport;
        let Some(doc) = self.doc() else {
            lines.push(tr!("no-canvas"));
            return lines;
        };
        let arena = &cx.world.arena;
        let entity = arena.get_by_id(doc.picture).ok();
        let size = entity.and_then(sue_model::picture::size).unwrap_or([0, 0]);
        let dpi = entity
            .and_then(|entity| entity.property(sue_model::picture::DPI))
            .and_then(sue_core::Value::as_f64)
            .unwrap_or(72.0);
        let layers = crate::doc::layers(arena, doc.picture);
        if options.canvas {
            let view = (doc.view.zoom, doc.view.rotation);
            canvas_lines(&mut lines, size, dpi, layers.len(), view);
        }
        if options.memory {
            let bytes: usize = layers
                .iter()
                .filter_map(|layer| crate::doc::raster(arena, layer.id))
                .map(sue_core::RasterTile2D::byte_len)
                .sum();
            lines.push(tr!("info-memory", value = megabytes(bytes)));
        }
        let unit = options.unit;
        if options.coordinates {
            if let Some(pointer) = pointer.filter(|pointer| viewport.contains(*pointer)) {
                let at = doc.view.to_picture(viewport, size, pointer);
                lines.push(tr!(
                    "info-pointer",
                    x = unit.show(f64::from(at.x), dpi),
                    y = unit.show(f64::from(at.y), dpi)
                ));
                let (pixels, shown, _) = doc.compositor.display();
                if at.x >= 0.0
                    && at.y >= 0.0
                    && (at.x as u32) < shown[0]
                    && (at.y as u32) < shown[1]
                {
                    let index = ((at.y as u32 * shown[0] + at.x as u32) * 4) as usize;
                    if let Some(rgba) = pixels.get(index..index + 4) {
                        lines.push(tr!(
                            "color-r-g-b-a",
                            value = rgba[0],
                            value2 = rgba[1],
                            value3 = rgba[2],
                            value4 = rgba[3],
                            value5 = format!("{:02X}", rgba[0]),
                            value6 = format!("{:02X}", rgba[1]),
                            value7 = format!("{:02X}", rgba[2])
                        ));
                    }
                }
            } else {
                lines.push(tr!("pointer-off-the-canvas"));
            }
        }
        if options.length
            && let (Some(from), Some(to)) = (pressed, pointer)
            && viewport.contains(from)
        {
            let a = doc.view.to_picture(viewport, size, from);
            let b = doc.view.to_picture(viewport, size, to);
            let length = f64::from((b - a).length());
            lines.push(tr!("info-length", value = unit.show(length, dpi)));
        }
        lines
    }

    /// The Information palette.
    pub(crate) fn info_tab(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let painter = ui.painter().with_clip_rect(body);
        let pressed = ui.ctx().input(|input| {
            input
                .pointer
                .primary_down()
                .then(|| input.pointer.press_origin())
                .flatten()
        });
        let lines = self.info_lines(cx, self.pointer, pressed);
        let font = theme.text_font(TextRole::Caption);
        let pad = theme.space(Space::S4);
        let step = theme.text_size(TextRole::Caption) + theme.space(Space::S3);
        for (row, line) in lines.iter().enumerate() {
            painter.text(
                pos2(body.left() + pad, body.top() + pad + row as f32 * step),
                Align2::LEFT_TOP,
                line,
                font.clone(),
                theme.text,
            );
        }
        // Keep it current while the pointer moves.
        let _ = ui.interact(body, ui.scope_id().with("sue-paint-info"), Sense::hover());
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn information_options_round_trip_as_text() {
        let options = InfoOptions {
            canvas: false,
            memory: false,
            coordinates: true,
            length: false,
            unit: Unit::Mm,
        };
        assert_eq!(InfoOptions::from_text(&options.to_text()), options);
        // An empty or broken file keeps the defaults.
        assert_eq!(InfoOptions::from_text(""), InfoOptions::default());
        assert_eq!(
            InfoOptions::from_text("unit furlong\nmemory"),
            InfoOptions::default()
        );
    }

    #[test]
    fn lengths_read_in_every_unit() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        // 300 pixels at 300 dpi are an inch.
        assert_eq!(Unit::Px.show(300.0, 300.0), "300 px");
        assert_eq!(Unit::In.show(300.0, 300.0), "1.00 in");
        assert_eq!(Unit::Cm.show(300.0, 300.0), "2.54 cm");
        assert_eq!(Unit::Mm.show(300.0, 300.0), "25.4 mm");
    }
}
