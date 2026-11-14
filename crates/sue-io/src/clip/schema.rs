//! What the database of a `.clip` is called: its tables, their columns,
//! and the numbers Clip Studio gives the blend modes.

use crate::sqlite::{NewTable, Value};

/// Clip Studio's layer blend modes by number, as Photoshop keys.
pub(super) const BLENDS: [(i64, &[u8; 4]); 31] = [
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

pub(super) fn blend_of(composite: i64) -> [u8; 4] {
    BLENDS
        .iter()
        .find(|(number, _)| *number == composite)
        .map_or(*b"norm", |(_, key)| **key)
}

pub(super) fn composite_of(key: [u8; 4]) -> i64 {
    // The first number listed for a key is the one Clip Studio uses.
    BLENDS
        .iter()
        .find(|(number, k)| **k == key && *number >= 0)
        .map_or(0, |(number, _)| *number)
}

pub(super) const LAYER_COLUMNS: &str = "MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, LayerName TEXT DEFAULT NULL, LayerType INTEGER DEFAULT NULL, LayerLock INTEGER DEFAULT NULL, LayerClip INTEGER DEFAULT NULL, LayerMasking INTEGER DEFAULT NULL, LayerOffsetX INTEGER DEFAULT NULL, LayerOffsetY INTEGER DEFAULT NULL, LayerRenderOffscrOffsetX INTEGER DEFAULT NULL, LayerRenderOffscrOffsetY INTEGER DEFAULT NULL, LayerMaskOffsetX INTEGER DEFAULT NULL, LayerMaskOffsetY INTEGER DEFAULT NULL, LayerMaskOffscrOffsetX INTEGER DEFAULT NULL, LayerMaskOffscrOffsetY INTEGER DEFAULT NULL, LayerOpacity INTEGER DEFAULT NULL, LayerComposite INTEGER DEFAULT NULL, LayerUsePaletteColor INTEGER DEFAULT NULL, LayerNoticeablePaletteColor INTEGER DEFAULT NULL, LayerPaletteRed INTEGER DEFAULT NULL, LayerPaletteGreen INTEGER DEFAULT NULL, LayerPaletteBlue INTEGER DEFAULT NULL, LayerFolder INTEGER DEFAULT NULL, LayerVisibility INTEGER DEFAULT NULL, LayerSelect INTEGER DEFAULT NULL, LayerNextIndex INTEGER DEFAULT NULL, LayerFirstChildIndex INTEGER DEFAULT NULL, LayerUuid TEXT DEFAULT NULL, LayerRenderMipmap INTEGER DEFAULT NULL, LayerLayerMaskMipmap INTEGER DEFAULT NULL, LayerRenderThumbnail INTEGER DEFAULT NULL, LayerLayerMaskThumbnail INTEGER DEFAULT NULL";

pub(super) fn table(name: &str, columns: &str, rows: Vec<(i64, Vec<Value>)>) -> NewTable {
    NewTable {
        name: name.to_owned(),
        sql: format!("CREATE TABLE {name}(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, {columns})"),
        rows,
    }
}
