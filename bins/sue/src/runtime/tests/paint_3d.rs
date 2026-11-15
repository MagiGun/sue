//! Sue Paint: the 3D Tool Property driven through real frames: the
//! object's and the camera's drag buttons, body types, the body shape
//! chart, the head to body ratio that follows the height, Manga
//! Perspective and the outline width. Each change is one undo step that
//! undoes to the arena exactly as it was.

use super::*;
use egui::pos2;

/// The whole arena as bytes: entities, ids, components, properties and
/// relations, so two states compare exactly.
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

/// One frame with `events`, as [`PaintDriver::step`], that also returns
/// every control the frame named for assistive technology, with where it
/// is: the drag buttons and the chart have no text of their own.
fn step_named(paint: &mut PaintDriver, events: Vec<egui::Event>) -> Vec<(String, egui::Rect)> {
    step_told(paint, events)
        .into_iter()
        .map(|(label, _, rect)| (label, rect))
        .collect()
}

/// [`step_named`] with each control's role.
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

/// The part of the screen Tool Property's rows show in: under its tab,
/// above the palettes below it.
fn band(paint: &PaintDriver) -> (f32, f32, f32) {
    let tab = paint
        .rect_of("Tool Property")
        .expect("the Tool Property tab");
    let below = paint
        .texts
        .iter()
        .filter(|(text, rect)| text == "Color Wheel" && rect.top() > tab.bottom())
        .map(|(_, rect)| rect.top())
        .fold(f32::INFINITY, f32::min);
    // Under the tab, the sub tool's name and its stroke preview come
    // before the rows.
    (tab.center().x, tab.bottom() + 80.0, below - 24.0)
}

/// Where the control named `name` is, scrolled into Tool Property's view
/// with the wheel when it is above or below it.
fn control(paint: &mut PaintDriver, name: &str) -> egui::Rect {
    for _ in 0..80 {
        let told = step_told(paint, Vec::new());
        // A slider and its number field share a name: the track is the
        // slider.
        let mut same: Vec<_> = told.iter().filter(|(label, ..)| label == name).collect();
        // The one nearest Tool Property's tab when others share the name
        // (the canvas's own bars name a Zoom too).
        let tab = paint
            .rect_of("Tool Property")
            .expect("the Tool Property tab");
        same.sort_by(|(_, _, a), (_, _, b)| {
            let far = |rect: &egui::Rect| (rect.center().x - tab.center().x).abs();
            far(a).total_cmp(&far(b))
        });
        same.sort_by_key(|(_, role, _)| *role != egui::accesskit::Role::Slider);
        let found = same.first().map(|(_, _, rect)| *rect).unwrap_or_else(|| {
            panic!(
                "{name:?} is named: {:?}",
                told.iter().map(|(label, ..)| label).collect::<Vec<_>>()
            )
        });
        let (x, top, bottom) = band(paint);
        let y = found.center().y;
        if y >= top && y <= bottom {
            return found;
        }
        let over = pos2(x, (top + bottom) / 2.0);
        let by = if y > bottom { -40.0 } else { 40.0 };
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
        paint.settle(2);
    }
    panic!("{name:?} never scrolled into view");
}

/// Scrolls Tool Property from its top to its end; every text it showed
/// on the way.
fn every_row(paint: &mut PaintDriver) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let wheel = |paint: &mut PaintDriver, by: f32| {
        let (x, top, bottom) = band(paint);
        paint.step(vec![
            egui::Event::PointerMoved(pos2(x, (top + bottom) / 2.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, by),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            },
        ]);
        paint.settle(2);
    };
    for _ in 0..30 {
        wheel(paint, 400.0);
    }
    for _ in 0..60 {
        let (_, _, bottom) = band(paint);
        let top = paint
            .rect_of("Tool Property")
            .map_or(0.0, |tab| tab.bottom());
        seen.extend(
            paint
                .texts
                .iter()
                .filter(|(_, rect)| rect.center().y >= top && rect.center().y <= bottom)
                .map(|(text, _)| text.clone()),
        );
        wheel(paint, -60.0);
    }
    seen.sort();
    seen.dedup();
    seen
}

/// Where `text` is, scrolled into Tool Property's view.
fn text_shown(paint: &mut PaintDriver, text: &str) -> egui::Rect {
    for _ in 0..80 {
        let (x, top, bottom) = band(paint);
        if let Some((_, rect)) = paint.texts.iter().find(|(found, rect)| {
            found == text && rect.center().y >= top && rect.center().y <= bottom
        }) {
            return *rect;
        }
        paint.step(vec![
            egui::Event::PointerMoved(pos2(x, (top + bottom) / 2.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, -40.0),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
            },
        ]);
        paint.settle(2);
    }
    panic!("{text:?} never scrolled into view");
}

/// Shows the Tool Property palette (it shares its place with Sub Tool).
fn show_tool_property(paint: &mut PaintDriver) {
    paint.settle(2);
    paint.click_text("Tool Property");
}

/// Drags from the middle of the control named `name` by `by`, in small
/// moves, and lets go; then lets the frames after draw.
fn drag_control(paint: &mut PaintDriver, name: &str, by: egui::Vec2) {
    let from = control(paint, name).center();
    paint.drag(from, from + by, egui::PointerButton::Primary);
}

/// Clicks the middle of the control named `name`.
fn click_control(paint: &mut PaintDriver, name: &str) {
    let at = control(paint, name).center();
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

/// Adds a 3D layer with `new` (a Layer, New Layer command) and returns
/// it: the current layer, with the Object tool picked.
fn new_3d(paint: &mut PaintDriver, new: &str) -> sue_core::EntityId {
    paint.command(new);
    paint.settle(2);
    // Layer, New Layer, 3D Layer asks for its shape first.
    if new == "layer.new.3d" {
        paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    }
    paint.settle(4);
    let arena = &paint.runtime.engine.world.arena;
    let layer = sue_paint::doc::layers(arena, picture(paint))
        .into_iter()
        .rev()
        .find(|info| sue_paint::three::is_3d(arena, info.id))
        .map(|info| info.id)
        .expect("a 3D layer");
    show_tool_property(paint);
    layer
}

/// The 3D layer's model.
fn model(paint: &PaintDriver, layer: sue_core::EntityId) -> &sue_core::Entity {
    let arena = &paint.runtime.engine.world.arena;
    arena
        .get_by_id(sue_paint::three::model_of(arena, layer).unwrap())
        .unwrap()
}

/// The layer's pixels.
fn pixels(paint: &PaintDriver, layer: sue_core::EntityId) -> Vec<u8> {
    sue_paint::doc::raster(&paint.runtime.engine.world.arena, layer)
        .unwrap()
        .bytes()
        .unwrap()
        .to_vec()
}

/// Does `change` (a drag or click in Tool Property), which must change
/// the layer and be one undo step; `check` holds after it. Undo finds the
/// arena exactly as before, redo exactly as after.
fn one_step(
    paint: &mut PaintDriver,
    layer: sue_core::EntityId,
    what: &str,
    change: impl FnOnce(&mut PaintDriver),
    check: impl Fn(&PaintDriver),
) {
    let before = arena_bytes(paint);
    let drawn = pixels(paint, layer);
    let depth = paint.runtime.engine.history.depth();
    change(paint);
    check(paint);
    assert_ne!(pixels(paint, layer), drawn, "{what}: drawn again");
    assert_eq!(
        paint.runtime.engine.history.depth(),
        depth + 1,
        "{what}: one undo step"
    );
    let after = arena_bytes(paint);
    paint.command("edit.undo");
    paint.settle(3);
    assert!(arena_bytes(paint) == before, "{what}: undone exactly");
    paint.command("edit.redo");
    paint.settle(3);
    assert!(arena_bytes(paint) == after, "{what}: redone exactly");
    check(paint);
}

#[test]
fn sue_paint_3d_tool_property_has_the_object_camera_light_outline_and_body_rows() {
    let mut paint = PaintDriver::new("paint-3d-rows");
    new_3d(&mut paint, "layer.new.figure");
    let named = step_named(&mut paint, Vec::new());
    let seen = every_row(&mut paint);
    let shown: Vec<&str> = seen.iter().map(String::as_str).collect();
    for text in [
        "3D object",
        "Camera",
        "Light source",
        "Outline",
        "Body shape",
        "Pose",
        "Male",
        "Female",
        "Manga Perspective",
        "Adjust head to body ratio with height",
        "Put on the ground",
        "Initialize",
        "Register body shape",
        "More muscle",
        "Less muscle",
        "Thin",
        "Fat",
    ] {
        assert!(shown.contains(&text), "{text:?} in {shown:?}");
    }
    // The controls without words of their own are named for screen
    // readers and in their tooltips.
    for name in [
        "Move",
        "Rotate",
        "Slide",
        "Orbit",
        "Pan",
        "Zoom",
        "Body shape chart",
        "Reset angle",
        "Reset position",
        "Reset scale",
        "Object scale",
        "Outline width",
        "Body part",
        "Head to body ratio",
    ] {
        assert!(
            named.iter().any(|(label, _)| label == name),
            "{name:?} is named"
        );
    }
}

#[test]
fn sue_paint_3d_camera_buttons_turn_move_and_zoom_as_one_step_each() {
    let mut paint = PaintDriver::new("paint-3d-camera");
    let layer = new_3d(&mut paint, "layer.new.3d");
    let view = |paint: &PaintDriver| {
        sue_paint::three::View3D::of(paint.runtime.engine.world.arena.get_by_id(layer).unwrap())
    };
    let start = view(&paint);
    one_step(
        &mut paint,
        layer,
        "turn the camera",
        |paint| drag_control(paint, "Orbit", vec2(50.0, 0.0)),
        |paint| {
            assert!(
                (view(paint).yaw - (start.yaw - 20.0)).abs() < 1.0,
                "{:?}",
                view(paint).yaw
            )
        },
    );
    let start = view(&paint);
    one_step(
        &mut paint,
        layer,
        "move the camera",
        |paint| drag_control(paint, "Pan", vec2(60.0, 0.0)),
        |paint| {
            assert!(
                view(paint).pan[0] < start.pan[0] - 0.1,
                "{:?}",
                view(paint).pan
            )
        },
    );
    let start = view(&paint);
    one_step(
        &mut paint,
        layer,
        "bring the camera nearer",
        |paint| drag_control(paint, "Zoom", vec2(0.0, -80.0)),
        |paint| assert!(view(paint).distance < start.distance * 0.8),
    );
    // Manga Perspective, on and off, from its checkbox.
    one_step(
        &mut paint,
        layer,
        "manga perspective",
        |paint| click_control(paint, "Manga Perspective"),
        |paint| assert!(view(paint).manga > 0.0),
    );
}

#[test]
fn sue_paint_3d_object_buttons_move_turn_and_slide_it_as_one_step_each() {
    let mut paint = PaintDriver::new("paint-3d-object");
    let layer = new_3d(&mut paint, "layer.new.3d");
    let place = |paint: &PaintDriver| sue_paint::three::Placement::of(model(paint, layer));
    one_step(
        &mut paint,
        layer,
        "move the object",
        |paint| drag_control(paint, "Move", vec2(0.0, -60.0)),
        |paint| assert!(place(paint).position[1] > 0.2, "up: {:?}", place(paint)),
    );
    one_step(
        &mut paint,
        layer,
        "turn the object",
        |paint| drag_control(paint, "Rotate", vec2(60.0, 0.0)),
        |paint| assert!(place(paint).turn[0] > 20.0, "{:?}", place(paint)),
    );
    let before = place(&paint);
    one_step(
        &mut paint,
        layer,
        "slide the object",
        |paint| drag_control(paint, "Slide", vec2(0.0, -60.0)),
        |paint| {
            let now = place(paint);
            assert_eq!(now.position[1], before.position[1], "on the ground's plane");
            assert!(
                (now.position[0] - before.position[0]).abs()
                    + (now.position[2] - before.position[2]).abs()
                    > 0.2,
                "{now:?}"
            );
        },
    );
}

#[test]
fn sue_paint_3d_figure_body_type_chart_and_ratio_shape_it_as_one_step_each() {
    let mut paint = PaintDriver::new("paint-3d-body");
    let layer = new_3d(&mut paint, "layer.new.figure");
    let body = |paint: &PaintDriver| sue_paint::figure::Body::of(model(paint, layer));
    // A body type.
    one_step(
        &mut paint,
        layer,
        "body type",
        |paint| {
            let at = text_shown(paint, "Female").center();
            paint.drag(at, at, egui::PointerButton::Primary);
        },
        |paint| assert_eq!(body(paint).build, sue_paint::figure::Build::Female),
    );
    // The body shape chart: dragged to the fat side, the full body.
    one_step(
        &mut paint,
        layer,
        "body shape chart",
        |paint| drag_control(paint, "Body shape chart", vec2(40.0, 0.0)),
        |paint| assert!(body(paint).shape.0[0][0] > 0.2, "{:?}", body(paint).shape),
    );
    // The body part list: Arms picked, the chart shapes the arms alone.
    click_control(&mut paint, "Body part");
    paint.click_text("Arms");
    paint.settle(4);
    let whole = body(&paint).shape.0[0];
    one_step(
        &mut paint,
        layer,
        "arms on the chart",
        |paint| drag_control(paint, "Body shape chart", vec2(0.0, -40.0)),
        |paint| {
            let shape = body(paint).shape;
            assert!(shape.0[3][1] > 0.2, "more muscle on the arms: {shape:?}");
            assert_eq!(shape.0[0], whole, "the full body stays");
            assert_eq!(shape.0[4], [0.0, 0.0], "the legs stay");
        },
    );
    // Adjust head to body ratio with height: on, then a shorter figure
    // has a smaller ratio.
    click_control(&mut paint, "Adjust head to body ratio with height");
    assert!(body(&paint).follow);
    let heads = body(&paint).heads;
    one_step(
        &mut paint,
        layer,
        "height with the ratio",
        |paint| {
            let slider = control(paint, "Height");

            let from = slider.center();

            paint.drag(
                from,
                pos2(slider.left() + 2.0, from.y),
                egui::PointerButton::Primary,
            );
        },
        |paint| {
            let now = body(paint);
            assert!(now.height < 150.0, "shorter: {now:?}");
            assert!(now.heads < heads - 0.5, "the ratio follows: {}", now.heads);
            assert!((now.heads - sue_paint::figure::Body::heads_for(now.height)).abs() < 1e-3);
        },
    );
}

#[test]
fn sue_paint_3d_outline_width_draws_wider_lines() {
    let mut paint = PaintDriver::new("paint-3d-outline");
    let layer = new_3d(&mut paint, "layer.new.3d");
    let view = |paint: &PaintDriver| {
        sue_paint::three::View3D::of(paint.runtime.engine.world.arena.get_by_id(layer).unwrap())
    };
    let ink = |paint: &PaintDriver| {
        pixels(paint, layer)
            .chunks(4)
            .filter(|pixel| pixel[3] > 0 && pixel[0] < 64)
            .count()
    };
    let thin = ink(&paint);
    let width = view(&paint).line_width;
    one_step(
        &mut paint,
        layer,
        "outline width",
        |paint| {
            let slider = control(paint, "Outline width");
            let from = slider.center();
            paint.drag(
                from,
                pos2(slider.right() - 2.0, from.y),
                egui::PointerButton::Primary,
            );
        },
        |paint| {
            assert!(
                view(paint).line_width > width * 2.0,
                "{}",
                view(paint).line_width
            )
        },
    );
    assert!(
        ink(&paint) > thin,
        "wider lines ink more: {thin} to {}",
        ink(&paint)
    );
}
