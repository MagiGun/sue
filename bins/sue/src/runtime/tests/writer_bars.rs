//! Sue Writer's bars, driven through real frames: the formatting
//! toolbar, the status bar, the find bar and the edges of the panels.
//! Controls are found by the name they give assistive technology and
//! clicked where the last frame put them.

use super::*;
use egui::accesskit::{Role, Toggled};

/// What one frame told assistive technology about a control.
#[derive(Clone, Debug)]
struct Told {
    role: Role,
    name: String,
    rect: egui::Rect,
    on: Option<bool>,
    focused: bool,
}

/// Sue Writer at 1440 by 900 with a blank document open, with
/// assistive technology listening, driven a frame at a time.
struct Bars {
    runtime: Runtime,
    ctx: egui::Context,
    now: Instant,
    root: PathBuf,
    told: Vec<Told>,
    texts: Vec<(String, egui::Rect)>,
    /// Every rectangle the last frame drew, with its fill and outline.
    rects: Vec<(egui::Rect, egui::Color32, egui::Stroke)>,
}

impl Bars {
    fn new(tag: &str) -> Self {
        let (mut runtime, root) = runtime(tag);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let now = Instant::now();
        frames(&mut runtime, &ctx, vec2(1440.0, 900.0), now, |_| Vec::new());
        runtime.request(
            ShellRequest::Create {
                applet: Self::id(),
                key: "blank".to_owned(),
            },
            &mut NoTextures,
            now,
        );
        let mut bars = Self {
            runtime,
            ctx,
            now,
            root,
            told: Vec::new(),
            texts: Vec::new(),
            rects: Vec::new(),
        };
        bars.settle(30);
        bars
    }

    fn id() -> AppletId {
        AppletId::new(sue_writer::ID).unwrap()
    }

    fn step(&mut self, events: Vec<egui::Event>) {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events,
            time: Some(self.ctx.input(|input| input.time) + 0.05),
            ..egui::RawInput::default()
        };
        self.now += Duration::from_millis(50);
        let runtime = &mut self.runtime;
        let now = self.now;
        let output = self
            .ctx
            .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
        self.texts = text_rects(&output);
        // egui writes a warning over two controls that share an id: none
        // of the bars' controls may (their focus and motion would mix).
        let clash = self
            .texts
            .iter()
            .find(|(text, _)| text.contains("use of widget ID") || text.contains("ID clash"));
        assert!(clash.is_none(), "two controls share an id: {clash:?}");
        self.rects = rects(&output);
        self.told = told(&output);
        output.drop_without_applying_deltas();
    }

    fn settle(&mut self, count: usize) {
        for _ in 0..count {
            self.step(Vec::new());
        }
    }

    /// The control called `name`, the highest on screen first (the
    /// toolbar before anything under it).
    fn find(&self, name: &str) -> Option<Told> {
        self.told
            .iter()
            .filter(|told| told.name == name && told.rect.is_positive())
            .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
            .cloned()
    }

    fn control(&self, name: &str) -> Told {
        self.find(name).unwrap_or_else(|| {
            let names: Vec<&str> = self.told.iter().map(|told| told.name.as_str()).collect();
            panic!("nothing is called {name:?}: {names:?}")
        })
    }

    fn click_at(&mut self, at: Pos2) {
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.step(vec![egui::Event::PointerMoved(at)]);
        self.step(vec![press(true)]);
        self.step(vec![press(false)]);
        self.settle(3);
    }

    /// A click on the control called `name`.
    fn click(&mut self, name: &str) {
        let at = self.control(name).rect.center();
        self.click_at(at);
    }

    /// The pointer rests on the control called `name` until its tooltip
    /// shows; what the frame drew then.
    fn hover(&mut self, name: &str) -> Vec<String> {
        let at = self.control(name).rect.center();
        // Away first, so a tooltip shown before goes.
        self.step(vec![egui::Event::PointerMoved(pos2(720.0, 600.0))]);
        self.settle(3);
        self.step(vec![egui::Event::PointerMoved(at)]);
        self.settle(20);
        self.texts.iter().map(|(text, _)| text.clone()).collect()
    }

    fn type_text(&mut self, text: &str) {
        self.step(vec![egui::Event::Text(text.to_owned())]);
        self.settle(3);
        self.runtime.typing.flush(&mut self.runtime.engine.history);
    }

    fn command(&mut self, command: &str) {
        self.runtime.request(
            ShellRequest::AppletCommand {
                applet: Self::id(),
                command: command.to_owned(),
            },
            &mut NoTextures,
            self.now,
        );
        self.settle(4);
    }

    /// The document holding `words`: its text and marks.
    fn document(&self, words: &str) -> (String, sue_model::TextMarks) {
        self.runtime
            .engine
            .world
            .arena
            .iter()
            .find_map(|(_, entity)| {
                let text = entity.string_buffer()?.as_str().to_owned();
                (sue_model::manuscript::is_document(entity) && text.contains(words)).then(|| {
                    (
                        text,
                        entity
                            .ext::<sue_model::TextMarks>()
                            .cloned()
                            .unwrap_or_default(),
                    )
                })
            })
            .unwrap_or_else(|| panic!("a document holds {words:?}"))
    }

    /// The whole arena as the file writes it.
    fn arena(&self) -> Vec<u8> {
        let mut out = sue_core::codec::Writer::new();
        self.runtime.engine.world.arena.encode(&mut out);
        out.into_bytes()
    }

    /// Whether the last frame drew a focus ring (an outline of the focus
    /// stroke's width with nothing inside) around all of `around`.
    fn ring_around(&self, around: egui::Rect) -> bool {
        self.rects.iter().any(|(drawn, fill, stroke)| {
            *fill == egui::Color32::TRANSPARENT
                && stroke.width == sue_wm::Theme::STRONG_STROKE
                && stroke.color.a() > 0
                && drawn.expand(3.0).contains_rect(around)
                && around.expand(6.0).contains_rect(*drawn)
        })
    }
}

impl Drop for Bars {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn told(output: &egui::FullOutput) -> Vec<Told> {
    let Some(update) = &output.platform_output.accesskit_update else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter_map(|(id, node)| {
            let name = node.label()?.to_owned();
            let bounds = node.bounds()?;
            let rect = egui::Rect::from_min_max(
                pos2(bounds.x0 as f32, bounds.y0 as f32),
                pos2(bounds.x1 as f32, bounds.y1 as f32),
            );
            Some(Told {
                role: node.role(),
                name,
                rect,
                on: node.toggled().map(|toggled| toggled == Toggled::True),
                focused: update.focus == *id,
            })
        })
        .collect()
}

fn rects(output: &egui::FullOutput) -> Vec<(egui::Rect, egui::Color32, egui::Stroke)> {
    fn walk(shape: &egui::Shape, out: &mut Vec<(egui::Rect, egui::Color32, egui::Stroke)>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, out)),
            egui::Shape::Rect(rect) => out.push((rect.rect, rect.fill, rect.stroke)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, &mut out);
    }
    out
}

fn pos2(x: f32, y: f32) -> Pos2 {
    Pos2::new(x, y)
}

fn bold_over(marks: &sue_model::TextMarks, text: &str, words: &str) -> bool {
    let start = text.find(words).unwrap();
    let end = start + words.len();
    marks.marks().iter().any(|mark| {
        mark.kind == sue_model::mark_kind::BOLD
            && mark.range.start <= start
            && mark.range.end >= end
    })
}

/// The toolbar's Bold is an icon button that bolds the selection, shows
/// that it is on, and is undone and redone by the toolbar's own Undo and
/// Redo, which put the arena back exactly.
#[test]
fn sue_writers_toolbar_bolds_with_its_icon_and_its_undo_puts_it_back() {
    let mut bars = Bars::new("writer-bars-bold");
    bars.type_text("the cat sat");
    bars.command("edit.select_all");
    let before = bars.arena();
    let bold = bars.control("Bold");
    assert_eq!(bold.role, Role::Button);
    assert_eq!(bold.on, Some(false), "nothing is bold yet");
    bars.click("Bold");
    let (text, marks) = bars.document("the cat sat");
    assert!(bold_over(&marks, &text, "the cat sat"), "{marks:?}");
    assert_eq!(bars.control("Bold").on, Some(true), "Bold shows it is on");
    let after = bars.arena();
    assert_ne!(before, after);
    bars.click("Undo");
    let (text, marks) = bars.document("the cat sat");
    assert!(!bold_over(&marks, &text, "the cat sat"));
    assert_eq!(
        bars.arena(),
        before,
        "undo puts the arena back, ids and all"
    );
    bars.click("Redo");
    assert_eq!(bars.arena(), after, "redo brings the bold back");
}

/// Every button of the toolbar, the status bar and the find bar has a
/// name; the tooltip of one with keys shows them, and follows a key
/// changed in Settings.
#[test]
fn sue_writers_bar_buttons_are_named_and_their_tooltips_show_their_keys() {
    let mut bars = Bars::new("writer-bars-tips");
    bars.command("edit.find");
    for name in [
        "Undo",
        "Redo",
        "Bold",
        "Italic",
        "Underline",
        "Highlight",
        "Bulleted List",
        "Numbered List",
        "Increase Indent",
        "Decrease Indent",
        "Find",
        "Zoom In",
        "Zoom Out",
        "Find Next",
        "Find Previous",
        "Close",
    ] {
        assert!(bars.find(name).is_some(), "{name} is named");
    }
    let shown = bars.hover("Bold");
    assert!(shown.iter().any(|text| text == "Ctrl+B"), "{shown:?}");
    let shown = bars.hover("Find Next");
    assert!(shown.iter().any(|text| text == "F3"), "{shown:?}");
    // Bold gets another key in Settings; the tooltip says the new one.
    bars.runtime
        .engine
        .world
        .rice
        .set(
            "sue_writer_keys.format_bold",
            sue_rice::RiceValue::Keybind("Ctrl+Shift+B".parse().unwrap()),
        )
        .unwrap();
    bars.settle(3);
    let shown = bars.hover("Bold");
    assert!(shown.iter().any(|text| text == "Ctrl+Shift+B"), "{shown:?}");
    // The find bar's boxes and its Replace button say their names too
    // (the name drawn beside a box is one text; its tooltip is another).
    for name in ["Match case", "Whole words", "Replace..."] {
        let before = bars.texts.iter().filter(|(text, _)| text == name).count();
        let shown = bars.hover(name);
        let after = shown.iter().filter(|text| *text == name).count();
        assert!(after > before, "{name} shows a tooltip: {shown:?}");
    }
}

/// Shift and Tab walk the keyboard's focus back from the end of the
/// page, through the status bar and on to the toolbar; every button of
/// theirs that holds it shows a ring.
#[test]
fn sue_writers_bar_buttons_show_a_focus_ring_when_tabbed_to() {
    let mut bars = Bars::new("writer-bars-focus");
    // A click on the status bar's words takes the keyboard from the text.
    let words = bars
        .texts
        .iter()
        .find(|(text, _)| text.contains("words"))
        .map(|(_, rect)| rect.center())
        .expect("the status bar counts words");
    bars.click_at(words);
    let (mut status, mut toolbar) = (Vec::new(), Vec::new());
    for _ in 0..120 {
        bars.step(vec![key(egui::Key::Tab, egui::Modifiers::SHIFT)]);
        bars.settle(3);
        let Some(focused) = bars.told.iter().find(|told| told.focused).cloned() else {
            continue;
        };
        if !matches!(focused.role, Role::Button | Role::ComboBox) {
            continue;
        }
        let place = if focused.rect.top() > 860.0 {
            &mut status
        } else if focused.rect.bottom() < 120.0 {
            &mut toolbar
        } else {
            continue;
        };
        assert!(
            bars.ring_around(focused.rect),
            "{} shows a ring at {:?}",
            focused.name,
            focused.rect
        );
        place.push(focused.name);
        if ["Font", "Size", "Paragraph Style"]
            .iter()
            .all(|name| toolbar.iter().any(|seen| seen == name))
        {
            break;
        }
    }
    assert!(
        status.iter().any(|name| name == "Zoom In"),
        "the status bar's buttons take focus: {status:?}"
    );
    // Every kind of the toolbar's controls: its buttons, the text color
    // and its three boxes.
    for name in ["Bold", "Text Color", "Font", "Size", "Paragraph Style"] {
        assert!(
            toolbar.iter().any(|seen| seen == name),
            "{name} takes focus and shows a ring: {toolbar:?}"
        );
    }
}

/// The find bar's arrows step through what it found and its cross
/// closes it; all three are icon buttons.
#[test]
fn sue_writers_find_bar_steps_with_its_arrows_and_closes_with_its_cross() {
    let mut bars = Bars::new("writer-bars-find");
    bars.type_text("one cat, two cats");
    bars.command("edit.find");
    assert_eq!(bars.control("Find").on, Some(true), "Find shows it is open");
    bars.step(vec![egui::Event::Text("cat".to_owned())]);
    bars.settle(3);
    bars.click("Find Next");
    let first = bars.arena();
    bars.click("Find Next");
    bars.click("Find Previous");
    assert_eq!(bars.arena(), first, "finding changes nothing");
    bars.click("Close");
    assert!(bars.find("Find Next").is_none(), "the find bar went");
    assert_eq!(bars.control("Find").on, Some(false));
    // The toolbar's Find opens it again.
    bars.click("Find");
    assert!(bars.find("Find Next").is_some());
}

/// The status bar's zoom buttons change the zoom shown beside them, and
/// its Draft and Page switch shows the pages.
#[test]
fn sue_writers_status_bar_zooms_and_switches_to_pages() {
    let mut bars = Bars::new("writer-bars-status");
    let zoom = |bars: &Bars| {
        bars.texts
            .iter()
            .find(|(text, rect)| text.ends_with('%') && rect.top() > 860.0)
            .map(|(text, _)| text.clone())
            .expect("the status bar shows the zoom")
    };
    let fitted = zoom(&bars);
    assert!(fitted.starts_with("Fit"), "{fitted}");
    bars.click("Zoom In");
    let zoomed = zoom(&bars);
    assert!(!zoomed.starts_with("Fit"), "{zoomed}");
    bars.click("Zoom Out");
    bars.click("Zoom Out");
    assert_ne!(zoom(&bars), zoomed);
    let pages = |bars: &Bars| {
        bars.texts.iter().any(|(text, rect)| {
            text.contains("Page ") && text.contains(" of ") && rect.top() > 860.0
        })
    };
    let page = bars
        .texts
        .iter()
        .find(|(text, rect)| text == "Page" && rect.top() > 860.0)
        .map(|(_, rect)| rect.center())
        .expect("the switch shows Page");
    let draft = bars
        .texts
        .iter()
        .find(|(text, rect)| text == "Draft" && rect.top() > 860.0)
        .map(|(_, rect)| rect.center())
        .expect("the switch shows Draft");
    bars.click_at(draft);
    assert!(!pages(&bars), "Draft shows no pages");
    bars.click_at(page);
    assert!(pages(&bars), "Page shows the pages");
}

/// The toolbar's Highlight opens a list of colors; picking one marks the
/// selection, and undo takes it off again.
#[test]
fn sue_writers_highlight_button_offers_its_colors_and_undoes() {
    let mut bars = Bars::new("writer-bars-highlight");
    bars.type_text("bright words");
    bars.command("edit.select_all");
    let before = bars.arena();
    bars.click("Highlight");
    let yellow = bars
        .texts
        .iter()
        .find(|(text, _)| text == "Yellow")
        .map(|(_, rect)| rect.center())
        .expect("the list offers Yellow");
    bars.click_at(yellow);
    let (_, marks) = bars.document("bright words");
    assert!(
        marks
            .marks()
            .iter()
            .any(|mark| mark.kind == sue_model::mark_kind::HIGHLIGHT),
        "{marks:?}"
    );
    assert!(
        !bars.texts.iter().any(|(text, _)| text == "Yellow"),
        "the list closed"
    );
    bars.click("Undo");
    assert_eq!(bars.arena(), before);
}

/// The font box looks like the boxes beside it and opens the installed
/// fonts under a search field that takes the typing at once; Escape
/// closes it with the text untouched.
#[test]
fn sue_writers_font_box_opens_a_search_that_takes_the_keys() {
    let mut bars = Bars::new("writer-bars-font");
    bars.type_text("plain words");
    bars.command("edit.select_all");
    let before = bars.arena();
    let font = bars.control("Font");
    assert_eq!(font.role, Role::ComboBox, "a box like the size's");
    assert!(bars.hover("Font").iter().any(|text| text == "Font"));
    bars.click("Font");
    assert!(
        bars.texts.iter().any(|(text, _)| text == "Style's font"),
        "the list opened"
    );
    bars.step(vec![egui::Event::Text("no such face".to_owned())]);
    bars.settle(3);
    assert!(
        bars.texts.iter().any(|(text, _)| text == "no such face"),
        "the search field took the typing"
    );
    assert_eq!(bars.arena(), before, "the text did not");
    bars.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    bars.settle(3);
    assert!(
        !bars.texts.iter().any(|(text, _)| text == "Style's font"),
        "Escape closed the list"
    );
    assert_eq!(bars.arena(), before);
}

/// Where the last frame drew `text` lowest on screen (a list's row under
/// the field that searched for it).
fn lowest(bars: &Bars, text: &str) -> Pos2 {
    bars.texts
        .iter()
        .filter(|(shown, _)| shown == text)
        .max_by(|a, b| a.1.top().total_cmp(&b.1.top()))
        .map(|(_, rect)| rect.center())
        .unwrap_or_else(|| panic!("the frame shows {text:?}"))
}

/// A font picked from the font box's list marks the selection in that
/// face, gives the keyboard back to the text and is undone in one step.
#[test]
fn sue_writers_font_box_picks_a_face_and_undo_takes_it_off() {
    let mut bars = Bars::new("writer-bars-font-pick");
    bars.type_text("plain words");
    bars.command("edit.select_all");
    let before = bars.arena();
    bars.click("Font");
    bars.step(vec![egui::Event::Text("Hack".to_owned())]);
    bars.settle(3);
    let row = lowest(&bars, "Hack");
    bars.click_at(row);
    let (_, marks) = bars.document("plain words");
    assert!(
        marks
            .marks()
            .iter()
            .any(|mark| mark.kind == sue_model::mark_kind::FONT),
        "{marks:?}"
    );
    assert!(
        !bars.texts.iter().any(|(text, _)| text == "Style's font"),
        "the list closed"
    );
    let faced = bars.arena();
    // The caret is back in the text: what is typed replaces the words.
    bars.type_text("new");
    let (text, _) = bars.document("new");
    assert!(!text.contains("plain words"), "{text:?}");
    bars.click("Undo");
    assert_eq!(bars.arena(), faced);
    bars.click("Undo");
    assert_eq!(bars.arena(), before, "one undo takes the face off");
}

/// The size box lists the sizes; one picked sizes the selection, the
/// keys go back to the text, and undo puts the size back.
#[test]
fn sue_writers_size_box_sizes_the_text_and_hands_the_keys_back() {
    let mut bars = Bars::new("writer-bars-size");
    bars.type_text("small words");
    bars.command("edit.select_all");
    let before = bars.arena();
    assert_eq!(bars.control("Size").role, Role::ComboBox);
    bars.click("Size");
    // 18 is in the first rows of the list (it scrolls past twelve).
    let row = lowest(&bars, "18");
    bars.click_at(row);
    let (_, marks) = bars.document("small words");
    assert!(
        marks
            .marks()
            .iter()
            .any(|mark| mark.kind == sue_model::mark_kind::SIZE),
        "{marks:?}"
    );
    let sized = bars.arena();
    // The text holds the keyboard again: what is typed replaces the
    // selected words.
    bars.type_text("big");
    let (text, _) = bars.document("big");
    assert!(!text.contains("small words"), "{text:?}");
    bars.click("Undo");
    assert_eq!(bars.arena(), sized);
    bars.click("Undo");
    assert_eq!(bars.arena(), before);
}

/// The text color button shows the letter A over the color and opens a
/// wheel; a click on the wheel colors the selection, and undo takes the
/// color off.
#[test]
fn sue_writers_text_color_opens_a_wheel_that_colors_the_selection() {
    let mut bars = Bars::new("writer-bars-color");
    bars.type_text("red words");
    bars.command("edit.select_all");
    let before = bars.arena();
    let button = bars.control("Text Color");
    assert_eq!(button.role, Role::Button);
    assert!(
        bars.hover("Text Color")
            .iter()
            .any(|text| text == "Text Color")
    );
    bars.click("Text Color");
    assert_eq!(
        bars.control("Text Color").on,
        Some(true),
        "it shows it is open"
    );
    // The wheel opens in a popover under the button. The text is black,
    // which no hue changes: a click up and right of the middle, in the
    // square of bright colors, picks one.
    let frame = bars
        .rects
        .iter()
        .map(|(rect, _, _)| *rect)
        .filter(|rect| {
            rect.top() > button.rect.bottom()
                && rect.top() < button.rect.bottom() + 24.0
                && rect.width() > 180.0
                && rect.width() < 260.0
                && rect.height() > 180.0
        })
        .min_by(|a, b| a.width().total_cmp(&b.width()))
        .expect("a popover opened under the button");
    let at = frame.center() + vec2(30.0, -30.0);
    bars.click_at(at);
    let (_, marks) = bars.document("red words");
    assert!(
        marks
            .marks()
            .iter()
            .any(|mark| mark.kind == sue_model::mark_kind::COLOR),
        "{marks:?}"
    );
    bars.step(vec![key(egui::Key::Escape, egui::Modifiers::NONE)]);
    bars.settle(3);
    bars.click("Undo");
    assert_eq!(bars.arena(), before);
}
