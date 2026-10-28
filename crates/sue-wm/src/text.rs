//! Text measurement, layout and rasterization with `fontdue` and the bundled
//! fonts (Ubuntu Light, Hack, Noto Emoji). One [`Fonts`] serves both the UI
//! layout and the GPU renderer, so what is measured is what is drawn.

use fontdue::{Font, FontSettings, Metrics};

use crate::FontFace;

/// Which loaded font a glyph comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FontId {
    /// Ubuntu Light.
    Proportional,
    /// Hack.
    Monospace,
    /// Noto Emoji (monochrome), the fallback for symbols.
    Emoji,
}

/// A glyph placed on a line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    /// Font the glyph comes from.
    pub font: FontId,
    /// Glyph index in that font.
    pub glyph: u16,
    /// Pen position of the glyph's origin, from the line start.
    pub x: f32,
    /// Horizontal advance.
    pub advance: f32,
    /// Byte index of the character in the source text.
    pub byte_index: usize,
}

/// The bundled fonts.
pub struct Fonts {
    proportional: Font,
    monospace: Font,
    emoji: Font,
    proportional_face: FontId,
}

impl std::fmt::Debug for Fonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fonts")
            .field("ui_face", &self.proportional_face)
            .finish_non_exhaustive()
    }
}

impl Fonts {
    /// Loads the bundled fonts.
    ///
    /// # Errors
    /// If a bundled font fails to parse (a broken build).
    pub fn new() -> Result<Self, String> {
        let load =
            |bytes: &[u8]| Font::from_bytes(bytes, FontSettings::default()).map_err(str::to_owned);
        Ok(Self {
            proportional: load(epaint_default_fonts::UBUNTU_LIGHT)?,
            monospace: load(epaint_default_fonts::HACK_REGULAR)?,
            emoji: load(epaint_default_fonts::NOTO_EMOJI_REGULAR)?,
            proportional_face: FontId::Proportional,
        })
    }

    /// Chooses the font used for [`FontFace::Ui`] from the rice name
    /// (`typography.ui_font`): `"Hack"` selects the monospaced font.
    pub fn set_ui_font(&mut self, name: &str) {
        self.proportional_face = if name.eq_ignore_ascii_case("hack") {
            FontId::Monospace
        } else {
            FontId::Proportional
        };
    }

    fn primary(&self, face: FontFace) -> FontId {
        match face {
            FontFace::Ui => self.proportional_face,
            FontFace::Mono => FontId::Monospace,
        }
    }

    /// The loaded font for `id`.
    #[must_use]
    pub fn font(&self, id: FontId) -> &Font {
        match id {
            FontId::Proportional => &self.proportional,
            FontId::Monospace => &self.monospace,
            FontId::Emoji => &self.emoji,
        }
    }

    /// Distance from the line top to the baseline.
    #[must_use]
    pub fn ascent(&self, face: FontFace, size: f32) -> f32 {
        self.font(self.primary(face))
            .horizontal_line_metrics(size)
            .map_or(size * 0.8, |metrics| metrics.ascent)
    }

    /// Height of one line.
    #[must_use]
    pub fn line_height(&self, face: FontFace, size: f32) -> f32 {
        self.font(self.primary(face))
            .horizontal_line_metrics(size)
            .map_or(size * 1.2, |metrics| metrics.new_line_size)
    }

    /// Places every character of one line, falling back to the emoji font for
    /// symbols the primary font lacks.
    #[must_use]
    pub fn layout(&self, face: FontFace, size: f32, text: &str) -> Vec<PlacedGlyph> {
        let primary = self.primary(face);
        let mut glyphs = Vec::with_capacity(text.len());
        let mut pen = 0.0;
        let mut previous: Option<(FontId, u16)> = None;
        for (byte_index, ch) in text.char_indices() {
            let mut font_id = primary;
            let mut glyph = self.font(primary).lookup_glyph_index(ch);
            if glyph == 0 && !ch.is_whitespace() {
                let fallback = self.emoji.lookup_glyph_index(ch);
                if fallback != 0 {
                    font_id = FontId::Emoji;
                    glyph = fallback;
                }
            }
            let font = self.font(font_id);
            if let Some((previous_font, previous_glyph)) = previous
                && previous_font == font_id
            {
                pen += font
                    .horizontal_kern_indexed(previous_glyph, glyph, size)
                    .unwrap_or(0.0);
            }
            let advance = font.metrics_indexed(glyph, size).advance_width;
            glyphs.push(PlacedGlyph {
                font: font_id,
                glyph,
                x: pen,
                advance,
                byte_index,
            });
            pen += advance;
            previous = Some((font_id, glyph));
        }
        glyphs
    }

    /// Width of one line of text.
    #[must_use]
    pub fn measure(&self, face: FontFace, size: f32, text: &str) -> f32 {
        self.layout(face, size, text)
            .last()
            .map_or(0.0, |glyph| glyph.x + glyph.advance)
    }

    /// X offset of the caret placed before byte `index` (or at the end).
    #[must_use]
    pub fn caret_x(&self, face: FontFace, size: f32, text: &str, index: usize) -> f32 {
        let glyphs = self.layout(face, size, text);
        glyphs
            .iter()
            .find(|glyph| glyph.byte_index >= index)
            .map_or_else(
                || glyphs.last().map_or(0.0, |glyph| glyph.x + glyph.advance),
                |glyph| glyph.x,
            )
    }

    /// Byte index of the caret position nearest to `x`.
    #[must_use]
    pub fn hit_index(&self, face: FontFace, size: f32, text: &str, x: f32) -> usize {
        for glyph in self.layout(face, size, text) {
            if x < glyph.x + glyph.advance / 2.0 {
                return glyph.byte_index;
            }
        }
        text.len()
    }

    /// The longest prefix of `text` that fits in `width`, cut at a character
    /// boundary, with an ellipsis if anything was dropped.
    #[must_use]
    pub fn elide(&self, face: FontFace, size: f32, text: &str, width: f32) -> String {
        if self.measure(face, size, text) <= width {
            return text.to_owned();
        }
        let budget = width - self.measure(face, size, "…");
        let cut = self
            .layout(face, size, text)
            .into_iter()
            .find(|glyph| glyph.x + glyph.advance > budget)
            .map_or(text.len(), |glyph| glyph.byte_index);
        format!("{}…", &text[..cut])
    }

    /// Splits `text` into lines no wider than `width`, breaking at spaces when
    /// possible and honouring explicit newlines. Returns byte ranges.
    #[must_use]
    pub fn wrap(
        &self,
        face: FontFace,
        size: f32,
        text: &str,
        width: f32,
    ) -> Vec<std::ops::Range<usize>> {
        let mut lines = Vec::new();
        let mut line_start = 0;
        for paragraph in text.split_inclusive('\n') {
            let content = paragraph.trim_end_matches('\n');
            let base = line_start;
            let glyphs = self.layout(face, size, content);
            let mut start = 0;
            let mut start_x = 0.0;
            let mut last_space: Option<usize> = None;
            for glyph in &glyphs {
                let ch = content[glyph.byte_index..].chars().next().unwrap_or(' ');
                if glyph.x + glyph.advance - start_x > width && glyph.byte_index > start {
                    let cut = last_space
                        .filter(|&space| space > start)
                        .unwrap_or(glyph.byte_index);
                    lines.push(base + start..base + cut);
                    start = if last_space == Some(cut) {
                        cut + 1
                    } else {
                        cut
                    };
                    start_x = glyphs
                        .iter()
                        .find(|g| g.byte_index >= start)
                        .map_or(glyph.x, |g| g.x);
                    last_space = None;
                }
                if ch == ' ' {
                    last_space = Some(glyph.byte_index);
                }
            }
            lines.push(base + start..base + content.len());
            line_start += paragraph.len();
        }
        if text.ends_with('\n') || text.is_empty() {
            lines.push(text.len()..text.len());
        }
        lines
    }

    /// Rasterizes one glyph at `size` pixels: an 8-bit coverage bitmap.
    #[must_use]
    pub fn rasterize(&self, font: FontId, glyph: u16, size: f32) -> (Metrics, Vec<u8>) {
        self.font(font).rasterize_indexed(glyph, size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts() -> Fonts {
        Fonts::new().expect("bundled fonts load")
    }

    #[test]
    fn measures_grow_with_text_and_size() {
        let fonts = fonts();
        let short = fonts.measure(FontFace::Ui, 14.0, "Sue");
        let long = fonts.measure(FontFace::Ui, 14.0, "Sue Engine");
        assert!(short > 0.0 && long > short);
        assert!(fonts.measure(FontFace::Ui, 28.0, "Sue") > short * 1.8);
        assert!(fonts.line_height(FontFace::Ui, 14.0) > 14.0);
        assert!(fonts.ascent(FontFace::Ui, 14.0) > 0.0);
    }

    #[test]
    fn monospace_advances_are_uniform() {
        let fonts = fonts();
        let narrow = fonts.measure(FontFace::Mono, 13.0, "iiii");
        let wide = fonts.measure(FontFace::Mono, 13.0, "WWWW");
        assert!((narrow - wide).abs() < 0.01);
    }

    #[test]
    fn symbols_fall_back_to_the_emoji_font() {
        let fonts = fonts();
        let glyphs = fonts.layout(FontFace::Ui, 16.0, "a📌");
        assert_eq!(glyphs[0].font, FontId::Proportional);
        assert_eq!(glyphs[1].font, FontId::Emoji);
        assert_ne!(glyphs[1].glyph, 0);
        assert_eq!(glyphs[1].byte_index, 1);
    }

    #[test]
    fn carets_and_hits_round_trip() {
        let fonts = fonts();
        let text = "héllo";
        let x = fonts.caret_x(FontFace::Mono, 13.0, text, 3);
        assert_eq!(fonts.hit_index(FontFace::Mono, 13.0, text, x + 0.1), 3);
        assert_eq!(fonts.hit_index(FontFace::Mono, 13.0, text, 1e6), text.len());
        assert_eq!(
            fonts.caret_x(FontFace::Mono, 13.0, text, text.len()),
            fonts.measure(FontFace::Mono, 13.0, text)
        );
    }

    #[test]
    fn wrapping_breaks_at_spaces_and_newlines() {
        let fonts = fonts();
        let text = "alpha beta gamma\ndelta";
        let width = fonts.measure(FontFace::Mono, 13.0, "alpha beta ");
        let lines: Vec<&str> = fonts
            .wrap(FontFace::Mono, 13.0, text, width)
            .into_iter()
            .map(|range| &text[range])
            .collect();
        assert_eq!(lines, ["alpha beta", "gamma", "delta"]);
        let empty = fonts.wrap(FontFace::Ui, 14.0, "", 100.0);
        assert_eq!(empty.len(), 1);
        assert!(empty[0].is_empty());
    }

    #[test]
    fn eliding_keeps_a_prefix() {
        let fonts = fonts();
        let text = "Faustian Enigma";
        let width = fonts.measure(FontFace::Ui, 14.0, "Faust")
            + fonts.measure(FontFace::Ui, 14.0, "…")
            + 0.5;
        let elided = fonts.elide(FontFace::Ui, 14.0, text, width);
        assert!(elided.starts_with("Faust") && elided.ends_with('…'));
        assert_eq!(fonts.elide(FontFace::Ui, 14.0, text, 1e6), text);
    }

    #[test]
    fn glyphs_rasterize_to_coverage() {
        let fonts = fonts();
        let glyph = fonts.layout(FontFace::Ui, 24.0, "S")[0];
        let (metrics, bitmap) = fonts.rasterize(glyph.font, glyph.glyph, 24.0);
        assert_eq!(bitmap.len(), metrics.width * metrics.height);
        assert!(bitmap.iter().any(|&coverage| coverage > 200));
    }
}
