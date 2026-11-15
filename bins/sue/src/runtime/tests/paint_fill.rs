//! Sue Paint: the Fill, Auto Select, Selection area and Gradient tools on
//! the canvas (P4-1b), driven through real frames.

use super::*;
use sue_core::EntityId;

/// Sue Paint with a new canvas, and what the tests read of it.
struct Canvas {
    paint: PaintDriver,
    picture: EntityId,
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
        Self { paint, picture }
    }

    fn arena(&self) -> &sue_core::Arena {
        &self.paint.runtime.engine.world.arena
    }

    /// The picture's layers, bottom first, but for the paper.
    fn layers(&self) -> Vec<EntityId> {
        sue_paint::doc::layers(self.arena(), self.picture)
            .into_iter()
            .filter(|info| info.kind != sue_model::picture::LayerKind::Paper)
            .map(|info| info.id)
            .collect()
    }

    /// The raster bytes of `id` (a layer or a mask).
    fn bytes(&self, id: EntityId) -> Vec<u8> {
        sue_paint::doc::raster(self.arena(), id)
            .and_then(|raster| raster.bytes().map(<[u8]>::to_vec))
            .unwrap_or_default()
    }

    /// How much each pixel of `id` covers: its alpha, or a gray mask's
    /// value (masks are Gray8 since P4-1a).
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

    /// The picture's pixel count.
    fn area(&self) -> usize {
        let [width, height] =
            sue_model::picture::size(self.arena().get_by_id(self.picture).unwrap()).unwrap();
        width as usize * height as usize
    }

    fn depth(&self) -> usize {
        self.paint.runtime.engine.history.depth()
    }

    fn command(&mut self, name: &str) {
        self.paint.command(name);
        self.paint.settle(2);
    }

    /// `tool` with its sub tool `steps` on from its first (each tool
    /// keeps its own pick; a new canvas starts on the first).
    fn sub(&mut self, tool: &str, steps: usize) {
        self.command("tool.eraser");
        self.command(tool);
        for _ in 0..steps {
            self.command("tool.next_sub");
        }
    }

    fn undo(&mut self) {
        self.paint
            .step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
        self.paint.settle(2);
    }

    /// A press at `points[0]`, moves through the rest, a release at the
    /// last.
    fn trace(&mut self, points: &[Pos2]) {
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.paint.step(vec![
            egui::Event::PointerMoved(points[0]),
            press(points[0], true),
        ]);
        for point in &points[1..] {
            self.paint.step(vec![egui::Event::PointerMoved(*point)]);
        }
        self.paint
            .step(vec![press(points[points.len() - 1], false)]);
        self.paint.settle(2);
    }

    fn click(&mut self, at: Pos2) {
        self.trace(&[at]);
    }

    fn middle(&self) -> Pos2 {
        paint_middle(&self.paint.runtime)
    }

    /// A closed ring drawn with the pen round `center`.
    fn ring(&mut self, center: Pos2, radius: f32) {
        self.sub("tool.pen", 0);
        let ring: Vec<Pos2> = (0..=48)
            .map(|index| {
                let angle = index as f32 / 48.0 * std::f32::consts::TAU;
                center + vec2(angle.cos(), angle.sin()) * radius
            })
            .collect();
        self.trace(&ring);
    }
}

#[test]
fn sue_paint_fill_drag_fills_every_area_it_crosses_in_one_undo_step() {
    // AB-20: drag to fill several areas.
    let mut canvas = Canvas::new("paint-fill-drag");
    let middle = canvas.middle();
    // A ring cut in two by a line from top to bottom.
    canvas.ring(middle, 60.0);
    canvas.trace(&[middle - vec2(0.0, 90.0), middle + vec2(0.0, 90.0)]);
    let layer = *canvas.layers().last().unwrap();
    let lines = canvas.painted(layer);
    assert!(lines > 100, "the lines are drawn: {lines}");
    let before = canvas.bytes(layer);

    // A click fills the left half alone.
    canvas.sub("tool.fill", 1);
    let depth = canvas.depth();
    canvas.click(middle - vec2(30.0, 0.0));
    let half = canvas.painted(layer) - lines;
    assert!(half > 300, "a click fills one half: {half}");
    assert_eq!(canvas.depth(), depth + 1);
    canvas.undo();
    assert_eq!(canvas.bytes(layer), before, "undo leaves the lines alone");

    // A drag from the left half into the right one fills both, as one
    // undo step, and nothing outside the ring.
    canvas.trace(&[
        middle - vec2(30.0, 0.0),
        middle - vec2(10.0, 0.0),
        middle + vec2(10.0, 0.0),
        middle + vec2(30.0, 0.0),
    ]);
    let both = canvas.painted(layer) - lines;
    assert!(
        both as f32 > half as f32 * 1.7,
        "a drag fills both halves: {both} against one half's {half}"
    );
    assert!(both < canvas.area() / 4, "and not round the ring: {both}");
    assert_eq!(canvas.depth(), depth + 1, "one undo step");
    let after = canvas.bytes(layer);
    canvas.undo();
    assert_eq!(canvas.bytes(layer), before, "undo takes both back");
    canvas.paint.step(vec![key(
        egui::Key::Z,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    )]);
    canvas.paint.settle(2);
    assert_eq!(canvas.bytes(layer), after, "redo lays both again");
}

#[test]
fn sue_paint_fill_edit_fill_and_gradient_write_to_the_mask_being_edited() {
    // CT-2: while the mask is edited, fills go to the mask: the
    // transparent color hides, any other color shows.
    let mut canvas = Canvas::new("paint-fill-mask");
    let middle = canvas.middle();
    canvas.ring(middle, 50.0);
    let layer = *canvas.layers().last().unwrap();
    let pixels = canvas.bytes(layer);
    canvas.command("layer.mask.outside");
    let mask = sue_paint::doc::mask_of(canvas.arena(), layer).expect("a mask");
    let area = canvas.area();
    assert_eq!(canvas.painted(mask), area, "the new mask shows everything");
    canvas.command("layer.mask.edit");

    // The bucket with the transparent color: the whole mask (one area)
    // hides, the layer's pixels stay as they were.
    canvas.command("color.transparent");
    canvas.sub("tool.fill", 1);
    let depth = canvas.depth();
    canvas.click(middle + vec2(200.0, 0.0));
    assert_eq!(canvas.painted(mask), 0, "the bucket hid the layer");
    assert_eq!(canvas.bytes(layer), pixels, "the layer is untouched");
    assert_eq!(canvas.depth(), depth + 1);
    canvas.undo();
    assert_eq!(canvas.painted(mask), area, "undo shows it again");
    canvas.command("color.transparent");

    // Edit > Fill with the drawing color shows what was hidden.
    canvas.command("color.transparent");
    canvas.click(middle + vec2(200.0, 0.0));
    assert_eq!(canvas.painted(mask), 0);
    canvas.command("color.transparent");
    let depth = canvas.depth();
    canvas.command("edit.fill");
    assert_eq!(canvas.depth(), depth + 1);
    assert_eq!(canvas.painted(mask), area, "Edit, Fill showed it all");
    assert_eq!(canvas.bytes(layer), pixels);

    // A gradient with the transparent color fades the layer out on the
    // mask: full cover at one end, none past the other.
    canvas.command("color.transparent");
    canvas.sub("tool.gradient", 0);
    let depth = canvas.depth();
    canvas.trace(&[middle - vec2(100.0, 0.0), middle, middle + vec2(100.0, 0.0)]);
    assert_eq!(canvas.depth(), depth + 1, "a gradient is one step");
    let alphas = canvas.cover(mask);
    assert!(alphas.contains(&0), "the gradient's start hides");
    assert!(alphas.contains(&255), "past its end the layer shows");
    assert_eq!(canvas.bytes(layer), pixels, "the layer is untouched");
}

#[test]
fn sue_paint_fill_and_auto_select_see_only_the_cels_of_the_frame_shown() {
    // A25-3, AB-5: an animation's other cels stay out of a fill that
    // refers to every layer, and an animation folder set as the
    // reference layer is what Auto Select refers to.
    let mut canvas = Canvas::new("paint-fill-frames");
    let middle = canvas.middle();
    let (left, right) = (middle - vec2(120.0, 0.0), middle + vec2(120.0, 0.0));
    // A cel at frame 1 with a ring on the left, one from frame 4 with a
    // ring on the right.
    canvas.command("anim.cel.new");
    canvas.ring(left, 50.0);
    for _ in 0..3 {
        canvas.command("anim.next");
    }
    canvas.command("anim.cel.new");
    canvas.ring(right, 50.0);
    let arena = canvas.arena();
    let folder = sue_paint::doc::layers(arena, canvas.picture)
        .into_iter()
        .find(|info| sue_paint::doc::is_folder(arena, info.id))
        .expect("the animation folder")
        .id;
    // The cel drawn last: the top layer that is not a folder.
    let cel = canvas
        .layers()
        .into_iter()
        .rev()
        .find(|id| !sue_paint::doc::is_folder(canvas.arena(), *id))
        .unwrap();
    let lines = canvas.painted(cel);
    let area = canvas.area();

    // Fill, Refer Other Layers, at frame 4, where the first cel's ring
    // was: frame 1's ring is not shown, so the fill reaches all round
    // the right ring.
    canvas.sub("tool.fill", 0);
    canvas.click(left);
    let filled = canvas.painted(cel) - lines;
    assert!(
        filled > area / 2,
        "the fill saw frame 1's ring: {filled} of {area}"
    );
    canvas.undo();

    // Auto Select, Refer Reference Layers, with the animation folder
    // as the reference: inside the ring of the frame shown it selects
    // that ring's inside, which Edit, Fill then fills.
    let mut mark = sue_core::txn::SetProperty::new(
        folder,
        sue_paint::doc::REFERENCE,
        sue_core::Value::Bool(true),
    );
    sue_core::ReversibleTransaction::apply(&mut mark, &mut canvas.paint.runtime.engine.world.arena)
        .unwrap();
    canvas.sub("tool.auto_select", 2);
    canvas.click(right);
    canvas.command("edit.fill");
    let inside = canvas.painted(cel) - lines;
    assert!(
        inside > 1_000 && inside < area / 8,
        "the reference folder's cel bounds the selection: {inside} of {area}"
    );
}

#[test]
fn sue_paint_shrink_selection_hugs_what_is_drawn_inside_the_lasso() {
    // CC-3: the seventh Selection area sub tool.
    let mut canvas = Canvas::new("paint-shrink-selection");
    let middle = canvas.middle();
    canvas.ring(middle, 40.0);
    let layer = *canvas.layers().last().unwrap();
    let ring = canvas.painted(layer);
    // Shrink selection round the ring, far outside it.
    canvas.sub("tool.selection", 6);
    let corners = [
        middle + vec2(-120.0, -120.0),
        middle + vec2(120.0, -120.0),
        middle + vec2(120.0, 120.0),
        middle + vec2(-120.0, 120.0),
        middle + vec2(-120.0, -119.0),
    ];
    let mut points = Vec::new();
    for pair in corners.windows(2) {
        for step in 0..10 {
            points.push(pair[0] + (pair[1] - pair[0]) * (step as f32 / 10.0));
        }
    }
    canvas.trace(&points);
    // Filled, the selection is the ring with the hole it closes in, far
    // less than the lasso's square.
    canvas.command("layer.new");
    let filled_layer = *canvas.layers().last().unwrap();
    assert_ne!(filled_layer, layer, "a new layer");
    canvas.command("edit.fill");
    let selected = canvas.painted(filled_layer);
    assert!(
        selected > ring,
        "the ring and its inside: {selected} against {ring}"
    );
    // The square is 240 screen pixels a side; the disc 80 across: its
    // share is about 0.09 of the square.
    let square = {
        canvas.undo();
        canvas.sub("tool.selection", 2);
        canvas.trace(&points);
        canvas.command("edit.fill");
        canvas.painted(filled_layer)
    };
    assert!(
        (selected as f32) < square as f32 * 0.2,
        "shrunk to the drawing: {selected} of the lasso's {square}"
    );
}
