//! Clip Studio Paint files: opened as a canvas, and exported from one.
//!
//! Sue Paint keeps a picture in a Sue project (`.sue`); a `.clip` is a
//! conversion both ways, through the neutral [`ClipDocument`] of
//! `sue_io::clip`:
//!
//! - Opening ([`open`]) makes a picture of the file's canvas and a layer
//!   or folder of each of its layers, with their style, pixels (off the
//!   canvas too), masks, paper, draft and reference marks. On every
//!   entity it made, a `clip.foreign` component keeps what the file held
//!   for it, Sue's terms or not, with a fingerprint of the entity
//!   ([`to_sue`], [`keep`], [`print`]).
//! - Exporting ([`encode`]) writes each layer back: one not changed since
//!   it was opened goes back exactly as it was read, everything Sue does
//!   not understand included; a changed one is written from Sue's model
//!   ([`to_clip`]).
//! - Both say per layer what they did in a [`Report`].
//!
//! Files Sue 0.0.75 wrote carry a hidden copy of Sue's own project. It is
//! no longer written, and it is still read, but only while the file's
//! layers are what that copy would have written: a file changed in Clip
//! Studio afterwards opens from its layers, and the report says so.

mod extras;
mod keep;
mod layers;
mod print;
mod report;
mod to_clip;
mod to_sue;

#[cfg(test)]
mod tests;

use sue_core::ReversibleTransaction;
use sue_core::txn::{Relate, SpawnEntity};
use sue_core::{Arena, EntityId, SueFile};
use sue_io::psd::{Psd, PsdKind, PsdLayer};
use sue_model::{BlendMode, picture};
use sue_wm::AppletContext;

pub use report::{Line, Outcome, Report};
pub use to_clip::document;
pub use to_sue::{into_picture, new_picture};

use crate::doc::Edit;

/// The format name the `clip.foreign` components of this converter carry.
pub const FORMAT: &str = "clip";

/// The Settings key that picks how Sue Paint saves a new picture: `sue`
/// (a Sue project, the default) or `clip`.
pub const SAVE_FORMAT_KEY: &str = "sue_paint.save_format";

/// `true` when Sue Paint is set to save new pictures as `.clip`.
#[must_use]
pub fn saves_clip(cx: &AppletContext<'_>) -> bool {
    cx.rice().get_str(SAVE_FORMAT_KEY) == Some("clip")
}

/// Clip Studio's blend numbers and Sue's modes. Every mode of Sue's has
/// one, Glow Dodge and Add (Glow) included; Through is folders' only.
const BLENDS: [(i64, BlendMode); 29] = [
    (0, BlendMode::Normal),
    (1, BlendMode::Darken),
    (2, BlendMode::Multiply),
    (3, BlendMode::ColorBurn),
    (4, BlendMode::LinearBurn),
    (5, BlendMode::Subtract),
    (6, BlendMode::DarkerColor),
    (7, BlendMode::Lighten),
    (8, BlendMode::Screen),
    (9, BlendMode::ColorDodge),
    (10, BlendMode::GlowDodge),
    (11, BlendMode::Add),
    (12, BlendMode::AddGlow),
    (13, BlendMode::LighterColor),
    (14, BlendMode::Overlay),
    (15, BlendMode::SoftLight),
    (16, BlendMode::HardLight),
    (17, BlendMode::VividLight),
    (18, BlendMode::LinearLight),
    (19, BlendMode::PinLight),
    (20, BlendMode::HardMix),
    (21, BlendMode::Difference),
    (22, BlendMode::Exclusion),
    (23, BlendMode::Hue),
    (24, BlendMode::Saturation),
    (25, BlendMode::Color),
    (26, BlendMode::Luminosity),
    (30, BlendMode::Through),
    (36, BlendMode::Divide),
];

/// Sue's mode for Clip Studio's blend number `composite`, if Sue has it.
#[must_use]
pub fn blend_of(composite: i64) -> Option<BlendMode> {
    BLENDS
        .iter()
        .find(|(number, _)| *number == composite)
        .map(|(_, mode)| *mode)
}

/// Clip Studio's blend number for `mode`.
#[must_use]
pub fn composite_of(mode: BlendMode) -> i64 {
    BLENDS
        .iter()
        .find(|(_, of)| *of == mode)
        .map_or(0, |(number, _)| *number)
}

/// `picture` as a `.clip`, and what became of each layer.
///
/// # Errors
/// When the picture has no size, or is too large for Clip Studio.
pub fn encode_with_report(arena: &Arena, picture: EntityId) -> Result<(Vec<u8>, Report), String> {
    let (doc, report) = document(arena, picture)?;
    Ok((sue_io::clip::write(&doc)?, report))
}

/// `picture` as a `.clip`.
///
/// # Errors
/// When the picture has no size, or is too large for Clip Studio.
pub fn encode(arena: &Arena, picture: EntityId) -> Result<Vec<u8>, String> {
    encode_with_report(arena, picture).map(|(bytes, _)| bytes)
}

/// The picture a `.clip` from Sue 0.0.75 carries: the edit that brings it
/// into `arena` with its ids (`None` when it is open already) and the
/// picture. `None` when `sue` holds no picture.
#[must_use]
pub fn embedded(arena: &Arena, sue: &[u8], label: &str) -> Option<(Option<Edit>, EntityId)> {
    let mut project = SueFile::from_bytes(sue)
        .ok()?
        .arena(&sue_model::registry())
        .ok()?;
    let picture = project
        .iter()
        .find(|(_, entity)| picture::is_picture(entity))
        .map(|(_, entity)| entity.id())?;
    if arena.get_by_id(picture).is_ok() {
        return Some((None, picture));
    }
    let relations: Vec<_> = project.relations().iter().collect();
    let handles: Vec<_> = project.iter().map(|(handle, _)| handle).collect();
    let mut edit = Edit::new(label);
    for handle in handles {
        if let Ok(entity) = project.despawn(handle) {
            edit = edit.then(SpawnEntity::new(entity));
        }
    }
    for relation in relations {
        edit = edit.then(Relate::new(relation));
    }
    Some((Some(edit), picture))
}

/// `layer` drawn on a canvas of `width` by `height` (clear pixels keep no
/// color; folders draw nothing), with its style in words.
fn drawn(width: u32, height: u32, layer: &PsdLayer) -> (String, Vec<u8>) {
    let style = format!(
        "{:?} {} {} {} {} {:?}",
        layer.kind, layer.name, layer.opacity, layer.visible, layer.clip, layer.blend
    );
    let mut canvas = Vec::new();
    if layer.kind == PsdKind::Pixels {
        canvas = vec![0_u8; width as usize * height as usize * 4];
        for y in 0..layer.height {
            for x in 0..layer.width {
                let (cx, cy) = (
                    i64::from(layer.left) + i64::from(x),
                    i64::from(layer.top) + i64::from(y),
                );
                if cx < 0 || cy < 0 || cx >= i64::from(width) || cy >= i64::from(height) {
                    continue;
                }
                let from = (y as usize * layer.width as usize + x as usize) * 4;
                let pixel = &layer.pixels[from..from + 4];
                if pixel[3] > 0 {
                    let to = (cy as usize * width as usize + cx as usize) * 4;
                    canvas[to..to + 4].copy_from_slice(pixel);
                }
            }
        }
    }
    (style, canvas)
}

/// Whether a `.clip` Sue 0.0.75 wrote still holds what its copy of
/// Sue's project would write: the same layers in the same order, with
/// the same names, style and pixels. A file changed in Clip Studio after
/// Sue wrote it does not.
fn copy_matches(bytes: &[u8], sue: &[u8]) -> bool {
    let Ok(clip) = sue_io::clip::decode_clip(bytes) else {
        return false;
    };
    let mut arena = Arena::new();
    let Some((Some(mut edit), picture)) = embedded(&arena, sue, "") else {
        return false;
    };
    if edit.apply(&mut arena).is_err() {
        return false;
    }
    let Some(written) = crate::psd_io::to_psd(&arena, picture) else {
        return false;
    };
    let Psd { width, height, .. } = clip.psd;
    // One layer of each at a time: a large canvas of many layers must
    // not need all of them drawn at once.
    [written.width, written.height] == [width, height]
        && written.layers.len() == clip.psd.layers.len()
        && written
            .layers
            .iter()
            .zip(&clip.psd.layers)
            .all(|(ours, theirs)| drawn(width, height, ours) == drawn(width, height, theirs))
}

/// What opening a `.clip` makes.
pub enum Opened {
    /// The copy of Sue's project a file of Sue 0.0.75 carries, still in
    /// step with its layers: the edit that brings it in (`None` when it
    /// is open already) and the picture.
    Copy(Option<Edit>, EntityId),
    /// A new picture made from the file's layers.
    Layers(Edit, EntityId),
}

/// Opens `.clip` `bytes` as a picture named `name`, with `label` for the
/// edit, and says what became of each layer.
///
/// # Errors
/// When `bytes` is not a `.clip`, or its database or bitmaps are broken.
pub fn open(
    arena: &Arena,
    bytes: &[u8],
    name: &str,
    label: &str,
) -> Result<(Opened, Report), String> {
    let doc = sue_io::clip::read(bytes)?;
    let mut stale = false;
    if let Some(sue) = &doc.sue {
        if copy_matches(bytes, sue)
            && let Some((edit, picture)) = embedded(arena, sue, label)
        {
            let report = Report {
                own_copy: true,
                ..Report::default()
            };
            return Ok((Opened::Copy(edit, picture), report));
        }
        stale = true;
    }
    let (edit, picture, mut report) = new_picture(&doc, name, label)?;
    report.stale_copy = stale;
    Ok((Opened::Layers(edit, picture), report))
}

/// The picture in `.clip` `bytes` on its own, as [`open`] makes it. For
/// thumbnails.
#[must_use]
pub fn picture_arena(bytes: &[u8]) -> Option<(Arena, EntityId)> {
    let mut arena = Arena::new();
    let (opened, _) = open(&arena, bytes, "", "").ok()?;
    let picture = match opened {
        Opened::Copy(Some(mut edit), picture) | Opened::Layers(mut edit, picture) => {
            edit.apply(&mut arena).ok()?;
            picture
        }
        Opened::Copy(None, picture) => picture,
    };
    Some((arena, picture))
}
