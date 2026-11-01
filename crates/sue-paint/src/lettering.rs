//! Text layers (Clip Studio Paint's Text tool): a raster layer of class
//! [`TEXT`] that keeps its words, font, size, ink and placement, and draws
//! them again with `sue_type::raster` after every edit, so text stays sharp
//! and editable. Clicked inside a balloon, the words center in it.

use egui::{Pos2, pos2};
use sue_core::txn::{ReplaceComponent, SetProperty};
use sue_core::{Color, EntityId, PixelFormat, RasterTile2D, Value};
use sue_wm::AppletContext;

use crate::manga;
use crate::studio::SuePaint;
use sue_type::raster::Justify;

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
/// Extra room between letters, in ems (`Float`).
const TRACKING: &str = "text.tracking";
/// Line spacing, times the font's own (`Float`).
const LEADING: &str = "text.leading";
/// How lines line up: `start`, `middle` or `end` (`Text`).
const JUSTIFY: &str = "text.justify";
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
    /// Extra room between letters, in ems.
    pub tracking: f32,
    /// Line spacing, times the font's own (0 for the font's own).
    pub leading: f32,
    /// How the lines line up with each other.
    pub justify: Justify,
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
            tracking: 0.0,
            leading: 0.0,
            justify: Justify::Start,
        }
    }

    /// The words as `sue_type` lays them out.
    pub(crate) fn spec(&self) -> sue_type::raster::TextSpec<'_> {
        sue_type::raster::TextSpec {
            text: &self.content,
            font: &self.font,
            size: self.size,
            rgb: self.rgb,
            at: self.at,
            center: self.center,
            vertical: self.vertical,
            tracking: self.tracking,
            leading: self.leading,
            justify: self.justify,
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
            tracking: number(TRACKING, 0.0),
            leading: number(LEADING, 0.0),
            // Centered words from before justification had centered lines.
            justify: match entity.property(JUSTIFY).and_then(Value::as_str) {
                Some("middle") => Justify::Middle,
                Some("end") => Justify::End,
                Some(_) => Justify::Start,
                None if flag(CENTER) => Justify::Middle,
                None => Justify::Start,
            },
        })
    }

    /// The layer's pixels for a `width` by `height` picture, and where the
    /// text is.
    fn draw(&self, width: u32, height: u32) -> (Vec<u8>, Option<[f32; 4]>) {
        let mut pixels = vec![0; (width * height * 4) as usize];
        let covered = sue_type::raster::draw_text(&mut pixels, width, height, &self.spec());
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
            (TRACKING, Value::from(f64::from(self.tracking))),
            (LEADING, Value::from(f64::from(self.leading))),
            (
                JUSTIFY,
                Value::from(match self.justify {
                    Justify::Start => "start",
                    Justify::Middle => "middle",
                    Justify::End => "end",
                }),
            ),
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
        .unwrap_or(&tr!("text"))
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
    let mut edit = Edit::new(tr!("edit-text"));
    for (key, value) in lettering.properties(covered) {
        edit = edit.then(SetProperty::new(layer, key, value));
    }
    Some(edit.then(ReplaceComponent::new(layer, raster, tr!("edit-text"))))
}

/// Where a text layer's text is.
pub(crate) fn area_of(entity: &sue_core::Entity) -> Option<[f32; 4]> {
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
    /// A Text tool click at `p` (picture pixels), as in Clip Studio Paint:
    /// types into the current text layer when clicked on its words,
    /// centers new words in a balloon clicked inside, else starts new
    /// words there, typed right on the canvas.
    pub(crate) fn text_at(&mut self, p: Pos2, cx: &mut AppletContext<'_>) {
        // A click on the words being typed moves the caret there.
        if let Some(typing) = self.typing.as_mut()
            && typing.holds(p)
        {
            typing.place = Some(typing.index_at(p));
            return;
        }
        self.finish_typing(cx);
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
            let layer = entity.id();
            self.begin_typing(cx, Some(layer), lettering, Some(p));
            return;
        }
        let look = self.text_look();
        let mut lettering = Lettering::new([p.x, p.y], look.rgb);
        look.put(&mut lettering);
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
                    shapes.into_iter().find_map(|shape| {
                        let [x0, y0, x1, y1] = shape.balloon()?;
                        ((x0..x1).contains(&p.x) && (y0..y1).contains(&p.y))
                            .then(|| [(x0 + x1) / 2.0, (y0 + y1) / 2.0, (y1 - y0).min(x1 - x0)])
                    })
                })
        });
        if let Some([x, y, room]) = balloon {
            lettering.at = [x, y];
            lettering.center = true;
            lettering.justify = Justify::Middle;
            lettering.size = (room / 5.0).clamp(12.0, 96.0);
        }
        self.begin_typing(cx, None, lettering, None);
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
                let mut edit = Edit::new(tr!("edit-text"));
                for (key, value) in properties {
                    edit = edit.then(SetProperty::new(layer, key, value));
                }
                Self::apply(
                    cx,
                    edit.then(ReplaceComponent::new(layer, raster, tr!("edit-text"))),
                );
            }
            None => {
                let name: String = lettering
                    .content
                    .lines()
                    .next()
                    .unwrap_or(&tr!("text"))
                    .chars()
                    .take(24)
                    .collect();
                self.spawn_drawn_layer(cx, &name, TEXT, pixels, properties);
            }
        }
    }
}

/// Words being typed on the canvas (Clip Studio Paint's Text tool): the
/// keys go to them, the canvas shows them as they grow, and they are kept
/// as one undo step when the typing ends.
pub(crate) struct Typing {
    /// The text layer being edited; `None` for new words.
    pub layer: Option<EntityId>,
    /// The words as typed so far.
    pub lettering: Lettering,
    /// The edited layer's pixels as they were (it shows empty while its
    /// words are typed over it).
    before: Option<Vec<u8>>,
    /// The picture typed in.
    picture: EntityId,
    /// The words drawn, what they say, and where (picture pixels).
    drawn: Option<(egui::TextureHandle, Lettering, [f32; 4])>,
    /// The caret's glide and blink.
    glide: sue_type::caret::CaretGlide,
    /// Where the caret goes next frame (a click on the words).
    place: Option<usize>,
    /// The Text tool's settings and the ink the words last took.
    settings: Look,
}

/// How the Text tool letters: everything but the words and where.
#[derive(Clone, Debug, PartialEq)]
struct Look {
    font: String,
    size: f32,
    vertical: bool,
    center: bool,
    rgb: [u8; 3],
    tracking: f32,
    leading: f32,
    justify: Justify,
}

impl Look {
    /// `lettering` lettered this way.
    fn put(&self, lettering: &mut Lettering) {
        lettering.font.clone_from(&self.font);
        lettering.size = self.size;
        lettering.vertical = self.vertical;
        lettering.center = self.center;
        lettering.rgb = self.rgb;
        lettering.tracking = self.tracking;
        lettering.leading = self.leading;
        lettering.justify = self.justify;
    }
}

impl Typing {
    fn spec(&self) -> sue_type::raster::TextSpec<'_> {
        self.lettering.spec()
    }

    /// The picture area the words take, with room for letters that lean
    /// out of their places.
    fn area(&self) -> Option<[f32; 4]> {
        let room = self.lettering.size * 0.6;
        sue_type::raster::caret_slots(&self.spec())
            .iter()
            .flat_map(|slot| [slot.top, slot.bottom])
            .fold(None, |area: Option<[f32; 4]>, [x, y]| {
                Some(area.map_or([x, y, x, y], |[x0, y0, x1, y1]| {
                    [x0.min(x), y0.min(y), x1.max(x), y1.max(y)]
                }))
            })
            .map(|[x0, y0, x1, y1]| [x0 - room, y0 - room, x1 + room, y1 + room])
    }

    /// The character nearest `p` (picture pixels).
    fn index_at(&self, p: Pos2) -> usize {
        let far = |slot: &sue_type::raster::CaretSlot| {
            pos2(
                (slot.top[0] + slot.bottom[0]) / 2.0,
                (slot.top[1] + slot.bottom[1]) / 2.0,
            )
            .distance_sq(p)
        };
        sue_type::raster::caret_slots(&self.spec())
            .iter()
            .min_by(|a, b| far(a).total_cmp(&far(b)))
            .map_or(0, |slot| slot.index)
    }

    /// Whether `p` (picture pixels) is on the words.
    fn holds(&self, p: Pos2) -> bool {
        self.area()
            .is_some_and(|[x0, y0, x1, y1]| (x0..=x1).contains(&p.x) && (y0..=y1).contains(&p.y))
    }
}

/// The hidden text box that takes the typed keys.
fn typing_id() -> egui::Id {
    egui::Id::unique("sue-paint-typing")
}

impl SuePaint {
    /// Starts typing `lettering` on the canvas, into `layer` or as new
    /// words, the caret at `place` (picture pixels) or at the end.
    fn begin_typing(
        &mut self,
        cx: &mut AppletContext<'_>,
        layer: Option<EntityId>,
        lettering: Lettering,
        place: Option<Pos2>,
    ) {
        self.finish_typing(cx);
        let Some(picture) = self.picture() else {
            return;
        };
        let mut before = None;
        if let Some(layer) = layer
            && let Some(bytes) =
                crate::doc::raster_mut(&mut cx.world.arena, layer).and_then(RasterTile2D::bytes_mut)
        {
            // The old words make way for the ones being typed.
            before = Some(bytes.to_vec());
            bytes.fill(0);
            self.refresh_all(cx);
        }
        // The Text tool shows the words' own settings.
        let sub = self.toolbox.current_mut();
        if sub.tool == crate::tools::Tool::Text {
            sub.font.clone_from(&lettering.font);
            sub.tip.size = lettering.size;
            sub.kind = u8::from(lettering.vertical);
            sub.whole = lettering.center;
            sub.tracking = lettering.tracking;
            sub.leading = lettering.leading;
            sub.justify = lettering.justify as u8;
        }
        let settings = self.text_look();
        let mut typing = Typing {
            settings,
            layer,
            lettering,
            before,
            picture,
            drawn: None,
            glide: sue_type::caret::CaretGlide::default(),
            place: None,
        };
        typing.place = Some(match place {
            Some(p) => typing.index_at(p),
            None => typing.lettering.content.chars().count(),
        });
        self.typing = Some(typing);
    }

    /// Ends the typing: the words are kept as one undo step (none when
    /// nothing changed).
    pub(crate) fn finish_typing(&mut self, cx: &mut AppletContext<'_>) {
        let Some(typing) = self.typing.take() else {
            return;
        };
        // Kept in the picture typed in, even when another tab is in front.
        let Some(index) = self
            .docs
            .iter()
            .position(|doc| doc.picture == typing.picture)
        else {
            return;
        };
        let active = std::mem::replace(&mut self.active, index);
        self.keep_typed(cx, &typing);
        self.active = active;
    }

    fn keep_typed(&mut self, cx: &mut AppletContext<'_>, typing: &Typing) {
        if let (Some(layer), Some(before)) = (typing.layer, typing.before.as_ref())
            && let Some(bytes) =
                crate::doc::raster_mut(&mut cx.world.arena, layer).and_then(RasterTile2D::bytes_mut)
            && bytes.len() == before.len()
        {
            bytes.copy_from_slice(before);
            self.refresh_all(cx);
        }
        let unchanged = typing.layer.is_some_and(|layer| {
            cx.world
                .arena
                .get_by_id(layer)
                .ok()
                .and_then(Lettering::of)
                .is_some_and(|was| was == typing.lettering)
        });
        if !unchanged {
            self.put_text(cx, typing.layer, &typing.lettering);
        }
    }

    /// How the Text tool letters now (the ink is the drawing color).
    fn text_look(&self) -> Look {
        let sub = self.toolbox.current();
        Look {
            font: sub.font.clone(),
            size: sub.tip.size,
            vertical: sub.kind == 1,
            center: sub.whole,
            rgb: self.drawing_color(),
            tracking: sub.tracking,
            leading: sub.leading,
            justify: match sub.justify {
                1 => Justify::Middle,
                2 => Justify::End,
                _ => Justify::Start,
            },
        }
    }

    fn refresh_all(&mut self, cx: &AppletContext<'_>) {
        if let Some(doc) = self.doc_mut() {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// The words being typed: drawn where they go, with a caret, the keys
    /// taken by a hidden text box. Esc, Ctrl and Enter, another tool or
    /// another document keep them.
    pub(crate) fn typing_ui(
        &mut self,
        ui: &mut egui::Ui,
        viewport: egui::Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let Some(typing) = self.typing.as_ref() else {
            return;
        };
        let done = self.toolbox.tool() != crate::tools::Tool::Text
            || self.picture() != Some(typing.picture)
            || self.dialog.is_some()
            || ui.input_mut(|input| {
                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    || input.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)
            });
        if done {
            self.finish_typing(cx);
            return;
        }
        let Some(view) = self.doc().map(|doc| doc.view) else {
            return;
        };
        let rice = cx.rice();
        let motion = sue_type::caret::CaretMotion::new(
            rice.get_bool(sue_wm::SMOOTH_CARET_KEY).unwrap_or(true),
            rice.get_i64(sue_wm::CARET_GLIDE_KEY)
                .unwrap_or(sue_type::caret::GLIDE_MS),
            cx.view.profile.budget().spring_physics,
            &ui.visuals().text_cursor,
        );
        let look = self.text_look();
        let Some(typing) = self.typing.as_mut() else {
            return;
        };
        // A setting changed in Tool Property or a new color changes the
        // words being typed.
        if look != typing.settings {
            look.put(&mut typing.lettering);
            typing.settings = look;
        }
        let screen = |[x, y]: [f32; 2]| view.to_screen(viewport, size, pos2(x, y));
        let ctx = ui.ctx().clone();
        let id = typing_id();
        if let Some(index) = typing.place.take() {
            let mut state = egui::text_edit::TextEditState::load(&ctx, id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(index),
                )));
            state.store(&ctx, id);
        }
        let slots = sue_type::raster::caret_slots(&typing.spec());
        let last = slots.len().saturating_sub(1);
        let caret = egui::text_edit::TextEditState::load(&ctx, id)
            .and_then(|state| state.cursor.char_range())
            .map_or(last, |range| usize::from(range.primary.index).min(last));
        let (top, bottom) = slots
            .get(caret)
            .map_or((typing.lettering.at, typing.lettering.at), |slot| {
                (slot.top, slot.bottom)
            });
        // The hidden box sits on the caret, so a system's input method
        // opens its window there.
        let hidden = egui::Rect::from_min_size(screen(bottom), egui::vec2(1.0, 1.0));
        let edited = ui
            .scope_builder(egui::UiBuilder::new().max_rect(hidden), |ui| {
                let visuals = ui.visuals_mut();
                visuals.text_cursor.stroke = egui::Stroke::NONE;
                visuals.selection.bg_fill = egui::Color32::TRANSPARENT;
                visuals.selection.stroke = egui::Stroke::NONE;
                // Lines never wrap in the tiny box, so End and the arrows
                // go by the words' own lines.
                let mut lines = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _: f32| {
                    let job = egui::text::LayoutJob::simple(
                        text.as_str().to_owned(),
                        egui::FontId::default(),
                        egui::Color32::TRANSPARENT,
                        f32::INFINITY,
                    );
                    ui.fonts_mut(|fonts| fonts.layout_job(job))
                };
                let response = ui.put(
                    hidden,
                    egui::TextEdit::multiline(&mut typing.lettering.content)
                        .id(id)
                        .layouter(&mut lines)
                        .frame(egui::Frame::NONE)
                        .text_color(egui::Color32::TRANSPARENT)
                        .desired_rows(1)
                        .desired_width(1.0),
                );
                // The keys stay with the words until the typing ends.
                response.request_focus();
                response.changed()
            })
            .inner;
        // The words, drawn again when they change.
        let stale = typing
            .drawn
            .as_ref()
            .is_none_or(|(_, drawn, _)| *drawn != typing.lettering);
        if (edited || stale)
            && let Some([x0, y0, x1, y1]) = typing.area()
        {
            let (x0, y0) = (x0.floor(), y0.floor());
            let (w, h) = (
                (x1 - x0).ceil().max(1.0) as u32,
                (y1 - y0).ceil().max(1.0) as u32,
            );
            let mut shifted = typing.lettering.clone();
            shifted.at = [shifted.at[0] - x0, shifted.at[1] - y0];
            let (pixels, _) = shifted.draw(w, h);
            let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &pixels);
            let texture = ctx.load_texture("sue-paint-typing", image, egui::TextureOptions::LINEAR);
            typing.drawn = Some((
                texture,
                typing.lettering.clone(),
                [x0, y0, x0 + w as f32, y0 + h as f32],
            ));
        }
        let painter = ui.painter().with_clip_rect(viewport);
        if let Some((texture, _, [x0, y0, x1, y1])) = &typing.drawn {
            let corners = [[*x0, *y0], [*x1, *y0], [*x1, *y1], [*x0, *y1]].map(screen);
            let mut mesh = egui::Mesh::with_texture(texture.id());
            let uvs = [
                pos2(0.0, 0.0),
                pos2(1.0, 0.0),
                pos2(1.0, 1.0),
                pos2(0.0, 1.0),
            ];
            for (pos, uv) in corners.into_iter().zip(uvs) {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos,
                    uv,
                    color: egui::Color32::WHITE,
                });
            }
            mesh.indices.extend([0, 1, 2, 0, 2, 3]);
            painter.add(egui::Shape::mesh(mesh));
        }
        // Clip Studio Paint's frame around the words being typed.
        if let Some([x0, y0, x1, y1]) = typing.area() {
            let room = typing.lettering.size * 0.5;
            let corners = [
                [x0 + room, y0 + room],
                [x1 - room, y0 + room],
                [x1 - room, y1 - room],
                [x0 + room, y1 - room],
            ]
            .map(screen);
            let mut line = corners.to_vec();
            line.push(corners[0]);
            painter.extend(egui::Shape::dashed_line(
                &line,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(64, 128, 255)),
                4.0,
                3.0,
            ));
        }
        // The caret glides and blinks as in Sue's editors.
        let look = typing.glide.frame(
            sue_type::caret::Frame {
                target: egui::Rect::from_two_pos(pos2(top[0], top[1]), pos2(bottom[0], bottom[1])),
                cause: ((caret as u64) << 32) | typing.lettering.content.len() as u64,
                view: u64::from(view.zoom.to_bits()),
                shown: true,
                viewport: egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(size[0] as f32, size[1] as f32),
                ),
                now: ctx.input(|input| input.time),
                dragging: false,
            },
            motion,
        );
        if look.lit {
            let bar = look.bar;
            let (a, b) = if typing.lettering.vertical {
                ([bar.max.x, bar.min.y], [bar.min.x, bar.min.y])
            } else {
                ([bar.min.x, bar.min.y], [bar.min.x, bar.max.y])
            };
            // Dark on a light edge, so it shows on any picture.
            for stroke in [
                egui::Stroke::new(3.5, egui::Color32::from_white_alpha(200)),
                egui::Stroke::new(1.5, egui::Color32::from_rgb(20, 20, 20)),
            ] {
                painter.line_segment([screen(a), screen(b)], stroke);
            }
        }
        if look.moving {
            ctx.request_repaint();
        } else if let Some(wake) = look.wake {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wake));
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
            tracking: 0.25,
            leading: 1.5,
            justify: Justify::End,
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
