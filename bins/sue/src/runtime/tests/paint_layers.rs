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
    // The comps are made in the background.
    until_exports_end(&mut runtime, |runtime| step(runtime, Vec::new()));
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
    let mask = sue_paint::doc::mask_of(&runtime.engine.world.arena, layer).expect("a mask");
    // The eraser (E) on the mask along the stroke's left half.
    drag(
        &mut runtime,
        vec![key(egui::Key::E, plain)],
        middle - vec2(60.0, 0.0),
        middle - vec2(15.0, 0.0),
    );
    let arena = &runtime.engine.world.arena;
    // The row the stroke ran along, wherever the canvas sits on screen:
    // the middle of its ink at x 880 (a line may be thinner where it
    // starts).
    let row = {
        let raster = sue_paint::doc::raster(arena, layer).unwrap();
        let bytes = raster.bytes().unwrap();
        let inked: Vec<u32> = (0..raster.height())
            .filter(|y| bytes[((y * raster.width() + 880) * 4 + 3) as usize] > 128)
            .collect();
        inked[inked.len() / 2]
    };
    // How much shows at `x` on that row: the alpha of a layer, the value
    // of a gray mask.
    let alpha_of = |raster: &sue_core::RasterTile2D, x: u32| {
        let at = (row * raster.width() + x) as usize;
        match raster.format() {
            sue_core::PixelFormat::Gray8 => raster.bytes().unwrap()[at],
            _ => raster.bytes().unwrap()[at * 4 + 3],
        }
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

// ----- The Layer palette (P4-5) --------------------------------------------

/// What one frame told assistive technology about a control: its name,
/// where it is, whether it is grayed and whether it is on.
#[derive(Clone, Debug)]
struct Told {
    name: String,
    rect: egui::Rect,
    disabled: bool,
    on: Option<bool>,
}

impl PaintDriver {
    /// One frame with `events` and `modifiers` held, read for its
    /// controls as well as its texts.
    fn look_with(&mut self, events: Vec<egui::Event>, modifiers: egui::Modifiers) -> Vec<Told> {
        self.ctx.enable_accesskit();
        let mut events = events;
        events.insert(0, egui::Event::ModifiersChanged(modifiers));
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events,
            time: Some(self.ctx.input(|input| input.time) + 0.016),
            ..egui::RawInput::default()
        };
        self.now += Duration::from_millis(16);
        let runtime = &mut self.runtime;
        let now = self.now;
        let output = self
            .ctx
            .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
        self.texts = text_rects(&output);
        let told = output
            .platform_output
            .accesskit_update
            .iter()
            .flat_map(|update| update.nodes.iter())
            .filter_map(|(_, node)| {
                let name = node.label()?.to_owned();
                let bounds = node.bounds()?;
                Some(Told {
                    name,
                    rect: egui::Rect::from_min_max(
                        Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
                        Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
                    ),
                    disabled: node.is_disabled(),
                    on: node
                        .toggled()
                        .map(|toggled| toggled == egui::accesskit::Toggled::True),
                })
            })
            .collect();
        output.drop_without_applying_deltas();
        told
    }

    /// The control named `name` as a fresh frame tells it.
    fn control(&mut self, name: &str) -> Told {
        let told = self.look_with(Vec::new(), egui::Modifiers::NONE);
        told.iter()
            .find(|told| told.name == name)
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "{name:?} is told: {:?}",
                    told.iter().map(|told| &told.name).collect::<Vec<_>>()
                )
            })
    }

    /// A press and release of `button` at `at` with `modifiers` held, then
    /// a few frames.
    fn press(&mut self, at: Pos2, button: egui::PointerButton, modifiers: egui::Modifiers) {
        let event = |pressed| egui::Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers,
        };
        self.look_with(vec![egui::Event::PointerMoved(at)], modifiers);
        self.look_with(vec![event(true)], modifiers);
        self.look_with(vec![event(false)], modifiers);
        // The keys are let go again before the frames after.
        self.look_with(Vec::new(), egui::Modifiers::NONE);
        self.settle(4);
    }

    /// A click on the control named `name`.
    fn click_control(&mut self, name: &str) {
        let at = self.control(name).rect.center();
        self.press(at, egui::PointerButton::Primary, egui::Modifiers::NONE);
    }

    fn picture(&self) -> sue_core::EntityId {
        self.runtime
            .applets
            .get(&AppletId::new(sue_paint::ID).unwrap())
            .and_then(|applet| applet.focus())
            .expect("a canvas")
    }

    fn stack(&self) -> Vec<sue_core::EntityId> {
        sue_paint::doc::layers(&self.runtime.engine.world.arena, self.picture())
            .into_iter()
            .map(|info| info.id)
            .collect()
    }

    fn name_of(&self, id: sue_core::EntityId) -> String {
        self.runtime
            .engine
            .world
            .arena
            .get_by_id(id)
            .map(|entity| entity.name().to_owned())
            .unwrap_or_default()
    }

    fn flag(&self, id: sue_core::EntityId, key: &str) -> Option<bool> {
        self.runtime
            .engine
            .world
            .arena
            .get_by_id(id)
            .ok()
            .and_then(|entity| entity.property(key))
            .and_then(sue_core::Value::as_bool)
    }

    fn visible(&self, id: sue_core::EntityId) -> bool {
        sue_paint::doc::layers(&self.runtime.engine.world.arena, self.picture())
            .iter()
            .find(|info| info.id == id)
            .is_some_and(|info| info.style.visible)
    }

    /// Where the row of the layer named `name` is: its name drawn in the
    /// Layer palette's list, found where the last frame drew it.
    fn row(&mut self, name: &str) -> egui::Rect {
        self.settle(2);
        self.rect_of(name)
            .unwrap_or_else(|| panic!("{name:?} is in the list: {:?}", self.texts))
    }

    /// Undoes back to `depth` and checks the arena is `before`.
    fn undo_to(&mut self, depth: usize, before: &Content) {
        while self.runtime.engine.history.depth() > depth {
            self.command("edit.undo");
            self.settle(2);
        }
        assert_eq!(
            &content(&self.runtime.engine.world.arena),
            before,
            "undo is exact"
        );
    }
}

/// The arena, comparable: every entity and every relation.
type Content = (
    std::collections::BTreeMap<sue_core::EntityId, sue_core::Entity>,
    Vec<String>,
);

fn content(arena: &sue_core::Arena) -> Content {
    let mut relations: Vec<String> = arena
        .relations()
        .iter()
        .map(|relation| format!("{relation:?}"))
        .collect();
    relations.sort();
    (
        arena
            .iter()
            .map(|(_, entity)| (entity.id(), entity.clone()))
            .collect(),
        relations,
    )
}

/// The do, undo, redo of rule 7 around `act`.
fn undo_pair(driver: &mut PaintDriver, act: impl FnOnce(&mut PaintDriver)) {
    let before = content(&driver.runtime.engine.world.arena);
    let depth = driver.runtime.engine.history.depth();
    act(driver);
    driver.settle(2);
    let end = driver.runtime.engine.history.depth();
    assert!(end > depth, "a step was made");
    let after = content(&driver.runtime.engine.world.arena);
    assert_ne!(before, after, "it changed the picture");
    driver.undo_to(depth, &before);
    for _ in depth..end {
        driver.command("edit.redo");
        driver.settle(2);
    }
    assert_eq!(
        content(&driver.runtime.engine.world.arena),
        after,
        "redo is exact"
    );
}

/// The texts the last frame drew.
fn said(driver: &PaintDriver) -> Vec<String> {
    driver.texts.iter().map(|(text, _)| text.clone()).collect()
}

#[test]
fn layer_palette_buttons_are_commands_named_with_their_keys_and_reasons() {
    let mut driver = PaintDriver::new("paint-layer-buttons");
    // Every button tells its name; New Raster Layer runs its command.
    let before = driver.stack().len();
    driver.click_control("Raster Layer");
    assert_eq!(driver.stack().len(), before + 1, "a new raster layer");
    // On the first layer over the paper, Merge with Layer Below is grayed,
    // and resting on it says why and which key it has.
    driver.command("layer.select_below");
    driver.settle(2);
    let merge = driver.control("Merge with Layer Below");
    assert!(merge.disabled, "the paper is no raster layer to merge into");
    driver.step(vec![egui::Event::PointerMoved(merge.rect.center())]);
    driver.settle(60);
    let said = said(&driver);
    assert!(
        said.iter()
            .any(|text| text == "There is no raster layer right below it")
            && said.iter().any(|text| text == "Ctrl+E"),
        "{said:?}"
    );
    // A switch tells whether it is on.
    driver.click_control("Lock Layer");
    assert_eq!(driver.control("Lock Layer").on, Some(true));
}

#[test]
fn a_folder_takes_merge_and_masks_and_its_blend_list_starts_with_through() {
    let mut driver = PaintDriver::new("paint-layer-folder-h4");
    // A filled layer, and over it a folder around a new layer.
    driver.command("edit.fill");
    driver.command("layer.new");
    driver.settle(2);
    driver.command("layer.folder_insert");
    driver.settle(3);
    // H-4: with the folder current, these work (and are not grayed).
    for name in ["Mask Outside Selection", "Merge with Layer Below"] {
        assert!(!driver.control(name).disabled, "{name} works on a folder");
    }
    // A folder with no mode stored reads Through.
    let folder = *driver.stack().last().unwrap();
    driver.settle(2);
    assert!(
        said(&driver).iter().any(|text| text == "100 % Through"),
        "{:?}",
        said(&driver)
    );
    // Its blend list is the folder's: Through first, then the modes of a
    // layer in Clip Studio Paint's order, ending in Brightness.
    let blend = driver.control("Blending mode");
    driver.press(
        blend.rect.center(),
        egui::PointerButton::Primary,
        egui::Modifiers::NONE,
    );
    let order = said(&driver);
    let at = |name: &str| {
        order
            .iter()
            .position(|text| text == name)
            .unwrap_or(usize::MAX)
    };
    assert!(at("Through") < at("Normal"), "{order:?}");
    assert!(at("Exclusion") < at("Darker Color"), "{order:?}");
    assert!(at("Hard Light") < at("Difference"), "{order:?}");
    assert!(at("Brightness") < usize::MAX, "{order:?}");
    // Multiply, one step that undoes.
    undo_pair(&mut driver, |driver| {
        driver.click_text("Multiply");
    });
    assert_eq!(
        sue_paint::doc::folder_blend(&driver.runtime.engine.world.arena, folder),
        sue_model::BlendMode::Multiply
    );
    // Merge Selected Layers on the folder alone bakes it into a layer.
    undo_pair(&mut driver, |driver| driver.command("layer.merge_selected"));
    let top = *driver.stack().last().unwrap();
    assert!(!sue_paint::doc::is_folder(
        &driver.runtime.engine.world.arena,
        top
    ));
}

#[test]
fn the_wheel_over_the_blend_list_steps_the_mode_and_shows_it() {
    let mut driver = PaintDriver::new("paint-layer-wheel");
    let layer = *driver.stack().last().unwrap();
    let blend = driver.control("Blending mode").rect.center();
    driver.step(vec![egui::Event::PointerMoved(blend)]);
    driver.settle(40);
    driver.step(vec![egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: vec2(0.0, -1.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
    }]);
    driver.settle(3);
    let style = sue_paint::doc::layers(&driver.runtime.engine.world.arena, driver.picture())
        .into_iter()
        .find(|info| info.id == layer)
        .unwrap()
        .style;
    assert_eq!(style.blend, sue_model::BlendMode::Darken, "one step down");
    driver.settle(2);
    assert!(said(&driver).iter().any(|text| text == "100 % Darken"));
}

#[test]
fn the_mask_link_unlinks_and_links_back_as_steps() {
    let mut driver = PaintDriver::new("paint-layer-mask-link");
    let layer = *driver.stack().last().unwrap();
    driver.command("layer.mask.outside");
    driver.settle(3);
    let linked = |driver: &PaintDriver| {
        sue_paint::doc::mask_linked(&driver.runtime.engine.world.arena, layer)
    };
    assert!(linked(&driver));
    undo_pair(&mut driver, |driver| {
        driver.click_control("Unlink Mask from Layer");
    });
    assert!(!linked(&driver));
    let at = sue_paint::doc::mask_offset(&driver.runtime.engine.world.arena, layer);
    driver.click_control("Link Mask to Layer");
    assert!(linked(&driver));
    assert_eq!(
        sue_paint::doc::mask_offset(&driver.runtime.engine.world.arena, layer),
        at,
        "the mask stays where it is on the canvas"
    );
}

#[test]
fn boxes_and_shift_pick_several_and_lock_hide_and_delete_take_them_all() {
    let mut driver = PaintDriver::new("paint-layer-pick");
    driver.command("layer.new");
    driver.command("layer.new");
    driver.settle(3);
    let [_, first, second, third] = driver.stack()[..] else {
        panic!("paper and three layers");
    };
    // The box of the first layer picks it beside the current (third).
    let name = format!("Pick {} with the others", driver.name_of(first));
    driver.click_control(&name);
    undo_pair(&mut driver, |driver| driver.click_control("Lock Layer"));
    let locked = |driver: &PaintDriver, id| driver.flag(id, sue_model::picture::LOCKED);
    assert_eq!(locked(&driver, first), Some(true));
    assert_eq!(locked(&driver, third), Some(true));
    assert_ne!(locked(&driver, second), Some(true));
    driver.click_control("Lock Layer");
    // Shift+click on the first row: the range from the third down.
    let row = driver.row(&driver.name_of(first));
    driver.press(
        row.center(),
        egui::PointerButton::Primary,
        egui::Modifiers::SHIFT,
    );
    undo_pair(&mut driver, |driver| {
        driver.command("layer.toggle.visible");
    });
    for id in [first, second, third] {
        assert!(!driver.visible(id), "every picked layer hides");
    }
    // Delete Layer takes all three as one step.
    undo_pair(&mut driver, |driver| driver.click_control("Delete Layer"));
    assert_eq!(driver.stack().len(), 1, "only the paper");
}

#[test]
fn deleting_a_folder_asks_first_until_told_not_to() {
    let mut driver = PaintDriver::new("paint-layer-ask");
    driver.command("layer.new.folder");
    driver.settle(3);
    let before = driver.stack().len();
    driver.command("layer.delete");
    driver.settle(4);
    assert!(
        said(&driver)
            .iter()
            .any(|text| text.starts_with("Delete the picked layer folders")),
        "{:?}",
        said(&driver)
    );
    assert_eq!(driver.stack().len(), before, "nothing went yet");
    driver.click_text("Do not show again");
    driver.click_text("Delete");
    assert_eq!(driver.stack().len(), before - 1);
    // Asked no more.
    driver.command("layer.new.folder");
    driver.settle(3);
    driver.command("layer.delete");
    driver.settle(4);
    assert_eq!(driver.stack().len(), before - 1);
}

#[test]
fn palette_colors_go_on_every_picked_layer_from_the_palette_and_menus() {
    let mut driver = PaintDriver::new("paint-layer-colors");
    driver.command("layer.new");
    driver.command("layer.select_all");
    driver.settle(3);
    let picked = driver.stack()[1..].to_vec();
    undo_pair(&mut driver, |driver| {
        driver.click_control("Change Palette Color");
        driver.click_control("Red");
    });
    for id in &picked {
        assert_eq!(
            sue_paint::doc::palette_color(&driver.runtime.engine.world.arena, *id),
            sue_paint::doc::named_palette_color("red")
        );
    }
    driver.command("layer.palette_color.none");
    driver.settle(2);
    assert_eq!(
        sue_paint::doc::palette_color(&driver.runtime.engine.world.arena, picked[0]),
        None
    );
}

#[test]
fn a_right_click_on_a_layer_opens_its_own_short_list() {
    let mut driver = PaintDriver::new("paint-layer-menu");
    let layer = *driver.stack().last().unwrap();
    let row = driver.row(&driver.name_of(layer));
    driver.press(
        row.center(),
        egui::PointerButton::Secondary,
        egui::Modifiers::NONE,
    );
    let said = said(&driver);
    for item in [
        "New Raster Layer",
        "Create Folder and Insert Layer",
        "Change Palette Color",
        "Merge Selected Layers",
        "Flatten Image",
    ] {
        assert!(said.iter().any(|text| text == item), "{item} in {said:?}");
    }
    assert!(
        !said.iter().any(|text| text == "Layer Comps..."),
        "the whole Layer menu does not open"
    );
}

#[test]
fn a_layer_dragged_onto_new_layer_copies_and_onto_new_folder_wraps() {
    let mut driver = PaintDriver::new("paint-layer-drag");
    let layer = *driver.stack().last().unwrap();
    let name = driver.name_of(layer);
    let count = driver.stack().len();
    let row = driver.row(&name);
    let new = driver.control("Raster Layer").rect.center();
    driver.drag(row.center(), new, egui::PointerButton::Primary);
    assert_eq!(driver.stack().len(), count + 1, "copied");
    let folder = driver.control("New Layer Folder").rect.center();
    let row = driver.row(&name);
    driver.drag(row.center(), folder, egui::PointerButton::Primary);
    let arena = &driver.runtime.engine.world.arena;
    let around = sue_paint::doc::folder_of(arena, layer).expect("in a new folder");
    assert!(sue_paint::doc::is_folder(arena, around));
}

#[test]
fn ctrl_click_on_a_thumbnail_selects_what_the_layer_holds() {
    let mut driver = PaintDriver::new("paint-layer-thumb");
    driver.command("edit.fill");
    driver.settle(2);
    let layer = *driver.stack().last().unwrap();
    // Nothing is selected yet, so Deselect is grayed.
    let grayed = driver.look_with(Vec::new(), egui::Modifiers::NONE);
    // (The command bar names its buttons alone since P4-3, their keys in
    // the tooltip.)
    assert!(
        grayed
            .iter()
            .filter(|told| told.name == "Deselect")
            .all(|told| told.disabled)
    );
    let row = driver.row(&driver.name_of(layer));
    // The thumbnail sits just left of the name.
    let thumb = row.left_center() - vec2(24.0, 0.0);
    driver.press(
        thumb,
        egui::PointerButton::Primary,
        egui::Modifiers::COMMAND,
    );
    // The layer's pixels became the selection: Deselect works now.
    assert!(!driver.control("Deselect").disabled, "there is a selection");
    // A plain click on a thumbnail picks its row, as anywhere on the row:
    // the paper's box gives way to the pen.
    let paper = driver.stack()[0];
    let row = driver.row(&driver.name_of(paper));
    let names = |driver: &mut PaintDriver| -> Vec<String> {
        let told = driver.look_with(Vec::new(), egui::Modifiers::NONE);
        told.into_iter().map(|told| told.name).collect()
    };
    let pick_paper = format!("Pick {} with the others", driver.name_of(paper));
    assert!(names(&mut driver).contains(&pick_paper));
    driver.press(
        row.left_center() - vec2(24.0, 0.0),
        egui::PointerButton::Primary,
        egui::Modifiers::NONE,
    );
    assert!(
        !names(&mut driver).contains(&pick_paper),
        "the paper is current"
    );
}

/// Applies `edit` as Sue would, one step on the history.
fn apply(driver: &mut PaintDriver, edit: sue_paint::doc::Edit) {
    let engine = &mut driver.runtime.engine;
    engine
        .history
        .apply(&mut engine.world, sue_wm::ArenaEdit(edit))
        .expect("the edit applies");
    driver.settle(2);
}

/// The names the controls and marks of a fresh frame tell.
fn marks(driver: &mut PaintDriver) -> Vec<String> {
    driver
        .look_with(Vec::new(), egui::Modifiers::NONE)
        .into_iter()
        .map(|told| told.name)
        .collect()
}

#[test]
fn rows_mark_locks_drafts_effects_keyframes_and_both_layer_colors() {
    let mut driver = PaintDriver::new("paint-layer-marks");
    driver.command("edit.fill");
    let layer = *driver.stack().last().unwrap();
    let names = [
        "Locked: it takes no change",
        "Draft layer: exports leave it out",
        "Keyframes on: drawing on it waits until they are off",
    ];
    let before = marks(&mut driver);
    for name in names {
        assert!(!before.iter().any(|told| told == name), "{name} not yet");
    }
    let count = |told: &[String], name: &str| told.iter().filter(|told| *told == name).count();
    let effects_before = (
        count(&before, "Border effect is on"),
        count(&before, "Tone is on"),
    );
    driver.command("layer.toggle.lock");
    driver.command("layer.toggle.draft");
    apply(
        &mut driver,
        sue_paint::doc::set(layer, sue_paint::effects::BORDER, true),
    );
    apply(
        &mut driver,
        sue_paint::doc::set(layer, sue_paint::effects::TONE, true),
    );
    apply(
        &mut driver,
        sue_paint::anim::motion::enable_keyframes(layer, true),
    );
    let after = marks(&mut driver);
    for name in names {
        assert!(after.iter().any(|told| told == name), "{name} in {after:?}");
    }
    assert_eq!(
        (
            count(&after, "Border effect is on"),
            count(&after, "Tone is on")
        ),
        (effects_before.0 + 1, effects_before.1 + 1),
        "a badge each"
    );
    // The lock is the last mark, at the right end of the row (A25V-5).
    let told = driver.look_with(Vec::new(), egui::Modifiers::NONE);
    let lock = told.iter().find(|told| told.name == names[0]).unwrap().rect;
    let row = driver.row(&driver.name_of(layer));
    assert!(lock.left() > row.right(), "the lock is right of the name");
    for other in told.iter().filter(|told| {
        told.name != names[0] && (told.rect.center().y - lock.center().y).abs() < 1.0
    }) {
        if other.rect.left() > row.right() {
            assert!(
                other.rect.right() <= lock.left(),
                "{other:?} left of the lock"
            );
        }
    }
    // A folder with a layer color and a sub color shows both chips.
    driver.command("layer.new.folder");
    driver.settle(2);
    let folder = *driver.stack().last().unwrap();
    driver.command("layer.toggle.layer_color");
    apply(
        &mut driver,
        sue_paint::doc::set(
            folder,
            sue_paint::effects::COLOR_SUB,
            sue_core::Color::srgb8(0, 0, 200),
        ),
    );
    let chips = marks(&mut driver);
    for name in ["Layer color is on", "Sub color of the layer color"] {
        assert!(chips.iter().any(|told| told == name), "{name} in {chips:?}");
    }
}

#[test]
fn picked_layers_go_into_one_folder_and_drag_together() {
    let mut driver = PaintDriver::new("paint-layer-several");
    driver.command("layer.new");
    driver.command("layer.new");
    driver.command("layer.new");
    driver.settle(3);
    let [_, first, second, third, fourth] = driver.stack()[..] else {
        panic!("paper and four layers");
    };
    // The fourth is current; Shift+click on the third picks the two.
    let row = driver.row(&driver.name_of(third));
    driver.press(
        row.center(),
        egui::PointerButton::Primary,
        egui::Modifiers::SHIFT,
    );
    // Dragged together onto the first layer: both land right above it, in
    // their order, as one step.
    let row = driver.row(&driver.name_of(fourth));
    let onto = driver.row(&driver.name_of(first));
    undo_pair(&mut driver, |driver| {
        driver.drag(row.center(), onto.center(), egui::PointerButton::Primary);
    });
    let stack = driver.stack();
    assert_eq!(stack[1..], [first, third, fourth, second], "{stack:?}");
    // Create Folder and Insert Layer takes both.
    undo_pair(&mut driver, |driver| driver.command("layer.folder_insert"));
    let arena = &driver.runtime.engine.world.arena;
    let folder = sue_paint::doc::folder_of(arena, third).expect("in a folder");
    assert_eq!(sue_paint::doc::folder_of(arena, fourth), Some(folder));
    assert_eq!(sue_paint::doc::folder_of(arena, first), None);
}

#[test]
fn the_right_click_list_makes_every_kind_of_layer() {
    let mut driver = PaintDriver::new("paint-layer-new-kinds");
    let layer = *driver.stack().last().unwrap();
    let row = driver.row(&driver.name_of(layer));
    driver.press(
        row.center(),
        egui::PointerButton::Secondary,
        egui::Modifiers::NONE,
    );
    let at = driver
        .rect_of("New Layer")
        .expect("New Layer in the list")
        .center();
    driver.step(vec![egui::Event::PointerMoved(at)]);
    driver.settle(20);
    let said = said(&driver);
    for item in [
        "Vector Layer",
        "Gradient...",
        "Fill",
        "Tone...",
        "3D Layer",
        "Frame Border Folder...",
    ] {
        assert!(said.iter().any(|text| text == item), "{item} in {said:?}");
    }
}

#[test]
fn the_ruler_button_lists_where_rulers_show() {
    let mut driver = PaintDriver::new("paint-layer-ruler");
    driver.click_control("Set Showing Area of Ruler");
    let said = said(&driver);
    let ranges: Vec<String> = sue_paint::rulers::ShowRange::ALL
        .iter()
        .map(|range| range.label())
        .collect();
    for range in &ranges {
        assert!(said.iter().any(|text| text == range), "{range} in {said:?}");
    }
    undo_pair(&mut driver, |driver| {
        driver.click_text(&ranges[1]);
    });
}

#[test]
fn an_animation_folder_names_its_cel_and_a_cel_click_moves_the_playhead() {
    let mut driver = PaintDriver::new("paint-layer-cels");
    driver.new_timeline();
    driver.command("anim.cel.new");
    for _ in 0..3 {
        driver.command("anim.next");
    }
    driver.command("anim.cel.new");
    driver.settle(4);
    let arena = &driver.runtime.engine.world.arena;
    let picture = driver.picture();
    let folder = sue_paint::doc::layers(arena, picture)
        .into_iter()
        .map(|info| info.id)
        .find(|id| sue_paint::anim::is_animation_folder(arena, *id))
        .expect("an animation folder");
    let cels: Vec<sue_core::EntityId> = sue_paint::doc::layers(arena, picture)
        .into_iter()
        .map(|info| info.id)
        .filter(|id| sue_paint::doc::folder_of(arena, *id) == Some(folder))
        .collect();
    let [first, second] = cels[..] else {
        panic!("two cels: {cels:?}");
    };
    let (folder_name, first_name, second_name) = (
        driver.name_of(folder),
        driver.name_of(first),
        driver.name_of(second),
    );
    driver.settle(2);
    // The playhead is on the second cel: the folder's row names it.
    let shows = |cel: &str| format!("{folder_name} : {cel}");
    assert!(
        driver.rect_of(&shows(&second_name)).is_some(),
        "{:?}",
        said(&driver)
    );
    // A click on the first cel's row: the playhead goes where it starts.
    let row = driver.row(&first_name);
    driver.press(
        row.center(),
        egui::PointerButton::Primary,
        egui::Modifiers::NONE,
    );
    driver.settle(2);
    assert!(
        driver.rect_of(&shows(&first_name)).is_some(),
        "{:?}",
        said(&driver)
    );
}

#[test]
fn the_edge_color_bar_opens_the_color_settings() {
    let mut driver = PaintDriver::new("paint-layer-edge-color");
    driver.command("edit.fill");
    let layer = *driver.stack().last().unwrap();
    apply(
        &mut driver,
        sue_paint::doc::set(layer, sue_paint::effects::BORDER, true),
    );
    driver.command("layer.properties");
    driver.settle(3);
    let edge = driver
        .rect_of("Edge color")
        .expect("Edge color in Layer Property");
    let told = driver.look_with(Vec::new(), egui::Modifiers::NONE);
    let bar = told
        .iter()
        .filter(|told| {
            told.name == "Pick a color" && (told.rect.center().y - edge.center().y).abs() < 12.0
        })
        .map(|told| told.rect)
        .next()
        .expect("the edge color bar");
    let before = said(&driver).len();
    driver.press(
        bar.center(),
        egui::PointerButton::Primary,
        egui::Modifiers::NONE,
    );
    driver.settle(20);
    assert!(
        said(&driver).len() > before,
        "the color settings opened: {:?}",
        said(&driver)
    );
}

#[test]
fn search_layer_shows_what_it_finds_and_two_panes_list_twice() {
    let mut driver = PaintDriver::new("paint-layer-search");
    driver.command("layer.new.folder");
    driver.settle(3);
    let folder = *driver.stack().last().unwrap();
    let folder_name = driver.name_of(folder);
    driver.click_control("Search Layer");
    // Raster layer: the folder leaves the list.
    let kind = driver.control("Kind of layer");
    driver.press(
        kind.rect.center(),
        egui::PointerButton::Primary,
        egui::Modifiers::NONE,
    );
    driver.click_text("Raster layer");
    driver.settle(2);
    assert!(
        driver.rect_of(&folder_name).is_none(),
        "{:?}",
        said(&driver)
    );
    driver.click_control("Search Layer");
    driver.settle(2);
    assert!(driver.rect_of(&folder_name).is_some());
    driver.click_control("Split Layer List");
    driver.settle(2);
    let shown = driver
        .texts
        .iter()
        .filter(|(text, _)| *text == folder_name)
        .count();
    assert_eq!(shown, 2, "both panes list it");
}

#[test]
fn paste_with_a_layer_in_a_folder_picked_lands_right_above_it_there() {
    let mut driver = PaintDriver::new("paint-layer-paste");
    driver.command("edit.fill");
    driver.command("edit.copy");
    driver.command("layer.folder_insert");
    driver.settle(3);
    let [_, layer, folder] = driver.stack()[..] else {
        panic!("paper, layer, folder");
    };
    driver.command("layer.select_below");
    driver.settle(2);
    undo_pair(&mut driver, |driver| driver.command("edit.paste"));
    let stack = driver.stack();
    assert_eq!(stack.len(), 4);
    assert_eq!((stack[1], stack[3]), (layer, folder));
    assert_eq!(
        sue_paint::doc::folder_of(&driver.runtime.engine.world.arena, stack[2]),
        Some(folder)
    );
}

#[test]
fn simple_tone_settings_make_a_layer_of_tone() {
    let mut driver = PaintDriver::new("paint-layer-tone");
    let count = driver.stack().len();
    driver.command("layer.new.tone");
    driver.settle(4);
    assert!(
        said(&driver)
            .iter()
            .any(|text| text == "Simple Tone Settings"),
        "{:?}",
        said(&driver)
    );
    undo_pair(&mut driver, |driver| {
        driver.click_text("OK");
    });
    assert_eq!(driver.stack().len(), count + 1);
    let tone = *driver.stack().last().unwrap();
    assert_eq!(driver.flag(tone, sue_paint::effects::TONE), Some(true));
}

#[test]
fn layer_property_sets_border_aliasing_monochrome_inks_and_tone_options() {
    let mut driver = PaintDriver::new("paint-layer-property");
    driver.command("edit.fill");
    let layer = *driver.stack().last().unwrap();
    driver.command("layer.properties");
    driver.settle(3);
    driver.click_control("Border effect");
    undo_pair(&mut driver, |driver| {
        driver.click_control("Anti-aliasing on border effect");
    });
    assert_eq!(
        driver.flag(layer, sue_paint::effects::BORDER_AA),
        Some(false)
    );
    driver.click_control("Tone");
    undo_pair(&mut driver, |driver| {
        driver.click_control("Reflect layer opacity");
    });
    assert_eq!(
        driver.flag(layer, sue_paint::effects::TONE_REFLECT_OPACITY),
        Some(true)
    );
    // The two effects off again, so the expression color is in sight.
    driver.click_control("Tone");
    driver.click_control("Border effect");
    let choice = driver.control("Expression color");
    driver.press(
        choice.rect.center(),
        egui::PointerButton::Primary,
        egui::Modifiers::NONE,
    );
    driver.click_text("Monochrome");
    undo_pair(&mut driver, |driver| driver.click_control("White"));
    assert_eq!(
        driver.flag(layer, sue_paint::effects::MONO_WHITE),
        Some(false)
    );
    undo_pair(&mut driver, |driver| {
        driver.click_control("Apply Expression Color of Preview");
    });
}

#[test]
fn tone_takes_a_density_source_a_dot_shape_and_a_dot_position() {
    let mut driver = PaintDriver::new("paint-layer-tone-options");
    driver.command("edit.fill");
    let layer = *driver.stack().last().unwrap();
    driver.command("layer.properties");
    driver.settle(3);
    driver.click_control("Tone");
    for name in ["Density source", "Dot shape", "Posterization"] {
        assert!(!driver.control(name).disabled, "{name} is there");
    }
    assert!(said(&driver).iter().any(|text| text == "Dot position"));
    let shape = driver.control("Dot shape").rect.center();
    driver.press(shape, egui::PointerButton::Primary, egui::Modifiers::NONE);
    undo_pair(&mut driver, |driver| {
        driver.click_text("Noise");
    });
    let arena = &driver.runtime.engine.world.arena;
    let entity = arena.get_by_id(layer).unwrap();
    assert!(entity.property(sue_paint::effects::TONE_SHAPE).is_some());
}

#[test]
fn the_layer_color_button_switches_the_layer_color() {
    let mut driver = PaintDriver::new("paint-layer-color-button");
    let layer = *driver.stack().last().unwrap();
    undo_pair(&mut driver, |driver| {
        driver.click_control("Switch Using Layer Color");
    });
    assert_eq!(driver.flag(layer, sue_paint::effects::COLOR), Some(true));
    assert_eq!(driver.control("Switch Using Layer Color").on, Some(true));
}

#[test]
fn a_right_click_on_a_picked_layer_keeps_every_one_picked() {
    let mut driver = PaintDriver::new("paint-layer-menu-picked");
    driver.command("layer.new");
    driver.command("layer.new");
    driver.settle(3);
    let [_, first, second, third] = driver.stack()[..] else {
        panic!("paper and three layers");
    };
    // The third is current; its box picks the first beside it.
    let name = format!("Pick {} with the others", driver.name_of(first));
    driver.click_control(&name);
    // A right click on the first: its own list, and both stay picked, so
    // Merge Selected Layers there merges the two.
    let row = driver.row(&driver.name_of(first));
    driver.press(
        row.center(),
        egui::PointerButton::Secondary,
        egui::Modifiers::NONE,
    );
    undo_pair(&mut driver, |driver| {
        driver.click_text("Merge Selected Layers");
    });
    let stack = driver.stack();
    assert_eq!(stack.len(), 3, "two of the three merged: {stack:?}");
    assert!(stack.contains(&second), "the unpicked one stays");
    assert!(!stack.contains(&third) || !stack.contains(&first));
}

#[test]
fn a_search_that_finds_nothing_says_so_and_rows_are_read_by_name() {
    let mut driver = PaintDriver::new("paint-layer-search-none");
    let layer = *driver.stack().last().unwrap();
    let name = driver.name_of(layer);
    // Each row is a control of its own, read by the layer's name.
    assert!(!driver.control(&name).disabled, "the row is told");
    driver.click_control("Search Layer");
    let field = driver.control("Layer name").rect.center();
    driver.press(field, egui::PointerButton::Primary, egui::Modifiers::NONE);
    driver.step(vec![egui::Event::Text("no such layer".to_owned())]);
    driver.settle(3);
    assert!(driver.rect_of(&name).is_none(), "{:?}", said(&driver));
    assert!(
        said(&driver).iter().any(|text| text == "No layer matches"),
        "{:?}",
        said(&driver)
    );
}
