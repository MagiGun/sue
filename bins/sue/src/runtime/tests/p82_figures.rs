//! Sue Paint 0.0.82: the 3D drawing figures driven through real frames.
//! The whole body is moved by its hips with its feet and a pinned hand
//! staying put, along one axis by an arrow of the translate manipulator,
//! and leaned by its chest; a pinned hand holds while its own upper arm
//! is dragged. Each is one undo step that undoes to the arena exactly as
//! it was and redoes exactly.

use super::p81_figures::{click_at, on_screen, redo, sight, undo};
use super::paint_3d::{arena_bytes, click_control, drag_control, model, new_3d, one_step};
use super::*;

use sue_paint::figure::{Body, Kind, Pose};

fn far(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Where every joint of the figure of `layer` is.
fn placed(paint: &PaintDriver, layer: sue_core::EntityId) -> (Pose, Body, Vec<[f32; 3]>) {
    let (pose, body, _) = sight(paint, layer);
    let joints = sue_paint::figure::solve(&pose, &body)
        .into_iter()
        .map(|(at, _)| at)
        .collect();
    (pose, body, joints)
}

#[test]
fn p82_the_hips_move_the_whole_body_and_held_hands_and_feet_stay() {
    let mut paint = PaintDriver::new("p82-figure-move");
    let layer = new_3d(&mut paint, "layer.new.figure");
    let (base, body, before) = placed(&paint, layer);
    assert!(body.ground, "a new figure keeps to the ground");
    // The hips dragged down and across: the body comes down on bent
    // knees and goes across; its feet stay on the floor where they were.
    let from = on_screen(&paint, layer, before[0]);
    let to = from + vec2(26.0, 34.0);
    one_step(
        &mut paint,
        layer,
        "the hips dragged down and across",
        |paint| {
            paint.drag(from, to, egui::PointerButton::Primary);
            paint.settle(4);
        },
        |paint| {
            let (pose, body, now) = placed(paint, layer);
            assert_eq!(pose.0[0], base.0[0], "the hips did not turn");
            assert!(now[0][1] < before[0][1] - 0.04, "the hips came down");
            assert!(body.place.position[0] != 0.0, "the figure went across");
            assert_eq!(body.place.position[1], 0.0, "and was not sunk");
            assert!(
                pose.0[12][0] > 10.0 && pose.0[15][0] > 10.0,
                "the knees bent"
            );
            // The feet went across with the body and stayed on the
            // floor.
            for foot in [13, 16] {
                assert!(
                    (now[foot][1] - before[foot][1]).abs() < 0.03,
                    "foot {foot} stays down: {:?} {:?}",
                    now[foot],
                    before[foot]
                );
                assert!(now[foot][0] > before[foot][0] + 0.02, "and went along");
            }
            // The hips are where they were let go.
            let seen = on_screen(paint, layer, now[0]);
            assert!((seen - to).length() < 16.0, "{seen:?} for {to:?}");
        },
    );
    // The left hand pinned (picked by a click on it, then Pin joint):
    // the hips dragged across again, and the hand stays while the other
    // goes with the body.
    let (_, _, held) = placed(&paint, layer);
    let wrist = on_screen(&paint, layer, held[7]);
    click_at(&mut paint, wrist);
    paint.command("figure.pin");
    paint.settle(3);
    let pins = sue_paint::figure::pins_of(super::paint_3d::model(&paint, layer));
    assert_eq!(pins, vec![7], "the picked hand is pinned");
    let (_, _, held) = placed(&paint, layer);
    let from = on_screen(&paint, layer, held[0]);
    let to = from + vec2(24.0, 0.0);
    one_step(
        &mut paint,
        layer,
        "the hips dragged across with a hand pinned",
        |paint| {
            paint.drag(from, to, egui::PointerButton::Primary);
            paint.settle(4);
        },
        |paint| {
            let (_, _, now) = placed(paint, layer);
            assert!(far(now[0], held[0]) > 0.05, "the body moved");
            assert!(
                far(now[7], held[7]) < 0.04,
                "the pinned hand stays: {}",
                far(now[7], held[7])
            );
            assert!(far(now[10], held[10]) > 0.04, "the free hand went along");
        },
    );
    // The pinned hand's own upper arm dragged: the elbow swings, the
    // wrist holds.
    let (_, _, held) = placed(&paint, layer);
    let upper = [0, 1, 2].map(|axis| held[5][axis] * 0.5 + held[6][axis] * 0.5);
    let from = on_screen(&paint, layer, upper);
    let depth = paint.runtime.engine.history.depth();
    paint.drag(from, from + vec2(18.0, -10.0), egui::PointerButton::Primary);
    paint.settle(4);
    let (_, _, now) = placed(&paint, layer);
    assert!(
        far(now[7], held[7]) < 0.04,
        "the pin holds as its arm is dragged: {}",
        far(now[7], held[7])
    );
    assert!(paint.runtime.engine.history.depth() <= depth + 1);
}

#[test]
fn p82_an_arrow_moves_the_body_along_one_axis_and_the_chest_leans_it() {
    let mut paint = PaintDriver::new("p82-figure-arrows");
    let layer = new_3d(&mut paint, "layer.new.figure");
    let (base, body, before) = placed(&paint, layer);
    // The hips picked by a click: the translate manipulator shows.
    let hips = on_screen(&paint, layer, before[0]);
    click_at(&mut paint, hips);
    let arrows = sue_paint::figure::arrows(&base, &body, 0).expect("the root has arrows");
    assert!(sue_paint::figure::arrows(&base, &body, 5).is_none());
    // The x arrow dragged along itself.
    let (tail, head) = (
        on_screen(&paint, layer, arrows[0][0]),
        on_screen(&paint, layer, arrows[0][1]),
    );
    let grip = tail + (head - tail) * 0.5;
    let along = (head - tail).normalized();
    let state = arena_bytes(&paint);
    one_step(
        &mut paint,
        layer,
        "the x arrow dragged",
        |paint| {
            paint.drag(grip, grip + along * 40.0, egui::PointerButton::Primary);
            paint.settle(4);
        },
        |paint| {
            let (pose, body, now) = placed(paint, layer);
            assert!(body.place.position[0] > 0.02, "{:?}", body.place.position);
            assert_eq!(body.place.position[1], 0.0);
            assert_eq!(body.place.position[2], 0.0);
            assert_eq!(pose.0[0], base.0[0]);
            assert!(now[0][0] > before[0][0] + 0.02, "the hips went along x");
            assert!((now[0][2] - before[0][2]).abs() < 1e-4, "and only x");
        },
    );
    undo(&mut paint);
    assert!(arena_bytes(&paint) == state, "the move undone exactly");
    redo(&mut paint);
    // The chest dragged: the trunk leans, the hips and legs stay.
    let (posed, _, now) = placed(&paint, layer);
    let chest = [0, 1, 2].map(|axis| now[2][axis] * 0.5 + now[3][axis] * 0.5);
    let from = on_screen(&paint, layer, chest);
    one_step(
        &mut paint,
        layer,
        "the chest dragged",
        |paint| {
            paint.drag(from, from + vec2(34.0, 6.0), egui::PointerButton::Primary);
            paint.settle(4);
        },
        |paint| {
            let (pose, _, after) = placed(paint, layer);
            assert!(
                pose.0[1] != posed.0[1] || pose.0[2] != posed.0[2],
                "the trunk turned"
            );
            assert_eq!(pose.0[0], posed.0[0], "the hips did not");
            assert!(far(after[3], now[3]) > 0.03, "the neck's foot moved");
            assert!(far(after[13], now[13]) < 0.03, "the feet stay");
            assert_eq!(pose.0[11], posed.0[11], "the legs did not turn");
        },
    );
}

#[test]
fn p82_the_face_is_shaped_wears_an_expression_and_looks_where_it_is_told() {
    let mut paint = PaintDriver::new("p82-figure-face");
    let layer = new_3d(&mut paint, "layer.new.figure");
    let body = |paint: &PaintDriver| Body::of(model(paint, layer));
    // The Face section of Tool Property: a slider of the face, dragged.
    one_step(
        &mut paint,
        layer,
        "a longer nose",
        |paint| drag_control(paint, "Nose length", vec2(40.0, 0.0)),
        |paint| {
            assert!(
                body(paint).slider("nose_length") > 0.1,
                "{:?}",
                body(paint).parts
            )
        },
    );
    one_step(
        &mut paint,
        layer,
        "larger eyes",
        |paint| drag_control(paint, "Eye size", vec2(50.0, 0.0)),
        |paint| assert!(body(paint).slider("eye_size") > 0.1),
    );
    // An expression's own slider.
    one_step(
        &mut paint,
        layer,
        "a smile",
        |paint| drag_control(paint, "Smile", vec2(200.0, 0.0)),
        |paint| {
            assert_eq!(body(paint).slider("smile"), 1.0);
            assert_eq!(body(paint).expression(), Some(1));
        },
    );
    // The face is kept with the body shape, on the model.
    let kept = model(&paint, layer)
        .property(sue_paint::figure::PARTS_TEXT)
        .and_then(sue_core::Value::as_str)
        .unwrap()
        .to_owned();
    assert!(
        kept.contains("nose_length=") && kept.contains("smile=1"),
        "{kept}"
    );
    // The gaze: both eyes turned by one slider, as one pose step.
    let (base, ..) = sight(&paint, layer);
    one_step(
        &mut paint,
        layer,
        "the eyes look across",
        |paint| drag_control(paint, "Gaze across", vec2(40.0, 0.0)),
        |paint| {
            let (pose, body, _) = sight(paint, layer);
            let [across, up] = sue_paint::figure::gaze(&pose, &body);
            assert!(across > 5.0 && up == 0.0, "{across} {up}");
            assert_eq!(pose.0[52], pose.0[53], "both eyes together");
            for joint in 0..52 {
                assert_eq!(pose.0[joint], base.0[joint], "only the eyes turned");
            }
        },
    );
    // The head figure has the face's rows too, and none of the legs'.
    paint.command("figure.model.4");
    paint.settle(4);
    assert_eq!(body(&paint).kind, Kind::Head);
    assert!(body(&paint).slider("smile") == 1.0, "the face stays");
    one_step(
        &mut paint,
        layer,
        "a wider jaw on the head",
        |paint| drag_control(paint, "Jaw", vec2(40.0, 0.0)),
        |paint| assert!(body(paint).slider("jaw") > 0.1),
    );
    let told = super::paint_3d::step_named(&mut paint, Vec::new());
    assert!(
        !told.iter().any(|(name, _)| name == "Leg length"),
        "a head has no legs to shape"
    );
    let _ = click_control;
}

#[test]
fn p82_the_hands_veins_have_a_pattern_slider() {
    let mut paint = PaintDriver::new("p82-figure-veins");
    let layer = new_3d(&mut paint, "layer.new.figure");
    paint.command("figure.model.2");
    paint.settle(4);
    let body = |paint: &PaintDriver| Body::of(model(paint, layer));
    assert_eq!(body(&paint).kind, Kind::Hand);
    one_step(
        &mut paint,
        layer,
        "more veins",
        |paint| drag_control(paint, "Veins", vec2(80.0, 0.0)),
        |paint| assert!(body(paint).slider("veins") > 0.5),
    );
    one_step(
        &mut paint,
        layer,
        "another vein pattern",
        |paint| drag_control(paint, "Vein pattern", vec2(60.0, 0.0)),
        |paint| assert!(body(paint).slider("vein_pattern") > 0.2),
    );
}

/// How many of the layer's pixels are colored (not a gray).
fn colored(paint: &PaintDriver, layer: sue_core::EntityId) -> usize {
    super::paint_3d::pixels(paint, layer)
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 0 && pixel[0].abs_diff(pixel[1]) > 12)
        .count()
}

#[test]
fn p82_an_imported_model_shows_its_colors_morphs_and_keeps_to_limits_when_asked() {
    use super::p81_figures::{added, layers, undo_pair};
    let mut paint = PaintDriver::new("p82-import-look");
    let folder = std::env::temp_dir().join(format!("sue-p82-import-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let square = folder.join("Square.gltf");
    std::fs::write(&square, sue_paint::model3d::sample::painted_gltf(true)).unwrap();
    let (obj, mtl) = sue_paint::model3d::sample::painted_obj();
    let pyramid = folder.join("pyramid.obj");
    std::fs::write(&pyramid, obj).unwrap();
    std::fs::write(folder.join("pyramid.mtl"), mtl).unwrap();
    std::fs::write(
        folder.join("colors.png"),
        sue_paint::model3d::sample::four_colors_png(),
    )
    .unwrap();
    let import = |paint: &mut PaintDriver, path: &std::path::Path| {
        paint.command("file.import.3d");
        paint.settle(2);
        let now = paint.now;
        let _ = paint.runtime.answer_pick(path, &mut NoTextures, now);
        paint.settle(6);
    };
    // The glTF with a skeleton: it shows its colors and its picture.
    let had = layers(&paint);
    import(&mut paint, &square);
    let layer = added(&paint, &had);
    super::paint_3d::show_tool_property(&mut paint);
    let entity = |paint: &PaintDriver| model(paint, layer).clone();
    assert!(sue_paint::figure::is_figure(&entity(&paint)));
    assert!(colored(&paint, layer) > 200, "it shows in color");
    // Joint angle limits are off until its own tick turns them on.
    assert!(!Body::of(&entity(&paint)).limits);
    let depth = paint.runtime.engine.history.depth();
    let before = arena_bytes(&paint);
    click_control(&mut paint, "Joint angle limits");
    assert!(Body::of(&entity(&paint)).limits, "limits on for this model");
    assert_eq!(paint.runtime.engine.history.depth(), depth + 1, "one step");
    undo(&mut paint);
    assert!(arena_bytes(&paint) == before, "undone exactly");
    // Plain gray, by its tick.
    one_step(
        &mut paint,
        layer,
        "plain gray",
        |paint| click_control(paint, "Show its colors and pictures"),
        |paint| {
            assert!(!sue_paint::model3d::Look::of(&entity(paint)).colors);
            assert_eq!(colored(paint, layer), 0, "all gray");
        },
    );
    undo(&mut paint);
    // Its morph target, by its own slider.
    one_step(
        &mut paint,
        layer,
        "the morph target brought in",
        |paint| drag_control(paint, "Taller", vec2(300.0, 0.0)),
        |paint| {
            let look = sue_paint::model3d::Look::of(&entity(paint));
            assert_eq!(look.morphs, vec![("Taller".to_owned(), 1.0)]);
        },
    );
    // The OBJ with its material file and picture beside it.
    let count = layers(&paint).len();
    let had = layers(&paint);
    undo_pair(
        &mut paint,
        "an OBJ with materials imported",
        |paint| import(paint, &pyramid),
        |paint| layers(paint).len() == count + 1,
    );
    let layer = added(&paint, &had);
    let pyramid = model(&paint, layer);
    assert!(sue_paint::model3d::has_colors(pyramid));
    assert_eq!(sue_paint::model3d::file_of(pyramid).unwrap().1, "glb");
    assert!(colored(&paint, layer) > 100, "its red side shows");
    let _ = std::fs::remove_dir_all(&folder);
}
