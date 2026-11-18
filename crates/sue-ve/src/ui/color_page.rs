//! The Color page, laid out as DaVinci Resolve's is: the selected clip's
//! correctors (Resolve's serial nodes) at the left, the viewer (with a
//! before and after split) in the middle and the scopes at the right; a
//! strip of the clips under them; and along the bottom the corrector's
//! controls: the lift, gamma, gain and offset wheels with contrast, pivot,
//! saturation, temperature and tint; the custom curves; the hue curves;
//! the qualifier; and the LUT.

use egui::{Align2, Pos2, Rect, Sense, Ui, pos2, vec2};
use sue_core::Time;
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, Space, TextRole};
use sue_wm::{AppletContext, Theme};

use super::{SQUARE, controls, font, monitor, pool::texture_key};
use crate::grade::{self, Corrector, Qualifier};
use crate::model;
use crate::scopes::Scope;
use crate::studio::SueVe;

/// The viewer row's share of the page's height.
const TOP_SHARE: f32 = 0.52;
/// The corrector list's share of the width.
const NODES_SHARE: f32 = 0.16;
/// The scopes' share of the width.
const SCOPES_SHARE: f32 = 0.3;

/// Draws the Color page in `body`.
pub fn color_page(
    studio: &mut SueVe,
    ui: &mut Ui,
    body: Rect,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let strip_height = Density::Regular.row() * 2.0;
    let top_bottom = body.top() + body.height() * TOP_SHARE;
    let nodes = Rect::from_min_max(
        body.min,
        pos2(body.left() + body.width() * NODES_SHARE, top_bottom),
    );
    let scopes = Rect::from_min_max(
        pos2(body.right() - body.width() * SCOPES_SHARE, body.top()),
        pos2(body.right(), top_bottom),
    );
    let viewer = Rect::from_min_max(
        pos2(nodes.right(), body.top()),
        pos2(scopes.left(), top_bottom),
    );
    let strip = Rect::from_min_max(
        pos2(body.left(), top_bottom),
        pos2(body.right(), top_bottom + strip_height),
    );
    let controls = Rect::from_min_max(pos2(body.left(), strip.bottom()), body.max);
    for rect in [nodes, scopes, strip, controls] {
        ui.painter().rect_filled(rect, SQUARE, theme.panel);
        ui.painter()
            .rect_stroke(rect, SQUARE, theme.hairline(), egui::StrokeKind::Inside);
    }
    monitor::program_monitor(studio, ui, viewer, cx, theme);
    clips_strip(studio, ui, strip, cx, theme);
    scopes_ui(studio, ui, scopes, theme, cx);
    let Some((_, clip)) = studio.selected_clip(cx) else {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(controls));
        sue_wm::ui::empty_state("sve-color-empty", &tr!("color-pick-a-clip"))
            .icon(Icon::PaletteColorWheel)
            .show(&mut child, theme);
        return;
    };
    let mut list = grade::grade_of(&clip);
    nodes_ui(studio, ui, nodes, &list, cx, theme);
    if list.is_empty() {
        list.push(Corrector::default());
    }
    let index = studio.color.corrector.min(list.len() - 1);
    let mut corrector = list[index].clone();
    let before = corrector.clone();
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(controls.shrink(theme.space(Space::S3)))
            .id_salt("sve-color-controls"),
    );
    child.set_clip_rect(controls);
    let (changed, done) = controls_ui(studio, &mut child, &mut corrector, cx, theme);
    if changed && corrector != before {
        list[index] = corrector;
        let id = clip.id;
        studio.live_clip(cx, id, |clip| grade::set_grade(clip, &list));
    }
    if done {
        studio.commit_live(cx, &tr!("color-grade"));
    }
}

/// The correctors of the clip, as a column of nodes: a click picks one,
/// its switch bypasses it; the buttons add one or reset the grade.
fn nodes_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    rect: Rect,
    list: &[Corrector],
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink(theme.space(Space::S3)))
            .id_salt("sve-nodes"),
    );
    let ui = &mut child;
    sue_wm::ui::heading(ui, &tr!("color-nodes"), theme);
    let width = ui.available_width();
    let height = Density::Regular.row();
    let mut toggle = None;
    for (index, corrector) in list.iter().enumerate() {
        let (place, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
        let picked = studio.color.corrector == index;
        ui.painter().rect_filled(
            place.shrink(theme.space(Space::S1)),
            theme.corner(sue_wm::ui::Radius::Sm),
            if picked {
                theme.fill_selected()
            } else {
                theme.surface(1)
            },
        );
        ui.painter().text(
            pos2(place.left() + theme.space(Space::S4), place.center().y),
            Align2::LEFT_CENTER,
            format!("{:02}", index + 1),
            font(theme, TextRole::Mono),
            if corrector.enabled {
                theme.text
            } else {
                theme.text_muted()
            },
        );
        let switch = Rect::from_min_size(
            pos2(place.right() - height, place.top()),
            vec2(height, height),
        );
        if sue_wm::ui::icon_button(
            ("sve-node-on", index),
            if corrector.enabled {
                Icon::Eye
            } else {
                Icon::EyeOff
            },
            &tr!("color-node-on"),
        )
        .quiet()
        .show_at(ui, switch.shrink(theme.space(Space::S1)), theme)
        .clicked()
        {
            toggle = Some(index);
        } else if response.clicked() {
            studio.color.corrector = index;
        }
    }
    ui.add_space(theme.space(Space::S2));
    let mut run = None;
    ui.horizontal_wrapped(|ui| {
        if sue_wm::ui::icon_button("sve-node-add", Icon::Plus, &tr!("cmd-add-corrector"))
            .show(ui, theme)
            .clicked()
        {
            run = Some("color.add_corrector");
        }
        if sue_wm::ui::icon_button("sve-node-reset", Icon::Reset, &tr!("cmd-reset-grade"))
            .show(ui, theme)
            .clicked()
        {
            run = Some("color.reset_grade");
        }
        if sue_wm::ui::icon_button("sve-node-lut", Icon::Import, &tr!("cmd-import-lut"))
            .show(ui, theme)
            .clicked()
        {
            run = Some("file.import_lut");
        }
        if sue_wm::ui::icon_button(
            "sve-node-split",
            Icon::ViewGridSmall,
            &tr!("cmd-split-before-after"),
        )
        .selected(studio.color.split)
        .show(ui, theme)
        .clicked()
        {
            run = Some("color.split");
        }
    });
    if let Some(index) = toggle
        && let Some((_, clip)) = studio.selected_clip(cx)
    {
        studio.change_clip(cx, &tr!("color-node-on"), clip.id, |clip| {
            let mut list = grade::grade_of(clip);
            if let Some(corrector) = list.get_mut(index) {
                corrector.enabled = !corrector.enabled;
            }
            grade::set_grade(clip, &list);
            Ok(())
        });
    }
    if let Some(command) = run {
        studio.run(command, cx);
    }
}

/// The clips on the timeline's video tracks, in order, as Resolve's clip
/// strip: a click selects one and moves the playhead into it.
fn clips_strip(
    studio: &mut SueVe,
    ui: &mut Ui,
    rect: Rect,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let Some(sequence) = studio.current(cx).cloned() else {
        return;
    };
    let mut clips: Vec<&sue_model::Clip> = sequence
        .tracks
        .iter()
        .filter(|track| model::is_video(track))
        .flat_map(|track| track.clips())
        .collect();
    clips.sort_by_key(|clip| clip.start);
    let tile =
        vec2(rect.height() * 16.0 / 9.0, rect.height()).min(vec2(rect.width(), rect.height()));
    let mut x = rect.left() + theme.space(Space::S2);
    let mut picked = None;
    for (index, clip) in clips.iter().enumerate() {
        let place = Rect::from_min_size(pos2(x, rect.top()), tile).shrink(theme.space(Space::S1));
        x += tile.x;
        if place.left() > rect.right() {
            break;
        }
        let response = ui.interact(
            place,
            egui::Id::unique(("sve-color-clip", clip.id)),
            Sense::click(),
        );
        let selected = studio.selection.contains(&clip.id);
        ui.painter().rect_filled(
            place,
            theme.corner(sue_wm::ui::Radius::Xs),
            theme.surface(2),
        );
        super::pool::picture(
            studio,
            ui,
            place.shrink(theme.space(Space::S1)),
            clip.source,
            cx,
            theme,
        );
        ui.painter().text(
            place.left_top() + vec2(theme.space(Space::S1), 0.0),
            Align2::LEFT_TOP,
            format!("{:02}", index + 1),
            font(theme, TextRole::Micro),
            theme.text,
        );
        if !grade::grade_of(clip).is_empty() {
            // A dot in the corner: this clip is graded.
            let dot = theme.space(Space::S2);
            ui.painter().circle_filled(
                place.right_top() + vec2(-dot * 1.5, dot * 1.5),
                dot / 2.0,
                theme.focus(),
            );
        }
        ui.painter().rect_stroke(
            place,
            theme.corner(sue_wm::ui::Radius::Xs),
            if selected {
                theme.selected_stroke()
            } else {
                theme.hairline()
            },
            egui::StrokeKind::Inside,
        );
        if response.clicked() {
            picked = Some((
                clip.id,
                clip.start + Time::from_seconds(clip.duration.seconds() / 2.0),
            ));
        }
    }
    if let Some((id, at)) = picked {
        studio.selection = vec![id];
        studio.color.corrector = 0;
        let now = cx.view.now;
        studio.clock.seek(at, now, cx.sink());
    }
}

/// The scope picker and the scope of the viewer's frame, worked out on
/// the scopes' thread.
fn scopes_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    rect: Rect,
    theme: &Theme,
    cx: &mut AppletContext<'_>,
) {
    let bar = Density::Compact.icon_button() + theme.space(Space::S2);
    let top = Rect::from_min_size(rect.min, vec2(rect.width(), bar)).shrink(theme.space(Space::S1));
    let names: Vec<String> = Scope::ALL.iter().map(|scope| scope.label()).collect();
    let chosen = Scope::ALL
        .iter()
        .position(|scope| *scope == studio.color.scope);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(top));
    let answer = sue_wm::ui::segmented("sve-scope", &names, chosen).show(&mut child, theme);
    if let Some(picked) = answer.picked {
        studio.color.scope = Scope::ALL[picked];
    }
    let area = Rect::from_min_max(pos2(rect.left(), rect.top() + bar), rect.max)
        .shrink(theme.space(Space::S3));
    let painter = ui.painter_at(area);
    painter.rect_filled(area, SQUARE, theme.void);
    if let Some(frame) = studio.shown.clone() {
        studio.scopes.want(studio.color.scope, &frame);
    }
    let Some((_, scope, picture)) = studio.scopes.latest() else {
        return;
    };
    // The graticule: levels for the waveforms, the circle for chroma.
    let inner = if scope == Scope::Vectorscope {
        let side = area.width().min(area.height());
        Rect::from_center_size(area.center(), vec2(side, side))
    } else {
        area
    };
    match scope {
        Scope::Vectorscope => {
            painter.circle_stroke(inner.center(), inner.width() * 0.45, theme.hairline());
            painter.hline(inner.x_range(), inner.center().y, theme.hairline());
            painter.vline(inner.center().x, inner.y_range(), theme.hairline());
            // Skin tones run along this line (Resolve's skin line).
            let angle = 123_f32.to_radians();
            let reach = inner.width() * 0.45;
            painter.line_segment(
                [
                    inner.center(),
                    inner.center() + vec2(-angle.cos(), -angle.sin()) * reach,
                ],
                theme.hairline_active(),
            );
        }
        _ => {
            for step in 0..=4 {
                let y = inner.bottom() - inner.height() * step as f32 / 4.0;
                painter.hline(inner.x_range(), y, theme.hairline());
                painter.text(
                    pos2(inner.left() + theme.space(Space::S1), y),
                    Align2::LEFT_BOTTOM,
                    format!("{}", step * 256 / 4),
                    font(theme, TextRole::Micro),
                    theme.text_muted(),
                );
            }
        }
    }
    let key = texture_key(("sve-scope", scope.id()));
    if let Some(texture) = cx.textures.texture(
        key,
        picture.serial,
        [picture.width, picture.height],
        4,
        &picture.rgba,
        false,
    ) {
        painter.image(
            texture,
            inner,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
}

/// The corrector's controls under the viewer, by tab. Returns whether
/// it changed and whether a change ended.
fn controls_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    corrector: &mut Corrector,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) -> (bool, bool) {
    let tabs = [
        tr!("color-tab-primaries"),
        tr!("color-tab-curves"),
        tr!("color-tab-hue-curves"),
        tr!("color-tab-qualifier"),
        tr!("color-tab-lut"),
    ];
    let answer =
        sue_wm::ui::segmented("sve-color-tab", &tabs, Some(studio.color.tab)).show(ui, theme);
    if let Some(picked) = answer.picked {
        studio.color.tab = picked;
    }
    ui.add_space(theme.space(Space::S2));
    let mut changed = false;
    let mut done = false;
    match studio.color.tab {
        0 => primaries(ui, corrector, &mut changed, &mut done, theme),
        1 => {
            ui.horizontal(|ui| {
                let side = ((ui.available_width() - theme.space(Space::S4) * 4.0) / 4.0)
                    .min(ui.available_height() - theme.space(Space::S6))
                    .max(48.0);
                for (index, name) in [
                    tr!("color-master"),
                    tr!("color-red"),
                    tr!("color-green"),
                    tr!("color-blue"),
                ]
                .iter()
                .enumerate()
                {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(name).font(font(theme, TextRole::Caption)));
                        let (c, d) = controls::curve_box(
                            ui,
                            ("sve-grade-curve", index),
                            &mut corrector.curves[index],
                            side,
                            theme,
                        );
                        changed |= c;
                        done |= d;
                    });
                }
            });
        }
        2 => {
            ui.horizontal(|ui| {
                let side = ((ui.available_width() - theme.space(Space::S4) * 3.0) / 3.0)
                    .min(ui.available_height() - theme.space(Space::S6))
                    .max(48.0);
                for (index, name) in [
                    tr!("color-hue-vs-hue"),
                    tr!("color-hue-vs-sat"),
                    tr!("color-hue-vs-lum"),
                ]
                .iter()
                .enumerate()
                {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(name).font(font(theme, TextRole::Caption)));
                        let curve = match index {
                            0 => &mut corrector.hue_hue,
                            1 => &mut corrector.hue_sat,
                            _ => &mut corrector.hue_lum,
                        };
                        let (c, d) =
                            hue_curve_box(ui, ("sve-hue-curve", index), curve, side, theme);
                        changed |= c;
                        done |= d;
                    });
                }
            });
        }
        3 => qualifier(ui, corrector, &mut changed, &mut done, theme),
        _ => lut(studio, ui, corrector, &mut changed, &mut done, cx, theme),
    }
    (changed, done)
}

/// A slider row: label, track and field. Sets `changed` and `done`.
#[expect(
    clippy::too_many_arguments,
    reason = "a slider's place, name, value, range and the flags it sets"
)]
fn slide(
    ui: &mut Ui,
    salt: &str,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
    changed: &mut bool,
    done: &mut bool,
    theme: &Theme,
) {
    let mut wide = f64::from(*value);
    let response = sue_wm::ui::slider(salt, &mut wide, range)
        .label(label)
        .decimals(decimals)
        .show(ui, theme);
    if response.changed() {
        *value = wide as f32;
        *changed = true;
        if !response.dragged() {
            *done = true;
        }
    }
    if response.drag_stopped() || response.lost_focus() {
        *done = true;
    }
}

/// The four wheels at the left and the primary sliders beside them.
fn primaries(
    ui: &mut Ui,
    corrector: &mut Corrector,
    changed: &mut bool,
    done: &mut bool,
    theme: &Theme,
) {
    let room = ui.available_rect_before_wrap();
    let gap = theme.space(Space::S4);
    // Over a wheel its name, under it its master slider.
    let extra = theme.text_size(TextRole::Caption) + Density::Compact.row() + gap;
    let wheels_width = room.width() * 0.66;
    let side = ((wheels_width - gap * 3.0) / 4.0)
        .min(room.height() - extra)
        .max(48.0);
    let sliders = Rect::from_min_max(pos2(room.left() + (side + gap) * 4.0, room.top()), room.max);
    ui.horizontal(|ui| {
        for (index, name) in [
            tr!("color-lift"),
            tr!("color-gamma"),
            tr!("color-gain"),
            tr!("color-offset"),
        ]
        .iter()
        .enumerate()
        {
            let wheel = match index {
                0 => &mut corrector.lift,
                1 => &mut corrector.gamma,
                2 => &mut corrector.gain,
                _ => &mut corrector.offset,
            };
            let (c, d) = wheel_ui(ui, index, name, wheel, side, theme);
            *changed |= c;
            *done |= d;
        }
    });
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(sliders)
            .id_salt("sve-primary-sliders"),
    );
    let column = &mut child;
    for (salt, label, value, range, decimals) in [
        (
            "sve-contrast",
            tr!("color-contrast"),
            &mut corrector.contrast,
            0.0..=2.0,
            2,
        ),
        (
            "sve-pivot",
            tr!("color-pivot"),
            &mut corrector.pivot,
            0.0..=1.0,
            3,
        ),
        (
            "sve-saturation",
            tr!("color-saturation"),
            &mut corrector.saturation,
            0.0..=2.0,
            2,
        ),
        (
            "sve-temperature",
            tr!("color-temperature"),
            &mut corrector.temperature,
            -100.0..=100.0,
            0,
        ),
        (
            "sve-tint",
            tr!("color-tint"),
            &mut corrector.tint,
            -100.0..=100.0,
            0,
        ),
    ] {
        slide(
            column, salt, &label, value, range, decimals, changed, done, theme,
        );
    }
}

/// The chroma a wheel's red, green and blue offsets point at, as a point
/// in the wheel (x right, y up), and back.
#[must_use]
pub fn wheel_point(rgb: [f32; 3]) -> [f32; 2] {
    let x = (2.0 * rgb[0] - rgb[1] - rgb[2]) / 3.0;
    let y = (rgb[1] - rgb[2]) / 3.0_f32.sqrt();
    [x, y]
}

/// The red, green and blue offsets (summing to nothing) of a point in a
/// wheel.
#[must_use]
pub fn wheel_offsets([x, y]: [f32; 2]) -> [f32; 3] {
    let s3 = 3.0_f32.sqrt();
    [x, -x / 2.0 + y * s3 / 2.0, -x / 2.0 - y * s3 / 2.0]
}

/// How far the wheel's edge reaches, as an offset.
const WHEEL_REACH: f32 = 0.25;

/// One color wheel: its puck dragged sets the balance, its master slider
/// under it the level.
fn wheel_ui(
    ui: &mut Ui,
    index: usize,
    name: &str,
    wheel: &mut [f32; 4],
    side: f32,
    theme: &Theme,
) -> (bool, bool) {
    let mut changed = false;
    let mut done = false;
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(name).font(font(theme, TextRole::Caption)));
        let (rect, response) = ui.allocate_exact_size(vec2(side, side), Sense::click_and_drag());
        let center = rect.center();
        // The hue ring's dots stay inside the wheel's square.
        let radius = (rect.width().min(rect.height()) / 2.0 - theme.space(Space::S1)) / 1.07;
        let painter = ui.painter_at(rect);
        // The hue ring, faint, so the puck's direction reads.
        for step in 0..36 {
            let angle = step as f32 / 36.0 * std::f32::consts::TAU;
            let rgb = grade::from_hsl([
                1.0 - (angle / std::f32::consts::TAU + 0.25).rem_euclid(1.0),
                0.6,
                0.5,
            ]);
            let color = egui::Color32::from_rgb(
                (rgb[0] * 255.0) as u8,
                (rgb[1] * 255.0) as u8,
                (rgb[2] * 255.0) as u8,
            );
            let at = center + vec2(angle.cos(), -angle.sin()) * radius;
            painter.circle_filled(at, radius * 0.06, color.gamma_multiply(0.6));
        }
        painter.circle_stroke(center, radius, theme.hairline());
        painter.line_segment(
            [center - vec2(radius, 0.0), center + vec2(radius, 0.0)],
            theme.hairline(),
        );
        painter.line_segment(
            [center - vec2(0.0, radius), center + vec2(0.0, radius)],
            theme.hairline(),
        );
        let [x, y] = wheel_point([wheel[0], wheel[1], wheel[2]]);
        let puck: Pos2 = center + vec2(x, -y) / WHEEL_REACH * radius;
        painter.circle_filled(puck, radius * 0.1, theme.focus());
        if response.dragged()
            && let Some(at) = response.interact_pointer_pos()
        {
            let mut d = (at - center) / radius;
            if d.length() > 1.0 {
                d /= d.length();
            }
            let rgb = wheel_offsets([d.x * WHEEL_REACH, -d.y * WHEEL_REACH]);
            wheel[0] = rgb[0];
            wheel[1] = rgb[1];
            wheel[2] = rgb[2];
            changed = true;
        }
        if response.double_clicked() {
            *wheel = [0.0, 0.0, 0.0, wheel[3]];
            changed = true;
            done = true;
        }
        if response.drag_stopped() {
            done = true;
        }
        let mut master = f64::from(wheel[3]);
        let slider = sue_wm::ui::slider(("sve-wheel-master", index), &mut master, -1.0..=1.0)
            .name(name)
            .decimals(2)
            .width(side)
            .show(ui, theme);
        if slider.changed() {
            wheel[3] = master as f32;
            changed = true;
            if !slider.dragged() {
                done = true;
            }
        }
        if slider.drag_stopped() || slider.lost_focus() {
            done = true;
        }
    });
    (changed, done)
}

/// A hue curve: points across the hue (x) dragged up or down from the
/// middle; a double click adds one, a right click takes one away.
fn hue_curve_box(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    points: &mut Vec<[f32; 2]>,
    side: f32,
    theme: &Theme,
) -> (bool, bool) {
    let (rect, response) =
        ui.allocate_exact_size(vec2(side * 1.6, side * 0.7), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    // The hues along the bottom.
    let columns = 48;
    for column in 0..columns {
        let h = column as f32 / columns as f32;
        let rgb = grade::from_hsl([h, 0.7, 0.5]);
        let x0 = rect.left() + rect.width() * h;
        let band = Rect::from_min_max(
            pos2(x0, rect.bottom() - theme.space(Space::S2)),
            pos2(x0 + rect.width() / columns as f32 + 1.0, rect.bottom()),
        );
        painter.rect_filled(
            band,
            0.0,
            egui::Color32::from_rgb(
                (rgb[0] * 255.0) as u8,
                (rgb[1] * 255.0) as u8,
                (rgb[2] * 255.0) as u8,
            ),
        );
    }
    painter.rect_stroke(rect, 0.0, theme.hairline(), egui::StrokeKind::Inside);
    painter.hline(rect.x_range(), rect.center().y, theme.hairline());
    let to_screen = |[x, y]: [f32; 2]| {
        pos2(
            rect.left() + x * rect.width(),
            rect.bottom() - y * rect.height(),
        )
    };
    let line: Vec<Pos2> = (0..=64)
        .map(|step| {
            let h = step as f32 / 64.0;
            to_screen([h, grade::hue_curve(points, h)])
        })
        .collect();
    painter.add(egui::Shape::line(
        line,
        egui::Stroke::new(Theme::STRONG_STROKE, theme.text),
    ));
    let id = ui.scope_id().with(salt);
    let mut held: Option<usize> = ui.data(|data| data.get_temp(id)).flatten();
    let reach = Density::Compact.icon() * 0.6;
    let near = |at: Pos2, points: &[[f32; 2]]| {
        points
            .iter()
            .enumerate()
            .map(|(index, point)| (index, to_screen(*point).distance(at)))
            .filter(|(_, d)| *d <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index)
    };
    let mut changed = false;
    if response.drag_started()
        && let Some(at) = response.interact_pointer_pos()
    {
        held = near(at, points);
    }
    if response.dragged()
        && let (Some(index), Some(at)) = (held, response.interact_pointer_pos())
        && let Some(point) = points.get_mut(index)
    {
        *point = [
            ((at.x - rect.left()) / rect.width()).clamp(0.0, 1.0),
            ((rect.bottom() - at.y) / rect.height()).clamp(0.0, 1.0),
        ];
        changed = true;
    }
    let mut done = response.drag_stopped() && held.is_some();
    if response.drag_stopped() {
        held = None;
    }
    if response.double_clicked()
        && let Some(at) = response.interact_pointer_pos()
    {
        let h = ((at.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        points.push([h, grade::hue_curve(points, h)]);
        changed = true;
        done = true;
    }
    if response.secondary_clicked()
        && let Some(at) = response.interact_pointer_pos()
        && let Some(index) = near(at, points)
    {
        points.remove(index);
        changed = true;
        done = true;
    }
    ui.data_mut(|data| data.insert_temp(id, held));
    for point in points.iter() {
        painter.rect_filled(
            Rect::from_center_size(to_screen(*point), vec2(reach, reach)),
            0.0,
            theme.focus(),
        );
    }
    (changed, done)
}

/// The qualifier: on, its hue, saturation and luminance ranges, softness
/// and inversion.
fn qualifier(
    ui: &mut Ui,
    corrector: &mut Corrector,
    changed: &mut bool,
    done: &mut bool,
    theme: &Theme,
) {
    let mut on = corrector.qualifier.is_some();
    if sue_wm::ui::switch("sve-qualifier-on", &tr!("color-qualifier-on"), on)
        .show(ui, theme)
        .clicked()
    {
        on = !on;
        corrector.qualifier = on.then(Qualifier::default);
        *changed = true;
        *done = true;
    }
    let Some(q) = corrector.qualifier.as_mut() else {
        ui.label(
            egui::RichText::new(tr!("color-qualifier-note"))
                .font(font(theme, TextRole::Caption))
                .color(theme.text_muted()),
        );
        return;
    };
    ui.columns(2, |columns| {
        slide(
            &mut columns[0],
            "sve-q-hue",
            &tr!("color-q-hue"),
            &mut q.hue,
            0.0..=1.0,
            3,
            changed,
            done,
            theme,
        );
        slide(
            &mut columns[0],
            "sve-q-hue-width",
            &tr!("color-q-hue-width"),
            &mut q.hue_width,
            0.0..=1.0,
            3,
            changed,
            done,
            theme,
        );
        slide(
            &mut columns[0],
            "sve-q-softness",
            &tr!("color-q-softness"),
            &mut q.softness,
            0.0..=0.5,
            3,
            changed,
            done,
            theme,
        );
        slide(
            &mut columns[1],
            "sve-q-sat-low",
            &tr!("color-q-sat-low"),
            &mut q.saturation[0],
            0.0..=1.0,
            2,
            changed,
            done,
            theme,
        );
        slide(
            &mut columns[1],
            "sve-q-sat-high",
            &tr!("color-q-sat-high"),
            &mut q.saturation[1],
            0.0..=1.0,
            2,
            changed,
            done,
            theme,
        );
        slide(
            &mut columns[1],
            "sve-q-lum-low",
            &tr!("color-q-lum-low"),
            &mut q.luminance[0],
            0.0..=1.0,
            2,
            changed,
            done,
            theme,
        );
        slide(
            &mut columns[1],
            "sve-q-lum-high",
            &tr!("color-q-lum-high"),
            &mut q.luminance[1],
            0.0..=1.0,
            2,
            changed,
            done,
            theme,
        );
    });
    let mut invert = q.invert;
    if sue_wm::ui::switch("sve-q-invert", &tr!("color-q-invert"), invert)
        .show(ui, theme)
        .clicked()
    {
        invert = !invert;
        q.invert = invert;
        *changed = true;
        *done = true;
    }
}

/// The LUT: picked from the pool's LUTs, with how much of it.
fn lut(
    studio: &mut SueVe,
    ui: &mut Ui,
    corrector: &mut Corrector,
    changed: &mut bool,
    done: &mut bool,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let luts: Vec<(sue_core::EntityId, String)> = studio
        .project
        .map(|project| {
            model::pool(cx.arena(), project)
                .into_iter()
                .filter_map(|(id, _)| {
                    let entity = cx.arena().get_by_id(id).ok()?;
                    (entity.class() == grade::LUT_CLASS).then(|| (id, entity.name().to_owned()))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut names = vec![tr!("color-no-lut")];
    names.extend(luts.iter().map(|(_, name)| name.clone()));
    let mut chosen = corrector
        .lut
        .and_then(|id| luts.iter().position(|(found, _)| *found == id))
        .map_or(0, |at| at + 1);
    let before = chosen;
    controls::labeled(ui, &tr!("color-lut"), theme, |ui| {
        sue_wm::ui::dropdown("sve-lut", &mut chosen, &names).show(ui, theme);
    });
    if chosen != before {
        corrector.lut = chosen
            .checked_sub(1)
            .and_then(|at| luts.get(at))
            .map(|(id, _)| *id);
        *changed = true;
        *done = true;
    }
    slide(
        ui,
        "sve-lut-mix",
        &tr!("color-lut-mix"),
        &mut corrector.lut_mix,
        0.0..=1.0,
        2,
        changed,
        done,
        theme,
    );
    if sue_wm::ui::button("sve-lut-import", &tr!("cmd-import-lut"))
        .icon(Icon::Import)
        .show(ui, theme)
        .clicked()
    {
        studio.run("file.import_lut", cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wheel_point_and_its_offsets_agree() {
        for point in [[0.1, 0.0], [0.0, 0.2], [-0.15, 0.05]] {
            let back = wheel_point(wheel_offsets(point));
            assert!((back[0] - point[0]).abs() < 1e-5 && (back[1] - point[1]).abs() < 1e-5);
        }
        let red = wheel_offsets([0.2, 0.0]);
        assert!(red[0] > 0.0 && red[1] < 0.0 && red[2] < 0.0);
        assert!(
            (red.iter().sum::<f32>()).abs() < 1e-6,
            "chroma only, no level"
        );
    }
}
