//! Rasters are shared and copy-on-write (decision D21 of the 0.0.8 plan).
//!
//! A clone of a raster, of an entity or of a whole arena is a snapshot:
//! it copies no pixel, it can be read on another thread for as long as it
//! lives, and nothing written afterwards shows in it. The writer pays for
//! one copy of one layer, on its first write while such a snapshot is
//! alive, and for none once the snapshot is gone.
//!
//! `sue_core::shared_copies` counts every copy made on the calling
//! thread, so "copies no pixel" is asserted here and not only promised.
//!
//! The benchmark at the end is ignored; run it with
//! `cargo test -p sue-core --release --test cow -- --ignored --nocapture`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use sue_core::codec::Writer;
use sue_core::txn::{RasterEdit, RasterPatch};
use sue_core::{
    Arena, AssetHandle, ComponentData, ComponentKind, ComponentRegistry, Entity, EntityId,
    PixelFormat, RasterTile2D, SueFile, UndoStack, shared_copies,
};

const WIDTH: u32 = 8;
const HEIGHT: u32 = 6;

/// A small layer whose every byte differs from its neighbours.
fn gradient() -> RasterTile2D {
    let pixels = (0..WIDTH * HEIGHT * 4).map(|byte| byte as u8).collect();
    RasterTile2D::from_bytes(WIDTH, HEIGHT, PixelFormat::Rgba8, pixels).unwrap()
}

fn raster(arena: &Arena, id: EntityId) -> &RasterTile2D {
    arena
        .get_by_id(id)
        .unwrap()
        .component(ComponentKind::RasterTile2D)
        .and_then(ComponentData::as_raster)
        .unwrap()
}

fn raster_mut(arena: &mut Arena, id: EntityId) -> &mut RasterTile2D {
    let Some(ComponentData::RasterTile2D(raster)) = arena
        .get_by_id_mut(id)
        .unwrap()
        .component_mut(ComponentKind::RasterTile2D)
    else {
        panic!("the entity has no raster");
    };
    raster
}

fn bytes(arena: &Arena, id: EntityId) -> Vec<u8> {
    raster(arena, id).bytes().unwrap().to_vec()
}

/// The arena as a file would hold it: every entity with its id, every
/// component, every relation.
fn encoded(arena: &Arena) -> Vec<u8> {
    let mut out = Writer::new();
    arena.encode(&mut out);
    out.into_bytes()
}

/// Paints `color` over a rectangle in place, as a tool does, and returns
/// the edit for the undo stack.
fn paint(arena: &mut Arena, id: EntityId, rect: [u32; 4], color: [u8; 4]) -> RasterEdit {
    let [x, y, width, height] = rect;
    let before = RasterPatch::read(raster(arena, id), x, y, width, height).unwrap();
    let layer = raster_mut(arena, id);
    let stride = layer.width() as usize * 4;
    let pixels = layer.bytes_mut().unwrap();
    for line in y..y + height {
        for column in x..x + width {
            let at = line as usize * stride + column as usize * 4;
            pixels[at..at + 4].copy_from_slice(&color);
        }
    }
    let patch = RasterPatch::capture(layer, rect, before).unwrap();
    RasterEdit::new(id, "Stroke", vec![patch])
}

/// A document of three layers and its ids.
fn document() -> (Arena, [EntityId; 3]) {
    let mut arena = Arena::new();
    let mut ids = [EntityId::new(); 3];
    for (index, id) in ids.iter_mut().enumerate() {
        let handle: AssetHandle = arena.spawn(format!("layer {index}"), gradient());
        *id = arena.get(handle).unwrap().id();
    }
    (arena, ids)
}

#[test]
fn cloning_an_entity_copies_no_pixel() {
    let entity = Entity::new("layer", gradient());
    let start = shared_copies();
    let snapshot = entity.clone();
    assert_eq!(shared_copies(), start);
    let pixels = |entity: &Entity| {
        entity
            .component(ComponentKind::RasterTile2D)
            .and_then(ComponentData::as_raster)
            .unwrap()
            .clone()
    };
    assert!(pixels(&entity).shares_pixels_with(&pixels(&snapshot)));
    assert_eq!(entity, snapshot);
    assert_eq!(shared_copies(), start, "comparing and reading copy nothing");
}

#[test]
fn cloning_an_arena_copies_no_pixel() {
    let (arena, ids) = document();
    let start = shared_copies();
    let snapshot = arena.clone();
    let second = snapshot.clone();
    assert_eq!(shared_copies(), start);
    for id in ids {
        assert!(raster(&arena, id).shares_pixels_with(raster(&snapshot, id)));
        assert!(raster(&arena, id).shares_pixels_with(raster(&second, id)));
        assert!(raster(&arena, id).is_shared());
    }
    assert_eq!(encoded(&arena), encoded(&snapshot));
    drop(snapshot);
    drop(second);
    for id in ids {
        assert!(!raster(&arena, id).is_shared(), "the snapshots are gone");
    }
    assert_eq!(shared_copies(), start);
}

#[test]
fn a_write_copies_one_layer_once_and_only_beside_a_snapshot() {
    let (mut arena, ids) = document();
    let start = shared_copies();
    // Nobody else holds the pixels: written in place.
    let _ = paint(&mut arena, ids[0], [0, 0, 2, 2], [1, 1, 1, 1]);
    assert_eq!(shared_copies(), start);

    let snapshot = arena.clone();
    let as_it_was = encoded(&snapshot);
    let _ = paint(&mut arena, ids[1], [1, 1, 3, 2], [9, 9, 9, 9]);
    assert_eq!(shared_copies(), start + 1, "the first write copies");
    let _ = paint(&mut arena, ids[1], [4, 2, 2, 2], [8, 8, 8, 8]);
    let _ = paint(&mut arena, ids[1], [0, 5, 8, 1], [7, 7, 7, 7]);
    assert_eq!(shared_copies(), start + 1, "later writes are in place");
    // Only the written layer left the snapshot's buffer.
    assert!(raster(&arena, ids[0]).shares_pixels_with(raster(&snapshot, ids[0])));
    assert!(!raster(&arena, ids[1]).shares_pixels_with(raster(&snapshot, ids[1])));
    assert!(raster(&arena, ids[2]).shares_pixels_with(raster(&snapshot, ids[2])));
    // And the snapshot holds what it held.
    assert_eq!(encoded(&snapshot), as_it_was);
    assert_ne!(bytes(&arena, ids[1]), bytes(&snapshot, ids[1]));

    drop(snapshot);
    let _ = paint(&mut arena, ids[2], [0, 0, 1, 1], [5, 5, 5, 5]);
    assert_eq!(shared_copies(), start + 1, "no snapshot, no copy");
}

#[test]
fn a_snapshot_is_read_on_another_thread_while_the_layer_is_painted() {
    let (mut arena, ids) = document();
    let before = bytes(&arena, ids[0]);
    let snapshot = arena.clone();
    let (ready, wait) = std::sync::mpsc::channel::<()>();
    let (go, gone) = std::sync::mpsc::channel::<()>();
    let reader = std::thread::spawn(move || {
        ready.send(()).unwrap();
        // Read only once the painter has written.
        gone.recv().unwrap();
        bytes(&snapshot, ids[0])
    });
    wait.recv().unwrap();
    let _ = paint(&mut arena, ids[0], [0, 0, 8, 6], [200, 100, 50, 255]);
    go.send(()).unwrap();
    assert_eq!(reader.join().unwrap(), before, "the reader saw no stroke");
    assert_eq!(bytes(&arena, ids[0])[..4], [200, 100, 50, 255]);
}

#[test]
fn a_raster_edit_undoes_and_redoes_across_a_live_snapshot() {
    let (mut arena, ids) = document();
    let id = ids[1];
    let state_before = encoded(&arena);
    // A snapshot from before the stroke, as a background save holds one.
    let early = arena.clone();

    let mut history = UndoStack::new();
    let edit = paint(&mut arena, id, [2, 1, 3, 2], [9, 9, 9, 9]);
    history.record(edit);
    let state_after = encoded(&arena);
    assert_ne!(state_after, state_before);
    // A snapshot from after the stroke.
    let late = arena.clone();

    let start = shared_copies();
    assert!(history.undo(&mut arena).unwrap());
    assert_eq!(
        encoded(&arena),
        state_before,
        "undo gives the state before, ids included"
    );
    assert_eq!(shared_copies(), start + 1, "undo wrote beside `late`");
    assert!(history.redo(&mut arena).unwrap());
    assert_eq!(
        encoded(&arena),
        state_after,
        "redo gives the state after, ids included"
    );
    assert_eq!(shared_copies(), start + 1, "redo wrote in place");
    assert!(history.undo(&mut arena).unwrap());
    assert_eq!(encoded(&arena), state_before);

    // Neither snapshot moved, whatever the history did.
    assert_eq!(encoded(&early), state_before);
    assert_eq!(encoded(&late), state_after);
    // The layers the stroke never touched are still one buffer in all three.
    for other in [ids[0], ids[2]] {
        assert!(raster(&arena, other).shares_pixels_with(raster(&early, other)));
        assert!(raster(&arena, other).shares_pixels_with(raster(&late, other)));
    }
}

#[test]
fn a_refused_edit_copies_nothing_and_changes_nothing() {
    let (mut arena, ids) = document();
    let snapshot = arena.clone();
    let start = shared_copies();
    let wild = RasterPatch {
        x: 6,
        y: 0,
        width: 4,
        height: 1,
        before: vec![0; 16],
        after: vec![1; 16],
    };
    let mut history = UndoStack::new();
    assert!(
        history
            .apply(&mut arena, RasterEdit::new(ids[0], "Bad", vec![wild]))
            .is_err()
    );
    assert_eq!(shared_copies(), start);
    assert!(raster(&arena, ids[0]).shares_pixels_with(raster(&snapshot, ids[0])));
}

#[test]
fn the_golden_file_is_written_back_the_same_from_a_snapshot() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/kernel.sue");
    let golden = std::fs::read(path).expect("the golden file is committed");
    let file = SueFile::from_bytes(&golden).expect("a 0.0.75 file reads");
    let registry = ComponentRegistry::new();
    let arena = file.arena(&registry).expect("the arena reads");
    let rasters: Vec<EntityId> = arena
        .iter()
        .filter(|(_, entity)| entity.component(ComponentKind::RasterTile2D).is_some())
        .map(|(_, entity)| entity.id())
        .collect();
    assert!(!rasters.is_empty(), "the golden file holds rasters");

    let start = shared_copies();
    let mut painted = arena.clone();
    let snapshot = arena.clone();
    assert_eq!(shared_copies(), start);
    // Write every raster of one clone.
    for &id in &rasters {
        let layer = raster_mut(&mut painted, id);
        if let Some(pixels) = layer.bytes_mut() {
            pixels.iter_mut().for_each(|byte| *byte = !*byte);
        } else if let Some(samples) = layer.f32_samples_mut() {
            samples.iter_mut().for_each(|sample| *sample += 1.0);
        }
    }
    assert_eq!(shared_copies(), start + rasters.len() as u64);
    // The file written from the arena, and from its untouched snapshot,
    // is the same file.
    let save = |arena: &Arena| {
        let mut out = file.clone();
        out.set_arena(arena);
        out.to_bytes()
    };
    let saved = save(&arena);
    assert_eq!(save(&snapshot), saved);
    assert_ne!(save(&painted), saved);
    // And it reads back to the same arena bytes.
    let again = SueFile::from_bytes(&saved)
        .and_then(|file| file.arena(&registry))
        .expect("what was saved reads");
    assert_eq!(encoded(&again), encoded(&arena));
}

// The cost that remains: one copy of one layer on the first write after a
// snapshot was taken.

fn dab(layer: &mut RasterTile2D, x: usize, y: usize, size: usize) {
    let stride = layer.width() as usize * 4;
    let pixels = layer.bytes_mut().unwrap();
    for line in y..y + size {
        let at = line * stride + x * 4;
        pixels[at..at + size * 4].fill(0x7F);
    }
}

fn millis(time: Duration) -> f64 {
    time.as_secs_f64() * 1000.0
}

/// Smallest, middle and largest of `runs` timings of `step`, which gets a
/// fresh layer each time from `setup`.
fn timed<S>(runs: usize, mut setup: impl FnMut() -> S, mut step: impl FnMut(&mut S)) -> [f64; 3] {
    let mut times: Vec<f64> = (0..runs)
        .map(|_| {
            let mut state = setup();
            let start = Instant::now();
            step(&mut state);
            let time = start.elapsed();
            drop(state);
            millis(time)
        })
        .collect();
    times.sort_by(f64::total_cmp);
    [times[0], times[runs / 2], times[runs - 1]]
}

#[test]
#[ignore = "a benchmark: prints the time of the first dab after a snapshot"]
fn first_dab_after_a_snapshot() {
    const RUNS: usize = 15;
    const DAB: usize = 100;
    println!("first dab after a snapshot, Rgba8, {RUNS} runs, milliseconds");
    println!("| layer | megabytes | case | least | middle | most |");
    println!("| --- | --- | --- | --- | --- | --- |");
    // The plan's 4,000 px square, an A4 page at 350 dpi, and a small one.
    for (width, height) in [(4000_u32, 4000_u32), (2894, 4093), (1920, 1080)] {
        let layer = || {
            let mut layer = RasterTile2D::new(width, height, PixelFormat::Rgba8).unwrap();
            // Touch every page, as a layer that was painted on has.
            layer.bytes_mut().unwrap().fill(0x10);
            layer
        };
        let megabytes = f64::from(width) * f64::from(height) * 4.0 / 1_000_000.0;
        let row = |case: &str, [least, middle, most]: [f64; 3]| {
            println!(
                "| {width} x {height} | {megabytes:.1} | {case} | {least:.3} | {middle:.3} | {most:.3} |"
            );
        };
        row(
            "no snapshot",
            timed(RUNS, layer, |layer| dab(layer, 50, 50, DAB)),
        );
        row(
            "taking the snapshot",
            timed(RUNS, layer, |layer| {
                std::hint::black_box(layer.clone());
            }),
        );
        row(
            "first dab, snapshot alive",
            timed(
                RUNS,
                || {
                    let layer = layer();
                    let snapshot = layer.clone();
                    (layer, snapshot)
                },
                |(layer, _snapshot)| dab(layer, 50, 50, DAB),
            ),
        );
        row(
            "second dab, snapshot alive",
            timed(
                RUNS,
                || {
                    let mut layer = layer();
                    let snapshot = layer.clone();
                    dab(&mut layer, 50, 50, DAB);
                    (layer, snapshot)
                },
                |(layer, _snapshot)| dab(layer, 300, 300, DAB),
            ),
        );
        row(
            "first dab, snapshot dropped",
            timed(
                RUNS,
                || {
                    let layer = layer();
                    drop(layer.clone());
                    layer
                },
                |layer| dab(layer, 50, 50, DAB),
            ),
        );
    }
}
