//! Layer operations seen through the compositor: what each one redraws
//! of the screen image, that the image stays exactly what a composite
//! from nothing gives, and how long each takes on a picture of many
//! layers (the benchmark, `--ignored`).

use std::time::Instant;

use sue_core::txn::{Rename, SetProperty};
use sue_core::{Arena, Color, EntityId, ReversibleTransaction};
use sue_model::picture;

use super::Compositor;
use crate::doc::{self, NewPicture};

/// A picture of `size` with a paper and `count` raster layers, every
/// tenth in a folder with the nine before it. Each layer has a block of
/// paint a fifth of the picture a side, at its own place; the first one
/// is painted all over.
fn layered(size: [u32; 2], count: usize) -> (Arena, EntityId, Vec<EntityId>) {
    let mut arena = Arena::new();
    let (mut edit, picture) = doc::create(&NewPicture {
        name: "Layers".to_owned(),
        width: size[0],
        height: size[1],
        dpi: 350.0,
        paper: Some(Color::srgb8(255, 255, 255)),
        timelapse: false,
    })
    .unwrap();
    edit.apply(&mut arena).unwrap();
    let mut layers = vec![
        doc::layers(&arena, picture)
            .last()
            .map(|info| info.id)
            .unwrap(),
    ];
    for index in 1..count {
        let (mut spawn, layer) = doc::add_raster_layer(
            &arena,
            picture,
            layers.last().copied(),
            format!("Layer {}", index + 1),
            None,
        )
        .unwrap();
        spawn.apply(&mut arena).unwrap();
        layers.push(layer);
    }
    let [width, height] = size;
    for (index, layer) in layers.iter().enumerate() {
        let block = [width / 5, height / 5];
        let at = [
            (index as u32 * 37 % 4) * width / 5,
            (index as u32 * 53 % 4) * height / 5,
        ];
        let whole = index == 0;
        let raster = doc::raster_mut(&mut arena, *layer).unwrap();
        let bytes = raster.bytes_mut().unwrap();
        for y in 0..height {
            for x in 0..width {
                let inside =
                    x >= at[0] && x < at[0] + block[0] && y >= at[1] && y < at[1] + block[1];
                if whole || inside {
                    let pixel = (y as usize * width as usize + x as usize) * 4;
                    bytes[pixel..pixel + 4].copy_from_slice(&[
                        (index * 40 % 256) as u8,
                        (x % 256) as u8,
                        (y % 256) as u8,
                        if whole { 90 } else { 200 },
                    ]);
                }
            }
        }
    }
    // Folders: each takes the ten layers under it.
    for chunk in layers.chunks(10).skip(1) {
        let (mut edit, folder) = doc::new_folder(&arena, picture, chunk.last().copied(), false);
        edit.apply(&mut arena).unwrap();
        for layer in chunk {
            if let Some(mut edit) = crate::anim::drop_layer(&arena, picture, *layer, folder) {
                edit.apply(&mut arena).unwrap();
            }
        }
    }
    (arena, picture, layers)
}

/// The screen image composited from nothing.
fn fresh(arena: &Arena, picture: EntityId) -> Vec<u8> {
    let mut compositor = Compositor::new(picture);
    compositor.sync(arena);
    compositor.display().0.to_vec()
}

/// How many pixels the compositor redrew since `version`.
fn redrawn(compositor: &Compositor, version: u64) -> u64 {
    compositor
        .changed_since(version)
        .expect("the changes are on record")
        .iter()
        .map(|[x0, y0, x1, y1]| u64::from(x1 - x0) * u64::from(y1 - y0))
        .sum()
}

/// The first folder of `picture`.
fn a_folder(arena: &Arena, picture: EntityId) -> EntityId {
    doc::layers(arena, picture)
        .into_iter()
        .map(|info| info.id)
        .find(|id| doc::is_folder(arena, *id))
        .expect("a folder")
}

/// Makes a layer operation's edit for the arena as it is.
type MakeEdit = Box<dyn Fn(&Arena) -> doc::Edit>;

/// Each layer operation of the Layer palette, as (its name, the edit).
fn operations(
    arena: &Arena,
    picture: EntityId,
    layers: &[EntityId],
) -> Vec<(&'static str, MakeEdit)> {
    let small = layers[7];
    let folder = a_folder(arena, picture);
    let step = |edit: SetProperty| doc::Edit::new("step").then(edit);
    vec![
        (
            "rename",
            Box::new(move |_| doc::Edit::new("rename").then(Rename::new(small, "Ink"))),
        ),
        (
            "close a folder",
            Box::new(move |_| step(SetProperty::new(folder, doc::OPEN, false))),
        ),
        (
            "lock",
            Box::new(move |_| step(SetProperty::new(small, picture::LOCKED, true))),
        ),
        (
            "opacity",
            Box::new(move |_| step(SetProperty::new(small, picture::OPACITY, 0.4))),
        ),
        (
            "blend mode",
            Box::new(move |_| step(SetProperty::new(small, picture::BLEND, "multiply"))),
        ),
        (
            "hide",
            Box::new(move |_| step(SetProperty::new(small, picture::VISIBLE, false))),
        ),
        (
            "show",
            Box::new(move |_| step(SetProperty::new(small, picture::VISIBLE, true))),
        ),
        (
            "move in the picture",
            Box::new(move |_| {
                doc::Edit::new("move")
                    .then(SetProperty::new(small, picture::X, 23_i64))
                    .then(SetProperty::new(small, picture::Y, -11_i64))
            }),
        ),
        (
            "hide a folder",
            Box::new(move |_| step(SetProperty::new(folder, picture::VISIBLE, false))),
        ),
        (
            "folder opacity",
            Box::new(move |_| {
                doc::Edit::new("folder")
                    .then(SetProperty::new(folder, picture::VISIBLE, true))
                    .then(SetProperty::new(folder, picture::OPACITY, 0.5))
            }),
        ),
        (
            "new layer",
            Box::new(move |arena| {
                let (spawn, _) =
                    doc::add_raster_layer(arena, picture, Some(small), "New".to_owned(), None)
                        .unwrap();
                doc::Edit::new("new").then(spawn)
            }),
        ),
        (
            "reorder",
            Box::new(move |arena| doc::reorder(arena, picture, small, 2).expect("it moves")),
        ),
        (
            "clip",
            Box::new(move |_| step(SetProperty::new(small, picture::CLIP, true))),
        ),
        (
            "delete",
            Box::new(move |_| doc::Edit::new("delete").then(doc::delete(small))),
        ),
        (
            "hide the layer painted all over",
            Box::new({
                let whole = layers[0];
                move |_| step(SetProperty::new(whole, picture::VISIBLE, false))
            }),
        ),
    ]
}

/// After every layer operation, and after undoing each, the screen
/// image a compositor kept in step is exactly the one composited from
/// nothing; and an operation that changes nothing on screen (a name, a
/// folder closed in the palette, a lock, a blank layer) redraws nothing.
#[test]
fn layer_operations_redraw_what_they_change_and_nothing_else() {
    let size = [300, 400];
    let (mut arena, picture, layers) = layered(size, 24);
    let mut compositor = Compositor::new(picture);
    compositor.sync(&arena);
    // One more look with nothing changed, as the frame after: from here
    // the compositor knows what each layer shows.
    compositor.sync(&arena);
    compositor.sync(&arena);
    let all = u64::from(size[0]) * u64::from(size[1]);
    let mut undo = Vec::new();
    for (name, make) in operations(&arena, picture, &layers) {
        let mut edit = make(&arena);
        edit.apply(&mut arena).unwrap();
        let version = compositor.display().2;
        compositor.sync(&arena);
        assert!(
            compositor.display().0 == fresh(&arena, picture).as_slice(),
            "{name}: the image is what a composite from nothing gives"
        );
        let pixels = redrawn(&compositor, version);
        match name {
            "rename" | "close a folder" | "lock" | "new layer" => {
                assert_eq!(pixels, 0, "{name} shows nothing new");
            }
            "hide the layer painted all over" => assert_eq!(pixels, all, "{name}"),
            "hide a folder" | "folder opacity" | "reorder" => {
                assert!(
                    pixels > 0 && pixels < all,
                    "{name} redraws a part: {pixels}"
                );
            }
            _ => {
                // One layer's block, a fifth of the picture a side, to
                // the 64 pixel tiles it lies in (and where it was, when
                // it moved).
                assert!(
                    pixels > 0 && pixels <= all / 5,
                    "{name} redraws the layer's block: {pixels} of {all}"
                );
            }
        }
        undo.push((name, edit));
    }
    while let Some((name, mut edit)) = undo.pop() {
        edit.revert(&mut arena).unwrap();
        compositor.sync(&arena);
        assert!(
            compositor.display().0 == fresh(&arena, picture).as_slice(),
            "undo of {name}: the image is what a composite from nothing gives"
        );
    }
}

/// An edit made in place (a stroke, shown through `refresh`) and then
/// taken back by putting the layer's old pixels back leaves no trace:
/// what the compositor knew before the stroke is not trusted after it.
#[test]
fn a_change_after_a_stroke_was_shown_redraws_what_the_stroke_touched() {
    let (mut arena, picture, layers) = layered([300, 400], 12);
    let mut compositor = Compositor::new(picture);
    compositor.sync(&arena);
    compositor.sync(&arena);
    compositor.sync(&arena);
    let top = *layers.last().unwrap();
    let before = doc::raster(&arena, top).unwrap().clone();
    // A stroke far from the layer's own block, shown at once.
    {
        let raster = doc::raster_mut(&mut arena, top).unwrap();
        let bytes = raster.bytes_mut_in([280, 380, 10, 10]).unwrap();
        for y in 380..390 {
            for x in 280..290 {
                let pixel = (y * 300 + x) * 4;
                bytes[pixel..pixel + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
    }
    compositor.refresh(&arena, [280, 380, 290, 390]);
    assert!(compositor.display().0 == fresh(&arena, picture).as_slice());
    // The old pixels come back whole, with the revisions they had.
    *doc::raster_mut(&mut arena, top).unwrap() = before;
    compositor.sync(&arena);
    assert!(
        compositor.display().0 == fresh(&arena, picture).as_slice(),
        "the stroke is gone from the screen"
    );
}

/// Applies `edit`, lets `compositor` follow it, and prints how long
/// that took and how much of the picture was redrawn.
fn timed(name: &str, compositor: &mut Compositor, arena: &mut Arena, mut edit: doc::Edit) {
    edit.apply(arena).unwrap();
    let version = compositor.display().2;
    let (_, size, _) = compositor.display();
    let all = u64::from(size[0]) * u64::from(size[1]);
    let start = Instant::now();
    compositor.sync(arena);
    let took = start.elapsed().as_secs_f64() * 1000.0;
    let pixels = compositor.changed_since(version).map_or(all, |rects| {
        rects
            .iter()
            .map(|[x0, y0, x1, y1]| u64::from(x1 - x0) * u64::from(y1 - y0))
            .sum()
    });
    println!(
        "  {name:36} {took:8.1} ms, {:5.1} % redrawn",
        pixels as f64 * 100.0 / all as f64
    );
    // The frames after, with nothing changed: what the compositor knows
    // of each layer is brought up to date.
    compositor.sync(arena);
    compositor.sync(arena);
}

/// A slider dragged over `steps` values of the opacity of `layer`: the
/// time a step takes, mean and worst.
fn dragged(
    name: &str,
    compositor: &mut Compositor,
    arena: &mut Arena,
    layer: EntityId,
    steps: u32,
) {
    let (mut total, mut worst) = (0.0_f64, 0.0_f64);
    for step in 0..steps {
        let mut edit = doc::Edit::new("opacity").then(SetProperty::new(
            layer,
            picture::OPACITY,
            1.0 - f64::from(step) / f64::from(steps + 10),
        ));
        edit.apply(arena).unwrap();
        let start = Instant::now();
        compositor.sync(arena);
        let took = start.elapsed().as_secs_f64() * 1000.0;
        total += took;
        worst = worst.max(took);
    }
    println!(
        "  {name:36} {:8.1} ms a step, worst {worst:.1} ms",
        total / f64::from(steps)
    );
}

/// How long the screen image takes to follow each layer operation on a
/// picture of 60 layers in folders at 3000 by 4000, on one of 200
/// layers, and on a comic page at 600 dots an inch; and how long each
/// frame of an animation of 24 cels takes to show. Run with
/// `cargo test -p sue-picture --release layer_operations_speed -- --ignored --nocapture`
/// (`SUE_BENCH` picks cases, see [`wanted`]).
#[test]
#[ignore = "a benchmark"]
fn layer_operations_speed() {
    for (size, count) in [
        ([3000, 4000], 60),
        ([3000, 4000], 200),
        ([4299, 6070], 12),
        ([4299, 6070], 60),
    ] {
        if !wanted(&format!("{}x{}x{count}", size[0], size[1])) {
            continue;
        }
        let (mut arena, picture, layers) = layered(size, count);
        let mut compositor = Compositor::new(picture);
        let start = Instant::now();
        compositor.sync(&arena);
        println!(
            "{} x {}, {count} layers: first composite {:.1} ms",
            size[0],
            size[1],
            start.elapsed().as_secs_f64() * 1000.0
        );
        compositor.sync(&arena);
        compositor.sync(&arena);
        for (name, make) in operations(&arena, picture, &layers) {
            let edit = make(&arena);
            timed(name, &mut compositor, &mut arena, edit);
        }
        // The cases of the 0.0.82 work, on the picture as the list above
        // left it (layer 8 deleted, the layer painted all over hidden).
        let whole = layers[0];
        let show = |layer: EntityId, on: bool| {
            doc::Edit::new("show").then(SetProperty::new(layer, picture::VISIBLE, on))
        };
        let edit = show(whole, true);
        timed(
            "show the layer painted all over",
            &mut compositor,
            &mut arena,
            edit,
        );
        let folders: Vec<EntityId> = doc::layers(&arena, picture)
            .into_iter()
            .map(|info| info.id)
            .filter(|id| doc::is_folder(&arena, *id))
            .collect();
        let folder = folders[0];
        // Within a folder: a member three rows down.
        let member = layers[15.min(count - 1)];
        let at = doc::layers(&arena, picture)
            .iter()
            .position(|info| info.id == member)
            .unwrap();
        let edit = doc::reorder(&arena, picture, member, at - 3).expect("it moves");
        timed("reorder within a folder", &mut compositor, &mut arena, edit);
        // Across folders: a member of one folder dropped onto another.
        if let Some(other) = folders.get(1)
            && let Some(edit) = crate::anim::drop_layer(&arena, picture, member, *other)
        {
            timed(
                "reorder into another folder",
                &mut compositor,
                &mut arena,
                edit,
            );
        }
        let rows = doc::layers(&arena, picture).len();
        let edit = doc::reorder(&arena, picture, layers[9], rows - 1).expect("it moves");
        timed(
            "reorder from the bottom to the top",
            &mut compositor,
            &mut arena,
            edit,
        );
        let edit = show(folder, false);
        timed("hide a folder", &mut compositor, &mut arena, edit);
        let edit = show(folder, true);
        timed("show a folder", &mut compositor, &mut arena, edit);
        dragged(
            "folder opacity drag, 30 steps",
            &mut compositor,
            &mut arena,
            folder,
            30,
        );
        dragged(
            "layer opacity drag, 30 steps",
            &mut compositor,
            &mut arena,
            layers[5],
            30,
        );
        dragged(
            "opacity drag, layer painted all over",
            &mut compositor,
            &mut arena,
            whole,
            30,
        );
        let blend = |mode: &'static str| {
            doc::Edit::new("blend").then(SetProperty::new(whole, picture::BLEND, mode))
        };
        let edit = blend("multiply");
        timed(
            "blend mode, layer painted all over",
            &mut compositor,
            &mut arena,
            edit,
        );
        let edit = blend("normal");
        timed(
            "blend mode back, the same layer",
            &mut compositor,
            &mut arena,
            edit,
        );
        let edit = doc::reorder(&arena, picture, whole, 5).expect("it moves");
        timed(
            "reorder the layer painted all over",
            &mut compositor,
            &mut arena,
            edit,
        );
    }
    for size in [[1920, 1080], [3000, 4000], [4299, 6070]] {
        if wanted(&format!("play{}x{}", size[0], size[1])) {
            playback_speed(size);
        }
    }
}

/// Whether the benchmark runs the case `name`: all of them, or the ones
/// `SUE_BENCH` lists (as `3000x4000x60,play1920x1080`).
fn wanted(name: &str) -> bool {
    std::env::var("SUE_BENCH").map_or(true, |list| list.split(',').any(|item| item == name))
}

/// An animation of 24 cels over a background painted all over, each cel
/// a drawing over the middle half of the picture: the time a frame takes
/// to show, the first time round and the second.
fn playback_speed(size: [u32; 2]) {
    let mut arena = Arena::new();
    let (mut edit, picture) = doc::create(&NewPicture {
        name: "Cels".to_owned(),
        width: size[0],
        height: size[1],
        dpi: 350.0,
        paper: Some(Color::srgb8(255, 255, 255)),
        timelapse: false,
    })
    .unwrap();
    edit.apply(&mut arena).unwrap();
    let mut cels = vec![doc::layers(&arena, picture).last().unwrap().id];
    for index in 0..24 {
        let (mut spawn, layer) = doc::add_raster_layer(
            &arena,
            picture,
            cels.last().copied(),
            format!("Cel {index}"),
            None,
        )
        .unwrap();
        spawn.apply(&mut arena).unwrap();
        cels.push(layer);
    }
    let [width, height] = size;
    for (index, layer) in cels.iter().enumerate() {
        let raster = doc::raster_mut(&mut arena, *layer).unwrap();
        let bytes = raster.bytes_mut().unwrap();
        // The background all over; a cel's lines over the middle half,
        // every third row, a little further right each frame.
        let (xs, ys) = if index == 0 {
            (0..width, 0..height)
        } else {
            let shift = index as u32 * 7;
            (
                width / 4 + shift..width * 3 / 4 + shift,
                height / 4..height * 3 / 4,
            )
        };
        for y in ys {
            if index > 0 && y % 3 != 0 {
                continue;
            }
            for x in xs.clone() {
                let pixel = (y as usize * width as usize + x as usize) * 4;
                bytes[pixel..pixel + 4].copy_from_slice(&[
                    (index * 10) as u8,
                    (x % 256) as u8,
                    (y % 256) as u8,
                    if index == 0 { 255 } else { 220 },
                ]);
            }
        }
    }
    let frames = &cels[1..];
    let view = |shown: usize| super::FrameView {
        hidden: frames
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != shown)
            .map(|(_, id)| *id)
            .collect(),
        ..super::FrameView::default()
    };
    let mut compositor = Compositor::new(picture);
    compositor.set_view(Some(view(0)));
    compositor.sync(&arena);
    compositor.sync(&arena);
    compositor.sync(&arena);
    println!("{} x {}, playback of 24 cels:", size[0], size[1]);
    for round in ["first time round", "second time round"] {
        let (mut total, mut worst) = (0.0_f64, 0.0_f64);
        for frame in 1..=24 {
            compositor.set_view(Some(view(frame % 24)));
            let start = Instant::now();
            compositor.sync(&arena);
            let took = start.elapsed().as_secs_f64() * 1000.0;
            total += took;
            worst = worst.max(took);
        }
        println!(
            "  {round:36} {:8.1} ms a frame, worst {worst:.1} ms",
            total / 24.0
        );
    }
}
