//! One animation frame's view of the layers: the cels it hides, the ones
//! shown faintly around it, and what its camera folders see.

use std::collections::HashSet;

use sue_core::{Arena, EntityId, PixelFormat, RasterTile2D};
use sue_model::picture;

use super::gather::{gather_view, prepare};
use super::rows::{pixel, rows};
use super::{Compositor, RenderOptions};

/// How an animation frame shows the layers: the cels it hides, and the
/// ones shown faintly around it (onion skin, the light table).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameView {
    /// Cels not on this frame.
    pub hidden: HashSet<EntityId>,
    /// Cels shown anyway.
    pub onion: Vec<Skin>,
    /// Camera folders and what each canvas point sees through them (an
    /// affine map, `camera::Camera::map`).
    pub cameras: Vec<(EntityId, [f32; 6])>,
}

/// A hidden cel shown anyway: in a color, faintly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skin {
    /// The cel.
    pub cel: EntityId,
    /// Its skin color (sRGB, 0 to 1).
    pub color: [f32; 3],
    /// How much of its own color gives way to the skin color: 1 for
    /// Monochrome, a half for Half color, 0 for Color.
    pub tint: f32,
    /// How much shows, 0 to 1.
    pub opacity: f32,
    /// What each canvas point sees on the layer, when a light table moved
    /// it.
    pub place: Option<[f32; 6]>,
    /// Shown even while the layer itself is hidden (a file only on the
    /// light table).
    pub force: bool,
}

/// As [`render`], for one animation frame's `view` of the layers.
#[must_use]
pub fn render_view(
    arena: &Arena,
    picture: EntityId,
    [x0, y0, x1, y1]: [u32; 4],
    options: RenderOptions,
    view: Option<&FrameView>,
) -> Vec<u8> {
    let mut layers = gather_view(arena, picture, options, view);
    prepare(&mut layers, [x0, y0, x1, y1]);
    let (width, height) = (
        x1.saturating_sub(x0) as usize,
        y1.saturating_sub(y0) as usize,
    );
    let mut out = vec![0_u8; width * height * 4];
    rows(&mut out, width * 4, y0, width * height, |y, row| {
        for (x, slot) in (x0..x1).zip(row.as_chunks_mut::<4>().0) {
            *slot = pixel(&layers, x, y).map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    });
    out
}

/// As [`flatten`], for one animation frame's `view` of the layers.
#[must_use]
pub fn flatten_view(
    arena: &Arena,
    picture: EntityId,
    options: RenderOptions,
    view: Option<&FrameView>,
) -> Option<RasterTile2D> {
    let [width, height] = picture::size(arena.get_by_id(picture).ok()?)?;
    let pixels = render_view(arena, picture, [0, 0, width, height], options, view);
    RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels).ok()
}

impl Compositor {
    /// Shows the layers as animation frame `view` has them (`None`: all
    /// of them); a change is composited on the next [`Self::sync`].
    pub fn set_view(&mut self, view: Option<FrameView>) {
        if self.view != view {
            self.view = view;
            self.seen.clear();
            self.focus = None;
        }
    }
}
