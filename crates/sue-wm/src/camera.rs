//! The infinite canvas camera. Pan and zoom are three [`SpringDamper`]s
//! (center x, center y and log zoom), so framing an entity, resetting the
//! view or a wheel notch glides instead of teleporting. Direct manipulation
//! (a drag) snaps the springs so the canvas sticks to the pointer.

use egui::emath::TSTransform;
use egui::{Pos2, Rect, Vec2, pos2, vec2};

use crate::{SpringDamper, SpringMode, SpringParams};

/// Pan and zoom of the canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    x: SpringDamper,
    y: SpringDamper,
    log_zoom: SpringDamper,
    zoom_range: (f32, f32),
    /// While a wheel zoom runs: the screen point that keeps showing the
    /// same canvas point.
    anchor: Option<(Pos2, Pos2)>,
}

impl Camera {
    /// A camera centered on the origin at 100%.
    #[must_use]
    pub fn new(params: SpringParams, mode: SpringMode, zoom_range: (f32, f32)) -> Self {
        Self {
            x: SpringDamper::new(params, mode, 0.0),
            y: SpringDamper::new(params, mode, 0.0),
            log_zoom: SpringDamper::new(params, mode, 0.0),
            zoom_range,
            anchor: None,
        }
    }

    /// Follows new spring constants, mode and zoom limits.
    pub fn configure(&mut self, params: SpringParams, mode: SpringMode, zoom_range: (f32, f32)) {
        for spring in [&mut self.x, &mut self.y, &mut self.log_zoom] {
            spring.set_params(params);
            spring.set_mode(mode);
        }
        self.zoom_range = zoom_range;
        let clamped = self.clamp_log(self.log_zoom.target());
        if clamped != self.log_zoom.target() {
            self.log_zoom.set_target(clamped);
        }
    }

    fn clamp_log(&self, log_zoom: f32) -> f32 {
        let (min, max) = self.zoom_range;
        log_zoom.clamp(min.max(1e-3).ln(), max.max(min).ln())
    }

    /// The current zoom (1.0 is 100%).
    #[must_use]
    pub fn zoom(&self) -> f32 {
        self.log_zoom.position().exp()
    }

    /// The zoom the camera is heading to.
    #[must_use]
    pub fn target_zoom(&self) -> f32 {
        self.log_zoom.target().exp()
    }

    /// The canvas point at the center of the viewport.
    #[must_use]
    pub fn center(&self) -> Pos2 {
        pos2(self.x.position(), self.y.position())
    }

    /// Canvas to screen, for a viewport.
    #[must_use]
    pub fn transform(&self, viewport: Rect) -> TSTransform {
        let zoom = self.zoom();
        TSTransform::new(
            viewport.center().to_vec2() - self.center().to_vec2() * zoom,
            zoom,
        )
    }

    /// A canvas point on screen.
    #[must_use]
    pub fn to_screen(&self, viewport: Rect, point: Pos2) -> Pos2 {
        self.transform(viewport) * point
    }

    /// A screen point on the canvas.
    #[must_use]
    pub fn to_canvas(&self, viewport: Rect, point: Pos2) -> Pos2 {
        self.transform(viewport).inverse() * point
    }

    /// The canvas area the viewport shows.
    #[must_use]
    pub fn visible(&self, viewport: Rect) -> Rect {
        self.transform(viewport).inverse() * viewport
    }

    /// Glides to show `target` (canvas units) inside a viewport of `size`
    /// with `margin` screen points around it.
    pub fn frame(&mut self, size: Vec2, target: Rect, margin: f32) {
        self.anchor = None;
        let room = (size - vec2(margin, margin) * 2.0).max(vec2(1.0, 1.0));
        let fit = (room.x / target.width().max(1.0)).min(room.y / target.height().max(1.0));
        self.x.set_target(target.center().x);
        self.y.set_target(target.center().y);
        self.log_zoom.set_target(self.clamp_log(fit.max(1e-3).ln()));
    }

    /// Glides to `zoom` around the canvas point `center`.
    pub fn glide_to(&mut self, center: Pos2, zoom: f32) {
        self.anchor = None;
        self.x.set_target(center.x);
        self.y.set_target(center.y);
        self.log_zoom
            .set_target(self.clamp_log(zoom.max(1e-3).ln()));
    }

    /// Zooms by `log_step` (natural log) around the screen point `pointer`,
    /// which keeps showing the same canvas point while the zoom glides.
    pub fn zoom_at(&mut self, viewport: Rect, pointer: Pos2, log_step: f32) {
        let canvas_point = match self.anchor {
            Some((screen, canvas)) if screen.distance(pointer) < 0.5 => canvas,
            _ => self.to_canvas(viewport, pointer),
        };
        self.anchor = Some((pointer, canvas_point));
        let target = self.clamp_log(self.log_zoom.target() + log_step);
        self.log_zoom.set_target(target);
        self.follow_anchor(viewport);
    }

    /// Pans by a screen delta, sticking to the pointer.
    pub fn pan_by(&mut self, screen_delta: Vec2) {
        self.anchor = None;
        let zoom = self.zoom();
        let center = self.center() - screen_delta / zoom;
        self.x.snap_to(center.x);
        self.y.snap_to(center.y);
    }

    fn follow_anchor(&mut self, viewport: Rect) {
        if let Some((screen, canvas)) = self.anchor {
            let zoom = self.zoom();
            let center = canvas - (screen - viewport.center()) / zoom;
            self.x.snap_to(center.x);
            self.y.snap_to(center.y);
        }
    }

    /// Advances the springs by `dt` seconds. Returns `true` while moving.
    /// Pan settles once it is within a twentieth of a screen pixel, so a
    /// far-zoomed camera does not keep drawing for invisible motion.
    pub fn step(&mut self, dt: f32, viewport: Rect) -> bool {
        let zoom_moving = self.log_zoom.step(dt);
        let zoom = self.zoom();
        let mut moving = zoom_moving;
        for spring in [&mut self.x, &mut self.y] {
            if spring.step(dt) {
                let close = (spring.position() - spring.target()).abs() * zoom < 0.05
                    && spring.velocity().abs() * zoom < 0.5;
                if close {
                    spring.snap_to(spring.target());
                } else {
                    moving = true;
                }
            }
        }
        self.follow_anchor(viewport);
        if !zoom_moving {
            self.anchor = None;
        }
        moving
    }

    /// `true` while any spring moves.
    #[must_use]
    pub fn is_moving(&self) -> bool {
        !(self.x.is_settled() && self.y.is_settled() && self.log_zoom.is_settled())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera(mode: SpringMode) -> Camera {
        Camera::new(
            SpringParams {
                stiffness: 600.0,
                damping: 49.0,
                mass: 1.0,
            },
            mode,
            (0.1, 32.0),
        )
    }

    fn viewport() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 800.0))
    }

    #[test]
    fn framing_glides_in_about_a_quarter_second() {
        let mut camera = camera(SpringMode::Physics);
        let target = Rect::from_min_size(pos2(500.0, 300.0), vec2(400.0, 300.0));
        camera.frame(viewport().size(), target, 50.0);
        let mut frames = 0;
        while camera.step(1.0 / 120.0, viewport()) {
            frames += 1;
            assert!(frames < 240, "the camera never settled: {camera:?}");
        }
        assert!(frames > 10, "framing must animate, not teleport");
        // Within 2% of the way left after about a quarter second.
        let seconds_to_close = {
            let mut probe = self::camera(SpringMode::Physics);
            probe.frame(viewport().size(), target, 50.0);
            let start = target.center().distance(Pos2::ZERO);
            let mut t = 0.0;
            while probe.center().distance(target.center()) > start * 0.02 {
                probe.step(1.0 / 120.0, viewport());
                t += 1.0 / 120.0;
            }
            t
        };
        assert!(seconds_to_close < 0.3, "{seconds_to_close}");
        assert!((camera.center() - target.center()).length() < 0.01);
        assert!((camera.zoom() - 2.25).abs() < 0.01);
    }

    #[test]
    fn wheel_zoom_keeps_the_point_under_the_pointer() {
        let mut camera = camera(SpringMode::Physics);
        let pointer = pos2(800.0, 200.0);
        let before = camera.to_canvas(viewport(), pointer);
        camera.zoom_at(viewport(), pointer, 0.5);
        while camera.step(1.0 / 120.0, viewport()) {
            let now = camera.to_canvas(viewport(), pointer);
            assert!(now.distance(before) < 0.01);
        }
        assert!((camera.zoom() - 0.5_f32.exp()).abs() < 0.01);
    }

    #[test]
    fn zoom_stays_between_ten_percent_and_thirty_two_hundred() {
        let mut camera = camera(SpringMode::InstantSnap);
        camera.zoom_at(viewport(), pos2(10.0, 10.0), 100.0);
        assert!((camera.zoom() - 32.0).abs() < 0.01);
        camera.zoom_at(viewport(), pos2(10.0, 10.0), -100.0);
        assert!((camera.zoom() - 0.1).abs() < 0.001);
    }

    #[test]
    fn drags_stick_to_the_pointer() {
        let mut camera = camera(SpringMode::Physics);
        camera.pan_by(vec2(100.0, 0.0));
        assert_eq!(camera.center(), pos2(-100.0, 0.0));
        assert!(!camera.is_moving());
    }
}
