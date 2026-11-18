//! Phase 6, "Layers, materials, templates, actions and view": the rows
//! of the Full Guide that stayed open after the tutorial was followed in
//! Sue Paint (P5-2), each closed by a test named after the step or gap it
//! closes (`step_<time>_<gap>_...`, the time in the video). Nothing of the
//! video is quoted here, and every material, template, pose and 3D model
//! the tests use is Sue Paint's own.
//!
//! Every test drives real frames: it clicks, drags and types what a
//! person does, found by the names the frame gives its controls, and it
//! reads what changed in the document. Each change to a document is
//! undone and redone and compared with the arena before and after.

use super::*;
use egui::pos2;

// Helpers, as the tutorial tests have them.

fn arena_bytes(paint: &PaintDriver) -> Vec<u8> {
    let mut out = sue_core::codec::Writer::new();
    paint.runtime.engine.world.arena.encode(&mut out);
    out.into_bytes()
}

fn picture(paint: &PaintDriver) -> sue_core::EntityId {
    paint
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.focus())
        .expect("a canvas is open")
}

fn layers(paint: &PaintDriver) -> Vec<sue_paint::doc::LayerInfo> {
    sue_paint::doc::layers(&paint.runtime.engine.world.arena, picture(paint))
}

fn said(paint: &PaintDriver) -> String {
    paint
        .runtime
        .shell
        .message()
        .map(|(text, _)| text.to_string())
        .unwrap_or_default()
}

/// One frame that also returns every control the frame named for
/// assistive technology, with its role and place.
fn step_told(
    paint: &mut PaintDriver,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::accesskit::Role, egui::Rect)> {
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

/// Every control name and text the next frame gives.
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

/// Where the control named `name` is drawn now (the topmost of them).
fn named(paint: &mut PaintDriver, name: &str) -> egui::Rect {
    let all = step_told(paint, Vec::new());
    all.iter()
        .filter(|(label, _, rect)| label == name && rect.width() > 0.0)
        .map(|(_, _, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| {
            panic!(
                "{name:?} is on screen: {:?}",
                all.into_iter().map(|(label, ..)| label).collect::<Vec<_>>()
            )
        })
}

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

fn click_named(paint: &mut PaintDriver, name: &str) {
    let at = named(paint, name).center();
    click_at(paint, at);
}

fn press(paint: &mut PaintDriver, which: egui::Key) {
    paint.step(vec![key(which, egui::Modifiers::NONE)]);
    paint.settle(4);
}

fn undo(paint: &mut PaintDriver) {
    paint.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    paint.settle(4);
}

fn redo(paint: &mut PaintDriver) {
    paint.step(vec![key(egui::Key::Y, egui::Modifiers::COMMAND)]);
    paint.settle(4);
}

/// Does `change`, which must change the document so that `check` holds;
/// undo gives back the arena as before (ids included), redo as after.
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

fn pixels(paint: &PaintDriver, layer: sue_core::EntityId) -> Vec<u8> {
    sue_paint::doc::raster(&paint.runtime.engine.world.arena, layer)
        .map(|raster| raster.bytes().unwrap_or_default().to_vec())
        .unwrap_or_default()
}

fn ink(paint: &PaintDriver, layer: sue_core::EntityId) -> u64 {
    pixels(paint, layer)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| u64::from(pixel[3]))
        .sum()
}

fn middle(paint: &PaintDriver) -> Pos2 {
    paint_middle(&paint.runtime)
}

/// The layer `change` adds.
fn added(paint: &PaintDriver, before: &[sue_paint::doc::LayerInfo]) -> sue_core::EntityId {
    layers(paint)
        .into_iter()
        .find(|info| !before.iter().any(|old| old.id == info.id))
        .expect("a new layer")
        .id
}

/// A new raster layer (Ctrl+Shift+N); returns it.
fn new_layer(paint: &mut PaintDriver) -> sue_core::EntityId {
    let before = layers(paint);
    paint.step(vec![key(
        egui::Key::N,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    )]);
    paint.settle(4);
    added(paint, &before)
}

/// Dismisses the shell's notice, if one shows over the work.
fn dismiss(paint: &mut PaintDriver) {
    if names(paint).iter().any(|name| name == "Dismiss") {
        click_named(paint, "Dismiss");
    }
}

/// A stroke of the pen across the middle of the canvas, `dy` below it.
fn pen_stroke(paint: &mut PaintDriver, dy: f32) {
    dismiss(paint);
    paint.command("tool.pen");
    paint.settle(2);
    let at = middle(paint) + vec2(0.0, dy);
    paint.drag(
        at - vec2(60.0, 0.0),
        at + vec2(60.0, 0.0),
        egui::PointerButton::Primary,
    );
}

fn label_of(name: &str) -> String {
    sue_paint::registry()
        .label(name)
        .unwrap_or_else(|| panic!("{name} is a command"))
}

/// Every command in Sue Paint's menu titled `title`.
fn menu_commands(title: &str) -> Vec<String> {
    fn walk(items: &[sue_wm::MenuItem], out: &mut Vec<String>) {
        for item in items {
            match item {
                sue_wm::MenuItem::Action { command, .. } => out.push(command.clone()),
                sue_wm::MenuItem::Sub { items, .. } => walk(items, out),
                sue_wm::MenuItem::Separator => {}
            }
        }
    }
    let mut out = Vec::new();
    for menu in sue_paint::menus() {
        if menu.title == title {
            walk(&menu.items, &mut out);
        }
    }
    out
}

/// The Material palette, shown at the folder of material kind `kind`
/// with the button of that folder; the thumbnails it shows, by name.
fn material_folder(paint: &mut PaintDriver, folder: &str) -> Vec<(String, egui::Rect)> {
    paint.settle(2);
    if paint.rect_of("Material").is_none() {
        paint.command("window.material");
        paint.settle(4);
    }
    paint.click_text("Material");
    paint.settle(4);
    click_named(paint, folder);
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

fn thumb(thumbs: &[(String, egui::Rect)], name: &str) -> egui::Rect {
    thumbs
        .iter()
        .find(|(shown, _)| shown == name)
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("{name} among {thumbs:?}"))
}

fn rice_bool(paint: &PaintDriver, key: &str) -> bool {
    paint
        .runtime
        .engine
        .world
        .rice
        .get_bool(key)
        .unwrap_or(false)
}

// A. Layers.

/// 00:11:44, FG-38: New Layer, File Object. A layer shows a picture file
/// kept outside the canvas; it takes no drawing until it is rasterized,
/// and Update File Object shows the file as it is now.
#[test]
fn step_00_11_44_fg_38_new_layer_file_object() {
    let mut paint = PaintDriver::new("p6-file-object");
    assert!(menu_commands("Layer").contains(&"layer.new.file_object".to_owned()));
    let file = paint.root.join("reference.png");
    let picture_of = |shade: u8| {
        let mut bytes = vec![0_u8; 40 * 30 * 4];
        for pixel in bytes.chunks_mut(4) {
            pixel.copy_from_slice(&[shade, 40, 90, 255]);
        }
        bytes
    };
    let first = picture_of(10);
    sue_platform::fs::write(&file, sue_io::encode_png_rgba(40, 30, &first).unwrap()).unwrap();
    let count = layers(&paint).len();
    undo_pair(
        &mut paint,
        "New Layer, File Object",
        |paint| {
            paint.command("layer.new.file_object");
            paint.settle(2);
            assert!(
                paint.runtime.pending_pick().is_some(),
                "it asks for the file"
            );
            let now = paint.now;
            assert!(paint.runtime.answer_pick(&file, &mut NoTextures, now));
            paint.settle(4);
            dismiss(paint);
        },
        |paint| layers(paint).len() == count + 1,
    );
    let layer = layers(&paint).last().unwrap().clone();
    assert_eq!(layer.name, "reference");
    // Picked in the Layer palette.
    paint.click_text("reference");
    let entity = paint
        .runtime
        .engine
        .world
        .arena
        .get_by_id(layer.id)
        .unwrap();
    assert_eq!(
        entity
            .property("sue_paint.file_object")
            .and_then(sue_core::Value::as_str),
        Some(file.display().to_string().as_str())
    );
    assert_eq!(pixels(&paint, layer.id), first, "the file's pixels");
    // In the middle of the 1600 by 1200 canvas.
    assert_eq!(layer.style.offset, [780, 585]);
    // The pen does not draw on it: it shows a file.
    let held = pixels(&paint, layer.id);
    pen_stroke(&mut paint, 0.0);
    assert_eq!(pixels(&paint, layer.id), held);
    paint.settle(2);
    assert!(said(&paint).contains("Rasterize"), "{}", said(&paint));
    // The file changes; Update File Object shows it, one undo step.
    let second = picture_of(200);
    sue_platform::fs::write(&file, sue_io::encode_png_rgba(40, 30, &second).unwrap()).unwrap();
    undo_pair(
        &mut paint,
        "Update File Object",
        |paint| {
            paint.command("layer.file_object.update");
            paint.settle(2);
        },
        |paint| pixels(paint, layer.id) == second,
    );
    // Rasterize makes it a raster layer, which the pen draws on.
    undo_pair(
        &mut paint,
        "Rasterize the file object",
        |paint| {
            paint.command("layer.rasterize");
            paint.settle(2);
        },
        |paint| {
            paint
                .runtime
                .engine
                .world
                .arena
                .get_by_id(layer.id)
                .is_ok_and(|entity| entity.property("sue_paint.file_object").is_none())
        },
    );
    let before = ink(&paint, layer.id);
    pen_stroke(&mut paint, 0.0);
    assert!(ink(&paint, layer.id) >= before);
    assert_ne!(pixels(&paint, layer.id), second, "the pen drew on it");
}

/// 00:11:50, FGV-6: Layer Settings, Display palette colors in parent
/// folder (the folder's row shows the colors of what it holds) and on
/// canvas (each layer's drawing shows in its palette color, the picture
/// itself unchanged).
#[test]
fn step_00_11_50_fgv_6_palette_colors_in_parent_folder_and_on_canvas() {
    let mut paint = PaintDriver::new("p6-palette-colors");
    let layer_menu = menu_commands("Layer");
    for command in ["layer.palette_in_folder", "layer.palette_on_canvas"] {
        assert!(layer_menu.contains(&command.to_owned()), "{command}");
    }
    let layer = new_layer(&mut paint);
    let layer_name = layers(&paint).last().unwrap().name.clone();
    paint.command("layer.folder_insert");
    paint.settle(4);
    assert!(
        sue_paint::doc::folders_around(&paint.runtime.engine.world.arena, layer).len() == 1,
        "the new layer is in the folder"
    );
    paint.click_text(&layer_name);
    pen_stroke(&mut paint, 0.0);
    paint.command("layer.palette_color.red");
    paint.settle(4);
    let chip = format!("{} inside", label_of("layer.palette_color.red"));
    assert!(!names(&mut paint).contains(&chip), "off at first");
    // In the parent folder: a setting, so the document stays as it is.
    let document = arena_bytes(&paint);
    paint.command("layer.palette_in_folder");
    paint.settle(4);
    assert!(rice_bool(&paint, "sue_paint.palette_in_folder"));
    let seen = names(&mut paint);
    assert!(seen.contains(&chip), "{chip:?} in {seen:?}");
    assert!(arena_bytes(&paint) == document);
    // On the canvas: Obtain Display Color picks what the canvas shows.
    let picked_ink = |paint: &mut PaintDriver| {
        dismiss(paint);
        paint.command("tool.eyedropper");
        paint.settle(2);
        paint.click_text("Obtain Display Color");
        let at = middle(paint);
        click_at(paint, at);
        let probe = new_layer(paint);
        pen_stroke(paint, 0.0);
        let found = pixels(paint, probe)
            .as_chunks::<4>()
            .0
            .iter()
            .max_by_key(|pixel| pixel[3])
            .copied()
            .filter(|pixel| pixel[3] > 0)
            .unwrap_or_else(|| {
                panic!(
                    "the probe stroke inks: {} {:?}",
                    said(paint),
                    layers(paint)
                        .iter()
                        .map(|info| (info.name.clone(), ink(paint, info.id)))
                        .collect::<Vec<_>>()
                )
            });
        undo(paint);
        undo(paint);
        found
    };
    let neutral =
        |pixel: [u8; 4]| pixel[0].abs_diff(pixel[1]) < 16 && pixel[1].abs_diff(pixel[2]) < 16;
    let plain = picked_ink(&mut paint);
    assert!(neutral(plain), "gray as drawn: {plain:?}");
    paint.command("layer.palette_on_canvas");
    paint.settle(6);
    assert!(rice_bool(&paint, "sue_paint.palette_on_canvas"));
    let shown = picked_ink(&mut paint);
    assert!(
        shown[0] > shown[1].saturating_add(60) && shown[0] > shown[2].saturating_add(60),
        "the canvas shows it red: {shown:?}"
    );
    assert!(arena_bytes(&paint) == document, "the picture is unchanged");
    // Off again, the canvas shows the ink as drawn.
    paint.command("layer.palette_on_canvas");
    paint.settle(6);
    let again = picked_ink(&mut paint);
    assert!(neutral(again), "{again:?}");
}

// B. Auto Action.

/// The steps the Auto Action palette shows now.
fn step_rows(paint: &mut PaintDriver) -> Vec<String> {
    if !names(paint)
        .iter()
        .any(|name| name == "Record" || name == "Stop")
    {
        paint.command("window.auto_actions");
        paint.settle(4);
    }
    if paint.rect_of("Auto Action").is_some() {
        paint.click_text("Auto Action");
    }
    names(paint)
}

fn index_of(paint: &PaintDriver, layer: sue_core::EntityId) -> usize {
    layers(paint)
        .iter()
        .position(|info| info.id == layer)
        .unwrap()
}

/// 00:20:56, FGV-16: an action records a filter with its dialog, a layer
/// dragged in the Layer palette and a stroke on the canvas; each step's
/// own box says whether its dialog shows while the action plays, and the
/// action waits for it (Cancel stops the rest).
#[test]
fn step_00_20_56_fgv_16_dialog_switch_and_recorded_layer_drag_and_strokes() {
    let mut paint = PaintDriver::new("p6-auto-action");
    let first = layers(&paint).last().unwrap().id;
    let second = new_layer(&mut paint);
    let third = new_layer(&mut paint);
    pen_stroke(&mut paint, 0.0);
    let first_name = layers(&paint)
        .iter()
        .find(|info| info.id == first)
        .unwrap()
        .name
        .clone();
    let third_name = layers(&paint)
        .iter()
        .find(|info| info.id == third)
        .unwrap()
        .name
        .clone();
    // Record: a filter through its dialog, a layer drag, a stroke.
    paint.command("auto.new");
    paint.settle(4);
    paint.command("filter.gaussian");
    paint.settle(4);
    press(&mut paint, egui::Key::Enter);
    let from = paint.rect_of(&third_name).expect("the dragged row");
    let onto = paint.rect_of(&first_name).expect("the row dropped on");
    // Onto the upper half of the row (over its name): right above it.
    let upper = onto.center() - vec2(0.0, onto.height() * 1.5);
    paint.drag(from.center(), upper, egui::PointerButton::Primary);
    assert_eq!(
        index_of(&paint, third) + 1,
        index_of(&paint, second),
        "moved"
    );
    pen_stroke(&mut paint, 40.0);
    paint.command("auto.record");
    paint.settle(4);
    let seen = step_rows(&mut paint);
    for step in [
        label_of("filter.gaussian"),
        label_of("layer.move_by.1"),
        label_of("auto.stroke.x"),
    ] {
        assert!(seen.contains(&step), "{step:?} recorded: {seen:?}");
    }
    // The action is kept with its steps, in Sue Paint's data folder.
    let kept = sue_platform::fs::read_to_string(
        paint
            .root
            .join("data")
            .join("paint")
            .join("auto_actions.txt"),
    )
    .unwrap();
    assert!(kept.contains("step filter.gaussian"), "{kept}");
    assert!(kept.contains("step layer.move_by.-1"), "{kept}");
    assert!(kept.contains("step auto.stroke.subtool."), "{kept}");
    // The filter's dialog shows while playing.
    let switch = format!(
        "Show the dialog of {} while playing",
        label_of("filter.gaussian")
    );
    click_named(&mut paint, &switch);
    let kept = sue_platform::fs::read_to_string(
        paint
            .root
            .join("data")
            .join("paint")
            .join("auto_actions.txt"),
    )
    .unwrap();
    assert!(kept.contains("step filter.gaussian\ndialog\n"), "{kept}");
    // Played: the dialog shows and the action waits; OK goes on.
    let depth = paint.runtime.engine.history.depth();
    let (at, inked) = (index_of(&paint, third), ink(&paint, third));
    paint.command("auto.play");
    paint.settle(6);
    assert!(
        names(&mut paint).contains(&"OK".to_owned()),
        "the dialog shows"
    );
    assert_eq!(index_of(&paint, third), at, "the rest waits");
    press(&mut paint, egui::Key::Enter);
    paint.settle(6);
    assert_eq!(index_of(&paint, third), at - 1, "the drag played");
    assert!(ink(&paint, third) > inked, "the stroke played");
    assert_eq!(
        paint.runtime.engine.history.depth(),
        depth + 3,
        "three steps"
    );
    // Cancel in the dialog stops the action.
    let (at, inked) = (index_of(&paint, third), ink(&paint, third));
    paint.command("auto.play");
    paint.settle(6);
    press(&mut paint, egui::Key::Escape);
    paint.settle(6);
    assert_eq!(index_of(&paint, third), at);
    assert_eq!(ink(&paint, third), inked);
    // Without the box the action runs through, its dialog not shown.
    click_named(&mut paint, &switch);
    let before = arena_bytes(&paint);
    let depth = paint.runtime.engine.history.depth();
    paint.command("auto.play");
    paint.settle(6);
    assert!(!names(&mut paint).contains(&"OK".to_owned()), "no dialog");
    assert!(ink(&paint, third) > inked);
    // Each step is a step of History, undone one by one.
    let steps = paint.runtime.engine.history.depth() - depth;
    assert_eq!(steps, 3);
    let after = arena_bytes(&paint);
    for _ in 0..steps {
        undo(&mut paint);
    }
    assert!(arena_bytes(&paint) == before, "undone");
    for _ in 0..steps {
        redo(&mut paint);
    }
    assert!(arena_bytes(&paint) == after, "redone");
}

/// 00:20:56, FGV-16: a step keeps the settings its dialog had when it
/// was recorded. A blur recorded at a radius of its own plays at that
/// radius with its dialog hidden (not at the dialog's first radius), and
/// a dialog cancelled while recording leaves no step behind.
#[test]
fn step_00_20_56_fgv_16_a_step_keeps_the_settings_of_its_dialog() {
    let mut paint = PaintDriver::new("p6-auto-settings");
    let layer = new_layer(&mut paint);
    pen_stroke(&mut paint, 0.0);
    let drawn = pixels(&paint, layer);
    let file = paint
        .root
        .join("data")
        .join("paint")
        .join("auto_actions.txt");
    // Cancelled while recording: nothing is kept.
    paint.command("auto.new");
    paint.settle(4);
    paint.command("filter.gaussian");
    paint.settle(4);
    press(&mut paint, egui::Key::Escape);
    paint.settle(4);
    assert_eq!(pixels(&paint, layer), drawn, "cancelled");
    // Recorded at a radius of its own, near the slider's far end.
    paint.command("filter.gaussian");
    paint.settle(4);
    let radius = named(&mut paint, &label_of_text("radius"));
    click_at(&mut paint, pos2(radius.right() - 6.0, radius.center().y));
    press(&mut paint, egui::Key::Enter);
    paint.command("auto.record");
    paint.settle(4);
    let blurred = pixels(&paint, layer);
    assert_ne!(blurred, drawn, "blurred");
    let kept = sue_platform::fs::read_to_string(&file).unwrap();
    let action = kept
        .split("[")
        .last()
        .expect("the action recorded last")
        .to_owned();
    assert_eq!(
        action.matches("step ").count(),
        1,
        "the cancelled dialog left no step: {action}"
    );
    let settings = action
        .lines()
        .find_map(|line| line.strip_prefix("settings filter:gaussian="))
        .unwrap_or_else(|| panic!("the radius is kept: {action}"));
    let kept_radius: f32 = settings.parse().unwrap();
    assert!(
        kept_radius > 8.0,
        "the radius set, not the first: {kept_radius}"
    );
    // Played on the drawing as it was, the dialog hidden: the same blur.
    undo(&mut paint);
    assert_eq!(pixels(&paint, layer), drawn);
    paint.command("auto.play");
    paint.settle(6);
    assert!(!names(&mut paint).contains(&"OK".to_owned()), "no dialog");
    assert_eq!(pixels(&paint, layer), blurred, "played at the kept radius");
    // With its dialog shown, the dialog opens at the kept radius: OK
    // gives the same blur again.
    undo(&mut paint);
    let switch = format!(
        "Show the dialog of {} while playing",
        label_of("filter.gaussian")
    );
    step_rows(&mut paint);
    click_named(&mut paint, &switch);
    paint.command("auto.play");
    paint.settle(6);
    assert!(names(&mut paint).contains(&"OK".to_owned()), "the dialog");
    press(&mut paint, egui::Key::Enter);
    paint.settle(4);
    assert_eq!(pixels(&paint, layer), blurred);
}

/// A string of Sue Paint's language database.
fn label_of_text(key: &str) -> String {
    sue_lang::text("paint", key)
}

// C. Preferences, New and templates.

/// The opaque rectangles one frame paints, by color.
fn fills(paint: &mut PaintDriver) -> Vec<egui::Color32> {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        time: Some(paint.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    paint.now += Duration::from_millis(16);
    let runtime = &mut paint.runtime;
    let now = paint.now;
    let output = paint
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    let mut found = Vec::new();
    for clipped in &output.shapes {
        let mut take = |shape: &egui::Shape| {
            if let egui::Shape::Rect(rect) = shape
                && rect.fill.a() == 255
                && rect.rect.intersect(clipped.clip_rect).is_positive()
            {
                found.push(rect.fill);
            }
        };
        match &clipped.shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(&mut take),
            shape => take(shape),
        }
    }
    output.drop_without_applying_deltas();
    found
}

/// 00:38:57, FG-16: Preferences, Interface, Adjust density: Sue Paint's
/// surfaces turn darker or lighter, its text stays readable.
#[test]
fn step_00_38_57_fg_16_adjust_density_of_the_interface_color() {
    let mut paint = PaintDriver::new("p6-density");
    let rice = &paint.runtime.engine.world.rice;
    assert_eq!(rice.get_i64("sue_paint.theme_density"), Some(0));
    let base =
        sue_wm::Theme::from_rice(rice).for_studio(rice, "sue_paint", &sue_rice::builtin_presets());
    // Settings finds it by Clip Studio's name.
    let page = paint
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.settings_page())
        .expect("Sue Paint's page of Settings");
    assert!(
        page.search
            .iter()
            .any(|(path, words)| path == "sue_paint.theme_density"
                && words.contains("Adjust density")),
        "{:?}",
        page.search
    );
    let plain = fills(&mut paint);
    let panels = plain.iter().filter(|fill| **fill == base.panel).count();
    assert!(panels > 3, "the palettes wear the panel color: {panels}");
    for (percent, darker) in [(60, true), (-60, false)] {
        paint
            .runtime
            .engine
            .world
            .rice
            .set("sue_paint.theme_density", sue_rice::RiceValue::Int(percent))
            .unwrap();
        paint.settle(4);
        let dense = base.with_density(percent as f32 / 100.0);
        assert_ne!(dense.panel, base.panel);
        let lum = |color: egui::Color32| {
            u32::from(color.r()) + u32::from(color.g()) + u32::from(color.b())
        };
        assert_eq!(lum(dense.panel) < lum(base.panel), darker);
        let shown = fills(&mut paint);
        let now = shown.iter().filter(|fill| **fill == dense.panel).count();
        let old = shown.iter().filter(|fill| **fill == base.panel).count();
        assert!(now >= panels, "{percent}: {now} of {panels} panels");
        assert!(old * 4 < panels, "{percent}: {old} still the old color");
        assert!(sue_wm::ui::tokens::contrast(dense.text, dense.panel) >= 4.5);
    }
}

/// 00:41:10 and 00:46:11, FG-18: Edit, Register Material, Template keeps
/// a canvas's layers as a template (a `.clip` file); File, New with
/// Template starts a canvas with them; and Sue Paint's own frame
/// templates are there too.
#[test]
fn step_00_41_10_fg_18_new_with_template_and_register_material_template() {
    let mut paint = PaintDriver::new("p6-template");
    let edit = menu_commands("Edit");
    assert!(edit.contains(&"edit.material.template".to_owned()));
    // The canvas to keep: an inked layer over the paper.
    let inked = new_layer(&mut paint);
    pen_stroke(&mut paint, 0.0);
    let inked_name = layers(&paint)
        .iter()
        .find(|info| info.id == inked)
        .unwrap()
        .name
        .clone();
    let held = ink(&paint, inked);
    let document = arena_bytes(&paint);
    paint.command("edit.material.template");
    paint.settle(4);
    let seen = names(&mut paint);
    assert!(seen.contains(&"Template name".to_owned()), "{seen:?}");
    // Its name.
    let field = named(&mut paint, "Template name");
    click_at(&mut paint, field.center());
    paint.step(vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
    paint.step(vec![egui::Event::Text("Two inks".to_owned())]);
    paint.settle(2);
    press(&mut paint, egui::Key::Enter);
    paint.settle(4);
    assert!(
        arena_bytes(&paint) == document,
        "registering changes nothing"
    );
    let file = paint
        .root
        .join("data")
        .join("paint")
        .join("templates")
        .join("Two inks.clip");
    assert!(
        sue_platform::fs::exists(&file),
        "{file:?}: {}",
        said(&paint)
    );
    // File, New, Template.
    let old = picture(&paint);
    paint.command("file.new");
    paint.settle(6);
    let seen = names(&mut paint);
    assert!(
        seen.contains(&"Template to start from".to_owned()),
        "{seen:?}"
    );
    click_named(&mut paint, "Template");
    let pick = named(&mut paint, "Template to start from");
    click_at(&mut paint, pick.center());
    paint.click_text("Two inks");
    paint.settle(2);
    click_named(&mut paint, "OK");
    paint.settle(10);
    assert_ne!(picture(&paint), old, "a new canvas: {}", said(&paint));
    let made = layers(&paint);
    let copy = made
        .iter()
        .find(|info| info.name == inked_name)
        .unwrap_or_else(|| panic!("the template's layer: {made:?} {}", said(&paint)));
    assert_eq!(ink(&paint, copy.id), held, "with its pixels");
    let papers = made
        .iter()
        .filter(|info| info.kind == sue_model::picture::LayerKind::Paper)
        .count();
    assert_eq!(papers, 1, "one paper: the template's stays out");
    // The template is one undo step on the new canvas.
    let with = arena_bytes(&paint);
    undo(&mut paint);
    let bare = layers(&paint);
    assert_eq!(bare.len(), 2, "the new canvas's paper and layer: {bare:?}");
    assert!(bare.iter().all(|info| ink(&paint, info.id) == 0));
    redo(&mut paint);
    assert!(arena_bytes(&paint) == with);
    // Sue Paint's own frame templates, from the Material palette.
    let thumbs = material_folder(&mut paint, "Templates");
    for name in ["Two by two", "Manga page (2, 1, 2)", "Two inks"] {
        thumb(&thumbs, name);
    }
    let count = layers(&paint).len();
    let to = middle(&paint);
    undo_pair(
        &mut paint,
        "a frame template dropped on the canvas",
        |paint| {
            paint.drag(
                thumb(&thumbs, "Two by two").center(),
                to,
                egui::PointerButton::Primary,
            );
        },
        |paint| layers(paint).len() == count + 1,
    );
}

/// 00:51:07, FG-38: View, Webtoon Display Settings: the canvas shows
/// where each screen of a reader's device ends, and Fit Width.
#[test]
fn step_00_51_07_fg_38_webtoon_display_settings() {
    let mut paint = PaintDriver::new("p6-webtoon");
    let view = menu_commands("View");
    for command in ["view.webtoon_display", "view.fit_width"] {
        assert!(view.contains(&command.to_owned()), "{command}");
    }
    let document = arena_bytes(&paint);
    assert!(
        !paint.texts.iter().any(|(text, _)| text == "Screen 1"),
        "no guides at first"
    );
    paint.command("view.webtoon_display");
    paint.settle(4);
    click_named(&mut paint, "Show where each screen ends");
    let read_on = named(&mut paint, "Read on");
    click_at(&mut paint, read_on.center());
    paint.click_text("Desktop (16:10)");
    paint.settle(2);
    click_named(&mut paint, "OK");
    paint.settle(6);
    assert!(rice_bool(&paint, "sue_paint.webtoon_guides"));
    assert_eq!(
        paint
            .runtime
            .engine
            .world
            .rice
            .get_str("sue_paint.webtoon_screen"),
        Some("desktop")
    );
    // A 1600 pixel wide canvas read on a 16:10 screen: one screen ends
    // 1000 pixels down, the next past the canvas.
    let texts = paint.settle(4);
    let guides: Vec<&String> = texts
        .iter()
        .map(|(text, _)| text)
        .filter(|text| text.starts_with("Screen "))
        .collect();
    assert_eq!(guides, ["Screen 1"], "{texts:?}");
    // Fit Width: the canvas fills the window across, its top in sight,
    // so the guide is lower and further apart from the canvas's left.
    paint.command("view.fit");
    paint.settle(4);
    let fitted = paint.rect_of("Screen 1").expect("the guide shows");
    paint.command("view.fit_width");
    paint.settle(4);
    let wide = paint.rect_of("Screen 1").expect("the guide shows");
    assert!(wide.left() < fitted.left(), "{fitted:?} then {wide:?}");
    assert!(arena_bytes(&paint) == document, "only the view changed");
}

// D. Materials.

/// 00:52:33: one button per folder of the Material palette, each showing
/// it open at that folder, and the same in Window, Material Folders.
#[test]
fn step_00_52_33_one_material_palette_button_per_folder() {
    let mut paint = PaintDriver::new("p6-material-folders");
    let window = menu_commands("Window");
    let kinds = sue_paint::materials::MaterialKinds::builtin();
    for kind in kinds.iter() {
        let command = format!("material.folder.{}", kind.id);
        assert!(window.contains(&command), "{command}");
    }
    let balloons = material_folder(&mut paint, "Balloons");
    let names_of = |thumbs: &[(String, egui::Rect)]| -> Vec<String> {
        thumbs.iter().map(|(name, _)| name.clone()).collect()
    };
    assert!(names_of(&balloons).contains(&"Round balloon".to_owned()));
    assert!(!names_of(&balloons).contains(&"Dot 10%".to_owned()));
    let tones = material_folder(&mut paint, "Tones");
    assert!(names_of(&tones).contains(&"Dot 10%".to_owned()));
    assert!(!names_of(&tones).contains(&"Round balloon".to_owned()));
    // From the Window menu: the palette at Effect lines.
    paint.command("material.folder.sue_paint.effect");
    paint.settle(6);
    let seen = names(&mut paint);
    assert!(seen.contains(&"Focus lines".to_owned()), "{seen:?}");
    assert!(!seen.contains(&"Dot 10%".to_owned()));
}

/// 00:54:01 and 00:54:15: monochrome patterns and manga materials (effect
/// lines, balloons, frames) of Sue Paint's own making, put on the canvas
/// from the Material palette.
#[test]
fn step_00_54_01_monochrome_patterns_and_manga_materials_of_sue_paints_own() {
    let mut paint = PaintDriver::new("p6-manga-materials");
    let library = sue_paint::materials::library(None);
    let of = |kind: &str| library.iter().filter(|m| m.kind == kind).count();
    assert!(of(sue_paint::materials::TONE) >= 15, "screentones");
    assert!(of(sue_paint::materials::PATTERN) >= 15, "patterns");
    assert!(of(sue_paint::materials::EFFECT) >= 5, "effect lines");
    assert!(of(sue_paint::materials::BALLOON) >= 5, "balloons");
    let size = |paint: &PaintDriver| {
        paint
            .runtime
            .engine
            .world
            .arena
            .get_by_id(picture(paint))
            .ok()
            .and_then(sue_model::picture::size)
            .unwrap()
    };
    let [w, h] = size(&paint);
    let alpha = |paint: &PaintDriver, layer, x: u32, y: u32| {
        pixels(paint, layer)[((y * w + x) * 4 + 3) as usize]
    };
    // Focus lines: over the canvas, the middle left clear.
    let effects = material_folder(&mut paint, "Effect lines");
    let count = layers(&paint).len();
    let stack = layers(&paint);
    let to = middle(&paint);
    undo_pair(
        &mut paint,
        "focus lines dropped on the canvas",
        |paint| {
            paint.drag(
                thumb(&effects, "Focus lines").center(),
                to,
                egui::PointerButton::Primary,
            );
        },
        |paint| layers(paint).len() == count + 1,
    );
    let lines = added(&paint, &stack);
    assert_eq!(alpha(&paint, lines, w / 2, h / 2), 0, "the middle is clear");
    let edge: u32 = (0..w).map(|x| u32::from(alpha(&paint, lines, x, 2))).sum();
    assert!(edge > 255 * w / 20, "lines reach the edge: {edge}");
    // A balloon: white inside, its outline in the drawing color.
    let balloons = material_folder(&mut paint, "Balloons");
    let count = layers(&paint).len();
    let stack = layers(&paint);
    undo_pair(
        &mut paint,
        "a balloon dropped on the canvas",
        |paint| {
            paint.drag(
                thumb(&balloons, "Round balloon").center(),
                to,
                egui::PointerButton::Primary,
            );
        },
        |paint| layers(paint).len() == count + 1,
    );
    let balloon = added(&paint, &stack);
    let bytes = pixels(&paint, balloon);
    let opaque: Vec<&[u8; 4]> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] == 255)
        .collect();
    assert!(
        opaque.iter().any(|pixel| pixel[..3] == [255, 255, 255]),
        "white paper"
    );
    assert!(opaque.iter().any(|pixel| pixel[0] < 40), "a dark outline");
    // A monochrome pattern covers the canvas in the drawing color.
    let patterns = material_folder(&mut paint, "Patterns");
    let count = layers(&paint).len();
    let stack = layers(&paint);
    undo_pair(
        &mut paint,
        "a pattern dropped on the canvas",
        |paint| {
            paint.drag(
                thumb(&patterns, "Herringbone").center(),
                to,
                egui::PointerButton::Primary,
            );
        },
        |paint| layers(paint).len() == count + 1,
    );
    // Its ink reaches every corner of the canvas, in the drawing color
    // (black), and leaves room between its lines.
    let pattern = added(&paint, &stack);
    let bytes = pixels(&paint, pattern);
    let inked: Vec<&[u8; 4]> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 128)
        .collect();
    let share = inked.len() as f32 / (bytes.len() / 4) as f32;
    assert!((0.05..0.8).contains(&share), "lines and room: {share}");
    assert!(
        inked.iter().all(|pixel| pixel[..3] == [0, 0, 0]),
        "black ink"
    );
    for (x, y) in [(0, 0), (w - 24, 0), (0, h - 24), (w - 24, h - 24)] {
        let corner: u32 = (0..24)
            .flat_map(|dy| (0..24).map(move |dx| (x + dx, y + dy)))
            .map(|(x, y)| u32::from(alpha(&paint, pattern, x, y)))
            .sum();
        assert!(corner > 0, "ink at the corner {x}, {y}");
    }
}

// E. 3D.

fn model_bytes(paint: &PaintDriver, layer: sue_core::EntityId) -> Vec<u8> {
    let arena = &paint.runtime.engine.world.arena;
    let mut out = sue_core::codec::Writer::new();
    arena
        .get_by_id(sue_paint::three::model_of(arena, layer).unwrap())
        .unwrap()
        .encode(&mut out);
    out.into_bytes()
}

/// 00:58:39: a pose dragged from the Material palette onto a drawing
/// figure on the canvas poses that figure, though another layer is the
/// current one; let go where no figure is, it makes one in that pose.
#[test]
fn step_00_58_39_drag_a_pose_onto_a_3d_model() {
    let mut paint = PaintDriver::new("p6-pose-drag");
    paint.command("layer.new.figure");
    paint.settle(6);
    let figure = layers(&paint).last().unwrap().id;
    assert!(sue_paint::three::is_3d(
        &paint.runtime.engine.world.arena,
        figure
    ));
    // The figure shows in the middle of the canvas.
    let [w, h] = paint
        .runtime
        .engine
        .world
        .arena
        .get_by_id(picture(&paint))
        .ok()
        .and_then(sue_model::picture::size)
        .unwrap();
    let raster = pixels(&paint, figure);
    assert!(
        raster[(((h / 2) * w + w / 2) * 4 + 3) as usize] > 0,
        "the figure covers the middle"
    );
    // Another layer is the current one.
    let above = new_layer(&mut paint);
    let poses = material_folder(&mut paint, "Poses");
    let count = layers(&paint).len();
    let start = model_bytes(&paint, figure);
    let to = middle(&paint);
    let t_pose = thumb(&poses, "T-pose");
    undo_pair(
        &mut paint,
        "T-pose dropped onto the figure",
        |paint| {
            paint.drag(t_pose.center(), to, egui::PointerButton::Primary);
        },
        |paint| model_bytes(paint, figure) != start,
    );
    assert_eq!(layers(&paint).len(), count, "no layer was added");
    let arena = &paint.runtime.engine.world.arena;
    let model = arena
        .get_by_id(sue_paint::three::model_of(arena, figure).unwrap())
        .unwrap();
    assert_eq!(
        sue_paint::figure::Pose::of(model),
        sue_paint::figure::Body::of(model).preset(1),
        "in the T-pose"
    );
    assert!(pixels(&paint, above).iter().all(|byte| *byte == 0));
    // Let go on empty canvas: a new figure in that pose.
    let corner = middle(&paint) + vec2(-300.0, -200.0);
    undo_pair(
        &mut paint,
        "a pose dropped where no figure is",
        |paint| {
            paint.drag(t_pose.center(), corner, egui::PointerButton::Primary);
        },
        |paint| layers(paint).len() == count + 1,
    );
}

/// 00:58:56 to 00:59:19: 3D backgrounds and small objects of Sue Paint's
/// own, each put on the canvas as a 3D layer. (Characters with physics
/// are Clip Studio's downloaded character models: excluded, as the
/// owner excluded material downloads.)
#[test]
fn step_00_58_56_3d_backgrounds_and_small_objects() {
    let mut paint = PaintDriver::new("p6-3d-materials");
    for (folder, name) in [("3D backgrounds", "Room"), ("3D objects", "Chair")] {
        let thumbs = material_folder(&mut paint, folder);
        assert!(thumbs.len() >= 5, "{folder}: {thumbs:?}");
        let count = layers(&paint).len();
        let stack = layers(&paint);
        let to = middle(&paint);
        undo_pair(
            &mut paint,
            name,
            |paint| {
                paint.drag(
                    thumb(&thumbs, name).center(),
                    to,
                    egui::PointerButton::Primary,
                );
            },
            |paint| layers(paint).len() == count + 1,
        );
        // The layer it made (not the one made before it).
        let layer = added(&paint, &stack);
        assert_eq!(
            layers(&paint)
                .iter()
                .find(|info| info.id == layer)
                .map(|info| info.name.clone()),
            Some(name.to_owned())
        );
        let arena = &paint.runtime.engine.world.arena;
        assert!(
            sue_paint::three::is_3d(arena, layer),
            "{name} is a 3D layer"
        );
        assert!(ink(&paint, layer) > 0, "{name} shows");
        // A background is seen from inside, where a person would stand:
        // it fills the canvas (a room has walls, a floor and a ceiling all
        // round); an object stands in the middle with room round it.
        let shown = pixels(&paint, layer);
        let covered = shown
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .count() as f32
            / (shown.len() / 4) as f32;
        if folder == "3D backgrounds" {
            assert!(covered > 0.95, "{name} fills the canvas: {covered}");
        } else {
            assert!(covered < 0.5, "{name} stands in the middle: {covered}");
        }
    }
}
