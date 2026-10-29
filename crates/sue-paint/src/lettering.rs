//! Text layers (Clip Studio Paint's Text tool): a raster layer of class
//! [`TEXT`] that keeps its words, font, size, ink and placement, and draws
//! them again with `sue_type::raster` after every edit, so text stays sharp
//! and editable. Clicked inside a balloon, the words center in it.

use egui::Pos2;
use sue_core::txn::{ReplaceComponent, SetProperty};
use sue_core::{Color, EntityId, PixelFormat, RasterTile2D, Value};
use sue_wm::AppletContext;

use crate::manga::{self, Shape};
use crate::studio::SuePaint;

/// The class of a text layer.
pub const TEXT: &str = "sue.paint.text";
const CONTENT: &str = "text.content";
const FONT: &str = "text.font";
const SIZE: &str = "text.size";
const INK: &str = "text.color";
const AT_X: &str = "text.x";
const AT_Y: &str = "text.y";
const CENTER: &str = "text.center";
const VERTICAL: &str = "text.vertical";
/// Where the drawn text is (`Text`, `x0,y0,x1,y1`), for clicking on it.
const BOX: &str = "text.box";

type Edit = sue_core::txn::Batch<sue_core::Arena>;

/// What a text layer says and how.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Lettering {
    pub content: String,
    pub font: String,
    pub size: f32,
    pub rgb: [u8; 3],
    pub at: [f32; 2],
    pub center: bool,
    pub vertical: bool,
}

impl Lettering {
    /// New words at `at` in `rgb`.
    pub(crate) fn new(at: [f32; 2], rgb: [u8; 3]) -> Self {
        Self {
            content: String::new(),
            font: String::new(),
            size: 48.0,
            rgb,
            at,
            center: false,
            vertical: false,
        }
    }

    /// A text layer's lettering.
    pub(crate) fn of(entity: &sue_core::Entity) -> Option<Self> {
        if entity.class() != TEXT {
            return None;
        }
        let number = |key, default: f64| {
            entity
                .property(key)
                .and_then(Value::as_f64)
                .unwrap_or(default) as f32
        };
        let flag = |key| {
            entity
                .property(key)
                .and_then(Value::as_bool)
                .unwrap_or(false)
        };
        let text = |key| {
            entity
                .property(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let rgb = match entity.property(INK) {
            Some(Value::Color(color)) => crate::color::bytes(color.rgb),
            _ => [0, 0, 0],
        };
        Some(Self {
            content: text(CONTENT),
            font: text(FONT),
            size: number(SIZE, 48.0),
            rgb,
            at: [number(AT_X, 0.0), number(AT_Y, 0.0)],
            center: flag(CENTER),
            vertical: flag(VERTICAL),
        })
    }

    /// The layer's pixels for a `width` by `height` picture, and where the
    /// text is.
    fn draw(&self, width: u32, height: u32) -> (Vec<u8>, Option<[f32; 4]>) {
        let mut pixels = vec![0; (width * height * 4) as usize];
        let covered = sue_type::raster::draw_text(
            &mut pixels,
            width,
            height,
            &sue_type::raster::TextSpec {
                text: &self.content,
                font: &self.font,
                size: self.size,
                rgb: self.rgb,
                at: self.at,
                center: self.center,
                vertical: self.vertical,
            },
        );
        (pixels, covered)
    }

    fn properties(&self, covered: Option<[f32; 4]>) -> Vec<(&'static str, Value)> {
        let [r, g, b] = self.rgb;
        let area = covered.map_or_else(String::new, |[x0, y0, x1, y1]| {
            format!("{x0},{y0},{x1},{y1}")
        });
        vec![
            (CONTENT, Value::from(self.content.clone())),
            (FONT, Value::from(self.font.clone())),
            (SIZE, Value::from(f64::from(self.size))),
            (INK, Value::Color(Color::srgb8(r, g, b))),
            (AT_X, Value::from(f64::from(self.at[0]))),
            (AT_Y, Value::from(f64::from(self.at[1]))),
            (CENTER, Value::from(self.center)),
            (VERTICAL, Value::from(self.vertical)),
            (BOX, Value::from(area)),
        ]
    }
}

/// A new text layer entity holding `lettering` drawn on a `width` by
/// `height` page (Batch Import Text).
#[must_use]
pub(crate) fn text_layer(
    lettering: &Lettering,
    width: u32,
    height: u32,
) -> Option<sue_core::Entity> {
    let (pixels, covered) = lettering.draw(width, height);
    let raster = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels).ok()?;
    let name: String = lettering
        .content
        .lines()
        .next()
        .unwrap_or("Text")
        .chars()
        .take(24)
        .collect();
    let mut entity = sue_core::Entity::new(name, raster).with_class(TEXT);
    for (key, value) in lettering.properties(covered) {
        entity.properties_mut().set(key, value);
    }
    Some(entity)
}

/// The edit that changes text layer `layer` (on a `width` by `height`
/// page) to say `content` (the Story Editor).
#[must_use]
pub(crate) fn retext(
    arena: &sue_core::Arena,
    layer: EntityId,
    content: &str,
    width: u32,
    height: u32,
) -> Option<Edit> {
    let mut lettering = Lettering::of(arena.get_by_id(layer).ok()?)?;
    lettering.content = content.to_owned();
    let (pixels, covered) = lettering.draw(width, height);
    let raster = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels).ok()?;
    let mut edit = Edit::new("Edit Text");
    for (key, value) in lettering.properties(covered) {
        edit = edit.then(SetProperty::new(layer, key, value));
    }
    Some(edit.then(ReplaceComponent::new(layer, raster, "Edit Text")))
}

/// Where a text layer's text is.
fn area_of(entity: &sue_core::Entity) -> Option<[f32; 4]> {
    let text = entity.property(BOX).and_then(Value::as_str)?;
    let values: Vec<f32> = text
        .split(',')
        .filter_map(|value| value.parse().ok())
        .collect();
    match values.as_slice() {
        &[x0, y0, x1, y1] => Some([x0, y0, x1, y1]),
        _ => None,
    }
}

impl SuePaint {
    /// A Text tool click at `p` (picture pixels): edits the current text
    /// layer when clicked on its words, centers new words in a balloon
    /// clicked inside, else starts new words there.
    pub(crate) fn text_at(&mut self, p: Pos2, cx: &mut AppletContext<'_>) {
        let arena = &cx.world.arena;
        let current = self
            .doc()
            .and_then(|doc| doc.layer)
            .and_then(|layer| arena.get_by_id(layer).ok())
            .filter(|entity| entity.class() == TEXT);
        if let Some(entity) = current
            && let Some([x0, y0, x1, y1]) = area_of(entity)
            && (x0 - 8.0..=x1 + 8.0).contains(&p.x)
            && (y0 - 8.0..=y1 + 8.0).contains(&p.y)
            && let Some(lettering) = Lettering::of(entity)
        {
            self.dialog = Some(crate::dialogs::Dialog::Text(Some(entity.id()), lettering));
            return;
        }
        let rgb = self.drawing_color();
        let mut lettering = Lettering::new([p.x, p.y], rgb);
        // Inside a balloon: centered in it.
        let balloon = self.picture().and_then(|picture| {
            crate::doc::layers(arena, picture)
                .into_iter()
                .rev()
                .find_map(|info| {
                    let entity = arena.get_by_id(info.id).ok()?;
                    let shapes = manga::decode_shapes(
                        entity.property(manga::SHAPES).and_then(Value::as_str)?,
                    );
                    shapes.into_iter().find_map(|shape| match shape {
                        Shape::Ellipse([x0, y0, x1, y1])
                            if (x0..x1).contains(&p.x) && (y0..y1).contains(&p.y) =>
                        {
                            Some([(x0 + x1) / 2.0, (y0 + y1) / 2.0, (y1 - y0).min(x1 - x0)])
                        }
                        _ => None,
                    })
                })
        });
        if let Some([x, y, room]) = balloon {
            lettering.at = [x, y];
            lettering.center = true;
            lettering.size = (room / 5.0).clamp(12.0, 96.0);
        }
        self.dialog = Some(crate::dialogs::Dialog::Text(None, lettering));
    }

    /// Writes `lettering` to `layer` (or a new text layer on top), drawn
    /// and kept, as one undo step.
    pub(crate) fn put_text(
        &mut self,
        cx: &mut AppletContext<'_>,
        layer: Option<EntityId>,
        lettering: &Lettering,
    ) {
        let Some([width, height]) = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .and_then(sue_model::picture::size)
        else {
            return;
        };
        if lettering.content.trim().is_empty() {
            return;
        }
        let (pixels, covered) = lettering.draw(width, height);
        let properties = lettering.properties(covered);
        match layer {
            Some(layer) => {
                let Ok(raster) =
                    RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels)
                else {
                    return;
                };
                let mut edit = Edit::new("Edit Text");
                for (key, value) in properties {
                    edit = edit.then(SetProperty::new(layer, key, value));
                }
                Self::apply(
                    cx,
                    edit.then(ReplaceComponent::new(layer, raster, "Edit Text")),
                );
            }
            None => {
                let name: String = lettering
                    .content
                    .lines()
                    .next()
                    .unwrap_or("Text")
                    .chars()
                    .take(24)
                    .collect();
                self.spawn_drawn_layer(cx, &name, TEXT, pixels, properties);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lettering_reads_back_from_its_layer() {
        let lettering = Lettering {
            content: "Hello\nthere".to_owned(),
            font: String::new(),
            size: 30.0,
            rgb: [20, 40, 60],
            at: [12.0, 34.0],
            center: true,
            vertical: true,
        };
        let (pixels, covered) = lettering.draw(200, 200);
        assert!(covered.is_some());
        assert!(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[3] == 255)
        );
        let mut entity = sue_core::Entity::new(
            "Text",
            RasterTile2D::from_bytes(200, 200, PixelFormat::Rgba8, pixels).unwrap(),
        )
        .with_class(TEXT);
        for (key, value) in lettering.properties(covered) {
            entity.properties_mut().set(key, value);
        }
        assert_eq!(Lettering::of(&entity), Some(lettering));
        assert!(area_of(&entity).is_some());
    }
}
