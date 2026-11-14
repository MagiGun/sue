//! The 3D rows of Tool Property: the 3D view, figures and the camera.

use egui::{RichText, Ui, vec2};
use sue_wm::Theme;

use crate::studio::SuePaint;

impl SuePaint {
    pub(super) fn view3d_properties(
        &mut self,
        ui: &mut Ui,
        layer: sue_core::EntityId,
        arena: &sue_core::Arena,
        theme: &Theme,
    ) {
        let Ok(entity) = arena.get_by_id(layer) else {
            return;
        };
        let now = crate::three::View3D::of(entity);
        let mut view = now;
        let mut released = false;
        let slider = |ui: &mut Ui,
                      name: &str,
                      value: &mut f32,
                      range: std::ops::RangeInclusive<f32>,
                      suffix: &str| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).color(theme.muted));
                let response = ui.add(egui::Slider::new(value, range).suffix(suffix));
                response.drag_stopped() || (response.changed() && !response.dragged())
            })
            .inner
        };
        released |= slider(
            ui,
            &tr!("camera-angle"),
            &mut view.yaw,
            -180.0..=180.0,
            &tr!("deg"),
        );
        released |= slider(
            ui,
            &tr!("height-2"),
            &mut view.pitch,
            -89.0..=89.0,
            &tr!("deg"),
        );
        released |= slider(ui, &tr!("distance"), &mut view.distance, 0.6..=30.0, "");
        released |= slider(
            ui,
            &tr!("perspective"),
            &mut view.fov,
            5.0..=120.0,
            &tr!("deg"),
        );
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr!("camera-view")).color(theme.muted));
            egui::ComboBox::from_id_salt(("sue-paint-3d-camera", layer))
                .selected_text(tr!("choose-a-view"))
                .show_ui(ui, |ui| {
                    for (index, (name, _, _)) in crate::three::CAMERAS.iter().enumerate() {
                        if ui.selectable_label(false, crate::localize(name)).clicked() {
                            view = view.preset(index);
                            released = true;
                        }
                    }
                });
        });
        released |= slider(
            ui,
            &tr!("light"),
            &mut view.light,
            -180.0..=180.0,
            &tr!("deg"),
        );
        released |= slider(
            ui,
            &tr!("light-intensity"),
            &mut view.intensity,
            0.0..=2.0,
            "",
        );
        released |= slider(ui, &tr!("ambient-light"), &mut view.ambient, 0.0..=1.0, "");
        // Clip Studio Paint 5.0 and 5.1: the light carried on the camera,
        // and fog rising from the foot.
        released |= ui
            .checkbox(&mut view.follow_light, tr!("light-follows-camera"))
            .changed();
        released |= slider(ui, &tr!("fog"), &mut view.fog, 0.0..=1.0, "");
        if view.fog > 0.0 {
            released |= slider(ui, &tr!("fog-height"), &mut view.fog_height, 0.0..=1.0, "");
        }
        released |= ui
            .checkbox(&mut view.shadows, tr!("cast-shadows"))
            .changed();
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr!("draw-as")).color(theme.muted));
            egui::ComboBox::from_id_salt(("sue-paint-3d-style", layer))
                .selected_text(crate::localize(
                    crate::three::STYLES[usize::from(view.style.min(2))],
                ))
                .show_ui(ui, |ui| {
                    for (index, name) in crate::three::STYLES.iter().enumerate() {
                        if ui
                            .selectable_value(&mut view.style, index as u8, crate::localize(name))
                            .clicked()
                        {
                            released = true;
                        }
                    }
                });
        });
        if view.style > 0 {
            released |= slider(
                ui,
                &tr!("line-width-2"),
                &mut view.line_width,
                0.5..=20.0,
                &tr!("px"),
            );
            released |= slider(
                ui,
                &tr!("fold-lines-at"),
                &mut view.crease,
                5.0..=170.0,
                &tr!("deg"),
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr!("line-color")).color(theme.muted));
                released |= ui.color_edit_button_srgb(&mut view.line_color).changed();
            });
            released |= slider(
                ui,
                &tr!("line-opacity"),
                &mut view.line_opacity,
                0.0..=1.0,
                "",
            );
        }
        ui.label(
            RichText::new(tr!("drag-canvas-turn-camera-shift-moves"))
                .size(theme.small_size)
                .color(theme.muted),
        );
        if let Some(model) = crate::figure::of_layer(arena, layer)
            && let Ok(model) = arena.get_by_id(model)
        {
            let body = crate::figure::Body::of(model);
            // A hand or a head has a size but no head to body ratio.
            if body.kind == crate::figure::Kind::Body {
                self.figure_body(ui, layer, body, theme);
            }
            self.figure_properties(
                ui,
                layer,
                &crate::figure::Pose::of(model),
                &body,
                arena,
                theme,
            );
        }
        if view != now {
            if self
                .view3d_before
                .as_ref()
                .is_none_or(|(id, _, _)| *id != layer)
            {
                let before = crate::doc::raster(arena, layer)
                    .and_then(|raster| raster.bytes())
                    .map(<[u8]>::to_vec)
                    .unwrap_or_default();
                self.view3d_before = Some((layer, now, before));
            }
            self.pending_view3d = Some((layer, view));
        }
        if released {
            self.view3d_release = true;
        }
    }

    /// A drawing figure's body shape, as Clip Studio Paint's: its height and
    /// head to body ratio, kept as one step when let go.
    fn figure_body(
        &mut self,
        ui: &mut Ui,
        layer: sue_core::EntityId,
        now: crate::figure::Body,
        theme: &Theme,
    ) {
        use crate::figure::Body;
        ui.separator();
        ui.label(RichText::new(tr!("body-shape")).color(theme.text));
        let mut body = self
            .body_drag
            .filter(|(id, _)| *id == layer)
            .map_or(now, |(_, body)| body);
        let mut released = false;
        let mut dragging = false;
        for (name, value, range, suffix) in [
            (tr!("height-2"), &mut body.height, Body::HEIGHTS, tr!("cm")),
            (
                tr!("head-to-body-ratio"),
                &mut body.heads,
                Body::RATIOS,
                String::new(),
            ),
        ] {
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).color(theme.muted));
                let response = ui.add(egui::Slider::new(value, range).suffix(suffix));
                dragging |= response.dragged();
                released |= response.drag_stopped() || (response.changed() && !response.dragged());
            });
        }
        if ui.button(tr!("reset-body-shape")).clicked() {
            body = Body::default();
            released = true;
        }
        if released {
            self.body_drag = None;
            if body != now {
                self.pending_body = Some((layer, body));
            }
        } else if dragging {
            self.body_drag = Some((layer, body));
        }
    }

    /// A drawing figure's settings, as Clip Studio Paint's: a preset pose,
    /// Flip pose and Reset pose, and the picked joint's turns.
    fn figure_properties(
        &mut self,
        ui: &mut Ui,
        layer: sue_core::EntityId,
        pose: &crate::figure::Pose,
        body: &crate::figure::Body,
        arena: &sue_core::Arena,
        theme: &Theme,
    ) {
        let joints = body.joints();
        ui.separator();
        ui.label(RichText::new(tr!("drawing-figure")).color(theme.text));
        let mut next: Option<crate::figure::Pose> = None;
        let mut released = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr!("pose")).color(theme.muted));
            egui::ComboBox::from_id_salt(("sue-paint-figure-preset", layer))
                .selected_text(tr!("choose-a-pose"))
                .show_ui(ui, |ui| {
                    for (index, name) in body.presets().iter().enumerate() {
                        if ui.selectable_label(false, crate::localize(name)).clicked() {
                            next = Some(body.preset(index));
                            released = true;
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            if ui.button(tr!("flip-pose")).clicked() {
                next = Some(pose.flipped());
                released = true;
            }
            if ui.button(tr!("reset-pose")).clicked() {
                next = Some(body.standing());
                released = true;
            }
        });
        if ui.button(tr!("register-pose-as-material")).clicked() {
            self.pending_command = Some("figure.register_pose");
        }
        // Pose Scanner and Hand Scanner: a photo poses it.
        match body.kind {
            crate::figure::Kind::Body => {
                if ui
                    .button(tr!("pose-scanner"))
                    .on_hover_text(tr!("pose-scanner-hint"))
                    .clicked()
                {
                    self.pending_command = Some("figure.pose_scanner");
                }
            }
            crate::figure::Kind::Hand => {
                if ui
                    .button(tr!("hand-scanner"))
                    .on_hover_text(tr!("hand-scanner-hint"))
                    .clicked()
                {
                    self.pending_command = Some("figure.hand_scanner");
                }
            }
            crate::figure::Kind::Head => {}
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr!("joint")).color(theme.muted));
            let shown = self
                .figure_joint
                .and_then(|joint| joints.get(joint))
                .map_or_else(|| tr!("none"), |joint| crate::localize(joint.name));
            egui::ComboBox::from_id_salt(("sue-paint-figure-joint", layer))
                .selected_text(shown)
                .height(400.0)
                .show_ui(ui, |ui| {
                    for (index, joint) in joints.iter().enumerate() {
                        if ui
                            .selectable_label(
                                self.figure_joint == Some(index),
                                crate::localize(joint.name),
                            )
                            .clicked()
                        {
                            self.figure_joint = Some(index);
                        }
                    }
                });
        });
        if let Some(joint) = self
            .figure_joint
            .filter(|joint| *joint < joints.len() && *joint < pose.0.len())
        {
            let mut turned = pose.clone();
            for (axis, name) in [tr!("bend"), tr!("twist"), tr!("swing")].iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(name).color(theme.muted));
                    let response = ui.add(
                        egui::Slider::new(&mut turned.0[joint][axis], -180.0..=180.0)
                            .suffix(tr!("deg")),
                    );
                    if response.drag_stopped() || (response.changed() && !response.dragged()) {
                        released = true;
                    }
                });
            }
            if turned != *pose {
                next = Some(turned);
            }
        }
        ui.label(
            RichText::new(tr!("drag-body-part-bend-shift-twists"))
                .size(theme.small_size)
                .color(theme.muted),
        );
        if let Some(next) = next {
            if self
                .pose_before
                .as_ref()
                .is_none_or(|(id, _, _)| *id != layer)
            {
                let before = crate::doc::raster(arena, layer)
                    .and_then(|raster| raster.bytes())
                    .map(<[u8]>::to_vec)
                    .unwrap_or_default();
                self.pose_before = Some((layer, pose.clone(), before));
            }
            self.pending_pose = Some((layer, next));
        }
        if released {
            self.pose_release = true;
        }
    }

    /// The Object tool's settings for a camera folder: where its camera
    /// stands at this frame, and its keyframes.
    pub(super) fn camera_properties(
        &mut self,
        ui: &mut Ui,
        folder: sue_core::EntityId,
        arena: &sue_core::Arena,
        theme: &Theme,
    ) -> Option<&'static str> {
        let small = |text: &str| {
            RichText::new(text)
                .size(theme.small_size + 0.5)
                .color(theme.muted)
        };
        let (frame, time) = self.frame_time(arena);
        let mut camera = self.camera_now(arena, folder);
        let before = camera;
        ui.label(small(&tr!("camera-at-frame", value = frame + 1)));
        egui::Grid::new("sue-paint-camera")
            .num_columns(2)
            .spacing(vec2(10.0, 6.0))
            .show(ui, |ui| {
                ui.label(small(&tr!("position-x")));
                ui.add(egui::DragValue::new(&mut camera.center[0]).suffix(tr!("px")));
                ui.end_row();
                ui.label(small(&tr!("position-y")));
                ui.add(egui::DragValue::new(&mut camera.center[1]).suffix(tr!("px")));
                ui.end_row();
                ui.label(small(&tr!("scale")));
                let mut percent = camera.scale * 100.0;
                if ui
                    .add(
                        egui::DragValue::new(&mut percent)
                            .range(1.0..=2000.0)
                            .suffix(" %"),
                    )
                    .changed()
                {
                    camera.scale = percent / 100.0;
                }
                ui.end_row();
                ui.label(small(&tr!("rotation")));
                let mut degrees = camera.angle.to_degrees();
                if ui
                    .add(egui::DragValue::new(&mut degrees).suffix(tr!("deg")))
                    .changed()
                {
                    camera.angle = degrees.to_radians();
                }
                ui.end_row();
            });
        if camera != before {
            self.camera_pending = Some((folder, frame, camera));
        }
        let keys = crate::camera::keys(arena, folder);
        let keyed = keys.contains(&time);
        let mut command = None;
        ui.horizontal(|ui| {
            if ui
                .button(if keyed {
                    tr!("update-keyframe")
                } else {
                    tr!("add-keyframe")
                })
                .clicked()
            {
                command = Some("anim.camera.key");
            }
            if keyed && ui.button(tr!("delete-keyframe")).clicked() {
                command = Some("anim.camera.unkey");
            }
        });
        let rate = self
            .picture()
            .and_then(|picture| crate::anim::timeline(arena, picture))
            .map_or(sue_core::Rate::new(24, 1), |sequence| sequence.rate);
        let frames: Vec<String> = keys
            .iter()
            .map(|time| (time.frame(rate) + 1).to_string())
            .collect();
        ui.label(small(&if frames.is_empty() {
            tr!("no-keyframes-yet-camera-shows-canvas")
        } else {
            tr!("keyframes-at-frames", value = frames.join(", "))
        }));
        ui.label(small(&tr!("drag-inside-frame-move-camera-corner")));
        command
    }
}
