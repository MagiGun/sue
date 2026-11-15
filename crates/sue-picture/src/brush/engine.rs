//! A sub tool's brush on the brush engine (`sue-brush`): what every new
//! stroke paints with.
//!
//! A sub tool keeps its brush as the legacy [`Tip`] the Tool Property
//! palette edits. [`preset_of`] reads it as a [`BrushPreset`]: for a
//! built-in sub tool whose tip is as Sue ships it, the built-in preset
//! itself (with every paint behaviour the old text has no word for:
//! spray, a dual brush, a watercolor edge); for a tip someone changed,
//! the tip's text read as a preset, with the built-in's behaviours kept
//! where the text says nothing of them. [`line_brush`] adds the tip's
//! pictures, kept by hash, so a stroke paints the same in any run and a
//! vector line keeps what it needs to be painted again.

use std::sync::Arc;

use sue_brush::paint::Resources;
use sue_core::txn::{RasterEdit, RasterPatch};
use sue_core::{EntityId, RasterTile2D};
use sue_model::BrushPreset;
use sue_model::brush::{PictureRef, TipSource};

use super::legacy::Tip;
use crate::vector::{BrushPicture, LineBrush};

/// The built-in preset of tool `key` (`pen`, `pencil`, `brush`,
/// `airbrush`, `decoration`, `eraser`) named `name`, if Sue ships one.
#[must_use]
pub fn builtin(key: &str, name: &str) -> Option<BrushPreset> {
    sue_brush::presets::of_tool(key)
        .into_iter()
        .find(|preset| preset.name == name)
}

/// `preset` with what the legacy text cannot say left out, so two
/// presets compare by what a tip holds.
fn as_tip_says(preset: &BrushPreset) -> Vec<u8> {
    let mut plain = BrushPreset::from_legacy_text(&sue_brush::presets::legacy_text(preset));
    plain.tip = TipSource::Round;
    plain.input = sue_model::brush::InputSettings::default();
    plain.to_bytes()
}

/// The preset `tip` paints with, `base` the built-in preset its sub tool
/// was made from (see the module).
#[must_use]
pub fn preset_of(tip: &Tip, base: Option<&BrushPreset>) -> BrushPreset {
    let read = BrushPreset::from_legacy_text(&tip.to_text());
    let Some(base) = base else {
        return read;
    };
    if as_tip_says(&read) == as_tip_says(base) {
        // As Sue ships it: the preset itself.
        let mut preset = base.clone();
        preset.tip = read.tip;
        return preset;
    }
    // Changed: the tip's settings, and the built-in's behaviours the
    // tip's text has no word for.
    let mut preset = read;
    for (key, bytes) in &base.extra {
        if preset.extra(key).is_none() && !key.starts_with(BrushPreset::LEGACY) {
            preset.extra.push((key.clone(), bytes.clone()));
        }
    }
    for (key, value) in &base.params {
        preset.params.entry(key.clone()).or_insert(*value);
    }
    preset
}

/// The brush `tip` paints with on the engine (see [`preset_of`]), its
/// tip pictures kept by hash.
#[must_use]
pub fn line_brush(tip: &Tip, base: Option<&BrushPreset>) -> LineBrush {
    let mut preset = preset_of(tip, base);
    let mut pictures = Vec::new();
    if let TipSource::Images { pictures: refs, .. } = &mut preset.tip {
        let count = tip.shape_count.max(1);
        let mut kept = Vec::new();
        for id in tip.shape..tip.shape.saturating_add(count) {
            if let Some(picture) = super::legacy::shape(id)
                .as_deref()
                .and_then(BrushPicture::of_shape)
            {
                let hash = picture.hash();
                kept.push(PictureRef::Hash(hash));
                pictures.push((hash, picture));
            }
        }
        if kept.is_empty() {
            // No picture to stamp: the round tip.
            preset.tip = TipSource::Round;
        } else {
            *refs = kept;
        }
    }
    LineBrush { preset, pictures }
}

/// The pictures `brush` reads, decoded, as the engine takes them.
#[must_use]
pub fn resources(brush: &LineBrush) -> Resources {
    brush.resources()
}

/// What a stroke changed in `after` against `before` (the same raster
/// before the stroke, a snapshot), over the layer pixels `rects` (`[x0,
/// y0, x1, y1]`), tile by tile, as one edit of layer `id` under `label`.
#[must_use]
pub fn edit_between(
    id: EntityId,
    label: &str,
    before: &RasterTile2D,
    after: &RasterTile2D,
    rects: &[[i32; 4]],
) -> RasterEdit {
    const TILE: i32 = 64;
    let (width, height) = (after.width() as i32, after.height() as i32);
    let mut tiles: Vec<[i32; 2]> = Vec::new();
    for rect in rects {
        let (x0, y0) = (rect[0].max(0), rect[1].max(0));
        let (x1, y1) = (rect[2].min(width), rect[3].min(height));
        if x0 >= x1 || y0 >= y1 {
            continue;
        }
        for ty in y0.div_euclid(TILE)..=(y1 - 1).div_euclid(TILE) {
            for tx in x0.div_euclid(TILE)..=(x1 - 1).div_euclid(TILE) {
                tiles.push([tx, ty]);
            }
        }
    }
    tiles.sort_unstable();
    tiles.dedup();
    let patches = tiles
        .into_iter()
        .filter_map(|[tx, ty]| {
            let (x, y) = (tx * TILE, ty * TILE);
            let (w, h) = ((x + TILE).min(width) - x, (y + TILE).min(height) - y);
            let (x, y, w, h) = (x as u32, y as u32, w as u32, h as u32);
            let old = RasterPatch::read(before, x, y, w, h)?;
            let new = RasterPatch::read(after, x, y, w, h)?;
            (old != new).then_some(RasterPatch {
                x,
                y,
                width: w,
                height: h,
                before: old,
                after: new,
            })
        })
        .collect();
    RasterEdit::new(id, label, patches)
}

/// The shared pictures of `brush`, for strokes that hold them.
#[must_use]
pub fn shared(brush: LineBrush) -> Arc<LineBrush> {
    Arc::new(brush)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{Tool, Toolbox};

    #[test]
    fn every_built_in_sub_tool_paints_with_its_own_preset() {
        let toolbox = Toolbox::default();
        for (tool, key) in [
            (Tool::Pen, "pen"),
            (Tool::Pencil, "pencil"),
            (Tool::Brush, "brush"),
            (Tool::Airbrush, "airbrush"),
            (Tool::Decoration, "decoration"),
            (Tool::Eraser, "eraser"),
        ] {
            for sub in toolbox.subs(tool) {
                let base = builtin(key, &sub.name).expect("a built-in preset");
                let preset = preset_of(&sub.tip, Some(&base));
                let mut expected = base.clone();
                expected.tip = preset.tip.clone();
                assert_eq!(
                    preset.to_bytes(),
                    expected.to_bytes(),
                    "{key} {}: the built-in preset, every behaviour kept",
                    sub.name
                );
                let brush = line_brush(&sub.tip, Some(&base));
                if let TipSource::Images { pictures, .. } = &brush.preset.tip {
                    assert!(
                        pictures.iter().all(|p| matches!(p, PictureRef::Hash(_))),
                        "{}: pictures by hash",
                        sub.name
                    );
                    assert_eq!(brush.pictures.len(), pictures.len(), "{}", sub.name);
                }
            }
        }
    }

    #[test]
    fn a_changed_tip_keeps_the_behaviours_it_has_no_word_for() {
        let base = sue_brush::presets::builtin()
            .into_iter()
            .find(|preset| {
                preset
                    .extra
                    .iter()
                    .any(|(key, _)| !key.starts_with(BrushPreset::LEGACY))
            })
            .expect("a built-in brush with behaviours");
        let mut tip = Tip::from_text(&sue_brush::presets::legacy_text(&base));
        tip.size *= 2.0;
        let preset = preset_of(&tip, Some(&base));
        assert!((preset.value(sue_model::brush::param::SIZE).unwrap_or(0.0) - tip.size).abs() < 1e-3);
        for (key, bytes) in &base.extra {
            if !key.starts_with(BrushPreset::LEGACY) {
                assert_eq!(preset.extra(key), Some(bytes.as_slice()), "{key} kept");
            }
        }
    }
}
