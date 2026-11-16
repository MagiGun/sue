//! Sue Paint: transforms, selections, fills, liquify.

use super::*;

#[test]
fn sue_paint_puppet_warp_bends_a_stroke_in_one_undo_step() {
    let (mut runtime, root) = runtime("paint-warp");
    let ctx = egui::Context::default();
    let start = Instant::now();
    let mut now = start;
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        now,
    );
    frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| {
        frame_with(runtime, &ctx, now, events);
        now += Duration::from_millis(8);
    };
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(
        &mut runtime,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    step(&mut runtime, Vec::new());
    let arena = &runtime.engine.world.arena;
    let picture = arena
        .iter()
        .find(|(_, entity)| sue_model::picture::is_picture(entity))
        .map(|(_, entity)| entity.id())
        .unwrap();
    let layer = sue_paint::doc::layers(arena, picture).last().unwrap().id;
    let ink = |runtime: &Runtime, x: u32, rows: std::ops::Range<u32>| {
        let raster = sue_paint::doc::raster(&runtime.engine.world.arena, layer).unwrap();
        rows.map(|y| {
            u32::from(raster.bytes().unwrap()[((y * raster.width() + x) * 4 + 3) as usize])
        })
        .sum::<u32>()
    };
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    // A mouse stroke across the middle, from x = 800 to past 990.
    let from = paint_middle(&runtime);
    let to = from + vec2(100.0, 0.0);
    step(
        &mut runtime,
        vec![egui::Event::PointerMoved(from), press(from, true)],
    );
    step(
        &mut runtime,
        vec![
            egui::Event::PointerMoved(from + vec2(50.0, 0.0)),
            egui::Event::PointerMoved(to),
        ],
    );
    step(&mut runtime, vec![press(to, false)]);
    assert!(ink(&runtime, 960, 580..620) > 0, "the stroke is there");
    let depth = runtime.engine.history.depth();

    // Puppet warp: pin the start, drag the end up.
    let paint = AppletId::new(sue_paint::ID).unwrap();
    runtime.request(
        ShellRequest::AppletCommand {
            applet: paint,
            command: "edit.transform.puppet".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    step(&mut runtime, Vec::new());
    step(
        &mut runtime,
        vec![egui::Event::PointerMoved(from), press(from, true)],
    );
    step(&mut runtime, vec![press(from, false)]);
    let end = to - vec2(8.0, 0.0);
    let up = end + vec2(0.0, -60.0);
    step(
        &mut runtime,
        vec![egui::Event::PointerMoved(end), press(end, true)],
    );
    step(
        &mut runtime,
        vec![
            egui::Event::PointerMoved(end + vec2(0.0, -30.0)),
            egui::Event::PointerMoved(up),
        ],
    );
    step(&mut runtime, vec![press(up, false)]);
    assert_eq!(
        runtime.engine.history.depth(),
        depth,
        "a preview, not history yet"
    );
    step(
        &mut runtime,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    step(&mut runtime, Vec::new());
    assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");
    assert_eq!(ink(&runtime, 960, 580..620), 0, "the end left the middle");
    assert!(ink(&runtime, 960, 440..580) > 0, "and went up");
    assert!(ink(&runtime, 810, 580..620) > 0, "the pinned start stayed");

    // Undo puts the stroke back as it was.
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    assert!(ink(&runtime, 960, 580..620) > 0);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_scale_rotate_moves_a_stroke_in_one_undo_step() {
    let (mut runtime, root) = runtime("paint-transform");
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
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
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
    let layer = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
        .last()
        .unwrap()
        .id;
    let ink = |runtime: &Runtime, rows: std::ops::Range<u32>| {
        let raster = sue_paint::doc::raster(&runtime.engine.world.arena, layer).unwrap();
        rows.map(|y| {
            u32::from(raster.bytes().unwrap()[((y * raster.width() + 800) * 4 + 3) as usize])
        })
        .sum::<u32>()
    };
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let middle = paint_middle(&runtime);
    let (from, to) = (middle - vec2(60.0, 0.0), middle + vec2(60.0, 0.0));
    step(
        &mut runtime,
        vec![egui::Event::PointerMoved(from), press(from, true)],
    );
    step(
        &mut runtime,
        vec![
            egui::Event::PointerMoved(middle),
            egui::Event::PointerMoved(to),
        ],
    );
    step(&mut runtime, vec![press(to, false)]);
    assert!(ink(&runtime, 580..620) > 0);
    let depth = runtime.engine.history.depth();

    // Ctrl+T, drag from inside the box down, Enter.
    step(
        &mut runtime,
        vec![key(egui::Key::T, egui::Modifiers::COMMAND)],
    );
    step(&mut runtime, Vec::new());
    let down = middle + vec2(0.0, 80.0);
    step(
        &mut runtime,
        vec![egui::Event::PointerMoved(middle), press(middle, true)],
    );
    step(
        &mut runtime,
        vec![
            egui::Event::PointerMoved(middle + vec2(0.0, 40.0)),
            egui::Event::PointerMoved(down),
        ],
    );
    step(&mut runtime, vec![press(down, false)]);
    assert_eq!(
        runtime.engine.history.depth(),
        depth,
        "a preview, not history yet"
    );
    step(
        &mut runtime,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
    step(&mut runtime, Vec::new());
    assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");
    assert_eq!(ink(&runtime, 580..620), 0, "the stroke left its place");
    assert!(ink(&runtime, 700..820) > 0, "and moved down");
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    assert!(ink(&runtime, 580..620) > 0, "undo puts it back");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_crops_to_the_selection_in_one_undo_step() {
    let (mut runtime, root) = runtime("paint-crop");
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
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
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
    let size = |runtime: &Runtime| {
        sue_model::picture::size(runtime.engine.world.arena.get_by_id(picture).unwrap()).unwrap()
    };
    assert_eq!(size(&runtime), [1600, 1200]);
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let middle = paint_middle(&runtime);
    // M picks the rectangle selection; drag a box around the middle.
    step(&mut runtime, vec![key(egui::Key::M, egui::Modifiers::NONE)]);
    let (a, b) = (middle - vec2(40.0, 30.0), middle + vec2(40.0, 30.0));
    step(
        &mut runtime,
        vec![egui::Event::PointerMoved(a), press(a, true)],
    );
    step(&mut runtime, vec![egui::Event::PointerMoved(b)]);
    step(&mut runtime, vec![press(b, false)]);
    let depth = runtime.engine.history.depth();
    runtime.request(
        ShellRequest::AppletCommand {
            applet: paint.clone(),
            command: "edit.crop".to_owned(),
        },
        &mut NoTextures,
        start,
    );
    let [width, height] = size(&runtime);
    assert!(
        width < 400 && height < 300 && width > height,
        "cropped to {width} by {height}"
    );
    assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");
    let layer = sue_paint::doc::layers(&runtime.engine.world.arena, picture)
        .last()
        .unwrap()
        .id;
    assert_eq!(
        sue_paint::doc::raster(&runtime.engine.world.arena, layer)
            .unwrap()
            .width(),
        width
    );
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    assert_eq!(size(&runtime), [1600, 1200]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_lasso_fill_close_and_fill_and_copy_stamp_paint() {
    let (mut runtime, root) = runtime("paint-lasso-stamp");
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
    // Painted pixels on every layer but the paper.
    let painted = |runtime: &Runtime| -> usize {
        let arena = &runtime.engine.world.arena;
        sue_paint::doc::layers(arena, picture)
            .iter()
            .filter_map(|info| sue_paint::doc::raster(arena, info.id))
            .filter_map(|raster| raster.bytes())
            .filter(|bytes| bytes.as_chunks::<4>().0.iter().any(|pixel| pixel[3] < 255))
            .map(|bytes| {
                bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|pixel| pixel[3] > 0)
                    .count()
            })
            .sum()
    };
    let middle = paint_middle(&runtime);
    let drag = |runtime: &mut Runtime,
                step: &mut dyn FnMut(&mut Runtime, Vec<egui::Event>),
                points: &[Pos2],
                modifiers: egui::Modifiers| {
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        };
        step(
            runtime,
            vec![egui::Event::PointerMoved(points[0]), press(points[0], true)],
        );
        for point in &points[1..] {
            step(runtime, vec![egui::Event::PointerMoved(*point)]);
        }
        step(runtime, vec![press(points[points.len() - 1], false)]);
        step(runtime, Vec::new());
    };
    let ring = |center: Pos2, radius: f32| -> Vec<Pos2> {
        (0..=24)
            .map(|index| {
                let angle = index as f32 / 24.0 * std::f32::consts::TAU;
                center + vec2(angle.cos(), angle.sin()) * radius
            })
            .collect()
    };
    // A tool's key goes round the tools that share it, so each pick
    // starts from the Eraser (alone on E); then `steps` sub tools on from
    // the one the tool has picked (each tool keeps its own).
    let sub = |runtime: &mut Runtime, tool: &str, steps: usize| {
        command(runtime, "tool.eraser");
        command(runtime, tool);
        for _ in 0..steps {
            command(runtime, "tool.next_sub");
        }
    };
    // Lasso fill, the Fill tool's fourth sub tool as in Clip Studio Paint
    // Ver. 4: the area drawn round fills.
    assert_eq!(painted(&runtime), 0);
    sub(&mut runtime, "tool.fill", 3);
    drag(
        &mut runtime,
        &mut step,
        &ring(middle - vec2(80.0, 0.0), 30.0),
        plain,
    );
    let lasso = painted(&runtime);
    assert!(lasso > 500, "lasso fill painted {lasso} pixels");
    // Close and fill: nothing closed off on a new layer, then a ring
    // drawn with the pen fills inside.
    command(&mut runtime, "layer.new");
    // Close and fill, the third: from the fourth, round past the fifth.
    sub(&mut runtime, "tool.fill", 4);
    let around = ring(middle + vec2(80.0, 0.0), 50.0);
    drag(&mut runtime, &mut step, &around, plain);
    assert_eq!(painted(&runtime), lasso, "no lines, nothing closed off");
    command(&mut runtime, "tool.pen");
    // Round a little past where it began, as a hand closes a ring: the
    // pen's ends taper to points that would only touch.
    let pen_ring = ring(middle + vec2(80.0, 0.0), 25.0);
    let closed: Vec<Pos2> = pen_ring.iter().chain(&pen_ring[1..4]).copied().collect();
    drag(&mut runtime, &mut step, &closed, plain);
    let lines = painted(&runtime);
    sub(&mut runtime, "tool.fill", 0);
    drag(&mut runtime, &mut step, &around, plain);
    let filled = painted(&runtime);
    assert!(
        filled > lines + 500,
        "close and fill: {lines} before, {filled} after"
    );
    // Copy stamp: nothing until Alt and click sets a source, then the
    // lasso fill's paint copies onto an empty place.
    command(&mut runtime, "layer.select_below");
    // Copy stamp, the Blend tool's third (Liquify is a tool of its own).
    sub(&mut runtime, "tool.blend", 2);
    let empty = middle + vec2(-80.0, 120.0);
    let line = [empty, empty + vec2(10.0, 0.0), empty + vec2(20.0, 0.0)];
    drag(&mut runtime, &mut step, &line, plain);
    assert_eq!(painted(&runtime), filled, "no source yet");
    drag(
        &mut runtime,
        &mut step,
        &[middle - vec2(80.0, 0.0)],
        egui::Modifiers::ALT,
    );
    drag(&mut runtime, &mut step, &line, plain);
    assert!(
        painted(&runtime) > filled + 100,
        "copy stamp painted {}",
        painted(&runtime) - filled
    );
    // Polyline and Continuous Curve: a click at each corner, a
    // double-click on the last ends the line.
    // (Direct draw's sixth and eighth.)
    for (index, steps) in [(5, 5), (7, 2)] {
        let before = painted(&runtime);
        sub(&mut runtime, "tool.figure", steps);
        let base = middle + vec2(-150.0, -150.0 + index as f32 * 20.0);
        for corner in [base, base + vec2(60.0, 30.0), base + vec2(120.0, 0.0)] {
            drag(&mut runtime, &mut step, &[corner], plain);
        }
        drag(&mut runtime, &mut step, &[base + vec2(120.0, 0.0)], plain);
        assert!(
            painted(&runtime) > before + 100,
            "figure sub tool {index} drew {}",
            painted(&runtime) - before
        );
    }
    // Paint unpainted area: a ring of paint on a new layer, then the
    // gap inside fills, and only that.
    command(&mut runtime, "layer.new");
    command(&mut runtime, "tool.pen");
    let gap = middle + vec2(-80.0, 160.0);
    // Copy stamp's notice (no source yet) sits on the canvas's view now,
    // where the ring goes: it is put away first, as a person would.
    runtime.shell.clear_notices();
    drag(&mut runtime, &mut step, &ring(gap, 20.0), plain);
    let before = painted(&runtime);
    // (The Fill tool still has Close and fill, two before.)
    sub(&mut runtime, "tool.fill", 2);
    drag(&mut runtime, &mut step, &ring(gap, 30.0), plain);
    assert!(
        painted(&runtime) > before + 300,
        "paint unpainted area filled {}",
        painted(&runtime) - before
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn sue_paint_selections_shrink_become_layers_and_quick_masks() {
    let (mut runtime, root) = runtime("paint-selection");
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
    let command = |runtime: &mut Runtime, command: &str| {
        runtime.request(
            ShellRequest::AppletCommand {
                applet: paint.clone(),
                command: command.to_owned(),
            },
            &mut NoTextures,
            start,
        );
    };
    let named = |runtime: &Runtime, name: &str| {
        sue_paint::doc::layers(&runtime.engine.world.arena, picture)
            .into_iter()
            .find(|info| info.name == name)
    };
    // Select All, Shrink Selected Area by 4, Convert to Selection Layer:
    // the layer holds the selection, 4 pixels in from every edge.
    command(&mut runtime, "select.all");
    command(&mut runtime, "select.shrink");
    step(&mut runtime, Vec::new());
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    command(&mut runtime, "select.to_layer");
    let layer = named(&runtime, "Selection").expect("a selection layer");
    assert!(layer.style.draft, "left out of exports");
    let raster = sue_paint::doc::raster(&runtime.engine.world.arena, layer.id).unwrap();
    let selection = sue_paint::select::Selection::from_alpha(raster);
    let (w, h) = (raster.width(), raster.height());
    assert_eq!(selection.at(4, 4), 1.0);
    assert_eq!(selection.at(w - 5, h - 5), 1.0);
    assert_eq!(selection.at(2, 2), 0.0, "drawn in from the edge");
    assert_eq!(selection.at(w - 3, h / 2), 0.0);
    // Quick Mask: a red layer to paint the selection on, and back.
    command(&mut runtime, "select.quick_mask");
    assert!(named(&runtime, "Quick Mask").is_some());
    command(&mut runtime, "select.quick_mask");
    assert!(named(&runtime, "Quick Mask").is_none(), "gone again");

    // Convert to Lines and Tones: a lines layer over a tone layer.
    command(&mut runtime, "layer.select_below");
    command(&mut runtime, "filter.lines_tones");
    // Its dialog opens first; OK (Enter) converts.
    step(&mut runtime, Vec::new());
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    let tones = named(&runtime, "Tones").expect("a tone layer");
    assert!(named(&runtime, "Lines").is_some(), "and a lines layer");
    assert!(
        sue_paint::effects::Effects::of(
            runtime.engine.world.arena.get_by_id(tones.id).unwrap(),
            72.0
        )
        .tone
        .is_some()
    );

    // Export as PDF and as EPUB.
    std::fs::create_dir_all(&root).unwrap();
    for (purpose, file) in [("book-pdf", "Book.pdf"), ("book-epub", "Book.epub")] {
        runtime.request(
            ShellRequest::Picked {
                pick: FilePick::save(Some(paint.clone()), purpose, "Export", file),
                path: root.join(file),
            },
            &mut NoTextures,
            start,
        );
    }
    assert!(
        std::fs::read(root.join("Book.pdf"))
            .unwrap()
            .starts_with(b"%PDF")
    );
    let epub = std::fs::read(root.join("Book.epub")).unwrap();
    let files = sue_io::zip::read_zip(&epub).unwrap();
    assert_eq!(files[0].0, "mimetype");
    assert!(
        files
            .iter()
            .any(|(name, _)| name == "OEBPS/page-0001.xhtml")
    );

    // Export Webtoon: OK keeps the cut and width (1280 and 800), then
    // the picture goes out scaled to 800 wide in numbered pieces.
    command(&mut runtime, "file.export.webtoon");
    step(&mut runtime, Vec::new());
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    runtime.request(
        ShellRequest::Picked {
            pick: FilePick::save(
                Some(paint.clone()),
                "webtoon",
                "Export Webtoon",
                "Strip.png",
            ),
            path: root.join("Strip.png"),
        },
        &mut NoTextures,
        start,
    );
    // The pieces are made in the background.
    until_exports_end(&mut runtime, |runtime| step(runtime, Vec::new()));
    let piece = std::fs::read(root.join("Strip-001.png")).expect("the first piece");
    let tile = sue_io::decode_png(&piece).unwrap().tile;
    assert_eq!(tile.width(), 800);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_liquify_pushes_a_stroke_in_one_undo_step() {
    let (mut runtime, root) = runtime("paint-liquify");
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
    let ink = |runtime: &Runtime, rows: std::ops::Range<u32>| {
        let raster = sue_paint::doc::raster(&runtime.engine.world.arena, layer).unwrap();
        rows.map(|y| {
            u32::from(raster.bytes().unwrap()[((y * raster.width() + 800) * 4 + 3) as usize])
        })
        .sum::<u32>()
    };
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let middle = paint_middle(&runtime);
    let mut drag = |runtime: &mut Runtime, before: Vec<egui::Event>, path: &[Pos2]| {
        step(runtime, before);
        step(
            runtime,
            vec![egui::Event::PointerMoved(path[0]), press(path[0], true)],
        );
        step(
            runtime,
            path[1..]
                .iter()
                .map(|at| egui::Event::PointerMoved(*at))
                .collect(),
        );
        step(runtime, vec![press(*path.last().unwrap(), false)]);
    };
    drag(
        &mut runtime,
        Vec::new(),
        &[middle - vec2(60.0, 0.0), middle, middle + vec2(60.0, 0.0)],
    );
    assert!(ink(&runtime, 590..610) > 0);
    assert_eq!(ink(&runtime, 640..700), 0);
    let depth = runtime.engine.history.depth();
    // J for Blend, J again for Liquify, a tool of its own (its one sub
    // tool, in Push mode). Push down.
    let path: Vec<Pos2> = (0..12)
        .map(|index| middle + vec2(0.0, -10.0 + index as f32 * 4.0))
        .collect();
    drag(
        &mut runtime,
        vec![key(egui::Key::J, plain), key(egui::Key::J, plain)],
        &path,
    );
    assert_eq!(runtime.engine.history.depth(), depth + 1, "one undo step");
    assert!(ink(&runtime, 610..700) > 0, "the stroke was pushed down");
    step(
        &mut runtime,
        vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
    );
    assert_eq!(ink(&runtime, 640..700), 0, "undo puts it back");
    let _ = std::fs::remove_dir_all(&root);
}

/// The whole arena as the file writes it: entities with their ids,
/// components and properties, and every relation.
fn arena_bytes(runtime: &Runtime) -> Vec<u8> {
    let mut out = sue_core::codec::Writer::new();
    runtime.engine.world.arena.encode(&mut out);
    out.into_bytes()
}

#[test]
fn sue_paint_flips_and_rotate_180_take_masks_and_vector_lines_along() {
    // A stroke on a raster layer with a mask that hides its left part,
    // and a stroke on a vector layer above it. Flip Horizontal, Flip
    // Vertical and Rotate 180 Degrees, run as the menu runs them, show the
    // picture mirrored (the mask still covers the same part of the
    // stroke) and mirror the vector line's points; undo and redo are
    // exact.
    use sue_core::ReversibleTransaction;
    let mut driver = PaintDriver::new("paint-flip-masks");
    let paint = AppletId::new(sue_paint::ID).unwrap();
    let picture = driver
        .runtime
        .applets
        .get(&paint)
        .and_then(|applet| applet.focus())
        .expect("New made a picture");
    let middle = paint_middle(&driver.runtime);
    driver.drag(
        middle - vec2(220.0, 150.0),
        middle - vec2(60.0, 120.0),
        egui::PointerButton::Primary,
    );
    let raster_layer = sue_paint::doc::layers(&driver.runtime.engine.world.arena, picture)
        .last()
        .unwrap()
        .id;
    let (mut mask_edit, _) =
        sue_paint::doc::new_mask(&driver.runtime.engine.world.arena, raster_layer, |x, _| {
            if x < 500 { 0.0 } else { 1.0 }
        })
        .unwrap();
    mask_edit
        .apply(&mut driver.runtime.engine.world.arena)
        .unwrap();
    driver.command("layer.new.vector");
    driver.settle(4);
    let vector = sue_paint::doc::layers(&driver.runtime.engine.world.arena, picture)
        .last()
        .unwrap()
        .id;
    assert!(sue_paint::vector::is_vector(
        &driver.runtime.engine.world.arena,
        vector
    ));
    driver.drag(
        middle + vec2(40.0, 60.0),
        middle + vec2(160.0, 120.0),
        egui::PointerButton::Primary,
    );
    let points = |runtime: &Runtime| -> Vec<[f32; 2]> {
        sue_paint::vector::lines(&sue_paint::vector::geometry(
            &runtime.engine.world.arena,
            vector,
        ))
        .iter()
        .flat_map(|line| line.samples.iter().map(|sample| sample.at))
        .collect()
    };
    assert!(
        !points(&driver.runtime).is_empty(),
        "the vector line is kept"
    );
    let shown = |runtime: &Runtime| -> Vec<[u8; 4]> {
        sue_paint::composite::render(
            &runtime.engine.world.arena,
            picture,
            [0, 0, 1600, 1200],
            sue_paint::composite::RenderOptions::default(),
        )
        .as_chunks::<4>()
        .0
        .to_vec()
    };
    let before = shown(&driver.runtime);
    let ink = before.iter().filter(|pixel| pixel[0] < 128).count();
    assert!(ink > 100, "something is drawn and shown");
    let line_before = points(&driver.runtime);
    let [lw, lh] = {
        let pixels = sue_paint::doc::raster(&driver.runtime.engine.world.arena, vector).unwrap();
        [pixels.width() as f32, pixels.height() as f32]
    };
    for (command, flip_h, flip_v) in [
        ("canvas.flip_h", true, false),
        ("canvas.flip_v", false, true),
        ("canvas.rotate180", true, true),
    ] {
        let arena_before = arena_bytes(&driver.runtime);
        let depth = driver.runtime.engine.history.depth();
        driver.command(command);
        driver.settle(4);
        assert_eq!(
            driver.runtime.engine.history.depth(),
            depth + 1,
            "{command}"
        );
        let after = shown(&driver.runtime);
        let mut wrong = 0;
        for y in 0..1200_usize {
            for x in 0..1600_usize {
                let ox = if flip_h { 1599 - x } else { x };
                let oy = if flip_v { 1199 - y } else { y };
                if after[y * 1600 + x] != before[oy * 1600 + ox] {
                    wrong += 1;
                }
            }
        }
        assert_eq!(wrong, 0, "{command}: the picture shows mirrored");
        let want: Vec<[f32; 2]> = line_before
            .iter()
            .map(|[x, y]| {
                [
                    if flip_h { lw - x } else { *x },
                    if flip_v { lh - y } else { *y },
                ]
            })
            .collect();
        assert_eq!(points(&driver.runtime), want, "{command}: the line");
        let arena_after = arena_bytes(&driver.runtime);
        driver.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
        driver.settle(2);
        assert_eq!(
            arena_bytes(&driver.runtime),
            arena_before,
            "{command}: undo"
        );
        driver.step(vec![key(
            egui::Key::Z,
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        )]);
        driver.settle(2);
        assert_eq!(arena_bytes(&driver.runtime), arena_after, "{command}: redo");
        driver.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
        driver.settle(2);
    }
}

#[test]
fn sue_paint_pick_screen_color_sets_the_drawing_color() {
    // CT-29, FG-30: Edit, Pick Screen Color, run as the menu runs it,
    // waits for a click anywhere on the screen; the color there becomes
    // the drawing color, and Fill paints in it.
    let mut driver = PaintDriver::new("paint-screen-color");
    let state = |name: &str| {
        sue_paint::registry()
            .iter()
            .find(|command| command.name == name)
            .map(|command| command.label)
    };
    assert!(state("edit.screen_color").is_some());
    assert!(state("edit.screen_color_hidden").is_some());
    // Sue shows a large square of a known color in a window of its own
    // and the click lands inside it, so the test reads what Sue drew,
    // never the desktop around it, and no real pointer takes part. The
    // color is neither the drawing color a fresh studio has (black) nor
    // its sub color (white), so a pick that went wrong shows.
    let want = [37_u8, 142, 211];
    let patch = sue_platform::screen::ScreenPatch::show(want, 240).expect("Sue's color patch");
    let at = patch
        .spot(Duration::from_secs(5))
        .expect("a point of the patch shows its color");
    driver.command("edit.screen_color");
    let mut clicked = false;
    for _ in 0..400 {
        driver.step(Vec::new());
        if sue_platform::screen::click_latest(at[0], at[1]) {
            clicked = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(clicked, "the picker came up and took the click");
    // The answer reaches the studio within a few frames.
    let picture = driver
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.focus())
        .unwrap();
    let layer = sue_paint::doc::layers(&driver.runtime.engine.world.arena, picture)
        .last()
        .unwrap()
        .id;
    // The pick reads the patch and ends; the next frames take its color.
    let start = std::time::Instant::now();
    while sue_platform::screen::latest_waiting() && start.elapsed() < Duration::from_secs(10) {
        driver.step(Vec::new());
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!sue_platform::screen::latest_waiting(), "the pick ended");
    driver.settle(4);
    drop(patch);
    // Fill puts the drawing color on the layer as it is, to the last
    // level.
    driver.command("edit.fill");
    driver.settle(4);
    let raster = sue_paint::doc::raster(&driver.runtime.engine.world.arena, layer).unwrap();
    let bytes = raster.bytes().unwrap();
    let middle = ((raster.height() / 2 * raster.width() + raster.width() / 2) * 4) as usize;
    assert_eq!(
        &bytes[middle..middle + 3],
        &want,
        "the layer is filled with the picked color"
    );
}
