//! The File menu: projects, documents, templates, import and export.

use sue_wm::{AppletContext, Call, Command, Facts, FilePick, Flags, ItemState, ShellRequest};

use crate::compile::{FORMATS, Separator, Titles};
use crate::studio::{CompileChoices, Dialog, SueWriter, me};
use crate::templates::Templates;

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    Command::new(name, label, "file", run).state(state)
}

pub(super) const COMMANDS: &[Command<SueWriter>] = &[
    c("file.new", "new-project-ellipsis").with(Flags::PALETTE),
    // The shell's Open key (`keys.open_file`) runs `file.open`.
    c("file.open", "open-ellipsis").with(Flags::LEAVES_CANVAS),
    c("file.import", "import-a-document-ellipsis")
        .with(Flags::PALETTE)
        .with(Flags::LEAVES_CANVAS),
    c("binder.new_doc", "new-document").keys("Ctrl+N"),
    c("binder.new_folder", "new-folder").keys("Ctrl+Shift+N"),
    // The shell's Save key (`keys.save`) runs `file.save`: the project.
    c("file.save", "save").with(Flags::LEAVES_CANVAS),
    c("file.save_as", "save-as-ellipsis")
        .keys("Ctrl+Shift+S")
        .with(Flags::LEAVES_CANVAS),
    c("file.save_template", "save-as-template"),
    c("file.templates_folder", "templates-folder").with(Flags::LEAVES_CANVAS),
    c("compile.open", "export-ellipsis")
        .keys("Ctrl+Shift+E")
        .with(Flags::PALETTE),
    // One per format of `compile::FORMATS`, in its order.
    c("compile.pdf", "export-as-pdf-ellipsis")
        .keys("Ctrl+P")
        .with(Flags::PALETTE)
        .with(Flags::LEAVES_CANVAS),
    c("compile.docx", "export-as-docx-ellipsis").with(Flags::LEAVES_CANVAS),
    c("compile.odt", "export-as-odt-ellipsis").with(Flags::LEAVES_CANVAS),
    c("compile.epub", "export-as-epub-ellipsis").with(Flags::LEAVES_CANVAS),
    c("compile.html", "export-as-html-ellipsis").with(Flags::LEAVES_CANVAS),
    c("compile.rtf", "export-as-rtf-ellipsis").with(Flags::LEAVES_CANVAS),
    c("compile.md", "export-as-md-ellipsis").with(Flags::LEAVES_CANVAS),
    c("compile.txt", "export-as-txt-ellipsis").with(Flags::LEAVES_CANVAS),
    c("file.settings", "settings-ellipsis")
        .with(Flags::PALETTE)
        .with(Flags::LEAVES_CANVAS),
    c("file.leave", "back-to-the-hub").with(Flags::LEAVES_CANVAS),
    // The name 0.0.75 gave Back to the Hub: it still works where it is
    // saved (a script, a palette history), but it is listed nowhere.
    c("studio.leave", "back-to-the-hub")
        .with(Flags::HIDDEN)
        .with(Flags::LEAVES_CANVAS),
    // The shell's Quit key (`keys.quit`) runs `app.quit`.
    c("app.quit", "quit-sue").with(Flags::LEAVES_CANVAS),
];

/// Does what a File menu command does.
fn run(writer: &mut SueWriter, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let command = call.name;
    let sheet = writer.sheet(cx);
    match command {
        "file.new" => {
            writer.dialog = Some(Dialog::NewProject {
                template: "blank".to_owned(),
                title: tr!("untitled"),
            });
        }
        "file.save_template" => writer.save_as_template(cx),
        "file.templates_folder" => {
            if let Some(dir) = crate::templates::folder() {
                let _ = std::fs::create_dir_all(&dir);
                writer.templates = Templates::load();
                cx.request(ShellRequest::Command(format!("reveal:{}", dir.display())));
            }
        }
        "binder.new_doc" => writer.add_item(cx, false),
        "binder.new_folder" => writer.add_item(cx, true),
        "file.open" | "file.import" => {
            let pick = FilePick::open(Some(me()), "import", &tr!("import-a-document"))
                .extensions(&[
                    "docx", "dotx", "odt", "ott", "pdf", "md", "markdown", "txt", "html", "htm",
                    "fountain",
                ])
                .starting_in(SueWriter::export_dir(cx));
            cx.request(ShellRequest::PickFile(pick));
        }
        "compile.open" => {
            let choices = writer
                .last_compile
                .clone()
                .unwrap_or_else(|| CompileChoices {
                    format: 0,
                    titles: Titles::Folders,
                    separator: if sheet.key == "manuscript" {
                        Separator::SceneBreak
                    } else {
                        Separator::None
                    },
                    title_page: sheet.key == "manuscript",
                    chapter_pages: sheet.key != "standard",
                    whole: true,
                    text: crate::compile::system_text(),
                });
            writer.dialog = Some(Dialog::Compile(choices));
        }
        command
            if command.starts_with("compile.")
                && FORMATS
                    .iter()
                    .any(|(key, ..)| command == format!("compile.{key}")) =>
        {
            let format = FORMATS
                .iter()
                .position(|(key, ..)| command == format!("compile.{key}"))
                .unwrap_or(0);
            let mut choices = writer.last_compile.clone().unwrap_or(CompileChoices {
                format,
                titles: Titles::Folders,
                separator: Separator::None,
                title_page: false,
                chapter_pages: sheet.key != "standard",
                whole: true,
                text: crate::compile::system_text(),
            });
            choices.format = format;
            writer.ask_compile(cx, choices);
        }
        "file.leave" | "studio.leave" => cx.request(ShellRequest::Hub),
        // The project is Sue's: the shell saves it, and asks where for
        // Save As.
        "file.save" | "file.save_as" | "app.quit" => {
            writer.editor.commit(cx);
            cx.request(ShellRequest::Command(command.to_owned()));
        }
        "file.settings" => cx.request(ShellRequest::OpenSettings(Some("studios".to_owned()))),

        _ => {}
    }
}

/// Whether a File menu command works now.
fn state(writer: &SueWriter, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    let has_project = writer.project.is_some();
    let has_text = writer.editor.current().is_some();
    match call.name {
        "binder.new_doc" | "binder.new_folder" => {
            if has_project {
                ItemState::on()
            } else {
                ItemState::off()
            }
        }
        command if command.starts_with("compile.") => {
            if has_project || has_text {
                ItemState::on()
            } else {
                ItemState::off()
            }
        }
        _ => ItemState::on(),
    }
}
