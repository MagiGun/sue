//! The layout solver: where the activity rails, the spring-driven
//! drawers, the infinite canvas, the bottom timeline and the status bar go,
//! for every form factor. Reveal values come from springs, so every drawer
//! glides instead of popping.

use egui::{Rect, pos2, vec2};
use sue_platform::{FormFactor, SafeAreaInsets};

/// How drawers share the screen with the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topology {
    /// Desktop: drawers and the timeline push the canvas aside.
    Tiled,
    /// Tablet and phone: drawers and the timeline slide over the canvas.
    Overlay,
}

impl Topology {
    /// The topology for a form factor.
    #[must_use]
    pub const fn for_form_factor(form_factor: FormFactor) -> Self {
        match form_factor {
            FormFactor::Desktop => Self::Tiled,
            FormFactor::Tablet | FormFactor::Mobile => Self::Overlay,
        }
    }
}

/// Sizes from the rice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutMetrics {
    /// Width of each activity rail.
    pub rail_width: f32,
    /// Width of the open left drawer.
    pub left_drawer_width: f32,
    /// Width of the open right drawer.
    pub right_drawer_width: f32,
    /// Height of the open timeline.
    pub timeline_height: f32,
    /// Height of the status bar.
    pub status_bar_height: f32,
    /// Narrowest the canvas may get before tiled drawers shrink.
    pub min_canvas_width: f32,
    /// Widest an overlay drawer may be, as a share of the screen.
    pub drawer_max_fraction: f32,
    /// Tallest the overlay timeline sheet may be, as a share of the screen.
    pub sheet_max_fraction: f32,
}

/// One layout question.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutRequest {
    /// The whole window, in points.
    pub viewport: Rect,
    /// Notches and system bars to keep clear.
    pub safe_area: SafeAreaInsets,
    /// The device class.
    pub form_factor: FormFactor,
    /// How far the left drawer is open, 0 to 1 (a spring position).
    pub left_reveal: f32,
    /// How far the right drawer is open, 0 to 1.
    pub right_reveal: f32,
    /// How far the timeline is open, 0 to 1.
    pub timeline_reveal: f32,
    /// Sizes from the rice.
    pub metrics: LayoutMetrics,
}

/// Where everything goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutSolution {
    /// How drawers share the screen.
    pub topology: Topology,
    /// The left activity rail.
    pub left_rail: Rect,
    /// The right activity rail.
    pub right_rail: Rect,
    /// The status bar.
    pub status: Rect,
    /// The visible part of the left drawer (zero width when closed).
    pub left_drawer: Rect,
    /// The full-width left drawer, slid by its reveal; content is laid out
    /// here and clipped to [`Self::left_drawer`].
    pub left_drawer_full: Rect,
    /// The visible part of the right drawer.
    pub right_drawer: Rect,
    /// The full-width right drawer, slid by its reveal.
    pub right_drawer_full: Rect,
    /// The visible part of the timeline.
    pub timeline: Rect,
    /// The full-height timeline, slid by its reveal.
    pub timeline_full: Rect,
    /// The infinite canvas.
    pub canvas: Rect,
}

fn inset(rect: Rect, insets: SafeAreaInsets) -> Rect {
    Rect::from_min_max(
        pos2(rect.left() + insets.left, rect.top() + insets.top),
        pos2(rect.right() - insets.right, rect.bottom() - insets.bottom),
    )
}

/// Solves the layout for `request`.
#[must_use]
pub fn solve_layout(request: &LayoutRequest) -> LayoutSolution {
    let metrics = request.metrics;
    let area = inset(request.viewport, request.safe_area);
    let status_height = metrics.status_bar_height.min(area.height() / 4.0).max(0.0);
    let status = Rect::from_min_max(pos2(area.left(), area.bottom() - status_height), area.max);
    let body = Rect::from_min_max(area.min, pos2(area.right(), status.top()));
    let rail = metrics.rail_width.min(body.width() / 6.0).max(0.0);
    let left_rail = Rect::from_min_size(body.min, vec2(rail, body.height()));
    let right_rail = Rect::from_min_max(pos2(body.right() - rail, body.top()), body.max);
    let middle = Rect::from_min_max(
        pos2(left_rail.right(), body.top()),
        pos2(right_rail.left(), body.bottom()),
    );
    let topology = Topology::for_form_factor(request.form_factor);

    let mut left_width = metrics.left_drawer_width.max(0.0);
    let mut right_width = metrics.right_drawer_width.max(0.0);
    match topology {
        Topology::Overlay => {
            let widest = middle.width() * metrics.drawer_max_fraction.clamp(0.1, 1.0);
            left_width = left_width.min(widest);
            right_width = right_width.min(widest);
        }
        Topology::Tiled => {
            // Shrink both drawers evenly before the canvas gets too narrow.
            let room = (middle.width() - metrics.min_canvas_width).max(0.0);
            let wanted = left_width + right_width;
            if wanted > room && wanted > 0.0 {
                let scale = room / wanted;
                left_width *= scale;
                right_width *= scale;
            }
        }
    }
    let left_visible = left_width * request.left_reveal.clamp(0.0, 1.0);
    let right_visible = right_width * request.right_reveal.clamp(0.0, 1.0);
    let left_drawer_full = Rect::from_min_size(
        pos2(middle.left() - left_width + left_visible, middle.top()),
        vec2(left_width, middle.height()),
    );
    let left_drawer = Rect::from_min_max(
        middle.min,
        pos2(middle.left() + left_visible, middle.bottom()),
    );
    let right_drawer_full = Rect::from_min_size(
        pos2(middle.right() - right_visible, middle.top()),
        vec2(right_width, middle.height()),
    );
    let right_drawer = Rect::from_min_max(
        pos2(middle.right() - right_visible, middle.top()),
        middle.max,
    );

    let region = match topology {
        Topology::Tiled => Rect::from_min_max(
            pos2(left_drawer.right(), middle.top()),
            pos2(right_drawer.left(), middle.bottom()),
        ),
        Topology::Overlay => middle,
    };
    let tallest = match topology {
        Topology::Tiled => region.height() * 0.6,
        Topology::Overlay => region.height() * metrics.sheet_max_fraction.clamp(0.1, 1.0),
    };
    let timeline_height = metrics.timeline_height.min(tallest).max(0.0);
    let timeline_visible = timeline_height * request.timeline_reveal.clamp(0.0, 1.0);
    let timeline_full = Rect::from_min_size(
        pos2(region.left(), region.bottom() - timeline_visible),
        vec2(region.width(), timeline_height),
    );
    let timeline = Rect::from_min_max(
        pos2(region.left(), region.bottom() - timeline_visible),
        region.max,
    );
    let canvas = match topology {
        Topology::Tiled => Rect::from_min_max(region.min, pos2(region.right(), timeline.top())),
        Topology::Overlay => middle,
    };
    LayoutSolution {
        topology,
        left_rail,
        right_rail,
        status,
        left_drawer,
        left_drawer_full,
        right_drawer,
        right_drawer_full,
        timeline,
        timeline_full,
        canvas,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics() -> LayoutMetrics {
        LayoutMetrics {
            rail_width: 36.0,
            left_drawer_width: 260.0,
            right_drawer_width: 280.0,
            timeline_height: 240.0,
            status_bar_height: 22.0,
            min_canvas_width: 420.0,
            drawer_max_fraction: 0.85,
            sheet_max_fraction: 0.5,
        }
    }

    fn request(form_factor: FormFactor, reveals: [f32; 3], width: f32) -> LayoutRequest {
        LayoutRequest {
            viewport: Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 900.0)),
            safe_area: SafeAreaInsets::default(),
            form_factor,
            left_reveal: reveals[0],
            right_reveal: reveals[1],
            timeline_reveal: reveals[2],
            metrics: metrics(),
        }
    }

    #[test]
    fn closed_drawers_leave_the_canvas_between_the_rails() {
        let layout = solve_layout(&request(FormFactor::Desktop, [0.0; 3], 1440.0));
        assert_eq!(layout.left_rail.width(), 36.0);
        assert_eq!(layout.canvas.left(), 36.0);
        assert_eq!(layout.canvas.right(), 1404.0);
        assert_eq!(layout.canvas.bottom(), 878.0);
        assert_eq!(layout.left_drawer.width(), 0.0);
    }

    #[test]
    fn tiled_drawers_push_the_canvas_as_they_glide_open() {
        let half = solve_layout(&request(FormFactor::Desktop, [0.5, 0.0, 0.0], 1440.0));
        assert_eq!(half.left_drawer.width(), 130.0);
        assert_eq!(half.canvas.left(), 166.0);
        // The content is laid out full width and slid, not squeezed.
        assert_eq!(half.left_drawer_full.width(), 260.0);
        assert_eq!(half.left_drawer_full.right(), 166.0);
        let open = solve_layout(&request(FormFactor::Desktop, [1.0, 1.0, 1.0], 1440.0));
        assert_eq!(open.canvas.width(), 1440.0 - 72.0 - 260.0 - 280.0);
        assert_eq!(open.timeline.height(), 240.0);
        assert_eq!(open.canvas.bottom(), open.timeline.top());
    }

    #[test]
    fn narrow_windows_shrink_tiled_drawers_before_the_canvas() {
        let layout = solve_layout(&request(FormFactor::Desktop, [1.0, 1.0, 0.0], 900.0));
        assert!(layout.canvas.width() >= 420.0 - 0.01);
    }

    #[test]
    fn phones_slide_drawers_over_the_canvas() {
        let layout = solve_layout(&request(FormFactor::Mobile, [1.0, 0.0, 1.0], 390.0));
        assert_eq!(layout.topology, Topology::Overlay);
        assert_eq!(layout.canvas.left(), layout.left_rail.right());
        assert!(layout.left_drawer.width() <= (390.0 - 2.0 * 36.0) * 0.85 + 0.01);
        assert!(layout.timeline.height() <= layout.canvas.height() * 0.5 + 0.01);
    }
}
