//! Puppet warp: pins on a layer bend its pixels around them while keeping
//! shapes as rigid as they can be, as Clip Studio Paint's Puppet warp does.
//! Pins that stay put hold their part of the picture still.
//!
//! The bend is the rigid kind of moving least squares (Schaefer, McPhail
//! and Warren, "Image Deformation Using Moving Least Squares", 2006): every
//! point moves by the rotation and translation that best carries the pins
//! near it, nearer pins counting more. Pixels are found backwards, from
//! where they land to where they came from, on a coarse grid that is
//! filled in between, and sampled with premultiplied bilinear filtering.
//!
//! [`warp`] bends the whole layer as one piece, as Mesh Transformation's
//! points should. [`puppet`] is Puppet warp proper: it lays a triangle
//! mesh on the layer's silhouette ([`PuppetMesh`]), and each separate
//! part of the drawing bends only by the pins on it, so a dragged arm
//! does not pull a figure standing beside it. Its pins also turn
//! ([`Pin::angle`]) and are picked one by one or several at once.

use std::collections::HashMap;

/// A pin: where it was put and where it has been dragged (layer pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pin {
    /// Where it was put.
    pub from: [f32; 2],
    /// Where it is now.
    pub to: [f32; 2],
    /// How far the part under it is turned, in radians (clockwise on the
    /// screen, as y runs down).
    pub angle: f32,
    /// Picked, to move or turn with the others picked (Shift and click).
    pub picked: bool,
}

impl Pin {
    /// A pin put at `at`.
    #[must_use]
    pub const fn at(at: [f32; 2]) -> Self {
        Self {
            from: at,
            to: at,
            angle: 0.0,
            picked: false,
        }
    }

    /// The same pin dragged to `to`.
    #[must_use]
    pub const fn dragged(mut self, to: [f32; 2]) -> Self {
        self.to = to;
        self
    }
}

/// Picks pin `index`: alone, or (`add`, Shift and click) in or out of the
/// pins picked already.
pub fn pick(pins: &mut [Pin], index: usize, add: bool) {
    if index >= pins.len() {
        return;
    }
    if add {
        pins[index].picked = !pins[index].picked;
    } else {
        for (at, pin) in pins.iter_mut().enumerate() {
            pin.picked = at == index;
        }
    }
}

/// Picks the pins inside `[x0, y0, x1, y1]` (where they are now), adding
/// to those picked when `add`.
pub fn pick_inside(pins: &mut [Pin], [x0, y0, x1, y1]: [f32; 4], add: bool) {
    for pin in pins {
        let inside = (x0.min(x1)..=x0.max(x1)).contains(&pin.to[0])
            && (y0.min(y1)..=y0.max(y1)).contains(&pin.to[1]);
        pin.picked = inside || (add && pin.picked);
    }
}

/// Moves the picked pins by `by`.
pub fn move_picked(pins: &mut [Pin], by: [f32; 2]) {
    for pin in pins.iter_mut().filter(|pin| pin.picked) {
        pin.to = [pin.to[0] + by[0], pin.to[1] + by[1]];
    }
}

/// Turns the picked pins by `angle` radians around `about`: where each
/// is and the part under it.
pub fn turn_picked(pins: &mut [Pin], about: [f32; 2], angle: f32) {
    let (sin, cos) = angle.sin_cos();
    for pin in pins.iter_mut().filter(|pin| pin.picked) {
        let (dx, dy) = (pin.to[0] - about[0], pin.to[1] - about[1]);
        pin.to = [
            about[0] + dx * cos - dy * sin,
            about[1] + dx * sin + dy * cos,
        ];
        pin.angle += angle;
    }
}

/// How Puppet warp samples the picture it bends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Interpolation {
    /// Smooth: between pixels (bilinear).
    #[default]
    Smooth,
    /// Hard: the nearest pixel, for pixel art.
    Nearest,
}

impl Interpolation {
    /// Its stable name in saved settings.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Smooth => "smooth",
            Self::Nearest => "nearest",
        }
    }
}

/// Puppet warp's Tool Property settings.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(clippy::struct_excessive_bools, reason = "one switch each, as shown")]
pub struct PuppetSettings {
    /// The original stays under what is bent (Keep original image).
    pub keep_original: bool,
    /// How far, in pixels, the mesh reaches past the drawing's edge
    /// (Expansion).
    pub expansion: f32,
    /// The mesh shows over the canvas (Show mesh).
    pub show_mesh: bool,
    /// A click off the pins puts a new pin (Click pin); off, it picks.
    pub click_adds_pins: bool,
    /// Several pins can be picked at once to move or turn together
    /// (Select pins).
    pub select_pins: bool,
    /// Each part turns as the bend around it turns (Correct pin
    /// rotation); off, a pin holds its part at its own angle.
    pub correct_pin_rotation: bool,
    /// How it samples the picture.
    pub interpolation: Interpolation,
    /// The mesh's spacing, in pixels.
    pub spacing: u32,
}

impl Default for PuppetSettings {
    fn default() -> Self {
        Self {
            keep_original: false,
            expansion: 4.0,
            show_mesh: true,
            click_adds_pins: true,
            select_pins: true,
            correct_pin_rotation: true,
            interpolation: Interpolation::Smooth,
            spacing: 8,
        }
    }
}

/// A triangle, its corners in layer pixels.
pub type Triangle = [[f32; 2]; 3];

/// The triangle mesh Puppet warp bends: squares of the grid that hold
/// part of the drawing (or lie within the expansion of it), two triangles
/// each, sorted into parts that touch along an edge.
#[derive(Clone, Debug, PartialEq)]
pub struct PuppetMesh {
    /// The mesh's spacing, in pixels.
    pub spacing: u32,
    /// The grid squares held, as `(column, row)`.
    cells: Vec<(u32, u32)>,
    /// The part each square belongs to.
    part: Vec<usize>,
    /// How many separate parts there are.
    parts: usize,
}

impl PuppetMesh {
    /// The mesh over the pixels of `source` (RGBA8, `width` by `height`)
    /// that are not clear, reaching `expansion` pixels past them, with
    /// squares `spacing` pixels wide.
    #[must_use]
    pub fn build(source: &[u8], width: u32, height: u32, spacing: u32, expansion: f32) -> Self {
        let spacing = spacing.max(2);
        let (columns, rows) = (width.div_ceil(spacing), height.div_ceil(spacing));
        let (w, h) = (width as usize, height as usize);
        // A table of opaque pixel counts, to ask any rectangle at once.
        let mut table = vec![0_u32; (w + 1) * (h + 1)];
        for y in 0..h {
            let mut row = 0;
            for x in 0..w {
                row += u32::from(source.get((y * w + x) * 4 + 3).is_some_and(|a| *a > 0));
                table[(y + 1) * (w + 1) + x + 1] = table[y * (w + 1) + x + 1] + row;
            }
        }
        let reach = expansion.max(0.0).ceil() as i64;
        let any = |x0: i64, y0: i64, x1: i64, y1: i64| {
            let clamp = |value: i64, top: usize| value.clamp(0, top as i64) as usize;
            let (x0, y0, x1, y1) = (clamp(x0, w), clamp(y0, h), clamp(x1, w), clamp(y1, h));
            if x1 <= x0 || y1 <= y0 {
                return false;
            }
            table[y1 * (w + 1) + x1] + table[y0 * (w + 1) + x0]
                > table[y0 * (w + 1) + x1] + table[y1 * (w + 1) + x0]
        };
        let mut cells = Vec::new();
        for row in 0..rows {
            for column in 0..columns {
                let (x, y) = (i64::from(column * spacing), i64::from(row * spacing));
                let s = i64::from(spacing);
                if any(x - reach, y - reach, x + s + reach, y + s + reach) {
                    cells.push((column, row));
                }
            }
        }
        // Parts: squares that share an edge, found by joining sets.
        let index_of: HashMap<(u32, u32), usize> = cells
            .iter()
            .enumerate()
            .map(|(index, cell)| (*cell, index))
            .collect();
        let mut parent: Vec<usize> = (0..cells.len()).collect();
        fn root(parent: &mut [usize], mut at: usize) -> usize {
            while parent[at] != at {
                parent[at] = parent[parent[at]];
                at = parent[at];
            }
            at
        }
        for (index, (column, row)) in cells.iter().enumerate() {
            for neighbor in [(column + 1, *row), (*column, row + 1)] {
                if let Some(&other) = index_of.get(&neighbor) {
                    let (a, b) = (root(&mut parent, index), root(&mut parent, other));
                    parent[a] = b;
                }
            }
        }
        let mut names: HashMap<usize, usize> = HashMap::new();
        let part: Vec<usize> = (0..cells.len())
            .map(|index| {
                let found = root(&mut parent, index);
                let next = names.len();
                *names.entry(found).or_insert(next)
            })
            .collect();
        Self {
            spacing,
            cells,
            part,
            parts: names.len(),
        }
    }

    /// How many separate parts the mesh has.
    #[must_use]
    pub fn parts(&self) -> usize {
        self.parts
    }

    /// The mesh's triangles where they lie now, before any pin moves.
    #[must_use]
    pub fn triangles(&self) -> Vec<Triangle> {
        self.cells
            .iter()
            .flat_map(|cell| self.cell_triangles(*cell))
            .collect()
    }

    fn cell_triangles(&self, (column, row): (u32, u32)) -> [Triangle; 2] {
        let s = self.spacing as f32;
        let (x, y) = (column as f32 * s, row as f32 * s);
        let (a, b, c, d) = ([x, y], [x + s, y], [x + s, y + s], [x, y + s]);
        [[a, b, c], [a, c, d]]
    }

    /// The part a point lies in: the square under it, or the nearest
    /// square within two squares.
    #[must_use]
    pub fn part_at(&self, point: [f32; 2]) -> Option<usize> {
        let s = self.spacing as f32;
        let mut best: Option<(f32, usize)> = None;
        for (index, (column, row)) in self.cells.iter().enumerate() {
            let center = [(*column as f32 + 0.5) * s, (*row as f32 + 0.5) * s];
            let (dx, dy) = (
                ((point[0] - center[0]).abs() - s / 2.0).max(0.0),
                ((point[1] - center[1]).abs() - s / 2.0).max(0.0),
            );
            let distance = dx.hypot(dy);
            if distance <= 2.0 * s && best.is_none_or(|(found, _)| distance < found) {
                best = Some((distance, self.part[index]));
            }
        }
        best.map(|(_, part)| part)
    }

    /// Each triangle where it starts and where `pins` carry it: a part
    /// bends only by the pins on it, and a part with none stays.
    #[must_use]
    pub fn bent(&self, pins: &[Pin], settings: &PuppetSettings) -> Vec<(Triangle, Triangle)> {
        let reach = self.spacing as f32 * 2.0;
        let mut pairs: Vec<Vec<([f32; 2], [f32; 2])>> = vec![Vec::new(); self.parts];
        for pin in pins {
            let Some(part) = self.part_at(pin.from) else {
                continue;
            };
            pairs[part].push((pin.from, pin.to));
            // A pin that holds or turns its part: two more points beside
            // it, carried round by its angle.
            if pin.angle != 0.0 || !settings.correct_pin_rotation {
                let (sin, cos) = pin.angle.sin_cos();
                for offset in [[reach, 0.0], [0.0, reach]] {
                    let turned = [
                        offset[0] * cos - offset[1] * sin,
                        offset[0] * sin + offset[1] * cos,
                    ];
                    pairs[part].push((
                        [pin.from[0] + offset[0], pin.from[1] + offset[1]],
                        [pin.to[0] + turned[0], pin.to[1] + turned[1]],
                    ));
                }
            }
        }
        let mut moved: HashMap<(usize, u32, u32), [f32; 2]> = HashMap::new();
        let s = self.spacing;
        let mut out = Vec::with_capacity(self.cells.len() * 2);
        for (index, cell) in self.cells.iter().enumerate() {
            let part = self.part[index];
            for triangle in self.cell_triangles(*cell) {
                let carried = triangle.map(|corner| {
                    let key = (part, corner[0] as u32 / s, corner[1] as u32 / s);
                    *moved
                        .entry(key)
                        .or_insert_with(|| deform(corner, &pairs[part]))
                });
                out.push((triangle, carried));
            }
        }
        out
    }
}

/// Puppet warp: `source` (RGBA8, `width` by `height`) bent by `pins` on a
/// triangle mesh of its silhouette, as `settings` say; the same size.
#[must_use]
pub fn puppet(
    source: &[u8],
    width: u32,
    height: u32,
    pins: &[Pin],
    settings: &PuppetSettings,
) -> Vec<u8> {
    let still = pins
        .iter()
        .all(|pin| pin.from == pin.to && pin.angle == 0.0);
    if (still && settings.correct_pin_rotation) || width == 0 || height == 0 {
        return source.to_vec();
    }
    let mesh = PuppetMesh::build(source, width, height, settings.spacing, settings.expansion);
    let mut out = if settings.keep_original {
        source.to_vec()
    } else {
        vec![0; source.len()]
    };
    for (from, to) in mesh.bent(pins, settings) {
        draw_triangle(
            &mut out,
            [width, height],
            source,
            &from,
            &to,
            settings.interpolation,
            settings.keep_original,
        );
    }
    out
}

/// Draws the part of `source` under triangle `from` onto `out` where
/// triangle `to` lies, sampling backwards; laid over what is there when
/// `over`, else in its place.
#[allow(clippy::too_many_arguments, reason = "a triangle and where it goes")]
fn draw_triangle(
    out: &mut [u8],
    [width, height]: [u32; 2],
    source: &[u8],
    from: &Triangle,
    to: &Triangle,
    interpolation: Interpolation,
    over: bool,
) {
    let [a, b, c] = *to;
    let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
    if area.abs() < 1e-6 {
        return;
    }
    let (x0, x1) = (
        a[0].min(b[0]).min(c[0]).floor().max(0.0) as u32,
        (a[0].max(b[0]).max(c[0]).ceil() as i64).clamp(0, i64::from(width)) as u32,
    );
    let (y0, y1) = (
        a[1].min(b[1]).min(c[1]).floor().max(0.0) as u32,
        (a[1].max(b[1]).max(c[1]).ceil() as i64).clamp(0, i64::from(height)) as u32,
    );
    // A hair of slack so pixels on a shared edge are not lost.
    let slack = -1e-4;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            let l1 = ((p[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (p[1] - a[1])) / area;
            let l2 = ((b[0] - a[0]) * (p[1] - a[1]) - (p[0] - a[0]) * (b[1] - a[1])) / area;
            let l0 = 1.0 - l1 - l2;
            if l0 < slack || l1 < slack || l2 < slack {
                continue;
            }
            let sx = l0 * from[0][0] + l1 * from[1][0] + l2 * from[2][0];
            let sy = l0 * from[0][1] + l1 * from[1][1] + l2 * from[2][1];
            let pixel = match interpolation {
                Interpolation::Smooth => sample(source, width, height, sx, sy),
                Interpolation::Nearest => {
                    let (nx, ny) = (sx.floor(), sy.floor());
                    if nx < 0.0 || ny < 0.0 || nx >= width as f32 || ny >= height as f32 {
                        [0; 4]
                    } else {
                        let at = ((ny as u32 * width + nx as u32) * 4) as usize;
                        [source[at], source[at + 1], source[at + 2], source[at + 3]]
                    }
                }
            };
            let at = ((y * width + x) * 4) as usize;
            if !over {
                out[at..at + 4].copy_from_slice(&pixel);
                continue;
            }
            let cover = f32::from(pixel[3]) / 255.0;
            if cover <= 0.0 {
                continue;
            }
            let under = f32::from(out[at + 3]) / 255.0;
            let alpha = cover + under * (1.0 - cover);
            for channel in 0..3 {
                let mixed = f32::from(pixel[channel]) * cover
                    + f32::from(out[at + channel]) * under * (1.0 - cover);
                out[at + channel] = (mixed / alpha).round().clamp(0.0, 255.0) as u8;
            }
            out[at + 3] = (alpha * 255.0).round() as u8;
        }
    }
}

/// `source` bent by a lattice: the grid of points `from` (`columns` by
/// `rows`, row by row) carried to `to`, each square of the grid as two
/// triangles (Mesh Transformation with a chosen grid). What lies outside
/// the grid stays clear.
#[must_use]
pub fn lattice(
    source: &[u8],
    width: u32,
    height: u32,
    [columns, rows]: [usize; 2],
    from: &[[f32; 2]],
    to: &[[f32; 2]],
    interpolation: Interpolation,
) -> Vec<u8> {
    let mut out = vec![0; source.len()];
    if columns < 2 || rows < 2 || from.len() != columns * rows || to.len() != from.len() {
        return source.to_vec();
    }
    for row in 0..rows - 1 {
        for column in 0..columns - 1 {
            let corner = |points: &[[f32; 2]], dx: usize, dy: usize| {
                points[(row + dy) * columns + column + dx]
            };
            for [p, q, r] in [[(0, 0), (1, 0), (1, 1)], [(0, 0), (1, 1), (0, 1)]] {
                let pick = |points: &[[f32; 2]]| {
                    [
                        corner(points, p.0, p.1),
                        corner(points, q.0, q.1),
                        corner(points, r.0, r.1),
                    ]
                };
                draw_triangle(
                    &mut out,
                    [width, height],
                    source,
                    &pick(from),
                    &pick(to),
                    interpolation,
                    false,
                );
            }
        }
    }
    out
}

/// Where `v` goes under the rigid moving least squares bend that carries
/// each pair's first point to its second.
#[must_use]
pub fn deform(v: [f32; 2], pairs: &[([f32; 2], [f32; 2])]) -> [f32; 2] {
    match pairs {
        [] => v,
        [(p, q)] => [v[0] + q[0] - p[0], v[1] + q[1] - p[1]],
        _ => {
            let v = [f64::from(v[0]), f64::from(v[1])];
            let wide = |point: [f32; 2]| [f64::from(point[0]), f64::from(point[1])];
            let mut total = 0.0;
            let (mut p_star, mut q_star) = ([0.0; 2], [0.0; 2]);
            for (p, q) in pairs {
                let (p, q) = (wide(*p), wide(*q));
                let d2 = (p[0] - v[0]).powi(2) + (p[1] - v[1]).powi(2);
                if d2 < 1e-10 {
                    return [q[0] as f32, q[1] as f32];
                }
                let w = 1.0 / d2;
                total += w;
                for axis in 0..2 {
                    p_star[axis] += w * p[axis];
                    q_star[axis] += w * q[axis];
                }
            }
            for axis in 0..2 {
                p_star[axis] /= total;
                q_star[axis] /= total;
            }
            let d = [v[0] - p_star[0], v[1] - p_star[1]];
            // With (x, y) perpendicular as (-y, x).
            let d_perp = [-d[1], d[0]];
            let mut f = [0.0; 2];
            for (p, q) in pairs {
                let (p, q) = (wide(*p), wide(*q));
                let w = 1.0 / ((p[0] - v[0]).powi(2) + (p[1] - v[1]).powi(2));
                let ph = [p[0] - p_star[0], p[1] - p_star[1]];
                let qh = [q[0] - q_star[0], q[1] - q_star[1]];
                let ph_perp = [-ph[1], ph[0]];
                let dot = |a: [f64; 2], b: [f64; 2]| a[0] * b[0] + a[1] * b[1];
                let a00 = dot(ph, d);
                let a01 = -dot(ph, d_perp);
                let a10 = -dot(ph_perp, d);
                let a11 = dot(ph_perp, d_perp);
                f[0] += w * (qh[0] * a00 + qh[1] * a10);
                f[1] += w * (qh[0] * a01 + qh[1] * a11);
            }
            let length = f[0].hypot(f[1]);
            let reach = d[0].hypot(d[1]);
            if length < 1e-12 {
                return [(q_star[0] + d[0]) as f32, (q_star[1] + d[1]) as f32];
            }
            [
                (q_star[0] + f[0] / length * reach) as f32,
                (q_star[1] + f[1] / length * reach) as f32,
            ]
        }
    }
}

/// Where a point of the picture lands once the pins have moved.
#[must_use]
pub fn forward(v: [f32; 2], pins: &[Pin]) -> [f32; 2] {
    let pairs: Vec<_> = pins.iter().map(|pin| (pin.from, pin.to)).collect();
    deform(v, &pairs)
}

/// `source` (RGBA8, `width` by `height`) bent by `pins`, the same size.
/// The bend is worked out every `step` pixels and filled in between.
#[must_use]
pub fn warp(source: &[u8], width: u32, height: u32, pins: &[Pin], step: u32) -> Vec<u8> {
    if pins.iter().all(|pin| pin.from == pin.to) || width == 0 || height == 0 {
        return source.to_vec();
    }
    let back: Vec<_> = pins.iter().map(|pin| (pin.to, pin.from)).collect();
    let step = step.max(1);
    let columns = width.div_ceil(step) + 1;
    let rows = height.div_ceil(step) + 1;
    let grid: Vec<[f32; 2]> = (0..rows)
        .flat_map(|row| (0..columns).map(move |column| (column, row)))
        .map(|(column, row)| deform([(column * step) as f32, (row * step) as f32], &back))
        .collect();
    let at = |column: u32, row: u32| grid[(row * columns + column) as usize];
    let mut out = vec![0; source.len()];
    for y in 0..height {
        let (row, fy) = (y / step, (y % step) as f32 / step as f32);
        for x in 0..width {
            let (column, fx) = (x / step, (x % step) as f32 / step as f32);
            let mix = |a: [f32; 2], b: [f32; 2], t: f32| {
                [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
            };
            let top = mix(at(column, row), at(column + 1, row), fx);
            let bottom = mix(at(column, row + 1), at(column + 1, row + 1), fx);
            let [sx, sy] = mix(top, bottom, fy);
            let pixel = sample(source, width, height, sx, sy);
            let index = ((y * width + x) * 4) as usize;
            out[index..index + 4].copy_from_slice(&pixel);
        }
    }
    out
}

/// The color at `[x, y]` of an RGBA8 picture (pixel corners at whole
/// numbers), bilinear over premultiplied alpha so edges stay clean.
pub(crate) fn sample(source: &[u8], width: u32, height: u32, x: f32, y: f32) -> [u8; 4] {
    let (x, y) = (x - 0.5, y - 0.5);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let mut sum = [0.0_f32; 3];
    let mut alpha = 0.0_f32;
    for (dx, dy, weight) in [
        (0, 0, (1.0 - fx) * (1.0 - fy)),
        (1, 0, fx * (1.0 - fy)),
        (0, 1, (1.0 - fx) * fy),
        (1, 1, fx * fy),
    ] {
        let (px, py) = (x0 as i64 + dx, y0 as i64 + dy);
        if weight <= 0.0 || px < 0 || py < 0 || px >= i64::from(width) || py >= i64::from(height) {
            continue;
        }
        let index = ((py as u32 * width + px as u32) * 4) as usize;
        let a = f32::from(source[index + 3]) / 255.0 * weight;
        for (channel, value) in sum.iter_mut().zip(&source[index..index + 3]) {
            *channel += f32::from(*value) * a;
        }
        alpha += a;
    }
    if alpha <= 1e-6 {
        return [0; 4];
    }
    [
        (sum[0] / alpha).round().clamp(0.0, 255.0) as u8,
        (sum[1] / alpha).round().clamp(0.0, 255.0) as u8,
        (sum[2] / alpha).round().clamp(0.0, 255.0) as u8,
        (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

/// The pixels of an RGBA8 picture that are not clear, as `[x0, y0, x1,
/// y1]` (exclusive), or `None` if it is all clear.
#[must_use]
pub fn bounds(source: &[u8], width: u32, height: u32) -> Option<[u32; 4]> {
    let mut found: Option<[u32; 4]> = None;
    for y in 0..height {
        for x in 0..width {
            if source[((y * width + x) * 4 + 3) as usize] == 0 {
                continue;
            }
            found = Some(match found {
                None => [x, y, x + 1, y + 1],
                Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1)],
            });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn pins_carry_the_picture_rigidly() {
        // No pins and unmoved pins change nothing; one pin translates.
        assert_eq!(deform([3.0, 4.0], &[]), [3.0, 4.0]);
        assert!(close(
            forward([3.0, 4.0], &[Pin::at([0.0, 0.0])]),
            [3.0, 4.0]
        ));
        let moved = Pin::at([0.0, 0.0]).dragged([5.0, -2.0]);
        assert!(close(forward([3.0, 4.0], &[moved]), [8.0, 2.0]));
        // Two pins turned a quarter around the first: everything turns.
        let turn = [
            Pin::at([0.0, 0.0]),
            Pin::at([10.0, 0.0]).dragged([0.0, 10.0]),
        ];
        assert!(close(forward([5.0, 5.0], &turn), [-5.0, 5.0]));
        assert!(
            close(forward([10.0, 0.0], &turn), [0.0, 10.0]),
            "a pin lands where it was dragged"
        );
        // A point between still and dragged pins moves part way, smoothly.
        let bend = [
            Pin::at([0.0, 0.0]),
            Pin::at([100.0, 0.0]),
            Pin::at([50.0, 0.0]).dragged([50.0, 30.0]),
        ];
        let near = forward([45.0, 0.0], &bend);
        let far = forward([5.0, 0.0], &bend);
        assert!(
            near[1] > far[1] && near[1] > 10.0 && far[1] < 5.0,
            "{near:?} {far:?}"
        );
    }

    #[test]
    fn a_warped_layer_moves_its_pixels_and_keeps_pinned_ones() {
        let (width, height) = (64, 64);
        let mut source = vec![0; (width * height * 4) as usize];
        // A red square at 8..16, a blue one at 44..52.
        for y in 8..16 {
            for x in 8..16 {
                source[((y * width + x) * 4) as usize..][..4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        for y in 44..52 {
            for x in 44..52 {
                source[((y * width + x) * 4) as usize..][..4].copy_from_slice(&[0, 0, 255, 255]);
            }
        }
        let still = [Pin::at([12.0, 12.0]), Pin::at([48.0, 48.0])];
        assert_eq!(warp(&source, width, height, &still, 8), source);
        // Hold the red square, drag the blue one up by 20.
        let pins = [
            Pin::at([12.0, 12.0]),
            Pin::at([48.0, 48.0]).dragged([48.0, 28.0]),
        ];
        let bent = warp(&source, width, height, &pins, 4);
        let pixel = |buffer: &[u8], x: u32, y: u32| {
            let index = ((y * width + x) * 4) as usize;
            [
                buffer[index],
                buffer[index + 1],
                buffer[index + 2],
                buffer[index + 3],
            ]
        };
        assert_eq!(
            pixel(&bent, 12, 12),
            [255, 0, 0, 255],
            "the held square stays"
        );
        assert_eq!(
            pixel(&bent, 48, 28)[2],
            255,
            "the dragged square came along"
        );
        assert!(pixel(&bent, 48, 48)[3] < 128, "and left its old place");
        assert_eq!(bounds(&source, width, height), Some([8, 8, 52, 52]));
    }

    /// A 64 by 64 clear picture with a red square at 8..16 and a blue
    /// one at 44..52, apart.
    fn two_squares() -> Vec<u8> {
        let width = 64;
        let mut source = vec![0; 64 * 64 * 4];
        for y in 8..16 {
            for x in 8..16 {
                source[(y * width + x) * 4..][..4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        for y in 44..52 {
            for x in 44..52 {
                source[(y * width + x) * 4..][..4].copy_from_slice(&[0, 0, 255, 255]);
            }
        }
        source
    }

    fn pixel(buffer: &[u8], x: u32, y: u32) -> [u8; 4] {
        let index = ((y * 64 + x) * 4) as usize;
        [
            buffer[index],
            buffer[index + 1],
            buffer[index + 2],
            buffer[index + 3],
        ]
    }

    #[test]
    fn the_puppet_mesh_lies_on_the_silhouette_in_separate_parts() {
        let source = two_squares();
        let mesh = PuppetMesh::build(&source, 64, 64, 8, 0.0);
        assert_eq!(mesh.parts(), 2, "the two squares are apart");
        // The red square fills one square of the grid, the blue one (off
        // the grid) four; two triangles each.
        assert_eq!(mesh.triangles().len(), 10);
        assert!(mesh.part_at([12.0, 12.0]).is_some());
        assert_ne!(mesh.part_at([12.0, 12.0]), mesh.part_at([48.0, 48.0]));
        assert_eq!(mesh.part_at([28.0, 28.0]), None, "nothing drawn near");
        // Expansion reaches further, and joins what comes close.
        let wide = PuppetMesh::build(&source, 64, 64, 8, 4.0);
        assert!(wide.triangles().len() > 4);
        let joined = PuppetMesh::build(&source, 64, 64, 8, 20.0);
        assert_eq!(joined.parts(), 1);
    }

    #[test]
    fn puppet_warp_moves_only_the_part_a_pin_is_on() {
        let source = two_squares();
        let settings = PuppetSettings::default();
        // Unmoved pins change nothing.
        let still = [Pin::at([12.0, 12.0]), Pin::at([48.0, 48.0])];
        assert_eq!(puppet(&source, 64, 64, &still, &settings), source);
        // One pin on the blue square, dragged up by 20: the red square,
        // with no pin of its own, stays where it is. (The one-piece bend
        // carries everything along with a single pin.)
        let pins = [Pin::at([48.0, 48.0]).dragged([48.0, 28.0])];
        let bent = puppet(&source, 64, 64, &pins, &settings);
        assert_eq!(pixel(&bent, 12, 12), [255, 0, 0, 255], "the red stays");
        assert_eq!(pixel(&bent, 48, 28), [0, 0, 255, 255], "the blue came");
        assert_eq!(pixel(&bent, 48, 48)[3], 0, "and left its old place");
        let whole = warp(&source, 64, 64, &pins, 4);
        assert_eq!(pixel(&whole, 12, 12)[3], 0, "the one-piece bend moves red");
        // Keep original image: the old place keeps the picture.
        let kept = puppet(
            &source,
            64,
            64,
            &pins,
            &PuppetSettings {
                keep_original: true,
                ..settings
            },
        );
        assert_eq!(pixel(&kept, 48, 48), [0, 0, 255, 255]);
        assert_eq!(pixel(&kept, 48, 28), [0, 0, 255, 255]);
    }

    #[test]
    fn a_pin_turns_its_part() {
        // A long bar across the picture, one pin in its middle turned a
        // quarter: the bar stands up.
        let mut source = vec![0_u8; 64 * 64 * 4];
        for y in 30..34 {
            for x in 8..56 {
                source[(y * 64 + x) * 4..][..4].copy_from_slice(&[0, 0, 0, 255]);
            }
        }
        let mut pin = Pin::at([32.0, 32.0]);
        pin.angle = std::f32::consts::FRAC_PI_2;
        let settings = PuppetSettings::default();
        let turned = puppet(&source, 64, 64, &[pin], &settings);
        assert_eq!(pixel(&turned, 32, 12)[3], 255, "the bar stands up");
        assert_eq!(pixel(&turned, 12, 32)[3], 0, "and left across");
        // With Correct pin rotation off, an unturned pin holds its angle;
        // still it changes nothing on its own.
        let held = puppet(
            &source,
            64,
            64,
            &[Pin::at([32.0, 32.0])],
            &PuppetSettings {
                correct_pin_rotation: false,
                ..settings
            },
        );
        assert_eq!(held, source);
        // Nearest pixel sampling keeps hard colors.
        let hard = puppet(
            &two_squares(),
            64,
            64,
            &[Pin::at([48.0, 48.0]).dragged([48.5, 28.3])],
            &PuppetSettings {
                interpolation: Interpolation::Nearest,
                ..settings
            },
        );
        assert!(hard.chunks(4).all(|pixel| pixel[3] == 0 || pixel[3] == 255));
    }

    #[test]
    fn pins_are_picked_one_by_one_or_together() {
        let mut pins = vec![
            Pin::at([0.0, 0.0]),
            Pin::at([10.0, 0.0]),
            Pin::at([20.0, 0.0]),
        ];
        pick(&mut pins, 0, false);
        pick(&mut pins, 2, true);
        let picked: Vec<bool> = pins.iter().map(|pin| pin.picked).collect();
        assert_eq!(picked, [true, false, true], "Shift adds");
        pick(&mut pins, 2, true);
        assert!(!pins[2].picked, "Shift on a picked one takes it out");
        pick(&mut pins, 1, false);
        assert_eq!(
            pins.iter().filter(|pin| pin.picked).count(),
            1,
            "a plain click picks one"
        );
        pick_inside(&mut pins, [5.0, -1.0, 25.0, 1.0], false);
        let picked: Vec<bool> = pins.iter().map(|pin| pin.picked).collect();
        assert_eq!(picked, [false, true, true]);
        move_picked(&mut pins, [0.0, 5.0]);
        assert_eq!(pins[0].to, [0.0, 0.0]);
        assert_eq!(pins[1].to, [10.0, 5.0]);
        turn_picked(&mut pins, [10.0, 5.0], std::f32::consts::FRAC_PI_2);
        assert!(close(pins[1].to, [10.0, 5.0]));
        assert!(close(pins[2].to, [10.0, 15.0]));
        assert!((pins[2].angle - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert_eq!(pins[0].angle, 0.0, "only the picked turn");
    }
}
