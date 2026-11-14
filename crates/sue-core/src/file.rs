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
//! chunk it does not understand and writes it back unchanged. The same
//! holds for the header: a file of a later container version keeps its
//! version number and its header extension when this build saves it again.
//! Built-in tags: [`ARENA`] (entities and relations), [`META`] (document
//! properties) and [`PRESETS`] (a preset library). Features add their own,
//! e.g. the shell's `SCNE` (canvas layout).
//!
//! A file can be read whole ([`SueFile::from_bytes`]) or looked into
//! without loading it: [`SueFile::scan`] reads only the header and the
//! frame of each chunk and returns a [`Directory`] that says where every
//! payload lies, and [`Directory::read`] then fetches the one chunk that
//! is wanted. [`SueFile::write_to`] writes a file piece by piece into any
//! [`Write`], so saving needs no second copy of the document in memory.

use std::convert::Infallible;
use std::io::{self, Read, Seek, SeekFrom, Write};

use crate::codec::{CodecError, Reader, Writer, crc32};
use crate::pixels::{
    KEY_WIDTH, PixelOptions, PixelReader, PixelWriter, SaveCache, SaveStats, StoreHeader,
    VERSION as PIXELS_VERSION, may_refer_to_blobs,
};
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
/// The pixel store: the tiles of the rasters an [`ARENA`] of version 2
/// writes by reference ([`crate::pixels`]).
pub const PIXELS: [u8; 4] = *b"PIXL";
const END: [u8; 4] = *b"END\0";
/// The arena version whose rasters may go by reference into [`PIXELS`].
const ARENA_VERSION: u32 = 2;
/// The arena version of 0.0.75 and older: every raster inline.
const ARENA_VERSION_INLINE: u32 = 1;
/// The tags this build knows. Any other chunk may point at blobs of the
/// pixel store, so they are kept (Rule 3). A chunk a feature of this build
/// adds must be named here, or no save drops a blob again.
const KNOWN: [[u8; 4]; 5] = [ARENA, META, PRESETS, THUMBNAIL, PIXELS];
/// Chunks only builds older than the pixel store wrote (the canvas layout
/// of the builds before the Hub). This build does not read them and keeps
/// them, but they cannot point at a blob, so they keep no blob alive.
const OLDER_THAN_PIXELS: [[u8; 4]; 1] = [*b"SCNE"];
/// Tag, version and length: what stands before a chunk's payload.
const FRAME: usize = 16;

fn tag_name(tag: [u8; 4]) -> String {
    String::from_utf8_lossy(&tag)
        .trim_end_matches('\0')
        .to_owned()
}

fn damaged(tag: [u8; 4], offset: usize) -> CodecError {
    CodecError::new(
        format!("the {} part of the document is damaged", tag_name(tag)),
        offset,
    )
}

fn newer(part: &str, version: u32) -> CodecError {
    CodecError::new(
        format!("the document was saved by a newer Sue ({part} version {version})"),
        0,
    )
}

fn unreadable(error: &io::Error, offset: usize) -> CodecError {
    CodecError::new(format!("the document cannot be read: {error}"), offset)
}

/// A file offset as the `usize` a [`CodecError`] carries.
fn offset_of(at: u64) -> usize {
    usize::try_from(at).unwrap_or(usize::MAX)
}

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
        tag_name(self.tag)
    }
}

/// Where one chunk lies in a file, from [`SueFile::scan`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkEntry {
    /// Four-byte tag, e.g. `ARNA`.
    pub tag: [u8; 4],
    /// The payload's own version.
    pub version: u32,
    /// Where the payload starts, in bytes from the start of the file.
    pub offset: u64,
    /// The payload's length in bytes.
    pub len: u64,
    /// The CRC-32 the file holds for the payload. A scan does not read the
    /// payload, so it has not checked this; [`Directory::read`] does.
    pub checksum: u32,
}

impl ChunkEntry {
    /// The tag as text, for messages.
    #[must_use]
    pub fn tag_name(&self) -> String {
        tag_name(self.tag)
    }
}

/// The header and the chunk list of a file, read without its payloads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Directory {
    /// The container version the file was written with.
    pub version: u32,
    /// The header extension (empty in version 1).
    pub extension: Vec<u8>,
    /// Every chunk in file order, without the end mark.
    pub chunks: Vec<ChunkEntry>,
}

impl Directory {
    /// The first chunk tagged `tag`.
    #[must_use]
    pub fn chunk(&self, tag: [u8; 4]) -> Option<&ChunkEntry> {
        self.chunks.iter().find(|chunk| chunk.tag == tag)
    }

    /// Reads the payload of `entry` from the file it was scanned from and
    /// checks it against its checksum.
    ///
    /// # Errors
    /// If the file cannot be read, ends early, or the payload is damaged.
    pub fn read<R: Read + Seek>(input: &mut R, entry: &ChunkEntry) -> Result<Chunk, CodecError> {
        let at = offset_of(entry.offset);
        input
            .seek(SeekFrom::Start(entry.offset))
            .map_err(|error| unreadable(&error, at))?;
        let mut payload = Vec::new();
        // `take` bounds the read, so a wrong length cannot ask for more
        // memory than the file has bytes.
        input
            .by_ref()
            .take(entry.len)
            .read_to_end(&mut payload)
            .map_err(|error| unreadable(&error, at))?;
        if payload.len() as u64 != entry.len {
            return Err(CodecError::new(
                format!("needs {} bytes, {} left", entry.len, payload.len()),
                at,
            ));
        }
        if crc32(&payload) != entry.checksum {
            return Err(damaged(entry.tag, at));
        }
        Ok(Chunk {
            tag: entry.tag,
            version: entry.version,
            payload,
        })
    }
}

/// Reads from a stream whose length is known, counting the offset, so a
/// file that ends early is reported the way [`Reader`] reports it.
struct Scanner<'a, R> {
    input: &'a mut R,
    at: u64,
    total: u64,
}

impl<R: Read + Seek> Scanner<'_, R> {
    fn left(&self) -> u64 {
        self.total - self.at
    }

    fn error(&self, message: impl Into<String>) -> CodecError {
        CodecError::new(message, offset_of(self.at))
    }

    fn fill(&mut self, buffer: &mut [u8]) -> Result<(), CodecError> {
        if buffer.len() as u64 > self.left() {
            return Err(self.error(format!(
                "needs {} bytes, {} left",
                buffer.len(),
                self.left()
            )));
        }
        self.input
            .read_exact(buffer)
            .map_err(|error| unreadable(&error, offset_of(self.at)))?;
        self.at += buffer.len() as u64;
        Ok(())
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], CodecError> {
        let mut out = [0; N];
        self.fill(&mut out)?;
        Ok(out)
    }

    fn varint(&mut self) -> Result<u64, CodecError> {
        let mut value = 0_u64;
        for shift in (0..64).step_by(7) {
            let [byte] = self.array()?;
            value |= u64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(self.error("varint overflows 64 bits"))
    }

    fn skip(&mut self, len: u64) -> Result<(), CodecError> {
        if len > self.left() {
            return Err(self.error(format!("needs {len} bytes, {} left", self.left())));
        }
        self.at += len;
        self.input
            .seek(SeekFrom::Start(self.at))
            .map_err(|error| unreadable(&error, offset_of(self.at)))?;
        Ok(())
    }
}

/// A `.sue` file in memory: its header and its chunks, in order.
///
/// Saving into the same `SueFile` again is incremental: it remembers how
/// it wrote each tile of the pixel store ([`SaveCache`]), so a tile that
/// did not change is neither read, hashed nor encoded again, and a tile
/// whose pixels the store already holds is copied as it is encoded. What
/// it remembers is not part of the file and plays no part in `==`.
#[derive(Clone, Debug)]
pub struct SueFile {
    version: u32,
    extension: Vec<u8>,
    chunks: Vec<Chunk>,
    /// How the last [`Self::set_arena_with`] wrote each tile of the
    /// [`PIXELS`] chunk it made; empty when that chunk came from anywhere
    /// else.
    cache: SaveCache,
    stats: SaveStats,
}

impl PartialEq for SueFile {
    /// Same header and chunks.
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version
            && self.extension == other.extension
            && self.chunks == other.chunks
    }
}

impl Eq for SueFile {}

impl Default for SueFile {
    fn default() -> Self {
        Self {
            version: CONTAINER_VERSION,
            extension: Vec::new(),
            chunks: Vec::new(),
            cache: SaveCache::default(),
            stats: SaveStats::default(),
        }
    }
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

    /// The container version: [`CONTAINER_VERSION`] for a new file, and
    /// for a file that was read the version it was written with, which
    /// saving keeps.
    #[must_use]
    pub fn container_version(&self) -> u32 {
        self.version
    }

    /// The header extension: empty in version 1. What a later version put
    /// there is kept and written back.
    #[must_use]
    pub fn header_extension(&self) -> &[u8] {
        &self.extension
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
        if chunk.tag == PIXELS {
            self.cache = SaveCache::default();
        }
        match self.chunks.iter_mut().find(|old| old.tag == chunk.tag) {
            Some(old) => *old = chunk,
            None => self.chunks.push(chunk),
        }
    }

    /// Removes the chunk tagged `tag`.
    pub fn remove(&mut self, tag: [u8; 4]) -> Option<Chunk> {
        if tag == PIXELS {
            self.cache = SaveCache::default();
        }
        let index = self.chunks.iter().position(|chunk| chunk.tag == tag)?;
        Some(self.chunks.remove(index))
    }

    /// Stores `arena` in the [`ARENA`] chunk, its rasters as tiles in the
    /// [`PIXELS`] chunk ([`PixelOptions::SAVE`]).
    pub fn set_arena(&mut self, arena: &Arena) {
        self.set_arena_where(arena, &|_| true);
    }

    /// Stores the entities of `arena` that `keep` accepts, with the
    /// relations between them.
    pub fn set_arena_where(&mut self, arena: &Arena, keep: &dyn Fn(&crate::Entity) -> bool) {
        self.set_arena_with(arena, keep, &PixelOptions::SAVE);
    }

    /// As [`Self::set_arena_where`], with the pixels written as `options`
    /// say.
    ///
    /// The arena is version 2 when a raster went into the pixel store and
    /// version 1, as 0.0.75 wrote it, when none did (a document without
    /// pictures). The store holds each distinct tile once. A blob of the
    /// store this file was read with is written back, even when no raster
    /// points at it any more, whenever something this build cannot read
    /// may point at it: a chunk it does not know, or an entity holding an
    /// unread component, a tail, an opaque extension component or an
    /// opaque value (Rule 3). Otherwise only the blobs the rasters use are
    /// written.
    ///
    /// It is incremental (see the type): a tile is read, hashed and
    /// encoded only when it changed since this file was last saved, and a
    /// blob of the store this file holds is copied, still encoded, for
    /// every tile that has its pixels. The bytes equal a save into a new
    /// file whenever the old blobs were written at the same deflate level.
    /// [`Self::last_save`] says what it did.
    pub fn set_arena_with(
        &mut self,
        arena: &Arena,
        keep: &dyn Fn(&crate::Entity) -> bool,
        options: &PixelOptions,
    ) {
        let old = self
            .chunks
            .iter_mut()
            .find(|chunk| chunk.tag == PIXELS)
            .map(|chunk| (chunk.version, std::mem::take(&mut chunk.payload)));
        let unknown =
            self.chunks.iter().any(|chunk| {
                !KNOWN.contains(&chunk.tag) && !OLDER_THAN_PIXELS.contains(&chunk.tag)
            }) || arena
                .iter()
                .any(|(_, entity)| keep(entity) && may_refer_to_blobs(entity));
        let mut out = Writer::new();
        let mut next = None;
        let (arena_version, store) = match &old {
            Some((version, payload)) if unknown => {
                // New tiles share the old store's keys, so its keys must be
                // wide enough to name every tile of this save (D12: at
                // least 128 bits); a narrower store is kept as it is.
                let reader = PixelReader::parse(payload).ok().filter(|reader| {
                    *version == PIXELS_VERSION && reader.header().key_width >= KEY_WIDTH
                });
                if let Some(reader) = reader {
                    let mut pixels = PixelWriter::keeping(&reader, Some(&self.cache), options);
                    arena.encode_where_with(&mut out, keep, Some(&mut pixels));
                    let (store, cache, stats) = pixels.finish_saving();
                    next = Some((cache, stats));
                    (
                        ARENA_VERSION,
                        store.map(|payload| (PIXELS_VERSION, payload)),
                    )
                } else {
                    // A store this build cannot read stays as it was, and
                    // the rasters go inline beside it.
                    arena.encode_where(&mut out, keep);
                    (ARENA_VERSION_INLINE, Some((*version, payload.clone())))
                }
            }
            _ => {
                // The store this file holds lends its encoded blobs to
                // the tiles that did not change.
                let reader = old
                    .as_ref()
                    .filter(|(version, _)| *version == PIXELS_VERSION)
                    .and_then(|(_, payload)| PixelReader::parse(payload).ok());
                let mut pixels = match &reader {
                    Some(reader) => PixelWriter::reusing(reader, Some(&self.cache), options),
                    None => PixelWriter::new(options),
                };
                arena.encode_where_with(&mut out, keep, Some(&mut pixels));
                let (store, cache, stats) = pixels.finish_saving();
                next = Some((cache, stats));
                match store {
                    Some(payload) => (ARENA_VERSION, Some((PIXELS_VERSION, payload))),
                    None => (ARENA_VERSION_INLINE, None),
                }
            }
        };
        self.set(Chunk {
            tag: ARENA,
            version: arena_version,
            payload: out.into_bytes(),
        });
        match store {
            Some((version, payload)) => self.set(Chunk {
                tag: PIXELS,
                version,
                payload,
            }),
            None => {
                self.remove(PIXELS);
            }
        }
        // Set after the chunk, which forgets what was remembered.
        let (cache, stats) = next.unwrap_or_default();
        self.cache = cache;
        self.stats = stats;
    }

    /// A file with this one's container version and header extension, and
    /// `chunks` (the autosave journal's records, [`crate::journal`]).
    pub(crate) fn with_chunks(&self, chunks: Vec<Chunk>) -> Self {
        Self {
            version: self.version,
            extension: self.extension.clone(),
            chunks,
            ..Self::default()
        }
    }

    /// What the last [`Self::set_arena_with`] into this file did with the
    /// tiles: how many it wrote, remembered, hashed, copied and encoded.
    #[must_use]
    pub fn last_save(&self) -> SaveStats {
        self.stats
    }

    /// A copy of this file that Sue 0.0.75 and older open: `arena` in an
    /// [`ARENA`] chunk of version 1 with every raster inline, no pixel
    /// store, and the other chunks as they are. What such a build cannot
    /// read is left out of the copy and named in the second value, for the
    /// conversion report: today the components a later Sue wrote in a form
    /// this build keeps without reading (an older Sue refuses the whole
    /// file over one).
    #[must_use]
    pub fn copy_for_older_sue(&self, arena: &Arena) -> (Self, Vec<String>) {
        let mut older = arena.clone();
        let handles: Vec<_> = older.iter().map(|(handle, _)| handle).collect();
        let mut left_out = Vec::new();
        for handle in handles {
            if let Ok(entity) = older.get_mut(handle)
                && entity.forget_unread_components()
            {
                left_out.push(entity.name().to_owned());
            }
        }
        let mut out = Writer::new();
        older.encode(&mut out);
        let mut copy = Self {
            version: self.version,
            extension: self.extension.clone(),
            chunks: self
                .chunks
                .iter()
                .filter(|chunk| chunk.tag != PIXELS)
                .cloned()
                .collect(),
            ..Self::default()
        };
        copy.set(Chunk {
            tag: ARENA,
            version: ARENA_VERSION_INLINE,
            payload: out.into_bytes(),
        });
        (copy, left_out)
    }

    /// Reads the [`ARENA`] chunk (an empty arena if there is none), with
    /// the rasters of a version 2 arena from the [`PIXELS`] chunk.
    ///
    /// # Errors
    /// If the chunk is malformed or written by a newer Sue, or a raster's
    /// pixels are missing or damaged (the error names its entity).
    pub fn arena(&self, registry: &ComponentRegistry) -> Result<Arena, CodecError> {
        let Some(chunk) = self.chunk(ARENA) else {
            return Ok(Arena::new());
        };
        if chunk.version > ARENA_VERSION {
            return Err(newer("arena", chunk.version));
        }
        let pixels = if chunk.version >= ARENA_VERSION {
            let store = self
                .chunk(PIXELS)
                .ok_or_else(|| CodecError::new("the pixel store of the document is missing", 0))?;
            if store.version > PIXELS_VERSION {
                return Err(newer("pixel store", store.version));
            }
            Some(PixelReader::parse(&store.payload)?)
        } else {
            None
        };
        let mut input = Reader::new(&chunk.payload);
        let arena = Arena::decode_with(&mut input, registry, pixels.as_ref())?;
        if !input.is_empty() {
            return Err(input.error("unexpected bytes after the arena"));
        }
        Ok(arena)
    }

    /// The header of the [`PIXELS`] chunk (tile size, hash kind, key
    /// width), if the file has one this build reads.
    ///
    /// # Errors
    /// If the chunk is malformed.
    pub fn pixel_store(&self) -> Result<Option<StoreHeader>, CodecError> {
        match self.chunk(PIXELS) {
            Some(store) if store.version <= PIXELS_VERSION => {
                PixelReader::parse(&store.payload).map(|reader| Some(reader.header()))
            }
            _ => Ok(None),
        }
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

    /// The file in pieces, in order: the header, then for each chunk its
    /// frame, its payload and its checksum, then the end mark. Every way
    /// of writing a file goes through here, so they cannot differ.
    fn pieces<E>(&self, mut put: impl FnMut(&[u8]) -> Result<(), E>) -> Result<(), E> {
        let mut header = Writer::new();
        header.raw(&MAGIC);
        header.u32(self.version);
        header.bytes(&self.extension);
        put(header.as_bytes())?;
        let mut chunk = |tag: [u8; 4], version: u32, payload: &[u8]| {
            let mut frame = Writer::new();
            frame.raw(&tag);
            frame.u32(version);
            frame.u64(payload.len() as u64);
            put(frame.as_bytes())?;
            put(payload)?;
            put(&crc32(payload).to_le_bytes())
        };
        for part in self.chunks.iter().filter(|chunk| chunk.tag != END) {
            chunk(part.tag, part.version, &part.payload)?;
        }
        chunk(END, 0, &[])
    }

    /// The file's bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.encoded_len());
        let written: Result<(), Infallible> = self.pieces(|piece| {
            bytes.extend_from_slice(piece);
            Ok(())
        });
        match written {
            Ok(()) => bytes,
            Err(never) => match never {},
        }
    }

    /// How many bytes [`Self::to_bytes`] and [`Self::write_to`] produce.
    #[must_use]
    pub fn encoded_len(&self) -> usize {
        let mut len = 0;
        let counted: Result<(), Infallible> = self.pieces(|piece| {
            len += piece.len();
            Ok(())
        });
        match counted {
            Ok(()) => len,
            Err(never) => match never {},
        }
    }

    /// Writes the file into `out`, the same bytes as [`Self::to_bytes`],
    /// without building them in memory first. Wrap a file in a
    /// [`std::io::BufWriter`]: the frames are small writes.
    ///
    /// # Errors
    /// Whatever `out` reports. What was written until then is not a whole
    /// file: a save writes into a new file and renames it over the old one
    /// only when this has returned.
    pub fn write_to(&self, out: &mut impl Write) -> io::Result<()> {
        self.pieces(|piece| out.write_all(piece))
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
        // This build reads neither past version 1 and writes both back.
        let version = input.u32()?;
        let extension = input.bytes()?.to_vec();
        let mut chunks = Vec::new();
        loop {
            let mut tag = [0; 4];
            tag.copy_from_slice(input.raw(4)?);
            let version = input.u32()?;
            let len = usize::try_from(input.u64()?).map_err(|_| input.error("chunk too large"))?;
            let at = input.offset();
            let payload = input.raw(len)?;
            let checksum = input.u32()?;
            if crc32(payload) != checksum {
                return Err(damaged(tag, at));
            }
            if tag == END {
                break;
            }
            chunks.push(Chunk {
                tag,
                version,
                payload: payload.to_vec(),
            });
        }
        Ok(Self {
            version,
            extension,
            chunks,
            ..Self::default()
        })
    }

    /// Reads the header and the frame of every chunk of a file and seeks
    /// over the payloads: a few small reads per chunk, however large the
    /// file. Wrap a file in a [`std::io::BufReader`].
    ///
    /// The payloads are not read, so their checksums are not checked here;
    /// [`Directory::read`] checks the one it fetches. A file that ends
    /// before its end mark is an error, as in [`Self::from_bytes`].
    ///
    /// # Errors
    /// Not a `.sue` file, truncated, or unreadable.
    pub fn scan<R: Read + Seek>(input: &mut R) -> Result<Directory, CodecError> {
        let total = input
            .seek(SeekFrom::End(0))
            .and_then(|total| input.seek(SeekFrom::Start(0)).map(|_| total))
            .map_err(|error| unreadable(&error, 0))?;
        let mut input = Scanner {
            input,
            at: 0,
            total,
        };
        if input.array().ok() != Some(MAGIC) {
            return Err(CodecError::new("not a Sue document", 0));
        }
        let version = u32::from_le_bytes(input.array()?);
        let len = input.varint()?;
        if len > input.left() {
            return Err(input.error(format!("length {len} exceeds the input")));
        }
        // No longer than the file, so this cannot ask for absurd memory.
        let mut extension = vec![0; offset_of(len)];
        input.fill(&mut extension)?;
        let mut chunks = Vec::new();
        loop {
            let frame: [u8; FRAME] = input.array()?;
            let mut frame = Reader::new(&frame);
            let mut tag = [0; 4];
            tag.copy_from_slice(frame.raw(4)?);
            let (version, len) = (frame.u32()?, frame.u64()?);
            let offset = input.at;
            input.skip(len)?;
            let checksum = u32::from_le_bytes(input.array()?);
            if tag == END {
                break;
            }
            chunks.push(ChunkEntry {
                tag,
                version,
                offset,
                len,
                checksum,
            });
        }
        Ok(Directory {
            version,
            extension,
            chunks,
        })
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

    #[test]
    fn a_new_file_is_version_one_with_no_extension() {
        let file = SueFile::new();
        assert_eq!(file.container_version(), CONTAINER_VERSION);
        assert!(file.header_extension().is_empty());
        assert_eq!(file.encoded_len(), file.to_bytes().len());
        // The bytes 0.0.75 wrote for an empty document.
        let mut old = Vec::new();
        old.extend_from_slice(&MAGIC);
        old.extend_from_slice(&1_u32.to_le_bytes());
        old.push(0);
        old.extend_from_slice(b"END\0");
        old.extend_from_slice(&[0; 16]);
        assert_eq!(file.to_bytes(), old);
    }
}
