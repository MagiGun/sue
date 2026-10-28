//! Selections: a coverage mask over the picture that brushes, fills,
//! clears and filters stay inside. Rectangles, ellipses and lasso shapes
//! replace, add to, subtract from or intersect the mask; a layer's pixels
//! become a selection by their alpha.

use sue_core::RasterTile2D;

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
    pub const fn label(self) -> &'static str {
        match self {
            Self::Replace => "New",
            Self::Add => "Add",
            Self::Subtract => "Subtract",
            Self::Intersect => "Intersect",
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
