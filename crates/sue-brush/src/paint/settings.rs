//! The behaviours' settings as data on a [`BrushPreset`].
//!
//! Numbers a pen may drive are parameters ([`key`]) registered with
//! [`register`], so they take bindings like size and opacity do and
//! reach the painter through [`crate::input::StrokePoint::extra`].
//! Switches and choices are tagged fields of the preset (`paint.*`),
//! which the brush model keeps and writes back even where a build does
//! not know them (backbone rule 3). `docs/brush-behaviours.md` lists
//! every stored name; a name, once given, is never taken back.

use sue_model::BrushPreset;
use sue_model::brush::{
    BrushRegistry, ContentHash, ParamDef, TextureMode as ModelTextureMode, TextureSource, Unit,
};

use crate::texture::{TextureMode, TextureSettings};

/// The parameter keys the paint behaviours register. Each is a number a
/// pen can drive.
pub mod key {
    /// Clip Studio's "density of paint": how fully the mixed paint
    /// covers what is under it, 0 to 1.
    pub const PAINT_DENSITY: &str = "paint.paint_density";
    /// Multiplies the brush's amount of paint (`mix.load`), so a pen can
    /// drive it; 1 leaves it as it is.
    pub const AMOUNT_OF_PAINT: &str = "paint.amount_of_paint";
    /// Multiplies the brush's color stretch (`mix.pull`); 1 leaves it.
    pub const COLOR_STRETCH: &str = "paint.color_stretch";
    /// A sprayed particle's diameter: pixels, or a share of the brush's
    /// size when the particle size is linked to it.
    pub const PARTICLE_SIZE: &str = "paint.particle_size";
    /// Particles in one spray dab.
    pub const PARTICLE_DENSITY: &str = "paint.particle_density";
    /// 0: particles spread evenly over the brush; 1: gathered close to
    /// its middle.
    pub const SPRAY_DEVIATION: &str = "paint.spray_deviation";
    /// The second tip's size: pixels, or a share of the main size when
    /// linked to it.
    pub const DUAL_SIZE: &str = "paint.dual_size";
    /// The gap between the second tip's dabs, in its sizes.
    pub const DUAL_SPACING: &str = "paint.dual_spacing";
    /// How far the second tip's dabs fall from the line, in its sizes.
    pub const DUAL_SCATTER: &str = "paint.dual_scatter";
    /// How far in from the stroke's rim the watercolor edge reaches, in
    /// pixels.
    pub const WATERCOLOR_RANGE: &str = "paint.watercolor_range";
    /// How opaque the inside of a watercolor stroke stays, 0 to 1.
    pub const WATERCOLOR_OPACITY: &str = "paint.watercolor_opacity";
    /// How much darker the rim gets, 0 to 1.
    pub const WATERCOLOR_DARKNESS: &str = "paint.watercolor_darkness";
    /// How far the rim is softened, in pixels.
    pub const WATERCOLOR_BLUR: &str = "paint.watercolor_blur";
    /// The width of one tone cell, in pixels.
    pub const TONE_CELL: &str = "paint.tone_cell";
    /// The tone's screen angle, in degrees.
    pub const TONE_ANGLE: &str = "paint.tone_angle";
    /// Dabs a second while the pen holds still with continuous spraying.
    pub const SPRAY_RATE: &str = "paint.spray_rate";
    /// The paper's brightness, -1 to 1.
    pub const TEXTURE_BRIGHTNESS: &str = "paint.texture_brightness";
    /// The paper's contrast, -1 to 1.
    pub const TEXTURE_CONTRAST: &str = "paint.texture_contrast";
}

/// The tagged fields (switches and choices) the behaviours store on a
/// preset, beside the brush model's own.
pub mod field {
    /// How a several-picture tip takes its pictures: [`super::TipOrder`].
    pub const TIP_ORDER: &str = "paint.tip_order";
    /// Flip the tip across: [`super::Flip`].
    pub const FLIP_X: &str = "paint.flip_x";
    /// Flip the tip up and down: [`super::Flip`].
    pub const FLIP_Y: &str = "paint.flip_y";
    /// Dabs keep their density however close they are: a flag.
    pub const GAP_DENSITY: &str = "paint.gap_density";
    /// How overlapping dabs of a color tip meet: [`super::TipBlend`].
    pub const TIP_BLEND: &str = "paint.tip_blend";
    /// Spraying is on: a flag.
    pub const SPRAY: &str = "paint.spray";
    /// The particle size is a share of the brush's size: a flag.
    pub const SPRAY_LINKED: &str = "paint.spray_linked";
    /// Which way particles turn: [`super::ParticleDirection`].
    pub const SPRAY_DIRECTION: &str = "paint.spray_direction";
    /// Dabs keep coming while the pen holds still: a flag.
    pub const CONTINUOUS: &str = "paint.continuous";
    /// Which of Sue's own papers, when the texture is Sue's grain.
    pub const TEXTURE_PAPER: &str = "paint.texture_paper";
    /// The texture mode, where it is one the brush model has no name
    /// for ([`crate::texture::TextureMode`]).
    pub const TEXTURE_MODE: &str = "paint.texture_mode";
    /// The paper is inverted: a flag.
    pub const TEXTURE_INVERT: &str = "paint.texture_invert";
    /// Emphasized density: a flag.
    pub const TEXTURE_EMPHASIZE: &str = "paint.texture_emphasize";
    /// The texture is laid on every dab: a flag.
    pub const TEXTURE_PER_DAB: &str = "paint.texture_per_dab";
    /// The dual brush is on: a flag.
    pub const DUAL: &str = "paint.dual";
    /// How the second tip meets the first: [`super::DualMode`].
    pub const DUAL_MODE: &str = "paint.dual_mode";
    /// The second tip's size is a share of the main size: a flag.
    pub const DUAL_LINKED: &str = "paint.dual_linked";
    /// The second tip's hardness, as text (`0.5`): round tips only.
    pub const DUAL_HARDNESS: &str = "paint.dual_hardness";
    /// The second tip's pictures: SHA-256 hashes, 32 bytes each.
    pub const DUAL_PICTURES: &str = "paint.dual_pictures";
    /// The watercolor edge is on: a flag.
    pub const WATERCOLOR: &str = "paint.watercolor";
    /// The edge is made once the pen lifts, not while drawing: a flag.
    pub const WATERCOLOR_AFTER: &str = "paint.watercolor_after";
    /// Corners stay sharp (on, the default) or are rounded: a flag.
    pub const SHARP_ANGLES: &str = "paint.sharp_angles";
    /// The brush lays a halftone: a flag.
    pub const TONE: &str = "paint.tone";
    /// The halftone's dots: [`super::ToneShape`].
    pub const TONE_SHAPE: &str = "paint.tone_shape";
    /// The paint goes behind what is on the layer: a flag.
    pub const BEHIND: &str = "paint.behind";
    /// The eraser erases on every layer it is given, not one: a flag.
    pub const ALL_LAYERS: &str = "paint.all_layers";
}

/// Registers the behaviours' parameters ([`key`]) in `registry`, so a
/// preset's bindings can drive them.
pub fn register(registry: &mut BrushRegistry) {
    for def in [
        ParamDef::new(key::PAINT_DENSITY, Unit::Share, 1.0, 0.0, 1.0),
        ParamDef::new(key::AMOUNT_OF_PAINT, Unit::Share, 1.0, 0.0, 1.0),
        ParamDef::new(key::COLOR_STRETCH, Unit::Share, 1.0, 0.0, 1.0),
        ParamDef::new(key::PARTICLE_SIZE, Unit::Pixels, 0.15, 0.0, 2000.0),
        ParamDef::new(key::PARTICLE_DENSITY, Unit::Pixels, 12.0, 1.0, 256.0),
        ParamDef::new(key::SPRAY_DEVIATION, Unit::Share, 0.0, 0.0, 1.0),
        ParamDef::new(key::DUAL_SIZE, Unit::Pixels, 0.5, 0.0, 2000.0),
        ParamDef::new(key::DUAL_SPACING, Unit::Sizes, 0.5, 0.0, 100.0),
        ParamDef::new(key::DUAL_SCATTER, Unit::Sizes, 0.5, 0.0, 100.0),
        ParamDef::new(key::WATERCOLOR_RANGE, Unit::Pixels, 6.0, 0.0, 200.0),
        ParamDef::new(key::WATERCOLOR_OPACITY, Unit::Share, 0.55, 0.0, 1.0),
        ParamDef::new(key::WATERCOLOR_DARKNESS, Unit::Share, 0.35, 0.0, 1.0),
        ParamDef::new(key::WATERCOLOR_BLUR, Unit::Pixels, 1.0, 0.0, 50.0),
        ParamDef::new(key::TONE_CELL, Unit::Pixels, 6.0, 2.0, 200.0),
        ParamDef::new(key::TONE_ANGLE, Unit::Degrees, 45.0, -360.0, 360.0),
        ParamDef::new(key::SPRAY_RATE, Unit::Pixels, 60.0, 0.0, 1000.0),
        ParamDef::new(key::TEXTURE_BRIGHTNESS, Unit::Share, 0.0, -1.0, 1.0),
        ParamDef::new(key::TEXTURE_CONTRAST, Unit::Share, 0.0, -1.0, 1.0),
    ] {
        registry.register_param(def);
    }
}

/// [`BrushRegistry::standard`] with the paint behaviours' parameters.
#[must_use]
pub fn registry() -> BrushRegistry {
    let mut registry = BrushRegistry::standard();
    register(&mut registry);
    registry
}

/// Reads and writes a choice by its stored name.
macro_rules! choice {
    ($(#[$doc:meta])* $type:ident { $($(#[$vdoc:meta])* $variant:ident => $name:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub enum $type {
            $($(#[$vdoc])* $variant),+
        }

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

            /// The choice stored as `name`.
            #[must_use]
            pub fn from_name(name: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|choice| choice.name() == name)
            }
        }
    };
}

choice! {
    /// How a tip of several pictures takes them, dab by dab (Clip
    /// Studio's "repeat method").
    TipOrder {
        /// One after another, then from the first again.
        #[default]
        Repeat => "repeat",
        /// One after another, then back the other way.
        BackAndForth => "back-and-forth",
        /// One after another, then the last for good.
        Clamp => "clamp",
        /// One at random for each dab.
        Random => "random",
    }
}

choice! {
    /// Whether a dab's picture is flipped.
    Flip {
        /// Never.
        #[default]
        Off => "off",
        /// Always.
        On => "on",
        /// At random, dab by dab.
        Random => "random",
    }
}

choice! {
    /// How the dabs of a color tip meet where they overlap.
    TipBlend {
        /// The later dab lies over the earlier.
        #[default]
        Over => "over",
        /// The darker of the two, channel by channel.
        Darken => "darken",
    }
}

choice! {
    /// Which way a sprayed particle turns.
    ParticleDirection {
        /// As the brush's own angle says.
        #[default]
        Brush => "brush",
        /// Along the line.
        Line => "line",
        /// Every particle its own way.
        Random => "random",
    }
}

choice! {
    /// How the second tip of a dual brush meets the first.
    DualMode {
        /// The second tip's coverage multiplies the first's.
        #[default]
        Multiply => "multiply",
        /// The lesser of the two.
        Darken => "darken",
        /// The second tip takes paint away.
        Subtract => "subtract",
        /// Around half: more where the second tip is strong, less where
        /// it is weak.
        Overlay => "overlay",
        /// The second tip lets paint through where it is stronger than
        /// what the first leaves out: crisp, broken edges.
        Height => "height",
    }
}

choice! {
    /// The dots of a halftone.
    ToneShape {
        /// Round dots that grow into each other.
        #[default]
        Dot => "dot",
        /// Parallel lines that thicken.
        Line => "line",
    }
}

/// Clip Studio's spraying effect: each dab of the brush becomes a spray
/// of small particles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spray {
    /// A particle's diameter: pixels, or a share of the brush's size
    /// when `linked`.
    pub size: f32,
    /// The particle size follows the brush's size.
    pub linked: bool,
    /// Particles in a dab.
    pub density: f32,
    /// 0: spread evenly over the brush; 1: gathered at its middle.
    pub deviation: f32,
    /// Which way particles turn.
    pub direction: ParticleDirection,
}

impl Default for Spray {
    fn default() -> Self {
        Self {
            size: 0.15,
            linked: true,
            density: 12.0,
            deviation: 0.0,
            direction: ParticleDirection::Brush,
        }
    }
}

/// The paper of a textured brush.
#[derive(Clone, Debug, PartialEq)]
pub enum PaperChoice {
    /// One of Sue's own papers ([`crate::texture::BUILTIN`]).
    Builtin(String),
    /// A person's own paper picture, by the hash of its file.
    Picture(ContentHash),
}

/// A textured brush: which paper and how it is laid.
#[derive(Clone, Debug, PartialEq)]
pub struct TextureChoice {
    /// The paper.
    pub paper: PaperChoice,
    /// How it is laid and how it meets the stroke.
    pub settings: TextureSettings,
}

/// The second tip of a dual brush: its dabs make a mask the main
/// stroke is laid through.
#[derive(Clone, Debug, PartialEq)]
pub struct DualBrush {
    /// How it meets the main tip.
    pub mode: DualMode,
    /// Its size: pixels, or a share of the main size when `linked`.
    pub size: f32,
    /// The size follows the main brush's size.
    pub linked: bool,
    /// The gap between its dabs, in its sizes.
    pub spacing: f32,
    /// How far its dabs fall from the line, in its sizes.
    pub scatter: f32,
    /// Its hardness, when it is round.
    pub hardness: f32,
    /// Its pictures (empty: a round tip).
    pub pictures: Vec<ContentHash>,
}

impl Default for DualBrush {
    fn default() -> Self {
        Self {
            mode: DualMode::Multiply,
            size: 0.5,
            linked: true,
            spacing: 0.5,
            scatter: 0.5,
            hardness: 0.6,
            pictures: Vec::new(),
        }
    }
}

/// The watercolor edge: the paint gathers at the stroke's rim, as wet
/// paint does when it dries, and the inside stays lighter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WatercolorEdge {
    /// How far in from the rim the edge reaches, in pixels.
    pub range: f32,
    /// How opaque the inside stays, 0 to 1.
    pub opacity: f32,
    /// How much darker the rim gets, 0 to 1.
    pub darkness: f32,
    /// How far the rim is softened, in pixels.
    pub blur: f32,
    /// The edge is made when the pen lifts; while drawing the stroke
    /// shows without it.
    pub after_stroke: bool,
}

impl Default for WatercolorEdge {
    fn default() -> Self {
        Self {
            range: 6.0,
            opacity: 0.55,
            darkness: 0.35,
            blur: 1.0,
            after_stroke: false,
        }
    }
}

/// A halftone brush: what it lays (or, as an eraser, takes away) is a
/// screen of dots whose size follows the coverage, so an airbrush over a
/// tone scrapes it away dot by dot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tone {
    /// The width of one cell, in pixels.
    pub cell: f32,
    /// The screen's angle, in degrees.
    pub angle: f32,
    /// Dots or lines.
    pub shape: ToneShape,
}

impl Default for Tone {
    fn default() -> Self {
        Self {
            cell: 6.0,
            angle: 45.0,
            shape: ToneShape::Dot,
        }
    }
}

/// Every paint behaviour of one brush, besides what the brush model
/// itself holds (tip, size, dynamics, tapers, color jitter, mixing,
/// blend mode, erase, build-up).
#[derive(Clone, Debug, PartialEq)]
pub struct Behaviours {
    /// How several tip pictures are taken.
    pub tip_order: TipOrder,
    /// Flip across, and up and down.
    pub flip: [Flip; 2],
    /// Dabs keep their density however close they fall (Clip Studio's
    /// "adjust brush density by gap").
    pub gap_density: bool,
    /// How overlapping dabs of a color tip meet.
    pub tip_blend: TipBlend,
    /// Clip Studio's density of paint (mixing brushes), 0 to 1.
    pub paint_density: f32,
    /// The spraying effect.
    pub spray: Option<Spray>,
    /// Continuous spraying: dabs a second while the pen holds still.
    pub continuous: Option<f32>,
    /// The paper texture.
    pub texture: Option<TextureChoice>,
    /// The second tip.
    pub dual: Option<DualBrush>,
    /// The watercolor edge.
    pub watercolor: Option<WatercolorEdge>,
    /// Corners stay sharp; off, they are rounded within the brush's
    /// radius.
    pub sharp_angles: bool,
    /// The halftone.
    pub tone: Option<Tone>,
    /// The paint goes behind what the layer holds.
    pub behind: bool,
    /// As an eraser, it erases on every layer it is handed.
    pub all_layers: bool,
}

impl Default for Behaviours {
    fn default() -> Self {
        Self {
            tip_order: TipOrder::Repeat,
            flip: [Flip::Off; 2],
            gap_density: false,
            tip_blend: TipBlend::Over,
            paint_density: 1.0,
            spray: None,
            continuous: None,
            texture: None,
            dual: None,
            watercolor: None,
            sharp_angles: true,
            tone: None,
            behind: false,
            all_layers: false,
        }
    }
}

/// The text of a flag's field.
fn flag(on: bool) -> Vec<u8> {
    vec![u8::from(on)]
}

impl Behaviours {
    /// The behaviours `preset` holds. What it does not say is at its
    /// default; a field with a value this build does not know is read
    /// as the default and left in the preset untouched.
    #[must_use]
    pub fn from_preset(preset: &BrushPreset) -> Self {
        let registry = registry();
        let number = |key: &str| registry.value(preset, key).unwrap_or(0.0);
        let on = |field: &str| preset.extra(field).is_some_and(|b| b.first() == Some(&1));
        let name = |field: &str| {
            preset
                .extra(field)
                .and_then(|b| std::str::from_utf8(b).ok())
                .map(str::to_owned)
        };
        let mut behaviours = Self {
            tip_order: name(field::TIP_ORDER)
                .and_then(|n| TipOrder::from_name(&n))
                .unwrap_or(match &preset.tip {
                    sue_model::brush::TipSource::Images {
                        pick: sue_model::brush::Pick::Random,
                        ..
                    } => TipOrder::Random,
                    _ => TipOrder::Repeat,
                }),
            flip: [
                name(field::FLIP_X)
                    .and_then(|n| Flip::from_name(&n))
                    .unwrap_or_default(),
                name(field::FLIP_Y)
                    .and_then(|n| Flip::from_name(&n))
                    .unwrap_or_default(),
            ],
            gap_density: on(field::GAP_DENSITY),
            tip_blend: name(field::TIP_BLEND)
                .and_then(|n| TipBlend::from_name(&n))
                .unwrap_or_default(),
            paint_density: number(key::PAINT_DENSITY),
            sharp_angles: preset
                .extra(field::SHARP_ANGLES)
                .is_none_or(|b| b.first() != Some(&0)),
            behind: on(field::BEHIND),
            all_layers: on(field::ALL_LAYERS),
            ..Self::default()
        };
        if on(field::SPRAY) {
            behaviours.spray = Some(Spray {
                size: number(key::PARTICLE_SIZE),
                linked: preset
                    .extra(field::SPRAY_LINKED)
                    .is_none_or(|b| b.first() != Some(&0)),
                density: number(key::PARTICLE_DENSITY),
                deviation: number(key::SPRAY_DEVIATION),
                direction: name(field::SPRAY_DIRECTION)
                    .and_then(|n| ParticleDirection::from_name(&n))
                    .unwrap_or_default(),
            });
        }
        if on(field::CONTINUOUS) {
            behaviours.continuous = Some(number(key::SPRAY_RATE));
        }
        if let Some(texture) = &preset.texture {
            let paper = match &texture.source {
                TextureSource::Grain => PaperChoice::Builtin(
                    name(field::TEXTURE_PAPER).unwrap_or_else(|| "grain".into()),
                ),
                TextureSource::Picture(hash) => PaperChoice::Picture(*hash),
            };
            let mode = name(field::TEXTURE_MODE)
                .and_then(|n| TextureMode::from_name(&n))
                .unwrap_or(match texture.mode {
                    ModelTextureMode::Subtract => TextureMode::Subtract,
                    _ => TextureMode::Multiply,
                });
            behaviours.texture = Some(TextureChoice {
                paper,
                settings: TextureSettings {
                    scale: texture.scale,
                    rotation: texture.rotation,
                    brightness: number(key::TEXTURE_BRIGHTNESS),
                    contrast: number(key::TEXTURE_CONTRAST),
                    invert: on(field::TEXTURE_INVERT),
                    strength: registry
                        .value(preset, sue_model::brush::param::TEXTURE_STRENGTH)
                        .unwrap_or(1.0),
                    mode,
                    emphasize: on(field::TEXTURE_EMPHASIZE),
                    per_dab: on(field::TEXTURE_PER_DAB),
                },
            });
        }
        if on(field::DUAL) {
            let pictures = preset
                .extra(field::DUAL_PICTURES)
                .map(|bytes| bytes.as_chunks::<32>().0.to_vec())
                .unwrap_or_default();
            behaviours.dual = Some(DualBrush {
                mode: name(field::DUAL_MODE)
                    .and_then(|n| DualMode::from_name(&n))
                    .unwrap_or_default(),
                size: number(key::DUAL_SIZE),
                linked: preset
                    .extra(field::DUAL_LINKED)
                    .is_none_or(|b| b.first() != Some(&0)),
                spacing: number(key::DUAL_SPACING),
                scatter: number(key::DUAL_SCATTER),
                hardness: name(field::DUAL_HARDNESS)
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(DualBrush::default().hardness),
                pictures,
            });
        }
        if on(field::WATERCOLOR) {
            behaviours.watercolor = Some(WatercolorEdge {
                range: number(key::WATERCOLOR_RANGE),
                opacity: number(key::WATERCOLOR_OPACITY),
                darkness: number(key::WATERCOLOR_DARKNESS),
                blur: number(key::WATERCOLOR_BLUR),
                after_stroke: on(field::WATERCOLOR_AFTER),
            });
        }
        if on(field::TONE) {
            behaviours.tone = Some(Tone {
                cell: number(key::TONE_CELL),
                angle: number(key::TONE_ANGLE),
                shape: name(field::TONE_SHAPE)
                    .and_then(|n| ToneShape::from_name(&n))
                    .unwrap_or_default(),
            });
        }
        behaviours
    }

    /// Writes the behaviours into `preset`: numbers as parameters,
    /// switches and choices as `paint.*` fields, the texture into the
    /// model's own texture where the model has a word for it. Fields
    /// the preset holds that are not the behaviours' are left as they
    /// are; a setting at its default is still written, so what a person
    /// chose reads back the same in a build whose defaults differ.
    pub fn write_to(&self, preset: &mut BrushPreset) {
        let mut put = |field: &str, bytes: Vec<u8>| {
            if let Some(slot) = preset.extra.iter_mut().find(|(k, _)| k == field) {
                slot.1 = bytes;
            } else {
                preset.extra.push((field.to_owned(), bytes));
            }
        };
        put(field::TIP_ORDER, self.tip_order.name().as_bytes().to_vec());
        put(field::FLIP_X, self.flip[0].name().as_bytes().to_vec());
        put(field::FLIP_Y, self.flip[1].name().as_bytes().to_vec());
        put(field::GAP_DENSITY, flag(self.gap_density));
        put(field::TIP_BLEND, self.tip_blend.name().as_bytes().to_vec());
        put(field::SHARP_ANGLES, flag(self.sharp_angles));
        put(field::BEHIND, flag(self.behind));
        put(field::ALL_LAYERS, flag(self.all_layers));
        put(field::SPRAY, flag(self.spray.is_some()));
        if let Some(spray) = &self.spray {
            put(field::SPRAY_LINKED, flag(spray.linked));
            put(
                field::SPRAY_DIRECTION,
                spray.direction.name().as_bytes().to_vec(),
            );
        }
        put(field::CONTINUOUS, flag(self.continuous.is_some()));
        put(field::DUAL, flag(self.dual.is_some()));
        if let Some(dual) = &self.dual {
            put(field::DUAL_MODE, dual.mode.name().as_bytes().to_vec());
            put(field::DUAL_LINKED, flag(dual.linked));
            put(field::DUAL_HARDNESS, dual.hardness.to_string().into_bytes());
            put(field::DUAL_PICTURES, dual.pictures.concat());
        }
        put(field::WATERCOLOR, flag(self.watercolor.is_some()));
        if let Some(edge) = &self.watercolor {
            put(field::WATERCOLOR_AFTER, flag(edge.after_stroke));
        }
        put(field::TONE, flag(self.tone.is_some()));
        if let Some(tone) = &self.tone {
            put(field::TONE_SHAPE, tone.shape.name().as_bytes().to_vec());
        }
        if let Some(texture) = &self.texture {
            let s = &texture.settings;
            put(field::TEXTURE_MODE, s.mode.name().as_bytes().to_vec());
            put(field::TEXTURE_INVERT, flag(s.invert));
            put(field::TEXTURE_EMPHASIZE, flag(s.emphasize));
            put(field::TEXTURE_PER_DAB, flag(s.per_dab));
            if let PaperChoice::Builtin(name) = &texture.paper {
                put(field::TEXTURE_PAPER, name.as_bytes().to_vec());
            }
        }

        preset.set(key::PAINT_DENSITY, self.paint_density);
        if let Some(spray) = &self.spray {
            preset.set(key::PARTICLE_SIZE, spray.size);
            preset.set(key::PARTICLE_DENSITY, spray.density);
            preset.set(key::SPRAY_DEVIATION, spray.deviation);
        }
        if let Some(rate) = self.continuous {
            preset.set(key::SPRAY_RATE, rate);
        }
        if let Some(dual) = &self.dual {
            preset.set(key::DUAL_SIZE, dual.size);
            preset.set(key::DUAL_SPACING, dual.spacing);
            preset.set(key::DUAL_SCATTER, dual.scatter);
        }
        if let Some(edge) = &self.watercolor {
            preset.set(key::WATERCOLOR_RANGE, edge.range);
            preset.set(key::WATERCOLOR_OPACITY, edge.opacity);
            preset.set(key::WATERCOLOR_DARKNESS, edge.darkness);
            preset.set(key::WATERCOLOR_BLUR, edge.blur);
        }
        if let Some(tone) = &self.tone {
            preset.set(key::TONE_CELL, tone.cell);
            preset.set(key::TONE_ANGLE, tone.angle);
        }
        match &self.texture {
            Some(texture) => {
                let s = &texture.settings;
                let mut model = sue_model::brush::Texture::new(match &texture.paper {
                    PaperChoice::Builtin(_) => TextureSource::Grain,
                    PaperChoice::Picture(hash) => TextureSource::Picture(*hash),
                });
                model.scale = s.scale;
                model.rotation = s.rotation;
                // The model names two modes; an older Sue reads the
                // others as the nearer of the two.
                model.mode = match s.mode {
                    TextureMode::Subtract | TextureMode::Height => ModelTextureMode::Subtract,
                    _ => ModelTextureMode::Multiply,
                };
                preset.texture = Some(model);
                preset.set(sue_model::brush::param::TEXTURE_STRENGTH, s.strength);
                preset.set(key::TEXTURE_BRIGHTNESS, s.brightness);
                preset.set(key::TEXTURE_CONTRAST, s.contrast);
            }
            None => preset.texture = None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rich() -> Behaviours {
        Behaviours {
            tip_order: TipOrder::BackAndForth,
            flip: [Flip::Random, Flip::On],
            gap_density: true,
            tip_blend: TipBlend::Darken,
            paint_density: 0.4,
            spray: Some(Spray {
                size: 3.0,
                linked: false,
                density: 30.0,
                deviation: 0.7,
                direction: ParticleDirection::Random,
            }),
            continuous: Some(80.0),
            texture: Some(TextureChoice {
                paper: PaperChoice::Builtin("canvas".into()),
                settings: TextureSettings {
                    scale: 1.5,
                    rotation: 30.0,
                    brightness: 0.25,
                    contrast: 0.5,
                    invert: true,
                    strength: 0.75,
                    mode: TextureMode::Height,
                    emphasize: true,
                    per_dab: true,
                },
            }),
            dual: Some(DualBrush {
                mode: DualMode::Subtract,
                size: 12.0,
                linked: false,
                spacing: 0.75,
                scatter: 1.5,
                hardness: 0.25,
                pictures: vec![[7; 32], [9; 32]],
            }),
            watercolor: Some(WatercolorEdge {
                range: 9.0,
                opacity: 0.3,
                darkness: 0.6,
                blur: 2.0,
                after_stroke: true,
            }),
            sharp_angles: false,
            tone: Some(Tone {
                cell: 8.0,
                angle: 15.0,
                shape: ToneShape::Line,
            }),
            behind: true,
            all_layers: true,
        }
    }

    #[test]
    fn behaviours_round_trip_through_a_preset_and_its_bytes() {
        let mut preset = BrushPreset::new("Everything");
        let behaviours = rich();
        behaviours.write_to(&mut preset);
        assert_eq!(Behaviours::from_preset(&preset), behaviours);
        let bytes = preset.to_bytes();
        let read = BrushPreset::from_bytes(&bytes).expect("read back");
        assert_eq!(Behaviours::from_preset(&read), behaviours);
        // Written twice, a field is not doubled.
        behaviours.write_to(&mut preset);
        let count = preset
            .extra
            .iter()
            .filter(|(k, _)| k == field::TONE)
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn a_plain_preset_has_plain_behaviours() {
        let preset = BrushPreset::new("Pen");
        assert_eq!(Behaviours::from_preset(&preset), Behaviours::default());
        let mut written = BrushPreset::new("Pen");
        Behaviours::default().write_to(&mut written);
        assert_eq!(Behaviours::from_preset(&written), Behaviours::default());
    }

    #[test]
    fn an_older_sue_reads_the_texture_mode_it_knows_and_keeps_ours() {
        let mut preset = BrushPreset::new("Chalk");
        rich().write_to(&mut preset);
        // The model's own mode is the nearer one it has a name for.
        assert_eq!(
            preset.texture.as_ref().map(|t| t.mode),
            Some(ModelTextureMode::Subtract)
        );
        // A choice from a newer Sue that this build does not know reads
        // as the default and stays in the preset.
        preset.extra.retain(|(k, _)| k != field::TIP_ORDER);
        preset
            .extra
            .push((field::TIP_ORDER.into(), b"spiral".to_vec()));
        let read = Behaviours::from_preset(&preset);
        assert_eq!(read.tip_order, TipOrder::Repeat);
        let bytes = preset.to_bytes();
        let back = BrushPreset::from_bytes(&bytes).expect("read back");
        assert_eq!(back.extra(field::TIP_ORDER), Some(&b"spiral"[..]));
    }

    #[test]
    fn every_choice_reads_its_own_name_back() {
        for &c in TipOrder::ALL {
            assert_eq!(TipOrder::from_name(c.name()), Some(c));
        }
        for &c in Flip::ALL {
            assert_eq!(Flip::from_name(c.name()), Some(c));
        }
        for &c in TipBlend::ALL {
            assert_eq!(TipBlend::from_name(c.name()), Some(c));
        }
        for &c in ParticleDirection::ALL {
            assert_eq!(ParticleDirection::from_name(c.name()), Some(c));
        }
        for &c in DualMode::ALL {
            assert_eq!(DualMode::from_name(c.name()), Some(c));
        }
        for &c in ToneShape::ALL {
            assert_eq!(ToneShape::from_name(c.name()), Some(c));
        }
    }

    #[test]
    fn every_parameter_is_registered_inside_its_range() {
        let registry = registry();
        for key in [
            key::PAINT_DENSITY,
            key::AMOUNT_OF_PAINT,
            key::COLOR_STRETCH,
            key::PARTICLE_SIZE,
            key::PARTICLE_DENSITY,
            key::SPRAY_DEVIATION,
            key::DUAL_SIZE,
            key::DUAL_SPACING,
            key::DUAL_SCATTER,
            key::WATERCOLOR_RANGE,
            key::WATERCOLOR_OPACITY,
            key::WATERCOLOR_DARKNESS,
            key::WATERCOLOR_BLUR,
            key::TONE_CELL,
            key::TONE_ANGLE,
            key::SPRAY_RATE,
            key::TEXTURE_BRIGHTNESS,
            key::TEXTURE_CONTRAST,
        ] {
            let def = registry.param(key).expect(key);
            assert!(def.min <= def.default && def.default <= def.max, "{key}");
            assert!(key.starts_with("paint."), "{key}");
        }
    }
}
