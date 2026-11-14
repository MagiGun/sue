//! Saving the project and keeping unsaved work, on a background job.
//!
//! The interface thread takes a snapshot of the arena (a clone shares
//! every raster's pixels, D21) and hands it, with the open project's
//! [`SueFile`], to a job in the transfers list, so people go on working
//! while the file is written. The job saves into that `SueFile`, which is
//! incremental: only tiles changed since the last save are read, hashed
//! and encoded (D12). It adds the project's preview (`THMB`) and streams
//! the file into a temporary one that replaces the old file, so a save
//! that fails at any point leaves the old file whole. The `SueFile`
//! comes back with the result, for the next save.
//!
//! Unsaved work goes the same way into the Hub's kept work, as a record
//! of its changes ([`sue_wm::Autosave::write`]).

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use sue_core::file::{Chunk, THUMBNAIL};
use sue_core::pixels::PixelOptions;
use sue_core::{Arena, JobContext, SueFile};
use sue_paint::composite::{RenderOptions, flatten};

/// The version of the `THMB` chunk this build writes: PNG bytes.
const PREVIEW_VERSION: u32 = 1;

/// The project's preview for the Hub's cards: its first picture flattened
/// on its paper, at most [`sue_wm::files::thumbs::SIDE`] pixels on its
/// long edge, as PNG. `None` when the project holds no picture.
pub(crate) fn preview(arena: &Arena) -> Option<Chunk> {
    let picture = arena
        .iter()
        .find(|(_, entity)| sue_model::picture::is_picture(entity))
        .map(|(_, entity)| entity.id())?;
    let flat = flatten(
        arena,
        picture,
        RenderOptions {
            paper: true,
            drafts: false,
        },
    )?;
    let thumb = sue_wm::files::thumbs::shrink(&flat, sue_wm::files::thumbs::SIDE);
    let png = sue_io::encode_png_rgba(thumb.size[0], thumb.size[1], &thumb.pixels).ok()?;
    Some(Chunk {
        tag: THUMBNAIL,
        version: PREVIEW_VERSION,
        payload: png,
    })
}

/// `arena` saved into `file` (incrementally) with its preview, the way
/// Save writes a project. The preview of a project without a picture is
/// removed, so no card shows what the project no longer holds.
pub(crate) fn fill(
    file: &mut SueFile,
    arena: &Arena,
    options: &PixelOptions,
    job: Option<&JobContext>,
) {
    file.set_arena_with(arena, &|_| true, options);
    if let Some(job) = job {
        job.set_progress(0.5);
    }
    match preview(arena) {
        Some(chunk) => file.set(chunk),
        None => {
            file.remove(THUMBNAIL);
        }
    }
    if let Some(job) = job {
        job.set_progress(0.7);
    }
}

/// What a project save takes to its job.
pub(crate) struct SaveInput {
    /// The open project's file (chunks this build does not know survive),
    /// copied by the job only if a keep's job still holds it too.
    pub file: Arc<SueFile>,
    /// A snapshot of the document.
    pub arena: Arena,
    /// Where it goes.
    pub path: PathBuf,
    /// A test makes the write fail after this many bytes.
    pub fail_after: Option<u64>,
}

/// Saves `input` to its path; the file comes back either way, with the
/// error as people read it on failure.
pub(crate) fn save(input: SaveInput, job: Option<&JobContext>) -> (SueFile, Result<(), String>) {
    let SaveInput {
        file,
        arena,
        path,
        fail_after,
    } = input;
    let mut file = Arc::unwrap_or_clone(file);
    fill(&mut file, &arena, &PixelOptions::SAVE, job);
    // The snapshot's pixels are let go before the write, so a stroke made
    // meanwhile copies nothing more than it must.
    drop(arena);
    let result = write_sue(&path, &file, fail_after).map_err(|error| error.to_string());
    if let Some(job) = job {
        job.set_progress(1.0);
    }
    (file, result)
}

/// Writes `file` to `path` atomically, streamed.
pub(crate) fn write_sue(path: &Path, file: &SueFile, fail_after: Option<u64>) -> io::Result<()> {
    sue_platform::write_atomic_with(path, |mut out| match fail_after {
        Some(left) => file.write_to(&mut FailAfter { out, left }),
        None => file.write_to(&mut out),
    })
}

/// A writer that fails once it passed `left` bytes on: a full disk or a
/// pulled drive, for the tests.
struct FailAfter<'a> {
    out: &'a mut dyn Write,
    left: u64,
}

impl Write for FailAfter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Err(io::Error::other("the disk is full"));
        }
        let take = bytes
            .len()
            .min(usize::try_from(self.left).unwrap_or(usize::MAX));
        let written = self.out.write(&bytes[..take])?;
        self.left -= written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

/// A job's result, waiting to be taken on the interface thread.
pub(crate) struct Running<T> {
    slot: Arc<Mutex<Option<T>>>,
    /// Its entry in the transfers list.
    pub transfer: u64,
    /// The arena's revision of the snapshot it works on.
    pub revision: u64,
}

impl<T: Send + 'static> Running<T> {
    /// Starts `work` as a transfer of `kind` named `label`.
    pub fn spawn(
        transfers: &mut sue_wm::Transfers,
        kind: sue_wm::TransferKind,
        label: &str,
        path: Option<PathBuf>,
        revision: u64,
        work: impl FnOnce(&JobContext) -> (T, Result<(), String>) + Send + 'static,
    ) -> Self {
        let slot = Arc::new(Mutex::new(None));
        let out = Arc::clone(&slot);
        let transfer = transfers.spawn(kind, label, path, move |job| {
            let (value, result) = work(job);
            *out.lock().unwrap_or_else(PoisonError::into_inner) = Some(value);
            result
        });
        Self {
            slot,
            transfer,
            revision,
        }
    }

    /// The result, once the job finished.
    pub fn take(&self) -> Option<T> {
        self.slot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// `true` once the job ended without a result (it crashed).
    pub fn lost(&self, transfers: &sue_wm::Transfers) -> bool {
        transfers
            .get(self.transfer)
            .is_none_or(sue_wm::Transfer::finished)
            && self
                .slot
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_none()
    }
}
