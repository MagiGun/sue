//! Tests that drive real frames through the runtime, one file for each
//! area, and the helpers they share.

use super::*;
use crate::engine::tests::temp_host;
use egui::{Pos2, vec2};
use sue_platform::{FormFactor, NoTextures};
use sue_wm::AppletId;

mod hub;
mod menus;
mod paint_anim;
mod paint_basics;
mod paint_brush;
mod paint_cmd_anim;
mod paint_cmd_canvas;
mod paint_cmd_edit;
mod paint_cmd_figure;
mod paint_cmd_file;
mod paint_cmd_help;
mod paint_cmd_layer;
mod paint_cmd_story;
mod paint_cmd_tool;
mod paint_comic;
mod paint_command_bar;
mod paint_edit;
mod paint_extras;
mod paint_files;
mod paint_filter;
mod paint_hub;
mod paint_layers;
mod paint_menus;
mod paint_palettes;
mod paint_rulers;
mod paint_saving;
mod paint_select;
mod paint_vector_3d;
mod paint_view;
mod paint_window;
mod save;
mod settings;
mod shell;
mod windows;
mod writer;

pub(crate) fn runtime(tag: &str) -> (Runtime, PathBuf) {
    let (host, root) = temp_host(tag);
    // Sue Paint's palettes as 0.0.75 laid them out by default (the
    // Timeline open under the canvas, Tool Sliders docked), kept as a
    // person's layout: the tests that draw at places on the canvas were
    // written for that canvas, and it is read as any layout kept then.
    // `fresh_runtime` starts from the layout style's own arrangement.
    let paint = root.join("data").join("paint");
    sue_platform::fs::create_dir_all(&paint).unwrap();
    sue_platform::fs::write(paint.join("layout.txt"), LAYOUT_0_0_75).unwrap();
    let runtime = Runtime::new(Engine::boot(&host).unwrap()).unwrap();
    (runtime, root)
}

/// The palette layout Sue Paint 0.0.75 started with, as it kept it.
pub(crate) const LAYOUT_0_0_75: &str = "column left 46 1:tool
column left 236 0.44:sub_tool 0.3:color 0.26:color_set
column left 86 1:brush_size
column right 300 0.42:navigator 0.58:layer
top 38 1:command_bar
bottom 160 1:timeline
icon material
icon layer_property
icon animation_cels
icon history
icon sub_view
icon auto_action
icon item_bank
icon info
panels 1111111111
menus 1
";

/// [`runtime`] with no layout kept: Sue Paint lays its palettes out as
/// its layout style does.
pub(crate) fn fresh_runtime(tag: &str) -> (Runtime, PathBuf) {
    let (host, root) = temp_host(tag);
    let runtime = Runtime::new(Engine::boot(&host).unwrap()).unwrap();
    (runtime, root)
}

/// Runs frames of `size` until the shell sleeps, at a fixed frame time.
pub(crate) fn frames(
    runtime: &mut Runtime,
    ctx: &egui::Context,
    size: egui::Vec2,
    start: Instant,
    mut input: impl FnMut(usize) -> Vec<egui::Event>,
) -> usize {
    let mut count = 0;
    let mut now = start;
    loop {
        // egui's clock goes on from the context's last frame, so calls
        // in a row never turn it back.
        let time = ctx.input(|input| input.time) + 0.008;
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size)),
            events: input(count),
            time: Some(time),
            ..egui::RawInput::default()
        };
        ctx.run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now))
            .drop_without_applying_deltas();
        count += 1;
        now += Duration::from_millis(8);
        let asleep = !matches!(runtime.pacing(now), FramePacing::Animate { .. });
        if (asleep && count > 2) || count > 2_000 {
            return count;
        }
    }
}

/// Where Sue Paint's canvas has its middle in a 1440 by 900 window: the
/// page's middle, moved by Sue Paint's palettes.
pub(crate) fn paint_middle(runtime: &Runtime) -> Pos2 {
    runtime.shell.page_rect().center() + vec2(8.0, -63.0)
}

pub(crate) fn frame_with(
    runtime: &mut Runtime,
    ctx: &egui::Context,
    now: Instant,
    events: Vec<egui::Event>,
) {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        events,
        ..egui::RawInput::default()
    };
    ctx.run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now))
        .drop_without_applying_deltas();
}

pub(crate) fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// A click at `at` over three frames, for [`frames`].
pub(crate) fn click_at(at: egui::Pos2) -> impl FnMut(usize) -> Vec<egui::Event> {
    move |frame: usize| match frame {
        0 => vec![egui::Event::PointerMoved(at)],
        1 => vec![egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
        2 => vec![egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        _ => Vec::new(),
    }
}

/// Where the Hub drew the row (or button) that does `row`.
pub(crate) fn hub_row(runtime: &Runtime, row: &sue_wm::HubRow) -> Option<egui::Rect> {
    runtime
        .shell
        .hub_rows()
        .iter()
        .filter(|(what, _)| what == row)
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.width().total_cmp(&b.width()))
}

/// Where the Hub drew the box of the studio or tool `id`, and its pin.
pub(crate) fn hub_box(runtime: &Runtime, id: &str) -> Option<(egui::Rect, egui::Rect)> {
    runtime
        .shell
        .hub_boxes()
        .iter()
        .find(|(applet, _, _)| applet.as_str() == id)
        .map(|(_, rect, pin)| (*rect, *pin))
}

/// The first document's text and marks.
pub(crate) fn doc_any(runtime: &Runtime) -> (String, sue_model::TextMarks) {
    runtime
        .engine
        .world
        .arena
        .iter()
        .find_map(|(_, entity)| {
            let text = entity.string_buffer()?.as_str().to_owned();
            text.contains("cat").then(|| {
                (
                    text,
                    entity
                        .ext::<sue_model::TextMarks>()
                        .cloned()
                        .unwrap_or_default(),
                )
            })
        })
        .unwrap()
}

/// Every piece of text one frame draws, with where it starts.
pub(crate) fn texts_of(output: &egui::FullOutput) -> Vec<(String, Pos2)> {
    text_rects(output)
        .into_iter()
        .map(|(text, rect)| (text, rect.min))
        .collect()
}

/// Every piece of text one frame draws, with its rect.
pub(crate) fn text_rects(output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
    let mut found = Vec::new();
    for clipped in &output.shapes {
        let mut look = |shape: &egui::Shape| {
            if let egui::Shape::Text(text) = shape {
                found.push((
                    text.galley.text().to_owned(),
                    egui::Rect::from_min_size(text.pos, text.galley.size()),
                ));
            }
        };
        match &clipped.shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(&mut look),
            shape => look(shape),
        }
    }
    found
}

/// Sue Paint at 1440 by 900 with a new canvas open, driven a frame at
/// a time with egui events, as the palette tests above drive it.
pub(crate) struct PaintDriver {
    pub(crate) runtime: Runtime,
    pub(crate) ctx: egui::Context,
    pub(crate) now: Instant,
    pub(crate) root: PathBuf,
    /// What the last frame drew: every piece of text and its rect.
    pub(crate) texts: Vec<(String, egui::Rect)>,
}

impl PaintDriver {
    pub(crate) fn new(tag: &str) -> Self {
        let (runtime, root) = runtime(tag);
        Self::with(runtime, root)
    }

    /// [`Self::new`] with no palette layout kept: the layout style's own.
    pub(crate) fn fresh(tag: &str) -> Self {
        let (runtime, root) = fresh_runtime(tag);
        Self::with(runtime, root)
    }

    fn with(mut runtime: Runtime, root: PathBuf) -> Self {
        let ctx = egui::Context::default();
        let now = Instant::now();
        runtime.request(
            ShellRequest::Open(AppletId::new(sue_paint::ID).unwrap()),
            &mut NoTextures,
            now,
        );
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        let mut driver = Self {
            runtime,
            ctx,
            now,
            root,
            texts: Vec::new(),
        };
        driver.step(vec![key(egui::Key::N, egui::Modifiers::COMMAND)]);
        driver.step(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]);
        driver.settle(60);
        driver
    }

    /// One frame with `events`; every piece of text it draws and its
    /// rect.
    pub(crate) fn step(&mut self, events: Vec<egui::Event>) -> Vec<(String, egui::Rect)> {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
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
        let found = text_rects(&output);
        output.drop_without_applying_deltas();
        self.texts.clone_from(&found);
        found
    }

    /// Where the last frame drew `text`: the topmost, then the leftmost
    /// when it is drawn more than once.
    pub(crate) fn rect_of(&self, text: &str) -> Option<egui::Rect> {
        self.texts
            .iter()
            .filter(|(found, _)| found == text)
            .map(|(_, rect)| *rect)
            .min_by(|a, b| {
                a.top()
                    .total_cmp(&b.top())
                    .then(a.left().total_cmp(&b.left()))
            })
    }

    /// A click in the middle of `text`, found where the last frame drew
    /// it and never by a fixed position; what the frames after draw.
    pub(crate) fn click_text(&mut self, text: &str) -> Vec<(String, egui::Rect)> {
        let at = self
            .rect_of(text)
            .unwrap_or_else(|| panic!("{text:?} is on screen: {:?}", self.texts))
            .center();
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.step(vec![egui::Event::PointerMoved(at)]);
        self.step(vec![press(true)]);
        self.step(vec![press(false)]);
        self.settle(4)
    }

    /// `count` frames with nothing new; what the last one draws.
    pub(crate) fn settle(&mut self, count: usize) -> Vec<(String, egui::Rect)> {
        let mut texts = Vec::new();
        for _ in 0..count {
            texts = self.step(Vec::new());
        }
        texts
    }

    /// Runs one of Sue Paint's commands, as its menus do.
    pub(crate) fn command(&mut self, command: &str) {
        self.runtime.request(
            ShellRequest::AppletCommand {
                applet: AppletId::new(sue_paint::ID).unwrap(),
                command: command.to_owned(),
            },
            &mut NoTextures,
            self.now,
        );
    }

    /// Presses `button` at `from`, moves to `to` in 20 steps and lets
    /// go; what the frames after draw.
    pub(crate) fn drag(
        &mut self,
        from: Pos2,
        to: Pos2,
        button: egui::PointerButton,
    ) -> Vec<(String, egui::Rect)> {
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.step(vec![egui::Event::PointerMoved(from)]);
        self.step(vec![press(from, true)]);
        for index in 1..=20 {
            let at = from + (to - from) * (index as f32 / 20.0);
            self.step(vec![egui::Event::PointerMoved(at)]);
        }
        self.step(vec![press(to, false)]);
        self.settle(10)
    }

    /// The page's top, under Sue's own bar.
    pub(crate) fn page_top(&self) -> f32 {
        self.runtime.shell.page_rect().top()
    }
}

impl Drop for PaintDriver {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Where `name` is drawn in `area`, the leftmost first.
pub(crate) fn text_in(
    texts: &[(String, egui::Rect)],
    name: &str,
    area: egui::Rect,
) -> Option<egui::Rect> {
    texts
        .iter()
        .filter(|(text, rect)| text == name && area.contains(rect.center()))
        .map(|(_, rect)| *rect)
        .min_by(|a, b| a.left().total_cmp(&b.left()))
}
