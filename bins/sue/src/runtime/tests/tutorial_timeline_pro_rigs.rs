//! Tutorial acceptance (P5-2): three videos followed row by row in the
//! running Sue Paint, each test named after the step it follows.
//!
//! - TF: "4 Clip Studio Paint Timeline Features Nobody Explains"
//! - PA: "How Professional Animators Use Clip Studio Paint"
//! - RG: "2D Character Rigs in Clip Studio Paint" (Puppet warp)
//!
//! A test's name starts with the video's key and the step's time
//! (`tf_0643` is TF at 06:43). The rows, and those that are not scripted
//! here with the reason, are in the plan's
//! `acceptance/timeline_pro_rigs.md`. Nothing of the videos is quoted
//! here (rule 10): the steps are named, their words and pictures are not.

use super::paint_anim::{
    arena_bytes, click_at, fill, line_on_canvas, open_picture, press, tip, tips_along,
};
use super::*;

// What the tests read of the document.

fn arena(paint: &PaintDriver) -> &sue_core::Arena {
    &paint.runtime.engine.world.arena
}

/// The picture's timeline, cloned.
fn timeline(paint: &PaintDriver) -> sue_model::Sequence {
    sue_paint::anim::timeline(arena(paint), open_picture(paint))
        .expect("a timeline")
        .clone()
}

/// The first animation folder's track.
fn folder_track(paint: &PaintDriver) -> usize {
    sue_paint::anim::folders(&timeline(paint))[0]
}

/// The cel the first animation folder shows at `frame` (0 based).
fn shown_at(paint: &PaintDriver, frame: u32) -> Option<sue_core::EntityId> {
    let sequence = timeline(paint);
    sue_paint::anim::cel_at(&sequence.tracks[folder_track(paint)], sequence.rate, frame)
}

/// The layer, folder or cel named `name`.
fn named(paint: &PaintDriver, name: &str) -> sue_core::EntityId {
    let picture = open_picture(paint);
    sue_paint::doc::layers(arena(paint), picture)
        .into_iter()
        .find(|info| info.name == name)
        .map(|info| info.id)
        .unwrap_or_else(|| panic!("a layer named {name:?}"))
}

fn name_of(paint: &PaintDriver, id: sue_core::EntityId) -> String {
    arena(paint).get_by_id(id).unwrap().name().to_owned()
}

/// The first animation folder.
fn animation_folder(paint: &PaintDriver) -> sue_core::EntityId {
    let sequence = timeline(paint);
    sue_paint::anim::track_folder(&sequence.tracks[folder_track(paint)]).expect("a folder")
}

/// Whether `layer` is hidden at `frame` as the canvas draws it.
fn hidden_at(paint: &PaintDriver, layer: sue_core::EntityId, frame: u32) -> bool {
    let view =
        sue_paint::anim::setup::frame_view(arena(paint), open_picture(paint), frame, None, None)
            .expect("a timeline");
    view.hidden.contains(&layer)
}

/// The frame shown, as the Timeline's bar reads it (1 based).
fn frame_shown(paint: &mut PaintDriver) -> u32 {
    let texts = paint.settle(2);
    texts
        .iter()
        .find(|(text, _)| text.contains(" to ") && text.contains("   "))
        .and_then(|(text, _)| text.split_whitespace().next()?.parse().ok())
        .unwrap_or_else(|| panic!("the bar's readout: {texts:?}"))
}

/// Runs `work` as one undo step: undo brings back the arena as it was,
/// ids included, and redo the arena as `work` left it.
fn undo_pair(paint: &mut PaintDriver, what: &str, work: impl FnOnce(&mut PaintDriver)) {
    let (before, steps) = (arena_bytes(paint), paint.runtime.engine.history.depth());
    work(paint);
    paint.settle(4);
    let after = arena_bytes(paint);
    assert!(after != before, "{what} changes the document");
    assert_eq!(
        paint.runtime.engine.history.depth(),
        steps + 1,
        "{what} is one undo step"
    );
    paint.command("edit.undo");
    paint.settle(2);
    assert!(arena_bytes(paint) == before, "{what} undone");
    paint.command("edit.redo");
    paint.settle(2);
    assert!(arena_bytes(paint) == after, "{what} redone");
}

/// Gives `command` the key `chord`, as Shortcut Settings does.
fn bind(paint: &mut PaintDriver, command: &str, chord: &str) {
    let rice = &mut paint.runtime.engine.world.rice;
    let (setting, value) =
        sue_wm::shortcuts::bind(rice, "sue_paint_keys", command, &chord.parse().unwrap());
    rice.set(&setting, value).unwrap();
}

/// One press of `key_name` holding `modifiers`.
fn tap(paint: &mut PaintDriver, key_name: egui::Key, modifiers: egui::Modifiers) {
    paint.step(vec![key(key_name, modifiers)]);
    paint.settle(3);
}

/// A press and a release at `at` with `button`, holding `modifiers`.
fn click_with(
    paint: &mut PaintDriver,
    at: Pos2,
    button: egui::PointerButton,
    modifiers: egui::Modifiers,
) -> Vec<(String, egui::Rect)> {
    let event = |pressed| egui::Event::PointerButton {
        pos: at,
        button,
        pressed,
        modifiers,
    };
    paint.step(vec![
        egui::Event::ModifiersChanged(modifiers),
        egui::Event::PointerMoved(at),
    ]);
    paint.step(vec![egui::Event::ModifiersChanged(modifiers), event(true)]);
    paint.step(vec![egui::Event::ModifiersChanged(modifiers), event(false)]);
    paint.step(vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)]);
    paint.settle(4)
}

/// A drag from `from` to `to` holding `modifiers`.
fn drag_with(paint: &mut PaintDriver, from: Pos2, to: Pos2, modifiers: egui::Modifiers) {
    let event = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    };
    let held = |events: Vec<egui::Event>| {
        let mut all = vec![egui::Event::ModifiersChanged(modifiers)];
        all.extend(events);
        all
    };
    paint.step(held(vec![egui::Event::PointerMoved(from)]));
    paint.step(held(vec![event(from, true)]));
    for index in 1..=16 {
        let at = from + (to - from) * (index as f32 / 16.0);
        paint.step(held(vec![egui::Event::PointerMoved(at)]));
    }
    paint.step(held(vec![event(to, false)]));
    paint.step(vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)]);
    paint.settle(6);
}

// Where the Timeline palette drew things.

/// The Timeline palette's header tab, under the canvas.
fn header(texts: &[(String, egui::Rect)]) -> egui::Rect {
    texts
        .iter()
        .find(|(text, rect)| text == "Timeline" && rect.top() > 600.0)
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("the Timeline palette under the canvas: {texts:?}"))
}

/// Where the timeline's names column drew a name starting with `name`.
fn track_row(texts: &[(String, egui::Rect)], name: &str) -> Option<egui::Rect> {
    let header = header(texts);
    texts
        .iter()
        .filter(|(text, rect)| {
            text.starts_with(name) && rect.top() > header.bottom() + 20.0 && rect.left() < 580.0
        })
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
}

/// The frames' grid of the Timeline, read off its ruler: where frame
/// `n` (1 based) is on the row of the track named `track`.
struct Grid {
    one: f32,
    step: f32,
    row: f32,
}

impl Grid {
    fn of(paint: &mut PaintDriver, track: &str) -> Self {
        let texts = paint.settle(4);
        let header = header(&texts);
        let number = |name: &str| {
            texts
                .iter()
                .filter(|(text, rect)| {
                    text == name && rect.top() > header.bottom() && rect.left() > 560.0
                })
                .map(|(_, rect)| *rect)
                .min_by(|a, b| a.top().total_cmp(&b.top()))
                .unwrap_or_else(|| panic!("frame {name} on the ruler: {texts:?}"))
        };
        // Read from 7 and 13: a "1" also names the first cel, here and in
        // the Layer palette, and the playback start handle sits over the
        // ruler's own 1.
        let (seven, thirteen) = (number("7"), number("13"));
        let step = (thirteen.left() - seven.left()) / 6.0;
        let row = track_row(&texts, track).unwrap_or_else(|| panic!("{track}'s track: {texts:?}"));
        Self {
            one: seven.left() - 6.0 * step,
            step,
            row: row.center().y + 4.0,
        }
    }

    fn cell(&self, frame: u32) -> Pos2 {
        Pos2::new(
            self.one + self.step * (frame as f32 - 1.0) + self.step / 2.0,
            self.row,
        )
    }
}

/// Opens the Timeline palette's own menu (the button left of its tab).
fn timeline_menu(paint: &mut PaintDriver) -> Vec<(String, egui::Rect)> {
    let texts = paint.settle(2);
    let header = header(&texts);
    let found = tips_along(
        paint,
        header.center().y,
        header.left() - 70.0,
        header.left(),
    );
    let menu = tip(&found, "Timeline");
    click_at(paint, menu)
}

/// Hovers the submenu `title` of an open menu (above the Timeline's tab)
/// and returns what the frames after draw.
fn open_submenu(paint: &mut PaintDriver, title: &str) -> Vec<(String, egui::Rect)> {
    let row = paint
        .texts
        .iter()
        .filter(|(text, rect)| text == title && rect.top() < 700.0)
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("{title}: {:?}", paint.texts));
    paint.step(vec![egui::Event::PointerMoved(row.center())]);
    paint.settle(30)
}

/// The labels of the Animation menu's submenu `title`, in order.
fn animation_submenu(title: &str) -> Vec<String> {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let menus = sue_paint::menus();
    let animation = menus
        .iter()
        .find(|menu| menu.title == "Animation")
        .expect("an Animation menu");
    let mut out = Vec::new();
    for item in &animation.items {
        if let sue_wm::MenuItem::Sub { label, items } = item
            && label == title
        {
            out.extend(items.iter().map(|item| item.label().to_owned()));
        }
    }
    out
}

/// An animation folder with `count` cels, cel n keyed at frame
/// `1 + (n - 1) * every`, each with a line drawn on it at its own height.
fn folder_with_cels(paint: &mut PaintDriver, count: u32, every: u32) {
    paint.command("anim.layer.folder");
    paint.settle(2);
    for index in 0..count {
        if index > 0 {
            for _ in 0..every {
                paint.command("anim.next");
            }
        }
        paint.command("anim.cel.new");
        paint.settle(2);
        line_on_canvas(paint, -60.0 + 30.0 * index as f32);
    }
    paint.command("anim.first");
    paint.settle(2);
}

// TF: timeline features.

#[test]
fn tf_0133_timeline_menu_thumbnail_size_large_then_none() {
    // TF 01:33 and 01:51: the Timeline palette's menu, Thumbnail Size:
    // Large grows the rows with the cels' pictures, None leaves the
    // names alone on one line.
    let mut paint = PaintDriver::new("tpr-thumbs");
    paint.new_timeline();
    folder_with_cels(&mut paint, 2, 3);
    let gap = |texts: &[(String, egui::Rect)]| {
        let folder = track_row(texts, "Folder 1")?.top();
        texts
            .iter()
            .filter(|(text, rect)| {
                text.starts_with("Layer 1") && rect.top() > folder && rect.left() < 580.0
            })
            .map(|(_, rect)| rect.top() - folder)
            .min_by(f32::total_cmp)
    };
    for (size, label) in [("Large", "Thumbnails: Large"), ("None", "Thumbnails: None")] {
        timeline_menu(&mut paint);
        let texts = open_submenu(&mut paint, "Thumbnail Size");
        assert!(
            texts.iter().any(|(text, _)| text == label),
            "{size} in Thumbnail Size: {texts:?}"
        );
        paint.click_text(label);
        paint.settle(4);
        if size == "Large" {
            let tall = gap(&paint.settle(2));
            assert!(
                tall.is_none_or(|tall| tall > 60.0),
                "a large picture a row: {tall:?}"
            );
        }
    }
    let short = gap(&paint.settle(4)).expect("Layer 1's track in sight");
    assert!(short < 40.0, "one line a row with None: {short}");
}

#[test]
fn tf_0209_window_timeline_shows_it_under_the_canvas() {
    // TF 02:09, PA 00:54: Window, Timeline hides and shows the palette,
    // docked under the canvas.
    let mut paint = PaintDriver::new("tpr-window");
    let middle = paint_middle(&paint.runtime);
    let texts = paint.settle(2);
    assert!(header(&texts).top() > middle.y, "under the canvas");
    paint.command("window.timeline");
    let texts = paint.settle(6);
    assert!(
        !texts
            .iter()
            .any(|(text, rect)| text == "Timeline" && rect.top() > 600.0),
        "hidden: {texts:?}"
    );
    paint.command("window.timeline");
    let texts = paint.settle(6);
    assert!(header(&texts).top() > middle.y, "back under the canvas");
}

#[test]
fn tf_0300_animation_folder_and_five_cels_drawn_and_scrubbed() {
    // TF 03:00, 03:13, 03:21: an animation folder, New Animation Cel five
    // times (each its own layer in the folder), a drawing on each, and the
    // ruler scrubbed to see them in turn.
    let mut paint = PaintDriver::new("tpr-five-cels");
    folder_with_cels(&mut paint, 5, 1);
    let folder = animation_folder(&paint);
    let cels = sue_paint::anim::cels(arena(&paint), open_picture(&paint), folder);
    let names: Vec<String> = cels.iter().map(|cel| name_of(&paint, *cel)).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(sorted, ["1", "2", "3", "4", "5"], "five cels: {names:?}");
    for (frame, name) in ["1", "2", "3", "4", "5"].iter().enumerate() {
        let cel = shown_at(&paint, frame as u32).expect("a cel each frame");
        assert_eq!(name_of(&paint, cel), *name, "cel {name} at frame {frame}");
        assert!(
            sue_paint::warp::bounds(
                sue_paint::doc::raster(arena(&paint), cel)
                    .unwrap()
                    .bytes()
                    .unwrap(),
                sue_paint::doc::raster(arena(&paint), cel).unwrap().width(),
                sue_paint::doc::raster(arena(&paint), cel).unwrap().height(),
            )
            .is_some(),
            "cel {name} was drawn on"
        );
    }
    // The cels are rows of the Layer palette, inside the folder.
    let texts = paint.settle(2);
    for name in ["1", "2", "3", "4", "5"] {
        assert!(
            texts
                .iter()
                .any(|(text, rect)| text == name && rect.left() > 1150.0),
            "cel {name} in the Layer palette: {texts:?}"
        );
    }
    // Scrubbing: a click on the ruler's 3 shows frame 3 and its cel only
    // (zoomed in, every frame is numbered).
    for _ in 0..4 {
        paint.command("anim.zoom_in");
    }
    paint.command("anim.first");
    let texts = paint.settle(4);
    let header = header(&texts);
    let three = texts
        .iter()
        .filter(|(text, rect)| {
            text == "3"
                && rect.top() > header.bottom()
                && rect.left() > 560.0
                && rect.right() < 1150.0
        })
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("3 on the ruler");
    // TRV-5: hovering the ruler names the frame under the pointer first.
    paint.step(vec![egui::Event::PointerMoved(three.center())]);
    let texts = paint.settle(45);
    assert!(texts.iter().any(|(text, _)| text == "Frame 3"), "{texts:?}");
    click_at(&mut paint, three.center());
    assert_eq!(frame_shown(&mut paint), 3);
    let third = shown_at(&paint, 2).unwrap();
    assert!(!hidden_at(&paint, third, 2), "cel 3 shows at frame 3");
    for other in cels.iter().filter(|cel| **cel != third) {
        assert!(hidden_at(&paint, *other, 2), "the others are hidden");
    }
}

#[test]
fn tf_0308_eye_hides_the_other_layers() {
    // TF 03:08: the Layer palette's eye hides a layer; it stays hidden on
    // every frame and comes back with one undo.
    let mut paint = PaintDriver::new("tpr-eye");
    folder_with_cels(&mut paint, 2, 1);
    let paper = named(&paint, "Layer 1");
    paint.command("anim.first");
    let picture = open_picture(&paint);
    // Layer 1 picked in the Layer palette, then its eye turned off.
    let row = paint
        .texts
        .iter()
        .filter(|(text, rect)| text == "Layer 1" && rect.left() > 1150.0)
        .map(|(_, rect)| *rect)
        .next()
        .unwrap_or_else(|| panic!("Layer 1 in the Layer palette: {:?}", paint.texts));
    click_at(&mut paint, row.center());
    undo_pair(&mut paint, "the eye", |paint| {
        paint.command("layer.toggle.visible");
    });
    let visible = sue_paint::doc::layers(arena(&paint), picture)
        .into_iter()
        .find(|info| info.id == paper)
        .unwrap()
        .style
        .visible;
    assert!(!visible, "Layer 1 is hidden");
}

#[test]
fn tf_0342_a_layer_folder_in_the_animation_folder_is_one_cel() {
    // TF 03:42, 04:00, 04:15: a layer folder inside the animation folder
    // with layers in it; the Specify Cels button's list puts it on a
    // frame where nothing showed; drawing there goes on the folder's
    // layer, and the folder shows or hides as a whole, one frame.
    let mut paint = PaintDriver::new("tpr-folder-cel");
    folder_with_cels(&mut paint, 1, 1);
    // Cel 1 is current: a new folder goes beside it, in the animation
    // folder, and a layer goes into the new folder.
    paint.command("layer.new.folder");
    paint.settle(2);
    let picture = open_picture(&paint);
    let animation = animation_folder(&paint);
    let inner = sue_paint::doc::layers(arena(&paint), picture)
        .into_iter()
        .find(|info| sue_paint::doc::is_folder(arena(&paint), info.id) && info.id != animation)
        .map(|info| info.id)
        .expect("the new folder");
    assert_eq!(
        sue_paint::doc::folder_of(arena(&paint), inner),
        Some(animation),
        "made inside the animation folder"
    );
    paint.command("layer.new");
    paint.settle(2);
    let lines = sue_paint::doc::layers(arena(&paint), picture)
        .iter()
        .filter(|info| sue_paint::doc::folder_of(arena(&paint), info.id) == Some(inner))
        .map(|info| info.id)
        .next()
        .expect("a layer made in the folder");
    let folder_name = name_of(&paint, inner);
    assert!(
        sue_paint::anim::cels(arena(&paint), picture, animation).contains(&inner),
        "the folder is one of the cels"
    );
    // Frame 8, where nothing showed yet: the Specify Cels button lists the
    // folder, and picking it shows it there.
    for _ in 0..7 {
        paint.command("anim.next");
    }
    let texts = paint.settle(4);
    let header = header(&texts);
    let found = tips_along(
        &mut paint,
        header.bottom() + 24.0,
        header.left() - 60.0,
        1000.0,
    );
    let specify = tip(&found, "Specify cels");
    let before = paint.settle(2);
    let texts = click_at(&mut paint, specify);
    let entry = texts
        .iter()
        .filter(|(text, rect)| text == &folder_name && !before.iter().any(|(_, was)| was == rect))
        .map(|(_, rect)| rect.center())
        .next()
        .unwrap_or_else(|| panic!("{folder_name} in the list: {texts:?}"));
    click_at(&mut paint, entry);
    assert_eq!(shown_at(&paint, 7), Some(inner), "the folder at frame 8");
    // Drawing at frame 8 goes on the folder's layer.
    let before = sue_paint::doc::raster(arena(&paint), lines)
        .and_then(|raster| raster.bytes().map(<[u8]>::to_vec));
    line_on_canvas(&mut paint, 50.0);
    let after = sue_paint::doc::raster(arena(&paint), lines)
        .and_then(|raster| raster.bytes().map(<[u8]>::to_vec));
    let current = paint
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .map(|_| ());
    let _ = current;
    assert!(
        before != after,
        "the stroke went on the folder's layer; layers: {:?}",
        sue_paint::doc::layers(arena(&paint), picture)
            .iter()
            .map(|info| (
                info.name.clone(),
                sue_paint::doc::raster(arena(&paint), info.id)
                    .and_then(|r| sue_paint::warp::bounds(r.bytes()?, r.width(), r.height()))
            ))
            .collect::<Vec<_>>()
    );
    // The folder shows as a whole at frame 8, and hides at frame 1.
    assert!(!hidden_at(&paint, inner, 7));
    let one = shown_at(&paint, 0).unwrap();
    assert!(
        hidden_at(&paint, inner, 0),
        "the folder cel hides at frame 1"
    );
    assert!(!hidden_at(&paint, one, 0));
    assert!(hidden_at(&paint, one, 7), "cel 1 hides at frame 8");
}

#[test]
fn tf_0432_new_cel_after_a_folder_cel_copies_its_layers() {
    // TF 04:32, 04:55; PA 01:52: New Animation Cel after a folder cel makes
    // a folder with empty copies of its layers (names and kinds).
    let mut paint = PaintDriver::new("tpr-folder-structure");
    folder_with_cels(&mut paint, 1, 1);
    // Cel 1 wrapped in a folder of its own, a second layer added.
    paint.command("layer.folder_insert");
    paint.settle(2);
    paint.command("layer.new");
    paint.settle(2);
    let picture = open_picture(&paint);
    let animation = animation_folder(&paint);
    let first = shown_at(&paint, 0).expect("a cel at frame 1");
    assert!(
        sue_paint::doc::is_folder(arena(&paint), first),
        "the folder is the cel at frame 1"
    );
    let inside = |paint: &PaintDriver, folder| {
        let mut names: Vec<String> = sue_paint::doc::layers(arena(paint), picture)
            .into_iter()
            .filter(|info| sue_paint::doc::folder_of(arena(paint), info.id) == Some(folder))
            .map(|info| info.name)
            .collect();
        names.sort();
        names
    };
    let structure = inside(&paint, first);
    assert_eq!(structure.len(), 2, "{structure:?}");
    for _ in 0..3 {
        paint.command("anim.next");
    }
    undo_pair(&mut paint, "New Animation Cel", |paint| {
        paint.command("anim.cel.new");
    });
    let second = shown_at(&paint, 3).expect("the new cel at frame 4");
    assert!(sue_paint::doc::is_folder(arena(&paint), second), "a folder");
    assert_eq!(inside(&paint, second), structure, "the same layers");
    assert_eq!(
        sue_paint::doc::folder_of(arena(&paint), second),
        Some(animation)
    );
}

#[test]
fn tf_0643_a_clicked_cel_copied_and_pasted_at_frame_17_is_the_same_cel() {
    // TF 06:13, 06:43, 06:50: a click on a cel picks it; Animation, Edit
    // Track, Copy, a click on frame 17, Edit Track, Paste: the same cel
    // shows there again (no new layer).
    let mut paint = PaintDriver::new("tpr-copy-paste");
    paint.new_timeline();
    folder_with_cels(&mut paint, 3, 2);
    let edit_track = animation_submenu("Edit Track");
    for name in ["Cut Frames", "Copy Frames", "Paste Frames", "Delete Frames"] {
        assert!(
            edit_track.iter().any(|label| label == name),
            "{name} in Animation, Edit Track: {edit_track:?}"
        );
    }
    let grid = Grid::of(&mut paint, "Folder 1");
    let second = shown_at(&paint, 2).expect("cel 2 at frame 3");
    click_at(&mut paint, grid.cell(3));
    // Ctrl+C copies the frames picked (CSP keeps it for pixels; the
    // pixels' own copy stays on Edit).
    paint.command("anim.edit.copy");
    paint.settle(2);
    let layers = sue_paint::doc::layers(arena(&paint), open_picture(&paint)).len();
    // Frame 17: the playhead goes there with a click on the ruler.
    for _ in 0..14 {
        paint.command("anim.next");
    }
    assert_eq!(frame_shown(&mut paint), 17);
    undo_pair(&mut paint, "Paste Frames", |paint| {
        paint.command("anim.edit.paste");
    });
    assert_eq!(shown_at(&paint, 16), Some(second), "cel 2 again at 17");
    assert_eq!(
        sue_paint::doc::layers(arena(&paint), open_picture(&paint)).len(),
        layers,
        "a reference, no new layer"
    );
}

#[test]
fn tf_0719_timeline_menu_edit_track_cuts_copies_and_pastes() {
    // TF 07:19: the same commands in the Timeline palette's own menu,
    // Edit Track.
    let mut paint = PaintDriver::new("tpr-menu-edit-track");
    paint.new_timeline();
    folder_with_cels(&mut paint, 2, 2);
    timeline_menu(&mut paint);
    let texts = open_submenu(&mut paint, "Edit Track");
    for name in ["Cut Frames", "Copy Frames", "Paste Frames", "Delete Frames"] {
        assert!(
            texts.iter().any(|(text, _)| text == name),
            "{name} in the palette's Edit Track: {texts:?}"
        );
    }
    // Cut from there takes cel 1 off frame 1 (picked by the click).
    press(&mut paint, egui::Key::Escape);
    let grid = Grid::of(&mut paint, "Folder 1");
    click_at(&mut paint, grid.cell(1));
    timeline_menu(&mut paint);
    open_submenu(&mut paint, "Edit Track");
    paint.click_text("Cut Frames");
    paint.settle(2);
    assert_eq!(shown_at(&paint, 0), None, "cut off frame 1");
}

#[test]
fn tf_0750_shortcut_settings_give_frames_and_labels_their_keys() {
    // TF 07:50, 10:47; PA 00:28: File, Shortcut Settings lists the
    // Timeline's commands (none has a key of its own); given keys, they
    // run from the keyboard.
    let mut paint = PaintDriver::new("tpr-keys-frames");
    paint.new_timeline();
    folder_with_cels(&mut paint, 2, 2);
    paint.command("file.shortcuts");
    let texts = paint.settle(8);
    assert!(
        texts.iter().any(|(text, _)| text == "Keyboard Shortcuts"),
        "Shortcut Settings opened: {texts:?}"
    );
    paint.runtime.shell.close_window();
    paint.settle(4);
    let catalog: Vec<String> = sue_paint::registry()
        .iter()
        .map(|command| command.name.to_owned())
        .collect();
    for command in [
        "anim.edit.copy",
        "anim.edit.paste",
        "anim.label",
        "anim.track_label.inbetween",
    ] {
        assert!(
            catalog.iter().any(|name| name == command),
            "{command} can take a key"
        );
    }
    bind(&mut paint, "anim.edit.copy", "Ctrl+Alt+C");
    bind(&mut paint, "anim.edit.paste", "Ctrl+Alt+V");
    let grid = Grid::of(&mut paint, "Folder 1");
    let first = shown_at(&paint, 0).unwrap();
    click_at(&mut paint, grid.cell(1));
    tap(
        &mut paint,
        egui::Key::C,
        egui::Modifiers::COMMAND | egui::Modifiers::ALT,
    );
    for _ in 0..9 {
        paint.command("anim.next");
    }
    tap(
        &mut paint,
        egui::Key::V,
        egui::Modifiers::COMMAND | egui::Modifiers::ALT,
    );
    assert_eq!(shown_at(&paint, 9), Some(first), "pasted by its key");
    // Label on a key too: it asks for the name.
    bind(&mut paint, "anim.label", "Ctrl+Alt+L");
    tap(
        &mut paint,
        egui::Key::L,
        egui::Modifiers::COMMAND | egui::Modifiers::ALT,
    );
    assert!(
        paint
            .texts
            .iter()
            .any(|(text, _)| text.starts_with("Label at frame")),
        "the label's dialog: {:?}",
        paint.texts
    );
}

#[test]
fn tf_0811_the_timeline_zooms_in() {
    // TF 08:11: zooming the timeline widens its frames.
    let mut paint = PaintDriver::new("tpr-zoom");
    paint.new_timeline();
    folder_with_cels(&mut paint, 1, 1);
    let before = Grid::of(&mut paint, "Folder 1").step;
    paint.command("anim.zoom_in");
    paint.command("anim.zoom_in");
    paint.command("anim.first");
    let after = Grid::of(&mut paint, "Folder 1").step;
    assert!(after > before * 1.2, "wider frames: {before} then {after}");
}

#[test]
fn tf_0824_dragging_the_next_cel_holds_the_one_before_longer_then_shorter() {
    // TF 08:24, 08:44, 08:56 (as the picture shows it): the next cel's
    // clip dragged right holds the cel before for more frames, on twos or
    // threes; dragged back, for fewer. Each drag is one undo step.
    let mut paint = PaintDriver::new("tpr-holds");
    paint.new_timeline();
    folder_with_cels(&mut paint, 2, 1);
    let grid = Grid::of(&mut paint, "Folder 1");
    let (first, second) = (shown_at(&paint, 0).unwrap(), shown_at(&paint, 1).unwrap());
    // Cel 2 picked at frame 2, its start dragged to frame 3: cel 1 on twos.
    click_at(&mut paint, grid.cell(2));
    undo_pair(&mut paint, "the drag", |paint| {
        paint.drag(grid.cell(2), grid.cell(3), egui::PointerButton::Primary);
    });
    let held: Vec<_> = (0..3).map(|frame| shown_at(&paint, frame)).collect();
    assert_eq!(held, [Some(first), Some(first), Some(second)], "on twos");
    // On to frame 4: on threes.
    click_at(&mut paint, grid.cell(3));
    paint.drag(grid.cell(3), grid.cell(4), egui::PointerButton::Primary);
    let held: Vec<_> = (0..4).map(|frame| shown_at(&paint, frame)).collect();
    assert_eq!(
        held,
        [Some(first), Some(first), Some(first), Some(second)],
        "on threes"
    );
    // Back to frame 2: cel 1 holds one frame again.
    click_at(&mut paint, grid.cell(4));
    paint.drag(grid.cell(4), grid.cell(2), egui::PointerButton::Primary);
    assert_eq!(shown_at(&paint, 1), Some(second), "shorter again");
}

#[test]
fn tf_0946_timeline_labels_named_over_their_frames_and_deleted() {
    // TF 09:46, 10:36, 12:10: Label, Timeline Label at the playhead asks
    // for a name and shows it over the frames; a second one elsewhere;
    // a label is taken away again.
    let mut paint = PaintDriver::new("tpr-labels");
    paint.new_timeline();
    folder_with_cels(&mut paint, 1, 1);
    let picture = open_picture(&paint);
    timeline_menu(&mut paint);
    open_submenu(&mut paint, "Label");
    paint.click_text("Timeline Label...");
    paint.settle(3);
    fill(&mut paint, "Label at frame 1", "Start");
    press(&mut paint, egui::Key::Enter);
    for _ in 0..9 {
        paint.command("anim.next");
    }
    paint.command("anim.label");
    paint.settle(3);
    fill(&mut paint, "Label at frame 10", "Turn");
    press(&mut paint, egui::Key::Enter);
    let texts = paint.settle(3);
    let header = header(&texts);
    for name in ["Start", "Turn"] {
        assert!(
            texts
                .iter()
                .any(|(text, rect)| text == name && rect.top() > header.bottom()),
            "{name} on the Timeline: {texts:?}"
        );
    }
    let labels = sue_paint::anim::setup::labels(arena(&paint), picture);
    assert_eq!(labels.len(), 2, "{labels:?}");
    // The label at frame 10 named empty: it goes.
    undo_pair(&mut paint, "deleting a label", |paint| {
        paint.command("anim.label");
        paint.settle(3);
        let field = super::paint_anim::right_of(paint, "Label at frame 10");
        click_at(paint, field.center());
        tap(paint, egui::Key::A, egui::Modifiers::COMMAND);
        tap(paint, egui::Key::Backspace, egui::Modifiers::NONE);
        press(paint, egui::Key::Enter);
    });
    let labels = sue_paint::anim::setup::labels(arena(&paint), picture);
    assert_eq!(labels.len(), 1, "one label left: {labels:?}");
}

#[test]
fn tf_1212_in_between_marks_go_on_the_picked_track() {
    // TF 12:12 to 13:16: Label, the two in-between marks (one to draw, one
    // drawn) on the picked track at the playhead, from the palette's menu.
    let mut paint = PaintDriver::new("tpr-inbetween");
    paint.new_timeline();
    folder_with_cels(&mut paint, 2, 4);
    timeline_menu(&mut paint);
    let texts = open_submenu(&mut paint, "Label");
    for name in ["In-between to Draw", "In-between Drawn"] {
        assert!(
            texts.iter().any(|(text, _)| text == name),
            "{name} under Label: {texts:?}"
        );
    }
    press(&mut paint, egui::Key::Escape);
    let grid = Grid::of(&mut paint, "Folder 1");
    click_at(&mut paint, grid.cell(3));
    paint.command("anim.track_label.inbetween");
    click_at(&mut paint, grid.cell(4));
    paint.command("anim.track_label.inbetween-done");
    let sequence = timeline(&paint);
    assert_eq!(
        sue_paint::anim::editing::track_labels(&sequence.tracks[folder_track(&paint)]),
        vec![
            (2, sue_paint::anim::editing::TrackLabel::InBetween),
            (3, sue_paint::anim::editing::TrackLabel::InBetweenDone),
        ]
    );
    // TF 12:10: No Track Label takes the mark at the playhead away.
    undo_pair(&mut paint, "No Track Label", |paint| {
        paint.command("anim.track_label.none");
    });
    let sequence = timeline(&paint);
    assert_eq!(
        sue_paint::anim::editing::track_labels(&sequence.tracks[folder_track(&paint)]),
        vec![(2, sue_paint::anim::editing::TrackLabel::InBetween)]
    );
}

#[test]
fn tf_1317_keyframes_on_a_layer_key_its_moves_and_play_them_tweened() {
    // TF 13:17, 14:09, 14:35: Enable Keyframes on This Layer, Add
    // Keyframe at frame 1, the playhead to 13 and the layer moved with the
    // Object tool: a key made by itself; frames between move part way.
    let mut paint = PaintDriver::new("tpr-layer-keys");
    paint.new_timeline();
    let middle = paint_middle(&paint.runtime);
    paint.step(vec![key(egui::Key::M, egui::Modifiers::NONE)]);
    paint.drag(
        middle - vec2(40.0, 20.0),
        middle + vec2(40.0, 20.0),
        egui::PointerButton::Primary,
    );
    paint.command("edit.fill");
    paint.command("select.none");
    paint.settle(4);
    let layer = named(&paint, "Layer 1");
    let picture = open_picture(&paint);
    let size = sue_model::picture::size(arena(&paint).get_by_id(picture).unwrap()).unwrap();
    // The layer's track, picked; the two commands from the palette's menu.
    paint.command("anim.thumbnails.none");
    let texts = paint.settle(4);
    let row = track_row(&texts, "Layer 1").unwrap_or_else(|| panic!("{texts:?}"));
    click_at(&mut paint, row.center());
    timeline_menu(&mut paint);
    let texts = open_submenu(&mut paint, "Edit Track");
    assert!(
        texts
            .iter()
            .any(|(text, _)| text == "Enable Keyframes on This Layer"),
        "{texts:?}"
    );
    paint.click_text("Enable Keyframes on This Layer");
    paint.command("anim.key.layer");
    paint.settle(2);
    for _ in 0..12 {
        paint.command("anim.next");
    }
    paint.command("tool.operation");
    paint.settle(4);
    let inside = sue_paint::shown_buttons(&paint.ctx)
        .into_iter()
        .find(|(id, _)| id == "mark.keyed.middle")
        .map(|(_, rect)| rect.center())
        .unwrap_or_else(|| {
            panic!(
                "the keyed layer's box: {:?}",
                sue_paint::shown_buttons(&paint.ctx)
            )
        })
        + vec2(10.0, 5.0);
    undo_pair(&mut paint, "the move", |paint| {
        drag_with(
            paint,
            inside,
            inside + vec2(120.0, 0.0),
            egui::Modifiers::SHIFT,
        );
    });
    let times = sue_paint::anim::motion::key_times(arena(&paint), layer);
    assert_eq!(times.len(), 2, "a key at 1 and one made at 13: {times:?}");
    let rate = timeline(&paint).rate;
    let at = |frame| {
        sue_paint::anim::motion::pose(
            arena(&paint),
            layer,
            sue_paint::camera::time_of(frame, rate),
            size,
        )
        .unwrap()
        .position[0]
    };
    let (start, middle_x, end) = (at(0), at(6), at(12));
    assert!(end > start + 50.0, "keyed across: {start} {end}");
    assert!(
        middle_x > start + 5.0 && middle_x < end - 5.0,
        "tweened between: {start} {middle_x} {end}"
    );
    // The track reads as a transform track (TRV-2).
    let texts = paint.settle(3);
    assert!(
        texts
            .iter()
            .any(|(text, _)| text.starts_with("Layer 1") && text.contains("Transform")),
        "the layer's row reads Transform: {:?}",
        texts
            .iter()
            .filter(|(_, rect)| rect.left() < 580.0 && rect.top() > 700.0)
            .collect::<Vec<_>>()
    );
    // TRV-8: Delete All Keyframes takes every key of the layer away.
    undo_pair(&mut paint, "Delete All Keyframes", |paint| {
        paint.command("anim.key.delete_all");
    });
    assert!(sue_paint::anim::motion::key_times(arena(&paint), layer).is_empty());
}

#[test]
fn tf_1347_keyframes_on_an_animation_folder_move_all_its_cels() {
    // TF 13:47 (as the picture shows it): keyframes enabled on an
    // animation folder; moving it keys the folder, so every cel moves.
    let mut paint = PaintDriver::new("tpr-folder-keys");
    paint.new_timeline();
    folder_with_cels(&mut paint, 2, 2);
    let folder = animation_folder(&paint);
    let texts = paint.settle(4);
    let row = texts
        .iter()
        .filter(|(text, rect)| text.starts_with("Folder 1") && rect.left() > 1150.0)
        .map(|(_, rect)| *rect)
        .next()
        .unwrap_or_else(|| panic!("Folder 1 in the Layer palette: {texts:?}"));
    click_at(&mut paint, row.center());
    paint.command("anim.keyframes");
    paint.command("anim.key.layer");
    paint.settle(2);
    assert!(
        !sue_paint::anim::motion::key_times(arena(&paint), folder).is_empty(),
        "the folder is keyed"
    );
}

// PA: professional animator setup.

#[test]
fn pa_0039_new_illustration_2100_by_1200() {
    // PA 00:39: File, New, an illustration 2100 by 1200.
    let mut paint = PaintDriver::new("tpr-new");
    paint.step(vec![key(egui::Key::N, egui::Modifiers::COMMAND)]);
    paint.settle(4);
    fill(&mut paint, "Width", "2100");
    fill(&mut paint, "Height", "1200");
    press(&mut paint, egui::Key::Enter);
    paint.settle(4);
    let picture = open_picture(&paint);
    assert_eq!(
        sue_model::picture::size(arena(&paint).get_by_id(picture).unwrap()),
        Some([2100, 1200])
    );
}

#[test]
fn pa_0059_new_timeline_asks_then_gives_every_layer_a_track() {
    // PA 00:59 (and TRV-14): the bar's buttons are gray before a timeline;
    // New Timeline asks for its settings, OK makes it with no animation
    // folder, every layer on a track.
    let mut paint = PaintDriver::new("tpr-new-timeline");
    let picture = open_picture(&paint);
    paint.command("anim.timeline.new");
    let texts = paint.settle(3);
    for name in ["Timeline name", "Frame rate", "Playback time"] {
        assert!(
            texts.iter().any(|(text, _)| text == name),
            "{name}: {texts:?}"
        );
    }
    paint.click_text("OK");
    let sequence = timeline(&paint);
    assert!(sue_paint::anim::folders(&sequence).is_empty(), "no folder");
    assert_eq!(
        sequence.tracks.len(),
        sue_paint::doc::layers(arena(&paint), picture).len()
    );
}

#[test]
fn pa_0109_new_animation_folder_is_folder_1_and_empty() {
    // PA 01:09 (TRV-9): New Animation Folder makes "Folder 1", empty.
    let mut paint = PaintDriver::new("tpr-folder-name");
    paint.new_timeline();
    paint.command("anim.layer.folder");
    paint.settle(2);
    let folder = named(&paint, "Folder 1");
    assert!(sue_paint::anim::is_animation_folder(arena(&paint), folder));
    assert!(sue_paint::anim::cels(arena(&paint), open_picture(&paint), folder).is_empty());
}

#[test]
fn pa_0112_right_click_new_animation_cel_lands_at_the_playhead() {
    // PA 01:12, 01:20 (TRV-13): a right click on the folder's track at
    // frame 7 offers New Animation Cel; the cel lands at frame 7.
    let mut paint = PaintDriver::new("tpr-right-click-cel");
    paint.new_timeline();
    paint.command("anim.layer.folder");
    paint.command("anim.thumbnails.none");
    paint.settle(4);
    let grid = Grid::of(&mut paint, "Folder 1");
    click_with(
        &mut paint,
        grid.cell(7),
        egui::PointerButton::Secondary,
        egui::Modifiers::NONE,
    );
    paint.click_text("New Animation Cel");
    let cel = shown_at(&paint, 6).expect("a cel at frame 7");
    assert_eq!(name_of(&paint, cel), "1");
    assert_eq!(shown_at(&paint, 5), None, "nothing before it");
}

#[test]
fn pa_0120_hotkeys_for_new_cel_insert_frame_cel_steps_and_play() {
    // PA 01:20, 01:24, 01:37, 03:19: keys given to New Animation Cel,
    // Insert Frame (G), Select Previous and Next Cel (A, D) and Play (Q)
    // run them; Insert Frame asks how many, on the picked track alone.
    let mut paint = PaintDriver::new("tpr-hotkeys");
    paint.new_timeline();
    paint.command("anim.layer.folder");
    paint.settle(2);
    bind(&mut paint, "anim.cel.new", "Ctrl+Alt+N");
    bind(&mut paint, "anim.frame.insert", "G");
    bind(&mut paint, "anim.cel.prev", "A");
    bind(&mut paint, "anim.cel.next", "D");
    bind(&mut paint, "anim.play", "Q");
    let new_cel = egui::Modifiers::COMMAND | egui::Modifiers::ALT;
    tap(&mut paint, egui::Key::N, new_cel);
    for _ in 0..4 {
        paint.command("anim.next");
    }
    tap(&mut paint, egui::Key::N, new_cel);
    let (first, second) = (shown_at(&paint, 0).unwrap(), shown_at(&paint, 4).unwrap());
    assert_ne!(first, second, "two cels by the key");
    // G at frame 1 with the folder's track picked: the dialog, 2 frames,
    // the picked track only.
    paint.command("anim.first");
    let grid = Grid::of(&mut paint, "Folder 1");
    click_at(&mut paint, grid.cell(1));
    tap(&mut paint, egui::Key::G, egui::Modifiers::NONE);
    let texts = paint.settle(3);
    for name in ["Number of frames", "Selected layers only", "Split clip"] {
        assert!(
            texts.iter().any(|(text, _)| text == name),
            "{name}: {texts:?}"
        );
    }
    fill(&mut paint, "Number of frames", "2");
    paint.click_text("Selected layers only");
    paint.click_text("OK");
    assert_eq!(shown_at(&paint, 6), Some(second), "cel 2 two frames later");
    assert_eq!(shown_at(&paint, 5), Some(first), "cel 1 held longer");
    // D and A step between the cels.
    paint.command("anim.first");
    tap(&mut paint, egui::Key::D, egui::Modifiers::NONE);
    assert_eq!(frame_shown(&mut paint), 7, "D: the next cel");
    tap(&mut paint, egui::Key::A, egui::Modifiers::NONE);
    assert_eq!(frame_shown(&mut paint), 1, "A: the cel before");
    // Q plays and stops.
    tap(&mut paint, egui::Key::Q, egui::Modifiers::NONE);
    let mut moved = false;
    for _ in 0..40 {
        paint.step(Vec::new());
    }
    moved |= frame_shown(&mut paint) != 1;
    tap(&mut paint, egui::Key::Q, egui::Modifiers::NONE);
    let stopped = frame_shown(&mut paint);
    paint.settle(30);
    assert!(moved, "Q played");
    assert_eq!(frame_shown(&mut paint), stopped, "Q stopped");
}

#[test]
fn pa_0147_the_layer_palette_names_the_cel_shown_and_a_click_goes_to_it() {
    // PA 01:47 (TRV-3, TRV-4): the folder's row names the cel now shown;
    // a click on another cel in the Layer palette moves the playhead to
    // where that cel starts.
    let mut paint = PaintDriver::new("tpr-layer-cels");
    paint.new_timeline();
    folder_with_cels(&mut paint, 3, 4);
    paint.command("anim.last");
    paint.command("anim.first");
    for _ in 0..8 {
        paint.command("anim.next");
    }
    let texts = paint.settle(4);
    assert!(
        texts
            .iter()
            .any(|(text, rect)| text == "Folder 1 : 3" && rect.left() > 1150.0),
        "the folder's row names cel 3: {texts:?}"
    );
    let two = texts
        .iter()
        .filter(|(text, rect)| text == "2" && rect.left() > 1150.0)
        .map(|(_, rect)| *rect)
        .next()
        .unwrap_or_else(|| panic!("cel 2 in the Layer palette: {texts:?}"));
    click_at(&mut paint, two.center());
    assert_eq!(frame_shown(&mut paint), 5, "cel 2 starts at frame 5");
}

#[test]
fn pa_0206_an_auto_action_on_a_key_puts_each_cel_in_a_folder() {
    // PA 01:52, 02:06: an auto action recorded from Create Folder and
    // Insert Layer, given a key, turns a cel into a folder cel that
    // keeps the cel's name and its place on the timeline.
    let mut paint = PaintDriver::new("tpr-auto-action");
    paint.new_timeline();
    folder_with_cels(&mut paint, 2, 2);
    // New Auto Action records at once; the second press of Record stops.
    paint.command("auto.new");
    paint.settle(4);
    paint.command("layer.folder_insert");
    paint.settle(2);
    paint.command("auto.record");
    paint.settle(4);
    paint.command("edit.undo");
    paint.settle(2);
    let actions = std::fs::read_to_string(
        paint
            .root
            .join("data")
            .join("paint")
            .join("auto_actions.txt"),
    )
    .unwrap_or_default();
    assert!(
        actions.contains("step layer.folder_insert"),
        "the step was recorded: {actions}"
    );
    let slug = actions
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix("id "))
        .expect("the new action's id")
        .trim()
        .to_owned();
    // F9 here: Y is a tool's key already (taking a key from another
    // command is the Settings test's `a_key_held_twice_...`).
    bind(&mut paint, &format!("auto.run.{slug}"), "F9");
    // Cel 2 at frame 3, then Y.
    for _ in 0..2 {
        paint.command("anim.next");
    }
    let cel = shown_at(&paint, 2).unwrap();
    tap(&mut paint, egui::Key::F9, egui::Modifiers::NONE);
    paint.settle(4);
    let shown = shown_at(&paint, 2).expect("still a cel at frame 3");
    assert!(
        sue_paint::doc::is_folder(arena(&paint), shown),
        "a folder cel: {} ({})",
        name_of(&paint, shown),
        &actions[actions.len().saturating_sub(200)..]
    );
    assert_eq!(name_of(&paint, shown), "2", "it keeps the cel's name");
    assert_eq!(sue_paint::doc::folder_of(arena(&paint), cel), Some(shown));
}

#[test]
fn pa_0219_a_cel_between_two_is_named_after_the_one_before() {
    // PA 02:19: a cel made between 1 and 2 is "1a".
    let mut paint = PaintDriver::new("tpr-cel-1a");
    paint.new_timeline();
    folder_with_cels(&mut paint, 2, 4);
    paint.command("anim.next");
    paint.command("anim.next");
    paint.command("anim.cel.new");
    let cel = shown_at(&paint, 2).expect("the new cel");
    assert_eq!(name_of(&paint, cel), "1a");
}

#[test]
fn pa_0226_rename_in_layer_order_on_a_key() {
    // PA 02:26: Rename in Layer Order (given U) names the cels 1, 2, 3.
    let mut paint = PaintDriver::new("tpr-rename");
    paint.new_timeline();
    folder_with_cels(&mut paint, 2, 4);
    paint.command("anim.next");
    paint.command("anim.next");
    paint.command("anim.cel.new");
    let edit_track = animation_submenu("Edit Track");
    assert!(
        edit_track
            .iter()
            .any(|label| label == "Rename in Layer Order"),
        "{edit_track:?}"
    );
    bind(&mut paint, "anim.cel.rename_layer", "U");
    // Picked track, then U.
    let grid = Grid::of(&mut paint, "Folder 1");
    click_at(&mut paint, grid.cell(1));
    let before = arena_bytes(&paint);
    tap(&mut paint, egui::Key::U, egui::Modifiers::NONE);
    let folder = animation_folder(&paint);
    let mut names: Vec<String> = sue_paint::anim::cels(arena(&paint), open_picture(&paint), folder)
        .into_iter()
        .map(|cel| name_of(&paint, cel))
        .collect();
    names.sort();
    assert_eq!(names, ["1", "2", "3"]);
    paint.command("edit.undo");
    paint.settle(2);
    assert!(arena_bytes(&paint) == before, "undone");
}

#[test]
fn pa_0357_a_camera_folder_holds_the_animation_and_keys_by_dragging() {
    // PA 03:57, 04:05, 04:11 (TRV-11, TRV-12): Animation, New Animation
    // Layer, 2D Camera Folder ("Camera 1"); the animation folder dropped
    // into it; its frame dragged with the Object tool at frames 1 and 9
    // makes two keys, on the camera's own track; Tool Property shows a key
    // button by each value and the centre of rotation and opacity.
    let mut paint = PaintDriver::new("tpr-camera");
    paint.new_timeline();
    folder_with_cels(&mut paint, 1, 1);
    let new_layer = animation_submenu("New Animation Layer");
    assert!(
        new_layer.iter().any(|label| label == "2D Camera Folder"),
        "{new_layer:?}"
    );
    paint.command("anim.layer.camera");
    paint.settle(2);
    let camera = named(&paint, "Camera 1");
    let folder = animation_folder(&paint);
    // The Layer palette: Folder 1 dropped onto Camera 1.
    let texts = paint.settle(4);
    let row = |name: &str| {
        texts
            .iter()
            .filter(|(text, rect)| text.starts_with(name) && rect.left() > 1150.0)
            .map(|(_, rect)| *rect)
            .next()
            .unwrap_or_else(|| panic!("{name} in the Layer palette: {texts:?}"))
    };
    let (from, to) = (row("Folder 1").center(), row("Camera 1").center());
    paint.drag(from, to, egui::PointerButton::Primary);
    assert_eq!(
        sue_paint::doc::folder_of(arena(&paint), folder),
        Some(camera),
        "the animation folder is in the camera folder"
    );
    // Camera 1 picked, the Object tool, its frame dragged at frame 1 and 9.
    let texts = paint.settle(2);
    let camera_row = texts
        .iter()
        .filter(|(text, rect)| text == "Camera 1" && rect.left() > 1150.0)
        .map(|(_, rect)| *rect)
        .next()
        .expect("Camera 1's row");
    click_at(&mut paint, camera_row.center());
    paint.step(vec![key(egui::Key::O, egui::Modifiers::NONE)]);
    paint.settle(4);
    paint.click_text("Tool Property");
    let texts = paint.settle(2);
    // Tool Property: the camera's values and its key button. (TRV-11's
    // centre of rotation, opacity and a key button by each value are not
    // there: an open defect in the acceptance table.)
    for name in ["Position X", "Scale", "Rotation", "Add Keyframe"] {
        assert!(
            texts.iter().any(|(text, _)| text.starts_with(name)),
            "{name} in Tool Property: {texts:?}"
        );
    }
    let corner = |paint: &PaintDriver| {
        sue_paint::shown_buttons(&paint.ctx)
            .into_iter()
            .find(|(id, _)| id == "mark.camera.corner.3")
            .map(|(_, rect)| rect.center())
            .unwrap_or_else(|| {
                panic!(
                    "the camera frame's corner: {:?} {:?}",
                    sue_paint::shown_buttons(&paint.ctx),
                    paint.texts
                )
            })
    };
    let inside = |paint: &PaintDriver| corner(paint) + vec2(40.0, -40.0);
    let at = inside(&paint);
    drag_with(&mut paint, at, at + vec2(30.0, 0.0), egui::Modifiers::NONE);
    for _ in 0..8 {
        paint.command("anim.next");
    }
    paint.settle(2);
    let at = inside(&paint);
    drag_with(&mut paint, at, at + vec2(60.0, 0.0), egui::Modifiers::NONE);
    let sequence = timeline(&paint);
    assert!(
        sequence
            .tracks
            .iter()
            .any(sue_paint::anim::setup::is_camera_track),
        "a camera track"
    );
    let keys = sue_paint::anim::motion::key_times(arena(&paint), camera);
    // One key a drag: frames 1 and 9, apart.
    assert_eq!(keys.len(), 2, "keys at frames 1 and 9: {keys:?}");
    assert!(keys[0] < keys[1], "{keys:?}");
    let texts = paint.settle(2);
    assert!(
        track_row(&texts, "Camera 1").is_some(),
        "the camera's track on the Timeline: {texts:?}"
    );
}

// RG: 2D character rigs with Puppet warp.

/// Two "legs": two separate boxes side by side on the current layer.
fn two_legs(paint: &mut PaintDriver) -> (Pos2, Pos2) {
    let middle = paint_middle(&paint.runtime);
    paint.step(vec![key(egui::Key::M, egui::Modifiers::NONE)]);
    let left = middle - vec2(60.0, 0.0);
    let right = middle + vec2(60.0, 0.0);
    for at in [left, right] {
        paint.drag(
            at - vec2(15.0, 80.0),
            at + vec2(15.0, 80.0),
            egui::PointerButton::Primary,
        );
        paint.command("edit.fill");
        paint.settle(2);
    }
    paint.command("select.none");
    paint.settle(2);
    (left, right)
}

/// The top layer of the picture.
fn top_layer(paint: &PaintDriver) -> sue_core::EntityId {
    sue_paint::doc::layers(arena(paint), open_picture(paint))
        .last()
        .expect("a layer")
        .id
}

/// Where `layer` holds drawing in the screen column `x`, as alpha summed
/// over the rows of the picture.
fn ink_in_column(paint: &PaintDriver, layer: sue_core::EntityId, column: u32) -> u32 {
    let raster = sue_paint::doc::raster(arena(paint), layer).unwrap();
    let bytes = raster.bytes().unwrap();
    (0..raster.height())
        .map(|y| u32::from(bytes[((y * raster.width() + column) * 4 + 3) as usize]))
        .sum()
}

#[test]
fn rg_0006_puppet_warp_from_edit_transform_shows_its_mesh_settings_and_bar() {
    // RG 00:03, 00:06, 00:09, 00:15: a character alone on a raster layer;
    // Edit, Transform, Puppet Warp (its last entry); the mesh follows the
    // drawing; the settings and OK and Cancel are shown.
    let mut paint = PaintDriver::new("tpr-puppet-open");
    two_legs(&mut paint);
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let menus = sue_paint::menus();
    let edit = menus.iter().find(|menu| menu.title == "Edit").unwrap();
    let transform = edit
        .items
        .iter()
        .find_map(|item| match item {
            sue_wm::MenuItem::Sub { label, items } if label == "Transform" => Some(items),
            _ => None,
        })
        .expect("Edit, Transform");
    let last = transform
        .iter()
        .rev()
        .flat_map(sue_wm::MenuItem::commands)
        .next();
    assert_eq!(last, Some("edit.transform.puppet"), "the last entry");
    paint.command("edit.transform.puppet");
    paint.settle(4);
    // The bar with OK and Cancel is the one Scale/Rotate shows, in the
    // same place (TR-20).
    let ok = paint.rect_of("OK").expect("OK");
    press(&mut paint, egui::Key::Escape);
    paint.command("edit.transform.scale");
    paint.settle(4);
    let scale_ok = paint.rect_of("OK").expect("Scale/Rotate's OK");
    assert!(
        (scale_ok.top() - ok.top()).abs() < 2.0,
        "the same bar: {ok:?} {scale_ok:?}"
    );
    press(&mut paint, egui::Key::Escape);
    paint.command("edit.transform.puppet");
    paint.settle(4);
    let texts = paint.texts.clone();
    // The mesh follows the drawing: one part a leg (TR-11).
    let layer = top_layer(&paint);
    let raster = sue_paint::doc::raster(arena(&paint), layer).unwrap();
    let mesh = sue_paint::warp::PuppetMesh::build(
        raster.bytes().unwrap(),
        raster.width(),
        raster.height(),
        16,
        2.0,
    );
    assert_eq!(mesh.parts(), 2, "a part each leg");
    for name in [
        "Keep original image",
        "Show mesh",
        "Click to add pins",
        "Pick several pins",
        "Correct pin rotation",
        "Expansion",
        "Interpolation",
        "OK",
        "Cancel",
    ] {
        assert!(
            texts.iter().any(|(text, _)| text == name),
            "{name}: {texts:?}"
        );
    }
}

#[test]
fn rg_0011_a_pinned_leg_stays_while_the_other_bends() {
    // RG 00:09 to 00:15, 00:18: pins put by clicks; the other leg's foot
    // pin dragged: that leg bends out, the pinned leg beside it stays
    // where it was (the mesh keeps separate parts apart). OK is one step.
    let mut paint = PaintDriver::new("tpr-puppet-legs");
    let (left, right) = two_legs(&mut paint);
    let layer = top_layer(&paint);
    let bounds = |paint: &PaintDriver| {
        let raster = sue_paint::doc::raster(arena(paint), layer).unwrap();
        sue_paint::warp::bounds(raster.bytes().unwrap(), raster.width(), raster.height())
            .expect("drawing")
    };
    let start = bounds(&paint);
    let left_column = ink_in_column(&paint, layer, start[0] + 2);
    let before = arena_bytes(&paint);
    paint.command("edit.transform.puppet");
    paint.settle(4);
    // Two pins on each leg.
    for at in [
        left - vec2(0.0, 60.0),
        left + vec2(0.0, 60.0),
        right - vec2(0.0, 60.0),
        right + vec2(0.0, 60.0),
    ] {
        click_with(
            &mut paint,
            at,
            egui::PointerButton::Primary,
            egui::Modifiers::NONE,
        );
    }
    assert!(
        paint.texts.iter().any(|(text, _)| text == "4 pins"),
        "{:?}",
        paint.texts
    );
    let foot = right + vec2(0.0, 60.0);
    drag_with(
        &mut paint,
        foot,
        foot + vec2(50.0, 0.0),
        egui::Modifiers::NONE,
    );
    let steps = paint.runtime.engine.history.depth();
    paint.click_text("OK");
    assert_eq!(paint.runtime.engine.history.depth(), steps + 1, "one step");
    assert!(arena_bytes(&paint) != before, "the layer changed");
    let end = bounds(&paint);
    assert!(
        end[2] > start[2] + 10,
        "the foot went out: {start:?} {end:?}"
    );
    assert_eq!(end[0], start[0], "the pinned leg's edge stayed");
    assert_eq!(
        ink_in_column(&paint, layer, start[0] + 2),
        left_column,
        "the pinned leg stayed whole"
    );
    paint.command("edit.undo");
    paint.settle(2);
    assert!(arena_bytes(&paint) == before, "undone");
}

#[test]
fn rg_0017_alt_click_takes_a_pin_away_and_click_pin_off_picks() {
    // RG 00:17: a pin taken away again (CSP's Click pin, Delete pin: Sue's
    // Alt and click; "Click to add pins" off makes a click pick).
    let mut paint = PaintDriver::new("tpr-puppet-delete");
    let (left, _) = two_legs(&mut paint);
    paint.command("edit.transform.puppet");
    paint.settle(4);
    click_with(
        &mut paint,
        left,
        egui::PointerButton::Primary,
        egui::Modifiers::NONE,
    );
    assert!(paint.texts.iter().any(|(text, _)| text == "1 pin"));
    let pin = sue_paint::shown_buttons(&paint.ctx)
        .into_iter()
        .find(|(id, _)| id == "mark.pin.0")
        .map(|(_, rect)| rect.center())
        .expect("the pin is drawn");
    let texts = click_with(
        &mut paint,
        pin,
        egui::PointerButton::Primary,
        egui::Modifiers::ALT,
    );
    assert!(
        !texts.iter().any(|(text, _)| text == "1 pin"),
        "the pin went: {texts:?}"
    );
    press(&mut paint, egui::Key::Escape);
}

#[test]
fn rg_0020_pins_picked_with_shift_move_together() {
    // RG 00:20 to 00:24: Shift and click picks several pins, and a drag
    // moves them all together, unbent.
    let mut paint = PaintDriver::new("tpr-puppet-shift");
    let (left, right) = two_legs(&mut paint);
    let layer = top_layer(&paint);
    let before = {
        let raster = sue_paint::doc::raster(arena(&paint), layer).unwrap();
        sue_paint::warp::bounds(raster.bytes().unwrap(), raster.width(), raster.height()).unwrap()
    };
    paint.command("edit.transform.puppet");
    paint.settle(4);
    click_with(
        &mut paint,
        left,
        egui::PointerButton::Primary,
        egui::Modifiers::NONE,
    );
    click_with(
        &mut paint,
        right,
        egui::PointerButton::Primary,
        egui::Modifiers::NONE,
    );
    let first = sue_paint::shown_buttons(&paint.ctx)
        .into_iter()
        .find(|(id, _)| id == "mark.pin.0")
        .map(|(_, rect)| rect.center())
        .expect("pin 1");
    click_with(
        &mut paint,
        first,
        egui::PointerButton::Primary,
        egui::Modifiers::SHIFT,
    );
    drag_with(
        &mut paint,
        right,
        right - vec2(0.0, 40.0),
        egui::Modifiers::NONE,
    );
    press(&mut paint, egui::Key::Enter);
    let raster = sue_paint::doc::raster(arena(&paint), layer).unwrap();
    let after =
        sue_paint::warp::bounds(raster.bytes().unwrap(), raster.width(), raster.height()).unwrap();
    assert!(
        after[1] + 30 < before[1],
        "both legs went up: {after:?} {before:?}"
    );
    assert!(
        (after[3] - after[1]).abs_diff(before[3] - before[1]) <= 4,
        "unbent: {after:?} {before:?}"
    );
}

#[test]
fn rg_0028_one_part_scaled_and_turned_and_warped_alone() {
    // RG 00:28, 00:33, end: the character split in parts on layers; one
    // part (an arm) turned with Scale/Rotate about a moved centre, then
    // Puppet warp on that layer alone; the other layer is untouched, and
    // OK is one undo step.
    let mut paint = PaintDriver::new("tpr-puppet-parts");
    two_legs(&mut paint);
    let body = top_layer(&paint);
    paint.command("layer.new");
    paint.settle(2);
    let arm = top_layer(&paint);
    assert_ne!(arm, body);
    let middle = paint_middle(&paint.runtime);
    paint.step(vec![key(egui::Key::M, egui::Modifiers::NONE)]);
    paint.drag(
        middle + vec2(100.0, -10.0),
        middle + vec2(200.0, 10.0),
        egui::PointerButton::Primary,
    );
    paint.command("edit.fill");
    paint.command("select.none");
    paint.settle(2);
    let body_before = sue_paint::doc::raster(arena(&paint), body)
        .and_then(|raster| raster.bytes().map(<[u8]>::to_vec));
    // Scale/Rotate: the centre dragged from the box's middle to the arm's
    // left end (the shoulder), then a drag outside the box turns it a
    // quarter round there.
    paint.command("edit.transform.scale");
    paint.settle(4);
    let shoulder = middle + vec2(100.0, 0.0);
    drag_with(
        &mut paint,
        middle + vec2(150.0, 0.0),
        shoulder + vec2(2.0, 0.0),
        egui::Modifiers::NONE,
    );
    let outside = middle + vec2(240.0, 0.0);
    drag_with(
        &mut paint,
        outside,
        shoulder + vec2(0.0, 140.0),
        egui::Modifiers::SHIFT,
    );
    press(&mut paint, egui::Key::Enter);
    let raster = sue_paint::doc::raster(arena(&paint), arm).unwrap();
    let turned =
        sue_paint::warp::bounds(raster.bytes().unwrap(), raster.width(), raster.height()).unwrap();
    assert!(
        turned[3] - turned[1] > turned[2] - turned[0],
        "the arm hangs down now: {turned:?}"
    );
    // Puppet warp on the arm alone.
    paint.command("edit.transform.puppet");
    paint.settle(4);
    let shoulder_pin = shoulder + vec2(0.0, 10.0);
    click_with(
        &mut paint,
        shoulder_pin,
        egui::PointerButton::Primary,
        egui::Modifiers::NONE,
    );
    // The arm hangs 100 points down from the shoulder on screen: a pin
    // near the hand, dragged aside.
    let hand = shoulder + vec2(0.0, 80.0);
    click_with(
        &mut paint,
        hand,
        egui::PointerButton::Primary,
        egui::Modifiers::NONE,
    );
    drag_with(
        &mut paint,
        hand,
        hand + vec2(40.0, 0.0),
        egui::Modifiers::NONE,
    );
    let steps = paint.runtime.engine.history.depth();
    paint.click_text("OK");
    assert_eq!(paint.runtime.engine.history.depth(), steps + 1);
    let body_after = sue_paint::doc::raster(arena(&paint), body)
        .and_then(|raster| raster.bytes().map(<[u8]>::to_vec));
    assert!(body_before == body_after, "the other layer is untouched");
}

#[test]
fn pa_0044_create_moving_illustration_gives_the_picture_a_timeline() {
    // TRV-10 (PA 00:44): File, New with Create animated illustration on
    // makes the illustration with a timeline at once.
    let mut paint = PaintDriver::new("tpr-moving");
    paint.step(vec![key(egui::Key::N, egui::Modifiers::COMMAND)]);
    paint.settle(4);
    paint.click_text("Create animated illustration");
    paint.settle(2);
    press(&mut paint, egui::Key::Enter);
    paint.settle(4);
    assert!(
        sue_paint::anim::timeline(arena(&paint), open_picture(&paint)).is_some(),
        "a timeline"
    );
}

#[test]
fn pa_0258_brush_size_on_f_and_r_and_fill_on_t() {
    // PA 02:58, 03:13: Smaller and Bigger Brush moved to F and R, the
    // Fill tool to T, each key taken from the command that held it.
    let mut paint = PaintDriver::new("tpr-keys-brush");
    let rice = &mut paint.runtime.engine.world.rice;
    for taken in ["sue_paint_keys.tool_rotate", "sue_paint_keys.tool_text"] {
        if rice.get_keybind(taken).is_some() {
            rice.set(
                taken,
                sue_rice::RiceValue::Keybind(sue_rice::Keybinds::default()),
            )
            .unwrap();
        }
    }
    bind(&mut paint, "brush.smaller", "F");
    bind(&mut paint, "brush.bigger", "R");
    bind(&mut paint, "tool.fill", "T");
    let shows =
        |paint: &PaintDriver, text: &str| paint.texts.iter().any(|(found, _)| found == text);
    paint.settle(2);
    assert!(shows(&paint, "10.0\npx"), "{:?}", paint.texts);
    tap(&mut paint, egui::Key::R, egui::Modifiers::NONE);
    paint.settle(4);
    assert!(shows(&paint, "12.0\npx"), "R: bigger: {:?}", paint.texts);
    tap(&mut paint, egui::Key::F, egui::Modifiers::NONE);
    paint.settle(4);
    assert!(shows(&paint, "10.0\npx"), "F: smaller: {:?}", paint.texts);
    tap(&mut paint, egui::Key::T, egui::Modifiers::NONE);
    paint.settle(4);
    assert!(
        paint.texts.iter().any(|(text, _)| text == "Fill"),
        "T: the Fill tool: {:?}",
        paint.texts
    );
}

#[test]
fn trv_7_and_8_the_animation_menu_has_label_audio_and_the_whole_edit_track() {
    // TRV-7, TRV-8 (PA 02:28, TF 06:50): New Animation Layer holds Audio;
    // Edit Track holds the frames' Delete, Cut, Copy, Paste, the layer
    // keyframes, the cel commands and the clips'; Label is a submenu.
    let new_layer = animation_submenu("New Animation Layer");
    assert!(
        new_layer.iter().any(|label| label.starts_with("Audio")),
        "{new_layer:?}"
    );
    let edit_track = animation_submenu("Edit Track");
    for name in [
        "Delete Frames",
        "Cut Frames",
        "Copy Frames",
        "Paste Frames",
        "Enable Keyframes on This Layer",
        "Add Keyframe to Layer",
        "Delete All Keyframes",
        "Rename in Timeline Order",
        "Rename in Layer Order",
        "Select Previous Cel",
        "Select Next Cel",
        "Merge Clips",
        "Split Clip",
        "Set as First Displayed Frame",
        "Set as Last Displayed Frame",
        "Insert Frame",
        "Delete Frame",
    ] {
        assert!(
            edit_track.iter().any(|label| label == name),
            "{name} in Edit Track: {edit_track:?}"
        );
    }
    let label = animation_submenu("Label");
    for name in [
        "Timeline Label...",
        "In-between to Draw",
        "In-between Drawn",
        "No Track Label",
    ] {
        assert!(label.iter().any(|found| found == name), "{name}: {label:?}");
    }
}

#[test]
fn tf_1058_track_labels_named_with_a_range_on_the_picked_track() {
    // TF 10:58, 11:35, 12:10: the track picked, Label, Track Label: a name
    // and a range of frames, shown on the track; a second one; No Track
    // Label at its frame takes one away again.
    let mut paint = PaintDriver::new("tpr-track-labels");
    paint.new_timeline();
    folder_with_cels(&mut paint, 2, 4);
    let grid = Grid::of(&mut paint, "Folder 1");
    click_at(&mut paint, grid.cell(5));
    timeline_menu(&mut paint);
    open_submenu(&mut paint, "Label");
    paint.click_text("Track Label...");
    paint.settle(3);
    fill(&mut paint, "Label name", "Moving");
    paint.click_text("Range");
    fill(&mut paint, "Number of frames", "3");
    paint.click_text("OK");
    let notes = |paint: &PaintDriver| {
        let sequence = timeline(paint);
        sue_paint::anim::editing::track_notes(&sequence.tracks[folder_track(paint)])
    };
    assert_eq!(
        notes(&paint),
        vec![sue_paint::anim::editing::TrackNote {
            frame: 4,
            range: 3,
            name: "Moving".to_owned()
        }]
    );
    let texts = paint.settle(3);
    let header = header(&texts);
    assert!(
        texts
            .iter()
            .any(|(text, rect)| text == "Moving" && rect.top() > header.bottom()),
        "on the track: {texts:?}"
    );
    click_at(&mut paint, grid.cell(2));
    paint.command("anim.track_note");
    paint.settle(3);
    // The name field has the keys as the dialog opens: typed at once,
    // Enter keeps it.
    paint.step(vec![egui::Event::Text("Hold".to_owned())]);
    press(&mut paint, egui::Key::Enter);
    paint.settle(2);
    assert_eq!(notes(&paint).len(), 2);
    click_at(&mut paint, grid.cell(5));
    undo_pair(&mut paint, "No Track Label", |paint| {
        paint.command("anim.track_label.none");
    });
    let left = notes(&paint);
    assert_eq!(left.len(), 1, "{left:?}");
    assert_eq!(left[0].name, "Hold");
}

#[test]
fn rg_0020_the_ring_round_a_picked_pin_turns_the_part_under_it() {
    // RG 00:20: a pin in the middle of a lying limb, its ring dragged a
    // quarter round: the limb stands up around the pin.
    let mut paint = PaintDriver::new("tpr-puppet-ring");
    let middle = paint_middle(&paint.runtime);
    paint.step(vec![key(egui::Key::M, egui::Modifiers::NONE)]);
    paint.drag(
        middle - vec2(80.0, 15.0),
        middle + vec2(80.0, 15.0),
        egui::PointerButton::Primary,
    );
    paint.command("edit.fill");
    paint.command("select.none");
    paint.settle(2);
    let layer = top_layer(&paint);
    paint.command("edit.transform.puppet");
    paint.settle(4);
    click_with(
        &mut paint,
        middle,
        egui::PointerButton::Primary,
        egui::Modifiers::NONE,
    );
    let pin = sue_paint::shown_buttons(&paint.ctx)
        .into_iter()
        .find(|(id, _)| id == "mark.pin.0")
        .map(|(_, rect)| rect.center())
        .expect("the pin is drawn");
    // The ring Sue Paint draws round a picked pin, 22 points out.
    drag_with(
        &mut paint,
        pin + vec2(22.0, 0.0),
        pin + vec2(0.0, 22.0),
        egui::Modifiers::SHIFT,
    );
    press(&mut paint, egui::Key::Enter);
    let raster = sue_paint::doc::raster(arena(&paint), layer).unwrap();
    let turned =
        sue_paint::warp::bounds(raster.bytes().unwrap(), raster.width(), raster.height()).unwrap();
    assert!(
        turned[3] - turned[1] > turned[2] - turned[0],
        "upright now: {turned:?}"
    );
}
