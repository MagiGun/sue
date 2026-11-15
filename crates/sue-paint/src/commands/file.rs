//! The File menu's commands: canvases, saving, import and export,
//! printing, the settings dialogs, and leaving.

use sue_core::txn::SetProperty;
use sue_core::{Arena, UndoStack, Value};
use sue_wm::{
    AppletContext, Call, Command, Facts, FilePick, FileType, Flags, ItemState, ShellRequest, World,
};

use super::{Basis, flag, when};
use crate::dialogs::{Dialog, PressureDialog};
use crate::doc::{self, NewPicture};
use crate::studio::{Clip, SuePaint, export_dir, me_id};

/// Does what a File command does.
pub(super) fn run(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    paint.run_file(call.name, cx);
}

/// How an item for a File command looks right now.
pub(super) fn state(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    paint.state_file(call.name, facts.arena, facts.history)
}

impl SuePaint {
    /// How an item for a File command looks right now.
    fn state_file(&self, command: &str, arena: &Arena, _history: &UndoStack<World>) -> ItemState {
        let Basis { has_doc, .. } = self.basis(arena);
        match command {
            "file.new" | "file.open" | "file.save" | "file.save_as" | "file.preferences"
            | "file.shortcuts" | "file.pressure" | "file.leave" | "file.start_page"
            | "app.quit" => ItemState::on(),
            "file.timelapse.record" => self.doc().map_or(ItemState::off(), |doc| {
                flag(
                    arena
                        .get_by_id(doc.picture)
                        .ok()
                        .and_then(|entity| entity.property(crate::timelapse::RECORD))
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                )
            }),
            "file.new_clipboard" => when(self.clipboard.is_some()),
            _ => when(has_doc),
        }
    }

    /// Does what a File command does.
    #[allow(clippy::too_many_lines, reason = "one arm per command")]
    fn run_file(&mut self, command: &str, cx: &mut AppletContext<'_>) {
        match command {
            "file.new" => self.dialog = Some(Dialog::New(Self::new_dialog(cx))),
            "file.open" => self.pick_image(cx, false),
            "file.import.image" => self.pick_image(cx, true),
            "file.close" => {
                if let Some(picture) = self.picture() {
                    self.closed.insert(picture);
                    self.docs.retain(|doc| doc.picture != picture);
                    self.active = self.active.min(self.docs.len().saturating_sub(1));
                }
            }
            "file.save" if crate::clip_io::saves_clip(cx) && self.picture().is_some() => {
                self.save_clip(cx);
            }
            "file.save" => {
                cx.request(ShellRequest::Command("file.save".to_owned()));
                self.mark_clean();
                let at = cx.history.cursor();
                for doc in &self.docs {
                    self.saved_at.insert(doc.picture, at);
                }
            }
            "file.revert" => self.revert(cx),
            // The launcher's Start Page is gone: the command, under the
            // name shortcuts saved, goes back to the Hub as File, Back to
            // the Hub (`file.leave`) does, and offers it in the palette.
            "file.start_page" => {
                self.dialog = None;
                cx.request(ShellRequest::Hub);
            }
            "file.recent.clear" => {
                for path in std::mem::take(&mut self.recent_files) {
                    cx.request(ShellRequest::Forget(path));
                }
            }
            recent if recent.starts_with("file.recent.open.") => {
                let index = recent["file.recent.open.".len()..].parse::<usize>().ok();
                if let Some(path) = index
                    .and_then(|index| self.recent_files.get(index))
                    .cloned()
                {
                    if sue_platform::fs::exists(&path) {
                        self.open_picked(cx, &path, false);
                    } else {
                        cx.fail(tr!("is-gone", path = path.display()));
                        cx.request(ShellRequest::Forget(path));
                    }
                }
            }
            "file.import.vector" | "file.import.movie" | "file.import.pattern" => {
                if self.picture().is_none() {
                    cx.fail(tr!("open-make-canvas-first"));
                    return;
                }
                let (purpose, title, extensions): (&str, &str, &[&str]) = match command {
                    "file.import.vector" => ("import-vector", &tr!("import-vector-2"), &["svg"]),
                    "file.import.movie" => (
                        "import-movie",
                        &tr!("import-movie-2"),
                        &["gif", "png", "apng", "webp", "avi"],
                    ),
                    _ => (
                        "import-pattern",
                        &tr!("pattern-from-image"),
                        &["png", "jpg", "jpeg", "jxl", "webp", "bmp", "tif", "tiff"],
                    ),
                };
                let start = self
                    .last_dir
                    .clone()
                    .or_else(|| cx.catalog.folders.get("home").cloned());
                cx.request(ShellRequest::PickFile(
                    FilePick::open(Some(me_id()), purpose, title)
                        .extensions(extensions)
                        .starting_in(start),
                ));
            }
            "file.print_settings" => self.dialog = Some(Dialog::Print(self.print, false)),
            "file.print" => self.dialog = Some(Dialog::Print(self.print, true)),
            "file.export.batch" => {
                if self.story_here(&cx.world.arena).is_some() {
                    self.story_command("story.export_pages", cx);
                } else {
                    let start = self.last_dir.clone().unwrap_or_else(|| export_dir(cx));
                    cx.request(ShellRequest::PickFile(
                        FilePick::save(
                            Some(me_id()),
                            "batch",
                            &tr!("batch-export-every-open-canvas"),
                            &tr!("canvas-png"),
                        )
                        .types(
                            sue_io::PICTURE_SAVE_TYPES
                                .iter()
                                .map(|(what, extensions)| FileType::new(what, extensions))
                                .collect(),
                        )
                        .starting_in(Some(start)),
                    ));
                }
            }
            "file.modifiers" => {
                let rice = cx.rice();
                let chosen = crate::dialogs::MODIFIERS.map(|(key, _, choices)| {
                    let now = rice.get_str(key).unwrap_or(choices[0]);
                    choices
                        .iter()
                        .position(|choice| *choice == now)
                        .unwrap_or(0)
                });
                self.dialog = Some(Dialog::Modifiers(chosen));
            }
            "file.command_bar" => {
                self.panels.command_bar = true;
                self.dialog = Some(Dialog::CommandBar(String::new()));
            }
            "file.export.bmp" => self.export(cx, "bmp"),
            "file.export.webp" => self.export(cx, "webp"),
            "file.duplicate.clip" => {
                if let Some(picture) = self.picture() {
                    match crate::clip_io::encode(&cx.world.arena, picture) {
                        Ok(bytes) => self.export_bytes(cx, bytes, "clip"),
                        Err(error) => cx.fail(tr!("export-failed", error = error)),
                    }
                }
            }
            "file.export.tif" => self.export(cx, "tif"),
            "file.export.tga" => self.export(cx, "tga"),
            "file.export.psd" => self.export_psd(cx, false),
            "file.export.psb" => self.export_psd(cx, true),
            // Clip Studio Paint EX's Batch import: every picture in a folder,
            // one a page.
            "file.import.batch" => {
                let start = self
                    .last_dir
                    .clone()
                    .or_else(|| cx.catalog.folders.get("home").cloned());
                cx.request(ShellRequest::PickFile(
                    FilePick::open(Some(me_id()), "batch-import", &tr!("batch-import"))
                        .folder()
                        .starting_in(start),
                ));
            }
            "file.import.layers" => {
                let start = self
                    .last_dir
                    .clone()
                    .or_else(|| cx.catalog.folders.get("home").cloned());
                cx.request(ShellRequest::PickFile(
                    FilePick::open(
                        Some(me_id()),
                        "import-layers",
                        &tr!("import-image-as-separate-layers"),
                    )
                    .extensions(&[
                        "clip", "psd", "psb", "png", "jpg", "jpeg", "jxl", "webp", "tif", "tiff",
                        "bmp",
                    ])
                    .starting_in(start),
                ));
            }
            "file.save_as" => self.save_as(cx),
            "file.export.png" => self.export(cx, "png"),
            "file.export.jpg" => self.export(cx, "jpg"),
            "file.export.jxl" => self.export(cx, "jxl"),
            "file.preferences" => {
                self.dialog = Some(Dialog::Preferences(crate::prefs::Preferences::read(
                    cx.rice(),
                )));
            }
            "file.shortcuts" => cx.request(ShellRequest::OpenSettings(Some("keymaps".to_owned()))),
            "file.pressure" => {
                self.dialog = Some(Dialog::Pressure(PressureDialog::new(self.pen_curve)));
            }
            "file.leave" => cx.request(ShellRequest::Hub),
            "app.quit" => cx.request(ShellRequest::Command("app.quit".to_owned())),
            // A canvas the size of what was copied, with it pasted in.
            "file.new_clipboard" => {
                let Some(clip) = self.clipboard.clone() else {
                    return;
                };
                let spec = NewPicture {
                    name: tr!("illustration"),
                    width: clip.raster.width(),
                    height: clip.raster.height(),
                    dpi: 72.0,
                    paper: Some(sue_core::Color::srgb8(255, 255, 255)),
                    timelapse: false,
                };
                self.make_picture(cx, &spec);
                self.clipboard = Some(Clip {
                    raster: clip.raster.clone(),
                    origin: [0, 0],
                });
                self.paste(cx, &tr!("pasted"));
                self.clipboard = Some(clip);
            }
            "file.timelapse.record" => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let on = self.recording(&cx.world.arena, picture);
                let edit = doc::Edit::new(if on {
                    tr!("stop-recording-timelapse")
                } else {
                    tr!("record-timelapse-2")
                })
                .then(SetProperty::new(picture, crate::timelapse::RECORD, !on));
                Self::apply(cx, edit);
            }
            "file.timelapse.export" => {
                if self.picture().is_some() {
                    self.dialog = Some(Dialog::Timelapse(self.timelapse_export));
                }
            }
            "file.export.pdf" | "file.export.epub" => {
                self.ask_export_book(cx, command == "file.export.epub")
            }

            "file.export.layer_comps" => {
                // Clip Studio Paint's Export layer comp: each comp a picture.
                let Some(picture) = self.picture() else {
                    return;
                };
                if crate::comps::read(&cx.world.arena, picture).is_empty() {
                    cx.notify(tr!("no-layer-comps-yet"));
                    return;
                }
                let name = cx
                    .world
                    .arena
                    .get_by_id(picture)
                    .map_or_else(|_| tr!("picture"), |entity| entity.name().to_owned());
                cx.request(ShellRequest::PickFile(
                    FilePick::save(
                        Some(me_id()),
                        "layer-comps",
                        &tr!("export-layer-comps"),
                        &format!("{name}.png"),
                    )
                    .types(
                        sue_io::PICTURE_SAVE_TYPES
                            .iter()
                            .map(|(what, extensions)| sue_wm::FileType::new(what, extensions))
                            .collect(),
                    )
                    .starting_in(self.last_dir.clone()),
                ));
            }
            "file.export.vectors" => {
                // Clip Studio Paint's Export vectors: the vector layers'
                // lines as an SVG drawing.
                let Some(picture) = self.picture() else {
                    return;
                };
                let name = cx
                    .world
                    .arena
                    .get_by_id(picture)
                    .map_or_else(|_| tr!("picture"), |entity| entity.name().to_owned());
                cx.request(ShellRequest::PickFile(
                    FilePick::save(
                        Some(me_id()),
                        "vectors",
                        &tr!("export-vectors"),
                        &format!("{name}.svg"),
                    )
                    .types(vec![sue_wm::FileType::new(&tr!("svg-drawing"), &["svg"])])
                    .starting_in(self.last_dir.clone()),
                ));
            }
            "file.export.webtoon" => {
                if self.picture().is_some() {
                    self.dialog = Some(Dialog::Webtoon(self.webtoon.0, self.webtoon.1));
                }
            }
            "file.import.audio" => {
                if self
                    .picture()
                    .and_then(|picture| crate::anim::timeline(&cx.world.arena, picture))
                    .is_none()
                {
                    cx.fail(tr!("make-timeline-first-animation-timeline-new"));
                    return;
                }
                let start = self
                    .last_dir
                    .clone()
                    .or_else(|| cx.catalog.folders.get("home").cloned());
                cx.request(ShellRequest::PickFile(
                    FilePick::open(Some(me_id()), "anim-audio", &tr!("import-audio-2"))
                        .extensions(&["wav", "flac", "mp3", "ogg", "aif", "aiff", "m4a"])
                        .starting_in(start),
                ));
            }
            "file.import.3d" => {
                let start = self
                    .last_dir
                    .clone()
                    .or_else(|| cx.catalog.folders.get("home").cloned());
                cx.request(ShellRequest::PickFile(
                    FilePick::open(Some(me_id()), "model3d", &tr!("import-3d-data"))
                        .extensions(&["obj"])
                        .starting_in(start),
                ));
            }
            _ => {}
        }
    }
}

/// A command of this group.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "file", run).state(state)
}

/// A family of commands: every name that goes on from `prefix`.
const fn family(prefix: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::family(prefix, label, "file", run).state(state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("file.start_page", "back-to-the-hub")
        .with(Flags::PALETTE)
        .with(Flags::LEAVES_CANVAS),
    c("file.new", "new-ellipsis")
        .keys("Ctrl+N")
        .with(Flags::PALETTE),
    c("file.new_clipboard", "create-new-from-clipboard"),
    c("file.open", "open-picture-ellipsis").with(Flags::PALETTE),
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
