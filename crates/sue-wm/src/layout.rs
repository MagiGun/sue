//! The adaptive layout solver: desktop tiles, tablet overlays, phone drawers.

use sue_platform::{FormFactor, SafeAreaInsets, ScreenEdge};

/// An axis-aligned rectangle in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

impl Rect {
    /// A rectangle.
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Right edge.
    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    /// Bottom edge.
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// The rectangle minus `insets` on each side (never negative in size).
    #[must_use]
    pub fn inset(&self, insets: SafeAreaInsets) -> Self {
        Self::new(
            self.x + insets.left,
            self.y + insets.top,
            (self.width - insets.left - insets.right).max(0.0),
            (self.height - insets.top - insets.bottom).max(0.0),
        )
    }

    /// The rectangle shrunk by `amount` on every side.
    #[must_use]
    pub fn shrink(&self, amount: f32) -> Self {
        self.inset(SafeAreaInsets {
            top: amount,
            right: amount,
            bottom: amount,
            left: amount,
        })
    }

    /// The smallest rectangle containing both.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let (x, y) = (self.x.min(other.x), self.y.min(other.y));
        Self::new(
            x,
            y,
            self.right().max(other.right()) - x,
            self.bottom().max(other.bottom()) - y,
        )
    }

    /// `true` if the rectangles share a region of positive area.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    /// `true` if `other` lies inside this rectangle (with a small tolerance).
    #[must_use]
    pub fn contains_rect(&self, other: &Self) -> bool {
        const EPSILON: f32 = 1e-3;
        other.x >= self.x - EPSILON
            && other.y >= self.y - EPSILON
            && other.right() <= self.right() + EPSILON
            && other.bottom() <= self.bottom() + EPSILON
    }
}

/// The four regions of Sue's screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Zone {
    /// Daily utilities.
    LeftDock,
    /// The morphing workspace.
    CenterStage,
    /// Reference and control.
    RightDock,
    /// Timeline for time-based media.
    BottomTimeline,
}

/// Layout geometry family, chosen by form factor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Topology {
    /// Docks tile beside Center Stage.
    DesktopTiling,
    /// Docks overlay as drawers; the canvas stays maximal for the stylus.
    TabletStylus,
    /// Single pane; docks are edge drawers, the timeline a bottom sheet.
    MobilePocket,
}

impl Topology {
    /// The topology for `form_factor`.
    #[must_use]
    pub const fn for_form_factor(form_factor: FormFactor) -> Self {
        match form_factor {
            FormFactor::Desktop => Self::DesktopTiling,
            FormFactor::Tablet => Self::TabletStylus,
            FormFactor::Mobile => Self::MobilePocket,
        }
    }
}

/// How far a bottom sheet is pulled up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SheetDetent {
    /// A strip in the thumb zone.
    Peek,
    /// Half the screen.
    Half,
    /// Full screen.
    Full,
}

/// Where a zone is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Placement {
    /// Not shown.
    Hidden,
    /// A tile that takes space from Center Stage.
    Tiled(Rect),
    /// An overlay sliding in from an edge.
    Drawer {
        /// Edge it slides from.
        edge: ScreenEdge,
        /// Its current rectangle (partly off-screen while sliding).
        rect: Rect,
        /// How far it is revealed, `0.0..=1.0`.
        reveal: f32,
    },
    /// A sheet pulled up from the bottom.
    BottomSheet {
        /// Its rectangle.
        rect: Rect,
        /// Its detent.
        detent: SheetDetent,
    },
}

impl Placement {
    /// The drawn rectangle, if visible.
    #[must_use]
    pub fn rect(&self) -> Option<Rect> {
        match *self {
            Self::Hidden => None,
            Self::Tiled(rect) | Self::Drawer { rect, .. } | Self::BottomSheet { rect, .. } => {
                Some(rect)
            }
        }
    }
}

/// Riceable layout sizes, in logical pixels or fractions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutMetrics {
    /// Gap between tiles and around the content.
    pub gap: f32,
    /// Left dock width.
    pub left_dock_width: f32,
    /// Right dock width.
    pub right_dock_width: f32,
    /// Timeline height.
    pub timeline_height: f32,
    /// Narrowest Center Stage before docks turn into overlays.
    pub min_center_width: f32,
    /// Widest a phone drawer may be, as a share of the safe width.
    pub drawer_max_fraction: f32,
    /// Tallest the phone timeline sheet may peek, as a share of the safe height.
    pub sheet_max_fraction: f32,
}

/// Everything the solver needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutRequest {
    /// The window or screen.
    pub viewport: Rect,
    /// Notches and system bars.
    pub safe_area: SafeAreaInsets,
    /// Device form factor.
    pub form_factor: FormFactor,
    /// Left dock reveal, `0.0..=1.0` (driven by a spring).
    pub left_reveal: f32,
    /// Right dock reveal, `0.0..=1.0` (driven by a spring).
    pub right_reveal: f32,
    /// `true` while audio, video or animation is being edited.
    pub timeline_active: bool,
    /// Sizes from rice.
    pub metrics: LayoutMetrics,
}

/// Where every zone goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutSolution {
    /// Geometry family used.
    pub topology: Topology,
    /// Left dock.
    pub left: Placement,
    /// Center Stage (always visible).
    pub center: Rect,
    /// Right dock.
    pub right: Placement,
    /// Bottom timeline.
    pub bottom: Placement,
}

impl LayoutSolution {
    /// The placement of `zone`.
    #[must_use]
    pub fn placement(&self, zone: Zone) -> Placement {
        match zone {
            Zone::LeftDock => self.left,
            Zone::CenterStage => Placement::Tiled(self.center),
            Zone::RightDock => self.right,
            Zone::BottomTimeline => self.bottom,
        }
    }
}

/// Solves the layout for `request`. A pure function of its input.
#[must_use]
pub fn solve_layout(request: &LayoutRequest) -> LayoutSolution {
    let topology = Topology::for_form_factor(request.form_factor);
    let safe = request.viewport.inset(request.safe_area);
    let metrics = &request.metrics;
    let left_reveal = request.left_reveal.clamp(0.0, 1.0);
    let right_reveal = request.right_reveal.clamp(0.0, 1.0);
    let drawer = |edge: ScreenEdge, width: f32, reveal: f32| {
        if reveal <= 0.0 {
            return Placement::Hidden;
        }
        let width = width.min(safe.width * metrics.drawer_max_fraction);
        let x = match edge {
            ScreenEdge::Right => safe.right() - width * reveal,
            _ => safe.x - width * (1.0 - reveal),
        };
        Placement::Drawer {
            edge,
            rect: Rect::new(x, safe.y, width, safe.height),
            reveal,
        }
    };

    match topology {
        Topology::MobilePocket => {
            let bottom = if request.timeline_active {
                let height = metrics
                    .timeline_height
                    .min(safe.height * metrics.sheet_max_fraction);
                Placement::BottomSheet {
                    rect: Rect::new(safe.x, safe.bottom() - height, safe.width, height),
                    detent: SheetDetent::Peek,
                }
            } else {
                Placement::Hidden
            };
            LayoutSolution {
                topology,
                left: drawer(ScreenEdge::Left, metrics.left_dock_width, left_reveal),
                center: safe,
                right: drawer(ScreenEdge::Right, metrics.right_dock_width, right_reveal),
                bottom,
            }
        }
        Topology::TabletStylus => {
            let content = safe.shrink(metrics.gap);
            let (center, bottom) = split_timeline(content, metrics, request.timeline_active);
            LayoutSolution {
                topology,
                left: drawer(ScreenEdge::Left, metrics.left_dock_width, left_reveal),
                center,
                right: drawer(ScreenEdge::Right, metrics.right_dock_width, right_reveal),
                bottom,
            }
        }
        Topology::DesktopTiling => {
            let content = safe.shrink(metrics.gap);
            let gap = metrics.gap;
            let left_width = metrics.left_dock_width * left_reveal;
            let right_width = metrics.right_dock_width * right_reveal;
            let mut left_tiled = left_reveal > 0.0;
            let mut right_tiled = right_reveal > 0.0;
            let center_width = |left: bool, right: bool| {
                let taken = |tiled: bool, width: f32| if tiled { width + gap } else { 0.0 };
                content.width - taken(left, left_width) - taken(right, right_width)
            };
            // Too narrow: float the right dock first, then the left.
            if right_tiled && center_width(left_tiled, right_tiled) < metrics.min_center_width {
                right_tiled = false;
            }
            if left_tiled && center_width(left_tiled, right_tiled) < metrics.min_center_width {
                left_tiled = false;
            }
            let left = if left_tiled {
                Placement::Tiled(Rect::new(content.x, content.y, left_width, content.height))
            } else {
                drawer(ScreenEdge::Left, metrics.left_dock_width, left_reveal)
            };
            let right = if right_tiled {
                Placement::Tiled(Rect::new(
                    content.right() - right_width,
                    content.y,
                    right_width,
                    content.height,
                ))
            } else {
                drawer(ScreenEdge::Right, metrics.right_dock_width, right_reveal)
            };
            let column_x = if left_tiled {
                content.x + left_width + gap
            } else {
                content.x
            };
            let column = Rect::new(
                column_x,
                content.y,
                center_width(left_tiled, right_tiled).max(0.0),
                content.height,
            );
            let (center, bottom) = split_timeline(column, metrics, request.timeline_active);
            LayoutSolution {
                topology,
                left,
                center,
                right,
                bottom,
            }
        }
    }
}

/// Splits a column into Center Stage and a tiled timeline beneath it.
fn split_timeline(
    column: Rect,
    metrics: &LayoutMetrics,
    timeline_active: bool,
) -> (Rect, Placement) {
    if !timeline_active {
        return (column, Placement::Hidden);
    }
    let height = metrics
        .timeline_height
        .min((column.height - metrics.gap).max(0.0));
    let center = Rect::new(
        column.x,
        column.y,
        column.width,
        (column.height - height - metrics.gap).max(0.0),
    );
    let timeline = Rect::new(
        column.x,
        center.bottom() + metrics.gap,
        column.width,
        height,
    );
    (center, Placement::Tiled(timeline))
}

#[cfg(test)]
mod tests {
    use super::*;

    const METRICS: LayoutMetrics = LayoutMetrics {
        gap: 8.0,
        left_dock_width: 280.0,
        right_dock_width: 340.0,
        timeline_height: 220.0,
        min_center_width: 480.0,
        drawer_max_fraction: 0.85,
        sheet_max_fraction: 0.4,
    };

    fn request(width: f32, height: f32, form_factor: FormFactor) -> LayoutRequest {
        LayoutRequest {
            viewport: Rect::new(0.0, 0.0, width, height),
            safe_area: SafeAreaInsets::ZERO,
            form_factor,
            left_reveal: 1.0,
            right_reveal: 1.0,
            timeline_active: false,
            metrics: METRICS,
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn desktop_tiles_fill_the_content_width_without_overlap() {
        let solution = solve_layout(&LayoutRequest {
            timeline_active: true,
            ..request(1920.0, 1080.0, FormFactor::Desktop)
        });
        assert_eq!(solution.topology, Topology::DesktopTiling);
        let (Placement::Tiled(left), Placement::Tiled(right), Placement::Tiled(timeline)) =
            (solution.left, solution.right, solution.bottom)
        else {
            panic!("expected tiled docks and timeline: {solution:?}");
        };
        let content_width = 1920.0 - 2.0 * METRICS.gap;
        assert!(close(
            left.width + METRICS.gap + solution.center.width + METRICS.gap + right.width,
            content_width
        ));
        let tiles = [left, solution.center, right, timeline];
        for (i, a) in tiles.iter().enumerate() {
            for b in &tiles[i + 1..] {
                assert!(!a.overlaps(b), "{a:?} overlaps {b:?}");
            }
        }
        assert!(close(timeline.height, METRICS.timeline_height));
        assert!(
            close(timeline.x, solution.center.x) && close(timeline.width, solution.center.width)
        );
    }

    #[test]
    fn narrow_desktops_float_the_right_dock_then_the_left() {
        let solution = solve_layout(&request(1000.0, 800.0, FormFactor::Desktop));
        assert!(matches!(solution.left, Placement::Tiled(_)));
        assert!(matches!(
            solution.right,
            Placement::Drawer {
                edge: ScreenEdge::Right,
                ..
            }
        ));
        assert!(solution.center.width >= METRICS.min_center_width);

        let tiny = solve_layout(&request(600.0, 800.0, FormFactor::Desktop));
        assert!(matches!(
            tiny.left,
            Placement::Drawer {
                edge: ScreenEdge::Left,
                ..
            }
        ));
        assert!(close(tiny.center.width, 600.0 - 2.0 * METRICS.gap));
    }

    #[test]
    fn closed_docks_are_hidden_and_half_open_docks_tile_partially() {
        let closed = solve_layout(&LayoutRequest {
            left_reveal: 0.0,
            right_reveal: 0.0,
            ..request(1920.0, 1080.0, FormFactor::Desktop)
        });
        assert_eq!(
            (closed.left, closed.right, closed.bottom),
            (Placement::Hidden, Placement::Hidden, Placement::Hidden)
        );
        assert!(close(closed.center.width, 1920.0 - 2.0 * METRICS.gap));

        let sliding = solve_layout(&LayoutRequest {
            left_reveal: 0.5,
            ..request(1920.0, 1080.0, FormFactor::Desktop)
        });
        assert!(matches!(sliding.left, Placement::Tiled(rect) if close(rect.width, 140.0)));
    }

    #[test]
    fn phones_use_the_safe_area_drawers_and_a_thumb_zone_sheet() {
        let safe_area = SafeAreaInsets {
            top: 47.0,
            right: 0.0,
            bottom: 34.0,
            left: 0.0,
        };
        let solution = solve_layout(&LayoutRequest {
            safe_area,
            timeline_active: true,
            ..request(390.0, 844.0, FormFactor::Mobile)
        });
        let safe = Rect::new(0.0, 47.0, 390.0, 844.0 - 47.0 - 34.0);
        assert_eq!(solution.topology, Topology::MobilePocket);
        assert_eq!(solution.center, safe);
        let Placement::Drawer { rect: left, .. } = solution.left else {
            panic!("left drawer")
        };
        assert!(close(left.width, 280.0) && safe.contains_rect(&left));
        let Placement::Drawer { rect: right, .. } = solution.right else {
            panic!("right drawer")
        };
        assert!(close(right.width, 390.0 * 0.85) && safe.contains_rect(&right));
        let Placement::BottomSheet {
            rect: sheet,
            detent,
        } = solution.bottom
        else {
            panic!("sheet")
        };
        assert_eq!(detent, SheetDetent::Peek);
        assert!(close(sheet.height, 220.0) && close(sheet.bottom(), safe.bottom()));
    }

    #[test]
    fn drawers_slide_in_from_their_edge() {
        let solution = solve_layout(&LayoutRequest {
            left_reveal: 0.25,
            right_reveal: 0.0,
            ..request(390.0, 844.0, FormFactor::Mobile)
        });
        let Placement::Drawer { rect, reveal, .. } = solution.left else {
            panic!("left drawer")
        };
        assert!(close(rect.x, -280.0 * 0.75) && close(reveal, 0.25));
        assert_eq!(solution.right, Placement::Hidden);
        assert_eq!(
            solution.placement(Zone::CenterStage),
            Placement::Tiled(solution.center)
        );
    }

    #[test]
    fn tablets_overlay_docks_and_keep_the_canvas_large() {
        let solution = solve_layout(&request(1194.0, 834.0, FormFactor::Tablet));
        assert_eq!(solution.topology, Topology::TabletStylus);
        assert!(matches!(solution.left, Placement::Drawer { .. }));
        assert!(close(solution.center.width, 1194.0 - 2.0 * METRICS.gap));
    }

    #[test]
    fn rect_helpers() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(10.0, 0.0, 5.0, 5.0);
        assert!(!a.overlaps(&b));
        assert_eq!(a.union(&b), Rect::new(0.0, 0.0, 15.0, 10.0));
        assert_eq!(Rect::new(0.0, 0.0, 4.0, 4.0).shrink(3.0).width, 0.0);
    }
}
