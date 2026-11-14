//! How gestures on Sue Paint's canvas end, driven through the whole
//! runtime: a pen whose pressure comes from the tablet's driver and not
//! with the window's touch events (WinTab, XInput2, a browser) must not end
//! its strokes wider than it drew them, and every gesture ends where the
//! button was let go.

use std::path::PathBuf;
use std::time::Duration;

use egui::{Pos2, vec2};
use sue_platform::NoTextures;
use sue_platform::stylus::{self, PenSource};
use sue_platform::time::Instant;
use sue_wm::{AppletId, ShellRequest};

use crate::runtime::Runtime;
use crate::runtime::tests::{frames, runtime};

/// Sue Paint open on a new canvas in a 1440 by 900 window.
struct Paint {
    runtime: Runtime,
    ctx: egui::Context,
    now: Instant,
    root: PathBuf,
}

impl Paint {
    fn new(tag: &str) -> Self {
        let (mut runtime, root) = runtime(tag);
        let ctx = egui::Context::default();
        let now = Instant::now();
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
            &mut NoTextures,
            now,
        );
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        let mut paint = Self {
            runtime,
            ctx,
            now,
            root,
        };
        paint.step(vec![key(egui::Key::N, egui::Modifiers::COMMAND)]);
        paint.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
        paint.settle(10);
        paint
    }

    /// One frame with `events`, 16 thousandths of a second on.
    fn step(&mut self, events: Vec<egui::Event>) {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events,
            time: Some(self.ctx.input(|input| input.time) + 0.016),
            ..egui::RawInput::default()
        };
        self.now += Duration::from_millis(16);
        let (runtime, now) = (&mut self.runtime, self.now);
        self.ctx
            .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now))
            .drop_without_applying_deltas();
    }

    fn settle(&mut self, count: usize) {
        for _ in 0..count {
            self.step(Vec::new());
        }
    }

    /// Runs one of Sue Paint's commands, as its menus do.
    fn command(&mut self, command: &str) {
        self.runtime.request(
            ShellRequest::AppletCommand {
                applet: AppletId::new(sue_paint::ID).unwrap(),
                command: command.to_owned(),
            },
            &mut NoTextures,
            self.now,
        );
        self.step(Vec::new());
    }

    /// Where the canvas has its middle: the page's middle, moved by Sue
    /// Paint's palettes.
    fn middle(&self) -> Pos2 {
        self.runtime.shell.page_rect().center() + vec2(8.0, -63.0)
    }

    /// Presses the main button at `from` with `modifiers` held, moves to
    /// `to` in ten frames, and lets go at `lift` in a frame of its own
    /// with no move before it.
    fn drag(&mut self, from: Pos2, to: Pos2, lift: Pos2, modifiers: egui::Modifiers) {
        self.step(vec![egui::Event::PointerMoved(from)]);
        self.step(vec![button(from, true, modifiers)]);
        for index in 1..=10 {
            let at = from + (to - from) * (index as f32 / 10.0);
            self.step(vec![egui::Event::PointerMoved(at)]);
        }
        self.step(vec![button(lift, false, modifiers)]);
        self.settle(3);
    }

    /// Every layer of the picture, bottom first: where it lies and its
    /// pixels' width and bytes.
    fn layers(&self) -> Vec<([i64; 2], usize, Vec<u8>)> {
        let arena = &self.runtime.engine.world.arena;
        let picture = arena
            .iter()
            .find(|(_, entity)| sue_model::picture::is_picture(entity))
            .map(|(_, entity)| entity.id())
            .unwrap();
        sue_paint::doc::layers(arena, picture)
            .into_iter()
            .filter_map(|layer| {
                let raster = sue_paint::doc::raster(arena, layer.id)?;
                Some((
                    layer.style.offset,
                    raster.width() as usize,
                    raster.bytes()?.to_vec(),
                ))
            })
            .collect()
    }

    /// The top layer's ink as a line's profile: each painted column and
    /// how many of its pixels are more than half covered, left to right.
    fn profile(&self) -> Vec<(usize, usize)> {
        let layers = self.layers();
        let (_, width, bytes) = layers.last().unwrap();
        let height = bytes.len() / 4 / width;
        (0..*width)
            .map(|x| {
                let thick = (0..height)
                    .filter(|y| bytes[(y * width + x) * 4 + 3] > 128)
                    .count();
                (x, thick)
            })
            .filter(|(_, thick)| *thick > 0)
            .collect()
    }
}

impl Drop for Paint {
    fn drop(&mut self) {
        stylus::forget_for_test();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn button(pos: Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    }
}

/// How the tablet's driver and the window's pointer events fall in the
/// frame in which the pen lifts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lift {
    /// The driver says the pen has lifted, and the pen's last move and its
    /// button's release come in that same frame.
    SameFrame,
    /// The driver said so a frame before the button's release arrives, so
    /// by then no pressure at all is to be had.
    FrameLate,
    /// No pen: a mouse, which has no pressure from first to last.
    Mouse,
}

/// A line 240 points long with a bigger G-pen, pressed as hard as
/// `pressure` says and lifted as `lift` says, 3 points past its last move
/// as a real pen does: the line's profile.
fn stroke(tag: &str, pressure: f32, lift: Lift) -> Vec<(usize, usize)> {
    let mut paint = Paint::new(tag);
    // A bigger G-pen, so a full-pressure blob would show.
    for _ in 0..8 {
        paint.step(vec![key(egui::Key::CloseBracket, egui::Modifiers::NONE)]);
    }
    let pen = lift != Lift::Mouse;
    if pen {
        stylus::set_for_test(PenSource::WinTab, Some(pressure));
    }
    let start = paint.middle() - vec2(120.0, 0.0);
    paint.step(vec![
        egui::Event::PointerMoved(start),
        button(start, true, egui::Modifiers::NONE),
    ]);
    let mut at = start;
    for _ in 0..30 {
        at += vec2(8.0, 0.0);
        paint.step(vec![egui::Event::PointerMoved(at)]);
    }
    // The tip leaves the tablet: the driver's last packet of this frame
    // says so before the window's events of the frame are handled.
    if pen {
        stylus::set_for_test(PenSource::WinTab, None);
    }
    if lift == Lift::FrameLate {
        paint.step(Vec::new());
        stylus::forget_for_test();
    }
    let end = at + vec2(3.0, 0.0);
    paint.step(vec![
        egui::Event::PointerMoved(end),
        button(end, false, egui::Modifiers::NONE),
    ]);
    paint.settle(2);
    paint.profile()
}

/// The thickness of the line's middle and the most of its last 12 columns.
fn middle_and_end(profile: &[(usize, usize)]) -> (usize, usize) {
    assert!(profile.len() > 20, "the stroke was painted: {profile:?}");
    let middle = profile[profile.len() / 2].1;
    let end = profile[profile.len() - 12..]
        .iter()
        .map(|(_, thick)| *thick)
        .max()
        .unwrap();
    (middle, end)
}

/// The WinTab route of `sue_paint_a_light_pen_stroke_ends_light`: no touch
/// events at all, the pressure comes from the tablet's driver.
#[test]
fn sue_paint_a_light_driver_pen_stroke_ends_light() {
    let profile = stroke("paint-light-end-driver", 0.15, Lift::SameFrame);
    let (middle, end) = middle_and_end(&profile);
    assert!(
        end <= middle + 1,
        "the end ({end} px) is no blob over the middle ({middle} px): {:?}",
        &profile[profile.len() - 16..]
    );
}

/// The same when the button's release reaches the window a frame after the
/// driver said the pen lifted: nothing has the pen's pressure any more, and
/// the stroke ends with its own last pressure, not a full one.
#[test]
fn sue_paint_a_light_driver_pen_stroke_ends_light_a_frame_late() {
    let profile = stroke("paint-light-end-late", 0.15, Lift::FrameLate);
    let (middle, end) = middle_and_end(&profile);
    assert!(
        end <= middle + 1,
        "the end ({end} px) is no blob over the middle ({middle} px): {:?}",
        &profile[profile.len() - 16..]
    );
}

/// A mouse draws at full size from first to last, and ends no wider.
#[test]
fn sue_paint_a_mouse_stroke_ends_no_wider_than_it_is() {
    let profile = stroke("paint-mouse-end", 1.0, Lift::Mouse);
    let (middle, end) = middle_and_end(&profile);
    assert!(
        end <= middle + 1,
        "the end ({end} px) is no blob over the middle ({middle} px): {:?}",
        &profile[profile.len() - 16..]
    );
    let light = stroke("paint-mouse-end-light", 0.15, Lift::SameFrame);
    assert!(
        middle > middle_and_end(&light).0,
        "a mouse draws at the brush's full size"
    );
}

/// What a drag leaves when the button is let go somewhere the pointer was
/// never reported to have moved to: with `before` run first, the tool
/// picked by `tool`, the drag from `from` to `to` let go at `lift`, and
/// `after` run last.
fn gesture(
    tag: &str,
    before: bool,
    tool: &[&str],
    modifiers: egui::Modifiers,
    [to, lift]: [egui::Vec2; 2],
    after: &[&str],
) -> Vec<([i64; 2], usize, Vec<u8>)> {
    let mut paint = Paint::new(tag);
    let from = paint.middle() - vec2(100.0, 40.0);
    if before {
        // Something to move: a short line of the pen.
        paint.drag(
            from,
            from + vec2(60.0, 0.0),
            from + vec2(60.0, 0.0),
            egui::Modifiers::NONE,
        );
    }
    for command in tool {
        paint.command(command);
    }
    paint.drag(from, from + to, from + lift, modifiers);
    for command in after {
        paint.command(command);
    }
    paint.settle(3);
    paint.layers()
}

/// A gesture to try: its name, whether something is drawn first, the
/// commands that pick its tool, the keys held, and the commands run after.
type Case = (
    &'static str,
    bool,
    &'static [&'static str],
    egui::Modifiers,
    &'static [&'static str],
);

/// A release event is the gesture's last position even when no move to it
/// came first: a rectangle selection, a straight line (the Figure tool's,
/// and a pen's with Shift), a gradient and Move Layer end where the button
/// was let go.
#[test]
fn sue_paint_gestures_end_where_the_button_is_let_go() {
    let (near, far) = (vec2(120.0, 60.0), vec2(150.0, 84.0));
    let plain = egui::Modifiers::NONE;
    let cases: [Case; 5] = [
        ("select", false, &["tool.selection"], plain, &["edit.fill"]),
        ("figure-line", false, &["tool.figure"], plain, &[]),
        (
            "shift-line",
            false,
            &["tool.pen"],
            egui::Modifiers::SHIFT,
            &[],
        ),
        ("gradient", false, &["tool.fill", "tool.fill"], plain, &[]),
        ("move-layer", true, &["tool.move_layer"], plain, &[]),
    ];
    for (name, before, tool, modifiers, after) in cases {
        let run = |tag: &str, ends: [egui::Vec2; 2]| {
            gesture(
                &format!("paint-end-{name}-{tag}"),
                before,
                tool,
                modifiers,
                ends,
                after,
            )
        };
        let short = run("short", [near, near]);
        let long = run("long", [far, far]);
        let let_go = run("let-go", [near, far]);
        assert!(
            short != long,
            "{name}: the gesture's end shows in the picture"
        );
        assert!(
            let_go == long,
            "{name}: let go past the last move, it ends where it was let go"
        );
    }
}

/// A brush stroke too: its line reaches where the button was let go.
#[test]
fn sue_paint_a_stroke_reaches_where_the_button_is_let_go() {
    let last = |tag: &str, lift: f32| {
        let mut paint = Paint::new(tag);
        let from = paint.middle() - vec2(100.0, 0.0);
        let to = from + vec2(120.0, 0.0);
        paint.drag(from, to, to + vec2(lift, 0.0), egui::Modifiers::NONE);
        paint.profile().last().map_or(0, |(x, _)| *x)
    };
    let (at_move, past) = (
        last("paint-stroke-lift-0", 0.0),
        last("paint-stroke-lift-40", 40.0),
    );
    assert!(at_move > 0, "the stroke was painted");
    assert!(
        past >= at_move + 10,
        "let go 40 points on, the line ends at {past}, not at {at_move}"
    );
}

/// One frame of a sheet stroke: how far the pen moves in it (points) and
/// how hard it presses.
type Leg = (egui::Vec2, f32);

/// The strokes of the sheet, each as its frames: a light flick, a slow
/// lift, a fast lift and a hooked end.
fn sheet_strokes() -> [(&'static str, Vec<Leg>); 4] {
    let flick = (0..12)
        .map(|frame| (vec2(6.0 + frame as f32 * 2.2, -0.4 * frame as f32), 0.25))
        .collect();
    let slow = (0..30)
        .map(|frame| {
            let left = (30 - frame) as f32 / 30.0;
            (vec2(1.0 + 9.0 * left, 0.0), 0.15 + 0.4 * left)
        })
        .collect();
    let fast = (0..14).map(|_| (vec2(13.0, 0.0), 0.4)).collect();
    let mut hooked: Vec<Leg> = (0..22).map(|_| (vec2(8.0, 0.0), 0.4)).collect();
    hooked.extend([
        (vec2(1.0, 3.0), 0.3),
        (vec2(-2.0, 3.0), 0.2),
        (vec2(-2.0, 2.0), 0.1),
    ]);
    [
        ("light flick", flick),
        ("slow lift", slow),
        ("fast lift", fast),
        ("hooked end", hooked),
    ]
}

/// Writes a picture of stroke ends for a person to look at: four strokes
/// (rows: a light flick, a slow lift, a fast lift, a hooked end) by three
/// routes (columns: a driver's pen lifting in the release's frame, the
/// same with the release a frame late, a mouse), drawn through the whole
/// runtime with a bigger G-pen. `SUE_STROKE_ENDS` names the PNG file.
#[test]
#[ignore = "writes a picture of stroke ends for a person to look at"]
fn stroke_ends_sheet() {
    let mut paint = Paint::new("paint-stroke-ends-sheet");
    for _ in 0..8 {
        paint.step(vec![key(egui::Key::CloseBracket, egui::Modifiers::NONE)]);
    }
    let middle = paint.middle();
    let routes = [Lift::SameFrame, Lift::FrameLate, Lift::Mouse];
    for (row, (_, legs)) in sheet_strokes().into_iter().enumerate() {
        for (column, route) in routes.into_iter().enumerate() {
            let pen = route != Lift::Mouse;
            let start = middle + vec2(-330.0 + column as f32 * 195.0, -120.0 + row as f32 * 70.0);
            let first = legs.first().map_or(1.0, |leg| leg.1);
            if pen {
                stylus::set_for_test(PenSource::WinTab, Some(first));
            }
            paint.step(vec![egui::Event::PointerMoved(start)]);
            paint.step(vec![button(start, true, egui::Modifiers::NONE)]);
            let mut at = start;
            let mut step = vec2(0.0, 0.0);
            for (by, pressure) in &legs {
                at += *by;
                step = *by;
                if pen {
                    stylus::set_for_test(PenSource::WinTab, Some(*pressure));
                }
                paint.step(vec![egui::Event::PointerMoved(at)]);
            }
            if pen {
                stylus::set_for_test(PenSource::WinTab, None);
            }
            if route == Lift::FrameLate {
                paint.step(Vec::new());
                stylus::forget_for_test();
            }
            // The pen leaves the tablet a third of a frame's way on.
            let end = at + step * 0.35;
            paint.step(vec![
                egui::Event::PointerMoved(end),
                button(end, false, egui::Modifiers::NONE),
            ]);
            stylus::forget_for_test();
            paint.settle(3);
        }
    }
    // The ink over white, cut to where it is and three times as large.
    let layers = paint.layers();
    let (_, width, bytes) = layers.last().unwrap();
    let height = bytes.len() / 4 / width;
    let alpha = |x: usize, y: usize| bytes[(y * width + x) * 4 + 3];
    let inked: Vec<(usize, usize)> = (0..height)
        .flat_map(|y| (0..*width).map(move |x| (x, y)))
        .filter(|(x, y)| alpha(*x, *y) > 0)
        .collect();
    assert!(!inked.is_empty(), "the strokes were painted");
    let edge = |pick: fn(&(usize, usize)) -> usize| {
        let values = inked.iter().map(pick);
        (
            values.clone().min().unwrap_or(0).saturating_sub(12),
            values.max().unwrap_or(0) + 12,
        )
    };
    let ((x0, x1), (y0, y1)) = (edge(|at| at.0), edge(|at| at.1));
    let (x1, y1) = (x1.min(width - 1), y1.min(height - 1));
    let scale = 3;
    let (out_w, out_h) = ((x1 - x0 + 1) * scale, (y1 - y0 + 1) * scale);
    let mut out = Vec::with_capacity(out_w * out_h * 4);
    for y in 0..out_h {
        for x in 0..out_w {
            let at = ((y0 + y / scale) * width + x0 + x / scale) * 4;
            let cover = u32::from(bytes[at + 3]);
            for channel in 0..3 {
                let ink = u32::from(bytes[at + channel]);
                out.push(((ink * cover + 255 * (255 - cover)) / 255) as u8);
            }
            out.push(255);
        }
    }
    let png = sue_io::encode_png_rgba(out_w as u32, out_h as u32, &out).unwrap();
    let path = std::env::var("SUE_STROKE_ENDS").map_or_else(
        |_| std::env::temp_dir().join("sue-stroke-ends.png"),
        PathBuf::from,
    );
    std::fs::write(&path, png).unwrap();
    println!("stroke ends: {}", path.display());
}
