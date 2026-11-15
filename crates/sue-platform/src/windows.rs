//! Sue's windows besides the main one.
//!
//! An application asks for them by describing them ([`WindowSpec`]): a
//! key that stays the same while the window is open, a title and a size.
//! Where the system gives Sue many windows ([`WindowSupport::Many`], the
//! desktops) each becomes a window of its own that can be moved to
//! another screen, with its own input, its own scale factor and its own
//! frames: a window draws when its own input arrives, when something in it
//! moves, or when what it shows changed in another window, and never
//! because another window drew. Where the system gives Sue one window
//! ([`WindowSupport::One`]: phones, tablets and the browser) the
//! application draws the same content in a floating frame inside it.
//!
//! All windows share one egui context, so fonts, the theme, textures and
//! memory are shared; each window is an egui viewport of its own
//! ([`viewport_of`]) whose frame runs as a pass of its own
//! ([`window_pass`]). [`WindowSet`] is the bookkeeping every host keeps,
//! the desktop's and [`HeadlessWindows`], which drives several windows
//! without a screen for tests and scripted runs.

use std::collections::VecDeque;
use std::time::Duration;

use egui::{FullOutput, RawInput, Ui, ViewportBuilder, ViewportId};

use crate::time::Instant;
use crate::{MAIN_SURFACE, PlatformError, SafeAreaInsets, SurfaceId, SurfaceInfo};

/// How many windows the system gives Sue.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WindowSupport {
    /// Windows of its own, each movable to any screen (the desktops).
    Many,
    /// One window: a second window is drawn as a floating frame inside it
    /// (phones, tablets and the browser).
    #[default]
    One,
}

impl WindowSupport {
    /// What this build's system gives.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(any(
            target_os = "android",
            target_os = "ios",
            target_arch = "wasm32"
        )) {
            Self::One
        } else {
            Self::Many
        }
    }

    /// `true` when windows of their own can be opened.
    #[must_use]
    pub const fn many(self) -> bool {
        matches!(self, Self::Many)
    }
}

/// A window an application asks for besides its main one.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowSpec {
    /// Names the window while it is open: the same key in the next
    /// frame's list is the same window.
    pub key: String,
    /// Its title (the system's title bar, the taskbar, Sue's own frame).
    pub title: String,
    /// The size of its inside when it opens, in points.
    pub size: [f32; 2],
    /// Where its top left corner goes on the desktop when it opens, in
    /// the desktop's pixels (screens of different scales have no common
    /// points, only common pixels); `None` lets the system place it.
    pub at: Option<[i32; 2]>,
}

impl WindowSpec {
    /// A window named `key`, titled `title`, `size` points inside.
    #[must_use]
    pub fn new(key: impl Into<String>, title: impl Into<String>, size: [f32; 2]) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            size,
            at: None,
        }
    }

    /// Opens it with its top left corner at `at` (the desktop's pixels,
    /// as [`Screen::at`] gives them, so on another screen too).
    #[must_use]
    pub fn at(mut self, at: [i32; 2]) -> Self {
        self.at = Some(at);
        self
    }

    /// The egui viewport its frames run in.
    #[must_use]
    pub fn viewport(&self) -> ViewportId {
        viewport_of(&self.key)
    }
}

/// One screen of the desktop. Its place is in the desktop's pixels, the
/// one measure every screen shares when their scales differ.
#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    /// The name the system gives it (may be empty).
    pub name: String,
    /// Its top left corner on the desktop, in the desktop's pixels.
    pub at: [i32; 2],
    /// Its size in pixels.
    pub pixels: [u32; 2],
    /// Its pixels a point.
    pub scale: f32,
}

impl Screen {
    /// Its size in points.
    #[must_use]
    pub fn size(&self) -> [f32; 2] {
        let scale = if self.scale > 0.0 { self.scale } else { 1.0 };
        [
            self.pixels[0] as f32 / scale,
            self.pixels[1] as f32 / scale,
        ]
    }

    /// Where a window's top left corner goes to sit `inset` points in
    /// from this screen's top left corner, in the desktop's pixels (for
    /// [`WindowSpec::at`]).
    #[must_use]
    pub fn place(&self, inset: [f32; 2]) -> [i32; 2] {
        [
            self.at[0] + (inset[0] * self.scale).round() as i32,
            self.at[1] + (inset[1] * self.scale).round() as i32,
        ]
    }
}

/// The egui viewport of the window named `key`.
#[must_use]
pub fn viewport_of(key: &str) -> ViewportId {
    ViewportId::from_hash_of(("sue-window", key))
}

/// Keeps the windows in `specs` known to egui. Called inside every frame
/// of the main window: egui forgets a viewport its parent's frame did not
/// mention. Does nothing while egui draws every viewport inside the main
/// window (it has no windows of its own then).
pub fn keep(ctx: &egui::Context, specs: &[WindowSpec]) {
    if ctx.embed_viewports() {
        return;
    }
    for spec in specs {
        let builder = ViewportBuilder::default()
            .with_title(spec.title.clone())
            .with_inner_size(egui::vec2(spec.size[0], spec.size[1]));
        // Sue runs the window's frames itself (`window_pass`): egui's own
        // callback is never called.
        ctx.show_viewport_deferred(spec.viewport(), builder, |_, _| {});
    }
}

/// Runs one frame of the window named `key` on the shared context: `raw`
/// is that window's own input (its events, its size and scale), and only
/// that window's widgets see it.
pub fn window_pass(
    ctx: &egui::Context,
    key: &str,
    mut raw: RawInput,
    ui: impl FnMut(&mut Ui),
) -> FullOutput {
    let viewport = viewport_of(key);
    raw.viewport_id = viewport;
    raw.viewports.entry(viewport).or_default().parent = Some(ViewportId::ROOT);
    ctx.run_ui(raw, ui)
}

/// When egui wants viewport `id` drawn again after `output`, if ever.
fn repaint_after(output: &FullOutput, id: ViewportId, now: Instant) -> Option<Option<Instant>> {
    output.viewport_output.get(&id).map(|viewport| {
        // Anything past a day is egui's "never".
        (viewport.repaint_delay < Duration::from_secs(86_400)).then(|| now + viewport.repaint_delay)
    })
}

/// One window a host keeps, with the host's own handle `W` to it.
#[derive(Debug)]
pub struct Tracked<W> {
    /// Its surface id (never [`MAIN_SURFACE`]).
    pub surface: SurfaceId,
    /// What the application asked for.
    pub spec: WindowSpec,
    /// The host's handle: the system's window, or a headless queue.
    pub native: W,
    /// Its size and scale as last heard of.
    pub info: SurfaceInfo,
    /// When egui wants it drawn again.
    pub repaint_at: Option<Instant>,
    /// The application's revision its last frame showed (`None`: never
    /// drawn).
    pub drawn: Option<u64>,
}

/// What a host does to make its windows match the application's list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// Windows to open, in the order asked.
    pub open: Vec<WindowSpec>,
    /// Windows to close.
    pub close: Vec<SurfaceId>,
    /// Windows whose title changed.
    pub retitle: Vec<(SurfaceId, String)>,
}

impl Plan {
    /// `true` when nothing changes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.open.is_empty() && self.close.is_empty() && self.retitle.is_empty()
    }
}

/// The windows a host keeps besides the main one.
#[derive(Debug)]
pub struct WindowSet<W> {
    list: Vec<Tracked<W>>,
    next: u32,
}

impl<W> Default for WindowSet<W> {
    fn default() -> Self {
        Self {
            list: Vec::new(),
            next: MAIN_SURFACE.0 + 1,
        }
    }
}

impl<W> WindowSet<W> {
    /// No windows.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// What to open, close and retitle so the windows are `wanted`. A key
    /// asked for twice opens one window.
    #[must_use]
    pub fn plan(&self, wanted: &[WindowSpec]) -> Plan {
        let mut plan = Plan::default();
        for tracked in &self.list {
            match wanted.iter().find(|spec| spec.key == tracked.spec.key) {
                None => plan.close.push(tracked.surface),
                Some(spec) if spec.title != tracked.spec.title => {
                    plan.retitle.push((tracked.surface, spec.title.clone()));
                }
                Some(_) => {}
            }
        }
        for (index, spec) in wanted.iter().enumerate() {
            let known = self.list.iter().any(|tracked| tracked.spec.key == spec.key);
            let twice = wanted[..index].iter().any(|before| before.key == spec.key);
            if !known && !twice {
                plan.open.push(spec.clone());
            }
        }
        plan
    }

    /// Keeps a window the host opened for `spec`, `size` points inside at
    /// `scale` pixels a point. It wants a frame at once.
    pub fn insert(&mut self, spec: WindowSpec, native: W, size: [f32; 2], scale: f32) -> SurfaceId {
        let surface = SurfaceId(self.next);
        self.next += 1;
        let info = SurfaceInfo {
            id: surface,
            title: spec.title.clone(),
            width: size[0],
            height: size[1],
            scale_factor: scale,
            safe_area: SafeAreaInsets::ZERO,
        };
        self.list.push(Tracked {
            surface,
            spec,
            native,
            info,
            repaint_at: Some(Instant::now()),
            drawn: None,
        });
        surface
    }

    /// Lets go of a window; the others stay as they are.
    pub fn remove(&mut self, surface: SurfaceId) -> Option<Tracked<W>> {
        let index = self
            .list
            .iter()
            .position(|tracked| tracked.surface == surface)?;
        Some(self.list.remove(index))
    }

    /// A window by its surface.
    #[must_use]
    pub fn get(&self, surface: SurfaceId) -> Option<&Tracked<W>> {
        self.list.iter().find(|tracked| tracked.surface == surface)
    }

    /// A window by its surface, to change.
    pub fn get_mut(&mut self, surface: SurfaceId) -> Option<&mut Tracked<W>> {
        self.list
            .iter_mut()
            .find(|tracked| tracked.surface == surface)
    }

    /// A window by its key.
    #[must_use]
    pub fn by_key(&self, key: &str) -> Option<&Tracked<W>> {
        self.list.iter().find(|tracked| tracked.spec.key == key)
    }

    /// The first window `pick` says is the one.
    pub fn find_mut(&mut self, pick: impl Fn(&W) -> bool) -> Option<&mut Tracked<W>> {
        self.list.iter_mut().find(|tracked| pick(&tracked.native))
    }

    /// Every window, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &Tracked<W>> {
        self.list.iter()
    }

    /// Every window, to change.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Tracked<W>> {
        self.list.iter_mut()
    }

    /// How many windows there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// `true` with none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Takes in what a frame (of any window) gave: `own` is the window
    /// whose frame it was (`None` for the main window). That window's
    /// next frame is when egui says; another window's comes no later than
    /// before, and sooner if this frame asked for it.
    pub fn after_pass(
        &mut self,
        output: &FullOutput,
        own: Option<SurfaceId>,
        revision: u64,
        now: Instant,
    ) {
        for tracked in &mut self.list {
            let asked = repaint_after(output, tracked.spec.viewport(), now);
            if own == Some(tracked.surface) {
                tracked.repaint_at = asked.flatten();
                tracked.drawn = Some(revision);
            } else if let Some(Some(at)) = asked {
                tracked.repaint_at = Some(tracked.repaint_at.map_or(at, |before| before.min(at)));
            }
        }
    }

    /// The windows to draw now: egui asked for it, or what they show
    /// changed (`revision` is not the one they last drew).
    #[must_use]
    pub fn due(&self, now: Instant, revision: u64) -> Vec<SurfaceId> {
        self.list
            .iter()
            .filter(|tracked| {
                tracked.repaint_at.is_some_and(|at| at <= now) || tracked.drawn != Some(revision)
            })
            .map(|tracked| tracked.surface)
            .collect()
    }

    /// The soonest a window wants to draw, if any does.
    #[must_use]
    pub fn next_wake(&self) -> Option<Instant> {
        self.list
            .iter()
            .filter_map(|tracked| tracked.repaint_at)
            .min()
    }

    /// The window was resized to `size` points at `scale` pixels a point
    /// (it moved to a screen of another scale, too).
    pub fn resized(&mut self, surface: SurfaceId, size: [f32; 2], scale: f32) {
        if let Some(tracked) = self.get_mut(surface) {
            tracked.info.width = size[0];
            tracked.info.height = size[1];
            tracked.info.scale_factor = scale;
            tracked.repaint_at = Some(Instant::now());
        }
    }

    /// Takes in a new title.
    pub fn retitle(&mut self, surface: SurfaceId, title: &str) {
        if let Some(tracked) = self.get_mut(surface) {
            title.clone_into(&mut tracked.spec.title);
            title.clone_into(&mut tracked.info.title);
        }
    }
}

/// One window of [`HeadlessWindows`]: the events waiting for its next
/// frame.
#[derive(Debug, Default)]
pub struct Queue {
    events: VecDeque<egui::Event>,
}

/// Several windows on one egui context, without a screen: each has its
/// own events, size and scale, and draws only when asked. It does what the
/// desktop host does with the system's windows, for tests and scripted
/// runs.
#[derive(Debug)]
pub struct HeadlessWindows {
    ctx: egui::Context,
    main: SurfaceInfo,
    main_events: VecDeque<egui::Event>,
    windows: WindowSet<Queue>,
    time: f64,
}

impl HeadlessWindows {
    /// A main window `size` points big at `scale` pixels a point, on
    /// `ctx` (which then keeps viewports apart instead of drawing them
    /// inside the main window).
    #[must_use]
    pub fn new(ctx: &egui::Context, size: [f32; 2], scale: f32) -> Self {
        ctx.set_embed_viewports(false);
        Self {
            ctx: ctx.clone(),
            main: SurfaceInfo {
                id: MAIN_SURFACE,
                title: String::new(),
                width: size[0],
                height: size[1],
                scale_factor: scale,
                safe_area: SafeAreaInsets::ZERO,
            },
            main_events: VecDeque::new(),
            windows: WindowSet::new(),
            time: 0.0,
        }
    }

    /// The windows besides the main one.
    #[must_use]
    pub fn windows(&self) -> &WindowSet<Queue> {
        &self.windows
    }

    /// Opens and closes windows so they are `wanted`, as the desktop host
    /// does after every frame. Returns what changed.
    pub fn sync(&mut self, wanted: &[WindowSpec], scale: f32) -> Plan {
        let plan = self.windows.plan(wanted);
        for surface in &plan.close {
            self.windows.remove(*surface);
        }
        for (surface, title) in &plan.retitle {
            self.windows.retitle(*surface, title);
        }
        for spec in &plan.open {
            let size = spec.size;
            self.windows
                .insert(spec.clone(), Queue::default(), size, scale);
        }
        plan
    }

    /// The window named `key`'s surface.
    #[must_use]
    pub fn surface_of(&self, key: &str) -> Option<SurfaceId> {
        self.windows.by_key(key).map(|tracked| tracked.surface)
    }

    /// A window's size and scale.
    #[must_use]
    pub fn info(&self, surface: SurfaceId) -> Option<&SurfaceInfo> {
        if surface == MAIN_SURFACE {
            Some(&self.main)
        } else {
            self.windows.get(surface).map(|tracked| &tracked.info)
        }
    }

    /// Queues `event` for `surface`'s next frame.
    ///
    /// # Errors
    /// [`PlatformError::UnknownSurface`] for a window that is not open.
    pub fn inject(&mut self, surface: SurfaceId, event: egui::Event) -> Result<(), PlatformError> {
        if surface == MAIN_SURFACE {
            self.main_events.push_back(event);
            return Ok(());
        }
        let tracked = self
            .windows
            .get_mut(surface)
            .ok_or(PlatformError::UnknownSurface(surface))?;
        tracked.native.events.push_back(event);
        // Input wants a frame, as the system's window asks for one.
        tracked.repaint_at = Some(Instant::now());
        Ok(())
    }

    /// Resizes a window, or moves it to a screen of another scale.
    ///
    /// # Errors
    /// [`PlatformError::UnknownSurface`], or
    /// [`PlatformError::InvalidSurface`] for a size or scale that is not
    /// positive.
    pub fn resize(
        &mut self,
        surface: SurfaceId,
        size: [f32; 2],
        scale: f32,
    ) -> Result<(), PlatformError> {
        let positive = |value: f32| value.is_finite() && value > 0.0;
        if !(positive(size[0]) && positive(size[1]) && positive(scale)) {
            return Err(PlatformError::InvalidSurface(
                "size and scale must be positive and finite",
            ));
        }
        if surface == MAIN_SURFACE {
            self.main.width = size[0];
            self.main.height = size[1];
            self.main.scale_factor = scale;
            return Ok(());
        }
        if self.windows.get(surface).is_none() {
            return Err(PlatformError::UnknownSurface(surface));
        }
        self.windows.resized(surface, size, scale);
        Ok(())
    }

    /// The input of one frame of `info`'s window.
    fn raw(&mut self, info: &SurfaceInfo, events: Vec<egui::Event>) -> RawInput {
        self.time += 1.0 / 60.0;
        RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(info.width, info.height),
            )),
            events,
            time: Some(self.time),
            ..RawInput::default()
        }
    }

    /// Runs one frame of the main window. `ui` draws it; the windows in
    /// `keep_specs` stay known to egui. The output's textures are let go
    /// of (there is no screen to take them).
    pub fn main_frame(
        &mut self,
        revision: u64,
        keep_specs: &[WindowSpec],
        mut ui: impl FnMut(&mut Ui),
    ) -> FullOutput {
        let events: Vec<_> = self.main_events.drain(..).collect();
        let main = self.main.clone();
        let mut raw = self.raw(&main, events);
        raw.viewport_id = ViewportId::ROOT;
        raw.viewports.insert(
            ViewportId::ROOT,
            egui::ViewportInfo {
                native_pixels_per_point: Some(main.scale_factor),
                ..egui::ViewportInfo::default()
            },
        );
        let ctx = self.ctx.clone();
        let mut output = ctx.run_ui(raw, |root| {
            ui(root);
            keep(root.ctx(), keep_specs);
        });
        // No screen takes the textures.
        output.textures_delta.clear();
        self.windows
            .after_pass(&output, None, revision, Instant::now());
        output
    }

    /// Runs one frame of window `surface`: its own events, size and
    /// scale. `ui` draws it.
    ///
    /// # Errors
    /// [`PlatformError::UnknownSurface`] for a window that is not open.
    pub fn window_frame(
        &mut self,
        surface: SurfaceId,
        revision: u64,
        ui: impl FnMut(&mut Ui),
    ) -> Result<FullOutput, PlatformError> {
        let (info, key, events) = {
            let tracked = self
                .windows
                .get_mut(surface)
                .ok_or(PlatformError::UnknownSurface(surface))?;
            let events: Vec<_> = tracked.native.events.drain(..).collect();
            (tracked.info.clone(), tracked.spec.key.clone(), events)
        };
        let mut raw = self.raw(&info, events);
        raw.viewports.insert(
            viewport_of(&key),
            egui::ViewportInfo {
                native_pixels_per_point: Some(info.scale_factor),
                ..egui::ViewportInfo::default()
            },
        );
        let mut output = window_pass(&self.ctx, &key, raw, ui);
        output.textures_delta.clear();
        self.windows
            .after_pass(&output, Some(surface), revision, Instant::now());
        Ok(output)
    }

    /// The windows that would draw now, as the desktop host decides.
    #[must_use]
    pub fn due(&self, revision: u64) -> Vec<SurfaceId> {
        self.windows.due(Instant::now(), revision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click(at: egui::Pos2) -> [egui::Event; 3] {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        [egui::Event::PointerMoved(at), button(true), button(false)]
    }

    fn spec(key: &str) -> WindowSpec {
        WindowSpec::new(key, key.to_uppercase(), [400.0, 300.0])
    }

    #[test]
    fn the_plan_opens_closes_and_retitles_by_key() {
        let mut set: WindowSet<()> = WindowSet::new();
        let plan = set.plan(&[spec("a"), spec("b"), spec("a")]);
        assert_eq!(plan.open.len(), 2, "a key asked for twice opens once");
        assert!(plan.close.is_empty());
        let a = set.insert(spec("a"), (), [400.0, 300.0], 1.0);
        let b = set.insert(spec("b"), (), [400.0, 300.0], 1.0);
        assert_ne!(a, MAIN_SURFACE);
        assert_ne!(a, b);
        assert!(set.plan(&[spec("a"), spec("b")]).is_empty());
        let mut renamed = spec("b");
        renamed.title = "Other".to_owned();
        let plan = set.plan(&[renamed]);
        assert_eq!(plan.close, vec![a]);
        assert_eq!(plan.retitle, vec![(b, "Other".to_owned())]);
        assert!(plan.open.is_empty());
        set.remove(a);
        assert!(set.get(a).is_none());
        assert_eq!(
            set.get(b).map(|tracked| tracked.spec.key.as_str()),
            Some("b")
        );
        // Surface ids are never given twice.
        let c = set.insert(spec("c"), (), [10.0, 10.0], 1.0);
        assert!(c.0 > b.0);
    }

    #[test]
    fn a_place_on_another_screen_is_in_that_screens_pixels() {
        // A 4K screen at 150 percent with a 100 percent screen to its
        // right: in points the second would start at 2560, inside the
        // first's 3840 pixels, so places are named in pixels.
        let first = Screen {
            name: "first".to_owned(),
            at: [0, 0],
            pixels: [3840, 2160],
            scale: 1.5,
        };
        let second = Screen {
            name: "second".to_owned(),
            at: [3840, 0],
            pixels: [1920, 1080],
            scale: 1.0,
        };
        assert_eq!(first.size(), [2560.0, 1440.0]);
        assert_eq!(second.size(), [1920.0, 1080.0]);
        assert_eq!(first.place([40.0, 40.0]), [60, 60]);
        let at = second.place([40.0, 40.0]);
        assert_eq!(at, [3880, 40]);
        assert!(at[0] >= second.at[0], "on the second screen, not the first");
        let spec = spec("a").at(at);
        assert_eq!(spec.at, Some([3880, 40]));
        // A screen that reports no scale is read at one pixel a point.
        let odd = Screen { scale: 0.0, ..second };
        assert_eq!(odd.size(), [1920.0, 1080.0]);
    }

    #[test]
    fn this_build_knows_how_many_windows_it_has() {
        let support = WindowSupport::current();
        assert_eq!(
            support.many(),
            !cfg!(any(
                target_os = "android",
                target_os = "ios",
                target_arch = "wasm32"
            ))
        );
        assert_eq!(WindowSupport::default(), WindowSupport::One);
    }

    /// Two windows on one context, each with a button; returns how often
    /// each was clicked after `events` went to `to`.
    fn two_buttons(to: SurfaceId, second_is: SurfaceId, host: &mut HeadlessWindows) -> (u32, u32) {
        let mut clicks = (0, 0);
        let at = egui::pos2(16.0, 8.0);
        for event in click(at) {
            host.inject(to, event).unwrap();
        }
        let specs = [spec("second")];
        // Three frames each, as a click takes.
        for _ in 0..3 {
            host.main_frame(0, &specs, |ui| {
                if ui.button("main").clicked() {
                    clicks.0 += 1;
                }
            });
            host.window_frame(second_is, 0, |ui| {
                if ui.button("second").clicked() {
                    clicks.1 += 1;
                }
            })
            .unwrap();
        }
        clicks
    }

    #[test]
    fn two_windows_route_events_apart() {
        let ctx = egui::Context::default();
        let mut host = HeadlessWindows::new(&ctx, [800.0, 600.0], 1.0);
        let plan = host.sync(&[spec("second")], 1.0);
        assert_eq!(plan.open.len(), 1);
        let second = host.surface_of("second").unwrap();
        assert_ne!(second, MAIN_SURFACE);
        // Nothing clicked yet: a frame each to lay the buttons out.
        assert_eq!(two_buttons(MAIN_SURFACE, second, &mut host), (0, 0));
        // A click on the second window presses its button only, though
        // the main window's button sits at the same place.
        let (main, other) = two_buttons(second, second, &mut host);
        assert_eq!((main, other), (0, 1));
        // And the other way round.
        let (main, other) = two_buttons(MAIN_SURFACE, second, &mut host);
        assert_eq!((main, other), (1, 0));
    }

    #[test]
    fn each_window_draws_at_its_own_scale() {
        let ctx = egui::Context::default();
        let mut host = HeadlessWindows::new(&ctx, [800.0, 600.0], 1.0);
        host.sync(&[spec("second")], 2.0);
        let second = host.surface_of("second").unwrap();
        let specs = [spec("second")];
        let main = host.main_frame(0, &specs, |ui| {
            ui.label("main");
        });
        let other = host
            .window_frame(second, 0, |ui| {
                ui.label("second");
            })
            .unwrap();
        assert!((main.pixels_per_point - 1.0).abs() < 1e-6);
        assert!((other.pixels_per_point - 2.0).abs() < 1e-6);
        // Moved to a screen of another scale.
        host.resize(second, [500.0, 400.0], 1.5).unwrap();
        let other = host
            .window_frame(second, 0, |ui| {
                assert!((ui.max_rect().width() - 500.0).abs() < 1.0);
            })
            .unwrap();
        assert!((other.pixels_per_point - 1.5).abs() < 1e-6);
        assert!(host.resize(second, [0.0, 10.0], 1.0).is_err());
    }

    #[test]
    fn closing_the_second_window_leaves_the_first() {
        let ctx = egui::Context::default();
        let mut host = HeadlessWindows::new(&ctx, [800.0, 600.0], 1.0);
        host.sync(&[spec("second"), spec("third")], 1.0);
        let second = host.surface_of("second").unwrap();
        let third = host.surface_of("third").unwrap();
        let plan = host.sync(&[spec("third")], 1.0);
        assert_eq!(plan.close, vec![second]);
        assert!(host.info(second).is_none());
        assert_eq!(
            host.inject(second, egui::Event::PointerGone),
            Err(PlatformError::UnknownSurface(second))
        );
        // The main window and the third still draw and take input.
        assert!(host.info(MAIN_SURFACE).is_some());
        host.inject(third, egui::Event::PointerGone).unwrap();
        let mut drew = 0;
        host.main_frame(0, &[spec("third")], |_| drew += 1);
        host.window_frame(third, 0, |_| drew += 1).unwrap();
        assert_eq!(drew, 2);
        assert!(host.window_frame(second, 0, |_| {}).is_err());
    }

    #[test]
    fn a_window_draws_for_its_own_reasons_only() {
        let ctx = egui::Context::default();
        let mut host = HeadlessWindows::new(&ctx, [800.0, 600.0], 1.0);
        host.sync(&[spec("second")], 1.0);
        let second = host.surface_of("second").unwrap();
        let specs = [spec("second")];
        // Opened: it wants its first frame.
        assert_eq!(host.due(0), vec![second]);
        host.window_frame(second, 0, |ui| {
            ui.label("still");
        })
        .unwrap();
        // Settle egui's first frames.
        for _ in 0..3 {
            host.main_frame(0, &specs, |ui| {
                ui.label("main");
            });
            host.window_frame(second, 0, |ui| {
                ui.label("still");
            })
            .unwrap();
        }
        assert!(host.due(0).is_empty(), "nothing moves: it sleeps");
        // The main window draws again: the second does not.
        host.main_frame(0, &specs, |ui| {
            ui.label("main");
        });
        assert!(
            host.due(0).is_empty(),
            "another window's frame wakes nothing"
        );
        // What it shows changed in another window: it draws.
        assert_eq!(host.due(1), vec![second]);
        host.window_frame(second, 1, |ui| {
            ui.label("still");
        })
        .unwrap();
        assert!(host.due(1).is_empty());
        // The main window's frame asks for it.
        host.main_frame(1, &specs, |ui| {
            ui.ctx().request_repaint_of(viewport_of("second"));
        });
        assert_eq!(host.due(1), vec![second]);
        // Its own input does too.
        host.window_frame(second, 1, |_| {}).unwrap();
        host.window_frame(second, 1, |_| {}).unwrap();
        host.inject(second, egui::Event::PointerGone).unwrap();
        assert_eq!(host.due(1), vec![second]);
    }

    #[test]
    fn the_main_frame_keeps_the_windows_known_to_egui() {
        let ctx = egui::Context::default();
        let mut host = HeadlessWindows::new(&ctx, [800.0, 600.0], 1.0);
        host.sync(&[spec("second")], 1.0);
        let second = host.surface_of("second").unwrap();
        host.window_frame(second, 0, |_| {}).unwrap();
        let output = host.main_frame(0, &[spec("second")], |_| {});
        assert!(output.viewport_output.contains_key(&viewport_of("second")));
        // Left out of the main frame: egui forgets it.
        let output = host.main_frame(0, &[], |_| {});
        assert!(!output.viewport_output.contains_key(&viewport_of("second")));
    }
}
