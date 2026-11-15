//! Exports that run in the background: a movie, an animated picture, a
//! sequence of pictures, the cels and the timelapse are made on a job of
//! the shared transfers ([`sue_wm::Transfers::spawn_noticed`]), so the
//! window keeps drawing and the canvas keeps taking strokes while they
//! run.
//!
//! - **A snapshot.** An export works on a copy of the document taken
//!   when it starts (`Arena::clone`, which shares every raster by
//!   reference count, D21), so what is drawn meanwhile is not in it and
//!   never changes under it.
//! - **Progress and Cancel.** The window shows a notice with the work's
//!   progress and a Cancel button, and the Hub's Transfers list has it
//!   too. A cancelled export writes nothing: a single file is written
//!   only once the whole of it is made, and the files of a sequence are
//!   kept under temporary names ([`Staged`]) until the last one is made,
//!   and taken away when it stops early.
//! - **Done.** A notice says where the file went and offers "Show in
//!   Folder"; a failure says why.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use sue_core::JobContext;
use sue_wm::{AppletContext, TransferKind, TransferState};

use super::SuePaint;

/// What an export made, told when it ends well.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Done {
    /// The frames written (a movie's, an animation's), or 0.
    pub frames: usize,
    /// The bytes of a single file, or 0.
    pub bytes: usize,
    /// How many files were written.
    pub files: usize,
}

/// Which export, for what is said when it ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum What {
    /// A movie (MP4 or AVI).
    Movie,
    /// An animated GIF, PNG or WebP.
    Animation,
    /// Numbered pictures, one a frame.
    Sequence,
    /// The animation cels.
    Cels,
    /// The timelapse video or GIF.
    Timelapse,
}

impl What {
    /// What a failure says, `error` already in the reader's words.
    fn failed(self, error: &str) -> String {
        match self {
            Self::Movie => tr!(
                "cannot-export-the-movie",
                error = sue_lang::translate("io", error)
            ),
            Self::Animation => tr!("cannot-export-the-animation", error = error.to_owned()),
            Self::Sequence | Self::Cels => tr!("cannot-export", error = error.to_owned()),
            Self::Timelapse => tr!(
                "cannot-export-the-timelapse",
                error = sue_lang::translate("io", error)
            ),
        }
    }
}

/// An export that runs.
#[derive(Debug)]
struct Running {
    /// Its transfer.
    transfer: u64,
    what: What,
    /// The file it writes (a sequence's first).
    path: PathBuf,
    /// What it made, once it has.
    done: Arc<Mutex<Option<Done>>>,
}

/// The exports that run.
#[derive(Debug, Default)]
pub(crate) struct Exports {
    running: Vec<Running>,
}

impl Exports {
    /// `true` while an export runs.
    #[must_use]
    pub(crate) fn busy(&self) -> bool {
        !self.running.is_empty()
    }
}

/// What a running export sees: its progress and whether it was asked to
/// stop. [`Steps::NOW`] is an export made at once, on the calling thread.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Steps<'a>(Option<&'a JobContext>);

impl Steps<'_> {
    /// An export made at once: no progress, never stopped.
    #[cfg(test)]
    pub(crate) const NOW: Steps<'static> = Steps(None);

    /// Says how far it is, 0 to 1.
    pub(crate) fn set(self, fraction: f32) {
        if let Some(job) = self.0 {
            job.set_progress(fraction);
        }
    }

    /// Says it did `done` of `of` parts of its work, in the share
    /// `0..span` of the whole.
    pub(crate) fn part(self, done: usize, of: usize, span: f32) {
        self.set(done as f32 / of.max(1) as f32 * span);
    }

    /// Stops the work when it was asked to.
    ///
    /// # Errors
    /// When the export was cancelled.
    pub(crate) fn go_on(self) -> Result<(), String> {
        if self.0.is_some_and(JobContext::is_cancelled) {
            Err(tr!("export-was-cancelled"))
        } else {
            Ok(())
        }
    }
}

/// Files an export may not finish yet (scripted runs and tests): the
/// export makes everything and then waits, before it writes, until its
/// file is let go or it is cancelled.
static HELD: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Holds every export to `path` just before it writes (`hold`), or lets
/// it go; scripted runs and tests use it to see an export while it runs.
#[doc(hidden)]
pub fn hold_exports_to(path: &Path, hold: bool) {
    let mut held = HELD.lock().unwrap_or_else(PoisonError::into_inner);
    held.retain(|kept| kept != path);
    if hold {
        held.push(path.to_path_buf());
    }
}

/// Waits while `path` is held, then goes on unless it was cancelled.
///
/// # Errors
/// When the export was cancelled.
pub(crate) fn before_writing(path: &Path, steps: Steps<'_>) -> Result<(), String> {
    let held = || {
        HELD.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|kept| kept == path)
    };
    while steps.0.is_some() && held() && steps.go_on().is_ok() {
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    steps.go_on()
}

/// One file, written whole: only once all of it is made and the export
/// was not cancelled (see [`before_writing`]).
///
/// # Errors
/// When it was cancelled or will not write.
pub(crate) fn write_whole(path: &Path, bytes: &[u8], steps: Steps<'_>) -> Result<(), String> {
    before_writing(path, steps)?;
    sue_platform::write_atomic(path, bytes).map_err(|error| error.to_string())
}

/// The files of an export that writes many: each kept under a hidden
/// temporary name beside its place while the others are made, then all
/// put in place by [`Staged::finish`]. Dropped before that (cancelled,
/// failed), it takes away every file it wrote and every folder it made,
/// so nothing of a part export is left.
#[derive(Debug, Default)]
pub(crate) struct Staged {
    /// Each file's temporary name and its place.
    files: Vec<(PathBuf, PathBuf)>,
    /// The folders it made, the first made first.
    folders: Vec<PathBuf>,
}

impl Staged {
    /// The temporary name of `target`: hidden, beside it.
    fn temporary(target: &Path) -> PathBuf {
        let name = target
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        target.with_file_name(format!(".{name}.sue-part"))
    }

    /// Makes the folder `dir` (and those above it) where it is missing.
    ///
    /// # Errors
    /// When it cannot be made.
    pub(crate) fn folder(&mut self, dir: &Path) -> Result<(), String> {
        let mut missing = Vec::new();
        let mut at = Some(dir);
        while let Some(folder) = at {
            if folder.as_os_str().is_empty() || sue_platform::fs::is_dir(folder) {
                break;
            }
            missing.push(folder.to_path_buf());
            at = folder.parent();
        }
        sue_platform::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        missing.reverse();
        self.folders.extend(missing);
        Ok(())
    }

    /// Writes `bytes` for `target`, under its temporary name.
    ///
    /// # Errors
    /// When it will not write.
    pub(crate) fn write(&mut self, target: &Path, bytes: &[u8]) -> Result<(), String> {
        let temporary = Self::temporary(target);
        sue_platform::fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
        self.files.push((temporary, target.to_path_buf()));
        Ok(())
    }

    /// How many files it holds.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.files.len()
    }

    /// Puts every file in its place, once the export may write
    /// ([`before_writing`] on `path`).
    ///
    /// # Errors
    /// When it was cancelled, or a file will not go in its place (those
    /// already in place stay).
    pub(crate) fn finish(mut self, path: &Path, steps: Steps<'_>) -> Result<usize, String> {
        before_writing(path, steps)?;
        let files = std::mem::take(&mut self.files);
        let count = files.len();
        let mut left = files.into_iter();
        for (temporary, target) in left.by_ref() {
            if let Err(error) = sue_platform::fs::rename(&temporary, &target) {
                let _ = sue_platform::fs::remove_file(&temporary);
                self.files = left.collect();
                return Err(error.to_string());
            }
        }
        self.folders.clear();
        Ok(count)
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        for (temporary, _) in self.files.drain(..) {
            let _ = sue_platform::fs::remove_file(&temporary);
        }
        // The deepest first; a folder something else put a file in
        // meanwhile stays.
        for folder in self.folders.drain(..).rev() {
            let empty =
                sue_platform::list_dir(&folder, true).is_ok_and(|entries| entries.is_empty());
            if empty {
                let _ = sue_platform::fs::remove_dir_all(&folder);
            }
        }
    }
}

/// How an export ended.
enum Ended {
    /// It wrote what it made.
    Wrote(Done),
    /// It was asked to stop and wrote nothing.
    Stopped,
    /// It failed, and why.
    Failed(String),
}

thread_local! {
    /// Exports on this thread are made at once instead of in the
    /// background ([`export_at_once`]).
    static AT_ONCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Makes the exports this thread starts run at once, on it, the window
/// waiting (`true`), or in the background again (`false`): the
/// synchronous export tests compare the background one with.
#[doc(hidden)]
pub fn export_at_once(at_once: bool) {
    AT_ONCE.with(|cell| cell.set(at_once));
}

impl SuePaint {
    /// Runs `work`, an export to `path`, in the background: the window
    /// shows its progress and a Cancel button, and tells how it went when
    /// it ends ([`Self::poll_exports`]).
    pub(crate) fn run_export<F>(
        &mut self,
        cx: &mut AppletContext<'_>,
        what: What,
        path: &Path,
        work: F,
    ) where
        F: FnOnce(Steps<'_>) -> Result<Done, String> + Send + 'static,
    {
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        if AT_ONCE.with(std::cell::Cell::get) {
            let ended = match work(Steps(None)) {
                Ok(done) => Ended::Wrote(done),
                Err(why) => {
                    cx.transfers.failed(
                        TransferKind::Export,
                        &name,
                        Some(path.to_path_buf()),
                        &why,
                    );
                    Ended::Failed(why)
                }
            };
            tell(cx, what, path, ended);
            return;
        }
        let done: Arc<Mutex<Option<Done>>> = Arc::default();
        let into = Arc::clone(&done);
        let transfer = cx.transfers.spawn_noticed(
            TransferKind::Export,
            &name,
            Some(path.to_path_buf()),
            move |job| {
                let made = work(Steps(Some(job)))?;
                *into.lock().unwrap_or_else(PoisonError::into_inner) = Some(made);
                Ok(())
            },
        );
        self.exports.running.push(Running {
            transfer,
            what,
            path: path.to_path_buf(),
            done,
        });
    }

    /// Tells how each export that ended went ([`tell`]).
    pub(crate) fn poll_exports(&mut self, cx: &mut AppletContext<'_>) {
        if self.exports.running.is_empty() {
            return;
        }
        let mut ended = Vec::new();
        self.exports.running.retain(|running| {
            let state = cx
                .transfers
                .get(running.transfer)
                .map(|found| found.state.clone());
            let done = || {
                running
                    .done
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take()
            };
            let how = match state {
                Some(TransferState::Running(_)) => return true,
                Some(TransferState::Done) => done().map(Ended::Wrote),
                Some(TransferState::Cancelled) => Some(Ended::Stopped),
                // A job's own error reads "failed: why" in the list; the
                // notice says only why.
                Some(TransferState::Failed(why)) => Some(Ended::Failed(
                    why.strip_prefix("failed: ").unwrap_or(&why).to_owned(),
                )),
                None => None,
            };
            if let Some(how) = how {
                ended.push((running.what, running.path.clone(), how));
            }
            false
        });
        for (what, path, how) in ended {
            tell(cx, what, &path, how);
        }
    }
}

/// Says how an export to `path` ended: where its file is, with "Show in
/// Folder"; that it stopped and wrote nothing; or why it failed.
fn tell(cx: &mut AppletContext<'_>, what: What, path: &Path, how: Ended) {
    match how {
        Ended::Wrote(done) if done.files == 0 => {
            cx.notify(tr!("exported-cels", written = 0));
        }
        Ended::Wrote(done) => {
            let note = match what {
                What::Movie | What::Animation | What::Timelapse => tr!(
                    "frames-kib",
                    value = done.bytes.div_ceil(1024),
                    count = done.frames
                ),
                What::Sequence | What::Cels if done.files > 1 => {
                    tr!("and-more-files", count = done.files - 1)
                }
                What::Sequence | What::Cels => String::new(),
            };
            cx.saved(path, note);
        }
        Ended::Stopped => {
            let name = path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            cx.notify(tr!("export-cancelled", name = name));
        }
        Ended::Failed(why) => cx.fail(what.failed(&why)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sue-exports-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn staged_files_appear_together_or_not_at_all() {
        let dir = scratch("staged");
        std::fs::create_dir_all(&dir).unwrap();
        // Dropped before it finished: nothing is left, not even the
        // folders it made; a file that was there stays as it was.
        std::fs::write(dir.join("keep.png"), b"old").unwrap();
        {
            let mut staged = Staged::default();
            staged.folder(&dir.join("Walk").join("A")).unwrap();
            staged
                .write(&dir.join("Walk").join("A").join("1.png"), b"one")
                .unwrap();
            staged.write(&dir.join("keep.png"), b"new").unwrap();
            assert_eq!(staged.len(), 2);
        }
        let left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(left, vec![std::ffi::OsString::from("keep.png")]);
        assert_eq!(std::fs::read(dir.join("keep.png")).unwrap(), b"old");
        // Finished: every file in its place, no temporary name left.
        let mut staged = Staged::default();
        staged.write(&dir.join("a.png"), b"a").unwrap();
        staged.write(&dir.join("keep.png"), b"new").unwrap();
        assert_eq!(staged.finish(&dir.join("a.png"), Steps::NOW).unwrap(), 2);
        assert_eq!(std::fs::read(dir.join("keep.png")).unwrap(), b"new");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_held_export_waits_and_a_cancelled_one_writes_nothing() {
        let dir = scratch("held");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Walk.gif");
        hold_exports_to(&path, true);
        let pool = sue_core::JobPool::new(1);
        let target = path.clone();
        let job = pool.spawn("held", move |job| {
            write_whole(&target, b"GIF89a", Steps(Some(job)))
        });
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert!(!path.exists(), "held before it writes");
        job.cancel();
        assert_eq!(job.wait(), Err(sue_core::JobError::Cancelled));
        assert!(!path.exists(), "cancelled: nothing written");
        // Let go, it writes; made at once, a hold does not apply.
        hold_exports_to(&path, false);
        write_whole(&path, b"GIF89a", Steps::NOW).unwrap();
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
