//! Clip Studio Paint files (`.clip`), read and written with their layers,
//! folders, blend modes, opacity, visibility and clipping, as the same
//! [`Psd`] records Photoshop documents use.
//!
//! A `.clip` is a chunk file: a `CSFCHUNK` header, a `Head` chunk, one
//! `Exta` chunk per stored bitmap, an SQLite database (`SQLi`) that
//! describes the canvas and its layers, and an empty `Foot`. A layer's
//! pixels are reached through the database (layer, mipmap, mipmap level,
//! offscreen) and sit in an `Exta` chunk as 256 by 256 tiles, each zlib
//! compressed as an alpha plane followed by BGRx.
//!
//! Correction layers and gradient or fill layers, which Clip Studio draws
//! from their settings, come in as empty layers in their places; text and
//! vector layers come in as the pixels Clip Studio last drew for them.
//!
//! A file Sue writes also carries the Sue project of the picture in a
//! `SueProject` table, so Sue opens its own `.clip` files with nothing lost
//! (vector lines, tones, effects), while Clip Studio sees the layers.

use std::collections::HashMap;

use crate::psd::{Psd, PsdKind, PsdLayer};
use crate::sqlite::{self, NewTable, Table, Value};

/// A Clip Studio file read: its layers as Photoshop records, its
/// resolution, and the Sue project inside when Sue wrote it.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    /// The canvas and its layers (bottom first, folders as in [`Psd`]);
    /// `merged` is left empty.
    pub psd: Psd,
    /// Pixels per inch.
    pub dpi: f64,
    /// The `.sue` bytes of the picture, in files Sue wrote.
    pub sue: Option<Vec<u8>>,
}

const TILE: usize = 256;
const BEGIN: &str = "BlockDataBeginChunk";
const END: &str = "BlockDataEndChunk";

/// Clip Studio's layer blend modes by number, as Photoshop keys.
const BLENDS: [(i64, &[u8; 4]); 31] = [
    (0, b"norm"),
    (1, b"dark"),
    (2, b"mul "),
    (3, b"idiv"),
    (4, b"lbrn"),
    (5, b"fsub"),
    (6, b"dkCl"),
    (7, b"lite"),
    (8, b"scrn"),
    (9, b"div "),
    (10, b"div "),
    (11, b"lddg"),
    (12, b"lddg"),
    (13, b"lgCl"),
    (14, b"over"),
    (15, b"sLit"),
    (16, b"hLit"),
    (17, b"vLit"),
    (18, b"lLit"),
    (19, b"pLit"),
    (20, b"hMix"),
    (21, b"diff"),
    (22, b"smud"),
    (23, b"hue "),
    (24, b"sat "),
    (25, b"colr"),
    (26, b"lum "),
    (30, b"pass"),
    (36, b"fdiv"),
    (-1, b"norm"),
    (-2, b"norm"),
];

fn blend_of(composite: i64) -> [u8; 4] {
    BLENDS
        .iter()
        .find(|(number, _)| *number == composite)
        .map_or(*b"norm", |(_, key)| **key)
}

fn composite_of(key: [u8; 4]) -> i64 {
    // The first number listed for a key is the one Clip Studio uses.
    BLENDS
        .iter()
        .find(|(number, k)| **k == key && *number >= 0)
        .map_or(0, |(number, _)| *number)
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn be64(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

/// `true` if `head` starts a Clip Studio file.
#[must_use]
pub fn sniff(head: &[u8]) -> bool {
    head.starts_with(b"CSFCHUNK")
}

// ----- Reading ---------------------------------------------------------------

/// The chunks of a `.clip`: the `Exta` payloads by id, and the database.
/// A `.clip`'s bitmap chunks by id.
type Bitmaps<'a> = HashMap<String, &'a [u8]>;

fn chunks(bytes: &[u8]) -> Result<(Bitmaps<'_>, &[u8]), String> {
    if !sniff(bytes) {
        return Err("not a Clip Studio file".to_owned());
    }
    let mut at = usize::try_from(be64(bytes, 16).ok_or("the file is cut short")?)
        .map_err(|_| "the file header is broken")?;
    let mut exta = HashMap::new();
    let mut database = None;
    while at + 16 <= bytes.len() {
        if &bytes[at..at + 4] != b"CHNK" {
            return Err(format!("no chunk where one should start, at byte {at}"));
        }
        let kind = &bytes[at + 4..at + 8];
        let size = usize::try_from(be64(bytes, at + 8).ok_or("a chunk is cut short")?)
            .map_err(|_| "a chunk is too big")?;
        let data = bytes
            .get(at + 16..at + 16 + size)
            .ok_or("a chunk runs past the end of the file")?;
        match kind {
            b"Exta" => {
                let len = usize::try_from(be64(data, 0).ok_or("a bitmap chunk is short")?)
                    .map_err(|_| "a bitmap chunk is broken")?;
                let id = data.get(8..8 + len).ok_or("a bitmap chunk is short")?;
                let payload = data.get(8 + len + 8..).ok_or("a bitmap chunk is short")?;
                exta.insert(String::from_utf8_lossy(id).into_owned(), payload);
            }
            b"SQLi" => database = Some(data),
            _ => {}
        }
        at += 16 + size;
    }
    Ok((exta, database.ok_or("the file has no layer database")?))
}

/// One bitmap's description: its size in pixels, its tile grid, its
/// channels (4 for color with alpha, 1 for a mask) and the value of
/// pixels no tile covers.
struct Bitmap {
    width: usize,
    height: usize,
    columns: usize,
    rows: usize,
    channels: usize,
    fill: [u8; 4],
}

fn bitmap_of(attribute: &[u8]) -> Option<Bitmap> {
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
fn tiles(payload: &[u8]) -> Result<Vec<Option<&[u8]>>, String> {
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
fn decode_bitmap(bitmap: &Bitmap, payload: Option<&[u8]>) -> Result<Vec<u8>, String> {
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

/// A table's rows by `MainId`.
struct Rows<'a> {
    table: Option<&'a Table>,
    by_id: HashMap<i64, usize>,
}

impl<'a> Rows<'a> {
    fn new(tables: &'a [Table], name: &str) -> Self {
        let table = tables.iter().find(|table| table.name == name);
        let by_id = table
            .and_then(|table| {
                let key = table.column("MainId")?;
                Some(
                    table
                        .rows
                        .iter()
                        .enumerate()
                        .filter_map(|(i, (_, row))| Some((row.get(key)?.int()?, i)))
                        .collect(),
                )
            })
            .unwrap_or_default();
        Self { table, by_id }
    }

    fn get(&self, id: i64, column: &str) -> Option<&'a Value> {
        let table = self.table?;
        let row = &table.rows.get(*self.by_id.get(&id)?)?.1;
        row.get(table.column(column)?)
    }

    fn int(&self, id: i64, column: &str) -> i64 {
        self.get(id, column).and_then(Value::int).unwrap_or(0)
    }
}

struct Source<'a> {
    exta: HashMap<String, &'a [u8]>,
    layers: Rows<'a>,
    mipmaps: Rows<'a>,
    levels: Rows<'a>,
    offscreens: Rows<'a>,
    width: u32,
    height: u32,
}

impl Source<'_> {
    /// The bitmap and pixels of a layer's mipmap (`LayerRenderMipmap` or
    /// `LayerLayerMaskMipmap`).
    fn pixels(&self, mipmap: i64) -> Result<Option<(Bitmap, Vec<u8>)>, String> {
        if mipmap == 0 {
            return Ok(None);
        }
        let level = self.mipmaps.int(mipmap, "BaseMipmapInfo");
        let offscreen = self.levels.int(level, "Offscreen");
        let Some(attribute) = self
            .offscreens
            .get(offscreen, "Attribute")
            .and_then(Value::bytes)
        else {
            return Ok(None);
        };
        let Some(bitmap) = bitmap_of(attribute) else {
            return Err("a layer's bitmap description is broken".to_owned());
        };
        let id = self
            .offscreens
            .get(offscreen, "BlockData")
            .and_then(Value::bytes)
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            .unwrap_or_default();
        let pixels = decode_bitmap(&bitmap, self.exta.get(&id).copied())?;
        Ok(Some((bitmap, pixels)))
    }

    /// The record of one pixel layer, cut to the canvas.
    #[allow(clippy::cast_possible_truncation)]
    fn layer(&self, id: i64) -> Result<Option<PsdLayer>, String> {
        let name = self
            .layers
            .get(id, "LayerName")
            .and_then(Value::text)
            .unwrap_or_default()
            .to_owned();
        // Correction layers keep only their mask as pixels, and gradient
        // and fill layers are drawn from their settings: Sue cannot redo
        // those yet, so they come in as empty layers in their places.
        let generated = ["FilterLayerInfo", "GradationFillInfo"]
            .iter()
            .any(|column| self.layers.get(id, column).and_then(Value::bytes).is_some());
        if generated {
            let mut layer = PsdLayer::pixels(&name, 0, 0, 0, 0, Vec::new());
            self.style(id, &mut layer);
            return Ok(Some(layer));
        }
        let Some((bitmap, mut pixels)) = self.pixels(self.layers.int(id, "LayerRenderMipmap"))?
        else {
            return Ok(None);
        };
        if bitmap.channels != 4 {
            return Ok(None);
        }
        let left =
            self.layers.int(id, "LayerOffsetX") + self.layers.int(id, "LayerRenderOffscrOffsetX");
        let top =
            self.layers.int(id, "LayerOffsetY") + self.layers.int(id, "LayerRenderOffscrOffsetY");
        // A paper layer draws in its own color.
        if self.layers.int(id, "DrawColorEnable") != 0 && self.layers.int(id, "LayerType") == 1584 {
            let channel = |column: &str| (self.layers.int(id, column) >> 24) as u8;
            let color = [
                channel("DrawColorMainRed"),
                channel("DrawColorMainGreen"),
                channel("DrawColorMainBlue"),
            ];
            for pixel in pixels.as_chunks_mut::<4>().0 {
                if pixel[3] > 0 {
                    pixel[..3].copy_from_slice(&color);
                }
            }
        }
        // A layer mask, when on, cuts the alpha.
        let mask_on = self.layers.int(id, "LayerVisibility") & 2 != 0;
        if mask_on
            && let Some((mask, values)) =
                self.pixels(self.layers.int(id, "LayerLayerMaskMipmap"))?
        {
            let mx = self.layers.int(id, "LayerOffsetX")
                + self.layers.int(id, "LayerMaskOffsetX")
                + self.layers.int(id, "LayerMaskOffscrOffsetX");
            let my = self.layers.int(id, "LayerOffsetY")
                + self.layers.int(id, "LayerMaskOffsetY")
                + self.layers.int(id, "LayerMaskOffscrOffsetY");
            for y in 0..bitmap.height {
                for x in 0..bitmap.width {
                    let (cx, cy) = (left + x as i64 - mx, top + y as i64 - my);
                    let value = if cx >= 0
                        && cy >= 0
                        && (cx as usize) < mask.width
                        && (cy as usize) < mask.height
                    {
                        values[cy as usize * mask.width + cx as usize]
                    } else {
                        mask.fill[0]
                    };
                    let a = &mut pixels[(y * bitmap.width + x) * 4 + 3];
                    *a = ((u16::from(*a) * u16::from(value) + 127) / 255) as u8;
                }
            }
        }
        // Cut to the canvas.
        let (cw, ch) = (i64::from(self.width), i64::from(self.height));
        let (x0, y0) = (left.max(0), top.max(0));
        let (x1, y1) = (
            (left + bitmap.width as i64).min(cw),
            (top + bitmap.height as i64).min(ch),
        );
        let (w, h) = ((x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize);
        let mut cut = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            let sy = (y0 - top) as usize + y;
            let sx = (x0 - left) as usize;
            let start = (sy * bitmap.width + sx) * 4;
            cut.extend_from_slice(&pixels[start..start + w * 4]);
        }
        let mut layer = PsdLayer::pixels(&name, x0 as i32, y0 as i32, w as u32, h as u32, cut);
        self.style(id, &mut layer);
        Ok(Some(layer))
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn style(&self, id: i64, layer: &mut PsdLayer) {
        let opacity = self.layers.int(id, "LayerOpacity").clamp(0, 256);
        layer.opacity = ((opacity * 255 + 128) / 256) as u8;
        layer.visible = self.layers.int(id, "LayerVisibility") & 1 != 0;
        layer.clip = self.layers.int(id, "LayerClip") != 0;
        layer.blend = blend_of(self.layers.int(id, "LayerComposite"));
    }

    /// The records of the folder `folder`'s children, bottom first.
    fn children(&self, folder: i64, depth: usize, out: &mut Vec<PsdLayer>) -> Result<(), String> {
        if depth > 64 {
            return Err("the folders nest too deep".to_owned());
        }
        let mut id = self.layers.int(folder, "LayerFirstChildIndex");
        let mut seen = 0;
        while id != 0 {
            seen += 1;
            if seen > 100_000 {
                return Err("the layer list loops".to_owned());
            }
            if self.layers.int(id, "LayerFolder") != 0 {
                out.push(PsdLayer::folder("</Layer group>", PsdKind::FolderEnd));
                self.children(id, depth + 1, out)?;
                let name = self
                    .layers
                    .get(id, "LayerName")
                    .and_then(Value::text)
                    .unwrap_or_default()
                    .to_owned();
                let open = self.layers.int(id, "LayerFolder") & 16 == 0;
                let mut record = PsdLayer::folder(&name, PsdKind::FolderOpen { open });
                self.style(id, &mut record);
                out.push(record);
            } else if let Some(layer) = self.layer(id)? {
                out.push(layer);
            }
            id = self.layers.int(id, "LayerNextIndex");
        }
        Ok(())
    }
}

/// Reads a Clip Studio file.
///
/// # Errors
/// When `bytes` is not a `.clip`, or its database or bitmaps are broken.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn decode_clip(bytes: &[u8]) -> Result<Clip, String> {
    let (exta, database) = chunks(bytes)?;
    let tables = sqlite::read(database)?;
    let canvas = tables
        .iter()
        .find(|table| table.name == "Canvas")
        .and_then(|table| table.rows.first().map(|(_, row)| (table, row)))
        .ok_or("the file has no canvas")?;
    let value = |column: &str| canvas.0.column(column).and_then(|i| canvas.1.get(i));
    let width = value("CanvasWidth")
        .and_then(Value::real)
        .unwrap_or(0.0)
        .round();
    let height = value("CanvasHeight")
        .and_then(Value::real)
        .unwrap_or(0.0)
        .round();
    if !(1.0..=300_000.0).contains(&width) || !(1.0..=300_000.0).contains(&height) {
        return Err(format!(
            "a canvas of {width} by {height} pixels is not valid"
        ));
    }
    let dpi = value("CanvasResolution")
        .and_then(Value::real)
        .unwrap_or(72.0);
    let root = value("CanvasRootFolder").and_then(Value::int).unwrap_or(0);
    let source = Source {
        exta,
        layers: Rows::new(&tables, "Layer"),
        mipmaps: Rows::new(&tables, "Mipmap"),
        levels: Rows::new(&tables, "MipmapInfo"),
        offscreens: Rows::new(&tables, "Offscreen"),
        width: width as u32,
        height: height as u32,
    };
    let mut layers = Vec::new();
    source.children(root, 0, &mut layers)?;
    let sue = tables
        .iter()
        .find(|table| table.name == "SueProject")
        .and_then(|table| {
            let data = table.column("Data")?;
            table.rows.first()?.1.get(data)?.bytes().map(<[u8]>::to_vec)
        });
    Ok(Clip {
        psd: Psd {
            width: width as u32,
            height: height as u32,
            layers,
            merged: Vec::new(),
        },
        dpi,
        sue,
    })
}

// ----- Writing ---------------------------------------------------------------

fn put32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn put_string(out: &mut Vec<u8>, text: &str) {
    put32(out, u32::try_from(text.encode_utf16().count()).unwrap_or(0));
    out.extend_from_slice(&utf16(text));
}

/// The offscreen description of a `width` by `height` color bitmap whose
/// tiles compressed to `sizes` bytes.
fn attribute(width: u32, height: u32, white: bool, sizes: &[u32]) -> Vec<u8> {
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
fn encode_bitmap(layer: &PsdLayer, width: u32, height: u32) -> (Vec<u8>, Vec<u32>) {
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

/// A layer, or a folder of them, as Sue writes it.
enum Node<'a> {
    Pixels(&'a PsdLayer),
    Folder(&'a PsdLayer, Vec<Node<'a>>),
}

/// The tree of a document's records (bottom first in each folder).
fn tree(layers: &[PsdLayer]) -> Vec<Node<'_>> {
    let mut stack: Vec<Vec<Node<'_>>> = vec![Vec::new()];
    for layer in layers {
        match layer.kind {
            PsdKind::FolderEnd => stack.push(Vec::new()),
            PsdKind::FolderOpen { .. } => {
                let children = if stack.len() > 1 {
                    stack.pop().unwrap_or_default()
                } else {
                    Vec::new()
                };
                if let Some(parent) = stack.last_mut() {
                    parent.push(Node::Folder(layer, children));
                }
            }
            PsdKind::Pixels => {
                if let Some(parent) = stack.last_mut() {
                    parent.push(Node::Pixels(layer));
                }
            }
        }
    }
    // Folders left open close at the top.
    while stack.len() > 1 {
        let children = stack.pop().unwrap_or_default();
        if let Some(parent) = stack.last_mut() {
            parent.extend(children);
        }
    }
    stack.pop().unwrap_or_default()
}

/// A stable id in Clip Studio's spelling, from `seed`.
fn uuid(seed: u64) -> [u8; 16] {
    let mut state = seed ^ 0x9e37_79b9_7f4a_7c15;
    let mut out = [0_u8; 16];
    for chunk in out.chunks_mut(8) {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        chunk.copy_from_slice(&(z ^ (z >> 31)).to_be_bytes());
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

struct Writer {
    width: u32,
    height: u32,
    next: i64,
    seed: u64,
    layers: Vec<(i64, Vec<Value>)>,
    mipmaps: Vec<(i64, Vec<Value>)>,
    levels: Vec<(i64, Vec<Value>)>,
    offscreens: Vec<(i64, Vec<Value>)>,
    thumbnails: Vec<(i64, Vec<Value>)>,
    exta: Vec<(String, Vec<u8>)>,
}

const LAYER_COLUMNS: &str = "MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, LayerName TEXT DEFAULT NULL, LayerType INTEGER DEFAULT NULL, LayerLock INTEGER DEFAULT NULL, LayerClip INTEGER DEFAULT NULL, LayerMasking INTEGER DEFAULT NULL, LayerOffsetX INTEGER DEFAULT NULL, LayerOffsetY INTEGER DEFAULT NULL, LayerRenderOffscrOffsetX INTEGER DEFAULT NULL, LayerRenderOffscrOffsetY INTEGER DEFAULT NULL, LayerMaskOffsetX INTEGER DEFAULT NULL, LayerMaskOffsetY INTEGER DEFAULT NULL, LayerMaskOffscrOffsetX INTEGER DEFAULT NULL, LayerMaskOffscrOffsetY INTEGER DEFAULT NULL, LayerOpacity INTEGER DEFAULT NULL, LayerComposite INTEGER DEFAULT NULL, LayerUsePaletteColor INTEGER DEFAULT NULL, LayerNoticeablePaletteColor INTEGER DEFAULT NULL, LayerPaletteRed INTEGER DEFAULT NULL, LayerPaletteGreen INTEGER DEFAULT NULL, LayerPaletteBlue INTEGER DEFAULT NULL, LayerFolder INTEGER DEFAULT NULL, LayerVisibility INTEGER DEFAULT NULL, LayerSelect INTEGER DEFAULT NULL, LayerNextIndex INTEGER DEFAULT NULL, LayerFirstChildIndex INTEGER DEFAULT NULL, LayerUuid TEXT DEFAULT NULL, LayerRenderMipmap INTEGER DEFAULT NULL, LayerLayerMaskMipmap INTEGER DEFAULT NULL, LayerRenderThumbnail INTEGER DEFAULT NULL, LayerLayerMaskThumbnail INTEGER DEFAULT NULL";

impl Writer {
    fn id(&mut self) -> i64 {
        self.next += 1;
        self.next
    }

    /// The bitmap rows of one layer; answers its mipmap id.
    fn bitmap(&mut self, layer_id: i64, pixels: Option<&PsdLayer>) -> i64 {
        let (offscreen, level, mipmap) = (self.id(), self.id(), self.id());
        self.seed = self.seed.wrapping_add(1);
        let external = format!("extrnlid{}", hex(&uuid(self.seed)).to_uppercase());
        let encoded = pixels.map(|layer| encode_bitmap(layer, self.width, self.height));
        let sizes = encoded
            .as_ref()
            .map(|(_, sizes)| sizes.clone())
            .unwrap_or_default();
        if let Some((payload, _)) = encoded {
            self.exta.push((external.clone(), payload));
        }
        let rowid = |rows: &Vec<(i64, Vec<Value>)>| i64::try_from(rows.len()).unwrap_or(0) + 1;
        let row = rowid(&self.offscreens);
        self.offscreens.push((
            row,
            vec![
                Value::Null,
                Value::Integer(offscreen),
                Value::Integer(1),
                Value::Integer(layer_id),
                Value::Blob(attribute(self.width, self.height, false, &sizes)),
                Value::Blob(external.into_bytes()),
            ],
        ));
        let row = rowid(&self.levels);
        self.levels.push((
            row,
            vec![
                Value::Null,
                Value::Integer(level),
                Value::Integer(1),
                Value::Integer(layer_id),
                Value::Real(100.0),
                Value::Integer(offscreen),
                Value::Integer(0),
            ],
        ));
        let row = rowid(&self.mipmaps);
        self.mipmaps.push((
            row,
            vec![
                Value::Null,
                Value::Integer(mipmap),
                Value::Integer(1),
                Value::Integer(layer_id),
                Value::Integer(1),
                Value::Integer(level),
            ],
        ));
        let row = rowid(&self.thumbnails);
        self.thumbnails.push((
            row,
            vec![
                Value::Null,
                Value::Integer(mipmap),
                Value::Integer(1),
                Value::Integer(layer_id),
                Value::Integer(i64::from(self.width)),
                Value::Integer(i64::from(self.height)),
                Value::Integer(offscreen),
                Value::Integer(0),
                Value::Integer(-1),
            ],
        ));
        mipmap
    }

    /// One layer row.
    #[allow(clippy::too_many_arguments)]
    fn layer_row(
        &mut self,
        id: i64,
        record: Option<&PsdLayer>,
        kind: i64,
        folder: i64,
        next: i64,
        first_child: i64,
        mipmap: i64,
    ) {
        let (name, opacity, visible, clip, blend) =
            record.map_or((String::new(), 256, true, false, *b"norm"), |layer| {
                (
                    layer.name.clone(),
                    (i64::from(layer.opacity) * 256 + 127) / 255,
                    layer.visible,
                    layer.clip,
                    layer.blend,
                )
            });
        self.seed = self.seed.wrapping_add(1);
        let id_bytes = hex(&uuid(self.seed));
        let layer_uuid = format!(
            "{}-{}-{}-{}-{}",
            &id_bytes[..10],
            &id_bytes[10..14],
            &id_bytes[14..18],
            &id_bytes[18..22],
            &id_bytes[22..32]
        );
        let row = i64::try_from(self.layers.len()).unwrap_or(0) + 1;
        let int = Value::Integer;
        self.layers.push((
            row,
            vec![
                Value::Null,
                int(id),
                int(1),
                Value::Text(name),
                int(kind),
                int(0),
                int(i64::from(clip)),
                int(if id == 2 { 32 } else { 0 }),
                int(0),
                int(0),
                int(0),
                int(0),
                int(0),
                int(0),
                int(0),
                int(0),
                int(opacity.clamp(0, 256)),
                int(composite_of(blend)),
                int(0),
                int(0),
                int(0),
                int(0),
                int(0),
                int(folder),
                int(i64::from(visible)),
                int(0),
                int(next),
                int(first_child),
                Value::Text(layer_uuid),
                int(mipmap),
                int(0),
                int(mipmap),
                int(0),
            ],
        ));
    }

    /// Writes `nodes` (bottom first) as the children of a folder; answers
    /// the first child's id.
    fn nodes(&mut self, nodes: &[Node<'_>]) -> i64 {
        let ids: Vec<i64> = nodes.iter().map(|_| self.id()).collect();
        for (i, node) in nodes.iter().enumerate() {
            let id = ids[i];
            let next = ids.get(i + 1).copied().unwrap_or(0);
            match node {
                Node::Pixels(layer) => {
                    let mipmap = self.bitmap(id, Some(layer));
                    self.layer_row(id, Some(layer), 1, 0, next, 0, mipmap);
                }
                Node::Folder(layer, children) => {
                    let first = self.nodes(children);
                    let mipmap = self.bitmap(id, None);
                    let closed = matches!(layer.kind, PsdKind::FolderOpen { open: false });
                    let folder = if closed { 17 } else { 1 };
                    self.layer_row(id, Some(layer), 0, folder, next, first, mipmap);
                }
            }
        }
        ids.first().copied().unwrap_or(0)
    }
}

fn table(name: &str, columns: &str, rows: Vec<(i64, Vec<Value>)>) -> NewTable {
    NewTable {
        name: name.to_owned(),
        sql: format!("CREATE TABLE {name}(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, {columns})"),
        rows,
    }
}

/// Writes `psd` as a Clip Studio file at `dpi`, carrying `sue` (the
/// picture's `.sue` bytes) for Sue to open again with nothing lost.
///
/// # Errors
/// When the canvas is empty or too large.
pub fn encode_clip(psd: &Psd, dpi: f64, sue: Option<&[u8]>) -> Result<Vec<u8>, String> {
    if psd.width == 0 || psd.height == 0 || psd.width > 300_000 || psd.height > 300_000 {
        return Err(format!(
            "a canvas of {} by {} pixels cannot be saved",
            psd.width, psd.height
        ));
    }
    let seed = u64::from(psd.width) << 32 ^ u64::from(psd.height) ^ psd.layers.len() as u64;
    let mut writer = Writer {
        width: psd.width,
        height: psd.height,
        next: 2,
        seed,
        layers: Vec::new(),
        mipmaps: Vec::new(),
        levels: Vec::new(),
        offscreens: Vec::new(),
        thumbnails: Vec::new(),
        exta: Vec::new(),
    };
    let nodes = tree(&psd.layers);
    let first = writer.nodes(&nodes);
    let top = writer.layers.iter().rev().find_map(|(_, row)| {
        (row.get(23).and_then(Value::int) == Some(0)).then(|| row.get(1).and_then(Value::int))?
    });
    let root_mipmap = writer.bitmap(2, None);
    writer.layer_row(2, None, 256, 1, 0, first, root_mipmap);

    // Lay out the chunks: the bitmaps' offsets go into the database.
    let head_size = 16 + 40;
    let mut offset = 24 + head_size;
    let mut exta_chunks = Vec::new();
    let mut external = Vec::new();
    for (id, payload) in &writer.exta {
        let mut data = Vec::with_capacity(payload.len() + 64);
        data.extend_from_slice(&(id.len() as u64).to_be_bytes());
        data.extend_from_slice(id.as_bytes());
        data.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        data.extend_from_slice(payload);
        let mut chunk = b"CHNKExta".to_vec();
        chunk.extend_from_slice(&(data.len() as u64).to_be_bytes());
        chunk.extend_from_slice(&data);
        external.push((
            i64::try_from(external.len()).unwrap_or(0) + 1,
            vec![
                Value::Text(id.clone()),
                Value::Integer(i64::try_from(offset).unwrap_or(0)),
            ],
        ));
        offset += chunk.len();
        exta_chunks.push(chunk);
    }
    let int = Value::Integer;
    let mut tables = vec![
        table(
            "Project",
            "ProjectInternalVersion TEXT DEFAULT NULL, ProjectName TEXT DEFAULT NULL, ProjectCanvas INTEGER DEFAULT NULL, ProjectItemBank INTEGER DEFAULT NULL, ProjectCutBank INTEGER DEFAULT NULL, ProjectLayerCompManager INTEGER DEFAULT NULL, ProjectRootCanvasNode INTEGER DEFAULT NULL",
            vec![(
                1,
                vec![
                    Value::Null,
                    Value::Text("1.1.0".to_owned()),
                    Value::Text("Sue".to_owned()),
                    int(1),
                    int(1),
                    int(2),
                    int(3),
                    int(0),
                ],
            )],
        ),
        table(
            "CanvasItemBank",
            "MainId INTEGER DEFAULT NULL, ModelBankMainIndex INTEGER DEFAULT NULL, BankRootItemMainIndex INTEGER DEFAULT NULL",
            vec![(1, vec![Value::Null, int(1), int(0), int(0)])],
        ),
        table(
            "AnimationCutBank",
            "MainId INTEGER DEFAULT NULL, FirstTimeLine INTEGER DEFAULT NULL, FirstScenario INTEGER DEFAULT NULL, CurrentIndex INTEGER DEFAULT NULL, Enable INTEGER DEFAULT NULL",
            vec![(1, vec![Value::Null, int(2), int(0), int(0), int(0), int(0)])],
        ),
        table(
            "LayerCompManager",
            "MainId INTEGER DEFAULT NULL, FirstLayerCompIndex INTEGER DEFAULT NULL, LastStateLayerCompIndex INTEGER DEFAULT NULL, AppliedLayerCompIndex INTEGER DEFAULT NULL",
            vec![(1, vec![Value::Null, int(3), int(0), int(0), int(-1)])],
        ),
        table(
            "Canvas",
            "MainId INTEGER DEFAULT NULL, CanvasUnit INTEGER DEFAULT NULL, CanvasWidth REAL DEFAULT NULL, CanvasHeight REAL DEFAULT NULL, CanvasResolution REAL DEFAULT NULL, CanvasChannelBytes INTEGER DEFAULT NULL, CanvasDefaultChannelOrder INTEGER DEFAULT NULL, CanvasRootFolder INTEGER DEFAULT NULL, CanvasCurrentLayer INTEGER DEFAULT NULL, CanvasDoSimulateColor INTEGER DEFAULT NULL, CanvasUseColorAdjustment INTEGER DEFAULT NULL, CanvasDefaultColorTypeIndex INTEGER DEFAULT NULL, CanvasDefaultColorBlackChecked INTEGER DEFAULT NULL, CanvasDefaultColorWhiteChecked INTEGER DEFAULT NULL, CanvasDefaultToneLine REAL DEFAULT NULL, CanvasDoublePage INTEGER DEFAULT NULL, CanvasRenderMipmapForceSaved INTEGER DEFAULT NULL, CanvasWorkTime INTEGER DEFAULT NULL",
            vec![(
                1,
                vec![
                    Value::Null,
                    int(1),
                    int(0),
                    Value::Real(f64::from(psd.width)),
                    Value::Real(f64::from(psd.height)),
                    Value::Real(if dpi > 0.0 { dpi } else { 72.0 }),
                    int(1),
                    int(33),
                    int(2),
                    int(top.unwrap_or(0)),
                    int(0),
                    int(0),
                    int(0),
                    int(1),
                    int(1),
                    Value::Real(60.0),
                    int(0),
                    int(0),
                    int(0),
                ],
            )],
        ),
        table("Layer", LAYER_COLUMNS, std::mem::take(&mut writer.layers)),
        table(
            "Mipmap",
            "MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, LayerId INTEGER DEFAULT NULL, MipmapCount INTEGER DEFAULT NULL, BaseMipmapInfo INTEGER DEFAULT NULL",
            std::mem::take(&mut writer.mipmaps),
        ),
        table(
            "MipmapInfo",
            "MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, LayerId INTEGER DEFAULT NULL, ThisScale REAL DEFAULT NULL, Offscreen INTEGER DEFAULT NULL, NextIndex INTEGER DEFAULT NULL",
            std::mem::take(&mut writer.levels),
        ),
        table(
            "Offscreen",
            "MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, LayerId INTEGER DEFAULT NULL, Attribute BLOB DEFAULT NULL, BlockData BLOB DEFAULT NULL",
            std::mem::take(&mut writer.offscreens),
        ),
        table(
            "LayerThumbnail",
            "MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, LayerId INTEGER DEFAULT NULL, ThumbnailCanvasWidth INTEGER DEFAULT NULL, ThumbnailCanvasHeight INTEGER DEFAULT NULL, ThumbnailOffscreen INTEGER DEFAULT NULL, ThumbnailDrewMode INTEGER DEFAULT NULL, ThumbnailFixMode INTEGER DEFAULT NULL",
            std::mem::take(&mut writer.thumbnails),
        ),
        NewTable {
            name: "ExternalChunk".to_owned(),
            sql: "CREATE TABLE ExternalChunk(ExternalID BLOB, Offset INTEGER)".to_owned(),
            rows: external,
        },
        NewTable {
            name: "ExternalTableAndColumnName".to_owned(),
            sql: "CREATE TABLE ExternalTableAndColumnName(TableName TEXT, ColumnName TEXT)"
                .to_owned(),
            rows: vec![(
                1,
                vec![
                    Value::Text("Offscreen".to_owned()),
                    Value::Text("BlockData".to_owned()),
                ],
            )],
        },
    ];
    if let Some(sue) = sue {
        tables.push(NewTable {
            name: "SueProject".to_owned(),
            sql: "CREATE TABLE SueProject(Version INTEGER, Data BLOB)".to_owned(),
            rows: vec![(1, vec![int(1), Value::Blob(sue.to_vec())])],
        });
    }
    let database = sqlite::write(&tables);

    let mut head = b"CHNKHead".to_vec();
    head.extend_from_slice(&40_u64.to_be_bytes());
    head.extend_from_slice(&256_u64.to_be_bytes());
    head.extend_from_slice(&(offset as u64).to_be_bytes());
    head.extend_from_slice(&16_u64.to_be_bytes());
    head.extend_from_slice(&uuid(seed ^ database.len() as u64));
    let total = 24
        + head.len()
        + exta_chunks.iter().map(Vec::len).sum::<usize>()
        + 16
        + database.len()
        + 16;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"CSFCHUNK");
    out.extend_from_slice(&(total as u64).to_be_bytes());
    out.extend_from_slice(&24_u64.to_be_bytes());
    out.extend_from_slice(&head);
    for chunk in &exta_chunks {
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(b"CHNKSQLi");
    out.extend_from_slice(&(database.len() as u64).to_be_bytes());
    out.extend_from_slice(&database);
    out.extend_from_slice(b"CHNKFoot");
    out.extend_from_slice(&0_u64.to_be_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker(name: &str, left: i32, top: i32, width: u32, height: u32) -> PsdLayer {
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let on = (x / 7 + y / 5) % 2 == 0;
                pixels.extend_from_slice(&[
                    (x * 255 / width) as u8,
                    (y * 255 / height) as u8,
                    90,
                    if on { 255 } else { 0 },
                ]);
            }
        }
        PsdLayer::pixels(name, left, top, width, height, pixels)
    }

    /// `layer`'s pixel at (x, y) of the canvas, or clear.
    fn at(layer: &PsdLayer, x: i32, y: i32) -> [u8; 4] {
        let (lx, ly) = (x - layer.left, y - layer.top);
        if lx < 0 || ly < 0 || lx >= layer.width as i32 || ly >= layer.height as i32 {
            return [0; 4];
        }
        let o = (ly as usize * layer.width as usize + lx as usize) * 4;
        layer.pixels[o..o + 4].try_into().unwrap()
    }

    #[test]
    fn layers_and_folders_round_trip_through_a_clip_file() {
        let mut square = checker("Square", 200, 100, 300, 280);
        square.opacity = 200;
        square.blend = *b"mul ";
        square.clip = true;
        let mut hidden = checker("Hidden \u{65e5}\u{672c}", -40, -30, 90, 60);
        hidden.visible = false;
        let mut folder = PsdLayer::folder("Inks", PsdKind::FolderOpen { open: false });
        folder.opacity = 128;
        let psd = Psd {
            width: 600,
            height: 400,
            layers: vec![
                checker("Back", 0, 0, 600, 400),
                PsdLayer::folder("</Layer group>", PsdKind::FolderEnd),
                hidden.clone(),
                square.clone(),
                folder,
                PsdLayer::pixels("Empty", 0, 0, 0, 0, Vec::new()),
            ],
            merged: Vec::new(),
        };
        let bytes = encode_clip(&psd, 350.0, Some(b"SUE project")).unwrap();
        assert!(sniff(&bytes));
        let clip = decode_clip(&bytes).unwrap();
        assert_eq!(
            (clip.psd.width, clip.psd.height, clip.dpi),
            (600, 400, 350.0)
        );
        assert_eq!(clip.sue.as_deref(), Some(b"SUE project".as_slice()));
        let names: Vec<&str> = clip.psd.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Back",
                "</Layer group>",
                "Hidden \u{65e5}\u{672c}",
                "Square",
                "Inks",
                "Empty"
            ]
        );
        let back = &clip.psd.layers;
        assert_eq!(back[4].kind, PsdKind::FolderOpen { open: false });
        assert_eq!(back[4].opacity, 128);
        assert_eq!(
            (back[3].opacity, back[3].blend, back[3].clip),
            (200, *b"mul ", true)
        );
        assert!(!back[2].visible);
        // Every pixel lands where it was, cut to the canvas.
        for (original, read) in [
            (&psd.layers[0], &back[0]),
            (&square, &back[3]),
            (&hidden, &back[2]),
        ] {
            for (x, y) in [
                (0, 0),
                (1, 0),
                (7, 5),
                (250, 150),
                (499, 379),
                (599, 399),
                (10, 10),
                (45, 25),
            ] {
                let want = at(original, x, y);
                let got = at(read, x, y);
                if want[3] == 0 {
                    assert_eq!(got[3], 0, "{} at {x},{y}", original.name);
                } else {
                    assert_eq!(got, want, "{} at {x},{y}", original.name);
                }
            }
        }
        assert!(back[2].left >= 0 && back[2].top >= 0);
    }

    #[test]
    fn broken_files_are_refused_with_a_reason() {
        assert!(decode_clip(b"CSFCHUNK").is_err());
        assert!(decode_clip(b"8BPS").is_err());
        let psd = Psd {
            width: 10,
            height: 10,
            layers: vec![checker("A", 0, 0, 10, 10)],
            merged: Vec::new(),
        };
        let mut bytes = encode_clip(&psd, 72.0, None).unwrap();
        let cut = bytes.len() - 40;
        bytes.truncate(cut);
        assert!(decode_clip(&bytes).is_err());
        assert!(encode_clip(&Psd { width: 0, ..psd }, 72.0, None).is_err());
    }

    #[test]
    fn blend_modes_map_both_ways() {
        for key in [
            *b"norm", *b"mul ", *b"scrn", *b"over", *b"diff", *b"lum ", *b"pass",
        ] {
            assert_eq!(blend_of(composite_of(key)), key);
        }
        assert_eq!(blend_of(999), *b"norm");
    }
}
