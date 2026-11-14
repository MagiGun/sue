//! The registry against the menus, the shortcuts, the palette, the style
//! sheets and the formats, and every command run on a project.

use std::collections::BTreeSet;

use sue_core::{Arena, UndoStack};
use sue_rice::RiceSchema;
use sue_wm::{AppletContext, Flags, MenuItem, ShellRequest, SueApplet, World};

use super::{SHELL_OWNED, registry, shortcut_table};
use crate::studio::SueWriter;

/// Every command the menus name, submenus included.
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
    let mut out = Vec::new();
    for menu in crate::menus::menus() {
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
    assert!(missing.is_empty(), "not registered: {missing:?}");
}

/// Commands reached from somewhere other than a menu, the palette or a
/// default key: the toolbar's paragraph style picker runs `style.<key>`.
const ELSEWHERE: [&str; 1] = ["style."];

#[test]
fn every_command_can_be_reached() {
    // In a menu, in the palette, on a default key, on the toolbar, or
    // deliberately hidden (and then still bindable in Settings).
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
                || ELSEWHERE.iter().any(|prefix| name.starts_with(prefix)))
        })
        .map(|command| command.name)
        .collect();
    assert!(
        unreached.is_empty(),
        "in no menu, palette or key: {unreached:?}"
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
    // The shell's keys stay the shell's, so the menus show them.
    for name in SHELL_OWNED {
        assert!(registry().has(name), "{name}");
    }
}

#[test]
fn the_shortcuts_keep_the_keys_they_had() {
    // The default chords of 0.0.75 (and Ctrl+F12, which the menu showed
    // all along), as settings write them.
    let expected: &[(&str, &str)] = &[
        ("binder.new_folder", "Ctrl+Shift+N"),
        ("binder.new_doc", "Ctrl+N"),
        ("compile.open", "Ctrl+Shift+E"),
        ("compile.pdf", "Ctrl+P"),
        ("format.bold", "Ctrl+B"),
        ("format.italic", "Ctrl+I"),
        ("format.underline", "Ctrl+U"),
        ("format.strike", "Ctrl+Shift+X"),
        ("project.split", "Ctrl+Shift+K"),
        ("format.superscript", "Ctrl+Shift++ or Ctrl+Shift+="),
        ("format.subscript", "Ctrl+="),
        ("format.clear", "Ctrl+Space"),
        ("align.center", "Ctrl+E"),
        ("align.left", "Ctrl+L"),
        ("align.right", "Ctrl+R"),
        ("align.justify", "Ctrl+J"),
        ("style.heading1", "Ctrl+Alt+1"),
        ("style.heading2", "Ctrl+Alt+2"),
        ("style.heading3", "Ctrl+Alt+3"),
        ("style.body", "Ctrl+Alt+0"),
        ("insert.footnote", "Ctrl+Alt+F"),
        ("review.comment", "Ctrl+Alt+C"),
        ("insert.endnote", "Ctrl+Alt+D"),
        ("review.track", "Ctrl+Shift+C"),
        ("format.grow", "Ctrl+]"),
        ("format.shrink", "Ctrl+["),
        ("insert.mark.nbsp", "Ctrl+Shift+Space"),
        ("review.next_misspelling", "F7"),
        ("insert.page_break", "Ctrl+Enter"),
        ("insert.link", "Ctrl+K"),
        ("edit.find", "Ctrl+F"),
        ("edit.replace", "Ctrl+H"),
        ("edit.find_previous", "Shift+F3"),
        ("edit.find_next", "F3"),
        ("view.composition", "F11"),
        ("view.document", "Ctrl+1"),
        ("view.corkboard", "Ctrl+2"),
        ("view.outliner", "Ctrl+3"),
        ("project.snapshot", "Ctrl+5"),
        ("list.bullet", "Ctrl+Shift+L"),
        ("table.insert", "Ctrl+F12"),
        ("file.save_as", "Ctrl+Shift+S"),
    ];
    let keyed: Vec<(&str, String)> = shortcut_table()
        .iter()
        .filter(|row| !row.chords.is_empty())
        .map(|row| (row.command, row.chords.to_string()))
        .collect();
    assert_eq!(keyed.len(), expected.len(), "{keyed:?}");
    for (command, chords) in expected {
        let row = shortcut_table()
            .iter()
            .find(|row| row.command == *command)
            .unwrap_or_else(|| panic!("{command} has no row"));
        assert_eq!(
            row.chords,
            chords.parse().expect("the chords parse"),
            "{command}"
        );
    }
}

#[test]
fn keyboard_shortcuts_names_every_command_as_the_menus_do() {
    let rice = RiceSchema::new();
    for language in ["en", "de", "ja"] {
        let _scope = sue_lang::scope(language);
        let rows = super::shortcut_rows(&rice);
        assert_eq!(rows.len(), shortcut_table().len());
        for ((label, _), shortcut) in rows.iter().zip(shortcut_table()) {
            let named = registry().label(shortcut.command).expect("a label");
            assert_eq!(label, named.trim_end_matches("..."), "{language}");
        }
    }
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let rows = super::shortcut_rows(&rice);
    assert!(
        rows.contains(&("Export as PDF".to_owned(), "Ctrl+P".to_owned())),
        "{rows:?}"
    );
    assert!(rows.contains(&("Title Style".to_owned(), "None".to_owned())));
    // A command without a default key is named in the language in use too,
    // not in the English of the settings declarations.
    let _german = sue_lang::scope("de");
    let rows = super::shortcut_rows(&rice);
    assert!(
        rows.iter()
            .any(|(label, _)| label == "Als Word-Dokument exportieren"),
        "{rows:?}"
    );
    assert!(
        rows.iter().all(|(label, _)| label != "Title Style"),
        "{rows:?}"
    );
}

#[test]
fn the_palette_offers_what_it_did() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let entries: Vec<(String, String, String)> = registry()
        .palette(&RiceSchema::new())
        .into_iter()
        .map(|entry| (entry.label, entry.command, entry.hint))
        .collect();
    let expected = [
        ("New Project...", "file.new", ""),
        ("Import a Document...", "file.import", ""),
        ("Export...", "compile.open", "Ctrl+Shift+E"),
        ("Export as PDF...", "compile.pdf", "Ctrl+P"),
        ("Settings...", "file.settings", ""),
        ("Composition Mode", "view.composition", "F11"),
        ("Writing Statistics...", "project.statistics", ""),
        ("Keyboard Shortcuts", "help.shortcuts", ""),
    ]
    .map(|(label, command, hint)| (label.to_owned(), command.to_owned(), hint.to_owned()));
    assert_eq!(entries, expected);
}

#[test]
fn every_style_of_the_built_in_sheets_has_a_command() {
    let fonts = sue_type::Fonts::default();
    let mut keys = BTreeSet::new();
    for sheet in ["standard", "manuscript", "screenplay"] {
        let sheet = sue_type::StyleSheet::named(sheet, &fonts);
        keys.extend(
            sheet
                .styles
                .iter()
                .map(|style| format!("style.{}", style.key)),
        );
    }
    let registered: BTreeSet<String> = registry()
        .iter()
        .map(|command| command.name)
        .filter(|name| name.starts_with("style."))
        .map(str::to_owned)
        .collect();
    assert_eq!(registered, keys);
}

#[test]
fn every_format_has_an_export_command() {
    let registered: Vec<&str> = registry()
        .iter()
        .map(|command| command.name)
        .filter(|name| name.starts_with("compile.") && *name != "compile.open")
        .collect();
    let formats: Vec<String> = crate::FORMATS
        .iter()
        .map(|(key, ..)| format!("compile.{key}"))
        .collect();
    assert_eq!(registered, formats);
}

/// Runs `work` with an applet context over an empty world.
fn with_context<R>(work: impl FnOnce(&mut AppletContext<'_>) -> R) -> R {
    let rice = RiceSchema::new();
    let theme = sue_wm::Theme::from_rice(&rice);
    let mut world = World {
        arena: Arena::new(),
        rice,
    };
    let mut history = UndoStack::new();
    let mut typing = sue_wm::Typing::default();
    let mut requests = Vec::new();
    let catalog = sue_wm::Catalog::default();
    let mut textures = sue_platform::NoTextures;
    let mut transport = sue_wm::Transport::default();
    let work_list = sue_wm::WorkList::default();
    let mut transfers = sue_wm::Transfers::default();
    let mut cx = AppletContext {
        world: &mut world,
        history: &mut history,
        sue_history: None,
        typing: &mut typing,
        requests: &mut requests,
        catalog: &catalog,
        theme: &theme,
        textures: &mut textures,
        audio: None,
        transport: &mut transport,
        work: &work_list,
        transfers: &mut transfers,
        view: sue_wm::ShellView {
            form_factor: sue_platform::FormFactor::Desktop,
            profile: sue_rice::HardwareProfile::Balanced,
            now: std::time::Instant::now(),
            dt: 0.0,
            page: None,
            reveal: 1.0,
            interactive: false,
        },
    };
    work(&mut cx)
}

/// One name each command answers to: a plain command's own, a family's
/// first member in the menus (or `table.insert.2x3`).
fn names() -> Vec<String> {
    let menus = menu_commands();
    registry()
        .iter()
        .map(|command| {
            if !command.is_family() {
                return command.name.to_owned();
            }
            menus
                .iter()
                .find(|item| item.starts_with(command.name))
                .cloned()
                .unwrap_or_else(|| format!("{}2x3", command.name))
        })
        .collect()
}

#[test]
fn every_command_that_stays_runs_on_a_project_without_leaving_it() {
    with_context(|cx| {
        let root = crate::sample_project(cx).expect("the sample project");
        let mut writer = SueWriter::default();
        writer.open_project(cx, root, None);
        cx.view.page = Some(sue_wm::AppletId::new(crate::ID).expect("the id"));
        // One frame, so the editor holds the open document's text.
        let ctx = egui::Context::default();
        let frame = |writer: &mut SueWriter, cx: &mut AppletContext<'_>| {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                ..egui::RawInput::default()
            };
            let mut output = ctx.run_ui(raw, |ui| writer.page_ui(ui, cx));
            output.textures_delta.clear();
        };
        frame(&mut writer, cx);
        assert!(writer.editor.current().is_some(), "a document is open");
        let names = names();
        assert_eq!(names.len(), registry().len());
        for name in names {
            let flags = registry().flags(&name);
            if flags.has(Flags::LEAVES_CANVAS) {
                // It does leave (the templates folder is not made here, and
                // Export Table As needs the caret in a table: the runtime
                // test of tables runs it).
                if name != "file.templates_folder" && name != "table.export" {
                    cx.requests.clear();
                    assert!(writer.run(&name, cx), "{name} did not run");
                    assert!(
                        cx.requests.iter().any(|request| matches!(
                            request,
                            ShellRequest::PickFile(_)
                                | ShellRequest::Hub
                                | ShellRequest::Command(_)
                                | ShellRequest::OpenSettings(_)
                        )),
                        "{name} is marked as leaving but stays"
                    );
                }
                continue;
            }
            cx.requests.clear();
            assert!(writer.run(&name, cx), "{name} did not run");
            let left: Vec<&ShellRequest> = cx
                .requests
                .iter()
                .filter(|request| {
                    matches!(
                        request,
                        ShellRequest::PickFile(_)
                            | ShellRequest::Hub
                            | ShellRequest::Command(_)
                            | ShellRequest::Fail(_)
                    )
                })
                .collect();
            assert!(left.is_empty(), "{name} asked for {left:?}");
            // Close what it opened, so the next command finds the studio.
            writer.dialog = None;
            frame(&mut writer, cx);
        }
    });
}

/// File, Open offers every file Sue Writer opens, table files too: what
/// the system's dialog filters out could not be opened from the menu.
#[test]
fn file_open_offers_every_file_sue_writer_opens() {
    with_context(|cx| {
        let mut writer = SueWriter::default();
        for command in ["file.open", "file.import"] {
            cx.requests.clear();
            assert!(writer.run(command, cx));
            let Some(ShellRequest::PickFile(pick)) = cx.requests.first() else {
                panic!("{command} asks for a file");
            };
            for ext in ["csv", "tsv", "tab", "docx", "md", "txt"] {
                assert!(
                    pick.extensions.iter().any(|one| one == ext),
                    "{command}: {ext} in {:?}",
                    pick.extensions
                );
                let name = std::path::PathBuf::from(format!("a.{ext}"));
                assert!(writer.opens_file(&name), "{ext} is opened");
            }
        }
    });
}
