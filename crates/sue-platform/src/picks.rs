//! File picks on phones and tablets, where the system's own pickers run
//! beside Sue and answer later, and people's files live in places Sue
//! reaches only through them (Android's documents, iOS's Files).
//!
//! Sue works with paths, so a chosen file is staged in Sue's cache: an
//! opened document is copied there and the copy's path is the answer,
//! and a file to save gets a path there whose writes [`Picker::deliver`]
//! hands back to the place the person chose. Everything that touches the
//! system goes through [`System`], so the whole flow runs headless in
//! tests with a stand-in.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::SystemTime;

use crate::os::{PickNews, Picking};

/// What a picker is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A file to open.
    Open,
    /// Where to save a file.
    Save,
    /// A folder.
    Folder,
}

/// How a system picker answered. A place is the system's own name for
/// a document or folder: an Android content URI, or a path on iOS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    /// The person chose this place.
    Chosen(String),
    /// The picker closed without a choice.
    Cancelled,
    /// The picker could not be shown or answer.
    Failed(String),
}

/// The system's own pickers and documents, as the [`Picker`] uses them.
/// `show`, `export` and `answer` run on the thread that polls; the
/// copies run on worker threads.
pub(crate) trait System: Send + Sync + 'static {
    /// Saving asks where first (Android); otherwise the file is written
    /// first and then offered to the export picker (iOS).
    const SAVE_ASKS_FIRST: bool;
    /// A chosen folder is a path Sue writes in directly (iOS), not a
    /// place to make documents in (Android).
    const FOLDERS_ARE_PATHS: bool;
    /// Writes to an opened document's copy go back to the document
    /// (Android); otherwise an opened file is only a copy (iOS).
    const WRITES_BACK: bool;

    /// Shows the picker; its answer comes from [`System::answer`].
    fn show(&self, kind: Kind, extensions: &[String], file_name: &str) -> Result<(), String>;

    /// The shown picker's answer, once it closed.
    fn answer(&self) -> Option<Answer>;

    /// The name the place shows people, e.g. `Picture.png`.
    fn name(&self, place: &str) -> String;

    /// Copies the document at `place` into `file`.
    fn copy_in(&self, place: &str, file: &Path) -> Result<(), String>;

    /// Writes `file` over the document at `place`.
    fn copy_out(&self, file: &Path, place: &str) -> Result<(), String>;

    /// Makes a document called `name` in the folder at `place`; its
    /// own place.
    fn create_in(&self, folder: &str, name: &str) -> Result<String, String> {
        let _ = (folder, name);
        Err("this system makes no documents in folders".to_owned())
    }

    /// The documents right inside the folder at `place`, folders left
    /// out: each one's place and name.
    fn list_in(&self, folder: &str) -> Result<Vec<(String, String)>, String> {
        let _ = folder;
        Ok(Vec::new())
    }

    /// Offers `file` to the export picker; its answer comes from
    /// [`System::answer`].
    fn export(&self, file: &Path) -> Result<(), String> {
        let _ = file;
        Err("this system has no export picker".to_owned())
    }
}

/// The most files of a chosen folder copied into Sue's cache, as many as
/// the Converter adds from a folder.
const FOLDER_FILES: usize = 500;

/// Where a staged file is handed back to.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Place {
    /// Written over this document.
    Document(String),
    /// Each file made inside becomes a document in this folder.
    Folder(String),
    /// Offered to the export picker once written.
    Export,
}

/// What a staged file's handover does.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    /// Writes over this document.
    Document(String),
    /// Makes a document called `name` in this folder.
    NewIn { folder: String, name: String },
    /// Offers the file to the export picker.
    Export,
}

/// A file's size and time of change, to tell when it was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

fn stamp(path: &Path) -> Option<Stamp> {
    fs::metadata(path)
        .ok()
        .filter(fs::Metadata::is_file)
        .map(|meta| Stamp {
            len: meta.len(),
            modified: meta.modified().ok(),
        })
}

/// A name a document shows, made safe as one file name in a folder.
fn file_name(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|ch| {
            if ch == '/' || ch == '\\' || ch.is_control() {
                '_'
            } else {
                ch
            }
        })
        .collect();
    let name = name.trim();
    if name.is_empty() || name.chars().all(|ch| ch == '.') {
        "file".to_owned()
    } else {
        name.to_owned()
    }
}

/// The files in Sue's cache that stand for places the person chose.
struct Staging {
    root: PathBuf,
    next: u64,
    kept: Vec<(PathBuf, Place)>,
    stamps: HashMap<PathBuf, Stamp>,
    /// Files in a chosen folder, with the documents made for them.
    made: HashMap<PathBuf, String>,
    /// Files being handed back right now.
    away: HashSet<PathBuf>,
}

impl Staging {
    /// Stages under `root`, which is emptied: what an earlier run left
    /// there stands for nothing any more.
    fn new(root: PathBuf) -> Self {
        let _ = fs::remove_dir_all(&root);
        Self {
            root,
            next: 0,
            kept: Vec::new(),
            stamps: HashMap::new(),
            made: HashMap::new(),
            away: HashSet::new(),
        }
    }

    /// A fresh path for a file called `name`, in a folder of its own so
    /// that it keeps its name.
    fn slot(&mut self, name: &str) -> io::Result<PathBuf> {
        self.next += 1;
        let dir = self.root.join(self.next.to_string());
        fs::create_dir_all(&dir)?;
        Ok(dir.join(file_name(name)))
    }

    /// Hands writes to `path` (a file, or each file in a folder) back to
    /// `place` from now on.
    fn keep(&mut self, path: PathBuf, place: Place) {
        let files = match place {
            Place::Folder(_) => Self::files_in(&path),
            Place::Document(_) | Place::Export => vec![path.clone()],
        };
        for file in files {
            if let Some(now) = stamp(&file) {
                self.stamps.insert(file, now);
            }
        }
        self.kept.push((path, place));
    }

    /// The files right inside a folder, without hidden ones (a write in
    /// progress is a hidden temporary file, see `write_atomic`).
    fn files_in(dir: &Path) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .collect();
        files.sort();
        files
    }

    /// The files written since they were last handed back, each marked
    /// away until [`Staging::landed`].
    fn changed(&mut self) -> Vec<(PathBuf, Target)> {
        let mut candidates = Vec::new();
        for (path, place) in &self.kept {
            match place {
                Place::Document(document) => {
                    candidates.push((path.clone(), Target::Document(document.clone())));
                }
                Place::Export => candidates.push((path.clone(), Target::Export)),
                Place::Folder(folder) => {
                    for file in Self::files_in(path) {
                        let target = match self.made.get(&file) {
                            Some(document) => Target::Document(document.clone()),
                            None => Target::NewIn {
                                folder: folder.clone(),
                                name: file
                                    .file_name()
                                    .map(|name| name.to_string_lossy().into_owned())
                                    .unwrap_or_default(),
                            },
                        };
                        candidates.push((file, target));
                    }
                }
            }
        }
        candidates.retain(|(file, _)| {
            if self.away.contains(file) {
                return false;
            }
            let Some(now) = stamp(file) else {
                return false;
            };
            if self.stamps.get(file) == Some(&now) {
                return false;
            }
            self.stamps.insert(file.clone(), now);
            self.away.insert(file.clone());
            true
        });
        candidates
    }

    /// A handover of `file` finished; `made` is the document made for it
    /// in a chosen folder.
    fn landed(&mut self, file: &Path, made: Option<String>) {
        self.away.remove(file);
        if let Some(document) = made {
            self.made.insert(file.to_path_buf(), document);
        }
    }
}

/// What a worker thread reports.
enum Landed {
    /// An opened document's copy is in place, or why not.
    Opened {
        file: PathBuf,
        place: String,
        result: Result<(), String>,
    },
    /// A staged file reached its place (with the document made for it).
    Delivered { file: PathBuf, made: Option<String> },
    /// A staged file could not reach its place.
    Undelivered { file: PathBuf, why: String },
    /// A chosen folder's files are copied into its staged folder: each
    /// with its document, and how many could not be read; or why not.
    Listed {
        dir: PathBuf,
        place: String,
        result: Result<(Vec<(PathBuf, String)>, usize), String>,
    },
}

/// What the picker is waiting for.
enum Asking {
    Pick(Kind),
    Export(PathBuf),
}

/// The picks of a phone or tablet: the open picker, the staged files and
/// the copies on their way.
pub(crate) struct Picker<S: System> {
    system: Arc<S>,
    staging: Staging,
    asking: Option<Asking>,
    exports: VecDeque<PathBuf>,
    flights: usize,
    send: Sender<Landed>,
    landed: Receiver<Landed>,
}

impl<S: System> Picker<S> {
    /// Picks through `system`, staging files under `root`.
    pub(crate) fn new(system: S, root: PathBuf) -> Self {
        let (send, landed) = channel();
        Self {
            system: Arc::new(system),
            staging: Staging::new(root),
            asking: None,
            exports: VecDeque::new(),
            flights: 0,
            send,
            landed,
        }
    }

    /// Asks the system's picker; mostly the answer comes from
    /// [`Picker::poll`] later.
    pub(crate) fn show(&mut self, kind: Kind, extensions: &[String], file_name: &str) -> Picking {
        if kind == Kind::Save && !S::SAVE_ASKS_FIRST {
            return match self.staging.slot(file_name) {
                Ok(file) => {
                    self.staging.keep(file.clone(), Place::Export);
                    Picking::Chosen(file)
                }
                Err(error) => Picking::Failed(error.to_string()),
            };
        }
        if matches!(self.asking, Some(Asking::Export(_))) {
            return Picking::Failed("finish choosing where the last file goes first".to_owned());
        }
        // A new pick replaces one whose answer never came.
        match self.system.show(kind, extensions, file_name) {
            Ok(()) => {
                self.asking = Some(Asking::Pick(kind));
                Picking::Waiting
            }
            Err(why) => Picking::Failed(why),
        }
    }

    /// The picker's answer and the copies that finished since last time.
    pub(crate) fn poll(&mut self) -> Vec<PickNews> {
        let mut news = Vec::new();
        if self.asking.is_some()
            && let Some(answer) = self.system.answer()
            && let Some(asking) = self.asking.take()
        {
            self.answered(asking, answer, &mut news);
        }
        while let Ok(landed) = self.landed.try_recv() {
            self.flights = self.flights.saturating_sub(1);
            match landed {
                Landed::Opened {
                    file,
                    place,
                    result: Ok(()),
                } => {
                    if S::WRITES_BACK {
                        self.staging.keep(file.clone(), Place::Document(place));
                    }
                    news.push(PickNews::Answered(Picking::Chosen(file)));
                }
                Landed::Opened {
                    result: Err(why), ..
                } => news.push(PickNews::Answered(Picking::Failed(why))),
                Landed::Delivered { file, made } => self.staging.landed(&file, made),
                Landed::Undelivered { file, why } => {
                    self.staging.landed(&file, None);
                    news.push(PickNews::Failed(format!(
                        "Cannot save {} where you chose: {why}",
                        shown(&file)
                    )));
                }
                Landed::Listed {
                    dir,
                    place,
                    result: Ok((copied, unread)),
                } => {
                    // A copied file written later goes back to its own
                    // document.
                    self.staging.made.extend(copied);
                    self.staging.keep(dir.clone(), Place::Folder(place));
                    if unread > 0 {
                        news.push(PickNews::Failed(format!(
                            "{unread} files in {} could not be read",
                            shown(&dir)
                        )));
                    }
                    news.push(PickNews::Answered(Picking::Chosen(dir)));
                }
                Landed::Listed {
                    result: Err(why), ..
                } => news.push(PickNews::Answered(Picking::Failed(why))),
            }
        }
        self.next_export(&mut news);
        news
    }

    fn answered(&mut self, asking: Asking, answer: Answer, news: &mut Vec<PickNews>) {
        match (asking, answer) {
            (Asking::Pick(_), Answer::Cancelled) => {
                news.push(PickNews::Answered(Picking::Cancelled));
            }
            (Asking::Pick(_), Answer::Failed(why)) => {
                news.push(PickNews::Answered(Picking::Failed(why)));
            }
            (Asking::Pick(Kind::Open), Answer::Chosen(place)) => {
                match self.staging.slot(&self.system.name(&place)) {
                    Ok(file) => self.spawn(
                        move |system, file| {
                            let result = system.copy_in(&place, &file);
                            Landed::Opened {
                                file,
                                place,
                                result,
                            }
                        },
                        file,
                    ),
                    Err(error) => {
                        news.push(PickNews::Answered(Picking::Failed(error.to_string())));
                    }
                }
            }
            (Asking::Pick(Kind::Save), Answer::Chosen(place)) => {
                let answer = match self.staging.slot(&self.system.name(&place)) {
                    Ok(file) => {
                        self.staging.keep(file.clone(), Place::Document(place));
                        Picking::Chosen(file)
                    }
                    Err(error) => Picking::Failed(error.to_string()),
                };
                news.push(PickNews::Answered(answer));
            }
            (Asking::Pick(Kind::Folder), Answer::Chosen(place)) => {
                if S::FOLDERS_ARE_PATHS {
                    news.push(PickNews::Answered(Picking::Chosen(PathBuf::from(place))));
                    return;
                }
                // The folder's files are copied in first, so that what
                // reads the folder finds them.
                match self
                    .staging
                    .slot(&self.system.name(&place))
                    .and_then(|dir| {
                        fs::create_dir_all(&dir)?;
                        Ok(dir)
                    }) {
                    Ok(dir) => self.spawn(
                        move |system, dir| {
                            let result = system
                                .list_in(&place)
                                .map(|documents| copy_folder(system, &dir, documents));
                            Landed::Listed { dir, place, result }
                        },
                        dir,
                    ),
                    Err(error) => {
                        news.push(PickNews::Answered(Picking::Failed(error.to_string())));
                    }
                }
            }
            (Asking::Export(file), answer) => {
                self.staging.landed(&file, None);
                if let Answer::Failed(why) = answer {
                    news.push(PickNews::Failed(format!(
                        "Cannot save {}: {why}",
                        shown(&file)
                    )));
                }
            }
        }
    }

    /// Hands every staged file written since last time back to the place
    /// the person chose. Call it after writing.
    pub(crate) fn deliver(&mut self) {
        for (file, target) in self.staging.changed() {
            match target {
                Target::Export => self.exports.push_back(file),
                Target::Document(document) => self.spawn(
                    move |system, file| match system.copy_out(&file, &document) {
                        Ok(()) => Landed::Delivered { file, made: None },
                        Err(why) => Landed::Undelivered { file, why },
                    },
                    file,
                ),
                Target::NewIn { folder, name } => self.spawn(
                    move |system, file| {
                        let made = system.create_in(&folder, &name).and_then(|document| {
                            system.copy_out(&file, &document)?;
                            Ok(document)
                        });
                        match made {
                            Ok(document) => Landed::Delivered {
                                file,
                                made: Some(document),
                            },
                            Err(why) => Landed::Undelivered { file, why },
                        }
                    },
                    file,
                ),
            }
        }
    }

    /// `true` while a picker is open or a copy is on its way, so the
    /// caller keeps polling.
    pub(crate) fn busy(&self) -> bool {
        self.asking.is_some() || self.flights > 0 || !self.exports.is_empty()
    }

    /// Shows the export picker for the next written file, once no picker
    /// is open.
    fn next_export(&mut self, news: &mut Vec<PickNews>) {
        if self.asking.is_some() {
            return;
        }
        let Some(file) = self.exports.pop_front() else {
            return;
        };
        match self.system.export(&file) {
            Ok(()) => self.asking = Some(Asking::Export(file)),
            Err(why) => {
                self.staging.landed(&file, None);
                news.push(PickNews::Failed(format!(
                    "Cannot save {}: {why}",
                    shown(&file)
                )));
            }
        }
    }

    /// Runs a copy for `file` on a worker thread.
    fn spawn<F>(&mut self, work: F, file: PathBuf)
    where
        F: FnOnce(&S, PathBuf) -> Landed + Send + 'static,
    {
        let system = Arc::clone(&self.system);
        let send = self.send.clone();
        let job_file = file.clone();
        let spawned = std::thread::Builder::new()
            .name("sue-files".to_owned())
            .spawn(move || {
                let _ = send.send(work(&system, job_file));
            });
        match spawned {
            Ok(_) => self.flights += 1,
            Err(error) => {
                let _ = self.send.send(Landed::Undelivered {
                    file,
                    why: error.to_string(),
                });
                self.flights += 1;
            }
        }
    }
}

/// Copies a chosen folder's documents into its staged folder `dir`: the
/// files copied, each with its document, and how many could not be read.
/// Hidden files are left out, and a name met twice keeps its first file.
fn copy_folder<S: System>(
    system: &S,
    dir: &Path,
    documents: Vec<(String, String)>,
) -> (Vec<(PathBuf, String)>, usize) {
    let mut copied = Vec::new();
    let mut unread = 0;
    for (document, name) in documents.into_iter().take(FOLDER_FILES) {
        let name = file_name(&name);
        let file = dir.join(&name);
        if name.starts_with('.') || file.exists() {
            continue;
        }
        match system.copy_in(&document, &file) {
            Ok(()) => copied.push((file, document)),
            Err(_) => {
                let _ = fs::remove_file(&file);
                unread += 1;
            }
        }
    }
    (copied, unread)
}

/// A staged file's name, for messages.
fn shown(file: &Path) -> String {
    file.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// The picker of this phone or tablet, staging in Sue's cache.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) fn native() -> std::sync::MutexGuard<'static, Picker<Native>> {
    use std::sync::{Mutex, OnceLock, PoisonError};
    static PICKER: OnceLock<Mutex<Picker<Native>>> = OnceLock::new();
    PICKER
        .get_or_init(|| {
            let root = crate::PlatformPaths::detect().map_or_else(
                |_| std::env::temp_dir().join("sue-picked"),
                |paths| paths.cache_dir.join("picked"),
            );
            Mutex::new(Picker::new(Native::default(), root))
        })
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

#[cfg(target_os = "android")]
type Native = crate::android::Documents;
#[cfg(target_os = "ios")]
type Native = crate::ios::Files;

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use super::*;

    /// A stand-in for the system: documents are files under a folder,
    /// places are their paths, and the person's next answer is set by
    /// the test.
    #[derive(Default)]
    struct Stand {
        shown: Mutex<Vec<(Kind, Vec<String>, String)>>,
        exported: Mutex<Vec<PathBuf>>,
        next: Mutex<Option<Answer>>,
    }

    impl Stand {
        fn answer_with(&self, answer: Answer) {
            *self.next.lock().unwrap() = Some(answer);
        }
    }

    macro_rules! stand {
        ($name:ident, $asks_first:expr, $folders_are_paths:expr, $writes_back:expr) => {
            #[derive(Default)]
            struct $name(Stand);

            impl System for $name {
                const SAVE_ASKS_FIRST: bool = $asks_first;
                const FOLDERS_ARE_PATHS: bool = $folders_are_paths;
                const WRITES_BACK: bool = $writes_back;

                fn show(
                    &self,
                    kind: Kind,
                    extensions: &[String],
                    file_name: &str,
                ) -> Result<(), String> {
                    self.0.shown.lock().unwrap().push((
                        kind,
                        extensions.to_vec(),
                        file_name.to_owned(),
                    ));
                    Ok(())
                }

                fn answer(&self) -> Option<Answer> {
                    self.0.next.lock().unwrap().take()
                }

                fn name(&self, place: &str) -> String {
                    Path::new(place)
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                }

                fn copy_in(&self, place: &str, file: &Path) -> Result<(), String> {
                    fs::copy(place, file)
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                }

                fn copy_out(&self, file: &Path, place: &str) -> Result<(), String> {
                    fs::copy(file, place)
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                }

                fn create_in(&self, folder: &str, name: &str) -> Result<String, String> {
                    let path = Path::new(folder).join(name);
                    fs::write(&path, b"").map_err(|error| error.to_string())?;
                    Ok(path.to_string_lossy().into_owned())
                }

                fn list_in(&self, folder: &str) -> Result<Vec<(String, String)>, String> {
                    let entries = fs::read_dir(folder).map_err(|error| error.to_string())?;
                    Ok(entries
                        .flatten()
                        .filter(|entry| entry.path().is_file())
                        .map(|entry| {
                            (
                                entry.path().to_string_lossy().into_owned(),
                                entry.file_name().to_string_lossy().into_owned(),
                            )
                        })
                        .collect())
                }

                fn export(&self, file: &Path) -> Result<(), String> {
                    self.0.exported.lock().unwrap().push(file.to_path_buf());
                    Ok(())
                }
            }
        };
    }

    stand!(Android, true, false, true);
    stand!(Ios, false, true, false);

    fn scratch(tag: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("sue-picks-{tag}-{}-{unique}", std::process::id()));
        fs::create_dir_all(dir.join("documents")).unwrap();
        dir
    }

    /// Polls until the picker settles, gathering what it said.
    fn settle<S: System>(picker: &mut Picker<S>) -> Vec<PickNews> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut news = Vec::new();
        loop {
            news.extend(picker.poll());
            if picker.flights == 0 || Instant::now() > deadline {
                return news;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn chosen(news: &[PickNews]) -> PathBuf {
        match news {
            [PickNews::Answered(Picking::Chosen(path))] => path.clone(),
            other => panic!("expected one chosen path, got {other:?}"),
        }
    }

    /// Writes again with a different size, so the stamp surely changes.
    fn write(path: &Path, text: &str) {
        crate::write_atomic(path, text.as_bytes()).unwrap();
    }

    #[test]
    fn android_opens_a_copy_and_writes_it_back() {
        let dir = scratch("open");
        let document = dir.join("documents").join("Picture.png");
        fs::write(&document, b"pixels").unwrap();
        let mut picker = Picker::new(Android::default(), dir.join("cache"));

        let asked = picker.show(Kind::Open, &["png".to_owned()], "");
        assert_eq!(asked, Picking::Waiting);
        assert!(picker.busy());
        assert!(picker.poll().is_empty(), "nothing until the person answers");
        picker
            .system
            .0
            .answer_with(Answer::Chosen(document.to_string_lossy().into_owned()));
        let copy = chosen(&settle(&mut picker));
        assert_eq!(copy.file_name().unwrap(), "Picture.png");
        assert!(copy.starts_with(dir.join("cache")));
        assert_eq!(fs::read(&copy).unwrap(), b"pixels");
        assert!(!picker.busy());

        // Untouched, nothing goes back; a save goes back once.
        picker.deliver();
        assert!(!picker.busy());
        write(&copy, "pixels, painted");
        picker.deliver();
        assert!(settle(&mut picker).is_empty());
        assert_eq!(fs::read(&document).unwrap(), b"pixels, painted");
        picker.deliver();
        assert!(!picker.busy(), "written once, handed back once");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn android_asks_where_to_save_first() {
        let dir = scratch("save");
        let document = dir.join("documents").join("Novel (1).docx");
        fs::write(&document, b"").unwrap();
        let mut picker = Picker::new(Android::default(), dir.join("cache"));

        assert_eq!(
            picker.show(Kind::Save, &["docx".to_owned()], "Novel.docx"),
            Picking::Waiting
        );
        assert_eq!(
            picker.system.0.shown.lock().unwrap()[0],
            (Kind::Save, vec!["docx".to_owned()], "Novel.docx".to_owned())
        );
        picker
            .system
            .0
            .answer_with(Answer::Chosen(document.to_string_lossy().into_owned()));
        let file = chosen(&picker.poll());
        // The name the system gave the document wins: it says the format.
        assert_eq!(file.file_name().unwrap(), "Novel (1).docx");

        write(&file, "chapter one");
        picker.deliver();
        assert!(settle(&mut picker).is_empty());
        assert_eq!(fs::read(&document).unwrap(), b"chapter one");
        // Saving again (Ctrl+S) writes the same path and goes back again.
        write(&file, "chapter one, two");
        picker.deliver();
        assert!(settle(&mut picker).is_empty());
        assert_eq!(fs::read(&document).unwrap(), b"chapter one, two");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn android_reads_and_makes_files_in_a_chosen_folder() {
        let dir = scratch("folder");
        let folder = dir.join("documents");
        fs::write(folder.join("Song.wav"), b"samples").unwrap();
        fs::create_dir(folder.join("Albums")).unwrap();
        let mut picker = Picker::new(Android::default(), dir.join("cache"));

        assert_eq!(picker.show(Kind::Folder, &[], ""), Picking::Waiting);
        picker
            .system
            .0
            .answer_with(Answer::Chosen(folder.to_string_lossy().into_owned()));
        let staged = chosen(&settle(&mut picker));
        assert!(staged.is_dir());

        // What is in the folder is there to read, folders left out, and
        // untouched it goes nowhere.
        assert_eq!(fs::read(staged.join("Song.wav")).unwrap(), b"samples");
        assert!(!staged.join("Albums").exists());
        picker.deliver();
        assert!(!picker.busy());
        // Written, it goes back to its own document.
        write(&staged.join("Song.wav"), "samples, mixed");
        picker.deliver();
        assert!(settle(&mut picker).is_empty());
        assert_eq!(
            fs::read(folder.join("Song.wav")).unwrap(),
            b"samples, mixed"
        );

        write(&staged.join("a.jxl"), "one");
        fs::write(staged.join(".b.jxl.1.tmp"), b"half written").unwrap();
        picker.deliver();
        assert!(settle(&mut picker).is_empty());
        assert_eq!(fs::read(folder.join("a.jxl")).unwrap(), b"one");
        assert!(
            !folder.join(".b.jxl.1.tmp").exists(),
            "temporary files stay"
        );

        // Written again, the same document is updated, not made twice.
        write(&staged.join("a.jxl"), "one again");
        picker.deliver();
        assert!(settle(&mut picker).is_empty());
        assert_eq!(fs::read(folder.join("a.jxl")).unwrap(), b"one again");
        // Song.wav, Albums and a.jxl: nothing made twice.
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 3);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn cancelled_and_failed_picks_say_so() {
        let dir = scratch("cancel");
        let mut picker = Picker::new(Android::default(), dir.join("cache"));
        picker.show(Kind::Open, &[], "");
        picker.system.0.answer_with(Answer::Cancelled);
        assert_eq!(picker.poll(), [PickNews::Answered(Picking::Cancelled)]);
        assert!(!picker.busy());

        picker.show(Kind::Open, &[], "");
        let gone = dir.join("documents").join("gone.png");
        picker
            .system
            .0
            .answer_with(Answer::Chosen(gone.to_string_lossy().into_owned()));
        assert!(matches!(
            settle(&mut picker).as_slice(),
            [PickNews::Answered(Picking::Failed(_))]
        ));

        // A copy that cannot reach its place is reported.
        picker.show(Kind::Save, &[], "x.png");
        let missing = dir.join("no such folder").join("x.png");
        picker
            .system
            .0
            .answer_with(Answer::Chosen(missing.to_string_lossy().into_owned()));
        let file = chosen(&picker.poll());
        write(&file, "x");
        picker.deliver();
        assert!(matches!(
            settle(&mut picker).as_slice(),
            [PickNews::Failed(why)] if why.starts_with("Cannot save x.png")
        ));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ios_saves_first_and_then_offers_the_export_picker() {
        let dir = scratch("ios");
        let mut picker = Picker::new(Ios::default(), dir.join("cache"));

        let Picking::Chosen(file) = picker.show(Kind::Save, &["sue".to_owned()], "Untitled.sue")
        else {
            panic!("iOS saves to Sue's cache first");
        };
        assert!(picker.system.0.shown.lock().unwrap().is_empty());
        assert!(!picker.busy());
        write(&file, "project");
        picker.deliver();
        assert!(picker.poll().is_empty());
        assert_eq!(
            *picker.system.0.exported.lock().unwrap(),
            std::slice::from_ref(&file)
        );
        assert!(picker.busy(), "waiting for the export picker");
        picker
            .system
            .0
            .answer_with(Answer::Chosen("/Files/Untitled.sue".to_owned()));
        assert!(picker.poll().is_empty());
        assert!(!picker.busy());

        // A chosen folder is a path to write in directly.
        picker.show(Kind::Folder, &[], "");
        picker
            .system
            .0
            .answer_with(Answer::Chosen("/Files/Exports".to_owned()));
        assert_eq!(chosen(&picker.poll()), PathBuf::from("/Files/Exports"));

        // An opened file is a copy that nothing writes back.
        let picked = dir.join("documents").join("Song.wav");
        fs::write(&picked, b"samples").unwrap();
        picker.show(Kind::Open, &[], "");
        picker
            .system
            .0
            .answer_with(Answer::Chosen(picked.to_string_lossy().into_owned()));
        let copy = chosen(&settle(&mut picker));
        write(&copy, "samples, edited");
        picker.deliver();
        assert!(!picker.busy());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn names_from_documents_stay_one_file_name() {
        assert_eq!(file_name("a/b\\c.png"), "a_b_c.png");
        assert_eq!(file_name(".."), "file");
        assert_eq!(file_name("  "), "file");
        assert_eq!(file_name("Song (1).wav"), "Song (1).wav");
    }
}
