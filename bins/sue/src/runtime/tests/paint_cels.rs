//! Sue Paint: the Animation Cels palette's entries (AB-8, ABV-16): the
//! cel being drawn, one entry per layer on a light table with its eye,
//! opacity and color mode, and a layer dragged from the Layer palette to
//! register it.

use super::paint_anim::{arena_bytes, open_picture, press};
use super::*;

/// The cel being drawn: the one the timeline shows on its folder's track
/// at the frame shown.
fn editing_cel(paint: &PaintDriver) -> sue_core::EntityId {
    let arena = &paint.runtime.engine.world.arena;
    let picture = open_picture(paint);
    let sequence = sue_paint::anim::timeline(arena, picture).expect("a timeline");
    let track = &sequence.tracks[sue_paint::anim::folders(sequence)[0]];
    sue_paint::anim::cel_at(track, sequence.rate, 3).expect("a cel at frame 4")
}

/// The layer named `name`.
fn layer_named(paint: &PaintDriver, name: &str) -> sue_core::EntityId {
    paint
        .runtime
        .engine
        .world
        .arena
        .iter()
        .find(|(_, entity)| entity.name() == name)
        .map(|(_, entity)| entity.id())
        .unwrap_or_else(|| panic!("a layer named {name}"))
}

/// Where the right dock (its two columns) drew `text`, the topmost.
fn in_right_dock(paint: &PaintDriver, text: &str) -> egui::Rect {
    paint
        .texts
        .iter()
        .filter(|(found, rect)| found == text && rect.left() > 840.0)
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("{text:?} in the right dock: {:?}", paint.texts))
}

/// The Layer palette's row named `name` (in the right column, the
/// lowest drawing of the name).
fn layer_row(paint: &mut PaintDriver, name: &str) -> egui::Rect {
    paint.settle(2);
    paint
        .texts
        .iter()
        .filter(|(text, rect)| text == name && rect.left() > 1106.0 && rect.bottom() < 900.0)
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("{name}'s row: {:?}", paint.texts))
}

/// Where the Animation Cels palette drew `text` (its column, left of the
/// Layer palette's), the topmost.
fn in_cels(texts: &[(String, egui::Rect)], text: &str, below: f32) -> Option<egui::Rect> {
    texts
        .iter()
        .filter(|(found, rect)| {
            found == text && rect.left() > 840.0 && rect.right() < 1106.0 && rect.top() > below
        })
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
}

#[test]
fn sue_paint_animation_cels_entries_have_an_eye_opacity_and_mode_and_take_a_drop() {
    let mut paint = PaintDriver::new("paint-cels");
    // A folder with cel 1 on frame 1 and cel 2 from frame 4.
    paint.command("anim.layer.folder");
    paint.command("anim.cel.new");
    for _ in 0..3 {
        paint.command("anim.next");
    }
    paint.command("anim.cel.new");
    // Window, Animation Cels: docked under the Layer palette, both in
    // sight, so a layer can be dragged from one to the other.
    paint.command("window.cels");
    paint.settle(6);
    let cel = editing_cel(&paint);
    let caption = in_right_dock(&paint, "Light table of cel 2");
    let row = layer_row(&mut paint, "Layer 1");
    assert!(
        caption.right() < row.left(),
        "the Animation Cels palette in a column beside the Layer palette's: {caption:?} {row:?}"
    );
    let layer = layer_named(&paint, "Layer 1");
    let on_table =
        |paint: &PaintDriver| sue_paint::anim::light_table(&paint.runtime.engine.world.arena, cel);
    assert!(on_table(&paint).is_empty(), "nothing traced yet");

    // Drag to register: Layer 1's row let go over the cel's light table.
    let depth = paint.runtime.engine.history.depth();
    let before_drop = arena_bytes(&paint);
    paint.drag(row.center(), caption.center(), egui::PointerButton::Primary);
    assert_eq!(on_table(&paint), vec![layer], "registered by the drop");
    let after_drop = arena_bytes(&paint);
    assert_eq!(
        paint.runtime.engine.history.depth(),
        depth + 1,
        "one undo step"
    );
    // Its entry, as a Layer palette row: its name and how it shows.
    let texts = paint.settle(4);
    let entry = in_cels(&texts, "Layer 1", caption.bottom())
        .unwrap_or_else(|| panic!("Layer 1's entry under the cel: {texts:?}"));
    assert!(
        texts
            .iter()
            .any(|(text, rect)| text == "100 %  As the light table"
                && (rect.bottom() - entry.top()).abs() < 4.0),
        "its opacity and mode over its name: {texts:?}"
    );
    // Undo and redo the drop: the arena as before, then as after.
    paint.command("edit.undo");
    paint.settle(2);
    assert!(arena_bytes(&paint) == before_drop, "undo is exact");
    paint.command("edit.redo");
    paint.settle(4);
    assert!(arena_bytes(&paint) == after_drop, "redo is exact");
    // Dropped again: it is on the table once, and says so.
    let depth = paint.runtime.engine.history.depth();
    let caption = in_right_dock(&paint, "Light table of cel 2");
    let row = layer_row(&mut paint, "Layer 1");
    paint.drag(row.center(), caption.center(), egui::PointerButton::Primary);
    assert_eq!(on_table(&paint), vec![layer], "still once");
    assert_eq!(
        paint.runtime.engine.history.depth(),
        depth,
        "nothing to undo"
    );

    let shown_entry = |paint: &PaintDriver| {
        sue_paint::anim::entry(
            &paint.runtime.engine.world.arena,
            cel,
            sue_paint::anim::LIGHT_TABLE,
            layer,
        )
    };
    let traced = |paint: &PaintDriver| {
        sue_paint::anim::traced_on(
            &paint.runtime.engine.world.arena,
            cel,
            sue_paint::anim::LIGHT_TABLE,
            [64, 64],
        )
        .len()
    };
    // The entry's eye: off, it leaves the light table; undo brings it.
    let texts = paint.settle(2);
    let entry = in_cels(&texts, "Layer 1", caption.bottom()).expect("the entry");
    let found = super::paint_anim::tips_along(&mut paint, entry.top(), 846.0, entry.left());
    let eye = super::paint_anim::tip(&found, "Show or Hide on the Light Table");
    let before_eye = arena_bytes(&paint);
    super::paint_anim::click_at(&mut paint, eye);
    assert!(!shown_entry(&paint).shown, "the eye hides it");
    assert_eq!(traced(&paint), 0, "and the light table leaves it out");
    let after_eye = arena_bytes(&paint);
    paint.command("edit.undo");
    paint.settle(2);
    assert!(shown_entry(&paint).shown, "undo shows it again");
    assert_eq!(traced(&paint), 1);
    assert!(arena_bytes(&paint) == before_eye, "undo is exact");
    paint.command("edit.redo");
    paint.settle(2);
    assert!(!shown_entry(&paint).shown, "redo hides it");
    assert!(arena_bytes(&paint) == after_eye, "redo is exact");
    paint.command("edit.undo");
    paint.settle(2);

    // The entry a drop made is picked: its opacity, with the number, and
    // its color mode at the palette's foot. A click on it puts them away,
    // a second brings them back.
    let colors_shown =
        |texts: &[(String, egui::Rect)]| texts.iter().any(|(text, _)| text == "Colors");
    assert!(colors_shown(&paint.settle(2)), "picked by the drop");
    let texts = super::paint_anim::click_at(&mut paint, entry.center());
    assert!(!colors_shown(&texts), "a click puts them away: {texts:?}");
    let texts = super::paint_anim::click_at(&mut paint, entry.center());
    assert!(
        texts.iter().any(|(text, _)| text == "Colors"),
        "the picked entry's settings: {texts:?}"
    );
    let opacity = texts
        .iter()
        .filter(|(text, rect)| {
            text == "Opacity" && rect.top() > entry.bottom() && rect.right() < 1106.0
        })
        .map(|(_, rect)| *rect)
        .next()
        .unwrap_or_else(|| panic!("the entry's opacity: {texts:?}"));
    let number = texts
        .iter()
        .filter(|(text, rect)| {
            text.starts_with("100") && (rect.center().y - opacity.center().y).abs() < 8.0
        })
        .map(|(_, rect)| *rect)
        .next()
        .unwrap_or_else(|| panic!("the opacity's number: {texts:?}"));
    // Dragged along its track, it shows at once and is kept as one step
    // when let go.
    let depth = paint.runtime.engine.history.depth();
    let before_drag = arena_bytes(&paint);
    let track = egui::Rect::from_min_max(
        Pos2::new(opacity.right() + 12.0, opacity.top()),
        Pos2::new(number.left() - 12.0, opacity.bottom()),
    );
    let from = Pos2::new(track.right() - 2.0, track.center().y);
    let to = Pos2::new(track.left() + track.width() * 0.4, track.center().y);
    paint.drag(from, to, egui::PointerButton::Primary);
    let kept = shown_entry(&paint).opacity;
    assert!(kept > 0.2 && kept < 0.7, "dragged to about 40 %: {kept}");
    let after_drag = arena_bytes(&paint);
    assert_eq!(
        paint.runtime.engine.history.depth(),
        depth + 1,
        "one undo step for the whole drag"
    );
    let reads = format!(
        "{} %  As the light table",
        sue_lang::number(f64::from(kept) * 100.0, 0)
    );
    let texts = paint.settle(2);
    assert!(
        texts.iter().any(|(text, _)| *text == reads),
        "the entry reads {reads}: {texts:?}"
    );
    paint.command("edit.undo");
    paint.settle(2);
    assert!((shown_entry(&paint).opacity - 1.0).abs() < 1e-3, "undone");
    assert!(arena_bytes(&paint) == before_drag, "undo is exact");
    paint.command("edit.redo");
    paint.settle(2);
    assert!((shown_entry(&paint).opacity - kept).abs() < 1e-3, "redone");
    assert!(arena_bytes(&paint) == after_drag, "redo is exact");

    // The color mode: Own colors, never tinted.
    let texts = paint.settle(2);
    let colors = texts
        .iter()
        .filter(|(text, rect)| {
            text == "As the light table" && rect.left() > 840.0 && rect.right() < 1106.0
        })
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("the mode's dropdown: {texts:?}"));
    super::paint_anim::click_at(&mut paint, colors.center());
    paint.settle(4);
    let own = paint
        .texts
        .iter()
        .filter(|(text, _)| text == "Own colors")
        .map(|(_, rect)| *rect)
        .next()
        .unwrap_or_else(|| panic!("Own colors in the list: {:?}", paint.texts));
    super::paint_anim::click_at(&mut paint, own.center());
    let after_mode = arena_bytes(&paint);
    assert_eq!(
        shown_entry(&paint).mode,
        sue_paint::anim::EntryMode::Own,
        "{:?}",
        paint.texts
    );
    paint.command("edit.undo");
    paint.settle(2);
    assert_eq!(shown_entry(&paint).mode, sue_paint::anim::EntryMode::Table);
    assert!(
        (shown_entry(&paint).opacity - kept).abs() < 1e-3,
        "one step"
    );
    assert!(arena_bytes(&paint) == after_drag, "undo is exact");
    paint.command("edit.redo");
    paint.settle(2);
    assert!(arena_bytes(&paint) == after_mode, "redo is exact");
    paint.command("edit.undo");
    paint.settle(2);

    // Its X takes it off, as one step.
    let texts = paint.settle(2);
    let entry = in_cels(&texts, "Layer 1", caption.bottom())
        .unwrap_or_else(|| panic!("the entry: {texts:?}"));
    let found = super::paint_anim::tips_along(&mut paint, entry.top(), entry.right(), 1104.0);
    let off = super::paint_anim::tip(&found, "Deregister from the light table");
    let before_off = arena_bytes(&paint);
    super::paint_anim::click_at(&mut paint, off);
    assert!(on_table(&paint).is_empty(), "taken off");
    let after_off = arena_bytes(&paint);
    paint.command("edit.undo");
    paint.settle(2);
    assert!(arena_bytes(&paint) == before_off, "undo is exact");
    paint.command("edit.redo");
    paint.settle(2);
    assert!(arena_bytes(&paint) == after_off, "redo is exact");
    press(&mut paint, egui::Key::Escape);
}
