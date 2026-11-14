//! `ClipDocument`: a `.clip` as a neutral tree, the one thing the reader
//! makes and the writer takes.
//!
//! The model names what Sue understands of a `.clip` (the canvas, the
//! layer tree, each layer's style, its pixels and its mask, Sue's own
//! project and layer marks) and keeps everything else as it was found, in
//! [`ClipForeign`] and in the `columns` of each row: chunks of a kind Sue
//! does not know, tables it does not read, columns it does not read and
//! rows nothing points at. Writing a document read from a file gives that
//! file's data back, and `write(read(write(doc)))` is byte for byte
//! `write(doc)`.
//!
//! Numbers that come from a column are kept as the file has them (opacity
//! 0 to 256, the visibility and folder bits, the blend number), so nothing
//! is rounded on the way through; helpers read them in Sue's terms.
//!
//! Pixels stay as the file stores them: 256 by 256 blocks, each zlib
//! compressed. [`ClipBitmap::decode`] unpacks them when asked, and
//! [`ClipBitmap::color`] and [`ClipBitmap::gray`] pack new ones.

use super::offscreen::{AttributePart, BlockList};
use super::schema::{blend_of, composite_of};
use crate::sqlite::Value;

/// Column values of one row, by column name, for the columns the model
/// does not name.
pub type Columns = Vec<(String, Value)>;

/// The value of `column` in `columns` (any case).
#[must_use]
pub fn column<'a>(columns: &'a Columns, column: &str) -> Option<&'a Value> {
    columns
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(column))
        .map(|(_, value)| value)
}

/// `LayerType` of a raster layer, as Sue writes it.
pub const RASTER: i64 = 1;
/// `LayerType` of a folder, as Sue writes it.
pub const FOLDER: i64 = 0;
/// `LayerType` of the root folder, as Sue writes it.
pub const ROOT: i64 = 256;
/// `LayerType` of a paper layer.
pub const PAPER: i64 = 1584;

/// A whole `.clip`.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipDocument {
    /// Canvas width in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
    /// Pixels per inch.
    pub dpi: f64,
    /// The canvas row's `MainId` (0: Sue picks 1).
    pub canvas_id: i64,
    /// The canvas row's other columns.
    pub canvas_columns: Columns,
    /// The root folder; the layers are its children, bottom first.
    pub root: ClipLayer,
    /// The `.sue` bytes of the picture, in files Sue wrote with them.
    pub sue: Option<Vec<u8>>,
    /// The `Head` chunk.
    pub head: ClipHead,
    /// Everything read that the model does not name.
    pub foreign: ClipForeign,
}

impl ClipDocument {
    /// An empty canvas of `width` by `height` at `dpi`, with no layers.
    #[must_use]
    pub fn new(width: u32, height: u32, dpi: f64) -> Self {
        let mut root = ClipLayer::folder("");
        root.layer_type = ROOT;
        root.composite = 0;
        root.pixels = Some(ClipBitmap::empty(width, height));
        Self {
            width,
            height,
            dpi,
            canvas_id: 0,
            canvas_columns: Vec::new(),
            root,
            sue: None,
            head: ClipHead::Sue { id: [0; 16] },
            foreign: ClipForeign::default(),
        }
    }

    /// The layers on the top level, bottom first.
    #[must_use]
    pub fn layers(&self) -> &[ClipLayer] {
        &self.root.children
    }

    /// Every layer and folder, the root not included, in the order the
    /// writer lays them out (a folder after what it holds).
    #[must_use]
    pub fn all_layers(&self) -> Vec<&ClipLayer> {
        fn walk<'a>(layers: &'a [ClipLayer], out: &mut Vec<&'a ClipLayer>) {
            for layer in layers {
                walk(&layer.children, out);
                out.push(layer);
            }
        }
        let mut out = Vec::new();
        walk(&self.root.children, &mut out);
        out
    }
}

/// The `Head` chunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipHead {
    /// Laid out as Sue writes it: 256, where the database chunk starts,
    /// 16, and this id. An id of zeros is filled in when written.
    Sue {
        /// The file's id.
        id: [u8; 16],
    },
    /// Any other layout, kept as it was.
    Raw(Vec<u8>),
}

/// What a file holds that the model does not name, kept to be written
/// back.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClipForeign {
    /// The `CREATE TABLE` statements of the tables the model reads, as the
    /// file had them, so their columns come back in their order and with
    /// their types.
    pub shapes: Vec<ClipShape>,
    /// Tables the model does not read, whole.
    pub tables: Vec<ClipTable>,
    /// Rows of the tables the model reads that no layer reached (another
    /// mipmap level, a layer outside the tree), each by column name.
    pub rows: Vec<ClipRow>,
    /// Chunks of a kind Sue does not read, and stored bitmaps no layer
    /// reached, in file order.
    pub chunks: Vec<ClipChunk>,
    /// Columns of `ExternalChunk` beyond the id and the offset, by the
    /// id of the chunk they describe.
    pub external: Vec<(String, Columns)>,
    /// Columns of the `SueProject` row beyond its data.
    pub sue_columns: Columns,
    /// Bytes between the file header and the first chunk.
    pub preamble: Vec<u8>,
    /// The data of the `Foot` chunk (empty in every file seen).
    pub foot: Vec<u8>,
    /// Bytes after the last chunk.
    pub trailing: Vec<u8>,
}

impl ClipForeign {
    /// The shape of the table `table`, when one was read or set.
    #[must_use]
    pub fn shape(&self, table: &str) -> Option<&ClipShape> {
        self.shapes
            .iter()
            .find(|shape| shape.table.eq_ignore_ascii_case(table))
    }
}

/// A table's `CREATE TABLE` statement and the column names in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipShape {
    /// The table.
    pub table: String,
    /// `CREATE TABLE table(...)`.
    pub sql: String,
    /// Its columns, in order.
    pub columns: Vec<String>,
}

/// A whole table kept as it was read.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipTable {
    /// Its name.
    pub name: String,
    /// Its `CREATE TABLE` statement.
    pub sql: String,
    /// Its columns, in order.
    pub columns: Vec<String>,
    /// Its rows: rowid, then one value per column.
    pub rows: Vec<(i64, Vec<Value>)>,
}

/// One row of a table the model reads, kept by column name.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipRow {
    /// The table.
    pub table: String,
    /// Its values, the rowid column left out.
    pub columns: Columns,
}

/// One chunk kept as it was read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipChunk {
    /// Its four letter kind.
    pub kind: [u8; 4],
    /// Its data.
    pub data: Vec<u8>,
}

/// A layer, a folder or the root folder.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipLayer {
    /// `MainId` (0: the writer gives one).
    pub id: i64,
    /// `LayerName`.
    pub name: String,
    /// `LayerType`: [`RASTER`], [`FOLDER`], [`ROOT`], [`PAPER`] or a kind
    /// Sue does not know yet.
    pub layer_type: i64,
    /// `LayerFolder`: 0 for a layer; for a folder not 0, the bit worth 16
    /// set when it is closed.
    pub folder: i64,
    /// `LayerLock`, the lock bits as the file has them.
    pub lock: i64,
    /// `LayerClip`: not 0 when clipped to the layer below.
    pub clip: i64,
    /// `LayerOpacity`, 0 to 256.
    pub opacity: i64,
    /// `LayerComposite`, Clip Studio's blend number.
    pub composite: i64,
    /// `LayerVisibility`: the bit worth 1 is visible, the bit worth 2
    /// is the mask on.
    pub visibility: i64,
    /// `LayerOffsetX`, `LayerOffsetY`.
    pub offset: [i64; 2],
    /// `LayerRenderOffscrOffsetX`, `LayerRenderOffscrOffsetY`: where the
    /// pixels lie, from `offset`.
    pub render_offset: [i64; 2],
    /// `LayerUuid` (empty: the writer gives one).
    pub uuid: String,
    /// `DrawColorEnable` and `DrawColorMainRed`, `Green`, `Blue`: the
    /// layer color effect and, on paper, the paper's color.
    pub draw_color: ClipDrawColor,
    /// A draft layer (Sue's own mark).
    pub draft: bool,
    /// A reference layer (Sue's own mark).
    pub reference: bool,
    /// Its pixels (`LayerRenderMipmap`).
    pub pixels: Option<ClipBitmap>,
    /// Its mask (`LayerLayerMaskMipmap`).
    pub mask: Option<ClipMask>,
    /// What a folder holds, bottom first.
    pub children: Vec<ClipLayer>,
    /// The row's other columns.
    pub columns: Columns,
}

impl ClipLayer {
    fn blank(name: &str, layer_type: i64, folder: i64) -> Self {
        Self {
            id: 0,
            name: name.to_owned(),
            layer_type,
            folder,
            lock: 0,
            clip: 0,
            opacity: 256,
            composite: 0,
            visibility: 1,
            offset: [0; 2],
            render_offset: [0; 2],
            uuid: String::new(),
            draw_color: ClipDrawColor::default(),
            draft: false,
            reference: false,
            pixels: None,
            mask: None,
            children: Vec::new(),
            columns: Vec::new(),
        }
    }

    /// A visible, opaque, normal raster layer of `pixels`, whose top left
    /// corner is at `left`, `top` of the canvas.
    #[must_use]
    pub fn raster(name: &str, left: i64, top: i64, pixels: ClipBitmap) -> Self {
        Self {
            offset: [left, top],
            pixels: Some(pixels),
            ..Self::blank(name, RASTER, 0)
        }
    }

    /// An open, visible folder blending Through, with nothing in it.
    #[must_use]
    pub fn folder(name: &str) -> Self {
        Self {
            composite: composite_of(*b"pass"),
            ..Self::blank(name, FOLDER, 1)
        }
    }

    /// A paper layer of `color` over a canvas of `width` by `height`. Its
    /// bitmap has its (empty) blocks, as every layer that is not a folder
    /// gets: Clip Studio calls a pixel layer without them broken.
    #[must_use]
    pub fn paper(name: &str, color: [u8; 3], width: u32, height: u32) -> Self {
        Self {
            draw_color: ClipDrawColor::rgb(color),
            pixels: Some(ClipBitmap::color(width, height, &[])),
            ..Self::blank(name, PAPER, 0)
        }
    }

    /// Whether this is a folder.
    #[must_use]
    pub fn is_folder(&self) -> bool {
        self.folder != 0
    }

    /// Whether this is a paper layer.
    #[must_use]
    pub fn is_paper(&self) -> bool {
        self.layer_type == PAPER
    }

    /// Whether a folder is shown open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.folder & 16 == 0
    }

    /// Opens or closes a folder.
    pub fn set_open(&mut self, open: bool) {
        self.folder = if open {
            self.folder & !16
        } else {
            self.folder | 16
        };
    }

    /// Whether it is drawn.
    #[must_use]
    pub fn visible(&self) -> bool {
        self.visibility & 1 != 0
    }

    /// Shows or hides it.
    pub fn set_visible(&mut self, visible: bool) {
        self.visibility = (self.visibility & !1) | i64::from(visible);
    }

    /// Whether its mask is on.
    #[must_use]
    pub fn mask_enabled(&self) -> bool {
        self.visibility & 2 != 0
    }

    /// Turns its mask on or off.
    pub fn set_mask_enabled(&mut self, on: bool) {
        self.visibility = (self.visibility & !2) | (i64::from(on) << 1);
    }

    /// Whether it is clipped to the layer below.
    #[must_use]
    pub fn clipped(&self) -> bool {
        self.clip != 0
    }

    /// Opacity 0 to 255.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn opacity_u8(&self) -> u8 {
        ((self.opacity.clamp(0, 256) * 255 + 128) / 256) as u8
    }

    /// Sets the opacity from 0 to 255.
    pub fn set_opacity_u8(&mut self, opacity: u8) {
        self.opacity = (i64::from(opacity) * 256 + 127) / 255;
    }

    /// The blend mode as a Photoshop key (`norm`, `mul `, `pass`...).
    /// Two of Clip Studio's modes have no key of their own (Glow Dodge
    /// reads as `div `, Add Glow as `lddg`); `composite` keeps them.
    #[must_use]
    pub fn blend(&self) -> [u8; 4] {
        blend_of(self.composite)
    }

    /// Sets the blend mode from a Photoshop key.
    pub fn set_blend(&mut self, key: [u8; 4]) {
        self.composite = composite_of(key);
    }

    /// Where its pixels' top left corner is on the canvas.
    #[must_use]
    pub fn pixels_at(&self) -> [i64; 2] {
        [
            self.offset[0].saturating_add(self.render_offset[0]),
            self.offset[1].saturating_add(self.render_offset[1]),
        ]
    }

    /// Where its mask's top left corner is on the canvas.
    #[must_use]
    pub fn mask_at(&self) -> Option<[i64; 2]> {
        let mask = self.mask.as_ref()?;
        Some([
            self.offset[0]
                .saturating_add(mask.offset[0])
                .saturating_add(mask.offscreen_offset[0]),
            self.offset[1]
                .saturating_add(mask.offset[1])
                .saturating_add(mask.offscreen_offset[1]),
        ])
    }
}

/// The four `DrawColor` columns, as the file has them. A channel's 8-bit
/// value is its top byte.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClipDrawColor {
    /// `DrawColorEnable`.
    pub enable: i64,
    /// `DrawColorMainRed`, `DrawColorMainGreen`, `DrawColorMainBlue`.
    pub main: [i64; 3],
}

impl ClipDrawColor {
    /// On, in `color`.
    #[must_use]
    pub fn rgb(color: [u8; 3]) -> Self {
        Self {
            enable: 1,
            main: color.map(|channel| i64::from(channel) * 0x0101_0101),
        }
    }

    /// Whether it is on.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enable != 0
    }

    /// The color, 8 bits a channel.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn color(&self) -> [u8; 3] {
        self.main.map(|channel| (channel >> 24) as u8)
    }
}

/// A layer mask: its pixels and where they lie.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipMask {
    /// `LayerMaskOffsetX`, `LayerMaskOffsetY`, from the layer's offset.
    pub offset: [i64; 2],
    /// `LayerMaskOffscrOffsetX`, `LayerMaskOffscrOffsetY`.
    pub offscreen_offset: [i64; 2],
    /// The mask, one channel: 255 shows, 0 hides.
    pub bitmap: ClipBitmap,
}

/// A stored bitmap: its rows in the database and its blocks.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipBitmap {
    /// `MainId`s of its `Mipmap`, `MipmapInfo` and `Offscreen` rows
    /// (0: the writer gives one).
    pub ids: [i64; 3],
    /// The other columns of those three rows.
    pub rows: [Columns; 3],
    /// The id of its `Exta` chunk (empty: the writer gives one).
    pub external: String,
    /// The `Attribute` blob.
    pub attribute: ClipAttribute,
    /// Its blocks, `None` when the file has no chunk for it (folders).
    pub blocks: Option<ClipBlocks>,
}

/// An offscreen's `Attribute` blob.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipAttribute {
    /// Laid out in named parts (`Parameter`, `InitColor`, `BlockSize`).
    Parts(Vec<AttributePart>),
    /// Laid out some other way, kept as it was.
    Raw(Vec<u8>),
}

/// The payload of a stored bitmap's chunk.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClipBlocks {
    /// The blocks, in file order.
    pub blocks: Vec<ClipBlock>,
    /// The lists after them (`BlockStatus`, `BlockCheckSum`).
    pub lists: Vec<BlockList>,
    /// Bytes after the lists, kept as they were.
    pub tail: Vec<u8>,
}

/// One block of a stored bitmap: a 256 by 256 tile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipBlock {
    /// Its number in the grid, row by row.
    pub index: u32,
    /// The bytes it inflates to.
    pub raw_size: u32,
    /// Its width and height (256 and 256).
    pub size: [u32; 2],
    /// Its zlib data, `None` for a tile with nothing in it.
    pub data: Option<Vec<u8>>,
}

/// A stored bitmap unpacked: straight RGBA8, or one byte per pixel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipPixels {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// 4 for color with alpha, 1 for one channel (a mask).
    pub channels: u8,
    /// `width * height * channels` bytes.
    pub data: Vec<u8>,
}

impl ClipBitmap {
    /// The named part `name` of the attribute.
    #[must_use]
    pub fn part(&self, name: &str) -> Option<&AttributePart> {
        match &self.attribute {
            ClipAttribute::Parts(parts) => parts.iter().find(|part| part.name == name),
            ClipAttribute::Raw(_) => None,
        }
    }

    /// Width and height in pixels, as the attribute gives them.
    #[must_use]
    pub fn size(&self) -> Option<[u32; 2]> {
        let values = &self.part("Parameter")?.values;
        Some([*values.first()?, *values.get(1)?])
    }

    /// 4 for color with alpha, 1 for one channel.
    #[must_use]
    pub fn channels(&self) -> Option<u8> {
        let values = &self.part("Parameter")?.values;
        let color = (values.get(5), values.get(6)) == (Some(&1), Some(&4));
        Some(if color { 4 } else { 1 })
    }

    /// Whether a pixel no block covers is white (or, for color, opaque
    /// white); otherwise it is clear (or black for one channel).
    #[must_use]
    pub fn white(&self) -> bool {
        self.part("InitColor")
            .and_then(|part| part.values.get(1))
            .is_some_and(|value| *value != 0)
    }
}
