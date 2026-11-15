//! Sue Paint's page of Settings. File, Preferences opens it: one settings
//! model for all of Sue, applied at once and undone like any setting,
//! instead of a second dialog of its own.
//!
//! Clip Studio Paint's tutorials name its Preferences options, so the
//! names it gives the options Sue Paint honors are search words: typing
//! "Brush-type cursor" in the Settings filter finds Brush cursor. An option
//! becomes a row here when its feature lands, never before.

use sue_wm::{
    Choice, CommandButton, KeyMap, SettingsGroup, SettingsItem, SettingsPage, shortcuts::Shortcut,
};

use crate::studio::{ID, KEYS_SECTION};

/// Clip Studio Paint's Preferences: the name of an option Sue Paint
/// honors (its label id in the language database, with the page it is
/// on) and the setting that holds it.
const CLIP_NAMES: &[(&str, &str, &str)] = &[
    (
        "pref-tools",
        "pref-preview-lines-by-pressing-modifier-key-shift",
        "sue_paint.shift_line",
    ),
    (
        "pref-touch-gestures",
        "pref-use-tools-with-finger",
        crate::studio_mode::FINGER_KEY,
    ),
    (
        "pref-touch-gestures",
        "pref-two-finger-swipe",
        crate::studio_mode::SWIPE_KEY,
    ),
    (
        "pref-touch-gestures",
        "pref-two-finger-pinch",
        crate::studio_mode::PINCH_KEY,
    ),
    (
        "pref-touch-gestures",
        "pref-two-finger-rotation",
        crate::studio_mode::ROTATE_KEY,
    ),
    (
        "pref-touch-gestures",
        "pref-two-finger-tap",
        "sue_paint.two_finger_tap",
    ),
    (
        "pref-touch-gestures",
        "pref-three-finger-tap",
        "sue_paint.three_finger_tap",
    ),
    ("pref-interface", "pref-interface-color", "sue_paint.theme"),
    (
        "pref-interface",
        "pref-switch-to-tablet-interface",
        "display.interface",
    ),
    (
        "pref-cursor",
        "pref-brush-type-cursor",
        "sue_paint.brush_cursor",
    ),
    (
        "pref-layer-frame",
        "pref-opacity-of-selection-layers",
        "sue_paint.selection_layer_opacity",
    ),
    ("pref-canvas", "pref-step", "sue_paint.rotate_step"),
    (
        "pref-file",
        "pref-save-recovery-data-every",
        "general.autosave_seconds",
    ),
    (
        "pref-file",
        "pref-save-format",
        crate::clip_io::SAVE_FORMAT_KEY,
    ),
];

/// The search words of each setting: Clip Studio's name for it and the
/// Preferences page it is on, in the language in use.
fn search_words() -> Vec<(String, String)> {
    CLIP_NAMES
        .iter()
        .map(|(page, label, path)| {
            let words = format!(
                "{} {} {}",
                tr!("preferences"),
                sue_lang::text("paint", page),
                sue_lang::text("paint", label).trim_end_matches(':'),
            );
            ((*path).to_owned(), words)
        })
        .collect()
}

/// The page, in the language in use, titled `title` (Sue Paint's name).
pub(crate) fn page(title: &str) -> SettingsPage {
    let param = |path: &str| SettingsItem::Param(path.to_owned());
    let params = |paths: &[&str]| paths.iter().map(|path| param(path)).collect::<Vec<_>>();
    let group = SettingsGroup::new;
    let custom_keys: Vec<SettingsItem> = (0..crate::studio_mode::CUSTOM_KEYS)
        .map(|index| param(&crate::studio_mode::custom_key(index)))
        .collect();
    let mut touch = params(&[
        crate::simple::MODE_KEY,
        crate::studio_mode::FINGER_KEY,
        crate::studio_mode::SWIPE_KEY,
        crate::studio_mode::PINCH_KEY,
        crate::studio_mode::ROTATE_KEY,
        "sue_paint.two_finger_tap",
        "sue_paint.three_finger_tap",
    ]);
    touch.extend(custom_keys);
    let mut pen = params(&[
        "sue_paint.shift_line",
        "sue_paint.brush_cursor",
        "sue_paint.rotate_step",
        crate::panels::TOOL_COLOR_ICON,
    ]);
    pen.extend(
        crate::dialogs::MODIFIERS
            .iter()
            .map(|(path, _, _)| param(path)),
    );
    pen.push(SettingsItem::Actions(vec![
        CommandButton::of(ID, tr!("pen-pressure-settings-ellipsis"), "file.pressure"),
        CommandButton::of(ID, tr!("command-bar-settings-ellipsis"), "file.command_bar"),
    ]));
    let mut page = SettingsPage::new(
        ID,
        title,
        vec![
            group(
                tr!("settings-group-layout"),
                params(&[crate::layout_style::KEY, crate::extras::REMEMBER_LAYOUT]),
            ),
            group(
                tr!("settings-group-new-canvas"),
                params(&["sue_paint.width", "sue_paint.height", "sue_paint.dpi"]),
            ),
            group(
                tr!("settings-group-saving"),
                params(&[
                    crate::clip_io::SAVE_FORMAT_KEY,
                    crate::materials::LOCATION_KEY,
                ]),
            ),
            group(tr!("settings-group-pen-and-tools"), pen),
            group(tr!("settings-group-touch"), touch),
            group(
                tr!("settings-group-layers"),
                params(&[
                    "sue_paint.selection_layer_opacity",
                    crate::studio::ASK_DELETE_FOLDERS,
                ]),
            ),
            group(
                tr!("settings-group-colors"),
                vec![
                    SettingsItem::Note(tr!("settings-theme-note")),
                    param("sue_paint.theme"),
                ],
            ),
            group(
                tr!("settings-group-custom-colors"),
                vec![SettingsItem::Section("sue_paint_colors".to_owned())],
            )
            .when("sue_paint.theme", sue_wm::STUDIO_THEME_CUSTOM),
            group(
                tr!("settings-group-advanced"),
                params(&[crate::dialogs::PEN_PRESSURE]),
            )
            .advanced(),
        ],
    );
    // Its deep link before it had a page of its own.
    page.aliases.push("studios".to_owned());
    page.search = search_words();
    page.keys = Some(keymap(title));
    page
}

/// Sue Paint's keyboard shortcuts, grouped as its menus are, then the
/// tools and the commands no menu shows.
pub(crate) fn keymap(title: &str) -> KeyMap {
    let mut map = studio_keymap(title);
    map.groups.push(sub_tool_keys());
    map
}

/// The key map of the registry's commands, grouped as the menus are.
fn studio_keymap(title: &str) -> KeyMap {
    let registry = crate::commands::registry();
    let table: &[Shortcut] = crate::commands::shortcut_table();
    KeyMap::from_menus(
        ID,
        title,
        KEYS_SECTION,
        &crate::menus::menus(),
        table,
        |command| registry.label(command),
        |start| match start {
            "tool" | "subtool" | "brush" => tr!("keys-group-tools"),
            "color" => tr!("keys-group-colors"),
            _ => tr!("keys-group-other"),
        },
    )
}

/// The key map's group of sub tools: every sub tool of the tool palette
/// as Sue Paint brings it, in its order, none with a key until someone
/// gives one.
fn sub_tool_keys() -> sue_wm::KeyGroup {
    let toolbox = crate::tools::Toolbox::default();
    let mut commands: Vec<sue_wm::KeyCommand> = Vec::new();
    for slot in toolbox.slots() {
        for sub in slot.subs() {
            let command = crate::commands::subtool::command_name(sub.tool, &sub.name);
            if commands.iter().any(|known| known.command == command) {
                continue;
            }
            commands.push(sue_wm::KeyCommand {
                command,
                label: format!(
                    "{}: {}",
                    crate::localize(&slot.name),
                    crate::localize(&sub.name)
                ),
                defaults: sue_rice::Keybinds::default(),
            });
        }
    }
    sue_wm::KeyGroup {
        title: tr!("keys-group-sub-tools"),
        commands,
    }
}

/// What the drop-downs of the page offer: each Studio Mode key runs one
/// of the menus' commands.
pub(crate) fn choices() -> Vec<(String, Vec<Choice>)> {
    let mut commands = vec![Choice::new("", tr!("settings-no-command"))];
    for (label, command) in crate::menus::commands(&crate::menus::menus()) {
        if command.ends_with('.') || commands.iter().any(|known| known.value == command) {
            continue;
        }
        commands.push(Choice::new(command, label));
    }
    (0..crate::studio_mode::CUSTOM_KEYS)
        .map(|index| (crate::studio_mode::custom_key(index), commands.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_wm::SueApplet;

    fn declared() -> Vec<String> {
        crate::SuePaint::default()
            .rice_declarations()
            .iter()
            .map(|declaration| declaration.path.clone())
            .collect()
    }

    #[test]
    fn every_clip_studio_name_finds_a_setting_sue_ships() {
        let mut rice = sue_rice::RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
        sue_wm::register_rice(&mut rice).unwrap();
        for (page, label, path) in CLIP_NAMES {
            assert!(rice.entry(path).is_some(), "{path} is shipped");
            assert!(sue_lang::has("paint", page), "{page}");
            assert!(sue_lang::has("paint", label), "{label}");
        }
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let words = search_words();
        assert!(
            words
                .iter()
                .any(|(path, words)| path == "sue_paint.brush_cursor"
                    && words.contains("Brush-type cursor")),
            "{words:?}"
        );
    }

    #[test]
    fn the_page_shows_every_setting_of_sue_paint_once() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let page = page("Sue Paint");
        let mut shown: Vec<String> = Vec::new();
        for item in page.claims() {
            match item {
                SettingsItem::Param(path) => shown.push(path.clone()),
                SettingsItem::Section(section) => shown.push(format!("{section}.*")),
                _ => {}
            }
        }
        let mut once = shown.clone();
        once.sort();
        once.dedup();
        assert_eq!(once.len(), shown.len(), "a row twice: {shown:?}");
        for path in declared() {
            let section = path.split('.').next().unwrap_or_default();
            if section == KEYS_SECTION || path == "sue_paint.language" {
                // The keys are on Keyboard Shortcuts, the language on
                // Language.
                continue;
            }
            assert!(
                shown.contains(&path) || shown.contains(&format!("{section}.*")),
                "{path} has no row"
            );
        }
        assert!(page.answers("studios"));
        let keys = page.keys.expect("a key map");
        assert_eq!(keys.section, KEYS_SECTION);
        assert_eq!(keys.groups[0].title, "File");
        // Every command that can be given a key is on it, once per menu.
        for shortcut in crate::commands::shortcut_table() {
            assert!(
                keys.find(shortcut.command).is_some(),
                "{}",
                shortcut.command
            );
        }
    }

    #[test]
    fn studio_mode_keys_choose_among_the_menus_commands() {
        let choices = choices();
        assert_eq!(choices.len(), crate::studio_mode::CUSTOM_KEYS);
        for (index, (path, offered)) in choices.iter().enumerate() {
            assert_eq!(path, &crate::studio_mode::custom_key(index));
            let default = crate::studio_mode::CUSTOM_DEFAULTS[index];
            assert!(
                offered.iter().any(|choice| choice.value == default),
                "{default} is offered"
            );
        }
    }
}
