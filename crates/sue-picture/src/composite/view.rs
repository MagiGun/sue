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
    /// Layers and folders moved, turned or scaled by their keyframes at
    /// this frame: what each canvas point sees on them (an affine map, as
    /// for cameras).
    pub moves: Vec<(EntityId, [f32; 6])>,
    /// Layers and folders whose opacity their keyframes set at this frame
    /// (0 to 1), in place of their own.
    pub opacities: Vec<(EntityId, f32)>,
}

/// `outer` followed by `inner`: the map that sees through `outer` first,
/// then through `inner` (each `[a, b, c, d, tx, ty]`, a point `(x, y)`
/// seeing `(a x + b y + tx, c x + d y + ty)`).
#[must_use]
pub fn compose(outer: [f32; 6], inner: [f32; 6]) -> [f32; 6] {
    let [a0, a1, a2, a3, a4, a5] = inner;
    let [b0, b1, b2, b3, b4, b5] = outer;
    [
        a0 * b0 + a1 * b2,
        a0 * b1 + a1 * b3,
        a2 * b0 + a3 * b2,
        a2 * b1 + a3 * b3,
        a0 * b4 + a1 * b5 + a4,
        a2 * b4 + a3 * b5 + a5,
    ]
}

impl FrameView {
    /// The opacity the keyframes give `id` at this frame, if they set one.
    #[must_use]
    pub fn opacity_of(&self, id: EntityId) -> Option<f32> {
        self.opacities
            .iter()
            .find(|(layer, _)| *layer == id)
            .map(|(_, opacity)| *opacity)
    }

    /// What each canvas point sees on layer `id`: through every camera
    /// folder and keyed folder around it, outermost first, then through
    /// its own keyframes. `None` when nothing moves it.
    #[must_use]
    pub fn warp(&self, arena: &Arena, id: EntityId) -> Option<[f32; 6]> {
        if self.cameras.is_empty() && self.moves.is_empty() {
            return None;
        }
        let of = |layer: EntityId| {
            let camera = self
                .cameras
                .iter()
                .find(|(folder, _)| *folder == layer)
                .map(|(_, map)| *map);
            let moved = self
                .moves
                .iter()
                .find(|(moved, _)| *moved == layer)
                .map(|(_, map)| *map);
            match (camera, moved) {
                (Some(camera), Some(moved)) => Some(compose(camera, moved)),
                (camera, moved) => camera.or(moved),
            }
        };
        let mut out: Option<[f32; 6]> = None;
        let mut around = crate::doc::folders_around(arena, id);
        around.reverse();
        around.push(id);
        for layer in around {
            if let Some(map) = of(layer) {
                out = Some(match out {
                    Some(outer) => compose(outer, map),
                    None => map,
                });
            }
        }
        out
    }

    /// The same with every layer in `ids` hidden as well (a tool's own
    /// exclusions, as Fill and Auto Select take them).
    #[must_use]
    pub fn hiding(mut self, ids: impl IntoIterator<Item = EntityId>) -> Self {
        self.hidden.extend(ids);
        self
    }
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

/// As [`render`](super::render), for one animation frame's `view` of the
/// layers.
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

/// As [`flatten`](super::flatten), for one animation frame's `view` of
/// the layers.
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
    /// How the layers show now: the animation frame's view, if one is set.
    #[must_use]
    pub fn view(&self) -> Option<&FrameView> {
        self.view.as_ref()
    }

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
