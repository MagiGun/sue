//! The 3D rows of Tool Property, as Clip Studio Paint lays them out for
//! the Object tool on a 3D layer: the object (its place, angle and size,
//! moved by dragging buttons), the camera (turned, moved and brought
//! nearer the same way, its presets and Manga Perspective), the light
//! source, the outline, and for a drawing figure its body shape (type,
//! height, head to body ratio and the body shape chart) and pose. Also
//! the Object tool's settings for a 2D camera folder.

use egui::{Pos2, Rect, Sense, Ui, Vec2, pos2, vec2};
use sue_wm::Theme;
use sue_wm::icons::Icon;
use sue_wm::ui::{self, Density, Radius, Space, TextRole, Tip};

use crate::studio::SuePaint;

/// The commands that turn the object to [`crate::three::ANGLES`], in
/// order.
const ANGLE_COMMANDS: [&str; 6] = [
    "figure.object.angle.0",
    "figure.object.angle.1",
    "figure.object.angle.2",
    "figure.object.angle.3",
    "figure.object.angle.4",
    "figure.object.angle.5",
];

/// How far one press of an arrow key on a drag button drags, in pixels.
const KEY_DRAG: f32 = 12.0;

/// How far the object moves for a pixel of drag, in its own units.
const MOVE_PER_PIXEL: f32 = 0.005;

/// What a drag button reports this frame: how far it was dragged (or
/// stepped with the arrow keys) and whether it was let go.
#[derive(Clone, Copy, Debug, Default)]
struct Dragged {
    delta: Vec2,
    released: bool,
}

/// A slider's change this frame: whether it changed and whether it was
/// let go (or set by a key or the field, which is a let go at once).
#[derive(Clone, Copy, Debug, Default)]
struct Slid {
    changed: bool,
    released: bool,
}

impl std::ops::BitOrAssign for Slid {
    fn bitor_assign(&mut self, other: Self) {
        self.changed |= other.changed;
        self.released |= other.released;
    }
}

/// A slider of `value` in `range`, named `name` (written before it and in
/// its tooltip), showing `suffix` after the number.
#[allow(clippy::too_many_arguments)]
fn slide(
    ui: &mut Ui,
    theme: &Theme,
    salt: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    decimals: usize,
) -> Slid {
    let mut wide = f64::from(*value);
    let response = ui::slider(
        salt,
        &mut wide,
        f64::from(*range.start())..=f64::from(*range.end()),
    )
    .label(name)
    .suffix(suffix)
    .decimals(decimals)
    .show(ui, theme);
    let response = ui::tooltip(response, &Tip::new(name), theme);
    if response.changed() {
        *value = wide as f32;
    }
    // A change while the pointer is held is a drag (of the track or of
    // the number field); it is kept when the pointer lets go, which
    // `view3d_properties` sees.
    Slid {
        changed: response.changed(),
        released: response.drag_stopped() || (response.changed() && !held(ui)),
    }
}

/// `true` while a pointer button is held anywhere.
fn held(ui: &Ui) -> bool {
    ui.input(|input| input.pointer.any_down())
}

/// A checkbox that says `name`, with a tooltip; `true` when clicked (and
/// `on` flipped).
fn check(
    ui: &mut Ui,
    theme: &Theme,
    salt: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    on: &mut bool,
) -> bool {
    let response = ui::checkbox(salt, name, *on).show(ui, theme);
    let response = ui::tooltip(response, &Tip::new(name), theme);
    if response.clicked() {
        *on = !*on;
    }
    response.clicked()
}

/// One drag button of a [`drag_row`]: its id, icon, the word it shows
/// and the sentence its tooltip says.
struct DragSpec {
    salt: &'static str,
    icon: Icon,
    label: String,
    name: String,
}

/// Three drag buttons side by side, sharing the row's width, each with
/// its icon and a word (what each one does reads at a glance; the
/// tooltip says it in full).
fn drag_row(ui: &mut Ui, theme: &Theme, specs: [DragSpec; 3]) -> [Dragged; 3] {
    let gap = ui.spacing().item_spacing.x;
    let height = Density::of(ui).control_height();
    let count = specs.len() as f32;
    let width = ((ui.available_width() - gap * (count - 1.0)) / count).max(height);
    let across = width * count + gap * (count - 1.0);
    let (row, _) = ui.allocate_exact_size(vec2(across, height), Sense::hover());
    let mut left = row.left();
    specs.map(|spec| {
        let rect = Rect::from_min_size(pos2(left, row.top()), vec2(width, height));
        left += width + gap;
        drag_button(ui, theme, rect, &spec)
    })
}

/// A drag button at `rect`: drags what it names while the pointer holds
/// it, and steps it with the arrow keys while it has focus (Clip Studio
/// Paint's camera and object buttons above the model).
fn drag_button(ui: &mut Ui, theme: &Theme, rect: Rect, spec: &DragSpec) -> Dragged {
    let salt = spec.salt;
    // The drag is sensed under the button, which takes the clicks, the
    // focus and the hover on top of it.
    let drag = ui.interact(rect, ui.scope_id().with((salt, "drag")), Sense::drag());
    let button = ui::toggle_button(salt, &spec.label, drag.dragged())
        .icon(spec.icon)
        .show_at(ui, rect, theme);
    let button = ui::tooltip(
        button,
        &Tip::new(&spec.name).help(&tr!("drag-a-button-or-use-arrow-keys")),
        theme,
    );
    let mut delta = if drag.dragged() {
        drag.drag_delta()
    } else {
        Vec2::ZERO
    };
    let mut stepped = false;
    if button.has_focus() {
        ui.input(|input| {
            for (key, by) in [
                (egui::Key::ArrowLeft, -Vec2::X * KEY_DRAG),
                (egui::Key::ArrowRight, Vec2::X * KEY_DRAG),
                (egui::Key::ArrowUp, -Vec2::Y * KEY_DRAG),
                (egui::Key::ArrowDown, Vec2::Y * KEY_DRAG),
            ] {
                if input.key_pressed(key) {
                    delta += by;
                    stepped = true;
                }
            }
        });
    }
    Dragged {
        delta,
        released: drag.drag_stopped() || stepped,
    }
}

/// The body shape chart: thin to fat across, less to more muscle up, a
/// dot where `value` is. Dragging or clicking sets it; the arrow keys step
/// it while it has focus. Returns what it did this frame.
fn shape_chart(ui: &mut Ui, theme: &Theme, salt: usize, value: &mut [f32; 2]) -> Slid {
    let density = Density::of(ui);
    let label_height = theme.text_size(TextRole::Caption) + theme.space(Space::S2);
    let side = ui
        .available_width()
        .min(density.control_height() * 5.0)
        .max(density.control_height() * 3.0);
    let (outer, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), side + label_height + label_height),
        Sense::hover(),
    );
    let rect = Rect::from_center_size(outer.center(), Vec2::splat(side));
    let id = ui.scope_id().with(("sue-paint-figure-chart", salt));
    let response = ui.interact(rect, id, Sense::click_and_drag());
    let name = tr!("body-shape-chart");
    let response = ui::tooltip(
        response,
        &Tip::new(&name).help(&tr!("body-shape-chart-help")),
        theme,
    );
    let mut slid = Slid::default();
    if let Some(pointer) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked() || response.drag_started())
    {
        let at = [
            ((pointer.x - rect.center().x) / (side / 2.0)).clamp(-1.0, 1.0),
            ((rect.center().y - pointer.y) / (side / 2.0)).clamp(-1.0, 1.0),
        ];
        if at != *value {
            *value = at;
            slid.changed = true;
        }
    }
    slid.released |= response.drag_stopped() || response.clicked();
    if response.has_focus() {
        let step = 0.05;
        ui.input(|input| {
            for (key, axis, by) in [
                (egui::Key::ArrowLeft, 0, -step),
                (egui::Key::ArrowRight, 0, step),
                (egui::Key::ArrowDown, 1, -step),
                (egui::Key::ArrowUp, 1, step),
            ] {
                if input.key_pressed(key) {
                    value[axis] = (value[axis] + by).clamp(-1.0, 1.0);
                    slid.changed = true;
                    slid.released = true;
                }
            }
        });
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::accesskit::Role::Slider, ui.is_enabled(), &name)
    });
    if ui.is_rect_visible(outer) {
        let painter = ui.painter();
        let radius = theme.radius(Radius::Md);
        let line = egui::Stroke::new(theme.border_width, theme.border);
        painter.rect_filled(rect, radius, theme.surface(2));
        painter.rect_stroke(
            rect,
            radius,
            egui::Stroke::new(theme.border_width, theme.border_strong()),
            egui::StrokeKind::Inside,
        );
        painter.line_segment(
            [
                pos2(rect.center().x, rect.top()),
                pos2(rect.center().x, rect.bottom()),
            ],
            line,
        );
        painter.line_segment(
            [
                pos2(rect.left(), rect.center().y),
                pos2(rect.right(), rect.center().y),
            ],
            line,
        );
        let font = theme.text_font(TextRole::Caption);
        let ink = theme.text_muted();
        let gap = theme.space(Space::S1);
        for (text, at, align) in [
            (
                tr!("muscle"),
                pos2(rect.center().x, rect.top() - gap),
                egui::Align2::CENTER_BOTTOM,
            ),
            (
                tr!("less"),
                pos2(rect.center().x, rect.bottom() + gap),
                egui::Align2::CENTER_TOP,
            ),
            (
                tr!("thin"),
                pos2(rect.left() + gap * 2.0, rect.center().y - gap),
                egui::Align2::LEFT_BOTTOM,
            ),
            (
                tr!("fat"),
                pos2(rect.right() - gap * 2.0, rect.center().y - gap),
                egui::Align2::RIGHT_BOTTOM,
            ),
        ] {
            painter.text(at, align, text, font.clone(), ink);
        }
        let dot = Pos2::new(
            rect.center().x + value[0] * side / 2.0,
            rect.center().y - value[1] * side / 2.0,
        );
        painter.circle_filled(dot, theme.space(Space::S3), theme.text);
        painter.circle_stroke(
            dot,
            theme.space(Space::S3),
            egui::Stroke::new(theme.border_width, theme.surface(2)),
        );
        ui::focus_ring(ui, &response, radius, theme);
    }
    slid
}

impl SuePaint {
    /// A text button that runs `command`, with a tooltip naming it and its
    /// key as Settings has it.
    fn three_command_button(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        command: &'static str,
        name: &str,
    ) {
        let key = self
            .bound
            .first_chord(crate::commands::shortcut_table(), command);
        let response = ui::button(command, name).show(ui, theme);
        let response = ui::tooltip(response, &Tip::new(name).shortcut(&key), theme);
        if response.clicked() {
            self.pending_command = Some(command);
        }
    }

    /// An icon button that runs `command`, as [`Self::command_button`].
    fn command_icon(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        command: &'static str,
        icon: Icon,
        name: &str,
    ) {
        let key = self
            .bound
            .first_chord(crate::commands::shortcut_table(), command);
        let response = ui::icon_button(command, icon, name).show(ui, theme);
        // The icon button names itself; the key goes in its own tip.
        let response = if key.is_empty() {
            response
        } else {
            ui::tooltip(response, &Tip::new(name).shortcut(&key), theme)
        };
        if response.clicked() {
            self.pending_command = Some(command);
        }
    }

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
        let gap = theme.space(Space::S4);
        ui.spacing_mut().item_spacing.y = theme.space(Space::S2);
        let now = crate::three::View3D::of(entity);
        let mut view = now;
        let mut slid = Slid::default();
        let model =
            crate::three::model_of(arena, layer).and_then(|model| arena.get_by_id(model).ok());
        let place_now = model.map(crate::three::Placement::of);
        let mut place = place_now;
        if let Some(place) = place.as_mut() {
            slid |= self.object_rows(ui, theme, &view, place);
            ui.add_space(gap);
        }
        slid |= self.camera_rows(ui, theme, layer, &mut view);
        ui.add_space(gap);
        slid |= light_rows(ui, theme, &mut view);
        ui.add_space(gap);
        slid |= outline_rows(ui, theme, &mut view);
        let figure = model.filter(|model| crate::figure::is_figure(model));
        let mut shape: Option<(crate::figure::Body, crate::figure::Body)> = None;
        if let Some(model) = figure {
            let body = crate::figure::Body::of(model);
            ui.add_space(gap);
            // A hand or a head has a size but no type, ratio or chart.
            if body.kind == crate::figure::Kind::Body {
                let mut shaped = body;
                slid |= self.body_rows(ui, theme, &mut shaped);
                shape = Some((body, shaped));
            }
            ui.add_space(gap);
            self.figure_properties(
                ui,
                layer,
                &crate::figure::Pose::of(model),
                &body,
                arena,
                theme,
            );
        }
        ui.add_space(gap);
        ui.label(
            egui::RichText::new(tr!("drag-canvas-turn-camera-shift-moves"))
                .font(theme.text_font(TextRole::Caption))
                .color(theme.text_muted()),
        );
        let place_changed = place != place_now;
        let shape_changed = shape.is_some_and(|(was, now)| was != now);
        if view != now || place_changed || shape_changed {
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
            if let Some(place) = place.filter(|_| place_changed) {
                self.pending_place = Some((layer, place));
            }
            if let Some((_, shaped)) = shape.filter(|_| shape_changed) {
                self.pending_shape = Some((layer, shaped));
            }
            self.pending_view3d = Some((layer, view));
        }
        // Kept when let go: by a control that says so, or as soon as no
        // pointer is held any more (a drag that ended outside its
        // control, a number field dragged).
        if self.view3d_before.is_some() && (slid.released || !held(ui)) {
            self.view3d_release = true;
        }
    }

    /// The object: buttons that move, turn and slide it along the ground
    /// while dragged; its angle (presets, the turns, Reset); its position
    /// (Put on the ground, Reset); its scale (Reset).
    fn object_rows(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        view: &crate::three::View3D,
        place: &mut crate::three::Placement,
    ) -> Slid {
        let mut slid = Slid::default();
        ui::section(ui, theme, &tr!("section-object-3d"), |ui| {
            let [move_by, turn_by, slide_by] = drag_row(
                ui,
                theme,
                [
                    DragSpec {
                        salt: "sue-paint-3d-move",
                        icon: Icon::ToolMoveLayer,
                        label: tr!("drag-move"),
                        name: tr!("move-object"),
                    },
                    DragSpec {
                        salt: "sue-paint-3d-turn",
                        icon: Icon::RotateRight,
                        label: tr!("drag-rotate"),
                        name: tr!("turn-object"),
                    },
                    DragSpec {
                        salt: "sue-paint-3d-slide",
                        icon: Icon::SnapGrid,
                        label: tr!("drag-slide"),
                        name: tr!("slide-object"),
                    },
                ],
            );
            // Across the screen and up, or across and away along the
            // ground: the camera's right, and its forward flattened.
            let yaw = view.yaw.to_radians();
            let right = [yaw.cos(), 0.0, -yaw.sin()];
            let away = [-yaw.sin(), 0.0, -yaw.cos()];
            for (dragged, along) in [(move_by, false), (slide_by, true)] {
                if dragged.delta != Vec2::ZERO {
                    let (dx, dy) = (
                        dragged.delta.x * MOVE_PER_PIXEL,
                        dragged.delta.y * MOVE_PER_PIXEL,
                    );
                    for axis in [0, 2] {
                        place.position[axis] += right[axis] * dx;
                        if along {
                            place.position[axis] -= away[axis] * dy;
                        }
                    }
                    if !along {
                        place.position[1] -= dy;
                    }
                    slid.changed = true;
                }
                slid.released |= dragged.released;
            }
            if turn_by.delta != Vec2::ZERO {
                place.turn[0] = wrap(place.turn[0] + turn_by.delta.x * 0.5);
                place.turn[1] = (place.turn[1] + turn_by.delta.y * 0.5).clamp(-180.0, 180.0);
                slid.changed = true;
            }
            slid.released |= turn_by.released;
            // The angle.
            ui.horizontal(|ui| {
                let names: Vec<String> = std::iter::once(tr!("choose-an-angle"))
                    .chain(
                        crate::three::ANGLES
                            .iter()
                            .map(|(name, _)| crate::localize(name)),
                    )
                    .collect();
                let mut chosen = 0;
                let response = ui::dropdown("sue-paint-3d-angle", &mut chosen, &names)
                    .label(&tr!("object-angle"))
                    .show(ui, theme);
                let response = ui::tooltip(response, &Tip::new(&tr!("object-angle")), theme);
                if response.changed()
                    && let Some(command) =
                        chosen.checked_sub(1).and_then(|at| ANGLE_COMMANDS.get(at))
                {
                    self.pending_command = Some(command);
                }
                self.command_icon(
                    ui,
                    theme,
                    "figure.object.reset_angle",
                    Icon::Reset,
                    &tr!("reset-angle"),
                );
            });
            for (axis, name) in [tr!("turn-round"), tr!("turn-lean"), tr!("turn-tilt")]
                .iter()
                .enumerate()
            {
                slid |= slide(
                    ui,
                    theme,
                    ("sue-paint-3d-turn", axis),
                    name,
                    &mut place.turn[axis],
                    -180.0..=180.0,
                    &tr!("deg"),
                    0,
                );
            }
            // The position.
            for (axis, name) in [tr!("position-x"), tr!("position-y"), tr!("position-z")]
                .iter()
                .enumerate()
            {
                slid |= slide(
                    ui,
                    theme,
                    ("sue-paint-3d-position", axis),
                    name,
                    &mut place.position[axis],
                    -10.0..=10.0,
                    "",
                    2,
                );
            }
            ui.horizontal(|ui| {
                self.three_command_button(ui, theme, "figure.object.ground", &tr!("put-on-ground"));
                self.command_icon(
                    ui,
                    theme,
                    "figure.object.reset_position",
                    Icon::Reset,
                    &tr!("reset-position"),
                );
            });
            // The size, in percent.
            ui.horizontal(|ui| {
                let mut percent = place.scale * 100.0;
                let range = crate::three::Placement::SCALES;
                let room =
                    ui.available_width() - Density::of(ui).icon_button() - theme.space(Space::S2);
                let mut wide = f64::from(percent);
                let response = ui::slider(
                    "sue-paint-3d-scale",
                    &mut wide,
                    f64::from(*range.start() * 100.0)..=f64::from(*range.end() * 100.0),
                )
                .label(&tr!("object-scale"))
                .suffix(&tr!("percent"))
                .logarithmic()
                .width(room)
                .show(ui, theme);
                let response = ui::tooltip(response, &Tip::new(&tr!("object-scale")), theme);
                if response.changed() {
                    percent = wide as f32;
                    place.scale = (percent / 100.0).clamp(*range.start(), *range.end());
                    slid.changed = true;
                }
                slid.released |= response.drag_stopped() || (response.changed() && !held(ui));
                self.command_icon(
                    ui,
                    theme,
                    "figure.object.reset_scale",
                    Icon::Reset,
                    &tr!("reset-scale"),
                );
            });
        });
        slid
    }

    /// The camera: buttons that turn it, move it and bring it nearer while
    /// dragged; its presets; its angle, height, distance and perspective;
    /// Manga Perspective.
    fn camera_rows(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        layer: sue_core::EntityId,
        view: &mut crate::three::View3D,
    ) -> Slid {
        let mut slid = Slid::default();
        ui::section(ui, theme, &tr!("section-camera"), |ui| {
            let [turn, pan, zoom] = drag_row(
                ui,
                theme,
                [
                    DragSpec {
                        salt: "sue-paint-3d-orbit",
                        icon: Icon::ToolRotateView,
                        label: tr!("drag-orbit"),
                        name: tr!("turn-camera"),
                    },
                    DragSpec {
                        salt: "sue-paint-3d-pan",
                        icon: Icon::ToolHand,
                        label: tr!("drag-pan"),
                        name: tr!("pan-camera"),
                    },
                    DragSpec {
                        salt: "sue-paint-3d-zoom",
                        icon: Icon::ToolZoom,
                        label: tr!("drag-zoom"),
                        name: tr!("zoom-camera"),
                    },
                ],
            );
            // As the canvas drags them (canvas/drag.rs, Gesture::Orbit).
            if turn.delta != Vec2::ZERO {
                view.yaw = wrap(view.yaw - turn.delta.x * 0.4);
                view.pitch = (view.pitch + turn.delta.y * 0.4).clamp(-89.0, 89.0);
                slid.changed = true;
            }
            if pan.delta != Vec2::ZERO {
                let scale = view.distance / 600.0;
                view.pan = [
                    view.pan[0] - pan.delta.x * scale,
                    view.pan[1] + pan.delta.y * scale,
                ];
                slid.changed = true;
            }
            if zoom.delta.y != 0.0 {
                view.distance = (view.distance * (zoom.delta.y / 200.0).exp()).clamp(0.6, 100.0);
                slid.changed = true;
            }
            slid.released |= turn.released || pan.released || zoom.released;
            ui.label(
                egui::RichText::new(tr!("drag-a-button-or-use-arrow-keys"))
                    .font(theme.text_font(TextRole::Caption))
                    .color(theme.text_muted()),
            );
            let names: Vec<String> = std::iter::once(tr!("choose-a-view"))
                .chain(
                    crate::three::CAMERAS
                        .iter()
                        .map(|(name, _, _)| crate::localize(name)),
                )
                .collect();
            let mut chosen = 0;
            let response = ui::dropdown(("sue-paint-3d-camera", layer), &mut chosen, &names)
                .label(&tr!("camera-view"))
                .show(ui, theme);
            let response = ui::tooltip(response, &Tip::new(&tr!("camera-view")), theme);
            if response.changed()
                && let Some(index) = chosen.checked_sub(1)
            {
                *view = view.preset(index);
                slid.changed = true;
                slid.released = true;
            }
            slid |= slide(
                ui,
                theme,
                "sue-paint-3d-yaw",
                &tr!("camera-angle"),
                &mut view.yaw,
                -180.0..=180.0,
                &tr!("deg"),
                0,
            );
            slid |= slide(
                ui,
                theme,
                "sue-paint-3d-pitch",
                &tr!("camera-height"),
                &mut view.pitch,
                -89.0..=89.0,
                &tr!("deg"),
                0,
            );
            slid |= slide(
                ui,
                theme,
                "sue-paint-3d-distance",
                &tr!("distance"),
                &mut view.distance,
                0.6..=30.0,
                "",
                1,
            );
            slid |= slide(
                ui,
                theme,
                "sue-paint-3d-fov",
                &tr!("perspective"),
                &mut view.fov,
                5.0..=120.0,
                &tr!("deg"),
                0,
            );
            let mut manga = view.manga > 0.0;
            if check(
                ui,
                theme,
                "sue-paint-3d-manga",
                &tr!("manga-perspective"),
                &mut manga,
            ) {
                view.manga = if manga { 0.5 } else { 0.0 };
                slid.changed = true;
                slid.released = true;
            }
            if view.manga > 0.0 {
                slid |= slide(
                    ui,
                    theme,
                    "sue-paint-3d-manga-strength",
                    &tr!("manga-strength"),
                    &mut view.manga,
                    0.01..=1.0,
                    "",
                    2,
                );
            }
        });
        slid
    }

    /// A drawing figure's body shape, as Clip Studio Paint's: its type,
    /// height and head to body ratio (which may follow the height), the
    /// body part and the body shape chart, Initialize, and Register body
    /// shape as material.
    fn body_rows(&mut self, ui: &mut Ui, theme: &Theme, body: &mut crate::figure::Body) -> Slid {
        use crate::figure::{Body, Build, PARTS};
        let mut slid = Slid::default();
        ui::section(ui, theme, &tr!("body-shape"), |ui| {
            let types: Vec<String> = Build::ALL
                .iter()
                .map(|build| crate::localize(build.name()))
                .collect();
            let chosen = Build::ALL.iter().position(|build| *build == body.build);
            let picked = ui::segmented("sue-paint-figure-build", &types, chosen)
                .full_width()
                .show(ui, theme);
            let response = ui::tooltip(picked.response, &Tip::new(&tr!("body-type")), theme);
            drop(response);
            if let Some(build) = picked.picked.and_then(|at| Build::ALL.get(at)) {
                body.build = *build;
                slid.changed = true;
                slid.released = true;
            }
            let mut height = body.height;
            let moved = slide(
                ui,
                theme,
                "sue-paint-figure-height",
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
            slid |= slide(
                ui,
                theme,
                "sue-paint-figure-heads",
                &tr!("head-to-body-ratio"),
                &mut body.heads,
                Body::RATIOS,
                "",
                1,
            );
            if check(
                ui,
                theme,
                "sue-paint-figure-follow",
                &tr!("adjust-head-to-body-ratio-with-height"),
                &mut body.follow,
            ) {
                if body.follow {
                    body.heads = Body::heads_for(body.height);
                }
                slid.changed = true;
                slid.released = true;
            }
            let parts: Vec<String> = PARTS.iter().map(|part| crate::localize(part)).collect();
            let mut part = self.figure_part.min(PARTS.len() - 1);
            let response = ui::dropdown("sue-paint-figure-part", &mut part, &parts)
                .label(&tr!("body-part"))
                .show(ui, theme);
            let response = ui::tooltip(response, &Tip::new(&tr!("body-part")), theme);
            if response.changed() {
                self.figure_part = part;
            }
            slid |= shape_chart(ui, theme, part, &mut body.shape.0[part]);
            ui.horizontal(|ui| {
                self.three_command_button(
                    ui,
                    theme,
                    "figure.body.initialize",
                    &tr!("initialize-body-shape"),
                );
                self.three_command_button(
                    ui,
                    theme,
                    "figure.body.register",
                    &tr!("register-body-shape"),
                );
            });
        });
        slid
    }

    /// A drawing figure's pose, as Clip Studio Paint's: a preset pose,
    /// Flip pose and Reset pose, Register pose as material, the scanners,
    /// and the picked joint's turns.
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
        let mut next: Option<crate::figure::Pose> = None;
        let mut released = false;
        ui::section(ui, theme, &tr!("pose"), |ui| {
            let names: Vec<String> = std::iter::once(tr!("choose-a-pose"))
                .chain(body.presets().iter().map(|name| crate::localize(name)))
                .collect();
            let mut chosen = 0;
            let response = ui::dropdown(("sue-paint-figure-preset", layer), &mut chosen, &names)
                .label(&tr!("pose"))
                .show(ui, theme);
            let response = ui::tooltip(response, &Tip::new(&tr!("pose")), theme);
            if response.changed()
                && let Some(index) = chosen.checked_sub(1)
            {
                next = Some(body.preset(index));
                released = true;
            }
            ui.horizontal(|ui| {
                self.command_icon(
                    ui,
                    theme,
                    "figure.flip",
                    Icon::FlipHorizontal,
                    &tr!("flip-pose"),
                );
                self.command_icon(ui, theme, "figure.reset", Icon::Reset, &tr!("reset-pose"));
                self.command_icon(
                    ui,
                    theme,
                    "figure.register_pose",
                    Icon::MaterialPose,
                    &tr!("register-pose-as-material"),
                );
            });
            // Pose Scanner and Hand Scanner: a photo poses it.
            match body.kind {
                crate::figure::Kind::Body => {
                    self.three_command_button(
                        ui,
                        theme,
                        "figure.pose_scanner",
                        &tr!("pose-scanner"),
                    );
                }
                crate::figure::Kind::Hand => {
                    self.three_command_button(
                        ui,
                        theme,
                        "figure.hand_scanner",
                        &tr!("hand-scanner"),
                    );
                }
                crate::figure::Kind::Head => {}
            }
            let names: Vec<String> = std::iter::once(tr!("none"))
                .chain(joints.iter().map(|joint| crate::localize(joint.name)))
                .collect();
            let mut chosen = self
                .figure_joint
                .filter(|joint| *joint < joints.len())
                .map_or(0, |joint| joint + 1);
            let response = ui::dropdown(("sue-paint-figure-joint", layer), &mut chosen, &names)
                .label(&tr!("joint"))
                .show(ui, theme);
            let response = ui::tooltip(response, &Tip::new(&tr!("joint")), theme);
            if response.changed() {
                self.figure_joint = chosen.checked_sub(1);
            }
            if let Some(joint) = self
                .figure_joint
                .filter(|joint| *joint < joints.len() && *joint < pose.0.len())
            {
                let mut turned = pose.clone();
                for (axis, name) in [tr!("bend"), tr!("twist"), tr!("swing")].iter().enumerate() {
                    let moved = slide(
                        ui,
                        theme,
                        ("sue-paint-figure-turn", axis),
                        name,
                        &mut turned.0[joint][axis],
                        -180.0..=180.0,
                        &tr!("deg"),
                        0,
                    );
                    released |= moved.released;
                }
                if turned != *pose {
                    next = Some(turned);
                }
            }
            ui.label(
                egui::RichText::new(tr!("drag-body-part-bend-shift-twists"))
                    .font(theme.text_font(TextRole::Caption))
                    .color(theme.text_muted()),
            );
        });
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
        if self.pose_before.is_some() && (released || !held(ui)) {
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
        let caption = |text: &str| {
            egui::RichText::new(text)
                .font(theme.text_font(TextRole::Caption))
                .color(theme.text_muted())
        };
        ui.spacing_mut().item_spacing.y = theme.space(Space::S2);
        let (frame, time) = self.frame_time(arena);
        let size = self
            .picture()
            .and_then(|picture| arena.get_by_id(picture).ok())
            .and_then(sue_model::picture::size)
            .unwrap_or([0, 0]);
        // The seven values as keyed or set here; a drag of the camera
        // frame on the canvas or a value being set shows what it makes.
        let mut values = crate::camera::values(arena, folder, Some(time), size);
        if let Some((pending, at, camera)) = self.camera_pending
            && pending == folder
            && at == frame
        {
            let center = camera.unswung([values[4] as f32, values[5] as f32]);
            values[0] = f64::from(center[0]);
            values[1] = f64::from(center[1]);
            values[2] = f64::from(camera.scale);
            values[3] = f64::from(camera.angle);
        }
        if let Some((pending, at, channel, value)) = self.camera_value
            && pending == folder
            && at == frame
            && let Some(index) = crate::camera::VALUES.iter().position(|key| *key == channel)
        {
            values[index] = value;
        }
        ui.label(caption(&tr!("camera-at-frame", value = frame + 1)));
        // The whole camera keyed at once, over the values each keyed alone.
        let mut command = None;
        let keys = crate::camera::keys(arena, folder);
        let keyed = keys.contains(&time);
        ui.horizontal(|ui| {
            let (name, run) = if keyed {
                (tr!("update-keyframe"), "anim.camera.key")
            } else {
                (tr!("add-keyframe"), "anim.camera.key")
            };
            let key = self
                .bound
                .first_chord(crate::commands::shortcut_table(), run);
            let response = ui::button("sue-paint-camera-key", &name).show(ui, theme);
            if ui::tooltip(response, &Tip::new(&name).shortcut(&key), theme).clicked() {
                command = Some(run);
            }
            if keyed {
                let name = tr!("delete-keyframe");
                let key = self
                    .bound
                    .first_chord(crate::commands::shortcut_table(), "anim.camera.unkey");
                let response = ui::button("sue-paint-camera-unkey", &name).show(ui, theme);
                if ui::tooltip(response, &Tip::new(&name).shortcut(&key), theme).clicked() {
                    command = Some("anim.camera.unkey");
                }
            }
        });
        // Each value as Clip Studio Paint's Tool Property lists them, how
        // it reads (times shown), its unit, its range, and the key button
        // beside it that keys it alone.
        let row = |channel, name, times, suffix, range, key| CameraRow {
            channel,
            name,
            times,
            suffix,
            range,
            key,
        };
        let any = f64::NEG_INFINITY..=f64::INFINITY;
        let rows = [
            row(
                crate::camera::X,
                tr!("position-x"),
                1.0,
                tr!("px"),
                any.clone(),
                "anim.camera.value_key.camera.x",
            ),
            row(
                crate::camera::Y,
                tr!("position-y"),
                1.0,
                tr!("px"),
                any.clone(),
                "anim.camera.value_key.camera.y",
            ),
            row(
                crate::camera::SCALE,
                tr!("scale"),
                100.0,
                tr!("percent"),
                1.0..=2000.0,
                "anim.camera.value_key.camera.scale",
            ),
            row(
                crate::camera::ANGLE,
                tr!("rotation"),
                180.0 / std::f64::consts::PI,
                tr!("deg"),
                any.clone(),
                "anim.camera.value_key.camera.angle",
            ),
            row(
                crate::camera::PIVOT_X,
                tr!("center-of-rotation-x"),
                1.0,
                tr!("px"),
                any.clone(),
                "anim.camera.value_key.camera.pivot.x",
            ),
            row(
                crate::camera::PIVOT_Y,
                tr!("center-of-rotation-y"),
                1.0,
                tr!("px"),
                any,
                "anim.camera.value_key.camera.pivot.y",
            ),
            row(
                crate::camera::OPACITY,
                tr!("layer-opacity"),
                100.0,
                tr!("percent"),
                0.0..=100.0,
                "anim.camera.value_key.camera.opacity",
            ),
        ];
        let mut set: Option<(&'static str, f64, bool)> = None;
        ui::form_grid(ui, "sue-paint-camera", theme, |grid| {
            for (index, row) in rows.iter().enumerate() {
                let CameraRow {
                    channel,
                    name,
                    times,
                    suffix,
                    range,
                    key: key_command,
                } = row;
                grid.row(name, |ui| {
                    let mut shown = values[index] * times;
                    let response = ui::number_field(("sue-paint-camera", *channel), &mut shown)
                        .label(name)
                        .suffix(suffix)
                        .range(range.clone())
                        .decimals(1)
                        .show(ui, theme);
                    let response = ui::tooltip(response, &Tip::new(name), theme);
                    // A drag of the field shows at once and is keyed once,
                    // when the pointer lets go; typing is keyed on Enter
                    // or when the field is left.
                    let finished = response.drag_stopped()
                        || response.lost_focus()
                        || (response.changed()
                            && !response.dragged()
                            && !response.has_focus()
                            && !response.is_pointer_button_down_on());
                    if response.changed() {
                        set = Some((*channel, shown / times, finished));
                    } else if finished {
                        set = Some((*channel, values[index], true));
                    }
                    let keyed = crate::camera::keyed(arena, folder, channel, time);
                    let tip = if keyed {
                        tr!("unkey-camera-value", name = name.as_str())
                    } else {
                        tr!("key-camera-value", name = name.as_str())
                    };
                    let response =
                        ui::icon_button(("sue-paint-camera-key", *channel), Icon::Keyframe, &tip)
                            .selected(keyed)
                            .show(ui, theme);
                    crate::launchers::note_palette_mark(
                        ui.ctx(),
                        &format!("camera.key.{channel}"),
                        response.rect.center(),
                    );
                    if ui::tooltip(response, &Tip::new(&tip), theme).clicked() {
                        command = Some(*key_command);
                    }
                });
            }
        });
        if let Some((channel, value, finished)) = set {
            let index = crate::camera::VALUES
                .iter()
                .position(|key| *key == channel)
                .unwrap_or(0);
            let editing = self.camera_value.is_some_and(|(pending, at, was, _)| {
                pending == folder && at == frame && was == channel
            });
            // A field left as it was keys nothing.
            if editing || (value - values[index]).abs() > f64::EPSILON {
                values[index] = value;
                self.camera_value = Some((folder, frame, channel, value));
                // The frame on the canvas follows at once.
                self.camera_pending =
                    Some((folder, frame, crate::camera::Camera::from_values(&values)));
                if finished && command.is_none() {
                    command = Some("anim.camera.value_set");
                }
            }
        }
        let rate = self
            .picture()
            .and_then(|picture| crate::anim::timeline(arena, picture))
            .map_or(sue_core::Rate::new(24, 1), |sequence| sequence.rate);
        let frames: Vec<String> = keys
            .iter()
            .map(|time| (time.frame(rate) + 1).to_string())
            .collect();
        ui.label(caption(&if frames.is_empty() {
            tr!("no-keyframes-yet-camera-shows-canvas")
        } else {
            tr!("keyframes-at-frames", value = frames.join(", "))
        }));
        ui.label(caption(&tr!("drag-inside-frame-move-camera-corner")));
        command
    }
}

/// A value of the 2D camera as Tool Property lists it: its channel, its
/// name, how many times the stored value it reads (degrees, percent), its
/// unit, its range, and the command of the key button beside it.
struct CameraRow {
    channel: &'static str,
    name: String,
    times: f64,
    suffix: String,
    range: std::ops::RangeInclusive<f64>,
    key: &'static str,
}

/// The light source: where the light comes from, how strong it is and how
/// much reaches the shade, whether it follows the camera, shadows, fog.
fn light_rows(ui: &mut Ui, theme: &Theme, view: &mut crate::three::View3D) -> Slid {
    let mut slid = Slid::default();
    ui::section(ui, theme, &tr!("light-source"), |ui| {
        slid |= slide(
            ui,
            theme,
            "sue-paint-3d-light",
            &tr!("light"),
            &mut view.light,
            -180.0..=180.0,
            &tr!("deg"),
            0,
        );
        slid |= slide(
            ui,
            theme,
            "sue-paint-3d-intensity",
            &tr!("light-intensity"),
            &mut view.intensity,
            0.0..=2.0,
            "",
            2,
        );
        slid |= slide(
            ui,
            theme,
            "sue-paint-3d-ambient",
            &tr!("ambient-light"),
            &mut view.ambient,
            0.0..=1.0,
            "",
            2,
        );
        // Clip Studio Paint 5.0 and 5.1: the light carried on the camera,
        // and fog rising from the foot.
        for (salt, name, on) in [
            (
                "sue-paint-3d-follow",
                tr!("light-follows-camera"),
                &mut view.follow_light,
            ),
            (
                "sue-paint-3d-shadows",
                tr!("cast-shadows"),
                &mut view.shadows,
            ),
        ] {
            if check(ui, theme, salt, &name, on) {
                slid.changed = true;
                slid.released = true;
            }
        }
        slid |= slide(
            ui,
            theme,
            "sue-paint-3d-fog",
            &tr!("fog"),
            &mut view.fog,
            0.0..=1.0,
            "",
            2,
        );
        if view.fog > 0.0 {
            slid |= slide(
                ui,
                theme,
                "sue-paint-3d-fog-height",
                &tr!("fog-height"),
                &mut view.fog_height,
                0.0..=1.0,
                "",
                2,
            );
        }
    });
    slid
}

/// The outline: how the model is drawn, the outline's width, where folds
/// draw lines, the lines' color and opacity.
fn outline_rows(ui: &mut Ui, theme: &Theme, view: &mut crate::three::View3D) -> Slid {
    let mut slid = Slid::default();
    ui::section(ui, theme, &tr!("outline"), |ui| {
        let styles: Vec<String> = crate::three::STYLES
            .iter()
            .map(|name| crate::localize(name))
            .collect();
        let mut style = usize::from(view.style.min(2));
        let response = ui::dropdown("sue-paint-3d-style", &mut style, &styles)
            .label(&tr!("draw-as"))
            .show(ui, theme);
        let response = ui::tooltip(response, &Tip::new(&tr!("draw-as")), theme);
        if response.changed() {
            view.style = style.min(2) as u8;
            slid.changed = true;
            slid.released = true;
        }
        if view.style > 0 {
            slid |= slide(
                ui,
                theme,
                "sue-paint-3d-line-width",
                &tr!("outline-width"),
                &mut view.line_width,
                0.5..=20.0,
                &tr!("px"),
                1,
            );
            slid |= slide(
                ui,
                theme,
                "sue-paint-3d-crease",
                &tr!("fold-lines-at"),
                &mut view.crease,
                5.0..=170.0,
                &tr!("deg"),
                0,
            );
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(tr!("line-color"))
                        .font(theme.text_font(TextRole::Caption))
                        .color(theme.text_muted()),
                );
                let response = ui.color_edit_button_srgb(&mut view.line_color);
                let response = ui::tooltip(response, &Tip::new(&tr!("line-color")), theme);
                // Kept when the pointer lets go of the picker (see
                // `view3d_properties`), so dragging in it is one step.
                slid.changed |= response.changed();
            });
            slid |= slide(
                ui,
                theme,
                "sue-paint-3d-line-opacity",
                &tr!("line-opacity"),
                &mut view.line_opacity,
                0.0..=1.0,
                "",
                2,
            );
        }
    });
    slid
}

/// `degrees` kept between -180 and 180.
fn wrap(degrees: f32) -> f32 {
    (degrees + 180.0).rem_euclid(360.0) - 180.0
}

#[cfg(test)]
mod tests {
    use super::wrap;

    #[test]
    fn turns_wrap_round() {
        assert_eq!(wrap(190.0), -170.0);
        assert_eq!(wrap(-190.0), 170.0);
        assert_eq!(wrap(45.0), 45.0);
    }
}
