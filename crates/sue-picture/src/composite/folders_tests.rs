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

/// A raster layer on top of the picture, in no folder, holding `pixels`
/// (straight RGBA, one a column).
fn layer(
    arena: &mut Arena,
    history: &mut UndoStack<Arena>,
    picture: EntityId,
    pixels: &[[u8; 4]],
) -> EntityId {
    let raster =
        RasterTile2D::from_bytes(pixels.len() as u32, 1, PixelFormat::Rgba8, pixels.concat())
            .unwrap();
    let (spawn, id) =
        doc::add_layer_pixels(arena, picture, None, format!("L{}", pixels.len()), raster).unwrap();
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
    // The same cover as a color mask (0.0.75's), its alpha the cover.
    let tile = doc::raster(&arena, mask).unwrap();
    assert_eq!(tile.format(), PixelFormat::Gray8, "a new mask is gray");
    let color: Vec<u8> = tile
        .bytes()
        .unwrap()
        .iter()
        .flat_map(|&value| [255, 255, 255, value])
        .collect();
    let color = RasterTile2D::from_bytes(4, 1, PixelFormat::Rgba8, color).unwrap();
    history
        .apply(
            &mut arena,
            doc::Edit::new("Color").then(ReplaceComponent::new(mask, color, "Color")),
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

/// Writes a picture of what folders, masks, modes, live layers and tones
/// do, for a person to look at: `SUE_SHOTS=<folder> cargo test -p
/// sue-picture folder_sheet -- --ignored`.
#[test]
#[ignore = "writes a picture for a person to look at"]
fn folder_sheet() {
    const SIDE: u32 = 120;
    let add = |arena: &mut Arena,
               history: &mut UndoStack<Arena>,
               picture: EntityId,
               paint: &dyn Fn(u32, u32) -> [u8; 4]| {
        let mut pixels = Vec::new();
        for y in 0..SIDE {
            for x in 0..SIDE {
                pixels.extend_from_slice(&paint(x, y));
            }
        }
        let raster = RasterTile2D::from_bytes(SIDE, SIDE, PixelFormat::Rgba8, pixels).unwrap();
        let (spawn, id) =
            doc::add_layer_pixels(arena, picture, None, "Layer".to_owned(), raster).unwrap();
        history
            .apply(arena, doc::Edit::new("Add").then(spawn))
            .unwrap();
        id
    };
    // Stripes of red, green and blue to see modes against.
    let stripes = |x: u32, _: u32| match (x / 20) % 3 {
        0 => [220, 60, 60, 255],
        1 => [60, 200, 90, 255],
        _ => [70, 90, 220, 255],
    };
    let disc = |rgba: [u8; 4]| {
        move |x: u32, y: u32| {
            let (dx, dy) = (x as f32 - 60.0, y as f32 - 60.0);
            if dx.hypot(dy) < 40.0 { rgba } else { [0; 4] }
        }
    };
    let fresh = || {
        let mut arena = Arena::new();
        let mut history = UndoStack::new();
        let (edit, picture) = doc::create(&NewPicture {
            width: SIDE,
            height: SIDE,
            paper: Some(Color::srgb8(255, 255, 255)),
            ..NewPicture::default()
        })
        .unwrap();
        history.apply(&mut arena, edit).unwrap();
        (arena, history, picture)
    };
    let all = [0, 0, SIDE, SIDE];
    let mut panels: Vec<Vec<u8>> = Vec::new();
    // 1, 2: a gray disc set to Multiply in a folder at 70%: Through (it
    // multiplies the stripes), then Normal (it is plain gray at 70%).
    for mode in ["through", "normal"] {
        let (mut arena, mut history, picture) = fresh();
        add(&mut arena, &mut history, picture, &stripes);
        let gray = add(
            &mut arena,
            &mut history,
            picture,
            &disc([128, 128, 128, 255]),
        );
        set(&mut arena, &mut history, gray, picture::BLEND, "multiply");
        let folder = folder_around(&mut arena, &mut history, picture, gray);
        set(&mut arena, &mut history, folder, picture::OPACITY, 0.7);
        set(&mut arena, &mut history, folder, picture::BLEND, mode);
        panels.push(render(&arena, picture, all, RenderOptions::default()));
    }
    // 3: a yellow disc in a Screen folder whose mask shows the left half,
    // inside a Through folder whose mask fades from top to bottom.
    {
        let (mut arena, mut history, picture) = fresh();
        add(&mut arena, &mut history, picture, &stripes);
        let yellow = add(
            &mut arena,
            &mut history,
            picture,
            &disc([230, 210, 40, 255]),
        );
        let inner = folder_around(&mut arena, &mut history, picture, yellow);
        set(&mut arena, &mut history, inner, picture::BLEND, "screen");
        let (edit, _) =
            doc::new_mask(&arena, inner, |x, _| if x < 60 { 1.0 } else { 0.0 }).unwrap();
        history.apply(&mut arena, edit).unwrap();
        let outer = folder_around(&mut arena, &mut history, picture, inner);
        let (edit, _) = doc::new_mask(&arena, outer, |_, y| 1.0 - y as f32 / SIDE as f32).unwrap();
        history.apply(&mut arena, edit).unwrap();
        panels.push(render(&arena, picture, all, RenderOptions::default()));
    }
    // 4: an Invert correction in a Normal folder with a disc: only the
    // disc is inverted, not the stripes.
    {
        let (mut arena, mut history, picture) = fresh();
        add(&mut arena, &mut history, picture, &stripes);
        let dark = add(&mut arena, &mut history, picture, &disc([40, 40, 120, 255]));
        let (edit, invert) = doc::new_correction(&arena, picture, Some(dark), "Invert", "invert");
        history.apply(&mut arena, edit).unwrap();
        let folder = folder_around(&mut arena, &mut history, picture, invert);
        let into = doc::set_folder(&arena, dark, Some(folder)).unwrap();
        history.apply(&mut arena, into).unwrap();
        set(&mut arena, &mut history, folder, picture::BLEND, "normal");
        panels.push(render(&arena, picture, all, RenderOptions::default()));
    }
    // 5: a tiled, turned image material and a round gradient in Glow
    // Dodge over the stripes.
    {
        let (mut arena, mut history, picture) = fresh();
        add(&mut arena, &mut history, picture, &stripes);
        let image = RasterTile2D::from_bytes(
            2,
            2,
            PixelFormat::Rgba8,
            [[0, 0, 0, 200], [0; 4], [0; 4], [0, 0, 0, 200]].concat(),
        )
        .unwrap();
        let place = doc::Placement {
            at: [60.0, 60.0],
            scale: 8.0,
            angle: 30.0,
            tile: true,
        };
        let (edit, _) = doc::new_material_layer(&arena, picture, None, "Checks", image, place);
        history.apply(&mut arena, edit).unwrap();
        let gradient = doc::Gradient::two(
            [[60.0, 60.0], [60.0, 110.0]],
            [[1.0, 0.8, 0.4, 0.9], [1.0, 0.8, 0.4, 0.0]],
            true,
        );
        let (edit, glow) = doc::new_gradient_layer(&arena, picture, None, "Glow", gradient);
        history.apply(&mut arena, edit).unwrap();
        set(&mut arena, &mut history, glow, picture::BLEND, "glow-dodge");
        panels.push(render(&arena, picture, all, RenderOptions::default()));
    }
    // 6: a gray ramp as tone in the six dot shapes, a band each.
    {
        let (mut arena, mut history, picture) = fresh();
        for shape in 0..6_u32 {
            let ramp = move |x: u32, y: u32| {
                if y / 20 == shape {
                    let gray = (x * 255 / SIDE) as u8;
                    [gray, gray, gray, 255]
                } else {
                    [0; 4]
                }
            };
            let id = add(&mut arena, &mut history, picture, &ramp);
            set(&mut arena, &mut history, id, crate::effects::TONE, true);
            set(
                &mut arena,
                &mut history,
                id,
                crate::effects::TONE_LINES,
                12.0,
            );
            set(
                &mut arena,
                &mut history,
                id,
                crate::effects::TONE_SHAPE,
                i64::from(shape),
            );
        }
        panels.push(render(&arena, picture, all, RenderOptions::default()));
    }
    let gap = 8;
    let width = panels.len() as u32 * (SIDE + gap) + gap;
    let height = SIDE + 2 * gap;
    let mut sheet = vec![236_u8; (width * height * 4) as usize];
    for (index, panel) in panels.iter().enumerate() {
        let left = gap + index as u32 * (SIDE + gap);
        for y in 0..SIDE {
            let from = (y * SIDE * 4) as usize;
            let to = (((y + gap) * width + left) * 4) as usize;
            sheet[to..to + (SIDE * 4) as usize]
                .copy_from_slice(&panel[from..from + (SIDE * 4) as usize]);
        }
    }
    let png = sue_io::encode_png_rgba(width, height, &sheet).unwrap();
    let folder = std::env::var("SUE_SHOTS").expect("SUE_SHOTS names the folder to write in");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(std::path::Path::new(&folder).join("folders.png"), png).unwrap();
}
