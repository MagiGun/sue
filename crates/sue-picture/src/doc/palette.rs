//! Palette color: a color a person gives layers and folders to tell them
//! apart in the Layer palette and on the timeline's tracks. It is a label
//! only; the picture never shows it.

use sue_core::txn::SetProperty;
use sue_core::{Arena, Color, EntityId, Value};

use super::Edit;

/// A layer's or folder's palette color (`Color`; none when missing).
pub const PALETTE_COLOR: &str = "layer.palette_color";

/// The palette colors offered by name, in menu order, as stable names
/// (saved data) and their colors; "Use other color" picks any other.
pub const PALETTE_COLORS: [(&str, [u8; 3]); 6] = [
    ("red", [226, 76, 76]),
    ("orange", [238, 146, 60]),
    ("yellow", [232, 204, 72]),
    ("green", [92, 178, 96]),
    ("blue", [74, 132, 220]),
    ("purple", [156, 98, 206]),
];

/// The palette color of `layer`, if it has one.
#[must_use]
pub fn palette_color(arena: &Arena, layer: EntityId) -> Option<Color> {
    match arena.get_by_id(layer).ok()?.property(PALETTE_COLOR) {
        Some(Value::Color(color)) => Some(*color),
        _ => None,
    }
}

/// The color of the offered palette color named `name` (see
/// [`PALETTE_COLORS`]).
#[must_use]
pub fn named_palette_color(name: &str) -> Option<Color> {
    PALETTE_COLORS
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, [r, g, b])| Color::srgb8(*r, *g, *b))
}

/// The edit that gives every layer of `ids` the palette color `color`
/// (`None`: No Palette Color), as one step; `None` when nothing changes.
#[must_use]
pub fn set_palette_color(arena: &Arena, ids: &[EntityId], color: Option<Color>) -> Option<Edit> {
    let mut edit = Edit::new(tr!("change-palette-color"));
    let mut changed = false;
    for id in ids {
        if arena.get_by_id(*id).is_err() || palette_color(arena, *id) == color {
            continue;
        }
        edit = edit.then(match color {
            Some(color) => SetProperty::new(*id, PALETTE_COLOR, color),
            None => SetProperty::remove(*id, PALETTE_COLOR),
        });
        changed = true;
    }
    changed.then_some(edit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{NewPicture, create, layers, new_folder};
    use sue_core::{ReversibleTransaction, UndoStack};

    #[test]
    fn palette_colors_go_on_every_picked_layer_and_come_off() {
        let mut arena = Arena::new();
        let mut history = UndoStack::new();
        let (edit, picture) = create(&NewPicture::default()).unwrap();
        history.apply(&mut arena, edit).unwrap();
        let layer = layers(&arena, picture).last().unwrap().id;
        let (edit, folder) = new_folder(&arena, picture, Some(layer), true);
        history.apply(&mut arena, edit).unwrap();
        let red = named_palette_color("red").unwrap();
        let edit = set_palette_color(&arena, &[layer, folder], Some(red)).unwrap();
        assert_eq!(edit.label(), "Change Palette Color");
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(palette_color(&arena, layer), Some(red));
        assert_eq!(palette_color(&arena, folder), Some(red));
        assert!(set_palette_color(&arena, &[layer], Some(red)).is_none());
        let edit = set_palette_color(&arena, &[folder], None).unwrap();
        history.apply(&mut arena, edit).unwrap();
        assert_eq!(palette_color(&arena, folder), None);
        history.undo(&mut arena).unwrap();
        assert_eq!(palette_color(&arena, folder), Some(red));
        history.undo(&mut arena).unwrap();
        assert_eq!(palette_color(&arena, layer), None);
        assert!(named_palette_color("teal").is_none());
        let mut names: Vec<&str> = PALETTE_COLORS.iter().map(|(name, _)| *name).collect();
        names.dedup();
        assert_eq!(names.len(), 6);
    }
}
