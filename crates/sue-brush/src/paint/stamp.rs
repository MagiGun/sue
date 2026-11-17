//! Planned dabs stamped as coverage: the tip (round or a picture),
//! flipped as planned, and broken up by the paper when the texture is
//! laid dab by dab.

use std::sync::Arc;

use sue_core::JobPool;

use super::plan::Planned;
use crate::buffer::Painter;
use crate::raster::{Dab, DabShape, PixelRect, RoundTip, Shape, TipImage, Turn};
use crate::texture::{Paper, Texture, TextureSettings};

/// What dabs are stamped with, owned, so a dab's coverage can be worked
/// out on several threads: the tip's pictures, the width of a hard
/// rim's antialiasing, and the paper laid dab by dab.
#[derive(Clone, Debug)]
pub struct SharedTips {
    /// The tip's pictures.
    pub pictures: Arc<[TipImage]>,
    /// The width of a hard round rim's antialiasing.
    pub edge: f32,
    /// The paper laid dab by dab, if any.
    pub texture: Option<(Arc<Paper>, TextureSettings)>,
}

/// A dab with fewer pixels than this is worked out on one thread:
/// handing it out would cost more than it saves.
pub(crate) const WORTH_SHARING: usize = 96 * 96;

/// What `shape` answers for the center of every pixel of rows `rows`
/// of `rect`, row by row.
fn cover_rows(shape: &impl Shape, rect: PixelRect, rows: std::ops::Range<i32>, out: &mut [f32]) {
    let w = (rect.x1 - rect.x0) as usize;
    for (row, py) in out.chunks_mut(w.max(1)).zip(rows) {
        for (c, px) in row.iter_mut().zip(rect.x0..rect.x1) {
            *c = shape.coverage([px as f32 + 0.5, py as f32 + 0.5]);
        }
    }
}

/// The coverage of the dab `planned`, stamped with `tips`, at the
/// center of every pixel of its bounds `rect` (row by row into `out`),
/// its rows worked out in `runs` runs: the first here, the others on
/// `pool`. Each value is what [`Stamp`]'s `coverage` answers there, so
/// it does not matter which thread asked.
///
/// # Errors
/// When a job of the pool failed.
pub(crate) fn cover_shared(
    planned: &Planned,
    tips: &SharedTips,
    rect: PixelRect,
    runs: usize,
    pool: &JobPool,
    out: &mut [f32],
) -> Result<(), String> {
    let w = (rect.x1 - rect.x0) as usize;
    let rows = (rect.y1 - rect.y0) as usize;
    let per = rows.div_ceil(runs.max(1));
    let stamped = |tips: &SharedTips, rows: std::ops::Range<i32>, out: &mut [f32]| {
        let texture = tips
            .texture
            .as_ref()
            .map(|(paper, settings)| Texture::new(paper, *settings));
        let borrowed = Tips {
            pictures: &tips.pictures,
            edge: tips.edge,
        };
        cover_rows(
            &Stamp::new(planned, &borrowed, texture.as_ref()),
            rect,
            rows,
            out,
        );
    };
    let jobs: Vec<_> = (1..runs)
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
                let mut part = vec![0.0_f32; (to - from) * w];
                cover_rows(
                    &stamp,
                    rect,
                    rect.y0 + from as i32..rect.y0 + to as i32,
                    &mut part,
                );
                Ok(part)
            });
            (from * w, job)
        })
        .collect();
    let first = per.min(rows);
    stamped(tips, rect.y0..rect.y0 + first as i32, &mut out[..first * w]);
    let mut failed = None;
    for (at, job) in jobs {
        match job.wait() {
            Ok(part) => out[at..at + part.len()].copy_from_slice(&part),
            Err(error) => failed = Some(error.to_string()),
        }
    }
    failed.map_or(Ok(()), Err)
}

/// [`stamp_coverage`] with every big dab's coverage worked out on
/// `pool`'s threads (`runs` runs of its rows) and laid here: the same
/// values combined the same way, dab after dab.
///
/// # Errors
/// When a job of the pool failed: the part cannot be trusted.
pub fn stamp_coverage_shared(
    painter: &mut Painter<'_>,
    dabs: &[Planned],
    tips: &SharedTips,
    runs: usize,
    pool: &JobPool,
    cover: &mut Vec<f32>,
) -> Result<(), String> {
    let texture = tips
        .texture
        .as_ref()
        .map(|(paper, settings)| Texture::new(paper, *settings));
    let borrowed = Tips {
        pictures: &tips.pictures,
        edge: tips.edge,
    };
    for planned in dabs {
        let stamp = Stamp::new(planned, &borrowed, texture.as_ref());
        let rect = stamp.bounds();
        if rect.is_empty() {
            continue;
        }
        let area = (rect.x1 - rect.x0) as usize * (rect.y1 - rect.y0) as usize;
        if area < WORTH_SHARING {
            painter.fill(&stamp);
            continue;
        }
        cover.clear();
        cover.resize(area, 0.0);
        cover_shared(planned, tips, rect, runs, pool, cover)?;
        painter.fill_cover(rect, cover);
    }
    Ok(())
}

/// The tip a stroke stamps: its pictures (none for a round tip) and the
/// antialiasing width of a hard round rim.
#[derive(Clone, Copy, Debug)]
pub struct Tips<'a> {
    /// The tip's pictures, as [`Planned::tip`] counts them.
    pub pictures: &'a [TipImage],
    /// The width of a hard round rim's antialiasing, in pixels.
    pub edge: f32,
}

impl<'a> Tips<'a> {
    /// A round tip with a hard rim antialiased over `edge` pixels.
    #[must_use]
    pub const fn round(edge: f32) -> Self {
        Self {
            pictures: &[],
            edge,
        }
    }

    /// The dab `planned` stamps with these tips.
    #[must_use]
    pub fn dab(&self, planned: &Planned) -> Dab<'a> {
        let shape = match planned.tip.and_then(|i| self.pictures.get(i)) {
            Some(picture) => DabShape::Image(picture),
            None => DabShape::Round(RoundTip::new(planned.hardness, self.edge)),
        };
        Dab::new(planned.disc, planned.aspect, planned.turn, shape)
    }
}

/// One planned dab as a shape: the tip's dab, mirrored in the tip's own
/// frame where flipped, and multiplied by the paper where a texture is
/// laid dab by dab.
#[derive(Clone, Copy, Debug)]
pub struct Stamp<'a> {
    dab: Dab<'a>,
    turn: Turn,
    flip: [bool; 2],
    texture: Option<(&'a Texture<'a>, f32)>,
}

impl<'a> Stamp<'a> {
    /// `planned` stamped with `tips`, broken up by `texture` at the dab's
    /// own texture strength when one is given.
    #[must_use]
    pub fn new(planned: &Planned, tips: &Tips<'a>, texture: Option<&'a Texture<'a>>) -> Self {
        Self {
            dab: tips.dab(planned),
            turn: planned.turn,
            flip: planned.flip,
            texture: texture.map(|t| (t, planned.texture_strength)),
        }
    }
}

impl Shape for Stamp<'_> {
    fn bounds(&self) -> PixelRect {
        // A mirror in the tip's frame keeps the tip's turned box.
        self.dab.bounds()
    }

    fn coverage(&self, p: [f32; 2]) -> f32 {
        let at = self.dab.disc().at;
        let q = if self.flip[0] || self.flip[1] {
            let (c, s) = (self.turn.cos(), self.turn.sin());
            let (x, y) = (p[0] - at[0], p[1] - at[1]);
            let (mut u, mut v) = (c * x + s * y, -s * x + c * y);
            if self.flip[0] {
                u = -u;
            }
            if self.flip[1] {
                v = -v;
            }
            [at[0] + c * u - s * v, at[1] + s * u + c * v]
        } else {
            p
        };
        let coverage = self.dab.coverage(q);
        match self.texture {
            Some((texture, strength)) if coverage > 0.0 => {
                texture.apply_with(coverage, p, strength)
            }
            _ => coverage,
        }
    }
}

/// Stamps `dabs` into `painter`'s part of a stroke buffer.
pub fn stamp_coverage(
    painter: &mut Painter<'_>,
    dabs: &[Planned],
    tips: &Tips<'_>,
    texture: Option<&Texture<'_>>,
) {
    for planned in dabs {
        painter.fill(&Stamp::new(planned, tips, texture));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::Disc;

    fn planned(flip: [bool; 2]) -> Planned {
        Planned {
            disc: Disc::new([20.0, 20.0], 10.0, 1.0),
            aspect: 1.0,
            turn: Turn::NONE,
            tip: Some(0),
            flip,
            hardness: 1.0,
            color: [0.0; 3],
            texture_strength: 1.0,
            wet: [1.0; 3],
            index: 0,
        }
    }

    #[test]
    fn a_flipped_picture_is_its_mirror_image() {
        // A picture solid on its left half only.
        let texels: Vec<f32> = (0..16 * 16)
            .map(|i| if i % 16 < 8 { 1.0 } else { 0.0 })
            .collect();
        let picture = TipImage::from_texels(16, 16, texels).expect("a picture");
        let pictures = [picture];
        let tips = Tips {
            pictures: &pictures,
            edge: 1.0,
        };
        let plain = Stamp::new(&planned([false, false]), &tips, None);
        let flipped = Stamp::new(&planned([true, false]), &tips, None);
        assert!(plain.coverage([14.5, 20.5]) > 0.9);
        assert!(plain.coverage([25.5, 20.5]) < 0.1);
        assert!(flipped.coverage([14.5, 20.5]) < 0.1);
        assert!(flipped.coverage([25.5, 20.5]) > 0.9);
        // Up and down leaves a left and right picture as it is.
        let upside = Stamp::new(&planned([false, true]), &tips, None);
        assert!(upside.coverage([14.5, 20.5]) > 0.9);
    }

    #[test]
    fn a_round_dab_uses_its_planned_hardness() {
        let tips = Tips::round(1.0);
        let mut soft = planned([false; 2]);
        soft.tip = None;
        soft.hardness = 0.0;
        let mut hard = soft;
        hard.hardness = 1.0;
        let at = [27.5, 20.5];
        assert!(
            Stamp::new(&soft, &tips, None).coverage(at)
                < Stamp::new(&hard, &tips, None).coverage(at)
        );
    }
}
