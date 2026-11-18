//! Text and shape layers, as After Effects makes them.
//!
//! A shape is a generated source of class [`SHAPE`]: a rectangle (with
//! rounded corners), an ellipse, a star or a regular polygon, filled and
//! outlined, drawn at the sequence's frame size; the clip's motion keys
//! animate it whole.
//!
//! A title clip may carry a text animator ([`ANIMATOR`] in the clip's
//! properties): a property (position, scale, rotation, opacity) offset by
//! an amount on the letters a range selector has not reached yet, the
//! selector's progress keyframed on the clip ([`RANGE`], 0 to 100), per
//! letter or for the whole title. Typing on, letters dropping in, spinning
//! or growing into place are all one animator. Each letter is set where
//! `sue-type` lays it out, then placed by the compositor's own drawing.

use std::collections::BTreeMap;

use sue_core::{Arena, Entity, EntityId, Time, Value};
use sue_model::Clip;

use crate::media::frames::VideoFrame;
use crate::model::{self, key};
use crate::render::{self, Draw, Layer};

/// The class of a shape.
pub const SHAPE: &str = "ve.shape";
/// A shape's kind: `rect`, `ellipse`, `star`, `polygon`.
pub const SHAPE_KIND: &str = "shape.kind";
/// Its width and height in sequence pixels.
pub const SHAPE_SIZE: &str = "shape.size";
/// A rectangle's corner radius, sequence pixels.
pub const SHAPE_ROUND: &str = "shape.round";
/// A star's or polygon's points.
pub const SHAPE_POINTS: &str = "shape.points";
/// Its outline's color (its fill is [`key::COLOR`]).
pub const SHAPE_STROKE: &str = "shape.stroke";
/// Its outline's width, sequence pixels (0: none).
pub const SHAPE_STROKE_WIDTH: &str = "shape.stroke_width";

/// The shape kinds.
pub const SHAPES: [&str; 4] = ["rect", "ellipse", "star", "polygon"];

/// A new shape named `name` of `kind`.
#[must_use]
pub fn shape(name: &str, kind: &str) -> Entity {
    Entity::empty(name, SHAPE)
        .with_property(SHAPE_KIND, kind)
        .with_property(SHAPE_SIZE, Value::Vec2([480.0, 480.0]))
        .with_property(SHAPE_ROUND, 0.0)
        .with_property(SHAPE_POINTS, 5_i64)
        .with_property(key::COLOR, sue_core::Color::srgb8(255, 255, 255))
        .with_property(SHAPE_STROKE, sue_core::Color::srgb8(0, 0, 0))
        .with_property(SHAPE_STROKE_WIDTH, 0.0)
}

/// A color property as RGBA, or `default`.
#[must_use]
pub fn rgba_of(value: Option<&Value>, default: [u8; 4]) -> [u8; 4] {
    match value {
        Some(Value::Color(color)) => {
            let srgb = color.to_space(sue_core::ColorSpace::Srgb);
            let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            [
                byte(srgb.rgb[0]),
                byte(srgb.rgb[1]),
                byte(srgb.rgb[2]),
                byte(srgb.alpha),
            ]
        }
        _ => default,
    }
}

/// The outline of a shape of `kind`, `size` large, centred at `center`.
#[must_use]
pub fn outline(
    kind: &str,
    size: [f64; 2],
    round: f64,
    points: usize,
    center: [f64; 2],
) -> Vec<[f64; 2]> {
    let (hw, hh) = (size[0] / 2.0, size[1] / 2.0);
    let at = |x: f64, y: f64| [center[0] + x, center[1] + y];
    match kind {
        "ellipse" => (0..128)
            .map(|step| {
                let angle = std::f64::consts::TAU * f64::from(step) / 128.0;
                at(hw * angle.cos(), hh * angle.sin())
            })
            .collect(),
        "star" | "polygon" => {
            let count = points.clamp(3, 64);
            let corners = if kind == "star" { count * 2 } else { count };
            (0..corners)
                .map(|step| {
                    let angle = std::f64::consts::TAU * step as f64 / corners as f64
                        - std::f64::consts::FRAC_PI_2;
                    let reach = if kind == "star" && step % 2 == 1 {
                        0.45
                    } else {
                        1.0
                    };
                    at(hw * reach * angle.cos(), hh * reach * angle.sin())
                })
                .collect()
        }
        _ => {
            let r = round.clamp(0.0, hw.min(hh));
            if r <= 0.5 {
                return vec![at(-hw, -hh), at(hw, -hh), at(hw, hh), at(-hw, hh)];
            }
            let mut out = Vec::new();
            for (cx, cy, start) in [
                (hw - r, -hh + r, -90.0_f64),
                (hw - r, hh - r, 0.0),
                (-hw + r, hh - r, 90.0),
                (-hw + r, -hh + r, 180.0),
            ] {
                for step in 0..=8 {
                    let angle = (start + 90.0 * f64::from(step) / 8.0).to_radians();
                    out.push(at(cx + r * angle.cos(), cy + r * angle.sin()));
                }
            }
            out
        }
    }
}

/// Draws shape `id` at the frame size `frame`, `unit` frame pixels to the
/// sequence pixel.
#[must_use]
pub fn draw_shape(arena: &Arena, id: EntityId, frame: (u32, u32), unit: f32) -> Option<VideoFrame> {
    let entity = arena.get_by_id(id).ok()?;
    let kind = model::text(entity, SHAPE_KIND).unwrap_or("rect");
    let unit = f64::from(unit);
    let size = match entity.property(SHAPE_SIZE) {
        Some(Value::Vec2(size)) => [size[0] * unit, size[1] * unit],
        _ => [480.0 * unit, 480.0 * unit],
    };
    let round = model::number(entity, SHAPE_ROUND).unwrap_or(0.0) * unit;
    let points = model::number(entity, SHAPE_POINTS).unwrap_or(5.0) as usize;
    let polygon = outline(
        kind,
        size,
        round,
        points,
        [f64::from(frame.0) / 2.0, f64::from(frame.1) / 2.0],
    );
    let (w, h) = (frame.0 as usize, frame.1 as usize);
    let cover = crate::fx::mask::fill(&polygon, w, h);
    let fill = rgba_of(entity.property(key::COLOR), [255, 255, 255, 255]);
    let mut image = crate::fx::pixels::Image::clear(frame.0, frame.1);
    for (pixel, c) in image.rgba.as_chunks_mut::<4>().0.iter_mut().zip(&cover) {
        let alpha = (c * f32::from(fill[3])).round() as u8;
        *pixel = [fill[0], fill[1], fill[2], alpha];
    }
    let width = model::number(entity, SHAPE_STROKE_WIDTH).unwrap_or(0.0) * unit;
    if width > 0.0 {
        let stroke = rgba_of(entity.property(SHAPE_STROKE), [0, 0, 0, 255]);
        let mut params = crate::fx::Params::default();
        params.values.set("width", width);
        params.values.set("opacity", f64::from(stroke[3]) / 2.55);
        params.values.set(
            "color",
            crate::fx::color_value([stroke[0], stroke[1], stroke[2]]),
        );
        crate::fx::pixels::apply(
            "stylize.stroke",
            &params,
            &mut image,
            &crate::fx::pixels::Ctx::default(),
        );
    }
    Some(VideoFrame::new(frame.0, frame.1, image.rgba, 0))
}

/// The clip property a text animator is kept in.
pub const ANIMATOR: &str = "ve.text_animator";
/// The animator's range selector, 0 to 100 (a clip channel).
pub const RANGE: &str = "text.range";

/// What a text animator moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Animates {
    /// Moved up (or down) by the amount, in sequence pixels.
    Position,
    /// Scaled to the amount, in percent.
    Scale,
    /// Turned by the amount, in degrees.
    Rotation,
    /// Faded to the amount, in percent.
    Opacity,
}

impl Animates {
    /// Every property.
    pub const ALL: [Self; 4] = [Self::Position, Self::Scale, Self::Rotation, Self::Opacity];

    /// Its stable name.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Position => "position",
            Self::Scale => "scale",
            Self::Rotation => "rotation",
            Self::Opacity => "opacity",
        }
    }
}

/// A text animator.
#[derive(Clone, Debug, PartialEq)]
pub struct Animator {
    /// What it moves.
    pub animates: Animates,
    /// How far, on the letters the range has not reached.
    pub amount: f64,
    /// Letter by letter (else the whole title at once).
    pub per_letter: bool,
    /// How many letters the change fades over (0: a hard step).
    pub softness: f64,
}

impl Default for Animator {
    fn default() -> Self {
        Self {
            animates: Animates::Opacity,
            amount: 0.0,
            per_letter: true,
            softness: 2.0,
        }
    }
}

impl Animator {
    /// As it is stored.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut map = BTreeMap::new();
        map.insert(
            "animates".to_owned(),
            Value::Text(self.animates.id().to_owned()),
        );
        map.insert("amount".to_owned(), Value::Float(self.amount));
        map.insert("per_letter".to_owned(), Value::Bool(self.per_letter));
        map.insert("softness".to_owned(), Value::Float(self.softness));
        Value::Map(map)
    }

    /// Read back.
    #[must_use]
    pub fn from_value(value: &Value) -> Option<Self> {
        let Value::Map(map) = value else {
            return None;
        };
        let name = map.get("animates").and_then(Value::as_str)?;
        Some(Self {
            animates: Animates::ALL.into_iter().find(|a| a.id() == name)?,
            amount: map.get("amount").and_then(Value::as_f64).unwrap_or(0.0),
            per_letter: map
                .get("per_letter")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            softness: map.get("softness").and_then(Value::as_f64).unwrap_or(2.0),
        })
    }

    /// How much of the change letter `index` of `count` still has when
    /// the range stands at `range` (0 to 100): 1 before the range reaches
    /// it, 0 after.
    #[must_use]
    pub fn weight(&self, index: usize, count: usize, range: f64) -> f64 {
        if !self.per_letter {
            return 1.0 - (range / 100.0).clamp(0.0, 1.0);
        }
        let count = count.max(1) as f64;
        let reached = range.clamp(0.0, 100.0) / 100.0 * (count + self.softness);
        let position = index as f64 + self.softness;
        if self.softness <= 0.0 {
            return if (index as f64) < reached.floor() {
                0.0
            } else {
                1.0
            };
        }
        ((position - reached) / self.softness).clamp(0.0, 1.0)
    }
}

/// The animator on `clip`, if it has one.
#[must_use]
pub fn animator(clip: &Clip) -> Option<Animator> {
    clip.properties.get(ANIMATOR).and_then(Animator::from_value)
}

/// Title `id` drawn with `animator` at `range`, at the frame size `frame`
/// (`unit` frame pixels to the sequence pixel): each letter set where the
/// title lays it out, then moved, scaled, turned or faded by its weight.
#[must_use]
pub fn animated_title(
    arena: &Arena,
    id: EntityId,
    frame: (u32, u32),
    unit: f32,
    animator: &Animator,
    range: f64,
) -> Option<VideoFrame> {
    let entity = arena.get_by_id(id).ok()?;
    let text = entity
        .string_buffer()
        .map(|buffer| buffer.as_str().to_owned())
        .unwrap_or_default();
    let color = crate::media::import::color_of(entity, [255, 255, 255, 255]);
    let size = model::number(entity, key::SIZE).unwrap_or(96.0) as f32 * unit;
    let font = model::text(entity, key::FONT).unwrap_or("");
    let spec = sue_type::raster::TextSpec {
        text: &text,
        font,
        size,
        rgb: [color[0], color[1], color[2]],
        at: [frame.0 as f32 / 2.0, frame.1 as f32 / 2.0],
        center: true,
        vertical: false,
        tracking: 0.0,
        leading: 0.0,
        justify: sue_type::raster::Justify::default(),
        path: &[],
        ruby: &[],
        style: sue_type::raster::TextStyle::default(),
    };
    let slots = sue_type::raster::caret_slots(&spec);
    let letters: Vec<char> = text.chars().collect();
    let mut out = vec![0_u8; frame.0 as usize * frame.1 as usize * 4];
    let count = letters.iter().filter(|c| !c.is_whitespace()).count();
    let tile_side = (size * 2.0).ceil().max(4.0) as u32;
    let mut drawn = 0;
    for (index, letter) in letters.iter().enumerate() {
        if letter.is_whitespace() {
            continue;
        }
        let (Some(here), Some(next)) = (slots.get(index), slots.get(index + 1)) else {
            continue;
        };
        // The letter's middle: between its caret and the next one, half
        // way down the caret.
        let middle = [
            (here.top[0] + next.top[0]) / 2.0,
            (here.top[1] + here.bottom[1]) / 2.0,
        ];
        let weight = animator.weight(drawn, count, range) as f32;
        drawn += 1;
        let amount = animator.amount as f32;
        let mut tile = vec![0_u8; (tile_side * tile_side * 4) as usize];
        let one = letter.to_string();
        let letter_spec = sue_type::raster::TextSpec {
            text: &one,
            at: [tile_side as f32 / 2.0, tile_side as f32 / 2.0],
            ..spec
        };
        sue_type::raster::draw_text(&mut tile, tile_side, tile_side, &letter_spec);
        if color[3] < 255 {
            for pixel in tile.as_chunks_mut::<4>().0 {
                pixel[3] = ((u16::from(pixel[3]) * u16::from(color[3])) / 255) as u8;
            }
        }
        let mut layer = Layer::new(id, model::SourceKind::Title, Time::ZERO);
        let fit = (frame.0 as f32 / tile_side as f32).min(frame.1 as f32 / tile_side as f32);
        let mut scale = 1.0;
        layer.offset = [
            middle[0] - frame.0 as f32 / 2.0,
            middle[1] - frame.1 as f32 / 2.0,
        ];
        match animator.animates {
            Animates::Position => layer.offset[1] += amount * unit * weight,
            Animates::Scale => scale = 1.0 + (amount / 100.0 - 1.0) * weight,
            Animates::Rotation => layer.rotation = amount * weight,
            Animates::Opacity => layer.opacity = 1.0 + (amount / 100.0 - 1.0) * weight,
        }
        layer.scale = scale.max(0.0) / fit;
        layer.opacity = layer.opacity.clamp(0.0, 1.0);
        if layer.opacity <= 0.0 || layer.scale <= 0.0 {
            continue;
        }
        render::draw(
            &mut out,
            frame,
            &Draw {
                rgba: &tile,
                size: (tile_side, tile_side),
                natural: (tile_side, tile_side),
                layer: &layer,
                blend: sue_model::BlendMode::Normal,
                cover: None,
            },
        );
    }
    Some(VideoFrame::new(frame.0, frame.1, out, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_range_reaches_the_letters_in_turn() {
        let typing = Animator {
            animates: Animates::Opacity,
            amount: 0.0,
            per_letter: true,
            softness: 0.0,
        };
        assert_eq!(typing.weight(0, 4, 0.0), 1.0, "nothing shown at 0");
        assert_eq!(typing.weight(0, 4, 50.0), 0.0, "half way: the first two");
        assert_eq!(typing.weight(1, 4, 50.0), 0.0);
        assert_eq!(typing.weight(2, 4, 50.0), 1.0);
        assert_eq!(typing.weight(3, 4, 100.0), 0.0);
        let soft = Animator {
            softness: 2.0,
            ..typing.clone()
        };
        let middle = soft.weight(2, 4, 50.0);
        assert!(middle > 0.0 && middle < 1.0, "{middle}");
        let whole = Animator {
            per_letter: false,
            ..typing.clone()
        };
        assert!((whole.weight(3, 4, 25.0) - 0.75).abs() < 1e-9);
        assert_eq!(Animator::from_value(&soft.to_value()), Some(soft));
    }

    #[test]
    fn shapes_have_their_outline_and_draw_filled_and_stroked() {
        let star = outline("star", [100.0, 100.0], 0.0, 5, [50.0, 50.0]);
        assert_eq!(star.len(), 10);
        let rounded = outline("rect", [100.0, 60.0], 10.0, 0, [0.0, 0.0]);
        assert!(rounded.len() > 4);
        let mut arena = Arena::new();
        let made = shape("Square", "rect")
            .with_property(SHAPE_SIZE, Value::Vec2([20.0, 20.0]))
            .with_property(SHAPE_STROKE_WIDTH, 3.0);
        let id = made.id();
        arena.insert_keep_id(made).unwrap();
        let picture = draw_shape(&arena, id, (64, 36), 1.0).unwrap();
        let at = |x: u32, y: u32| {
            let i = ((y * 64 + x) * 4) as usize;
            [
                picture.rgba[i],
                picture.rgba[i + 1],
                picture.rgba[i + 2],
                picture.rgba[i + 3],
            ]
        };
        assert_eq!(at(32, 18), [255, 255, 255, 255], "filled white");
        assert_eq!(at(20, 18)[3], 255, "the outline, outside the square");
        assert_eq!(at(20, 18)[0], 0, "black");
        assert_eq!(at(2, 2)[3], 0);
    }

    #[test]
    fn an_animated_title_hides_the_letters_not_reached() {
        let mut arena = Arena::new();
        let title = crate::media::import::title("T", "AB");
        let id = title.id();
        arena.insert_keep_id(title).unwrap();
        let typing = Animator {
            animates: Animates::Opacity,
            amount: 0.0,
            per_letter: true,
            softness: 0.0,
        };
        let lit = |frame: &VideoFrame| frame.rgba.chunks(4).filter(|p| p[3] > 128).count();
        let none = animated_title(&arena, id, (320, 180), 1.0, &typing, 0.0).unwrap();
        let half = animated_title(&arena, id, (320, 180), 1.0, &typing, 50.0).unwrap();
        let all = animated_title(&arena, id, (320, 180), 1.0, &typing, 100.0).unwrap();
        assert_eq!(lit(&none), 0);
        assert!(
            lit(&half) > 0 && lit(&half) < lit(&all),
            "{} {}",
            lit(&half),
            lit(&all)
        );
        // All shown: as the plain title draws it, near enough (letters
        // set one by one lose a little kerning).
        let plain = render::title(&arena, id, (320, 180), 1.0).unwrap();
        let (a, b) = (lit(&all) as f64, lit(&plain) as f64);
        assert!((a - b).abs() / b < 0.2, "{a} {b}");
    }
}
