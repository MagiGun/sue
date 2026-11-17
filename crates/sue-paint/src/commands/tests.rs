//! The registry against the menus, the shortcuts, the palette and the
//! names 0.0.75 saved.

use std::collections::BTreeSet;

use sue_wm::{Flags, MenuItem};

use super::{SHELL_OWNED, registry, shortcut_table};

/// Every command the menus name, submenus included, with the items Open
/// Recent and Workspace add.
fn menu_commands() -> Vec<String> {
    fn walk(items: &[MenuItem], out: &mut Vec<String>) {
        for item in items {
            match item {
                MenuItem::Action { command, .. } => out.push(command.clone()),
                MenuItem::Sub { items, .. } => walk(items, out),
                _ => {}
            }
        }
    }
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut menus = crate::menus::menus();
    crate::menus::set_workspaces(&mut menus, &["Inking".to_owned()]);
    crate::menus::set_recent(
        &mut menus,
        crate::menus::recent_items(&["picture.sue".into()]),
    );
    crate::menus::set_canvases(&mut menus, &["Illustration".to_owned()]);
    let mut out = Vec::new();
    for menu in &menus {
        walk(&menu.items, &mut out);
    }
    out
}

#[test]
fn the_tables_have_no_problems() {
    assert_eq!(registry().problems(), Vec::<String>::new());
}

#[test]
fn every_menu_command_is_registered() {
    let missing: Vec<String> = menu_commands()
        .into_iter()
        .filter(|command| !registry().has(command))
        .collect();
    assert!(missing.is_empty(), "grayed menu items: {missing:?}");
    // The families answer for their members.
    for name in [
        "file.recent.open.0",
        "window.workspace.use.0",
        "figure.preset.2",
    ] {
        assert!(registry().has(name), "{name}");
    }
    assert!(!registry().has("file.recent.open."), "a family needs more");
}

/// Commands that palettes run, not a menu: the timeline's camera keys, the
/// Animation Cels and light table palettes, and the 3D drawing figure's
/// tool property, and Quick Access's palette menu. Named one by one, so a command no one can reach cannot
/// hide behind a prefix.
const IN_PALETTES: [&str; 84] = [
    "anim.camera.key",
    "anim.camera.unkey",
    "anim.light_table.reverse",
    "anim.light_table.color",
    "anim.light_table.cel",
    "anim.light_table.general",
    "anim.light_table.opacity_target",
    "anim.light_table.tool",
    "anim.light_table.reset",
    "anim.light_table.file",
    "anim.light_table.register_general",
    "anim.light_table.deregister",
    "anim.cel.lock",
    "figure.register_pose",
    "figure.pose_scanner",
    "figure.hand_scanner",
    "window.quick_access.new_set",
    "window.quick_access.delete_set",
    "window.quick_access.settings",
    "subview.import",
    "subview.previous",
    "subview.next",
    "subview.list",
    "subview.eyedropper",
    "subview.flip_h",
    "subview.flip_v",
    "subview.fit",
    "subview.remove",
    "subview.show_bar",
    "subview.show_turn",
    "info.canvas",
    "info.memory",
    "info.coordinates",
    "info.length",
    "info.unit_px",
    "info.unit_mm",
    "info.unit_cm",
    "info.unit_in",
    "item_bank.keep_sub_tool",
    "item_bank.add_picture",
    "item_bank.keep_layer",
    "auto.record",
    "auto.play",
    "auto.new",
    "auto.delete",
    "auto.rename",
    "auto.duplicate",
    "auto.new_set",
    "auto.rename_set",
    "auto.delete_set",
    "auto.import_set",
    "auto.export_set",
    "auto.restore_defaults",
    "material.paste",
    "material.favorite",
    "material.delete",
    "material.size_small",
    "material.size_large",
    "material.size_list",
    "tool_palette.settings",
    "tool_palette.delete",
    "tool_palette.restore",
    "tool_palette.reset",
    "tool_palette.import",
    "tool_palette.export",
    "tool_palette.color_icon",
    "sub_tool.duplicate",
    "sub_tool.rename",
    "sub_tool.delete",
    "sub_tool.reset",
    "sub_tool.new_group",
    "sub_tool.rename_group",
    "sub_tool.import",
    "sub_tool.detail",
    "sub_tool.lock",
    "sub_tool.categories",
    "sub_tool.save_default",
    "sub_tool.register_shape",
    "figure.object.ground",
    "figure.object.reset_position",
    "figure.object.reset_angle",
    "figure.object.reset_scale",
    "figure.body.initialize",
    "figure.body.register",
];

/// The palettes' code, which runs those commands by name: every source
/// file under `panels/`, read from the folder so a palette file added or
/// split later is never left out.
fn palettes_source() -> Vec<String> {
    fn walk(dir: &std::path::Path, out: &mut Vec<String>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .expect("the panels folder")
            .map(|entry| entry.expect("a panels entry").path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(std::fs::read_to_string(&path).expect("a panels file"));
            }
        }
    }
    let mut out = Vec::new();
    walk(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/panels"),
        &mut out,
    );
    assert!(out.len() > 20, "the panels folder has its palettes");
    out
}

#[test]
fn every_command_can_be_reached() {
    // In a menu, in the palette, on a default key, on a palette's button,
    // or deliberately hidden. Those reached only from a palette can still
    // be given a key in Settings.
    let menus = menu_commands();
    let unreached: Vec<&str> = registry()
        .iter()
        .filter(|command| {
            let name = command.name;
            let in_menu = if command.is_family() {
                menus.iter().any(|item| item.starts_with(name))
            } else {
                menus.iter().any(|item| item == name)
            };
            !(in_menu
                || command.flags.has(Flags::PALETTE)
                || command.flags.has(Flags::HIDDEN)
                || !command.chords.is_empty()
                || IN_PALETTES.contains(&name))
        })
        .map(|command| command.name)
        .collect();
    assert!(
        unreached.is_empty(),
        "in no menu, palette or key: {unreached:?}"
    );
    // The list stays true: each of its names is registered, in no menu,
    // and a palette's button runs it.
    let palettes = palettes_source();
    for name in IN_PALETTES {
        assert!(registry().has(name), "{name} is not registered");
        assert!(
            !menus.iter().any(|item| item == name),
            "{name} is in a menu"
        );
        assert!(
            palettes
                .iter()
                .any(|source| source.contains(&format!("\"{name}\""))),
            "no palette runs {name}"
        );
    }
    // The names no menu, palette or key reaches are few, and each is
    // there on purpose (Auto Actions and scripts run them).
    let hidden: Vec<&str> = registry()
        .iter()
        .filter(|command| command.flags.has(Flags::HIDDEN))
        .map(|command| command.name)
        .collect();
    assert_eq!(
        hidden,
        [
            "edit.transform.cancel",
            "edit.material.paste",
            "figure.flip",
            "figure.reset",
            "figure.preset.",
            "figure.object.angle.",
            "auto.run.",
            "subtool.",
            "auto.stroke.",
            "layer.move_by.",
        ]
    );
}

#[test]
fn every_command_but_the_shells_can_be_given_a_key() {
    let rows: BTreeSet<&str> = shortcut_table().iter().map(|row| row.command).collect();
    for command in registry().iter() {
        let listed = !command.is_family() && !command.flags.has(Flags::HIDDEN);
        assert_eq!(
            rows.contains(command.name),
            listed && !SHELL_OWNED.contains(&command.name),
            "{}",
            command.name
        );
    }
    for name in SHELL_OWNED {
        assert!(registry().has(name), "{name}");
    }
    // One setting per command that ships with a key, and the one that
    // holds the keys people give the others.
    let declarations = sue_wm::shortcuts::declarations(super::KEYS_SECTION, shortcut_table());
    let keys: BTreeSet<&str> = declarations
        .iter()
        .map(|declaration| declaration.path.as_str())
        .collect();
    assert_eq!(keys.len(), declarations.len(), "one setting per command");
    let with_chords = shortcut_table()
        .iter()
        .filter(|row| !row.chords.is_empty())
        .count();
    assert_eq!(declarations.len(), with_chords + 1);
}

#[test]
fn the_shortcuts_keep_the_keys_and_names_they_had() {
    // The default chords of 0.0.75, as settings write them, each command
    // once with every chord it has, and labeled as Settings named it.
    // Since 0.0.8 Ctrl+Alt+Shift+K opens Shortcut Settings and
    // Ctrl+Alt+Shift+Y Modifier Key Settings (Clip Studio Paint's keys),
    // and Shift+Delete deletes outside the selection instead of filling.
    let expected: &[(&str, &str, &str)] = &[
        ("help.shortcuts", "F1 or Alt+I", "Keyboard Shortcuts"),
        ("file.preferences", "Ctrl+K", "Preferences"),
        ("file.shortcuts", "Ctrl+Alt+Shift+K", "Shortcut Settings"),
        (
            "file.modifiers",
            "Ctrl+Alt+Shift+Y",
            "Modifier Key Settings",
        ),
        (
            "file.save_as",
            "Ctrl+Shift+S or Ctrl+Alt+S or Alt+Shift+S",
            "Save As",
        ),
        ("file.print", "Ctrl+P", "Print"),
        ("layer.new", "Ctrl+Shift+N", "Raster Layer"),
        ("file.new", "Ctrl+N", "New"),
        ("file.close", "Ctrl+W", "Close"),
        ("edit.transform.free", "Ctrl+Shift+T", "Free Transform"),
        ("edit.transform.scale", "Ctrl+T", "Scale/Rotate"),
        ("filter.hue", "Ctrl+U", "Hue/Saturation/Luminosity"),
        (
            "select.invert",
            "Ctrl+Shift+I or Shift+F7",
            "Invert Selected Area",
        ),
        ("filter.invert", "Ctrl+I", "Reverse Gradient"),
        ("edit.cut", "Ctrl+X or F2", "Cut"),
        ("edit.copy", "Ctrl+C or F3", "Copy"),
        ("edit.paste", "Ctrl+V or F4", "Paste"),
        ("edit.paste_here", "Ctrl+Shift+V", "Paste to Shown Position"),
        (
            "edit.clear",
            "Ctrl+Delete or Ctrl+Backspace or Delete or Backspace",
            "Delete",
        ),
        ("edit.fill", "Alt+Delete or Alt+Backspace", "Fill"),
        (
            "edit.clear_outside",
            "Shift+Delete or Shift+Backspace",
            "Delete Outside Selection",
        ),
        ("layer.toggle.clip", "Ctrl+Alt+G", "Clip to Layer Below"),
        (
            "layer.folder_insert",
            "Ctrl+G",
            "Create Folder and Insert Layer",
        ),
        (
            "layer.merge_visible_here",
            "Ctrl+Shift+E",
            "Merge Visible Layers",
        ),
        ("layer.ungroup", "Ctrl+Shift+G", "Ungroup Layer Folder"),
        (
            "layer.merge_selected",
            "Alt+Shift+E",
            "Merge Selected Layers",
        ),
        ("layer.merge_down", "Ctrl+E", "Merge with Layer Below"),
        ("layer.select_above", "Alt+]", "Select Layer Above"),
        ("layer.select_below", "Alt+[", "Select Layer Below"),
        ("layer.order.up", "Ctrl+]", "Move Up"),
        ("layer.order.down", "Ctrl+[", "Move Down"),
        (
            "layer.toggle.layer_color",
            "Ctrl+B",
            "Switch Using Layer Color",
        ),
        ("select.again", "Ctrl+Shift+D", "Reselect"),
        ("select.all", "Ctrl+A", "Select All"),
        ("select.none", "Ctrl+D", "Deselect"),
        ("view.actual", "Ctrl+Alt+0", "100%"),
        ("view.fit", "Ctrl+0", "Fit to Screen"),
        ("view.rotate_reset", "Ctrl+Shift+2", "Reset Rotation"),
        ("view.zoom_in", "Ctrl++ or Ctrl+= or Ctrl+;", "Zoom In"),
        ("view.zoom_out", "Ctrl+-", "Zoom Out"),
        ("view.ruler_bar", "Ctrl+R", "Ruler Bar"),
        ("view.snap_ruler", "Ctrl+1", "Snap to Ruler"),
        ("view.snap_special", "Ctrl+2", "Snap to Special Ruler"),
        ("view.snap_grid", "Ctrl+3", "Snap to Grid"),
        (
            "view.special_ruler_snapping",
            "Ctrl+4",
            "Toggle Special Ruler Snapping",
        ),
        ("view.rotate_left", "-", "Rotate Left"),
        ("view.rotate_right", "=", "Rotate Right"),
        ("window.prev_tab", "Ctrl+Shift+Tab", "Previous Canvas"),
        ("window.next_tab", "Ctrl+Tab", "Next Canvas"),
        ("window.hide_menus", "Shift+Tab", "Hide Menu Bar"),
        ("window.hide_all", "Tab", "Hide All Palettes"),
        ("tool.zoom", "/", "Zoom Tool"),
        ("tool.hand", "H", "Hand Tool"),
        ("tool.rotate", "R", "Rotate Tool"),
        ("tool.operation", "O", "Operation Tool"),
        ("tool.move_layer", "K", "Move Layer Tool"),
        ("tool.selection", "M", "Selection Tool"),
        ("tool.auto_select", "W", "Auto Select Tool"),
        ("tool.eyedropper", "I", "Eyedropper Tool"),
        ("tool.pen", "P", "Pen and Pencil Tools"),
        ("tool.brush", "B", "Brush, Airbrush and Decoration Tools"),
        ("tool.eraser", "E", "Eraser Tool"),
        ("tool.blend", "J", "Blend Tool"),
        ("tool.fill", "G", "Fill and Gradient Tools"),
        ("tool.figure", "U", "Figure, Frame Border and Ruler Tools"),
        ("tool.text", "T", "Text and Balloon Tools"),
        ("tool.correct_line", "Y", "Correct Line Tool"),
        ("tool.previous_sub", ",", "Previous Sub Tool"),
        ("tool.next_sub", ".", "Next Sub Tool"),
        ("color.swap", "X", "Switch Between Main and Sub Color"),
        ("color.transparent", "C", "Drawing or Transparent Color"),
        ("brush.smaller", "[", "Smaller Brush"),
        ("brush.bigger", "]", "Bigger Brush"),
    ];
    let found: Vec<(&str, String, &str)> = shortcut_table()
        .iter()
        .filter(|row| !row.chords.is_empty())
        .map(|row| (row.command, row.chords.to_string(), row.label.as_str()))
        .collect();
    assert_eq!(found.len(), expected.len(), "{found:?}");
    for (command, chords, label) in expected {
        assert!(
            found
                .iter()
                .any(|(name, keys, named)| name == command && keys == chords && named == label),
            "{command}: {chords} {label} is not in {found:?}"
        );
    }
}

#[test]
fn the_palette_offers_what_it_offered() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let palette = registry().palette(&sue_rice::RiceSchema::new());
    let names: Vec<&str> = palette.iter().map(|entry| entry.command.as_str()).collect();
    assert_eq!(
        names,
        [
            "file.start_page",
            "file.new",
            "file.open",
            "file.export.png",
            "file.fonts",
            "file.import.pose_scanner",
            "file.batch_convert",
            "story.webtoon.right_to_left",
            "story.webtoon.left_to_right",
            "anim.cel.rename_timeline",
            "anim.cel.rename_layer",
            // The Timeline palette's own commands (its menu and right
            // click), offered here so they can be found and given keys.
            "anim.timeline.settings",
            "anim.zoom_in",
            "anim.zoom_out",
            "anim.edit.cut",
            "anim.edit.copy",
            "anim.edit.paste",
            "anim.edit.delete",
            "anim.clip.split",
            "anim.clip.merge",
            "anim.clip.first",
            "anim.clip.last",
            "anim.label",
            "anim.track_note",
            "anim.track_label.inbetween",
            "anim.track_label.inbetween-done",
            "anim.track_label.reverse-sheet",
            "anim.track_label.none",
            "anim.play_range.start",
            "anim.play_range.end",
            "anim.play_range.clear",
            "anim.thumbnails.none",
            "anim.thumbnails.smallest",
            "anim.thumbnails.small",
            "anim.thumbnails.medium",
            "anim.thumbnails.large",
            "anim.thumbnails.largest",
            "anim.time_unit.frames",
            "anim.time_unit.seconds",
            "anim.time_unit.timecode",
            "anim.key.layer",
            "anim.key.ease.smooth",
            "anim.key.ease.linear",
            "anim.key.ease.hold",
            "anim.key.delete_all",
            "anim.keyframes",
            "anim.audio.settings",
            "anim.audio.mute",
            "anim.audio.delete",
            "anim.export.audio",
            "anim.palette_color.clips",
            "anim.key.layer_delete",
            "anim.playback.camera",
            "window.interface.auto",
            "window.interface.desktop",
            "window.interface.tablet",
            "window.interface.phone",
            "help.shortcuts",
            "help.tips",
            "help.online_guide",
            "help.faq",
            "help.contact",
            "help.feedback",
            "tool.pick.pen",
            "tool.pick.pencil",
            "tool.pick.brush",
            "tool.pick.airbrush",
            "tool.pick.decoration",
            "tool.pick.fill",
            "tool.pick.gradient",
            "tool.pick.figure",
            "tool.pick.frame",
            "tool.pick.ruler",
            "tool.pick.text",
            "tool.pick.balloon",
            "color.settings",
            "color.new_set",
            "color.edit_sets",
            "touch.finger_draws",
            "touch.key_bar",
        ]
    );
    let new = &palette[1];
    assert_eq!(
        (new.label.as_str(), new.hint.as_str()),
        ("New...", "Ctrl+N")
    );
    // Beside the shell's Open File, the studio's Open says what it opens;
    // the File menu keeps the short "Open...".
    assert_eq!(palette[2].label, "Open Picture...");
    assert_eq!(palette[3].label, "Export as PNG...");
    let menus = crate::menus::menus();
    assert_eq!(crate::menus::label_of(&menus, "file.open"), Some("Open..."));
}

#[test]
fn every_command_has_its_label_in_every_language_or_falls_back() {
    // A label is an English message; another language without it shows
    // the English one, never the key.
    for language in ["de", "ja"] {
        let _scope = sue_lang::scope(language);
        for command in registry().iter() {
            let label = sue_lang::text("paint", command.label);
            assert!(
                !label.is_empty() && label != command.label,
                "{language} {}: {label:?}",
                command.name
            );
        }
    }
}

#[test]
fn the_menus_label_items_as_the_registry_does() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let menus = crate::menus::menus();
    assert_eq!(crate::menus::label_of(&menus, "file.new"), Some("New..."));
    assert_eq!(crate::menus::label_of(&menus, "view.actual"), Some("100%"));
    // Under Save Duplicate the format names the item; elsewhere the
    // command says what it does.
    assert_eq!(
        crate::menus::label_of(&menus, "file.export.png"),
        Some(".png (PNG)...")
    );
    assert_eq!(
        registry().label("file.export.png").as_deref(),
        Some("Export as PNG...")
    );
}

#[test]
fn a_key_given_to_a_command_without_one_runs_it() {
    let table = shortcut_table();
    let section = super::KEYS_SECTION;
    let mut rice = sue_rice::RiceSchema::new();
    rice.register_all(&sue_wm::shortcuts::declarations(section, table))
        .expect("the key settings register");
    let (setting, value) = sue_wm::shortcuts::bind(
        &rice,
        section,
        "layer.merge_visible",
        &"Ctrl+M".parse().expect("a chord"),
    );
    assert_eq!(setting, "sue_paint_keys.custom");
    rice.set(&setting, value)
        .expect("the text setting takes it");
    let chords = sue_wm::shortcuts::resolve(&rice, section, table);
    let at = table
        .iter()
        .position(|row| row.command == "layer.merge_visible")
        .expect("Merge Visible to New Layer has a row");
    assert_eq!(chords[at].to_string(), "Ctrl+M");
    assert!(
        sue_wm::shortcuts::conflicts(&rice, section, table).is_empty(),
        "Ctrl+M is free"
    );
}

/// Every key the menus show, pressed: the shortcut code Sue Paint reads
/// its keys with ([`super::SuePaint::keys`]) answers with the very command
/// of the item, and the items the shell runs (Undo, Redo, Open, Save,
/// Quit) show a key of the shell's own settings for that command.
#[test]
fn every_menu_item_shows_the_chord_that_runs_it() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let table = shortcut_table();
    let section = super::KEYS_SECTION;
    let mut rice = sue_rice::RiceSchema::new();
    sue_wm::register_rice(&mut rice).expect("the shell's settings register");
    rice.register_all(&sue_wm::shortcuts::declarations(section, table))
        .expect("the key settings register");
    let mut menus = crate::menus::menus();
    crate::menus::set_canvases(&mut menus, &["Illustration".to_owned()]);
    let mut bound = sue_wm::shortcuts::Bound::default();
    bound.refresh(&mut menus, &rice, section, table);

    fn shown(items: &[MenuItem], out: &mut Vec<(String, String)>) {
        for item in items {
            match item {
                MenuItem::Action {
                    command, shortcut, ..
                } if !shortcut.is_empty() => out.push((command.clone(), shortcut.clone())),
                MenuItem::Sub { items, .. } => shown(items, out),
                _ => {}
            }
        }
    }
    let mut items = Vec::new();
    for menu in &menus {
        shown(&menu.items, &mut items);
    }
    assert!(
        items.len() > 50,
        "the items show their keys: {}",
        items.len()
    );

    let shell = [
        ("edit.undo", &["keys.undo"][..]),
        ("edit.redo", &["keys.redo", "keys.redo_alt"][..]),
        ("file.open", &["keys.open_file"][..]),
        ("file.save", &["keys.save"][..]),
        ("app.quit", &["keys.quit"][..]),
    ];
    let ctx = egui::Context::default();
    let mut checked = 0;
    for (command, text) in &items {
        let chord: sue_rice::Keybind = text
            .parse()
            .unwrap_or_else(|_| panic!("{command} shows {text:?}, which is no chord"));
        if let Some((_, settings)) = shell.iter().find(|(name, _)| name == command) {
            let runs = settings.iter().any(|setting| {
                rice.get_keybind(setting)
                    .is_some_and(|chords| chords.chords().contains(&chord))
            });
            assert!(
                runs,
                "{command} shows {text}, which the shell does not run it with"
            );
            continue;
        }
        let key = sue_wm::keys::egui_key(chord.key)
            .unwrap_or_else(|| panic!("{command} shows {text}, which no key can press"));
        let modifiers = egui::Modifiers {
            alt: chord.modifiers.alt,
            ctrl: chord.modifiers.ctrl,
            shift: chord.modifiers.shift,
            mac_cmd: false,
            command: chord.modifiers.ctrl || chord.modifiers.super_key,
        };
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        };
        let mut ran = Vec::new();
        ctx.run_ui(input, |ui| {
            ran = ui
                .ctx()
                .input_mut(|input| bound.take(input, &rice, section, table));
        })
        .drop_without_applying_deltas();
        assert_eq!(ran, [command.as_str()], "{text}, shown beside {command}");
        checked += 1;
    }
    assert!(checked > 45, "{checked} chords pressed");
    // And the other way: no item hides a key that runs it.
    fn all(items: &[MenuItem], out: &mut Vec<String>) {
        for item in items {
            match item {
                MenuItem::Action { command, .. } => out.push(command.clone()),
                MenuItem::Sub { items, .. } => all(items, out),
                MenuItem::Separator => {}
            }
        }
    }
    let mut every = Vec::new();
    for menu in &menus {
        all(&menu.items, &mut every);
    }
    for command in every {
        let keyed = table
            .iter()
            .any(|row| row.command == command && !row.chords.is_empty());
        if keyed {
            assert!(
                items.iter().any(|(name, _)| *name == command),
                "{command} has a key the menu does not show"
            );
        }
    }
    // The defects the menus had: Delete Outside Selection showed
    // Shift+Del, which filled; Rotate Right showed a key it was not bound
    // to; Preferences showed Ctrl+, while Ctrl+K opened it.
    let key_of = |command: &str| {
        items
            .iter()
            .find(|(name, _)| name == command)
            .map(|(_, text)| text.as_str())
    };
    assert_eq!(key_of("edit.clear_outside"), Some("Shift+Delete"));
    assert_eq!(key_of("edit.fill"), Some("Alt+Delete"));
    assert_eq!(key_of("file.preferences"), Some("Ctrl+K"));
    assert_eq!(key_of("file.shortcuts"), Some("Ctrl+Alt+Shift+K"));
    assert_eq!(key_of("file.modifiers"), Some("Ctrl+Alt+Shift+Y"));
    assert_eq!(key_of("view.rotate_right"), Some("="));
    assert_eq!(key_of("window.hide_all"), Some("Tab"));
}
