//! Writing a `.clip`: [`Psd`] records to layer rows, each with its stored
//! bitmap, laid out as chunks around a fresh database.

use super::offscreen::{attribute, encode_bitmap};
use super::schema::{
    ANIMATION_CUT_BANK_COLUMNS, CANVAS_COLUMNS, CANVAS_ITEM_BANK_COLUMNS, LAYER_COLUMNS,
    LAYER_COMP_MANAGER_COLUMNS, LAYER_THUMBNAIL_COLUMNS, MIPMAP_COLUMNS, MIPMAP_INFO_COLUMNS,
    OFFSCREEN_COLUMNS, PROJECT_COLUMNS, composite_of, table,
};
use crate::psd::{Psd, PsdKind, PsdLayer};
use crate::sqlite::{self, NewTable, Value};

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
            PROJECT_COLUMNS,
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
            CANVAS_ITEM_BANK_COLUMNS,
            vec![(1, vec![Value::Null, int(1), int(0), int(0)])],
        ),
        table(
            "AnimationCutBank",
            ANIMATION_CUT_BANK_COLUMNS,
            vec![(1, vec![Value::Null, int(2), int(0), int(0), int(0), int(0)])],
        ),
        table(
            "LayerCompManager",
            LAYER_COMP_MANAGER_COLUMNS,
            vec![(1, vec![Value::Null, int(3), int(0), int(0), int(-1)])],
        ),
        table(
            "Canvas",
            CANVAS_COLUMNS,
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
            MIPMAP_COLUMNS,
            std::mem::take(&mut writer.mipmaps),
        ),
        table(
            "MipmapInfo",
            MIPMAP_INFO_COLUMNS,
            std::mem::take(&mut writer.levels),
        ),
        table(
            "Offscreen",
            OFFSCREEN_COLUMNS,
            std::mem::take(&mut writer.offscreens),
        ),
        table(
            "LayerThumbnail",
            LAYER_THUMBNAIL_COLUMNS,
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
