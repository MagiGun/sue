//! The marks Sue Paint draws over the picture to be dragged: the handles
//! of a transform box and of the camera and light table frames, the
//! rotation centre, puppet warp's pins, and the guide lines between them.
//!
//! They are Sue's own drawing, read from the theme so they follow it (the
//! default look is monochrome): every mark is two tones, the theme's text
//! color on its panel color, so it reads on a light picture and on a dark
//! one alike, and a mark being dragged or picked is lit in the theme's
//! focus color. Their sizes come from the spacing scale.

use egui::{Painter, Pos2, Rect, Shape, Stroke, Vec2};
use sue_wm::Theme;
use sue_wm::ui::Space;

/// How far from a mark, in points, a press still grabs it: a little more
/// than the mark, so a pen that lands beside it still takes it.
pub(crate) const GRAB_REACH: f32 = 10.0;

/// How long, in points, the stalk of a frame's turning handle is.
pub(crate) const STALK: f32 = 28.0;

/// The width of a guide line's light middle.
fn thin(theme: &Theme) -> f32 {
    theme.border_width.max(1.0)
}

/// A hairline over the picture: the theme's border width.
pub(crate) fn hair(theme: &Theme) -> f32 {
    thin(theme)
}

/// A firmer line over the picture (a ruler, a path being drawn).
pub(crate) fn firm(theme: &Theme) -> f32 {
    thin(theme) * 1.5
}

/// A bold line over the picture.
pub(crate) fn bold(theme: &Theme) -> f32 {
    thin(theme) * 2.0
}

/// The boldest line over the picture (what is being posed).
pub(crate) fn heavy(theme: &Theme) -> f32 {
    thin(theme) * 2.5
}

/// The radius of the smallest dot (the middle of the brush cursor).
pub(crate) fn speck(theme: &Theme) -> f32 {
    thin(theme)
}

/// The radius of a dot a little larger (a vanishing point's middle).
pub(crate) fn speck_mid(theme: &Theme) -> f32 {
    thin(theme) * 1.5
}

/// The radius of the dot cursor.
pub(crate) fn speck_big(theme: &Theme) -> f32 {
    thin(theme) * 2.5
}

/// The radius of a small knob (a corner being clicked out, a ruler end).
pub(crate) fn knob_small(theme: &Theme) -> f32 {
    theme.space(Space::S2) - theme.space(Space::S1) / 4.0
}

/// The radius of a knob (a gradient's ends, an effect line's ends).
pub(crate) fn knob(theme: &Theme) -> f32 {
    theme.space(Space::S2)
}

/// The radius of a small ring (a vanishing point, a ruler's centre).
pub(crate) fn ring_small(theme: &Theme) -> f32 {
    theme.space(Space::S2) + theme.space(Space::S1) / 2.0
}

/// The side of a vector line's control point.
pub(crate) fn control_point(theme: &Theme) -> f32 {
    theme.space(Space::S3) + theme.space(Space::S1) / 2.0
}

/// The side of a square mark (the eye level's handle).
pub(crate) fn square(theme: &Theme) -> f32 {
    theme.space(Space::S4)
}

/// The arm of a crosshair.
pub(crate) fn arm(theme: &Theme) -> f32 {
    theme.space(Space::S4)
}

/// The ring round a picked pin, `radius` out from it.
pub(crate) fn ring(painter: &Painter, at: Pos2, radius: f32, theme: &Theme) {
    painter.circle_stroke(at, radius, Stroke::new(thick(theme), theme.panel));
    painter.circle_stroke(at, radius, Stroke::new(thin(theme), theme.text));
}

/// The width of the dark edge under a guide line.
fn thick(theme: &Theme) -> f32 {
    thin(theme) + sue_wm::Theme::STRONG_STROKE
}

/// A guide line through `points` (screen), closed when `closed`: the
/// panel color under the text color.
pub(crate) fn guide(painter: &Painter, points: Vec<Pos2>, closed: bool, theme: &Theme) {
    let under = Stroke::new(thick(theme), theme.panel);
    let over = Stroke::new(thin(theme), theme.text);
    if closed {
        painter.add(Shape::closed_line(points.clone(), under));
        painter.add(Shape::closed_line(points, over));
    } else {
        painter.add(Shape::line(points.clone(), under));
        painter.add(Shape::line(points, over));
    }
}

/// The two tones of a selection path: its light rim (white, nearly
/// opaque) and its dark core (near black). They lie on the picture, so
/// they are fixed, not the theme's.
pub(crate) fn path_tones() -> [egui::Color32; 2] {
    [
        egui::Color32::from_rgba_premultiplied(242, 242, 242, 242),
        egui::Color32::from_rgb(16, 16, 16),
    ]
}

/// The path of a selection being drawn (lasso, lasso fill, Shrink
/// selection, a polyline selection) through `points` (screen), closed
/// when `closed`: a dark core on a light rim on both sides. It is not the
/// theme's colors but black and white, since what it lies on is the
/// picture, which can be any color: on white paper the core reads, on
/// black ink the rim, on any middle color both.
pub(crate) fn selection_path(painter: &Painter, points: Vec<Pos2>, closed: bool, theme: &Theme) {
    let core = firm(theme);
    let rim = core + 2.0 * thin(theme);
    let [light, dark] = path_tones();
    let strokes = [Stroke::new(rim, light), Stroke::new(core, dark)];
    for stroke in strokes {
        painter.add(if closed {
            Shape::closed_line(points.clone(), stroke)
        } else {
            Shape::line(points.clone(), stroke)
        });
    }
}

/// A faint guide line, for what only shows the way (a path, a frame
/// before or after this one).
pub(crate) fn faint(painter: &Painter, points: Vec<Pos2>, closed: bool, theme: &Theme) {
    let stroke = Stroke::new(thin(theme), theme.muted);
    if closed {
        painter.add(Shape::closed_line(points, stroke));
    } else {
        painter.add(Shape::line(points, stroke));
    }
}

/// A line of a mesh laid over the picture (puppet warp's): one thin line
/// in the theme's border color, quieter than a guide, so the drawing under
/// it still reads.
pub(crate) fn mesh(painter: &Painter, points: Vec<Pos2>, theme: &Theme) {
    painter.add(Shape::closed_line(
        points,
        Stroke::new(thin(theme), theme.border),
    ));
}

/// A dashed guide line from `a` to `b`.
pub(crate) fn dashed(painter: &Painter, a: Pos2, b: Pos2, theme: &Theme) {
    let dash = theme.space(Space::S2);
    painter.extend(Shape::dashed_line(
        &[a, b],
        Stroke::new(thick(theme), theme.panel),
        dash,
        dash,
    ));
    painter.extend(Shape::dashed_line(
        &[a, b],
        Stroke::new(thin(theme), theme.text),
        dash,
        dash,
    ));
}

/// A square handle at `at`, lit while `held`.
pub(crate) fn handle(painter: &Painter, at: Pos2, held: bool, theme: &Theme) {
    let square = Rect::from_center_size(at, Vec2::splat(theme.space(Space::S4)));
    let fill = if held { theme.focus() } else { theme.text };
    painter.rect_filled(square, theme.radius(sue_wm::ui::Radius::Xs), fill);
    painter.rect_stroke(
        square,
        theme.radius(sue_wm::ui::Radius::Xs),
        Stroke::new(thin(theme), theme.panel),
        egui::StrokeKind::Outside,
    );
}

/// A round handle at `at` (a pin, a lattice point), lit while `held`.
pub(crate) fn dot(painter: &Painter, at: Pos2, held: bool, theme: &Theme) {
    let radius = theme.space(Space::S2) + theme.space(Space::S1);
    let fill = if held { theme.focus() } else { theme.text };
    painter.circle_filled(at, radius, fill);
    painter.circle_stroke(at, radius, Stroke::new(thin(theme), theme.panel));
}

/// The rotation centre at `at`: a ring with a cross, lit while `held`.
pub(crate) fn pivot(painter: &Painter, at: Pos2, held: bool, theme: &Theme) {
    let radius = theme.space(Space::S3);
    let ink = if held { theme.focus() } else { theme.text };
    let arm = Vec2::new(radius + theme.space(Space::S2), 0.0);
    for stroke in [
        Stroke::new(thick(theme), theme.panel),
        Stroke::new(thin(theme), ink),
    ] {
        painter.circle_stroke(at, radius, stroke);
        painter.line_segment([at - arm, at + arm], stroke);
        painter.line_segment([at - arm.yx(), at + arm.yx()], stroke);
    }
}

/// A pin at `at` turned `angle` radians, with a tick that shows which
/// way it faces; lit while `held`.
pub(crate) fn pin(painter: &Painter, at: Pos2, angle: f32, held: bool, theme: &Theme) {
    dot(painter, at, held, theme);
    let reach = theme.space(Space::S5);
    let (sin, cos) = angle.sin_cos();
    let tip = at + Vec2::new(cos, sin) * reach;
    let stroke = Stroke::new(thin(theme), if held { theme.focus() } else { theme.text });
    painter.line_segment([at, tip], Stroke::new(thick(theme), theme.panel));
    painter.line_segment([at, tip], stroke);
}

/// A handle on a stalk from `from` to `at`: the one that turns a frame.
pub(crate) fn stalk(painter: &Painter, from: Pos2, at: Pos2, held: bool, theme: &Theme) {
    guide(painter, vec![from, at], false, theme);
    dot(painter, at, held, theme);
}

/// The text `label` in a small chip at `at` (its top left), for the
/// name of a frame.
pub(crate) fn tag(painter: &Painter, at: Pos2, label: &str, theme: &Theme) {
    let font = theme.text_font(sue_wm::ui::TextRole::Caption);
    let galley = painter.layout_no_wrap(label.to_owned(), font, theme.text);
    let pad = theme.space(Space::S1);
    let rect = Rect::from_min_size(at, galley.size() + Vec2::splat(pad + pad));
    painter.rect_filled(rect, theme.radius(sue_wm::ui::Radius::Xs), theme.panel);
    painter.galley(rect.min + Vec2::splat(pad), galley, theme.text);
}

#[cfg(test)]
mod selection_path_tests {
    use super::*;

    /// The contrast ratio of two sRGB colors, as WCAG reckons it.
    fn contrast(a: [u8; 3], b: [u8; 3]) -> f32 {
        let luminance = |color: [u8; 3]| {
            let linear = color.map(|channel| {
                let value = f32::from(channel) / 255.0;
                if value <= 0.040_45 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            });
            0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
        };
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn a_selection_path_reads_on_any_color_of_picture() {
        let [light, dark] = path_tones().map(|tone| [tone.r(), tone.g(), tone.b()]);
        // Every gray, and the pure and mixed colors of a palette: one of
        // the two tones stands out from each by at least 3 to 1 (what
        // WCAG asks of a mark that is not text).
        let mut pictures: Vec<[u8; 3]> = (0..=255_u8).map(|value| [value; 3]).collect();
        for r in [0_u8, 128, 255] {
            for g in [0_u8, 128, 255] {
                for b in [0_u8, 128, 255] {
                    pictures.push([r, g, b]);
                }
            }
        }
        for picture in pictures {
            let best = contrast(dark, picture).max(contrast(light, picture));
            assert!(best >= 3.0, "{picture:?}: {best}");
        }
        // White paper and black ink, the two that the old line lost.
        assert!(contrast(dark, [255; 3]) > 15.0);
        assert!(contrast(light, [0; 3]) > 15.0);
    }

    #[test]
    fn the_light_rim_shows_on_both_sides_of_the_dark_core() {
        let theme = Theme::from_rice(&sue_rice::RiceSchema::new());
        let core = firm(&theme);
        let rim = core + 2.0 * thin(&theme);
        assert!(rim - core >= 2.0, "a point of rim each side");
        assert!(core >= 1.5, "the core is firmer than a hairline");
    }
}
