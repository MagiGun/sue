//! The screen image by tiles against a composite of every layer at
//! every pixel: after a few hundred layer operations in a seeded random
//! order with undo and redo mixed in, through frames of a slider drag
//! that each take a small time, and through an animation's frames.

use std::time::{Duration, Instant};

use sue_core::txn::{Rename, SetProperty};
use sue_core::{Arena, Color, EntityId, ReversibleTransaction};
use sue_model::{BlendMode, picture};

use super::gather::{gather_view, prepare};
use super::rows::{pixel, shown};
use super::{Compositor, FrameView, RenderOptions};
use crate::doc::{self, NewPicture};

/// Numbers that look random and are the same every run.
struct Dice(u64);

impl Dice {
    fn roll(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number below `count`.
    fn below(&mut self, count: usize) -> usize {
        (self.roll() % count.max(1) as u64) as usize
    }
}

/// The screen image composited the slow way: every layer gone through at
/// every pixel, nothing passed over, nothing kept.
fn reference(arena: &Arena, picture: EntityId, view: Option<&FrameView>) -> Vec<u8> {
    let [width, height] = picture::size(arena.get_by_id(picture).unwrap()).unwrap();
    let mut layers = gather_view(arena, picture, RenderOptions::default(), view);
    prepare(&mut layers, [0, 0, width, height]);
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            out.extend_from_slice(&shown(pixel(&layers, x, y), x, y));
        }
    }
    out
}

/// Paints a block of `layer` in place and answers the block in layer
/// pixels `[x0, y0, x1, y1]`.
fn block(arena: &mut Arena, layer: EntityId, dice: &mut Dice) -> Option<[u32; 4]> {
    let raster = doc::raster_mut(arena, layer)?;
    let (width, height) = (raster.width(), raster.height());
    let x0 = dice.below(width as usize) as u32;
    let y0 = dice.below(height as usize) as u32;
    let x1 = (x0 + 1 + dice.below(90) as u32).min(width);
    let y1 = (y0 + 1 + dice.below(90) as u32).min(height);
    let rgba = [
        dice.below(256) as u8,
        dice.below(256) as u8,
        dice.below(256) as u8,
        // Now and then an eraser: the block goes clear.
        if dice.below(5) == 0 {
            0
        } else {
            40 + dice.below(216) as u8
        },
    ];
    let bytes = raster.bytes_mut_in([x0, y0, x1 - x0, y1 - y0])?;
    for y in y0..y1 {
        for x in x0..x1 {
            let at = (y * width + x) as usize * 4;
            bytes[at..at + 4].copy_from_slice(&rgba);
        }
    }
    Some([x0, y0, x1, y1])
}

/// A picture of `size` with a paper, raster layers with paint here and
/// there, a mask, clipped layers, a correction, a Through folder and a
/// folder composited on its own with a clipped pair inside.
fn busy_picture(size: [u32; 2], dice: &mut Dice) -> (Arena, EntityId) {
    let mut arena = Arena::new();
    let (mut edit, picture) = doc::create(&NewPicture {
        name: "Busy".to_owned(),
        width: size[0],
        height: size[1],
        dpi: 72.0,
        paper: Some(Color::srgb8(250, 248, 240)),
        timelapse: false,
    })
    .unwrap();
    edit.apply(&mut arena).unwrap();
    let mut layers = vec![doc::layers(&arena, picture).last().unwrap().id];
    for index in 1..12 {
        let (mut spawn, layer) = doc::add_raster_layer(
            &arena,
            picture,
            layers.last().copied(),
            format!("Layer {index}"),
            None,
        )
        .unwrap();
        spawn.apply(&mut arena).unwrap();
        layers.push(layer);
    }
    for (index, layer) in layers.iter().enumerate() {
        // Two stay blank.
        if index % 6 != 5 {
            block(&mut arena, *layer, dice);
            block(&mut arena, *layer, dice);
        }
    }
    let set = |arena: &mut Arena, id: EntityId, key: &'static str, value: sue_core::Value| {
        SetProperty::new(id, key, value).apply(arena).unwrap();
    };
    set(&mut arena, layers[2], picture::CLIP, true.into());
    set(&mut arena, layers[3], picture::CLIP, true.into());
    set(&mut arena, layers[4], picture::OPACITY, 0.6.into());
    set(
        &mut arena,
        layers[6],
        picture::BLEND,
        BlendMode::Multiply.name().into(),
    );
    set(&mut arena, layers[9], picture::CLIP, true.into());
    let (mut mask, _) = doc::new_mask(&arena, layers[4], |x, y| {
        if (x / 9 + y / 7) % 2 == 0 { 1.0 } else { 0.35 }
    })
    .unwrap();
    mask.apply(&mut arena).unwrap();
    let (mut correction, _) = doc::new_correction(
        &arena,
        picture,
        Some(layers[5]),
        "Brightness",
        "brightness:20,10",
    );
    correction.apply(&mut arena).unwrap();
    // A Through folder of three, and one composited on its own of three
    // (a clipped pair among them).
    for (members, mode) in [
        (&layers[1..4], None),
        (&layers[8..11], Some(BlendMode::Screen)),
    ] {
        let (mut edit, folder) = doc::new_folder(&arena, picture, members.last().copied(), false);
        edit.apply(&mut arena).unwrap();
        for layer in members {
            if let Some(mut edit) = crate::anim::drop_layer(&arena, picture, *layer, folder) {
                edit.apply(&mut arena).unwrap();
            }
        }
        if let Some(mode) = mode {
            set(&mut arena, folder, picture::BLEND, mode.name().into());
        }
    }
    (arena, picture)
}

/// One random layer operation as an edit, or `None` when the dice asked
/// for one that cannot be done now.
fn random_edit(arena: &Arena, picture: EntityId, dice: &mut Dice) -> Option<doc::Edit> {
    let stack = doc::layers(arena, picture);
    let any = stack[dice.below(stack.len())].id;
    let other = stack[dice.below(stack.len())].id;
    let folder = doc::is_folder(arena, any);
    let entity = arena.get_by_id(any).ok()?;
    let style = picture::LayerStyle::of(entity);
    let set = |key: &'static str, value: sue_core::Value| {
        Some(doc::Edit::new("set").then(SetProperty::new(any, key, value)))
    };
    match dice.below(12) {
        0 => set(picture::OPACITY, (dice.below(11) as f64 / 10.0).into()),
        1 => set(picture::VISIBLE, (!style.visible).into()),
        2 => {
            let modes = [
                BlendMode::Normal,
                BlendMode::Multiply,
                BlendMode::Screen,
                BlendMode::Add,
                BlendMode::Darken,
                BlendMode::Through,
            ];
            // Through is a folder's own.
            let mode = modes[dice.below(if folder { 6 } else { 5 })];
            set(picture::BLEND, mode.name().into())
        }
        3 => set(picture::CLIP, (!style.clip).into()),
        4 => Some(
            doc::Edit::new("move")
                .then(SetProperty::new(
                    any,
                    picture::X,
                    dice.below(120) as i64 - 60,
                ))
                .then(SetProperty::new(
                    any,
                    picture::Y,
                    dice.below(120) as i64 - 60,
                )),
        ),
        5 | 6 => doc::reorder(arena, picture, any, dice.below(stack.len())),
        7 => crate::anim::drop_layer(arena, picture, any, other),
        8 => {
            let (spawn, _) =
                doc::add_raster_layer(arena, picture, Some(any), "New".to_owned(), None).ok()?;
            Some(doc::Edit::new("new").then(spawn))
        }
        9 => (!folder && stack.len() > 8).then(|| doc::Edit::new("delete").then(doc::delete(any))),
        10 => {
            doc::mask_of(arena, any)?;
            set(doc::MASK_ON, (!doc::mask_on(arena, any)).into())
        }
        _ => Some(doc::Edit::new("rename").then(Rename::new(any, "Named"))),
    }
}

/// After each of a few hundred layer operations in a seeded random
/// order (properties, reorders, drops into folders, new and deleted
/// layers, paint, animation frames, undo and redo), followed at once, a
/// little a frame, or only some steps later, the screen image is exactly
/// what compositing every layer at every pixel gives.
#[test]
fn random_layer_operations_keep_the_image_exact() {
    for seed in [0x5EED_0082_u64, 0x00C0_FFEE] {
        let mut dice = Dice(seed);
        let size = [200, 170];
        let (mut arena, picture) = busy_picture(size, &mut dice);
        let mut compositor = Compositor::new(picture);
        // Every change is shared out over frames, however few its tiles.
        compositor.small_change = 0;
        compositor.sync(&arena);
        let mut view: Option<FrameView> = None;
        let (mut done, mut undone): (Vec<doc::Edit>, Vec<doc::Edit>) = (Vec::new(), Vec::new());
        let mut kept = 0;
        for step in 0..320 {
            let what = dice.below(20);
            let name = match what {
                0..=10 => match random_edit(&arena, picture, &mut dice) {
                    Some(mut edit) => {
                        edit.apply(&mut arena).unwrap();
                        done.push(edit);
                        undone.clear();
                        "an edit"
                    }
                    None => "nothing",
                },
                11 | 12 => match done.pop() {
                    Some(mut edit) => {
                        edit.revert(&mut arena).unwrap();
                        undone.push(edit);
                        "undo"
                    }
                    None => "nothing",
                },
                13 => match undone.pop() {
                    Some(mut edit) => {
                        edit.apply(&mut arena).unwrap();
                        done.push(edit);
                        "redo"
                    }
                    None => "nothing",
                },
                14..=16 => {
                    // Paint in place, shown as a stroke's tiles are, as
                    // a tool's whole picture is, or left to the next look.
                    let rasters: Vec<(EntityId, [i64; 2])> = doc::layers(&arena, picture)
                        .into_iter()
                        .filter(|info| doc::raster(&arena, info.id).is_some())
                        .map(|info| (info.id, info.style.offset))
                        .collect();
                    let (layer, offset) = rasters[dice.below(rasters.len())];
                    // A stroke's refresh takes the picture as seen: it
                    // comes after the frame's look, as in the studio.
                    if what == 14 {
                        compositor.sync(&arena);
                    }
                    if let Some([x0, y0, x1, y1]) = block(&mut arena, layer, &mut dice) {
                        let at = |value: u32, by: i64| (i64::from(value) + by).max(0) as u32;
                        let rect = [
                            at(x0, offset[0]),
                            at(y0, offset[1]),
                            at(x1, offset[0]),
                            at(y1, offset[1]),
                        ];
                        match what {
                            14 => compositor.refresh_rects(&arena, &[rect], None),
                            15 => compositor.refresh(&arena, [0, 0, u32::MAX, u32::MAX]),
                            _ => {}
                        }
                    }
                    "paint"
                }
                17 | 18 => {
                    // Another animation frame: some layers hidden.
                    let stack = doc::layers(&arena, picture);
                    view = (dice.below(4) != 0).then(|| FrameView {
                        hidden: stack
                            .iter()
                            .filter(|_| dice.below(4) == 0)
                            .map(|info| info.id)
                            .collect(),
                        ..FrameView::default()
                    });
                    compositor.set_view(view.clone());
                    "a frame"
                }
                _ => "a look",
            };
            // Followed at once, a little a frame until all is shown, or
            // not at all this step.
            match dice.below(4) {
                0 => continue,
                1 => {
                    let visible = [0, 0, 64 + dice.below(100) as u32, 64];
                    let mut frames = 0;
                    let step = [1, 2, 4, 8][dice.below(4)];
                    compositor.sync_within(&arena, visible, step, Duration::ZERO);
                    while compositor.settling() {
                        compositor.sync_within(&arena, visible, step, Duration::ZERO);
                        frames += 1;
                        assert!(frames < 100, "the image settles");
                    }
                }
                _ => {
                    compositor.sync(&arena);
                }
            }
            assert!(!compositor.settling());
            assert!(
                compositor.display().0 == reference(&arena, picture, view.as_ref()).as_slice(),
                "seed {seed:#x}, step {step} ({name}): the image is what every layer at every pixel gives"
            );
            kept = kept.max(compositor.tiles.len());
        }
        assert!(kept > 0, "composited tiles are kept");
    }
}

/// The screen image's pixel at `at`, composited the slow way.
fn reference_at(arena: &Arena, picture: EntityId, [x, y]: [u32; 2]) -> [u8; 4] {
    let layers = gather_view(arena, picture, RenderOptions::default(), None);
    shown(pixel(&layers, x, y), x, y)
}

/// A picture of `size` with a paper, a layer painted all over and one
/// with a block, painted row by row (fast enough for a big one).
fn big_picture(size: [u32; 2]) -> (Arena, EntityId, [EntityId; 2]) {
    let mut arena = Arena::new();
    let (mut edit, picture) = doc::create(&NewPicture {
        name: "Big".to_owned(),
        width: size[0],
        height: size[1],
        dpi: 600.0,
        paper: Some(Color::srgb8(255, 255, 255)),
        timelapse: false,
    })
    .unwrap();
    edit.apply(&mut arena).unwrap();
    let whole = doc::layers(&arena, picture).last().unwrap().id;
    let (mut spawn, small) =
        doc::add_raster_layer(&arena, picture, Some(whole), "Small".to_owned(), None).unwrap();
    spawn.apply(&mut arena).unwrap();
    for (layer, rect) in [
        (whole, [0, 0, size[0], size[1]]),
        (small, [size[0] / 3, size[1] / 3, size[0] / 2, size[1] / 2]),
    ] {
        let width = size[0] as usize;
        let row: Vec<u8> = (0..width)
            .flat_map(|x| [(x % 256) as u8, 120, 200, 150])
            .collect();
        let bytes = doc::raster_mut(&mut arena, layer)
            .unwrap()
            .bytes_mut()
            .unwrap();
        for y in rect[1] as usize..rect[3] as usize {
            let (from, to) = (rect[0] as usize * 4, rect[2] as usize * 4);
            bytes[y * width * 4 + from..y * width * 4 + to].copy_from_slice(&row[from..to]);
        }
    }
    (arena, picture, [whole, small])
}

/// An opacity slider dragged on a layer painted all over a comic page at
/// 600 dots an inch: each frame's look takes about the time it is given,
/// never the time the whole page takes, what is on screen comes first,
/// and once the drag rests the image is exactly the picture.
#[test]
fn a_frame_of_an_opacity_drag_on_a_big_picture_stays_within_its_time() {
    let size = [4299, 6070];
    let (mut arena, picture, [whole, _]) = big_picture(size);
    let mut compositor = Compositor::new(picture);
    compositor.sync(&arena);
    compositor.sync(&arena);
    compositor.sync(&arena);
    let budget = Duration::from_millis(8);
    // Far over the budget only if a frame composited the page: that
    // takes seconds without optimizing, as tests run.
    let allowed = Duration::from_millis(if cfg!(debug_assertions) { 400 } else { 60 });
    let visible = [1000, 2000, 2200, 2900];
    let mut worst = Duration::ZERO;
    let mut frame = |compositor: &mut Compositor, arena: &Arena| {
        let start = Instant::now();
        compositor.sync_within(arena, visible, 1, budget);
        worst = worst.max(start.elapsed());
    };
    // Seen small, every frame of the drag shows the value of that
    // frame all over what is on screen, one sample a screen pixel.
    for step in 0..6 {
        SetProperty::new(whole, picture::OPACITY, 0.2 + f64::from(step) * 0.05)
            .apply(&mut arena)
            .unwrap();
        let start = Instant::now();
        compositor.sync_within(&arena, [0, 0, size[0], size[1]], 8, budget);
        let took = start.elapsed();
        assert!(took <= allowed, "a frame seen small took {took:?}");
        assert!(compositor.settling(), "the detail comes after");
        let want = reference_at(&arena, picture, [1028, 2052]);
        for (x, y) in [(1024, 2048), (1031, 2055), (1028, 2052)] {
            assert_eq!(compositor.pixel(x, y), Some(want), "step {step} at {x},{y}");
        }
    }
    for step in 0..12 {
        SetProperty::new(whole, picture::OPACITY, 0.9 - f64::from(step) * 0.05)
            .apply(&mut arena)
            .unwrap();
        frame(&mut compositor, &arena);
        assert!(compositor.settling(), "the page does not fit one frame");
    }
    // What is on screen came first: after a few more frames of the
    // last value it is exact, while the rest of the page still waits.
    let mut fresh = Compositor::new(picture);
    fresh.sync(&arena);
    let mut frames = 0;
    let shows = |compositor: &Compositor| {
        let [x0, y0, x1, y1] = visible;
        let stride = size[0] as usize * 4;
        (y0..y1).all(|y| {
            let row = y as usize * stride + x0 as usize * 4..y as usize * stride + x1 as usize * 4;
            compositor.display().0[row.clone()] == fresh.display().0[row]
        })
    };
    while !shows(&compositor) {
        frame(&mut compositor, &arena);
        frames += 1;
        assert!(frames < 200, "what is on screen is shown");
    }
    assert!(compositor.settling(), "the rest of the page still waits");
    let mut rest = 0;
    while compositor.settling() {
        frame(&mut compositor, &arena);
        rest += 1;
        assert!(rest < 20_000, "the page settles");
    }
    assert!(worst <= allowed, "a frame took {worst:?}, given {budget:?}");
    assert!(
        compositor.display().0 == fresh.display().0,
        "settled, the image is the one composited from nothing"
    );
}

/// An animation's frames redraw only where the cels that come and go
/// have paint, and a frame seen before is copied from the kept tiles.
#[test]
fn animation_frames_redraw_only_what_differs() {
    let size = [640, 480];
    let (mut arena, picture, [whole, small]) = big_picture(size);
    let (mut spawn, other) =
        doc::add_raster_layer(&arena, picture, Some(small), "Other".to_owned(), None).unwrap();
    spawn.apply(&mut arena).unwrap();
    {
        let raster = doc::raster_mut(&mut arena, other).unwrap();
        let bytes = raster.bytes_mut_in([500, 400, 60, 60]).unwrap();
        for y in 400..460 {
            for x in 500..560 {
                let at = (y * 640 + x) * 4;
                bytes[at..at + 4].copy_from_slice(&[200, 0, 0, 255]);
            }
        }
    }
    let frame = |hidden: EntityId| FrameView {
        hidden: [hidden].into_iter().collect(),
        ..FrameView::default()
    };
    let mut compositor = Compositor::new(picture);
    compositor.set_view(Some(frame(other)));
    compositor.sync(&arena);
    let all = u64::from(size[0]) * u64::from(size[1]);
    for round in 0..2 {
        for hidden in [small, other] {
            let version = compositor.display().2;
            compositor.set_view(Some(frame(hidden)));
            assert!(compositor.sync(&arena));
            let redrawn: u64 = compositor
                .changed_since(version)
                .unwrap()
                .iter()
                .map(|[x0, y0, x1, y1]| u64::from(x1 - x0) * u64::from(y1 - y0))
                .sum();
            assert!(
                redrawn > 0 && redrawn < all / 4,
                "round {round}: the two cels' tiles, not the picture: {redrawn} of {all}"
            );
            assert!(
                compositor.display().0 == reference(&arena, picture, compositor.view()).as_slice()
            );
        }
    }
    let _ = whole;
    // Nothing kept: still exact.
    compositor.set_tile_memory(0);
    compositor.set_view(Some(frame(small)));
    compositor.sync(&arena);
    assert_eq!(compositor.tiles.len(), 0);
    assert!(compositor.display().0 == reference(&arena, picture, compositor.view()).as_slice());
}

/// What a stroke's refreshes show through the kept stacks (bytes, a
/// step off at most) comes exact once the stroke is over, with no
/// change of the layers to wait for.
#[test]
fn what_a_stroke_showed_comes_exact_when_it_is_over() {
    let mut dice = Dice(0x57_0C3E);
    let (mut arena, picture) = busy_picture([200, 170], &mut dice);
    let mut compositor = Compositor::new(picture);
    compositor.sync(&arena);
    let (layer, offset) = doc::layers(&arena, picture)
        .into_iter()
        .filter(|info| doc::raster(&arena, info.id).is_some())
        .map(|info| (info.id, info.style.offset))
        .nth(4)
        .unwrap();
    for _ in 0..6 {
        let [x0, y0, x1, y1] = block(&mut arena, layer, &mut dice).unwrap();
        let at = |value: u32, by: i64| (i64::from(value) + by).max(0) as u32;
        let rect = [
            at(x0, offset[0]),
            at(y0, offset[1]),
            at(x1, offset[0]),
            at(y1, offset[1]),
        ];
        compositor.refresh_rects(&arena, &[rect], Some(layer));
    }
    assert!(
        compositor.focus.is_some(),
        "the stroke went through the kept stacks"
    );
    // The frames after the stroke.
    for _ in 0..3 {
        compositor.sync(&arena);
    }
    assert!(
        compositor.display().0 == reference(&arena, picture, None).as_slice(),
        "the image is what every layer at every pixel gives"
    );
}
