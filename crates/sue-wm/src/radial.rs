//! The right-click marking menu: wedges around the pointer whose radius
//! grows from zero on a spring. Pick a wedge with a click, or press, drag
//! toward it and release. A wedge can open another wheel (New File, then a
//! kind of file, then the exact kind): the next wheel grows around the
//! picked wedge, and a click in its middle goes back to the one before.

use std::f32::consts::{FRAC_PI_2, TAU};

use egui::{Align2, Id, Order, Pos2, Rect, Sense, Shape, Stroke, Vec2, pos2, vec2};

use crate::{SpringDamper, SpringMode, SpringParams, Theme};

/// What the menu did this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadialOutcome {
    /// Still open, nothing picked.
    Open,
    /// The wedge at this index was picked.
    Picked(usize),
    /// The middle of a nested wheel was clicked: back to the wheel before.
    Back,
    /// Dismissed.
    Dismissed,
}

/// An open marking menu.
#[derive(Clone, Debug, PartialEq)]
pub struct Radial {
    /// Screen point it opened at.
    pub center: Pos2,
    /// The canvas point under it, where new entities go.
    pub canvas_point: Pos2,
    grow: SpringDamper,
    closing: bool,
    hovered: Option<usize>,
    parents: Vec<Pos2>,
}

/// The wedge under `offset` from the center, for `count` wedges starting
/// at the top and going clockwise.
#[must_use]
pub fn wedge_at(offset: Vec2, count: usize, inner: f32) -> Option<usize> {
    if count == 0 || offset.length() < inner {
        return None;
    }
    let angle = (offset.y.atan2(offset.x) + FRAC_PI_2).rem_euclid(TAU);
    let slice = TAU / count as f32;
    Some((((angle + slice / 2.0) / slice) as usize) % count)
}

impl Radial {
    /// Opens at `center` (screen) over `canvas_point`.
    #[must_use]
    pub fn open(center: Pos2, canvas_point: Pos2, params: SpringParams, mode: SpringMode) -> Self {
        let mut grow = SpringDamper::new(params, mode, 0.0);
        grow.set_target(1.0);
        Self {
            center,
            canvas_point,
            grow,
            closing: false,
            hovered: None,
            parents: Vec::new(),
        }
    }

    /// Opens the next wheel around `center` (screen); the current one is
    /// remembered for [`Self::ascend`].
    pub fn descend(&mut self, center: Pos2) {
        self.parents.push(self.center);
        self.center = center;
        self.regrow();
    }

    /// Goes back to the wheel before. Returns `false` at the first wheel.
    pub fn ascend(&mut self) -> bool {
        match self.parents.pop() {
            Some(center) => {
                self.center = center;
                self.regrow();
                true
            }
            None => false,
        }
    }

    /// How many wheels deep the menu is (0 for the first).
    #[must_use]
    pub fn depth(&self) -> usize {
        self.parents.len()
    }

    fn regrow(&mut self) {
        self.grow.snap_to(0.0);
        self.grow.set_target(1.0);
        self.hovered = None;
    }

    /// Starts closing (the radius springs back to zero).
    pub fn close(&mut self) {
        self.closing = true;
        self.grow.set_target(0.0);
    }

    /// `true` once closed all the way.
    #[must_use]
    pub fn is_gone(&self) -> bool {
        self.closing && self.grow.is_settled()
    }

    /// Advances the spring. Returns `true` while it moves.
    pub fn step(&mut self, dt: f32) -> bool {
        self.grow.step(dt)
    }

    /// Draws the menu over `screen` and handles the pointer.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        screen: Rect,
        labels: &[String],
        radius: f32,
        theme: &Theme,
    ) -> RadialOutcome {
        let grow = self.grow.position().clamp(0.0, 1.2);
        let outer = radius * grow;
        let inner = outer * 0.34;
        let mut outcome = RadialOutcome::Open;
        egui::Area::new(Id::unique("sue-radial"))
            .order(Order::Foreground)
            .fixed_pos(screen.min)
            .interactable(!self.closing)
            .show(ctx, |ui| {
                let response = ui.allocate_rect(screen, Sense::click_and_drag());
                let painter = ui.painter();
                let pointer = ctx.input(|input| input.pointer.hover_pos());
                self.hovered = pointer
                    .filter(|_| !self.closing)
                    .and_then(|at| wedge_at(at - self.center, labels.len(), inner))
                    .filter(|_| pointer.is_some_and(|at| at.distance(self.center) <= outer * 1.6));
                let alpha = grow.clamp(0.0, 1.0);
                painter.circle_filled(
                    self.center,
                    outer + theme.shadow_blur * 0.5,
                    theme.shadow.gamma_multiply(0.25 * alpha),
                );
                // A solid disc under the wedges, so whatever is behind the
                // menu (a studio's palettes) never shows through it.
                painter.circle_filled(self.center, outer + 1.0, theme.panel.gamma_multiply(alpha));
                let slice = TAU / labels.len().max(1) as f32;
                for (index, label) in labels.iter().enumerate() {
                    let middle = index as f32 * slice - FRAC_PI_2;
                    let (from, to) = (middle - slice / 2.0 + 0.02, middle + slice / 2.0 - 0.02);
                    let steps = 12;
                    let mut points = Vec::with_capacity(steps * 2 + 2);
                    for step in 0..=steps {
                        let angle = from + (to - from) * step as f32 / steps as f32;
                        points.push(self.center + vec2(angle.cos(), angle.sin()) * outer);
                    }
                    for step in (0..=steps).rev() {
                        let angle = from + (to - from) * step as f32 / steps as f32;
                        points.push(self.center + vec2(angle.cos(), angle.sin()) * inner);
                    }
                    let hot = self.hovered == Some(index);
                    let fill = if hot { theme.hover } else { theme.pill };
                    // A ring slice is not convex: fill it as quads from the
                    // outer arc to the inner one.
                    let mut mesh = egui::Mesh::default();
                    let color = fill.gamma_multiply(alpha);
                    for &point in &points {
                        mesh.colored_vertex(point, color);
                    }
                    let last = points.len() as u32 - 1;
                    for step in 0..steps as u32 {
                        let (outer_a, outer_b) = (step, step + 1);
                        let (inner_a, inner_b) = (last - step, last - step - 1);
                        mesh.add_triangle(outer_a, outer_b, inner_b);
                        mesh.add_triangle(outer_a, inner_b, inner_a);
                    }
                    painter.add(Shape::mesh(mesh));
                    painter.add(Shape::closed_line(
                        points,
                        Stroke::new(
                            theme.border_width,
                            if hot {
                                theme.border_active
                            } else {
                                theme.border
                            }
                            .gamma_multiply(alpha),
                        ),
                    ));
                    let text_at =
                        self.center + vec2(middle.cos(), middle.sin()) * (inner + outer) * 0.5;
                    if grow > 0.35 {
                        painter.text(
                            text_at,
                            Align2::CENTER_CENTER,
                            label,
                            theme.font(theme.small_size),
                            if hot { theme.text } else { theme.muted }.gamma_multiply(alpha),
                        );
                    }
                }
                if self.parents.is_empty() {
                    painter.circle_filled(self.center, 3.0, theme.text.gamma_multiply(alpha));
                } else {
                    // The middle of a nested wheel goes back.
                    let back = pointer.is_some_and(|at| at.distance(self.center) < inner);
                    painter.circle_filled(
                        self.center,
                        inner * 0.8,
                        if back { theme.hover } else { theme.pill }.gamma_multiply(alpha),
                    );
                    if grow > 0.35 {
                        painter.text(
                            self.center,
                            Align2::CENTER_CENTER,
                            "Back",
                            theme.font(theme.small_size),
                            if back { theme.text } else { theme.muted }.gamma_multiply(alpha),
                        );
                    }
                }
                if self.closing {
                    return;
                }
                let released_right = ctx.input(|input| {
                    input
                        .pointer
                        .button_released(egui::PointerButton::Secondary)
                });
                let moved_away = pointer.is_some_and(|at| at.distance(self.center) > inner);
                let in_middle = pointer.is_some_and(|at| at.distance(self.center) < inner);
                if response.clicked() || (released_right && moved_away) {
                    outcome = match self.hovered {
                        Some(index) => RadialOutcome::Picked(index),
                        None if response.clicked() && in_middle && !self.parents.is_empty() => {
                            RadialOutcome::Back
                        }
                        None if response.clicked() => RadialOutcome::Dismissed,
                        None => RadialOutcome::Open,
                    };
                }
                if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
                    outcome = RadialOutcome::Dismissed;
                }
            });
        if matches!(outcome, RadialOutcome::Dismissed) {
            self.close();
        }
        outcome
    }
}

/// A point on the ring of wedge `index`, for tests and scripted runs.
#[must_use]
pub fn wedge_point(center: Pos2, index: usize, count: usize, radius: f32) -> Pos2 {
    let slice = TAU / count.max(1) as f32;
    let angle = index as f32 * slice - FRAC_PI_2;
    pos2(
        center.x + angle.cos() * radius * 0.67,
        center.y + angle.sin() * radius * 0.67,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wedges_start_at_the_top_and_go_clockwise() {
        assert_eq!(wedge_at(vec2(0.0, -50.0), 6, 10.0), Some(0));
        assert_eq!(wedge_at(vec2(50.0, -28.0), 6, 10.0), Some(1));
        assert_eq!(wedge_at(vec2(0.0, 50.0), 6, 10.0), Some(3));
        assert_eq!(wedge_at(vec2(-50.0, -28.0), 6, 10.0), Some(5));
        assert_eq!(wedge_at(vec2(1.0, 1.0), 6, 10.0), None);
        for index in 0..6 {
            let point = wedge_point(Pos2::ZERO, index, 6, 100.0);
            assert_eq!(wedge_at(point.to_vec2(), 6, 10.0), Some(index));
        }
    }

    #[test]
    fn the_radius_springs_open_and_closed() {
        let params = SpringParams {
            stiffness: 260.0,
            damping: 32.0,
            mass: 1.0,
        };
        let mut radial = Radial::open(Pos2::ZERO, Pos2::ZERO, params, SpringMode::Physics);
        let mut frames = 0;
        while radial.step(1.0 / 120.0) {
            frames += 1;
        }
        assert!(frames > 5);
        radial.close();
        while radial.step(1.0 / 120.0) {}
        assert!(radial.is_gone());
    }

    #[test]
    fn nested_wheels_grow_around_the_pick_and_go_back() {
        let params = SpringParams {
            stiffness: 260.0,
            damping: 32.0,
            mass: 1.0,
        };
        let mut radial = Radial::open(pos2(100.0, 100.0), Pos2::ZERO, params, SpringMode::Physics);
        while radial.step(1.0 / 120.0) {}
        radial.descend(pos2(160.0, 100.0));
        assert_eq!(radial.depth(), 1);
        assert_eq!(radial.center, pos2(160.0, 100.0));
        assert!(radial.step(1.0 / 120.0), "the new wheel grows from nothing");
        radial.descend(pos2(220.0, 140.0));
        assert!(radial.ascend());
        assert_eq!(radial.center, pos2(160.0, 100.0));
        assert!(radial.ascend());
        assert_eq!(radial.center, pos2(100.0, 100.0));
        assert!(!radial.ascend(), "the first wheel has nothing before it");
    }
}
