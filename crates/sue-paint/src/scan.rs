//! Pose Scanner and Hand Scanner (Clip Studio Paint 1.10 and 5.0): a photo
//! of a person or a hand poses a drawing figure or a 3D hand. Two openly
//! licensed models (Apache 2.0) find the points: MoveNet Lightning the
//! body's seventeen and MediaPipe's hand landmark model the hand's
//! twenty one. They run on the processor through `sue_ai`, downloaded once
//! into Sue's models folder and checked against their SHA-256.
//! [`crate::figure::aimed`] then turns each joint so its bone lies along
//! what the photo shows.

use std::path::{Path, PathBuf};

use sue_ai::{OnnxInput, OnnxModel};

use crate::figure::{Body, HAND, Pose};

/// A model file Sue downloads the first time it is needed.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ModelFile {
    /// Its name in the models folder.
    pub name: &'static str,
    /// Where it downloads from.
    pub url: &'static str,
    /// Its SHA-256, in hex.
    pub sha256: &'static str,
}

/// MoveNet SinglePose Lightning (Google, Apache 2.0), as ONNX.
pub(crate) const BODY_MODEL: ModelFile = ModelFile {
    name: "movenet-singlepose-lightning.onnx",
    url: "https://huggingface.co/Xenova/movenet-singlepose-lightning/resolve/main/onnx/model.onnx",
    sha256: "1ad4f8d6c2f776a9967db3993c9ca740bc350104f9d37c151dc183fc29a464ad",
};

/// MediaPipe's hand landmark model (Google, Apache 2.0), as OpenCV's model
/// zoo converted it to ONNX.
pub(crate) const HAND_MODEL: ModelFile = ModelFile {
    name: "mediapipe-hand-landmark-2023feb.onnx",
    url: "https://huggingface.co/opencv/handpose_estimation_mediapipe/resolve/main/handpose_estimation_mediapipe_2023feb.onnx",
    sha256: "db0898ae717b76b075d9bf563af315b29562e11f8df5027a1ef07b02bef6d81c",
};

/// `file` in `dir`, downloaded first when it is not there whole.
pub(crate) fn fetch(
    dir: &Path,
    file: ModelFile,
    progress: &dyn Fn(f32),
    cancelled: &dyn Fn() -> bool,
) -> Result<PathBuf, String> {
    let path = dir.join(file.name);
    if sue_platform::update::sha256_of(&path).is_ok_and(|sum| sum == file.sha256) {
        return Ok(path);
    }
    // Downloaded beside, then checked and put in place.
    let part = dir.join(format!(".{}.download", file.name));
    let _ = sue_platform::fs::remove_dir_all(&part);
    let got = sue_platform::update::download_link(
        file.url,
        &part,
        concat!("Sue/", env!("CARGO_PKG_VERSION")),
        progress,
        cancelled,
    )?;
    let sum = sue_platform::update::sha256_of(&got).map_err(|error| error.to_string())?;
    if sum != file.sha256 {
        let _ = sue_platform::fs::remove_dir_all(&part);
        return Err(tr!("model-download-damaged", name = file.name));
    }
    let bytes = sue_platform::fs::read(&got).map_err(|error| error.to_string())?;
    sue_platform::write_atomic(&path, &bytes).map_err(|error| error.to_string())?;
    let _ = sue_platform::fs::remove_dir_all(&part);
    Ok(path)
}

/// A picture as straight RGBA8, `width` by `height`.
pub(crate) struct Photo<'a> {
    pub pixels: &'a [u8],
    pub width: u32,
    pub height: u32,
}

impl Photo<'_> {
    /// The square `[x, y, size]` (photo pixels, reaching past the edges as
    /// black) shrunk to `n` by `n`: each cell the average of the pixels in
    /// it, red, green and blue from 0 to 255.
    fn square(&self, [x, y, size]: [f32; 3], n: usize) -> Vec<[f32; 3]> {
        let cell = size / n as f32;
        let step = (cell / 3.0).max(1.0);
        let mut out = Vec::with_capacity(n * n);
        for row in 0..n {
            for column in 0..n {
                let (x0, y0) = (x + column as f32 * cell, y + row as f32 * cell);
                let mut sum = [0.0; 3];
                let mut count = 0.0_f32;
                let mut sy = y0 + step / 2.0;
                while sy < y0 + cell {
                    let mut sx = x0 + step / 2.0;
                    while sx < x0 + cell {
                        count += 1.0;
                        if sx >= 0.0
                            && sy >= 0.0
                            && (sx as u32) < self.width
                            && (sy as u32) < self.height
                        {
                            let at = ((sy as u32 * self.width + sx as u32) * 4) as usize;
                            for (channel, total) in sum.iter_mut().enumerate() {
                                *total += f32::from(self.pixels[at + channel]);
                            }
                        }
                        sx += step;
                    }
                    sy += step;
                }
                out.push(sum.map(|total| total / count.max(1.0)));
            }
        }
        out
    }

    /// The whole photo as a square about its middle.
    fn whole(&self) -> [f32; 3] {
        let (width, height) = (self.width as f32, self.height as f32);
        let size = width.max(height);
        [(width - size) / 2.0, (height - size) / 2.0, size]
    }
}

/// The body's seventeen points in `photo` (MoveNet's order: nose, eyes,
/// ears, shoulders, elbows, wrists, hips, knees, ankles, left before
/// right): across, down (photo pixels) and how sure, 0 to 1.
pub(crate) fn body_points(model: &OnnxModel, photo: &Photo<'_>) -> Result<Vec<[f32; 3]>, String> {
    // The whole photo first, then twice more framed around the person
    // found (MoveNet reads a person who fills its square best).
    let mean = |points: &[[f32; 3]]| points.iter().map(|point| point[2]).sum::<f32>() / 17.0;
    let mut best = read_body(model, photo, photo.whole())?;
    for _ in 0..2 {
        let (mut low, mut high) = ([f32::MAX; 2], [f32::MIN; 2]);
        for point in best.iter().filter(|point| point[2] >= 0.1) {
            for axis in 0..2 {
                low[axis] = low[axis].min(point[axis]);
                high[axis] = high[axis].max(point[axis]);
            }
        }
        if low[0] > high[0] {
            break;
        }
        let size = ((high[0] - low[0]).max(high[1] - low[1]) * 1.5).max(32.0);
        let middle = [(low[0] + high[0]) / 2.0, (low[1] + high[1]) / 2.0];
        let again = read_body(
            model,
            photo,
            [middle[0] - size / 2.0, middle[1] - size / 2.0, size],
        )?;
        if mean(&again) <= mean(&best) {
            break;
        }
        best = again;
    }
    Ok(best)
}

fn read_body(
    model: &OnnxModel,
    photo: &Photo<'_>,
    square: [f32; 3],
) -> Result<Vec<[f32; 3]>, String> {
    const N: usize = 192;
    let input: Vec<i32> = photo
        .square(square, N)
        .iter()
        .flat_map(|pixel| pixel.map(|value| value.round() as i32))
        .collect();
    let outputs = model
        .run(vec![OnnxInput::I32(vec![1, N, N, 3], input)])
        .map_err(|error| error.to_string())?;
    let values = &outputs.first().ok_or("no output")?.values;
    if values.len() < 17 * 3 {
        return Err("the body model gave too few points".to_owned());
    }
    Ok((0..17)
        .map(|at| {
            let [down, across, sure] = [values[at * 3], values[at * 3 + 1], values[at * 3 + 2]];
            [
                square[0] + across * square[2],
                square[1] + down * square[2],
                sure,
            ]
        })
        .collect())
}

/// A hand found in a photo.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Hand {
    /// MediaPipe's twenty one points: across, down and toward the back
    /// (photo pixels; smaller is nearer).
    pub points: Vec<[f32; 3]>,
    /// How sure the model is that it is a hand, 0 to 1.
    pub sure: f32,
    /// How much the model takes it for a right hand in a mirrored (selfie)
    /// picture, 0 to 1.
    pub right: f32,
}

/// The hand in `photo`: the model reads a square around a hand, so it
/// looks over the whole photo and squares of it at two sizes, keeps the
/// surest, then reads again around what it found.
pub(crate) fn hand_points(model: &OnnxModel, photo: &Photo<'_>) -> Result<Option<Hand>, String> {
    let whole = photo.whole();
    let mut squares = vec![whole];
    for parts in [2.0_f32, 3.0] {
        let step = whole[2] / parts;
        let size = step * 1.4;
        for row in 0..parts as u32 {
            for column in 0..parts as u32 {
                squares.push([
                    whole[0] + column as f32 * step - (size - step) / 2.0,
                    whole[1] + row as f32 * step - (size - step) / 2.0,
                    size,
                ]);
            }
        }
    }
    let mut best: Option<Hand> = None;
    for square in squares {
        let hand = read_hand(model, photo, square)?;
        if best.as_ref().is_none_or(|best| hand.sure > best.sure) {
            best = Some(hand);
        }
    }
    let Some(mut best) = best else {
        return Ok(None);
    };
    // Once more, framed around the hand as MediaPipe tracks one.
    let (mut low, mut high) = ([f32::MAX; 2], [f32::MIN; 2]);
    for point in &best.points {
        for axis in 0..2 {
            low[axis] = low[axis].min(point[axis]);
            high[axis] = high[axis].max(point[axis]);
        }
    }
    let size = (high[0] - low[0]).max(high[1] - low[1]) * 1.6;
    if size > 8.0 {
        let middle = [(low[0] + high[0]) / 2.0, (low[1] + high[1]) / 2.0];
        let again = read_hand(
            model,
            photo,
            [middle[0] - size / 2.0, middle[1] - size / 2.0, size],
        )?;
        if again.sure >= best.sure * 0.9 {
            best = again;
        }
    }
    Ok((best.sure >= 0.5).then_some(best))
}

fn read_hand(model: &OnnxModel, photo: &Photo<'_>, square: [f32; 3]) -> Result<Hand, String> {
    const N: usize = 224;
    let input: Vec<f32> = photo
        .square(square, N)
        .iter()
        .flat_map(|pixel| pixel.map(|value| value / 255.0))
        .collect();
    let outputs = model
        .run(vec![OnnxInput::F32(vec![1, N, N, 3], input)])
        .map_err(|error| error.to_string())?;
    let first = |at: usize| {
        outputs
            .get(at)
            .and_then(|output| output.values.first().copied())
            .unwrap_or(0.0)
    };
    let values = &outputs.first().ok_or("no output")?.values;
    if values.len() < 63 {
        return Err("the hand model gave too few points".to_owned());
    }
    let scale = square[2] / N as f32;
    Ok(Hand {
        points: (0..21)
            .map(|at| {
                [
                    square[0] + values[at * 3] * scale,
                    square[1] + values[at * 3 + 1] * scale,
                    values[at * 3 + 2] * scale,
                ]
            })
            .collect(),
        sure: first(1),
        right: first(2),
    })
}

fn minus(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

fn unit(a: [f32; 3]) -> Option<[f32; 3]> {
    let size = length(a);
    (size > 1e-6).then(|| a.map(|value| value / size))
}

/// The least sure a point may be and still pose a joint.
const SURE: f32 = 0.2;

/// A figure posed as `points` (from [`body_points`]) show it, for `body`:
/// across and up from the photo, and toward or away from it from how much
/// shorter each limb looks than it is. `None` when the shoulders and hips
/// are not clear enough to stand it up.
pub(crate) fn body_pose(points: &[[f32; 3]], body: &Body) -> Option<Pose> {
    if points.len() < 17 {
        return None;
    }
    let joints = body.joints();
    let rest = crate::figure::bones(&Pose(vec![[0.0; 3]; joints.len()]), body);
    // The photo's points in the figure's space: across, up.
    let at = |index: usize| [points[index][0], -points[index][1], 0.0];
    let sure = |index: usize| points[index][2] >= SURE;
    let middle = |a: [f32; 3], b: [f32; 3]| [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, 0.0];
    if !(sure(5) && sure(6) && sure(11) && sure(12)) {
        return None;
    }
    let shoulders = middle(at(5), at(6));
    let hips = middle(at(11), at(12));
    // Facing away when the left shoulder shows on the photo's left.
    let away = at(5)[0] < at(6)[0];
    let forward = if away { -1.0 } else { 1.0 };
    // Pixels a figure meter: the longest-looking of the torso and the long
    // bones (the one turned least away) sets it.
    let model_torso = length(minus(
        middle(rest[5].0, rest[8].0),
        middle(rest[11].0, rest[14].0),
    ));
    let mut scale = length(minus(shoulders, hips)) / model_torso.max(1e-3);
    // Upper arms, forearms, thighs and shins, with their photo points.
    let limbs: [(usize, usize, usize); 8] = [
        (5, 5, 7),
        (6, 7, 9),
        (8, 6, 8),
        (9, 8, 10),
        (11, 11, 13),
        (12, 13, 15),
        (14, 12, 14),
        (15, 14, 16),
    ];
    let bone_length = |joint: usize| length(joints[joint].bone);
    for (joint, from, to) in limbs {
        if sure(from) && sure(to) {
            scale = scale.max(length(minus(at(to), at(from))) / bone_length(joint).max(1e-3));
        }
    }
    if scale <= 0.0 {
        return None;
    }
    let mut targets: Vec<Option<[f32; 3]>> = vec![None; joints.len()];
    for (joint, from, to) in limbs {
        if !(sure(from) && sure(to)) {
            continue;
        }
        let flat = minus(at(to), at(from)).map(|value| value / scale);
        let full = bone_length(joint);
        let depth = (full * full - dot(flat, flat)).max(0.0).sqrt();
        // Arms and thighs reach forward when shortened; shins fold back.
        let toward = if matches!(joint, 12 | 15) {
            -forward
        } else {
            forward
        };
        targets[joint] = Some([flat[0], flat[1], depth * toward]);
    }
    // The head leans as the nose (or the ears) sit over the shoulders.
    let head = if sure(3) && sure(4) {
        Some(middle(at(3), at(4)))
    } else if sure(0) {
        Some(at(0))
    } else {
        None
    };
    if let Some(head) = head {
        targets[3] = Some(minus(head, shoulders));
    }
    // The hips: stood along the torso, turned about when facing away.
    let up = unit(minus(shoulders, hips))?;
    let lean = crate::figure::aimed(&joints[..1], &Pose(vec![[0.0; 3]]), None, &[Some(up)]);
    let base_angle = lean.0[0];
    let root = rotation(base_angle, if away { 180.0 } else { 0.0 });
    Some(crate::figure::aimed(
        &joints,
        &Pose(vec![[0.0; 3]; joints.len()]),
        Some(root),
        &targets,
    ))
}

/// The turn `[x, y, z]` degrees (as poses keep them), then `yaw` more
/// about its own up.
fn rotation(degrees: [f32; 3], yaw: f32) -> [[f32; 3]; 3] {
    let turn = |[x, y, z]: [f32; 3]| {
        let (sx, cx) = x.to_radians().sin_cos();
        let (sy, cy) = y.to_radians().sin_cos();
        let (sz, cz) = z.to_radians().sin_cos();
        let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
        let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
        let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
        times(&times(&rz, &ry), &rx)
    };
    times(&turn(degrees), &turn([0.0, yaw, 0.0]))
}

fn times(a: &[[f32; 3]; 3], b: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    [0, 1, 2].map(|row| [0, 1, 2].map(|column| (0..3).map(|k| a[row][k] * b[k][column]).sum()))
}

/// The hand's sixteen joints and the photo points each bone runs between.
const FINGER_BONES: [(usize, usize, usize); 15] = [
    (1, 5, 6),
    (2, 6, 7),
    (3, 7, 8),
    (4, 9, 10),
    (5, 10, 11),
    (6, 11, 12),
    (7, 13, 14),
    (8, 14, 15),
    (9, 15, 16),
    (10, 17, 18),
    (11, 18, 19),
    (12, 19, 20),
    (13, 1, 2),
    (14, 2, 3),
    (15, 3, 4),
];

/// A 3D hand posed as `hand` shows. Sue's hand is a right hand, so a left
/// hand poses it as its mirror image.
pub(crate) fn hand_pose(hand: &Hand) -> Option<Pose> {
    if hand.points.len() < 21 {
        return None;
    }
    // The figure's space: across, up, toward the viewer.
    let mut points: Vec<[f32; 3]> = hand.points.iter().map(|p| [p[0], -p[1], -p[2]]).collect();
    let frame = |points: &[[f32; 3]]| -> Option<[[f32; 3]; 3]> {
        let up = unit(minus(points[9], points[0]))?;
        let across = minus(points[17], points[5]);
        let across = unit(minus(across, up.map(|value| value * dot(across, up))))?;
        let normal = cross(across, up);
        Some([
            [across[0], up[0], normal[0]],
            [across[1], up[1], normal[1]],
            [across[2], up[2], normal[2]],
        ])
    };
    let palm = frame(&points)?;
    // A right hand's fingers curl toward its palm, the frame's normal; a
    // left hand's away from it. A flat hand goes by the model's guess.
    let normal = [palm[0][2], palm[1][2], palm[2][2]];
    let span = length(minus(points[9], points[0])).max(1e-3);
    let curl: f32 = [(5, 8), (9, 12), (13, 16), (17, 20)]
        .iter()
        .map(|(base, tip)| dot(minus(points[*tip], points[*base]), normal) / span)
        .sum::<f32>()
        / 4.0;
    let left = if curl.abs() > 0.08 {
        curl < 0.0
    } else {
        hand.right > 0.5
    };
    if left {
        for point in &mut points {
            point[0] = -point[0];
        }
    }
    let palm = frame(&points)?;
    let mut targets: Vec<Option<[f32; 3]>> = vec![None; HAND.len()];
    for (joint, from, to) in FINGER_BONES {
        targets[joint] = Some(minus(points[to], points[from]));
    }
    Some(crate::figure::aimed(
        &HAND,
        &Pose(vec![[0.0; 3]; HAND.len()]),
        Some(palm),
        &targets,
    ))
}

/// A scan running in the background: the 3D layer it poses and, once
/// done, the pose or why there is none.
type Found = std::sync::Arc<std::sync::Mutex<Option<Result<Pose, String>>>>;

/// The scans still running.
#[derive(Debug, Default)]
pub(crate) struct Scans {
    running: Vec<(u64, sue_core::EntityId, Found)>,
}

impl crate::studio::SuePaint {
    /// Pose Scanner or Hand Scanner: picks a photo for the current 3D
    /// layer's figure or hand.
    pub(crate) fn pick_scan(&mut self, cx: &mut sue_wm::AppletContext<'_>, hand: bool) {
        let Some(layer) = self.layer() else {
            return;
        };
        let kind = crate::figure::of_layer(&cx.world.arena, layer)
            .and_then(|model| cx.world.arena.get_by_id(model).ok())
            .map(|model| Body::of(model).kind);
        let wanted = if hand {
            crate::figure::Kind::Hand
        } else {
            crate::figure::Kind::Body
        };
        if kind != Some(wanted) {
            cx.notify(if hand {
                tr!("hand-scanner-needs-hand")
            } else {
                tr!("pose-scanner-needs-figure")
            });
            return;
        }
        let start = self
            .last_dir
            .clone()
            .or_else(|| cx.catalog.folders.get("home").cloned());
        cx.request(sue_wm::ShellRequest::PickFile(
            sue_wm::FilePick::open(
                Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
                if hand { "hand-scanner" } else { "pose-scanner" },
                &if hand {
                    tr!("hand-scanner")
                } else {
                    tr!("pose-scanner")
                },
            )
            .extensions(&["png", "jpg", "jpeg", "webp", "jxl", "bmp", "tif", "tiff"])
            .starting_in(start),
        ));
    }

    /// Reads the photo at `path` in the background (downloading the model
    /// the first time) and poses the current 3D layer as it shows.
    pub(crate) fn scan(&mut self, cx: &mut sue_wm::AppletContext<'_>, path: &Path, hand: bool) {
        let Some(layer) = self.layer() else {
            return;
        };
        let Some(body) = crate::figure::of_layer(&cx.world.arena, layer)
            .and_then(|model| cx.world.arena.get_by_id(model).ok())
            .map(Body::of)
        else {
            return;
        };
        let models = cx
            .catalog
            .folders
            .get("models")
            .or_else(|| cx.catalog.folders.get("cache"))
            .cloned()
            .unwrap_or_else(std::env::temp_dir);
        let photo = path.to_path_buf();
        let found: Found = std::sync::Arc::default();
        let into = std::sync::Arc::clone(&found);
        let label = if hand {
            tr!("hand-scanner")
        } else {
            tr!("pose-scanner")
        };
        let id = cx.transfers.spawn(
            sue_wm::TransferKind::Import,
            &label,
            Some(photo.clone()),
            move |job| {
                let file = if hand { HAND_MODEL } else { BODY_MODEL };
                let result = (|| {
                    let model = fetch(
                        &models,
                        file,
                        &|fraction| job.set_progress(fraction * 0.7),
                        &|| job.is_cancelled(),
                    )?;
                    let bytes =
                        sue_platform::fs::read(&model).map_err(|error| error.to_string())?;
                    let model = OnnxModel::load(&bytes).map_err(|error| error.to_string())?;
                    job.set_progress(0.8);
                    let ([width, height], pixels) = crate::subview::decode(&photo)?;
                    let photo = Photo {
                        pixels: &pixels,
                        width,
                        height,
                    };
                    if hand {
                        let found =
                            hand_points(&model, &photo)?.ok_or_else(|| tr!("no-hand-in-photo"))?;
                        hand_pose(&found).ok_or_else(|| tr!("no-hand-in-photo"))
                    } else {
                        let points = body_points(&model, &photo)?;
                        body_pose(&points, &body).ok_or_else(|| tr!("no-person-in-photo"))
                    }
                })();
                let failed = result.as_ref().err().cloned();
                if let Ok(mut slot) = into.lock() {
                    *slot = Some(result);
                }
                failed.map_or(Ok(()), Err)
            },
        );
        self.scans.running.push((id, layer, found));
    }

    /// Poses the layers whose scans finished.
    pub(crate) fn poll_scans(&mut self, cx: &mut sue_wm::AppletContext<'_>) {
        if self.scans.running.is_empty() {
            return;
        }
        let mut done = Vec::new();
        self.scans.running.retain(|(id, layer, found)| {
            let running = cx.transfers.get(*id).is_some_and(|transfer| {
                matches!(transfer.state, sue_wm::TransferState::Running(_))
            });
            if !running && let Some(result) = found.lock().ok().and_then(|mut slot| slot.take()) {
                done.push((*layer, result));
            }
            running
        });
        for (layer, result) in done {
            match result {
                Ok(pose) => {
                    // Seen as the photo was: from the front, level.
                    let view = cx
                        .world
                        .arena
                        .get_by_id(layer)
                        .map(|entity| crate::three::View3D {
                            yaw: 0.0,
                            pitch: 0.0,
                            ..crate::three::View3D::of(entity)
                        });
                    if let Ok(view) = view
                        && let Some(edit) = crate::figure::set_pose_seen(
                            &cx.world.arena,
                            layer,
                            &pose,
                            &view,
                            &tr!("pose-scanner"),
                        )
                    {
                        Self::apply(cx, edit);
                    }
                }
                // The transfer says why already; say it here too.
                Err(error) => cx.fail(error),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::figure::{Kind, bones};

    fn angle(a: [f32; 3], b: [f32; 3]) -> f32 {
        let (a, b) = (unit(a).unwrap(), unit(b).unwrap());
        dot(a, b).clamp(-1.0, 1.0).acos().to_degrees()
    }

    fn flat(a: [f32; 3]) -> [f32; 3] {
        [a[0], a[1], 0.0]
    }

    /// The seventeen photo points a front camera sees of `pose`.
    fn photo_of(pose: &Pose, body: &Body) -> Vec<[f32; 3]> {
        let bones = bones(pose, body);
        let shot = |p: [f32; 3]| [500.0 + p[0] * 300.0, 700.0 - p[1] * 300.0, 0.9];
        let head = bones[4];
        let ear = |side: f32| {
            let mut p = head.0;
            p[0] += side * 0.08;
            p[1] += 0.1;
            shot(p)
        };
        let mut points = vec![shot(head.1); 3];
        points.push(ear(1.0));
        points.push(ear(-1.0));
        for joint in [5, 8, 6, 9, 7, 10, 11, 14, 12, 15, 13, 16] {
            points.push(shot(bones[joint].0));
        }
        points
    }

    #[test]
    fn the_pose_scanner_lays_limbs_where_the_photo_shows_them() {
        let body = Body::default();
        for preset in [0, 1, 2, 4, 5] {
            let truth = body.preset(preset);
            let points = photo_of(&truth, &body);
            let scanned = body_pose(&points, &body).unwrap();
            let (want, got) = (bones(&truth, &body), bones(&scanned, &body));
            for joint in [5, 6, 8, 9, 11, 12, 14, 15] {
                let (w, g) = (
                    minus(want[joint].1, want[joint].0),
                    minus(got[joint].1, got[joint].0),
                );
                if length(flat(w)) < 0.02 {
                    continue;
                }
                assert!(
                    angle(flat(w), flat(g)) < 6.0,
                    "preset {preset} joint {joint}: {} degrees off",
                    angle(flat(w), flat(g))
                );
            }
        }
        // Too unsure of the shoulders: no pose.
        let mut unsure = photo_of(&body.preset(0), &body);
        unsure[5][2] = 0.05;
        assert!(body_pose(&unsure, &body).is_none());
    }

    #[test]
    fn a_body_seen_from_behind_is_turned_about() {
        let body = Body::default();
        let mut truth = body.preset(2);
        truth.0[0][1] += 180.0;
        let scanned = body_pose(&photo_of(&truth, &body), &body).unwrap();
        let got = bones(&scanned, &body);
        // Its left shoulder is on the photo's left, as in the photo.
        assert!(got[5].0[0] < got[8].0[0]);
    }

    /// MediaPipe's twenty one points of `pose`, seen from the front.
    fn hand_photo(pose: &Pose, body: &Body) -> Hand {
        let bones = bones(pose, body);
        let shot = |p: [f32; 3]| [500.0 + p[0] * 2000.0, 600.0 - p[1] * 2000.0, -p[2] * 2000.0];
        let mut points = vec![shot(bones[0].0)];
        for joint in [13, 14, 15] {
            points.push(shot(bones[joint].0));
        }
        points.push(shot(bones[15].1));
        for first in [1, 4, 7, 10] {
            for bone in &bones[first..first + 3] {
                points.push(shot(bone.0));
            }
            points.push(shot(bones[first + 2].1));
        }
        Hand {
            points,
            sure: 0.99,
            right: 0.0,
        }
    }

    #[test]
    fn the_hand_scanner_bends_every_finger_as_the_photo_does() {
        let body = Body {
            kind: Kind::Hand,
            ..Body::default()
        };
        for preset in 0..body.presets().len() {
            let truth = body.preset(preset);
            let scanned = hand_pose(&hand_photo(&truth, &body)).unwrap();
            let (want, got) = (bones(&truth, &body), bones(&scanned, &body));
            for joint in 1..HAND.len() {
                let off = angle(
                    minus(want[joint].1, want[joint].0),
                    minus(got[joint].1, got[joint].0),
                );
                assert!(
                    off < 3.0,
                    "preset {preset} joint {joint}: {off} degrees off"
                );
            }
        }
    }

    #[test]
    fn a_left_hand_poses_the_right_hand_as_its_mirror() {
        let body = Body {
            kind: Kind::Hand,
            ..Body::default()
        };
        // A fist, mirrored across: a left hand.
        let truth = body.preset(1);
        let mut photo = hand_photo(&truth, &body);
        for point in &mut photo.points {
            point[0] = 1000.0 - point[0];
        }
        let scanned = hand_pose(&photo).unwrap();
        let (want, got) = (bones(&truth, &body), bones(&scanned, &body));
        let off = angle(minus(want[3].1, want[3].0), minus(got[3].1, got[3].0));
        assert!(off < 3.0, "{off}");
    }

    /// Real photos through the real models: needs the network once, then
    /// the models folder. `cargo test -p sue-paint -- --ignored real_scans`
    /// with `SUE_SCAN_PHOTOS` a folder holding `body.jpg` and `hand.jpg`.
    #[test]
    #[ignore = "needs the network and photos"]
    fn real_scans() {
        let dir = std::env::temp_dir().join("sue-scan-models");
        sue_platform::fs::create_dir_all(&dir).unwrap();
        let photos = PathBuf::from(std::env::var("SUE_SCAN_PHOTOS").unwrap());
        let open = |name: &str| crate::subview::decode(&photos.join(name)).unwrap();
        let body_model =
            std::fs::read(fetch(&dir, BODY_MODEL, &|_| {}, &|| false).unwrap()).unwrap();
        let body_model = OnnxModel::load(&body_model).unwrap();
        let ([width, height], pixels) = open("body.jpg");
        let photo = Photo {
            pixels: &pixels,
            width,
            height,
        };
        let points = body_points(&body_model, &photo).unwrap();
        println!("body {points:?}");
        let pose = body_pose(&points, &Body::default()).expect("a pose");
        // Front and side pictures of the poses, to look at.
        let shoot = |pose: &Pose, body: &Body, name: &str| {
            for (yaw, side) in [(0.0, "front"), (90.0, "side")] {
                let view = crate::three::View3D {
                    yaw,
                    pitch: 0.0,
                    ..crate::three::View3D::default()
                };
                let buffers = crate::three::render_framed(
                    &crate::figure::mesh(pose, body),
                    &view,
                    [400, 400],
                    Some(body.frame()),
                );
                let tile = crate::three::draw(&buffers, &view).unwrap();
                std::fs::write(
                    photos.join(format!("{name}-{side}.png")),
                    sue_io::encode_png(&tile).unwrap(),
                )
                .unwrap();
            }
        };
        shoot(&pose, &Body::default(), "body-pose");
        let hand_model =
            std::fs::read(fetch(&dir, HAND_MODEL, &|_| {}, &|| false).unwrap()).unwrap();
        let hand_model = OnnxModel::load(&hand_model).unwrap();
        let ([width, height], pixels) = open("hand.jpg");
        let photo = Photo {
            pixels: &pixels,
            width,
            height,
        };
        let hand = hand_points(&hand_model, &photo).unwrap().expect("a hand");
        println!(
            "hand sure {} right {} {:?}",
            hand.sure, hand.right, hand.points
        );
        let pose = hand_pose(&hand).unwrap();
        println!("hand pose {pose:?}");
        let hand_body = Body {
            kind: crate::figure::Kind::Hand,
            ..Body::default()
        };
        shoot(&pose, &hand_body, "hand-pose");
    }
}
