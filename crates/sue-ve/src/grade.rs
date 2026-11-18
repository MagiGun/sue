//! Color grading, as DaVinci Resolve's Color page does it: a serial list
//! of correctors per clip (Resolve's nodes, one after another), each with
//! lift, gamma, gain and offset wheels, contrast about a pivot,
//! saturation, temperature and tint, custom curves (master and per
//! channel), hue against hue, saturation and luminance curves, a LUT, and
//! a qualifier (an HSL key) that limits the corrector to what it keys.
//!
//! A grade is a pure function of a pixel's color, so it is baked into a
//! 3D table ([`Baked`], 33 points a side) once per grade and applied to
//! every pixel by trilinear lookup, which is fast enough for playback;
//! [`Grade::eval`] is the exact function the table samples and the tests
//! compare with.
//!
//! A clip keeps its grade in its properties ([`crate::fx::key::GRADE`])
//! as a list of maps. A LUT is imported from a `.cube` file into the
//! media pool as an entity of class [`LUT_CLASS`] holding the table, and
//! correctors name it by [`EntityId`] (rule 1).

use std::collections::BTreeMap;

use sue_core::{Arena, Entity, EntityId, Value};
use sue_model::Clip;

/// The class of an imported LUT.
pub const LUT_CLASS: &str = "ve.lut";
/// A LUT entity's side (points along each axis).
pub const LUT_SIZE: &str = "lut.size";
/// A LUT entity's table: red, green, blue as 32-bit floats, red changing
/// fastest, as `.cube` lists them.
pub const LUT_DATA: &str = "lut.data";

/// A qualifier: which colors a corrector touches.
#[derive(Clone, Debug, PartialEq)]
pub struct Qualifier {
    /// The hue in the middle of the range, 0 to 1 around the wheel.
    pub hue: f32,
    /// How wide the hue range is, 0 to 1 (1: every hue).
    pub hue_width: f32,
    /// Saturation from, to (0 to 1).
    pub saturation: [f32; 2],
    /// Luminance from, to (0 to 1).
    pub luminance: [f32; 2],
    /// How soft the edges of every range are.
    pub softness: f32,
    /// Everything but what it keys.
    pub invert: bool,
}

impl Default for Qualifier {
    fn default() -> Self {
        Self {
            hue: 0.0,
            hue_width: 1.0,
            saturation: [0.0, 1.0],
            luminance: [0.0, 1.0],
            softness: 0.05,
            invert: false,
        }
    }
}

/// One corrector.
#[derive(Clone, Debug, PartialEq)]
pub struct Corrector {
    /// On (else bypassed).
    pub enabled: bool,
    /// Lift: red, green, blue and master, 0 is none (-1 to 1).
    pub lift: [f32; 4],
    /// Gamma, as `lift`.
    pub gamma: [f32; 4],
    /// Gain, as `lift`.
    pub gain: [f32; 4],
    /// Offset, as `lift`.
    pub offset: [f32; 4],
    /// Contrast, 1 is none.
    pub contrast: f32,
    /// The tone contrast turns about.
    pub pivot: f32,
    /// Saturation, 1 is none.
    pub saturation: f32,
    /// Warmer above 0, cooler below (-100 to 100).
    pub temperature: f32,
    /// Toward magenta above 0, green below (-100 to 100).
    pub tint: f32,
    /// Custom curves: master, red, green, blue (points 0 to 1).
    pub curves: [Vec<[f32; 2]>; 4],
    /// Hue against hue: x a hue, y the turn (0.5 is none).
    pub hue_hue: Vec<[f32; 2]>,
    /// Hue against saturation: y the saturation's scale (0.5 is none).
    pub hue_sat: Vec<[f32; 2]>,
    /// Hue against luminance: y the luminance's change (0.5 is none).
    pub hue_lum: Vec<[f32; 2]>,
    /// The qualifier, when it has one.
    pub qualifier: Option<Qualifier>,
    /// An imported LUT applied at the end.
    pub lut: Option<EntityId>,
    /// How much of the LUT, 0 to 1.
    pub lut_mix: f32,
}

impl Default for Corrector {
    fn default() -> Self {
        Self {
            enabled: true,
            lift: [0.0; 4],
            gamma: [0.0; 4],
            gain: [0.0; 4],
            offset: [0.0; 4],
            contrast: 1.0,
            pivot: 0.435,
            saturation: 1.0,
            temperature: 0.0,
            tint: 0.0,
            curves: std::array::from_fn(|_| straight()),
            hue_hue: Vec::new(),
            hue_sat: Vec::new(),
            hue_lum: Vec::new(),
            qualifier: None,
            lut: None,
            lut_mix: 1.0,
        }
    }
}

fn straight() -> Vec<[f32; 2]> {
    vec![[0.0, 0.0], [1.0, 1.0]]
}

fn vec4(value: [f32; 4]) -> Value {
    Value::Vec4(value.map(f64::from))
}

fn points(list: &[[f32; 2]]) -> Value {
    crate::fx::curve_value(list)
}

impl Corrector {
    /// `true` when it changes nothing.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        let defaults = Self::default();
        Self {
            enabled: true,
            ..self.clone()
        } == defaults
    }

    /// As it is stored.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut map = BTreeMap::new();
        map.insert("on".to_owned(), Value::Bool(self.enabled));
        map.insert("lift".to_owned(), vec4(self.lift));
        map.insert("gamma".to_owned(), vec4(self.gamma));
        map.insert("gain".to_owned(), vec4(self.gain));
        map.insert("offset".to_owned(), vec4(self.offset));
        for (key, value) in [
            ("contrast", self.contrast),
            ("pivot", self.pivot),
            ("saturation", self.saturation),
            ("temperature", self.temperature),
            ("tint", self.tint),
            ("lut_mix", self.lut_mix),
        ] {
            map.insert(key.to_owned(), Value::Float(f64::from(value)));
        }
        for (key, curve) in ["curve_master", "curve_red", "curve_green", "curve_blue"]
            .iter()
            .zip(&self.curves)
        {
            map.insert((*key).to_owned(), points(curve));
        }
        map.insert("hue_hue".to_owned(), points(&self.hue_hue));
        map.insert("hue_sat".to_owned(), points(&self.hue_sat));
        map.insert("hue_lum".to_owned(), points(&self.hue_lum));
        if let Some(q) = &self.qualifier {
            let mut qualifier = BTreeMap::new();
            qualifier.insert(
                "hsl".to_owned(),
                Value::List(
                    [
                        q.hue,
                        q.hue_width,
                        q.saturation[0],
                        q.saturation[1],
                        q.luminance[0],
                        q.luminance[1],
                        q.softness,
                    ]
                    .iter()
                    .map(|v| Value::Float(f64::from(*v)))
                    .collect(),
                ),
            );
            qualifier.insert("invert".to_owned(), Value::Bool(q.invert));
            map.insert("qualifier".to_owned(), Value::Map(qualifier));
        }
        if let Some(lut) = self.lut {
            map.insert("lut".to_owned(), Value::Entity(lut));
        }
        Value::Map(map)
    }

    /// Read back from what is stored.
    #[must_use]
    pub fn from_value(value: &Value) -> Option<Self> {
        let Value::Map(map) = value else {
            return None;
        };
        let mut out = Self::default();
        let four = |key: &str| match map.get(key) {
            Some(Value::Vec4(v)) => Some(v.map(|x| x as f32)),
            _ => None,
        };
        let number = |key: &str| map.get(key).and_then(Value::as_f64).map(|v| v as f32);
        let curve = |key: &str| {
            map.get(key)
                .map(crate::fx::points_of)
                .filter(|points| points.len() >= 2)
        };
        out.enabled = map.get("on").and_then(Value::as_bool).unwrap_or(true);
        out.lift = four("lift").unwrap_or(out.lift);
        out.gamma = four("gamma").unwrap_or(out.gamma);
        out.gain = four("gain").unwrap_or(out.gain);
        out.offset = four("offset").unwrap_or(out.offset);
        out.contrast = number("contrast").unwrap_or(out.contrast);
        out.pivot = number("pivot").unwrap_or(out.pivot);
        out.saturation = number("saturation").unwrap_or(out.saturation);
        out.temperature = number("temperature").unwrap_or(0.0);
        out.tint = number("tint").unwrap_or(0.0);
        out.lut_mix = number("lut_mix").unwrap_or(1.0);
        for (index, key) in ["curve_master", "curve_red", "curve_green", "curve_blue"]
            .iter()
            .enumerate()
        {
            if let Some(points) = curve(key) {
                out.curves[index] = points;
            }
        }
        let hues = |key: &str| map.get(key).map(crate::fx::points_of).unwrap_or_default();
        out.hue_hue = hues("hue_hue");
        out.hue_sat = hues("hue_sat");
        out.hue_lum = hues("hue_lum");
        if let Some(Value::Map(q)) = map.get("qualifier") {
            let hsl: Vec<f32> = match q.get("hsl") {
                Some(Value::List(list)) => list
                    .iter()
                    .filter_map(Value::as_f64)
                    .map(|v| v as f32)
                    .collect(),
                _ => Vec::new(),
            };
            if hsl.len() == 7 {
                out.qualifier = Some(Qualifier {
                    hue: hsl[0],
                    hue_width: hsl[1],
                    saturation: [hsl[2], hsl[3]],
                    luminance: [hsl[4], hsl[5]],
                    softness: hsl[6],
                    invert: q.get("invert").and_then(Value::as_bool).unwrap_or(false),
                });
            }
        }
        out.lut = map.get("lut").and_then(Value::as_entity);
        Some(out)
    }
}

/// The grade on `clip`: its correctors, in order.
#[must_use]
pub fn grade_of(clip: &Clip) -> Vec<Corrector> {
    match clip.properties.get(crate::fx::key::GRADE) {
        Some(Value::List(list)) => list.iter().filter_map(Corrector::from_value).collect(),
        _ => Vec::new(),
    }
}

/// Sets `clip`'s grade (none removes it).
pub fn set_grade(clip: &mut Clip, grade: &[Corrector]) {
    if grade.is_empty() {
        clip.properties.remove(crate::fx::key::GRADE);
    } else {
        clip.properties.set(
            crate::fx::key::GRADE,
            Value::List(grade.iter().map(Corrector::to_value).collect()),
        );
    }
}

/// A 3D table: `size` points a side, red fastest.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    /// Points along each axis.
    pub size: usize,
    /// `size`³ colors.
    pub data: Vec<[f32; 3]>,
}

impl Table {
    /// The identity table.
    #[must_use]
    pub fn identity(size: usize) -> Self {
        let size = size.max(2);
        let step = 1.0 / (size - 1) as f32;
        let mut data = Vec::with_capacity(size * size * size);
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    data.push([r as f32 * step, g as f32 * step, b as f32 * step]);
                }
            }
        }
        Self { size, data }
    }

    /// Looks `rgb` up (0 to 1, clamped) with trilinear interpolation.
    #[must_use]
    pub fn sample(&self, rgb: [f32; 3]) -> [f32; 3] {
        let n = self.size;
        if n < 2 || self.data.len() < n * n * n {
            return rgb;
        }
        let scale = (n - 1) as f32;
        let mut base = [0_usize; 3];
        let mut frac = [0.0_f32; 3];
        for channel in 0..3 {
            let x = rgb[channel].clamp(0.0, 1.0) * scale;
            let i = (x.floor() as usize).min(n - 2);
            base[channel] = i;
            frac[channel] = x - i as f32;
        }
        let at = |r: usize, g: usize, b: usize| self.data[(b * n + g) * n + r];
        let mut out = [0.0_f32; 3];
        for (dr, dg, db) in [
            (0, 0, 0),
            (1, 0, 0),
            (0, 1, 0),
            (1, 1, 0),
            (0, 0, 1),
            (1, 0, 1),
            (0, 1, 1),
            (1, 1, 1),
        ] {
            let weight = (if dr == 1 { frac[0] } else { 1.0 - frac[0] })
                * (if dg == 1 { frac[1] } else { 1.0 - frac[1] })
                * (if db == 1 { frac[2] } else { 1.0 - frac[2] });
            if weight == 0.0 {
                continue;
            }
            let value = at(base[0] + dr, base[1] + dg, base[2] + db);
            for channel in 0..3 {
                out[channel] += value[channel] * weight;
            }
        }
        out
    }

    /// The table as a LUT entity's properties' bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.data
            .iter()
            .flat_map(|rgb| rgb.iter().flat_map(|v| v.to_le_bytes()))
            .collect()
    }

    /// A table from a LUT entity's bytes.
    #[must_use]
    pub fn from_bytes(size: usize, bytes: &[u8]) -> Option<Self> {
        let data: Vec<[f32; 3]> = bytes
            .as_chunks::<12>()
            .0
            .iter()
            .map(|chunk| {
                let f = |at: usize| {
                    f32::from_le_bytes([chunk[at], chunk[at + 1], chunk[at + 2], chunk[at + 3]])
                };
                [f(0), f(4), f(8)]
            })
            .collect();
        (size >= 2 && data.len() == size * size * size).then_some(Self { size, data })
    }
}

/// Reads a `.cube` LUT (Adobe and Resolve's text format): a 3D table, or
/// a 1D one turned into a 3D table.
///
/// # Errors
/// What is wrong with the file.
pub fn parse_cube(text: &str) -> Result<Table, String> {
    let mut size3 = 0_usize;
    let mut size1 = 0_usize;
    let mut rows: Vec<[f32; 3]> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        let first = words.next().unwrap_or_default();
        let numbers = |words: std::str::SplitWhitespace<'_>| -> Vec<f32> {
            words.filter_map(|word| word.parse().ok()).collect()
        };
        match first {
            // Inputs are taken as 0 to 1 (the domain lines, which almost
            // every LUT leaves at that, are read past).
            "TITLE" | "DOMAIN_MIN" | "DOMAIN_MAX" | "LUT_3D_INPUT_RANGE" | "LUT_1D_INPUT_RANGE" => {
            }
            "LUT_3D_SIZE" => size3 = numbers(words).first().map_or(0, |n| *n as usize),
            "LUT_1D_SIZE" => size1 = numbers(words).first().map_or(0, |n| *n as usize),
            _ => {
                let values: Vec<f32> = line
                    .split_whitespace()
                    .filter_map(|word| word.parse().ok())
                    .collect();
                if values.len() == 3 {
                    rows.push([values[0], values[1], values[2]]);
                } else if first.chars().next().is_some_and(char::is_alphabetic) {
                    // A keyword this reader does not know: skipped.
                } else {
                    return Err(tr!("lut-bad-line", line = line.to_owned()));
                }
            }
        }
    }
    if size3 >= 2 {
        if rows.len() != size3 * size3 * size3 {
            return Err(tr!(
                "lut-wrong-count",
                want = (size3 * size3 * size3) as i64,
                got = rows.len() as i64
            ));
        }
        if size3 > 129 {
            return Err(tr!("lut-too-large"));
        }
        return Ok(Table {
            size: size3,
            data: rows,
        });
    }
    if size1 >= 2 {
        if rows.len() != size1 {
            return Err(tr!(
                "lut-wrong-count",
                want = size1 as i64,
                got = rows.len() as i64
            ));
        }
        let curve = |channel: usize, x: f32| {
            let position = x.clamp(0.0, 1.0) * (size1 - 1) as f32;
            let i = (position.floor() as usize).min(size1 - 2);
            let t = position - i as f32;
            rows[i][channel] * (1.0 - t) + rows[i + 1][channel] * t
        };
        let mut table = Table::identity(33);
        for rgb in &mut table.data {
            *rgb = [curve(0, rgb[0]), curve(1, rgb[1]), curve(2, rgb[2])];
        }
        return Ok(table);
    }
    Err(tr!("lut-no-size"))
}

/// A LUT entity named `name` holding `table`.
#[must_use]
pub fn lut_entity(name: &str, table: &Table) -> Entity {
    Entity::empty(name, LUT_CLASS)
        .with_property(LUT_SIZE, table.size as i64)
        .with_property(LUT_DATA, Value::Bytes(table.to_bytes()))
}

/// The table of LUT entity `id`.
#[must_use]
pub fn lut_of(arena: &Arena, id: EntityId) -> Option<Table> {
    let entity = arena.get_by_id(id).ok()?;
    let size = entity.property(LUT_SIZE).and_then(Value::as_i64)? as usize;
    match entity.property(LUT_DATA) {
        Some(Value::Bytes(bytes)) => Table::from_bytes(size, bytes),
        _ => None,
    }
}

fn luma(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

/// Hue (0 to 1), saturation and lightness of a color (HSL).
#[must_use]
pub fn hsl([r, g, b]: [f32; 3]) -> [f32; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let delta = max - min;
    if delta <= 1e-6 {
        return [0.0, 0.0, l];
    }
    let s = delta / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6);
    let h = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    } / 6.0;
    [h, s.clamp(0.0, 1.0), l]
}

/// The color of a hue (0 to 1), saturation and lightness.
#[must_use]
pub fn from_hsl([h, s, l]: [f32; 3]) -> [f32; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h6 = h.rem_euclid(1.0) * 6.0;
    let x = c * (1.0 - (h6 % 2.0 - 1.0).abs());
    let (r, g, b) = match h6 as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r + m, g + m, b + m]
}

/// A hue curve's value at hue `h` (wrapping round), 0.5 where it has no
/// points.
#[must_use]
pub fn hue_curve(points: &[[f32; 2]], h: f32) -> f32 {
    if points.is_empty() {
        return 0.5;
    }
    let mut sorted: Vec<[f32; 2]> = points.to_vec();
    sorted.sort_by(|a, b| a[0].total_cmp(&b[0]));
    // The points a turn before and after too, so any hue has a point on
    // either side.
    let around: Vec<[f32; 2]> = [-1.0_f32, 0.0, 1.0]
        .iter()
        .flat_map(|shift| sorted.iter().map(move |p| [p[0] + shift, p[1]]))
        .collect();
    let h = h.rem_euclid(1.0);
    let after = around
        .partition_point(|p| p[0] <= h)
        .clamp(1, around.len() - 1);
    let (a, b) = (around[after - 1], around[after]);
    let span = b[0] - a[0];
    if span <= 1e-6 {
        return b[1];
    }
    a[1] + (b[1] - a[1]) * ((h - a[0]) / span).clamp(0.0, 1.0)
}

/// How much of a color `qualifier` keys, 0 to 1.
#[must_use]
pub fn key(qualifier: &Qualifier, rgb: [f32; 3]) -> f32 {
    let [h, s, l] = hsl(rgb);
    let soft = qualifier.softness.max(1e-4);
    let range = |value: f32, [from, to]: [f32; 2]| {
        let rise = ((value - from + soft) / soft).clamp(0.0, 1.0);
        let fall = ((to + soft - value) / soft).clamp(0.0, 1.0);
        rise.min(fall)
    };
    let hue = if qualifier.hue_width >= 1.0 {
        1.0
    } else {
        let d = (h - qualifier.hue).rem_euclid(1.0);
        let d = d.min(1.0 - d);
        let half = qualifier.hue_width / 2.0;
        ((half + soft - d) / soft).clamp(0.0, 1.0)
    };
    let keyed = hue * range(s, qualifier.saturation) * range(l, qualifier.luminance);
    if qualifier.invert { 1.0 - keyed } else { keyed }
}

/// What one corrector does to a color (0 to 1), its qualifier and LUT
/// included. `luts` gives the tables of LUT entities.
#[must_use]
pub fn correct(
    corrector: &Corrector,
    rgb: [f32; 3],
    luts: &dyn Fn(EntityId) -> Option<Table>,
) -> [f32; 3] {
    if !corrector.enabled {
        return rgb;
    }
    let c = corrector;
    let mut x = rgb;
    for channel in 0..3 {
        let wheel = |w: [f32; 4]| w[channel] + w[3];
        let mut v = x[channel] + wheel(c.offset);
        let lift = wheel(c.lift);
        v += lift * (1.0 - v);
        v *= 1.0 + wheel(c.gain);
        let exponent = 2.0_f32.powf(-wheel(c.gamma) * 1.5);
        v = v.max(0.0).powf(exponent);
        // Contrast about the pivot.
        v = (v - c.pivot) * c.contrast + c.pivot;
        x[channel] = v;
    }
    // Temperature and tint.
    let warm = c.temperature / 100.0 * 0.25;
    let tint = c.tint / 100.0 * 0.25;
    x[0] *= 1.0 + warm;
    x[2] *= 1.0 - warm;
    x[1] *= 1.0 - tint;
    // Saturation, keeping luma.
    let y = luma(x);
    for value in &mut x {
        *value = y + (*value - y) * c.saturation;
    }
    // Custom curves.
    let master = sue_model::Response::new(&c.curves[0]);
    for channel in 0..3 {
        let own = sue_model::Response::new(&c.curves[channel + 1]);
        x[channel] = own.eval(master.eval(x[channel].clamp(0.0, 1.0)));
    }
    // Hue curves.
    if !(c.hue_hue.is_empty() && c.hue_sat.is_empty() && c.hue_lum.is_empty()) {
        let [h, s, l] = hsl(x.map(|v| v.clamp(0.0, 1.0)));
        let h2 = h + (hue_curve(&c.hue_hue, h) - 0.5);
        let s2 = (s * hue_curve(&c.hue_sat, h) * 2.0).clamp(0.0, 1.0);
        let l2 = (l + (hue_curve(&c.hue_lum, h) - 0.5) * 0.5).clamp(0.0, 1.0);
        x = from_hsl([h2, s2, l2]);
    }
    if let Some(table) = c.lut.and_then(luts) {
        let looked = table.sample(x);
        for channel in 0..3 {
            x[channel] += (looked[channel] - x[channel]) * c.lut_mix.clamp(0.0, 1.0);
        }
    }
    let x = x.map(|v| v.clamp(0.0, 1.0));
    match &c.qualifier {
        Some(qualifier) => {
            let amount = key(qualifier, rgb);
            std::array::from_fn(|channel| rgb[channel] + (x[channel] - rgb[channel]) * amount)
        }
        None => x,
    }
}

/// A grade: correctors one after another.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Grade {
    /// The correctors.
    pub correctors: Vec<Corrector>,
}

impl Grade {
    /// The exact grade of a color (0 to 1).
    #[must_use]
    pub fn eval(&self, rgb: [f32; 3], luts: &dyn Fn(EntityId) -> Option<Table>) -> [f32; 3] {
        self.correctors
            .iter()
            .fold(rgb, |x, corrector| correct(corrector, x, luts))
    }

    /// `true` when it changes nothing.
    #[must_use]
    pub fn is_neutral(&self) -> bool {
        self.correctors
            .iter()
            .all(|corrector| !corrector.enabled || corrector.is_neutral())
    }

    /// The grade baked into a table of `size` points a side.
    #[must_use]
    pub fn bake(&self, size: usize, luts: &dyn Fn(EntityId) -> Option<Table>) -> Baked {
        let mut table = Table::identity(size);
        for rgb in &mut table.data {
            *rgb = self.eval(*rgb, luts);
        }
        Baked { table }
    }
}

/// A grade baked into a table, ready for pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Baked {
    /// The table.
    pub table: Table,
}

/// The points a side of a baked grade.
pub const BAKE_SIZE: usize = 33;

impl Baked {
    /// Grades straight alpha RGBA pixels in place, on every core.
    pub fn apply(&self, rgba: &mut [u8], width: usize) {
        let rows = rgba.len() / (width * 4).max(1);
        crate::fx::pixels::strips(rgba, width * 4, rows, &|_, part| {
            for pixel in part.as_chunks_mut::<4>().0 {
                let rgb = [pixel[0], pixel[1], pixel[2]].map(|c| f32::from(c) / 255.0);
                let out = self.table.sample(rgb);
                for channel in 0..3 {
                    pixel[channel] = (out[channel] * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: EntityId) -> Option<Table> {
        None
    }

    fn close(a: [f32; 3], b: [f32; 3], tolerance: f32) -> bool {
        a.iter().zip(&b).all(|(x, y)| (x - y).abs() <= tolerance)
    }

    #[test]
    fn a_neutral_corrector_changes_nothing() {
        let corrector = Corrector::default();
        assert!(corrector.is_neutral());
        for rgb in [[0.0, 0.0, 0.0], [0.2, 0.5, 0.9], [1.0, 1.0, 1.0]] {
            assert!(close(correct(&corrector, rgb, &none), rgb, 1e-5));
        }
    }

    #[test]
    fn the_wheels_move_blacks_mids_and_whites() {
        let lift = Corrector {
            lift: [0.0, 0.0, 0.0, 0.2],
            ..Corrector::default()
        };
        assert!(
            correct(&lift, [0.0; 3], &none)[0] > 0.15,
            "lift raises black"
        );
        assert!(
            (correct(&lift, [1.0; 3], &none)[0] - 1.0).abs() < 1e-5,
            "and keeps white"
        );
        let gain = Corrector {
            gain: [0.0, 0.0, 0.0, -0.5],
            ..Corrector::default()
        };
        assert!((correct(&gain, [1.0; 3], &none)[0] - 0.5).abs() < 1e-5);
        assert_eq!(correct(&gain, [0.0; 3], &none)[0], 0.0, "gain keeps black");
        let gamma = Corrector {
            gamma: [0.0, 0.0, 0.0, 0.3],
            ..Corrector::default()
        };
        let mid = correct(&gamma, [0.5; 3], &none)[0];
        assert!(mid > 0.55, "gamma up lifts the midtones: {mid}");
        assert_eq!(correct(&gamma, [1.0; 3], &none)[0], 1.0);
        let warm = Corrector {
            offset: [0.1, 0.0, -0.1, 0.0],
            ..Corrector::default()
        };
        let out = correct(&warm, [0.5; 3], &none);
        assert!(out[0] > 0.55 && out[2] < 0.45 && (out[1] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn contrast_turns_about_the_pivot_and_saturation_keeps_luma() {
        let contrast = Corrector {
            contrast: 1.5,
            pivot: 0.5,
            ..Corrector::default()
        };
        assert!(close(correct(&contrast, [0.5; 3], &none), [0.5; 3], 1e-5));
        assert!(correct(&contrast, [0.7; 3], &none)[0] > 0.75);
        let gray = Corrector {
            saturation: 0.0,
            ..Corrector::default()
        };
        let rgb = [0.8, 0.3, 0.1];
        let out = correct(&gray, rgb, &none);
        assert!((out[0] - out[1]).abs() < 1e-5 && (out[1] - out[2]).abs() < 1e-5);
        assert!((out[0] - luma(rgb)).abs() < 1e-4);
    }

    #[test]
    fn curves_and_hue_curves() {
        let inverted = Corrector {
            curves: [
                vec![[0.0, 1.0], [1.0, 0.0]],
                straight(),
                straight(),
                straight(),
            ],
            ..Corrector::default()
        };
        assert!(correct(&inverted, [0.0; 3], &none)[0] > 0.99);
        // Hue against hue: red pushed a third of the way round, to green.
        let turned = Corrector {
            hue_hue: vec![[0.0, 0.5 + 1.0 / 3.0]],
            ..Corrector::default()
        };
        let out = correct(&turned, [1.0, 0.0, 0.0], &none);
        assert!(out[1] > 0.9 && out[0] < 0.1, "{out:?}");
        // Hue against saturation: zero takes the color out.
        let flat = Corrector {
            hue_sat: vec![[0.0, 0.0]],
            ..Corrector::default()
        };
        let out = correct(&flat, [0.9, 0.2, 0.2], &none);
        assert!((out[0] - out[2]).abs() < 1e-4, "{out:?}");
        assert_eq!(hue_curve(&[], 0.3), 0.5);
        let wrap = [[0.1, 0.2], [0.9, 0.6]];
        let across = hue_curve(&wrap, 0.0);
        assert!((across - 0.4).abs() < 1e-4, "{across}");
    }

    #[test]
    fn a_qualifier_limits_the_correction_to_what_it_keys() {
        let corrector = Corrector {
            saturation: 0.0,
            qualifier: Some(Qualifier {
                hue: 0.0,
                hue_width: 0.1,
                saturation: [0.3, 1.0],
                luminance: [0.1, 0.9],
                softness: 0.02,
                invert: false,
            }),
            ..Corrector::default()
        };
        let red = correct(&corrector, [0.9, 0.1, 0.1], &none);
        assert!(
            (red[0] - red[1]).abs() < 1e-3,
            "red is keyed and grayed: {red:?}"
        );
        let blue = [0.1, 0.1, 0.9];
        assert!(
            close(correct(&corrector, blue, &none), blue, 1e-5),
            "blue untouched"
        );
        let mut inverted = corrector;
        if let Some(q) = &mut inverted.qualifier {
            q.invert = true;
        }
        let blue_out = correct(&inverted, blue, &none);
        assert!((blue_out[0] - blue_out[2]).abs() < 1e-3);
    }

    #[test]
    fn the_baked_table_matches_the_exact_grade() {
        let grade = Grade {
            correctors: vec![
                Corrector {
                    lift: [0.02, 0.0, -0.02, 0.05],
                    gamma: [0.0, 0.05, 0.0, 0.1],
                    gain: [0.0, 0.0, 0.05, -0.1],
                    contrast: 1.2,
                    saturation: 1.3,
                    temperature: 20.0,
                    ..Corrector::default()
                },
                Corrector {
                    curves: [
                        vec![[0.0, 0.05], [0.5, 0.55], [1.0, 0.95]],
                        straight(),
                        straight(),
                        straight(),
                    ],
                    ..Corrector::default()
                },
            ],
        };
        let baked = grade.bake(BAKE_SIZE, &none);
        let mut worst = 0.0_f32;
        for r in 0..12 {
            for g in 0..12 {
                for b in 0..12 {
                    let rgb = [r, g, b].map(|v| v as f32 / 11.0);
                    let exact = grade.eval(rgb, &none);
                    let looked = baked.table.sample(rgb);
                    for c in 0..3 {
                        worst = worst.max((exact[c] - looked[c]).abs());
                    }
                }
            }
        }
        assert!(worst < 0.02, "worst difference {worst}");
        // On pixels.
        let mut pixels = vec![128, 64, 200, 255];
        baked.apply(&mut pixels, 1);
        let exact = grade.eval([128.0, 64.0, 200.0].map(|v| v / 255.0), &none);
        for c in 0..3 {
            assert!((f32::from(pixels[c]) - exact[c] * 255.0).abs() < 4.0);
        }
        assert_eq!(pixels[3], 255);
    }

    #[test]
    fn cube_files_read_in_3d_and_1d_and_apply() {
        // A 2 point 3D table that swaps red and blue.
        let cube = "TITLE \"swap\"\n# a comment\nLUT_3D_SIZE 2\n\
            0 0 0\n0 0 1\n0 1 0\n0 1 1\n1 0 0\n1 0 1\n1 1 0\n1 1 1\n";
        let table = parse_cube(cube).unwrap();
        assert_eq!(table.size, 2);
        let out = table.sample([0.8, 0.3, 0.1]);
        assert!(close(out, [0.1, 0.3, 0.8], 1e-5), "{out:?}");
        let one = "LUT_1D_SIZE 2\n1 1 1\n0 0 0\n";
        let inverted = parse_cube(one).unwrap();
        assert!(close(
            inverted.sample([0.25, 0.5, 1.0]),
            [0.75, 0.5, 0.0],
            0.02
        ));
        assert!(parse_cube("LUT_3D_SIZE 3\n0 0 0\n").is_err());
        assert!(parse_cube("nothing here").is_err());
        // Through an entity, as the pool keeps it.
        let entity = lut_entity("Swap", &table);
        let id = entity.id();
        let mut arena = Arena::new();
        arena.insert_keep_id(entity).unwrap();
        assert_eq!(lut_of(&arena, id), Some(table.clone()));
        let corrector = Corrector {
            lut: Some(id),
            ..Corrector::default()
        };
        let luts = |lut: EntityId| lut_of(&arena, lut);
        let out = correct(&corrector, [0.8, 0.3, 0.1], &luts);
        assert!(close(out, [0.1, 0.3, 0.8], 1e-4));
    }

    #[test]
    fn grades_round_trip_on_a_clip() {
        let corrector = Corrector {
            lift: [0.1, 0.0, 0.0, 0.0],
            hue_sat: vec![[0.3, 0.7]],
            qualifier: Some(Qualifier::default()),
            lut: Some(EntityId::new()),
            ..Corrector::default()
        };
        assert_eq!(
            Corrector::from_value(&corrector.to_value()),
            Some(corrector.clone())
        );
        let mut clip = Clip::new(
            EntityId::new(),
            sue_core::Time::ZERO,
            sue_core::Time::SECOND,
        );
        set_grade(&mut clip, &[corrector.clone(), Corrector::default()]);
        assert_eq!(grade_of(&clip), [corrector, Corrector::default()]);
        set_grade(&mut clip, &[]);
        assert!(grade_of(&clip).is_empty());
    }

    #[test]
    fn hsl_round_trips() {
        for rgb in [
            [0.9, 0.2, 0.1],
            [0.1, 0.8, 0.3],
            [0.2, 0.3, 0.95],
            [0.5, 0.5, 0.5],
        ] {
            assert!(close(from_hsl(hsl(rgb)), rgb, 1e-4), "{rgb:?}");
        }
    }
}
