//! Phase 6, tools B: the full guide's ruler button links (00:06:17), a
//! picked vector line's brush shape (00:07:50) and its tool tour from the
//! Figure tool to Correct Line (00:36:03 to 00:38:12), each test named
//! after the step or gap it closes. The rows they close are listed with
//! the plan (`acceptance/full_guide.md`, `acceptance/crash_tips.md`);
//! nothing of the video is quoted here.
//!
//! Every test drives real frames: it clicks what a person clicks, found
//! by the names the frame gives its controls, and reads what changed in
//! the document. Each change to the document is undone and redone and
//! compared with the arena before and after.

use super::*;
use egui::pos2;

// Helpers (as the full guide's tests have them).

/// The whole arena as bytes, so two states compare exactly.
fn arena_bytes(paint: &PaintDriver) -> Vec<u8> {
    let mut out = sue_core::codec::Writer::new();
    paint.runtime.engine.world.arena.encode(&mut out);
    out.into_bytes()
}

fn arena(paint: &PaintDriver) -> &sue_core::Arena {
    &paint.runtime.engine.world.arena
}

/// The picture of the canvas Sue Paint has open.
fn picture(paint: &PaintDriver) -> sue_core::EntityId {
    paint
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.focus())
        .expect("a canvas is open")
}

/// The layers of the open canvas, bottom first (folders too).
fn layers(paint: &PaintDriver) -> Vec<sue_paint::doc::LayerInfo> {
    sue_paint::doc::layers(arena(paint), picture(paint))
}

fn entity(paint: &PaintDriver, id: sue_core::EntityId) -> &sue_core::Entity {
    arena(paint).get_by_id(id).unwrap()
}

/// The layers of class `class`, bottom first.
fn of_class(paint: &PaintDriver, class: &str) -> Vec<sue_core::EntityId> {
    layers(paint)
        .into_iter()
        .filter(|info| entity(paint, info.id).class() == class)
        .map(|info| info.id)
        .collect()
}

/// What the shell says now, if anything.
fn said(paint: &PaintDriver) -> String {
    paint
        .runtime
        .shell
        .message()
        .map(|(text, _)| text.to_string())
        .unwrap_or_default()
}

/// One frame that also returns every control the frame named for
/// assistive technology, with its place.
fn step_told(paint: &mut PaintDriver, events: Vec<egui::Event>) -> Vec<(String, egui::Rect)> {
    paint.ctx.enable_accesskit();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        events,
        time: Some(paint.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    paint.now += Duration::from_millis(16);
    let runtime = &mut paint.runtime;
    let now = paint.now;
    let output = paint
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    paint.texts = text_rects(&output);
    let named = output
        .platform_output
        .accesskit_update
        .iter()
        .flat_map(|update| update.nodes.iter())
        .filter_map(|(_, node)| {
            let bounds = node.bounds()?;
            Some((
                node.label()?.to_owned(),
                egui::Rect::from_min_max(
                    pos2(bounds.x0 as f32, bounds.y0 as f32),
                    pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
            ))
        })
        .collect();
    output.drop_without_applying_deltas();
    named
}

/// Every name the next frame gives, texts and controls.
fn names(paint: &mut PaintDriver) -> Vec<String> {
    let mut found: Vec<String> = step_told(paint, Vec::new())
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    found.extend(paint.texts.iter().map(|(text, _)| text.clone()));
    found.sort();
    found.dedup();
    found
}

/// Where the control named `name` is drawn now, the topmost.
fn named(paint: &mut PaintDriver, name: &str) -> egui::Rect {
    let found = step_told(paint, Vec::new());
    found
        .iter()
        .filter(|(label, rect)| label == name && rect.width() > 0.0)
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| {
            panic!(
                "{name:?} is on screen: {:?}",
                found.iter().map(|(label, _)| label).collect::<Vec<_>>()
            )
        })
}

/// A click at `at`.
fn click_at(paint: &mut PaintDriver, at: Pos2) {
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    paint.step(vec![egui::Event::PointerMoved(at)]);
    paint.step(vec![press(true)]);
    paint.step(vec![press(false)]);
    paint.settle(6);
}

/// A click on the control named `name`.
fn click_named(paint: &mut PaintDriver, name: &str) {
    let at = named(paint, name).center();
    click_at(paint, at);
}

/// Picks `option` of the row named `row`: the option itself when it is
/// shown beside the row's name, else from the row's drop-down (the
/// option the opened list newly shows).
fn choose(paint: &mut PaintDriver, row: &str, option: &str) {
    paint.settle(2);
    let label = paint
        .rect_of(row)
        .unwrap_or_else(|| panic!("the row {row:?}: {:?}", paint.texts));
    let beside = paint
        .texts
        .iter()
        .find(|(text, rect)| text == option && (rect.center().y - label.center().y).abs() < 10.0)
        .map(|(_, rect)| rect.center());
    if let Some(at) = beside {
        click_at(paint, at);
        return;
    }
    let before = paint.texts.clone();
    click_at(
        paint,
        label.center() + vec2(label.width() / 2.0 + 60.0, 0.0),
    );
    let at = paint
        .texts
        .iter()
        .find(|(text, rect)| {
            text == option && !before.iter().any(|(was, at)| was == text && at == rect)
        })
        .map(|(_, rect)| rect.center())
        .unwrap_or_else(|| panic!("{option:?} in the list of {row:?}: {:?}", paint.texts));
    click_at(paint, at);
}

/// Ctrl+Z, as a person undoes.
fn undo(paint: &mut PaintDriver) {
    paint.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    paint.settle(4);
}

/// Ctrl+Y, as a person redoes.
fn redo(paint: &mut PaintDriver) {
    paint.step(vec![key(egui::Key::Y, egui::Modifiers::COMMAND)]);
    paint.settle(4);
}

/// Does `change`, which must change the document; `check` holds after
/// it and not before. Undo gives back the arena exactly as before (ids
/// included), redo exactly as after.
fn undo_pair(
    paint: &mut PaintDriver,
    what: &str,
    change: impl FnOnce(&mut PaintDriver),
    check: impl Fn(&PaintDriver) -> bool,
) {
    paint.settle(2);
    let before = arena_bytes(paint);
    assert!(!check(paint), "{what}: not so before");
    change(paint);
    paint.settle(4);
    assert!(check(paint), "{what}: so after ({})", said(paint));
    let after = arena_bytes(paint);
    assert_ne!(before, after, "{what}: the document changed");
    undo(paint);
    assert!(
        arena_bytes(paint) == before,
        "{what}: undo gives back before"
    );
    redo(paint);
    assert!(arena_bytes(paint) == after, "{what}: redo gives back after");
}

/// A stroke of the current tool from `from` to `to` on the canvas.
fn stroke(paint: &mut PaintDriver, from: Pos2, to: Pos2) {
    paint.drag(from, to, egui::PointerButton::Primary);
}

/// A stroke through every point of `path` (screen points).
fn trace(paint: &mut PaintDriver, path: &[Pos2]) {
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    paint.step(vec![egui::Event::PointerMoved(path[0])]);
    paint.step(vec![press(path[0], true)]);
    for pair in path.windows(2) {
        for index in 1..=6 {
            let at = pair[0] + (pair[1] - pair[0]) * (index as f32 / 6.0);
            paint.step(vec![egui::Event::PointerMoved(at)]);
        }
    }
    paint.step(vec![press(path[path.len() - 1], false)]);
    paint.settle(10);
}

/// The layer's pixels.
fn pixels(paint: &PaintDriver, layer: sue_core::EntityId) -> Vec<u8> {
    sue_paint::doc::raster(arena(paint), layer)
        .map(|raster| raster.bytes().unwrap_or_default().to_vec())
        .unwrap_or_default()
}

/// The layer's width in pixels.
fn width_of(paint: &PaintDriver, layer: sue_core::EntityId) -> usize {
    sue_paint::doc::raster(arena(paint), layer).map_or(0, |raster| raster.width() as usize)
}

/// How much ink the layer holds: the sum of its alpha.
fn ink(paint: &PaintDriver, layer: sue_core::EntityId) -> u64 {
    pixels(paint, layer)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| u64::from(pixel[3]))
        .sum()
}

/// The middle of the canvas on screen.
fn middle(paint: &PaintDriver) -> Pos2 {
    paint_middle(&paint.runtime)
}

/// The current layer: the one last made (each test makes its layers in
/// order, and a new layer becomes the current one on top).
fn current_layer(paint: &PaintDriver) -> sue_core::EntityId {
    layers(paint).last().unwrap().id
}

/// Scrolls the palette at `over` from its top to its end; every text and
/// control name it showed on the way.
fn scroll_all(paint: &mut PaintDriver, over: Pos2) -> Vec<String> {
    let wheel = |paint: &mut PaintDriver, by: f32| {
        paint.step(vec![
            egui::Event::PointerMoved(over),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, by),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            },
        ]);
        paint.settle(1);
    };
    for _ in 0..30 {
        wheel(paint, 400.0);
    }
    let mut seen = Vec::new();
    for _ in 0..50 {
        seen.extend(names(paint));
        wheel(paint, -60.0);
    }
    for _ in 0..30 {
        wheel(paint, 400.0);
    }
    seen.sort();
    seen.dedup();
    seen
}

/// Every row Tool Property shows for the sub tool picked now.
fn tool_property_rows(paint: &mut PaintDriver) -> Vec<String> {
    paint.settle(2);
    paint.click_text("Tool Property");
    let tab = paint
        .rect_of("Tool Property")
        .expect("the Tool Property tab");
    scroll_all(paint, tab.center() + vec2(0.0, 160.0))
}

/// Runs `tool`'s command and shows its Sub Tool palette; every text.
fn sub_tools_of(paint: &mut PaintDriver, tool: &str) -> Vec<String> {
    paint.command(tool);
    paint.settle(3);
    paint.click_text("Sub Tool");
    let mut seen: Vec<String> = paint.texts.iter().map(|(text, _)| text.clone()).collect();
    seen.sort();
    seen.dedup();
    seen
}

/// Picks sub tool `sub` of `tool` (the tool's command), as a key bound
/// to the sub tool does, and shows the Sub Tool palette.
fn pick_sub(paint: &mut PaintDriver, tool: &str, sub: &str) {
    let id = match tool {
        "tool.pick.figure" => "figure",
        "tool.pick.frame" => "frame_border",
        "tool.pick.ruler" => "ruler",
        "tool.pick.text" => "text",
        "tool.pick.balloon" => "balloon",
        "tool.correct_line" => "correct_line",
        other => panic!("no sub tool family for {other}"),
    };
    sub_tools_of(paint, tool);
    paint.command(&format!("subtool.{id}.{}", sub.replace(' ', "_")));
    paint.settle(4);
}

/// A click on the Layer palette's row of the layer named `name` (the
/// palette is on the right of the window).
fn click_layer_row(paint: &mut PaintDriver, name: &str) {
    paint.settle(2);
    let at = paint
        .texts
        .iter()
        .filter(|(text, rect)| text == name && rect.left() > 1000.0)
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("a layer row {name:?}: {:?}", paint.texts))
        .center();
    click_at(paint, at);
}

/// Picks sub tool `sub` of `tool`; the rows its Tool Property shows.
fn rows_of(paint: &mut PaintDriver, tool: &str, sub: &str) -> Vec<String> {
    pick_sub(paint, tool, sub);
    tool_property_rows(paint)
}

/// Shows Tool Property, scrolled to its top.
fn show_tool_property(paint: &mut PaintDriver) {
    tool_property_rows(paint);
    paint.settle(2);
}

/// Asserts each of `wanted` is among `seen`.
fn shows_all(seen: &[String], wanted: &[&str], what: &str) {
    let missing: Vec<&&str> = wanted
        .iter()
        .filter(|want| !seen.iter().any(|text| text == **want))
        .collect();
    assert!(
        missing.is_empty(),
        "{what}: {missing:?} missing from {seen:?}"
    );
}

/// The text property `key` of `id`.
fn text_of(paint: &PaintDriver, id: sue_core::EntityId, key: &str) -> Option<String> {
    entity(paint, id)
        .property(key)
        .and_then(sue_core::Value::as_str)
        .map(str::to_owned)
}

/// The flag property `key` of `id` (off when missing).
fn flag_of(paint: &PaintDriver, id: sue_core::EntityId, key: &str) -> bool {
    entity(paint, id)
        .property(key)
        .and_then(sue_core::Value::as_bool)
        .unwrap_or(false)
}

/// Writes the canvas's picture, as it is composited, to the folder named
/// by `SUE_P6_SHOTS` (when it is set) as `name.png`: what a person sees on
/// the canvas, to look at.
fn shoot_canvas(paint: &PaintDriver, name: &str) {
    let Ok(dir) = std::env::var("SUE_P6_SHOTS") else {
        return;
    };
    let arena = arena(paint);
    let picture = picture(paint);
    let options = sue_paint::composite::RenderOptions::default();
    let Some(flat) = sue_paint::composite::flatten(arena, picture, options) else {
        return;
    };
    let (w, h) = (flat.width(), flat.height());
    let Some(bytes) = flat.bytes() else {
        return;
    };
    if let Ok(png) = sue_io::encode_png_rgba(w, h, bytes) {
        let path = std::path::Path::new(&dir).join(format!("{name}.png"));
        let _ = sue_platform::fs::write(path, png);
    }
}

// The tests.

/// FGV-1 (00:06:17): the Layer palette's ruler drop-down holds Link Guide
/// to Ruler and Link Ruler to Layer under the showing areas. Linked, the
/// ruler moves when its layer moves; guides linked to the ruler show
/// only where it does, unlinked on every layer.
#[test]
fn step_00_06_17_fgv_1_link_guide_and_link_ruler_to_layer() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p6b-ruler-links");
    let page = picture(&paint);
    let layer = current_layer(&paint);
    // A linear ruler and a guide on the layer.
    pick_sub(&mut paint, "tool.pick.ruler", "Linear Ruler");
    let at = middle(&paint);
    stroke(&mut paint, at - vec2(80.0, 0.0), at + vec2(80.0, 0.0));
    pick_sub(&mut paint, "tool.pick.ruler", "Guide");
    stroke(&mut paint, at + vec2(-60.0, 50.0), at + vec2(60.0, 50.0));
    let rulers = sue_paint::rulers::Rulers::of(arena(&paint), page);
    assert!(rulers.linear.is_some(), "a linear ruler");
    assert_eq!(rulers.guides.len(), 1, "a guide");
    // The drop-down.
    click_named(&mut paint, "Set Showing Area of Ruler");
    let texts: Vec<String> = paint.texts.iter().map(|(text, _)| text.clone()).collect();
    shows_all(
        &texts,
        &["Link Guide to Ruler", "Link Ruler to Layer"],
        "the ruler drop-down",
    );
    undo_pair(
        &mut paint,
        "link ruler to layer",
        |paint| {
            paint.click_text("Link Ruler to Layer");
        },
        move |paint| sue_paint::rulers::layer_linked(arena(paint), page),
    );
    // The layer moves 30 pixels right: the ruler with it.
    let before = sue_paint::rulers::Rulers::of(arena(&paint), page)
        .linear
        .unwrap();
    paint.command("tool.move_layer");
    paint.settle(2);
    stroke(&mut paint, at + vec2(0.0, -40.0), at + vec2(40.0, -40.0));
    let moved = sue_paint::doc::layers(arena(&paint), page)
        .into_iter()
        .find(|info| info.id == layer)
        .unwrap()
        .style
        .offset;
    assert!(moved[0] > 0, "the layer moved: {moved:?}");
    let after = sue_paint::rulers::Rulers::of(arena(&paint), page)
        .linear
        .unwrap();
    for (was, now) in before.iter().zip(after) {
        assert!(
            (now[0] - was[0] - moved[0] as f32).abs() < 0.01
                && (now[1] - was[1] - moved[1] as f32).abs() < 0.01,
            "the ruler moved with its layer: {before:?} to {after:?}, by {moved:?}"
        );
    }
    // The ruler shows only while its layer is edited; the guide, not
    // linked, everywhere; linked, with the ruler.
    click_named(&mut paint, "Set Showing Area of Ruler");
    paint.click_text("Show only when editing target");
    paint.command("layer.new");
    paint.settle(4);
    let other = current_layer(&paint);
    assert_ne!(other, layer);
    let seen = sue_paint::rulers::Rulers::seen(arena(&paint), page, Some(other));
    assert!(seen.linear.is_none(), "the ruler is not shown here");
    assert_eq!(seen.guides.len(), 1, "the guide shows on every layer");
    click_named(&mut paint, "Set Showing Area of Ruler");
    undo_pair(
        &mut paint,
        "link guide to ruler",
        |paint| {
            paint.click_text("Link Guide to Ruler");
        },
        move |paint| sue_paint::rulers::guides_linked(arena(paint), page),
    );
    let seen = sue_paint::rulers::Rulers::seen(arena(&paint), page, Some(other));
    assert!(
        seen.guides.is_empty(),
        "linked, the guide goes with the ruler"
    );
    let own = sue_paint::rulers::Rulers::seen(arena(&paint), page, Some(layer));
    assert!(
        own.linear.is_some() && own.guides.len() == 1,
        "on its layer both show"
    );
    // The same two in Layer, Ruler/Frame.
    let labels: Vec<String> = ["layer.ruler.link_layer", "layer.ruler.link_guide"]
        .iter()
        .map(|command| sue_paint::registry().label(command).unwrap())
        .collect();
    assert_eq!(labels, ["Link Ruler to Layer", "Link Guide to Ruler"]);
    shoot_canvas(&paint, "fgv1-ruler-linked");
}

/// 00:07:50: a vector line drawn with one brush takes another brush's
/// shape once picked with the Object tool, keeping its path; one undo
/// step.
#[test]
fn step_00_07_50_a_picked_vector_line_takes_another_brush_shape() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p6b-line-shape");
    paint.command("layer.new.vector");
    paint.settle(4);
    let layer = current_layer(&paint);
    paint.command("tool.pick.pen");
    paint.settle(2);
    let at = middle(&paint);
    stroke(&mut paint, at - vec2(80.0, 0.0), at + vec2(80.0, 10.0));
    let lines = |paint: &PaintDriver| {
        sue_paint::vector::lines(&sue_paint::vector::geometry(arena(paint), layer))
    };
    assert_eq!(lines(&paint).len(), 1, "a vector line");
    let drawn = lines(&paint)[0].clone();
    // The Object tool picks it.
    paint.command("tool.operation");
    paint.settle(2);
    click_at(&mut paint, at + vec2(0.0, 5.0));
    show_tool_property(&mut paint);
    let rows = names(&mut paint);
    shows_all(&rows, &["Brush shape", "Brush Size"], "the line's rows");
    // Another pen's brush (the second of the pen tool's sub tools).
    let other = sue_paint::tools::Tool::Pen.sub_tools()[1].name.clone();
    undo_pair(
        &mut paint,
        "brush shape",
        |paint| {
            click_named(paint, "Brush shape");
            paint.click_text(&other);
        },
        move |paint| {
            let now = &lines(paint)[0];
            now.tip != drawn.tip
                && now.samples.len() == drawn.samples.len()
                && now
                    .samples
                    .iter()
                    .zip(&drawn.samples)
                    .all(|(a, b)| a.at == b.at)
        },
    );
    assert!(ink(&paint, layer) > 0, "drawn again with the new brush");
}

/// 00:36:03: the Figure tool's rectangle has Roundness of corner,
/// Line/Fill and Aspect. Line and fill lays the inside in the other
/// color and the outline round it in the drawing color, as one step.
#[test]
fn step_00_36_03_figure_roundness_line_fill_and_aspect() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p6b-figure-rows");
    let rows = rows_of(&mut paint, "tool.pick.figure", "Rectangle");
    shows_all(
        &rows,
        &["Roundness", "Line/Fill", "Aspect", "Brush Size"],
        "Rectangle",
    );
    let polygon = rows_of(&mut paint, "tool.pick.figure", "Polygon");
    shows_all(
        &polygon,
        &["Roundness", "Number of corners", "Line/Fill"],
        "Polygon",
    );
    pick_sub(&mut paint, "tool.pick.figure", "Rectangle");
    show_tool_property(&mut paint);
    choose(&mut paint, "Line/Fill", "Create line and fill");
    let layer = current_layer(&paint);
    let at = middle(&paint);
    undo_pair(
        &mut paint,
        "line and fill",
        |paint| stroke(paint, at - vec2(70.0, 45.0), at + vec2(70.0, 45.0)),
        move |paint| {
            // Black outline round a white inside.
            let all = pixels(paint, layer);
            let painted = all.as_chunks::<4>().0.iter().filter(|p| p[3] == 255);
            let (mut dark, mut light) = (0, 0);
            for pixel in painted {
                if pixel[..3] == [0, 0, 0] {
                    dark += 1;
                } else if pixel[..3] == [255, 255, 255] {
                    light += 1;
                }
            }
            dark > 100 && light > dark
        },
    );
    shoot_canvas(&paint, "36-03-line-and-fill");
}

/// 00:36:22, 00:36:30, FGV-36: the saturated and speed line groups hold
/// several sub tools each, with their settings in Tool Property; a
/// dense saturated line lands on a layer named after it that keeps its
/// settings.
#[test]
fn step_00_36_22_fgv_36_effect_line_groups_and_settings() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p6b-effect-lines");
    let subs = sub_tools_of(&mut paint, "tool.pick.figure");
    shows_all(&subs, &["Saturated Line", "Speed Line"], "the groups");
    paint.click_text("Saturated Line");
    let saturated: Vec<String> = paint.texts.iter().map(|(text, _)| text.clone()).collect();
    shows_all(
        &saturated,
        &[
            "Scattered saturated line",
            "Dense saturated line",
            "Dark saturated line",
            "Burst",
        ],
        "the saturated line group",
    );
    paint.click_text("Speed Line");
    let speed: Vec<String> = paint.texts.iter().map(|(text, _)| text.clone()).collect();
    shows_all(
        &speed,
        &["Scattered speed line", "Dark speed line", "Rain"],
        "the speed line group",
    );
    let rows = rows_of(&mut paint, "tool.pick.figure", "Dense saturated line");
    shows_all(
        &rows,
        &[
            "Number of lines",
            "Length",
            "Length varies",
            "Make curve",
            "Grouping",
            "Gap from reference position",
            "Brush Size",
        ],
        "Dense saturated line",
    );
    let count = layers(&paint).len();
    let at = middle(&paint);
    undo_pair(
        &mut paint,
        "dense saturated lines",
        |paint| stroke(paint, at - vec2(60.0, 40.0), at + vec2(60.0, 40.0)),
        move |paint| layers(paint).len() == count + 1,
    );
    let layer = current_layer(&paint);
    let info = layers(&paint).last().unwrap().name.clone();
    assert_eq!(info, "Dense saturated line", "named after its sub tool");
    let spec = text_of(&paint, layer, sue_paint::manga::EFFECT_SPEC).unwrap();
    let lines = sue_paint::manga::EffectLines::decode(&spec).unwrap();
    assert_eq!(lines.count, 360, "the dense lines' count is kept");
    assert!(ink(&paint, layer) > 0);
    shoot_canvas(&paint, "36-22-dense-saturated");
    // Curved speed lines (Make curve) on a layer of their own.
    pick_sub(&mut paint, "tool.pick.figure", "Dark speed line");
    show_tool_property(&mut paint);
    paint.click_text("Make curve");
    let effects = of_class(&paint, sue_paint::manga::EFFECT);
    stroke(&mut paint, at - vec2(80.0, 0.0), at + vec2(80.0, 20.0));
    let speed = of_class(&paint, sue_paint::manga::EFFECT)
        .into_iter()
        .find(|id| !effects.contains(id))
        .expect("a speed line layer");
    let spec = text_of(&paint, speed, sue_paint::manga::EFFECT_SPEC).unwrap();
    let lines = sue_paint::manga::EffectLines::decode(&spec).unwrap();
    assert!(
        lines.speed && lines.style.curve != 0.0 && lines.style.group == 5,
        "{spec}"
    );
    shoot_canvas(&paint, "36-30-curved-speed");
}

/// 00:36:43, 00:37:10, 00:37:17, FG-24, FGV-37: a frame drawn with the
/// Frame Border tool is a folder of its own, masked to the panel, with a
/// paper background, a layer to draw on and the border; the Frame pen
/// draws one round freehand; Tool Property has How to add, Draw border
/// and the division's gutters, which the division keeps between panels.
#[test]
fn step_00_36_43_fg_24_fgv_37_frame_folder_pen_and_gutters() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p6b-frames");
    let subs = sub_tools_of(&mut paint, "tool.pick.frame");
    shows_all(
        &subs,
        &[
            "Rectangle Frame",
            "Polyline frame",
            "Frame pen",
            "Frame Border Division",
        ],
        "Frame Border",
    );
    let rows = rows_of(&mut paint, "tool.pick.frame", "Rectangle Frame");
    shows_all(
        &rows,
        &["How to add", "Draw border", "Brush Size", "Aspect"],
        "Rectangle Frame",
    );
    let page = picture(&paint);
    let at = middle(&paint);
    let count = layers(&paint).len();
    undo_pair(
        &mut paint,
        "rectangle frame",
        |paint| stroke(paint, at - vec2(120.0, 90.0), at + vec2(120.0, 90.0)),
        move |paint| layers(paint).len() == count + 4,
    );
    let world = arena(&paint);
    let folder = layers(&paint)
        .into_iter()
        .find(|info| sue_paint::doc::is_folder(world, info.id))
        .expect("a frame folder")
        .id;
    assert!(flag_of(&paint, folder, "frame.folder"));
    let members = sue_paint::doc::members(world, page, folder);
    let classes: Vec<&str> = members
        .iter()
        .map(|id| world.get_by_id(*id).unwrap().class())
        .collect();
    assert_eq!(members.len(), 3, "{classes:?}");
    assert!(classes.contains(&sue_paint::manga::FRAME), "{classes:?}");
    assert!(classes.contains(&sue_paint::doc::FILL_LAYER), "{classes:?}");
    // The mask shows the panel and hides the rest.
    let mask = sue_paint::doc::mask_of(world, folder).expect("the folder's mask");
    let frame = *members
        .iter()
        .find(|id| world.get_by_id(**id).unwrap().class() == sue_paint::manga::FRAME)
        .unwrap();
    let panel =
        sue_paint::manga::decode_panels(&text_of(&paint, frame, sue_paint::manga::PANELS).unwrap());
    let [x0, y0, x1, y1] = panel[0].iter().fold(
        [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
        |[a, b, c, d], p| [a.min(p[0]), b.min(p[1]), c.max(p[0]), d.max(p[1])],
    );
    let cover = sue_paint::doc::raster(world, mask).unwrap();
    let mask_at = |x: f32, y: f32| sue_paint::doc::cover(cover, x as i64, y as i64);
    assert!(
        mask_at((x0 + x1) / 2.0, (y0 + y1) / 2.0) > 0.99,
        "inside shows"
    );
    assert!(mask_at(x0 - 5.0, y0 - 5.0) < 0.01, "outside is hidden");
    shoot_canvas(&paint, "36-43-frame-folder");
    // The division keeps the vertical gutter between the two panels.
    let gutters = rows_of(&mut paint, "tool.pick.frame", "Frame Border Division");
    shows_all(
        &gutters,
        &["Gap across", "Gap down"],
        "Frame Border Division",
    );
    stroke(&mut paint, at - vec2(0.0, 110.0), at + vec2(0.0, 110.0));
    let halves =
        sue_paint::manga::decode_panels(&text_of(&paint, frame, sue_paint::manga::PANELS).unwrap());
    assert_eq!(halves.len(), 2, "divided");
    let right_of = |panel: &[[f32; 2]]| panel.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
    let left_of = |panel: &[[f32; 2]]| panel.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
    let (a, b) = if left_of(&halves[0]) < left_of(&halves[1]) {
        (&halves[0], &halves[1])
    } else {
        (&halves[1], &halves[0])
    };
    let gap = left_of(b) - right_of(a);
    assert!((gap - 20.0).abs() < 2.0, "the vertical gutter: {gap}");
    // The mask follows the panels: the gutter is hidden now.
    let cover = sue_paint::doc::raster(arena(&paint), mask).unwrap();
    let gutter_x = (right_of(a) + left_of(b)) / 2.0;
    assert!(sue_paint::doc::cover(cover, gutter_x as i64, ((y0 + y1) / 2.0) as i64) < 0.01);
    shoot_canvas(&paint, "37-17-frame-divided");
    // The Frame pen: a frame round a freehand loop, in a folder too.
    pick_sub(&mut paint, "tool.pick.frame", "Frame pen");
    let folders = |paint: &PaintDriver| {
        layers(paint)
            .into_iter()
            .filter(|info| flag_of(paint, info.id, "frame.folder"))
            .count()
    };
    let before = folders(&paint);
    let loop_at = at + vec2(0.0, 150.0);
    let path: Vec<Pos2> = (0..=24)
        .map(|step| {
            let t = step as f32 / 24.0 * std::f32::consts::TAU;
            loop_at + vec2(t.cos() * 60.0, t.sin() * 35.0)
        })
        .collect();
    let frames_before = of_class(&paint, sue_paint::manga::FRAME);
    trace(&mut paint, &path);
    assert_eq!(folders(&paint), before + 1, "a frame drawn with the pen");
    let pen_frame = of_class(&paint, sue_paint::manga::FRAME)
        .into_iter()
        .find(|id| !frames_before.contains(id))
        .expect("the pen's frame border layer");
    let outline = sue_paint::manga::decode_panels(
        &text_of(&paint, pen_frame, sue_paint::manga::PANELS).unwrap(),
    );
    assert!(
        outline[0].len() > 8,
        "a freehand outline: {}",
        outline[0].len()
    );
    shoot_canvas(&paint, "37-10-frame-pen");
}

/// 00:37:26, FGV-38: the Ruler tool's Guide lays a guide a pen line
/// follows; Create at editing layer off lays a ruler on a new layer of
/// its own; the linear ruler has a scale and the curve ruler three
/// kinds.
#[test]
fn step_00_37_26_fgv_38_guide_ruler_and_ruler_settings() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p6b-guide");
    let subs = sub_tools_of(&mut paint, "tool.pick.ruler");
    shows_all(&subs, &["Guide", "Linear Ruler", "Curve Ruler"], "Ruler");
    let rows = rows_of(&mut paint, "tool.pick.ruler", "Guide");
    shows_all(&rows, &["Create at editing layer"], "Guide");
    let page = picture(&paint);
    let layer = current_layer(&paint);
    let at = middle(&paint);
    undo_pair(
        &mut paint,
        "guide",
        |paint| stroke(paint, at + vec2(-100.0, 0.0), at + vec2(100.0, 3.0)),
        move |paint| {
            text_of(paint, page, sue_paint::rulers::GUIDES)
                .is_some_and(|text| text.starts_with("h:"))
        },
    );
    // A pen line started on the guide runs along it.
    paint.command("tool.pick.pen");
    paint.settle(2);
    let before = pixels(&paint, layer);
    stroke(&mut paint, at + vec2(-40.0, 2.0), at + vec2(40.0, 30.0));
    let after = pixels(&paint, layer);
    let width = width_of(&paint, layer);
    let rows_touched: Vec<usize> = before
        .as_chunks::<4>()
        .0
        .iter()
        .zip(after.as_chunks::<4>().0)
        .enumerate()
        .filter(|(_, (was, now))| was[3] != now[3])
        .map(|(index, _)| index / width)
        .collect();
    let tall = rows_touched.iter().max().unwrap() - rows_touched.iter().min().unwrap();
    assert!(tall < 15, "the line follows the guide: {tall} rows tall");
    shoot_canvas(&paint, "37-26-guide");
    undo(&mut paint);
    // Create at editing layer off: a new layer the ruler belongs to.
    pick_sub(&mut paint, "tool.pick.ruler", "Linear Ruler");
    show_tool_property(&mut paint);
    let linear_rows = names(&mut paint);
    shows_all(&linear_rows, &["Scale", "Step of angle"], "Linear Ruler");
    paint.click_text("Scale");
    paint.click_text("Create at editing layer");
    let count = layers(&paint).len();
    undo_pair(
        &mut paint,
        "ruler on a new layer",
        |paint| stroke(paint, at + vec2(-90.0, -60.0), at + vec2(90.0, -60.0)),
        move |paint| {
            let arena = arena(paint);
            let owner = arena
                .get_by_id(page)
                .ok()
                .and_then(|entity| entity.property(sue_paint::rulers::LAYER))
                .and_then(sue_core::Value::as_entity);
            sue_paint::doc::layers(arena, page).len() == count + 1
                && owner
                    == sue_paint::doc::layers(arena, page)
                        .last()
                        .map(|info| info.id)
                && sue_paint::rulers::Rulers::of(arena, page).linear_scale == Some(10.0)
        },
    );
    // The curve ruler's kinds: a quadratic Bezier lays on its third click.
    pick_sub(&mut paint, "tool.pick.ruler", "Curve Ruler");
    show_tool_property(&mut paint);
    choose(&mut paint, "Curve", "Quadratic Bezier");
    paint.click_text("Create at editing layer");
    let paths = |paint: &PaintDriver| {
        sue_paint::rulers::Rulers::of(arena(paint), page)
            .paths
            .len()
    };
    let before = paths(&paint);
    for point in [vec2(-100.0, 80.0), vec2(0.0, -20.0), vec2(100.0, 80.0)] {
        click_at(&mut paint, at + point);
    }
    assert_eq!(paths(&paint), before + 1, "a Bezier curve ruler");
    let curve = sue_paint::rulers::Rulers::of(arena(&paint), page)
        .paths
        .last()
        .unwrap()
        .clone();
    assert!(curve.points.len() > 20, "sampled along the curve");
    shoot_canvas(&paint, "37-27-curve-and-scale");
}

/// 00:37:42, FGV-39: the Text tool's Style row makes words bold, italic,
/// underlined or struck through; the style is kept on the text layer.
#[test]
fn step_00_37_42_fgv_39_text_style() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p6b-text-style");
    let rows = rows_of(&mut paint, "tool.pick.text", "Text");
    shows_all(
        &rows,
        &["Style", "Bold", "Italic", "Underline", "Strike through"],
        "Text",
    );
    show_tool_property(&mut paint);
    click_named(&mut paint, "Bold");
    click_named(&mut paint, "Underline");
    let count = layers(&paint).len();
    let at = middle(&paint);
    click_at(&mut paint, at - vec2(80.0, 20.0));
    paint.step(vec![egui::Event::Text("Bold words".into())]);
    paint.settle(3);
    paint.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    paint.settle(4);
    assert_eq!(
        layers(&paint).len(),
        count + 1,
        "a text layer: {}",
        said(&paint)
    );
    let layer = current_layer(&paint);
    assert!(flag_of(&paint, layer, "text.bold") && flag_of(&paint, layer, "text.underline"));
    assert!(!flag_of(&paint, layer, "text.italic") && !flag_of(&paint, layer, "text.strike"));
    let styled = ink(&paint, layer);
    assert!(styled > 0);
    shoot_canvas(&paint, "37-42-text-bold-underline");
    // The same words plain hold less ink.
    click_named(&mut paint, "Bold");
    click_named(&mut paint, "Underline");
    click_at(&mut paint, at + vec2(-80.0, 80.0));
    paint.step(vec![egui::Event::Text("Bold words".into())]);
    paint.settle(3);
    paint.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    paint.settle(4);
    let plain = current_layer(&paint);
    assert_ne!(plain, layer);
    assert!(ink(&paint, plain) < styled, "styled words hold more ink");
    // Undo takes the styled words away again, redo brings them back.
    undo(&mut paint);
    undo(&mut paint);
    assert_eq!(layers(&paint).len(), count);
    redo(&mut paint);
    assert!(flag_of(&paint, layer, "text.bold"));
}

/// 00:37:52, FGV-40 and the crash tips' balloon colors (V-14): a
/// balloon's Line color and Fill color (the drawing color, the other one
/// or its own), its edge line on or off, and How to add.
#[test]
fn step_00_37_52_fgv_40_balloon_colors_edge_and_how_to_add() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p6b-balloons");
    let rows = rows_of(&mut paint, "tool.pick.balloon", "Ellipse Balloon");
    shows_all(
        &rows,
        &[
            "How to add",
            "Edge line",
            "Line width",
            "Line color",
            "Fill color",
        ],
        "Ellipse Balloon",
    );
    let at = middle(&paint);
    let balloons = |paint: &PaintDriver| of_class(paint, sue_paint::manga::BALLOON);
    // The first balloon: black on white, on a new balloon layer.
    undo_pair(
        &mut paint,
        "balloon",
        |paint| stroke(paint, at + vec2(-140.0, -80.0), at + vec2(-40.0, -20.0)),
        move |paint| balloons(paint).len() == 1,
    );
    let first = balloons(&paint)[0];
    // Undo and redo leave another layer picked: the balloon layer again.
    click_layer_row(&mut paint, "Balloon");
    assert_eq!(
        text_of(&paint, first, "balloon.line_color").as_deref(),
        Some("#000000")
    );
    assert_eq!(
        text_of(&paint, first, "balloon.fill_color").as_deref(),
        Some("#ffffff")
    );
    // Added to the selected balloon layer.
    stroke(&mut paint, at + vec2(40.0, -80.0), at + vec2(140.0, -20.0));
    assert_eq!(balloons(&paint).len(), 1, "added to the selected layer");
    // The line in the other color: a balloon layer of its own colors.
    show_tool_property(&mut paint);
    choose(&mut paint, "Line color", "Other color");
    stroke(&mut paint, at + vec2(-140.0, 20.0), at + vec2(-40.0, 80.0));
    let all = balloons(&paint);
    assert_eq!(all.len(), 2, "a layer for the new colors");
    assert_eq!(
        text_of(&paint, all[1], "balloon.line_color").as_deref(),
        Some("#ffffff")
    );
    // No edge: only the fill.
    show_tool_property(&mut paint);
    paint.click_text("Edge line");
    stroke(&mut paint, at + vec2(40.0, 20.0), at + vec2(140.0, 80.0));
    let bare = *balloons(&paint).last().unwrap();
    let line = entity(&paint, bare)
        .property(sue_paint::manga::LINE)
        .and_then(sue_core::Value::as_f64);
    assert_eq!(line, Some(0.0), "no edge");
    let painted: Vec<[u8; 4]> = pixels(&paint, bare)
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] == 255)
        .copied()
        .collect();
    assert!(!painted.is_empty() && painted.iter().all(|p| p[..3] == [255, 255, 255]));
    shoot_canvas(&paint, "37-52-balloons");
    // How to add, Create new layer: a new layer even on a balloon layer.
    show_tool_property(&mut paint);
    paint.click_text("Edge line");
    choose(&mut paint, "Line color", "Drawing color");
    choose(&mut paint, "How to add", "Create new layer");
    let count = balloons(&paint).len();
    stroke(&mut paint, at + vec2(-40.0, 100.0), at + vec2(40.0, 140.0));
    assert_eq!(balloons(&paint).len(), count + 1, "a new balloon layer");
}

/// 00:38:12, FGV-41: Pinch vector line's Fix end has three choices, Add
/// control point and Connect lines; Redraw vector line has Fix end,
/// Connect lines and Simplify; the Remove dust group holds Fill leftover
/// and Select garbage.
#[test]
fn step_00_38_12_fgv_41_pinch_redraw_and_the_dust_group() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p6b-correct");
    let pinch = rows_of(&mut paint, "tool.correct_line", "Pinch vector line");
    shows_all(
        &pinch,
        &[
            "Fix end",
            "Add control point",
            "Connect lines",
            "Pinch level",
            "Effect range",
        ],
        "Pinch",
    );
    let redraw = rows_of(&mut paint, "tool.correct_line", "Redraw vector line");
    shows_all(
        &redraw,
        &["Fix end", "Connect lines", "Simplify", "Stabilization"],
        "Redraw",
    );
    let subs = sub_tools_of(&mut paint, "tool.correct_line");
    shows_all(&subs, &["Remove dust"], "the dust group");
    paint.click_text("Remove dust");
    let dust: Vec<String> = paint.texts.iter().map(|(text, _)| text.clone()).collect();
    shows_all(
        &dust,
        &["Fill leftover", "Select garbage"],
        "the dust group",
    );
    // Pinch with Add control point: the line has a control point more.
    paint.command("layer.new.vector");
    paint.settle(4);
    let layer = current_layer(&paint);
    paint.command("tool.pick.pen");
    paint.settle(2);
    let at = middle(&paint);
    stroke(&mut paint, at - vec2(100.0, 0.0), at + vec2(100.0, 0.0));
    let controls = |paint: &PaintDriver| {
        sue_paint::vector::lines(&sue_paint::vector::geometry(arena(paint), layer))
            .first()
            .map_or(0, |line| line.controls().len())
    };
    let before = controls(&paint);
    pick_sub(&mut paint, "tool.correct_line", "Pinch vector line");
    show_tool_property(&mut paint);
    paint.click_text("Add control point");
    undo_pair(
        &mut paint,
        "pinch",
        |paint| stroke(paint, at, at + vec2(0.0, -40.0)),
        move |paint| controls(paint) > before,
    );
    // Fill leftover: a hole cleared in a filled layer fills again.
    paint.command("layer.new.fill");
    paint.settle(4);
    let filled = current_layer(&paint);
    // At 100 % a few points of selection are a few pixels of hole.
    paint.command("view.actual");
    paint.settle(4);
    let at = middle(&paint);
    paint.command("tool.selection");
    paint.settle(2);
    let hole = at + vec2(60.0, 60.0);
    stroke(&mut paint, hole, hole + vec2(6.0, 6.0));
    paint.command("edit.clear");
    paint.settle(4);
    paint.command("select.none");
    paint.settle(2);
    let clear = |paint: &PaintDriver| {
        pixels(paint, filled)
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] == 0)
            .count()
    };
    assert!(clear(&paint) > 0, "a hole");
    pick_sub(&mut paint, "tool.correct_line", "Fill leftover");
    undo_pair(
        &mut paint,
        "fill leftover",
        |paint| click_at(paint, at + vec2(-120.0, -120.0)),
        move |paint| clear(paint) == 0,
    );
    // Select garbage: a dot on a clear layer becomes the selection.
    paint.command("layer.new");
    paint.settle(4);
    paint.command("tool.selection");
    paint.settle(2);
    let speck = at + vec2(30.0, 90.0);
    stroke(&mut paint, speck, speck + vec2(5.0, 5.0));
    paint.command("edit.fill");
    paint.settle(4);
    paint.command("select.none");
    paint.settle(2);
    pick_sub(&mut paint, "tool.correct_line", "Select garbage");
    let document = arena_bytes(&paint);
    click_at(&mut paint, at + vec2(-120.0, -120.0));
    assert!(said(&paint).contains("specks selected"), "{}", said(&paint));
    assert!(
        arena_bytes(&paint) == document,
        "the layer is left as it was"
    );
}

/// FG-24 (00:36:43): with How to add on Create new frame folder, frames
/// drawn one after another each get a folder of their own, although the
/// first one's frame border layer is the one picked when the second is
/// drawn; on Add to frame folder the next frame joins the picked one.
#[test]
fn step_00_36_43_fg_24_frames_drawn_in_a_row_each_get_a_folder() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut paint = PaintDriver::new("p6b-frames-in-a-row");
    pick_sub(&mut paint, "tool.pick.frame", "Rectangle Frame");
    let folders = |paint: &PaintDriver| {
        layers(paint)
            .into_iter()
            .filter(|info| flag_of(paint, info.id, "frame.folder"))
            .count()
    };
    let at = middle(&paint);
    stroke(&mut paint, at - vec2(200.0, 120.0), at - vec2(20.0, 10.0));
    paint.settle(4);
    assert_eq!(folders(&paint), 1, "the first frame");
    assert_eq!(of_class(&paint, sue_paint::manga::FRAME).len(), 1);
    undo_pair(
        &mut paint,
        "a second frame",
        |paint| stroke(paint, at + vec2(20.0, 10.0), at + vec2(200.0, 120.0)),
        |paint| folders(paint) == 2,
    );
    assert_eq!(of_class(&paint, sue_paint::manga::FRAME).len(), 2);
    // Add to frame folder: the next frame joins the picked frame layer.
    let rows = rows_of(&mut paint, "tool.pick.frame", "Rectangle Frame");
    shows_all(&rows, &["How to add"], "Rectangle Frame");
    choose(&mut paint, "How to add", "Add to frame border layer");
    let panels_of = |paint: &PaintDriver| {
        of_class(paint, sue_paint::manga::FRAME)
            .into_iter()
            .map(|id| {
                sue_paint::manga::decode_panels(
                    &text_of(paint, id, sue_paint::manga::PANELS).unwrap_or_default(),
                )
                .len()
            })
            .sum::<usize>()
    };
    let before = panels_of(&paint);
    stroke(
        &mut paint,
        at + vec2(-200.0, 140.0),
        at + vec2(-40.0, 220.0),
    );
    paint.settle(4);
    assert_eq!(folders(&paint), 2, "no new folder");
    assert_eq!(
        of_class(&paint, sue_paint::manga::FRAME).len(),
        2,
        "no new layer"
    );
    assert_eq!(panels_of(&paint), before + 1, "the panel joined");
}
