//! The canvas's and layers' dialogs: Change Canvas Size, Change Image
//! Resolution, Canvas Properties, Color Profile Preview Settings, Print,
//! Convert Layer, New 3D Layer, Register Material and Create Perspective
//! Ruler.

use std::sync::{Arc, Mutex};

use egui::{Rect, Response, Sense, Ui};
use sue_core::{Arena, EntityId, ReversibleTransaction, TxnError};
use sue_model::BlendMode;
use sue_wm::ui::{Radius, Space, Tip, dropdown, focus_ring, form_grid, radio, slider, text_field};
use sue_wm::{AppletContext, Theme};

use super::fill::{Reads, check, number};
use super::{Outcome, bare, note, row_height, tip};
use crate::doc::{self, Edit};
use crate::studio::SuePaint;

/// Change Canvas Size: the new size and where the old picture sits in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CanvasDialog {
    pub size: [u32; 2],
    pub old: [u32; 2],
    /// 0 top left to 8 bottom right, row by row.
    pub anchor: u8,
    /// Where the old top left corner lands once the box's handles on the
    /// canvas were dragged; `None` while the reference point places it.
    pub placed: Option<[i64; 2]>,
    /// The picture it changes: the box shows on it alone, and OK does
    /// nothing once another picture is in front.
    pub picture: Option<EntityId>,
}

/// The most pixels a side of the canvas takes.
const MOST: i64 = 30_000;

/// How near the pointer must come to one of the canvas box's handles, in
/// screen points, to grab it.
const REACH: f32 = 10.0;

impl CanvasDialog {
    /// The dialog for `picture`, a canvas of `size`, kept to its middle.
    pub(crate) fn new(size: [u32; 2], picture: Option<EntityId>) -> Self {
        Self {
            size,
            old: size,
            anchor: 4,
            placed: None,
            picture,
        }
    }

    /// The new canvas in the old picture's pixels: left, top, right and
    /// bottom.
    pub(crate) fn rect(&self) -> [i64; 4] {
        let [x, y] = self.shift();
        [
            -x,
            -y,
            i64::from(self.size[0]) - x,
            i64::from(self.size[1]) - y,
        ]
    }

    /// The box's handle `handle` (0 to 3 the corners from the top left
    /// clockwise, 4 to 7 the edges from the top, 8 the whole box) moved by
    /// `by` picture pixels from this box: the new size and place.
    #[must_use]
    pub(crate) fn dragged(&self, handle: usize, by: [i64; 2]) -> Self {
        let [mut left, mut top, mut right, mut bottom] = self.rect();
        let (moves_left, moves_top, moves_right, moves_bottom) = match handle {
            0 => (true, true, false, false),
            1 => (false, true, true, false),
            2 => (false, false, true, true),
            3 => (true, false, false, true),
            4 => (false, true, false, false),
            5 => (false, false, true, false),
            6 => (false, false, false, true),
            7 => (true, false, false, false),
            _ => (true, true, true, true),
        };
        if moves_left {
            left += by[0];
        }
        if moves_right {
            right += by[0];
        }
        if moves_top {
            top += by[1];
        }
        if moves_bottom {
            bottom += by[1];
        }
        // Never narrower than a pixel or wider than the most: the edge
        // that moves stops.
        if right - left < 1 || right - left > MOST {
            if moves_left && !moves_right {
                left = right - (right - left).clamp(1, MOST);
            } else {
                right = left + (right - left).clamp(1, MOST);
            }
        }
        if bottom - top < 1 || bottom - top > MOST {
            if moves_top && !moves_bottom {
                top = bottom - (bottom - top).clamp(1, MOST);
            } else {
                bottom = top + (bottom - top).clamp(1, MOST);
            }
        }
        Self {
            size: [(right - left) as u32, (bottom - top) as u32],
            placed: Some([-left, -top]),
            ..*self
        }
    }

    /// Where the old top left corner lands.
    pub(crate) fn shift(&self) -> [i64; 2] {
        if let Some(placed) = self.placed {
            return placed;
        }
        let (column, row) = (i64::from(self.anchor % 3), i64::from(self.anchor / 3));
        [
            (i64::from(self.size[0]) - i64::from(self.old[0])) * column / 2,
            (i64::from(self.size[1]) - i64::from(self.old[1])) * row / 2,
        ]
    }
}

/// Change Image Resolution: the new size in pixels and the resolution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResolutionDialog {
    pub size: [u32; 2],
    pub old: [u32; 2],
    pub dpi: f64,
    pub keep: bool,
}

/// Canvas Properties: the canvas's name, resolution and paper.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CanvasProperties {
    pub picture: sue_core::EntityId,
    pub name: String,
    pub dpi: f64,
    pub paper: Option<sue_core::EntityId>,
    pub paper_color: [u8; 3],
    pub paper_shown: bool,
}

/// Create Perspective Ruler: how many vanishing points, and whether the
/// ruler gets a new layer of its own (on at first, as in Clip Studio
/// Paint).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PerspectiveDialog {
    /// 1 to 3.
    pub count: u32,
    pub new_layer: bool,
}

impl PerspectiveDialog {
    /// The dialog for a ruler of `count` vanishing points, on a new layer.
    pub(crate) fn new(count: u32) -> Self {
        Self {
            count: count.clamp(1, 3),
            new_layer: true,
        }
    }
}

/// Convert Layer: what the layer becomes, as Clip Studio Paint's dialog
/// asks it: its name, its type, its expression color, its blending mode,
/// and whether the original stays beside it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ConvertDialog {
    pub layer: EntityId,
    pub name: String,
    /// 0 raster, 1 vector.
    pub kind: u8,
    /// 0 color, 1 gray, 2 monochrome (an index of
    /// `crate::effects::EXPRESSIONS`).
    pub expression: u8,
    /// The blending mode, or `None` to keep the layer's own.
    pub blend: Option<BlendMode>,
    /// Keep the original layer and convert a copy above it.
    pub keep: bool,
}

impl ConvertDialog {
    /// Converting `layer` (named `name`) to `kind`, keeping the rest.
    pub(crate) fn new(layer: EntityId, kind: u8, name: String) -> Self {
        Self {
            layer,
            name,
            kind,
            expression: 0,
            blend: None,
            keep: false,
        }
    }
}

/// The nine reference points of a frame, row by row from the top left,
/// as a grid of cells to click: the chosen one filled. Returns whether
/// the choice changed.
pub(super) fn anchor_ui(ui: &mut Ui, salt: &str, anchor: &mut u8, theme: &Theme) -> bool {
    let names = [
        tr!("anchor-top-left"),
        tr!("anchor-top"),
        tr!("anchor-top-right"),
        tr!("anchor-left"),
        tr!("anchor-middle"),
        tr!("anchor-right"),
        tr!("anchor-bottom-left"),
        tr!("anchor-bottom"),
        tr!("anchor-bottom-right"),
    ];
    let cell = row_height(ui);
    let gap = theme.space(Space::S1);
    // Three cells a side and the two gaps between them.
    let side = cell * 3.0 + gap * 2.0;
    let (grid, _) = ui.allocate_exact_size(egui::Vec2::splat(side), Sense::hover());
    let corner = theme.radius(Radius::Sm);
    let mut changed = false;
    for (index, name) in names.iter().enumerate() {
        let (column, row) = ((index % 3) as f32, (index / 3) as f32);
        let rect = Rect::from_min_size(
            grid.min + egui::Vec2::new(column * (cell + gap), row * (cell + gap)),
            egui::Vec2::splat(cell),
        );
        let id = ui.scope_id().with((salt, index));
        let response = ui.interact(rect, id, Sense::click());
        let chosen = usize::from(*anchor) == index;
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::RadioButton, true, chosen, name)
        });
        let fill = if response.hovered() {
            theme.fill_hover()
        } else {
            theme.surface(2)
        };
        ui.painter().rect_filled(rect, corner, fill);
        ui.painter()
            .rect_stroke(rect, corner, theme.hairline(), egui::StrokeKind::Inside);
        if chosen {
            // The point the picture keeps to: a solid square in the
            // text color.
            ui.painter().rect_filled(
                rect.shrink(cell / 3.0),
                theme.radius(Radius::Xs),
                theme.text,
            );
        }
        focus_ring(ui, &response, corner, theme);
        let response = tooltip_named(response, name, theme);
        if response.clicked() && !chosen {
            *anchor = index as u8;
            changed = true;
        }
    }
    changed
}

fn tooltip_named(response: Response, name: &str, theme: &Theme) -> Response {
    sue_wm::ui::tooltip(response, &Tip::new(name), theme)
}

/// A size in pixels in a form row. Returns whether it changed.
fn pixels(ui: &mut Ui, salt: &str, value: &mut u32, name: &str, theme: &Theme) -> bool {
    let px = tr!("px");
    let mut number_now = f64::from(*value);
    let reads = Reads {
        range: 1.0..=f64::from(doc::MAX_SIDE),
        decimals: 0,
        suffix: &px,
    };
    if number(ui, salt, &mut number_now, &reads, name, theme) {
        *value = number_now.round() as u32;
        return true;
    }
    false
}

/// A resolution in dots an inch in a form row.
fn resolution_field(ui: &mut Ui, salt: &str, dpi: &mut f64, theme: &Theme) -> bool {
    let name = bare(tr!("resolution"));
    let unit = tr!("dpi");
    let reads = Reads {
        range: 1.0..=2400.0,
        decimals: 0,
        suffix: &unit,
    };
    number(ui, salt, dpi, &reads, &name, theme)
}

/// Radio buttons, one per option, in a row; `chosen` follows a click.
fn radios<T: Copy + PartialEq>(
    ui: &mut Ui,
    salt: &str,
    chosen: &mut T,
    options: &[(T, String)],
    theme: &Theme,
) {
    for (index, (value, name)) in options.iter().enumerate() {
        let response = radio((salt, index), name, *chosen == *value).show(ui, theme);
        if tip(response, name, "", theme).clicked() {
            *chosen = *value;
        }
    }
}

/// Change Canvas Size: width, height and the reference point. The old
/// picture keeps to the reference point; the rest is new canvas.
pub(super) fn canvas_ui(ui: &mut Ui, canvas: &mut CanvasDialog, theme: &Theme) -> Outcome {
    form_grid(ui, "sue-paint-canvas-size", theme, |form| {
        let name = bare(tr!("width"));
        form.row(&name, |ui| {
            pixels(
                ui,
                "sue-paint-canvas-width",
                &mut canvas.size[0],
                &name,
                theme,
            );
        });
        let name = bare(tr!("height"));
        form.row(&name, |ui| {
            pixels(
                ui,
                "sue-paint-canvas-height",
                &mut canvas.size[1],
                &name,
                theme,
            );
        });
        form.row(&bare(tr!("reference-point")), |ui| {
            // Placed by the box's handles: no reference point holds it,
            // so none is lit. One picked places the picture by it again.
            let mut shown = if canvas.placed.is_some() {
                u8::MAX
            } else {
                canvas.anchor
            };
            if anchor_ui(ui, "sue-paint-canvas-anchor", &mut shown, theme) {
                canvas.anchor = shown;
                canvas.placed = None;
            }
        });
    });
    note(
        ui,
        &tr!("now-by-px", value = canvas.old[0], value2 = canvas.old[1]),
        theme,
    );
    note(ui, &tr!("canvas-size-drag-handles"), theme);
    Outcome::Open
}

impl SuePaint {
    /// The box of Change Canvas Size, while it is open, on the canvas in
    /// `viewport`: the new canvas's edge with a handle at each corner and
    /// in the middle of each side, and what falls outside it dimmed.
    pub(crate) fn canvas_size_marks(&self, ui: &Ui, viewport: Rect, size: [u32; 2], theme: &Theme) {
        let Some(super::Dialog::Canvas(canvas)) = &self.dialog else {
            return;
        };
        if canvas.picture != self.picture() {
            return;
        }
        let Some(view) = self.doc().map(|doc| doc.view) else {
            return;
        };
        let painter = ui.painter().with_clip_rect(viewport);
        let points = canvas_size_points(canvas, |p| {
            view.to_screen(viewport, size, egui::pos2(p[0], p[1]))
        });
        // What the new canvas leaves out of the old picture, dimmed.
        let [left, top, right, bottom] = canvas.rect().map(|value| value as f32);
        let (w, h) = (size[0] as f32, size[1] as f32);
        let screen = |x: f32, y: f32| view.to_screen(viewport, size, egui::pos2(x, y));
        let shade = theme.scrim();
        for [x0, y0, x1, y1] in [
            [0.0, 0.0, w, top.max(0.0)],
            [0.0, bottom.min(h), w, h],
            [0.0, top.max(0.0), left.max(0.0), bottom.min(h)],
            [right.min(w), top.max(0.0), w, bottom.min(h)],
        ] {
            if x1 > x0 && y1 > y0 {
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        screen(x0, y0),
                        screen(x1, y0),
                        screen(x1, y1),
                        screen(x0, y1),
                    ],
                    shade,
                    egui::Stroke::NONE,
                ));
            }
        }
        crate::canvas::marks::guide(&painter, points[..4].to_vec(), true, theme);
        let held = match &self.gesture {
            Some(crate::studio::Gesture::CanvasSize { handle, .. }) => Some(*handle),
            _ => None,
        };
        for (index, at) in points.iter().enumerate() {
            crate::canvas::marks::handle(&painter, *at, held == Some(index), theme);
            crate::launchers::note_mark(ui.ctx(), &format!("canvas_size.{index}"), *at);
        }
    }

    /// A press on the canvas while Change Canvas Size is open, at `pos`
    /// on screen: grabs a handle of its box, or the box itself. `false`
    /// when the press is not the box's.
    pub(crate) fn canvas_size_press(
        &mut self,
        pos: egui::Pos2,
        viewport: Rect,
        size: [u32; 2],
    ) -> bool {
        let Some(super::Dialog::Canvas(canvas)) = &self.dialog else {
            return false;
        };
        if canvas.picture != self.picture() {
            return false;
        }
        let Some(view) = self.doc().map(|doc| doc.view) else {
            return false;
        };
        let points = canvas_size_points(canvas, |p| {
            view.to_screen(viewport, size, egui::pos2(p[0], p[1]))
        });
        let near = points
            .iter()
            .enumerate()
            .map(|(index, at)| (index, at.distance(pos)))
            .filter(|(_, distance)| *distance <= REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index);
        let inside = egui::Rect::from_points(&points[..4]).contains(pos);
        let Some(handle) = near.or(inside.then_some(8)) else {
            return false;
        };
        self.gesture = Some(crate::studio::Gesture::CanvasSize {
            handle,
            start: view.to_picture(viewport, size, pos),
            from: *canvas,
        });
        true
    }

    /// Change Canvas Size's box dragged to `pos` (on screen): its size
    /// and place follow, in whole pixels.
    pub(crate) fn canvas_size_drag(&mut self, pos: egui::Pos2, viewport: Rect, size: [u32; 2]) {
        let Some(crate::studio::Gesture::CanvasSize {
            handle,
            start,
            from,
        }) = &self.gesture
        else {
            return;
        };
        let Some(view) = self.doc().map(|doc| doc.view) else {
            return;
        };
        let p = view.to_picture(viewport, size, pos);
        let by = [
            (p.x - start.x).round() as i64,
            (p.y - start.y).round() as i64,
        ];
        let next = from.dragged(*handle, by);
        if let Some(super::Dialog::Canvas(canvas)) = self.dialog.as_mut() {
            *canvas = next;
        }
    }
}

/// The screen points of `canvas`'s box handles: its four corners from
/// the top left clockwise, then the middles of its sides from the top.
fn canvas_size_points(
    canvas: &CanvasDialog,
    screen: impl Fn([f32; 2]) -> egui::Pos2,
) -> [egui::Pos2; 8] {
    let [left, top, right, bottom] = canvas.rect().map(|value| value as f32);
    let (mx, my) = ((left + right) / 2.0, (top + bottom) / 2.0);
    [
        [left, top],
        [right, top],
        [right, bottom],
        [left, bottom],
        [mx, top],
        [right, my],
        [mx, bottom],
        [left, my],
    ]
    .map(screen)
}

/// Change Image Resolution: the size in pixels (keeping its shape) and the
/// resolution.
pub(super) fn resolution_ui(
    ui: &mut Ui,
    resolution: &mut ResolutionDialog,
    theme: &Theme,
) -> Outcome {
    let ratio = f64::from(resolution.old[1]) / f64::from(resolution.old[0].max(1));
    form_grid(ui, "sue-paint-resolution", theme, |form| {
        let name = bare(tr!("width"));
        form.row(&name, |ui| {
            if pixels(
                ui,
                "sue-paint-resolution-width",
                &mut resolution.size[0],
                &name,
                theme,
            ) && resolution.keep
            {
                resolution.size[1] =
                    ((f64::from(resolution.size[0]) * ratio).round() as u32).max(1);
            }
        });
        let name = bare(tr!("height"));
        form.row(&name, |ui| {
            if pixels(
                ui,
                "sue-paint-resolution-height",
                &mut resolution.size[1],
                &name,
                theme,
            ) && resolution.keep
            {
                resolution.size[0] =
                    ((f64::from(resolution.size[1]) / ratio.max(1e-6)).round() as u32).max(1);
            }
        });
        form.row(&bare(tr!("resolution")), |ui| {
            resolution_field(ui, "sue-paint-resolution-dpi", &mut resolution.dpi, theme);
        });
        form.wide(|ui| {
            let name = tr!("keep-the-shape");
            check(
                ui,
                "sue-paint-resolution-keep",
                &mut resolution.keep,
                &name,
                "",
                theme,
            );
        });
    });
    Outcome::Open
}

/// Print Settings, or Print when `printing`.
pub(super) fn print_ui(
    ui: &mut Ui,
    settings: &mut crate::print::PrintSettings,
    printing: bool,
    theme: &Theme,
) -> Outcome {
    let papers: Vec<String> = crate::print::PAPERS
        .iter()
        .map(|(name, ..)| crate::localize(name))
        .collect();
    form_grid(ui, "sue-paint-print", theme, |form| {
        let name = bare(tr!("paper"));
        form.row(&name, |ui| {
            let response = dropdown("sue-paint-paper", &mut settings.paper, &papers)
                .label(&name)
                .show(ui, theme);
            tip(response, &name, "", theme);
        });
        form.row(&bare(tr!("orientation")), |ui| {
            let options = [
                (0, tr!("auto")),
                (1, tr!("portrait")),
                (2, tr!("landscape")),
            ];
            radios(
                ui,
                "sue-paint-print-orientation",
                &mut settings.orientation,
                &options,
                theme,
            );
        });
        form.row(&bare(tr!("size")), |ui| {
            let options = [
                (0, tr!("fit-to-page")),
                (1, tr!("print-size")),
                (2, tr!("scale")),
            ];
            radios(
                ui,
                "sue-paint-print-size",
                &mut settings.size,
                &options,
                theme,
            );
        });
        if settings.size == 2 {
            let name = tr!("scale");
            form.row(&name, |ui| {
                let mut percent = f64::from(settings.percent);
                let response = slider("sue-paint-print-scale", &mut percent, 1.0..=400.0)
                    .name(&name)
                    .suffix(" %")
                    .show(ui, theme);
                if response.changed() {
                    settings.percent = percent as f32;
                }
                tip(response, &name, "", theme);
            });
        }
        let name = bare(tr!("margin"));
        form.row(&name, |ui| {
            let mut margin = f64::from(settings.margin);
            let mm = tr!("mm");
            let response = slider("sue-paint-print-margin", &mut margin, 0.0..=50.0)
                .name(&name)
                .suffix(&mm)
                .show(ui, theme);
            if response.changed() {
                settings.margin = margin as f32;
            }
            tip(response, &name, "", theme);
        });
        form.wide(|ui| {
            let name = tr!("every-page-of-the-story");
            check(
                ui,
                "sue-paint-print-all",
                &mut settings.all_pages,
                &name,
                "",
                theme,
            );
        });
    });
    if printing {
        note(ui, &tr!("pages-open-as-pdf-your-viewer"), theme);
    }
    Outcome::Open
}

/// Color Profile Preview Settings: which profile.
pub(super) fn proof_ui(ui: &mut Ui, profile: &mut usize, theme: &Theme) -> Outcome {
    note(ui, &tr!("how-picture-would-print-shown-while"), theme);
    for (index, (name, ..)) in crate::proof::PROFILES.iter().enumerate() {
        let name = crate::localize(name);
        let response = radio(("sue-paint-proof", index), &name, *profile == index).show(ui, theme);
        if tip(response, &name, "", theme).clicked() {
            *profile = index;
        }
    }
    Outcome::Open
}

/// Canvas Properties: the name, resolution and paper.
pub(super) fn canvas_properties_ui(
    ui: &mut Ui,
    props: &mut CanvasProperties,
    theme: &Theme,
) -> Outcome {
    form_grid(ui, "sue-paint-canvas-properties", theme, |form| {
        let name = bare(tr!("name"));
        form.row(&name, |ui| {
            let response = text_field("sue-paint-canvas-name", &mut props.name)
                .label(&name)
                .show(ui, theme);
            tip(response, &name, "", theme);
        });
        form.row(&bare(tr!("resolution")), |ui| {
            resolution_field(ui, "sue-paint-canvas-dpi", &mut props.dpi, theme);
        });
        if props.paper.is_some() {
            form.row(&tr!("paper-2"), |ui| {
                let name = tr!("paper-2");
                check(
                    ui,
                    "sue-paint-canvas-paper",
                    &mut props.paper_shown,
                    &name,
                    "",
                    theme,
                );
                let [r, g, b] = props.paper_color;
                let mut rgba = [r, g, b, 255];
                let salt = "sue-paint-canvas-paper-color";
                if sue_wm::color_button(ui, salt, &mut rgba, false, theme).changed {
                    props.paper_color = [rgba[0], rgba[1], rgba[2]];
                }
            });
        }
    });
    Outcome::Open
}

/// Convert Layer: its name, type, expression color and blending mode, and
/// whether the original layer stays.
pub(super) fn convert_ui(ui: &mut Ui, convert: &mut ConvertDialog, theme: &Theme) -> Outcome {
    form_grid(ui, "sue-paint-convert", theme, |form| {
        let name = bare(tr!("name"));
        form.row(&name, |ui| {
            let response = text_field("sue-paint-convert-name", &mut convert.name)
                .label(&name)
                .show(ui, theme);
            tip(response, &name, "", theme);
        });
        form.row(&bare(tr!("type")), |ui| {
            let options = [(0, tr!("raster-layer")), (1, tr!("vector-layer"))];
            radios(
                ui,
                "sue-paint-convert-type",
                &mut convert.kind,
                &options,
                theme,
            );
        });
        let name = tr!("expression-color");
        form.row(&name, |ui| {
            let colors = expressions();
            let mut chosen = usize::from(convert.expression);
            let response = dropdown("sue-paint-convert-expression", &mut chosen, &colors)
                .label(&name)
                .show(ui, theme);
            if tip(response, &name, "", theme).changed() {
                convert.expression = chosen as u8;
            }
        });
        let name = tr!("blending-mode");
        form.row(&name, |ui| {
            let mut modes = vec![tr!("convert-blend-as-it-is")];
            modes.extend(
                BlendMode::ALL
                    .iter()
                    .map(|mode| sue_lang::translate("model", mode.label())),
            );
            let mut chosen = convert
                .blend
                .and_then(|blend| BlendMode::ALL.iter().position(|mode| *mode == blend))
                .map_or(0, |index| index + 1);
            let response = dropdown("sue-paint-convert-blend", &mut chosen, &modes)
                .label(&name)
                .show(ui, theme);
            if tip(response, &name, "", theme).changed() {
                convert.blend = chosen.checked_sub(1).map(|index| BlendMode::ALL[index]);
            }
        });
        form.wide(|ui| {
            let name = tr!("keep-original-layer");
            check(
                ui,
                "sue-paint-convert-keep",
                &mut convert.keep,
                &name,
                "",
                theme,
            );
        });
    });
    note(ui, &tr!("vector-pixels-are-traced-lines-down"), theme);
    Outcome::Open
}

/// The expression colors as people read them.
fn expressions() -> Vec<String> {
    crate::effects::EXPRESSIONS
        .iter()
        .map(|name| crate::localize(name))
        .collect()
}

/// New 3D Layer: which shape.
pub(super) fn shape_3d_ui(ui: &mut Ui, kind: &mut usize, theme: &Theme) -> Outcome {
    let options: Vec<(usize, String)> = crate::three::SHAPES
        .iter()
        .enumerate()
        .map(|(index, name)| (index, crate::localize(name)))
        .collect();
    form_grid(ui, "sue-paint-shape-3d", theme, |form| {
        form.row(&bare(tr!("shape")), |ui| {
            ui.vertical(|ui| radios(ui, "sue-paint-shape-3d", kind, &options, theme));
        });
    });
    note(ui, &tr!("file-import-3d-data-brings-models"), theme);
    Outcome::Open
}

/// Register Material, Image: its name.
pub(super) fn material_ui(ui: &mut Ui, name: &mut String, theme: &Theme) -> Outcome {
    form_grid(ui, "sue-paint-material", theme, |form| {
        let label = bare(tr!("material-name"));
        form.row(&label, |ui| {
            let response = text_field("sue-paint-material-name", name)
                .label(&label)
                .show(ui, theme);
            tip(response, &label, "", theme);
        });
    });
    note(ui, &tr!("current-layer-goes-into-material-palette"), theme);
    Outcome::Open
}

/// Create Perspective Ruler: how many vanishing points, and whether it
/// gets a new layer.
pub(super) fn perspective_ui(
    ui: &mut Ui,
    perspective: &mut PerspectiveDialog,
    theme: &Theme,
) -> Outcome {
    form_grid(ui, "sue-paint-perspective", theme, |form| {
        form.row(&bare(tr!("type")), |ui| {
            let options = [
                (1, tr!("n-1-point-perspective")),
                (2, tr!("n-2-point-perspective")),
                (3, tr!("n-3-point-perspective")),
            ];
            ui.vertical(|ui| {
                radios(
                    ui,
                    "sue-paint-perspective-type",
                    &mut perspective.count,
                    &options,
                    theme,
                );
            });
        });
        form.wide(|ui| {
            let name = tr!("create-new-layer");
            check(
                ui,
                "sue-paint-perspective-layer",
                &mut perspective.new_layer,
                &name,
                "",
                theme,
            );
        });
    });
    note(ui, &tr!("strokes-run-toward-vanishing-points-while"), theme);
    Outcome::Open
}

/// A step made of a first edit and a second one worked out from the
/// picture as the first left it, undone and redone as one. The second is
/// worked out once, on the first apply, and kept, so a redo puts back the
/// very same entities.
struct Then<F> {
    label: String,
    first: Edit,
    then: Option<F>,
    second: Option<Edit>,
}

impl<F> ReversibleTransaction<Arena> for Then<F>
where
    F: FnOnce(&Arena) -> Result<Edit, String> + Send,
{
    fn label(&self) -> String {
        self.label.clone()
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        self.first.apply(arena)?;
        if self.second.is_none()
            && let Some(then) = self.then.take()
        {
            match then(arena) {
                Ok(edit) => self.second = Some(edit),
                Err(error) => {
                    self.first.revert(arena)?;
                    return Err(TxnError::new(error));
                }
            }
        }
        if let Some(second) = &mut self.second
            && let Err(error) = second.apply(arena)
        {
            self.first.revert(arena)?;
            return Err(error);
        }
        Ok(())
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        if let Some(second) = &mut self.second {
            second.revert(arena)?;
        }
        self.first.revert(arena)
    }
}

/// The edit that converts `layer` as `convert` asks, and the id the
/// layer has after it (a new one when its type changes).
fn convert_edit(
    arena: &Arena,
    picture: EntityId,
    layer: EntityId,
    convert: &ConvertDialog,
) -> Result<(Edit, EntityId), String> {
    let vector = crate::vector::is_vector(arena, layer);
    let (mut edit, id) = match (convert.kind, vector) {
        (1, false) => doc::vectorize(arena, picture, layer)?,
        (0, true) => doc::rasterize(arena, picture, layer)?,
        _ => (Edit::new(tr!("convert-layer")), layer),
    };
    let entity = arena.get_by_id(layer).map_err(|error| error.to_string())?;
    let name = convert.name.trim();
    if !name.is_empty() && (id != layer || name != entity.name()) {
        edit = edit.then(sue_core::txn::Rename::new(id, name));
    }
    let expression = entity
        .property(crate::effects::EXPRESSION)
        .and_then(sue_core::Value::as_i64)
        .unwrap_or(0);
    if id != layer || expression != i64::from(convert.expression) {
        edit = edit.then(sue_core::txn::SetProperty::new(
            id,
            crate::effects::EXPRESSION,
            i64::from(convert.expression),
        ));
    }
    if let Some(blend) = convert.blend {
        edit = edit.then(sue_core::txn::SetProperty::new(
            id,
            sue_model::picture::BLEND,
            blend.name(),
        ));
    }
    Ok((edit, id))
}

/// View, Grid Settings: the gap between the main lines, how many parts a
/// cell is divided into, and where the grid starts.
pub(super) fn grid_ui(ui: &mut Ui, grid: &mut crate::grid::Grid, theme: &Theme) -> Outcome {
    form_grid(ui, "sue-paint-grid", theme, |form| {
        let name = tr!("grid-gap");
        form.row(&name, |ui| {
            let px = tr!("px");
            let mut gap = f64::from(grid.gap);
            let reads = Reads {
                range: crate::grid::GAPS,
                decimals: 0,
                suffix: &px,
            };
            if number(ui, "sue-paint-grid-gap", &mut gap, &reads, &name, theme) {
                grid.gap = gap as f32;
            }
        });
        let name = tr!("grid-divisions");
        form.row(&name, |ui| {
            let mut divisions = f64::from(grid.divisions);
            let reads = Reads {
                range: 1.0..=f64::from(crate::grid::MOST_DIVISIONS),
                decimals: 0,
                suffix: "",
            };
            if number(
                ui,
                "sue-paint-grid-divisions",
                &mut divisions,
                &reads,
                &name,
                theme,
            ) {
                grid.divisions = divisions.round() as u32;
            }
        });
        form.row(&tr!("grid-origin"), |ui| {
            let options = [
                (0, tr!("grid-origin-top-left")),
                (1, tr!("grid-origin-center")),
            ];
            radios(
                ui,
                "sue-paint-grid-origin",
                &mut grid.origin,
                &options,
                theme,
            );
        });
    });
    note(ui, &tr!("grid-hint"), theme);
    Outcome::Open
}

impl SuePaint {
    /// Converts the layer as Convert Layer asks, as one undo step; with
    /// Keep original layer, a copy above it is converted and the layer
    /// stays as it was.
    pub(super) fn convert_ok(&mut self, cx: &mut AppletContext<'_>, convert: &ConvertDialog) {
        let Some(picture) = self.picture() else {
            return;
        };
        let arena = &cx.world.arena;
        let made = if convert.keep {
            doc::duplicate(arena, picture, convert.layer).map(|(first, copy)| {
                let landed = Arc::new(Mutex::new(copy));
                let found = Arc::clone(&landed);
                let settings = convert.clone();
                let step = Then {
                    label: tr!("convert-layer"),
                    first,
                    then: Some(move |arena: &Arena| {
                        let (edit, id) = convert_edit(arena, picture, copy, &settings)?;
                        if let Ok(mut landed) = found.lock() {
                            *landed = id;
                        }
                        Ok(edit)
                    }),
                    second: None,
                };
                (Some(step), None, landed)
            })
        } else {
            convert_edit(arena, picture, convert.layer, convert)
                .map(|(edit, id)| (None, Some(edit), Arc::new(Mutex::new(id))))
        };
        let (kept, plain, landed) = match made {
            Ok(made) => made,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let applied = match (kept, plain) {
            (Some(step), _) => Self::apply(cx, step),
            (None, Some(edit)) if !edit.is_empty() => Self::apply(cx, edit),
            _ => false,
        };
        if applied
            && let Ok(id) = landed.lock()
            && let Some(doc) = self.doc_mut()
        {
            doc.layer = Some(*id);
        }
    }

    /// Grid Settings' OK: the picture's new grid, as one undo step (none
    /// when nothing changed).
    pub(super) fn grid_ok(cx: &mut AppletContext<'_>, picture: EntityId, grid: crate::grid::Grid) {
        if let Some(edit) = grid.apply(&cx.world.arena, picture) {
            Self::apply(cx, edit);
        }
    }

    /// Lays the perspective ruler, on a new layer of its own when asked
    /// (one undo step).
    pub(super) fn perspective_ok(
        &mut self,
        cx: &mut AppletContext<'_>,
        perspective: PerspectiveDialog,
    ) {
        if !perspective.new_layer {
            self.make_perspective(cx, perspective.count);
            return;
        }
        let Some(picture) = self.picture() else {
            return;
        };
        let arena = &cx.world.arena;
        let Some(size) = arena
            .get_by_id(picture)
            .ok()
            .and_then(sue_model::picture::size)
        else {
            return;
        };
        let label = tr!("create-perspective-ruler");
        let (spawn, layer) = match doc::add_raster_layer(
            arena,
            picture,
            self.layer(),
            tr!("perspective-ruler-layer"),
            None,
        ) {
            Ok(made) => made,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        let ruler = crate::rulers::Perspective::new(perspective.count, size);
        let edit = Edit::new(label.clone())
            .then(spawn)
            .then(crate::rulers::edit(
                picture,
                crate::rulers::PERSPECTIVE,
                Some(ruler.to_text()),
                &label,
            ));
        if Self::apply(cx, edit) {
            self.pick_ruler(1);
            if let Some(doc) = self.doc_mut() {
                doc.layer = Some(layer);
                doc.ruler_on = true;
                doc.show_rulers = true;
            }
        }
    }

    /// Keeps the print settings, and prints when `printing`.
    pub(super) fn print_ok(
        &mut self,
        cx: &mut AppletContext<'_>,
        settings: crate::print::PrintSettings,
        printing: bool,
    ) {
        self.print = settings;
        if printing {
            self.print_now(cx);
        }
    }

    /// Keeps the canvas's name, resolution and paper as one undo step.
    pub(super) fn canvas_properties_ok(cx: &mut AppletContext<'_>, props: &CanvasProperties) {
        let mut edit = crate::doc::Edit::new(tr!("canvas-properties")).then(
            sue_core::txn::SetProperty::new(props.picture, sue_model::picture::DPI, props.dpi),
        );
        let name = props.name.trim();
        if !name.is_empty() {
            edit = edit.then(sue_core::txn::Rename::new(props.picture, name));
        }
        if let Some(paper) = props.paper {
            let [r, g, b] = props.paper_color;
            edit = edit
                .then(sue_core::txn::SetProperty::new(
                    paper,
                    sue_model::picture::COLOR,
                    sue_core::Color::srgb8(r, g, b),
                ))
                .then(sue_core::txn::SetProperty::new(
                    paper,
                    sue_model::picture::VISIBLE,
                    props.paper_shown,
                ));
        }
        Self::apply(cx, edit);
    }

    /// Adds a 3D layer with shape `kind`.
    pub(super) fn shape_3d_ok(&mut self, cx: &mut AppletContext<'_>, kind: usize) {
        let name = crate::three::SHAPES[kind.min(crate::three::SHAPES.len() - 1)];
        let model = sue_core::Entity::empty(crate::localize(name), sue_model::class::MODEL)
            .with_ext(crate::three::shape(kind));
        self.add_3d_layer(cx, name, model);
    }

    /// Changes the canvas size.
    pub(super) fn canvas_ok(&mut self, cx: &mut AppletContext<'_>, canvas: &CanvasDialog) {
        // The canvas stays in reach while the dialog is open: another
        // picture come to the front, or this one's size changed since
        // (undone, say), is left as it is.
        let same = canvas.picture.is_some()
            && self.picture() == canvas.picture
            && self
                .picture()
                .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
                .and_then(sue_model::picture::size)
                == Some(canvas.old);
        if !same {
            return;
        }
        if let Some(picture) = self.picture() {
            match doc::resize_canvas(&cx.world.arena, picture, canvas.size, canvas.shift()) {
                Ok(edit) => {
                    Self::apply(cx, edit);
                    if let Some(doc) = self.doc_mut() {
                        doc.set_selection(None);
                    }
                }
                Err(error) => cx.fail(error),
            }
        }
    }

    /// Changes the image resolution.
    pub(super) fn resolution_ok(
        &mut self,
        cx: &mut AppletContext<'_>,
        resolution: &ResolutionDialog,
    ) {
        if let Some(picture) = self.picture() {
            match doc::resample(&cx.world.arena, picture, resolution.size, resolution.dpi) {
                Ok(edit) => {
                    Self::apply(cx, edit);
                    if let Some(doc) = self.doc_mut() {
                        doc.set_selection(None);
                    }
                }
                Err(error) => cx.fail(error),
            }
        }
    }

    /// Shows the picture as `profile` would print it.
    pub(super) fn proof_ok(&mut self, profile: usize) {
        self.proof_profile = profile;
        self.proof_on = true;
    }
}
