//! Sue Paint's touch modes driven through real frames: Draw with Fingers,
//! the touch gesture settings, Studio Mode's edge key bar and command bar,
//! and Simple Mode on a phone. Controls are found by the name they give
//! assistive technology and where the frame put them, never by a fixed
//! position.

use super::*;
use egui::pos2;

/// One control a frame told assistive technology about: its name, where
/// it is and whether it is on.
type Told = (String, egui::Rect, Option<bool>);

/// Sue Paint in a window of `size` with the layout `interface` (`tablet`,
/// `phone`) and the touch layout `mode` (`simple`, `studio`), a picture
/// open, driven a frame at a time.
struct TouchDriver {
    runtime: Runtime,
    ctx: egui::Context,
    now: Instant,
    root: PathBuf,
    size: egui::Vec2,
    /// What the last frame told assistive technology.
    told: Vec<Told>,
}

impl TouchDriver {
    fn new(tag: &str, interface: &str, mode: &str, size: egui::Vec2) -> Self {
        let (mut runtime, root) = runtime(tag);
        for (key, value) in [
            ("display.interface", interface),
            (sue_paint::simple::MODE_KEY, mode),
        ] {
            runtime
                .engine
                .world
                .rice
                .set(key, sue_rice::RiceValue::Text(value.to_owned()))
                .unwrap();
        }
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let now = Instant::now();
        frames(&mut runtime, &ctx, size, now, |_| Vec::new());
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
            &mut NoTextures,
            now,
        );
        frames(&mut runtime, &ctx, size, now, |_| Vec::new());
        runtime.with_context(&mut NoTextures, now, |cx| {
            sue_paint::sample_picture(cx, 320, 240)
        });
        let mut driver = Self {
            runtime,
            ctx,
            now,
            root,
            size,
            told: Vec::new(),
        };
        driver.settle(30);
        driver
    }

    /// One frame with `events`.
    fn step(&mut self, events: Vec<egui::Event>) {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, self.size)),
            events,
            time: Some(self.ctx.input(|input| input.time) + 0.016),
            ..egui::RawInput::default()
        };
        self.now += Duration::from_millis(16);
        let runtime = &mut self.runtime;
        let now = self.now;
        let output = self
            .ctx
            .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
        self.told = output
            .platform_output
            .accesskit_update
            .iter()
            .flat_map(|update| update.nodes.iter())
            .filter_map(|(_, node)| {
                let label = node.label()?.to_owned();
                let bounds = node.bounds()?;
                let rect = egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                );
                let on = node
                    .toggled()
                    .map(|toggled| toggled == egui::accesskit::Toggled::True);
                Some((label, rect, on))
            })
            .collect();
        output.drop_without_applying_deltas();
    }

    /// `count` frames with nothing new.
    fn settle(&mut self, count: usize) {
        for _ in 0..count {
            self.step(Vec::new());
        }
    }

    /// The control called `name`, the lowest when there are several (a
    /// sheet lies over the bars).
    fn find(&self, name: &str) -> Option<&Told> {
        self.told
            .iter()
            .filter(|(label, rect, _)| label == name && rect.area() > 0.0)
            .max_by(|a, b| a.1.top().total_cmp(&b.1.top()))
    }

    /// Where `name` is.
    fn rect(&self, name: &str) -> egui::Rect {
        self.find(name)
            .unwrap_or_else(|| {
                let names: Vec<&str> = self.told.iter().map(|told| told.0.as_str()).collect();
                panic!("{name:?} is on screen: {names:?}")
            })
            .1
    }

    /// Whether `name` is on.
    fn on(&self, name: &str) -> Option<bool> {
        self.find(name).and_then(|told| told.2)
    }

    /// A tap of the mouse on `name`.
    fn tap(&mut self, name: &str) {
        let at = self.rect(name).center();
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.step(vec![egui::Event::PointerMoved(at)]);
        self.step(vec![press(true)]);
        self.step(vec![press(false)]);
        self.settle(20);
    }

    /// A finger (touch `id`) laid down at `from`, moved to `to` and
    /// lifted, as the window hands it over: each touch with the pointer
    /// it moves.
    fn finger(&mut self, id: u64, from: Pos2, to: Pos2) {
        self.step(touch_down(id, from, None));
        for index in 1..=12 {
            let at = from + (to - from) * (index as f32 / 12.0);
            self.step(touch_move(id, at, None));
        }
        self.step(touch_up(id, to));
        self.settle(20);
    }

    fn rice(&self) -> &sue_rice::RiceSchema {
        &self.runtime.engine.world.rice
    }
}

impl Drop for TouchDriver {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn touch(id: u64, phase: egui::TouchPhase, pos: Pos2, force: Option<f32>) -> egui::Event {
    egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(id),
        phase,
        pos,
        force,
    }
}

fn button(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn touch_down(id: u64, at: Pos2, force: Option<f32>) -> Vec<egui::Event> {
    vec![
        touch(id, egui::TouchPhase::Start, at, force),
        egui::Event::PointerMoved(at),
        button(at, true),
    ]
}

fn touch_move(id: u64, at: Pos2, force: Option<f32>) -> Vec<egui::Event> {
    vec![
        touch(id, egui::TouchPhase::Move, at, force),
        egui::Event::PointerMoved(at),
    ]
}

fn touch_up(id: u64, at: Pos2) -> Vec<egui::Event> {
    vec![
        touch(id, egui::TouchPhase::End, at, None),
        button(at, false),
    ]
}

/// The pixels of the picture's top layer.
fn layer_bytes(runtime: &Runtime) -> (Vec<u8>, u32) {
    let arena = &runtime.engine.world.arena;
    let picture = arena
        .iter()
        .find(|(_, entity)| sue_model::picture::is_picture(entity))
        .map(|(_, entity)| entity.id())
        .unwrap();
    let layer = sue_paint::doc::layers(arena, picture).last().unwrap().id;
    let raster = sue_paint::doc::raster(arena, layer).unwrap();
    (raster.bytes().unwrap().to_vec(), raster.width())
}

/// The leftmost column of pixels that differs between `before` and
/// `after`, rows `width` pixels wide.
fn leftmost_change(before: &[u8], after: &[u8], width: u32) -> Option<u32> {
    before
        .chunks(4)
        .zip(after.chunks(4))
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(index, _)| index as u32 % width)
        .min()
}

/// A stroke by touch `id` from `from` to `to` on the desktop driver.
fn stroke(paint: &mut PaintDriver, id: u64, force: Option<f32>, from: Pos2, to: Pos2) {
    paint.step(touch_down(id, from, force));
    for index in 1..=10 {
        let at = from + (to - from) * (index as f32 / 10.0);
        paint.step(touch_move(id, at, force));
    }
    paint.step(touch_up(id, to));
    paint.settle(10);
}

// FG-36 (01:04:28): the finger button, fingers only gesture.
#[test]
fn with_draw_with_fingers_off_a_finger_moves_the_view_and_only_the_pen_draws() {
    let mut paint = PaintDriver::new("touch-finger");
    // Touch 1 is a finger, touch 2 the pen, as the system says.
    sue_platform::stylus::mark_touch_for_test(1, false);
    sue_platform::stylus::mark_touch_for_test(2, true);
    let middle = paint_middle(&paint.runtime);
    let (from, to) = (middle - vec2(80.0, 0.0), middle + vec2(-20.0, 30.0));
    let depth = |paint: &PaintDriver| paint.runtime.engine.history.depth();

    // Fingers draw at first.
    let (blank, width) = layer_bytes(&paint.runtime);
    let start = depth(&paint);
    stroke(&mut paint, 1, None, from, to);
    assert_eq!(depth(&paint), start + 1, "the finger drew a stroke");
    assert_ne!(layer_bytes(&paint.runtime).0, blank);
    paint.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    paint.settle(4);
    assert_eq!(layer_bytes(&paint.runtime).0, blank);

    // Draw with Fingers off, as its command switches it.
    paint.command("touch.finger_draws");
    paint.settle(2);
    let rice = &paint.runtime.engine.world.rice;
    assert_eq!(
        rice.get_bool(sue_paint::studio_mode::FINGER_KEY),
        Some(false)
    );

    // The pen still draws; where its line lands on the picture is noted.
    let start = depth(&paint);
    stroke(&mut paint, 2, Some(0.6), from, to);
    assert_eq!(depth(&paint), start + 1, "the pen drew");
    let drawn = layer_bytes(&paint.runtime).0;
    let first = leftmost_change(&blank, &drawn, width).expect("the pen left paint");
    paint.step(vec![key(egui::Key::Z, egui::Modifiers::COMMAND)]);
    paint.settle(4);

    // A finger now leaves no paint and no step to undo: it moves the view.
    let start = depth(&paint);
    stroke(&mut paint, 1, None, middle, middle + vec2(120.0, 0.0));
    assert_eq!(depth(&paint), start, "a finger adds nothing to undo");
    assert_eq!(
        layer_bytes(&paint.runtime).0,
        blank,
        "a finger paints nothing"
    );

    // The same pen line lands further left on the picture, which moved
    // right under it.
    stroke(&mut paint, 2, Some(0.6), from, to);
    let again = layer_bytes(&paint.runtime).0;
    let moved = leftmost_change(&blank, &again, width).expect("the pen left paint");
    assert!(
        moved + 40 < first,
        "the view moved with the finger: the line starts at {moved}, before at {first}"
    );

    // Switched back on, a finger draws again.
    paint.command("touch.finger_draws");
    paint.settle(2);
    let start = depth(&paint);
    stroke(&mut paint, 1, None, from, to);
    assert_eq!(depth(&paint), start + 1);
    sue_platform::stylus::forget_for_test();
}

/// The zoom the window title shows, such as `65.3%`.
fn zoom_in_title(paint: &PaintDriver) -> String {
    let title = paint
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.window_title(&paint.runtime.engine.world.arena))
        .unwrap_or_default();
    title
        .split_whitespace()
        .find(|word| word.ends_with("%)"))
        .unwrap_or_else(|| panic!("a zoom in {title:?}"))
        .to_owned()
}

/// Two fingers land around `middle`, spread apart and lift.
fn pinch(paint: &mut PaintDriver, middle: Pos2) {
    let (one, two) = (middle - vec2(40.0, 0.0), middle + vec2(40.0, 0.0));
    paint.step(touch_down(5, one, None));
    paint.step(vec![touch(6, egui::TouchPhase::Start, two, None)]);
    for spread in 1..=8 {
        let d = vec2(10.0, 0.0) * spread as f32;
        paint.step(vec![
            touch(5, egui::TouchPhase::Move, one - d, None),
            touch(6, egui::TouchPhase::Move, two + d, None),
            egui::Event::PointerMoved(one - d),
        ]);
    }
    paint.step(vec![
        touch(6, egui::TouchPhase::End, two + vec2(80.0, 0.0), None),
        touch(5, egui::TouchPhase::End, one - vec2(80.0, 0.0), None),
        button(one - vec2(80.0, 0.0), false),
    ]);
    paint.settle(10);
}

// 01:04:44: the touch gesture settings, wired.
#[test]
fn the_two_finger_pinch_zooms_only_while_its_setting_allows_it() {
    let mut paint = PaintDriver::new("touch-pinch-setting");
    let middle = paint_middle(&paint.runtime);
    let set = |paint: &mut PaintDriver, value: &str| {
        paint
            .runtime
            .engine
            .world
            .rice
            .set(
                sue_paint::studio_mode::PINCH_KEY,
                sue_rice::RiceValue::Text(value.to_owned()),
            )
            .unwrap();
    };
    let was = zoom_in_title(&paint);
    set(&mut paint, "none");
    pinch(&mut paint, middle);
    assert_eq!(
        zoom_in_title(&paint),
        was,
        "a pinch set to Nothing keeps the zoom"
    );
    set(&mut paint, "scale");
    pinch(&mut paint, middle);
    assert_ne!(zoom_in_title(&paint), was, "a pinch zooms again");
}

// FG-36 (01:04:50): the edge key bar from either side; once see-through,
// twice pinned.
#[test]
fn the_edge_key_bar_swipes_in_see_through_then_pins_from_either_side() {
    let mut tablet = TouchDriver::new("touch-key-bar", "tablet", "studio", vec2(1180.0, 820.0));
    let pin = sue_lang::text("paint", "edge-key-bar-pin");
    let hide = sue_lang::text("paint", "edge-key-bar-hide");
    let ctrl = sue_lang::text("paint", "studio-key-ctrl");
    let tab = tablet.rect(&sue_lang::text("paint", "edge-key-bar"));
    assert!(tablet.find("Esc").is_none(), "the bar starts away");
    // Its tab sits on the right edge of the canvas; a finger swipes in
    // from just inside that edge.
    let edge = tab.right() - 4.0;
    let y = tab.top() - 120.0;
    tablet.finger(1, pos2(edge, y), pos2(edge - 90.0, y));
    assert!(tablet.find("Esc").is_some(), "one swipe brings the bar in");
    assert_eq!(tablet.on(&pin), Some(false), "see-through, not pinned");
    let peek = tablet.rect("Esc");
    assert!(peek.center().x > tablet.size.x / 2.0, "on the right");
    // A second swipe from the same edge, over the see-through bar, pins
    // it, and the canvas makes room.
    tablet.finger(2, pos2(edge, y), pos2(edge - 90.0, y));
    assert_eq!(tablet.on(&pin), Some(true), "two swipes pin it");
    // A latched key.
    tablet.tap(&ctrl);
    assert_eq!(tablet.on(&ctrl), Some(true), "Ctrl stays down");
    tablet.tap(&ctrl);
    assert_eq!(tablet.on(&ctrl), Some(false));
    // A swipe from the left edge takes it there.
    let column = tablet.rect(&sue_lang::text("paint", "studio-lasso"));
    let left = column.right() + 8.0;
    tablet.finger(3, pos2(left, y), pos2(left + 100.0, y));
    assert!(
        tablet.rect("Esc").center().x < tablet.size.x / 2.0,
        "the bar moved to the left: {:?}",
        tablet.rect("Esc")
    );
    // Put away, the tab brings it back see-through.
    tablet.tap(&hide);
    assert!(tablet.find("Esc").is_none(), "put away");
    tablet.tap(&sue_lang::text("paint", "edge-key-bar"));
    assert_eq!(tablet.on(&pin), Some(false), "back, see-through");
    // Its last button opens Settings, where T1 to T7 are chosen.
    tablet.tap(&sue_lang::text("paint", "edge-key-bar-choose"));
    assert!(
        tablet.runtime.shell.window_rect().is_some(),
        "Settings is open"
    );
}

// FG-36 (01:04:28, 01:06:57): the finger button and the timelapse on
// Studio Mode's command bar.
#[test]
fn studio_modes_command_bar_switches_draw_with_fingers_and_the_timelapse() {
    let mut tablet = TouchDriver::new("touch-command-bar", "tablet", "studio", vec2(1180.0, 820.0));
    let fingers = sue_lang::text("paint", "draw-with-fingers");
    let record = sue_lang::text("paint", "record-timelapse-2");
    assert_eq!(tablet.on(&fingers), Some(true));
    tablet.tap(&fingers);
    assert_eq!(tablet.on(&fingers), Some(false));
    assert_eq!(
        tablet.rice().get_bool(sue_paint::studio_mode::FINGER_KEY),
        Some(false)
    );
    assert_eq!(tablet.on(&record), Some(false));
    tablet.tap(&record);
    assert_eq!(tablet.on(&record), Some(true), "recording");
    // Undo and Redo name their two- and three-finger taps' keys too.
    assert!(tablet.find(&sue_lang::text("paint", "undo")).is_some());
}

// 01:02:49: the phone's compact interface.
#[test]
fn on_a_phone_simple_mode_keeps_its_bar_and_the_more_sheet_switches_fingers() {
    let mut phone = TouchDriver::new("touch-phone", "phone", "simple", vec2(430.0, 932.0));
    for name in [
        "undo",
        "redo",
        "draw-with-fingers",
        "simple-more",
        "start-back",
    ] {
        let rect = phone.rect(&sue_lang::text("paint", name));
        assert!(rect.width() >= 40.0, "{name} is thumb sized: {rect:?}");
    }
    for name in ["simple-swap", "simple-pens", "simple-brushes", "layers-3"] {
        assert!(
            phone.find(&sue_lang::text("paint", name)).is_some(),
            "{name}"
        );
    }
    phone.tap(&sue_lang::text("paint", "simple-more"));
    let studio = sue_lang::text("paint", "simple-to-studio");
    assert!(phone.find(&studio).is_some(), "the More sheet is open");
    // The sheet's own Draw with Fingers row (lower than the bar's button).
    phone.tap(&sue_lang::text("paint", "draw-with-fingers"));
    assert_eq!(
        phone.rice().get_bool(sue_paint::studio_mode::FINGER_KEY),
        Some(false)
    );
    phone.settle(30);
    assert!(phone.find(&studio).is_none(), "the sheet closed");
    // Switching to Studio Mode from the sheet.
    phone.tap(&sue_lang::text("paint", "simple-more"));
    phone.tap(&studio);
    assert_eq!(
        phone.rice().get_str(sue_paint::simple::MODE_KEY),
        Some("studio")
    );
}
