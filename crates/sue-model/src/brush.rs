//! The brush model. A [`BrushPreset`] says everything a brush engine needs
//! to paint with one brush: what lays the paint down, the tip, the
//! settings a pen can drive and what drives each, starting and ending,
//! color jitter, mixing, and how the pointer is steadied. It is the
//! extension component `sue.brush`, so a brush is an ordinary entity that
//! every studio reads, a brush set is a folder of them, and both save in
//! an ordinary `.sue` file.
//!
//! Three things are open (backbone rule 3):
//!
//! - **Parameters and inputs are keys, not fields.** A preset holds a value
//!   for any parameter key ([`param`]) and a list of [`Binding`]s, each
//!   making one parameter follow one input key ([`input`]). So size can
//!   follow pressure, hardness can follow tilt and spacing can follow
//!   speed without a new field for each pair. A [`BrushRegistry`] says
//!   which keys a build knows; a feature registers its own under its own
//!   namespace, and a preset keeps the keys of a feature that is not
//!   installed.
//! - **The encoding is tagged.** After a leading version every field is a
//!   key and a run of bytes with its length, so a reader skips what it
//!   does not know and writes it back unchanged ([`BrushPreset::extra`]).
//!   A field can be added without a new version; a new version is for a
//!   change an older Sue must not misread (a new engine, a new choice in
//!   a list), and the kernel then keeps the whole component as bytes.
//! - **Pictures are named by what they hold.** A tip picture or a texture
//!   is referenced by the SHA-256 of its encoded file, never by a name in
//!   a list this run happens to have.
//!
//! Brushes saved by Sue 0.0.75 and older were text
//! (`size=8;opacity=1;...`); [`BrushPreset::from_legacy_text`] reads it.

use std::collections::BTreeMap;

use sue_core::ComponentType;
use sue_core::codec::{CodecError, Reader, Writer};

use crate::{BlendMode, Response};

/// The SHA-256 of an encoded file (a tip picture, a texture).
pub type ContentHash = [u8; 32];

/// Unknown or foreign data: a key and its bytes, in the order read.
pub type Extra = Vec<(String, Vec<u8>)>;

/// The parameter keys every build knows. Each is a number a pen can drive
/// through a [`Binding`]. A feature that adds one names it in its own
/// namespace (`vendor.bristles`).
pub mod param {
    /// The brush's diameter, in pixels.
    pub const SIZE: &str = "size";
    /// How opaque a whole stroke gets, 0 to 1.
    pub const OPACITY: &str = "opacity";
    /// How strong one dab is when dabs add up, 0 to 1.
    pub const FLOW: &str = "flow";
    /// How dense the tip's own coverage is, 0 to 1.
    pub const DENSITY: &str = "density";
    /// 1 is a hard disc; lower values fade toward the rim.
    pub const HARDNESS: &str = "hardness";
    /// The tip's thickness: its height over its width, 0 to 1.
    pub const ASPECT: &str = "aspect";
    /// The tip's turn, in degrees.
    pub const ANGLE: &str = "angle";
    /// The distance between dabs: a share of the size or pixels, as
    /// [`super::Spacing`] says.
    pub const SPACING: &str = "spacing";
    /// How far dabs fall from the line, in brush sizes.
    pub const SCATTER: &str = "scatter";
    /// How strongly the texture breaks up the stroke, 0 to 1.
    pub const TEXTURE_STRENGTH: &str = "texture_strength";
    /// A turn of the paint's hue, in degrees.
    pub const HUE: &str = "hue";
    /// A change of the paint's saturation, -1 to 1.
    pub const SATURATION: &str = "saturation";
    /// A change of the paint's luminosity, -1 to 1.
    pub const LUMINOSITY: &str = "luminosity";
}

/// The input keys every build knows. An engine hands each to a binding as
/// a share from 0 to 1; how it scales speed, distance and time into that
/// range is the engine's to say.
pub mod input {
    /// How hard the pen presses.
    pub const PRESSURE: &str = "pressure";
    /// How fast the pen moves, still to fast.
    pub const SPEED: &str = "speed";
    /// How far the pen leans, upright to flat.
    pub const TILT: &str = "tilt";
    /// The way the pen leans, as a share of a full turn.
    pub const TILT_DIRECTION: &str = "tilt_direction";
    /// The pen's turn about its own axis, as a share of a full turn.
    pub const TWIST: &str = "twist";
    /// The way the line runs, as a share of a full turn.
    pub const DIRECTION: &str = "direction";
    /// How far along the stroke.
    pub const DISTANCE: &str = "distance";
    /// How long since the stroke began.
    pub const TIME: &str = "time";
    /// Chance, new for every dab.
    pub const RANDOM: &str = "random";
    /// Chance, drawn once for the whole stroke.
    pub const STROKE_RANDOM: &str = "stroke_random";
}

/// What a parameter's number measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// Pixels of the picture.
    Pixels,
    /// A share, where 1 is all of it.
    Share,
    /// Multiples of the brush's size.
    Sizes,
    /// Degrees of a turn.
    Degrees,
}

/// A parameter a preset can hold a value for and bind to an input.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamDef {
    /// Its key, e.g. `size`.
    pub key: String,
    /// What its number measures.
    pub unit: Unit,
    /// Its value in a preset that holds none.
    pub default: f32,
    /// The least it can be.
    pub min: f32,
    /// The most it can be.
    pub max: f32,
}

impl ParamDef {
    /// A parameter `key` in `unit`, `default` when a preset holds none,
    /// from `min` to `max`.
    pub fn new(key: impl Into<String>, unit: Unit, default: f32, min: f32, max: f32) -> Self {
        Self {
            key: key.into(),
            unit,
            default,
            min,
            max,
        }
    }

    /// `value` kept within the parameter's range.
    #[must_use]
    pub fn clamp(&self, value: f32) -> f32 {
        value.clamp(self.min, self.max)
    }
}

/// An input a binding can follow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputDef {
    /// Its key, e.g. `pressure`.
    pub key: String,
    /// It is a direction: 1 is the same as 0, so smoothing and
    /// interpolation go the short way round.
    pub wraps: bool,
}

impl InputDef {
    /// An input `key` that runs from 0 to 1.
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            wraps: false,
        }
    }

    /// An input `key` that is a direction.
    pub fn direction(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            wraps: true,
        }
    }
}

/// Every parameter and input a build knows, by key. The settings panel
/// lists them and an engine evaluates them; a preset may hold keys that
/// are not here (a feature that is not installed), and keeps them.
#[derive(Clone, Debug, Default)]
pub struct BrushRegistry {
    params: BTreeMap<String, ParamDef>,
    inputs: BTreeMap<String, InputDef>,
}

impl BrushRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A registry with the keys of [`param`] and [`input`].
    #[must_use]
    pub fn standard() -> Self {
        let mut registry = Self::new();
        for def in [
            ParamDef::new(param::SIZE, Unit::Pixels, 8.0, 0.0, 10_000.0),
            ParamDef::new(param::OPACITY, Unit::Share, 1.0, 0.0, 1.0),
            ParamDef::new(param::FLOW, Unit::Share, 1.0, 0.0, 1.0),
            ParamDef::new(param::DENSITY, Unit::Share, 1.0, 0.0, 1.0),
            ParamDef::new(param::HARDNESS, Unit::Share, 1.0, 0.0, 1.0),
            ParamDef::new(param::ASPECT, Unit::Share, 1.0, 0.01, 1.0),
            ParamDef::new(param::ANGLE, Unit::Degrees, 0.0, -360.0, 360.0),
            ParamDef::new(param::SPACING, Unit::Sizes, 0.06, 0.0, 10_000.0),
            ParamDef::new(param::SCATTER, Unit::Sizes, 0.0, 0.0, 100.0),
            ParamDef::new(param::TEXTURE_STRENGTH, Unit::Share, 1.0, 0.0, 1.0),
            ParamDef::new(param::HUE, Unit::Degrees, 0.0, -180.0, 180.0),
            ParamDef::new(param::SATURATION, Unit::Share, 0.0, -1.0, 1.0),
            ParamDef::new(param::LUMINOSITY, Unit::Share, 0.0, -1.0, 1.0),
        ] {
            registry.register_param(def);
        }
        for def in [
            InputDef::new(input::PRESSURE),
            InputDef::new(input::SPEED),
            InputDef::new(input::TILT),
            InputDef::direction(input::TILT_DIRECTION),
            InputDef::direction(input::TWIST),
            InputDef::direction(input::DIRECTION),
            InputDef::new(input::DISTANCE),
            InputDef::new(input::TIME),
            InputDef::new(input::RANDOM),
            InputDef::new(input::STROKE_RANDOM),
        ] {
            registry.register_input(def);
        }
        registry
    }

    /// Registers a parameter. Registering the same key twice keeps the
    /// last.
    pub fn register_param(&mut self, def: ParamDef) -> &mut Self {
        self.params.insert(def.key.clone(), def);
        self
    }

    /// Registers an input. Registering the same key twice keeps the last.
    pub fn register_input(&mut self, def: InputDef) -> &mut Self {
        self.inputs.insert(def.key.clone(), def);
        self
    }

    /// The parameter `key`.
    #[must_use]
    pub fn param(&self, key: &str) -> Option<&ParamDef> {
        self.params.get(key)
    }

    /// The input `key`.
    #[must_use]
    pub fn input(&self, key: &str) -> Option<&InputDef> {
        self.inputs.get(key)
    }

    /// Every parameter, by key.
    pub fn params(&self) -> impl Iterator<Item = &ParamDef> + '_ {
        self.params.values()
    }

    /// Every input, by key.
    pub fn inputs(&self) -> impl Iterator<Item = &InputDef> + '_ {
        self.inputs.values()
    }

    /// The value of parameter `key` in `preset` before anything drives
    /// it: the preset's own, or the parameter's default. `None` for a key
    /// neither holds.
    #[must_use]
    pub fn value(&self, preset: &BrushPreset, key: &str) -> Option<f32> {
        preset
            .value(key)
            .or_else(|| self.param(key).map(|def| def.default))
    }
}

/// Reads and writes a choice by its stored name.
macro_rules! named {
    ($type:ident, $what:literal, { $($variant:ident => $name:literal),+ $(,)? }) => {
        impl $type {
            /// Every choice, in menu order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The stored name.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name),+
                }
            }

            /// The choice stored as `name`, if this build knows it.
            #[must_use]
            pub fn from_name(name: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|choice| choice.name() == name)
            }

            fn read(key: &str, payload: &[u8]) -> Result<Self, CodecError> {
                let name = text(key, payload)?;
                Self::from_name(&name)
                    .ok_or_else(|| CodecError::new(format!("{key}: {name:?} is no {}", $what), 0))
            }
        }
    };
}

/// What lays the paint down.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum EngineKind {
    /// One continuous swept line (pens): no dab ever shows.
    #[default]
    Stroke,
    /// Dabs stamped along the line (airbrushes, mixing brushes, picture
    /// tips).
    Dab,
    /// The tip's picture laid along the line as a ribbon, bent round
    /// every curve.
    Ribbon,
    /// Paints nothing of its own and works on what is under it: the kind
    /// is a key such as `blur` or `smudge`.
    Blend(String),
}

impl EngineKind {
    const BLEND: &'static str = "blend:";

    fn stored(&self) -> String {
        match self {
            Self::Stroke => "stroke".to_owned(),
            Self::Dab => "dab".to_owned(),
            Self::Ribbon => "ribbon".to_owned(),
            Self::Blend(kind) => format!("{}{kind}", Self::BLEND),
        }
    }

    fn read(key: &str, payload: &[u8]) -> Result<Self, CodecError> {
        let name = text(key, payload)?;
        Ok(match name.as_str() {
            "stroke" => Self::Stroke,
            "dab" => Self::Dab,
            "ribbon" => Self::Ribbon,
            other => match other.strip_prefix(Self::BLEND) {
                Some(kind) => Self::Blend(kind.to_owned()),
                None => {
                    return Err(CodecError::new(
                        format!("{key}: {name:?} is no brush engine"),
                        0,
                    ));
                }
            },
        })
    }
}

/// One picture of a brush tip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PictureRef {
    /// The encoded file with this hash, kept in the document or the
    /// brush library.
    Hash(ContentHash),
    /// A picture known only by the name an old brush gave it
    /// ([`BrushPreset::from_legacy_text`]). Whoever holds the pictures
    /// turns it into a [`Self::Hash`].
    Named(String),
}

/// Which of a tip's pictures a dab takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pick {
    /// One at random for each dab.
    #[default]
    Random,
    /// Each in turn.
    Sequence,
}

named!(Pick, "way to pick a tip picture", {
    Random => "random",
    Sequence => "sequence",
});

/// The shape of the tip. Its hardness, thickness and turn are the
/// parameters [`param::HARDNESS`], [`param::ASPECT`] and [`param::ANGLE`],
/// so a pen can drive them and they hold for pictures as well.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TipSource {
    /// A disc.
    #[default]
    Round,
    /// One or more pictures.
    Images {
        /// The pictures.
        pictures: Vec<PictureRef>,
        /// Which one a dab takes.
        pick: Pick,
        /// A picture with colors paints them instead of the brush's
        /// color.
        keep_colors: bool,
    },
}

/// What the [`param::SPACING`] number means.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Spacing {
    /// The engine picks a spacing that shows no dabs; the number is not
    /// used.
    #[default]
    Auto,
    /// A share of the brush's size.
    Ratio,
    /// Pixels, whatever the size.
    Fixed,
}

named!(Spacing, "kind of spacing", {
    Auto => "auto",
    Ratio => "ratio",
    Fixed => "fixed",
});

/// How soft the edge of a hard tip is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AntiAlias {
    /// Hard pixels.
    None,
    /// A thin soft edge.
    #[default]
    Weak,
    /// A medium soft edge.
    Medium,
    /// A wide soft edge.
    Strong,
}

named!(AntiAlias, "anti-aliasing level", {
    None => "none",
    Weak => "weak",
    Medium => "medium",
    Strong => "strong",
});

/// Where a texture's pattern comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextureSource {
    /// Sue's own paper grain, made by rule and the same everywhere.
    Grain,
    /// The encoded picture with this hash.
    Picture(ContentHash),
}

/// How a texture meets the stroke.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextureMode {
    /// The pattern's dark parts hold paint back in proportion.
    #[default]
    Multiply,
    /// The pattern is taken away from the coverage, so light strokes
    /// catch only the paper's high spots.
    Subtract,
}

named!(TextureMode, "texture mode", {
    Multiply => "multiply",
    Subtract => "subtract",
});

/// A pattern that breaks up the stroke, as paper does. How strongly is
/// the parameter [`param::TEXTURE_STRENGTH`].
#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    /// The pattern.
    pub source: TextureSource,
    /// Its size, 1 as it is.
    pub scale: f32,
    /// Its turn, in degrees.
    pub rotation: f32,
    /// How it meets the stroke.
    pub mode: TextureMode,
}

impl Texture {
    /// `source` at its own size, not turned, multiplied.
    #[must_use]
    pub fn new(source: TextureSource) -> Self {
        Self {
            source,
            scale: 1.0,
            rotation: 0.0,
            mode: TextureMode::Multiply,
        }
    }
}

/// How a binding's amount meets the parameter's value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Combine {
    /// The value is multiplied by the amount.
    #[default]
    Multiply,
    /// The amount is added to the value.
    Add,
}

named!(Combine, "way to combine a binding", {
    Multiply => "multiply",
    Add => "add",
});

/// One parameter following one input. The input (0 to 1) goes through the
/// curve, the result picks an amount between `min` and `max`, and the
/// amount is multiplied into or added to the parameter's value. Several
/// bindings of one parameter apply one after another, in list order.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    /// The parameter key that follows.
    pub param: String,
    /// The input key it follows.
    pub input: String,
    /// How the input becomes a share.
    pub curve: Response,
    /// The amount where the curve gives 0.
    pub min: f32,
    /// The amount where the curve gives 1.
    pub max: f32,
    /// How the amount meets the value.
    pub combine: Combine,
    /// Switched on. One that is off keeps its settings for when it is
    /// switched on again.
    pub on: bool,
    /// Fields this build does not know, kept as they were read.
    pub extra: Extra,
}

impl Binding {
    /// `param` follows `input`: straight through, multiplied, from
    /// nothing at 0 to all of it at 1.
    pub fn new(param: impl Into<String>, input: impl Into<String>) -> Self {
        Self {
            param: param.into(),
            input: input.into(),
            curve: Response::LINEAR,
            min: 0.0,
            max: 1.0,
            combine: Combine::Multiply,
            on: true,
            extra: Vec::new(),
        }
    }

    /// With amounts from `min` to `max`.
    #[must_use]
    pub fn range(mut self, min: f32, max: f32) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    /// With `curve` between the input and the amount.
    #[must_use]
    pub fn curve(mut self, curve: Response) -> Self {
        self.curve = curve;
        self
    }

    /// With the amount added instead of multiplied.
    #[must_use]
    pub fn added(mut self) -> Self {
        self.combine = Combine::Add;
        self
    }

    /// Switched on or off.
    #[must_use]
    pub fn switched(mut self, on: bool) -> Self {
        self.on = on;
        self
    }

    /// The amount at `input` (0 to 1).
    #[must_use]
    pub fn amount(&self, input: f32) -> f32 {
        self.min + (self.max - self.min) * self.curve.eval(input.clamp(0.0, 1.0))
    }

    /// `value` after this binding at `input` (0 to 1); unchanged when the
    /// binding is off.
    #[must_use]
    pub fn apply(&self, value: f32, input: f32) -> f32 {
        if !self.on {
            return value;
        }
        match self.combine {
            Combine::Multiply => value * self.amount(input),
            Combine::Add => value + self.amount(input),
        }
    }
}

/// How a stroke's starting and ending are measured.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TaperMode {
    /// Starting and ending in pixels.
    #[default]
    Length,
    /// Starting and ending as percentages of the whole stroke.
    Percent,
    /// The stroke fades out by the ending length from where it began.
    Fade,
}

named!(TaperMode, "way to measure a taper", {
    Length => "length",
    Percent => "percent",
    Fade => "fade",
});

/// How a stroke starts from a point and ends in one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Taper {
    /// The length the stroke grows over at its start.
    pub start: f32,
    /// The length the stroke thins over at its end.
    pub end: f32,
    /// What the two lengths measure.
    pub mode: TaperMode,
    /// The starting and ending thin the brush.
    pub size: bool,
    /// The starting and ending fade the paint.
    pub opacity: bool,
    /// Quicker strokes get longer starts and ends.
    pub by_speed: bool,
}

impl Default for Taper {
    fn default() -> Self {
        Self {
            start: 0.0,
            end: 0.0,
            mode: TaperMode::Length,
            size: true,
            opacity: false,
            by_speed: false,
        }
    }
}

/// How far the paint's color wanders. Amounts are 0 to 1: hue as a share
/// of half the color wheel, saturation and luminosity as shares of their
/// range, and how far toward the sub color.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColorJitter {
    /// Every dab differs.
    pub tip: bool,
    /// Hue for each dab.
    pub hue: f32,
    /// Saturation for each dab.
    pub saturation: f32,
    /// Luminosity for each dab.
    pub luminosity: f32,
    /// Blend with the sub color for each dab.
    pub sub: f32,
    /// The whole stroke differs from the one before.
    pub stroke: bool,
    /// Hue for each stroke.
    pub stroke_hue: f32,
    /// Saturation for each stroke.
    pub stroke_saturation: f32,
    /// Luminosity for each stroke.
    pub stroke_luminosity: f32,
}

/// How the brush mixes with the paint already on the picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mixing {
    /// How much of the color under the brush it picks up, 0 to 1.
    pub amount: f32,
    /// How much of its own paint it carries, 0 (none: it only moves what
    /// is there) to 1.
    pub load: f32,
    /// How far it drags the color it picked up, 0 to 1.
    pub pull: f32,
    /// How much it softens what is under it, 0 to 1.
    pub blur: f32,
}

impl Mixing {
    /// A brush that picks up `amount` of what is under it, fully loaded,
    /// without dragging or softening.
    #[must_use]
    pub const fn new(amount: f32) -> Self {
        Self {
            amount,
            load: 1.0,
            pull: 0.0,
            blur: 0.0,
        }
    }
}

/// How the pointer is steadied and the stroke finished.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputSettings {
    /// Stabilization, 0 (none) to 100.
    pub stabilization: u32,
    /// Stabilization loosens as the pen speeds up.
    pub by_speed: bool,
    /// Post correction when the pen lifts, 0 (none) to 10.
    pub post_correction: u32,
    /// A quick stroke carries on into a fine point when the pen lifts.
    pub flick: bool,
    /// Holding the pen still at a stroke's end makes it a clean shape.
    pub smart_shape: bool,
}

/// One brush: everything an engine needs to paint with it.
#[derive(Clone, Debug, PartialEq)]
pub struct BrushPreset {
    /// A key that never changes, for a brush Sue ships
    /// (`sue.pen.round`); empty for one a person made.
    pub id: String,
    /// What people read.
    pub name: String,
    /// The group it is listed under.
    pub group: String,
    /// Notes about it.
    pub notes: String,
    /// What lays the paint down.
    pub engine: EngineKind,
    /// The shape of the tip.
    pub tip: TipSource,
    /// The value of each parameter, by key ([`param`]), before anything
    /// drives it. A parameter without one is at its default
    /// ([`BrushRegistry::value`]).
    pub params: BTreeMap<String, f32>,
    /// What the spacing number means.
    pub spacing: Spacing,
    /// Edge softness of hard tips.
    pub antialias: AntiAlias,
    /// The line never gets thinner than this many pixels, however light
    /// the pen (0: no limit).
    pub min_width: f32,
    /// The size is measured on the screen, so the brush looks the same at
    /// any zoom.
    pub size_on_screen: bool,
    /// A pattern that breaks up the stroke.
    pub texture: Option<Texture>,
    /// What drives which parameter.
    pub dynamics: Vec<Binding>,
    /// Starting and ending.
    pub taper: Taper,
    /// How far the color wanders.
    pub color: ColorJitter,
    /// Mixing with the paint on the picture.
    pub mix: Option<Mixing>,
    /// How the stroke blends with the layer.
    pub blend: BlendMode,
    /// Removes paint instead of laying it down.
    pub erase: bool,
    /// Dabs add up within a stroke (airbrush) instead of reaching the
    /// opacity at most (pen).
    pub build_up: bool,
    /// How the pointer is steadied.
    pub input: InputSettings,
    /// Fields this build does not know (a newer Sue's, a feature's, what
    /// an importer could not map), kept as they were read and written
    /// back unchanged.
    pub extra: Extra,
}

impl Default for BrushPreset {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            group: String::new(),
            notes: String::new(),
            engine: EngineKind::Stroke,
            tip: TipSource::Round,
            params: BTreeMap::new(),
            spacing: Spacing::Auto,
            antialias: AntiAlias::Weak,
            min_width: 0.0,
            size_on_screen: false,
            texture: None,
            dynamics: Vec::new(),
            taper: Taper::default(),
            color: ColorJitter::default(),
            mix: None,
            blend: BlendMode::Normal,
            erase: false,
            build_up: false,
            input: InputSettings::default(),
            extra: Vec::new(),
        }
    }
}

impl BrushPreset {
    /// The version the encoding leads with.
    pub const FORMAT: u16 = 1;

    /// A round pen named `name` with every parameter at its default.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// The preset's own value of parameter `key`, if it holds one.
    #[must_use]
    pub fn value(&self, key: &str) -> Option<f32> {
        self.params.get(key).copied()
    }

    /// Gives parameter `key` the value `value`.
    pub fn set(&mut self, key: impl Into<String>, value: f32) -> &mut Self {
        self.params.insert(key.into(), value);
        self
    }

    /// Puts parameter `key` back at its default, and returns the value it
    /// had.
    pub fn unset(&mut self, key: &str) -> Option<f32> {
        self.params.remove(key)
    }

    /// The bindings that drive parameter `key`, in the order they apply.
    pub fn bindings<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a Binding> + 'a {
        self.dynamics
            .iter()
            .filter(move |binding| binding.param == key)
    }

    /// The bytes kept under the unknown key `key`.
    #[must_use]
    pub fn extra(&self, key: &str) -> Option<&[u8]> {
        self.extra
            .iter()
            .find(|(known, _)| known == key)
            .map(|(_, bytes)| bytes.as_slice())
    }

    /// The preset as bytes: [`Self::FORMAT`], then every field as a key
    /// and its bytes. The same preset always gives the same bytes, so
    /// their hash names it.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Writer::new();
        ComponentType::encode(self, &mut out);
        out.into_bytes()
    }

    /// A preset read from [`Self::to_bytes`].
    ///
    /// # Errors
    /// If the bytes are malformed, or written by a newer format than
    /// [`Self::FORMAT`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CodecError> {
        Self::read(&mut Reader::new(bytes))
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let format = input.varint()?;
        if format == 0 || format > u64::from(Self::FORMAT) {
            return Err(input.error(format!(
                "brush format {format} is not one this build reads (1 to {})",
                Self::FORMAT
            )));
        }
        let mut preset = Self::default();
        let mut texture = Texture::new(TextureSource::Grain);
        let mut textured = false;
        let mut mix = Mixing::new(0.0);
        let mut mixing = false;
        let mut pictures = Vec::new();
        let mut pick = Pick::default();
        let mut keep_colors = false;
        let mut images = false;
        let mut parts: Vec<(&str, &[u8])> = Vec::new();
        let mut seen: Vec<&str> = Vec::new();
        for (key, payload) in fields(input)? {
            // A field counts once: a second one of the same key is kept
            // as unknown, and so is every key this build does not know.
            let Some(known) = KEYS.iter().copied().find(|known| *known == key) else {
                preset.extra.push((key, payload.to_vec()));
                continue;
            };
            if seen.contains(&known) {
                preset.extra.push((key, payload.to_vec()));
                continue;
            }
            seen.push(known);
            if PARTS.contains(&known) {
                parts.push((known, payload));
            }
            match known {
                "id" => preset.id = text(known, payload)?,
                "name" => preset.name = text(known, payload)?,
                "group" => preset.group = text(known, payload)?,
                "notes" => preset.notes = text(known, payload)?,
                "engine" => preset.engine = EngineKind::read(known, payload)?,
                "tip" => {
                    images = match text(known, payload)?.as_str() {
                        "round" => false,
                        "images" => true,
                        other => {
                            return Err(CodecError::new(
                                format!("tip: {other:?} is no kind of tip"),
                                0,
                            ));
                        }
                    }
                }
                "tip.pictures" => {
                    pictures = field(known, payload, |input| {
                        let mut pictures = Vec::new();
                        while !input.is_empty() {
                            pictures.push(match input.str()?.as_str() {
                                "hash" => PictureRef::Hash(hash(input)?),
                                "name" => PictureRef::Named(input.str()?),
                                other => {
                                    return Err(
                                        input.error(format!("{other:?} is no kind of tip picture"))
                                    );
                                }
                            });
                        }
                        Ok(pictures)
                    })?;
                }
                "tip.pick" => pick = Pick::read(known, payload)?,
                "tip.keep_colors" => keep_colors = flag(known, payload)?,
                "params" => {
                    preset.params = field(known, payload, |input| {
                        let mut params = BTreeMap::new();
                        while !input.is_empty() {
                            let key = input.str()?;
                            params.insert(key, input.f32()?);
                        }
                        Ok(params)
                    })?;
                }
                "spacing" => preset.spacing = Spacing::read(known, payload)?,
                "antialias" => preset.antialias = AntiAlias::read(known, payload)?,
                "min_width" => preset.min_width = number(known, payload)?,
                "size_on_screen" => preset.size_on_screen = flag(known, payload)?,
                "texture.source" => {
                    textured = true;
                    texture.source = field(known, payload, |input| match input.str()?.as_str() {
                        "grain" => Ok(TextureSource::Grain),
                        "picture" => Ok(TextureSource::Picture(hash(input)?)),
                        other => Err(input.error(format!("{other:?} is no kind of texture"))),
                    })?;
                }
                "texture.scale" => texture.scale = number(known, payload)?,
                "texture.rotation" => texture.rotation = number(known, payload)?,
                "texture.mode" => texture.mode = TextureMode::read(known, payload)?,
                "dynamics" => {
                    preset.dynamics = field(known, payload, |input| {
                        let mut bindings = Vec::new();
                        while !input.is_empty() {
                            bindings.push(Binding::read(&mut input.nested()?)?);
                        }
                        Ok(bindings)
                    })?;
                }
                "taper.start" => preset.taper.start = number(known, payload)?,
                "taper.end" => preset.taper.end = number(known, payload)?,
                "taper.mode" => preset.taper.mode = TaperMode::read(known, payload)?,
                "taper.size" => preset.taper.size = flag(known, payload)?,
                "taper.opacity" => preset.taper.opacity = flag(known, payload)?,
                "taper.by_speed" => preset.taper.by_speed = flag(known, payload)?,
                "color.tip" => preset.color.tip = flag(known, payload)?,
                "color.hue" => preset.color.hue = number(known, payload)?,
                "color.saturation" => preset.color.saturation = number(known, payload)?,
                "color.luminosity" => preset.color.luminosity = number(known, payload)?,
                "color.sub" => preset.color.sub = number(known, payload)?,
                "color.stroke" => preset.color.stroke = flag(known, payload)?,
                "color.stroke_hue" => preset.color.stroke_hue = number(known, payload)?,
                "color.stroke_saturation" => {
                    preset.color.stroke_saturation = number(known, payload)?;
                }
                "color.stroke_luminosity" => {
                    preset.color.stroke_luminosity = number(known, payload)?;
                }
                "mix.amount" => {
                    mixing = true;
                    mix.amount = number(known, payload)?;
                }
                "mix.load" => mix.load = number(known, payload)?,
                "mix.pull" => mix.pull = number(known, payload)?,
                "mix.blur" => mix.blur = number(known, payload)?,
                "blend" => {
                    let name = text(known, payload)?;
                    preset.blend = BlendMode::from_name(&name).ok_or_else(|| {
                        CodecError::new(format!("blend: {name:?} is no blend mode"), 0)
                    })?;
                }
                "erase" => preset.erase = flag(known, payload)?,
                "build_up" => preset.build_up = flag(known, payload)?,
                "input.stabilization" => preset.input.stabilization = count(known, payload)?,
                "input.by_speed" => preset.input.by_speed = flag(known, payload)?,
                "input.post_correction" => preset.input.post_correction = count(known, payload)?,
                "input.flick" => preset.input.flick = flag(known, payload)?,
                "input.smart_shape" => preset.input.smart_shape = flag(known, payload)?,
                other => unreachable!("{other} is in KEYS and has no reader"),
            }
        }
        // A part without its whole (the pictures of a round tip, a scale
        // with no texture, a blur with no mixing) has no place in the
        // preset. It is kept as it was read and written back, not dropped.
        for (key, payload) in parts {
            let whole = match key.split_once('.') {
                Some(("tip", _)) => images,
                Some(("texture", _)) => textured,
                _ => mixing,
            };
            if !whole {
                preset.extra.push((key.to_owned(), payload.to_vec()));
            }
        }
        if images {
            preset.tip = TipSource::Images {
                pictures,
                pick,
                keep_colors,
            };
        }
        preset.texture = textured.then_some(texture);
        preset.mix = mixing.then_some(mix);
        Ok(preset)
    }
}

/// Every key [`BrushPreset`] writes, in the order it writes them.
const KEYS: &[&str] = &[
    "id",
    "name",
    "group",
    "notes",
    "engine",
    "tip",
    "tip.pictures",
    "tip.pick",
    "tip.keep_colors",
    "params",
    "spacing",
    "antialias",
    "min_width",
    "size_on_screen",
    "texture.source",
    "texture.scale",
    "texture.rotation",
    "texture.mode",
    "dynamics",
    "taper.start",
    "taper.end",
    "taper.mode",
    "taper.size",
    "taper.opacity",
    "taper.by_speed",
    "color.tip",
    "color.hue",
    "color.saturation",
    "color.luminosity",
    "color.sub",
    "color.stroke",
    "color.stroke_hue",
    "color.stroke_saturation",
    "color.stroke_luminosity",
    "mix.amount",
    "mix.load",
    "mix.pull",
    "mix.blur",
    "blend",
    "erase",
    "build_up",
    "input.stabilization",
    "input.by_speed",
    "input.post_correction",
    "input.flick",
    "input.smart_shape",
];

/// The keys of [`KEYS`] that say something only beside another: a picture
/// tip (`tip` is `images`), a texture (`texture.source`), a mixing
/// (`mix.amount`).
const PARTS: &[&str] = &[
    "tip.pictures",
    "tip.pick",
    "tip.keep_colors",
    "texture.scale",
    "texture.rotation",
    "texture.mode",
    "mix.load",
    "mix.pull",
    "mix.blur",
];

/// Every key [`Binding`] writes.
const BINDING_KEYS: &[&str] = &["param", "input", "curve", "min", "max", "combine", "on"];

impl ComponentType for BrushPreset {
    const NAME: &'static str = "sue.brush";
    const VERSION: u32 = Self::FORMAT as u32;

    fn encode(&self, out: &mut Writer) {
        out.varint(u64::from(Self::FORMAT));
        put_text(out, "id", &self.id);
        put_text(out, "name", &self.name);
        put_text(out, "group", &self.group);
        put_text(out, "notes", &self.notes);
        put_text(out, "engine", &self.engine.stored());
        match &self.tip {
            TipSource::Round => put_text(out, "tip", "round"),
            TipSource::Images {
                pictures,
                pick,
                keep_colors,
            } => {
                put_text(out, "tip", "images");
                put(out, "tip.pictures", |out| {
                    for picture in pictures {
                        match picture {
                            PictureRef::Hash(hash) => {
                                out.str("hash");
                                out.raw(hash);
                            }
                            PictureRef::Named(name) => {
                                out.str("name");
                                out.str(name);
                            }
                        }
                    }
                });
                put_text(out, "tip.pick", pick.name());
                put_flag(out, "tip.keep_colors", *keep_colors);
            }
        }
        put(out, "params", |out| {
            for (key, value) in &self.params {
                out.str(key);
                out.f32(*value);
            }
        });
        put_text(out, "spacing", self.spacing.name());
        put_text(out, "antialias", self.antialias.name());
        put_number(out, "min_width", self.min_width);
        put_flag(out, "size_on_screen", self.size_on_screen);
        if let Some(texture) = &self.texture {
            put(out, "texture.source", |out| match &texture.source {
                TextureSource::Grain => out.str("grain"),
                TextureSource::Picture(hash) => {
                    out.str("picture");
                    out.raw(hash);
                }
            });
            put_number(out, "texture.scale", texture.scale);
            put_number(out, "texture.rotation", texture.rotation);
            put_text(out, "texture.mode", texture.mode.name());
        }
        put(out, "dynamics", |out| {
            for binding in &self.dynamics {
                out.nested(|out| binding.write(out));
            }
        });
        put_number(out, "taper.start", self.taper.start);
        put_number(out, "taper.end", self.taper.end);
        put_text(out, "taper.mode", self.taper.mode.name());
        put_flag(out, "taper.size", self.taper.size);
        put_flag(out, "taper.opacity", self.taper.opacity);
        put_flag(out, "taper.by_speed", self.taper.by_speed);
        put_flag(out, "color.tip", self.color.tip);
        put_number(out, "color.hue", self.color.hue);
        put_number(out, "color.saturation", self.color.saturation);
        put_number(out, "color.luminosity", self.color.luminosity);
        put_number(out, "color.sub", self.color.sub);
        put_flag(out, "color.stroke", self.color.stroke);
        put_number(out, "color.stroke_hue", self.color.stroke_hue);
        put_number(out, "color.stroke_saturation", self.color.stroke_saturation);
        put_number(out, "color.stroke_luminosity", self.color.stroke_luminosity);
        if let Some(mix) = &self.mix {
            put_number(out, "mix.amount", mix.amount);
            put_number(out, "mix.load", mix.load);
            put_number(out, "mix.pull", mix.pull);
            put_number(out, "mix.blur", mix.blur);
        }
        put_text(out, "blend", self.blend.name());
        put_flag(out, "erase", self.erase);
        put_flag(out, "build_up", self.build_up);
        put_count(out, "input.stabilization", self.input.stabilization);
        put_flag(out, "input.by_speed", self.input.by_speed);
        put_count(out, "input.post_correction", self.input.post_correction);
        put_flag(out, "input.flick", self.input.flick);
        put_flag(out, "input.smart_shape", self.input.smart_shape);
        put_extra(out, &self.extra);
    }

    fn decode(_version: u32, input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Self::read(input)
    }

    fn heap_bytes(&self) -> usize {
        let extra = |extra: &Extra| {
            extra
                .iter()
                .map(|(key, bytes)| key.len() + bytes.len())
                .sum::<usize>()
        };
        self.id.len()
            + self.name.len()
            + self.group.len()
            + self.notes.len()
            + self
                .params
                .keys()
                .map(|key| key.len() + size_of::<f32>())
                .sum::<usize>()
            + self
                .dynamics
                .iter()
                .map(|binding| {
                    size_of::<Binding>()
                        + binding.param.len()
                        + binding.input.len()
                        + extra(&binding.extra)
                })
                .sum::<usize>()
            + match &self.tip {
                TipSource::Round => 0,
                TipSource::Images { pictures, .. } => pictures.len() * size_of::<PictureRef>(),
            }
            + extra(&self.extra)
    }
}

impl Binding {
    fn write(&self, out: &mut Writer) {
        put_text(out, "param", &self.param);
        put_text(out, "input", &self.input);
        put(out, "curve", |out| {
            for [x, y] in self.curve.points() {
                out.f32(*x);
                out.f32(*y);
            }
        });
        put_number(out, "min", self.min);
        put_number(out, "max", self.max);
        put_text(out, "combine", self.combine.name());
        put_flag(out, "on", self.on);
        put_extra(out, &self.extra);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let mut binding = Self::new("", "");
        let mut seen: Vec<&str> = Vec::new();
        for (key, payload) in fields(input)? {
            let Some(known) = BINDING_KEYS.iter().copied().find(|known| *known == key) else {
                binding.extra.push((key, payload.to_vec()));
                continue;
            };
            if seen.contains(&known) {
                binding.extra.push((key, payload.to_vec()));
                continue;
            }
            seen.push(known);
            match known {
                "param" => binding.param = text(known, payload)?,
                "input" => binding.input = text(known, payload)?,
                "curve" => {
                    binding.curve = field(known, payload, |input| {
                        let mut points = Vec::new();
                        while !input.is_empty() {
                            points.push([input.f32()?, input.f32()?]);
                        }
                        Ok(Response::new(&points))
                    })?;
                }
                "min" => binding.min = number(known, payload)?,
                "max" => binding.max = number(known, payload)?,
                "combine" => binding.combine = Combine::read(known, payload)?,
                "on" => binding.on = flag(known, payload)?,
                other => unreachable!("{other} is in BINDING_KEYS and has no reader"),
            }
        }
        Ok(binding)
    }
}

/// Every field left in `input`: its key and its bytes.
fn fields<'a>(input: &mut Reader<'a>) -> Result<Vec<(String, &'a [u8])>, CodecError> {
    let mut out = Vec::new();
    while !input.is_empty() {
        let key = input.str()?;
        out.push((key, input.bytes()?));
    }
    Ok(out)
}

/// Field `key` read from all of `payload`, with the key in any error.
fn field<T>(
    key: &str,
    payload: &[u8],
    read: impl FnOnce(&mut Reader<'_>) -> Result<T, CodecError>,
) -> Result<T, CodecError> {
    let mut input = Reader::new(payload);
    let value = read(&mut input).and_then(|value| {
        if input.is_empty() {
            Ok(value)
        } else {
            Err(input.error(format!("{} bytes too many", input.remaining())))
        }
    });
    value.map_err(|error| CodecError::new(format!("{key}: {}", error.message), error.offset))
}

fn text(key: &str, payload: &[u8]) -> Result<String, CodecError> {
    String::from_utf8(payload.to_vec())
        .map_err(|_| CodecError::new(format!("{key}: text is not UTF-8"), 0))
}

fn number(key: &str, payload: &[u8]) -> Result<f32, CodecError> {
    field(key, payload, |input| input.f32())
}

fn flag(key: &str, payload: &[u8]) -> Result<bool, CodecError> {
    field(key, payload, |input| input.bool())
}

fn count(key: &str, payload: &[u8]) -> Result<u32, CodecError> {
    field(key, payload, |input| {
        u32::try_from(input.varint()?).map_err(|_| input.error("number overflows"))
    })
}

fn hash(input: &mut Reader<'_>) -> Result<ContentHash, CodecError> {
    let mut hash = [0; 32];
    hash.copy_from_slice(input.raw(32)?);
    Ok(hash)
}

fn put(out: &mut Writer, key: &str, write: impl FnOnce(&mut Writer)) {
    out.str(key);
    out.nested(write);
}

fn put_text(out: &mut Writer, key: &str, text: &str) {
    out.str(key);
    out.str(text);
}

fn put_number(out: &mut Writer, key: &str, value: f32) {
    put(out, key, |out| out.f32(value));
}

fn put_flag(out: &mut Writer, key: &str, value: bool) {
    put(out, key, |out| out.bool(value));
}

fn put_count(out: &mut Writer, key: &str, value: u32) {
    put(out, key, |out| out.varint(u64::from(value)));
}

fn put_extra(out: &mut Writer, extra: &Extra) {
    for (key, bytes) in extra {
        out.str(key);
        out.bytes(bytes);
    }
}

/// The keys of a brush saved as text by Sue 0.0.75 and older.
const LEGACY_KEYS: &[&str] = &[
    "size",
    "opacity",
    "hardness",
    "shape",
    "aa",
    "spacing",
    "build_up",
    "flow",
    "grain",
    "screen_size",
    "at_least_one",
    "eraser",
    "taper_start",
    "taper_end",
    "taper_mode",
    "taper_size",
    "taper_opacity",
    "taper_by_speed",
    "mix",
    "follow",
    "follow_tilt",
    "angle",
    "scatter",
    "size_jitter",
    "turn_jitter",
    "ribbon",
    "keep_colors",
    "shape_count",
    "jitter.tip",
    "jitter.hue",
    "jitter.saturation",
    "jitter.value",
    "jitter.sub",
    "jitter.stroke",
    "jitter.stroke_hue",
    "jitter.stroke_saturation",
    "jitter.stroke_value",
];

/// The two settings an old brush's pen could drive, as its text names
/// them, and the parameter each is now.
const LEGACY_DYNAMICS: [(&str, &str); 2] =
    [("size_by", param::SIZE), ("opacity_by", param::OPACITY)];

/// What could drive them, as the text names it, and the input each is
/// now.
const LEGACY_INPUTS: [(&str, &str); 4] = [
    ("pressure", input::PRESSURE),
    ("velocity", input::SPEED),
    ("tilt", input::TILT),
    ("random", input::RANDOM),
];

impl BrushPreset {
    /// The prefix of the [`Self::extra`] keys that hold what an old
    /// brush's text said and the model has no place for.
    pub const LEGACY: &'static str = "legacy.";

    /// A brush saved as text by Sue 0.0.75 and older: `key=value` pairs
    /// joined by semicolons, as vector lines and custom sub tools kept
    /// their brushes. It reads the text as that Sue did: a key that is
    /// missing or does not parse keeps the old default (a size of 8
    /// driven by pressure down to 0.15, and so on), the last of a
    /// repeated key counts, and a flag is on only as `1`.
    ///
    /// Where each key goes:
    ///
    /// - `size`, `opacity`, `hardness`, `spacing`, `flow`, `angle` and
    ///   `scatter` are the parameters of the same names, and the spacing
    ///   is a [`Spacing::Ratio`].
    /// - `size_by.*` and `opacity_by.*` become bindings of size and
    ///   opacity to pressure, speed (`velocity`), tilt and random, each
    ///   multiplied in from its minimum (held within 0 to 1, as the old
    ///   engine held it) to 1. One that is off is kept, off, when it
    ///   holds a minimum or a curve.
    /// - `size_jitter` is a binding of size to random from 1 down to
    ///   1 less the jitter; `turn_jitter` one of angle to random, added,
    ///   half a turn each way at 1; `follow` and `follow_tilt` bindings
    ///   of angle to the line's and the pen's direction, added, a full
    ///   turn. The old engine turned a dab with the pen's lean when
    ///   `follow_tilt` was on and the pen leaned, else with the line
    ///   when `follow` was on.
    /// - `grain` above 0 is a [`TextureSource::Grain`] texture of that
    ///   strength; `mix` above 0 is a [`Mixing`] of that amount.
    /// - `shape` names the tip's picture ([`PictureRef::Named`]: the old
    ///   text has no more than the name). With `ribbon` the engine is a
    ///   ribbon; else a brush that builds up, mixes or has a picture
    ///   dabs, and every other is one swept stroke.
    /// - `aa`, `taper_*`, `jitter.*`, `eraser`, `build_up`,
    ///   `screen_size` and `at_least_one` (a least width of 1) have
    ///   fields of their own.
    /// - What has no place is kept in [`Self::extra`] under
    ///   [`Self::LEGACY`] and its key, as the text of its value: a
    ///   `shape_count` other than 1 (the pictures after the first were
    ///   numbered, not named), `ribbon` and `keep_colors` on a brush
    ///   without a picture, and keys that Sue did not know.
    ///
    /// One difference from that Sue: a number that is not finite is
    /// read as missing.
    #[must_use]
    pub fn from_legacy_text(text: &str) -> Self {
        let pairs: Vec<(&str, &str)> = text
            .split(';')
            .filter_map(|pair| pair.split_once('='))
            .collect();
        let get = |key: &str| {
            pairs
                .iter()
                .rev()
                .find(|(known, _)| *known == key)
                .map(|(_, value)| *value)
        };
        let number = |key: &str, default: f32| {
            get(key)
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|value| value.is_finite())
                .unwrap_or(default)
        };
        let flag = |key: &str, default: bool| get(key).map_or(default, |value| value == "1");
        let curve = |key: &str| {
            get(key)
                .and_then(|value| value.parse::<Response>().ok())
                .unwrap_or(Response::LINEAR)
        };
        let index = |key: &str| get(key).and_then(|value| value.parse::<u8>().ok());

        let mut preset = Self::default();
        let mut leftover = |key: &str, value: &str| {
            preset
                .extra
                .push((format!("{}{key}", Self::LEGACY), value.as_bytes().to_vec()));
        };

        // The tip.
        let shape = get("shape").filter(|name| !name.is_empty());
        let keep_colors = flag("keep_colors", false);
        let ribbon = flag("ribbon", false);
        let tip = match shape {
            Some(name) => TipSource::Images {
                pictures: vec![PictureRef::Named(name.to_owned())],
                pick: Pick::Random,
                keep_colors,
            },
            None => {
                if ribbon {
                    leftover("ribbon", "1");
                }
                if keep_colors {
                    leftover("keep_colors", "1");
                }
                TipSource::Round
            }
        };
        let shape_count = get("shape_count")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(1);
        if shape_count != 1 {
            leftover("shape_count", &shape_count.to_string());
        }
        // Keys that Sue did not know, the last of each.
        for (at, (key, value)) in pairs.iter().enumerate() {
            let known = LEGACY_KEYS.contains(key)
                || LEGACY_DYNAMICS.iter().any(|(name, _)| {
                    key.strip_prefix(name)
                        .and_then(|rest| rest.strip_prefix('.'))
                        .is_some_and(|rest| {
                            LEGACY_INPUTS.iter().any(|(input, _)| {
                                rest.strip_prefix(input).is_some_and(|end| {
                                    matches!(end, "" | "_min")
                                        || (end == "_curve" && *input != "random")
                                })
                            })
                        })
                });
            let last = pairs[at + 1..].iter().all(|(later, _)| later != key);
            if !known && last {
                leftover(key, value);
            }
        }

        // The numbers a pen can drive.
        let mix = number("mix", 0.0);
        let grain = number("grain", 0.0);
        let build_up = flag("build_up", false);
        preset.set(param::SIZE, number("size", 8.0));
        preset.set(param::OPACITY, number("opacity", 1.0));
        preset.set(param::HARDNESS, number("hardness", 1.0));
        preset.set(param::SPACING, number("spacing", 0.06));
        preset.set(param::FLOW, number("flow", 1.0));
        preset.set(param::ANGLE, number("angle", 0.0));
        preset.set(param::SCATTER, number("scatter", 0.0));
        preset.spacing = Spacing::Ratio;
        if grain > 0.0 {
            preset.texture = Some(Texture::new(TextureSource::Grain));
            preset.set(param::TEXTURE_STRENGTH, grain);
        }
        if mix > 0.0 {
            preset.mix = Some(Mixing::new(mix));
        }

        // What drives them.
        for (name, param) in LEGACY_DYNAMICS {
            for (legacy, input) in LEGACY_INPUTS {
                // Size followed pressure down to 0.15 unless the text
                // said otherwise; everything else was off.
                let default = name == "size_by" && legacy == "pressure";
                let on = flag(&format!("{name}.{legacy}"), default);
                // The old engine held a minimum within 0 to 1 as it
                // used it.
                let min = number(
                    &format!("{name}.{legacy}_min"),
                    if default { 0.15 } else { 0.0 },
                )
                .clamp(0.0, 1.0);
                // Chance had no curve.
                let curve = if legacy == "random" {
                    Response::LINEAR
                } else {
                    curve(&format!("{name}.{legacy}_curve"))
                };
                if on || min != 0.0 || !curve.is_linear() {
                    preset.dynamics.push(
                        Binding::new(param, input)
                            .range(min, 1.0)
                            .curve(curve)
                            .switched(on),
                    );
                }
            }
        }
        let size_jitter = number("size_jitter", 0.0);
        if size_jitter > 0.0 {
            preset.dynamics.push(
                Binding::new(param::SIZE, input::RANDOM).range(1.0, 1.0 - size_jitter.min(1.0)),
            );
        }
        if flag("follow", false) {
            preset.dynamics.push(
                Binding::new(param::ANGLE, input::DIRECTION)
                    .range(0.0, 360.0)
                    .added(),
            );
        }
        if flag("follow_tilt", false) {
            preset.dynamics.push(
                Binding::new(param::ANGLE, input::TILT_DIRECTION)
                    .range(0.0, 360.0)
                    .added(),
            );
        }
        let turn_jitter = number("turn_jitter", 0.0);
        if turn_jitter > 0.0 {
            let reach = turn_jitter.min(1.0) * 180.0;
            preset.dynamics.push(
                Binding::new(param::ANGLE, input::RANDOM)
                    .range(-reach, reach)
                    .added(),
            );
        }

        // The rest.
        preset.engine = match &tip {
            TipSource::Images { .. } if ribbon => EngineKind::Ribbon,
            TipSource::Images { .. } => EngineKind::Dab,
            TipSource::Round if build_up || mix > 0.0 => EngineKind::Dab,
            TipSource::Round => EngineKind::Stroke,
        };
        preset.tip = tip;
        preset.antialias = match index("aa") {
            None | Some(1) => AntiAlias::Weak,
            Some(0) => AntiAlias::None,
            Some(2) => AntiAlias::Medium,
            Some(_) => AntiAlias::Strong,
        };
        preset.min_width = if flag("at_least_one", false) {
            1.0
        } else {
            0.0
        };
        preset.size_on_screen = flag("screen_size", false);
        preset.taper = Taper {
            start: number("taper_start", 0.0),
            end: number("taper_end", 0.0),
            mode: match index("taper_mode") {
                None | Some(0) => TaperMode::Length,
                Some(1) => TaperMode::Percent,
                Some(_) => TaperMode::Fade,
            },
            size: flag("taper_size", true),
            opacity: flag("taper_opacity", false),
            by_speed: flag("taper_by_speed", false),
        };
        preset.color = ColorJitter {
            tip: flag("jitter.tip", false),
            hue: number("jitter.hue", 0.0),
            saturation: number("jitter.saturation", 0.0),
            luminosity: number("jitter.value", 0.0),
            sub: number("jitter.sub", 0.0),
            stroke: flag("jitter.stroke", false),
            stroke_hue: number("jitter.stroke_hue", 0.0),
            stroke_saturation: number("jitter.stroke_saturation", 0.0),
            stroke_luminosity: number("jitter.stroke_value", 0.0),
        };
        preset.erase = flag("eraser", false);
        preset.build_up = build_up;
        preset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A preset with something in every field.
    fn full() -> BrushPreset {
        let mut preset = BrushPreset::new("Dry bristle");
        preset.id = "test.dry_bristle".to_owned();
        preset.group = "Paint".to_owned();
        preset.notes = "Leans, drags and breaks up.".to_owned();
        preset.engine = EngineKind::Blend("smudge".to_owned());
        preset.tip = TipSource::Images {
            pictures: vec![
                PictureRef::Hash([7; 32]),
                PictureRef::Named("Bristle".to_owned()),
            ],
            pick: Pick::Sequence,
            keep_colors: true,
        };
        preset
            .set(param::SIZE, 42.5)
            .set(param::OPACITY, 0.8)
            .set(param::ASPECT, 0.4)
            .set("vendor.bristles", 12.0);
        preset.spacing = Spacing::Fixed;
        preset.antialias = AntiAlias::Strong;
        preset.min_width = 1.0;
        preset.size_on_screen = true;
        preset.texture = Some(Texture {
            source: TextureSource::Picture([9; 32]),
            scale: 2.0,
            rotation: 30.0,
            mode: TextureMode::Subtract,
        });
        preset.dynamics = vec![
            Binding::new(param::SIZE, input::PRESSURE)
                .range(0.15, 1.0)
                .curve(Response::new(&[[0.0, 0.05], [0.4, 0.2], [1.0, 1.0]])),
            Binding::new(param::ANGLE, input::TILT_DIRECTION)
                .range(0.0, 360.0)
                .added(),
            Binding::new("vendor.bristles", "vendor.barrel_button").switched(false),
        ];
        preset.taper = Taper {
            start: 20.0,
            end: 35.0,
            mode: TaperMode::Percent,
            size: false,
            opacity: true,
            by_speed: true,
        };
        preset.color = ColorJitter {
            tip: true,
            hue: 0.1,
            saturation: 0.2,
            luminosity: 0.3,
            sub: 0.4,
            stroke: true,
            stroke_hue: 0.5,
            stroke_saturation: 0.6,
            stroke_luminosity: 0.7,
        };
        preset.mix = Some(Mixing {
            amount: 0.6,
            load: 0.5,
            pull: 0.25,
            blur: 0.125,
        });
        preset.blend = BlendMode::Multiply;
        preset.erase = true;
        preset.build_up = true;
        preset.input = InputSettings {
            stabilization: 15,
            by_speed: true,
            post_correction: 4,
            flick: true,
            smart_shape: true,
        };
        preset
    }

    /// `bytes` with one more field at their end.
    fn with_field(mut bytes: Vec<u8>, key: &str, payload: &[u8]) -> Vec<u8> {
        let mut out = Writer::new();
        out.str(key);
        out.bytes(payload);
        bytes.extend_from_slice(out.as_bytes());
        bytes
    }

    #[test]
    fn a_preset_round_trips() {
        for preset in [
            BrushPreset::default(),
            BrushPreset::new("Pen"),
            full(),
            BrushPreset::from_legacy_text("size=12;grain=0.35;mix=0.5;shape=Dot;ribbon=1"),
        ] {
            let bytes = preset.to_bytes();
            let back = BrushPreset::from_bytes(&bytes).unwrap();
            assert_eq!(back, preset);
            assert_eq!(back.to_bytes(), bytes, "the same preset, the same bytes");
        }
    }

    #[test]
    fn the_encoding_leads_with_its_version_and_refuses_a_newer_one() {
        let mut bytes = full().to_bytes();
        assert_eq!(bytes[0], BrushPreset::FORMAT as u8);
        assert_eq!(BrushPreset::VERSION, u32::from(BrushPreset::FORMAT));
        bytes[0] = BrushPreset::FORMAT as u8 + 1;
        let error = BrushPreset::from_bytes(&bytes).unwrap_err();
        assert!(error.message.contains("brush format 2"), "{error}");
        bytes[0] = 0;
        assert!(BrushPreset::from_bytes(&bytes).is_err());
        assert!(BrushPreset::from_bytes(&[]).is_err());
    }

    #[test]
    fn unknown_fields_are_kept_as_they_were() {
        // A field a newer Sue added, between two this build knows.
        let preset = full();
        let mut out = Writer::new();
        out.varint(u64::from(BrushPreset::FORMAT));
        put_text(&mut out, "name", "Newer");
        out.str("bristles.count");
        out.bytes(&[1, 2, 3, 0, 255]);
        put_number(&mut out, "min_width", 2.0);
        out.str("empty");
        out.bytes(&[]);
        let read = BrushPreset::from_bytes(out.as_bytes()).unwrap();
        assert_eq!(read.name, "Newer");
        assert_eq!(read.min_width, 2.0);
        assert_eq!(read.extra("bristles.count"), Some(&[1, 2, 3, 0, 255][..]));
        assert_eq!(read.extra("empty"), Some(&[][..]));
        assert_eq!(read.extra("nothing"), None);

        // Saved and loaded again, and edited in between, they are there.
        let mut edited = BrushPreset::from_bytes(&read.to_bytes()).unwrap();
        assert_eq!(edited, read);
        edited.set(param::SIZE, 3.0);
        let again = BrushPreset::from_bytes(&edited.to_bytes()).unwrap();
        assert_eq!(again.extra, read.extra);
        assert_eq!(again.value(param::SIZE), Some(3.0));

        // On a full preset too, in the order they came.
        let bytes = with_field(
            with_field(preset.to_bytes(), "z.later", b"one"),
            "a.sooner",
            b"two",
        );
        let read = BrushPreset::from_bytes(&bytes).unwrap();
        assert_eq!(
            read.extra,
            vec![
                ("z.later".to_owned(), b"one".to_vec()),
                ("a.sooner".to_owned(), b"two".to_vec()),
            ]
        );
        assert_eq!(read.to_bytes(), bytes);
        assert_eq!(
            BrushPreset {
                extra: Vec::new(),
                ..read
            },
            preset
        );
    }

    #[test]
    fn a_field_counts_once_and_a_second_one_is_kept() {
        let bytes = with_field(BrushPreset::new("First").to_bytes(), "name", b"Second");
        let read = BrushPreset::from_bytes(&bytes).unwrap();
        assert_eq!(read.name, "First");
        assert_eq!(read.extra("name"), Some(&b"Second"[..]));
        assert_eq!(read.to_bytes(), bytes);
    }

    #[test]
    fn a_binding_keeps_its_unknown_fields() {
        let mut binding = Binding::new(param::FLOW, input::SPEED).range(0.2, 0.9);
        binding.extra.push(("smoothing".to_owned(), vec![4, 2]));
        let mut preset = BrushPreset::new("Bound");
        preset.dynamics.push(binding.clone());
        let read = BrushPreset::from_bytes(&preset.to_bytes()).unwrap();
        assert_eq!(read.dynamics, vec![binding]);
        assert_eq!(read.to_bytes(), preset.to_bytes());
    }

    #[test]
    fn a_missing_field_is_at_its_default() {
        // Only the version: what a later format may leave out.
        let read = BrushPreset::from_bytes(&[BrushPreset::FORMAT as u8]).unwrap();
        assert_eq!(read, BrushPreset::default());
        // A texture and a mixing are there when their first field is.
        let mut out = Writer::new();
        out.varint(u64::from(BrushPreset::FORMAT));
        put(&mut out, "texture.source", |out| out.str("grain"));
        put_number(&mut out, "mix.amount", 0.5);
        let read = BrushPreset::from_bytes(out.as_bytes()).unwrap();
        assert_eq!(read.texture, Some(Texture::new(TextureSource::Grain)));
        assert_eq!(read.mix, Some(Mixing::new(0.5)));
    }

    #[test]
    fn a_part_without_its_whole_is_kept_and_written_back() {
        // The scale of a texture that is not there, the blur of a mixing
        // that is not there, and the pictures' colors of a round tip.
        let mut out = Writer::new();
        out.varint(u64::from(BrushPreset::FORMAT));
        put_number(&mut out, "texture.scale", 2.0);
        put_number(&mut out, "mix.blur", 0.5);
        put_flag(&mut out, "tip.keep_colors", true);
        put_text(&mut out, "tip", "round");
        let read = BrushPreset::from_bytes(out.as_bytes()).unwrap();
        assert_eq!(read.texture, None);
        assert_eq!(read.mix, None);
        assert_eq!(read.tip, TipSource::Round);
        let mut scale = Writer::new();
        scale.f32(2.0);
        assert_eq!(read.extra("texture.scale"), Some(scale.as_bytes()));
        assert!(read.extra("mix.blur").is_some());
        assert!(read.extra("tip.keep_colors").is_some());
        assert_eq!(read.extra.len(), 3);
        let bytes = read.to_bytes();
        let again = BrushPreset::from_bytes(&bytes).unwrap();
        assert_eq!(again, read);
        assert_eq!(again.to_bytes(), bytes);

        // Beside its whole a part is a field, wherever the whole comes.
        let mut out = Writer::new();
        out.varint(u64::from(BrushPreset::FORMAT));
        put_number(&mut out, "texture.scale", 2.0);
        put_flag(&mut out, "tip.keep_colors", true);
        put_text(&mut out, "tip", "images");
        put(&mut out, "texture.source", |out| out.str("grain"));
        let read = BrushPreset::from_bytes(out.as_bytes()).unwrap();
        assert!(read.extra.is_empty());
        assert_eq!(
            read.texture.as_ref().map(|texture| texture.scale),
            Some(2.0)
        );
        assert_eq!(
            read.tip,
            TipSource::Images {
                pictures: Vec::new(),
                pick: Pick::Random,
                keep_colors: true,
            }
        );
    }

    #[test]
    fn malformed_fields_are_errors_that_name_the_field() {
        let version = BrushPreset::FORMAT as u8;
        let cases: [(&str, &[u8], &str); 9] = [
            ("min_width", &[1, 2, 3], "min_width"),
            ("erase", &[2], "erase"),
            ("erase", &[1, 1], "erase: 1 bytes too many"),
            ("engine", b"laser", "is no brush engine"),
            ("tip", b"square", "is no kind of tip"),
            ("spacing", b"wide", "is no kind of spacing"),
            ("blend", b"sparkle", "is no blend mode"),
            ("name", &[0xFF, 0xFE], "name: text is not UTF-8"),
            (
                "tip.pictures",
                &[4, b'h', b'a', b's', b'h', 1],
                "tip.pictures",
            ),
        ];
        for (key, payload, message) in cases {
            let error =
                BrushPreset::from_bytes(&with_field(vec![version], key, payload)).expect_err(key);
            assert!(error.message.contains(message), "{key}: {error}");
        }
        // A field cut short.
        let bytes = full().to_bytes();
        assert!(BrushPreset::from_bytes(&bytes[..bytes.len() - 1]).is_err());
    }

    #[test]
    fn every_choice_reads_back_by_its_name() {
        macro_rules! check {
            ($($type:ident),+) => {$(
                for choice in $type::ALL {
                    assert_eq!($type::from_name(choice.name()), Some(*choice));
                }
                assert_eq!($type::from_name("no such name"), None);
            )+};
        }
        check!(Pick, Spacing, AntiAlias, TextureMode, Combine, TaperMode);
        for engine in [
            EngineKind::Stroke,
            EngineKind::Dab,
            EngineKind::Ribbon,
            EngineKind::Blend("blur".to_owned()),
            EngineKind::Blend(String::new()),
        ] {
            let stored = engine.stored();
            assert_eq!(
                EngineKind::read("engine", stored.as_bytes()).unwrap(),
                engine
            );
        }
    }

    #[test]
    fn the_registry_knows_the_standard_keys_and_takes_new_ones() {
        let mut registry = BrushRegistry::standard();
        for key in [
            param::SIZE,
            param::OPACITY,
            param::FLOW,
            param::DENSITY,
            param::HARDNESS,
            param::ASPECT,
            param::ANGLE,
            param::SPACING,
            param::SCATTER,
            param::TEXTURE_STRENGTH,
            param::HUE,
            param::SATURATION,
            param::LUMINOSITY,
        ] {
            let def = registry.param(key).expect(key);
            assert!(def.min <= def.default && def.default <= def.max, "{key}");
        }
        for key in [
            input::PRESSURE,
            input::SPEED,
            input::TILT,
            input::TILT_DIRECTION,
            input::TWIST,
            input::DIRECTION,
            input::DISTANCE,
            input::TIME,
            input::RANDOM,
            input::STROKE_RANDOM,
        ] {
            assert!(registry.input(key).is_some(), "{key}");
        }
        assert_eq!(registry.params().count(), 13);
        assert_eq!(registry.inputs().count(), 10);
        assert!(registry.input(input::TWIST).unwrap().wraps);
        assert!(!registry.input(input::PRESSURE).unwrap().wraps);

        // A preset's own value, else the default, else nothing.
        let mut preset = BrushPreset::new("Pen");
        assert_eq!(registry.value(&preset, param::SIZE), Some(8.0));
        preset.set(param::SIZE, 20.0);
        assert_eq!(registry.value(&preset, param::SIZE), Some(20.0));
        assert_eq!(preset.unset(param::SIZE), Some(20.0));
        assert_eq!(registry.value(&preset, param::SIZE), Some(8.0));
        assert_eq!(registry.value(&preset, "vendor.bristles"), None);
        assert_eq!(registry.param(param::OPACITY).unwrap().clamp(3.0), 1.0);

        // A feature's own keys.
        registry
            .register_param(ParamDef::new("vendor.bristles", Unit::Share, 0.5, 0.0, 1.0))
            .register_input(InputDef::new("vendor.barrel_button"));
        assert_eq!(registry.value(&preset, "vendor.bristles"), Some(0.5));
        assert!(registry.input("vendor.barrel_button").is_some());
        // A preset keeps keys no registry knows.
        preset.set("other.thing", 2.0);
        assert_eq!(registry.value(&preset, "other.thing"), Some(2.0));
    }

    #[test]
    fn any_parameter_follows_any_input() {
        let mut preset = BrushPreset::new("Bound");
        preset.dynamics = vec![
            Binding::new(param::SIZE, input::PRESSURE).range(0.2, 1.0),
            Binding::new(param::HARDNESS, input::TILT),
            Binding::new(param::SIZE, input::SPEED).range(1.0, 0.5),
            Binding::new(param::ANGLE, input::DIRECTION)
                .range(0.0, 360.0)
                .added(),
            Binding::new(param::SIZE, input::TWIST).switched(false),
        ];
        assert_eq!(preset.bindings(param::SIZE).count(), 3);
        assert_eq!(preset.bindings(param::HARDNESS).count(), 1);
        assert_eq!(preset.bindings(param::FLOW).count(), 0);

        // Size 10 at half pressure and full speed: both multiply in, and
        // the one that is off does nothing.
        let size = preset.bindings(param::SIZE).fold(10.0, |value, binding| {
            match binding.input.as_str() {
                "pressure" => binding.apply(value, 0.5),
                "speed" => binding.apply(value, 1.0),
                _ => binding.apply(value, 0.9),
            }
        });
        assert!((size - 10.0 * 0.6 * 0.5).abs() < 1e-5, "{size}");
        let angle = preset.dynamics[3].apply(15.0, 0.25);
        assert!((angle - 105.0).abs() < 1e-4, "{angle}");
        // Inputs are shares: outside 0 to 1 they stop at the ends.
        assert_eq!(preset.dynamics[0].amount(-3.0), 0.2);
        assert_eq!(preset.dynamics[0].amount(7.0), 1.0);
    }

    #[test]
    fn an_old_brush_without_text_is_the_old_default() {
        let preset = BrushPreset::from_legacy_text("");
        let mut expected = BrushPreset::default();
        expected
            .set(param::SIZE, 8.0)
            .set(param::OPACITY, 1.0)
            .set(param::HARDNESS, 1.0)
            .set(param::SPACING, 0.06)
            .set(param::FLOW, 1.0)
            .set(param::ANGLE, 0.0)
            .set(param::SCATTER, 0.0);
        expected.spacing = Spacing::Ratio;
        expected.dynamics = vec![Binding::new(param::SIZE, input::PRESSURE).range(0.15, 1.0)];
        assert_eq!(preset, expected);
    }

    #[test]
    fn old_text_is_read_as_the_old_reader_read_it() {
        // A value that does not parse keeps the default, the last of a
        // repeated key counts, a flag is on only as "1", and a pair
        // without "=" is passed over.
        let preset = BrushPreset::from_legacy_text(
            "size=12;size=nope;opacity=0.5;opacity=0.25;eraser=true;build_up=1;junk;aa=9;\
             taper_mode=7;hardness=inf;flow=NaN",
        );
        assert_eq!(preset.value(param::SIZE), Some(8.0));
        assert_eq!(preset.value(param::OPACITY), Some(0.25));
        assert_eq!(preset.value(param::HARDNESS), Some(1.0));
        assert_eq!(preset.value(param::FLOW), Some(1.0));
        assert!(!preset.erase);
        assert!(preset.build_up);
        assert_eq!(preset.engine, EngineKind::Dab);
        assert_eq!(preset.antialias, AntiAlias::Strong);
        assert_eq!(preset.taper.mode, TaperMode::Fade);
        assert!(preset.extra.is_empty());
    }

    #[test]
    fn old_dynamics_become_bindings_that_give_the_old_factor() {
        let preset = BrushPreset::from_legacy_text(
            "size_by.pressure=1;size_by.pressure_min=0.2;size_by.pressure_curve=0,0 0.5,0.2 1,1;\
             size_by.velocity=1;size_by.velocity_min=0.4;size_by.tilt=0;size_by.tilt_min=0.3;\
             size_by.random=1;size_by.random_min=0.5;opacity_by.pressure=1;\
             opacity_by.tilt_curve=0,0.5 1,1",
        );
        let soft: Response = "0,0 0.5,0.2 1,1".parse().unwrap();
        assert_eq!(
            preset.dynamics,
            vec![
                Binding::new(param::SIZE, input::PRESSURE)
                    .range(0.2, 1.0)
                    .curve(soft),
                Binding::new(param::SIZE, input::SPEED).range(0.4, 1.0),
                Binding::new(param::SIZE, input::TILT)
                    .range(0.3, 1.0)
                    .switched(false),
                Binding::new(param::SIZE, input::RANDOM).range(0.5, 1.0),
                Binding::new(param::OPACITY, input::PRESSURE),
                Binding::new(param::OPACITY, input::TILT)
                    .curve("0,0.5 1,1".parse().unwrap())
                    .switched(false),
            ]
        );
        // The old engine: each input that is on scales the setting
        // between its minimum and full through its curve, and the
        // inputs multiply.
        let (pressure, speed, tilt, chance) = (0.5_f32, 0.25_f32, 0.9_f32, 0.5_f32);
        let old = (0.2 + 0.8 * soft.eval(pressure)) * (0.4 + 0.6 * speed) * (0.5 + 0.5 * chance);
        let new =
            preset
                .bindings(param::SIZE)
                .fold(1.0, |value, binding| match binding.input.as_str() {
                    "pressure" => binding.apply(value, pressure),
                    "speed" => binding.apply(value, speed),
                    "tilt" => binding.apply(value, tilt),
                    _ => binding.apply(value, chance),
                });
        assert!((old - new).abs() < 1e-6, "{old} {new}");

        // A minimum outside 0 to 1 counts as the old engine counted it.
        let preset = BrushPreset::from_legacy_text(
            "size_by.pressure_min=3;opacity_by.velocity=1;opacity_by.velocity_min=-2",
        );
        assert_eq!(
            preset.dynamics,
            vec![
                Binding::new(param::SIZE, input::PRESSURE).range(1.0, 1.0),
                Binding::new(param::OPACITY, input::SPEED),
            ]
        );
    }

    #[test]
    fn old_decoration_settings_become_bindings() {
        let preset = BrushPreset::from_legacy_text(
            "shape=Leaf;follow=1;follow_tilt=1;size_jitter=0.35;turn_jitter=0.2;scatter=0.35;\
             size_by.pressure=0;size_by.pressure_min=0;keep_colors=1",
        );
        assert_eq!(preset.engine, EngineKind::Dab);
        assert_eq!(
            preset.tip,
            TipSource::Images {
                pictures: vec![PictureRef::Named("Leaf".to_owned())],
                pick: Pick::Random,
                keep_colors: true,
            }
        );
        assert_eq!(preset.value(param::SCATTER), Some(0.35));
        assert_eq!(
            preset.dynamics,
            vec![
                Binding::new(param::SIZE, input::RANDOM).range(1.0, 1.0 - 0.35),
                Binding::new(param::ANGLE, input::DIRECTION)
                    .range(0.0, 360.0)
                    .added(),
                Binding::new(param::ANGLE, input::TILT_DIRECTION)
                    .range(0.0, 360.0)
                    .added(),
                Binding::new(param::ANGLE, input::RANDOM)
                    .range(-36.0, 36.0)
                    .added(),
            ]
        );
        // The old engine: the radius times 1 less the jitter times the
        // roll, and the turn by the roll less a half, times the jitter,
        // of a full turn.
        for roll in [0.0_f32, 0.3, 1.0] {
            let size = preset.dynamics[0].apply(10.0, roll);
            assert!((size - 10.0 * (1.0 - 0.35 * roll)).abs() < 1e-5);
            let turn = preset.dynamics[3].apply(0.0, roll);
            assert!((turn - (roll - 0.5) * 0.2 * 360.0).abs() < 1e-4);
        }
    }

    #[test]
    fn what_old_text_says_and_the_model_cannot_hold_is_kept() {
        let preset = BrushPreset::from_legacy_text(
            "shape=Petals;shape_count=3;wetness=0.4;wetness=0.6;size_by.pressure_curve=0,0 1,1;\
             size_by.random_curve=0,0 1,1;size_by.squeeze=1",
        );
        assert_eq!(preset.extra("legacy.shape_count"), Some(&b"3"[..]));
        assert_eq!(preset.extra("legacy.wetness"), Some(&b"0.6"[..]));
        assert_eq!(
            preset.extra("legacy.size_by.random_curve"),
            Some(&b"0,0 1,1"[..])
        );
        assert_eq!(preset.extra("legacy.size_by.squeeze"), Some(&b"1"[..]));
        assert_eq!(preset.extra.len(), 4);

        // A ribbon and kept colors need a picture.
        let plain = BrushPreset::from_legacy_text("ribbon=1;keep_colors=1;shape=");
        assert_eq!(plain.tip, TipSource::Round);
        assert_eq!(plain.engine, EngineKind::Stroke);
        assert_eq!(plain.extra("legacy.ribbon"), Some(&b"1"[..]));
        assert_eq!(plain.extra("legacy.keep_colors"), Some(&b"1"[..]));
        let ribbon = BrushPreset::from_legacy_text("ribbon=1;shape=Rope");
        assert_eq!(ribbon.engine, EngineKind::Ribbon);
        assert!(ribbon.extra.is_empty());
    }

    #[test]
    fn the_component_is_registered_as_sue_brush() {
        assert_eq!(BrushPreset::NAME, "sue.brush");
        let registry = crate::registry();
        assert!(registry.contains("sue.brush"));
        let preset = full();
        let component = registry
            .decode("sue.brush", BrushPreset::VERSION, &preset.to_bytes())
            .unwrap();
        assert_eq!(
            component.as_any().downcast_ref::<BrushPreset>(),
            Some(&preset)
        );
        assert!(ComponentType::heap_bytes(&preset) > 100);
        // A newer version is kept as bytes by the kernel, untouched.
        let newer = registry
            .decode("sue.brush", BrushPreset::VERSION + 1, &[9, 9, 9])
            .unwrap();
        let mut out = Writer::new();
        newer.encode(&mut out);
        assert_eq!(out.as_bytes(), &[9, 9, 9]);
    }
}
