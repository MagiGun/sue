//! The File menu: projects, documents, templates, import and export.

use sue_wm::{Command, Flags};

use crate::studio::SueWriter;

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    super::old(name, label, "file")
}

pub(super) const COMMANDS: &[Command<SueWriter>] = &[
    c("file.new", "new-project-ellipsis").with(Flags::PALETTE),
    // The shell's Open key (`keys.open_file`) runs `file.open`.
    c("file.open", "import-a-document-ellipsis").with(Flags::LEAVES_CANVAS),
    c("file.import", "import-a-document-ellipsis")
        .with(Flags::PALETTE)
        .with(Flags::LEAVES_CANVAS),
    c("binder.new_doc", "new-document").keys("Ctrl+N"),
    c("binder.new_folder", "new-folder").keys("Ctrl+Shift+N"),
    c("file.save_template", "save-as-template"),
    c("file.templates_folder", "templates-folder").with(Flags::LEAVES_CANVAS),
    c("compile.open", "compile-ellipsis")
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
    c("studio.leave", "back-to-the-hub").with(Flags::LEAVES_CANVAS),
];
