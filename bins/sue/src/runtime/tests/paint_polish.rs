//! Phase 4's leftovers, driven through real frames: the docked Timeline
//! opens tall enough and keeps the height dragged; an export's notice
//! never covers the timeline; the Sub Tool palette cuts no row in half;
//! Export Timelapse fitted to a length lasts that length with a few
//! frames; Show Animation Cels' three entries (ABV-9); Edit, Cut on a
//! mask; the mask of a gradient layer takes brushes and gradients; the
//! pen's twist turns and mirrors with the view.

use super::paint_anim::{line_on_canvas, open_picture, walk};
use super::*;
use egui::{Rect, pos2};
use sue_platform::stylus::{self, PenKind, PenSample, PenSource};

/// One frame with `events`: every shape it draws (texts by their words,
/// other shapes by an empty name), where, and the clip it was drawn in.
fn clipped_shapes(paint: &mut PaintDriver, events: Vec<egui::Event>) -> Vec<(String, Rect, Rect)> {
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        events,
        time: Some(paint.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    paint.now += Duration::from_millis(16);
    let (runtime, now) = (&mut paint.runtime, paint.now);
    let output = paint
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    let mut found = Vec::new();
    for clipped in &output.shapes {
        let mut look = |shape: &egui::Shape| match shape {
            egui::Shape::Text(text) => found.push((
                text.galley.text().to_owned(),
                Rect::from_min_size(text.pos, text.galley.size()),
                clipped.clip_rect,
            )),
            shape => {
                let rect = shape.visual_bounding_rect();
                if rect.is_finite() && rect.is_positive() {
                    found.push((String::new(), rect, clipped.clip_rect));
                }
            }
        };
        match &clipped.shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(&mut look),
            shape => look(shape),
        }
    }
    output.drop_without_applying_deltas();
    found
}

/// The bottom row (the Timeline's) as the last frame laid it.
fn timeline_row(paint: &PaintDriver) -> Option<Rect> {
    sue_wm::dock::bottom_row(&paint.ctx)
}

/// Opens the Timeline under the canvas when it is not open.
fn open_timeline(paint: &mut PaintDriver) -> Rect {
    paint.settle(2);
    if timeline_row(paint).is_none() {
        paint.command("window.timeline");
        paint.settle(10);
    }
    timeline_row(paint).expect("the Timeline is open under the canvas")
}

#[test]
fn sue_paint_timeline_opens_tall_and_keeps_the_height_dragged() {
    // The style's own layout: the Timeline folded under the canvas.
    let mut paint = PaintDriver::fresh("paint-polish-timeline");
    let row = open_timeline(&mut paint);
    // Room for the ruler, a folder's frames with thumbnails and more
    // tracks: not the thin strip of 150 points it was.
    assert!(row.height() >= 249.0, "{row:?}");
    assert!(
        row.bottom() > paint.page_top() + 600.0,
        "under the canvas: {row:?}"
    );
    // A person drags its top edge up: it grows by as much.
    let x = row.center().x;
    paint.drag(
        pos2(x, row.top()),
        pos2(x, row.top() - 120.0),
        egui::PointerButton::Primary,
    );
    let dragged = timeline_row(&paint).expect("still open");
    assert!(
        (dragged.height() - (row.height() + 120.0)).abs() < 2.0,
        "{row:?} dragged to {dragged:?}"
    );
    // Closed and opened again, it keeps that height.
    paint.command("window.timeline");
    paint.settle(10);
    assert!(timeline_row(&paint).is_none(), "closed");
    paint.command("window.timeline");
    paint.settle(10);
    let again = timeline_row(&paint).expect("open again");
    assert!(
        (again.height() - dragged.height()).abs() < 1.0,
        "{dragged:?} became {again:?}"
    );
    // Dragged down past its least, it stops there and still opens.
    paint.drag(
        pos2(x, again.top()),
        pos2(x, again.top() + 600.0),
        egui::PointerButton::Primary,
    );
    let least = timeline_row(&paint).expect("still open");
    assert!(
        (least.height() - sue_wm::dock::BOTTOM_LEAST).abs() < 1.0,
        "{least:?}"
    );
    // And up past the room the canvas needs, it stops short of it.
    paint.drag(
        pos2(x, least.top()),
        pos2(x, 10.0),
        egui::PointerButton::Primary,
    );
    let most = timeline_row(&paint).expect("still open");
    assert!(
        most.top() >= paint.page_top() + sue_wm::dock::BOTTOM_LEAVES - 1.0,
        "{most:?}"
    );
    // Dragged from there, it moves at once (it starts from the height
    // drawn, not a taller one kept).
    paint.drag(
        pos2(x, most.top()),
        pos2(x, most.top() + 50.0),
        egui::PointerButton::Primary,
    );
    let moved = timeline_row(&paint).expect("still open");
    assert!(
        (moved.height() - (most.height() - 50.0)).abs() < 2.0,
        "{most:?} then {moved:?}"
    );
}

/// Lets a held export go when dropped.
struct Release(std::path::PathBuf);

impl Drop for Release {
    fn drop(&mut self) {
        sue_paint::hold_export_starts(&self.0, false);
    }
}

#[test]
fn sue_paint_export_notice_never_covers_the_timeline() {
    let mut paint = PaintDriver::new("paint-polish-notice");
    walk(&mut paint);
    let row = open_timeline(&mut paint);
    let gif = paint.root.join("Walk.gif");
    std::fs::create_dir_all(&paint.root).unwrap();
    // Held before it writes, so its notice stays while frames are drawn;
    // let go however the test ends, so the export never waits forever.
    sue_paint::hold_export_starts(&gif, true);
    let _release = Release(gif.clone());
    paint.command("file.export.gif");
    paint.settle(3);
    paint.click_text("OK");
    paint.runtime.request(
        ShellRequest::Picked {
            pick: FilePick::save(
                Some(AppletId::new(sue_paint::ID).unwrap()),
                "anim-gif",
                "Export",
                "x",
            ),
            path: gif.clone(),
        },
        &mut NoTextures,
        paint.now,
    );
    let transfer = paint
        .runtime
        .transfers
        .list()
        .iter()
        .find(|transfer| transfer.path.as_deref() == Some(gif.as_path()))
        .map(|transfer| transfer.id)
        .expect("the export runs");
    for _ in 0..20 {
        paint.runtime.transfers.poll();
        paint.step(Vec::new());
        let notice = paint
            .runtime
            .shell
            .working_notice_rects(transfer)
            .expect("its notice shows");
        let row = timeline_row(&paint).unwrap_or(row);
        assert!(
            !notice.toast.intersects(row),
            "the notice {:?} is on the timeline {row:?}",
            notice.toast
        );
        assert!(notice.toast.bottom() <= row.top(), "{:?}", notice.toast);
        // On the canvas's view, beside no palette.
        let area = paint.runtime.shell.notices_area().expect("notices show");
        assert!(area.bottom() <= row.top() + 0.5, "{area:?}");
    }
    sue_paint::hold_export_starts(&gif, false);
    paint.finish_exports();
    assert!(gif.exists(), "it still exports");
}

#[test]
fn sue_paint_sub_tool_palette_cuts_no_row_in_half() {
    let mut paint = PaintDriver::fresh("paint-polish-sub-tools");
    paint.settle(4);
    let mut overflowed = 0;
    let spacing = paint.ctx.global_style().spacing.item_spacing.y;
    for tool in [
        "tool.pick.pen",
        "tool.pick.pencil",
        "tool.pick.brush",
        "tool.pick.airbrush",
        "tool.pick.decoration",
        "tool.pick.fill",
        "tool.pick.gradient",
        "tool.pick.figure",
        "tool.pick.ruler",
        "tool.pick.text",
    ] {
        paint.command(tool);
        paint.settle(6);
        let shapes = clipped_shapes(&mut paint, Vec::new());
        let header = shapes
            .iter()
            .find(|(text, _, _)| text == "Sub Tool")
            .map(|(_, rect, _)| *rect)
            .expect("the Sub Tool palette");
        let next = shapes
            .iter()
            .find(|(text, _, _)| text == "Tool Property")
            .map(|(_, rect, _)| *rect)
            .expect("Tool Property under it");
        // The Sub Tool palette's list: the clip its sub tool names are
        // drawn in, under the group chips.
        let list = shapes
            .iter()
            .filter(|(text, rect, clip)| {
                !text.is_empty()
                    && rect.top() > header.bottom()
                    && rect.top() < next.top()
                    && rect.left() < header.left() + 320.0
                    && clip.height() < next.top() - header.bottom() - 30.0
            })
            .map(|(_, _, clip)| *clip)
            .max_by(|a, b| a.top().total_cmp(&b.top()))
            .unwrap_or_else(|| panic!("{tool}: a list of sub tools"));
        // The rows' names, top to bottom: one a row, at the same place in
        // each, so the rows come every `pitch` points from the first.
        let mut names: Vec<(String, Rect)> = shapes
            .iter()
            .filter(|(text, _, clip)| *clip == list && !text.is_empty())
            .map(|(text, rect, _)| (text.clone(), *rect))
            .collect();
        names.sort_by(|a, b| a.1.top().total_cmp(&b.1.top()));
        assert!(names.len() >= 2, "{tool}: {names:?}");
        let pitch = names[1].1.top() - names[0].1.top();
        let last = names.last().map(|(_, rect)| rect.bottom()).unwrap_or(0.0);
        if last <= list.bottom() + 0.5 {
            // Everything fits, the Add Sub Tool row too.
            continue;
        }
        overflowed += 1;
        // More rows than fit: the list is as tall as whole rows, the
        // last one shown ending at its bottom (its height and one gap
        // between rows make a pitch).
        let rows = (list.height() + spacing) / pitch;
        assert!(
            (rows - rows.round()).abs() < 0.02 && rows.round() >= 1.0,
            "{tool}: {rows} rows show in {list:?} (a row every {pitch} points)"
        );
        // And every name the list draws there is whole.
        for (text, rect) in &names {
            if rect.intersects(list) {
                assert!(
                    list.top() <= rect.top() + 0.5 && rect.bottom() <= list.bottom() + 0.5,
                    "{tool}: {text:?} at {rect:?} is cut by {list:?}"
                );
            }
        }
    }
    assert!(overflowed > 0, "a tool with more sub tools than show");
}

#[test]
fn sue_paint_brush_size_palette_cuts_no_row_in_half() {
    // The default layout with the pen: Brush Size shows whole rows of
    // sizes, each dot with its number under it, never a row cut through
    // at the palette's bottom.
    let mut paint = PaintDriver::fresh("paint-polish-brush-size");
    paint.settle(4);
    paint.command("tool.pick.pen");
    paint.settle(6);
    let shapes = clipped_shapes(&mut paint, Vec::new());
    // The palette's header: the lowest "Brush Size" (Tool Property has a
    // row of that name above it).
    let header = shapes
        .iter()
        .filter(|(text, _, _)| text == "Brush Size")
        .map(|(_, rect, _)| *rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("the Brush Size palette");
    // The sizes' numbers: texts that read as a number, drawn in the
    // palette's body (a clip starting just under its header).
    let labels: Vec<(String, Rect, Rect)> = shapes
        .iter()
        .filter(|(text, rect, clip)| {
            text.parse::<f32>().is_ok()
                && (clip.top() - header.bottom()).abs() < 20.0
                && rect.left() < header.left() + 320.0
        })
        .cloned()
        .collect();
    assert!(labels.len() >= 8, "a row of sizes at least: {labels:?}");
    for (text, rect, clip) in &labels {
        assert!(
            clip.contains_rect(*rect),
            "size {text} at {rect:?} is cut by {clip:?}"
        );
    }
    // Every row it starts is full (the eight a row the default width
    // gives), so no row was left half drawn.
    let mut rows: Vec<f32> = labels.iter().map(|(_, rect, _)| rect.top()).collect();
    rows.dedup_by(|a, b| (*a - *b).abs() < 1.0);
    assert!(rows.len() >= 2, "two rows of sizes: {labels:?}");
    assert_eq!(labels.len() % rows.len(), 0, "{labels:?}");
}

/// The frames a second and how many frames the AVI `bytes` says it has.
fn avi_timing(bytes: &[u8]) -> (f64, u32) {
    let at = bytes
        .windows(4)
        .position(|four| four == b"avih")
        .expect("an AVI main header");
    let word = |offset: usize| {
        let start = at + 8 + offset;
        u32::from_le_bytes(bytes[start..start + 4].try_into().unwrap())
    };
    (1_000_000.0 / f64::from(word(0)), word(16))
}

#[test]
fn sue_paint_timelapse_fitted_to_thirty_seconds_lasts_thirty_seconds() {
    // Three strokes recorded; Export Timelapse, Fit to specified length
    // of 30 seconds (its default): the video lasts 30 seconds, each
    // stroke held, not the 4 second flash it was.
    let mut paint = PaintDriver::new("paint-polish-timelapse");
    paint.command("file.timelapse.record");
    paint.settle(3);
    for dy in [-40.0, 0.0, 40.0] {
        line_on_canvas(&mut paint, dy);
        paint.settle(20);
    }
    paint.command("file.timelapse.export");
    let start = Instant::now();
    while !paint.texts.iter().any(|(text, _)| text.contains(" px, ")) {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "{:?}",
            paint.texts
        );
        paint.step(Vec::new());
    }
    // The dialog says so before it exports.
    let size = paint
        .texts
        .iter()
        .find(|(text, _)| text.contains(" px, "))
        .map(|(text, _)| text.clone())
        .unwrap();
    assert!(size.ends_with("300 frames at 10 a second"), "{size}");
    assert!(
        paint
            .texts
            .iter()
            .any(|(text, _)| text.ends_with("of 0:30")),
        "{:?}",
        paint.texts
    );
    paint.click_text("OK");
    paint.settle(2);
    std::fs::create_dir_all(&paint.root).unwrap();
    let avi = paint.root.join("Drawing.avi");
    paint.runtime.request(
        ShellRequest::Picked {
            pick: FilePick::save(
                Some(AppletId::new(sue_paint::ID).unwrap()),
                "timelapse",
                "Export",
                "x",
            ),
            path: avi.clone(),
        },
        &mut NoTextures,
        paint.now,
    );
    paint.finish_exports();
    let bytes = std::fs::read(&avi).expect("the timelapse is written");
    let (fps, frames) = avi_timing(&bytes);
    let seconds = f64::from(frames) / fps;
    assert!(
        (seconds - 30.0).abs() < 0.05,
        "{frames} frames at {fps}: {seconds} s"
    );
}

/// The open picture's animation folder track: its keys' frames and cels.
fn cel_keys(paint: &PaintDriver) -> Vec<(u32, sue_core::EntityId)> {
    let arena = &paint.runtime.engine.world.arena;
    let sequence = sue_paint::anim::timeline(arena, open_picture(paint)).expect("a timeline");
    let track = sue_paint::anim::folders(sequence)[0];
    sue_paint::anim::keys(&sequence.tracks[track], sequence.rate)
        .into_iter()
        .filter_map(|(frame, cel)| Some((frame, cel?)))
        .collect()
}

/// Goes to frame `frame` (0 based) as the transport's buttons do.
fn go_to(paint: &mut PaintDriver, frame: u32) {
    paint.command("anim.first");
    for _ in 0..frame {
        paint.command("anim.next");
    }
    paint.settle(3);
}

/// An animation folder with a drawn cel on frames 1, 4 and 7.
fn three_cels(paint: &mut PaintDriver) {
    paint.command("anim.layer.folder");
    for (at, dy) in [(0, -60.0), (3, 0.0), (6, 60.0)] {
        go_to(paint, at);
        paint.command("anim.cel.new");
        paint.settle(2);
        line_on_canvas(paint, dy);
    }
    let keys: Vec<u32> = cel_keys(paint).iter().map(|(frame, _)| *frame).collect();
    assert_eq!(keys, [0, 3, 6]);
}

#[test]
fn sue_paint_show_animation_cels_has_its_five_entries_and_they_work() {
    // ABV-9: Show Edited Cel Only, Check Cel Motion by Key Input and
    // Check Surrounding Cels by Key Input beside the onion skin's two.
    let mut paint = PaintDriver::new("paint-polish-cels");
    three_cels(&mut paint);
    let cels = cel_keys(&paint);
    let (a, b, c) = (cels[0].1, cels[1].1, cels[2].1);
    paint.click_text("Animation");
    paint.click_text("Show Animation Cels");
    let order = [
        "Show Edited Cel Only",
        "Enable Onion Skin",
        "Onion Skin Settings...",
        "Check Cel Motion by Key Input",
        "Check Surrounding Cels by Key Input",
    ];
    let tops: Vec<f32> = order
        .iter()
        .map(|label| {
            paint
                .rect_of(label)
                .unwrap_or_else(|| panic!("{label} in the menu: {:?}", paint.texts))
                .top()
        })
        .collect();
    assert!(tops.windows(2).all(|pair| pair[0] < pair[1]), "{tops:?}");
    paint.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    paint.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    paint.settle(3);

    // Show Edited Cel Only: cel 3 locked as the one drawn while frame 1
    // shows cel 1; with the entry on, the canvas shows cel 3 instead.
    go_to(&mut paint, 6);
    paint.command("anim.cel.lock");
    go_to(&mut paint, 0);
    let seen = sue_paint::last_view().expect("the canvas shows the frame");
    assert_eq!(seen.frame, 0);
    assert!(
        !seen.hidden.contains(&a) && seen.hidden.contains(&c),
        "{seen:?}"
    );
    let arena = arena_bytes(&paint);
    paint.click_text("Animation");
    paint.click_text("Show Animation Cels");
    paint.click_text("Show Edited Cel Only");
    paint.settle(3);
    let seen = sue_paint::last_view().unwrap();
    assert!(
        seen.hidden.contains(&a) && seen.hidden.contains(&b) && !seen.hidden.contains(&c),
        "only the edited cel: {seen:?}"
    );
    assert_eq!(arena_bytes(&paint), arena, "a way of looking, not an edit");
    paint.command("anim.cel.edited_only");
    paint.settle(3);
    let seen = sue_paint::last_view().unwrap();
    assert!(
        !seen.hidden.contains(&a) && seen.hidden.contains(&c),
        "{seen:?}"
    );
    paint.command("anim.cel.lock");

    // Check Cel Motion by Key Input on frame 7 (cel 3): frames 1 to 6
    // play once at the timeline's rate, then frame 7 again; the playhead
    // stays.
    go_to(&mut paint, 6);
    paint.command("anim.cel.check_motion");
    let mut frames = Vec::new();
    for _ in 0..90 {
        paint.step(Vec::new());
        frames.push(sue_paint::last_view().unwrap().frame);
    }
    assert_eq!(frames.first(), Some(&0), "{frames:?}");
    assert!(
        frames.windows(2).all(|pair| pair[0] <= pair[1]),
        "{frames:?}"
    );
    for frame in 0..=6 {
        assert!(frames.contains(&frame), "{frame} in {frames:?}");
    }
    assert_eq!(frames.last(), Some(&6), "back on the playhead's frame");

    // Check Surrounding Cels by Key Input on frame 4 (cel 2): a press
    // shows the cel before, the next the cel after, the next frame 4
    // again.
    go_to(&mut paint, 3);
    let mut shown = Vec::new();
    for _ in 0..3 {
        paint.command("anim.cel.check_surrounding");
        paint.settle(2);
        shown.push(sue_paint::last_view().unwrap().frame);
    }
    assert_eq!(shown, [0, 6, 3]);
    // Moving the playhead ends a look.
    paint.command("anim.cel.check_surrounding");
    paint.settle(2);
    assert_eq!(sue_paint::last_view().unwrap().frame, 0);
    paint.command("anim.next");
    paint.settle(2);
    assert_eq!(sue_paint::last_view().unwrap().frame, 4);
}

/// The whole arena, as bytes.
fn arena_bytes(paint: &PaintDriver) -> Vec<u8> {
    super::paint_anim::arena_bytes(paint)
}

/// The current layer of the open picture: the last one made.
fn newest_layer(paint: &PaintDriver) -> sue_core::EntityId {
    let arena = &paint.runtime.engine.world.arena;
    sue_paint::doc::layers(arena, open_picture(paint))
        .into_iter()
        .last()
        .expect("a layer")
        .id
}

/// The bytes of `id`'s pixels.
fn pixels(paint: &PaintDriver, id: sue_core::EntityId) -> Vec<u8> {
    sue_paint::doc::raster(&paint.runtime.engine.world.arena, id)
        .and_then(|raster| raster.bytes().map(<[u8]>::to_vec))
        .expect("pixels")
}

/// Undo, then redo: the arena is as it was before, then after.
fn undo_pair(paint: &mut PaintDriver, before: &[u8], what: &str) {
    let after = arena_bytes(paint);
    assert_ne!(after, before, "{what} changed the document");
    paint.command("edit.undo");
    paint.settle(3);
    assert_eq!(arena_bytes(paint), before, "{what}: undone exactly");
    paint.command("edit.redo");
    paint.settle(3);
    assert_eq!(arena_bytes(paint), after, "{what}: redone exactly");
}

#[test]
fn sue_paint_cut_on_a_mask_hides_the_selection_and_keeps_the_pixels() {
    // Edit, Cut while the mask is edited: what shows in the selection
    // goes to the clipboard, the mask hides it there, and the layer's own
    // pixels stay (undo brings the mask back, exactly).
    let mut paint = PaintDriver::new("paint-polish-cut-mask");
    line_on_canvas(&mut paint, 0.0);
    let layer = newest_layer(&paint);
    let drawn = pixels(&paint, layer);
    // A new mask is what the tools draw on next.
    paint.command("layer.mask.outside");
    paint.settle(3);
    let mask = sue_paint::doc::mask_of(&paint.runtime.engine.world.arena, layer).expect("a mask");
    assert!(
        pixels(&paint, mask).iter().all(|value| *value == 255),
        "all shown"
    );
    // M picks the rectangle selection: a box over the left of the line.
    paint.step(vec![key(egui::Key::M, egui::Modifiers::NONE)]);
    let middle = paint_middle(&paint.runtime);
    paint.drag(
        middle + vec2(-100.0, -30.0),
        middle + vec2(-20.0, 30.0),
        egui::PointerButton::Primary,
    );
    let before = arena_bytes(&paint);
    let depth = paint.runtime.engine.history.depth();
    paint.command("edit.cut");
    paint.settle(3);
    assert_eq!(paint.runtime.engine.history.depth(), depth + 1, "one step");
    assert_eq!(pixels(&paint, layer), drawn, "the layer's pixels stay");
    let hidden = pixels(&paint, mask);
    let zero = hidden.iter().filter(|value| **value == 0).count();
    let full = hidden.iter().filter(|value| **value == 255).count();
    assert!(zero > 1_000, "the selection is hidden: {zero}");
    assert!(full > hidden.len() / 2, "the rest shows: {full}");
    undo_pair(&mut paint, &before, "Cut on a mask");
    // What was cut pastes as a layer of its own, with the line in it.
    paint.command("select.none");
    paint.command("edit.paste");
    paint.settle(4);
    let pasted = newest_layer(&paint);
    assert_ne!(pasted, layer);
    let ink = pixels(&paint, pasted)
        .chunks(4)
        .filter(|pixel| pixel[3] > 128)
        .count();
    assert!(ink > 20, "the cut part of the line: {ink}");
}

#[test]
fn sue_paint_paints_the_mask_of_a_gradient_layer() {
    // A gradient layer has no pixels of its own to paint, but its mask is
    // gray pixels like any other: with Edit Mask on, the eraser hides,
    // the pen shows and the Gradient tool lays a ramp on the mask, and
    // the layer's gradient stays as it was. Locked, it takes nothing and
    // says so.
    let mut paint = PaintDriver::new("paint-polish-gradient-mask");
    paint.command("layer.new.gradient");
    paint.settle(4);
    let layer = newest_layer(&paint);
    let line = |paint: &PaintDriver| {
        let arena = &paint.runtime.engine.world.arena;
        sue_paint::doc::Gradient::of(arena.get_by_id(layer).unwrap()).line
    };
    let gradient_line = line(&paint);
    // A new mask is what the tools draw on next.
    paint.command("layer.mask.outside");
    paint.settle(3);
    let mask = sue_paint::doc::mask_of(&paint.runtime.engine.world.arena, layer).expect("a mask");
    // The eraser hides.
    paint.command("tool.eraser");
    paint.settle(2);
    let before = arena_bytes(&paint);
    line_on_canvas(&mut paint, 0.0);
    let erased = pixels(&paint, mask)
        .iter()
        .filter(|value| **value < 128)
        .count();
    assert!(erased > 200, "the eraser hid a line: {erased}");
    assert_eq!(line(&paint), gradient_line, "the gradient stays");
    undo_pair(&mut paint, &before, "the eraser on a gradient layer's mask");
    // The Gradient tool lays a ramp on the mask, not a new line on the
    // layer.
    paint.command("tool.pick.gradient");
    paint.settle(2);
    let before = arena_bytes(&paint);
    let middle = paint_middle(&paint.runtime);
    paint.drag(
        middle + vec2(-200.0, 0.0),
        middle + vec2(200.0, 0.0),
        egui::PointerButton::Primary,
    );
    let ramp = pixels(&paint, mask);
    let between = ramp
        .iter()
        .filter(|value| **value > 10 && **value < 245)
        .count();
    assert!(between > 1_000, "a ramp on the mask: {between}");
    assert_eq!(line(&paint), gradient_line, "the gradient stays");
    undo_pair(&mut paint, &before, "a gradient on a gradient layer's mask");
    // Locked: nothing changes, and the notice says why.
    paint.command("layer.toggle.lock");
    paint.settle(2);
    paint.command("tool.eraser");
    paint.settle(2);
    let before = arena_bytes(&paint);
    line_on_canvas(&mut paint, 40.0);
    assert_eq!(
        arena_bytes(&paint),
        before,
        "a locked layer's mask takes nothing"
    );
    assert!(
        paint
            .texts
            .iter()
            .any(|(text, _)| text.ends_with(" is locked")),
        "{:?}",
        paint.texts
    );
}

/// A WinTab packet at `pos` turned `twist` degrees.
fn twisted(kind: PenKind, pos: Pos2, time: f64, twist: f32) -> PenSample {
    let mut sample = PenSample::at(PenSource::WinTab, [pos.x, pos.y], time);
    sample.kind = kind;
    sample.pressure = (kind != PenKind::Up).then_some(0.6);
    sample.twist = Some(twist);
    sample
}

/// A pen stroke to the right on screen from `start`, the pen turned
/// `twist` degrees: the angle the vector line kept for it in the
/// picture, and the line's own way in the picture, in degrees.
fn twisted_stroke(
    paint: &mut PaintDriver,
    layer: sue_core::EntityId,
    start: Pos2,
    twist: f32,
) -> (f32, f32) {
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut time = paint.now.elapsed().as_secs_f64() + 100.0;
    stylus::push_for_test(twisted(PenKind::Down, start, time, twist));
    paint.step(vec![egui::Event::PointerMoved(start), button(start, true)]);
    let mut at = start;
    for _ in 0..12 {
        for _ in 0..4 {
            at += vec2(3.0, 0.0);
            time += 0.004;
            stylus::push_for_test(twisted(PenKind::Move, at, time, twist));
        }
        paint.step(vec![egui::Event::PointerMoved(at)]);
    }
    time += 0.002;
    stylus::push_for_test(twisted(PenKind::Up, at, time, twist));
    paint.step(vec![button(at, false)]);
    paint.settle(3);
    stylus::forget_for_test();
    let lines = sue_paint::vector::lines(&sue_paint::vector::geometry(
        &paint.runtime.engine.world.arena,
        layer,
    ));
    let line = lines.last().expect("the stroke is kept as a line");
    let engine = line.engine.as_ref().expect("on the brush engine");
    let kept = engine
        .pen
        .iter()
        .find_map(|part| part.twist)
        .expect("the pen's twist is kept");
    let (first, last) = (
        line.samples.first().unwrap().at,
        line.samples.last().unwrap().at,
    );
    let way = (last[1] - first[1]).atan2(last[0] - first[0]).to_degrees();
    (kept, way)
}

#[test]
fn sue_paint_the_pens_twist_turns_and_mirrors_with_the_view() {
    // A pen turned 30 degrees, drawing to the right on screen: in the
    // picture its twist is 30 degrees from the line's own way when the
    // view is turned, and 30 degrees the other side of it when the view
    // is mirrored, as the tip lies on screen.
    let mut paint = PaintDriver::new("paint-polish-twist");
    paint.command("layer.new.vector");
    paint.settle(3);
    let layer = newest_layer(&paint);
    paint.command("tool.pick.pen");
    paint.settle(2);
    let middle = paint_middle(&paint.runtime);
    let turned = |twist: f32, way: f32| (twist - way).rem_euclid(360.0);
    let near = |a: f32, b: f32| {
        let off = (a - b).rem_euclid(360.0);
        off.min(360.0 - off) < 1.0
    };
    let (kept, way) = twisted_stroke(&mut paint, layer, middle + vec2(-60.0, -80.0), 30.0);
    assert!(near(way, 0.0), "straight view: the line goes right ({way})");
    assert!(near(kept, 30.0), "straight view: {kept}");
    // Turned a quarter (six steps of 15 degrees).
    for _ in 0..6 {
        paint.command("view.rotate_right");
    }
    paint.settle(3);
    let (kept, way) = twisted_stroke(&mut paint, layer, middle + vec2(-60.0, 0.0), 30.0);
    assert!(
        near(way.abs(), 90.0),
        "the line turned with the view ({way})"
    );
    assert!(
        near(turned(kept, way), 30.0),
        "turned view: {kept} on a line going {way}"
    );
    // Mirrored left to right as well.
    paint.command("view.flip_h");
    paint.settle(3);
    let (kept, way) = twisted_stroke(&mut paint, layer, middle + vec2(-60.0, 80.0), 30.0);
    assert!(
        near(turned(kept, way), 330.0),
        "mirrored view: {kept} on a line going {way}"
    );
}
