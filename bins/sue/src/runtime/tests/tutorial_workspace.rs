//! P5-2 tutorial acceptance: the two workspace customization videos
//! (`tutorial-workspace.md` of the 0.0.8 plan, rows A1 to A40 and B1 to
//! B32), followed in the real Sue Paint row by row. Each test is named
//! after its row and drives real frames: controls are found by the name
//! they tell assistive technology or where the frame drew their text,
//! never at fixed places. Rows A37 and B32 (workspaces as downloaded
//! materials) are excluded by the owner; A38 to A40 are not features of
//! the paint program (`acceptance/workspace.md` of the plan says why).

use super::*;
use egui::{Event, Modifiers, PointerButton, Rect, pos2};

/// Sue Paint at 1440 by 900 with a new canvas, its palettes laid out as
/// the layout style does on a first start (the workspace the videos
/// start from), driven a frame at a time.
struct Walk {
    paint: PaintDriver,
    /// The controls of the last frame: name, rect, on.
    nodes: Vec<(String, Rect, Option<bool>)>,
    /// The keys held.
    held: Modifiers,
}

impl Walk {
    fn new(tag: &str) -> Self {
        Self::with(PaintDriver::fresh(tag))
    }

    fn with(paint: PaintDriver) -> Self {
        paint.ctx.enable_accesskit();
        let mut walk = Self {
            paint,
            nodes: Vec::new(),
            held: Modifiers::NONE,
        };
        // The pointer over the canvas, where no tooltip shows.
        walk.step(vec![Event::PointerMoved(pos2(720.0, 450.0))]);
        walk.settle(6);
        walk
    }

    fn step(&mut self, events: Vec<Event>) {
        let paint = &mut self.paint;
        let mut all = vec![Event::ModifiersChanged(self.held)];
        all.extend(events);
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events: all,
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
        self.nodes = output
            .platform_output
            .accesskit_update
            .iter()
            .flat_map(|update| update.nodes.iter())
            .filter_map(|(_, node)| {
                let label = node.label()?;
                let bounds = node.bounds()?;
                let rect = Rect::from_min_max(
                    pos2(bounds.x0 as f32, bounds.y0 as f32),
                    pos2(bounds.x1 as f32, bounds.y1 as f32),
                );
                let on = node
                    .toggled()
                    .map(|toggled| toggled == egui::accesskit::Toggled::True);
                Some((label.to_owned(), rect, on))
            })
            .collect();
        output.drop_without_applying_deltas();
    }

    fn settle(&mut self, count: usize) {
        for _ in 0..count {
            self.step(Vec::new());
        }
    }

    /// The top of the palettes: under Sue's bar and the menu bar.
    fn top(&self) -> f32 {
        self.paint.page_top() + 40.0
    }

    /// Every control named `name`.
    fn all_named(&self, name: &str) -> Vec<Rect> {
        self.nodes
            .iter()
            .filter(|(label, _, _)| label == name)
            .map(|(_, rect, _)| *rect)
            .collect()
    }

    /// The control `name`: the topmost, then the leftmost.
    fn node(&self, name: &str) -> Option<Rect> {
        self.node_in(name, Rect::EVERYTHING)
    }

    /// The control `name` inside `area`: the topmost, then the leftmost.
    fn node_in(&self, name: &str, area: Rect) -> Option<Rect> {
        self.all_named(name)
            .into_iter()
            .filter(|rect| area.contains(rect.center()))
            .min_by(|a, b| {
                a.top()
                    .total_cmp(&b.top())
                    .then(a.left().total_cmp(&b.left()))
            })
    }

    /// Whether the control `name` in `area` is on (a check or a toggle).
    fn on(&self, name: &str, area: Rect) -> Option<bool> {
        self.nodes
            .iter()
            .find(|(label, rect, _)| label == name && area.contains(rect.center()))
            .and_then(|(_, _, on)| *on)
    }

    /// Where `text` is drawn among the palettes (under the menu bar), the
    /// leftmost first.
    fn shown(&self, text: &str) -> Option<Rect> {
        let top = self.top();
        self.paint
            .texts
            .iter()
            .filter(|(found, rect)| found == text && rect.top() >= top)
            .map(|(_, rect)| *rect)
            .min_by(|a, b| a.left().total_cmp(&b.left()))
    }

    /// Whether `text` is drawn anywhere.
    fn drawn(&self, text: &str) -> bool {
        self.paint.texts.iter().any(|(found, _)| found == text)
    }

    /// A palette's tab by its key, where the last frame drew it.
    fn tab(&self, key: &str) -> Option<Rect> {
        sue_wm::dock::tab_place(&self.paint.ctx, key)
    }

    fn press(&mut self, at: Pos2, button: PointerButton) {
        let press = |pressed| Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: self.held,
        };
        let (down, up) = (press(true), press(false));
        self.step(vec![Event::PointerMoved(at)]);
        self.step(vec![down]);
        self.step(vec![up]);
        // Dialogs take a few frames to find how tall they are.
        self.settle(8);
    }

    fn double_click(&mut self, at: Pos2) {
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.step(vec![Event::PointerMoved(at)]);
        for _ in 0..2 {
            self.step(vec![press(true)]);
            self.step(vec![press(false)]);
        }
        self.settle(8);
    }

    /// A click on the control `name`.
    fn click(&mut self, name: &str) {
        let at = self.expect(name).center();
        self.press(at, PointerButton::Primary);
    }

    /// A right click on the control `name`.
    fn right_click(&mut self, name: &str) {
        let at = self.expect(name).center();
        self.press(at, PointerButton::Secondary);
    }

    fn expect(&self, name: &str) -> Rect {
        self.node(name).unwrap_or_else(|| {
            let names: Vec<&str> = self.nodes.iter().map(|(n, _, _)| n.as_str()).collect();
            panic!("{name:?} is drawn: {names:?}")
        })
    }

    /// A click on the text `text`, where the last frame drew it.
    fn click_text(&mut self, text: &str) {
        let find = |walk: &Self| {
            walk.paint
                .rect_of(text)
                .unwrap_or_else(|| panic!("{text:?} is on screen: {:?}", walk.paint.texts))
                .center()
        };
        // The pointer comes to it first, and the click lands where it is
        // then (a menu may settle into its place meanwhile).
        let at = find(self);
        self.step(vec![Event::PointerMoved(at)]);
        self.settle(4);
        let at = find(self);
        self.press(at, PointerButton::Primary);
    }

    /// The pointer moved to `to` and resting there.
    fn hover(&mut self, to: Pos2) {
        let from = self
            .paint
            .ctx
            .input(|input| input.pointer.latest_pos())
            .unwrap_or(to);
        for index in 1..=8 {
            let at = from + (to - from) * (index as f32 / 8.0);
            self.step(vec![Event::PointerMoved(at)]);
        }
        self.settle(8);
    }

    /// A drag from `from` to `to` with the primary button, slow enough
    /// for the dock to show where it lands.
    fn drag(&mut self, from: Pos2, to: Pos2) {
        let press = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: self.held,
        };
        let (down, up) = (press(from, true), press(to, false));
        self.step(vec![Event::PointerMoved(from)]);
        self.step(vec![down]);
        for index in 1..=20 {
            let at = from + (to - from) * (index as f32 / 20.0);
            self.step(vec![Event::PointerMoved(at)]);
        }
        self.step(vec![up]);
        self.settle(10);
    }

    /// The menu bar's `menu`, then its entry `entry` (found in the menu
    /// that opened, never among the palettes).
    fn menu(&mut self, menu: &str, entry: &str) {
        self.paint.step(Vec::new());
        let title = self
            .paint
            .texts
            .iter()
            .filter(|(text, _)| text == menu)
            .map(|(_, rect)| *rect)
            .min_by(|a, b| a.top().total_cmp(&b.top()))
            .unwrap_or_else(|| panic!("the {menu} menu"));
        self.press(title.center(), PointerButton::Primary);
        let at = self
            .menu_entry(title, entry)
            .unwrap_or_else(|| panic!("{entry:?} in {menu}: {:?}", self.paint.texts));
        self.press(at.center(), PointerButton::Primary);
    }

    /// Where the open menu under `title` shows `entry`.
    fn menu_entry(&self, title: Rect, entry: &str) -> Option<Rect> {
        self.paint
            .texts
            .iter()
            .filter(|(text, rect)| {
                text == entry
                    && rect.top() > title.bottom()
                    && rect.left() > title.left() - 12.0
                    && rect.left() < title.left() + 90.0
            })
            .map(|(_, rect)| *rect)
            .min_by(|a, b| a.top().total_cmp(&b.top()))
    }

    /// The entries the menu bar's `menu` holds, top to bottom; the menu
    /// is closed again.
    fn menu_entries(&mut self, menu: &str) -> Vec<String> {
        self.paint.step(Vec::new());
        let title = self
            .paint
            .texts
            .iter()
            .filter(|(text, _)| text == menu)
            .map(|(_, rect)| *rect)
            .min_by(|a, b| a.top().total_cmp(&b.top()))
            .unwrap_or_else(|| panic!("the {menu} menu"));
        self.press(title.center(), PointerButton::Primary);
        let mut found: Vec<(f32, String)> = self
            .paint
            .texts
            .iter()
            .filter(|(_, rect)| {
                rect.top() > title.bottom()
                    && rect.left() > title.left() - 12.0
                    && rect.left() < title.left() + 90.0
            })
            .map(|(text, rect)| (rect.top(), text.clone()))
            .collect();
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.step(vec![key(egui::Key::Escape, Modifiers::NONE)]);
        self.settle(4);
        found.into_iter().map(|(_, text)| text).collect()
    }

    /// Opens the palette menu of the palette titled `title`.
    fn palette_menu(&mut self, title: &str) {
        self.click(&format!("{title}: palette menu"));
    }

    /// The text of a file in Sue Paint's data folder.
    fn file(&self, name: &str) -> String {
        std::fs::read_to_string(self.paint.root.join("data").join("paint").join(name))
            .unwrap_or_default()
    }

    /// Where the dock's chrome button `name` was drawn last.
    fn chrome(&self, name: &str) -> Option<Rect> {
        self.paint
            .ctx
            .data(|data| {
                data.get_temp::<(u64, Rect)>(egui::Id::unique(("sue-paint-chrome-place", name)))
            })
            .map(|(_, rect)| rect)
    }

    /// Where button `index` of a floating palette's header is: roll up
    /// (0), dock again (1), close (2).
    fn float_button(&self, key: &str, index: usize) -> Option<Pos2> {
        self.paint
            .ctx
            .data(|data| {
                data.get_temp::<Rect>(egui::Id::unique(("sue-paint-float-button", key, index)))
            })
            .map(|rect| rect.center())
    }

    /// Drags the palette tab `key` to `to`: out of its group it floats.
    fn tear(&mut self, key: &str, to: Pos2) {
        let tab = self
            .tab(key)
            .unwrap_or_else(|| panic!("the {key} tab"))
            .center();
        self.drag(tab, to);
    }

    /// The command bar's strip across the top of the page.
    fn bar_area(&self) -> Rect {
        let page = self.paint.runtime.shell.page_rect();
        Rect::from_min_size(page.min, vec2(page.width(), 44.0))
    }

    /// The names of the command bar's buttons, left to right.
    fn bar_names(&self) -> Vec<String> {
        let area = self.bar_area();
        let mut found: Vec<(f32, String)> = self
            .nodes
            .iter()
            .filter(|(_, rect, _)| area.contains(rect.center()) && rect.width() < 60.0)
            .map(|(label, rect, _)| (rect.left(), label.clone()))
            .collect();
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        found.dedup_by(|a, b| a.1 == b.1);
        found.into_iter().map(|(_, name)| name).collect()
    }

    /// Types `text` over what the focused field holds.
    fn type_over(&mut self, text: &str) {
        self.step(vec![key(egui::Key::A, Modifiers::COMMAND)]);
        self.step(vec![Event::Text(text.to_owned())]);
        self.settle(2);
    }

    fn enter(&mut self) {
        self.step(vec![key(egui::Key::Enter, Modifiers::NONE)]);
        self.settle(8);
    }
}

/// Every palette name a person can read in the dock, by its key.
const PALETTES: [(&str, &str); 21] = [
    ("sub_tool", "Sub Tool"),
    ("tool_property", "Tool Property"),
    ("brush_grid", "Brush Size"),
    ("brush_size", "Tool Sliders"),
    ("color", "Color Wheel"),
    ("color_slider", "Color Slider"),
    ("color_set", "Color Set"),
    ("intermediate_color", "Intermediate Color"),
    ("approximate_color", "Approximate Color"),
    ("color_history", "Color History"),
    ("navigator", "Navigator"),
    ("sub_view", "Sub View"),
    ("item_bank", "Item Bank"),
    ("info", "Information"),
    ("layer_property", "Layer Property"),
    ("layer", "Layer"),
    ("history", "History"),
    ("auto_action", "Auto Action"),
    ("animation_cels", "Animation Cels"),
    ("material", "Material"),
    ("timeline", "Timeline"),
];

impl Walk {
    /// Goes down the menu bar's `path`: the menu, then each entry, a
    /// submenu opening under the pointer that rests on it.
    fn menu_path(&mut self, path: &[&str]) {
        self.paint.step(Vec::new());
        let title = self
            .paint
            .texts
            .iter()
            .filter(|(text, _)| text == path[0])
            .map(|(_, rect)| *rect)
            .min_by(|a, b| a.top().total_cmp(&b.top()))
            .unwrap_or_else(|| panic!("the {} menu", path[0]));
        self.press(title.center(), PointerButton::Primary);
        let mut under = self
            .menu_entry(title, path[1])
            .unwrap_or_else(|| panic!("{:?} in {}: {:?}", path[1], path[0], self.paint.texts));
        for entry in &path[2..] {
            self.hover(under.center());
            let parent = under;
            under = self
                .paint
                .texts
                .iter()
                .filter(|(text, rect)| {
                    text == entry
                        && rect.left() > parent.right()
                        && rect.top() > parent.top() - 40.0
                })
                .map(|(_, rect)| *rect)
                .min_by(|a, b| a.top().total_cmp(&b.top()))
                .unwrap_or_else(|| panic!("{entry:?} in the submenu: {:?}", self.paint.texts));
        }
        self.press(under.center(), PointerButton::Primary);
    }

    /// The entries of the submenu `path[1..]` of the menu `path[0]`, left
    /// open; top to bottom, with their rects.
    fn submenu(&mut self, menu: &str, entry: &str) -> Vec<(String, Rect)> {
        self.paint.step(Vec::new());
        let title = self
            .paint
            .texts
            .iter()
            .filter(|(text, _)| text == menu)
            .map(|(_, rect)| *rect)
            .min_by(|a, b| a.top().total_cmp(&b.top()))
            .unwrap_or_else(|| panic!("the {menu} menu"));
        self.press(title.center(), PointerButton::Primary);
        let parent = self
            .menu_entry(title, entry)
            .unwrap_or_else(|| panic!("{entry:?} in {menu}"));
        self.hover(parent.center());
        let mut found: Vec<(String, Rect)> = self
            .paint
            .texts
            .iter()
            .filter(|(_, rect)| rect.left() > parent.right() && rect.top() > parent.top() - 40.0)
            .cloned()
            .collect();
        found.sort_by(|a, b| a.1.top().total_cmp(&b.1.top()));
        found
    }

    /// The palette names a person reads among the palettes now.
    fn palettes_shown(&self) -> Vec<&'static str> {
        PALETTES
            .iter()
            .filter(|(_, name)| self.shown(name).is_some())
            .map(|(_, name)| *name)
            .collect()
    }

    /// The Command Bar Settings dialog's `category`, searched for `entry`,
    /// which is picked; where the search field is.
    fn find_entry(&mut self, category: &str, entry: &str) -> Rect {
        self.click("Category");
        self.click_text(category);
        let search = self.paint.rect_of("Search").expect("the search field");
        self.press(search.center(), PointerButton::Primary);
        self.step(vec![Event::Text(entry.to_owned())]);
        self.settle(3);
        let column = Rect::from_min_max(
            pos2(search.left() - 40.0, search.bottom()),
            pos2(search.left() + 240.0, 900.0),
        );
        let at = self
            .paint
            .texts
            .iter()
            .filter(|(text, rect)| text == entry && column.contains(rect.center()))
            .map(|(_, rect)| rect.center())
            .next()
            .unwrap_or_else(|| panic!("{entry:?} in {category}"));
        self.press(at, PointerButton::Primary);
        search
    }

    /// Adds `entry` of `category` to the bar in Command Bar Settings.
    fn add_from(&mut self, category: &str, entry: &str) {
        let search = self.find_entry(category, entry);
        self.click_text("Add");
        self.press(search.center(), PointerButton::Primary);
        self.step(vec![key(egui::Key::A, Modifiers::COMMAND)]);
        self.step(vec![key(egui::Key::Backspace, Modifiers::NONE)]);
        self.settle(2);
    }

    /// The dialog's Close button: the lowest control of that name.
    fn close_dialog(&mut self) {
        let at = self
            .all_named("Close")
            .into_iter()
            .max_by(|a, b| a.top().total_cmp(&b.top()))
            .expect("the dialog's Close")
            .center();
        self.press(at, PointerButton::Primary);
    }

    /// The tool buttons of the Tool palette, by name.
    fn tools(&self) -> Vec<(String, Rect)> {
        self.nodes
            .iter()
            .filter(|(name, _, _)| name.ends_with(')') && name.contains(" ("))
            .filter(|(name, _, _)| {
                ["Pen (P)", "Eraser (E)", "Fill (G)", "Hand (H)", "Text (T)"]
                    .iter()
                    .any(|tool| tool == name)
            })
            .map(|(name, rect, _)| (name.clone(), *rect))
            .collect()
    }

    /// Where the canvas has its middle.
    fn canvas(&self) -> Pos2 {
        paint_middle(&self.paint.runtime)
    }
}

// ----- Video A: Customizing your Clip Studio Paint Workspace ---------------

/// A1: the default workspace has the Command Bar on top, the Tool palette,
/// one left column with Sub Tool, Tool Property, Brush Size and the six
/// color palettes, at the right the Navigator group, Layer Property and
/// the Layer group, a thin strip with Quick Access and Material, and the
/// Timeline folded to an icon under the canvas.
#[test]
fn a01_the_default_workspace() {
    let walk = Walk::new("ws-a01");
    let bar = walk.bar_area();
    assert!(walk.node_in("Undo", bar).is_some(), "the Command Bar");
    let pen = walk.expect("Pen (P)");
    assert!(pen.right() < 46.0, "the Tool palette at the far left");
    let sub = walk.tab("sub_tool").expect("Sub Tool");
    let property = walk.tab("tool_property").expect("Tool Property");
    let size = walk.tab("brush_grid").expect("Brush Size");
    let wheel = walk.tab("color").expect("Color Wheel");
    assert!(sub.top() < property.top() && property.top() < size.top());
    assert!(size.top() < wheel.top());
    for rect in [sub, property, size, wheel] {
        assert!(rect.left() > pen.right() && rect.right() < 400.0);
    }
    for key in [
        "color_slider",
        "color_set",
        "intermediate_color",
        "approximate_color",
        "color_history",
    ] {
        let tab = walk.tab(key).expect(key);
        assert_eq!(tab.top(), wheel.top(), "{key} is a tab beside the wheel");
    }
    let navigator = walk.tab("navigator").expect("Navigator");
    let layer_property = walk.tab("layer_property").expect("Layer Property");
    let layer = walk.tab("layer").expect("Layer");
    assert!(navigator.left() > 1000.0);
    assert!(navigator.top() < layer_property.top() && layer_property.top() < layer.top());
    let strip = Rect::from_min_max(pos2(1400.0, 60.0), pos2(1440.0, 900.0));
    for icon in ["Quick Access", "Material"] {
        assert!(walk.node_in(icon, strip).is_some(), "{icon} in the strip");
    }
    let timeline = walk.expect("Timeline");
    assert!(
        timeline.top() > 800.0 && timeline.left() < 700.0,
        "{timeline:?}"
    );
    assert!(walk.shown("Timeline").is_none(), "folded to its icon");
}

/// A2: the Tool palette is widened into more columns, a tool is taken
/// off and added back.
#[test]
fn a02_the_tool_palette_widens_and_loses_and_gains_tools() {
    let mut walk = Walk::new("ws-a02");
    let pen = walk.expect("Pen (P)");
    let column_x = |walk: &Walk| {
        let mut xs: Vec<i32> = walk
            .tools()
            .iter()
            .map(|(_, rect)| rect.left().round() as i32)
            .collect();
        xs.sort_unstable();
        xs.dedup();
        xs.len()
    };
    assert_eq!(column_x(&walk), 1, "one column at first");
    // Its column's edge dragged to the right.
    let edge = pos2(46.5, pen.center().y + 200.0);
    walk.drag(edge, pos2(140.0, edge.y));
    assert!(column_x(&walk) > 1, "more columns: {:?}", walk.tools());
    // Delete Tool and Add Tool from Default, from its menu.
    walk.click("Pen (P)");
    walk.palette_menu("Tool");
    walk.click_text("Delete Tool");
    assert!(walk.node("Pen (P)").is_none(), "the Pen is off the palette");
    walk.palette_menu("Tool");
    walk.click_text("Add Tool from Default");
    assert!(walk.node("Pen (P)").is_some(), "the Pen is back");
}

/// A3: a tool shows its sub tools, some in groups shown as tabs (the
/// Pen's Pen and Marker); the Selection tool has one group.
#[test]
fn a03_a_tool_shows_its_sub_tools_in_group_tabs() {
    let mut walk = Walk::new("ws-a03");
    walk.click("Pen (P)");
    let pen = walk.expect("Pen");
    let marker = walk.expect("Marker");
    assert_eq!(pen.top(), marker.top(), "two group tabs side by side");
    assert!(walk.node("G-pen").is_some());
    walk.click("Marker");
    assert!(walk.node("G-pen").is_none(), "the Marker group shows");
    walk.click("Pen");
    assert!(walk.node("G-pen").is_some());
    walk.click("Selection (M)");
    assert!(walk.node("Lasso").is_some(), "{:?}", walk.paint.texts);
}

/// A4: picking a sub tool shows its Tool Property under the Sub Tool
/// palette, both at once.
#[test]
fn a04_a_sub_tool_shows_its_tool_property_below() {
    let mut walk = Walk::new("ws-a04");
    // The list shows two rows in the default layout: the second one.
    walk.click_text("Grainy G-pen");
    let property = walk.tab("tool_property").expect("Tool Property");
    assert!(walk.node("G-pen").is_some(), "the sub tool list stays");
    let named = walk
        .paint
        .texts
        .iter()
        .any(|(text, rect)| text == "Grainy G-pen" && rect.top() > property.bottom());
    assert!(named, "Tool Property shows the Grainy G-pen");
}

/// A5: the Brush Size palette is a grid of sizes; a click sets the size.
#[test]
fn a05_the_brush_size_palette_sets_the_size() {
    let mut walk = Walk::new("ws-a05");
    walk.click("Brush Size 8");
    let property = walk.tab("tool_property").expect("Tool Property");
    let size = walk.tab("brush_grid").expect("Brush Size");
    let shown = walk.paint.texts.iter().any(|(text, rect)| {
        text == "8.0" && rect.top() > property.bottom() && rect.bottom() < size.top()
    });
    assert!(shown, "Tool Property reads 8: {:?}", walk.paint.texts);
}

/// A6: the color palettes are tabs of one window, each shown by its tab.
#[test]
fn a06_the_color_palettes_are_tabs_of_one_window() {
    let mut walk = Walk::new("ws-a06");
    let wheel = walk.tab("color").expect("Color Wheel");
    for key in [
        "color_slider",
        "color_set",
        "intermediate_color",
        "approximate_color",
        "color_history",
    ] {
        let tab = walk.tab(key).expect(key);
        assert_eq!(tab.top(), wheel.top());
    }
    let slider = walk.tab("color_slider").unwrap().center();
    walk.press(slider, PointerButton::Primary);
    assert!(walk.node("HSV").is_some(), "the Color Slider shows");
    let wheel = walk.tab("color").unwrap().center();
    walk.press(wheel, PointerButton::Primary);
    assert!(walk.node("HSV").is_none(), "the Color Wheel shows again");
}

/// A7: the Command Bar holds menu commands as icons, and takes tools and
/// sub tools as well.
#[test]
fn a07_the_command_bar_holds_commands_tools_and_sub_tools() {
    let mut walk = Walk::new("ws-a07");
    let names = walk.bar_names();
    for name in ["New", "Open Picture", "Undo", "Redo"] {
        assert!(names.iter().any(|found| found == name), "{name}: {names:?}");
    }
    walk.right_click("Undo");
    walk.click_text("Command Bar Settings...");
    walk.add_from("Tools", "Eraser");
    walk.add_from("Sub Tools", "G-pen");
    walk.close_dialog();
    let names = walk.bar_names();
    for name in ["Eraser", "G-pen"] {
        assert!(names.iter().any(|found| found == name), "{name}: {names:?}");
    }
    let area = walk.bar_area();
    let eraser = walk.node_in("Eraser", area).unwrap().center();
    walk.press(eraser, PointerButton::Primary);
    assert_eq!(walk.on("Eraser (E)", Rect::EVERYTHING), Some(true));
}

/// A8: Window, Command Bar hides the bar, and shows it again.
#[test]
fn a08_window_command_bar_hides_it() {
    let mut walk = Walk::new("ws-a08");
    walk.menu("Window", "Command Bar");
    assert!(
        !walk.bar_names().iter().any(|name| name == "Undo"),
        "hidden"
    );
    walk.menu("Window", "Command Bar");
    assert!(walk.bar_names().iter().any(|name| name == "Undo"), "back");
}

/// A9: Quick Access, from its icon and from the Window menu: buttons with
/// icon and name, in sets; a set is made and deleted from its menu.
#[test]
fn a09_quick_access_has_named_buttons_in_sets() {
    let mut walk = Walk::new("ws-a09");
    walk.click("Quick Access");
    for set in ["Set 1", "Set 2"] {
        assert!(walk.drawn(set), "{set}");
    }
    let undo = walk
        .paint
        .texts
        .iter()
        .any(|(text, rect)| text == "Undo" && rect.left() > 700.0);
    assert!(undo, "Undo by its name in Quick Access");
    walk.palette_menu("Quick Access");
    let items: Vec<String> = walk
        .paint
        .texts
        .iter()
        .map(|(text, _)| text.clone())
        .collect();
    for item in ["New Set", "Delete Set", "Quick Access Settings..."] {
        assert!(items.iter().any(|text| text == item), "{item}: {items:?}");
    }
    walk.click_text("New Set");
    assert!(walk.drawn("Set 3"), "{:?}", walk.paint.texts);
    walk.palette_menu("Quick Access");
    walk.click_text("Delete Set");
    assert!(!walk.drawn("Set 3"));
    walk.press(pos2(720.0, 450.0), PointerButton::Primary);
    walk.menu("Window", "Quick Access");
    assert!(walk.drawn("Set 1"), "Window, Quick Access shows it");
}

/// A10: the Material icon in the strip opens the Material palette (local
/// materials; downloading is excluded).
#[test]
fn a10_the_material_icon_opens_its_palette() {
    let mut walk = Walk::new("ws-a10");
    assert!(walk.shown("Material").is_none());
    walk.click("Material");
    let shown = walk
        .paint
        .texts
        .iter()
        .any(|(text, rect)| text == "Material" && rect.left() < 1400.0);
    assert!(shown, "{:?}", walk.paint.texts);
}

/// A11: at the top right, Navigator, Sub View, Item Bank and Information
/// are tabs of one group.
#[test]
fn a11_the_navigator_group_has_four_tabs() {
    let mut walk = Walk::new("ws-a11");
    let navigator = walk.tab("navigator").expect("Navigator");
    for key in ["sub_view", "item_bank", "info"] {
        let tab = walk.tab(key).expect(key);
        assert_eq!(tab.top(), navigator.top(), "{key}");
        walk.press(tab.center(), PointerButton::Primary);
        let again = walk.tab(key).expect(key);
        assert_eq!(again.top(), navigator.top(), "{key} shows in its group");
    }
}

/// A12: Layer Property in a group of its own; Layer, History and Auto
/// Action as tabs; Search Layer is in the Layer palette.
#[test]
fn a12_the_layer_groups_and_search_layer() {
    let mut walk = Walk::new("ws-a12");
    let property = walk.tab("layer_property").expect("Layer Property");
    let layer = walk.tab("layer").expect("Layer");
    assert!(property.top() < layer.top());
    for key in ["history", "auto_action"] {
        assert_eq!(walk.tab(key).expect(key).top(), layer.top(), "{key}");
    }
    walk.click("Search Layer");
    assert!(walk.drawn("Layer name"), "{:?}", walk.paint.texts);
}

/// A13: the Timeline is an icon under the canvas; a click opens it.
#[test]
fn a13_the_timeline_icon_opens_the_timeline() {
    let mut walk = Walk::new("ws-a13");
    assert!(walk.shown("Timeline").is_none());
    walk.click("Timeline");
    let shown = walk.shown("Timeline").expect("the Timeline opened");
    assert!(shown.top() > 500.0, "under the canvas: {shown:?}");
}

/// A14: a single arrow folds a side into icons and a double arrow hides
/// it, on both sides and under the canvas.
#[test]
fn a14_arrows_fold_and_hide_every_side() {
    let mut walk = Walk::new("ws-a14");
    // The right side: folded, then back.
    let fold = walk.chrome("fold-side-right").expect("its fold arrow");
    walk.press(fold.center(), PointerButton::Primary);
    assert!(walk.shown("Navigator").is_none(), "folded");
    let strip = Rect::from_min_max(pos2(1380.0, 60.0), pos2(1440.0, 900.0));
    assert!(walk.node_in("Navigator", strip).is_some(), "as an icon");
    let fold = walk.chrome("fold-side-right").expect("its fold arrow");
    walk.press(fold.center(), PointerButton::Primary);
    assert!(walk.shown("Navigator").is_some(), "unfolded");
    // Hidden whole, then shown again.
    let hide = walk.chrome("close-side-right").expect("its hide arrow");
    walk.press(hide.center(), PointerButton::Primary);
    assert!(walk.shown("Navigator").is_none());
    assert!(walk.node_in("Navigator", strip).is_none(), "no icon either");
    let open = walk.chrome("open-side-right").expect("the bar to show it");
    walk.press(open.center(), PointerButton::Primary);
    assert!(walk.shown("Navigator").is_some());
    // The left side folds too.
    let fold = walk.chrome("fold-side-left").expect("the left fold arrow");
    walk.press(fold.center(), PointerButton::Primary);
    assert!(walk.shown("Sub Tool").is_none(), "the left side folded");
    let fold = walk.chrome("fold-side-left").expect("the left fold arrow");
    walk.press(fold.center(), PointerButton::Primary);
    assert!(walk.shown("Sub Tool").is_some());
    // Under the canvas: the Timeline open, folded back, hidden.
    walk.click("Timeline");
    assert!(walk.shown("Timeline").is_some());
    let fold = walk.chrome("fold-bottom").expect("the bottom's fold arrow");
    walk.press(fold.center(), PointerButton::Primary);
    assert!(walk.shown("Timeline").is_none());
    walk.click("Timeline");
    let hide = walk
        .chrome("close-bottom")
        .expect("the bottom's hide arrow");
    walk.press(hide.center(), PointerButton::Primary);
    assert!(walk.shown("Timeline").is_none());
    assert!(walk.node("Timeline").is_none(), "no icon either");
    let open = walk.chrome("open-row-1").expect("the bar to show it");
    walk.press(open.center(), PointerButton::Primary);
    assert!(walk.node("Timeline").is_some());
}

/// A15: Tab hides every palette and Window says Show All Palettes; Tab
/// again shows them as they were.
#[test]
fn a15_tab_hides_every_palette_and_shows_them_again() {
    let mut walk = Walk::new("ws-a15");
    let before = walk.palettes_shown();
    assert!(before.len() > 8, "{before:?}");
    walk.step(vec![key(egui::Key::Tab, Modifiers::NONE)]);
    walk.settle(6);
    assert!(
        walk.palettes_shown().is_empty(),
        "{:?}",
        walk.palettes_shown()
    );
    assert!(walk.node("Quick Access").is_none(), "the strip hides too");
    let entries = walk.menu_entries("Window");
    assert!(
        entries.iter().any(|text| text == "Show All Palettes"),
        "{entries:?}"
    );
    walk.step(vec![key(egui::Key::Tab, Modifiers::NONE)]);
    walk.settle(6);
    assert_eq!(walk.palettes_shown(), before);
    let entries = walk.menu_entries("Window");
    assert!(entries.iter().any(|text| text == "Hide All Palettes"));
}

/// A16: a palette's menu sits at its top left; the Timeline's ends with
/// Hide Timeline Palette, which hides it.
#[test]
fn a16_the_timeline_menu_ends_with_hide_timeline() {
    let mut walk = Walk::new("ws-a16");
    walk.click("Timeline");
    let tab = walk.tab("timeline").expect("the Timeline's tab");
    let button = walk.expect("Timeline: palette menu");
    assert!(button.right() <= tab.left() + 1.0, "{button:?} {tab:?}");
    walk.palette_menu("Timeline");
    let hide = walk
        .paint
        .rect_of("Hide Timeline Palette")
        .unwrap_or_else(|| panic!("{:?}", walk.paint.texts));
    let lowest = walk
        .paint
        .texts
        .iter()
        .filter(|(_, rect)| (rect.left() - hide.left()).abs() < 2.0)
        .map(|(_, rect)| rect.top())
        .fold(0.0, f32::max);
    assert_eq!(hide.top(), lowest, "the last entry");
    walk.click_text("Hide Timeline Palette");
    assert!(walk.shown("Timeline").is_none());
    assert!(walk.node("Timeline").is_none(), "gone, not an icon");
}

/// A17: Window, Timeline brings it back.
#[test]
fn a17_window_timeline_brings_it_back() {
    let mut walk = Walk::new("ws-a17");
    walk.click("Timeline");
    walk.palette_menu("Timeline");
    walk.click_text("Hide Timeline Palette");
    walk.menu("Window", "Timeline");
    assert!(
        walk.shown("Timeline").is_some(),
        "{:?}",
        walk.palettes_shown()
    );
}

/// A18: Tool Property has a Brush Size slider and a number field to type
/// the size into.
#[test]
fn a18_tool_property_brush_size_slider_and_number() {
    let mut walk = Walk::new("ws-a18");
    let property = walk.tab("tool_property").unwrap();
    let field = walk
        .paint
        .texts
        .iter()
        .find(|(text, rect)| text == "10.0" && rect.top() > property.bottom())
        .map(|(_, rect)| *rect)
        .expect("the size's number");
    walk.double_click(field.center());
    walk.type_over("25");
    walk.enter();
    assert!(walk.drawn("25.0"), "typed: {:?}", walk.paint.texts);
    // The slider's track, dragged to the left, makes it smaller.
    let track = walk
        .all_named("Brush Size")
        .into_iter()
        .filter(|rect| rect.top() > property.bottom() && rect.top() < field.bottom())
        .min_by(|a, b| a.left().total_cmp(&b.left()))
        .expect("the slider");
    walk.drag(
        pos2(track.right() - 2.0, track.center().y),
        pos2(track.left() + 4.0, track.center().y),
    );
    assert!(!walk.drawn("25.0"), "the slider changed it");
}

/// A19: the Brush Size tab dragged toward the middle floats in a window
/// that rolls up and closes; Close hides it until the Window menu shows
/// it again.
#[test]
fn a19_brush_size_floats_rolls_up_and_closes() {
    let mut walk = Walk::new("ws-a19");
    walk.tear("brush_grid", pos2(720.0, 400.0));
    let roll = walk.float_button("brush_grid", 0).expect("floating");
    assert!(walk.node("Brush Size 30").is_some());
    walk.press(roll, PointerButton::Primary);
    assert!(walk.node("Brush Size 30").is_none(), "rolled up");
    let roll = walk.float_button("brush_grid", 0).unwrap();
    walk.press(roll, PointerButton::Primary);
    assert!(walk.node("Brush Size 30").is_some(), "unrolled");
    let close = walk.float_button("brush_grid", 2).unwrap();
    walk.press(close, PointerButton::Primary);
    assert!(walk.tab("brush_grid").is_none(), "closed");
    assert!(walk.node("Brush Size 30").is_none(), "and not an icon");
    walk.menu("Window", "Brush Size");
    assert!(walk.node("Brush Size 30").is_some(), "shown again");
}

/// A20: the right column's edge dragged outward makes it wider.
#[test]
fn a20_a_column_edge_widens_the_palettes() {
    let mut walk = Walk::new("ws-a20");
    let before = walk.tab("navigator").unwrap();
    // The edge, on the column's side of the canvas's scroll bar.
    let menu = walk.expect("Navigator: palette menu");
    let at = pos2(menu.left() + 0.5, 400.0);
    walk.drag(at, pos2(at.x - 120.0, 400.0));
    let after = walk.tab("navigator").unwrap();
    assert!(after.left() < before.left() - 60.0, "{before:?} {after:?}");
}

/// A21: the color palettes dragged to the inner edge of the left column
/// make a new column of their own.
#[test]
fn a21_a_palette_dropped_at_a_column_edge_makes_a_column() {
    let mut walk = Walk::new("ws-a21");
    let sub = walk.tab("sub_tool").unwrap();
    let column_right = walk.tab("tool_property").unwrap().left() + 294.0;
    walk.tear("color", pos2(column_right + 6.0, 450.0));
    let wheel = walk.tab("color").expect("the Color Wheel");
    assert!(wheel.left() > column_right - 2.0, "a new column: {wheel:?}");
    assert_eq!(walk.tab("sub_tool").unwrap().left(), sub.left());
}

/// A22: tabs reorder, and one pulled into its own window lets the Color
/// Wheel and the Color Set show together.
#[test]
fn a22_tabs_reorder_and_pull_out() {
    let mut walk = Walk::new("ws-a22");
    let wheel = walk.tab("color").unwrap();
    let set = walk.tab("color_set").unwrap();
    walk.drag(set.center(), pos2(wheel.left() + 2.0, wheel.center().y));
    assert!(walk.tab("color_set").unwrap().left() < walk.tab("color").unwrap().left());
    walk.tear("color_set", pos2(720.0, 500.0));
    assert!(walk.shown("Color Set").is_some() && walk.shown("Color Wheel").is_some());
    let wheel = walk.tab("color").unwrap();
    assert!(wheel.left() < 400.0, "the Color Wheel stays docked");
}

/// A23: Sub View dragged under the colors keeps its buttons: a picture to
/// import, and picking colors from it.
#[test]
fn a23_sub_view_under_the_colors() {
    let mut walk = Walk::new("ws-a23");
    let wheel = walk.tab("color").unwrap();
    walk.tear("sub_view", pos2(wheel.center().x, 890.0));
    let tab = walk.tab("sub_view").expect("Sub View");
    assert!(tab.left() < 400.0 && tab.top() > wheel.top(), "{tab:?}");
    assert!(walk.node("Import Picture").is_some());
    assert!(walk.node("Switch to Eyedropper Automatically").is_some());
}

/// A24: the Information and Item Bank tabs dragged out and closed are
/// gone, not folded back.
#[test]
fn a24_information_and_item_bank_close_for_good() {
    let mut walk = Walk::new("ws-a24");
    for (key, name) in [("info", "Information"), ("item_bank", "Item Bank")] {
        walk.tear(key, pos2(720.0, 400.0));
        let close = walk.float_button(key, 2).expect("floating");
        walk.press(close, PointerButton::Primary);
        assert!(walk.shown(name).is_none(), "{name} closed");
        assert!(walk.node(name).is_none(), "{name} has no icon");
    }
    assert!(walk.tab("navigator").is_some());
}

/// A25: the line between Layer Property and Layer dragged up makes the
/// Layer palette taller.
#[test]
fn a25_the_line_between_palettes_moves() {
    let mut walk = Walk::new("ws-a25");
    let layer = walk.tab("layer").unwrap();
    let at = pos2(layer.center().x + 60.0, layer.top() - 2.0);
    walk.drag(at, pos2(at.x, at.y - 80.0));
    let after = walk.tab("layer").unwrap();
    assert!(after.top() < layer.top() - 40.0, "{layer:?} {after:?}");
}

impl Walk {
    /// A drag along `points` with the primary button (a lasso's loop).
    fn trace(&mut self, points: &[Pos2]) {
        let press = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.step(vec![Event::PointerMoved(points[0])]);
        self.step(vec![press(points[0], true)]);
        for pair in points.windows(2) {
            for index in 1..=6 {
                let at = pair[0] + (pair[1] - pair[0]) * (index as f32 / 6.0);
                self.step(vec![Event::PointerMoved(at)]);
            }
        }
        let last = *points.last().unwrap();
        self.step(vec![press(last, false)]);
        self.settle(8);
    }

    /// The mouse wheel turned over `at` by `lines` (down when positive).
    fn scroll(&mut self, at: Pos2, lines: f32) {
        self.step(vec![Event::PointerMoved(at)]);
        self.step(vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -lines * 40.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: Modifiers::NONE,
        }]);
        self.settle(8);
    }

    /// The window a floating palette `key` stands in.
    fn float_rect(&self, key: &str) -> Option<Rect> {
        let id = egui::Id::unique(("sue-paint-float", key, false));
        self.paint.ctx.memory(|memory| memory.area_rect(id))
    }

    /// Floats the palette `key` (by its tab or its icon `name`) over the
    /// canvas and closes it by its X.
    fn float_and_close(&mut self, key: &str, name: &str) {
        let to = pos2(720.0, 360.0);
        match self.tab(key) {
            Some(tab) => self.drag(tab.center(), to),
            None => {
                let icon = self.expect(name);
                self.drag(icon.center(), to);
            }
        }
        let close = self
            .float_button(key, 2)
            .unwrap_or_else(|| panic!("{key} floats: {:?}", self.palettes_shown()));
        self.press(close, PointerButton::Primary);
    }

    /// Whether the floating palette `key` stopped floating.
    fn float_button_gone(&mut self, key: &str) -> bool {
        let id = egui::Id::unique(("sue-paint-float-button", key, 2_usize));
        self.paint.ctx.data_mut(|data| data.remove::<Rect>(id));
        self.settle(2);
        self.float_button(key, 2).is_none()
    }

    /// How many columns the Tool palette's buttons stand in.
    fn tool_columns(&self) -> usize {
        let mut xs: Vec<i32> = self
            .tools()
            .iter()
            .map(|(_, rect)| rect.left().round() as i32)
            .collect();
        xs.sort_unstable();
        xs.dedup();
        xs.len()
    }
}

/// A26: icons are moved by dragging them: tools in the Tool palette, and
/// with Ctrl held on the Command Bar and in Quick Access.
#[test]
fn a26_icons_reorder_by_drag() {
    let mut walk = Walk::new("ws-a26");
    let pen = walk.expect("Pen (P)");
    let eraser = walk.expect("Eraser (E)");
    let to = pos2(eraser.center().x, eraser.bottom() - eraser.height() / 8.0);
    walk.drag(pen.center(), to);
    assert!(walk.expect("Pen (P)").top() > walk.expect("Eraser (E)").top());
    // The Command Bar, Ctrl held.
    let area = walk.bar_area();
    let undo = walk.node_in("Undo", area).unwrap();
    let redo = walk.node_in("Redo", area).unwrap();
    walk.held = Modifiers::COMMAND;
    walk.drag(redo.center(), undo.left_center() + vec2(2.0, 0.0));
    walk.held = Modifiers::NONE;
    let names = walk.bar_names();
    let redo = names.iter().position(|name| name == "Redo").unwrap();
    assert_eq!(names[redo + 1], "Undo", "{names:?}");
    // Quick Access, Ctrl held.
    walk.click("Quick Access");
    let in_quick = |walk: &Walk, name: &str| {
        walk.paint
            .texts
            .iter()
            .filter(|(text, rect)| text == name && rect.left() > 700.0 && rect.top() > 100.0)
            .map(|(_, rect)| *rect)
            .next()
    };
    let undo = in_quick(&walk, "Undo").expect("Undo in Quick Access");
    let redo = in_quick(&walk, "Redo").expect("Redo in Quick Access");
    assert!(undo.top() < redo.top());
    walk.held = Modifiers::COMMAND;
    // Let go on the line above Undo (over its middle it would group).
    walk.drag(redo.center(), pos2(redo.center().x, undo.top() - 6.0));
    walk.held = Modifiers::NONE;
    if in_quick(&walk, "Undo").is_none() {
        walk.click("Quick Access");
    }
    let undo = in_quick(&walk, "Undo").unwrap();
    let redo = in_quick(&walk, "Redo").unwrap();
    assert!(redo.top() < undo.top(), "Redo moved before Undo");
}

/// A27: a right click on the Command Bar opens its menu.
#[test]
fn a27_the_command_bar_right_click_menu() {
    let mut walk = Walk::new("ws-a27");
    walk.right_click("Delete");
    for item in [
        "Icon Settings...",
        "Delete",
        "Add Drawing Color",
        "Insert Drawing Color into Group",
        "Add Separator",
        "Change Order",
        "Command Bar Settings...",
    ] {
        assert!(walk.paint.rect_of(item).is_some(), "{item}");
    }
}

/// A28: Delete in that menu takes the icon off the bar.
#[test]
fn a28_delete_takes_an_icon_off_the_bar() {
    let mut walk = Walk::new("ws-a28");
    assert!(walk.bar_names().iter().any(|name| name == "Fill"));
    walk.right_click("Fill");
    walk.click_text("Delete");
    assert!(!walk.bar_names().iter().any(|name| name == "Fill"));
    assert!(walk.file("command_bar.txt").contains("command edit.undo"));
}

/// A29: the Tool palette's menu: Tool settings, Delete Tool, Add Tool
/// from Default, Show Color Icon, and Hide Tool Palette.
#[test]
fn a29_the_tool_palette_menu() {
    let mut walk = Walk::new("ws-a29");
    walk.click("Eraser (E)");
    walk.palette_menu("Tool");
    for item in [
        "Tool Settings...",
        "Delete Tool",
        "Add Tool from Default",
        "Show Color Icon",
        "Hide Tool Palette",
    ] {
        assert!(
            walk.paint.rect_of(item).is_some(),
            "{item}: {:?}",
            walk.paint.texts
        );
    }
    walk.click_text("Delete Tool");
    assert!(walk.node("Eraser (E)").is_none());
    walk.palette_menu("Tool");
    walk.click_text("Add Tool from Default");
    assert!(walk.node("Eraser (E)").is_some(), "added back");
}

/// A30: a sub tool dragged onto the Tool palette is a tool of its own;
/// dragged back into a tool's list it is a sub tool again.
#[test]
fn a30_a_sub_tool_becomes_a_tool_and_goes_back() {
    let mut walk = Walk::new("ws-a30");
    let grainy = walk
        .paint
        .rect_of("Grainy G-pen")
        .expect("Grainy G-pen in the list");
    let eraser = walk.expect("Eraser (E)");
    let to = pos2(eraser.center().x, eraser.top() + eraser.height() / 8.0);
    walk.drag(grainy.center(), to);
    let made = walk
        .all_named("Grainy G-pen")
        .into_iter()
        .find(|rect| rect.left() < 46.0)
        .expect("a tool of its own");
    walk.click("Pen (P)");
    let list = walk.paint.rect_of("G-pen").expect("the Pen's list");
    walk.drag(made.center(), list.center());
    let tools = walk
        .all_named("Grainy G-pen")
        .into_iter()
        .filter(|rect| rect.left() < 46.0)
        .count();
    assert_eq!(tools, 0, "no longer a tool");
    assert!(walk.node("Grainy G-pen").is_some(), "in the Pen's list");
}

/// A31: Command Bar Settings, from the bar's right-click menu: its
/// categories, Add, Delete, Add Separator, Insert into Group, Restore
/// Default Layout; a button of the bar picked in it is deleted.
#[test]
fn a31_command_bar_settings_from_the_bar() {
    let mut walk = Walk::new("ws-a31");
    walk.right_click("Undo");
    walk.click_text("Command Bar Settings...");
    walk.click("Category");
    for category in [
        "Main Menu",
        "Pop-up Palettes",
        "Tools",
        "Sub Tools",
        "Auto Action",
        "Drawing Colors",
    ] {
        assert!(walk.drawn(category), "{category}: {:?}", walk.paint.texts);
    }
    walk.step(vec![key(egui::Key::Escape, Modifiers::NONE)]);
    walk.settle(4);
    for button in [
        "Add",
        "Delete",
        "Add Separator",
        "Insert into Group",
        "Restore Default Layout",
    ] {
        assert!(walk.drawn(button), "{button}");
    }
    // The bar's buttons are listed at the right of the dialog (the bar
    // itself is behind it): Fill picked there, Delete takes it off.
    let heading = walk
        .paint
        .rect_of("On the Command Bar")
        .expect("the bar's buttons");
    let fill = walk
        .paint
        .texts
        .iter()
        .find(|(text, rect)| {
            text == "Fill" && rect.left() > heading.left() - 20.0 && rect.top() > heading.bottom()
        })
        .map(|(_, rect)| rect.center())
        .expect("Fill at the right");
    walk.press(fill, PointerButton::Primary);
    let delete = walk
        .paint
        .texts
        .iter()
        .filter(|(text, rect)| {
            text == "Delete" && rect.top() > 100.0 && rect.left() < heading.left()
        })
        .map(|(_, rect)| rect.center())
        .next()
        .expect("the dialog's Delete");
    walk.press(delete, PointerButton::Primary);
    assert!(
        !walk.bar_names().iter().any(|name| name == "Fill"),
        "{:?}",
        walk.bar_names()
    );
}

/// A32 and A33: a lasso selection shows the Selection Launcher under it;
/// its last button opens Selection Launcher Settings.
#[test]
fn a32_a33_a_lasso_shows_the_selection_launcher_and_its_settings() {
    let mut walk = Walk::new("ws-a32");
    walk.click("Selection (M)");
    // The list shows two rows in the default layout: scrolled to Lasso.
    let buttons_top = walk.expect("Duplicate Sub Tool").top();
    if walk
        .paint
        .rect_of("Lasso")
        .is_none_or(|rect| rect.bottom() > buttons_top)
    {
        let list = walk.tab("sub_tool").unwrap().center() + vec2(60.0, 100.0);
        walk.scroll(list, 1.0);
    }
    walk.click_text("Lasso");
    let middle = walk.canvas();
    walk.trace(&[
        middle + vec2(-60.0, -40.0),
        middle + vec2(60.0, -50.0),
        middle + vec2(70.0, 40.0),
        middle + vec2(-50.0, 50.0),
        middle + vec2(-60.0, -40.0),
    ]);
    let buttons = sue_paint::shown_buttons(&walk.paint.ctx);
    assert!(buttons.len() > 5, "the launcher shows: {buttons:?}");
    assert!(buttons[0].1.top() > middle.y + 40.0, "under the selection");
    let (last, rect) = buttons.last().unwrap().clone();
    assert_eq!(last, "launcher.settings");
    walk.press(rect.center(), PointerButton::Primary);
    assert!(walk.drawn("Selection Launcher Settings"));
}

/// A34: Window, Workspace, Register Workspace keeps the palettes, the
/// command bar, the shortcuts and the units by a name.
#[test]
fn a34_register_workspace_keeps_palettes_bar_keys_and_units() {
    let mut walk = Walk::new("ws-a34");
    // Undo on another key, lengths in millimeters.
    let undo = sue_wm::shortcuts::setting(KEYS, "edit.undo");
    set_key(&mut walk, "edit.undo", "Ctrl+Alt+U");
    walk.paint.command("info.unit_mm");
    walk.settle(2);
    walk.menu_path(&["Window", "Workspace", "Register Workspace..."]);
    assert!(walk.drawn("Register Workspace"), "{:?}", walk.paint.texts);
    // The dialog says what it keeps; the name takes the keys at once.
    assert!(walk.drawn(
        "Kept in the workspace: where the palettes are, the Command Bar and Quick Access, the keyboard shortcuts and the unit of lengths"
    ));
    walk.type_over("Tutorial");
    walk.enter();
    let kept = walk.file("workspaces.txt");
    assert!(kept.contains("[Tutorial]"), "{kept}");
    assert!(kept.contains("column"), "the palettes: {kept}");
    assert!(
        kept.contains("command edit.undo"),
        "the command bar: {kept}"
    );
    assert!(
        kept.contains(
            "
keys
"
        ),
        "the shortcuts: {kept}"
    );
    assert!(kept.contains("edit.undo=Ctrl+Alt+U"), "Undo's key: {kept}");
    assert!(kept.contains("unit mm"), "the unit: {kept}");
    // Changed since, and the workspace picked again: both come back.
    set_key(&mut walk, "edit.undo", "Ctrl+Z");
    walk.paint.command("info.unit_px");
    walk.settle(2);
    walk.menu_path(&["Window", "Workspace", "Tutorial"]);
    let rice = &walk.paint.runtime.engine.world.rice;
    let keys = sue_wm::shortcuts::SparseKeys::read(rice, KEYS);
    assert_eq!(
        keys.get("edit.undo").map(ToString::to_string),
        Some("Ctrl+Alt+U".to_owned()),
        "{undo}"
    );
    assert!(walk.file("information.txt").contains("unit mm"));
}

/// The section Sue Paint's keys are set in.
const KEYS: &str = "sue_paint_keys";

/// Gives `command` the keys `chords`, as Keyboard Shortcuts does.
fn set_key(walk: &mut Walk, command: &str, chords: &str) {
    let rice = &mut walk.paint.runtime.engine.world.rice;
    let (setting, value) = sue_wm::shortcuts::bind(rice, KEYS, command, &chords.parse().unwrap());
    rice.set(&setting, value).unwrap();
}

/// A35: Window, Workspace lists Reset to Default, Register Workspace,
/// Manage Workspaces, Reload, the presets and the registered workspaces,
/// the one in use checked.
#[test]
fn a35_the_workspace_submenu() {
    let mut walk = Walk::new("ws-a35");
    walk.menu_path(&["Window", "Workspace", "Register Workspace..."]);
    walk.type_over("Tutorial");
    walk.enter();
    walk.menu_path(&["Window", "Workspace", "Tutorial"]);
    let entries: Vec<String> = walk
        .submenu("Window", "Workspace")
        .into_iter()
        .map(|(text, _)| text)
        .collect();
    let place = |name: &str| {
        entries
            .iter()
            .position(|text| text == name)
            .unwrap_or_else(|| panic!("{name} in {entries:?}"))
    };
    assert!(place("Reset to Default") < place("Register Workspace..."));
    assert!(place("Register Workspace...") < place("Manage Workspaces..."));
    assert!(place("Manage Workspaces...") < place("Reload \"Tutorial\""));
    assert!(place("Reload \"Tutorial\"") < place("Illustration"));
    assert!(place("Illustration") < place("Tutorial"));
    assert_eq!(walk.on("Tutorial", Rect::EVERYTHING), Some(true), "checked");
}

/// A36: a registered workspace picked later lays the palettes out again.
#[test]
fn a36_a_registered_workspace_is_picked_again() {
    let mut walk = Walk::new("ws-a36");
    walk.menu_path(&["Window", "Workspace", "Register Workspace..."]);
    walk.type_over("Tutorial");
    walk.enter();
    walk.palette_menu("Layer");
    walk.click_text("Hide Layer Palette");
    assert!(walk.tab("layer").is_none(), "{:?}", walk.paint.texts);
    walk.menu_path(&["Window", "Workspace", "Tutorial"]);
    assert!(walk.tab("layer").is_some(), "the Layer palette is back");
}

/// A39 (the program's side of a tablet's radial menu): the keys it sends
/// work: a tool's key, Ctrl+Z and Ctrl+Y.
#[test]
fn a39_the_keys_a_radial_menu_sends() {
    let mut walk = Walk::new("ws-a39");
    walk.step(vec![key(egui::Key::E, Modifiers::NONE)]);
    walk.settle(4);
    assert_eq!(walk.on("Eraser (E)", Rect::EVERYTHING), Some(true));
    walk.paint.command("layer.new");
    walk.settle(4);
    let steps = walk.paint.runtime.engine.history.depth();
    walk.step(vec![key(egui::Key::Z, Modifiers::COMMAND)]);
    walk.settle(4);
    assert_eq!(
        walk.paint.runtime.engine.history.depth(),
        steps - 1,
        "undone"
    );
    walk.step(vec![key(egui::Key::Y, Modifiers::COMMAND)]);
    walk.settle(4);
    assert_eq!(walk.paint.runtime.engine.history.depth(), steps, "redone");
}

// ----- Video B: How to: UI customization ---------------------------------

/// B1: closing the canvas leaves the workspace and its palettes.
#[test]
fn b01_closing_the_canvas_leaves_the_workspace() {
    let mut walk = Walk::new("ws-b01");
    walk.step(vec![key(egui::Key::W, Modifiers::COMMAND)]);
    walk.settle(10);
    assert!(walk.drawn("No canvas is open"), "{:?}", walk.paint.texts);
    for key in ["sub_tool", "tool_property", "color", "navigator", "layer"] {
        assert!(walk.tab(key).is_some(), "{key} stays");
    }
    assert!(walk.node("Pen (P)").is_some(), "the Tool palette stays");
    let entries = walk.menu_entries("Window");
    assert!(
        entries.iter().any(|text| text == "Workspace"),
        "{entries:?}"
    );
}

/// B2: Window, Workspace offers Sue Paint's preset workspaces, and the
/// workspace as it is registers under a name.
#[test]
fn b02_register_the_current_workspace_and_the_presets() {
    let mut walk = Walk::new("ws-b02");
    let entries: Vec<String> = walk
        .submenu("Window", "Workspace")
        .into_iter()
        .map(|(text, _)| text)
        .collect();
    for preset in ["Illustration", "Animation"] {
        assert!(
            entries.iter().any(|text| text == preset),
            "{preset}: {entries:?}"
        );
    }
    walk.step(vec![key(egui::Key::Escape, Modifiers::NONE)]);
    walk.settle(4);
    walk.menu_path(&["Window", "Workspace", "Register Workspace..."]);
    walk.type_over("MyCurrentWorkspace");
    walk.enter();
    assert!(walk.file("workspaces.txt").contains("MyCurrentWorkspace"));
}

/// Every palette of the default workspace: its key and the name its icon
/// or tab gives.
const ALL: [(&str, &str); 22] = [
    ("sub_tool", "Sub Tool"),
    ("tool_property", "Tool Property"),
    ("brush_grid", "Brush Size"),
    ("color", "Color Wheel"),
    ("color_slider", "Color Slider"),
    ("color_set", "Color Set"),
    ("intermediate_color", "Intermediate Color"),
    ("approximate_color", "Approximate Color"),
    ("color_history", "Color History"),
    ("navigator", "Navigator"),
    ("sub_view", "Sub View"),
    ("item_bank", "Item Bank"),
    ("info", "Information"),
    ("layer_property", "Layer Property"),
    ("layer", "Layer"),
    ("history", "History"),
    ("auto_action", "Auto Action"),
    ("quick_access", "Quick Access"),
    ("material", "Material"),
    ("animation_cels", "Animation Cels"),
    ("brush_size", "Tool Sliders"),
    ("timeline", "Timeline"),
];

/// B3's steps: every palette floated and closed by its X, the Tool
/// palette hidden by its menu and the Command Bar by the Window menu.
fn close_everything(walk: &mut Walk) {
    for (key, name) in ALL {
        walk.float_and_close(key, name);
    }
    walk.palette_menu("Tool");
    walk.click_text("Hide Tool Palette");
    walk.menu("Window", "Command Bar");
}

/// Whether nothing of a palette is on screen: no tab, no icon, no tool.
fn empty(walk: &Walk) -> Result<(), String> {
    // Under the menu bar, whose Layer menu is no palette.
    let below = Rect::from_min_max(pos2(0.0, walk.paint.page_top() + 2.0), pos2(1440.0, 900.0));
    let left: Vec<String> = ALL
        .iter()
        .filter(|(key, name)| walk.tab(key).is_some() || walk.node_in(name, below).is_some())
        .map(|(key, name)| format!("{name} {:?} {:?}", walk.tab(key), walk.node_in(name, below)))
        .collect();
    if !left.is_empty() {
        return Err(format!("still shown: {left:?}"));
    }
    if walk.node("Pen (P)").is_some() || walk.bar_names().iter().any(|name| name == "Undo") {
        return Err("the Tool palette or the Command Bar shows".to_owned());
    }
    Ok(())
}

/// B3: every palette is floated and closed by its X, until none is left.
#[test]
fn b03_every_palette_floats_and_closes() {
    let mut walk = Walk::new("ws-b03");
    close_everything(&mut walk);
    if let Err(fault) = empty(&walk) {
        panic!("{fault}");
    }
}

/// B4: the empty workspace registers as "Empty"; after Reset to Default
/// brings the palettes back, picking "Empty" empties the screen again.
#[test]
fn b04_an_empty_workspace_stays_empty() {
    let mut walk = Walk::new("ws-b04");
    close_everything(&mut walk);
    walk.menu_path(&["Window", "Workspace", "Register Workspace..."]);
    walk.type_over("Empty");
    walk.enter();
    walk.menu_path(&["Window", "Workspace", "Reset to Default"]);
    assert!(walk.tab("layer").is_some(), "the default is back");
    walk.menu_path(&["Window", "Workspace", "Empty"]);
    if let Err(fault) = empty(&walk) {
        panic!("{fault}");
    }
}

/// B5: every palette is in the Window menu, with Hide All Palettes.
#[test]
fn b05_every_palette_is_in_the_window_menu() {
    let mut walk = Walk::new("ws-b05");
    let entries = walk.menu_entries("Window");
    for name in [
        "Command Bar",
        "Quick Access",
        "Tool",
        "Sub Tool",
        "Tool Property",
        "Brush Size",
        "Sub Tool Detail",
        "Color Wheel",
        "Color Slider",
        "Color Set",
        "Intermediate Color",
        "Approximate Color",
        "Color History",
        "Layer",
        "Layer Property",
        "Animation Cels",
        "Timeline",
        "Navigator",
        "Sub View",
        "History",
        "Auto Action",
        "Information",
        "Item Bank",
        "Material",
        "Hide All Palettes",
    ] {
        assert!(
            entries.iter().any(|text| text == name),
            "{name}: {entries:?}"
        );
    }
}

/// B6: Window, Layer shows the Layer palette closed while floating, in
/// its window, whose corner drag resizes it.
#[test]
fn b06_window_layer_shows_it_floating_and_it_resizes() {
    let mut walk = Walk::new("ws-b06");
    walk.float_and_close("layer", "Layer");
    assert!(walk.tab("layer").is_none());
    walk.menu("Window", "Layer");
    let window = walk.float_rect("layer").expect("the Layer palette floats");
    assert!(walk.float_button("layer", 2).is_some());
    let corner = window.right_bottom() - vec2(3.0, 3.0);
    walk.drag(corner, corner + vec2(80.0, 60.0));
    let after = walk.float_rect("layer").unwrap();
    assert!(
        after.width() > window.width() + 40.0,
        "{window:?} {after:?}"
    );
    assert!(after.height() > window.height() + 30.0);
}

/// B7: a double click on a floating palette's title rolls it up, and
/// another unrolls it.
#[test]
fn b07_a_double_click_rolls_a_floating_palette_up() {
    let mut walk = Walk::new("ws-b07");
    walk.tear("layer", pos2(720.0, 360.0));
    assert!(walk.node("Layer 1").is_some());
    let title = walk.tab("layer").expect("its title");
    walk.double_click(title.center());
    assert!(walk.node("Layer 1").is_none(), "rolled up");
    // A moment later (not a third click of the same gesture).
    walk.settle(30);
    let title = walk.tab("layer").expect("its title");
    walk.double_click(title.center());
    assert!(walk.node("Layer 1").is_some(), "unrolled");
}

/// B8: the Layer palette's menu lists layer commands.
#[test]
fn b08_the_layer_palette_menu_lists_layer_commands() {
    let mut walk = Walk::new("ws-b08");
    walk.palette_menu("Layer");
    for item in [
        "Raster Layer",
        "Vector Layer",
        "New Layer Folder",
        "Duplicate Layer",
        "Delete Layer",
        "Change Layer Name...",
        "Hide Layer Palette",
    ] {
        assert!(
            walk.paint.rect_of(item).is_some(),
            "{item}: {:?}",
            walk.paint.texts
        );
    }
}

/// B9: a floating palette dragged to the screen's right border docks
/// there.
#[test]
fn b09_a_palette_dragged_to_the_border_docks() {
    let mut walk = Walk::new("ws-b09");
    walk.tear("layer", pos2(720.0, 360.0));
    assert!(walk.float_button("layer", 2).is_some(), "floating");
    let title = walk.tab("layer").unwrap();
    walk.drag(title.center(), pos2(1395.0, 450.0));
    assert!(walk.float_button_gone("layer"), "docked");
    let tab = walk.tab("layer").unwrap();
    assert!(tab.left() > 1000.0, "at the right: {tab:?}");
}

/// B10: the single arrow at the top of the dock shrinks its palettes to
/// icons; an icon opens its palette.
#[test]
fn b10_the_single_arrow_folds_the_dock_into_icons() {
    let mut walk = Walk::new("ws-b10");
    let fold = walk.chrome("fold-side-right").unwrap();
    walk.press(fold.center(), PointerButton::Primary);
    let strip = Rect::from_min_max(pos2(1380.0, 60.0), pos2(1440.0, 900.0));
    for name in ["Navigator", "Layer Property", "Layer"] {
        assert!(walk.node_in(name, strip).is_some(), "{name} as an icon");
    }
    let icon = walk.node_in("Layer", strip).unwrap().center();
    walk.press(icon, PointerButton::Primary);
    assert!(walk.node("Layer 1").is_some(), "an icon opens its palette");
}

/// B11: the double arrow hides the whole dock and shows it again.
#[test]
fn b11_the_double_arrow_hides_and_shows_the_dock() {
    let mut walk = Walk::new("ws-b11");
    let hide = walk.chrome("close-side-right").unwrap();
    walk.press(hide.center(), PointerButton::Primary);
    assert!(walk.tab("layer").is_none() && walk.node("Layer 1").is_none());
    let open = walk.chrome("open-side-right").unwrap();
    walk.press(open.center(), PointerButton::Primary);
    assert!(walk.tab("layer").is_some());
}

/// B12: a right click on a folded icon offers Display Palette as Tab
/// (it stays open) and as Pop-up (a click elsewhere closes it).
#[test]
fn b12_folded_icons_show_as_tab_or_as_pop_up() {
    let mut walk = Walk::new("ws-b12");
    let open = |walk: &Walk| {
        walk.paint
            .texts
            .iter()
            .any(|(text, rect)| text == "Material" && rect.left() < 1400.0)
    };
    let strip = Rect::from_min_max(pos2(1400.0, 60.0), pos2(1440.0, 900.0));
    let icon = walk.node_in("Material", strip).expect("its icon").center();
    walk.press(icon, PointerButton::Secondary);
    walk.click_text("Display Palette as Tab");
    walk.press(icon, PointerButton::Primary);
    assert!(open(&walk));
    walk.press(pos2(720.0, 450.0), PointerButton::Primary);
    assert!(open(&walk), "as a tab it stays");
    walk.press(icon, PointerButton::Primary);
    assert!(!open(&walk), "its icon puts it away");
    walk.press(icon, PointerButton::Secondary);
    walk.click_text("Display Palette as Pop-up");
    walk.press(icon, PointerButton::Primary);
    assert!(open(&walk));
    walk.press(pos2(720.0, 450.0), PointerButton::Primary);
    assert!(!open(&walk), "as a pop-up a click elsewhere closes it");
}

/// B13 and B14: Window, Tool shows the Tool palette; floated, it widens
/// into several columns, docks at the left, and its column's edge makes
/// it one narrow column again.
#[test]
fn b13_b14_the_tool_palette_floats_widens_docks_and_narrows() {
    let mut walk = Walk::new("ws-b13");
    walk.palette_menu("Tool");
    walk.click_text("Hide Tool Palette");
    assert!(walk.node("Pen (P)").is_none());
    walk.menu("Window", "Tool");
    assert!(walk.node("Pen (P)").is_some(), "Window, Tool shows it");
    walk.palette_menu("Tool");
    walk.click_text("Float in a Window");
    let window = walk.float_rect("tool").expect("the Tool palette floats");
    // Its roll up, dock again and close buttons stand in a header of
    // their own, over none of its tools.
    let buttons: Vec<Pos2> = (0..3)
        .map(|index| walk.float_button("tool", index).expect("a header button"))
        .collect();
    let header = buttons.iter().map(|at| at.y).fold(0.0, f32::max) + 8.0;
    let zoom = walk.expect("Hand (H)");
    assert!(
        zoom.top() > header && buttons.iter().all(|at| !zoom.contains(*at)),
        "the first row {zoom:?} is clear of the header {buttons:?}"
    );
    let corner = window.right_bottom() - vec2(3.0, 3.0);
    walk.drag(corner, corner + vec2(120.0, 0.0));
    assert!(
        walk.tool_columns() > 1,
        "several columns: {:?}",
        walk.tools()
    );
    // Docked at the left by its title bar.
    let window = walk.float_rect("tool").unwrap();
    walk.drag(window.left_top() + vec2(20.0, 8.0), pos2(4.0, 450.0));
    assert!(walk.float_button_gone("tool"), "docked");
    let pen = walk.expect("Pen (P)");
    assert!(pen.left() < 120.0, "{pen:?}");
    // B14: its column's edge dragged in: one narrow column.
    let right = walk
        .tools()
        .iter()
        .map(|(_, rect)| rect.right())
        .fold(0.0, f32::max);
    walk.drag(pos2(right + 4.0, 450.0), pos2(30.0, 450.0));
    assert_eq!(walk.tool_columns(), 1, "{:?}", walk.tools());
}

/// B15: Tool Property, floating, dragged over a docked palette's header
/// joins it as a tab.
#[test]
fn b15_tool_property_joins_another_palette_as_a_tab() {
    let mut walk = Walk::new("ws-b15");
    walk.tear("tool_property", pos2(720.0, 360.0));
    assert!(walk.float_button("tool_property", 2).is_some());
    let title = walk.tab("tool_property").unwrap();
    let layer = walk.tab("layer").unwrap();
    let auto = walk.tab("auto_action").unwrap();
    walk.drag(title.center(), pos2(auto.right() - 4.0, layer.center().y));
    let tab = walk.tab("tool_property").expect("Tool Property");
    assert_eq!(
        tab.top(),
        walk.tab("layer").unwrap().top(),
        "a tab of the Layer group"
    );
}

/// B16: palettes that share a tab group keep together as icons when
/// their side folds.
#[test]
fn b16_grouped_palettes_fold_into_grouped_icons() {
    let mut walk = Walk::new("ws-b16");
    let fold = walk.chrome("fold-side-right").unwrap();
    walk.press(fold.center(), PointerButton::Primary);
    let group = |key: &str| {
        walk.paint
            .ctx
            .data(|data| data.get_temp::<usize>(egui::Id::unique(("sue-paint-strip-group", key))))
    };
    let navigator = group("navigator").expect("the Navigator icon's group");
    for key in ["sub_view", "item_bank", "info"] {
        assert_eq!(group(key), Some(navigator), "{key}");
    }
    assert_ne!(group("layer"), Some(navigator));
    assert_eq!(group("history"), group("layer"));
}

/// B17: a workspace of icons only: the Tool palette collapsed to an icon
/// by its menu, both sides folded; each icon pops its palette up.
#[test]
fn b17_both_sides_fold_into_icons_with_the_tool_palette() {
    let mut walk = Walk::new("ws-b17");
    walk.palette_menu("Tool");
    walk.click_text("Collapse to an Icon");
    for side in ["left", "right"] {
        let fold = walk.chrome(&format!("fold-side-{side}")).unwrap();
        walk.press(fold.center(), PointerButton::Primary);
    }
    assert!(
        walk.palettes_shown().is_empty(),
        "{:?}",
        walk.palettes_shown()
    );
    assert!(walk.node("Pen (P)").is_none(), "the tools fold too");
    let strips = Rect::from_min_max(pos2(0.0, 60.0), pos2(1440.0, 900.0));
    let icon = walk
        .all_named("Tool")
        .into_iter()
        .find(|rect| strips.contains(rect.center()) && (rect.left() < 40.0 || rect.left() > 1400.0))
        .expect("the Tool icon");
    walk.press(icon.center(), PointerButton::Primary);
    assert!(
        walk.node("Pen (P)").is_some(),
        "it pops up: {:?}",
        walk.nodes
    );
}

/// B18: Window, Sub Tool puts it back between the Tool palette and Tool
/// Property.
#[test]
fn b18_window_sub_tool_between_tool_and_tool_property() {
    let mut walk = Walk::new("ws-b18");
    walk.palette_menu("Sub Tool");
    walk.click_text("Hide Sub Tool Palette");
    assert!(walk.tab("sub_tool").is_none());
    walk.menu("Window", "Sub Tool");
    let sub = walk.tab("sub_tool").expect("Sub Tool is back");
    let property = walk.tab("tool_property").unwrap();
    assert!(sub.left() > walk.expect("Pen (P)").right());
    assert!(sub.top() < property.top());
}

/// B19: the tool, the sub tool group's tab, the sub tool and its
/// properties, all in view in one column.
#[test]
fn b19_tool_group_sub_tool_and_property_in_one_column() {
    let mut walk = Walk::new("ws-b19");
    walk.click("Sketch (P)");
    let first = walk.expect("Pencil");
    let second = walk
        .nodes
        .iter()
        .filter(|(_, rect, _)| {
            (rect.top() - first.top()).abs() < 1.0 && rect.left() > first.right() - 1.0
        })
        .map(|(name, rect, _)| (name.clone(), *rect))
        .min_by(|a, b| a.1.left().total_cmp(&b.1.left()))
        .expect("a second group tab");
    walk.press(second.1.center(), PointerButton::Primary);
    let tab = walk.expect(&second.0);
    let buttons = walk.expect("Duplicate Sub Tool").top();
    let row = walk
        .paint
        .texts
        .iter()
        .filter(|(_, rect)| {
            rect.top() > tab.bottom() && rect.bottom() < buttons && rect.left() > 200.0
        })
        .map(|(text, rect)| (text.clone(), *rect))
        .next_back()
        .expect("a sub tool in view");
    walk.press(row.1.center(), PointerButton::Primary);
    let property = walk.tab("tool_property").unwrap();
    let named = walk
        .paint
        .texts
        .iter()
        .any(|(text, rect)| *text == row.0 && rect.top() > property.bottom());
    assert!(named, "Tool Property shows {}", row.0);
    assert!(walk.node("Sketch (P)").is_some() && walk.node(&second.0).is_some());
}

/// B20: a double click on the color icon of the Tool palette opens Color
/// Settings with HSV, Lab, RGB, CMYK and hex.
#[test]
fn b20_double_click_the_color_icon_for_color_settings() {
    let mut walk = Walk::new("ws-b20");
    let main = walk
        .all_named("Main color")
        .into_iter()
        .find(|rect| rect.left() < 46.0)
        .expect("the color icon on the Tool palette");
    walk.double_click(main.center() - vec2(4.0, 4.0));
    assert!(walk.drawn("Color Settings"), "{:?}", walk.paint.texts);
    for name in [
        "H", "S", "V", "L", "a", "b", "R", "G", "B", "C", "M", "Y", "K", "Hex",
    ] {
        assert!(walk.node(name).is_some(), "{name}");
    }
}

/// B21: the Color Wheel docked at the right above the Layer palette.
#[test]
fn b21_the_color_wheel_docks_above_layer() {
    let mut walk = Walk::new("ws-b21");
    let layer = walk.tab("layer").unwrap();
    walk.tear(
        "color",
        pos2(layer.center().x + 60.0, layer.bottom() + 30.0),
    );
    let wheel = walk.tab("color").expect("the Color Wheel");
    let layer = walk.tab("layer").unwrap();
    assert!(wheel.left() > 1000.0, "at the right: {wheel:?}");
    assert!(
        wheel.top() < layer.top(),
        "above Layer: {wheel:?} {layer:?}"
    );
}

/// B22: Show Color Icon in the Tool palette's menu hides the color icons.
#[test]
fn b22_show_color_icon_off_hides_the_colors() {
    let mut walk = Walk::new("ws-b22");
    let on_tools = |walk: &Walk| {
        walk.all_named("Main color")
            .iter()
            .any(|rect| rect.left() < 46.0)
    };
    assert!(on_tools(&walk));
    walk.palette_menu("Tool");
    walk.click_text("Show Color Icon");
    assert!(!on_tools(&walk));
}

/// B23: the Color Slider, floating, shows HSV, RGB and CMYK, and goes back
/// as a tab beside the Color Wheel.
#[test]
fn b23_the_color_slider_modes_and_its_tab() {
    let mut walk = Walk::new("ws-b23");
    walk.tear("color_slider", pos2(720.0, 360.0));
    assert!(walk.float_button("color_slider", 2).is_some());
    walk.click("CMYK");
    for name in ["C", "M", "Y", "K"] {
        assert!(walk.node(name).is_some(), "{name}");
    }
    walk.click("RGB");
    assert!(walk.node("R").is_some());
    let title = walk.tab("color_slider").unwrap();
    let last = walk.tab("color_history").unwrap();
    walk.drag(title.center(), pos2(last.right() + 6.0, last.center().y));
    let tab = walk.tab("color_slider").unwrap();
    assert_eq!(
        tab.top(),
        walk.tab("color").unwrap().top(),
        "a tab beside the wheel"
    );
}

/// B24: the Sub View's menu: Switch to Eyedropper Automatically, Show
/// Command Bar, Show Rotate and Flip, and Hide Sub View Palette.
#[test]
fn b24_the_sub_view_menu() {
    let mut walk = Walk::new("ws-b24");
    let tab = walk.tab("sub_view").unwrap();
    walk.press(tab.center(), PointerButton::Primary);
    walk.palette_menu("Sub View");
    for item in [
        "Switch to Eyedropper Automatically",
        "Show Command Bar",
        "Show Rotate and Flip",
        "Hide Sub View Palette",
    ] {
        assert!(
            walk.paint.rect_of(item).is_some(),
            "{item}: {:?}",
            walk.paint.texts
        );
    }
    walk.click_text("Show Command Bar");
    assert!(
        walk.node("Import Picture").is_none(),
        "its buttons are hidden"
    );
}

/// B25: Add Drawing Color from the bar's menu puts the color on it.
#[test]
fn b25_add_drawing_color_to_the_bar() {
    let mut walk = Walk::new("ws-b25");
    walk.right_click("Redo");
    walk.click_text("Add Drawing Color");
    let names = walk.bar_names();
    assert!(
        names.iter().any(|name| name.starts_with("Drawing Color #")),
        "{names:?}"
    );
}

/// B26: Tab hides the palettes; the Command Bar with its colors stays.
#[test]
fn b26_tab_keeps_the_command_bar() {
    let mut walk = Walk::new("ws-b26");
    walk.right_click("Redo");
    walk.click_text("Add Drawing Color");
    walk.step(vec![key(egui::Key::Tab, Modifiers::NONE)]);
    walk.settle(6);
    assert!(walk.palettes_shown().is_empty());
    let names = walk.bar_names();
    assert!(names.iter().any(|name| name == "Undo"));
    assert!(names.iter().any(|name| name.starts_with("Drawing Color #")));
}

/// B27: one color dragged onto another makes a group.
#[test]
fn b27_a_color_dropped_on_another_makes_a_group() {
    let mut walk = Walk::new("ws-b27");
    walk.right_click("Redo");
    walk.click_text("Add Drawing Color");
    let first = walk
        .bar_names()
        .into_iter()
        .find(|name| name.starts_with("Drawing Color #"))
        .unwrap();
    walk.paint.command("color.swap");
    walk.settle(2);
    walk.right_click(&first);
    walk.click_text("Add Drawing Color");
    let colors: Vec<String> = walk
        .bar_names()
        .into_iter()
        .filter(|name| name.starts_with("Drawing Color #"))
        .collect();
    assert_eq!(colors.len(), 2, "{colors:?}");
    let area = walk.bar_area();
    let from = walk.node_in(&colors[1], area).unwrap().center();
    let to = walk.node_in(&colors[0], area).unwrap().center();
    walk.held = Modifiers::COMMAND;
    walk.drag(from, to);
    walk.held = Modifiers::NONE;
    let kept = walk.file("command_bar.txt");
    assert!(kept.contains("group\ncolor #"), "{kept}");
}

/// B28: Command Bar Settings, Pop-up Palettes, Color Wheel, Add: a button
/// that pops the palette up, gone again on a click elsewhere.
#[test]
fn b28_a_pop_up_palette_on_the_bar() {
    let mut walk = Walk::new("ws-b28");
    walk.menu("Window", "Color Wheel");
    assert!(walk.tab("color").is_none(), "the Color Wheel is closed");
    walk.right_click("Redo");
    walk.click_text("Command Bar Settings...");
    walk.add_from("Pop-up Palettes", "Color Wheel");
    walk.close_dialog();
    let area = walk.bar_area();
    let button = walk
        .node_in("Color Wheel", area)
        .expect("on the bar")
        .center();
    let before = walk.paint.texts.len();
    walk.press(button, PointerButton::Primary);
    assert!(walk.paint.texts.len() > before, "it pops up");
    walk.press(pos2(720.0, 500.0), PointerButton::Primary);
    assert_eq!(walk.paint.texts.len(), before, "and goes away");
}

/// B29: sub tools on the bar, dropped onto each other, make a group.
#[test]
fn b29_sub_tools_on_the_bar_make_a_group() {
    let mut walk = Walk::new("ws-b29");
    walk.right_click("Redo");
    walk.click_text("Command Bar Settings...");
    walk.add_from("Sub Tools", "G-pen");
    walk.add_from("Sub Tools", "Mapping pen");
    walk.close_dialog();
    let area = walk.bar_area();
    let from = walk.node_in("Mapping pen", area).unwrap().center();
    let to = walk.node_in("G-pen", area).unwrap().center();
    walk.held = Modifiers::COMMAND;
    walk.drag(from, to);
    walk.held = Modifiers::NONE;
    let kept = walk.file("command_bar.txt");
    assert!(kept.contains("group\nsub-tool pen "), "{kept}");
    // The group dropped onto another button moves; a group never goes
    // into a group.
    let group = walk.node_in("Group: G-pen", area).unwrap().center();
    let undo = walk.node_in("Undo", area).unwrap().center();
    walk.held = Modifiers::COMMAND;
    walk.drag(group, undo);
    walk.held = Modifiers::NONE;
    let kept = walk.file("command_bar.txt");
    assert!(!kept.contains("group\ngroup"), "{kept}");
    assert!(!kept.contains("group\ncommand edit.undo"), "{kept}");
    let names = walk.bar_names();
    let at = |name: &str| names.iter().position(|found| found == name).unwrap();
    assert_eq!(
        at("Group: G-pen").abs_diff(at("Undo")),
        1,
        "beside Undo: {names:?}"
    );
}

/// B30: the Information palette emptied by its menu, docked as a thin
/// column at the far right, a spacer.
#[test]
fn b30_information_as_a_spacer_column() {
    let mut walk = Walk::new("ws-b30");
    let tab = walk.tab("info").unwrap();
    walk.press(tab.center(), PointerButton::Primary);
    for item in [
        "Show Canvas Information",
        "Show Memory Use",
        "Show Coordinates",
        "Show Length",
    ] {
        walk.palette_menu("Information");
        walk.click_text(item);
    }
    let shown = walk.tab("info").unwrap();
    let body: Vec<&String> = walk
        .paint
        .texts
        .iter()
        .filter(|(_, rect)| {
            rect.top() > shown.bottom() + 2.0 && rect.top() < 300.0 && rect.left() > 1100.0
        })
        .map(|(text, _)| text)
        .collect();
    assert!(body.is_empty(), "the palette is empty: {body:?}");
    walk.drag(shown.center(), pos2(1402.0, 500.0));
    let tab = walk.tab("info").unwrap();
    assert!(
        tab.left() > walk.tab("layer").unwrap().right() - 2.0,
        "a column at the far right: {tab:?}"
    );
}

/// B31: the same spacer docked under the canvas.
#[test]
fn b31_the_spacer_docks_under_the_canvas() {
    let mut walk = Walk::new("ws-b31");
    let row = walk.expect("Timeline");
    walk.tear("info", row.center() + vec2(200.0, 0.0));
    // The row under the canvas holds its palettes as icons, as Clip
    // Studio Paint's Timeline starts; the spacer's icon opens the row.
    let below = Rect::from_min_max(pos2(0.0, row.top() - 4.0), pos2(1100.0, 900.0));
    let icon = walk
        .node_in("Information", below)
        .expect("its icon under the canvas");
    walk.press(icon.center(), PointerButton::Primary);
    // The row opens under the canvas, 250 points tall in the 900 point
    // window (p5-polish), so its tabs sit in the lowest third.
    let tab = walk.tab("info").expect("Information");
    assert!(
        tab.top() > 600.0 && tab.left() < 1100.0,
        "under the canvas: {tab:?}"
    );
}
