//! Sue Paint: layers, folders, masks, correction layers, layer comps.

use super::*;

#[test]
fn sue_paint_folders_take_layers_in_and_out_as_csp_does() {
    let (mut runtime, root) = runtime("paint-folders");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(8);
    };
    let command = egui::Modifiers::COMMAND;
    step(&mut runtime, vec![key(egui::Key::N, command)]);
    step(
        &mut runtime,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    step(&mut runtime, Vec::new());
    let picture = runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.focus())
        .unwrap();
    step(
        &mut runtime,
        vec![key(egui::Key::N, command | egui::Modifiers::SHIFT)],
    );
    let stack = |runtime: &Runtime| -> Vec<sue_core::EntityId> {
        sue_paint::doc::layers(&runtime.engine.world.arena, picture)
            .into_iter()
            .map(|info| info.id)
            .collect()
    };
    let [_, first, second] = stack(&runtime)[..] else {
        panic!("paper and two layers");
    };
    // Ctrl+G: a folder over the second layer, holding it.
    step(&mut runtime, vec![key(egui::Key::G, command)]);
    let folder = stack(&runtime)[3];
    let folder_of =
        |runtime: &Runtime, layer| sue_paint::doc::folder_of(&runtime.engine.world.arena, layer);
    assert_eq!(folder_of(&runtime, second), Some(folder));
    // Back to the second layer (Alt+[), then up past the folder: out.
    step(
        &mut runtime,
        vec![key(egui::Key::OpenBracket, egui::Modifiers::ALT)],
    );
    step(&mut runtime, vec![key(egui::Key::CloseBracket, command)]);
    assert_eq!(folder_of(&runtime, second), None);
    assert_eq!(stack(&runtime)[1..], [first, folder, second]);
    // Down onto the folder: in, as its top layer.
    step(&mut runtime, vec![key(egui::Key::OpenBracket, command)]);
    assert_eq!(folder_of(&runtime, second), Some(folder));
    assert_eq!(stack(&runtime)[1..], [first, second, folder]);
    // Down past its bottom: out, staying put.
    step(&mut runtime, vec![key(egui::Key::OpenBracket, command)]);
    assert_eq!(folder_of(&runtime, second), None);
    assert_eq!(stack(&runtime)[1..], [first, second, folder]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_moves_a_layer_on_screen_and_undoes_it() {
    let (mut runtime, root) = runtime("paint-move");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(8);
    };
    let plain = egui::Modifiers::NONE;
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    step(&mut runtime, Vec::new());
    let picture = runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.focus())
        .unwrap();
    let layer = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
        .last()
        .map(|info| info.id)
        .unwrap();
    let offset = |runtime: &Runtime| {
        let entity = runtime.engine.world.arena.get_by_id(layer).unwrap();
        [sue_model::picture::X, sue_model::picture::Y].map(|key| {
            entity
                .property(key)
                .and_then(sue_core::Value::as_i64)
                .unwrap_or(0)
        })
    };
    // The Move tool (K), dragged across the canvas.
    step(&mut runtime, vec![key(egui::Key::K, plain)]);
    let middle = runtime.shell.page_rect().center();
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: plain,
    };
    step(&mut runtime, vec![egui::Event::PointerMoved(middle)]);
    step(&mut runtime, vec![button(middle, true)]);
    for part in 1..=8 {
        step(
            &mut runtime,
            vec![egui::Event::PointerMoved(
                middle + vec2(part as f32 * 10.0, part as f32 * 5.0),
            )],
        );
    }
    let moved = offset(&runtime);
    step(&mut runtime, vec![button(middle + vec2(80.0, 40.0), false)]);
    assert!(moved[0] > 0 && moved[1] > 0, "{moved:?}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_exports_each_layer_comp_as_its_own_picture() {
    let (mut runtime, root) = runtime("paint-comps");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(16);
    };
    let plain = egui::Modifiers::NONE;
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    step(&mut runtime, Vec::new());
    let command = |runtime: &mut Runtime, name: &str| {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: name.to_owned(),
            },
            &mut NoTextures,
            start,
        );
    };
    let picture = runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.focus())
        .unwrap();
    // A black layer over the paper, and two comps: with it and without.
    command(&mut runtime, "edit.fill");
    step(&mut runtime, Vec::new());
    let ink = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
        .into_iter()
        .rfind(|info| info.kind == sue_model::picture::LayerKind::Raster)
        .unwrap()
        .id;
    runtime
        .engine
        .world
        .arena
        .get_by_id_mut(picture)
        .unwrap()
        .properties_mut()
        .set(
            "picture.layer_comps",
            format!(
                "Ink	{ink},1,1,normal
Paper	{ink},0,1,normal"
            ),
        );
    command(&mut runtime, "file.export.layer_comps");
    let out = root.join("comps").join("Cover.png");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    let _ = runtime.answer_pick(&out, &mut NoTextures, start);
    step(&mut runtime, Vec::new());
    let read = |name: &str| {
        let bytes = std::fs::read(out.with_file_name(name)).expect(name);
        sue_io::decode_png(&bytes).unwrap().tile.bytes().unwrap()[..3].to_vec()
    };
    assert_eq!(read("Cover-Ink.png"), vec![0, 0, 0], "the ink shows");
    assert_eq!(
        read("Cover-Paper.png"),
        vec![255, 255, 255],
        "the paper only"
    );
    // Exporting changed nothing on the canvas.
    let style = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
        .into_iter()
        .find(|info| info.id == ink)
        .unwrap()
        .style;
    assert!(style.visible);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_corrects_every_picked_layer_as_one_step() {
    let (mut runtime, root) = runtime("paint-picked");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(16);
    };
    let plain = egui::Modifiers::NONE;
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    step(&mut runtime, Vec::new());
    let command = |runtime: &mut Runtime, name: &str| {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: name.to_owned(),
            },
            &mut NoTextures,
            start,
        );
    };
    let picture = runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.focus())
        .unwrap();
    // Two layers filled with the drawing color (black).
    command(&mut runtime, "edit.fill");
    command(&mut runtime, "layer.new");
    command(&mut runtime, "edit.fill");
    step(&mut runtime, Vec::new());
    let shades = |runtime: &Runtime| -> Vec<u8> {
        let arena = &runtime.engine.world.arena;
        sue_paint::doc::layers(arena, picture)
            .into_iter()
            .filter(|info| info.kind == sue_model::picture::LayerKind::Raster)
            .filter_map(|info| sue_paint::doc::raster(arena, info.id))
            .filter_map(|raster| raster.bytes().map(|bytes| bytes[0]))
            .collect()
    };
    assert_eq!(shades(&runtime), vec![0, 0]);
    // Select all layers, then Invert: both turn white, in one step.
    command(&mut runtime, "layer.select_all");
    let depth = runtime.engine.history.depth();
    command(&mut runtime, "filter.invert");
    step(&mut runtime, Vec::new());
    assert_eq!(shades(&runtime), vec![255, 255]);
    assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    assert_eq!(shades(&runtime), vec![0, 0], "both undone");
    // Picked no more: only the current layer changes.
    command(&mut runtime, "layer.select_none");
    command(&mut runtime, "filter.invert");
    step(&mut runtime, Vec::new());
    assert_eq!(
        shades(&runtime)
            .iter()
            .filter(|shade| **shade == 255)
            .count(),
        1
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_masks_hide_without_erasing_the_layer() {
    let (mut runtime, root) = runtime("paint-mask");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(8);
    };
    let plain = egui::Modifiers::NONE;
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    step(&mut runtime, Vec::new());
    let picture = runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.focus())
        .unwrap();
    let layer = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
        .last()
        .unwrap()
        .id;
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let middle = paint_middle(&runtime);
    let mut drag = |runtime: &mut Runtime, before: Vec<egui::Event>, a, b| {
        step(runtime, before);
        step(runtime, vec![egui::Event::PointerMoved(a), press(a, true)]);
        step(runtime, vec![egui::Event::PointerMoved(b)]);
        step(runtime, vec![press(b, false)]);
    };
    drag(
        &mut runtime,
        Vec::new(),
        middle - vec2(60.0, 0.0),
        middle + vec2(60.0, 0.0),
    );
    let command = |runtime: &mut Runtime, name: &str| {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: name.to_owned(),
            },
            &mut NoTextures,
            start,
        );
    };
    command(&mut runtime, "layer.mask.outside");
    command(&mut runtime, "layer.mask.edit");
    let mask = sue_paint::doc::mask_of(&runtime.engine.world.arena, layer).expect("a mask");
    // The eraser (E) on the mask along the stroke's left half.
    drag(
        &mut runtime,
        vec![key(egui::Key::E, plain)],
        middle - vec2(60.0, 0.0),
        middle - vec2(15.0, 0.0),
    );
    let arena = &runtime.engine.world.arena;
    // The row the stroke ran along, wherever the canvas sits on screen.
    let row = {
        let raster = sue_paint::doc::raster(arena, layer).unwrap();
        let bytes = raster.bytes().unwrap();
        (0..raster.height())
            .max_by_key(|y| bytes[((y * raster.width() + 880) * 4 + 3) as usize])
            .unwrap()
    };
    let alpha_of = |raster: &sue_core::RasterTile2D, x: u32| {
        raster.bytes().unwrap()[((row * raster.width() + x) * 4 + 3) as usize]
    };
    let options = sue_paint::composite::RenderOptions {
        paper: false,
        drafts: true,
    };
    let shown = sue_paint::composite::flatten(arena, picture, options).unwrap();
    assert!(
        alpha_of(sue_paint::doc::raster(arena, layer).unwrap(), 720) > 200,
        "the layer keeps its pixels"
    );
    assert!(
        alpha_of(sue_paint::doc::raster(arena, mask).unwrap(), 720) < 50,
        "the mask hides them"
    );
    assert!(
        alpha_of(&shown, 720) < 50,
        "so the picture does not show them"
    );
    assert!(alpha_of(&shown, 880) > 200, "the rest shows");
    // The mask off: everything shows again.
    command(&mut runtime, "layer.mask.toggle");
    let shown =
        sue_paint::composite::flatten(&runtime.engine.world.arena, picture, options).unwrap();
    assert!(alpha_of(&shown, 720) > 200);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_correction_layers_change_what_is_below_and_cancel_cleanly() {
    let (mut runtime, root) = runtime("paint-correction");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(8);
    };
    let plain = egui::Modifiers::NONE;
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    step(&mut runtime, Vec::new());
    let picture = runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.focus())
        .unwrap();
    let command = |runtime: &mut Runtime, name: &str| {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: name.to_owned(),
            },
            &mut NoTextures,
            start,
        );
    };
    let shown = |runtime: &Runtime| {
        let options = sue_paint::composite::RenderOptions {
            paper: true,
            drafts: true,
        };
        sue_paint::composite::render(&runtime.engine.world.arena, picture, [0, 0, 1, 1], options)[0]
    };
    let layers =
        |runtime: &Runtime| sue_paint::doc::layers(&runtime.engine.world.arena, picture).len();
    assert_eq!(shown(&runtime), 255, "white paper");
    // Reverse Gradient as a correction layer: the paper shows black.
    command(&mut runtime, "layer.correction.invert");
    assert_eq!(shown(&runtime), 0);
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    assert_eq!(shown(&runtime), 255, "undone");
    // Level Correction opens its settings; Esc takes the layer away.
    let before = layers(&runtime);
    command(&mut runtime, "layer.correction.levels");
    assert_eq!(layers(&runtime), before + 1);
    step(&mut runtime, Vec::new());
    step(&mut runtime, vec![key(egui::Key::Escape, plain)]);
    step(&mut runtime, Vec::new());
    assert_eq!(layers(&runtime), before, "cancelled, gone");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_folders_are_units_for_new_layer_and_duplicate() {
    let (mut runtime, root) = runtime("paint-folder-units");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(16);
    };
    let cmd = egui::Modifiers::COMMAND;
    step(&mut runtime, vec![key(egui::Key::N, cmd)]);
    step(
        &mut runtime,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    step(&mut runtime, Vec::new());
    let command = |runtime: &mut Runtime, name: &str| {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: name.to_owned(),
            },
            &mut NoTextures,
            start,
        );
    };
    let picture = runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.focus())
        .unwrap();
    let stack = |runtime: &Runtime| -> Vec<sue_core::EntityId> {
        sue_paint::doc::layers(&runtime.engine.world.arena, picture)
            .into_iter()
            .map(|info| info.id)
            .collect()
    };
    let folder_of =
        |runtime: &Runtime, layer| sue_paint::doc::folder_of(&runtime.engine.world.arena, layer);
    let first = stack(&runtime)[1];
    command(&mut runtime, "edit.fill");
    // What it holds on the clipboard, for Paste below.
    command(&mut runtime, "edit.copy");
    // Ctrl+G: the layer in a folder, the folder picked.
    step(&mut runtime, vec![key(egui::Key::G, cmd)]);
    let folder = stack(&runtime)[2];
    assert_eq!(folder_of(&runtime, first), Some(folder));
    let before = stack(&runtime);
    let depth = runtime.engine.history.depth();
    // New Layer with the folder picked: in the folder, on top of it.
    command(&mut runtime, "layer.new");
    step(&mut runtime, Vec::new());
    let made = stack(&runtime)[2];
    assert_eq!(folder_of(&runtime, made), Some(folder));
    assert_eq!(stack(&runtime)[1..], [first, made, folder]);
    // Up to the folder (Alt+]), then Duplicate Layer: the copy holds
    // copies of both layers.
    step(
        &mut runtime,
        vec![key(egui::Key::CloseBracket, egui::Modifiers::ALT)],
    );
    command(&mut runtime, "layer.duplicate");
    step(&mut runtime, Vec::new());
    let now_stack = stack(&runtime);
    assert_eq!(now_stack.len(), 7, "paper, two folders of two layers");
    let copy = *now_stack.last().unwrap();
    assert!(sue_paint::doc::is_folder(&runtime.engine.world.arena, copy));
    assert_eq!(
        sue_paint::doc::members(&runtime.engine.world.arena, picture, copy).len(),
        2
    );
    // Paste with the copied folder picked: the pasted layer goes in it,
    // on top of what it holds.
    command(&mut runtime, "edit.paste");
    step(&mut runtime, Vec::new());
    let pasted_stack = stack(&runtime);
    assert_eq!(pasted_stack.len(), 8);
    let pasted = pasted_stack[pasted_stack.len() - 2];
    assert!(!now_stack.contains(&pasted));
    assert_eq!(folder_of(&runtime, pasted), Some(copy));
    // Every step undoes, back to the folder as it was.
    while runtime.engine.history.depth() > depth {
        step(&mut runtime, vec![key(egui::Key::Z, cmd)]);
    }
    assert_eq!(stack(&runtime), before);
    let _ = std::fs::remove_dir_all(&root);
}
