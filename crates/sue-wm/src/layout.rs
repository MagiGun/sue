//! The layout solver: where the top bar (the Sue button and the open
//! studio's or tool's menus), the page (the Hub, a studio or a tool) and
//! the status bar go, for every form factor. Touch layouts get a taller bar
//! for fingers; a phone's bar fits 44 point buttons.

use egui::{Rect, pos2};
use sue_platform::{FormFactor, SafeAreaInsets};

/// Sizes from the rice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutMetrics {
    /// Height of the top bar on a desktop.
    pub bar_height: f32,
    /// Height of the status bar.
    pub status_bar_height: f32,
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
    /// Sizes from the rice.
    pub metrics: LayoutMetrics,
}

/// Where everything goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutSolution {
    /// The top bar: the Sue button at its left, then the menus.
    pub bar: Rect,
    /// The square the Sue button takes, at the bar's left.
    pub sue_button: Rect,
    /// The rest of the bar, for the open studio's or tool's menus.
    pub menus: Rect,
    /// The page: the Hub, a studio or a tool.
    pub page: Rect,
    /// The status bar.
    pub status: Rect,
}

/// The top bar's height for `form_factor`: the riced height on a desktop,
/// at least 36 points on a tablet and 52 on a phone (a 44 point button and
/// its margin).
#[must_use]
pub fn bar_height(form_factor: FormFactor, metrics: LayoutMetrics) -> f32 {
    match form_factor {
        FormFactor::Desktop => metrics.bar_height,
        FormFactor::Tablet => metrics.bar_height.max(36.0),
        FormFactor::Mobile => metrics.bar_height.max(52.0),
    }
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
    let area = inset(request.viewport, request.safe_area);
    let status_height = request
        .metrics
        .status_bar_height
        .min(area.height() / 4.0)
        .max(0.0);
    let bar_height = bar_height(request.form_factor, request.metrics)
        .min(area.height() / 4.0)
        .max(0.0);
    let bar = Rect::from_min_max(area.min, pos2(area.right(), area.top() + bar_height));
    let status = Rect::from_min_max(pos2(area.left(), area.bottom() - status_height), area.max);
    // The Sue button is as wide as the bar is tall, plus a little room on a
    // desktop, where the bar is short.
    let button_width = if request.form_factor == FormFactor::Desktop {
        bar_height + 12.0
    } else {
        bar_height
    }
    .min(bar.width());
    let sue_button = Rect::from_min_max(bar.min, pos2(bar.left() + button_width, bar.bottom()));
    let menus = Rect::from_min_max(pos2(sue_button.right(), bar.top()), bar.max);
    let page = Rect::from_min_max(
        pos2(area.left(), bar.bottom()),
        pos2(area.right(), status.top().max(bar.bottom())),
    );
    LayoutSolution {
        bar,
        sue_button,
        menus,
        page,
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    fn metrics() -> LayoutMetrics {
        LayoutMetrics {
            bar_height: 28.0,
            status_bar_height: 22.0,
        }
    }

    fn request(form_factor: FormFactor, size: egui::Vec2) -> LayoutRequest {
        LayoutRequest {
            viewport: Rect::from_min_size(egui::Pos2::ZERO, size),
            safe_area: SafeAreaInsets::default(),
            form_factor,
            metrics: metrics(),
        }
    }

    #[test]
    fn the_bar_page_and_status_bar_stack_without_gaps() {
        let solution = solve_layout(&request(FormFactor::Desktop, vec2(1440.0, 900.0)));
        assert_eq!(solution.bar.height(), 28.0);
        assert_eq!(solution.page.top(), solution.bar.bottom());
        assert_eq!(solution.page.bottom(), solution.status.top());
        assert_eq!(solution.status.bottom(), 900.0);
        assert_eq!(solution.page.width(), 1440.0, "no side panels");
        // The Sue button sits at the very top left, the menus right after.
        assert_eq!(solution.sue_button.min, egui::Pos2::ZERO);
        assert_eq!(solution.menus.left(), solution.sue_button.right());
        assert_eq!(solution.menus.right(), 1440.0);
    }

    #[test]
    fn touch_layouts_get_taller_bars_for_fingers() {
        let tablet = solve_layout(&request(FormFactor::Tablet, vec2(1024.0, 768.0)));
        let phone = solve_layout(&request(FormFactor::Mobile, vec2(390.0, 844.0)));
        assert_eq!(tablet.bar.height(), 36.0);
        assert_eq!(phone.bar.height(), 52.0);
        assert_eq!(phone.sue_button.width(), 52.0, "a square thumb target");
        assert_eq!(phone.page.width(), 390.0, "the page has the whole width");
    }

    #[test]
    fn safe_areas_keep_the_bars_clear_of_notches() {
        let mut asked = request(FormFactor::Mobile, vec2(390.0, 844.0));
        asked.safe_area = SafeAreaInsets {
            top: 47.0,
            bottom: 34.0,
            left: 0.0,
            right: 0.0,
        };
        let solution = solve_layout(&asked);
        assert_eq!(solution.bar.top(), 47.0);
        assert_eq!(solution.status.bottom(), 810.0);
    }

    #[test]
    fn a_tiny_window_still_lays_out_in_order() {
        let solution = solve_layout(&request(FormFactor::Desktop, vec2(60.0, 40.0)));
        assert!(solution.bar.height() <= 10.0);
        assert!(solution.page.height() >= 0.0);
        assert!(solution.sue_button.width() <= 60.0);
    }
}
