//! Fill, gradient, text and vector layers, gray layers, palette color
//! and the layer color's sub color through a whole file: written in Clip
//! Studio's own storage and read back from it, with Sue's own tables
//! present (a file Sue wrote), gone (the same file after Clip Studio saved
//! it, which drops tables it does not know) and saying something else (a
//! layer changed in Clip Studio after Sue wrote it).

use super::kinds::{
    ClipAlign, ClipContent, ClipEffects, ClipFill, ClipGradient, ClipGradientRepeat,
    ClipGradientShape, ClipLine, ClipPoint, ClipStop, ClipText, ClipTextRun, ClipVector,
};
use super::model::{ClipBitmap, ClipDocument, ClipDrawColor, ClipLayer, ClipRow, ClipTable};
use super::native::{self, DRAWN, GENERATED};
use super::testkit::as_clip_studio_stores;
use super::{read, write};
use crate::sqlite::{Database, Value};

fn clear(width: u32, height: u32) -> ClipBitmap {
    ClipBitmap::color(width, height, &[])
}

fn text(words: &str) -> ClipContent {
    ClipContent::Text(ClipText {
        runs: vec![ClipTextRun {
            text: words.to_owned(),
            font: "Tahoma".to_owned(),
            size: 24.0,
            color: [20, 30, 40, 255],
            bold: false,
            italic: false,
        }],
        at: [12.0, 30.0],
        frame: None,
        vertical: false,
        align: ClipAlign::Start,
        line_spacing: 1.0,
    })
}

fn line(from: [f32; 2], to: [f32; 2], color: [u8; 4]) -> ClipLine {
    ClipLine {
        points: (0..5)
            .map(|i| {
                let t = i as f32 / 4.0;
                ClipPoint {
                    at: [
                        from[0] + (to[0] - from[0]) * t,
                        from[1] + (to[1] - from[1]) * t,
                    ],
                    pressure: 1.0,
                    width: [0.2, 0.8, 1.0, 0.8, 0.2][i],
                    opacity: 1.0,
                    corner: false,
                }
            })
            .collect(),
        width: 4.0,
        color,
        brush: String::new(),
        closed: false,
    }
}

fn gradient() -> ClipContent {
    ClipContent::Gradient(ClipGradient {
        shape: ClipGradientShape::Linear,
        start: [0.0, 0.0],
        end: [0.0, 90.0],
        ratio: 1.0,
        repeat: ClipGradientRepeat::Clamp,
        stops: vec![
            ClipStop {
                at: 0.0,
                color: [255, 0, 0, 255],
            },
            ClipStop {
                at: 0.5,
                color: [0, 255, 0, 128],
            },
            ClipStop {
                at: 1.0,
                color: [0, 0, 255, 255],
            },
        ],
    })
}

/// A picture of each kind Clip Studio's own storage holds.
fn picture() -> ClipDocument {
    let (w, h) = (120, 90);
    let mut doc = ClipDocument::new(w, h, 144.0);
    let mut fill = ClipLayer::generated(
        "Fill",
        ClipContent::Fill(ClipFill {
            color: [250, 40, 90, 255],
        }),
        0,
        0,
        clear(w, h),
    );
    fill.palette_color = Some([10, 200, 30]);
    let mut words = ClipLayer::generated("Words", text("Hello there"), 0, 0, clear(w, h));
    words.draw_color = ClipDrawColor::rgb([0, 0, 255]);
    words.effects = Some(ClipEffects {
        sub_color: Some([255, 255, 0]),
        ..ClipEffects::default()
    });
    doc.root.children = vec![
        fill,
        ClipLayer::generated("Gradient", gradient(), 0, 0, clear(w, h)),
        words,
        ClipLayer::generated(
            "Lines",
            ClipContent::Vector(ClipVector {
                lines: vec![
                    line([10.0, 10.0], [100.0, 80.0], [0, 0, 0, 255]),
                    line([100.0, 10.0], [10.0, 80.0], [200, 10, 10, 200]),
                ],
            }),
            0,
            0,
            clear(w, h),
        ),
    ];
    doc
}

/// Each layer's name and content.
fn contents(doc: &ClipDocument) -> Vec<(String, Option<ClipContent>)> {
    doc.all_layers()
        .into_iter()
        .map(|layer| (layer.name.clone(), layer.content.clone()))
        .collect()
}

/// Each layer's content as Clip Studio's storage gives it back.
fn as_read(doc: &ClipDocument) -> Vec<(String, Option<ClipContent>)> {
    doc.all_layers()
        .into_iter()
        .map(|layer| {
            let content = layer
                .content
                .as_ref()
                .and_then(|content| native::as_clip_studio_reads(content, doc.dpi));
            (layer.name.clone(), content)
        })
        .collect()
}

/// The same file as Clip Studio would save it, as far as Sue's own
/// tables go: they are gone. Every layer keeps only what Clip Studio's
/// own columns hold.
fn without_sue_tables(bytes: &[u8]) -> Vec<u8> {
    let mut doc = read(bytes).unwrap();
    fn strip(layers: &mut [ClipLayer]) {
        for layer in layers {
            layer.content = None;
            layer.effects = None;
            layer.palette_color = None;
            strip(&mut layer.children);
        }
    }
    strip(&mut doc.root.children);
    write(&doc).unwrap()
}

/// The table `name` of a written file's database: its columns and rows.
fn table(bytes: &[u8], name: &str) -> Option<(Vec<String>, Vec<Vec<Value>>)> {
    let layout = super::container::layout(bytes).unwrap();
    let chunk = layout.chunks.iter().find(|chunk| &chunk.kind == b"SQLi")?;
    let database = Database::open(chunk.data).unwrap();
    let table = database.table(name).unwrap()?;
    Some((
        table.columns.clone(),
        table.rows.iter().map(|(_, row)| row.clone()).collect(),
    ))
}

fn value<'a>(columns: &[String], row: &'a [Value], name: &str) -> Option<&'a Value> {
    row.get(columns.iter().position(|column| column == name)?)
}

#[test]
fn each_kind_is_written_in_clip_studios_own_columns() {
    let doc = picture();
    let bytes = write(&doc).unwrap();
    let (columns, rows) = table(&bytes, "Layer").unwrap();
    let row = |name: &str| {
        rows.iter()
            .find(|row| value(&columns, row, "LayerName").and_then(Value::text) == Some(name))
            .unwrap()
    };
    let int = |name: &str, column: &str| value(&columns, row(name), column).and_then(Value::int);
    let blob = |name: &str, column: &str| {
        value(&columns, row(name), column)
            .and_then(Value::bytes)
            .map(<[u8]>::to_vec)
    };
    // A fill and a gradient: Clip Studio's layer type, its blob, a mask
    // that is on.
    for name in ["Fill", "Gradient"] {
        assert_eq!(int(name, "LayerType"), Some(GENERATED), "{name}");
        assert_eq!(int(name, "LayerVisibility"), Some(3), "{name}");
        assert_eq!(int(name, "LayerMasking"), Some(1), "{name}");
        assert!(
            int(name, "LayerLayerMaskMipmap").unwrap_or(0) != 0,
            "{name}"
        );
        assert_eq!(int(name, "DrawToRenderOffscreenType"), Some(60), "{name}");
    }
    assert_eq!(
        blob("Fill", "GradationFillInfo"),
        native::gradation_info(&doc.root.children[0].content.clone().unwrap())
    );
    assert_eq!(int("Gradient", "GradationFillColorMixingInfo"), Some(1));
    // The palette color, each byte four times over.
    assert_eq!(int("Fill", "LayerUsePaletteColor"), Some(1));
    assert_eq!(int("Fill", "LayerPaletteGreen"), Some(200 * 0x0101_0101));
    // A text: its words and its records.
    assert_eq!(int("Words", "LayerType"), Some(DRAWN));
    assert_eq!(
        blob("Words", "TextLayerString").as_deref(),
        Some(b"Hello there".as_slice())
    );
    assert!(blob("Words", "TextLayerAttributes").is_some());
    assert!(blob("Words", "TextLayerAddAttributesV01").is_some());
    assert_eq!(int("Words", "DrawColorSubBlue"), Some(0));
    assert_eq!(int("Words", "DrawColorSubRed"), Some(255 * 0x0101_0101));
    // A vector layer: a row of VectorObjectList, its strokes in a chunk,
    // the pen in the brush tables, and the canvas pointing at them.
    assert_eq!(int("Lines", "LayerType"), Some(DRAWN));
    let stroke = int("Lines", "VectorNormalStrokeIndex").unwrap();
    let (list_columns, list) = table(&bytes, "VectorObjectList").unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(
        value(&list_columns, &list[0], "MainId").and_then(Value::int),
        Some(stroke)
    );
    assert_eq!(
        value(&list_columns, &list[0], "LayerId").and_then(Value::int),
        int("Lines", "MainId")
    );
    let (_, styles) = table(&bytes, "BrushStyle").unwrap();
    let (_, graphs) = table(&bytes, "BrushEffectorGraphData").unwrap();
    let (_, managers) = table(&bytes, "BrushStyleManager").unwrap();
    assert_eq!((styles.len(), graphs.len(), managers.len()), (1, 2, 1));
    let (canvas_columns, canvas) = table(&bytes, "Canvas").unwrap();
    assert_eq!(
        value(&canvas_columns, &canvas[0], "BrushStyleManager").and_then(Value::int),
        Some(1)
    );
    let (_, names) = table(&bytes, "ExternalTableAndColumnName").unwrap();
    assert!(names.contains(&vec![
        Value::Text("VectorObjectList".to_owned()),
        Value::Text("VectorData".to_owned())
    ]));
    // Sue's own parts are there too: Sue gets back what Clip Studio has no
    // place for.
    assert!(table(&bytes, "SueLayerPart").is_some());
}

#[test]
fn a_file_sue_wrote_reads_back_with_sue_parts_and_the_same_bytes_again() {
    let doc = picture();
    let bytes = write(&doc).unwrap();
    let back = read(&bytes).unwrap();
    assert_eq!(contents(&back), contents(&doc));
    assert_eq!(back.root.children[0].palette_color, Some([10, 200, 30]));
    assert_eq!(
        back.root.children[2]
            .effects
            .as_ref()
            .and_then(|effects| effects.sub_color),
        Some([255, 255, 0])
    );
    let expected = as_clip_studio_stores(&doc);
    for (want, got) in expected.all_layers().iter().zip(back.all_layers()) {
        assert_eq!(
            (want.layer_type, want.visibility, want.mask.is_some()),
            (got.layer_type, got.visibility, got.mask.is_some()),
            "{}",
            want.name
        );
    }
    let again = write(&back).unwrap();
    assert!(again == bytes, "writing what was read gives the same bytes");
}

#[test]
fn a_file_clip_studio_saved_reads_back_from_clip_studios_own_columns() {
    let doc = picture();
    let saved = without_sue_tables(&write(&doc).unwrap());
    assert!(table(&saved, "SueLayerPart").is_none());
    let back = read(&saved).unwrap();
    // What Clip Studio's storage holds comes back: the words, font, size,
    // color and place of the text; the lines' points, widths and colors;
    // the fill and the gradient whole.
    assert_eq!(contents(&back), as_read(&doc));
    let ClipContent::Vector(lines) = back.root.children[3].content.clone().unwrap() else {
        panic!("the vector layer reads as lines")
    };
    assert_eq!(lines.lines.len(), 2);
    assert_eq!(lines.lines[1].color, [200, 10, 10, 200]);
    assert_eq!(lines.lines[0].points[2].width, 1.0);
    assert_eq!(back.root.children[0].palette_color, Some([10, 200, 30]));
    assert_eq!(
        back.root.children[2]
            .effects
            .as_ref()
            .and_then(|effects| effects.sub_color),
        Some([255, 255, 0])
    );
    // Written again unchanged, nothing is added or lost.
    let again = write(&back).unwrap();
    assert_eq!(contents(&read(&again).unwrap()), contents(&back));
    let (_, list) = table(&again, "VectorObjectList").unwrap();
    assert_eq!(list.len(), 1);
}

#[test]
fn a_layer_changed_in_clip_studio_wins_over_sues_older_part() {
    let doc = picture();
    let bytes = write(&doc).unwrap();
    let mut changed = read(&bytes).unwrap();
    // As if the words were retyped in Clip Studio: its columns say one
    // thing, Sue's part (kept as a row of its own) another.
    let words = &mut changed.root.children[2];
    let part = super::parts::encode_content(words.content.as_ref().unwrap());
    let id = words.id;
    words.content = None;
    let ClipContent::Text(retyped) = text("Goodbye") else {
        unreachable!()
    };
    let blobs = native::text_blobs(&retyped, changed.dpi);
    for (name, blob) in [
        ("TextLayerString", blobs.string),
        ("TextLayerAttributes", blobs.attributes),
    ] {
        let slot = words
            .columns
            .iter_mut()
            .find(|(column, _)| column == name)
            .unwrap();
        slot.1 = Value::Blob(blob);
    }
    changed.foreign.rows.push(ClipRow {
        table: "SueLayerPart".to_owned(),
        columns: vec![
            ("LayerId".to_owned(), Value::Integer(id)),
            ("Kind".to_owned(), Value::Text("text".to_owned())),
            ("Version".to_owned(), Value::Integer(1)),
            ("Data".to_owned(), Value::Blob(part)),
        ],
    });
    let back = read(&write(&changed).unwrap()).unwrap();
    let Some(ClipContent::Text(read_text)) = &back.root.children[2].content else {
        panic!("the text layer reads as text")
    };
    assert_eq!(read_text.plain(), "Goodbye");
    // The other layers keep Sue's parts.
    assert_eq!(back.root.children[3].content, doc.root.children[3].content);
}

#[test]
fn a_changed_vector_layer_replaces_its_strokes_and_keeps_one_pen() {
    let doc = picture();
    let saved = without_sue_tables(&write(&doc).unwrap());
    let mut opened = read(&saved).unwrap();
    // Sue changes the lines: the layer is written from Sue's model again,
    // with no kept columns.
    let lines = &mut opened.root.children[3];
    lines.columns.clear();
    lines.id = 0;
    lines.content = Some(ClipContent::Vector(ClipVector {
        lines: vec![line([5.0, 5.0], [50.0, 5.0], [0, 0, 255, 255])],
    }));
    let bytes = write(&opened).unwrap();
    let (list_columns, list) = table(&bytes, "VectorObjectList").unwrap();
    assert_eq!(list.len(), 1, "the old strokes went");
    let chunk = String::from_utf8(
        value(&list_columns, &list[0], "VectorData")
            .and_then(Value::bytes)
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    // Only the new strokes' chunk is in the file, and it is listed.
    let layout = super::container::layout(&bytes).unwrap();
    let extas: Vec<String> = layout
        .chunks
        .iter()
        .filter(|chunk| &chunk.kind == b"Exta")
        .filter_map(|chunk| chunk.external().ok().map(|(id, _)| id))
        .collect();
    assert!(extas.contains(&chunk));
    let (_, external) = table(&bytes, "ExternalChunk").unwrap();
    assert_eq!(external.len(), extas.len());
    // The pen already in the file is used again, not added twice.
    let (_, styles) = table(&bytes, "BrushStyle").unwrap();
    let (_, graphs) = table(&bytes, "BrushEffectorGraphData").unwrap();
    assert_eq!((styles.len(), graphs.len()), (1, 2));
    let back = read(&bytes).unwrap();
    let Some(ClipContent::Vector(read_lines)) = &back.root.children[3].content else {
        panic!("the vector layer reads as lines")
    };
    assert_eq!(read_lines.lines.len(), 1);
    assert_eq!(read_lines.lines[0].color, [0, 0, 255, 255]);
}

#[test]
fn a_kept_brush_list_gets_sues_pen_added_after_it() {
    let mut doc = picture();
    // A file whose brush tables hold another brush: Sue's pen goes after
    // it, linked into both lists.
    let int = Value::Integer;
    doc.foreign.tables = vec![
        ClipTable {
            name: "BrushStyleManager".to_owned(),
            sql: native::BRUSH_MANAGER_SQL.to_owned(),
            columns: super::schema::sql_columns(native::BRUSH_MANAGER_SQL),
            rows: vec![(
                1,
                vec![
                    int(1),
                    int(1),
                    int(1),
                    int(4),
                    int(0),
                    int(0),
                    int(0),
                    int(7),
                    int(0),
                ],
            )],
        },
        ClipTable {
            name: "BrushEffectorGraphData".to_owned(),
            sql: native::BRUSH_GRAPH_SQL.to_owned(),
            columns: super::schema::sql_columns(native::BRUSH_GRAPH_SQL),
            rows: vec![(
                1,
                vec![int(1), int(4), int(1), int(0), int(1), int(16), Value::Blob(vec![0; 16])],
            )],
        },
        ClipTable {
            name: "BrushStyle".to_owned(),
            sql: "CREATE TABLE BrushStyle(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, NextIndex INTEGER DEFAULT NULL, StyleFlag INTEGER DEFAULT NULL)".to_owned(),
            columns: vec![
                "_PW_ID".to_owned(),
                "MainId".to_owned(),
                "CanvasId".to_owned(),
                "NextIndex".to_owned(),
                "StyleFlag".to_owned(),
            ],
            rows: vec![(1, vec![int(1), int(7), int(1), int(0), int(3)])],
        },
    ];
    let bytes = write(&doc).unwrap();
    let (columns, graphs) = table(&bytes, "BrushEffectorGraphData").unwrap();
    let main = |row: &[Value]| value(&columns, row, "MainId").and_then(Value::int);
    let next = |row: &[Value]| value(&columns, row, "NextIndex").and_then(Value::int);
    let found: Vec<(Option<i64>, Option<i64>)> =
        graphs.iter().map(|row| (main(row), next(row))).collect();
    assert_eq!(
        found,
        [(Some(4), Some(5)), (Some(5), Some(6)), (Some(6), Some(0))]
    );
    let (style_columns, styles) = table(&bytes, "BrushStyle").unwrap();
    let style = |row: &[Value], name: &str| value(&style_columns, row, name).and_then(Value::int);
    assert_eq!(styles.len(), 2);
    assert_eq!(
        (style(&styles[0], "MainId"), style(&styles[0], "NextIndex")),
        (Some(7), Some(8))
    );
    assert_eq!(style(&styles[1], "MainId"), Some(8));
    // The new pen's effectors point at its own curves.
    let size = value(&style_columns, &styles[1], "SizeEffector")
        .and_then(Value::bytes)
        .unwrap();
    assert_eq!(&size[8..12], &5_u32.to_be_bytes());
    assert_eq!(&size[20..24], &6_u32.to_be_bytes());
    // And the strokes are drawn with it.
    let (list_columns, list) = table(&bytes, "VectorObjectList").unwrap();
    let id = String::from_utf8(
        value(&list_columns, &list[0], "VectorData")
            .and_then(Value::bytes)
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    let layout = super::container::layout(&bytes).unwrap();
    let strokes = layout
        .chunks
        .iter()
        .find_map(|chunk| {
            let (found, payload) = chunk.external().ok()?;
            (found == id).then_some(payload)
        })
        .unwrap();
    assert_eq!(&strokes[76..80], &8_u32.to_be_bytes());
}

#[test]
fn clip_studios_id_counts_are_raised_past_what_sue_wrote() {
    let mut doc = picture();
    doc.foreign.tables.push(ClipTable {
        name: "ElemScheme".to_owned(),
        sql: "CREATE TABLE ElemScheme(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, TableName TEXT, ElemType INTEGER, MaxIndex INTEGER)".to_owned(),
        columns: vec![
            "_PW_ID".to_owned(),
            "TableName".to_owned(),
            "ElemType".to_owned(),
            "MaxIndex".to_owned(),
        ],
        rows: vec![
            (1, vec![Value::Integer(1), Value::Text("Layer".to_owned()), Value::Integer(4), Value::Integer(1)]),
            (2, vec![Value::Integer(2), Value::Text("VectorObjectList".to_owned()), Value::Integer(0), Value::Integer(0)]),
            (3, vec![Value::Integer(3), Value::Text("Track".to_owned()), Value::Integer(4096), Value::Integer(9)]),
        ],
    });
    let bytes = write(&doc).unwrap();
    let (columns, rows) = table(&bytes, "ElemScheme").unwrap();
    let max = |name: &str| {
        rows.iter()
            .find(|row| value(&columns, row, "TableName").and_then(Value::text) == Some(name))
            .and_then(|row| value(&columns, row, "MaxIndex").and_then(Value::int))
    };
    let (layer_columns, layers) = table(&bytes, "Layer").unwrap();
    let top = layers
        .iter()
        .filter_map(|row| value(&layer_columns, row, "MainId").and_then(Value::int))
        .max();
    assert_eq!(max("Layer"), top);
    assert_eq!(max("VectorObjectList"), Some(1));
    assert_eq!(
        max("Track"),
        Some(9),
        "a table Sue did not write keeps its count"
    );
}

#[test]
fn what_clip_studio_has_no_place_for_stays_in_sues_tables_only() {
    let mut doc = picture();
    let mut circle = gradient();
    if let ClipContent::Gradient(gradient) = &mut circle {
        gradient.shape = ClipGradientShape::Circle;
    }
    doc.root.children[1].content = Some(circle.clone());
    let bytes = write(&doc).unwrap();
    let (columns, rows) = table(&bytes, "Layer").unwrap();
    let row = rows
        .iter()
        .find(|row| value(&columns, row, "LayerName").and_then(Value::text) == Some("Gradient"))
        .unwrap();
    assert_eq!(
        value(&columns, row, "LayerType").and_then(Value::int),
        Some(1)
    );
    assert!(value(&columns, row, "GradationFillInfo").is_none_or(|value| *value == Value::Null));
    // Sue still reads it whole from its own part.
    assert_eq!(read(&bytes).unwrap().root.children[1].content, Some(circle));
}

/// The survey's own files, when `SUE_CLIP_SURVEY` names the folder they
/// are in (they are Clip Studio's, made on the maintainer's machine, and
/// never enter the repository): every layer kind reads as what it holds,
/// every block checksum is the one Sue computes, and writing what was
/// read keeps every layer.
#[test]
fn the_survey_files_read_as_what_they_hold() {
    let Ok(folder) = std::env::var("SUE_CLIP_SURVEY") else {
        return;
    };
    let path = std::path::Path::new(&folder).join("05-gradient.clip");
    let bytes = std::fs::read(path).unwrap();
    let layout = super::container::layout(&bytes).unwrap();
    let mut checked = 0;
    for chunk in layout.chunks.iter().filter(|chunk| &chunk.kind == b"Exta") {
        let Ok((_, payload)) = chunk.external() else {
            continue;
        };
        let blocks = super::offscreen::parse_payload(payload);
        let Some(sums) = blocks
            .lists
            .iter()
            .find(|list| list.name == "BlockCheckSum")
        else {
            continue;
        };
        for (block, sum) in blocks.blocks.iter().zip(&sums.values) {
            let want = block.data.as_deref().map_or(0, super::offscreen::checksum);
            assert_eq!(*sum, want, "block {}", block.index);
            checked += 1;
        }
    }
    assert!(checked > 0);
    let doc = read(&bytes).unwrap();
    let find = |name: &str| {
        doc.all_layers()
            .into_iter()
            .find(|layer| layer.name == name)
            .unwrap()
            .clone()
    };
    let gray = find("Layer 2");
    assert!(gray.pixels.as_ref().unwrap().is_gray());
    let pixels = gray.pixels.as_ref().unwrap().decode().unwrap();
    assert_eq!(pixels.channels, 4);
    // The pen line is gray 55 wherever it shows, its edges thinner.
    let shown: Vec<&[u8]> = pixels.data.chunks(4).filter(|pixel| pixel[3] > 0).collect();
    assert!(shown.len() > 100);
    assert!(shown.iter().all(|pixel| pixel[..3] == [55, 55, 55]));
    let Some(ClipContent::Vector(lines)) = find("Layer 3").content else {
        panic!("the vector layer reads as lines")
    };
    assert_eq!(lines.lines.len(), 1);
    assert_eq!(lines.lines[0].points.len(), 23);
    assert_eq!(
        (lines.lines[0].width, lines.lines[0].color),
        (1.5, [255, 0x32, 0x64, 255])
    );
    let Some(ClipContent::Text(words)) = find("Hello").content else {
        panic!("the text layer reads as text")
    };
    assert_eq!(
        (
            words.plain(),
            words.runs[0].font.as_str(),
            words.runs[0].size,
            words.at
        ),
        ("Hello".to_owned(), "Tahoma", 30.0, [1057.0, 354.0])
    );
    assert_eq!(
        find("Fill 1").content,
        Some(ClipContent::Fill(ClipFill {
            color: [255, 0x32, 0x64, 255]
        }))
    );
    let Some(ClipContent::Gradient(gradient)) = find("Gradient 1").content else {
        panic!("the gradient layer reads as a gradient")
    };
    assert_eq!(
        (gradient.start, gradient.end),
        ([800.0, 0.0], [800.0, 1200.0])
    );
    // Written again unchanged, every layer keeps what it was, and the
    // file's own vector table and brush stay as they were.
    let again = write(&doc).unwrap();
    let back = read(&again).unwrap();
    assert_eq!(contents(&back), contents(&doc));
    assert_eq!(table(&again, "BrushStyle").unwrap().1.len(), 1);
    assert_eq!(table(&again, "VectorObjectList").unwrap().1.len(), 1);
    // Its vector layer redrawn in Sue: the new strokes are drawn with the
    // file's own pen, found again, not a second one.
    let mut redrawn = doc.clone();
    let paths: Vec<_> = redrawn.paths().into_iter().map(|(path, _)| path).collect();
    for path in paths {
        let layer = redrawn.layer_at_mut(&path).unwrap();
        if layer.name == "Layer 3" {
            layer.columns.clear();
            layer.id = 0;
            layer.content = Some(ClipContent::Vector(ClipVector {
                lines: vec![line([200.0, 900.0], [1400.0, 900.0], [0, 0, 255, 255])],
            }));
        }
    }
    let bytes = write(&redrawn).unwrap();
    assert_eq!(table(&bytes, "BrushStyle").unwrap().1.len(), 1);
    assert_eq!(table(&bytes, "BrushEffectorGraphData").unwrap().1.len(), 2);
    assert_eq!(table(&bytes, "VectorObjectList").unwrap().1.len(), 1);
}

#[test]
fn a_kept_pen_stored_with_whole_numbers_is_found_again() {
    // SQLite keeps a real with no fraction as a whole number: a pen Clip
    // Studio wrote reads back with 90 where Sue writes 90.0.
    let mut doc = picture();
    let int = Value::Integer;
    let mut style = vec![
        ("MainId", int(1)),
        ("CanvasId", int(1)),
        ("NextIndex", int(0)),
    ];
    style.extend(native::pen_style([1, 2]).into_iter().map(|(name, value)| {
        let value = match value {
            Value::Real(real) if real.fract() == 0.0 => int(real as i64),
            other => other,
        };
        (name, value)
    }));
    let style_columns: Vec<String> = std::iter::once("_PW_ID".to_owned())
        .chain(style.iter().map(|(name, _)| (*name).to_owned()))
        .collect();
    let graphs = native::pen_graphs();
    doc.foreign.tables = vec![
        ClipTable {
            name: "BrushStyleManager".to_owned(),
            sql: native::BRUSH_MANAGER_SQL.to_owned(),
            columns: super::schema::sql_columns(native::BRUSH_MANAGER_SQL),
            rows: vec![(
                1,
                vec![
                    int(1),
                    int(1),
                    int(1),
                    int(1),
                    int(0),
                    int(0),
                    int(0),
                    int(1),
                    int(0),
                ],
            )],
        },
        ClipTable {
            name: "BrushEffectorGraphData".to_owned(),
            sql: native::BRUSH_GRAPH_SQL.to_owned(),
            columns: super::schema::sql_columns(native::BRUSH_GRAPH_SQL),
            rows: (0..2)
                .map(|i| {
                    let (number, size, points) = graphs[i].clone();
                    (
                        i as i64 + 1,
                        vec![
                            int(i as i64 + 1),
                            int(i as i64 + 1),
                            int(1),
                            int(if i == 0 { 2 } else { 0 }),
                            int(number),
                            int(size),
                            Value::Blob(points),
                        ],
                    )
                })
                .collect(),
        },
        ClipTable {
            name: "BrushStyle".to_owned(),
            sql: native::BRUSH_STYLE_SQL.to_owned(),
            columns: style_columns,
            rows: vec![(
                1,
                std::iter::once(int(1))
                    .chain(style.into_iter().map(|(_, value)| value))
                    .collect(),
            )],
        },
    ];
    let bytes = write(&doc).unwrap();
    assert_eq!(table(&bytes, "BrushStyle").unwrap().1.len(), 1);
    assert_eq!(table(&bytes, "BrushEffectorGraphData").unwrap().1.len(), 2);
}

#[test]
fn a_kept_stroke_row_stays_while_its_layer_points_at_it() {
    // A layer whose kept columns point at a stroke row, with content of
    // another kind (as a newer Sue might write): the row and its chunk
    // stay, since only new strokes may take a row's place.
    let doc = picture();
    let saved = without_sue_tables(&write(&doc).unwrap());
    let mut opened = read(&saved).unwrap();
    opened.root.children[3].content = Some(text("Words"));
    let bytes = write(&opened).unwrap();
    assert_eq!(table(&bytes, "VectorObjectList").unwrap().1.len(), 1);
    let back = read(&bytes).unwrap();
    assert!(matches!(
        back.root.children[3].content,
        Some(ClipContent::Text(_))
    ));
}
