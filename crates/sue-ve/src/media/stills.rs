//! Pictures drawn off the interface's thread: the frames of image
//! sequences (a file read and decoded per frame) and of Sue Paint
//! pictures and animations (layers composited per frame). Stage 1 drew
//! them on the interface's thread, where a large frame stuttered
//! playback; now two worker threads draw them and hand them back by
//! handle (rule 5), and the monitors show the nearest frame drawn until
//! the one asked for is there, as they do for movies.
//!
//! A job is a closure that makes one frame. The queue keeps the newest
//! wants first: when a monitor asks for frames of a source again, the
//! jobs for that source it no longer wants are dropped.

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, PoisonError};

use sue_core::EntityId;

use super::frames::VideoFrame;

/// Makes one frame, on a worker.
pub type Make = Box<dyn FnOnce() -> Option<VideoFrame> + Send>;

/// One frame to draw: whose, which revision of it, which frame.
pub struct Job {
    /// The source.
    pub source: EntityId,
    /// The source's revision when it was asked for.
    pub revision: u64,
    /// The frame.
    pub index: i64,
    /// What draws it.
    pub make: Make,
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Job")
            .field("source", &self.source)
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

/// A frame drawn: whose, which revision, and the picture.
pub type Drawn = (EntityId, u64, i64, Option<Arc<VideoFrame>>);

#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    stop: bool,
}

/// The workers and their queue.
pub struct Stills {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    done: Receiver<Drawn>,
    to_done: Sender<Drawn>,
    started: bool,
    /// Frames asked for and not back yet.
    pending: HashSet<(EntityId, u64, i64)>,
    working: Arc<AtomicBool>,
}

impl std::fmt::Debug for Stills {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stills")
            .field("pending", &self.pending.len())
            .finish_non_exhaustive()
    }
}

impl Default for Stills {
    fn default() -> Self {
        let (to_done, done) = std::sync::mpsc::channel();
        Self {
            queue: Arc::new((Mutex::new(Queue::default()), Condvar::new())),
            done,
            to_done,
            started: false,
            pending: HashSet::new(),
            working: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// How many threads draw.
const WORKERS: usize = 2;

impl Stills {
    fn start(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        for _ in 0..WORKERS {
            let queue = Arc::clone(&self.queue);
            let done = self.to_done.clone();
            let working = Arc::clone(&self.working);
            let _ = std::thread::Builder::new()
                .name("sue-ve-stills".to_owned())
                .spawn(move || run(&queue, &done, &working));
        }
    }

    /// `true` when frame `index` of `source` at `revision` was asked for
    /// and is not back yet.
    #[must_use]
    pub fn is_pending(&self, source: EntityId, revision: u64, index: i64) -> bool {
        self.pending.contains(&(source, revision, index))
    }

    /// Asks for `jobs` of `source`, dropping the queued jobs of `source`
    /// that are not among them (what a monitor no longer wants). Jobs
    /// already asked for are not asked again.
    pub fn want(&mut self, source: EntityId, jobs: Vec<Job>) {
        self.start();
        let wanted: HashSet<i64> = jobs.iter().map(|job| job.index).collect();
        let (lock, wake) = &*self.queue;
        let mut queue = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut dropped = Vec::new();
        queue.jobs.retain(|job| {
            let keep = job.source != source || wanted.contains(&job.index);
            if !keep {
                dropped.push((job.source, job.revision, job.index));
            }
            keep
        });
        for key in dropped {
            self.pending.remove(&key);
        }
        for job in jobs {
            let key = (job.source, job.revision, job.index);
            if self.pending.insert(key) {
                queue.jobs.push_back(job);
            }
        }
        drop(queue);
        wake.notify_all();
    }

    /// The frames drawn since the last call.
    pub fn take(&mut self) -> Vec<Drawn> {
        let drawn: Vec<Drawn> = self.done.try_iter().collect();
        for (source, revision, index, _) in &drawn {
            self.pending.remove(&(*source, *revision, *index));
        }
        drawn
    }

    /// `true` while frames are asked for and not drawn.
    #[must_use]
    pub fn busy(&self) -> bool {
        !self.pending.is_empty() || self.working.load(Ordering::Relaxed)
    }

    /// Forgets every queued job (the sources changed).
    pub fn clear(&mut self) {
        let (lock, _) = &*self.queue;
        lock.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .jobs
            .clear();
        self.pending.clear();
    }
}

impl Drop for Stills {
    fn drop(&mut self) {
        let (lock, wake) = &*self.queue;
        lock.lock().unwrap_or_else(PoisonError::into_inner).stop = true;
        wake.notify_all();
    }
}

fn run(queue: &(Mutex<Queue>, Condvar), done: &Sender<Drawn>, working: &AtomicBool) {
    let (lock, wake) = queue;
    loop {
        let job = {
            let mut queue = lock.lock().unwrap_or_else(PoisonError::into_inner);
            loop {
                if queue.stop {
                    return;
                }
                if let Some(job) = queue.jobs.pop_front() {
                    break job;
                }
                queue = wake.wait(queue).unwrap_or_else(PoisonError::into_inner);
            }
        };
        working.store(true, Ordering::Relaxed);
        let Job {
            source,
            revision,
            index,
            make,
        } = job;
        let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(make))
            .ok()
            .flatten()
            .map(Arc::new);
        working.store(false, Ordering::Relaxed);
        if done.send((source, revision, index, made)).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(stills: &mut Stills, count: usize) -> Vec<Drawn> {
        let mut out = Vec::new();
        for _ in 0..400 {
            out.extend(stills.take());
            if out.len() >= count {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        out
    }

    fn job(source: EntityId, index: i64) -> Job {
        Job {
            source,
            revision: 1,
            index,
            make: Box::new(move || Some(VideoFrame::new(2, 2, vec![index as u8; 16], index))),
        }
    }

    #[test]
    fn frames_are_drawn_off_the_thread_and_come_back_once() {
        let mut stills = Stills::default();
        let source = EntityId::new();
        let caller = std::thread::current().id();
        let seen = Arc::new(Mutex::new(None));
        let into = Arc::clone(&seen);
        stills.want(
            source,
            vec![Job {
                source,
                revision: 1,
                index: 0,
                make: Box::new(move || {
                    *into.lock().unwrap() = Some(std::thread::current().id());
                    Some(VideoFrame::new(1, 1, vec![0; 4], 0))
                }),
            }],
        );
        stills.want(source, vec![job(source, 0), job(source, 1)]);
        let drawn = wait(&mut stills, 2);
        assert_eq!(drawn.len(), 2, "frame 0 was not asked twice");
        assert!(drawn.iter().all(|(_, _, _, frame)| frame.is_some()));
        assert_ne!(seen.lock().unwrap().unwrap(), caller, "drawn on a worker");
        assert!(!stills.is_pending(source, 1, 1));
    }

    #[test]
    fn frames_no_longer_wanted_are_dropped_from_the_queue() {
        let mut stills = Stills::default();
        let source = EntityId::new();
        // Hold the workers so the queue fills.
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        for _ in 0..WORKERS {
            let gate = Arc::clone(&gate);
            let holder = EntityId::new();
            stills.want(
                holder,
                vec![Job {
                    source: holder,
                    revision: 0,
                    index: -1,
                    make: Box::new(move || {
                        let (lock, wake) = &*gate;
                        let mut open = lock.lock().unwrap();
                        while !*open {
                            open = wake.wait(open).unwrap();
                        }
                        None
                    }),
                }],
            );
        }
        stills.want(source, (0..10).map(|index| job(source, index)).collect());
        stills.want(source, vec![job(source, 7)]);
        assert!(stills.is_pending(source, 1, 7) && !stills.is_pending(source, 1, 3));
        {
            let (lock, wake) = &*gate;
            *lock.lock().unwrap() = true;
            wake.notify_all();
        }
        let drawn = wait(&mut stills, WORKERS + 1);
        let indices: Vec<i64> = drawn
            .iter()
            .filter(|(from, _, _, _)| *from == source)
            .map(|(_, _, index, _)| *index)
            .collect();
        assert_eq!(indices, [7]);
    }
}
