//! Area fills: the paint bucket's flood fill, masked fills and gradients.
//! They work on a copy of a layer's pixels (straight RGBA8); the studio
//! turns the change into one undo step with [`crate::doc::rewrite`].
//!
//! Which pixels a Fill or Auto Select looks at is [`reference`]'s work
//! ([`Refer`], [`Exclude`]); which of them it takes is [`area`]'s
//! ([`AreaSettings`]).

use sue_core::{Arena, EntityId};

use crate::brush::AntiAlias;
use crate::composite::{self, FrameView, RenderOptions};
use crate::doc;
use crate::select::Selection;

// ----- What a fill looks at ----------------------------------------------------------

/// The layers a Fill or Auto Select looks at (Clip Studio Paint's Refer
/// multiple, and its single layer when that is off).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Refer {
    /// The layer being edited only.
    #[default]
    Editing,
    /// Every shown layer.
    All,
    /// The layers picked in the Layer palette, the editing one with them.
    Selected,
    /// The shown layers in the folder the editing layer is in.
    Folder,
    /// The shown reference layers.
    Reference,
}

impl Refer {
    /// Every choice, in Tool Property's order.
    pub const ALL: [Self; 5] = [
        Self::Editing,
        Self::All,
        Self::Selected,
        Self::Folder,
        Self::Reference,
    ];

    /// Its stable name in saved settings.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Editing => "editing",
            Self::All => "all",
            Self::Selected => "selected",
            Self::Folder => "folder",
            Self::Reference => "reference",
        }
    }

    /// The choice named `id`.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|refer| refer.id() == id)
    }

    /// What people read.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Editing => tr!("editing-layer"),
            Self::All => tr!("all-layers"),
            Self::Selected => tr!("refer-selected-layers"),
            Self::Folder => tr!("refer-layers-in-folder"),
            Self::Reference => tr!("reference-layer"),
        }
    }
}

/// Layers a fill that looks at several layers leaves out (Clip Studio
/// Paint's exclusions, next to Refer multiple).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(clippy::struct_excessive_bools, reason = "one switch each, as shown")]
pub struct Exclude {
    /// Draft layers.
    pub drafts: bool,
    /// Text layers.
    pub text: bool,
    /// Paper layers.
    pub paper: bool,
    /// Locked layers.
    pub locked: bool,
}

impl Default for Exclude {
    /// As Sue Paint always looked: paper left out, everything else in.
    fn default() -> Self {
        Self {
            drafts: false,
            text: false,
            paper: true,
            locked: false,
        }
    }
}

/// What [`referred`] needs to know of a layer.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "what a layer is, flag by flag"
)]
pub struct ReferLayer {
    /// The layer.
    pub id: EntityId,
    /// A draft layer.
    pub draft: bool,
    /// A text layer.
    pub text: bool,
    /// A paper layer.
    pub paper: bool,
    /// Locked against edits.
    pub locked: bool,
    /// A reference layer.
    pub reference: bool,
    /// A folder (looked at only through its members).
    pub folder: bool,
    /// The folders around it, innermost first.
    pub around: Vec<EntityId>,
}

impl ReferLayer {
    /// The layers of `picture` as [`referred`] sees them, bottom first.
    #[must_use]
    pub fn of_picture(arena: &Arena, picture: EntityId) -> Vec<Self> {
        doc::layers(arena, picture)
            .into_iter()
            .map(|info| Self {
                id: info.id,
                draft: info.style.draft,
                text: arena
                    .get_by_id(info.id)
                    .is_ok_and(|entity| entity.class() == doc::TEXT),
                paper: info.kind == sue_model::picture::LayerKind::Paper,
                locked: info.locked,
                reference: doc::is_reference(arena, info.id),
                folder: doc::is_folder(arena, info.id),
                around: doc::folders_around(arena, info.id),
            })
            .collect()
    }
}

/// The layers of `layers` a fill refers to, in their order: by `refer`,
/// from the editing layer `editing` with `picked` picked beside it, less
/// what `exclude` leaves out (when it looks at more than one). Folders
/// never count themselves; their members do.
#[must_use]
pub fn referred(
    layers: &[ReferLayer],
    editing: Option<EntityId>,
    picked: &[EntityId],
    refer: Refer,
    exclude: Exclude,
) -> Vec<EntityId> {
    let home = layers
        .iter()
        .find(|layer| Some(layer.id) == editing)
        .and_then(|layer| layer.around.first().copied());
    // A reference folder (an animation folder set as the reference, say)
    // makes every layer in it a reference layer.
    let reference_folders: Vec<EntityId> = layers
        .iter()
        .filter(|layer| layer.folder && layer.reference)
        .map(|layer| layer.id)
        .collect();
    let wanted = |layer: &ReferLayer| match refer {
        Refer::Editing => Some(layer.id) == editing,
        Refer::All => true,
        Refer::Selected => Some(layer.id) == editing || picked.contains(&layer.id),
        Refer::Folder => match home {
            Some(folder) => layer.around.contains(&folder),
            None => layer.around.is_empty(),
        },
        Refer::Reference => {
            layer.reference
                || layer
                    .around
                    .iter()
                    .any(|folder| reference_folders.contains(folder))
        }
    };
    let left_out = |layer: &ReferLayer| {
        refer != Refer::Editing
            && ((exclude.drafts && layer.draft)
                || (exclude.text && layer.text)
                || (exclude.paper && layer.paper)
                || (exclude.locked && layer.locked))
    };
    layers
        .iter()
        .filter(|layer| !layer.folder && wanted(layer) && !left_out(layer))
        .map(|layer| layer.id)
        .collect()
}

/// What a fill looks at in `picture`, as straight RGBA8 picture pixels
/// (`size`): the editing layer's own pixels in place, or the referred
/// layers as they show together. `None` when there is no editing layer to
/// look at.
#[must_use]
pub fn reference(
    arena: &Arena,
    picture: EntityId,
    size: [u32; 2],
    editing: Option<EntityId>,
    picked: &[EntityId],
    refer: Refer,
    exclude: Exclude,
) -> Option<Vec<u8>> {
    reference_in(arena, picture, size, editing, picked, refer, exclude, None)
}

/// [`reference`] at one frame of an animation: `frame` is what the frame
/// shows (its cels, and the layers its keyframes move or fade), so a fill
/// that looks at several layers sees only the cels of that frame.
#[must_use]
#[allow(
    clippy::too_many_arguments,
    reason = "reference's setting and the frame"
)]
pub fn reference_in(
    arena: &Arena,
    picture: EntityId,
    size: [u32; 2],
    editing: Option<EntityId>,
    picked: &[EntityId],
    refer: Refer,
    exclude: Exclude,
    frame: Option<&FrameView>,
) -> Option<Vec<u8>> {
    if refer == Refer::Editing {
        return layer_on_picture(arena, editing?, size);
    }
    let layers = ReferLayer::of_picture(arena, picture);
    let keep = referred(&layers, editing, picked, refer, exclude);
    let mut view = frame.cloned().unwrap_or_default();
    view.hidden.extend(
        layers
            .iter()
            .filter(|layer| !layer.folder && !keep.contains(&layer.id))
            .map(|layer| layer.id),
    );
    Some(composite::render_view(
        arena,
        picture,
        [0, 0, size[0], size[1]],
        RenderOptions {
            paper: !exclude.paper,
            drafts: !exclude.drafts,
        },
        Some(&view),
    ))
}

/// The pixels of raster layer `layer` laid where they sit in a picture of
/// `size`, clear around them.
fn layer_on_picture(arena: &Arena, layer: EntityId, size: [u32; 2]) -> Option<Vec<u8>> {
    let raster = doc::raster(arena, layer)?;
    let offset = arena
        .get_by_id(layer)
        .map(|entity| sue_model::picture::LayerStyle::of(entity).offset)
        .unwrap_or([0, 0]);
    let [width, height] = size;
    let (lw, lh) = (raster.width(), raster.height());
    let bytes = raster.bytes()?;
    if offset == [0, 0] && [lw, lh] == size {
        return Some(bytes.to_vec());
    }
    let mut out = vec![0; width as usize * height as usize * 4];
    for y in 0..lh {
        let py = i64::from(y) + offset[1];
        if py < 0 || py >= i64::from(height) {
            continue;
        }
        for x in 0..lw {
            let px = i64::from(x) + offset[0];
            if px < 0 || px >= i64::from(width) {
                continue;
            }
            let from = ((y * lw + x) * 4) as usize;
            let to = ((py as u32 * width + px as u32) * 4) as usize;
            out[to..to + 4].copy_from_slice(&bytes[from..from + 4]);
        }
    }
    Some(out)
}

// ----- Which pixels it takes ---------------------------------------------------------

/// How a Fill or Auto Select takes its area from what it looks at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AreaSettings {
    /// How different a color may be and still count, 0 to 1.
    pub tolerance: f32,
    /// Close gap level, 0 (none) to 4 ([`gap_radius`]).
    pub gap: u32,
    /// Area scaling in pixels: above nothing it spreads under the lines,
    /// below nothing it keeps back from them.
    pub spread: i32,
    /// Only pixels joined to the one clicked (Apply to connected pixels
    /// only); off, every like pixel of the picture.
    pub connected_only: bool,
    /// The edge of the area, hard or softened.
    pub antialias: AntiAlias,
}

impl Default for AreaSettings {
    fn default() -> Self {
        Self {
            tolerance: 0.0,
            gap: 0,
            spread: 0,
            connected_only: true,
            antialias: AntiAlias::None,
        }
    }
}

/// The area a click at `seed` takes in `reference` (straight RGBA8 rows of
/// `size`), as one coverage byte a pixel.
#[must_use]
pub fn area(reference: &[u8], size: [u32; 2], seed: [u32; 2], settings: &AreaSettings) -> Vec<u8> {
    let [width, height] = size;
    let count = width as usize * height as usize;
    if seed[0] >= width || seed[1] >= height || reference.len() < count * 4 {
        return vec![0; count];
    }
    let hard = if settings.connected_only {
        flood_closing(
            reference,
            size,
            seed,
            settings.tolerance,
            gap_radius(settings.gap),
            settings.spread,
        )
    } else {
        let like = like_pixels(reference, size, seed, settings.tolerance);
        scale_area(&like, width, height, settings.spread)
            .into_iter()
            .map(|inside| if inside { 255 } else { 0 })
            .collect()
    };
    antialias(&hard, width, height, settings.antialias)
}

/// Every pixel of `reference` like the one at `seed`, joined to it or not.
fn like_pixels(reference: &[u8], size: [u32; 2], seed: [u32; 2], tolerance: f32) -> Vec<bool> {
    let premultiplied = |index: usize| {
        let p = &reference[index * 4..index * 4 + 4];
        let alpha = u32::from(p[3]);
        [
            u32::from(p[0]) * alpha / 255,
            u32::from(p[1]) * alpha / 255,
            u32::from(p[2]) * alpha / 255,
            alpha,
        ]
    };
    let target = premultiplied((seed[1] * size[0] + seed[0]) as usize);
    let limit = (tolerance.clamp(0.0, 1.0) * 255.0).round() as u32;
    (0..size[0] as usize * size[1] as usize)
        .map(|index| {
            premultiplied(index)
                .iter()
                .zip(target)
                .all(|(a, b)| a.abs_diff(b) <= limit)
        })
        .collect()
}

/// `mask` with its edge softened: not at all for [`AntiAlias::None`],
/// then over one, two or three pixels.
#[must_use]
pub fn antialias(mask: &[u8], width: u32, height: u32, level: AntiAlias) -> Vec<u8> {
    let passes = match level {
        AntiAlias::None => 0,
        AntiAlias::Weak => 1,
        AntiAlias::Medium => 2,
        AntiAlias::Strong => 3,
    };
    let (w, h) = (width as usize, height as usize);
    let mut out = mask.to_vec();
    if w == 0 || h == 0 {
        return out;
    }
    for _ in 0..passes {
        let from = out.clone();
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0_u32;
                let mut n = 0_u32;
                for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                    for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                        sum += u32::from(from[ny * w + nx]);
                        n += 1;
                    }
                }
                out[y * w + x] = ((sum + n / 2) / n) as u8;
            }
        }
    }
    out
}

/// The area a flood fill from `seed` reaches in `reference` (straight RGBA8
/// rows of `size`): one coverage byte per pixel. Pixels count as the same
/// when no premultiplied channel differs by more than `tolerance` (0 to 1).
/// The area grows by one pixel so it tucks under anti-aliased edges.
#[must_use]
pub fn flood(reference: &[u8], size: [u32; 2], seed: [u32; 2], tolerance: f32) -> Vec<u8> {
    let [width, height] = size;
    let mask = flood_joined(reference, size, seed, tolerance);
    if mask.len() < width as usize * height as usize || width == 0 || height == 0 {
        return mask;
    }
    grow(&mask, width, height)
}

/// The pixels joined to `seed` that are like it, one byte a pixel, not
/// yet grown under the edge.
fn flood_joined(reference: &[u8], size: [u32; 2], seed: [u32; 2], tolerance: f32) -> Vec<u8> {
    let [width, height] = size;
    let count = width as usize * height as usize;
    let mut mask = vec![0_u8; count];
    if seed[0] >= width || seed[1] >= height || reference.len() < count * 4 {
        return mask;
    }
    let premultiplied = |index: usize| {
        let p = &reference[index * 4..index * 4 + 4];
        let alpha = u32::from(p[3]);
        [
            u32::from(p[0]) * alpha / 255,
            u32::from(p[1]) * alpha / 255,
            u32::from(p[2]) * alpha / 255,
            alpha,
        ]
    };
    let target = premultiplied((seed[1] * width + seed[0]) as usize);
    let limit = (tolerance.clamp(0.0, 1.0) * 255.0).round() as u32;
    let same = |index: usize| {
        premultiplied(index)
            .iter()
            .zip(target)
            .all(|(a, b)| a.abs_diff(b) <= limit)
    };
    let mut stack = vec![(seed[0], seed[1])];
    while let Some((x, y)) = stack.pop() {
        let row = (y * width) as usize;
        if mask[row + x as usize] != 0 || !same(row + x as usize) {
            continue;
        }
        let mut left = x;
        while left > 0 && mask[row + left as usize - 1] == 0 && same(row + left as usize - 1) {
            left -= 1;
        }
        let mut right = x;
        while right + 1 < width
            && mask[row + right as usize + 1] == 0
            && same(row + right as usize + 1)
        {
            right += 1;
        }
        for fill in left..=right {
            mask[row + fill as usize] = 255;
            for ny in [y.wrapping_sub(1), y + 1] {
                if ny < height {
                    let below = (ny * width + fill) as usize;
                    if mask[below] == 0 && same(below) {
                        stack.push((fill, ny));
                    }
                }
            }
        }
    }
    mask
}

/// How far a Close gap level (0 none, then weak to strongest) reaches,
/// in pixels.
#[must_use]
pub const fn gap_radius(level: u32) -> u32 {
    match level {
        0 => 0,
        1 => 2,
        2 => 4,
        3 => 7,
        _ => 11,
    }
}

/// `mask` (one flag a pixel) grown by `radius` pixels, a square at a time.
fn dilate(mask: &[bool], width: u32, height: u32, radius: u32) -> Vec<bool> {
    if radius == 0 {
        return mask.to_vec();
    }
    let (w, h, r) = (width as usize, height as usize, radius as usize);
    let mut across = vec![false; mask.len()];
    for y in 0..h {
        let row = &mask[y * w..(y + 1) * w];
        let mut prefix = vec![0_u32; w + 1];
        for x in 0..w {
            prefix[x + 1] = prefix[x] + u32::from(row[x]);
        }
        for x in 0..w {
            let (low, high) = (x.saturating_sub(r), (x + r + 1).min(w));
            across[y * w + x] = prefix[high] > prefix[low];
        }
    }
    let mut out = vec![false; mask.len()];
    for x in 0..w {
        let mut prefix = vec![0_u32; h + 1];
        for y in 0..h {
            prefix[y + 1] = prefix[y] + u32::from(across[y * w + x]);
        }
        for y in 0..h {
            let (low, high) = (y.saturating_sub(r), (y + r + 1).min(h));
            out[y * w + x] = prefix[high] > prefix[low];
        }
    }
    out
}

/// A flood fill that closes gaps in lines (Clip Studio Paint's Close gap)
/// and spreads under them (Area scaling): the lines (pixels that are not
/// like the seed) count as `gap` pixels thicker while flooding, so small
/// breaks hold the fill in; the area then grows back up to the lines it
/// was kept from, and `spread` pixels further, under them.
#[must_use]
pub fn flood_closing(
    reference: &[u8],
    size: [u32; 2],
    seed: [u32; 2],
    tolerance: f32,
    gap: u32,
    spread: i32,
) -> Vec<u8> {
    let plain = flood(reference, size, seed, tolerance);
    let [width, height] = size;
    if gap == 0 && spread == 0 {
        return plain;
    }
    // The area itself, before the one pixel tucked under the lines that
    // the spread below adds, so a spread of n reaches n pixels further.
    let joined = flood_joined(reference, size, seed, tolerance);
    let count = (width * height) as usize;
    let premultiplied = |index: usize| {
        let p = &reference[index * 4..index * 4 + 4];
        let alpha = u32::from(p[3]);
        [
            u32::from(p[0]) * alpha / 255,
            u32::from(p[1]) * alpha / 255,
            u32::from(p[2]) * alpha / 255,
            alpha,
        ]
    };
    if seed[0] >= width || seed[1] >= height || reference.len() < count * 4 {
        return plain;
    }
    let target = premultiplied((seed[1] * width + seed[0]) as usize);
    let limit = (tolerance.clamp(0.0, 1.0) * 255.0).round() as u32;
    let open: Vec<bool> = (0..count)
        .map(|index| {
            premultiplied(index)
                .iter()
                .zip(target)
                .all(|(a, b)| a.abs_diff(b) <= limit)
        })
        .collect();
    let mut region: Vec<bool> = if gap > 0 {
        // Flood where even the thickened lines leave room.
        let walls: Vec<bool> = open.iter().map(|open| !open).collect();
        let blocked = dilate(&walls, width, height, gap);
        let start = (seed[1] * width + seed[0]) as usize;
        if blocked[start] {
            joined.iter().map(|value| *value != 0).collect()
        } else {
            let mut region = vec![false; count];
            let mut stack = vec![start];
            while let Some(index) = stack.pop() {
                if region[index] || blocked[index] {
                    continue;
                }
                region[index] = true;
                let (x, y) = (index as u32 % width, index as u32 / width);
                if x > 0 {
                    stack.push(index - 1);
                }
                if x + 1 < width {
                    stack.push(index + 1);
                }
                if y > 0 {
                    stack.push(index - width as usize);
                }
                if y + 1 < height {
                    stack.push(index + width as usize);
                }
            }
            // Back out to the lines, through open pixels only.
            let mut frontier: Vec<usize> = (0..count).filter(|index| region[*index]).collect();
            for _ in 0..=gap {
                let mut next = Vec::new();
                for index in frontier {
                    let (x, y) = (index as u32 % width, index as u32 / width);
                    let mut near = Vec::with_capacity(4);
                    if x > 0 {
                        near.push(index - 1);
                    }
                    if x + 1 < width {
                        near.push(index + 1);
                    }
                    if y > 0 {
                        near.push(index - width as usize);
                    }
                    if y + 1 < height {
                        near.push(index + width as usize);
                    }
                    for neighbor in near {
                        if !region[neighbor] && open[neighbor] {
                            region[neighbor] = true;
                            next.push(neighbor);
                        }
                    }
                }
                frontier = next;
            }
            region
        }
    } else {
        joined.iter().map(|value| *value != 0).collect()
    };
    // Under the lines: one pixel always (as the plain fill), more by a
    // spread above nothing; a spread below nothing pulls the area back
    // from the lines.
    region = scale_area(&region, width, height, spread);
    region
        .into_iter()
        .map(|inside| if inside { 255 } else { 0 })
        .collect()
}

/// `region` tucked one pixel under the lines it stops at, then grown by
/// `spread` pixels or, below nothing, shrunk by as many (Area scaling).
fn scale_area(region: &[bool], width: u32, height: u32, spread: i32) -> Vec<bool> {
    if spread >= 0 {
        return dilate(region, width, height, 1 + spread.unsigned_abs());
    }
    let tucked = dilate(region, width, height, 1);
    let outside: Vec<bool> = tucked.iter().map(|inside| !inside).collect();
    dilate(&outside, width, height, spread.unsigned_abs())
        .into_iter()
        .map(|outside| !outside)
        .collect()
}

/// The mask grown by one pixel in every direction.
fn grow(mask: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut out = mask.to_vec();
    for y in 0..height {
        for x in 0..width {
            if mask[(y * width + x) as usize] != 0 {
                continue;
            }
            let touches = (y.saturating_sub(1)..=(y + 1).min(height - 1)).any(|ny| {
                (x.saturating_sub(1)..=(x + 1).min(width - 1))
                    .any(|nx| mask[(ny * width + nx) as usize] != 0)
            });
            if touches {
                out[(y * width + x) as usize] = 255;
            }
        }
    }
    out
}

/// Lays `color` (sRGB 0 to 1) at `opacity` over the layer pixels covered by
/// `mask` (one byte per picture pixel). The layer is `layer_size` and sits
/// at `offset` in a picture of `picture_size`. `alpha_lock` keeps alpha.
#[allow(clippy::too_many_arguments, reason = "a fill names its whole setting")]
pub fn paint_mask(
    pixels: &mut [u8],
    layer_size: [u32; 2],
    offset: [i64; 2],
    mask: &[u8],
    picture_size: [u32; 2],
    color: [f32; 3],
    opacity: f32,
    alpha_lock: bool,
) {
    for y in 0..layer_size[1] {
        for x in 0..layer_size[0] {
            let (px, py) = (i64::from(x) + offset[0], i64::from(y) + offset[1]);
            if px < 0
                || py < 0
                || px >= i64::from(picture_size[0])
                || py >= i64::from(picture_size[1])
            {
                continue;
            }
            let cover = f32::from(mask[(py as u32 * picture_size[0] + px as u32) as usize]) / 255.0;
            if cover <= 0.0 {
                continue;
            }
            let at = ((y * layer_size[0] + x) * 4) as usize;
            let old = crate::doc::unit(&pixels[at..at + 4]);
            let amount = cover * opacity;
            let out = if alpha_lock {
                [
                    old[0] + (color[0] - old[0]) * amount,
                    old[1] + (color[1] - old[1]) * amount,
                    old[2] + (color[2] - old[2]) * amount,
                    old[3],
                ]
            } else {
                sue_model::BlendMode::Normal.composite(old, [color[0], color[1], color[2], amount])
            };
            pixels[at..at + 4].copy_from_slice(&crate::doc::bytes(out));
        }
    }
}

/// A linear (or radial) gradient from `from` to `to` (picture pixels),
/// `start` to `end` (straight RGBA 0 to 1), laid over the layer's pixels
/// inside `selection` (or everywhere).
#[allow(
    clippy::too_many_arguments,
    reason = "a gradient names its whole setting"
)]
pub fn gradient(
    pixels: &mut [u8],
    layer_size: [u32; 2],
    offset: [i64; 2],
    [from, to]: [[f32; 2]; 2],
    [start, end]: [[f32; 4]; 2],
    radial: bool,
    selection: Option<&Selection>,
    alpha_lock: bool,
) {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let length_sq = (dx * dx + dy * dy).max(1e-6);
    for y in 0..layer_size[1] {
        for x in 0..layer_size[0] {
            let (px, py) = (
                (i64::from(x) + offset[0]) as f32 + 0.5,
                (i64::from(y) + offset[1]) as f32 + 0.5,
            );
            let cover = match selection {
                Some(selection) if px >= 0.0 && py >= 0.0 => selection.at(px as u32, py as u32),
                Some(_) => 0.0,
                None => 1.0,
            };
            if cover <= 0.0 {
                continue;
            }
            let t = if radial {
                (((px - from[0]).powi(2) + (py - from[1]).powi(2)) / length_sq).sqrt()
            } else {
                ((px - from[0]) * dx + (py - from[1]) * dy) / length_sq
            }
            .clamp(0.0, 1.0);
            let mut color = [0.0; 4];
            for channel in 0..4 {
                color[channel] = start[channel] + (end[channel] - start[channel]) * t;
            }
            color[3] *= cover;
            let at = ((y * layer_size[0] + x) * 4) as usize;
            let old = crate::doc::unit(&pixels[at..at + 4]);
            let out = if alpha_lock {
                let amount = color[3];
                [
                    old[0] + (color[0] - old[0]) * amount,
                    old[1] + (color[1] - old[1]) * amount,
                    old[2] + (color[2] - old[2]) * amount,
                    old[3],
                ]
            } else {
                sue_model::BlendMode::Normal.composite(old, color)
            };
            pixels[at..at + 4].copy_from_slice(&crate::doc::bytes(out));
        }
    }
}

/// Which colors Advanced Fill (and its Target color) fills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Everything in the area.
    All,
    /// Only see-through pixels.
    Transparent,
    /// Anything not see-through.
    NotTransparent,
    /// Only black ink.
    Black,
    /// Anything but black ink.
    NotBlack,
    /// White and see-through pixels.
    WhiteOrClear,
    /// Anything but white or see-through.
    NotWhiteOrClear,
    /// Areas see-through pixels close in (with Fill closed area).
    SurroundedByClear,
    /// Areas black ink closes in (with Fill closed area).
    SurroundedByBlack,
    /// Areas white and see-through pixels close in (with Fill closed area).
    SurroundedByWhite,
    /// See-through pixels, the faintly covered edges of lines counted as
    /// see-through too.
    SemiClear,
}

impl Target {
    /// Every target, in Clip Studio Paint's order.
    pub const ALL: [Self; 11] = [
        Self::All,
        Self::Transparent,
        Self::NotTransparent,
        Self::SurroundedByClear,
        Self::Black,
        Self::NotBlack,
        Self::SurroundedByBlack,
        Self::WhiteOrClear,
        Self::NotWhiteOrClear,
        Self::SurroundedByWhite,
        Self::SemiClear,
    ];

    /// Whether the dialog offers it: "Other than" without Fill closed
    /// area, "Area surrounded by" only with it, as Clip Studio Paint does.
    #[must_use]
    pub fn offered(self, closed: bool) -> bool {
        match self {
            Self::NotTransparent | Self::NotBlack | Self::NotWhiteOrClear => !closed,
            Self::SurroundedByClear | Self::SurroundedByBlack | Self::SurroundedByWhite => closed,
            _ => true,
        }
    }

    /// `pixel` (straight RGBA8) is one this target fills, `tolerance` (0
    /// to 1) widening what counts.
    #[must_use]
    pub fn matches(self, pixel: [u8; 4], tolerance: f32) -> bool {
        let alpha = f32::from(pixel[3]) / 255.0;
        let luma = (0.299 * f32::from(pixel[0])
            + 0.587 * f32::from(pixel[1])
            + 0.114 * f32::from(pixel[2]))
            / 255.0;
        let clear = alpha <= tolerance.max(1.0 / 255.0);
        let black = alpha >= 0.5 && luma <= tolerance.max(0.05);
        let white = clear || luma >= 1.0 - tolerance.max(0.05);
        match self {
            Self::All => true,
            Self::Transparent => clear,
            Self::NotTransparent => !clear,
            Self::Black => black,
            Self::NotBlack => !black,
            Self::WhiteOrClear => white,
            Self::NotWhiteOrClear => !white,
            // Inside what surrounds it: everything but the surrounding.
            Self::SurroundedByClear => !clear,
            Self::SurroundedByBlack => !black,
            Self::SurroundedByWhite => !white,
            Self::SemiClear => alpha < 0.98,
        }
    }
}

/// Advanced Fill's settings that decide where it fills.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdvancedArea {
    /// Which colors are filled.
    pub target: Target,
    /// 0 to 1.
    pub tolerance: f32,
    /// Pixels the area grows (or, below 0, shrinks) by.
    pub scaling: i32,
    /// Grow and shrink with rounded corners (else square).
    pub round: bool,
    /// Fill only the closed areas inside the selection.
    pub closed: bool,
    /// Gaps in lines this wide hold a closed area in.
    pub gap: u32,
    /// The canvas's edge counts as a line.
    pub border: bool,
    /// The fill's edge is softened.
    pub anti_alias: bool,
}

impl Default for AdvancedArea {
    fn default() -> Self {
        Self {
            target: Target::All,
            tolerance: 0.1,
            scaling: 0,
            round: false,
            closed: false,
            gap: 0,
            border: false,
            anti_alias: true,
        }
    }
}

/// `mask` grown by `steps` pixels (square, or near round when `round`),
/// staying where `allowed` is set.
fn grow_area(
    mask: &[bool],
    size: [u32; 2],
    steps: u32,
    round: bool,
    allowed: &[bool],
) -> Vec<bool> {
    let (w, h) = (size[0] as usize, size[1] as usize);
    let mut now = mask.to_vec();
    for step in 0..steps {
        // Rounded: every other step reaches only the four sides.
        let diagonal = !round || step % 2 == 1;
        let before = now.clone();
        for y in 0..h {
            for x in 0..w {
                let at = y * w + x;
                if before[at] || !allowed[at] {
                    continue;
                }
                let mut near = false;
                for (dx, dy) in [
                    (-1, 0),
                    (1, 0),
                    (0, -1),
                    (0, 1),
                    (-1, -1),
                    (1, -1),
                    (-1, 1),
                    (1, 1),
                ] {
                    if !diagonal && dx != 0 && dy != 0 {
                        continue;
                    }
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx >= 0
                        && ny >= 0
                        && (nx as usize) < w
                        && (ny as usize) < h
                        && before[ny as usize * w + nx as usize]
                    {
                        near = true;
                        break;
                    }
                }
                now[at] = near;
            }
        }
    }
    now
}

/// Where Advanced Fill fills: one coverage byte per pixel of `size`.
/// `reference` is what it looks at (straight RGBA8), `region` the
/// selection's coverage (or 255 everywhere).
#[must_use]
pub fn advanced_mask(
    reference: &[u8],
    size: [u32; 2],
    region: &[u8],
    area: &AdvancedArea,
) -> Vec<u8> {
    let (w, h) = (size[0] as usize, size[1] as usize);
    let count = w * h;
    if reference.len() < count * 4 || region.len() < count {
        return vec![0; count];
    }
    let inside: Vec<bool> = region.iter().map(|cover| *cover > 127).collect();
    let matches: Vec<bool> = reference
        .as_chunks::<4>()
        .0
        .iter()
        .take(count)
        .map(|pixel| area.target.matches(*pixel, area.tolerance))
        .collect();
    let everywhere = vec![true; count];
    let mut fill: Vec<bool> = (0..count).map(|at| inside[at] && matches[at]).collect();
    if area.closed {
        // Lines (what is not filled), thickened by the gap, hold areas in.
        let lines: Vec<bool> = matches.iter().map(|matched| !matched).collect();
        let thick = grow_area(&lines, size, area.gap, true, &everywhere);
        let open: Vec<bool> = (0..count).map(|at| fill[at] && !thick[at]).collect();
        // Keep the pieces that touch neither the area's edge nor (unless
        // it counts as a line) the canvas's.
        let mut kept = vec![false; count];
        let mut seen = vec![false; count];
        for start in 0..count {
            if !open[start] || seen[start] {
                continue;
            }
            let mut piece = vec![start];
            let mut stack = vec![start];
            seen[start] = true;
            let mut leaks = false;
            while let Some(at) = stack.pop() {
                let (x, y) = (at % w, at / w);
                for (dx, dy) in [(-1_i64, 0_i64), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                        leaks |= !area.border;
                        continue;
                    }
                    let next = ny as usize * w + nx as usize;
                    if !inside[next] {
                        leaks = true;
                    } else if open[next] && !seen[next] {
                        seen[next] = true;
                        stack.push(next);
                        piece.push(next);
                    }
                }
            }
            if !leaks {
                for at in piece {
                    kept[at] = true;
                }
            }
        }
        // Back up to the lines the gap kept it from.
        fill = grow_area(&kept, size, area.gap, true, &fill);
    }
    if area.scaling > 0 {
        fill = grow_area(&fill, size, area.scaling as u32, area.round, &inside);
    } else if area.scaling < 0 {
        let outside: Vec<bool> = fill.iter().map(|filled| !filled).collect();
        let eaten = grow_area(
            &outside,
            size,
            area.scaling.unsigned_abs(),
            area.round,
            &everywhere,
        );
        fill = eaten.iter().map(|out| !out).collect();
    }
    (0..count)
        .map(|at| {
            if fill[at] {
                return 255;
            }
            if !area.anti_alias || !inside[at] {
                return 0;
            }
            // A soft edge: part covered by how much of the fill is next to it.
            let (x, y) = (at % w, at / w);
            let mut near = 0_u32;
            for dy in -1_i64..=1 {
                for dx in -1_i64..=1 {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx >= 0
                        && ny >= 0
                        && (nx as usize) < w
                        && (ny as usize) < h
                        && fill[ny as usize * w + nx as usize]
                    {
                        near += 1;
                    }
                }
            }
            (near * 255 / 18) as u8
        })
        .collect()
}

/// Lays `color` (sRGB 0 to 1, or `None` to erase) at `opacity` in `blend`
/// over the layer pixels `mask` covers, as [`paint_mask`] places them.
#[allow(clippy::too_many_arguments, reason = "a fill names its whole setting")]
pub fn paint_blended(
    pixels: &mut [u8],
    layer_size: [u32; 2],
    offset: [i64; 2],
    mask: &[u8],
    picture_size: [u32; 2],
    color: Option<[f32; 3]>,
    opacity: f32,
    blend: sue_model::BlendMode,
    alpha_lock: bool,
) {
    for y in 0..layer_size[1] {
        for x in 0..layer_size[0] {
            let (px, py) = (i64::from(x) + offset[0], i64::from(y) + offset[1]);
            if px < 0
                || py < 0
                || px >= i64::from(picture_size[0])
                || py >= i64::from(picture_size[1])
            {
                continue;
            }
            let cover = f32::from(mask[(py as u32 * picture_size[0] + px as u32) as usize]) / 255.0;
            if cover <= 0.0 {
                continue;
            }
            let at = ((y * layer_size[0] + x) * 4) as usize;
            let old = crate::doc::unit(&pixels[at..at + 4]);
            let amount = cover * opacity;
            let mut out = match color {
                None => [old[0], old[1], old[2], old[3] * (1.0 - amount)],
                Some([r, g, b]) => blend.composite(old, [r, g, b, amount]),
            };
            if alpha_lock {
                out[3] = old[3];
            }
            pixels[at..at + 4].copy_from_slice(&crate::doc::bytes(out));
        }
    }
}

/// Close and fill: inside `lasso` (a mask, 0 or more), the areas the
/// lines in `reference` (RGBA) close off from the lasso's edge, grown a
/// pixel under the lines so no seam shows. Lines are pixels at least half
/// opaque.
#[must_use]
pub fn enclosed(reference: &[u8], size: [u32; 2], lasso: &[u8]) -> Vec<u8> {
    let [width, height] = size;
    let count = (width * height) as usize;
    let mut out = vec![0; count];
    if reference.len() < count * 4 || lasso.len() < count {
        return out;
    }
    let inside = |index: usize| lasso[index] >= 128;
    let line = |index: usize| reference[index * 4 + 3] >= 128;
    // Open: what the lasso's edge reaches without crossing a line.
    let mut open = vec![false; count];
    let mut stack = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) as usize;
            if !inside(index) || line(index) {
                continue;
            }
            let edge = x == 0
                || y == 0
                || x + 1 == width
                || y + 1 == height
                || !inside(index - 1)
                || !inside(index + 1)
                || !inside(index - width as usize)
                || !inside(index + width as usize);
            if edge {
                open[index] = true;
                stack.push(index);
            }
        }
    }
    while let Some(index) = stack.pop() {
        let (x, y) = (index as u32 % width, index as u32 / width);
        let mut visit = |next: usize| {
            if !open[next] && inside(next) && !line(next) {
                open[next] = true;
                stack.push(next);
            }
        };
        if x > 0 {
            visit(index - 1);
        }
        if x + 1 < width {
            visit(index + 1);
        }
        if y > 0 {
            visit(index - width as usize);
        }
        if y + 1 < height {
            visit(index + width as usize);
        }
    }
    for index in 0..count {
        if inside(index) && !line(index) && !open[index] {
            out[index] = 255;
        }
    }
    // A pixel under the lines around each area.
    let grown: Vec<u8> = (0..count)
        .map(|index| {
            if out[index] > 0 || !inside(index) || !line(index) {
                return out[index];
            }
            let (x, y) = (index as u32 % width, index as u32 / width);
            let near = (x > 0 && out[index - 1] > 0)
                || (x + 1 < width && out[index + 1] > 0)
                || (y > 0 && out[index - width as usize] > 0)
                || (y + 1 < height && out[index + width as usize] > 0);
            if near { 255 } else { 0 }
        })
        .collect();
    grown
}

/// Paint unpainted area: inside `lasso`, what the editing layer (`layer`,
/// RGBA as it lies on the picture) left unpainted, where no line in
/// `reference` (RGBA) is either. Lines and paint are pixels at least half
/// opaque.
#[must_use]
pub fn unpainted(layer: &[u8], reference: &[u8], size: [u32; 2], lasso: &[u8]) -> Vec<u8> {
    let count = (size[0] * size[1]) as usize;
    if layer.len() < count * 4 || reference.len() < count * 4 || lasso.len() < count {
        return vec![0; count];
    }
    (0..count)
        .map(|index| {
            let open = layer[index * 4 + 3] < 128 && reference[index * 4 + 3] < 128;
            if open { lasso[index] } else { 0 }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn paint_unpainted_area_fills_only_the_gaps() {
        let size = [4u32, 1u32];
        // Painted, a gap, a line on another layer, a gap outside the lasso.
        let layer = [0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let reference = [0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 0];
        let lasso = [255, 255, 255, 0];
        assert_eq!(
            super::unpainted(&layer, &reference, size, &lasso),
            vec![0, 255, 0, 0]
        );
    }

    #[test]
    fn close_and_fill_fills_only_what_lines_close_off() {
        // Two rings drawn on 40 x 20: a closed one at the left, one with a
        // gap at the right; the lasso takes in the whole picture but its
        // border row.
        let size = [40u32, 20u32];
        let mut reference = vec![0u8; 40 * 20 * 4];
        let mut ink = |x: u32, y: u32| reference[((y * 40 + x) * 4 + 3) as usize] = 255;
        for i in 3..15 {
            ink(3, i);
            ink(14, i);
            ink(i, 3);
            ink(i, 14);
            ink(23, i);
            ink(36, i);
            ink(i + 20, 3);
            if i != 8 {
                ink(i + 20, 14);
            }
        }
        let lasso: Vec<u8> = (0..40 * 20)
            .map(|index| {
                let (x, y) = (index % 40, index / 40);
                if x > 0 && y > 0 && x < 39 && y < 19 {
                    255
                } else {
                    0
                }
            })
            .collect();
        let filled = super::enclosed(&reference, size, &lasso);
        let at = |x: usize, y: usize| filled[y * 40 + x];
        assert_eq!(at(8, 8), 255, "inside the closed ring");
        assert_eq!(at(3, 8), 255, "a pixel under its line");
        assert_eq!(at(1, 1), 0, "outside every ring");
        assert_eq!(at(30, 8), 0, "the ring with a gap stays open");
    }

    use super::*;

    #[test]
    fn close_gap_keeps_a_fill_inside_a_broken_line() {
        // A ring of black line with a three pixel break on its right.
        let (width, height) = (40_u32, 40_u32);
        let mut pixels = vec![0_u8; (width * height * 4) as usize];
        for y in 0..height {
            for x in 0..width {
                let (dx, dy) = (x as f32 - 20.0, y as f32 - 20.0);
                let ring = (dx.hypot(dy) - 12.0).abs() < 1.5;
                let gap = x > 30 && (19..22).contains(&y);
                if ring && !gap {
                    pixels[((y * width + x) * 4) as usize..][..4].copy_from_slice(&[0, 0, 0, 255]);
                }
            }
        }
        let inside = [20, 20];
        let leaked = flood(&pixels, [width, height], inside, 0.1);
        assert_eq!(
            leaked[(2 * width + 2) as usize],
            255,
            "without close gap it leaks out"
        );
        let held = flood_closing(&pixels, [width, height], inside, 0.1, gap_radius(2), 0);
        assert_eq!(
            held[(2 * width + 2) as usize],
            0,
            "with it the fill stays inside"
        );
        assert_eq!(held[(20 * width + 20) as usize], 255);
        assert_eq!(
            held[(20 * width + 30) as usize],
            255,
            "and reaches the line"
        );
        // Area scaling spreads under the line.
        let spread = flood_closing(&pixels, [width, height], inside, 0.1, gap_radius(2), 2);
        let count = |mask: &[u8]| mask.iter().filter(|value| **value != 0).count();
        assert!(count(&spread) > count(&held));
    }

    /// A 6 by 4 picture split by a black vertical line at x = 2.
    fn walled() -> Vec<u8> {
        let mut pixels = Vec::new();
        for _y in 0..4 {
            for x in 0..6 {
                pixels.extend(if x == 2 { [0, 0, 0, 255] } else { [0, 0, 0, 0] });
            }
        }
        pixels
    }

    #[test]
    fn floods_stop_at_lines_and_tuck_under_them() {
        let pixels = walled();
        let mask = flood(&pixels, [6, 4], [0, 0], 0.0);
        let row: Vec<u8> = mask[..6].to_vec();
        assert_eq!(row, [255, 255, 255, 0, 0, 0], "left side plus one pixel");
        let right = flood(&pixels, [6, 4], [5, 3], 0.0);
        assert_eq!(&right[18..24], &[0, 0, 255, 255, 255, 255]);
        assert!(flood(&pixels, [6, 4], [9, 9], 0.0).iter().all(|v| *v == 0));
        let all = flood(&pixels, [6, 4], [0, 0], 1.0);
        assert!(
            all.iter().all(|v| *v == 255),
            "full tolerance takes everything"
        );
    }

    #[test]
    fn masked_fills_and_gradients_paint_inside() {
        let mut pixels = vec![0_u8; 4 * 4 * 4];
        let mut mask = vec![0_u8; 16];
        mask[5] = 255;
        paint_mask(
            &mut pixels,
            [4, 4],
            [0, 0],
            &mask,
            [4, 4],
            [1.0, 0.0, 0.0],
            1.0,
            false,
        );
        assert_eq!(&pixels[5 * 4..6 * 4], &[255, 0, 0, 255]);
        assert_eq!(&pixels[..4], &[0, 0, 0, 0]);
        let mut ramp = vec![0_u8; 10 * 4];
        gradient(
            &mut ramp,
            [10, 1],
            [0, 0],
            [[0.0, 0.5], [10.0, 0.5]],
            [[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0]],
            false,
            None,
            false,
        );
        assert!(ramp[3] > ramp[9 * 4 + 3], "fades out to the right");
        assert!(ramp[3] > 200 && ramp[9 * 4 + 3] < 30);
    }

    #[test]
    fn advanced_fill_fills_targets_closed_areas_and_scales() {
        // A 20 by 20 clear layer with a black ring (an open gap of two
        // pixels on its right) around 8 by 8 clear pixels.
        let size = [20_u32, 20];
        let mut layer = vec![0_u8; 20 * 20 * 4];
        for y in 4..16_u32 {
            for x in 4..16_u32 {
                let ring = x == 4 || x == 15 || y == 4 || y == 15;
                let gap = x == 15 && (9..11).contains(&y);
                if ring && !gap {
                    let at = ((y * 20 + x) * 4) as usize;
                    layer[at..at + 4].copy_from_slice(&[0, 0, 0, 255]);
                }
            }
        }
        let all = vec![255_u8; 400];
        let at = |x: u32, y: u32| (y * 20 + x) as usize;
        let mut area = AdvancedArea {
            anti_alias: false,
            ..AdvancedArea::default()
        };
        // All: everything in the selection.
        assert!(
            advanced_mask(&layer, size, &all, &area)
                .iter()
                .all(|c| *c == 255)
        );
        // Only black: just the ring.
        area.target = Target::Black;
        let ring = advanced_mask(&layer, size, &all, &area);
        assert_eq!(ring[at(4, 4)], 255);
        assert_eq!(ring[at(8, 8)], 0);
        // Closed areas of clear: the gap lets it out, until Close gap holds it.
        area.target = Target::Transparent;
        area.closed = true;
        let leaking = advanced_mask(&layer, size, &all, &area);
        assert_eq!(leaking[at(8, 8)], 0, "the ring is open");
        area.gap = 2;
        let held = advanced_mask(&layer, size, &all, &area);
        assert_eq!(held[at(8, 8)], 255, "the gap is closed");
        assert_eq!(held[at(1, 1)], 0, "outside stays empty");
        // Area scaling grows it under the ring, not past the selection.
        area.scaling = 1;
        let grown = advanced_mask(&layer, size, &all, &area);
        assert_eq!(grown[at(4, 8)], 255);
        let mut half = vec![255_u8; 400];
        for y in 0..20 {
            for x in 10..20 {
                half[at(x, y)] = 0;
            }
        }
        area = AdvancedArea {
            anti_alias: false,
            ..AdvancedArea::default()
        };
        let left = advanced_mask(&layer, size, &half, &area);
        assert_eq!(left[at(2, 2)], 255);
        assert_eq!(left[at(12, 2)], 0, "only the selection");
    }

    /// A clear 40 by 40 picture with a black square outline, 2 pixels
    /// thick, from 10 to 29, and a separate clear area beyond it.
    fn boxed() -> (Vec<u8>, [u32; 2]) {
        let (width, height) = (40_u32, 40_u32);
        let mut pixels = vec![0_u8; (width * height * 4) as usize];
        for y in 10..30 {
            for x in 10..30 {
                let edge = !(12..28).contains(&x) || !(12..28).contains(&y);
                if edge {
                    let at = ((y * width + x) * 4) as usize;
                    pixels[at..at + 4].copy_from_slice(&[0, 0, 0, 255]);
                }
            }
        }
        (pixels, [width, height])
    }

    fn count(mask: &[u8]) -> usize {
        mask.iter().filter(|value| **value != 0).count()
    }

    #[test]
    fn area_scaling_spreads_under_lines_or_keeps_back_from_them() {
        let (pixels, size) = boxed();
        let at = |x: u32, y: u32| (y * size[0] + x) as usize;
        let take = |spread: i32| {
            area(
                &pixels,
                size,
                [20, 20],
                &AreaSettings {
                    spread,
                    ..AreaSettings::default()
                },
            )
        };
        let plain = take(0);
        // The inside is 16 by 16; the plain fill tucks one pixel under.
        assert_eq!(count(&plain), 18 * 18);
        assert_eq!(plain[at(11, 20)], 255, "one under the line");
        let wide = take(1);
        assert_eq!(count(&wide), 20 * 20);
        let narrow = take(-2);
        assert_eq!(count(&narrow), 14 * 14);
        assert_eq!(narrow[at(12, 20)], 0, "kept back from the line");
        assert_eq!(narrow[at(13, 20)], 255);
        // Close gap with a negative spread still holds inside.
        let held = area(
            &pixels,
            size,
            [20, 20],
            &AreaSettings {
                gap: 1,
                spread: -1,
                ..AreaSettings::default()
            },
        );
        // The inside less its corners (the way back to the lines goes
        // straight, a pixel a step), kept a pixel back from the lines.
        assert!(
            (16 * 16 - 4..=16 * 16).contains(&count(&held)),
            "{}",
            count(&held)
        );
        assert_eq!(held[at(11, 20)], 0);
        assert_eq!(held[at(12, 20)], 255);
    }

    #[test]
    fn connected_only_off_takes_every_like_pixel() {
        let (pixels, size) = boxed();
        let joined = area(&pixels, size, [20, 20], &AreaSettings::default());
        let every = area(
            &pixels,
            size,
            [20, 20],
            &AreaSettings {
                connected_only: false,
                ..AreaSettings::default()
            },
        );
        // Off, the clear outside counts as well; only the lines stay out.
        assert!(count(&every) > count(&joined));
        assert_eq!(count(&every), 40 * 40, "tucked under every line");
    }

    #[test]
    fn anti_aliasing_softens_the_edge_only() {
        let (pixels, size) = boxed();
        let hard = area(&pixels, size, [20, 20], &AreaSettings::default());
        assert!(hard.iter().all(|value| *value == 0 || *value == 255));
        for (level, width) in [
            (AntiAlias::Weak, 1),
            (AntiAlias::Medium, 2),
            (AntiAlias::Strong, 3),
        ] {
            let soft = area(
                &pixels,
                size,
                [20, 20],
                &AreaSettings {
                    antialias: level,
                    ..AreaSettings::default()
                },
            );
            let partial = soft
                .iter()
                .filter(|value| (1..255).contains(*value))
                .count();
            assert!(partial > 0, "{level:?}");
            // The middle stays whole and far outside stays clear.
            assert_eq!(soft[(20 * size[0] + 20) as usize], 255);
            assert_eq!(soft[(2 * size[0] + 2) as usize], 0);
            let reach = soft.iter().filter(|value| **value != 0).count();
            assert!(reach <= (18 + 2 * width) * (18 + 2 * width), "{level:?}");
        }
    }

    #[test]
    fn close_gap_works_for_auto_select_as_for_fill() {
        // A ring with a gap: without Close gap the area leaks out.
        let (mut pixels, size) = boxed();
        for y in 19..21 {
            for x in 10..12 {
                let at = ((y * size[0] + x) * 4) as usize;
                pixels[at..at + 4].copy_from_slice(&[0, 0, 0, 0]);
            }
        }
        let leak = area(&pixels, size, [20, 20], &AreaSettings::default());
        assert!(count(&leak) > 1000);
        // Auto Select takes its area with the same settings as Fill.
        let settings = AreaSettings {
            tolerance: 0.1,
            gap: 2,
            ..AreaSettings::default()
        };
        let held = area(&pixels, size, [20, 20], &settings);
        assert!(count(&held) < 20 * 20, "{}", count(&held));
    }

    /// Five layers: a plain one, a draft, a text layer, paper, a locked
    /// one, a reference layer, and two in a folder.
    fn stack() -> (Vec<ReferLayer>, [EntityId; 9]) {
        let ids: [EntityId; 9] = std::array::from_fn(|_| EntityId::new());
        let layer = |index: usize| ReferLayer {
            id: ids[index],
            draft: false,
            text: false,
            paper: false,
            locked: false,
            reference: false,
            folder: false,
            around: Vec::new(),
        };
        let layers = vec![
            ReferLayer {
                paper: true,
                ..layer(0)
            },
            layer(1),
            ReferLayer {
                draft: true,
                ..layer(2)
            },
            ReferLayer {
                text: true,
                ..layer(3)
            },
            ReferLayer {
                locked: true,
                ..layer(4)
            },
            ReferLayer {
                reference: true,
                ..layer(5)
            },
            ReferLayer {
                folder: true,
                ..layer(6)
            },
            ReferLayer {
                around: vec![ids[6]],
                ..layer(7)
            },
            ReferLayer {
                around: vec![ids[6]],
                ..layer(8)
            },
        ];
        (layers, ids)
    }

    #[test]
    fn refer_modes_pick_their_layers() {
        let (layers, ids) = stack();
        let none = Exclude {
            drafts: false,
            text: false,
            paper: false,
            locked: false,
        };
        let take = |editing: usize, picked: &[EntityId], refer: Refer, exclude: Exclude| {
            referred(&layers, Some(ids[editing]), picked, refer, exclude)
        };
        assert_eq!(take(1, &[], Refer::Editing, none), [ids[1]]);
        let all: Vec<EntityId> = ids.iter().copied().filter(|id| *id != ids[6]).collect();
        assert_eq!(
            take(1, &[], Refer::All, none),
            all,
            "a folder is not a layer"
        );
        assert_eq!(take(1, &[ids[5]], Refer::Selected, none), [ids[1], ids[5]]);
        assert_eq!(take(7, &[], Refer::Folder, none), [ids[7], ids[8]]);
        assert_eq!(
            take(1, &[], Refer::Folder, none),
            [ids[0], ids[1], ids[2], ids[3], ids[4], ids[5]],
            "outside folders: the layers outside folders"
        );
        assert_eq!(take(1, &[], Refer::Reference, none), [ids[5]]);
        // A reference folder: the layers in it count as reference layers.
        let mut folders = layers.clone();
        folders[6].reference = true;
        assert_eq!(
            referred(&folders, Some(ids[1]), &[], Refer::Reference, none),
            [ids[5], ids[7], ids[8]]
        );
        // The exclusions, one at a time and all together.
        let leave = |exclude: Exclude| take(1, &[], Refer::All, exclude);
        assert!(
            !leave(Exclude {
                drafts: true,
                ..none
            })
            .contains(&ids[2])
        );
        assert!(!leave(Exclude { text: true, ..none }).contains(&ids[3]));
        assert!(
            !leave(Exclude {
                paper: true,
                ..none
            })
            .contains(&ids[0])
        );
        assert!(
            !leave(Exclude {
                locked: true,
                ..none
            })
            .contains(&ids[4])
        );
        let every = Exclude {
            drafts: true,
            text: true,
            paper: true,
            locked: true,
        };
        assert_eq!(leave(every), [ids[1], ids[5], ids[7], ids[8]]);
        // The editing layer alone ignores the exclusions.
        assert_eq!(take(2, &[], Refer::Editing, every), [ids[2]]);
        // Choices round-trip by name.
        for refer in Refer::ALL {
            assert_eq!(Refer::from_id(refer.id()), Some(refer));
        }
    }

    #[test]
    fn a_fill_looks_at_the_layers_it_refers_to_in_a_real_picture() {
        use sue_core::ReversibleTransaction;
        use sue_core::txn::SetProperty;
        let mut arena = Arena::new();
        let (mut edit, picture) = doc::create(&doc::NewPicture {
            name: "Test".to_owned(),
            width: 8,
            height: 4,
            dpi: 72.0,
            paper: Some(sue_core::Color::srgb8(255, 255, 255)),
            timelapse: false,
        })
        .unwrap();
        edit.apply(&mut arena).unwrap();
        let first = doc::layers(&arena, picture).last().unwrap().id;
        let (mut spawn, second) =
            doc::add_raster_layer(&arena, picture, Some(first), "Lines".to_owned(), None).unwrap();
        spawn.apply(&mut arena).unwrap();
        SetProperty::new(second, doc::REFERENCE, true)
            .apply(&mut arena)
            .unwrap();
        // A line down the whole picture at `x`.
        let paint = |arena: &mut Arena, layer: EntityId, x: u32| {
            let raster = doc::raster_mut(arena, layer).unwrap();
            for y in 0..4 {
                let at = (y * raster.width() + x) as usize * 4;
                raster.bytes_mut().unwrap()[at..at + 4].copy_from_slice(&[0, 0, 0, 255]);
            }
        };
        paint(&mut arena, first, 1);
        paint(&mut arena, second, 5);
        let dark = |pixels: &[u8], x: u32| pixels[((8 + x) * 4 + 3) as usize] == 255;
        let look = |refer: Refer, exclude: Exclude| {
            reference(&arena, picture, [8, 4], Some(first), &[], refer, exclude).unwrap()
        };
        let editing = look(Refer::Editing, Exclude::default());
        assert!(dark(&editing, 1) && !dark(&editing, 5));
        let all = look(Refer::All, Exclude::default());
        assert!(dark(&all, 1) && dark(&all, 5));
        assert_eq!(all[3], 0, "paper left out by default");
        let lines = look(Refer::Reference, Exclude::default());
        assert!(!dark(&lines, 1) && dark(&lines, 5));
        let with_paper = look(
            Refer::All,
            Exclude {
                paper: false,
                ..Exclude::default()
            },
        );
        assert_eq!(&with_paper[..4], &[255, 255, 255, 255]);
        // A fill from a clear point of the reference stops at its line.
        let mask = area(&lines, [8, 4], [0, 1], &AreaSettings::default());
        assert_eq!(mask[(8 + 5) as usize], 255, "tucked under the line");
        assert_eq!(mask[(8 + 6) as usize], 0, "not past it");
    }
}
