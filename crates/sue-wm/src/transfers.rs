//! Data transfers: the files Sue brings in, writes out and copies in this
//! run, the ones still going and the ones done. Quick ones (a picture
//! opened, a project saved) are noted as they finish; long ones run as
//! background jobs ([`sue_core::JobPool`]) that report their progress, so
//! the interface never waits for them. Every studio and tool shares the
//! one list, and a start page shows it in its Data Transfers popup.

use std::path::PathBuf;

use sue_core::{JobContext, JobError, JobHandle, JobPool};

use crate::unix_now;

/// How many finished transfers the list keeps.
const KEEP: usize = 200;

/// What a transfer does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferKind {
    /// Brings a file in (opening, importing).
    Import,
    /// Writes a file out (saving, exporting).
    Export,
    /// Copies files (into a library, between folders).
    Copy,
}

impl TransferKind {
    /// What people read.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Import => tr!("transfer-import"),
            Self::Export => tr!("transfer-export"),
            Self::Copy => tr!("transfer-copy"),
        }
    }
}

/// Where a transfer is.
#[derive(Clone, Debug, PartialEq)]
pub enum TransferState {
    /// Still going, with its progress from 0 to 1.
    Running(f32),
    /// Done.
    Done,
    /// Stopped by an error.
    Failed(String),
    /// Stopped on request.
    Cancelled,
}

/// One transfer.
#[derive(Debug)]
pub struct Transfer {
    /// Its id in this run.
    pub id: u64,
    /// What it does.
    pub kind: TransferKind,
    /// What people read, e.g. the file name.
    pub label: String,
    /// The file or folder it writes or reads, if one.
    pub path: Option<PathBuf>,
    /// Where it is.
    pub state: TransferState,
    /// When it started, in seconds since 1970.
    pub started: u64,
    job: Option<JobHandle<()>>,
}

impl Transfer {
    /// `true` once it is done, failed or cancelled.
    #[must_use]
    pub fn finished(&self) -> bool {
        !matches!(self.state, TransferState::Running(_))
    }
}

/// The transfers of this run, newest first.
#[derive(Debug, Default)]
pub struct Transfers {
    list: Vec<Transfer>,
    next: u64,
    /// Workers for long transfers, started on first use.
    pool: Option<JobPool>,
    revision: u64,
}

impl Transfers {
    /// Goes up on every change.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn push(&mut self, transfer: Transfer) -> u64 {
        let id = transfer.id;
        self.list.insert(0, transfer);
        if self.list.len() > KEEP
            && let Some(index) = self.list.iter().rposition(Transfer::finished)
        {
            self.list.remove(index);
        }
        self.revision += 1;
        id
    }

    fn make(&mut self, kind: TransferKind, label: &str, path: Option<PathBuf>) -> Transfer {
        self.next += 1;
        Transfer {
            id: self.next,
            kind,
            label: label.to_owned(),
            path,
            state: TransferState::Done,
            started: unix_now(),
            job: None,
        }
    }

    /// Notes a transfer that already finished. Returns its id.
    pub fn done(&mut self, kind: TransferKind, label: &str, path: Option<PathBuf>) -> u64 {
        let transfer = self.make(kind, label, path);
        self.push(transfer)
    }

    /// Notes a transfer that failed. Returns its id.
    pub fn failed(
        &mut self,
        kind: TransferKind,
        label: &str,
        path: Option<PathBuf>,
        why: &str,
    ) -> u64 {
        let mut transfer = self.make(kind, label, path);
        transfer.state = TransferState::Failed(why.to_owned());
        self.push(transfer)
    }

    /// Runs `work` in the background as a transfer and returns its id.
    /// It reports progress through its [`JobContext`] and should stop when
    /// cancelled.
    pub fn spawn<F>(
        &mut self,
        kind: TransferKind,
        label: &str,
        path: Option<PathBuf>,
        work: F,
    ) -> u64
    where
        F: FnOnce(&JobContext) -> Result<(), String> + Send + 'static,
    {
        let pool = self.pool.get_or_insert_with(|| JobPool::new(2));
        let job = pool.spawn(label.to_owned(), work);
        let mut transfer = self.make(kind, label, path);
        transfer.state = TransferState::Running(0.0);
        transfer.job = Some(job);
        self.push(transfer)
    }

    /// Reads the progress of running transfers. Returns `true` while any
    /// still runs.
    pub fn poll(&mut self) -> bool {
        let mut running = false;
        let mut changed = false;
        for transfer in &mut self.list {
            let Some(job) = transfer.job.as_mut() else {
                continue;
            };
            let state = match job.try_take() {
                None => TransferState::Running(job.progress()),
                Some(Ok(())) => TransferState::Done,
                Some(Err(JobError::Cancelled)) => TransferState::Cancelled,
                Some(Err(error)) => TransferState::Failed(error.to_string()),
            };
            if transfer.state != state {
                transfer.state = state;
                changed = true;
            }
            if transfer.finished() {
                transfer.job = None;
            } else {
                running = true;
            }
        }
        if changed {
            self.revision += 1;
        }
        running
    }

    /// Asks the running transfer `id` to stop.
    pub fn cancel(&mut self, id: u64) {
        if let Some(job) = self
            .list
            .iter()
            .find(|transfer| transfer.id == id)
            .and_then(|transfer| transfer.job.as_ref())
        {
            job.cancel();
        }
    }

    /// The transfers, newest first.
    #[must_use]
    pub fn list(&self) -> &[Transfer] {
        &self.list
    }

    /// The transfer `id`, while the list keeps it.
    #[must_use]
    pub fn get(&self, id: u64) -> Option<&Transfer> {
        self.list.iter().find(|transfer| transfer.id == id)
    }

    /// How many still run.
    #[must_use]
    pub fn running(&self) -> usize {
        self.list
            .iter()
            .filter(|transfer| !transfer.finished())
            .count()
    }

    /// Forgets the finished transfers.
    pub fn clear_completed(&mut self) {
        let before = self.list.len();
        self.list.retain(|transfer| !transfer.finished());
        if self.list.len() != before {
            self.revision += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfers_run_report_and_clear() {
        let mut transfers = Transfers::default();
        transfers.done(TransferKind::Export, "out.png", None);
        transfers.failed(TransferKind::Import, "broken.png", None, "not a picture");
        let copy = transfers.spawn(TransferKind::Copy, "brush.png", None, |job| {
            job.set_progress(0.5);
            Ok(())
        });
        let failing = transfers.spawn(TransferKind::Copy, "gone.png", None, |_| {
            Err("missing".to_owned())
        });
        let start = std::time::Instant::now();
        while transfers.poll() {
            assert!(start.elapsed().as_secs() < 10, "the jobs finish");
            std::thread::yield_now();
        }
        assert_eq!(transfers.get(copy).unwrap().state, TransferState::Done);
        assert!(matches!(
            transfers.get(failing).unwrap().state,
            TransferState::Failed(_)
        ));
        // Newest first.
        assert_eq!(transfers.list()[0].id, failing);
        assert_eq!(transfers.list().len(), 4);
        assert_eq!(transfers.running(), 0);
        transfers.clear_completed();
        assert!(transfers.list().is_empty());
    }

    #[test]
    fn a_running_transfer_stays_when_completed_ones_clear() {
        let mut transfers = Transfers::default();
        let (go, wait) = std::sync::mpsc::channel::<()>();
        let id = transfers.spawn(TransferKind::Copy, "slow", None, move |job| {
            let _ = wait.recv();
            if job.is_cancelled() {
                Err("cancelled".to_owned())
            } else {
                Ok(())
            }
        });
        transfers.done(TransferKind::Import, "quick", None);
        assert!(transfers.poll());
        transfers.clear_completed();
        assert_eq!(transfers.list().len(), 1);
        transfers.cancel(id);
        go.send(()).unwrap();
        while transfers.poll() {
            std::thread::yield_now();
        }
        assert_eq!(transfers.get(id).unwrap().state, TransferState::Cancelled);
    }
}
