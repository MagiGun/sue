//! A finished stroke laid on the layer: in the brush's blend mode,
//! behind what the layer holds, or as an eraser on one layer or on many.
//!
//! The blend modes are the picture model's ([`BlendMode::mix`]), applied
//! the way the W3C compositing model applies a layer's mode: where the
//! layer is empty the paint is laid as it is, where it is opaque the
//! paint's color is mixed with the layer's first, and the result goes
//! over the layer by the stroke's coverage. A pixel the stroke does not
//! reach keeps its exact samples.

use sue_core::{PixelFormat, RasterTile2D};
use sue_model::BlendMode;

use super::finish::Field;
use super::math::unit;
use crate::buffer::MergeError;
use crate::raster::PixelRect;
use crate::raster::kernel::Channel;

/// How a stroke meets the layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lay {
    /// Paint of `color` (or the field's own colors), as opaque as
    /// `opacity` at full coverage, mixed in `mode`.
    Paint {
        /// The color, straight RGB from 0 to 1.
        color: [f32; 3],
        /// The stroke's opacity, 0 to 1.
        opacity: f32,
        /// The brush's blend mode.
        mode: BlendMode,
    },
    /// Paint that goes behind what the layer already holds.
    Behind {
        /// The color.
        color: [f32; 3],
        /// The stroke's opacity.
        opacity: f32,
    },
    /// Takes away `opacity` of the layer's alpha at full coverage.
    Erase {
        /// The stroke's opacity.
        opacity: f32,
    },
}

/// One pixel of the layer, straight, as floats.
type Pixel = [f32; 4];

impl Lay {
    fn opacity(&self) -> f32 {
        match *self {
            Self::Paint { opacity, .. }
            | Self::Behind { opacity, .. }
            | Self::Erase { opacity } => unit(opacity),
        }
    }

    /// The pixel `dst` after the stroke laid `share` (coverage times
    /// opacity) of `color` on it, darkened by `dark`; with `keep_alpha`
    /// (lock transparent pixels) the layer's alpha does not change.
    fn pixel(&self, dst: Pixel, share: f32, color: [f32; 3], dark: f32, keep_alpha: bool) -> Pixel {
        let color = color.map(|c| unit(c) * (1.0 - unit(dark)));
        let [br, bg, bb, ab] = dst;
        let backdrop = [br, bg, bb];
        match *self {
            Self::Paint { mode, .. } => {
                let mixed = mode.mix(backdrop, color);
                // The paint's color where it meets the layer: its own over
                // nothing, the mix over the layer.
                let cs: [f32; 3] =
                    std::array::from_fn(|i| (1.0 - ab) * color[i] + ab * unit(mixed[i]));
                let a = share + ab * (1.0 - share);
                if keep_alpha {
                    let c: [f32; 3] =
                        std::array::from_fn(|i| backdrop[i] + (cs[i] - backdrop[i]) * share);
                    [c[0], c[1], c[2], ab]
                } else if a > 0.0 {
                    let c: [f32; 3] = std::array::from_fn(|i| {
                        (cs[i] * share + backdrop[i] * ab * (1.0 - share)) / a
                    });
                    [c[0], c[1], c[2], a]
                } else {
                    dst
                }
            }
            Self::Behind { .. } => {
                if keep_alpha {
                    dst
                } else {
                    let a = ab + share * (1.0 - ab);
                    if a > 0.0 {
                        let c: [f32; 3] = std::array::from_fn(|i| {
                            (backdrop[i] * ab + color[i] * share * (1.0 - ab)) / a
                        });
                        [c[0], c[1], c[2], a]
                    } else {
                        dst
                    }
                }
            }
            Self::Erase { .. } => {
                if keep_alpha {
                    dst
                } else {
                    [br, bg, bb, ab * (1.0 - share)]
                }
            }
        }
    }

    /// The ink color of a paint, or black for an eraser.
    fn color(&self) -> [f32; 3] {
        match *self {
            Self::Paint { color, .. } | Self::Behind { color, .. } => color,
            Self::Erase { .. } => [0.0; 3],
        }
    }
}

/// Lays `field` on `raster`, whose pixel `(0, 0)` is layer pixel
/// `origin`, as `lay` says; with `keep_alpha` the layer's transparent
/// pixels stay so (Clip Studio's "lock transparent pixels"). Returns the
/// rectangle of layer pixels it may have changed (what an undo record
/// keeps), or `None` when the field misses the raster.
///
/// # Errors
/// [`MergeError::Format`] for a raster with no color (`Gray8`).
pub fn lay_field(
    field: &Field,
    raster: &mut RasterTile2D,
    origin: [i32; 2],
    lay: Lay,
    keep_alpha: bool,
) -> Result<Option<PixelRect>, MergeError> {
    let (width, height) = (raster.width() as i32, raster.height() as i32);
    match raster.format() {
        PixelFormat::Rgba8 => {
            let Some(bytes) = raster.bytes_mut() else {
                return Err(MergeError::Format(PixelFormat::Rgba8));
            };
            Ok(lay_samples(
                field, bytes, width, height, origin, lay, keep_alpha,
            ))
        }
        PixelFormat::RgbaF32 => {
            let Some(samples) = raster.f32_samples_mut() else {
                return Err(MergeError::Format(PixelFormat::RgbaF32));
            };
            Ok(lay_samples(
                field, samples, width, height, origin, lay, keep_alpha,
            ))
        }
        other => Err(MergeError::Format(other)),
    }
}

/// [`lay_field`] over samples of any type.
fn lay_samples<C: Channel>(
    field: &Field,
    samples: &mut [C],
    width: i32,
    height: i32,
    origin: [i32; 2],
    lay: Lay,
    keep_alpha: bool,
) -> Option<PixelRect> {
    let r = field.rect;
    let x0 = r.x0.max(origin[0]);
    let y0 = r.y0.max(origin[1]);
    let x1 = r.x1.min(origin[0] + width);
    let y1 = r.y1.min(origin[1] + height);
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    let opacity = lay.opacity();
    let ink = lay.color();
    let fw = field.width();
    for y in y0..y1 {
        for x in x0..x1 {
            let fi = (y - r.y0) as usize * fw + (x - r.x0) as usize;
            let a = field.alpha[fi];
            if a.is_nan() || a <= 0.0 {
                continue;
            }
            let color = field.color.as_ref().map_or(ink, |c| c[fi]);
            let i = (((y - origin[1]) * width + (x - origin[0])) * 4) as usize;
            let px = &mut samples[i..i + 4];
            let dst = [
                px[0].to_unit(),
                px[1].to_unit(),
                px[2].to_unit(),
                px[3].to_unit(),
            ];
            let out = lay.pixel(dst, unit(a) * opacity, color, field.dark[fi], keep_alpha);
            // An empty pixel keeps its color samples.
            if out[3] > 0.0 || matches!(lay, Lay::Erase { .. }) {
                for (s, v) in px.iter_mut().zip(out) {
                    *s = C::from_unit(v);
                }
            }
        }
    }
    Some(PixelRect { x0, y0, x1, y1 })
}

/// One layer an eraser is handed: its raster, where its pixel `(0, 0)`
/// lies, and whether its transparent pixels are locked.
#[derive(Debug)]
pub struct EraseTarget<'a> {
    /// The layer's pixels.
    pub raster: &'a mut RasterTile2D,
    /// The layer pixel of the raster's `(0, 0)`.
    pub origin: [i32; 2],
}

/// The eraser on every layer of `targets` at once (Clip Studio's
/// "Multiple layers" eraser): the same stroke, erased from each. Returns
/// the rectangle each layer may have changed, in the order given.
///
/// # Errors
/// [`MergeError::Format`] when a layer holds no color; no layer is
/// changed then, so the stroke is all or nothing.
pub fn erase_layers(
    field: &Field,
    targets: &mut [EraseTarget<'_>],
    opacity: f32,
) -> Result<Vec<Option<PixelRect>>, MergeError> {
    for target in targets.iter() {
        let format = target.raster.format();
        if !matches!(format, PixelFormat::Rgba8 | PixelFormat::RgbaF32) {
            return Err(MergeError::Format(format));
        }
    }
    targets
        .iter_mut()
        .map(|t| lay_field(field, t.raster, t.origin, Lay::Erase { opacity }, false))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(alpha: f32) -> Field {
        let mut f = Field::empty(PixelRect {
            x0: 0,
            y0: 0,
            x1: 2,
            y1: 1,
        });
        f.alpha = vec![alpha, 0.0];
        f
    }

    fn raster(pixel: [u8; 4]) -> RasterTile2D {
        let mut r = RasterTile2D::new(2, 1, PixelFormat::Rgba8).expect("a raster");
        let bytes = r.bytes_mut().expect("bytes");
        bytes[..4].copy_from_slice(&pixel);
        bytes[4..].copy_from_slice(&pixel);
        r
    }

    fn first(r: &RasterTile2D) -> [u8; 4] {
        let b = r.bytes().expect("bytes");
        [b[0], b[1], b[2], b[3]]
    }

    #[test]
    fn multiply_darkens_and_screen_lightens_an_opaque_layer() {
        let gray = [128, 128, 128, 255];
        let mut r = raster(gray);
        let paint = |mode| Lay::Paint {
            color: [0.5, 0.5, 0.5],
            opacity: 1.0,
            mode,
        };
        lay_field(
            &field(1.0),
            &mut r,
            [0, 0],
            paint(BlendMode::Multiply),
            false,
        )
        .expect("laid");
        assert_eq!(first(&r), [64, 64, 64, 255]);
        let mut r = raster(gray);
        lay_field(&field(1.0), &mut r, [0, 0], paint(BlendMode::Screen), false).expect("laid");
        assert_eq!(first(&r), [192, 192, 192, 255]);
        // The pixel the stroke does not reach keeps its samples.
        assert_eq!(r.bytes().expect("bytes")[4..], gray);
    }

    #[test]
    fn on_an_empty_layer_every_mode_lays_the_paint_itself() {
        for mode in BlendMode::ALL {
            let mut r = raster([0, 0, 0, 0]);
            let lay = Lay::Paint {
                color: [1.0, 0.5, 0.0],
                opacity: 1.0,
                mode,
            };
            lay_field(&field(1.0), &mut r, [0, 0], lay, false).expect("laid");
            assert_eq!(first(&r), [255, 128, 0, 255], "{mode:?}");
        }
    }

    #[test]
    fn behind_fills_only_what_the_layer_leaves_open() {
        let mut r = raster([255, 0, 0, 128]);
        let lay = Lay::Behind {
            color: [0.0, 0.0, 1.0],
            opacity: 1.0,
        };
        lay_field(&field(1.0), &mut r, [0, 0], lay, false).expect("laid");
        let [red, _, blue, a] = first(&r);
        assert_eq!(a, 255);
        assert!((red as i32 - 128).abs() <= 1 && (blue as i32 - 127).abs() <= 1);
        // Over an opaque pixel it shows nothing.
        let mut solid = raster([10, 20, 30, 255]);
        lay_field(&field(1.0), &mut solid, [0, 0], lay, false).expect("laid");
        assert_eq!(first(&solid), [10, 20, 30, 255]);
    }

    #[test]
    fn locked_transparent_pixels_keep_their_alpha() {
        let mut r = raster([255, 255, 255, 100]);
        let lay = Lay::Paint {
            color: [0.0, 0.0, 0.0],
            opacity: 1.0,
            mode: BlendMode::Normal,
        };
        lay_field(&field(1.0), &mut r, [0, 0], lay, true).expect("laid");
        assert_eq!(first(&r), [0, 0, 0, 100]);
    }

    #[test]
    fn the_multiple_layers_eraser_erases_every_layer_or_none() {
        let mut a = raster([10, 20, 30, 255]);
        let mut b = raster([40, 50, 60, 200]);
        let changed = erase_layers(
            &field(1.0),
            &mut [
                EraseTarget {
                    raster: &mut a,
                    origin: [0, 0],
                },
                EraseTarget {
                    raster: &mut b,
                    origin: [0, 0],
                },
            ],
            0.5,
        )
        .expect("erased");
        assert_eq!(changed.len(), 2);
        assert_eq!(first(&a), [10, 20, 30, 128]);
        assert_eq!(first(&b), [40, 50, 60, 100]);
        let mut c = raster([1, 2, 3, 255]);
        let mut gray = RasterTile2D::new(2, 1, PixelFormat::Gray8).expect("a mask");
        let failed = erase_layers(
            &field(1.0),
            &mut [
                EraseTarget {
                    raster: &mut c,
                    origin: [0, 0],
                },
                EraseTarget {
                    raster: &mut gray,
                    origin: [0, 0],
                },
            ],
            1.0,
        );
        assert!(failed.is_err());
        assert_eq!(
            first(&c),
            [1, 2, 3, 255],
            "nothing erased when one layer cannot be"
        );
    }

    #[test]
    fn the_watercolor_rim_lays_darker_paint() {
        let mut f = field(1.0);
        f.dark = vec![0.5, 0.0];
        let mut r = raster([255, 255, 255, 255]);
        let lay = Lay::Paint {
            color: [0.8, 0.8, 0.8],
            opacity: 1.0,
            mode: BlendMode::Normal,
        };
        lay_field(&f, &mut r, [0, 0], lay, false).expect("laid");
        assert_eq!(first(&r), [102, 102, 102, 255]);
    }
}
