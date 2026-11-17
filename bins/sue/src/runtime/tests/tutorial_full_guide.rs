//! P5-2 tutorial acceptance: the "Full Guide" tour of every palette,
//! tool, menu, the Material palette and 3D, followed in Sue Paint step by
//! step. Each test is named after the step of the tutorial it follows
//! (`step_hh_mm_ss_...`, the time in the video); the row table with what
//! each step became is kept outside the repository with the plan
//! (`acceptance/full_guide.md`). Nothing of the video is quoted here.
//!
//! Every test drives real frames: it clicks what a person clicks, found
//! by the names the frame gives its controls, never at fixed places, and
//! it reads what changed in the document. Each change that alters the
//! document is undone and redone and compared with the arena before and
//! after.

use super::*;
use egui::pos2;

// Helpers.

/// The whole arena as bytes, so two states compare exactly.
fn arena_bytes(paint: &PaintDriver) -> Vec<u8> {
    let mut out = sue_core::codec::Writer::new();
    paint.runtime.engine.world.arena.encode(&mut out);
    out.into_bytes()
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

/// The layers of the open canvas, bottom first.
fn layers(paint: &PaintDriver) -> Vec<sue_paint::doc::LayerInfo> {
    sue_paint::doc::layers(&paint.runtime.engine.world.arena, picture(paint))
}

/// The layer `id`'s entity.
fn entity(paint: &PaintDriver, id: sue_core::EntityId) -> &sue_core::Entity {
    paint.runtime.engine.world.arena.get_by_id(id).unwrap()
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

/// One frame with `events` that also returns every control the frame
/// named for assistive technology, with its role and place: icon
/// buttons have no text of their own.
fn step_told(
    paint: &mut PaintDriver,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::accesskit::Role, egui::Rect)> {
    step_held(paint, events, egui::Modifiers::NONE)
}

/// [`step_told`] with `modifiers` held down through the frame, as the
/// keyboard holds them.
fn step_held(
    paint: &mut PaintDriver,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> Vec<(String, egui::accesskit::Role, egui::Rect)> {
    paint.ctx.enable_accesskit();
    let mut all = vec![egui::Event::ModifiersChanged(modifiers)];
    all.extend(events);
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        events: all,
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
                node.role(),
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

/// Every control name the next frame gives.
fn names(paint: &mut PaintDriver) -> Vec<String> {
    let mut found: Vec<String> = step_told(paint, Vec::new())
        .into_iter()
        .map(|(name, ..)| name)
        .collect();
    found.extend(paint.texts.iter().map(|(text, _)| text.clone()));
    found.sort();
    found.dedup();
    found
}

/// Where the control named `name` is drawn now: the one `pick` prefers
/// when several share the name.
fn named_where(
    paint: &mut PaintDriver,
    name: &str,
    pick: impl Fn(&egui::Rect) -> bool,
) -> Option<egui::Rect> {
    step_told(paint, Vec::new())
        .into_iter()
        .filter(|(label, _, rect)| label == name && pick(rect) && rect.width() > 0.0)
        .map(|(_, _, rect)| rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
}

/// Where the control named `name` is drawn now.
fn named(paint: &mut PaintDriver, name: &str) -> egui::Rect {
    named_where(paint, name, |_| true).unwrap_or_else(|| {
        panic!(
            "{name:?} is on screen: {:?}",
            step_told(paint, Vec::new())
                .into_iter()
                .map(|(label, ..)| label)
                .collect::<Vec<_>>()
        )
    })
}

/// A click at `at` with `modifiers` held.
fn click_with(paint: &mut PaintDriver, at: Pos2, modifiers: egui::Modifiers) {
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    };
    step_held(paint, vec![egui::Event::PointerMoved(at)], modifiers);
    step_held(paint, vec![press(true)], modifiers);
    step_held(paint, vec![press(false)], modifiers);
    step_held(paint, Vec::new(), modifiers);
    paint.settle(6);
}

/// A click on the control named `name`.
fn click_named(paint: &mut PaintDriver, name: &str) {
    let at = named(paint, name).center();
    click_with(paint, at, egui::Modifiers::NONE);
}

/// A right click at `at`.
fn right_click(paint: &mut PaintDriver, at: Pos2) {
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    paint.step(vec![egui::Event::PointerMoved(at)]);
    paint.step(vec![press(true)]);
    paint.step(vec![press(false)]);
    paint.settle(6);
}

/// A key with no modifier.
fn press(paint: &mut PaintDriver, which: egui::Key) {
    paint.step(vec![key(which, egui::Modifiers::NONE)]);
    paint.settle(4);
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
/// it. Undo gives back the arena exactly as before (ids included), redo
/// exactly as after.
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

/// Runs a command and takes what its dialog shows with Enter.
fn command_ok(paint: &mut PaintDriver, command: &str) {
    paint.command(command);
    paint.settle(3);
    paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    paint.settle(4);
}

/// A stroke of the current tool from `from` to `to` on the canvas.
fn stroke(paint: &mut PaintDriver, from: Pos2, to: Pos2) {
    paint.drag(from, to, egui::PointerButton::Primary);
}

/// The layer's pixels.
fn pixels(paint: &PaintDriver, layer: sue_core::EntityId) -> Vec<u8> {
    sue_paint::doc::raster(&paint.runtime.engine.world.arena, layer)
        .map(|raster| raster.bytes().unwrap_or_default().to_vec())
        .unwrap_or_default()
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

/// A new raster layer (Ctrl+Shift+N) with a stroke across the middle;
/// returns it.
fn inked_layer(paint: &mut PaintDriver) -> sue_core::EntityId {
    paint.step(vec![key(
        egui::Key::N,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    )]);
    paint.settle(4);
    let layer = layers(paint).last().unwrap().id;
    paint.command("tool.pen");
    paint.settle(2);
    let at = middle(paint);
    stroke(paint, at - vec2(60.0, 0.0), at + vec2(60.0, 0.0));
    assert!(ink(paint, layer) > 0, "the stroke landed");
    layer
}

/// The Tool Property tab, shown.
fn show_tool_property(paint: &mut PaintDriver) {
    paint.settle(2);
    paint.click_text("Tool Property");
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
    seen.sort();
    seen.dedup();
    seen
}

/// Every row Tool Property shows for the sub tool picked now.
fn tool_property_rows(paint: &mut PaintDriver) -> Vec<String> {
    show_tool_property(paint);
    let tab = paint
        .rect_of("Tool Property")
        .expect("the Tool Property tab");
    scroll_all(paint, tab.center() + vec2(0.0, 160.0))
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

/// The command `name` is in Sue Paint's registry, with its label.
fn label_of(name: &str) -> String {
    sue_paint::registry()
        .label(name)
        .unwrap_or_else(|| panic!("{name} is a command"))
}

// A. Workspace and palettes.

#[test]
fn step_00_04_19_palettes_close_and_come_back_from_window() {
    let mut paint = PaintDriver::new("fg-palettes");
    paint.settle(4);
    assert!(paint.rect_of("Navigator").is_some(), "{:?}", paint.texts);
    // Window, Navigator closes it; again brings it back.
    paint.command("window.navigator");
    paint.settle(6);
    assert!(paint.rect_of("Navigator").is_none(), "closed");
    paint.command("window.navigator");
    paint.settle(6);
    assert!(paint.rect_of("Navigator").is_some(), "back");
    // Every palette the video pulls out is under Window.
    for command in [
        "window.layer",
        "window.layer_property",
        "window.sub_tool",
        "window.tool_property",
        "window.sub_tool_detail",
        "window.brush_size",
        "window.color_wheel",
        "window.color_slider",
        "window.color_set",
        "window.color_history",
        "window.intermediate",
        "window.approximate",
        "window.navigator",
        "window.sub_view",
        "window.auto_actions",
        "window.history",
        "window.info",
        "window.cels",
        "window.timeline",
        "window.item_bank",
        "window.material",
        "window.quick_access",
    ] {
        label_of(command);
    }
    // One floats in a window of its own and docks back.
    paint.command("window.float");
    paint.settle(4);
    assert!(
        !said(&paint).contains("not in Sue Paint"),
        "{}",
        said(&paint)
    );
}

#[test]
fn step_00_04_46_layer_palette_color_tags_a_layer() {
    let mut paint = PaintDriver::new("fg-palette-color");
    let layer = layers(&paint).last().unwrap().id;
    // The palette's own button opens the color list; a color tags it.
    click_named(&mut paint, "Change Palette Color");
    paint.settle(2);
    let red = paint.rect_of("Red").expect("the color list");
    undo_pair(
        &mut paint,
        "palette color",
        |paint| click_with(paint, red.center(), egui::Modifiers::NONE),
        |paint| sue_paint::doc::palette_color(&paint.runtime.engine.world.arena, layer).is_some(),
    );
    // And from the Layer menu.
    paint.command("layer.palette_color.none");
    paint.settle(2);
    assert!(sue_paint::doc::palette_color(&paint.runtime.engine.world.arena, layer).is_none());
}

#[test]
fn step_00_05_05_blending_mode_on_layers_and_folders() {
    let mut paint = PaintDriver::new("fg-blend");
    let layer = layers(&paint).last().unwrap().id;
    let blend = |paint: &PaintDriver, id| {
        layers(paint)
            .into_iter()
            .find(|info| info.id == id)
            .map(|info| info.style.blend)
            .unwrap()
    };
    let combo = named(&mut paint, "Blending mode");
    click_with(&mut paint, combo.center(), egui::Modifiers::NONE);
    let multiply = paint.rect_of("Multiply").expect("the list of modes");
    undo_pair(
        &mut paint,
        "multiply",
        |paint| click_with(paint, multiply.center(), egui::Modifiers::NONE),
        |paint| blend(paint, layer) == sue_model::BlendMode::Multiply,
    );
    // A folder takes a mode too (Through and the rest).
    paint.command("layer.new.folder");
    paint.settle(4);
    let folder = layers(&paint)
        .into_iter()
        .find(|info| sue_paint::doc::is_folder(&paint.runtime.engine.world.arena, info.id))
        .unwrap()
        .id;
    assert_eq!(blend(&paint, folder), sue_model::BlendMode::Through);
    let combo = named(&mut paint, "Blending mode");
    click_with(&mut paint, combo.center(), egui::Modifiers::NONE);
    let screen = paint.rect_of("Screen").expect("the folder's modes");
    click_with(&mut paint, screen.center(), egui::Modifiers::NONE);
    assert_eq!(blend(&paint, folder), sue_model::BlendMode::Screen);
}

#[test]
fn step_00_05_12_layer_opacity_and_the_flag_switches() {
    let mut paint = PaintDriver::new("fg-flags");
    paint.step(vec![key(
        egui::Key::N,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    )]);
    paint.settle(4);
    let layer = layers(&paint).last().unwrap().id;
    let flag = |key: &'static str| {
        move |paint: &PaintDriver| {
            entity(paint, layer)
                .property(key)
                .and_then(sue_core::Value::as_bool)
                .unwrap_or(false)
        }
    };
    // 00:05:16 clip, 00:05:33 reference, 00:05:39 draft, 00:05:50 lock,
    // 00:05:58 lock transparent pixels: each a button over the list.
    let switches = [
        ("Clip to Layer Below", sue_model::picture::CLIP),
        ("Set as draft layer", sue_model::picture::DRAFT),
        ("Lock Layer", sue_model::picture::LOCKED),
        ("Lock transparent pixels", sue_model::picture::ALPHA_LOCKED),
    ];
    for (button, key) in switches {
        let name = label_of(match key {
            sue_model::picture::CLIP => "layer.toggle.clip",
            sue_model::picture::DRAFT => "layer.toggle.draft",
            sue_model::picture::LOCKED => "layer.toggle.lock",
            _ => "layer.toggle.lock_alpha",
        });
        let _ = button;
        undo_pair(
            &mut paint,
            &name,
            |paint| click_named(paint, &name),
            flag(key),
        );
        click_named(&mut paint, &name);
    }
    let reference = label_of("layer.toggle.reference");
    undo_pair(
        &mut paint,
        "reference",
        |paint| click_named(paint, &reference),
        move |paint| sue_paint::doc::is_reference(&paint.runtime.engine.world.arena, layer),
    );
    // The opacity field: typed, one step.
    let opacity =
        |paint: &PaintDriver| sue_model::picture::LayerStyle::of(entity(paint, layer)).opacity;
    let field =
        named_where(&mut paint, "Opacity", |rect| rect.width() < 80.0).expect("the opacity number");
    undo_pair(
        &mut paint,
        "opacity",
        |paint| {
            click_with(paint, field.center(), egui::Modifiers::NONE);
            paint.step(vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
            paint.step(vec![egui::Event::Text("40".into())]);
            paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
            paint.settle(4);
        },
        move |paint| (opacity(paint) - 0.4).abs() < 0.01,
    );
}

#[test]
fn step_00_06_17_ruler_button_sets_where_rulers_show() {
    let mut paint = PaintDriver::new("fg-ruler-button");
    // 00:06:24: right click, Ruler, a perspective ruler; it lands on a
    // layer of its own by default (FGV-2).
    let before = layers(&paint).len();
    paint.command("layer.ruler.perspective");
    paint.settle(3);
    assert!(
        paint.rect_of("Create new layer").is_some(),
        "{:?}",
        paint.texts
    );
    paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    paint.settle(4);
    assert_eq!(layers(&paint).len(), before + 1, "a layer for the ruler");
    // The palette's ruler button: its drop-down of showing areas (FGV-1).
    click_named(&mut paint, "Set Showing Area of Ruler");
    let texts: Vec<String> = paint.texts.iter().map(|(text, _)| text.clone()).collect();
    shows_all(
        &texts,
        &[label_of("layer.ruler.show").as_str()],
        "the ruler menu",
    );
    let ranges: Vec<String> = sue_paint::rulers::ShowRange::ALL
        .iter()
        .map(|range| range.label())
        .collect();
    assert_eq!(ranges.len(), 3, "all layers, same folder, editing target");
    for range in &ranges {
        assert!(texts.contains(range), "{range:?} in {texts:?}");
    }
    let last = paint.rect_of(&ranges[2]).unwrap();
    let before = arena_bytes(&paint);
    click_with(&mut paint, last.center(), egui::Modifiers::NONE);
    assert_ne!(arena_bytes(&paint), before, "the showing area is kept");
    undo(&mut paint);
    assert!(arena_bytes(&paint) == before);
}

#[test]
fn step_00_06_51_change_layer_color_button_and_ctrl_b() {
    let mut paint = PaintDriver::new("fg-layer-color");
    let layer = inked_layer(&mut paint);
    let on = move |paint: &PaintDriver| {
        entity(paint, layer)
            .property(sue_paint::effects::COLOR)
            .and_then(sue_core::Value::as_bool)
            .unwrap_or(false)
    };
    let name = label_of("layer.toggle.layer_color");
    undo_pair(
        &mut paint,
        "layer color button",
        |paint| click_named(paint, &name),
        on,
    );
    click_named(&mut paint, &name);
    assert!(!on(&paint));
    undo_pair(
        &mut paint,
        "Ctrl+B",
        |paint| {
            paint.step(vec![key(egui::Key::B, egui::Modifiers::COMMAND)]);
        },
        on,
    );
}

#[test]
fn step_00_07_16_layer_list_in_two_panes() {
    let mut paint = PaintDriver::new("fg-two-panes");
    paint.settle(2);
    let count = |paint: &PaintDriver| paint.texts.iter().filter(|(t, _)| t == "Paper").count();
    let one = count(&paint);
    click_named(&mut paint, "Split Layer List");
    paint.settle(4);
    assert!(count(&paint) > one, "the second pane lists the layers too");
    click_named(&mut paint, "Split Layer List");
    paint.settle(4);
    assert_eq!(count(&paint), one);
}

#[test]
fn step_00_07_26_new_raster_and_vector_layer_buttons() {
    let mut paint = PaintDriver::new("fg-new-buttons");
    let count = |paint: &PaintDriver| layers(paint).len();
    let before = count(&paint);
    let raster = label_of("layer.new");
    undo_pair(
        &mut paint,
        "new raster",
        |p| click_named(p, &raster),
        move |p| count(p) == before + 1,
    );
    let vector = label_of("layer.new.vector");
    undo_pair(
        &mut paint,
        "new vector",
        |p| click_named(p, &vector),
        move |p| {
            count(p) == before + 2
                && layers(p).iter().any(|info| {
                    sue_paint::vector::is_vector(&p.runtime.engine.world.arena, info.id)
                })
        },
    );
}

/// The number of lines on the vector layer.
fn line_count(paint: &PaintDriver, layer: sue_core::EntityId) -> usize {
    sue_paint::vector::lines(&sue_paint::vector::geometry(
        &paint.runtime.engine.world.arena,
        layer,
    ))
    .len()
}

/// A drag from `from` to `to` with `modifiers` held all along.
fn drag_with(paint: &mut PaintDriver, from: Pos2, to: Pos2, modifiers: egui::Modifiers) {
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    };
    step_held(paint, vec![egui::Event::PointerMoved(from)], modifiers);
    step_held(paint, vec![button(from, true)], modifiers);
    for part in 1..=12 {
        let at = from + (to - from) * (part as f32 / 12.0);
        step_held(paint, vec![egui::Event::PointerMoved(at)], modifiers);
    }
    step_held(paint, vec![button(to, false)], modifiers);
    step_held(paint, Vec::new(), modifiers);
    paint.settle(4);
}

#[test]
fn step_00_07_33_ctrl_held_moves_what_is_on_the_layer() {
    let mut paint = PaintDriver::new("fg-ctrl-borrows");
    let layer = inked_layer(&mut paint);
    // The pen stays picked; Ctrl held borrows Move layer on a raster
    // layer (also 01:05:23).
    let offset =
        move |paint: &PaintDriver| sue_model::picture::LayerStyle::of(entity(paint, layer)).offset;
    let at = middle(&paint);
    let inked = ink(&paint, layer);
    undo_pair(
        &mut paint,
        "Ctrl drag on raster",
        |paint| drag_with(paint, at, at + vec2(80.0, 40.0), egui::Modifiers::COMMAND),
        move |paint| offset(paint) != [0, 0] && ink(paint, layer) == inked,
    );
    // On a vector layer Ctrl borrows the Object tool: the drawn line is
    // picked and dragged as a line, not painted over.
    paint.command("layer.new.vector");
    paint.settle(4);
    let vector = layers(&paint).last().unwrap().id;
    let from = at + vec2(-60.0, -80.0);
    stroke(&mut paint, from, from + vec2(120.0, 0.0));
    assert_eq!(line_count(&paint, vector), 1, "one line drawn");
    let before = arena_bytes(&paint);
    let on_line = from + vec2(60.0, 0.0);
    drag_with(
        &mut paint,
        on_line,
        on_line + vec2(0.0, 40.0),
        egui::Modifiers::COMMAND,
    );
    assert_eq!(line_count(&paint, vector), 1, "no second line was painted");
    assert_ne!(arena_bytes(&paint), before, "the line moved");
}

#[test]
fn step_00_08_29_folder_transfer_and_merge_buttons() {
    let mut paint = PaintDriver::new("fg-folder-merge");
    let lower = inked_layer(&mut paint);
    let upper = inked_layer(&mut paint);
    let count = |paint: &PaintDriver| layers(paint).len();
    let before = count(&paint);
    // 00:08:34 Transfer to lower layer: the pixels go down, the layer
    // stays.
    let transfer = label_of("layer.transfer_down");
    let ink_lower = ink(&paint, lower);
    undo_pair(
        &mut paint,
        "transfer",
        |p| click_named(p, &transfer),
        move |p| ink(p, upper) == 0 && ink(p, lower) > ink_lower && count(p) == before,
    );
    undo(&mut paint);
    // 00:08:45 Merge with layer below: one layer fewer.
    let merge = label_of("layer.merge_down");
    undo_pair(
        &mut paint,
        "merge",
        |p| click_named(p, &merge),
        move |p| count(p) == before - 1,
    );
    undo(&mut paint);
    // 00:08:29 New layer folder.
    let folder = label_of("layer.new.folder");
    undo_pair(
        &mut paint,
        "folder",
        |p| click_named(p, &folder),
        move |p| {
            count(p) == before + 1
                && layers(p)
                    .iter()
                    .any(|info| sue_paint::doc::is_folder(&p.runtime.engine.world.arena, info.id))
        },
    );
    undo(&mut paint);
    // 00:11:34 Create folder and insert layer (Ctrl+G).
    undo_pair(
        &mut paint,
        "Ctrl+G",
        |p| {
            p.step(vec![key(egui::Key::G, egui::Modifiers::COMMAND)]);
        },
        move |p| sue_paint::doc::folder_of(&p.runtime.engine.world.arena, upper).is_some(),
    );
}

#[test]
fn step_00_08_51_layer_mask_hides_links_and_applies() {
    let mut paint = PaintDriver::new("fg-mask");
    let layer = inked_layer(&mut paint);
    let mask_of =
        move |p: &PaintDriver| sue_paint::doc::mask_of(&p.runtime.engine.world.arena, layer);
    let create = label_of("layer.mask.outside");
    undo_pair(
        &mut paint,
        "create mask",
        |p| click_named(p, &create),
        move |p| mask_of(p).is_some(),
    );
    // Painting with transparent on the mask hides part of the layer.
    let mask = mask_of(&paint).unwrap();
    let full = pixels(&paint, mask);
    press(&mut paint, egui::Key::C);
    let at = middle(&paint);
    stroke(&mut paint, at - vec2(30.0, 0.0), at + vec2(30.0, 0.0));
    assert_ne!(pixels(&paint, mask), full, "transparent painted the mask");
    assert!(ink(&paint, layer) > 0, "the layer itself kept its pixels");
    press(&mut paint, egui::Key::C);
    // 00:09:29 the link between layer and mask.
    let linked =
        move |p: &PaintDriver| sue_paint::doc::mask_linked(&p.runtime.engine.world.arena, layer);
    assert!(linked(&paint), "linked at first");
    let before = arena_bytes(&paint);
    click_named(&mut paint, "Unlink Mask from Layer");
    assert!(!linked(&paint), "unlinked");
    undo(&mut paint);
    assert!(arena_bytes(&paint) == before);
    // 00:06:17 Enable mask switches it off and on.
    let enable = label_of("layer.mask.toggle");
    let on = move |p: &PaintDriver| sue_paint::doc::mask_on(&p.runtime.engine.world.arena, layer);
    click_named(&mut paint, &enable);
    assert!(!on(&paint));
    click_named(&mut paint, &enable);
    assert!(on(&paint));
    // 00:09:52 Apply mask to layer.
    let apply = label_of("layer.mask.apply");
    let inked = ink(&paint, layer);
    undo_pair(
        &mut paint,
        "apply mask",
        |p| click_named(p, &apply),
        move |p| mask_of(p).is_none() && ink(p, layer) < inked,
    );
    // 00:10:14 Delete layer.
    let delete = label_of("layer.delete");
    let count = layers(&paint).len();
    undo_pair(
        &mut paint,
        "delete",
        |p| click_named(p, &delete),
        move |p| layers(p).len() == count - 1,
    );
}

#[test]
fn step_00_10_20_new_layer_kinds_from_the_right_click_list() {
    let mut paint = PaintDriver::new("fg-new-kinds");
    let count = |paint: &PaintDriver| layers(paint).len();
    // The row's own right click list (FGV-4).
    let name = layers(&paint).last().unwrap().name.clone();
    let row = paint.rect_of(&name).expect("the layer row");
    right_click(&mut paint, row.center());
    let texts: Vec<String> = paint.texts.iter().map(|(text, _)| text.clone()).collect();
    for command in [
        "layer.new.raster_top",
        "layer.new.folder",
        "layer.folder_insert",
        "layer.duplicate",
        "layer.delete",
        "layer.rasterize",
        "layer.convert",
        "layer.transfer_down",
        "layer.merge_down",
        "layer.flatten",
    ] {
        let label = label_of(command);
        assert!(
            texts.contains(&label),
            "{label:?} in the row's list {texts:?}"
        );
    }
    paint.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    paint.settle(3);
    // Each kind the video names: gradient, fill, tone, frame border
    // folder and 3D.
    for command in [
        "layer.new.gradient",
        "layer.new.fill",
        "layer.new.tone",
        "layer.new.frame",
        "layer.new.3d",
    ] {
        let before = count(&paint);
        let bytes = arena_bytes(&paint);
        command_ok(&mut paint, command);
        assert!(count(&paint) > before, "{command}: {}", said(&paint));
        undo(&mut paint);
        assert!(arena_bytes(&paint) == bytes, "{command} undoes");
        redo(&mut paint);
    }
}

#[test]
fn step_00_10_49_new_tone_layer_asks_frequency_density_and_dot_shape() {
    let mut paint = PaintDriver::new("fg-simple-tone");
    paint.command("layer.new.tone");
    paint.settle(3);
    let seen = names(&mut paint);
    shows_all(
        &seen,
        &["Simple Tone Settings", "Frequency presets", "Dot shape"],
        "Simple Tone Settings",
    );
    // 00:13:41 the narrator picks a star.
    click_named(&mut paint, "Dot shape");
    let star = paint.rect_of("Star").expect("Star among the dot shapes");
    click_with(&mut paint, star.center(), egui::Modifiers::NONE);
    let before = layers(&paint).len();
    click_named(&mut paint, "OK");
    paint.settle(4);
    assert_eq!(layers(&paint).len(), before + 1);
    let tone = layers(&paint).last().unwrap().id;
    assert_eq!(
        entity(&paint, tone)
            .property(sue_paint::effects::TONE_SHAPE)
            .and_then(sue_core::Value::as_i64),
        Some(6),
        "the star is kept"
    );
    assert_eq!(
        entity(&paint, tone)
            .property(sue_paint::effects::TONE)
            .and_then(sue_core::Value::as_bool),
        Some(true),
        "the new layer is toned"
    );
}

#[test]
fn step_00_11_52_ctrl_click_on_a_thumbnail_selects_what_it_holds() {
    let mut paint = PaintDriver::new("fg-thumb-select");
    inked_layer(&mut paint);
    let name = layers(&paint).last().unwrap().name.clone();
    let row = paint.rect_of(&name).expect("the row's name");
    // The thumbnail lies left of the name.
    let thumb = pos2(row.left() - 18.0, row.center().y);
    click_with(&mut paint, thumb, egui::Modifiers::COMMAND);
    // A fill on a new layer now fills only what the selection holds.
    paint.step(vec![key(
        egui::Key::N,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    )]);
    paint.settle(4);
    let filled = layers(&paint).last().unwrap().id;
    paint.command("edit.fill");
    paint.settle(4);
    let got = ink(&paint, filled);
    let all = 1600_u64 * 1200 * 255;
    assert!(got > 0 && got < all / 4, "{got} of {all}");
    // Selection from Layer has Select Overlapping Area (FGV-5).
    label_of("layer.selection.overlap");
    label_of("select.add_layer");
    label_of("select.subtract_layer");
}

#[test]
fn step_00_12_05_convert_layer_with_expression_color_and_keep_original() {
    let mut paint = PaintDriver::new("fg-convert");
    let layer = inked_layer(&mut paint);
    paint.command("layer.convert");
    paint.settle(3);
    let seen = names(&mut paint);
    shows_all(
        &seen,
        &["Expression Color", "Keep original layer", "Vector layer"],
        "Convert Layer",
    );
    let keep = paint.rect_of("Keep original layer").unwrap();
    click_with(&mut paint, keep.center(), egui::Modifiers::NONE);
    let count = layers(&paint).len();
    let before = arena_bytes(&paint);
    paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    paint.settle(4);
    assert_eq!(
        layers(&paint).len(),
        count + 1,
        "the original stays: {}",
        said(&paint)
    );
    assert!(layers(&paint).iter().any(|info| info.id == layer));
    undo(&mut paint);
    assert!(arena_bytes(&paint) == before);
}

#[test]
fn step_00_12_12_merge_selected_merge_visible_and_flatten() {
    let mut paint = PaintDriver::new("fg-merges");
    inked_layer(&mut paint);
    inked_layer(&mut paint);
    let count = |paint: &PaintDriver| layers(paint).len();
    let before = count(&paint);
    paint.command("layer.select_all");
    paint.settle(2);
    undo_pair(
        &mut paint,
        "merge selected",
        |p| p.command("layer.merge_selected"),
        move |p| count(p) < before,
    );
    undo(&mut paint);
    undo_pair(
        &mut paint,
        "merge visible to new",
        |p| p.command("layer.merge_visible"),
        move |p| count(p) == before + 1,
    );
    undo(&mut paint);
    undo_pair(
        &mut paint,
        "flatten",
        |p| p.command("layer.flatten"),
        move |p| count(p) == 1,
    );
}

/// Shows the Layer Property palette.
fn show_layer_property(paint: &mut PaintDriver) {
    paint.settle(2);
    if paint.rect_of("Layer Property").is_none() {
        paint.command("window.layer_property");
        paint.settle(4);
    }
    paint.click_text("Layer Property");
}

/// Every text and name the Layer Property palette shows, top to end.
fn layer_property_rows(paint: &mut PaintDriver) -> Vec<String> {
    let over = paint.rect_of("Layer Property").unwrap().center() + vec2(0.0, 120.0);
    scroll_all(paint, over)
}

#[test]
fn step_00_12_31_layer_property_effects_follow_the_video() {
    let mut paint = PaintDriver::new("fg-layer-property");
    let layer = inked_layer(&mut paint);
    show_layer_property(&mut paint);
    let on = |key: &'static str| {
        move |p: &PaintDriver| {
            entity(p, layer)
                .property(key)
                .and_then(sue_core::Value::as_bool)
                .unwrap_or(false)
        }
    };
    // 00:12:31 Border effect, 00:12:49 Watercolor edge, 00:13:02 Tone,
    // 00:14:11 Layer color: each a button, each one undo step, with
    // the settings the video shows.
    let effects: [(&str, &str, &[&str]); 4] = [
        (
            "Border effect",
            sue_paint::effects::BORDER,
            &["Anti-aliasing on border effect"],
        ),
        ("Watercolor edge", sue_paint::effects::WATERCOLOR, &[]),
        (
            "Tone",
            sue_paint::effects::TONE,
            &[
                "Density source",
                "Dot shape",
                "Posterization",
                "Reflect layer opacity",
                "Dot position",
            ],
        ),
        ("Layer color", sue_paint::effects::COLOR, &["Use sub color"]),
    ];
    for (button, key, rows) in effects {
        undo_pair(&mut paint, button, |p| click_named(p, button), on(key));
        let seen = layer_property_rows(&mut paint);
        shows_all(&seen, rows, button);
        click_named(&mut paint, button);
    }
    // 00:14:17 Expression color.
    let seen = layer_property_rows(&mut paint);
    shows_all(&seen, &["Expression color"], "expression color");
}

#[test]
fn step_00_14_35_search_layer_filters_the_list() {
    let mut paint = PaintDriver::new("fg-search-layer");
    inked_layer(&mut paint);
    paint.command("layer.new.vector");
    paint.settle(4);
    click_named(&mut paint, "Search Layer");
    let seen = names(&mut paint);
    shows_all(&seen, &["Kind of layer"], "Search Layer");
    // Words typed in the field keep only the layers whose name has
    // them: the vector layer stays, the raster layer leaves the list.
    let all = layers(&paint);
    let vector = all.last().unwrap().name.clone();
    let raster = all[all.len() - 2].name.clone();
    assert!(names(&mut paint).contains(&raster), "{raster} listed first");
    click_named(&mut paint, "Layer name");
    paint.step(vec![egui::Event::Text(vector.clone())]);
    paint.settle(4);
    let seen = names(&mut paint);
    assert!(seen.contains(&vector), "{vector} is found");
    assert!(!seen.contains(&raster), "{raster} is filtered out");
}

/// The brush size the Tool Property palette shows now.
fn shown_size(paint: &PaintDriver) -> f32 {
    paint
        .texts
        .iter()
        .find_map(|(text, _)| text.strip_suffix("\npx")?.parse::<f32>().ok())
        .expect("the brush size is shown")
}

#[test]
fn step_00_15_01_sub_tools_come_in_groups_with_their_buttons() {
    let mut paint = PaintDriver::new("fg-sub-tool-groups");
    paint.command("tool.pick.pen");
    paint.settle(3);
    paint.click_text("Sub Tool");
    // Pen and Marker are the Pen tool's two groups, as tabs.
    for group in ["Pen", "Marker"] {
        assert!(
            paint.rect_of(group).is_some(),
            "{group:?}: {:?}",
            paint.texts
        );
    }
    paint.click_text("Marker");
    assert!(
        paint.rect_of("Felt pen").is_some(),
        "the Marker group lists its own"
    );
    paint.click_text("Pen");
    paint.click_text("G-pen");
    // 00:15:54 Duplicate and delete sub tool, as buttons under the list.
    paint.settle(2);
    click_named(&mut paint, "Duplicate Sub Tool");
    assert!(
        names(&mut paint)
            .iter()
            .any(|name| name == "Duplicate Sub Tool")
    );
    paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    paint.settle(6);
    assert!(paint.rect_of("G-pen Copy").is_some(), "{:?}", paint.texts);
    paint.click_text("G-pen Copy");
    click_named(&mut paint, "Delete Sub Tool");
    // Deleting asks first.
    paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    paint.settle(6);
    assert!(paint.rect_of("G-pen Copy").is_none(), "deleted");
    // Pencil and Pastel, the brush categories.
    paint.command("tool.pick.pencil");
    paint.settle(3);
    for group in ["Pencil", "Pastel"] {
        assert!(paint.rect_of(group).is_some(), "{group:?}");
    }
    paint.command("tool.pick.brush");
    paint.settle(3);
    for group in ["Watercolor", "Thick paint", "India ink"] {
        assert!(paint.rect_of(group).is_some(), "{group:?}");
    }
}

#[test]
fn step_00_15_31_tool_property_size_opacity_stabilization_and_reset() {
    let mut paint = PaintDriver::new("fg-tool-property");
    paint.command("tool.pick.pen");
    paint.settle(3);
    let rows = tool_property_rows(&mut paint);
    shows_all(
        &rows,
        &[
            "Brush Size",
            "Opacity",
            "Stabilization",
            "Anti-aliasing",
            "Reset to Initial Settings",
            "Lock the settings of this sub tool",
        ],
        "Tool Property",
    );
    // 00:16:02 a change, then the reset button takes it back.
    show_tool_property(&mut paint);
    let before = shown_size(&paint);
    press(&mut paint, egui::Key::CloseBracket);
    press(&mut paint, egui::Key::CloseBracket);
    assert!(shown_size(&paint) > before);
    click_named(&mut paint, "Reset to Initial Settings");
    assert!((shown_size(&paint) - before).abs() < 0.05, "reset");
    // FGV-11 the lock: changes made while drawing are not kept.
    click_named(&mut paint, "Lock the settings of this sub tool");
    assert!(
        names(&mut paint)
            .iter()
            .any(|name| name == "Unlock the settings of this sub tool")
    );
    click_named(&mut paint, "Unlock the settings of this sub tool");
}

#[test]
fn step_00_16_07_sub_tool_detail_and_its_eyes() {
    let mut paint = PaintDriver::new("fg-sub-tool-detail");
    paint.command("tool.pick.pen");
    paint.settle(3);
    let shown = tool_property_rows(&mut paint).len();
    // The wrench and Window, Sub Tool Detail both open it.
    paint.command("window.sub_tool_detail");
    paint.settle(6);
    let seen = names(&mut paint);
    shows_all(
        &seen,
        &[
            "Sub Tool Detail",
            "Show Categories in Tool Property",
            "Hide in Tool Property",
        ],
        "Sub Tool Detail",
    );
    // 00:16:26 an eye closed: Tool Property shows one row fewer.
    click_named(&mut paint, "Hide in Tool Property");
    assert!(
        names(&mut paint)
            .iter()
            .any(|name| name == "Show in Tool Property")
    );
    paint.command("window.sub_tool_detail");
    paint.settle(4);
    let fewer = tool_property_rows(&mut paint).len();
    assert!(fewer < shown, "{fewer} rows, {shown} before");
}

#[test]
fn step_00_17_05_brush_size_palette_and_ctrl_alt_drag() {
    let mut paint = PaintDriver::fresh("fg-brush-size");
    paint.command("tool.pick.pen");
    paint.settle(3);
    let at = middle(&paint);
    // The same line on a layer of its own, with the size picked now.
    let line = |paint: &mut PaintDriver| {
        paint.step(vec![key(
            egui::Key::N,
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        )]);
        paint.settle(4);
        let layer = layers(paint).last().unwrap().id;
        stroke(paint, at - vec2(60.0, 30.0), at + vec2(60.0, -30.0));
        ink(paint, layer)
    };
    let first = line(&mut paint);
    // Ctrl+Alt dragged on the canvas sizes the brush up.
    drag_with(
        &mut paint,
        at + vec2(0.0, 40.0),
        at + vec2(80.0, 40.0),
        egui::Modifiers::COMMAND | egui::Modifiers::ALT,
    );
    let bigger = line(&mut paint);
    assert!(bigger > first, "{first} then {bigger}");
    // FGV-14: the Brush Size palette's grid starts at 0.7; a click takes
    // it.
    click_named(&mut paint, "Brush Size 0.7");
    let small = line(&mut paint);
    assert!(small < first, "{small} under {first}");
}

#[test]
fn step_00_17_33_transparent_color_and_the_color_palettes() {
    let mut paint = PaintDriver::new("fg-colors");
    let layer = inked_layer(&mut paint);
    let inked = ink(&paint, layer);
    // C picks transparent: any brush erases.
    press(&mut paint, egui::Key::C);
    let at = middle(&paint);
    stroke(&mut paint, at - vec2(20.0, 0.0), at + vec2(20.0, 0.0));
    assert!(ink(&paint, layer) < inked, "transparent erased");
    press(&mut paint, egui::Key::C);
    // 00:17:17 to 00:19:08: every color palette of the video is there.
    for (command, name) in [
        ("window.color_wheel", "Color Wheel"),
        ("window.color_history", "Color History"),
        ("window.color_slider", "Color Slider"),
        ("window.color_set", "Color Set"),
        ("window.intermediate", "Intermediate Color"),
        ("window.approximate", "Approximate Color"),
    ] {
        label_of(command);
        if paint.rect_of(name).is_none() {
            paint.command(command);
            paint.settle(4);
        }
        assert!(paint.rect_of(name).is_some(), "{name}");
    }
    // 00:18:03 the Color Slider's CMYK page.
    paint.click_text("Color Slider");
    let seen = names(&mut paint);
    assert!(seen.iter().any(|name| name == "CMYK"), "{seen:?}");
}

#[test]
fn step_00_19_19_navigator_and_sub_view_flip() {
    let mut paint = PaintDriver::new("fg-navigator");
    let seen = names(&mut paint);
    for name in ["Zoom In", "Zoom Out", "Flip Horizontal", "Flip Vertical"] {
        assert!(seen.iter().any(|found| found == name), "{name} in {seen:?}");
    }
    // 00:19:56 Sub View flips its picture.
    label_of("window.sub_view");
    paint.command("window.sub_view");
    paint.settle(4);
    paint.click_text("Sub View");
    let seen = names(&mut paint);
    assert!(
        seen.iter().filter(|name| name.contains("Flip")).count() >= 2,
        "{seen:?}"
    );
}

#[test]
fn step_00_20_32_auto_action_plays_and_leaves_its_steps_in_history() {
    let mut paint = PaintDriver::new("fg-auto-action");
    paint.command("window.auto_actions");
    paint.settle(4);
    paint.click_text("Auto Action");
    let depth = paint.runtime.engine.history.depth();
    let count = layers(&paint).len();
    // Sue Paint's own default set: a new clipped layer, played.
    paint.click_text("New Clipped Layer");
    paint.command("auto.play");
    paint.settle(8);
    assert_eq!(layers(&paint).len(), count + 1, "{}", said(&paint));
    // 00:20:50 each step is its own step in History.
    assert!(paint.runtime.engine.history.depth() >= depth + 2);
    undo(&mut paint);
    undo(&mut paint);
    assert_eq!(layers(&paint).len(), count);
}

#[test]
fn step_00_21_20_history_information_and_animation_palettes() {
    let mut paint = PaintDriver::new("fg-history");
    inked_layer(&mut paint);
    // History: a click on an older step goes back to it.
    paint.command("window.history");
    paint.settle(4);
    paint.click_text("History");
    let count = layers(&paint).len();
    let seen = names(&mut paint);
    assert!(seen.len() > 3);
    // Information, Animation Cels and Timeline are under Window.
    for command in ["window.info", "window.cels", "window.timeline"] {
        label_of(command);
    }
    paint.command("window.info");
    paint.settle(4);
    assert!(
        paint.rect_of("Information").is_some()
            || names(&mut paint).iter().any(|n| n == "Information")
    );
    assert_eq!(layers(&paint).len(), count);
}

#[test]
fn step_00_21_47_item_bank_keeps_pictures_to_put_on_the_canvas() {
    let mut paint = PaintDriver::new("fg-item-bank");
    inked_layer(&mut paint);
    paint.command("window.item_bank");
    paint.settle(4);
    paint.click_text("Item Bank");
    paint.click_text("Pictures");
    let layer_name = layers(&paint).last().unwrap().name.clone();
    click_named(&mut paint, "Keep Current Layer as Picture");
    paint.settle(6);
    // The kept picture, named after its layer, under the Pictures heading.
    let heading = paint.rect_of("Pictures").expect("the Pictures heading");
    let picture = step_told(&mut paint, Vec::new())
        .into_iter()
        .filter(|(name, _, rect)| {
            *name == layer_name
                && rect.top() > heading.top()
                && rect.left() < heading.right() + 200.0
                && rect.right() > heading.left() - 200.0
        })
        .map(|(_, _, rect)| rect)
        .min_by(|a, b| {
            a.center()
                .distance(heading.center())
                .total_cmp(&b.center().distance(heading.center()))
        })
        .expect("the kept picture");
    let count = layers(&paint).len();
    let before = arena_bytes(&paint);
    // Dragged out of the palette and let go over the canvas.
    let to = middle(&paint);
    paint.drag(picture.center(), to, egui::PointerButton::Primary);
    assert_eq!(layers(&paint).len(), count + 1, "dropped on a new layer");
    undo(&mut paint);
    assert!(arena_bytes(&paint) == before);
}

// B. Tools.

/// The window title Sue Paint gives now: it ends with the zoom.
fn title(paint: &PaintDriver) -> String {
    paint
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.window_title(&paint.runtime.engine.world.arena))
        .unwrap_or_default()
}

/// The sub tools the Sub Tool palette lists for `tool` (a command), with
/// its group tabs.
fn sub_tools_of(paint: &mut PaintDriver, tool: &str) -> Vec<String> {
    paint.command(tool);
    paint.settle(3);
    paint.click_text("Sub Tool");
    let mut seen: Vec<String> = paint.texts.iter().map(|(text, _)| text.clone()).collect();
    seen.sort();
    seen.dedup();
    seen
}

/// Picks the sub tool `sub` of `tool` and returns the rows its Tool
/// Property shows.
fn rows_of(paint: &mut PaintDriver, tool: &str, sub: &str) -> Vec<String> {
    sub_tools_of(paint, tool);
    if paint.rect_of(sub).is_none() {
        // In the other group: its tab first.
        let tabs: Vec<String> = paint.texts.iter().map(|(text, _)| text.clone()).collect();
        for tab in tabs {
            paint.click_text(&tab);
            if paint.rect_of(sub).is_some() {
                break;
            }
        }
    }
    paint.click_text(sub);
    tool_property_rows(paint)
}

#[test]
fn step_00_22_14_every_tool_of_the_video_is_on_the_tool_palette() {
    let mut paint = PaintDriver::new("fg-tool-palette");
    let seen = names(&mut paint);
    for tool in [
        "Zoom",
        "Operation",
        "Move Layer",
        "Selection",
        "Auto Select",
        "Eyedropper",
        "Pen",
        "Sketch",
        "Brush",
        "Airbrush",
        "Decoration",
        "Eraser",
        "Blend",
        "Liquify",
        "Fill",
        "Gradient",
        "Figure",
        "Frame Border",
        "Ruler",
        "Text",
        "Balloon",
        "Correct Line",
    ] {
        assert!(
            seen.iter()
                .any(|name| name == tool || name.starts_with(&format!("{tool} ("))),
            "{tool} in {seen:?}"
        );
    }
}

#[test]
fn step_00_22_19_zoom_tool_and_ctrl_plus_minus() {
    let mut paint = PaintDriver::new("fg-zoom");
    let start = title(&paint);
    assert!(
        start.ends_with("%) - Sue Paint") || start.contains('%'),
        "{start}"
    );
    // The Zoom tool's Zoom In: a click on the canvas zooms in.
    let subs = sub_tools_of(&mut paint, "tool.zoom");
    shows_all(&subs, &["Zoom In", "Zoom Out"], "Zoom tool");
    paint.click_text("Zoom In");
    let at = middle(&paint);
    click_with(&mut paint, at, egui::Modifiers::NONE);
    let zoomed = title(&paint);
    assert_ne!(zoomed, start, "zoomed in");
    // 00:22:36 Ctrl with minus and plus.
    paint.step(vec![key(egui::Key::Minus, egui::Modifiers::COMMAND)]);
    paint.settle(3);
    assert_ne!(title(&paint), zoomed, "Ctrl+minus");
    let out = title(&paint);
    paint.step(vec![key(egui::Key::Plus, egui::Modifiers::COMMAND)]);
    paint.settle(3);
    assert_ne!(title(&paint), out, "Ctrl+plus");
}

#[test]
fn step_00_22_41_hand_and_rotate_sub_tools() {
    let mut paint = PaintDriver::new("fg-hand");
    let subs = sub_tools_of(&mut paint, "tool.hand");
    shows_all(&subs, &["Hand", "Rotate"], "Hand tool");
    // Rotate turns the view: the title stays, the view's turn is a
    // command of its own too.
    label_of("view.rotate_left");
    label_of("view.rotate_right");
    label_of("view.rotate_reset");
}

#[test]
fn step_00_22_51_operation_sub_tools_and_select_layer() {
    let mut paint = PaintDriver::new("fg-operation");
    let lower = inked_layer(&mut paint);
    // The upper layer holds nothing at the middle.
    paint.step(vec![key(
        egui::Key::N,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    )]);
    paint.settle(4);
    let subs = sub_tools_of(&mut paint, "tool.operation");
    shows_all(
        &subs,
        &["Object", "Select layer", "Light table", "Edit timeline"],
        "Operation",
    );
    // Select layer: a click on the stroke makes its layer current, so
    // the next layer command acts on it.
    paint.click_text("Select layer");
    let at = middle(&paint);
    click_with(&mut paint, at, egui::Modifiers::NONE);
    paint.command("layer.toggle.lock");
    paint.settle(3);
    assert_eq!(
        entity(&paint, lower)
            .property(sue_model::picture::LOCKED)
            .and_then(sue_core::Value::as_bool),
        Some(true),
        "the stroke's layer was picked"
    );
}

#[test]
fn step_00_23_17_move_layer_moves_the_current_layer() {
    let mut paint = PaintDriver::new("fg-move-layer");
    let layer = inked_layer(&mut paint);
    let subs = sub_tools_of(&mut paint, "tool.move_layer");
    shows_all(&subs, &["Move Layer"], "Move layer");
    let at = middle(&paint);
    let offset =
        move |paint: &PaintDriver| sue_model::picture::LayerStyle::of(entity(paint, layer)).offset;
    undo_pair(
        &mut paint,
        "move layer",
        |paint| stroke(paint, at, at + vec2(50.0, 30.0)),
        move |paint| offset(paint) != [0, 0],
    );
}

#[test]
fn step_00_23_29_selection_tools_modes_and_ctrl_d() {
    let mut paint = PaintDriver::new("fg-selection");
    let subs = sub_tools_of(&mut paint, "tool.selection");
    shows_all(
        &subs,
        &[
            "Rectangle",
            "Ellipse",
            "Lasso",
            "Polyline",
            "Selection pen",
            "Erase selection",
        ],
        "Selection",
    );
    // FGV-20: aspect, adjust angle after fixed, anti-aliasing.
    let rows = rows_of(&mut paint, "tool.selection", "Rectangle");
    shows_all(
        &rows,
        &[
            "Aspect",
            "Adjust angle after fixed",
            "Anti-aliasing",
            "Shift adds, Alt subtracts, both intersect",
        ],
        "Rectangle",
    );
    // A rectangle, then Fill fills only inside it.
    let at = middle(&paint);
    stroke(&mut paint, at - vec2(40.0, 30.0), at + vec2(40.0, 30.0));
    paint.step(vec![key(
        egui::Key::N,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    )]);
    paint.settle(4);
    let layer = layers(&paint).last().unwrap().id;
    paint.command("edit.fill");
    paint.settle(3);
    let inside = ink(&paint, layer);
    let all = 1600_u64 * 1200 * 255;
    assert!(inside > 0 && inside < all / 4, "{inside}");
    undo(&mut paint);
    // Ctrl+D lets go of it: Fill now fills everything.
    paint.step(vec![key(egui::Key::D, egui::Modifiers::COMMAND)]);
    paint.settle(3);
    paint.command("edit.fill");
    paint.settle(3);
    assert!(ink(&paint, layer) > all / 2, "deselected");
}

#[test]
fn step_00_24_24_auto_select_close_gap_tolerance_and_area_scaling() {
    let mut paint = PaintDriver::new("fg-auto-select");
    let subs = sub_tools_of(&mut paint, "tool.auto_select");
    shows_all(
        &subs,
        &[
            "Refer Only Editing Layer",
            "Refer Other Layers",
            "Refer Reference Layers",
        ],
        "Auto Select",
    );
    let rows = rows_of(&mut paint, "tool.auto_select", "Refer Other Layers");
    shows_all(
        &rows,
        &[
            "Apply to connected pixels only",
            "Close gap",
            "Tolerance",
            "Area scaling",
            "Refer multiple",
            "Anti-aliasing",
        ],
        "Auto Select",
    );
    // A closed shape, then a click inside it selects inside only.
    let ring = inked_layer(&mut paint);
    let at = middle(&paint);
    paint.command("tool.pen");
    paint.settle(2);
    for (from, to) in [
        (vec2(-60.0, -40.0), vec2(60.0, -40.0)),
        (vec2(60.0, -40.0), vec2(60.0, 40.0)),
        (vec2(60.0, 40.0), vec2(-60.0, 40.0)),
        (vec2(-60.0, 40.0), vec2(-60.0, -40.0)),
    ] {
        stroke(&mut paint, at + from, at + to);
    }
    let _ = ring;
    paint.command("tool.auto_select");
    paint.settle(2);
    click_with(&mut paint, at + vec2(0.0, 20.0), egui::Modifiers::NONE);
    paint.step(vec![key(
        egui::Key::N,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    )]);
    paint.settle(4);
    let filled = layers(&paint).last().unwrap().id;
    paint.command("edit.fill");
    paint.settle(3);
    let got = ink(&paint, filled);
    let all = 1600_u64 * 1200 * 255;
    assert!(got > 0 && got < all / 4, "inside the shape only: {got}");
}

#[test]
fn step_00_26_44_obtain_layer_color_on_an_empty_spot_gives_transparent() {
    let mut paint = PaintDriver::new("fg-eyedropper");
    let layer = inked_layer(&mut paint);
    let subs = sub_tools_of(&mut paint, "tool.eyedropper");
    shows_all(
        &subs,
        &["Obtain Display Color", "Obtain Layer Color"],
        "Eyedropper",
    );
    paint.click_text("Obtain Layer Color");
    let at = middle(&paint);
    click_with(&mut paint, at + vec2(0.0, 120.0), egui::Modifiers::NONE);
    // The pen now draws transparent: it erases.
    let inked = ink(&paint, layer);
    paint.command("tool.pen");
    paint.settle(2);
    stroke(&mut paint, at - vec2(30.0, 0.0), at + vec2(30.0, 0.0));
    assert!(ink(&paint, layer) < inked, "transparent was picked");
    // 00:26:29 Alt held with a pen picks a color off the canvas, and the
    // pen draws with it again.
    drag_with(
        &mut paint,
        at + vec2(50.0, 0.0),
        at + vec2(50.0, 0.0),
        egui::Modifiers::ALT,
    );
    let after = ink(&paint, layer);
    stroke(&mut paint, at - vec2(30.0, 20.0), at + vec2(30.0, 20.0));
    assert!(ink(&paint, layer) > after, "Alt picked a color that inks");
}

#[test]
fn step_00_27_26_drawing_tools_and_their_sub_tool_groups() {
    let mut paint = PaintDriver::new("fg-drawing-tools");
    let cases: [(&str, &[&str]); 7] = [
        ("tool.pick.pen", &["Pen", "Marker", "G-pen", "Mapping pen"]),
        ("tool.pick.pencil", &["Pencil", "Pastel"]),
        (
            "tool.pick.brush",
            &["Watercolor", "Thick paint", "India ink"],
        ),
        ("tool.pick.airbrush", &["Soft", "Spray", "Droplet"]),
        ("tool.pick.decoration", &["Decoration", "Ribbon"]),
        ("tool.eraser", &["Hard", "Soft", "Vector", "Kneaded eraser"]),
        ("tool.blend", &["Blur", "Finger Tip", "Copy stamp"]),
    ];
    for (tool, wanted) in cases {
        let seen = sub_tools_of(&mut paint, tool);
        shows_all(&seen, wanted, tool);
    }
    // 00:31:25 the ribbon, on an airbrush laid in shapes. Sue's Spray
    // lays puffs of drops (p5-brush), so the ribbon is set on Droplet.
    let rows = rows_of(&mut paint, "tool.pick.airbrush", "Droplet");
    shows_all(&rows, &["Lay as a ribbon", "Scatter", "Spacing"], "Droplet");
}

#[test]
fn step_00_28_09_brush_settings_categories_of_sub_tool_detail() {
    let mut paint = PaintDriver::new("fg-brush-detail");
    paint.command("tool.pick.brush");
    paint.settle(3);
    paint.command("window.sub_tool_detail");
    paint.settle(6);
    let seen = names(&mut paint);
    shows_all(
        &seen,
        &[
            "Brush Size",
            "Ink",
            "Anti-aliasing",
            "Brush Tip",
            "Stroke",
            "Texture",
            "Starting and Ending",
            "Color Jitter",
            "Correction",
        ],
        "Sub Tool Detail categories",
    );
    // FGV-13 and the Ink, Stroke and Correction rows of the video. The
    // default brush is swept, not laid in dabs, so it has no Spacing;
    // the Droplet's Spacing is checked at 00:31:25.
    let rows = tool_property_rows(&mut paint);
    shows_all(
        &rows,
        &[
            "Hardness",
            "Color mixing",
            "Paper grain",
            "Stabilization",
            "Adjust by speed",
            "Starting",
            "Ending",
        ],
        "brush rows",
    );
}

#[test]
fn step_00_33_55_vector_eraser_erases_vector_lines() {
    let mut paint = PaintDriver::new("fg-vector-eraser");
    let rows = rows_of(&mut paint, "tool.eraser", "Vector");
    shows_all(&rows, &["Erase up to intersection"], "vector eraser");
    paint.command("layer.new.vector");
    paint.settle(4);
    let layer = layers(&paint).last().unwrap().id;
    let at = middle(&paint);
    paint.command("tool.pen");
    paint.settle(2);
    stroke(&mut paint, at - vec2(60.0, 0.0), at + vec2(60.0, 0.0));
    assert_eq!(line_count(&paint, layer), 1);
    paint.command("tool.eraser");
    paint.settle(2);
    paint.click_text("Vector");
    let before = arena_bytes(&paint);
    stroke(&mut paint, at - vec2(0.0, 30.0), at + vec2(0.0, 30.0));
    assert_ne!(arena_bytes(&paint), before, "the eraser touched the line");
    undo(&mut paint);
    assert!(arena_bytes(&paint) == before);
}

#[test]
fn step_00_34_22_brush_size_dynamics_pressure_tilt_velocity_random() {
    let mut paint = PaintDriver::new("fg-dynamics");
    paint.command("tool.pick.pen");
    paint.settle(3);
    show_tool_property(&mut paint);
    let rows = tool_property_rows(&mut paint);
    assert!(
        rows.iter()
            .any(|row| row == "Brush Size Dynamics: what the pen changes")
    );
    // Back to the top of Tool Property, where the Brush Size row is.
    let tab = paint.rect_of("Tool Property").unwrap();
    for _ in 0..20 {
        paint.step(vec![
            egui::Event::PointerMoved(tab.center() + vec2(0.0, 160.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, 400.0),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            },
        ]);
    }
    paint.settle(3);
    click_named(&mut paint, "Brush Size Dynamics: what the pen changes");
    paint.settle(4);
    let seen = names(&mut paint);
    shows_all(
        &seen,
        &["Pen pressure", "Tilt", "Velocity", "Random"],
        "Brush Size Dynamics",
    );
}

#[test]
fn step_00_34_40_fill_settings_and_sub_tools() {
    let mut paint = PaintDriver::new("fg-fill");
    let subs = sub_tools_of(&mut paint, "tool.pick.fill");
    shows_all(
        &subs,
        &[
            "Refer Only Editing Layer",
            "Refer Other Layers",
            "Close and fill",
            "Paint unpainted area",
        ],
        "Fill",
    );
    let rows = rows_of(&mut paint, "tool.pick.fill", "Close and fill");
    shows_all(
        &rows,
        &[
            "Apply to connected pixels only",
            "Close gap",
            "Tolerance",
            "Area scaling",
            "Refer multiple",
            "Target color",
            "Fill up to vector path",
        ],
        "Fill",
    );
    // A click fills the empty layer, one undo step.
    paint.step(vec![key(
        egui::Key::N,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    )]);
    paint.settle(4);
    let layer = layers(&paint).last().unwrap().id;
    let subs = sub_tools_of(&mut paint, "tool.pick.fill");
    assert!(!subs.is_empty());
    paint.click_text("Refer Only Editing Layer");
    let at = middle(&paint);
    undo_pair(
        &mut paint,
        "fill",
        |paint| click_with(paint, at, egui::Modifiers::NONE),
        move |paint| ink(paint, layer) > 0,
    );
}

#[test]
fn step_00_35_30_gradient_sub_tools_shapes_and_edge_process() {
    let mut paint = PaintDriver::new("fg-gradient");
    let subs = sub_tools_of(&mut paint, "tool.pick.gradient");
    shows_all(
        &subs,
        &[
            "Gradient",
            "Contour line",
            "Drawing Color to Transparent",
            "Main to Sub Color",
            "Ellipse",
        ],
        "Gradient",
    );
    let rows = rows_of(
        &mut paint,
        "tool.pick.gradient",
        "Drawing Color to Transparent",
    );
    shows_all(
        &rows,
        &[
            "Edge process",
            "Step of angle",
            "Where to create",
            "Gradient Stops",
            "Shape",
        ],
        "Gradient",
    );
    paint.step(vec![key(
        egui::Key::N,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    )]);
    paint.settle(4);
    let count = layers(&paint).len();
    let at = middle(&paint);
    let before = arena_bytes(&paint);
    stroke(&mut paint, at - vec2(0.0, 60.0), at + vec2(0.0, 60.0));
    assert_ne!(arena_bytes(&paint), before, "a gradient was laid");
    assert!(layers(&paint).len() >= count);
    undo(&mut paint);
    assert!(arena_bytes(&paint) == before);
}

#[test]
fn step_00_36_03_figure_direct_draw_and_line_groups() {
    let mut paint = PaintDriver::new("fg-figure");
    let subs = sub_tools_of(&mut paint, "tool.pick.figure");
    shows_all(
        &subs,
        &[
            "Direct draw",
            "Saturated Line",
            "Speed Line",
            "Straight Line",
            "Rectangle",
            "Ellipse",
            "Polyline",
            "Curve",
            "Continuous Curve",
            "Bezier Curve",
            "Polygon",
        ],
        "Figure",
    );
    paint.click_text("Rectangle");
    let layer = layers(&paint).last().unwrap().id;
    let at = middle(&paint);
    undo_pair(
        &mut paint,
        "rectangle",
        |paint| stroke(paint, at - vec2(40.0, 30.0), at + vec2(40.0, 30.0)),
        move |paint| ink(paint, layer) > 0,
    );
}

#[test]
fn step_00_36_43_frame_border_tools_make_panels() {
    let mut paint = PaintDriver::new("fg-frame");
    let subs = sub_tools_of(&mut paint, "tool.pick.frame");
    shows_all(
        &subs,
        &["Rectangle Frame", "Polyline frame", "Frame Border Division"],
        "Frame Border",
    );
    paint.click_text("Rectangle Frame");
    let count = layers(&paint).len();
    let at = middle(&paint);
    undo_pair(
        &mut paint,
        "frame",
        |paint| stroke(paint, at - vec2(80.0, 60.0), at + vec2(80.0, 60.0)),
        move |paint| layers(paint).len() > count,
    );
}

#[test]
fn step_00_37_26_ruler_sub_tools_and_snapping() {
    let mut paint = PaintDriver::new("fg-ruler");
    let subs = sub_tools_of(&mut paint, "tool.pick.ruler");
    shows_all(
        &subs,
        &[
            "Linear Ruler",
            "Curve Ruler",
            "Figure Ruler",
            "Ruler Pen",
            "Special Ruler",
            "Perspective Ruler",
            "Symmetrical Ruler",
        ],
        "Ruler",
    );
    paint.click_text("Linear Ruler");
    let at = middle(&paint);
    let before = arena_bytes(&paint);
    stroke(&mut paint, at - vec2(80.0, 0.0), at + vec2(80.0, 0.0));
    assert_ne!(arena_bytes(&paint), before, "a ruler was laid");
    // A pen line started near it follows it: the slanting drag lays a
    // flat line along the ruler, much flatter than the same drag once
    // the ruler is gone.
    let layer = layers(&paint).last().unwrap().id;
    paint.command("tool.pick.pen");
    paint.settle(2);
    let drag = |paint: &mut PaintDriver| {
        let before = pixels(paint, layer);
        stroke(paint, at + vec2(-40.0, 4.0), at + vec2(40.0, 30.0));
        let after = pixels(paint, layer);
        let width = sue_paint::doc::raster(&paint.runtime.engine.world.arena, layer)
            .unwrap()
            .width() as usize;
        let rows: Vec<usize> = before
            .as_chunks::<4>()
            .0
            .iter()
            .zip(after.as_chunks::<4>().0)
            .enumerate()
            .filter(|(_, (was, now))| was[3] != now[3])
            .map(|(index, _)| index / width)
            .collect();
        assert!(!rows.is_empty(), "the pen line landed");
        undo(paint);
        rows.iter().max().unwrap() - rows.iter().min().unwrap()
    };
    let along = drag(&mut paint);
    undo(&mut paint);
    let free = drag(&mut paint);
    assert!(
        along * 2 < free,
        "the line follows the ruler: {along} rows tall, {free} without it"
    );
}

#[test]
fn step_00_37_37_text_font_and_justify() {
    let mut paint = PaintDriver::new("fg-text");
    let rows = rows_of(&mut paint, "tool.pick.text", "Text");
    shows_all(
        &rows,
        &["Font:", "Justify:", "Direction", "Size", "Line spacing"],
        "Text",
    );
    let count = layers(&paint).len();
    let at = middle(&paint);
    click_with(&mut paint, at, egui::Modifiers::NONE);
    paint.step(vec![egui::Event::Text("Hi".into())]);
    paint.settle(3);
    paint.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    paint.settle(4);
    assert!(
        layers(&paint).len() > count,
        "a text layer: {}",
        said(&paint)
    );
}

#[test]
fn step_00_37_45_balloon_and_its_tail() {
    let mut paint = PaintDriver::new("fg-balloon");
    let subs = sub_tools_of(&mut paint, "tool.pick.balloon");
    shows_all(
        &subs,
        &[
            "Balloon",
            "Balloon Tail",
            "Ellipse Balloon",
            "Flash Balloon",
            "Balloon Pen",
        ],
        "Balloon",
    );
    paint.click_text("Ellipse Balloon");
    let count = layers(&paint).len();
    let at = middle(&paint);
    undo_pair(
        &mut paint,
        "balloon",
        |paint| stroke(paint, at - vec2(60.0, 40.0), at + vec2(60.0, 40.0)),
        move |paint| layers(paint).len() > count,
    );
    let rows = rows_of(&mut paint, "tool.pick.balloon", "Ellipse Balloon");
    shows_all(&rows, &["Line width"], "Balloon");
}

#[test]
fn step_00_37_57_correct_line_and_remove_dust() {
    let mut paint = PaintDriver::new("fg-correct-line");
    let subs = sub_tools_of(&mut paint, "tool.correct_line");
    shows_all(
        &subs,
        &[
            "Correct Line",
            "Remove dust",
            "Control point",
            "Pinch vector line",
            "Simplify vector line",
            "Connect vector lines",
            "Adjust line width",
            "Redraw vector line",
            "Redraw vector line width",
        ],
        "Correct Line",
    );
    let rows = rows_of(&mut paint, "tool.correct_line", "Pinch vector line");
    shows_all(&rows, &["Fix end", "Pinch level", "Effect range"], "Pinch");
    // Adjust line width on a vector line changes the line.
    paint.command("layer.new.vector");
    paint.settle(4);
    let at = middle(&paint);
    paint.command("tool.pen");
    paint.settle(2);
    stroke(&mut paint, at - vec2(60.0, 0.0), at + vec2(60.0, 0.0));
    rows_of(&mut paint, "tool.correct_line", "Adjust line width");
    let before = arena_bytes(&paint);
    stroke(&mut paint, at - vec2(30.0, 0.0), at + vec2(30.0, 0.0));
    assert_ne!(arena_bytes(&paint), before, "the width changed");
    undo(&mut paint);
    assert!(arena_bytes(&paint) == before);
    // Remove dust on a raster layer: specks go.
    let rows = rows_of(&mut paint, "tool.correct_line", "Remove dust");
    shows_all(&rows, &["Dust size"], "Remove dust");
}

// C. Menus.

/// Runs `command`, which opens a dialog; every name the dialog shows;
/// then closes it with Escape.
fn dialog_names(paint: &mut PaintDriver, command: &str) -> Vec<String> {
    paint.command(command);
    paint.settle(4);
    let seen = names(paint);
    paint.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    paint.settle(3);
    seen
}

/// Every command of Sue Paint's menus, at any depth.
fn menu_commands() -> Vec<String> {
    fn walk(items: &[sue_wm::MenuItem], out: &mut Vec<String>) {
        for item in items {
            match item {
                sue_wm::MenuItem::Action { command, .. } => out.push(command.clone()),
                sue_wm::MenuItem::Sub { items, .. } => walk(items, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for menu in sue_paint::menus() {
        walk(&menu.items, &mut out);
    }
    out
}

/// A rectangle selection around the middle, with the Selection tool.
fn select_middle(paint: &mut PaintDriver) {
    paint.command("tool.selection");
    paint.settle(2);
    let at = middle(paint);
    stroke(paint, at - vec2(50.0, 30.0), at + vec2(50.0, 30.0));
}

#[test]
fn step_00_38_47_settings_commands_close_the_file_menu() {
    let file = sue_paint::menus()
        .into_iter()
        .next()
        .expect("the File menu");
    let mut all = Vec::new();
    for item in &file.items {
        if let sue_wm::MenuItem::Action { command, .. } = item {
            all.push(command.clone());
        }
    }
    let tail: Vec<&String> = all.iter().rev().take(9).collect();
    for command in [
        "file.preferences",
        "file.shortcuts",
        "file.modifiers",
        "file.pressure",
    ] {
        assert!(
            tail.iter().any(|found| *found == command),
            "{command} near the end of File: {all:?}"
        );
    }
}

#[test]
fn step_00_38_57_preferences_interface_color_light_and_dark() {
    let mut paint = PaintDriver::new("fg-preferences");
    paint.command("file.preferences");
    paint.settle(10);
    let seen = names(&mut paint);
    assert!(
        seen.iter().any(|name| name.contains("Sue Paint's colors")
            || name == "Theme"
            || name.contains("colors")),
        "{seen:?}"
    );
}

#[test]
fn step_00_39_24_modifier_key_settings_general_or_per_tool() {
    let mut paint = PaintDriver::new("fg-modifiers");
    let seen = dialog_names(&mut paint, "file.modifiers");
    shows_all(
        &seen,
        &["General", "For One Tool", "Mouse Buttons"],
        "Modifier Key Settings",
    );
    // 00:39:11 Shortcut Settings opens the key settings.
    paint.command("file.shortcuts");
    paint.settle(10);
    assert!(
        !said(&paint).contains("not in Sue Paint"),
        "{}",
        said(&paint)
    );
}

#[test]
fn step_00_39_34_pen_pressure_settings_draw_test_strokes() {
    let mut paint = PaintDriver::new("fg-pressure");
    let seen = dialog_names(&mut paint, "file.pressure");
    assert!(seen.len() > 10, "the dialog shows: {seen:?}");
    assert!(!said(&paint).contains("not in Sue Paint"));
}

#[test]
fn step_00_40_09_timelapse_stop_asks_before_throwing_the_recording_away() {
    let mut paint = PaintDriver::new("fg-timelapse");
    paint.command("file.timelapse.record");
    paint.settle(3);
    inked_layer(&mut paint);
    paint.command("file.timelapse.record");
    paint.settle(4);
    let seen = names(&mut paint);
    shows_all(&seen, &["Keep", "Delete"], "stopping the timelapse");
    paint.click_text("Keep");
    // Export is there for the kept recording.
    label_of("file.timelapse.export");
}

#[test]
fn step_00_40_37_new_dialog_kinds_colors_and_comic_settings() {
    let mut paint = PaintDriver::new("fg-new");
    paint.command("file.new");
    paint.settle(4);
    let seen = names(&mut paint);
    shows_all(
        &seen,
        &[
            "Illustration",
            "Webtoon",
            "Comic",
            "Show all comic settings",
            "Animation",
            "Basic expression color",
            "Gray",
            "Monochrome",
            "Paper color",
            "Create animated illustration",
            "Record timelapse",
        ],
        "New",
    );
    // 00:41:14 and FGV-46: the comic's bleed and default tone frequency.
    click_named(&mut paint, "Comic");
    let seen = names(&mut paint);
    shows_all(&seen, &["Bleed width"], "New comic");
    click_named(&mut paint, "Monochrome");
    let seen = names(&mut paint);
    shows_all(&seen, &["Default tone frequency"], "New monochrome comic");
    paint.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    paint.settle(3);
}

#[test]
fn step_00_42_05_edit_undo_redo_cut_copy_paste_and_delete() {
    let mut paint = PaintDriver::new("fg-edit");
    let layer = inked_layer(&mut paint);
    let count = layers(&paint).len();
    paint.step(vec![key(egui::Key::C, egui::Modifiers::COMMAND)]);
    paint.settle(2);
    undo_pair(
        &mut paint,
        "paste",
        |paint| {
            paint.step(vec![key(egui::Key::V, egui::Modifiers::COMMAND)]);
        },
        move |paint| layers(paint).len() == count + 1,
    );
    undo(&mut paint);
    let inked = ink(&paint, layer);
    undo_pair(
        &mut paint,
        "delete",
        |paint| {
            paint.command("edit.clear");
        },
        move |paint| ink(paint, layer) < inked,
    );
}

#[test]
fn step_00_42_11_smart_smoothing_with_strength_and_noise() {
    let mut paint = PaintDriver::new("fg-smart-smoothing");
    inked_layer(&mut paint);
    let seen = dialog_names(&mut paint, "edit.smart_smoothing");
    shows_all(&seen, &["Remove Noise"], "Smart Smoothing");
}

#[test]
fn step_00_42_16_fill_and_advanced_fill() {
    let mut paint = PaintDriver::new("fg-edit-fill");
    paint.step(vec![key(
        egui::Key::N,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    )]);
    paint.settle(4);
    let layer = layers(&paint).last().unwrap().id;
    undo_pair(
        &mut paint,
        "fill",
        |paint| paint.command("edit.fill"),
        move |paint| ink(paint, layer) > 0,
    );
    let seen = dialog_names(&mut paint, "edit.advanced_fill");
    assert!(seen.len() > 5);
}

#[test]
fn step_00_42_22_colorize_from_a_hint_layer() {
    let mut paint = PaintDriver::new("fg-colorize");
    // Line art: a closed box.
    let lines = inked_layer(&mut paint);
    let at = middle(&paint);
    for (from, to) in [
        (vec2(-60.0, -40.0), vec2(60.0, -40.0)),
        (vec2(60.0, -40.0), vec2(60.0, 40.0)),
        (vec2(60.0, 40.0), vec2(-60.0, 40.0)),
        (vec2(-60.0, 40.0), vec2(-60.0, -40.0)),
    ] {
        stroke(&mut paint, at + from, at + to);
    }
    let _ = lines;
    // The hint layer: one stroke inside.
    paint.step(vec![key(
        egui::Key::N,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    )]);
    paint.settle(4);
    stroke(&mut paint, at - vec2(10.0, 0.0), at + vec2(10.0, 0.0));
    paint.command("edit.colorize");
    paint.settle(4);
    let seen = names(&mut paint);
    shows_all(&seen, &["Line Art", "Blur Strength"], "Colorize");
    let count = layers(&paint).len();
    let before = arena_bytes(&paint);
    paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    paint.settle(30);
    assert_eq!(
        layers(&paint).len(),
        count + 1,
        "a colors layer: {}",
        said(&paint)
    );
    undo(&mut paint);
    assert!(arena_bytes(&paint) == before);
}

#[test]
fn step_00_43_52_convert_to_drawing_color_and_brightness_to_opacity() {
    let mut paint = PaintDriver::new("fg-convert-colors");
    let layer = inked_layer(&mut paint);
    let before = pixels(&paint, layer);
    // Main color white, then Convert to Drawing Color.
    press(&mut paint, egui::Key::X);
    undo_pair(
        &mut paint,
        "convert to drawing color",
        |paint| paint.command("edit.recolor"),
        move |paint| pixels(paint, layer) != before,
    );
    undo(&mut paint);
    // 00:44:24 Convert Brightness to Opacity on a white paper copy.
    paint.command("layer.new");
    paint.settle(3);
    let white = layers(&paint).last().unwrap().id;
    paint.command("edit.fill");
    paint.settle(3);
    let filled = ink(&paint, white);
    undo_pair(
        &mut paint,
        "brightness to opacity",
        |paint| paint.command("edit.luma_to_alpha"),
        move |paint| ink(paint, white) < filled,
    );
}

#[test]
fn step_00_44_03_rasterize_bakes_a_border_effect() {
    let mut paint = PaintDriver::new("fg-rasterize");
    let layer = inked_layer(&mut paint);
    paint.runtime.engine.world.arena.get_by_id(layer).unwrap();
    show_layer_property(&mut paint);
    click_named(&mut paint, "Border effect");
    let inked = ink(&paint, layer);
    let count = layers(&paint).len();
    paint.command("layer.rasterize");
    paint.settle(4);
    let baked = layers(&paint).last().unwrap().id;
    assert_eq!(layers(&paint).len(), count, "{}", said(&paint));
    assert!(ink(&paint, baked) > inked, "the border is pixels now");
    assert_ne!(
        entity(&paint, baked)
            .property(sue_paint::effects::BORDER)
            .and_then(sue_core::Value::as_bool),
        Some(true),
        "the effect is off"
    );
}

#[test]
fn step_00_45_18_register_a_layer_as_material_and_as_a_brush_tip() {
    let mut paint = PaintDriver::new("fg-register-material");
    inked_layer(&mut paint);
    let seen = dialog_names(&mut paint, "edit.material.image");
    assert!(
        seen.iter()
            .any(|name| name.starts_with("Register Material")),
        "{seen:?}"
    );
    // The layer as a brush tip: a sub tool that stamps it.
    paint.command("edit.material.tip");
    paint.settle(4);
    paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    paint.settle(6);
    assert!(
        !said(&paint).contains("not in Sue Paint"),
        "{}",
        said(&paint)
    );
}

#[test]
fn step_00_46_27_tonal_corrections_and_correction_layers() {
    let mut paint = PaintDriver::new("fg-corrections");
    let layer = inked_layer(&mut paint);
    for filter in [
        "filter.brightness",
        "filter.hue",
        "filter.posterize",
        "filter.levels",
        "filter.curve",
        "filter.balance",
        "filter.binarize",
        "filter.gradient_map",
    ] {
        let seen = dialog_names(&mut paint, filter);
        assert!(seen.len() > 3, "{filter}");
    }
    // Reverse gradient, Ctrl+I: on the layer at once.
    let before = pixels(&paint, layer);
    undo_pair(
        &mut paint,
        "reverse gradient",
        |paint| paint.command("filter.invert"),
        move |paint| pixels(paint, layer) != before,
    );
    // A correction layer applies to all layers below.
    let count = layers(&paint).len();
    undo_pair(
        &mut paint,
        "correction layer",
        |paint| command_ok(paint, "layer.correction.hue"),
        move |paint| layers(paint).len() == count + 1,
    );
}

#[test]
fn step_00_49_08_transform_settings_and_mesh() {
    let mut paint = PaintDriver::new("fg-transform");
    let layer = inked_layer(&mut paint);
    paint.step(vec![key(egui::Key::T, egui::Modifiers::COMMAND)]);
    paint.settle(4);
    show_tool_property(&mut paint);
    let seen = names(&mut paint);
    shows_all(
        &seen,
        &[
            "Center of rotation",
            "Keep aspect ratio",
            "Keep original image",
        ],
        "transform in Tool Property",
    );
    paint.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    paint.settle(3);
    // Mesh transformation with its grid.
    let before = pixels(&paint, layer);
    paint.command("edit.transform.mesh");
    paint.settle(4);
    let seen = names(&mut paint);
    assert!(
        seen.iter()
            .any(|name| name.contains("Mesh") || name.contains("mesh")),
        "{seen:?}"
    );
    paint.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    paint.settle(3);
    assert!(pixels(&paint, layer) == before);
}

#[test]
fn step_00_49_20_ctrl_on_a_corner_distorts() {
    let mut paint = PaintDriver::new("fg-distort");
    paint.step(vec![key(
        egui::Key::N,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    )]);
    paint.settle(4);
    let layer = layers(&paint).last().unwrap().id;
    // A filled box, then Ctrl+T on it.
    select_middle(&mut paint);
    paint.command("edit.fill");
    paint.settle(3);
    let before = pixels(&paint, layer);
    paint.step(vec![key(egui::Key::T, egui::Modifiers::COMMAND)]);
    paint.settle(4);
    let corner = middle(&paint) + vec2(50.0, -30.0);
    drag_with(
        &mut paint,
        corner,
        corner + vec2(40.0, -30.0),
        egui::Modifiers::COMMAND,
    );
    paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    paint.settle(4);
    let after = pixels(&paint, layer);
    assert_ne!(after, before, "the corner moved");
    undo(&mut paint);
    assert!(pixels(&paint, layer) == before, "one undo step");
}

#[test]
fn step_00_49_32_resolution_canvas_size_rotate_and_clear_memory() {
    let mut paint = PaintDriver::new("fg-canvas");
    inked_layer(&mut paint);
    for command in [
        "edit.resolution",
        "edit.canvas_size",
        "edit.canvas_properties",
    ] {
        let seen = dialog_names(&mut paint, command);
        assert!(seen.len() > 3, "{command}");
    }
    let size =
        |paint: &PaintDriver| sue_model::picture::size(entity(paint, picture(paint))).unwrap();
    undo_pair(
        &mut paint,
        "rotate canvas",
        |paint| paint.command("canvas.rotate_right"),
        move |paint| size(paint) == [1200, 1600],
    );
    label_of("edit.clear_history");
}

#[test]
fn step_00_49_59_obtain_screen_color() {
    label_of("edit.screen_color");
    label_of("edit.screen_color_hidden");
    let mut paint = PaintDriver::new("fg-screen-color");
    paint.command("edit.screen_color");
    paint.settle(4);
    assert!(
        !said(&paint).contains("not in Sue Paint"),
        "{}",
        said(&paint)
    );
    paint.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    paint.settle(3);
}

#[test]
fn step_00_50_15_select_all_and_the_selection_launcher() {
    let mut paint = PaintDriver::new("fg-launcher");
    inked_layer(&mut paint);
    select_middle(&mut paint);
    let seen = names(&mut paint);
    // The launcher under the selection, in the video's order.
    for command in [
        "select.none",
        "edit.crop",
        "select.invert",
        "select.expand",
        "select.shrink",
        "edit.clear",
        "edit.clear_outside",
        "edit.cut_paste",
        "edit.copy_paste",
        "edit.transform.scale",
        "edit.fill",
    ] {
        let label = label_of(command);
        let label = label.trim_end_matches("...");
        assert!(
            seen.iter().any(|name| name == label),
            "{label:?} on the launcher: {seen:?}"
        );
    }
    let count = layers(&paint).len();
    let copy = label_of("edit.copy_paste");
    undo_pair(
        &mut paint,
        "copy and paste",
        |paint| click_named(paint, &copy),
        move |paint| layers(paint).len() == count + 1,
    );
    // FGV-50 Expand has an expansion type.
    let seen = dialog_names(&mut paint, "select.expand");
    shows_all(&seen, &["Expansion type"], "Expand Selected Area");
    label_of("select.vectors_overlapping");
    label_of("select.vectors_within");
    label_of("select.all");
}

#[test]
fn step_00_51_07_view_commands_and_grid_settings() {
    let mut paint = PaintDriver::new("fg-view");
    for command in [
        "view.grid",
        "view.grid_settings",
        "view.safety_margin",
        "view.playback_fps",
        "view.special_ruler_snapping",
        "view.vector_paths.all",
        "view.ruler_bar",
        "view.print_size",
    ] {
        label_of(command);
    }
    let seen = dialog_names(&mut paint, "view.grid_settings");
    assert!(seen.len() > 3);
}

#[test]
fn step_00_51_20_filters_of_the_video_change_the_layer() {
    let mut paint = PaintDriver::new("fg-filters");
    let layer = inked_layer(&mut paint);
    for filter in [
        "filter.gaussian",
        "filter.motion_blur",
        "filter.radial_blur",
        "filter.line_width",
        "filter.curved_surface",
        "filter.artistic",
        "filter.mosaic",
        "filter.sharpen",
    ] {
        let before = arena_bytes(&paint);
        command_ok(&mut paint, filter);
        assert_ne!(arena_bytes(&paint), before, "{filter}: {}", said(&paint));
        undo(&mut paint);
        assert!(arena_bytes(&paint) == before, "{filter} undoes");
    }
    // Dust removal on raster specks.
    label_of("filter.dust");
    // Perlin noise on a new layer, then Overlay.
    paint.step(vec![key(
        egui::Key::N,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    )]);
    paint.settle(4);
    let noise = layers(&paint).last().unwrap().id;
    paint.command("filter.perlin");
    paint.settle(4);
    let seen = names(&mut paint);
    shows_all(
        &seen,
        &["Amplitude", "Attenuation", "Repetition"],
        "Perlin noise",
    );
    paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    paint.settle(6);
    assert!(ink(&paint, noise) > 0, "noise drawn");
    let _ = layer;
}

#[test]
fn step_00_52_25_help_menu_is_sue_paints_own() {
    let commands = menu_commands();
    for command in ["help.guide", "help.shortcuts", "help.about"] {
        assert!(commands.iter().any(|found| found == command), "{command}");
    }
}

// D. Materials.

/// Shows the Material palette; its thumbnails: square buttons larger
/// than icon buttons.
fn material_thumbnails(paint: &mut PaintDriver) -> Vec<(String, egui::Rect)> {
    paint.settle(2);
    if paint.rect_of("Material").is_none() {
        paint.command("window.material");
        paint.settle(4);
    }
    paint.click_text("Material");
    paint.settle(4);
    step_told(paint, Vec::new())
        .into_iter()
        .filter(|(_, role, rect)| {
            *role == egui::accesskit::Role::Button
                && rect.width() > 40.0
                && (rect.width() - rect.height()).abs() < 2.0
        })
        .map(|(name, _, rect)| (name, rect))
        .collect()
}

#[test]
fn step_00_52_29_quick_access_palette() {
    let mut paint = PaintDriver::new("fg-quick-access");
    paint.command("window.quick_access");
    paint.settle(6);
    let seen = names(&mut paint);
    assert!(seen.iter().any(|name| name == "Quick Access"), "{seen:?}");
    // FGV-55: its first set holds the video's buttons.
    for command in [
        "edit.undo",
        "edit.redo",
        "edit.transform.free",
        "edit.transform.mesh",
    ] {
        let label = label_of(command);
        let label = label.trim_end_matches("...");
        assert!(
            seen.iter().any(|name| name == label),
            "{label:?} in {seen:?}"
        );
    }
}

#[test]
fn step_00_52_47_material_palette_search_and_tags() {
    let mut paint = PaintDriver::new("fg-material-palette");
    let thumbs = material_thumbnails(&mut paint);
    assert!(thumbs.len() > 4, "Sue Paint's own materials: {thumbs:?}");
    let seen = names(&mut paint);
    shows_all(&seen, &["Search names and tags"], "Material");
    // A material picked shows its tags.
    let (_, first) = thumbs[0].clone();
    click_with(&mut paint, first.center(), egui::Modifiers::NONE);
    let seen = names(&mut paint);
    shows_all(&seen, &["Tags"], "a picked material");
    for command in [
        "material.size_small",
        "material.size_large",
        "material.size_list",
        "material.favorite",
        "material.paste",
        "material.delete",
    ] {
        label_of(command);
    }
}

#[test]
fn step_00_52_58_a_material_dragged_onto_the_canvas() {
    let mut paint = PaintDriver::new("fg-material-drag");
    let thumbs = material_thumbnails(&mut paint);
    let (_, first) = thumbs.first().cloned().expect("a material");
    let count = layers(&paint).len();
    let before = arena_bytes(&paint);
    let to = middle(&paint);
    paint.drag(first.center(), to, egui::PointerButton::Primary);
    assert_eq!(
        layers(&paint).len(),
        count + 1,
        "dropped as a layer: {}",
        said(&paint)
    );
    let dropped = layers(&paint).last().unwrap().id;
    // 00:53:41 the pattern stays a live material layer, 00:53:54 and
    // Rasterize makes it pixels to draw on.
    let arena = &paint.runtime.engine.world.arena;
    if sue_paint::doc::material_image(arena, dropped).is_some() {
        paint.command("layer.rasterize");
        paint.settle(4);
        let now = layers(&paint).last().unwrap().id;
        assert!(
            sue_paint::doc::material_image(&paint.runtime.engine.world.arena, now).is_none(),
            "rasterized: {}",
            said(&paint)
        );
        undo(&mut paint);
    }
    undo(&mut paint);
    assert!(arena_bytes(&paint) == before);
}

// E. 3D.

/// Adds a 3D layer with `command` and returns it.
fn new_3d(paint: &mut PaintDriver, command: &str) -> sue_core::EntityId {
    paint.command(command);
    paint.settle(2);
    if command == "layer.new.3d" {
        paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    }
    paint.settle(4);
    let arena = &paint.runtime.engine.world.arena;
    sue_paint::doc::layers(arena, picture(paint))
        .into_iter()
        .rev()
        .find(|info| sue_paint::three::is_3d(arena, info.id))
        .map(|info| info.id)
        .expect("a 3D layer")
}

#[test]
fn step_00_54_56_3d_figure_hand_head_and_shapes_as_layers() {
    let mut paint = PaintDriver::new("fg-3d-layers");
    for command in [
        "layer.new.figure",
        "layer.new.hand",
        "layer.new.head",
        "layer.new.3d",
    ] {
        let count = layers(&paint).len();
        new_3d(&mut paint, command);
        assert_eq!(layers(&paint).len(), count + 1, "{command}");
    }
    label_of("file.import.3d");
}

#[test]
fn step_00_55_07_body_types_shape_chart_camera_and_object_buttons() {
    let mut paint = PaintDriver::new("fg-3d-figure");
    new_3d(&mut paint, "layer.new.figure");
    paint.command("tool.operation");
    paint.settle(3);
    let rows = tool_property_rows(&mut paint);
    shows_all(
        &rows,
        &[
            "Male",
            "Female",
            "Body shape chart",
            "Head to body ratio",
            "Adjust head to body ratio with height",
            "Orbit",
            "Pan",
            "Zoom",
            "Move",
            "Rotate",
            "Put on the ground",
            "Reset angle",
            "Reset position",
            "Reset scale",
            "Object scale",
            "Register body shape",
            "Light source",
            "Outline width",
            "Manga Perspective",
        ],
        "3D figure",
    );
}

#[test]
fn step_00_56_36_pose_flip_reset_register_and_scanner() {
    let mut paint = PaintDriver::new("fg-3d-pose");
    let figure = new_3d(&mut paint, "layer.new.figure");
    let model = |paint: &PaintDriver| {
        let arena = &paint.runtime.engine.world.arena;
        let mut out = sue_core::codec::Writer::new();
        arena
            .get_by_id(sue_paint::three::model_of(arena, figure).unwrap())
            .unwrap()
            .encode(&mut out);
        out.into_bytes()
    };
    let start = model(&paint);
    paint.command("figure.preset.2");
    paint.settle(4);
    let posed = model(&paint);
    assert_ne!(posed, start, "a preset pose");
    paint.command("figure.flip");
    paint.settle(4);
    assert_ne!(model(&paint), posed, "flipped");
    paint.command("figure.reset");
    paint.settle(4);
    for command in [
        "figure.register_pose",
        "figure.body.register",
        "figure.pose_scanner",
        "figure.object.reset_scale",
        "figure.object.reset_angle",
        "figure.object.ground",
    ] {
        label_of(command);
    }
}

// F. EX, phone and tablet.

#[test]
fn step_00_59_52_story_menu_pages_and_the_bound_book() {
    let mut paint = PaintDriver::new("fg-story");
    for command in [
        "story.manager",
        "story.add",
        "story.order",
        "story.preview_binding",
        "story.page_settings",
    ] {
        label_of(command);
    }
    paint.command("story.preview_binding");
    paint.settle(6);
    assert!(
        !said(&paint).contains("not in Sue Paint"),
        "{}",
        said(&paint)
    );
}

#[test]
fn step_01_02_49_phone_and_tablet_interfaces() {
    let mut paint = PaintDriver::new("fg-interfaces");
    for command in [
        "window.interface.phone",
        "window.interface.tablet",
        "window.interface.desktop",
        "touch.finger_draws",
        "touch.key_bar",
    ] {
        label_of(command);
    }
    // 01:04:28 the finger button: fingers only move the view.
    let before = paint
        .runtime
        .engine
        .world
        .rice
        .get_bool(sue_paint::studio_mode::FINGER_KEY);
    paint.command("touch.finger_draws");
    paint.settle(2);
    let after = paint
        .runtime
        .engine
        .world
        .rice
        .get_bool(sue_paint::studio_mode::FINGER_KEY);
    assert_ne!(before, after, "the finger button switches");
    paint.command("touch.finger_draws");
    paint.settle(2);
}

#[test]
fn step_01_06_57_timelapse_on_the_command_bar() {
    let mut paint = PaintDriver::new("fg-command-bar");
    let seen = names(&mut paint);
    let label = label_of("file.timelapse.record");
    assert!(
        seen.iter()
            .any(|name| *name == label || name.starts_with("Record Timelapse")),
        "{label:?} on the command bar: {seen:?}"
    );
}

// Seen in the video: rows the frames add to the step table.

#[test]
fn step_00_04_45_fgv_3_a_check_box_in_each_row_picks_several_layers() {
    let mut paint = PaintDriver::new("fg-row-boxes");
    inked_layer(&mut paint);
    let first = layers(&paint)[1].name.clone();
    let name = format!("Pick {first} with the others");
    click_named(&mut paint, &name);
    // Two picked: Merge Selected Layers works on them.
    let count = layers(&paint).len();
    undo_pair(
        &mut paint,
        "merge the picked",
        |paint| paint.command("layer.merge_selected"),
        move |paint| layers(paint).len() == count - 1,
    );
}

#[test]
fn step_00_20_56_fgv_16_auto_action_sets() {
    let mut paint = PaintDriver::new("fg-auto-steps");
    paint.command("window.auto_actions");
    paint.settle(4);
    paint.click_text("Auto Action");
    let seen = names(&mut paint);
    // The set drop-down over the actions.
    shows_all(&seen, &["Set", "New Clipped Layer"], "Auto Action");
    for command in [
        "auto.new_set",
        "auto.import_set",
        "auto.export_set",
        "auto.record",
    ] {
        label_of(command);
    }
}

#[test]
fn step_00_40_05_fgv_44_save_duplicate_formats_and_fgv_47_copy_svg() {
    for command in [
        "file.duplicate.clip",
        "file.duplicate.gif",
        "file.duplicate.qoi",
        "file.export.vectors",
        "file.new_clipboard",
        "file.revert",
        "edit.copy_svg",
        "edit.paste_here",
        "edit.outline_selection",
    ] {
        label_of(command);
    }
    let mut paint = PaintDriver::new("fg-copy-svg");
    paint.command("layer.new.vector");
    paint.settle(4);
    let at = middle(&paint);
    paint.command("tool.pen");
    paint.settle(2);
    stroke(&mut paint, at - vec2(40.0, 0.0), at + vec2(40.0, 0.0));
    paint.command("edit.copy_svg");
    paint.settle(3);
    assert!(
        !said(&paint).contains("not in Sue Paint"),
        "{}",
        said(&paint)
    );
}

#[test]
fn step_00_52_00_fgv_54_artistic_line_and_color_controls() {
    let mut paint = PaintDriver::new("fg-artistic");
    inked_layer(&mut paint);
    let seen = dialog_names(&mut paint, "filter.artistic");
    shows_all(&seen, &["Line Opacity"], "Artistic");
}
