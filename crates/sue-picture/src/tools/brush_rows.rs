//! The brush settings of a sub tool that its tip's text has no word for
//! (the full guide's FG-4 rows): the brush's own blending mode, how a
//! mixing brush carries its paint, the tip's thickness and density, the
//! gap presets, a tip of several pictures, snapping, anti-overflow and
//! how post correction works.
//!
//! [`BrushRows::preset`] is laid over the sub tool's brush preset as the
//! engine paints it ([`PresetRows::apply`]) and read back from a preset
//! ([`PresetRows::of_preset`]), so a brush file keeps every one of them.
//! The rest of [`BrushRows`] is how the canvas treats the pen's path; it
//! is kept in a preset as tagged fields ([`BrushRows::write_to`],
//! [`BrushRows::read_from`]) that a build which does not know them keeps
//! untouched.

use sue_model::brush::{Mixing, Spacing, param};
use sue_model::{BlendMode, BrushPreset};

/// The tagged fields [`BrushRows`] keeps in a brush preset.
pub mod field {
    /// The gap preset: [`super::Gap::name`].
    pub const GAP: &str = "sue_paint.gap";
    /// The tip's thickness runs up and down instead of across: a flag.
    pub const VERTICAL: &str = "sue_paint.thickness_vertical";
    /// The stroke snaps to rulers and the grid: a flag.
    pub const SNAPPING: &str = "sue_paint.snapping";
    /// Post correction grows with the pen's speed: a flag.
    pub const BY_SPEED: &str = "sue_paint.correction_by_speed";
    /// Post correction is measured on the screen: a flag.
    pub const BY_SCALE: &str = "sue_paint.correction_by_scale";
    /// Post correction lays Bezier curves through the line: a flag.
    pub const BEZIER: &str = "sue_paint.correction_bezier";
    /// Anti-overflow, as text: `tolerance scaling`, or empty for off.
    pub const OVERFLOW: &str = "sue_paint.anti_overflow";
}

/// Clip Studio Paint's gap presets: how far apart the dabs fall.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Gap {
    /// The same number of pixels whatever the brush's size.
    Fixed,
    /// Half the normal gap.
    Narrow,
    /// The brush's own gap.
    #[default]
    Normal,
    /// Twice the normal gap.
    Wide,
}

impl Gap {
    /// Every preset, in the order the row shows them.
    pub const ALL: [Self; 4] = [Self::Fixed, Self::Narrow, Self::Normal, Self::Wide];

    /// The stored name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Narrow => "narrow",
            Self::Normal => "normal",
            Self::Wide => "wide",
        }
    }

    /// The preset stored as `name`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|gap| gap.name() == name)
    }

    /// How many times the normal gap this preset lays (1 for fixed).
    #[must_use]
    pub const fn factor(self) -> f32 {
        match self {
            Self::Narrow => 0.5,
            Self::Wide => 2.0,
            Self::Fixed | Self::Normal => 1.0,
        }
    }
}

/// The settings of a brush preset that a sub tool's tip has no word for.
#[derive(Clone, Debug, PartialEq)]
pub struct PresetRows {
    /// How the brush's paint meets the layer's (the brush's own blending
    /// mode, full guide 00:28:16).
    pub blend: BlendMode,
    /// A mixing brush's amount of paint: how much of its own it carries,
    /// 0 to 1 (`Mixing::load`).
    pub amount: f32,
    /// How far it drags the color it picks up, 0 to 1 (color stretch,
    /// `Mixing::pull`).
    pub stretch: f32,
    /// How much it softens what is under it, 0 to 1 (blur intensity,
    /// `Mixing::blur`).
    pub blur: f32,
    /// The tip's thickness: its short side over its long one, 0.01 to 1.
    pub thickness: f32,
    /// The thickness runs up and down (the tip is narrow across) instead
    /// of across (the tip is low).
    pub vertical: bool,
    /// How dense each dab of the tip is, apart from the opacity, 0 to 1
    /// (brush density, 00:30:34).
    pub density: f32,
    /// The gap preset.
    pub gap: Gap,
    /// The gap in pixels while the preset is [`Gap::Fixed`].
    pub fixed_gap: f32,
    /// The tip's pictures, by their shape numbers
    /// (`crate::brush::legacy::shape`), when a person chose them; empty
    /// for the pictures the tip itself names.
    pub tips: Vec<u32>,
}

impl Default for PresetRows {
    fn default() -> Self {
        Self {
            blend: BlendMode::Normal,
            amount: 1.0,
            stretch: 0.0,
            blur: 0.0,
            thickness: 1.0,
            vertical: false,
            density: 1.0,
            gap: Gap::Normal,
            fixed_gap: 4.0,
            tips: Vec::new(),
        }
    }
}

/// A flag field's bytes.
fn flag(on: bool) -> Vec<u8> {
    vec![u8::from(on)]
}

/// Sets tagged field `key` of `preset` to `bytes`.
fn put(preset: &mut BrushPreset, key: &str, bytes: Vec<u8>) {
    if let Some(slot) = preset.extra.iter_mut().find(|(known, _)| known == key) {
        slot.1 = bytes;
    } else {
        preset.extra.push((key.to_owned(), bytes));
    }
}

/// The flag `key` of `preset`, `or` when it has none.
fn flag_of(preset: &BrushPreset, key: &str, or: bool) -> bool {
    preset
        .extra(key)
        .and_then(|bytes| bytes.first())
        .map_or(or, |byte| *byte != 0)
}

impl PresetRows {
    /// What `preset` says of these settings: a brush as it is, read so
    /// that [`Self::apply`] gives it back.
    #[must_use]
    pub fn of_preset(preset: &BrushPreset) -> Self {
        let mix = preset.mix.unwrap_or(Mixing::new(0.0));
        let gap = preset
            .extra(field::GAP)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .and_then(Gap::from_name)
            .unwrap_or(if preset.spacing == Spacing::Fixed {
                Gap::Fixed
            } else {
                Gap::Normal
            });
        Self {
            blend: preset.blend,
            amount: mix.load,
            stretch: mix.pull,
            blur: mix.blur,
            thickness: preset.value(param::ASPECT).unwrap_or(1.0).clamp(0.01, 1.0),
            vertical: flag_of(preset, field::VERTICAL, false),
            density: preset.value(param::DENSITY).unwrap_or(1.0).clamp(0.0, 1.0),
            gap,
            fixed_gap: if preset.spacing == Spacing::Fixed {
                preset.value(param::SPACING).unwrap_or(4.0).max(1.0)
            } else {
                4.0
            },
            tips: Vec::new(),
        }
    }

    /// Lays these settings over `preset`, the brush a tip of spacing
    /// `spacing` (a share of its size) paints with. The tip's pictures
    /// are the caller's: they need the shapes' pictures.
    pub fn apply(&self, preset: &mut BrushPreset, spacing: f32) {
        preset.blend = self.blend;
        if let Some(mix) = &mut preset.mix {
            mix.load = self.amount.clamp(0.0, 1.0);
            mix.pull = self.stretch.clamp(0.0, 1.0);
            mix.blur = self.blur.clamp(0.0, 1.0);
        }
        let thickness = self.thickness.clamp(0.01, 1.0);
        preset.set(param::ASPECT, thickness);
        put(preset, field::VERTICAL, flag(self.vertical));
        if self.vertical && thickness < 1.0 {
            // Narrow across: the same low tip turned a quarter.
            let angle = preset.value(param::ANGLE).unwrap_or(0.0);
            preset.set(param::ANGLE, angle + 90.0);
        }
        preset.set(param::DENSITY, self.density.clamp(0.0, 1.0));
        put(preset, field::GAP, self.gap.name().as_bytes().to_vec());
        match self.gap {
            Gap::Fixed => {
                preset.spacing = Spacing::Fixed;
                preset.set(param::SPACING, self.fixed_gap.max(1.0));
            }
            Gap::Normal => {}
            Gap::Narrow | Gap::Wide => {
                // The normal gap: the engine's own where it picks one.
                let normal = if preset.spacing == Spacing::Auto {
                    sue_brush::paint::plan::AUTO_GAP
                } else {
                    preset.value(param::SPACING).unwrap_or(spacing)
                };
                preset.spacing = Spacing::Ratio;
                preset.set(param::SPACING, normal * self.gap.factor());
            }
        }
    }

    /// The angle a tip read back from a preset these settings were laid
    /// on ([`Self::apply`]) had before: the quarter turn of a vertical
    /// thickness taken off `angle`.
    #[must_use]
    pub fn unturned(&self, angle: f32) -> f32 {
        if self.vertical && self.thickness < 1.0 {
            angle - 90.0
        } else {
            angle
        }
    }
}

/// Anti-overflow: the stroke stays inside the area the reference layer's
/// lines close around where it starts (FGV-31).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AntiOverflow {
    /// How opaque a line must be to stop the paint, 0 to 1.
    pub tolerance: f32,
    /// How many pixels the area grows past the lines (or, below 0,
    /// stays back from them).
    pub scaling: i32,
}

impl Default for AntiOverflow {
    fn default() -> Self {
        Self {
            tolerance: 0.5,
            scaling: 0,
        }
    }
}

/// The brush settings of a sub tool beyond its tip (see the module).
#[derive(Clone, Debug, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "switches of Tool Property, one each"
)]
pub struct BrushRows {
    /// The preset's settings a person changed; `None` while they are as
    /// the sub tool's brush has them.
    pub preset: Option<PresetRows>,
    /// The stroke snaps to rulers and the grid when they are on (Enable
    /// snapping, 00:33:16); off, this sub tool draws freely.
    pub snapping: bool,
    /// Post correction grows with the pen's speed (by speed).
    pub correction_by_speed: bool,
    /// Post correction is the same on the screen at any zoom (by scale).
    pub correction_by_scale: bool,
    /// Post correction lays Bezier curves through the line, with fewer
    /// points (Bezier curve).
    pub bezier: bool,
    /// Anti-overflow, when it is on.
    pub overflow: Option<AntiOverflow>,
}

impl Default for BrushRows {
    fn default() -> Self {
        Self {
            preset: None,
            snapping: true,
            correction_by_speed: false,
            correction_by_scale: false,
            bezier: false,
            overflow: None,
        }
    }
}

impl BrushRows {
    /// Writes the settings that are not the preset's own into `preset`
    /// as tagged fields.
    pub fn write_to(&self, preset: &mut BrushPreset) {
        put(preset, field::SNAPPING, flag(self.snapping));
        put(preset, field::BY_SPEED, flag(self.correction_by_speed));
        put(preset, field::BY_SCALE, flag(self.correction_by_scale));
        put(preset, field::BEZIER, flag(self.bezier));
        let overflow = self
            .overflow
            .map(|o| format!("{} {}", o.tolerance, o.scaling))
            .unwrap_or_default();
        put(preset, field::OVERFLOW, overflow.into_bytes());
    }

    /// The settings `preset` keeps ([`Self::write_to`]); what it does not
    /// say is at its default. The preset's own rows are read as they are
    /// ([`PresetRows::of_preset`]).
    #[must_use]
    pub fn read_from(preset: &BrushPreset) -> Self {
        let overflow = preset
            .extra(field::OVERFLOW)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .and_then(|text| {
                let (tolerance, scaling) = text.split_once(' ')?;
                Some(AntiOverflow {
                    tolerance: tolerance.parse::<f32>().ok()?.clamp(0.0, 1.0),
                    scaling: scaling.parse().ok()?,
                })
            });
        Self {
            preset: Some(PresetRows::of_preset(preset)),
            snapping: flag_of(preset, field::SNAPPING, true),
            correction_by_speed: flag_of(preset, field::BY_SPEED, false),
            correction_by_scale: flag_of(preset, field::BY_SCALE, false),
            bezier: flag_of(preset, field::BEZIER, false),
            overflow,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mixing() -> BrushPreset {
        let mut preset = BrushPreset::from_legacy_text("size=20;mix=0.6;spacing=0.1");
        preset.set(param::ANGLE, 10.0);
        preset
    }

    #[test]
    fn every_row_reads_back_from_the_preset_it_was_laid_on() {
        let rows = PresetRows {
            blend: BlendMode::Multiply,
            amount: 0.4,
            stretch: 0.7,
            blur: 0.25,
            thickness: 0.3,
            vertical: true,
            density: 0.55,
            gap: Gap::Fixed,
            fixed_gap: 12.0,
            tips: Vec::new(),
        };
        let mut preset = mixing();
        rows.apply(&mut preset, 0.1);
        assert_eq!(preset.blend, BlendMode::Multiply);
        let mix = preset.mix.expect("still mixing");
        assert_eq!((mix.load, mix.pull, mix.blur), (0.4, 0.7, 0.25));
        assert_eq!(preset.value(param::ANGLE), Some(100.0), "turned a quarter");
        assert_eq!(preset.spacing, Spacing::Fixed);
        // Through the file's bytes, as a brush file keeps it.
        let back = BrushPreset::from_bytes(&preset.to_bytes()).expect("reads");
        let read = PresetRows::of_preset(&back);
        assert_eq!(read, rows);
        assert_eq!(read.unturned(100.0), 10.0);
    }

    #[test]
    fn narrow_and_wide_halve_and_double_the_normal_gap() {
        let gap = |gap: Gap| {
            let mut preset = mixing();
            PresetRows {
                gap,
                ..PresetRows::of_preset(&preset)
            }
            .apply(&mut preset, 0.1);
            preset.value(param::SPACING).unwrap()
        };
        assert!((gap(Gap::Narrow) - 0.05).abs() < 1e-6);
        assert!((gap(Gap::Normal) - 0.1).abs() < 1e-6);
        assert!((gap(Gap::Wide) - 0.2).abs() < 1e-6);
        // A brush whose engine picks the gap: narrow is half of that.
        let mut auto = mixing();
        auto.spacing = Spacing::Auto;
        PresetRows {
            gap: Gap::Narrow,
            ..PresetRows::of_preset(&auto)
        }
        .apply(&mut auto, 0.1);
        assert_eq!(auto.spacing, Spacing::Ratio);
        assert!(auto.value(param::SPACING).unwrap() < sue_brush::paint::plan::AUTO_GAP);
    }

    #[test]
    fn the_canvas_switches_round_trip_and_default_when_unsaid() {
        let rows = BrushRows {
            preset: None,
            snapping: false,
            correction_by_speed: true,
            correction_by_scale: true,
            bezier: true,
            overflow: Some(AntiOverflow {
                tolerance: 0.25,
                scaling: -2,
            }),
        };
        let mut preset = mixing();
        rows.write_to(&mut preset);
        let back = BrushPreset::from_bytes(&preset.to_bytes()).expect("reads");
        let read = BrushRows::read_from(&back);
        assert_eq!(
            BrushRows {
                preset: None,
                ..read
            },
            rows
        );
        let plain = BrushRows::read_from(&mixing());
        assert!(plain.snapping && plain.overflow.is_none() && !plain.bezier);
    }
}
