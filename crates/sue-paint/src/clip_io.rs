//! Clip Studio Paint files: how Sue Paint saves a picture by default, and
//! a picture file it opens like any other.
//!
//! A `.clip` Sue writes holds the picture's layers for Clip Studio and the
//! picture's own Sue project inside, so opening it again in Sue brings back
//! everything (vector lines, tones, effects) with the same entity ids. A
//! `.clip` from Clip Studio opens with its layers and folders.

use sue_core::ReversibleTransaction;
use sue_core::txn::{Relate, SpawnEntity};
use sue_core::{Arena, Bundle, EntityId, SueFile};
use sue_model::picture;
use sue_wm::AppletContext;

use crate::doc::Edit;

/// The Settings key that picks how Sue Paint saves: `clip` or `sue`.
pub const SAVE_FORMAT_KEY: &str = "sue_paint.save_format";

/// `true` when Sue Paint saves pictures as `.clip` (the default).
#[must_use]
pub fn saves_clip(cx: &AppletContext<'_>) -> bool {
    cx.rice().get_str(SAVE_FORMAT_KEY) != Some("sue")
}

/// The `.sue` bytes of `picture` and everything under it.
fn project_bytes(arena: &Arena, picture: EntityId) -> Option<Vec<u8>> {
    let bundle = Bundle::copy_from(arena, &[picture], true).ok()?;
    let mut out = Arena::new();
    for entity in bundle.entities {
        out.insert_keep_id(entity).ok()?;
    }
    for relation in bundle.relations {
        // A relation to something outside the picture stays behind.
        let _ = out.relate(relation);
    }
    let mut file = SueFile::new();
    file.set_arena(&out);
    Some(file.to_bytes())
}

/// `picture` as a `.clip`.
///
/// # Errors
/// When the picture has no size, or is too large for Clip Studio.
pub fn encode(arena: &Arena, picture: EntityId) -> Result<Vec<u8>, String> {
    let psd =
        crate::psd_io::to_psd(arena, picture).ok_or_else(|| tr!("this-picture-has-no-size"))?;
    let dpi = arena
        .get_by_id(picture)
        .ok()
        .and_then(|entity| entity.property(picture::DPI))
        .and_then(sue_core::Value::as_f64)
        .unwrap_or(72.0);
    let sue = project_bytes(arena, picture);
    sue_io::clip::encode_clip(&psd, dpi, sue.as_deref())
}

/// The picture a `.clip` from Sue carries: the edit that brings it into
/// `arena` with its ids (`None` when it is open already) and the picture.
/// `None` when `sue` holds no picture.
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

/// The picture in `.clip` `bytes` on its own: Sue's project when it carries
/// one, else a picture made from its layers. For thumbnails.
#[must_use]
pub fn picture_arena(bytes: &[u8]) -> Option<(Arena, EntityId)> {
    let clip = sue_io::clip::decode_clip(bytes).ok()?;
    let mut arena = Arena::new();
    if let Some(sue) = &clip.sue
        && let Some((Some(mut edit), picture)) = embedded(&arena, sue, "")
    {
        edit.apply(&mut arena).ok()?;
        return Some((arena, picture));
    }
    let image = sue_core::Entity::empty(String::new(), sue_model::class::IMAGE)
        .with_property(picture::WIDTH, i64::from(clip.psd.width))
        .with_property(picture::HEIGHT, i64::from(clip.psd.height))
        .with_property(picture::DPI, clip.dpi);
    let picture = image.id();
    SpawnEntity::new(image).apply(&mut arena).ok()?;
    let mut layers = crate::psd_io::import(&arena, picture, None, &clip.psd).ok()?;
    layers.apply(&mut arena).ok()?;
    Some((arena, picture))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{self, NewPicture};

    #[test]
    fn a_picture_saved_as_clip_comes_back_whole() {
        let mut arena = Arena::new();
        let spec = NewPicture {
            width: 300,
            height: 200,
            ..NewPicture::default()
        };
        let (mut edit, picture) = doc::create(&spec).unwrap();
        edit.apply(&mut arena).unwrap();
        let bytes = encode(&arena, picture).unwrap();
        let clip = sue_io::clip::decode_clip(&bytes).unwrap();
        assert_eq!((clip.psd.width, clip.psd.height), (300, 200));
        assert!(!clip.psd.layers.is_empty());
        let sue = clip.sue.expect("Sue's own project rides along");
        // Open already: nothing to spawn.
        let (none, same) = embedded(&arena, &sue, "Open").unwrap();
        assert!(none.is_none());
        assert_eq!(same, picture);
        // Into a fresh world: the same picture, with its layers.
        let mut fresh = Arena::new();
        let (edit, again) = embedded(&fresh, &sue, "Open").unwrap();
        edit.unwrap().apply(&mut fresh).unwrap();
        assert_eq!(again, picture);
        assert_eq!(
            doc::layers(&fresh, picture).len(),
            doc::layers(&arena, picture).len()
        );
        // Thumbnails read either kind.
        let (thumb, id) = picture_arena(&bytes).unwrap();
        assert_eq!(id, picture);
        assert!(
            crate::composite::flatten(
                &thumb,
                id,
                crate::composite::RenderOptions {
                    paper: true,
                    drafts: false,
                },
            )
            .is_some()
        );
        let plain = sue_io::clip::encode_clip(&clip.psd, 72.0, None).unwrap();
        let (layers_only, _) = picture_arena(&plain).unwrap();
        assert!(layers_only.iter().count() > 1);
    }
}
