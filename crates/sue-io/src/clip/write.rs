//! Writing a [`ClipDocument`]: its layers as rows, each bitmap with its
//! chunk, everything kept from a file it was read from in its place, laid
//! out as chunks around a fresh database.
//!
//! [`encode_clip`] writes the [`Psd`] records Sue Paint saves today
//! through the same model.

use std::collections::{HashMap, HashSet};

use super::kinds::ClipContent;
use super::model::{
    ClipBitmap, ClipDocument, ClipHead, ClipLayer, ClipMask, ClipShape, ClipTable, Columns, FOLDER,
    column,
};
use super::native::{self, NATIVE_LAYER_COLUMNS};
use super::offscreen::{attribute_of, payload_bytes};
use super::parts::{self, VERSION};
use super::read::MODELED;
use super::schema::{
    ANIMATION_CUT_BANK_COLUMNS, CANVAS_COLUMNS, CANVAS_ITEM_BANK_COLUMNS, EXTERNAL_CHUNK_SQL,
    EXTERNAL_NAMES_SQL, LAYER_COLUMNS, LAYER_COMP_MANAGER_COLUMNS, LAYER_THUMBNAIL_COLUMNS,
    MIPMAP_COLUMNS, MIPMAP_INFO_COLUMNS, OFFSCREEN_COLUMNS, PROJECT_COLUMNS,
    SUE_DOCUMENT_PART_COLUMNS, SUE_DOCUMENT_PART_SQL, SUE_LAYER_PART_COLUMNS, SUE_LAYER_PART_SQL,
    SUE_LAYER_SQL, SUE_PROJECT_SQL, composite_of, definition, names, quoted, rowid_column,
    sql_columns, table_sql,
};
use crate::psd::{Psd, PsdKind, PsdLayer};
use crate::sqlite::{self, NewTable, Value};

// ----- Fresh ids ------------------------------------------------------------------

/// A 64-bit fingerprint of a document, so ids made for it differ from
/// those of any other document.
struct Fingerprint(u64);

impl Fingerprint {
    fn word(&mut self, value: u64) {
        self.0 = (self.0 ^ value)
            .wrapping_mul(0x0000_0100_0000_01b3)
            .rotate_left(29);
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.word(bytes.len() as u64);
        let (words, rest) = bytes.as_chunks::<8>();
        for word in words {
            self.word(u64::from_le_bytes(*word));
        }
        let mut last = [0; 8];
        last[..rest.len()].copy_from_slice(rest);
        self.word(u64::from_le_bytes(last));
    }

    #[allow(clippy::cast_sign_loss)]
    fn layer(&mut self, layer: &ClipLayer) {
        self.bytes(layer.name.as_bytes());
        for value in [
            layer.layer_type,
            layer.folder,
            layer.opacity,
            layer.composite,
            layer.visibility,
            layer.offset[0],
            layer.offset[1],
        ] {
            self.word(value as u64);
        }
        let mask = layer.mask.as_ref().map(|mask| &mask.bitmap);
        for bitmap in [layer.pixels.as_ref(), mask].into_iter().flatten() {
            self.bytes(&attribute_of(bitmap));
            for block in bitmap.blocks.iter().flat_map(|blocks| &blocks.blocks) {
                self.word(u64::from(block.index));
                self.bytes(block.data.as_deref().unwrap_or_default());
            }
        }
        for (kind, data) in layer_parts(layer) {
            self.bytes(kind.as_bytes());
            self.bytes(&data);
        }
        for child in &layer.children {
            self.layer(child);
        }
        self.word(u64::MAX);
    }
}

/// The parts of a layer Sue stores in its own table, by part name, in the
/// order they are written.
fn layer_parts(layer: &ClipLayer) -> Vec<(&'static str, Vec<u8>)> {
    let mut out = Vec::new();
    if let Some(content) = &layer.content {
        out.push((content.kind(), parts::encode_content(content)));
    }
    if let Some(effects) = &layer.effects {
        out.push(("effects", parts::encode_effects(effects)));
    }
    if let Some(color) = layer.palette_color {
        out.push(("palette-color", parts::encode_color(color)));
    }
    if !layer.rulers.is_empty() {
        out.push(("rulers", parts::encode_rulers(&layer.rulers)));
    }
    if let Some(picture) = &layer.thumbnail {
        out.push(("thumbnail", parts::encode_picture(picture)));
    }
    out
}

/// The parts of the document Sue stores in its own table.
fn document_parts(doc: &ClipDocument) -> Vec<(&'static str, Vec<u8>)> {
    let mut out = Vec::new();
    if !doc.comps.is_empty() {
        out.push(("comps", parts::encode_comps(&doc.comps)));
    }
    if let Some(timeline) = &doc.timeline {
        out.push(("timeline", parts::encode_timeline(timeline)));
    }
    if let Some(picture) = &doc.preview {
        out.push(("preview", parts::encode_picture(picture)));
    }
    out
}

/// Fresh ids for one document: each new one differs from the last.
struct Ids {
    seed: u64,
    count: u64,
}

impl Ids {
    fn new(doc: &ClipDocument) -> Self {
        let mut print = Fingerprint(0xcbf2_9ce4_8422_2325);
        print.word(u64::from(doc.width) << 32 | u64::from(doc.height));
        print.word(doc.dpi.to_bits());
        print.layer(&doc.root);
        print.bytes(doc.sue.as_deref().unwrap_or_default());
        for (kind, data) in document_parts(doc) {
            print.bytes(kind.as_bytes());
            print.bytes(&data);
        }
        Self {
            seed: print.0,
            count: 0,
        }
    }

    /// The next 16 bytes.
    fn next(&mut self) -> [u8; 16] {
        let mut out = [0_u8; 16];
        for chunk in out.chunks_mut(8) {
            self.count += 1;
            let mut z = self
                .seed
                .wrapping_add(self.count.wrapping_mul(0x9e37_79b9_7f4a_7c15));
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            chunk.copy_from_slice(&(z ^ (z >> 31)).to_be_bytes());
        }
        out
    }

    /// A layer id in Clip Studio's spelling.
    fn uuid(&mut self) -> String {
        let hex = hex(&self.next());
        format!(
            "{}-{}-{}-{}-{}",
            &hex[..10],
            &hex[10..14],
            &hex[14..18],
            &hex[18..22],
            &hex[22..32]
        )
    }

    /// A stored bitmap's chunk id.
    fn external(&mut self) -> String {
        format!("extrnlid{}", hex(&self.next()).to_uppercase())
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

// ----- Tables ---------------------------------------------------------------------

/// One row to lay out: what the model names, what the file had, and what
/// Sue writes when neither says.
#[derive(Default)]
struct Row {
    typed: Vec<(&'static str, Value)>,
    kept: Columns,
    made: Vec<(&'static str, Value)>,
}

/// A table being written over a shape: the file's, or Sue's own.
struct Out {
    name: &'static str,
    sql: String,
    columns: Vec<String>,
    /// Sue's column list, for the definitions of columns to add.
    defaults: &'static str,
    rows: Vec<Row>,
}

impl Out {
    fn new(doc: &ClipDocument, name: &'static str, sql: String, defaults: &'static str) -> Self {
        let (sql, columns) = match doc.foreign.shape(name) {
            Some(ClipShape { sql, columns, .. }) => (sql.clone(), columns.clone()),
            None => {
                let mut columns = names(defaults);
                if rowid_column(&sql) == Some(0) {
                    columns.insert(0, "_PW_ID".to_owned());
                }
                (sql, columns)
            }
        };
        Self {
            name,
            sql,
            columns,
            defaults,
            rows: Vec::new(),
        }
    }

    fn has(&self, name: &str) -> bool {
        self.columns
            .iter()
            .any(|column| column.eq_ignore_ascii_case(name))
    }

    /// Adds `column` to the shape.
    fn add(&mut self, column: &str, definition: &str) -> Result<(), String> {
        let close = self
            .sql
            .rfind(')')
            .ok_or("a table's shape in the document is broken")?;
        self.sql.insert_str(close, &format!(", {definition}"));
        self.columns.push(column.to_owned());
        Ok(())
    }

    /// The table, with the columns its rows need and the shape lacks
    /// added at the end.
    fn finish(mut self) -> Result<NewTable, String> {
        let rows = std::mem::take(&mut self.rows);
        for row in &rows {
            for (name, value) in &row.typed {
                let blank = matches!(value, Value::Null | Value::Integer(0))
                    || matches!(value, Value::Text(text) if text.is_empty())
                    || matches!(value, Value::Real(real) if *real == 0.0);
                // Clip Studio's own columns of a kind of layer are
                // written even when they hold 0, as Clip Studio writes them.
                let native = definition(NATIVE_LAYER_COLUMNS, name).is_some();
                if (!blank || native) && !self.has(name) {
                    let definition = definition(self.defaults, name)
                        .or_else(|| definition(NATIVE_LAYER_COLUMNS, name))
                        .map_or_else(|| format!("{name} INTEGER DEFAULT NULL"), str::to_owned);
                    self.add(name, &definition)?;
                }
            }
            for (name, _) in &row.kept {
                if !self.has(name) {
                    self.add(name, &quoted(name))?;
                }
            }
        }
        let key = rowid_column(&self.sql);
        let lookup = |list: &[(&str, Value)], name: &str| {
            list.iter()
                .find(|(column, _)| column.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.clone())
        };
        let rows = rows
            .iter()
            .zip(1_i64..)
            .map(|(row, rowid)| {
                let values = self
                    .columns
                    .iter()
                    .enumerate()
                    .map(|(i, name)| {
                        if Some(i) == key {
                            return Value::Null;
                        }
                        lookup(&row.typed, name)
                            .or_else(|| column(&row.kept, name).cloned())
                            .or_else(|| lookup(&row.made, name))
                            .unwrap_or(Value::Null)
                    })
                    .collect();
                (rowid, values)
            })
            .collect();
        Ok(NewTable {
            name: self.name.to_owned(),
            sql: self.sql,
            rows,
        })
    }
}

/// A table kept whole, its rowid column stored as NULL again.
fn kept_table(table: &ClipTable) -> NewTable {
    let key = rowid_column(&table.sql);
    NewTable {
        name: table.name.clone(),
        sql: table.sql.clone(),
        rows: table
            .rows
            .iter()
            .map(|(rowid, values)| {
                let mut values = values.clone();
                if let Some(key) = key
                    && values.get(key) == Some(&Value::Integer(*rowid))
                {
                    values[key] = Value::Null;
                }
                (*rowid, values)
            })
            .collect(),
    }
}

// ----- Layers ---------------------------------------------------------------------

struct Writer<'a> {
    doc: &'a ClipDocument,
    ids: Ids,
    next: i64,
    used: [HashSet<i64>; 4],
    uuids: HashSet<String>,
    externals: HashSet<String>,
    canvas: i64,
    layers: Out,
    mipmaps: Out,
    levels: Out,
    offscreens: Out,
    thumbnails: Vec<(i64, Vec<Value>)>,
    marks: Vec<(i64, bool, bool)>,
    parts: Vec<(i64, &'static str, Vec<u8>)>,
    exta: Vec<(String, Vec<u8>)>,
    /// The last layer row that is not a folder.
    top: i64,
    /// The vector layers written in Clip Studio's own storage.
    vectors: Vectors,
}

/// The vector layers of a document as Clip Studio stores them: a
/// `VectorObjectList` row each, whose strokes sit in an `Exta` chunk.
#[derive(Default)]
struct Vectors {
    /// The `MainId` of the brush the strokes are drawn with.
    brush: u32,
    /// The next free `VectorObjectList` `MainId`.
    next: i64,
    /// New rows: `MainId`, layer, chunk id.
    rows: Vec<(i64, i64, String)>,
    /// Their chunks, written after every bitmap's: where a file read
    /// back keeps them, so writing it again gives the same bytes.
    chunks: Vec<(String, Vec<u8>)>,
    /// Kept rows still pointed at, by `MainId`, with the layer that does.
    kept: HashMap<i64, i64>,
}

/// What a layer's row gets in Clip Studio's own columns beyond Sue's.
#[derive(Default)]
struct NativeRow {
    /// Columns that take the place of what the row would hold.
    typed: Vec<(&'static str, Value)>,
    /// A mask the layer needs and has not (a fill or gradient layer).
    mask: Option<ClipMask>,
}

/// The palette color's three `Layer` columns.
const PALETTE: [&str; 3] = ["LayerPaletteRed", "LayerPaletteGreen", "LayerPaletteBlue"];
/// The layer color effect's sub color.
const SUB_COLOR: [&str; 3] = ["DrawColorSubRed", "DrawColorSubGreen", "DrawColorSubBlue"];

impl Writer<'_> {
    /// `id` when it is set and free in table `table` (0 `Layer`, 1
    /// `Mipmap`, 2 `MipmapInfo`, 3 `Offscreen`), otherwise a fresh one.
    fn id(&mut self, table: usize, id: i64) -> i64 {
        if id != 0 && self.used[table].insert(id) {
            return id;
        }
        self.next += 1;
        self.used[table].insert(self.next);
        self.next
    }

    /// The rows of one bitmap of layer `layer`; answers its mipmap id.
    fn bitmap(&mut self, layer: i64, bitmap: &ClipBitmap) -> i64 {
        let offscreen = self.id(3, bitmap.ids[2]);
        let level = self.id(2, bitmap.ids[1]);
        let mipmap = self.id(1, bitmap.ids[0]);
        let external = if bitmap.external.is_empty() || self.externals.contains(&bitmap.external) {
            self.ids.external()
        } else {
            bitmap.external.clone()
        };
        self.externals.insert(external.clone());
        if let Some(blocks) = &bitmap.blocks {
            self.exta.push((external.clone(), payload_bytes(blocks)));
        }
        let (canvas, int) = (self.canvas, Value::Integer);
        self.offscreens.rows.push(Row {
            typed: vec![
                ("MainId", int(offscreen)),
                ("LayerId", int(layer)),
                ("Attribute", Value::Blob(attribute_of(bitmap))),
                ("BlockData", Value::Blob(external.into_bytes())),
            ],
            kept: bitmap.rows[2].clone(),
            made: vec![("CanvasId", int(canvas))],
        });
        self.levels.rows.push(Row {
            typed: vec![
                ("MainId", int(level)),
                ("LayerId", int(layer)),
                ("Offscreen", int(offscreen)),
            ],
            kept: bitmap.rows[1].clone(),
            made: vec![
                ("CanvasId", int(canvas)),
                ("ThisScale", Value::Real(100.0)),
                ("NextIndex", int(0)),
            ],
        });
        self.mipmaps.rows.push(Row {
            typed: vec![
                ("MainId", int(mipmap)),
                ("LayerId", int(layer)),
                ("BaseMipmapInfo", int(level)),
            ],
            kept: bitmap.rows[0].clone(),
            made: vec![("CanvasId", int(canvas)), ("MipmapCount", int(1))],
        });
        self.thumbnails.push((
            i64::try_from(self.thumbnails.len()).unwrap_or(0) + 1,
            vec![
                Value::Null,
                int(mipmap),
                int(canvas),
                int(layer),
                int(i64::from(self.doc.width)),
                int(i64::from(self.doc.height)),
                int(offscreen),
                int(0),
                int(-1),
            ],
        ));
        mipmap
    }

    /// What `layer` (with row `id`) gets in Clip Studio's own storage:
    /// its fill, gradient, text or vector lines where the survey shows
    /// how, its palette color and its layer color's sub color. A layer
    /// whose kept columns already say the same keeps them as they are.
    #[allow(clippy::too_many_lines)]
    fn native(&mut self, layer: &ClipLayer, id: i64) -> NativeRow {
        let mut out = NativeRow::default();
        let int = Value::Integer;
        let kept_int = |name: &str| column(&layer.columns, name).and_then(Value::int);
        if let Some(color) = layer.palette_color {
            let kept = (kept_int("LayerUsePaletteColor").unwrap_or(0) != 0)
                .then(|| native::color_columns(&layer.columns, PALETTE))
                .flatten();
            if kept != Some(color) {
                out.typed.push(("LayerUsePaletteColor", int(1)));
                for (name, channel) in PALETTE.into_iter().zip(color) {
                    out.typed.push((name, int(native::quad_column(channel))));
                }
            }
        }
        if layer.draw_color.enabled()
            && let Some(sub) = layer.effects.as_ref().and_then(|effects| effects.sub_color)
            && native::color_columns(&layer.columns, SUB_COLOR) != Some(sub)
        {
            for (name, channel) in SUB_COLOR.into_iter().zip(sub) {
                out.typed.push((name, int(native::quad_column(channel))));
            }
        }
        let stroke = kept_int("VectorNormalStrokeIndex").filter(|index| *index != 0);
        let Some(content) = layer
            .content
            .as_ref()
            .filter(|content| native::clip_studio_fit(content).is_some())
        else {
            if let Some(index) = stroke {
                self.vectors.kept.insert(index, id);
            }
            return out;
        };
        let dpi = self.doc.dpi;
        let reads = native::as_clip_studio_reads(content, dpi);
        let kept = native::content_of_columns(&layer.columns).or_else(|| {
            stroke
                .and_then(|index| kept_strokes(self.doc, index))
                .map(ClipContent::Vector)
        });
        // A kept stroke row stays unless new strokes take its place.
        let replaced = !(reads.is_some() && kept == reads);
        if let Some(index) = stroke
            && !(replaced && matches!(content, ClipContent::Vector(_)))
        {
            self.vectors.kept.insert(index, id);
        }
        if !replaced {
            return out;
        }
        match content {
            ClipContent::Fill(_) | ClipContent::Gradient(_) => {
                let Some(info) = native::gradation_info(content) else {
                    return out;
                };
                out.typed.extend([
                    ("LayerType", int(native::GENERATED)),
                    ("LayerMasking", int(1)),
                    ("GradationFillInfo", Value::Blob(info)),
                    ("DrawToRenderOffscreenType", int(60)),
                    ("DrawToRenderMipmapType", int(32)),
                    ("MoveOffsetAndExpandType", int(30)),
                    ("FixOffsetAndExpandType", int(30)),
                    ("RenderBoundForLayerMoveType", int(30)),
                    ("SetRenderThumbnailInfoType", int(30)),
                    ("DrawRenderThumbnailType", int(30)),
                ]);
                if matches!(content, ClipContent::Gradient(_)) {
                    out.typed.push(("GradationFillColorMixingInfo", int(1)));
                }
                // Clip Studio gives a fill or gradient layer a mask, white
                // and with nothing stored in it.
                if layer.mask.is_none() {
                    out.mask = Some(ClipMask {
                        offset: [-layer.offset[0], -layer.offset[1]],
                        offscreen_offset: [0, 0],
                        bitmap: ClipBitmap::gray(self.doc.width, self.doc.height, &[], true),
                    });
                }
            }
            ClipContent::Text(text) => {
                let blobs = native::text_blobs(text, dpi);
                out.typed.extend([
                    ("LayerType", int(native::DRAWN)),
                    ("DrawToRenderOffscreenType", int(40)),
                    ("DrawToRenderMipmapType", int(40)),
                    ("MoveOffsetAndExpandType", int(50)),
                    ("MaterialContentType", int(110)),
                    ("TextLayerType", int(0)),
                    ("TextLayerString", Value::Blob(blobs.string)),
                    ("TextLayerAttributes", Value::Blob(blobs.attributes)),
                    ("TextLayerAddAttributesV01", Value::Blob(blobs.add)),
                    ("TextLayerAttributesVersion", int(1)),
                    // The layer keeps its own name.
                    ("TextLayerNameAutoChangeEnabled", int(0)),
                ]);
            }
            ClipContent::Vector(vector) => {
                let main = self.vectors.next;
                self.vectors.next += 1;
                let external = self.ids.external();
                self.externals.insert(external.clone());
                self.vectors.chunks.push((
                    external.clone(),
                    native::vector_bytes(vector, self.vectors.brush),
                ));
                self.vectors.rows.push((main, id, external));
                out.typed.extend([
                    ("LayerType", int(native::DRAWN)),
                    ("DrawToRenderOffscreenType", int(10)),
                    ("VectorNormalStrokeIndex", int(main)),
                    ("VectorNormalFillIndex", int(0)),
                    ("VectorNormalBalloonIndex", int(0)),
                    ("VectorNormalType", int(0)),
                    ("MixSubColorForEveryPlot", int(1)),
                ]);
            }
            _ => {}
        }
        out
    }

    /// The rows of one layer, given its id and the next one's.
    fn layer(&mut self, layer: &ClipLayer, id: i64, next: i64, root: bool) {
        let first = if layer.is_folder() || root {
            self.nodes(&layer.children)
        } else {
            0
        };
        // Clip Studio needs a bitmap on every folder, an empty one at the
        // least: it closes when it opens a file with a folder that has
        // none (seen in Clip Studio Paint 5.0.4 on 2026-11-16).
        let empty = (layer.pixels.is_none() && layer.is_folder())
            .then(|| ClipBitmap::empty(self.doc.width, self.doc.height));
        let render = layer
            .pixels
            .as_ref()
            .or(empty.as_ref())
            .map(|bitmap| self.bitmap(id, bitmap));
        let native = if root || layer.is_folder() {
            NativeRow::default()
        } else {
            self.native(layer, id)
        };
        let mask = layer
            .mask
            .as_ref()
            .or(native.mask.as_ref())
            .map(|mask| (mask, self.bitmap(id, &mask.bitmap)));
        let uuid = if layer.uuid.is_empty() || self.uuids.contains(&layer.uuid) {
            self.ids.uuid()
        } else {
            layer.uuid.clone()
        };
        self.uuids.insert(uuid.clone());
        if layer.draft || layer.reference {
            self.marks.push((id, layer.draft, layer.reference));
        }
        if !root {
            for (kind, data) in layer_parts(layer) {
                self.parts.push((id, kind, data));
            }
        }
        let int = Value::Integer;
        let mut typed = vec![
            ("MainId", int(id)),
            ("LayerName", Value::Text(layer.name.clone())),
            ("LayerType", int(layer.layer_type)),
            ("LayerFolder", int(layer.folder)),
            ("LayerLock", int(layer.lock)),
            ("LayerClip", int(layer.clip)),
            ("LayerOpacity", int(layer.opacity)),
            ("LayerComposite", int(layer.composite)),
            ("LayerVisibility", int(layer.visibility)),
            ("LayerOffsetX", int(layer.offset[0])),
            ("LayerOffsetY", int(layer.offset[1])),
            ("LayerRenderOffscrOffsetX", int(layer.render_offset[0])),
            ("LayerRenderOffscrOffsetY", int(layer.render_offset[1])),
            ("LayerUuid", Value::Text(uuid)),
            ("DrawColorEnable", int(layer.draw_color.enable)),
            ("DrawColorMainRed", int(layer.draw_color.main[0])),
            ("DrawColorMainGreen", int(layer.draw_color.main[1])),
            ("DrawColorMainBlue", int(layer.draw_color.main[2])),
            ("LayerNextIndex", int(next)),
        ];
        if layer.is_folder() || root {
            typed.push(("LayerFirstChildIndex", int(first)));
        }
        if native.mask.is_some() {
            // The mask made for a fill or gradient layer is on.
            for (name, value) in &mut typed {
                if *name == "LayerVisibility" {
                    *value = int(layer.visibility | 2);
                }
            }
        }
        for (name, value) in native.typed {
            match typed.iter_mut().find(|(known, _)| *known == name) {
                Some(slot) => slot.1 = value,
                None => typed.push((name, value)),
            }
        }
        if let Some(render) = render {
            typed.push(("LayerRenderMipmap", int(render)));
        }
        if let Some((mask, mipmap)) = mask {
            typed.extend([
                ("LayerLayerMaskMipmap", int(mipmap)),
                ("LayerMaskOffsetX", int(mask.offset[0])),
                ("LayerMaskOffsetY", int(mask.offset[1])),
                ("LayerMaskOffscrOffsetX", int(mask.offscreen_offset[0])),
                ("LayerMaskOffscrOffsetY", int(mask.offscreen_offset[1])),
            ]);
        }
        let made = vec![
            ("CanvasId", int(self.canvas)),
            ("LayerMasking", int(if root { 32 } else { 0 })),
            ("LayerSelect", int(0)),
            ("LayerUsePaletteColor", int(0)),
            ("LayerNoticeablePaletteColor", int(0)),
            ("LayerPaletteRed", int(0)),
            ("LayerPaletteGreen", int(0)),
            ("LayerPaletteBlue", int(0)),
            ("LayerFirstChildIndex", int(0)),
            ("LayerRenderMipmap", int(0)),
            ("LayerLayerMaskMipmap", int(0)),
            ("LayerMaskOffsetX", int(0)),
            ("LayerMaskOffsetY", int(0)),
            ("LayerMaskOffscrOffsetX", int(0)),
            ("LayerMaskOffscrOffsetY", int(0)),
            ("LayerRenderThumbnail", int(render.unwrap_or(0))),
            (
                "LayerLayerMaskThumbnail",
                int(mask.map_or(0, |(_, mipmap)| mipmap)),
            ),
        ];
        if !root && layer.folder == 0 {
            self.top = id;
        }
        self.layers.rows.push(Row {
            typed,
            kept: layer.columns.clone(),
            made,
        });
    }

    /// Writes `layers` (bottom first) as the children of a folder;
    /// answers the first one's id.
    fn nodes(&mut self, layers: &[ClipLayer]) -> i64 {
        let ids: Vec<i64> = layers.iter().map(|layer| self.id(0, layer.id)).collect();
        for (i, layer) in layers.iter().enumerate() {
            let next = ids.get(i + 1).copied().unwrap_or(0);
            self.layer(layer, ids[i], next, false);
        }
        ids.first().copied().unwrap_or(0)
    }
}

// ----- Clip Studio's own tables of vector layers ------------------------------------

/// The kept table `name`, when the document has one.
fn kept_table_named<'a>(doc: &'a ClipDocument, name: &str) -> Option<&'a ClipTable> {
    doc.foreign.tables.iter().find(|table| table.name == name)
}

/// The value of `name` in a kept row.
fn cell<'a>(table: &'a ClipTable, values: &'a [Value], name: &str) -> Option<&'a Value> {
    let at = table.columns.iter().position(|column| column == name)?;
    values.get(at)
}

/// The highest `MainId` of a kept table.
fn highest_main(table: &ClipTable) -> i64 {
    table
        .rows
        .iter()
        .filter_map(|(_, values)| cell(table, values, "MainId").and_then(Value::int))
        .max()
        .unwrap_or(0)
}

/// Whether two values say the same: a whole number and a real are the
/// same number either way (SQLite stores a real with no fraction as a
/// whole number, and reads it back as one).
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Integer(_) | Value::Real(_), Value::Integer(_) | Value::Real(_)) => {
            a.real() == b.real()
        }
        _ => a == b,
    }
}

/// The stroke data of a kept chunk, by its id.
fn kept_chunk<'a>(doc: &'a ClipDocument, id: &str) -> Option<&'a [u8]> {
    doc.foreign.chunks.iter().find_map(|chunk| {
        let (found, payload) = (super::container::Chunk {
            kind: chunk.kind,
            offset: 0,
            data: &chunk.data,
        })
        .external()
        .ok()?;
        (&chunk.kind == b"Exta" && found == id).then_some(payload)
    })
}

/// The strokes the kept `VectorObjectList` row `index` points at.
fn kept_strokes(doc: &ClipDocument, index: i64) -> Option<super::kinds::ClipVector> {
    let table = kept_table_named(doc, "VectorObjectList")?;
    let (_, id) = native::stroke_row(table, index)?;
    native::vector_of(kept_chunk(doc, &id)?)
}

/// Appends a row of `values` (by column name) to a kept table; a column
/// the table lacks is added to it.
fn append_row(table: &mut ClipTable, values: &[(&str, Value)]) {
    for (name, _) in values {
        if !table.columns.iter().any(|column| column == name)
            && let Some(close) = table.sql.rfind(')')
        {
            table.sql.insert_str(close, &format!(", {name}"));
            table.columns.push((*name).to_owned());
            for (_, row) in &mut table.rows {
                row.push(Value::Null);
            }
        }
    }
    let key = rowid_column(&table.sql);
    let rowid = table
        .rows
        .iter()
        .map(|(rowid, _)| *rowid)
        .max()
        .unwrap_or(0)
        + 1;
    let row = table
        .columns
        .iter()
        .enumerate()
        .map(|(i, name)| {
            if Some(i) == key {
                return Value::Integer(rowid);
            }
            values
                .iter()
                .find(|(column, _)| column == name)
                .map_or(Value::Null, |(_, value)| value.clone())
        })
        .collect();
    table.rows.push((rowid, row));
}

/// Sets `name` of the rows of a kept table whose `MainId` is `main`.
fn set_cell(table: &mut ClipTable, main: i64, name: &str, value: &Value) {
    let (Some(key), Some(at)) = (
        table.columns.iter().position(|column| column == "MainId"),
        table.columns.iter().position(|column| column == name),
    ) else {
        return;
    };
    for (_, row) in &mut table.rows {
        if row.get(key).and_then(Value::int) == Some(main) {
            row[at] = value.clone();
        }
    }
}

/// The last row of a list linked by `NextIndex` in a kept table, from
/// the row `first`.
fn last_of_list(table: &ClipTable, first: i64) -> Option<i64> {
    let mut at = first;
    let mut seen = HashSet::new();
    loop {
        if at == 0 || !seen.insert(at) {
            return None;
        }
        let next = table.rows.iter().find_map(|(_, values)| {
            (cell(table, values, "MainId")?.int()? == at)
                .then(|| cell(table, values, "NextIndex").and_then(Value::int))?
        })?;
        if next == 0 {
            return Some(at);
        }
        at = next;
    }
}

/// Where the plain pen of Sue's vector lines is in the document's brush
/// tables: found among the kept ones, or added after them.
struct Brush {
    /// Its `BrushStyle` `MainId`.
    style: i64,
    /// Its two `BrushEffectorGraphData` `MainId`s.
    graphs: [i64; 2],
    /// The style and the graphs must be added.
    add_style: bool,
    add_graphs: bool,
}

impl Brush {
    fn of(doc: &ClipDocument) -> Self {
        let graphs_table = kept_table_named(doc, "BrushEffectorGraphData");
        let pen = native::pen_graphs();
        let found_graphs: Option<Vec<i64>> = graphs_table.and_then(|table| {
            pen.iter()
                .map(|(number, size, points)| {
                    table.rows.iter().find_map(|(_, values)| {
                        let same = cell(table, values, "ControlNumber").and_then(Value::int)
                            == Some(*number)
                            && cell(table, values, "ControlDataSize").and_then(Value::int)
                                == Some(*size)
                            && cell(table, values, "ControlPoints").and_then(Value::bytes)
                                == Some(points.as_slice());
                        if same {
                            cell(table, values, "MainId").and_then(Value::int)
                        } else {
                            None
                        }
                    })
                })
                .collect()
        });
        let (graphs, add_graphs) = match found_graphs.as_deref() {
            Some(&[a, b]) => ([a, b], false),
            _ => {
                let top = graphs_table.map_or(0, highest_main);
                ([top + 1, top + 2], true)
            }
        };
        let styles = kept_table_named(doc, "BrushStyle");
        let template = native::pen_style(graphs.map(|id| u32::try_from(id).unwrap_or(0)));
        let found_style = styles.and_then(|table| {
            table.rows.iter().find_map(|(_, values)| {
                template
                    .iter()
                    .all(|(name, value)| {
                        cell(table, values, name).is_some_and(|kept| same(kept, value))
                    })
                    .then(|| cell(table, values, "MainId").and_then(Value::int))?
            })
        });
        match found_style {
            Some(style) if !add_graphs => Self {
                style,
                graphs,
                add_style: false,
                add_graphs,
            },
            _ => Self {
                style: styles.map_or(0, highest_main) + 1,
                graphs,
                add_style: true,
                add_graphs,
            },
        }
    }

    /// The brush tables with the pen in them; `canvas` is the canvas row's
    /// id. Answers the tables changed or made, and the manager's id.
    fn tables(&self, doc: &ClipDocument, canvas: i64) -> (Vec<ClipTable>, i64) {
        let table = |name: &str, sql: &str| {
            kept_table_named(doc, name)
                .cloned()
                .unwrap_or_else(|| ClipTable {
                    name: name.to_owned(),
                    sql: sql.to_owned(),
                    columns: sql_columns(sql),
                    rows: Vec::new(),
                })
        };
        let int = Value::Integer;
        let mut manager = table("BrushStyleManager", native::BRUSH_MANAGER_SQL);
        if manager.rows.is_empty() {
            append_row(
                &mut manager,
                &[
                    ("MainId", int(1)),
                    ("CanvasId", int(canvas)),
                    ("FirstGraphData", int(0)),
                    ("FirstPatternImage", int(0)),
                    ("FirstPattern", int(0)),
                    ("FirstFixedSpray", int(0)),
                    ("FirstBrushStyle", int(0)),
                    ("FirstFillStyle", int(0)),
                ],
            );
        }
        let (_, first_row) = &manager.rows[0];
        let manager_id = cell(&manager, first_row, "MainId")
            .and_then(Value::int)
            .unwrap_or(1);
        let first = |name: &str| {
            cell(&manager, &manager.rows[0].1, name)
                .and_then(Value::int)
                .unwrap_or(0)
        };
        let (first_graph, first_style) = (first("FirstGraphData"), first("FirstBrushStyle"));
        let mut graphs = table("BrushEffectorGraphData", native::BRUSH_GRAPH_SQL);
        if self.add_graphs {
            match last_of_list(&graphs, first_graph) {
                Some(last) => set_cell(&mut graphs, last, "NextIndex", &int(self.graphs[0])),
                None => set_cell(
                    &mut manager,
                    manager_id,
                    "FirstGraphData",
                    &int(self.graphs[0]),
                ),
            }
            for (i, (number, size, points)) in native::pen_graphs().into_iter().enumerate() {
                let next = if i == 0 { self.graphs[1] } else { 0 };
                append_row(
                    &mut graphs,
                    &[
                        ("MainId", int(self.graphs[i])),
                        ("CanvasId", int(canvas)),
                        ("NextIndex", int(next)),
                        ("ControlNumber", int(number)),
                        ("ControlDataSize", int(size)),
                        ("ControlPoints", Value::Blob(points)),
                    ],
                );
            }
        }
        let mut styles = table("BrushStyle", native::BRUSH_STYLE_SQL);
        if self.add_style {
            match last_of_list(&styles, first_style) {
                Some(last) => set_cell(&mut styles, last, "NextIndex", &int(self.style)),
                None => set_cell(
                    &mut manager,
                    manager_id,
                    "FirstBrushStyle",
                    &int(self.style),
                ),
            }
            let mut row = vec![
                ("MainId", int(self.style)),
                ("CanvasId", int(canvas)),
                ("NextIndex", int(0)),
            ];
            row.extend(native::pen_style(
                self.graphs.map(|id| u32::try_from(id).unwrap_or(0)),
            ));
            append_row(&mut styles, &row);
        }
        (vec![manager, graphs, styles], manager_id)
    }
}

/// The highest `MainId` in the document's layers, bitmaps and kept rows
/// of those tables, and the highest layer a kept row of Sue's own layer
/// tables names.
fn highest(doc: &ClipDocument) -> i64 {
    fn walk(layer: &ClipLayer, top: &mut i64) {
        *top = (*top).max(layer.id);
        let mask = layer.mask.as_ref().map(|mask| &mask.bitmap);
        for bitmap in [layer.pixels.as_ref(), mask].into_iter().flatten() {
            *top = bitmap.ids.iter().fold(*top, |top, id| top.max(*id));
        }
        for child in &layer.children {
            walk(child, top);
        }
    }
    let mut top = 2;
    walk(&doc.root, &mut top);
    for row in &doc.foreign.rows {
        if ["Layer", "Mipmap", "MipmapInfo", "Offscreen"].contains(&row.table.as_str())
            && let Some(id) = column(&row.columns, "MainId").and_then(Value::int)
        {
            top = top.max(id);
        }
        // A kept row of Sue's own tables names its layer: a new layer
        // must never take that id, or the row would ride on it.
        if ["SueLayer", "SueLayerPart"].contains(&row.table.as_str())
            && let Some(id) = column(&row.columns, "LayerId").and_then(Value::int)
        {
            top = top.max(id);
        }
    }
    top
}

/// Writes `doc` as a Clip Studio file.
///
/// # Errors
/// When the canvas is empty or too large, or what is kept from another
/// file does not fit (a kept table named as one Sue writes, a kept row of
/// a table Sue does not write, a shape with no columns).
#[allow(clippy::too_many_lines)]
pub fn write(doc: &ClipDocument) -> Result<Vec<u8>, String> {
    if doc.width == 0 || doc.height == 0 || doc.width > 300_000 || doc.height > 300_000 {
        return Err(format!(
            "a canvas of {} by {} pixels cannot be saved",
            doc.width, doc.height
        ));
    }
    let mut names_seen = HashSet::new();
    for table in &doc.foreign.tables {
        if MODELED
            .iter()
            .chain(&["sqlite_sequence"])
            .any(|name| name.eq_ignore_ascii_case(&table.name))
            || !names_seen.insert(table.name.to_ascii_lowercase())
        {
            return Err(format!(
                "the kept table {} clashes with another table",
                table.name
            ));
        }
    }
    if let Some(row) = doc.foreign.rows.iter().find(|row| {
        !MODELED
            .iter()
            .any(|name| *name == row.table && *name != "ExternalChunk")
    }) {
        return Err(format!(
            "a kept row belongs to the table {}, which Sue does not write",
            row.table
        ));
    }
    let canvas = if doc.canvas_id == 0 { 1 } else { doc.canvas_id };
    let mut writer = Writer {
        doc,
        ids: Ids::new(doc),
        next: highest(doc),
        used: Default::default(),
        uuids: HashSet::new(),
        externals: HashSet::new(),
        canvas,
        layers: Out::new(
            doc,
            "Layer",
            table_sql("Layer", LAYER_COLUMNS),
            LAYER_COLUMNS,
        ),
        mipmaps: Out::new(
            doc,
            "Mipmap",
            table_sql("Mipmap", MIPMAP_COLUMNS),
            MIPMAP_COLUMNS,
        ),
        levels: Out::new(
            doc,
            "MipmapInfo",
            table_sql("MipmapInfo", MIPMAP_INFO_COLUMNS),
            MIPMAP_INFO_COLUMNS,
        ),
        offscreens: Out::new(
            doc,
            "Offscreen",
            table_sql("Offscreen", OFFSCREEN_COLUMNS),
            OFFSCREEN_COLUMNS,
        ),
        thumbnails: Vec::new(),
        marks: Vec::new(),
        parts: Vec::new(),
        exta: Vec::new(),
        top: 0,
        vectors: Vectors::default(),
    };
    let brush = Brush::of(doc);
    writer.vectors.brush = u32::try_from(brush.style).unwrap_or(1);
    writer.vectors.next = kept_table_named(doc, "VectorObjectList").map_or(0, highest_main) + 1;
    // The root is 2 when it has no id of its own, as Sue has always
    // written it; layers and bitmaps count on from the highest id.
    let root = if doc.root.id == 0 && writer.next == 2 {
        writer.used[0].insert(2);
        2
    } else {
        writer.id(0, doc.root.id)
    };
    writer.layer(&doc.root, root, 0, true);
    let top_before_root = writer.top;

    // Clip Studio's own tables of vector layers: kept rows a layer still
    // points at stay (with that layer's id), the others go with their
    // chunks, and the new ones are added.
    let vectors = std::mem::take(&mut writer.vectors);
    let mut changed: Vec<ClipTable> = Vec::new();
    let mut dropped_chunks: HashSet<String> = HashSet::new();
    let kept_list = kept_table_named(doc, "VectorObjectList");
    if kept_list.is_some() || !vectors.rows.is_empty() {
        let mut list = kept_list.cloned().unwrap_or_else(|| ClipTable {
            name: "VectorObjectList".to_owned(),
            sql: native::VECTOR_OBJECT_LIST_SQL.to_owned(),
            columns: sql_columns(native::VECTOR_OBJECT_LIST_SQL),
            rows: Vec::new(),
        });
        let main = list.columns.iter().position(|column| column == "MainId");
        let layer_at = list.columns.iter().position(|column| column == "LayerId");
        let data = list
            .columns
            .iter()
            .position(|column| column == "VectorData");
        list.rows.retain_mut(|(_, values)| {
            let id = main.and_then(|at| values.get(at)).and_then(Value::int);
            match id.and_then(|id| vectors.kept.get(&id)) {
                Some(layer) => {
                    if let Some(at) = layer_at {
                        values[at] = Value::Integer(*layer);
                    }
                    true
                }
                None => {
                    if let Some(chunk) = data.and_then(|at| values.get(at)).and_then(Value::bytes) {
                        dropped_chunks.insert(String::from_utf8_lossy(chunk).into_owned());
                    }
                    false
                }
            }
        });
        for (main, layer, external) in &vectors.rows {
            append_row(
                &mut list,
                &[
                    ("MainId", Value::Integer(*main)),
                    ("CanvasId", Value::Integer(canvas)),
                    ("LayerId", Value::Integer(*layer)),
                    ("VectorData", Value::Blob(external.clone().into_bytes())),
                ],
            );
        }
        changed.push(list);
    }
    let mut brush_manager = None;
    if !vectors.rows.is_empty() {
        let (tables, manager) = brush.tables(doc, canvas);
        changed.extend(tables);
        brush_manager = Some(manager);
    }

    // Kept rows go after the rows of the layers.
    for row in &doc.foreign.rows {
        let out = match row.table.as_str() {
            "Layer" => &mut writer.layers,
            "Mipmap" => &mut writer.mipmaps,
            "MipmapInfo" => &mut writer.levels,
            "Offscreen" => &mut writer.offscreens,
            _ => continue,
        };
        out.rows.push(Row {
            kept: row.columns.clone(),
            ..Row::default()
        });
    }

    // The chunks: the bitmaps' offsets go into the database, and the
    // database's into the head.
    let head = match &doc.head {
        ClipHead::Sue { id } => {
            let id = if *id == [0; 16] {
                writer.ids.next()
            } else {
                *id
            };
            Head::Sue(id)
        }
        ClipHead::Raw(data) => Head::Raw(data),
    };
    let head_size = match &head {
        Head::Sue(_) => 40,
        Head::Raw(data) => data.len(),
    };
    let mut offset = 24 + doc.foreign.preamble.len() + 16 + head_size;
    let mut chunks: Vec<Vec<u8>> = Vec::new();
    let mut external = Vec::new();
    let mut exta = std::mem::take(&mut writer.exta);
    exta.extend(vectors.chunks.iter().cloned());
    for (id, payload) in &exta {
        let mut data = Vec::with_capacity(payload.len() + 64);
        data.extend_from_slice(&(id.len() as u64).to_be_bytes());
        data.extend_from_slice(id.as_bytes());
        data.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        data.extend_from_slice(payload);
        external.push((id.clone(), offset));
        offset += 16 + data.len();
        chunks.push(chunk(*b"Exta", &data));
    }
    for kept in &doc.foreign.chunks {
        if &kept.kind == b"Exta"
            && let Ok((id, _)) = (super::container::Chunk {
                kind: kept.kind,
                offset: 0,
                data: &kept.data,
            })
            .external()
        {
            if dropped_chunks.contains(&id) {
                continue;
            }
            external.push((id, offset));
        }
        offset += 16 + kept.data.len();
        chunks.push(chunk(kept.kind, &kept.data));
    }
    let database_at = offset;

    let int = Value::Integer;
    let skeleton = |name: &str, columns: &str, row: Vec<Value>| match doc
        .foreign
        .tables
        .iter()
        .find(|table| table.name == name)
    {
        Some(table) => kept_table(table),
        None => NewTable {
            name: name.to_owned(),
            sql: table_sql(name, columns),
            rows: vec![(1, row)],
        },
    };
    let mut tables = vec![
        skeleton(
            "Project",
            PROJECT_COLUMNS,
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
        ),
        skeleton(
            "CanvasItemBank",
            CANVAS_ITEM_BANK_COLUMNS,
            vec![Value::Null, int(1), int(0), int(0)],
        ),
        skeleton(
            "AnimationCutBank",
            ANIMATION_CUT_BANK_COLUMNS,
            vec![Value::Null, int(2), int(0), int(0), int(0), int(0)],
        ),
        skeleton(
            "LayerCompManager",
            LAYER_COMP_MANAGER_COLUMNS,
            vec![Value::Null, int(3), int(0), int(0), int(-1)],
        ),
    ];
    let mut canvas_out = Out::new(
        doc,
        "Canvas",
        table_sql("Canvas", CANVAS_COLUMNS),
        CANVAS_COLUMNS,
    );
    let dpi = if doc.dpi > 0.0 { doc.dpi } else { 72.0 };
    canvas_out.rows.push(Row {
        typed: vec![
            ("MainId", int(canvas)),
            ("CanvasWidth", Value::Real(f64::from(doc.width))),
            ("CanvasHeight", Value::Real(f64::from(doc.height))),
            ("CanvasResolution", Value::Real(dpi)),
            ("CanvasRootFolder", int(root)),
        ]
        .into_iter()
        .chain(
            brush_manager
                .filter(|_| {
                    column(&doc.canvas_columns, "BrushStyleManager")
                        .and_then(Value::int)
                        .unwrap_or(0)
                        == 0
                })
                .map(|manager| ("BrushStyleManager", int(manager))),
        )
        .collect(),
        kept: doc.canvas_columns.clone(),
        made: vec![
            ("CanvasUnit", int(0)),
            ("CanvasChannelBytes", int(1)),
            ("CanvasDefaultChannelOrder", int(33)),
            ("CanvasCurrentLayer", int(top_before_root)),
            ("CanvasDoSimulateColor", int(0)),
            ("CanvasUseColorAdjustment", int(0)),
            ("CanvasDefaultColorTypeIndex", int(0)),
            ("CanvasDefaultColorBlackChecked", int(1)),
            ("CanvasDefaultColorWhiteChecked", int(1)),
            ("CanvasDefaultToneLine", Value::Real(60.0)),
            ("CanvasDoublePage", int(0)),
            ("CanvasRenderMipmapForceSaved", int(0)),
            ("CanvasWorkTime", int(0)),
        ],
    });
    push_kept(doc, "Canvas", &mut canvas_out);
    tables.push(canvas_out.finish()?);
    let thumbnails = std::mem::take(&mut writer.thumbnails);
    let marks = std::mem::take(&mut writer.marks);
    let layer_part_rows = std::mem::take(&mut writer.parts);
    for out in [
        writer.layers,
        writer.mipmaps,
        writer.levels,
        writer.offscreens,
    ] {
        tables.push(out.finish()?);
    }
    tables.push(
        match doc
            .foreign
            .tables
            .iter()
            .find(|table| table.name == "LayerThumbnail")
        {
            Some(table) => kept_table(table),
            None => NewTable {
                name: "LayerThumbnail".to_owned(),
                sql: table_sql("LayerThumbnail", LAYER_THUMBNAIL_COLUMNS),
                rows: thumbnails,
            },
        },
    );
    let mut external_out = Out::new(
        doc,
        "ExternalChunk",
        EXTERNAL_CHUNK_SQL.to_owned(),
        "ExternalID BLOB, Offset INTEGER",
    );
    for (id, offset) in external {
        let kept = doc
            .foreign
            .external
            .iter()
            .find(|(known, _)| *known == id)
            .map(|(_, columns)| columns.clone())
            .unwrap_or_default();
        external_out.rows.push(Row {
            typed: vec![
                ("ExternalID", Value::Text(id)),
                ("Offset", int(i64::try_from(offset).unwrap_or(0))),
            ],
            kept,
            made: Vec::new(),
        });
    }
    tables.push(external_out.finish()?);
    let mut names_table = doc
        .foreign
        .tables
        .iter()
        .find(|table| table.name == "ExternalTableAndColumnName")
        .cloned()
        .unwrap_or_else(|| ClipTable {
            name: "ExternalTableAndColumnName".to_owned(),
            sql: EXTERNAL_NAMES_SQL.to_owned(),
            columns: sql_columns(EXTERNAL_NAMES_SQL),
            rows: Vec::new(),
        });
    let mut needed = vec![("Offscreen", "BlockData")];
    if !vectors.rows.is_empty() {
        needed.push(("VectorObjectList", "VectorData"));
    }
    for (table, name) in needed {
        let named = names_table.rows.iter().any(|(_, values)| {
            cell(&names_table, values, "TableName").and_then(Value::text) == Some(table)
                && cell(&names_table, values, "ColumnName").and_then(Value::text) == Some(name)
        });
        if !named {
            append_row(
                &mut names_table,
                &[
                    ("TableName", Value::Text(table.to_owned())),
                    ("ColumnName", Value::Text(name.to_owned())),
                ],
            );
        }
    }
    tables.push(kept_table(&names_table));
    let mut sue_project = Out::new(
        doc,
        "SueProject",
        SUE_PROJECT_SQL.to_owned(),
        "Version INTEGER, Data BLOB",
    );
    if let Some(sue) = &doc.sue {
        sue_project.rows.push(Row {
            typed: vec![("Data", Value::Blob(sue.clone()))],
            kept: doc.foreign.sue_columns.clone(),
            made: vec![("Version", int(1))],
        });
    }
    push_kept(doc, "SueProject", &mut sue_project);
    if !sue_project.rows.is_empty() {
        tables.push(sue_project.finish()?);
    }
    let mut sue_layer = Out::new(
        doc,
        "SueLayer",
        SUE_LAYER_SQL.to_owned(),
        "LayerId INTEGER, Draft INTEGER, Reference INTEGER",
    );
    for (id, draft, reference) in marks {
        sue_layer.rows.push(Row {
            typed: vec![
                ("LayerId", int(id)),
                ("Draft", int(i64::from(draft))),
                ("Reference", int(i64::from(reference))),
            ],
            ..Row::default()
        });
    }
    push_kept(doc, "SueLayer", &mut sue_layer);
    if !sue_layer.rows.is_empty() {
        tables.push(sue_layer.finish()?);
    }
    let mut layer_parts_out = Out::new(
        doc,
        "SueLayerPart",
        SUE_LAYER_PART_SQL.to_owned(),
        SUE_LAYER_PART_COLUMNS,
    );
    for (id, kind, data) in layer_part_rows {
        layer_parts_out.rows.push(part_row(Some(id), kind, data));
    }
    push_kept(doc, "SueLayerPart", &mut layer_parts_out);
    if !layer_parts_out.rows.is_empty() {
        tables.push(layer_parts_out.finish()?);
    }
    let mut document_parts_out = Out::new(
        doc,
        "SueDocumentPart",
        SUE_DOCUMENT_PART_SQL.to_owned(),
        SUE_DOCUMENT_PART_COLUMNS,
    );
    for (kind, data) in document_parts(doc) {
        document_parts_out.rows.push(part_row(None, kind, data));
    }
    push_kept(doc, "SueDocumentPart", &mut document_parts_out);
    if !document_parts_out.rows.is_empty() {
        tables.push(document_parts_out.finish()?);
    }
    for table in &doc.foreign.tables {
        let skeleton = [
            "Project",
            "CanvasItemBank",
            "AnimationCutBank",
            "LayerCompManager",
            "LayerThumbnail",
            "ExternalTableAndColumnName",
        ];
        if skeleton.contains(&table.name.as_str()) {
            continue;
        }
        match changed.iter().position(|made| made.name == table.name) {
            Some(at) => tables.push(kept_table(&changed.remove(at))),
            None => tables.push(kept_table(table)),
        }
    }
    for table in &changed {
        tables.push(kept_table(table));
    }
    raise_max_index(&mut tables);
    let database = sqlite::write(&tables);

    let mut out = Vec::new();
    out.extend_from_slice(b"CSFCHUNK");
    out.extend_from_slice(&0_u64.to_be_bytes());
    out.extend_from_slice(&((24 + doc.foreign.preamble.len()) as u64).to_be_bytes());
    out.extend_from_slice(&doc.foreign.preamble);
    match head {
        Head::Sue(id) => {
            let mut data = Vec::with_capacity(40);
            data.extend_from_slice(&256_u64.to_be_bytes());
            data.extend_from_slice(&(database_at as u64).to_be_bytes());
            data.extend_from_slice(&16_u64.to_be_bytes());
            data.extend_from_slice(&id);
            out.extend_from_slice(&chunk(*b"Head", &data));
        }
        Head::Raw(data) => out.extend_from_slice(&chunk(*b"Head", data)),
    }
    for chunk in &chunks {
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&chunk(*b"SQLi", &database));
    out.extend_from_slice(&chunk(*b"Foot", &doc.foreign.foot));
    out.extend_from_slice(&doc.foreign.trailing);
    let total = out.len() as u64;
    out[8..16].copy_from_slice(&total.to_be_bytes());
    Ok(out)
}

enum Head<'a> {
    Sue([u8; 16]),
    Raw(&'a [u8]),
}

fn chunk(kind: [u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + data.len());
    out.extend_from_slice(b"CHNK");
    out.extend_from_slice(&kind);
    out.extend_from_slice(&(data.len() as u64).to_be_bytes());
    out.extend_from_slice(data);
    out
}

/// In a kept `ElemScheme` (the table Clip Studio counts the ids it gave
/// out in), raises each table's `MaxIndex` to the highest `MainId` the
/// file now holds, so Clip Studio never gives out an id Sue used.
fn raise_max_index(tables: &mut [NewTable]) {
    let mut highest: HashMap<String, i64> = HashMap::new();
    for table in tables.iter() {
        let columns = sql_columns(&table.sql);
        if let Some(at) = columns.iter().position(|column| column == "MainId") {
            let top = table
                .rows
                .iter()
                .filter_map(|(_, values)| values.get(at).and_then(Value::int))
                .max();
            if let Some(top) = top {
                highest.insert(table.name.clone(), top);
            }
        }
    }
    let Some(scheme) = tables.iter_mut().find(|table| table.name == "ElemScheme") else {
        return;
    };
    let columns = sql_columns(&scheme.sql);
    let (Some(name_at), Some(max_at)) = (
        columns.iter().position(|column| column == "TableName"),
        columns.iter().position(|column| column == "MaxIndex"),
    ) else {
        return;
    };
    for (_, values) in &mut scheme.rows {
        let Some(name) = values.get(name_at).and_then(Value::text) else {
            continue;
        };
        if let Some(top) = highest.get(name)
            && values.get(max_at).and_then(Value::int).unwrap_or(0) < *top
        {
            values[max_at] = Value::Integer(*top);
        }
    }
}

/// A row of one of Sue's part tables.
fn part_row(layer: Option<i64>, kind: &str, data: Vec<u8>) -> Row {
    let mut typed = Vec::with_capacity(4);
    if let Some(id) = layer {
        typed.push(("LayerId", Value::Integer(id)));
    }
    typed.extend([
        ("Kind", Value::Text(kind.to_owned())),
        ("Version", Value::Integer(VERSION)),
        ("Data", Value::Blob(data)),
    ]);
    Row {
        typed,
        ..Row::default()
    }
}

fn push_kept(doc: &ClipDocument, table: &str, out: &mut Out) {
    for row in doc.foreign.rows.iter().filter(|row| row.table == table) {
        out.rows.push(Row {
            kept: row.columns.clone(),
            ..Row::default()
        });
    }
}

// ----- From Photoshop records, as Sue Paint saves today ---------------------------

/// A layer, or a folder of them, among Photoshop records.
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

fn styled(record: &PsdLayer, mut layer: ClipLayer) -> ClipLayer {
    layer.set_opacity_u8(record.opacity);
    layer.visibility = i64::from(record.visible);
    layer.clip = i64::from(record.clip);
    layer.composite = composite_of(record.blend);
    layer
}

/// `record`'s pixels where they sit on a `width` by `height` canvas.
fn canvas_pixels(record: &PsdLayer, width: u32, height: u32) -> ClipBitmap {
    ClipBitmap::color_placed(
        width,
        height,
        [i64::from(record.left), i64::from(record.top)],
        [record.width, record.height],
        &record.pixels,
    )
}

fn layers_of(nodes: &[Node<'_>], width: u32, height: u32) -> Vec<ClipLayer> {
    nodes
        .iter()
        .map(|node| match node {
            Node::Pixels(record) => styled(
                record,
                ClipLayer::raster(&record.name, 0, 0, canvas_pixels(record, width, height)),
            ),
            Node::Folder(record, children) => {
                let mut folder = styled(record, ClipLayer::folder(&record.name));
                folder.layer_type = FOLDER;
                folder.set_open(!matches!(record.kind, PsdKind::FolderOpen { open: false }));
                folder.pixels = Some(ClipBitmap::empty(width, height));
                folder.children = layers_of(children, width, height);
                folder
            }
        })
        .collect()
}

/// The document Sue Paint's records make: every layer a canvas sized
/// color bitmap, folders and the root with their empty ones.
#[must_use]
pub fn document_of(psd: &Psd, dpi: f64, sue: Option<&[u8]>) -> ClipDocument {
    let mut doc = ClipDocument::new(psd.width, psd.height, dpi);
    doc.root.children = layers_of(&tree(&psd.layers), psd.width, psd.height);
    doc.sue = sue.map(<[u8]>::to_vec);
    doc
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
    write(&document_of(psd, dpi, sue))
}
