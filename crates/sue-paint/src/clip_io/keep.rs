//! The bytes a `clip.foreign` component holds: one layer of a `.clip`
//! as it was read (its row, its bitmaps and their rows, without what it
//! holds), or the parts of a whole `.clip` that belong to no layer (the
//! canvas row, the `Head` chunk, the root folder's row, and everything
//! the reader kept as foreign). `docs/clip-foreign.md` lays them out.
//!
//! Each starts with a version byte. A model value is built from a
//! constructor of `sue_io::clip` and then filled in, so a field the model
//! gains later starts at its default instead of breaking this file.

use sue_core::codec::{CodecError, Reader, Writer};
use sue_io::clip::parts;
use sue_io::clip::{
    AttributePart, BlockList, ClipAttribute, ClipBitmap, ClipBlock, ClipBlocks, ClipChunk,
    ClipDocument, ClipDrawColor, ClipForeign, ClipHead, ClipLayer, ClipMask, ClipRow, ClipShape,
    ClipTable, Columns,
};
use sue_io::sqlite::Value;

/// The version of both encodings. Version 2 adds, after what version 1
/// held, the typed parts of `.clip` model P3-18 (a layer's content,
/// effects, palette color, rulers and thumbnail; the document's comps,
/// timeline and preview), each in the encoding of `sue_io::clip::parts`.
/// Version 3 adds, after what version 2 held for a picture, the
/// fingerprint of what Sue made of the document's comps, rulers and
/// timeline when it opened the file (a flag, then 32 bytes when set).
const VERSION: u8 = 3;

/// Whether a kept value of `version` is one this build reads.
const fn reads(version: u8) -> bool {
    version >= 1 && version <= VERSION
}

fn value(out: &mut Writer, value: &Value) {
    match value {
        Value::Null => out.u8(0),
        Value::Integer(number) => {
            out.u8(1);
            out.varint_signed(*number);
        }
        Value::Real(number) => {
            out.u8(2);
            out.f64(*number);
        }
        Value::Text(text) => {
            out.u8(3);
            out.str(text);
        }
        Value::Blob(bytes) => {
            out.u8(4);
            out.bytes(bytes);
        }
    }
}

fn read_value(input: &mut Reader<'_>) -> Result<Value, CodecError> {
    Ok(match input.u8()? {
        0 => Value::Null,
        1 => Value::Integer(input.varint_signed()?),
        2 => Value::Real(input.f64()?),
        3 => Value::Text(input.str()?),
        4 => Value::Blob(input.bytes()?.to_vec()),
        other => return Err(input.error(format!("unknown value kind {other}"))),
    })
}

fn values(out: &mut Writer, list: &[Value]) {
    out.len(list.len());
    for item in list {
        value(out, item);
    }
}

fn read_values(input: &mut Reader<'_>) -> Result<Vec<Value>, CodecError> {
    let count = input.len(1)?;
    (0..count).map(|_| read_value(input)).collect()
}

fn columns(out: &mut Writer, columns: &Columns) {
    out.len(columns.len());
    for (name, item) in columns {
        out.str(name);
        value(out, item);
    }
}

fn read_columns(input: &mut Reader<'_>) -> Result<Columns, CodecError> {
    let count = input.len(2)?;
    (0..count)
        .map(|_| Ok((input.str()?, read_value(input)?)))
        .collect()
}

fn strings(out: &mut Writer, list: &[String]) {
    out.len(list.len());
    for item in list {
        out.str(item);
    }
}

fn read_strings(input: &mut Reader<'_>) -> Result<Vec<String>, CodecError> {
    let count = input.len(1)?;
    (0..count).map(|_| input.str()).collect()
}

fn numbers(out: &mut Writer, list: &[u32]) {
    out.len(list.len());
    for item in list {
        out.u32(*item);
    }
}

fn read_numbers(input: &mut Reader<'_>) -> Result<Vec<u32>, CodecError> {
    let count = input.len(4)?;
    (0..count).map(|_| input.u32()).collect()
}

fn option_bytes(out: &mut Writer, bytes: Option<&[u8]>) {
    out.bool(bytes.is_some());
    if let Some(bytes) = bytes {
        out.bytes(bytes);
    }
}

fn read_option_bytes(input: &mut Reader<'_>) -> Result<Option<Vec<u8>>, CodecError> {
    Ok(if input.bool()? {
        Some(input.bytes()?.to_vec())
    } else {
        None
    })
}

fn bitmap(out: &mut Writer, bitmap: &ClipBitmap) {
    for id in bitmap.ids {
        out.varint_signed(id);
    }
    for rows in &bitmap.rows {
        columns(out, rows);
    }
    out.str(&bitmap.external);
    match &bitmap.attribute {
        ClipAttribute::Parts(parts) => {
            out.u8(0);
            out.len(parts.len());
            for part in parts {
                out.str(&part.name);
                numbers(out, &part.values);
                out.bytes(&part.rest);
            }
        }
        ClipAttribute::Raw(bytes) => {
            out.u8(1);
            out.bytes(bytes);
        }
    }
    out.bool(bitmap.blocks.is_some());
    if let Some(blocks) = &bitmap.blocks {
        out.len(blocks.blocks.len());
        for block in &blocks.blocks {
            out.u32(block.index);
            out.u32(block.raw_size);
            out.u32(block.size[0]);
            out.u32(block.size[1]);
            option_bytes(out, block.data.as_deref());
        }
        out.len(blocks.lists.len());
        for list in &blocks.lists {
            out.str(&list.name);
            for number in list.head {
                out.u32(number);
            }
            numbers(out, &list.values);
        }
        out.bytes(&blocks.tail);
    }
}

fn read_bitmap(input: &mut Reader<'_>) -> Result<ClipBitmap, CodecError> {
    let mut out = ClipBitmap::empty(0, 0);
    for id in &mut out.ids {
        *id = input.varint_signed()?;
    }
    for rows in &mut out.rows {
        *rows = read_columns(input)?;
    }
    out.external = input.str()?;
    out.attribute = match input.u8()? {
        0 => {
            let count = input.len(3)?;
            let mut parts = Vec::with_capacity(count);
            for _ in 0..count {
                parts.push(AttributePart {
                    name: input.str()?,
                    values: read_numbers(input)?,
                    rest: input.bytes()?.to_vec(),
                });
            }
            ClipAttribute::Parts(parts)
        }
        1 => ClipAttribute::Raw(input.bytes()?.to_vec()),
        other => return Err(input.error(format!("unknown attribute kind {other}"))),
    };
    out.blocks = if input.bool()? {
        let count = input.len(17)?;
        let mut blocks = Vec::with_capacity(count);
        for _ in 0..count {
            blocks.push(ClipBlock {
                index: input.u32()?,
                raw_size: input.u32()?,
                size: [input.u32()?, input.u32()?],
                data: read_option_bytes(input)?,
            });
        }
        let count = input.len(14)?;
        let mut lists = Vec::with_capacity(count);
        for _ in 0..count {
            lists.push(BlockList {
                name: input.str()?,
                head: [input.u32()?, input.u32()?, input.u32()?],
                values: read_numbers(input)?,
            });
        }
        Some(ClipBlocks {
            blocks,
            lists,
            tail: input.bytes()?.to_vec(),
        })
    } else {
        None
    };
    Ok(out)
}

fn layer_into(out: &mut Writer, layer: &ClipLayer) {
    out.varint_signed(layer.id);
    out.str(&layer.name);
    for number in [
        layer.layer_type,
        layer.folder,
        layer.lock,
        layer.clip,
        layer.opacity,
        layer.composite,
        layer.visibility,
        layer.offset[0],
        layer.offset[1],
        layer.render_offset[0],
        layer.render_offset[1],
        layer.draw_color.enable,
        layer.draw_color.main[0],
        layer.draw_color.main[1],
        layer.draw_color.main[2],
    ] {
        out.varint_signed(number);
    }
    out.str(&layer.uuid);
    out.bool(layer.draft);
    out.bool(layer.reference);
    out.bool(layer.pixels.is_some());
    if let Some(pixels) = &layer.pixels {
        bitmap(out, pixels);
    }
    out.bool(layer.mask.is_some());
    if let Some(mask) = &layer.mask {
        for number in [
            mask.offset[0],
            mask.offset[1],
            mask.offscreen_offset[0],
            mask.offscreen_offset[1],
        ] {
            out.varint_signed(number);
        }
        bitmap(out, &mask.bitmap);
    }
    columns(out, &layer.columns);
    // Version 2: the typed parts.
    out.bool(layer.content.is_some());
    if let Some(content) = &layer.content {
        out.str(content.kind());
        out.bytes(&parts::encode_content(content));
    }
    option_bytes(
        out,
        layer.effects.as_ref().map(parts::encode_effects).as_deref(),
    );
    option_bytes(out, layer.palette_color.map(parts::encode_color).as_deref());
    out.bytes(&parts::encode_rulers(&layer.rulers));
    option_bytes(
        out,
        layer
            .thumbnail
            .as_ref()
            .map(parts::encode_picture)
            .as_deref(),
    );
}

/// Reads the typed parts version 2 added after a layer's columns.
fn read_layer_parts(input: &mut Reader<'_>, layer: &mut ClipLayer) -> Result<(), CodecError> {
    layer.content = if input.bool()? {
        let kind = input.str()?;
        Some(parts::decode_content(&kind, input.bytes()?)?)
    } else {
        None
    };
    layer.effects = read_option_bytes(input)?
        .map(|bytes| parts::decode_effects(&bytes))
        .transpose()?;
    layer.palette_color = read_option_bytes(input)?
        .map(|bytes| parts::decode_color(&bytes))
        .transpose()?;
    layer.rulers = parts::decode_rulers(input.bytes()?)?;
    layer.thumbnail = read_option_bytes(input)?
        .map(|bytes| parts::decode_picture(&bytes))
        .transpose()?;
    Ok(())
}

fn read_layer(input: &mut Reader<'_>) -> Result<ClipLayer, CodecError> {
    let mut layer = ClipLayer::folder("");
    layer.id = input.varint_signed()?;
    layer.name = input.str()?;
    let mut number = || input.varint_signed();
    layer.layer_type = number()?;
    layer.folder = number()?;
    layer.lock = number()?;
    layer.clip = number()?;
    layer.opacity = number()?;
    layer.composite = number()?;
    layer.visibility = number()?;
    layer.offset = [number()?, number()?];
    layer.render_offset = [number()?, number()?];
    layer.draw_color = ClipDrawColor {
        enable: number()?,
        main: [number()?, number()?, number()?],
    };
    layer.uuid = input.str()?;
    layer.draft = input.bool()?;
    layer.reference = input.bool()?;
    layer.pixels = if input.bool()? {
        Some(read_bitmap(input)?)
    } else {
        None
    };
    layer.mask = if input.bool()? {
        let mut number = || input.varint_signed();
        let offset = [number()?, number()?];
        let offscreen_offset = [number()?, number()?];
        Some(ClipMask {
            offset,
            offscreen_offset,
            bitmap: read_bitmap(input)?,
        })
    } else {
        None
    };
    layer.columns = read_columns(input)?;
    Ok(layer)
}

/// `layer` as it was read, without what it holds.
#[must_use]
pub(super) fn layer_bytes(layer: &ClipLayer) -> Vec<u8> {
    let mut out = Writer::new();
    out.u8(VERSION);
    layer_into(&mut out, layer);
    out.into_bytes()
}

/// The layer [`layer_bytes`] wrote, holding nothing.
///
/// # Errors
/// When the bytes are not such a layer, or of a newer version.
pub(super) fn layer_from(bytes: &[u8]) -> Result<ClipLayer, CodecError> {
    let mut input = Reader::new(bytes);
    let version = input.u8()?;
    if !reads(version) {
        return Err(input.error(format!("kept layer version {version}")));
    }
    let mut layer = read_layer(&mut input)?;
    if version >= 2 {
        read_layer_parts(&mut input, &mut layer)?;
    }
    if !input.is_empty() {
        return Err(input.error("bytes after a kept layer"));
    }
    Ok(layer)
}

/// What of `doc` belongs to no layer: the canvas, the `Head` chunk, the
/// root folder's row and the foreign data; and `print`, the fingerprint
/// of what Sue made of its comps, rulers and timeline.
#[must_use]
pub(super) fn picture_bytes(doc: &ClipDocument, print: Option<&[u8; 32]>) -> Vec<u8> {
    let mut out = Writer::new();
    out.u8(VERSION);
    out.u32(doc.width);
    out.u32(doc.height);
    out.f64(doc.dpi);
    out.varint_signed(doc.canvas_id);
    columns(&mut out, &doc.canvas_columns);
    match &doc.head {
        ClipHead::Sue { id } => {
            out.u8(0);
            out.raw(id);
        }
        ClipHead::Raw(bytes) => {
            out.u8(1);
            out.bytes(bytes);
        }
    }
    layer_into(&mut out, &doc.root);
    let foreign = &doc.foreign;
    out.len(foreign.shapes.len());
    for shape in &foreign.shapes {
        out.str(&shape.table);
        out.str(&shape.sql);
        strings(&mut out, &shape.columns);
    }
    out.len(foreign.tables.len());
    for table in &foreign.tables {
        out.str(&table.name);
        out.str(&table.sql);
        strings(&mut out, &table.columns);
        out.len(table.rows.len());
        for (rowid, row) in &table.rows {
            out.varint_signed(*rowid);
            values(&mut out, row);
        }
    }
    out.len(foreign.rows.len());
    for row in &foreign.rows {
        out.str(&row.table);
        columns(&mut out, &row.columns);
    }
    out.len(foreign.chunks.len());
    for chunk in &foreign.chunks {
        out.raw(&chunk.kind);
        out.bytes(&chunk.data);
    }
    out.len(foreign.external.len());
    for (id, rows) in &foreign.external {
        out.str(id);
        columns(&mut out, rows);
    }
    columns(&mut out, &foreign.sue_columns);
    out.bytes(&foreign.preamble);
    out.bytes(&foreign.foot);
    out.bytes(&foreign.trailing);
    // Version 2: the document's typed parts.
    out.bytes(&parts::encode_comps(&doc.comps));
    option_bytes(
        &mut out,
        doc.timeline.as_ref().map(parts::encode_timeline).as_deref(),
    );
    option_bytes(
        &mut out,
        doc.preview.as_ref().map(parts::encode_picture).as_deref(),
    );
    // Version 3: the fingerprint of the parts as Sue made them.
    out.bool(print.is_some());
    if let Some(print) = print {
        out.raw(print);
    }
    out.into_bytes()
}

/// The document [`picture_bytes`] wrote: no layers, and no Sue project.
///
/// # Errors
/// When the bytes are not such a document, or of a newer version.
pub(super) fn picture_from(bytes: &[u8]) -> Result<ClipDocument, CodecError> {
    picture_and_print(bytes).map(|(doc, _)| doc)
}

/// The document and the fingerprint [`picture_bytes`] wrote.
///
/// # Errors
/// When the bytes are not such a document, or of a newer version.
pub(super) fn picture_and_print(
    bytes: &[u8],
) -> Result<(ClipDocument, Option<[u8; 32]>), CodecError> {
    let mut input = Reader::new(bytes);
    let version = input.u8()?;
    if !reads(version) {
        return Err(input.error(format!("kept canvas version {version}")));
    }
    let (width, height, dpi) = (input.u32()?, input.u32()?, input.f64()?);
    let mut doc = ClipDocument::new(width, height, dpi);
    doc.canvas_id = input.varint_signed()?;
    doc.canvas_columns = read_columns(&mut input)?;
    doc.head = match input.u8()? {
        0 => {
            let mut id = [0; 16];
            id.copy_from_slice(input.raw(16)?);
            ClipHead::Sue { id }
        }
        1 => ClipHead::Raw(input.bytes()?.to_vec()),
        other => return Err(input.error(format!("unknown head kind {other}"))),
    };
    doc.root = read_layer(&mut input)?;
    if version >= 2 {
        read_layer_parts(&mut input, &mut doc.root)?;
    }
    let mut foreign = ClipForeign::default();
    let count = input.len(3)?;
    for _ in 0..count {
        foreign.shapes.push(ClipShape {
            table: input.str()?,
            sql: input.str()?,
            columns: read_strings(&mut input)?,
        });
    }
    let count = input.len(4)?;
    for _ in 0..count {
        let name = input.str()?;
        let sql = input.str()?;
        let table_columns = read_strings(&mut input)?;
        let rows = input.len(2)?;
        let mut kept = Vec::with_capacity(rows);
        for _ in 0..rows {
            kept.push((input.varint_signed()?, read_values(&mut input)?));
        }
        foreign.tables.push(ClipTable {
            name,
            sql,
            columns: table_columns,
            rows: kept,
        });
    }
    let count = input.len(2)?;
    for _ in 0..count {
        foreign.rows.push(ClipRow {
            table: input.str()?,
            columns: read_columns(&mut input)?,
        });
    }
    let count = input.len(5)?;
    for _ in 0..count {
        let mut kind = [0; 4];
        kind.copy_from_slice(input.raw(4)?);
        foreign.chunks.push(ClipChunk {
            kind,
            data: input.bytes()?.to_vec(),
        });
    }
    let count = input.len(2)?;
    for _ in 0..count {
        foreign
            .external
            .push((input.str()?, read_columns(&mut input)?));
    }
    foreign.sue_columns = read_columns(&mut input)?;
    foreign.preamble = input.bytes()?.to_vec();
    foreign.foot = input.bytes()?.to_vec();
    foreign.trailing = input.bytes()?.to_vec();
    if version >= 2 {
        doc.comps = parts::decode_comps(input.bytes()?)?;
        doc.timeline = read_option_bytes(&mut input)?
            .map(|bytes| parts::decode_timeline(&bytes))
            .transpose()?;
        doc.preview = read_option_bytes(&mut input)?
            .map(|bytes| parts::decode_picture(&bytes))
            .transpose()?;
    }
    let mut print = None;
    if version >= 3 && input.bool()? {
        let mut bytes = [0; 32];
        bytes.copy_from_slice(input.raw(32)?);
        print = Some(bytes);
    }
    if !input.is_empty() {
        return Err(input.error("bytes after a kept canvas"));
    }
    doc.foreign = foreign;
    Ok((doc, print))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_io::clip::testkit;

    #[test]
    fn every_layer_and_the_canvas_of_the_generated_documents_round_trip() {
        for (name, doc) in testkit::documents() {
            fn walk(layer: &ClipLayer, name: &str) {
                let mut alone = layer.clone();
                alone.children.clear();
                let bytes = layer_bytes(&alone);
                assert_eq!(layer_from(&bytes).unwrap(), alone, "{name}: {}", layer.name);
                // Cut short anywhere: an error, never a panic.
                for cut in (0..bytes.len()).step_by(bytes.len() / 50 + 1) {
                    assert!(layer_from(&bytes[..cut]).is_err());
                }
                for child in &layer.children {
                    walk(child, name);
                }
            }
            walk(&doc.root, &name);
            let mut canvas = doc.clone();
            canvas.root.children.clear();
            canvas.sue = None;
            let bytes = picture_bytes(&canvas, None);
            assert_eq!(picture_from(&bytes).unwrap(), canvas, "{name}");
            let print = [7; 32];
            let with = picture_bytes(&canvas, Some(&print));
            assert_eq!(
                picture_and_print(&with).unwrap(),
                (canvas.clone(), Some(print)),
                "{name}"
            );
            for cut in (0..bytes.len()).step_by(bytes.len() / 50 + 1) {
                assert!(picture_from(&bytes[..cut]).is_err());
            }
        }
    }

    #[test]
    fn a_newer_version_is_refused() {
        let mut bytes = layer_bytes(&ClipLayer::folder("A"));
        bytes[0] = VERSION + 1;
        assert!(layer_from(&bytes).is_err());
        let mut bytes = picture_bytes(&ClipDocument::new(4, 4, 72.0), None);
        bytes[0] = VERSION + 1;
        assert!(picture_from(&bytes).is_err());
    }
}
