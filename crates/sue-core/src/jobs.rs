//! Background jobs. Rendering, exporting, transcoding, simulation, indexing
//! and AI inference must never block the interface. A [`JobPool`] runs
//! them on a few worker threads that sleep when idle; each job reports
//! progress, can be cancelled, and a panic inside it becomes an error
//! instead of taking Sue down. Jobs read shared data through
//! [`Shared`](crate::Shared) handles and hand back a result; the interface
//! applies the result as a transaction, so undo still covers it.

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;

type Work = Box<dyn FnOnce() + Send>;

/// Why a job produced no result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobError {
    /// [`JobHandle::cancel`] was called and the job stopped.
    Cancelled,
    /// The job returned an error.
    Failed(String),
    /// The job panicked.
    Panicked(String),
    /// The pool shut down before the job ran.
    Dropped,
}

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::Failed(message) => write!(f, "failed: {message}"),
            Self::Panicked(message) => write!(f, "crashed: {message}"),
            Self::Dropped => f.write_str("the job pool shut down"),
        }
    }
}

impl std::error::Error for JobError {}

/// What a running job sees.
#[derive(Clone, Debug)]
pub struct JobContext {
    progress: Arc<AtomicU32>,
    cancelled: Arc<AtomicBool>,
}

impl JobContext {
    /// Reports progress from 0 to 1.
    pub fn set_progress(&self, fraction: f32) {
        self.progress
            .store(fraction.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// `true` once the job was cancelled; long jobs should check this and
    /// return an error (any error counts as a clean cancel).
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// A job that was submitted.
#[derive(Debug)]
pub struct JobHandle<T> {
    label: String,
    context: JobContext,
    result: Receiver<Result<T, JobError>>,
    done: Option<Result<T, JobError>>,
}

impl<T> JobHandle<T> {
    /// The job's label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Progress from 0 to 1.
    #[must_use]
    pub fn progress(&self) -> f32 {
        f32::from_bits(self.context.progress.load(Ordering::Relaxed))
    }

    /// Asks the job to stop.
    pub fn cancel(&self) {
        self.context.cancelled.store(true, Ordering::Relaxed);
    }

    fn poll(&mut self) {
        if self.done.is_none() {
            match self.result.try_recv() {
                Ok(result) => self.done = Some(result),
                Err(TryRecvError::Disconnected) => self.done = Some(Err(JobError::Dropped)),
                Err(TryRecvError::Empty) => {}
            }
        }
    }

    /// `true` once a result is waiting.
    pub fn is_finished(&mut self) -> bool {
        self.poll();
        self.done.is_some()
    }

    /// The result, if the job finished. Returns it once.
    pub fn try_take(&mut self) -> Option<Result<T, JobError>> {
        self.poll();
        self.done.take()
    }

    /// Blocks until the job finishes.
    ///
    /// # Errors
    /// Why the job produced no result.
    pub fn wait(mut self) -> Result<T, JobError> {
        if let Some(done) = self.done.take() {
            return done;
        }
        self.result.recv().unwrap_or(Err(JobError::Dropped))
    }
}

/// A small pool of worker threads.
#[derive(Debug)]
pub struct JobPool {
    sender: Option<Sender<Work>>,
    workers: Vec<JoinHandle<()>>,
}

impl JobPool {
    /// A pool with `threads` workers (at least one). Where no thread can
    /// start (a browser), it is [`Self::inline`] instead.
    #[must_use]
    pub fn new(threads: usize) -> Self {
        let (sender, receiver) = mpsc::channel::<Work>();
        let receiver = Arc::new(Mutex::new(receiver));
        let workers = (0..threads.max(1))
            .map(|index| {
                let receiver = Arc::clone(&receiver);
                std::thread::Builder::new()
                    .name(format!("sue-job-{index}"))
                    .spawn(move || {
                        loop {
                            // Blocks (sleeps) until work arrives or the pool drops.
                            let next = receiver
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .recv();
                            match next {
                                Ok(work) => work(),
                                Err(_) => break,
                            }
                        }
                    })
            })
            .collect::<Result<Vec<_>, _>>();
        let Ok(workers) = workers else {
            return Self::inline();
        };
        Self {
            sender: Some(sender),
            workers,
        }
    }

    /// A pool without threads: each job runs at once on the thread that
    /// spawns it, and its handle is finished when `spawn` returns.
    #[must_use]
    pub fn inline() -> Self {
        Self {
            sender: None,
            workers: Vec::new(),
        }
    }

    /// A pool that leaves one core for the interface.
    #[must_use]
    pub fn for_this_machine() -> Self {
        let cores = std::thread::available_parallelism().map_or(2, usize::from);
        Self::new(cores.saturating_sub(1).clamp(1, 8))
    }

    /// Number of workers.
    #[must_use]
    pub fn threads(&self) -> usize {
        self.workers.len()
    }

    /// Runs `work` on a worker.
    pub fn spawn<T, F>(&self, label: impl Into<String>, work: F) -> JobHandle<T>
    where
        T: Send + 'static,
        F: FnOnce(&JobContext) -> Result<T, String> + Send + 'static,
    {
        let context = JobContext {
            progress: Arc::new(AtomicU32::new(0)),
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        let (reply, result) = mpsc::channel();
        let job_context = context.clone();
        let run: Work = Box::new(move || {
            let outcome = if job_context.is_cancelled() {
                Err(JobError::Cancelled)
            } else {
                match catch_unwind(AssertUnwindSafe(|| work(&job_context))) {
                    Ok(Ok(value)) => {
                        job_context.set_progress(1.0);
                        Ok(value)
                    }
                    Ok(Err(_)) if job_context.is_cancelled() => Err(JobError::Cancelled),
                    Ok(Err(message)) => Err(JobError::Failed(message)),
                    Err(panic) => {
                        let message = panic
                            .downcast_ref::<&str>()
                            .map(|text| (*text).to_owned())
                            .or_else(|| panic.downcast_ref::<String>().cloned())
                            .unwrap_or_else(|| "unknown panic".to_owned());
                        Err(JobError::Panicked(message))
                    }
                }
            };
            // The handle may have been dropped; the result is then unwanted.
            let _ = reply.send(outcome);
        });
        match &self.sender {
            Some(sender) => {
                let _ = sender.send(run);
            }
            None if self.workers.is_empty() => run(),
            None => {}
        }
        JobHandle {
            label: label.into(),
            context,
            result,
            done: None,
        }
    }
}

impl Drop for JobPool {
    /// Finishes queued jobs, then stops the workers.
    fn drop(&mut self) {
        self.sender = None;
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Shared;

    #[test]
    fn jobs_run_report_and_fail_safely() {
        let pool = JobPool::new(2);
        let data = Shared::new((1..=1000_u64).collect::<Vec<_>>());
        let input = data.clone();
        let sum = pool.spawn("sum", move |context| {
            context.set_progress(0.5);
            Ok(input.iter().sum::<u64>())
        });
        assert_eq!(sum.wait(), Ok(500_500));
        let failed = pool.spawn("fail", |_| Err::<(), _>("disk full".to_owned()));
        assert_eq!(failed.wait(), Err(JobError::Failed("disk full".into())));
        let crashed = pool.spawn("crash", |_| -> Result<(), String> { panic!("boom") });
        assert_eq!(crashed.wait(), Err(JobError::Panicked("boom".into())));
        // The pool still works after a panic.
        let mut again = pool.spawn("again", |_| Ok(7));
        let value = loop {
            if let Some(result) = again.try_take() {
                break result;
            }
            std::thread::yield_now();
        };
        assert_eq!(value, Ok(7));
        assert_eq!(data.holders(), 1, "the job released its handle");
    }

    #[test]
    fn an_inline_pool_runs_each_job_as_it_is_spawned() {
        let pool = JobPool::inline();
        assert_eq!(pool.threads(), 0);
        let mut job = pool.spawn("sum", |context| {
            context.set_progress(0.5);
            Ok(2 + 2)
        });
        assert!(job.is_finished());
        assert!((job.progress() - 1.0).abs() < f32::EPSILON);
        assert_eq!(job.wait(), Ok(4));
        let failed = pool.spawn::<(), _>("fail", |_| Err("no".to_owned()));
        assert_eq!(failed.wait(), Err(JobError::Failed("no".to_owned())));
    }

    #[test]
    fn jobs_can_be_cancelled() {
        let pool = JobPool::new(1);
        let (started, wait) = mpsc::channel();
        let job = pool.spawn("long", move |context| {
            started.send(()).ok();
            while !context.is_cancelled() {
                std::thread::yield_now();
            }
            Err::<(), _>("stopped".to_owned())
        });
        wait.recv().unwrap();
        job.cancel();
        assert_eq!(job.wait(), Err(JobError::Cancelled));
        assert!(pool.threads() == 1);
    }
}
