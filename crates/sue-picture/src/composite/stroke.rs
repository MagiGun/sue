//! Compositing while a stroke paints one layer: the stacks under and over
//! it kept per tile ([`Focus`]), one sample a block at small zooms, and
//! the tiles left waiting brought to full detail.

use sue_core::{Arena, EntityId};

use super::gather::{Layer, Source, gather_view, prepare};
use super::rows::{Stack, composite_span, fold, fold_plain, pixel, rows, shown, to_byte, unit};
use super::{Compositor, RenderOptions, next_version, workers};

/// Side of a [`Focus`] tile.
const FOCUS_TILE: u32 = 64;
/// The most tiles a [`Focus`] keeps (about 70 MB).
const FOCUS_TILES: usize = 2048;

/// While a stroke paints one layer, everything under it and over it stays
/// as it is: the stack under the layer is kept per tile, and so is the
/// stack over it, flattened, when its layers are plain Normal ones. A
/// pixel of the screen is then three layers' work, not every layer's, as
/// painting programs do with their layer caches.
#[derive(Debug)]
pub(super) struct Focus {
    layer: EntityId,
    /// The picture's signature apart from the layer (and its mask): the
    /// cache holds while it does.
    others: Vec<(EntityId, u64)>,
    /// Per tile, each pixel's [`Stack`] under the layer: color and base
    /// alpha as bytes, and whether the base shows.
    below: std::collections::HashMap<(u32, u32), Box<[[u8; 6]]>>,
    /// Per tile, the stack over the layer flattened (straight RGBA), when
    /// that stack is plain.
    above: std::collections::HashMap<(u32, u32), Box<[[u8; 4]]>>,
}

/// `true` when the layers over the focused one can be flattened ahead:
/// Normal blends only, no correction, no border, and the first of them
/// not clipped onto the focused layer.
fn plain_above(layers: &[Layer<'_>]) -> bool {
    layers.first().is_none_or(|layer| !layer.style.clip)
        && layers.iter().all(|layer| {
            !matches!(layer.source, Source::Correction(_))
                && layer.style.blend == sue_model::BlendMode::Normal
                && !layer.effects.needs_area()
        })
}

/// The [`Focus`] tiles `wanted`, made on every core: each pixel's stack
/// under layer `index`, and the layers over it flattened when `flat`.
#[allow(clippy::type_complexity)]
fn focus_tiles(
    layers: &[Layer<'_>],
    index: usize,
    flat: bool,
    wanted: &[(u32, u32)],
    size: [u32; 2],
) -> Vec<((u32, u32), Box<[[u8; 6]]>, Option<Box<[[u8; 4]]>>)> {
    let make = |&(tx, ty): &(u32, u32)| {
        let area = (FOCUS_TILE * FOCUS_TILE) as usize;
        let mut below = vec![[0_u8; 6]; area];
        let mut above = flat.then(|| vec![[0_u8; 4]; area]);
        for y in ty * FOCUS_TILE..((ty + 1) * FOCUS_TILE).min(size[1]) {
            for x in tx * FOCUS_TILE..((tx + 1) * FOCUS_TILE).min(size[0]) {
                let at = ((y % FOCUS_TILE) * FOCUS_TILE + x % FOCUS_TILE) as usize;
                let mut stack = Stack::EMPTY;
                fold(&layers[..index], x, y, &mut stack);
                let [r, g, b, a] = stack.color.map(to_byte);
                below[at] = [
                    r,
                    g,
                    b,
                    a,
                    to_byte(stack.base_alpha),
                    u8::from(stack.base_visible),
                ];
                if let Some(above) = &mut above {
                    let mut over = Stack::EMPTY;
                    fold(&layers[index + 1..], x, y, &mut over);
                    above[at] = over.color.map(to_byte);
                }
            }
        }
        (
            (tx, ty),
            below.into_boxed_slice(),
            above.map(Vec::into_boxed_slice),
        )
    };
    let threads = if wanted.len() < 4 {
        1
    } else {
        workers().min(wanted.len())
    };
    if threads <= 1 {
        return wanted.iter().map(make).collect();
    }
    let per = wanted.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let workers: Vec<_> = wanted
            .chunks(per)
            .map(|chunk| scope.spawn(move || chunk.iter().map(make).collect::<Vec<_>>()))
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap_or_default())
            .collect()
    })
}

/// The color at `(x, y)` through a [`Focus`]: the kept stack under layer
/// `index` (`below`, the pixel's tile; its layers when not kept), the layer
/// itself, then the kept flattened stack over it (`above`; the layers over
/// it when that is not kept).
fn focused_color(
    layers: &[Layer<'_>],
    below: Option<&[[u8; 6]]>,
    above: Option<&[[u8; 4]]>,
    index: usize,
    x: u32,
    y: u32,
) -> [f32; 4] {
    let unit = |byte: u8| f32::from(byte) / 255.0;
    let at = ((y % FOCUS_TILE) * FOCUS_TILE + x % FOCUS_TILE) as usize;
    let mut stack = match below {
        Some(below) => {
            let [r, g, b, a, base, shows] = below[at];
            Stack {
                color: [unit(r), unit(g), unit(b), unit(a)],
                base_alpha: unit(base),
                base_visible: shows != 0,
            }
        }
        None => {
            let mut stack = Stack::EMPTY;
            fold(&layers[..index], x, y, &mut stack);
            stack
        }
    };
    fold(&layers[index..=index], x, y, &mut stack);
    match above {
        Some(above) => {
            let over = above[at].map(unit);
            sue_model::BlendMode::Normal.composite(stack.color, over)
        }
        None => {
            fold(&layers[index + 1..], x, y, &mut stack);
            stack.color
        }
    }
}

/// [`composite_span`] through a [`Focus`] (see [`focused_color`]).
#[allow(clippy::too_many_arguments)]
fn focused_span(
    layers: &[Layer<'_>],
    focus: &Focus,
    index: usize,
    flat: bool,
    y: u32,
    x0: u32,
    x1: u32,
    row: &mut [u8],
) {
    let ty = y / FOCUS_TILE;
    // A plain painted layer is read along its row, as the fast path does.
    let plain = layers[index].plain();
    let source = [layers[index].row(y)];
    let mut x = x0;
    while x < x1 {
        let tx = x / FOCUS_TILE;
        let end = x1.min((tx + 1) * FOCUS_TILE);
        let below = focus.below.get(&(tx, ty)).map(AsRef::as_ref);
        let above = flat
            .then(|| focus.above.get(&(tx, ty)))
            .flatten()
            .map(AsRef::as_ref);
        match below.filter(|_| plain) {
            Some(below) => {
                for x in x..end {
                    let at = ((y % FOCUS_TILE) * FOCUS_TILE + x % FOCUS_TILE) as usize;
                    let [r, g, b, a, base, shows] = below[at];
                    let mut stack = Stack {
                        color: [unit(r), unit(g), unit(b), unit(a)],
                        base_alpha: unit(base),
                        base_visible: shows != 0,
                    };
                    fold_plain(&layers[index..=index], &source, x, y, &mut stack);
                    let color = match above {
                        Some(above) => {
                            sue_model::BlendMode::Normal.composite(stack.color, above[at].map(unit))
                        }
                        None => {
                            fold(&layers[index + 1..], x, y, &mut stack);
                            stack.color
                        }
                    };
                    row[x as usize * 4..x as usize * 4 + 4].copy_from_slice(&shown(color, x, y));
                }
            }
            None => {
                for x in x..end {
                    let color = focused_color(layers, below, above, index, x, y);
                    row[x as usize * 4..x as usize * 4 + 4].copy_from_slice(&shown(color, x, y));
                }
            }
        }
        x = end;
    }
}

/// Side of a [`Compositor::refine`] tile.
const PENDING_TILE: u32 = 64;

/// `rect` cut to `clip`, or `None` when they do not meet.
fn meet(rect: [u32; 4], clip: [u32; 4]) -> Option<[u32; 4]> {
    let out = [
        rect[0].max(clip[0]),
        rect[1].max(clip[1]),
        rect[2].min(clip[2]),
        rect[3].min(clip[3]),
    ];
    (out[0] < out[2] && out[1] < out[3]).then_some(out)
}

/// How many picture pixels a side one sample covers while painting at a
/// view of `scale` screen pixels per picture pixel: 1 from half size up,
/// else the largest power of two the screen cannot tell apart (8 at most).
#[must_use]
pub fn view_step(scale: f32) -> u32 {
    let mut step = 1;
    while step < 8 && (step * 2) as f32 * scale <= 1.0 {
        step *= 2;
    }
    step
}

impl Compositor {
    /// The [`Focus`] around `painted` in `layers` (with its index and
    /// whether the stack over it is flattened): the kept one while nothing
    /// else changed (`signature` is the picture's now), else a new one. The
    /// tiles in `wanted` it lacks are made.
    #[allow(clippy::type_complexity)]
    fn kept_focus(
        &mut self,
        arena: &Arena,
        layers: &[Layer<'_>],
        painted: Option<EntityId>,
        signature: &[(EntityId, u64)],
        wanted: &[(u32, u32)],
    ) -> Option<(Focus, usize, bool)> {
        let painted = painted?;
        let index = layers.iter().position(|layer| layer.id == Some(painted))?;
        // A border is drawn for a rectangle at a time: no cache.
        if layers.iter().any(|layer| layer.effects.needs_area()) {
            return None;
        }
        let mask = crate::doc::mask_of(arena, painted);
        let others: Vec<(EntityId, u64)> = signature
            .iter()
            .filter(|(id, _)| *id != painted && Some(*id) != mask)
            .copied()
            .collect();
        let keep = self.focus.take().filter(|focus| {
            focus.layer == painted && focus.others == others && focus.below.len() <= FOCUS_TILES
        });
        let mut focus = keep.unwrap_or(Focus {
            layer: painted,
            others,
            below: std::collections::HashMap::new(),
            above: std::collections::HashMap::new(),
        });
        let flat = plain_above(&layers[index + 1..]);
        let missing: Vec<(u32, u32)> = wanted
            .iter()
            .copied()
            .filter(|tile| !focus.below.contains_key(tile))
            .collect();
        for (key, below, above) in focus_tiles(layers, index, flat, &missing, self.size) {
            focus.below.insert(key, below);
            if let Some(above) = above {
                focus.above.insert(key, above);
            }
        }
        Some((focus, index, flat))
    }

    /// [`Self::refresh_rects`] for a stroke seen at some zoom: of `rects`
    /// only what lies in `visible` is composited now, one sample each
    /// `step` pixels a side (see [`view_step`]), as Krita's instant preview
    /// does for big brushes. The rest, and every coarse tile, waits for
    /// [`Self::refine`] to bring it to full detail.
    pub fn refresh_view(
        &mut self,
        arena: &Arena,
        rects: &[[u32; 4]],
        painted: Option<EntityId>,
        visible: [u32; 4],
        step: u32,
    ) {
        if self.size == [0, 0] {
            self.sync(arena);
            return;
        }
        let step = step.max(1);
        let whole = [0, 0, self.size[0], self.size[1]];
        let mut now = Vec::new();
        for rect in rects {
            let Some(rect) = meet(*rect, whole) else {
                continue;
            };
            for ty in rect[1] / PENDING_TILE..=(rect[3] - 1) / PENDING_TILE {
                for tx in rect[0] / PENDING_TILE..=(rect[2] - 1) / PENDING_TILE {
                    let tile = [
                        tx * PENDING_TILE,
                        ty * PENDING_TILE,
                        (tx + 1) * PENDING_TILE,
                        (ty + 1) * PENDING_TILE,
                    ];
                    let Some(piece) = meet(rect, tile) else {
                        continue;
                    };
                    let seen = meet(piece, visible);
                    if step > 1 || seen != Some(piece) {
                        self.pending.insert((tx, ty));
                        self.pending_layer = painted;
                    }
                }
            }
            if let Some(seen) = meet(rect, visible) {
                now.push(seen);
            }
        }
        if step == 1 {
            if now.is_empty() {
                self.seen = self.signature(arena);
            } else {
                self.refresh_rects(arena, &now, painted);
            }
            return;
        }
        self.coarse(arena, &now, painted, step);
    }

    /// Composites `rects` one sample a `step` by `step` block, each block
    /// filled with its sample.
    fn coarse(&mut self, arena: &Arena, rects: &[[u32; 4]], painted: Option<EntityId>, step: u32) {
        let signature = self.signature(arena);
        if rects.is_empty() {
            self.seen = signature;
            return;
        }
        let bounds = rects
            .iter()
            .fold(rects[0], |[a, b, c, d], [x0, y0, x1, y1]| {
                [a.min(*x0), b.min(*y0), c.max(*x1), d.max(*y1)]
            });
        let mut layers = gather_view(
            arena,
            self.picture,
            RenderOptions::default(),
            self.view.as_ref(),
        );
        prepare(&mut layers, bounds);
        self.attach(arena, &mut layers, false);
        let focus = self.kept_focus(arena, &layers, painted, &signature, &[]);
        // One sample per block, the blocks' rows shared out over the cores.
        let jobs: Vec<([u32; 4], u32)> = rects
            .iter()
            .flat_map(|rect| {
                let first = rect[1] / step * step;
                (first..rect[3])
                    .step_by(step as usize)
                    .map(move |gy| (*rect, gy))
            })
            .collect();
        let sample_row = |&(rect, gy): &([u32; 4], u32)| -> Vec<[u8; 4]> {
            let first = rect[0] / step * step;
            let y = (gy + step / 2).clamp(rect[1], rect[3] - 1);
            (first..rect[2])
                .step_by(step as usize)
                .map(|gx| {
                    let x = (gx + step / 2).clamp(rect[0], rect[2] - 1);
                    let color = match &focus {
                        Some((focus, index, flat)) => {
                            let tile = (x / FOCUS_TILE, y / FOCUS_TILE);
                            let below = focus.below.get(&tile).map(AsRef::as_ref);
                            let above = flat
                                .then(|| focus.above.get(&tile))
                                .flatten()
                                .map(AsRef::as_ref);
                            focused_color(&layers, below, above, *index, x, y)
                        }
                        None => pixel(&layers, x, y),
                    };
                    shown(color, x, y)
                })
                .collect()
        };
        let threads = workers().min(jobs.len()).max(1);
        let samples: Vec<Vec<[u8; 4]>> = if threads <= 1 {
            jobs.iter().map(sample_row).collect()
        } else {
            let per = jobs.len().div_ceil(threads);
            std::thread::scope(|scope| {
                let workers: Vec<_> = jobs
                    .chunks(per)
                    .map(|chunk| {
                        let sample_row = &sample_row;
                        scope.spawn(move || chunk.iter().map(sample_row).collect::<Vec<_>>())
                    })
                    .collect();
                workers
                    .into_iter()
                    .flat_map(|worker| worker.join().unwrap_or_default())
                    .collect()
            })
        };
        let stride = self.size[0] as usize * 4;
        for ((rect, gy), row) in jobs.iter().zip(samples) {
            let first = rect[0] / step * step;
            for y in (*gy).max(rect[1])..(gy + step).min(rect[3]) {
                let line = &mut self.display[y as usize * stride..(y as usize + 1) * stride];
                for (block, color) in row.iter().enumerate() {
                    let gx = first + block as u32 * step;
                    for x in gx.max(rect[0])..(gx + step).min(rect[2]) {
                        line[x as usize * 4..x as usize * 4 + 4].copy_from_slice(color);
                    }
                }
            }
        }
        self.focus = focus.map(|(focus, _, _)| focus);
        self.version = next_version();
        self.changes.push_back((self.version, rects.to_vec()));
        if self.changes.len() > 64
            && let Some((version, _)) = self.changes.pop_front()
        {
            self.forgotten = version;
        }
        self.seen = signature;
    }

    /// Brings up to `tiles` of the waiting tiles (the ones in `visible`
    /// first) to full detail. Returns `true` while some still wait, so the
    /// caller draws another frame.
    pub fn refine(&mut self, arena: &Arena, visible: [u32; 4], tiles: usize) -> bool {
        if self.pending.is_empty() {
            return false;
        }
        let tile_rect = |(tx, ty): (u32, u32)| {
            [
                tx * PENDING_TILE,
                ty * PENDING_TILE,
                ((tx + 1) * PENDING_TILE).min(self.size[0]),
                ((ty + 1) * PENDING_TILE).min(self.size[1]),
            ]
        };
        let mut chosen: Vec<(u32, u32)> = self
            .pending
            .iter()
            .copied()
            .filter(|tile| meet(tile_rect(*tile), visible).is_some())
            .take(tiles)
            .collect();
        if chosen.len() < tiles {
            let more: Vec<(u32, u32)> = self
                .pending
                .iter()
                .copied()
                .filter(|tile| !chosen.contains(tile))
                .take(tiles - chosen.len())
                .collect();
            chosen.extend(more);
        }
        let rects: Vec<[u32; 4]> = chosen
            .iter()
            .map(|tile| tile_rect(*tile))
            .filter(|rect| rect[0] < rect[2] && rect[1] < rect[3])
            .collect();
        for tile in &chosen {
            self.pending.remove(tile);
        }
        let painted = self.pending_layer;
        if self.pending.is_empty() {
            self.pending_layer = None;
        }
        self.refresh_rects(arena, &rects, painted);
        !self.pending.is_empty()
    }

    /// Takes the picture as it is now as seen when the only change since
    /// the last look is to `painted` (a stroke's layer, written to
    /// although its pen moved too little to paint): no recomposite for
    /// that. Any other change is left for [`Self::sync`].
    pub fn accept(&mut self, arena: &Arena, painted: EntityId) {
        let signature = self.signature(arena);
        let mask = crate::doc::mask_of(arena, painted);
        let only = signature.len() == self.seen.len()
            && signature.iter().zip(&self.seen).all(|(now, seen)| {
                now == seen || (now.0 == seen.0 && (now.0 == painted || Some(now.0) == mask))
            });
        if only {
            self.seen = signature;
        }
    }

    /// `true` while some tiles wait for full detail.
    #[must_use]
    pub fn refining(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Recomposites the rectangle `[x0, y0, x1, y1]` after an edit made in
    /// place, and takes the edit as seen.
    pub fn refresh(&mut self, arena: &Arena, rect: [u32; 4]) {
        if self.size == [0, 0] {
            self.sync(arena);
            return;
        }
        let rect = [
            rect[0].min(self.size[0]),
            rect[1].min(self.size[1]),
            rect[2].min(self.size[0]),
            rect[3].min(self.size[1]),
        ];
        self.paint(arena, rect);
        self.seen = self.signature(arena);
    }

    /// Recomposites the rectangles `[x0, y0, x1, y1]` after an edit made in
    /// place (the tiles a stroke changed), gathering the layers once, and
    /// takes the edit as seen. `painted` is the layer the edit changed:
    /// the stacks under and over it are kept for the next refresh (see
    /// [`Focus`]).
    pub fn refresh_rects(&mut self, arena: &Arena, rects: &[[u32; 4]], painted: Option<EntityId>) {
        if self.size == [0, 0] {
            self.sync(arena);
            return;
        }
        let [width, height] = self.size;
        let rects: Vec<[u32; 4]> = rects
            .iter()
            .map(|r| {
                [
                    r[0].min(width),
                    r[1].min(height),
                    r[2].min(width),
                    r[3].min(height),
                ]
            })
            .filter(|r| r[0] < r[2] && r[1] < r[3])
            .collect();
        if rects.is_empty() {
            return;
        }
        let bounds = rects
            .iter()
            .fold(rects[0], |[a, b, c, d], [x0, y0, x1, y1]| {
                [a.min(*x0), b.min(*y0), c.max(*x1), d.max(*y1)]
            });
        let mut layers = gather_view(
            arena,
            self.picture,
            RenderOptions::default(),
            self.view.as_ref(),
        );
        prepare(&mut layers, bounds);
        self.attach(arena, &mut layers, false);
        let stride = width as usize * 4;
        let signature = self.signature(arena);
        // The stacks of the tiles this refresh needs.
        let mut wanted = Vec::new();
        for [x0, y0, x1, y1] in &rects {
            for ty in y0 / FOCUS_TILE..=(y1 - 1) / FOCUS_TILE {
                for tx in x0 / FOCUS_TILE..=(x1 - 1) / FOCUS_TILE {
                    if !wanted.contains(&(tx, ty)) {
                        wanted.push((tx, ty));
                    }
                }
            }
        }
        let focus = self.kept_focus(arena, &layers, painted, &signature, &wanted);
        // Each row's spans, then the rows painted on every core.
        let mut spans: Vec<Vec<(u32, u32)>> = vec![Vec::new(); (bounds[3] - bounds[1]) as usize];
        for [x0, y0, x1, y1] in &rects {
            for y in *y0..*y1 {
                spans[(y - bounds[1]) as usize].push((*x0, *x1));
            }
        }
        let area: usize = rects
            .iter()
            .map(|[x0, y0, x1, y1]| ((x1 - x0) * (y1 - y0)) as usize)
            .sum();
        let band = &mut self.display[bounds[1] as usize * stride..bounds[3] as usize * stride];
        rows(band, stride, bounds[1], area, |y, row| {
            for &(x0, x1) in &spans[(y - bounds[1]) as usize] {
                match &focus {
                    Some((focus, index, flat)) => {
                        focused_span(&layers, focus, *index, *flat, y, x0, x1, row);
                    }
                    None => composite_span(&layers, y, x0, x1, row),
                }
            }
        });
        self.focus = focus.map(|(focus, _, _)| focus);
        self.version = next_version();
        self.changes.push_back((self.version, rects));
        if self.changes.len() > 64
            && let Some((version, _)) = self.changes.pop_front()
        {
            self.forgotten = version;
        }
        self.seen = signature;
    }
}
