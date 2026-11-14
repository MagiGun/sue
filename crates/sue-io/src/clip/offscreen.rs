//! A stored bitmap ("offscreen"): its description in the database, and
//! its pixels in an `Exta` chunk as 256 by 256 tiles, each zlib compressed
//! as an alpha plane followed by BGRx (or one plane for a mask).

use super::container::{be32, put_string, put32, utf16};
use crate::psd::PsdLayer;

pub(super) const TILE: usize = 256;
pub(super) const BEGIN: &str = "BlockDataBeginChunk";
pub(super) const END: &str = "BlockDataEndChunk";

/// One bitmap's description: its size in pixels, its tile grid, its
/// channels (4 for color with alpha, 1 for a mask) and the value of
/// pixels no tile covers.
pub(super) struct Bitmap {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) columns: usize,
    pub(super) rows: usize,
    pub(super) channels: usize,
    pub(super) fill: [u8; 4],
}

pub(super) fn bitmap_of(attribute: &[u8]) -> Option<Bitmap> {
    let mut at = 16;
    let skip_string = |at: &mut usize| -> Option<()> {
        let len = be32(attribute, *at)? as usize;
        *at += 4 + 2 * len;
        Some(())
    };
    skip_string(&mut at)?;
    let width = be32(attribute, at)? as usize;
    let height = be32(attribute, at + 4)? as usize;
    let columns = be32(attribute, at + 8)? as usize;
    let rows = be32(attribute, at + 12)? as usize;
    at += 16;
    let first = be32(attribute, at + 4)? as usize;
    let second = be32(attribute, at + 8)? as usize;
    at += 64;
    skip_string(&mut at)?;
    let white = be32(attribute, at + 4)? != 0;
    let channels = if (first, second) == (1, 4) { 4 } else { 1 };
    let fill = if white { [255; 4] } else { [0; 4] };
    Some(Bitmap {
        width,
        height,
        columns,
        rows,
        channels,
        fill,
    })
}

/// The tiles of a bitmap chunk, in grid order (`None` for empty ones).
pub(super) fn tiles(payload: &[u8]) -> Result<Vec<Option<&[u8]>>, String> {
    let begin = utf16(BEGIN);
    let mut at = 0;
    let mut out = Vec::new();
    while at + 8 < payload.len() {
        // Each tile: its size, then its name; the status and checksum
        // lists after the tiles have no size.
        if payload.get(at + 8..at + 8 + begin.len()) != Some(begin.as_slice()) {
            break;
        }
        let size = be32(payload, at).ok_or("a tile is short")? as usize;
        let tile = payload
            .get(at..at + size)
            .ok_or("a tile runs past its chunk")?;
        let body = tile.get(8 + begin.len()..).ok_or("a tile is short")?;
        let has = be32(body, 16).ok_or("a tile is short")?;
        if has == 1 {
            let len = be32(body, 20).ok_or("a tile is short")? as usize;
            let data = body.get(28..24 + len).ok_or("a tile's data runs past it")?;
            out.push(Some(data));
        } else {
            out.push(None);
        }
        at += size;
    }
    Ok(out)
}

/// Decodes a bitmap: straight RGBA8 (or one byte per pixel for a mask).
pub(super) fn decode_bitmap(bitmap: &Bitmap, payload: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let (width, height) = (bitmap.width, bitmap.height);
    if width.saturating_mul(height) > 1 << 31 {
        return Err("a layer is too big".to_owned());
    }
    let mut out: Vec<u8> = if bitmap.channels == 4 {
        bitmap.fill.repeat(width * height)
    } else {
        vec![bitmap.fill[0]; width * height]
    };
    let Some(payload) = payload else {
        return Ok(out);
    };
    let tiles = tiles(payload)?;
    let area = TILE * TILE;
    for (index, tile) in tiles.iter().enumerate() {
        let Some(tile) = tile else {
            continue;
        };
        if index >= bitmap.columns * bitmap.rows {
            break;
        }
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib(tile)
            .map_err(|error| format!("a tile does not decompress: {error:?}"))?;
        let (tx, ty) = (index % bitmap.columns * TILE, index / bitmap.columns * TILE);
        if bitmap.channels == 4 {
            if raw.len() < area * 5 {
                return Err("a tile is smaller than it should be".to_owned());
            }
            for y in 0..TILE.min(height.saturating_sub(ty)) {
                for x in 0..TILE.min(width.saturating_sub(tx)) {
                    let t = y * TILE + x;
                    let o = ((ty + y) * width + tx + x) * 4;
                    let c = area + t * 4;
                    out[o..o + 4].copy_from_slice(&[raw[c + 2], raw[c + 1], raw[c], raw[t]]);
                }
            }
        } else {
            if raw.len() < area {
                return Err("a mask tile is smaller than it should be".to_owned());
            }
            for y in 0..TILE.min(height.saturating_sub(ty)) {
                let row = (ty + y) * width + tx;
                let len = TILE.min(width.saturating_sub(tx));
                out[row..row + len].copy_from_slice(&raw[y * TILE..y * TILE + len]);
            }
        }
    }
    Ok(out)
}

/// The offscreen description of a `width` by `height` color bitmap whose
/// tiles compressed to `sizes` bytes.
pub(super) fn attribute(width: u32, height: u32, white: bool, sizes: &[u32]) -> Vec<u8> {
    let (columns, rows) = (width.div_ceil(256), height.div_ceil(256));
    let mut parameter = Vec::new();
    put_string(&mut parameter, "Parameter");
    for value in [width, height, columns, rows] {
        put32(&mut parameter, value);
    }
    for value in [
        33, 1, 4, 5, 65_536, 4, 1024, 1, 256, 65_536, 256, 256, 8, 8, 0, 0,
    ] {
        put32(&mut parameter, value);
    }
    let mut init = Vec::new();
    put_string(&mut init, "InitColor");
    let values: &[u32] = if white {
        &[20, 1, u32::MAX, 4, 4, 0, 0, 0, 0]
    } else {
        &[20, 0, 0, 0, 4]
    };
    for value in values {
        put32(&mut init, *value);
    }
    let mut sizes_part = Vec::new();
    put_string(&mut sizes_part, "BlockSize");
    put32(&mut sizes_part, 12);
    put32(&mut sizes_part, columns * rows);
    put32(&mut sizes_part, 4);
    for i in 0..(columns * rows) as usize {
        put32(&mut sizes_part, sizes.get(i).copied().unwrap_or(0x68));
    }
    let mut out = Vec::new();
    put32(&mut out, 16);
    for part in [&parameter, &init, &sizes_part] {
        put32(&mut out, u32::try_from(part.len()).unwrap_or(0));
    }
    out.extend_from_slice(&parameter);
    out.extend_from_slice(&init);
    out.extend_from_slice(&sizes_part);
    out
}

/// The tiles of a canvas sized bitmap holding `layer`'s pixels where it
/// sits, and their compressed sizes. A layer with no pixels still gets its
/// (empty) tiles: Clip Studio calls a pixel layer without them broken.
pub(super) fn encode_bitmap(layer: &PsdLayer, width: u32, height: u32) -> (Vec<u8>, Vec<u32>) {
    let (columns, rows) = (width.div_ceil(256) as usize, height.div_ceil(256) as usize);
    let (mut lw, mut lh) = (layer.width as usize, layer.height as usize);
    if layer.pixels.len() < lw * lh * 4 {
        (lw, lh) = (0, 0);
    }
    let area = TILE * TILE;
    let begin = utf16(BEGIN);
    let end = utf16(END);
    let mut out = Vec::new();
    let mut sizes = Vec::new();
    for index in 0..columns * rows {
        let (tx, ty) = (
            (index % columns * TILE) as i64,
            (index / columns * TILE) as i64,
        );
        let mut raw = vec![0_u8; area * 5];
        let mut has = false;
        for y in 0..TILE as i64 {
            let ly = ty + y - i64::from(layer.top);
            if ly < 0 || ly >= lh as i64 || ty + y >= i64::from(height) {
                continue;
            }
            for x in 0..TILE as i64 {
                let lx = tx + x - i64::from(layer.left);
                if lx < 0 || lx >= lw as i64 || tx + x >= i64::from(width) {
                    continue;
                }
                let o = (ly as usize * lw + lx as usize) * 4;
                let px = &layer.pixels[o..o + 4];
                if px[3] == 0 {
                    continue;
                }
                has = true;
                let t = (y * TILE as i64 + x) as usize;
                raw[t] = px[3];
                let c = area + t * 4;
                raw[c..c + 4].copy_from_slice(&[px[2], px[1], px[0], 255]);
            }
        }
        let mut body = Vec::new();
        put32(&mut body, u32::try_from(index).unwrap_or(0));
        put32(&mut body, u32::try_from(area * 5).unwrap_or(0));
        put32(&mut body, 256);
        put32(&mut body, 256);
        if has {
            let data = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 3);
            let len = u32::try_from(data.len()).unwrap_or(0);
            put32(&mut body, 1);
            put32(&mut body, len + 4);
            body.extend_from_slice(&len.to_le_bytes());
            body.extend_from_slice(&data);
            sizes.push(len);
        } else {
            put32(&mut body, 0);
            sizes.push(0x68);
        }
        let mut tile = Vec::new();
        put32(&mut tile, u32::try_from(BEGIN.len()).unwrap_or(0));
        tile.extend_from_slice(&begin);
        tile.extend_from_slice(&body);
        put32(&mut tile, u32::try_from(END.len()).unwrap_or(0));
        tile.extend_from_slice(&end);
        put32(&mut out, u32::try_from(tile.len() + 4).unwrap_or(0));
        out.extend_from_slice(&tile);
    }
    let count = u32::try_from(columns * rows).unwrap_or(0);
    for (name, value) in [("BlockStatus", 1), ("BlockCheckSum", 0)] {
        put_string(&mut out, name);
        put32(&mut out, 12);
        put32(&mut out, count);
        put32(&mut out, 4);
        for _ in 0..count {
            put32(&mut out, value);
        }
    }
    (out, sizes)
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

    fn dots(width: u32, height: u32) -> PsdLayer {
        let mut pixels = vec![0_u8; (width * height * 4) as usize];
        // One dot in the first tile and one in the last.
        for at in [0, pixels.len() - 4] {
            pixels[at..at + 4].copy_from_slice(&[10, 20, 30, 255]);
        }
        PsdLayer::pixels("Dots", 0, 0, width, height, pixels)
    }

    #[test]
    fn an_attribute_reads_as_its_named_parts() {
        let (_, sizes) = encode_bitmap(&dots(600, 300), 600, 300);
        let blob = attribute(600, 300, true, &sizes);
        let parts = attribute_parts(&blob).unwrap();
        let names: Vec<&str> = parts.iter().map(|part| part.name.as_str()).collect();
        assert_eq!(names, ["Parameter", "InitColor", "BlockSize"]);
        assert_eq!(parts[0].values[..4], [600, 300, 3, 2]);
        assert_eq!(parts[0].values.len(), 20);
        assert_eq!(parts[1].values[..2], [20, 1]);
        assert_eq!(parts[2].values[..3], [12, 6, 4]);
        assert_eq!(parts[2].values[3..], sizes);
        assert!(parts.iter().all(|part| part.rest.is_empty()));
        // What the reader takes from the same blob.
        let bitmap = bitmap_of(&blob).unwrap();
        assert_eq!(
            (bitmap.width, bitmap.height, bitmap.columns, bitmap.rows),
            (600, 300, 3, 2)
        );
        assert_eq!((bitmap.channels, bitmap.fill), (4, [255; 4]));

        // Anything else is not taken for one, whatever its bytes.
        assert_eq!(attribute_parts(b""), None);
        assert_eq!(attribute_parts(b"extrnlid0123456789"), None);
        assert_eq!(attribute_parts(&blob[..blob.len() - 1]), None);
        assert_eq!(attribute_parts(&[0xff; 64]), None);
        let mut unnamed = blob.clone();
        unnamed[16..20].copy_from_slice(&0_u32.to_be_bytes());
        assert_eq!(attribute_parts(&unnamed), None);
    }

    #[test]
    fn a_bitmap_is_surveyed_block_by_block() {
        let (payload, sizes) = encode_bitmap(&dots(600, 300), 600, 300);
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
}
