//! Selections: a coverage mask over the picture that brushes, fills,
//! clears and filters stay inside. Rectangles, ellipses and lasso shapes
//! replace, add to, subtract from or intersect the mask; a layer's pixels
//! become a selection by their alpha.

use sue_core::RasterTile2D;

/// The Selection area tool's Shrink selection sub tool's mode: a lasso
/// whose selection shrinks to what is drawn inside it.
pub const SHRINK_SELECTION: u8 = 6;

/// The shape a selection or figure drag keeps (Clip Studio Paint's
/// Aspect).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Aspect {
    /// Any shape.
    #[default]
    Free,
    /// As wide as tall.
    Square,
    /// `width` to `height`.
    Ratio {
        /// Parts across.
        width: f32,
        /// Parts down.
        height: f32,
    },
}

impl Aspect {
    /// Its stable name in saved settings (a ratio adds its numbers).
    #[must_use]
    pub fn id(self) -> String {
        match self {
            Self::Free => "free".to_owned(),
            Self::Square => "square".to_owned(),
            Self::Ratio { width, height } => format!("ratio {width} {height}"),
        }
    }

    /// The aspect named `id` ([`Self::id`]).
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        let mut words = id.split_whitespace();
        match words.next()? {
            "free" => Some(Self::Free),
            "square" => Some(Self::Square),
            "ratio" => {
                let width: f32 = words.next()?.parse().ok()?;
                let height: f32 = words.next()?.parse().ok()?;
                (width > 0.0 && height > 0.0).then_some(Self::Ratio { width, height })
            }
            _ => None,
        }
    }

    /// Where a drag from `from` to `to` ends once it keeps this shape: the
    /// longer side rules, and the corner stays on the side it was dragged
    /// to.
    #[must_use]
    pub fn constrain(self, from: [f32; 2], to: [f32; 2]) -> [f32; 2] {
        let (rw, rh) = match self {
            Self::Free => return to,
            Self::Square => (1.0, 1.0),
            Self::Ratio { width, height } => (width.max(1e-3), height.max(1e-3)),
        };
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        // The size that holds the drag: whichever side asks more.
        let scale = (dx.abs() / rw).max(dy.abs() / rh);
        let sign = |value: f32| if value < 0.0 { -1.0 } else { 1.0 };
        [
            from[0] + sign(dx) * rw * scale,
            from[1] + sign(dy) * rh * scale,
        ]
    }
}

/// Shrink selection: of the area `inside` (one byte a pixel, a lasso's
/// inside) only what is drawn in it, by `reference` (straight RGBA8
/// picture pixels of `size`): the clear pixels joined to the lasso's rim
/// fall away, so the selection hugs the drawing's outline and keeps the
/// clear holes the drawing closes in.
#[must_use]
pub fn shrink_to_drawn(inside: &[u8], reference: &[u8], size: [u32; 2]) -> Vec<u8> {
    let [width, height] = size;
    let count = width as usize * height as usize;
    if inside.len() < count || reference.len() < count * 4 {
        return vec![0; count];
    }
    let clear = |index: usize| reference[index * 4 + 3] < 8;
    let mut outside = vec![false; count];
    let mut stack: Vec<usize> = Vec::new();
    // Clear pixels inside that touch the outside (or the picture's edge).
    for index in 0..count {
        if inside[index] == 0 || !clear(index) {
            continue;
        }
        let (x, y) = (index % width as usize, index / width as usize);
        let rim = x == 0
            || y == 0
            || x + 1 == width as usize
            || y + 1 == height as usize
            || inside[index - 1] == 0
            || inside[index + 1] == 0
            || inside[index - width as usize] == 0
            || inside[index + width as usize] == 0;
        if rim {
            stack.push(index);
        }
    }
    while let Some(index) = stack.pop() {
        if outside[index] || inside[index] == 0 || !clear(index) {
            continue;
        }
        outside[index] = true;
        let (x, y) = (index % width as usize, index / width as usize);
        if x > 0 {
            stack.push(index - 1);
        }
        if x + 1 < width as usize {
            stack.push(index + 1);
        }
        if y > 0 {
            stack.push(index - width as usize);
        }
        if y + 1 < height as usize {
            stack.push(index + width as usize);
        }
    }
    (0..count)
        .map(|index| if outside[index] { 0 } else { inside[index] })
        .collect()
}

/// How a new shape combines with the selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Op {
    /// The shape alone.
    #[default]
    Replace,
    /// The selection plus the shape.
    Add,
    /// The selection minus the shape.
    Subtract,
    /// Where both overlap.
    Intersect,
}

impl Op {
    /// Every operation.
    pub const ALL: [Self; 4] = [Self::Replace, Self::Add, Self::Subtract, Self::Intersect];

    /// What people read.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Replace => tr!("new-2"),
            Self::Add => tr!("add-2"),
            Self::Subtract => tr!("subtract"),
            Self::Intersect => tr!("intersect"),
        }
    }
}

/// A selection mask, 0 (out) to 255 (in) per pixel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    width: u32,
    height: u32,
    mask: Vec<u8>,
}

impl Selection {
    /// Nothing selected.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            mask: vec![0; width as usize * height as usize],
        }
    }

    /// Everything selected.
    #[must_use]
    pub fn all(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            mask: vec![255; width as usize * height as usize],
        }
    }

    /// The pixels of `raster` (straight RGBA) by their alpha.
    #[must_use]
    pub fn from_alpha(raster: &RasterTile2D) -> Self {
        let mask = raster
            .bytes()
            .map(|bytes| {
                bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|pixel| pixel[3])
                    .collect()
            })
            .unwrap_or_else(|| vec![255; raster.width() as usize * raster.height() as usize]);
        Self {
            width: raster.width(),
            height: raster.height(),
            mask,
        }
    }

    /// A selection from a ready mask (one byte per pixel).
    #[must_use]
    pub fn from_mask(width: u32, height: u32, mask: Vec<u8>) -> Option<Self> {
        (mask.len() == width as usize * height as usize).then_some(Self {
            width,
            height,
            mask,
        })
    }

    /// Width and height.
    #[must_use]
    pub fn size(&self) -> [u32; 2] {
        [self.width, self.height]
    }

    /// Coverage at a pixel, 0 to 1 (0 outside the picture).
    #[must_use]
    pub fn at(&self, x: u32, y: u32) -> f32 {
        if x >= self.width || y >= self.height {
            return 0.0;
        }
        f32::from(self.mask[(y * self.width + x) as usize]) / 255.0
    }

    /// `true` if nothing is selected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.mask.iter().all(|value| *value == 0)
    }

    /// The selected area's bounds `[x0, y0, x1, y1]`, if any.
    #[must_use]
    pub fn bounds(&self) -> Option<[u32; 4]> {
        let mut bounds: Option<[u32; 4]> = None;
        for y in 0..self.height {
            let row = &self.mask[(y * self.width) as usize..((y + 1) * self.width) as usize];
            let Some(first) = row.iter().position(|value| *value > 0) else {
                continue;
            };
            let last = row.iter().rposition(|value| *value > 0).unwrap_or(first);
            let (first, last) = (first as u32, last as u32 + 1);
            bounds = Some(match bounds {
                Some([x0, y0, x1, _]) => [x0.min(first), y0, x1.max(last), y + 1],
                None => [first, y, last, y + 1],
            });
        }
        bounds
    }

    /// The coverage, one byte a pixel, row by row.
    #[must_use]
    pub fn mask(&self) -> &[u8] {
        &self.mask
    }

    /// The selection grown by `by` pixels all round, with round corners
    /// (Expand Selected Area).
    #[must_use]
    pub fn expanded(&self, by: f32) -> Self {
        self.grown(by, 0.0)
    }

    /// The selection grown by `by` pixels, counting what lies past the
    /// picture's edge as `outside` (0 out, 1 in).
    fn grown(&self, by: f32, outside: f32) -> Self {
        let (width, height) = (i64::from(self.width), i64::from(self.height));
        let edge = crate::effects::Edge::build(
            |x, y| {
                if x < 0 || y < 0 || x >= width || y >= height {
                    outside
                } else {
                    self.at(x as u32, y as u32)
                }
            },
            [0, 0, self.width, self.height],
            by.max(0.0),
        );
        let mut out = self.clone();
        for y in 0..self.height {
            for x in 0..self.width {
                let at = (y * self.width + x) as usize;
                let grown = (edge.at(x, y) * 255.0).round() as u8;
                out.mask[at] = out.mask[at].max(grown);
            }
        }
        out
    }

    /// The selection grown by `by` pixels with square corners (Expand
    /// Selected Area with the square expansion type): each pixel takes
    /// the most coverage of the square `by` pixels round it.
    #[must_use]
    pub fn expanded_square(&self, by: f32) -> Self {
        let reach = by.max(0.0).round() as usize;
        if reach == 0 || self.width == 0 || self.height == 0 {
            return self.clone();
        }
        let (width, height) = (self.width as usize, self.height as usize);
        let across = widest(&self.mask, height, width, [width, 1], reach);
        let mut out = self.clone();
        out.mask = widest(&across, width, height, [1, width], reach);
        out
    }

    /// The selection drawn in by `by` pixels all round (Shrink Selected
    /// Area).
    #[must_use]
    pub fn shrunk(&self, by: f32) -> Self {
        // What is not selected grows in, from past the edge too.
        let mut outside = self.clone();
        outside.invert();
        let mut out = outside.grown(by, 1.0);
        out.invert();
        out
    }

    /// The selection's border softened over `radius` pixels (Blur Border):
    /// three box blurs, as near a gaussian as needs be.
    #[must_use]
    pub fn blurred(&self, radius: f32) -> Self {
        let r = (radius / 1.7).round().max(1.0) as usize;
        let (w, h) = (self.width as usize, self.height as usize);
        let mut values: Vec<f32> = self.mask.iter().map(|value| f32::from(*value)).collect();
        let mut line = Vec::new();
        for _ in 0..3 {
            for (count, stride, step) in [(h, w, 1_usize), (w, 1, w)] {
                let length = if step == 1 { w } else { h };
                for lane in 0..count {
                    let start = lane * stride;
                    line.clear();
                    line.extend((0..length).map(|i| values[start + i * step]));
                    let mut sum: f32 = line.iter().take(r + 1).sum::<f32>() + line[0] * r as f32;
                    for i in 0..length {
                        values[start + i * step] = sum / (2 * r + 1) as f32;
                        let add = line[(i + r + 1).min(length - 1)];
                        let drop = line[i.saturating_sub(r)];
                        sum += add - drop;
                    }
                }
            }
        }
        Self {
            width: self.width,
            height: self.height,
            mask: values
                .iter()
                .map(|value| value.round().clamp(0.0, 255.0) as u8)
                .collect(),
        }
    }

    /// Paints a soft-edged disc into the selection, or out of it (the
    /// Selection pen and Erase selection).
    pub fn disc(&mut self, center: [f32; 2], radius: f32, add: bool) {
        let reach = radius + 1.0;
        let clamp = |value: f32, top: u32| (value.max(0.0) as u32).min(top);
        let (x0, x1) = (
            clamp(center[0] - reach, self.width),
            clamp(center[0] + reach + 1.0, self.width),
        );
        let (y0, y1) = (
            clamp(center[1] - reach, self.height),
            clamp(center[1] + reach + 1.0, self.height),
        );
        for y in y0..y1 {
            for x in x0..x1 {
                let away = (x as f32 + 0.5 - center[0]).hypot(y as f32 + 0.5 - center[1]);
                let cover = ((radius - away + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8;
                let at = (y * self.width + x) as usize;
                self.mask[at] = if add {
                    self.mask[at].max(cover)
                } else {
                    ((u16::from(self.mask[at]) * (255 - u16::from(cover)) + 127) / 255) as u8
                };
            }
        }
    }

    /// Paints a dab of the Selection pen into the selection, or out of it
    /// (Erase selection): full in the middle as far as `hardness` (0 to
    /// 1) of `radius` says, then falling off softly to the rim, and at
    /// most `density` (0 to 1) selected. A hard dab at full density is
    /// [`Self::disc`].
    pub fn dab(&mut self, center: [f32; 2], radius: f32, hardness: f32, density: f32, add: bool) {
        let hardness = hardness.clamp(0.0, 1.0);
        let density = density.clamp(0.0, 1.0);
        if hardness >= 1.0 && density >= 1.0 {
            self.disc(center, radius, add);
            return;
        }
        let reach = radius + 1.0;
        let clamp = |value: f32, top: u32| (value.max(0.0) as u32).min(top);
        let (x0, x1) = (
            clamp(center[0] - reach, self.width),
            clamp(center[0] + reach + 1.0, self.width),
        );
        let (y0, y1) = (
            clamp(center[1] - reach, self.height),
            clamp(center[1] + reach + 1.0, self.height),
        );
        let inner = radius * hardness;
        for y in y0..y1 {
            for x in x0..x1 {
                let away = (x as f32 + 0.5 - center[0]).hypot(y as f32 + 0.5 - center[1]);
                let edge = (radius - away + 0.5).clamp(0.0, 1.0);
                let fall = if away <= inner {
                    1.0
                } else {
                    let t = ((away - inner) / (radius - inner).max(0.5)).clamp(0.0, 1.0);
                    1.0 - t * t * (3.0 - 2.0 * t)
                };
                let cover = (edge * fall * density * 255.0).round() as u8;
                let at = (y * self.width + x) as usize;
                self.mask[at] = if add {
                    self.mask[at].max(cover)
                } else {
                    ((u16::from(self.mask[at]) * (255 - u16::from(cover)) + 127) / 255) as u8
                };
            }
        }
    }

    /// The pixels of `pixels` (RGBA8, `width` by `height`) within
    /// `tolerance` (0 to 1) of `color` (Select Color Gamut).
    #[must_use]
    pub fn of_color(
        pixels: &[u8],
        width: u32,
        height: u32,
        color: [u8; 3],
        tolerance: f32,
    ) -> Self {
        let limit = (tolerance.clamp(0.0, 1.0) * 255.0).round() as i32;
        let mask = pixels
            .chunks(4)
            .take((width * height) as usize)
            .map(|pixel| {
                let near = pixel[3] > 0
                    && (0..3).all(|channel| {
                        (i32::from(pixel[channel]) - i32::from(color[channel])).abs() <= limit
                    });
                if near { 255 } else { 0 }
            })
            .collect();
        Self {
            width,
            height,
            mask,
        }
    }

    /// Selects everything that was not, and the other way round.
    pub fn invert(&mut self) {
        for value in &mut self.mask {
            *value = 255 - *value;
        }
    }

    fn combine(&mut self, op: Op, shape: impl Fn(u32, u32) -> u8) {
        for y in 0..self.height {
            for x in 0..self.width {
                let at = (y * self.width + x) as usize;
                let new = shape(x, y);
                let old = self.mask[at];
                self.mask[at] = match op {
                    Op::Replace => new,
                    Op::Add => old.max(new),
                    Op::Subtract => ((u16::from(old) * (255 - u16::from(new)) + 127) / 255) as u8,
                    Op::Intersect => old.min(new),
                };
            }
        }
    }

    /// Combines a ready mask (one byte per pixel) into the selection.
    pub fn apply_mask(&mut self, mask: &[u8], op: Op) {
        if mask.len() != self.mask.len() {
            return;
        }
        let width = self.width;
        self.combine(op, |x, y| mask[(y * width + x) as usize]);
    }

    /// The selection as seen from a layer whose pixels start at `offset`
    /// in the picture: pixel `(x, y)` of the result is picture pixel
    /// `(x + offset.x, y + offset.y)`.
    #[must_use]
    pub fn shifted(&self, offset: [i64; 2]) -> Self {
        let mut moved = Self::new(self.width, self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                let (px, py) = (i64::from(x) + offset[0], i64::from(y) + offset[1]);
                if px >= 0 && py >= 0 && px < i64::from(self.width) && py < i64::from(self.height) {
                    moved.mask[(y * self.width + x) as usize] =
                        self.mask[(py as u32 * self.width + px as u32) as usize];
                }
            }
        }
        moved
    }

    /// A rectangle `[x0, y0, x1, y1]` (end exclusive).
    pub fn rect(&mut self, [x0, y0, x1, y1]: [u32; 4], op: Op) {
        self.combine(op, |x, y| {
            if x >= x0 && x < x1 && y >= y0 && y < y1 {
                255
            } else {
                0
            }
        });
    }

    /// The ellipse inside `[x0, y0, x1, y1]`, with a soft one-pixel rim.
    pub fn ellipse(&mut self, [x0, y0, x1, y1]: [f32; 4], op: Op) {
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (rx, ry) = (((x1 - x0) / 2.0).abs(), ((y1 - y0) / 2.0).abs());
        self.combine(op, |x, y| {
            if rx < 0.5 || ry < 0.5 {
                return 0;
            }
            let (dx, dy) = ((x as f32 + 0.5 - cx) / rx, (y as f32 + 0.5 - cy) / ry);
            let distance = (dx * dx + dy * dy).sqrt();
            // Distance past the rim in pixels, roughly.
            let past = (distance - 1.0) * rx.min(ry);
            ((0.5 - past).clamp(0.0, 1.0) * 255.0).round() as u8
        });
    }

    /// The inside of a closed polygon (even-odd rule).
    pub fn polygon(&mut self, points: &[[f32; 2]], op: Op) {
        if points.len() < 3 {
            if op == Op::Replace || op == Op::Intersect {
                self.combine(op, |_, _| 0);
            }
            return;
        }
        let mut inside = vec![0_u8; self.mask.len()];
        let width = self.width;
        for y in 0..self.height {
            let sample = y as f32 + 0.5;
            let mut crossings: Vec<f32> = Vec::new();
            for (index, a) in points.iter().enumerate() {
                let b = points[(index + 1) % points.len()];
                if (a[1] <= sample) != (b[1] <= sample) {
                    let t = (sample - a[1]) / (b[1] - a[1]);
                    crossings.push(a[0] + (b[0] - a[0]) * t);
                }
            }
            crossings.sort_by(f32::total_cmp);
            for pair in crossings.as_chunks::<2>().0 {
                let from = (pair[0] - 0.5).ceil().max(0.0) as u32;
                let to = ((pair[1] - 0.5).floor() + 1.0).clamp(0.0, width as f32) as u32;
                for x in from..to.min(width) {
                    inside[(y * width + x) as usize] = 255;
                }
            }
        }
        self.combine(op, |x, y| inside[(y * width + x) as usize]);
    }

    /// The outline between selected and unselected pixels, as segments in
    /// picture pixels (for drawing marching ants).
    #[must_use]
    pub fn edges(&self) -> Vec<[[f32; 2]; 2]> {
        let inside = |x: i64, y: i64| {
            x >= 0
                && y >= 0
                && x < i64::from(self.width)
                && y < i64::from(self.height)
                && self.mask[(y as u32 * self.width + x as u32) as usize] >= 128
        };
        let mut segments = Vec::new();
        // Horizontal edges, merged into runs along each row line.
        for y in 0..=i64::from(self.height) {
            let mut run: Option<i64> = None;
            for x in 0..=i64::from(self.width) {
                let edge = x < i64::from(self.width) && inside(x, y - 1) != inside(x, y);
                match (edge, run) {
                    (true, None) => run = Some(x),
                    (false, Some(start)) => {
                        segments.push([[start as f32, y as f32], [x as f32, y as f32]]);
                        run = None;
                    }
                    _ => {}
                }
            }
        }
        // Vertical edges, merged along each column line.
        for x in 0..=i64::from(self.width) {
            let mut run: Option<i64> = None;
            for y in 0..=i64::from(self.height) {
                let edge = y < i64::from(self.height) && inside(x - 1, y) != inside(x, y);
                match (edge, run) {
                    (true, None) => run = Some(y),
                    (false, Some(start)) => {
                        segments.push([[x as f32, start as f32], [x as f32, y as f32]]);
                        run = None;
                    }
                    _ => {}
                }
            }
        }
        segments
    }
}

/// The most of `values` within `reach` along each of `lanes` lanes of
/// `length` values: lane `n` starts at `n * step[0]` and walks by
/// `step[1]`. A window that slides with a queue of the falling maxima, so
/// the cost does not grow with `reach`.
fn widest(values: &[u8], lanes: usize, length: usize, step: [usize; 2], reach: usize) -> Vec<u8> {
    let mut out = values.to_vec();
    let mut queue = std::collections::VecDeque::with_capacity(length);
    for lane in 0..lanes {
        let at = |index: usize| lane * step[0] + index * step[1];
        queue.clear();
        let mut next = 0;
        for index in 0..length {
            let last = (index + reach).min(length - 1);
            while next <= last {
                while queue
                    .back()
                    .is_some_and(|back: &usize| values[at(*back)] <= values[at(next)])
                {
                    queue.pop_back();
                }
                queue.push_back(next);
                next += 1;
            }
            while queue.front().is_some_and(|front| *front + reach < index) {
                queue.pop_front();
            }
            if let Some(front) = queue.front() {
                out[at(index)] = values[at(*front)];
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_soft_dab_is_full_in_its_middle_and_never_past_its_density() {
        let mut soft = Selection::new(40, 40);
        soft.dab([20.0, 20.0], 10.0, 0.2, 0.5, true);
        let at = |s: &Selection, x: u32| s.mask()[(20 * 40 + x) as usize];
        assert_eq!(at(&soft, 20), 128, "half selected in the middle");
        assert!(
            at(&soft, 27) > 0 && at(&soft, 27) < at(&soft, 22),
            "falls off"
        );
        assert_eq!(at(&soft, 31), 0, "nothing past the rim");
        // Laid again it stays at its density.
        soft.dab([20.0, 20.0], 10.0, 0.2, 0.5, true);
        assert_eq!(at(&soft, 20), 128);
        let mut hard = Selection::new(40, 40);
        hard.dab([20.0, 20.0], 10.0, 1.0, 1.0, true);
        let mut disc = Selection::new(40, 40);
        disc.disc([20.0, 20.0], 10.0, true);
        assert_eq!(hard.mask(), disc.mask());
    }

    #[test]
    fn selections_grow_shrink_soften_paint_and_pick_colors() {
        let mut square = Selection::new(40, 40);
        square.rect([10, 10, 30, 30], Op::Replace);
        let grown = square.expanded(4.0);
        assert_eq!(grown.at(7, 20), 1.0, "three pixels out");
        assert_eq!(grown.at(3, 20), 0.0, "past the growth");
        assert!(grown.at(7, 7) < 1.0, "round at the corners");
        let shrunk = square.shrunk(4.0);
        assert_eq!(shrunk.at(15, 20), 1.0);
        assert_eq!(shrunk.at(11, 20), 0.0, "drawn in");
        assert!(!shrunk.is_empty());
        let soft = square.blurred(4.0);
        let edge = soft.at(10, 20);
        assert!(edge > 0.2 && edge < 0.8, "{edge}");
        assert_eq!(soft.at(20, 20), 1.0);
        // The selection pen and its eraser.
        let mut painted = Selection::new(40, 40);
        painted.disc([20.0, 20.0], 5.0, true);
        assert_eq!(painted.at(20, 20), 1.0);
        assert_eq!(painted.at(30, 20), 0.0);
        painted.disc([20.0, 20.0], 2.0, false);
        assert_eq!(painted.at(20, 20), 0.0, "erased in the middle");
        assert_eq!(painted.at(23, 20), 1.0);
        // Select Color Gamut: the red pixels, near enough.
        let mut pixels = vec![255_u8; 4 * 4 * 4];
        pixels[..4].copy_from_slice(&[250, 10, 0, 255]);
        let red = Selection::of_color(&pixels, 4, 4, [255, 0, 0], 0.1);
        assert_eq!(red.at(0, 0), 1.0);
        assert_eq!(red.at(1, 0), 0.0);
    }

    #[test]
    fn a_square_expansion_keeps_square_corners() {
        let mut square = Selection::new(40, 40);
        square.rect([10, 10, 30, 30], Op::Replace);
        let grown = square.expanded_square(4.0);
        assert_eq!(grown.at(6, 20), 1.0, "four pixels out");
        assert_eq!(grown.at(5, 20), 0.0, "past the growth");
        assert_eq!(grown.at(6, 6), 1.0, "square at the corners");
        assert_eq!(grown.at(5, 6), 0.0);
        assert!(
            square.expanded(4.0).at(6, 6) < 1.0,
            "the round one is round"
        );
        assert_eq!(square.expanded_square(0.0).mask(), square.mask());
        // Partial coverage keeps its own value.
        let mut soft = Selection::new(5, 1);
        soft.mask[2] = 128;
        let grown = soft.expanded_square(1.0);
        assert_eq!(grown.mask(), [0, 128, 128, 128, 0]);
    }

    #[test]
    fn shapes_combine_and_invert() {
        let mut selection = Selection::new(20, 10);
        assert!(selection.is_empty());
        assert_eq!(selection.bounds(), None);
        selection.rect([2, 2, 8, 6], Op::Replace);
        assert_eq!(selection.bounds(), Some([2, 2, 8, 6]));
        selection.rect([6, 4, 12, 8], Op::Add);
        assert_eq!(selection.bounds(), Some([2, 2, 12, 8]));
        selection.rect([0, 0, 4, 10], Op::Subtract);
        assert_eq!(selection.at(3, 3), 0.0);
        assert_eq!(selection.at(5, 3), 1.0);
        selection.rect([5, 0, 7, 10], Op::Intersect);
        assert_eq!(selection.bounds(), Some([5, 2, 7, 8]));
        selection.invert();
        assert_eq!(selection.at(0, 0), 1.0);
        assert_eq!(selection.at(5, 3), 0.0);
        assert_eq!(selection.at(99, 99), 0.0, "outside the picture");
    }

    #[test]
    fn ellipses_and_lassos_fill_their_inside() {
        let mut selection = Selection::new(40, 40);
        selection.ellipse([0.0, 0.0, 40.0, 40.0], Op::Replace);
        assert_eq!(selection.at(20, 20), 1.0);
        assert_eq!(selection.at(1, 1), 0.0);
        let mut lasso = Selection::new(40, 40);
        lasso.polygon(&[[0.0, 0.0], [40.0, 0.0], [0.0, 40.0]], Op::Replace);
        assert_eq!(lasso.at(5, 5), 1.0);
        assert_eq!(lasso.at(35, 35), 0.0);
        assert_eq!(lasso.bounds().map(|b| b[0]), Some(0));
    }

    #[test]
    fn outlines_follow_the_edge() {
        let mut selection = Selection::new(10, 10);
        selection.rect([2, 3, 6, 7], Op::Replace);
        let edges = selection.edges();
        assert_eq!(edges.len(), 4, "one run per side: {edges:?}");
        assert!(edges.contains(&[[2.0, 3.0], [6.0, 3.0]]));
        assert!(edges.contains(&[[6.0, 3.0], [6.0, 7.0]]));
        let alpha = RasterTile2D::from_bytes(
            2,
            1,
            sue_core::PixelFormat::Rgba8,
            vec![0, 0, 0, 255, 0, 0, 0, 0],
        )
        .unwrap();
        let from = Selection::from_alpha(&alpha);
        assert_eq!((from.at(0, 0), from.at(1, 0)), (1.0, 0.0));
    }

    #[test]
    fn aspect_keeps_the_shape_of_a_drag() {
        assert_eq!(
            Aspect::Free.constrain([0.0, 0.0], [30.0, 10.0]),
            [30.0, 10.0]
        );
        assert_eq!(
            Aspect::Square.constrain([10.0, 10.0], [40.0, 20.0]),
            [40.0, 40.0],
            "the longer side rules"
        );
        assert_eq!(
            Aspect::Square.constrain([10.0, 10.0], [0.0, -30.0]),
            [-30.0, -30.0],
            "up and left stay up and left"
        );
        let wide = Aspect::Ratio {
            width: 16.0,
            height: 9.0,
        };
        let [x, y] = wide.constrain([0.0, 0.0], [32.0, 2.0]);
        assert_eq!((x, y), (32.0, 18.0));
        for aspect in [Aspect::Free, Aspect::Square, wide] {
            assert_eq!(Aspect::from_id(&aspect.id()), Some(aspect));
        }
        assert_eq!(Aspect::from_id("ratio 0 2"), None);
        assert_eq!(Aspect::from_id("round"), None);
    }

    #[test]
    fn shrink_selection_hugs_what_is_drawn_inside_the_lasso() {
        // A 30 by 30 picture with a drawn ring from 10 to 19 (a clear hole
        // in it) and a dot outside the lasso.
        let size = [30_u32, 30_u32];
        let mut reference = vec![0_u8; 30 * 30 * 4];
        for y in 10..20_usize {
            for x in 10..20_usize {
                let hole = (13..17).contains(&x) && (13..17).contains(&y);
                if !hole {
                    reference[(y * 30 + x) * 4 + 3] = 255;
                }
            }
        }
        reference[(2 * 30 + 2) * 4 + 3] = 255;
        let mut lasso = Selection::new(30, 30);
        lasso.polygon(
            &[[5.0, 5.0], [25.0, 5.0], [25.0, 25.0], [5.0, 25.0]],
            Op::Replace,
        );
        let shrunk = shrink_to_drawn(lasso.mask(), &reference, size);
        let at = |x: usize, y: usize| shrunk[y * 30 + x];
        assert_eq!(at(10, 10), 255, "the drawing");
        assert_eq!(at(14, 14), 255, "a hole it closes in stays");
        assert_eq!(at(7, 7), 0, "clear around it falls away");
        assert_eq!(at(2, 2), 0, "outside the lasso stays out");
        assert_eq!(shrunk.iter().filter(|value| **value != 0).count(), 100);
        // A lasso round nothing drawn selects nothing.
        let empty = shrink_to_drawn(lasso.mask(), &vec![0; 30 * 30 * 4], size);
        assert!(empty.iter().all(|value| *value == 0));
    }
}
