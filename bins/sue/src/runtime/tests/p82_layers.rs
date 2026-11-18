//! The Layer palette driven through real frames on a picture as people
//! have them: sixty layers, folders, masks and clipped layers. Picking
//! several with Ctrl and Shift and dragging them (into, out of and
//! between folders, to the very top and bottom), the mark of where a
//! dragged layer lands, renaming while scrolled, eyes swept in one drag,
//! locks, an opacity typed as a number, thumbnails after a stroke and
//! after undo, and the list staying where it was scrolled.

use super::*;
use sue_core::{EntityId, ReversibleTransaction};
use sue_model::picture;
use sue_paint::doc;

fn paint() -> AppletId {
    AppletId::new(sue_paint::ID).unwrap()
}

/// The canvas in focus.
fn canvas(driver: &PaintDriver) -> EntityId {
    driver
        .runtime
        .applets
        .get(&paint())
        .and_then(|applet| applet.focus())
        .expect("a canvas is open")
}

fn arena(driver: &PaintDriver) -> &sue_core::Arena {
    &driver.runtime.engine.world.arena
}

/// The layers, bottom first.
fn stack(driver: &PaintDriver) -> Vec<EntityId> {
    doc::layers(arena(driver), canvas(driver))
        .into_iter()
        .map(|info| info.id)
        .collect()
}

/// The layers' names as the palette lists them, top first, each with
/// the folders around it (innermost first).
fn listed(driver: &PaintDriver) -> Vec<String> {
    let arena = arena(driver);
    doc::layers(arena, canvas(driver))
        .into_iter()
        .rev()
        .map(|info| {
            let mut name = info.name.clone();
            for folder in doc::folders_around(arena, info.id) {
                name.push('<');
                name.push_str(arena.get_by_id(folder).map_or("?", |entity| entity.name()));
            }
            name
        })
        .collect()
}

fn named(driver: &PaintDriver, name: &str) -> EntityId {
    doc::layers(arena(driver), canvas(driver))
        .into_iter()
        .find(|info| info.name == name)
        .unwrap_or_else(|| panic!("a layer named {name:?}"))
        .id
}

/// A row of the Layer palette's list as a frame drew it.
#[derive(Clone, Debug)]
struct Row {
    name: String,
    rect: egui::Rect,
    /// Current or picked with the others.
    picked: bool,
}

/// Where a dragged layer would land, as a frame marked it.
#[derive(Clone, Debug, PartialEq)]
enum Mark {
    /// A line across the list at this height.
    Line(f32),
    /// The outline of this row: into its folder.
    Into(String),
    Nothing,
}

/// What one frame showed of the Layer palette.
struct Shown {
    /// The rows in the list's part of the screen, top first.
    rows: Vec<Row>,
    /// The list's part of the screen.
    list: egui::Rect,
    mark: Mark,
    /// Every named control: its name, where it is, and whether it is on.
    told: Vec<(String, egui::Rect, bool)>,
}

/// The small pictures (thumbnails) the frames uploaded, by their key:
/// a number for the pixels of the last upload.
#[derive(Default)]
struct Thumbs(std::collections::BTreeMap<u64, u64>);

impl sue_platform::TextureUpload for Thumbs {
    fn texture(
        &mut self,
        key: u64,
        _version: u64,
        size: [u32; 2],
        _channels: u8,
        pixels: &[u8],
        _nearest: bool,
    ) -> Option<egui::TextureId> {
        use std::hash::{Hash, Hasher};
        if size[0] <= 96 && size[1] <= 96 {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            pixels.hash(&mut hasher);
            self.0.insert(key, hasher.finish());
        }
        None
    }
}

/// One frame with `events` and `modifiers` held, read for the Layer
/// palette's rows, its mark and its controls; textures go to `textures`.
fn frame(
    driver: &mut PaintDriver,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
    textures: &mut dyn sue_platform::TextureUpload,
) -> Shown {
    driver.ctx.enable_accesskit();
    let mut events = events;
    events.insert(0, egui::Event::ModifiersChanged(modifiers));
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        events,
        time: Some(driver.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    driver.now += Duration::from_millis(16);
    let runtime = &mut driver.runtime;
    let now = driver.now;
    let output = driver.ctx.run_ui(raw, |ui| runtime.ui(ui, textures, now));
    driver.texts = text_rects(&output);
    let told: Vec<(String, egui::Rect, bool)> = output
        .platform_output
        .accesskit_update
        .iter()
        .flat_map(|update| update.nodes.iter())
        .filter_map(|(_, node)| {
            let bounds = node.bounds()?;
            Some((
                node.label()?.to_owned(),
                egui::Rect::from_min_max(
                    Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
                    Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
                ),
                node.toggled() == Some(egui::accesskit::Toggled::True)
                    || node.is_selected() == Some(true),
            ))
        })
        .collect();
    // The layers' names as the list drew them, each with the part of the
    // screen it was clipped to: the list's.
    let names: Vec<String> = doc::layers(arena(driver), canvas(driver))
        .into_iter()
        .map(|info| info.name)
        .collect();
    let mut texts: Vec<(String, egui::Rect, egui::Rect)> = Vec::new();
    let mut lines: Vec<(f32, [f32; 2], egui::Color32, egui::Rect)> = Vec::new();
    let mut outlines: Vec<(egui::Rect, egui::Color32, egui::Rect)> = Vec::new();
    for clipped in &output.shapes {
        let clip = clipped.clip_rect;
        let mut read = |shape: &egui::Shape| match shape {
            egui::Shape::Text(text) => {
                let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                // The copy of a dragged row that follows the pointer is
                // clipped to nothing less than the window: not a row.
                if clip.intersects(rect)
                    && clip.width() < 1000.0
                    && names.iter().any(|name| name == text.galley.text())
                {
                    texts.push((text.galley.text().to_owned(), rect, clip));
                }
            }
            egui::Shape::LineSegment { points, stroke }
                if (points[0].y - points[1].y).abs() < 0.5 && clip.contains(points[0]) =>
            {
                let (a, b) = (points[0].x, points[1].x);
                lines.push((points[0].y, [a.min(b), a.max(b)], stroke.color, clip));
            }
            egui::Shape::Rect(rect) if rect.stroke.width > 0.0 && clip.intersects(rect.rect) => {
                outlines.push((rect.rect, rect.stroke.color, clip));
            }
            _ => {}
        };
        match &clipped.shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(&mut read),
            shape => read(shape),
        }
    }
    output.drop_without_applying_deltas();
    driver.ctx.disable_accesskit();
    // The list: the smallest part of the screen lines were clipped to
    // that holds the most of the names (each name is clipped to its row).
    let holds = |outer: &egui::Rect, inner: &egui::Rect| outer.expand(0.5).contains_rect(*inner);
    let count = |clip: &egui::Rect| {
        texts
            .iter()
            .filter(|(_, _, text)| holds(clip, text))
            .count()
    };
    let list = lines
        .iter()
        .map(|(_, _, _, clip)| *clip)
        .max_by(|a, b| count(a).cmp(&count(b)).then(b.area().total_cmp(&a.area())))
        .unwrap_or(egui::Rect::NOTHING);
    // The lines across the list: the hairlines under the rows, all of one
    // color, and the mark of a drop in another.
    let across: Vec<(f32, egui::Color32)> = lines
        .iter()
        .filter(|(_, [left, right], _, clip)| *clip == list && right - left > list.width() * 0.8)
        .map(|(y, _, color, _)| (*y, *color))
        .collect();
    let hairline = across
        .iter()
        .map(|(_, color)| *color)
        .max_by_key(|color| across.iter().filter(|(_, other)| other == color).count());
    let mut unders: Vec<f32> = across
        .iter()
        .filter(|(_, color)| Some(*color) == hairline)
        .map(|(y, _)| *y)
        .collect();
    unders.sort_by(f32::total_cmp);
    unders.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    // A name is clipped to its row: a row all in sight gives the height.
    let height = texts
        .iter()
        .filter(|(_, _, clip)| holds(&list, clip))
        .map(|(_, _, clip)| clip.height())
        .fold(0.0, f32::max);
    let mut rows: Vec<Row> = texts
        .iter()
        .filter(|(_, _, clip)| holds(&list, clip))
        .map(|(name, text, _)| {
            // The hairline under the row is the first one under its name.
            let bottom = unders
                .iter()
                .copied()
                .find(|y| *y >= text.bottom() - 1.0)
                .unwrap_or(text.bottom());
            Row {
                name: name.clone(),
                rect: egui::Rect::from_min_max(
                    Pos2::new(list.left(), bottom - height),
                    Pos2::new(list.right(), bottom),
                ),
                picked: told.iter().any(|(told, _, on)| told == name && *on),
            }
        })
        .collect();
    rows.sort_by(|a, b| a.rect.top().total_cmp(&b.rect.top()));
    rows.dedup_by(|a, b| a.name == b.name);
    let line = across
        .iter()
        .find(|(_, color)| Some(*color) != hairline)
        .map(|(y, _)| Mark::Line(*y));
    let into = outlines
        .iter()
        .filter(|(rect, color, clip)| {
            *clip == list && Some(*color) != hairline && rect.width() > list.width() * 0.8
        })
        .find_map(|(rect, _, _)| {
            rows.iter()
                .find(|row| (row.rect.center().y - rect.center().y).abs() < 2.0)
                .map(|row| Mark::Into(row.name.clone()))
        });
    Shown {
        rows,
        list,
        mark: into.or(line).unwrap_or(Mark::Nothing),
        told,
    }
}

/// A frame with nothing new.
fn look(driver: &mut PaintDriver) -> Shown {
    frame(driver, Vec::new(), egui::Modifiers::NONE, &mut NoTextures)
}

/// A frame with the pointer moved to `at`.
fn point(driver: &mut PaintDriver, at: Pos2) -> Shown {
    frame(
        driver,
        vec![egui::Event::PointerMoved(at)],
        egui::Modifiers::NONE,
        &mut NoTextures,
    )
}

/// The names of the picked rows in the list, top first.
fn picked(driver: &mut PaintDriver) -> Vec<String> {
    look(driver)
        .rows
        .into_iter()
        .filter(|row| row.picked)
        .map(|row| row.name)
        .collect()
}

/// Where a press picks or drags a row: its text part, right of its eye,
/// box and thumbnail, `part` of the way down it (0 its top, 1 its
/// bottom).
fn grip(row: &Row, part: f32) -> Pos2 {
    Pos2::new(
        row.rect.right() - 70.0,
        row.rect.top() + row.rect.height() * part,
    )
}

fn button(at: Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    }
}

/// A click at `at` with `modifiers` held, then a few frames.
fn click(driver: &mut PaintDriver, at: Pos2, modifiers: egui::Modifiers) {
    let mut one = |events| {
        frame(driver, events, modifiers, &mut NoTextures);
    };
    one(vec![egui::Event::PointerMoved(at)]);
    one(vec![button(at, true, modifiers)]);
    one(vec![button(at, false, modifiers)]);
    look(driver);
    driver.settle(3);
}

/// One turn of the wheel over `at` (`delta` points, up is positive).
fn wheel(driver: &mut PaintDriver, at: Pos2, delta: f32) {
    driver.step(vec![egui::Event::PointerMoved(at)]);
    driver.step(vec![egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, delta),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
        source: egui::MouseWheelSource::Wheel,
    }]);
    driver.settle(2);
}

/// Whether `name` comes before (above) the first row on screen in the
/// list as it stands.
fn is_above(driver: &PaintDriver, shown: &Shown, name: &str) -> bool {
    let order: Vec<String> = listed(driver)
        .into_iter()
        .map(|entry| entry.split('<').next().unwrap_or_default().to_owned())
        .collect();
    let at = |name: &str| order.iter().position(|listed| listed == name);
    let first = shown.rows.first().and_then(|row| at(&row.name));
    at(name) <= first
}

/// The row of `name` when all of it is in the list's part of the screen.
fn whole_row(shown: &Shown, name: &str) -> Option<Row> {
    shown
        .rows
        .iter()
        .find(|row| {
            row.name == name
                && row.rect.top() >= shown.list.top() - 0.5
                && row.rect.bottom() <= shown.list.bottom() + 0.5
        })
        .cloned()
}

/// Turns the wheel until the layer named `name` has its whole row in
/// the list, and answers the row.
fn scroll_to(driver: &mut PaintDriver, name: &str) -> Row {
    for _ in 0..400 {
        let shown = look(driver);
        if let Some(row) = whole_row(&shown, name) {
            return row;
        }
        let delta = if is_above(driver, &shown, name) {
            30.0
        } else {
            -30.0
        };
        wheel(driver, shown.list.center(), delta);
    }
    panic!("{name:?} scrolls into the list: {}", report(driver));
}

/// Drags the row of `from` to the row of `to`, `part` of the way down
/// it, holding at the list's edge while `to` is out of sight (the list
/// scrolls by itself). Answers what the last frame before letting go
/// showed, then lets go.
fn drag_row(driver: &mut PaintDriver, from: &str, to: &str, part: f32) -> Shown {
    let start = grip(&scroll_to(driver, from), 0.5);
    driver.step(vec![egui::Event::PointerMoved(start)]);
    driver.step(vec![button(start, true, egui::Modifiers::NONE)]);
    // Far enough to be a drag.
    for step in 1..=4 {
        point(driver, start + vec2(0.0, 3.0 * step as f32));
    }
    let mut steady = 0;
    let mut last = start;
    let mut shown = look(driver);
    for _ in 0..1500 {
        let at = match whole_row(&shown, to) {
            Some(row) => grip(&row, part),
            None if is_above(driver, &shown, to) => Pos2::new(start.x, shown.list.top() + 6.0),
            None => Pos2::new(start.x, shown.list.bottom() - 6.0),
        };
        let there = whole_row(&shown, to).is_some() && (at - last).length() < 0.5;
        steady = if there { steady + 1 } else { 0 };
        last = at;
        shown = point(driver, at);
        if steady >= 3 {
            break;
        }
    }
    assert!(
        steady >= 3,
        "the row of {to:?} comes under the pointer: {}",
        report(driver)
    );
    driver.step(vec![button(last, false, egui::Modifiers::NONE)]);
    driver.settle(6);
    shown
}

/// The names right above and right below a mark's line among the rows
/// of `shown`, leaving out the rows in `moving`.
fn around_line(shown: &Shown, moving: &[&str]) -> (Option<String>, Option<String>) {
    let Mark::Line(y) = shown.mark else {
        panic!("a line marks the drop: {:?}", shown.mark);
    };
    let rest: Vec<&Row> = shown
        .rows
        .iter()
        .filter(|row| !moving.contains(&row.name.as_str()))
        .collect();
    let above = rest
        .iter()
        .rev()
        .find(|row| row.rect.center().y < y)
        .map(|row| row.name.clone());
    let below = rest
        .iter()
        .find(|row| row.rect.center().y > y)
        .map(|row| row.name.clone());
    (above, below)
}

/// The names (without their folders) right above and below the block
/// `moving` in the stack as it stands, top first.
fn around_block(driver: &PaintDriver, moving: &[&str]) -> (Option<String>, Option<String>) {
    let order: Vec<String> = listed(driver)
        .into_iter()
        .map(|entry| entry.split('<').next().unwrap_or_default().to_owned())
        .collect();
    let places: Vec<usize> = moving
        .iter()
        .map(|name| order.iter().position(|listed| listed == name).unwrap())
        .collect();
    let (first, last) = (*places.iter().min().unwrap(), *places.iter().max().unwrap());
    assert_eq!(
        last - first + 1,
        moving.len(),
        "the moved layers stand together: {order:?}"
    );
    (
        first.checked_sub(1).map(|at| order[at].clone()),
        order.get(last + 1).cloned(),
    )
}

/// The folders around the layer named `name`, innermost first.
fn folders_of(driver: &PaintDriver, name: &str) -> Vec<String> {
    let arena = arena(driver);
    doc::folders_around(arena, named(driver, name))
        .into_iter()
        .map(|folder| arena.get_by_id(folder).unwrap().name().to_owned())
        .collect()
}

/// Undo, with the stack back as `before`, and redo, with it as it was.
fn undo_and_redo(driver: &mut PaintDriver, before: &[String]) {
    let after = listed(driver);
    assert_ne!(after, before, "the drop moved something");
    driver.command("edit.undo");
    driver.settle(3);
    assert_eq!(listed(driver), before, "undo puts every layer back");
    driver.command("edit.redo");
    driver.settle(3);
    assert_eq!(listed(driver), after, "redo moves them again");
}

/// Sue Paint with a picture as people have them: `L01` (bottom) to
/// `L60` over the paper; `Folder A` holds L11 to L20, `Folder B` L31 to
/// L40 with `Folder C` (L35 to L37) inside it; L05 and L33 have masks;
/// L06 and L07 are clipped to L05 and L34 to L33. Every fifth layer has
/// paint.
fn studio(tag: &str) -> PaintDriver {
    let mut driver = PaintDriver::new(tag);
    let picture = canvas(&driver);
    let first = *stack(&driver).last().unwrap();
    let world = &mut driver.runtime.engine.world;
    sue_core::txn::Rename::new(first, "L01")
        .apply(&mut world.arena)
        .unwrap();
    let mut layers = vec![first];
    for index in 2..=60 {
        let (mut spawn, layer) = doc::add_raster_layer(
            &world.arena,
            picture,
            layers.last().copied(),
            format!("L{index:02}"),
            None,
        )
        .unwrap();
        spawn.apply(&mut world.arena).unwrap();
        layers.push(layer);
    }
    let of = |number: usize| layers[number - 1];
    for (name, members) in [
        ("Folder A", 11..=20),
        ("Folder B", 31..=40),
        ("Folder C", 35..=37),
    ] {
        let (mut edit, folder) =
            doc::new_folder(&world.arena, picture, Some(of(*members.end())), false);
        edit.apply(&mut world.arena).unwrap();
        sue_core::txn::Rename::new(folder, name)
            .apply(&mut world.arena)
            .unwrap();
        for number in members {
            if let Some(mut edit) =
                sue_paint::anim::drop_layer(&world.arena, picture, of(number), folder)
            {
                edit.apply(&mut world.arena).unwrap();
            }
        }
    }
    for number in [5, 33] {
        let (mut edit, _) = doc::new_mask(&world.arena, of(number), |x, _| {
            if x % 40 < 20 { 1.0 } else { 0.3 }
        })
        .unwrap();
        edit.apply(&mut world.arena).unwrap();
    }
    for number in [6, 7, 34] {
        sue_core::txn::SetProperty::new(of(number), picture::CLIP, true)
            .apply(&mut world.arena)
            .unwrap();
    }
    for number in (5..=60).step_by(5) {
        let raster = doc::raster_mut(&mut world.arena, of(number)).unwrap();
        let (width, height) = (raster.width(), raster.height());
        let at = [
            (number as u32 * 53) % (width - 80),
            (number as u32 * 31) % (height - 80),
        ];
        let bytes = raster.bytes_mut_in([at[0], at[1], 80, 80]).unwrap();
        for y in at[1]..at[1] + 80 {
            for x in at[0]..at[0] + 80 {
                let pixel = (y * width + x) as usize * 4;
                bytes[pixel..pixel + 4].copy_from_slice(&[(number * 4) as u8, 80, 160, 255]);
            }
        }
    }
    driver.settle(6);
    driver
}

/// What the list looks like, for a failed check.
fn report(driver: &mut PaintDriver) -> String {
    let shown = look(driver);
    format!(
        "the list {:?} shows {:?}, marked {:?}; the stack {:?}",
        shown.list,
        shown
            .rows
            .iter()
            .map(|row| (row.name.clone(), row.rect.top() as i32, row.picked))
            .collect::<Vec<_>>(),
        shown.mark,
        listed(driver)
    )
}

fn depth(driver: &PaintDriver) -> usize {
    driver.runtime.engine.history.depth()
}

/// A drag of `from` to `part` of the row of `to` that moves `moving`
/// (top first): the mark shown before letting go is where they land,
/// and undo and redo are exact.
fn drop_checked(driver: &mut PaintDriver, from: &str, to: &str, part: f32, moving: &[&str]) {
    let before = listed(driver);
    let steps = depth(driver);
    let shown = drag_row(driver, from, to, part);
    assert_eq!(
        depth(driver),
        steps + 1,
        "{from} onto {to}: one step: {}",
        report(driver)
    );
    match &shown.mark {
        Mark::Line(_) => {
            let marked = around_line(&shown, moving);
            let landed = around_block(driver, moving);
            // The list shows only some rows: an end of it off screen
            // says nothing.
            if marked.0.is_some() {
                assert_eq!(
                    landed.0, marked.0,
                    "{from} onto {to}: the row above the line"
                );
            }
            if marked.1.is_some() {
                assert_eq!(
                    landed.1, marked.1,
                    "{from} onto {to}: the row below the line"
                );
            }
            assert!(
                marked.0.is_some() || marked.1.is_some(),
                "{from} onto {to}: the line is beside a row"
            );
        }
        Mark::Into(folder) => {
            let (above, _) = around_block(driver, moving);
            assert_eq!(
                above.as_ref(),
                Some(folder),
                "{from} onto {to}: on top inside"
            );
            for name in moving {
                assert_eq!(
                    folders_of(driver, name).first(),
                    Some(folder),
                    "{name} is in {folder}"
                );
            }
        }
        Mark::Nothing => panic!("{from} onto {to}: the drop is marked: {}", report(driver)),
    }
    // In their order, top first.
    let order: Vec<String> = listed(driver)
        .into_iter()
        .map(|entry| entry.split('<').next().unwrap_or_default().to_owned())
        .filter(|name| moving.contains(&name.as_str()))
        .collect();
    assert_eq!(order, moving, "{from} onto {to}: in their order");
    // The list stays where the drop was made.
    let after = look(driver);
    assert!(
        after.rows.iter().any(|row| row.name == moving[0]),
        "{from} onto {to}: the moved row is still in sight: {}",
        report(driver)
    );
    undo_and_redo(driver, &before);
}

#[test]
fn picked_layers_dragged_together_land_where_the_mark_shows() {
    let mut driver = studio("p82-drag");
    // Ctrl and Shift pick several: L60 is the layer being edited.
    let row = scroll_to(&mut driver, "L60");
    click(&mut driver, grip(&row, 0.5), egui::Modifiers::NONE);
    assert_eq!(picked(&mut driver), ["L60"], "{}", report(&mut driver));
    let row = scroll_to(&mut driver, "L58");
    click(&mut driver, grip(&row, 0.5), egui::Modifiers::COMMAND);
    assert_eq!(picked(&mut driver), ["L60", "L58"]);
    let row = scroll_to(&mut driver, "L57");
    click(&mut driver, grip(&row, 0.5), egui::Modifiers::SHIFT);
    assert_eq!(picked(&mut driver), ["L60", "L59", "L58", "L57"]);
    let row = scroll_to(&mut driver, "L59");
    click(&mut driver, grip(&row, 0.5), egui::Modifiers::COMMAND);
    assert_eq!(picked(&mut driver), ["L60", "L58", "L57"]);
    // Dragged by one of them, all three go: above the row from its
    // upper half.
    let three = ["L60", "L58", "L57"];
    drop_checked(&mut driver, "L58", "L54", 0.25, &three);
    scroll_to(&mut driver, "L60");
    assert_eq!(
        picked(&mut driver),
        three,
        "the same layers stay picked after the move and its undo and redo"
    );
    // A row that is not picked goes alone: below a row from its lower
    // half.
    drop_checked(&mut driver, "L59", "L50", 0.75, &["L59"]);
    scroll_to(&mut driver, "L60");
    assert_eq!(picked(&mut driver), three, "and the picked ones stay");
    // Into a folder far down the list: the list scrolls by itself while
    // the row is held at its edge.
    drop_checked(&mut driver, "L45", "Folder A", 0.6, &["L45"]);
    assert_eq!(folders_of(&driver, "L45"), ["Folder A"]);
    // Out of a folder, to the very top.
    let top = listed(&driver)[0].clone();
    drop_checked(&mut driver, "L15", &top, 0.25, &["L15"]);
    assert!(folders_of(&driver, "L15").is_empty());
    assert_eq!(listed(&driver)[0], "L15");
    // From one folder into a folder inside another.
    drop_checked(&mut driver, "L18", "L36", 0.25, &["L18"]);
    assert_eq!(folders_of(&driver, "L18"), ["Folder C", "Folder B"]);
    // To the bottom of a folder: under its last layer.
    drop_checked(&mut driver, "L52", "L11", 0.75, &["L52"]);
    assert_eq!(folders_of(&driver, "L52"), ["Folder A"]);
    assert_eq!(
        around_block(&driver, &["L52"]),
        (Some("L11".to_owned()), Some("L10".to_owned()))
    );
    // Above a folder's row from its top edge: beside it, not in it.
    drop_checked(&mut driver, "L51", "Folder A", 0.1, &["L51"]);
    assert!(folders_of(&driver, "L51").is_empty());
    // To the very bottom: under the lowest layer, and the paper stays
    // under everything.
    drop_checked(&mut driver, "L53", "L01", 0.75, &["L53"]);
    assert_eq!(
        around_block(&driver, &["L53"]),
        (Some("L01".to_owned()), Some("Paper".to_owned()))
    );
    drop_checked(&mut driver, "L49", "Paper", 0.75, &["L49"]);
    assert_eq!(listed(&driver).last().map(String::as_str), Some("Paper"));
}

#[test]
fn a_drop_that_moves_nothing_is_no_step() {
    let mut driver = studio("p82-nothing");
    // The list scrolled down: rows lie out of sight above it, under the
    // palette's buttons.
    scroll_to(&mut driver, "L40");
    let before = listed(&driver);
    let steps = depth(&driver);
    // Let go over the buttons, well above the list.
    let shown = look(&mut driver);
    let start = grip(&scroll_to(&mut driver, "L42"), 0.5);
    driver.step(vec![egui::Event::PointerMoved(start)]);
    driver.step(vec![button(start, true, egui::Modifiers::NONE)]);
    let away = Pos2::new(start.x, shown.list.top() - 70.0);
    for step in 1..=10 {
        point(&mut driver, start + (away - start) * (step as f32 / 10.0));
    }
    let held = point(&mut driver, away);
    assert_eq!(held.mark, Mark::Nothing, "no row out of sight is marked");
    driver.step(vec![button(away, false, egui::Modifiers::NONE)]);
    driver.settle(6);
    assert_eq!(listed(&driver), before, "nothing moved");
    // Dropped where it already is (above the row right under it).
    drag_row(&mut driver, "L42", "L41", 0.25);
    assert_eq!(listed(&driver), before, "nothing moved");
    assert_eq!(depth(&driver), steps, "and Undo has no empty step");
}

/// The middle of the eye of `row`.
fn eye(shown: &Shown, row: &Row) -> Pos2 {
    let x = shown
        .told
        .iter()
        .filter(|(name, rect, _)| {
            (name == "Hide Layer" || name == "Show Layer")
                && shown.list.x_range().contains(rect.center().x)
        })
        .map(|(_, rect, _)| rect.center().x)
        .fold(f32::MAX, f32::min);
    assert!(x < f32::MAX, "the rows have eyes");
    Pos2::new(x, row.rect.center().y)
}

fn visible(driver: &PaintDriver, name: &str) -> bool {
    let id = named(driver, name);
    doc::layers(arena(driver), canvas(driver))
        .iter()
        .find(|info| info.id == id)
        .is_some_and(|info| info.style.visible)
}

/// Presses on the eye of `names[0]` and moves over the eyes of the rest
/// before letting go.
fn sweep(driver: &mut PaintDriver, names: &[&str]) {
    let shown = look(driver);
    let eyes: Vec<Pos2> = names
        .iter()
        .map(|name| {
            eye(
                &shown,
                &whole_row(&shown, name).expect("the row is in sight"),
            )
        })
        .collect();
    driver.step(vec![egui::Event::PointerMoved(eyes[0])]);
    driver.step(vec![button(eyes[0], true, egui::Modifiers::NONE)]);
    for pair in eyes.windows(2) {
        for step in 1..=4 {
            point(driver, pair[0] + (pair[1] - pair[0]) * (step as f32 / 4.0));
        }
    }
    let last = *eyes.last().unwrap();
    driver.step(vec![button(last, false, egui::Modifiers::NONE)]);
    driver.settle(4);
}

#[test]
fn eyes_swept_in_one_drag_are_one_step() {
    let mut driver = studio("p82-eyes");
    scroll_to(&mut driver, "L60");
    let steps = depth(&driver);
    let four = ["L60", "L59", "L58", "L57"];
    sweep(&mut driver, &four);
    for name in four {
        assert!(!visible(&driver, name), "{name} is hidden");
    }
    assert!(visible(&driver, "L56"), "the row after the last stays");
    assert_eq!(depth(&driver), steps + 1, "the sweep is one step");
    driver.command("edit.undo");
    driver.settle(3);
    for name in four {
        assert!(visible(&driver, name), "undo shows {name} again");
    }
    driver.command("edit.redo");
    driver.settle(3);
    for name in four {
        assert!(!visible(&driver, name), "redo hides {name}");
    }
    // Swept back from another end: they show as the first one does.
    sweep(&mut driver, &["L58", "L59", "L60"]);
    assert!(visible(&driver, "L60") && visible(&driver, "L59") && visible(&driver, "L58"));
    assert!(!visible(&driver, "L57"));
    assert_eq!(depth(&driver), steps + 2);
    // A click on one eye is still one layer and one step.
    sweep(&mut driver, &["L57"]);
    assert!(visible(&driver, "L57"));
    assert_eq!(depth(&driver), steps + 3);
}

/// The names and tops of the rows in the list.
fn places(driver: &mut PaintDriver) -> Vec<(String, i32)> {
    look(driver)
        .rows
        .into_iter()
        .map(|row| (row.name, row.rect.top().round() as i32))
        .collect()
}

#[test]
fn rename_lock_and_a_typed_opacity_keep_the_list_where_it_was_scrolled() {
    let mut driver = studio("p82-scrolled");
    // Scrolled into the middle of the list.
    let row = scroll_to(&mut driver, "L30");
    click(&mut driver, grip(&row, 0.5), egui::Modifiers::NONE);
    let before = places(&mut driver);
    let id = named(&driver, "L30");
    assert!(
        !before.iter().any(|(name, _)| name == "L60"),
        "the list is scrolled: {before:?}"
    );
    // Rename by a double click, typed over, Enter.
    let at = grip(&row, 0.5);
    for pressed in [true, false, true, false] {
        driver.step(vec![button(at, pressed, egui::Modifiers::NONE)]);
    }
    driver.settle(4);
    driver.step(vec![egui::Event::Text("Ink".to_owned())]);
    driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    driver.settle(4);
    let name = |driver: &PaintDriver| arena(driver).get_by_id(id).unwrap().name().to_owned();
    assert_eq!(name(&driver), "Ink", "{}", report(&mut driver));
    let renamed: Vec<(String, i32)> = before
        .iter()
        .map(|(name, top)| {
            let name = if name == "L30" { "Ink" } else { name.as_str() };
            (name.to_owned(), *top)
        })
        .collect();
    assert_eq!(places(&mut driver), renamed, "the list did not move");
    driver.command("edit.undo");
    driver.settle(3);
    assert_eq!(name(&driver), "L30");
    assert_eq!(places(&mut driver), before, "nor on undo");
    assert_eq!(picked(&mut driver), ["L30"], "and the layer stays picked");
    // Lock, from the palette's button.
    let steps = depth(&driver);
    let lock = look(&mut driver)
        .told
        .iter()
        .find(|(name, _, _)| name == "Lock Layer")
        .map(|(_, rect, _)| rect.center())
        .expect("the Lock Layer button");
    click(&mut driver, lock, egui::Modifiers::NONE);
    let locked = |driver: &PaintDriver| {
        arena(driver)
            .get_by_id(id)
            .unwrap()
            .property(picture::LOCKED)
            .and_then(sue_core::Value::as_bool)
            .unwrap_or(false)
    };
    assert!(locked(&driver), "L30 is locked");
    assert_eq!(depth(&driver), steps + 1);
    assert_eq!(places(&mut driver), before, "the list did not move");
    driver.command("edit.undo");
    driver.settle(3);
    assert!(!locked(&driver));
    // Opacity typed as a number: the field at the right of the slider.
    let field = look(&mut driver)
        .told
        .iter()
        .filter(|(name, _, _)| name == "Opacity")
        .map(|(_, rect, _)| *rect)
        .max_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("the opacity field");
    let steps = depth(&driver);
    click(&mut driver, field.center(), egui::Modifiers::NONE);
    driver.step(vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
    driver.step(vec![egui::Event::Text("37".to_owned())]);
    driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
    driver.settle(4);
    let opacity = |driver: &PaintDriver| {
        picture::LayerStyle::of(arena(driver).get_by_id(id).unwrap()).opacity
    };
    assert!(
        (opacity(&driver) - 0.37).abs() < 0.005,
        "typed 37: {}",
        opacity(&driver)
    );
    assert_eq!(depth(&driver), steps + 1, "one step");
    assert_eq!(places(&mut driver), before, "the list did not move");
    driver.command("edit.undo");
    driver.settle(3);
    assert!((opacity(&driver) - 1.0).abs() < 0.005, "undo: opaque again");
    assert_eq!(places(&mut driver), before);
    assert_eq!(picked(&mut driver), ["L30"]);
}

/// A few frames whose thumbnails are kept.
fn thumbs(driver: &mut PaintDriver) -> std::collections::BTreeMap<u64, u64> {
    let mut kept = Thumbs::default();
    for _ in 0..3 {
        frame(driver, Vec::new(), egui::Modifiers::NONE, &mut kept);
    }
    kept.0
}

#[test]
fn thumbnails_follow_a_stroke_and_its_undo() {
    let mut driver = studio("p82-thumbs");
    let row = scroll_to(&mut driver, "L60");
    click(&mut driver, grip(&row, 0.5), egui::Modifiers::NONE);
    let before = thumbs(&mut driver);
    assert!(before.len() >= 4, "the rows in sight have thumbnails");
    // Strokes side by side on L60, the layer being edited: enough paint
    // for a small picture of the layer to show it.
    let steps = depth(&driver);
    let middle = paint_middle(&driver.runtime);
    for line in 0..24 {
        let from = middle + vec2(-80.0, -40.0 + 3.0 * line as f32);
        let to = from + vec2(160.0, 0.0);
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        driver.step(vec![egui::Event::PointerMoved(from)]);
        driver.step(vec![press(from, true)]);
        for step in 1..=4 {
            let at = from + (to - from) * (step as f32 / 4.0);
            driver.step(vec![egui::Event::PointerMoved(at)]);
        }
        driver.step(vec![press(to, false)]);
        driver.settle(2);
    }
    driver.settle(8);
    let strokes = depth(&driver) - steps;
    assert!(strokes >= 20, "the strokes were drawn: {strokes}");
    let painted = thumbs(&mut driver);
    let changed: Vec<u64> = before
        .iter()
        .filter(|(key, pixels)| painted.get(*key) != Some(*pixels))
        .map(|(key, _)| *key)
        .collect();
    assert_eq!(
        changed.len(),
        1,
        "the painted layer's thumbnail shows the strokes, and only its"
    );
    for _ in 0..strokes {
        driver.command("edit.undo");
        driver.settle(1);
    }
    driver.settle(4);
    assert_eq!(depth(&driver), steps);
    assert_eq!(
        thumbs(&mut driver),
        before,
        "undone, it shows the layer as it was"
    );
}
