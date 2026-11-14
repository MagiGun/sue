//! The pixel store through the file API: `PIXL` version 1 beside an
//! `ARNA` of version 2, the copy for older Sue (version 1), the blobs kept
//! for data this build cannot read, and a save that fails part way.

use std::io::{self, Write};
use std::path::PathBuf;

use sue_core::codec::{Reader, Writer};
use sue_core::file::{ARENA, PIXELS};
use sue_core::pixels::{HASH_SHA256, KEY_WIDTH, PixelOptions, TILE_SIZE, TILED};
use sue_core::{
    Arena, Chunk, ComponentKind, ComponentRegistry, Entity, PixelFormat, RasterTile2D,
    StringBuffer, SueFile,
};

fn raster_of(entity: &Entity) -> Option<&RasterTile2D> {
    entity
        .component(ComponentKind::RasterTile2D)
        .and_then(sue_core::ComponentData::as_raster)
}

fn registry() -> ComponentRegistry {
    ComponentRegistry::new()
}

/// A raster whose pixels are `pixel(x, y)`.
fn raster(
    width: u32,
    height: u32,
    format: PixelFormat,
    pixel: impl Fn(u32, u32) -> Vec<u8>,
) -> RasterTile2D {
    let mut data = Vec::new();
    for y in 0..height {
        for x in 0..width {
            data.extend(pixel(x, y));
        }
    }
    RasterTile2D::from_bytes(width, height, format, data).unwrap()
}

/// Line art: thin dark lines on white, so tiles differ.
fn line_art(width: u32, height: u32, seed: u32) -> RasterTile2D {
    raster(width, height, PixelFormat::Rgba8, |x, y| {
        let on = (x + seed).is_multiple_of(37) || (y * 3 + x + seed).is_multiple_of(101);
        if on {
            vec![20, 20, 30, 255]
        } else {
            vec![255, 255, 255, 255]
        }
    })
}

fn layer(arena: &mut Arena, name: &str, raster: RasterTile2D) -> sue_core::EntityId {
    let handle = arena.spawn(name, raster);
    arena.get(handle).unwrap().id()
}

fn saved(arena: &Arena) -> SueFile {
    let mut file = SueFile::new();
    file.set_arena(arena);
    SueFile::from_bytes(&file.to_bytes()).unwrap()
}

fn store_len(file: &SueFile) -> usize {
    let store = file.chunk(PIXELS).expect("a pixel store");
    let mut input = Reader::new(&store.payload);
    input.varint().unwrap();
    input.u8().unwrap();
    input.u8().unwrap();
    usize::try_from(input.varint().unwrap()).unwrap()
}

/// A picture of every tile state: empty, solid and blob tiles, edge tiles
/// narrower and lower than the rest, in every pixel format.
fn every_state() -> Arena {
    let mut arena = Arena::new();
    // 600 x 300: three columns (256, 256, 88) and two rows (256, 44).
    layer(
        &mut arena,
        "Empty",
        RasterTile2D::new(600, 300, PixelFormat::Rgba8).unwrap(),
    );
    layer(
        &mut arena,
        "Mixed",
        raster(600, 300, PixelFormat::Rgba8, |x, y| {
            match (x / 256, y / 256) {
                (0, 0) => vec![0, 0, 0, 0],
                (1, 0) => vec![200, 100, 50, 255],
                _ => vec![(x % 251) as u8, (y % 13) as u8, 7, 255],
            }
        }),
    );
    layer(
        &mut arena,
        "Mask",
        raster(300, 600, PixelFormat::Gray8, |x, y| {
            vec![((x ^ y) & 0xFF) as u8]
        }),
    );
    let samples: Vec<f32> = (0..300 * 270 * 4)
        .map(|i: u32| {
            if i.is_multiple_of(7) {
                f32::NAN
            } else {
                i as f32 * 0.5
            }
        })
        .collect();
    layer(
        &mut arena,
        "Light",
        RasterTile2D::from_f32(300, 270, samples).unwrap(),
    );
    layer(
        &mut arena,
        "Notes",
        RasterTile2D::new(1, 1, PixelFormat::Gray8).unwrap(),
    );
    arena.spawn("Text", StringBuffer::from("not a picture"));
    arena
}

fn same(a: &Arena, b: &Arena) {
    assert_eq!(a.len(), b.len());
    for (_, entity) in a.iter() {
        let other = b.get_by_id(entity.id()).unwrap();
        // NaN != NaN, so float rasters compare by bits.
        match (raster_of(entity), raster_of(other)) {
            (Some(x), Some(y)) if x.format() == PixelFormat::RgbaF32 => {
                let bits = |r: &RasterTile2D| {
                    r.f32_samples()
                        .unwrap()
                        .iter()
                        .map(|s| s.to_bits())
                        .collect::<Vec<_>>()
                };
                assert_eq!((x.width(), x.height()), (y.width(), y.height()));
                assert_eq!(bits(x), bits(y), "{}", entity.name());
            }
            _ => assert_eq!(entity, other, "{}", entity.name()),
        }
    }
    assert_eq!(
        a.relations().iter().collect::<Vec<_>>(),
        b.relations().iter().collect::<Vec<_>>()
    );
}

#[test]
fn every_tile_state_round_trips_through_the_store() {
    let arena = every_state();
    let file = saved(&arena);
    assert_eq!(file.chunk(ARENA).unwrap().version, 2);
    assert_eq!(file.chunk(PIXELS).unwrap().version, 1);
    // Every raster went by reference.
    let loaded = file.arena(&registry()).unwrap();
    same(&arena, &loaded);
    // Saving what was loaded gives the same bytes again.
    let mut again = SueFile::new();
    again.set_arena(&loaded);
    assert_eq!(again.to_bytes(), {
        let mut first = SueFile::new();
        first.set_arena(&arena);
        first.to_bytes()
    });
}

#[test]
fn identical_tiles_are_stored_once() {
    let mut arena = Arena::new();
    let art = line_art(512, 512, 0);
    layer(&mut arena, "Ink", art.clone());
    layer(&mut arena, "Ink copy", art.clone());
    // The same tile repeated four times inside one raster.
    let tile = line_art(256, 256, 5);
    let bytes = tile.bytes().unwrap();
    let repeated = raster(512, 512, PixelFormat::Rgba8, |x, y| {
        let at = (((y % 256) * 256 + (x % 256)) * 4) as usize;
        bytes[at..at + 4].to_vec()
    });
    layer(&mut arena, "Pattern", repeated);
    let file = saved(&arena);
    // "Ink" has four distinct tiles, its copy none of its own, "Pattern" one.
    assert_eq!(store_len(&file), 5);
    same(&arena, &file.arena(&registry()).unwrap());
}

#[test]
fn an_empty_layer_costs_a_few_bytes() {
    let mut arena = Arena::new();
    layer(
        &mut arena,
        "Empty A4",
        RasterTile2D::new(2894, 4093, PixelFormat::Rgba8).unwrap(),
    );
    let bytes = {
        let mut file = SueFile::new();
        file.set_arena(&arena);
        file.to_bytes()
    };
    // 47.4 MB as 0.0.75 stores it; one byte per tile here.
    assert!(bytes.len() < 1024, "{} bytes", bytes.len());
}

#[test]
fn version_one_and_version_two_load_to_equal_arenas() {
    let arena = every_state();
    let mut tiled = SueFile::new();
    tiled.set_arena(&arena);
    let (older, left_out) = tiled.copy_for_older_sue(&arena);
    assert!(left_out.is_empty());
    let older = SueFile::from_bytes(&older.to_bytes()).unwrap();
    assert_eq!(older.chunk(ARENA).unwrap().version, 1);
    assert!(older.chunk(PIXELS).is_none());
    let tiled = SueFile::from_bytes(&tiled.to_bytes()).unwrap();
    let (a, b) = (
        tiled.arena(&registry()).unwrap(),
        older.arena(&registry()).unwrap(),
    );
    same(&a, &b);
    same(&arena, &b);
    // The copy is what 0.0.75 wrote: every raster inline, no TILED bit.
    let mut inline = Writer::new();
    arena.encode(&mut inline);
    assert_eq!(older.chunk(ARENA).unwrap().payload, inline.into_bytes());
}

#[test]
fn a_document_without_pictures_stays_version_one() {
    let mut arena = Arena::new();
    arena.spawn("Chapter", StringBuffer::from("Once."));
    let file = saved(&arena);
    assert_eq!(file.chunk(ARENA).unwrap().version, 1);
    assert!(file.chunk(PIXELS).is_none());
}

#[test]
fn a_damaged_blob_names_its_layer() {
    let mut arena = Arena::new();
    layer(&mut arena, "Background", line_art(300, 300, 1));
    layer(&mut arena, "Inks of page 3", line_art(300, 300, 2));
    let file = saved(&arena);
    let store = file.chunk(PIXELS).unwrap().clone();

    // Every blob's data bytes flipped in turn: the error, if any, names
    // the layer whose tile it is. The checksum of the chunk is made right
    // again, so only the store's own checks can catch it.
    let mut named = 0;
    for at in (8..store.payload.len()).step_by(97) {
        let mut damaged = file.clone();
        let mut payload = store.payload.clone();
        payload[at] ^= 0x5A;
        damaged.set(Chunk {
            payload,
            ..store.clone()
        });
        let Ok(reread) = SueFile::from_bytes(&damaged.to_bytes()) else {
            continue;
        };
        if let Err(error) = reread.arena(&registry()) {
            let names_layer = error.message.contains("\"Background\"")
                || error.message.contains("\"Inks of page 3\"");
            let store_level = error.message.contains("pixel store");
            assert!(names_layer || store_level, "{}", error.message);
            named += usize::from(names_layer);
        }
    }
    assert!(named > 0, "some damage was caught at a layer");

    // A tile missing from the store names its layer.
    let mut missing = file.clone();
    let mut other = Arena::new();
    layer(&mut other, "Elsewhere", line_art(300, 300, 9));
    let mut decoy = SueFile::new();
    decoy.set_arena(&other);
    missing.set(decoy.chunk(PIXELS).unwrap().clone());
    let error = missing.arena(&registry()).unwrap_err();
    assert!(
        error.message.contains("\"Background\"") && error.message.contains("missing"),
        "{}",
        error.message
    );

    // A version 2 arena without its store says so.
    let mut no_store = file.clone();
    no_store.remove(PIXELS);
    let error = no_store.arena(&registry()).unwrap_err();
    assert!(error.message.contains("pixel store"), "{}", error.message);
}

#[test]
fn a_newer_store_or_arena_is_refused_clearly() {
    let file = saved(&every_state());
    let mut newer = file.clone();
    let mut store = newer.chunk(PIXELS).unwrap().clone();
    store.version = 2;
    newer.set(store);
    let error = newer.arena(&registry()).unwrap_err();
    assert!(error.message.contains("newer Sue"), "{}", error.message);
    let mut newer = file.clone();
    let mut arena = newer.chunk(ARENA).unwrap().clone();
    arena.version = 3;
    newer.set(arena);
    assert!(
        newer
            .arena(&registry())
            .unwrap_err()
            .message
            .contains("newer Sue")
    );
}

#[test]
fn tile_size_and_hash_kind_are_read_from_the_header() {
    let arena = every_state();
    let options = PixelOptions {
        level: 6,
        tile_size: 64,
        key_width: 32,
    };
    let mut file = SueFile::new();
    file.set_arena_with(&arena, &|_| true, &options);
    let file = SueFile::from_bytes(&file.to_bytes()).unwrap();
    let header = file.pixel_store().unwrap().unwrap();
    assert_eq!(
        (header.tile_size, header.hash_kind, header.key_width),
        (64, HASH_SHA256, 32)
    );
    same(&arena, &file.arena(&registry()).unwrap());
    // The default is 256 px and 128-bit keys.
    let header = saved(&arena).pixel_store().unwrap().unwrap();
    assert_eq!((header.tile_size, header.key_width), (TILE_SIZE, KEY_WIDTH));

    // A hash kind this build has never heard of reads the same: a reader
    // never recomputes a key. Byte 1 is the hash kind after the one-byte
    // tile size 64.
    let mut strange = file.clone();
    let mut store = strange.chunk(PIXELS).unwrap().clone();
    assert_eq!(store.payload[..3], [64, HASH_SHA256, 32]);
    store.payload[1] = 200;
    strange.set(store);
    same(&arena, &strange.arena(&registry()).unwrap());
    assert_eq!(strange.pixel_store().unwrap().unwrap().hash_kind, 200);
    // Saved again with something unknown beside it, the old blobs are
    // kept under their old keys, and the kind becomes "keys are names".
    let loaded = strange.arena(&registry()).unwrap();
    strange.set(Chunk {
        tag: *b"FUTR",
        version: 1,
        payload: vec![1],
    });
    strange.set_arena(&loaded);
    let header = strange.pixel_store().unwrap().unwrap();
    assert_eq!(
        (header.tile_size, header.hash_kind, header.key_width),
        (64, 0, 32)
    );
    same(&arena, &strange.arena(&registry()).unwrap());
}

#[test]
fn blobs_nothing_known_points_at_survive_beside_unknown_data() {
    let mut arena = Arena::new();
    layer(&mut arena, "Kept", line_art(300, 300, 3));
    let gone = layer(&mut arena, "Deleted", line_art(300, 300, 4));
    let file = saved(&arena);
    let blobs = store_len(&file);
    let mut smaller = file.arena(&registry()).unwrap();
    let handle = smaller.handle_of(gone).unwrap();
    smaller.despawn(handle).unwrap();

    // Nothing unknown: the deleted layer's blobs go.
    let mut plain = file.clone();
    plain.set_arena(&smaller);
    assert!(store_len(&plain) < blobs);

    // A chunk from the future may point at them: every blob stays, byte
    // for byte, and the chunk too.
    let mut future = file.clone();
    future.set(Chunk {
        tag: *b"FUTR",
        version: 9,
        payload: vec![1, 2, 3],
    });
    future.set_arena(&smaller);
    let reread = SueFile::from_bytes(&future.to_bytes()).unwrap();
    assert_eq!(store_len(&reread), blobs);
    let old = file.chunk(PIXELS).unwrap().payload.clone();
    let new = &reread.chunk(PIXELS).unwrap().payload;
    assert_eq!(new, &old, "the same blobs in the same order");
    assert_eq!(reread.chunk(*b"FUTR").unwrap().payload, [1, 2, 3]);
    same(&smaller, &reread.arena(&registry()).unwrap());

    // So does a raster of a pixel format this build does not know: it may
    // be tiles by reference.
    let mut opaque = file.clone();
    let mut with_unread = smaller.clone();
    let mut payload = Writer::new();
    payload.u32(4);
    payload.u32(4);
    payload.u8(TILED | 0x7E);
    payload.raw(&[9; 20]);
    let mut entity = Writer::new();
    entity.raw(&[0xAB; 16]);
    entity.str("From later");
    entity.str("sue.raster");
    entity.str("");
    entity.len(1);
    entity.str(ComponentKind::RasterTile2D.name());
    entity.bytes(payload.as_bytes());
    entity.len(0);
    entity.len(0);
    entity.len(0);
    let bytes = entity.into_bytes();
    let later = Entity::decode(&mut Reader::new(&bytes), &registry()).unwrap();
    assert!(later.holds_unread());
    with_unread.insert_keep_id(later).unwrap();
    opaque.set_arena(&with_unread);
    assert_eq!(store_len(&opaque), blobs);
}

#[test]
fn the_copy_for_older_sue_leaves_out_what_it_cannot_read() {
    let mut arena = every_state();
    let mut payload = Writer::new();
    payload.u32(2);
    payload.u32(2);
    payload.u8(PixelFormat::RGBA16_CODE);
    payload.bytes(&[0; 32]);
    let mut entity = Writer::new();
    entity.raw(&[0xCD; 16]);
    entity.str("Deep layer");
    entity.str("sue.raster");
    entity.str(ComponentKind::RasterTile2D.name());
    entity.len(1);
    entity.str(ComponentKind::RasterTile2D.name());
    entity.bytes(payload.as_bytes());
    entity.len(0);
    entity.len(0);
    entity.len(0);
    let bytes = entity.into_bytes();
    let deep = Entity::decode(&mut Reader::new(&bytes), &registry()).unwrap();
    // A 16-bit code is reserved, not read: the raster is kept, not drawn.
    assert!(raster_of(&deep).is_none());
    assert!(deep.unread_component(ComponentKind::RasterTile2D).is_some());
    arena.insert_keep_id(deep).unwrap();

    // The new build keeps it through a save.
    let file = saved(&arena);
    let loaded = file.arena(&registry()).unwrap();
    same(&arena, &loaded);

    let (older, left_out) = file.copy_for_older_sue(&loaded);
    assert_eq!(left_out, ["Deep layer"]);
    let older = SueFile::from_bytes(&older.to_bytes()).unwrap();
    let back = older.arena(&registry()).unwrap();
    let deep = back
        .iter()
        .find(|(_, e)| e.name() == "Deep layer")
        .unwrap()
        .1;
    assert!(!deep.holds_unread());
    assert_eq!(deep.base_kind(), None);
}

#[test]
fn the_golden_file_of_0_0_75_loads_and_saves_into_the_store() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/kernel.sue");
    let bytes = std::fs::read(path).unwrap();
    let file = SueFile::from_bytes(&bytes).unwrap();
    let arena = file.arena(&registry()).unwrap();
    assert!(arena.iter().any(|(_, entity)| raster_of(entity).is_some()));
    // Saved by this build: the rasters go into the store, the unknown
    // chunk of the golden file stays, and it all reads back the same.
    let mut resaved = file.clone();
    resaved.set_arena(&arena);
    let reread = SueFile::from_bytes(&resaved.to_bytes()).unwrap();
    assert_eq!(reread.chunk(ARENA).unwrap().version, 2);
    assert!(reread.chunk(*b"GOLD").is_some());
    same(&arena, &reread.arena(&registry()).unwrap());
    // And the copy for older Sue is the golden arena again, byte for byte.
    let (older, _) = reread.copy_for_older_sue(&reread.arena(&registry()).unwrap());
    assert_eq!(older.chunk(ARENA), file.chunk(ARENA));
}

/// A writer that fails once `limit` bytes have gone through it.
struct FailAt<W> {
    inner: W,
    written: usize,
    limit: usize,
}

impl<W: Write> Write for FailAt<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.written >= self.limit {
            return Err(io::Error::other("the disk is full"));
        }
        let len = buf.len().min(self.limit - self.written);
        let len = self.inner.write(&buf[..len])?;
        self.written += len;
        Ok(len)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Writes `file` the way a save does: into a new file beside `path`, which
/// replaces `path` only once it is whole.
fn save(file: &SueFile, path: &std::path::Path, limit: usize) -> io::Result<()> {
    let temporary = path.with_extension("sue.tmp");
    let result = (|| {
        let mut out = FailAt {
            inner: io::BufWriter::new(std::fs::File::create(&temporary)?),
            written: 0,
            limit,
        };
        file.write_to(&mut out)?;
        out.flush()?;
        drop(out);
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[test]
fn a_save_that_fails_at_any_byte_leaves_the_old_file_whole() {
    let dir = std::env::temp_dir().join(format!("sue-pixels-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("picture.sue");

    let mut first = Arena::new();
    layer(&mut first, "Sketch", line_art(300, 200, 1));
    let mut old = SueFile::new();
    old.set_arena(&first);
    let old_bytes = old.to_bytes();
    std::fs::write(&path, &old_bytes).unwrap();

    let mut second = first.clone();
    layer(&mut second, "Inks", line_art(300, 200, 7));
    let mut new = old.clone();
    new.set_arena(&second);
    let len = new.encoded_len();

    for limit in 0..len {
        assert!(save(&new, &path, limit).is_err(), "failing at byte {limit}");
        assert_eq!(std::fs::read(&path).unwrap(), old_bytes, "byte {limit}");
    }
    let loaded = SueFile::from_bytes(&std::fs::read(&path).unwrap())
        .unwrap()
        .arena(&registry())
        .unwrap();
    same(&first, &loaded);
    save(&new, &path, usize::MAX).unwrap();
    let loaded = SueFile::from_bytes(&std::fs::read(&path).unwrap())
        .unwrap()
        .arena(&registry())
        .unwrap();
    same(&second, &loaded);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_canvas_layout_of_older_builds_keeps_no_blob_alive() {
    let mut arena = Arena::new();
    layer(&mut arena, "Kept", line_art(300, 300, 3));
    let gone = layer(&mut arena, "Deleted", line_art(300, 300, 4));
    let mut file = SueFile::new();
    // Written by the builds before the Hub, long before the pixel store.
    file.set(Chunk {
        tag: *b"SCNE",
        version: 1,
        payload: vec![7, 1],
    });
    file.set_arena(&arena);
    let file = SueFile::from_bytes(&file.to_bytes()).unwrap();
    let blobs = store_len(&file);
    let mut smaller = file.arena(&registry()).unwrap();
    smaller.despawn(smaller.handle_of(gone).unwrap()).unwrap();
    let mut again = file.clone();
    again.set_arena(&smaller);
    assert!(store_len(&again) < blobs, "the deleted layer's blobs go");
    assert_eq!(again.chunk(*b"SCNE").unwrap().payload, [7, 1]);
    same(&smaller, &again.arena(&registry()).unwrap());
}
