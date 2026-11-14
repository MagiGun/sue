//! Wet paint: brushes that mix with the paint already on the picture,
//! and the blend tools, which only move it.
//!
//! A mixing stroke cannot be collected as coverage and laid once: each
//! dab picks up what is under it, including what the stroke itself laid
//! a moment before. So it paints on a [`WetCanvas`], a copy of the
//! layer's tiles made as the stroke first touches them (from a snapshot
//! of the layer, D21), and replaces those tiles on lift.
//!
//! Each dab:
//!
//! 1. picks up the mean color under it, weighted by its coverage;
//! 2. carries it in its reservoir, which takes up the new color by the
//!    brush's pickup (`mix.amount`), less the more color stretch
//!    (`mix.pull`) it has: with a long stretch the color it picked up is
//!    dragged far along the stroke;
//! 3. mixes the reservoir with the brush's own color by the amount of
//!    paint (`mix.load`): at 0 it only moves what is there;
//! 4. softens what is under it by the blur (`mix.blur`), toward a local
//!    mean around each pixel;
//! 5. lays the result by its coverage and the density of paint.
//!
//! The arithmetic is in premultiplied color, so a dab over clear pixels
//! drags their clearness along as a finger does.

use std::collections::BTreeMap;

use sue_core::{PixelFormat, RasterTile2D};
use sue_model::brush::Mixing;

use super::math::unit;
use super::plan::Planned;
use super::stamp::{Stamp, Tips};
use crate::buffer::{AREA, MergeError, TILE, TileKey};
use crate::raster::{PixelRect, Shape};
use crate::texture::Texture;

/// Where a wet stroke reads the picture under it: straight RGBA, 0 to
/// 1, of a layer pixel. Outside the picture it is clear.
pub trait Backdrop {
    /// The pixel at layer pixel `(x, y)`.
    fn pixel(&self, x: i32, y: i32) -> [f32; 4];
}

/// A raster as a backdrop: its pixel `(0, 0)` is layer pixel `origin`.
#[derive(Clone, Copy, Debug)]
pub struct RasterBackdrop<'a> {
    /// The raster.
    pub raster: &'a RasterTile2D,
    /// The layer pixel of its `(0, 0)`.
    pub origin: [i32; 2],
}

impl Backdrop for RasterBackdrop<'_> {
    fn pixel(&self, x: i32, y: i32) -> [f32; 4] {
        let (rx, ry) = (x - self.origin[0], y - self.origin[1]);
        let (w, h) = (self.raster.width() as i32, self.raster.height() as i32);
        if rx < 0 || ry < 0 || rx >= w || ry >= h {
            return [0.0; 4];
        }
        let i = ((ry * w + rx) * 4) as usize;
        match self.raster.format() {
            PixelFormat::Rgba8 => self.raster.bytes().map_or([0.0; 4], |b| {
                [b[i], b[i + 1], b[i + 2], b[i + 3]].map(|v| f32::from(v) / 255.0)
            }),
            PixelFormat::RgbaF32 => self
                .raster
                .f32_samples()
                .map_or([0.0; 4], |s| [s[i], s[i + 1], s[i + 2], s[i + 3]]),
            _ => [0.0; 4],
        }
    }
}

/// How a wet brush mixes, all 0 to 1. Built from the brush model's
/// [`Mixing`] and the density of paint, and driven per dab by the
/// multipliers a pen may drive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wet {
    /// How much of the color under it the reservoir takes up a dab.
    pub pickup: f32,
    /// Clip Studio's amount of paint: the brush's own color in what it
    /// lays.
    pub amount: f32,
    /// Clip Studio's color stretch: how long it keeps what it picked up.
    pub stretch: f32,
    /// How much it softens what is under it.
    pub blur: f32,
    /// Clip Studio's density of paint: how fully what it lays covers.
    pub density: f32,
}

impl Wet {
    /// The wet settings of `mixing` with `density` of paint.
    #[must_use]
    pub fn new(mixing: Mixing, density: f32) -> Self {
        Self {
            pickup: unit(mixing.amount),
            amount: unit(mixing.load),
            stretch: unit(mixing.pull),
            blur: unit(mixing.blur),
            density: unit(density),
        }
    }
}

/// The blend tools and painterly blenders Sue offers, by stored name,
/// as wet settings with no paint of their own. A name is never taken
/// back. Clip Studio's tools of the same names were watched for what
/// they do; the numbers are Sue's own.
pub const BLENDERS: &[(&str, Wet)] = &[
    // Smooths colors into each other where it passes.
    (
        "blend",
        Wet {
            pickup: 0.6,
            amount: 0.0,
            stretch: 0.35,
            blur: 0.45,
            density: 0.9,
        },
    ),
    // Blurs and moves nothing.
    (
        "blur",
        Wet {
            pickup: 1.0,
            amount: 0.0,
            stretch: 0.0,
            blur: 1.0,
            density: 1.0,
        },
    ),
    // Drags the color it lands on along the stroke, as a finger does.
    (
        "finger-tip",
        Wet {
            pickup: 0.35,
            amount: 0.0,
            stretch: 0.9,
            blur: 0.0,
            density: 1.0,
        },
    ),
    // Wet paint that bleeds: picks up a lot, softens, lays thinly.
    (
        "wet-bleed",
        Wet {
            pickup: 0.8,
            amount: 0.0,
            stretch: 0.55,
            blur: 0.7,
            density: 0.55,
        },
    ),
    // A gentle wash over watercolor: softens and lays very thinly.
    (
        "soothing-watercolor",
        Wet {
            pickup: 0.9,
            amount: 0.0,
            stretch: 0.2,
            blur: 0.85,
            density: 0.3,
        },
    ),
    // Moves paint in strokes that keep their marks.
    (
        "painterly",
        Wet {
            pickup: 0.5,
            amount: 0.0,
            stretch: 0.75,
            blur: 0.0,
            density: 1.0,
        },
    ),
    // Like blend, broken up by the paper (give it a texture laid by dab).
    (
        "textured",
        Wet {
            pickup: 0.6,
            amount: 0.0,
            stretch: 0.5,
            blur: 0.25,
            density: 0.9,
        },
    ),
];

/// The blender stored as `name`.
#[must_use]
pub fn blender(name: &str) -> Option<Wet> {
    BLENDERS.iter().find(|(n, _)| *n == name).map(|b| b.1)
}

/// A tile of the canvas: premultiplied RGBA floats, and which pixels the
/// stroke changed.
#[derive(Clone, Debug)]
struct WetTile {
    pixels: Box<[f32]>,
    touched: Box<[bool]>,
}

/// The picture under a wet stroke, as the stroke has left it so far.
#[derive(Clone, Debug)]
pub struct WetCanvas {
    wet: Wet,
    color: [f32; 3],
    tiles: BTreeMap<TileKey, WetTile>,
    /// The reservoir: premultiplied RGBA the brush carries; `None`
    /// before the first dab.
    reservoir: Option<[f32; 4]>,
}

impl WetCanvas {
    /// A canvas for a wet stroke of `wet` whose own paint is `color`.
    #[must_use]
    pub fn new(wet: Wet, color: [f32; 3]) -> Self {
        Self {
            wet,
            color: color.map(unit),
            tiles: BTreeMap::new(),
            reservoir: None,
        }
    }

    /// The premultiplied pixel at layer pixel `(x, y)` as the stroke has
    /// left it, loading its tile from `backdrop` first.
    fn load(&mut self, backdrop: &dyn Backdrop, key: TileKey) -> &mut WetTile {
        self.tiles.entry(key).or_insert_with(|| {
            let [ox, oy] = key.origin();
            let mut pixels = vec![0.0; AREA * 4].into_boxed_slice();
            for y in 0..TILE {
                for x in 0..TILE {
                    let [r, g, b, a] = backdrop.pixel(ox + x, oy + y);
                    let i = ((y * TILE + x) * 4) as usize;
                    pixels[i..i + 4].copy_from_slice(&[r * a, g * a, b * a, a]);
                }
            }
            WetTile {
                pixels,
                touched: vec![false; AREA].into_boxed_slice(),
            }
        })
    }

    /// The premultiplied pixel `(x, y)`.
    fn read(&mut self, backdrop: &dyn Backdrop, x: i32, y: i32) -> [f32; 4] {
        let key = TileKey::of_pixel(x, y);
        let [ox, oy] = key.origin();
        let tile = self.load(backdrop, key);
        let i = (((y - oy) * TILE + (x - ox)) * 4) as usize;
        [
            tile.pixels[i],
            tile.pixels[i + 1],
            tile.pixels[i + 2],
            tile.pixels[i + 3],
        ]
    }

    /// The straight color of layer pixel `(x, y)` as the stroke has left
    /// it (what the live view shows).
    #[must_use]
    pub fn pixel(&self, backdrop: &dyn Backdrop, x: i32, y: i32) -> [f32; 4] {
        let key = TileKey::of_pixel(x, y);
        let [ox, oy] = key.origin();
        let Some(tile) = self.tiles.get(&key) else {
            return backdrop.pixel(x, y);
        };
        let i = (((y - oy) * TILE + (x - ox)) * 4) as usize;
        let a = tile.pixels[i + 3];
        if a <= 0.0 {
            return [0.0; 4];
        }
        [
            tile.pixels[i] / a,
            tile.pixels[i + 1] / a,
            tile.pixels[i + 2] / a,
            a,
        ]
    }

    /// Lays `dabs` one by one, each reading what the ones before left.
    pub fn stamp(
        &mut self,
        backdrop: &dyn Backdrop,
        dabs: &[Planned],
        tips: &Tips<'_>,
        texture: Option<&Texture<'_>>,
    ) {
        for planned in dabs {
            let stamp = Stamp::new(planned, tips, texture);
            self.dab(backdrop, &stamp, planned.disc.radius, planned.wet);
        }
    }

    /// One dab of `shape`, whose radius is `radius`; `driven` holds what
    /// a pen made of the amount of paint and the color stretch (as
    /// multipliers of the brush's own) and the density of paint here
    /// ([`Planned::wet`]).
    pub fn dab(
        &mut self,
        backdrop: &dyn Backdrop,
        shape: &impl Shape,
        radius: f32,
        driven: [f32; 3],
    ) {
        let rect = shape.bounds();
        if rect.is_empty() {
            return;
        }
        let (w, h) = ((rect.x1 - rect.x0) as usize, (rect.y1 - rect.y0) as usize);
        // The blur reaches a third of the radius around each pixel.
        let reach = if self.wet.blur > 0.0 {
            (radius * 0.33).round().clamp(1.0, 64.0) as i32
        } else {
            0
        };
        // What is under the dab (and the blur's reach around it).
        let outer = PixelRect {
            x0: rect.x0 - reach,
            y0: rect.y0 - reach,
            x1: rect.x1 + reach,
            y1: rect.y1 + reach,
        };
        let ow = (outer.x1 - outer.x0) as usize;
        let mut under = Vec::with_capacity(ow * (outer.y1 - outer.y0) as usize);
        for y in outer.y0..outer.y1 {
            for x in outer.x0..outer.x1 {
                under.push(self.read(backdrop, x, y));
            }
        }
        let mut cover = vec![0.0_f32; w * h];
        let mut picked = [0.0_f32; 4];
        let mut weight = 0.0;
        for y in 0..h {
            for x in 0..w {
                let c = unit(shape.coverage([
                    (rect.x0 + x as i32) as f32 + 0.5,
                    (rect.y0 + y as i32) as f32 + 0.5,
                ]));
                cover[y * w + x] = c;
                if c > 0.0 {
                    let u = under[(y + reach as usize) * ow + x + reach as usize];
                    for k in 0..4 {
                        picked[k] += u[k] * c;
                    }
                    weight += c;
                }
            }
        }
        if weight <= 0.0 {
            return;
        }
        let picked = picked.map(|v| v / weight);
        let wet = Wet {
            amount: unit(self.wet.amount * driven[0]),
            stretch: unit(self.wet.stretch * driven[1]),
            density: unit(self.wet.density * driven[2]),
            ..self.wet
        };
        let take = wet.pickup * (1.0 - wet.stretch);
        let reservoir = match self.reservoir {
            None => picked,
            Some(r) => std::array::from_fn(|k| r[k] + (picked[k] - r[k]) * take),
        };
        self.reservoir = Some(reservoir);
        let own = [self.color[0], self.color[1], self.color[2], 1.0];
        let paint: [f32; 4] =
            std::array::from_fn(|k| reservoir[k] + (own[k] - reservoir[k]) * wet.amount);
        let blurred = (reach > 0).then(|| {
            let mut out = under.clone();
            for _ in 0..2 {
                blur_box(&mut out, ow, reach as usize);
            }
            out
        });
        for y in 0..h {
            for x in 0..w {
                let c = cover[y * w + x];
                if c <= 0.0 {
                    continue;
                }
                let ui = (y + reach as usize) * ow + x + reach as usize;
                let u = under[ui];
                let target: [f32; 4] = match &blurred {
                    Some(b) => std::array::from_fn(|k| paint[k] + (b[ui][k] - paint[k]) * wet.blur),
                    None => paint,
                };
                let t = c * wet.density;
                let out: [f32; 4] = std::array::from_fn(|k| u[k] + (target[k] - u[k]) * t);
                let (px, py) = (rect.x0 + x as i32, rect.y0 + y as i32);
                let key = TileKey::of_pixel(px, py);
                let [ox, oy] = key.origin();
                let tile = self.load(backdrop, key);
                let i = ((py - oy) * TILE + (px - ox)) as usize;
                tile.pixels[i * 4..i * 4 + 4].copy_from_slice(&out);
                tile.touched[i] = true;
            }
        }
    }

    /// Replaces the pixels the stroke changed in `raster` (whose pixel
    /// `(0, 0)` is layer pixel `origin`) and returns the rectangle of
    /// each tile it changed, in order.
    ///
    /// # Errors
    /// [`MergeError::Format`] for a raster with no color.
    pub fn merge(
        &self,
        raster: &mut RasterTile2D,
        origin: [i32; 2],
    ) -> Result<Vec<PixelRect>, MergeError> {
        let (w, h) = (raster.width() as i32, raster.height() as i32);
        let format = raster.format();
        let mut changed = Vec::new();
        for (key, tile) in &self.tiles {
            let [ox, oy] = key.origin();
            let mut rect: Option<PixelRect> = None;
            for y in 0..TILE {
                for x in 0..TILE {
                    let i = (y * TILE + x) as usize;
                    if !tile.touched[i] {
                        continue;
                    }
                    let (rx, ry) = (ox + x - origin[0], oy + y - origin[1]);
                    if rx < 0 || ry < 0 || rx >= w || ry >= h {
                        continue;
                    }
                    let p = &tile.pixels[i * 4..i * 4 + 4];
                    let a = p[3].clamp(0.0, 1.0);
                    let straight = if a > 0.0 {
                        [p[0] / a, p[1] / a, p[2] / a, a]
                    } else {
                        [0.0; 4]
                    };
                    let j = ((ry * w + rx) * 4) as usize;
                    match format {
                        PixelFormat::Rgba8 => {
                            let bytes = raster
                                .bytes_mut()
                                .ok_or(MergeError::Format(PixelFormat::Rgba8))?;
                            for (b, v) in bytes[j..j + 4].iter_mut().zip(straight) {
                                *b = <u8 as crate::raster::kernel::Channel>::from_unit(v);
                            }
                        }
                        PixelFormat::RgbaF32 => {
                            let samples = raster
                                .f32_samples_mut()
                                .ok_or(MergeError::Format(PixelFormat::RgbaF32))?;
                            samples[j..j + 4].copy_from_slice(&straight);
                        }
                        other => return Err(MergeError::Format(other)),
                    }
                    let (lx, ly) = (ox + x, oy + y);
                    rect = Some(match rect {
                        None => PixelRect {
                            x0: lx,
                            y0: ly,
                            x1: lx + 1,
                            y1: ly + 1,
                        },
                        Some(r) => PixelRect {
                            x0: r.x0.min(lx),
                            y0: r.y0.min(ly),
                            x1: r.x1.max(lx + 1),
                            y1: r.y1.max(ly + 1),
                        },
                    });
                }
            }
            changed.extend(rect);
        }
        Ok(changed)
    }
}

/// Box blurs premultiplied pixels (`width` wide) by `radius` each way,
/// rows then columns, holding the edge pixels beyond the border.
fn blur_box(pixels: &mut [[f32; 4]], width: usize, radius: usize) {
    if width == 0 || radius == 0 {
        return;
    }
    let height = pixels.len() / width;
    let span = (2 * radius + 1) as f32;
    let mut line = Vec::new();
    let run = |line: &[[f32; 4]], out: &mut dyn FnMut(usize, [f32; 4])| {
        let n = line.len() as isize;
        let at = |i: isize| line[i.clamp(0, n - 1) as usize];
        let r = radius as isize;
        let mut sum = [0.0_f32; 4];
        for i in -r..=r {
            let p = at(i);
            for k in 0..4 {
                sum[k] += p[k];
            }
        }
        for i in 0..n {
            out(i as usize, sum.map(|s| s / span));
            let (add, sub) = (at(i + r + 1), at(i - r));
            for k in 0..4 {
                sum[k] += add[k] - sub[k];
            }
        }
    };
    for y in 0..height {
        line.clear();
        line.extend_from_slice(&pixels[y * width..(y + 1) * width]);
        run(&line, &mut |x, v| pixels[y * width + x] = v);
    }
    for x in 0..width {
        line.clear();
        line.extend((0..height).map(|y| pixels[y * width + x]));
        run(&line, &mut |y, v| pixels[y * width + x] = v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::{Disc, RoundTip, Turn};

    /// A raster whose left half is red and right half blue, opaque.
    fn halves() -> RasterTile2D {
        let mut r = RasterTile2D::new(64, 32, PixelFormat::Rgba8).expect("a raster");
        let bytes = r.bytes_mut().expect("bytes");
        for (i, px) in bytes.chunks_mut(4).enumerate() {
            let x = i % 64;
            px.copy_from_slice(if x < 32 {
                &[255, 0, 0, 255]
            } else {
                &[0, 0, 255, 255]
            });
        }
        r
    }

    fn dabs(from: f32, to: f32, radius: f32) -> Vec<Planned> {
        let mut out = Vec::new();
        let mut x = from;
        while x <= to {
            out.push(Planned {
                disc: Disc::new([x, 16.0], radius, 1.0),
                aspect: 1.0,
                turn: Turn::NONE,
                tip: None,
                flip: [false; 2],
                hardness: 0.6,
                color: [0.0; 3],
                texture_strength: 1.0,
                wet: [1.0; 3],
                index: 0,
            });
            x += 1.0;
        }
        out
    }

    fn red_at(canvas: &WetCanvas, backdrop: &dyn Backdrop, x: i32) -> f32 {
        canvas.pixel(backdrop, x, 16)[0]
    }

    #[test]
    fn a_finger_tip_drags_the_color_it_lands_on_far_along_the_stroke() {
        let picture = halves();
        let backdrop = RasterBackdrop {
            raster: &picture,
            origin: [0, 0],
        };
        let tips = Tips::round(1.0);
        let mut finger = WetCanvas::new(blender("finger-tip").expect("a blender"), [0.0; 3]);
        finger.stamp(&backdrop, &dabs(16.0, 50.0, 5.0), &tips, None);
        let mut blend = WetCanvas::new(blender("blend").expect("a blender"), [0.0; 3]);
        blend.stamp(&backdrop, &dabs(16.0, 50.0, 5.0), &tips, None);
        // Well into the blue, the finger still carries red; the blend
        // tool has let go of it sooner.
        let (f, b) = (
            red_at(&finger, &backdrop, 44),
            red_at(&blend, &backdrop, 44),
        );
        assert!(f > 0.3, "{f}");
        assert!(f > b + 0.1, "{f} against {b}");
        // Nothing outside the stroke changed.
        assert_eq!(finger.pixel(&backdrop, 5, 2), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn amount_of_paint_lays_the_brush_color_and_density_how_fully() {
        let picture = halves();
        let backdrop = RasterBackdrop {
            raster: &picture,
            origin: [0, 0],
        };
        let tips = Tips::round(1.0);
        let paint = |amount: f32, density: f32, to: f32| {
            let wet = Wet {
                pickup: 1.0,
                amount,
                stretch: 0.0,
                blur: 0.0,
                density,
            };
            let mut canvas = WetCanvas::new(wet, [0.0, 1.0, 0.0]);
            canvas.stamp(&backdrop, &dabs(8.0, to, 6.0), &tips, None);
            canvas.pixel(&backdrop, 8, 16)
        };
        let full = paint(1.0, 1.0, 20.0);
        assert!(full[1] > 0.95 && full[0] < 0.05, "{full:?}");
        // One dab of half paint lays half the brush's color and half of
        // what it picked up.
        let half = paint(0.5, 1.0, 8.0);
        assert!(
            (half[1] - 0.5).abs() < 0.05 && (half[0] - 0.5).abs() < 0.05,
            "{half:?}"
        );
        let thin = paint(1.0, 0.3, 8.0);
        assert!(thin[0] > full[0] + 0.2, "{thin:?}");
    }

    #[test]
    fn blur_softens_a_hard_edge_and_moves_nothing_else() {
        let picture = halves();
        let backdrop = RasterBackdrop {
            raster: &picture,
            origin: [0, 0],
        };
        let mut canvas = WetCanvas::new(blender("blur").expect("a blender"), [0.0; 3]);
        canvas.stamp(&backdrop, &dabs(26.0, 38.0, 8.0), &Tips::round(1.0), None);
        let left = canvas.pixel(&backdrop, 31, 16);
        let right = canvas.pixel(&backdrop, 32, 16);
        assert!(left[2] > 0.15 && right[0] > 0.15, "{left:?} {right:?}");
        let far = canvas.pixel(&backdrop, 20, 16);
        assert!(far[0] > 0.95, "{far:?}");
    }

    #[test]
    fn a_merge_replaces_only_the_pixels_the_stroke_touched() {
        let mut picture = halves();
        let snapshot = picture.clone();
        let backdrop = RasterBackdrop {
            raster: &snapshot,
            origin: [0, 0],
        };
        let mut canvas = WetCanvas::new(blender("blend").expect("a blender"), [0.0; 3]);
        canvas.stamp(&backdrop, &dabs(28.0, 36.0, 4.0), &Tips::round(1.0), None);
        let changed = canvas.merge(&mut picture, [0, 0]).expect("merged");
        assert_eq!(changed.len(), 1);
        let r = changed[0];
        assert!(r.x0 >= 23 && r.x1 <= 42, "{r:?}");
        let bytes = picture.bytes().expect("bytes");
        assert_eq!(bytes[..4], [255, 0, 0, 255]);
        let live = canvas.pixel(&backdrop, 32, 16);
        let i = (16 * 64 + 32) * 4;
        assert!((f32::from(bytes[i]) / 255.0 - live[0]).abs() < 0.01);
        let _ = RoundTip::default();
    }

    #[test]
    fn every_blender_has_a_name_and_lays_none_of_its_own_paint() {
        for (name, wet) in BLENDERS {
            assert_eq!(blender(name), Some(*wet));
            assert_eq!(wet.amount, 0.0, "{name}");
        }
        assert_eq!(blender("copy-stamp"), None);
    }
}
