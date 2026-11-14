//! Generated `.clip` documents for tests: procedural pixels (checkers,
//! gradients, seeded noise, dots on tile edges), every layer kind and
//! flag the model holds, deep folders, every blend number, and foreign
//! data under names no code has heard of.
//!
//! Nothing here comes from a file Clip Studio made: every document is
//! built in code, so tests prove that Sue's reader and writer agree and
//! keep what they do not understand. Whether Clip Studio opens what Sue
//! writes is checked by hand (the hands-on gate of D13).

use super::model::{
    ClipBitmap, ClipChunk, ClipDocument, ClipLayer, ClipMask, ClipRow, ClipShape, ClipTable,
    Columns,
};
use super::schema::{BLENDS, LAYER_COLUMNS, OFFSCREEN_COLUMNS, names, table_sql};
use crate::sqlite::Value;

/// A small seeded random number source (splitmix64).
#[derive(Clone, Debug)]
pub struct Seeded(u64);

impl Seeded {
    /// A source started from `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number below `bound` (0 when `bound` is 0).
    #[allow(clippy::cast_possible_truncation)]
    pub fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next_u64() % bound as u64) as usize
        }
    }

    /// A name of letters no code knows, starting with `Zz`.
    pub fn name(&mut self) -> String {
        let letters = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let len = 4 + self.below(8);
        let mut name = String::from("Zz");
        for _ in 0..len {
            name.push(char::from(letters[self.below(letters.len())]));
        }
        name
    }

    /// A random value of any kind SQLite stores.
    #[allow(clippy::cast_possible_wrap, clippy::cast_precision_loss)]
    pub fn value(&mut self) -> Value {
        match self.below(6) {
            0 => Value::Null,
            1 => Value::Integer(self.next_u64() as i64 >> self.below(60)),
            2 => Value::Real((self.next_u64() >> 11) as f64 / 7.0),
            3 => Value::Text(format!(
                "{} \u{65e5}\u{672c} {}",
                self.name(),
                self.below(99)
            )),
            4 => {
                let len = self.below(3000);
                Value::Blob((0..len).map(|_| self.next_u64() as u8).collect())
            }
            _ => Value::Integer(0),
        }
    }
}

/// A checker of `width` by `height` in straight RGBA8: squares of 7 by 5,
/// colored by position, every other one clear.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn checker(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let on = (x / 7 + y / 5) % 2 == 0;
            out.extend_from_slice(&[
                (x * 255 / width.max(1)) as u8,
                (y * 255 / height.max(1)) as u8,
                90,
                if on { 255 } else { 0 },
            ]);
        }
    }
    out
}

/// A gradient: red across, green down, alpha rising to the right.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn gradient(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let a = (x * 254 / width.max(1)) as u8 + 1;
            out.extend_from_slice(&[x as u8, y as u8, (x + y) as u8, a]);
        }
    }
    out
}

/// Seeded noise, partly clear.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn noise(width: u32, height: u32, seed: u64) -> Vec<u8> {
    let mut random = Seeded::new(seed);
    (0..width as usize * height as usize)
        .flat_map(|_| {
            let bits = random.next_u64();
            let a = if bits & 3 == 0 { 0 } else { (bits >> 8) as u8 };
            [
                (bits >> 16) as u8,
                (bits >> 24) as u8,
                (bits >> 32) as u8,
                a,
            ]
        })
        .collect()
}

/// Single opaque dots on both sides of every tile edge.
#[must_use]
pub fn tile_edges(width: u32, height: u32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let mut out = vec![0_u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let edge = |v: usize| v.is_multiple_of(256) || v % 256 == 255;
            if edge(x) && edge(y) {
                let o = (y * w + x) * 4;
                out[o..o + 4].copy_from_slice(&[200, 10, 60, 255]);
            }
        }
    }
    out
}

/// A one channel picture: a ramp with seeded specks.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn ramp(width: u32, height: u32, seed: u64) -> Vec<u8> {
    let mut random = Seeded::new(seed);
    let (w, h) = (width as usize, height as usize);
    (0..w * h)
        .map(|i| {
            if random.below(17) == 0 {
                random.next_u64() as u8
            } else {
                ((i % w) * 255 / w.max(1)) as u8 ^ ((i / w * 3) as u8)
            }
        })
        .take(w * h.max(1))
        .collect()
}

fn layer(name: &str, left: i64, top: i64, width: u32, height: u32, rgba: &[u8]) -> ClipLayer {
    ClipLayer::raster(name, left, top, ClipBitmap::color(width, height, rgba))
}

/// A document of the raster subset over a `width` by `height` canvas:
/// paper, color layers, a layer reaching off the canvas, a one channel
/// layer, masks on and off with their own offsets, nested folders with
/// their own blend, clipping and closed state, lock bits, draft and
/// reference marks, and one small layer for every blend number Clip
/// Studio has.
#[must_use]
#[allow(clippy::too_many_lines, clippy::cast_possible_wrap)]
pub fn raster_subset(width: u32, height: u32, seed: u64) -> ClipDocument {
    let mut doc = ClipDocument::new(width, height, 350.0);
    let (w, h) = (i64::from(width), i64::from(height));
    let mut paper = ClipLayer::paper("Paper", [250, 240, 200], width, height);
    paper.lock = 1;
    let back = layer("Back", 0, 0, width, height, &checker(width, height));

    // Pixels that reach past every edge of the canvas.
    let (ow, oh) = (width + 40, height + 30);
    let mut off = layer("Off canvas", -20, -15, ow, oh, &noise(ow, oh, seed));
    off.render_offset = [3, -2];
    off.set_opacity_u8(200);
    off.set_blend(*b"mul ");

    // A one channel layer (gray).
    let mut gray = ClipLayer::raster(
        "Gray",
        5,
        7,
        ClipBitmap::gray(
            width.div_ceil(2),
            height.div_ceil(2),
            &ramp(width.div_ceil(2), height.div_ceil(2), seed),
            false,
        ),
    );
    gray.reference = true;

    // Masks on and off, offset from their layers.
    let mut masked = layer("Masked", 0, 0, width, height, &gradient(width, height));
    let (mw, mh) = (width.div_ceil(3) + 1, height.div_ceil(3) + 1);
    masked.mask = Some(ClipMask {
        offset: [w / 4, h / 5],
        offscreen_offset: [1, 2],
        bitmap: ClipBitmap::gray(mw, mh, &ramp(mw, mh, seed ^ 1), true),
    });
    masked.set_mask_enabled(true);
    masked.lock = 3;
    let mut unmasked = layer("Mask off", 2, 3, width, height, &checker(width, height));
    unmasked.mask = Some(ClipMask {
        offset: [-3, 4],
        offscreen_offset: [0, 0],
        bitmap: ClipBitmap::gray(mw, mh, &ramp(mw, mh, seed ^ 2), false),
    });
    unmasked.set_mask_enabled(false);
    unmasked.draft = true;
    unmasked.reference = true;

    // Folders: closed, multiply, clipped, half opaque, five deep.
    let mut inner = ClipLayer::folder("Inner");
    inner.children.push(layer(
        "Edges",
        0,
        0,
        width,
        height,
        &tile_edges(width, height),
    ));
    let mut clipped = layer(
        "Clipped",
        1,
        1,
        width / 2 + 1,
        height / 2 + 1,
        &noise(width / 2 + 1, height / 2 + 1, seed ^ 3),
    );
    clipped.clip = 1;
    clipped.set_visible(false);
    inner.children.push(clipped);
    for depth in 0..5 {
        let mut outer = ClipLayer::folder(&format!("Depth {depth}"));
        outer.set_open(depth % 2 == 0);
        outer.set_blend(if depth == 2 { *b"mul " } else { *b"pass" });
        outer.clip = i64::from(depth == 3);
        outer.opacity = 128 + i64::from(depth);
        outer.lock = i64::from(depth);
        outer.draft = depth == 1;
        outer.children.push(inner);
        inner = outer;
    }

    // Every blend number, Glow Dodge and Add Glow among them.
    let mut blends = ClipLayer::folder("Blends");
    for (number, _) in BLENDS.iter().filter(|(number, _)| *number >= 0) {
        let mut one = layer(
            &format!("Blend {number}"),
            *number,
            0,
            3,
            2,
            &gradient(3, 2),
        );
        one.composite = *number;
        blends.children.push(one);
    }
    let empty = ClipLayer::raster("Empty", 0, 0, ClipBitmap::color(width, height, &[]));

    doc.root.children = vec![
        paper, back, off, gray, masked, unmasked, inner, blends, empty,
    ];
    doc.sue = Some(b"SUE\0FILE a picture's own project".to_vec());
    doc
}

/// Adds foreign data under names from `seed`: chunks of unknown kinds,
/// whole tables, columns on the layer, canvas and bitmap rows, rows no
/// layer reaches (a second mipmap level with its own stored bitmap), and
/// a column of `ExternalChunk`.
#[allow(clippy::too_many_lines)]
pub fn add_foreign(doc: &mut ClipDocument, seed: u64) {
    let mut random = Seeded::new(seed);
    // Chunks of kinds no reader knows.
    for _ in 0..3 {
        let kind = loop {
            let name = random.name().into_bytes();
            let kind = [name[2], name[3], name[4], name[5]];
            if ![*b"Head", *b"Exta", *b"SQLi", *b"Foot"].contains(&kind) {
                break kind;
            }
        };
        let len = random.below(500);
        let data = (0..len)
            .map(|_| random.next_u64().to_le_bytes()[0])
            .collect();
        doc.foreign.chunks.push(ClipChunk { kind, data });
    }
    // Whole tables.
    for _ in 0..3 {
        let name = random.name();
        let columns: Vec<String> = (0..1 + random.below(6)).map(|_| random.name()).collect();
        let definitions: Vec<String> = columns
            .iter()
            .enumerate()
            .map(|(i, column)| {
                let kind = ["INTEGER", "TEXT", "BLOB", "REAL", ""][(i + column.len()) % 5];
                format!("{column} {kind}").trim_end().to_owned()
            })
            .collect();
        let rows = (1..=random.below(40) as i64)
            .map(|rowid| (rowid * 3, columns.iter().map(|_| random.value()).collect()))
            .collect();
        doc.foreign.tables.push(ClipTable {
            sql: format!("CREATE TABLE {name}({})", definitions.join(", ")),
            name,
            columns,
            rows,
        });
    }
    // One with a rowid column, as Clip Studio's tables have.
    let name = random.name();
    doc.foreign.tables.push(ClipTable {
        sql: table_sql(&name, "MainId INTEGER DEFAULT NULL, Data BLOB DEFAULT NULL"),
        columns: vec!["_PW_ID".to_owned(), "MainId".to_owned(), "Data".to_owned()],
        rows: (1..=5)
            .map(|rowid| {
                (
                    rowid,
                    vec![
                        Value::Integer(rowid),
                        Value::Integer(rowid * 9),
                        random.value(),
                    ],
                )
            })
            .collect(),
        name,
    });
    // Columns no code knows, declared in the shapes.
    let layer_column = random.name();
    let canvas_column = random.name();
    let offscreen_column = random.name();
    doc.foreign.shapes.push(ClipShape {
        table: "Layer".to_owned(),
        sql: table_sql(
            "Layer",
            &format!("{LAYER_COLUMNS}, {layer_column} BLOB DEFAULT NULL"),
        ),
        columns: std::iter::once("_PW_ID".to_owned())
            .chain(names(LAYER_COLUMNS))
            .chain(std::iter::once(layer_column.clone()))
            .collect(),
    });
    doc.foreign.shapes.push(ClipShape {
        table: "Offscreen".to_owned(),
        sql: table_sql(
            "Offscreen",
            &format!("{OFFSCREEN_COLUMNS}, {offscreen_column} TEXT DEFAULT NULL"),
        ),
        columns: std::iter::once("_PW_ID".to_owned())
            .chain(names(OFFSCREEN_COLUMNS))
            .chain(std::iter::once(offscreen_column.clone()))
            .collect(),
    });
    let values = |layer: &mut ClipLayer, random: &mut Seeded| {
        layer.columns.push((layer_column.clone(), random.value()));
        if let Some(bitmap) = &mut layer.pixels {
            bitmap.rows[2].push((offscreen_column.clone(), random.value()));
        }
    };
    fn each(layer: &mut ClipLayer, f: &mut dyn FnMut(&mut ClipLayer)) {
        f(layer);
        for child in &mut layer.children {
            each(child, f);
        }
    }
    each(&mut doc.root, &mut |layer| values(layer, &mut random));
    // A column the shape does not declare: the writer adds it.
    doc.canvas_columns
        .push((canvas_column, Value::Text("kept".to_owned())));
    // A second mipmap level of the first layer: rows nothing reads and a
    // stored bitmap no layer reaches.
    let external = "extrnlidZZ00000000000000000000000000000001".to_owned();
    let level: Columns = vec![
        ("MainId".to_owned(), Value::Integer(90_001)),
        ("CanvasId".to_owned(), Value::Integer(1)),
        ("LayerId".to_owned(), Value::Integer(3)),
        ("ThisScale".to_owned(), Value::Real(50.0)),
        ("Offscreen".to_owned(), Value::Integer(90_002)),
        ("NextIndex".to_owned(), Value::Integer(0)),
    ];
    let offscreen: Columns = vec![
        ("MainId".to_owned(), Value::Integer(90_002)),
        ("CanvasId".to_owned(), Value::Integer(1)),
        ("LayerId".to_owned(), Value::Integer(3)),
        ("Attribute".to_owned(), Value::Blob(vec![1, 2, 3])),
        (
            "BlockData".to_owned(),
            Value::Blob(external.clone().into_bytes()),
        ),
        (offscreen_column, Value::Null),
    ];
    doc.foreign.rows.push(ClipRow {
        table: "MipmapInfo".to_owned(),
        columns: level,
    });
    doc.foreign.rows.push(ClipRow {
        table: "Offscreen".to_owned(),
        columns: offscreen,
    });
    let mut data = (external.len() as u64).to_be_bytes().to_vec();
    data.extend_from_slice(external.as_bytes());
    data.extend_from_slice(&7_u64.to_be_bytes());
    data.extend_from_slice(b"pixels!");
    doc.foreign.chunks.push(ClipChunk {
        kind: *b"Exta",
        data,
    });
    // A column of `ExternalChunk` for that bitmap.
    doc.foreign.shapes.push(ClipShape {
        table: "ExternalChunk".to_owned(),
        sql: format!(
            "CREATE TABLE ExternalChunk(ExternalID BLOB, Offset INTEGER, {} INTEGER)",
            "ZzWhen"
        ),
        columns: vec![
            "ExternalID".to_owned(),
            "Offset".to_owned(),
            "ZzWhen".to_owned(),
        ],
    });
    doc.foreign.external.push((
        external,
        vec![("ZzWhen".to_owned(), Value::Integer(20_261_114))],
    ));
    doc.foreign.preamble = b"between".to_vec();
    doc.foreign.foot = b"foot".to_vec();
    doc.foreign.trailing = b"tail".to_vec();
}

/// The generated documents every round trip test runs over, by name.
#[must_use]
pub fn documents() -> Vec<(String, ClipDocument)> {
    let mut out = Vec::new();
    for (width, height) in [(1, 1), (64, 48), (257, 257), (300, 520)] {
        out.push((
            format!("raster subset {width} by {height}"),
            raster_subset(width, height, u64::from(width * 31 + height)),
        ));
    }
    for seed in [1, 2, 3] {
        let mut doc = raster_subset(80, 60, seed);
        add_foreign(&mut doc, seed);
        out.push((format!("foreign data, seed {seed}"), doc));
    }
    out.push(("empty canvas".to_owned(), ClipDocument::new(10, 10, 72.0)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::model::{ClipAttribute, ClipHead, ClipPixels, PAPER};
    use crate::clip::{decode_clip, read, write};

    /// Every bitmap of a layer tree with its layer's name, in writing
    /// order.
    fn bitmaps(doc: &ClipDocument) -> Vec<(String, &ClipBitmap)> {
        let mut out = Vec::new();
        for layer in doc.all_layers().into_iter().chain([&doc.root]) {
            if let Some(bitmap) = &layer.pixels {
                out.push((layer.name.clone(), bitmap));
            }
            if let Some(mask) = &layer.mask {
                out.push((format!("{} mask", layer.name), &mask.bitmap));
            }
        }
        out
    }

    /// Whether every column of `want` is in `got` with its value: the
    /// writer adds the columns it fills in itself.
    fn holds(got: &Columns, want: &Columns) -> bool {
        want.iter()
            .all(|(name, value)| crate::clip::column(got, name) == Some(value))
    }

    fn decoded(bitmap: &ClipBitmap) -> ClipPixels {
        bitmap.decode().unwrap()
    }

    #[test]
    fn writing_what_was_read_gives_the_same_bytes() {
        for (name, doc) in documents() {
            let first = write(&doc).unwrap();
            let read_back = read(&first).unwrap_or_else(|error| panic!("{name}: {error}"));
            let second = write(&read_back).unwrap();
            assert!(first == second, "{name}: the second write differs");
            assert_eq!(read(&second).unwrap(), read_back, "{name}");
        }
    }

    #[test]
    fn the_raster_subset_comes_back_as_it_was_built() {
        for (name, doc) in documents() {
            let back = read(&write(&doc).unwrap()).unwrap();
            assert_eq!(
                (back.width, back.height, back.dpi, &back.sue),
                (doc.width, doc.height, doc.dpi, &doc.sue),
                "{name}"
            );
            let (built, read_layers) = (doc.all_layers(), back.all_layers());
            assert_eq!(built.len(), read_layers.len(), "{name}");
            for (want, got) in built.iter().zip(&read_layers) {
                let what = format!("{name}, layer {}", want.name);
                assert_eq!(
                    (
                        &got.name,
                        got.layer_type,
                        got.folder,
                        got.lock,
                        got.clip,
                        got.opacity,
                        got.composite,
                        got.visibility
                    ),
                    (
                        &want.name,
                        want.layer_type,
                        want.folder,
                        want.lock,
                        want.clip,
                        want.opacity,
                        want.composite,
                        want.visibility
                    ),
                    "{what}"
                );
                assert_eq!(
                    (got.offset, got.render_offset, got.draw_color),
                    (want.offset, want.render_offset, want.draw_color),
                    "{what}"
                );
                assert_eq!(
                    (got.draft, got.reference),
                    (want.draft, want.reference),
                    "{what}"
                );
                assert_eq!(got.children.len(), want.children.len(), "{what}");
                assert!(holds(&got.columns, &want.columns), "{what}");
                assert_eq!(got.mask.is_some(), want.mask.is_some(), "{what}");
                if let (Some(a), Some(b)) = (&got.mask, &want.mask) {
                    assert_eq!(
                        (a.offset, a.offscreen_offset),
                        (b.offset, b.offscreen_offset),
                        "{what}"
                    );
                }
                assert!(!got.uuid.is_empty() && got.id != 0, "{what}");
            }
            let (built, read_bitmaps) = (bitmaps(&doc), bitmaps(&back));
            assert_eq!(built.len(), read_bitmaps.len(), "{name}");
            for ((layer, want), (_, got)) in built.iter().zip(&read_bitmaps) {
                assert_eq!(got.attribute, want.attribute, "{name}, {layer}");
                assert_eq!(got.blocks, want.blocks, "{name}, {layer}");
                for (got, want) in got.rows.iter().zip(&want.rows) {
                    assert!(holds(got, want), "{name}, {layer}");
                }
                assert_eq!(decoded(got), decoded(want), "{name}, {layer}");
            }
            assert!(holds(&back.canvas_columns, &doc.canvas_columns), "{name}");
        }
    }

    #[test]
    fn layers_keep_what_photoshop_records_lose() {
        let doc = raster_subset(64, 48, 9);
        let back = read(&write(&doc).unwrap()).unwrap();
        let find = |name: &str| {
            back.all_layers()
                .into_iter()
                .find(|layer| layer.name == name)
                .unwrap()
                .clone()
        };
        // Paper is paper, in its color.
        let paper = find("Paper");
        assert!(paper.is_paper() && paper.layer_type == PAPER);
        assert_eq!(paper.draw_color.color(), [250, 240, 200]);
        // A one channel layer is read, not dropped.
        let gray = find("Gray");
        let pixels = gray.pixels.as_ref().unwrap().decode().unwrap();
        assert_eq!((pixels.channels, pixels.width, pixels.height), (1, 32, 24));
        assert_eq!(pixels.data, ramp(32, 24, 9));
        // Masks are masks, on or off, where they were.
        let masked = find("Masked");
        assert!(masked.mask_enabled());
        assert_eq!(masked.mask_at(), Some([64 / 4 + 1, 48 / 5 + 2]));
        let mask = masked.mask.as_ref().unwrap();
        assert!(mask.bitmap.white());
        assert_eq!(mask.bitmap.decode().unwrap().data, ramp(23, 17, 9 ^ 1));
        let off = find("Mask off");
        assert!(!off.mask_enabled() && off.visible());
        assert_eq!(off.mask_at(), Some([2 - 3, 3 + 4]));
        assert!(off.draft && off.reference);
        // Pixels off the canvas are all there.
        let wide = find("Off canvas");
        assert_eq!(wide.pixels_at(), [-17, -17]);
        let pixels = wide.pixels.as_ref().unwrap().decode().unwrap();
        assert_eq!((pixels.width, pixels.height), (104, 78));
        let noise = noise(104, 78, 9);
        for (want, got) in noise.chunks(4).zip(pixels.data.chunks(4)) {
            assert_eq!(got, if want[3] == 0 { &[0; 4][..] } else { want });
        }
        assert_eq!((wide.opacity_u8(), wide.blend()), (200, *b"mul "));
        // Folders keep their blend, clipping, opacity and closed state.
        let folders: Vec<ClipLayer> = (0..5).map(|d| find(&format!("Depth {d}"))).collect();
        assert_eq!(folders[2].blend(), *b"mul ");
        assert_eq!(folders[0].blend(), *b"pass");
        assert!(folders[3].clipped() && !folders[2].clipped());
        assert!(folders[0].is_open() && !folders[1].is_open());
        assert_eq!(folders[4].opacity, 132);
        assert!(folders[1].draft);
        assert!(!find("Clipped").visible() && find("Clipped").clipped());
        // Glow Dodge and Add Glow keep their own numbers.
        assert_eq!(find("Blend 10").composite, 10);
        assert_eq!(find("Blend 12").composite, 12);
        assert_eq!(find("Blend 36").composite, 36);
        // Lock bits as they were.
        assert_eq!(
            (find("Masked").lock, folders[3].lock, paper.lock),
            (3, 3, 1)
        );
    }

    #[test]
    fn every_file_gets_ids_of_its_own() {
        let a = read(&write(&raster_subset(64, 48, 1)).unwrap()).unwrap();
        let b = read(&write(&raster_subset(64, 48, 2)).unwrap()).unwrap();
        let ids = |doc: &ClipDocument| -> Vec<String> {
            let mut ids: Vec<String> = doc
                .all_layers()
                .iter()
                .map(|layer| layer.uuid.clone())
                .collect();
            ids.extend(
                bitmaps(doc)
                    .iter()
                    .map(|(_, bitmap)| bitmap.external.clone()),
            );
            ids
        };
        let (ids_a, ids_b) = (ids(&a), ids(&b));
        let unique: std::collections::HashSet<&String> = ids_a.iter().collect();
        assert_eq!(unique.len(), ids_a.len(), "ids repeat inside one file");
        assert!(
            ids_a.iter().all(|id| !ids_b.contains(id)),
            "two pictures share ids"
        );
        let (ClipHead::Sue { id: head_a }, ClipHead::Sue { id: head_b }) = (&a.head, &b.head)
        else {
            panic!("the head is laid out as Sue writes it");
        };
        assert_ne!(head_a, head_b);
        assert_ne!(*head_a, [0; 16]);
        // The same document written twice is the same file.
        assert_eq!(
            write(&raster_subset(64, 48, 1)).unwrap(),
            write(&raster_subset(64, 48, 1)).unwrap()
        );
        // Ids set twice by hand are made unique when written.
        let mut doc = raster_subset(20, 20, 4);
        let uuid = "0011223344-5566-7788-99aa-bbccddeeff".to_owned();
        doc.root.children[1].uuid.clone_from(&uuid);
        doc.root.children[2].uuid.clone_from(&uuid);
        doc.root.children[1].id = 77;
        doc.root.children[2].id = 77;
        let back = read(&write(&doc).unwrap()).unwrap();
        assert_eq!(back.root.children[1].uuid, uuid);
        assert_ne!(back.root.children[2].uuid, uuid);
        assert_eq!(back.root.children[1].id, 77);
        assert_ne!(back.root.children[2].id, 77);
    }

    #[test]
    fn foreign_tables_columns_rows_and_chunks_come_back_unchanged() {
        for seed in [11, 12, 13, 14] {
            let mut doc = raster_subset(40, 30, seed);
            add_foreign(&mut doc, seed);
            let back = read(&write(&doc).unwrap()).unwrap();
            // The tables Sue writes and does not read come back too.
            let kept: Vec<&ClipTable> = back
                .foreign
                .tables
                .iter()
                .filter(|table| !table.name.starts_with("Zz"))
                .collect();
            assert_eq!(kept.len(), 6, "seed {seed}");
            let foreign: Vec<&ClipTable> = back
                .foreign
                .tables
                .iter()
                .filter(|table| table.name.starts_with("Zz"))
                .collect();
            assert_eq!(
                foreign,
                doc.foreign.tables.iter().collect::<Vec<_>>(),
                "seed {seed}"
            );
            assert_eq!(back.foreign.chunks, doc.foreign.chunks, "seed {seed}");
            assert_eq!(back.foreign.rows, doc.foreign.rows, "seed {seed}");
            // The other chunks' rows read back with that column empty.
            for (id, columns) in &doc.foreign.external {
                let got = back.foreign.external.iter().find(|(known, _)| known == id);
                assert_eq!(
                    got.map(|(_, columns)| columns),
                    Some(columns),
                    "seed {seed}"
                );
            }
            assert_eq!(
                (
                    &back.foreign.preamble,
                    &back.foreign.foot,
                    &back.foreign.trailing
                ),
                (
                    &doc.foreign.preamble,
                    &doc.foreign.foot,
                    &doc.foreign.trailing
                ),
                "seed {seed}"
            );
            // Shapes come back with what they declared first, in order;
            // the writer adds what the model needs after (the paper's
            // color columns here).
            for shape in &doc.foreign.shapes {
                let got = back.foreign.shape(&shape.table).unwrap();
                let open = &shape.sql[..shape.sql.len() - 1];
                assert!(got.sql.starts_with(open), "seed {seed}: {}", got.sql);
                assert!(got.columns.starts_with(&shape.columns), "seed {seed}");
            }
            // The canvas column no shape declared was added to the shape.
            let canvas = back.foreign.shape("Canvas").unwrap();
            assert_eq!(canvas.columns.last(), Some(&doc.canvas_columns[0].0));
            assert!(holds(&back.canvas_columns, &doc.canvas_columns));
            for (want, got) in doc.all_layers().iter().zip(back.all_layers()) {
                assert!(holds(&got.columns, &want.columns), "seed {seed}");
                if let (Some(a), Some(b)) = (&want.pixels, &got.pixels) {
                    assert!(holds(&b.rows[2], &a.rows[2]), "seed {seed}");
                }
            }
        }
    }

    #[test]
    fn a_clip_from_0_0_75_reads_and_writes_without_loss() {
        let golden = include_bytes!("../../tests/golden/picture.clip");
        let doc = read(golden).unwrap();
        assert!(doc.sue.is_some());
        let again = write(&doc).unwrap();
        assert_eq!(read(&again).unwrap(), doc);
        assert_eq!(write(&read(&again).unwrap()).unwrap(), again);
        assert_eq!(decode_clip(&again).unwrap(), decode_clip(golden).unwrap());
        // Nothing was unknown in a file Sue wrote.
        assert!(doc.foreign.rows.is_empty() && doc.foreign.chunks.is_empty());
        assert!(doc.foreign.external.is_empty());
        let kept: Vec<&str> = doc.foreign.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            kept,
            [
                "Project",
                "CanvasItemBank",
                "AnimationCutBank",
                "LayerCompManager",
                "LayerThumbnail",
                "ExternalTableAndColumnName"
            ]
        );
    }

    /// Reads `bytes`, and when that answers a document, writes it and
    /// decodes every bitmap of small size: none of it may panic.
    fn survives(bytes: &[u8]) {
        if let Ok(doc) = read(bytes) {
            let _ = write(&doc);
            for (_, bitmap) in bitmaps(&doc) {
                let small = bitmap
                    .size()
                    .is_some_and(|[w, h]| u64::from(w) * u64::from(h) <= 1 << 20);
                if small {
                    let _ = bitmap.decode();
                }
            }
            let _ = decode_clip(bytes);
        }
    }

    #[test]
    fn cut_or_flipped_files_never_panic() {
        let mut doc = raster_subset(40, 30, 5);
        add_foreign(&mut doc, 5);
        let bytes = write(&doc).unwrap();
        let layout = crate::clip::layout(&bytes).unwrap();
        for chunk in &layout.chunks {
            for cut in [
                chunk.offset,
                chunk.offset + 8,
                chunk.offset + 16,
                chunk.offset + 17,
            ] {
                survives(&bytes[..cut.min(bytes.len())]);
            }
        }
        for cut in (0..bytes.len()).step_by(211) {
            survives(&bytes[..cut]);
        }
        let mut random = Seeded::new(0x5eed);
        for _ in 0..600 {
            let mut flipped = bytes.clone();
            for _ in 0..1 + random.below(3) {
                let at = random.below(flipped.len());
                flipped[at] ^= 1 << random.below(8);
            }
            survives(&flipped);
        }
        // A broken attribute stays, and says why it cannot be decoded.
        let mut odd = raster_subset(8, 8, 1);
        if let Some(bitmap) = &mut odd.root.children[1].pixels {
            bitmap.attribute = ClipAttribute::Raw(b"no parts".to_vec());
        }
        let back = read(&write(&odd).unwrap()).unwrap();
        let bitmap = back.root.children[1].pixels.as_ref().unwrap();
        assert_eq!(bitmap.attribute, ClipAttribute::Raw(b"no parts".to_vec()));
        assert!(bitmap.decode().is_err());
    }

    #[test]
    fn a_sue_layer_table_with_more_columns_reads_and_writes_again() {
        // A newer Sue may give its own layer table a column: the marks
        // still read, the rows that use the column come back as they
        // were, and the file can be written again.
        let mut doc = raster_subset(20, 20, 6);
        doc.foreign.shapes.push(ClipShape {
            table: "SueLayer".to_owned(),
            sql: "CREATE TABLE SueLayer(LayerId INTEGER, Draft INTEGER, Reference INTEGER, \
                  ZzLater TEXT)"
                .to_owned(),
            columns: ["LayerId", "Draft", "Reference", "ZzLater"]
                .map(str::to_owned)
                .to_vec(),
        });
        let later: Columns = vec![
            ("LayerId".to_owned(), Value::Integer(4)),
            ("Draft".to_owned(), Value::Integer(1)),
            ("Reference".to_owned(), Value::Integer(0)),
            ("ZzLater".to_owned(), Value::Text("newer".to_owned())),
        ];
        doc.foreign.rows.push(ClipRow {
            table: "SueLayer".to_owned(),
            columns: later.clone(),
        });
        let first = write(&doc).unwrap();
        let back = read(&first).unwrap();
        let marked: Vec<(&str, bool, bool)> = back
            .all_layers()
            .into_iter()
            .filter(|layer| layer.draft || layer.reference)
            .map(|layer| (layer.name.as_str(), layer.draft, layer.reference))
            .collect();
        let want: Vec<(&str, bool, bool)> = doc
            .all_layers()
            .into_iter()
            .filter(|layer| layer.draft || layer.reference)
            .map(|layer| (layer.name.as_str(), layer.draft, layer.reference))
            .collect();
        assert_eq!(marked, want);
        assert_eq!(back.foreign.rows.len(), 1);
        assert!(holds(&back.foreign.rows[0].columns, &later));
        assert_eq!(write(&back).unwrap(), first);
    }

    #[test]
    fn a_bitmap_chunk_whose_length_does_not_match_keeps_its_pixels() {
        let doc = raster_subset(40, 30, 7);
        let mut bytes = write(&doc).unwrap();
        let want = decode_clip(&bytes).unwrap();
        let layout = crate::clip::layout(&bytes).unwrap();
        let exta: Vec<usize> = layout
            .chunks
            .iter()
            .filter(|chunk| &chunk.kind == b"Exta")
            .map(|chunk| chunk.offset)
            .collect();
        assert!(!exta.is_empty());
        for offset in exta {
            // The payload's length sits after the 16 bytes of the chunk's
            // mark, the id's length and the id.
            let id_len = u64::from_be_bytes(bytes[offset + 16..offset + 24].try_into().unwrap());
            let at = offset + 24 + usize::try_from(id_len).unwrap();
            let len = u64::from_be_bytes(bytes[at..at + 8].try_into().unwrap());
            bytes[at..at + 8].copy_from_slice(&(len - 1).to_be_bytes());
        }
        assert_eq!(decode_clip(&bytes).unwrap(), want);
    }

    #[test]
    fn documents_that_cannot_be_written_say_why() {
        assert!(write(&ClipDocument::new(0, 5, 72.0)).is_err());
        assert!(write(&ClipDocument::new(300_001, 5, 72.0)).is_err());
        let mut clash = ClipDocument::new(5, 5, 72.0);
        clash.foreign.tables.push(ClipTable {
            name: "Layer".to_owned(),
            sql: "CREATE TABLE Layer(a)".to_owned(),
            columns: vec!["a".to_owned()],
            rows: Vec::new(),
        });
        assert!(write(&clash).is_err());
        let mut lost = ClipDocument::new(5, 5, 72.0);
        lost.foreign.rows.push(ClipRow {
            table: "Project".to_owned(),
            columns: Vec::new(),
        });
        assert!(write(&lost).is_err());
    }
}
