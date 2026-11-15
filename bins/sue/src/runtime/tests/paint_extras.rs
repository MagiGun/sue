//! Sue Paint: the palettes that keep work at hand, Navigator, Sub View,
//! Information, History, Item Bank, Auto Action and Material (package
//! P4-6 of the 0.0.8 plan, gaps WS-21, WS-22, CT-33, FG-6, FG-15, FG-28,
//! FG-31, FG-33, FGV-16, CTV-3, A25V-26 and step 115 of the beginners'
//! animation video). Controls are found by the name each frame tells
//! assistive technology and pressed where that frame drew them, never at
//! fixed places; what a change keeps is read back from the arena, the
//! history and Sue Paint's own files.

use super::*;
use egui::accesskit::{Role, Toggled};
use egui::{Modifiers, PointerButton, Rect};
use sue_rice::{Keybind, Keybinds};

/// Any role but a check box: a row of a list, whose box has its name
/// too.
const ROW: Option<Role> = None;

/// One control of a frame: its name, role, rect, whether it is on and
/// whether it can be used.
#[derive(Clone, Debug)]
struct Node {
    name: String,
    role: Role,
    rect: Rect,
    on: Option<bool>,
    enabled: bool,
    /// Its number, for a field or a slider.
    value: Option<f64>,
}

/// Sue Paint at 1440 by 900 with a canvas open, laid out as its layout
/// style does, driven a frame at a time.
struct Kept {
    driver: PaintDriver,
    nodes: Vec<Node>,
    /// The keys held.
    held: Modifiers,
}

impl Kept {
    fn new(tag: &str) -> Self {
        let driver = PaintDriver::fresh(tag);
        driver.ctx.enable_accesskit();
        let mut kept = Self {
            driver,
            nodes: Vec::new(),
            held: Modifiers::NONE,
        };
        kept.settle(4);
        kept
    }

    fn step(&mut self, events: Vec<egui::Event>) {
        let driver = &mut self.driver;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
            events: std::iter::once(egui::Event::ModifiersChanged(self.held))
                .chain(events)
                .collect(),
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
        self.nodes = output
            .platform_output
            .accesskit_update
            .iter()
            .flat_map(|update| update.nodes.iter())
            .filter_map(|(_, node)| {
                let bounds = node.bounds()?;
                Some(Node {
                    name: node.label()?.to_owned(),
                    role: node.role(),
                    rect: Rect::from_min_max(
                        Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
                        Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
                    ),
                    on: node.toggled().map(|toggled| toggled == Toggled::True),
                    enabled: !node.is_disabled(),
                    value: node.numeric_value(),
                })
            })
            .collect();
        output.drop_without_applying_deltas();
    }

    fn settle(&mut self, count: usize) {
        for _ in 0..count {
            self.step(Vec::new());
        }
    }

    fn paint_id() -> AppletId {
        AppletId::new(sue_paint::ID).unwrap()
    }

    /// Shows the palette whose key is `key` where the layout has it.
    fn show(&mut self, key: &str) {
        let applet = self
            .driver
            .runtime
            .applets
            .get_mut(&Self::paint_id())
            .unwrap();
        assert!(applet.navigate(&format!("extras/show/{key}")), "{key}");
        self.settle(6);
    }

    /// The picture open.
    fn picture(&self) -> sue_core::EntityId {
        self.driver
            .runtime
            .applets
            .get(&Self::paint_id())
            .and_then(|applet| applet.focus())
            .expect("a canvas is open")
    }

    fn layers(&self) -> usize {
        sue_paint::doc::layers(&self.driver.runtime.engine.world.arena, self.picture()).len()
    }

    /// The controls named `name` (with role `role`, any role for none),
    /// topmost first.
    fn find(&self, name: &str, role: Option<Role>) -> Vec<Node> {
        let mut found: Vec<Node> = self
            .nodes
            .iter()
            .filter(|node| node.name == name && role.is_none_or(|role| node.role == role))
            .cloned()
            .collect();
        found.sort_by(|a, b| a.rect.top().total_cmp(&b.rect.top()));
        found
    }

    fn node(&self, name: &str, role: Option<Role>) -> Node {
        self.find(name, role)
            .into_iter()
            .find(|node| role.is_some() || node.role != Role::CheckBox)
            .unwrap_or_else(|| {
                let names: Vec<&str> = self.nodes.iter().map(|node| node.name.as_str()).collect();
                panic!("{name:?} is drawn: {names:?}")
            })
    }

    fn press_at(&mut self, at: Pos2, modifiers: Modifiers) {
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers,
        };
        self.held = modifiers;
        self.step(vec![
            egui::Event::ModifiersChanged(modifiers),
            egui::Event::PointerMoved(at),
        ]);
        self.step(vec![press(true)]);
        self.step(vec![press(false)]);
        self.held = Modifiers::NONE;
        self.step(vec![egui::Event::ModifiersChanged(Modifiers::NONE)]);
        self.settle(4);
    }

    fn click(&mut self, name: &str, role: Option<Role>) {
        let at = self.node(name, role).rect.center();
        self.press_at(at, Modifiers::NONE);
    }

    fn double_click(&mut self, name: &str, role: Option<Role>) {
        let at = self.node(name, role).rect.center();
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.step(vec![egui::Event::PointerMoved(at)]);
        self.step(vec![press(true)]);
        self.step(vec![press(false)]);
        self.step(vec![press(true)]);
        self.step(vec![press(false)]);
        self.settle(4);
    }

    /// Rests the pointer on `name` until its tooltip shows; every text
    /// the frame draws.
    fn hover(&mut self, name: &str, role: Option<Role>) -> Vec<String> {
        let at = self.node(name, role).rect.center();
        self.step(vec![egui::Event::PointerMoved(at)]);
        self.settle(60);
        self.driver
            .texts
            .iter()
            .map(|(text, _)| text.clone())
            .collect()
    }

    /// Drags from `from` to `to` in 20 steps and lets go there.
    fn drag(&mut self, from: Pos2, to: Pos2) {
        self.driver.drag(from, to, PointerButton::Primary);
        self.settle(4);
    }

    fn key(&mut self, key: egui::Key, modifiers: Modifiers) {
        self.step(vec![
            egui::Event::ModifiersChanged(modifiers),
            self::key(key, modifiers),
        ]);
        self.settle(4);
    }

    fn undo(&mut self) {
        self.key(egui::Key::Z, Modifiers::COMMAND);
    }

    fn redo(&mut self) {
        self.key(egui::Key::Y, Modifiers::COMMAND);
    }

    fn data(&self, file: &str) -> String {
        std::fs::read_to_string(self.driver.root.join("data").join("paint").join(file))
            .unwrap_or_default()
    }

    /// Gives `command` the key `chord` among Sue Paint's keys.
    fn bind(&mut self, command: &str, chord: &str) {
        let rice = &self.driver.runtime.engine.world.rice;
        let mut chords = Keybinds::default();
        chords.push(chord.parse::<Keybind>().unwrap());
        let (setting, value) = sue_wm::shortcuts::bind(rice, "sue_paint_keys", command, &chords);
        self.driver
            .runtime
            .engine
            .world
            .rice
            .set(&setting, value)
            .unwrap();
        self.settle(2);
    }

    fn texts(&self) -> Vec<String> {
        self.driver
            .texts
            .iter()
            .map(|(text, _)| text.clone())
            .collect()
    }

    fn text_with(&self, part: &str) -> Option<String> {
        self.texts().into_iter().find(|text| text.contains(part))
    }

    /// The middle of the canvas.
    fn canvas(&self) -> Pos2 {
        paint_middle(&self.driver.runtime)
    }
}

#[test]
fn the_navigator_buttons_run_the_view_commands_and_name_their_keys() {
    let mut kept = Kept::new("extras-navigator");
    kept.show("navigator");
    // Each button tells its name and the key that runs it.
    let texts = kept.hover("Zoom In", Some(Role::Button));
    assert!(
        texts.iter().any(|text| text.contains("Ctrl")),
        "the tooltip names the key: {texts:?}"
    );
    let zoom = |kept: &Kept| {
        kept.find("Zoom", None)
            .iter()
            .find_map(|node| node.value)
            .expect("the Navigator's zoom")
    };
    let before = zoom(&kept);
    kept.click("Zoom In", Some(Role::Button));
    kept.settle(6);
    assert!(zoom(&kept) > before, "Zoom In zoomed the view");
    // Flip Horizontal is a switch that the button shows on.
    kept.click("Flip Horizontal", Some(Role::Button));
    kept.settle(4);
    assert_eq!(
        kept.node("Flip Horizontal", Some(Role::Button)).on,
        Some(true),
        "the flip is on"
    );
}

#[test]
fn the_sub_view_flips_and_its_menu_hides_its_rows() {
    // WS-21 and FG-33: a reference picture, flipped, with Switch to
    // Eyedropper Automatically, Show Command Bar and Show Rotate and Flip
    // as switches.
    let mut kept = Kept::new("extras-sub-view");
    let picture = kept.driver.root.join("reference.png");
    let mut pixels = vec![0_u8; 4 * 4 * 4];
    for (index, pixel) in pixels.chunks_mut(4).enumerate() {
        // The left half red, the right half blue.
        let rgb = if index % 4 < 2 {
            [255, 0, 0]
        } else {
            [0, 0, 255]
        };
        pixel.copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
    std::fs::create_dir_all(&kept.driver.root).unwrap();
    std::fs::write(&picture, sue_io::encode_png_rgba(4, 4, &pixels).unwrap()).unwrap();
    kept.show("sub_view");
    kept.click("Import Picture", Some(Role::Button));
    let pick = kept
        .driver
        .runtime
        .pending_pick
        .take()
        .expect("asks for a file");
    assert_eq!(pick.purpose, "subview");
    kept.driver.runtime.request(
        ShellRequest::Picked {
            pick,
            path: picture,
        },
        &mut NoTextures,
        kept.driver.now,
    );
    kept.settle(6);
    assert!(kept.find("Remove Picture", Some(Role::Button))[0].enabled);
    // Flipped, the eyedropper picks what shows: blue at the left.
    kept.click("Flip Horizontally", Some(Role::Button));
    assert_eq!(
        kept.node("Flip Horizontally", Some(Role::Button)).on,
        Some(true)
    );
    // The picture is square in a wider area, so its left half lies left
    // of the middle by up to half the area's height.
    let shown = kept.node("Reference Picture", None).rect;
    let left = shown.center() - vec2(shown.height() * 0.25, 0.0);
    kept.press_at(left, Modifiers::NONE);
    // A new layer filled with the drawing color shows what was picked.
    kept.driver.command("layer.new");
    kept.driver.command("edit.fill");
    kept.settle(4);
    let arena = &kept.driver.runtime.engine.world.arena;
    let filled: Vec<[u8; 4]> = sue_paint::doc::layers(arena, kept.picture())
        .iter()
        .filter_map(|layer| sue_paint::doc::raster(arena, layer.id))
        .filter_map(|raster| {
            raster
                .bytes()
                .map(|bytes| [bytes[0], bytes[1], bytes[2], bytes[3]])
        })
        .collect();
    assert!(
        filled.contains(&[0, 0, 255, 255]),
        "the flipped picture shows blue at its left: {filled:?}"
    );
    assert!(!filled.contains(&[255, 0, 0, 255]), "not the red under it");
    // The menu's switches: hiding the buttons takes them away.
    kept.driver.command("subview.show_bar");
    kept.settle(4);
    assert!(kept.find("Import Picture", Some(Role::Button)).is_empty());
    kept.driver.command("subview.show_turn");
    kept.settle(4);
    assert!(kept.find("Zoom the reference", None).is_empty());
    kept.driver.command("subview.show_bar");
    kept.driver.command("subview.show_turn");
    kept.driver.command("subview.eyedropper");
    kept.settle(4);
    assert_eq!(
        kept.node("Switch to Eyedropper Automatically", Some(Role::Button))
            .on,
        Some(false),
        "the eyedropper switch is off"
    );
}

#[test]
fn information_shows_memory_length_and_the_unit_chosen() {
    // WS-22.
    let mut kept = Kept::new("extras-information");
    kept.show("info");
    assert!(
        kept.text_with("MB of pixels").is_some(),
        "{:?}",
        kept.texts()
    );
    kept.driver.command("info.unit_mm");
    let middle = kept.canvas();
    kept.step(vec![egui::Event::PointerMoved(middle)]);
    kept.settle(10);
    let pointer = kept.text_with("Pointer").unwrap_or_default();
    assert!(pointer.contains("mm"), "{pointer}");
    // A drag on the canvas tells its length while it lasts.
    let press = |pressed, pos| egui::Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    kept.step(vec![press(true, middle)]);
    let to = middle + vec2(60.0, 0.0);
    for index in 1..=10 {
        kept.step(vec![egui::Event::PointerMoved(
            middle + vec2(6.0 * index as f32, 0.0),
        )]);
    }
    kept.settle(8);
    let length = kept.text_with("Length").unwrap_or_default();
    assert!(length.contains("mm"), "{length} {:?}", kept.texts());
    kept.step(vec![press(false, to)]);
    kept.settle(4);
    // Memory off: the line goes, and the choice is kept.
    kept.driver.command("info.memory");
    kept.settle(6);
    assert!(kept.text_with("MB of pixels").is_none());
    let file = kept.data("information.txt");
    assert!(
        file.contains("memory 0") && file.contains("unit mm"),
        "{file}"
    );
}

#[test]
fn auto_actions_come_in_sets_with_switches_and_play_with_a_key() {
    // FG-15, A25V-26: nine actions of Sue Paint's own; FGV-16, CTV-3: a
    // set picker, a box per action and per step, steps that unfold, four
    // buttons; and a key for an action.
    let mut kept = Kept::new("extras-auto-sets");
    kept.show("auto_action");
    for name in [
        "Flip Canvas and Fit",
        "Rotate Canvas Right",
        "Reset the View",
        "New Clipped Layer",
        "New Draft Layer",
        "Gray Copy of the Layer",
        "Fill and Deselect",
    ] {
        assert!(!kept.find(name, Some(Role::CheckBox)).is_empty(), "{name}");
    }
    for button in ["Record", "Play", "New Auto Action", "Delete"] {
        assert!(
            !kept.find(button, Some(Role::Button)).is_empty(),
            "{button}"
        );
    }
    // Played by a double click: New Clipped Layer adds one layer, one
    // undo step per command.
    let layers = kept.layers();
    kept.double_click("New Clipped Layer", ROW);
    assert_eq!(kept.layers(), layers + 1);
    kept.undo();
    kept.undo();
    assert_eq!(kept.layers(), layers, "undone");
    kept.redo();
    assert_eq!(kept.layers(), layers + 1, "redone");
    kept.undo();
    // Its steps unfold, each with a box; a step turned off is skipped.
    let row = kept.node("New Clipped Layer", ROW).rect;
    kept.press_at(
        Pos2::new(row.left() + 10.0, row.center().y),
        Modifiers::NONE,
    );
    let steps = kept.find("Raster Layer", Some(Role::CheckBox));
    assert!(!steps.is_empty(), "the steps unfold: {:?}", kept.nodes);
    kept.press_at(steps[0].rect.center(), Modifiers::NONE);
    assert!(kept.data("auto_actions.txt").contains("step off layer.new"));
    let before = kept.layers();
    kept.double_click("New Clipped Layer", ROW);
    assert_eq!(kept.layers(), before, "Raster Layer was skipped");
    // An action turned off cannot be played.
    kept.click("Flip Canvas and Fit", Some(Role::CheckBox));
    kept.click("Flip Canvas and Fit", ROW);
    assert!(!kept.node("Play", Some(Role::Button)).enabled);
    // A key for Reset the View plays it: the turned view comes back
    // upright.
    kept.show("navigator");
    kept.bind("auto.run.reset-the-view", "F9");
    kept.driver.command("view.rotate_90");
    kept.settle(6);
    let rotation = |kept: &Kept| {
        kept.find("Rotation", None)
            .iter()
            .find_map(|node| node.value)
            .expect("the Navigator's rotation")
    };
    assert!((rotation(&kept).abs() - 90.0).abs() < 0.5, "turned");
    kept.key(egui::Key::F9, Modifiers::NONE);
    kept.settle(6);
    assert!(rotation(&kept).abs() < 0.5, "upright again");
    // A new set is in the picker; Sue Paint's set is still there.
    kept.driver.command("auto.new_set");
    kept.settle(4);
    let file = kept.data("auto_actions.txt");
    assert!(file.starts_with("sue-auto-actions 1"), "{file}");
    assert!(file.contains("set Sue Paint Actions") && file.contains("set Set 2"));
}

#[test]
fn auto_action_sets_are_shared_as_files() {
    // Owner note 12: a set is a file people can share. The set in use is
    // exported, and importing the file brings it back as a set of its
    // own, in use, with every action and step it had.
    let mut kept = Kept::new("extras-auto-share");
    kept.show("auto_action");
    kept.click("New Clipped Layer", Some(Role::CheckBox));
    let shared = kept.driver.root.join("mine.sueactions");
    let answer = |kept: &mut Kept, command: &str, purpose: &str| {
        kept.driver.command(command);
        kept.settle(2);
        let pick = kept
            .driver
            .runtime
            .pending_pick
            .take()
            .expect("asks for a file");
        assert_eq!(pick.purpose, purpose);
        kept.driver.runtime.request(
            ShellRequest::Picked {
                pick,
                path: shared.clone(),
            },
            &mut NoTextures,
            kept.driver.now,
        );
        kept.settle(4);
    };
    answer(&mut kept, "auto.export_set", "auto-export");
    let text = std::fs::read_to_string(&shared).expect("the set was written");
    assert!(text.starts_with("sue-auto-actions 1"), "{text}");
    assert!(text.contains("[New Clipped Layer]\nid new-clipped-layer\noff\n"));
    answer(&mut kept, "auto.import_set", "auto-import");
    let file = kept.data("auto_actions.txt");
    assert_eq!(
        file.matches("set Sue Paint Actions").count(),
        2,
        "the shared set is read in beside the one it came from: {file}"
    );
    // Its ids were told apart, so a key still plays one action only.
    assert!(file.contains("id new-clipped-layer-2"), "{file}");
    // The imported set is the one in use, its action still off.
    assert!(file.contains("set Sue Paint Actions\nin-use\n"), "{file}");
    assert_eq!(
        kept.node("New Clipped Layer", Some(Role::CheckBox)).on,
        Some(false)
    );
}

#[test]
fn recording_keeps_commands_from_menus_keys_and_filters() {
    // CT-33 and step 115: Merge Selected Layers, a filter and the Select
    // Next Cel key, recorded; Record turns into Stop while it records.
    let mut kept = Kept::new("extras-auto-record");
    kept.show("auto_action");
    kept.click("New Auto Action", Some(Role::Button));
    assert!(
        !kept.find("Stop", Some(Role::Button)).is_empty(),
        "Record became Stop"
    );
    kept.bind("anim.cel.next", "F8");
    kept.driver.command("layer.merge_selected");
    kept.driver.command("filter.grayscale");
    kept.settle(4);
    kept.key(egui::Key::F8, Modifiers::NONE);
    // Not recorded: a Window command.
    kept.driver.command("window.info");
    kept.settle(4);
    kept.show("auto_action");
    kept.click("Stop", Some(Role::Button));
    assert!(!kept.find("Record", Some(Role::Button)).is_empty());
    let file = kept.data("auto_actions.txt");
    let recorded: Vec<&str> = file
        .lines()
        .skip_while(|line| *line != "[Auto Action 10]")
        .filter_map(|line| line.strip_prefix("step "))
        .collect();
    assert_eq!(
        recorded,
        ["layer.merge_selected", "filter.grayscale", "anim.cel.next"],
        "{file}"
    );
}

#[test]
fn item_bank_pictures_drag_onto_the_canvas_as_one_undo_step() {
    // FG-31: a picture put in the bank's folder shows in the bank; one
    // dragged onto the canvas lands on a new layer, one undo step. Keep
    // Current Layer as Picture is grayed while the layer is empty.
    let mut kept = Kept::new("extras-item-bank");
    let folder = kept
        .driver
        .root
        .join("data")
        .join("paint")
        .join("item_bank");
    std::fs::create_dir_all(&folder).unwrap();
    let pixels = vec![200_u8; 8 * 8 * 4];
    std::fs::write(
        folder.join("Star.png"),
        sue_io::encode_png_rgba(8, 8, &pixels).unwrap(),
    )
    .unwrap();
    kept.driver.command("item_bank.keep_sub_tool");
    kept.driver.command("item_bank.keep_layer");
    kept.settle(2);
    kept.show("item_bank");
    assert!(
        !kept
            .node("Keep Current Layer as Picture", Some(Role::Button))
            .enabled,
        "nothing drawn on the layer yet"
    );
    assert!(
        kept.data("item_bank.txt").lines().count() >= 1,
        "the sub tool is kept"
    );
    let star = kept.node("Star", Some(Role::Button));
    let layers = kept.layers();
    let depth = kept.driver.runtime.engine.history.depth();
    let to = kept.canvas();
    kept.drag(star.rect.center(), to);
    assert_eq!(kept.layers(), layers + 1, "dropped on a new layer");
    assert_eq!(kept.driver.runtime.engine.history.depth(), depth + 1);
    kept.undo();
    assert_eq!(kept.layers(), layers);
    kept.redo();
    assert_eq!(kept.layers(), layers + 1);
}

#[test]
fn materials_are_searched_tagged_and_dragged_onto_the_canvas() {
    // FG-28, FG-6 (dragging onto the canvas) and WS-25.
    let mut kept = Kept::new("extras-material");
    kept.show("material");
    let all: Vec<Node> = kept
        .nodes
        .iter()
        .filter(|node| {
            // The thumbnails: square buttons larger than icon buttons.
            node.role == Role::Button
                && node.on.is_some()
                && node.rect.width() > 40.0
                && (node.rect.width() - node.rect.height()).abs() < 2.0
        })
        .cloned()
        .collect();
    assert!(
        all.len() > 4,
        "Sue Paint's own materials show: {:?}",
        kept.nodes
    );
    // Several picked with Ctrl, then one alone.
    kept.press_at(all[0].rect.center(), Modifiers::NONE);
    kept.press_at(all[1].rect.center(), Modifiers::COMMAND);
    let picked = kept
        .nodes
        .iter()
        .filter(|node| {
            node.role == Role::Button
                && node.on == Some(true)
                && all.iter().any(|thumb| thumb.name == node.name)
        })
        .count();
    assert_eq!(picked, 2, "two picked");
    // Favorite: the first picked becomes one, and the Favorites folder
    // holds it.
    kept.driver.command("material.favorite");
    kept.settle(4);
    assert_eq!(kept.node("Favorite", Some(Role::Button)).on, Some(true));
    // The column is too narrow for the tree, so its folders are a
    // picker: Favorites holds the two just made favorites.
    let thumbs = |kept: &Kept| -> Vec<String> {
        kept.nodes
            .iter()
            .filter(|node| node.role == Role::Button && all.iter().any(|t| t.name == node.name))
            .map(|node| node.name.clone())
            .collect()
    };
    kept.click("Folder", None);
    kept.click("Favorites", None);
    let mut favorites = thumbs(&kept);
    favorites.sort();
    let mut expected = vec![all[0].name.clone(), all[1].name.clone()];
    expected.sort();
    assert_eq!(favorites, expected, "Favorites shows the favorites");
    kept.click("Folder", None);
    kept.click("All", None);
    assert_eq!(thumbs(&kept).len(), all.len(), "All shows every one again");
    // Tagged: the tag is the library's, and the search finds it.
    kept.press_at(all[3].rect.center(), Modifiers::NONE);
    kept.click("Tags", None);
    kept.step(vec![egui::Event::Text("sky, night".to_owned())]);
    kept.key(egui::Key::Enter, Modifiers::NONE);
    kept.settle(4);
    kept.click("Search names and tags", None);
    kept.step(vec![egui::Event::Text("night".to_owned())]);
    kept.settle(4);
    let found: Vec<String> = kept
        .nodes
        .iter()
        .filter(|node| {
            all.iter()
                .any(|thumb| thumb.name == node.name && thumb.rect.width() > 40.0)
        })
        .filter(|node| node.role == Role::Button)
        .map(|node| node.name.clone())
        .collect();
    assert_eq!(found, [all[3].name.clone()], "the search finds the tag");
    kept.key(egui::Key::A, Modifiers::COMMAND);
    kept.key(egui::Key::Backspace, Modifiers::NONE);
    kept.key(egui::Key::Escape, Modifiers::NONE);
    kept.settle(4);
    // Dragged onto the canvas: one new layer, one undo step.
    let layers = kept.layers();
    let to = kept.canvas();
    let first = kept
        .nodes
        .iter()
        .find(|node| node.name == all[2].name && node.role == Role::Button)
        .cloned()
        .unwrap();
    kept.drag(first.rect.center(), to);
    assert_eq!(kept.layers(), layers + 1, "dropped on the canvas");
    kept.undo();
    assert_eq!(kept.layers(), layers);
}
