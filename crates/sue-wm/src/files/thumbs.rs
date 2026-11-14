//! Small pictures of files for the browser's cards, made in the
//! background and kept while the file stays the same.
//!
//! A `.sue` file that holds a preview (its `THMB` part) is shown from
//! that, without reading the document. Any other file goes to the
//! [`Thumbnailer`] of the studio whose work it is
//! ([`crate::SueApplet::thumbnail`]); without one, Sue reads the picture
//! formats it knows by itself.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sue_core::{JobContext, JobHandle, JobPool, RasterTile2D};

use crate::AppletId;

/// A picture made small for a card: RGBA8, laid over white.
#[derive(Clone, Debug)]
pub struct Thumb {
    /// Width and height in pixels.
    pub size: [u32; 2],
    /// The pixels, four bytes each.
    pub pixels: Vec<u8>,
}

/// The work a studio does to make a [`Thumb`] of a file: given the file,
/// its bytes, the longest side wanted and the background job it runs in.
type Make = dyn Fn(&Path, &[u8], u32, &JobContext) -> Option<Thumb> + Send + Sync;

/// How a studio turns one of its files into a [`Thumb`], in the
/// background. Cloning it is cheap.
#[derive(Clone)]
pub struct Thumbnailer(Arc<Make>);

impl Thumbnailer {
    /// A thumbnailer that runs `make`.
    pub fn new(
        make: impl Fn(&Path, &[u8], u32, &JobContext) -> Option<Thumb> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(make))
    }

    /// The thumbnail of `path`, whose bytes are `bytes`, at most `side`
    /// pixels on its long edge.
    #[must_use]
    pub fn make(&self, path: &Path, bytes: &[u8], side: u32, job: &JobContext) -> Option<Thumb> {
        (self.0)(path, bytes, side, job)
    }
}

impl std::fmt::Debug for Thumbnailer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Thumbnailer")
    }
}

/// How many pixels a card's picture has on its long edge.
pub const SIDE: u32 = 256;

/// Thumbnails, made in the background.
#[derive(Debug, Default)]
pub struct Thumbs {
    pool: Option<JobPool>,
    jobs: HashMap<PathBuf, (u64, JobHandle<Thumb>)>,
    done: HashMap<PathBuf, (u64, Option<Thumb>)>,
    /// What turns a file without a preview of its own into a picture,
    /// when no studio's own maker is known for it.
    maker: Option<Thumbnailer>,
    /// Each studio's own maker, for its work.
    makers: HashMap<AppletId, Thumbnailer>,
}

/// When `path` last changed, in seconds since 1970 (0: unknown).
#[must_use]
pub fn stamp(path: &Path) -> u64 {
    sue_platform::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified())
        .and_then(|time| time.duration_since(sue_platform::time::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_secs())
}

impl Thumbs {
    /// Thumbnails that `maker` makes, for files without a preview of their
    /// own.
    #[must_use]
    pub fn new(maker: Option<Thumbnailer>) -> Self {
        Self {
            maker,
            ..Self::default()
        }
    }

    /// Hands `studio`'s work to `maker` (the studio's
    /// [`crate::SueApplet::thumbnail`]) instead of the maker of
    /// [`Self::new`].
    pub fn set_maker(&mut self, studio: AppletId, maker: Thumbnailer) {
        self.makers.insert(studio, maker);
    }

    /// The thumbnail of `path` and when the file last changed, starting it
    /// in the background if it is new or changed.
    pub fn get(&mut self, path: &Path) -> Option<(u64, &Thumb)> {
        self.get_for(path, None)
    }

    /// The thumbnail of `path`, the work of `studio` when that is known,
    /// and when the file last changed, starting it in the background if
    /// it is new or changed. The studio's own maker makes it, if it has
    /// one ([`Self::set_maker`]).
    pub fn get_for(&mut self, path: &Path, studio: Option<&AppletId>) -> Option<(u64, &Thumb)> {
        let now = stamp(path);
        let fresh = self.done.get(path).is_some_and(|(at, _)| *at == now);
        if !fresh && !self.jobs.contains_key(path) {
            let pool = self.pool.get_or_insert_with(|| JobPool::new(1));
            let file = path.to_path_buf();
            let maker = studio
                .and_then(|studio| self.makers.get(studio))
                .or(self.maker.as_ref())
                .cloned();
            let job = pool.spawn(tr!("start-thumbnail"), move |job| {
                thumbnail(&file, maker.as_ref(), SIDE, job).ok_or_else(String::new)
            });
            self.jobs.insert(path.to_path_buf(), (now, job));
        }
        self.done
            .get(path)
            .and_then(|(at, thumb)| thumb.as_ref().map(|thumb| (*at, thumb)))
    }

    /// Takes in what the background finished. Returns `true` while any
    /// still runs.
    pub fn poll(&mut self) -> bool {
        let mut finished = Vec::new();
        for (path, (at, job)) in &mut self.jobs {
            if let Some(result) = job.try_take() {
                finished.push((path.clone(), *at, result.ok()));
            }
        }
        for (path, at, thumb) in finished {
            self.jobs.remove(&path);
            self.done.insert(path, (at, thumb));
        }
        !self.jobs.is_empty()
    }

    /// Forgets every thumbnail made, so each is made again when shown.
    pub fn clear(&mut self) {
        self.done.clear();
    }
}

/// The thumbnail of `path`: the preview a `.sue` file holds, else what
/// `maker` makes of it, else the picture Sue reads by itself.
fn thumbnail(
    path: &Path,
    maker: Option<&Thumbnailer>,
    side: u32,
    job: &JobContext,
) -> Option<Thumb> {
    let bytes = sue_platform::fs::read(path).ok()?;
    if let Some(thumb) = preview(&bytes, side) {
        return Some(thumb);
    }
    match maker {
        Some(maker) => maker.make(path, &bytes, side, job),
        None => {
            let tile = sue_io::picture_ops::read_picture(path, &bytes).ok()?;
            Some(shrink(&tile, side))
        }
    }
}

/// The preview a `.sue` file's `THMB` part holds (PNG or JPEG XL bytes),
/// made at most `side` on its long edge. `None` for any other file, a
/// file without one, or one that cannot be read; nothing else of the
/// document is decoded.
#[must_use]
pub fn preview(bytes: &[u8], side: u32) -> Option<Thumb> {
    if !sue_core::SueFile::sniff(bytes) {
        return None;
    }
    let mut input = std::io::Cursor::new(bytes);
    let directory = sue_core::SueFile::scan(&mut input).ok()?;
    let entry = directory.chunk(sue_core::file::THUMBNAIL)?;
    let chunk = sue_core::file::Directory::read(&mut input, entry).ok()?;
    let name = if chunk.payload.starts_with(b"\x89PNG") {
        "preview.png"
    } else {
        "preview.jxl"
    };
    let tile = sue_io::picture_ops::read_picture(Path::new(name), &chunk.payload).ok()?;
    Some(shrink(&tile, side))
}

/// `tile` (RGBA8) box-filtered down to at most `side` on its long edge,
/// over white.
#[must_use]
pub fn shrink(tile: &RasterTile2D, side: u32) -> Thumb {
    let (width, height) = (tile.width().max(1), tile.height().max(1));
    let scale = (side as f32 / width.max(height) as f32).min(1.0);
    let (w, h) = (
        ((width as f32 * scale).round() as u32).max(1),
        ((height as f32 * scale).round() as u32).max(1),
    );
    let bytes = tile.bytes().unwrap_or(&[]);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let (y0, y1) = (
            y * height / h,
            ((y + 1) * height / h).max(y * height / h + 1),
        );
        for x in 0..w {
            let (x0, x1) = (x * width / w, ((x + 1) * width / w).max(x * width / w + 1));
            let mut sum = [0_u64; 4];
            let mut count = 0_u64;
            // At most a 4 by 4 sample of the box, so big pictures stay quick.
            let step_y = ((y1 - y0) / 4).max(1);
            let step_x = ((x1 - x0) / 4).max(1);
            for sy in (y0..y1.min(height)).step_by(step_y as usize) {
                for sx in (x0..x1.min(width)).step_by(step_x as usize) {
                    let at = ((sy * width + sx) * 4) as usize;
                    if let Some(pixel) = bytes.get(at..at + 4) {
                        // Over white, as on paper.
                        let alpha = u64::from(pixel[3]);
                        for channel in 0..3 {
                            sum[channel] +=
                                (u64::from(pixel[channel]) * alpha + 255 * (255 - alpha)) / 255;
                        }
                        sum[3] += 255;
                        count += 1;
                    }
                }
            }
            let count = count.max(1);
            pixels.extend(sum.map(|value| (value / count) as u8));
        }
    }
    Thumb {
        size: [w, h],
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::PixelFormat;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sue-wm-thumbs-{tag}-{}", std::process::id()));
        let _ = sue_platform::fs::remove_dir_all(&dir);
        sue_platform::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Waits for the background to finish.
    fn settle(thumbs: &mut Thumbs) {
        let start = std::time::Instant::now();
        while thumbs.poll() {
            assert!(start.elapsed().as_secs() < 20);
            std::thread::yield_now();
        }
    }

    #[test]
    fn thumbnails_shrink_to_fit_over_white() {
        let mut pixels = Vec::new();
        for _ in 0..(400 * 200) {
            pixels.extend([255, 0, 0, 0]);
        }
        let tile = RasterTile2D::from_bytes(400, 200, PixelFormat::Rgba8, pixels).unwrap();
        let thumb = shrink(&tile, 100);
        assert_eq!(thumb.size, [100, 50]);
        // Clear pixels show the paper.
        assert_eq!(&thumb.pixels[..4], &[255, 255, 255, 255]);
        // A real picture file comes back small in the background.
        let root = temp("thumb");
        let png = sue_io::encode_png_rgba(64, 32, &vec![0; 64 * 32 * 4]).unwrap();
        let path = root.join("small.png");
        sue_platform::fs::write(&path, png).unwrap();
        let mut thumbs = Thumbs::default();
        assert!(thumbs.get(&path).is_none());
        settle(&mut thumbs);
        assert_eq!(thumbs.get(&path).unwrap().1.size, [64, 32]);
        let _ = sue_platform::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_sue_file_shows_its_own_preview_and_others_go_to_the_studio() {
        let root = temp("preview");
        // A project that holds a 40 by 20 preview, solid black.
        let png = sue_io::encode_png_rgba(40, 20, &[0, 0, 0, 255].repeat(40 * 20)).unwrap();
        let mut file = sue_core::SueFile::new();
        file.set(sue_core::file::Chunk {
            tag: sue_core::file::THUMBNAIL,
            version: 1,
            payload: png,
        });
        let project = root.join("with-preview.sue");
        sue_platform::fs::write(&project, file.to_bytes()).unwrap();
        // A project without one, and a file Sue cannot read itself.
        let bare = root.join("bare.sue");
        sue_platform::fs::write(&bare, sue_core::SueFile::new().to_bytes()).unwrap();
        let odd = root.join("odd.thing");
        sue_platform::fs::write(&odd, b"not a picture").unwrap();

        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = Arc::clone(&asked);
        let maker = Thumbnailer::new(move |path, bytes, side, _job| {
            seen.lock().unwrap().push(path.to_path_buf());
            assert!(!bytes.is_empty());
            Some(Thumb {
                size: [side.min(8), 1],
                pixels: vec![9; side.min(8) as usize * 4],
            })
        });
        let mut thumbs = Thumbs::new(Some(maker));
        for path in [&project, &bare, &odd] {
            assert!(thumbs.get(path).is_none());
        }
        settle(&mut thumbs);
        let (_, shown) = thumbs.get(&project).unwrap();
        assert_eq!(shown.size, [40, 20]);
        assert_eq!(&shown.pixels[..4], &[0, 0, 0, 255]);
        assert_eq!(thumbs.get(&bare).unwrap().1.size, [8, 1]);
        assert_eq!(thumbs.get(&odd).unwrap().1.size, [8, 1]);
        let mut asked = asked.lock().unwrap().clone();
        asked.sort();
        assert_eq!(
            asked,
            [bare.clone(), odd.clone()],
            "never the previewed one"
        );

        // Without a studio's thumbnailer a file Sue cannot read has none.
        let mut plain = Thumbs::default();
        assert!(plain.get(&odd).is_none());
        settle(&mut plain);
        assert!(plain.get(&odd).is_none());
        // A damaged preview is no preview.
        assert!(preview(b"SUE\0FILE", SIDE).is_none());
        assert!(preview(b"\x89PNG", SIDE).is_none());
        let _ = sue_platform::fs::remove_dir_all(&root);
    }

    /// A thumbnailer that makes a one pixel picture of `shade`.
    fn solid(shade: u8) -> Thumbnailer {
        Thumbnailer::new(move |_, _, _, _| {
            Some(Thumb {
                size: [1, 1],
                pixels: vec![shade; 4],
            })
        })
    }

    #[test]
    fn each_studios_work_goes_to_its_own_thumbnailer() {
        let root = temp("studios");
        let paint = AppletId::new("sue-paint").unwrap();
        let writer = AppletId::new("sue-writer").unwrap();
        let [picture, story, stray] =
            ["picture.thing", "story.thing", "stray.thing"].map(|name| root.join(name));
        for path in [&picture, &story, &stray] {
            sue_platform::fs::write(path, b"not a picture").unwrap();
        }
        let mut thumbs = Thumbs::new(Some(solid(1)));
        thumbs.set_maker(paint.clone(), solid(2));
        thumbs.set_maker(writer.clone(), solid(3));
        assert!(thumbs.get_for(&picture, Some(&paint)).is_none());
        assert!(thumbs.get_for(&story, Some(&writer)).is_none());
        // Work of no known studio goes to the maker every file has.
        assert!(thumbs.get(&stray).is_none());
        settle(&mut thumbs);
        let shade = |thumbs: &mut Thumbs, path: &Path| thumbs.get(path).unwrap().1.pixels[0];
        assert_eq!(shade(&mut thumbs, &picture), 2);
        assert_eq!(shade(&mut thumbs, &story), 3);
        assert_eq!(shade(&mut thumbs, &stray), 1);
        let _ = sue_platform::fs::remove_dir_all(&root);
    }
}
