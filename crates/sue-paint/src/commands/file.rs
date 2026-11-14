//! The File menu's commands: canvases, saving, import and export,
//! printing, the settings dialogs, and leaving.

use sue_wm::{Command, Flags};

use crate::studio::SuePaint;

/// A command of this group, run and grayed as the studio always did.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "file", super::run).state(super::state)
}

/// A family of commands: every name that goes on from `prefix`.
const fn family(prefix: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::family(prefix, label, "file", super::run).state(super::state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("file.start_page", "start-page").with(Flags::PALETTE),
    c("file.new", "new-ellipsis")
        .keys("Ctrl+N")
        .with(Flags::PALETTE),
    c("file.new_clipboard", "create-new-from-clipboard"),
    c("file.open", "open-ellipsis").with(Flags::PALETTE),
    c("file.recent.clear", "clear-recent-files"),
    c("file.close", "close")
        .keys("Ctrl+W")
        .with(Flags::LEAVES_CANVAS),
    c("file.save", "save"),
    c("file.save_as", "save-as-ellipsis").keys("Ctrl+Shift+S or Ctrl+Alt+S or Alt+Shift+S"),
    c("file.duplicate.clip", "clip-clip-studio-format-ellipsis"),
    c("file.export.bmp", "bmp-bmp-ellipsis"),
    c("file.export.jpg", "jpg-jpeg-ellipsis"),
    c("file.export.png", "export-as-png-ellipsis").with(Flags::PALETTE),
    c("file.export.tif", "tif-tiff-ellipsis"),
    c("file.export.tga", "tga-targa-ellipsis"),
    c("file.export.webp", "webp-webp-ellipsis"),
    c("file.export.psb", "psb-photoshop-big-ellipsis"),
    c("file.export.psd", "psd-photoshop-document-ellipsis"),
    c("file.export.jxl", "jxl-jpeg-xl-ellipsis"),
    c("file.revert", "revert"),
    c("file.import.image", "image-ellipsis"),
    c("file.import.layers", "image-as-separate-layers-ellipsis"),
    c("file.import.batch", "batch-import-ellipsis"),
    c("file.import.exposure_sheet", "exposure-sheet-ellipsis"),
    c("file.import.pattern", "pattern-from-image-ellipsis"),
    c("file.import.vector", "vector-ellipsis"),
    c("file.import.audio", "audio-ellipsis"),
    c("file.import.movie", "movie-ellipsis"),
    c("file.import.3d", "n-3d-data-ellipsis"),
    c("file.export.gif", "animated-gif-ellipsis"),
    c("file.export.apng", "animated-sticker-apng-ellipsis"),
    c("file.export.webp_animation", "animated-webp-ellipsis"),
    c("file.export.exposure_sheet", "exposure-sheet-ellipsis"),
    c("file.export.movie", "movie-ellipsis"),
    c("file.export.sequence", "image-sequence-ellipsis"),
    c("file.export.cels", "animation-cels-ellipsis"),
    c("file.export.batch", "batch-export-ellipsis"),
    c("file.export.pdf", "export-as-pdf-ellipsis"),
    c("file.export.epub", "export-as-epub-ellipsis"),
    c("file.export.webtoon", "export-webtoon-ellipsis"),
    c("file.export.vectors", "export-vectors-ellipsis"),
    c("file.export.layer_comps", "export-layer-comps-ellipsis"),
    c("file.timelapse.record", "record-timelapse-2"),
    c("file.timelapse.export", "export-timelapse-ellipsis"),
    c("file.print_settings", "print-settings-ellipsis"),
    c("file.print", "print-ellipsis")
        .keys("Ctrl+P")
        .with(Flags::LEAVES_CANVAS),
    c("file.preferences", "preferences-ellipsis").keys("Ctrl+K"),
    c("file.shortcuts", "shortcut-settings-ellipsis"),
    c("file.modifiers", "modifier-key-settings-ellipsis"),
    c("file.command_bar", "command-bar-settings-ellipsis"),
    c("file.pressure", "pen-pressure-settings-ellipsis"),
    c("file.leave", "back-to-the-hub").with(Flags::LEAVES_CANVAS),
    c("app.quit", "quit-sue").with(Flags::LEAVES_CANVAS),
    family("file.recent.open.", "open-recent"),
];
