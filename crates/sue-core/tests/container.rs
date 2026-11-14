//! The `.sue` container keeps what a later Sue wrote, and can be read and
//! written in pieces.
//!
//! Three things a build does not understand must survive a load and a
//! save (Rule 3): a container version and header extension of a later
//! version, bytes after the last field of an entity this build knows, and
//! a raster in a pixel format this build does not know. No later Sue
//! exists to write such files, so the tests here build them byte by byte
//! from the documented layout.
//!
//! The file can be looked into without loading it (`SueFile::scan`) and
//! written without building it in memory (`SueFile::write_to`); both are
//! held against the whole-file reader and writer, on the 0.0.75 golden
//! file among others (Rule 4: the old file still loads).

use std::io::{self, BufReader, Cursor, Write};

use sue_core::codec::{Reader, Writer, crc32};
use sue_core::file::{ARENA, CONTAINER_VERSION, Directory, MAGIC, META, THUMBNAIL};
use sue_core::txn::{AttachProjection, DespawnEntity, Rename, ReplaceComponent, SetProperty};
use sue_core::{
    Arena, Bundle, Chunk, ComponentKind, ComponentRegistry, Entity, EntityId, PixelFormat,
    Projection, Properties, RasterTile2D, RelationStore, StringBuffer, SueFile, TimeSpan,
    UndoStack,
};

/// The file 0.0.75 wrote (P0-4). Read, never rewritten.
const GOLDEN: &[u8] = include_bytes!("golden/kernel.sue");

fn registry() -> ComponentRegistry {
    ComponentRegistry::new()
}

/// One chunk as the file holds it: frame, payload, checksum.
fn framed(out: &mut Vec<u8>, tag: [u8; 4], version: u32, payload: &[u8]) {
    out.extend_from_slice(&tag);
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(payload);
    out.extend_from_slice(&crc32(payload).to_le_bytes());
}

/// A file as a Sue with container version `version` could write it.
fn file_bytes(version: u32, extension: &[u8], chunks: &[([u8; 4], u32, &[u8])]) -> Vec<u8> {
    let mut header = Writer::new();
    header.raw(&MAGIC);
    header.u32(version);
    header.bytes(extension);
    let mut out = header.into_bytes();
    for (tag, chunk_version, payload) in chunks {
        framed(&mut out, *tag, *chunk_version, payload);
    }
    framed(&mut out, *b"END\0", 0, &[]);
    out
}

/// An arena payload holding the given entity bodies and no relations.
fn arena_payload(bodies: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Writer::new();
    out.len(bodies.len());
    for body in bodies {
        out.bytes(body);
    }
    RelationStore::new().encode(&mut out);
    out.into_bytes()
}

fn load(payload: &[u8]) -> Arena {
    let mut input = Reader::new(payload);
    let arena = Arena::decode(&mut input, &registry()).expect("the arena loads");
    assert!(input.is_empty());
    arena
}

fn save(arena: &Arena) -> Vec<u8> {
    let mut out = Writer::new();
    arena.encode(&mut out);
    out.into_bytes()
}

/// A sample document with a few chunks, one of them unknown.
fn sample() -> SueFile {
    let mut arena = Arena::new();
    arena.spawn("Page", StringBuffer::from("It was a dark night."));
    arena.spawn(
        "Ink",
        RasterTile2D::from_bytes(2, 2, PixelFormat::Rgba8, (0..16).collect()).unwrap(),
    );
    let mut file = SueFile::new();
    file.set_arena(&arena);
    file.set_meta(&Properties::new().with("doc.title", "Novel"));
    file.set(Chunk {
        tag: *b"FUTR",
        version: 7,
        payload: vec![1, 2, 3],
    });
    file.set(Chunk {
        tag: *b"NULL",
        version: 0,
        payload: Vec::new(),
    });
    file
}

/// A writer that takes one byte per call and fails once `room` is used up.
struct Dribble {
    bytes: Vec<u8>,
    room: usize,
}

impl Write for Dribble {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.bytes.len() >= self.room {
            return Err(io::Error::other("the disk is full"));
        }
        match buffer.first() {
            Some(byte) => {
                self.bytes.push(*byte);
                Ok(1)
            }
            None => Ok(0),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// The header.

#[test]
fn a_future_container_version_and_header_extension_round_trip() {
    let meta = {
        let mut out = Writer::new();
        Properties::new()
            .with("doc.title", "From later")
            .encode(&mut out);
        out.into_bytes()
    };
    // A short extension, and one whose length needs a two-byte varint.
    for extension in [vec![0xCA, 0xFE, 0x00, 0x01], vec![0x5A; 300]] {
        let bytes = file_bytes(
            7,
            &extension,
            &[(META, 1, &meta[..]), (*b"FUTR", 3, &[9, 9][..])],
        );
        let mut file = SueFile::from_bytes(&bytes).expect("a later container still loads");
        assert_eq!(file.container_version(), 7);
        assert_eq!(file.header_extension(), &extension[..]);
        assert_eq!(file.meta().unwrap().str("doc.title"), Some("From later"));
        assert_eq!(file.to_bytes(), bytes, "saved again byte for byte");

        // The header survives a change of the document too.
        file.set_meta(&Properties::new().with("doc.title", "Edited here"));
        let edited = SueFile::from_bytes(&file.to_bytes()).unwrap();
        assert_eq!(edited.container_version(), 7);
        assert_eq!(edited.header_extension(), &extension[..]);
        assert_eq!(edited.meta().unwrap().str("doc.title"), Some("Edited here"));
        assert_eq!(edited.chunk(*b"FUTR").unwrap().payload, [9, 9]);

        // The directory reads the same header.
        let directory = SueFile::scan(&mut Cursor::new(&bytes)).unwrap();
        assert_eq!(directory.version, 7);
        assert_eq!(directory.extension, extension);
        assert_eq!(directory.chunks.len(), 2);
    }
    // A file that differs only in its header is a different file.
    assert_ne!(
        SueFile::from_bytes(&file_bytes(7, &[1], &[])).unwrap(),
        SueFile::from_bytes(&file_bytes(1, &[], &[])).unwrap()
    );
    assert_eq!(
        SueFile::from_bytes(&file_bytes(CONTAINER_VERSION, &[], &[])).unwrap(),
        SueFile::new()
    );
}

// The entity's tail.

#[test]
fn an_entity_keeps_the_tail_a_later_sue_wrote() {
    let known = Entity::new("Page", StringBuffer::from("text"))
        .with_meta("source", "typed")
        .with_property("page.number", 4);
    let id = known.id();
    let mut body = Writer::new();
    known.encode(&mut body);
    let plain = body.as_bytes().to_vec();
    let tail = [0xF0, 0x0D, 0x00, 0xFF, 0x80];
    body.raw(&tail);
    let payload = arena_payload(&[body.into_bytes()]);

    let mut arena = load(&payload);
    let entity = arena.get_by_id(id).unwrap();
    assert_eq!(entity.unread_tail(), tail);
    assert!(entity.holds_unread());
    // Everything this build knows is read as before.
    assert_eq!(entity.name(), "Page");
    assert_eq!(entity.string_buffer().unwrap().as_str(), "text");
    assert_eq!(entity.meta("source"), Some("typed"));
    assert_eq!(
        entity.property("page.number"),
        known.property("page.number")
    );
    assert_ne!(entity, &known, "the tail is part of what the entity holds");
    assert_eq!(save(&arena), payload, "saved again byte for byte");

    // Edits, undo and redo leave the tail alone (Rule 2).
    let before = arena.get_by_id(id).unwrap().clone();
    let mut undo = UndoStack::new();
    undo.apply(&mut arena, Rename::new(id, "Page 4")).unwrap();
    undo.apply(&mut arena, SetProperty::new(id, "page.number", 5))
        .unwrap();
    let after = arena.get_by_id(id).unwrap().clone();
    assert_eq!(after.unread_tail(), tail);
    assert_eq!(load(&save(&arena)).get_by_id(id).unwrap(), &after);
    assert!(undo.undo(&mut arena).unwrap());
    assert!(undo.undo(&mut arena).unwrap());
    assert_eq!(arena.get_by_id(id).unwrap(), &before);
    assert_eq!(save(&arena), payload);
    assert!(undo.redo(&mut arena).unwrap());
    assert!(undo.redo(&mut arena).unwrap());
    assert_eq!(arena.get_by_id(id).unwrap(), &after);

    // Deleting the entity and undoing that brings the tail back.
    undo.apply(&mut arena, DespawnEntity::new(id)).unwrap();
    assert!(arena.get_by_id(id).is_err());
    assert!(undo.undo(&mut arena).unwrap());
    assert_eq!(arena.get_by_id(id).unwrap(), &after);
    assert_eq!(arena.get_by_id(id).unwrap().unread_tail(), tail);

    // An entity of this build has no tail, and its bytes did not change.
    let arena = load(&arena_payload(std::slice::from_ref(&plain)));
    let entity = arena.get_by_id(id).unwrap();
    assert!(entity.unread_tail().is_empty());
    assert!(!entity.holds_unread());
    assert_eq!(entity, &known);
    let mut again = Writer::new();
    entity.encode(&mut again);
    assert_eq!(again.into_bytes(), plain);
}

// The unknown pixel format.

/// A raster payload in pixel format `code`, which no build knows: width,
/// height, the code, then whatever that format holds.
fn strange_raster(code: u8) -> Vec<u8> {
    let mut out = Writer::new();
    out.u32(640);
    out.u32(480);
    out.u8(code);
    out.raw(b"tiles by reference, or sixteen bits, or anything");
    out.into_bytes()
}

/// An entity body as a later Sue could write it: `components` are (kind
/// name, payload) pairs in file order.
fn entity_body(id: EntityId, class: &str, base: &str, components: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = Writer::new();
    out.raw(&id.to_bytes());
    out.str("Layer 1");
    out.str(class);
    out.str(base);
    out.len(components.len());
    for (name, payload) in components {
        out.str(name);
        out.bytes(payload);
    }
    out.len(0);
    Properties::new()
        .with("layer.opacity", 0.5)
        .encode(&mut out);
    out.len(0);
    out.into_bytes()
}

fn encoded(component: impl Into<sue_core::ComponentData>) -> Vec<u8> {
    let mut out = Writer::new();
    component.into().encode(&mut out);
    out.into_bytes()
}

#[test]
fn an_unknown_pixel_format_is_kept_and_not_shown() {
    let id = EntityId::new();
    let raster = strange_raster(9);
    let payload = arena_payload(&[entity_body(
        id,
        "sue.image",
        "RasterTile2D",
        &[("RasterTile2D", raster.clone())],
    )]);

    let mut arena = load(&payload);
    let entity = arena.get_by_id(id).unwrap();
    // The file loads and the rest of the layer is there.
    assert_eq!(entity.name(), "Layer 1");
    assert_eq!(entity.class(), "sue.image");
    assert!(entity.property("layer.opacity").is_some());
    // Nothing can show or edit pixels this build cannot read.
    assert!(entity.component(ComponentKind::RasterTile2D).is_none());
    assert!(!entity.projections().contains(Projection::Raster2D));
    assert_eq!(entity.base_kind(), None);
    assert_eq!(arena.with_projection(Projection::Raster2D).count(), 0);
    // But they are kept as they were written.
    assert_eq!(
        entity.unread_component(ComponentKind::RasterTile2D),
        Some(&raster[..])
    );
    assert!(
        entity
            .unread_component(ComponentKind::StringBuffer)
            .is_none()
    );
    assert!(entity.holds_unread());
    assert_eq!(save(&arena), payload, "saved again byte for byte");

    // Through a whole file as well.
    let bytes = file_bytes(1, &[], &[(ARENA, 1, &payload[..])]);
    let file = SueFile::from_bytes(&bytes).unwrap();
    let from_file = file.arena(&registry()).unwrap();
    assert_eq!(from_file.get_by_id(id).unwrap(), entity);
    let mut again = SueFile::new();
    again.set_arena(&from_file);
    assert_eq!(again.to_bytes(), bytes);

    // Edits, undo and redo keep the pixels (Rule 2).
    let before = entity.clone();
    let mut undo = UndoStack::new();
    undo.apply(&mut arena, Rename::new(id, "Sketch")).unwrap();
    let after = arena.get_by_id(id).unwrap().clone();
    assert_eq!(
        after.unread_component(ComponentKind::RasterTile2D),
        Some(&raster[..])
    );
    assert_eq!(load(&save(&arena)).get_by_id(id).unwrap(), &after);
    assert!(undo.undo(&mut arena).unwrap());
    assert_eq!(arena.get_by_id(id).unwrap(), &before);
    assert_eq!(save(&arena), payload);
    assert!(undo.redo(&mut arena).unwrap());
    assert_eq!(arena.get_by_id(id).unwrap(), &after);
    undo.apply(&mut arena, DespawnEntity::new(id)).unwrap();
    assert!(undo.undo(&mut arena).unwrap());
    assert_eq!(arena.get_by_id(id).unwrap(), &after);
}

#[test]
fn an_unread_raster_stays_in_its_place_among_the_other_components() {
    // Kinds are written in their fixed order: text before a raster, a
    // time span after it.
    let id = EntityId::new();
    let raster = strange_raster(200);
    let payload = arena_payload(&[entity_body(
        id,
        "clip.video",
        "StringBuffer",
        &[
            ("StringBuffer", encoded(StringBuffer::from("caption"))),
            ("RasterTile2D", raster.clone()),
            ("TimeSpan", encoded(TimeSpan::new(0, 100).unwrap())),
        ],
    )]);
    let mut arena = load(&payload);
    let entity = arena.get_by_id(id).unwrap();
    assert_eq!(entity.base_kind(), Some(ComponentKind::StringBuffer));
    assert_eq!(entity.string_buffer().unwrap().as_str(), "caption");
    assert!(entity.component(ComponentKind::TimeSpan).is_some());
    assert!(entity.component(ComponentKind::RasterTile2D).is_none());
    assert_eq!(
        entity.unread_component(ComponentKind::RasterTile2D),
        Some(&raster[..])
    );
    assert_eq!(save(&arena), payload, "saved again byte for byte");

    // Pixels this build gives the entity take the place of the unread
    // ones, and undo brings the unread ones back.
    let before = entity.clone();
    let tile = RasterTile2D::new(2, 2, PixelFormat::Gray8).unwrap();
    let mut undo = UndoStack::new();
    let handle = arena.handle_of(id).unwrap();
    undo.apply(
        &mut arena,
        AttachProjection::new(handle, Projection::Raster2D, tile.clone()),
    )
    .unwrap();
    let entity = arena.get_by_id(id).unwrap();
    assert_eq!(
        entity
            .component(ComponentKind::RasterTile2D)
            .and_then(|data| data.as_raster()),
        Some(&tile)
    );
    assert!(
        entity
            .unread_component(ComponentKind::RasterTile2D)
            .is_none()
    );
    assert!(!entity.holds_unread());
    let saved = load(&save(&arena));
    let reloaded = saved.get_by_id(id).unwrap();
    assert_eq!(
        reloaded
            .component(ComponentKind::RasterTile2D)
            .and_then(|data| data.as_raster()),
        Some(&tile)
    );
    assert!(!reloaded.holds_unread());
    assert!(undo.undo(&mut arena).unwrap());
    assert_eq!(arena.get_by_id(id).unwrap(), &before);
    assert_eq!(save(&arena), payload);
}

#[test]
fn what_is_unread_survives_the_clipboard_and_a_copy_to_another_document() {
    // One entity with all of it at once: an unread raster as its base, a
    // readable component, an extension this build does not know, a tail.
    let id = EntityId::new();
    let raster = strange_raster(77);
    let tail = [0xAB, 0x00, 0xCD];
    let mut body = Writer::new();
    body.raw(&id.to_bytes());
    body.str("Layer 1");
    body.str("sue.image");
    body.str("RasterTile2D");
    body.len(2);
    body.str("RasterTile2D");
    body.bytes(&raster);
    body.str("TimeSpan");
    body.bytes(&encoded(TimeSpan::new(0, 100).unwrap()));
    body.len(0);
    Properties::new().encode(&mut body);
    body.len(1);
    body.str("later.effect");
    body.varint(3);
    body.bytes(&[4, 5, 6]);
    body.raw(&tail);
    let body = body.into_bytes();
    let payload = arena_payload(std::slice::from_ref(&body));
    let arena = load(&payload);
    let entity = arena.get_by_id(id).unwrap();
    assert_eq!(
        entity.unread_component(ComponentKind::RasterTile2D),
        Some(&raster[..])
    );
    assert_eq!(entity.unread_tail(), tail);
    assert!(entity.ext_named("later.effect").is_some());
    assert_eq!(save(&arena), payload, "saved again byte for byte");

    // The clipboard format is the second reader of an entity.
    let bundle = Bundle::copy_from(&arena, &[id], true).unwrap();
    let mut out = Writer::new();
    bundle.encode(&mut out);
    let bytes = out.into_bytes();
    let mut input = Reader::new(&bytes);
    let pasted = Bundle::decode(&mut input, &registry()).unwrap();
    assert!(input.is_empty());
    assert_eq!(pasted.entities.len(), 1);
    assert_eq!(&pasted.entities[0], entity);

    // Into another document, and saved from there.
    let mut other = Arena::new();
    for entity in pasted.entities {
        other.insert_keep_id(entity).unwrap();
    }
    assert_eq!(save(&other), payload);

    // A copy under a new id keeps the bytes too; only the id differs.
    let mut copy = Bundle::copy_from(&arena, &[id], true).unwrap();
    let new_id = copy.renew_ids()[&id];
    let mut renamed = Writer::new();
    copy.entities[0].encode(&mut renamed);
    let renamed = renamed.into_bytes();
    assert_eq!(&renamed[..16], &new_id.to_bytes()[..]);
    assert_eq!(&renamed[16..], &body[16..]);
}

#[test]
fn the_known_pixel_formats_are_not_taken_for_unknown_ones() {
    for format in [PixelFormat::Gray8, PixelFormat::Rgba8, PixelFormat::RgbaF32] {
        assert_eq!(PixelFormat::from_code(format.code()), Some(format));
        let tile = RasterTile2D::new(3, 2, format).unwrap();
        let entity = Entity::new("tile", tile.clone());
        let mut body = Writer::new();
        entity.encode(&mut body);
        let arena = load(&arena_payload(&[body.into_bytes()]));
        let back = arena.get_by_id(entity.id()).unwrap();
        assert_eq!(back, &entity);
        assert!(!back.holds_unread());
        assert!(back.projections().contains(Projection::Raster2D));
    }
    // The codes 0.0.75 wrote.
    assert_eq!(PixelFormat::Gray8.code(), 0);
    assert_eq!(PixelFormat::Rgba8.code(), 1);
    assert_eq!(PixelFormat::RgbaF32.code(), 2);
    assert_eq!(PixelFormat::from_code(3), None);

    // A known format with a damaged payload is still an error, and so is
    // a raster cut off before its format code: only a format this build
    // does not know is kept unread.
    let id = EntityId::new();
    let mut short = Writer::new();
    short.u32(4);
    short.u32(4);
    short.u8(PixelFormat::Rgba8.code());
    short.bytes(&[0; 7]);
    for payload in [short.into_bytes(), vec![1, 0, 0, 0, 1, 0]] {
        let body = entity_body(
            id,
            "sue.image",
            "RasterTile2D",
            &[("RasterTile2D", payload)],
        );
        let bytes = arena_payload(&[body]);
        assert!(Arena::decode(&mut Reader::new(&bytes), &registry()).is_err());
    }
    // Replacing a component goes on as before for a readable raster.
    let mut arena = Arena::new();
    let handle = arena.spawn("tile", RasterTile2D::new(1, 1, PixelFormat::Gray8).unwrap());
    let id = arena.get(handle).unwrap().id();
    let mut undo = UndoStack::new();
    let wide = RasterTile2D::new(2, 1, PixelFormat::Gray8).unwrap();
    undo.apply(&mut arena, ReplaceComponent::new(id, wide, "resize"))
        .unwrap();
    assert!(undo.undo(&mut arena).unwrap());
    assert!(!arena.get_by_id(id).unwrap().holds_unread());
}

// Truncation.

#[test]
fn a_truncated_file_is_an_error_for_both_readers() {
    let later = file_bytes(3, &[7; 200], &[(*b"FUTR", 1, &[5; 40][..])]);
    for bytes in [GOLDEN.to_vec(), sample().to_bytes(), later] {
        assert!(SueFile::from_bytes(&bytes).is_ok());
        assert!(SueFile::scan(&mut Cursor::new(&bytes)).is_ok());
        for cut in 0..bytes.len() {
            let part = &bytes[..cut];
            assert!(
                SueFile::from_bytes(part).is_err(),
                "a file cut at byte {cut} of {} must not load",
                bytes.len()
            );
            assert!(
                SueFile::scan(&mut Cursor::new(part)).is_err(),
                "a file cut at byte {cut} of {} must not scan",
                bytes.len()
            );
        }
    }
    // Lengths that reach past the end ask for no memory and do not panic.
    let mut huge = Vec::new();
    huge.extend_from_slice(&MAGIC);
    huge.extend_from_slice(&1_u32.to_le_bytes());
    huge.push(0);
    huge.extend_from_slice(b"ARNA");
    huge.extend_from_slice(&1_u32.to_le_bytes());
    huge.extend_from_slice(&u64::MAX.to_le_bytes());
    huge.extend_from_slice(&[0; 64]);
    assert!(SueFile::from_bytes(&huge).is_err());
    assert!(SueFile::scan(&mut Cursor::new(&huge)).is_err());
    let mut long_extension = Vec::new();
    long_extension.extend_from_slice(&MAGIC);
    long_extension.extend_from_slice(&1_u32.to_le_bytes());
    long_extension.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
    assert!(SueFile::from_bytes(&long_extension).is_err());
    assert!(SueFile::scan(&mut Cursor::new(&long_extension)).is_err());
    let mut endless_varint = Vec::new();
    endless_varint.extend_from_slice(&MAGIC);
    endless_varint.extend_from_slice(&1_u32.to_le_bytes());
    endless_varint.extend_from_slice(&[0x80; 32]);
    assert!(SueFile::from_bytes(&endless_varint).is_err());
    assert!(SueFile::scan(&mut Cursor::new(&endless_varint)).is_err());
    assert!(SueFile::scan(&mut Cursor::new(b"PK\x03\x04")).is_err());
    assert!(SueFile::scan(&mut Cursor::new(b"")).is_err());
}

// The directory.

/// `scan` must tell exactly what `from_bytes` reads.
fn assert_directory_matches(bytes: &[u8]) {
    let file = SueFile::from_bytes(bytes).unwrap();
    let mut input = Cursor::new(bytes);
    let directory = SueFile::scan(&mut input).unwrap();
    assert_eq!(directory.version, file.container_version());
    assert_eq!(directory.extension, file.header_extension());
    assert_eq!(directory.chunks.len(), file.chunks().len());
    for (entry, chunk) in directory.chunks.iter().zip(file.chunks()) {
        assert_eq!(entry.tag, chunk.tag);
        assert_eq!(entry.tag_name(), chunk.tag_name());
        assert_eq!(entry.version, chunk.version);
        assert_eq!(entry.len, chunk.payload.len() as u64);
        assert_eq!(entry.checksum, crc32(&chunk.payload));
        let start = usize::try_from(entry.offset).unwrap();
        assert_eq!(
            &bytes[start..start + chunk.payload.len()],
            &chunk.payload[..]
        );
    }
    // Chunks can be fetched one at a time, in any order.
    for (entry, chunk) in directory.chunks.iter().zip(file.chunks()).rev() {
        assert_eq!(&Directory::read(&mut input, entry).unwrap(), chunk);
    }
    for chunk in file.chunks() {
        let first = file.chunk(chunk.tag).unwrap();
        let entry = directory.chunk(chunk.tag).unwrap();
        assert_eq!(&Directory::read(&mut input, entry).unwrap(), first);
    }
    assert!(directory.chunk(*b"NONE").is_none());
}

#[test]
fn scan_returns_the_chunk_directory() {
    assert_directory_matches(GOLDEN);
    assert_directory_matches(&sample().to_bytes());
    assert_directory_matches(&SueFile::new().to_bytes());
    assert_directory_matches(&file_bytes(9, &[3; 130], &[(*b"FUTR", 2, &[1][..])]));

    // The golden file: the parts 0.0.75 wrote, where it wrote them.
    let directory = SueFile::scan(&mut Cursor::new(GOLDEN)).unwrap();
    assert_eq!(directory.version, 1);
    assert!(directory.extension.is_empty());
    for tag in [ARENA, META, THUMBNAIL] {
        assert!(directory.chunk(tag).is_some());
    }
    // The first payload starts after the 13 byte header and one frame.
    assert_eq!(directory.chunks[0].offset, 13 + 16);
    let last = directory.chunks.last().unwrap();
    // After the last payload: its checksum, then the 20 byte end mark.
    assert_eq!(last.offset + last.len + 4 + 20, GOLDEN.len() as u64);
}

#[test]
fn a_scan_reads_no_payload_and_a_read_checks_the_one_it_fetches() {
    // Damage inside a payload: the whole-file reader refuses the file, a
    // scan does not look there, and reading that chunk reports it while
    // the other chunks still read.
    let file = sample();
    let mut bytes = file.to_bytes();
    let directory = SueFile::scan(&mut Cursor::new(&bytes)).unwrap();
    let arena = *directory.chunk(ARENA).unwrap();
    let meta = *directory.chunk(META).unwrap();
    bytes[usize::try_from(arena.offset).unwrap() + 3] ^= 0x55;
    assert!(SueFile::from_bytes(&bytes).is_err());
    let mut input = Cursor::new(&bytes);
    assert_eq!(SueFile::scan(&mut input).unwrap(), directory);
    let error = Directory::read(&mut input, &arena).unwrap_err();
    assert!(error.message.contains("ARNA"), "{error}");
    assert_eq!(
        &Directory::read(&mut input, &meta).unwrap(),
        file.chunk(META).unwrap()
    );
    // An entry that points past the end is an error, not a huge buffer.
    let wrong = sue_core::file::ChunkEntry {
        len: u64::MAX,
        ..meta
    };
    assert!(Directory::read(&mut input, &wrong).is_err());
}

#[test]
fn a_file_on_disk_is_scanned_through_a_buffered_reader() {
    let file = sample();
    let bytes = file.to_bytes();
    let path = std::env::temp_dir().join(format!("sue-p1-7-scan-{}.sue", std::process::id()));
    {
        let mut out = io::BufWriter::new(std::fs::File::create(&path).unwrap());
        file.write_to(&mut out).unwrap();
        out.flush().unwrap();
    }
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let mut input = BufReader::new(std::fs::File::open(&path).unwrap());
    let directory = SueFile::scan(&mut input).unwrap();
    let meta = Directory::read(&mut input, directory.chunk(META).unwrap()).unwrap();
    assert_eq!(&meta, file.chunk(META).unwrap());
    drop(input);
    std::fs::remove_file(&path).unwrap();
}

// Writing.

#[test]
fn write_to_gives_the_bytes_of_to_bytes() {
    let later = SueFile::from_bytes(&file_bytes(4, &[8; 129], &[(*b"FUTR", 1, &[5; 40][..])]));
    let golden = SueFile::from_bytes(GOLDEN);
    for file in [sample(), SueFile::new(), later.unwrap(), golden.unwrap()] {
        let bytes = file.to_bytes();
        assert_eq!(file.encoded_len(), bytes.len());
        let mut whole = Vec::new();
        file.write_to(&mut whole).unwrap();
        assert_eq!(whole, bytes);
        // A writer that takes a byte at a time gets the same file.
        let mut slow = Dribble {
            bytes: Vec::new(),
            room: usize::MAX,
        };
        file.write_to(&mut slow).unwrap();
        assert_eq!(slow.bytes, bytes);
    }
}

#[test]
fn a_writer_that_fails_stops_the_save_with_its_error() {
    // At every byte offset in turn: the error comes back, and what was
    // written until then is the start of the file and never loads as one.
    let file = sample();
    let bytes = file.to_bytes();
    for room in 0..bytes.len() {
        let mut out = Dribble {
            bytes: Vec::new(),
            room,
        };
        let error = file.write_to(&mut out).unwrap_err();
        assert_eq!(error.to_string(), "the disk is full");
        assert_eq!(out.bytes, &bytes[..room]);
        assert!(SueFile::from_bytes(&out.bytes).is_err());
    }
}

// The old file.

#[test]
fn the_golden_file_still_loads_and_holds_nothing_unread() {
    let file = SueFile::from_bytes(GOLDEN).expect("the 0.0.75 file loads");
    assert_eq!(file.container_version(), 1);
    assert!(file.header_extension().is_empty());
    assert_eq!(file.to_bytes(), GOLDEN, "saved again byte for byte");
    // Its extension component is read as opaque here, which is enough to
    // see that every entity decodes to its end.
    let arena = file.arena(&registry()).expect("its arena loads");
    assert!(!arena.is_empty());
    for (_, entity) in arena.iter() {
        assert!(!entity.holds_unread(), "{} has unread bytes", entity.name());
        assert!(entity.unread_tail().is_empty());
        for kind in ComponentKind::ALL {
            assert!(entity.unread_component(kind).is_none());
        }
    }
    // A save puts the rasters into the pixel store (arena version 2); the
    // copy for older Sue encodes the arena as 0.0.75 did, byte for byte.
    let mut again = SueFile::from_bytes(GOLDEN).unwrap();
    again.set_arena(&arena);
    assert_ne!(again.to_bytes(), GOLDEN);
    let (older, left_out) = again.copy_for_older_sue(&arena);
    assert!(left_out.is_empty());
    assert_eq!(older.to_bytes(), GOLDEN, "the arena encodes as 0.0.75 did");
}
