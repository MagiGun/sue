//! The pixel store: rasters saved as tiles, each distinct tile once.
//!
//! A raster in an `ARNA` chunk of version 2 may be written "by reference":
//! its format byte carries [`TILED`], and instead of its samples it lists
//! its tiles in rows from the top left, each one of
//!
//! ```text
//! 0  empty             every byte of the tile is zero
//! 1  solid  value      every pixel is the same; the value is one pixel
//! 2  blob   key        the pixels are the blob with that key in PIXL
//! ```
//!
//! The blobs live in the `PIXL` chunk (version 1):
//!
//! ```text
//! tile size  varint   pixels per side of a tile (256)
//! hash kind  u8       how keys were made (HASH_SHA256), or HASH_NONE
//! key width  u8       bytes per key (16)
//! blobs      varint count, then for each:
//!            key [key width], codec u8, format u8, width varint,
//!            height varint, data (length-prefixed bytes)
//! ```
//!
//! Tiles at the right and bottom edges are as wide and as high as what is
//! left of the raster. A blob says its own format and size, so it can be
//! read without the raster that points at it. The key of a tile is a hash
//! of its format, size and pixels; a store that meets a key it already
//! holds compares the pixels before it shares the blob, and a mismatch
//! gets a key of its own (the hash taken again with a salt). A reader never
//! recomputes a key: to it a key is a name.
//!
//! Codecs: [`CODEC_RAW`] (the pixels as they are) and [`CODEC_PLANES`]
//! (one plane per byte of a pixel, alpha first, each plane with the median
//! predictor of LOCO-I, then deflate). The planes codec was chosen by the
//! tile codec benchmark (`tests/codec_bench.rs`); a tile it would not make
//! smaller is stored raw. A blob in a codec this build does not know
//! leaves its raster unread and kept, like a pixel format it does not know.
//! Samples are little-endian bytes, as in an inline raster.

use std::collections::HashMap;

use miniz_oxide::deflate::compress_to_vec;
use miniz_oxide::inflate::decompress_to_vec_with_limit;
use sha2::{Digest, Sha256};

use crate::codec::{CodecError, Reader, Writer};
use crate::component::{PixelFormat, PixelStorage, RasterTile2D};
use crate::{Entity, OpaqueComponent, Value};

/// The version of the `PIXL` chunk this build reads and writes.
pub const VERSION: u32 = 1;
/// The tile size this build writes, in pixels per side. The brush's 64 px
/// working tiles divide it exactly.
pub const TILE_SIZE: u32 = 256;
/// The key width this build writes, in bytes (128 bits).
pub const KEY_WIDTH: u8 = 16;
/// The bit of a raster's format byte that says "tiles by reference".
pub const TILED: u8 = 0x80;
/// Keys are names only; nothing says how they were made.
pub const HASH_NONE: u8 = 0;
/// Keys are SHA-256 of a salt, the format, the size and the pixels of a
/// tile, cut to the key width.
pub const HASH_SHA256: u8 = 1;
/// A blob holding the pixels as they are.
pub const CODEC_RAW: u8 = 0;
/// A blob holding byte planes with the median predictor, deflated.
pub const CODEC_PLANES: u8 = 1;

const TILE_EMPTY: u8 = 0;
const TILE_SOLID: u8 = 1;
const TILE_BLOB: u8 = 2;
/// The largest tile side a reader accepts.
const MAX_TILE_SIZE: u32 = 1 << 14;
/// The widest key there is (all of SHA-256).
const MAX_KEY_WIDTH: usize = 32;

type Key = [u8; MAX_KEY_WIDTH];

/// How a save writes pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelOptions {
    /// Deflate level, 1 (fastest) to 10. One reader reads every level.
    pub level: u8,
    /// Pixels per side of a tile.
    pub tile_size: u32,
    /// Bytes per key, 16 to 32.
    pub key_width: u8,
}

impl PixelOptions {
    /// What Save writes: deflate level 3.
    pub const SAVE: Self = Self {
        level: 3,
        tile_size: TILE_SIZE,
        key_width: KEY_WIDTH,
    };
    /// What autosave writes: deflate level 1, a little larger and about
    /// twice as fast.
    pub const AUTOSAVE: Self = Self {
        level: 1,
        ..Self::SAVE
    };
}

impl Default for PixelOptions {
    fn default() -> Self {
        Self::SAVE
    }
}

/// A tile's place in its raster, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

/// The tiles of a `width` by `height` raster, in rows from the top left.
fn tiles(width: u32, height: u32, size: u32) -> impl Iterator<Item = Rect> {
    (0..height).step_by(size as usize).flat_map(move |y| {
        (0..width).step_by(size as usize).map(move |x| Rect {
            x,
            y,
            width: size.min(width - x),
            height: size.min(height - y),
        })
    })
}

/// Copies the pixels of `rect` out of `raster` as little-endian bytes.
fn tile_bytes(raster: &RasterTile2D, rect: Rect, out: &mut Vec<u8>) {
    out.clear();
    let stride = raster.width() as usize;
    let (x, w) = (rect.x as usize, rect.width as usize);
    match raster.pixels() {
        PixelStorage::U8(bytes) => {
            let bpp = raster.format().bytes_per_pixel();
            for row in rect.y as usize..(rect.y + rect.height) as usize {
                let start = (row * stride + x) * bpp;
                out.extend_from_slice(&bytes[start..start + w * bpp]);
            }
        }
        PixelStorage::F32(samples) => {
            let channels = raster.format().channels();
            for row in rect.y as usize..(rect.y + rect.height) as usize {
                let start = (row * stride + x) * channels;
                for sample in &samples[start..start + w * channels] {
                    out.extend_from_slice(&sample.to_le_bytes());
                }
            }
        }
    }
}

/// Writes `tile` (little-endian bytes) into the pixels of `rect`.
fn put_tile(storage: &mut Samples, stride: usize, format: PixelFormat, rect: Rect, tile: &[u8]) {
    let (x, w) = (rect.x as usize, rect.width as usize);
    let rows = rect.y as usize..(rect.y + rect.height) as usize;
    match storage {
        Samples::U8(bytes) => {
            let bpp = format.bytes_per_pixel();
            for (row, line) in rows.zip(tile.chunks_exact(w * bpp)) {
                let start = (row * stride + x) * bpp;
                bytes[start..start + w * bpp].copy_from_slice(line);
            }
        }
        Samples::F32(samples) => {
            let channels = format.channels();
            for (row, line) in rows.zip(tile.chunks_exact(w * format.bytes_per_pixel())) {
                let start = (row * stride + x) * channels;
                let (values, _) = line.as_chunks::<4>();
                for (sample, value) in samples[start..start + w * channels].iter_mut().zip(values) {
                    *sample = f32::from_le_bytes(*value);
                }
            }
        }
    }
}

/// Fills `rect` with one pixel (little-endian bytes).
fn fill_tile(storage: &mut Samples, stride: usize, format: PixelFormat, rect: Rect, pixel: &[u8]) {
    let row: Vec<u8> = pixel.repeat(rect.width as usize);
    let tile = row.repeat(rect.height as usize);
    put_tile(storage, stride, format, rect, &tile);
}

enum Samples {
    U8(Vec<u8>),
    F32(Vec<f32>),
}

/// A zeroed buffer of `len` samples, or an error when there is no memory
/// for it (a damaged size must not abort the program).
fn zeroed<T: Clone + Default>(len: usize, input: &Reader<'_>) -> Result<Vec<T>, CodecError> {
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(len)
        .map_err(|_| input.error(format!("a picture of {len} samples does not fit in memory")))?;
    buffer.resize(len, T::default());
    Ok(buffer)
}

enum Class {
    Empty,
    Solid,
    Blob,
}

fn classify(tile: &[u8], bpp: usize) -> Class {
    if tile.iter().all(|byte| *byte == 0) {
        return Class::Empty;
    }
    let first = &tile[..bpp];
    if tile.chunks_exact(bpp).all(|pixel| pixel == first) {
        Class::Solid
    } else {
        Class::Blob
    }
}

// ─── The planes codec ──────────────────────────────────────────────────────

/// The median of left, up and left + up - upleft (LOCO-I).
fn median(left: u8, up: u8, upleft: u8) -> u8 {
    let (low, high) = (left.min(up), left.max(up));
    if upleft >= high {
        low
    } else if upleft <= low {
        high
    } else {
        // Between low and high, so the sum stays in a byte.
        left.wrapping_add(up).wrapping_sub(upleft)
    }
}

/// The byte offsets inside a pixel, in plane order: the alpha channel
/// first, then the others, each sample's bytes from the lowest.
fn plane_order(format: PixelFormat) -> Vec<usize> {
    let channels = format.channels();
    let sample = format.bytes_per_pixel() / channels;
    let order: Vec<usize> = if channels == 4 {
        vec![3, 0, 1, 2]
    } else {
        (0..channels).collect()
    };
    order
        .into_iter()
        .flat_map(|channel| (0..sample).map(move |byte| channel * sample + byte))
        .collect()
}

/// Appends the residues of `plane` (rows of `width`) after the median
/// predictor: left only in the first row, up only in the first column,
/// nothing for the first pixel.
fn med_forward(plane: &[u8], width: usize, out: &mut Vec<u8>) {
    let mut up: Option<&[u8]> = None;
    for row in plane.chunks_exact(width) {
        match up {
            None => {
                out.push(row[0]);
                out.extend(row.windows(2).map(|pair| pair[1].wrapping_sub(pair[0])));
            }
            Some(up) => {
                out.push(row[0].wrapping_sub(up[0]));
                for x in 1..width {
                    out.push(row[x].wrapping_sub(median(row[x - 1], up[x], up[x - 1])));
                }
            }
        }
        up = Some(row);
    }
}

fn med_inverse(plane: &mut [u8], width: usize) {
    let height = plane.len() / width;
    for x in 1..width {
        plane[x] = plane[x].wrapping_add(plane[x - 1]);
    }
    for y in 1..height {
        let (done, rest) = plane.split_at_mut(y * width);
        let up = &done[(y - 1) * width..];
        let row = &mut rest[..width];
        row[0] = row[0].wrapping_add(up[0]);
        for x in 1..width {
            row[x] = row[x].wrapping_add(median(row[x - 1], up[x], up[x - 1]));
        }
    }
}

/// Encodes one tile: the planes codec, or raw when that is not smaller.
fn encode_tile(tile: &[u8], format: PixelFormat, width: usize, level: u8) -> (u8, Vec<u8>) {
    let bpp = format.bytes_per_pixel();
    let count = tile.len() / bpp;
    let mut residues = Vec::with_capacity(tile.len());
    let mut plane = Vec::with_capacity(count);
    for offset in plane_order(format) {
        plane.clear();
        plane.extend(tile.chunks_exact(bpp).map(|pixel| pixel[offset]));
        med_forward(&plane, width, &mut residues);
    }
    let blob = compress_to_vec(&residues, level);
    if blob.len() < tile.len() {
        (CODEC_PLANES, blob)
    } else {
        (CODEC_RAW, tile.to_vec())
    }
}

/// What reading a blob gave.
enum Decoded {
    Pixels(Vec<u8>),
    /// A codec this build does not know.
    Unknown,
}

/// Decodes a blob of `len` bytes of pixels.
fn decode_blob(blob: &BlobView<'_>, format: PixelFormat) -> Result<Decoded, String> {
    let bpp = format.bytes_per_pixel();
    let len = blob.width as usize * blob.height as usize * bpp;
    match blob.codec {
        CODEC_RAW => {
            if blob.data.len() != len {
                return Err(format!(
                    "a raw tile holds {} bytes, not {len}",
                    blob.data.len()
                ));
            }
            Ok(Decoded::Pixels(blob.data.to_vec()))
        }
        CODEC_PLANES => {
            let mut planes = decompress_to_vec_with_limit(blob.data, len)
                .map_err(|error| format!("a tile does not inflate ({:?})", error.status))?;
            if planes.len() != len {
                return Err(format!(
                    "a tile inflates to {} bytes, not {len}",
                    planes.len()
                ));
            }
            let count = len / bpp;
            let width = blob.width as usize;
            for plane in planes.chunks_exact_mut(count) {
                med_inverse(plane, width);
            }
            let mut tile = vec![0; len];
            for (plane, offset) in planes.chunks_exact(count).zip(plane_order(format)) {
                for (pixel, value) in tile.chunks_exact_mut(bpp).zip(plane) {
                    pixel[offset] = *value;
                }
            }
            Ok(Decoded::Pixels(tile))
        }
        _ => Ok(Decoded::Unknown),
    }
}

/// Runs `work` over `items` on as many threads as the machine has, in
/// order. Where no thread can start (a browser), it runs here.
fn parallel_map<I: Sync, O: Send>(items: &[I], work: impl Fn(&I) -> O + Sync) -> Vec<O> {
    let threads = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(items.len().div_ceil(4));
    if threads <= 1 {
        return items.iter().map(&work).collect();
    }
    let size = items.len().div_ceil(threads);
    let work = &work;
    std::thread::scope(|scope| {
        let parts: Vec<_> = items
            .chunks(size)
            .map(|part| {
                let handle = std::thread::Builder::new()
                    .name("sue-pixels".to_owned())
                    .spawn_scoped(scope, move || part.iter().map(work).collect::<Vec<O>>());
                (part, handle.ok())
            })
            .collect();
        parts
            .into_iter()
            .flat_map(|(part, handle)| {
                handle
                    .and_then(|handle| handle.join().ok())
                    .unwrap_or_else(|| part.iter().map(work).collect())
            })
            .collect()
    })
}

/// SHA-256 of the salt, the tile's description and its pixels.
fn sha256_key(salt: u64, description: &[u8], pixels: &[u8]) -> Key {
    let mut hasher = Sha256::new();
    hasher.update(salt.to_le_bytes());
    hasher.update(description);
    hasher.update(pixels);
    let mut key = [0; MAX_KEY_WIDTH];
    key.copy_from_slice(&hasher.finalize());
    key
}

/// `true` if `entity` holds anything this build could not read, which may
/// point at blobs no raster this build understands points at (Rule 3).
pub(crate) fn may_refer_to_blobs(entity: &Entity) -> bool {
    fn opaque(value: &Value) -> bool {
        match value {
            Value::Opaque { .. } => true,
            Value::List(values) => values.iter().any(opaque),
            Value::Map(values) => values.values().any(opaque),
            _ => false,
        }
    }
    entity.holds_unread()
        || entity
            .exts()
            .any(|component| component.as_any().is::<OpaqueComponent>())
        || entity.properties().iter().any(|(_, value)| opaque(value))
}

// ─── Reading ───────────────────────────────────────────────────────────────

/// One blob as the file holds it.
#[derive(Clone, Copy, Debug)]
struct BlobView<'a> {
    codec: u8,
    format: u8,
    width: u32,
    height: u32,
    data: &'a [u8],
    /// The whole record, key included, to write it back as it is.
    record: &'a [u8],
}

/// The header of a `PIXL` chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoreHeader {
    /// Pixels per side of a tile.
    pub tile_size: u32,
    /// [`HASH_SHA256`] or [`HASH_NONE`] (or a later kind).
    pub hash_kind: u8,
    /// Bytes per key.
    pub key_width: u8,
}

/// The blobs of a `PIXL` chunk, read without decoding any.
#[derive(Debug)]
pub(crate) struct PixelReader<'a> {
    header: StoreHeader,
    blobs: HashMap<Key, BlobView<'a>>,
    /// Every blob in file order.
    order: Vec<Key>,
}

fn key_of(bytes: &[u8]) -> Key {
    let mut key = [0; MAX_KEY_WIDTH];
    key[..bytes.len()].copy_from_slice(bytes);
    key
}

impl<'a> PixelReader<'a> {
    /// Reads the header and the table of blobs of a `PIXL` payload.
    pub(crate) fn parse(payload: &'a [u8]) -> Result<Self, CodecError> {
        let mut input = Reader::new(payload);
        let tile_size = u32::try_from(input.varint()?)
            .ok()
            .filter(|size| (1..=MAX_TILE_SIZE).contains(size))
            .ok_or_else(|| input.error("the pixel store has an impossible tile size"))?;
        let hash_kind = input.u8()?;
        let key_width = input.u8()?;
        if !(1..=MAX_KEY_WIDTH).contains(&usize::from(key_width)) {
            return Err(input.error("the pixel store has an impossible key width"));
        }
        let width = usize::from(key_width);
        let count = input.len(width + 5)?;
        let mut blobs = HashMap::with_capacity(count);
        let mut order = Vec::with_capacity(count);
        for _ in 0..count {
            let start = input.offset();
            let key = key_of(input.raw(width)?);
            let codec = input.u8()?;
            let format = input.u8()?;
            let size = |input: &mut Reader<'_>| {
                u32::try_from(input.varint()?)
                    .ok()
                    .filter(|side| (1..=tile_size).contains(side))
                    .ok_or_else(|| input.error("a tile of the pixel store has an impossible size"))
            };
            let (blob_width, blob_height) = (size(&mut input)?, size(&mut input)?);
            let data = input.bytes()?;
            let blob = BlobView {
                codec,
                format,
                width: blob_width,
                height: blob_height,
                data,
                record: &payload[start..input.offset()],
            };
            if blobs.insert(key, blob).is_some() {
                return Err(input.error("the pixel store lists one tile twice"));
            }
            order.push(key);
        }
        if !input.is_empty() {
            return Err(input.error("unexpected bytes after the pixel store"));
        }
        Ok(Self {
            header: StoreHeader {
                tile_size,
                hash_kind,
                key_width,
            },
            blobs,
            order,
        })
    }

    /// The header: tile size, hash kind and key width.
    pub(crate) fn header(&self) -> StoreHeader {
        self.header
    }

    /// How many blobs the store holds.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.order.len()
    }

    /// Reads a raster written by reference. `None` (and `input` wherever
    /// it got to) when the raster uses something this build does not know:
    /// a pixel format, a kind of tile or a codec. The caller keeps such a
    /// raster's bytes.
    ///
    /// # Errors
    /// If the raster is malformed or one of its tiles is missing or damaged.
    pub(crate) fn read_raster(
        &self,
        input: &mut Reader<'_>,
    ) -> Result<Option<RasterTile2D>, CodecError> {
        let (width, height) = (input.u32()?, input.u32()?);
        let code = input.u8()?;
        let Some(format) = PixelFormat::from_code(code & !TILED) else {
            return Ok(None);
        };
        if width == 0 || height == 0 {
            return Err(input.error("a picture has no pixels"));
        }
        let bpp = format.bytes_per_pixel();
        let key_width = usize::from(self.header.key_width);
        let mut solids = Vec::new();
        let mut blobs = Vec::new();
        for rect in tiles(width, height, self.header.tile_size) {
            match input.u8()? {
                TILE_EMPTY => {}
                TILE_SOLID => solids.push((rect, input.raw(bpp)?)),
                TILE_BLOB => {
                    let key = key_of(input.raw(key_width)?);
                    let blob = self.blobs.get(&key).ok_or_else(|| {
                        input.error("a tile of the picture is missing from the pixel store")
                    })?;
                    if blob.format != format.code()
                        || blob.width != rect.width
                        || blob.height != rect.height
                    {
                        return Err(input.error("a tile of the picture does not fit its place"));
                    }
                    blobs.push((rect, *blob));
                }
                _ => return Ok(None),
            }
        }
        // Nothing drawn: a blank raster, which shares its zeros with the
        // other blank layers of its size (a comic's untouched pages).
        if solids.is_empty() && blobs.is_empty() {
            return RasterTile2D::blank(width, height, format)
                .map(Some)
                .map_err(|error| input.error(error.to_string()));
        }
        let decoded = parallel_map(&blobs, |(_, blob)| decode_blob(blob, format));
        let len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(format.channels()))
            .ok_or_else(|| input.error("a picture is too large"))?;
        let mut samples = match format {
            PixelFormat::RgbaF32 => Samples::F32(zeroed(len, input)?),
            PixelFormat::Gray8 | PixelFormat::Rgba8 => Samples::U8(zeroed(len, input)?),
        };
        let stride = width as usize;
        for (rect, pixel) in solids {
            fill_tile(&mut samples, stride, format, rect, pixel);
        }
        for ((rect, _), tile) in blobs.iter().zip(decoded) {
            match tile.map_err(|message| input.error(message))? {
                Decoded::Pixels(tile) => put_tile(&mut samples, stride, format, *rect, &tile),
                Decoded::Unknown => return Ok(None),
            }
        }
        let raster = match samples {
            Samples::U8(bytes) => RasterTile2D::from_bytes(width, height, format, bytes),
            Samples::F32(samples) => RasterTile2D::from_f32(width, height, samples),
        };
        raster
            .map(Some)
            .map_err(|error| input.error(error.to_string()))
    }
}

// ─── Writing ───────────────────────────────────────────────────────────────

/// Where a key's blob comes from.
#[derive(Clone, Copy, Debug)]
enum Source<'k> {
    /// A blob of this save, the `n`th of [`PixelWriter::pending`].
    New(usize),
    /// A blob of the file this save replaces, kept as it was.
    Kept(BlobView<'k>),
}

/// What becomes a blob of this save.
#[derive(Debug)]
enum Blob<'k> {
    /// A tile to encode when the save finishes, from a snapshot of its
    /// raster (a clone shares the pixels).
    Encode { raster: RasterTile2D, rect: Rect },
    /// A blob of the store this save replaces, the same pixels already
    /// encoded: written back as it is.
    Copy(BlobView<'k>),
}

/// A blob this save writes.
#[derive(Debug)]
struct Pending<'k> {
    key: Key,
    blob: Blob<'k>,
}

/// The hash a writer keys tiles with.
type Hasher = fn(u64, &[u8], &[u8]) -> Key;

/// A tile's place, format and revision ([`RasterTile2D::tile_revisions`]):
/// two tiles with the same hold the same pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Memo {
    revision: u64,
    format: u8,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

/// How a save wrote one tile.
#[derive(Clone, Debug, PartialEq, Eq)]
enum TileCode {
    Empty,
    /// The one pixel every pixel of the tile is.
    Solid(Vec<u8>),
    Blob(Key),
}

/// What a save of a `.sue` file remembers for the next save of the same
/// file: how it wrote each tile, by the tile's revision. A tile whose
/// revision did not change is written again without being read, hashed or
/// encoded; its blob is copied from the store the last save wrote. It
/// lives beside the file in memory only and is dropped whenever the store
/// it describes is replaced.
#[derive(Clone, Debug, Default)]
pub struct SaveCache {
    tiles: HashMap<Memo, TileCode>,
}

impl SaveCache {
    /// How many tiles it remembers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    /// `true` when it remembers nothing (a file never saved by this run).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }
}

/// What one save did with the tiles, so tests and the benchmark can tell
/// an incremental save from a full one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SaveStats {
    /// Tiles written, empty and solid ones included.
    pub tiles: usize,
    /// Tiles written from the [`SaveCache`] without reading their pixels.
    pub remembered: usize,
    /// Tiles whose pixels were hashed for their key.
    pub hashed: usize,
    /// Blobs copied, still encoded, from the store this save replaces.
    pub copied: usize,
    /// Blobs encoded anew.
    pub encoded: usize,
}

/// Collects the tiles of the rasters a save writes by reference, and
/// writes the `PIXL` chunk at the end.
#[derive(Debug)]
pub(crate) struct PixelWriter<'k> {
    header: StoreHeader,
    level: u8,
    hasher: Hasher,
    keys: HashMap<Key, Source<'k>>,
    pending: Vec<Pending<'k>>,
    /// Blobs of the old file kept whatever points at them, in their order.
    kept: Vec<&'k [u8]>,
    /// Blobs of the store this save replaces, written again only when a
    /// tile of this save has their very pixels.
    old: HashMap<Key, BlobView<'k>>,
    /// How the last save wrote each tile.
    memo: Option<&'k SaveCache>,
    /// How this save wrote each tile, for the next one.
    next: SaveCache,
    stats: SaveStats,
    /// Whether any raster was written by reference.
    used: bool,
    scratch: Vec<u8>,
    other: Vec<u8>,
}

impl<'k> PixelWriter<'k> {
    /// A store of nothing but this save's tiles.
    pub(crate) fn new(options: &PixelOptions) -> Self {
        Self {
            header: StoreHeader {
                tile_size: options.tile_size.clamp(1, MAX_TILE_SIZE),
                hash_kind: HASH_SHA256,
                key_width: options.key_width.clamp(KEY_WIDTH, MAX_KEY_WIDTH as u8),
            },
            level: options.level.clamp(1, 10),
            hasher: sha256_key,
            keys: HashMap::new(),
            pending: Vec::new(),
            kept: Vec::new(),
            old: HashMap::new(),
            memo: None,
            next: SaveCache::default(),
            stats: SaveStats::default(),
            used: false,
            scratch: Vec::new(),
            other: Vec::new(),
        }
    }

    /// A store of this save's tiles that takes the blobs of `old` (the
    /// store this save replaces) as they are, already encoded, for every
    /// tile whose pixels one of them holds; `memo` says how the save that
    /// wrote `old` wrote each tile. Nothing of `old` is written that no
    /// tile uses. Tile size and key width stay `old`'s when they are
    /// this build's, so the keys stay valid; otherwise `old` is not used.
    pub(crate) fn reusing(
        old: &PixelReader<'k>,
        memo: Option<&'k SaveCache>,
        options: &PixelOptions,
    ) -> Self {
        let mut writer = Self::new(options);
        if old.header.tile_size == writer.header.tile_size
            && old.header.key_width == writer.header.key_width
            && old.header.hash_kind == HASH_SHA256
        {
            writer.old = old.blobs.clone();
            writer.memo = memo;
        }
        writer
    }

    /// A store that writes back every blob of `old` as it was, because
    /// something this build cannot read may point at any of them (Rule 3).
    /// Tile size and key width stay the old file's, so its keys stay
    /// valid; new tiles that equal an old blob share it.
    pub(crate) fn keeping(
        old: &PixelReader<'k>,
        memo: Option<&'k SaveCache>,
        options: &PixelOptions,
    ) -> Self {
        let mut writer = Self::new(options);
        writer.header = StoreHeader {
            tile_size: old.header.tile_size,
            hash_kind: if old.header.hash_kind == HASH_SHA256 {
                HASH_SHA256
            } else {
                HASH_NONE
            },
            key_width: old.header.key_width,
        };
        for key in &old.order {
            let blob = old.blobs[key];
            writer.keys.insert(*key, Source::Kept(blob));
            writer.kept.push(blob.record);
        }
        writer.memo = memo;
        writer
    }

    /// Uses `hasher` for keys, so a test can force two tiles onto one key.
    #[cfg(test)]
    pub(crate) fn with_hasher(mut self, hasher: Hasher) -> Self {
        self.hasher = hasher;
        self
    }

    /// Writes `raster` by reference: its size, its format with [`TILED`],
    /// and its tiles.
    pub(crate) fn write_raster(&mut self, raster: &RasterTile2D, out: &mut Writer) {
        self.used = true;
        out.u32(raster.width());
        out.u32(raster.height());
        out.u8(TILED | raster.format().code());
        let bpp = raster.format().bytes_per_pixel();
        let width = usize::from(self.header.key_width);
        // Revisions follow this build's tile grid; a store of another tile
        // size cannot use them.
        let revisions = (self.header.tile_size == TILE_SIZE).then(|| raster.tile_revisions());
        for (index, rect) in
            tiles(raster.width(), raster.height(), self.header.tile_size).enumerate()
        {
            self.stats.tiles += 1;
            let memo = revisions
                .and_then(|revisions| revisions.get(index))
                .map(|revision| Memo {
                    revision: *revision,
                    format: raster.format().code(),
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height,
                });
            let code = match memo.and_then(|memo| self.remembered(&memo)) {
                Some(code) => {
                    self.stats.remembered += 1;
                    code
                }
                None => {
                    let mut tile = std::mem::take(&mut self.scratch);
                    tile_bytes(raster, rect, &mut tile);
                    let code = match classify(&tile, bpp) {
                        Class::Empty => TileCode::Empty,
                        Class::Solid => TileCode::Solid(tile[..bpp].to_vec()),
                        Class::Blob => TileCode::Blob(self.key_for(raster, rect, &tile)),
                    };
                    self.scratch = tile;
                    code
                }
            };
            match &code {
                TileCode::Empty => out.u8(TILE_EMPTY),
                TileCode::Solid(pixel) => {
                    out.u8(TILE_SOLID);
                    out.raw(pixel);
                }
                TileCode::Blob(key) => {
                    out.u8(TILE_BLOB);
                    out.raw(&key[..width]);
                }
            }
            if let Some(memo) = memo {
                self.next.tiles.insert(memo, code);
            }
        }
    }

    /// How the last save wrote the tile `memo`, if that still holds here:
    /// an empty or solid tile always, a blob when its key is free in this
    /// save and its blob is at hand (kept, or in the old store, which it
    /// then joins). `None` sends the tile the long way: read and hashed.
    fn remembered(&mut self, memo: &Memo) -> Option<TileCode> {
        let code = self.memo?.tiles.get(memo)?;
        let TileCode::Blob(key) = code else {
            return Some(code.clone());
        };
        match self.keys.get(key) {
            // Already named in this save: by this very tile elsewhere, or by
            // others pixels after a collision; the long way tells which.
            Some(Source::New(_)) => None,
            Some(Source::Kept(_)) => Some(code.clone()),
            None => {
                let blob = *self.old.get(key)?;
                self.copy(*key, blob);
                Some(code.clone())
            }
        }
    }

    /// Takes the old blob `blob` into this save under `key`.
    fn copy(&mut self, key: Key, blob: BlobView<'k>) {
        self.keys.insert(key, Source::New(self.pending.len()));
        self.pending.push(Pending {
            key,
            blob: Blob::Copy(blob),
        });
        self.stats.copied += 1;
    }

    /// `true` if `blob` holds the pixels `tile` of a `format` tile of
    /// `rect`'s size.
    fn blob_holds(blob: &BlobView<'_>, format: PixelFormat, rect: Rect, tile: &[u8]) -> bool {
        blob.format == format.code()
            && blob.width == rect.width
            && blob.height == rect.height
            && matches!(
                decode_blob(blob, format),
                Ok(Decoded::Pixels(pixels)) if pixels == tile
            )
    }

    /// The key of a tile: its hash, salted again until the key is free or
    /// names the very same pixels.
    fn key_for(&mut self, raster: &RasterTile2D, rect: Rect, tile: &[u8]) -> Key {
        self.stats.hashed += 1;
        let format = raster.format();
        let mut description = vec![format.code()];
        description.extend_from_slice(&rect.width.to_le_bytes());
        description.extend_from_slice(&rect.height.to_le_bytes());
        let width = usize::from(self.header.key_width);
        for salt in 0.. {
            let mut key = (self.hasher)(salt, &description, tile);
            key[width..].fill(0);
            let same = match self.keys.get(&key) {
                None => {
                    // The store this save replaces may hold these pixels
                    // already encoded; any other blob under the key is not
                    // written, so the key is free for these pixels anyway.
                    match self.old.get(&key) {
                        Some(blob) if Self::blob_holds(blob, format, rect, tile) => {
                            let blob = *blob;
                            self.copy(key, blob);
                        }
                        _ => {
                            self.keys.insert(key, Source::New(self.pending.len()));
                            self.pending.push(Pending {
                                key,
                                blob: Blob::Encode {
                                    raster: raster.clone(),
                                    rect,
                                },
                            });
                            self.stats.encoded += 1;
                        }
                    }
                    return key;
                }
                Some(Source::New(index)) => match &self.pending[*index].blob {
                    Blob::Encode {
                        raster: earlier,
                        rect: place,
                    } => {
                        earlier.format() == format
                            && place.width == rect.width
                            && place.height == rect.height
                            && {
                                tile_bytes(earlier, *place, &mut self.other);
                                self.other == tile
                            }
                    }
                    Blob::Copy(blob) => Self::blob_holds(blob, format, rect, tile),
                },
                Some(Source::Kept(blob)) => Self::blob_holds(blob, format, rect, tile),
            };
            if same {
                return key;
            }
        }
        unreachable!("a salt that frees the key is found long before u64 runs out")
    }

    /// The `PIXL` payload, or `None` when no raster was written by
    /// reference and no old blob is kept.
    #[cfg(test)]
    pub(crate) fn finish(self) -> Option<Vec<u8>> {
        self.finish_saving().0
    }

    /// As [`Self::finish`], with how this save wrote each tile (for the
    /// next save) and what it did.
    pub(crate) fn finish_saving(self) -> (Option<Vec<u8>>, SaveCache, SaveStats) {
        if !self.used && self.kept.is_empty() {
            return (None, SaveCache::default(), self.stats);
        }
        let level = self.level;
        let encoded = parallel_map(&self.pending, |pending| match &pending.blob {
            Blob::Encode { raster, rect } => {
                let mut tile = Vec::new();
                tile_bytes(raster, *rect, &mut tile);
                Some(encode_tile(
                    &tile,
                    raster.format(),
                    rect.width as usize,
                    level,
                ))
            }
            Blob::Copy(_) => None,
        });
        let width = usize::from(self.header.key_width);
        let mut out = Writer::new();
        out.varint(u64::from(self.header.tile_size));
        out.u8(self.header.hash_kind);
        out.u8(self.header.key_width);
        out.len(self.kept.len() + self.pending.len());
        for record in &self.kept {
            out.raw(record);
        }
        for (pending, encoded) in self.pending.iter().zip(encoded) {
            match (&pending.blob, encoded) {
                (Blob::Encode { raster, rect }, Some((codec, data))) => {
                    out.raw(&pending.key[..width]);
                    out.u8(codec);
                    out.u8(raster.format().code());
                    out.varint(u64::from(rect.width));
                    out.varint(u64::from(rect.height));
                    out.bytes(&data);
                }
                (Blob::Copy(blob), _) => out.raw(blob.record),
                (Blob::Encode { .. }, None) => unreachable!("every tile to encode was encoded"),
            }
        }
        (Some(out.into_bytes()), self.next, self.stats)
    }
}

/// The `PIXL` payload `next` without the blobs `previous` already holds
/// under the same key with the same bytes: what an autosave journal
/// record adds ([`crate::journal`]). `None` if either payload is not a
/// store this build reads, or their tile size or key width differ.
#[must_use]
pub fn store_delta(previous: &[u8], next: &[u8]) -> Option<Vec<u8>> {
    let previous = PixelReader::parse(previous).ok()?;
    let next = PixelReader::parse(next).ok()?;
    if previous.header.tile_size != next.header.tile_size
        || previous.header.key_width != next.header.key_width
    {
        return None;
    }
    let fresh: Vec<&[u8]> = next
        .order
        .iter()
        .map(|key| next.blobs[key].record)
        .filter(|record| {
            let key = key_of(&record[..usize::from(next.header.key_width)]);
            previous
                .blobs
                .get(&key)
                .is_none_or(|old| old.record != *record)
        })
        .collect();
    Some(store_payload(next.header, &fresh))
}

/// The `PIXL` payload of `base` with the blobs of `delta` added; a blob of
/// `delta` replaces one of `base` under the same key. `None` as for
/// [`store_delta`].
#[must_use]
pub fn store_merge(base: &[u8], delta: &[u8]) -> Option<Vec<u8>> {
    let base = PixelReader::parse(base).ok()?;
    let delta = PixelReader::parse(delta).ok()?;
    if base.header.tile_size != delta.header.tile_size
        || base.header.key_width != delta.header.key_width
    {
        return None;
    }
    let mut records: Vec<&[u8]> = base
        .order
        .iter()
        .filter(|key| !delta.blobs.contains_key(*key))
        .map(|key| base.blobs[key].record)
        .collect();
    records.extend(delta.order.iter().map(|key| delta.blobs[key].record));
    Some(store_payload(delta.header, &records))
}

/// A `PIXL` payload of `header` and the blob `records`.
fn store_payload(header: StoreHeader, records: &[&[u8]]) -> Vec<u8> {
    let mut out = Writer::new();
    out.varint(u64::from(header.tile_size));
    out.u8(header.hash_kind);
    out.u8(header.key_width);
    out.len(records.len());
    for record in records {
        out.raw(record);
    }
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn raster(
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

    fn round_trip(rasters: &[RasterTile2D], options: &PixelOptions) -> (Vec<RasterTile2D>, usize) {
        let mut writer = PixelWriter::new(options);
        let mut out = Writer::new();
        for raster in rasters {
            writer.write_raster(raster, &mut out);
        }
        let store = writer.finish().unwrap();
        let reader = PixelReader::parse(&store).unwrap();
        let bytes = out.into_bytes();
        let mut input = Reader::new(&bytes);
        let back = rasters
            .iter()
            .map(|_| reader.read_raster(&mut input).unwrap().unwrap())
            .collect();
        assert!(input.is_empty());
        (back, reader.len())
    }

    #[test]
    fn the_median_predictor_inverts() {
        let plane: Vec<u8> = (0..37 * 11).map(|i: u32| (i * 7919 % 251) as u8).collect();
        let mut residues = Vec::new();
        med_forward(&plane, 37, &mut residues);
        assert_ne!(residues, plane);
        med_inverse(&mut residues, 37);
        assert_eq!(residues, plane);
    }

    #[test]
    fn every_format_and_codec_round_trips() {
        let noise = |x: u32, y: u32| (x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(40_503)) as u8;
        let gray = raster(300, 270, PixelFormat::Gray8, |x, y| vec![(x / 3 + y) as u8]);
        let rgba = raster(300, 270, PixelFormat::Rgba8, |x, y| {
            vec![noise(x, y), x as u8, y as u8, 255]
        });
        let samples: Vec<f32> = (0..90 * 70 * 4).map(|i| i as f32 * 0.25 - 3.0).collect();
        let float = RasterTile2D::from_f32(90, 70, samples).unwrap();
        let (back, _) = round_trip(
            &[gray.clone(), rgba.clone(), float.clone()],
            &PixelOptions::SAVE,
        );
        assert_eq!(back, vec![gray, rgba, float]);
    }

    #[test]
    fn a_noisy_tile_is_stored_raw_and_a_smooth_one_deflated() {
        let mut state = 0x2545_f491_4f6c_dd1d_u64;
        let noise: Vec<u8> = (0..64 * 64 * 4)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 24) as u8
            })
            .collect();
        assert_eq!(encode_tile(&noise, PixelFormat::Rgba8, 64, 3).0, CODEC_RAW);
        let smooth: Vec<u8> = (0..64 * 64 * 4).map(|i: u32| (i / 256) as u8).collect();
        let (codec, blob) = encode_tile(&smooth, PixelFormat::Rgba8, 64, 3);
        assert_eq!(codec, CODEC_PLANES);
        assert!(blob.len() * 20 < smooth.len());
    }

    /// Every key is the same unless the salt moves it: a collision of the
    /// hash on demand.
    fn colliding(salt: u64, description: &[u8], pixels: &[u8]) -> Key {
        if salt == 0 {
            [7; MAX_KEY_WIDTH]
        } else {
            sha256_key(salt, description, pixels)
        }
    }

    #[test]
    fn a_forced_key_collision_stores_two_blobs() {
        let a = raster(40, 30, PixelFormat::Rgba8, |x, y| {
            vec![x as u8, y as u8, 0, 255]
        });
        let b = raster(40, 30, PixelFormat::Rgba8, |x, y| {
            vec![y as u8, x as u8, 9, 255]
        });
        let mut writer = PixelWriter::new(&PixelOptions::SAVE).with_hasher(colliding);
        let mut out = Writer::new();
        for raster in [&a, &b, &a] {
            writer.write_raster(raster, &mut out);
        }
        let store = writer.finish().unwrap();
        let reader = PixelReader::parse(&store).unwrap();
        assert_eq!(reader.len(), 2, "two different tiles, one shared key");
        assert!(reader.blobs.contains_key(&key_of(&[7; 16])));
        let bytes = out.into_bytes();
        let mut input = Reader::new(&bytes);
        for expected in [&a, &b, &a] {
            assert_eq!(&reader.read_raster(&mut input).unwrap().unwrap(), expected);
        }
    }

    #[test]
    fn a_key_of_an_old_blob_with_other_pixels_is_salted_too() {
        let a = raster(16, 16, PixelFormat::Gray8, |x, y| vec![(x * y) as u8]);
        let b = raster(16, 16, PixelFormat::Gray8, |x, y| vec![(x + y) as u8]);
        let mut writer = PixelWriter::new(&PixelOptions::SAVE).with_hasher(colliding);
        writer.write_raster(&a, &mut Writer::new());
        let old = writer.finish().unwrap();
        let old = PixelReader::parse(&old).unwrap();
        let mut writer =
            PixelWriter::keeping(&old, None, &PixelOptions::SAVE).with_hasher(colliding);
        let mut out = Writer::new();
        writer.write_raster(&b, &mut out);
        writer.write_raster(&a, &mut out);
        let store = writer.finish().unwrap();
        let reader = PixelReader::parse(&store).unwrap();
        assert_eq!(reader.len(), 2, "the old blob and the new one");
        let bytes = out.into_bytes();
        let mut input = Reader::new(&bytes);
        assert_eq!(reader.read_raster(&mut input).unwrap().unwrap(), b);
        assert_eq!(reader.read_raster(&mut input).unwrap().unwrap(), a);
    }

    #[test]
    fn tiles_of_one_value_cost_no_blob() {
        let empty = RasterTile2D::new(600, 300, PixelFormat::Rgba8).unwrap();
        let solid = raster(600, 300, PixelFormat::Rgba8, |_, _| vec![10, 20, 30, 40]);
        let negative_zero = RasterTile2D::from_f32(30, 20, vec![-0.0; 30 * 20 * 4]).unwrap();
        let (back, blobs) = round_trip(
            &[empty.clone(), solid.clone(), negative_zero.clone()],
            &PixelOptions::SAVE,
        );
        assert_eq!(blobs, 0);
        assert_eq!(back[..2], [empty, solid]);
        // Bit for bit: minus zero is a value, not an empty tile.
        let samples = back[2].f32_samples().unwrap();
        assert!(
            samples
                .iter()
                .all(|sample| sample.to_bits() == (-0.0_f32).to_bits())
        );
        assert_eq!(back[2], negative_zero);
    }
}
