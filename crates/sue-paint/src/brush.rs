//! The brush engine: round dabs stamped along the pointer's path.
//!
//! A stroke paints straight into a layer's pixels (so the screen follows
//! the pen), but never stacks on itself: each touched 64-pixel tile keeps
//! its pixels from before the stroke and a coverage mask, and every pixel is
//! recomputed from those two. A pen at 50% opacity therefore stays at 50%
//! where the stroke crosses itself, while an airbrush builds up. When the
//! stroke ends, the kept tiles and their new pixels become one
//! [`RasterEdit`] for the undo history (backbone rule 2).

use std::collections::BTreeMap;

use sue_core::txn::{RasterEdit, RasterPatch};
use sue_core::{EntityId, RasterTile2D};

use crate::select::Selection;

/// Side of an undo tile, in pixels.
pub const TILE: u32 = 64;

/// How soft the edge of a hard brush is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AntiAlias {
    /// Hard pixels.
    None,
    /// A thin soft edge.
    #[default]
    Weak,
    /// A medium soft edge.
    Medium,
    /// A wide soft edge.
    Strong,
}

impl AntiAlias {
    /// Every level, weakest first.
    pub const ALL: [Self; 4] = [Self::None, Self::Weak, Self::Medium, Self::Strong];

    /// Width of the soft edge, in pixels.
    #[must_use]
    pub const fn edge(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Weak => 0.8,
            Self::Medium => 1.4,
            Self::Strong => 2.4,
        }
    }

    /// What people read.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Weak => "Weak",
            Self::Medium => "Medium",
            Self::Strong => "Strong",
        }
    }
}

/// The shape and behavior of a brush.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tip {
    /// Diameter in pixels.
    pub size: f32,
    /// 0 to 1.
    pub opacity: f32,
    /// 1 is a hard disc; lower values fade toward the rim.
    pub hardness: f32,
    /// Edge softness of hard tips.
    pub aa: AntiAlias,
    /// Distance between dabs, as a fraction of the size.
    pub spacing: f32,
    /// Dabs add up (airbrush) instead of reaching the opacity at most (pen).
    pub build_up: bool,
    /// Strength of one dab when building up, 0 to 1.
    pub flow: f32,
    /// Paper grain that breaks up the stroke (pencils), 0 to 1.
    pub grain: f32,
    /// Pen pressure changes the size.
    pub pressure_size: bool,
    /// Pen pressure changes the opacity.
    pub pressure_opacity: bool,
    /// Size at the lightest pressure, as a fraction of the size.
    pub min_size: f32,
    /// Removes pixels instead of painting them.
    pub eraser: bool,
}

impl Default for Tip {
    fn default() -> Self {
        Self {
            size: 8.0,
            opacity: 1.0,
            hardness: 1.0,
            aa: AntiAlias::Weak,
            spacing: 0.1,
            build_up: false,
            flow: 1.0,
            grain: 0.0,
            pressure_size: true,
            pressure_opacity: false,
            min_size: 0.15,
            eraser: false,
        }
    }
}

impl Tip {
    /// Radius and strength of a dab at `pressure` (0 to 1).
    #[must_use]
    pub fn dab(&self, pressure: f32) -> (f32, f32) {
        let pressure = pressure.clamp(0.0, 1.0);
        let scale = if self.pressure_size {
            self.min_size + (1.0 - self.min_size) * pressure
        } else {
            1.0
        };
        let strength = if self.pressure_opacity {
            self.opacity * pressure
        } else {
            self.opacity
        };
        (
            (self.size * 0.5 * scale).max(0.25),
            strength.clamp(0.0, 1.0),
        )
    }

    /// How much of a dab of `radius` covers a pixel `distance` from its
    /// center, 0 to 1.
    #[must_use]
    pub fn coverage(&self, distance: f32, radius: f32) -> f32 {
        if self.hardness >= 0.99 {
            let edge = self.aa.edge();
            if edge <= 0.0 {
                if distance <= radius { 1.0 } else { 0.0 }
            } else {
                ((radius - distance) / edge + 0.5).clamp(0.0, 1.0)
            }
        } else {
            let t = distance / radius.max(1e-3);
            if t >= 1.0 {
                return 0.0;
            }
            let inner = self.hardness.clamp(0.0, 0.98);
            if t <= inner {
                1.0
            } else {
                let u = (t - inner) / (1.0 - inner);
                1.0 - u * u * (3.0 - 2.0 * u)
            }
        }
    }
}

/// Deterministic paper grain at a pixel, 0 to 1.
fn grain_at(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65_535.0
}

/// A tile's pixels before the stroke and how much the stroke covers them.
#[derive(Clone, Debug)]
struct TileState {
    rect: [u32; 4],
    before: Vec<u8>,
    mask: Vec<f32>,
}

/// A stroke in progress on one layer.
#[derive(Clone, Debug)]
pub struct Stroke {
    layer: EntityId,
    tip: Tip,
    color: [f32; 3],
    alpha_lock: bool,
    tiles: BTreeMap<(u32, u32), TileState>,
    last: Option<([f32; 2], f32)>,
    carry: f32,
    dirty: Option<[u32; 4]>,
}

impl Stroke {
    /// A stroke with `tip` in `color` (sRGB, 0 to 1) on `layer`.
    /// `alpha_lock` keeps the layer's alpha (painting only over pixels).
    #[must_use]
    pub fn new(layer: EntityId, tip: Tip, color: [f32; 3], alpha_lock: bool) -> Self {
        Self {
            layer,
            tip,
            color,
            alpha_lock,
            tiles: BTreeMap::new(),
            last: None,
            carry: 0.0,
            dirty: None,
        }
    }

    /// The layer it paints on.
    #[must_use]
    pub fn layer(&self) -> EntityId {
        self.layer
    }

    /// The tip it paints with.
    #[must_use]
    pub fn tip(&self) -> &Tip {
        &self.tip
    }

    /// Continues the stroke to `at` (layer pixels) at `pressure`, stamping
    /// dabs every `spacing` along the way.
    pub fn to(
        &mut self,
        raster: &mut RasterTile2D,
        at: [f32; 2],
        pressure: f32,
        selection: Option<&Selection>,
    ) {
        let Some((from, from_pressure)) = self.last else {
            self.stamp(raster, at, pressure, selection);
            self.last = Some((at, pressure));
            return;
        };
        let (dx, dy) = (at[0] - from[0], at[1] - from[1]);
        let length = (dx * dx + dy * dy).sqrt();
        let (radius, _) = self.tip.dab(pressure.max(from_pressure));
        let step = (radius * 2.0 * self.tip.spacing).max(0.5);
        let mut travelled = step - self.carry;
        while travelled <= length {
            let t = if length > 0.0 {
                travelled / length
            } else {
                1.0
            };
            let point = [from[0] + dx * t, from[1] + dy * t];
            let p = from_pressure + (pressure - from_pressure) * t;
            self.stamp(raster, point, p, selection);
            travelled += step;
        }
        self.carry = step - (travelled - length);
        self.last = Some((at, pressure));
    }

    /// Paints one dab at `center`.
    fn stamp(
        &mut self,
        raster: &mut RasterTile2D,
        center: [f32; 2],
        pressure: f32,
        selection: Option<&Selection>,
    ) {
        let (width, height) = (raster.width(), raster.height());
        let (mut radius, mut strength) = self.tip.dab(pressure);
        // Tiny dabs keep their area instead of vanishing between pixels.
        if radius < 0.7 {
            strength *= (radius / 0.7).powi(2);
            radius = 0.7;
        }
        let reach = radius + self.tip.aa.edge() + 1.0;
        let x0 = (center[0] - reach).floor().max(0.0) as u32;
        let y0 = (center[1] - reach).floor().max(0.0) as u32;
        let x1 = ((center[0] + reach).ceil().max(0.0) as u32).min(width);
        let y1 = ((center[1] + reach).ceil().max(0.0) as u32).min(height);
        if x0 >= x1 || y0 >= y1 || strength <= 0.0 {
            return;
        }
        let stride = width as usize * 4;
        let tip = self.tip;
        let color = self.color;
        let alpha_lock = self.alpha_lock;
        let Some(bytes) = raster.bytes_mut() else {
            return;
        };
        for ty in (y0 / TILE)..=((y1 - 1) / TILE) {
            for tx in (x0 / TILE)..=((x1 - 1) / TILE) {
                let state = self.tiles.entry((tx, ty)).or_insert_with(|| {
                    let rect = [
                        tx * TILE,
                        ty * TILE,
                        TILE.min(width - tx * TILE),
                        TILE.min(height - ty * TILE),
                    ];
                    let mut before = Vec::with_capacity(rect[2] as usize * rect[3] as usize * 4);
                    for line in 0..rect[3] as usize {
                        let at = (rect[1] as usize + line) * stride + rect[0] as usize * 4;
                        before.extend_from_slice(&bytes[at..at + rect[2] as usize * 4]);
                    }
                    TileState {
                        rect,
                        before,
                        mask: vec![0.0; rect[2] as usize * rect[3] as usize],
                    }
                });
                let [rx, ry, rw, rh] = state.rect;
                let (sx0, sy0) = (x0.max(rx), y0.max(ry));
                let (sx1, sy1) = (x1.min(rx + rw), y1.min(ry + rh));
                for y in sy0..sy1 {
                    for x in sx0..sx1 {
                        let (px, py) = (x as f32 + 0.5 - center[0], y as f32 + 0.5 - center[1]);
                        let mut cover = tip.coverage((px * px + py * py).sqrt(), radius);
                        if cover <= 0.0 {
                            continue;
                        }
                        if tip.grain > 0.0 {
                            cover *= 1.0 - tip.grain * grain_at(x, y);
                        }
                        if let Some(selection) = selection {
                            cover *= selection.at(x, y);
                        }
                        let local = ((y - ry) * rw + (x - rx)) as usize;
                        let mask = &mut state.mask[local];
                        let old = *mask;
                        *mask = if tip.build_up {
                            old + (1.0 - old) * cover * strength * tip.flow
                        } else {
                            old.max(cover * strength)
                        };
                        if (*mask - old).abs() < 1e-6 {
                            continue;
                        }
                        let before = &state.before[local * 4..local * 4 + 4];
                        let pixel = paint_pixel(before, *mask, color, tip.eraser, alpha_lock);
                        let at = y as usize * stride + x as usize * 4;
                        bytes[at..at + 4].copy_from_slice(&pixel);
                    }
                }
            }
        }
        self.dirty = Some(match self.dirty {
            Some([a, b, c, d]) => [a.min(x0), b.min(y0), c.max(x1), d.max(y1)],
            None => [x0, y0, x1, y1],
        });
    }

    /// The rectangle `[x0, y0, x1, y1]` (layer pixels) painted since the
    /// last call.
    pub fn take_dirty(&mut self) -> Option<[u32; 4]> {
        self.dirty.take()
    }

    /// Ends the stroke: the undo step holding the tiles it changed.
    #[must_use]
    pub fn finish(self, raster: &RasterTile2D, label: &str) -> RasterEdit {
        let patches = self
            .tiles
            .into_values()
            .filter_map(|tile| RasterPatch::capture(raster, tile.rect, tile.before))
            .filter(|patch| patch.before != patch.after)
            .collect();
        RasterEdit::new(self.layer, label, patches)
    }
}

/// The pixel `before` becomes under a stroke covering it by `amount`.
fn paint_pixel(
    before: &[u8],
    amount: f32,
    color: [f32; 3],
    eraser: bool,
    alpha_lock: bool,
) -> [u8; 4] {
    let to_byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    let old = [
        f32::from(before[0]) / 255.0,
        f32::from(before[1]) / 255.0,
        f32::from(before[2]) / 255.0,
        f32::from(before[3]) / 255.0,
    ];
    if eraser {
        return [
            before[0],
            before[1],
            before[2],
            to_byte(old[3] * (1.0 - amount)),
        ];
    }
    if alpha_lock {
        let mix = |channel: usize| old[channel] + (color[channel] - old[channel]) * amount;
        return [to_byte(mix(0)), to_byte(mix(1)), to_byte(mix(2)), before[3]];
    }
    let out = sue_model::BlendMode::Normal.composite(old, [color[0], color[1], color[2], amount]);
    [
        to_byte(out[0]),
        to_byte(out[1]),
        to_byte(out[2]),
        to_byte(out[3]),
    ]
}

/// Smooths the pointer: each sample pulls the brush part of the way toward
/// it, more strongly the lower the stabilization (0 follows exactly).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stabilizer {
    at: Option<[f32; 2]>,
    follow: f32,
}

impl Stabilizer {
    /// A stabilizer of `level` 0 to 100.
    #[must_use]
    pub fn new(level: u32) -> Self {
        Self {
            at: None,
            follow: 1.0 / (1.0 + level.min(100) as f32 * 0.35),
        }
    }

    /// The smoothed position for a raw sample.
    pub fn sample(&mut self, raw: [f32; 2]) -> [f32; 2] {
        let next = match self.at {
            Some(at) => [
                at[0] + (raw[0] - at[0]) * self.follow,
                at[1] + (raw[1] - at[1]) * self.follow,
            ],
            None => raw,
        };
        self.at = Some(next);
        next
    }

    /// Points that close the gap to `raw` when the pen lifts.
    pub fn catch_up(&mut self, raw: [f32; 2]) -> Vec<[f32; 2]> {
        let Some(at) = self.at else {
            return vec![raw];
        };
        let steps = ((1.0 / self.follow).ceil() as usize).clamp(1, 24);
        let points = (1..=steps)
            .map(|step| {
                let t = step as f32 / steps as f32;
                [at[0] + (raw[0] - at[0]) * t, at[1] + (raw[1] - at[1]) * t]
            })
            .collect();
        self.at = Some(raw);
        points
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::{Arena, ComponentData, ComponentKind, Entity, PixelFormat, UndoStack};

    fn layer(width: u32, height: u32) -> RasterTile2D {
        RasterTile2D::new(width, height, PixelFormat::Rgba8).unwrap()
    }

    fn pixel(raster: &RasterTile2D, x: u32, y: u32) -> [u8; 4] {
        let at = (y * raster.width() + x) as usize * 4;
        raster.bytes().unwrap()[at..at + 4].try_into().unwrap()
    }

    #[test]
    fn a_pen_reaches_its_opacity_once_however_often_it_crosses() {
        let mut raster = layer(64, 64);
        let tip = Tip {
            size: 10.0,
            opacity: 0.5,
            pressure_size: false,
            ..Tip::default()
        };
        let mut stroke = Stroke::new(EntityId::new(), tip, [0.0, 0.0, 0.0], false);
        for point in [[10.0, 32.0], [54.0, 32.0], [10.0, 32.0], [54.0, 32.0]] {
            stroke.to(&mut raster, point, 1.0, None);
        }
        let center = pixel(&raster, 32, 32);
        assert_eq!(center[3], 128, "half opacity, not built up");
        assert_eq!(pixel(&raster, 32, 2)[3], 0, "outside the tip");
        assert!(stroke.take_dirty().is_some());
        assert!(stroke.take_dirty().is_none());
    }

    #[test]
    fn an_airbrush_builds_up_and_an_eraser_removes() {
        let mut raster = layer(32, 32);
        let tip = Tip {
            size: 12.0,
            hardness: 0.0,
            build_up: true,
            flow: 0.2,
            pressure_size: false,
            ..Tip::default()
        };
        let mut once = Stroke::new(EntityId::new(), tip, [1.0, 0.0, 0.0], false);
        once.to(&mut raster, [16.0, 16.0], 1.0, None);
        let light = pixel(&raster, 16, 16)[3];
        for _ in 0..10 {
            once.stamp(&mut raster, [16.0, 16.0], 1.0, None);
        }
        let heavy = pixel(&raster, 16, 16)[3];
        assert!(light > 0 && heavy > light, "{light} then {heavy}");
        assert_eq!(pixel(&raster, 16, 16)[0], 255, "still red");

        let eraser = Tip {
            size: 40.0,
            eraser: true,
            pressure_size: false,
            ..Tip::default()
        };
        let mut rub = Stroke::new(EntityId::new(), eraser, [0.0; 3], false);
        rub.to(&mut raster, [16.0, 16.0], 1.0, None);
        assert_eq!(pixel(&raster, 16, 16)[3], 0);
    }

    #[test]
    fn locked_alpha_paints_only_over_pixels() {
        let mut raster = layer(16, 16);
        raster.bytes_mut().unwrap()[..4].copy_from_slice(&[0, 0, 255, 200]);
        let tip = Tip {
            size: 64.0,
            pressure_size: false,
            ..Tip::default()
        };
        let mut stroke = Stroke::new(EntityId::new(), tip, [1.0, 1.0, 1.0], true);
        stroke.to(&mut raster, [8.0, 8.0], 1.0, None);
        assert_eq!(pixel(&raster, 0, 0), [255, 255, 255, 200]);
        assert_eq!(pixel(&raster, 5, 5)[3], 0, "clear pixels stay clear");
    }

    #[test]
    fn pressure_scales_size_and_selections_clip() {
        let tip = Tip {
            size: 20.0,
            min_size: 0.5,
            ..Tip::default()
        };
        assert_eq!(tip.dab(1.0).0, 10.0);
        assert_eq!(tip.dab(0.0).0, 5.0);
        let mut raster = layer(40, 40);
        let mut selection = Selection::new(40, 40);
        selection.rect([0, 0, 20, 40], crate::select::Op::Replace);
        let mut stroke = Stroke::new(EntityId::new(), tip, [0.0; 3], false);
        stroke.to(&mut raster, [20.0, 20.0], 1.0, Some(&selection));
        assert_eq!(pixel(&raster, 15, 20)[3], 255);
        assert_eq!(pixel(&raster, 25, 20)[3], 0, "outside the selection");
    }

    #[test]
    fn a_finished_stroke_undoes_through_its_tiles() {
        let mut arena = Arena::new();
        let handle = arena.insert(Entity::new("Ink", layer(200, 100)));
        let id = arena.get(handle).unwrap().id();
        let mut stroke = Stroke::new(id, Tip::default(), [0.0; 3], false);
        let edit = {
            let Some(ComponentData::RasterTile2D(raster)) = arena
                .get_mut(handle)
                .unwrap()
                .component_mut(ComponentKind::RasterTile2D)
            else {
                panic!("no raster");
            };
            stroke.to(raster, [10.0, 10.0], 1.0, None);
            stroke.to(raster, [150.0, 20.0], 1.0, None);
            stroke.finish(raster, "Pen")
        };
        assert_eq!(edit.patches().len(), 3, "one row of three tiles");
        let alpha = |arena: &Arena| {
            pixel(
                arena
                    .get(handle)
                    .unwrap()
                    .component(ComponentKind::RasterTile2D)
                    .unwrap()
                    .as_raster()
                    .unwrap(),
                80,
                15,
            )[3]
        };
        assert_eq!(alpha(&arena), 255);
        let mut history = UndoStack::new();
        history.record(edit);
        history.undo(&mut arena).unwrap();
        assert_eq!(alpha(&arena), 0);
        history.redo(&mut arena).unwrap();
        assert_eq!(alpha(&arena), 255);
    }

    #[test]
    fn the_stabilizer_lags_and_catches_up() {
        let mut free = Stabilizer::new(0);
        assert_eq!(free.sample([10.0, 0.0]), [10.0, 0.0]);
        assert_eq!(free.sample([20.0, 0.0]), [20.0, 0.0]);
        let mut steady = Stabilizer::new(20);
        steady.sample([0.0, 0.0]);
        let lagging = steady.sample([100.0, 0.0]);
        assert!(lagging[0] > 0.0 && lagging[0] < 50.0);
        let rest = steady.catch_up([100.0, 0.0]);
        assert_eq!(rest.last(), Some(&[100.0, 0.0]));
    }
}
