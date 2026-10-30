//! Materials, as in Clip Studio Paint's Material palette: tones, patterns
//! and pictures ready to put on the canvas. Sue Paint brings screentones
//! and patterns of its own, drawn at any size; pictures registered with
//! Edit, Register Material are PNG files in Sue Paint's folder
//! (`materials/`), so every project has them.

use std::path::{Path, PathBuf};

use sue_core::{PixelFormat, RasterTile2D};

/// What a material is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A screentone: dots or lines of one density.
    Tone,
    /// A pattern that repeats.
    Pattern,
    /// A picture of its own, registered by the person.
    Image,
}

impl Kind {
    /// What people read, in the palette's order.
    pub const NAMES: [&str; 3] = ["Tones", "Patterns", "Images"];
}

/// One material in the library.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    /// Its name.
    pub name: String,
    /// What it is.
    pub kind: Kind,
    /// Where it comes from.
    pub source: Source,
}

/// Where a material comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// One of Sue Paint's own, by number.
    Builtin(usize),
    /// A registered picture.
    File(PathBuf),
}

/// Sue Paint's own tones and patterns: a name, a kind and how much ink
/// each pixel has.
type Builtin = (&'static str, Kind, fn(f32, f32) -> f32);

/// Halftone dots on a 45 degree grid of 8 pixel cells covering `share`.
fn dots(x: f32, y: f32, share: f32) -> f32 {
    let cell = 8.0;
    let (s, c) = std::f32::consts::FRAC_PI_4.sin_cos();
    let (u, v) = ((x * c + y * s) / cell, (-x * s + y * c) / cell);
    let (du, dv) = (u - u.round(), v - v.round());
    let away = du.hypot(dv) * cell;
    let radius = cell * (share / std::f32::consts::PI).sqrt();
    (radius - away + 0.5).clamp(0.0, 1.0)
}

fn noise(x: f32, y: f32) -> f32 {
    let mut h =
        (x as i64 as u32).wrapping_mul(0x9E37_79B1) ^ (y as i64 as u32).wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFF) as f32 / 255.0
}

const BUILTIN: [Builtin; 12] = [
    ("Dot 10%", Kind::Tone, |x, y| dots(x, y, 0.1)),
    ("Dot 20%", Kind::Tone, |x, y| dots(x, y, 0.2)),
    ("Dot 30%", Kind::Tone, |x, y| dots(x, y, 0.3)),
    ("Dot 50%", Kind::Tone, |x, y| dots(x, y, 0.5)),
    ("Lines 20%", Kind::Tone, |_, y| {
        if y.rem_euclid(6.0) < 1.2 { 1.0 } else { 0.0 }
    }),
    ("Sand", Kind::Tone, |x, y| {
        if noise(x, y) > 0.82 { 1.0 } else { 0.0 }
    }),
    ("Crosshatch", Kind::Pattern, |x, y| {
        let a = (x + y).rem_euclid(8.0) < 1.0;
        let b = (x - y).rem_euclid(8.0) < 1.0;
        if a || b { 1.0 } else { 0.0 }
    }),
    ("Grid", Kind::Pattern, |x, y| {
        if x.rem_euclid(16.0) < 1.0 || y.rem_euclid(16.0) < 1.0 {
            1.0
        } else {
            0.0
        }
    }),
    ("Checker", Kind::Pattern, |x, y| {
        if ((x / 16.0).floor() + (y / 16.0).floor()).rem_euclid(2.0) < 1.0 {
            1.0
        } else {
            0.0
        }
    }),
    ("Bricks", Kind::Pattern, |x, y| {
        let row = (y / 12.0).floor();
        let shift = if row.rem_euclid(2.0) < 1.0 { 0.0 } else { 12.0 };
        if y.rem_euclid(12.0) < 1.5 || (x + shift).rem_euclid(24.0) < 1.5 {
            1.0
        } else {
            0.0
        }
    }),
    ("Stripes", Kind::Pattern, |x, y| {
        if (x + y).rem_euclid(12.0) < 4.0 {
            1.0
        } else {
            0.0
        }
    }),
    ("Speckles", Kind::Pattern, |x, y| noise(x, y) * 0.6),
];

/// Every material: Sue Paint's own, then the pictures registered in
/// Sue Paint's `folder`.
#[must_use]
pub fn library(folder: Option<&Path>) -> Vec<Material> {
    let mut out: Vec<Material> = BUILTIN
        .iter()
        .enumerate()
        .map(|(index, (name, kind, _))| Material {
            name: (*name).to_owned(),
            kind: *kind,
            source: Source::Builtin(index),
        })
        .collect();
    if let Some(dir) = folder.map(|folder| folder.join("materials"))
        && let Ok(entries) = sue_platform::list_dir(&dir, true)
    {
        let mut files: Vec<PathBuf> = entries
            .into_iter()
            .map(|entry| entry.path)
            .filter(|path| {
                path.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
            })
            .collect();
        files.sort();
        out.extend(files.into_iter().map(|path| Material {
            name: path.file_stem().map_or_else(
                || tr!("material"),
                |stem| stem.to_string_lossy().into_owned(),
            ),
            kind: Kind::Image,
            source: Source::File(path),
        }));
    }
    out
}

/// `ink` (0 to 1 a pixel) as black on clear, `size`.
fn draw(size: [u32; 2], color: [u8; 3], ink: fn(f32, f32) -> f32) -> RasterTile2D {
    let mut pixels = Vec::with_capacity((size[0] * size[1] * 4) as usize);
    for y in 0..size[1] {
        for x in 0..size[0] {
            let alpha = (ink(x as f32 + 0.5, y as f32 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8;
            pixels.extend_from_slice(&[color[0], color[1], color[2], alpha]);
        }
    }
    RasterTile2D::from_bytes(size[0], size[1], PixelFormat::Rgba8, pixels)
        .unwrap_or_else(|_| RasterTile2D::new(1, 1, PixelFormat::Rgba8).expect("one pixel"))
}

/// The material as a picture: Sue Paint's own at `size` in `color`, a
/// registered one as it is.
#[must_use]
pub fn picture(material: &Material, size: [u32; 2], color: [u8; 3]) -> Option<RasterTile2D> {
    match &material.source {
        Source::Builtin(index) => BUILTIN
            .get(*index)
            .map(|(_, _, ink)| draw(size, color, *ink)),
        Source::File(path) => {
            let bytes = sue_platform::fs::read(path).ok()?;
            sue_io::decode_png(&bytes).ok().map(|decoded| decoded.tile)
        }
    }
}

/// The layer that puts `material` on a `size` canvas: tones and patterns
/// cover it (in `color`), pictures sit in the middle, shrunk to fit.
#[must_use]
pub fn layer(material: &Material, size: [u32; 2], color: [u8; 3]) -> Option<RasterTile2D> {
    if material.kind != Kind::Image {
        return picture(material, size, color);
    }
    let tile = picture(material, size, color)?;
    fitted(&tile, size)
}

/// `tile` (straight RGBA8) made to fit inside `size` keeping its shape,
/// never larger than it is, in the middle of a clear picture of `size`.
#[must_use]
pub fn fitted(tile: &RasterTile2D, size: [u32; 2]) -> Option<RasterTile2D> {
    let from = [tile.width(), tile.height()];
    let scale = (f64::from(size[0]) / f64::from(from[0]))
        .min(f64::from(size[1]) / f64::from(from[1]))
        .min(1.0);
    let to = [
        ((f64::from(from[0]) * scale).round() as u32).max(1),
        ((f64::from(from[1]) * scale).round() as u32).max(1),
    ];
    let rgba = match tile.format() {
        PixelFormat::Rgba8 => tile.bytes()?.to_vec(),
        _ => return None,
    };
    let fitted = if to == from {
        rgba
    } else {
        crate::doc::resize_rgba(&rgba, from, to)
    };
    let mut out = vec![0_u8; (size[0] * size[1] * 4) as usize];
    let (left, top) = ((size[0] - to[0]) / 2, (size[1] - to[1]) / 2);
    for y in 0..to[1] {
        let row = (y * to[0] * 4) as usize;
        let at = (((top + y) * size[0] + left) * 4) as usize;
        out[at..at + (to[0] * 4) as usize]
            .copy_from_slice(&fitted[row..row + (to[0] * 4) as usize]);
    }
    RasterTile2D::from_bytes(size[0], size[1], PixelFormat::Rgba8, out).ok()
}

/// `tile` (straight RGBA8) repeated over a clear picture of `size` from
/// its top left corner (File, Import, Pattern from Image).
#[must_use]
pub fn tiled(tile: &RasterTile2D, size: [u32; 2]) -> Option<RasterTile2D> {
    let (w, h) = (tile.width(), tile.height());
    if w == 0 || h == 0 || tile.format() != PixelFormat::Rgba8 {
        return None;
    }
    let bytes = tile.bytes()?;
    let mut out = vec![0_u8; (size[0] * size[1] * 4) as usize];
    for y in 0..size[1] {
        for x in 0..size[0] {
            let from = (((y % h) * w + x % w) * 4) as usize;
            let to = ((y * size[0] + x) * 4) as usize;
            out[to..to + 4].copy_from_slice(&bytes[from..from + 4]);
        }
    }
    RasterTile2D::from_bytes(size[0], size[1], PixelFormat::Rgba8, out).ok()
}

/// Keeps `tile` (RGBA8) as a picture material named `name` in Sue Paint's
/// `folder`, cut to what it covers; returns its file.
///
/// # Errors
/// If it is empty, or will not encode or write.
pub fn register(folder: &Path, name: &str, tile: &RasterTile2D) -> Result<PathBuf, String> {
    let bytes = tile
        .bytes()
        .ok_or(tr!("only-rgba-layers-register-as-materials"))?;
    let (w, h) = (tile.width(), tile.height());
    let (mut min, mut max) = ([u32::MAX; 2], [0_u32; 2]);
    for y in 0..h {
        for x in 0..w {
            if bytes[((y * w + x) * 4 + 3) as usize] > 0 {
                min = [min[0].min(x), min[1].min(y)];
                max = [max[0].max(x), max[1].max(y)];
            }
        }
    }
    if min[0] > max[0] {
        return Err(tr!("layer-empty-draw-something-register-first"));
    }
    let (cw, ch) = (max[0] - min[0] + 1, max[1] - min[1] + 1);
    let mut cut = Vec::with_capacity((cw * ch * 4) as usize);
    for y in min[1]..=max[1] {
        let at = ((y * w + min[0]) * 4) as usize;
        cut.extend_from_slice(&bytes[at..at + (cw * 4) as usize]);
    }
    let png = sue_io::encode_png_rgba(cw, ch, &cut)?;
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe = if safe.trim().is_empty() {
        tr!("material")
    } else {
        safe.trim().to_owned()
    };
    let dir = folder.join("materials");
    sue_platform::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(format!("{safe}.png"));
    sue_platform::write_atomic(&path, &png).map_err(|error| error.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage(tile: &RasterTile2D) -> f32 {
        let bytes = tile.bytes().unwrap();
        bytes
            .chunks(4)
            .map(|pixel| f32::from(pixel[3]) / 255.0)
            .sum::<f32>()
            / (bytes.len() / 4) as f32
    }

    #[test]
    fn tones_cover_their_share_and_pictures_register_and_place() {
        let library = library(None);
        assert!(library.iter().any(|material| material.kind == Kind::Tone));
        assert!(
            library
                .iter()
                .any(|material| material.kind == Kind::Pattern)
        );
        // A 30% dot tone covers about 30% of the canvas, in its color.
        let dot = library
            .iter()
            .find(|material| material.name == "Dot 30%")
            .unwrap();
        let tone = layer(dot, [256, 256], [10, 20, 30]).unwrap();
        assert_eq!((tone.width(), tone.height()), (256, 256));
        let share = coverage(&tone);
        assert!((share - 0.3).abs() < 0.04, "{share}");
        assert_eq!(&tone.bytes().unwrap()[..3], &[10, 20, 30]);

        // A picture registered is cut to what it covers and comes back.
        let folder = std::env::temp_dir().join(format!("sue-materials-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let mut pixels = vec![0_u8; 40 * 30 * 4];
        for y in 10..20 {
            for x in 5..25 {
                pixels[(y * 40 + x) * 4..][..4].copy_from_slice(&[200, 0, 0, 255]);
            }
        }
        let tile = RasterTile2D::from_bytes(40, 30, PixelFormat::Rgba8, pixels).unwrap();
        let empty = RasterTile2D::new(4, 4, PixelFormat::Rgba8).unwrap();
        assert!(register(&folder, "Nothing", &empty).is_err());
        register(&folder, "Red / box", &tile).unwrap();
        let mine: Vec<Material> = super::library(Some(&folder))
            .into_iter()
            .filter(|material| material.kind == Kind::Image)
            .collect();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].name, "Red _ box");
        let back = picture(&mine[0], [0, 0], [0; 3]).unwrap();
        assert_eq!((back.width(), back.height()), (20, 10));
        // Placed in the middle of a canvas.
        let placed = layer(&mine[0], [100, 50], [0; 3]).unwrap();
        let at = |x: usize, y: usize| placed.bytes().unwrap()[(y * 100 + x) * 4 + 3];
        assert_eq!(at(50, 25), 255);
        assert_eq!(at(5, 5), 0);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
