//! The batch list: picture files waiting to become other picture files,
//! converted in the background on a [`JobPool`], each with its status.
//! Sue Paint's File, Batch Convert Pictures shows it under the picture
//! page of the export settings dialog ([`crate::export`]).
//!
//! It holds pictures only: what [`sue_io::picture_ops::reads`] takes.
//! Files come from Add Files and Add a Folder (the system's dialogs,
//! which the host opens when [`JobList::show`] asks for them); a folder
//! adds the pictures directly in it, at most [`FOLDER_LIMIT`]. Results go
//! next to each file or into one folder, named by a pattern in which
//! `{name}` stands for the original's name, and either keep both files
//! or replace one of the same name, but never the file being converted.
//! The group "Change every picture" turns, mirrors and adjusts each one
//! on the way (scanned pages come in sideways, a stack of scans is made
//! lighter in one go).
//!
//! On a phone the system's picker hands Sue copies of files chosen one by
//! one, in its cache, where nobody sees what is written beside them: the
//! list then asks for an output folder before it starts
//! ([`JobList::strands_next_to`], [`sue_platform::stays_in_cache`]).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use egui::{Context, Ui};
use sue_core::{JobError, JobHandle, JobPool};
use sue_io::picture_ops::{read_picture, reads};
use sue_rice::RiceSchema;

use crate::Theme;
use crate::export::{PictureSettings, picture_page};
use crate::icons::Icon;
use crate::ui::{
    Answer, Dialog, ProgressBar, RowAction, Space, button, checkbox, dropdown, empty_state,
    form_grid, group, list_row, radio, section, segmented, slider, text_field,
};

/// The most pictures one Add a Folder adds.
pub const FOLDER_LIMIT: usize = 500;

/// What a finished conversion gives: where the file went, and a note on
/// what it became (its size in pixels and in bytes).
type Converted = Result<(PathBuf, String), String>;

/// How one file's conversion stands.
#[derive(Debug)]
pub enum JobStatus {
    /// In the list, not converted in this run.
    Waiting,
    /// Converting on a worker.
    Working(JobHandle<(PathBuf, String)>),
    /// Written to the path, with a note.
    Done(PathBuf, String),
    /// Not written, and why.
    Failed(String),
}

impl JobStatus {
    /// `true` while it converts.
    #[must_use]
    pub fn working(&self) -> bool {
        matches!(self, Self::Working(_))
    }
}

/// One picture file in the list.
#[derive(Debug)]
pub struct JobItem {
    /// The file.
    pub path: PathBuf,
    /// Its name, for display.
    pub name: String,
    /// Its size in bytes, 0 when unknown.
    pub size: u64,
    /// How its conversion stands.
    pub status: JobStatus,
}

/// Where converted files go and what they are called.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobOutput {
    /// `None` puts each next to its file.
    pub folder: Option<PathBuf>,
    /// The name, with `{name}` for the original's.
    pub pattern: String,
    /// Replace a file of the same name instead of keeping both.
    pub replace: bool,
}

impl Default for JobOutput {
    fn default() -> Self {
        Self {
            folder: None,
            pattern: "{name}".to_owned(),
            replace: false,
        }
    }
}

/// `true` when `a` and `b` name one file. Names that differ only in
/// case count as one, as they do on Windows and on a Mac: a result
/// called `Photo.png` would replace a source called `photo.PNG` there.
fn same_file(a: &Path, b: &Path) -> bool {
    a == b || a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

/// `true` when `path` is one of `taken`, case aside ([`same_file`]).
fn is_taken(path: &Path, taken: &HashSet<PathBuf>) -> bool {
    taken.contains(path) || taken.iter().any(|other| same_file(other, path))
}

/// `path`, or the first free sibling such as `photo (2).png` when it
/// exists on disk or is in `taken`.
#[must_use]
pub fn free_path(path: &Path, taken: &HashSet<PathBuf>) -> PathBuf {
    let used = |candidate: &Path| sue_platform::fs::exists(candidate) || is_taken(candidate, taken);
    if !used(path) {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    let extension = path
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    (2..)
        .map(|index| path.with_file_name(format!("{stem} ({index}){extension}")))
        .find(|candidate| !used(candidate))
        .unwrap_or_else(|| path.to_path_buf())
}

impl JobOutput {
    /// Where `source` converted to a file of `extension` goes. `taken`
    /// holds the paths no result may replace: the files of the run
    /// (each is read on a worker, perhaps after another result is
    /// written) and the results already placed, so two pictures never
    /// land on one name.
    #[must_use]
    pub fn path_for(&self, source: &Path, extension: &str, taken: &HashSet<PathBuf>) -> PathBuf {
        let folder = self
            .folder
            .clone()
            .or_else(|| source.parent().map(Path::to_path_buf))
            .unwrap_or_default();
        let stem = source.file_stem().map_or_else(
            || "picture".to_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let pattern = match self.pattern.trim() {
            "" => "{name}",
            pattern => pattern,
        };
        // A pattern holds a name, never a folder.
        let name = pattern.replace("{name}", &stem).replace(['/', '\\'], "_");
        let path = folder.join(format!("{name}.{extension}"));
        // Never write over the file being converted, another file of
        // the run, or another result of it.
        if self.replace && !same_file(&path, source) && !is_taken(&path, taken) {
            path
        } else {
            let mut taken = taken.clone();
            taken.insert(source.to_path_buf());
            free_path(&path, &taken)
        }
    }
}

/// What the list asks its host to do: the system's dialogs and the file
/// manager are the host's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobListRequest {
    /// Open the system's dialog for picture files, then
    /// [`JobList::add_path`] each one.
    AddFiles,
    /// Open the system's folder dialog, then [`JobList::add_path`] it.
    AddFolder,
    /// Open the system's folder dialog, then
    /// [`JobList::set_folder`] it.
    ChooseFolder,
    /// Show this file in the file manager.
    Reveal(PathBuf),
}

/// How [`JobList::start`] went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// This many files are converting.
    Started(usize),
    /// Results would stay in the phone's cache: choose an output folder
    /// first ([`JobListRequest::ChooseFolder`]), then start again.
    NeedsFolder,
    /// Nothing waits, or a run is going.
    Nothing,
}

/// How a run went, once its last file finished.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunSummary {
    /// The files written.
    pub done: Vec<PathBuf>,
    /// How many failed.
    pub failed: usize,
}

impl RunSummary {
    /// What people read: how many were converted and where to, or that
    /// none was.
    #[must_use]
    pub fn message(&self) -> String {
        match self.done.as_slice() {
            [] => tr!("batch-nothing-converted", failed = self.failed),
            [one] if self.failed == 0 => {
                tr!("batch-converted-one", value = sue_platform::shown_path(one))
            }
            done => {
                let folder = done[0].parent();
                let one_folder = done.iter().all(|path| path.parent() == folder);
                let text = match folder {
                    Some(folder) if one_folder => tr!(
                        "batch-converted-into",
                        count = done.len(),
                        value = sue_platform::shown_path(folder)
                    ),
                    _ => tr!("batch-converted-next-to", count = done.len()),
                };
                if self.failed == 0 {
                    text
                } else {
                    tr!("batch-and-failed", text = text, failed = self.failed)
                }
            }
        }
    }
}

/// The first line of a message, without the control characters some
/// decoders end theirs with.
fn one_line(message: &str) -> String {
    message
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .trim()
        .to_owned()
}

/// A size in bytes as people read it.
fn byte_size(bytes: u64) -> String {
    let value = bytes as f64;
    if bytes < 1024 {
        tr!("batch-size-bytes", size = sue_lang::number(value, 0))
    } else if bytes < 1024 * 1024 {
        tr!("batch-size-kib", size = sue_lang::number(value / 1024.0, 1))
    } else {
        tr!(
            "batch-size-mib",
            size = sue_lang::number(value / (1024.0 * 1024.0), 1)
        )
    }
}

/// Converts the picture file `source` as `settings` say and writes it at
/// `out`. Runs on a worker.
fn convert(source: &Path, out: &Path, settings: &PictureSettings) -> Converted {
    let bytes = sue_platform::fs::read(source).map_err(|error| error.to_string())?;
    let picture = read_picture(source, &bytes)?;
    let written = settings.export(&picture)?;
    let (width, height) = settings.output_size(picture.width(), picture.height());
    if let Some(folder) = out.parent() {
        sue_platform::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    sue_platform::write_atomic(out, &written).map_err(|error| error.to_string())?;
    Ok((
        out.to_path_buf(),
        tr!(
            "batch-written-note",
            width = width,
            height = height,
            size = byte_size(written.len() as u64)
        ),
    ))
}

/// The batch list.
#[derive(Debug)]
pub struct JobList {
    items: Vec<JobItem>,
    /// Where results go and what they are called.
    pub output: JobOutput,
    /// The picture page's settings and the group "Change every picture".
    pub picture: PictureSettings,
    /// Show the first result in the file manager when a run ends.
    pub reveal_after: bool,
    /// Workers, started on first use.
    pool: Option<JobPool>,
    /// `true` from [`Self::start`] until [`Self::poll`] reports the run.
    running: bool,
    /// Whether a folder's results stay in the cache: the platform's
    /// answer, or a stand-in in tests.
    stays: fn(&Path) -> bool,
}

impl Default for JobList {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            output: JobOutput::default(),
            picture: PictureSettings::default(),
            reveal_after: true,
            pool: None,
            running: false,
            stays: sue_platform::stays_in_cache,
        }
    }
}

impl JobList {
    /// An empty list with the defaults of `rice`: JPEG quality, JPEG XL
    /// effort and the output folder `io.convert_dir` (empty puts each
    /// result next to its file).
    #[must_use]
    pub fn from_rice(rice: &RiceSchema) -> Self {
        let folder = rice.get_str("io.convert_dir").unwrap_or("").trim();
        Self {
            picture: PictureSettings::from_rice(rice),
            output: JobOutput {
                folder: (!folder.is_empty()).then(|| PathBuf::from(folder)),
                ..JobOutput::default()
            },
            ..Self::default()
        }
    }

    /// Converts on `pool` instead of workers of its own (one that runs
    /// each job at once, say).
    #[must_use]
    pub fn with_pool(mut self, pool: JobPool) -> Self {
        self.pool = Some(pool);
        self
    }

    /// The files in the list.
    #[must_use]
    pub fn items(&self) -> &[JobItem] {
        &self.items
    }

    /// `true` while a file converts.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.items.iter().any(|item| item.status.working())
    }

    /// How many files a [`Self::start`] would convert.
    #[must_use]
    pub fn waiting(&self) -> usize {
        self.items
            .iter()
            .filter(|item| !item.status.working())
            .count()
    }

    /// Adds the picture file `path`, or the pictures directly in the
    /// folder `path` (at most [`FOLDER_LIMIT`]). Files already listed and
    /// files that are not pictures Sue reads are left out. Returns how
    /// many were added.
    pub fn add_path(&mut self, path: &Path) -> usize {
        if sue_platform::fs::is_dir(path) {
            let Ok(entries) = sue_platform::list_dir(path, false) else {
                return 0;
            };
            return entries
                .into_iter()
                .filter(|entry| !entry.is_dir && reads(&entry.path))
                .take(FOLDER_LIMIT)
                .map(|entry| self.add_file(&entry.path))
                .sum();
        }
        self.add_file(path)
    }

    fn add_file(&mut self, path: &Path) -> usize {
        if !reads(path) || self.items.iter().any(|item| item.path == path) {
            return 0;
        }
        self.items.push(JobItem {
            path: path.to_path_buf(),
            name: path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            size: sue_platform::fs::metadata(path).map_or(0, |meta| meta.len()),
            status: JobStatus::Waiting,
        });
        1
    }

    /// Takes the file at `index` off the list, unless it converts.
    pub fn remove(&mut self, index: usize) {
        if self
            .items
            .get(index)
            .is_some_and(|item| !item.status.working())
        {
            self.items.remove(index);
        }
    }

    /// Empties the list, unless a file converts.
    pub fn clear(&mut self) {
        if !self.busy() {
            self.items.clear();
        }
    }

    /// Puts every result into `folder` (a file's own folder, when a
    /// picker hands a file).
    pub fn set_folder(&mut self, folder: &Path) {
        let folder = if sue_platform::fs::is_dir(folder) {
            folder.to_path_buf()
        } else {
            folder.parent().map(Path::to_path_buf).unwrap_or_default()
        };
        self.output.folder = Some(folder);
    }

    /// `true` when results put next to their files would stay in the
    /// cache, where nobody sees them: a phone's picker gives Sue a copy
    /// of a file chosen on its own, and the place it came from takes no
    /// new files. `stays` says whether a folder is such a place.
    pub fn strands_next_to(&self, stays: impl Fn(&Path) -> bool) -> bool {
        self.output.folder.is_none()
            && self
                .items
                .iter()
                .any(|item| !item.status.working() && item.path.parent().is_some_and(&stays))
    }

    /// Converts every file that is not converting, in the background.
    pub fn start(&mut self) -> Start {
        if self.running || self.waiting() == 0 {
            return Start::Nothing;
        }
        if self.strands_next_to(self.stays) {
            return Start::NeedsFolder;
        }
        let extension = self.picture.extension();
        let pool = self.pool.get_or_insert_with(|| JobPool::new(2));
        // No result replaces a file of the list: a worker may not have
        // read it yet.
        let mut taken: HashSet<PathBuf> = self.items.iter().map(|item| item.path.clone()).collect();
        let mut started = 0;
        for item in &mut self.items {
            if item.status.working() {
                continue;
            }
            let out = self.output.path_for(&item.path, extension, &taken);
            taken.insert(out.clone());
            let source = item.path.clone();
            let settings = self.picture.clone();
            let job = pool.spawn(item.name.clone(), move |_| {
                convert(&source, &out, &settings)
            });
            item.status = JobStatus::Working(job);
            started += 1;
        }
        self.running = true;
        Start::Started(started)
    }

    /// Takes in finished conversions. Returns how the run went in the
    /// call that sees its last file finish, and `None` before and after.
    pub fn poll(&mut self) -> Option<RunSummary> {
        for item in &mut self.items {
            let JobStatus::Working(job) = &mut item.status else {
                continue;
            };
            if let Some(result) = job.try_take() {
                item.status = match result {
                    Ok((path, note)) => JobStatus::Done(path, note),
                    Err(JobError::Failed(why)) => JobStatus::Failed(why),
                    Err(error) => JobStatus::Failed(error.to_string()),
                };
            }
        }
        if !self.running || self.busy() {
            return None;
        }
        self.running = false;
        let mut summary = RunSummary {
            done: Vec::new(),
            failed: 0,
        };
        for item in &self.items {
            match &item.status {
                JobStatus::Done(path, _) => summary.done.push(path.clone()),
                JobStatus::Failed(_) => summary.failed += 1,
                JobStatus::Waiting | JobStatus::Working(_) => {}
            }
        }
        Some(summary)
    }

    /// The part of the files converted in this run, 0 to 1, while it
    /// runs.
    #[must_use]
    pub fn progress(&self) -> Option<f32> {
        if !self.running || self.items.is_empty() {
            return None;
        }
        let finished = self
            .items
            .iter()
            .filter(|item| matches!(item.status, JobStatus::Done(..) | JobStatus::Failed(_)))
            .count();
        Some(finished as f32 / self.items.len() as f32)
    }

    /// The label of the button that starts a run.
    #[must_use]
    pub fn convert_label(&self) -> String {
        match self.waiting() {
            0 => tr!("batch-convert"),
            count => tr!("batch-convert-count", count = count),
        }
    }

    /// What a row says under the file's name.
    fn detail(&self, item: &JobItem) -> String {
        match &item.status {
            JobStatus::Waiting => tr!(
                "batch-becomes",
                format = crate::export::format_name(self.picture.format)
            ),
            JobStatus::Working(_) => tr!("batch-converting"),
            JobStatus::Done(path, note) => tr!(
                "batch-done",
                value = path
                    .file_name()
                    .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
                note = note
            ),
            JobStatus::Failed(why) => tr!("batch-failed", why = one_line(why)),
        }
    }

    /// Draws the files, the output and the group "Change every picture".
    /// Returns what the host is asked to do.
    pub fn show(&mut self, ui: &mut Ui, theme: &Theme) -> Vec<JobListRequest> {
        let mut requests = Vec::new();
        let busy = self.busy();
        self.files_ui(ui, theme, busy, &mut requests);
        ui.add_space(theme.space(Space::S5));
        self.output_ui(ui, theme, &mut requests);
        ui.add_space(theme.space(Space::S5));
        self.changes_ui(ui, theme);
        requests
    }

    fn files_ui(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        busy: bool,
        requests: &mut Vec<JobListRequest>,
    ) {
        section(ui, theme, &tr!("batch-pictures"), |ui| {
            ui.horizontal_wrapped(|ui| {
                if button("sue-batch-add", &tr!("batch-add-files"))
                    .icon(Icon::Plus)
                    .show(ui, theme)
                    .clicked()
                {
                    requests.push(JobListRequest::AddFiles);
                }
                if button("sue-batch-add-folder", &tr!("batch-add-folder"))
                    .icon(Icon::Folder)
                    .show(ui, theme)
                    .clicked()
                {
                    requests.push(JobListRequest::AddFolder);
                }
                if !self.items.is_empty()
                    && button("sue-batch-clear", &tr!("batch-clear"))
                        .ghost()
                        .enabled(!busy)
                        .show(ui, theme)
                        .clicked()
                {
                    self.clear();
                }
            });
            ui.add_space(theme.space(Space::S3));
            if self.items.is_empty() {
                // A bounded room: the empty state centers itself in what
                // is left, which in a scrolling dialog would push the
                // output and the changes out of sight.
                let room = egui::vec2(ui.available_width(), theme.space(Space::S9) * 4.0);
                ui.allocate_ui(room, |ui| {
                    empty_state("sue-batch-empty", &tr!("batch-empty"))
                        .icon(Icon::Picture)
                        .detail(&tr!("batch-empty-detail"))
                        .show(ui, theme);
                });
                return;
            }
            if let Some(fraction) = self.progress() {
                ProgressBar::new(fraction)
                    .label(&tr!("batch-converting"))
                    .show(ui, theme);
                ui.add_space(theme.space(Space::S3));
            }
            let remove_label = tr!("batch-remove");
            let actions = [RowAction::new(Icon::Close, &remove_label)];
            let reveal =
                sue_platform::file_manager().map(|manager| tr!("batch-show-in", manager = manager));
            let mut remove = None;
            group(ui, theme, |ui| {
                for (index, item) in self.items.iter().enumerate() {
                    let detail = self.detail(item);
                    let size = if item.size > 0 {
                        byte_size(item.size)
                    } else {
                        String::new()
                    };
                    let icon = match item.status {
                        JobStatus::Done(..) => Icon::Check,
                        JobStatus::Failed(_) => Icon::Error,
                        JobStatus::Waiting | JobStatus::Working(_) => Icon::Picture,
                    };
                    let working = item.status.working();
                    let row = list_row(("sue-batch-item", &item.path), &item.name)
                        .icon(icon)
                        .detail(&detail)
                        .note(&size)
                        .actions(if working { &[] } else { &actions })
                        .show(ui, theme);
                    if row.action == Some(0) {
                        remove = Some(index);
                    } else if let JobStatus::Done(path, _) = &item.status
                        && let Some(reveal) = &reveal
                        && row.response.on_hover_text(reveal).clicked()
                    {
                        requests.push(JobListRequest::Reveal(path.clone()));
                    }
                }
            });
            if let Some(index) = remove {
                self.remove(index);
            }
        });
    }

    fn output_ui(&mut self, ui: &mut Ui, theme: &Theme, requests: &mut Vec<JobListRequest>) {
        let next_to = self.output.folder.is_none();
        let stranded = self.strands_next_to(self.stays);
        section(ui, theme, &tr!("batch-save-to"), |ui| {
            form_grid(ui, "sue-batch-output", theme, |form| {
                form.row(&tr!("batch-folder"), |ui| {
                    ui.vertical(|ui| {
                        if radio("sue-batch-next-to", &tr!("batch-next-to-each"), next_to)
                            .show(ui, theme)
                            .clicked()
                        {
                            self.output.folder = None;
                        }
                        if radio("sue-batch-one-folder", &tr!("batch-one-folder"), !next_to)
                            .show(ui, theme)
                            .clicked()
                            && next_to
                        {
                            requests.push(JobListRequest::ChooseFolder);
                        }
                    });
                });
                if let Some(folder) = &self.output.folder {
                    let shown = sue_platform::shown_path(folder);
                    form.row(&tr!("batch-chosen-folder"), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(shown);
                            if button("sue-batch-change", &tr!("batch-change"))
                                .ghost()
                                .show(ui, theme)
                                .clicked()
                            {
                                requests.push(JobListRequest::ChooseFolder);
                            }
                        });
                    });
                }
                if stranded {
                    form.wide(|ui| caption(ui, theme, &tr!("batch-picked-copies")));
                }
                form.row(&tr!("batch-file-name"), |ui| {
                    text_field("sue-batch-pattern", &mut self.output.pattern)
                        .hint("{name}")
                        .label(&tr!("batch-file-name"))
                        .show(ui, theme)
                        .on_hover_text(tr!("batch-pattern-hint"));
                });
                form.row(&tr!("batch-if-it-exists"), |ui| {
                    let options = [tr!("batch-keep-both"), tr!("batch-replace")];
                    let mut chosen = usize::from(self.output.replace);
                    if dropdown("sue-batch-replace", &mut chosen, &options)
                        .label(&tr!("batch-if-it-exists"))
                        .show(ui, theme)
                        .changed()
                    {
                        self.output.replace = chosen == 1;
                    }
                });
                if let Some(manager) = sue_platform::file_manager() {
                    form.wide(|ui| {
                        if checkbox(
                            "sue-batch-reveal",
                            &tr!("batch-reveal-after", manager = manager),
                            self.reveal_after,
                        )
                        .show(ui, theme)
                        .clicked()
                        {
                            self.reveal_after = !self.reveal_after;
                        }
                    });
                }
            });
        });
    }

    fn changes_ui(&mut self, ui: &mut Ui, theme: &Theme) {
        let options = &mut self.picture.options;
        section(ui, theme, &tr!("batch-change-every"), |ui| {
            form_grid(ui, "sue-batch-changes", theme, |form| {
                form.row(&tr!("batch-turn"), |ui| {
                    let turns = [
                        tr!("batch-turn-none"),
                        tr!("batch-turn-right"),
                        tr!("batch-turn-half"),
                        tr!("batch-turn-left"),
                    ];
                    if let Some(picked) = segmented(
                        "sue-batch-turn",
                        &turns,
                        Some(usize::from(options.quarter_turns % 4)),
                    )
                    .show(ui, theme)
                    .picked
                    {
                        options.quarter_turns = picked as u8;
                    }
                });
                form.row(&tr!("batch-mirror"), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if checkbox(
                            "sue-batch-flip-h",
                            &tr!("batch-left-to-right"),
                            options.flip_h,
                        )
                        .show(ui, theme)
                        .clicked()
                        {
                            options.flip_h = !options.flip_h;
                        }
                        if checkbox(
                            "sue-batch-flip-v",
                            &tr!("batch-top-to-bottom"),
                            options.flip_v,
                        )
                        .show(ui, theme)
                        .clicked()
                        {
                            options.flip_v = !options.flip_v;
                        }
                    });
                });
                for (label, salt, value) in [
                    (
                        tr!("batch-brightness"),
                        "sue-batch-brightness",
                        &mut options.brightness,
                    ),
                    (
                        tr!("batch-contrast"),
                        "sue-batch-contrast",
                        &mut options.contrast,
                    ),
                    (
                        tr!("batch-saturation"),
                        "sue-batch-saturation",
                        &mut options.saturation,
                    ),
                ] {
                    form.row(&label, |ui| {
                        let mut percent = f64::from(*value) * 100.0;
                        if slider(salt, &mut percent, -100.0..=100.0)
                            .step(1.0)
                            .suffix(" %")
                            .name(&label)
                            .show(ui, theme)
                            .changed()
                        {
                            *value = (percent / 100.0).clamp(-1.0, 1.0) as f32;
                        }
                    });
                }
            });
        });
    }

    /// Shows the whole Batch Convert Pictures dialog for one frame: the
    /// picture page of the export settings, then the list. Its accept
    /// button starts the run, Cancel closes it (a run going on goes on).
    /// Returns the answer and what the host is asked to do; a run that
    /// needs a folder first asks for one. While a run goes the host
    /// calls [`Self::poll`] each frame (the dialog keeps frames coming)
    /// and tells the person its [`RunSummary`].
    pub fn show_dialog(
        &mut self,
        ctx: &Context,
        theme: &Theme,
        title: &str,
    ) -> (Option<Answer>, Vec<JobListRequest>) {
        let label = self.convert_label();
        let can_start = self.waiting() > 0 && !self.running;
        let response = Dialog::new("sue-batch-convert", title, theme)
            .width(theme.space(Space::S9) * 14.0)
            .accept(label)
            .accept_enabled(can_start)
            .cancel_as(tr!("batch-close"))
            .resizable()
            .show(ctx, |ui| {
                section(ui, theme, &tr!("batch-convert-to"), |ui| {
                    picture_page(ui, theme, &mut self.picture, None);
                });
                ui.add_space(theme.space(Space::S5));
                self.show(ui, theme)
            });
        if self.running {
            // The statuses and the progress move while nothing else does.
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let mut requests = response.inner;
        if response.answer == Some(Answer::Accept) && self.start() == Start::NeedsFolder {
            requests.push(JobListRequest::ChooseFolder);
        }
        (response.answer, requests)
    }
}

/// A muted line in a form.
fn caption(ui: &mut Ui, theme: &Theme, text: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .font(theme.text_font(crate::ui::TextRole::Caption))
                .color(theme.text_muted()),
        )
        .wrap(),
    );
}

#[cfg(test)]
mod tests {
    use sue_core::{PixelFormat, RasterTile2D};

    use super::*;

    /// A fresh folder for one test.
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sue-batch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let pixels = (0..width * height)
            .flat_map(|index| [(index * 7 % 256) as u8, 40, 220, 255])
            .collect();
        let tile = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels).unwrap();
        sue_io::encode_png(&tile).unwrap()
    }

    #[test]
    fn outputs_go_next_to_their_file_and_never_over_it() {
        let dir = folder("out");
        let source = dir.join("photo.png");
        std::fs::write(&source, b"x").unwrap();
        let none = HashSet::new();
        let mut output = JobOutput {
            replace: true,
            ..JobOutput::default()
        };
        assert_eq!(
            output.path_for(&source, "jxl", &none),
            dir.join("photo.jxl")
        );
        // The same name and format would be the source itself: keep both.
        assert_eq!(
            output.path_for(&source, "png", &none),
            dir.join("photo (2).png")
        );
        output.pattern = "{name}-small".to_owned();
        output.folder = Some(dir.join("out"));
        assert_eq!(
            output.path_for(&source, "png", &none),
            dir.join("out").join("photo-small.png")
        );
        // A pattern names a file, never a folder.
        output.pattern = "../{name}".to_owned();
        assert_eq!(
            output.path_for(&source, "png", &none),
            dir.join("out").join(".._photo.png")
        );
        // Keep both: a file that exists gets a free sibling.
        output = JobOutput::default();
        std::fs::write(dir.join("photo.webp"), b"x").unwrap();
        assert_eq!(
            output.path_for(&source, "webp", &none),
            dir.join("photo (2).webp")
        );
        output.replace = true;
        assert_eq!(
            output.path_for(&source, "webp", &none),
            dir.join("photo.webp")
        );
        // Two results of one run never share a name, even replacing.
        let taken = HashSet::from([dir.join("photo.webp")]);
        assert_eq!(
            output.path_for(&dir.join("photo.jpg"), "webp", &taken),
            dir.join("photo (2).webp")
        );
        // A name that differs from the source's only in case is the
        // source on Windows: replacing never takes it.
        let shout = dir.join("Shout.PNG");
        std::fs::write(&shout, b"x").unwrap();
        assert_eq!(
            output.path_for(&shout, "png", &none),
            dir.join("Shout (2).png")
        );
        // Nor another file of the run, which a worker may still read.
        let sources = HashSet::from([dir.join("scan.jpg"), dir.join("scan.png")]);
        assert_eq!(
            output.path_for(&dir.join("scan.png"), "jpg", &sources),
            dir.join("scan (2).jpg")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copies_of_picked_files_ask_where_their_results_go() {
        // A stand-in for a phone's cache: the copy of a file picked on its
        // own is in `picked/1`, a chosen folder's files are in `picked/2`.
        let stays = |path: &Path| path == Path::new("picked/1");
        let item = |path: &str| JobItem {
            path: PathBuf::from(path),
            name: "Picture.png".to_owned(),
            size: 1,
            status: JobStatus::Waiting,
        };
        let mut list = JobList::default();
        list.items.push(item("picked/2/Song.png"));
        assert!(!list.strands_next_to(stays), "these land where they are");
        list.items.push(item("picked/1/Picture.png"));
        assert!(list.strands_next_to(stays));
        assert!(
            !list.strands_next_to(sue_platform::stays_in_cache),
            "a desktop writes next to every file"
        );
        // Already on its way, it asks nothing.
        let pool = JobPool::inline();
        list.items[1].status =
            JobStatus::Working(pool.spawn("x", |_| Ok((PathBuf::new(), String::new()))));
        assert!(!list.strands_next_to(stays));
        list.items[1].status = JobStatus::Done(PathBuf::new(), String::new());
        assert!(list.strands_next_to(stays));
        // With a folder chosen, every result goes there.
        list.output.folder = Some(PathBuf::from("picked/3"));
        assert!(!list.strands_next_to(stays));
        // And a run waits for that folder.
        list.output.folder = None;
        list.stays = |path| path == Path::new("picked/1");
        assert_eq!(list.start(), Start::NeedsFolder);
        let chosen = folder("chosen");
        list.set_folder(&chosen);
        assert_eq!(list.output.folder, Some(chosen.clone()));
        // A picker that hands a file gives its folder.
        list.set_folder(&chosen.join("picture.png"));
        assert_eq!(list.output.folder, Some(chosen.clone()));
        assert_eq!(
            list.start(),
            Start::Started(2),
            "the converted one again too"
        );
        while list.poll().is_none() {
            std::thread::yield_now();
        }
        let _ = std::fs::remove_dir_all(&chosen);
    }

    #[test]
    fn a_folder_of_pictures_converts_in_the_background() {
        let dir = folder("run");
        for name in ["a.png", "b.png", "c.png"] {
            std::fs::write(dir.join(name), png(8, 4)).unwrap();
        }
        std::fs::write(dir.join("notes.txt"), b"not a picture").unwrap();
        std::fs::write(dir.join("broken.png"), b"not a png either").unwrap();
        let mut list = JobList::default().with_pool(JobPool::new(2));
        assert_eq!(list.add_path(&dir), 4, "pictures only");
        assert_eq!(list.add_path(&dir.join("a.png")), 0, "once each");
        assert_eq!(list.add_path(&dir.join("notes.txt")), 0);
        assert!(list.picture.choose_format("jpg"));
        list.picture.options.quarter_turns = 1;
        list.picture.options.brightness = 0.1;
        list.output.pattern = "{name}-turned".to_owned();
        assert_eq!(list.convert_label(), tr!("batch-convert-count", count = 4));
        assert_eq!(list.start(), Start::Started(4));
        assert_eq!(list.start(), Start::Nothing, "one run at a time");
        let summary = loop {
            if let Some(summary) = list.poll() {
                break summary;
            }
            std::thread::yield_now();
        };
        assert_eq!(summary.done.len(), 3);
        assert_eq!(summary.failed, 1);
        assert!(list.poll().is_none(), "reported once");
        assert!(!summary.message().is_empty());
        for name in ["a", "b", "c"] {
            let out = dir.join(format!("{name}-turned.jpg"));
            let bytes = std::fs::read(&out).unwrap();
            let back = read_picture(&out, &bytes).unwrap();
            assert_eq!((back.width(), back.height()), (4, 8), "turned a quarter");
        }
        let failed = list
            .items()
            .iter()
            .find(|item| item.name == "broken.png")
            .unwrap();
        assert!(matches!(failed.status, JobStatus::Failed(_)));
        // A second run keeps both files.
        assert_eq!(list.start(), Start::Started(4));
        while list.poll().is_none() {
            std::thread::yield_now();
        }
        assert!(dir.join("a-turned (2).jpg").exists());
        list.remove(0);
        assert_eq!(list.items().len(), 3);
        list.clear();
        assert!(list.items().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_summary_counts_in_words_and_names_the_folder() {
        let summary = |done: &[&str], failed| RunSummary {
            done: done.iter().map(PathBuf::from).collect(),
            failed,
        };
        let one_folder = summary(&["out/a.jpg", "out/b.jpg"], 0).message();
        assert!(
            one_folder.starts_with("2 pictures converted into "),
            "{one_folder}"
        );
        assert!(!one_folder.contains("failed"), "{one_folder}");
        let one_failed = summary(&["out/a.jpg"], 1).message();
        assert!(
            one_failed.starts_with("1 picture converted into "),
            "{one_failed}"
        );
        assert!(one_failed.ends_with(", 1 failed"), "{one_failed}");
        let spread = summary(&["one/a.jpg", "two/b.jpg"], 0).message();
        assert_eq!(spread, "2 pictures converted next to their files");
    }

    #[test]
    fn a_run_never_writes_over_a_file_it_converts() {
        let dir = folder("sources");
        std::fs::write(dir.join("scan.png"), png(6, 6)).unwrap();
        let mut jpeg = PictureSettings::default();
        assert!(jpeg.choose_format("jpg"));
        let tile = read_picture(Path::new("scan.png"), &png(6, 6)).unwrap();
        let original = jpeg.export(&tile).unwrap();
        std::fs::write(dir.join("scan.jpg"), &original).unwrap();
        let mut list = JobList::default().with_pool(JobPool::inline());
        assert_eq!(list.add_path(&dir), 2);
        list.picture = jpeg;
        list.output.replace = true;
        assert_eq!(list.start(), Start::Started(2));
        let summary = list.poll().expect("an inline pool finishes at once");
        assert_eq!((summary.done.len(), summary.failed), (2, 0));
        assert_eq!(std::fs::read(dir.join("scan.jpg")).unwrap(), original);
        for name in ["scan (2).jpg", "scan (3).jpg"] {
            assert!(dir.join(name).exists(), "{name}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_folder_and_quality_start_from_the_rice() {
        let mut rice = RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
        assert_eq!(JobList::from_rice(&rice).output.folder, None);
        rice.set(
            "io.convert_dir",
            sue_rice::RiceValue::Text("N:/exports".to_owned()),
        )
        .unwrap();
        rice.set("io.jpeg_quality", sue_rice::RiceValue::Int(70))
            .unwrap();
        let list = JobList::from_rice(&rice);
        assert_eq!(list.output.folder, Some(PathBuf::from("N:/exports")));
        assert_eq!(list.picture.options.jpeg_quality, 70);
    }

    #[test]
    fn the_dialog_draws_an_empty_and_a_full_list() {
        let ctx = Context::default();
        let theme = Theme::from_rice(&RiceSchema::default());
        theme.motion.store(&ctx);
        let dir = folder("dialog");
        std::fs::write(dir.join("a.png"), png(4, 4)).unwrap();
        let mut list = JobList::default().with_pool(JobPool::inline());
        for fill in [false, true] {
            if fill {
                list.add_path(&dir);
            }
            for frame in 0..4 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 900.0),
                    )),
                    time: Some(f64::from(frame) * 0.1),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| {
                    let (answer, requests) = list.show_dialog(ui.ctx(), &theme, "Batch");
                    assert_eq!(answer, None);
                    assert!(requests.is_empty());
                });
                output.textures_delta.clear();
            }
        }
        assert_eq!(list.items().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
