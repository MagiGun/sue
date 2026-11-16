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

/// The layer named `name`.
fn layer_named(paint: &PaintDriver, name: &str) -> sue_core::EntityId {
    layers(paint)
        .into_iter()
        .find(|info| info.name == name)
        .map(|info| info.id)
        .unwrap_or_else(|| {
            panic!(
                "a layer {name:?} among {:?}",
                layers(paint)
                    .iter()
                    .map(|l| l.name.clone())
                    .collect::<Vec<_>>()
            )
        })
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
        .chunks_exact(4)
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

/// Picks `tool` (a `tool.pick.*` or `tool.*` command), then the sub tool
/// named `sub` in the Sub Tool palette.
fn pick_sub_tool(paint: &mut PaintDriver, tool: &str, sub: &str) {
    paint.command(tool);
    paint.settle(3);
    paint.settle(2);
    paint.click_text("Sub Tool");
    if paint.rect_of(sub).is_none() {
        // A second group's tab hides it: its name is a tab.
        paint.settle(2);
    }
    paint.click_text(sub);
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
    let before = layers(&paint).len();
    paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    paint.settle(4);
    assert_eq!(layers(&paint).len(), before + 1);
    let tone = layers(&paint).last().unwrap().id;
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
