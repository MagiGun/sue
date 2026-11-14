//! The contact sheet: the icons side by side at the sizes Sue uses, for
//! the review every icon goes through. A snapshot scene of the bin
//! draws it with the real painter in every theme and at every scale.
//!
//! The words on the sheet are the names of icons, keylines and styles,
//! as the code spells them, and numbers: nothing here is interface text.

use egui::{Align2, Color32, CornerRadius, Painter, Rect, Stroke, StrokeKind, pos2, vec2};

use super::{Icon, IconStyle, Keyline, fit::CANVAS, paint_styled};
use crate::Theme;

/// How the sheet is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sheet {
    /// A card per icon: large over its keyline and the grid, on buttons
    /// in every state, and at 16, 20, 24, 32 and 48 points in both
    /// styles. For looking closely at a few.
    Cards,
    /// A small cell per icon: 16, 24 and 48 points, the solid twin at
    /// 24, and the name. For looking over many.
    Grid,
}

/// The sizes of a card's rows, in points.
const SIZES: [f32; 5] = [16.0, 20.0, 24.0, 32.0, 48.0];

impl Sheet {
    /// The size of one icon's place, in points.
    const fn cell(self) -> [f32; 2] {
        match self {
            Self::Cards => [360.0, 300.0],
            Self::Grid => [160.0, 76.0],
        }
    }

    /// How many icons fit into `rect`, in columns and rows.
    fn layout(self, rect: Rect) -> [usize; 2] {
        let [width, height] = self.cell();
        [
            ((rect.width() / width).floor() as usize).max(1),
            ((rect.height() / height).floor() as usize).max(1),
        ]
    }

    /// How many icons one sheet in `rect` shows. More are shown on
    /// further sheets: `icons.chunks(capacity)`.
    #[must_use]
    pub fn capacity(self, rect: Rect) -> usize {
        let [columns, rows] = self.layout(rect);
        columns * rows
    }
}

/// The tint of an icon on a control that is switched off.
fn disabled(theme: &Theme) -> Color32 {
    theme.muted.gamma_multiply(0.45)
}

/// Draws the contact sheet of `icons` into `rect`, as many as fit
/// ([`Sheet::capacity`]), in the colors of `theme`.
pub fn paint(painter: &Painter, rect: Rect, theme: &Theme, sheet: Sheet, icons: &[Icon]) {
    painter.rect_filled(rect, CornerRadius::ZERO, theme.void);
    let [columns, rows] = sheet.layout(rect);
    let cell = vec2(rect.width() / columns as f32, rect.height() / rows as f32);
    for (index, icon) in icons.iter().take(columns * rows).enumerate() {
        let corner = rect.min
            + vec2(
                (index % columns) as f32 * cell.x,
                (index / columns) as f32 * cell.y,
            );
        let place = Rect::from_min_size(corner, cell);
        match sheet {
            Sheet::Cards => card(painter, place.shrink(8.0), theme, *icon),
            Sheet::Grid => small(painter, place, theme, *icon),
        }
    }
}

/// One icon of the grid: three sizes, the solid twin and the name.
fn small(painter: &Painter, place: Rect, theme: &Theme, icon: Icon) {
    let top = place.top() + 4.0;
    let mut left = place.left() + 10.0;
    for side in [16.0_f32, 24.0, 48.0] {
        let at = Rect::from_min_size(pos2(left, top + (48.0 - side) / 2.0), vec2(side, side));
        paint_styled(painter, at, icon, IconStyle::Line, theme.text);
        left += side + 10.0;
    }
    if icon.has_solid() {
        let at = Rect::from_min_size(pos2(left, top + 12.0), vec2(24.0, 24.0));
        paint_styled(painter, at, icon, IconStyle::Solid, theme.text);
    }
    painter.text(
        pos2(place.left() + 10.0, top + 52.0),
        Align2::LEFT_TOP,
        icon.name(),
        theme.font(theme.small_size),
        theme.muted,
    );
}

/// One icon in full.
fn card(painter: &Painter, place: Rect, theme: &Theme, icon: Icon) {
    let radius = CornerRadius::same(10);
    painter.rect_filled(place, radius, theme.panel);
    painter.rect_stroke(place, radius, theme.hairline(), StrokeKind::Inside);
    let pad = 16.0;
    let art = icon.art();

    // Large, over the canvas, the live area and the keyline.
    let side = 132.0_f32.min(place.height() - 2.0 * pad);
    let large = Rect::from_min_size(place.min + vec2(pad, pad), vec2(side, side));
    let unit = side / CANVAS;
    let guide = Stroke::new(1.0, theme.border);
    painter.rect_filled(large, CornerRadius::ZERO, theme.void);
    painter.rect_stroke(large, CornerRadius::ZERO, guide, StrokeKind::Inside);
    painter.rect_stroke(
        large.shrink(super::MARGIN * unit),
        CornerRadius::ZERO,
        guide,
        StrokeKind::Inside,
    );
    let key = Stroke::new(1.0, theme.border_active.gamma_multiply(0.55));
    let [width, height] = art.keyline.size();
    if art.keyline == Keyline::Circle {
        painter.circle_stroke(large.center(), width / 2.0 * unit, key);
    } else {
        painter.rect_stroke(
            Rect::from_center_size(large.center(), vec2(width * unit, height * unit)),
            CornerRadius::ZERO,
            key,
            StrokeKind::Inside,
        );
    }
    paint_styled(painter, large, icon, IconStyle::Line, theme.text);

    // Its name, its keyline and its styles.
    let column = large.right() + pad;
    painter.text(
        pos2(column, large.top()),
        Align2::LEFT_TOP,
        icon.name(),
        theme.font(theme.ui_size + 2.0),
        theme.text,
    );
    let styles = if icon.has_solid() {
        format!("{}, {}", IconStyle::Line.name(), IconStyle::Solid.name())
    } else {
        IconStyle::Line.name().to_owned()
    };
    painter.text(
        pos2(column, large.top() + 22.0),
        Align2::LEFT_TOP,
        format!("{} {width} x {height}, {styles}", art.keyline.name()),
        theme.font(theme.small_size),
        theme.muted,
    );

    // On buttons: at rest, under the pointer, chosen, switched off.
    let states = [
        (theme.pill, theme.text),
        (theme.hover, theme.text),
        (theme.active, theme.text),
        (theme.pill, disabled(theme)),
    ];
    for (row, (button, picture)) in [(32.0_f32, 24.0_f32), (24.0, 16.0)].into_iter().enumerate() {
        let top = large.top() + 50.0 + row as f32 * 42.0;
        for (index, (fill, tint)) in states.into_iter().enumerate() {
            let at = Rect::from_min_size(
                pos2(column + index as f32 * (button + 8.0), top),
                vec2(button, button),
            );
            painter.rect_filled(at, CornerRadius::same(6), fill);
            let style = if index == 2 {
                IconStyle::Solid
            } else {
                IconStyle::Line
            };
            paint_styled(
                painter,
                Rect::from_center_size(at.center(), vec2(picture, picture)),
                icon,
                style,
                tint,
            );
        }
    }

    // Every size, as a line and as its solid twin.
    for (row, style) in IconStyle::ALL.into_iter().enumerate() {
        if style == IconStyle::Solid && !icon.has_solid() {
            break;
        }
        let bottom = large.bottom() + 14.0 + 48.0 + row as f32 * 56.0;
        let mut left = place.left() + pad;
        for side in SIZES {
            let at = Rect::from_min_size(pos2(left, bottom - side), vec2(side, side));
            paint_styled(painter, at, icon, style, theme.text);
            left += side + 14.0;
        }
        painter.text(
            pos2(left + 4.0, bottom),
            Align2::LEFT_BOTTOM,
            style.name(),
            theme.font(theme.small_size),
            theme.muted,
        );
    }
}

#[cfg(test)]
mod tests {
    use egui::Context;

    use super::super::{REFERENCE, cached, headless, weight};
    use super::*;

    fn theme() -> Theme {
        Theme::from_rice(&sue_rice::RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap())
    }

    fn frame(ctx: &Context, size: [f32; 2], draw: impl Fn(&Painter, Rect)) -> headless::Seen {
        headless::frame(ctx, size, 1.0, draw)
    }

    #[test]
    fn the_reference_icons_fill_one_sheet_of_cards() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1440.0, 900.0));
        assert_eq!(Sheet::Cards.capacity(rect), 12);
        assert_eq!(REFERENCE.len(), 12);
        assert!(Sheet::Grid.capacity(rect) >= 90);
        // A sheet too small for one still shows one.
        let tiny = Rect::from_min_size(pos2(0.0, 0.0), vec2(50.0, 50.0));
        assert_eq!(Sheet::Cards.capacity(tiny), 1);
    }

    #[test]
    fn both_sheets_draw_every_icon_without_a_window() {
        let theme = theme();
        for sheet in [Sheet::Cards, Sheet::Grid] {
            let ctx = Context::default();
            let output = frame(&ctx, [1440.0, 900.0], |painter, rect| {
                paint(painter, rect, &theme, sheet, &REFERENCE);
            });
            // Every reference icon was made at more than one size.
            assert!(cached(&ctx) >= 2 * REFERENCE.len(), "{sheet:?}");
            assert!(!output.shapes.is_empty());
            let meshes = ctx.tessellate(output.shapes, output.pixels_per_point);
            assert!(!meshes.is_empty());
        }
    }

    #[test]
    fn a_sheet_shows_no_more_than_fits() {
        let theme = theme();
        let ctx = Context::default();
        let many: Vec<Icon> = Icon::ALL.iter().copied().cycle().take(500).collect();
        let _ = frame(&ctx, [360.0, 300.0], |painter, rect| {
            paint(painter, rect, &theme, Sheet::Cards, &many);
        });
        // One card: one icon, in at most its two styles at seven sizes.
        assert!(cached(&ctx) <= 14, "{}", cached(&ctx));
    }

    // What follows writes pictures for the eye, without a GPU: the tiles
    // laid on a plain ground by hand.

    /// A picture in the making: RGB bytes.
    struct Canvas {
        width: usize,
        height: usize,
        rgb: Vec<u8>,
    }

    impl Canvas {
        fn new(width: usize, height: usize, ground: Color32) -> Self {
            let mut rgb = Vec::with_capacity(width * height * 3);
            for _ in 0..width * height {
                rgb.extend([ground.r(), ground.g(), ground.b()]);
            }
            Self { width, height, rgb }
        }

        fn rect(&mut self, [left, top]: [usize; 2], [w, h]: [usize; 2], color: Color32) {
            for y in top..(top + h).min(self.height) {
                for x in left..(left + w).min(self.width) {
                    let at = (y * self.width + x) * 3;
                    self.rgb[at..at + 3].copy_from_slice(&[color.r(), color.g(), color.b()]);
                }
            }
        }

        /// Lays a tile down in `tint`, each of its pixels `zoom` wide.
        fn tile(
            &mut self,
            at: [usize; 2],
            tile: &super::super::raster::Tile,
            zoom: usize,
            tint: Color32,
        ) {
            for (index, alpha) in tile.alpha.iter().enumerate() {
                let (tx, ty) = (index % tile.size, index / tile.size);
                let cover = f32::from(*alpha) / 255.0;
                for dy in 0..zoom {
                    for dx in 0..zoom {
                        let (x, y) = (at[0] + tx * zoom + dx, at[1] + ty * zoom + dy);
                        if x >= self.width || y >= self.height {
                            continue;
                        }
                        let pixel = (y * self.width + x) * 3;
                        for (channel, ink) in [tint.r(), tint.g(), tint.b()].into_iter().enumerate()
                        {
                            let under = f32::from(self.rgb[pixel + channel]);
                            self.rgb[pixel + channel] =
                                (under + (f32::from(ink) - under) * cover).round() as u8;
                        }
                    }
                }
            }
        }

        /// The picture as a PNG file, its pixels stored as they are.
        fn png(&self) -> Vec<u8> {
            fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
                out.extend((data.len() as u32).to_be_bytes());
                let from = out.len();
                out.extend(kind);
                out.extend(data);
                let mut crc = 0xFFFF_FFFF_u32;
                for byte in &out[from..] {
                    crc ^= u32::from(*byte);
                    for _ in 0..8 {
                        crc = (crc >> 1) ^ (0xEDB8_8320 & 0_u32.wrapping_sub(crc & 1));
                    }
                }
                out.extend((!crc).to_be_bytes());
            }
            let mut raw = Vec::with_capacity(self.rgb.len() + self.height);
            for row in self.rgb.chunks(self.width * 3) {
                raw.push(0);
                raw.extend(row);
            }
            let mut packed = vec![0x78, 0x01];
            let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
            for (index, block) in blocks.iter().enumerate() {
                packed.push(u8::from(index + 1 == blocks.len()));
                packed.extend((block.len() as u16).to_le_bytes());
                packed.extend((!(block.len() as u16)).to_le_bytes());
                packed.extend(*block);
            }
            let (mut a, mut b) = (1_u32, 0_u32);
            for byte in &raw {
                a = (a + u32::from(*byte)) % 65_521;
                b = (b + a) % 65_521;
            }
            packed.extend(((b << 16) | a).to_be_bytes());
            let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
            let mut header = Vec::new();
            header.extend((self.width as u32).to_be_bytes());
            header.extend((self.height as u32).to_be_bytes());
            header.extend([8, 2, 0, 0, 0]);
            chunk(&mut out, b"IHDR", &header);
            chunk(&mut out, b"IDAT", &packed);
            chunk(&mut out, b"IEND", &[]);
            out
        }
    }

    /// Writes, into the folder the environment variable
    /// `SUE_ICON_PROOFS` names, a proof of every icon on a dark and on
    /// a light ground: the tiles of 16, 20, 24, 32 and 48 pixels as they
    /// are, and those of 16 and 24 with every pixel eight wide, which
    /// shows what the pixel fitting did. Also every icon as an SVG file.
    /// Run it while drawing:
    /// `cargo test -p sue-wm proofs -- --ignored`.
    #[test]
    #[ignore = "writes pictures for the eye; set SUE_ICON_PROOFS to a folder"]
    fn proofs() {
        let Some(folder) = std::env::var_os("SUE_ICON_PROOFS").map(std::path::PathBuf::from) else {
            return;
        };
        std::fs::create_dir_all(folder.join("svg")).unwrap();
        let grounds = [
            ("dark", Color32::from_gray(18), Color32::from_gray(235)),
            ("light", Color32::from_gray(245), Color32::from_gray(25)),
        ];
        let row = 8 * 24 + 16;
        for (name, ground, ink) in grounds {
            for (page, icons) in Icon::ALL.chunks(6).enumerate() {
                let mut canvas = Canvas::new(2 * 760, icons.len() * row, ground);
                for (index, icon) in icons.iter().enumerate() {
                    for style in IconStyle::ALL {
                        if style == IconStyle::Solid && !icon.has_solid() {
                            continue;
                        }
                        let left = if style == IconStyle::Line { 8 } else { 768 };
                        let top = index * row + 8;
                        let mut x = left;
                        for (pixels, zoom) in [(16_usize, 8_usize), (24, 8)] {
                            let tile = icon.tile(style, pixels, weight(pixels as f32));
                            canvas.rect([x, top], [pixels * zoom, pixels * zoom], ground);
                            canvas.tile([x, top], &tile, zoom, ink);
                            x += pixels * zoom + 16;
                        }
                        let mut y = top;
                        for pixels in [16_usize, 20, 24, 32, 48] {
                            let tile = icon.tile(style, pixels, weight(pixels as f32));
                            canvas.tile([x, y], &tile, 1, ink);
                            if pixels == 24 {
                                // The same at one and a half and at twice
                                // the pixels per point.
                                let tile = icon.tile(style, 36, weight(24.0) * 1.5);
                                canvas.tile([x + 64, top], &tile, 1, ink);
                                let tile = icon.tile(style, 48, weight(24.0) * 2.0);
                                canvas.tile([x + 64, top + 44], &tile, 1, ink);
                                let tile = icon.tile(style, 96, weight(48.0) * 2.0);
                                canvas.tile([x + 124, top], &tile, 1, ink);
                            }
                            y += pixels + 6;
                        }
                    }
                }
                let file = folder.join(format!("proof-{name}-{page}.png"));
                std::fs::write(file, canvas.png()).unwrap();
            }
        }
        for icon in Icon::ALL {
            for style in IconStyle::ALL {
                if style == IconStyle::Solid && !icon.has_solid() {
                    continue;
                }
                let file = folder
                    .join("svg")
                    .join(format!("{}-{}.svg", icon.name(), style.name()));
                std::fs::write(file, super::super::svg(*icon, style)).unwrap();
            }
        }
    }
}
