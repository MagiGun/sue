//! What the database of a `.clip` is called: its tables, their columns,
//! and the numbers Clip Studio gives the blend modes.

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

/// The columns Sue writes of `Layer`.
pub(super) const LAYER_COLUMNS: &str = "MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, LayerName TEXT DEFAULT NULL, LayerType INTEGER DEFAULT NULL, LayerLock INTEGER DEFAULT NULL, LayerClip INTEGER DEFAULT NULL, LayerMasking INTEGER DEFAULT NULL, LayerOffsetX INTEGER DEFAULT NULL, LayerOffsetY INTEGER DEFAULT NULL, LayerRenderOffscrOffsetX INTEGER DEFAULT NULL, LayerRenderOffscrOffsetY INTEGER DEFAULT NULL, LayerMaskOffsetX INTEGER DEFAULT NULL, LayerMaskOffsetY INTEGER DEFAULT NULL, LayerMaskOffscrOffsetX INTEGER DEFAULT NULL, LayerMaskOffscrOffsetY INTEGER DEFAULT NULL, LayerOpacity INTEGER DEFAULT NULL, LayerComposite INTEGER DEFAULT NULL, LayerUsePaletteColor INTEGER DEFAULT NULL, LayerNoticeablePaletteColor INTEGER DEFAULT NULL, LayerPaletteRed INTEGER DEFAULT NULL, LayerPaletteGreen INTEGER DEFAULT NULL, LayerPaletteBlue INTEGER DEFAULT NULL, LayerFolder INTEGER DEFAULT NULL, LayerVisibility INTEGER DEFAULT NULL, LayerSelect INTEGER DEFAULT NULL, LayerNextIndex INTEGER DEFAULT NULL, LayerFirstChildIndex INTEGER DEFAULT NULL, LayerUuid TEXT DEFAULT NULL, LayerRenderMipmap INTEGER DEFAULT NULL, LayerLayerMaskMipmap INTEGER DEFAULT NULL, LayerRenderThumbnail INTEGER DEFAULT NULL, LayerLayerMaskThumbnail INTEGER DEFAULT NULL";

/// The columns Sue writes of `Project`.
pub(super) const PROJECT_COLUMNS: &str = "ProjectInternalVersion TEXT DEFAULT NULL, ProjectName TEXT DEFAULT NULL, ProjectCanvas INTEGER DEFAULT NULL, ProjectItemBank INTEGER DEFAULT NULL, ProjectCutBank INTEGER DEFAULT NULL, ProjectLayerCompManager INTEGER DEFAULT NULL, ProjectRootCanvasNode INTEGER DEFAULT NULL";

/// The columns Sue writes of `CanvasItemBank`.
pub(super) const CANVAS_ITEM_BANK_COLUMNS: &str = "MainId INTEGER DEFAULT NULL, ModelBankMainIndex INTEGER DEFAULT NULL, BankRootItemMainIndex INTEGER DEFAULT NULL";

/// The columns Sue writes of `AnimationCutBank`.
pub(super) const ANIMATION_CUT_BANK_COLUMNS: &str = "MainId INTEGER DEFAULT NULL, FirstTimeLine INTEGER DEFAULT NULL, FirstScenario INTEGER DEFAULT NULL, CurrentIndex INTEGER DEFAULT NULL, Enable INTEGER DEFAULT NULL";

/// The columns Sue writes of `LayerCompManager`.
pub(super) const LAYER_COMP_MANAGER_COLUMNS: &str = "MainId INTEGER DEFAULT NULL, FirstLayerCompIndex INTEGER DEFAULT NULL, LastStateLayerCompIndex INTEGER DEFAULT NULL, AppliedLayerCompIndex INTEGER DEFAULT NULL";

/// The columns Sue writes of `Canvas`.
pub(super) const CANVAS_COLUMNS: &str = "MainId INTEGER DEFAULT NULL, CanvasUnit INTEGER DEFAULT NULL, CanvasWidth REAL DEFAULT NULL, CanvasHeight REAL DEFAULT NULL, CanvasResolution REAL DEFAULT NULL, CanvasChannelBytes INTEGER DEFAULT NULL, CanvasDefaultChannelOrder INTEGER DEFAULT NULL, CanvasRootFolder INTEGER DEFAULT NULL, CanvasCurrentLayer INTEGER DEFAULT NULL, CanvasDoSimulateColor INTEGER DEFAULT NULL, CanvasUseColorAdjustment INTEGER DEFAULT NULL, CanvasDefaultColorTypeIndex INTEGER DEFAULT NULL, CanvasDefaultColorBlackChecked INTEGER DEFAULT NULL, CanvasDefaultColorWhiteChecked INTEGER DEFAULT NULL, CanvasDefaultToneLine REAL DEFAULT NULL, CanvasDoublePage INTEGER DEFAULT NULL, CanvasRenderMipmapForceSaved INTEGER DEFAULT NULL, CanvasWorkTime INTEGER DEFAULT NULL";

/// The columns Sue writes of `Mipmap`.
pub(super) const MIPMAP_COLUMNS: &str = "MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, LayerId INTEGER DEFAULT NULL, MipmapCount INTEGER DEFAULT NULL, BaseMipmapInfo INTEGER DEFAULT NULL";

/// The columns Sue writes of `MipmapInfo`.
pub(super) const MIPMAP_INFO_COLUMNS: &str = "MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, LayerId INTEGER DEFAULT NULL, ThisScale REAL DEFAULT NULL, Offscreen INTEGER DEFAULT NULL, NextIndex INTEGER DEFAULT NULL";

/// The columns Sue writes of `Offscreen`.
pub(super) const OFFSCREEN_COLUMNS: &str = "MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, LayerId INTEGER DEFAULT NULL, Attribute BLOB DEFAULT NULL, BlockData BLOB DEFAULT NULL";

/// The columns Sue writes of `LayerThumbnail`.
pub(super) const LAYER_THUMBNAIL_COLUMNS: &str = "MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, LayerId INTEGER DEFAULT NULL, ThumbnailCanvasWidth INTEGER DEFAULT NULL, ThumbnailCanvasHeight INTEGER DEFAULT NULL, ThumbnailOffscreen INTEGER DEFAULT NULL, ThumbnailDrewMode INTEGER DEFAULT NULL, ThumbnailFixMode INTEGER DEFAULT NULL";

/// Sue's own table of the layer marks Clip Studio's own columns for are
/// not yet observed (survey 8.19): draft and reference. Clip Studio
/// ignores tables it does not know, as it does `SueProject`.
pub(super) const SUE_LAYER_SQL: &str =
    "CREATE TABLE SueLayer(LayerId INTEGER, Draft INTEGER, Reference INTEGER)";

/// Sue's own table of the parts of a layer Clip Studio's own storage for
/// is not yet observed: content, effects, palette color, rulers and
/// thumbnail (`docs/clip-sue-tables.md`).
pub(super) const SUE_LAYER_PART_SQL: &str =
    "CREATE TABLE SueLayerPart(LayerId INTEGER, Kind TEXT, Version INTEGER, Data BLOB)";

/// Sue's column list of `SueLayerPart`.
pub(super) const SUE_LAYER_PART_COLUMNS: &str =
    "LayerId INTEGER, Kind TEXT, Version INTEGER, Data BLOB";

/// Sue's own table of the parts of the document: layer comps, the
/// timeline and the canvas preview.
pub(super) const SUE_DOCUMENT_PART_SQL: &str =
    "CREATE TABLE SueDocumentPart(Kind TEXT, Version INTEGER, Data BLOB)";

/// Sue's column list of `SueDocumentPart`.
pub(super) const SUE_DOCUMENT_PART_COLUMNS: &str = "Kind TEXT, Version INTEGER, Data BLOB";

/// `SueProject`: the picture's `.sue` bytes in files Sue writes.
pub(super) const SUE_PROJECT_SQL: &str = "CREATE TABLE SueProject(Version INTEGER, Data BLOB)";

/// `ExternalChunk`: where each stored bitmap's chunk is.
pub(super) const EXTERNAL_CHUNK_SQL: &str =
    "CREATE TABLE ExternalChunk(ExternalID BLOB, Offset INTEGER)";

/// `ExternalTableAndColumnName`: which column names stored bitmaps.
pub(super) const EXTERNAL_NAMES_SQL: &str =
    "CREATE TABLE ExternalTableAndColumnName(TableName TEXT, ColumnName TEXT)";

/// The `CREATE TABLE` statement Sue writes for `name` with `columns`.
pub(super) fn table_sql(name: &str, columns: &str) -> String {
    format!("CREATE TABLE {name}(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, {columns})")
}

/// The column definitions inside the brackets of a `CREATE TABLE`, split
/// at the commas that are not inside brackets of their own.
fn definitions(sql: &str) -> Vec<&str> {
    let Some(open) = sql.find('(') else {
        return Vec::new();
    };
    let close = sql.rfind(')').unwrap_or(sql.len()).max(open + 1);
    let inner = &sql[open + 1..close];
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0_i32, 0);
    for (i, ch) in inner.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(inner[start..].trim());
    parts
}

/// Where among a table's columns the rowid stands (an `INTEGER PRIMARY
/// KEY` column, by the rule `sqlite::read` follows): its value is stored
/// as NULL and read back as the rowid.
pub(super) fn rowid_column(sql: &str) -> Option<usize> {
    let mut index = 0;
    for part in definitions(sql) {
        let upper = part.to_ascii_uppercase();
        if ["PRIMARY ", "UNIQUE", "CHECK", "FOREIGN ", "CONSTRAINT "]
            .iter()
            .any(|word| upper.starts_with(word))
            || part.is_empty()
        {
            continue;
        }
        let words: Vec<&str> = upper.split_whitespace().collect();
        if words.get(1) == Some(&"INTEGER") && upper.contains("PRIMARY KEY") {
            return Some(index);
        }
        index += 1;
    }
    None
}

/// The column names of a `CREATE TABLE` statement, in order.
pub(super) fn sql_columns(sql: &str) -> Vec<String> {
    definitions(sql)
        .into_iter()
        .filter(|part| {
            let upper = part.to_ascii_uppercase();
            !part.is_empty()
                && !["PRIMARY ", "UNIQUE", "CHECK", "FOREIGN ", "CONSTRAINT "]
                    .iter()
                    .any(|word| upper.starts_with(word))
        })
        .filter_map(|part| part.split_whitespace().next())
        .map(|name| name.trim_matches('"').to_owned())
        .collect()
}

/// The definition of `column` in Sue's column list `columns` (such as
/// `LayerName TEXT DEFAULT NULL`), if it is there.
pub(super) fn definition<'a>(columns: &'a str, column: &str) -> Option<&'a str> {
    columns.split(", ").find(|definition| {
        definition
            .split_whitespace()
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(column))
    })
}

/// The column names of Sue's column list `columns`.
pub(super) fn names(columns: &str) -> Vec<String> {
    columns
        .split(", ")
        .filter_map(|definition| definition.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

/// `name` as SQL spells a column name: bare when it is a plain word,
/// otherwise in double quotes.
pub(super) fn quoted(name: &str) -> String {
    let plain = name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && name.chars().next().is_some_and(|ch| !ch.is_ascii_digit());
    if plain {
        name.to_owned()
    } else {
        format!("\"{}\"", name.replace('"', "\"\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rowid_column_is_found_as_sqlite_reads_it() {
        assert_eq!(rowid_column(&table_sql("Layer", LAYER_COLUMNS)), Some(0));
        assert_eq!(rowid_column(SUE_LAYER_SQL), None);
        assert_eq!(
            rowid_column("CREATE TABLE t(a TEXT, PRIMARY KEY(a), b integer primary key)"),
            Some(1)
        );
        assert_eq!(rowid_column("no brackets"), None);
        assert_eq!(
            definition(LAYER_COLUMNS, "layername"),
            Some("LayerName TEXT DEFAULT NULL")
        );
        assert_eq!(names(MIPMAP_COLUMNS)[4], "BaseMipmapInfo");
        assert_eq!(quoted("Plain_1"), "Plain_1");
        assert_eq!(quoted("1st"), "\"1st\"");
        assert_eq!(quoted("say \"hi\""), "\"say \"\"hi\"\"\"");
    }
}
