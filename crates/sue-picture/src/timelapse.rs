//! Timelapse, as in Clip Studio Paint: while a canvas records, every change
//! to it is kept as a frame, and Export Timelapse makes a video of how the
//! picture was drawn. The frames are small JPEG pictures in Sue Paint's own
//! folder, a folder a picture, so recording costs the undo history and the
//! project nothing; the picture keeps only whether it records
//! ([`RECORD`]).

use std::path::{Path, PathBuf};

use sue_core::{EntityId, PixelFormat, RasterTile2D};

/// Whether a picture records its timelapse (`Bool`).
pub const RECORD: &str = "timelapse.record";

/// The longest side a frame is kept at.
pub const FRAME_SIDE: u32 = 1280;

/// The folder `picture`'s frames are kept in, under Sue Paint's folder
/// `base`.
#[must_use]
pub fn folder(base: &Path, picture: EntityId) -> PathBuf {
    base.join("timelapse").join(picture.to_string())
}

/// The frames kept in `dir`, in order.
#[must_use]
pub fn frames(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = sue_platform::list_dir(dir, true) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .into_iter()
        .map(|entry| entry.path)
        .filter(|path| path.extension().is_some_and(|ext| ext == "jpg"))
        .collect();
    out.sort();
    out
}

/// `size` shrunk to fit a `side` by `side` box, keeping its shape.
#[must_use]
pub fn fit(size: [u32; 2], side: u32) -> [u32; 2] {
    let longest = size[0].max(size[1]).max(1);
    if longest <= side {
        return size;
    }
    let scale = f64::from(side) / f64::from(longest);
    [
        ((f64::from(size[0]) * scale).round() as u32).max(1),
        ((f64::from(size[1]) * scale).round() as u32).max(1),
    ]
}

/// Keeps `pixels` (RGBA8, `size`) as frame `index` in `dir`.
///
/// # Errors
/// If the frame will not encode or write.
pub fn capture(dir: &Path, index: usize, pixels: &[u8], size: [u32; 2]) -> Result<(), String> {
    let to = fit(size, FRAME_SIDE);
    let small = if to == size {
        pixels.to_vec()
    } else {
        crate::doc::resize_rgba(pixels, size, to)
    };
    let tile = RasterTile2D::from_bytes(to[0], to[1], PixelFormat::Rgba8, small)
        .map_err(|error| error.to_string())?;
    let jpeg = sue_io::encode_as(&tile, "jpg", 85, 1)?;
    sue_platform::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    sue_platform::write_atomic(&dir.join(format!("{index:06}.jpg")), &jpeg)
        .map_err(|error| error.to_string())
}

/// The shape of the video Export Timelapse makes. The drawing is fitted
/// into it on white, so a tall picture in a wide video has white at its
/// sides.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Aspect {
    /// As the picture was when the last frame was kept.
    #[default]
    Canvas,
    /// 16:9, wide, as most players and video sites show.
    Wide,
    /// 9:16, tall, as phones and short video sites show.
    Tall,
    /// 1:1, square.
    Square,
    /// 4:5, a little taller than wide, as picture feeds show.
    Portrait,
    /// 4:3, as older screens are.
    Classic,
}

impl Aspect {
    /// Every shape, in the order a dialog offers them.
    pub const ALL: [Self; 6] = [
        Self::Canvas,
        Self::Wide,
        Self::Tall,
        Self::Square,
        Self::Portrait,
        Self::Classic,
    ];

    /// Width to height, or `None` for the canvas's own.
    #[must_use]
    pub fn ratio(self) -> Option<(u32, u32)> {
        match self {
            Self::Canvas => None,
            Self::Wide => Some((16, 9)),
            Self::Tall => Some((9, 16)),
            Self::Square => Some((1, 1)),
            Self::Portrait => Some((4, 5)),
            Self::Classic => Some((4, 3)),
        }
    }

    /// What a person reads.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Canvas => tr!("aspect-same-as-canvas"),
            Self::Wide => tr!("aspect-wide"),
            Self::Tall => tr!("aspect-tall"),
            Self::Square => tr!("timelapse-aspect-square"),
            Self::Portrait => tr!("aspect-portrait"),
            Self::Classic => tr!("aspect-classic"),
        }
    }
}

/// How Export Timelapse lays the video out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Export {
    /// The longest side of the video, in pixels.
    pub side: u32,
    /// Fit to this many seconds; `None` exports every frame at 30 a second.
    pub seconds: Option<f32>,
    /// The video's shape.
    pub aspect: Aspect,
}

impl Default for Export {
    fn default() -> Self {
        Self {
            side: 1080,
            seconds: Some(30.0),
            aspect: Aspect::Canvas,
        }
    }
}

impl Export {
    /// The video's size for a drawing of `shape`: its long side `side`
    /// (a canvas smaller than that stays its size when the shape is the
    /// canvas's), both sides even, as video players like them.
    #[must_use]
    pub fn size(&self, shape: [u32; 2]) -> [u32; 2] {
        let even = |value: u32| (value.max(2) / 2) * 2;
        // The short side of a ratio: the even number nearest to it, so
        // 16:9 at 1,080 is 1,080 by 608 (607.5), not 606.
        let short = |side: u32, over: u32, under: u32| {
            let exact = u64::from(side) * u64::from(over);
            let under = u64::from(under.max(1));
            u32::try_from((exact + under) / (2 * under) * 2).unwrap_or(u32::MAX)
        };
        let side = self.side.max(16);
        let size = match self.aspect.ratio() {
            None => fit(shape, side),
            Some((wide, high)) if wide >= high => [side, short(side, high, wide)],
            Some((wide, high)) => [short(side, wide, high), side],
        };
        size.map(even)
    }
}

/// How a video of `count` kept frames laid out as `export` goes: its
/// frames a second, and every how many kept frames it shows one
/// ([`Timing`]).
#[must_use]
pub fn pace(count: usize, export: Export) -> (u32, usize) {
    let timing = Timing::new(count, export);
    (timing.fps, timing.step)
}

/// The fewest frames a second a video fitted to a length plays at: a few
/// kept frames are each held for several video frames, and at this rate
/// each is held for nearly the same time.
pub const FIT_FPS: u32 = 10;

/// The most frames a second a timelapse video plays at.
const MOST_FPS: u32 = 60;

/// How many kept frames a video of `count` shows when it shows every
/// `step`th, the last always among them.
#[must_use]
pub fn shown(count: usize, step: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let step = step.max(1);
    count.div_ceil(step) + usize::from(!(count - 1).is_multiple_of(step))
}

/// When each kept frame a timelapse video shows comes and how long it
/// stays, worked out from how many frames there are and the export's
/// length alone, so the dialog's preview and the export agree.
///
/// Not fitted, every kept frame shows for one video frame at 30 a
/// second and the last holds a second more. Fitted to a length, the
/// video lasts that length: the drawing as it grows fills it, each kept
/// frame held for its share (a few frames make a slow video, never a
/// short one), at [`FIT_FPS`] to 60 frames a second, skipping kept
/// frames when there are more than 60 a second; its last second shows
/// the finished picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Timing {
    /// Frames a second.
    pub fps: u32,
    /// Every how many kept frames one is shown.
    pub step: usize,
    /// The kept frames shown, in order, the last among them.
    pub picks: Vec<usize>,
    /// For each pick, the video frame after its last: pick `i` shows from
    /// `ends[i - 1]` (0 for the first) up to `ends[i]`.
    pub ends: Vec<usize>,
}

impl Timing {
    /// The timing of `count` kept frames laid out as `export`.
    #[must_use]
    pub fn new(count: usize, export: Export) -> Self {
        let pick = |step: usize| -> Vec<usize> {
            if count == 0 {
                return Vec::new();
            }
            let mut picks: Vec<usize> = (0..count).step_by(step.max(1)).collect();
            if picks.last() != Some(&(count - 1)) {
                picks.push(count - 1);
            }
            picks
        };
        let seconds = export
            .seconds
            .filter(|seconds| seconds.is_finite() && *seconds > 0.0);
        let Some(seconds) = seconds else {
            // Every frame at 30 a second, the last held a second.
            let picks = pick(1);
            let mut ends: Vec<usize> = (1..=picks.len()).collect();
            if let Some(last) = ends.last_mut() {
                *last += 30;
            }
            return Self {
                fps: 30,
                step: 1,
                picks,
                ends,
            };
        };
        // The finished picture holds the last second when the length
        // leaves room for the drawing before it.
        let held = if seconds >= 2.0 { 1.0 } else { 0.0 };
        let drawing = f64::from(seconds) - held;
        let wanted = (count as f64 / drawing).ceil();
        let fps = if wanted > f64::from(MOST_FPS) {
            MOST_FPS
        } else {
            (wanted as u32).clamp(FIT_FPS, MOST_FPS)
        };
        let total = ((f64::from(seconds) * f64::from(fps)).round() as usize).max(1);
        let hold = (held * f64::from(fps)).round() as usize;
        let room = total.saturating_sub(hold).max(1);
        // Kept frames are skipped until each one shown has a video frame
        // of its own.
        let mut step = count.div_ceil(room).max(1);
        while shown(count, step) > room {
            step += 1;
        }
        let picks = pick(step);
        let n = picks.len().max(1);
        let mut ends: Vec<usize> = (1..=picks.len()).map(|at| at * room / n).collect();
        if let Some(last) = ends.last_mut() {
            *last = total;
        }
        Self {
            fps,
            step,
            picks,
            ends,
        }
    }

    /// How many frames the video has.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ends.last().copied().unwrap_or(0)
    }

    /// `true` when it shows nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.picks.is_empty()
    }

    /// How long the video plays, in seconds.
    #[must_use]
    pub fn seconds(&self) -> f64 {
        self.len() as f64 / f64::from(self.fps.max(1))
    }

    /// Which pick the video's frame `index` shows (the last one past the
    /// end).
    #[must_use]
    pub fn pick_at(&self, index: usize) -> usize {
        self.ends
            .partition_point(|end| *end <= index)
            .min(self.picks.len().saturating_sub(1))
    }

    /// Which kept frame the video shows `at` seconds in.
    #[must_use]
    pub fn kept_at(&self, at: f64) -> usize {
        let frame = (at * f64::from(self.fps)).floor().max(0.0) as usize;
        self.picks.get(self.pick_at(frame)).copied().unwrap_or(0)
    }
}

/// Which kept frames a timelapse video shows, how big and how fast: what
/// Export Timelapse works from, read off a list of frames taken when it
/// began (frames kept while it runs are not in it).
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// The frame files shown, in order, the last frame among them.
    pub chosen: Vec<PathBuf>,
    /// The video's size.
    pub size: [u32; 2],
    /// Frames a second.
    pub fps: u32,
    /// When each chosen frame shows and for how long ([`Timing::ends`]).
    pub ends: Vec<usize>,
}

impl Plan {
    /// The plan for the frames `paths` laid out as `export` ([`Timing`]).
    /// The shape of the canvas is the last frame's.
    ///
    /// # Errors
    /// If there are no frames, or the last will not read.
    pub fn new(paths: &[PathBuf], export: Export) -> Result<Self, String> {
        let last = paths
            .last()
            .ok_or(tr!("canvas-has-no-timelapse-yet-record"))?;
        let shape = read_frame(last)?;
        Ok(Self::sized(paths, export, [shape.width(), shape.height()]))
    }

    /// [`Self::new`] for a canvas of `shape`, without reading a frame.
    #[must_use]
    pub fn sized(paths: &[PathBuf], export: Export, shape: [u32; 2]) -> Self {
        let size = export.size(shape);
        let timing = Timing::new(paths.len(), export);
        let chosen = timing
            .picks
            .iter()
            .filter_map(|index| paths.get(*index).cloned())
            .collect();
        Self {
            chosen,
            size,
            fps: timing.fps,
            ends: timing.ends,
        }
    }

    /// How many frames the video has.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ends.last().copied().unwrap_or(0)
    }

    /// `true` when it shows nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chosen.is_empty()
    }

    /// How long the video plays, in seconds.
    #[must_use]
    pub fn seconds(&self) -> f64 {
        self.len() as f64 / f64::from(self.fps.max(1))
    }

    /// Which chosen frame the video's frame `index` shows.
    #[must_use]
    pub fn source(&self, index: usize) -> usize {
        self.ends
            .partition_point(|end| *end <= index)
            .min(self.chosen.len().saturating_sub(1))
    }

    /// The video's frame `index`: its frame laid over white in the
    /// video's size.
    ///
    /// # Errors
    /// If the frame will not read.
    pub fn frame(&self, index: usize) -> Result<RasterTile2D, String> {
        let path = self
            .chosen
            .get(self.source(index))
            .ok_or(tr!("no-frames"))?;
        Ok(letterbox(&read_frame(path)?, self.size))
    }
}

/// A kept frame read back.
///
/// # Errors
/// If it will not read or decode.
pub fn read_frame(path: &Path) -> Result<RasterTile2D, String> {
    let bytes = sue_platform::fs::read(path).map_err(|error| error.to_string())?;
    Ok(sue_io::decode_jpeg(&bytes)?.tile)
}

/// The frames of `dir` read back, each laid over white in the video's
/// size, and the frames a second ([`Plan`]). The last frame holds a
/// second.
///
/// # Errors
/// If there are no frames, or one will not read.
pub fn video_frames(dir: &Path, export: Export) -> Result<(Vec<RasterTile2D>, u32), String> {
    let plan = Plan::new(&frames(dir), export)?;
    let out = (0..plan.len())
        .map(|index| plan.frame(index))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((out, plan.fps))
}

/// `tile` scaled to fit `size`, centered on white.
#[must_use]
pub fn letterbox(tile: &RasterTile2D, size: [u32; 2]) -> RasterTile2D {
    let from = [tile.width().max(1), tile.height().max(1)];
    let scale =
        (f64::from(size[0]) / f64::from(from[0])).min(f64::from(size[1]) / f64::from(from[1]));
    let to = [
        ((f64::from(from[0]) * scale).round() as u32).clamp(1, size[0]),
        ((f64::from(from[1]) * scale).round() as u32).clamp(1, size[1]),
    ];
    let pixels = tile
        .bytes()
        .map_or_else(Vec::new, |bytes| crate::doc::resize_rgba(bytes, from, to));
    let mut out = vec![255_u8; (size[0] * size[1] * 4) as usize];
    let (left, top) = ((size[0] - to[0]) / 2, (size[1] - to[1]) / 2);
    for y in 0..to[1] {
        let from_row = (y * to[0] * 4) as usize;
        let into_row = (((top + y) * size[0] + left) * 4) as usize;
        if let Some(row) = pixels.get(from_row..from_row + (to[0] * 4) as usize) {
            out[into_row..into_row + row.len()].copy_from_slice(row);
        }
    }
    RasterTile2D::from_bytes(size[0], size[1], PixelFormat::Rgba8, out)
        .unwrap_or_else(|_| tile.clone())
}

/// The timelapse in `dir` as a Motion JPEG AVI video, and how many frames
/// it has.
///
/// # Errors
/// As [`video_frames`].
pub fn export_avi(dir: &Path, export: Export) -> Result<(Vec<u8>, usize), String> {
    let (frames, fps) = video_frames(dir, export)?;
    let first = frames.first().ok_or(tr!("no-frames"))?;
    let mut video = sue_io::MjpegAvi::new(first.width(), first.height(), fps);
    for frame in &frames {
        video.push_frame(frame, 85)?;
    }
    let count = video.frames();
    Ok((video.finish(), count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_kept_and_export_as_a_video_of_the_drawing() {
        let dir = std::env::temp_dir().join(format!("sue-timelapse-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            video_frames(&dir, Export::default()).is_err(),
            "nothing recorded"
        );
        // Three steps of a 64 by 32 picture darkening.
        for (index, shade) in [255_u8, 160, 60].into_iter().enumerate() {
            let pixels = [shade, shade, shade, 255].repeat(64 * 32);
            capture(&dir, index, &pixels, [64, 32]).unwrap();
        }
        assert_eq!(frames(&dir).len(), 3);
        assert_eq!(fit([4000, 2000], 1280), [1280, 640]);
        // Every frame at 30 a second, the last held a second.
        let all = Export {
            side: 32,
            seconds: None,
            ..Export::default()
        };
        let (video, fps) = video_frames(&dir, all).unwrap();
        assert_eq!(fps, 30);
        assert_eq!(video.len(), 3 + 30);
        assert_eq!((video[0].width(), video[0].height()), (32, 16));
        assert!(
            video[0].bytes().unwrap()[0] > video[2].bytes().unwrap()[0],
            "it darkens"
        );
        let (avi, count) = export_avi(&dir, all).unwrap();
        assert_eq!(&avi[..4], b"RIFF");
        assert_eq!(count, 33);
        // Fit to a second: a second long, the three frames held evenly.
        let (video, fps) = video_frames(
            &dir,
            Export {
                side: 32,
                seconds: Some(1.0),
                ..Export::default()
            },
        )
        .unwrap();
        assert_eq!((fps, video.len()), (FIT_FPS, 10));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_video_takes_the_shape_asked_and_fits_the_drawing_on_white() {
        let wide = Export {
            side: 1280,
            aspect: Aspect::Wide,
            ..Export::default()
        };
        assert_eq!(wide.size([600, 800]), [1280, 720]);
        let tall = Export {
            aspect: Aspect::Tall,
            ..wide
        };
        assert_eq!(tall.size([600, 800]), [720, 1280]);
        let square = Export {
            aspect: Aspect::Square,
            ..wide
        };
        assert_eq!(square.size([600, 800]), [1280, 1280]);
        let portrait = Export {
            aspect: Aspect::Portrait,
            ..wide
        };
        assert_eq!(portrait.size([600, 800]), [1024, 1280]);
        // A short side that is not whole goes to the nearest even one.
        let odd = Export { side: 1080, ..wide };
        assert_eq!(odd.size([600, 800]), [1080, 608]);
        assert_eq!(Export { side: 1080, ..tall }.size([1, 1]), [608, 1080]);
        assert_eq!(
            Export {
                side: 1080,
                ..portrait
            }
            .size([1, 1]),
            [864, 1080]
        );
        // The canvas's own: a small canvas stays its size, sides even.
        assert_eq!(Export::default().size([301, 201]), [300, 200]);
        assert_eq!(Aspect::ALL.len(), 6);
        assert!(Aspect::ALL.iter().all(|aspect| !aspect.label().is_empty()));
        // A tall drawing in a wide video: white at its sides, the drawing
        // in the middle.
        let tile =
            RasterTile2D::from_bytes(2, 4, PixelFormat::Rgba8, [0, 0, 0, 255].repeat(8)).unwrap();
        let boxed = letterbox(&tile, [16, 8]);
        let px = |x: u32, y: u32| {
            let at = ((y * 16 + x) * 4) as usize;
            boxed.bytes().unwrap()[at]
        };
        assert_eq!(px(0, 4), 255, "white at the side");
        assert_eq!(px(8, 4), 0, "the drawing in the middle");
    }

    #[test]
    fn a_plan_is_made_from_the_frames_it_was_given() {
        let dir = std::env::temp_dir().join(format!("sue-timelapse-plan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for index in 0..5 {
            capture(&dir, index, &[128, 128, 128, 255].repeat(16 * 16), [16, 16]).unwrap();
        }
        let taken = frames(&dir);
        let plan = Plan::new(
            &taken,
            Export {
                side: 16,
                seconds: None,
                ..Export::default()
            },
        )
        .unwrap();
        // A frame kept after the plan was made is not in it.
        capture(&dir, 5, &[0, 0, 0, 255].repeat(16 * 16), [16, 16]).unwrap();
        assert_eq!(plan.chosen.len(), 5);
        assert_eq!(shown(5, 1), 5);
        assert_eq!(shown(5, 2), 3, "0, 2, 4");
        assert_eq!(shown(6, 4), 3, "0, 4 and the last, 5");
        assert_eq!(shown(0, 3), 0);
        assert_eq!(
            pace(
                5,
                Export {
                    seconds: None,
                    ..Export::default()
                }
            ),
            (30, 1)
        );
        assert_eq!(pace(7200, Export::default()), (60, 5));
        assert_eq!(plan.len(), 5 + 30);
        assert!((plan.seconds() - 35.0 / 30.0).abs() < 1e-9);
        assert_eq!(plan.source(plan.len() - 1), 4, "the last frame holds");
        assert!(plan.frame(34).unwrap().bytes().unwrap()[0] > 100);
        assert!(Plan::new(&[], Export::default()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_video_fitted_to_a_length_lasts_that_length_however_few_the_frames() {
        let thirty = Export::default();
        assert_eq!(thirty.seconds, Some(30.0));
        for count in [
            1_usize, 2, 3, 7, 45, 290, 291, 900, 1_740, 1_741, 7_200, 20_000,
        ] {
            let timing = Timing::new(count, thirty);
            let length = timing.seconds();
            assert!(
                (length - 30.0).abs() < 1e-9,
                "{count} frames: {length} s, not 30"
            );
            assert_eq!(timing.picks.first(), Some(&0), "{count}: the first frame");
            assert_eq!(timing.picks.last(), Some(&(count - 1)), "{count}: the last");
            assert!(
                (FIT_FPS..=60).contains(&timing.fps),
                "{count}: {}",
                timing.fps
            );
            // Every pick shows for at least a frame, in order.
            let mut before = 0;
            for end in &timing.ends {
                assert!(*end > before, "{count}: a pick with no frame");
                before = *end;
            }
            // The last second shows the finished picture.
            let last_second = timing.len() - timing.fps as usize;
            assert_eq!(timing.kept_at(29.5), count - 1, "{count}");
            assert_eq!(
                timing.pick_at(last_second),
                timing.picks.len() - 1,
                "{count}"
            );
            // The drawing before it: held evenly, no pick more than one
            // video frame longer than another.
            let spans: Vec<usize> = timing
                .ends
                .iter()
                .scan(0, |at, end| {
                    let span = end - *at;
                    *at = *end;
                    Some(span)
                })
                .collect();
            let drawing = &spans[..spans.len() - 1];
            if let (Some(least), Some(most)) = (drawing.iter().min(), drawing.iter().max()) {
                assert!(
                    most - least <= 1,
                    "{count}: held unevenly {least} to {most}"
                );
            }
        }
        // Three frames over 30 seconds: each of the first two shows for
        // nearly 15 seconds of the 29 before the last second, not 4 s.
        let few = Timing::new(3, thirty);
        assert_eq!(few.fps, FIT_FPS);
        assert_eq!(few.ends, vec![96, 193, 300]);
        assert_eq!(few.kept_at(0.0), 0);
        assert_eq!(few.kept_at(12.0), 1);
        assert_eq!(few.kept_at(20.0), 2);
        // Many frames skip some at 60 a second.
        let many = Timing::new(7_200, thirty);
        assert_eq!((many.fps, many.step), (60, 5));
        assert_eq!(pace(7_200, thirty), (60, 5));
        // Not fitted: every frame at 30 a second, the last held a second.
        let all = Timing::new(
            3,
            Export {
                seconds: None,
                ..thirty
            },
        );
        assert_eq!((all.fps, all.ends.clone()), (30, vec![1, 2, 33]));
        assert!(Timing::new(0, thirty).is_empty());
        assert_eq!(Timing::new(0, thirty).kept_at(1.0), 0);
        // A plan of three kept frames fitted to 30 s is a 30 s video.
        let paths: Vec<PathBuf> = (0..3)
            .map(|index| PathBuf::from(format!("{index}.jpg")))
            .collect();
        let plan = Plan::sized(&paths, thirty, [100, 100]);
        assert_eq!((plan.len(), plan.fps), (300, FIT_FPS));
        assert!((plan.seconds() - 30.0).abs() < 1e-9);
        assert_eq!(plan.source(150), 1);
        assert_eq!(plan.source(299), 2);
    }
}
