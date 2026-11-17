//! The Hand Scanner (Clip Studio Paint 5.0's): hold a hand up to the
//! camera, and the 3D hand on the canvas (or one hand of a drawing
//! figure) takes its pose, live. OK keeps the pose as one undo step;
//! Cancel puts the figure back as it was.
//!
//! The dialog shows the camera's picture with the hand it found boxed
//! and its skeleton drawn over it, a camera chooser, Pause Capture, which
//! hand of a figure to pose, the hint that the palm should face the
//! camera, and one line on privacy: the camera runs only while this
//! dialog is open, and its pictures stay in this computer's memory, never
//! saved and never sent anywhere.
//!
//! The parts, each from where Sue keeps such things:
//!
//! - the camera: `sue_platform::camera` (a thread of its own);
//! - finding the hand: `sue_ai::hand` (MediaPipe's models through tract),
//!   run on another thread here ([`Tracking`]), never on the interface's;
//! - the models: the optional package Hand tracking. When it is not
//!   installed the dialog shows Sue's package card (what is needed, its
//!   size, Install with progress and Cancel) and starts once it is in;
//! - from points to joint turns: `sue_picture::hand_scan`;
//! - the pose: `figure::set_pose`, shown live without touching the
//!   history, then applied once on OK.
//!
//! Tests and previews run it without hardware or models: [`inject`]
//! hands the next Hand Scanner a made-up camera and tracker ([`Parts`]).

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use egui::{Color32, Rect, Stroke, Ui, pos2, vec2};
use sue_ai::hand::{HandFrame, HandObservation, HandTracker, OnnxHandTracker};
use sue_core::{EntityId, ReversibleTransaction};
use sue_picture::hand_scan::{self, Side, Smoother};
use sue_platform::camera::{
    self, CameraError, CameraFrame, CameraInfo, CameraOpener, CameraStream, FakeCamera,
};
use sue_platform::packages::{HAND_TRACKING, PackageCenter, PackageState};
use sue_wm::ui::{self, Space, TextRole, Tip};
use sue_wm::{AppletContext, Theme};

use super::Outcome;
use crate::figure::{self, Body, Kind, Pose};
use crate::studio::SuePaint;

/// Makes the hand tracker, on the tracking thread.
pub type TrackerMaker = Arc<dyn Fn() -> Result<Box<dyn HandTracker>, String> + Send + Sync>;
/// Makes what opens the camera with the given id (the first one for
/// `None`).
pub type CameraMaker = Arc<dyn Fn(Option<&str>) -> CameraOpener + Send + Sync>;

/// What a Hand Scanner runs on, in place of the real camera and models:
/// for tests, previews and snapshots.
#[derive(Clone)]
pub struct Parts {
    /// The cameras the chooser lists.
    pub cameras: Vec<CameraInfo>,
    /// What opens one of them.
    pub camera: CameraMaker,
    /// What finds the hand. `None`: the real models, from the Hand
    /// tracking package.
    pub tracker: Option<TrackerMaker>,
    /// Whether the Hand tracking package must be installed first, as it
    /// must with the real models.
    pub needs_package: bool,
}

impl std::fmt::Debug for Parts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parts")
            .field("cameras", &self.cameras)
            .field("needs_package", &self.needs_package)
            .finish_non_exhaustive()
    }
}

thread_local! {
    /// The parts the next Hand Scanner opened on this thread runs on.
    static INJECTED: RefCell<Option<Parts>> = const { RefCell::new(None) };
}

/// Makes the next Hand Scanner opened on this thread run on `parts`
/// instead of the real camera and models.
pub fn inject(parts: Parts) {
    INJECTED.with(|slot| *slot.borrow_mut() = Some(parts));
}

/// The size of the made-up camera's pictures.
const FAKE_SIZE: (u32, u32) = (640, 480);

/// What a camera sees of the hand figure posed as `pose`, held up to it
/// palm first, in a picture of `width` by `height` pixels: the
/// observation a hand tracker would give. `right` makes it a right hand
/// (the figure's mirror image).
#[must_use]
pub fn observation_of(pose: &Pose, right: bool, width: u32, height: u32) -> HandObservation {
    let side = if right { -1.0 } else { 1.0 };
    let points = hand_scan::points_of(pose);
    // The hand figure is about 0.2 tall: it fills two thirds of the
    // picture's height, its wrist a little under the middle.
    let scale = height as f32 * 0.66 / 0.2;
    let middle = [width as f32 / 2.0, height as f32 * 0.5 + 0.09 * scale];
    let mut seen = HandObservation {
        points: [[0.0; 3]; hand_scan::POINTS],
        world: [[0.0; 3]; hand_scan::POINTS],
        right,
        confidence: 0.97,
        bounds: [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
    };
    for (at, [x, y, z]) in points.into_iter().enumerate() {
        let (px, py) = (middle[0] + x * side * scale, middle[1] - y * scale);
        seen.points[at] = [px, py, -z * scale];
        seen.world[at] = [x * side, -y, -z];
        seen.bounds = [
            seen.bounds[0].min(px),
            seen.bounds[1].min(py),
            seen.bounds[2].max(px),
            seen.bounds[3].max(py),
        ];
    }
    seen
}

/// The bones of a hand, as pairs of its 21 points.
const BONES: [(usize, usize); 21] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 4),
    (0, 5),
    (5, 6),
    (6, 7),
    (7, 8),
    (5, 9),
    (9, 10),
    (10, 11),
    (11, 12),
    (9, 13),
    (13, 14),
    (14, 15),
    (15, 16),
    (13, 17),
    (17, 18),
    (18, 19),
    (19, 20),
    (0, 17),
];

/// Paints a made-up camera picture: a soft backdrop and, when there is
/// one, the hand of `seen` as rounded strokes. Its number `n` is kept in
/// the first bytes, where [`ScriptedTracker`] reads it.
fn paint_fake(n: u64, seen: Option<&HandObservation>, rgb: &mut [u8]) {
    let (width, height) = (FAKE_SIZE.0 as usize, FAKE_SIZE.1 as usize);
    for y in 0..height {
        let shade = 58.0 + 40.0 * (y as f32 / height as f32);
        for x in 0..width {
            let light = shade + 14.0 * (x as f32 / width as f32);
            let at = (y * width + x) * 3;
            rgb[at] = light as u8;
            rgb[at + 1] = (light * 1.04) as u8;
            rgb[at + 2] = (light * 1.12) as u8;
        }
    }
    if let Some(seen) = seen {
        let skin = [226.0, 186.0, 160.0];
        for (index, (from, to)) in BONES.into_iter().enumerate() {
            let (a, b) = (seen.points[from], seen.points[to]);
            // The palm's bones are drawn wide so they close into a palm.
            let radius = if matches!(index, 4 | 8 | 12 | 16 | 20) {
                24.0
            } else {
                11.0
            };
            let low = [a[0].min(b[0]) - radius, a[1].min(b[1]) - radius];
            let high = [a[0].max(b[0]) + radius, a[1].max(b[1]) + radius];
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let length = (dx * dx + dy * dy).max(1e-6);
            for y in (low[1].max(0.0) as usize)..(high[1].min(height as f32 - 1.0) as usize + 1) {
                for x in (low[0].max(0.0) as usize)..(high[0].min(width as f32 - 1.0) as usize + 1)
                {
                    let (px, py) = (x as f32 + 0.5 - a[0], y as f32 + 0.5 - a[1]);
                    let along = ((px * dx + py * dy) / length).clamp(0.0, 1.0);
                    let (ox, oy) = (px - along * dx, py - along * dy);
                    let cover = (radius + 0.5 - (ox * ox + oy * oy).sqrt()).clamp(0.0, 1.0);
                    if cover > 0.0 {
                        let at = (y * width + x) * 3;
                        for channel in 0..3 {
                            let under = f32::from(rgb[at + channel]);
                            rgb[at + channel] = (under + (skin[channel] - under) * cover) as u8;
                        }
                    }
                }
            }
        }
    }
    rgb[..8].copy_from_slice(&n.to_le_bytes());
}

/// A hand tracker that needs no model: it reads the picture's number
/// from its first bytes (a made-up camera of [`Parts::scripted`] wrote
/// it) and answers with what the script says the camera saw then.
struct ScriptedTracker {
    script: Script,
}

/// What a made-up camera sees in its picture number `n` (from 0): a
/// hand figure's pose and whether it is a right hand, or no hand.
pub type Script = Arc<dyn Fn(u64) -> Option<(Pose, bool)> + Send + Sync>;

impl HandTracker for ScriptedTracker {
    fn track(&mut self, frame: &HandFrame<'_>) -> Result<Option<HandObservation>, sue_ai::AiError> {
        let Some(number) = frame.rgb.first_chunk::<8>() else {
            return Ok(None);
        };
        Ok(
            (self.script)(u64::from_le_bytes(*number)).map(|(pose, right)| {
                observation_of(&pose, right, frame.width as u32, frame.height as u32)
            }),
        )
    }
}

impl Parts {
    /// A made-up camera and tracker: picture `n` shows the hand `script`
    /// gives for it, one picture every `pace`, and the tracker finds
    /// exactly that hand. No package is needed.
    #[must_use]
    pub fn scripted(pace: Duration, script: Script) -> Self {
        let seen = Arc::clone(&script);
        let found = Arc::clone(&script);
        Self {
            cameras: vec![CameraInfo {
                name: "Demo camera".to_owned(),
                id: "demo".to_owned(),
            }],
            camera: Arc::new(move |_| {
                let seen = Arc::clone(&seen);
                FakeCamera::new(FAKE_SIZE.0, FAKE_SIZE.1, pace, move |n, rgb| {
                    let hand = seen(n).map(|(pose, right)| {
                        observation_of(&pose, right, FAKE_SIZE.0, FAKE_SIZE.1)
                    });
                    paint_fake(n, hand.as_ref(), rgb);
                })
                .opener()
            }),
            tracker: Some(Arc::new(move || {
                Ok(Box::new(ScriptedTracker {
                    script: Arc::clone(&found),
                }) as Box<dyn HandTracker>)
            })),
            needs_package: false,
        }
    }

    /// The real camera and the real models.
    fn real() -> Self {
        Self {
            cameras: Vec::new(),
            camera: Arc::new(|id| {
                let id = id.map(str::to_owned);
                // The stream's own opener: made here so the fake and the
                // real camera start the same way.
                Box::new(move || {
                    let stream_id = id.clone();
                    camera_source(stream_id.as_deref())
                })
            }),
            tracker: None,
            needs_package: true,
        }
    }
}

/// Opens the system's camera `id` as a source, on the calling (camera)
/// thread.
fn camera_source(id: Option<&str>) -> Result<Box<dyn camera::CameraSource>, CameraError> {
    camera::open_source(id, CameraStream::MODEST)
}

/// What the tracking thread found in one picture.
struct Tracked {
    frame: Arc<CameraFrame>,
    hand: Option<HandObservation>,
    /// How long finding it took.
    took: Duration,
}

/// What the tracking thread and the dialog share.
struct TrackShared {
    /// The newest picture, waiting to be looked at.
    input: Mutex<Option<Arc<CameraFrame>>>,
    wake: Condvar,
    output: Mutex<Option<Tracked>>,
    /// Why there is no tracker, if making it failed.
    failed: Mutex<Option<String>>,
    stop: AtomicBool,
}

/// The thread that finds the hand in the camera's pictures, so the
/// interface never waits for a model. It looks at the newest picture
/// only: one that arrives while it is busy replaces the one waiting.
struct Tracking {
    shared: Arc<TrackShared>,
}

impl Tracking {
    fn start(make: TrackerMaker, wake: impl Fn() + Send + 'static) -> Self {
        let shared = Arc::new(TrackShared {
            input: Mutex::new(None),
            wake: Condvar::new(),
            output: Mutex::new(None),
            failed: Mutex::new(None),
            stop: AtomicBool::new(false),
        });
        let thread = Arc::clone(&shared);
        let run = move || {
            let mut tracker = match make() {
                Ok(tracker) => tracker,
                Err(error) => {
                    if let Ok(mut slot) = thread.failed.lock() {
                        *slot = Some(error);
                    }
                    wake();
                    return;
                }
            };
            while !thread.stop.load(Ordering::Relaxed) {
                let frame = {
                    let Ok(mut input) = thread.input.lock() else {
                        return;
                    };
                    if input.is_none() {
                        let Ok((waited, _)) =
                            thread.wake.wait_timeout(input, Duration::from_millis(50))
                        else {
                            return;
                        };
                        input = waited;
                    }
                    input.take()
                };
                let Some(frame) = frame else { continue };
                let started = Instant::now();
                let hand = tracker
                    .track(&HandFrame {
                        width: frame.width as usize,
                        height: frame.height as usize,
                        rgb: &frame.rgb,
                    })
                    .unwrap_or(None);
                if let Ok(mut output) = thread.output.lock() {
                    *output = Some(Tracked {
                        frame,
                        hand,
                        took: started.elapsed(),
                    });
                }
                wake();
            }
        };
        if let Err(error) = std::thread::Builder::new()
            .name("sue-hand-tracking".to_owned())
            .spawn(run)
            && let Ok(mut slot) = shared.failed.lock()
        {
            *slot = Some(error.to_string());
        }
        Self { shared }
    }

    /// Hands the thread a picture to look at.
    fn look_at(&self, frame: Arc<CameraFrame>) {
        if let Ok(mut input) = self.shared.input.lock() {
            *input = Some(frame);
            self.shared.wake.notify_one();
        }
    }

    fn take(&self) -> Option<Tracked> {
        self.shared.output.lock().ok()?.take()
    }

    fn failed(&self) -> Option<String> {
        self.shared.failed.lock().ok()?.clone()
    }
}

impl Drop for Tracking {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.wake.notify_one();
    }
}

/// Which hand the camera is taken to show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Seen {
    /// What the tracker says.
    #[default]
    Automatic,
    Left,
    Right,
}

/// An open Hand Scanner: what it poses, how the figure was, and its
/// camera and tracking while they run.
pub(crate) struct HandScan {
    /// The 3D layer whose figure it poses.
    layer: EntityId,
    kind: Kind,
    /// Which hand of a body.
    side: Side,
    /// The figure's pose when the dialog opened: Cancel's, and what the
    /// scanned fingers are laid onto.
    before: Pose,
    /// The pose the canvas shows now.
    shown: Pose,
    parts: Parts,
    center: Option<Arc<PackageCenter>>,
    /// The cameras to choose from, and the one chosen.
    cameras: Vec<CameraInfo>,
    chosen: usize,
    stream: Option<CameraStream>,
    tracking: Option<Tracking>,
    /// The number of the last picture handed to the tracker.
    pumped: u64,
    /// The last picture looked at, as a texture, with its size.
    picture: Option<(egui::TextureHandle, [f32; 2])>,
    /// The hand in that picture, if one was found.
    hand: Option<HandObservation>,
    /// When a hand was last found, for the smoother's clock.
    last_found: Option<Instant>,
    smoother: Smoother,
    /// When the canvas was last drawn with a scanned pose.
    drawn: Option<Instant>,
    paused: bool,
    /// The whole hand turns as the camera sees it (a hand figure).
    turn_hand: bool,
    seen: Seen,
    /// How long the tracker took for a picture, lately, in seconds.
    took: f32,
    /// How many pictures showed a hand, and how many were looked at.
    found: (u64, u64),
}

impl std::fmt::Debug for HandScan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandScan")
            .field("layer", &self.layer)
            .field("kind", &self.kind)
            .field("paused", &self.paused)
            .finish_non_exhaustive()
    }
}

/// How far two poses are apart: the largest difference of any turn, in
/// degrees.
fn apart(a: &Pose, b: &Pose) -> f32 {
    a.0.iter()
        .zip(&b.0)
        .flat_map(|(a, b)| (0..3).map(move |axis| (a[axis] - b[axis]).abs()))
        .fold(0.0, f32::max)
}

/// The canvas is drawn with a new pose at most this often...
const DRAW_EVERY: Duration = Duration::from_millis(50);
/// ...and only when a joint moved at least this many degrees.
const DRAW_FROM: f32 = 0.6;
/// A hand not seen for this long is a new hand to the smoother.
const FORGET_AFTER: Duration = Duration::from_millis(600);

impl HandScan {
    /// What the status line under the picture says.
    fn status(&self) -> String {
        if self.paused {
            return tr!("hand-scanner-paused");
        }
        match &self.hand {
            Some(hand) => {
                let right = match self.seen {
                    Seen::Automatic => hand.right,
                    Seen::Left => false,
                    Seen::Right => true,
                };
                let rate = if self.took > 0.0 {
                    (1.0 / self.took).min(60.0)
                } else {
                    0.0
                };
                if right {
                    tr!("hand-scanner-following-right", rate = format!("{rate:.0}"))
                } else {
                    tr!("hand-scanner-following-left", rate = format!("{rate:.0}"))
                }
            }
            None if self.picture.is_some() => tr!("hand-scanner-looking"),
            None => tr!("hand-scanner-starting"),
        }
    }

    /// Starts the camera chosen and, if it is not running, the tracker.
    fn start(&mut self, ctx: &egui::Context) {
        let id = self
            .cameras
            .get(self.chosen)
            .map(|camera| camera.id.clone());
        let repaint = ctx.clone();
        self.stream = Some(CameraStream::with(
            (self.parts.camera)(id.as_deref()),
            move || repaint.request_repaint(),
        ));
        self.stream
            .as_ref()
            .inspect(|stream| stream.pause(self.paused));
        self.pumped = 0;
        if self.tracking.is_none() {
            let make: TrackerMaker = match (&self.parts.tracker, &self.center) {
                (Some(make), _) => Arc::clone(make),
                (None, center) => {
                    let folder = center
                        .as_ref()
                        .and_then(|center| center.folder(HAND_TRACKING));
                    Arc::new(move || {
                        let folder = folder
                            .clone()
                            .ok_or_else(|| tr!("hand-scanner-no-models"))?;
                        OnnxHandTracker::from_folder(&folder)
                            .map(|tracker| Box::new(tracker) as Box<dyn HandTracker>)
                            .map_err(|error| error.to_string())
                    })
                }
            };
            let repaint = ctx.clone();
            self.tracking = Some(Tracking::start(make, move || repaint.request_repaint()));
        }
    }

    /// Stops the camera and the tracker.
    fn stop(&mut self) {
        self.stream = None;
        self.tracking = None;
    }
}

/// Why the camera is not sending pictures, in words for people.
fn camera_words(error: &CameraError) -> String {
    match error {
        CameraError::Unsupported => tr!("hand-scanner-camera-unsupported"),
        CameraError::NoCamera => tr!("hand-scanner-camera-none"),
        CameraError::Busy => tr!("hand-scanner-camera-busy"),
        CameraError::Denied => tr!("hand-scanner-camera-denied"),
        CameraError::Gone => tr!("hand-scanner-camera-gone"),
        CameraError::Failed(why) => tr!("hand-scanner-camera-failed", why = why.as_str()),
    }
}

fn words(ui: &mut Ui, text: &str, role: TextRole, color: Color32, theme: &Theme) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .font(theme.text_font(role))
                .color(color),
        )
        .wrap(),
    );
}

impl SuePaint {
    /// Opens the Hand Scanner for the current 3D layer's hand, or for a
    /// hand of its drawing figure.
    pub(crate) fn open_hand_scanner(&mut self, cx: &mut AppletContext<'_>) {
        let Some(layer) = self.layer() else {
            return;
        };
        let found = figure::of_layer(&cx.world.arena, layer)
            .and_then(|model| cx.world.arena.get_by_id(model).ok())
            .map(|model| (Body::of(model), Pose::of(model)));
        let Some((body, before)) = found.filter(|(body, _)| body.kind != Kind::Head) else {
            cx.notify(tr!("hand-scanner-needs-hands"));
            return;
        };
        if body.kind == Kind::Body && hand_scan::body_fingers(&body.joints(), Side::Left).is_none()
        {
            cx.notify(tr!("hand-scanner-no-fingers"));
            return;
        }
        let parts = INJECTED
            .with(|slot| slot.borrow_mut().take())
            .unwrap_or_else(Parts::real);
        let center = cx
            .catalog
            .folders
            .get("packages")
            .map(|folder| PackageCenter::at(folder));
        let cameras = if parts.cameras.is_empty() && parts.needs_package {
            camera::cameras().unwrap_or_default()
        } else {
            parts.cameras.clone()
        };
        self.hand_scan = Some(Box::new(HandScan {
            layer,
            kind: body.kind,
            side: Side::Right,
            shown: before.clone(),
            before,
            parts,
            center,
            cameras,
            chosen: 0,
            stream: None,
            tracking: None,
            pumped: 0,
            picture: None,
            hand: None,
            last_found: None,
            smoother: Smoother::default(),
            drawn: None,
            paused: false,
            turn_hand: true,
            seen: Seen::Automatic,
            took: 0.0,
            found: (0, 0),
        }));
        self.dialog = Some(super::Dialog::HandScanner);
    }

    /// Shows `pose` on the canvas without a step in the history: the
    /// live pose while the dialog is open.
    fn show_scanned(cx: &mut AppletContext<'_>, scan: &mut HandScan, pose: Pose) {
        if let Some(mut edit) = figure::set_pose(
            &cx.world.arena,
            scan.layer,
            &pose,
            &tr!("hand-scanner-title"),
        ) && edit.apply(&mut cx.world.arena).is_ok()
        {
            scan.shown = pose;
        }
    }

    /// Takes in what the camera and the tracker have since the last
    /// frame: the newest picture goes to the tracker, and a hand it
    /// found poses the figure.
    fn hand_scanner_pump(cx: &mut AppletContext<'_>, scan: &mut HandScan, ctx: &egui::Context) {
        if let (Some(stream), Some(tracking)) = (&scan.stream, &scan.tracking)
            && let Some(frame) = stream.latest()
            && frame.number != scan.pumped
            && !scan.paused
        {
            scan.pumped = frame.number;
            tracking.look_at(frame);
        }
        let Some(tracked) = scan.tracking.as_ref().and_then(Tracking::take) else {
            return;
        };
        // The picture just looked at is the one shown, so the box and the
        // hand under it never part.
        let size = [tracked.frame.width as usize, tracked.frame.height as usize];
        if tracked.frame.rgb.len() == size[0] * size[1] * 3 {
            let image = egui::ColorImage::from_rgb(size, &tracked.frame.rgb);
            match &mut scan.picture {
                Some((texture, shape)) => {
                    texture.set(image, egui::TextureOptions::LINEAR);
                    *shape = [size[0] as f32, size[1] as f32];
                }
                None => {
                    let texture = ctx.load_texture(
                        "sue-paint-hand-scanner",
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    scan.picture = Some((texture, [size[0] as f32, size[1] as f32]));
                }
            }
        }
        let seconds = tracked.took.as_secs_f32();
        scan.took = if scan.took > 0.0 {
            scan.took * 0.8 + seconds * 0.2
        } else {
            seconds
        };
        scan.found.1 += 1;
        scan.hand = tracked.hand;
        let Some(hand) = &scan.hand else {
            return;
        };
        scan.found.0 += 1;
        let now = Instant::now();
        let since = scan.last_found.map(|last| now.duration_since(last));
        if since.is_none_or(|since| since > FORGET_AFTER) {
            scan.smoother.reset();
        }
        scan.last_found = Some(now);
        let right = match scan.seen {
            Seen::Automatic => hand.right,
            Seen::Left => false,
            Seen::Right => true,
        };
        let points = hand_scan::to_model(&hand.world, right);
        let points = scan.smoother.step(
            &points,
            since.map_or(1.0 / 30.0, |since| since.as_secs_f32()),
        );
        let Some(angles) = hand_scan::solve(&points) else {
            return;
        };
        let mut pose = scan.before.clone();
        match scan.kind {
            Kind::Hand => hand_scan::pose_hand(&mut pose, &angles, scan.turn_hand),
            _ => {
                let joints = figure::of_layer(&cx.world.arena, scan.layer)
                    .and_then(|model| cx.world.arena.get_by_id(model).ok())
                    .map(|model| Body::of(model).joints())
                    .unwrap_or_default();
                hand_scan::pose_body(&mut pose, &joints, scan.side, &angles);
            }
        }
        let due = scan
            .drawn
            .is_none_or(|drawn| now.duration_since(drawn) >= DRAW_EVERY);
        if due && apart(&pose, &scan.shown) >= DRAW_FROM {
            scan.drawn = Some(now);
            Self::show_scanned(cx, scan, pose);
        }
    }

    /// The Hand Scanner's body.
    pub(super) fn hand_scanner_ui(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> Outcome {
        let Some(mut scan) = self.hand_scan.take() else {
            return Outcome::Cancel;
        };
        let outcome = Self::hand_scanner_body(ui, cx, theme, &mut scan);
        self.hand_scan = Some(scan);
        outcome
    }

    fn hand_scanner_body(
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        scan: &mut HandScan,
    ) -> Outcome {
        // The models first: without them there is nothing to run, and
        // the camera stays off.
        let ready = !scan.parts.needs_package
            || scan
                .center
                .as_ref()
                .is_some_and(|center| center.state(HAND_TRACKING) == PackageState::Installed);
        if !ready {
            scan.stop();
            words(
                ui,
                &tr!("hand-scanner-needs-package"),
                TextRole::Body,
                theme.text,
                theme,
            );
            ui.add_space(theme.space(Space::S3));
            match scan
                .center
                .as_ref()
                .and_then(|center| Some((center, center.package(HAND_TRACKING)?)))
            {
                Some((center, package)) => {
                    sue_wm::packages::package_card(ui, theme, center, &package, false);
                }
                None => words(
                    ui,
                    &tr!("hand-scanner-no-models"),
                    TextRole::Body,
                    theme.danger(),
                    theme,
                ),
            }
            ui.add_space(theme.space(Space::S3));
            words(
                ui,
                &tr!("hand-scanner-privacy"),
                TextRole::Caption,
                theme.text_muted(),
                theme,
            );
            return Outcome::Open;
        }
        if scan.stream.is_none() {
            scan.start(ui.ctx());
        }
        Self::hand_scanner_pump(cx, scan, ui.ctx());

        // The camera's picture, as a mirror shows it, with the hand
        // boxed and its bones drawn.
        let width = ui.available_width();
        let room = vec2(width, (width * 0.6).min(theme.space(Space::S9) * 7.0));
        let (area, _) = ui.allocate_exact_size(room, egui::Sense::hover());
        let painter = ui.painter_at(area);
        let corner = theme.corner(ui::Radius::Md);
        painter.rect_filled(area, corner, theme.surface(0));
        let camera_error = scan.stream.as_ref().and_then(CameraStream::error);
        let tracker_error = scan.tracking.as_ref().and_then(Tracking::failed);
        if let Some((texture, shape)) = &scan.picture {
            let scale = (area.width() / shape[0]).min(area.height() / shape[1]);
            let shown = Rect::from_center_size(area.center(), vec2(shape[0], shape[1]) * scale);
            painter.image(
                texture.id(),
                shown,
                Rect::from_min_max(pos2(1.0, 0.0), pos2(0.0, 1.0)),
                Color32::WHITE,
            );
            // A picture point, mirrored like the picture.
            let at = |x: f32, y: f32| pos2(shown.right() - x * scale, shown.top() + y * scale);
            if let Some(hand) = &scan.hand {
                let mark = Color32::from_rgb(64, 224, 132);
                let pad = 14.0;
                let bounds = Rect::from_two_pos(
                    at(hand.bounds[0] - pad, hand.bounds[1] - pad),
                    at(hand.bounds[2] + pad, hand.bounds[3] + pad),
                )
                .intersect(shown);
                painter.rect_stroke(
                    bounds,
                    theme.corner(ui::Radius::Sm),
                    Stroke::new(2.0, mark),
                    egui::StrokeKind::Inside,
                );
                for (from, to) in BONES {
                    let (a, b) = (hand.points[from], hand.points[to]);
                    painter.line_segment(
                        [at(a[0], a[1]), at(b[0], b[1])],
                        Stroke::new(1.5, Color32::from_white_alpha(210)),
                    );
                }
                for point in hand.points {
                    painter.circle_filled(at(point[0], point[1]), 2.5, mark);
                }
            }
        }
        if let Some(why) = camera_error
            .as_ref()
            .map(camera_words)
            .or(tracker_error.map(|why| tr!("hand-scanner-tracker-failed", why = why)))
        {
            // Over the (last) picture, on a scrim so it reads.
            painter.rect_filled(area, corner, theme.surface(0).gamma_multiply(0.86));
            let inner = area.shrink(theme.space(Space::S5));
            let galley = painter.layout(
                why,
                theme.text_font(TextRole::Body),
                theme.text,
                inner.width(),
            );
            let at = pos2(
                inner.center().x - galley.size().x / 2.0,
                inner.center().y - galley.size().y / 2.0,
            );
            painter.galley(at, galley, theme.text);
        } else if scan.picture.is_none() {
            painter.text(
                area.center(),
                egui::Align2::CENTER_CENTER,
                tr!("hand-scanner-starting"),
                theme.text_font(TextRole::Body),
                theme.text_muted(),
            );
        }
        ui.add_space(theme.space(Space::S2));
        words(
            ui,
            &scan.status(),
            TextRole::Caption,
            theme.text_muted(),
            theme,
        );
        ui.add_space(theme.space(Space::S2));

        // The camera, and holding its picture still.
        ui.horizontal(|ui| {
            let names: Vec<String> = if scan.cameras.is_empty() {
                vec![tr!("hand-scanner-default-camera")]
            } else {
                scan.cameras
                    .iter()
                    .map(|camera| camera.name.clone())
                    .collect()
            };
            let label = tr!("hand-scanner-camera");
            words(ui, &label, TextRole::Body, theme.text, theme);
            let mut chosen = scan.chosen.min(names.len() - 1);
            let response = ui::dropdown("sue-paint-hand-scanner-camera", &mut chosen, &names)
                .label(&label)
                .show(ui, theme);
            let help = tr!("hand-scanner-camera-hint");
            if ui::tooltip(response, &Tip::new(&label).help(&help), theme).changed()
                && chosen != scan.chosen
            {
                scan.chosen = chosen;
                scan.picture = None;
                scan.hand = None;
                scan.start(ui.ctx());
            }
            let label = if scan.paused {
                tr!("hand-scanner-resume")
            } else {
                tr!("hand-scanner-pause")
            };
            let response = ui::toggle_button("sue-paint-hand-scanner-pause", &label, scan.paused)
                .show(ui, theme);
            let help = tr!("hand-scanner-pause-hint");
            if ui::tooltip(response, &Tip::new(&label).help(&help), theme).clicked() {
                scan.paused = !scan.paused;
                if let Some(stream) = &scan.stream {
                    stream.pause(scan.paused);
                }
            }
            if camera_error.is_some() {
                let label = tr!("hand-scanner-try-again");
                let response = ui::button("sue-paint-hand-scanner-again", &label).show(ui, theme);
                if ui::tooltip(response, &Tip::new(&label), theme).clicked() {
                    if scan.parts.needs_package {
                        scan.cameras = camera::cameras().unwrap_or_default();
                        scan.chosen = scan.chosen.min(scan.cameras.len().saturating_sub(1));
                    }
                    scan.start(ui.ctx());
                }
            }
        });
        ui.add_space(theme.space(Space::S1));
        // Which hand of a figure, or whether a hand turns as a whole;
        // and which hand the camera shows, when it guesses wrong.
        ui.horizontal(|ui| {
            if scan.kind == Kind::Hand {
                let label = tr!("hand-scanner-turn-hand");
                let response = ui::checkbox("sue-paint-hand-scanner-turn", &label, scan.turn_hand)
                    .show(ui, theme);
                let help = tr!("hand-scanner-turn-hand-hint");
                if ui::tooltip(response, &Tip::new(&label).help(&help), theme).clicked() {
                    scan.turn_hand = !scan.turn_hand;
                }
            } else {
                let label = tr!("hand-scanner-pose");
                words(ui, &label, TextRole::Body, theme.text, theme);
                let options = [
                    tr!("hand-scanner-left-hand"),
                    tr!("hand-scanner-right-hand"),
                ];
                let now = Side::ALL.iter().position(|side| *side == scan.side);
                let picked = ui::segmented("sue-paint-hand-scanner-side", &options, now)
                    .show(ui, theme)
                    .picked;
                if let Some(side) = picked.and_then(|at| Side::ALL.get(at)).copied()
                    && side != scan.side
                {
                    // The hand posed so far goes back as it was; the other
                    // one follows from here.
                    scan.side = side;
                    let before = scan.before.clone();
                    Self::show_scanned(cx, scan, before);
                }
            }
            ui.add_space(theme.space(Space::S4));
            let label = tr!("hand-scanner-seen");
            words(ui, &label, TextRole::Body, theme.text, theme);
            let options = [
                tr!("hand-scanner-seen-automatic"),
                tr!("hand-scanner-seen-left"),
                tr!("hand-scanner-seen-right"),
            ];
            let now = [Seen::Automatic, Seen::Left, Seen::Right]
                .iter()
                .position(|seen| *seen == scan.seen);
            let picked = ui::segmented("sue-paint-hand-scanner-seen", &options, now)
                .show(ui, theme)
                .picked;
            if let Some(seen) =
                picked.and_then(|at| [Seen::Automatic, Seen::Left, Seen::Right].get(at).copied())
            {
                scan.seen = seen;
            }
        });
        ui.add_space(theme.space(Space::S2));
        words(
            ui,
            &tr!("hand-scanner-hint"),
            TextRole::Body,
            theme.text,
            theme,
        );
        words(
            ui,
            &tr!("hand-scanner-privacy"),
            TextRole::Caption,
            theme.text_muted(),
            theme,
        );
        Outcome::Open
    }

    /// Ends the Hand Scanner: the camera and the tracker stop, the
    /// figure goes back as it was, and with `keep` the scanned pose is
    /// then applied as one undo step.
    pub(super) fn hand_scanner_end(&mut self, cx: &mut AppletContext<'_>, keep: bool) {
        let Some(mut scan) = self.hand_scan.take() else {
            return;
        };
        scan.stop();
        let scanned = scan.shown.clone();
        if scanned == scan.before {
            return;
        }
        let before = scan.before.clone();
        Self::show_scanned(cx, &mut scan, before);
        if keep
            && let Some(edit) = figure::set_pose(
                &cx.world.arena,
                scan.layer,
                &scanned,
                &tr!("hand-scanner-title"),
            )
        {
            Self::apply(cx, edit);
        }
    }

    /// A Hand Scanner whose dialog is gone (closed some other way than
    /// by its buttons) ends as Cancel does: the camera never runs
    /// without its dialog.
    pub(crate) fn poll_hand_scanner(&mut self, cx: &mut AppletContext<'_>) {
        if self.hand_scan.is_some() && !matches!(self.dialog, Some(super::Dialog::HandScanner)) {
            self.hand_scanner_end(cx, false);
        }
    }

    /// What an open Hand Scanner is doing, for tests: whether its camera
    /// runs, how many pictures showed a hand and how many were looked at.
    #[must_use]
    pub fn hand_scanner_state(&self) -> Option<(bool, u64, u64)> {
        self.hand_scan
            .as_ref()
            .map(|scan| (scan.stream.is_some(), scan.found.0, scan.found.1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::figure::{HAND, hand_preset};

    #[test]
    fn what_the_made_up_camera_shows_reads_back_as_its_pose() {
        for right in [false, true] {
            let pose = hand_preset(4);
            let seen = observation_of(&pose, right, 640, 480);
            assert_eq!(seen.right, right);
            assert!(seen.bounds[0] >= 0.0 && seen.bounds[2] <= 640.0);
            assert!(seen.bounds[1] >= 0.0 && seen.bounds[3] <= 480.0);
            // The wrist is below the fingertips in the picture.
            assert!(seen.points[0][1] > seen.points[12][1]);
            let angles =
                hand_scan::solve(&hand_scan::to_model(&seen.world, right)).expect("a hand");
            let mut back = Pose(vec![[0.0; 3]; HAND.len()]);
            hand_scan::pose_hand(&mut back, &angles, true);
            for at in 1..=12 {
                for axis in 0..3 {
                    assert!((back.0[at][axis] - pose.0[at][axis]).abs() < 1.0);
                }
            }
        }
    }

    #[test]
    fn the_scripted_tracker_finds_the_hand_its_camera_painted() {
        let script: Script = Arc::new(|n| (n % 2 == 1).then(|| (hand_preset(2), false)));
        let parts = Parts::scripted(Duration::from_millis(1), script);
        let mut tracker = (parts.tracker.expect("a tracker"))().expect("makes one");
        let mut rgb = vec![0_u8; 640 * 480 * 3];
        for n in 0..4_u64 {
            paint_fake(n, None, &mut rgb);
            let frame = HandFrame {
                width: 640,
                height: 480,
                rgb: &rgb,
            };
            assert_eq!(tracker.track(&frame).expect("tracks").is_some(), n % 2 == 1);
        }
        // The painted hand is lighter than the backdrop where it is.
        let seen = observation_of(&hand_preset(0), false, 640, 480);
        paint_fake(0, Some(&seen), &mut rgb);
        let palm = seen.points[9];
        let at = (palm[1] as usize * 640 + palm[0] as usize) * 3;
        assert!(rgb[at] > 200, "the palm is painted: {}", rgb[at]);
        assert!(rgb[(470 * 640 + 630) * 3] < 130, "the backdrop is not");
    }

    #[test]
    fn tracking_runs_off_the_interface_thread_and_keeps_the_newest_picture() {
        let script: Script = Arc::new(|_| Some((hand_preset(3), true)));
        let parts = Parts::scripted(Duration::from_millis(1), script);
        let tracking = Tracking::start(parts.tracker.expect("a tracker"), || {});
        let mut rgb = vec![0_u8; 640 * 480 * 3];
        paint_fake(7, None, &mut rgb);
        tracking.look_at(Arc::new(CameraFrame {
            width: 640,
            height: 480,
            rgb,
            number: 8,
        }));
        let started = Instant::now();
        let tracked = loop {
            if let Some(tracked) = tracking.take() {
                break tracked;
            }
            assert!(started.elapsed().as_secs() < 10, "nothing was tracked");
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(tracked.frame.number, 8);
        assert!(tracked.hand.expect("a hand").right);
        assert!(tracking.failed().is_none());
        // A tracker that cannot be made says why.
        let broken = Tracking::start(Arc::new(|| Err("no models".to_owned())), || {});
        let started = Instant::now();
        while broken.failed().is_none() {
            assert!(started.elapsed().as_secs() < 10);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(broken.failed().as_deref(), Some("no models"));
    }

    #[test]
    fn poses_are_compared_by_their_largest_turn() {
        let a = hand_preset(0);
        let mut b = a.clone();
        assert!(apart(&a, &b) < f32::EPSILON);
        b.0[5][0] = 12.5;
        b.0[9][2] = -3.0;
        assert!((apart(&a, &b) - 12.5).abs() < 1e-5);
    }

    #[test]
    fn every_camera_failure_has_its_own_words() {
        let all = [
            CameraError::Unsupported,
            CameraError::NoCamera,
            CameraError::Busy,
            CameraError::Denied,
            CameraError::Gone,
            CameraError::Failed("0x80004005".to_owned()),
        ];
        let said: Vec<String> = all.iter().map(camera_words).collect();
        for (at, words) in said.iter().enumerate() {
            assert!(words.len() > 20, "{words}");
            assert!(said[..at].iter().all(|other| other != words));
        }
        assert!(said[5].contains("0x80004005"));
    }
}
