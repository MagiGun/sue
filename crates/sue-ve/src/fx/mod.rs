//! Effects: a stack of effects per clip, as Premiere Pro's Effect
//! Controls and After Effects' layers have them.
//!
//! A clip's effects live in its properties ([`key::EFFECTS`]): an ordered
//! list, each one a map of its kind (a registered name such as
//! `blur.gaussian`), its id in the clip, whether it is on, and its
//! parameters. The list is the serial form of a chain of nodes whose
//! types are registered ([`register`], rule 3): a serial chain is all a
//! clip needs today, and keeping it in the clip's properties makes every
//! change to it part of the sequence's one undo step ([`SequenceEdit`]).
//!
//! A number parameter is keyframable: its keys are the clip's animation
//! channel [`channel`]`(effect, parameter)`, keyed in source time like
//! every clip key (trims and splits never move them), with linear,
//! constant or Bezier steps (the graph editor sets eases).
//!
//! The pixels are worked in [`pixels`]: Sue's own filters from
//! `sue-picture` (blur, sharpen, levels, curves, color balance, glow,
//! drop shadow, mosaic, through [`pixels::run_filter`]) and the ones a
//! video editor adds (keys, crop, mirror, stroke, vignette, grain).
//! Masks are in [`mask`].
//!
//! [`SequenceEdit`]: sue_model::SequenceEdit

pub mod mask;
pub mod pixels;
pub mod preset;

use std::sync::{LazyLock, PoisonError, RwLock};

use sue_core::{Properties, Time, Value};
use sue_model::Clip;

/// The keys of what a clip carries for effects and compositing.
pub mod key {
    /// The effects stack (a list of maps).
    pub const EFFECTS: &str = "ve.effects";
    /// The masks (a list of maps).
    pub const MASKS: &str = "ve.masks";
    /// The blend mode's name (`sue_model::BlendMode::name`).
    pub const BLEND: &str = "ve.blend";
    /// The anchor point across, in source pixels from the middle
    /// (keyframable).
    pub const ANCHOR_X: &str = "ve.anchor_x";
    /// The anchor point down (keyframable).
    pub const ANCHOR_Y: &str = "ve.anchor_y";
    /// Motion blur's shutter angle, 0 (none) to 360 degrees.
    pub const MOTION_BLUR: &str = "ve.motion_blur";
    /// Time remapping: the source time (seconds) that shows, keyed over
    /// the clip's own time (a channel only).
    pub const REMAP: &str = "ve.remap";
    /// The color grade (a list of correctors, `crate::grade`).
    pub const GRADE: &str = "ve.grade";
    /// A track's audio effects (a list of maps, `crate::audio`).
    pub const AUDIO_FX: &str = "ve.audio_fx";
    /// A clip's or track's pan, -1 (left) to 1 (right) (keyframable).
    pub const PAN: &str = "ve.pan";
}

/// Where an effect is listed in the effects browser.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    /// Blurs and sharpening.
    Blur,
    /// Color and tone.
    Color,
    /// Looks: glow, shadow, stroke, vignette, grain, mosaic.
    Stylize,
    /// Keys and mattes.
    Keying,
    /// Crop, mirror.
    Transform,
}

impl Category {
    /// Every category, as the browser lists them.
    pub const ALL: [Self; 5] = [
        Self::Blur,
        Self::Color,
        Self::Stylize,
        Self::Keying,
        Self::Transform,
    ];

    /// Its stable name.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Blur => "blur",
            Self::Color => "color",
            Self::Stylize => "stylize",
            Self::Keying => "keying",
            Self::Transform => "transform",
        }
    }

    /// Its name as people read it.
    #[must_use]
    pub fn label(self) -> String {
        tr!(&format!("fx-category-{}", self.id()))
    }
}

/// What a parameter holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamKind {
    /// A number from `min` to `max` (keyframable), shown with `suffix`.
    Number {
        /// The least.
        min: f64,
        /// The most.
        max: f64,
        /// What a new effect has.
        default: f64,
        /// The unit shown after it (`px`, `%`, `deg`), not translated.
        suffix: &'static str,
    },
    /// On or off.
    Toggle(bool),
    /// One of several stable names; the default's index.
    Choice(&'static [&'static str], usize),
    /// A color (RGB), the default.
    Color([u8; 3]),
    /// A tone curve: points from (0, 0) to (1, 1).
    Curve,
}

/// One parameter of an effect kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamDef {
    /// Its key in the effect's parameters.
    pub key: &'static str,
    /// What it holds.
    pub kind: ParamKind,
}

impl ParamDef {
    /// Its name as people read it.
    #[must_use]
    pub fn label(&self) -> String {
        tr!(&format!("fx-param-{}", self.key.replace('_', "-")))
    }

    /// Its default value.
    #[must_use]
    pub fn default_value(&self) -> Value {
        match self.kind {
            ParamKind::Number { default, .. } => Value::Float(default),
            ParamKind::Toggle(on) => Value::Bool(on),
            ParamKind::Choice(_, index) => Value::Int(index as i64),
            ParamKind::Color(rgb) => color_value(rgb),
            ParamKind::Curve => curve_value(&[[0.0, 0.0], [1.0, 1.0]]),
        }
    }
}

const fn number(
    key: &'static str,
    min: f64,
    max: f64,
    default: f64,
    suffix: &'static str,
) -> ParamDef {
    ParamDef {
        key,
        kind: ParamKind::Number {
            min,
            max,
            default,
            suffix,
        },
    }
}

const fn toggle(key: &'static str, default: bool) -> ParamDef {
    ParamDef {
        key,
        kind: ParamKind::Toggle(default),
    }
}

const fn choice(key: &'static str, options: &'static [&'static str], default: usize) -> ParamDef {
    ParamDef {
        key,
        kind: ParamKind::Choice(options, default),
    }
}

const fn color(key: &'static str, rgb: [u8; 3]) -> ParamDef {
    ParamDef {
        key,
        kind: ParamKind::Color(rgb),
    }
}

const fn curve(key: &'static str) -> ParamDef {
    ParamDef {
        key,
        kind: ParamKind::Curve,
    }
}

/// A kind of effect: its stable name, where it is listed and its
/// parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectKind {
    /// Its stable name, e.g. `blur.gaussian`.
    pub id: &'static str,
    /// Where it is listed.
    pub category: Category,
    /// Its parameters, in the order Effect Controls shows them.
    pub params: Vec<ParamDef>,
}

impl EffectKind {
    /// Its name as people read it.
    #[must_use]
    pub fn label(&self) -> String {
        tr!(&format!("fx-{}", self.id.replace(['.', '_'], "-")))
    }

    /// The parameter `key`.
    #[must_use]
    pub fn param(&self, key: &str) -> Option<&ParamDef> {
        self.params.iter().find(|param| param.key == key)
    }
}

/// The mirror modes.
pub const MIRROR: &[&str] = &["left", "right", "top", "bottom", "flip-x", "flip-y"];
/// The track matte modes.
pub const MATTE: &[&str] = &["alpha", "luma"];
/// The luma key's sides.
pub const LUMA_SIDE: &[&str] = &["dark", "bright"];

fn builtin() -> Vec<EffectKind> {
    use Category::{Blur, Color, Keying, Stylize, Transform};
    let kind = |id, category, params: &[ParamDef]| EffectKind {
        id,
        category,
        params: params.to_vec(),
    };
    vec![
        kind(
            "blur.gaussian",
            Blur,
            &[number("radius", 0.0, 250.0, 10.0, "px")],
        ),
        kind(
            "blur.directional",
            Blur,
            &[
                number("angle", -360.0, 360.0, 0.0, "deg"),
                number("length", 0.0, 500.0, 20.0, "px"),
            ],
        ),
        kind(
            "blur.sharpen",
            Blur,
            &[
                number("amount", 0.0, 500.0, 100.0, "%"),
                number("radius", 0.1, 50.0, 1.5, "px"),
            ],
        ),
        kind(
            "color.brightness_contrast",
            Color,
            &[
                number("brightness", -100.0, 100.0, 0.0, ""),
                number("contrast", -100.0, 100.0, 0.0, ""),
            ],
        ),
        kind(
            "color.hue_saturation",
            Color,
            &[
                number("hue", -180.0, 180.0, 0.0, "deg"),
                number("saturation", -100.0, 100.0, 0.0, ""),
                number("lightness", -100.0, 100.0, 0.0, ""),
            ],
        ),
        kind(
            "color.levels",
            Color,
            &[
                number("in_black", 0.0, 255.0, 0.0, ""),
                number("in_white", 0.0, 255.0, 255.0, ""),
                number("gamma", 0.1, 10.0, 1.0, ""),
                number("out_black", 0.0, 255.0, 0.0, ""),
                number("out_white", 0.0, 255.0, 255.0, ""),
            ],
        ),
        kind(
            "color.curves",
            Color,
            &[curve("rgb"), curve("red"), curve("green"), curve("blue")],
        ),
        kind(
            "color.balance",
            Color,
            &[
                number("shadow_red", -100.0, 100.0, 0.0, ""),
                number("shadow_green", -100.0, 100.0, 0.0, ""),
                number("shadow_blue", -100.0, 100.0, 0.0, ""),
                number("mid_red", -100.0, 100.0, 0.0, ""),
                number("mid_green", -100.0, 100.0, 0.0, ""),
                number("mid_blue", -100.0, 100.0, 0.0, ""),
                number("high_red", -100.0, 100.0, 0.0, ""),
                number("high_green", -100.0, 100.0, 0.0, ""),
                number("high_blue", -100.0, 100.0, 0.0, ""),
            ],
        ),
        kind(
            "stylize.glow",
            Stylize,
            &[
                number("radius", 0.0, 200.0, 20.0, "px"),
                number("threshold", 0.0, 100.0, 60.0, "%"),
                number("strength", 0.0, 200.0, 80.0, "%"),
                color("color", [255, 255, 255]),
            ],
        ),
        kind(
            "stylize.drop_shadow",
            Stylize,
            &[
                number("angle", -360.0, 360.0, 135.0, "deg"),
                number("distance", 0.0, 500.0, 10.0, "px"),
                number("softness", 0.0, 200.0, 8.0, "px"),
                number("opacity", 0.0, 100.0, 60.0, "%"),
                color("color", [0, 0, 0]),
            ],
        ),
        kind(
            "stylize.stroke",
            Stylize,
            &[
                number("width", 0.0, 100.0, 4.0, "px"),
                number("opacity", 0.0, 100.0, 100.0, "%"),
                color("color", [255, 255, 255]),
            ],
        ),
        kind(
            "stylize.vignette",
            Stylize,
            &[
                number("amount", -100.0, 100.0, 50.0, ""),
                number("size", 0.0, 100.0, 60.0, "%"),
                number("roundness", 0.0, 100.0, 50.0, "%"),
                number("feather", 0.0, 100.0, 50.0, "%"),
            ],
        ),
        kind(
            "stylize.grain",
            Stylize,
            &[
                number("amount", 0.0, 100.0, 20.0, "%"),
                number("size", 1.0, 8.0, 1.0, "px"),
                toggle("color", false),
                toggle("animated", true),
            ],
        ),
        kind(
            "stylize.mosaic",
            Stylize,
            &[number("size", 1.0, 400.0, 16.0, "px")],
        ),
        kind(
            "key.chroma",
            Keying,
            &[
                color("key_color", [0, 177, 64]),
                number("tolerance", 0.0, 100.0, 30.0, "%"),
                number("softness", 0.0, 100.0, 10.0, "%"),
                number("spill", 0.0, 100.0, 50.0, "%"),
            ],
        ),
        kind(
            "key.luma",
            Keying,
            &[
                choice("side", LUMA_SIDE, 0),
                number("threshold", 0.0, 100.0, 10.0, "%"),
                number("softness", 0.0, 100.0, 10.0, "%"),
            ],
        ),
        kind(
            "key.track_matte",
            Keying,
            &[
                number("track", 1.0, 99.0, 2.0, ""),
                choice("mode", MATTE, 0),
                toggle("invert", false),
            ],
        ),
        kind(
            "transform.crop",
            Transform,
            &[
                number("left", 0.0, 100.0, 0.0, "%"),
                number("top", 0.0, 100.0, 0.0, "%"),
                number("right", 0.0, 100.0, 0.0, "%"),
                number("bottom", 0.0, 100.0, 0.0, "%"),
                number("feather", 0.0, 200.0, 0.0, "px"),
            ],
        ),
        kind("transform.mirror", Transform, &[choice("mode", MIRROR, 0)]),
    ]
}

static REGISTRY: LazyLock<RwLock<Vec<EffectKind>>> = LazyLock::new(|| RwLock::new(builtin()));

/// Registers an effect kind (a later one of the same name replaces it).
/// Kinds a build does not know stay in clips untouched and do nothing.
pub fn register(kind: EffectKind) {
    let mut kinds = REGISTRY.write().unwrap_or_else(PoisonError::into_inner);
    kinds.retain(|known| known.id != kind.id);
    kinds.push(kind);
}

/// Every registered effect kind.
#[must_use]
pub fn kinds() -> Vec<EffectKind> {
    REGISTRY
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// The registered kind `id`.
#[must_use]
pub fn kind(id: &str) -> Option<EffectKind> {
    REGISTRY
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|kind| kind.id == id)
        .cloned()
}

/// The kinds whose name matches `search` (any case), by category.
#[must_use]
pub fn search(search: &str) -> Vec<EffectKind> {
    let needle = search.trim().to_lowercase();
    kinds()
        .into_iter()
        .filter(|kind| {
            needle.is_empty()
                || kind.label().to_lowercase().contains(&needle)
                || kind.id.contains(&needle)
                || kind.category.label().to_lowercase().contains(&needle)
        })
        .collect()
}

/// One effect on a clip.
#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    /// Its id in the clip (keyframe channels name it).
    pub id: u64,
    /// Its kind's stable name.
    pub kind: String,
    /// On (else bypassed).
    pub enabled: bool,
    /// Its parameters (those left out have their defaults).
    pub params: Properties,
}

impl Effect {
    /// A new effect of `kind` with id `id` and its defaults.
    #[must_use]
    pub fn new(kind: &str, id: u64) -> Self {
        let mut params = Properties::new();
        if let Some(found) = self::kind(kind) {
            for param in &found.params {
                params.set(param.key, param.default_value());
            }
        }
        Self {
            id,
            kind: kind.to_owned(),
            enabled: true,
            params,
        }
    }

    /// As it is stored.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut map = std::collections::BTreeMap::new();
        map.insert("id".to_owned(), Value::Int(self.id as i64));
        map.insert("kind".to_owned(), Value::Text(self.kind.clone()));
        map.insert("on".to_owned(), Value::Bool(self.enabled));
        map.insert(
            "params".to_owned(),
            Value::Map(
                self.params
                    .iter()
                    .map(|(key, value)| (key.to_owned(), value.clone()))
                    .collect(),
            ),
        );
        Value::Map(map)
    }

    /// Read back from what is stored.
    #[must_use]
    pub fn from_value(value: &Value) -> Option<Self> {
        let Value::Map(map) = value else {
            return None;
        };
        let mut params = Properties::new();
        if let Some(Value::Map(stored)) = map.get("params") {
            for (key, value) in stored {
                params.set(key.clone(), value.clone());
            }
        }
        Some(Self {
            id: map.get("id").and_then(Value::as_i64)?.max(0) as u64,
            kind: map.get("kind").and_then(Value::as_str)?.to_owned(),
            enabled: map.get("on").and_then(Value::as_bool).unwrap_or(true),
            params,
        })
    }

    /// The number parameter `key` at sequence time `time` on `clip`: its
    /// keyframes, else its value, else its default.
    #[must_use]
    pub fn number(&self, clip: &Clip, key: &str, time: Time) -> f64 {
        let fallback = self
            .params
            .get(key)
            .and_then(Value::as_f64)
            .or_else(|| {
                kind(&self.kind).and_then(|kind| match kind.param(key)?.kind {
                    ParamKind::Number { default, .. } => Some(default),
                    _ => None,
                })
            })
            .unwrap_or(0.0);
        clip.animation
            .channel(&channel(self.id, key))
            .and_then(|curve| curve.sample(clip.source_time(time)))
            .unwrap_or(fallback)
    }

    /// The parameters at sequence time `time` on `clip`: numbers sampled
    /// from their keys, the rest as they are.
    #[must_use]
    pub fn at(&self, clip: &Clip, time: Time) -> Params {
        let mut out = Params::default();
        let known = kind(&self.kind);
        let defs = known.as_ref().map_or(&[][..], |kind| &kind.params[..]);
        for def in defs {
            let value = match def.kind {
                ParamKind::Number { .. } => Value::Float(self.number(clip, def.key, time)),
                _ => self
                    .params
                    .get(def.key)
                    .cloned()
                    .unwrap_or_else(|| def.default_value()),
            };
            out.values.set(def.key, value);
        }
        out
    }
}

/// An effect's parameters at one moment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Params {
    /// By key.
    pub values: Properties,
}

impl Params {
    /// The number `key` (0 when missing).
    #[must_use]
    pub fn f(&self, key: &str) -> f32 {
        self.values.get(key).and_then(Value::as_f64).unwrap_or(0.0) as f32
    }

    /// The switch `key`.
    #[must_use]
    pub fn on(&self, key: &str) -> bool {
        self.values
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// The choice `key`'s index.
    #[must_use]
    pub fn pick(&self, key: &str) -> usize {
        self.values
            .get(key)
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .max(0) as usize
    }

    /// The color `key`.
    #[must_use]
    pub fn rgb(&self, key: &str) -> [u8; 3] {
        self.values.get(key).and_then(rgb_of).unwrap_or([0, 0, 0])
    }

    /// The curve `key`'s points.
    #[must_use]
    pub fn points(&self, key: &str) -> Vec<[f32; 2]> {
        self.values
            .get(key)
            .map(points_of)
            .filter(|points| points.len() >= 2)
            .unwrap_or_else(|| vec![[0.0, 0.0], [1.0, 1.0]])
    }

    /// Sets number `key`.
    #[must_use]
    pub fn with(mut self, key: &str, value: f64) -> Self {
        self.values.set(key, value);
        self
    }
}

/// A color as stored: red, green and blue from 0 to 1.
#[must_use]
pub fn color_value(rgb: [u8; 3]) -> Value {
    Value::Vec3(rgb.map(|channel| f64::from(channel) / 255.0))
}

/// The color a value holds.
#[must_use]
pub fn rgb_of(value: &Value) -> Option<[u8; 3]> {
    match value {
        Value::Vec3(rgb) => {
            Some(rgb.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8))
        }
        _ => None,
    }
}

/// A curve as stored: its points.
#[must_use]
pub fn curve_value(points: &[[f32; 2]]) -> Value {
    Value::List(
        points
            .iter()
            .map(|[x, y]| Value::Vec2([f64::from(*x), f64::from(*y)]))
            .collect(),
    )
}

/// The points a curve value holds.
#[must_use]
pub fn points_of(value: &Value) -> Vec<[f32; 2]> {
    match value {
        Value::List(points) => points
            .iter()
            .filter_map(|point| match point {
                Value::Vec2([x, y]) => Some([*x as f32, *y as f32]),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The animation channel of effect `effect`'s parameter `param`.
#[must_use]
pub fn channel(effect: u64, param: &str) -> String {
    format!("fx.{effect}.{param}")
}

/// The effect and parameter a channel name is of, if it is an effect's.
#[must_use]
pub fn parse_channel(name: &str) -> Option<(u64, &str)> {
    let rest = name.strip_prefix("fx.")?;
    let (id, param) = rest.split_once('.')?;
    Some((id.parse().ok()?, param))
}

/// The effects on `clip`, in order.
#[must_use]
pub fn effects(clip: &Clip) -> Vec<Effect> {
    match clip.properties.get(key::EFFECTS) {
        Some(Value::List(list)) => list.iter().filter_map(Effect::from_value).collect(),
        _ => Vec::new(),
    }
}

/// Sets `clip`'s effects (none removes the property).
pub fn set_effects(clip: &mut Clip, effects: &[Effect]) {
    if effects.is_empty() {
        clip.properties.remove(key::EFFECTS);
    } else {
        clip.properties.set(
            key::EFFECTS,
            Value::List(effects.iter().map(Effect::to_value).collect()),
        );
    }
}

/// The next free effect id on `clip`.
#[must_use]
pub fn next_id(clip: &Clip) -> u64 {
    effects(clip)
        .iter()
        .map(|effect| effect.id)
        .max()
        .unwrap_or(0)
        + 1
}

/// Adds a new effect of `kind` at the end of `clip`'s stack; its id.
pub fn add(clip: &mut Clip, kind: &str) -> u64 {
    let mut list = effects(clip);
    let id = next_id(clip);
    list.push(Effect::new(kind, id));
    set_effects(clip, &list);
    id
}

/// Removes effect `id` from `clip` with its keys. `true` when it was
/// there.
pub fn remove(clip: &mut Clip, id: u64) -> bool {
    let mut list = effects(clip);
    let before = list.len();
    list.retain(|effect| effect.id != id);
    if list.len() == before {
        return false;
    }
    set_effects(clip, &list);
    let channels: Vec<String> = clip
        .animation
        .channels()
        .filter(|(name, _)| parse_channel(name).is_some_and(|(effect, _)| effect == id))
        .map(|(name, _)| name.to_owned())
        .collect();
    for name in channels {
        clip.animation.remove_channel(&name);
    }
    true
}

/// Moves effect `id` by `by` places in `clip`'s stack.
pub fn reorder(clip: &mut Clip, id: u64, by: i64) -> bool {
    let mut list = effects(clip);
    let Some(at) = list.iter().position(|effect| effect.id == id) else {
        return false;
    };
    let to = (at as i64 + by).clamp(0, list.len() as i64 - 1) as usize;
    if to == at {
        return false;
    }
    let effect = list.remove(at);
    list.insert(to, effect);
    set_effects(clip, &list);
    true
}

/// Changes effect `id` on `clip` with `change`. `true` when it was there.
pub fn update(clip: &mut Clip, id: u64, change: impl FnOnce(&mut Effect)) -> bool {
    let mut list = effects(clip);
    let Some(effect) = list.iter_mut().find(|effect| effect.id == id) else {
        return false;
    };
    change(effect);
    set_effects(clip, &list);
    true
}

/// `true` when `clip` has anything the graphics card's quads cannot
/// show: effects that are on, masks, a blend mode, motion blur, a grade.
#[must_use]
pub fn needs_processing(clip: &Clip) -> bool {
    effects(clip).iter().any(|effect| effect.enabled)
        || clip.properties.get(key::MASKS).is_some()
        || clip
            .properties
            .get(key::BLEND)
            .and_then(Value::as_str)
            .is_some_and(|blend| blend != "normal")
        || clip
            .properties
            .get(key::MOTION_BLUR)
            .and_then(Value::as_f64)
            .is_some_and(|angle| angle > 0.0)
        || crate::grade::grade_of(clip)
            .iter()
            .any(|corrector| corrector.enabled)
        || clip.properties.get(crate::layers::ANIMATOR).is_some()
}

#[cfg(test)]
mod tests;
