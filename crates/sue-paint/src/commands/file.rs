//! The File menu's commands: canvases, saving, import and export,
//! printing, the settings dialogs, and leaving.
//!
//! Each command is one entry of [`COMMANDS`] with its own `run` and
//! `state`, its label key, default chords, icon and flags. Which of them
//! Auto Actions may record and which leave the canvas are also listed by
//! name in [`RECORDABLE`] and [`LEAVES_CANVAS`], which set those flags of
//! the table; a test holds the lists and the flags to each other.

use std::path::PathBuf;

use sue_core::Value;
use sue_core::txn::SetProperty;
use sue_wm::{
    AppletContext, Call, Command, Facts, FilePick, FileType, Flags, ItemState, ShellRequest,
};

use super::{doc_open, flag, when};
use crate::dialogs::{Dialog, PressureDialog};
use crate::doc::{self, NewPicture};
use crate::studio::{Clip, SuePaint, export_dir, me_id};

/// The commands of this group that Auto Actions may record: none. Saving,
/// exporting, importing and opening dialogs ask for files or settings a
/// recorded action cannot replay, and quitting is never a step of one.
pub(crate) const RECORDABLE: &[&str] = &[];

/// The commands of this group that close the canvas, print, or leave Sue
/// Paint, so a test that runs every command on a canvas skips them.
pub(crate) const LEAVES_CANVAS: &[&str] = &[
    "file.start_page",
    "file.close",
    "file.print",
    "file.leave",
    "app.quit",
];

// ----- How the items look ------------------------------------------------------

/// Checked while the canvas records its timelapse.
fn recording_state(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    paint.doc().map_or(ItemState::off(), |doc| {
        flag(
            facts
                .arena
                .get_by_id(doc.picture)
                .ok()
                .and_then(|entity| entity.property(crate::timelapse::RECORD))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        )
    })
}

/// Works while something is copied.
fn clipboard_state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.clipboard.is_some())
}

// ----- What they do --------------------------------------------------------------

/// Where a file dialog that opens something starts: the folder used last,
/// else the home folder.
fn open_dir(paint: &SuePaint, cx: &AppletContext<'_>) -> Option<PathBuf> {
    paint
        .last_dir
        .clone()
        .or_else(|| cx.catalog.folders.get("home").cloned())
}

/// The name of the canvas in front, or `fallback`.
fn picture_name(paint: &SuePaint, cx: &AppletContext<'_>, fallback: &str) -> String {
    paint
        .picture()
        .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
        .map_or_else(|| fallback.to_owned(), |entity| entity.name().to_owned())
}

/// The picture formats Sue saves, as a file dialog lists them.
fn picture_types() -> Vec<FileType> {
    sue_io::PICTURE_SAVE_TYPES
        .iter()
        .map(|(what, extensions)| FileType::new(what, extensions))
        .collect()
}

/// The launcher's Start Page is gone: the command, under the name
/// shortcuts saved, goes back to the Hub as File, Back to the Hub
/// (`file.leave`) does, and offers it in the palette.
fn start_page(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.dialog = None;
    cx.request(ShellRequest::Hub);
}

fn new(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.dialog = Some(Dialog::New(SuePaint::new_dialog(cx)));
}

/// A canvas the size of what was copied, with it pasted in.
fn new_from_clipboard(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(clip) = paint.clipboard.clone() else {
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
    paint.make_picture(cx, &spec);
    paint.clipboard = Some(Clip {
        raster: clip.raster.clone(),
        origin: [0, 0],
    });
    paint.paste(cx, &tr!("pasted"));
    paint.clipboard = Some(clip);
}

fn open(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.pick_image(cx, false);
}

fn clear_recent(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    for path in std::mem::take(&mut paint.recent_files) {
        cx.request(ShellRequest::Forget(path));
    }
}

/// Open Recent's entries: `file.recent.open.<n>` opens the n-th file.
fn open_recent(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let Some(path) = call
        .arg
        .parse::<usize>()
        .ok()
        .and_then(|index| paint.recent_files.get(index))
        .cloned()
    else {
        return;
    };
    if sue_platform::fs::exists(&path) {
        paint.open_picked(cx, &path, false);
    } else {
        cx.fail(tr!("is-gone", path = path.display()));
        cx.request(ShellRequest::Forget(path));
    }
}

fn close(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(picture) = paint.picture() {
        paint.closed.insert(picture);
        paint.docs.retain(|doc| doc.picture != picture);
        paint.active = paint.active.min(paint.docs.len().saturating_sub(1));
    }
}

/// Save: the project, or the file the canvas was opened from (see
/// `studio/saving.rs`).
fn save(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.save(cx);
}

/// File, Export Back to the Same .clip: writes the canvas into the
/// `.clip` it was opened from, in one step.
fn export_clip_back(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.export_clip_back(cx);
}

/// Works while the canvas in front came from a `.clip`.
fn clip_back_state(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(
        paint
            .picture()
            .is_some_and(|picture| paint.clip_paths.contains_key(&picture)),
    )
}

fn save_as(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.save_as(cx);
}

/// Save Duplicate, `.clip`: the canvas as a Clip Studio file.
fn duplicate_clip(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(picture) = paint.picture() {
        match crate::clip_io::encode(&cx.world.arena, picture) {
            Ok(bytes) => paint.export_bytes(cx, bytes, "clip"),
            Err(error) => cx.fail(tr!("export-failed", error = error)),
        }
    }
}

/// Export as a flat picture: the format is the last part of the name
/// (`file.export.png` writes a PNG).
fn export_flat(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let extension = call.name.rsplit('.').next().unwrap_or("png");
    paint.export(cx, extension);
}

fn export_psd(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.export_psd(cx, false);
}

fn export_psb(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.export_psd(cx, true);
}

fn revert(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.revert(cx);
}

fn import_image(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.pick_image(cx, true);
}

fn import_layers(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let start = open_dir(paint, cx);
    cx.request(ShellRequest::PickFile(
        FilePick::open(
            Some(me_id()),
            "import-layers",
            &tr!("import-image-as-separate-layers"),
        )
        .extensions(&[
            "clip", "psd", "psb", "png", "jpg", "jpeg", "jxl", "webp", "tif", "tiff", "bmp",
        ])
        .starting_in(start),
    ));
}

/// Clip Studio Paint EX's Batch import: every picture in a folder, one a
/// page.
fn import_batch(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let start = open_dir(paint, cx);
    cx.request(ShellRequest::PickFile(
        FilePick::open(Some(me_id()), "batch-import", &tr!("batch-import"))
            .folder()
            .starting_in(start),
    ));
}

/// Import or export an exposure sheet: a CSV of the timeline's cels.
fn exposure_sheet(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    if paint.timeline_here(&cx.world.arena).is_none() {
        cx.fail(tr!("make-timeline-first-animation-timeline-new"));
        return;
    }
    let types = vec![FileType::new(&tr!("exposure-sheet-csv"), &["csv"])];
    let pick = if call.name == "file.export.exposure_sheet" {
        let name = picture_name(paint, cx, &tr!("animation"));
        FilePick::save(
            Some(me_id()),
            "exposure-sheet-out",
            &tr!("exposure-sheet"),
            &format!("{name}.csv"),
        )
    } else {
        FilePick::open(Some(me_id()), "exposure-sheet-in", &tr!("exposure-sheet"))
    };
    cx.request(ShellRequest::PickFile(
        pick.types(types).starting_in(paint.last_dir.clone()),
    ));
}

/// Import a pattern, a vector drawing or a movie onto the canvas.
fn import_onto_canvas(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    if paint.picture().is_none() {
        cx.fail(tr!("open-make-canvas-first"));
        return;
    }
    let (purpose, title, extensions): (&str, String, &[&str]) = match call.name {
        "file.import.vector" => ("import-vector", tr!("import-vector-2"), &["svg"]),
        "file.import.movie" => (
            "import-movie",
            tr!("import-movie-2"),
            &["gif", "png", "apng", "webp", "avi"],
        ),
        _ => (
            "import-pattern",
            tr!("pattern-from-image"),
            &["png", "jpg", "jpeg", "jxl", "webp", "bmp", "tif", "tiff"],
        ),
    };
    let start = open_dir(paint, cx);
    cx.request(ShellRequest::PickFile(
        FilePick::open(Some(me_id()), purpose, &title)
            .extensions(extensions)
            .starting_in(start),
    ));
}

/// Import audio onto the canvas's timeline.
fn import_audio(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint
        .picture()
        .and_then(|picture| crate::anim::timeline(&cx.world.arena, picture))
        .is_none()
    {
        cx.fail(tr!("make-timeline-first-animation-timeline-new"));
        return;
    }
    let start = open_dir(paint, cx);
    cx.request(ShellRequest::PickFile(
        FilePick::open(Some(me_id()), "anim-audio", &tr!("import-audio-2"))
            .extensions(&["wav", "flac", "mp3", "ogg", "aif", "aiff", "m4a"])
            .starting_in(start),
    ));
}

fn import_3d(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let start = open_dir(paint, cx);
    cx.request(ShellRequest::PickFile(
        FilePick::open(Some(me_id()), "model3d", &tr!("import-3d-data"))
            .extensions(&["obj"])
            .starting_in(start),
    ));
}

/// File, Import, Pose Scanner: a photo poses the 3D drawing figure that
/// is picked, as the figure's own Pose Scanner button does. With no
/// figure picked, a new one is made first (one step that undoes), then
/// the photo is asked for.
fn import_pose_scanner(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let picked = paint.layer().is_some_and(|layer| {
        crate::figure::of_layer(&cx.world.arena, layer)
            .and_then(|model| cx.world.arena.get_by_id(model).ok())
            .is_some_and(|model| crate::figure::Body::of(model).kind == crate::figure::Kind::Body)
    });
    if !picked {
        let name = tr!("drawing-figure");
        paint.add_3d_layer(cx, &name, crate::figure::new_model(&name));
    }
    paint.pick_scan(cx, false);
}

/// Export an animation as a movie, an animated picture or a picture
/// sequence.
fn export_animation(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    paint.ask_export_animation(cx, call.name);
}

/// Export each animation cel as a picture.
fn export_cels(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let name = picture_name(paint, cx, &tr!("cel"));
    cx.request(ShellRequest::PickFile(
        FilePick::save(
            Some(me_id()),
            "anim-cels",
            &tr!("export-animation-cels"),
            &format!("{name}.png"),
        )
        .types(picture_types())
        .starting_in(paint.last_dir.clone()),
    ));
}

/// Batch export: a story's pages, else every open canvas.
fn export_batch(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.story_here(&cx.world.arena).is_some() {
        paint.story_command("story.export_pages", cx);
        return;
    }
    let start = paint.last_dir.clone().unwrap_or_else(|| export_dir(cx));
    cx.request(ShellRequest::PickFile(
        FilePick::save(
            Some(me_id()),
            "batch",
            &tr!("batch-export-every-open-canvas"),
            &tr!("canvas-png"),
        )
        .types(picture_types())
        .starting_in(Some(start)),
    ));
}

fn export_book(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    paint.ask_export_book(cx, call.name == "file.export.epub");
}

fn export_webtoon(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.picture().is_some() {
        paint.dialog = Some(Dialog::Webtoon(paint.webtoon.0, paint.webtoon.1));
    }
}

/// Clip Studio Paint's Export vectors: the vector layers' lines as an
/// SVG drawing.
fn export_vectors(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.picture().is_none() {
        return;
    }
    let name = picture_name(paint, cx, &tr!("picture"));
    cx.request(ShellRequest::PickFile(
        FilePick::save(
            Some(me_id()),
            "vectors",
            &tr!("export-vectors"),
            &format!("{name}.svg"),
        )
        .types(vec![FileType::new(&tr!("svg-drawing"), &["svg"])])
        .starting_in(paint.last_dir.clone()),
    ));
}

/// Clip Studio Paint's Export layer comp: each comp a picture.
fn export_layer_comps(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(picture) = paint.picture() else {
        return;
    };
    if crate::comps::read(&cx.world.arena, picture).is_empty() {
        cx.notify(tr!("no-layer-comps-yet"));
        return;
    }
    let name = picture_name(paint, cx, &tr!("picture"));
    cx.request(ShellRequest::PickFile(
        FilePick::save(
            Some(me_id()),
            "layer-comps",
            &tr!("export-layer-comps"),
            &format!("{name}.png"),
        )
        .types(picture_types())
        .starting_in(paint.last_dir.clone()),
    ));
}

/// Record Timelapse: turns the canvas's recording on or off, a step that
/// undoes.
fn record_timelapse(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(picture) = paint.picture() else {
        return;
    };
    let on = paint.recording(&cx.world.arena, picture);
    let edit = doc::Edit::new(if on {
        tr!("stop-recording-timelapse")
    } else {
        tr!("record-timelapse-2")
    })
    .then(SetProperty::new(picture, crate::timelapse::RECORD, !on));
    SuePaint::apply(cx, edit);
}

fn export_timelapse(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.picture().is_some() {
        paint.dialog = Some(Dialog::Timelapse(paint.timelapse_export));
    }
}

fn print_settings(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.dialog = Some(Dialog::Print(paint.print, false));
}

fn print(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.dialog = Some(Dialog::Print(paint.print, true));
}

/// Sue Paint's preferences are its page of Settings.
fn preferences(_: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    cx.request(ShellRequest::OpenSettings(Some(
        crate::studio::ID.to_owned(),
    )));
}

fn shortcuts(_: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    cx.request(ShellRequest::OpenSettings(Some("keymaps".to_owned())));
}

fn modifiers(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let rice = cx.rice();
    let chosen = crate::dialogs::MODIFIERS.map(|(key, _, choices)| {
        let now = rice.get_str(key).unwrap_or(choices[0]);
        choices
            .iter()
            .position(|choice| *choice == now)
            .unwrap_or(0)
    });
    paint.dialog = Some(Dialog::Modifiers(chosen));
}

fn command_bar(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.panels.command_bar = true;
    paint.dialog = Some(Dialog::CommandBar(crate::dialogs::BarDialog::new(
        crate::dialogs::BarTarget::CommandBar,
    )));
}

/// Manage Fonts: Sue's shared font list, the one every font menu of Sue
/// shows.
fn manage_fonts(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.dialog = Some(Dialog::Fonts(sue_wm::fonts::FontManager::open()));
}

fn pressure(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.dialog = Some(Dialog::Pressure(PressureDialog::new(paint.pen_curve)));
}

fn leave(_: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    cx.request(ShellRequest::Hub);
}

fn quit(_: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    cx.request(ShellRequest::Command("app.quit".to_owned()));
}

// ----- The table -----------------------------------------------------------------

/// `true` if `names` holds `name` (a `const` look-up, so the lists above
/// set the flags of the table below).
const fn listed(names: &[&str], name: &str) -> bool {
    let mut index = 0;
    while index < names.len() {
        let (a, b) = (names[index].as_bytes(), name.as_bytes());
        if a.len() == b.len() {
            let mut at = 0;
            while at < a.len() && a[at] == b[at] {
                at += 1;
            }
            if at == a.len() {
                return true;
            }
        }
        index += 1;
    }
    false
}

/// The flags [`RECORDABLE`] and [`LEAVES_CANVAS`] give `name`.
const fn listed_flags(name: &str) -> Flags {
    let recordable = if listed(RECORDABLE, name) {
        Flags::RECORDABLE
    } else {
        Flags::NONE
    };
    let leaves = if listed(LEAVES_CANVAS, name) {
        Flags::LEAVES_CANVAS
    } else {
        Flags::NONE
    };
    recordable.with(leaves)
}

/// A command of this group that always works, with its icon, and the
/// flags its name has in the lists.
const fn on(
    name: &'static str,
    label: &'static str,
    icon: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
) -> Command<SuePaint> {
    Command::new(name, label, "file", run)
        .icon(icon)
        .with(listed_flags(name))
}

/// A command of this group that works while a canvas is open.
const fn c(
    name: &'static str,
    label: &'static str,
    icon: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
) -> Command<SuePaint> {
    on(name, label, icon, run).state(doc_open)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    on("file.start_page", "back-to-the-hub", "home", start_page).with(Flags::PALETTE),
    on("file.new", "new-ellipsis", "file-new", new)
        .keys("Ctrl+N")
        .with(Flags::PALETTE),
    c(
        "file.new_clipboard",
        "create-new-from-clipboard",
        "paste",
        new_from_clipboard,
    )
    .state(clipboard_state),
    on("file.open", "open-picture-ellipsis", "file-open", open).with(Flags::PALETTE),
    c(
        "file.recent.clear",
        "clear-recent-files",
        "recent",
        clear_recent,
    ),
    c("file.close", "close", "close", close).keys("Ctrl+W"),
    on("file.save", "save", "save", save),
    on("file.save_as", "save-as-ellipsis", "save-as", save_as)
        .keys("Ctrl+Shift+S or Ctrl+Alt+S or Alt+Shift+S"),
    c(
        "file.duplicate.clip",
        "clip-clip-studio-format-ellipsis",
        "duplicate",
        duplicate_clip,
    ),
    c("file.export.bmp", "bmp-bmp-ellipsis", "export", export_flat),
    c(
        "file.export.jpg",
        "jpg-jpeg-ellipsis",
        "export",
        export_flat,
    ),
    c(
        "file.export.png",
        "export-as-png-ellipsis",
        "export",
        export_flat,
    )
    .with(Flags::PALETTE),
    c(
        "file.export.tif",
        "tif-tiff-ellipsis",
        "export",
        export_flat,
    ),
    c(
        "file.export.tga",
        "tga-targa-ellipsis",
        "export",
        export_flat,
    ),
    c(
        "file.export.webp",
        "webp-webp-ellipsis",
        "export",
        export_flat,
    ),
    c(
        "file.export.psb",
        "psb-photoshop-big-ellipsis",
        "export",
        export_psb,
    ),
    c(
        "file.export.psd",
        "psd-photoshop-document-ellipsis",
        "export",
        export_psd,
    ),
    c(
        "file.export.jxl",
        "jxl-jpeg-xl-ellipsis",
        "export",
        export_flat,
    ),
    c(
        "file.export.clip_back",
        "export-back-to-the-same-clip",
        "export",
        export_clip_back,
    )
    .state(clip_back_state),
    c("file.revert", "revert", "reload", revert),
    c(
        "file.import.image",
        "image-ellipsis",
        "import",
        import_image,
    ),
    c(
        "file.import.layers",
        "image-as-separate-layers-ellipsis",
        "layers",
        import_layers,
    ),
    c(
        "file.import.batch",
        "batch-import-ellipsis",
        "folder-open",
        import_batch,
    ),
    c(
        "file.import.exposure_sheet",
        "exposure-sheet-ellipsis",
        "table",
        exposure_sheet,
    ),
    c(
        "file.import.pattern",
        "pattern-from-image-ellipsis",
        "material-pattern",
        import_onto_canvas,
    ),
    c(
        "file.import.vector",
        "vector-ellipsis",
        "draw",
        import_onto_canvas,
    ),
    c("file.import.audio", "audio-ellipsis", "audio", import_audio),
    c(
        "file.import.movie",
        "movie-ellipsis",
        "film",
        import_onto_canvas,
    ),
    c("file.import.3d", "n-3d-data-ellipsis", "cube", import_3d),
    c(
        "file.export.gif",
        "animated-gif-ellipsis",
        "film",
        export_animation,
    ),
    c(
        "file.export.apng",
        "animated-sticker-apng-ellipsis",
        "film",
        export_animation,
    ),
    c(
        "file.export.webp_animation",
        "animated-webp-ellipsis",
        "film",
        export_animation,
    ),
    c(
        "file.export.exposure_sheet",
        "exposure-sheet-ellipsis",
        "table",
        exposure_sheet,
    ),
    c(
        "file.export.movie",
        "movie-ellipsis",
        "film",
        export_animation,
    ),
    c(
        "file.export.sequence",
        "image-sequence-ellipsis",
        "picture-list",
        export_animation,
    ),
    c(
        "file.export.cels",
        "animation-cels-ellipsis",
        "cel",
        export_cels,
    ),
    c(
        "file.export.batch",
        "batch-export-ellipsis",
        "export",
        export_batch,
    ),
    c(
        "file.export.pdf",
        "export-as-pdf-ellipsis",
        "export-pdf",
        export_book,
    ),
    c(
        "file.export.epub",
        "export-as-epub-ellipsis",
        "book",
        export_book,
    ),
    c(
        "file.export.webtoon",
        "export-webtoon-ellipsis",
        "doc-webtoon",
        export_webtoon,
    ),
    c(
        "file.export.vectors",
        "export-vectors-ellipsis",
        "export",
        export_vectors,
    ),
    c(
        "file.export.layer_comps",
        "export-layer-comps-ellipsis",
        "layers",
        export_layer_comps,
    ),
    c(
        "file.timelapse.record",
        "record-timelapse-2",
        "history",
        record_timelapse,
    )
    .state(recording_state),
    c(
        "file.timelapse.export",
        "export-timelapse-ellipsis",
        "film",
        export_timelapse,
    ),
    c(
        "file.print_settings",
        "print-settings-ellipsis",
        "settings",
        print_settings,
    ),
    c("file.print", "print-ellipsis", "print", print).keys("Ctrl+P"),
    on(
        "file.preferences",
        "preferences-ellipsis",
        "settings",
        preferences,
    )
    .keys("Ctrl+K"),
    on(
        "file.shortcuts",
        "shortcut-settings-ellipsis",
        "keyboard",
        shortcuts,
    )
    .keys("Ctrl+Alt+Shift+K"),
    c(
        "file.modifiers",
        "modifier-key-settings-ellipsis",
        "keyboard",
        modifiers,
    )
    .keys("Ctrl+Alt+Shift+Y"),
    c(
        "file.command_bar",
        "command-bar-settings-ellipsis",
        "palette-command-bar",
        command_bar,
    ),
    on(
        "file.pressure",
        "pen-pressure-settings-ellipsis",
        "pressure-curve",
        pressure,
    ),
    on("file.leave", "back-to-the-hub", "home", leave),
    on("app.quit", "quit-sue", "window-close", quit),
    Command::family("file.recent.open.", "open-recent", "file", open_recent)
        .icon("recent")
        .state(doc_open),
    // Added for the tutorials (gap CC-12, CT-35). P3-2 puts them in the
    // File menu (Manage Fonts after Modifier Key Settings, Pose Scanner
    // under Import); until then the command palette offers them.
    on(
        "file.fonts",
        "manage-fonts-ellipsis",
        "tool-text",
        manage_fonts,
    )
    .with(Flags::PALETTE),
    c(
        "file.import.pose_scanner",
        "pose-scanner-from-image-ellipsis",
        "material-pose",
        import_pose_scanner,
    )
    .with(Flags::PALETTE),
];

#[cfg(test)]
mod tests {
    use sue_wm::Flags;
    use sue_wm::icons::Icon;

    use super::{COMMANDS, LEAVES_CANVAS, RECORDABLE};

    #[test]
    fn every_entry_has_an_icon_of_the_shared_set() {
        for command in COMMANDS {
            assert_eq!(command.group, "file", "{}", command.name);
            assert!(
                Icon::from_name(command.icon).is_some(),
                "{}: no icon is called {:?}",
                command.name,
                command.icon
            );
        }
    }

    #[test]
    fn the_lists_and_the_flags_say_the_same() {
        for command in COMMANDS {
            assert_eq!(
                command.flags.has(Flags::RECORDABLE),
                RECORDABLE.contains(&command.name),
                "{} recordable",
                command.name
            );
            assert_eq!(
                command.flags.has(Flags::LEAVES_CANVAS),
                LEAVES_CANVAS.contains(&command.name),
                "{} leaves the canvas",
                command.name
            );
        }
        for name in RECORDABLE.iter().chain(LEAVES_CANVAS) {
            assert!(
                COMMANDS.iter().any(|command| command.name == *name),
                "{name} is not a File command"
            );
        }
        // Auto Actions never record leaving Sue: a played action that
        // quits would end the session it runs in.
        assert!(!RECORDABLE.contains(&"app.quit"));
    }

    #[test]
    fn the_names_0_0_75_saved_are_all_still_here() {
        // Command names are saved data (shortcut settings, Auto Actions,
        // custom touch keys): none of these may go without a migration.
        let names: Vec<&str> = COMMANDS.iter().map(|command| command.name).collect();
        for name in [
            "file.start_page",
            "file.new",
            "file.new_clipboard",
            "file.open",
            "file.recent.clear",
            "file.close",
            "file.save",
            "file.save_as",
            "file.duplicate.clip",
            "file.export.bmp",
            "file.export.jpg",
            "file.export.png",
            "file.export.tif",
            "file.export.tga",
            "file.export.webp",
            "file.export.psb",
            "file.export.psd",
            "file.export.jxl",
            "file.revert",
            "file.import.image",
            "file.import.layers",
            "file.import.batch",
            "file.import.exposure_sheet",
            "file.import.pattern",
            "file.import.vector",
            "file.import.audio",
            "file.import.movie",
            "file.import.3d",
            "file.export.gif",
            "file.export.apng",
            "file.export.webp_animation",
            "file.export.exposure_sheet",
            "file.export.movie",
            "file.export.sequence",
            "file.export.cels",
            "file.export.batch",
            "file.export.pdf",
            "file.export.epub",
            "file.export.webtoon",
            "file.export.vectors",
            "file.export.layer_comps",
            "file.timelapse.record",
            "file.timelapse.export",
            "file.print_settings",
            "file.print",
            "file.preferences",
            "file.shortcuts",
            "file.modifiers",
            "file.command_bar",
            "file.pressure",
            "file.leave",
            "app.quit",
            "file.recent.open.",
        ] {
            assert!(names.contains(&name), "{name} is gone");
        }
    }

    #[test]
    fn the_flat_exports_write_the_format_their_name_ends_in() {
        for name in [
            "file.export.bmp",
            "file.export.jpg",
            "file.export.png",
            "file.export.tif",
            "file.export.tga",
            "file.export.webp",
            "file.export.jxl",
        ] {
            let extension = name.rsplit('.').next().unwrap_or_default();
            assert!(
                sue_io::PICTURE_SAVE_TYPES
                    .iter()
                    .any(|(_, extensions)| extensions.contains(&extension)),
                "{name}: Sue does not save .{extension}"
            );
        }
    }
}
