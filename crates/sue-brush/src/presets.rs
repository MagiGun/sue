//! Sue's built-in brushes, written by hand as [`BrushPreset`]s.
//!
//! Every brush sub tool Sue ships (the Pen, Sketch, Brush, Airbrush,
//! Decoration and Eraser tools) is one preset here, in the order the Sub
//! Tool palette lists it. Each has an id that never changes
//! (`sue.<tool>.<name>`, [`tool_of`] reads the tool back), an English
//! name shown through the language database, and the group it is listed
//! under in its tool. They are Sue's own: tuned by drawing, never made by
//! running another program's brushes through an importer.
//!
//! Until the new engine paints on the canvas, the 0.0.75 engine paints
//! with them through [`legacy_text`], the text that engine reads.

use sue_model::BrushPreset;
use sue_model::brush::{
    AntiAlias, Binding, BrushRegistry, EngineKind, Mixing, Pick, PictureRef, Spacing, TaperMode,
    Texture, TextureSource, TipSource, input, param,
};

/// The tools whose sub tools are brushes, by the key their presets' ids
/// carry, in the tool palette's order.
pub const TOOLS: [&str; 6] = ["pen", "pencil", "brush", "airbrush", "decoration", "eraser"];

/// Fields this module stores on a preset, beside the brush model's own
/// and the paint behaviours' (`docs/brush-files.md` lists them).
pub mod field {
    /// How an eraser takes vector lines away: `touched` (only where it
    /// touches), `intersection` (up to the lines it crosses) or `whole`
    /// (every line it touches). Absent: `touched`.
    pub const VECTOR_ERASE: &str = "erase.vector";
}

/// The ways an eraser takes vector lines away, by stored name, in the
/// order the Tool Property palette lists them.
pub const VECTOR_ERASE: [&str; 3] = ["touched", "intersection", "whole"];

/// Which of [`VECTOR_ERASE`] `preset` uses, as its index (0 when it says
/// none or names one this build does not know).
#[must_use]
pub fn vector_erase(preset: &BrushPreset) -> u8 {
    preset
        .extra(field::VECTOR_ERASE)
        .and_then(|bytes| {
            VECTOR_ERASE
                .iter()
                .position(|name| name.as_bytes() == bytes)
        })
        .map_or(0, |index| index as u8)
}

/// The tool key of a built-in preset's id (`sue.pen.g_pen` is `pen`).
#[must_use]
pub fn tool_of(preset: &BrushPreset) -> Option<&str> {
    let rest = preset.id.strip_prefix("sue.")?;
    let (tool, _) = rest.split_once('.')?;
    TOOLS.contains(&tool).then_some(tool)
}

/// The built-in preset with id `id`.
#[must_use]
pub fn find(id: &str) -> Option<BrushPreset> {
    builtin().into_iter().find(|preset| preset.id == id)
}

/// The built-in presets of tool `tool` ([`TOOLS`]), in palette order.
#[must_use]
pub fn of_tool(tool: &str) -> Vec<BrushPreset> {
    builtin()
        .into_iter()
        .filter(|preset| tool_of(preset) == Some(tool))
        .collect()
}

/// A preset being written: the brush model plus the 0.0.75 settings that
/// the model keeps as bindings, a texture and a mixing, gathered until
/// [`Self::done`] lays them out the way [`BrushPreset::from_legacy_text`]
/// does.
struct Make {
    preset: BrushPreset,
    grain: f32,
    mix: f32,
    shape: Option<&'static str>,
    ribbon: bool,
    follow: bool,
    size_jitter: f32,
    turn_jitter: f32,
    // Size and opacity: pressure, speed (on, minimum), tilt.
    size_pressure: (bool, f32),
    size_speed: (bool, f32),
    opacity_pressure: bool,
}

impl Make {
    /// A brush named `name` with id `id` in group `group`, `size` pixels
    /// wide, as a 0.0.75 tip starts: weak anti-aliasing, size following
    /// pressure down to 0.15, stabilization 5 that loosens with speed.
    fn new(id: &str, name: &str, group: &str, size: f32) -> Self {
        let mut preset = BrushPreset::new(name);
        preset.id = id.to_owned();
        preset.group = group.to_owned();
        preset.set(param::SIZE, size);
        preset.spacing = Spacing::Ratio;
        preset.input.stabilization = 5;
        preset.input.by_speed = true;
        preset.input.smart_shape = true;
        Self {
            preset,
            grain: 0.0,
            mix: 0.0,
            shape: None,
            ribbon: false,
            follow: false,
            size_jitter: 0.0,
            turn_jitter: 0.0,
            size_pressure: (true, 0.15),
            size_speed: (false, 0.0),
            opacity_pressure: false,
        }
    }

    /// As Clip Studio Paint's defaults start: medium anti-aliasing.
    fn csp(id: &str, name: &str, group: &str, size: f32) -> Self {
        Self::new(id, name, group, size).aa(AntiAlias::Medium)
    }

    /// A decoration: the picture `shape` every `spacing` of the size,
    /// its size not following pressure.
    fn deco(
        id: &str,
        name: &str,
        group: &str,
        shape: &'static str,
        size: f32,
        spacing: f32,
    ) -> Self {
        let mut make = Self::new(id, name, group, size).no_size_pressure();
        make.shape = Some(shape);
        make.spacing(spacing)
    }

    fn set(mut self, key: &str, value: f32) -> Self {
        self.preset.set(key, value);
        self
    }

    fn aa(mut self, aa: AntiAlias) -> Self {
        self.preset.antialias = aa;
        self
    }

    fn opacity(self, value: f32) -> Self {
        self.set(param::OPACITY, value)
    }

    fn hardness(self, value: f32) -> Self {
        self.set(param::HARDNESS, value)
    }

    fn flow(self, value: f32) -> Self {
        self.set(param::FLOW, value)
    }

    fn spacing(self, value: f32) -> Self {
        self.set(param::SPACING, value)
    }

    fn scatter(self, value: f32) -> Self {
        self.set(param::SCATTER, value)
    }

    fn grain(mut self, value: f32) -> Self {
        self.grain = value;
        self
    }

    fn mix(mut self, value: f32) -> Self {
        self.mix = value;
        self
    }

    fn build_up(mut self) -> Self {
        self.preset.build_up = true;
        self
    }

    fn eraser(mut self) -> Self {
        self.preset.erase = true;
        self
    }

    fn at_least_one(mut self) -> Self {
        self.preset.min_width = 1.0;
        self
    }

    /// Size follows pressure down to `min`.
    fn pressure_min(mut self, min: f32) -> Self {
        self.size_pressure.1 = min;
        self
    }

    /// Size does not follow pressure.
    fn no_size_pressure(mut self) -> Self {
        self.size_pressure.0 = false;
        self
    }

    /// Size follows speed down to `min`.
    fn speed_min(mut self, min: f32) -> Self {
        self.size_speed = (true, min);
        self
    }

    /// Opacity follows pressure.
    fn opacity_pressure(mut self) -> Self {
        self.opacity_pressure = true;
        self
    }

    fn size_jitter(mut self, value: f32) -> Self {
        self.size_jitter = value;
        self
    }

    fn turn_jitter(mut self, value: f32) -> Self {
        self.turn_jitter = value;
        self
    }

    fn follow(mut self) -> Self {
        self.follow = true;
        self
    }

    fn ribbon(mut self) -> Self {
        self.ribbon = true;
        self
    }

    fn shape(mut self, shape: &'static str) -> Self {
        self.shape = Some(shape);
        self
    }

    /// Clip Studio Paint's Starting and ending: 20 pixels each way, by
    /// speed, so only a quick stroke thins at its ends.
    fn tapered(mut self) -> Self {
        self.preset.taper.start = 20.0;
        self.preset.taper.end = 20.0;
        self.preset.taper.by_speed = true;
        self
    }

    fn stabilization(mut self, level: u32) -> Self {
        self.preset.input.stabilization = level;
        self
    }

    fn flick(mut self) -> Self {
        self.preset.input.flick = true;
        self
    }

    fn vector_erase(mut self, index: usize) -> Self {
        self.preset.extra.push((
            field::VECTOR_ERASE.to_owned(),
            VECTOR_ERASE[index].as_bytes().to_vec(),
        ));
        self
    }

    /// The preset, laid out as [`BrushPreset::from_legacy_text`] lays out
    /// the same settings: every parameter of 0.0.75 written, the bindings
    /// in its order, the engine its rule picks.
    fn done(self) -> BrushPreset {
        let mut preset = self.preset;
        let registry = BrushRegistry::standard();
        for key in [
            param::OPACITY,
            param::HARDNESS,
            param::SPACING,
            param::FLOW,
            param::ANGLE,
            param::SCATTER,
        ] {
            if preset.value(key).is_none()
                && let Some(value) = registry.value(&preset, key)
            {
                preset.set(key, value);
            }
        }
        if self.grain > 0.0 {
            preset.texture = Some(Texture::new(TextureSource::Grain));
            preset.set(param::TEXTURE_STRENGTH, self.grain);
        }
        if self.mix > 0.0 {
            preset.mix = Some(Mixing::new(self.mix));
        }
        let (on, min) = self.size_pressure;
        if on || min != 0.0 {
            preset.dynamics.push(
                Binding::new(param::SIZE, input::PRESSURE)
                    .range(min, 1.0)
                    .switched(on),
            );
        }
        if self.size_speed.0 {
            preset
                .dynamics
                .push(Binding::new(param::SIZE, input::SPEED).range(self.size_speed.1, 1.0));
        }
        if self.opacity_pressure {
            preset
                .dynamics
                .push(Binding::new(param::OPACITY, input::PRESSURE));
        }
        if self.size_jitter > 0.0 {
            preset.dynamics.push(
                Binding::new(param::SIZE, input::RANDOM)
                    .range(1.0, 1.0 - self.size_jitter.min(1.0)),
            );
        }
        if self.follow {
            preset.dynamics.push(
                Binding::new(param::ANGLE, input::DIRECTION)
                    .range(0.0, 360.0)
                    .added(),
            );
        }
        if self.turn_jitter > 0.0 {
            let reach = self.turn_jitter.min(1.0) * 180.0;
            preset.dynamics.push(
                Binding::new(param::ANGLE, input::RANDOM)
                    .range(-reach, reach)
                    .added(),
            );
        }
        preset.tip = match self.shape {
            Some(name) => TipSource::Images {
                pictures: vec![PictureRef::Named(name.to_owned())],
                pick: Pick::Random,
                keep_colors: false,
            },
            None => TipSource::Round,
        };
        preset.engine = match (&preset.tip, self.ribbon) {
            (TipSource::Images { .. }, true) => EngineKind::Ribbon,
            (TipSource::Images { .. }, false) => EngineKind::Dab,
            (TipSource::Round, _) if preset.build_up || self.mix > 0.0 => EngineKind::Dab,
            (TipSource::Round, _) => EngineKind::Stroke,
        };
        preset
    }
}

/// Every built-in brush, tool by tool in [`TOOLS`] order, each tool's in
/// the order its Sub Tool palette lists them.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn builtin() -> Vec<BrushPreset> {
    use AntiAlias::{None as Hard, Strong, Weak};
    let pen = |id: &str, name: &str, size: f32| Make::csp(id, name, "Pen", size);
    let marker = |id: &str, name: &str, size: f32| Make::csp(id, name, "Marker", size);
    let pencil = |id: &str, name: &str, size: f32| Make::csp(id, name, "Pencil", size);
    let pastel = |id: &str, name: &str, size: f32| Make::csp(id, name, "Pastel", size);
    let watercolor = |id: &str, name: &str, size: f32| Make::csp(id, name, "Watercolor", size);
    let thick = |id: &str, name: &str, size: f32| Make::csp(id, name, "Thick paint", size);
    let ink = |id: &str, name: &str, size: f32| Make::csp(id, name, "Ink", size);
    let airbrush = |id: &str, name: &str, size: f32| Make::csp(id, name, "Airbrush", size);
    let eraser = |id: &str, name: &str, size: f32| Make::csp(id, name, "Eraser", size).eraser();
    let effect = |id: &str, name: &str, shape: &'static str, size: f32, spacing: f32| {
        Make::deco(id, name, "Effect", shape, size, spacing)
    };
    let flower = |id: &str, name: &str, shape: &'static str, size: f32, spacing: f32| {
        Make::deco(id, name, "Flower", shape, size, spacing)
    };
    let clothing = |id: &str, name: &str, shape: &'static str, size: f32, spacing: f32| {
        Make::deco(id, name, "Clothing", shape, size, spacing)
    };
    let lines = |id: &str, name: &str, shape: &'static str, size: f32, spacing: f32| {
        Make::deco(id, name, "Lines", shape, size, spacing)
    };
    // Shapes scattered about the stroke, each turned and sized its own way.
    let scattered = |make: Make| make.scatter(1.0).size_jitter(0.55).turn_jitter(1.0);

    vec![
        // The Pen tool: Clip Studio Paint's pen sub tools in its order,
        // with its default settings (size, opacity, hardness, density,
        // anti-aliasing, stabilization, and starting and ending by
        // speed, so a slow line keeps its size from the first point).
        pen("sue.pen.g_pen", "G-pen", 10.0)
            .tapered()
            .stabilization(10)
            .flick(),
        pen("sue.pen.grainy_g_pen", "Grainy G-pen", 10.0)
            .pressure_min(0.05)
            .grain(0.08)
            .stabilization(10),
        pen("sue.pen.mapping_pen", "Mapping pen", 10.0)
            .tapered()
            .pressure_min(0.05)
            .stabilization(10)
            .flick(),
        pen("sue.pen.turnip_pen", "Turnip pen", 8.0)
            .tapered()
            .pressure_min(0.3)
            .stabilization(10)
            .flick(),
        pen("sue.pen.textured_pen", "Textured pen", 12.0)
            .hardness(0.6)
            .aa(Weak)
            .pressure_min(0.2)
            .grain(0.35)
            .stabilization(10),
        // 1.2 mm in Clip Studio, at 350 dpi.
        pen("sue.pen.rough_pen", "Rough pen", 17.0)
            .aa(Weak)
            .pressure_min(0.2)
            .grain(0.6)
            .stabilization(10),
        pen("sue.pen.calligraphy", "Calligraphy", 50.0)
            .tapered()
            .aa(Weak)
            .pressure_min(0.15)
            .speed_min(0.4)
            .stabilization(10),
        marker("sue.pen.fineliner", "Fineliner", 10.0)
            .tapered()
            .no_size_pressure()
            .stabilization(10),
        marker("sue.pen.felt_pen", "Felt pen", 20.0)
            .aa(Weak)
            .no_size_pressure()
            .stabilization(10),
        marker("sue.pen.dot_pen", "Dot pen", 1.0)
            .no_size_pressure()
            .aa(Hard)
            .at_least_one(),
        marker("sue.pen.flat_marker", "Flat marker", 50.0)
            .hardness(0.7)
            .aa(Weak)
            .no_size_pressure()
            .stabilization(10),
        // The Sketch tool (once the Pencil tool), with Clip Studio
        // Paint's default settings; its paper textures are Sue's grain.
        pencil("sue.pencil.pencil", "Pencil", 10.0)
            .opacity(0.9)
            .flow(0.85)
            .aa(Weak)
            .grain(0.45)
            .pressure_min(0.4)
            .opacity_pressure()
            .stabilization(10),
        pencil("sue.pencil.mechanical_pencil", "Mechanical pencil", 5.0)
            .tapered()
            .flow(0.85)
            .aa(Weak)
            .grain(0.3)
            .opacity_pressure()
            .stabilization(10),
        pencil("sue.pencil.charcoal", "Charcoal", 20.0)
            .aa(Weak)
            .grain(0.8)
            .pressure_min(0.5)
            .opacity_pressure()
            .stabilization(10),
        pastel("sue.pencil.crayon", "Crayon", 30.0)
            .tapered()
            .flow(0.8)
            .aa(Weak)
            .grain(0.65)
            .mix(0.8)
            .pressure_min(0.6)
            .opacity_pressure()
            .stabilization(10),
        pastel("sue.pencil.pastel", "Pastel", 40.0)
            .tapered()
            .hardness(0.01)
            .flow(0.5)
            .aa(Hard)
            .grain(0.5)
            .mix(0.7)
            .pressure_min(0.6)
            .opacity_pressure()
            .stabilization(10),
        pastel("sue.pencil.chalk", "Chalk", 30.0)
            .hardness(0.81)
            .aa(Weak)
            .grain(0.75)
            .mix(0.7)
            .pressure_min(0.7)
            .opacity_pressure()
            .stabilization(10),
        // The Brush tool: watercolor, thick paint and ink brushes, with
        // Clip Studio Paint's default settings.
        watercolor("sue.brush.round_watercolor", "Round watercolor brush", 80.0)
            .hardness(0.3)
            .flow(0.6)
            .build_up()
            .mix(0.3)
            .aa(Weak)
            .stabilization(2),
        watercolor("sue.brush.flat_watercolor", "Flat watercolor brush", 100.0)
            .hardness(0.6)
            .flow(0.6)
            .build_up()
            .mix(0.25)
            .aa(Weak)
            .stabilization(10),
        thick("sue.brush.gouache", "Gouache", 80.0)
            .flow(0.75)
            .mix(0.8)
            .aa(Hard)
            .pressure_min(0.6)
            .stabilization(2),
        thick("sue.brush.oil_paint", "Oil paint", 80.0)
            .mix(0.61)
            .aa(Weak)
            .grain(0.12)
            .pressure_min(0.6)
            .stabilization(2),
        thick("sue.brush.thick_oil_paint", "Thick oil paint", 60.0)
            .flow(0.85)
            .mix(0.85)
            .aa(Weak)
            .grain(0.3)
            .pressure_min(0.6)
            .stabilization(2),
        watercolor("sue.brush.wet_wash", "Wet wash", 30.0)
            .hardness(0.4)
            .build_up()
            .flow(0.5)
            .mix(0.7)
            .aa(Hard)
            .stabilization(10),
        ink("sue.brush.brush_pen", "Brush pen", 70.0)
            .tapered()
            .pressure_min(0.02)
            .stabilization(10)
            .flick(),
        ink("sue.brush.dry_ink", "Dry ink", 60.0)
            .aa(Strong)
            .grain(0.5)
            .pressure_min(0.1)
            .stabilization(10),
        ink("sue.brush.watery_ink", "Watery ink", 100.0)
            .hardness(0.5)
            .aa(Strong)
            .grain(0.35)
            .opacity_pressure()
            .stabilization(10),
        thick("sue.brush.pointillism", "Pointillism", 80.0)
            .mix(0.5)
            .aa(Weak)
            .spacing(1.2)
            .scatter(0.6)
            .size_jitter(0.6)
            .shape("Dot")
            .no_size_pressure()
            .stabilization(6),
        // The Airbrush tool, with Clip Studio Paint's defaults; its spray
        // particles are scattered dots.
        airbrush("sue.airbrush.soft", "Soft", 150.0)
            .hardness(0.0)
            .build_up()
            .flow(0.06)
            .no_size_pressure()
            .opacity_pressure(),
        // Clip Studio's spray is 200 wide: 6 pixel drops scattered over it.
        airbrush("sue.airbrush.spray", "Spray", 6.0)
            .build_up()
            .spacing(0.08)
            .scatter(16.0)
            .size_jitter(0.8)
            .shape("Dot")
            .no_size_pressure()
            .opacity_pressure(),
        airbrush("sue.airbrush.droplet", "Droplet", 30.0)
            .spacing(2.0)
            .scatter(3.0)
            .size_jitter(0.85)
            .shape("Dot")
            .no_size_pressure(),
        // The Decoration tool: shapes scattered about the stroke, or
        // running along it and turning with it, in groups as Clip Studio
        // Paint lists its own (effect, flower, clothing, lines).
        scattered(effect("sue.decoration.stars", "Stars", "Star", 36.0, 1.4)),
        scattered(effect(
            "sue.decoration.hearts",
            "Hearts",
            "Heart",
            34.0,
            1.4,
        )),
        effect("sue.decoration.sparkles", "Sparkles", "Sparkle", 40.0, 1.3)
            .scatter(1.2)
            .size_jitter(0.7),
        effect("sue.decoration.snow", "Snow", "Snowflake", 30.0, 1.6)
            .scatter(1.6)
            .size_jitter(0.65)
            .turn_jitter(1.0),
        flower("sue.decoration.flowers", "Flowers", "Flower", 40.0, 1.15)
            .scatter(0.7)
            .size_jitter(0.4)
            .turn_jitter(1.0),
        flower("sue.decoration.leaves", "Leaves", "Leaf", 34.0, 0.8)
            .follow()
            .scatter(0.35)
            .size_jitter(0.35)
            .turn_jitter(0.2),
        effect("sue.decoration.bubbles", "Bubbles", "Bubble", 36.0, 1.3)
            .scatter(1.4)
            .size_jitter(0.75),
        clothing("sue.decoration.chain", "Chain", "Chain link", 26.0, 0.72).follow(),
        clothing("sue.decoration.lace", "Lace", "Scallop", 30.0, 0.9).follow(),
        clothing("sue.decoration.stitches", "Stitches", "Stitch", 16.0, 1.5).follow(),
        lines("sue.decoration.dotted_line", "Dotted line", "Dot", 8.0, 2.0),
        lines(
            "sue.decoration.dashed_line",
            "Dashed line",
            "Stitch",
            22.0,
            1.25,
        )
        .follow(),
        // Ribbons: one band bent along the whole stroke.
        clothing("sue.decoration.rope", "Rope", "Rope", 24.0, 1.0).ribbon(),
        clothing("sue.decoration.rickrack", "Rickrack", "Rickrack", 24.0, 1.0).ribbon(),
        lines(
            "sue.decoration.film_strip",
            "Film strip",
            "Film strip",
            40.0,
            0.8,
        )
        .ribbon(),
        lines(
            "sue.decoration.checkered_band",
            "Checkered band",
            "Checkered band",
            20.0,
            1.0,
        )
        .ribbon(),
        clothing(
            "sue.decoration.stitched_ribbon",
            "Stitched ribbon",
            "Stitched ribbon",
            26.0,
            1.0,
        )
        .ribbon(),
        clothing(
            "sue.decoration.lace_ribbon",
            "Lace ribbon",
            "Scallop",
            26.0,
            1.0,
        )
        .ribbon(),
        // The Eraser tool, with Clip Studio Paint's defaults.
        eraser("sue.eraser.hard", "Hard", 30.0)
            .no_size_pressure()
            .stabilization(10),
        eraser("sue.eraser.soft", "Soft", 50.0)
            .hardness(0.01)
            .build_up()
            .flow(0.3)
            .no_size_pressure()
            .stabilization(10),
        eraser("sue.eraser.rough", "Rough", 100.0)
            .grain(0.6)
            .stabilization(10),
        // Clip Studio Paint's Vector eraser: up to intersection.
        eraser("sue.eraser.vector", "Vector", 3.0)
            .aa(Hard)
            .no_size_pressure()
            .vector_erase(1)
            .stabilization(10),
        eraser("sue.eraser.kneaded", "Kneaded eraser", 50.0)
            .opacity(0.9)
            .flow(0.75)
            .build_up()
            .grain(0.5)
            .aa(Weak)
            .opacity_pressure()
            .stabilization(2),
    ]
    .into_iter()
    .map(Make::done)
    .collect()
}

/// `preset` as the text the 0.0.75 engine reads (`Tip::to_text` of
/// `sue-picture`, `key=value` pairs joined by semicolons), every key in
/// the order that engine writes them. [`BrushPreset::from_legacy_text`]
/// reads it back. What the old engine has no word for is left out.
#[must_use]
pub fn legacy_text(preset: &BrushPreset) -> String {
    let registry = BrushRegistry::standard();
    let value = |key: &str| registry.value(preset, key).unwrap_or(0.0);
    let flag = |on: bool| if on { "1" } else { "0" }.to_owned();
    let legacy = |key: &str| {
        preset
            .extra(&format!("{}{key}", BrushPreset::LEGACY))
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
    };
    // A size binding to random that runs from 1 down is a jitter, not a
    // dynamic of 0.0.75's own.
    let jitter = |binding: &Binding| {
        binding.param == param::SIZE
            && binding.input == input::RANDOM
            && binding.min == 1.0
            && binding.max < 1.0
    };
    let find = |param: &str, input: &str| {
        preset
            .dynamics
            .iter()
            .find(|binding| binding.param == param && binding.input == input && !jitter(binding))
    };
    let added = |input: &str| {
        preset.dynamics.iter().find(|binding| {
            binding.param == param::ANGLE
                && binding.input == input
                && binding.combine == sue_model::brush::Combine::Add
                && binding.on
        })
    };
    let (shape, keep_colors, count) = match &preset.tip {
        TipSource::Images {
            pictures,
            keep_colors,
            ..
        } => (
            pictures.first().and_then(|picture| match picture {
                PictureRef::Named(name) => Some(name.as_str()),
                PictureRef::Hash(_) => None,
            }),
            *keep_colors,
            pictures.len().max(1),
        ),
        TipSource::Round => (None, legacy("keep_colors") == Some("1"), 1),
    };
    let count = legacy("shape_count")
        .and_then(|text| text.parse::<usize>().ok())
        .unwrap_or(count);
    let ribbon = preset.engine == EngineKind::Ribbon || legacy("ribbon") == Some("1");
    let taper = &preset.taper;
    let color = &preset.color;
    let mut out: Vec<(String, String)> = vec![
        ("size".into(), value(param::SIZE).to_string()),
        ("opacity".into(), value(param::OPACITY).to_string()),
        ("hardness".into(), value(param::HARDNESS).to_string()),
        (
            "aa".into(),
            match preset.antialias {
                AntiAlias::None => "0",
                AntiAlias::Weak => "1",
                AntiAlias::Medium => "2",
                AntiAlias::Strong => "3",
            }
            .into(),
        ),
        ("spacing".into(), value(param::SPACING).to_string()),
        ("build_up".into(), flag(preset.build_up)),
        ("flow".into(), value(param::FLOW).to_string()),
        (
            "grain".into(),
            if preset.texture.is_some() {
                value(param::TEXTURE_STRENGTH)
            } else {
                0.0
            }
            .to_string(),
        ),
        ("screen_size".into(), flag(preset.size_on_screen)),
        ("at_least_one".into(), flag(preset.min_width >= 1.0)),
        ("eraser".into(), flag(preset.erase)),
        ("taper_start".into(), taper.start.to_string()),
        ("taper_end".into(), taper.end.to_string()),
        (
            "taper_mode".into(),
            match taper.mode {
                TaperMode::Length => "0",
                TaperMode::Percent => "1",
                TaperMode::Fade => "2",
            }
            .into(),
        ),
        ("taper_size".into(), flag(taper.size)),
        ("taper_opacity".into(), flag(taper.opacity)),
        ("taper_by_speed".into(), flag(taper.by_speed)),
        (
            "mix".into(),
            preset.mix.map_or(0.0, |mix| mix.amount).to_string(),
        ),
        ("follow".into(), flag(added(input::DIRECTION).is_some())),
        (
            "follow_tilt".into(),
            flag(added(input::TILT_DIRECTION).is_some()),
        ),
        ("angle".into(), value(param::ANGLE).to_string()),
        ("scatter".into(), value(param::SCATTER).to_string()),
        (
            "size_jitter".into(),
            preset
                .dynamics
                .iter()
                .find(|binding| jitter(binding))
                .map_or(0.0, |binding| {
                    shortest(
                        1.0 - binding.max,
                        |jitter| 1.0 - jitter.min(1.0),
                        binding.max,
                    )
                })
                .to_string(),
        ),
        (
            "turn_jitter".into(),
            added(input::RANDOM)
                .map_or(0.0, |binding| {
                    shortest(
                        binding.max / 180.0,
                        |jitter| jitter.min(1.0) * 180.0,
                        binding.max,
                    )
                })
                .to_string(),
        ),
        ("ribbon".into(), flag(ribbon)),
        ("keep_colors".into(), flag(keep_colors)),
        ("shape_count".into(), count.to_string()),
        ("jitter.tip".into(), flag(color.tip)),
        ("jitter.hue".into(), color.hue.to_string()),
        ("jitter.saturation".into(), color.saturation.to_string()),
        ("jitter.value".into(), color.luminosity.to_string()),
        ("jitter.sub".into(), color.sub.to_string()),
        ("jitter.stroke".into(), flag(color.stroke)),
        ("jitter.stroke_hue".into(), color.stroke_hue.to_string()),
        (
            "jitter.stroke_saturation".into(),
            color.stroke_saturation.to_string(),
        ),
        (
            "jitter.stroke_value".into(),
            color.stroke_luminosity.to_string(),
        ),
    ];
    for (name, param) in [("size_by", param::SIZE), ("opacity_by", param::OPACITY)] {
        for (legacy, input) in [
            ("pressure", input::PRESSURE),
            ("velocity", input::SPEED),
            ("tilt", input::TILT),
            ("random", input::RANDOM),
        ] {
            let binding = find(param, input)
                .filter(|binding| binding.combine != sue_model::brush::Combine::Add);
            out.push((
                format!("{name}.{legacy}"),
                flag(binding.is_some_and(|binding| binding.on)),
            ));
            out.push((
                format!("{name}.{legacy}_min"),
                binding.map_or(0.0, |binding| binding.min).to_string(),
            ));
            if legacy != "random" {
                out.push((
                    format!("{name}.{legacy}_curve"),
                    binding
                        .map_or(sue_model::Response::LINEAR, |binding| binding.curve)
                        .to_string(),
                ));
            }
        }
    }
    if let Some(name) = shape {
        out.push(("shape".into(), name.replace([';', '='], " ")));
    }
    out.iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(";")
}

/// The number with the fewest decimals near `guess` that `rule` turns
/// into exactly `target`, so a setting the old text held as `0.4` and
/// the model as `1 - 0.4` reads `0.4` again; `guess` when none does.
fn shortest(guess: f32, rule: impl Fn(f32) -> f32, target: f32) -> f32 {
    (1..=9)
        .map(|digits| {
            // Tens multiplied, not the system's powi: the replay scan
            // keeps system math out of sue-brush.
            let scale = (0..digits).fold(1.0_f64, |scale, _| scale * 10.0);
            ((f64::from(guess) * scale).round() / scale) as f32
        })
        .find(|candidate| rule(*candidate) == target)
        .unwrap_or(guess)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The brush of every built-in sub tool of 0.0.75, as that Sue wrote
    /// it (the golden file of package P0-4): tool, name, text.
    const OLD: &str = include_str!("../../sue-paint/tests/golden/tips.txt");

    /// The 0.0.75 label of each tool key.
    fn label(tool: &str) -> &'static str {
        match tool {
            "pen" => "Pen",
            "pencil" => "Sketch",
            "brush" => "Brush",
            "airbrush" => "Airbrush",
            "decoration" => "Decoration",
            _ => "Eraser",
        }
    }

    #[test]
    fn every_built_in_brush_paints_as_its_0_0_75_sub_tool() {
        let presets = builtin();
        let mut matched = 0;
        for line in OLD.lines() {
            let mut parts = line.splitn(3, '\t');
            let (Some(tool), Some(name), Some(text)) = (parts.next(), parts.next(), parts.next())
            else {
                panic!("not a line of tips.txt: {line}");
            };
            // P3-5 renamed two pens after Clip Studio Paint Ver. 4; their
            // brushes are the same.
            let name = match name {
                "Real G-Pen" => "Grainy G-pen",
                "Milli pen" => "Fineliner",
                other => other,
            };
            let Some(preset) = presets
                .iter()
                .find(|preset| preset.name == name && tool_of(preset).map(label) == Some(tool))
            else {
                continue;
            };
            assert_eq!(legacy_text(preset), text, "{name} ({tool})");
            matched += 1;
        }
        assert_eq!(matched, presets.len(), "every preset was a 0.0.75 brush");
    }

    #[test]
    fn the_legacy_text_reads_back_to_the_same_preset() {
        for preset in builtin() {
            let mut back = BrushPreset::from_legacy_text(&legacy_text(&preset));
            back.id.clone_from(&preset.id);
            back.name.clone_from(&preset.name);
            back.group.clone_from(&preset.group);
            back.input = preset.input;
            back.extra.extend(preset.extra.iter().cloned());
            assert_eq!(back, preset, "{}", preset.id);
        }
    }

    #[test]
    fn ids_are_unique_stable_and_name_their_tool() {
        let presets = builtin();
        let mut ids: Vec<&str> = presets.iter().map(|preset| preset.id.as_str()).collect();
        assert_eq!(ids.len(), 53);
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), presets.len(), "no id twice");
        for preset in &presets {
            assert!(tool_of(preset).is_some(), "{}", preset.id);
            assert!(!preset.group.is_empty(), "{}", preset.id);
            assert!(
                preset.id[4..]
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c == '.'),
                "{}",
                preset.id
            );
        }
        // Tools in palette order, never interleaved.
        let tools: Vec<&str> = presets.iter().filter_map(tool_of).collect();
        let mut seen: Vec<&str> = tools.clone();
        seen.dedup();
        assert_eq!(seen, TOOLS);
        assert_eq!(
            find("sue.pen.g_pen").map(|preset| preset.name),
            Some("G-pen".into())
        );
        assert_eq!(of_tool("airbrush").len(), 3);
    }

    #[test]
    fn every_built_in_brush_round_trips_its_bytes() {
        for preset in builtin() {
            assert_eq!(
                BrushPreset::from_bytes(&preset.to_bytes()).as_ref(),
                Ok(&preset),
                "{}",
                preset.id
            );
        }
    }

    #[test]
    fn the_vector_eraser_says_how_it_erases() {
        assert_eq!(vector_erase(&find("sue.eraser.vector").unwrap()), 1);
        assert_eq!(vector_erase(&find("sue.eraser.hard").unwrap()), 0);
    }
}
