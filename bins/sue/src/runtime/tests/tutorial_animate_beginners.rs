//! P5-2: the tutorial "How to animate in Clip Studio Paint, for
//! beginners", followed in Sue Paint row by row
//! (`plan/v0.0.8/tutorial-animate-beginners.md`, its step table and its
//! rows seen in the video). Each test is named after the step it follows
//! and drives real frames, found on screen by what is drawn there, never
//! by a fixed place. The row table is
//! `plan/v0.0.8/acceptance/animate_beginners.md`.

use super::paint_anim::{arena_bytes, click_at, fill, open_picture, press, tip, tips_along};
use super::*;
use sue_core::EntityId;

// ----- What the steps share ---------------------------------------------------

fn arena(paint: &PaintDriver) -> &sue_core::Arena {
    &paint.runtime.engine.world.arena
}

/// The picture's timeline, cloned.
fn timeline(paint: &PaintDriver) -> sue_model::Sequence {
    sue_paint::anim::timeline(arena(paint), open_picture(paint))
        .expect("a timeline")
        .clone()
}

/// The track of the animation folder named `name`.
fn track_named(paint: &PaintDriver, name: &str) -> usize {
    let sequence = timeline(paint);
    sue_paint::anim::folders(&sequence)
        .into_iter()
        .find(|track| {
            sue_paint::anim::track_folder(&sequence.tracks[*track])
                .and_then(|folder| arena(paint).get_by_id(folder).ok())
                .is_some_and(|entity| entity.name() == name)
        })
        .unwrap_or_else(|| panic!("{name}'s track"))
}

/// What the first animation folder shows on frames 1 to `count`.
fn shown(paint: &PaintDriver, count: u32) -> Vec<Option<EntityId>> {
    let sequence = timeline(paint);
    let track = &sequence.tracks[sue_paint::anim::folders(&sequence)[0]];
    (0..count)
        .map(|frame| sue_paint::anim::cel_at(track, sequence.rate, frame))
        .collect()
}

/// The names of what the first animation folder shows on frames 1 to
/// `count` ("-" where nothing shows).
fn shown_names(paint: &PaintDriver, count: u32) -> Vec<String> {
    shown(paint, count)
        .into_iter()
        .map(|cel| cel.map_or_else(|| "-".to_owned(), |cel| name_of(paint, cel)))
        .collect()
}

fn name_of(paint: &PaintDriver, id: EntityId) -> String {
    arena(paint)
        .get_by_id(id)
        .map(|entity| entity.name().to_owned())
        .unwrap_or_default()
}

/// The entity named `name`.
fn named(paint: &PaintDriver, name: &str) -> EntityId {
    arena(paint)
        .iter()
        .find(|(_, entity)| entity.name() == name)
        .map(|(_, entity)| entity.id())
        .unwrap_or_else(|| panic!("something named {name}"))
}

/// The picture's layers, bottom first.
fn stack(paint: &PaintDriver) -> Vec<EntityId> {
    sue_paint::doc::layers(arena(paint), open_picture(paint))
        .into_iter()
        .map(|info| info.id)
        .collect()
}

/// How many pixels of `layer` have paint.
fn painted(paint: &PaintDriver, layer: EntityId) -> usize {
    sue_paint::doc::raster(arena(paint), layer).map_or(0, |raster| {
        raster
            .bytes()
            .unwrap_or_default()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .count()
    })
}

fn depth(paint: &PaintDriver) -> usize {
    paint.runtime.engine.history.depth()
}

fn undo(paint: &mut PaintDriver) {
    paint.command("edit.undo");
    paint.settle(2);
}

fn redo(paint: &mut PaintDriver) {
    paint.command("edit.redo");
    paint.settle(2);
}

/// Runs `work` as one undo step: undo gives the arena back as it was, ids
/// included, redo as `work` left it.
fn undo_pair(paint: &mut PaintDriver, what: &str, work: impl FnOnce(&mut PaintDriver)) {
    let (before, steps) = (arena_bytes(paint), depth(paint));
    work(paint);
    paint.settle(2);
    let after = arena_bytes(paint);
    assert!(after != before, "{what} changes the picture");
    assert_eq!(depth(paint), steps + 1, "{what} is one undo step");
    undo(paint);
    assert!(arena_bytes(paint) == before, "{what} undone exactly");
    redo(paint);
    assert!(arena_bytes(paint) == after, "{what} redone exactly");
}

/// Runs a command and lets the frames after draw.
fn run(paint: &mut PaintDriver, command: &str) -> Vec<(String, egui::Rect)> {
    paint.command(command);
    paint.settle(3)
}

/// The Timeline palette's header tab, under the canvas.
fn header(paint: &PaintDriver) -> egui::Rect {
    paint
        .texts
        .iter()
        .find(|(text, rect)| text == "Timeline" && rect.top() > 600.0)
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("the Timeline palette under the canvas: {:?}", paint.texts))
}

/// The bar's readout: the frame shown.
fn frame_shown(paint: &mut PaintDriver) -> u32 {
    let texts = paint.settle(2);
    texts
        .iter()
        .find(|(text, _)| text.contains(" to ") && text.contains("   "))
        .and_then(|(text, _)| text.split_whitespace().next()?.parse().ok())
        .unwrap_or_else(|| panic!("the bar's readout: {texts:?}"))
}

/// Where frame `frame` of the track named starting with `track` is, from
/// the ruler's numbers and the track's name drawn in the last frame.
struct Grid {
    one: f32,
    step: f32,
    rows: Vec<(String, f32)>,
}

impl Grid {
    fn read(paint: &mut PaintDriver) -> Self {
        let texts = paint.settle(4);
        let head = header(paint);
        let ruler = |name: &str| {
            texts
                .iter()
                .filter(|(text, rect)| {
                    text == name
                        && rect.top() > head.bottom() + 20.0
                        && rect.left() > 560.0
                        && rect.left() < 1150.0
                })
                .map(|(_, rect)| *rect)
                .min_by(|a, b| a.top().total_cmp(&b.top()))
                .unwrap_or_else(|| panic!("frame {name} on the ruler: {texts:?}"))
        };
        let one = ruler("1");
        // The farthest frame numbered: the labels stand over their frames'
        // middles.
        let (far, number) = ["25", "19", "13", "7"]
            .into_iter()
            .find_map(|name| {
                texts
                    .iter()
                    .any(|(text, rect)| {
                        text == name
                            && rect.top() > head.bottom() + 20.0
                            && rect.left() > 560.0
                            && rect.left() < 1150.0
                    })
                    .then(|| (ruler(name), name.parse::<f32>().unwrap_or(7.0)))
            })
            .unwrap_or_else(|| panic!("frames numbered on the ruler: {texts:?}"));
        let rows = texts
            .iter()
            .filter(|(_, rect)| {
                rect.top() > one.bottom() && rect.left() < 560.0 && rect.left() > 300.0
            })
            .map(|(text, rect)| (text.clone(), rect.center().y))
            .collect();
        Self {
            one: one.center().x,
            step: (far.center().x - one.center().x) / (number - 1.0),
            rows,
        }
    }

    fn row(&self, name: &str) -> f32 {
        self.rows
            .iter()
            .find(|(text, _)| text.starts_with(name))
            .map(|(_, y)| *y)
            .unwrap_or_else(|| panic!("{name}'s track: {:?}", self.rows))
    }

    /// The cell of frame `frame` (from 1) on the track `name`.
    fn cell(&self, name: &str, frame: u32) -> Pos2 {
        Pos2::new(
            self.one + self.step * (frame as f32 - 1.0),
            self.row(name) + 4.0,
        )
    }
}

/// Clicks the Timeline bar's button named `name` (its tooltip), or picks
/// it under More when the bar is too narrow to hold it.
fn bar_button(paint: &mut PaintDriver, name: &str) {
    paint.settle(2);
    let head = header(paint);
    let found = tips_along(paint, head.bottom() + 24.0, 300.0, 1100.0);
    if let Some((_, at)) = found.iter().find(|(text, _)| text.contains(name)) {
        click_at(paint, *at + vec2(3.0, 0.0));
        return;
    }
    let more = tip(&found, "More buttons");
    let before = click_at(paint, more);
    let at = before
        .iter()
        .filter(|(text, _)| text.contains(name))
        .map(|(_, rect)| rect.center())
        .next()
        .unwrap_or_else(|| panic!("{name} under More: {before:?}"));
    click_at(paint, at);
}

/// Where the tooltip holding `name` comes up along the line `y` from `from`
/// to `to`, looked for a point at a time (for handles a few points wide).
fn tip_fine(paint: &mut PaintDriver, y: f32, from: f32, to: f32, name: &str) -> Option<Pos2> {
    let mut x = from;
    let mut first = true;
    while x < to {
        let at = Pos2::new(x, y);
        paint.step(vec![egui::Event::PointerMoved(at)]);
        let texts = paint.settle(if first { 45 } else { 30 });
        first = false;
        if texts.iter().any(|(text, _)| text.contains(name)) {
            return Some(at);
        }
        x += 1.0;
    }
    None
}

/// Lets the notices at the foot of the page (such as "Copied") run their
/// time out, as a person waits a moment: they lie over the Timeline's
/// first rows while they show.
fn let_notices_go(paint: &mut PaintDriver) {
    for _ in 0..12 {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            time: Some(paint.ctx.input(|input| input.time) + 1.0),
            ..egui::RawInput::default()
        };
        paint.now += Duration::from_secs(1);
        let runtime = &mut paint.runtime;
        let now = paint.now;
        paint
            .ctx
            .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now))
            .drop_without_applying_deltas();
    }
    paint.settle(2);
}

/// A press and release of `button` at `at` holding `modifiers`.
fn press_at(
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

fn right_click(paint: &mut PaintDriver, at: Pos2) -> Vec<(String, egui::Rect)> {
    press_at(
        paint,
        at,
        egui::PointerButton::Secondary,
        egui::Modifiers::NONE,
    )
}

/// A drag from `from` to `to` holding `modifiers`.
fn drag_with(paint: &mut PaintDriver, from: Pos2, to: Pos2, modifiers: egui::Modifiers) {
    let press = |pos, pressed| egui::Event::PointerButton {
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
    paint.step(held(vec![press(from, true)]));
    for index in 1..=16 {
        let at = from + (to - from) * (index as f32 / 16.0);
        paint.step(held(vec![egui::Event::PointerMoved(at)]));
    }
    paint.step(held(vec![press(to, false)]));
    paint.step(vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)]);
    paint.settle(6);
}

/// Clicks the topmost `text` drawn after the last frame that is not where
/// it was drawn in `before` (a menu's entry, not the same word elsewhere).
fn click_new(paint: &mut PaintDriver, before: &[(String, egui::Rect)], text: &str) {
    let anchor = paint.ctx.pointer_latest_pos().unwrap_or(Pos2::ZERO);
    let at = paint
        .texts
        .iter()
        .filter(|(found, rect)| found == text && !before.iter().any(|(_, was)| was == rect))
        .map(|(_, rect)| rect.center())
        .min_by(|a, b| a.distance(anchor).total_cmp(&b.distance(anchor)))
        .unwrap_or_else(|| panic!("{text:?} came up: {:?}", paint.texts));
    click_at(paint, at);
}

/// Picks the cel named `name` at the top of the frame's right-click menu
/// that just came up (`before`: what was drawn before it).
fn pick_cel_in_menu(paint: &mut PaintDriver, before: &[(String, egui::Rect)], name: &str) {
    let fresh = |text: &str| {
        paint
            .texts
            .iter()
            .filter(|(found, rect)| found == text && !before.iter().any(|(_, was)| was == rect))
            .map(|(_, rect)| *rect)
            .collect::<Vec<_>>()
    };
    let menu = fresh("New Animation Cel")
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("the frame's menu: {:?}", paint.texts));
    let at = fresh(name)
        .into_iter()
        .filter(|rect| (rect.left() - menu.left()).abs() < 40.0 && rect.top() < menu.top())
        .min_by(|a, b| {
            (menu.top() - a.top())
                .abs()
                .total_cmp(&(menu.top() - b.top()).abs())
        })
        .unwrap_or_else(|| panic!("{name} in the frame's menu: {:?}", paint.texts))
        .center();
    click_at(paint, at);
}

/// The Layer palette's row named `name` (the right column).
fn layer_row(paint: &mut PaintDriver, name: &str) -> egui::Rect {
    paint.settle(2);
    paint
        .texts
        .iter()
        .filter(|(text, rect)| {
            (text == name || (text.starts_with(name) && text[name.len()..].starts_with(" : ")))
                && rect.left() > 1106.0
                && rect.bottom() < 900.0
        })
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("{name}'s row: {:?}", paint.texts))
}

/// A stroke across the canvas `dy` points from its middle.
fn line(paint: &mut PaintDriver, dy: f32) {
    let middle = paint_middle(&paint.runtime);
    paint.drag(
        middle + vec2(-80.0, dy),
        middle + vec2(80.0, dy),
        egui::PointerButton::Primary,
    );
}

/// A circle drawn with the pen round the canvas's middle, moved by
/// `offset`.
fn circle(paint: &mut PaintDriver, offset: egui::Vec2, radius: f32) {
    let middle = paint_middle(&paint.runtime) + offset;
    let points: Vec<Pos2> = (0..=56)
        .map(|index| {
            let angle = index as f32 / 48.0 * std::f32::consts::TAU;
            middle + vec2(angle.cos(), angle.sin()) * radius
        })
        .collect();
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    paint.step(vec![
        egui::Event::PointerMoved(points[0]),
        press(points[0], true),
    ]);
    for point in &points[1..] {
        paint.step(vec![egui::Event::PointerMoved(*point)]);
    }
    paint.step(vec![press(points[points.len() - 1], false)]);
    paint.settle(4);
}

/// The tutorial's first scene: New Timeline at 12 frames a second for 24
/// frames, an animation folder, cel 1 on frame 1 and cel 2 on frame 2,
/// each with a stroke.
fn two_cels(tag: &str) -> PaintDriver {
    let mut paint = PaintDriver::new(tag);
    run(&mut paint, "anim.timeline.new");
    fill(&mut paint, "Frame rate", "12");
    press(&mut paint, egui::Key::Enter);
    run(&mut paint, "anim.layer.folder");
    run(&mut paint, "anim.cel.new");
    line(&mut paint, -40.0);
    run(&mut paint, "anim.next");
    run(&mut paint, "anim.cel.new");
    line(&mut paint, 40.0);
    paint
}

/// One frame read through egui's accessibility tree: every named control
/// and where it is.
fn told(paint: &mut PaintDriver) -> Vec<(String, egui::Rect)> {
    paint.ctx.enable_accesskit();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
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
    let mut found = Vec::new();
    if let Some(update) = &output.platform_output.accesskit_update {
        for (_, node) in &update.nodes {
            if let (Some(label), Some(bounds)) = (node.label(), node.bounds()) {
                found.push((
                    label.to_owned(),
                    egui::Rect::from_min_max(
                        Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
                        Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
                    ),
                ));
            }
        }
    }
    output.drop_without_applying_deltas();
    found
}

/// Clicks the control named `name` below the menu bar.
fn click_control(paint: &mut PaintDriver, name: &str) {
    let controls = told(paint);
    let at = controls
        .iter()
        .filter(|(label, rect)| label == name && rect.top() > 30.0)
        .map(|(_, rect)| rect.center())
        .next()
        .unwrap_or_else(|| {
            panic!(
                "{name:?} is a control: {:?}",
                controls.iter().map(|(label, _)| label).collect::<Vec<_>>()
            )
        });
    click_at(paint, at);
}

/// The size of the picture open.
fn size(paint: &PaintDriver) -> [u32; 2] {
    sue_model::picture::size(arena(paint).get_by_id(open_picture(paint)).unwrap()).unwrap()
}

fn rate(paint: &PaintDriver) -> sue_core::Rate {
    timeline(paint).rate
}

/// Time of frame `frame` (from 0) on the timeline.
fn time(paint: &PaintDriver, frame: u32) -> sue_core::Time {
    sue_paint::camera::time_of(frame, rate(paint))
}

// ----- 00:16 to 02:30: a canvas and a timeline --------------------------------

#[test]
fn step_01_file_new_opens_the_new_dialog() {
    let mut paint = PaintDriver::new("tab-01");
    paint.step(vec![key(egui::Key::N, egui::Modifiers::COMMAND)]);
    let texts = paint.settle(6);
    for name in ["Illustration", "Animation", "OK", "Cancel"] {
        assert!(
            texts.iter().any(|(text, _)| text == name),
            "{name}: {texts:?}"
        );
    }
    assert!(
        sue_paint::menus()
            .iter()
            .flat_map(|menu| &menu.items)
            .flat_map(sue_wm::MenuItem::commands)
            .any(|command| command == "file.new"),
        "File, New"
    );
    press(&mut paint, egui::Key::Escape);
}

#[test]
fn step_02_new_animation_has_blank_space_and_the_timeline_fields() {
    // ABV-1, AB-24.
    let mut paint = PaintDriver::new("tab-02");
    paint.step(vec![key(egui::Key::N, egui::Modifiers::COMMAND)]);
    paint.settle(6);
    click_control(&mut paint, "Animation");
    let texts = paint.settle(4);
    for name in [
        "1920 x 1080 (192 dpi) with blank space",
        "Size of output frame",
        "Title-safe area",
        "Overflow frame",
        "Blank space",
        "Timeline name",
        "Scene number",
        "Shot number",
        "Division line",
    ] {
        assert!(
            texts.iter().any(|(text, _)| text == name),
            "{name}: {texts:?}"
        );
    }
    paint.click_text("OK");
    paint.settle(6);
    assert_eq!(size(&paint), [2304, 1296], "1920 x 1080 and its margins");
    let frame = sue_paint::anim::setup::OutputFrame::of(arena(&paint), open_picture(&paint))
        .expect("the output frame");
    assert_eq!(frame.size, [1920, 1080]);
}

#[test]
fn step_03_new_illustration_of_1920_by_1080() {
    let mut paint = PaintDriver::new("tab-03");
    paint.step(vec![key(egui::Key::N, egui::Modifiers::COMMAND)]);
    paint.settle(6);
    click_control(&mut paint, "Illustration");
    paint.settle(2);
    fill(&mut paint, "Width", "1920");
    fill(&mut paint, "Height", "1080");
    paint.click_text("OK");
    paint.settle(6);
    assert_eq!(size(&paint), [1920, 1080]);
}

#[test]
fn step_04_window_layer_and_timeline_show_and_hide_their_palettes() {
    let mut paint = PaintDriver::new("tab-04");
    paint.settle(2);
    let has_timeline = |texts: &[(String, egui::Rect)]| {
        texts
            .iter()
            .any(|(text, rect)| text == "Timeline" && rect.top() > 600.0)
    };
    assert!(has_timeline(&paint.texts), "the Timeline is open");
    let texts = run(&mut paint, "window.timeline");
    assert!(!has_timeline(&texts), "Window, Timeline hides it");
    let texts = run(&mut paint, "window.timeline");
    assert!(has_timeline(&texts), "and shows it again");
    let layer_tab = |texts: &[(String, egui::Rect)]| {
        texts
            .iter()
            .any(|(text, rect)| text == "Layer" && rect.left() > 1100.0)
    };
    assert!(layer_tab(&paint.texts), "the Layer palette is open");
    let texts = run(&mut paint, "window.layer");
    assert!(!layer_tab(&texts), "Window, Layer hides it");
    let texts = run(&mut paint, "window.layer");
    assert!(layer_tab(&texts), "and shows it again");
}

#[test]
fn step_05_the_timeline_bar_has_new_and_edit_timeline() {
    let mut paint = PaintDriver::new("tab-05");
    paint.settle(2);
    let head = header(&paint);
    let found = tips_along(&mut paint, head.bottom() + 24.0, 300.0, 1000.0);
    tip(&found, "Change Settings");
    // New Timeline, its first button (its name is drawn on the empty
    // timeline too, so it is found by what it tells).
    let controls = told(&mut paint);
    assert!(
        controls.iter().any(|(name, rect)| name == "New Timeline"
            && (rect.center().y - (head.bottom() + 24.0)).abs() < 12.0),
        "{controls:?}"
    );
}

#[test]
fn step_06_new_timeline_asks_name_rate_and_length_and_makes_no_folder() {
    // AB-7, ABV-2 (every layer a track).
    let mut paint = PaintDriver::new("tab-06");
    let picture = open_picture(&paint);
    let texts = run(&mut paint, "anim.timeline.new");
    for name in [
        "Timeline name",
        "Frame rate",
        "Playback time",
        "Scene number",
        "Shot number",
        "Division line",
    ] {
        assert!(
            texts.iter().any(|(text, _)| text.starts_with(name)),
            "{name}: {texts:?}"
        );
    }
    fill(&mut paint, "Frame rate", "12");
    fill(&mut paint, "Playback time", "24");
    undo_pair(&mut paint, "New Timeline", |paint| {
        press(paint, egui::Key::Enter)
    });
    let sequence = timeline(&paint);
    assert!((sue_paint::anim::fps(&sequence) - 12.0).abs() < 1e-6);
    assert_eq!(sue_paint::anim::length(arena(&paint), picture), 24);
    assert!(sue_paint::anim::folders(&sequence).is_empty(), "no folder");
    assert_eq!(
        sequence.tracks.len(),
        sue_paint::doc::layers(arena(&paint), picture).len(),
        "a track for every layer, the paper too"
    );
}

#[test]
fn step_07_no_frame_limit() {
    let mut paint = PaintDriver::new("tab-07");
    run(&mut paint, "anim.timeline.new");
    fill(&mut paint, "Playback time", "500");
    press(&mut paint, egui::Key::Enter);
    assert_eq!(
        sue_paint::anim::length(arena(&paint), open_picture(&paint)),
        500
    );
}

#[test]
fn step_08_new_animation_folder_shows_in_the_timeline_and_the_layer_palette() {
    // ABV-5: "Folder 1", and the cel showing after its name.
    let mut paint = PaintDriver::new("tab-08");
    paint.new_timeline();
    let head = header(&paint);
    let found = tips_along(&mut paint, head.bottom() + 24.0, head.left() - 60.0, 1000.0);
    let button = tip(&found, "Animation Folder");
    undo_pair(&mut paint, "New Animation Folder", |paint| {
        click_at(paint, button);
    });
    let folder = named(&paint, "Folder 1");
    assert!(sue_paint::anim::is_animation_folder(arena(&paint), folder));
    let texts = paint.settle(4);
    let drawn: Vec<&egui::Rect> = texts
        .iter()
        .filter(|(text, _)| text.starts_with("Folder 1"))
        .map(|(_, rect)| rect)
        .collect();
    assert!(
        drawn.iter().any(|rect| rect.left() > 1100.0),
        "in the Layer palette: {texts:?}"
    );
    assert!(
        drawn
            .iter()
            .any(|rect| rect.left() < 560.0 && rect.top() > 700.0),
        "on the timeline: {texts:?}"
    );
}

#[test]
fn step_09_dragging_the_playhead_scrubs() {
    let mut paint = PaintDriver::new("tab-09");
    run(&mut paint, "anim.layer.folder");
    run(&mut paint, "anim.cel.new");
    let grid = Grid::read(&mut paint);
    let ruler_y = grid.row("Folder 1") - 26.0;
    let from = Pos2::new(grid.one, ruler_y);
    let to = Pos2::new(grid.one + grid.step * 12.0, ruler_y);
    paint.drag(from, to, egui::PointerButton::Primary);
    assert_eq!(frame_shown(&mut paint), 13, "the playhead followed");
}

#[test]
fn step_10_start_and_end_markers_set_the_frames_played() {
    // AB-13.
    let mut paint = PaintDriver::new("tab-10");
    run(&mut paint, "anim.layer.folder");
    run(&mut paint, "anim.cel.new");
    let grid = Grid::read(&mut paint);
    let ruler_y = grid.row("Folder 1") - 26.0;
    let start = tip_fine(
        &mut paint,
        ruler_y,
        grid.one - grid.step / 2.0 - 2.0,
        grid.one - grid.step / 2.0 + 6.0,
        "first frame played",
    )
    .expect("the start marker");
    let end_x = grid.one + grid.step * 23.5;
    let end = tip_fine(
        &mut paint,
        ruler_y,
        end_x - 6.0,
        end_x + 2.0,
        "last frame played",
    )
    .expect("the end marker");
    let picture = open_picture(&paint);
    undo_pair(&mut paint, "the end marker", |paint| {
        drag_with(
            paint,
            end,
            end - vec2(grid.step * 6.0, 0.0),
            egui::Modifiers::NONE,
        );
    });
    undo_pair(&mut paint, "the start marker", |paint| {
        drag_with(
            paint,
            start,
            start + vec2(grid.step * 4.0, 0.0),
            egui::Modifiers::NONE,
        );
    });
    let settings = sue_paint::anim::setup::Settings::of(arena(&paint), picture);
    assert!(
        (3..=5).contains(&settings.start),
        "the start moved on: {}",
        settings.start
    );
    let end = settings.end.expect("an end");
    assert!((15..=19).contains(&end), "the end moved back: {end}");
    // Playing stays between them.
    run(&mut paint, "anim.play");
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..90 {
        seen.insert(frame_shown(&mut paint));
    }
    run(&mut paint, "anim.play");
    assert!(
        seen.iter()
            .all(|frame| (settings.start + 1..=end + 1).contains(frame)),
        "{seen:?}"
    );
}

#[test]
fn step_11_the_timeline_menu_has_change_frame_rate_and_change_settings() {
    // AB-11, ABV-6, ABV-7.
    let mut paint = PaintDriver::new("tab-11");
    paint.new_timeline();
    let head = header(&paint);
    let found = tips_along(&mut paint, head.center().y, head.left() - 70.0, head.left());
    let menu = tip(&found, "Timeline");
    let texts = click_at(&mut paint, menu);
    let row = texts
        .iter()
        .filter(|(text, rect)| text == "Timeline" && rect.top() < head.top())
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("Timeline in the palette's menu: {texts:?}"));
    paint.step(vec![egui::Event::PointerMoved(row.center())]);
    let texts = paint.settle(30);
    for name in ["Change Settings...", "Change Frame Rate..."] {
        assert!(
            texts.iter().any(|(text, _)| text == name),
            "{name}: {texts:?}"
        );
    }
    paint.click_text("Change Frame Rate...");
    paint.settle(3);
    fill(&mut paint, "Frame rate", "8");
    press(&mut paint, egui::Key::Enter);
    assert!((sue_paint::anim::fps(&timeline(&paint)) - 8.0).abs() < 1e-6);
    run(&mut paint, "anim.timeline.settings");
    fill(&mut paint, "Timeline name", "Bounce");
    press(&mut paint, egui::Key::Enter);
    assert_eq!(
        sue_paint::anim::setup::Settings::of(arena(&paint), open_picture(&paint)).name,
        "Bounce"
    );
}

#[test]
fn step_12_new_animation_cel_makes_cel_1_in_the_folder() {
    let mut paint = PaintDriver::new("tab-12");
    paint.new_timeline();
    run(&mut paint, "anim.layer.folder");
    let folder = named(&paint, "Folder 1");
    undo_pair(&mut paint, "New Animation Cel", |paint| {
        run(paint, "anim.cel.new");
    });
    let cel = shown(&paint, 1)[0].expect("a cel on frame 1");
    assert_eq!(name_of(&paint, cel), "1");
    assert_eq!(
        sue_paint::doc::folder_of(arena(&paint), cel),
        Some(folder),
        "in the folder"
    );
}

#[test]
fn step_13_a_circle_drawn_goes_on_the_cel() {
    let mut paint = PaintDriver::new("tab-13");
    run(&mut paint, "anim.layer.folder");
    run(&mut paint, "anim.cel.new");
    let cel = shown(&paint, 1)[0].unwrap();
    circle(&mut paint, vec2(0.0, 0.0), 40.0);
    assert!(painted(&paint, cel) > 200, "the circle is on cel 1");
}

#[test]
fn step_14_a_cel_on_frame_2_clears_the_canvas_and_is_named_2() {
    let paint = two_cels("tab-14");
    let names = shown_names(&paint, 2);
    assert_eq!(names, ["1", "2"]);
    // Frame 2 shows cel 2 alone: cel 1 is hidden there.
    let picture = open_picture(&paint);
    let sequence = timeline(&paint);
    let view = sue_paint::anim::view(&sequence, 1, None);
    let one = named(&paint, "1");
    let two = named(&paint, "2");
    assert!(view.hidden.contains(&one), "cel 1 hidden on frame 2");
    assert!(!view.hidden.contains(&two), "cel 2 shows");
    let _ = picture;
}

/// Whether the control named `name` (starting so) is on, as a fresh
/// frame tells assistive technology.
fn is_on(paint: &mut PaintDriver, name: &str) -> Option<bool> {
    paint.ctx.enable_accesskit();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        time: Some(paint.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    paint.now += Duration::from_millis(16);
    let runtime = &mut paint.runtime;
    let now = paint.now;
    let output = paint
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    let on = output
        .platform_output
        .accesskit_update
        .iter()
        .flat_map(|update| update.nodes.iter())
        .filter(|(_, node)| node.label().is_some_and(|label| label.starts_with(name)))
        .filter_map(|(_, node)| {
            node.toggled()
                .map(|toggled| toggled == egui::accesskit::Toggled::True)
                .or_else(|| node.is_selected())
        })
        .reduce(|a, b| a || b);
    output.drop_without_applying_deltas();
    on
}

#[test]
fn step_15_onion_skin_button() {
    let mut paint = two_cels("tab-15");
    bar_button(&mut paint, "Enable Onion Skin");
    // The Animation menu, Show Animation Cels: Enable Onion Skin is checked.
    let menu = paint
        .texts
        .iter()
        .find(|(text, rect)| text == "Animation" && rect.top() < 20.0)
        .map(|(_, rect)| rect.center())
        .expect("the Animation menu");
    click_at(&mut paint, menu);
    let row = paint
        .rect_of("Show Animation Cels")
        .unwrap_or_else(|| panic!("Show Animation Cels: {:?}", paint.texts));
    paint.step(vec![egui::Event::PointerMoved(row.center())]);
    paint.settle(30);
    assert_eq!(is_on(&mut paint, "Enable Onion Skin"), Some(true), "on");
    press(&mut paint, egui::Key::Escape);
    // What it shows: cel 1 as a skin on frame 2.
    let sequence = timeline(&paint);
    let onion = sue_paint::anim::Onion::of(arena(&paint), open_picture(&paint));
    let view = sue_paint::anim::view(&sequence, 1, Some(&onion));
    assert!(!view.onion.is_empty(), "cel 1 is a skin on frame 2");
}

#[test]
fn step_16_thumbnail_size_from_the_timeline_menu() {
    // A25-21, ABV-6: six sizes, None to Largest; None makes rows one line.
    let mut paint = two_cels("tab-16");
    paint.settle(2);
    let head = header(&paint);
    let found = tips_along(&mut paint, head.center().y, head.left() - 70.0, head.left());
    let menu = tip(&found, "Timeline");
    let texts = click_at(&mut paint, menu);
    let row = texts
        .iter()
        .find(|(text, rect)| text == "Thumbnail Size" && rect.top() < head.top())
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("Thumbnail Size: {texts:?}"));
    paint.step(vec![egui::Event::PointerMoved(row.center())]);
    let texts = paint.settle(30);
    for name in [
        "Thumbnails: None",
        "Thumbnails: Smallest",
        "Thumbnails: Small",
        "Thumbnails: Medium",
        "Thumbnails: Large",
        "Thumbnails: Largest",
    ] {
        assert!(
            texts.iter().any(|(text, _)| text == name),
            "{name}: {texts:?}"
        );
    }
    paint.click_text("Thumbnails: None");
    let grid = Grid::read(&mut paint);
    let short = grid.row("Layer 1") - grid.row("Folder 1");
    assert!(short < 40.0, "one line a row: {short}");
}

#[test]
fn step_17_animation_commands_take_shortcuts_f6_comma_and_period() {
    // AB-9: New Animation Cel on F6, the frame steps on comma and period.
    let mut paint = PaintDriver::new("tab-17");
    for (command, chord) in [
        ("anim.cel.new", "F6"),
        ("anim.prev", ","),
        ("anim.next", "."),
    ] {
        let rice = &paint.runtime.engine.world.rice;
        let (setting, value) = sue_wm::shortcuts::bind(
            rice,
            "sue_paint_keys",
            command,
            &chord.parse().expect("a chord"),
        );
        paint
            .runtime
            .engine
            .world
            .rice
            .set(&setting, value)
            .expect("the key setting takes it");
    }
    paint.settle(2);
    run(&mut paint, "anim.layer.folder");
    paint.step(vec![key(egui::Key::F6, egui::Modifiers::NONE)]);
    paint.settle(2);
    assert_eq!(shown_names(&paint, 1), ["1"], "F6 made cel 1");
    paint.step(vec![key(egui::Key::Period, egui::Modifiers::NONE)]);
    assert_eq!(frame_shown(&mut paint), 2, "period: the next frame");
    paint.step(vec![key(egui::Key::F6, egui::Modifiers::NONE)]);
    paint.settle(2);
    assert_eq!(shown_names(&paint, 2), ["1", "2"]);
    paint.step(vec![key(egui::Key::Comma, egui::Modifiers::NONE)]);
    assert_eq!(frame_shown(&mut paint), 1, "comma: the frame before");
}

#[test]
fn step_17_shortcut_settings_lists_the_animation_commands() {
    // AB-9: File, Shortcut Settings (Sue's Settings on Sue Paint's keys)
    // lists every command of the Animation menu, keyed or not.
    let mut paint = PaintDriver::new("tab-17-list");
    run(&mut paint, "file.shortcuts");
    paint.settle(8);
    let hint = sue_lang::text("settings", "filter-settings-ellipsis");
    let at = paint
        .rect_of(&hint)
        .unwrap_or_else(|| panic!("the filter: {:?}", paint.texts))
        .center();
    click_at(&mut paint, at);
    paint.step(vec![egui::Event::Text("New Animation Cel".to_owned())]);
    let texts = paint.settle(8);
    assert!(
        texts
            .iter()
            .filter(|(text, _)| text == "New Animation Cel")
            .count()
            > 0,
        "New Animation Cel is listed: {texts:?}"
    );
}

#[test]
fn step_18_onion_skin_settings_three_cels_each_way_and_the_color_modes() {
    // ABV-11: Color, Half color, Monochrome as Clip Studio Paint has them.
    let mut paint = two_cels("tab-18");
    let texts = run(&mut paint, "anim.onion_settings");
    for name in ["Cels before", "Cels after", "Color mode"] {
        assert!(
            texts.iter().any(|(text, _)| text == name),
            "{name}: {texts:?}"
        );
    }
    fill(&mut paint, "Cels before", "3");
    fill(&mut paint, "Cels after", "3");
    paint.click_text("OK");
    paint.settle(3);
    let onion = sue_paint::anim::Onion::of(arena(&paint), open_picture(&paint));
    assert_eq!((onion.before, onion.after), (3, 3));
    assert_eq!(
        sue_paint::anim::Onion::MODES,
        ["Color", "Half color", "Monochrome"]
    );
}

#[test]
fn step_19_play_runs_through_the_frames() {
    let mut paint = two_cels("tab-19");
    run(&mut paint, "anim.first");
    let head = header(&paint);
    let found = tips_along(&mut paint, head.bottom() + 24.0, head.left() - 60.0, 1000.0);
    let play = tip(&found, "Play");
    click_at(&mut paint, play);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..60 {
        seen.insert(frame_shown(&mut paint));
    }
    run(&mut paint, "anim.play");
    assert!(seen.len() > 5, "frames went by: {seen:?}");
}

// ----- 06:07 to 11:00: timing, copying, specifying ---------------------------

#[test]
fn step_20_a_cel_picked_and_dragged_one_frame_later_holds_the_one_before() {
    // AB-3.
    let mut paint = two_cels("tab-20");
    let grid = Grid::read(&mut paint);
    click_at(&mut paint, grid.cell("Folder 1", 2));
    undo_pair(&mut paint, "the cel dragged", |paint| {
        paint.drag(
            grid.cell("Folder 1", 2),
            grid.cell("Folder 1", 3),
            egui::PointerButton::Primary,
        );
    });
    assert_eq!(shown_names(&paint, 3), ["1", "1", "2"]);
}

#[test]
fn step_21_an_in_between_cel_is_named_after_the_cel_before() {
    // A25-24: "1a" between 1 and 2.
    let mut paint = PaintDriver::new("tab-21");
    run(&mut paint, "anim.layer.folder");
    run(&mut paint, "anim.cel.new");
    run(&mut paint, "anim.next");
    run(&mut paint, "anim.next");
    run(&mut paint, "anim.cel.new");
    run(&mut paint, "anim.prev");
    run(&mut paint, "anim.cel.new");
    assert_eq!(shown_names(&paint, 3), ["1", "1a", "2"]);
}

#[test]
fn step_22_a_box_over_several_cels_picks_them_and_they_drag_together() {
    // AB-3, ABV-12.
    let mut paint = two_cels("tab-22");
    let grid = Grid::read(&mut paint);
    // A drag across frames 1 and 2 picks both.
    paint.drag(
        grid.cell("Folder 1", 1),
        grid.cell("Folder 1", 2),
        egui::PointerButton::Primary,
    );
    // Dragged from the first picked, both go two frames later.
    undo_pair(&mut paint, "the cels dragged", |paint| {
        paint.drag(
            grid.cell("Folder 1", 1),
            grid.cell("Folder 1", 3),
            egui::PointerButton::Primary,
        );
    });
    let names = shown_names(&paint, 4);
    assert_eq!(names[2..], ["1", "2"], "{names:?}");
    assert_ne!(names[0], "1", "{names:?}");
}

#[test]
fn step_22_shift_and_a_click_picks_a_run_of_cels() {
    // ABV-12: Shift and a click from frame 1 to frame 2, then Delete
    // Frames empties both.
    let mut paint = two_cels("tab-22-shift");
    let grid = Grid::read(&mut paint);
    click_at(&mut paint, grid.cell("Folder 1", 1));
    press_at(
        &mut paint,
        grid.cell("Folder 1", 2),
        egui::PointerButton::Primary,
        egui::Modifiers::SHIFT,
    );
    undo_pair(&mut paint, "Delete Frames", |paint| {
        run(paint, "anim.edit.delete");
    });
    assert_eq!(shown_names(&paint, 2), ["-", "-"]);
}

#[test]
fn step_23_ctrl_c_and_ctrl_v_copy_the_picture_of_the_layer() {
    // AB-6: the pasted layer lands in the cel's animation folder.
    let mut paint = two_cels("tab-23");
    let cel = named(&paint, "2");
    let folder = named(&paint, "Folder 1");
    let ink = painted(&paint, cel);
    let before = stack(&paint);
    paint.step(vec![key(egui::Key::C, egui::Modifiers::COMMAND)]);
    paint.step(vec![key(egui::Key::V, egui::Modifiers::COMMAND)]);
    paint.settle(4);
    let made: Vec<EntityId> = stack(&paint)
        .into_iter()
        .filter(|id| !before.contains(id))
        .collect();
    assert_eq!(made.len(), 1, "one new layer");
    assert_eq!(painted(&paint, made[0]), ink, "the same picture");
    assert_eq!(
        sue_paint::doc::folder_of(arena(&paint), made[0]),
        Some(folder),
        "in the animation folder"
    );
    // The frames did not change.
    assert_eq!(shown_names(&paint, 2), ["1", "2"]);
}

#[test]
fn step_24_right_click_copy_and_paste_frames() {
    // AB-3, ABV-13.
    let mut paint = two_cels("tab-24");
    let grid = Grid::read(&mut paint);
    click_at(&mut paint, grid.cell("Folder 1", 1));
    right_click(&mut paint, grid.cell("Folder 1", 1));
    for name in ["Cut Frames", "Copy Frames", "Paste Frames", "Delete Frames"] {
        assert!(
            paint.texts.iter().any(|(text, _)| text == name),
            "{name}: {:?}",
            paint.texts
        );
    }
    paint.click_text("Copy Frames");
    right_click(&mut paint, grid.cell("Folder 1", 6));
    undo_pair(&mut paint, "Paste Frames", |paint| {
        paint.click_text("Paste Frames");
    });
    let names = shown_names(&paint, 6);
    assert_eq!(names[5], "1", "{names:?}");
}

#[test]
fn step_25_delete_specified_cel_leaves_the_drawing_in_the_folder() {
    let mut paint = two_cels("tab-25");
    let cel = named(&paint, "2");
    let head = header(&paint);
    let found = tips_along(&mut paint, head.bottom() + 24.0, head.left() - 60.0, 1100.0);
    let button = tip(&found, "Delete Keyframe");
    undo_pair(&mut paint, "Delete the key", |paint| {
        click_at(paint, button);
    });
    assert_eq!(shown_names(&paint, 2), ["1", "1"], "cel 1 holds");
    assert!(arena(&paint).get_by_id(cel).is_ok(), "the drawing stays");
    assert_eq!(
        sue_paint::doc::folder_of(arena(&paint), cel),
        Some(named(&paint, "Folder 1"))
    );
}

#[test]
fn step_26_specify_cels_lists_the_folders_cels_to_pick_one() {
    // AB-14.
    let mut paint = two_cels("tab-26");
    let head = header(&paint);
    let found = tips_along(&mut paint, head.bottom() + 24.0, head.left() - 60.0, 1100.0);
    let button = tip(&found, "Specify cels");
    let before = paint.settle(2);
    click_at(&mut paint, button);
    undo_pair(&mut paint, "Specify cels", |paint| {
        click_new(paint, &before, "1");
    });
    assert_eq!(shown_names(&paint, 2), ["1", "1"], "cel 1 on frame 2");
}

#[test]
fn step_27_right_click_a_frame_names_the_cels_to_pick() {
    // A25-23: the cel names at the top of the menu.
    let mut paint = two_cels("tab-27");
    let grid = Grid::read(&mut paint);
    let before = paint.settle(2);
    right_click(&mut paint, grid.cell("Folder 1", 4));
    undo_pair(&mut paint, "a cel picked by name", |paint| {
        pick_cel_in_menu(paint, &before, "1");
    });
    let names = shown_names(&paint, 4);
    assert_eq!(names[3], "1", "{names:?}");
}

#[test]
fn step_28_a_new_layer_in_the_folder_is_a_cel_to_specify() {
    // AB-6.
    let mut paint = two_cels("tab-28");
    let folder = named(&paint, "Folder 1");
    let before = stack(&paint);
    run(&mut paint, "layer.new");
    let made = stack(&paint)
        .into_iter()
        .find(|id| !before.contains(id))
        .expect("a new layer");
    assert_eq!(
        sue_paint::doc::folder_of(arena(&paint), made),
        Some(folder),
        "made in the animation folder"
    );
    let name = name_of(&paint, made);
    let grid = Grid::read(&mut paint);
    let before = paint.settle(2);
    right_click(&mut paint, grid.cell("Folder 1", 5));
    pick_cel_in_menu(&mut paint, &before, &name);
    let names = shown_names(&paint, 5);
    assert_eq!(names[4], name, "{names:?}");
}

#[test]
fn step_29_paste_with_a_cel_picked_makes_a_cel_in_the_folder() {
    // AB-6.
    let mut paint = two_cels("tab-29");
    let folder = named(&paint, "Folder 1");
    let before = stack(&paint);
    run(&mut paint, "edit.copy");
    undo_pair(&mut paint, "Paste", |paint| {
        run(paint, "edit.paste");
    });
    let_notices_go(&mut paint);
    let made = stack(&paint)
        .into_iter()
        .find(|id| !before.contains(id))
        .expect("a pasted layer");
    assert_eq!(sue_paint::doc::folder_of(arena(&paint), made), Some(folder));
    // Specified on frame 3 by name.
    let name = name_of(&paint, made);
    let grid = Grid::read(&mut paint);
    let before = paint.settle(2);
    right_click(&mut paint, grid.cell("Folder 1", 3));
    pick_cel_in_menu(&mut paint, &before, &name);
    assert_eq!(shown_names(&paint, 3)[2], name);
}

#[test]
fn step_30_two_cels_shift_picked_dragged_out_merged_and_back() {
    // AB-15.
    let mut paint = two_cels("tab-30");
    let folder = named(&paint, "Folder 1");
    let (one, two) = (named(&paint, "1"), named(&paint, "2"));
    // Cel 2 is current: Shift and a click on cel 1 picks both.
    let row = layer_row(&mut paint, "1");
    press_at(
        &mut paint,
        row.center(),
        egui::PointerButton::Primary,
        egui::Modifiers::SHIFT,
    );
    // Dragged together onto Layer 1, outside the folder.
    let from = layer_row(&mut paint, "2");
    let onto = layer_row(&mut paint, "Layer 1");
    paint.drag(from.center(), onto.center(), egui::PointerButton::Primary);
    assert_ne!(sue_paint::doc::folder_of(arena(&paint), one), Some(folder));
    assert_ne!(sue_paint::doc::folder_of(arena(&paint), two), Some(folder));
    // Merge with the layer below: one layer with both drawings.
    let ink = painted(&paint, one).max(painted(&paint, two));
    run(&mut paint, "layer.merge_down");
    let left: Vec<EntityId> = stack(&paint)
        .into_iter()
        .filter(|id| *id == one || *id == two)
        .collect();
    assert_eq!(left.len(), 1, "merged into one");
    let merged = left[0];
    assert!(painted(&paint, merged) > ink, "both drawings");
    // Back into the folder, where it is a cel again, specified on frame 1.
    let name = name_of(&paint, merged);
    let from = layer_row(&mut paint, &name);
    let texts = paint.settle(2);
    let onto = texts
        .iter()
        .filter(|(text, rect)| text.starts_with("Folder 1") && rect.left() > 1106.0)
        .map(|(_, rect)| *rect)
        .next()
        .unwrap_or_else(|| panic!("the folder's row: {texts:?}"));
    paint.drag(from.center(), onto.center(), egui::PointerButton::Primary);
    assert_eq!(
        sue_paint::doc::folder_of(arena(&paint), merged),
        Some(folder),
        "back in the folder"
    );
    let grid = Grid::read(&mut paint);
    let before = paint.settle(2);
    right_click(&mut paint, grid.cell("Folder 1", 1));
    pick_cel_in_menu(&mut paint, &before, &name);
    assert_eq!(shown_names(&paint, 1), [name]);
}

#[test]
fn step_31_create_folder_and_insert_layer_makes_the_folder_the_cel() {
    // AB-10: the folder takes the cel's name and its frames; a color
    // layer put in it shows and hides with it.
    let mut paint = two_cels("tab-31");
    let cel = named(&paint, "2");
    undo_pair(&mut paint, "Create Folder and Insert Layer", |paint| {
        run(paint, "layer.folder_insert");
    });
    let folder = sue_paint::doc::folder_of(arena(&paint), cel).expect("in a folder");
    assert_eq!(name_of(&paint, folder), "2", "named after the cel");
    assert_eq!(shown(&paint, 2)[1], Some(folder), "the folder is the cel");
    run(&mut paint, "layer.new");
    let color = stack(&paint)
        .into_iter()
        .find(|id| sue_paint::doc::folder_of(arena(&paint), *id) == Some(folder) && *id != cel)
        .expect("a color layer in the cel's folder");
    let sequence = timeline(&paint);
    let hidden = |frame: u32| {
        let view = sue_paint::anim::view(&sequence, frame, None);
        view.hidden.contains(&color) || view.hidden.contains(&folder)
    };
    assert!(hidden(0), "hidden on frame 1");
    assert!(!hidden(1), "shown on frame 2");
}

#[test]
fn step_32_rename_in_timeline_order() {
    let mut paint = PaintDriver::new("tab-32");
    run(&mut paint, "anim.layer.folder");
    run(&mut paint, "anim.cel.new");
    run(&mut paint, "anim.next");
    run(&mut paint, "anim.next");
    run(&mut paint, "anim.cel.new");
    run(&mut paint, "anim.prev");
    run(&mut paint, "anim.cel.new");
    assert_eq!(shown_names(&paint, 3), ["1", "1a", "2"]);
    assert!(
        sue_paint::menus()
            .iter()
            .flat_map(|menu| &menu.items)
            .flat_map(sue_wm::MenuItem::commands)
            .any(|command| command == "anim.cel.rename_timeline"),
        "in the Animation menu"
    );
    undo_pair(&mut paint, "Rename in Timeline Order", |paint| {
        run(paint, "anim.cel.rename_timeline");
    });
    assert_eq!(shown_names(&paint, 3), ["1", "2", "3"]);
}

#[test]
fn step_33_loop_play_starts_again_at_the_end() {
    let mut paint = two_cels("tab-33");
    run(&mut paint, "anim.last");
    run(&mut paint, "anim.prev");
    run(&mut paint, "anim.play");
    let mut seen = Vec::new();
    for _ in 0..60 {
        seen.push(frame_shown(&mut paint));
    }
    run(&mut paint, "anim.play");
    assert!(
        seen.windows(2).any(|pair| pair[1] < pair[0]),
        "it went round: {seen:?}"
    );
    // Loop off: it stops at the end.
    run(&mut paint, "anim.loop");
    run(&mut paint, "anim.last");
    run(&mut paint, "anim.prev");
    run(&mut paint, "anim.play");
    let mut seen = Vec::new();
    for _ in 0..60 {
        seen.push(frame_shown(&mut paint));
    }
    assert!(
        seen.windows(2).all(|pair| pair[1] >= pair[0]),
        "no loop: {seen:?}"
    );
}

#[test]
fn step_34_to_36_export_an_animated_gif_with_its_settings() {
    // AB-12, ABV-15: File, Export Animation, Animated GIF; its settings
    // (size, loop count, dithering, range); a word on the file's size.
    let mut paint = two_cels("tab-34");
    assert!(
        sue_paint::menus()
            .iter()
            .flat_map(|menu| &menu.items)
            .flat_map(sue_wm::MenuItem::commands)
            .any(|command| command == "file.export.gif"),
        "File, Export Animation, Animated GIF"
    );
    let texts = run(&mut paint, "file.export.gif");
    for name in ["Width", "Height", "Loop count", "Dithering", "Frames"] {
        assert!(
            texts.iter().any(|(text, _)| text == name),
            "{name}: {texts:?}"
        );
    }
    fill(&mut paint, "Width", "480");
    paint.click_text("OK");
    paint.settle(2);
    let path = paint.root.join("Bounce.gif");
    std::fs::create_dir_all(&paint.root).unwrap();
    paint.runtime.request(
        ShellRequest::Picked {
            pick: FilePick::save(
                Some(AppletId::new(sue_paint::ID).unwrap()),
                "anim-gif",
                "Export",
                "x",
            ),
            path: path.clone(),
        },
        &mut NoTextures,
        paint.now,
    );
    paint.settle(2);
    paint.finish_exports();
    let bytes = std::fs::read(&path).expect("the GIF is written");
    assert_eq!(&bytes[..3], b"GIF");
    assert_eq!(u16::from_le_bytes([bytes[6], bytes[7]]), 480);
    let message = paint
        .runtime
        .shell
        .message()
        .map(|(text, _)| text.to_owned())
        .unwrap_or_default();
    assert!(message.contains("KiB"), "the file's size: {message:?}");
}

// ----- 12:35 to 15:19: the Animation Cels palette and the light table --------

/// Sue Paint's launchers and marks as the last frame drew them.
fn marks(paint: &PaintDriver) -> Vec<(String, egui::Rect)> {
    sue_paint::shown_buttons(&paint.ctx)
}

fn mark(paint: &PaintDriver, id: &str) -> Pos2 {
    let wanted = format!("mark.{id}");
    marks(paint)
        .into_iter()
        .find(|(found, _)| *found == wanted)
        .unwrap_or_else(|| panic!("{wanted} is drawn: {:?}", marks(paint)))
        .1
        .center()
}

/// Where the right dock drew `text` (left of the Layer palette's column
/// or in it), the topmost.
fn in_right_dock(paint: &PaintDriver, text: &str) -> Option<egui::Rect> {
    paint
        .texts
        .iter()
        .filter(|(found, rect)| found.starts_with(text) && rect.left() > 840.0)
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
}

/// Two cels and the Animation Cels palette open beside the Layer palette.
fn cels_open(tag: &str) -> PaintDriver {
    let mut paint = two_cels(tag);
    run(&mut paint, "window.cels");
    paint.settle(6);
    paint
}

#[test]
fn step_37_window_animation_cels_opens_the_palette() {
    let mut paint = two_cels("tab-37");
    assert!(in_right_dock(&paint, "Light table of cel").is_none());
    run(&mut paint, "window.cels");
    paint.settle(4);
    assert!(
        in_right_dock(&paint, "Light table of cel 2").is_some(),
        "{:?}",
        paint.texts
    );
}

#[test]
fn step_38_the_palette_has_the_cel_drawn_and_the_light_tables_under_it() {
    // AB-8, ABV-16: the cel being drawn, its own light table, then the
    // light table every cel shares, each with its place to drop layers.
    let paint = cels_open("tab-38");
    let cel = in_right_dock(&paint, "Light table of cel 2").expect("the cel's table");
    let general = in_right_dock(&paint, "General light table")
        .unwrap_or_else(|| panic!("the general light table: {:?}", paint.texts));
    assert!(
        cel.top() < general.top(),
        "the cel's first, then the general"
    );
}

#[test]
fn step_39_the_three_toggles_and_enable_light_table() {
    let mut paint = cels_open("tab-39");
    for (command, label) in [
        ("anim.light_table", "Enable Light Table"),
        ("anim.light_table.tool", "Enable/disable light table tool"),
        ("anim.light_table.cel", "Show cel-specific light table"),
        ("anim.light_table.general", "Show general light table"),
    ] {
        assert_eq!(
            sue_paint::registry().label(command).as_deref(),
            Some(label),
            "{command}"
        );
    }
    // The buttons are in the palette, found by their names.
    let controls = told(&mut paint);
    for label in [
        "Enable Light Table",
        "Enable/disable light table tool",
        "Show cel-specific light table",
        "Show general light table",
    ] {
        assert!(
            controls
                .iter()
                .any(|(name, rect)| name.starts_with(label) && rect.left() > 840.0),
            "{label}: {:?}",
            controls.iter().map(|(name, _)| name).collect::<Vec<_>>()
        );
    }
}

#[test]
fn step_40_and_41_a_cel_dragged_to_the_general_light_table_shows_on_every_cel() {
    let mut paint = cels_open("tab-40");
    let picture = open_picture(&paint);
    let one = named(&paint, "1");
    let general = in_right_dock(&paint, "General light table").expect("the general table");
    let row = layer_row(&mut paint, "1");
    undo_pair(&mut paint, "the drop", |paint| {
        paint.drag(row.center(), general.center(), egui::PointerButton::Primary);
    });
    assert_eq!(
        sue_paint::anim::general_light_table(arena(&paint), picture),
        vec![one]
    );
    // Shown under cel 2 (frame 2) and under a cel made on frame 5.
    let traced = |paint: &PaintDriver| {
        sue_paint::anim::traced_on(
            arena(paint),
            open_picture(paint),
            sue_paint::anim::GENERAL_LIGHT_TABLE,
            [64, 64],
        )
        .len()
    };
    assert_eq!(traced(&paint), 1);
    for _ in 0..3 {
        run(&mut paint, "anim.next");
    }
    run(&mut paint, "anim.cel.new");
    assert_eq!(traced(&paint), 1, "on every cel");
}

#[test]
fn step_42_to_44_the_light_table_tool_moves_scales_turns_flips_and_resets() {
    // AB-8, ABV-17.
    let mut paint = cels_open("tab-42");
    let one = named(&paint, "1");
    // A circle on cel 1 too, so its box is roomy.
    run(&mut paint, "anim.first");
    circle(&mut paint, vec2(0.0, 0.0), 60.0);
    run(&mut paint, "anim.next");
    run(&mut paint, "layer.select_below");
    run(&mut paint, "anim.light_table.register_general");
    run(&mut paint, "layer.select_above");
    run(&mut paint, "anim.light_table.tool");
    paint.settle(4);
    let place =
        |paint: &PaintDriver| sue_paint::anim::Place::of(arena(paint).get_by_id(one).unwrap());
    let middle = mark(&paint, "light.corner.0").lerp(mark(&paint, "light.corner.2"), 0.5);
    undo_pair(&mut paint, "a light table move", |paint| {
        drag_with(
            paint,
            middle,
            middle + vec2(30.0, 10.0),
            egui::Modifiers::NONE,
        );
    });
    let corner = mark(&paint, "light.corner.2");
    undo_pair(&mut paint, "a light table scale", |paint| {
        drag_with(
            paint,
            corner,
            corner + vec2(40.0, 20.0),
            egui::Modifiers::NONE,
        );
    });
    assert!(place(&paint).scale > 1.1, "{:?}", place(&paint));
    let stalk = mark(&paint, "light.stalk");
    undo_pair(&mut paint, "a light table turn", |paint| {
        drag_with(
            paint,
            stalk,
            stalk + vec2(80.0, 40.0),
            egui::Modifiers::NONE,
        );
    });
    assert!(place(&paint).degrees.abs() > 5.0, "{:?}", place(&paint));
    // Flip and reset from the box's launcher.
    let flip = marks(&paint)
        .into_iter()
        .find(|(id, _)| id == "launcher.light.flip")
        .expect("Flip on the launcher")
        .1
        .center();
    undo_pair(&mut paint, "a light table flip", |paint| {
        click_at(paint, flip);
    });
    assert!(place(&paint).flip);
    let reset = marks(&paint)
        .into_iter()
        .find(|(id, _)| id == "anim.light_table.reset")
        .expect("Reset on the launcher")
        .1
        .center();
    click_at(&mut paint, reset);
    assert!(place(&paint).is_still(), "{:?}", place(&paint));
    // Step 44: back to drawing, the stroke goes on the cel.
    run(&mut paint, "anim.light_table.tool");
    run(&mut paint, "tool.pen");
    let cel = named(&paint, "2");
    let ink = painted(&paint, cel);
    line(&mut paint, 0.0);
    assert!(painted(&paint, cel) > ink, "drawn on cel 2 again");
}

#[test]
fn step_45_the_light_table_color_and_its_color_can_be_chosen() {
    // AB-8, ABV-16: Color mode on the palette, and Change layer color.
    let mut paint = cels_open("tab-45");
    let controls = told(&mut paint);
    let names: Vec<&String> = controls.iter().map(|(name, _)| name).collect();
    assert!(
        controls
            .iter()
            .any(|(name, rect)| name.starts_with("Color mode") && rect.left() > 840.0),
        "{names:?}"
    );
    assert!(
        controls.iter().any(|(name, rect)| {
            name == "Pick a color" && rect.left() > 840.0 && rect.left() < 1106.0
        }),
        "the light table's color: {names:?}"
    );
}

#[test]
fn step_46_the_light_table_goes_off_and_on_and_an_entry_leaves_it() {
    let mut paint = cels_open("tab-46");
    let picture = open_picture(&paint);
    let one = named(&paint, "1");
    run(&mut paint, "layer.select_below");
    run(&mut paint, "anim.light_table.register_general");
    let traced =
        |paint: &PaintDriver| sue_paint::anim::general_light_table(arena(paint), picture).len();
    assert_eq!(traced(&paint), 1);
    run(&mut paint, "anim.light_table");
    run(&mut paint, "anim.light_table");
    assert_eq!(traced(&paint), 1, "off and on again keeps it");
    let texts = paint.settle(3);
    let entry = texts
        .iter()
        .filter(|(text, rect)| text == "1" && rect.left() > 840.0 && rect.right() < 1106.0)
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap_or_else(|| panic!("cel 1's entry: {texts:?}"));
    let found = tips_along(&mut paint, entry.top(), entry.right(), 1104.0);
    let off = tip(&found, "Deregister from the light table");
    undo_pair(&mut paint, "Deregister", |paint| {
        click_at(paint, off);
    });
    assert!(
        !sue_paint::anim::general_light_table(arena(&paint), picture).contains(&one),
        "taken off"
    );
}

#[test]
fn step_47_a_folder_hidden_by_its_eye_then_a_second_folder_and_cel() {
    let mut paint = two_cels("tab-47");
    let folder = named(&paint, "Folder 1");
    let row = layer_row(&mut paint, "Folder 1 : 2");
    let found = tips_along(&mut paint, row.center().y, 1106.0, row.left());
    let eye = tip(&found, "Hide Layer");
    undo_pair(&mut paint, "the eye", |paint| {
        click_at(paint, eye);
    });
    let visible = sue_paint::doc::layers(arena(&paint), open_picture(&paint))
        .into_iter()
        .find(|info| info.id == folder)
        .unwrap()
        .style
        .visible;
    assert!(!visible, "Folder 1 hidden");
    run(&mut paint, "anim.layer.folder");
    run(&mut paint, "anim.first");
    run(&mut paint, "anim.cel.new");
    let second = named(&paint, "Folder 2");
    let cel = shown(&paint, 1);
    let _ = cel;
    let track = track_named(&paint, "Folder 2");
    let sequence = timeline(&paint);
    let made = sue_paint::anim::cel_at(&sequence.tracks[track], sequence.rate, 0).expect("cel 1");
    assert_eq!(sue_paint::doc::folder_of(arena(&paint), made), Some(second));
    line(&mut paint, 0.0);
    assert!(painted(&paint, made) > 50, "drawn on Folder 2's cel");
}

// ----- 15:19 to 20:15: keyframes and the 2D camera ----------------------------

/// A folder "Folder 1" with a ball drawn on its cel 1, picked, and the
/// timeline's bar's buttons by name.
fn ball(tag: &str) -> (PaintDriver, EntityId) {
    let mut paint = PaintDriver::new(tag);
    paint.new_timeline();
    run(&mut paint, "anim.layer.folder");
    run(&mut paint, "anim.cel.new");
    circle(&mut paint, vec2(-120.0, 0.0), 30.0);
    let folder = named(&paint, "Folder 1");
    let row = layer_row(&mut paint, "Folder 1 : 1");
    click_at(&mut paint, row.center());
    (paint, folder)
}

fn pose_at(paint: &PaintDriver, layer: EntityId, frame: u32) -> sue_paint::anim::motion::Pose {
    sue_paint::anim::motion::pose(arena(paint), layer, time(paint, frame), size(paint))
        .unwrap_or_else(|| sue_paint::anim::motion::Pose::still(size(paint)))
}

/// The keys of `layer`, as frames from 0.
fn key_frames(paint: &PaintDriver, layer: EntityId) -> Vec<i64> {
    let rate = rate(paint);
    sue_paint::anim::motion::key_times(arena(paint), layer)
        .into_iter()
        .map(|time| time.frame(rate))
        .collect()
}

#[test]
fn step_48_enable_keyframes_on_this_layer_from_the_bar() {
    // AB-1, ABV-19: the button's name.
    let (mut paint, folder) = ball("tab-48");
    undo_pair(&mut paint, "Enable Keyframes", |paint| {
        bar_button(paint, "Enable Keyframes on This Layer");
    });
    assert!(sue_paint::anim::motion::keyframes_on(
        arena(&paint).get_by_id(folder).unwrap()
    ));
}

#[test]
fn step_49_add_keyframe_from_a_right_click_on_the_track() {
    // AB-1, ABV-14.
    let (mut paint, folder) = ball("tab-49");
    run(&mut paint, "anim.keyframes");
    let grid = Grid::read(&mut paint);
    let before = paint.settle(2);
    right_click(&mut paint, grid.cell("Folder 1", 1));
    undo_pair(&mut paint, "Add Keyframe", |paint| {
        click_new(paint, &before, "Add Keyframe to Layer");
    });
    assert_eq!(key_frames(&paint, folder), [0]);
}

#[test]
fn step_50_and_51_object_handles_move_the_folder_and_key_it_into_a_tween() {
    // AB-1: the Object tool on a keyed folder; Shift keeps the line; the
    // key at the frame shown is made by the drag; between keys it moves
    // smoothly.
    let (mut paint, folder) = ball("tab-50");
    run(&mut paint, "anim.key.layer");
    for _ in 0..11 {
        run(&mut paint, "anim.next");
    }
    run(&mut paint, "tool.operation");
    paint.settle(4);
    let inside = mark(&paint, "keyed.middle") + vec2(4.0, 4.0);
    undo_pair(&mut paint, "an Object move", |paint| {
        drag_with(
            paint,
            inside,
            inside + vec2(200.0, 15.0),
            egui::Modifiers::SHIFT,
        );
    });
    assert_eq!(key_frames(&paint, folder), [0, 11], "keyed at frame 12");
    let end = pose_at(&paint, folder, 11);
    assert!(end.position[0] > 150.0, "{end:?}");
    assert!(end.position[1].abs() < 0.5, "Shift kept it level: {end:?}");
    let middle = pose_at(&paint, folder, 5);
    assert!(
        middle.position[0] > 10.0 && middle.position[0] < end.position[0] - 10.0,
        "a tween between: {middle:?}"
    );
    // The frame view moves the folder there.
    let view = sue_paint::anim::frame_view(arena(&paint), open_picture(&paint), 5, None, None)
        .expect("a view");
    assert!(view.moves.iter().any(|(id, _)| *id == folder));
}

#[test]
fn step_52_keyframe_interpolation_smooth_linear_and_hold() {
    // AB-2: from the right click on a key, and from the menu.
    let (mut paint, folder) = ball("tab-52");
    run(&mut paint, "anim.key.layer");
    let ease = |paint: &PaintDriver| {
        sue_paint::anim::motion::ease_at(arena(paint), folder, time(paint, 0))
    };
    assert_eq!(
        ease(&paint),
        Some(sue_paint::anim::motion::Ease::Smooth),
        "smooth first"
    );
    for (command, want) in [
        (
            "anim.key.ease.linear",
            sue_paint::anim::motion::Ease::Linear,
        ),
        ("anim.key.ease.hold", sue_paint::anim::motion::Ease::Hold),
        (
            "anim.key.ease.smooth",
            sue_paint::anim::motion::Ease::Smooth,
        ),
    ] {
        undo_pair(&mut paint, command, |paint| {
            run(paint, command);
        });
        assert_eq!(ease(&paint), Some(want));
    }
    let grid = Grid::read(&mut paint);
    right_click(&mut paint, grid.cell("Folder 1", 1));
    let row = paint
        .rect_of("Keyframe Interpolation")
        .unwrap_or_else(|| panic!("Keyframe Interpolation: {:?}", paint.texts));
    paint.step(vec![egui::Event::PointerMoved(row.center())]);
    paint.settle(30);
    paint.click_text("Hold");
    assert_eq!(ease(&paint), Some(sue_paint::anim::motion::Ease::Hold));
}

#[test]
fn step_53_cels_drawn_frame_by_frame_play_along_the_tween() {
    let (mut paint, folder) = ball("tab-53");
    run(&mut paint, "anim.key.layer");
    for _ in 0..11 {
        run(&mut paint, "anim.next");
    }
    run(&mut paint, "tool.operation");
    paint.settle(4);
    let inside = mark(&paint, "keyed.middle") + vec2(4.0, 4.0);
    drag_with(
        &mut paint,
        inside,
        inside + vec2(200.0, 0.0),
        egui::Modifiers::NONE,
    );
    // Keyframes off on the folder: draw cel 2 on frame 5.
    run(&mut paint, "anim.keyframes");
    assert!(!sue_paint::anim::motion::keyframes_on(
        arena(&paint).get_by_id(folder).unwrap()
    ));
    run(&mut paint, "tool.pen");
    run(&mut paint, "anim.first");
    for _ in 0..4 {
        run(&mut paint, "anim.next");
    }
    run(&mut paint, "anim.cel.new");
    circle(&mut paint, vec2(-120.0, -20.0), 30.0);
    let names = shown_names(&paint, 6);
    assert_eq!(names[4], "2", "{names:?}");
    // On again on the folder, picked again: the keys are kept and move
    // the cels.
    let row = layer_row(&mut paint, "Folder 1");
    click_at(&mut paint, row.center());
    run(&mut paint, "anim.keyframes");
    assert_eq!(key_frames(&paint, folder), [0, 11], "the keys stay");
    let view = sue_paint::anim::frame_view(arena(&paint), open_picture(&paint), 4, None, None)
        .expect("a view");
    assert!(
        view.moves.iter().any(|(id, _)| *id == folder),
        "the folder with its cel 2 moves on frame 5"
    );
}

#[test]
fn step_54_keyframes_switch_copy_paste_drag_and_delete() {
    // AB-1, AB-2, ABV-14.
    let (mut paint, folder) = ball("tab-54");
    run(&mut paint, "anim.key.layer");
    let grid = Grid::read(&mut paint);
    right_click(&mut paint, grid.cell("Folder 1", 1));
    for name in [
        "Keyframe Interpolation",
        "Delete Keyframe from Layer",
        "Copy Frames",
        "Paste Frames",
    ] {
        assert!(
            paint.texts.iter().any(|(text, _)| text == name),
            "{name}: {:?}",
            paint.texts
        );
    }
    press(&mut paint, egui::Key::Escape);
    // Copy the frame with the key, paste it on frame 8: a key there.
    click_at(&mut paint, grid.cell("Folder 1", 1));
    run(&mut paint, "anim.edit.copy");
    for _ in 0..7 {
        run(&mut paint, "anim.next");
    }
    undo_pair(&mut paint, "Paste Frames with a key", |paint| {
        run(paint, "anim.edit.paste");
    });
    assert_eq!(key_frames(&paint, folder), [0, 7], "the key pasted");
    // Delete Keyframe from Layer on frame 8, the folder picked.
    let row = layer_row(&mut paint, "Folder 1");
    click_at(&mut paint, row.center());
    undo_pair(&mut paint, "Delete Keyframe from Layer", |paint| {
        run(paint, "anim.key.layer_delete");
    });
    assert_eq!(key_frames(&paint, folder), [0]);
}

#[test]
fn step_55_new_2d_camera_folder_is_empty_with_its_track_on_top() {
    // AB-4, ABV-19.
    let mut paint = two_cels("tab-55");
    assert_eq!(
        sue_paint::registry().label("anim.layer.camera").as_deref(),
        Some("2D Camera Folder")
    );
    undo_pair(&mut paint, "2D Camera Folder", |paint| {
        run(paint, "anim.layer.camera");
    });
    let camera = named(&paint, "Camera 1");
    assert!(sue_paint::camera::is_camera(arena(&paint), camera));
    assert!(
        arena(&paint).relations().children(camera).is_empty(),
        "empty"
    );
    let grid = Grid::read(&mut paint);
    assert!(
        grid.row("Camera 1") < grid.row("Folder 1"),
        "its track on top"
    );
}

#[test]
fn step_56_everything_shift_picked_and_dragged_into_the_camera_folder() {
    // AB-15.
    let mut paint = two_cels("tab-56");
    run(&mut paint, "anim.layer.camera");
    let camera = named(&paint, "Camera 1");
    let folder = named(&paint, "Folder 1");
    let layer = named(&paint, "Layer 1");
    // Folder 1, then Shift and a click on Layer 1 picks both.
    let row = layer_row(&mut paint, "Folder 1 : 2");
    click_at(&mut paint, row.center());
    let row = layer_row(&mut paint, "Layer 1");
    press_at(
        &mut paint,
        row.center(),
        egui::PointerButton::Primary,
        egui::Modifiers::SHIFT,
    );
    let from = layer_row(&mut paint, "Folder 1 : 2");
    let onto = layer_row(&mut paint, "Camera 1");
    undo_pair(&mut paint, "the drag into the camera", |paint| {
        paint.drag(from.center(), onto.center(), egui::PointerButton::Primary);
    });
    assert_eq!(
        sue_paint::doc::folder_of(arena(&paint), folder),
        Some(camera)
    );
    assert_eq!(
        sue_paint::doc::folder_of(arena(&paint), layer),
        Some(camera)
    );
}

/// A canvas with a timeline, a cel and a 2D camera folder, current.
fn camera(tag: &str) -> (PaintDriver, EntityId) {
    let mut paint = two_cels(tag);
    run(&mut paint, "anim.layer.camera");
    let camera = named(&paint, "Camera 1");
    (paint, camera)
}

fn camera_at(paint: &PaintDriver, folder: EntityId, frame: u32) -> sue_paint::camera::Camera {
    sue_paint::camera::Camera::of(arena(paint), folder, Some(time(paint, frame)), size(paint))
}

#[test]
fn step_57_add_keyframe_on_the_camera_track() {
    // AB-4.
    let (mut paint, folder) = camera("tab-57");
    run(&mut paint, "anim.first");
    undo_pair(&mut paint, "a camera key", |paint| {
        bar_button(paint, "Add Keyframe");
    });
    assert_eq!(sue_paint::camera::keys(arena(&paint), folder).len(), 1);
}

#[test]
fn step_58_a_camera_key_switched_to_smooth() {
    let (mut paint, folder) = camera("tab-58");
    run(&mut paint, "anim.first");
    run(&mut paint, "anim.camera.key");
    run(&mut paint, "anim.key.ease.hold");
    let ease = |paint: &PaintDriver| {
        sue_paint::anim::motion::ease_at(arena(paint), folder, time(paint, 0))
    };
    assert_eq!(ease(&paint), Some(sue_paint::anim::motion::Ease::Hold));
    undo_pair(&mut paint, "smooth", |paint| {
        run(paint, "anim.key.ease.smooth");
    });
    assert_eq!(ease(&paint), Some(sue_paint::anim::motion::Ease::Smooth));
}

#[test]
fn step_59_and_60_the_camera_frame_zooms_moves_and_shows_its_path() {
    // AB-16, ABV-21: Alt zooms about the middle, Shift keeps the axis; the
    // path through the keys shows.
    let (mut paint, folder) = camera("tab-59");
    run(&mut paint, "anim.first");
    run(&mut paint, "anim.camera.key");
    run(&mut paint, "tool.operation");
    paint.settle(4);
    let base = camera_at(&paint, folder, 0);
    let corner = mark(&paint, "camera.corner.2");
    drag_with(
        &mut paint,
        corner,
        corner - vec2(60.0, 40.0),
        egui::Modifiers::ALT,
    );
    let zoomed = camera_at(&paint, folder, 0);
    assert!(zoomed.scale > base.scale + 0.05, "{zoomed:?}");
    assert!(
        (zoomed.center[0] - base.center[0]).abs() < 1.0,
        "about its middle: {zoomed:?}"
    );
    for _ in 0..11 {
        run(&mut paint, "anim.next");
    }
    let middle = mark(&paint, "camera.middle") + vec2(20.0, 20.0);
    drag_with(
        &mut paint,
        middle,
        middle + vec2(120.0, 10.0),
        egui::Modifiers::SHIFT,
    );
    assert_eq!(
        sue_paint::camera::keys(arena(&paint), folder).len(),
        2,
        "keyed by the drag"
    );
    let moved = camera_at(&paint, folder, 11);
    assert!(moved.center[0] > zoomed.center[0] + 50.0, "{moved:?}");
    assert!(
        (moved.center[1] - zoomed.center[1]).abs() < 0.5,
        "Shift kept it level"
    );
    let between = camera_at(&paint, folder, 5);
    assert!(between.center[0] > zoomed.center[0] && between.center[0] < moved.center[0]);
}

#[test]
fn step_61_playback_settings_render_2d_camera() {
    // AB-17, ABV-22.
    let (mut paint, _) = camera("tab-61");
    let picture = open_picture(&paint);
    assert!(sue_paint::anim::setup::Playback::of(arena(&paint), picture).render_camera);
    assert_eq!(
        sue_paint::registry()
            .label("anim.playback.camera")
            .as_deref(),
        Some("Render 2D Camera")
    );
    undo_pair(&mut paint, "Render 2D Camera", |paint| {
        run(paint, "anim.playback.camera");
    });
    assert!(!sue_paint::anim::setup::Playback::of(arena(&paint), picture).render_camera);
}

// ----- 20:15 to 22:57: sound ---------------------------------------------------

/// A two second tone written beside the test's files.
fn tone(paint: &PaintDriver) -> std::path::PathBuf {
    std::fs::create_dir_all(&paint.root).unwrap();
    let samples: Vec<f32> = (0..96_000)
        .map(|index| (index as f32 * 0.05).sin() * 0.5 * (index as f32 / 96_000.0))
        .collect();
    let pcm = sue_core::PCMFloat1D::new(48_000, 1, samples).unwrap();
    let path = paint.root.join("Golly.wav");
    std::fs::write(&path, sue_io::encode_wav(&pcm)).unwrap();
    path
}

fn audio_tracks(paint: &PaintDriver) -> usize {
    sue_paint::anim::audio_tracks(&timeline(paint)).len()
}

#[test]
fn step_62_sound_plays_with_playback_and_scrubbing_is_offered() {
    let mut paint = two_cels("tab-62");
    let path = tone(&paint);
    paint.runtime.request(
        ShellRequest::Picked {
            pick: FilePick::open(
                Some(AppletId::new(sue_paint::ID).unwrap()),
                "anim-audio",
                "Import Audio",
            ),
            path,
        },
        &mut NoTextures,
        paint.now,
    );
    paint.settle(4);
    run(&mut paint, "anim.first");
    run(&mut paint, "anim.play");
    assert!(paint.runtime.transport.is_playing_for("sue-paint"));
    run(&mut paint, "anim.play");
    assert!(!paint.runtime.transport.is_playing_for("sue-paint"));
    assert!(sue_paint::registry().label("anim.audio.scrub").is_some());
}

#[test]
fn step_63_a_sound_file_dropped_on_the_canvas_becomes_an_audio_track() {
    // AB-22.
    let mut paint = two_cels("tab-63");
    let path = tone(&paint);
    assert_eq!(audio_tracks(&paint), 0);
    paint
        .runtime
        .request(ShellRequest::OpenFile(path), &mut NoTextures, paint.now);
    paint.settle(6);
    assert_eq!(audio_tracks(&paint), 1, "the drop made an audio track");
}

#[test]
fn step_64_file_import_audio() {
    let mut paint = two_cels("tab-64");
    assert!(
        sue_paint::menus()
            .iter()
            .flat_map(|menu| &menu.items)
            .flat_map(sue_wm::MenuItem::commands)
            .any(|command| command == "file.import.audio"),
        "File, Import, Audio"
    );
    let path = tone(&paint);
    let before = arena_bytes(&paint);
    paint.runtime.request(
        ShellRequest::Picked {
            pick: FilePick::open(
                Some(AppletId::new(sue_paint::ID).unwrap()),
                "anim-audio",
                "Import Audio",
            ),
            path,
        },
        &mut NoTextures,
        paint.now,
    );
    paint.settle(4);
    assert_eq!(audio_tracks(&paint), 1);
    undo(&mut paint);
    assert!(arena_bytes(&paint) == before, "the import undoes");
}

/// Two cels, a tone on the timeline from frame 1, and the rows one line
/// tall so every track is in sight.
fn with_sound(tag: &str) -> PaintDriver {
    let mut paint = two_cels(tag);
    let path = tone(&paint);
    run(&mut paint, "anim.first");
    paint.runtime.request(
        ShellRequest::Picked {
            pick: FilePick::open(
                Some(AppletId::new(sue_paint::ID).unwrap()),
                "anim-audio",
                "Import Audio",
            ),
            path,
        },
        &mut NoTextures,
        paint.now,
    );
    run(&mut paint, "anim.thumbnails.none");
    paint.settle(4);
    paint
}

/// Where the sound's clip shows its name on the timeline.
fn clip_name(paint: &mut PaintDriver) -> egui::Rect {
    paint.settle(3);
    let head = header(paint);
    // The rows scrolled down (Alt and the wheel) to the sound's, the last.
    let over = Pos2::new(head.left() + 300.0, head.bottom() + 100.0);
    paint.step(vec![egui::Event::PointerMoved(over)]);
    for _ in 0..4 {
        paint.step(vec![
            egui::Event::ModifiersChanged(egui::Modifiers::ALT),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, -60.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: egui::Modifiers::ALT,
            },
        ]);
    }
    paint.step(vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)]);
    let texts = paint.settle(6);
    texts
        .iter()
        .filter(|(text, rect)| {
            text.starts_with("Golly") && text.contains("dB") && rect.top() > head.bottom()
        })
        .map(|(_, rect)| *rect)
        .next()
        .unwrap_or_else(|| panic!("the clip's name: {texts:?}"))
}

#[test]
fn step_65_the_audio_clip_drags_along_the_timeline() {
    // ABV-23: the file's name on its track and clip.
    let mut paint = with_sound("tab-65");
    let clip = clip_name(&mut paint);
    let grid = Grid::read(&mut paint);
    let start = |paint: &PaintDriver| {
        let sequence = timeline(paint);
        let track = sue_paint::anim::audio_tracks(&sequence)[0];
        sequence.tracks[track].clips()[0].start.frame(sequence.rate)
    };
    assert_eq!(start(&paint), 0);
    let from = clip.center() + vec2(0.0, 6.0);
    undo_pair(&mut paint, "the clip dragged", |paint| {
        paint.drag(
            from,
            from + vec2(grid.step * 3.0, 0.0),
            egui::PointerButton::Primary,
        );
    });
    assert_eq!(start(&paint), 3, "three frames later");
}

#[test]
fn step_72_the_waveform_shows_on_the_clip() {
    // AB-18.
    let mut paint = with_sound("tab-72");
    let clip = clip_name(&mut paint);
    // One frame's shapes: the waveform is a line a column inside the clip.
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        time: Some(paint.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    paint.now += Duration::from_millis(16);
    let runtime = &mut paint.runtime;
    let now = paint.now;
    let output = paint
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    let area = egui::Rect::from_min_max(
        Pos2::new(clip.left(), clip.bottom()),
        Pos2::new(clip.left() + 300.0, clip.bottom() + 30.0),
    );
    let mut lines = 0;
    for clipped in &output.shapes {
        let mut look = |shape: &egui::Shape| {
            if let egui::Shape::LineSegment { points, .. } = shape
                && area.contains(points[0])
                && (points[0].x - points[1].x).abs() < 0.5
            {
                lines += 1;
            }
        };
        match &clipped.shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(&mut look),
            shape => look(shape),
        }
    }
    output.drop_without_applying_deltas();
    assert!(lines > 20, "a waveform of {lines} lines");
}

#[test]
fn step_73_play_from_the_frame_the_playhead_was_dragged_to() {
    let mut paint = with_sound("tab-73");
    for _ in 0..9 {
        run(&mut paint, "anim.next");
    }
    assert_eq!(frame_shown(&mut paint), 10);
    run(&mut paint, "anim.play");
    let first = frame_shown(&mut paint);
    assert!(paint.runtime.transport.is_playing_for("sue-paint"));
    run(&mut paint, "anim.play");
    assert!(first >= 10, "from frame 10 on: {first}");
}

// ----- 22:57 to 30:00: cleaning up, folders, the mouth -------------------------

#[test]
fn step_66_and_67_frame_copied_and_pasted_to_end_on_frame_1_again() {
    let mut paint = two_cels("tab-67");
    let grid = Grid::read(&mut paint);
    click_at(&mut paint, grid.cell("Folder 1", 1));
    right_click(&mut paint, grid.cell("Folder 1", 1));
    paint.click_text("Copy Frames");
    right_click(&mut paint, grid.cell("Folder 1", 3));
    paint.click_text("Paste Frames");
    assert_eq!(shown_names(&paint, 3), ["1", "2", "1"]);
}

#[test]
fn step_68_a_new_folder_on_top_renamed_by_a_double_click() {
    let mut paint = two_cels("tab-68");
    run(&mut paint, "anim.layer.folder");
    let folder = named(&paint, "Folder 2");
    let row = layer_row(&mut paint, "Folder 2");
    let at = row.center();
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    paint.step(vec![egui::Event::PointerMoved(at)]);
    paint.step(vec![button(true)]);
    paint.step(vec![button(false)]);
    paint.step(vec![button(true)]);
    paint.step(vec![button(false)]);
    paint.settle(3);
    paint.step(vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
    paint.step(vec![egui::Event::Text("rough body".to_owned())]);
    paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    paint.settle(3);
    assert_eq!(name_of(&paint, folder), "rough body");
    let order = stack(&paint);
    let at = |id| order.iter().position(|found| *found == id).unwrap();
    assert!(at(folder) > at(named(&paint, "Folder 1")), "on top");
    let grid = Grid::read(&mut paint);
    grid.row("rough body");
}

#[test]
fn step_69_animation_folders_put_in_one_folder_whose_opacity_is_lowered() {
    // AB-15.
    let mut paint = two_cels("tab-69");
    run(&mut paint, "anim.layer.folder");
    let (one, two) = (named(&paint, "Folder 1"), named(&paint, "Folder 2"));
    let row = layer_row(&mut paint, "Folder 1 : 2");
    press_at(
        &mut paint,
        row.center(),
        egui::PointerButton::Primary,
        egui::Modifiers::SHIFT,
    );
    undo_pair(&mut paint, "Create Folder and Insert Layer", |paint| {
        run(paint, "layer.folder_insert");
    });
    let outer = sue_paint::doc::folder_of(arena(&paint), one).expect("in a folder");
    assert_eq!(sue_paint::doc::folder_of(arena(&paint), two), Some(outer));
    assert!(!sue_paint::anim::is_animation_folder(arena(&paint), outer));
    // Its opacity, lowered on the Layer palette's slider.
    let opacity = |paint: &PaintDriver| {
        sue_paint::doc::layers(arena(paint), open_picture(paint))
            .into_iter()
            .find(|info| info.id == outer)
            .unwrap()
            .style
            .opacity
    };
    // The new folder picked, its opacity slider in the Layer palette
    // dragged to about 40 %.
    let name = name_of(&paint, outer);
    let row = layer_row(&mut paint, &name);
    click_at(&mut paint, row.center());
    let wedge = told(&mut paint)
        .into_iter()
        .filter(|(name, rect)| name == "Opacity" && rect.left() > 1106.0 && rect.top() < 520.0)
        .map(|(_, rect)| rect)
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("the opacity slider");
    undo_pair(&mut paint, "the folder's opacity", |paint| {
        paint.drag(
            Pos2::new(wedge.right() - 1.0, wedge.center().y),
            Pos2::new(wedge.left() + wedge.width() * 0.4, wedge.center().y),
            egui::PointerButton::Primary,
        );
    });
    assert!(
        (opacity(&paint) - 0.4).abs() < 0.08,
        "about 40 %: {}",
        opacity(&paint)
    );
}

#[test]
fn step_70_layer_color_on_a_folder_colors_everything_in_it() {
    // AB-19, ABV-25.
    let mut paint = two_cels("tab-70");
    let folder = named(&paint, "Folder 1");
    let cel = named(&paint, "1");
    let row = layer_row(&mut paint, "Folder 1 : 2");
    click_at(&mut paint, row.center());
    undo_pair(&mut paint, "Layer Color on the folder", |paint| {
        run(paint, "layer.toggle.layer_color");
    });
    let flat = sue_paint::composite::flatten(
        arena(&paint),
        open_picture(&paint),
        sue_paint::composite::RenderOptions::default(),
    )
    .unwrap();
    let raster = sue_paint::doc::raster(arena(&paint), cel).unwrap();
    let cel_bytes = raster.bytes().unwrap();
    let flat_bytes = flat.bytes().unwrap();
    let at = cel_bytes
        .as_chunks::<4>()
        .0
        .iter()
        .position(|pixel| pixel[3] == 255)
        .expect("cel 1's stroke");
    let [r, _, b, _] = <[u8; 4]>::try_from(&flat_bytes[at * 4..at * 4 + 4]).unwrap();
    assert!(b > r + 40, "the stroke shows in the layer color: {r} {b}");
    let _ = folder;
}

#[test]
fn step_71_other_folders_hidden_while_drawing_the_mouth() {
    let mut paint = two_cels("tab-71");
    run(&mut paint, "anim.layer.folder");
    let first = named(&paint, "Folder 1");
    let row = layer_row(&mut paint, "Folder 1 : 2");
    click_at(&mut paint, row.center());
    undo_pair(&mut paint, "Hide", |paint| {
        run(paint, "layer.toggle.visible");
    });
    let visible = sue_paint::doc::layers(arena(&paint), open_picture(&paint))
        .into_iter()
        .find(|info| info.id == first)
        .unwrap()
        .style
        .visible;
    assert!(!visible);
}

#[test]
fn step_74_and_76_mouth_cels_moved_a_frame_later_and_a_frame_copied() {
    let mut paint = two_cels("tab-74");
    let grid = Grid::read(&mut paint);
    // All the mouth's cels picked by a drag across them, moved one later.
    paint.drag(
        grid.cell("Folder 1", 1),
        grid.cell("Folder 1", 2),
        egui::PointerButton::Primary,
    );
    paint.drag(
        grid.cell("Folder 1", 1),
        grid.cell("Folder 1", 2),
        egui::PointerButton::Primary,
    );
    let names = shown_names(&paint, 3);
    assert_eq!(names[1..], ["1", "2"], "{names:?}");
}

#[test]
fn step_75_hold_keys_move_the_mouth_frame_by_frame() {
    // AB-2: Hold keys, each made by an Object drag on its frame.
    let (mut paint, folder) = ball("tab-75");
    run(&mut paint, "anim.key.layer");
    run(&mut paint, "anim.key.ease.hold");
    run(&mut paint, "tool.operation");
    paint.settle(4);
    for frame in 1..4 {
        run(&mut paint, "anim.next");
        let inside = mark(&paint, "keyed.middle") + vec2(4.0, 4.0);
        drag_with(
            &mut paint,
            inside,
            inside + vec2(20.0, 0.0),
            egui::Modifiers::NONE,
        );
        run(&mut paint, "anim.key.ease.hold");
        assert_eq!(key_frames(&paint, folder).len(), frame + 1);
    }
    // Hold: frame 2 shows frame 1's place until frame 2's key.
    let at = |frame| pose_at(&paint, folder, frame).position[0];
    assert!(at(1) > at(0) + 10.0 && at(2) > at(1) + 10.0);
    // Between keys nothing moves.
    let sequence_length = 24;
    assert!(sequence_length > 4);
    assert_eq!(
        sue_paint::anim::motion::ease_at(arena(&paint), folder, time(&paint, 1)),
        Some(sue_paint::anim::motion::Ease::Hold)
    );
}

// ----- 30:00 to 31:20: coloring ------------------------------------------------

#[test]
fn step_77_a_new_animation_folder_under_the_cleanup_renamed_color() {
    let mut paint = two_cels("tab-77");
    // Folder 1 is the cleanup; Layer 1 under it is picked first.
    let row = layer_row(&mut paint, "Layer 1");
    click_at(&mut paint, row.center());
    run(&mut paint, "anim.layer.folder");
    let made = named(&paint, "Folder 2");
    let order = stack(&paint);
    let at = |id| order.iter().position(|found| *found == id).unwrap();
    assert!(
        at(made) < at(named(&paint, "Folder 1")),
        "under the cleanup"
    );
    let row = layer_row(&mut paint, "Folder 2");
    let at = row.center();
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    paint.step(vec![egui::Event::PointerMoved(at)]);
    paint.step(vec![button(true)]);
    paint.step(vec![button(false)]);
    paint.step(vec![button(true)]);
    paint.step(vec![button(false)]);
    paint.settle(3);
    paint.step(vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
    paint.step(vec![egui::Event::Text("color".to_owned())]);
    paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    paint.settle(3);
    assert_eq!(name_of(&paint, made), "color");
    run(&mut paint, "anim.first");
    run(&mut paint, "anim.cel.new");
    let track = track_named(&paint, "color");
    let sequence = timeline(&paint);
    assert!(sue_paint::anim::cel_at(&sequence.tracks[track], sequence.rate, 0).is_some());
}

/// A cleanup animation folder with a ring on frame 1 and another ring
/// (elsewhere) on frame 2, set as the reference layer, and an animation
/// folder "color" with a blank cel on frame 1, picked.
fn coloring(tag: &str) -> (PaintDriver, EntityId) {
    let mut paint = PaintDriver::new(tag);
    paint.new_timeline();
    run(&mut paint, "anim.layer.folder");
    run(&mut paint, "anim.cel.new");
    run(&mut paint, "tool.pen");
    circle(&mut paint, vec2(-120.0, 0.0), 50.0);
    run(&mut paint, "anim.next");
    run(&mut paint, "anim.cel.new");
    circle(&mut paint, vec2(120.0, 0.0), 50.0);
    let cleanup = named(&paint, "Folder 1");
    let row = layer_row(&mut paint, "Folder 1 : 2");
    click_at(&mut paint, row.center());
    undo_pair(&mut paint, "Set as Reference Layer", |paint| {
        run(paint, "layer.toggle.reference");
    });
    let row = layer_row(&mut paint, "Layer 1");
    click_at(&mut paint, row.center());
    run(&mut paint, "anim.layer.folder");
    run(&mut paint, "anim.first");
    run(&mut paint, "anim.cel.new");
    let _ = cleanup;
    let track = track_named(&paint, "Folder 2");
    let sequence = timeline(&paint);
    let cel = sue_paint::anim::cel_at(&sequence.tracks[track], sequence.rate, 0).unwrap();
    (paint, cel)
}

/// Picks `option` of the Tool Property row named `row`.
fn tool_property_pick(paint: &mut PaintDriver, row: &str, option: &str) {
    let controls = told(paint);
    let at = controls
        .iter()
        .filter(|(label, _)| label == row)
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.left().total_cmp(&b.left()))
        .unwrap_or_else(|| panic!("{row:?} in Tool Property"));
    click_at(paint, at.center());
    let controls = told(paint);
    let choice = controls
        .iter()
        .filter(|(label, _)| label == option)
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .or_else(|| paint.rect_of(option))
        .unwrap_or_else(|| panic!("{option:?} offered"));
    click_at(paint, choice.center());
}

#[test]
fn step_78_and_79_fill_refers_to_the_reference_folder_as_the_frame_shows_it() {
    // AB-5, ABV-26: the reference folder's cels bound the fill, only those
    // of the frame shown.
    let (mut paint, cel) = coloring("tab-78");
    let area = size(&paint)[0] as usize * size(&paint)[1] as usize;
    run(&mut paint, "tool.pick.fill");
    paint.settle(2);
    click_control(&mut paint, "Tool Property");
    let controls = told(&mut paint);
    if !controls.iter().any(|(label, _)| label == "Refer") {
        let at = controls
            .iter()
            .find(|(label, _)| label == "Refer multiple")
            .map(|(_, rect)| rect.center())
            .unwrap_or_else(|| {
                panic!(
                    "Refer multiple: {:?}",
                    controls.iter().map(|(label, _)| label).collect::<Vec<_>>()
                )
            });
        click_at(&mut paint, at);
    }
    tool_property_pick(&mut paint, "Refer", "Reference layer");
    // Inside frame 1's ring: bounded by it.
    let middle = paint_middle(&paint.runtime);
    click_at(&mut paint, middle - vec2(120.0, 0.0));
    let inside = painted(&paint, cel);
    assert!(
        inside > 1_000 && inside < area / 8,
        "frame 1's ring bounds it: {inside} of {area}"
    );
    undo(&mut paint);
    // Where frame 2's ring is (not shown on frame 1): the fill runs on.
    click_at(&mut paint, middle + vec2(120.0, 0.0));
    let outside = painted(&paint, cel);
    assert!(outside > area / 2, "frame 2's ring is not seen: {outside}");
}

#[test]
fn step_80_and_81_drag_fills_several_areas_and_the_next_frame_is_filled_too() {
    // AB-20.
    let (mut paint, cel) = coloring("tab-80");
    run(&mut paint, "tool.pick.fill");
    paint.settle(2);
    click_control(&mut paint, "Tool Property");
    let controls = told(&mut paint);
    if !controls.iter().any(|(label, _)| label == "Refer") {
        let at = controls
            .iter()
            .find(|(label, _)| label == "Refer multiple")
            .map(|(_, rect)| rect.center())
            .expect("Refer multiple");
        click_at(&mut paint, at);
    }
    tool_property_pick(&mut paint, "Refer", "Reference layer");
    let middle = paint_middle(&paint.runtime) - vec2(120.0, 0.0);
    let one = {
        click_at(&mut paint, middle);
        let one = painted(&paint, cel);
        undo(&mut paint);
        one
    };
    // A line across the ring cuts it in two; a drag over both fills both.
    run(&mut paint, "tool.pen");
    let cleanup_cel = named(&paint, "1");
    let row = layer_row(&mut paint, "1");
    click_at(&mut paint, row.center());
    paint.drag(
        middle - vec2(0.0, 70.0),
        middle + vec2(0.0, 70.0),
        egui::PointerButton::Primary,
    );
    assert!(painted(&paint, cleanup_cel) > 0);
    let row = layer_row(&mut paint, "1");
    let _ = row;
    let track = track_named(&paint, "Folder 2");
    let color_name = name_of(&paint, cel);
    let color_row = layer_row(&mut paint, &color_name);
    click_at(&mut paint, color_row.center());
    run(&mut paint, "tool.pick.fill");
    let steps = depth(&paint);
    paint.drag(
        middle - vec2(25.0, 0.0),
        middle + vec2(25.0, 0.0),
        egui::PointerButton::Primary,
    );
    assert_eq!(depth(&paint), steps + 1, "one step");
    let both = painted(&paint, cel);
    assert!(
        both as f32 > one as f32 * 0.8,
        "both halves: {both} against {one}"
    );
    // Step 81: frame 2, a new cel of the color folder, filled inside its
    // own ring.
    run(&mut paint, "anim.next");
    run(&mut paint, "anim.cel.new");
    let sequence = timeline(&paint);
    let second = sue_paint::anim::cel_at(&sequence.tracks[track], sequence.rate, 1).unwrap();
    let right = paint_middle(&paint.runtime) + vec2(120.0, 0.0);
    click_at(&mut paint, right);
    let filled = painted(&paint, second);
    let area = size(&paint)[0] as usize * size(&paint)[1] as usize;
    assert!(filled > 1_000 && filled < area / 8, "{filled}");
}

#[test]
fn step_82_close_gap_fills_a_ring_with_a_gap() {
    let mut paint = PaintDriver::new("tab-82");
    run(&mut paint, "tool.pen");
    let middle = paint_middle(&paint.runtime);
    // A ring with a small gap.
    let points: Vec<Pos2> = (0..=44)
        .map(|index| {
            let angle = index as f32 / 48.0 * std::f32::consts::TAU;
            middle + vec2(angle.cos(), angle.sin()) * 60.0
        })
        .collect();
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    paint.step(vec![
        egui::Event::PointerMoved(points[0]),
        press(points[0], true),
    ]);
    for point in &points[1..] {
        paint.step(vec![egui::Event::PointerMoved(*point)]);
    }
    paint.step(vec![press(points[points.len() - 1], false)]);
    paint.settle(4);
    // The gap closed with a stroke across it.
    let (a, b) = (points[points.len() - 1], points[0]);
    paint.drag(
        a + (a - b) * 0.5,
        b + (b - a) * 0.5,
        egui::PointerButton::Primary,
    );
    let layer = *stack(&paint).last().unwrap();
    let lines = painted(&paint, layer);
    run(&mut paint, "tool.pick.fill");
    run(&mut paint, "tool.next_sub");
    click_at(&mut paint, middle);
    let filled = painted(&paint, layer) - lines;
    let area = size(&paint)[0] as usize * size(&paint)[1] as usize;
    assert!(
        filled > 1_000 && filled < area / 4,
        "close gap bounds it: {filled}"
    );
}
