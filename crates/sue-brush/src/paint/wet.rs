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
//! 1. picks up the mean color under it, weighted by its coverage: a
//!    brush with paint of its own only where its stroke has laid none
//!    yet (the paint its leading edge meets, not its own of a dab ago);
//! 2. carries it in its reservoir, which takes up the new color by the
//!    brush's pickup (`mix.amount`), less the more color stretch
//!    (`mix.pull`) it has, while its own paint flows back in by the
//!    amount of paint (`mix.load`) for each width it travels, slower the
//!    longer the stretch: so white dragged out of black carries the
//!    black on and lightens as it goes, as on a real canvas;
//! 3. lays what it carries, with some fresh paint from the tip (all of
//!    it at full amount of paint, the eighth power of the amount
//!    below): at 0 it only moves what is there;
//! 4. softens what is under it by the blur (`mix.blur`), toward a local
//!    mean around each pixel;
//! 5. lays the result by its coverage and the density of paint.
//!
//! The arithmetic is in premultiplied color, so a dab over clear pixels
//! drags their clearness along as a finger does.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use sue_core::{JobPool, PixelFormat, RasterTile2D};
use sue_model::brush::Mixing;

use super::math::unit;
use super::overflow::Region;
use super::plan::Planned;
use super::stamp::{Stamp, Tips};
use crate::buffer::{AREA, MergeError, TILE, TileKey};
use crate::raster::{PixelRect, Shape};
use crate::texture::{Paper, Texture, TextureSettings};

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
    /// Clip Studio's amount of paint: how fast the brush's own color
    /// flows back into what it carries, and how much fresh paint it lays
    /// with it.
    pub amount: f32,
    /// Clip Studio's color stretch: how long it keeps what it picked up.
    pub stretch: f32,
    /// How much it softens what is under it.
    pub blur: f32,
    /// Clip Studio's density of paint: how fully what it lays covers.
    pub density: f32,
    /// How much it pushes the picture along with it (a finger in wet
    /// paint: edges and detail move), against laying the mean color it
    /// carries, 0 to 1. The color stretch says how nearly the pushed
    /// paint keeps up with the brush, so how far it is dragged.
    pub smear: f32,
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
            smear: 0.0,
        }
    }
}

/// A dab of a brush with paint meets new paint only at its leading
/// edge (a few tenths of it): its take up is counted that many times
/// over, so what it meets fills its hairs as a whole dab would.
const PICKUP_BOOST: f32 = 1.5;

/// How strongly a brush with paint mixes with the wet paint right under
/// it, at full pickup and no fresh paint of its own.
const LOCAL_MIX: f32 = 0.5;

/// How much of a brush's own paint flows back into what it carries for
/// each of its widths it travels, at full amount of paint and no color
/// stretch.
const REFILL: f32 = 0.7;

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
            smear: 0.0,
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
            smear: 0.0,
        },
    ),
    // Drags the picture it lands on along the stroke, as a finger does:
    // edges and detail move with it and fade over a few brush widths.
    (
        "finger-tip",
        Wet {
            pickup: 0.35,
            amount: 0.0,
            stretch: 0.75,
            blur: 0.0,
            density: 1.0,
            smear: 1.0,
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
            smear: 0.0,
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
            smear: 0.0,
        },
    ),
    // Moves paint in strokes that keep their marks.
    (
        "painterly",
        Wet {
            pickup: 0.5,
            amount: 0.0,
            stretch: 0.8,
            blur: 0.0,
            density: 0.8,
            smear: 0.35,
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
            density: 0.7,
            smear: 0.0,
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
    /// The strongest share of paint a dab has laid on each pixel (see
    /// [`WetCanvas::capped`]).
    held: Box<[f32]>,
    /// For a capped canvas, each pixel as the stroke found it and the
    /// paint the stroke's dabs have left on it (premultiplied); empty
    /// otherwise.
    base: Box<[f32]>,
    paint: Box<[f32]>,
}

impl WetTile {
    /// Pixel `i` of the tile, premultiplied.
    fn pixel(&self, i: usize) -> [f32; 4] {
        let p = &self.pixels[i * 4..i * 4 + 4];
        [p[0], p[1], p[2], p[3]]
    }
}

/// The tile rows `y` with `y mod n == k`, for band `k` of `n`: the tiles
/// one thread writes while a dab is laid on several (see
/// [`WetCanvas::sharing`]). A tile is found by its key in `index` and
/// kept in `tiles`, so a dab looks each tile up once, not each pixel.
#[derive(Clone, Debug, Default)]
struct Band {
    index: BTreeMap<TileKey, u32>,
    tiles: Vec<WetTile>,
}

impl Band {
    fn get(&self, key: TileKey) -> Option<&WetTile> {
        self.index.get(&key).map(|&at| &self.tiles[at as usize])
    }
}

/// The most threads one dab is laid on.
const MOST_BANDS: usize = 8;

/// A dab with fewer pixels than this is laid on one thread: handing it
/// out would cost more than it saves.
const WORTH_SHARING: usize = 96 * 96;

/// Marks a tile of a dab's [`Grid`] nobody has asked for yet.
const UNKNOWN: u32 = u32::MAX;

/// The tiles under one dab, each found once: where tile `key` is kept
/// in its band, for the tiles of a rectangle.
#[derive(Debug)]
struct Grid {
    x0: i32,
    y0: i32,
    width: i32,
    height: i32,
    slots: Vec<u32>,
}

impl Grid {
    /// A grid over the tiles of `rect`; with none for a rectangle so
    /// large that its tiles are better looked up one by one.
    fn over(rect: PixelRect, mut slots: Vec<u32>) -> Self {
        let (k0, k1) = (
            TileKey::of_pixel(rect.x0, rect.y0),
            TileKey::of_pixel(rect.x1 - 1, rect.y1 - 1),
        );
        let (width, height) = (
            i64::from(k1.x) - i64::from(k0.x) + 1,
            i64::from(k1.y) - i64::from(k0.y) + 1,
        );
        slots.clear();
        if width <= 0 || height <= 0 || width * height > 1 << 16 {
            return Self {
                x0: 0,
                y0: 0,
                width: 0,
                height: 0,
                slots,
            };
        }
        slots.resize((width * height) as usize, UNKNOWN);
        Self {
            x0: k0.x,
            y0: k0.y,
            width: width as i32,
            height: height as i32,
            slots,
        }
    }

    /// Where the grid keeps tile `key`, if it covers it.
    fn cell(&self, key: TileKey) -> Option<usize> {
        let (x, y) = (key.x.wrapping_sub(self.x0), key.y.wrapping_sub(self.y0));
        (x >= 0 && x < self.width && y >= 0 && y < self.height)
            .then(|| (y * self.width + x) as usize)
    }
}

/// One dab, worked out as far as every pixel shares it: what the write
/// of each pixel reads. Shared by the threads that write the dab.
#[derive(Debug)]
struct DabWork {
    /// The dab's own rectangle, and its width.
    rect: PixelRect,
    w: usize,
    /// The blur's reach around it, and the width of `under`.
    reach: usize,
    ow: usize,
    /// The picture under the dab and the reach around it, before the dab.
    under: Vec<[f32; 4]>,
    /// `under` blurred, for a brush that blurs.
    blurred: Vec<[f32; 4]>,
    blurs: bool,
    /// What a smearing brush carries to each pixel of the dab.
    carried: Vec<[f32; 4]>,
    smears: bool,
    /// The dab's coverage of each of its pixels.
    cover: Vec<f32>,
    paint: [f32; 4],
    local: f32,
    wet: Wet,
    share: f32,
    capped: bool,
}

impl Default for DabWork {
    fn default() -> Self {
        Self {
            rect: PixelRect {
                x0: 0,
                y0: 0,
                x1: 0,
                y1: 0,
            },
            w: 0,
            reach: 0,
            ow: 0,
            under: Vec::new(),
            blurred: Vec::new(),
            blurs: false,
            carried: Vec::new(),
            smears: false,
            cover: Vec::new(),
            paint: [0.0; 4],
            local: 0.0,
            wet: Wet {
                pickup: 0.0,
                amount: 0.0,
                stretch: 0.0,
                blur: 0.0,
                density: 0.0,
                smear: 0.0,
            },
            share: 0.0,
            capped: false,
        }
    }
}

impl DabWork {
    /// Writes the dab into the tiles of `band` (band `k` of `n`), and
    /// adds the tiles it changed to `dirty`.
    fn write(&self, band: &mut Band, k: i32, n: i32, dirty: &mut Vec<TileKey>) {
        let rect = self.rect;
        let (k0, k1) = (
            TileKey::of_pixel(rect.x0, rect.y0),
            TileKey::of_pixel(rect.x1 - 1, rect.y1 - 1),
        );
        let wet = self.wet;
        for ty in k0.y..=k1.y {
            if ty.rem_euclid(n) != k {
                continue;
            }
            for tx in k0.x..=k1.x {
                let key = TileKey { x: tx, y: ty };
                let Some(&at) = band.index.get(&key) else {
                    continue;
                };
                let tile = &mut band.tiles[at as usize];
                let [ox, oy] = key.origin();
                let mut any = false;
                for py in rect.y0.max(oy)..rect.y1.min(oy + TILE) {
                    let y = (py - rect.y0) as usize;
                    for px in rect.x0.max(ox)..rect.x1.min(ox + TILE) {
                        let x = (px - rect.x0) as usize;
                        let c = self.cover[y * self.w + x];
                        if c <= 0.0 {
                            continue;
                        }
                        let ui = (y + self.reach) * self.ow + x + self.reach;
                        let u = self.under[ui];
                        let paint = self.paint;
                        let mut target: [f32; 4] = if self.blurs {
                            let b = self.blurred[ui];
                            std::array::from_fn(|k| paint[k] + (b[k] - paint[k]) * wet.blur)
                        } else {
                            paint
                        };
                        // Where it lands on wet paint, the paint it lays
                        // mixes with that paint on the spot (its color,
                        // never its clearness), as a loaded brush does the
                        // moment it touches: white laid into black goes
                        // down gray at once.
                        if self.local > 0.0 && u[3] > 0.0 && target[3] > 0.0 {
                            let w = self.local * u[3].min(1.0);
                            for k in 0..3 {
                                let there = u[k] / u[3] * target[3];
                                target[k] += (there - target[k]) * w;
                            }
                        }
                        let i = ((py - oy) * TILE + (px - ox)) as usize;
                        let t = c * wet.density * self.share;
                        let mut out: [f32; 4] = if self.capped {
                            // The pixel is the paint over what the stroke
                            // found there, as strongly as its strongest
                            // dab laid it. This dab's paint takes over the
                            // paint there as strongly as it lays beside
                            // that strongest one: a dab as strong replaces
                            // it, the rim of a later dab only tints it. So
                            // paint does not stack, and the first dabs'
                            // paint (unmixed yet) does not stay as a spot
                            // where they were strongest.
                            let held = tile.held[i];
                            let now = held.max(t);
                            if now <= 0.0 {
                                continue;
                            }
                            let w = t / now;
                            let j = i * 4;
                            let mut laid = [0.0; 4];
                            for k in 0..4 {
                                let p = tile.paint[j + k] + (target[k] - tile.paint[j + k]) * w;
                                tile.paint[j + k] = p;
                                laid[k] = tile.base[j + k] + (p - tile.base[j + k]) * now;
                            }
                            tile.held[i] = now;
                            laid
                        } else {
                            std::array::from_fn(|k| u[k] + (target[k] - u[k]) * t)
                        };
                        if self.smears {
                            let d = self.carried[y * self.w + x];
                            out = std::array::from_fn(|k| out[k] + (d[k] - out[k]) * wet.smear);
                        }
                        tile.pixels[i * 4..i * 4 + 4].copy_from_slice(&out);
                        tile.touched[i] = true;
                        any = true;
                    }
                }
                if any {
                    dirty.push(key);
                }
            }
        }
    }
}

/// What dabs are stamped with, owned, so a dab's coverage can be worked
/// out on several threads: the tip's pictures, the width of a hard
/// rim's antialiasing, and the paper laid dab by dab.
#[derive(Clone, Debug)]
pub struct SharedTips {
    /// The tip's pictures.
    pub pictures: Arc<[crate::raster::TipImage]>,
    /// The width of a hard round rim's antialiasing.
    pub edge: f32,
    /// The paper laid dab by dab, if any.
    pub texture: Option<(Arc<Paper>, TextureSettings)>,
}

/// The coverage of rows `rows` of `rect` by `shape`, row by row.
fn cover_rows(shape: &impl Shape, rect: PixelRect, rows: std::ops::Range<i32>, out: &mut [f32]) {
    let w = (rect.x1 - rect.x0) as usize;
    for (row, py) in out.chunks_mut(w.max(1)).zip(rows) {
        for (c, px) in row.iter_mut().zip(rect.x0..rect.x1) {
            *c = unit(shape.coverage([px as f32 + 0.5, py as f32 + 0.5]));
        }
    }
}

/// The picture under a wet stroke, as the stroke has left it so far.
#[derive(Clone, Debug)]
pub struct WetCanvas {
    wet: Wet,
    color: [f32; 3],
    /// The tiles, by the band of their row.
    bands: Vec<Band>,
    /// The reservoir: premultiplied RGBA the brush carries; `None`
    /// before the first dab.
    reservoir: Option<[f32; 4]>,
    /// Where the last dab fell: a smearing brush pushes by the step from
    /// there.
    last_at: Option<[f32; 2]>,
    /// The tiles a dab changed since [`Self::take_dirty`] was last asked.
    dirty: BTreeSet<TileKey>,
    /// Paint of its own does not stack within the stroke (see
    /// [`Self::capped`]).
    capped: bool,
    /// The last dab's buffers, kept for the next one.
    scratch: Scratch,
}

/// Buffers a dab works in, kept from dab to dab so a stroke does not
/// ask for memory at every dab.
#[derive(Debug, Default)]
struct Scratch {
    work: DabWork,
    slots: Vec<u32>,
    line: Vec<[f32; 4]>,
}

impl Clone for Scratch {
    /// A copy starts with buffers of its own, empty.
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl WetCanvas {
    /// A canvas for a wet stroke of `wet` whose own paint is `color`.
    #[must_use]
    pub fn new(wet: Wet, color: [f32; 3]) -> Self {
        Self {
            wet,
            color: color.map(unit),
            bands: vec![Band::default()],
            reservoir: None,
            last_at: None,
            dirty: BTreeSet::new(),
            capped: false,
            scratch: Scratch::default(),
        }
    }

    /// The canvas with paint that does not stack within its stroke: a
    /// pixel takes the brush's paint as strongly as the strongest dab
    /// over it lays it, not once for every dab (as a stroke that does
    /// not mix lays its coverage, the strongest of its dabs). So the
    /// density of paint is how fully one stroke covers, and a light
    /// touch stays light however close its dabs fall; what each dab
    /// picks up and drags along still mixes. A brush that builds up, or
    /// a blend tool, has its dabs stack instead.
    #[must_use]
    pub fn capped(mut self, capped: bool) -> Self {
        self.capped = capped;
        self
    }

    /// Keeps the tiles in `bands` bands of tile rows (1 to 8), so a big
    /// dab can be written on that many threads at once
    /// ([`Self::stamp_shared`]). Every pixel of a dab is worked out from
    /// what lay there before the dab alone, so the pixels do not depend
    /// on how many bands there are.
    pub fn sharing(&mut self, bands: usize) {
        let bands = bands.clamp(1, MOST_BANDS);
        if bands == self.bands.len() {
            return;
        }
        let old = std::mem::replace(&mut self.bands, vec![Band::default(); bands]);
        for band in old {
            let mut tiles: Vec<Option<WetTile>> = band.tiles.into_iter().map(Some).collect();
            for (key, at) in band.index {
                if let Some(tile) = tiles[at as usize].take() {
                    let into = &mut self.bands[key.y.rem_euclid(bands as i32) as usize];
                    into.index.insert(key, into.tiles.len() as u32);
                    into.tiles.push(tile);
                }
            }
        }
    }

    /// The band tile `key` is kept in.
    fn band_of(&self, key: TileKey) -> usize {
        key.y.rem_euclid(self.bands.len() as i32) as usize
    }

    /// Tile `key`, if the stroke has loaded it.
    fn tile(&self, key: TileKey) -> Option<&WetTile> {
        self.bands[self.band_of(key)].get(key)
    }

    /// Every tile with its key, in the keys' order.
    fn sorted(&self) -> Vec<(TileKey, &WetTile)> {
        let mut all: Vec<(TileKey, &WetTile)> = self
            .bands
            .iter()
            .flat_map(|band| {
                band.index
                    .iter()
                    .map(|(key, &at)| (*key, &band.tiles[at as usize]))
            })
            .collect();
        all.sort_unstable_by_key(|(key, _)| *key);
        all
    }

    /// Where tile `key` is kept in its band, loading it from `backdrop`
    /// first.
    fn load(&mut self, backdrop: &dyn Backdrop, key: TileKey) -> u32 {
        let capped = self.capped;
        let at = self.band_of(key);
        let band = &mut self.bands[at];
        if let Some(&at) = band.index.get(&key) {
            return at;
        }
        let [ox, oy] = key.origin();
        let mut pixels = vec![0.0; AREA * 4].into_boxed_slice();
        for y in 0..TILE {
            for x in 0..TILE {
                let [r, g, b, a] = backdrop.pixel(ox + x, oy + y);
                let i = ((y * TILE + x) * 4) as usize;
                pixels[i..i + 4].copy_from_slice(&[r * a, g * a, b * a, a]);
            }
        }
        let at = band.tiles.len() as u32;
        band.tiles.push(WetTile {
            touched: vec![false; AREA].into_boxed_slice(),
            held: vec![0.0; AREA].into_boxed_slice(),
            base: if capped { pixels.clone() } else { Box::new([]) },
            paint: vec![0.0; if capped { AREA * 4 } else { 0 }].into_boxed_slice(),
            pixels,
        });
        band.index.insert(key, at);
        at
    }

    /// Where tile `key` is kept in its band, loading it first; found
    /// through `grid` when the grid covers it.
    fn slot(&mut self, backdrop: &dyn Backdrop, key: TileKey, grid: &mut Grid) -> u32 {
        match grid.cell(key) {
            Some(cell) if grid.slots[cell] != UNKNOWN => grid.slots[cell],
            Some(cell) => {
                let at = self.load(backdrop, key);
                grid.slots[cell] = at;
                at
            }
            None => self.load(backdrop, key),
        }
    }

    /// The premultiplied pixel `(x, y)`, its tile loaded first.
    fn read(&mut self, backdrop: &dyn Backdrop, x: i32, y: i32, grid: &mut Grid) -> [f32; 4] {
        let key = TileKey::of_pixel(x, y);
        let [ox, oy] = key.origin();
        let at = self.slot(backdrop, key, grid);
        self.bands[self.band_of(key)].tiles[at as usize]
            .pixel(((y - oy) * TILE + (x - ox)) as usize)
    }

    /// The premultiplied picture at `(x, y)` in pixel numbers (pixel
    /// `(1, 2)` is at `(1.0, 2.0)`), read between the four nearest
    /// pixels.
    fn read_between(
        &mut self,
        backdrop: &dyn Backdrop,
        x: f32,
        y: f32,
        grid: &mut Grid,
    ) -> [f32; 4] {
        let (fx, fy) = (x.floor(), y.floor());
        let (tx, ty) = (x - fx, y - fy);
        let (ix, iy) = (fx as i32, fy as i32);
        let a = self.read(backdrop, ix, iy, grid);
        let b = self.read(backdrop, ix + 1, iy, grid);
        let c = self.read(backdrop, ix, iy + 1, grid);
        let d = self.read(backdrop, ix + 1, iy + 1, grid);
        std::array::from_fn(|k| {
            let top = a[k] + (b[k] - a[k]) * tx;
            let bottom = c[k] + (d[k] - c[k]) * tx;
            top + (bottom - top) * ty
        })
    }

    /// The straight color of layer pixel `(x, y)` as the stroke has left
    /// it (what the live view shows).
    #[must_use]
    pub fn pixel(&self, backdrop: &dyn Backdrop, x: i32, y: i32) -> [f32; 4] {
        let key = TileKey::of_pixel(x, y);
        let [ox, oy] = key.origin();
        let Some(tile) = self.tile(key) else {
            return backdrop.pixel(x, y);
        };
        straight(tile.pixel(((y - oy) * TILE + (x - ox)) as usize))
    }

    /// The straight color layer pixel `(x, y)` will have once the stroke
    /// is laid: the canvas, held to the anti-overflow `region` and, with
    /// `keep_alpha` (lock transparent pixels), at the layer's own alpha.
    /// The live view and the merge both read it, so what shows while
    /// drawing is what is laid.
    #[must_use]
    pub fn laid(
        &self,
        backdrop: &dyn Backdrop,
        x: i32,
        y: i32,
        region: Option<&Region>,
        paper: Option<&Texture<'_>>,
        keep_alpha: bool,
    ) -> [f32; 4] {
        let before = backdrop.pixel(x, y);
        settle(
            before,
            self.pixel(backdrop, x, y),
            share(region, paper, x, y),
            keep_alpha,
        )
    }

    /// [`Self::laid`] for every pixel of tile `key` the stroke changed,
    /// row by row: `each` is given the pixel's place in the tile (row
    /// times [`TILE`] plus column) and its straight color. Answers
    /// `false`, having called nothing, for a tile the stroke never
    /// loaded.
    pub fn laid_tile(
        &self,
        backdrop: &dyn Backdrop,
        key: TileKey,
        region: Option<&Region>,
        paper: Option<&Texture<'_>>,
        keep_alpha: bool,
        mut each: impl FnMut(usize, [f32; 4]),
    ) -> bool {
        let Some(tile) = self.tile(key) else {
            return false;
        };
        let [ox, oy] = key.origin();
        for (i, _) in tile.touched.iter().enumerate().filter(|(_, t)| **t) {
            let (x, y) = (ox + (i as i32 % TILE), oy + (i as i32 / TILE));
            let before = backdrop.pixel(x, y);
            each(
                i,
                settle(
                    before,
                    straight(tile.pixel(i)),
                    share(region, paper, x, y),
                    keep_alpha,
                ),
            );
        }
        true
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
            self.dab(
                backdrop,
                &stamp,
                planned.disc.at,
                planned.disc.radius,
                planned.wet,
            );
        }
    }

    /// [`Self::stamp`] with a big dab's pixels worked out and written on
    /// the threads of `pool` (as many as the canvas has bands, see
    /// [`Self::sharing`]). The dabs still go down one by one, each
    /// reading what the ones before left, and every pixel is the same
    /// arithmetic in the same order: the bytes are those of
    /// [`Self::stamp`].
    ///
    /// # Errors
    /// When a job of the pool failed: the canvas cannot be trusted.
    pub fn stamp_shared(
        &mut self,
        backdrop: &dyn Backdrop,
        dabs: &[Planned],
        tips: &SharedTips,
        pool: &JobPool,
    ) -> Result<(), String> {
        let texture = tips
            .texture
            .as_ref()
            .map(|(paper, settings)| Texture::new(paper, *settings));
        let borrowed = Tips {
            pictures: &tips.pictures,
            edge: tips.edge,
        };
        let bands = self.bands.len();
        for planned in dabs {
            let stamp = Stamp::new(planned, &borrowed, texture.as_ref());
            let rect = stamp.bounds();
            if rect.is_empty() {
                continue;
            }
            let area = (rect.x1 - rect.x0) as usize * (rect.y1 - rect.y0) as usize;
            let share = (bands > 1 && pool.threads() > 1 && area >= WORTH_SHARING).then_some(pool);
            let mut failed = None;
            let cover = |out: &mut [f32]| match share {
                None => cover_rows(&stamp, rect, rect.y0..rect.y1, out),
                Some(pool) => {
                    // The dab's rows in as many runs as bands: the first
                    // here, the others on the pool.
                    let w = (rect.x1 - rect.x0) as usize;
                    let rows = (rect.y1 - rect.y0) as usize;
                    let per = rows.div_ceil(bands);
                    let jobs: Vec<_> = (1..bands)
                        .filter(|run| run * per < rows)
                        .map(|run| {
                            let (from, to) = (run * per, ((run + 1) * per).min(rows));
                            let (planned, tips) = (*planned, tips.clone());
                            let job = pool.spawn("brush dab", move |_| {
                                let texture = tips
                                    .texture
                                    .as_ref()
                                    .map(|(paper, settings)| Texture::new(paper, *settings));
                                let borrowed = Tips {
                                    pictures: &tips.pictures,
                                    edge: tips.edge,
                                };
                                let stamp = Stamp::new(&planned, &borrowed, texture.as_ref());
                                let mut out = vec![0.0_f32; (to - from) * w];
                                let rows = rect.y0 + from as i32..rect.y0 + to as i32;
                                cover_rows(&stamp, rect, rows, &mut out);
                                Ok(out)
                            });
                            (from * w, job)
                        })
                        .collect();
                    let first = per.min(rows);
                    cover_rows(
                        &stamp,
                        rect,
                        rect.y0..rect.y0 + first as i32,
                        &mut out[..first * w],
                    );
                    for (at, job) in jobs {
                        match job.wait() {
                            Ok(part) => out[at..at + part.len()].copy_from_slice(&part),
                            Err(error) => failed = Some(error.to_string()),
                        }
                    }
                }
            };
            let written = self.dab_with(
                backdrop,
                rect,
                cover,
                planned.disc.at,
                planned.disc.radius,
                planned.wet,
                share,
            );
            if let Some(error) = failed.or(written.err()) {
                return Err(error);
            }
        }
        Ok(())
    }

    /// One dab of `shape`, whose radius is `radius`; `driven` holds what
    /// a pen made of the amount of paint and the color stretch (as
    /// multipliers of the brush's own) and the density of paint here
    /// ([`Planned::wet`]).
    pub fn dab(
        &mut self,
        backdrop: &dyn Backdrop,
        shape: &impl Shape,
        at: [f32; 2],
        radius: f32,
        driven: [f32; 3],
    ) {
        let rect = shape.bounds();
        if rect.is_empty() {
            return;
        }
        let cover = |out: &mut [f32]| cover_rows(shape, rect, rect.y0..rect.y1, out);
        // On one thread nothing can fail.
        let _ = self.dab_with(backdrop, rect, cover, at, radius, driven, None);
    }

    /// One dab over `rect`, whose coverage of each pixel `cover` writes
    /// (row by row); written on `pool`'s threads when there is one.
    #[allow(clippy::too_many_arguments)]
    fn dab_with(
        &mut self,
        backdrop: &dyn Backdrop,
        rect: PixelRect,
        cover: impl FnOnce(&mut [f32]),
        at: [f32; 2],
        radius: f32,
        driven: [f32; 3],
        pool: Option<&JobPool>,
    ) -> Result<(), String> {
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
        let mut scratch = std::mem::take(&mut self.scratch);
        let mut work = std::mem::take(&mut scratch.work);
        let mut grid = Grid::over(outer, std::mem::take(&mut scratch.slots));
        work.under.clear();
        work.under
            .resize(ow * (outer.y1 - outer.y0) as usize, [0.0; 4]);
        let (k0, k1) = (
            TileKey::of_pixel(outer.x0, outer.y0),
            TileKey::of_pixel(outer.x1 - 1, outer.y1 - 1),
        );
        for ty in k0.y..=k1.y {
            for tx in k0.x..=k1.x {
                let key = TileKey { x: tx, y: ty };
                let [ox, oy] = key.origin();
                let slot = self.slot(backdrop, key, &mut grid);
                let tile = &self.bands[self.band_of(key)].tiles[slot as usize];
                let (x0, x1) = (outer.x0.max(ox), outer.x1.min(ox + TILE));
                let pixels = tile.pixels.as_chunks::<4>().0;
                for y in outer.y0.max(oy)..outer.y1.min(oy + TILE) {
                    let from = ((y - oy) * TILE + (x0 - ox)) as usize;
                    let to = (y - outer.y0) as usize * ow + (x0 - outer.x0) as usize;
                    let n = (x1 - x0) as usize;
                    work.under[to..to + n].copy_from_slice(&pixels[from..from + n]);
                }
            }
        }
        work.cover.clear();
        work.cover.resize(w * h, 0.0);
        cover(&mut work.cover);
        // A brush with paint of its own takes up the paint it meets: the
        // pixels its stroke has not laid paint on yet (its leading edge,
        // as the hairs meet the wet paint ahead), not what it laid a dab
        // ago, which is its own color again. A blend tool takes up all.
        let fresh_only = self.capped && self.wet.amount > 0.0;
        let mut picked = [0.0_f32; 4];
        let mut weight = 0.0;
        let mut met = 0.0;
        for y in 0..h {
            let py = rect.y0 + y as i32;
            let mut px = rect.x0;
            while px < rect.x1 {
                // A run of the row inside one tile.
                let key = TileKey::of_pixel(px, py);
                let [ox, oy] = key.origin();
                let end = rect.x1.min(ox + TILE);
                let held = if fresh_only {
                    self.tile(key).map(|tile| &tile.held[..])
                } else {
                    None
                };
                for qx in px..end {
                    let x = (qx - rect.x0) as usize;
                    let c = work.cover[y * w + x];
                    if c > 0.0 {
                        let u = work.under[(y + reach as usize) * ow + x + reach as usize];
                        let m = if fresh_only {
                            let held = held
                                .map_or(0.0, |held| held[((py - oy) * TILE + (qx - ox)) as usize]);
                            c * (1.0 - held)
                        } else {
                            c
                        };
                        for k in 0..4 {
                            picked[k] += u[k] * m;
                        }
                        weight += c;
                        met += m;
                    }
                }
                px = end;
            }
        }
        if weight <= 0.0 {
            scratch.work = work;
            scratch.slots = grid.slots;
            self.scratch = scratch;
            return Ok(());
        }
        // How much of the dab met paint it had not laid itself.
        let fresh = met / weight;
        let picked = if met > 0.0 {
            picked.map(|v| v / met)
        } else {
            [0.0; 4]
        };
        let wet = Wet {
            amount: unit(self.wet.amount * driven[0]),
            stretch: unit(self.wet.stretch * driven[1]),
            density: unit(self.wet.density * driven[2]),
            ..self.wet
        };
        let mut take = wet.pickup * (1.0 - wet.stretch) * (fresh * PICKUP_BOOST).min(1.0);
        let own = [self.color[0], self.color[1], self.color[2], 1.0];
        // A brush with paint of its own picks up paint, not clearness:
        // over clear pixels it finds nothing to take up and keeps what
        // it carries, so it does not thin on an empty layer. (A blend
        // tool drags clearness along, as a finger does.)
        let picked = if self.wet.amount > 0.0 {
            if picked[3] > 0.0 {
                take *= unit(picked[3]);
                picked.map(|v| v / picked[3])
            } else {
                take = 0.0;
                picked
            }
        } else {
            picked
        };
        // A brush with paint of its own lands full of it and takes up
        // what it passes from the next dab on; one with none (a blend
        // tool) lands holding what it lands on. What it carries is what
        // it lays, as a real brush lays the paint on its hairs: what it
        // picked up stays in it, and its own paint flows back in only
        // as fast as the amount of paint allows, slower the longer its
        // color stretch. So white dragged out of black lays a gray that
        // carries the black on and lightens as it goes.
        let reservoir = match self.reservoir {
            None if self.wet.amount > 0.0 => own,
            None => picked,
            Some(r) => {
                let carried: [f32; 4] = std::array::from_fn(|k| r[k] + (picked[k] - r[k]) * take);
                // Measured by the way the brush went since the last dab,
                // in its widths, so dabs closer together refill no faster.
                let travelled = self.last_at.map_or(0.0, |from| {
                    let (dx, dy) = (at[0] - from[0], at[1] - from[1]);
                    (dx * dx + dy * dy).sqrt() / (2.0 * radius).max(1.0)
                });
                let refill = (wet.amount * (1.0 - wet.stretch) * REFILL * travelled).min(1.0);
                std::array::from_fn(|k| carried[k] + (own[k] - carried[k]) * refill)
            }
        };
        self.reservoir = Some(reservoir);
        // Fresh paint from the tip goes down with it: all of it at full
        // amount of paint, little at less (the share falls as the eighth
        // power, so a brush that is not full lays mostly what it
        // carries).
        let squared = wet.amount * wet.amount;
        let fourth = squared * squared;
        let direct = fourth * fourth;
        let paint: [f32; 4] =
            std::array::from_fn(|k| reservoir[k] + (own[k] - reservoir[k]) * direct);
        // How much a brush with paint mixes with the wet paint right
        // under each pixel: by its pickup, less the fuller it is.
        let local = if self.wet.amount > 0.0 {
            wet.pickup * (1.0 - direct) * LOCAL_MIX
        } else {
            0.0
        };
        work.blurs = reach > 0;
        if work.blurs {
            work.blurred.clear();
            work.blurred.extend_from_slice(&work.under);
            for _ in 0..2 {
                blur_box(&mut work.blurred, ow, reach as usize, &mut scratch.line);
            }
        }
        // A smearing brush moves the picture along with it, as a finger
        // pushes wet paint: each pixel takes what lay behind it by the
        // dab's step, scaled by how hard the dab presses there and by the
        // color stretch. Paint ahead of the middle moves slower than the
        // brush and is overtaken; paint under the middle rides along and,
        // moving a little slower than the brush (a stretch below 1), slips
        // out behind it, so a longer stretch drags it further. Read between
        // pixels, all before any is written, so nothing is lost or doubled.
        let last_at = self.last_at.replace(at);
        // A blend tool's dabs stack, so each works by the share of the
        // brush it moved on since the last (twice that, as a dab covers
        // a pixel about half as strongly on average): the stroke as a
        // whole works the paint by its density, however close its dabs
        // fall, and does not wash every mark into one mean color.
        let share = if self.wet.amount > 0.0 {
            1.0
        } else {
            last_at.map_or(0.15, |from| {
                let (dx, dy) = (at[0] - from[0], at[1] - from[1]);
                ((dx * dx + dy * dy).sqrt() / radius.max(0.5)).clamp(0.02, 1.0)
            })
        };
        work.smears = false;
        if let Some(from) = last_at.filter(|_| wet.smear > 0.0) {
            let step = [at[0] - from[0], at[1] - from[1]];
            // The tiles a pixel may be carried from: as far as the step.
            let far = |v: f32| {
                if v.is_finite() {
                    v.abs().min(4096.0).ceil() as i32 + 2
                } else {
                    2
                }
            };
            let around = PixelRect {
                x0: rect.x0 - far(step[0]),
                y0: rect.y0 - far(step[1]),
                x1: rect.x1 + far(step[0]),
                y1: rect.y1 + far(step[1]),
            };
            let mut near = Grid::over(around, std::mem::take(&mut grid.slots));
            work.carried.clear();
            for y in 0..h {
                for x in 0..w {
                    let pull = work.cover[y * w + x] * wet.density * wet.stretch;
                    let px = (rect.x0 + x as i32) as f32 - step[0] * pull;
                    let py = (rect.y0 + y as i32) as f32 - step[1] * pull;
                    let carried = self.read_between(backdrop, px, py, &mut near);
                    work.carried.push(carried);
                }
            }
            work.smears = true;
            grid.slots = near.slots;
        }
        work.rect = rect;
        work.w = w;
        work.reach = reach as usize;
        work.ow = ow;
        work.paint = paint;
        work.local = local;
        work.wet = wet;
        work.share = share;
        work.capped = self.capped;
        let n = self.bands.len();
        let mut dirty = Vec::new();
        let mut failed = None;
        match pool.filter(|_| n > 1) {
            None => {
                for (k, band) in self.bands.iter_mut().enumerate() {
                    work.write(band, k as i32, n as i32, &mut dirty);
                }
            }
            Some(pool) => {
                // Bands 1.. are written by jobs that own them; band 0
                // here meanwhile.
                let shared = Arc::new(work);
                let jobs: Vec<_> = self.bands[1..]
                    .iter_mut()
                    .enumerate()
                    .map(|(index, band)| {
                        let mut owned = std::mem::take(band);
                        let work = Arc::clone(&shared);
                        pool.spawn("brush dab", move |_| {
                            let mut dirty = Vec::new();
                            work.write(&mut owned, index as i32 + 1, n as i32, &mut dirty);
                            drop(work);
                            Ok((owned, dirty))
                        })
                    })
                    .collect();
                shared.write(&mut self.bands[0], 0, n as i32, &mut dirty);
                for (slot, job) in self.bands[1..].iter_mut().zip(jobs) {
                    match job.wait() {
                        Ok((band, keys)) => {
                            *slot = band;
                            dirty.extend(keys);
                        }
                        Err(error) => failed = Some(error.to_string()),
                    }
                }
                // Every job has let go of it: the buffers come back.
                work = Arc::try_unwrap(shared).unwrap_or_default();
            }
        }
        self.dirty.extend(dirty);
        scratch.work = work;
        scratch.slots = grid.slots;
        self.scratch = scratch;
        failed.map_or(Ok(()), Err)
    }

    /// The tiles dabs changed since the last call, in order: what is
    /// shown again.
    pub fn take_dirty(&mut self) -> Vec<TileKey> {
        std::mem::take(&mut self.dirty).into_iter().collect()
    }

    /// `true` where a dab of the stroke changed layer pixel `(x, y)`.
    #[must_use]
    pub fn touched(&self, x: i32, y: i32) -> bool {
        let key = TileKey::of_pixel(x, y);
        let [ox, oy] = key.origin();
        self.tile(key)
            .is_some_and(|tile| tile.touched[((y - oy) * TILE + (x - ox)) as usize])
    }

    /// The tiles the stroke changed a pixel of, in order.
    pub fn keys(&self) -> impl Iterator<Item = TileKey> + '_ {
        self.sorted()
            .into_iter()
            .filter(|(_, tile)| tile.touched.iter().any(|&t| t))
            .map(|(key, _)| key)
    }

    /// Replaces the pixels the stroke changed in `raster` (whose pixel
    /// `(0, 0)` is layer pixel `origin`) as [`Self::laid`] says, and
    /// returns the rectangle of each tile it changed, in order.
    ///
    /// # Errors
    /// [`MergeError::Format`] for a raster with no color.
    pub fn merge(
        &self,
        raster: &mut RasterTile2D,
        origin: [i32; 2],
        region: Option<&Region>,
        paper: Option<&Texture<'_>>,
        keep_alpha: bool,
    ) -> Result<Vec<PixelRect>, MergeError> {
        let (w, h) = (raster.width() as i32, raster.height() as i32);
        let format = raster.format();
        match format {
            PixelFormat::Rgba8 => {
                let bytes = raster.bytes_mut().ok_or(MergeError::Format(format))?;
                Ok(self.merge_into(bytes, [w, h], origin, region, paper, keep_alpha))
            }
            PixelFormat::RgbaF32 => {
                let samples = raster.f32_samples_mut().ok_or(MergeError::Format(format))?;
                Ok(self.merge_into(samples, [w, h], origin, region, paper, keep_alpha))
            }
            other => Err(MergeError::Format(other)),
        }
    }

    /// [`Self::merge`] into the samples of a raster of `size`.
    fn merge_into<C: Stored>(
        &self,
        samples: &mut [C],
        [w, h]: [i32; 2],
        origin: [i32; 2],
        region: Option<&Region>,
        paper: Option<&Texture<'_>>,
        keep_alpha: bool,
    ) -> Vec<PixelRect> {
        let mut changed = Vec::new();
        for (key, tile) in self.sorted() {
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
                    let wet = if a > 0.0 {
                        [p[0] / a, p[1] / a, p[2] / a, a]
                    } else {
                        [0.0; 4]
                    };
                    let j = ((ry * w + rx) * 4) as usize;
                    let (lx, ly) = (ox + x, oy + y);
                    let before = C::straight(&samples[j..j + 4]);
                    let straight = settle(before, wet, share(region, paper, lx, ly), keep_alpha);
                    if straight == before {
                        continue;
                    }
                    C::store(&mut samples[j..j + 4], straight);
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
        changed
    }
}

/// A raster's sample, read as [`RasterBackdrop`] reads a pixel and
/// written as the merge writes one.
trait Stored: Copy {
    fn straight(pixel: &[Self]) -> [f32; 4];
    fn store(pixel: &mut [Self], straight: [f32; 4]);
}

impl Stored for u8 {
    fn straight(b: &[Self]) -> [f32; 4] {
        [b[0], b[1], b[2], b[3]].map(|v| f32::from(v) / 255.0)
    }

    fn store(pixel: &mut [Self], straight: [f32; 4]) {
        for (b, v) in pixel.iter_mut().zip(straight) {
            *b = <u8 as crate::raster::kernel::Channel>::from_unit(v);
        }
    }
}

impl Stored for f32 {
    fn straight(s: &[Self]) -> [f32; 4] {
        [s[0], s[1], s[2], s[3]]
    }

    fn store(pixel: &mut [Self], straight: [f32; 4]) {
        pixel.copy_from_slice(&straight);
    }
}

/// A premultiplied pixel of the canvas as a straight color.
fn straight(p: [f32; 4]) -> [f32; 4] {
    let a = p[3];
    if a <= 0.0 {
        return [0.0; 4];
    }
    [p[0] / a, p[1] / a, p[2] / a, a]
}

/// How much of the wet paint layer pixel `(x, y)` takes: `region`'s
/// share where anti-overflow holds the stroke, times what `paper` lets
/// through there. A paper laid once on the whole stroke (not by dab)
/// holds the paint back from its valleys however many dabs pass, as it
/// does for a stroke that does not mix; laid by dab, dabs that mix fill
/// the valleys one after another. `None`: all of it.
fn share(region: Option<&Region>, paper: Option<&Texture<'_>>, x: i32, y: i32) -> Option<f32> {
    let held = region.map(|r| r.at(x, y));
    let let_through = paper.map(|t| t.apply(1.0, [x as f32 + 0.5, y as f32 + 0.5]));
    match (held, let_through) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(1.0) * b.unwrap_or(1.0)),
    }
}

/// A layer pixel `before` (straight) with the wet canvas's `wet`
/// (straight) laid on it: by `region`'s share of the pixel where
/// anti-overflow holds the stroke, and at `before`'s alpha with
/// `keep_alpha`, so a locked clear pixel stays clear and keeps its
/// color samples.
fn settle(before: [f32; 4], wet: [f32; 4], region: Option<f32>, keep_alpha: bool) -> [f32; 4] {
    let share = region.map_or(1.0, unit);
    let mut out = if share >= 1.0 {
        wet
    } else if share <= 0.0 {
        before
    } else {
        // Mixed in premultiplied color, so a clear pixel's color does
        // not bleed in.
        let pre = |p: [f32; 4]| [p[0] * p[3], p[1] * p[3], p[2] * p[3], p[3]];
        let (b, w) = (pre(before), pre(wet));
        let m: [f32; 4] = std::array::from_fn(|k| b[k] + (w[k] - b[k]) * share);
        if m[3] > 0.0 {
            [m[0] / m[3], m[1] / m[3], m[2] / m[3], m[3]]
        } else {
            before
        }
    };
    if keep_alpha {
        if before[3] <= 0.0 || out[3] <= 0.0 {
            return before;
        }
        out[3] = before[3];
    }
    out
}

/// Box blurs premultiplied pixels (`width` wide) by `radius` each way,
/// rows then columns, holding the edge pixels beyond the border. `line`
/// is a buffer to work in.
fn blur_box(pixels: &mut [[f32; 4]], width: usize, radius: usize, line: &mut Vec<[f32; 4]>) {
    if width == 0 || radius == 0 {
        return;
    }
    let height = pixels.len() / width;
    let span = (2 * radius + 1) as f32;
    for y in 0..height {
        line.clear();
        line.extend_from_slice(&pixels[y * width..(y + 1) * width]);
        blur_run(line, radius, span, |x, v| pixels[y * width + x] = v);
    }
    for x in 0..width {
        line.clear();
        line.extend((0..height).map(|y| pixels[y * width + x]));
        blur_run(line, radius, span, |y, v| pixels[y * width + x] = v);
    }
}

/// The running mean of `line` over `radius` each way (`span` values),
/// each handed to `out` with its place.
fn blur_run(line: &[[f32; 4]], radius: usize, span: f32, mut out: impl FnMut(usize, [f32; 4])) {
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
    fn a_finger_tip_drags_a_thin_line_along_and_lets_it_fade() {
        // A dark line one pixel wide, upright at x = 20, on white.
        let mut picture = RasterTile2D::new(96, 32, PixelFormat::Rgba8).expect("a raster");
        for (i, px) in picture
            .bytes_mut()
            .expect("bytes")
            .chunks_mut(4)
            .enumerate()
        {
            px.copy_from_slice(if i % 96 == 20 {
                &[0, 0, 0, 255]
            } else {
                &[255, 255, 255, 255]
            });
        }
        let backdrop = RasterBackdrop {
            raster: &picture,
            origin: [0, 0],
        };
        let mut finger = WetCanvas::new(blender("finger-tip").expect("a blender"), [1.0; 3]);
        finger.stamp(&backdrop, &dabs(14.0, 80.0, 5.0), &Tips::round(1.0), None);
        let gray = |x: i32| finger.pixel(&backdrop, x, 16)[0];
        // The line is pushed right and spread out behind the finger: the
        // place it was is lighter, and the dark it lost lies further on,
        // the darkness kept (within what the softening between pixels
        // loses), fading out well before the stroke ends.
        assert!(gray(20) > 0.9, "{}", gray(20));
        let lost: f32 = (10..90).map(|x| 1.0 - gray(x)).sum();
        assert!(lost > 0.8 && lost < 1.2, "{lost}");
        let ahead: f32 = (24..90).map(|x| 1.0 - gray(x)).sum();
        assert!(ahead > 0.6, "{ahead}");
        assert!(gray(88) > 0.999, "{}", gray(88));
        // Off the stroke's path the line is untouched.
        assert_eq!(finger.pixel(&backdrop, 20, 2), [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn amount_of_paint_lays_the_brush_color_and_density_how_fully() {
        let picture = halves();
        let backdrop = RasterBackdrop {
            raster: &picture,
            origin: [0, 0],
        };
        let tips = Tips::round(1.0);
        let paint = |amount: f32, density: f32, from: f32, to: f32| {
            let wet = Wet {
                pickup: 1.0,
                amount,
                stretch: 0.0,
                blur: 0.0,
                density,
                smear: 0.0,
            };
            let mut canvas = WetCanvas::new(wet, [0.0, 1.0, 0.0]);
            canvas.stamp(&backdrop, &dabs(from, to, 6.0), &tips, None);
            canvas.pixel(&backdrop, to as i32, 16)
        };
        let full = paint(1.0, 1.0, 8.0, 20.0);
        assert!(full[1] > 0.95 && full[0] < 0.05, "{full:?}");
        // A brush lands full of its own paint (0.0.8: it no longer lands
        // holding what is under it), mixed on the spot with the wet paint
        // it touches as a loaded brush is: by its pickup (all of it
        // here), at most half and half.
        let landing = paint(0.5, 1.0, 8.0, 8.0);
        assert!(
            landing[1] >= 0.45 && landing[0] > 0.05 && landing[0] <= 0.55,
            "{landing:?}"
        );
        // From the next dab on, half paint lays the brush's color and
        // what it picked up and met together: a second dab in the same
        // place picks up the red under the first dab's rim, and lays a
        // mix of red and green.
        let half = paint(0.5, 1.0, 8.0, 9.0);
        assert!(
            half[0] > 0.05 && half[1] > 0.3 && half[1] < 0.95,
            "{half:?}"
        );
        let thin = paint(1.0, 0.3, 8.0, 8.0);
        assert!(thin[0] > full[0] + 0.2, "{thin:?}");
    }

    /// A brush whose own paint is capped lays it as strongly as its
    /// strongest dab, however close its dabs fall: thin paint stays
    /// thin along the stroke. Uncapped, the same dabs stack to cover.
    #[test]
    fn a_capped_stroke_lays_thin_paint_once_however_close_its_dabs() {
        let clear = RasterTile2D::new(64, 32, PixelFormat::Rgba8).expect("a raster");
        let backdrop = RasterBackdrop {
            raster: &clear,
            origin: [0, 0],
        };
        let tips = Tips::round(1.0);
        let alpha = |capped: bool| {
            let wet = Wet {
                pickup: 0.6,
                amount: 1.0,
                stretch: 0.0,
                blur: 0.0,
                density: 0.3,
                smear: 0.0,
            };
            let mut canvas = WetCanvas::new(wet, [0.0; 3]).capped(capped);
            canvas.stamp(&backdrop, &dabs(8.0, 40.0, 6.0), &tips, None);
            canvas.pixel(&backdrop, 24, 16)[3]
        };
        let once = alpha(true);
        assert!((once - 0.3).abs() < 0.02, "{once}");
        assert!(alpha(false) > 0.9, "stacked: {}", alpha(false));
    }

    /// A brush with paint of its own finds nothing to take up on a clear
    /// layer: it does not thin as it goes. A blend tool does drag the
    /// clearness along.
    #[test]
    fn a_loaded_brush_does_not_pick_up_clearness() {
        let clear = RasterTile2D::new(64, 32, PixelFormat::Rgba8).expect("a raster");
        let backdrop = RasterBackdrop {
            raster: &clear,
            origin: [0, 0],
        };
        let tips = Tips::round(1.0);
        let wet = Wet {
            pickup: 0.8,
            amount: 0.6,
            stretch: 0.0,
            blur: 0.0,
            density: 1.0,
            smear: 0.0,
        };
        let mut canvas = WetCanvas::new(wet, [0.0, 0.0, 1.0]).capped(true);
        canvas.stamp(&backdrop, &dabs(8.0, 40.0, 6.0), &tips, None);
        for x in [8, 24, 40] {
            let p = canvas.pixel(&backdrop, x, 16);
            assert!(p[3] > 0.99 && p[2] > 0.99, "at {x}: {p:?}");
        }
    }

    /// A paper laid on a mixing stroke as a whole holds the paint back
    /// from its valleys however many dabs pass over them.
    #[test]
    fn a_wet_strokes_paper_holds_paint_back_from_its_valleys() {
        let clear = RasterTile2D::new(64, 32, PixelFormat::Rgba8).expect("a raster");
        let backdrop = RasterBackdrop {
            raster: &clear,
            origin: [0, 0],
        };
        // Columns alternate between a high spot and a valley.
        let texels: Vec<f32> = (0..16 * 16)
            .map(|i| if (i % 16) % 2 == 0 { 1.0 } else { 0.0 })
            .collect();
        let paper = crate::texture::Paper::from_texels(16, 16, &texels).expect("a paper");
        let texture = Texture::new(
            &paper,
            crate::texture::TextureSettings {
                mode: crate::texture::TextureMode::Subtract,
                ..crate::texture::TextureSettings::default()
            },
        );
        let wet = Wet {
            pickup: 0.5,
            amount: 1.0,
            stretch: 0.0,
            blur: 0.0,
            density: 1.0,
            smear: 0.0,
        };
        let mut canvas = WetCanvas::new(wet, [0.0; 3]);
        canvas.stamp(&backdrop, &dabs(8.0, 40.0, 6.0), &Tips::round(1.0), None);
        let high = canvas.laid(&backdrop, 24, 16, None, Some(&texture), false);
        let valley = canvas.laid(&backdrop, 25, 16, None, Some(&texture), false);
        assert!(high[3] > 0.95, "{high:?}");
        assert!(valley[3] < 0.05, "{valley:?}");
        let mut picture = clear.clone();
        canvas
            .merge(&mut picture, [0, 0], None, Some(&texture), false)
            .expect("laid");
        let bytes = picture.bytes().expect("bytes");
        let alpha = |x: usize| bytes[(16 * 64 + x) * 4 + 3];
        assert!(alpha(24) > 240 && alpha(25) < 13);
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
        let changed = canvas
            .merge(&mut picture, [0, 0], None, None, false)
            .expect("merged");
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
