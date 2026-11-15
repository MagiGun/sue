//! Sue Paint: the Gradient tool and gradient layers, Close and fill's
//! Target color, Fill's and the Selection tool's Tool Property rows,
//! and Delete on a mask, driven through real frames. Every control is
//! found by its name in egui's accessibility tree, never by a fixed
//! position.

use super::*;
use sue_core::EntityId;

/// Sue Paint with a new canvas, and what the tests read of it.
struct Canvas {
    paint: PaintDriver,
    picture: EntityId,
    /// The keys held down for the frames that follow.
    held: egui::Modifiers,
}

impl Canvas {
    fn new(tag: &str) -> Self {
        let paint = PaintDriver::new(tag);
        let picture = paint
            .runtime
            .applets
            .get(&AppletId::new(sue_paint::ID).unwrap())
            .and_then(|applet| applet.focus())
            .unwrap();
        Self {
            paint,
            picture,
            held: egui::Modifiers::NONE,
        }
    }

    fn arena(&self) -> &sue_core::Arena {
        &self.paint.runtime.engine.world.arena
    }

    /// The picture's layers, bottom first, but for the paper and folders.
    fn layers(&self) -> Vec<EntityId> {
        sue_paint::doc::layers(self.arena(), self.picture)
            .into_iter()
            .filter(|info| info.kind != sue_model::picture::LayerKind::Paper)
            .filter(|info| !sue_paint::doc::is_folder(self.arena(), info.id))
            .map(|info| info.id)
            .collect()
    }

    fn top(&self) -> EntityId {
        *self.layers().last().unwrap()
    }

    /// The raster bytes of `id` (a layer or a mask).
    fn bytes(&self, id: EntityId) -> Vec<u8> {
        sue_paint::doc::raster(self.arena(), id)
            .and_then(|raster| raster.bytes().map(<[u8]>::to_vec))
            .unwrap_or_default()
    }

    /// How much each pixel of `id` covers: its alpha, or a gray mask's
    /// value.
    fn cover(&self, id: EntityId) -> Vec<u8> {
        let gray = sue_paint::doc::raster(self.arena(), id)
            .is_some_and(|raster| raster.format() == sue_core::PixelFormat::Gray8);
        let bytes = self.bytes(id);
        if gray {
            bytes
        } else {
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| pixel[3])
                .collect()
        }
    }

    /// How many pixels of `id` are not clear.
    fn painted(&self, id: EntityId) -> usize {
        self.cover(id)
            .into_iter()
            .filter(|cover| *cover > 0)
            .count()
    }

    /// How many pixels of `id` are partly covered.
    fn soft(&self, id: EntityId) -> usize {
        self.cover(id)
            .into_iter()
            .filter(|cover| *cover > 0 && *cover < 255)
            .count()
    }

    fn size(&self) -> [u32; 2] {
        sue_model::picture::size(self.arena().get_by_id(self.picture).unwrap()).unwrap()
    }

    fn area(&self) -> usize {
        let [width, height] = self.size();
        width as usize * height as usize
    }

    fn depth(&self) -> usize {
        self.paint.runtime.engine.history.depth()
    }

    fn command(&mut self, name: &str) {
        self.paint.command(name);
        self.paint.settle(3);
    }

    /// `tool` with its sub tool `steps` on from the one it has (each tool
    /// keeps its own; a new canvas starts on the first).
    fn sub(&mut self, tool: &str, steps: usize) {
        self.command("tool.eraser");
        self.command(tool);
        for _ in 0..steps {
            self.command("tool.next_sub");
        }
    }

    fn undo(&mut self) {
        self.frame(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
        self.settle(3);
    }

    fn redo(&mut self) {
        self.frame(vec![key(
            egui::Key::Z,
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        )]);
        self.settle(3);
    }

    /// One frame with `events` and the keys held, read through egui's
    /// accessibility tree: every widget with a name, and where it is.
    fn frame(&mut self, mut events: Vec<egui::Event>) -> Vec<(String, egui::Rect)> {
        events.insert(0, egui::Event::ModifiersChanged(self.held));
        let driver = &mut self.paint;
        driver.ctx.enable_accesskit();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1440.0, 900.0),
            )),
            events,

            time: Some(driver.ctx.input(|input| input.time) + 0.016),
            ..egui::RawInput::default()
        };
        driver.now += std::time::Duration::from_millis(16);
        let runtime = &mut driver.runtime;
        let now = driver.now;
        let output = driver
            .ctx
            .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
        driver.texts = text_rects(&output);
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

    fn settle(&mut self, count: usize) {
        for _ in 0..count {
            self.frame(Vec::new());
        }
    }

    /// Every widget named `name`, topmost first.
    fn widgets(&mut self, name: &str) -> Vec<egui::Rect> {
        let mut found: Vec<egui::Rect> = self
            .frame(Vec::new())
            .into_iter()
            .filter(|(label, _)| label == name)
            .map(|(_, rect)| rect)
            .collect();
        found.sort_by(|a, b| a.top().total_cmp(&b.top()));
        found
    }

    /// The Tool Property palette's body: where the sub tool's name is
    /// drawn over its rows (the rightmost column holding the name).
    fn widget(&mut self, name: &str) -> egui::Rect {
        self.widgets(name)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("{name:?} is on screen"))
    }

    /// A click at `at`, then a few frames.
    fn click(&mut self, at: Pos2) {
        self.trace(&[at]);
    }

    /// A press at `points[0]`, moves through the rest, a release at the
    /// last, with the keys held.
    fn trace(&mut self, points: &[Pos2]) {
        let held = self.held;
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: held,
        };
        self.frame(vec![
            egui::Event::PointerMoved(points[0]),
            press(points[0], true),
        ]);
        for point in &points[1..] {
            self.frame(vec![egui::Event::PointerMoved(*point)]);
        }
        self.frame(vec![press(points[points.len() - 1], false)]);
        self.settle(4);
    }

    /// A drag from `from` to `to` in ten steps.
    fn drag(&mut self, from: Pos2, to: Pos2) {
        let points: Vec<Pos2> = (0..=10)
            .map(|step| from + (to - from) * (step as f32 / 10.0))
            .collect();
        self.trace(&points);
    }

    /// Clicks the widget named `name`.
    fn click_widget(&mut self, name: &str) {
        let at = self.widget(name).center();
        self.click(at);
    }

    /// Picks `option` in the drop-down named `name`: opens it, then
    /// clicks the option in its list.
    fn pick(&mut self, rect: egui::Rect, option: &str) {
        self.click(rect.center());
        let at = self
            .widgets(option)
            .into_iter()
            .max_by(|a, b| a.top().total_cmp(&b.top()))
            .or_else(|| self.paint.rect_of(option))
            .unwrap_or_else(|| panic!("{option:?} in the open list"))
            .center();
        self.click(at);
    }

    fn middle(&self) -> Pos2 {
        paint_middle(&self.paint.runtime)
    }

    /// A closed ring drawn with the pen round `center`, its ends
    /// overlapping so the pen's tapers leave no gap.
    fn ring(&mut self, center: Pos2, radius: f32) {
        self.sub("tool.pen", 0);
        let ring: Vec<Pos2> = (0..=56)
            .map(|index| {
                let angle = index as f32 / 48.0 * std::f32::consts::TAU;
                center + vec2(angle.cos(), angle.sin()) * radius
            })
            .collect();
        self.trace(&ring);
    }

    /// Picks `option` on the row named `row`: a click on it where the row
    /// shows its options side by side, else from the row's drop-down.
    fn choose(&mut self, row: &str, option: &str) {
        let list = self.reveal(row);
        // Side by side: the option on the row's line, in its column.
        let side_by_side = self.widgets(option).into_iter().find(|rect| {
            (rect.center().y - list.center().y).abs() < list.height() / 2.0
                && rect.left() >= list.left() - 1.0
                && rect.right() <= list.right() + 1.0
        });
        match side_by_side {
            Some(rect) => self.click(rect.center()),
            None => self.pick(list, option),
        }
    }

    /// Scrolls Tool Property until the widget named `name` (the
    /// rightmost so named) shows whole, above the palette under it, and
    /// says where it is.
    fn reveal(&mut self, name: &str) -> egui::Rect {
        let tab = self.widget("Tool Property");
        let over = egui::pos2(tab.center().x, tab.bottom() + 120.0);
        for _ in 0..40 {
            let found = self.frame(Vec::new());
            let rect = found
                .iter()
                // In Tool Property's column, not another palette's.
                .filter(|(label, rect)| {
                    label == name && (rect.center().x - tab.center().x).abs() < 300.0
                })
                .map(|(_, rect)| *rect)
                .max_by(|a, b| a.left().total_cmp(&b.left()))
                .unwrap_or_else(|| panic!("{name:?} is laid out"));
            // The palette under Tool Property starts at its menu button.
            let limit = found
                .iter()
                .filter(|(label, rect)| {
                    label.ends_with(": palette menu")
                        && rect.left() < tab.right()
                        && rect.top() > tab.bottom() + 20.0
                })
                .map(|(_, rect)| rect.top())
                .fold(900.0_f32, f32::min);
            let top = tab.bottom() + 36.0;
            if rect.bottom() < limit - 2.0 && rect.top() > top {
                return rect;
            }
            let delta = if rect.bottom() >= limit - 2.0 {
                -40.0
            } else {
                40.0
            };
            self.frame(vec![
                egui::Event::PointerMoved(over),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, delta),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            self.settle(2);
        }
        panic!("{name:?} could not be scrolled into view");
    }

    /// Clicks the Tool Property control named `name`, scrolled into view.
    fn click_row(&mut self, name: &str) {
        let at = self.reveal(name).center();
        self.click(at);
    }

    /// Shows the Tool Property palette (a tab beside Sub Tool).
    fn tool_property(&mut self) {
        self.click_widget("Tool Property");
    }

    /// The box round what `id` covers, in its pixels: left, top, right,
    /// bottom; `None` when nothing is.
    fn bounds(&self, id: EntityId) -> Option<[u32; 4]> {
        let width = sue_paint::doc::raster(self.arena(), id)?.width();
        let cover = self.cover(id);
        let mut out: Option<[u32; 4]> = None;
        for (index, value) in cover.iter().enumerate() {
            if *value == 0 {
                continue;
            }
            let (x, y) = (index as u32 % width, index as u32 / width);
            out = Some(match out {
                None => [x, y, x, y],
                Some([l, t, r, b]) => [l.min(x), t.min(y), r.max(x), b.max(y)],
            });
        }
        out
    }

    /// The entity `id` as it is now (for undo comparisons).
    fn entity(&self, id: EntityId) -> sue_core::Entity {
        self.arena().get_by_id(id).unwrap().clone()
    }
}

/// The file named `name` anywhere under `root`.
fn find_file(root: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|file| file == name) {
            return Some(path);
        }
    }
    None
}

#[test]
fn sue_paint_gradient_makes_a_live_layer_of_its_shape_and_edge() {
    // FG-22, FGV-34: Where to create, the shape, the edge process; the
    // layer stays a gradient, its end is dragged again and Tool Property
    // changes it, each one undo step.
    let mut canvas = Canvas::new("paint-gradient-layer");
    let middle = canvas.middle();
    canvas.sub("tool.pick.gradient", 0);
    canvas.tool_property();
    canvas.choose("Where to create", "New gradient layer");
    canvas.choose("Shape", "Ellipse");
    canvas.choose("Edge process", "Repeat");
    let before = canvas.layers();
    let depth = canvas.depth();
    canvas.drag(middle - vec2(80.0, 0.0), middle + vec2(40.0, 0.0));
    let layers = canvas.layers();
    assert_eq!(layers.len(), before.len() + 1, "a new layer");
    let layer = canvas.top();
    assert_eq!(canvas.depth(), depth + 1, "one undo step");
    let made = canvas.entity(layer);
    assert_eq!(made.class(), sue_paint::doc::GRADIENT_LAYER);
    let gradient = sue_paint::doc::Gradient::of(&made);
    assert_eq!(gradient.look.shape, sue_paint::gradient::Shape::Ellipse);
    assert_eq!(gradient.look.edge, sue_paint::gradient::Edge::Repeat);
    let [from, to] = gradient.line;
    assert!(
        to[0] > from[0] && (to[1] - from[1]).abs() < 1.0,
        "{from:?} {to:?}"
    );
    // Undo takes it away, redo brings the same layer back.
    canvas.undo();
    assert_eq!(canvas.layers(), before);
    canvas.redo();
    assert_eq!(canvas.top(), layer);
    assert_eq!(canvas.entity(layer), made);

    // Undo and redo leave another layer current: the gradient layer is
    // picked again. On it, a press on its end drags that end alone.
    canvas.command("layer.select_above");
    let depth = canvas.depth();
    canvas.drag(middle + vec2(40.0, 0.0), middle + vec2(40.0, 60.0));
    assert_eq!(canvas.depth(), depth + 1);
    let moved = sue_paint::doc::Gradient::of(&canvas.entity(layer));
    assert_eq!(moved.line[0], from, "the start stays");
    assert!(moved.line[1][1] > to[1] + 10.0, "the end went down");
    assert_eq!(canvas.layers().len(), before.len() + 1, "no new layer");
    canvas.undo();
    assert_eq!(canvas.entity(layer), made, "undo puts the line back");

    // Tool Property edits the layer itself: Circle, one step.
    let depth = canvas.depth();
    canvas.choose("Shape", "Circle");
    canvas.settle(4);
    assert_eq!(canvas.depth(), depth + 1, "one step for the change");
    let round = sue_paint::doc::Gradient::of(&canvas.entity(layer));
    assert_eq!(round.look.shape, sue_paint::gradient::Shape::Circle);
    canvas.undo();
    assert_eq!(canvas.entity(layer), made);
}

#[test]
fn sue_paint_gradient_keeps_shift_to_its_steps() {
    // FGV-34: Step of angle, and Shift's 45 degrees.
    let mut canvas = Canvas::new("paint-gradient-steps");
    let middle = canvas.middle();
    canvas.sub("tool.pick.gradient", 0);
    canvas.tool_property();
    canvas.choose("Where to create", "New gradient layer");
    canvas.held = egui::Modifiers::SHIFT;
    // About 30 degrees down: kept to 45.
    canvas.drag(middle, middle + vec2(100.0, 58.0));
    canvas.held = egui::Modifiers::NONE;
    let gradient = sue_paint::doc::Gradient::of(&canvas.entity(canvas.top()));
    let [from, to] = gradient.line;
    let angle = (to[1] - from[1]).atan2(to[0] - from[0]).to_degrees();
    assert!((angle - 45.0).abs() < 0.5, "Shift keeps it to 45: {angle}");
}

#[test]
fn sue_paint_gradient_bar_and_sets_are_files_people_keep() {
    // FGV-34: the editable bar; gradient presets as set files (owner
    // note 12).
    let mut canvas = Canvas::new("paint-gradient-sets");
    let middle = canvas.middle();
    canvas.sub("tool.pick.gradient", 0);
    canvas.tool_property();
    // Dusk from Sue's own set, laid across the whole picture.
    let set = canvas.reveal("Gradient Set");
    canvas.pick(set, "Dusk (Sue)");
    let layer = canvas.top();
    canvas.drag(middle - vec2(340.0, 0.0), middle + vec2(340.0, 0.0));
    let width = sue_paint::doc::raster(canvas.arena(), layer)
        .unwrap()
        .width() as usize;
    let bytes = canvas.bytes(layer);
    let row = &bytes[width * 40 * 4..width * 41 * 4];
    let first = row.as_chunks::<4>().0.first().copied().unwrap();
    let last = row.as_chunks::<4>().0.last().copied().unwrap();
    assert!(
        first[2] > first[0] && first[2] > first[1],
        "Dusk starts deep blue: {first:?}"
    );
    assert!(
        last[0] > 200 && last[1] > 120,
        "and ends pale gold: {last:?}"
    );
    // A click on the bar adds a stop; Keep writes the gradient to a set
    // file of the person's own.
    let bar = canvas.reveal("Gradient Stops");
    canvas.click(egui::pos2(bar.left() + bar.width() * 0.3, bar.top() + 6.0));
    canvas.click_row("Keep in My Gradients");
    let file = find_file(&canvas.paint.root, "my_gradients.suegradients")
        .expect("My Gradients is written");
    let text = std::fs::read_to_string(file).unwrap();
    let line = text
        .lines()
        .find(|line| line.starts_with("gradient = "))
        .expect("a gradient line");
    assert_eq!(
        line.matches('@').count(),
        10,
        "Dusk's four stops and one more: {line}"
    );
}

#[test]
fn sue_paint_contour_fill_runs_from_the_outline_inward() {
    // FG-22: Contour line fill.
    let mut canvas = Canvas::new("paint-gradient-contour");
    let middle = canvas.middle();
    canvas.ring(middle, 60.0);
    let layer = canvas.top();
    let before = canvas.bytes(layer);
    let lines = canvas.painted(layer);
    // Contour Fill: the eleventh Gradient sub tool.
    canvas.sub("tool.pick.gradient", 10);
    let depth = canvas.depth();
    canvas.click(middle);
    assert_eq!(canvas.depth(), depth + 1);
    let filled = canvas.painted(layer);
    assert!(filled > lines + 1_000, "inside the ring is laid: {filled}");
    // The drawing color fades from the outline (opaque) to the middle
    // (clear): the ring's inside has opaque and see-through parts.
    let cover = canvas.cover(layer);
    let inside: Vec<u8> = cover
        .iter()
        .zip(before.as_chunks::<4>().0)
        .filter(|(_, old)| old[3] == 0)
        .map(|(new, _)| *new)
        .filter(|new| *new > 0)
        .collect();
    assert!(
        inside.iter().any(|value| *value > 200),
        "opaque at the outline"
    );
    assert!(
        inside.iter().any(|value| *value < 60),
        "faint in the middle"
    );
    canvas.undo();
    assert_eq!(canvas.bytes(layer), before);
}

#[test]
fn sue_paint_fill_takes_connected_pixels_and_anti_aliasing_from_tool_property() {
    // FGV-21, ABV-26, A25V-23: Apply to connected pixels only and
    // Anti-aliasing, set by their controls.
    let mut canvas = Canvas::new("paint-fill-connected");
    let middle = canvas.middle();
    let (left, right) = (middle - vec2(110.0, 0.0), middle + vec2(110.0, 0.0));
    canvas.ring(left, 50.0);
    canvas.ring(right, 50.0);
    let lines = canvas.top();
    canvas.command("layer.new");
    let layer = canvas.top();
    assert_ne!(layer, lines);
    // Refer Other Layers.
    canvas.sub("tool.pick.fill", 0);
    canvas.tool_property();
    let area = canvas.area();
    canvas.click(left);
    let one = canvas.painted(layer);
    assert!(one > 1_000 && one < area / 8, "one ring's inside: {one}");
    assert_eq!(canvas.soft(layer), 0, "no anti-aliasing: a hard edge");
    canvas.undo();
    // Off: every like pixel, both insides and all round.
    canvas.click_row("Apply to connected pixels only");
    canvas.click(left);
    let every = canvas.painted(layer);
    assert!(every > area / 2, "every clear pixel: {every}");
    canvas.undo();
    canvas.click_row("Apply to connected pixels only");
    // Anti-aliasing, strong: the edge is soft.
    canvas.choose("Anti-aliasing", "Strong");
    canvas.click(left);
    assert!(
        canvas.soft(layer) > 50,
        "a soft edge: {}",
        canvas.soft(layer)
    );
}

#[test]
fn sue_paint_fill_refers_to_the_layers_refer_multiple_names() {
    // ABV-26, FG-25, A25-4: Refer multiple's four choices, selected
    // layers and layers in a folder, and the Exclude row.
    let mut canvas = Canvas::new("paint-fill-refer");
    let middle = canvas.middle();
    canvas.ring(middle, 60.0);
    let lines = canvas.top();
    canvas.command("layer.new");
    let layer = canvas.top();
    // Refer Only Editing Layer: the empty layer has no lines.
    canvas.sub("tool.pick.fill", 1);
    canvas.tool_property();
    let area = canvas.area();
    let bounded = |canvas: &mut Canvas| {
        canvas.click(middle);
        let filled = canvas.painted(layer);
        canvas.undo();
        assert_eq!(canvas.painted(layer), 0, "undo empties it");
        filled < area / 4
    };
    assert!(!bounded(&mut canvas), "the editing layer alone is empty");
    // Refer multiple: its four choices.
    canvas.click_row("Refer multiple");
    let refer = canvas.reveal("Refer");
    canvas.click(refer.center());
    for choice in [
        "All layers",
        "Selected layers",
        "Layers in folder",
        "Reference layer",
    ] {
        assert!(
            canvas.paint.rect_of(choice).is_some(),
            "{choice} is offered: {:?}",
            canvas.paint.texts
        );
    }
    canvas.frame(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    canvas.settle(2);
    canvas.choose("Refer", "All layers");
    assert!(bounded(&mut canvas), "every layer: the ring bounds it");
    // Selected layers: only the editing layer until both are picked.
    canvas.choose("Refer", "Selected layers");
    assert!(!bounded(&mut canvas), "nothing else picked");
    canvas.command("layer.select_all");
    assert!(bounded(&mut canvas), "the ring's layer picked too");
    canvas.command("layer.select_none");
    // Layers in folder: the layers in the editing layer's folder (here,
    // the layers outside folders), so a ring in a folder is left out.
    canvas.choose("Refer", "Layers in folder");
    assert!(bounded(&mut canvas), "both outside folders");
    canvas.command("layer.select_below");
    canvas.command("layer.folder_insert");
    canvas.command("layer.select_above");
    canvas.command("layer.select_above");
    assert_eq!(
        sue_paint::doc::folders_around(canvas.arena(), layer).len(),
        0,
        "the empty layer stays outside the folder"
    );
    assert!(!bounded(&mut canvas), "the ring's layer is in a folder");
    // The Exclude row: draft layers left out.
    canvas.choose("Refer", "All layers");
    assert!(bounded(&mut canvas));
    let mut draft = sue_core::txn::SetProperty::new(lines, sue_model::picture::DRAFT, true);
    sue_core::ReversibleTransaction::apply(
        &mut draft,
        &mut canvas.paint.runtime.engine.world.arena,
    )
    .unwrap();
    assert!(bounded(&mut canvas), "drafts count unless left out");
    canvas.click_row("Leave out draft layers");
    assert!(!bounded(&mut canvas), "the draft ring is left out");
    canvas.click_row("Leave out draft layers");
    assert!(bounded(&mut canvas));
    // The editing layer left out: only the others are looked at, so the
    // ring (on another layer) still bounds, and nothing on the editing
    // layer does.
    canvas.click_row("Leave out the editing layer");
    assert!(bounded(&mut canvas), "the ring is on another layer");
}

#[test]
fn sue_paint_fill_up_to_vector_path_reaches_the_middle_of_the_line() {
    // A25V-23, FGV-33: Fill up to vector path.
    let mut canvas = Canvas::new("paint-fill-vector-path");
    let middle = canvas.middle();
    canvas.command("layer.new.vector");
    canvas.ring(middle, 60.0);
    canvas.command("layer.new");
    let layer = canvas.top();
    canvas.sub("tool.pick.fill", 0);
    canvas.tool_property();
    canvas.click(middle);
    let to_the_line = canvas.painted(layer);
    assert!(
        to_the_line > 1_000,
        "the ring bounds the fill: {to_the_line}"
    );
    canvas.undo();
    canvas.click_row("Fill up to vector path");
    canvas.click(middle);
    let to_the_path = canvas.painted(layer);
    assert!(
        to_the_path > to_the_line + 100,
        "up to the line's middle: {to_the_path} against {to_the_line}"
    );
}

#[test]
fn sue_paint_selection_keeps_its_aspect_softens_and_turns_after_the_drag() {
    // FGV-20: Aspect, Anti-aliasing, Adjust angle after fixed.
    let mut canvas = Canvas::new("paint-selection-rows");
    let middle = canvas.middle();
    canvas.command("layer.new");
    let layer = canvas.top();
    canvas.sub("tool.selection", 0);
    canvas.tool_property();
    // Square: a wide drag selects a square.
    canvas.choose("Aspect", "Square");
    canvas.drag(middle - vec2(100.0, 40.0), middle + vec2(100.0, 40.0));
    canvas.command("edit.fill");
    let [l, t, r, b] = canvas.bounds(layer).expect("filled");
    let (w, h) = (r - l, b - t);
    assert!(w.abs_diff(h) <= 2, "square: {w} by {h}");
    assert_eq!(canvas.soft(layer), 0, "no anti-aliasing");
    canvas.undo();
    canvas.choose("Aspect", "Free");
    // Anti-aliasing, strong, on an ellipse: its edge is soft.
    canvas.choose("Shape", "Ellipse");
    canvas.choose("Anti-aliasing", "Strong");
    canvas.drag(middle - vec2(100.0, 40.0), middle + vec2(100.0, 40.0));
    canvas.command("edit.fill");
    assert!(canvas.soft(layer) > 50, "a soft edge");
    canvas.undo();
    canvas.command("select.none");
    canvas.choose("Anti-aliasing", "None");
    canvas.choose("Shape", "Rectangle");
    // A square, straight, to measure against.
    canvas.drag(middle - vec2(50.0, 50.0), middle + vec2(50.0, 50.0));
    canvas.command("edit.fill");
    let [l2, _, r2, _] = canvas.bounds(layer).expect("filled");
    let straight = (r2 - l2) as f32;
    canvas.undo();
    canvas.command("select.none");
    // Adjust angle after fixed: the square turns with the pointer until
    // a click; here an eighth of a turn.
    canvas.click_row("Adjust angle after fixed");
    canvas.drag(middle - vec2(50.0, 50.0), middle + vec2(50.0, 50.0));
    canvas.frame(vec![egui::Event::PointerMoved(middle + vec2(0.0, 70.0))]);
    canvas.click(middle + vec2(0.0, 70.0));
    canvas.command("edit.fill");
    let [l, t, r, b] = canvas.bounds(layer).expect("filled");
    let turned = (r - l) as f32;
    assert!(
        turned > straight * 1.3,
        "turned an eighth: {turned} across against {straight}, {t} to {b}"
    );
}

#[test]
fn sue_paint_delete_clears_the_mask_being_edited() {
    // CT-2: Edit, Delete while the mask is edited.
    let mut canvas = Canvas::new("paint-delete-mask");
    let middle = canvas.middle();
    canvas.ring(middle, 50.0);
    let layer = canvas.top();
    let pixels = canvas.bytes(layer);
    canvas.command("layer.mask.outside");
    let mask = sue_paint::doc::mask_of(canvas.arena(), layer).expect("a mask");
    let area = canvas.area();
    assert_eq!(canvas.painted(mask), area);
    canvas.command("layer.mask.edit");
    canvas.command("select.all");
    let depth = canvas.depth();
    let shown = canvas.entity(mask);
    canvas.command("edit.clear");
    assert_eq!(canvas.depth(), depth + 1, "one step");
    assert_eq!(canvas.painted(mask), 0, "the mask hides it all");
    assert_eq!(canvas.bytes(layer), pixels, "the layer is untouched");
    let hidden = canvas.entity(mask);
    canvas.undo();
    assert_eq!(canvas.entity(mask), shown);
    canvas.redo();
    assert_eq!(canvas.entity(mask), hidden);
}

#[test]
fn sue_paint_close_and_fill_fills_only_its_target_color() {
    // FGV-33: Close and fill's Target color, judged on the editing
    // layer; the lines that close the area are the other layers'.
    let mut canvas = Canvas::new("paint-close-target");
    let middle = canvas.middle();
    canvas.ring(middle, 50.0);
    canvas.command("layer.new");
    let layer = canvas.top();
    // The ring's inside painted on the new layer (Refer Other Layers).
    canvas.sub("tool.pick.fill", 0);
    canvas.click(middle);
    let painted = canvas.bytes(layer);
    let area = canvas.painted(layer);
    assert!(area > 1_000, "the inside is painted: {area}");
    canvas.command("color.swap");
    // Close and fill: two on from Refer Other Layers.
    canvas.sub("tool.pick.fill", 2);
    canvas.tool_property();
    let round: Vec<Pos2> = (0..=40)
        .map(|index| {
            let angle = index as f32 / 40.0 * std::f32::consts::TAU;
            middle + vec2(angle.cos(), angle.sin()) * 90.0
        })
        .collect();
    // Only transparent: what is painted inside keeps its color; only the
    // clear pixels left round it (under the ring's soft edge) fill.
    canvas.choose("Target color", "Only transparent");
    let depth = canvas.depth();
    canvas.trace(&round);
    let after = canvas.bytes(layer);
    let kept = painted
        .as_chunks::<4>()
        .0
        .iter()
        .zip(after.as_chunks::<4>().0)
        .filter(|(old, _)| old[3] > 0)
        .all(|(old, new)| old == new);
    assert!(kept, "no painted pixel took the new color");
    if after != painted {
        canvas.undo();
    }
    assert_eq!(canvas.depth(), depth);
    // Other than transparent: what is painted inside takes the new color,
    // as one step, and covers no more than it did.
    canvas.choose("Target color", "Other than transparent");
    canvas.trace(&round);
    assert_eq!(canvas.depth(), depth + 1);
    assert_ne!(canvas.bytes(layer), painted, "the painted inside is filled");
    assert_eq!(canvas.painted(layer), area, "and nothing past it");
    canvas.undo();
    assert_eq!(canvas.bytes(layer), painted);
}

#[test]
fn sue_paint_gradient_blends_by_its_mode_and_draws_nothing_past_its_end() {
    // FGV-34: blending mode and the edge process on the editing layer.
    let mut canvas = Canvas::new("paint-gradient-blend");
    let middle = canvas.middle();
    canvas.command("layer.new");
    let layer = canvas.top();
    // The layer black all over (Edit, Fill with the drawing color).
    canvas.command("edit.fill");
    let black = canvas.bytes(layer);
    // Main to Sub Color: black to white.
    canvas.sub("tool.pick.gradient", 1);
    canvas.tool_property();
    canvas.choose("Blending mode", "Multiply");
    let depth = canvas.depth();
    canvas.drag(middle - vec2(60.0, 0.0), middle + vec2(60.0, 0.0));
    let lightest_channel = canvas
        .bytes(layer)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[0].max(pixel[1]).max(pixel[2]))
        .max()
        .unwrap_or(0);
    assert!(
        lightest_channel <= 1,
        "white multiplied on black stays black: {lightest_channel}"
    );
    if canvas.depth() > depth {
        canvas.undo();
    }
    assert_eq!(canvas.bytes(layer), black);
    canvas.choose("Blending mode", "Normal");
    canvas.drag(middle - vec2(60.0, 0.0), middle + vec2(60.0, 0.0));
    let lightest = |bytes: &[u8]| {
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[0] > 250)
            .count()
    };
    let all_the_way = lightest(&canvas.bytes(layer));
    assert!(all_the_way > 1_000, "Normal lays white past the end");
    canvas.undo();
    // Do not draw: past the end nothing is laid, so far less turns white.
    canvas.choose("Edge process", "Do not draw");
    canvas.drag(middle - vec2(60.0, 0.0), middle + vec2(60.0, 0.0));
    let to_the_end = lightest(&canvas.bytes(layer));
    assert!(
        to_the_end * 4 < all_the_way,
        "only up to the end: {to_the_end} against {all_the_way}"
    );
}
