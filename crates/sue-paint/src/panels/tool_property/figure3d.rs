//! The 3D drawing figure's own rows of Tool Property: the model chooser
//! (Sue's bodies, hands and head, or a 3D file), the body shape sliders
//! part by part (as Clip Studio Paint's body shape palette lists them),
//! the posing options (joint angle limits, pin, reset part) and the hand
//! setup (the pad that opens, closes and folds the fingers, the spread
//! and a lock for each finger).

use egui::{Pos2, Sense, Ui, pos2, vec2};
use sue_wm::Theme;
use sue_wm::icons::Icon;
use sue_wm::ui::{self, Density, Space, TextRole, Tip};

use super::three::{Slid, check, slide};
use crate::figure::{Body, Build, HandSetup, Kind, Pose, SLIDERS, Side};
use crate::studio::SuePaint;

/// Sue's own models, as the chooser lists them: the command that makes
/// the current figure that model, what it is called, and what it is.
pub(crate) const MODELS: [(&str, &str, Kind, Build, Side); 5] = [
    (
        "figure.model.0",
        "Male body",
        Kind::Body,
        Build::Male,
        Side::Left,
    ),
    (
        "figure.model.1",
        "Female body",
        Kind::Body,
        Build::Female,
        Side::Left,
    ),
    (
        "figure.model.2",
        "Left hand",
        Kind::Hand,
        Build::Male,
        Side::Left,
    ),
    (
        "figure.model.3",
        "Right hand",
        Kind::Hand,
        Build::Male,
        Side::Right,
    ),
    (
        "figure.model.4",
        "Head",
        Kind::Head,
        Build::Male,
        Side::Left,
    ),
];

/// Which of [`MODELS`] `body` is.
pub(crate) fn model_index(body: &Body) -> Option<usize> {
    MODELS.iter().position(|(_, _, kind, build, side)| {
        *kind == body.kind
            && match body.kind {
                Kind::Body => *build == body.build,
                Kind::Hand => *side == body.side,
                _ => true,
            }
    })
}

impl SuePaint {
    /// The model chooser: which of Sue's models the figure is (picking
    /// another changes it, as one step), and a button that imports a 3D
    /// file as a new 3D layer.
    pub(super) fn model_rows(&mut self, ui: &mut Ui, theme: &Theme, body: &Body) {
        ui::section(ui, theme, &tr!("model-3d"), |ui| {
            if body.kind == Kind::Imported {
                ui.label(
                    egui::RichText::new(tr!("imported-model-posed-by-its-bones"))
                        .font(theme.text_font(TextRole::Caption))
                        .color(theme.text_muted()),
                );
            } else {
                let names: Vec<String> = MODELS
                    .iter()
                    .map(|(_, name, ..)| crate::localize(name))
                    .collect();
                let now = model_index(body).unwrap_or(0);
                let mut chosen = now;
                let response = ui::dropdown("sue-paint-figure-model", &mut chosen, &names)
                    .label(&tr!("model-3d"))
                    .show(ui, theme);
                let response = ui::tooltip(response, &Tip::new(&tr!("model-3d-help")), theme);
                if response.changed() && chosen != now {
                    self.pending_command = Some(MODELS[chosen].0);
                }
            }
            let name = tr!("import-3d-file");
            let response = ui::button("sue-paint-figure-import", &name).show(ui, theme);
            let response = ui::tooltip(response, &Tip::new(&tr!("import-3d-file-help")), theme);
            if response.clicked() {
                self.pending_command = Some("file.import.3d");
            }
        });
    }

    /// The body shape part by part: every slider of [`SLIDERS`] the
    /// figure has, under the part it shapes, with what the figure shows
    /// (guide lines) and whether it keeps to the ground.
    pub(super) fn part_rows(&mut self, ui: &mut Ui, theme: &Theme, body: &mut Body) -> Slid {
        let mut slid = Slid::default();
        let title = if body.kind == Kind::Hand {
            tr!("hand-shape")
        } else {
            tr!("body-shape-details")
        };
        ui::section(ui, theme, &title, |ui| {
            if body.kind == Kind::Hand {
                let mut height = body.height;
                let moved = slide(
                    ui,
                    theme,
                    "sue-paint-hand-height",
                    &tr!("height-2"),
                    &mut height,
                    Body::HEIGHTS,
                    &tr!("cm"),
                    0,
                );
                if moved.changed {
                    *body = body.with_height(height);
                }
                slid |= moved;
            }
            let mut group = "";
            for (index, slider) in SLIDERS.iter().enumerate() {
                let shown = match body.kind {
                    Kind::Hand => slider.hand,
                    Kind::Body => slider.key != "bust" || body.build == Build::Female,
                    _ => false,
                };
                if !shown {
                    continue;
                }
                if slider.group != group {
                    group = slider.group;
                    ui.label(
                        egui::RichText::new(crate::localize(group))
                            .font(theme.text_font(TextRole::Caption))
                            .color(theme.text_muted()),
                    );
                }
                let mut value = body.parts[index] * 100.0;
                let moved = slide(
                    ui,
                    theme,
                    ("sue-paint-figure-slider", slider.key),
                    &crate::localize(slider.name),
                    &mut value,
                    slider.range[0] * 100.0..=slider.range[1] * 100.0,
                    "",
                    0,
                );
                if moved.changed {
                    *body = body.with_slider(slider.key, value / 100.0);
                }
                slid |= moved;
            }
            let mut ticks: Vec<(&str, String, &mut bool)> = vec![(
                "sue-paint-figure-guides",
                tr!("show-guide-lines"),
                &mut body.guides,
            )];
            if body.kind == Kind::Body {
                ticks.push((
                    "sue-paint-figure-ground",
                    tr!("keep-on-the-ground"),
                    &mut body.ground,
                ));
            }
            for (salt, name, on) in ticks {
                if check(ui, theme, salt, &name, on) {
                    slid.changed = true;
                    slid.released = true;
                }
            }
        });
        slid
    }

    /// The posing options under the pose buttons: Joint angle limits,
    /// Reset part and Pin for the picked joint.
    pub(super) fn pose_option_rows(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        body: &Body,
        pinned: bool,
    ) {
        let mut limits = self.figure_limits;
        if body.kind != Kind::Imported
            && check(
                ui,
                theme,
                "sue-paint-figure-limits",
                &tr!("joint-angle-limits"),
                &mut limits,
            )
        {
            self.figure_limits = limits;
        }
        if let Some(joint) = self.figure_joint {
            ui.horizontal(|ui| {
                let name = tr!("reset-part");
                let response =
                    ui::icon_button("figure.reset_part", Icon::Reset, &name).show(ui, theme);
                if response.clicked() {
                    self.pending_command = Some("figure.reset_part");
                }
                if crate::figure::limb_of(body, joint).is_some() {
                    let name = if pinned {
                        tr!("unpin-joint")
                    } else {
                        tr!("pin-joint")
                    };
                    let response = ui::button("figure.pin", &name).show(ui, theme);
                    let response = ui::tooltip(response, &Tip::new(&tr!("pin-joint-help")), theme);
                    if response.clicked() {
                        self.pending_command = Some("figure.pin");
                    }
                }
            });
        }
    }

    /// The hand setup: the pad (open at the top, a fist at the lower
    /// left, a flat fold at the lower right), the spread, and a lock for
    /// each finger. Returns the pose it makes of `pose` when it was
    /// moved, and whether it was let go.
    pub(super) fn hand_setup_rows(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        pose: &Pose,
        body: &Body,
    ) -> Option<(Pose, bool)> {
        if !matches!(body.kind, Kind::Hand | Kind::Body) {
            return None;
        }
        let mut setup = self.hand_setup;
        let mut slid = Slid::default();
        ui.label(
            egui::RichText::new(tr!("hand-setup"))
                .font(theme.text_font(TextRole::Caption))
                .color(theme.text_muted()),
        );
        if body.kind == Kind::Body {
            let sides = [tr!("left-hand"), tr!("right-hand"), tr!("both-hands")];
            let picked = ui::segmented("sue-paint-hand-setup-side", &sides, Some(self.hand_side))
                .full_width()
                .show(ui, theme);
            if let Some(side) = picked.picked {
                self.hand_side = side;
            }
        }
        slid |= hand_pad(ui, theme, &mut setup);
        slid |= slide(
            ui,
            theme,
            "sue-paint-hand-spread",
            &tr!("finger-spread"),
            &mut setup.spread,
            -1.0..=1.0,
            "",
            2,
        );
        ui.horizontal_wrapped(|ui| {
            for (finger, name) in [
                tr!("lock-thumb"),
                tr!("lock-index"),
                tr!("lock-middle"),
                tr!("lock-ring"),
                tr!("lock-little"),
            ]
            .iter()
            .enumerate()
            {
                // A lock changes nothing by itself: it keeps a finger
                // as it is the next time the pad moves.
                check(
                    ui,
                    theme,
                    ("sue-paint-hand-lock", finger),
                    name,
                    &mut setup.locks[finger],
                );
            }
        });
        self.hand_setup = setup;
        if !slid.changed {
            return slid.released.then(|| (pose.clone(), true));
        }
        let next = if body.kind == Kind::Hand {
            crate::figure::hand_setup(body, pose, &setup)
        } else {
            let mut next = pose.clone();
            for (index, side) in [Side::Left, Side::Right].into_iter().enumerate() {
                if self.hand_side != 2 && self.hand_side != index {
                    continue;
                }
                let hand = crate::figure::hand_from_body(body, &next, side);
                let shaped = crate::figure::hand_setup(body, &hand, &setup);
                crate::figure::hand_onto_body(body, &mut next, side, &shaped);
            }
            next
        };
        Some((next, slid.released))
    }
}

/// The hand setup's pad: a triangle with the open hand at its top, the
/// fist at its lower left and the flat fold at its lower right; the dot
/// is how the fingers are closed. Dragging or clicking sets it.
fn hand_pad(ui: &mut Ui, theme: &Theme, setup: &mut HandSetup) -> Slid {
    let density = Density::of(ui);
    let side = ui
        .available_width()
        .min(density.control_height() * 5.0)
        .max(density.control_height() * 3.0);
    let label = theme.text_size(TextRole::Caption) + theme.space(Space::S2);
    // An even sided triangle is 0.87 of its side tall; a label above
    // and one below.
    let tall = side * 0.87 + label + label;
    let (outer, _) = ui.allocate_exact_size(vec2(ui.available_width(), tall), Sense::hover());
    let top = pos2(outer.center().x, outer.top() + label);
    let left = pos2(outer.center().x - side / 2.0, top.y + side * 0.87);
    let right = pos2(outer.center().x + side / 2.0, top.y + side * 0.87);
    let rect = egui::Rect::from_min_max(pos2(left.x, top.y), right);
    let id = ui.scope_id().with("sue-paint-hand-pad");
    let response = ui.interact(rect, id, Sense::click_and_drag());
    let name = tr!("hand-setup");
    let response = ui::tooltip(
        response,
        &Tip::new(&name).help(&tr!("hand-setup-help")),
        theme,
    );
    let mut slid = Slid::default();
    if let Some(pointer) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked() || response.drag_started())
    {
        // Where the pointer is between the three corners.
        let area =
            |a: Pos2, b: Pos2, c: Pos2| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        let whole = area(top, left, right);
        let mut fist = (area(top, pointer, right) / whole).max(0.0);
        let mut fold = (area(top, left, pointer) / whole).max(0.0);
        let total = fist + fold;
        if total > 1.0 {
            fist /= total;
            fold /= total;
        }
        if (fist, fold) != (setup.fist, setup.fold) {
            setup.fist = fist;
            setup.fold = fold;
            slid.changed = true;
        }
    }
    slid.released |= response.drag_stopped() || response.clicked();
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::accesskit::Role::Slider, ui.is_enabled(), &name)
    });
    if ui.is_rect_visible(outer) {
        let painter = ui.painter();
        painter.add(egui::Shape::convex_polygon(
            vec![top, right, left],
            theme.surface(2),
            egui::Stroke::new(theme.border_width, theme.border_strong()),
        ));
        let font = theme.text_font(TextRole::Caption);
        let ink = theme.text_muted();
        let gap = theme.space(Space::S1);
        for (text, at, align) in [
            (
                tr!("hand-open"),
                pos2(top.x, top.y - gap),
                egui::Align2::CENTER_BOTTOM,
            ),
            (
                tr!("hand-fist"),
                pos2(left.x, left.y + gap),
                egui::Align2::LEFT_TOP,
            ),
            (
                tr!("hand-fold"),
                pos2(right.x, right.y + gap),
                egui::Align2::RIGHT_TOP,
            ),
        ] {
            painter.text(at, align, text, font.clone(), ink);
        }
        let open = (1.0 - setup.fist - setup.fold).max(0.0);
        let dot = pos2(
            top.x * open + left.x * setup.fist + right.x * setup.fold,
            top.y * open + left.y * setup.fist + right.y * setup.fold,
        );
        painter.circle_filled(dot, theme.space(Space::S3), theme.text);
    }
    slid
}
