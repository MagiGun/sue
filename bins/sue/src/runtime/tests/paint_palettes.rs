//! Sue Paint: the strip, palettes, docks and workspaces.

use super::*;

/// The icons of the strip beside the canvas open the palette they
/// stand for. The grip that drags an icon out of the strip senses
/// drags only, and must be registered under the icon: a drag-only
/// widget above a click-sensing one makes egui drop the click
/// (`hit_test_on_close`), which left the whole strip dead to clicks.
#[test]
fn sue_paint_strip_icons_open_their_palettes() {
    let (mut runtime, root) = runtime("paint-strip");
    let ctx = egui::Context::default();
    let size = vec2(1440.0, 900.0);
    let start = Instant::now();
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        start,
    );
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    // A picture so the studio shows its palettes, not the start page.
    runtime.with_context(&mut NoTextures, start, |cx| {
        sue_paint::sample_picture(cx, 320, 240)
    });
    frames(&mut runtime, &ctx, size, start, |_| Vec::new());
    let page = runtime.shell.page_rect();
    let mut now = start;
    let mut step = |runtime: &mut Runtime, events: Vec<egui::Event>| -> Vec<(String, Pos2)> {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
            events,
            time: Some(ctx.input(|input| input.time) + 0.016),
            ..egui::RawInput::default()
        };
        now += Duration::from_millis(16);
        let output = ctx.run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
        let texts = texts_of(&output);
        output.drop_without_applying_deltas();
        texts
    };
    // Where a strip icon is: the strip runs down the right edge, its
    // icons start under the command bar (38 high), inset 4, 26 across
    // and 32 apart.
    let icon = |index: usize| {
        Pos2::new(
            page.right() - sue_wm::dock::STRIP + 4.0 + 13.0,
            page.top() + 38.0 + sue_wm::dock::COLUMN_BAR + 6.0 + 13.0 + index as f32 * 32.0,
        )
    };
    // A palette is open when its own title is drawn beside the strip;
    // the icon's tooltip sits in the strip itself.
    let open = |texts: &[(String, Pos2)], name: &str| {
        texts
            .iter()
            .any(|(text, at)| text == name && at.x < page.right() - sue_wm::dock::STRIP - 40.0)
    };
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut texts = step(&mut runtime, Vec::new());
    for _ in 0..4 {
        texts = step(&mut runtime, Vec::new());
    }
    // Material, then History, each opened by its icon and closed by it again.
    for (index, name) in [(0_usize, "Material"), (3, "History")] {
        assert!(
            !open(&texts, name),
            "{name} is open before its icon is clicked"
        );
        let at = icon(index);
        step(&mut runtime, vec![egui::Event::PointerMoved(at)]);
        step(&mut runtime, vec![press(at, true)]);
        step(&mut runtime, vec![press(at, false)]);
        for _ in 0..4 {
            texts = step(&mut runtime, Vec::new());
        }
        assert!(
            open(&texts, name),
            "clicking the {name} icon at {at:?} did not open its palette"
        );
        step(&mut runtime, vec![press(at, true)]);
        step(&mut runtime, vec![press(at, false)]);
        for _ in 0..4 {
            texts = step(&mut runtime, Vec::new());
        }
        assert!(
            !open(&texts, name),
            "clicking the {name} icon again did not close its palette"
        );
    }
    // The icon still drags its palette out: a press and a pull.
    let at = icon(7);
    step(&mut runtime, vec![egui::Event::PointerMoved(at)]);
    step(&mut runtime, vec![press(at, true)]);
    let to = Pos2::new(page.center().x, page.center().y);
    for index in 1..=20 {
        let at = at + (to - at) * (index as f32 / 20.0);
        step(&mut runtime, vec![egui::Event::PointerMoved(at)]);
    }
    step(&mut runtime, vec![press(to, false)]);
    for _ in 0..4 {
        texts = step(&mut runtime, Vec::new());
    }
    assert!(
        open(&texts, "Info"),
        "dragging the Info icon did not float its palette over the canvas"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_palettes_drag_by_their_title_bars() {
    let (mut runtime, root) = runtime("paint-dock-drag");
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
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events,
            time: Some(ctx.input(|input| input.time) + 0.016),
            ..egui::RawInput::default()
        };
        now += Duration::from_millis(16);
        let output = ctx.run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
        let texts = texts_of(&output);
        output.drop_without_applying_deltas();
        texts
    };
    let plain = egui::Modifiers::NONE;
    step(
        &mut runtime,
        vec![key(egui::Key::N, egui::Modifiers::COMMAND)],
    );
    step(&mut runtime, vec![key(egui::Key::Enter, plain)]);
    let mut texts = Vec::new();
    for _ in 0..60 {
        texts = step(&mut runtime, Vec::new());
    }
    let layers_at = |texts: &[(String, Pos2)]| {
        texts
            .iter()
            .find(|(text, _)| text == "Layer 1")
            .map(|(_, at)| *at)
    };
    let before = layers_at(&texts).expect("the Layer palette lists Layer 1");
    assert!(before.x > 1000.0, "the Layer palette starts at the right");
    // The Layer palette's title, dragged into the left column.
    let title = texts
        .iter()
        .find(|(text, at)| text == "Layer" && at.x > 1000.0 && at.y > 300.0)
        .map(|(_, at)| *at + vec2(12.0, 6.0))
        .expect("the Layer palette's title");
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: plain,
    };
    step(&mut runtime, vec![egui::Event::PointerMoved(title)]);
    step(&mut runtime, vec![press(title, true)]);
    let to = Pos2::new(150.0, 640.0);
    for index in 1..=20 {
        let at = title + (to - title) * (index as f32 / 20.0);
        step(&mut runtime, vec![egui::Event::PointerMoved(at)]);
    }
    step(&mut runtime, vec![press(to, false)]);
    for _ in 0..10 {
        texts = step(&mut runtime, Vec::new());
    }
    let after = layers_at(&texts).expect("Layer 1 is still listed");
    assert!(
        after.x < 400.0,
        "the Layer palette moved to the left column: {before:?} then {after:?}"
    );
    // Every other palette drags by its title too, to the other side.
    let mut stuck = Vec::new();
    for name in [
        "Tool settings",
        "Color Wheel",
        "Color Set",
        "Navigator",
        "Sub View",
    ] {
        let Some(from) = texts
            .iter()
            .find(|(text, at)| text == name && at.y > 40.0)
            .map(|(_, at)| *at + vec2(10.0, 6.0))
        else {
            stuck.push(format!("{name}: no title drawn"));
            continue;
        };
        let to = if from.x < 720.0 {
            Pos2::new(1300.0, 700.0)
        } else {
            Pos2::new(150.0, 700.0)
        };
        step(&mut runtime, vec![egui::Event::PointerMoved(from)]);
        step(&mut runtime, vec![press(from, true)]);
        for index in 1..=20 {
            let at = from + (to - from) * (index as f32 / 20.0);
            step(&mut runtime, vec![egui::Event::PointerMoved(at)]);
        }
        step(&mut runtime, vec![press(to, false)]);
        for _ in 0..10 {
            texts = step(&mut runtime, Vec::new());
        }
        let moved = texts
            .iter()
            .find(|(text, at)| text == name && at.y > 40.0)
            .map(|(_, at)| *at);
        if moved.is_none_or(|at| (at.x < 720.0) == (from.x < 720.0)) {
            stuck.push(format!("{name}: {from:?} then {moved:?}"));
        }
    }
    assert!(
        stuck.is_empty(),
        "palettes that did not move:\n{}",
        stuck.join("\n")
    );
    // A pen under Windows Ink arrives as touches with the pointer
    // events egui makes of them; it drags a palette the same way.
    let touch = |phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase,
        pos,
        force: Some(0.5),
    };
    let from = texts
        .iter()
        .find(|(text, at)| text == "Layer" && at.y > 40.0)
        .map(|(_, at)| *at + vec2(10.0, 6.0))
        .expect("the Layer palette's title");
    let to = if from.x < 720.0 {
        Pos2::new(1300.0, 700.0)
    } else {
        Pos2::new(150.0, 700.0)
    };
    step(
        &mut runtime,
        vec![
            touch(egui::TouchPhase::Start, from),
            egui::Event::PointerMoved(from),
            press(from, true),
        ],
    );
    for index in 1..=20 {
        let at = from + (to - from) * (index as f32 / 20.0);
        step(
            &mut runtime,
            vec![
                touch(egui::TouchPhase::Move, at),
                egui::Event::PointerMoved(at),
            ],
        );
    }
    step(
        &mut runtime,
        vec![
            touch(egui::TouchPhase::End, to),
            press(to, false),
            egui::Event::PointerGone,
        ],
    );
    for _ in 0..10 {
        texts = step(&mut runtime, Vec::new());
    }
    let moved = layers_at(&texts).expect("Layer 1 is still listed");
    assert!(
        (moved.x < 720.0) != (from.x < 720.0),
        "a pen drags the Layer palette too: {from:?} then {moved:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sue_paint_docks_at_the_top_keeps_a_default_workspace_and_resets_to_it() {
    let mut paint = PaintDriver::new("paint-dock-top");
    let texts = paint.settle(1);
    let top = paint.page_top();
    let whole = egui::Rect::from_min_max(Pos2::new(0.0, top + 1.0), Pos2::new(1440.0, 900.0));
    let title = |texts: &[(String, egui::Rect)]| {
        texts
            .iter()
            .filter(|(text, rect)| text == "Layer" && whole.contains(rect.center()))
            .map(|(_, rect)| *rect)
            .min_by(|a, b| a.top().total_cmp(&b.top()))
            .expect("the Layer palette's title")
    };
    let before = title(&texts);
    assert!(
        before.left() > 1000.0 && before.top() > top + 200.0,
        "{before:?}"
    );
    // The Layer palette's title dragged to the top edge of the canvas:
    // it docks in the top row, beside the command bar.
    let to = Pos2::new(720.0, top + 38.0 + 14.0);
    let texts = paint.drag(before.center(), to, egui::PointerButton::Primary);
    let docked = title(&texts);
    assert!(
        docked.top() < top + 38.0 + 30.0 && docked.top() > top,
        "the Layer palette went into the top row: {before:?} then {docked:?}"
    );

    // Use Current Workspace as Default, then move it away.
    paint.command("window.workspace.default");
    paint.settle(2);
    assert!(
        paint
            .runtime
            .shell
            .message()
            .is_none_or(|(_, error)| !error),
        "{:?}",
        paint.runtime.shell.message()
    );
    assert!(paint.root.join("data").exists() || paint.root.exists());
    let texts = paint.drag(
        docked.center(),
        Pos2::new(150.0, 640.0),
        egui::PointerButton::Primary,
    );
    let moved = title(&texts);
    assert!(
        moved.left() < 400.0 && moved.top() > top + 200.0,
        "{moved:?}"
    );

    // Reset to Default: back as the default says, in the top row.
    paint.command("window.workspace.reset");
    let texts = paint.settle(10);
    let reset = title(&texts);
    assert!(
        reset.top() < top + 38.0 + 30.0,
        "back to the default workspace: {moved:?} then {reset:?}"
    );
    // Reset again, already there: Sue Paint's own default layout.
    paint.command("window.workspace.reset");
    let texts = paint.settle(10);
    let factory = title(&texts);
    assert!(
        factory.left() > 1000.0 && factory.top() > top + 200.0,
        "back to Sue Paint's own layout: {factory:?}"
    );
}

#[test]
fn sue_paint_tears_a_tab_off_into_a_palette_of_its_own() {
    let mut paint = PaintDriver::new("paint-dock-tear");
    let texts = paint.settle(1);
    let left = egui::Rect::from_min_max(Pos2::new(0.0, 40.0), Pos2::new(400.0, 900.0));
    let right = egui::Rect::from_min_max(Pos2::new(720.0, 40.0), Pos2::new(1440.0, 900.0));
    // The color wheel palette's second tab, Color Slider: by its name
    // when the names fit, else the icon right after the chosen tab.
    let wheel = text_in(&texts, "Color Wheel", left).expect("the Color Wheel tab");
    let tab = text_in(&texts, "Color Slider", left).map_or_else(
        || Pos2::new(wheel.right() + 14.0 + 2.0 + 16.0, wheel.center().y),
        |rect| rect.center(),
    );
    assert!(text_in(&texts, "Color Slider", right).is_none());
    let texts = paint.drag(tab, Pos2::new(1300.0, 700.0), egui::PointerButton::Primary);
    assert!(
        text_in(&texts, "Color Slider", right).is_some(),
        "Color Slider is a palette of its own at the right"
    );
    assert!(
        text_in(&texts, "Color Slider", left).is_none(),
        "and no longer a tab at the left"
    );
    assert!(
        text_in(&texts, "Color Wheel", left).is_some(),
        "the color wheel stays where it was"
    );
}
