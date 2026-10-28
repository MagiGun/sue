//! How a picture sits in its canvas view: zoom, scroll, rotation and flips.
//! The view maps picture pixels to screen points and back, so pointer
//! input lands on the right pixel however the view is turned.

use egui::{Pos2, Rect, Vec2, pos2, vec2};

/// Zoom limits (1.0 is 100%).
pub const ZOOM_RANGE: (f32, f32) = (0.01, 64.0);

/// One picture's view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// Screen points per picture pixel.
    pub zoom: f32,
    /// Offset of the picture's center from the viewport's center, in points.
    pub pan: Vec2,
    /// Clockwise rotation in degrees.
    pub rotation: f32,
    /// Mirrored left to right.
    pub flip_h: bool,
    /// Mirrored top to bottom.
    pub flip_v: bool,
    /// `false` until the view is first fitted to its viewport.
    pub fitted: bool,
}

impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
            fitted: false,
        }
    }
}

fn rotate(v: Vec2, degrees: f32) -> Vec2 {
    let (sin, cos) = degrees.to_radians().sin_cos();
    vec2(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}

impl View {
    /// The screen point of picture pixel coordinates `p`.
    #[must_use]
    pub fn to_screen(&self, viewport: Rect, size: [u32; 2], p: Pos2) -> Pos2 {
        let mut v = p - pos2(size[0] as f32 / 2.0, size[1] as f32 / 2.0);
        if self.flip_h {
            v.x = -v.x;
        }
        if self.flip_v {
            v.y = -v.y;
        }
        viewport.center() + self.pan + rotate(v * self.zoom, self.rotation)
    }

    /// The picture coordinates under screen point `s`.
    #[must_use]
    pub fn to_picture(&self, viewport: Rect, size: [u32; 2], s: Pos2) -> Pos2 {
        let mut v = rotate(s - viewport.center() - self.pan, -self.rotation) / self.zoom;
        if self.flip_h {
            v.x = -v.x;
        }
        if self.flip_v {
            v.y = -v.y;
        }
        pos2(size[0] as f32 / 2.0, size[1] as f32 / 2.0) + v
    }

    /// The picture's corners on screen: top left, top right, bottom right,
    /// bottom left (of the unflipped picture).
    #[must_use]
    pub fn corners(&self, viewport: Rect, size: [u32; 2]) -> [Pos2; 4] {
        let (w, h) = (size[0] as f32, size[1] as f32);
        [pos2(0.0, 0.0), pos2(w, 0.0), pos2(w, h), pos2(0.0, h)]
            .map(|corner| self.to_screen(viewport, size, corner))
    }

    /// The zoom that shows the whole picture with a margin.
    #[must_use]
    pub fn fit_zoom(viewport: Rect, size: [u32; 2]) -> f32 {
        let room = (viewport.size() - vec2(48.0, 48.0)).max(vec2(16.0, 16.0));
        (room.x / size[0].max(1) as f32)
            .min(room.y / size[1].max(1) as f32)
            .clamp(ZOOM_RANGE.0, ZOOM_RANGE.1)
    }

    /// Shows the whole picture, centered and unrotated.
    pub fn fit(&mut self, viewport: Rect, size: [u32; 2]) {
        self.zoom = Self::fit_zoom(viewport, size);
        self.pan = Vec2::ZERO;
        self.rotation = 0.0;
        self.fitted = true;
    }

    /// Zooms by `factor`, keeping the pixel under `anchor` in place.
    pub fn zoom_at(&mut self, viewport: Rect, size: [u32; 2], anchor: Pos2, factor: f32) {
        self.set_zoom_at(viewport, size, anchor, self.zoom * factor);
    }

    /// Sets the zoom, keeping the pixel under `anchor` in place.
    pub fn set_zoom_at(&mut self, viewport: Rect, size: [u32; 2], anchor: Pos2, zoom: f32) {
        let under = self.to_picture(viewport, size, anchor);
        self.zoom = zoom.clamp(ZOOM_RANGE.0, ZOOM_RANGE.1);
        let moved = self.to_screen(viewport, size, under);
        self.pan += anchor - moved;
    }

    /// Turns the view by `degrees` around the viewport's center.
    pub fn rotate_by(&mut self, viewport: Rect, size: [u32; 2], degrees: f32) {
        self.set_rotation(viewport, size, self.rotation + degrees);
    }

    /// Sets the rotation, keeping the pixel at the viewport's center there.
    pub fn set_rotation(&mut self, viewport: Rect, size: [u32; 2], degrees: f32) {
        let center = viewport.center();
        let under = self.to_picture(viewport, size, center);
        let mut turned = (degrees + 180.0).rem_euclid(360.0) - 180.0;
        if (turned + 180.0).abs() < 1e-3 {
            turned = 180.0;
        }
        self.rotation = turned;
        let moved = self.to_screen(viewport, size, under);
        self.pan += center - moved;
    }

    /// Mirrors the view, keeping the pixel at the viewport's center there.
    pub fn flip(&mut self, viewport: Rect, size: [u32; 2], horizontal: bool) {
        let center = viewport.center();
        let under = self.to_picture(viewport, size, center);
        if horizontal {
            self.flip_h = !self.flip_h;
        } else {
            self.flip_v = !self.flip_v;
        }
        let moved = self.to_screen(viewport, size, under);
        self.pan += center - moved;
    }

    /// Scrolls so picture point `p` sits at the viewport's center.
    pub fn center_on(&mut self, viewport: Rect, size: [u32; 2], p: Pos2) {
        let at = self.to_screen(viewport, size, p);
        self.pan += viewport.center() - at;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Rect {
        Rect::from_min_size(pos2(100.0, 50.0), vec2(800.0, 600.0))
    }

    #[test]
    fn screen_and_picture_points_map_both_ways_however_turned() {
        let size = [400, 300];
        for (rotation, flip_h, flip_v) in [
            (0.0, false, false),
            (37.0, true, false),
            (-90.0, true, true),
        ] {
            let view = View {
                zoom: 1.7,
                pan: vec2(13.0, -40.0),
                rotation,
                flip_h,
                flip_v,
                fitted: true,
            };
            for point in [pos2(0.0, 0.0), pos2(399.0, 12.5), pos2(200.0, 150.0)] {
                let screen = view.to_screen(viewport(), size, point);
                let back = view.to_picture(viewport(), size, screen);
                assert!(
                    (back - point).length() < 1e-3,
                    "{point:?} came back as {back:?}"
                );
            }
        }
    }

    #[test]
    fn zooming_and_turning_keep_the_anchor_still() {
        let size = [1600, 1200];
        let mut view = View::default();
        view.fit(viewport(), size);
        assert!(view.zoom < 1.0);
        let picture_center = view.to_screen(viewport(), size, pos2(800.0, 600.0));
        assert!((picture_center - viewport().center()).length() < 1e-3);
        let anchor = pos2(300.0, 200.0);
        let under = view.to_picture(viewport(), size, anchor);
        view.zoom_at(viewport(), size, anchor, 3.0);
        let after = view.to_picture(viewport(), size, anchor);
        assert!((after - under).length() < 1e-2);
        let middle = view.to_picture(viewport(), size, viewport().center());
        view.rotate_by(viewport(), size, 45.0);
        view.flip(viewport(), size, true);
        let still = view.to_picture(viewport(), size, viewport().center());
        assert!((still - middle).length() < 1e-2);
        view.set_rotation(viewport(), size, 200.0);
        assert!(
            (view.rotation + 160.0).abs() < 1e-3,
            "wrapped into -180..180"
        );
        view.zoom_at(viewport(), size, anchor, 1e6);
        assert_eq!(view.zoom, ZOOM_RANGE.1);
    }
}
