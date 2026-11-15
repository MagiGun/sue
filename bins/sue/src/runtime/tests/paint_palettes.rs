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
    // A picture so the studio has a canvas between its palettes.
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
        open(&texts, "Information"),
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
    // Every other palette drags by its tab too, to the other side: by
    // its name or, where the names do not fit, by its icon.
    let mut stuck = Vec::new();
    for (name, key) in [
        ("Tool Property", "tool_property"),
        ("Color Wheel", "color"),
        ("Color Set", "color_set"),
        ("Navigator", "navigator"),
        ("Color Slider", "color_slider"),
    ] {
        let Some(from) = sue_wm::dock::tab_place(&ctx, key).map(|place| place.center()) else {
            stuck.push(format!("{name}: no tab drawn"));
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
        let moved = sue_wm::dock::tab_place(&ctx, key).map(|place| place.center());
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
    // The color wheel palette's second tab, Color Slider: its name, or
    // its icon where the names do not fit.
    let tab = sue_wm::dock::tab_place(&paint.ctx, "color_slider")
        .expect("the Color Slider tab")
        .center();
    assert!(left.contains(tab), "{tab:?}");
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
    let wheel = sue_wm::dock::tab_place(&paint.ctx, "color").expect("the Color Wheel tab");
    assert!(
        left.contains(wheel.center()),
        "the color wheel stays where it was"
    );
}

/// The leftmost place `text` is drawn at or below `top`.
fn below(paint: &PaintDriver, text: &str, top: f32) -> Option<egui::Rect> {
    paint
        .texts
        .iter()
        .filter(|(found, rect)| found == text && rect.top() >= top)
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.left().total_cmp(&b.left()))
}

#[test]
fn sue_paint_starts_in_the_clip_studio_style() {
    let mut paint = PaintDriver::fresh("paint-style-default");
    let top = paint.page_top() + 40.0;
    // The pointer over the canvas, so no tooltip shows.
    paint.step(vec![egui::Event::PointerMoved(egui::pos2(720.0, 400.0))]);
    paint.settle(30);
    // Sub Tool, Tool Property and Brush Size stacked at the left, over the
    // six color palettes as tabs.
    let sub = below(&paint, "Sub Tool", top).expect("Sub Tool");
    let property = below(&paint, "Tool Property", top).expect("Tool Property");
    let size = below(&paint, "Brush Size", top).expect("Brush Size");
    assert!(sub.top() < property.top() && property.top() < size.top());
    for key in [
        "color",
        "color_slider",
        "color_set",
        "intermediate_color",
        "approximate_color",
        "color_history",
    ] {
        let tab = sue_wm::dock::tab_place(&paint.ctx, key).expect(key);
        assert!(
            tab.left() < 400.0 && tab.top() > size.bottom(),
            "{key} at the left: {tab:?}"
        );
    }
    // Navigator, Layer Property and Layer at the right; the Timeline
    // folded to an icon under the canvas (no palette shows its name).
    for name in ["Navigator", "Layer Property", "Layer"] {
        let at = below(&paint, name, top).expect(name);
        assert!(at.left() > 1000.0, "{name} at the right: {at:?}");
    }
    assert!(
        below(&paint, "Timeline", top).is_none(),
        "{:?}",
        paint.texts
    );
    // Picking a sub tool leaves the list where it is: Tool Property shows
    // beside it, no tab switches.
    paint.click_text("Grainy G-pen");
    assert!(paint.rect_of("Mapping pen").is_some(), "the list stays");
    assert!(paint.rect_of("Tool Property").is_some());
    // Window, Layout Style, directly above Workspace; the style not built
    // yet does nothing when asked for.
    paint.click_text("Window");
    let style = paint.rect_of("Layout Style").expect("Window, Layout Style");
    let workspace = paint.rect_of("Workspace").expect("Window, Workspace");
    assert!(style.bottom() <= workspace.top() + 1.0);
    paint.command("window.layout_style.sue_paint");
    paint.settle(4);
    assert_eq!(
        paint
            .runtime
            .engine
            .world
            .rice
            .get_str("sue_paint.layout_style"),
        Some("classic"),
        "the grayed style is never chosen"
    );
}

#[test]
fn sue_paint_hides_every_palette_and_shows_them_back_as_they_were() {
    let mut paint = PaintDriver::fresh("paint-hide-all");
    let top = paint.page_top() + 40.0;
    paint.settle(4);
    let before: Vec<String> = paint
        .texts
        .iter()
        .filter(|(_, rect)| rect.top() >= top)
        .map(|(text, _)| text.clone())
        .collect();
    assert!(before.iter().any(|text| text == "Layer"));
    paint.command("window.hide_all");
    paint.settle(6);
    for name in [
        "Layer",
        "Sub Tool",
        "Tool Property",
        "Navigator",
        "Color Wheel",
    ] {
        assert!(below(&paint, name, top).is_none(), "{name} hidden");
    }
    paint.click_text("Window");
    assert!(
        paint.rect_of("Show All Palettes").is_some(),
        "{:?}",
        paint.texts
    );
    paint.click_text("Show All Palettes");
    paint.settle(6);
    let after: Vec<String> = paint
        .texts
        .iter()
        .filter(|(_, rect)| rect.top() >= top)
        .map(|(text, _)| text.clone())
        .collect();
    for name in [
        "Layer",
        "Sub Tool",
        "Tool Property",
        "Navigator",
        "Color Wheel",
    ] {
        assert!(after.iter().any(|text| text == name), "{name} back");
    }
    paint.click_text("Window");
    assert!(paint.rect_of("Hide All Palettes").is_some());
}

#[test]
fn sue_paint_palette_menu_sits_top_left_and_hides_its_palette() {
    let mut paint = PaintDriver::fresh("paint-palette-menu");
    let top = paint.page_top() + 40.0;
    paint.settle(4);
    let title = below(&paint, "Layer Property", top).expect("Layer Property");
    // The menu button, left of the tab, at the header's left end.
    let tab = sue_wm::dock::tab_place(&paint.ctx, "layer_property").expect("its tab");
    let button = egui::pos2(tab.left() - 14.0, tab.center().y);
    let press = |pressed| egui::Event::PointerButton {
        pos: button,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    paint.step(vec![egui::Event::PointerMoved(button)]);
    paint.step(vec![press(true)]);
    paint.step(vec![press(false)]);
    paint.settle(4);
    let hide = paint
        .rect_of("Hide Layer Property Palette")
        .unwrap_or_else(|| panic!("the menu opened: {:?}", paint.texts));
    assert!(paint.rect_of("Float in a Window").is_some());
    assert!(hide.left() < title.left(), "it opens from the left");
    paint.click_text("Hide Layer Property Palette");
    paint.settle(4);
    assert!(below(&paint, "Layer Property", top).is_none(), "hidden");
    // The Window menu shows it again where it was.
    paint.click_text("Window");
    paint.click_text("Layer Property");
    paint.settle(4);
    let back = below(&paint, "Layer Property", top).expect("shown again");
    assert!(back.left() > 1000.0, "at the right again: {back:?}");
}

/// Every palette that can show in Sue Paint's dock, by key and name.
const PALETTES: [(&str, &str); 21] = [
    ("sub_tool", "Sub Tool"),
    ("tool_property", "Tool Property"),
    ("brush_grid", "Brush Size"),
    ("brush_size", "Tool Sliders"),
    ("color", "Color Wheel"),
    ("color_slider", "Color Slider"),
    ("color_set", "Color Set"),
    ("intermediate_color", "Intermediate Color"),
    ("approximate_color", "Approximate Color"),
    ("color_history", "Color History"),
    ("navigator", "Navigator"),
    ("sub_view", "Sub View"),
    ("item_bank", "Item Bank"),
    ("info", "Information"),
    ("layer_property", "Layer Property"),
    ("layer", "Layer"),
    ("history", "History"),
    ("auto_action", "Auto Action"),
    ("animation_cels", "Animation Cels"),
    ("material", "Material"),
    ("timeline", "Timeline"),
];

/// What is wrong with the palettes' names in the dock as `paint` drew
/// them last: a palette named twice (its tab and a title of its own), a
/// tab whose name is cut short or runs out of its tab.
fn name_faults(paint: &PaintDriver) -> Vec<String> {
    let top = paint.page_top() + 40.0;
    let mut faults = Vec::new();
    for (key, name) in PALETTES {
        let Some(tab) = sue_wm::dock::tab_place(&paint.ctx, key) else {
            continue;
        };
        // Named more than once around its header: the dock's tab and a
        // title of the palette's own.
        let drawn: Vec<egui::Rect> = paint
            .texts
            .iter()
            .filter(|(text, rect)| {
                text == name
                    && rect.top() >= top
                    && (rect.center().y - tab.center().y).abs() < tab.height() * 1.5
                    && (rect.center().x - tab.center().x).abs() < 400.0
            })
            .map(|(_, rect)| *rect)
            .collect();
        if drawn.len() > 1 {
            faults.push(format!("{name} is named {} times: {drawn:?}", drawn.len()));
        }
        // What the tab shows: its whole name inside it, or its icon alone;
        // never its name cut short.
        for (text, rect) in &paint.texts {
            if !tab.contains(rect.center()) {
                continue;
            }
            let cut = text.ends_with('\u{2026}')
                || (text != name && !text.is_empty() && name.starts_with(text.as_str()));
            if cut {
                faults.push(format!("the {name} tab shows {text:?}"));
            } else if text == name
                && (rect.left() < tab.left() - 0.5 || rect.right() > tab.right() + 0.5)
            {
                faults.push(format!("{name} runs out of its tab: {rect:?} in {tab:?}"));
            }
        }
    }
    faults
}

/// Phase 3 left every palette named twice (the dock's tab strip and the
/// palette's own header) and the color tabs cut mid-word in a narrow
/// column ("Color Wh"). Each palette is named once now, by its tab, and
/// a tab with no room for its name shows its icon, its name on hover.
#[test]
fn sue_paint_names_each_palette_once_and_never_cuts_a_name() {
    for fresh in [true, false] {
        let mut paint = if fresh {
            PaintDriver::fresh("paint-names-once")
        } else {
            PaintDriver::new("paint-names-once-kept")
        };
        paint.step(vec![egui::Event::PointerMoved(egui::pos2(720.0, 400.0))]);
        paint.settle(4);
        let faults = name_faults(&paint);
        assert!(faults.is_empty(), "fresh {fresh}:\n{}", faults.join("\n"));
        // Every palette of the left column's color group has a tab there.
        for key in ["color", "color_slider", "color_set"] {
            let tab = sue_wm::dock::tab_place(&paint.ctx, key)
                .unwrap_or_else(|| panic!("fresh {fresh}: no {key} tab"));
            assert!(tab.right() < 400.0, "fresh {fresh}: {key} at {tab:?}");
        }
    }
    // A tab showing its icon alone names its palette on hover.
    let mut paint = PaintDriver::fresh("paint-names-hover");
    paint.settle(2);
    let (tab, name) = PALETTES
        .iter()
        .find_map(|(key, name)| {
            let tab = sue_wm::dock::tab_place(&paint.ctx, key)?;
            let named = paint
                .texts
                .iter()
                .any(|(text, rect)| text == name && tab.contains(rect.center()));
            (!named).then_some((tab, *name))
        })
        .expect("a narrow column folds some tabs to their icons");
    paint.step(vec![egui::Event::PointerMoved(tab.center())]);
    paint.settle(60);
    assert!(
        paint
            .texts
            .iter()
            .any(|(text, rect)| text == name && !tab.contains(rect.center())),
        "hovering the {name} icon shows its name: {:?}",
        paint.texts
    );
}

/// A tab dragged along its header takes another place among the tabs;
/// the palette that showed still does.
#[test]
fn sue_paint_drags_a_tab_to_another_place_in_its_header() {
    let mut paint = PaintDriver::fresh("paint-tab-order");
    paint.settle(2);
    let place = |paint: &PaintDriver, key| {
        sue_wm::dock::tab_place(&paint.ctx, key).unwrap_or_else(|| panic!("the {key} tab"))
    };
    let wheel = place(&paint, "color");
    let set = place(&paint, "color_set");
    assert!(wheel.left() < set.left(), "Color Wheel comes first");
    // Along the header, to the left of the first tab.
    let to = egui::pos2(wheel.left() + 2.0, wheel.center().y);
    paint.drag(set.center(), to, egui::PointerButton::Primary);
    let wheel = place(&paint, "color");
    let set = place(&paint, "color_set");
    assert!(
        set.left() < wheel.left(),
        "Color Set comes first now: {set:?} {wheel:?}"
    );
    assert!(set.right() < 400.0, "in the same header");
    assert!(
        text_in(&paint.texts, "Color Wheel", wheel.expand(2.0)).is_some(),
        "the Color Wheel still shows"
    );
}

/// Where the button `index` of a floating palette's header is (roll up,
/// dock again, close), as the last frame drew it.
fn float_button(paint: &PaintDriver, key: &str, index: usize) -> Pos2 {
    paint
        .ctx
        .data(|data| {
            data.get_temp::<egui::Rect>(egui::Id::unique(("sue-paint-float-button", key, index)))
        })
        .expect("the header's buttons")
        .center()
}

/// A click at `at`, and the frames after.
fn click(paint: &mut PaintDriver, at: Pos2) {
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    paint.step(vec![egui::Event::PointerMoved(at)]);
    paint.step(vec![press(true)]);
    paint.step(vec![press(false)]);
    paint.settle(4);
}

/// A floating palette's header is its title bar: its name once, its
/// menu, and buttons to roll it up, dock it again and close it. Dragged
/// by its header over the canvas, its window moves.
#[test]
fn sue_paint_floating_palette_moves_rolls_up_and_docks_again() {
    let mut paint = PaintDriver::fresh("paint-float-header");
    paint.settle(2);
    let top = paint.page_top() + 40.0;
    // Color Set shown, then its menu (left of its header), then Float in
    // a Window.
    let tab = sue_wm::dock::tab_place(&paint.ctx, "color_set").expect("the Color Set tab");
    click(&mut paint, tab.center());
    let first = sue_wm::dock::tab_place(&paint.ctx, "color").expect("the Color Wheel tab");
    let tab = sue_wm::dock::tab_place(&paint.ctx, "color_set").expect("the Color Set tab");
    click(
        &mut paint,
        egui::pos2(first.left().min(tab.left()) - 14.0, tab.center().y),
    );
    paint.click_text("Float in a Window");
    paint.settle(4);
    // It floats where it was, its header buttons drawn.
    let title = below(&paint, "Color Set", top).expect("the floating Color Set");
    let _ = float_button(&paint, "color_set", 2);
    let faults = name_faults(&paint);
    assert!(faults.is_empty(), "{}", faults.join("\n"));
    // Dragged by its header over the canvas, the window moves with the
    // pointer.
    let by = egui::pos2(720.0, 400.0) - title.center();
    paint.drag(
        title.center(),
        title.center() + by,
        egui::PointerButton::Primary,
    );
    let moved = below(&paint, "Color Set", top).expect("still floating");
    assert!(
        (moved.center() - (title.center() + by)).length() < 6.0,
        "{title:?} moved by {by:?} to {moved:?}"
    );
    // Rolled up: only its header shows; unrolled: the palette again.
    assert!(paint.rect_of("Default").is_some(), "the palette shows");
    let at = float_button(&paint, "color_set", 0);
    click(&mut paint, at);
    assert!(
        paint.rect_of("Default").is_none(),
        "rolled up to its header"
    );
    let rolled = below(&paint, "Color Set", top).expect("its header stays");
    assert!(
        (rolled.center() - moved.center()).length() < 2.0,
        "{moved:?} then {rolled:?}"
    );
    let at = float_button(&paint, "color_set", 0);
    click(&mut paint, at);
    assert!(paint.rect_of("Default").is_some(), "unrolled");
    // Docked again, in the column on the side it is nearer.
    let at = float_button(&paint, "color_set", 1);
    click(&mut paint, at);
    let docked = sue_wm::dock::tab_place(&paint.ctx, "color_set").expect("docked");
    assert!(
        docked.right() < 400.0 || docked.left() > 1000.0,
        "docked in a column: {docked:?}"
    );
    // Floated again and closed by its X: hidden until the Window menu
    // shows it again.
    if let Some(applet) = paint
        .runtime
        .applets
        .get_mut(&AppletId::new(sue_paint::ID).unwrap())
    {
        applet.navigate("dock/float/color_set");
    }
    paint.settle(4);
    assert!(below(&paint, "Color Set", top).is_some(), "floating again");
    let at = float_button(&paint, "color_set", 2);
    click(&mut paint, at);
    assert!(below(&paint, "Color Set", top).is_none(), "closed");
    paint.click_text("Window");
    paint.click_text("Color Set");
    paint.settle(2);
    assert!(below(&paint, "Color Set", top).is_some(), "shown again");
}

/// Where the dock's chrome button `name` (such as `close-side-left`) was
/// drawn by the last frame.
fn chrome_button(paint: &PaintDriver, name: &str) -> egui::Rect {
    paint
        .ctx
        .data(|data| {
            data.get_temp::<(u64, egui::Rect)>(egui::Id::unique(("sue-paint-chrome-place", name)))
        })
        .unwrap_or_else(|| panic!("the {name} button"))
        .1
}

/// The buttons at the top of a side are the shared icon buttons: each
/// names itself on hover, and hiding the side and showing it again
/// brings its palettes back where they were.
#[test]
fn sue_paint_side_buttons_name_themselves_and_hide_and_show_a_side() {
    let mut paint = PaintDriver::fresh("paint-side-buttons");
    paint.settle(2);
    let top = paint.page_top() + 40.0;
    let before = below(&paint, "Sub Tool", top).expect("Sub Tool at the left");
    assert!(before.right() < 400.0, "{before:?}");
    let hide = chrome_button(&paint, "close-side-left");
    paint.step(vec![egui::Event::PointerMoved(hide.center())]);
    paint.settle(60);
    assert!(
        paint.rect_of("Hide palettes").is_some(),
        "its name on hover: {:?}",
        paint.texts
    );
    click(&mut paint, hide.center());
    assert!(
        below(&paint, "Sub Tool", top).is_none(),
        "the left side is hidden"
    );
    let open = chrome_button(&paint, "open-side-left");
    click(&mut paint, open.center());
    let after = below(&paint, "Sub Tool", top).expect("shown again");
    assert!(
        (after.center() - before.center()).length() < 2.0,
        "{before:?} then {after:?}"
    );
}
