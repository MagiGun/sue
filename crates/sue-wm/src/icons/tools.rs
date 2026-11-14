//! Group E: the tools of Sue Paint, one icon for each tool of the tool
//! palette. The brush is one of the twelve reference icons and lives
//! with them.
//!
//! Each was drawn on the grid of this module from the description in
//! the icon table (`mod.rs`), and from nothing else. The tools that are
//! held in the hand (the pen, the pencil, the dropper, the wand) lie the
//! way the reference brush lies: their working end at the bottom left.

use super::canvas::FRAME;
use super::{Art, Keyline, Part};

pub(super) const TOOL_ZOOM: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M3.75 10.5 A6.75 6.75 0 0 1 17.25 10.5 A6.75 6.75 0 0 1 3.75 10.5 Z"),
        Part::Stroke("M7 11.4 A3.61 3.61 0 0 1 11.4 7"),
        Part::Fill(
            "M15.47 17.73 L18.12 20.38 A1.6 1.6 0 0 0 20.38 18.12 L17.73 15.47 A1.6 1.6 0 0 0 \
             15.47 17.73 Z",
        ),
    ],
    solid: &[],
};

pub(super) const TOOL_HAND: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(
            "M6.75 11.03 V6.75 A1.5 1.5 0 0 1 9.75 6.75 V5.5 A1.75 1.75 0 0 1 13.25 5.5 V6.25 A1.5 \
             1.5 0 0 1 16.25 6.25 V8.75 A1.5 1.5 0 0 1 19.25 8.75 V14 C19.25 17.75 16.75 20.25 \
             13.5 20.25 C11.5 20.25 10 19.6 8.9 18.2 L5.2 13.4 A1.5 1.5 0 0 1 6.75 11.03 Z",
        ),
        Part::Stroke("M9.75 6.75 V11.5 M13.25 6.25 V11.5 M16.25 8.75 V11.5"),
    ],
    solid: &[],
};

pub(super) const TOOL_ROTATE_VIEW: Art = Art {
    keyline: Keyline::Circle,
    line: &[
        Part::Stroke(
            "M9.8 8.54 A1.5 1.5 0 0 1 11.72 7.64 L15.01 8.84 A1.5 1.5 0 0 1 15.91 10.76 L14.2 \
             15.46 A1.5 1.5 0 0 1 12.28 16.36 L8.99 15.16 A1.5 1.5 0 0 1 8.09 13.24 Z",
        ),
        Part::Stroke("M8.29 5.02 A7.9 7.9 0 0 1 17.87 6.71 M15.71 18.98 A7.9 7.9 0 0 1 6.13 17.29"),
        Part::Fill("M19.28 5.44 L19.47 9.43 L16.46 7.99 Z M4.72 18.56 L4.53 14.57 L7.54 16.01 Z"),
    ],
    solid: &[],
};

/// The arrow pointer of the object tool.
const POINTER: &str = "M10.5 10.5 L20.25 14.2 L15.9 15.9 L14.2 20.25 Z";

pub(super) const TOOL_OPERATION: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(
            "M5.75 3.75 H14.25 A2 2 0 0 1 16.25 5.75 V12.25 A2 2 0 0 1 14.25 14.25 H5.75 A2 2 0 0 \
             1 3.75 12.25 V5.75 A2 2 0 0 1 5.75 3.75 Z",
        ),
        Part::Clear(POINTER),
        Part::Fill(POINTER),
        Part::Stroke(POINTER),
    ],
    solid: &[],
};

pub(super) const TOOL_MOVE_LAYER: Art = Art {
    keyline: Keyline::Circle,
    line: &[
        Part::Stroke("M12 2.75 V21.25 M2.75 12 H21.25"),
        Part::Stroke(
            "M9.25 5.5 L12 2.75 L14.75 5.5 M9.25 18.5 L12 21.25 L14.75 18.5 M5.5 9.25 L2.75 12 \
             L5.5 14.75 M18.5 9.25 L21.25 12 L18.5 14.75",
        ),
    ],
    solid: &[],
};

pub(super) const TOOL_SELECT: Art = Art {
    keyline: Keyline::Square,
    line: &[Part::Stroke(FRAME)],
    solid: &[],
};

pub(super) const TOOL_SELECT_LASSO: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke("M2.75 9.75 A9.25 5 0 0 1 21.25 9.75 A9.25 5 0 0 1 2.75 9.75 Z"),
        Part::Stroke("M6.69 13.85 C5.6 16.1 6.75 18.75 9.75 19.25"),
        Part::Fill("M5.09 13.85 A1.6 1.6 0 0 1 8.29 13.85 A1.6 1.6 0 0 1 5.09 13.85 Z"),
    ],
    solid: &[],
};

pub(super) const TOOL_AUTO_SELECT: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(
            "M5.88 19.88 L13.13 12.63 A1.25 1.25 0 0 0 11.37 10.87 L4.12 18.12 A1.25 1.25 0 0 0 \
             5.88 19.88 Z",
        ),
        Part::Stroke("M11.19 14.57 L9.43 12.81"),
        Part::Fill(
            "M15.75 3.5 Q16.65 7.35 20.5 8.25 Q16.65 9.15 15.75 13 Q14.85 9.15 11 8.25 Q14.85 7.35 \
             15.75 3.5 Z",
        ),
    ],
    solid: &[],
};

/// The rubber bulb of the dropper.
const BULB: &str = "M11.53 8.58 L14.82 5.29 A2.75 2.75 0 0 1 18.71 9.18 L15.42 12.47 Z";

pub(super) const TOOL_EYEDROPPER: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M12.24 9.29 L5.69 15.83 L4.28 19.72 L8.17 18.31 L14.71 11.76"),
        Part::Stroke("M10.64 7.7 L16.3 13.36"),
        Part::Fill(BULB),
        Part::Stroke(BULB),
    ],
    solid: &[],
};

pub(super) const TOOL_PEN: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(
            "M11 9.82 C9.94 10.88 7.99 10 6.76 11.23 C4.63 13.36 4.46 17.42 4.28 19.72 C6.58 19.54 \
             10.64 19.37 12.77 17.24 C14 16.01 13.12 14.06 14.18 13",
        ),
        Part::Stroke("M11 9.82 L16.94 3.88 L20.12 7.06 L14.18 13 Z"),
        Part::Stroke("M4.28 19.72 L7.82 16.18"),
        Part::Fill("M8.38 14.42 A1.2 1.2 0 0 1 10.78 14.42 A1.2 1.2 0 0 1 8.38 14.42 Z"),
    ],
    solid: &[],
};

pub(super) const TOOL_PENCIL: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M4.28 19.72 L6.3 14.31 L16.76 3.85 L20.15 7.24 L9.69 17.7 Z"),
        Part::Stroke("M6.3 14.31 L9.69 17.7 M14.25 6.36 L17.64 9.75"),
    ],
    solid: &[],
};

pub(super) const TOOL_AIRBRUSH: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(
            "M13.75 10.75 H17.25 A2 2 0 0 1 19.25 12.75 V18.25 A2 2 0 0 1 17.25 20.25 H13.75 A2 \
             2 0 0 1 11.75 18.25 V12.75 A2 2 0 0 1 13.75 10.75 Z",
        ),
        Part::Stroke("M14.25 10.75 V6.75 H17.25 V10.75 M14.25 7.75 H12.25"),
        Part::Fill(
            "M8 6.25 A1 1 0 0 1 10 6.25 A1 1 0 0 1 8 6.25 Z M8 9.25 A1 1 0 0 1 10 9.25 A1 1 0 0 1 \
             8 9.25 Z M4.25 4.25 A1 1 0 0 1 6.25 4.25 A1 1 0 0 1 4.25 4.25 Z M4.25 7.75 A1 1 0 0 \
             1 6.25 7.75 A1 1 0 0 1 4.25 7.75 Z M4.25 11.25 A1 1 0 0 1 6.25 11.25 A1 1 0 0 1 4.25 \
             11.25 Z",
        ),
    ],
    solid: &[],
};

pub(super) const TOOL_DECORATION: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(
            "M10.25 7.25 Q11.55 12.45 16.75 13.75 Q11.55 15.05 10.25 20.25 Q8.95 15.05 3.75 13.75 \
             Q8.95 12.45 10.25 7.25 Z",
        ),
        Part::Fill(
            "M17.25 3.25 Q17.95 6.05 20.75 6.75 Q17.95 7.45 17.25 10.25 Q16.55 7.45 13.75 6.75 \
             Q16.55 6.05 17.25 3.25 Z M18 15.75 Q18.45 17.55 20.25 18 Q18.45 18.45 18 20.25 Q17.55 \
             18.45 15.75 18 Q17.55 17.55 18 15.75 Z",
        ),
    ],
    solid: &[],
};

pub(super) const TOOL_ERASER: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(
            "M5.21 15.11 A2 2 0 0 1 5.21 12.28 L12.28 5.21 A2 2 0 0 1 15.11 5.21 L17.59 7.69 A2 2 \
             0 0 1 17.59 10.52 L10.52 17.59 A2 2 0 0 1 7.69 17.59 Z",
        ),
        Part::Stroke("M7.69 9.81 L12.99 15.11"),
        Part::Stroke("M12.75 20.25 H20.25"),
    ],
    solid: &[],
};

pub(super) const TOOL_BLEND: Art = Art {
    keyline: Keyline::Wide,
    line: &[
        Part::Stroke("M2.75 12 A5.75 5.75 0 0 1 14.25 12 A5.75 5.75 0 0 1 2.75 12 Z"),
        Part::Stroke("M9.75 12 A5.75 5.75 0 0 1 21.25 12 A5.75 5.75 0 0 1 9.75 12 Z"),
        Part::Fill("M12 7.44 A5.75 5.75 0 0 1 12 16.56 A5.75 5.75 0 0 1 12 7.44 Z"),
    ],
    solid: &[],
};

pub(super) const TOOL_LIQUIFY: Art = Art {
    keyline: Keyline::Circle,
    line: &[Part::Stroke(
        "M3 12.9 A9 9 0 0 1 21 12.9 A7.2 7.2 0 0 1 6.6 12.9 A5.4 5.4 0 0 1 17.4 12.9 A3.6 3.6 0 0 \
         1 10.2 12.9 A1.8 1.8 0 0 1 13.8 12.9",
    )],
    solid: &[],
};

pub(super) const TOOL_FILL: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(
            "M9.51 6.74 A1.75 1.75 0 0 1 11.99 6.74 L16.51 11.26 A1.75 1.75 0 0 1 16.51 13.74 \
             L11.99 18.26 A1.75 1.75 0 0 1 9.51 18.26 L4.99 13.74 A1.75 1.75 0 0 1 4.99 11.26 Z",
        ),
        Part::Stroke("M4.5 13.25 H17"),
        Part::Stroke("M5.6 10.6 A3.6 3.6 0 0 1 10.7 5.5"),
        Part::Fill(
            "M19.25 14.25 C20 15.5 21 16.5 21 17.75 A1.75 1.75 0 0 1 17.5 17.75 C17.5 16.5 18.5 \
             15.5 19.25 14.25 Z",
        ),
    ],
    solid: &[],
};

pub(super) const TOOL_GRADIENT: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Fill(
            "M3.75 5.75 A2 2 0 0 1 5.75 3.75 H7 V20.25 H5.75 A2 2 0 0 1 3.75 18.25 Z M9 3.75 H10.5 \
             V20.25 H9 Z M12.5 3.75 H13.5 V20.25 H12.5 Z M15.5 3.75 H16 V20.25 H15.5 Z",
        ),
        Part::Stroke(
            "M5.75 3.75 H18.25 A2 2 0 0 1 20.25 5.75 V18.25 A2 2 0 0 1 18.25 20.25 H5.75 A2 2 0 0 \
             1 3.75 18.25 V5.75 A2 2 0 0 1 5.75 3.75 Z",
        ),
    ],
    solid: &[],
};

pub(super) const TOOL_FIGURE: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(
            "M7.5 13.25 H5.75 A2 2 0 0 1 3.75 11.25 V5.75 A2 2 0 0 1 5.75 3.75 H11.25 A2 2 0 0 1 \
             13.25 5.75 V7.5",
        ),
        Part::Stroke("M10.25 15.25 A5 5 0 0 1 20.25 15.25 A5 5 0 0 1 10.25 15.25 Z"),
    ],
    solid: &[],
};

pub(super) const TOOL_FRAME: Art = Art {
    keyline: Keyline::Tall,
    line: &[
        Part::Stroke(
            "M6.75 2.75 H17.25 A2 2 0 0 1 19.25 4.75 V19.25 A2 2 0 0 1 17.25 21.25 H6.75 A2 2 0 0 \
             1 4.75 19.25 V4.75 A2 2 0 0 1 6.75 2.75 Z",
        ),
        Part::Stroke("M4.75 10.75 L19.25 8.25"),
        Part::Stroke("M12 9.5 V21.25"),
    ],
    solid: &[],
};

pub(super) const TOOL_BALLOON: Art = Art {
    keyline: Keyline::Square,
    line: &[Part::Stroke(
        "M6 20.25 L6.48 15.08 A8.25 6.5 0 1 1 10.28 16.61 Z",
    )],
    solid: &[],
};

pub(super) const TOOL_RULER: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke(
            "M19.14 17.16 A1.5 1.5 0 0 0 19.14 15.04 L8.96 4.86 A1.5 1.5 0 0 0 6.84 4.86 L4.86 \
             6.84 A1.5 1.5 0 0 0 4.86 8.96 L15.04 19.14 A1.5 1.5 0 0 0 17.16 19.14 Z",
        ),
        Part::Stroke("M17.06 12.95 L15.29 14.72 M14.05 9.95 L12.28 11.72 M11.05 6.94 L9.28 8.71"),
    ],
    solid: &[],
};

pub(super) const TOOL_TEXT: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M3.75 6.75 V3.75 H20.25 V6.75"),
        Part::Stroke("M12 3.75 V20.25"),
        Part::Stroke("M8.75 20.25 H15.25"),
    ],
    solid: &[],
};

pub(super) const TOOL_CORRECT_LINE: Art = Art {
    keyline: Keyline::Square,
    line: &[
        Part::Stroke("M6.75 17.25 C9.5 9.5 14.5 14.5 17.25 6.75"),
        Part::Stroke("M3.75 17.25 H6.75 V20.25 H3.75 Z M17.25 3.75 H20.25 V6.75 H17.25 Z"),
        Part::Fill("M10.4 12 A1.6 1.6 0 0 1 13.6 12 A1.6 1.6 0 0 1 10.4 12 Z"),
    ],
    solid: &[],
};

#[cfg(test)]
mod tests {
    use egui::Color32;
    use sue_core::ReversibleTransaction;
    use sue_rice::{RiceSchema, builtin_presets};

    use super::super::raster::Tile;
    use super::super::{Icon, IconStyle, REFERENCE, weight};
    use crate::Theme;

    /// Group E, the tools, in the order of the tool palette.
    const TOOLS: [Icon; 25] = [
        Icon::ToolZoom,
        Icon::ToolHand,
        Icon::ToolRotateView,
        Icon::ToolOperation,
        Icon::ToolMoveLayer,
        Icon::ToolSelect,
        Icon::ToolSelectLasso,
        Icon::ToolAutoSelect,
        Icon::ToolEyedropper,
        Icon::ToolPen,
        Icon::ToolPencil,
        Icon::ToolBrush,
        Icon::ToolAirbrush,
        Icon::ToolDecoration,
        Icon::ToolEraser,
        Icon::ToolBlend,
        Icon::ToolLiquify,
        Icon::ToolFill,
        Icon::ToolGradient,
        Icon::ToolFigure,
        Icon::ToolFrame,
        Icon::ToolBalloon,
        Icon::ToolRuler,
        Icon::ToolText,
        Icon::ToolCorrectLine,
    ];

    /// Group F, what is done to a selection and to the canvas.
    const CANVAS: [Icon; 13] = [
        Icon::SelectAll,
        Icon::Deselect,
        Icon::SelectInvert,
        Icon::SelectExpand,
        Icon::SelectShrink,
        Icon::ClearInside,
        Icon::ClearOutside,
        Icon::FillSelection,
        Icon::Crop,
        Icon::Transform,
        Icon::SnapRuler,
        Icon::SnapGrid,
        Icon::ColorSwap,
    ];

    fn both() -> impl Iterator<Item = Icon> {
        TOOLS.into_iter().chain(CANVAS)
    }

    /// How much ink two tiles do not have in common, in whole pixels:
    /// 0 for the same picture. Icons of one family (the selections, the
    /// magnets) share their frame and differ only in what is inside it,
    /// so this counts what differs, not what is alike.
    fn difference(one: &Tile, other: &Tile) -> f32 {
        let apart: u32 = one
            .alpha
            .iter()
            .zip(&other.alpha)
            .map(|(a, b)| u32::from(a.abs_diff(*b)))
            .sum();
        apart as f32 / 255.0
    }

    /// Every pair of an icon of groups E and F and another icon of the
    /// set, with how many pixels of ink tell them apart at 16 pixels,
    /// the closest first.
    fn look_alikes() -> Vec<(f32, Icon, Icon)> {
        let tiles: Vec<(Icon, Tile)> = Icon::ALL
            .iter()
            .map(|icon| (*icon, icon.tile(IconStyle::Line, 16, weight(16.0))))
            .collect();
        let mut pairs = Vec::new();
        for icon in both() {
            let mine = icon.tile(IconStyle::Line, 16, weight(16.0));
            for (other, tile) in &tiles {
                if *other != icon {
                    pairs.push((difference(&mine, tile), icon, *other));
                }
            }
        }
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
        pairs
    }

    #[test]
    fn the_tools_and_canvas_commands_are_each_their_own_picture_at_16() {
        // Twelve pixels of a 256 pixel tile: more than a dot or a short
        // line that a glance would miss. The closest pair is the zoom
        // tool and search, two magnifiers by meaning, told apart by the
        // zoom tool's solid handle and its glint.
        for (apart, one, other) in look_alikes().into_iter().take(1) {
            assert!(
                apart >= 12.0,
                "{} and {} differ by only {apart:.1} pixels of ink at 16",
                one.name(),
                other.name()
            );
        }
    }

    /// The colors of the built-in theme preset `id`.
    fn theme(id: &str) -> Theme {
        let mut rice = RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
        let preset = builtin_presets()
            .into_iter()
            .find(|preset| preset.id == id)
            .unwrap();
        preset.transaction(&rice).apply(&mut rice).unwrap();
        Theme::from_rice(&rice)
    }

    /// A picture in the making: RGB bytes.
    struct Picture {
        width: usize,
        height: usize,
        rgb: Vec<u8>,
    }

    impl Picture {
        fn new(width: usize, height: usize, ground: Color32) -> Self {
            let rgb = (0..width * height)
                .flat_map(|_| [ground.r(), ground.g(), ground.b()])
                .collect();
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

        /// Lays `tile` down in `ink`, each of its pixels `zoom` wide.
        fn tile(&mut self, at: [usize; 2], tile: &Tile, zoom: usize, ink: Color32) {
            for (index, alpha) in tile.alpha.iter().enumerate() {
                let cover = f32::from(*alpha) / 255.0;
                let (tx, ty) = (index % tile.size, index / tile.size);
                for dy in 0..zoom {
                    for dx in 0..zoom {
                        let (x, y) = (at[0] + tx * zoom + dx, at[1] + ty * zoom + dy);
                        if x >= self.width || y >= self.height {
                            continue;
                        }
                        let pixel = (y * self.width + x) * 3;
                        for (channel, color) in [ink.r(), ink.g(), ink.b()].into_iter().enumerate()
                        {
                            let under = f32::from(self.rgb[pixel + channel]);
                            self.rgb[pixel + channel] =
                                (under + (f32::from(color) - under) * cover).round() as u8;
                        }
                    }
                }
            }
        }

        fn save(&self, file: &std::path::Path) {
            use zune_core::bit_depth::BitDepth;
            use zune_core::colorspace::ColorSpace;
            use zune_core::options::EncoderOptions;
            let options =
                EncoderOptions::new(self.width, self.height, ColorSpace::RGB, BitDepth::Eight);
            let mut out = Vec::new();
            zune_png::PngEncoder::new(&self.rgb, options)
                .encode(&mut out)
                .unwrap();
            std::fs::write(file, out).unwrap();
        }
    }

    /// One sheet of `icons` on the panel color of `theme`: each icon at
    /// 16, 24 and 48 pixels, every pixel `zoom` wide, `columns` to a row.
    fn sheet(theme: &Theme, icons: &[Icon], zoom: usize, columns: usize) -> Picture {
        let gap = 8 * zoom;
        let cell = [(16 + 24 + 48) * zoom + 4 * gap, 48 * zoom + 2 * gap];
        let rows = icons.len().div_ceil(columns);
        let mut picture = Picture::new(columns * cell[0], rows * cell[1], theme.void);
        for (index, icon) in icons.iter().enumerate() {
            let corner = [(index % columns) * cell[0], (index / columns) * cell[1]];
            picture.rect(
                [corner[0] + 2, corner[1] + 2],
                [cell[0] - 4, cell[1] - 4],
                theme.panel,
            );
            let mut x = corner[0] + gap;
            for pixels in [16_usize, 24, 48] {
                let tile = icon.tile(IconStyle::Line, pixels, weight(pixels as f32));
                let y = corner[1] + gap + (48 - pixels) * zoom / 2;
                picture.tile([x, y], &tile, zoom, theme.text);
                x += pixels * zoom + gap;
            }
        }
        picture
    }

    /// Writes the contact sheets of groups E and F into the folder the
    /// environment variable `SUE_ICON_PROOFS` names, in the dark preset
    /// Sue ships in, the gray one Sue Paint ships in, and paper: every
    /// icon at 16, 24 and 48 pixels as they are, the twelve reference
    /// icons first to judge the weight by (`*-actual.png`), and the same
    /// with every pixel three wide (`*-tools-zoom.png`,
    /// `*-canvas-zoom.png`). Also prints the pairs of icons whose ink at
    /// 16 is most alike. Run it while drawing:
    /// `cargo test -p sue-wm contact_sheets -- --ignored --nocapture`.
    #[test]
    #[ignore = "writes pictures for the eye; set SUE_ICON_PROOFS to a folder"]
    fn contact_sheets() {
        let Some(folder) = std::env::var_os("SUE_ICON_PROOFS").map(std::path::PathBuf::from) else {
            return;
        };
        std::fs::create_dir_all(&folder).unwrap();
        for id in ["void", "gray", "paper"] {
            let theme = theme(id);
            let all: Vec<Icon> = REFERENCE.into_iter().chain(both()).collect();
            sheet(&theme, &all, 1, 10).save(&folder.join(format!("{id}-actual.png")));
            sheet(&theme, &TOOLS, 3, 4).save(&folder.join(format!("{id}-tools-zoom.png")));
            sheet(&theme, &CANVAS, 3, 4).save(&folder.join(format!("{id}-canvas-zoom.png")));
        }
        let pairs = look_alikes();
        for (shared, one, other) in pairs.iter().take(16) {
            println!("{shared:.1} {} {}", one.name(), other.name());
        }
    }
}
