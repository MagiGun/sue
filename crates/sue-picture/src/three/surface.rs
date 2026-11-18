//! Smooth surfaces for the 3D renderer: a mesh with a normal at every
//! point, drawn with soft light, soft edges, guide lines lying on it, fine
//! relief (the veins and tendons of a hand) and a shadow on the floor.
//! Drawing figures and imported models are drawn this way; Sue Paint's
//! plain shapes keep the flat renderer in the parent module.

use std::sync::{Arc, Mutex, PoisonError};

use super::{Buffers, Camera, ShadowMap, View3D, cross, dot, normalized, sub};

/// Fine relief over part of a surface: heights over a rectangle of its
/// own two axes, in the surface's units.
#[derive(Clone, Debug, PartialEq)]
pub struct Relief {
    /// Cells across and up.
    pub size: [usize; 2],
    /// How large one cell is, in the surface's units.
    pub cell: f32,
    /// The height of each cell, rows from the bottom.
    pub heights: Vec<f32>,
}

impl Relief {
    /// The slope at `(u, v)` (0 to 1 across and up): how fast the height
    /// rises along each axis.
    #[must_use]
    pub fn slope(&self, u: f32, v: f32) -> [f32; 2] {
        let [w, h] = self.size;
        if w < 3 || h < 3 {
            return [0.0; 2];
        }
        let x = (u * (w - 1) as f32).clamp(1.0, (w - 2) as f32);
        let y = (v * (h - 1) as f32).clamp(1.0, (h - 2) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let at = |x: usize, y: usize| self.heights[y.min(h - 1) * w + x.min(w - 1)];
        let grad = |x: usize, y: usize| {
            [
                (at(x + 1, y) - at(x.saturating_sub(1), y)) / (2.0 * self.cell),
                (at(x, y + 1) - at(x, y.saturating_sub(1))) / (2.0 * self.cell),
            ]
        };
        let (a, b, c, d) = (
            grad(x0, y0),
            grad(x0 + 1, y0),
            grad(x0, y0 + 1),
            grad(x0 + 1, y0 + 1),
        );
        [0, 1].map(|k| {
            (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy
        })
    }
}

/// A mesh the smooth renderer draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Surface {
    /// Its points.
    pub positions: Vec<[f32; 3]>,
    /// The normal at each point.
    pub normals: Vec<[f32; 3]>,
    /// Its triangles.
    pub indices: Vec<u32>,
    /// Lines lying on the surface, each from one point to another.
    pub guides: Vec<[[f32; 3]; 2]>,
    /// Where each point is on the relief: across and up (0 to 1), how
    /// much of the relief it takes (0 to 1) and which of [`Self::axes`]
    /// it lies along. Empty without relief.
    pub detail: Vec<[f32; 4]>,
    /// The relief's two axes on the posed surface, one pair for each
    /// part that carries it (the left hand, the right hand).
    pub axes: Vec<[[f32; 3]; 2]>,
    /// The relief itself.
    pub relief: Option<Arc<Relief>>,
    /// Each point's own color, red, green and blue from 0 to 1 (empty
    /// for plain clay).
    pub colors: Vec<[f32; 3]>,
    /// Where each point is on the picture laid over the surface, across
    /// and down from 0 to 1 (empty without one).
    pub uvs: Vec<[f32; 2]>,
    /// For each triangle, which of [`Self::textures`] is laid over it
    /// (`u32::MAX` for none; empty when no triangle has one).
    pub skins: Vec<u32>,
    /// The pictures laid over the surface.
    pub textures: Vec<Arc<Texture>>,
}

/// A picture laid over a surface.
#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    /// Width and height.
    pub size: [u32; 2],
    /// Red, green, blue and alpha for each pixel, rows from the top.
    pub rgba: Vec<u8>,
}

impl Texture {
    /// The color at `(u, v)` (across and down; the picture repeats
    /// beyond 0 to 1), blended between its nearest pixels.
    #[must_use]
    pub fn at(&self, u: f32, v: f32) -> [f32; 3] {
        let [w, h] = self.size.map(|value| value as usize);
        if w == 0 || h == 0 || self.rgba.len() < w * h * 4 {
            return [1.0; 3];
        }
        let x = (u - u.floor()) * w as f32 - 0.5;
        let y = (v - v.floor()) * h as f32 - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let pixel = |x: f32, y: f32| {
            let x = (x as i64).rem_euclid(w as i64) as usize;
            let y = (y as i64).rem_euclid(h as i64) as usize;
            let at = (y * w + x) * 4;
            [0, 1, 2].map(|k| f32::from(self.rgba[at + k]) / 255.0)
        };
        let (a, b, c, d) = (
            pixel(x0, y0),
            pixel(x0 + 1.0, y0),
            pixel(x0, y0 + 1.0),
            pixel(x0 + 1.0, y0 + 1.0),
        );
        [0, 1, 2].map(|k| {
            (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy
        })
    }
}

impl Surface {
    /// A surface of `positions` and `indices` with smooth normals: each
    /// point's is the mean of its triangles', larger triangles counting
    /// more.
    #[must_use]
    pub fn smooth(positions: Vec<[f32; 3]>, indices: Vec<u32>) -> Self {
        let normals = smooth_normals(&positions, &indices);
        Self {
            positions,
            normals,
            indices,
            ..Self::default()
        }
    }

    /// A surface with a fold kept wherever two faces meet at more than
    /// `crease` degrees: points are split there, so a box keeps its
    /// edges and a ball is round.
    #[must_use]
    pub fn creased(positions: &[[f32; 3]], indices: &[u32], crease: f32) -> Self {
        let fold = crease.to_radians().cos();
        let faces: Vec<[f32; 3]> = indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| positions[i as usize]);
                cross(sub(b, a), sub(c, a))
            })
            .collect();
        // The faces round each point.
        let mut around: Vec<Vec<u32>> = vec![Vec::new(); positions.len()];
        for (face, t) in indices.as_chunks::<3>().0.iter().enumerate() {
            for &i in t {
                around[i as usize].push(face as u32);
            }
        }
        let mut out = Self::default();
        for (face, t) in indices.as_chunks::<3>().0.iter().enumerate() {
            let mine = normalized(faces[face]);
            for &i in t {
                let mut normal = [0.0_f32; 3];
                for &other in &around[i as usize] {
                    let theirs = faces[other as usize];
                    if dot(mine, normalized(theirs)) >= fold {
                        for axis in 0..3 {
                            normal[axis] += theirs[axis];
                        }
                    }
                }
                out.indices.push(out.positions.len() as u32);
                out.positions.push(positions[i as usize]);
                out.normals.push(normalized(normal));
            }
        }
        out
    }

    /// The middle and largest extent of the surface.
    #[must_use]
    pub fn frame(&self) -> ([f32; 3], f32) {
        frame_of_points(&self.positions)
    }
}

/// The middle and largest extent of `points` (a box 2 wide when empty).
#[must_use]
pub fn frame_of_points(points: &[[f32; 3]]) -> ([f32; 3], f32) {
    let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in points {
        for axis in 0..3 {
            min[axis] = min[axis].min(p[axis]);
            max[axis] = max[axis].max(p[axis]);
        }
    }
    if !min[0].is_finite() {
        return ([0.0; 3], 2.0);
    }
    let middle = [0, 1, 2].map(|axis| (min[axis] + max[axis]) / 2.0);
    let extent = (0..3)
        .map(|axis| max[axis] - min[axis])
        .fold(0.0_f32, f32::max);
    (middle, extent)
}

/// The normal at each of `positions`: the mean of its triangles', larger
/// triangles counting more.
#[must_use]
pub fn smooth_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut normals = vec![[0.0_f32; 3]; positions.len()];
    for t in indices.as_chunks::<3>().0 {
        let [a, b, c] = t.map(|i| positions[i as usize]);
        let face = cross(sub(b, a), sub(c, a));
        for &i in t {
            for axis in 0..3 {
                normals[i as usize][axis] += face[axis];
            }
        }
    }
    for normal in &mut normals {
        *normal = normalized(*normal);
    }
    normals
}

/// The direction the light of `view` comes from.
#[must_use]
pub fn light_of(view: &View3D) -> [f32; 3] {
    let angle = (view.light + if view.follow_light { view.yaw } else { 0.0 }).to_radians();
    let height = view.light_height.to_radians();
    normalized([
        angle.sin() * height.cos(),
        height.sin(),
        angle.cos() * height.cos(),
    ])
}

/// Runs `work(first_row, band)` on `data` (`width` values a row) cut
/// into bands of rows, on the shared workers (the compositor's), each
/// worker taking the next band not yet begun so uneven bands even out.
pub(crate) fn in_bands<T: Send>(
    data: &mut [T],
    width: usize,
    work: impl Fn(usize, &mut [T]) + Sync,
) {
    if data.is_empty() || width == 0 {
        return;
    }
    let height = data.len() / width;
    // Enough bands that every worker has several to take.
    let rows = height
        .div_ceil(crate::composite::workers().max(1) * 8)
        .max(4);
    let bands: Vec<Mutex<&mut [T]>> = data.chunks_mut(width * rows).map(Mutex::new).collect();
    let _ = crate::composite::each_index(bands.len(), |index| {
        let mut band = bands[index].lock().unwrap_or_else(PoisonError::into_inner);
        work(index * rows, &mut band);
    });
}

/// A 5 by 5 box blur of `values` (`width` across), each value the mean
/// of those round it that lie inside, worked as a pass across and a pass
/// down, both in bands on the shared workers.
fn box_blur(values: &[f32], width: usize) -> Vec<f32> {
    if values.is_empty() || width == 0 {
        return values.to_vec();
    }
    let height = values.len() / width;
    let mut across = vec![0.0_f32; values.len()];
    in_bands(&mut across, width, |first, band| {
        for (row, line) in band.chunks_mut(width).enumerate() {
            let source = &values[(first + row) * width..(first + row + 1) * width];
            for (x, out) in line.iter_mut().enumerate() {
                let (from, to) = (x.saturating_sub(2), (x + 3).min(width));
                *out = source[from..to].iter().sum::<f32>();
            }
        }
    });
    let mut out = vec![0.0_f32; values.len()];
    in_bands(&mut out, width, |first, band| {
        for (row, line) in band.chunks_mut(width).enumerate() {
            let y = first + row;
            let (from, to) = (y.saturating_sub(2), (y + 3).min(height));
            for (x, value) in line.iter_mut().enumerate() {
                let count = ((x + 3).min(width) - x.saturating_sub(2)) * (to - from);
                let sum: f32 = (from..to).map(|sy| across[sy * width + x]).sum();
                *value = sum / count as f32;
            }
        }
    });
    out
}

/// A triangle as it lies on the finer picture: its corners there (and
/// their depths), its twice signed area, and the rows and columns it
/// may cover.
#[derive(Clone, Copy)]
struct Placed {
    corners: [[f32; 3]; 3],
    area: f32,
    rows: [usize; 2],
    columns: [usize; 2],
}

/// One band of the canvas's rows of [`Buffers`], for one worker to fill.
struct Rows<'a> {
    first: usize,
    depth: &'a mut [f32],
    normal: &'a mut [[f32; 3]],
    light: &'a mut [f32],
    fog: &'a mut [f32],
    cover: &'a mut [f32],
    color: &'a mut [[f32; 3]],
    guide: &'a mut [f32],
}

/// One sample of the finer picture the surface is drawn into before it
/// is brought down to the canvas.
#[derive(Clone, Copy)]
struct Sample {
    depth: f32,
    normal: [f32; 3],
    light: f32,
    fog: f32,
    color: [f32; 3],
}

/// `surface` as `view` sees it on a `size` canvas, framed by `frame`:
/// every pixel's depth, normal and light, how much of the pixel the
/// surface covers (so its edge is soft), its guide lines and its shadow
/// on the floor. `view.fast` draws without the finer picture, for while
/// something is dragged.
#[allow(clippy::needless_range_loop)]
#[must_use]
pub fn render_surface(
    surface: &Surface,
    view: &View3D,
    size: [u32; 2],
    frame: ([f32; 3], f32),
) -> Buffers {
    let (w, h) = (size[0] as usize, size[1] as usize);
    let mut buffers = Buffers {
        size,
        depth: vec![f32::INFINITY; w * h],
        normal: vec![[0.0; 3]; w * h],
        light: vec![0.0; w * h],
        fog: vec![0.0; w * h],
        cover: vec![0.0; w * h],
        guide: Vec::new(),
        floor: Vec::new(),
        color: Vec::new(),
        region: [0; 4],
    };
    if surface.positions.is_empty() || w == 0 || h == 0 {
        return buffers;
    }
    let lens = Camera::new(view, size, frame);
    let light = light_of(view);
    let seen: Vec<([f32; 3], [f32; 3])> = surface.positions.iter().map(|p| lens.see(*p)).collect();
    let screen: Vec<[f32; 3]> = seen
        .iter()
        .map(|(_, c)| {
            if c[2] > 0.05 {
                lens.screen(*c)
            } else {
                [f32::NAN; 3]
            }
        })
        .collect();
    // The part of the canvas the surface can show on.
    let (mut x0, mut y0, mut x1, mut y1) = (w as f32, h as f32, 0.0_f32, 0.0_f32);
    for s in &screen {
        if s[0].is_finite() {
            x0 = x0.min(s[0]);
            y0 = y0.min(s[1]);
            x1 = x1.max(s[0]);
            y1 = y1.max(s[1]);
        }
    }
    let x0 = (x0.floor().max(0.0) as usize).min(w);
    let y0 = (y0.floor().max(0.0) as usize).min(h);
    let x1 = ((x1.ceil() + 1.0).max(0.0) as usize).min(w);
    let y1 = ((y1.ceil() + 1.0).max(0.0) as usize).min(h);
    if x1 <= x0 || y1 <= y0 {
        return buffers;
    }
    if surface.colors.len() == surface.positions.len()
        || (surface.uvs.len() == surface.positions.len() && !surface.textures.is_empty())
    {
        buffers.color = vec![[1.0; 3]; w * h];
    }
    let fine = if view.fast { 1 } else { 2 };
    let (tw, th) = ((x1 - x0) * fine, (y1 - y0) * fine);
    // Its own colors, when it has any: a color at each point, pictures
    // laid over its triangles, or both.
    let tinted = surface.colors.len() == surface.positions.len();
    let pictured = surface.uvs.len() == surface.positions.len() && !surface.textures.is_empty();
    let colored = tinted || pictured;
    let (low, high) = seen
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), (p, _)| {
            (low.min(p[1]), high.max(p[1]))
        });
    let fog_top = low + (high - low) * view.fog_height;
    let fog_at = |y: f32| {
        if view.fog <= 0.0 || fog_top <= low {
            return 0.0;
        }
        (view.fog * ((fog_top - y) / (fog_top - low)).clamp(0.0, 1.0)).clamp(0.0, 1.0)
    };
    let worlds: Vec<[f32; 3]> = seen.iter().map(|(p, _)| *p).collect();
    let shadow = (view.shadows || view.floor)
        .then(|| ShadowMap::from_points(&worlds, &surface.indices, light));
    let f = fine as f32;
    let to_tile = |s: [f32; 3]| [(s[0] - x0 as f32) * f, (s[1] - y0 as f32) * f, s[2]];
    let relief = surface
        .relief
        .as_deref()
        .filter(|_| !surface.detail.is_empty());
    // Where each triangle lies on the finer picture.
    let triangles = surface.indices.as_chunks::<3>().0;
    let placed: Vec<Option<Placed>> = triangles
        .iter()
        .map(|t| {
            let [ia, ib, ic] = t.map(|i| i as usize);
            if !(screen[ia][0].is_finite()
                && screen[ib][0].is_finite()
                && screen[ic][0].is_finite())
            {
                return None;
            }
            let [pa, pb, pc] = [ia, ib, ic].map(|i| to_tile(screen[i]));
            let area = (pb[0] - pa[0]) * (pc[1] - pa[1]) - (pb[1] - pa[1]) * (pc[0] - pa[0]);
            if area.abs() < 1e-6 {
                return None;
            }
            let bx0 = pa[0].min(pb[0]).min(pc[0]).floor().max(0.0) as usize;
            let bx1 = (pa[0].max(pb[0]).max(pc[0]).ceil().max(0.0) as usize).min(tw);
            let by0 = pa[1].min(pb[1]).min(pc[1]).floor().max(0.0) as usize;
            let by1 = (pa[1].max(pb[1]).max(pc[1]).ceil().max(0.0) as usize).min(th);
            (bx0 < bx1 && by0 < by1).then_some(Placed {
                corners: [pa, pb, pc],
                area,
                rows: [by0, by1],
                columns: [bx0, bx1],
            })
        })
        .collect();
    // Where sample `(x, y)` lies in triangle `on`: its three screen
    // weights, or `None` outside it.
    let weights = |on: &Placed, x: usize, y: usize| {
        let [pa, pb, pc] = on.corners;
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        let wa = ((pb[0] - px) * (pc[1] - py) - (pb[1] - py) * (pc[0] - px)) / on.area;
        let wb = ((pc[0] - px) * (pa[1] - py) - (pc[1] - py) * (pa[0] - px)) / on.area;
        let wc = 1.0 - wa - wb;
        (wa >= 0.0 && wb >= 0.0 && wc >= 0.0).then_some([wa, wb, wc])
    };
    // Guide lines on the finer picture, each from end to end with its
    // depth at both.
    let half = 0.55 * f;
    let segments: Vec<([f32; 3], [f32; 3])> = surface
        .guides
        .iter()
        .filter_map(|[a, b]| {
            let (ca, cb) = (lens.see(*a).1, lens.see(*b).1);
            (ca[2] > 0.05 && cb[2] > 0.05)
                .then(|| (to_tile(lens.screen(ca)), to_tile(lens.screen(cb))))
        })
        .collect();
    let guided = !surface.guides.is_empty();
    if guided {
        buffers.guide = vec![0.0; w * h];
    }
    // Band by band of the canvas's rows on the shared workers, each band
    // its own rows of the finer picture: first which triangle is
    // nearest at each sample (every triangle in order, as one pass
    // would), then that one sample shaded once, the guide lines laid on
    // it, and each canvas pixel the mean of its samples.
    let share = 1.0 / (fine * fine) as f32;
    let (from, to) = (y0 * w, y1 * w);
    let mut rows: Vec<Mutex<Rows<'_>>> = Vec::new();
    {
        let per = (y1 - y0)
            .div_ceil(crate::composite::workers().max(1) * 8)
            .max(2)
            * w;
        let mut colors = buffers.color.get_mut(from..to).map(|c| c.chunks_mut(per));
        let mut guides = buffers.guide.get_mut(from..to).map(|g| g.chunks_mut(per));
        for (index, ((((depth, normal), light), fog), cover)) in buffers.depth[from..to]
            .chunks_mut(per)
            .zip(buffers.normal[from..to].chunks_mut(per))
            .zip(buffers.light[from..to].chunks_mut(per))
            .zip(buffers.fog[from..to].chunks_mut(per))
            .zip(buffers.cover[from..to].chunks_mut(per))
            .enumerate()
        {
            rows.push(Mutex::new(Rows {
                first: y0 + index * (per / w),
                depth,
                normal,
                light,
                fog,
                cover,
                color: colors.as_mut().and_then(Iterator::next).unwrap_or_default(),
                guide: guides.as_mut().and_then(Iterator::next).unwrap_or_default(),
            }));
        }
    }
    let _ = crate::composite::each_index(rows.len(), |index| {
        let mut out = rows[index].lock().unwrap_or_else(PoisonError::into_inner);
        let out = &mut *out;
        let count = out.depth.len() / w;
        // This band's rows of the finer picture.
        let top = (out.first - y0) * fine;
        let last = top + count * fine;
        let empty = Sample {
            depth: f32::INFINITY,
            normal: [0.0; 3],
            light: 0.0,
            fog: 0.0,
            color: [1.0; 3],
        };
        let mut samples = vec![empty; count * fine * tw];
        let mut nearest = vec![u32::MAX; samples.len()];
        for (face, on) in placed.iter().enumerate() {
            let Some(on) = on else { continue };
            if on.rows[1] <= top || on.rows[0] >= last {
                continue;
            }
            let [pa, pb, pc] = on.corners;
            for y in on.rows[0].max(top)..on.rows[1].min(last) {
                for x in on.columns[0]..on.columns[1] {
                    let Some([wa, wb, wc]) = weights(on, x, y) else {
                        continue;
                    };
                    let (ka, kb, kc) = (wa / pa[2], wb / pb[2], wc / pc[2]);
                    let depth = 1.0 / (ka + kb + kc);
                    let at = (y - top) * tw + x;
                    if depth >= samples[at].depth {
                        continue;
                    }
                    samples[at].depth = depth;
                    nearest[at] = face as u32;
                }
            }
        }
        for (at, face) in nearest.iter().enumerate() {
            let face = *face as usize;
            let Some(Some(on)) = placed.get(face) else {
                continue;
            };
            let (x, y) = (at % tw, top + at / tw);
            let Some([wa, wb, wc]) = weights(on, x, y) else {
                continue;
            };
            let [pa, pb, pc] = on.corners;
            let [ia, ib, ic] = triangles[face].map(|i| i as usize);
            let (ka, kb, kc) = (wa / pa[2], wb / pb[2], wc / pc[2]);
            let depth = 1.0 / (ka + kb + kc);
            let (ka, kb, kc) = (ka * depth, kb * depth, kc * depth);
            let mix = |a: [f32; 3], b: [f32; 3], c: [f32; 3]| {
                [0, 1, 2].map(|axis| ka * a[axis] + kb * b[axis] + kc * c[axis])
            };
            let world = mix(worlds[ia], worlds[ib], worlds[ic]);
            let [na, nb, nc] = [ia, ib, ic].map(|i| surface.normals[i]);
            let mut normal = normalized(mix(na, nb, nc));
            if let Some(relief) = relief {
                let [da, db, dc] = [ia, ib, ic].map(|i| surface.detail[i]);
                let amount = ka * da[2] + kb * db[2] + kc * dc[2];
                if amount > 0.01
                    && let Some([across, up]) = surface.axes.get(da[3] as usize)
                {
                    let u = ka * da[0] + kb * db[0] + kc * dc[0];
                    let v = ka * da[1] + kb * db[1] + kc * dc[1];
                    let [su, sv] = relief.slope(u, v);
                    normal =
                        normalized([0, 1, 2].map(|axis| {
                            normal[axis] - amount * (su * across[axis] + sv * up[axis])
                        }));
                }
            }
            // Toward the camera, whichever way the face winds.
            let toward = normalized(sub(lens.eye, world));
            let mut facing_eye = dot(normal, toward);
            if facing_eye < 0.0 {
                normal = normal.map(|value| -value);
                facing_eye = -facing_eye;
            }
            let sunlit = if view.shadows {
                shadow.as_ref().map_or(1.0, |shadow| shadow.lit(world))
            } else {
                1.0
            };
            // A soft key light that wraps a little round the form, a
            // faint fill from the camera, and edges that turn away a
            // touch darker, as a clay figure under a studio lamp.
            let wrap = 0.25;
            let key = ((dot(normal, light) + wrap) / (1.0 + wrap)).max(0.0);
            let fill = 0.12 * facing_eye;
            let rim = 0.86 + 0.14 * facing_eye.sqrt();
            let lit = (view.ambient
                + (1.0 - view.ambient) * view.intensity * (key * sunlit * 0.92 + fill))
                * rim;
            let mut color = if tinted {
                mix(surface.colors[ia], surface.colors[ib], surface.colors[ic])
            } else {
                [1.0; 3]
            };
            if pictured
                && let Some(texture) = surface
                    .skins
                    .get(face)
                    .or(surface.skins.is_empty().then_some(&0))
                    .and_then(|skin| surface.textures.get(*skin as usize))
            {
                let [ua, ub, uc] = [ia, ib, ic].map(|i| surface.uvs[i]);
                let u = ka * ua[0] + kb * ub[0] + kc * uc[0];
                let v = ka * ua[1] + kb * ub[1] + kc * uc[1];
                let seen = texture.at(u, v);
                color = [0, 1, 2].map(|k| color[k] * seen[k]);
            }
            samples[at] = Sample {
                depth,
                normal,
                light: lit.clamp(0.0, 1.0),
                fog: fog_at(world[1]),
                color,
            };
        }
        // Guide lines, lying on the surface: drawn where nothing nearer
        // hides them.
        let mut guide = vec![0.0_f32; if guided { samples.len() } else { 0 }];
        for (sa, sb) in &segments {
            let (sa, sb) = (*sa, *sb);
            let gx0 = (sa[0].min(sb[0]) - half - 1.0).floor().max(0.0) as usize;
            let gx1 = ((sa[0].max(sb[0]) + half + 2.0).max(0.0) as usize).min(tw);
            let gy0 = (sa[1].min(sb[1]) - half - 1.0).floor().max(0.0) as usize;
            let gy1 = ((sa[1].max(sb[1]) + half + 2.0).max(0.0) as usize).min(th);
            let (gy0, gy1) = (gy0.max(top), gy1.min(last));
            if gy0 >= gy1 {
                continue;
            }
            let (dx, dy) = (sb[0] - sa[0], sb[1] - sa[1]);
            let length = (dx * dx + dy * dy).max(1e-6);
            for y in gy0..gy1 {
                for x in gx0..gx1 {
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let along = (((px - sa[0]) * dx + (py - sa[1]) * dy) / length).clamp(0.0, 1.0);
                    let (qx, qy) = (sa[0] + dx * along, sa[1] + dy * along);
                    let distance = ((px - qx).powi(2) + (py - qy).powi(2)).sqrt();
                    let ink = (half + 0.5 - distance).clamp(0.0, 1.0);
                    if ink <= 0.0 {
                        continue;
                    }
                    let depth = 1.0 / ((1.0 - along) / sa[2] + along / sb[2]);
                    let at = (y - top) * tw + x;
                    if depth <= samples[at].depth * 1.012 + 0.004 && samples[at].depth.is_finite() {
                        guide[at] = guide[at].max(ink);
                    }
                }
            }
        }
        // Down to the canvas: each pixel the mean of its samples.
        for row in 0..count {
            for x in x0..x1 {
                let (mut depth, mut normal, mut lit, mut fog, mut hits, mut line) =
                    (f32::INFINITY, [0.0_f32; 3], 0.0, 0.0, 0.0_f32, 0.0);
                let mut color = [0.0_f32; 3];
                for sy in 0..fine {
                    for sx in 0..fine {
                        let at = (row * fine + sy) * tw + (x - x0) * fine + sx;
                        let sample = &samples[at];
                        if sample.depth.is_finite() {
                            depth = depth.min(sample.depth);
                            for axis in 0..3 {
                                normal[axis] += sample.normal[axis];
                            }
                            lit += sample.light;
                            fog += sample.fog;
                            for k in 0..3 {
                                color[k] += sample.color[k];
                            }
                            hits += 1.0;
                        }
                        if guided {
                            line += guide[at];
                        }
                    }
                }
                if hits > 0.0 {
                    let to = row * w + x;
                    out.depth[to] = depth;
                    out.normal[to] = normalized(normal);
                    out.light[to] = lit / hits;
                    out.fog[to] = fog / hits;
                    out.cover[to] = hits * share;
                    if colored {
                        out.color[to] = color.map(|value| value / hits);
                    }
                    if guided {
                        out.guide[to] = line * share;
                    }
                }
            }
        }
    });
    drop(rows);
    if !view.fast {
        occlude(&mut buffers, &lens, [x0, y0, x1, y1]);
    }
    buffers.region = [x0, y0, x1, y1].map(|value| value as u32);
    if view.floor
        && let Some(shadow) = &shadow
    {
        buffers.floor = floor_shadow(&lens, shadow, low, [w, h], &buffers);
        buffers.region = [0, 0, size[0], size[1]];
    }
    buffers
}

/// Darkens what sits in a hollow: where the surface round a pixel comes
/// nearer the camera than the pixel itself (between the fingers, under
/// the chin, where an arm lies against the body).
fn occlude(buffers: &mut Buffers, lens: &Camera, [x0, y0, x1, y1]: [usize; 4]) {
    let w = buffers.size[0] as usize;
    let h = buffers.size[1] as usize;
    // The 24 samples round each pixel: eight directions, three rings
    // out, each ring turned a little so the samples do not line up into
    // bands. Their turns (before each pixel's own) are worked once.
    let turns: Vec<([f32; 2], f32)> = (0..8)
        .flat_map(|k| {
            [0.3_f32, 0.65, 1.0]
                .into_iter()
                .enumerate()
                .map(move |(ring, step)| {
                    let (sin, cos) = ((k as f32 + ring as f32 * 0.37) * 0.785).sin_cos();
                    ([cos, sin], step)
                })
        })
        .collect();
    let reach = 0.05_f32;
    let span = x1 - x0;
    let mut shade = vec![1.0_f32; span * (y1 - y0)];
    let depths = &buffers.depth;
    in_bands(&mut shade, span, |first, band| {
        for (at, value) in band.iter_mut().enumerate() {
            let (x, y) = (x0 + at % span, y0 + first + at / span);
            let depth = depths[y * w + x];
            if !depth.is_finite() {
                continue;
            }
            let radius = (reach * lens.focal / depth).clamp(2.0, 48.0);
            let mut hidden = 0.0;
            // The pixel's own turn, added to each sample's.
            let (sin, cos) = ((x + y * 3) as f32 * 0.61 * 0.785).sin_cos();
            for &([c, s], step) in &turns {
                {
                    let (tc, ts) = (c * cos - s * sin, s * cos + c * sin);
                    let sx = (x as f32 + tc * radius * step).round();
                    let sy = (y as f32 + ts * radius * step).round();
                    if sx < 0.0 || sy < 0.0 || sx >= w as f32 || sy >= h as f32 {
                        continue;
                    }
                    let other = depths[sy as usize * w + sx as usize];
                    if !other.is_finite() {
                        continue;
                    }
                    let nearer = depth - other;
                    // Nearer, but not so much nearer that it is another
                    // thing altogether in front.
                    let rises = ((nearer - reach * 0.08 * step) / (reach * 0.5)).clamp(0.0, 1.0);
                    hidden += rises * (1.0 - (nearer / (reach * 3.0)).min(1.0)) / 24.0;
                }
            }
            *value = 1.0 - 0.9 * hidden.min(1.0);
        }
    });
    // Smoothed, so the shade lies soft and shows no grain.
    let shade = box_blur(&box_blur(&shade, span), span);
    for y in y0..y1 {
        for x in x0..x1 {
            buffers.light[y * w + x] *= shade[(y - y0) * span + (x - x0)];
        }
    }
}

/// How dark the model's shadow lies on the floor (the height of its
/// lowest point) at each pixel the model does not cover, softened.
fn floor_shadow(
    lens: &Camera,
    shadow: &ShadowMap,
    floor: f32,
    [w, h]: [usize; 2],
    buffers: &Buffers,
) -> Vec<f32> {
    let mut dark = vec![0.0_f32; w * h];
    in_bands(&mut dark, w, |first, band| {
        for (at, value) in band.iter_mut().enumerate() {
            let (x, y) = (at % w, first + at / w);
            // The ray through the pixel, to the floor.
            let (cx, cy) = (
                (x as f32 + 0.5 - lens.size[0] / 2.0) / lens.focal,
                -(y as f32 + 0.5 - lens.size[1] / 2.0) / lens.focal,
            );
            let ray = [0, 1, 2]
                .map(|axis| lens.forward[axis] + lens.right[axis] * cx + lens.up[axis] * cy);
            if ray[1].abs() < 1e-6 {
                continue;
            }
            let t = (floor - lens.eye[1]) / ray[1];
            if t <= 0.0 {
                continue;
            }
            let hit = [0, 1, 2].map(|axis| lens.eye[axis] + ray[axis] * t);
            let depth = t * dot(ray, lens.forward);
            if depth < buffers.depth[y * w + x] {
                *value = 1.0 - shadow.lit([hit[0], hit[1] + 0.002, hit[2]]);
            }
        }
    });
    // Softened a little, so the shadow's edge is not a stair.
    box_blur(&dark, w)
}
