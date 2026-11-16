//! The brush sub tools are Sue's built-in presets (`sue_brush::presets`)
//! and paint as the sub tools of 0.0.75 did, but for those tuned in 0.0.8
//! (`sue_brush::presets::RETUNED`): the brush of each is the one that Sue
//! wrote (`crates/sue-paint/tests/golden/tips.txt`, package P0-4), which
//! this file only reads.

use sue_picture::tools::Tool;

const OLD: &str = include_str!("../../sue-paint/tests/golden/tips.txt");

#[test]
fn every_brush_sub_tool_paints_as_in_0_0_75() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut checked = 0;
    for tool in [
        Tool::Pen,
        Tool::Pencil,
        Tool::Brush,
        Tool::Airbrush,
        Tool::Decoration,
        Tool::Eraser,
    ] {
        let subs = tool.sub_tools();
        let old: Vec<(&str, &str)> = OLD
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(3, '\t');
                (parts.next()? == tool.label()).then_some(())?;
                Some((parts.next()?, parts.next()?))
            })
            .collect();
        let names: Vec<&str> = subs.iter().map(|sub| sub.name.as_str()).collect();
        // P3-5 renamed two pens after Clip Studio Paint Ver. 4; their
        // brushes are the same.
        let old_names: Vec<&str> = old
            .iter()
            .map(|(name, _)| match *name {
                "Real G-Pen" => "Grainy G-pen",
                "Milli pen" => "Fineliner",
                other => other,
            })
            .collect();
        assert_eq!(names, old_names, "{tool:?}: the same sub tools in order");
        let presets = sue_brush::presets::of_tool(match tool {
            Tool::Pen => "pen",
            Tool::Pencil => "pencil",
            Tool::Brush => "brush",
            Tool::Airbrush => "airbrush",
            Tool::Decoration => "decoration",
            _ => "eraser",
        });
        for ((sub, (name, text)), preset) in subs.iter().zip(&old).zip(&presets) {
            // The brushes tuned in 0.0.8 paint as their tuning says, on
            // purpose (`sue_brush::presets::RETUNED` says why).
            if sue_brush::presets::RETUNED
                .iter()
                .any(|(id, _)| *id == preset.id)
            {
                assert_ne!(sub.tip.to_text(), *text, "{name} was retuned");
            } else {
                assert_eq!(sub.tip.to_text(), *text, "{name}");
            }
            checked += 1;
        }
    }
    assert_eq!(checked, sue_brush::presets::builtin().len());
}

#[test]
fn sub_tools_take_their_preset_settings() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let pen = Tool::Pen.sub_tools();
    assert_eq!((pen[0].stabilization, pen[0].flick), (10, true));
    let dot = &pen[9];
    assert_eq!((dot.name.as_str(), dot.stabilization), ("Dot pen", 5));
    assert!(dot.adjust_by_speed && dot.smart_shape);
    let eraser = Tool::Eraser.sub_tools();
    assert_eq!(
        eraser[3].kind, 1,
        "the vector eraser erases up to intersection"
    );
    assert_eq!(eraser[0].kind, 0);
    let decoration = Tool::Decoration.sub_tools();
    let modes: Vec<u8> = decoration.iter().map(|sub| sub.mode).collect();
    assert_eq!(modes, (0..18).collect::<Vec<u8>>());
}

/// Draws every built-in brush, then a G-pen that went out as a `.sut`
/// and came back, one stroke a row, into the PNG `SUE_PRESET_SHEET`
/// names, to look at by eye.
#[test]
fn preset_sheet() {
    use sue_picture::brush::{Finish, Stroke, Tip};
    let Ok(path) = std::env::var("SUE_PRESET_SHEET") else {
        return;
    };
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut tips: Vec<Tip> = [
        Tool::Pen,
        Tool::Pencil,
        Tool::Brush,
        Tool::Airbrush,
        Tool::Decoration,
        Tool::Eraser,
    ]
    .into_iter()
    .flat_map(|tool| tool.sub_tools())
    .filter(|sub| !sub.tip.eraser)
    .map(|sub| sub.tip)
    .collect();
    let pen = sue_brush::presets::find("sue.pen.g_pen").unwrap();
    let file = sue_io::brushes::sut::export(&pen, &|_| None).unwrap();
    let back = sue_io::brushes::read("pen.sut", &file).unwrap().remove(0);
    assert!(back.tips.is_empty());
    tips.push(Tip::from_text(&sue_brush::presets::legacy_text(
        back.preset.as_ref().unwrap(),
    )));
    let (width, row) = (900_u32, 60_u32);
    let height = row * tips.len() as u32;
    let mut raster =
        sue_core::RasterTile2D::new(width, height, sue_core::PixelFormat::Rgba8).unwrap();
    for (index, tip) in tips.iter().enumerate() {
        let tip = Tip {
            size: tip.size.min(40.0),
            ..*tip
        };
        let mut stroke = Stroke::new(sue_core::EntityId::new(), tip, [0.1, 0.1, 0.2], false);
        let y = row as f32 * (index as f32 + 0.5);
        for step in 0..=160 {
            let t = step as f32 / 160.0;
            let x = 40.0 + step as f32 * 5.0;
            let wave = (t * std::f32::consts::TAU).sin() * 10.0;
            stroke.to(
                &mut raster,
                [x, y + wave],
                0.2 + 0.8 * (t * std::f32::consts::PI).sin(),
                None,
            );
        }
        stroke.end(&mut raster, Finish::default(), None);
    }
    let mut white = raster.bytes().unwrap().to_vec();
    for pixel in white.chunks_mut(4) {
        let a = u16::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = (255 - a + u16::from(*channel) * a / 255) as u8;
        }
        pixel[3] = 255;
    }
    let png = sue_io::encode_png_rgba(width, height, &white).unwrap();
    std::fs::write(path, png).unwrap();
}
