//! Shapes drawn on a layer: the Figure tool's lines, rectangles,
//! ellipses and polygons, its curves, and the points around an ellipse.
//!
//! What a Figure drag will draw is worked out once, by [`outline`], so
//! the preview over the canvas and the line laid on release are the same
//! line: Shift keeps a straight line to every 45 degrees and a box shape
//! square, the sub tool's Aspect keeps its ratio, and with the rulers on
//! a straight line follows the ruler it starts on and a box shape lies in
//! the perspective ruler's plane.

use egui::{Painter, Pos2, Shape, Stroke};
use sue_core::Arena;
use sue_wm::{AppletContext, ArenaEdit};

use crate::brush::Stroke as BrushStroke;
use crate::color;
use crate::doc;
use crate::fill;
use crate::rulers::{Rulers, Snap};
use crate::select::{Aspect, Op, Selection};
use crate::studio::{Doc, SuePaint};
use crate::tools::{
    BEZIER_CURVE, CONTINUOUS_CURVE, CURVE, POLYGON, POLYLINE, SATURATED_LINES, SPEED_LINES,
    SubTool, Tool,
};

/// Points around an ellipse inside two corners.
pub(super) fn ellipse_points(from: Pos2, to: Pos2) -> Vec<[f32; 2]> {
    let center = from + (to - from) / 2.0;
    let (rx, ry) = (((to.x - from.x) / 2.0).abs(), ((to.y - from.y) / 2.0).abs());
    (0..=96)
        .map(|step| {
            let t = step as f32 / 96.0 * std::f32::consts::TAU;
            [center.x + rx * t.cos(), center.y + ry * t.sin()]
        })
        .collect()
}

/// A Figure line under way between clicks.
#[derive(Clone, Debug, Default)]
pub(crate) struct FigureDraft {
    /// Curve: the ends laid by the drag, waiting for the pointer to bend
    /// the line and a click to set it.
    pub bend: Option<Bend>,
    /// Bezier Curve: the handle pulled out of each anchor (the anchors
    /// are the studio's `polyline`); an anchor clicked without a drag has
    /// its own position.
    pub handles: Vec<[f32; 2]>,
}

/// A Curve's ends, laid by a drag, before the click that bends it.
#[derive(Clone, Debug)]
pub(crate) struct Bend {
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub sub: SubTool,
}

/// What a Figure line may follow: the picture's rulers and which of them
/// snap, and how near a line must start to one to follow it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Guides {
    rulers: Rulers,
    /// Snap to Ruler: the linear, curve and figure rulers.
    ruler: bool,
    /// Snap to Special Ruler: the perspective and special rulers.
    special: bool,
    /// Picture pixels.
    reach: f32,
}

impl Guides {
    /// The rulers of `doc`'s picture, as the brush follows them.
    pub(crate) fn of(arena: &Arena, doc: &Doc) -> Self {
        if !doc.show_rulers {
            return Self::default();
        }
        Self {
            rulers: Rulers::of(arena, doc.picture),
            ruler: doc.snap_ruler,
            special: doc.ruler_on && crate::commands::special_snaps(arena, doc.picture),
            reach: 24.0 / doc.view.zoom.max(0.01),
        }
    }

    /// `to` once a straight line from `from` follows the ruler it starts
    /// on; `None` when it starts on none.
    pub(crate) fn line_end(&self, from: [f32; 2], to: [f32; 2]) -> Option<[f32; 2]> {
        let mut snap = Snap::begin(&self.rulers, from, self.ruler, self.special, self.reach)?;
        snap.follow(to, 0.0).or_else(|| Some(snap.project(to)))
    }

    /// The four corners of the box dragged from `a` to `c` in the
    /// perspective ruler's plane: upright (or toward the third vanishing
    /// point) and toward whichever vanishing point the drag heads most
    /// for. `None` without a perspective ruler that snaps.
    fn perspective_box(&self, a: [f32; 2], c: [f32; 2]) -> Option<[[f32; 2]; 4]> {
        let perspective = self.rulers.perspective.as_ref().filter(|_| self.special)?;
        let count = perspective.points.len();
        if count == 0 {
            return None;
        }
        let drag = unit([c[0] - a[0], c[1] - a[1]])?;
        let toward = |point: [f32; 2], at: [f32; 2]| unit([point[0] - at[0], point[1] - at[1]]);
        // The vanishing point the drag runs toward the most (of the
        // first two, the ones on the eye level).
        let across = perspective.points[..count.min(2)]
            .iter()
            .copied()
            .max_by(|p, q| {
                let along = |point: [f32; 2]| {
                    toward(point, a).map_or(0.0, |d| (d[0] * drag[0] + d[1] * drag[1]).abs())
                };
                along(*p).total_cmp(&along(*q))
            })?;
        let upright = perspective
            .directions(a)
            .last()
            .copied()
            .filter(|_| count < 3)
            .unwrap_or([0.0, 1.0]);
        let down = |at: [f32; 2]| {
            if count >= 3 {
                toward(perspective.points[2], at).unwrap_or(upright)
            } else {
                upright
            }
        };
        let side = |at: [f32; 2]| toward(across, at);
        let line = |at: [f32; 2], d: [f32; 2]| [at, [at[0] + d[0], at[1] + d[1]]];
        let b = crate::rulers::crossing(line(a, side(a)?), line(c, down(c)))?;
        let d = crate::rulers::crossing(line(a, down(a)), line(c, side(c)?))?;
        Some([a, b, c, d])
    }
}

/// A unit vector along `v`, or `None` for a zero one.
fn unit(v: [f32; 2]) -> Option<[f32; 2]> {
    let length = v[0].hypot(v[1]);
    (length > 1e-6).then(|| [v[0] / length, v[1] / length])
}

/// A drawn Figure: its points in picture pixels, and whether the last
/// joins the first.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Outline {
    pub points: Vec<[f32; 2]>,
    pub closed: bool,
}

/// `true` for the Figure sub tools that fill their shape.
pub(crate) fn filled(mode: u8) -> bool {
    matches!(mode, 3 | 4)
}

/// The direction `angle` (radians) of a line kept to its steps: every
/// `step_degrees` when the sub tool has a Step of angle, else every 45
/// degrees while Shift is held; `None` when it runs freely.
pub(crate) fn snapped_angle(angle: f32, step_degrees: f32, shift: bool) -> Option<f32> {
    let step = if step_degrees > 0.0 {
        step_degrees.to_radians()
    } else if shift {
        std::f32::consts::FRAC_PI_4
    } else {
        return None;
    };
    Some((angle / step).round() * step)
}

/// Where a Figure drag from `from` ends once Shift (`shift`), the sub
/// tool's Step of angle and its Aspect have their say: a straight line
/// to every step (45 degrees with Shift), a box shape square or at its
/// ratio.
pub(crate) fn constrained(sub: &SubTool, from: [f32; 2], to: [f32; 2], shift: bool) -> [f32; 2] {
    if sub.mode == 0 || sub.mode == CURVE || sub.mode == POLYLINE {
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        let Some(angle) = snapped_angle(dy.atan2(dx), sub.angle_step, shift) else {
            return to;
        };
        let length = dx.hypot(dy);
        return [
            from[0] + angle.cos() * length,
            from[1] + angle.sin() * length,
        ];
    }
    let aspect = if shift { Aspect::Square } else { sub.aspect };
    aspect.constrain(from, to)
}

/// What a Figure drag of `sub` from `from` to `to` draws (picture
/// pixels), with Shift held or not, following `guides`.
pub(crate) fn outline(
    sub: &SubTool,
    from: [f32; 2],
    to: [f32; 2],
    shift: bool,
    guides: &Guides,
) -> Outline {
    let open = |points| Outline {
        points,
        closed: false,
    };
    if sub.mode == 0 || sub.mode == CURVE {
        // A straight line (a Curve before it bends) follows the ruler it
        // starts on, else Shift.
        let end = guides
            .line_end(from, to)
            .unwrap_or_else(|| constrained(sub, from, to, shift));
        return open(vec![from, end]);
    }
    let to = constrained(sub, from, to, shift);
    // The shape in a box from (0, 0) to (1, 1), then laid in the box
    // dragged (or the perspective ruler's plane).
    let (width, height) = ((to[0] - from[0]).abs(), (to[1] - from[1]).abs());
    let unit_points = match sub.mode {
        2 | 4 => (0..=96)
            .map(|step| {
                let t = step as f32 / 96.0 * std::f32::consts::TAU;
                [0.5 + 0.5 * t.cos(), 0.5 + 0.5 * t.sin()]
            })
            .collect(),
        POLYGON => polygon_points(sub.corners, sub.roundness, [width, height]),
        _ => rounded_box(sub.roundness, [width, height]),
    };
    let points = match guides.perspective_box(from, to) {
        Some(quad) => {
            let map = SquareToQuad::new(quad);
            unit_points.into_iter().map(|p| map.apply(p)).collect()
        }
        None => {
            let (min, max) = (
                [from[0].min(to[0]), from[1].min(to[1])],
                [from[0].max(to[0]), from[1].max(to[1])],
            );
            unit_points
                .into_iter()
                .map(|[u, v]| {
                    [
                        min[0] + u * (max[0] - min[0]),
                        min[1] + v * (max[1] - min[1]),
                    ]
                })
                .collect()
        }
    };
    Outline {
        points,
        closed: true,
    }
}

/// A rectangle in the unit box with corners rounded by `roundness`
/// picture pixels, for a box of `size` picture pixels.
fn rounded_box(roundness: f32, size: [f32; 2]) -> Vec<[f32; 2]> {
    let radius = roundness.max(0.0).min(size[0].min(size[1]) / 2.0);
    if radius < 0.5 {
        return vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    }
    let (rx, ry) = (radius / size[0].max(1e-3), radius / size[1].max(1e-3));
    let corners = [
        ([1.0 - rx, ry], -90.0_f32),
        ([1.0 - rx, 1.0 - ry], 0.0),
        ([rx, 1.0 - ry], 90.0),
        ([rx, ry], 180.0),
    ];
    let mut out = Vec::new();
    for (center, start) in corners {
        for step in 0..=8 {
            let angle = (start + step as f32 * 90.0 / 8.0).to_radians();
            out.push([center[0] + rx * angle.cos(), center[1] + ry * angle.sin()]);
        }
    }
    out
}

/// A polygon of `corners` corners in the unit box, the first at the top,
/// its corners rounded by `roundness` picture pixels for a box of `size`.
fn polygon_points(corners: u32, roundness: f32, size: [f32; 2]) -> Vec<[f32; 2]> {
    let count = corners.clamp(3, 32) as usize;
    let at = |index: usize| {
        let angle =
            -std::f32::consts::FRAC_PI_2 + index as f32 / count as f32 * std::f32::consts::TAU;
        [0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin()]
    };
    let sharp: Vec<[f32; 2]> = (0..count).map(at).collect();
    if roundness < 0.5 {
        return sharp;
    }
    // Each corner cut back along both its sides and joined by a curve
    // through the corner's pull (a quadratic), in picture pixels so the
    // roundness is the same on a stretched box.
    let scale = [size[0].max(1e-3), size[1].max(1e-3)];
    let pixels: Vec<[f32; 2]> = sharp
        .iter()
        .map(|p| [p[0] * scale[0], p[1] * scale[1]])
        .collect();
    let mut out = Vec::new();
    for index in 0..count {
        let corner = pixels[index];
        let before = pixels[(index + count - 1) % count];
        let after = pixels[(index + 1) % count];
        let cut = |toward: [f32; 2]| {
            let (dx, dy) = (toward[0] - corner[0], toward[1] - corner[1]);
            let length = dx.hypot(dy).max(1e-3);
            let by = roundness.min(length / 2.0) / length;
            [corner[0] + dx * by, corner[1] + dy * by]
        };
        let (a, b) = (cut(before), cut(after));
        for step in 0..=6 {
            let t = step as f32 / 6.0;
            let s = 1.0 - t;
            let point = [
                s * s * a[0] + 2.0 * s * t * corner[0] + t * t * b[0],
                s * s * a[1] + 2.0 * s * t * corner[1] + t * t * b[1],
            ];
            out.push([point[0] / scale[0], point[1] / scale[1]]);
        }
    }
    out
}

/// The projective map from the unit square onto four corners (Heckbert's
/// square to quadrilateral), so a shape in the square lies in a plane
/// seen in perspective.
struct SquareToQuad {
    m: [f32; 8],
}

impl SquareToQuad {
    fn new([p0, p1, p2, p3]: [[f32; 2]; 4]) -> Self {
        let sx = p0[0] - p1[0] + p2[0] - p3[0];
        let sy = p0[1] - p1[1] + p2[1] - p3[1];
        let (dx1, dx2) = (p1[0] - p2[0], p3[0] - p2[0]);
        let (dy1, dy2) = (p1[1] - p2[1], p3[1] - p2[1]);
        let det = dx1 * dy2 - dx2 * dy1;
        let (g, h) = if det.abs() < 1e-9 || (sx.abs() < 1e-6 && sy.abs() < 1e-6) {
            (0.0, 0.0)
        } else {
            ((sx * dy2 - dx2 * sy) / det, (dx1 * sy - sx * dy1) / det)
        };
        Self {
            m: [
                p1[0] - p0[0] + g * p1[0],
                p3[0] - p0[0] + h * p3[0],
                p0[0],
                p1[1] - p0[1] + g * p1[1],
                p3[1] - p0[1] + h * p3[1],
                p0[1],
                g,
                h,
            ],
        }
    }

    fn apply(&self, [u, v]: [f32; 2]) -> [f32; 2] {
        let [a, b, c, d, e, f, g, h] = self.m;
        let w = (g * u + h * v + 1.0).max(1e-6);
        [(a * u + b * v + c) / w, (d * u + e * v + f) / w]
    }
}

/// The Curve from `from` to `to` bent so it passes through `through`
/// (a quadratic whose middle is there).
pub(crate) fn bent(from: [f32; 2], to: [f32; 2], through: [f32; 2]) -> Vec<[f32; 2]> {
    let pull = [
        2.0 * through[0] - (from[0] + to[0]) / 2.0,
        2.0 * through[1] - (from[1] + to[1]) / 2.0,
    ];
    (0..=48)
        .map(|step| {
            let t = step as f32 / 48.0;
            let s = 1.0 - t;
            [
                s * s * from[0] + 2.0 * s * t * pull[0] + t * t * to[0],
                s * s * from[1] + 2.0 * s * t * pull[1] + t * t * to[1],
            ]
        })
        .collect()
}

/// The Bezier Curve through `anchors`, each leaving along its handle in
/// `handles` and arriving along the mirror of it; `closed` joins the last
/// anchor back to the first.
pub(crate) fn bezier(anchors: &[[f32; 2]], handles: &[[f32; 2]], closed: bool) -> Vec<[f32; 2]> {
    let handle = |index: usize| handles.get(index).copied().unwrap_or(anchors[index]);
    let mirror = |index: usize| {
        let (p, h) = (anchors[index], handle(index));
        [2.0 * p[0] - h[0], 2.0 * p[1] - h[1]]
    };
    let count = anchors.len();
    let segments = if closed {
        count
    } else {
        count.saturating_sub(1)
    };
    let mut out = Vec::new();
    for index in 0..segments {
        let next = (index + 1) % count;
        let (p0, p1, p2, p3) = (anchors[index], handle(index), mirror(next), anchors[next]);
        for step in 0..=24 {
            if step == 0 && index > 0 {
                continue;
            }
            let t = step as f32 / 24.0;
            let s = 1.0 - t;
            let (a, b, c, d) = (s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t);
            out.push([
                a * p0[0] + b * p1[0] + c * p2[0] + d * p3[0],
                a * p0[1] + b * p1[1] + c * p2[1] + d * p3[1],
            ]);
        }
    }
    if out.is_empty() {
        out = anchors.to_vec();
    }
    out
}

/// The line the corners clicked with the Figure tool's `mode` make:
/// straight between them (Polyline), smoothed through them (Continuous
/// Curve) or along their handles (Bezier Curve).
pub(crate) fn corner_line(
    mode: u8,
    points: &[[f32; 2]],
    handles: &[[f32; 2]],
    closed: bool,
) -> Vec<[f32; 2]> {
    match mode {
        CONTINUOUS_CURVE if closed => crate::manga::smooth_closed(points, 12),
        CONTINUOUS_CURVE => crate::manga::smooth_open(points, 12),
        BEZIER_CURVE => bezier(points, handles, closed),
        _ => points.to_vec(),
    }
}

/// The radius of the rings marking a Figure's corners, ends and
/// handles on screen, as the other tools' corners are marked.
const HANDLE: f32 = 3.5;

/// What the overlay needs to draw a Figure under way.
pub(crate) struct Preview<'a> {
    pub painter: &'a Painter,
    pub screen: &'a dyn Fn([f32; 2]) -> Pos2,
    pub stroke: Stroke,
    /// The pointer, in picture pixels.
    pub pointer: Option<[f32; 2]>,
    pub shift: bool,
    /// The current sub tool's Step of angle, in degrees (0 for none).
    pub angle_step: f32,
    pub guides: &'a Guides,
}

/// The width of a Figure's preview line on screen, as thin as the other
/// tools' previews.
const LINE: f32 = 1.0;

impl Preview<'_> {
    /// The preview line in `color`.
    pub(crate) fn line_of(color: egui::Color32) -> Stroke {
        Stroke::new(LINE, color)
    }

    fn line(&self, points: &[[f32; 2]], closed: bool) {
        let path: Vec<Pos2> = points.iter().map(|p| (self.screen)(*p)).collect();
        self.painter.add(if closed {
            Shape::closed_line(path, self.stroke)
        } else {
            Shape::line(path, self.stroke)
        });
    }

    /// A Figure drag from `from` to `to` (not the effect lines).
    pub(crate) fn drag(&self, sub: &SubTool, from: Pos2, to: Pos2) {
        let (a, b) = ([from.x, from.y], [to.x, to.y]);
        match sub.mode {
            SATURATED_LINES => {
                self.line(&ellipse_points(from, to), true);
            }
            SPEED_LINES => self.line(&[a, b], false),
            BEZIER_CURVE => {
                // A handle being pulled out of its anchor.
                let mirror = [2.0 * a[0] - b[0], 2.0 * a[1] - b[1]];
                self.line(&[mirror, b], false);
                for end in [mirror, b] {
                    self.painter
                        .circle_stroke((self.screen)(end), HANDLE, self.stroke);
                }
            }
            _ => {
                let outline = outline(sub, a, b, self.shift, self.guides);
                self.line(&outline.points, outline.closed);
            }
        }
    }

    /// The Curve waiting to be bent, bent to the pointer.
    pub(crate) fn bend(&self, bend: &Bend) {
        let through = self.pointer.unwrap_or([
            (bend.from[0] + bend.to[0]) / 2.0,
            (bend.from[1] + bend.to[1]) / 2.0,
        ]);
        self.line(&bent(bend.from, bend.to, through), false);
        for end in [bend.from, bend.to] {
            self.painter
                .circle_stroke((self.screen)(end), HANDLE, self.stroke);
        }
    }

    /// Corners clicked out with the Figure tool's `mode`, and the line on
    /// to the pointer.
    pub(crate) fn corners(&self, mode: u8, points: &[[f32; 2]], handles: &[[f32; 2]]) {
        let mut line = points.to_vec();
        let mut ends = handles.to_vec();
        if let (Some(pointer), Some(last)) = (self.pointer, points.last()) {
            let pointer = if mode == POLYLINE {
                self.guides.line_end(*last, pointer).unwrap_or_else(|| {
                    line_constrained(*last, pointer, self.angle_step, self.shift)
                })
            } else {
                pointer
            };
            line.push(pointer);
            ends.resize(points.len(), [f32::NAN; 2]);
            ends.push(pointer);
        }
        let handles: Vec<[f32; 2]> = ends
            .iter()
            .zip(&line)
            .map(|(h, p)| if h[0].is_nan() { *p } else { *h })
            .collect();
        self.line(&corner_line(mode, &line, &handles, false), false);
        for corner in points {
            self.painter
                .circle_stroke((self.screen)(*corner), HANDLE, self.stroke);
        }
    }
}

/// `to` kept to every `step_degrees` (Step of angle), or to every 45
/// degrees while Shift is held, from `from`.
pub(crate) fn line_constrained(
    from: [f32; 2],
    to: [f32; 2],
    step_degrees: f32,
    shift: bool,
) -> [f32; 2] {
    let mut line = SubTool::new(Tool::Figure, "", 0, crate::brush::Tip::default());
    line.angle_step = step_degrees;
    constrained(&line, from, to, shift)
}

impl SuePaint {
    /// The Figure tool's box shapes and straight lines dragged from
    /// `from` to `to`: lines and outlines with the brush, filled shapes
    /// as fills, each one undo step.
    pub(super) fn figure(
        &mut self,
        from: Pos2,
        to: Pos2,
        sub: &SubTool,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let shift = self.modifiers.shift;
        let guides = self
            .doc()
            .map(|doc| Guides::of(&cx.world.arena, doc))
            .unwrap_or_default();
        let shape = outline(sub, [from.x, from.y], [to.x, to.y], shift, &guides);
        if !filled(sub.mode) {
            let mut points = shape.points;
            if shape.closed
                && let Some(first) = points.first().copied()
            {
                points.push(first);
            }
            self.stroke_line(points, sub, cx);
            return;
        }
        let Ok(info) = self.paintable(&cx.world.arena) else {
            return;
        };
        let rgb = color::unit(self.drawing_color());
        let to = constrained(sub, [from.x, from.y], [to.x, to.y], shift);
        let (min, max) = (
            [from.x.min(to[0]), from.y.min(to[1])],
            [from.x.max(to[0]), from.y.max(to[1])],
        );
        let mut area = Selection::new(size[0], size[1]);
        let plain = guides.perspective_box([from.x, from.y], to).is_none();
        if plain && sub.mode == 3 && sub.roundness < 0.5 {
            let clamp = |value: f32, limit: u32| value.round().clamp(0.0, limit as f32) as u32;
            area.rect(
                [
                    clamp(min[0], size[0]),
                    clamp(min[1], size[1]),
                    clamp(max[0], size[0]),
                    clamp(max[1], size[1]),
                ],
                Op::Replace,
            );
        } else if plain && sub.mode == 4 {
            area.ellipse([min[0], min[1], max[0], max[1]], Op::Replace);
        } else {
            area.polygon(&shape.points, Op::Replace);
        }
        let selection = self.doc().and_then(|doc| doc.selection.clone());
        let mask: Vec<u8> = (0..size[0] * size[1])
            .map(|index| {
                let (x, y) = (index % size[0], index / size[0]);
                let inside = area.at(x, y)
                    * selection
                        .as_ref()
                        .map_or(1.0, |selection| selection.at(x, y));
                (inside * 255.0).round() as u8
            })
            .collect();
        let (offset, alpha_lock, opacity) = (info.style.offset, info.alpha_locked, sub.tip.opacity);
        if let Ok(Some(edit)) = doc::rewrite(
            &cx.world.arena,
            info.id,
            &sub.name,
            |pixels, width, height| {
                fill::paint_mask(
                    pixels,
                    [width, height],
                    offset,
                    &mask,
                    size,
                    rgb,
                    opacity,
                    alpha_lock,
                );
            },
        ) {
            Self::apply(cx, edit);
        }
    }

    /// The Figure tool's lines: `points` (picture pixels) drawn with the
    /// sub tool's brush, untapered, within the selection, as one step. On
    /// a vector layer the line stays a vector line, with the brush's
    /// shape, as a pen line does.
    pub(crate) fn stroke_line(
        &mut self,
        points: Vec<[f32; 2]>,
        sub: &SubTool,
        cx: &mut AppletContext<'_>,
    ) {
        let info = match self.drawable(&cx.world.arena) {
            Ok(info) => info,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let vector = crate::vector::is_vector(&cx.world.arena, info.id);
        let rgb = color::unit(self.drawing_color());
        let offset = info.style.offset;
        let shifted = self
            .doc()
            .and_then(|doc| doc.selection.as_ref())
            .map(|selection| {
                if offset == [0, 0] {
                    selection.clone()
                } else {
                    selection.shifted(offset)
                }
            });
        let plain = crate::brush::Tip {
            taper_start: 0.0,
            taper_end: 0.0,
            mix: 0.0,
            ..sub.tip
        };
        let mut stroke = BrushStroke::new(info.id, plain, rgb, info.alpha_locked);
        if let Some(raster) = doc::raster_mut(&mut cx.world.arena, info.id) {
            for point in points {
                stroke.to(
                    raster,
                    [point[0] - offset[0] as f32, point[1] - offset[1] as f32],
                    1.0,
                    shifted.as_ref(),
                );
            }
            stroke.flush(raster, shifted.as_ref());
        }
        let kept = vector.then(|| {
            crate::vector::Line::of(
                &stroke,
                stroke.samples().to_vec(),
                crate::brush::Finish {
                    correction: 0,
                    flick: None,
                },
            )
        });
        let Some(raster) = doc::raster(&cx.world.arena, info.id) else {
            return;
        };
        let edit = stroke.finish(raster, &sub.name);
        if edit.is_empty() {
            return;
        }
        match kept {
            Some(line) => {
                // The line stays on the vector layer, in the same undo
                // step as its pixels.
                let old = crate::vector::geometry(&cx.world.arena, info.id);
                let mut lines = crate::vector::lines(&old);
                lines.push(line);
                let edit = crate::vector::keep(
                    &mut cx.world.arena,
                    info.id,
                    old,
                    &lines,
                    Some(edit),
                    &sub.name,
                );
                cx.record(ArenaEdit(edit));
            }
            None => {
                cx.record(ArenaEdit(edit));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-3
    }

    fn sub(mode: u8) -> SubTool {
        SubTool::new(Tool::Figure, "", mode, crate::brush::Tip::default())
    }

    #[test]
    fn a_bent_curve_passes_through_the_click() {
        let line = bent([0.0, 0.0], [100.0, 0.0], [50.0, 40.0]);
        assert!(near(line[0], [0.0, 0.0]) && near(line[48], [100.0, 0.0]));
        assert!(near(line[24], [50.0, 40.0]), "{:?}", line[24]);
    }

    #[test]
    fn shift_keeps_lines_at_45_degrees_and_boxes_square() {
        // The length dragged stays, the angle snaps.
        let end = constrained(&sub(0), [0.0, 0.0], [100.0, 40.0], true);
        assert!(end[1].abs() < 1e-3 && end[0] > 100.0, "{end:?}");
        let end = constrained(&sub(0), [0.0, 0.0], [100.0, 80.0], true);
        assert!((end[0] - end[1]).abs() < 1e-3, "{end:?}");
        let corner = constrained(&sub(1), [0.0, 0.0], [100.0, -40.0], true);
        assert!(near(corner, [100.0, -100.0]), "{corner:?}");
        assert!(near(
            constrained(&sub(1), [0.0, 0.0], [100.0, -40.0], false),
            [100.0, -40.0]
        ));
    }

    #[test]
    fn a_step_of_angle_keeps_lines_to_its_steps_without_shift() {
        // FGV-38: Step of angle at 30 degrees; a drag at about 25 lands
        // on 30, with or without Shift, and keeps its length.
        let mut line = sub(0);
        line.angle_step = 30.0;
        let to = [
            100.0 * 25f32.to_radians().cos(),
            100.0 * 25f32.to_radians().sin(),
        ];
        for shift in [false, true] {
            let end = constrained(&line, [0.0, 0.0], to, shift);
            let angle = end[1].atan2(end[0]).to_degrees();
            assert!((angle - 30.0).abs() < 1e-3, "{angle}");
            assert!((end[0].hypot(end[1]) - 100.0).abs() < 1e-3);
        }
        // The polyline's next corner and the Ruler tool keep to it too.
        let end = line_constrained([0.0, 0.0], to, 30.0, false);
        assert!((end[1].atan2(end[0]).to_degrees() - 30.0).abs() < 1e-3);
        assert_eq!(snapped_angle(0.4, 0.0, false), None, "free without either");
        let shift = snapped_angle(0.4, 0.0, true).unwrap();
        assert!((shift - std::f32::consts::FRAC_PI_4).abs() < 1e-6);
    }

    #[test]
    fn polygons_have_their_corners_and_rounded_ones_more_points() {
        let five = polygon_points(5, 0.0, [100.0, 100.0]);
        assert_eq!(five.len(), 5);
        assert!(near(five[0], [0.5, 0.0]), "the first corner on top");
        assert_eq!(polygon_points(1, 0.0, [10.0, 10.0]).len(), 3);
        let round = polygon_points(5, 10.0, [100.0, 100.0]);
        assert_eq!(round.len(), 5 * 7);
        assert!(round.iter().all(|p| p[1] > 1e-3), "the top corner cut");
        let square = rounded_box(10.0, [100.0, 50.0]);
        assert!(square.iter().all(|p| !near(*p, [0.0, 0.0])));
        assert_eq!(rounded_box(0.0, [100.0, 50.0]).len(), 4);
    }

    #[test]
    fn the_square_maps_onto_its_four_corners() {
        let quad = [[0.0, 0.0], [100.0, 20.0], [100.0, 80.0], [0.0, 100.0]];
        let map = SquareToQuad::new(quad);
        for (unit, corner) in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
            .into_iter()
            .zip(quad)
        {
            assert!(near(map.apply(unit), corner), "{unit:?}");
        }
    }

    #[test]
    fn bezier_handles_bend_the_line_and_closed_ones_return() {
        let anchors = [[0.0, 0.0], [100.0, 0.0]];
        let straight = bezier(&anchors, &anchors, false);
        assert!(straight.iter().all(|p| p[1].abs() < 1e-3));
        let bent = bezier(&anchors, &[[0.0, -60.0], [100.0, 60.0]], false);
        assert!(bent.iter().any(|p| p[1] < -10.0), "pulled up by the first");
        let closed = bezier(&[[0.0, 0.0], [100.0, 0.0], [50.0, 80.0]], &[], true);
        assert!(near(*closed.last().unwrap(), [0.0, 0.0]));
    }

    #[test]
    fn a_rectangle_lies_in_the_perspective_plane() {
        let guides = Guides {
            rulers: Rulers {
                perspective: Some(crate::rulers::Perspective {
                    points: vec![[1000.0, 500.0]],
                }),
                ..Rulers::default()
            },
            ruler: true,
            special: true,
            reach: 10.0,
        };
        let shape = outline(&sub(1), [100.0, 100.0], [300.0, 300.0], false, &guides);
        let xs: Vec<f32> = shape.points.iter().map(|p| p[0]).collect();
        // Upright sides: two corners at x 100 and two at x 300; the top
        // and bottom edges head for the vanishing point, so they are
        // closer at x 300 than at x 100.
        assert!(near([xs[0], 0.0], [100.0, 0.0]) && near([xs[2], 0.0], [300.0, 0.0]));
        let left = (shape.points[3][1] - shape.points[0][1]).abs();
        let right = (shape.points[2][1] - shape.points[1][1]).abs();
        assert!(right < left, "{:?}", shape.points);
        let off = Guides::default();
        let plain = outline(&sub(1), [100.0, 100.0], [300.0, 300.0], false, &off);
        assert_eq!(
            plain.points,
            vec![
                [100.0, 100.0],
                [300.0, 100.0],
                [300.0, 300.0],
                [100.0, 300.0]
            ]
        );
    }
}
