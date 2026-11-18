//! The Effects page, laid out as After Effects is: the effects browser
//! at the left, the program monitor in the middle with the picked clip's
//! masks drawn on it to drag, Effect Controls at the right, and along the
//! bottom the timeline beside the graph editor.

use egui::{Pos2, Rect, Sense, Ui, pos2, vec2};
use sue_wm::ui::{Density, Space};
use sue_wm::{AppletContext, Theme};

use super::{SQUARE, browser, controls, graph, monitor, timeline};
use crate::fx::mask::{self, Mask, Shape};
use crate::model::{self, SourceKind};
use crate::render;
use crate::studio::SueVe;

/// The share of the page's height the monitor row takes.
const TOP_SHARE: f32 = 0.6;
/// The browser's share of the width.
const BROWSER_SHARE: f32 = 0.2;
/// Effect Controls' share of the width.
const CONTROLS_SHARE: f32 = 0.27;
/// The timeline's share of the bottom row.
const TIMELINE_SHARE: f32 = 0.58;

/// Draws the Effects page in `body`.
pub fn effects_page(
    studio: &mut SueVe,
    ui: &mut Ui,
    body: Rect,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let split_y = body.top() + body.height() * TOP_SHARE;
    let browser_right = body.left() + body.width() * BROWSER_SHARE;
    let controls_left = body.right() - body.width() * CONTROLS_SHARE;
    let browser = Rect::from_min_max(body.min, pos2(browser_right, split_y));
    let viewer = Rect::from_min_max(
        pos2(browser_right, body.top()),
        pos2(controls_left, split_y),
    );
    let controls = Rect::from_min_max(pos2(controls_left, body.top()), pos2(body.right(), split_y));
    let timeline_right = body.left() + body.width() * TIMELINE_SHARE;
    let timeline = Rect::from_min_max(
        pos2(body.left(), split_y),
        pos2(timeline_right, body.bottom()),
    );
    let graph = Rect::from_min_max(pos2(timeline_right, split_y), body.max);
    for (rect, salt) in [
        (browser, "sve-fx-page-browser"),
        (controls, "sve-fx-page-controls"),
        (timeline, "sve-fx-page-timeline"),
        (graph, "sve-fx-page-graph"),
    ] {
        ui.painter().rect_filled(rect, SQUARE, theme.panel);
        ui.painter()
            .rect_stroke(rect, SQUARE, theme.hairline(), egui::StrokeKind::Inside);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt(salt));
        child.set_clip_rect(rect);
        match salt {
            "sve-fx-page-browser" => browser::browser_ui(studio, &mut child, cx, theme),
            "sve-fx-page-controls" => controls::controls_ui(studio, &mut child, cx, theme),
            "sve-fx-page-timeline" => timeline::timeline_ui(studio, &mut child, cx, theme),
            _ => graph::graph_ui(studio, &mut child, cx, theme),
        }
    }
    monitor::program_monitor(studio, ui, viewer, cx, theme);
    mask_overlay(studio, ui, cx, theme);
}

/// Where a point of the picked clip's picture (0 to 1 each way) is on the
/// monitor: through the clip's placement at the playhead.
struct Placement {
    origin: Pos2,
    across: egui::Vec2,
    down: egui::Vec2,
}

impl Placement {
    fn to_screen(&self, [u, v]: [f64; 2]) -> Pos2 {
        self.origin + self.across * u as f32 + self.down * v as f32
    }

    fn to_picture(&self, at: Pos2) -> [f64; 2] {
        // Solve origin + across u + down v = at.
        let d = at - self.origin;
        let det = self.across.x * self.down.y - self.across.y * self.down.x;
        if det.abs() < 1e-6 {
            return [0.0, 0.0];
        }
        let u = (d.x * self.down.y - d.y * self.down.x) / det;
        let v = (self.across.x * d.y - self.across.y * d.x) / det;
        [f64::from(u), f64::from(v)]
    }
}

/// The handles of a mask: where each is in the picture.
fn handles(mask: &Mask) -> Vec<[f64; 2]> {
    match &mask.shape {
        Shape::Rect { at, size } => vec![
            *at,
            [at[0] + size[0], at[1]],
            [at[0] + size[0], at[1] + size[1]],
            [at[0], at[1] + size[1]],
        ],
        Shape::Ellipse { center, radius } => vec![
            *center,
            [center[0] + radius[0], center[1]],
            [center[0], center[1] + radius[1]],
        ],
        Shape::Path(points) => points.iter().map(|point| point.at).collect(),
    }
}

/// Moves handle `index` of `mask` to `to`.
fn move_handle(mask: &mut Mask, index: usize, to: [f64; 2]) {
    match &mut mask.shape {
        Shape::Rect { at, size } => {
            let (mut x0, mut y0, mut x1, mut y1) = (at[0], at[1], at[0] + size[0], at[1] + size[1]);
            match index {
                0 => (x0, y0) = (to[0], to[1]),
                1 => (x1, y0) = (to[0], to[1]),
                2 => (x1, y1) = (to[0], to[1]),
                _ => (x0, y1) = (to[0], to[1]),
            }
            *at = [x0.min(x1), y0.min(y1)];
            *size = [(x1 - x0).abs(), (y1 - y0).abs()];
        }
        Shape::Ellipse { center, radius } => match index {
            0 => *center = to,
            1 => radius[0] = (to[0] - center[0]).abs(),
            _ => radius[1] = (to[1] - center[1]).abs(),
        },
        Shape::Path(points) => {
            if let Some(point) = points.get_mut(index) {
                point.at = to;
            }
        }
    }
}

/// The picked clip's masks over the program monitor, their handles
/// dragged to reshape the picked mask (one undo step a drag).
fn mask_overlay(studio: &mut SueVe, ui: &Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
    let Some(frame_rect) = studio.program_rect else {
        return;
    };
    let Some((track, clip)) = studio.selected_clip(cx) else {
        return;
    };
    let masks = mask::masks(&clip);
    if masks.is_empty() {
        return;
    }
    let Some(sequence) = studio.current(cx) else {
        return;
    };
    let size = model::frame_size(sequence);
    let at = studio.clock.position;
    if at < clip.start || at >= clip.end() {
        return;
    }
    let kind = cx
        .arena()
        .get_by_id(clip.source)
        .map_or(SourceKind::Unknown, SourceKind::of);
    let natural = match kind {
        SourceKind::Title
        | SourceKind::Solid
        | SourceKind::Shape
        | SourceKind::Adjustment
        | SourceKind::Sequence => size,
        _ => model::size_of(cx.arena(), clip.source).unwrap_or(size),
    };
    let layer = render::placed_at(track, &clip, kind, at);
    let quad = render::corners(&layer, natural, size);
    let scale = frame_rect.width() / size.0.max(1) as f32;
    let screen = |[x, y]: [f32; 2]| frame_rect.min + vec2(x * scale, y * scale);
    let placement = Placement {
        origin: screen(quad[0]),
        across: screen(quad[1]) - screen(quad[0]),
        down: screen(quad[3]) - screen(quad[0]),
    };
    let painter = ui.painter_at(frame_rect);
    let picked = studio.mask_picked.unwrap_or(0).min(masks.len() - 1);
    for (index, found) in masks.iter().enumerate() {
        let outline: Vec<Pos2> = found
            .polygon(1.0, 1.0)
            .into_iter()
            .map(|point| placement.to_screen(point))
            .collect();
        let stroke = if index == picked {
            egui::Stroke::new(Theme::STRONG_STROKE, theme.focus())
        } else {
            theme.hairline_active()
        };
        painter.add(egui::Shape::closed_line(outline, stroke));
    }
    let side = Density::Compact.icon() * 0.6;
    let mask = &masks[picked];
    let spots: Vec<Pos2> = handles(mask)
        .into_iter()
        .map(|point| placement.to_screen(point))
        .collect();
    for spot in &spots {
        painter.rect_filled(
            Rect::from_center_size(*spot, vec2(side, side)),
            0.0,
            theme.focus(),
        );
    }
    let response = ui.interact(
        frame_rect,
        egui::Id::unique("sve-mask-overlay"),
        Sense::drag(),
    );
    let id = egui::Id::unique("sve-mask-handle");
    if response.drag_started()
        && let Some(at) = response.interact_pointer_pos()
    {
        let near = spots
            .iter()
            .enumerate()
            .map(|(index, spot)| (index, spot.distance(at)))
            .filter(|(_, d)| *d <= side * 1.5)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index);
        ui.data_mut(|data| data.insert_temp(id, near));
    }
    let held: Option<usize> = ui.data(|data| data.get_temp(id)).flatten();
    if response.dragged()
        && let (Some(handle), Some(at)) = (held, response.interact_pointer_pos())
    {
        let to = placement.to_picture(at);
        let clip_id = clip.id;
        studio.live_clip(cx, clip_id, |clip| {
            let mut masks = mask::masks(clip);
            if let Some(mask) = masks.get_mut(picked) {
                move_handle(mask, handle, to);
            }
            mask::set_masks(clip, &masks);
        });
    }
    if response.drag_stopped() && held.is_some() {
        ui.data_mut(|data| data.insert_temp::<Option<usize>>(id, None));
        studio.commit_live(cx, &tr!("controls-move-mask"));
    }
    let _ = theme.space(Space::S1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_handles_move_their_shapes() {
        let mut rect = Mask::rect();
        move_handle(&mut rect, 2, [0.9, 0.8]);
        assert!(matches!(rect.shape, Shape::Rect { at, size }
            if at == [0.25, 0.25] && (size[0] - 0.65).abs() < 1e-9 && (size[1] - 0.55).abs() < 1e-9));
        let mut ellipse = Mask::ellipse();
        move_handle(&mut ellipse, 1, [0.9, 0.5]);
        assert!(
            matches!(ellipse.shape, Shape::Ellipse { radius, .. } if (radius[0] - 0.4).abs() < 1e-9)
        );
        let placement = Placement {
            origin: pos2(10.0, 20.0),
            across: vec2(100.0, 0.0),
            down: vec2(0.0, 50.0),
        };
        let back = placement.to_picture(placement.to_screen([0.25, 0.5]));
        assert!((back[0] - 0.25).abs() < 1e-5 && (back[1] - 0.5).abs() < 1e-5);
    }
}
