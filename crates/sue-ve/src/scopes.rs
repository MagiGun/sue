//! Video scopes, as DaVinci Resolve's Color page shows them: the
//! waveform (luma by column), the RGB parade, the vectorscope (chroma)
//! and the histogram. They are worked out on a thread of their own from
//! the frame the viewer shows (shared by handle, rule 5), never on the
//! interface's thread, and come back as small pictures to draw.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};

use crate::media::frames::VideoFrame;

/// Which scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Luma down each column.
    Waveform,
    /// Red, green and blue side by side.
    Parade,
    /// Chroma round a circle.
    Vectorscope,
    /// How many pixels at each level.
    Histogram,
}

impl Scope {
    /// Every scope, as the picker lists them.
    pub const ALL: [Self; 4] = [
        Self::Waveform,
        Self::Parade,
        Self::Vectorscope,
        Self::Histogram,
    ];

    /// Its stable name.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Waveform => "waveform",
            Self::Parade => "parade",
            Self::Vectorscope => "vectorscope",
            Self::Histogram => "histogram",
        }
    }

    /// Its name as people read it.
    #[must_use]
    pub fn label(self) -> String {
        tr!(&format!("scope-{}", self.id()))
    }
}

/// The side of a scope's picture (its levels).
pub const LEVELS: u32 = 256;

fn luma(p: &[u8]) -> f32 {
    0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2])
}

/// Counts to a brightness: soft, so a few pixels show and many do not
/// saturate at once.
fn glow(count: u32, scale: f32) -> u8 {
    if count == 0 {
        return 0;
    }
    ((count as f32 * scale).sqrt().min(1.0) * 255.0) as u8
}

/// Works out `scope` of `frame` as a picture: `LEVELS` tall (and wide
/// for the vectorscope), level 255 at the top. Its alpha says where the
/// trace is, its color is the channel's (white for luma).
#[must_use]
pub fn compute(scope: Scope, frame: &VideoFrame) -> VideoFrame {
    let (w, h) = (frame.width as usize, frame.height as usize);
    let levels = LEVELS as usize;
    // Sample at most about 512 by 288 pixels: enough for a scope.
    let step = (w / 512).max(h / 288).max(1);
    let pixels = |each: &mut dyn FnMut(usize, &[u8])| {
        for y in (0..h).step_by(step) {
            for x in (0..w).step_by(step) {
                let at = (y * w + x) * 4;
                if let Some(p) = frame.rgba.get(at..at + 4) {
                    each(x, p);
                }
            }
        }
    };
    let samples = ((w / step).max(1) * (h / step).max(1)) as f32;
    match scope {
        Scope::Waveform | Scope::Parade => {
            let columns = levels;
            let parts = if scope == Scope::Parade { 3 } else { 1 };
            let mut counts = vec![0_u32; columns * levels * parts];
            pixels(&mut |x, p| {
                let column = x * columns / w.max(1);
                for part in 0..parts {
                    let value = if parts == 1 {
                        luma(p).round() as usize
                    } else {
                        usize::from(p[part])
                    };
                    counts[(part * columns + column) * levels + value.min(levels - 1)] += 1;
                }
            });
            let width = columns * parts;
            let per_column = samples / columns as f32;
            let scale = 6.0 / per_column.max(1.0);
            let mut rgba = vec![0_u8; width * levels * 4];
            for part in 0..parts {
                let color = match (parts, part) {
                    (1, _) => [235, 235, 235],
                    (_, 0) => [235, 80, 80],
                    (_, 1) => [80, 220, 100],
                    _ => [90, 140, 245],
                };
                for column in 0..columns {
                    for value in 0..levels {
                        let count = counts[(part * columns + column) * levels + value];
                        let alpha = glow(count, scale);
                        if alpha == 0 {
                            continue;
                        }
                        let x = part * columns + column;
                        let y = levels - 1 - value;
                        let at = (y * width + x) * 4;
                        rgba[at..at + 4].copy_from_slice(&[color[0], color[1], color[2], alpha]);
                    }
                }
            }
            VideoFrame::new(width as u32, LEVELS, rgba, 0)
        }
        Scope::Vectorscope => {
            let mut counts = vec![0_u32; levels * levels];
            let mut tint = vec![[0_u32; 3]; levels * levels];
            pixels(&mut |_, p| {
                let rgb = [p[0], p[1], p[2]].map(|c| f32::from(c) / 255.0);
                let y = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
                let cb = (rgb[2] - y) / 1.8556;
                let cr = (rgb[0] - y) / 1.5748;
                let x = ((cb + 0.5) * (levels - 1) as f32).round() as usize;
                let v = ((0.5 - cr) * (levels - 1) as f32).round() as usize;
                let at = v.min(levels - 1) * levels + x.min(levels - 1);
                counts[at] += 1;
                for c in 0..3 {
                    tint[at][c] += u32::from(p[c]);
                }
            });
            let scale = 400.0 / samples.max(1.0);
            let mut rgba = vec![0_u8; levels * levels * 4];
            for (at, count) in counts.iter().enumerate() {
                let alpha = glow(*count, scale);
                if alpha == 0 {
                    continue;
                }
                let mean = tint[at].map(|sum| (sum / count.max(&1)) as u8);
                // Lifted toward white so dark colors still show.
                let lift = |c: u8| ((u16::from(c) + 255) / 2) as u8;
                rgba[at * 4..at * 4 + 4].copy_from_slice(&[
                    lift(mean[0]),
                    lift(mean[1]),
                    lift(mean[2]),
                    alpha,
                ]);
            }
            VideoFrame::new(LEVELS, LEVELS, rgba, 0)
        }
        Scope::Histogram => {
            let mut counts = vec![[0_u32; 4]; levels];
            pixels(&mut |_, p| {
                for c in 0..3 {
                    counts[usize::from(p[c])][c] += 1;
                }
                counts[(luma(p).round() as usize).min(levels - 1)][3] += 1;
            });
            let height = levels / 2;
            let most = counts
                .iter()
                .flat_map(|c| c.iter().take(3))
                .copied()
                .max()
                .unwrap_or(1)
                .max(1) as f32;
            let mut rgba = vec![0_u8; levels * height * 4];
            for (x, count) in counts.iter().enumerate() {
                for (c, color) in [[235, 80, 80], [80, 220, 100], [90, 140, 245]]
                    .iter()
                    .enumerate()
                {
                    let tall = ((count[c] as f32 / most).sqrt() * height as f32) as usize;
                    for y in height.saturating_sub(tall)..height {
                        let at = (y * levels + x) * 4;
                        // Channels add where they overlap.
                        for k in 0..3 {
                            rgba[at + k] = rgba[at + k].saturating_add(color[k] / 2);
                        }
                        rgba[at + 3] = rgba[at + 3].max(200);
                    }
                }
            }
            VideoFrame::new(LEVELS, height as u32, rgba, 0)
        }
    }
}

/// A scope worked out: of which frame (its serial), which scope, the
/// picture.
pub type Worked = (u64, Scope, Arc<VideoFrame>);

/// The scopes' worker: it works out the scope of the newest frame asked
/// for and drops the ones it did not get to.
pub struct Scopes {
    to: Option<Sender<(Scope, Arc<VideoFrame>)>>,
    done: Arc<Mutex<Option<Worked>>>,
    busy: Arc<AtomicBool>,
    asked: Option<(u64, Scope)>,
}

impl std::fmt::Debug for Scopes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scopes")
            .field("asked", &self.asked)
            .finish_non_exhaustive()
    }
}

impl Default for Scopes {
    fn default() -> Self {
        Self {
            to: None,
            done: Arc::new(Mutex::new(None)),
            busy: Arc::new(AtomicBool::new(false)),
            asked: None,
        }
    }
}

impl Scopes {
    /// Asks for `scope` of `frame` (nothing is asked twice).
    pub fn want(&mut self, scope: Scope, frame: &Arc<VideoFrame>) {
        if self.asked == Some((frame.serial, scope)) {
            return;
        }
        self.asked = Some((frame.serial, scope));
        if self.to.is_none() {
            let (to, from) = std::sync::mpsc::channel();
            let done = Arc::clone(&self.done);
            let busy = Arc::clone(&self.busy);
            let _ = std::thread::Builder::new()
                .name("sue-ve-scopes".to_owned())
                .spawn(move || run(&from, &done, &busy));
            self.to = Some(to);
        }
        if let Some(to) = &self.to {
            self.busy.store(true, Ordering::Relaxed);
            let _ = to.send((scope, Arc::clone(frame)));
        }
    }

    /// The newest scope worked out: of which frame, which scope, the
    /// picture.
    #[must_use]
    pub fn latest(&self) -> Option<Worked> {
        self.done
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// `true` while it works.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.busy.load(Ordering::Relaxed)
    }
}

fn run(from: &Receiver<(Scope, Arc<VideoFrame>)>, done: &Mutex<Option<Worked>>, busy: &AtomicBool) {
    while let Ok(mut job) = from.recv() {
        // Only the newest.
        loop {
            match from.try_recv() {
                Ok(newer) => job = newer,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        let (scope, frame) = job;
        let picture = Arc::new(compute(scope, &frame));
        *done.lock().unwrap_or_else(PoisonError::into_inner) = Some((frame.serial, scope, picture));
        busy.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(rgba: [u8; 4]) -> VideoFrame {
        VideoFrame::new(64, 32, rgba.repeat(64 * 32), 0)
    }

    fn lit_rows(picture: &VideoFrame) -> Vec<usize> {
        let w = picture.width as usize;
        (0..picture.height as usize)
            .filter(|y| (0..w).any(|x| picture.rgba[(y * w + x) * 4 + 3] > 0))
            .collect()
    }

    #[test]
    fn a_gray_frame_draws_one_line_on_the_waveform() {
        let gray = frame([128, 128, 128, 255]);
        let wave = compute(Scope::Waveform, &gray);
        assert_eq!((wave.width, wave.height), (256, 256));
        assert_eq!(lit_rows(&wave), [255 - 128]);
        let parade = compute(Scope::Parade, &gray);
        assert_eq!(parade.width, 768);
        assert_eq!(lit_rows(&parade), [127]);
    }

    #[test]
    fn gray_sits_in_the_middle_of_the_vectorscope_and_red_does_not() {
        let middle = compute(Scope::Vectorscope, &frame([128, 128, 128, 255]));
        let lit: Vec<usize> = middle
            .rgba
            .chunks(4)
            .enumerate()
            .filter(|(_, p)| p[3] > 0)
            .map(|(at, _)| at)
            .collect();
        assert_eq!(lit.len(), 1);
        let (x, y) = (lit[0] % 256, lit[0] / 256);
        assert!((x as i32 - 128).abs() <= 1 && (y as i32 - 128).abs() <= 1);
        let red = compute(Scope::Vectorscope, &frame([220, 20, 20, 255]));
        let at = red.rgba.chunks(4).position(|p| p[3] > 0).unwrap();
        assert!(at / 256 < 100, "red sits up (high Cr)");
    }

    #[test]
    fn the_histogram_counts_each_channel_and_the_worker_answers() {
        let picture = compute(Scope::Histogram, &frame([255, 0, 128, 255]));
        assert_eq!(picture.height, 128);
        let column_lit = |x: usize| picture.rgba[(127 * 256 + x) * 4 + 3] > 0;
        assert!(column_lit(255) && column_lit(0) && column_lit(128));
        assert!(!column_lit(60));
        let mut scopes = Scopes::default();
        let shared = Arc::new(frame([10, 20, 30, 255]));
        scopes.want(Scope::Histogram, &shared);
        for _ in 0..400 {
            if scopes.latest().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let (serial, scope, _) = scopes.latest().unwrap();
        assert_eq!((serial, scope), (shared.serial, Scope::Histogram));
    }
}
