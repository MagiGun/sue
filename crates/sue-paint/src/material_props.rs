//! What Register Material says of a picture material beyond its file
//! (full guide 00:45:18, FGV-48): how it is pasted (its scaling ratio,
//! tiling, and where its layer goes in the Layer palette) and whether
//! brushes may lay it as their paper texture.
//!
//! They are kept beside the material's PNG as a small text file a person
//! can read and share with it (`<name>.suematerial`, described in
//! `docs/material-files.md`): a first line `sue-material 1`, then one
//! `key<TAB>value` line each. Unknown lines are kept as they are when the
//! file is written again.

use std::path::{Path, PathBuf};
use std::sync::{PoisonError, RwLock};

use sue_core::{PixelFormat, RasterTile2D};
use sue_model::brush::ContentHash;

use crate::vector::BrushPicture;

/// The first line of a material's settings file.
pub(crate) const HEADER: &str = "sue-material 1";
/// The extension of a material's settings file.
pub(crate) const EXTENSION: &str = "suematerial";

/// Where a registered picture is kept, and so which Material palette
/// group lists it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Keep {
    /// With the image materials (`materials/`).
    #[default]
    Images,
    /// With the brush tips (`tips/`).
    BrushTips,
}

impl Keep {
    /// Every place, in the dialog's order.
    pub(crate) const ALL: [Self; 2] = [Self::Images, Self::BrushTips];

    /// What people read.
    pub(crate) fn label(self) -> String {
        match self {
            Self::Images => tr!("material-keep-images"),
            Self::BrushTips => tr!("material-keep-brush-tips"),
        }
    }

    /// The folder of the materials location it stands for.
    pub(crate) const fn dir(self) -> &'static str {
        match self {
            Self::Images => "materials",
            Self::BrushTips => "tips",
        }
    }
}

/// Where a pasted material's layer goes in the Layer palette.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Order {
    /// Just above the layer being edited.
    #[default]
    AboveCurrent,
    /// At the top of the Layer palette.
    Top,
}

impl Order {
    /// Every place, in the dialog's order.
    pub(crate) const ALL: [Self; 2] = [Self::AboveCurrent, Self::Top];

    /// What people read.
    pub(crate) fn label(self) -> String {
        match self {
            Self::AboveCurrent => tr!("material-order-above-current"),
            Self::Top => tr!("material-order-top"),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::AboveCurrent => "above-current",
            Self::Top => "top",
        }
    }
}

/// How a picture material is pasted, and what else it is used for.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MaterialProps {
    /// Its size when pasted, against its own (Scaling ratio).
    pub scale: f32,
    /// It is repeated over the whole canvas (Tiling) instead of placed
    /// once in the middle.
    pub tile: bool,
    /// Where its layer goes.
    pub order: Order,
    /// Brushes may lay it as their paper texture.
    pub paper: bool,
    /// Lines of the file this build does not know, kept as they were.
    pub unknown: Vec<String>,
}

impl Default for MaterialProps {
    fn default() -> Self {
        Self {
            scale: 1.0,
            tile: false,
            order: Order::AboveCurrent,
            paper: false,
            unknown: Vec::new(),
        }
    }
}

impl MaterialProps {
    /// The settings file's text.
    pub(crate) fn to_text(&self) -> String {
        let mut out = vec![
            HEADER.to_owned(),
            format!("scale\t{}", self.scale),
            format!("tile\t{}", u8::from(self.tile)),
            format!("order\t{}", self.order.name()),
            format!("paper\t{}", u8::from(self.paper)),
        ];
        out.extend(self.unknown.iter().cloned());
        out.join("\n") + "\n"
    }

    /// The settings `text` holds; what it does not say is at its default.
    pub(crate) fn from_text(text: &str) -> Self {
        let mut props = Self::default();
        for line in text.lines().map(|line| line.trim_end_matches('\r')) {
            if line.is_empty() || line == HEADER {
                continue;
            }
            match line.split_once('\t') {
                Some(("scale", value)) => {
                    if let Ok(scale) = value.parse::<f32>()
                        && scale.is_finite()
                    {
                        props.scale = scale.clamp(0.01, 10.0);
                    }
                }
                Some(("tile", value)) => props.tile = value == "1",
                Some(("order", "top")) => props.order = Order::Top,
                Some(("order", _)) => props.order = Order::AboveCurrent,
                Some(("paper", value)) => props.paper = value == "1",
                _ => props.unknown.push(line.to_owned()),
            }
        }
        props
    }

    /// The settings file beside the material file `png`.
    pub(crate) fn path_for(png: &Path) -> PathBuf {
        png.with_extension(EXTENSION)
    }

    /// The settings kept beside `png`, if any.
    pub(crate) fn read(png: &Path) -> Option<Self> {
        sue_platform::fs::read(&Self::path_for(png))
            .ok()
            .map(|bytes| Self::from_text(&String::from_utf8_lossy(&bytes)))
    }

    /// Keeps the settings beside `png`.
    pub(crate) fn write(&self, png: &Path) -> Result<(), String> {
        sue_platform::write_atomic(&Self::path_for(png), self.to_text().as_bytes())
            .map_err(|error| error.to_string())
    }

    /// `tile` (straight RGBA8, the material at its own size) as the layer
    /// it pastes on a `size` canvas: scaled by its ratio, then repeated
    /// over the canvas or put in its middle (shrunk to fit if it is
    /// still larger).
    pub(crate) fn paste(&self, tile: &RasterTile2D, size: [u32; 2]) -> Option<RasterTile2D> {
        let scaled = scaled(tile, self.scale)?;
        if self.tile {
            crate::materials::tiled(&scaled, size)
        } else {
            crate::materials::placed(&scaled, size, None)
        }
    }
}

/// `tile` (straight RGBA8) scaled by `ratio`, bilinear, at least one
/// pixel each way.
pub(crate) fn scaled(tile: &RasterTile2D, ratio: f32) -> Option<RasterTile2D> {
    if (ratio - 1.0).abs() < 1e-4 {
        return Some(tile.clone());
    }
    let bytes = tile.bytes()?;
    let (w, h) = (tile.width(), tile.height());
    let (nw, nh) = (
        ((w as f32 * ratio).round() as u32).clamp(1, 16_384),
        ((h as f32 * ratio).round() as u32).clamp(1, 16_384),
    );
    let mut out = vec![0_u8; (nw * nh * 4) as usize];
    let get = |x: u32, y: u32, c: usize| f32::from(bytes[((y * w + x) * 4) as usize + c]);
    for y in 0..nh {
        for x in 0..nw {
            let sx = ((x as f32 + 0.5) / ratio - 0.5).clamp(0.0, (w - 1) as f32);
            let sy = ((y as f32 + 0.5) / ratio - 0.5).clamp(0.0, (h - 1) as f32);
            let (x0, y0) = (sx.floor() as u32, sy.floor() as u32);
            let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
            let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
            // Colors weighed by their alpha, so a clear edge does not
            // darken.
            let mut sum = [0.0_f32; 4];
            for (px, py, weight) in [
                (x0, y0, (1.0 - fx) * (1.0 - fy)),
                (x1, y0, fx * (1.0 - fy)),
                (x0, y1, (1.0 - fx) * fy),
                (x1, y1, fx * fy),
            ] {
                let a = get(px, py, 3) * weight;
                for c in 0..3 {
                    sum[c] += get(px, py, c) * a;
                }
                sum[3] += a;
            }
            let at = ((y * nw + x) * 4) as usize;
            if sum[3] > 0.0 {
                for c in 0..3 {
                    out[at + c] = (sum[c] / sum[3]).round().clamp(0.0, 255.0) as u8;
                }
            }
            out[at + 3] = sum[3].round().clamp(0.0, 255.0) as u8;
        }
    }
    RasterTile2D::from_bytes(nw, nh, PixelFormat::Rgba8, out).ok()
}

/// The papers registered from materials: each one's name and picture.
fn papers_lock() -> &'static RwLock<Vec<(String, BrushPicture)>> {
    static PAPERS: RwLock<Vec<(String, BrushPicture)>> = RwLock::new(Vec::new());
    &PAPERS
}

/// Makes the picture `pixels` (straight RGBA8, `width` by `height`) a
/// paper brushes can lay, named `name` (a paper of that name is
/// replaced). Returns its hash.
pub(crate) fn register_paper(
    name: &str,
    pixels: &[u8],
    width: u32,
    height: u32,
) -> Option<ContentHash> {
    let shape = crate::brush::TipShape::from_rgba(name, pixels, width, height)?;
    let picture = BrushPicture::of_shape(&shape)?;
    let hash = picture.hash();
    let mut papers = papers_lock()
        .write()
        .unwrap_or_else(PoisonError::into_inner);
    papers.retain(|(known, _)| known != name);
    papers.push((name.to_owned(), picture));
    Some(hash)
}

/// Every paper registered from materials: its name and hash.
pub(crate) fn papers() -> Vec<(String, ContentHash)> {
    papers_lock()
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .map(|(name, picture)| (name.clone(), picture.hash()))
        .collect()
}

/// The registered paper whose hash is `hash`.
pub(crate) fn paper(hash: &ContentHash) -> Option<BrushPicture> {
    papers_lock()
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|(_, picture)| picture.hash() == *hash)
        .map(|(_, picture)| picture.clone())
}

/// Registers as papers the materials in `folder` (a materials location)
/// whose settings say so. Returns how many.
pub(crate) fn load_papers(folder: &Path) -> usize {
    let mut found = 0;
    for keep in Keep::ALL {
        let Ok(entries) = sue_platform::list_dir(&folder.join(keep.dir()), true) else {
            continue;
        };
        for path in entries.into_iter().map(|entry| entry.path) {
            if path.extension().and_then(|e| e.to_str()) != Some(EXTENSION) {
                continue;
            }
            let png = path.with_extension("png");
            let paper = MaterialProps::read(&png).is_some_and(|props| props.paper);
            let name = png
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            if paper
                && let Some(decoded) = sue_platform::fs::read(&png)
                    .ok()
                    .and_then(|bytes| sue_io::decode_png(&bytes).ok())
                && let Some(bytes) = decoded.tile.bytes()
                && register_paper(&name, bytes, decoded.tile.width(), decoded.tile.height())
                    .is_some()
            {
                found += 1;
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_file_round_trips_and_keeps_what_it_does_not_know() {
        let props = MaterialProps {
            scale: 0.5,
            tile: true,
            order: Order::Top,
            paper: true,
            unknown: vec!["glow\t3".to_owned()],
        };
        let text = props.to_text();
        assert!(text.starts_with(HEADER));
        assert_eq!(MaterialProps::from_text(&text), props);
        assert_eq!(MaterialProps::from_text(""), MaterialProps::default());
    }

    #[test]
    fn pasting_scales_then_tiles_or_centers() {
        let mut bytes = vec![0_u8; 8 * 8 * 4];
        for p in bytes.chunks_mut(4) {
            p.copy_from_slice(&[10, 20, 30, 255]);
        }
        let tile = RasterTile2D::from_bytes(8, 8, PixelFormat::Rgba8, bytes).unwrap();
        let half = scaled(&tile, 0.5).unwrap();
        assert_eq!((half.width(), half.height()), (4, 4));
        assert_eq!(&half.bytes().unwrap()[..4], &[10, 20, 30, 255]);
        let alpha = |r: &RasterTile2D, x: u32, y: u32| {
            r.bytes().unwrap()[((y * r.width() + x) * 4 + 3) as usize]
        };
        let tiled = MaterialProps {
            scale: 0.5,
            tile: true,
            ..MaterialProps::default()
        }
        .paste(&tile, [20, 20])
        .unwrap();
        assert!(
            (0..20).all(|x| alpha(&tiled, x, 19) == 255),
            "covers the canvas"
        );
        let once = MaterialProps {
            scale: 0.5,
            ..MaterialProps::default()
        }
        .paste(&tile, [20, 20])
        .unwrap();
        assert_eq!(alpha(&once, 10, 10), 255);
        assert_eq!(alpha(&once, 2, 2), 0, "once, in the middle");
    }

    #[test]
    fn a_paper_registers_by_name_and_is_found_by_hash() {
        let mut bytes = vec![255_u8; 16 * 16 * 4];
        for (i, p) in bytes.chunks_mut(4).enumerate() {
            let v = if i % 3 == 0 { 0 } else { 255 };
            p[..3].copy_from_slice(&[v, v, v]);
        }
        let hash = register_paper("test weave", &bytes, 16, 16).expect("a paper");
        assert!(
            papers()
                .iter()
                .any(|(name, h)| name == "test weave" && *h == hash)
        );
        assert!(paper(&hash).is_some());
    }
}
