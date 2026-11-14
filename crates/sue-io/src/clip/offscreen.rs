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
