//! Sue Writer's dialogs, Project Settings, Header and Footer, the start
//! page and a phone's bars, driven a frame at a time through the runtime
//! and found on screen by what they say or what they are named for a
//! screen reader, never by fixed places.

use super::*;
use egui::accesskit::Role;

/// Sue Writer with a blank project open, in a window of `size`, driven a
/// frame at a time. Screen reader names are on, so that icon buttons can
/// be found by what they are called.
struct Writer {
    runtime: Runtime,
    ctx: egui::Context,
    size: egui::Vec2,
    now: Instant,
    root: PathBuf,
    texts: Vec<(String, egui::Rect)>,
    named: Vec<(String, Role, egui::Rect, bool)>,
    /// What holds keyboard focus, by its name for a screen reader.
    focused: Option<String>,
}

impl Writer {
    fn new(tag: &str) -> Self {
        Self::sized(tag, vec2(1440.0, 900.0), None)
    }

    /// On `interface` (`phone`, `tablet`), at `size`.
    fn sized(tag: &str, size: egui::Vec2, interface: Option<&str>) -> Self {
        let (mut runtime, root) = runtime(tag);
        if let Some(interface) = interface {
            let _ = runtime.engine.world.rice.set(
                "display.interface",
                sue_rice::RiceValue::Text(interface.to_owned()),
            );
        }
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let now = Instant::now();
        frames(&mut runtime, &ctx, size, now, |_| Vec::new());
        runtime.request(
            ShellRequest::Create {
                applet: Self::id(),
                key: "blank".to_owned(),
            },
            &mut NoTextures,
            now,
        );
        let mut writer = Self {
            runtime,
            ctx,
            size,
            now,
            root,
            texts: Vec::new(),
            named: Vec::new(),
            focused: None,
        };
        writer.settle(40);
        writer
    }

    fn id() -> AppletId {
        AppletId::new(sue_writer::ID).unwrap()
    }

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
        self.texts = text_rects(&output);
        self.focused = output
            .platform_output
            .accesskit_update
            .as_ref()
            .and_then(|update| {
                update
                    .nodes
                    .iter()
                    .find(|(id, _)| *id == update.focus)
                    .and_then(|(_, node)| node.label().map(str::to_owned))
            });
        self.named = output
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
                Some((label, node.role(), rect, node.is_disabled()))
            })
            .collect();
        output.drop_without_applying_deltas();
    }

    fn settle(&mut self, count: usize) {
        for _ in 0..count {
            self.step(Vec::new());
        }
    }

    /// Where the last frame drew `text`, the lowest first (a dialog's
    /// footer sits under the menu bar).
    fn rect_of(&self, text: &str) -> Option<egui::Rect> {
        self.texts
            .iter()
            .filter(|(found, _)| found == text)
            .map(|(_, rect)| *rect)
            .max_by(|a, b| a.top().total_cmp(&b.top()))
    }

    /// Where the control called `name` for a screen reader was drawn
    /// (not a label of that name), and whether it can be used.
    fn control(&self, name: &str) -> Option<(egui::Rect, bool)> {
        self.named
            .iter()
            .find(|(label, role, ..)| label == name && *role != Role::Label)
            .map(|(_, _, rect, disabled)| (*rect, !*disabled))
    }

    fn shown(&self) -> Vec<&String> {
        self.texts.iter().map(|(text, _)| text).collect()
    }

    fn click(&mut self, at: Pos2) {
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.step(vec![egui::Event::PointerMoved(at)]);
        self.step(vec![press(true)]);
        self.step(vec![press(false)]);
        self.settle(6);
    }

    /// A click in the middle of `text`, found where the last frame drew it.
    fn click_text(&mut self, text: &str) {
        let at = self
            .rect_of(text)
            .unwrap_or_else(|| panic!("{text:?} is on screen: {:?}", self.shown()))
            .center();
        self.click(at);
    }

    /// A click on the control called `name`.
    fn click_control(&mut self, name: &str) {
        let (rect, _) = self
            .control(name)
            .unwrap_or_else(|| panic!("a control is named {name:?}: {:?}", self.named));
        self.click(rect.center());
    }

    fn press(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
        self.step(vec![super::key(key, modifiers)]);
        self.settle(6);
    }

    fn type_text(&mut self, text: &str) {
        self.step(vec![egui::Event::Text(text.to_owned())]);
        self.settle(4);
    }

    /// Runs one of Sue Writer's commands, as its menus do.
    fn command(&mut self, command: &str) {
        self.runtime.request(
            ShellRequest::AppletCommand {
                applet: Self::id(),
                command: command.to_owned(),
            },
            &mut NoTextures,
            self.now,
        );
        self.settle(30);
    }

    fn navigate(&mut self, target: &str) {
        if let Some(applet) = self.runtime.applets.get_mut(&Self::id()) {
            applet.navigate(target);
        }
        self.settle(30);
    }

    fn undo(&mut self, redo: bool) {
        let modifiers = if redo {
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::COMMAND
        };
        self.press(egui::Key::Z, modifiers);
    }

    fn flush(&mut self) {
        self.runtime.typing.flush(&mut self.runtime.engine.history);
    }

    fn arena(&self) -> &sue_core::Arena {
        &self.runtime.engine.world.arena
    }

    /// Every entity: id, class, name and properties, in id order, to tell
    /// one state of the arena from another.
    fn state(&self) -> Vec<String> {
        let mut rows: Vec<String> = self
            .arena()
            .iter()
            .map(|(_, entity)| {
                format!(
                    "{:?} {} {:?} {:?} {:?}",
                    entity.id(),
                    entity.class(),
                    entity.name(),
                    entity.properties(),
                    entity.string_buffer().map(|text| text.as_str().to_owned()),
                )
            })
            .collect();
        rows.sort();
        rows
    }

    /// The manuscript that is open: the one project of the blank start.
    fn project(&self) -> &sue_core::Entity {
        self.arena()
            .iter()
            .map(|(_, entity)| entity)
            .find(|entity| entity.class() == sue_model::class::MANUSCRIPT)
            .expect("a project")
    }

    fn manuscripts(&self) -> Vec<String> {
        self.arena()
            .iter()
            .map(|(_, entity)| entity)
            .filter(|entity| entity.class() == sue_model::class::MANUSCRIPT)
            .map(|entity| entity.name().to_owned())
            .collect()
    }

    fn tables(&self) -> usize {
        self.arena()
            .iter()
            .filter(|(_, entity)| entity.class() == sue_model::TEXT_TABLE)
            .count()
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Insert Table on Sue's dialog host: its buttons stand Insert then
/// Cancel; Escape sends it away and nothing is made; Insert makes one
/// table as one step that Undo takes back to the arena as it was, ids
/// and all, and Redo brings back.
#[test]
fn sue_writers_table_dialog_cancels_with_escape_and_inserts_with_one_undo_step() {
    let mut writer = Writer::new("writer-dialog-table");
    writer.command("table.insert");
    assert!(
        writer.rect_of("Insert Table").is_some(),
        "{:?}",
        writer.shown()
    );
    let insert = writer.rect_of("Insert").expect("Insert");
    let cancel = writer.rect_of("Cancel").expect("Cancel");
    assert!(
        insert.center().x < cancel.center().x
            && (insert.center().y - cancel.center().y).abs() < 1.0,
        "the footer reads Insert, Cancel: {insert:?} {cancel:?}"
    );
    writer.press(egui::Key::Escape, egui::Modifiers::NONE);
    assert!(writer.rect_of("Insert Table").is_none(), "Escape closes it");
    assert_eq!(writer.tables(), 0, "nothing was made");
    let before = writer.state();
    writer.command("table.insert");
    writer.click_text("Insert");
    assert!(writer.rect_of("Insert Table").is_none(), "Insert closes it");
    assert_eq!(writer.tables(), 1, "one table");
    let after = writer.state();
    writer.undo(false);
    assert_eq!(writer.state(), before, "Undo: the arena as it was");
    writer.undo(true);
    assert_eq!(writer.state(), after, "Redo: the table again");
}

/// Link: Enter in its one field accepts, as in every dialog of Sue; its
/// other answer, Remove Link, takes the link off, and Undo puts it back.
#[test]
fn sue_writers_link_dialog_accepts_with_enter_and_removes_with_its_other_answer() {
    let mut writer = Writer::new("writer-dialog-link");
    writer.type_text("Sue");
    writer.flush();
    let link_of = |writer: &Writer| {
        writer
            .arena()
            .iter()
            .filter_map(|(_, entity)| entity.ext::<sue_model::TextMarks>())
            .flat_map(|marks| marks.marks().iter())
            .find(|mark| mark.kind == sue_model::mark_kind::LINK)
            .map(|mark| mark.value.clone())
    };
    writer.command("edit.select_all");
    writer.command("insert.link");
    assert!(
        writer.rect_of("Web address").is_some(),
        "{:?}",
        writer.shown()
    );
    writer.click_control("Web address");
    writer.press(egui::Key::End, egui::Modifiers::NONE);
    writer.type_text("example.org");
    writer.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert!(writer.rect_of("Web address").is_none(), "Enter accepted");
    assert_eq!(
        link_of(&writer),
        Some(sue_core::Value::Text("https://example.org".to_owned()))
    );
    writer.command("edit.select_all");
    writer.command("insert.link");
    writer.click_text("Remove Link");
    assert_eq!(link_of(&writer), None, "the link came off");
    writer.undo(false);
    assert_eq!(
        link_of(&writer),
        Some(sue_core::Value::Text("https://example.org".to_owned())),
        "Undo puts it back"
    );
}

/// Project Settings: a new title typed in its field and Apply rename the
/// project as one step; Undo gives the old name back, Redo the new.
#[test]
fn sue_writers_project_settings_rename_the_project_as_one_undo_step() {
    let mut writer = Writer::new("writer-dialog-settings");
    let old = writer.project().name().to_owned();
    let before = writer.state();
    writer.command("project.settings");
    assert!(
        writer.rect_of("Project Settings").is_some(),
        "{:?}",
        writer.shown()
    );
    // The orientation is a pair of segments, the margins a number field.
    assert!(writer.rect_of("Portrait").is_some());
    assert!(writer.rect_of("Landscape").is_some());
    writer.click_control("Title");
    writer.press(egui::Key::A, egui::Modifiers::COMMAND);
    writer.type_text("The Long Road");
    writer.click_text("Apply");
    assert!(writer.rect_of("Project Settings").is_none());
    assert_eq!(writer.project().name(), "The Long Road");
    let after = writer.state();
    writer.undo(false);
    assert_eq!(writer.project().name(), old);
    assert_eq!(writer.state(), before, "Undo: the arena as it was");
    writer.undo(true);
    assert_eq!(writer.state(), after, "Redo: renamed again");
}

/// Header and Footer: a field's button puts the field in the footer's
/// middle when nothing was typed in yet; OK keeps it on the project as one
/// step, and Undo takes it off.
#[test]
fn sue_writers_header_and_footer_buttons_put_in_a_field_and_undo_takes_it_off() {
    let mut writer = Writer::new("writer-dialog-furniture");
    let before = writer.state();
    writer.command("insert.header_footer");
    assert!(
        writer.rect_of("Header and Footer").is_some(),
        "{:?}",
        writer.shown()
    );
    writer.click_text("Page Number");
    writer.click_text("OK");
    assert!(writer.rect_of("Header and Footer").is_none());
    let footer = writer
        .project()
        .property(FOOTER)
        .and_then(sue_core::Value::as_str)
        .map(str::to_owned)
        .unwrap_or_default();
    assert!(footer.contains("{page}"), "{footer:?}");
    writer.undo(false);
    assert_eq!(writer.state(), before, "Undo: the arena as it was");
}

/// The property Sue Writer keeps the footer in.
const FOOTER: &str = "sue_writer.footer";

/// The start page shows the templates and, as pages too, the presets for
/// long work; a click on a preset's page starts a project of that name,
/// and the page names itself and its help when the pointer rests on it.
#[test]
fn sue_writers_start_page_shows_presets_as_pages_and_a_click_starts_one() {
    let mut writer = Writer::new("writer-start-page");
    writer.navigate("start");
    for name in ["Blank Document", "Report", "Novel", "Screenplay"] {
        assert!(
            writer.rect_of(name).is_some(),
            "{name}: {:?}",
            writer.shown()
        );
    }
    // The card above the name: its page.
    let name = writer.rect_of("Novel").unwrap();
    let page = name.center_top() - vec2(0.0, name.height() * 2.0);
    writer.step(vec![egui::Event::PointerMoved(page)]);
    writer.settle(60);
    assert!(
        writer
            .rect_of("Chapters of scenes, with research for characters and places.")
            .is_some(),
        "the card's tip: {:?}",
        writer.shown()
    );
    let before = writer.manuscripts().len();
    writer.click(page);
    assert_eq!(
        writer.manuscripts().len(),
        before + 1,
        "{:?}",
        writer.manuscripts()
    );
    assert!(writer.manuscripts().iter().any(|name| name == "Novel"));
}

/// On a phone, the top bar's buttons are Sue's icon buttons that name
/// themselves: Redo cannot be used before anything was undone, resting on
/// Find says its name and its keys, Find opens the find bar and Find again
/// closes it.
#[test]
fn sue_writers_phone_bar_names_its_buttons_and_find_opens_the_find_bar() {
    let mut writer = Writer::sized("writer-phone-bar", vec2(430.0, 932.0), Some("phone"));
    for name in ["Menus", "Undo", "Redo", "Find"] {
        assert!(writer.control(name).is_some(), "{name}: {:?}", writer.named);
    }
    assert_eq!(
        writer.control("Redo").map(|(_, on)| on),
        Some(false),
        "nothing to redo"
    );
    assert!(
        writer.rect_of("Replace...").is_none(),
        "the find bar is closed at first: {:?}",
        writer.shown()
    );
    let (find, _) = writer.control("Find").unwrap();
    writer.step(vec![egui::Event::PointerMoved(find.center())]);
    writer.settle(60);
    assert!(
        writer.rect_of("Ctrl+F").is_some(),
        "the tip shows the keys: {:?}",
        writer.shown()
    );
    writer.click(find.center());
    assert!(
        writer.rect_of("Replace...").is_some(),
        "the find bar is open: {:?}",
        writer.shown()
    );
    // Find again closes it.
    writer.click(find.center());
    assert!(
        writer.rect_of("Replace...").is_none(),
        "the find bar closed: {:?}",
        writer.shown()
    );
}

/// A new comment: the typing goes straight into its field, Tab leaves the
/// field for the footer's Add Comment (the field does not pull focus back
/// every frame), and a second comment's field takes the typing again.
#[test]
fn sue_writers_comment_field_takes_the_typing_and_lets_tab_reach_the_footer() {
    let mut writer = Writer::new("writer-dialog-comment");
    writer.command("review.comment");
    assert!(
        writer.rect_of("New Comment").is_some(),
        "{:?}",
        writer.shown()
    );
    writer.type_text("First thought");
    assert!(
        writer.rect_of("First thought").is_some(),
        "typed into the field: {:?}",
        writer.shown()
    );
    writer.press(egui::Key::Tab, egui::Modifiers::NONE);
    assert_eq!(writer.focused.as_deref(), Some("Add Comment"));
    writer.press(egui::Key::Escape, egui::Modifiers::NONE);
    assert!(writer.rect_of("New Comment").is_none(), "Escape closed it");
    writer.command("review.comment");
    writer.type_text("Second thought");
    assert!(
        writer.rect_of("Second thought").is_some(),
        "the field took the typing again: {:?}",
        writer.shown()
    );
}

/// Project Settings: the heading font's box opens a search that keeps what
/// is typed into it (it was emptied again every frame), as the body
/// font's does.
#[test]
fn sue_writers_heading_font_search_keeps_what_is_typed() {
    let mut writer = Writer::new("writer-dialog-fonts");
    writer.command("project.settings");
    for name in ["Body font", "Heading font"] {
        let (rect, usable) = writer
            .control(name)
            .unwrap_or_else(|| panic!("{name} is a control: {:?}", writer.named));
        assert!(usable, "{name} can be used");
        let (title, _) = writer.control("Title").expect("the title field");
        assert!(
            (rect.width() - title.width()).abs() < 1.0,
            "{name} is as wide as the title field: {rect:?} {title:?}"
        );
    }
    writer.click_control("Heading font");
    writer.type_text("Zq");
    writer.settle(10);
    assert!(
        writer.rect_of("Zq").is_some(),
        "the search kept its text: {:?}",
        writer.shown()
    );
    writer.press(egui::Key::Escape, egui::Modifiers::NONE);
    assert!(writer.rect_of("Zq").is_none(), "Escape closed the fonts");
    assert!(
        writer.rect_of("Project Settings").is_some(),
        "Escape closed the fonts first, not the dialog"
    );
    writer.press(egui::Key::Escape, egui::Modifiers::NONE);
    assert!(
        writer.rect_of("Project Settings").is_none(),
        "a second Escape cancels the dialog"
    );
}

/// The start page's pages are reached with Tab like any button, and Enter
/// on one starts a project from it.
#[test]
fn sue_writers_start_page_opens_a_template_from_the_keyboard() {
    let mut writer = Writer::new("writer-start-keys");
    writer.navigate("start");
    let before = writer.manuscripts().len();
    let mut seen = Vec::new();
    for _ in 0..60 {
        writer.press(egui::Key::Tab, egui::Modifiers::NONE);
        seen.push(writer.focused.clone());
        if writer.focused.as_deref() == Some("Report") {
            break;
        }
    }
    assert_eq!(
        writer.focused.as_deref(),
        Some("Report"),
        "Tab walked: {seen:?}"
    );
    writer.press(egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(writer.manuscripts().len(), before + 1);
    assert!(writer.manuscripts().iter().any(|name| name == "Report"));
}
