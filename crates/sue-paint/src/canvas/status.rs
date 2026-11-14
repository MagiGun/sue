//! The status strip below the canvas.

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};
use sue_wm::Theme;

use super::view::{SCROLLBAR, scrollbar};
use crate::icons::Icon;
use crate::panels::child;
use crate::studio::SuePaint;

impl SuePaint {
    pub(super) fn status_strip(
        &mut self,
        ui: &mut Ui,
        strip: Rect,
        size: Option<[u32; 2]>,
        theme: &Theme,
    ) {
        ui.painter().rect_filled(strip, 0.0, theme.pill);
        ui.painter()
            .hline(strip.x_range(), strip.top(), theme.hairline());
        let Some(size) = size else {
            return;
        };
        let viewport = self.viewport;
        let mut inner = child(ui, strip.shrink2(vec2(8.0, 2.0)), "status-strip");
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        // As Clip Studio Paint's: the zoom and the turn, each a value and a
        // wedge with its buttons, then the horizontal scrollbar.
        let mut turn_button = None;
        let mut zoom_button = None;
        let (mut percent, mut rotation) = (doc.view.zoom * 100.0, doc.view.rotation);
        let (mut zoomed, mut turned) = (false, false);
        let controls = inner
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                zoomed |= ui
                    .add_sized(
                        vec2(46.0, 18.0),
                        egui::DragValue::new(&mut percent)
                            .range(1.0..=6400.0)
                            .max_decimals(1),
                    )
                    .on_hover_text(tr!("zoom"))
                    .changed();
                zoomed |= ui
                    .add(
                        crate::panels::Wedge::new(&mut percent, 1.0..=6400.0, 60.0, theme)
                            .logarithmic(),
                    )
                    .changed();
                for (index, (icon, tip)) in [
                    (Icon::ZoomOut, tr!("zoom-out")),
                    (Icon::ZoomIn, tr!("zoom-in")),
                    (Icon::Fit, tr!("fit-to-screen-ctrl-0")),
                ]
                .into_iter()
                .enumerate()
                {
                    let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                    if crate::panels::icon_button(ui, rect, icon, true, false, &tip, theme) {
                        zoom_button = Some(index);
                    }
                }
                ui.add_space(6.0);
                turned |= ui
                    .add_sized(
                        vec2(40.0, 18.0),
                        egui::DragValue::new(&mut rotation)
                            .range(-180.0..=180.0)
                            .max_decimals(1),
                    )
                    .on_hover_text(tr!("rotation"))
                    .changed();
                turned |= ui
                    .add(crate::panels::Wedge::new(
                        &mut rotation,
                        -180.0..=180.0,
                        60.0,
                        theme,
                    ))
                    .changed();
                for (index, (icon, tip, active)) in [
                    (Icon::RotateLeft, tr!("rotate-left"), false),
                    (Icon::RotateRight, tr!("rotate-right"), false),
                    (Icon::Reset, tr!("reset-rotation"), false),
                    (Icon::FlipH, tr!("flip-view-left-right"), doc.view.flip_h),
                ]
                .into_iter()
                .enumerate()
                {
                    let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                    if crate::panels::icon_button(ui, rect, icon, true, active, &tip, theme) {
                        turn_button = Some(index);
                    }
                }
            })
            .response
            .rect;
        let center = viewport.center();
        if zoomed {
            doc.view
                .set_zoom_at(viewport, size, center, percent / 100.0);
        }
        if turned {
            doc.view.set_rotation(viewport, size, rotation);
        }
        match zoom_button {
            Some(0) => doc.view.zoom_at(viewport, size, center, 0.8),
            Some(1) => doc.view.zoom_at(viewport, size, center, 1.25),
            Some(_) => doc.view.fit(viewport, size),
            None => {}
        }
        match turn_button {
            Some(0) => doc.view.rotate_by(viewport, size, -15.0),
            Some(1) => doc.view.rotate_by(viewport, size, 15.0),
            Some(2) => doc.view.set_rotation(viewport, size, 0.0),
            Some(_) => doc.view.flip(viewport, size, true),
            None => {}
        }
        let bar = Rect::from_min_max(
            pos2(controls.right() + 12.0, strip.center().y - SCROLLBAR / 2.0),
            pos2(
                strip.right() - SCROLLBAR - 4.0,
                strip.center().y + SCROLLBAR / 2.0,
            ),
        );
        if bar.width() > 40.0 && self.scroll_bars {
            let extent = size[0] as f32 * doc.view.zoom;
            if let Some(pan) = scrollbar(
                ui,
                bar,
                true,
                doc.view.pan.x,
                extent,
                viewport.width(),
                "sue-paint-scroll-h",
                theme,
            ) {
                doc.view.pan.x = pan;
            }
        }
        let _ = Align2::LEFT_CENTER;
    }
}
