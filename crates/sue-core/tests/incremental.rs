//! Incremental save (D12): a save into the file it saved before re-reads,
//! re-hashes and re-encodes only the tiles that changed, and still writes
//! the very bytes a save into a new file writes. A seeded mix of every
//! way pixels change (strokes, fills, filters, transforms, canvas resizes,
//! new and deleted layers, undo and redo) checks that no change is ever
//! missed.

use sue_core::txn::{DespawnEntity, RasterEdit, RasterPatch, ReplaceComponent, SpawnEntity};
use sue_core::{
    Arena, ComponentData, ComponentKind, ComponentRegistry, Entity, EntityId, PixelFormat,
    RasterTile2D, SueFile, UndoStack,
};

/// A small seeded generator (xorshift), so a failure repeats.
struct Seeded(u64);

impl Seeded {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, limit: u32) -> u32 {
        (self.next() % u64::from(limit.max(1))) as u32
    }
}

fn raster_of(arena: &Arena, id: EntityId) -> &RasterTile2D {
    arena
        .get_by_id(id)
        .unwrap()
        .component(ComponentKind::RasterTile2D)
        .and_then(ComponentData::as_raster)
        .unwrap()
}

fn raster_mut(arena: &mut Arena, id: EntityId) -> &mut RasterTile2D {
    match arena
        .get_by_id_mut(id)
        .unwrap()
        .component_mut(ComponentKind::RasterTile2D)
    {
        Some(ComponentData::RasterTile2D(raster)) => raster,
        _ => panic!("a layer without pixels"),
    }
}

/// Noise in some places, nothing in others, so tiles are blobs, empty and
/// solid.
fn layer(random: &mut Seeded, width: u32, height: u32) -> RasterTile2D {
    let mut raster = RasterTile2D::new(width, height, PixelFormat::Rgba8).unwrap();
    let solid = random.below(3) == 0;
    let bytes = raster.bytes_mut().unwrap();
    for (index, pixel) in bytes.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let x = index as u32 % width;
        if solid {
            pixel.copy_from_slice(&[40, 80, 120, 255]);
        } else if x < width / 2 {
            pixel.copy_from_slice(&[(random.next() >> 30) as u8, x as u8, 7, 255]);
        }
    }
    raster
}

/// A rectangle inside a `width` by `height` raster.
fn rect(random: &mut Seeded, width: u32, height: u32) -> [u32; 4] {
    let x = random.below(width);
    let y = random.below(height);
    let w = 1 + random.below((width - x).min(90));
    let h = 1 + random.below((height - y).min(90));
    [x, y, w, h]
}

/// Paints `area` of the layer in place the way the brush does (the whole
/// raster counts as changed) or the way a patch does (only the tiles of
/// the rectangle), and records the edit for undo.
fn paint(
    arena: &mut Arena,
    undo: &mut UndoStack<Arena>,
    id: EntityId,
    area: [u32; 4],
    shade: u8,
    whole: bool,
) {
    let [x, y, w, h] = area;
    let raster = raster_mut(arena, id);
    let before = RasterPatch::read(raster, x, y, w, h).unwrap();
    let stride = raster.width() as usize * 4;
    let bytes = if whole {
        raster.bytes_mut()
    } else {
        raster.bytes_mut_in(area)
    }
    .unwrap();
    for row in y..y + h {
        for column in x..x + w {
            let at = row as usize * stride + column as usize * 4;
            bytes[at..at + 4].copy_from_slice(&[shade, shade ^ 0x5a, row as u8, 255]);
        }
    }
    let patch = RasterPatch::capture(raster, [x, y, w, h], before).unwrap();
    undo.record(RasterEdit::new(id, "Paint", vec![patch]));
}

/// The arena saved into `file` (incrementally) and into a new file: the
/// same bytes, and both read back to the arena's pixels.
fn check(step: usize, what: &str, arena: &Arena, file: &mut SueFile) {
    file.set_arena(arena);
    let mut full = SueFile::new();
    full.set_arena(arena);
    assert!(
        file.to_bytes() == full.to_bytes(),
        "step {step} ({what}): the incremental save differs from a full one"
    );
    let back = file.arena(&ComponentRegistry::new()).unwrap();
    for (_, entity) in arena.iter() {
        assert_eq!(
            back.get_by_id(entity.id()).unwrap(),
            entity,
            "step {step} ({what})"
        );
    }
    assert_eq!(back.len(), arena.len());
}

#[test]
fn an_incremental_save_equals_a_full_one_after_every_kind_of_change() {
    for seed in [0x9e37_79b9_7f4a_7c15_u64, 42] {
        let mut random = Seeded(seed);
        let mut arena = Arena::new();
        let mut undo: UndoStack<Arena> = UndoStack::new();
        let mut layers: Vec<EntityId> = Vec::new();
        for _ in 0..3 {
            let raster = layer(&mut random, 520, 300);
            let handle = arena.insert(Entity::new("Layer", raster));
            layers.push(arena.get(handle).unwrap().id());
        }
        let mut file = SueFile::new();
        check(0, "start", &arena, &mut file);
        let mut skipped = 0;
        for step in 1..=45 {
            let live: Vec<EntityId> = layers
                .iter()
                .copied()
                .filter(|id| arena.get_by_id(*id).is_ok())
                .collect();
            let choice = random.below(9);
            let what = match (choice, live.is_empty()) {
                (_, true) | (8, _) => {
                    let width = 200 + random.below(400);
                    let raster = layer(&mut random, width, 200);
                    let entity = Entity::new("New Layer", raster);
                    layers.push(entity.id());
                    undo.apply(&mut arena, SpawnEntity::new(entity)).unwrap();
                    "a new layer"
                }
                (0 | 1, false) => {
                    let id = live[random.below(live.len() as u32) as usize];
                    let raster = raster_of(&arena, id);
                    let area = rect(&mut random, raster.width(), raster.height());
                    paint(&mut arena, &mut undo, id, area, random.next() as u8, true);
                    "a stroke"
                }
                (2, false) => {
                    let id = live[random.below(live.len() as u32) as usize];
                    let raster = raster_of(&arena, id);
                    let area = rect(&mut random, raster.width(), raster.height());
                    paint(&mut arena, &mut undo, id, area, random.next() as u8, false);
                    "a fill"
                }
                (3, false) => {
                    // A filter writes every pixel of the layer.
                    let id = live[random.below(live.len() as u32) as usize];
                    let raster = raster_of(&arena, id);
                    let area = [0, 0, raster.width(), raster.height()];
                    let before =
                        RasterPatch::read(raster, 0, 0, raster.width(), raster.height()).unwrap();
                    let raster = raster_mut(&mut arena, id);
                    for byte in raster.bytes_mut().unwrap().iter_mut().step_by(4) {
                        *byte = 255 - *byte;
                    }
                    let patch = RasterPatch::capture(raster, area, before).unwrap();
                    undo.record(RasterEdit::new(id, "Invert", vec![patch]));
                    "a filter"
                }
                (4, false) => {
                    // A transform: a flipped copy replaces the buffer.
                    let id = live[random.below(live.len() as u32) as usize];
                    let raster = raster_of(&arena, id);
                    let (width, height) = (raster.width(), raster.height());
                    let bytes = raster.bytes().unwrap();
                    let mut flipped = Vec::with_capacity(bytes.len());
                    for row in bytes.chunks_exact(width as usize * 4).rev() {
                        flipped.extend_from_slice(row);
                    }
                    let flipped =
                        RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, flipped)
                            .unwrap();
                    undo.apply(&mut arena, ReplaceComponent::new(id, flipped, "Flip"))
                        .unwrap();
                    "a transform"
                }
                (5, false) => {
                    // A canvas resize: the pixels move into a larger buffer.
                    let id = live[random.below(live.len() as u32) as usize];
                    let raster = raster_of(&arena, id);
                    let (width, height) = (raster.width(), raster.height());
                    let (grown_w, grown_h) = (width + 1 + random.below(200), height + 37);
                    let mut grown =
                        RasterTile2D::new(grown_w, grown_h, PixelFormat::Rgba8).unwrap();
                    let old = raster.bytes().unwrap().to_vec();
                    let out = grown.bytes_mut().unwrap();
                    for row in 0..height as usize {
                        let from = row * width as usize * 4;
                        let to = row * grown_w as usize * 4;
                        out[to..to + width as usize * 4]
                            .copy_from_slice(&old[from..from + width as usize * 4]);
                    }
                    undo.apply(&mut arena, ReplaceComponent::new(id, grown, "Canvas Size"))
                        .unwrap();
                    "a canvas resize"
                }
                (6, false) => {
                    let id = live[random.below(live.len() as u32) as usize];
                    undo.apply(&mut arena, DespawnEntity::new(id)).unwrap();
                    "a deleted layer"
                }
                (7, false) => {
                    if random.below(2) == 0 {
                        if !undo.undo(&mut arena).unwrap() {
                            skipped += 1;
                        }
                        "an undo"
                    } else {
                        if !undo.redo(&mut arena).unwrap() {
                            skipped += 1;
                        }
                        "a redo"
                    }
                }
                _ => unreachable!(),
            };
            check(step, what, &arena, &mut file);
        }
        assert!(skipped < 20, "most undos and redos had something to do");
        // Every step undone, one by one, then redone.
        let mut step = 100;
        while undo.undo(&mut arena).unwrap() {
            step += 1;
            check(step, "undo all", &arena, &mut file);
        }
        while undo.redo(&mut arena).unwrap() {
            step += 1;
            check(step, "redo all", &arena, &mut file);
        }
    }
}

#[test]
fn a_one_tile_edit_re_encodes_one_tile() {
    let mut random = Seeded(7);
    // Sixteen tiles of noise: every one a blob.
    let mut raster = RasterTile2D::new(1024, 1024, PixelFormat::Rgba8).unwrap();
    for byte in raster.bytes_mut().unwrap() {
        *byte = (random.next() >> 40) as u8;
    }
    let mut arena = Arena::new();
    let handle = arena.insert(Entity::new("Layer", raster));
    let id = arena.get(handle).unwrap().id();
    let mut file = SueFile::new();
    file.set_arena(&arena);
    let first = file.last_save();
    assert_eq!((first.tiles, first.encoded, first.remembered), (16, 16, 0));

    // Nothing changed: nothing is read, hashed or encoded.
    file.set_arena(&arena);
    let again = file.last_save();
    assert_eq!(
        (again.remembered, again.hashed, again.encoded, again.copied),
        (16, 0, 0, 16)
    );

    // A patch in one tile: that tile alone is read, hashed and encoded.
    raster_mut(&mut arena, id)
        .bytes_mut_in([300, 300, 5, 5])
        .unwrap()[300 * 4096 + 1200] ^= 1;
    file.set_arena(&arena);
    let one = file.last_save();
    assert_eq!(
        (one.remembered, one.hashed, one.encoded, one.copied),
        (15, 1, 1, 15)
    );

    // A write that cannot name its tiles marks them all: all are hashed,
    // but still only the changed one is encoded.
    raster_mut(&mut arena, id).bytes_mut().unwrap()[0] ^= 1;
    file.set_arena(&arena);
    let whole = file.last_save();
    assert_eq!(
        (whole.remembered, whole.hashed, whole.encoded, whole.copied),
        (0, 16, 1, 15)
    );

    // A file read from disk remembers nothing, yet re-encodes nothing.
    let mut loaded = SueFile::from_bytes(&file.to_bytes()).unwrap();
    let back = loaded.arena(&ComponentRegistry::new()).unwrap();
    loaded.set_arena(&back);
    let reloaded = loaded.last_save();
    assert_eq!(
        (reloaded.hashed, reloaded.encoded, reloaded.copied),
        (16, 0, 16)
    );
    assert_eq!(loaded.to_bytes(), file.to_bytes());
}

#[test]
fn a_snapshot_keeps_its_revisions_and_writes_mark_only_their_tiles() {
    let mut raster = RasterTile2D::new(600, 300, PixelFormat::Gray8).unwrap();
    assert_eq!(raster.tile_revisions().len(), 3 * 2);
    let snapshot = raster.clone();
    assert_eq!(snapshot.tile_revisions(), raster.tile_revisions());
    let before = raster.tile_revisions().to_vec();
    // The right half of the second row of tiles.
    raster.bytes_mut_in([300, 260, 300, 40]).unwrap();
    let after = raster.tile_revisions().to_vec();
    let changed: Vec<usize> = (0..6)
        .filter(|tile| before[*tile] != after[*tile])
        .collect();
    assert_eq!(changed, [4, 5]);
    // Nothing outside the raster: nothing marked.
    raster.bytes_mut_in([600, 0, 10, 10]).unwrap();
    assert_eq!(raster.tile_revisions(), after.as_slice());
    // Revisions are not pixels.
    assert_eq!(raster, snapshot);
    raster.f32_samples_mut();
    raster.bytes_mut().unwrap();
    assert!(raster.tile_revisions().iter().all(|r| !before.contains(r)));
}
