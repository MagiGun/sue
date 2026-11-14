//! The brush sub tools are Sue's built-in presets (`sue_brush::presets`)
//! and paint as the sub tools of 0.0.75 did: the brush of each is the one
//! that Sue wrote (`crates/sue-paint/tests/golden/tips.txt`, package
//! P0-4), which this file only reads.

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
        let old_names: Vec<&str> = old.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, old_names, "{tool:?}: the same sub tools in order");
        for (sub, (name, text)) in subs.iter().zip(&old) {
            assert_eq!(sub.tip.to_text(), *text, "{name}");
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
