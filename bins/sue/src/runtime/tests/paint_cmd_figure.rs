//! Sue Paint: every command of the Figure group (the 3D drawing figure's
//! poses, scanners and Register pose as material), run on a canvas, each
//! edit undone and redone.

use super::*;

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

/// Runs `command` and lets the frames after it draw.
fn run(paint: &mut PaintDriver, command: &str) {
    paint.command(command);
    paint.settle(3);
}

/// Runs `command`, which must change the document as one undo step;
/// then undoes it and finds the arena as before, ids included, and
/// redoes it and finds the arena as after.
fn undo_pair(paint: &mut PaintDriver, command: &str) {
    let before = arena_bytes(paint);
    let depth = paint.runtime.engine.history.depth();
    run(paint, command);
    let after = arena_bytes(paint);
    assert_ne!(after, before, "{command} changed nothing");
    assert_eq!(
        paint.runtime.engine.history.depth(),
        depth + 1,
        "{command} is one undo step"
    );
    run(paint, "edit.undo");
    assert!(arena_bytes(paint) == before, "{command}: undo");
    run(paint, "edit.redo");
    assert!(arena_bytes(paint) == after, "{command}: redo");
}

/// Runs `command`, which must leave the document and its history alone
/// and ask for no file.
fn keeps_the_document(paint: &mut PaintDriver, command: &str) {
    let before = arena_bytes(paint);
    let depth = paint.runtime.engine.history.depth();
    run(paint, command);
    assert!(arena_bytes(paint) == before, "{command} changed the arena");
    assert_eq!(paint.runtime.engine.history.depth(), depth, "{command}");
    assert!(
        paint.runtime.pending_pick().is_none(),
        "{command} asked for a file"
    );
}

/// Runs `command`, which must be off: it leaves the document, its
/// history and the status bar alone and asks for no file. (A command
/// that was on but found no figure to pose would say so in the status
/// bar.)
fn does_nothing(paint: &mut PaintDriver, command: &str) {
    const MARK: &str = "before the command";
    paint.runtime.shell.notify(MARK, false);
    keeps_the_document(paint, command);
    assert_eq!(
        paint.runtime.shell.message(),
        Some((MARK, false)),
        "{command} is off and quiet"
    );
}

/// Adds a 3D layer of `new`'s kind, which becomes the current layer.
fn new_figure(paint: &mut PaintDriver, new: &str, kind: sue_paint::figure::Kind) {
    let layers = sue_paint::doc::layers(&paint.runtime.engine.world.arena, picture(paint)).len();
    run(paint, new);
    let arena = &paint.runtime.engine.world.arena;
    let all = sue_paint::doc::layers(arena, picture(paint));
    assert_eq!(all.len(), layers + 1, "{new}");
    assert!(
        all.iter().any(|info| {
            sue_paint::figure::of_layer(arena, info.id)
                .and_then(|model| arena.get_by_id(model).ok())
                .is_some_and(|model| sue_paint::figure::Body::of(model).kind == kind)
        }),
        "{new} made a {kind:?}"
    );
}

/// The file picker the last command asked for: what it is for.
fn take_pick(paint: &mut PaintDriver) -> Option<String> {
    paint.runtime.pending_pick.take().map(|pick| pick.purpose)
}

#[test]
fn sue_paint_runs_every_figure_command() {
    let mut paint = PaintDriver::new("paint-cmd-figure");
    // The group, as the registry has it; each name is run below.
    let group: Vec<&str> = sue_paint::registry()
        .iter()
        .filter(|command| command.group == "figure")
        .map(|command| command.name)
        .collect();
    assert_eq!(
        group,
        [
            "figure.register_pose",
            "figure.pose_scanner",
            "figure.hand_scanner",
            "figure.flip",
            "figure.reset",
            "figure.preset.",
        ]
    );

    // On the new canvas's raster layer there is no figure to pose: every
    // command of the group is off and does nothing.
    for command in [
        "figure.register_pose",
        "figure.pose_scanner",
        "figure.hand_scanner",
        "figure.flip",
        "figure.reset",
        "figure.preset.1",
    ] {
        does_nothing(&mut paint, command);
    }

    // A drawing figure: each pose is one step, undone and redone exactly.
    new_figure(
        &mut paint,
        "layer.new.figure",
        sue_paint::figure::Kind::Body,
    );
    undo_pair(&mut paint, "figure.preset.2");
    undo_pair(&mut paint, "figure.flip");
    undo_pair(&mut paint, "figure.reset");
    for index in 1..sue_paint::figure::PRESETS.len() {
        undo_pair(&mut paint, &format!("figure.preset.{index}"));
    }
    // A preset the figure does not have, or no number, leaves it.
    let past = sue_paint::figure::PRESETS.len();
    does_nothing(&mut paint, &format!("figure.preset.{past}"));
    does_nothing(&mut paint, "figure.preset.x");

    // The Pose Scanner asks for a photo of a figure; the Hand Scanner is
    // for hands only.
    does_nothing(&mut paint, "figure.hand_scanner");
    run(&mut paint, "figure.pose_scanner");
    assert_eq!(take_pick(&mut paint).as_deref(), Some("pose-scanner"));

    // Register pose as material keeps the pose as a file in the materials
    // folder and leaves the document and its history alone.
    let materials = paint.root.join("materials-here");
    let _ = paint.runtime.engine.world.rice.set(
        "sue_paint.materials_dir",
        sue_rice::RiceValue::Text(materials.display().to_string()),
    );
    keeps_the_document(&mut paint, "figure.register_pose");
    let kept = materials.join("poses").join("Figure pose 1.pose");
    let text = std::fs::read_to_string(&kept).expect("the pose is kept");
    assert!(text.starts_with("body\n"), "{text}");

    // A 3D hand: its own presets, and the Hand Scanner but not the Pose
    // Scanner.
    new_figure(&mut paint, "layer.new.hand", sue_paint::figure::Kind::Hand);
    for index in 1..sue_paint::figure::HAND_PRESETS.len() {
        undo_pair(&mut paint, &format!("figure.preset.{index}"));
    }
    undo_pair(&mut paint, "figure.reset");
    does_nothing(&mut paint, "figure.pose_scanner");
    run(&mut paint, "figure.hand_scanner");
    assert_eq!(take_pick(&mut paint).as_deref(), Some("hand-scanner"));

    // A 3D head: posed by its presets, and no scanner of its own.
    new_figure(&mut paint, "layer.new.head", sue_paint::figure::Kind::Head);
    for index in 1..sue_paint::figure::HEAD_PRESETS.len() {
        undo_pair(&mut paint, &format!("figure.preset.{index}"));
    }
    undo_pair(&mut paint, "figure.reset");
    let past = sue_paint::figure::HEAD_PRESETS.len();
    does_nothing(&mut paint, &format!("figure.preset.{past}"));
    does_nothing(&mut paint, "figure.pose_scanner");
    does_nothing(&mut paint, "figure.hand_scanner");

    let _ = std::fs::remove_dir_all(&paint.root);
}
