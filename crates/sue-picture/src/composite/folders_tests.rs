//! Folders in the compositor: Through and every other mode, folder masks,
//! nested folders, corrections inside a folder, mask formats and links,
//! and layer color passed down, each against pixels worked out by hand.

use super::gather::{Layer, gather};
use super::rows::{RowSource, pixel, pixel_plain};
use super::{RenderOptions, render};
use crate::doc::{self, NewPicture};
use sue_core::txn::ReplaceComponent;
use sue_core::{Arena, Color, EntityId, PixelFormat, RasterTile2D, UndoStack};
use sue_model::picture;

/// A picture `width` by 1 with no paper and no layer, and an undo stack.
fn blank(width: u32, paper: Option<Color>) -> (Arena, EntityId, UndoStack<Arena>) {
    let mut arena = Arena::new();
    let mut history = UndoStack::new();
    let (edit, picture) = doc::create(&NewPicture {
        width,
        height: 1,
        paper,
        ..NewPicture::default()
    })
    .unwrap();
    history.apply(&mut arena, edit).unwrap();
    // The one layer a new picture has goes: each test makes its own.
    let first = doc::layers(&arena, picture)
        .into_iter()
        .find(|info| info.kind == picture::LayerKind::Raster)
        .unwrap()
        .id;
    history
        .apply(
            &mut arena,
            doc::Edit::new("Delete").then(doc::delete(first)),
        )
        .unwrap();
    (arena, picture, history)
}

/// A raster layer on top holding `pixels` (straight RGBA, one a column).
fn layer(
    arena: &mut Arena,
    history: &mut UndoStack<Arena>,
    picture: EntityId,
    pixels: &[[u8; 4]],
) -> EntityId {
    let raster =
        RasterTile2D::from_bytes(pixels.len() as u32, 1, PixelFormat::Rgba8, pixels.concat())
            .unwrap();
    let top = doc::layers(arena, picture).last().map(|info| info.id);
    let (spawn, id) =
        doc::add_layer_pixels(arena, picture, top, format!("L{}", pixels.len()), raster).unwrap();
    history
        .apply(arena, doc::Edit::new("Add").then(spawn))
        .unwrap();
    id
}

/// A folder right above `member`, holding it.
fn folder_around(
    arena: &mut Arena,
    history: &mut UndoStack<Arena>,
    picture: EntityId,
    member: EntityId,
) -> EntityId {
    let (edit, folder) = doc::new_folder(arena, picture, Some(member), true);
    history.apply(arena, edit).unwrap();
    folder
}

fn set(
    arena: &mut Arena,
    history: &mut UndoStack<Arena>,
    id: EntityId,
    key: &str,
    value: impl Into<sue_core::Value>,
) {
    history.apply(arena, doc::set(id, key, value)).unwrap();
}

fn shown(arena: &Arena, picture: EntityId, width: u32) -> Vec<[u8; 4]> {
    render(arena, picture, [0, 0, width, 1], RenderOptions::default())
        .as_chunks::<4>()
        .0
        .to_vec()
}

fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[test]
fn a_folder_is_through_until_it_takes_a_mode_of_its_own() {
    let (mut arena, picture, mut history) = blank(1, None);
    layer(&mut arena, &mut history, picture, &[[255, 0, 0, 255]]);
    let blue = layer(&mut arena, &mut history, picture, &[[0, 0, 255, 255]]);
    set(&mut arena, &mut history, blue, picture::BLEND, "multiply");
    let folder = folder_around(&mut arena, &mut history, picture, blue);
    set(&mut arena, &mut history, folder, picture::OPACITY, 0.5);
    assert_eq!(
        doc::folder_blend(&arena, folder),
        sue_model::BlendMode::Through
    );
    // Through: blue multiplies the red under the folder (black), at half:
    // half red.
    assert_eq!(shown(&arena, picture, 1), [[128, 0, 0, 255]]);
    // Normal: blue multiplies nothing inside the folder, so the folder is
    // plain blue, laid on at half: purple.
    set(&mut arena, &mut history, folder, picture::BLEND, "normal");
    assert_eq!(shown(&arena, picture, 1), [[128, 0, 128, 255]]);
    // The folder's own mode: Screen of blue over red is magenta.
    set(&mut arena, &mut history, folder, picture::OPACITY, 1.0);
    set(&mut arena, &mut history, folder, picture::BLEND, "screen");
    assert_eq!(shown(&arena, picture, 1), [[255, 0, 255, 255]]);
    // Hidden, nothing of it shows; undo brings each step back.
    set(&mut arena, &mut history, folder, picture::VISIBLE, false);
    assert_eq!(shown(&arena, picture, 1), [[255, 0, 0, 255]]);
    for _ in 0..4 {
        history.undo(&mut arena).unwrap();
    }
    assert_eq!(shown(&arena, picture, 1), [[128, 0, 0, 255]]);
}

#[test]
fn nested_folders_with_masks_and_modes_composite_as_worked_out() {
    let (mut arena, picture, mut history) = blank(2, None);
    layer(&mut arena, &mut history, picture, &[[255, 0, 0, 255]; 2]);
    let green = layer(&mut arena, &mut history, picture, &[[0, 255, 0, 255]; 2]);
    // The inner folder is Through with a mask showing half everywhere.
    let inner = folder_around(&mut arena, &mut history, picture, green);
    let (edit, _) = doc::new_mask(&arena, inner, |_, _| 0.5).unwrap();
    history.apply(&mut arena, edit).unwrap();
    // The outer folder is Screen with a mask showing only the left pixel.
    let outer = folder_around(&mut arena, &mut history, picture, inner);
    assert_eq!(doc::folders_around(&arena, green), vec![inner, outer]);
    set(&mut arena, &mut history, outer, picture::BLEND, "screen");
    let (edit, _) = doc::new_mask(&arena, outer, |x, _| if x == 0 { 1.0 } else { 0.0 }).unwrap();
    history.apply(&mut arena, edit).unwrap();
    // Inside the outer folder: green at half (the inner mask, rounded to
    // 128 / 255), over nothing. Laid on red by Screen: the mixed color
    // (1, 1, 0) at that alpha over red: (1, a, 0).
    let a = 128.0 / 255.0;
    assert_eq!(
        shown(&arena, picture, 2),
        [[255, byte(a), 0, 255], [255, 0, 0, 255]]
    );
    // The inner folder's mask off: green whole, so yellow on the left.
    set(&mut arena, &mut history, inner, doc::MASK_ON, false);
    assert_eq!(
        shown(&arena, picture, 2),
        [[255, 255, 0, 255], [255, 0, 0, 255]]
    );
    // The fast path reads the folders as the full one does.
    let layers = gather(&arena, picture, RenderOptions::default());
    let rows: Vec<RowSource<'_>> = layers.iter().map(|layer| layer.row(0)).collect();
    for x in 0..2 {
        if layers.iter().all(Layer::plain) {
            assert_eq!(pixel_plain(&layers, &rows, x, 0), pixel(&layers, x, 0));
        }
    }
}

#[test]
fn a_correction_in_a_folder_corrects_only_the_folder() {
    let gray = Some(Color::srgb8(200, 200, 200));
    let (mut arena, picture, mut history) = blank(2, gray);
    let ink = layer(
        &mut arena,
        &mut history,
        picture,
        &[[0, 0, 0, 255], [0, 0, 0, 0]],
    );
    let (edit, invert) = doc::new_correction(&arena, picture, Some(ink), "Invert", "invert");
    history.apply(&mut arena, edit).unwrap();
    let (edit, folder) = doc::new_folder(&arena, picture, Some(invert), true);
    history.apply(&mut arena, edit).unwrap();
    let into = doc::set_folder(&arena, ink, Some(folder)).unwrap();
    history.apply(&mut arena, into).unwrap();
    assert_eq!(doc::members(&arena, picture, folder), vec![ink, invert]);
    // Through: the correction turns the black ink white and the paper
    // under the clear pixel from 200 to 55.
    assert_eq!(
        shown(&arena, picture, 2),
        [[255, 255, 255, 255], [55, 55, 55, 255]]
    );
    // Normal: it sees only the folder's ink; the paper stays.
    set(&mut arena, &mut history, folder, picture::BLEND, "normal");
    assert_eq!(
        shown(&arena, picture, 2),
        [[255, 255, 255, 255], [200, 200, 200, 255]]
    );
}

#[test]
fn a_correction_takes_its_blend_mode_and_mask() {
    let (mut arena, picture, mut history) = blank(2, Some(Color::srgb8(200, 200, 200)));
    let (edit, invert) = doc::new_correction(&arena, picture, None, "Invert", "invert");
    history.apply(&mut arena, edit).unwrap();
    set(&mut arena, &mut history, invert, picture::BLEND, "multiply");
    let (edit, _) = doc::new_mask(&arena, invert, |x, _| if x == 0 { 1.0 } else { 0.5 }).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let back = 200.0 / 255.0;
    let multiplied = back * (55.0 / 255.0);
    let half = 128.0 / 255.0;
    let right = back + (multiplied - back) * half;
    assert_eq!(
        shown(&arena, picture, 2),
        [
            [byte(multiplied), byte(multiplied), byte(multiplied), 255],
            [byte(right), byte(right), byte(right), 255]
        ]
    );
}

#[test]
fn a_layer_clipped_to_a_folder_clips_to_all_of_it() {
    let (mut arena, picture, mut history) = blank(3, None);
    let left = layer(
        &mut arena,
        &mut history,
        picture,
        &[[0, 0, 0, 255], [0, 0, 0, 0], [0, 0, 0, 0]],
    );
    let folder = folder_around(&mut arena, &mut history, picture, left);
    set(&mut arena, &mut history, folder, picture::BLEND, "normal");
    let (spawn, middle) = doc::add_layer_pixels(
        &arena,
        picture,
        Some(left),
        "Middle".to_owned(),
        RasterTile2D::from_bytes(
            3,
            1,
            PixelFormat::Rgba8,
            [[0, 0, 0, 0], [0, 0, 0, 255], [0, 0, 0, 0]].concat(),
        )
        .unwrap(),
    )
    .unwrap();
    history
        .apply(&mut arena, doc::Edit::new("Add").then(spawn))
        .unwrap();
    if doc::folder_of(&arena, middle) != Some(folder) {
        let into = doc::set_folder(&arena, middle, Some(folder)).unwrap();
        history.apply(&mut arena, into).unwrap();
    }
    let red = layer(&mut arena, &mut history, picture, &[[255, 0, 0, 255]; 3]);
    set(&mut arena, &mut history, red, picture::CLIP, true);
    assert_eq!(
        shown(&arena, picture, 3),
        [[255, 0, 0, 255], [255, 0, 0, 255], [0, 0, 0, 0]]
    );
}

#[test]
fn gray_masks_show_as_the_alpha_of_color_masks() {
    let (mut arena, picture, mut history) = blank(4, None);
    let white = layer(
        &mut arena,
        &mut history,
        picture,
        &[[255, 255, 255, 255]; 4],
    );
    let (edit, mask) =
        doc::new_mask(&arena, white, |x, _| [1.0, 0.75, 0.25, 0.0][x as usize]).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let before = shown(&arena, picture, 4);
    assert_eq!(
        before.iter().map(|pixel| pixel[3]).collect::<Vec<_>>(),
        [255, 191, 64, 0]
    );
    // The same cover as a gray mask.
    let alpha: Vec<u8> = doc::raster(&arena, mask)
        .unwrap()
        .bytes()
        .unwrap()
        .iter()
        .skip(3)
        .step_by(4)
        .copied()
        .collect();
    let gray = RasterTile2D::from_bytes(4, 1, PixelFormat::Gray8, alpha).unwrap();
    history
        .apply(
            &mut arena,
            doc::Edit::new("Gray").then(ReplaceComponent::new(mask, gray, "Gray")),
        )
        .unwrap();
    assert_eq!(shown(&arena, picture, 4), before);
    assert!((doc::shown_at(&arena, white, 1, 0) - 191.0 / 255.0).abs() < 1e-6);
    // Applying it keeps the same picture.
    let edit = doc::apply_mask(&arena, white).unwrap();
    history.apply(&mut arena, edit).unwrap();
    assert_eq!(shown(&arena, picture, 4), before);
}

#[test]
fn a_linked_mask_moves_with_its_layer_and_an_unlinked_one_stays() {
    let (mut arena, picture, mut history) = blank(4, None);
    let white = layer(
        &mut arena,
        &mut history,
        picture,
        &[[255, 255, 255, 255]; 4],
    );
    let (edit, _) = doc::new_mask(&arena, white, |x, _| if x < 2 { 1.0 } else { 0.0 }).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let alpha = |arena: &Arena| -> Vec<u8> {
        shown(arena, picture, 4)
            .iter()
            .map(|pixel| pixel[3])
            .collect()
    };
    assert_eq!(alpha(&arena), [255, 255, 0, 0]);
    assert!(doc::mask_linked(&arena, white));
    // Linked: the mask moves with the layer.
    set(&mut arena, &mut history, white, picture::X, 1_i64);
    assert_eq!(alpha(&arena), [0, 255, 255, 0]);
    // Unlinked: the mask stays where it is now and the layer moves under it.
    let edit = doc::set_mask_linked(&arena, white, false).unwrap();
    history.apply(&mut arena, edit).unwrap();
    assert_eq!(alpha(&arena), [0, 255, 255, 0], "unlinking moves nothing");
    assert!(doc::set_mask_linked(&arena, white, false).is_none());
    set(&mut arena, &mut history, white, picture::X, 0_i64);
    assert_eq!(alpha(&arena), [0, 255, 255, 0]);
    // Linked again where it is: it goes with the layer from there.
    let edit = doc::set_mask_linked(&arena, white, true).unwrap();
    history.apply(&mut arena, edit).unwrap();
    assert_eq!(alpha(&arena), [0, 255, 255, 0]);
    set(&mut arena, &mut history, white, picture::X, 1_i64);
    assert_eq!(alpha(&arena), [0, 0, 255, 255]);
    // Undo, step by step, back to the mask as it was made.
    for _ in 0..4 {
        history.undo(&mut arena).unwrap();
    }
    assert_eq!(alpha(&arena), [0, 255, 255, 0]);
    history.undo(&mut arena).unwrap();
    assert_eq!(alpha(&arena), [255, 255, 0, 0]);
    assert!(doc::mask_linked(&arena, white));
}

#[test]
fn a_folders_layer_color_and_sub_color_reach_its_layers() {
    let (mut arena, picture, mut history) = blank(3, None);
    let ink = layer(
        &mut arena,
        &mut history,
        picture,
        &[[0, 0, 0, 255], [255, 255, 255, 255], [128, 128, 128, 255]],
    );
    let folder = folder_around(&mut arena, &mut history, picture, ink);
    set(
        &mut arena,
        &mut history,
        folder,
        crate::effects::COLOR,
        true,
    );
    set(
        &mut arena,
        &mut history,
        folder,
        crate::effects::COLOR_VALUE,
        Color::srgb8(0, 0, 255),
    );
    // Without a sub color everything is the layer color.
    assert_eq!(shown(&arena, picture, 3), [[0, 0, 255, 255]; 3]);
    // With one, the light parts take it and the dark the layer color.
    set(
        &mut arena,
        &mut history,
        folder,
        crate::effects::COLOR_SUB,
        Color::srgb8(255, 255, 0),
    );
    let mid = 128.0 / 255.0;
    assert_eq!(
        shown(&arena, picture, 3),
        [
            [0, 0, 255, 255],
            [255, 255, 0, 255],
            [byte(mid), byte(mid), byte(1.0 - mid), 255]
        ]
    );
    // A layer's own layer color wins over its folder's.
    set(&mut arena, &mut history, ink, crate::effects::COLOR, true);
    set(
        &mut arena,
        &mut history,
        ink,
        crate::effects::COLOR_VALUE,
        Color::srgb8(255, 0, 0),
    );
    assert_eq!(shown(&arena, picture, 3), [[255, 0, 0, 255]; 3]);
}

#[test]
fn every_kind_of_layer_takes_a_mask() {
    let (mut arena, picture, mut history) = blank(2, Some(Color::srgb8(10, 20, 30)));
    let paper = doc::layers(&arena, picture)[0].id;
    let (edit, mask) = doc::new_mask(&arena, paper, |x, _| if x == 0 { 1.0 } else { 0.0 }).unwrap();
    history.apply(&mut arena, edit).unwrap();
    assert_eq!(doc::mask_of(&arena, paper), Some(mask));
    let size = doc::raster(&arena, mask).map(|tile| [tile.width(), tile.height()]);
    assert_eq!(size, Some([2, 1]), "the picture's size");
    assert_eq!(shown(&arena, picture, 2), [[10, 20, 30, 255], [0, 0, 0, 0]]);
    history.undo(&mut arena).unwrap();
    assert_eq!(doc::mask_of(&arena, paper), None);
    assert_eq!(shown(&arena, picture, 2), [[10, 20, 30, 255]; 2]);
}
