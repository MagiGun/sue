//! The `.sue` project file: one container for everything a document holds.
//!
//! ```text
//! magic    8 bytes   "SUE\0FILE"
//! version  u32       container version (1)
//! extra    bytes     length-prefixed header extension (empty in version 1)
//! chunks   repeated  tag [u8; 4], version u32, length u64, payload, crc32 u32
//! end      chunk     tag "END\0", length 0
//! ```
//!
//! The chunk framing never changes. Each chunk carries its own version, so
//! a part can evolve without touching the others, and a reader keeps every
//! chunk it does not understand and writes it back unchanged. Built-in tags:
//! [`ARENA`] (entities and relations), [`META`] (document properties) and
//! [`PRESETS`] (a preset library). Features add their own, e.g. the shell's
//! `SCNE` (canvas layout).

use crate::codec::{CodecError, Reader, Writer, crc32};
use crate::{Arena, ComponentRegistry, PresetLibrary, Properties};

/// The first eight bytes of every `.sue` file.
pub const MAGIC: [u8; 8] = *b"SUE\0FILE";
/// The container version this build writes.
pub const CONTAINER_VERSION: u32 = 1;
/// Entities and relations.
pub const ARENA: [u8; 4] = *b"ARNA";
/// Document properties (title, author, created).
pub const META: [u8; 4] = *b"META";
/// A preset library.
pub const PRESETS: [u8; 4] = *b"PRST";
/// A small preview picture (PNG or JPEG XL bytes).
pub const THUMBNAIL: [u8; 4] = *b"THMB";
const END: [u8; 4] = *b"END\0";
const ARENA_VERSION: u32 = 1;

/// One tagged, versioned part of a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    /// Four-byte tag, e.g. `ARNA`.
    pub tag: [u8; 4],
    /// The payload's own version.
    pub version: u32,
    /// The payload.
    pub payload: Vec<u8>,
}

impl Chunk {
    /// The tag as text, for messages.
    #[must_use]
    pub fn tag_name(&self) -> String {
        String::from_utf8_lossy(&self.tag)
            .trim_end_matches('\0')
            .to_owned()
    }
}

/// A `.sue` file in memory: its chunks, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SueFile {
    chunks: Vec<Chunk>,
}

impl SueFile {
    /// An empty file.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `true` if `bytes` starts like a `.sue` file.
    #[must_use]
    pub fn sniff(bytes: &[u8]) -> bool {
        bytes.starts_with(&MAGIC)
    }

    /// The chunk tagged `tag`.
    #[must_use]
    pub fn chunk(&self, tag: [u8; 4]) -> Option<&Chunk> {
        self.chunks.iter().find(|chunk| chunk.tag == tag)
    }

    /// Every chunk.
    #[must_use]
    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    /// Adds a chunk, replacing one with the same tag in place.
    pub fn set(&mut self, chunk: Chunk) {
        match self.chunks.iter_mut().find(|old| old.tag == chunk.tag) {
            Some(old) => *old = chunk,
            None => self.chunks.push(chunk),
        }
    }

    /// Removes the chunk tagged `tag`.
    pub fn remove(&mut self, tag: [u8; 4]) -> Option<Chunk> {
        let index = self.chunks.iter().position(|chunk| chunk.tag == tag)?;
        Some(self.chunks.remove(index))
    }

    /// Stores `arena` in the [`ARENA`] chunk.
    pub fn set_arena(&mut self, arena: &Arena) {
        self.set_arena_where(arena, &|_| true);
    }

    /// Stores the entities of `arena` that `keep` accepts, with the
    /// relations between them.
    pub fn set_arena_where(&mut self, arena: &Arena, keep: &dyn Fn(&crate::Entity) -> bool) {
        let mut out = Writer::new();
        arena.encode_where(&mut out, keep);
        self.set(Chunk {
            tag: ARENA,
            version: ARENA_VERSION,
            payload: out.into_bytes(),
        });
    }

    /// Reads the [`ARENA`] chunk (an empty arena if there is none).
    ///
    /// # Errors
    /// If the chunk is malformed or written by a newer Sue.
    pub fn arena(&self, registry: &ComponentRegistry) -> Result<Arena, CodecError> {
        let Some(chunk) = self.chunk(ARENA) else {
            return Ok(Arena::new());
        };
        if chunk.version > ARENA_VERSION {
            return Err(CodecError::new(
                format!(
                    "the document was saved by a newer Sue (arena version {})",
                    chunk.version
                ),
                0,
            ));
        }
        let mut input = Reader::new(&chunk.payload);
        let arena = Arena::decode(&mut input, registry)?;
        if !input.is_empty() {
            return Err(input.error("unexpected bytes after the arena"));
        }
        Ok(arena)
    }

    /// Stores document properties in the [`META`] chunk.
    pub fn set_meta(&mut self, properties: &Properties) {
        let mut out = Writer::new();
        properties.encode(&mut out);
        self.set(Chunk {
            tag: META,
            version: 1,
            payload: out.into_bytes(),
        });
    }

    /// Reads the [`META`] chunk.
    ///
    /// # Errors
    /// If the chunk is malformed.
    pub fn meta(&self) -> Result<Properties, CodecError> {
        self.chunk(META).map_or(Ok(Properties::new()), |chunk| {
            Properties::decode(&mut Reader::new(&chunk.payload))
        })
    }

    /// Stores a preset library in the [`PRESETS`] chunk.
    pub fn set_presets(&mut self, presets: &PresetLibrary) {
        let mut out = Writer::new();
        presets.encode(&mut out);
        self.set(Chunk {
            tag: PRESETS,
            version: 1,
            payload: out.into_bytes(),
        });
    }

    /// Reads the [`PRESETS`] chunk.
    ///
    /// # Errors
    /// If the chunk is malformed.
    pub fn presets(&self) -> Result<PresetLibrary, CodecError> {
        self.chunk(PRESETS)
            .map_or(Ok(PresetLibrary::new()), |chunk| {
                PresetLibrary::decode(&mut Reader::new(&chunk.payload))
            })
    }

    /// The file's bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Writer::new();
        out.raw(&MAGIC);
        out.u32(CONTAINER_VERSION);
        out.bytes(&[]);
        for chunk in self.chunks.iter().filter(|chunk| chunk.tag != END) {
            out.raw(&chunk.tag);
            out.u32(chunk.version);
            out.u64(chunk.payload.len() as u64);
            out.raw(&chunk.payload);
            out.u32(crc32(&chunk.payload));
        }
        out.raw(&END);
        out.u32(0);
        out.u64(0);
        out.u32(crc32(&[]));
        out.into_bytes()
    }

    /// Reads a file.
    ///
    /// # Errors
    /// Not a `.sue` file, truncated, or a chunk whose checksum fails.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut input = Reader::new(bytes);
        if input.raw(MAGIC.len()).ok() != Some(&MAGIC[..]) {
            return Err(CodecError::new("not a Sue document", 0));
        }
        let _container_version = input.u32()?;
        let _extension = input.bytes()?;
        let mut chunks = Vec::new();
        loop {
            let mut tag = [0; 4];
            tag.copy_from_slice(input.raw(4)?);
            let version = input.u32()?;
            let len = usize::try_from(input.u64()?).map_err(|_| input.error("chunk too large"))?;
            let at = input.offset();
            let payload = input.raw(len)?;
            let checksum = input.u32()?;
            let chunk = Chunk {
                tag,
                version,
                payload: payload.to_vec(),
            };
            if crc32(payload) != checksum {
                return Err(CodecError::new(
                    format!("the {} part of the document is damaged", chunk.tag_name()),
                    at,
                ));
            }
            if tag == END {
                break;
            }
            chunks.push(chunk);
        }
        Ok(Self { chunks })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CHILD, Entity, Preset, Relation, StringBuffer};

    #[test]
    fn documents_round_trip_and_keep_unknown_chunks() {
        let mut arena = Arena::new();
        let folder = arena.insert(Entity::empty("Chapter 1", "book.chapter"));
        let page = arena.spawn("Page", StringBuffer::from("It was a dark night."));
        let (folder, page) = (
            arena.get(folder).unwrap().id(),
            arena.get(page).unwrap().id(),
        );
        arena.relate(Relation::new(folder, CHILD, page)).unwrap();

        let mut file = SueFile::new();
        file.set_arena(&arena);
        file.set_meta(&Properties::new().with("doc.title", "Novel"));
        let mut presets = PresetLibrary::new();
        presets.add(Preset::new(
            "Print",
            "export.pdf",
            Properties::new().with("dpi", 300),
        ));
        file.set_presets(&presets);
        file.set(Chunk {
            tag: *b"FUTR",
            version: 7,
            payload: vec![1, 2, 3],
        });
        let bytes = file.to_bytes();
        assert!(SueFile::sniff(&bytes));

        let back = SueFile::from_bytes(&bytes).unwrap();
        assert_eq!(back, file);
        let loaded = back.arena(&ComponentRegistry::new()).unwrap();
        assert_eq!(
            loaded.get_by_id(page).unwrap(),
            arena.get_by_id(page).unwrap()
        );
        assert_eq!(loaded.relations().children(folder), vec![page]);
        assert_eq!(back.meta().unwrap().str("doc.title"), Some("Novel"));
        assert_eq!(back.presets().unwrap(), presets);
        assert_eq!(back.chunk(*b"FUTR").unwrap().payload, [1, 2, 3]);
        assert_eq!(back.to_bytes(), bytes, "saving again is byte for byte");

        // Leaving an entity out drops its relations too.
        let mut subset = SueFile::new();
        subset.set_arena_where(&arena, &|entity| entity.id() != page);
        let partial = subset.arena(&ComponentRegistry::new()).unwrap();
        assert_eq!(partial.len(), 1);
        assert!(partial.relations().is_empty());
    }

    #[test]
    fn damage_and_strangers_are_reported() {
        let mut file = SueFile::new();
        file.set_arena(&Arena::new());
        let mut bytes = file.to_bytes();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0xFF;
        let error = SueFile::from_bytes(&bytes).unwrap_err();
        assert!(error.message.contains("damaged") || error.message.contains("bytes"));
        assert!(SueFile::from_bytes(b"PK\x03\x04").is_err());
        assert!(SueFile::from_bytes(&file.to_bytes()[..20]).is_err());
        file.set(Chunk {
            tag: ARENA,
            version: 99,
            payload: Vec::new(),
        });
        assert!(file.arena(&ComponentRegistry::new()).is_err());
        assert_eq!(
            SueFile::new()
                .arena(&ComponentRegistry::new())
                .unwrap()
                .len(),
            0
        );
    }
}
