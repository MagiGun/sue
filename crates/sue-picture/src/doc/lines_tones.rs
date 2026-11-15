//! Convert to Lines and Tones: a raster layer split into a layer of its
//! lines and a layer of its shading shown as tone, with the settings Clip
//! Studio Paint's dialog has (CTV-1). The dialog and its live preview are
//! the studio's; this is the conversion.

use sue_core::txn::SetProperty;
use sue_core::{Arena, EntityId, PixelFormat, RasterTile2D};
use sue_model::picture;

use super::{Edit, add_layers_pixels, raster, size_of};
use crate::filters::Filter;

/// How a layer becomes lines and tones.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinesAndTones {
    /// Lines are made at all.
    pub lines: bool,
    /// How faint an edge still becomes a line, 0 to 1.
    pub sensitivity: f32,
    /// Areas darker than this fill black, 0 (none) to 1.
    pub black: f32,
    /// How much the picture is smoothed first, in pixels.
    pub smooth: f32,
    /// Tones are made at all.
    pub tones: bool,
    /// Steps of tone density (0 or 1: as many as the picture has).
    pub levels: u8,
    /// The dots' shape (an index of [`crate::effects::TONE_SHAPES`]).
    pub shape: u8,
    /// The dots' screen frequency in lines an inch.
    pub frequency: f32,
    /// The dots' angle in degrees.
    pub angle: f32,
}

impl Default for LinesAndTones {
    /// What Sue converted with before the settings existed.
    fn default() -> Self {
        Self {
            lines: true,
            sensitivity: 0.5,
            black: 0.12,
            smooth: 1.0,
            tones: true,
            levels: 0,
            shape: 0,
            frequency: 60.0,
            angle: 45.0,
        }
    }
}

/// The edit that converts raster `layer` to a tone layer and a lines layer
/// right above it (as `settings` ask), hiding `layer`, and their ids
/// (tones first; either is missing when not asked for).
///
/// # Errors
/// If `layer` is not an 8-bit raster layer of the picture's size at its
/// corner, or neither lines nor tones are asked for.
pub fn lines_and_tones(
    arena: &Arena,
    picture: EntityId,
    layer: EntityId,
    settings: &LinesAndTones,
) -> Result<(Edit, Option<EntityId>, Option<EntityId>), String> {
    if !settings.lines && !settings.tones {
        return Err(tr!("pick-raster-layer-convert-lines-tones"));
    }
    let size = size_of(arena, picture)?;
    let tile = raster(arena, layer).ok_or(tr!("pick-raster-layer-convert-lines-tones"))?;
    let at = arena
        .get_by_id(layer)
        .map(|entity| picture::LayerStyle::of(entity).offset)
        .unwrap_or_default();
    if [tile.width(), tile.height()] != size || at != [0, 0] {
        return Err(tr!("move-layer-back-into-place-merge"));
    }
    let bytes = match (tile.format(), tile.bytes()) {
        (PixelFormat::Rgba8, Some(bytes)) => bytes,
        _ => return Err(tr!("pick-raster-layer-convert-lines-tones")),
    };
    let make = |pixels: Vec<u8>| {
        RasterTile2D::from_bytes(size[0], size[1], PixelFormat::Rgba8, pixels)
            .map_err(|error| error.to_string())
    };
    let mut made = Vec::new();
    if settings.tones {
        // The shading: darker is more ink.
        let tones: Vec<u8> = bytes
            .chunks(4)
            .flat_map(|p| {
                let a = f32::from(p[3]) / 255.0;
                let l =
                    (0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]))
                        / 255.0;
                let gray = (l * 255.0).round() as u8;
                [
                    gray,
                    gray,
                    gray,
                    ((1.0 - l) * a * 255.0).round().min(255.0) as u8,
                ]
            })
            .collect();
        made.push((tr!("tones"), make(tones)?));
    }
    if settings.lines {
        let mut lines = bytes.to_vec();
        Filter::LineExtraction {
            sensitivity: settings.sensitivity.clamp(0.0, 1.0),
            black: settings.black.clamp(0.0, 1.0),
            smooth: settings.smooth.max(0.0),
        }
        .apply(&mut lines, size[0], size[1], None, [0, 0]);
        made.push((tr!("lines"), make(lines)?));
    }
    let (spawns, ids) = add_layers_pixels(arena, picture, Some(layer), made)?;
    let mut edit = Edit::new(tr!("convert-to-lines-and-tones"));
    for spawn in spawns {
        edit = edit.then(spawn);
    }
    let tones = settings.tones.then(|| ids[0]);
    let lines = settings.lines.then(|| ids[ids.len() - 1]);
    if let Some(tones) = tones {
        edit = edit
            .then(SetProperty::new(tones, crate::effects::TONE, true))
            .then(SetProperty::new(
                tones,
                crate::effects::TONE_LINES,
                f64::from(settings.frequency.max(1.0)),
            ))
            .then(SetProperty::new(
                tones,
                crate::effects::TONE_ANGLE,
                f64::from(settings.angle),
            ))
            .then(SetProperty::new(
                tones,
                crate::effects::TONE_SHAPE,
                i64::from(settings.shape),
            ))
            .then(SetProperty::new(
                tones,
                crate::effects::TONE_LEVELS,
                i64::from(settings.levels),
            ));
    }
    Ok((
        edit.then(SetProperty::new(layer, picture::VISIBLE, false)),
        tones,
        lines,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{NewPicture, create, layers};
    use sue_core::{Color, UndoStack};

    #[test]
    fn a_layer_becomes_lines_and_tones_as_the_settings_say() {
        let mut arena = Arena::new();
        let mut history = UndoStack::new();
        let (edit, picture) = create(&NewPicture {
            width: 16,
            height: 16,
            paper: Some(Color::srgb8(255, 255, 255)),
            ..NewPicture::default()
        })
        .unwrap();
        history.apply(&mut arena, edit).unwrap();
        let layer = layers(&arena, picture).last().unwrap().id;
        let pixels = crate::doc::raster_mut(&mut arena, layer)
            .unwrap()
            .bytes_mut()
            .unwrap();
        for (index, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let gray = if index % 16 < 8 { 60 } else { 200 };
            *pixel = [gray, gray, gray, 255];
        }
        let settings = LinesAndTones {
            shape: 3,
            levels: 4,
            angle: 0.0,
            ..LinesAndTones::default()
        };
        let (edit, tones, lines) = lines_and_tones(&arena, picture, layer, &settings).unwrap();
        history.apply(&mut arena, edit).unwrap();
        let (tones, lines) = (tones.unwrap(), lines.unwrap());
        let stack: Vec<EntityId> = layers(&arena, picture).iter().map(|info| info.id).collect();
        assert_eq!(&stack[stack.len() - 3..], &[layer, tones, lines]);
        let effects = crate::effects::Effects::of(arena.get_by_id(tones).unwrap(), 72.0);
        assert_eq!((effects.tone_shape, effects.tone_levels), (3, 4));
        assert!(!picture::LayerStyle::of(arena.get_by_id(layer).unwrap()).visible);
        history.undo(&mut arena).unwrap();
        assert_eq!(layers(&arena, picture).len(), 2);
        // Only the lines.
        let only = LinesAndTones {
            tones: false,
            ..LinesAndTones::default()
        };
        let (_, tones, lines) = lines_and_tones(&arena, picture, layer, &only).unwrap();
        assert!(tones.is_none() && lines.is_some());
        let neither = LinesAndTones {
            tones: false,
            lines: false,
            ..LinesAndTones::default()
        };
        assert!(lines_and_tones(&arena, picture, layer, &neither).is_err());
    }
}
