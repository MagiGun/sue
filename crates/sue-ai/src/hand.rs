//! Hand tracking, fully on this machine: where a hand is in a camera
//! picture and the 21 points of its skeleton in 3D, for Sue Paint's Hand
//! Scanner. Two small ONNX models run through [`OnnxModel`] (tract, on
//! the processor): a palm detector that finds a hand anywhere in the
//! picture, and a landmark model that reads the 21 points from a square
//! cut around it. Once a hand is found the next picture's cut comes from
//! the points just read, so the detector runs only when the hand is lost.
//!
//! The models are MediaPipe Hands (Google, Apache-2.0) as converted to
//! ONNX by the OpenCV Zoo (Apache-2.0). Sue does not ship them: they are
//! the optional package "Hand tracking" (`docs/packages.md`). The steps
//! around the models (letterboxing, anchors, the turned cut, mapping the
//! points back) follow MediaPipe's published pipeline and the OpenCV
//! Zoo's `mp_palmdet.py` and `mp_handpose.py`, written anew here.
//!
//! The 21 points, in MediaPipe's order: 0 the wrist; 1 to 4 the thumb
//! (its base at the wrist, two knuckles, the tip); 5 to 8 the index
//! finger (knuckle, two joints, tip); 9 to 12 the middle finger; 13 to 16
//! the ring finger; 17 to 20 the little finger.

use crate::{AiError, AiTask, ModelDescriptor, ModelFormat, OnnxInput, OnnxModel};

/// How many points a hand has.
pub const POINTS: usize = 21;

/// The file of the palm detector in the Hand tracking package.
pub const PALM_FILE: &str = "palm_detection.onnx";
/// The file of the landmark model in the Hand tracking package.
pub const LANDMARK_FILE: &str = "hand_landmark.onnx";

/// The side of the palm detector's square input.
const PALM_SIDE: usize = 192;
/// The side of the landmark model's square input.
const HAND_SIDE: usize = 224;
/// A palm scored under this is no hand.
const PALM_SCORE: f32 = 0.5;
/// Points read with less confidence than this are no hand, and the next
/// picture starts from the detector again.
const HAND_SCORE: f32 = 0.5;

/// One camera picture: `width` by `height` pixels of red, green and
/// blue bytes, rows from the top.
#[derive(Clone, Copy, Debug)]
pub struct HandFrame<'a> {
    /// Pixels across.
    pub width: usize,
    /// Pixels down.
    pub height: usize,
    /// `width * height * 3` bytes.
    pub rgb: &'a [u8],
}

/// A hand seen in a picture.
#[derive(Clone, Debug, PartialEq)]
pub struct HandObservation {
    /// The 21 points in the picture: x and y in pixels, z the depth
    /// relative to the wrist in the same scale (nearer is smaller).
    pub points: [[f32; 3]; POINTS],
    /// The 21 points in meters around the hand's middle: x to the
    /// picture's right, y down, z away from the camera.
    pub world: [[f32; 3]; POINTS],
    /// A right hand, as the model reads it; a left hand otherwise.
    pub right: bool,
    /// How sure the model is that this is a hand, 0 to 1.
    pub confidence: f32,
    /// The box around the points: left, top, right, bottom, in pixels.
    pub bounds: [f32; 4],
}

/// Something that finds a hand in camera pictures, one after another.
/// Sue Paint's Hand Scanner works with any tracker; tests give it one
/// that needs no model.
pub trait HandTracker: Send {
    /// The hand in `frame`, if one is seen.
    ///
    /// # Errors
    ///
    /// When the tracker cannot run on the picture.
    fn track(&mut self, frame: &HandFrame<'_>) -> Result<Option<HandObservation>, AiError>;

    /// Forgets the hand it followed, so the next picture is searched
    /// whole (after a pause, or when the camera changes).
    fn reset(&mut self) {}
}

/// The square cut the landmark model reads: its middle, its side, and
/// the direction in the picture that is "up" in the cut (from the wrist
/// to the fingers).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Cut {
    center: [f32; 2],
    side: f32,
    up: [f32; 2],
}

impl Cut {
    /// The picture position of the cut's point (`u`, `v`), each from
    /// -0.5 to 0.5, right and down.
    fn at(&self, u: f32, v: f32) -> [f32; 2] {
        let right = [-self.up[1], self.up[0]];
        [
            self.center[0] + self.side * (u * right[0] - v * self.up[0]),
            self.center[1] + self.side * (u * right[1] - v * self.up[1]),
        ]
    }

    /// A direction of the cut (right, down), turned into the picture's.
    fn turned(&self, x: f32, y: f32) -> [f32; 2] {
        let right = [-self.up[1], self.up[0]];
        [x * right[0] - y * self.up[0], x * right[1] - y * self.up[1]]
    }

    /// The cut around `points` whose up is `up`: their box in the turned
    /// frame, moved by `shift` of its height toward the fingers and
    /// `scale` times its longer side.
    fn around(points: &[[f32; 2]], up: [f32; 2], shift: f32, scale: f32) -> Option<Self> {
        let length = up[0].hypot(up[1]);
        if !length.is_finite() || length < 1e-6 || points.is_empty() {
            return None;
        }
        let up = [up[0] / length, up[1] / length];
        let right = [-up[1], up[0]];
        // Each point along the cut's right and down.
        let mut low = [f32::MAX; 2];
        let mut high = [f32::MIN; 2];
        for point in points {
            let along = [
                point[0] * right[0] + point[1] * right[1],
                -(point[0] * up[0] + point[1] * up[1]),
            ];
            for axis in 0..2 {
                low[axis] = low[axis].min(along[axis]);
                high[axis] = high[axis].max(along[axis]);
            }
        }
        let size = [high[0] - low[0], high[1] - low[1]];
        let middle = [
            (low[0] + high[0]) * 0.5,
            (low[1] + high[1]) * 0.5 - shift * size[1],
        ];
        let side = size[0].max(size[1]) * scale;
        if !side.is_finite() || side < 8.0 {
            return None;
        }
        Some(Self {
            center: [
                middle[0] * right[0] - middle[1] * up[0],
                middle[0] * right[1] - middle[1] * up[1],
            ],
            side,
            up,
        })
    }
}

/// The color of `frame` at (`x`, `y`), between its pixels; black outside.
fn sample(frame: &HandFrame<'_>, x: f32, y: f32) -> [f32; 3] {
    let (x, y) = (x - 0.5, y - 0.5);
    let (left, top) = (x.floor(), y.floor());
    let (fx, fy) = (x - left, y - top);
    let mut out = [0.0_f32; 3];
    for (dy, wy) in [(0_i64, 1.0 - fy), (1, fy)] {
        for (dx, wx) in [(0_i64, 1.0 - fx), (1, fx)] {
            let (px, py) = (left as i64 + dx, top as i64 + dy);
            if px < 0 || py < 0 || px >= frame.width as i64 || py >= frame.height as i64 {
                continue;
            }
            let at = (py as usize * frame.width + px as usize) * 3;
            let weight = wx * wy / 255.0;
            for (channel, value) in out.iter_mut().enumerate() {
                *value += f32::from(frame.rgb[at + channel]) * weight;
            }
        }
    }
    out
}

/// The palm detector's anchors: the middle of every cell of a 24 by 24
/// grid twice, then of a 12 by 12 grid six times, as MediaPipe lays
/// them out for a 192 pixel input.
fn anchors() -> Vec<[f32; 2]> {
    let mut out = Vec::with_capacity(2016);
    for (cells, repeat) in [(24_usize, 2_usize), (12, 6)] {
        for y in 0..cells {
            for x in 0..cells {
                for _ in 0..repeat {
                    out.push([
                        (x as f32 + 0.5) / cells as f32,
                        (y as f32 + 0.5) / cells as f32,
                    ]);
                }
            }
        }
    }
    out
}

/// The best palm in the detector's answers: its box (left, top, right,
/// bottom) and seven points, all as fractions of the square input.
fn best_palm(
    boxes: &[f32],
    scores: &[f32],
    anchors: &[[f32; 2]],
) -> Option<([f32; 4], [[f32; 2]; 7])> {
    let (at, score) = scores
        .iter()
        .copied()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    let score = 1.0 / (1.0 + (-score).exp());
    let row = boxes.get(at * 18..at * 18 + 18)?;
    let anchor = anchors.get(at)?;
    if score < PALM_SCORE {
        return None;
    }
    let side = PALM_SIDE as f32;
    let (cx, cy) = (row[0] / side + anchor[0], row[1] / side + anchor[1]);
    let (w, h) = (row[2] / side, row[3] / side);
    let mut points = [[0.0; 2]; 7];
    for (index, point) in points.iter_mut().enumerate() {
        *point = [
            row[4 + index * 2] / side + anchor[0],
            row[5 + index * 2] / side + anchor[1],
        ];
    }
    Some((
        [cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0],
        points,
    ))
}

/// Whether a hand that shows its palm to the camera is a right hand:
/// seen so, a right hand's index finger is to the right of its little
/// finger when the fingers point up (its thumb is on the right).
fn palm_up_is_right(points: &[[f32; 3]; POINTS]) -> bool {
    let from = points[0];
    let index = [points[5][0] - from[0], points[5][1] - from[1]];
    let little = [points[17][0] - from[0], points[17][1] - from[1]];
    index[0] * little[1] - index[1] * little[0] < 0.0
}

/// The hand tracker Sue ships the code for: MediaPipe's palm detector
/// and hand landmark model, run through tract.
#[derive(Debug)]
pub struct OnnxHandTracker {
    palm: OnnxModel,
    hand: OnnxModel,
    anchors: Vec<[f32; 2]>,
    /// The cut for the next picture, from the points just read.
    next: Option<Cut>,
}

impl OnnxHandTracker {
    /// The task these models do, for the model registry.
    pub const TASK: AiTask = AiTask::HandTracking;

    /// A tracker from the two models' files (`.onnx`).
    ///
    /// # Errors
    ///
    /// [`AiError::UnknownFormat`] when a file is not an ONNX model,
    /// [`AiError::Run`] when tract cannot plan one.
    pub fn load(palm: &[u8], landmark: &[u8]) -> Result<Self, AiError> {
        Ok(Self {
            palm: OnnxModel::load(palm)?,
            hand: OnnxModel::load(landmark)?,
            anchors: anchors(),
            next: None,
        })
    }

    /// A tracker from the folder the Hand tracking package is installed
    /// in ([`PALM_FILE`] and [`LANDMARK_FILE`]).
    ///
    /// # Errors
    ///
    /// [`AiError::Run`] when a file cannot be read, and as [`Self::load`].
    pub fn from_folder(folder: &std::path::Path) -> Result<Self, AiError> {
        let read = |name: &str| {
            std::fs::read(folder.join(name))
                .map_err(|error| AiError::Run(format!("{name}: {error}")))
        };
        Self::load(&read(PALM_FILE)?, &read(LANDMARK_FILE)?)
    }

    /// What the model registry lists for the two models, `sizes` their
    /// files' bytes.
    #[must_use]
    pub fn descriptors(sizes: [u64; 2]) -> [ModelDescriptor; 2] {
        [
            ModelDescriptor::new("hand-tracking.palm", Self::TASK, ModelFormat::Onnx)
                .with_size(sizes[0])
                .with_memory(64 * 1024 * 1024),
            ModelDescriptor::new("hand-tracking.landmark", Self::TASK, ModelFormat::Onnx)
                .with_size(sizes[1])
                .with_memory(64 * 1024 * 1024),
        ]
    }

    /// Searches the whole picture for a palm, and gives the cut around
    /// the hand it belongs to.
    fn detect(&self, frame: &HandFrame<'_>) -> Result<Option<Cut>, AiError> {
        // The picture fitted into the square, middle to middle, with
        // black bars.
        let longest = frame.width.max(frame.height) as f32;
        let offset = [
            (longest - frame.width as f32) / 2.0,
            (longest - frame.height as f32) / 2.0,
        ];
        let step = longest / PALM_SIDE as f32;
        let mut input = Vec::with_capacity(PALM_SIDE * PALM_SIDE * 3);
        for y in 0..PALM_SIDE {
            for x in 0..PALM_SIDE {
                input.extend(sample(
                    frame,
                    (x as f32 + 0.5) * step - offset[0],
                    (y as f32 + 0.5) * step - offset[1],
                ));
            }
        }
        let outputs = self.palm.run(vec![OnnxInput::F32(
            vec![1, PALM_SIDE, PALM_SIDE, 3],
            input,
        )])?;
        let (Some(boxes), Some(scores)) = (
            outputs
                .iter()
                .find(|output| output.shape.last() == Some(&18)),
            outputs
                .iter()
                .find(|output| output.shape.last() == Some(&1)),
        ) else {
            return Err(AiError::ShapeMismatch("palm detector outputs"));
        };
        let Some((bounds, points)) = best_palm(&boxes.values, &scores.values, &self.anchors) else {
            return Ok(None);
        };
        let pixel = |point: [f32; 2]| {
            [
                point[0] * longest - offset[0],
                point[1] * longest - offset[1],
            ]
        };
        let (wrist, middle) = (pixel(points[0]), pixel(points[2]));
        let corners = [
            pixel([bounds[0], bounds[1]]),
            pixel([bounds[2], bounds[1]]),
            pixel([bounds[2], bounds[3]]),
            pixel([bounds[0], bounds[3]]),
        ];
        // The palm's box is upright in the picture; the hand's cut is its
        // square turned to the hand, moved half a palm toward the fingers
        // and 2.6 palms wide (MediaPipe's numbers).
        let side = (bounds[2] - bounds[0]).max(bounds[3] - bounds[1]) * longest;
        let up = [middle[0] - wrist[0], middle[1] - wrist[1]];
        let length = up[0].hypot(up[1]);
        if length < 1e-3 || side < 4.0 {
            return Ok(None);
        }
        let up = [up[0] / length, up[1] / length];
        let center = [
            (corners[0][0] + corners[2][0]) / 2.0 + up[0] * side * 0.5,
            (corners[0][1] + corners[2][1]) / 2.0 + up[1] * side * 0.5,
        ];
        Ok(Some(Cut {
            center,
            side: side * 2.6,
            up,
        }))
    }

    /// Reads the 21 points from `cut` of the picture.
    fn read(&self, frame: &HandFrame<'_>, cut: &Cut) -> Result<Option<HandObservation>, AiError> {
        let mut input = Vec::with_capacity(HAND_SIDE * HAND_SIDE * 3);
        for y in 0..HAND_SIDE {
            for x in 0..HAND_SIDE {
                let [px, py] = cut.at(
                    (x as f32 + 0.5) / HAND_SIDE as f32 - 0.5,
                    (y as f32 + 0.5) / HAND_SIDE as f32 - 0.5,
                );
                input.extend(sample(frame, px, py));
            }
        }
        let outputs = self.hand.run(vec![OnnxInput::F32(
            vec![1, HAND_SIDE, HAND_SIDE, 3],
            input,
        )])?;
        // In the model's order: the points in the cut's pixels, how sure
        // it is, right or left, and the points in meters.
        let [screen, confidence, handed, world] = outputs.as_slice() else {
            return Err(AiError::ShapeMismatch("hand landmark outputs"));
        };
        if screen.values.len() != POINTS * 3
            || world.values.len() != POINTS * 3
            || confidence.values.is_empty()
            || handed.values.is_empty()
        {
            return Err(AiError::ShapeMismatch("hand landmark outputs"));
        }
        let confidence = confidence.values[0];
        if confidence < HAND_SCORE {
            return Ok(None);
        }
        let mut seen = HandObservation {
            points: [[0.0; 3]; POINTS],
            world: [[0.0; 3]; POINTS],
            // The model was taught on mirrored pictures (a phone's front
            // camera): what it calls left is a right hand in a picture as
            // the camera took it.
            right: handed.values[0] < 0.5,
            confidence,
            bounds: [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
        };
        let side = HAND_SIDE as f32;
        for index in 0..POINTS {
            let raw = &screen.values[index * 3..index * 3 + 3];
            let [x, y] = cut.at(raw[0] / side - 0.5, raw[1] / side - 0.5);
            seen.points[index] = [x, y, raw[2] / side * cut.side];
            seen.bounds = [
                seen.bounds[0].min(x),
                seen.bounds[1].min(y),
                seen.bounds[2].max(x),
                seen.bounds[3].max(y),
            ];
            let raw = &world.values[index * 3..index * 3 + 3];
            let [x, y] = cut.turned(raw[0], raw[1]);
            seen.world[index] = [x, y, raw[2]];
        }
        // The model's own reading of left or right is unsure on many
        // hands. When it is, the hand is taken to show its palm (what the
        // Hand Scanner asks for): then the thumb's side tells which hand
        // it is.
        let score = handed.values[0];
        if (0.25..=0.75).contains(&score) {
            seen.right = palm_up_is_right(&seen.points);
        }
        Ok(Some(seen))
    }

    /// The cut for the next picture, around the points just read: the
    /// box of the palm and the fingers' first joints, turned to the hand,
    /// twice as wide and a tenth toward the fingers (MediaPipe's numbers).
    fn follow(seen: &HandObservation) -> Option<Cut> {
        let point = |index: usize| [seen.points[index][0], seen.points[index][1]];
        let wrist = point(0);
        let (index, middle, ring) = (point(5), point(9), point(13));
        let aim = [
            ((index[0] + ring[0]) / 2.0 + middle[0]) / 2.0,
            ((index[1] + ring[1]) / 2.0 + middle[1]) / 2.0,
        ];
        let around: Vec<[f32; 2]> = [0, 1, 2, 3, 5, 6, 9, 10, 13, 14, 17, 18]
            .into_iter()
            .map(point)
            .collect();
        Cut::around(&around, [aim[0] - wrist[0], aim[1] - wrist[1]], 0.1, 2.0)
    }
}

impl HandTracker for OnnxHandTracker {
    fn track(&mut self, frame: &HandFrame<'_>) -> Result<Option<HandObservation>, AiError> {
        if frame.width == 0 || frame.height == 0 || frame.rgb.len() < frame.width * frame.height * 3
        {
            return Err(AiError::ShapeMismatch("camera picture"));
        }
        // The hand followed from the last picture first; when it is gone
        // (or there was none), the whole picture is searched.
        if let Some(cut) = self.next.take()
            && let Some(seen) = self.read(frame, &cut)?
        {
            self.next = Self::follow(&seen);
            return Ok(Some(seen));
        }
        let Some(cut) = self.detect(frame)? else {
            return Ok(None);
        };
        let seen = self.read(frame, &cut)?;
        self.next = seen.as_ref().and_then(Self::follow);
        Ok(seen)
    }

    fn reset(&mut self) {
        self.next = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_detector_has_mediapipes_2016_anchors() {
        let anchors = anchors();
        assert_eq!(anchors.len(), 2016);
        assert!((anchors[0][0] - 0.020_833_33).abs() < 1e-6);
        assert_eq!(anchors[0], anchors[1]);
        assert!((anchors[2][0] - 0.0625).abs() < 1e-6);
        // The coarse grid starts after 24 * 24 * 2 anchors.
        assert!((anchors[1152][0] - 1.0 / 24.0).abs() < 1e-6);
        assert!((anchors[2015][1] - 23.0 / 24.0).abs() < 1e-6);
    }

    #[test]
    fn the_best_palm_is_decoded_around_its_anchor() {
        let anchors = anchors();
        let mut boxes = vec![0.0; 2016 * 18];
        let mut scores = vec![-10.0; 2016];
        assert!(best_palm(&boxes, &scores, &anchors).is_none());
        // Anchor 1200 is cell (0, 4) of the coarse grid... wherever it
        // is, a box 48 pixels wide moved 19.2 right of it.
        scores[1200] = 4.0;
        boxes[1200 * 18] = 19.2;
        boxes[1200 * 18 + 2] = 48.0;
        boxes[1200 * 18 + 3] = 96.0;
        boxes[1200 * 18 + 4] = -9.6;
        let (bounds, points) = best_palm(&boxes, &scores, &anchors).expect("a palm");
        let anchor = anchors[1200];
        assert!((bounds[0] - (anchor[0] + 0.1 - 0.125)).abs() < 1e-5);
        assert!((bounds[2] - bounds[0] - 0.25).abs() < 1e-5);
        assert!((bounds[3] - bounds[1] - 0.5).abs() < 1e-5);
        assert!((points[0][0] - (anchor[0] - 0.05)).abs() < 1e-5);
        assert!((points[1][1] - anchor[1]).abs() < 1e-5);
    }

    #[test]
    fn a_cut_maps_its_middle_and_edges_into_the_picture() {
        let upright = Cut {
            center: [100.0, 80.0],
            side: 40.0,
            up: [0.0, -1.0],
        };
        assert_eq!(upright.at(0.0, 0.0), [100.0, 80.0]);
        assert_eq!(upright.at(0.5, 0.0), [120.0, 80.0]);
        assert_eq!(upright.at(0.0, 0.5), [100.0, 100.0]);
        // A hand lying to the right: the cut's up is the picture's right,
        // so the cut's right is the picture's down.
        let lying = Cut {
            up: [1.0, 0.0],
            ..upright
        };
        let [x, y] = lying.at(0.5, 0.0);
        assert!((x - 100.0).abs() < 1e-4 && (y - 100.0).abs() < 1e-4);
        let [x, y] = lying.at(0.0, -0.5);
        assert!((x - 120.0).abs() < 1e-4 && (y - 80.0).abs() < 1e-4);
    }

    #[test]
    fn the_cut_around_points_is_square_and_shifted_to_the_fingers() {
        let points = [[10.0, 10.0], [50.0, 10.0], [50.0, 90.0], [10.0, 90.0]];
        let cut = Cut::around(&points, [0.0, -2.0], 0.1, 2.0).expect("a cut");
        assert_eq!(cut.up, [0.0, -1.0]);
        assert!((cut.side - 160.0).abs() < 1e-3);
        assert!((cut.center[0] - 30.0).abs() < 1e-3);
        // A tenth of the box's height (80) toward the fingers (up).
        assert!((cut.center[1] - 42.0).abs() < 1e-3);
        assert!(Cut::around(&points, [0.0, 0.0], 0.1, 2.0).is_none());
        assert!(Cut::around(&[], [0.0, -1.0], 0.1, 2.0).is_none());
    }

    #[test]
    fn a_palm_shown_tells_right_from_left() {
        let mut points = [[0.0; 3]; POINTS];
        points[0] = [100.0, 300.0, 0.0];
        // A right hand, palm to the camera: index finger on the right.
        points[5] = [130.0, 200.0, 0.0];
        points[17] = [70.0, 210.0, 0.0];
        assert!(palm_up_is_right(&points));
        points.swap(5, 17);
        assert!(!palm_up_is_right(&points));
    }

    #[test]
    fn sampling_blends_pixels_and_is_black_outside() {
        let rgb = [255, 0, 0, 0, 0, 255];
        let frame = HandFrame {
            width: 2,
            height: 1,
            rgb: &rgb,
        };
        let middle = sample(&frame, 1.0, 0.5);
        assert!((middle[0] - 0.5).abs() < 1e-5 && (middle[2] - 0.5).abs() < 1e-5);
        assert_eq!(sample(&frame, -5.0, 0.5), [0.0; 3]);
        let first = sample(&frame, 0.5, 0.5);
        assert!((first[0] - 1.0).abs() < 1e-5 && first[2].abs() < 1e-5);
    }

    /// The real models, on a real picture of a hand. They are not part of
    /// Sue (the package Hand tracking brings them), so this runs only by
    /// hand: set `SUE_HAND_MODELS` to the folder that holds
    /// [`PALM_FILE`] and [`LANDMARK_FILE`], and `SUE_HAND_PICTURE` to a
    /// raw picture file: `width height` on the first line, then the RGB
    /// bytes.
    #[test]
    #[ignore = "needs the Hand tracking package's models and a picture of a hand"]
    fn the_real_models_find_a_hand_at_interactive_speed() {
        let folder = std::env::var("SUE_HAND_MODELS").expect("SUE_HAND_MODELS");
        let picture = std::fs::read(std::env::var("SUE_HAND_PICTURE").expect("SUE_HAND_PICTURE"))
            .expect("the picture");
        let end = picture
            .iter()
            .position(|byte| *byte == b'\n')
            .expect("a header");
        let header = String::from_utf8_lossy(&picture[..end]).into_owned();
        let mut sizes = header
            .split_whitespace()
            .map(|n| n.parse::<usize>().expect("a size"));
        let (width, height) = (sizes.next().expect("width"), sizes.next().expect("height"));
        let frame = HandFrame {
            width,
            height,
            rgb: &picture[end + 1..],
        };
        let mut tracker =
            OnnxHandTracker::from_folder(std::path::Path::new(&folder)).expect("the models");
        let started = std::time::Instant::now();
        let first = tracker.track(&frame).expect("runs").expect("a hand");
        let searched = started.elapsed();
        let settled = tracker.track(&frame).expect("runs").expect("a hand");
        let started = std::time::Instant::now();
        let runs = 20;
        for _ in 0..runs {
            tracker.track(&frame).expect("runs").expect("a hand");
        }
        let followed = started.elapsed() / runs;
        let last = tracker.track(&frame).expect("runs").expect("a hand");
        let moved = (0..POINTS)
            .map(|at| {
                (last.points[at][0] - settled.points[at][0])
                    .hypot(last.points[at][1] - settled.points[at][1])
            })
            .fold(0.0_f32, f32::max);
        println!("followed points moved at most {moved:.2} pixels");
        assert!(moved < 10.0, "following a still hand keeps its points");
        println!(
            "search {searched:?}, follow {followed:?} ({:.1} pictures a second), right {}, \
             confidence {:.3}, bounds {:?}",
            1.0 / followed.as_secs_f32(),
            first.right,
            first.confidence,
            first.bounds
        );
        for (index, point) in first.points.iter().enumerate() {
            println!("point {index}: {point:?} world {:?}", first.world[index]);
        }
        assert!(first.confidence > 0.8);
        assert!(followed.as_millis() < 100, "under ten pictures a second");
    }
}
