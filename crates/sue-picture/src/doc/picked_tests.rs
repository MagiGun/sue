//! The picture model's edits of several layers and of folders, each with
//! its undo pair: done, undone (the arena as before, ids included) and
//! redone (the arena as after).

use sue_core::{
    Arena, Color, ComponentKind, EntityId, PixelFormat, RasterTile2D, ReversibleTransaction,
    UndoStack, Value,
};
use sue_model::picture;

use super::*;
use crate::composite::{RenderOptions, render};

/// Everything an edit can change, in an order that does not depend on
/// where entities sit in the arena: each entity (id, name, class,
/// properties, pixels) and each relation.
fn state(arena: &Arena) -> (Vec<String>, Vec<String>) {
    let mut entities: Vec<String> = arena
        .iter()
        .map(|(_, entity)| {
            let mut properties: Vec<String> = entity
                .properties()
                .iter()
                .map(|(key, value)| format!("{key}={value:?}"))
                .collect();
            properties.sort();
            let pixels = entity
                .component(ComponentKind::RasterTile2D)
                .and_then(|component| component.as_raster())
                .map(|raster| {
                    (
                        raster.width(),
                        raster.height(),
                        raster.bytes().map(<[u8]>::to_vec),
                    )
                });
            format!(
                "{:?} {} {} {properties:?} {pixels:?}",
                entity.id(),
                entity.name(),
                entity.class()
            )
        })
        .collect();
    entities.sort();
    let mut relations: Vec<String> = arena
        .relations()
        .iter()
        .map(|relation| {
            format!(
                "{:?} {} {:?} {:?}",
                relation.from, relation.kind, relation.to, relation.order
            )
        })
        .collect();
    relations.sort();
    (entities, relations)
}

/// Applies `edit`, then checks its undo pair: undone, the arena is as
/// before; redone, as after.
fn undo_pair(arena: &mut Arena, history: &mut UndoStack<Arena>, edit: Edit) {
    let before = state(arena);
    history.apply(arena, edit).unwrap();
    let after = state(arena);
    assert_ne!(before, after, "the edit changes something");
    history.undo(arena).unwrap();
    assert_eq!(state(arena), before, "undo is exact");
    history.redo(arena).unwrap();
    assert_eq!(state(arena), after, "redo is exact");
}

fn picture_of(width: u32, height: u32) -> (Arena, EntityId, UndoStack<Arena>, EntityId) {
    let mut arena = Arena::new();
    let mut history = UndoStack::new();
    let (edit, picture) = create(&NewPicture {
        width,
        height,
        paper: Some(Color::srgb8(255, 255, 255)),
        ..NewPicture::default()
    })
    .unwrap();
    history.apply(&mut arena, edit).unwrap();
    let first = layers(&arena, picture).last().unwrap().id;
    (arena, picture, history, first)
}

fn ids(arena: &Arena, picture: EntityId) -> Vec<EntityId> {
    layers(arena, picture).iter().map(|info| info.id).collect()
}

fn fill(arena: &mut Arena, layer: EntityId, rgba: [u8; 4]) {
    for pixel in raster_mut(arena, layer)
        .unwrap()
        .bytes_mut()
        .unwrap()
        .as_chunks_mut::<4>()
        .0
    {
        *pixel = rgba;
    }
}

#[test]
fn new_layers_land_in_the_picked_folder() {
    let (mut arena, picture, mut history, first) = picture_of(4, 2);
    let (edit, folder) = new_folder(&arena, picture, Some(first), true);
    history.apply(&mut arena, edit).unwrap();
    // The folder picked: a new layer goes in it, on top of what it holds.
    let (spawn, raster_layer) =
        add_raster_layer(&arena, picture, Some(folder), "In".to_owned(), None).unwrap();
    undo_pair(&mut arena, &mut history, Edit::new("Add").then(spawn));
    assert_eq!(folder_of(&arena, raster_layer), Some(folder));
    let stack = ids(&arena, picture);
    assert_eq!(&stack[1..], &[first, raster_layer, folder]);
    // A layer in it picked: right above that layer, in the same folder.
    let (spawn, vector) = add_vector_layer(&arena, picture, Some(first), "Ink".to_owned()).unwrap();
    history
        .apply(&mut arena, Edit::new("Add").then(spawn))
        .unwrap();
    assert_eq!(folder_of(&arena, vector), Some(folder));
    assert_eq!(
        &ids(&arena, picture)[1..],
        &[first, vector, raster_layer, folder]
    );
    let (edit, correction) = new_correction(&arena, picture, Some(vector), "Fix", "invert");
    history.apply(&mut arena, edit).unwrap();
    assert_eq!(folder_of(&arena, correction), Some(folder));
    let tile = || RasterTile2D::new(4, 2, PixelFormat::Rgba8).unwrap();
    let (spawns, made) = add_layers_pixels(
        &arena,
        picture,
        Some(folder),
        vec![("A".to_owned(), tile()), ("B".to_owned(), tile())],
    )
    .unwrap();
    let mut edit = Edit::new("Add");
    for spawn in spawns {
        edit = edit.then(spawn);
    }
    history.apply(&mut arena, edit).unwrap();
    assert!(made.iter().all(|id| folder_of(&arena, *id) == Some(folder)));
    let stack = ids(&arena, picture);
    assert_eq!(&stack[stack.len() - 3..], &[made[0], made[1], folder]);
    // A new folder with a member picked goes in that member's folder.
    let (edit, inner) = new_folder(&arena, picture, Some(first), false);
    history.apply(&mut arena, edit).unwrap();
    assert_eq!(folder_of(&arena, inner), Some(folder));
    // Nothing picked: on top, in no folder.
    let (spawn, loose) = add_layer_pixels(&arena, picture, None, "Top".to_owned(), tile()).unwrap();
    history
        .apply(&mut arena, Edit::new("Add").then(spawn))
        .unwrap();
    assert_eq!(folder_of(&arena, loose), None);
    assert_eq!(ids(&arena, picture).last(), Some(&loose));
}

#[test]
fn a_folder_duplicates_with_what_it_holds_masks_and_track() {
    let (mut arena, picture, mut history, first) = picture_of(4, 2);
    fill(&mut arena, first, [10, 20, 30, 255]);
    let (edit, _) = new_mask(&arena, first, |x, _| if x < 2 { 1.0 } else { 0.0 }).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let (edit, inner) = new_folder(&arena, picture, Some(first), true);
    history.apply(&mut arena, edit).unwrap();
    let (edit, outer) = new_folder(&arena, picture, Some(inner), true);
    history.apply(&mut arena, edit).unwrap();
    history
        .apply(&mut arena, set(outer, picture::BLEND, "multiply"))
        .unwrap();
    let shown = |arena: &Arena| render(arena, picture, [0, 0, 4, 2], RenderOptions::default());
    let before_pixels = shown(&arena);
    let (edit, copy) = duplicate(&arena, picture, outer).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    // The copy sits right above the original, outside it, and holds
    // copies of all it held, under new ids.
    let stack = ids(&arena, picture);
    let at = stack.iter().position(|id| *id == outer).unwrap();
    assert_eq!(stack.last(), Some(&copy));
    assert_eq!(stack.len(), at + 4);
    assert_eq!(folder_of(&arena, copy), None);
    let held = members(&arena, picture, copy);
    assert_eq!(held.len(), 2, "a folder and a layer");
    assert!(held.iter().all(|id| *id != first && *id != inner));
    let copied_layer = held[0];
    assert_eq!(folder_of(&arena, copied_layer), Some(held[1]));
    assert_eq!(folder_of(&arena, held[1]), Some(copy));
    assert_eq!(
        raster(&arena, copied_layer).unwrap().bytes(),
        raster(&arena, first).unwrap().bytes()
    );
    let (mask, copied_mask) = (mask_of(&arena, first), mask_of(&arena, copied_layer));
    assert!(copied_mask.is_some() && copied_mask != mask, "its own mask");
    assert_eq!(folder_blend(&arena, copy), sue_model::BlendMode::Multiply);
    assert_eq!(arena.get_by_id(copy).unwrap().name(), "Folder 2 copy");
    // The original is as it was, and the picture looks the same twice.
    assert_eq!(members(&arena, picture, outer), vec![first, inner]);
    // The copy multiplies over the original where the mask shows it; the
    // paper elsewhere stays white.
    let now = shown(&arena);
    assert_eq!(&now[8..16], &before_pixels[8..16]);
    assert_eq!(&now[..4], &[0, 2, 4, 255], "10, 20, 30 times themselves");
    // An animation folder's copy gets a track of its own showing its cels.
    let (edit, animation, track) = crate::anim::add_folder(&arena, picture, None);
    history.apply(&mut arena, edit).unwrap();
    let (edit, cel) = crate::anim::new_cel(&arena, picture, animation, 0).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let (edit, twin) = duplicate(&arena, picture, animation).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    let sequence = crate::anim::timeline(&arena, picture).unwrap();
    let copied = crate::anim::track_of(sequence, twin).expect("a track for the copy");
    assert_eq!(copied, track + 1, "right after the original's");
    let twin_cel = members(&arena, picture, twin)[0];
    assert_ne!(twin_cel, cel);
    assert_eq!(
        sequence.tracks[copied]
            .clips()
            .iter()
            .map(|clip| clip.source)
            .collect::<Vec<_>>(),
        vec![twin_cel]
    );
}

#[test]
fn picked_layers_show_hide_lock_and_go_together() {
    let (mut arena, picture, mut history, first) = picture_of(4, 2);
    let (spawn, second) =
        add_raster_layer(&arena, picture, Some(first), "Two".to_owned(), None).unwrap();
    history
        .apply(&mut arena, Edit::new("Add").then(spawn))
        .unwrap();
    let (edit, folder) = new_folder(&arena, picture, Some(second), true);
    history.apply(&mut arena, edit).unwrap();
    let picked = [first, folder];
    let edit = set_layers(&arena, &picked, picture::LOCKED, true).unwrap();
    assert_eq!(edit.label(), "Lock Layer");
    undo_pair(&mut arena, &mut history, edit);
    assert!(
        picked
            .iter()
            .all(|id| picture::is_locked(arena.get_by_id(*id).unwrap()))
    );
    assert!(set_layers(&arena, &picked, picture::LOCKED, true).is_none());
    let edit = set_layers(&arena, &picked, picture::VISIBLE, false).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    // Deleting a folder takes what it holds; the paper keeps the picture.
    let edit = delete_layers(&arena, picture, &picked).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    assert_eq!(layers(&arena, picture).len(), 1, "only the paper");
    assert!(arena.get_by_id(second).is_err());
    history.undo(&mut arena).unwrap();
    let paper = layers(&arena, picture)[0].id;
    assert!(
        delete_layers(&arena, picture, &[paper, first, folder]).is_err(),
        "something stays"
    );
}

#[test]
fn create_folder_and_insert_layer_takes_every_picked_layer() {
    let (mut arena, picture, mut history, first) = picture_of(4, 2);
    let mut made = vec![first];
    for name in ["Two", "Three", "Four"] {
        let (spawn, id) =
            add_raster_layer(&arena, picture, made.last().copied(), name.to_owned(), None).unwrap();
        history
            .apply(&mut arena, Edit::new("Add").then(spawn))
            .unwrap();
        made.push(id);
    }
    // Pick One and Three: they go into a new folder above Three, in their
    // order; Two stays where it is.
    let (edit, folder) = folder_insert(&arena, picture, &[made[2], made[0]]).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    let paper = layers(&arena, picture)[0].id;
    assert_eq!(
        ids(&arena, picture),
        vec![paper, made[1], made[0], made[2], folder, made[3]]
    );
    assert_eq!(members(&arena, picture, folder), vec![made[0], made[2]]);
    // Again with the folder and Four: a folder inside a new folder.
    let (edit, outer) = folder_insert(&arena, picture, &[folder, made[3]]).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    assert_eq!(folders_around(&arena, made[0]), vec![folder, outer]);
    assert_eq!(folder_of(&arena, made[3]), Some(outer));
    assert_eq!(ids(&arena, picture).last(), Some(&outer));
}

#[test]
fn move_layer_moves_every_picked_layer_and_what_folders_hold() {
    let (mut arena, picture, mut history, first) = picture_of(4, 2);
    let (spawn, second) =
        add_raster_layer(&arena, picture, Some(first), "Two".to_owned(), None).unwrap();
    history
        .apply(&mut arena, Edit::new("Add").then(spawn))
        .unwrap();
    let (edit, folder) = new_folder(&arena, picture, Some(second), true);
    history.apply(&mut arena, edit).unwrap();
    let edit = move_layers(&arena, picture, &[first, folder], [3, -1]).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    for id in [first, second] {
        assert_eq!(
            sue_model::picture::LayerStyle::of(arena.get_by_id(id).unwrap()).offset,
            [3, -1]
        );
    }
    assert!(move_layers(&arena, picture, &[first], [0, 0]).is_none());
}

#[test]
fn merging_a_folder_bakes_it_into_one_layer() {
    // As in the Crash Course: group, duplicate, hide the original, Merge
    // Selected Layers on the copy: one raster layer with the copy's name,
    // the masks applied, the original untouched.
    let (mut arena, picture, mut history, first) = picture_of(4, 1);
    fill(&mut arena, first, [200, 0, 0, 255]);
    let (edit, _) = new_mask(&arena, first, |x, _| if x < 2 { 1.0 } else { 0.0 }).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let (edit, folder) = new_folder(&arena, picture, Some(first), true);
    history.apply(&mut arena, edit).unwrap();
    let (edit, copy) = duplicate(&arena, picture, folder).unwrap();
    history.apply(&mut arena, edit).unwrap();
    history
        .apply(&mut arena, set(folder, picture::VISIBLE, false))
        .unwrap();
    let before = render(&arena, picture, [0, 0, 4, 1], RenderOptions::default());
    let (edit, merged) = merge_layers(&arena, picture, &[copy]).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    assert!(arena.get_by_id(copy).is_err());
    assert!(!is_folder(&arena, merged));
    assert_eq!(arena.get_by_id(merged).unwrap().name(), "Folder 1 copy");
    assert_eq!(mask_of(&arena, merged), None, "the mask is in the pixels");
    let alpha: Vec<u8> = raster(&arena, merged)
        .unwrap()
        .bytes()
        .unwrap()
        .iter()
        .skip(3)
        .step_by(4)
        .copied()
        .collect();
    assert_eq!(alpha, [255, 255, 0, 0]);
    assert_eq!(
        render(&arena, picture, [0, 0, 4, 1], RenderOptions::default()),
        before,
        "it looks the same"
    );
    assert_eq!(members(&arena, picture, folder), vec![first]);
    // Merge Down on a folder lays it on the layer below.
    let (spawn, base) = add_layer_pixels(
        &arena,
        picture,
        Some(layers(&arena, picture)[0].id),
        "Base".to_owned(),
        RasterTile2D::from_bytes(4, 1, PixelFormat::Rgba8, [0, 0, 255, 255].repeat(4)).unwrap(),
    )
    .unwrap();
    history
        .apply(&mut arena, Edit::new("Add").then(spawn))
        .unwrap();
    history
        .apply(&mut arena, set(folder, picture::VISIBLE, true))
        .unwrap();
    let before = render(&arena, picture, [0, 0, 4, 1], RenderOptions::default());
    let (edit, into) = merge_down(&arena, picture, folder).unwrap();
    assert_eq!(into, base);
    undo_pair(&mut arena, &mut history, edit);
    assert!(arena.get_by_id(folder).is_err() && arena.get_by_id(first).is_err());
    let pixels = raster(&arena, base).unwrap().bytes().unwrap().to_vec();
    assert_eq!(&pixels[..4], &[200, 0, 0, 255]);
    assert_eq!(&pixels[8..12], &[0, 0, 255, 255]);
    assert_eq!(
        render(&arena, picture, [0, 0, 4, 1], RenderOptions::default()),
        before
    );
}

#[test]
fn rasterize_bakes_the_effects_of_any_layer() {
    let (mut arena, picture, mut history, first) = picture_of(5, 1);
    raster_mut(&mut arena, first).unwrap().bytes_mut().unwrap()[8..12]
        .copy_from_slice(&[0, 0, 0, 255]);
    // A white border round the one black pixel, and half opacity.
    history
        .apply(&mut arena, set(first, crate::effects::BORDER, true))
        .unwrap();
    history
        .apply(&mut arena, set(first, crate::effects::BORDER_WIDTH, 1.0))
        .unwrap();
    history
        .apply(
            &mut arena,
            set(first, crate::effects::BORDER_COLOR, Color::srgb8(255, 0, 0)),
        )
        .unwrap();
    history
        .apply(&mut arena, set(first, picture::OPACITY, 0.5))
        .unwrap();
    assert!(rasterizes(&arena, first));
    let before = render(&arena, picture, [0, 0, 5, 1], RenderOptions::default());
    let (edit, flat) = rasterize(&arena, picture, first).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    let entity = arena.get_by_id(flat).unwrap();
    assert!(
        entity
            .properties()
            .iter()
            .all(|(key, _)| !key.starts_with("effect.")),
        "the effects are in the pixels"
    );
    assert_eq!(entity.property(picture::OPACITY), Some(&Value::from(0.5)));
    assert!(!rasterizes(&arena, flat));
    let pixels = raster(&arena, flat).unwrap().bytes().unwrap().to_vec();
    assert_eq!(&pixels[8..12], &[0, 0, 0, 255], "the layer's own pixel");
    assert!(
        pixels[4] == 255 && pixels[5] == 0 && pixels[7] > 0,
        "the border, red: {pixels:?}"
    );
    assert_eq!(
        render(&arena, picture, [0, 0, 5, 1], RenderOptions::default()),
        before,
        "it looks the same"
    );
    // A folder rasterizes into one layer of what it shows.
    let (edit, folder) = new_folder(&arena, picture, Some(flat), true);
    history.apply(&mut arena, edit).unwrap();
    let (edit, whole) = rasterize(&arena, picture, folder).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    assert!(arena.get_by_id(folder).is_err() && arena.get_by_id(flat).is_err());
    // The folder's composite holds the layer's half opacity now: the same
    // picture, give or take the rounding of one more step.
    let now = render(&arena, picture, [0, 0, 5, 1], RenderOptions::default());
    assert!(
        now.iter().zip(&before).all(|(a, b)| a.abs_diff(*b) <= 1),
        "{now:?} against {before:?}"
    );
    assert_eq!(arena.get_by_id(whole).unwrap().name(), "Folder 1");
}

#[test]
fn masks_and_live_layers_follow_the_canvas_size_and_resolution() {
    let (mut arena, picture, mut history, first) = picture_of(4, 1);
    fill(&mut arena, first, [0, 0, 0, 255]);
    let (edit, _) = new_mask(&arena, first, |x, _| if x < 2 { 1.0 } else { 0.0 }).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let gradient = Gradient {
        line: [[0.0, 0.0], [4.0, 0.0]],
        colors: [[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]],
        radial: false,
    };
    let (edit, live) = new_gradient_layer(&arena, picture, None, "Grad", gradient);
    history.apply(&mut arena, edit).unwrap();
    history
        .apply(&mut arena, set(live, picture::OPACITY, 0.0))
        .unwrap();
    let alpha = |arena: &Arena, width: u32| -> Vec<u8> {
        render(
            arena,
            picture,
            [0, 0, width, 1],
            RenderOptions {
                paper: false,
                drafts: true,
            },
        )
        .iter()
        .skip(3)
        .step_by(4)
        .copied()
        .collect()
    };
    assert_eq!(alpha(&arena, 4), [255, 255, 0, 0]);
    // Two more pixels on the left: the drawing and its mask move along.
    let edit = resize_canvas(&arena, picture, [6, 1], [2, 0]).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    assert_eq!(alpha(&arena, 6), [0, 0, 255, 255, 0, 0]);
    let line = Gradient::of(arena.get_by_id(live).unwrap()).line;
    assert_eq!(line, [[2.0, 0.0], [6.0, 0.0]], "the gradient moved too");
    // Twice the resolution: so do they.
    let edit = resample(&arena, picture, [12, 2], 144.0).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    let wide: Vec<u8> = alpha(&arena, 12);
    // Smoothed at its edges (and, one pixel high before, a little
    // across too).
    assert_eq!(&wide[..3], &[0, 0, 0]);
    assert!(wide[5] == wide[6] && wide[5] > 100, "{wide:?}");
    assert_eq!(&wide[10..], &[0, 0]);
    let line = Gradient::of(arena.get_by_id(live).unwrap()).line;
    assert_eq!(line, [[4.0, 0.0], [12.0, 0.0]]);
}

#[test]
fn a_mask_stays_on_its_layer_when_a_moved_layer_changes_resolution() {
    // An 8 by 2 picture: the layer moved two pixels right, its mask
    // showing it at picture x 2 and 3 only.
    let (mut arena, picture, mut history, first) = picture_of(8, 2);
    fill(&mut arena, first, [0, 0, 0, 255]);
    history
        .apply(&mut arena, set(first, picture::X, 2_i64))
        .unwrap();
    let (edit, _) = new_mask(
        &arena,
        first,
        |x, _| {
            if (2..4).contains(&x) { 1.0 } else { 0.0 }
        },
    )
    .unwrap();
    history.apply(&mut arena, edit).unwrap();
    let alpha = |arena: &Arena, width: u32| -> Vec<u8> {
        render(
            arena,
            picture,
            [0, 0, width, 2],
            RenderOptions {
                paper: false,
                drafts: true,
            },
        )
        .iter()
        .skip(3)
        .step_by(4)
        .take(width as usize)
        .copied()
        .collect()
    };
    assert_eq!(alpha(&arena, 8), [0, 0, 255, 255, 0, 0, 0, 0]);
    // Twice the resolution: the layer sits at x 4, and its mask still
    // shows it at picture x 4 to 7 (smoothed at the edges), not further
    // right by the layer's offset.
    let edit = resample(&arena, picture, [16, 4], 144.0).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    let wide = alpha(&arena, 16);
    assert_eq!(&wide[..3], &[0, 0, 0], "{wide:?}");
    assert!(wide[5] == wide[6] && wide[5] > 100, "{wide:?}");
    assert_eq!(&wide[9..], &[0; 7], "{wide:?}");
    // The mask lies over the layer's pixels, their size, as painting a
    // mask expects.
    let aligned = |arena: &Arena| {
        let mask = mask_of(arena, first).unwrap();
        let (layer, cover) = (raster(arena, first).unwrap(), raster(arena, mask).unwrap());
        let at = picture::LayerStyle::of(arena.get_by_id(first).unwrap()).offset;
        assert_eq!(mask_offset(arena, first), Some(at));
        assert_eq!(
            [layer.width(), layer.height()],
            [cover.width(), cover.height()]
        );
    };
    aligned(&arena);
    // Back to the first canvas, cut two pixels on the left: the mask
    // keeps to the drawing there too.
    history.undo(&mut arena).unwrap();
    let edit = resize_canvas(&arena, picture, [6, 2], [-2, 0]).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    assert_eq!(alpha(&arena, 6), [255, 255, 0, 0, 0, 0]);
    aligned(&arena);
}

#[test]
fn a_quarter_turn_turns_moved_layers_with_their_masks() {
    // A 5 by 3 picture, a layer moved one right and its mask showing a
    // corner of it, and the mask of a second layer unlinked: the turned
    // picture is the picture turned, both ways.
    let (mut arena, picture, mut history, first) = picture_of(5, 3);
    for (index, pixel) in raster_mut(&mut arena, first)
        .unwrap()
        .bytes_mut()
        .unwrap()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .enumerate()
    {
        *pixel = [(index * 17) as u8, 40, 200, 255];
    }
    history
        .apply(&mut arena, set(first, picture::X, 1_i64))
        .unwrap();
    let (edit, _) = new_mask(&arena, first, |x, y| if x < 3 && y < 2 { 1.0 } else { 0.3 }).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let (spawn, second) = add_raster_layer(
        &arena,
        picture,
        Some(first),
        "Top".to_owned(),
        Some([250, 200, 0, 255]),
    )
    .unwrap();
    history
        .apply(&mut arena, Edit::new("Add").then(spawn))
        .unwrap();
    let (edit, _) = new_mask(&arena, second, |x, _| if x == 4 { 1.0 } else { 0.0 }).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let unlink = set_mask_linked(&arena, second, false).unwrap();
    history.apply(&mut arena, unlink).unwrap();
    let shown = |arena: &Arena, size: [u32; 2]| -> Vec<[u8; 4]> {
        render(
            arena,
            picture,
            [0, 0, size[0], size[1]],
            RenderOptions::default(),
        )
        .as_chunks::<4>()
        .0
        .to_vec()
    };
    let before = shown(&arena, [5, 3]);
    for clockwise in [true, false] {
        let edit = rotate_canvas(&arena, picture, clockwise).unwrap();
        undo_pair(&mut arena, &mut history, edit);
        let after = shown(&arena, [3, 5]);
        for y in 0..5_usize {
            for x in 0..3_usize {
                // Where the turned pixel came from.
                let (ox, oy) = if clockwise { (y, 2 - x) } else { (4 - y, x) };
                assert_eq!(after[y * 3 + x], before[oy * 5 + ox], "{clockwise} {x},{y}");
            }
        }
        history.undo(&mut arena).unwrap();
    }
}

#[test]
fn a_moved_layers_mask_keeps_to_it_through_merge_down_and_rasterize() {
    // The mask lies over the layer's pixels, their size, as painting a
    // mask expects, and the picture shows the same.
    fn aligned(arena: &Arena, layer: EntityId) {
        let mask = mask_of(arena, layer).unwrap();
        let (pixels, cover) = (raster(arena, layer).unwrap(), raster(arena, mask).unwrap());
        let at = picture::LayerStyle::of(arena.get_by_id(layer).unwrap()).offset;
        assert_eq!(mask_offset(arena, layer), Some(at));
        assert_eq!(
            [pixels.width(), pixels.height()],
            [cover.width(), cover.height()]
        );
    }
    let (mut arena, picture, mut history, first) = picture_of(6, 2);
    fill(&mut arena, first, [10, 20, 30, 255]);
    history
        .apply(&mut arena, set(first, picture::X, 2_i64))
        .unwrap();
    let (edit, _) = new_mask(&arena, first, |x, _| if x < 4 { 1.0 } else { 0.0 }).unwrap();
    history.apply(&mut arena, edit).unwrap();
    // Over it a layer with a layer color: merging it down bakes.
    let (spawn, top) = add_raster_layer(
        &arena,
        picture,
        Some(first),
        "Top".to_owned(),
        Some([200, 200, 200, 128]),
    )
    .unwrap();
    history
        .apply(&mut arena, Edit::new("Add").then(spawn))
        .unwrap();
    history
        .apply(&mut arena, set(top, crate::effects::COLOR, true))
        .unwrap();
    let shown = |arena: &Arena| render(arena, picture, [0, 0, 6, 2], RenderOptions::default());
    let before = shown(&arena);
    let (edit, into) = merge_down(&arena, picture, top).unwrap();
    assert_eq!(into, first);
    undo_pair(&mut arena, &mut history, edit);
    // The layer below keeps its mask, so what the top layer showed
    // outside it is gone (as before 0.0.8); inside it, nothing changed.
    let after = shown(&arena);
    for row in 0..2 {
        let inside = (row * 6 + 2) * 4..(row * 6 + 4) * 4;
        assert_eq!(after[inside.clone()], before[inside]);
    }
    aligned(&arena, first);
    // A vector layer moved and masked, rasterized.
    let (spawn, ink) = add_vector_layer(&arena, picture, Some(first), "Ink".to_owned()).unwrap();
    history
        .apply(&mut arena, Edit::new("Add").then(spawn))
        .unwrap();
    fill(&mut arena, ink, [0, 0, 0, 255]);
    history
        .apply(&mut arena, set(ink, picture::Y, 1_i64))
        .unwrap();
    let (edit, _) = new_mask(&arena, ink, |x, _| if x % 2 == 0 { 1.0 } else { 0.0 }).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let before = shown(&arena);
    let (edit, flat) = rasterize(&arena, picture, ink).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    assert_eq!(shown(&arena), before);
    aligned(&arena, flat);
}

#[test]
fn a_3d_layer_lands_in_the_picked_folder() {
    let (mut arena, picture, mut history, first) = picture_of(4, 2);
    let (edit, folder) = new_folder(&arena, picture, Some(first), true);
    history.apply(&mut arena, edit).unwrap();
    let model =
        sue_core::Entity::empty("Cube", sue_model::class::MODEL).with_ext(crate::three::shape(0));
    let (edit, layer) =
        crate::three::new_layer(&arena, picture, Some(folder), "Cube", model).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    assert_eq!(folder_of(&arena, layer), Some(folder));
    assert_eq!(&ids(&arena, picture)[1..], &[first, layer, folder]);
}
