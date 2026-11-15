//! Reading a `.clip` into a [`ClipDocument`]: the chunks, the canvas, the
//! layer tree from the rows of its database, each layer's bitmaps through
//! its mipmaps, and everything else kept as it was found.
//!
//! [`decode_clip`] gives the same document as the [`Psd`] records Sue
//! Paint opens today.

use std::collections::{HashMap, HashSet};

use super::Clip;
use super::container::{be64, layout};
use super::kinds::{ClipComp, ClipPicture, ClipTimeline};
use super::model::{
    ClipBitmap, ClipChunk, ClipDocument, ClipDrawColor, ClipForeign, ClipHead, ClipLayer, ClipMask,
    ClipRow, ClipShape, ClipTable, Columns, column,
};
use super::offscreen::{parse_attribute, parse_payload};
use super::parts::{self, CONTENT_KINDS, VERSION};
use super::schema::rowid_column;
use crate::psd::{Psd, PsdKind, PsdLayer};
use crate::sqlite::{Database, Table, Value};

/// The tables the model reads; any other is kept whole.
pub(super) const MODELED: [&str; 10] = [
    "Canvas",
    "Layer",
    "Mipmap",
    "MipmapInfo",
    "Offscreen",
    "ExternalChunk",
    "SueProject",
    "SueLayer",
    "SueLayerPart",
    "SueDocumentPart",
];

/// The columns of `Layer` the model names, always.
const LAYER_TYPED: [&str; 19] = [
    "MainId",
    "LayerName",
    "LayerType",
    "LayerFolder",
    "LayerLock",
    "LayerClip",
    "LayerOpacity",
    "LayerComposite",
    "LayerVisibility",
    "LayerOffsetX",
    "LayerOffsetY",
    "LayerRenderOffscrOffsetX",
    "LayerRenderOffscrOffsetY",
    "LayerUuid",
    "DrawColorEnable",
    "DrawColorMainRed",
    "DrawColorMainGreen",
    "DrawColorMainBlue",
    "LayerNextIndex",
];

/// The columns of `Layer` the model names when the layer has a mask.
const MASK_TYPED: [&str; 5] = [
    "LayerLayerMaskMipmap",
    "LayerMaskOffsetX",
    "LayerMaskOffsetY",
    "LayerMaskOffscrOffsetX",
    "LayerMaskOffscrOffsetY",
];

/// The columns of `Canvas` the model names.
const CANVAS_TYPED: [&str; 5] = [
    "MainId",
    "CanvasWidth",
    "CanvasHeight",
    "CanvasResolution",
    "CanvasRootFolder",
];

/// The columns the model names in the three rows of a bitmap.
const BITMAP_TYPED: [&[&str]; 3] = [
    &["MainId", "LayerId", "BaseMipmapInfo"],
    &["MainId", "LayerId", "Offscreen"],
    &["MainId", "LayerId", "Attribute", "BlockData"],
];

/// One table the model reads, with its rows by `MainId` and which of
/// them were taken.
struct Rows<'a> {
    table: Option<&'a Table>,
    key: Option<usize>,
    by_id: HashMap<i64, usize>,
    taken: HashSet<usize>,
}

impl<'a> Rows<'a> {
    fn new(table: Option<&'a Table>, sql: Option<&str>) -> Self {
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
        Self {
            table,
            key: sql.and_then(rowid_column),
            by_id,
            taken: HashSet::new(),
        }
    }

    /// The row whose `MainId` is `id`, when there is one not yet taken.
    fn free(&self, id: i64) -> Option<usize> {
        if id == 0 {
            return None;
        }
        self.by_id
            .get(&id)
            .copied()
            .filter(|row| !self.taken.contains(row))
    }

    fn value(&self, row: usize, column: &str) -> Option<&'a Value> {
        let table = self.table?;
        table.rows.get(row)?.1.get(table.column(column)?)
    }

    fn int(&self, row: usize, column: &str) -> i64 {
        self.value(row, column).and_then(Value::int).unwrap_or(0)
    }

    fn text(&self, row: usize, column: &str) -> String {
        self.value(row, column)
            .and_then(Value::text)
            .unwrap_or_default()
            .to_owned()
    }

    /// The row's columns but `typed` and the rowid, by name.
    fn rest(&self, row: usize, typed: &[&str]) -> Columns {
        let Some(table) = self.table else {
            return Vec::new();
        };
        let Some((_, values)) = table.rows.get(row) else {
            return Vec::new();
        };
        table
            .columns
            .iter()
            .zip(values)
            .enumerate()
            .filter(|(i, (name, _))| {
                Some(*i) != self.key && !typed.iter().any(|t| t.eq_ignore_ascii_case(name))
            })
            .map(|(_, (name, value))| (name.clone(), value.clone()))
            .collect()
    }

    /// The rows nobody took, kept by name.
    fn left(&self, name: &str) -> Vec<ClipRow> {
        let Some(table) = self.table else {
            return Vec::new();
        };
        (0..table.rows.len())
            .filter(|row| !self.taken.contains(row))
            .map(|row| ClipRow {
                table: name.to_owned(),
                columns: self.rest(row, &[]),
            })
            .collect()
    }
}

/// A stored bitmap chunk not yet taken by a layer.
struct Stored<'a> {
    chunk: usize,
    payload: &'a [u8],
}

struct Reader<'a> {
    layers: Rows<'a>,
    mipmaps: Rows<'a>,
    levels: Rows<'a>,
    offscreens: Rows<'a>,
    stored: HashMap<String, Stored<'a>>,
    taken_chunks: HashSet<usize>,
    seen: HashSet<usize>,
}

impl Reader<'_> {
    /// The bitmap a `Mipmap` row leads to, when every link of the way is
    /// there; its rows are taken.
    fn bitmap(&mut self, mipmap: i64) -> Option<ClipBitmap> {
        let m = self.mipmaps.free(mipmap)?;
        let l = self.levels.free(self.mipmaps.int(m, "BaseMipmapInfo"))?;
        let o = self.offscreens.free(self.levels.int(l, "Offscreen"))?;
        let attribute = self.offscreens.value(o, "Attribute")?.bytes()?;
        let external = match self.offscreens.value(o, "BlockData") {
            Some(Value::Null) | None => String::new(),
            Some(value) => String::from_utf8(value.bytes()?.to_vec()).ok()?,
        };
        let ids = [
            mipmap,
            self.levels.int(l, "MainId"),
            self.offscreens.int(o, "MainId"),
        ];
        let rows = [
            self.mipmaps.rest(m, BITMAP_TYPED[0]),
            self.levels.rest(l, BITMAP_TYPED[1]),
            self.offscreens.rest(o, BITMAP_TYPED[2]),
        ];
        self.mipmaps.taken.insert(m);
        self.levels.taken.insert(l);
        self.offscreens.taken.insert(o);
        let blocks = self.stored.remove(&external).map(|stored| {
            self.taken_chunks.insert(stored.chunk);
            parse_payload(stored.payload)
        });
        Some(ClipBitmap {
            ids,
            rows,
            external,
            attribute: parse_attribute(attribute),
            blocks,
        })
    }

    /// The layer in row `row`, and what it holds.
    fn layer(&mut self, row: usize, root: bool, depth: usize) -> Result<ClipLayer, String> {
        if depth > 64 {
            return Err("the folders nest too deep".to_owned());
        }
        if !self.seen.insert(row) {
            return Err("the layer list loops".to_owned());
        }
        self.layers.taken.insert(row);
        let rows = &self.layers;
        let id = rows.int(row, "MainId");
        let int = |column: &str| rows.int(row, column);
        let mut layer = ClipLayer {
            id,
            name: rows.text(row, "LayerName"),
            layer_type: int("LayerType"),
            folder: int("LayerFolder"),
            lock: int("LayerLock"),
            clip: int("LayerClip"),
            opacity: int("LayerOpacity"),
            composite: int("LayerComposite"),
            visibility: int("LayerVisibility"),
            offset: [int("LayerOffsetX"), int("LayerOffsetY")],
            render_offset: [
                int("LayerRenderOffscrOffsetX"),
                int("LayerRenderOffscrOffsetY"),
            ],
            uuid: rows.text(row, "LayerUuid"),
            draw_color: ClipDrawColor {
                enable: int("DrawColorEnable"),
                main: [
                    int("DrawColorMainRed"),
                    int("DrawColorMainGreen"),
                    int("DrawColorMainBlue"),
                ],
            },
            draft: false,
            reference: false,
            pixels: None,
            mask: None,
            children: Vec::new(),
            columns: Vec::new(),
            content: None,
            effects: None,
            palette_color: None,
            rulers: Vec::new(),
            thumbnail: None,
        };
        let (render, masked, first) = (
            int("LayerRenderMipmap"),
            int("LayerLayerMaskMipmap"),
            int("LayerFirstChildIndex"),
        );
        let mask_offsets = [
            int("LayerMaskOffsetX"),
            int("LayerMaskOffsetY"),
            int("LayerMaskOffscrOffsetX"),
            int("LayerMaskOffscrOffsetY"),
        ];
        let mut typed: Vec<&str> = LAYER_TYPED.to_vec();
        if layer.is_folder() || root {
            typed.push("LayerFirstChildIndex");
        }
        layer.pixels = self.bitmap(render);
        if layer.pixels.is_some() {
            typed.push("LayerRenderMipmap");
        }
        if let Some(bitmap) = self.bitmap(masked) {
            layer.mask = Some(ClipMask {
                offset: [mask_offsets[0], mask_offsets[1]],
                offscreen_offset: [mask_offsets[2], mask_offsets[3]],
                bitmap,
            });
            typed.extend(MASK_TYPED);
        }
        layer.columns = self.layers.rest(row, &typed);
        if layer.is_folder() || root {
            let mut next = first;
            while let Some(child) = self.layers.by_id.get(&next).copied() {
                let child = self.layer(child, false, depth + 1)?;
                next = child_next(&self.layers, child.id);
                layer.children.push(child);
            }
        }
        Ok(layer)
    }
}

/// `LayerNextIndex` of the layer `id`.
fn child_next(rows: &Rows<'_>, id: i64) -> i64 {
    rows.by_id
        .get(&id)
        .map_or(0, |row| rows.int(*row, "LayerNextIndex"))
}

/// Gives draft and reference marks to the layers `marks` names, the
/// root left out; answers the ids it marked.
fn mark(
    folder: &mut ClipLayer,
    marks: &HashMap<i64, (usize, bool, bool)>,
    found: &mut HashSet<i64>,
) {
    for layer in &mut folder.children {
        if let Some((_, draft, reference)) = marks.get(&layer.id)
            && found.insert(layer.id)
        {
            (layer.draft, layer.reference) = (*draft, *reference);
        }
        mark(layer, marks, found);
    }
}

/// One layer part read from `SueLayerPart`, decoded.
enum LayerPart {
    Content(super::kinds::ClipContent),
    Effects(super::kinds::ClipEffects),
    PaletteColor([u8; 3]),
    Rulers(Vec<super::kinds::ClipRuler>),
    Thumbnail(ClipPicture),
}

impl LayerPart {
    /// The part named `kind`, when this build knows it and its bytes are
    /// laid out as this build writes them.
    fn decode(kind: &str, data: &[u8]) -> Option<Self> {
        match kind {
            "effects" => parts::decode_effects(data).ok().map(Self::Effects),
            "palette-color" => parts::decode_color(data).ok().map(Self::PaletteColor),
            "rulers" => parts::decode_rulers(data).ok().map(Self::Rulers),
            "thumbnail" => parts::decode_picture(data).ok().map(Self::Thumbnail),
            kind if CONTENT_KINDS.contains(&kind) => {
                parts::decode_content(kind, data).ok().map(Self::Content)
            }
            _ => None,
        }
    }

    /// Puts the part on `layer` when its place there is free.
    fn give(self, layer: &mut ClipLayer) -> bool {
        match self {
            Self::Content(content) if layer.content.is_none() => layer.content = Some(content),
            Self::Effects(effects) if layer.effects.is_none() => layer.effects = Some(effects),
            Self::PaletteColor(color) if layer.palette_color.is_none() => {
                layer.palette_color = Some(color);
            }
            Self::Rulers(rulers) if layer.rulers.is_empty() && !rulers.is_empty() => {
                layer.rulers = rulers;
            }
            Self::Thumbnail(picture) if layer.thumbnail.is_none() => {
                layer.thumbnail = Some(picture);
            }
            _ => return false,
        }
        true
    }
}

/// Gives the parts in `waiting` (by layer id, in row order) to the
/// layers they name, the root left out; answers the rows given.
fn give_parts(
    folder: &mut ClipLayer,
    waiting: &mut HashMap<i64, Vec<(usize, LayerPart)>>,
    given: &mut Vec<usize>,
) {
    for layer in &mut folder.children {
        if let Some(list) = waiting.remove(&layer.id) {
            for (row, part) in list {
                if part.give(layer) {
                    given.push(row);
                }
            }
        }
        give_parts(layer, waiting, given);
    }
}

/// Whether a part row is laid out as the writer lays one out: its
/// version is this build's, its data a blob, and no other column holds
/// anything.
fn plain_part<'a>(rows: &Rows<'a>, row: usize, columns: &[&str]) -> Option<(String, &'a [u8])> {
    let more = rows
        .rest(row, columns)
        .iter()
        .any(|(_, value)| *value != Value::Null);
    let version = rows.value(row, "Version").and_then(Value::int);
    let kind = rows.value(row, "Kind").and_then(Value::text)?;
    let data = rows.value(row, "Data")?.bytes()?;
    (!more && version == Some(VERSION)).then(|| (kind.to_owned(), data))
}

/// Reads a Clip Studio file into a [`ClipDocument`].
///
/// # Errors
/// When `bytes` is not a `.clip`, a chunk or its database is broken, the
/// canvas is not a valid size, or the layer tree loops.
#[allow(clippy::too_many_lines)]
pub fn read(bytes: &[u8]) -> Result<ClipDocument, String> {
    let layout = layout(bytes)?;
    if let Some(problem) = layout.problem {
        return Err(problem);
    }
    let first = usize::try_from(layout.first_chunk).map_err(|_| "the file header is broken")?;
    let mut foreign = ClipForeign {
        preamble: bytes
            .get(24..first)
            .ok_or("the file header is broken")?
            .to_vec(),
        trailing: bytes[bytes.len() - layout.trailing..].to_vec(),
        ..ClipForeign::default()
    };
    let (mut head_at, mut database, mut foot) = (None, None, None);
    let mut stored = HashMap::new();
    for (i, chunk) in layout.chunks.iter().enumerate() {
        match &chunk.kind {
            b"Head" if head_at.is_none() => head_at = Some(i),
            b"SQLi" if database.is_none() => database = Some(i),
            b"Foot" if foot.is_none() => foot = Some(i),
            b"Exta" => {
                // A chunk whose id is text is taken apart, its payload
                // being everything after the payload's length, as Sue's
                // reader has always taken it: a length that does not match
                // (not yet seen in a file Clip Studio wrote) must not cost
                // the layer its pixels. It is written back with the length
                // of what it holds. Any other chunk stays whole.
                if let Ok((_, payload)) = chunk.external()
                    && let Some(id_len) =
                        be64(chunk.data, 0).and_then(|len| usize::try_from(len).ok())
                    && let Some(Ok(id)) = chunk
                        .data
                        .get(8..8 + id_len)
                        .map(|raw| std::str::from_utf8(raw).map(str::to_owned))
                    && !stored.contains_key(&id)
                {
                    stored.insert(id, Stored { chunk: i, payload });
                }
            }
            _ => {}
        }
    }
    let database_at = database.ok_or("the file has no layer database")?;
    let database_chunk = &layout.chunks[database_at];
    // A file with no head gets one as Sue writes it.
    let head = head_at.map_or(ClipHead::Sue { id: [0; 16] }, |i| {
        let data = layout.chunks[i].data;
        let words = [be64(data, 0), be64(data, 8), be64(data, 16)];
        let at = u64::try_from(database_chunk.offset).ok();
        match (data.len(), words) {
            (40, [Some(256), Some(offset), Some(16)]) if Some(offset) == at => {
                let mut id = [0; 16];
                id.copy_from_slice(&data[24..40]);
                ClipHead::Sue { id }
            }
            _ => ClipHead::Raw(data.to_vec()),
        }
    });
    if let Some(i) = foot {
        foreign.foot = layout.chunks[i].data.to_vec();
    }

    let database = Database::open(database_chunk.data)?;
    let mut tables: Vec<(Table, String)> = Vec::new();
    for entry in database.schema() {
        if entry.kind != "table" || entry.name.eq_ignore_ascii_case("sqlite_sequence") {
            continue;
        }
        if let Some(table) = database.table(&entry.name)? {
            tables.push((table, entry.sql.clone()));
        }
    }
    let mut shapes: Vec<ClipShape> = Vec::new();
    for (table, sql) in &tables {
        // Every table the model reads keeps its shape, whatever columns
        // it has: kept whole under the same name, the writer could not
        // write it back beside its own.
        if MODELED.iter().any(|name| *name == table.name) {
            shapes.push(ClipShape {
                table: table.name.clone(),
                sql: sql.clone(),
                columns: table.columns.clone(),
            });
        } else {
            foreign.tables.push(ClipTable {
                name: table.name.clone(),
                sql: sql.clone(),
                columns: table.columns.clone(),
                rows: table.rows.clone(),
            });
        }
    }
    let rows = |name: &str| {
        let shape = shapes
            .iter()
            .find(|shape| shape.table == name)
            .map(|shape| shape.sql.as_str());
        let table = shape.and_then(|_| {
            tables
                .iter()
                .find(|(table, _)| table.name == name)
                .map(|(table, _)| table)
        });
        Rows::new(table, shape)
    };

    // The canvas.
    let mut canvas = rows("Canvas");
    let canvas_row = canvas
        .table
        .filter(|table| !table.rows.is_empty())
        .map(|_| 0)
        .ok_or("the file has no canvas")?;
    canvas.taken.insert(canvas_row);
    let real = |column: &str| canvas.value(canvas_row, column).and_then(Value::real);
    let width = real("CanvasWidth").unwrap_or(0.0).round();
    let height = real("CanvasHeight").unwrap_or(0.0).round();
    if !(1.0..=300_000.0).contains(&width) || !(1.0..=300_000.0).contains(&height) {
        return Err(format!(
            "a canvas of {width} by {height} pixels is not valid"
        ));
    }
    let dpi = real("CanvasResolution").unwrap_or(72.0);
    let root_id = canvas.int(canvas_row, "CanvasRootFolder");

    // The layers.
    let mut reader = Reader {
        layers: rows("Layer"),
        mipmaps: rows("Mipmap"),
        levels: rows("MipmapInfo"),
        offscreens: rows("Offscreen"),
        stored,
        taken_chunks: HashSet::new(),
        seen: HashSet::new(),
    };
    let mut root = match reader.layers.free(root_id) {
        Some(row) => reader.layer(row, true, 0)?,
        None => ClipLayer::folder(""),
    };

    // Sue's own tables.
    let sue_project = rows("SueProject");
    let mut sue = None;
    if let Some(row) = sue_project
        .table
        .filter(|table| table.column("Data").is_some() && !table.rows.is_empty())
        .map(|_| 0)
        && let Some(data) = sue_project.value(row, "Data").and_then(Value::bytes)
    {
        sue = Some(data.to_vec());
        foreign.sue_columns = sue_project.rest(row, &["Data"]);
    }
    let mut sue_project = sue_project;
    if sue.is_some() {
        sue_project.taken.insert(0);
    }
    let mut sue_layer = rows("SueLayer");
    if let Some(table) = sue_layer.table {
        // A mark is taken when it is laid out as the writer lays one out
        // (a column the table lacks reads as not set) and names a layer;
        // a row that holds more, such as columns a newer Sue added, stays
        // as it was, and so does any other row.
        let flag = |row: usize, column: &str| match sue_layer.value(row, column) {
            Some(Value::Integer(value @ (0 | 1))) => Some(*value == 1),
            None if table.column(column).is_none() => Some(false),
            _ => None,
        };
        let mut marks = HashMap::new();
        for row in 0..table.rows.len() {
            let id = sue_layer.int(row, "LayerId");
            let more = sue_layer
                .rest(row, &["LayerId", "Draft", "Reference"])
                .iter()
                .any(|(_, value)| *value != Value::Null);
            if let (Some(draft), Some(reference)) = (flag(row, "Draft"), flag(row, "Reference"))
                && (draft || reference)
                && !more
                && matches!(sue_layer.value(row, "LayerId"), Some(Value::Integer(_)))
            {
                marks.entry(id).or_insert((row, draft, reference));
            }
        }
        let mut found = HashSet::new();
        mark(&mut root, &marks, &mut found);
        for id in found {
            sue_layer.taken.insert(marks[&id].0);
        }
    }

    // Sue's own parts of layers.
    let mut layer_parts = rows("SueLayerPart");
    if let Some(table) = layer_parts.table {
        let mut waiting: HashMap<i64, Vec<(usize, LayerPart)>> = HashMap::new();
        for row in 0..table.rows.len() {
            let Some(Value::Integer(id)) = layer_parts.value(row, "LayerId") else {
                continue;
            };
            if let Some((kind, data)) =
                plain_part(&layer_parts, row, &["LayerId", "Kind", "Version", "Data"])
                && let Some(part) = LayerPart::decode(&kind, data)
            {
                waiting.entry(*id).or_default().push((row, part));
            }
        }
        let mut given = Vec::new();
        give_parts(&mut root, &mut waiting, &mut given);
        layer_parts.taken.extend(given);
    }

    // Sue's own parts of the document.
    let mut document_parts = rows("SueDocumentPart");
    let (mut comps, mut timeline, mut preview): (
        Option<Vec<ClipComp>>,
        Option<ClipTimeline>,
        Option<ClipPicture>,
    ) = (None, None, None);
    if let Some(table) = document_parts.table {
        for row in 0..table.rows.len() {
            let Some((kind, data)) = plain_part(&document_parts, row, &["Kind", "Version", "Data"])
            else {
                continue;
            };
            let taken = match kind.as_str() {
                "comps" if comps.is_none() => parts::decode_comps(data)
                    .ok()
                    .filter(|list| !list.is_empty())
                    .map(|list| comps = Some(list)),
                "timeline" if timeline.is_none() => parts::decode_timeline(data)
                    .ok()
                    .map(|value| timeline = Some(value)),
                "preview" if preview.is_none() => parts::decode_picture(data)
                    .ok()
                    .map(|value| preview = Some(value)),
                _ => None,
            };
            if taken.is_some() {
                document_parts.taken.insert(row);
            }
        }
    }

    // The bitmap chunks' extra columns.
    let external = rows("ExternalChunk");
    if let Some(table) = external.table {
        for row in 0..table.rows.len() {
            let rest = external.rest(row, &["ExternalID", "Offset"]);
            let id = external
                .value(row, "ExternalID")
                .and_then(Value::bytes)
                .map(|id| String::from_utf8_lossy(id).into_owned());
            if let Some(id) = id
                && !rest.is_empty()
                && !foreign.external.iter().any(|(known, _)| *known == id)
            {
                foreign.external.push((id, rest));
            }
        }
    }

    for (name, rows) in [
        ("Canvas", &canvas),
        ("Layer", &reader.layers),
        ("Mipmap", &reader.mipmaps),
        ("MipmapInfo", &reader.levels),
        ("Offscreen", &reader.offscreens),
        ("SueProject", &sue_project),
        ("SueLayer", &sue_layer),
        ("SueLayerPart", &layer_parts),
        ("SueDocumentPart", &document_parts),
    ] {
        foreign.rows.extend(rows.left(name));
    }
    for (i, chunk) in layout.chunks.iter().enumerate() {
        if [head_at, Some(database_at), foot].contains(&Some(i)) || reader.taken_chunks.contains(&i)
        {
            continue;
        }
        foreign.chunks.push(ClipChunk {
            kind: chunk.kind,
            data: chunk.data.to_vec(),
        });
    }

    let canvas_id = canvas.int(canvas_row, "MainId");
    let canvas_columns = canvas.rest(canvas_row, &CANVAS_TYPED);
    drop(canvas);
    foreign.shapes = shapes;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(ClipDocument {
        width: width as u32,
        height: height as u32,
        dpi,
        canvas_id,
        canvas_columns,
        root,
        sue,
        head,
        foreign,
        comps: comps.unwrap_or_default(),
        timeline,
        preview,
    })
}

// ----- As Photoshop records, for Sue Paint as it opens them today -----------------

fn style(layer: &ClipLayer, record: &mut PsdLayer) {
    record.opacity = layer.opacity_u8();
    record.visible = layer.visible();
    record.clip = layer.clipped();
    record.blend = layer.blend();
}

/// The record of one pixel layer, cut to the canvas: a mask that is on
/// is multiplied into the alpha, paper is drawn in its color, and only
/// color layers come in.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn record(doc: &ClipDocument, layer: &ClipLayer) -> Result<Option<PsdLayer>, String> {
    // Correction layers keep only their mask as pixels, and gradient
    // and fill layers are drawn from their settings: Sue cannot redo
    // those yet, so they come in as empty layers in their places.
    let generated = ["FilterLayerInfo", "GradationFillInfo"].iter().any(|name| {
        column(&layer.columns, name)
            .and_then(Value::bytes)
            .is_some()
    });
    if generated {
        let mut record = PsdLayer::pixels(&layer.name, 0, 0, 0, 0, Vec::new());
        style(layer, &mut record);
        return Ok(Some(record));
    }
    let Some(bitmap) = &layer.pixels else {
        return Ok(None);
    };
    let decoded = bitmap.decode()?;
    if decoded.channels != 4 {
        return Ok(None);
    }
    let (width, height) = (decoded.width as usize, decoded.height as usize);
    let mut pixels = decoded.data;
    let [left, top] = layer.pixels_at();
    // A paper layer draws in its own color.
    if layer.draw_color.enabled() && layer.is_paper() {
        let color = layer.draw_color.color();
        for pixel in pixels.as_chunks_mut::<4>().0 {
            if pixel[3] > 0 {
                pixel[..3].copy_from_slice(&color);
            }
        }
    }
    // A layer mask, when on, cuts the alpha.
    if layer.mask_enabled()
        && let Some(mask) = &layer.mask
        && let Some([mx, my]) = layer.mask_at()
    {
        let values = mask.bitmap.decode()?;
        let step = usize::from(values.channels);
        let (mw, mh) = (values.width as usize, values.height as usize);
        let outside = if mask.bitmap.white() { 255 } else { 0 };
        for y in 0..height {
            for x in 0..width {
                let (cx, cy) = (left + x as i64 - mx, top + y as i64 - my);
                let value = if cx >= 0 && cy >= 0 && (cx as usize) < mw && (cy as usize) < mh {
                    values.data[(cy as usize * mw + cx as usize) * step + step - 1]
                } else {
                    outside
                };
                let a = &mut pixels[(y * width + x) * 4 + 3];
                *a = ((u16::from(*a) * u16::from(value) + 127) / 255) as u8;
            }
        }
    }
    // Cut to the canvas.
    let (cw, ch) = (i64::from(doc.width), i64::from(doc.height));
    let (x0, y0) = (left.max(0), top.max(0));
    let (x1, y1) = (
        left.saturating_add(width as i64).min(cw),
        top.saturating_add(height as i64).min(ch),
    );
    let (w, h) = ((x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize);
    let mut cut = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let sy = (y0 - top) as usize + y;
        let sx = (x0 - left) as usize;
        let start = (sy * width + sx) * 4;
        cut.extend_from_slice(&pixels[start..start + w * 4]);
    }
    let mut record = PsdLayer::pixels(&layer.name, x0 as i32, y0 as i32, w as u32, h as u32, cut);
    style(layer, &mut record);
    Ok(Some(record))
}

/// The records of `folder`'s children, bottom first.
fn records(doc: &ClipDocument, folder: &ClipLayer, out: &mut Vec<PsdLayer>) -> Result<(), String> {
    for layer in &folder.children {
        if layer.is_folder() {
            out.push(PsdLayer::folder("</Layer group>", PsdKind::FolderEnd));
            records(doc, layer, out)?;
            let open = layer.is_open();
            let mut record = PsdLayer::folder(&layer.name, PsdKind::FolderOpen { open });
            style(layer, &mut record);
            out.push(record);
        } else if let Some(record) = record(doc, layer)? {
            out.push(record);
        }
    }
    Ok(())
}

/// Reads a Clip Studio file as Photoshop records, the way Sue Paint
/// opens one today.
///
/// # Errors
/// When `bytes` is not a `.clip`, or its database or bitmaps are broken.
pub fn decode_clip(bytes: &[u8]) -> Result<Clip, String> {
    let doc = read(bytes)?;
    let mut layers = Vec::new();
    records(&doc, &doc.root, &mut layers)?;
    Ok(Clip {
        psd: Psd {
            width: doc.width,
            height: doc.height,
            layers,
            merged: Vec::new(),
        },
        dpi: doc.dpi,
        sue: doc.sue,
    })
}
