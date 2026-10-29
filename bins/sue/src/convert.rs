//! Convert: turns any file into another kind. Pictures become PNG, JPEG,
//! JPEG XL, WebP, TIFF, BMP, GIF, TGA, QOI, ICO or PPM, resized, turned,
//! mirrored and adjusted on the way; sounds become FLAC, WAV or AIFF at any
//! rate, depth and channel count, trimmed, faded, sped up or reversed; text
//! is tidied, Markdown becomes HTML and tables move between CSV, TSV and
//! JSON; and anything becomes Base64, a hex dump or a byte map picture.
//!
//! It is a tool: it opens from the Hub or the Sue menu into a page of its
//! own, with its menus in Sue's top bar. Files come from Add Files or Add
//! a Folder (the system's dialogs), from the Sue menu's Send To in any
//! studio or tool, or from the project's own pictures and sounds. They
//! convert on a background thread, next to each file or into a chosen
//! folder, and the list says where each one went.

/// Text from the language database's `convert` namespace (the Converter).
macro_rules! tr {
    ($($arg:tt)*) => { sue_lang::tr!("convert", $($arg)*) };
}

mod ops;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use egui::{Align, Align2, Layout, Rect, RichText, ScrollArea, Sense, Ui, pos2, vec2};
use sue_core::{AssetHandle, ComponentKind};
use sue_io::{SampleDepth, meta};
use sue_platform::write_atomic;
use sue_wm::{
    AppletContext, AppletId, AppletManifest, AppletRole, FilePick, ItemState, Menu, MenuBar,
    MenuItem, ShellRequest, SueApplet, Theme, Tick, widgets,
};

use self::ops::{Endings, Filter, Kind, Options, Resize, Target};
use crate::util::{file_name, free_path, human_bytes};

/// The applet id.
pub const ID: &str = "convert";

/// Most files one Add Files adds from a folder.
const FOLDER_LIMIT: usize = 500;

/// Where a file to convert comes from.
#[derive(Clone, Debug, PartialEq)]
enum Source {
    /// A file on disk.
    File(PathBuf),
    /// A picture or sound in the project with no file behind it.
    Entity(AssetHandle),
}

/// How a file's conversion went.
#[derive(Clone, Debug, PartialEq)]
enum Status {
    Waiting,
    Working,
    Done(PathBuf, String),
    Failed(String),
}

/// One file in the list.
#[derive(Clone, Debug, PartialEq)]
struct Input {
    source: Source,
    name: String,
    kind: Kind,
    size: u64,
    status: Status,
}

/// Where converted files go and what they are called.
#[derive(Clone, Debug, PartialEq)]
struct Output {
    /// `None` puts each next to its file.
    folder: Option<PathBuf>,
    /// The name, with `{name}` for the original's.
    pattern: String,
    /// Replace a file of the same name instead of keeping both.
    replace: bool,
}

impl Output {
    /// Where `input` converted to `target` goes.
    fn path_for(&self, input: &Input, target: Target, fallback: &Path) -> PathBuf {
        let (folder, stem, extension) = match &input.source {
            Source::File(path) => (
                self.folder
                    .clone()
                    .or_else(|| path.parent().map(Path::to_path_buf))
                    .unwrap_or_else(|| fallback.to_path_buf()),
                path.file_stem().map_or_else(
                    || "converted".to_owned(),
                    |stem| stem.to_string_lossy().into_owned(),
                ),
                path.extension()
                    .map(|extension| extension.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
            Source::Entity(_) => (
                self.folder
                    .clone()
                    .unwrap_or_else(|| fallback.to_path_buf()),
                Path::new(&input.name).file_stem().map_or_else(
                    || "converted".to_owned(),
                    |stem| stem.to_string_lossy().into_owned(),
                ),
                String::new(),
            ),
        };
        let pattern = if self.pattern.trim().is_empty() {
            "{name}"
        } else {
            self.pattern.trim()
        };
        let name = pattern.replace("{name}", &stem);
        let extension = if target == Target::Copy {
            extension
        } else {
            target.extension().to_owned()
        };
        let file = if extension.is_empty() {
            name
        } else {
            format!("{name}.{extension}")
        };
        let path = folder.join(file);
        // Never write over the file being converted.
        let is_source = matches!(&input.source, Source::File(source) if *source == path);
        if self.replace && !is_source {
            path
        } else {
            free_path(&path)
        }
    }
}

/// A file's index in the list, and where it went and what it became.
type Finished = (usize, Result<(PathBuf, String), String>);

/// The Converter.
#[derive(Debug)]
pub struct Convert {
    inputs: Vec<Input>,
    targets: HashMap<Kind, Target>,
    options: Options,
    output: Output,
    reveal_after: bool,
    resize_mode: usize,
    resize_values: (f32, u32, u32),
    sample_rate: usize,
    channels: usize,
    started: bool,
    results: Option<Receiver<Finished>>,
    menus: Vec<Menu>,
    menubar: MenuBar,
    /// When to build the menus again, in another language.
    language: sue_lang::Watch,
}

impl Default for Convert {
    fn default() -> Self {
        Self {
            inputs: Vec::new(),
            targets: HashMap::new(),
            options: Options::default(),
            output: Output {
                folder: None,
                pattern: "{name}".to_owned(),
                replace: false,
            },
            reveal_after: true,
            resize_mode: 0,
            resize_values: (50.0, 1920, 1080),
            sample_rate: 0,
            channels: 0,
            started: false,
            results: None,
            menus: Self::menus(),
            menubar: MenuBar::new(),
            language: sue_lang::Watch::default(),
        }
    }
}

/// The ways to size a picture, in the order the Converter offers them.
fn resizes() -> [String; 6] {
    [
        tr!("keep-the-size"),
        tr!("scale"),
        tr!("width"),
        tr!("height"),
        tr!("fit-inside"),
        tr!("exact-size"),
    ]
}

const RATES: [Option<u32>; 9] = [
    None,
    Some(8_000),
    Some(16_000),
    Some(22_050),
    Some(32_000),
    Some(44_100),
    Some(48_000),
    Some(88_200),
    Some(96_000),
];

/// A sample rate as people read it, e.g. `44,100 Hz (CD)`.
fn rate_label(rate: Option<u32>) -> String {
    match rate {
        None => tr!("keep-the-rate"),
        Some(44_100) => tr!("rate-cd", rate = sue_lang::number(44_100.0, 0)),
        Some(rate) => tr!("rate-hz", rate = sue_lang::number(f64::from(rate), 0)),
    }
}

const CHANNELS: [Option<u16>; 3] = [None, Some(1), Some(2)];

/// A channel count as people read it.
fn channels_label(channels: Option<u16>) -> String {
    match channels {
        None => tr!("keep-the-channels"),
        Some(1) => tr!("mono"),
        Some(2) => tr!("stereo"),
        Some(count) => tr!("channel-count", count = count),
    }
}

fn combo<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: &str,
    width: f32,
    chosen: &mut T,
    options: &[(T, String)],
) -> bool {
    let label = options
        .iter()
        .find(|(value, _)| value == chosen)
        .map_or_else(String::new, |(_, label)| label.clone());
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(label)
        .width(width)
        .show_ui(ui, |ui| {
            for (value, label) in options {
                if ui
                    .add(egui::Button::selectable(value == chosen, label))
                    .clicked()
                {
                    *chosen = *value;
                    changed = true;
                }
            }
        });
    changed
}

fn row(ui: &mut Ui, label: &str, theme: &Theme, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized(
            vec2(110.0, 18.0),
            egui::Label::new(RichText::new(label).color(theme.muted)),
        );
        add(ui);
    });
}

impl Convert {
    /// What kind of input an entity makes: a picture (layered or not), a
    /// sound, or nothing the Converter takes.
    fn kind_of_entity(entity: &sue_core::Entity) -> Option<Kind> {
        if entity.component(ComponentKind::PCMFloat1D).is_some() {
            Some(Kind::Sound)
        } else if sue_model::picture::is_picture(entity)
            || (entity.component(ComponentKind::RasterTile2D).is_some()
                && entity.meta(meta::ENTROPY).is_none())
        {
            Some(Kind::Picture)
        } else {
            entity
                .meta(meta::SOURCE)
                .map(|path| ops::kind_of(Path::new(path), &[]))
        }
    }

    fn add_path(&mut self, path: &Path) {
        if path.is_dir() {
            let Ok(entries) = sue_platform::list_dir(path, false) else {
                return;
            };
            for entry in entries
                .into_iter()
                .filter(|entry| !entry.is_dir)
                .take(FOLDER_LIMIT)
            {
                self.add_path(&entry.path);
            }
            return;
        }
        if self
            .inputs
            .iter()
            .any(|input| input.source == Source::File(path.to_path_buf()))
        {
            return;
        }
        let head: Vec<u8> = std::fs::File::open(path)
            .ok()
            .map(|file| {
                use std::io::Read;
                let mut head = Vec::new();
                let _ = file.take(4096).read_to_end(&mut head);
                head
            })
            .unwrap_or_default();
        self.inputs.push(Input {
            source: Source::File(path.to_path_buf()),
            name: file_name(path),
            kind: ops::kind_of(path, &head),
            size: std::fs::metadata(path).map_or(0, |meta| meta.len()),
            status: Status::Waiting,
        });
    }

    /// The project's pictures and sounds: their files where they have one,
    /// else the entities themselves.
    fn add_project(&mut self, cx: &AppletContext<'_>) {
        let found: Vec<(AssetHandle, String, Option<String>, Kind)> = cx
            .arena()
            .iter()
            .filter_map(|(handle, entity)| {
                let kind = if entity.component(ComponentKind::PCMFloat1D).is_some() {
                    Kind::Sound
                } else if entity.component(ComponentKind::RasterTile2D).is_some()
                    && entity.meta(meta::ENTROPY).is_none()
                {
                    Kind::Picture
                } else {
                    return None;
                };
                Some((
                    handle,
                    entity.name().to_owned(),
                    entity.meta(meta::SOURCE).map(str::to_owned),
                    kind,
                ))
            })
            .collect();
        for (handle, name, source, kind) in found {
            match source.map(PathBuf::from).filter(|path| path.is_file()) {
                Some(path) => self.add_path(&path),
                None if !self
                    .inputs
                    .iter()
                    .any(|input| input.source == Source::Entity(handle)) =>
                {
                    self.inputs.push(Input {
                        source: Source::Entity(handle),
                        name,
                        kind,
                        size: 0,
                        status: Status::Waiting,
                    });
                }
                None => {}
            }
        }
    }

    fn target(&self, kind: Kind) -> Target {
        self.targets
            .get(&kind)
            .copied()
            .unwrap_or_else(|| Target::for_kind(kind)[0])
    }

    fn kinds(&self) -> Vec<Kind> {
        Kind::ALL
            .into_iter()
            .filter(|kind| self.inputs.iter().any(|input| input.kind == *kind))
            .collect()
    }

    fn busy(&self) -> bool {
        self.inputs
            .iter()
            .any(|input| input.status == Status::Working)
    }

    /// The options as the controls left them.
    fn settled_options(&self) -> Options {
        let mut options = self.options.clone();
        let (percent, width, height) = self.resize_values;
        options.picture.resize = match self.resize_mode {
            1 => Resize::Scale(percent),
            2 => Resize::Width(width),
            3 => Resize::Height(height),
            4 => Resize::Fit(width, height),
            5 => Resize::Exact(width, height),
            _ => Resize::Keep,
        };
        options.sound.sample_rate = RATES[self.sample_rate];
        options.sound.channels = CHANNELS[self.channels];
        options
    }

    /// `true` when results put next to their files would stay in Sue's
    /// cache, where nobody sees them: a phone's picker gives Sue a copy of
    /// a file chosen on its own, and the place it came from takes no new
    /// files. `stays` is [`sue_platform::stays_in_cache`] outside tests.
    fn strands_next_to(&self, stays: impl Fn(&Path) -> bool) -> bool {
        self.output.folder.is_none()
            && self.inputs.iter().any(|input| {
                input.status != Status::Working
                    && matches!(&input.source, Source::File(path)
                        if path.parent().is_some_and(&stays))
            })
    }

    /// Converts every waiting file: those on disk on a background thread,
    /// the project's own at once.
    fn run(&mut self, cx: &mut AppletContext<'_>) {
        // Results that would stay in the cache need a folder first; the
        // run goes on once one is chosen.
        if self.strands_next_to(sue_platform::stays_in_cache) {
            Self::pick(cx, "run", "Choose Where Converted Files Go", true);
            return;
        }
        let options = self.settled_options();
        let fallback = crate::util::export_dir(cx);
        let mut jobs = Vec::new();
        for index in 0..self.inputs.len() {
            if matches!(self.inputs[index].status, Status::Working) {
                continue;
            }
            let input = self.inputs[index].clone();
            let target = self.target(input.kind);
            let out = self.output.path_for(&input, target, &fallback);
            match &input.source {
                Source::File(path) => {
                    self.inputs[index].status = Status::Working;
                    jobs.push((index, path.clone(), input.kind, target, out));
                }
                Source::Entity(handle) => {
                    self.inputs[index].status =
                        match Self::convert_entity(cx, *handle, input.kind, target, &options, &out)
                        {
                            Ok(note) => Status::Done(out, note),
                            Err(error) => Status::Failed(error),
                        };
                }
            }
        }
        self.started = true;
        if jobs.is_empty() {
            self.finish(cx);
            return;
        }
        let (sender, receiver) = channel();
        self.results = Some(receiver);
        let _ = std::thread::Builder::new()
            .name("sue-convert".to_owned())
            .spawn(move || {
                for (index, path, kind, target, out) in jobs {
                    let result = std::fs::read(&path)
                        .map_err(|error| error.to_string())
                        .and_then(|bytes| ops::convert(&path, bytes, kind, target, &options))
                        .and_then(|(bytes, note)| {
                            if let Some(folder) = out.parent() {
                                std::fs::create_dir_all(folder)
                                    .map_err(|error| error.to_string())?;
                            }
                            write_atomic(&out, &bytes).map_err(|error| error.to_string())?;
                            Ok((out, format!("{note}, {}", human_bytes(bytes.len() as u64))))
                        });
                    if sender.send((index, result)).is_err() {
                        return;
                    }
                }
            });
    }

    /// Converts a picture or sound held only in the project.
    fn convert_entity(
        cx: &AppletContext<'_>,
        handle: AssetHandle,
        kind: Kind,
        target: Target,
        options: &Options,
        out: &Path,
    ) -> Result<String, String> {
        let entity = cx.arena().get(handle).map_err(|error| error.to_string())?;
        // A layered picture (Sue Paint's) is flattened the way it exports.
        let flat = sue_model::picture::is_picture(entity)
            .then(|| {
                sue_paint::composite::flatten(
                    cx.arena(),
                    entity.id(),
                    sue_paint::composite::RenderOptions::default(),
                )
            })
            .flatten();
        let (bytes, note) = match kind {
            Kind::Picture => {
                let tile = match &flat {
                    Some(flat) => flat,
                    None => entity
                        .component(ComponentKind::RasterTile2D)
                        .and_then(|data| data.as_raster())
                        .ok_or(tr!("the-picture-is-gone"))?,
                };
                let changed = ops::transform_picture(tile, &options.picture, target)?;
                let note = tr!(
                    "by-pixels",
                    width = changed.width(),
                    height = changed.height()
                );
                (
                    ops::write_picture(&changed, target, &options.picture)?,
                    note,
                )
            }
            Kind::Sound => {
                let pcm = entity
                    .component(ComponentKind::PCMFloat1D)
                    .and_then(|data| data.as_pcm())
                    .ok_or(tr!("the-sound-is-gone"))?;
                let changed = ops::transform_sound(pcm, &options.sound)?;
                let note = format!("{:.1} s", changed.duration_seconds());
                (
                    ops::write_sound(&changed, target, options.sound.depth)?,
                    note,
                )
            }
            Kind::Text | Kind::Data => return Err(tr!("only-files-disk-convert-as-text")),
        };
        if let Some(folder) = out.parent() {
            std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        }
        write_atomic(out, &bytes).map_err(|error| error.to_string())?;
        Ok(format!("{note}, {}", human_bytes(bytes.len() as u64)))
    }

    /// Takes in finished conversions. Returns `true` if any came.
    fn collect(&mut self, cx: &mut AppletContext<'_>) -> bool {
        let Some(results) = self.results.as_ref() else {
            return false;
        };
        let mut any = false;
        while let Ok((index, result)) = results.try_recv() {
            any = true;
            if let Some(input) = self.inputs.get_mut(index) {
                input.status = match result {
                    Ok((path, note)) => Status::Done(path, note),
                    Err(error) => Status::Failed(error),
                };
            }
        }
        if any && !self.busy() {
            self.results = None;
            self.finish(cx);
        }
        any
    }

    /// Says how a run went and, if asked, shows the result.
    fn finish(&mut self, cx: &mut AppletContext<'_>) {
        if !self.started {
            return;
        }
        self.started = false;
        let done: Vec<&PathBuf> = self
            .inputs
            .iter()
            .filter_map(|input| match &input.status {
                Status::Done(path, _) => Some(path),
                _ => None,
            })
            .collect();
        let failed = self
            .inputs
            .iter()
            .filter(|input| matches!(input.status, Status::Failed(_)))
            .count();
        match done.as_slice() {
            [] if failed > 0 => cx.fail(tr!("nothing-converted-failed", failed = failed)),
            [] => {}
            [one] => cx.saved(one.to_path_buf(), "converted"),
            many => {
                let folder = many[0].parent().map(Path::to_path_buf).unwrap_or_default();
                cx.saved(
                    many[0].to_path_buf(),
                    tr!(
                        "files-converted-failed-into-2",
                        count = many.len(),
                        value = sue_platform::shown_path(&folder),
                        failed = failed
                    ),
                );
            }
        }
        if self.reveal_after
            && sue_platform::file_manager().is_some()
            && let Some(first) = done.first()
        {
            cx.request(ShellRequest::Command(format!(
                "reveal-file:{}",
                first.display()
            )));
        }
    }

    /// Asks the system's dialog for a file, or with `folder` a folder.
    fn pick(cx: &mut AppletContext<'_>, purpose: &str, title: &str, folder: bool) {
        let pick = FilePick::open(Some(me()), purpose, title);
        cx.request(ShellRequest::PickFile(if folder {
            pick.folder()
        } else {
            pick
        }));
    }

    /// The Converter's menus, in Sue's top bar.
    fn menus() -> Vec<Menu> {
        let act = MenuItem::action;
        vec![
            Menu::new(
                tr!("file"),
                vec![
                    act(tr!("add-files-ellipsis"), "convert.add"),
                    act(tr!("add-a-folder-ellipsis"), "convert.add_folder"),
                    act(tr!("add-projects-pictures-sounds"), "convert.add_project"),
                    act(tr!("clear-the-list"), "convert.clear"),
                    MenuItem::Separator,
                    act(tr!("choose-the-output-folder-ellipsis"), "convert.folder"),
                    MenuItem::Separator,
                    act(tr!("back-to-the-hub"), "convert.leave"),
                ],
            ),
            Menu::new(
                tr!("convert"),
                vec![act(tr!("convert-the-list"), "convert.run").key("Ctrl+Enter")],
            ),
        ]
    }

    fn item_state(&self, command: &str) -> ItemState {
        match command {
            "convert.run" if self.busy() || self.inputs.is_empty() => ItemState::off(),
            "convert.clear" if self.busy() || self.inputs.is_empty() => ItemState::off(),
            _ => ItemState::on(),
        }
    }

    /// Takes the rice's quality, effort and folder while the list is empty.
    fn follow_rice(&mut self, cx: &AppletContext<'_>) {
        if !self.inputs.is_empty() {
            return;
        }
        let picture = &mut self.options.picture;
        picture.jpeg_quality = cx
            .rice()
            .get_i64("io.jpeg_quality")
            .and_then(|quality| u8::try_from(quality).ok())
            .unwrap_or(picture.jpeg_quality);
        picture.jxl_effort = cx
            .rice()
            .get_i64("io.jxl_effort")
            .and_then(|effort| u8::try_from(effort).ok())
            .unwrap_or(picture.jxl_effort);
        if self.output.folder.is_none() {
            let riced = cx.rice().get_str("io.convert_dir").unwrap_or("").trim();
            if !riced.is_empty() {
                self.output.folder = Some(PathBuf::from(riced));
            }
        }
    }

    // ----- The parts of the Converter -----------------------------------------

    fn files_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            if widgets::pill_button(ui, &tr!("add-files"), false, &theme)
                .on_hover_text(tr!("choose-file-systems-dialog"))
                .clicked()
            {
                Self::pick(cx, "add", &tr!("add-files-to-convert"), false);
            }
            if widgets::pill_button(ui, &tr!("add-a-folder"), false, &theme)
                .on_hover_text(tr!("every-file-in-a-folder"))
                .clicked()
            {
                Self::pick(cx, "add", &tr!("add-a-folder-to-convert"), true);
            }
            if widgets::pill_button(ui, &tr!("add-from-the-project"), false, &theme)
                .on_hover_text(tr!("pictures-sounds-open-sue"))
                .clicked()
            {
                self.add_project(cx);
            }
            if !self.inputs.is_empty()
                && !self.busy()
                && widgets::pill_button(ui, &tr!("clear"), false, &theme).clicked()
            {
                self.inputs.clear();
            }
        });
        if self.inputs.is_empty() {
            ui.add(
                egui::Label::new(
                    RichText::new(tr!("add-any-file-picture-song-text")).color(theme.muted),
                )
                .wrap(),
            );
            return;
        }
        let mut remove = None;
        let mut reveal = None;
        for (index, input) in self.inputs.iter().enumerate() {
            let height = theme.ui_size * 2.6;
            let (rect, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
            if response.hovered() {
                ui.painter().rect_filled(rect, 4.0, theme.hover);
            }
            let painter = ui
                .painter()
                .with_clip_rect(rect.shrink2(vec2(26.0, 0.0)).translate(vec2(-20.0, 0.0)));
            painter.text(
                rect.left_center() + vec2(8.0, -theme.small_size * 0.6),
                Align2::LEFT_CENTER,
                &input.name,
                theme.font(theme.ui_size),
                theme.text,
            );
            let detail = match &input.status {
                Status::Waiting => {
                    let size = if input.size > 0 {
                        format!(", {}", human_bytes(input.size))
                    } else {
                        String::new()
                    };
                    tr!(
                        "becomes",
                        value = input.kind.singular(),
                        label = self.target(input.kind).label(),
                        size = size
                    )
                }
                Status::Working => tr!("converting-ellipsis"),
                Status::Done(path, note) => tr!("done", value = file_name(path), note = note),
                Status::Failed(error) => format!("! {error}"),
            };
            painter.text(
                rect.left_center() + vec2(8.0, theme.ui_size * 0.55),
                Align2::LEFT_CENTER,
                detail,
                theme.font(theme.small_size),
                theme.muted,
            );
            let close = egui::Rect::from_center_size(
                rect.right_center() - vec2(12.0, 0.0),
                vec2(20.0, 20.0),
            );
            let close_response = ui.interact(
                close,
                ui.scope_id().with(("convert-remove", index)),
                Sense::click(),
            );
            ui.painter().text(
                close.center(),
                Align2::CENTER_CENTER,
                "\u{00D7}",
                theme.font(theme.ui_size * 1.2),
                if close_response.hovered() {
                    theme.text
                } else {
                    theme.muted
                },
            );
            if close_response
                .on_hover_text(tr!("take-it-off-the-list"))
                .clicked()
            {
                remove = Some(index);
            } else if let Status::Done(path, _) = &input.status
                && let Some(manager) = sue_platform::file_manager()
                && response
                    .on_hover_text(tr!("show-it-in", manager = manager))
                    .clicked()
            {
                reveal = Some(path.clone());
            }
        }
        if let Some(index) = remove.filter(|index| self.inputs[*index].status != Status::Working) {
            self.inputs.remove(index);
        }
        if let Some(path) = reveal {
            cx.request(ShellRequest::Command(format!(
                "reveal-file:{}",
                path.display()
            )));
        }
    }

    fn targets_ui(&mut self, ui: &mut Ui, theme: &Theme) {
        for kind in self.kinds() {
            let mut target = self.target(kind);
            let options: Vec<(Target, String)> = Target::for_kind(kind)
                .into_iter()
                .map(|target| (target, target.label()))
                .collect();
            row(ui, &tr!("become", plural = kind.plural()), theme, |ui| {
                if combo(
                    ui,
                    &format!("sue-convert-target-{kind:?}"),
                    210.0,
                    &mut target,
                    &options,
                ) {
                    self.targets.insert(kind, target);
                }
            });
        }
    }

    #[allow(clippy::too_many_lines, reason = "every picture option in one place")]
    fn picture_ui(&mut self, ui: &mut Ui, theme: &Theme) {
        let target = self.target(Kind::Picture);
        if matches!(
            target,
            Target::Base64 | Target::Hex | Target::ByteMap | Target::Copy
        ) {
            return;
        }
        egui::CollapsingHeader::new(RichText::new(tr!("picture-options")).color(theme.text))
            .id_salt("sue-convert-picture")
            .default_open(true)
            .show(ui, |ui| {
                let picture = &mut self.options.picture;
                row(ui, &tr!("size"), theme, |ui| {
                    let options: Vec<(usize, String)> = resizes().into_iter().enumerate().collect();
                    combo(
                        ui,
                        "sue-convert-resize",
                        140.0,
                        &mut self.resize_mode,
                        &options,
                    );
                });
                let (percent, width, height) = &mut self.resize_values;
                match self.resize_mode {
                    1 => row(ui, &tr!("scale"), theme, |ui| {
                        ui.add(
                            egui::Slider::new(percent, 1.0..=400.0)
                                .suffix(" %")
                                .max_decimals(0),
                        );
                    }),
                    2 => row(ui, &tr!("width"), theme, |ui| {
                        ui.add(
                            egui::DragValue::new(width)
                                .range(1..=32_768)
                                .suffix(tr!("px")),
                        );
                    }),
                    3 => row(ui, &tr!("height"), theme, |ui| {
                        ui.add(
                            egui::DragValue::new(height)
                                .range(1..=32_768)
                                .suffix(tr!("px")),
                        );
                    }),
                    4 | 5 => row(ui, &tr!("box"), theme, |ui| {
                        ui.add(
                            egui::DragValue::new(width)
                                .range(1..=32_768)
                                .suffix(tr!("px")),
                        );
                        ui.label("by");
                        ui.add(
                            egui::DragValue::new(height)
                                .range(1..=32_768)
                                .suffix(tr!("px")),
                        );
                    }),
                    _ => {}
                }
                if self.resize_mode != 0 {
                    row(ui, &tr!("resampling"), theme, |ui| {
                        let options: Vec<(Filter, String)> = Filter::ALL
                            .iter()
                            .map(|filter| (*filter, filter.label()))
                            .collect();
                        combo(
                            ui,
                            "sue-convert-filter",
                            160.0,
                            &mut picture.filter,
                            &options,
                        );
                    });
                }
                row(ui, &tr!("turn"), theme, |ui| {
                    let options = [
                        tr!("none"),
                        tr!("n-90-right"),
                        "180\u{00B0}".to_owned(),
                        tr!("n-90-left"),
                    ];
                    if let Some(index) = widgets::segmented(
                        ui,
                        &options,
                        Some(usize::from(picture.quarter_turns % 4)),
                        theme,
                    ) {
                        picture.quarter_turns = index as u8;
                    }
                });
                row(ui, &tr!("mirror"), theme, |ui| {
                    ui.checkbox(&mut picture.flip_h, tr!("left-to-right"));
                    ui.checkbox(&mut picture.flip_v, tr!("top-to-bottom"));
                });
                row(ui, &tr!("brightness"), theme, |ui| {
                    ui.add(egui::Slider::new(&mut picture.brightness, -1.0..=1.0).max_decimals(2));
                });
                row(ui, &tr!("contrast"), theme, |ui| {
                    ui.add(egui::Slider::new(&mut picture.contrast, -1.0..=1.0).max_decimals(2));
                });
                row(ui, &tr!("saturation"), theme, |ui| {
                    ui.add(egui::Slider::new(&mut picture.saturation, -1.0..=1.0).max_decimals(2));
                });
                row(ui, &tr!("colors"), theme, |ui| {
                    ui.checkbox(&mut picture.grayscale, tr!("grays-only"));
                    ui.checkbox(&mut picture.flatten, tr!("fill-transparency"));
                });
                if picture.flatten || target.flattens() {
                    row(ui, &tr!("background"), theme, |ui| {
                        let [r, g, b] = picture.background;
                        let mut rgba = [r, g, b, 255];
                        if sue_wm::color_button(
                            ui,
                            "sue-convert-background",
                            &mut rgba,
                            false,
                            theme,
                        )
                        .changed
                        {
                            picture.background = [rgba[0], rgba[1], rgba[2]];
                        }
                    });
                }
                if target == Target::Jpeg {
                    row(ui, &tr!("quality"), theme, |ui| {
                        ui.add(egui::Slider::new(&mut picture.jpeg_quality, 1..=100));
                    });
                }
                if target == Target::Jxl {
                    row(ui, &tr!("effort"), theme, |ui| {
                        ui.add(egui::Slider::new(&mut picture.jxl_effort, 1..=9))
                            .on_hover_text(tr!("higher-is-smaller-and-slower"));
                    });
                }
                if let Target::Picture(format) = target
                    && let Some(max) = format.max_side()
                    && max <= 256
                {
                    ui.label(
                        RichText::new(tr!(
                            "pictures-are-most-bigger-ones-are",
                            format = format.name(),
                            max = max
                        ))
                        .size(theme.small_size)
                        .color(theme.muted),
                    );
                }
            });
    }

    fn sound_ui(&mut self, ui: &mut Ui, theme: &Theme) {
        let target = self.target(Kind::Sound);
        if !matches!(target, Target::Wav | Target::Flac | Target::Aiff) {
            return;
        }
        egui::CollapsingHeader::new(RichText::new(tr!("sound-options")).color(theme.text))
            .id_salt("sue-convert-sound")
            .default_open(true)
            .show(ui, |ui| {
                row(ui, &tr!("sample-rate"), theme, |ui| {
                    let options: Vec<(usize, String)> = RATES
                        .iter()
                        .enumerate()
                        .map(|(index, rate)| (index, rate_label(*rate)))
                        .collect();
                    combo(
                        ui,
                        "sue-convert-rate",
                        150.0,
                        &mut self.sample_rate,
                        &options,
                    );
                });
                row(ui, &tr!("channels"), theme, |ui| {
                    let options: Vec<(usize, String)> = CHANNELS
                        .iter()
                        .enumerate()
                        .map(|(index, channels)| (index, channels_label(*channels)))
                        .collect();
                    combo(
                        ui,
                        "sue-convert-channels",
                        150.0,
                        &mut self.channels,
                        &options,
                    );
                });
                let sound = &mut self.options.sound;
                row(ui, &tr!("bits"), theme, |ui| {
                    let mut depths = vec![
                        (SampleDepth::Int16, tr!("n-16-bit-cd")),
                        (SampleDepth::Int24, tr!("n-24-bit-studio")),
                    ];
                    if target == Target::Wav {
                        depths.push((SampleDepth::Float32, tr!("n-32-bit-float")));
                    } else if sound.depth == SampleDepth::Float32 {
                        sound.depth = SampleDepth::Int24;
                    }
                    combo(ui, "sue-convert-depth", 150.0, &mut sound.depth, &depths);
                });
                row(ui, &tr!("loudness"), theme, |ui| {
                    ui.checkbox(&mut sound.normalize, tr!("normalize-to-1-db"));
                });
                if !sound.normalize {
                    row(ui, &tr!("gain"), theme, |ui| {
                        ui.add(
                            egui::Slider::new(&mut sound.gain_db, -24.0..=24.0)
                                .suffix(tr!("db"))
                                .max_decimals(1),
                        );
                    });
                }
                row(ui, &tr!("cut-the-start"), theme, |ui| {
                    ui.add(
                        egui::Slider::new(&mut sound.trim_start, 0.0..=60.0)
                            .suffix(" s")
                            .max_decimals(2),
                    );
                });
                row(ui, &tr!("cut-the-end"), theme, |ui| {
                    ui.add(
                        egui::Slider::new(&mut sound.trim_end, 0.0..=60.0)
                            .suffix(" s")
                            .max_decimals(2),
                    );
                });
                row(ui, &tr!("fade-in"), theme, |ui| {
                    ui.add(
                        egui::Slider::new(&mut sound.fade_in, 0.0..=10.0)
                            .suffix(" s")
                            .max_decimals(2),
                    );
                });
                row(ui, &tr!("fade-out"), theme, |ui| {
                    ui.add(
                        egui::Slider::new(&mut sound.fade_out, 0.0..=10.0)
                            .suffix(" s")
                            .max_decimals(2),
                    );
                });
                row(ui, &tr!("speed"), theme, |ui| {
                    ui.add(
                        egui::Slider::new(&mut sound.speed, 0.25..=4.0)
                            .suffix("x")
                            .max_decimals(2),
                    )
                    .on_hover_text(tr!("changes-the-pitch-too"));
                });
                row(ui, &tr!("direction"), theme, |ui| {
                    ui.checkbox(&mut sound.reverse, tr!("backwards"));
                });
            });
    }

    fn text_ui(&mut self, ui: &mut Ui, theme: &Theme) {
        let kinds = self.kinds();
        let text_target = kinds.contains(&Kind::Text).then(|| self.target(Kind::Text));
        let data_targets: Vec<Target> = kinds.iter().map(|kind| self.target(*kind)).collect();
        let text = &mut self.options.text;
        if let Some(target) = text_target
            && matches!(
                target,
                Target::Text | Target::Html | Target::Json | Target::Csv | Target::Tsv
            )
        {
            egui::CollapsingHeader::new(RichText::new(tr!("text-options")).color(theme.text))
                .id_salt("sue-convert-text")
                .default_open(true)
                .show(ui, |ui| {
                    row(ui, &tr!("line-endings"), theme, |ui| {
                        let options = vec![
                            (Endings::Keep, tr!("keep")),
                            (Endings::Lf, tr!("lf-linux-macos")),
                            (Endings::CrLf, tr!("crlf-windows")),
                        ];
                        combo(
                            ui,
                            "sue-convert-endings",
                            160.0,
                            &mut text.endings,
                            &options,
                        );
                    });
                    if target == Target::Text {
                        row(ui, &tr!("tidy"), theme, |ui| {
                            ui.checkbox(&mut text.trim_trailing, tr!("drop-trailing-spaces"));
                        });
                        row(ui, &tr!("tabs"), theme, |ui| {
                            ui.add(
                                egui::Slider::new(&mut text.tab_spaces, 0..=8)
                                    .suffix(tr!("spaces")),
                            )
                            .on_hover_text(tr!("n-0-keeps-the-tabs"));
                        });
                    }
                    if target == Target::Json {
                        row(ui, &tr!("indent"), theme, |ui| {
                            ui.add(
                                egui::Slider::new(&mut text.json_indent, 0..=8)
                                    .suffix(tr!("spaces")),
                            )
                            .on_hover_text(tr!("n-0-writes-one-line"));
                        });
                        row(ui, &tr!("tables"), theme, |ui| {
                            ui.checkbox(&mut text.header_row, tr!("first-row-names-columns"));
                        });
                    }
                    if target == Target::Html {
                        row(ui, &tr!("page-title"), theme, |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut text.html_title)
                                    .hint_text(tr!("the-files-name"))
                                    .desired_width(180.0),
                            );
                        });
                    }
                });
        }
        if data_targets.contains(&Target::Hex) {
            row(ui, &tr!("hex-bytes-a-line"), theme, |ui| {
                ui.add(egui::Slider::new(&mut text.hex_width, 4..=64));
            });
        }
        if data_targets.contains(&Target::Base64) {
            row(ui, &tr!("base64"), theme, |ui| {
                ui.checkbox(&mut text.base64_wrap, tr!("wrap-lines-at-76"));
            });
        }
        if data_targets.contains(&Target::ByteMap) {
            row(ui, &tr!("byte-map-width"), theme, |ui| {
                ui.add(egui::Slider::new(&mut text.map_width, 16..=2048).suffix(tr!("px")));
            });
        }
    }

    fn output_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        widgets::heading(ui, &tr!("save-to"), &theme);
        let mut next_to = self.output.folder.is_none();
        ui.horizontal_wrapped(|ui| {
            if ui
                .radio_value(&mut next_to, true, tr!("next-to-each-file"))
                .clicked()
            {
                self.output.folder = None;
            }
            if ui
                .radio_value(&mut next_to, false, tr!("this-folder"))
                .clicked()
                && self.output.folder.is_none()
            {
                Self::pick(cx, "folder", &tr!("choose-where-converted-files-go"), true);
            }
        });
        if self.strands_next_to(sue_platform::stays_in_cache) {
            ui.add(
                egui::Label::new(
                    RichText::new(
                        "Sue works on copies of files you pick one by one, so Convert first asks which folder the results go to.",
                    )
                    .size(theme.small_size)
                    .color(theme.muted),
                )
                .wrap(),
            );
        }
        if let Some(folder) = self.output.folder.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(sue_platform::shown_path(&folder))
                            .size(theme.small_size)
                            .color(theme.text),
                    )
                    .wrap(),
                );
                if widgets::pill_button(ui, &tr!("change"), false, &theme).clicked() {
                    Self::pick(cx, "folder", &tr!("choose-where-converted-files-go"), true);
                }
                if sue_platform::file_manager().is_some()
                    && widgets::pill_button(ui, &tr!("open"), false, &theme).clicked()
                {
                    cx.request(ShellRequest::Command(format!(
                        "reveal:{}",
                        folder.display()
                    )));
                }
            });
        }
        row(ui, &tr!("file-name"), &theme, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.output.pattern)
                    .desired_width(140.0)
                    .hint_text("{name}"),
            )
            .on_hover_text(tr!("originals-name-e-g-small"));
        });
        row(ui, &tr!("if-it-exists"), &theme, |ui| {
            let options = vec![(false, tr!("keep-both")), (true, tr!("replace-it"))];
            combo(
                ui,
                "sue-convert-replace",
                120.0,
                &mut self.output.replace,
                &options,
            );
        });
        if let Some(manager) = sue_platform::file_manager() {
            ui.checkbox(
                &mut self.reveal_after,
                tr!("show-result-when-done", manager = manager),
            );
        }
    }

    fn body(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        self.collect(cx);
        self.files_ui(ui, cx);
        if self.inputs.is_empty() {
            return;
        }
        ui.add_space(theme.padding);
        self.targets_ui(ui, &theme);
        if self.kinds().contains(&Kind::Picture) {
            self.picture_ui(ui, &theme);
        }
        if self.kinds().contains(&Kind::Sound) {
            self.sound_ui(ui, &theme);
        }
        self.text_ui(ui, &theme);
        ui.add_space(theme.padding);
        self.output_ui(ui, cx);
        ui.add_space(theme.padding);
        let waiting = self
            .inputs
            .iter()
            .filter(|input| !matches!(input.status, Status::Working))
            .count();
        ui.horizontal(|ui| {
            let label = if waiting == 1 {
                tr!("convert-1-file")
            } else {
                tr!("convert-files", waiting = waiting)
            };
            if ui
                .add_enabled(
                    !self.busy() && waiting > 0,
                    egui::Button::new(RichText::new(label).color(theme.text)),
                )
                .clicked()
            {
                self.run(cx);
            }
            if self.busy() {
                let done = self
                    .inputs
                    .iter()
                    .filter(|input| matches!(input.status, Status::Done(..) | Status::Failed(_)))
                    .count();
                ui.add(
                    egui::ProgressBar::new(done as f32 / self.inputs.len().max(1) as f32)
                        .desired_width(140.0)
                        .show_percentage(),
                );
                ui.ctx().request_repaint_after(Duration::from_millis(100));
            }
        });
    }
}

fn me() -> AppletId {
    AppletId::new(ID).expect("a valid id")
}

impl SueApplet for Convert {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "Converter", AppletRole::Tool, "\u{1F504}")
            .localized("convert");
        manifest.summary =
            "Turn any file into another kind: pictures, sounds, text, tables or bytes.".to_owned();
        manifest.order = 30;
        manifest
    }

    fn page_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.follow_rice(cx);
        let theme = cx.theme.clone();
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, theme.void);
        if ui
            .ctx()
            .input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter))
            && !self.busy()
        {
            self.run(cx);
        }
        // One readable column, centered on wide windows.
        let width = (full.width() - theme.padding * 4.0).clamp(240.0, 760.0);
        let area = Rect::from_min_max(
            pos2(
                full.center().x - width / 2.0,
                full.top() + theme.padding * 2.0,
            ),
            pos2(full.center().x + width / 2.0, full.bottom()),
        );
        let mut inner = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(area)
                .layout(Layout::top_down(Align::Min)),
        );
        inner.set_clip_rect(full.intersect(ui.clip_rect()));
        ScrollArea::vertical()
            .id_salt("sue-convert-page")
            .auto_shrink([false, false])
            .show(&mut inner, |ui| {
                ui.label(
                    RichText::new(tr!("applet-name"))
                        .size(theme.heading_size)
                        .color(theme.text),
                );
                ui.add_space(theme.padding);
                self.body(ui, cx);
                ui.add_space(theme.padding * 2.0);
            });
    }

    fn bar_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        if self.language.changed() {
            self.menus = Self::menus();
        }
        let theme = cx.theme.clone();
        let rect = ui.max_rect();
        let mut menubar = std::mem::take(&mut self.menubar);
        let picked = {
            let state = |command: &str| self.item_state(command);
            menubar.ui(ui, rect, &self.menus, &state, &theme)
        };
        self.menubar = menubar;
        if let Some(command) = picked {
            self.command(&command, cx);
        }
    }

    fn commands(&self) -> Vec<sue_wm::PaletteEntry> {
        [
            (tr!("add-files"), "convert.add"),
            (tr!("add-a-folder"), "convert.add_folder"),
            (tr!("convert-the-list"), "convert.run"),
        ]
        .into_iter()
        .map(|(label, command)| sue_wm::PaletteEntry {
            label: label.to_owned(),
            command: command.to_owned(),
            hint: String::new(),
        })
        .collect()
    }

    fn command(&mut self, name: &str, cx: &mut AppletContext<'_>) -> bool {
        match name {
            "convert.add" | "file.open" => {
                Self::pick(cx, "add", &tr!("add-files-to-convert"), false);
            }
            "convert.add_folder" => Self::pick(cx, "add", &tr!("add-a-folder-to-convert"), true),
            "convert.add_project" => self.add_project(cx),
            "convert.clear" if !self.busy() => self.inputs.clear(),
            "convert.folder" => {
                Self::pick(cx, "folder", &tr!("choose-where-converted-files-go"), true)
            }
            "convert.run" if !self.busy() => self.run(cx),
            "convert.run" | "convert.clear" => cx.notify(tr!("already-converting")),
            "convert.leave" => cx.request(ShellRequest::Hub),
            _ => return false,
        }
        if cx.view.page.as_ref().map(AppletId::as_str) != Some(ID) && name != "convert.leave" {
            cx.request(ShellRequest::Open(me()));
        }
        true
    }

    fn tick(&mut self, cx: &mut AppletContext<'_>) -> Tick {
        let arrived = self.collect(cx);
        Tick {
            repaint: arrived,
            wake: self
                .busy()
                .then(|| std::time::Instant::now() + Duration::from_millis(100)),
            ..Tick::default()
        }
    }

    fn sue_actions(
        &self,
        focus: Option<&sue_core::Entity>,
        _current: bool,
    ) -> Vec<sue_wm::SueAction> {
        match focus {
            Some(entity) if Self::kind_of_entity(entity).is_some() => {
                vec![sue_wm::SueAction::new(
                    sue_wm::sue_menu::SEND_TO,
                    tr!("applet-name"),
                    ShellRequest::SendEntity {
                        to: me(),
                        purpose: "add".to_owned(),
                        entity: entity.id(),
                    },
                )]
            }
            _ => Vec::new(),
        }
    }

    fn take_entity(
        &mut self,
        _purpose: &str,
        entity: sue_core::EntityId,
        cx: &mut AppletContext<'_>,
    ) {
        let Some(handle) = cx.arena().handle_of(entity) else {
            return;
        };
        let Ok(found) = cx.arena().get(handle) else {
            return;
        };
        let source = found
            .meta(meta::SOURCE)
            .map(PathBuf::from)
            .filter(|path| path.is_file() && !sue_model::picture::is_picture(found));
        let name = found.name().to_owned();
        let kind = Self::kind_of_entity(found);
        match (source, kind) {
            (Some(path), _) => self.add_path(&path),
            (None, Some(kind)) => {
                if !self
                    .inputs
                    .iter()
                    .any(|input| input.source == Source::Entity(handle))
                {
                    self.inputs.push(Input {
                        source: Source::Entity(handle),
                        name,
                        kind,
                        size: 0,
                        status: Status::Waiting,
                    });
                }
            }
            (None, None) => return,
        }
        cx.request(ShellRequest::Open(me()));
    }

    fn picked(&mut self, pick: &FilePick, path: &Path, cx: &mut AppletContext<'_>) {
        match pick.purpose.as_str() {
            "add" => self.add_path(path),
            "folder" | "run" => {
                let folder = if path.is_dir() {
                    path.to_path_buf()
                } else {
                    path.parent().map(Path::to_path_buf).unwrap_or_default()
                };
                self.output.folder = Some(folder);
                // Convert asked for it: carry on converting.
                if pick.purpose == "run" && !self.busy() {
                    self.run(cx);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outputs_go_next_to_their_file_and_never_over_it() {
        let dir = std::env::temp_dir().join(format!("sue-convert-out-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("photo.png");
        std::fs::write(&source, b"x").unwrap();
        let input = Input {
            source: Source::File(source.clone()),
            name: "photo.png".to_owned(),
            kind: Kind::Picture,
            size: 1,
            status: Status::Waiting,
        };
        let mut output = Output {
            folder: None,
            pattern: "{name}".to_owned(),
            replace: true,
        };
        assert_eq!(
            output.path_for(&input, Target::Jxl, &dir),
            dir.join("photo.jxl")
        );
        // The same name and format would be the source itself: keep both.
        assert_eq!(
            output.path_for(&input, Target::Png, &dir),
            dir.join("photo (2).png")
        );
        output.pattern = "{name}-small".to_owned();
        output.folder = Some(dir.join("out"));
        assert_eq!(
            output.path_for(&input, Target::Copy, &dir),
            dir.join("out").join("photo-small.png")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copies_of_picked_files_ask_where_their_results_go() {
        // A stand-in for a phone's cache: the copy of a file picked on its
        // own is in `picked/1`, a chosen folder's files are in `picked/2`.
        let stays = |path: &Path| path == Path::new("picked/1");
        let input = |source| Input {
            source,
            name: "Picture.png".to_owned(),
            kind: Kind::Picture,
            size: 1,
            status: Status::Waiting,
        };
        let mut convert = Convert::default();
        convert
            .inputs
            .push(input(Source::File(PathBuf::from("picked/2/Song.wav"))));
        convert
            .inputs
            .push(input(Source::Entity(AssetHandle::new(1, 1))));
        assert!(!convert.strands_next_to(stays), "these land where they are");

        convert
            .inputs
            .push(input(Source::File(PathBuf::from("picked/1/Picture.png"))));
        assert!(convert.strands_next_to(stays));
        assert!(
            !convert.strands_next_to(sue_platform::stays_in_cache),
            "a desktop writes next to every file"
        );
        // Already on its way, it asks nothing.
        convert.inputs[2].status = Status::Working;
        assert!(!convert.strands_next_to(stays));
        convert.inputs[2].status = Status::Done(PathBuf::new(), String::new());
        assert!(convert.strands_next_to(stays));
        // With a folder chosen, every result goes there.
        convert.output.folder = Some(PathBuf::from("picked/3"));
        assert!(!convert.strands_next_to(stays));
    }
}
