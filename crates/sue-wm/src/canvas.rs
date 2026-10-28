//! A renderer-agnostic draw list. The shell and applets draw into a
//! [`Canvas`]; the GPU renderer turns it into batches.

use sue_core::AssetHandle;
use sue_platform::Point;
use sue_rice::Rgba;

use crate::Rect;

/// A linear-light RGBA color with straight (non-premultiplied) alpha.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color {
    /// Red.
    pub r: f32,
    /// Green.
    pub g: f32,
    /// Blue.
    pub b: f32,
    /// Alpha.
    pub a: f32,
}

impl Color {
    /// Fully transparent.
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    /// Converts an sRGB-encoded rice color to linear light.
    #[must_use]
    pub fn from_rgba(color: Rgba) -> Self {
        fn linear(channel: u8) -> f32 {
            let c = f32::from(channel) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        Self {
            r: linear(color.r),
            g: linear(color.g),
            b: linear(color.b),
            a: f32::from(color.a) / 255.0,
        }
    }

    /// The same color with alpha multiplied by `alpha`.
    #[must_use]
    pub fn fade(self, alpha: f32) -> Self {
        Self {
            a: self.a * alpha.clamp(0.0, 1.0),
            ..self
        }
    }

    /// Linear interpolation towards `other`.
    #[must_use]
    pub fn mix(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let lerp = |a: f32, b: f32| a + (b - a) * t;
        Self {
            r: lerp(self.r, other.r),
            g: lerp(self.g, other.g),
            b: lerp(self.b, other.b),
            a: lerp(self.a, other.a),
        }
    }
}

/// Which bundled font a text run uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontFace {
    /// The proportional UI font.
    Ui,
    /// The monospaced font (code, hex).
    Mono,
}

/// Fill, border and corner rounding of a box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxStyle {
    /// Fill color.
    pub fill: Color,
    /// Border color.
    pub border: Color,
    /// Border width in logical pixels.
    pub border_width: f32,
    /// Corner radius in logical pixels.
    pub radius: f32,
}

impl BoxStyle {
    /// A borderless box.
    #[must_use]
    pub fn filled(fill: Color, radius: f32) -> Self {
        Self {
            fill,
            border: Color::TRANSPARENT,
            border_width: 0.0,
            radius,
        }
    }
}

/// One drawing operation, in logical pixels, with the clip active when issued.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawCommand {
    /// A rounded rectangle.
    Box {
        /// Where.
        rect: Rect,
        /// How.
        style: BoxStyle,
        /// Visible region.
        clip: Rect,
    },
    /// One line of text; `position` is the top-left of the line box.
    Text {
        /// Top-left of the line box.
        position: Point,
        /// The text.
        text: String,
        /// Font.
        face: FontFace,
        /// Size in logical pixels.
        size: f32,
        /// Color.
        color: Color,
        /// Visible region.
        clip: Rect,
    },
    /// An arena raster (`RasterTile2D`), stretched over `rect`.
    Image {
        /// Where.
        rect: Rect,
        /// The raster entity (LAW 2: a handle, never pixels).
        image: AssetHandle,
        /// `false` samples nearest-neighbour (crisp pixels, byte maps).
        smooth: bool,
        /// Visible region.
        clip: Rect,
    },
}

/// A frame's draw list with a clip stack.
#[derive(Clone, Debug)]
pub struct Canvas {
    viewport: Rect,
    clips: Vec<Rect>,
    commands: Vec<DrawCommand>,
}

impl Canvas {
    /// An empty canvas covering `viewport`.
    #[must_use]
    pub fn new(viewport: Rect) -> Self {
        Self {
            viewport,
            clips: Vec::new(),
            commands: Vec::new(),
        }
    }

    /// The whole drawable area.
    #[must_use]
    pub fn viewport(&self) -> Rect {
        self.viewport
    }

    /// The active clip rectangle.
    #[must_use]
    pub fn clip(&self) -> Rect {
        self.clips.last().copied().unwrap_or(self.viewport)
    }

    /// Restricts drawing to `rect` (intersected with the current clip).
    pub fn push_clip(&mut self, rect: Rect) {
        let clip = intersect(self.clip(), rect);
        self.clips.push(clip);
    }

    /// Restores the previous clip.
    pub fn pop_clip(&mut self) {
        self.clips.pop();
    }

    /// Draws a box.
    pub fn draw_box(&mut self, rect: Rect, style: BoxStyle) {
        let clip = self.clip();
        if rect.overlaps(&clip) && (style.fill.a > 0.0 || style.border.a > 0.0) {
            self.commands.push(DrawCommand::Box { rect, style, clip });
        }
    }

    /// Draws one line of text with its line box's top-left at `position`.
    pub fn draw_text(
        &mut self,
        position: Point,
        text: &str,
        face: FontFace,
        size: f32,
        color: Color,
    ) {
        let clip = self.clip();
        if !text.is_empty() && color.a > 0.0 && clip.width > 0.0 && clip.height > 0.0 {
            self.commands.push(DrawCommand::Text {
                position,
                text: text.to_owned(),
                face,
                size,
                color,
                clip,
            });
        }
    }

    /// Draws an arena raster.
    pub fn draw_image(&mut self, rect: Rect, image: AssetHandle, smooth: bool) {
        let clip = self.clip();
        if rect.overlaps(&clip) {
            self.commands.push(DrawCommand::Image {
                rect,
                image,
                smooth,
                clip,
            });
        }
    }

    /// Every command, in painter's order.
    #[must_use]
    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }
}

/// The overlap of two rectangles (zero-sized if they do not overlap).
#[must_use]
pub fn intersect(a: Rect, b: Rect) -> Rect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = a.right().min(b.right());
    let bottom = a.bottom().min(b.bottom());
    Rect::new(x, y, (right - x).max(0.0), (bottom - y).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_colors_convert_to_linear() {
        let white = Color::from_rgba(Rgba::new(255, 255, 255, 255));
        assert!((white.r - 1.0).abs() < 1e-6 && (white.a - 1.0).abs() < 1e-6);
        let mid = Color::from_rgba(Rgba::new(128, 128, 128, 128));
        assert!((mid.r - 0.2158).abs() < 1e-3, "{}", mid.r);
        assert!((Color::TRANSPARENT.mix(white, 0.5).a - 0.5).abs() < 1e-6);
    }

    #[test]
    fn clips_nest_and_cull() {
        let mut canvas = Canvas::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        canvas.push_clip(Rect::new(10.0, 10.0, 50.0, 50.0));
        canvas.push_clip(Rect::new(40.0, 40.0, 50.0, 50.0));
        assert_eq!(canvas.clip(), Rect::new(40.0, 40.0, 20.0, 20.0));
        canvas.draw_box(
            Rect::new(0.0, 0.0, 5.0, 5.0),
            BoxStyle::filled(Color::from_rgba(Rgba::new(1, 2, 3, 255)), 0.0),
        );
        assert!(
            canvas.commands().is_empty(),
            "boxes outside the clip are culled"
        );
        canvas.draw_text(
            Point::new(45.0, 45.0),
            "Sue",
            FontFace::Ui,
            14.0,
            Color::from_rgba(Rgba::new(9, 9, 9, 255)),
        );
        canvas.pop_clip();
        canvas.pop_clip();
        assert_eq!(canvas.clip(), canvas.viewport());
        assert!(
            matches!(&canvas.commands()[0], DrawCommand::Text { clip, .. } if clip.width == 20.0)
        );
    }
}
