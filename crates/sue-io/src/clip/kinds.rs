//! The kinds of layer and the document data a `.clip` holds beyond pixels
//! and folders, with their parameters typed: fill, gradient and correction
//! layers, text, vector lines, frame borders, balloons, 3D layers,
//! animation folders with their cels, the 2D camera, rulers, layer
//! effects, palette color, layer thumbnails, layer comps, the timeline
//! with its tracks, keyframes and audio, and the canvas preview.
//!
//! Sue keeps them in tables of its own inside the file, laid out as
//! `docs/clip-sue-tables.md` describes, and keeps whatever Clip Studio
//! wrote for them untouched beside them ([`ClipNative`] names the two
//! blobs Sue can point at). Where the survey of 2026-11-16 showed how
//! Clip Studio stores one (fill, linear gradient, text, vector lines),
//! the writer puts it in Clip Studio's own storage too and the reader
//! reads it from there (`super::native`); for the rest Clip Studio sees
//! the pixels of each layer, and Sue sees the layer as it was made.
//!
//! A layer elsewhere in the document is named by its [`ClipLayerPath`]:
//! the places of the folders down to it, never a row id, since the writer
//! may number rows afresh.

use crate::sqlite::Value;

/// Where a layer is in the tree: its index among the root's children
/// (bottom first), then among that folder's children, and so on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClipLayerPath(pub Vec<u32>);

/// An 8-bit straight color with alpha.
pub type Rgba = [u8; 4];

/// What a layer draws, when it is more than pixels or a folder.
#[derive(Clone, Debug, PartialEq)]
pub enum ClipContent {
    /// One color over the whole canvas.
    Fill(ClipFill),
    /// A gradient over the canvas.
    Gradient(ClipGradient),
    /// A correction of what lies below.
    Correction(ClipCorrection),
    /// Text.
    Text(ClipText),
    /// Vector lines.
    Vector(ClipVector),
    /// A frame border folder's panels.
    Frame(ClipFrame),
    /// A balloon with its tails and text.
    Balloon(ClipBalloon),
    /// A 3D layer: Sue's own scene data, kept as it was; the pixels
    /// last drawn for it are the layer's pixels.
    ThreeD(ClipThreeD),
    /// An animation folder: its cels by frame.
    AnimationFolder(ClipAnimationFolder),
    /// The 2D camera folder.
    Camera(ClipCamera),
}

impl ClipContent {
    /// The part name the content is stored under.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Fill(_) => "fill",
            Self::Gradient(_) => "gradient",
            Self::Correction(_) => "correction",
            Self::Text(_) => "text",
            Self::Vector(_) => "vector",
            Self::Frame(_) => "frame",
            Self::Balloon(_) => "balloon",
            Self::ThreeD(_) => "3d",
            Self::AnimationFolder(_) => "animation-folder",
            Self::Camera(_) => "camera",
        }
    }
}

/// A fill layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipFill {
    /// Its color.
    pub color: Rgba,
}

/// The shape a gradient spreads in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipGradientShape {
    /// Along a line.
    Linear,
    /// In circles around the start.
    Circle,
    /// In ellipses around the start, `ClipGradient::ratio` wide.
    Ellipse,
}

/// What a gradient does past its ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipGradientRepeat {
    /// The end colors go on.
    Clamp,
    /// It starts again.
    Repeat,
    /// It goes back and forth.
    Reflect,
    /// Nothing is drawn past the ends.
    Empty,
}

/// One color stop of a gradient or a gradient map.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipStop {
    /// Where, 0 at the start to 1 at the end.
    pub at: f32,
    /// Its color.
    pub color: Rgba,
}

/// A gradient layer.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipGradient {
    /// Its shape.
    pub shape: ClipGradientShape,
    /// Where it starts, in canvas pixels.
    pub start: [f32; 2],
    /// Where it ends, in canvas pixels.
    pub end: [f32; 2],
    /// An ellipse's height over its width.
    pub ratio: f32,
    /// Past the ends.
    pub repeat: ClipGradientRepeat,
    /// Its stops, in order.
    pub stops: Vec<ClipStop>,
}

/// A correction layer and its settings.
#[derive(Clone, Debug, PartialEq)]
pub enum ClipCorrection {
    /// Brightness and contrast, each -100 to 100.
    BrightnessContrast {
        /// -100 to 100.
        brightness: f32,
        /// -100 to 100.
        contrast: f32,
    },
    /// Hue (-180 to 180), saturation and luminosity (-100 to 100).
    HueSaturationLuminosity {
        /// Degrees.
        hue: f32,
        /// -100 to 100.
        saturation: f32,
        /// -100 to 100.
        luminosity: f32,
    },
    /// Fewer tones per channel.
    Posterize {
        /// 2 to 255.
        levels: u32,
    },
    /// Every color turned to its opposite.
    Invert,
    /// Input and output ranges (0 to 255) and a middle gamma.
    Levels {
        /// Black and white of the input.
        input: [f32; 2],
        /// The middle, 1 for none.
        gamma: f32,
        /// Black and white of the output.
        output: [f32; 2],
    },
    /// Curves through points (0 to 255 each way) for all channels, red,
    /// green and blue.
    ToneCurve {
        /// Master, red, green, blue.
        curves: [Vec<[f32; 2]>; 4],
    },
    /// Color balance of the shadows, midtones and highlights, each
    /// cyan-red, magenta-green, yellow-blue from -100 to 100.
    ColorBalance {
        /// Shadows.
        shadows: [f32; 3],
        /// Midtones.
        midtones: [f32; 3],
        /// Highlights.
        highlights: [f32; 3],
        /// Keep the luminosity.
        keep_luminosity: bool,
    },
    /// Black and white at a threshold (0 to 255).
    Threshold {
        /// The threshold.
        level: f32,
    },
    /// Every tone mapped to a gradient.
    GradientMap {
        /// The stops, dark to light.
        stops: Vec<ClipStop>,
    },
}

impl ClipCorrection {
    /// Its number in the stored part (see `docs/clip-sue-tables.md`).
    #[must_use]
    pub fn number(&self) -> u8 {
        match self {
            Self::BrightnessContrast { .. } => 0,
            Self::HueSaturationLuminosity { .. } => 1,
            Self::Posterize { .. } => 2,
            Self::Invert => 3,
            Self::Levels { .. } => 4,
            Self::ToneCurve { .. } => 5,
            Self::ColorBalance { .. } => 6,
            Self::Threshold { .. } => 7,
            Self::GradientMap { .. } => 8,
        }
    }
}

/// A run of text in one style.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipTextRun {
    /// The text (line breaks are `\n`).
    pub text: String,
    /// The font family.
    pub font: String,
    /// Size in points.
    pub size: f32,
    /// Its color.
    pub color: Rgba,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
}

/// How lines of text line up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipAlign {
    /// At the start.
    Start,
    /// In the middle.
    Center,
    /// At the end.
    End,
    /// Both edges.
    Justify,
}

/// A text layer, or the text in a balloon.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipText {
    /// The runs, in order.
    pub runs: Vec<ClipTextRun>,
    /// Its top left corner, in canvas pixels.
    pub at: [f32; 2],
    /// A box the text wraps in, when it has one.
    pub frame: Option<[f32; 2]>,
    /// Written top to bottom.
    pub vertical: bool,
    /// How lines line up.
    pub align: ClipAlign,
    /// Line spacing, 1 for the font's own.
    pub line_spacing: f32,
}

impl ClipText {
    /// The whole text, its runs joined.
    #[must_use]
    pub fn plain(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }
}

/// One control point of a vector line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipPoint {
    /// Where, in canvas pixels.
    pub at: [f32; 2],
    /// Pen pressure, 0 to 1.
    pub pressure: f32,
    /// The line's width here, times `ClipLine::width`.
    pub width: f32,
    /// The line's opacity here, 0 to 1.
    pub opacity: f32,
    /// A corner: the line does not curve through it.
    pub corner: bool,
}

/// One vector line.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipLine {
    /// Its points, in drawing order.
    pub points: Vec<ClipPoint>,
    /// Its width in pixels.
    pub width: f32,
    /// Its color.
    pub color: Rgba,
    /// The brush it was drawn with, by the name Sue gives it (empty when
    /// not known).
    pub brush: String,
    /// The last point joins the first.
    pub closed: bool,
}

/// A vector layer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClipVector {
    /// Its lines, bottom first.
    pub lines: Vec<ClipLine>,
}

/// One panel of a frame border folder.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipPanel {
    /// Its outline's corners, in canvas pixels.
    pub points: Vec<[f32; 2]>,
    /// The border's width in pixels.
    pub border: f32,
    /// The border's color.
    pub color: Rgba,
}

/// A frame border folder.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClipFrame {
    /// Its panels.
    pub panels: Vec<ClipPanel>,
}

/// A balloon's outline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipBalloonShape {
    /// An ellipse.
    Ellipse,
    /// A rectangle.
    Rectangle,
    /// A rectangle with round corners.
    Rounded,
    /// A cloud.
    Cloud,
    /// Spikes, for a shout.
    Flash,
}

/// A balloon's tail.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipTail {
    /// Its points from the balloon out to the tip, in canvas pixels.
    pub points: Vec<[f32; 2]>,
    /// Its width where it leaves the balloon.
    pub width: f32,
}

/// A balloon.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipBalloon {
    /// Its outline.
    pub shape: ClipBalloonShape,
    /// Left, top, width and height, in canvas pixels.
    pub rect: [f32; 4],
    /// Its tails.
    pub tails: Vec<ClipTail>,
    /// The outline's width in pixels.
    pub line_width: f32,
    /// The outline's color.
    pub line_color: Rgba,
    /// The inside's color.
    pub fill: Rgba,
    /// The text in it.
    pub text: Option<ClipText>,
}

/// A 3D layer's own data, kept as it is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClipThreeD {
    /// Sue's scene bytes, opaque here.
    pub scene: Vec<u8>,
}

/// One cel of an animation folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipCel {
    /// The frame it starts on, from 0.
    pub frame: u32,
    /// The index of the folder's child it shows (bottom first), or none
    /// for a blank cel.
    pub child: Option<u32>,
}

/// An animation folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClipAnimationFolder {
    /// Its cels, by frame.
    pub cels: Vec<ClipCel>,
}

/// Where the 2D camera stands when no keyframe says otherwise.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipCamera {
    /// The middle of its view, in canvas pixels.
    pub center: [f32; 2],
    /// How much it zooms in, 1 for none.
    pub scale: f32,
    /// How far it turns, in radians.
    pub angle: f32,
}

/// A layer's effects (its layer color is `ClipLayer::draw_color`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClipEffects {
    /// A border around what it draws: width in pixels and color.
    pub border: Option<(f32, Rgba)>,
    /// Drawn as a halftone screen.
    pub tone: Option<ClipTone>,
    /// Only its lines kept, at this threshold (0 to 255).
    pub extract_line: Option<f32>,
    /// The second color of the layer color effect.
    pub sub_color: Option<[u8; 3]>,
}

/// The tone effect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipTone {
    /// Lines per inch.
    pub frequency: f32,
    /// Degrees.
    pub angle: f32,
    /// The dot: 0 round, 1 square, 2 diamond, 3 line, 4 noise.
    pub dot: u8,
}

/// What kind of ruler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipRulerKind {
    /// A straight line.
    Linear,
    /// A curve through its points.
    Curve,
    /// A shape of straight sides.
    Figure,
    /// Parallel lines.
    Parallel,
    /// Circles around a point.
    Concentric,
    /// Lines through a point.
    Radial,
    /// Mirror lines, `ClipRuler::lines` of them.
    Symmetry,
    /// A perspective with its vanishing points.
    Perspective,
}

/// A ruler on a layer.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipRuler {
    /// Its kind.
    pub kind: ClipRulerKind,
    /// Its points, in canvas pixels (vanishing points for a perspective).
    pub points: Vec<[f32; 2]>,
    /// How many lines a symmetry ruler has.
    pub lines: u32,
    /// Strokes snap to it.
    pub snap: bool,
    /// Shown while any layer is current, not only its own.
    pub everywhere: bool,
}

/// A small picture: straight RGBA8.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipPicture {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

/// How a layer shows in a layer comp.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipCompState {
    /// The layer.
    pub layer: ClipLayerPath,
    /// It shows.
    pub visible: bool,
    /// 0 to 1.
    pub opacity: f32,
    /// Its blend number, as `ClipLayer::composite`.
    pub composite: i64,
}

/// A layer comp.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipComp {
    /// Its name.
    pub name: String,
    /// The layers it records.
    pub states: Vec<ClipCompState>,
}

/// How a keyframe goes on to the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipInterpolation {
    /// It keeps its value until the next.
    Hold,
    /// In a straight line.
    Linear,
    /// Easing in and out.
    Smooth,
}

/// One keyframe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipKey {
    /// Its frame, from 0.
    pub frame: u32,
    /// The value there.
    pub value: f64,
    /// How it goes on.
    pub interpolation: ClipInterpolation,
}

/// The keyframes of one property of one layer.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipTrack {
    /// The layer.
    pub layer: ClipLayerPath,
    /// The property, by Sue's name (`opacity`, `x`, `y`, `scale`,
    /// `angle`, `camera.x`...).
    pub property: String,
    /// Its keyframes, by frame.
    pub keys: Vec<ClipKey>,
}

/// A sound on the timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipAudio {
    /// What people call it.
    pub name: String,
    /// The frame it starts on.
    pub start: u32,
    /// Its volume, 1 for as recorded.
    pub volume: f32,
    /// Muted.
    pub muted: bool,
    /// The sound file's bytes, as they were imported.
    pub file: Vec<u8>,
}

/// The timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipTimeline {
    /// Its name.
    pub name: String,
    /// Frames per second.
    pub fps: f32,
    /// How many frames it has.
    pub frames: u32,
    /// The first and last frame played.
    pub playback: [u32; 2],
    /// Its keyframe tracks.
    pub tracks: Vec<ClipTrack>,
    /// Its sounds.
    pub audio: Vec<ClipAudio>,
}

/// What Clip Studio itself stored for a generated layer, found by the
/// columns Sue knows to look at; their layout is not yet observed, so they
/// are only pointed at, never changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipNative<'a> {
    /// `FilterLayerInfo`: a correction layer.
    Correction(&'a [u8]),
    /// `GradationFillInfo`: a gradient or fill layer.
    GradientOrFill(&'a [u8]),
}

impl<'a> ClipNative<'a> {
    /// The native data in a layer's other columns, when there is some.
    #[must_use]
    pub fn of(columns: &'a [(String, Value)]) -> Option<Self> {
        let blob = |name: &str| {
            columns
                .iter()
                .find(|(column, _)| column.eq_ignore_ascii_case(name))
                .and_then(|(_, value)| value.bytes())
        };
        blob("FilterLayerInfo")
            .map(Self::Correction)
            .or_else(|| blob("GradationFillInfo").map(Self::GradientOrFill))
    }
}
