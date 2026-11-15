//! Compositing: a picture's layers, read from the arena, laid over each
//! other with the shared blend modes ([`sue_model::BlendMode`]).
//!
//! [`render`] composites any rectangle (exports, merges, the eyedropper).
//! A [`Compositor`] keeps the screen image of one picture: it recomposites
//! everything when the layers changed behind its back (undo, a property, a
//! new layer) and only the painted rectangle while a stroke is drawn.
//! Large areas are split into bands of rows composited on every core.

#[cfg(test)]
mod folders_tests;
mod gather;
mod preview;
mod rows;
mod stroke;
mod view;

pub use preview::{preview, signature, thumbnail};
pub use stroke::view_step;
pub use view::{FrameView, Skin, flatten_view, render_view};

use sue_core::{Arena, EntityId, RasterTile2D};
use sue_model::picture;

use gather::{Occupancy, gather_view, prepare};
use rows::{composite_span, rows};
use stroke::Focus;

/// What a render includes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderOptions {
    /// Paper layers.
    pub paper: bool,
    /// Draft layers.
    pub drafts: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            paper: true,
            drafts: true,
        }
    }
}

/// The threads of the job pool compositing shares, once one is shared
/// (see [`share_pool`]); 0 until then.
static POOL_THREADS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Has compositing (and everything that asks [`workers`]) split big jobs
/// into no more parts than `pool` has threads, so the brush worker and the
/// compositor share one set of cores instead of each taking all of them.
/// A pool without threads ([`sue_core::JobPool::inline`]) makes every
/// composite run on the calling thread, as the browser must.
pub fn share_pool(pool: &sue_core::JobPool) {
    POOL_THREADS.store(pool.threads().max(1), std::sync::atomic::Ordering::Relaxed);
}

/// How many threads big jobs split into: the shared pool's threads once
/// one is shared ([`share_pool`]), else `SUE_THREADS` when set (1 measures
/// what the browser, which has no threads, goes through), else the cores.
#[must_use]
pub fn workers() -> usize {
    let shared = POOL_THREADS.load(std::sync::atomic::Ordering::Relaxed);
    if shared > 0 {
        return shared;
    }
    static COUNT: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *COUNT.get_or_init(|| {
        std::env::var("SUE_THREADS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|count| *count > 0)
            .unwrap_or_else(|| {
                std::thread::available_parallelism().map_or(1, std::num::NonZero::get)
            })
    })
}

/// Runs `work` on each of `items`, split over at most [`workers`] threads
/// when there is more than one item, and answers the results in the
/// order of `items`.
pub fn split<I: Sync, T: Send>(items: &[I], work: impl Fn(&I) -> T + Sync) -> Vec<T> {
    let threads = workers().min(items.len());
    if threads <= 1 {
        return items.iter().map(&work).collect();
    }
    let per = items.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let jobs: Vec<_> = items
            .chunks(per)
            .map(|chunk| {
                let work = &work;
                scope.spawn(move || chunk.iter().map(work).collect::<Vec<_>>())
            })
            .collect();
        jobs.into_iter()
            .flat_map(|job| job.join().unwrap_or_default())
            .collect()
    })
}

/// A stroke being painted that is not in its layer's pixels yet: the
/// brush worker's buffer, shown by the compositor over the layer as it
/// grows (see [`Compositor::set_live`]) and merged into the layer as one
/// edit when the pen lifts.
pub trait LiveStroke: Send + Sync + std::fmt::Debug {
    /// The layer it is painted on.
    fn layer(&self) -> EntityId;

    /// The layer pixels `[x0, y0, x1, y1]` it may change, or `None` while
    /// it changes nothing.
    fn bounds(&self) -> Option<[i64; 4]>;

    /// The layer's straight RGBA (0 to 1) at layer pixel `(x, y)` with the
    /// stroke on it, where `under` is the layer's own pixel there.
    fn over(&self, x: i64, y: i64, under: [f32; 4]) -> [f32; 4];
}

/// Composites the rectangle `[x0, y0, x1, y1]` of `picture` as straight
/// RGBA8, row by row.
#[must_use]
pub fn render(arena: &Arena, picture: EntityId, rect: [u32; 4], options: RenderOptions) -> Vec<u8> {
    render_view(arena, picture, rect, options, None)
}

/// The whole picture flattened into one straight RGBA8 raster.
#[must_use]
pub fn flatten(arena: &Arena, picture: EntityId, options: RenderOptions) -> Option<RasterTile2D> {
    flatten_view(arena, picture, options, None)
}

/// The screen image of one picture, kept in step with the arena.
#[derive(Debug)]
pub struct Compositor {
    picture: EntityId,
    size: [u32; 2],
    display: Vec<u8>,
    seen: Vec<(EntityId, u64)>,
    version: u64,
    view: Option<FrameView>,
    /// The rectangles repainted lately, by the version each made, so a
    /// texture can take just what changed (see [`Self::changed_since`]).
    changes: std::collections::VecDeque<(u64, Vec<[u32; 4]>)>,
    /// The newest version no longer on record: what changed up to it is
    /// unknown.
    forgotten: u64,
    /// The layer being painted and the stacks around it.
    focus: Option<Focus>,
    /// Each layer's [`Occupancy`], for the revision and offset it was
    /// found at.
    occupancy: std::collections::HashMap<EntityId, (u64, [i64; 2], std::sync::Arc<Occupancy>)>,
    /// The 64 pixel tiles shown coarse, or not redrawn at all because they
    /// were off screen, waiting for full detail (see [`Self::refine`]),
    /// by (column, row).
    pending: std::collections::BTreeSet<(u32, u32)>,
    /// The layer the waiting tiles were painted on.
    pending_layer: Option<EntityId>,
    /// The stroke being painted, shown over its layer (see
    /// [`Self::set_live`]).
    live: Option<std::sync::Arc<dyn LiveStroke>>,
}

/// Versions come from one count for every compositor, so a texture made
/// from another one (a picture closed and opened again) never passes for
/// this one's.
fn next_version() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl Compositor {
    /// A compositor for `picture`; it renders on the first [`Self::sync`].
    #[must_use]
    pub fn new(picture: EntityId) -> Self {
        let version = next_version();
        Self {
            picture,
            size: [0, 0],
            display: Vec::new(),
            seen: Vec::new(),
            version,
            view: None,
            changes: std::collections::VecDeque::new(),
            forgotten: version,
            focus: None,
            occupancy: std::collections::HashMap::new(),
            pending: std::collections::BTreeSet::new(),
            pending_layer: None,
            live: None,
        }
    }

    /// Shows `live` (a stroke not yet in its layer's pixels) over its
    /// layer from now on, or stops showing one (`None`: the stroke was
    /// merged into the layer, or given up). What the old and the new
    /// stroke cover is composited again at once.
    pub fn set_live(&mut self, arena: &Arena, live: Option<std::sync::Arc<dyn LiveStroke>>) {
        let mut rects = Vec::new();
        for stroke in self.live.iter().chain(live.iter()) {
            if let Some(rect) = self.live_rect(arena, stroke.as_ref()) {
                rects.push(rect);
            }
        }
        let painted = live
            .as_ref()
            .or(self.live.as_ref())
            .map(|stroke| stroke.layer());
        self.live = live;
        if !rects.is_empty() && self.size != [0, 0] {
            self.refresh_rects(arena, &rects, painted);
        }
    }

    /// The stroke shown over its layer, if one is.
    #[must_use]
    pub fn live(&self) -> Option<&std::sync::Arc<dyn LiveStroke>> {
        self.live.as_ref()
    }

    /// The picture pixels `stroke` may change, cut to the picture.
    fn live_rect(&self, arena: &Arena, stroke: &dyn LiveStroke) -> Option<[u32; 4]> {
        let [x0, y0, x1, y1] = stroke.bounds()?;
        let offset = arena
            .get_by_id(stroke.layer())
            .map(|entity| picture::LayerStyle::of(entity).offset)
            .unwrap_or_default();
        let clamp = |value: i64, side: u32| value.clamp(0, i64::from(side)) as u32;
        let rect = [
            clamp(x0 + offset[0], self.size[0]),
            clamp(y0 + offset[1], self.size[1]),
            clamp(x1 + offset[0], self.size[0]),
            clamp(y1 + offset[1], self.size[1]),
        ];
        (rect[0] < rect[2] && rect[1] < rect[3]).then_some(rect)
    }

    /// The picture it shows.
    #[must_use]
    pub fn picture(&self) -> EntityId {
        self.picture
    }

    /// Opaque RGBA8 pixels for the screen (a checkerboard shows through
    /// transparent areas), their size and a version that changes with them.
    #[must_use]
    pub fn display(&self) -> (&[u8], [u32; 2], u64) {
        (&self.display, self.size, self.version)
    }

    /// The screen color at a picture pixel.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.size[0] || y >= self.size[1] {
            return None;
        }
        let at = (y as usize * self.size[0] as usize + x as usize) * 4;
        self.display.get(at..at + 4)?.try_into().ok()
    }

    /// The rectangles `[x0, y0, x1, y1]` holding every pixel that changed
    /// since `version` (none when nothing did), or `None` when that is too
    /// long ago to know.
    #[must_use]
    pub fn changed_since(&self, version: u64) -> Option<Vec<[u32; 4]>> {
        if version == self.version {
            return Some(Vec::new());
        }
        // Every change after `version` must still be on record.
        if version < self.forgotten || version > self.version {
            return None;
        }
        let mut rects: Vec<[u32; 4]> = self
            .changes
            .iter()
            .filter(|(made, _)| *made > version)
            .flat_map(|(_, rects)| rects.iter().copied())
            .collect();
        // Many small pieces cost more to send one by one than together.
        if rects.len() > 256 {
            let all = rects
                .iter()
                .fold(rects[0], |[a, b, c, d], [x0, y0, x1, y1]| {
                    [a.min(*x0), b.min(*y0), c.max(*x1), d.max(*y1)]
                });
            rects = vec![all];
        }
        Some(rects)
    }

    fn signature(&self, arena: &Arena) -> Vec<(EntityId, u64)> {
        signature(arena, self.picture)
    }

    /// Recomposites everything if the layers changed since the last look.
    /// Returns `true` if the image changed.
    pub fn sync(&mut self, arena: &Arena) -> bool {
        let Some(size) = arena.get_by_id(self.picture).ok().and_then(picture::size) else {
            return false;
        };
        let seen = self.signature(arena);
        if size == self.size && seen == self.seen {
            return false;
        }
        self.size = size;
        self.display = vec![0; size[0] as usize * size[1] as usize * 4];
        self.paint(arena, [0, 0, size[0], size[1]]);
        self.seen = seen;
        self.pending.clear();
        self.pending_layer = None;
        true
    }

    fn paint(&mut self, arena: &Arena, [x0, y0, x1, y1]: [u32; 4]) {
        let mut layers = gather_view(
            arena,
            self.picture,
            RenderOptions::default(),
            self.view.as_ref(),
        );
        prepare(&mut layers, [x0, y0, x1, y1]);
        // A whole picture finds what each layer covers; a piece reuses it.
        let whole = [x0, y0, x1, y1] == [0, 0, self.size[0], self.size[1]];
        self.attach(arena, &mut layers, whole);
        let stride = self.size[0] as usize * 4;
        let band = &mut self.display[y0 as usize * stride..y1 as usize * stride];
        let area = (x1.saturating_sub(x0) * y1.saturating_sub(y0)) as usize;
        rows(band, stride, y0, area, |y, row| {
            composite_span(&layers, y, x0, x1, row);
        });
        self.version = next_version();
        self.changes
            .push_back((self.version, vec![[x0, y0, x1, y1]]));
        if self.changes.len() > 64
            && let Some((version, _)) = self.changes.pop_front()
        {
            self.forgotten = version;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::gather::{Layer, gather};
    use super::rows::{RowSource, checker, pixel, pixel_plain, shown};
    use super::*;
    use crate::doc::{self, NewPicture};
    use sue_core::txn::SetProperty;
    use sue_core::{Color, ReversibleTransaction};

    fn picture() -> (Arena, EntityId, EntityId) {
        let mut arena = Arena::new();
        let (mut edit, picture) = doc::create(&NewPicture {
            name: "Test".to_owned(),
            width: 8,
            height: 4,
            dpi: 72.0,
            paper: Some(Color::srgb8(255, 255, 255)),
            timelapse: false,
        })
        .unwrap();
        edit.apply(&mut arena).unwrap();
        let layer = doc::layers(&arena, picture)
            .last()
            .map(|info| info.id)
            .unwrap();
        (arena, picture, layer)
    }

    fn paint(arena: &mut Arena, layer: EntityId, x: u32, y: u32, rgba: [u8; 4]) {
        let raster = doc::raster_mut(arena, layer).unwrap();
        let at = (y * raster.width() + x) as usize * 4;
        raster.bytes_mut().unwrap()[at..at + 4].copy_from_slice(&rgba);
    }

    #[test]
    fn layers_stack_over_the_paper() {
        let (mut arena, picture, layer) = picture();
        paint(&mut arena, layer, 1, 1, [0, 0, 0, 128]);
        let all = render(&arena, picture, [0, 0, 8, 4], RenderOptions::default());
        assert_eq!(&all[..4], &[255, 255, 255, 255], "paper");
        let at = (8 + 1) * 4;
        assert_eq!(
            &all[at..at + 4],
            &[127, 127, 127, 255],
            "half black on white"
        );
        let clear = render(
            &arena,
            picture,
            [0, 0, 8, 4],
            RenderOptions {
                paper: false,
                drafts: true,
            },
        );
        assert_eq!(&clear[..4], &[0, 0, 0, 0]);
        assert_eq!(&clear[at..at + 4], &[0, 0, 0, 128]);
        // Multiply with white leaves the paper as it was.
        let mut mode = SetProperty::new(layer, picture::BLEND, "multiply");
        mode.apply(&mut arena).unwrap();
        paint(&mut arena, layer, 2, 2, [255, 255, 255, 255]);
        let multiplied = render(&arena, picture, [2, 2, 3, 3], RenderOptions::default());
        assert_eq!(multiplied, [255, 255, 255, 255]);
        let flat = flatten(&arena, picture, RenderOptions::default()).unwrap();
        assert_eq!((flat.width(), flat.height()), (8, 4));
    }

    #[test]
    fn clipped_layers_show_only_over_the_layer_below() {
        let (mut arena, picture, base) = picture();
        let (mut spawn, top) =
            doc::add_raster_layer(&arena, picture, Some(base), "Shade".to_owned(), None).unwrap();
        spawn.apply(&mut arena).unwrap();
        let mut clip = SetProperty::new(top, picture::CLIP, true);
        clip.apply(&mut arena).unwrap();
        paint(&mut arena, base, 0, 0, [0, 0, 255, 255]);
        paint(&mut arena, top, 0, 0, [255, 0, 0, 255]);
        paint(&mut arena, top, 1, 0, [255, 0, 0, 255]);
        let row = render(
            &arena,
            picture,
            [0, 0, 2, 1],
            RenderOptions {
                paper: false,
                drafts: true,
            },
        );
        assert_eq!(&row[..4], &[255, 0, 0, 255], "red over the blue base");
        assert_eq!(&row[4..], &[0, 0, 0, 0], "nothing where the base is clear");
    }

    #[test]
    fn the_fast_path_composites_exactly_as_the_full_one() {
        let (mut arena, picture, base) = picture();
        let (mut spawn, top) =
            doc::add_raster_layer(&arena, picture, Some(base), "Top".to_owned(), None).unwrap();
        spawn.apply(&mut arena).unwrap();
        let (mut spawn, clipped) =
            doc::add_raster_layer(&arena, picture, Some(top), "Clipped".to_owned(), None).unwrap();
        spawn.apply(&mut arena).unwrap();
        SetProperty::new(clipped, picture::CLIP, true)
            .apply(&mut arena)
            .unwrap();
        SetProperty::new(top, picture::OPACITY, 0.6)
            .apply(&mut arena)
            .unwrap();
        for (i, layer) in [base, top, clipped].into_iter().enumerate() {
            for x in 0..8 {
                for y in 0..4 {
                    if !(x + y + i as u32).is_multiple_of(3) {
                        let v = (x * 31 + y * 17 + i as u32 * 50) as u8;
                        paint(&mut arena, layer, x, y, [v, 255 - v, 90, 60 + v / 2]);
                    }
                }
            }
        }
        let layers = gather(&arena, picture, RenderOptions::default());
        assert!(layers.iter().all(Layer::plain));
        for y in 0..4 {
            let rows: Vec<RowSource<'_>> = layers.iter().map(|layer| layer.row(y)).collect();
            for x in 0..8 {
                assert_eq!(
                    pixel_plain(&layers, &rows, x, y),
                    pixel(&layers, x, y),
                    "{x},{y}"
                );
            }
        }
    }

    #[test]
    fn painting_through_the_layer_cache_shows_what_a_full_composite_does() {
        let (mut arena, picture, base) = picture();
        let mut below = Some(base);
        let mut ids = vec![base];
        for name in ["Middle", "Top", "Clipped"] {
            let (mut spawn, id) =
                doc::add_raster_layer(&arena, picture, below, name.to_owned(), None).unwrap();
            spawn.apply(&mut arena).unwrap();
            ids.push(id);
            below = Some(id);
        }
        SetProperty::new(ids[3], picture::CLIP, true)
            .apply(&mut arena)
            .unwrap();
        SetProperty::new(ids[2], picture::OPACITY, 0.7)
            .apply(&mut arena)
            .unwrap();
        for (i, layer) in ids.iter().enumerate() {
            for x in 0..8 {
                for y in 0..4 {
                    if !(x * 3 + y + i as u32).is_multiple_of(4) {
                        let v = (x * 29 + y * 13 + i as u32 * 61) as u8;
                        paint(&mut arena, *layer, x, y, [v, 200 - v / 2, 70, 90 + v / 3]);
                    }
                }
            }
        }
        // What the screen should show: the full composite over the
        // checkerboard, give or take a step for the kept bytes.
        let check = |compositor: &Compositor, arena: &Arena| {
            let full = render(arena, picture, [0, 0, 8, 4], RenderOptions::default());
            for y in 0..4 {
                for x in 0..8 {
                    let at = ((y * 8 + x) * 4) as usize;
                    let want = &full[at..at + 4];
                    let alpha = f32::from(want[3]) / 255.0;
                    let under = checker(x, y);
                    let got = compositor.pixel(x, y).unwrap();
                    for channel in 0..3 {
                        let shown = (f32::from(want[channel]) / 255.0 * alpha
                            + under * (1.0 - alpha))
                            * 255.0;
                        let diff = (f32::from(got[channel]) - shown.round()).abs();
                        assert!(
                            diff <= 2.0,
                            "{x},{y} channel {channel}: {got:?} vs {want:?}"
                        );
                    }
                }
            }
        };
        // Painting the middle layer (over it a clipped pair: not plain),
        // the top one (plain over it) and the base.
        for painted in [ids[1], ids[2], ids[0]] {
            let mut compositor = Compositor::new(picture);
            compositor.sync(&arena);
            for round in 0..3 {
                paint(&mut arena, painted, round * 2, 1, [10, 250, 10, 255]);
                compositor.refresh_rects(&arena, &[[0, 0, 8, 4]], Some(painted));
                assert!(compositor.focus.is_some(), "the stacks are kept");
                check(&compositor, &arena);
            }
            // Anything else changing drops the kept stacks, so the screen
            // follows it.
            let other = if painted == ids[0] { ids[1] } else { ids[0] };
            paint(&mut arena, other, 7, 3, [0, 0, 0, 255]);
            compositor.refresh_rects(&arena, &[[0, 0, 8, 4]], Some(painted));
            check(&compositor, &arena);
        }
    }

    #[test]
    fn empty_tiles_are_passed_over_exactly_as_clear_pixels() {
        let mut arena = Arena::new();
        let (mut edit, picture) = doc::create(&NewPicture {
            name: "Tiles".to_owned(),
            width: 200,
            height: 150,
            dpi: 72.0,
            paper: Some(Color::srgb8(255, 255, 255)),
            timelapse: false,
        })
        .unwrap();
        edit.apply(&mut arena).unwrap();
        let base = doc::layers(&arena, picture).last().unwrap().id;
        let mut ids = vec![base];
        let mut below = Some(base);
        for name in ["Empty base", "Clipped", "Corner"] {
            let (mut spawn, id) =
                doc::add_raster_layer(&arena, picture, below, name.to_owned(), None).unwrap();
            spawn.apply(&mut arena).unwrap();
            ids.push(id);
            below = Some(id);
        }
        // A clipped layer over a base that is empty in most tiles: there it
        // must show nothing.
        SetProperty::new(ids[2], picture::CLIP, true)
            .apply(&mut arena)
            .unwrap();
        for x in 0..200 {
            for y in 0..150 {
                paint(&mut arena, ids[2], x, y, [200, 30, 30, 255]);
                if (x + y) % 5 == 0 {
                    paint(&mut arena, base, x, y, [10, 10, 200, 120]);
                }
            }
        }
        for x in 70..90 {
            for y in 70..80 {
                paint(&mut arena, ids[1], x, y, [0, 200, 0, 255]);
            }
        }
        for x in 150..200 {
            for y in 0..20 {
                paint(&mut arena, ids[3], x, y, [0, 0, 0, 90]);
            }
        }
        let mut compositor = Compositor::new(picture);
        compositor.sync(&arena);
        assert!(
            compositor.occupancy.len() >= 4,
            "the layers' tiles are known"
        );
        let full = render(&arena, picture, [0, 0, 200, 150], RenderOptions::default());
        for y in 0..150 {
            for x in 0..200 {
                let at = ((y * 200 + x) * 4) as usize;
                let want = &full[at..at + 4];
                let alpha = f32::from(want[3]) / 255.0;
                let under = checker(x, y);
                let shown: Vec<u8> = (0..3)
                    .map(|c| {
                        ((f32::from(want[c]) / 255.0 * alpha + under * (1.0 - alpha))
                            .clamp(0.0, 1.0)
                            * 255.0)
                            .round() as u8
                    })
                    .collect();
                assert_eq!(&compositor.pixel(x, y).unwrap()[..3], &shown[..], "{x},{y}");
            }
        }
    }

    #[test]
    fn coarse_and_off_screen_pieces_come_to_full_detail() {
        let mut arena = Arena::new();
        let (mut edit, picture) = doc::create(&NewPicture {
            name: "View".to_owned(),
            width: 300,
            height: 200,
            dpi: 72.0,
            paper: Some(Color::srgb8(255, 255, 255)),
            timelapse: false,
        })
        .unwrap();
        edit.apply(&mut arena).unwrap();
        let layer = doc::layers(&arena, picture).last().unwrap().id;
        let mut compositor = Compositor::new(picture);
        compositor.sync(&arena);
        for x in 10..290 {
            for y in 20..180 {
                if (x / 3 + y / 7) % 2 == 0 {
                    paint(
                        &mut arena,
                        layer,
                        x,
                        y,
                        [(x % 256) as u8, (y % 256) as u8, 90, 200],
                    );
                }
            }
        }
        // Zoomed out to a quarter, with only the left half on screen.
        assert_eq!(view_step(0.25), 4);
        assert_eq!(view_step(0.6), 1);
        assert_eq!(view_step(0.01), 8);
        compositor.refresh_view(
            &arena,
            &[[0, 0, 300, 200]],
            Some(layer),
            [0, 0, 150, 200],
            4,
        );
        assert!(compositor.refining());
        // Off screen: not drawn yet.
        assert_eq!(compositor.pixel(250, 100), Some([255, 255, 255, 255]));
        let mut rounds = 0;
        while compositor.refine(&arena, [0, 0, 150, 200], 5) {
            rounds += 1;
            assert!(rounds < 100);
        }
        assert!(!compositor.refining());
        let full = render(&arena, picture, [0, 0, 300, 200], RenderOptions::default());
        for y in 0..200 {
            for x in 0..300 {
                let at = ((y * 300 + x) * 4) as usize;
                let want = &full[at..at + 4];
                let color = [0, 1, 2, 3].map(|c| f32::from(want[c]) / 255.0);
                let got = compositor.pixel(x, y).unwrap();
                let expect = shown(color, x, y);
                for c in 0..3 {
                    assert!(
                        got[c].abs_diff(expect[c]) <= 2,
                        "{x},{y}: {got:?} vs {expect:?}"
                    );
                }
            }
        }
        // Writing to the painted layer without changing it is no change;
        // anything else still is.
        let before = compositor.display().2;
        let _ = doc::raster_mut(&mut arena, layer);
        compositor.accept(&arena, layer);
        assert!(!compositor.sync(&arena), "nothing to recomposite");
        assert_eq!(compositor.display().2, before);
        let (mut spawn, _) =
            doc::add_raster_layer(&arena, picture, Some(layer), "New".to_owned(), None).unwrap();
        spawn.apply(&mut arena).unwrap();
        compositor.accept(&arena, layer);
        assert!(compositor.sync(&arena), "a new layer is a change");
        // A full composite drops whatever still waited.
        compositor.refresh_view(&arena, &[[0, 0, 300, 200]], Some(layer), [0, 0, 10, 10], 8);
        paint(&mut arena, layer, 0, 0, [0, 0, 0, 255]);
        compositor.sync(&arena);
        assert!(!compositor.refining());
    }

    #[test]
    fn the_compositor_follows_the_arena() {
        let (mut arena, picture, layer) = picture();
        let mut compositor = Compositor::new(picture);
        assert!(compositor.sync(&arena));
        assert!(!compositor.sync(&arena), "nothing changed");
        let (_, size, first) = compositor.display();
        assert_eq!(size, [8, 4]);
        paint(&mut arena, layer, 3, 2, [0, 0, 0, 255]);
        compositor.refresh(&arena, [3, 2, 4, 3]);
        assert_eq!(compositor.pixel(3, 2), Some([0, 0, 0, 255]));
        assert!(!compositor.sync(&arena), "the refresh saw the edit");
        // A texture at the first version takes just the refreshed pixel.
        assert_eq!(compositor.changed_since(first), Some(vec![[3, 2, 4, 3]]));
        assert_eq!(
            compositor.changed_since(compositor.display().2),
            Some(Vec::new())
        );
        compositor.refresh(&arena, [0, 0, 1, 1]);
        assert_eq!(
            compositor.changed_since(first),
            Some(vec![[3, 2, 4, 3], [0, 0, 1, 1]])
        );
        // Several pieces at once: each is composited, and only they.
        paint(&mut arena, layer, 6, 0, [0, 0, 0, 255]);
        paint(&mut arena, layer, 1, 3, [0, 0, 0, 255]);
        let before = compositor.display().2;
        compositor.refresh_rects(&arena, &[[6, 0, 7, 1], [1, 3, 2, 4]], None);
        assert_eq!(compositor.pixel(6, 0), Some([0, 0, 0, 255]));
        assert_eq!(compositor.pixel(1, 3), Some([0, 0, 0, 255]));
        assert_eq!(
            compositor.changed_since(before),
            Some(vec![[6, 0, 7, 1], [1, 3, 2, 4]])
        );
        for _ in 0..70 {
            compositor.refresh(&arena, [0, 0, 1, 1]);
        }
        assert_eq!(compositor.changed_since(first), None, "too long ago");
        // Another compositor's versions never pass for this one's.
        let other = Compositor::new(picture);
        assert_eq!(compositor.changed_since(other.display().2), None);
        assert_eq!(other.changed_since(compositor.display().2), None);
        let mut hide = SetProperty::new(layer, picture::VISIBLE, false);
        hide.apply(&mut arena).unwrap();
        assert!(compositor.sync(&arena), "a hidden layer recomposites");
        assert_eq!(compositor.pixel(3, 2), Some([255, 255, 255, 255]));
        assert!(compositor.display().2 > first);
        let (thumb, thumb_size) = thumbnail(&arena, layer, [16, 16]).unwrap();
        assert_eq!(thumb_size, [16, 8]);
        assert_eq!(thumb.len(), 16 * 8 * 4);
        let (small, small_size) = preview(&arena, picture, [4, 4]).unwrap();
        assert_eq!(small_size, [4, 2]);
        assert_eq!(&small[..4], &[255, 255, 255, 255], "the paper, opaque");
        let before = signature(&arena, picture);
        paint(&mut arena, layer, 0, 0, [1, 1, 1, 255]);
        assert_ne!(signature(&arena, picture), before);
    }

    /// A stroke for the tests: opaque red over the layer pixels of `rect`.
    #[derive(Debug)]
    struct Red {
        layer: EntityId,
        rect: [i64; 4],
    }

    impl LiveStroke for Red {
        fn layer(&self) -> EntityId {
            self.layer
        }

        fn bounds(&self) -> Option<[i64; 4]> {
            Some(self.rect)
        }

        fn over(&self, x: i64, y: i64, under: [f32; 4]) -> [f32; 4] {
            let [x0, y0, x1, y1] = self.rect;
            if x >= x0 && x < x1 && y >= y0 && y < y1 {
                [1.0, 0.0, 0.0, 1.0]
            } else {
                under
            }
        }
    }

    #[test]
    fn a_live_stroke_shows_over_its_layer_until_it_is_let_go() {
        let (mut arena, picture, layer) = picture();
        // The layer sits one pixel right: the stroke follows it.
        SetProperty::new(layer, picture::X, 1_i64)
            .apply(&mut arena)
            .unwrap();
        let mut compositor = Compositor::new(picture);
        compositor.sync(&arena);
        let white = compositor.pixel(2, 1).unwrap();
        assert_eq!(white, [255, 255, 255, 255]);
        let stroke: std::sync::Arc<dyn LiveStroke> = std::sync::Arc::new(Red {
            layer,
            rect: [1, 1, 3, 2],
        });
        compositor.set_live(&arena, Some(stroke));
        assert_eq!(compositor.pixel(2, 1), Some([255, 0, 0, 255]));
        assert_eq!(compositor.pixel(3, 1), Some([255, 0, 0, 255]));
        assert_eq!(compositor.pixel(1, 1), Some(white), "left of the stroke");
        // A refresh of what a stroke changed, and a full one, show it too.
        compositor.refresh_rects(&arena, &[[0, 0, 8, 4]], Some(layer));
        assert_eq!(compositor.pixel(2, 1), Some([255, 0, 0, 255]));
        paint(&mut arena, layer, 6, 3, [0, 0, 0, 255]);
        assert!(compositor.sync(&arena));
        assert_eq!(compositor.pixel(3, 1), Some([255, 0, 0, 255]));
        assert!(compositor.live().is_some());
        // Given up: the layer as it is shows again, and nothing stays.
        compositor.set_live(&arena, None);
        assert_eq!(compositor.pixel(2, 1), Some(white));
        assert!(compositor.live().is_none());
        // Exports never show a stroke that is not in the layer.
        let all = render(&arena, picture, [2, 1, 3, 2], RenderOptions::default());
        assert_eq!(all, vec![255, 255, 255, 255]);
    }

    #[test]
    fn work_is_split_in_order_over_the_shared_threads() {
        let items: Vec<u32> = (0..100).collect();
        assert_eq!(
            split(&items, |item| item * 2),
            (0..100).map(|item| item * 2).collect::<Vec<_>>()
        );
        assert!(split(&[] as &[u32], |item| *item).is_empty());
        // Sharing a pool of as many threads as compositing takes already
        // changes nothing for the other tests running beside this one.
        let pool = sue_core::JobPool::new(workers());
        share_pool(&pool);
        assert_eq!(workers(), pool.threads());
    }
}

/// Timings of compositing big areas, for tuning (`cargo test --release -p
/// sue-picture composite_speed -- --ignored --nocapture`).
#[cfg(test)]
mod speed {
    use super::*;
    use crate::doc::{self, NewPicture};
    use std::time::Instant;
    use sue_core::{Color, ReversibleTransaction};

    #[test]
    #[ignore = "a benchmark"]
    fn composite_speed() {
        let mut arena = Arena::new();
        let (mut edit, picture) = doc::create(&NewPicture {
            name: "Big".to_owned(),
            width: 4000,
            height: 3000,
            dpi: 72.0,
            paper: Some(Color::srgb8(255, 255, 255)),
            timelapse: false,
        })
        .unwrap();
        edit.apply(&mut arena).unwrap();
        let mut below = doc::layers(&arena, picture).last().map(|info| info.id);
        for i in 0..6 {
            let (mut spawn, layer) =
                doc::add_raster_layer(&arena, picture, below, format!("L{i}"), None).unwrap();
            spawn.apply(&mut arena).unwrap();
            if let Some(bytes) = doc::raster_mut(&mut arena, layer).and_then(|r| r.bytes_mut()) {
                for (n, pixel) in bytes.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    if (n / 4000 + n % 4000 + i * 300) % 900 < 400 {
                        pixel.copy_from_slice(&[(i * 40) as u8, 90, 200, 180]);
                    }
                }
            }
            below = Some(layer);
        }
        let mut compositor = Compositor::new(picture);
        let start = Instant::now();
        compositor.sync(&arena);
        println!(
            "full composite 4000x3000, 8 layers: {:.1} ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
        // Frames of a big stroke on the top layer: the brush, then the
        // screen, the old way (the square around what changed) and the new
        // (just the changed tiles).
        let top = below.unwrap();
        for (soft, size) in [(false, 1500.0), (true, 1500.0), (false, 300.0)] {
            for tiles in [false, true] {
                let tip = crate::brush::Tip {
                    size,
                    hardness: if soft { 0.3 } else { 1.0 },
                    eraser: soft,
                    size_by: crate::brush::Dynamics::NONE,
                    ..crate::brush::Tip::default()
                };
                let mut stroke = crate::brush::Stroke::new(top, tip, [0.1, 0.1, 0.1], false);
                let frames = 60;
                let start = Instant::now();
                for frame in 0..frames {
                    let first = Instant::now();
                    for step in 0..2 {
                        let i = (frame * 2 + step) as f32;
                        let raster = doc::raster_mut(&mut arena, top).unwrap();
                        stroke.to(raster, [900.0 + i * 3.0, 1500.0], 1.0, None);
                    }
                    let painted = first.elapsed();
                    if tiles {
                        let rects = stroke.take_dirty_tiles();
                        let listed = first.elapsed();
                        compositor.refresh_rects(&arena, &rects, Some(top));
                        if frame == 0 {
                            println!(
                                "  first frame: brush {:.1} ms, tiles listed {:.1} ms, screen {:.1} ms ({} rects)",
                                painted.as_secs_f64() * 1000.0,
                                (listed - painted).as_secs_f64() * 1000.0,
                                (first.elapsed() - listed).as_secs_f64() * 1000.0,
                                rects.len()
                            );
                        }
                    } else if let Some(rect) = stroke.take_dirty() {
                        compositor.refresh(&arena, rect);
                    }
                }
                println!(
                    "{} {size} stroke frame, {}: {:.2} ms",
                    if soft { "soft eraser" } else { "hard pen" },
                    if tiles {
                        "changed tiles"
                    } else {
                        "whole square"
                    },
                    start.elapsed().as_secs_f64() * 1000.0 / f64::from(frames)
                );
            }
        }
        for side in [100_u32, 500, 1500] {
            let start = Instant::now();
            let runs = 10;
            for run in 0..runs {
                let x = 500 + run * 7;
                compositor.refresh(&arena, [x, 500, x + side, 500 + side]);
            }
            println!(
                "refresh {side}x{side}: {:.2} ms",
                start.elapsed().as_secs_f64() * 1000.0 / f64::from(runs)
            );
        }
    }

    /// Frames of a stroke with each brush setting and effect: the brush,
    /// then the screen, and how many pixels the screen had to redo
    /// (`cargo test --release -p sue-picture stroke_frames -- --ignored
    /// --nocapture`; `SUE_BRUSH_SIZE` sets the size, 400 by default).
    #[test]
    #[ignore = "a benchmark"]
    fn stroke_frames() {
        use crate::brush::{Dynamics, Jitter, Stroke, Tip};
        let size = std::env::var("SUE_BRUSH_SIZE")
            .ok()
            .and_then(|size| size.parse().ok())
            .unwrap_or(400.0);
        let base = Tip {
            size,
            size_by: Dynamics::NONE,
            ..Tip::default()
        };
        let cases: Vec<(&str, Tip)> = vec![
            ("plain", base),
            (
                "opacity 50",
                Tip {
                    opacity: 0.5,
                    ..base
                },
            ),
            (
                "opacity 90",
                Tip {
                    opacity: 0.9,
                    ..base
                },
            ),
            (
                "hardness 90",
                Tip {
                    hardness: 0.9,
                    ..base
                },
            ),
            (
                "hardness 50",
                Tip {
                    hardness: 0.5,
                    ..base
                },
            ),
            (
                "hardness 0",
                Tip {
                    hardness: 0.0,
                    ..base
                },
            ),
            ("color mixing 20", Tip { mix: 0.2, ..base }),
            ("color mixing 90", Tip { mix: 0.9, ..base }),
            ("paper grain 20", Tip { grain: 0.2, ..base }),
            ("paper grain 90", Tip { grain: 0.9, ..base }),
            (
                "tip color jitter",
                Tip {
                    jitter: Jitter {
                        tip: true,
                        hue: 0.2,
                        ..Jitter::default()
                    },
                    ..base
                },
            ),
            (
                "airbrush",
                Tip {
                    build_up: true,
                    flow: 0.3,
                    hardness: 0.0,
                    ..base
                },
            ),
            (
                "eraser soft",
                Tip {
                    eraser: true,
                    hardness: 0.3,
                    ..base
                },
            ),
            (
                "size by pressure",
                Tip {
                    size_by: Dynamics::pressure(0.2),
                    ..base
                },
            ),
            (
                "everything at once",
                Tip {
                    opacity: 0.6,
                    hardness: 0.4,
                    mix: 0.5,
                    grain: 0.5,
                    ..base
                },
            ),
        ];
        for (name, tip) in cases {
            let (mut arena, picture, top) = big_picture();
            let mut compositor = Compositor::new(picture);
            compositor.sync(&arena);
            let mut stroke = Stroke::new(top, tip, [0.1, 0.2, 0.9], false);
            let frames: u32 = 120;
            let (mut brush, mut screen, mut worst, mut area) = (0.0, 0.0, 0.0_f64, 0_u64);
            for frame in 0..frames {
                let begun = Instant::now();
                for step in 0..2 {
                    let i = (frame * 2 + step) as f32;
                    let raster = doc::raster_mut(&mut arena, top).unwrap();
                    let pressure = 0.6 + 0.3 * (i * 0.07).sin();
                    stroke.to(
                        raster,
                        [900.0 + i * 3.0, 1500.0 + (i * 0.05).sin() * 200.0],
                        pressure,
                        None,
                    );
                }
                let painted = begun.elapsed().as_secs_f64();
                let rects = stroke.take_dirty_tiles();
                area += rects
                    .iter()
                    .map(|r| u64::from(r[2] - r[0]) * u64::from(r[3] - r[1]))
                    .sum::<u64>();
                compositor.refresh_rects(&arena, &rects, Some(top));
                let total = begun.elapsed().as_secs_f64();
                brush += painted;
                screen += total - painted;
                worst = worst.max(total);
            }
            let per = |seconds: f64| seconds * 1000.0 / f64::from(frames);
            println!(
                "{name:<20} brush {:>6.2} ms  screen {:>6.2} ms  worst {:>6.2} ms  {:>8} px a frame",
                per(brush),
                per(screen),
                worst * 1000.0,
                area / u64::from(frames),
            );
        }
    }

    /// A 4000 by 3000 picture of eight layers, the top one to paint on.
    fn big_picture() -> (Arena, EntityId, EntityId) {
        let mut arena = Arena::new();
        let (mut edit, picture) = doc::create(&NewPicture {
            name: "Big".to_owned(),
            width: 4000,
            height: 3000,
            dpi: 72.0,
            paper: Some(Color::srgb8(255, 255, 255)),
            timelapse: false,
        })
        .unwrap();
        edit.apply(&mut arena).unwrap();
        let mut below = doc::layers(&arena, picture).last().map(|info| info.id);
        for i in 0..6 {
            let (mut spawn, layer) =
                doc::add_raster_layer(&arena, picture, below, format!("L{i}"), None).unwrap();
            spawn.apply(&mut arena).unwrap();
            if let Some(bytes) = doc::raster_mut(&mut arena, layer).and_then(|r| r.bytes_mut()) {
                for (n, pixel) in bytes.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    if (n / 4000 + n % 4000 + i * 300) % 900 < 400 {
                        pixel.copy_from_slice(&[(i * 40) as u8, 90, 200, 180]);
                    }
                }
            }
            below = Some(layer);
        }
        (arena, picture, below.unwrap())
    }
}
