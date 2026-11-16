//! A stored bitmap ("offscreen"): its description in the database, and
//! its pixels in an `Exta` chunk as 256 by 256 tiles, each zlib compressed
//! as an alpha plane followed by BGRx (or one plane for a mask).

use super::container::{be32, put_string, put32, utf16};
use super::model::{ClipAttribute, ClipBitmap, ClipBlock, ClipBlocks, ClipPixels};

pub(super) const TILE: usize = 256;
pub(super) const BEGIN: &str = "BlockDataBeginChunk";
pub(super) const END: &str = "BlockDataEndChunk";

/// The `Parameter` numbers after the size and the grid, for color with
/// alpha, as Sue has always written them.
const COLOR: [u32; 16] = [
    33, 1, 4, 5, 65_536, 4, 1024, 1, 256, 65_536, 256, 256, 8, 8, 0, 0,
];

/// The same for one channel. Not yet observed in a file Clip Studio
/// wrote (survey 8.1 and 8.2): it is the color description with the
/// color channels taken out, and only "not 1 then 4" is what Sue's reader
/// looks at.
const ONE: [u32; 16] = [
    33, 1, 0, 1, 65_536, 0, 0, 1, 256, 65_536, 256, 256, 8, 8, 0, 0,
];

/// The `BlockSize` of a block with nothing in it, as Sue writes it.
const EMPTY_SIZE: u32 = 0x68;

// ----- The payload of a bitmap chunk -------------------------------------------

/// One block's bytes in a payload.
fn block_bytes(block: &ClipBlock, out: &mut Vec<u8>) {
    let mut tile = Vec::new();
    put32(&mut tile, u32::try_from(BEGIN.len()).unwrap_or(0));
    tile.extend_from_slice(&utf16(BEGIN));
    for value in [block.index, block.raw_size, block.size[0], block.size[1]] {
        put32(&mut tile, value);
    }
    match &block.data {
        Some(data) => {
            let len = u32::try_from(data.len()).unwrap_or(u32::MAX);
            put32(&mut tile, 1);
            put32(&mut tile, len.saturating_add(4));
            tile.extend_from_slice(&len.to_le_bytes());
            tile.extend_from_slice(data);
        }
        None => put32(&mut tile, 0),
    }
    put32(&mut tile, u32::try_from(END.len()).unwrap_or(0));
    tile.extend_from_slice(&utf16(END));
    put32(out, u32::try_from(tile.len() + 4).unwrap_or(u32::MAX));
    out.extend_from_slice(&tile);
}

fn list_bytes(list: &BlockList, out: &mut Vec<u8>) {
    put_string(out, &list.name);
    for value in list.head {
        put32(out, value);
    }
    for value in &list.values {
        put32(out, *value);
    }
}

/// The payload of a bitmap chunk holding `blocks`.
pub(super) fn payload_bytes(blocks: &ClipBlocks) -> Vec<u8> {
    let mut out = Vec::new();
    for block in &blocks.blocks {
        block_bytes(block, &mut out);
    }
    for list in &blocks.lists {
        list_bytes(list, &mut out);
    }
    out.extend_from_slice(&blocks.tail);
    out
}

/// The block that starts at `at`, if one does and it is laid out exactly
/// as [`block_bytes`] lays one out.
fn block_at(payload: &[u8], at: usize) -> Option<(ClipBlock, usize)> {
    let begin = utf16(BEGIN);
    let size = be32(payload, at)? as usize;
    let tile = payload.get(at..at.checked_add(size)?)?;
    if tile.get(8..8 + begin.len())? != begin.as_slice() {
        return None;
    }
    let body = tile.get(8 + begin.len()..)?;
    let number = |i: usize| be32(body, i * 4);
    let data = match number(4)? {
        1 => {
            let len = (number(5)? as usize).checked_sub(4)?;
            Some(body.get(28..28usize.checked_add(len)?)?.to_vec())
        }
        _ => None,
    };
    let block = ClipBlock {
        index: number(0)?,
        raw_size: number(1)?,
        size: [number(2)?, number(3)?],
        data,
    };
    let mut again = Vec::with_capacity(size);
    block_bytes(&block, &mut again);
    (again == tile).then_some((block, size))
}

/// The list that starts at `at`, if one does and it is laid out exactly
/// as [`list_bytes`] lays one out.
fn list_at(payload: &[u8], at: usize) -> Option<(BlockList, usize)> {
    let (name, after) = name_at(payload, at)?;
    let head = [
        be32(payload, after)?,
        be32(payload, after + 4)?,
        be32(payload, after + 8)?,
    ];
    let len = (head[1] as usize).checked_mul(4)?;
    let start = after + 12;
    let raw = payload.get(start..start.checked_add(len)?)?;
    let list = BlockList {
        name,
        head,
        values: raw
            .as_chunks::<4>()
            .0
            .iter()
            .map(|raw| u32::from_be_bytes(*raw))
            .collect(),
    };
    let mut again = Vec::new();
    list_bytes(&list, &mut again);
    (again.as_slice() == payload.get(at..start + len)?).then_some((list, start + len - at))
}

/// The blocks with data at the start of `bytes` that begin with the
/// block name but are not laid out exactly as [`block_bytes`] lays one
/// out, as Sue's reader has always taken them: the size, the name, the
/// block's number, and the data after the "has data" flag and its
/// lengths, whatever follows it in the block.
///
/// # Errors
/// When a block runs past the payload or its data past the block.
fn loose_blocks(bytes: &[u8]) -> Result<Vec<(u32, &[u8])>, String> {
    let begin = utf16(BEGIN);
    let mut at = 0;
    let mut out = Vec::new();
    while at + 8 < bytes.len() {
        if bytes.get(at + 8..at + 8 + begin.len()) != Some(begin.as_slice()) {
            break;
        }
        let size = be32(bytes, at).ok_or("a tile is short")? as usize;
        if size == 0 {
            return Err("a tile is short".to_owned());
        }
        let tile = at
            .checked_add(size)
            .and_then(|end| bytes.get(at..end))
            .ok_or("a tile runs past its chunk")?;
        let body = tile.get(8 + begin.len()..).ok_or("a tile is short")?;
        let index = be32(body, 0).ok_or("a tile is short")?;
        if be32(body, 16).ok_or("a tile is short")? == 1 {
            let len = be32(body, 20).ok_or("a tile is short")? as usize;
            let data = body
                .get(28..24usize.saturating_add(len))
                .ok_or("a tile's data runs past it")?;
            out.push((index, data));
        }
        at += size;
    }
    Ok(out)
}

/// The blocks and lists of a bitmap chunk's payload. What does not read
/// as either stays in `tail`, so [`payload_bytes`] gives `payload` back
/// whatever it holds.
pub(super) fn parse_payload(payload: &[u8]) -> ClipBlocks {
    let mut blocks = ClipBlocks::default();
    let mut at = 0;
    while let Some((block, size)) = block_at(payload, at) {
        blocks.blocks.push(block);
        at += size;
    }
    while let Some((list, size)) = list_at(payload, at) {
        blocks.lists.push(list);
        at += size;
    }
    blocks.tail = payload.get(at..).unwrap_or_default().to_vec();
    blocks
}

/// An attribute blob of `parts`: the length of the list of part sizes,
/// the sizes, then the parts.
pub(super) fn attribute_bytes(parts: &[AttributePart]) -> Vec<u8> {
    let bodies: Vec<Vec<u8>> = parts
        .iter()
        .map(|part| {
            let mut body = Vec::new();
            put_string(&mut body, &part.name);
            for value in &part.values {
                put32(&mut body, *value);
            }
            body.extend_from_slice(&part.rest);
            body
        })
        .collect();
    let mut out = Vec::new();
    put32(&mut out, u32::try_from(4 + 4 * parts.len()).unwrap_or(0));
    for body in &bodies {
        put32(&mut out, u32::try_from(body.len()).unwrap_or(0));
    }
    for body in &bodies {
        out.extend_from_slice(body);
    }
    out
}

/// The attribute's bytes, whichever way it is kept.
pub(super) fn attribute_of(bitmap: &ClipBitmap) -> Vec<u8> {
    match &bitmap.attribute {
        ClipAttribute::Parts(parts) => attribute_bytes(parts),
        ClipAttribute::Raw(raw) => raw.clone(),
    }
}

/// The attribute in `blob`, by parts when it is laid out in them.
pub(super) fn parse_attribute(blob: &[u8]) -> ClipAttribute {
    match attribute_parts(blob) {
        Some(parts) if attribute_bytes(&parts) == blob => ClipAttribute::Parts(parts),
        _ => ClipAttribute::Raw(blob.to_vec()),
    }
}

// ----- Packing and unpacking pixels ---------------------------------------------

fn part(name: &str, values: Vec<u32>) -> AttributePart {
    AttributePart {
        name: name.to_owned(),
        values,
        rest: Vec::new(),
    }
}

/// A bitmap of `width` by `height` whose tile at grid index `i` is
/// `tile(i)` (its bytes, or `None` when empty), `raw` bytes a tile.
fn pack(
    width: u32,
    height: u32,
    channels: &[u32; 16],
    white: bool,
    raw: u32,
    tile: impl Fn(usize) -> Option<Vec<u8>>,
) -> ClipBitmap {
    let (columns, rows) = (width.div_ceil(256), height.div_ceil(256));
    let count = columns as usize * rows as usize;
    let mut blocks = Vec::with_capacity(count);
    let mut sizes = Vec::with_capacity(count);
    for index in 0..count {
        let data = tile(index).map(|raw| miniz_oxide::deflate::compress_to_vec_zlib(&raw, 3));
        sizes.push(data.as_ref().map_or(EMPTY_SIZE, |data| {
            u32::try_from(data.len()).unwrap_or(u32::MAX)
        }));
        blocks.push(ClipBlock {
            index: u32::try_from(index).unwrap_or(u32::MAX),
            raw_size: raw,
            size: [256, 256],
            data,
        });
    }
    let count = u32::try_from(count).unwrap_or(u32::MAX);
    let lists = [("BlockStatus", 1), ("BlockCheckSum", 0)]
        .map(|(name, value)| BlockList {
            name: name.to_owned(),
            head: [12, count, 4],
            values: vec![value; count as usize],
        })
        .to_vec();
    let mut bitmap = empty_with(width, height, channels, white, sizes);
    bitmap.blocks = Some(ClipBlocks {
        blocks,
        lists,
        tail: Vec::new(),
    });
    bitmap
}

fn empty_with(
    width: u32,
    height: u32,
    channels: &[u32; 16],
    white: bool,
    sizes: Vec<u32>,
) -> ClipBitmap {
    let (columns, rows) = (width.div_ceil(256), height.div_ceil(256));
    let mut parameter = vec![width, height, columns, rows];
    parameter.extend_from_slice(channels);
    let init = if white {
        vec![20, 1, u32::MAX, 4, 4, 0, 0, 0, 0]
    } else {
        vec![20, 0, 0, 0, 4]
    };
    let count = columns.saturating_mul(rows);
    let mut block_size = vec![12, count, 4];
    block_size.extend((0..count as usize).map(|i| sizes.get(i).copied().unwrap_or(EMPTY_SIZE)));
    ClipBitmap {
        ids: [0; 3],
        rows: Default::default(),
        external: String::new(),
        attribute: ClipAttribute::Parts(vec![
            part("Parameter", parameter),
            part("InitColor", init),
            part("BlockSize", block_size),
        ]),
        blocks: None,
    }
}

impl ClipBitmap {
    /// A color bitmap described but with no chunk: what a folder has.
    #[must_use]
    pub fn empty(width: u32, height: u32) -> Self {
        empty_with(width, height, &COLOR, false, Vec::new())
    }

    /// A color bitmap of `rgba` (straight RGBA8, `width * height * 4`
    /// bytes; a short buffer reads as clear). Clear pixels keep no color,
    /// as in Clip Studio.
    #[must_use]
    pub fn color(width: u32, height: u32, rgba: &[u8]) -> Self {
        Self::color_placed(width, height, [0, 0], [width, height], rgba)
    }

    /// A color bitmap of `width` by `height`, every pixel opaque `rgb`.
    /// Packed tile by tile, so no picture of the whole bitmap is made.
    #[must_use]
    pub fn filled(width: u32, height: u32, rgb: [u8; 3]) -> Self {
        let (w, h) = (width as usize, height as usize);
        let columns = w.div_ceil(TILE);
        let area = TILE * TILE;
        pack(
            width,
            height,
            &COLOR,
            false,
            u32::try_from(area * 5).unwrap_or(0),
            |index| {
                if columns == 0 {
                    return None;
                }
                let (tx, ty) = (index % columns * TILE, index / columns * TILE);
                let mut raw = vec![0_u8; area * 5];
                for y in 0..TILE.min(h.saturating_sub(ty)) {
                    for x in 0..TILE.min(w.saturating_sub(tx)) {
                        let t = y * TILE + x;
                        raw[t] = 255;
                        let c = area + t * 4;
                        raw[c..c + 4].copy_from_slice(&[rgb[2], rgb[1], rgb[0], 255]);
                    }
                }
                Some(raw)
            },
        )
    }

    /// A color bitmap of `width` by `height` holding `rgba` (straight
    /// RGBA8 of `size[0]` by `size[1]` pixels; a short buffer reads as
    /// clear) with its top left corner at `at`; what it does not cover
    /// is clear. Packed tile by tile, so no picture of the whole bitmap
    /// is ever made.
    #[must_use]
    pub fn color_placed(
        width: u32,
        height: u32,
        at: [i64; 2],
        size: [u32; 2],
        rgba: &[u8],
    ) -> Self {
        let (w, h) = (width as usize, height as usize);
        let (rw, rh) = (size[0] as usize, size[1] as usize);
        let full = rgba.len() >= rw.saturating_mul(rh).saturating_mul(4);
        let columns = w.div_ceil(TILE);
        let area = TILE * TILE;
        pack(
            width,
            height,
            &COLOR,
            false,
            u32::try_from(area * 5).unwrap_or(0),
            |index| {
                if !full || columns == 0 || rw == 0 || rh == 0 {
                    return None;
                }
                let (tx, ty) = (index % columns * TILE, index / columns * TILE);
                let mut raw = vec![0_u8; area * 5];
                let mut has = false;
                for y in 0..TILE.min(h.saturating_sub(ty)) {
                    let Some(ry) = usize::try_from((ty + y) as i64 - at[1])
                        .ok()
                        .filter(|ry| *ry < rh)
                    else {
                        continue;
                    };
                    for x in 0..TILE.min(w.saturating_sub(tx)) {
                        let Some(rx) = usize::try_from((tx + x) as i64 - at[0])
                            .ok()
                            .filter(|rx| *rx < rw)
                        else {
                            continue;
                        };
                        let o = (ry * rw + rx) * 4;
                        let px = &rgba[o..o + 4];
                        if px[3] == 0 {
                            continue;
                        }
                        has = true;
                        let t = y * TILE + x;
                        raw[t] = px[3];
                        let c = area + t * 4;
                        raw[c..c + 4].copy_from_slice(&[px[2], px[1], px[0], 255]);
                    }
                }
                has.then_some(raw)
            },
        )
    }

    /// A one channel bitmap (a mask: 255 shows, 0 hides) of `values`,
    /// `width * height` bytes; pixels no tile covers are white when
    /// `white`, else black. A tile all of that value is left empty.
    #[must_use]
    pub fn gray(width: u32, height: u32, values: &[u8], white: bool) -> Self {
        let (w, h) = (width as usize, height as usize);
        let full = values.len() >= w * h;
        let fill = if white { 255 } else { 0 };
        let columns = w.div_ceil(TILE);
        let area = TILE * TILE;
        pack(
            width,
            height,
            &ONE,
            white,
            u32::try_from(area).unwrap_or(0),
            |index| {
                if !full || columns == 0 {
                    return None;
                }
                let (tx, ty) = (index % columns * TILE, index / columns * TILE);
                let mut raw = vec![fill; area];
                let mut has = false;
                for y in 0..TILE.min(h.saturating_sub(ty)) {
                    let len = TILE.min(w.saturating_sub(tx));
                    let row = &values[(ty + y) * w + tx..(ty + y) * w + tx + len];
                    has |= row.iter().any(|value| *value != fill);
                    raw[y * TILE..y * TILE + len].copy_from_slice(row);
                }
                has.then_some(raw)
            },
        )
    }

    /// The pixels: straight RGBA8 for color, one byte a pixel for one
    /// channel. Pixels no block covers take the fill.
    ///
    /// # Errors
    /// When the description is not one Sue reads, the bitmap is too big,
    /// or a block does not inflate to a whole tile.
    pub fn decode(&self) -> Result<ClipPixels, String> {
        let parameter = self
            .part("Parameter")
            .map(|part| part.values.as_slice())
            .filter(|values| values.len() >= 4)
            .ok_or("a layer's bitmap description is broken")?;
        let (width, height) = (parameter[0] as usize, parameter[1] as usize);
        let (columns, rows) = (parameter[2] as usize, parameter[3] as usize);
        let channels = self.channels().unwrap_or(1);
        // At most a gigabyte of pixels, whatever the file claims.
        if width
            .saturating_mul(height)
            .saturating_mul(usize::from(channels))
            > 1 << 30
        {
            return Err("a layer is too big".to_owned());
        }
        let fill = if self.white() { 255 } else { 0 };
        let mut out = vec![fill; width * height * usize::from(channels)];
        let area = TILE * TILE;
        let need = if channels == 4 { area * 5 } else { area };
        // The blocks laid out exactly as Sue writes them, then any more in
        // the tail that are laid out a little differently (not yet seen in
        // a file Clip Studio wrote): those are read as Sue always read
        // them, so their pixels are not lost.
        let mut tiles: Vec<(u32, &[u8])> = Vec::new();
        if let Some(blocks) = &self.blocks {
            tiles.extend(
                blocks
                    .blocks
                    .iter()
                    .filter_map(|block| Some((block.index, block.data.as_deref()?))),
            );
            tiles.extend(loose_blocks(&blocks.tail)?);
        }
        for (index, data) in tiles {
            let index = index as usize;
            if columns == 0 || index >= columns.saturating_mul(rows) {
                continue;
            }
            let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, INFLATE_LIMIT)
                .map_err(|error| format!("a tile does not decompress: {error:?}"))?;
            let (tx, ty) = (index % columns * TILE, index / columns * TILE);
            if raw.len() < need {
                return Err(if channels == 4 {
                    "a tile is smaller than it should be"
                } else {
                    "a mask tile is smaller than it should be"
                }
                .to_owned());
            }
            for y in 0..TILE.min(height.saturating_sub(ty)) {
                let len = TILE.min(width.saturating_sub(tx));
                if channels == 4 {
                    for x in 0..len {
                        let t = y * TILE + x;
                        let o = ((ty + y) * width + tx + x) * 4;
                        let c = area + t * 4;
                        out[o..o + 4].copy_from_slice(&[raw[c + 2], raw[c + 1], raw[c], raw[t]]);
                    }
                } else {
                    let row = (ty + y) * width + tx;
                    out[row..row + len].copy_from_slice(&raw[y * TILE..y * TILE + len]);
                }
            }
        }
        Ok(ClipPixels {
            width: u32::try_from(width).unwrap_or(0),
            height: u32::try_from(height).unwrap_or(0),
            channels,
            data: out,
        })
    }
}

// ----- Looking at a bitmap without decoding it --------------------------------

/// One named part of an attribute blob: its name and the numbers after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttributePart {
    /// The part's name, such as `Parameter`, `InitColor` or `BlockSize`.
    pub name: String,
    /// The 32-bit numbers that follow the name.
    pub values: Vec<u32>,
    /// Bytes left at the end that do not fill a number.
    pub rest: Vec<u8>,
}

/// A counted UTF-16 name of letters and digits at `at`, and where it ends.
fn name_at(bytes: &[u8], at: usize) -> Option<(String, usize)> {
    let len = be32(bytes, at)? as usize;
    if len == 0 || len > 256 {
        return None;
    }
    let end = at.checked_add(4 + 2 * len)?;
    let raw = bytes.get(at + 4..end)?;
    let units: Vec<u16> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_be_bytes(*pair))
        .collect();
    let name = String::from_utf16(&units).ok()?;
    name.chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        .then_some((name, end))
}

/// Reads a blob laid out as an offscreen's `Attribute` is: the length of
/// a list of part sizes, the sizes, then the parts, each a UTF-16 name
/// followed by 32-bit numbers. `None` when `blob` is not laid out so.
#[must_use]
pub fn attribute_parts(blob: &[u8]) -> Option<Vec<AttributePart>> {
    let head = be32(blob, 0)? as usize;
    if head < 8 || !head.is_multiple_of(4) || head > 1024 || head > blob.len() {
        return None;
    }
    let sizes: Vec<usize> = (1..head / 4)
        .map(|i| be32(blob, i * 4).map(|size| size as usize))
        .collect::<Option<_>>()?;
    let total = sizes
        .iter()
        .try_fold(head, |sum, size| sum.checked_add(*size))?;
    if total != blob.len() {
        return None;
    }
    let mut at = head;
    let mut parts = Vec::new();
    for size in sizes {
        let part = blob.get(at..at + size)?;
        let (name, after) = name_at(part, 0)?;
        let (numbers, rest) = part[after..].as_chunks::<4>();
        parts.push(AttributePart {
            name,
            values: numbers.iter().map(|raw| u32::from_be_bytes(*raw)).collect(),
            rest: rest.to_vec(),
        });
        at += size;
    }
    Some(parts)
}

/// One block (tile) of a stored bitmap, as its header describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// The block's number in the grid.
    pub index: u32,
    /// The size the header gives for the block's pixels before compression.
    pub raw_size: u32,
    /// The two numbers after it (256 and 256 in what Sue writes).
    pub size: [u32; 2],
    /// The length of the compressed data, when the block has any.
    pub compressed: Option<usize>,
    /// What the data inflates to: 327,680 bytes for color with alpha,
    /// 65,536 for one channel. `None` without data, when it was not
    /// asked for, or when the data does not inflate.
    pub inflated: Option<usize>,
}

/// One list after the blocks of a stored bitmap, such as `BlockStatus`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockList {
    /// Its name.
    pub name: String,
    /// The three numbers of its header (12, the count, 4 in what Sue
    /// writes).
    pub head: [u32; 3],
    /// Its values, one per block.
    pub values: Vec<u32>,
}

/// What a stored bitmap's payload holds, block by block.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BitmapSurvey {
    /// The blocks, in file order.
    pub blocks: Vec<Block>,
    /// The lists after the blocks.
    pub lists: Vec<BlockList>,
    /// Bytes at the end that are neither.
    pub leftover: usize,
}

/// The most a block may inflate to here: four times a color tile.
const INFLATE_LIMIT: usize = TILE * TILE * 5 * 4;

/// Walks the payload of a stored bitmap (an `Exta` chunk's) and says what
/// is in it, without building any picture. With `inflate` each block's
/// data is decompressed to learn its real size.
#[must_use]
pub fn survey_bitmap(payload: &[u8], inflate: bool) -> BitmapSurvey {
    let begin = utf16(BEGIN);
    let mut survey = BitmapSurvey::default();
    let mut at = 0;
    while payload.get(at + 8..at + 8 + begin.len()) == Some(begin.as_slice()) {
        let Some(tile) = be32(payload, at)
            .map(|size| size as usize)
            .filter(|size| *size >= 8 + begin.len())
            .and_then(|size| payload.get(at..at.checked_add(size)?))
        else {
            break;
        };
        let body = &tile[8 + begin.len()..];
        let number = |i: usize| be32(body, i * 4).unwrap_or(0);
        let data = (number(4) == 1)
            .then(|| body.get(28..24 + number(5) as usize))
            .flatten();
        survey.blocks.push(Block {
            index: number(0),
            raw_size: number(1),
            size: [number(2), number(3)],
            compressed: data.map(<[u8]>::len),
            inflated: data.filter(|_| inflate).and_then(|data| {
                miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, INFLATE_LIMIT)
                    .ok()
                    .map(|raw| raw.len())
            }),
        });
        at += tile.len();
    }
    while let Some((name, after)) = name_at(payload, at) {
        let (Some(a), Some(count), Some(b)) = (
            be32(payload, after),
            be32(payload, after + 4),
            be32(payload, after + 8),
        ) else {
            break;
        };
        let Some(raw) = (count as usize)
            .checked_mul(4)
            .and_then(|len| payload.get(after + 12..(after + 12).checked_add(len)?))
        else {
            break;
        };
        survey.lists.push(BlockList {
            name,
            head: [a, count, b],
            values: raw
                .as_chunks::<4>()
                .0
                .iter()
                .map(|raw| u32::from_be_bytes(*raw))
                .collect(),
        });
        at = after + 12 + raw.len();
    }
    survey.leftover = payload.len().saturating_sub(at);
    survey
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 600 by 300 picture with one dot in the first tile and one in the
    /// last.
    fn dots() -> ClipBitmap {
        let (width, height) = (600_u32, 300_u32);
        let mut pixels = vec![0_u8; (width * height * 4) as usize];
        for at in [0, pixels.len() - 4] {
            pixels[at..at + 4].copy_from_slice(&[10, 20, 30, 255]);
        }
        ClipBitmap::color(width, height, &pixels)
    }

    fn sizes(bitmap: &ClipBitmap) -> Vec<u32> {
        bitmap
            .blocks
            .as_ref()
            .unwrap()
            .blocks
            .iter()
            .map(|block| {
                block
                    .data
                    .as_ref()
                    .map_or(EMPTY_SIZE, |data| data.len() as u32)
            })
            .collect()
    }

    #[test]
    fn an_attribute_reads_as_its_named_parts() {
        let bitmap = dots();
        let blob = attribute_of(&bitmap);
        let parts = attribute_parts(&blob).unwrap();
        let names: Vec<&str> = parts.iter().map(|part| part.name.as_str()).collect();
        assert_eq!(names, ["Parameter", "InitColor", "BlockSize"]);
        assert_eq!(parts[0].values[..4], [600, 300, 3, 2]);
        assert_eq!(parts[0].values.len(), 20);
        assert_eq!(parts[1].values, [20, 0, 0, 0, 4]);
        assert_eq!(parts[2].values[..3], [12, 6, 4]);
        assert_eq!(parts[2].values[3..], sizes(&bitmap));
        assert!(parts.iter().all(|part| part.rest.is_empty()));
        assert_eq!(parse_attribute(&blob), bitmap.attribute);
        assert_eq!(
            (bitmap.size(), bitmap.channels()),
            (Some([600, 300]), Some(4))
        );
        assert!(!bitmap.white());

        // Anything else is not taken for one, whatever its bytes, and is
        // kept as it is.
        assert_eq!(attribute_parts(b""), None);
        assert_eq!(attribute_parts(b"extrnlid0123456789"), None);
        assert_eq!(attribute_parts(&blob[..blob.len() - 1]), None);
        assert_eq!(attribute_parts(&[0xff; 64]), None);
        let mut unnamed = blob.clone();
        unnamed[16..20].copy_from_slice(&0_u32.to_be_bytes());
        assert_eq!(attribute_parts(&unnamed), None);
        assert_eq!(
            parse_attribute(&unnamed),
            ClipAttribute::Raw(unnamed.clone())
        );
    }

    #[test]
    fn a_bitmap_is_surveyed_block_by_block() {
        let bitmap = dots();
        let payload = payload_bytes(bitmap.blocks.as_ref().unwrap());
        let sizes = sizes(&bitmap);
        let survey = survey_bitmap(&payload, true);
        assert_eq!(survey.blocks.len(), 6);
        assert_eq!(survey.leftover, 0);
        for (i, block) in survey.blocks.iter().enumerate() {
            assert_eq!(block.index as usize, i);
            assert_eq!((block.raw_size, block.size), (327_680, [256, 256]));
            let has = i == 0 || i == 5;
            assert_eq!(block.compressed.is_some(), has, "block {i}");
            assert_eq!(block.inflated, has.then_some(327_680), "block {i}");
            if has {
                assert_eq!(block.compressed, Some(sizes[i] as usize));
            }
        }
        let lists: Vec<(&str, [u32; 3], &[u32])> = survey
            .lists
            .iter()
            .map(|list| (list.name.as_str(), list.head, list.values.as_slice()))
            .collect();
        assert_eq!(
            lists,
            [
                ("BlockStatus", [12, 6, 4], [1; 6].as_slice()),
                ("BlockCheckSum", [12, 6, 4], [0; 6].as_slice()),
            ]
        );
        assert_eq!(survey_bitmap(&payload, false).blocks[0].inflated, None);

        // Cut anywhere, or not a bitmap at all: an answer, never a crash.
        for cut in (0..payload.len()).step_by(97) {
            let part = survey_bitmap(&payload[..cut], true);
            assert!(part.blocks.len() <= 6);
        }
        let junk = survey_bitmap(b"not a bitmap", true);
        assert_eq!((junk.blocks.len(), junk.leftover), (0, 12));
        let mut bad = payload.clone();
        let data = 8 + utf16(BEGIN).len() + 28;
        bad[data..data + 8].copy_from_slice(&[0xff; 8]);
        assert_eq!(survey_bitmap(&bad, true).blocks[0].inflated, None);
    }

    #[test]
    fn a_payload_reads_back_to_the_same_bytes_whatever_it_holds() {
        let payload = payload_bytes(dots().blocks.as_ref().unwrap());
        let read = parse_payload(&payload);
        assert_eq!((read.blocks.len(), read.lists.len()), (6, 2));
        assert!(read.tail.is_empty());
        assert_eq!(payload_bytes(&read), payload);
        // Cut, flipped or junk: what does not read is kept as the tail.
        for cut in (0..payload.len()).step_by(53) {
            let part = parse_payload(&payload[..cut]);
            assert_eq!(payload_bytes(&part), &payload[..cut]);
        }
        for at in (0..payload.len()).step_by(31) {
            let mut flipped = payload.clone();
            flipped[at] ^= 0x10;
            assert_eq!(payload_bytes(&parse_payload(&flipped)), flipped);
        }
        assert_eq!(payload_bytes(&parse_payload(b"junk")), b"junk");
    }

    #[test]
    fn pixels_pack_and_unpack_in_color_and_in_one_channel() {
        let (width, height) = (300_u32, 257_u32);
        let mut rgba = Vec::new();
        let mut values = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let alpha = if (x + y) % 3 == 0 {
                    0
                } else {
                    (x * 7 + y) as u8 | 1
                };
                rgba.extend_from_slice(&[x as u8, y as u8, (x ^ y) as u8, alpha]);
                values.push(if x > 280 { 255 } else { (x + 2 * y) as u8 });
            }
        }
        let color = ClipBitmap::color(width, height, &rgba);
        let back = color.decode().unwrap();
        assert_eq!((back.width, back.height, back.channels), (300, 257, 4));
        for (i, (want, got)) in rgba.chunks(4).zip(back.data.chunks(4)).enumerate() {
            if want[3] == 0 {
                assert_eq!(got, [0; 4], "pixel {i}");
            } else {
                assert_eq!(got, want, "pixel {i}");
            }
        }
        for white in [false, true] {
            let gray = ClipBitmap::gray(width, height, &values, white);
            assert_eq!((gray.channels(), gray.white()), (Some(1), white));
            let back = gray.decode().unwrap();
            assert_eq!((back.channels, back.data), (1, values.clone()));
        }
        // An all white mask has no block with data; an all clear picture
        // neither, and still has its blocks.
        let blank = ClipBitmap::gray(width, height, &vec![255; values.len()], true);
        let blocks = &blank.blocks.as_ref().unwrap().blocks;
        assert_eq!(blocks.len(), 4);
        assert!(blocks.iter().all(|block| block.data.is_none()));
        assert_eq!(blank.decode().unwrap().data, vec![255; values.len()]);
        let clear = ClipBitmap::color(4, 4, &[0; 64]);
        assert_eq!(clear.decode().unwrap().data, [0; 64]);
        // A folder's bitmap has no chunk and decodes clear.
        let empty = ClipBitmap::empty(3, 2);
        assert!(empty.blocks.is_none());
        assert_eq!(empty.decode().unwrap().data, [0; 24]);
    }

    #[test]
    fn blocks_laid_out_a_little_differently_still_decode() {
        // Four more bytes before the end name of the first block: the
        // payload no longer parses block by block, but its pixels are read
        // as Sue's reader has always read them, and its bytes are kept.
        let bitmap = dots();
        let payload = payload_bytes(bitmap.blocks.as_ref().unwrap());
        let size = be32(&payload, 0).unwrap() as usize;
        let end_name = size - 4 - 2 * END.len();
        let mut odd = payload[..end_name].to_vec();
        odd.extend_from_slice(&[0; 4]);
        odd.extend_from_slice(&payload[end_name..]);
        odd[..4].copy_from_slice(&(size as u32 + 4).to_be_bytes());
        let blocks = parse_payload(&odd);
        assert!(blocks.blocks.is_empty());
        assert_eq!(payload_bytes(&blocks), odd);
        let read = ClipBitmap {
            blocks: Some(blocks),
            ..bitmap.clone()
        };
        assert_eq!(read.decode().unwrap(), bitmap.decode().unwrap());
        // A block that runs past the payload is refused.
        let cut = ClipBitmap {
            blocks: Some(parse_payload(&odd[..size])),
            ..bitmap
        };
        assert!(cut.decode().is_err());
    }

    #[test]
    fn broken_blocks_are_refused_with_a_reason() {
        let mut bitmap = dots();
        let blocks = bitmap.blocks.as_mut().unwrap();
        blocks.blocks[0].data = Some(b"not zlib".to_vec());
        assert!(bitmap.decode().is_err());
        let mut short = dots();
        short.blocks.as_mut().unwrap().blocks[0].data =
            Some(miniz_oxide::deflate::compress_to_vec_zlib(&[1; 100], 3));
        assert!(short.decode().is_err());
        let raw = ClipBitmap {
            attribute: ClipAttribute::Raw(b"odd".to_vec()),
            ..dots()
        };
        assert!(raw.decode().is_err());
        let mut huge = ClipBitmap::empty(1, 1);
        if let ClipAttribute::Parts(parts) = &mut huge.attribute {
            parts[0].values[..2].copy_from_slice(&[u32::MAX, u32::MAX]);
        }
        assert!(huge.decode().is_err());
    }
}
