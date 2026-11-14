//! Brushes as files: `.sut` sub tools in and out, and the brush library
//! as entities with undoable edits. Every `.sut` here is made by the test
//! code; none is a file Clip Studio Paint made.

use sue_core::{Arena, UndoStack};
use sue_io::brushes::library;
use sue_io::brushes::sut::{self, TipPicture};
use sue_io::sqlite::{self, NewTable, Value};
use sue_model::BrushPreset;
use sue_model::brush::{AntiAlias, Binding, PictureRef, TipSource, input, param};

/// `true` if both arenas hold the same entities under the same ids, and
/// the same relations.
fn same(a: &Arena, b: &Arena) -> bool {
    let mut left: Vec<String> = a.relations().iter().map(|r| format!("{r:?}")).collect();
    let mut right: Vec<String> = b.relations().iter().map(|r| format!("{r:?}")).collect();
    left.sort();
    right.sort();
    a.len() == b.len()
        && a.iter()
            .all(|(_, entity)| b.get_by_id(entity.id()).ok() == Some(entity))
        && left == right
}

/// A small gray picture as a PNG.
fn png(shade: u8, side: u32) -> Vec<u8> {
    let pixels: Vec<u8> = (0..side * side)
        .flat_map(|i| [0, 0, 0, if i % 3 == 0 { shade } else { 255 - shade }])
        .collect();
    sue_io::encode_png_rgba(side, side, &pixels).expect("a picture encodes")
}

/// A tar archive with one file, as a material holds its thumbnail.
fn material(png: &[u8]) -> Vec<u8> {
    let mut header = [0_u8; 512];
    let name = b"thumbnail/thumbnail.png";
    header[..name.len()].copy_from_slice(name);
    header[124..136].copy_from_slice(format!("{:011o}\0", png.len()).as_bytes());
    header[156] = b'0';
    let mut out = header.to_vec();
    out.extend(png);
    out.resize(out.len().div_ceil(512) * 512 + 1024, 0);
    out
}

/// A picture array naming `paths`, laid out as Clip Studio lays it.
fn array(paths: &[&str]) -> Vec<u8> {
    let mut out = 8_u32.to_be_bytes().to_vec();
    out.extend((paths.len() as u32).to_be_bytes());
    for path in paths {
        let text: Vec<u8> = path.encode_utf16().flat_map(u16::to_le_bytes).collect();
        out.extend(((8 + text.len() + 20) as u32).to_be_bytes());
        out.extend((text.len() as u32).to_be_bytes());
        out.extend(&text);
        out.extend([0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
    }
    out
}

/// A size effector: pressure on with a minimum of 20 and a bent curve,
/// random on with a minimum of 70.
fn effector() -> Vec<u8> {
    let mut words = [0_i32; 12];
    words[0] = 48;
    words[1] = 0x1f0;
    words[2] = 16 | 128;
    words[3] = 20;
    words[6] = 70;
    words[8] = 12 + 3 * 16;
    words[10] = 100;
    let mut out: Vec<u8> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
    for w in [12_i32, 3, 16] {
        out.extend(w.to_be_bytes());
    }
    for [x, y] in [[0.0_f64, 0.0], [0.5, 0.25], [1.0, 1.0]] {
        out.extend(x.to_be_bytes());
        out.extend(y.to_be_bytes());
    }
    out
}

/// A `.sut` of the sub tools `nodes` (name and variant columns), with
/// `materials` (path and PNG) and two columns no Sue knows.
fn sut_file(nodes: &[(&str, Vec<(&str, Value)>)], materials: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut columns: Vec<&str> = vec!["VariantID"];
    for (_, values) in nodes {
        for (name, _) in values {
            if !columns.contains(name) {
                columns.push(name);
            }
        }
    }
    columns.extend(["FutureSetting", "FutureBlob"]);
    let kind = |name: &str| match name {
        "BrushSize" | "BrushInterval" | "BrushRotation" | "TextureScale" | "TextureRotate"
        | "BrushInLength" | "BrushOutLength" => "REAL",
        name if name.ends_with("Effector") || name.ends_with("Array") || name == "FutureBlob" => {
            "BLOB"
        }
        _ => "INTEGER",
    };
    let variant_sql = format!(
        "CREATE TABLE Variant(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, {})",
        columns
            .iter()
            .map(|c| format!("{c} {} DEFAULT NULL", kind(c)))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut variants = Vec::new();
    let mut node_rows = Vec::new();
    for (index, (name, values)) in nodes.iter().enumerate() {
        let id = 100 + index as i64;
        let mut row = vec![Value::Null];
        for column in &columns {
            row.push(match *column {
                "VariantID" => Value::Integer(id),
                "FutureSetting" => Value::Integer(7),
                "FutureBlob" => Value::Blob(vec![1, 2, 3]),
                column => values
                    .iter()
                    .find(|(known, _)| *known == column)
                    .map_or(Value::Null, |(_, value)| value.clone()),
            });
        }
        variants.push((index as i64 + 1, row));
        node_rows.push((
            index as i64 + 1,
            vec![
                Value::Null,
                Value::Blob(vec![index as u8; 16]),
                Value::Text((*name).to_owned()),
                Value::Integer(id),
                Value::Integer(0),
            ],
        ));
    }
    let mut tables = vec![
        NewTable {
            name: "Manager".to_owned(),
            sql: "CREATE TABLE Manager(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, ToolType INTEGER DEFAULT NULL, Version INTEGER DEFAULT NULL)".to_owned(),
            rows: vec![(1, vec![Value::Null, Value::Integer(0), Value::Integer(138)])],
        },
        NewTable {
            name: "Node".to_owned(),
            sql: "CREATE TABLE Node(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, NodeUuid BLOB DEFAULT NULL, NodeName TEXT DEFAULT NULL, NodeVariantID INTEGER DEFAULT NULL, NodeFutureFlag INTEGER DEFAULT NULL)".to_owned(),
            rows: node_rows,
        },
        NewTable {
            name: "Variant".to_owned(),
            sql: variant_sql,
            rows: variants,
        },
    ];
    if !materials.is_empty() {
        tables.push(NewTable {
            name: "MaterialFile".to_owned(),
            sql: "CREATE TABLE MaterialFile(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, OriginalPath TEXT DEFAULT NULL, FileData BLOB DEFAULT NULL)".to_owned(),
            rows: materials
                .iter()
                .zip(1_i64..)
                .map(|((path, png), rowid)| {
                    (
                        rowid,
                        vec![
                            Value::Null,
                            Value::Text((*path).to_owned()),
                            Value::Blob(material(png)),
                        ],
                    )
                })
                .collect(),
        });
    }
    sqlite::write(&tables)
}

/// A G-pen-like round pen: 1.2 mm, its size following pressure.
fn round_pen() -> Vec<(&'static str, Value)> {
    vec![
        ("BrushSize", Value::Real(1.2)),
        ("BrushSizeUnit", Value::Integer(2)),
        ("BrushSizeEffector", Value::Blob(effector())),
        ("Opacity", Value::Integer(80)),
        ("BrushHardness", Value::Integer(90)),
        ("BrushFlow", Value::Integer(75)),
        ("AntiAlias", Value::Integer(3)),
        ("FlickerReduction", Value::Integer(12)),
        ("FlickerReductionBySpeed", Value::Integer(1)),
        ("BrushInterval", Value::Real(10.0)),
        ("BrushAutoIntervalType", Value::Integer(2)),
        ("BrushUseIn", Value::Integer(1)),
        ("BrushInLength", Value::Real(25.0)),
        ("BrushUseOut", Value::Integer(1)),
        ("BrushOutLength", Value::Real(40.0)),
        ("BrushInOutBySpeed", Value::Integer(1)),
        ("CompositeMode", Value::Integer(0)),
        ("BrushUsePatternImage", Value::Integer(0)),
        ("BrushUseWaterColor", Value::Integer(0)),
    ]
}

/// The value of `column` in the `Variant` row of `file`'s first node.
fn column(file: &[u8], column: &str) -> Option<Value> {
    let tables = sqlite::read(file).unwrap();
    let variants = tables.iter().find(|t| t.name == "Variant").unwrap();
    let at = variants.column(column)?;
    variants.rows.first().map(|(_, row)| row[at].clone())
}

/// `file` with Sue's own table left out, as a program that knows nothing
/// of it would save it again.
fn without_sue(file: &[u8]) -> Vec<u8> {
    let database = sqlite::Database::open(file).unwrap();
    let schema = database.schema().to_vec();
    let tables: Vec<NewTable> = sqlite::read(file)
        .unwrap()
        .into_iter()
        .filter(|table| table.name != "SueBrush" && table.name != "sqlite_sequence")
        .map(|table| {
            let sql = schema
                .iter()
                .find(|entry| entry.name == table.name)
                .unwrap()
                .sql
                .clone();
            let rows = table
                .rows
                .into_iter()
                .map(|(rowid, mut row)| {
                    row[0] = Value::Null;
                    (rowid, row)
                })
                .collect();
            NewTable {
                name: table.name,
                sql,
                rows,
            }
        })
        .collect();
    sqlite::write(&tables)
}

fn no_pictures(_: &[u8; 32]) -> Option<Vec<u8>> {
    None
}

#[test]
fn a_round_pen_imports_with_every_setting_it_has() {
    let file = sut_file(&[("Inking pen", round_pen())], &[]);
    let brushes = sut::import(&file).expect("a sub tool without a tip reads");
    let preset = &brushes[0].preset;
    assert_eq!(preset.name, "Inking pen");
    assert_eq!(preset.tip, TipSource::Round);
    // 1.2 mm at 350 dpi.
    let size = preset.value(param::SIZE).unwrap();
    assert!((size - 16.535).abs() < 0.01, "{size}");
    assert_eq!(preset.value(param::OPACITY), Some(0.8));
    assert_eq!(preset.value(param::HARDNESS), Some(0.9));
    assert_eq!(preset.value(param::FLOW), Some(0.75));
    assert_eq!(preset.antialias, AntiAlias::Strong);
    assert_eq!(
        (preset.input.stabilization, preset.input.by_speed),
        (12, true)
    );
    assert_eq!((preset.taper.start, preset.taper.end), (25.0, 40.0));
    assert!(preset.taper.by_speed);
    // The dynamics: pressure with its curve, random with its minimum.
    let pressure = preset
        .bindings(param::SIZE)
        .find(|b| b.input == input::PRESSURE)
        .unwrap();
    assert_eq!((pressure.min, pressure.max, pressure.on), (0.2, 1.0, true));
    assert_eq!(
        pressure.curve.points(),
        &[[0.0, 0.0], [0.5, 0.25], [1.0, 1.0]]
    );
    let random = preset
        .bindings(param::SIZE)
        .find(|b| b.input == input::RANDOM)
        .unwrap();
    assert_eq!((random.min, random.on), (0.7, true));
    assert!(brushes[0].pictures.is_empty());

    // The old reader's view no longer refuses it either.
    let old = sue_io::brushes::read("pen.sut", &file).expect("no tip is no reason to refuse");
    assert!(old[0].tips.is_empty());
    assert_eq!(old[0].preset.as_ref(), Some(preset));
}

#[test]
fn each_sub_tool_takes_only_its_own_tips() {
    let (first, second) = (png(40, 8), png(200, 12));
    let tipped = |path: &str| {
        let mut columns = round_pen();
        columns.retain(|(name, _)| *name != "BrushUsePatternImage");
        columns.push(("BrushUsePatternImage", Value::Integer(1)));
        columns.push(("BrushPatternImageArray", Value::Blob(array(&[path]))));
        columns.push(("BrushChangePatternColor", Value::Integer(1)));
        columns
    };
    let file = sut_file(
        &[
            ("Leaf", tipped(".:1:a:data:material_0.layer")),
            ("Star", tipped(".:2:b:data:material_0.layer")),
        ],
        &[
            (".:1:a:data:material_0.layer", first.clone()),
            (".:2:b:data:material_0.layer", second.clone()),
        ],
    );
    let brushes = sut::import(&file).unwrap();
    for (brush, png) in brushes.iter().zip([first, second]) {
        let hash = sut::sha256(&png);
        assert_eq!(
            brush.pictures,
            vec![TipPicture { png }],
            "{}",
            brush.preset.name
        );
        let TipSource::Images { pictures, .. } = &brush.preset.tip else {
            panic!("{} has its tip", brush.preset.name);
        };
        assert_eq!(pictures, &vec![PictureRef::Hash(hash)]);
    }
    let old = sue_io::brushes::read("two.sut", &file).unwrap();
    assert_eq!((old[0].tips.len(), old[0].tips[0].0), (1, 8));
    assert_eq!((old[1].tips.len(), old[1].tips[0].0), (1, 12));
}

#[test]
fn unknown_columns_survive_import_and_export() {
    let file = sut_file(&[("Inking pen", round_pen())], &[]);
    let preset = sut::import(&file).unwrap().remove(0).preset;
    // Out again unchanged: every column comes back as it was.
    let out = sut::export(&preset, &no_pictures).unwrap();
    assert_eq!(column(&out, "FutureSetting"), Some(Value::Integer(7)));
    assert_eq!(column(&out, "FutureBlob"), Some(Value::Blob(vec![1, 2, 3])));
    for (name, value) in round_pen() {
        assert_eq!(column(&out, name), Some(value), "{name}");
    }
    let tables = sqlite::read(&out).unwrap();
    let nodes = tables.iter().find(|t| t.name == "Node").unwrap();
    let flag = nodes
        .column("NodeFutureFlag")
        .expect("the node keeps its own columns");
    assert_eq!(nodes.rows[0].1[flag], Value::Integer(0));

    // Edited in Sue: only what changed is written anew.
    let mut edited = preset.clone();
    edited.set(param::SIZE, 30.0);
    edited.input.stabilization = 4;
    let out = sut::export(&edited, &no_pictures).unwrap();
    assert_eq!(column(&out, "BrushSize"), Some(Value::Real(30.0)));
    assert_eq!(column(&out, "BrushSizeUnit"), Some(Value::Integer(0)));
    assert_eq!(column(&out, "FlickerReduction"), Some(Value::Integer(4)));
    assert_eq!(column(&out, "FutureSetting"), Some(Value::Integer(7)));
    assert_eq!(column(&out, "Opacity"), Some(Value::Integer(80)));
    assert_eq!(
        column(&out, "BrushSizeEffector"),
        Some(Value::Blob(effector()))
    );
    let back = sut::import(&without_sue(&out)).unwrap().remove(0).preset;
    assert_eq!(back.value(param::SIZE), Some(30.0));
    assert_eq!(back.input.stabilization, 4);
}

#[test]
fn every_built_in_preset_round_trips_through_sut() {
    for preset in sue_brush::presets::builtin() {
        let file = sut::export(&preset, &no_pictures).expect("a built-in brush exports");
        // Read back by Sue: the very same brush.
        let mut back = sut::import(&file).unwrap().remove(0).preset;
        back.extra.retain(|(key, _)| !key.starts_with("sut."));
        assert_eq!(back, preset, "{}", preset.id);

        // Saved again by a program that drops Sue's own table: what a
        // sub tool can say still comes back.
        let csp = sut::import(&without_sue(&file)).unwrap().remove(0).preset;
        let id = &preset.id;
        for key in [
            param::SIZE,
            param::OPACITY,
            param::HARDNESS,
            param::FLOW,
            param::ANGLE,
        ] {
            let (a, b) = (
                csp.value(key).unwrap_or(1.0),
                preset.value(key).unwrap_or(1.0),
            );
            assert!((a - b).abs() < 0.006, "{id} {key}: {a} {b}");
        }
        assert_eq!(csp.antialias, preset.antialias, "{id}");
        assert_eq!(csp.input.stabilization, preset.input.stabilization, "{id}");
        assert_eq!(csp.input.by_speed, preset.input.by_speed, "{id}");
        assert_eq!(
            (csp.taper.start, csp.taper.end),
            (preset.taper.start, preset.taper.end),
            "{id}"
        );
        assert_eq!(csp.erase, preset.erase, "{id}");
        assert_eq!(csp.mix.is_some(), preset.mix.is_some(), "{id}");
        let pressure = |p: &BrushPreset| -> Option<(f32, bool)> {
            p.bindings(param::SIZE)
                .find(|b| b.input == input::PRESSURE)
                .map(|b: &Binding| (b.min, b.on))
        };
        assert_eq!(pressure(&csp), pressure(&preset), "{id}");
        assert_eq!(
            sue_brush::presets::vector_erase(&csp),
            sue_brush::presets::vector_erase(&preset),
            "{id}"
        );
        // And reading what it reads gives the same again.
        let again = sut::import(&without_sue(&sut::export(&csp, &no_pictures).unwrap()))
            .unwrap()
            .remove(0)
            .preset;
        let strip = |mut p: BrushPreset| {
            p.extra.retain(|(key, _)| !key.starts_with("sut."));
            p
        };
        assert_eq!(strip(again), strip(csp), "{id}");
    }
}

#[test]
fn a_sue_brush_edited_in_another_program_keeps_that_edit() {
    let preset = sue_brush::presets::find("sue.pen.g_pen").unwrap();
    let file = sut::export(&preset, &no_pictures).unwrap();
    // Opacity changed to 50 in the file, the rest as Sue wrote it.
    let tables = sqlite::read(&file).unwrap();
    let schema = sqlite::Database::open(&file).unwrap().schema().to_vec();
    let edited: Vec<NewTable> = tables
        .into_iter()
        .filter(|t| t.name != "sqlite_sequence")
        .map(|mut table| {
            if table.name == "Variant" {
                let at = table.column("Opacity").unwrap();
                table.rows[0].1[at] = Value::Integer(50);
            }
            for row in &mut table.rows {
                row.1[0] = Value::Null;
            }
            NewTable {
                sql: schema
                    .iter()
                    .find(|e| e.name == table.name)
                    .unwrap()
                    .sql
                    .clone(),
                name: table.name,
                rows: table.rows,
            }
        })
        .collect();
    let back = sut::import(&sqlite::write(&edited))
        .unwrap()
        .remove(0)
        .preset;
    assert_eq!(back.value(param::OPACITY), Some(0.5));
    assert_eq!(back.id, preset.id, "still the Sue brush it was");
    assert_eq!(back.taper, preset.taper);
}

#[test]
fn a_picture_tip_goes_out_and_comes_back() {
    let tip = TipPicture { png: png(90, 16) };
    let hash = tip.hash();
    let mut preset = BrushPreset::new("Stamp");
    preset.tip = TipSource::Images {
        pictures: vec![PictureRef::Hash(hash)],
        pick: sue_model::brush::Pick::Random,
        keep_colors: false,
    };
    preset.engine = sue_model::brush::EngineKind::Dab;
    let missing = sut::export(&preset, &no_pictures);
    assert!(missing.is_err(), "a picture that is not at hand is said");
    let png = tip.png.clone();
    let file = sut::export(&preset, &move |h: &[u8; 32]| {
        (*h == hash).then(|| png.clone())
    })
    .unwrap();
    for file in [file.clone(), without_sue(&file)] {
        let brush = sut::import(&file).unwrap().remove(0);
        assert_eq!(brush.pictures, vec![tip.clone()]);
        let TipSource::Images { pictures, .. } = &brush.preset.tip else {
            panic!("the tip came back");
        };
        assert_eq!(pictures, &vec![PictureRef::Hash(hash)]);
    }
}

#[test]
fn files_that_are_no_sub_tool_are_refused() {
    assert!(sut::import(b"not a database").is_err());
    let empty = sqlite::write(&[NewTable {
        name: "Other".to_owned(),
        sql: "CREATE TABLE Other(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, A INTEGER DEFAULT NULL)"
            .to_owned(),
        rows: vec![],
    }]);
    assert_eq!(sut::import(&empty), Err(sut::NOT_A_SUB_TOOL.to_owned()));
}

// ----- The library -------------------------------------------------------------

#[test]
fn the_standard_library_holds_every_built_in_brush_under_stable_ids() {
    let arena = library::standard();
    let again = library::standard();
    assert!(same(&arena, &again), "the same ids every run");
    let builtin = sue_brush::presets::builtin();
    let mut listed = Vec::new();
    for set in library::sets(&arena) {
        assert!(library::tool(&arena, set).is_some());
        for brush in library::brushes(&arena, set) {
            let preset = library::preset(&arena, brush).unwrap();
            assert_eq!(brush, library::stable_id(&preset.id));
            listed.push(preset.clone());
        }
    }
    assert_eq!(listed.len(), builtin.len());
    for preset in &builtin {
        assert!(listed.contains(preset), "{}", preset.id);
    }
    // The decoration tool's groups.
    let groups: Vec<String> = library::sets(&arena)
        .into_iter()
        .filter(|set| library::tool(&arena, *set) == Some("decoration"))
        .map(|set| arena.get_by_id(set).unwrap().name().to_owned())
        .collect();
    assert_eq!(groups, ["Effect", "Flower", "Clothing", "Lines"]);

    // Saved and read again: the same entities, ids and relations.
    let read = library::from_file(&library::to_file(&arena)).unwrap();
    assert!(same(&read, &arena));
}

#[test]
fn custom_sub_tools_of_0_0_75_move_into_the_library() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../sue-paint/tests/golden/paint/item_bank.txt"
    );
    let bank = std::fs::read_to_string(path).unwrap();
    let (arena, moved) = library::open(None, Some(&bank)).unwrap();
    assert!(moved);
    let custom: Vec<_> = library::sets(&arena)
        .into_iter()
        .filter(|set| arena.get_by_id(*set).unwrap().name() == library::CUSTOM)
        .collect();
    assert_eq!(custom.len(), 2, "one for the pen, one for the decoration");
    assert_eq!(library::tool(&arena, custom[0]), Some("pen"));
    let pen = library::preset(&arena, library::brushes(&arena, custom[0])[0]).unwrap();
    assert_eq!(pen.name, "My inking pen");
    assert_eq!(pen.input.stabilization, 12);
    assert_eq!(pen.value(param::SIZE), Some(7.5));
    assert_eq!((pen.taper.start, pen.taper.end), (20.0, 35.0));
    let tip = bank.lines().next().unwrap().splitn(4, '\t').nth(3).unwrap();
    let mut expected = BrushPreset::from_legacy_text(tip);
    expected.name.clone_from(&pen.name);
    expected.group = library::CUSTOM.to_owned();
    expected.input = pen.input;
    assert_eq!(pen, &expected, "read through from_legacy_text");
    let chain = library::preset(&arena, library::brushes(&arena, custom[1])[0]).unwrap();
    assert_eq!(chain.name, "Chain");
    assert_eq!(library::tool(&arena, custom[1]), Some("decoration"));

    // The move is the same every time, and a saved library is read as
    // it is, without moving anything again.
    let (twice, _) = library::open(None, Some(&bank)).unwrap();
    assert!(same(&twice, &arena));
    let saved = library::to_file(&arena);
    let (opened, moved) = library::open(Some(&saved), Some(&bank)).unwrap();
    assert!(!moved);
    assert!(same(&opened, &arena));
}

#[test]
fn custom_sub_tools_saved_in_another_language_find_their_tool() {
    // Sue 0.0.75 named each line's tool in the language it showed: a
    // person who switched from English to German has both in one file.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../sue-paint/tests/golden/paint/item_bank.txt"
    );
    let english = std::fs::read_to_string(path).unwrap();
    let tip = english
        .lines()
        .next()
        .unwrap()
        .splitn(4, '\t')
        .nth(3)
        .unwrap();
    let bank =
        format!("Pen\tA\t3\t{tip}\nFeder\tB\t4\t{tip}\n消しゴム\tC\t0\t{tip}\nFill\tD\t0\t{tip}\n");
    let (arena, moved) = library::open(None, Some(&bank)).unwrap();
    assert!(moved);
    let custom: Vec<_> = library::sets(&arena)
        .into_iter()
        .filter(|set| {
            arena
                .get_by_id(*set)
                .unwrap()
                .name()
                .starts_with(library::CUSTOM)
        })
        .collect();
    let names = |set| -> Vec<String> {
        library::brushes(&arena, set)
            .into_iter()
            .map(|id| library::preset(&arena, id).unwrap().name.clone())
            .collect()
    };
    assert_eq!(custom.len(), 3, "pen, eraser, and the fill on its own");
    assert_eq!(library::tool(&arena, custom[0]), Some("pen"));
    assert_eq!(
        names(custom[0]),
        ["A", "B"],
        "one pen set for both languages"
    );
    assert_eq!(library::tool(&arena, custom[1]), Some("eraser"));
    assert_eq!(names(custom[1]), ["C"]);
    assert_eq!(library::tool(&arena, custom[2]), None);
    assert_eq!(arena.get_by_id(custom[2]).unwrap().name(), "Custom: Fill");
}

/// Applies `edit`, undoes it and checks the arena is as before, redoes it
/// and checks it is as after; ids included.
fn undo_pair(arena: &mut Arena, stack: &mut UndoStack<Arena>, edit: sue_core::txn::Batch<Arena>) {
    let before = arena.clone();
    stack.apply(arena, edit).expect("the edit applies");
    let after = arena.clone();
    assert!(!same(&before, &after), "the edit changed something");
    assert!(stack.undo(arena).unwrap());
    assert!(same(arena, &before), "undo restores the library");
    assert!(stack.redo(arena).unwrap());
    assert!(same(arena, &after), "redo brings the edit back");
}

#[test]
fn every_library_edit_undoes_and_redoes_exactly() {
    let mut arena = library::standard();
    let mut stack = UndoStack::new();
    let pen_set = library::sets(&arena)[0];
    let g_pen = library::stable_id("sue.pen.g_pen");

    let (edit, set) = library::new_set(&arena, "Mine", Some("pen")).unwrap();
    undo_pair(&mut arena, &mut stack, edit);
    assert_eq!(library::sets(&arena).last(), Some(&set));

    let tip = TipPicture { png: png(10, 4) };
    let mut stamp = BrushPreset::new("Stamp");
    stamp.tip = TipSource::Images {
        pictures: vec![PictureRef::Hash(tip.hash())],
        pick: sue_model::brush::Pick::Random,
        keep_colors: false,
    };
    let (edit, stamp_id) =
        library::add_brush(&arena, set, stamp, std::slice::from_ref(&tip)).unwrap();
    undo_pair(&mut arena, &mut stack, edit);
    assert_eq!(library::picture(&arena, &tip.hash()), Some(tip.png.clone()));
    assert_eq!(library::brushes(&arena, set), vec![stamp_id]);

    let mut bigger = library::preset(&arena, g_pen).unwrap().clone();
    bigger.set(param::SIZE, 24.0);
    let edit = library::edit_brush(&arena, g_pen, bigger).unwrap();
    undo_pair(&mut arena, &mut stack, edit);
    let edit = library::rename_brush(&arena, g_pen, "Inking").unwrap();
    undo_pair(&mut arena, &mut stack, edit);
    assert_eq!(arena.get_by_id(g_pen).unwrap().name(), "Inking");

    let (edit, copy) = library::duplicate_brush(&arena, g_pen, "Inking 2").unwrap();
    undo_pair(&mut arena, &mut stack, edit);
    assert_eq!(
        library::brushes(&arena, pen_set)[1],
        copy,
        "right after the original"
    );

    let edit = library::move_brush(&arena, copy, set).unwrap();
    undo_pair(&mut arena, &mut stack, edit);
    assert_eq!(library::brushes(&arena, set), vec![stamp_id, copy]);
    let edit = library::delete_brush(&arena, copy).unwrap();
    undo_pair(&mut arena, &mut stack, edit);

    // Shared and installed again: a new set with brushes of its own ids.
    let shared = library::share_set(&arena, set).unwrap();
    let (edit, installed) = library::install(&arena, &shared).unwrap();
    undo_pair(&mut arena, &mut stack, edit);
    let new = library::brushes(&arena, installed[0]);
    assert_eq!(new.len(), 1);
    assert_ne!(new[0], stamp_id);
    assert_eq!(library::preset(&arena, new[0]).unwrap().name, "Stamp");
    assert_eq!(
        arena.relations().targets(new[0], sue_core::REF),
        arena.relations().targets(stamp_id, sue_core::REF),
        "the picture is shared, not copied"
    );
    let one = library::share_brush(&arena, g_pen).unwrap();
    let alone = library::from_file(&one).unwrap();
    assert_eq!(library::sets(&alone).len(), 1);

    let edit = library::delete_set(&arena, set).unwrap();
    undo_pair(&mut arena, &mut stack, edit);
    assert!(library::install(&arena, b"no file").is_err());
}

#[test]
fn every_text_of_the_brush_files_is_in_the_language_database() {
    for text in [
        sut::NOT_A_SUB_TOOL,
        sut::NO_SUB_TOOL,
        sut::PICTURE_MISSING,
        library::NOT_BRUSHES,
        library::EDIT_BRUSHES,
        library::LIBRARY_NAME,
        library::CUSTOM,
    ] {
        assert!(sue_lang::has_english("io", text), "{text}");
    }
    for preset in sue_brush::presets::builtin() {
        assert!(
            sue_lang::has_english("io", &preset.group),
            "{}",
            preset.group
        );
    }
}
