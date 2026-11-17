//! Puppet warp on the canvas: its pins, the mesh they bend, and its bar.
//!
//! A click puts a pin (or, with Click to add pins off, starts a box that
//! picks the pins in it); a click on a pin picks it, Shift and click picks
//! several, and a drag moves the picked pins together. Picked pins show a
//! ring: dragging the ring turns them (Shift in steps of 15 degrees), and
//! the part of the drawing under each turns with it. Alt and click takes a
//! pin away, Delete takes the picked ones away.
//!
//! The bar over the canvas holds what Clip Studio Paint keeps in Puppet
//! warp's Tool Property (Keep original image, Expansion, Show mesh, Click
//! pin, Select pins, Correct pin rotation, the interpolation) with Reset,
//! Cancel and OK. Each part of the drawing bends only by the pins on it
//! (`sue_picture::warp::puppet`).

use egui::{Align2, Id, Key, Pos2, Rect, Ui, Vec2, pos2};
use sue_core::RasterTile2D;
use sue_wm::icons::Icon;
use sue_wm::ui::{Elevation, Radius, Space, TextRole, Tip};
use sue_wm::{AppletContext, Theme};

use crate::doc;
use crate::studio::{Gesture, SuePaint, WarpSession};
use crate::warp::{Interpolation, Pin, PuppetSettings};

/// How far from a picked pin, in points, its ring lies.
pub(crate) const RING: f32 = 22.0;

/// What a drag on puppet warp's pins does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PinHow {
    /// Moves the picked pins.
    Move,
    /// Turns the picked pins around `about` (layer pixels).
    Turn { about: [f32; 2] },
    /// Picks the pins in a box (adding to those picked when `add`).
    Pick { add: bool },
}

/// A drag on puppet warp's pins: where it began and where it is now
/// (layer pixels), the pins as they were, and what it does.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PinDrag {
    pub start: [f32; 2],
    pub now: [f32; 2],
    pub before: Vec<Pin>,
    pub how: PinHow,
}

/// The middle of the picked pins.
#[must_use]
pub(crate) fn picked_middle(pins: &[Pin]) -> Option<[f32; 2]> {
    let picked: Vec<&Pin> = pins.iter().filter(|pin| pin.picked).collect();
    if picked.is_empty() {
        return None;
    }
    let count = picked.len() as f32;
    let sum = picked.iter().fold([0.0, 0.0], |sum, pin| {
        [sum[0] + pin.to[0], sum[1] + pin.to[1]]
    });
    Some([sum[0] / count, sum[1] / count])
}

impl PinDrag {
    /// The pins with this drag carried to `now` (layer pixels), turning
    /// in steps of `step` radians when above nothing.
    #[must_use]
    pub(crate) fn applied(&self, now: [f32; 2], step: f32) -> Vec<Pin> {
        let mut pins = self.before.clone();
        match self.how {
            PinHow::Move => crate::warp::move_picked(
                &mut pins,
                [now[0] - self.start[0], now[1] - self.start[1]],
            ),
            PinHow::Turn { about } => {
                let angle = crate::transform::drag_angle(about, self.start, now, step);
                crate::warp::turn_picked(&mut pins, about, angle);
            }
            PinHow::Pick { add } => crate::warp::pick_inside(
                &mut pins,
                [self.start[0], self.start[1], now[0], now[1]],
                add,
            ),
        }
        pins
    }
}

impl WarpSession {
    /// The layer's pixels bent by the pins as `settings` say.
    #[must_use]
    pub(crate) fn bend(&self, settings: &PuppetSettings) -> Vec<u8> {
        crate::warp::puppet(
            &self.before,
            self.size[0],
            self.size[1],
            &self.pins,
            settings,
        )
    }
}

impl SuePaint {
    /// Shows the layer bent by the pins and settings as they are now, and
    /// the mesh as they bend it.
    pub(crate) fn puppet_preview(&mut self, cx: &mut AppletContext<'_>) {
        let settings = self.transform_prefs.puppet;
        let Some(warp) = self.warp.as_mut() else {
            return;
        };
        if warp.pins == warp.shown && warp.shown_settings == Some(settings) {
            return;
        }
        let pixels = warp.bend(&settings);
        if let Some(bytes) = doc::raster_mut(&mut cx.world.arena, warp.layer)
            .and_then(RasterTile2D::bytes_mut)
            .filter(|bytes| bytes.len() == pixels.len())
        {
            bytes.copy_from_slice(&pixels);
        }
        warp.mesh = if settings.show_mesh {
            crate::warp::PuppetMesh::build(
                &warp.before,
                warp.size[0],
                warp.size[1],
                settings.spacing,
                settings.expansion,
            )
            .bent(&warp.pins, &settings)
            .into_iter()
            .map(|(_, to)| to)
            .collect()
        } else {
            Vec::new()
        };
        warp.shown = warp.pins.clone();
        warp.shown_settings = Some(settings);
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor
                .refresh(&cx.world.arena, [0, 0, u32::MAX, u32::MAX]);
        }
    }

    /// Puppet warp's pins, the mesh they bend, a box picking pins, and its
    /// bar (the settings, Reset, Cancel, OK; Enter keeps the bend, Esc
    /// puts the layer back, Delete takes the picked pins away).
    pub(super) fn warp_ui(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(warp) = self.warp.as_ref() else {
            return;
        };
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return;
        };
        let offset = [warp.offset[0] as f32, warp.offset[1] as f32];
        let screen =
            |p: [f32; 2]| view.to_screen(viewport, size, pos2(p[0] + offset[0], p[1] + offset[1]));
        let painter = ui.painter().with_clip_rect(viewport);
        // The mesh over the layer's drawing, bent as the pins bend it.
        if self.transform_prefs.puppet.show_mesh {
            // Each square of the mesh is two triangles; its outline is
            // drawn, not the diagonal between them.
            for pair in warp.mesh.chunks(2) {
                let points: Vec<Pos2> = match pair {
                    [[a, b, c], [_, _, d]] => [a, b, c, d].map(|corner| screen(*corner)).to_vec(),
                    _ => pair[0].iter().map(|corner| screen(*corner)).collect(),
                };
                super::marks::mesh(&painter, points, theme);
            }
        }
        let drag = match &self.gesture {
            Some(Gesture::Pin { drag }) => Some(drag.clone()),
            _ => None,
        };
        for (index, pin) in warp.pins.iter().enumerate() {
            let at = screen(pin.to);
            crate::launchers::note_mark(ui.ctx(), &format!("pin.{index}"), at);
            super::marks::pin(&painter, at, pin.angle, pin.picked, theme);
            if pin.picked {
                super::marks::ring(&painter, at, RING, theme);
            }
        }
        if let Some(PinDrag {
            start,
            now,
            how: PinHow::Pick { .. },
            ..
        }) = &drag
        {
            let corners = [
                [start[0], start[1]],
                [now[0], start[1]],
                [now[0], now[1]],
                [start[0], now[1]],
            ]
            .map(screen);
            for edge in 0..4 {
                super::marks::dashed(&painter, corners[edge], corners[(edge + 1) % 4], theme);
            }
        }
        let action = self.puppet_bar(ui, viewport, theme);
        let (enter, escape, delete) = ui.input(|input| {
            (
                input.key_pressed(Key::Enter),
                input.key_pressed(Key::Escape),
                input.key_pressed(Key::Delete) || input.key_pressed(Key::Backspace),
            )
        });
        let typing = ui.ctx().text_edit_focused();
        let action = if enter && !typing {
            Some(BarAction::Ok)
        } else if escape && !typing {
            Some(BarAction::Cancel)
        } else if delete && !typing {
            Some(BarAction::DeletePicked)
        } else {
            action
        };
        match action {
            Some(BarAction::Reset) => {
                if let Some(warp) = self.warp.as_mut() {
                    warp.pins.clear();
                }
                self.warp_preview(cx);
            }
            Some(BarAction::DeletePicked) => {
                if let Some(warp) = self.warp.as_mut() {
                    warp.pins.retain(|pin| !pin.picked);
                }
                self.warp_preview(cx);
            }
            Some(BarAction::Cancel) => self.finish_warp(cx, false),
            Some(BarAction::Ok) => self.finish_warp(cx, true),
            None => self.warp_preview(cx),
        }
    }

    /// Puppet warp's bar over the canvas.
    #[expect(clippy::too_many_lines, reason = "one control after another")]
    fn puppet_bar(&mut self, ui: &mut Ui, viewport: Rect, theme: &Theme) -> Option<BarAction> {
        let pins = self.warp.as_ref().map_or(0, |warp| warp.pins.len());
        let settings = &mut self.transform_prefs.puppet;
        let mut action = None;
        let gap = theme.space(Space::S2);
        let frame = egui::Frame::new()
            .fill(theme.surface(2))
            .stroke(theme.hairline())
            .corner_radius(theme.corner(Radius::Lg))
            .shadow(theme.elevation(Elevation::Raised))
            .inner_margin(egui::Margin::same(gap.round() as i8));
        let shown = egui::Area::new(Id::unique("sue-paint-warp-bar"))
            .fade_in(false)
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(viewport.center_top() + Vec2::Y * gap)
            .constrain_to(viewport)
            .show(ui.ctx(), |ui| {
                frame.show(ui, |ui| {
                    ui.set_max_width(viewport.width() - theme.space(Space::S8));
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::splat(gap);
                        ui.label(
                            egui::RichText::new(tr!("puppet-warp-2"))
                                .font(theme.text_font(TextRole::Body))
                                .color(theme.text),
                        );
                        let count = match pins {
                            0 => tr!("click-to-put-pins"),
                            1 => tr!("n-1-pin"),
                            count => tr!("pins", count = count),
                        };
                        let help = tr!("puppet-pins-help");
                        let said = ui.label(
                            egui::RichText::new(count)
                                .font(theme.text_font(TextRole::Caption))
                                .color(theme.text_muted()),
                        );
                        sue_wm::ui::tooltip(said, &Tip::new(&help), theme);
                        let switches: [(&str, String, String, &mut bool); 5] = [
                            (
                                "original",
                                tr!("keep-original-image"),
                                tr!("puppet-keep-original-help"),
                                &mut settings.keep_original,
                            ),
                            (
                                "mesh",
                                tr!("puppet-show-mesh"),
                                String::new(),
                                &mut settings.show_mesh,
                            ),
                            (
                                "click",
                                tr!("puppet-click-pin"),
                                tr!("puppet-click-pin-help"),
                                &mut settings.click_adds_pins,
                            ),
                            (
                                "select",
                                tr!("puppet-select-pins"),
                                tr!("puppet-select-pins-help"),
                                &mut settings.select_pins,
                            ),
                            (
                                "rotation",
                                tr!("puppet-correct-rotation"),
                                tr!("puppet-correct-rotation-help"),
                                &mut settings.correct_pin_rotation,
                            ),
                        ];
                        for (salt, name, help, on) in switches {
                            let flip = sue_wm::ui::switch(("sue-paint-puppet", salt), &name, *on)
                                .show(ui, theme);
                            if flip.clicked() {
                                *on = !*on;
                            }
                            sue_wm::ui::tooltip(flip, &Tip::new(&name).help(&help), theme);
                        }
                        let expansion = tr!("puppet-expansion");
                        let mut reach = f64::from(settings.expansion);
                        let pixels = tr!("puppet-pixels");
                        crate::transform_ui::caption(ui, &expansion, theme);
                        let field =
                            sue_wm::ui::number_field("sue-paint-puppet-expansion", &mut reach)
                                .width(theme.space(Space::S9) + theme.space(Space::S7))
                                .range(0.0..=64.0)
                                .suffix(&pixels)
                                .label(&expansion)
                                .show(ui, theme);
                        if field.changed() {
                            settings.expansion = reach as f32;
                        }
                        sue_wm::ui::tooltip(
                            field,
                            &Tip::new(&expansion).help(&tr!("puppet-expansion-help")),
                            theme,
                        );
                        let interpolation = tr!("puppet-interpolation");
                        let ways = [tr!("interpolation-smooth"), tr!("interpolation-hard")];
                        let mut way = usize::from(settings.interpolation == Interpolation::Nearest);
                        crate::transform_ui::caption(ui, &interpolation, theme);
                        let picked =
                            sue_wm::ui::dropdown("sue-paint-puppet-interpolation", &mut way, &ways)
                                .label(&interpolation)
                                .show(ui, theme);
                        if picked.changed() {
                            settings.interpolation = if way == 1 {
                                Interpolation::Nearest
                            } else {
                                Interpolation::Smooth
                            };
                        }
                        sue_wm::ui::tooltip(picked, &Tip::new(&interpolation), theme);
                        let reset = tr!("reset");
                        let pressed = sue_wm::ui::button("sue-paint-puppet-reset", &reset)
                            .icon(Icon::Reset)
                            .show(ui, theme);
                        if pressed.clicked() {
                            action = Some(BarAction::Reset);
                        }
                        sue_wm::ui::tooltip(
                            pressed,
                            &Tip::new(&reset).help(&tr!("take-every-pin-away")),
                            theme,
                        );
                        let cancel = tr!("cancel");
                        let pressed =
                            sue_wm::ui::button("sue-paint-puppet-cancel", &cancel).show(ui, theme);
                        if pressed.clicked() {
                            action = Some(BarAction::Cancel);
                        }
                        sue_wm::ui::tooltip(pressed, &Tip::new(&cancel).shortcut("Esc"), theme);
                        let ok = tr!("ok");
                        let pressed = sue_wm::ui::button("sue-paint-puppet-ok", &ok)
                            .primary()
                            .show(ui, theme);
                        if pressed.clicked() {
                            action = Some(BarAction::Ok);
                        }
                        sue_wm::ui::tooltip(pressed, &Tip::new(&ok).shortcut("Enter"), theme);
                    });
                });
            });
        crate::launchers::note(ui.ctx(), shown.response.rect);
        action
    }
}

/// What puppet warp's bar or keys asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BarAction {
    Reset,
    DeletePicked,
    Cancel,
    Ok,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pin_drag_moves_turns_or_picks() {
        let mut a = Pin::at([0.0, 0.0]);
        a.picked = true;
        let b = Pin::at([10.0, 0.0]);
        let drag = |how| PinDrag {
            start: [0.0, 0.0],
            now: [0.0, 0.0],
            before: vec![a, b],
            how,
        };
        // Moving carries only the picked pin.
        let moved = drag(PinHow::Move).applied([3.0, 4.0], 0.0);
        assert_eq!(moved[0].to, [3.0, 4.0]);
        assert_eq!(moved[1].to, [10.0, 0.0]);
        // Turning about the pin turns the part under it, not where it is.
        let mut turn = drag(PinHow::Turn { about: [0.0, 0.0] });
        turn.start = [10.0, 0.0];
        let turned = turn.applied([0.0, 10.0], 0.0);
        assert!((turned[0].angle - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
        assert_eq!(turned[0].to, [0.0, 0.0]);
        assert_eq!(turned[1].angle, 0.0, "the other pin is not picked");
        // A box picks what is in it.
        let picked = drag(PinHow::Pick { add: false }).applied([12.0, 2.0], 0.0);
        assert!(picked[0].picked && picked[1].picked);
        let mut away = drag(PinHow::Pick { add: false });
        away.start = [5.0, -1.0];
        let only = away.applied([12.0, 2.0], 0.0);
        assert!(!only[0].picked && only[1].picked);
        assert_eq!(picked_middle(&only), Some([10.0, 0.0]));
        assert_eq!(picked_middle(&[b]), None);
    }
}
