//! Sue Paint: the Select menu's commands, each run in the running app.

use super::*;

/// A canvas open in Sue Paint, the helpers the tests below share, and
/// the way to drive it.
struct Canvas {
    runtime: Runtime,
    root: PathBuf,
    ctx: egui::Context,
    paint: AppletId,
    picture: sue_core::EntityId,
    start: Instant,
    now: Instant,
}

impl Canvas {
    /// A new canvas (File, New, OK) in a fresh runtime.
    fn new(tag: &str) -> Self {
        let (mut runtime, root) = runtime(tag);
        let ctx = egui::Context::default();
        let start = Instant::now();
        let paint = AppletId::new(sue_paint::ID).unwrap();
        runtime.request(ShellRequest::Open(paint.clone()), &mut NoTextures, start);
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), start, |_| {
            Vec::new()
        });
        let mut canvas = Self {
            runtime,
            root,
            ctx,
            paint,
            picture: sue_core::EntityId::NIL,
            start,
            now: start,
        };
        canvas.step(vec![key(egui::Key::N, egui::Modifiers::COMMAND)]);
        canvas.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
        canvas.step(Vec::new());
        canvas.picture = canvas
            .runtime
            .applets
            .get(&canvas.paint)
            .and_then(|applet| applet.focus())
            .unwrap();
        canvas
    }

    /// One frame with `events`.
    fn step(&mut self, events: Vec<egui::Event>) {
        frame_with(&mut self.runtime, &self.ctx, self.now, events);
        self.now += Duration::from_millis(8);
    }

    /// Runs `name` as a menu item does, then lets a frame pass.
    fn command(&mut self, name: &str) {
        self.runtime.request(
            ShellRequest::AppletCommand {
                applet: self.paint.clone(),
                command: name.to_owned(),
            },
            &mut NoTextures,
            self.start,
        );
        self.step(Vec::new());
        assert!(
            !self
                .runtime
                .shell
                .message()
                .is_some_and(|(message, _)| message.contains("not in Sue Paint")),
            "{name}: {:?}",
            self.runtime.shell.message()
        );
    }

    /// Runs `name`, which opens the dialog that asks by how much, and
    /// takes the dialog's default with OK.
    fn command_ok(&mut self, name: &str) {
        self.command(name);
        self.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
        self.step(Vec::new());
    }

    /// A drag with the primary button from `from` to `to`.
    fn drag(&mut self, from: Pos2, to: Pos2) {
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.step(vec![egui::Event::PointerMoved(from), press(from, true)]);
        for part in 1..=8 {
            let at = from + (to - from) * (part as f32 / 8.0);
            self.step(vec![egui::Event::PointerMoved(at)]);
        }
        self.step(vec![press(to, false)]);
    }

    fn arena(&self) -> &sue_core::Arena {
        &self.runtime.engine.world.arena
    }

    /// The whole arena as bytes: entities, ids, components, properties
    /// and relations.
    fn bytes(&self) -> Vec<u8> {
        let mut out = sue_core::codec::Writer::new();
        self.arena().encode(&mut out);
        out.into_bytes()
    }

    fn depth(&self) -> usize {
        self.runtime.engine.history.depth()
    }

    fn layers(&self) -> Vec<sue_core::EntityId> {
        sue_paint::doc::layers(self.arena(), self.picture)
            .into_iter()
            .map(|info| info.id)
            .collect()
    }

    /// The selection now, read by converting it to a selection layer and
    /// undoing that again: its mask, or `None` with nothing selected.
    fn selection(&mut self) -> Option<Vec<u8>> {
        let (before, depth, layers) = (self.bytes(), self.depth(), self.layers());
        self.command("select.to_layer");
        if self.depth() == depth {
            assert_eq!(self.bytes(), before, "nothing selected, nothing made");
            return None;
        }
        let made = self
            .layers()
            .into_iter()
            .find(|layer| !layers.contains(layer))
            .expect("a selection layer");
        let mask: Vec<u8> = sue_paint::doc::raster(self.arena(), made)
            .unwrap()
            .bytes()
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| pixel[3])
            .collect();
        self.command("edit.undo");
        assert_eq!(self.bytes(), before, "the probe leaves nothing behind");
        Some(mask)
    }

    /// Runs `name`, which changes the document, as one undo step: undo
    /// brings back the arena as it was, ids included, and redo the arena
    /// as the command left it.
    fn undo_pair(&mut self, name: &str) {
        let (before, depth) = (self.bytes(), self.depth());
        self.command(name);
        let after = self.bytes();
        assert_ne!(after, before, "{name} changes the document");
        assert_eq!(self.depth(), depth + 1, "{name} is one undo step");
        self.command("edit.undo");
        assert_eq!(self.bytes(), before, "{name} undone");
        self.command("edit.redo");
        assert_eq!(self.bytes(), after, "{name} redone");
    }

    /// Runs `name`, which leaves the document as it is: no undo step and
    /// the same arena.
    fn leaves_document(&mut self, name: &str) {
        let (before, depth) = (self.bytes(), self.depth());
        self.command(name);
        self.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
        self.step(Vec::new());
        assert_eq!(self.depth(), depth, "{name} puts nothing on the undo stack");
        assert_eq!(self.bytes(), before, "{name} leaves the document");
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// How many pixels of `mask` are selected at least half.
fn selected(mask: &[u8]) -> usize {
    mask.iter().filter(|alpha| **alpha >= 128).count()
}

#[test]
fn sue_paint_runs_every_select_command() {
    let mut canvas = Canvas::new("paint-select-all");
    // Every command of the group is one this test runs below.
    let names: Vec<&str> = sue_paint::registry()
        .iter()
        .filter(|command| command.group == "select")
        .map(|command| command.name)
        .collect();
    assert_eq!(
        names,
        [
            "select.from_layer",
            "select.all",
            "select.none",
            "select.again",
            "select.invert",
            "select.expand",
            "select.shrink",
            "select.feather",
            "select.gamut",
            "select.add_layer",
            "select.subtract_layer",
            "select.to_layer",
            "select.from_selection_layer",
            "select.quick_mask",
            "select.launcher",
            "select.vectors_overlapping",
            "select.vectors_within",
        ]
    );
    let [width, height] =
        sue_model::picture::size(canvas.arena().get_by_id(canvas.picture).unwrap()).unwrap();
    let pixels = (width * height) as usize;

    // Select All, Deselect, Reselect.
    assert!(
        canvas.selection().is_none(),
        "a new canvas has no selection"
    );
    canvas.leaves_document("select.all");
    let all = canvas.selection().expect("everything selected");
    assert_eq!(selected(&all), pixels);
    canvas.leaves_document("select.none");
    assert!(canvas.selection().is_none());
    canvas.leaves_document("select.again");
    assert!(
        canvas.selection() == Some(all.clone()),
        "the same selection back"
    );

    // Shrink, Invert, Expand, Blur Border: each through its dialog's OK.
    canvas.command_ok("select.shrink");
    let shrunk = canvas.selection().expect("still a selection");
    let inside = selected(&shrunk);
    assert!(
        inside < pixels && inside > pixels / 2,
        "{inside} of {pixels}"
    );
    assert_eq!(shrunk[0], 0, "the corner is let go");
    canvas.leaves_document("select.invert");
    let inverted = canvas.selection().expect("the border is selected");
    assert_eq!(selected(&inverted), pixels - inside);
    assert_eq!(inverted[0], 255);
    canvas.leaves_document("select.invert");
    assert!(canvas.selection() == Some(shrunk.clone()), "inverted twice");
    canvas.command_ok("select.expand");
    let expanded = canvas.selection().unwrap();
    assert!(selected(&expanded) > inside, "expanded again");
    canvas.command_ok("select.feather");
    let blurred = canvas.selection().unwrap();
    assert!(
        blurred.iter().any(|alpha| *alpha > 0 && *alpha < 255),
        "a soft border"
    );
    // The commands that open a dialog change nothing until OK.
    for name in [
        "select.expand",
        "select.shrink",
        "select.feather",
        "select.gamut",
    ] {
        canvas.leaves_document(name);
    }
    assert!(canvas.selection() == Some(blurred));

    // Ink on the layer for the Selection from Layer commands.
    canvas.leaves_document("select.none");
    let middle = paint_middle(&canvas.runtime);
    canvas.step(vec![key(egui::Key::P, egui::Modifiers::NONE)]);
    canvas.drag(middle - vec2(60.0, 0.0), middle + vec2(60.0, 0.0));
    canvas.leaves_document("select.from_layer");
    let ink = canvas.selection().expect("the ink is selected");
    let inked = selected(&ink);
    assert!(inked > 0 && inked < pixels / 4, "{inked}");
    canvas.leaves_document("select.none");
    canvas.leaves_document("select.add_layer");
    assert!(canvas.selection() == Some(ink.clone()), "added to nothing");
    canvas.leaves_document("select.subtract_layer");
    let left = canvas.selection().expect("the soft edge of the ink");
    assert_eq!(selected(&left), 0, "and taken away again");
    canvas.leaves_document("select.all");
    canvas.leaves_document("select.subtract_layer");
    let rest = canvas.selection().unwrap();
    assert_eq!(selected(&rest), pixels - inked, "all but the ink");

    // Convert to Selection Layer and back, Quick Mask on and off.
    canvas.command_ok("select.shrink");
    let shrunk = canvas.selection().unwrap();
    canvas.undo_pair("select.to_layer");
    // Undo left the layer below current; the selection layer lies on top.
    canvas.command("layer.select_above");
    canvas.leaves_document("select.none");
    canvas.leaves_document("select.from_selection_layer");
    assert!(
        canvas.selection() == Some(shrunk.clone()),
        "the layer as a selection"
    );
    canvas.undo_pair("select.quick_mask");
    assert!(
        canvas.selection().is_none(),
        "Quick Mask holds the selection"
    );
    canvas.undo_pair("select.quick_mask");
    assert!(canvas.selection() == Some(shrunk), "and gives it back");

    // The Selection Launcher, hidden and shown; the vector commands are
    // grayed on a raster layer, so they run nothing and say nothing.
    canvas.leaves_document("select.launcher");
    canvas.leaves_document("select.launcher");
    for name in ["select.vectors_overlapping", "select.vectors_within"] {
        let before = canvas
            .runtime
            .shell
            .message()
            .map(|(text, _)| text.to_owned());
        canvas.leaves_document(name);
        assert_eq!(
            canvas
                .runtime
                .shell
                .message()
                .map(|(text, _)| text.to_owned()),
            before,
            "{name}"
        );
        assert!(
            canvas.selection().is_some(),
            "{name} leaves the selection alone"
        );
    }
}

#[test]
fn sue_paint_selects_vector_lines_by_the_selected_area() {
    let mut canvas = Canvas::new("paint-select-vectors");
    canvas.command("layer.new.vector");
    let layer = *canvas.layers().last().unwrap();
    assert!(sue_paint::vector::is_vector(canvas.arena(), layer));
    let lines = |canvas: &Canvas| {
        sue_paint::vector::lines(&sue_paint::vector::geometry(canvas.arena(), layer))
    };
    // A level line, and an upright one that crosses it.
    let middle = paint_middle(&canvas.runtime);
    canvas.step(vec![key(egui::Key::P, egui::Modifiers::NONE)]);
    canvas.drag(middle - vec2(100.0, 0.0), middle + vec2(100.0, 0.0));
    canvas.drag(middle - vec2(0.0, 100.0), middle + vec2(0.0, 100.0));
    let drawn = lines(&canvas);
    assert_eq!(drawn.len(), 2);
    let upright = |line: &sue_paint::vector::Line| {
        let (first, last) = (line.samples[0].at, line.samples.last().unwrap().at);
        (first[0] - last[0]).abs() < (first[1] - last[1]).abs()
    };
    assert!(!upright(&drawn[0]) && upright(&drawn[1]));

    // Without a selection the commands are grayed and do nothing.
    canvas.leaves_document("select.vectors_within");

    // A wide, low box round the level line: it holds the level line
    // whole and only a piece of the upright.
    canvas.step(vec![key(egui::Key::M, egui::Modifiers::NONE)]);
    canvas.drag(middle - vec2(140.0, 30.0), middle + vec2(140.0, 30.0));
    assert!(canvas.selection().is_some());

    // Within: the level line alone; Delete takes it away as one step.
    canvas.leaves_document("select.vectors_within");
    let depth = canvas.depth();
    canvas.command("edit.clear");
    assert_eq!(canvas.depth(), depth + 1);
    let left = lines(&canvas);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0], drawn[1], "the upright stays");
    canvas.command("edit.undo");
    assert_eq!(lines(&canvas), drawn);

    // Overlapping: both lines.
    canvas.leaves_document("select.vectors_overlapping");
    canvas.command("edit.clear");
    assert!(lines(&canvas).is_empty(), "both are picked");
    canvas.command("edit.undo");
    assert_eq!(lines(&canvas), drawn);

    // A box that misses both lines picks none and says so; the lines
    // stay as they were.
    canvas.leaves_document("select.none");
    canvas.step(vec![key(egui::Key::M, egui::Modifiers::NONE)]);
    canvas.drag(middle + vec2(30.0, 30.0), middle + vec2(80.0, 80.0));
    canvas.leaves_document("select.vectors_overlapping");
    assert!(
        canvas
            .runtime
            .shell
            .message()
            .is_some_and(|(message, _)| message.contains("No vector line")),
        "{:?}",
        canvas.runtime.shell.message()
    );
    assert_eq!(lines(&canvas), drawn);
}
