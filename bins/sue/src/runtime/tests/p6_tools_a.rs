//! Phase 6, Tool Property of the view, pick and move tools, the
//! transform's settings and Change Canvas Size's box on the canvas: the
//! gaps the tutorials' acceptance tables left open. Each test is named
//! after the tutorial step or gap it closes (`fg_` the full guide, `ct_`
//! the crash course tips, then the time in the video or the gap's id);
//! nothing of the videos is quoted.
//!
//! Every test drives real frames: it picks the tool and its sub tool as
//! a person does, sets the row in Tool Property by clicking what the
//! frame drew, works on the canvas and reads what changed in the
//! document. A change to the document is undone and redone and compared
//! with the arena before and after.

use super::*;
use egui::{Event, Key, Modifiers, PointerButton, pos2};

// Helpers.

/// The whole arena as bytes, so two states compare exactly.
fn arena_bytes(paint: &PaintDriver) -> Vec<u8> {
    let mut out = sue_core::codec::Writer::new();
    paint.runtime.engine.world.arena.encode(&mut out);
    out.into_bytes()
}

/// The picture Sue Paint has open.
fn picture(paint: &PaintDriver) -> sue_core::EntityId {
    paint
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.focus())
        .expect("a canvas is open")
}

/// The open picture's layers, bottom first.
fn layers(paint: &PaintDriver) -> Vec<sue_paint::doc::LayerInfo> {
    sue_paint::doc::layers(&paint.runtime.engine.world.arena, picture(paint))
}

/// The top layer.
fn top(paint: &PaintDriver) -> sue_core::EntityId {
    layers(paint).last().expect("a layer").id
}

/// The entity `id`.
fn entity(paint: &PaintDriver, id: sue_core::EntityId) -> &sue_core::Entity {
    paint.runtime.engine.world.arena.get_by_id(id).unwrap()
}

/// A number property of `id`, 0 where it has none.
fn number(paint: &PaintDriver, id: sue_core::EntityId, key: &str) -> f64 {
    entity(paint, id)
        .property(key)
        .and_then(sue_core::Value::as_f64)
        .unwrap_or(0.0)
}

/// Where `layer` sits.
fn offset(paint: &PaintDriver, layer: sue_core::EntityId) -> [i64; 2] {
    sue_model::picture::LayerStyle::of(entity(paint, layer)).offset
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

/// Picture pixel `p` of `layer` (where the layer sits), straight RGBA.
fn pixel(paint: &PaintDriver, layer: sue_core::EntityId, p: [i64; 2]) -> [u8; 4] {
    let [ox, oy] = offset(paint, layer);
    let raster = sue_paint::doc::raster(&paint.runtime.engine.world.arena, layer).unwrap();
    let (x, y) = (p[0] - ox, p[1] - oy);
    if x < 0 || y < 0 || x >= i64::from(raster.width()) || y >= i64::from(raster.height()) {
        return [0; 4];
    }
    let at = ((y * i64::from(raster.width()) + x) * 4) as usize;
    let bytes = raster.bytes().unwrap();
    [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
}

/// The picture's size in pixels.
fn picture_size(paint: &PaintDriver) -> [u32; 2] {
    sue_model::picture::size(entity(paint, picture(paint))).expect("a size")
}

/// The middle of the canvas on screen.
fn middle(paint: &PaintDriver) -> Pos2 {
    paint_middle(&paint.runtime)
}

/// The window's zoom, from its title ("... 65.3%) - Sue Paint").
fn zoom(paint: &PaintDriver) -> f32 {
    let title = paint
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.window_title(&paint.runtime.engine.world.arena))
        .expect("Sue Paint names its window");
    title
        .split_whitespace()
        .find_map(|word| {
            word.trim_end_matches(')')
                .strip_suffix('%')?
                .parse::<f32>()
                .ok()
        })
        .unwrap_or_else(|| panic!("a zoom in {title:?}"))
}

/// The picture pixel under screen point `at`, as the canvas maps it
/// while the view is unturned: from the picture's middle at the canvas's
/// middle, one screen point being `1 / zoom` pixels.
fn picture_at(paint: &PaintDriver, at: Pos2) -> [i64; 2] {
    let [w, h] = picture_size(paint);
    let scale = zoom(paint) / 100.0;
    let from = middle(paint);
    [
        (f64::from(w) / 2.0 + f64::from((at.x - from.x) / scale)).floor() as i64,
        (f64::from(h) / 2.0 + f64::from((at.y - from.y) / scale)).floor() as i64,
    ]
}

/// One frame with `events` while `modifiers` are held; every control the
/// frame named for assistive technology, with its place.
fn step_told(
    paint: &mut PaintDriver,
    events: Vec<Event>,
    modifiers: Modifiers,
) -> Vec<(String, egui::Rect)> {
    paint.ctx.enable_accesskit();
    let mut all = vec![Event::ModifiersChanged(modifiers)];
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

/// Where the control named `name` is drawn now, the topmost if several.
fn named(paint: &mut PaintDriver, name: &str) -> Option<egui::Rect> {
    step_told(paint, Vec::new(), Modifiers::NONE)
        .into_iter()
        .filter(|(label, rect)| label == name && rect.width() > 0.0)
        .map(|(_, rect)| rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
}

/// A press and release at `at` with `modifiers` held, then frames until
/// things settle.
fn click_with(paint: &mut PaintDriver, at: Pos2, modifiers: Modifiers) {
    let press = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    };
    step_told(paint, vec![Event::PointerMoved(at)], modifiers);
    step_told(paint, vec![press(true)], modifiers);
    step_told(paint, vec![press(false)], modifiers);
    step_told(paint, Vec::new(), modifiers);
    paint.settle(6);
}

/// A click on the control named `name`.
fn click_named(paint: &mut PaintDriver, name: &str) {
    let at = named(paint, name)
        .unwrap_or_else(|| panic!("{name:?} is on screen: {:?}", paint.texts))
        .center();
    click_with(paint, at, Modifiers::NONE);
}

/// A click on the canvas at `at`.
fn click(paint: &mut PaintDriver, at: Pos2) {
    click_with(paint, at, Modifiers::NONE);
}

/// A drag of the current tool from `from` to `to` on the canvas.
fn stroke(paint: &mut PaintDriver, from: Pos2, to: Pos2) {
    paint.drag(from, to, PointerButton::Primary);
}

/// Undo, as Ctrl+Z does.
fn undo(paint: &mut PaintDriver) {
    paint.command("edit.undo");
    paint.settle(4);
}

/// Redo, as Ctrl+Y does.
fn redo(paint: &mut PaintDriver) {
    paint.command("edit.redo");
    paint.settle(4);
}

/// Runs `change`, which must make `check` true and change the
/// document; undo gives back the arena before, redo the one after.
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
    assert!(check(paint), "{what}: so after");
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

/// A new raster layer with a pen stroke from `from` to `to` (screen);
/// returns it.
fn inked(paint: &mut PaintDriver, from: Pos2, to: Pos2) -> sue_core::EntityId {
    let known: Vec<sue_core::EntityId> = layers(paint).into_iter().map(|info| info.id).collect();
    paint.command("layer.new");
    paint.settle(3);
    let layer = layers(paint)
        .into_iter()
        .map(|info| info.id)
        .find(|id| !known.contains(id))
        .expect("a new layer");
    paint.command("tool.pen");
    paint.settle(2);
    stroke(paint, from, to);
    assert!(
        ink(paint, layer) > 0,
        "the stroke landed: {:?} {:?}",
        paint.runtime.shell.message(),
        layers(paint)
            .iter()
            .map(|info| (info.id, info.kind, info.locked, info.style.visible))
            .collect::<Vec<_>>()
    );
    layer
}

/// Picks the sub tool `sub` of the tool `tool` (a command) in the Sub
/// Tool palette, then shows its Tool Property.
fn pick_sub(paint: &mut PaintDriver, tool: &str, sub: &str) {
    paint.command(tool);
    paint.settle(3);
    paint.click_text("Sub Tool");
    paint.click_text(sub);
    paint.click_text("Tool Property");
    paint.settle(2);
}

/// Where `text` is drawn in Tool Property now (under its tab).
fn in_tool_property(paint: &PaintDriver, text: &str) -> Option<egui::Rect> {
    let tab = paint.rect_of("Tool Property")?;
    paint
        .texts
        .iter()
        .filter(|(found, rect)| {
            found == text
                && rect.top() > tab.bottom()
                && (rect.center().x - tab.center().x).abs() < 300.0
        })
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
}

/// Asserts Tool Property shows each of `rows`.
fn shows_rows(paint: &mut PaintDriver, rows: &[&str]) {
    paint.settle(2);
    for row in rows {
        assert!(
            in_tool_property(paint, row).is_some() || named(paint, row).is_some(),
            "{row} in Tool Property: {:?}",
            paint.texts
        );
    }
}

/// Picks `wanted` in Tool Property's row `row`: a button of its strip,
/// or, when the row is a drop-down, the drop-down opened first.
fn choose(paint: &mut PaintDriver, row: &str, wanted: &str) {
    paint.settle(2);
    // A strip: the choice on the row's own line.
    let line = in_tool_property(paint, row);
    let beside = line.and_then(|line| {
        paint
            .texts
            .iter()
            .filter(|(found, rect)| {
                found == wanted
                    && (rect.center().y - line.center().y).abs() < 12.0
                    && rect.left() > line.left()
            })
            .map(|(_, rect)| *rect)
            .next()
    });
    if let Some(rect) = beside {
        click_with(paint, rect.center(), Modifiers::NONE);
        return;
    }
    // The drop-down is named after its row; the topmost one in Tool
    // Property is the one.
    let tab = paint.rect_of("Tool Property").expect("Tool Property");
    let list = step_told(paint, Vec::new(), Modifiers::NONE)
        .into_iter()
        .filter(|(label, rect)| label == row && rect.top() > tab.bottom() && rect.width() > 0.0)
        .map(|(_, rect)| rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("the row {row:?} in Tool Property: {:?}", paint.texts));
    click_with(paint, list.center(), Modifiers::NONE);
    // The choice in the list that opened, the one nearest the drop-down
    // (a menu of the same name sits far above).
    let item = paint
        .texts
        .iter()
        .filter(|(found, rect)| found == wanted && rect.top() > list.top() - 240.0)
        .map(|(_, rect)| *rect)
        .min_by(|a, b| {
            a.center()
                .distance(list.center())
                .total_cmp(&b.center().distance(list.center()))
        })
        .unwrap_or_else(|| panic!("{wanted:?} in the list: {:?}", paint.texts));
    click_with(paint, item.center(), Modifiers::NONE);
}

/// Where the last frame drew the canvas mark `id`.
fn mark(paint: &PaintDriver, id: &str) -> Pos2 {
    let wanted = format!("mark.{id}");
    sue_paint::shown_buttons(&paint.ctx)
        .into_iter()
        .find(|(found, _)| *found == wanted)
        .unwrap_or_else(|| panic!("{wanted} is drawn"))
        .1
        .center()
}

// The full guide, 00:22:19 to 00:23:22: the view, operation and move
// tools' settings.

/// 00:22:19, FG-27: the Zoom tool's Click and Drag rows. Drag set to
/// zoom to an area fills the view with the box dragged out; set to
/// nothing a drag leaves the zoom alone while a click still zooms.
#[test]
fn fg_00_22_19_zoom_by_drag_by_area_and_not_at_all() {
    let mut paint = PaintDriver::new("p6a-zoom");
    pick_sub(&mut paint, "tool.zoom", "Zoom In");
    shows_rows(&mut paint, &["Click", "Drag"]);
    // As it goes: a drag to the right zooms in.
    let at = middle(&paint);
    let before = zoom(&paint);
    stroke(&mut paint, at, at + vec2(140.0, 0.0));
    let scrubbed = zoom(&paint);
    assert!(scrubbed > before * 1.5, "{before} to {scrubbed}");
    paint.command("view.fit");
    paint.settle(3);
    // To the area: a box a quarter of the view wide fills it.
    choose(&mut paint, "Drag", "Zoom to the area");
    let before = zoom(&paint);
    stroke(&mut paint, at - vec2(60.0, 40.0), at + vec2(60.0, 40.0));
    let boxed = zoom(&paint);
    assert!(
        boxed > before * 2.0,
        "the box fills the view: {before} to {boxed}"
    );
    // Nothing: a drag leaves the zoom; a click zooms by half again.
    choose(&mut paint, "Drag", "Nothing");
    let before = zoom(&paint);
    stroke(&mut paint, at, at + vec2(140.0, 0.0));
    assert!((zoom(&paint) - before).abs() < 0.05, "the drag did nothing");
    click(&mut paint, at);
    assert!(
        (zoom(&paint) / before - 1.5).abs() < 0.05,
        "a click still zooms: {before} to {}",
        zoom(&paint)
    );
    // Click set to zoom out: the same click now zooms out.
    choose(&mut paint, "Click", "Zoom out");
    let before = zoom(&paint);
    click(&mut paint, at);
    assert!(zoom(&paint) < before, "{before} to {}", zoom(&paint));
}

/// 00:22:51, FGV-17: Object's Operation of transparent part and
/// Selectable object. A press on the current layer's clear part moves
/// it, picks and moves the layer painted there, or does nothing; with
/// raster layers unticked Object leaves them alone.
#[test]
fn fg_00_22_51_object_transparent_part_and_selectable_object() {
    let mut paint = PaintDriver::new("p6a-object");
    let at = middle(&paint);
    let lower = inked(&mut paint, at - vec2(60.0, 0.0), at + vec2(60.0, 0.0));
    let upper = inked(&mut paint, at + vec2(-60.0, 80.0), at + vec2(60.0, 80.0));
    pick_sub(&mut paint, "tool.operation", "Object");
    shows_rows(
        &mut paint,
        &[
            "Clear part",
            "Picks",
            "Raster layers",
            "Vector lines",
            "Camera",
        ],
    );
    // Picks the layer painted there: a drag on the lower line, clear on
    // the upper layer, moves the lower layer and makes it current.
    choose(&mut paint, "Clear part", "Picks the layer painted there");
    undo_pair(
        &mut paint,
        "Object picks the layer under a clear part",
        |paint| stroke(paint, at, at + vec2(0.0, 40.0)),
        move |paint| offset(paint, lower)[1] > 10,
    );
    assert_eq!(offset(&paint, upper), [0, 0], "the current layer stayed");
    // Nothing: the same drag on a clear part moves nothing.
    paint.command("layer.select_above");
    paint.settle(2);
    choose(&mut paint, "Clear part", "Nothing");
    let before = arena_bytes(&paint);
    stroke(&mut paint, at + vec2(0.0, 40.0), at + vec2(30.0, 40.0));
    assert!(arena_bytes(&paint) == before, "nothing moved");
    // Raster layers unticked: a drag on the upper layer's own paint
    // moves nothing either.
    choose(&mut paint, "Clear part", "Moves the layer");
    paint.click_text("Raster layers");
    let before = arena_bytes(&paint);
    stroke(&mut paint, at + vec2(0.0, 80.0), at + vec2(30.0, 80.0));
    assert!(
        arena_bytes(&paint) == before,
        "raster layers are not picked"
    );
    paint.click_text("Raster layers");
    undo_pair(
        &mut paint,
        "Object moves the raster layer again",
        |paint| stroke(paint, at + vec2(0.0, 80.0), at + vec2(30.0, 80.0)),
        move |paint| offset(paint, upper)[0] > 5,
    );
}

/// 00:22:57, FG-26: Select layer's Exclude row. A click on a draft
/// layer's paint picks it; with draft layers left out it picks the layer
/// under it. A box picks every layer painted in it.
#[test]
fn fg_00_22_57_select_layer_by_drag_and_exclude_draft() {
    let mut paint = PaintDriver::new("p6a-select-layer");
    let at = middle(&paint);
    let lower = inked(&mut paint, at - vec2(60.0, 0.0), at + vec2(60.0, 0.0));
    let draft = inked(&mut paint, at - vec2(60.0, 0.0), at + vec2(60.0, 0.0));
    paint.command("layer.toggle.draft");
    paint.settle(3);
    assert!(sue_model::picture::LayerStyle::of(entity(&paint, draft)).draft);
    pick_sub(&mut paint, "tool.operation", "Select layer");
    shows_rows(
        &mut paint,
        &[
            "Exclude",
            "Leave out draft layers",
            "Leave out locked layers",
        ],
    );
    let current = |paint: &mut PaintDriver| {
        // Lock the current layer and see which one it was.
        paint.command("layer.toggle.lock");
        paint.settle(3);
        let locked: Vec<sue_core::EntityId> = layers(paint)
            .into_iter()
            .filter(|info| info.locked)
            .map(|info| info.id)
            .collect();
        paint.command("layer.toggle.lock");
        paint.settle(3);
        locked
    };
    paint.command("layer.select_below");
    paint.settle(2);
    click(&mut paint, at);
    assert_eq!(
        current(&mut paint),
        vec![draft],
        "the draft's paint is on top"
    );
    click_named(&mut paint, "Leave out draft layers");
    paint.command("layer.select_above");
    paint.settle(2);
    click(&mut paint, at);
    assert_eq!(current(&mut paint), vec![lower], "the draft is left out");
    // A box over both lines picks every layer painted in it but the
    // draft: merging the picked layers leaves the count as it was less
    // none, so pick with drafts in again.
    click_named(&mut paint, "Leave out draft layers");
    let count = layers(&paint).len();
    stroke(&mut paint, at - vec2(100.0, 30.0), at + vec2(100.0, 30.0));
    paint.command("layer.merge_selected");
    paint.settle(4);
    assert_eq!(layers(&paint).len(), count - 1, "both were picked");
}

/// 00:23:16, FGV-19: the Light table and Edit timeline sub tools' rows
/// work. Edit timeline's click goes back a frame when set so, and its
/// drag can do nothing; the Light table's drag turns when set so, and a
/// click puts its layers back.
#[test]
fn fg_00_23_16_light_table_and_edit_timeline_settings() {
    let mut paint = PaintDriver::new("p6a-light-table");
    let at = middle(&paint);
    paint.command("layer.new");
    paint.settle(2);
    let reference = top(&paint);
    paint.new_timeline();
    let ids = |paint: &PaintDriver| -> Vec<sue_core::EntityId> {
        layers(paint).into_iter().map(|info| info.id).collect()
    };
    let known = ids(&paint);
    paint.command("anim.cel.new");
    paint.settle(2);
    let first = *ids(&paint)
        .iter()
        .find(|id| !known.contains(id))
        .expect("a cel at frame 1");
    for _ in 0..3 {
        paint.command("anim.next");
    }
    let known = ids(&paint);
    paint.command("anim.cel.new");
    paint.settle(2);
    let fourth = *ids(&paint)
        .iter()
        .find(|id| !known.contains(id))
        .expect("a cel at frame 4");
    // Edit timeline, Click set to the previous frame: three clicks from
    // frame 4 go back to frame 1, whose cel the pen then draws on.
    pick_sub(&mut paint, "tool.operation", "Edit timeline");
    shows_rows(&mut paint, &["Click", "Drag"]);
    choose(&mut paint, "Click", "Previous frame");
    for _ in 0..3 {
        click(&mut paint, at);
    }
    // Drag set to nothing: a long drag stays on frame 1.
    choose(&mut paint, "Drag", "Nothing");
    stroke(&mut paint, at, at + vec2(200.0, 0.0));
    paint.command("tool.pen");
    paint.settle(2);
    stroke(&mut paint, at - vec2(40.0, 0.0), at + vec2(40.0, 0.0));
    assert!(ink(&paint, first) > 0, "frame 1's cel is drawn on");
    assert_eq!(ink(&paint, fourth), 0, "frame 4's cel is not");
    // The Light table: the reference layer on the general light table.
    paint.command("layer.select_below");
    paint.command("anim.light_table.register_general");
    paint.settle(2);
    paint.command("anim.first");
    paint.settle(2);
    pick_sub(&mut paint, "tool.operation", "Light table");
    shows_rows(&mut paint, &["Drag", "Click"]);
    choose(&mut paint, "Drag", "Rotate");
    let place = |paint: &PaintDriver| sue_paint::anim::Place::of(entity(paint, reference));
    undo_pair(
        &mut paint,
        "the light table's drag turns",
        |paint| stroke(paint, at + vec2(80.0, 0.0), at + vec2(0.0, 80.0)),
        move |paint| place(paint).degrees.abs() > 30.0,
    );
    // Click set to reset: a click puts the layer back where it was.
    choose(&mut paint, "Click", "Reset position");
    undo_pair(
        &mut paint,
        "the light table's click resets",
        |paint| click(paint, at + vec2(20.0, 20.0)),
        move |paint| place(paint) == sue_paint::anim::Place::default(),
    );
}

/// 00:23:22, FGV-18 and FG-27: Move layer's Object to move. Tone dots
/// move while the layer's paint stays; the grid moves; and Move layers
/// in the selection moves every layer painted inside it, whole.
#[test]
fn fg_00_23_22_move_tone_pattern_grid_and_layers_in_selection() {
    let mut paint = PaintDriver::new("p6a-move");
    let at = middle(&paint);
    // A layer filled and shown as tone.
    paint.command("layer.new");
    paint.settle(2);
    let toned = top(&paint);
    paint.command("edit.fill");
    paint.settle(2);
    paint.command("layer.properties");
    paint.settle(3);
    click_named(&mut paint, "Tone");
    assert_eq!(
        entity(&paint, toned)
            .property(sue_paint::effects::TONE)
            .and_then(sue_core::Value::as_bool),
        Some(true)
    );
    pick_sub(&mut paint, "tool.move_layer", "Move Layer");
    shows_rows(
        &mut paint,
        &[
            "Moves",
            "Move what is clicked",
            "Move layers in the selection",
        ],
    );
    choose(&mut paint, "Moves", "Tone dots");
    let paint_before = pixels(&paint, toned);
    undo_pair(
        &mut paint,
        "Move tone dots",
        |paint| stroke(paint, at, at + vec2(30.0, 20.0)),
        move |paint| number(paint, toned, sue_paint::effects::TONE_DOT_X).abs() > 1.0,
    );
    assert!(pixels(&paint, toned) == paint_before, "the paint stayed");
    assert_eq!(offset(&paint, toned), [0, 0], "the layer stayed");
    // The grid.
    choose(&mut paint, "Moves", "Grid");
    let page = picture(&paint);
    undo_pair(
        &mut paint,
        "Move grid",
        |paint| stroke(paint, at, at + vec2(25.0, 15.0)),
        move |paint| number(paint, page, sue_paint::grid::SHIFT_X).abs() > 1.0,
    );
    // Back to layers: two lines side by side, a selection over the left
    // half of each; with Move layers in the selection both move whole.
    choose(&mut paint, "Moves", "Layer");
    paint.command("layer.delete");
    paint.settle(2);
    let left = inked(&mut paint, at + vec2(-70.0, -30.0), at + vec2(-10.0, -30.0));
    let right = inked(&mut paint, at + vec2(10.0, 30.0), at + vec2(70.0, 30.0));
    let line = ink(&paint, right);
    paint.command("tool.selection");
    paint.settle(2);
    stroke(&mut paint, at + vec2(-90.0, -50.0), at + vec2(30.0, 50.0));
    pick_sub(&mut paint, "tool.move_layer", "Move Layer");
    paint.click_text("Move layers in the selection");
    undo_pair(
        &mut paint,
        "Move layers in the selection",
        |paint| stroke(paint, at, at + vec2(0.0, 50.0)),
        move |paint| offset(paint, left)[1] > 10 && offset(paint, right)[1] > 10,
    );
    assert_eq!(ink(&paint, right), line, "the right line moved whole");
}

/// 00:27:25 (FGV-22) and the crash course's tip 12 (V-4): the
/// Eyedropper's Exclude row. Obtain Display Color picks what shows; with
/// draft layers left out it picks what is under the draft.
#[test]
fn fg_00_27_25_and_ct_v4_eyedropper_exclude_row() {
    let mut paint = PaintDriver::new("p6a-eyedropper");
    let at = middle(&paint);
    // A layer all black below; a white box on a draft layer over it.
    paint.command("layer.new");
    paint.settle(2);
    paint.command("edit.fill");
    paint.settle(2);
    paint.command("layer.new");
    paint.settle(2);
    paint.command("layer.toggle.draft");
    paint.settle(2);
    paint.command("tool.selection");
    paint.settle(2);
    stroke(&mut paint, at - vec2(60.0, 40.0), at + vec2(60.0, 40.0));
    paint.command("color.swap");
    paint.settle(2);
    paint.command("edit.fill");
    paint.settle(2);
    paint.command("select.none");
    paint.command("color.swap");
    paint.settle(2);
    pick_sub(&mut paint, "tool.eyedropper", "Obtain Display Color");
    shows_rows(&mut paint, &["Exclude", "Leave out draft layers"]);
    // The color the pen draws with after a pick at the line's middle.
    let picked = |paint: &mut PaintDriver| -> [u8; 3] {
        click(paint, at);
        let layer = inked(paint, at + vec2(-40.0, 90.0), at + vec2(40.0, 90.0));
        let p = picture_at(paint, at + vec2(0.0, 90.0));
        let found = pixel(paint, layer, p);
        paint.command("layer.delete");
        paint.settle(2);
        paint.command("tool.eyedropper");
        paint.settle(2);
        [found[0], found[1], found[2]]
    };
    assert!(picked(&mut paint)[0] > 200, "the white draft shows");
    click_named(&mut paint, "Leave out draft layers");
    let under = picked(&mut paint);
    assert!(under[0] < 50, "the black line under it: {under:?}");
    // Obtain Layer Color offers which layers it reads.
    pick_sub(&mut paint, "tool.eyedropper", "Obtain Layer Color");
    shows_rows(&mut paint, &["Refer multiple"]);
}

// The full guide, 00:49: transforms and the canvas size.

/// 00:49:17, FGV-49: while transforming, Tool Property holds the
/// transform's settings: the scale and the angle as numbers and the
/// interpolation method. Hard edges copies the layer's own pixels: no
/// new shade of alpha appears.
#[test]
fn fg_00_49_17_transform_interpolation_scale_and_angle_numbers() {
    let mut paint = PaintDriver::new("p6a-transform");
    let at = middle(&paint);
    let layer = inked(&mut paint, at - vec2(40.0, 10.0), at + vec2(40.0, 10.0));
    let shades = |paint: &PaintDriver| {
        let mut seen: Vec<u8> = pixels(paint, layer)
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| pixel[3])
            .collect();
        seen.sort_unstable();
        seen.dedup();
        seen
    };
    let before = shades(&paint);
    let was = ink(&paint, layer);
    paint.command("edit.transform.scale");
    paint.settle(3);
    paint.click_text("Tool Property");
    shows_rows(
        &mut paint,
        &[
            "Mode",
            "Center",
            "Scale W",
            "Scale H",
            "Keep aspect ratio",
            "Angle",
            "Interpolation",
            "Keep original image",
        ],
    );
    choose(&mut paint, "Interpolation", "Hard edges");
    let field = named(&mut paint, "Scale W").expect("the Scale W field");
    super::paint_anim::type_into(&mut paint, field, "200");
    paint.settle(2);
    // Keep aspect ratio: the height followed.
    let height = named(&mut paint, "Scale H").expect("the Scale H field");
    let shown: Vec<&String> = paint
        .texts
        .iter()
        .filter(|(_, rect)| rect.intersects(height.expand(2.0)))
        .map(|(text, _)| text)
        .collect();
    assert!(
        shown.iter().any(|text| text.starts_with("200")),
        "the height follows: {shown:?}"
    );
    let field = named(&mut paint, "Angle").expect("the Angle field");
    super::paint_anim::type_into(&mut paint, field, "90");
    click_named(&mut paint, "OK");
    paint.settle(4);
    let now = ink(&paint, layer);
    assert!(now > was * 3, "four times the paint: {was} to {now}");
    let after = shades(&paint);
    assert!(
        after.iter().all(|shade| before.contains(shade)),
        "hard edges make no new shade: {before:?} then {after:?}"
    );
    // Turned a quarter: the line now runs down, taller than wide.
    let raster = sue_paint::doc::raster(&paint.runtime.engine.world.arena, layer).unwrap();
    let (w, bytes) = (raster.width() as usize, raster.bytes().unwrap().to_vec());
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for (index, px) in bytes.as_chunks::<4>().0.iter().enumerate() {
        if px[3] > 0 {
            xs.push(index % w);
            ys.push(index / w);
        }
    }
    let span = |values: &[usize]| values.iter().max().unwrap() - values.iter().min().unwrap();
    assert!(
        span(&ys) > span(&xs),
        "turned: {} by {}",
        span(&xs),
        span(&ys)
    );
    undo(&mut paint);
    assert_eq!(ink(&paint, layer), was, "undo puts it back");
}

/// 00:49:39, FG-38: Change Canvas Size shows the new canvas on the
/// canvas with handles; dragging a corner sets the size, dragging inside
/// places the picture, and OK keeps it as one undo step.
#[test]
fn fg_00_49_39_canvas_size_handles_on_the_canvas() {
    let mut paint = PaintDriver::new("p6a-canvas-size");
    let at = middle(&paint);
    inked(&mut paint, at - vec2(60.0, 0.0), at + vec2(60.0, 0.0));
    let [w, h] = picture_size(&paint);
    let scale = zoom(&paint) / 100.0;
    paint.command("edit.canvas_size");
    paint.settle(4);
    assert!(
        paint
            .rect_of("Drag the handles on the canvas to set the size and place")
            .is_some(),
        "{:?}",
        paint.texts
    );
    // The bottom right corner, dragged up and left.
    let corner = mark(&paint, "canvas_size.2");
    let by = vec2(-100.0, -60.0);
    paint.drag(corner, corner + by, PointerButton::Primary);
    let moved = mark(&paint, "canvas_size.2");
    assert!(
        moved.distance(corner + by) < 3.0,
        "the handle followed: {corner:?} to {moved:?}"
    );
    // The inside, dragged right: the picture's place in the new canvas.
    // Inside the box, near its top left corner, clear of the dialog.
    let inside = mark(&paint, "canvas_size.0") + vec2(40.0, 40.0);
    paint.drag(inside, inside + vec2(20.0, 0.0), PointerButton::Primary);
    let corner = mark(&paint, "canvas_size.2");
    assert!(
        corner.distance(moved + vec2(20.0, 0.0)) < 3.0,
        "the box moved whole: {moved:?} to {corner:?}"
    );
    let before = arena_bytes(&paint);
    paint.step(vec![key(Key::Enter, Modifiers::NONE)]);
    paint.settle(6);
    let [nw, nh] = picture_size(&paint);
    let want = [
        f64::from(w) + f64::from(by.x / scale),
        f64::from(h) + f64::from(by.y / scale),
    ];
    assert!(
        (f64::from(nw) - want[0]).abs() <= 2.0 && (f64::from(nh) - want[1]).abs() <= 2.0,
        "{w}x{h} to {nw}x{nh}, wanted about {want:?}"
    );
    let after = arena_bytes(&paint);
    undo(&mut paint);
    assert_eq!(picture_size(&paint), [w, h]);
    assert!(arena_bytes(&paint) == before, "undo gives back before");
    redo(&mut paint);
    assert!(arena_bytes(&paint) == after, "redo gives back after");
}

// The crash course tips.

/// Tip 47: Lasso fill's Blending mode. Multiply with white over black
/// leaves the black; where the layer is clear the white lays down.
#[test]
fn ct_47_lasso_fill_blending_mode() {
    let mut paint = PaintDriver::new("p6a-lasso-fill");
    let at = middle(&paint);
    paint.command("layer.new");
    paint.settle(2);
    let layer = top(&paint);
    // The left half black.
    paint.command("tool.selection");
    paint.settle(2);
    stroke(&mut paint, at + vec2(-200.0, -100.0), at + vec2(0.0, 100.0));
    paint.command("edit.fill");
    paint.settle(2);
    paint.command("select.none");
    paint.settle(2);
    paint.command("color.swap");
    paint.settle(2);
    pick_sub(&mut paint, "tool.fill", "Lasso fill");
    shows_rows(&mut paint, &["Blending mode", "Opacity"]);
    choose(&mut paint, "Blending mode", "Multiply");
    let left = picture_at(&paint, at + vec2(-50.0, 0.0));
    let right = picture_at(&paint, at + vec2(50.0, 0.0));
    let ring = |paint: &mut PaintDriver| {
        let points = [
            at + vec2(-100.0, -50.0),
            at + vec2(100.0, -50.0),
            at + vec2(100.0, 50.0),
            at + vec2(-100.0, 50.0),
            at + vec2(-100.0, -48.0),
        ];
        paint.step(vec![Event::PointerMoved(points[0])]);
        paint.step(vec![Event::PointerButton {
            pos: points[0],
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }]);
        for pair in points.windows(2) {
            for index in 1..=10 {
                let p = pair[0] + (pair[1] - pair[0]) * (index as f32 / 10.0);
                paint.step(vec![Event::PointerMoved(p)]);
            }
        }
        paint.step(vec![Event::PointerButton {
            pos: points[4],
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
        paint.settle(6);
    };
    undo_pair(&mut paint, "lasso fill in Multiply", ring, move |paint| {
        pixel(paint, layer, right)[3] == 255
    });
    assert_eq!(
        pixel(&paint, layer, right)[..3],
        [255, 255, 255],
        "white where clear"
    );
    assert_eq!(
        pixel(&paint, layer, left),
        [0, 0, 0, 255],
        "black multiplied stays"
    );
}
