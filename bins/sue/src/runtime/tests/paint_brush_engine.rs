//! Every brush on the brush engine, through the whole runtime: each
//! built-in brush sub tool paints through the engine's worker and ends
//! no wider than its body; a vector layer keeps new lines on the engine;
//! a stroke snaps to the grid; post correction smooths a stroke; and a
//! sheet of every built-in brush for a person to look at.

use super::*;
use egui::{Pos2, vec2};
use sue_platform::stylus::{self, PenKind, PenSample, PenSource};

/// A frame's move of the pen and how hard it presses.
type Leg = (egui::Vec2, f32);

/// How a stroke reaches Sue Paint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    /// A tablet's packets (WinTab), four to a frame, with the window's
    /// pointer events.
    Pen,
    /// A mouse: the window's pointer events and the mouse's own samples,
    /// one a frame.
    Mouse,
}

/// A packet of `source` at `pos` and device time `time`.
fn packet(
    source: PenSource,
    kind: PenKind,
    pos: Pos2,
    time: f64,
    pressure: Option<f32>,
) -> PenSample {
    let mut sample = PenSample::at(source, [pos.x, pos.y], time);
    sample.kind = kind;
    sample.pressure = pressure;
    sample
}

fn button(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

/// Draws `legs` from `start` by `route`, `rest` frames still before the
/// lift, the device's clock at `time`; the lift comes with the last
/// move's frame. Answers where the stroke ended.
fn draw(
    driver: &mut PaintDriver,
    start: Pos2,
    legs: &[Leg],
    route: Route,
    rest: usize,
    time: &mut f64,
) -> Pos2 {
    let source = match route {
        Route::Pen => PenSource::WinTab,
        Route::Mouse => PenSource::Mouse,
    };
    let pressing = |p: f32| (route == Route::Pen).then_some(p);
    let first = legs.first().map_or(0.5, |leg| leg.1);
    *time += 0.005;
    stylus::push_for_test(packet(source, PenKind::Down, start, *time, pressing(first)));
    driver.step(vec![egui::Event::PointerMoved(start), button(start, true)]);
    let mut at = start;
    let count = legs.len();
    for (index, (by, pressure)) in legs.iter().enumerate() {
        let parts = if route == Route::Pen { 4 } else { 1 };
        for part in 1..=parts {
            let p = at + *by * (part as f32 / parts as f32);
            *time += 0.016 / parts as f64;
            stylus::push_for_test(packet(source, PenKind::Move, p, *time, pressing(*pressure)));
        }
        at += *by;
        let mut events = vec![egui::Event::PointerMoved(at)];
        if index + 1 == count && rest == 0 {
            *time += 0.002;
            stylus::push_for_test(packet(source, PenKind::Up, at, *time, None));
            events.push(button(at, false));
        }
        driver.step(events);
    }
    if rest > 0 || count == 0 {
        for _ in 0..rest {
            *time += 0.016;
            driver.step(Vec::new());
        }
        *time += 0.002;
        stylus::push_for_test(packet(source, PenKind::Up, at, *time, None));
        driver.step(vec![button(at, false)]);
    }
    driver.settle(3);
    stylus::forget_for_test();
    at
}

/// The picture Sue Paint shows and its top layer: its id, width and
/// bytes.
fn top_layer(driver: &PaintDriver) -> (sue_core::EntityId, usize, Vec<u8>) {
    let arena = &driver.runtime.engine.world.arena;
    let picture = arena
        .iter()
        .find(|(_, entity)| sue_model::picture::is_picture(entity))
        .map(|(_, entity)| entity.id())
        .expect("a picture");
    let layer = sue_paint::doc::layers(arena, picture)
        .into_iter()
        .last()
        .expect("a layer");
    let raster = sue_paint::doc::raster(arena, layer.id).expect("pixels");
    (
        layer.id,
        raster.width() as usize,
        raster.bytes().expect("bytes").to_vec(),
    )
}

/// How much paint each pixel of `bytes` has, as `erased` counts it: its
/// alpha, or for an eraser on a filled layer what it took away.
fn amounts(bytes: &[u8], erased: bool) -> Vec<u8> {
    bytes
        .chunks(4)
        .map(|pixel| if erased { 255 - pixel[3] } else { pixel[3] })
        .collect()
}

/// A horizontal stroke's width profile: for each painted column, left
/// to right, how many of its pixels hold at least half the most paint
/// the stroke laid anywhere.
fn profile(amount: &[u8], width: usize) -> Vec<(usize, usize)> {
    let most = amount.iter().copied().max().unwrap_or(0);
    if most == 0 {
        return Vec::new();
    }
    let half = most.div_ceil(2);
    let height = amount.len() / width;
    (0..width)
        .map(|x| {
            let thick = (0..height)
                .filter(|y| amount[y * width + x] >= half)
                .count();
            (x, thick)
        })
        .filter(|(_, thick)| *thick > 0)
        .collect()
}

/// The command that picks sub tool `name` of `tool`.
fn pick(tool: sue_paint::tools::Tool, name: &str) -> String {
    format!("subtool.{}.{}", tool.id(), name.replace(' ', "_"))
}

/// The tools whose sub tools are brushes.
const BRUSH_TOOLS: [sue_paint::tools::Tool; 6] = [
    sue_paint::tools::Tool::Pen,
    sue_paint::tools::Tool::Pencil,
    sue_paint::tools::Tool::Brush,
    sue_paint::tools::Tool::Airbrush,
    sue_paint::tools::Tool::Decoration,
    sue_paint::tools::Tool::Eraser,
];

/// A body pressed at `pressure`, rising over its first frames and easing
/// off over its last, `count` frames of `step` points to the right.
fn swell(count: usize, step: f32, pressure: f32) -> Vec<Leg> {
    (0..count)
        .map(|i| {
            let s = i as f32 / (count - 1).max(1) as f32;
            let ramp = (s * 6.0).min(1.0).min((1.0 - s) * 6.0 + 0.15);
            (vec2(step, 0.0), (pressure * ramp).max(0.05))
        })
        .collect()
}

/// `true` when `preset` lays its tips by chance: scattered, sprayed, or
/// sized at random.
fn scatters(preset: &sue_model::BrushPreset) -> bool {
    use sue_model::brush::{input, param};
    preset.value(param::SCATTER).is_some_and(|v| v > 0.0)
        || preset
            .dynamics
            .iter()
            .any(|b| b.on && (b.input == input::RANDOM || b.input == input::STROKE_RANDOM))
        || preset
            .extra("paint.spray")
            .is_some_and(|on| on.first() == Some(&1))
}

/// Every built-in brush sub tool paints two strokes through the brush
/// engine's worker (a swell, and one pressed evenly and lifted while
/// moving fast): the engine took each (`sue_paint::last_stroke`) and laid
/// it with the painter its brush needs (coverage for a plain round tip,
/// the paint behaviours for the rest), it changed the layer, neither of
/// its ends is wider than its body (the owner's first rule for strokes),
/// undo takes it back exactly and redo lays it again.
#[test]
fn every_built_in_brush_paints_through_the_engine_and_ends_no_wider_than_its_body() {
    let mut driver = PaintDriver::new("paint-every-brush");
    driver.command("view.actual");
    driver.settle(3);
    let toolbox = sue_paint::tools::Toolbox::default();
    let mut time = 100.0;
    let mut checked = 0;
    for tool in BRUSH_TOOLS {
        let eraser = tool == sue_paint::tools::Tool::Eraser;
        if eraser {
            // Something to erase.
            driver.command("select.all");
            driver.command("edit.fill");
            driver.command("select.none");
            driver.settle(2);
        }
        for sub in toolbox.subs(tool) {
            driver.command(&pick(tool, &sub.name));
            driver.settle(2);
            let preset = sue_paint::brush::engine::builtin(tool.id(), &sub.name)
                .unwrap_or_else(|| panic!("{}: a built-in preset", sub.name));
            // A brush that scatters its tips by chance (stars, spray) has
            // no edge to its line: a tip that falls at an end may be one
            // of the larger ones, never a blob over what the body holds.
            let scattered = scatters(&preset);
            // The brush's size in layer pixels (the view is at 100 %).
            let tip = preset.value(sue_model::brush::param::SIZE).unwrap_or(0.0);
            let plain = sue_paint::brush::engine::is_plain(&preset);
            // A swell (the pen pressing harder and easing off), and the
            // stroke that made the blob of old: pressed evenly and lifted
            // while still moving fast.
            let strokes = [
                ("swell", swell(40, 8.0, 0.7)),
                ("lifted moving", vec![(vec2(10.0, 0.0), 0.7); 32]),
            ];
            for (kind, legs) in strokes {
                let (layer, width, before) = top_layer(&driver);
                let start = paint_middle(&driver.runtime) - vec2(160.0, 40.0);
                draw(&mut driver, start, &legs, Route::Pen, 0, &mut time);
                let seen = sue_paint::last_stroke().expect("a stroke began");
                assert_eq!(seen.sub, sub.name, "the engine took {}", sub.name);
                assert!(!seen.vector, "{}: a raster layer", sub.name);
                assert_eq!(
                    seen.behaviours, !plain,
                    "{}: laid by the painter its brush needs",
                    sub.name
                );
                let (same, _, after) = top_layer(&driver);
                assert_eq!(same, layer);
                assert_ne!(after, before, "{} painted", sub.name);
                // Only what the stroke changed counts.
                let changed: Vec<u8> = before
                    .chunks(4)
                    .zip(after.chunks(4))
                    .map(|(a, b)| {
                        let (a, b) = (amounts(a, eraser)[0], amounts(b, eraser)[0]);
                        b.saturating_sub(a)
                    })
                    .collect();
                let line = profile(&changed, width);
                assert!(line.len() > 30, "{}: the stroke shows: {line:?}", sub.name);
                let n = line.len();
                let widest = |part: &[(usize, usize)]| {
                    part.iter().map(|(_, thick)| *thick).max().unwrap_or(0)
                };
                let first = widest(&line[..n * 15 / 100]);
                let body = widest(&line[n * 15 / 100..n * 85 / 100]);
                let end = widest(&line[n * 85 / 100..]);
                println!(
                    "{} {} ({kind}): start {first} px, body {body} px, end {end} px, behaviours {}",
                    tool.id(),
                    sub.name,
                    seen.behaviours
                );
                // A scattered brush's tips land by chance: its body need
                // not hold its largest tip, and two tips may fall in one
                // column. Its end may hold a tip of the brush's own size, or
                // a column a sixth more than the body's, never a blob.
                let allowed = if scattered {
                    (body + body / 6 + 1).max(tip.ceil() as usize + 1)
                } else {
                    body + 1
                };
                assert!(
                    end <= allowed,
                    "{} ({kind}): the end ({end} px) is no wider than the body ({body} px)",
                    sub.name
                );
                assert!(
                    first <= allowed,
                    "{} ({kind}): the start ({first} px) is no wider than the body ({body} px)",
                    sub.name
                );
                if kind == "swell" {
                    // The undo pair.
                    driver.command("edit.undo");
                    driver.settle(2);
                    assert_eq!(top_layer(&driver).2, before, "{}: undo", sub.name);
                    driver.command("edit.redo");
                    driver.settle(2);
                    assert_eq!(top_layer(&driver).2, after, "{}: redo", sub.name);
                }
                driver.command("edit.undo");
                driver.settle(2);
                assert_eq!(top_layer(&driver).2, before, "{}: undone", sub.name);
            }
            checked += 1;
        }
    }
    assert!(checked >= 30, "every built-in brush: {checked}");
}

/// A new line on a vector layer is a line of the brush engine, for a pen
/// and for a brush that dabs: the layer keeps it, and painted again from
/// what it keeps it gives the very pixels drawn. Undo takes line and
/// pixels back together.
#[test]
fn a_vector_layer_keeps_new_lines_on_the_engine() {
    let mut driver = PaintDriver::new("paint-vector-engine");
    driver.command("view.actual");
    driver.command("layer.new.vector");
    driver.settle(3);
    let mut time = 50.0;
    for (index, command) in ["subtool.pen.G-pen", "subtool.airbrush.Soft"]
        .into_iter()
        .enumerate()
    {
        driver.command(command);
        driver.settle(2);
        let (layer, width, before) = top_layer(&driver);
        let start = paint_middle(&driver.runtime) - vec2(150.0, 60.0 - 70.0 * index as f32);
        draw(
            &mut driver,
            start,
            &swell(30, 9.0, 0.6),
            Route::Pen,
            0,
            &mut time,
        );
        let seen = sue_paint::last_stroke().expect("a stroke");
        assert!(seen.vector, "{command}: kept as a line");
        let arena = &driver.runtime.engine.world.arena;
        assert!(sue_paint::vector::is_vector(arena, layer));
        let lines = sue_paint::vector::lines(&sue_paint::vector::geometry(arena, layer));
        assert_eq!(lines.len(), index + 1, "{command}: one more line");
        let line = lines.last().unwrap();
        assert!(line.engine.is_some(), "{command}: a line of the engine");
        let (_, _, after) = top_layer(&driver);
        let height = after.len() / 4 / width;
        let again = sue_paint::vector::render(&lines, width as u32, height as u32)
            .expect("the lines paint");
        assert_eq!(
            again.bytes().unwrap(),
            after.as_slice(),
            "{command}: painted again from what it keeps, the very pixels drawn"
        );
        driver.command("edit.undo");
        driver.settle(2);
        let arena = &driver.runtime.engine.world.arena;
        assert_eq!(
            sue_paint::vector::lines(&sue_paint::vector::geometry(arena, layer)).len(),
            index,
            "{command}: undo takes the line"
        );
        assert_eq!(top_layer(&driver).2, before, "{command}: and its pixels");
        driver.command("edit.redo");
        driver.settle(2);
        assert_eq!(top_layer(&driver).2, after, "{command}: redo");
    }
}

/// The rows of ink in `amount` (a layer `width` wide), each column's
/// middle: where a horizontal line lies.
fn middles(amount: &[u8], width: usize) -> Vec<f32> {
    let height = amount.len() / width;
    (0..width)
        .filter_map(|x| {
            let (mut sum, mut weight) = (0.0_f32, 0.0_f32);
            for y in 0..height {
                let a = f32::from(amount[y * width + x]);
                sum += a * (y as f32 + 0.5);
                weight += a;
            }
            (weight > 255.0).then(|| sum / weight)
        })
        .collect()
}

/// View, Snap to Grid: a stroke that begins near a line of the grid
/// follows it, however the hand wobbles, and ends on it.
#[test]
fn a_stroke_near_the_grid_follows_its_line() {
    let mut driver = PaintDriver::new("paint-snap-grid");
    driver.command("view.actual");
    driver.command("subtool.pen.G-pen");
    driver.settle(3);
    let mut time = 10.0;
    // Where a tap lands, to find the grid's lines on screen (every 64
    // pixels from the top left, at 100 %).
    let probe = paint_middle(&driver.runtime) - vec2(120.0, 30.0);
    draw(
        &mut driver,
        probe,
        &[(vec2(0.0, 0.0), 0.6)],
        Route::Pen,
        2,
        &mut time,
    );
    let (_, width, bytes) = top_layer(&driver);
    let landed = middles(&amounts(&bytes, false), width);
    let y = landed.iter().sum::<f32>() / landed.len().max(1) as f32;
    driver.command("edit.undo");
    driver.settle(2);
    let line = (y / 64.0).round() * 64.0;
    // Begin 5 points off the line, and wobble 4 points either way.
    let start = probe + vec2(0.0, line - y + 5.0);
    let wobbly: Vec<Leg> = (0..36)
        .map(|i| (vec2(8.0, if i % 2 == 0 { 4.0 } else { -4.0 }), 0.6))
        .collect();
    driver.command("view.snap_grid");
    driver.settle(2);
    draw(&mut driver, start, &wobbly, Route::Pen, 0, &mut time);
    let (_, width, bytes) = top_layer(&driver);
    let snapped = middles(&amounts(&bytes, false), width);
    assert!(snapped.len() > 200, "the stroke was drawn");
    for middle in &snapped {
        assert!(
            (middle - line).abs() <= 0.75,
            "on the grid's line at {line}: {middle}"
        );
    }
    driver.command("edit.undo");
    driver.command("view.snap_grid");
    driver.settle(2);
    draw(&mut driver, start, &wobbly, Route::Pen, 0, &mut time);
    let (_, width, bytes) = top_layer(&driver);
    let free = middles(&amounts(&bytes, false), width);
    let off = free
        .iter()
        .map(|middle| (middle - line).abs())
        .fold(0.0, f32::max);
    assert!(off > 2.0, "without the grid it is drawn where the pen went");
}

/// One frame with `events`, read through egui's accessibility tree:
/// every widget with a name, and where it is.
fn widgets(driver: &mut PaintDriver) -> Vec<(String, egui::Rect)> {
    driver.ctx.enable_accesskit();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        time: Some(driver.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    driver.now += std::time::Duration::from_millis(16);
    let runtime = &mut driver.runtime;
    let now = driver.now;
    let output = driver
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    let mut found = Vec::new();
    if let Some(update) = &output.platform_output.accesskit_update {
        for (_, node) in &update.nodes {
            if let (Some(label), Some(bounds)) = (node.label(), node.bounds()) {
                found.push((
                    label.to_owned(),
                    egui::Rect::from_min_max(
                        egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                        egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                    ),
                ));
            }
        }
    }
    output.drop_without_applying_deltas();
    found
}

fn click(driver: &mut PaintDriver, at: Pos2) {
    driver.step(vec![egui::Event::PointerMoved(at)]);
    driver.step(vec![button(at, true)]);
    driver.step(vec![button(at, false)]);
    driver.settle(4);
}

/// How far a horizontal line's middles stray from their own mean, at
/// most, leaving out its ends.
fn wobble(middles: &[f32]) -> f32 {
    let n = middles.len();
    let inner = &middles[n / 8..n * 7 / 8];
    let mean = inner.iter().sum::<f32>() / inner.len().max(1) as f32;
    inner
        .iter()
        .map(|middle| (middle - mean).abs())
        .fold(0.0, f32::max)
}

/// Post correction, set in Sub Tool Detail: a stroke the hand wobbled is
/// drawn again smoothed when the pen lifts, as one undo step.
#[test]
fn post_correction_smooths_a_wobbly_stroke() {
    let mut driver = PaintDriver::new("paint-post-correction");
    driver.command("view.actual");
    driver.command("subtool.pen.G-pen");
    driver.settle(3);
    let mut time = 10.0;
    let start = paint_middle(&driver.runtime) - vec2(160.0, 20.0);
    // A slow wobble the stabilizer lets through: up and down 3 points
    // every five frames.
    let wobbly: Vec<Leg> = (0..60)
        .map(|i| {
            let phase = (i / 5) % 2;
            (vec2(5.0, if phase == 0 { 1.2 } else { -1.2 }), 0.6)
        })
        .collect();
    draw(&mut driver, start, &wobbly, Route::Pen, 0, &mut time);
    let (_, width, bytes) = top_layer(&driver);
    let raw = wobble(&middles(&amounts(&bytes, false), width));
    driver.command("edit.undo");
    driver.settle(2);
    // Sub Tool Detail, Correction: post correction to its most.
    driver.command("sub_tool.detail");
    driver.settle(6);
    let found = widgets(&mut driver);
    let detail = found
        .iter()
        .filter(|(label, _)| label == "Sub Tool Detail")
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("Sub Tool Detail's window");
    let category = found
        .iter()
        .filter(|(label, rect)| label == "Correction" && detail.contains(rect.center()))
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("the Correction category");
    click(&mut driver, category.center());
    let slider = widgets(&mut driver)
        .into_iter()
        .filter(|(label, rect)| {
            label == "Correction" && detail.contains(rect.center()) && rect.width() > 100.0
        })
        .map(|(_, rect)| rect)
        .next()
        .expect("the Correction slider");
    driver.drag(
        egui::pos2(slider.left() + 6.0, slider.center().y),
        egui::pos2(slider.right() + 40.0, slider.center().y),
        egui::PointerButton::Primary,
    );
    let shown = driver
        .texts
        .iter()
        .filter(|(_, rect)| (rect.center().y - slider.center().y).abs() < slider.height())
        .filter(|(text, rect)| detail.contains(rect.center()) && text.parse::<u32>().is_ok())
        .map(|(text, _)| text.clone())
        .next();
    assert_eq!(shown.as_deref(), Some("10"), "post correction at its most");
    // Sub Tool Detail goes away again (it floats over the canvas).
    let hide = widgets(&mut driver)
        .into_iter()
        .find(|(label, _)| label == "Hide Sub Tool Detail Palette")
        .map(|(_, rect)| rect)
        .expect("the palette's hide button");
    click(&mut driver, hide.center());
    let (_, _, clear) = top_layer(&driver);
    draw(&mut driver, start, &wobbly, Route::Pen, 0, &mut time);
    let (_, width, bytes) = top_layer(&driver);
    let smooth = wobble(&middles(&amounts(&bytes, false), width));
    driver.command("edit.undo");
    driver.settle(2);
    assert_eq!(
        top_layer(&driver).2,
        clear,
        "the stroke, smoothed, is one undo step"
    );
    assert!(
        smooth < raw * 0.6,
        "post correction smooths the wobble: {smooth:.2} px from {raw:.2} px"
    );
}

/// The strokes of the sheet, by name: slow, fast, light, heavy, a tap, a
/// flick and a lift while moving.
fn sheet_rows() -> Vec<(&'static str, Vec<Leg>, usize)> {
    let ease = |t: f32| t * t * (3.0 - 2.0 * t);
    vec![
        (
            "slow",
            (0..40)
                .map(|i| {
                    let s = i as f32 / 39.0;
                    (
                        vec2(3.5, 0.6 * (s * 6.0).sin()),
                        0.25 + 0.5 * ease(s.min(1.0 - s) * 2.0),
                    )
                })
                .collect(),
            0,
        ),
        (
            "fast",
            (0..9)
                .map(|i| {
                    (
                        vec2(16.0, 2.0 - 0.5 * i as f32),
                        0.3 + 0.4 * ease(i as f32 / 8.0),
                    )
                })
                .collect(),
            0,
        ),
        (
            "light",
            (0..28).map(|_| (vec2(5.0, 0.0), 0.12)).collect(),
            0,
        ),
        (
            "heavy",
            (0..28).map(|_| (vec2(5.0, 0.0), 0.95)).collect(),
            0,
        ),
        ("tap", vec![(vec2(0.3, 0.0), 0.5)], 2),
        (
            "flick",
            (0..10)
                .map(|i| {
                    (
                        vec2(4.0 + i as f32 * 2.5, -0.6 * i as f32),
                        0.55 - 0.04 * i as f32,
                    )
                })
                .collect(),
            0,
        ),
        (
            "lift moving",
            (0..18).map(|_| (vec2(7.5, 0.0), 0.6)).collect(),
            0,
        ),
    ]
}

/// The ground a sheet's strokes are drawn on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ground {
    /// A clear layer over white paper.
    White,
    /// A layer filled with a mid-gray, so a brush that mixes picks the
    /// gray up and one that lays thin paint shows how thin.
    Gray,
}

impl Ground {
    const fn name(self) -> &'static str {
        match self {
            Self::White => "white",
            Self::Gray => "gray",
        }
    }

    const fn rgb(self) -> [u8; 3] {
        match self {
            Self::White => [255, 255, 255],
            Self::Gray => [128, 128, 128],
        }
    }
}

/// Fills the top layer with `rgba`, straight into its pixels: the ground
/// of a sheet, not a step anyone undoes.
fn ground_layer(driver: &mut PaintDriver, rgba: [u8; 4]) {
    let (layer, _, _) = top_layer(driver);
    let arena = &mut driver.runtime.engine.world.arena;
    let raster = sue_paint::doc::raster_mut(arena, layer).expect("pixels");
    for pixel in raster.bytes_mut().expect("bytes").chunks_mut(4) {
        pixel.copy_from_slice(&rgba);
    }
    driver.settle(2);
}

/// One brush's strokes on one ground at one zoom, each cut out of the
/// layer and laid over the ground's color: its width, height and pixels.
type Cut = (usize, usize, Vec<[u8; 3]>);

/// Draws every stroke of [`sheet_rows`] (a pen's packets) and a fast
/// mouse stroke with the picked sub tool, each at the middle of the
/// canvas, cut out and undone; a cut is magnified `zoom` times, as the
/// view shows it.
fn brush_cuts(
    driver: &mut PaintDriver,
    name: &str,
    ground: Ground,
    zoom: usize,
    time: &mut f64,
) -> Vec<Cut> {
    let middle = paint_middle(&driver.runtime);
    let mut strokes: Vec<(Vec<Leg>, usize, Route)> = sheet_rows()
        .into_iter()
        .map(|(_, legs, rest)| (legs, rest, Route::Pen))
        .collect();
    strokes.push((
        (0..6)
            .map(|i| (vec2(22.0, 3.0 - 1.5 * i as f32), 1.0))
            .collect(),
        0,
        Route::Mouse,
    ));
    let back = ground.rgb();
    let mut cuts = Vec::new();
    for (legs, rest, route) in strokes {
        let (_, width, before) = top_layer(driver);
        let start = middle - vec2(80.0, 0.0);
        draw(driver, start, &legs, route, rest, time);
        let (_, _, after) = top_layer(driver);
        let height = after.len() / 4 / width;
        // Where the stroke changed the layer, and its margin.
        let (mut x0, mut y0, mut x1, mut y1) = (width, height, 0, 0);
        for y in 0..height {
            for x in 0..width {
                let i = (y * width + x) * 4;
                if after[i..i + 4] != before[i..i + 4] {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        let margin = 10;
        let cut = if x1 >= x0 && y1 >= y0 {
            let (x0, y0) = (x0.saturating_sub(margin), y0.saturating_sub(margin));
            let (x1, y1) = ((x1 + margin).min(width - 1), (y1 + margin).min(height - 1));
            let (w, h) = ((x1 - x0 + 1) * zoom, (y1 - y0 + 1) * zoom);
            let pixels = (0..h)
                .flat_map(|y| (0..w).map(move |x| (x0 + x / zoom, y0 + y / zoom)))
                .map(|(x, y)| {
                    let i = (y * width + x) * 4;
                    let a = u32::from(after[i + 3]);
                    std::array::from_fn(|c| {
                        ((u32::from(after[i + c]) * a + u32::from(back[c]) * (255 - a)) / 255) as u8
                    })
                })
                .collect();
            (w, h, pixels)
        } else {
            (1, 1, vec![back])
        };
        cuts.push(cut);
        if after != before {
            driver.command("edit.undo");
            driver.settle(1);
        }
        assert_eq!(top_layer(driver).2, before, "{name}: undone");
    }
    cuts
}

/// `cuts` in rows of `columns`, each in a cell as large as the largest,
/// over `back`, appended to `sheet` with a rule under them.
fn lay_cuts(sheet: &mut Vec<Vec<[u8; 3]>>, cuts: &[Cut], columns: usize, back: [u8; 3]) {
    let cell_w = cuts.iter().map(|c| c.0).max().unwrap_or(1) + 8;
    let cell_h = cuts.iter().map(|c| c.1).max().unwrap_or(1) + 8;
    for row in cuts.chunks(columns) {
        for y in 0..cell_h {
            let mut line = Vec::with_capacity(cell_w * columns);
            for (cw, ch, pixels) in row {
                for x in 0..cell_w {
                    let (dx, dy) = ((cell_w - cw) / 2, (cell_h - ch) / 2);
                    let inside = x >= dx && x < dx + cw && y >= dy && y < dy + ch;
                    line.push(if inside {
                        pixels[(y - dy) * cw + (x - dx)]
                    } else {
                        back
                    });
                }
            }
            sheet.push(line);
        }
    }
    // A rule between brushes, red so it reads on either ground.
    sheet.push(vec![[200, 60, 60]; cell_w * columns]);
}

/// `rows` as a PNG at `path`, short rows filled with `back`.
fn write_sheet(path: &std::path::Path, rows: &[Vec<[u8; 3]>], back: [u8; 3]) {
    let width = rows.iter().map(Vec::len).max().unwrap_or(1);
    let mut out = Vec::with_capacity(width * rows.len() * 4);
    for row in rows {
        for x in 0..width {
            let pixel = row.get(x).copied().unwrap_or(back);
            out.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        }
    }
    let png = sue_io::encode_png_rgba(width as u32, rows.len() as u32, &out).unwrap();
    std::fs::write(path, png).unwrap();
}

/// Writes sheets of every built-in brush drawn through the canvas, for
/// a person to look at: for each brush sub tool its strokes slow, fast,
/// light, heavy, a tap, a flick, a lift while moving (a pen's packets)
/// and a fast mouse stroke, drawn at 100 % and at 200 % (shown magnified
/// as the view shows them), on a clear layer over white and on a layer
/// of mid-gray. `SUE_BRUSH_SHEET` names the folder: one PNG per tool,
/// zoom and ground (`pen-z1-white.png`), one per brush in `brushes/`,
/// and `rows.txt` naming the brushes in order. `SUE_BRUSH_SHEET_ONLY`
/// keeps to the brushes whose names hold it.
#[test]
#[ignore = "writes sheets of every built-in brush for a person to look at"]
fn every_brush_sheet() {
    let mut driver = PaintDriver::new("paint-brush-sheet");
    let folder = std::env::var("SUE_BRUSH_SHEET").map_or_else(
        |_| std::env::temp_dir().join("sue-brush-sheet"),
        PathBuf::from,
    );
    let only = std::env::var("SUE_BRUSH_SHEET_ONLY").ok();
    std::fs::create_dir_all(folder.join("brushes")).unwrap();
    let toolbox = sue_paint::tools::Toolbox::default();
    let mut time = 1000.0;
    let mut names = Vec::new();
    for (zoom, view) in [(1, "view.actual"), (2, "view.double")] {
        driver.command(view);
        driver.settle(3);
        for ground in [Ground::White, Ground::Gray] {
            let back = ground.rgb();
            for tool in BRUSH_TOOLS {
                let eraser = tool == sue_paint::tools::Tool::Eraser;
                let mut sheet: Vec<Vec<[u8; 3]>> = Vec::new();
                for (index, sub) in toolbox.subs(tool).iter().enumerate() {
                    if only.as_deref().is_some_and(|only| !sub.name.contains(only)) {
                        continue;
                    }
                    // An eraser takes black away; every other brush paints
                    // on the ground's own layer.
                    match (eraser, ground) {
                        (true, _) => ground_layer(&mut driver, [0, 0, 0, 255]),
                        (false, Ground::Gray) => ground_layer(&mut driver, [128, 128, 128, 255]),
                        (false, Ground::White) => {}
                    }
                    driver.command(&pick(tool, &sub.name));
                    driver.settle(2);
                    let size = sub.tip.size;
                    let cuts = brush_cuts(&mut driver, &sub.name, ground, zoom, &mut time);
                    let columns = if size > 40.0 || zoom > 1 { 2 } else { 4 };
                    let mut own = Vec::new();
                    lay_cuts(&mut own, &cuts, columns, back);
                    write_sheet(
                        &folder.join("brushes").join(format!(
                            "{}-{:02}-{}-z{zoom}-{}.png",
                            tool.id(),
                            index,
                            sub.name.replace(' ', "_"),
                            ground.name()
                        )),
                        &own,
                        back,
                    );
                    sheet.extend(own);
                    if zoom == 1 && ground == Ground::White {
                        names.push(format!("{} {}", tool.id(), sub.name));
                    }
                    ground_layer(&mut driver, [0, 0, 0, 0]);
                }
                if !sheet.is_empty() {
                    write_sheet(
                        &folder.join(format!("{}-z{zoom}-{}.png", tool.id(), ground.name())),
                        &sheet,
                        back,
                    );
                }
            }
        }
    }
    std::fs::write(folder.join("rows.txt"), names.join("\n")).unwrap();
    println!("brush sheet: {}", folder.display());
}
