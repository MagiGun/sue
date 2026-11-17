//! A `.clip` in Clip Studio's own storage, through the real Sue Paint:
//! a file Clip Studio saved (no Sue tables in it, no pixels stored for
//! its text and vector layers) opens with its fill, text and vector
//! layers live and drawn, and Save, Export to the .clip writes them back
//! in Clip Studio's own storage.

use super::*;
use sue_io::clip::{
    ClipAlign, ClipBitmap, ClipContent, ClipDocument, ClipFill, ClipLayer, ClipLine, ClipPoint,
    ClipText, ClipTextRun, ClipVector, column,
};
use sue_io::sqlite::Value;

fn paint() -> AppletId {
    AppletId::new(sue_paint::ID).unwrap()
}

/// A `.clip` as Clip Studio saves one: a fill, a text and a vector layer
/// written only in Clip Studio's own storage, with no pixels stored for
/// the text and the lines.
fn saved_by_clip_studio(width: u32, height: u32) -> Vec<u8> {
    let mut doc = ClipDocument::new(width, height, 72.0);
    let text = ClipContent::Text(ClipText {
        runs: vec![ClipTextRun {
            text: "Hello".to_owned(),
            font: "Tahoma".to_owned(),
            size: 30.0,
            color: [255, 0x32, 0x64, 255],
            bold: false,
            italic: false,
        }],
        at: [40.0, 30.0],
        frame: None,
        vertical: false,
        align: ClipAlign::Start,
        line_spacing: 1.0,
    });
    let line = ClipLine {
        points: (0..6)
            .map(|i| ClipPoint {
                at: [20.0 + 30.0 * i as f32, 120.0 - 10.0 * i as f32],
                pressure: 1.0,
                width: [0.0, 0.6, 1.0, 1.0, 0.6, 0.0][i],
                opacity: 1.0,
                corner: false,
            })
            .collect(),
        width: 5.0,
        color: [0, 0, 0, 255],
        brush: String::new(),
        closed: false,
    };
    for (name, content) in [
        (
            "Fill 1",
            ClipContent::Fill(ClipFill {
                color: [230, 240, 255, 255],
            }),
        ),
        ("Hello", text),
        (
            "Layer 3",
            ClipContent::Vector(ClipVector { lines: vec![line] }),
        ),
    ] {
        let mut layer = ClipLayer::generated(name, content, 0, 0, ClipBitmap::empty(width, height));
        layer.pixels = Some(ClipBitmap::empty(width, height));
        doc.root.children.push(layer);
    }
    // Written once with Sue's parts, then as Clip Studio keeps it: its
    // own columns only.
    let mut kept = sue_io::clip::read(&sue_io::clip::write(&doc).unwrap()).unwrap();
    for layer in &mut kept.root.children {
        layer.content = None;
    }
    sue_io::clip::write(&kept).unwrap()
}

#[test]
fn p6_clip_a_file_clip_studio_saved_opens_live_and_exports_back_in_its_storage() {
    let mut driver = PaintDriver::new("p6-clip");
    std::fs::create_dir_all(&driver.root).unwrap();
    let path = driver.root.join("From Clip Studio.clip");
    std::fs::write(&path, saved_by_clip_studio(240, 160)).unwrap();
    driver.runtime.request(
        ShellRequest::Picked {
            pick: FilePick::open(Some(paint()), "open", "Open"),
            path: path.clone(),
        },
        &mut NoTextures,
        driver.now,
    );
    driver.settle(20);
    // The Layer palette lists the three layers.
    for name in ["Fill 1", "Hello", "Layer 3"] {
        assert!(driver.rect_of(name).is_some(), "{name} is listed");
    }
    let picture = driver
        .runtime
        .applets
        .get(&paint())
        .and_then(|applet| applet.focus())
        .expect("a canvas is open");
    let arena = &driver.runtime.engine.world.arena;
    let layer = |name: &str| {
        sue_paint::doc::layers(arena, picture)
            .into_iter()
            .find(|info| info.name == name)
            .unwrap_or_else(|| panic!("a layer named {name}"))
            .id
    };
    let class = |name: &str| arena.get_by_id(layer(name)).unwrap().class().to_owned();
    assert_eq!(class("Fill 1"), sue_paint::doc::FILL_LAYER);
    assert_eq!(class("Hello"), sue_paint::doc::TEXT);
    assert_eq!(class("Layer 3"), sue_paint::vector::CLASS);
    // Clip Studio stored no pixels for the words and the line: Sue drew
    // them.
    for name in ["Hello", "Layer 3"] {
        let shows = sue_paint::doc::raster(arena, layer(name))
            .and_then(|raster| raster.bytes().map(<[u8]>::to_vec))
            .is_some_and(|bytes| bytes.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 0));
        assert!(shows, "{name} is drawn");
    }
    // Save asks, then exports back to the .clip, in Clip Studio's own
    // storage.
    driver.step(vec![key(egui::Key::S, egui::Modifiers::COMMAND)]);
    driver.settle(6);
    driver.click_text("Export to the .clip");
    let written = sue_io::clip::read(&std::fs::read(&path).unwrap()).unwrap();
    let find = |name: &str| {
        written
            .all_layers()
            .into_iter()
            .find(|layer| layer.name == name)
            .unwrap()
            .clone()
    };
    let has = |name: &str, key: &str| {
        column(&find(name).columns, key).is_some_and(|value| *value != Value::Null)
    };
    assert!(has("Fill 1", "GradationFillInfo"));
    assert!(has("Hello", "TextLayerAttributes"));
    assert!(has("Layer 3", "VectorNormalStrokeIndex"));
    assert!(matches!(
        find("Layer 3").content,
        Some(ClipContent::Vector(_))
    ));
}
