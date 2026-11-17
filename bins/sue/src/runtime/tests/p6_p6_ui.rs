//! Phase 6, "UI polish to top tier": the gaps the Phase 5 integration
//! found short of top tier, driven through real frames. Each test is
//! named after the tutorial step or gap it closes: `t50_02_` for tip 2 of
//! the fifty tips (tools, sub tools and colors dragged straight onto the
//! Command Bar), `ab_notice_` for the notice the beginners' animation
//! acceptance found over the Timeline, and `p6_` for the polish rows of
//! the integration report. Controls are found by the name the frame gives
//! assistive technology and pressed where that frame drew them, never at
//! fixed places.

use super::*;
use egui::{PointerButton, Rect};

/// Sue Paint at 1440 by 900 with a canvas, its palettes as its layout
/// style lays them out, driven a frame at a time; each frame says every
/// control's name and where it is.
struct Polish {
    driver: PaintDriver,
    /// The named controls of the last frame.
    nodes: Vec<(String, Rect)>,
    /// The named controls of the last frame that are on (picked).
    on: Vec<(String, Rect)>,
}

impl Polish {
    fn new(tag: &str) -> Self {
        let driver = PaintDriver::fresh(&format!("p6-ui-{tag}"));
        driver.ctx.enable_accesskit();
        let mut polish = Self {
            driver,
            nodes: Vec::new(),
            on: Vec::new(),
        };
        polish.settle(6);
        polish
    }

    fn step(&mut self, events: Vec<egui::Event>) {
        let driver = &mut self.driver;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events,
            time: Some(driver.ctx.input(|input| input.time) + 0.016),
            ..egui::RawInput::default()
        };
        driver.now += Duration::from_millis(16);
        let runtime = &mut driver.runtime;
        let now = driver.now;
        let output = driver
            .ctx
            .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
        driver.texts = text_rects(&output);
        let mut nodes = Vec::new();
        let mut on = Vec::new();
        for node in output
            .platform_output
            .accesskit_update
            .iter()
            .flat_map(|update| update.nodes.iter().map(|(_, node)| node))
        {
            let (Some(label), Some(bounds)) = (node.label(), node.bounds()) else {
                continue;
            };
            let rect = Rect::from_min_max(
                Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
                Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
            );
            if node.is_selected() == Some(true)
                || node.toggled() == Some(egui::accesskit::Toggled::True)
            {
                on.push((label.to_owned(), rect));
            }
            nodes.push((label.to_owned(), rect));
        }
        self.nodes = nodes;
        self.on = on;
        output.drop_without_applying_deltas();
    }

    fn settle(&mut self, frames: usize) {
        for _ in 0..frames {
            self.step(Vec::new());
        }
    }

    /// The first control whose name starts with `name` and whose rect
    /// `place` accepts.
    fn node(&self, name: &str, place: impl Fn(Rect) -> bool) -> Option<Rect> {
        self.nodes
            .iter()
            .find(|(label, rect)| label.starts_with(name) && place(*rect))
            .map(|(_, rect)| *rect)
    }

    /// The command bar's buttons: the controls along the top row of the
    /// page, under the menus.
    fn bar_buttons(&self) -> Vec<(String, Rect)> {
        let top = self.driver.page_top();
        let mut buttons: Vec<(String, Rect)> = self
            .nodes
            .iter()
            .filter(|(_, rect)| {
                rect.top() >= top - 1.0
                    && rect.bottom() <= top + 40.0
                    && rect.width() < 80.0
                    && rect.left() < 1400.0
            })
            .cloned()
            .collect();
        buttons.sort_by(|a, b| a.1.left().total_cmp(&b.1.left()));
        buttons.dedup_by(|a, b| a.1 == b.1);
        buttons
    }

    /// An empty place on the command bar, after its last button.
    fn bar_end(&self) -> Pos2 {
        let last = self
            .bar_buttons()
            .into_iter()
            .map(|(_, rect)| rect)
            .max_by(|a, b| a.right().total_cmp(&b.right()))
            .expect("the command bar has buttons");
        Pos2::new(last.right() + 60.0, last.center().y)
    }

    /// Presses at `from`, moves to `to` in small steps and lets go.
    fn drag(&mut self, from: Pos2, to: Pos2) {
        let button = PointerButton::Primary;
        self.step(vec![egui::Event::PointerMoved(from)]);
        self.step(vec![egui::Event::PointerButton {
            pos: from,
            button,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        for index in 1..=24 {
            let t = index as f32 / 24.0;
            self.step(vec![egui::Event::PointerMoved(from + (to - from) * t)]);
        }
        self.step(vec![egui::Event::PointerButton {
            pos: to,
            button,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        self.settle(4);
    }

    /// A click at `at`.
    fn click(&mut self, at: Pos2) {
        self.step(vec![egui::Event::PointerMoved(at)]);
        for pressed in [true, false] {
            self.step(vec![egui::Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
        self.settle(3);
    }

    fn bar_named(&self, name: &str) -> usize {
        self.bar_buttons()
            .iter()
            .filter(|(label, _)| label.starts_with(name))
            .count()
    }
}

/// Tip 2: a tool dragged from the Tool palette straight onto the Command
/// Bar becomes a button there that picks it.
#[test]
fn t50_02_a_tool_dragged_onto_the_command_bar() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut ui = Polish::new("bar-tool");
    let before = ui.bar_named("Eraser");
    let tool = ui
        .node("Eraser", |rect| rect.right() < 60.0)
        .expect("Eraser on the Tool palette");
    let to = ui.bar_end();
    ui.drag(tool.center(), to);
    assert_eq!(ui.bar_named("Eraser"), before + 1, "{:?}", ui.bar_buttons());
    // The new button picks the Eraser.
    let button = ui
        .bar_buttons()
        .into_iter()
        .filter(|(label, _)| label.starts_with("Eraser"))
        .map(|(_, rect)| rect)
        .max_by(|a, b| a.left().total_cmp(&b.left()))
        .unwrap();
    ui.click(button.center());
    assert!(
        ui.on
            .iter()
            .any(|(label, rect)| label.starts_with("Eraser") && rect.right() < 60.0),
        "the Eraser is picked on the Tool palette: {:?}",
        ui.on
    );
}

/// Tip 2: a sub tool dragged from the Sub Tool palette onto the Command
/// Bar becomes a button that picks that sub tool.
#[test]
fn t50_02_a_sub_tool_dragged_onto_the_command_bar() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut ui = Polish::new("bar-sub-tool");
    let before = ui.bar_named("Mapping pen");
    let row = ui
        .node("Mapping pen", |rect| {
            rect.left() > 40.0 && rect.right() < 400.0
        })
        .expect("Mapping pen in the Sub Tool palette");
    let to = ui.bar_end();
    ui.drag(row.center(), to);
    assert_eq!(
        ui.bar_named("Mapping pen"),
        before + 1,
        "{:?}",
        ui.bar_buttons()
    );
    // Another sub tool, then the new button: it picks the Mapping pen.
    let other = ui
        .node("Turnip pen", |rect| {
            rect.left() > 40.0 && rect.right() < 400.0
        })
        .expect("Turnip pen in the Sub Tool palette");
    ui.click(other.center());
    let picked = |ui: &Polish, name: &str| {
        ui.on.iter().any(|(label, rect)| {
            label.starts_with(name) && rect.left() > 40.0 && rect.right() < 400.0
        })
    };
    assert!(picked(&ui, "Turnip pen"), "{:?}", ui.on);
    let button = ui
        .bar_buttons()
        .into_iter()
        .filter(|(label, _)| label.starts_with("Mapping pen"))
        .map(|(_, rect)| rect)
        .max_by(|a, b| a.left().total_cmp(&b.left()))
        .unwrap();
    ui.click(button.center());
    assert!(picked(&ui, "Mapping pen"), "{:?}", ui.on);
}

/// Tip 2: a color dragged from a color set onto the Command Bar becomes
/// a button that makes it the drawing color.
#[test]
fn t50_02_a_color_dragged_onto_the_command_bar() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut ui = Polish::new("bar-color");
    let tab = ui
        .node("Color Set", |rect| rect.right() < 400.0)
        .expect("the Color Set tab");
    ui.click(tab.center());
    let swatch = ui
        .nodes
        .iter()
        .filter(|(label, rect)| label.contains('#') && rect.left() > 40.0 && rect.right() < 400.0)
        .map(|(label, rect)| (label.clone(), *rect))
        .find(|(_, rect)| rect.width() < 40.0)
        .expect("a swatch of the color set");
    let hex = swatch.0[swatch.0.find('#').unwrap() + 1..]
        .chars()
        .take(6)
        .collect::<String>();
    let before = ui.bar_named("Drawing Color");
    let to = ui.bar_end();
    ui.drag(swatch.1.center(), to);
    let made: Vec<_> = ui
        .bar_buttons()
        .into_iter()
        .filter(|(label, _)| label.starts_with("Drawing Color"))
        .collect();
    assert_eq!(made.len(), before + 1, "{:?}", ui.bar_buttons());
    assert!(
        made.iter()
            .any(|(label, _)| label.to_lowercase().contains(&hex.to_lowercase())),
        "{made:?} {hex}"
    );
}

/// Sue Paint's applet, as the shell holds it.
fn applet(ui: &Polish) -> &dyn sue_wm::SueApplet {
    ui.driver
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .expect("Sue Paint runs")
}

/// The picture's top layer.
fn top_layer(ui: &Polish) -> sue_core::EntityId {
    let arena = &ui.driver.runtime.engine.world.arena;
    let picture = applet(ui).focus().expect("a picture is open");
    sue_paint::doc::layers(arena, picture)
        .last()
        .expect("a layer")
        .id
}

/// How many pixels of `layer` hold paint.
fn painted(ui: &Polish, layer: sue_core::EntityId) -> usize {
    let arena = &ui.driver.runtime.engine.world.arena;
    sue_paint::doc::raster(arena, layer).map_or(0, |raster| {
        raster
            .bytes()
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .count()
    })
}

/// The beginners' animation acceptance (step 29) found the "Copied"
/// notice over the docked Timeline's first row, taking its clicks. A
/// notice now stands in the canvas's view, above the Timeline, however
/// small that view is.
#[test]
fn ab_notice_copied_stays_off_the_docked_timeline() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut ui = Polish::new("notice-timeline");
    ui.driver.new_timeline();
    ui.driver.command("window.timeline");
    ui.settle(4);
    let ruler = ui
        .driver
        .rect_of("Timeline 1")
        .expect("the Timeline is docked under the canvas");
    ui.driver.command("edit.copy");
    ui.settle(4);
    let band = ui
        .driver
        .runtime
        .shell
        .notices_area()
        .expect("the notice shows");
    let room = applet(&ui).notice_room().expect("the canvas's view");
    assert!(band.bottom() <= room.bottom() + 0.5, "{band:?} in {room:?}");
    assert!(band.bottom() <= ruler.top(), "{band:?} over {ruler:?}");
}

/// The same: a notice never takes a click from the work under it. A
/// pen stroke drawn straight across a notice lands on the canvas.
#[test]
fn ab_notice_a_stroke_across_a_notice_paints() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut ui = Polish::new("notice-click-through");
    // Close enough that the picture fills the canvas's view.
    for _ in 0..4 {
        ui.driver.command("view.zoom_in");
    }
    ui.settle(2);
    ui.driver.command("edit.copy");
    ui.settle(4);
    let band = ui
        .driver
        .runtime
        .shell
        .notices_area()
        .expect("the notice shows");
    ui.driver.command("tool.pen");
    ui.settle(2);
    let layer = top_layer(&ui);
    let before = painted(&ui, layer);
    // Through the middle of the newest notice, the lowest of the band.
    let y = band.bottom() - 40.0;
    let from = Pos2::new(band.center().x - 120.0, y);
    let to = Pos2::new(band.center().x + 60.0, y);
    ui.drag(from, to);
    assert!(
        painted(&ui, layer) > before,
        "the stroke under the notice painted"
    );
}

/// The integration report: the default layout's Sub Tool showed three
/// rows where Clip Studio Paint shows about seven. On a 1440 by 900
/// window it now lists five of the Pen's sub tools whole, each one
/// line, and Tool Property under it still shows its Opacity row.
#[test]
fn p6_the_default_sub_tool_lists_five_sub_tools() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let ui = Polish::new("sub-tool-rows");
    // The list ends above the buttons under it: a row drawn lower is
    // cut off by the list's edge and does not count.
    let buttons = ui
        .node("Delete Sub Tool", |rect| rect.right() < 400.0)
        .expect("the buttons under the list");
    let list_end = buttons.top() - 8.0;
    let listed = [
        "G-pen",
        "Grainy G-pen",
        "Mapping pen",
        "Turnip pen",
        "Textured pen",
    ]
    .iter()
    .filter(|name| {
        ui.driver
            .texts
            .iter()
            .any(|(text, rect)| text == *name && rect.right() < 400.0 && rect.bottom() < list_end)
    })
    .count();
    assert_eq!(listed, 5, "{:?}", ui.driver.texts);
    let opacity = ui
        .driver
        .texts
        .iter()
        .find(|(text, rect)| text == "Opacity" && rect.right() < 400.0)
        .map(|(_, rect)| *rect)
        .expect("Tool Property's Opacity row");
    // The Brush Size palette's title: the lowest "Brush Size" on the
    // left (Tool Property has a row of that name too).
    let brush_size = ui
        .driver
        .texts
        .iter()
        .filter(|(text, rect)| text == "Brush Size" && rect.right() < 400.0)
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("the Brush Size palette");
    assert!(
        opacity.bottom() < brush_size.top(),
        "Opacity whole above Brush Size"
    );
}

/// The integration report: the sample picture's colored dabs had a
/// wide pale halo. Each circle's color is now flat up to its ink: just
/// inside the line it is solid, and just outside it nothing is painted.
#[test]
fn p6_the_sample_picture_colors_meet_their_ink() {
    let (mut runtime, _root) = runtime("p6-ui-sample");
    let start = Instant::now();
    runtime.request(
        ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
        &mut NoTextures,
        start,
    );
    let (w, h) = (1600_u32, 1200_u32);
    let picture = runtime
        .with_context(&mut NoTextures, start, |cx| {
            sue_paint::sample_picture(cx, w, h)
        })
        .expect("the sample picture");
    let arena = &runtime.engine.world.arena;
    let color = sue_paint::doc::layers(arena, picture)
        .into_iter()
        .find(|layer| layer.name == "Color")
        .expect("the Color layer")
        .id;
    let raster = sue_paint::doc::raster(arena, color).expect("pixels");
    assert_eq!((raster.width(), raster.height()), (w, h));
    let bytes = raster.bytes().unwrap();
    let alpha =
        |x: f32, y: f32| bytes[((y.round() as u32 * w + x.round() as u32) * 4 + 3) as usize];
    let (wf, hf) = (w as f32, h as f32);
    // Half the ink's width, as the sample's pen draws it.
    let ink = wf / 160.0 / 2.0;
    for (center, radius) in [
        ([0.34, 0.45], 0.1),
        ([0.62, 0.38], 0.08),
        ([0.5, 0.66], 0.09),
    ] {
        let (cx, cy, r) = (wf * center[0], hf * center[1], wf * radius);
        for step in 0..32 {
            let t = step as f32 / 32.0 * std::f32::consts::TAU;
            let at = |reach: f32| (cx + t.cos() * reach, cy + t.sin() * reach);
            let (x, y) = at(r - ink - 1.0);
            assert_eq!(alpha(x, y), 255, "solid just inside the ink at {x}, {y}");
            let (x, y) = at(r + ink + 2.0);
            assert_eq!(alpha(x, y), 0, "clear just outside the ink at {x}, {y}");
        }
    }
}
