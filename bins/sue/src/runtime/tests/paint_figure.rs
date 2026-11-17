//! Sue Paint: the Figure tool on the canvas (P4-1c): the Curve's drag,
//! bend and click, Continuous Curve, Bezier Curve and Polygon, Shift's
//! squares, circles and 45 degree lines, figures along the perspective
//! ruler, effect line layers named after their sub tool, and figures on
//! vector layers.

use super::*;
use egui::{Modifiers, PointerButton};

/// The picture Sue Paint has open.
fn picture(paint: &PaintDriver) -> sue_core::EntityId {
    paint
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.focus())
        .expect("a picture is open")
}

/// Every inked pixel on the picture's raster layers (paper left out).
fn ink(paint: &PaintDriver) -> Vec<[u32; 2]> {
    let arena = &paint.runtime.engine.world.arena;
    let mut found = Vec::new();
    for info in sue_paint::doc::layers(arena, picture(paint)) {
        if info.kind != sue_model::picture::LayerKind::Raster {
            continue;
        }
        let Some(raster) = sue_paint::doc::raster(arena, info.id) else {
            continue;
        };
        let Some(bytes) = raster.bytes() else {
            continue;
        };
        let [dx, dy] = info.style.offset;
        let width = raster.width();
        for y in 0..raster.height() {
            for x in 0..width {
                if bytes[((y * width + x) * 4 + 3) as usize] > 0 {
                    found.push([(x as i64 + dx) as u32, (y as i64 + dy) as u32]);
                }
            }
        }
    }
    found
}

/// The box round `pixels`: left, top, right, bottom.
fn bounds(pixels: &[[u32; 2]]) -> [u32; 4] {
    let mut out = [u32::MAX, u32::MAX, 0, 0];
    for [x, y] in pixels {
        out = [
            out[0].min(*x),
            out[1].min(*y),
            out[2].max(*x),
            out[3].max(*y),
        ];
    }
    out
}

/// One frame with `events` and `held` keys down.
fn step_held(paint: &mut PaintDriver, mut events: Vec<egui::Event>, held: Modifiers) {
    events.insert(0, egui::Event::ModifiersChanged(held));
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        events,
        time: Some(paint.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    paint.now += Duration::from_millis(16);
    let runtime = &mut paint.runtime;
    let now = paint.now;
    paint
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now))
        .drop_without_applying_deltas();
}

/// One frame with `events` and `held` keys down: the lines it draws, as
/// screen points (paths and segments, polylines of two or more points).
fn step_lines(
    paint: &mut PaintDriver,
    mut events: Vec<egui::Event>,
    held: Modifiers,
) -> Vec<Vec<Pos2>> {
    events.insert(0, egui::Event::ModifiersChanged(held));
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
    let mut lines = Vec::new();
    fn look(shape: &egui::Shape, lines: &mut Vec<Vec<Pos2>>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| look(shape, lines)),
            egui::Shape::Path(path) => lines.push(path.points.clone()),
            egui::Shape::LineSegment { points, .. } => lines.push(points.to_vec()),
            _ => {}
        }
    }
    for clipped in &output.shapes {
        look(&clipped.shape, &mut lines);
    }
    output.drop_without_applying_deltas();
    lines
}

fn button(pos: Pos2, pressed: bool, held: Modifiers) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: held,
    }
}

/// A drag from `from` to `to` with `held` keys down.
fn drag_held(paint: &mut PaintDriver, from: Pos2, to: Pos2, held: Modifiers) {
    step_held(paint, vec![egui::Event::PointerMoved(from)], held);
    step_held(paint, vec![button(from, true, held)], held);
    for index in 1..=12 {
        let at = from + (to - from) * (index as f32 / 12.0);
        step_held(paint, vec![egui::Event::PointerMoved(at)], held);
    }
    step_held(paint, vec![button(to, false, held)], held);
    step_held(paint, Vec::new(), held);
}

/// A click at `at`.
fn click(paint: &mut PaintDriver, at: Pos2) {
    drag_held(paint, at, at, Modifiers::NONE);
}

/// The Figure tool's sub tools in the Sub Tool palette's order.
const FIGURE: [&str; 16] = [
    "Straight Line",
    "Rectangle",
    "Ellipse",
    "Filled Rectangle",
    "Filled Ellipse",
    "Polyline",
    "Curve",
    "Continuous Curve",
    "Bezier Curve",
    "Polygon",
    "Saturated Line",
    "Scattered saturated line",
    "Dense saturated line",
    "Dark saturated line",
    "Burst",
    "Speed Line",
];

/// The Figure tool with its sub tool `name` picked: the first clicked in
/// the Sub Tool palette (found by its name), then on to `name` as the
/// palette's Next Sub Tool goes (the lower rows are scrolled away).
fn figure(paint: &mut PaintDriver, name: &str) {
    // A tool's key goes round the tools that share it, so the pick
    // starts from the Eraser (alone on E).
    paint.command("tool.eraser");
    paint.command("tool.figure");
    paint.settle(4);
    // The first sub tool by its command: the Sub Tool list follows the
    // sub tool chosen, so its first row can be scrolled out of sight.
    paint.command(&format!("subtool.figure.{}", FIGURE[0].replace(' ', "_")));
    paint.settle(2);
    let steps = FIGURE.iter().position(|sub| *sub == name).unwrap();
    for _ in 0..steps {
        paint.command("tool.next_sub");
    }
    paint.settle(2);
}

#[test]
fn sue_paint_curve_is_dragged_then_bent_then_clicked() {
    // A25-8: press at the start, drag to the end, let go, move to bend,
    // click to set.
    let mut paint = PaintDriver::new("paint-figure-curve");
    figure(&mut paint, "Curve");
    let middle = paint_middle(&paint.runtime);
    let depth = paint.runtime.engine.history.depth();
    drag_held(
        &mut paint,
        middle + vec2(-120.0, 0.0),
        middle + vec2(120.0, 0.0),
        Modifiers::NONE,
    );
    assert!(
        ink(&paint).is_empty(),
        "nothing until the click: {:?}",
        bounds(&ink(&paint))
    );
    // The pointer bends the line; the click sets it through there.
    let bend = middle + vec2(0.0, 90.0);
    for index in 1..=6 {
        step_held(
            &mut paint,
            vec![egui::Event::PointerMoved(
                middle + vec2(0.0, 15.0 * index as f32),
            )],
            Modifiers::NONE,
        );
    }
    click(&mut paint, bend);
    let drawn = ink(&paint);
    let [left, top, right, bottom] = bounds(&drawn);
    assert!(bottom - top > 40, "bent: {left},{top} to {right},{bottom}");
    assert!(right - left > 2 * (bottom - top), "still across");
    assert_eq!(
        paint.runtime.engine.history.depth(),
        depth + 1,
        "one undo step"
    );
    // Esc drops a curve waiting for its bend.
    drag_held(
        &mut paint,
        middle + vec2(-120.0, -150.0),
        middle + vec2(120.0, -150.0),
        Modifiers::NONE,
    );
    paint.step(vec![key(egui::Key::Escape, Modifiers::NONE)]);
    paint.settle(2);
    click(&mut paint, middle + vec2(0.0, -220.0));
    assert_eq!(ink(&paint).len(), drawn.len(), "dropped");
}

#[test]
fn sue_paint_continuous_bezier_and_polygon_draw() {
    // A25V-14, FGV-35, FG-23: Direct draw has Continuous Curve, Bezier
    // Curve and Polygon; Lasso fill is not among them.
    let mut paint = PaintDriver::new("paint-figure-direct");
    paint.command("tool.eraser");
    paint.command("tool.figure");
    let texts = paint.settle(4);
    for name in ["Continuous Curve", "Bezier Curve", "Polygon", "Curve"] {
        assert!(
            texts.iter().any(|(text, _)| text == name),
            "{name} in the Sub Tool palette"
        );
    }
    assert!(!texts.iter().any(|(text, _)| text == "Lasso fill"));
    let middle = paint_middle(&paint.runtime);

    // Continuous Curve: a click at each point, the last again ends it,
    // smoothed through them (the middle point is reached).
    figure(&mut paint, "Continuous Curve");
    let base = middle + vec2(-200.0, -150.0);
    for corner in [vec2(0.0, 0.0), vec2(60.0, -50.0), vec2(120.0, 0.0)] {
        click(&mut paint, base + corner);
    }
    assert!(ink(&paint).is_empty(), "nothing until it ends");
    click(&mut paint, base + vec2(120.0, 0.0));
    let curve = ink(&paint);
    let [_, top, _, bottom] = bounds(&curve);
    assert!(bottom - top > 15, "through the raised point");

    // Bezier Curve: anchors with handles pulled out, a double-click on
    // the last ends it.
    figure(&mut paint, "Bezier Curve");
    let base = middle + vec2(-200.0, 50.0);
    drag_held(&mut paint, base, base + vec2(0.0, -60.0), Modifiers::NONE);
    drag_held(
        &mut paint,
        base + vec2(150.0, 0.0),
        base + vec2(150.0, 60.0),
        Modifiers::NONE,
    );
    assert_eq!(ink(&paint).len(), curve.len(), "nothing until it ends");
    click(&mut paint, base + vec2(150.0, 0.0));
    let bezier: Vec<[u32; 2]> = ink(&paint)
        .into_iter()
        .filter(|pixel| !curve.contains(pixel))
        .collect();
    assert!(bezier.len() > 100, "the bezier curve drew");
    let [_, top, _, bottom] = bounds(&bezier);
    assert!(bottom - top > 20, "the handles bend it: {top} to {bottom}");

    // Polygon: five corners in the box dragged, the first at the top
    // middle, so the top corners of the box stay empty.
    figure(&mut paint, "Polygon");
    let before = ink(&paint);
    drag_held(
        &mut paint,
        middle + vec2(80.0, -60.0),
        middle + vec2(240.0, 100.0),
        Modifiers::NONE,
    );
    let polygon: Vec<[u32; 2]> = ink(&paint)
        .into_iter()
        .filter(|pixel| !before.contains(pixel))
        .collect();
    let [left, top, right, _] = bounds(&polygon);
    let mid = (left + right) / 2;
    let row: Vec<u32> = polygon
        .iter()
        .filter(|[_, y]| *y <= top + 3)
        .map(|[x, _]| *x)
        .collect();
    assert!(!row.is_empty());
    assert!(
        row.iter().all(|x| x.abs_diff(mid) < (right - left) / 6),
        "only the top corner at the top: {row:?} around {mid}"
    );
}

#[test]
fn sue_paint_shift_keeps_figures_square_round_and_at_45_degrees() {
    // A25-25.
    let mut paint = PaintDriver::new("paint-figure-shift");
    let middle = paint_middle(&paint.runtime);
    let shift = Modifiers::SHIFT;
    let mut drawn = Vec::new();
    let mut fresh = |paint: &PaintDriver| {
        let now = ink(paint);
        let new: Vec<[u32; 2]> = now
            .iter()
            .copied()
            .filter(|pixel| !drawn.contains(pixel))
            .collect();
        drawn = now;
        bounds(&new)
    };
    for (name, at) in [
        ("Rectangle", vec2(-250.0, -180.0)),
        ("Ellipse", vec2(50.0, -180.0)),
    ] {
        figure(&mut paint, name);
        drag_held(
            &mut paint,
            middle + at,
            middle + at + vec2(180.0, 80.0),
            shift,
        );
        let [left, top, right, bottom] = fresh(&paint);
        let (w, h) = (right - left, bottom - top);
        assert!(w > 30 && w.abs_diff(h) <= 2, "{name} square: {w} by {h}");
    }
    // A straight line dragged at about 30 degrees lands on 45.
    figure(&mut paint, "Straight Line");
    drag_held(
        &mut paint,
        middle + vec2(-200.0, 60.0),
        middle + vec2(-20.0, 160.0),
        shift,
    );
    let [left, top, right, bottom] = fresh(&paint);
    let (w, h) = (right - left, bottom - top);
    assert!(w > 30 && w.abs_diff(h) <= 3, "45 degrees: {w} by {h}");
    // Without Shift the same drag keeps its slope.
    drag_held(
        &mut paint,
        middle + vec2(50.0, 60.0),
        middle + vec2(230.0, 160.0),
        Modifiers::NONE,
    );
    let [left, top, right, bottom] = fresh(&paint);
    assert!(right - left > (bottom - top) * 3 / 2, "free slope");
}

#[test]
fn sue_paint_figures_follow_the_perspective_ruler() {
    // CT-23: with a perspective ruler a rectangle lies in its plane (its
    // sides run upright, its top and bottom toward a vanishing point)
    // and a straight line follows the ruler it starts on.
    let mut paint = PaintDriver::new("paint-figure-perspective");
    paint.command("layer.ruler.perspective");
    paint.settle(2);
    paint.step(vec![key(egui::Key::Enter, Modifiers::NONE)]);
    paint.settle(4);
    assert!(
        sue_paint::rulers::Rulers::of(&paint.runtime.engine.world.arena, picture(&paint))
            .perspective
            .is_some()
    );
    let middle = paint.runtime.shell.page_rect().center();
    figure(&mut paint, "Rectangle");
    let (from, to) = (middle + vec2(-260.0, -160.0), middle + vec2(-100.0, -40.0));
    drag_held(&mut paint, from, to, Modifiers::NONE);
    // How tall the ink is at its left and right edges.
    let tall = |pixels: &[[u32; 2]]| {
        let [left, _, right, _] = bounds(pixels);
        let column = |x: u32| {
            let rows: Vec<u32> = pixels
                .iter()
                .filter(|p| p[0].abs_diff(x) <= 2)
                .map(|p| p[1])
                .collect();
            rows.iter().max().unwrap() - rows.iter().min().unwrap()
        };
        (column(left), column(right))
    };
    let (left, right) = tall(&ink(&paint));
    assert!(
        left.abs_diff(right) > 6,
        "in perspective the sides differ: {left} and {right}"
    );
    paint.step(vec![key(egui::Key::Z, Modifiers::COMMAND)]);
    paint.settle(2);
    assert!(ink(&paint).is_empty(), "undone");
    // With Snap to Special Ruler off it is a plain rectangle.
    paint.command("view.snap_special");
    drag_held(&mut paint, from, to, Modifiers::NONE);
    let (left, right) = tall(&ink(&paint));
    assert!(left.abs_diff(right) <= 2, "plain: {left} and {right}");
    paint.step(vec![key(egui::Key::Z, Modifiers::COMMAND)]);
    paint.command("view.snap_special");
    // A straight line heading down but drifting right runs upright.
    figure(&mut paint, "Straight Line");
    drag_held(
        &mut paint,
        middle + vec2(-20.0, -150.0),
        middle + vec2(10.0, 10.0),
        Modifiers::NONE,
    );
    let [left, top, right, bottom] = bounds(&ink(&paint));
    assert!(
        right - left < 12 && bottom - top > 60,
        "upright: {left},{top} to {right},{bottom}"
    );
}

#[test]
fn sue_paint_effect_lines_land_on_a_layer_named_after_the_sub_tool() {
    // CTV-15.
    let mut paint = PaintDriver::new("paint-figure-effect");
    figure(&mut paint, "Speed Line");
    let middle = paint_middle(&paint.runtime);
    drag_held(
        &mut paint,
        middle + vec2(-80.0, 0.0),
        middle + vec2(80.0, 20.0),
        Modifiers::NONE,
    );
    let arena = &paint.runtime.engine.world.arena;
    let names: Vec<String> = sue_paint::doc::layers(arena, picture(&paint))
        .into_iter()
        .map(|info| info.name)
        .collect();
    assert!(names.iter().any(|name| name == "Speed Line"), "{names:?}");
}

#[test]
fn sue_paint_figures_on_a_vector_layer_stay_vector_lines() {
    // CTL-3: an ellipse on a vector layer is a vector line drawn with the
    // sub tool's brush, one undo step.
    let mut paint = PaintDriver::new("paint-figure-vector");
    paint.command("layer.new.vector");
    paint.settle(4);
    let arena = &paint.runtime.engine.world.arena;
    let layer = sue_paint::doc::layers(arena, picture(&paint))
        .into_iter()
        .find(|info| sue_paint::vector::is_vector(arena, info.id))
        .expect("a vector layer")
        .id;
    figure(&mut paint, "Ellipse");
    let depth = paint.runtime.engine.history.depth();
    let middle = paint_middle(&paint.runtime);
    drag_held(
        &mut paint,
        middle + vec2(-80.0, -50.0),
        middle + vec2(80.0, 50.0),
        Modifiers::NONE,
    );
    let lines = |paint: &PaintDriver| {
        let arena = &paint.runtime.engine.world.arena;
        sue_paint::vector::lines(&sue_paint::vector::geometry(arena, layer))
    };
    assert_eq!(lines(&paint).len(), 1, "one vector line");
    assert!(!ink(&paint).is_empty(), "drawn");
    assert_eq!(paint.runtime.engine.history.depth(), depth + 1);
    paint.step(vec![key(egui::Key::Z, Modifiers::COMMAND)]);
    paint.settle(2);
    assert!(lines(&paint).is_empty(), "undone");
}

#[test]
fn sue_paint_bezier_preview_runs_to_the_anchor_being_dragged_out() {
    // A25V-14, FGV-35: while a Bezier Curve anchor is dragged out, the
    // preview runs to that anchor (where the line will go), not to the
    // pointer pulling its handle.
    let mut paint = PaintDriver::new("paint-figure-bezier-preview");
    figure(&mut paint, "Bezier Curve");
    let middle = paint_middle(&paint.runtime);
    let first = middle + vec2(-150.0, 0.0);
    drag_held(&mut paint, first, first + vec2(0.0, -60.0), Modifiers::NONE);
    let anchor = middle + vec2(100.0, 0.0);
    let pulled = anchor + vec2(60.0, 80.0);
    step_held(
        &mut paint,
        vec![egui::Event::PointerMoved(anchor)],
        Modifiers::NONE,
    );
    step_held(
        &mut paint,
        vec![button(anchor, true, Modifiers::NONE)],
        Modifiers::NONE,
    );
    let mut lines = Vec::new();
    for index in 1..=8 {
        let at = anchor + (pulled - anchor) * (index as f32 / 8.0);
        lines = step_lines(
            &mut paint,
            vec![egui::Event::PointerMoved(at)],
            Modifiers::NONE,
        );
    }
    // The curve so far: a long path from the first anchor.
    let curves: Vec<&Vec<Pos2>> = lines
        .iter()
        .filter(|line| line.len() >= 10 && line[0].distance(first) < 3.0)
        .collect();
    assert!(!curves.is_empty(), "the curve is previewed");
    for curve in &curves {
        let end = *curve.last().unwrap();
        assert!(
            end.distance(anchor) < 3.0,
            "it runs to the anchor {anchor:?}, not the pointer {pulled:?}: {end:?}"
        );
    }
    step_held(
        &mut paint,
        vec![button(pulled, false, Modifiers::NONE)],
        Modifiers::NONE,
    );
    // A double-click on it ends the line, and it lands where the
    // preview went.
    click(&mut paint, anchor);
    let [left, _, right, _] = bounds(&ink(&paint));
    assert!(right > left + 100, "drawn: {left} to {right}");
}

#[test]
fn sue_paint_ruler_preview_keeps_to_shift_45_degrees() {
    // A25-25 for the Ruler tool: what the drag shows with Shift is the
    // ruler laid, kept to 45 degrees.
    let mut paint = PaintDriver::new("paint-figure-ruler-shift");
    paint.command("tool.pick.ruler");
    paint.settle(4);
    paint.click_text("Linear Ruler");
    paint.settle(2);
    let middle = paint_middle(&paint.runtime);
    let (from, to) = (middle + vec2(-100.0, 0.0), middle + vec2(100.0, 60.0));
    let shift = Modifiers::SHIFT;
    step_held(&mut paint, vec![egui::Event::PointerMoved(from)], shift);
    step_held(&mut paint, vec![button(from, true, shift)], shift);
    let mut lines = Vec::new();
    for index in 1..=8 {
        let at = from + (to - from) * (index as f32 / 8.0);
        lines = step_lines(&mut paint, vec![egui::Event::PointerMoved(at)], shift);
    }
    let preview = lines
        .iter()
        .find(|line| line.len() == 2 && line[0].distance(from) < 1.0)
        .expect("the ruler is previewed");
    let along = preview[1] - preview[0];
    assert!(
        along.y.abs() < 1.0 && along.x > 150.0,
        "kept level by Shift: {along:?}"
    );
    step_held(&mut paint, vec![button(to, false, shift)], shift);
    step_held(&mut paint, Vec::new(), shift);
    let linear = sue_paint::rulers::Rulers::of(&paint.runtime.engine.world.arena, picture(&paint))
        .linear
        .expect("a linear ruler laid");
    let (a, b) = (linear[0], linear[1]);
    assert!((b[1] - a[1]).abs() < 1.0, "laid level as shown: {linear:?}");
}
