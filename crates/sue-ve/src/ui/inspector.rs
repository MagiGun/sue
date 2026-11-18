//! The inspector: the selected clip's settings, as Resolve's Inspector and
//! Premiere's Effect Controls show them. Transform (scale, position,
//! rotation) and opacity for pictures, volume and fades for sound, speed
//! for both, each keyframable at the playhead; a title's words, size and
//! color. A value dragged changes the clip live and becomes one undo step
//! when the drag ends.

use egui::{Ui, pos2, vec2};
use sue_core::{EntityId, Value};
use sue_model::Clip;
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, Space, TextRole};
use sue_wm::{AppletContext, ArenaEdit, Theme};

use super::font;
use crate::edit;
use crate::model::{self, SourceKind, key};
use crate::studio::SueVe;

/// One keyframable row: its key, label message, range, step, decimals and
/// the factor between what is stored and what is shown (opacity is
/// stored 0 to 1 and shown in percent).
struct Row {
    key: &'static str,
    label: &'static str,
    range: (f64, f64),
    speed: f64,
    decimals: usize,
    shown: f64,
    suffix: &'static str,
}

const PICTURE_ROWS: [Row; 5] = [
    Row {
        key: key::SCALE,
        label: "inspector-scale",
        range: (0.0, 1000.0),
        speed: 0.5,
        decimals: 1,
        shown: 100.0,
        suffix: " %",
    },
    Row {
        key: key::POS_X,
        label: "inspector-x",
        range: (-16_384.0, 16_384.0),
        speed: 1.0,
        decimals: 0,
        shown: 1.0,
        suffix: " px",
    },
    Row {
        key: key::POS_Y,
        label: "inspector-y",
        range: (-16_384.0, 16_384.0),
        speed: 1.0,
        decimals: 0,
        shown: 1.0,
        suffix: " px",
    },
    Row {
        key: key::ROTATION,
        label: "inspector-rotation",
        range: (-3600.0, 3600.0),
        speed: 0.5,
        decimals: 1,
        shown: 1.0,
        suffix: " deg",
    },
    Row {
        key: key::OPACITY,
        label: "inspector-opacity",
        range: (0.0, 100.0),
        speed: 0.5,
        decimals: 0,
        shown: 100.0,
        suffix: " %",
    },
];

const SOUND_ROWS: [Row; 1] = [Row {
    key: key::VOLUME,
    label: "inspector-volume",
    range: (-96.0, 24.0),
    speed: 0.1,
    decimals: 1,
    shown: 1.0,
    suffix: " dB",
}];

/// The share of a row its label takes.
const LABEL_SHARE: f32 = 0.38;

/// A labeled row: the label at the left, then what `add` draws.
fn labeled<R>(ui: &mut Ui, label: &str, theme: &Theme, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        let room = ui.available_width();
        let height = Density::Compact.icon_button();
        let (name, _) =
            ui.allocate_exact_size(vec2(room * LABEL_SHARE, height), egui::Sense::hover());
        ui.painter().text(
            pos2(name.left(), name.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            font(theme, TextRole::Caption),
            theme.text_muted(),
        );
        add(ui)
    })
    .inner
}

/// The inspector palette.
pub fn inspector_ui(studio: &mut SueVe, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
    let rect = ui.max_rect();
    let mut child =
        ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(theme.space(Space::S4))));
    let ui = &mut child;
    let Some(sequence) = studio.current(cx).cloned() else {
        return;
    };
    let clip = studio
        .selection
        .first()
        .and_then(|id| sequence.find(*id))
        .map(|(_, clip)| clip.clone());
    let Some(clip) = clip else {
        sequence_ui(studio, ui, &sequence, cx, theme);
        return;
    };
    let kind = cx
        .arena()
        .get_by_id(clip.source)
        .map_or(SourceKind::Unknown, SourceKind::of);
    let name = cx
        .arena()
        .get_by_id(clip.source)
        .map(|entity| entity.name().to_owned())
        .unwrap_or_default();
    egui::ScrollArea::vertical()
        .id_salt("sve-inspector")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            sue_wm::ui::heading(ui, &name, theme);
            let video = sequence
                .find(clip.id)
                .is_some_and(|(track, _)| model::is_video(&sequence.tracks[track]));
            let mut enabled = clip.enabled;
            if sue_wm::ui::switch("sve-inspector-enabled", &tr!("inspector-enabled"), enabled)
                .show(ui, theme)
                .clicked()
            {
                enabled = !enabled;
                let id = clip.id;
                studio.edit(cx, &tr!("cmd-enable"), |sequence, _| {
                    edit::set_enabled(sequence, &[id], enabled)
                });
            }
            if kind == SourceKind::Title {
                title_ui(studio, ui, clip.source, cx, theme);
            }
            if video {
                sue_wm::ui::list::heading(ui, &tr!("inspector-transform"), theme);
                for row in &PICTURE_ROWS {
                    row_ui(studio, ui, &clip, row, cx, theme);
                }
            } else {
                sue_wm::ui::list::heading(ui, &tr!("inspector-sound"), theme);
                for row in &SOUND_ROWS {
                    row_ui(studio, ui, &clip, row, cx, theme);
                }
                fades_ui(studio, ui, &clip, cx, theme);
            }
            sue_wm::ui::list::heading(ui, &tr!("inspector-speed"), theme);
            speed_ui(studio, ui, &clip, cx, theme);
        });
}

/// One keyframable value: its field and its keyframe button.
fn row_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    clip: &Clip,
    row: &Row,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let at = studio.clock.position;
    let inside = clip.start <= at && at < clip.end();
    let sample_at = if inside { at } else { clip.start };
    let stored = clip.value_at(row.key, sample_at, model::default_of(row.key));
    let mut value = stored * row.shown;
    let keyed = clip
        .animation
        .channel(row.key)
        .is_some_and(|curve| !curve.keys().is_empty());
    let source_time = clip.source_time(sample_at);
    let on_key = clip
        .animation
        .channel(row.key)
        .is_some_and(|curve| curve.keys().iter().any(|key| key.time == source_time));
    let id = clip.id;
    let name = row.key;
    let label = sue_lang::tr!("ve", row.label);
    let button = Density::Compact.icon_button();
    ui.horizontal(|ui| {
        let room = ui.available_width();
        let (spot, _) =
            ui.allocate_exact_size(vec2(room * LABEL_SHARE, button), egui::Sense::hover());
        ui.painter().text(
            pos2(spot.left(), spot.center().y),
            egui::Align2::LEFT_CENTER,
            &label,
            font(theme, TextRole::Caption),
            theme.text_muted(),
        );
        let width = ui.available_width() - button - theme.space(Space::S2);
        let response = sue_wm::ui::number_field(("sve-inspector", row.key), &mut value)
            .label(&label)
            .range(row.range.0..=row.range.1)
            .speed(row.speed)
            .decimals(row.decimals)
            .suffix(row.suffix)
            .width(width)
            .show(ui, theme);
        if response.changed() {
            let stored = value / row.shown;
            studio.edit_live(cx, |sequence| {
                if let Some((_, clip)) = sequence.find_mut(id) {
                    if keyed && inside {
                        let _ = clip.animation.channel_mut(name).insert(sue_model::Key {
                            time: source_time,
                            value: stored,
                            interpolation: sue_model::Interpolation::Linear,
                        });
                    } else {
                        clip.properties.set(name, stored);
                    }
                }
            });
            if !response.dragged() {
                studio.commit_live(cx, &label);
            }
        }
        if response.drag_stopped() || response.lost_focus() {
            studio.commit_live(cx, &label);
        }
        let icon = if on_key {
            Icon::Keyframe
        } else {
            Icon::KeyframeAdd
        };
        let tip = if on_key {
            tr!("inspector-remove-key")
        } else {
            tr!("inspector-add-key")
        };
        if sue_wm::ui::icon_button(("sve-key", row.key), icon, &tip)
            .selected(keyed)
            .enabled(inside)
            .show(ui, theme)
            .clicked()
        {
            studio.edit(cx, &tip, |sequence, _| {
                if on_key {
                    edit::remove_key(sequence, id, name, at)
                } else {
                    edit::set_key(sequence, id, name, at, stored)
                }
            });
        }
    });
}

/// A sound clip's fades.
fn fades_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    clip: &Clip,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    for (name, label) in [
        (key::FADE_IN, "inspector-fade-in"),
        (key::FADE_OUT, "inspector-fade-out"),
    ] {
        let mut seconds = clip
            .properties
            .get(name)
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let label = sue_lang::tr!("ve", label);
        let response = labeled(ui, &label, theme, |ui| {
            sue_wm::ui::number_field(("sve-fade", name), &mut seconds)
                .label(&label)
                .range(0.0..=clip.duration.seconds().max(0.0))
                .speed(0.01)
                .decimals(2)
                .suffix(" s")
                .show(ui, theme)
        });
        if response.changed() {
            let id = clip.id;
            studio.edit_live(cx, |sequence| {
                if let Some((_, clip)) = sequence.find_mut(id) {
                    clip.properties.set(name, seconds);
                }
            });
            if !response.dragged() {
                studio.commit_live(cx, &label);
            }
        }
        if response.drag_stopped() || response.lost_focus() {
            studio.commit_live(cx, &label);
        }
    }
}

/// A clip's speed, in percent (negative plays backward).
fn speed_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    clip: &Clip,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let mut percent = clip.speed * 100.0;
    let label = tr!("inspector-speed");
    let response = labeled(ui, &label, theme, |ui| {
        sue_wm::ui::number_field("sve-speed", &mut percent)
            .label(&label)
            .range(-1000.0..=1000.0)
            .speed(1.0)
            .decimals(0)
            .suffix(" %")
            .show(ui, theme)
    });
    // A speed changes the clip's length: it is made once the value is set.
    if (response.lost_focus()
        || response.drag_stopped()
        || (response.changed() && !response.dragged()))
        && (percent / 100.0 - clip.speed).abs() > 1e-6
        && percent.abs() >= 1.0
    {
        let id = clip.id;
        studio.edit(cx, &label, |sequence, _| {
            edit::set_speed(sequence, id, percent / 100.0, true)
        });
    }
    let hold = clip
        .properties
        .get(key::FREEZE)
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if hold {
        ui.label(egui::RichText::new(tr!("inspector-frozen")).font(font(theme, TextRole::Caption)));
    }
}

/// A title's words, size and color.
fn title_ui(
    studio: &mut SueVe,
    ui: &mut Ui,
    source: EntityId,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) {
    let Ok(entity) = cx.arena().get_by_id(source) else {
        return;
    };
    let words = entity
        .string_buffer()
        .map(|buffer| buffer.as_str().to_owned())
        .unwrap_or_default();
    let size = model::number(entity, key::SIZE).unwrap_or(96.0);
    let color = crate::media::import::color_of(entity, [255, 255, 255, 255]);
    sue_wm::ui::list::heading(ui, &tr!("inspector-title"), theme);
    if studio
        .title_draft
        .as_ref()
        .is_none_or(|(id, _)| *id != source)
    {
        studio.title_draft = Some((source, words.clone()));
    }
    let Some((_, draft)) = studio.title_draft.as_mut() else {
        return;
    };
    let response = sue_wm::ui::text_field("sve-title-words", draft).show(ui, theme);
    if !response.has_focus() && !response.lost_focus() && *draft != words {
        // Undo changed the words: the field follows.
        draft.clone_from(&words);
    }
    if response.lost_focus() && *draft != words {
        let text = draft.clone();
        if let Some(handle) = cx.arena().handle_of(source) {
            let length = words.len();
            let batch = sue_core::txn::Batch::new(tr!("inspector-title"))
                .then(sue_core::txn::DeleteText::new(handle, 0..length))
                .then(sue_core::txn::InsertText::new(handle, 0, text));
            if let Err(error) = cx.apply(ArenaEdit(batch)) {
                cx.fail(error.to_string());
            }
        }
    }
    let mut letters = size;
    let label = tr!("inspector-size");
    let sized = labeled(ui, &label, theme, |ui| {
        sue_wm::ui::number_field("sve-title-size", &mut letters)
            .label(&label)
            .range(4.0..=1000.0)
            .speed(0.5)
            .decimals(0)
            .suffix(" px")
            .show(ui, theme)
    });
    if (sized.drag_stopped() || sized.lost_focus() || (sized.changed() && !sized.dragged()))
        && (letters - size).abs() > 1e-6
    {
        let set = sue_core::txn::SetProperty::new(source, key::SIZE, letters);
        if let Err(error) = cx.apply(ArenaEdit(set)) {
            cx.fail(error.to_string());
        }
    }
    let mut rgba = color;
    let picked = labeled(ui, &tr!("inspector-color"), theme, |ui| {
        sue_wm::color_button(ui, "sve-title-color", &mut rgba, true, theme)
    });
    if (picked.released || (picked.changed && !picked.dragging)) && rgba != color {
        let mut value = sue_core::Color::srgb8(rgba[0], rgba[1], rgba[2]);
        value.alpha = f32::from(rgba[3]) / 255.0;
        let set = sue_core::txn::SetProperty::new(source, key::COLOR, value);
        if let Err(error) = cx.apply(ArenaEdit(set)) {
            cx.fail(error.to_string());
        }
    }
}

/// With no clip selected: the sequence's facts.
fn sequence_ui(
    studio: &SueVe,
    ui: &mut Ui,
    sequence: &sue_model::Sequence,
    cx: &AppletContext<'_>,
    theme: &Theme,
) {
    let name = studio
        .sequence
        .and_then(|id| cx.arena().get_by_id(id).ok())
        .map(|entity| entity.name().to_owned())
        .unwrap_or_default();
    sue_wm::ui::heading(ui, &name, theme);
    let (width, height) = model::frame_size(sequence);
    let facts = [
        (tr!("field-size"), format!("{width} x {height}")),
        (
            tr!("field-rate"),
            format!("{:.3}", sequence.rate.as_f64()).replace(".000", ""),
        ),
        (
            tr!("field-sample-rate"),
            format!("{} Hz", sequence.sample_rate),
        ),
        (
            tr!("field-duration"),
            super::timecode(sequence.duration(), sequence.rate),
        ),
        (tr!("field-markers"), sequence.markers.len().to_string()),
    ];
    for (label, value) in facts {
        let row = Density::Compact.row();
        let (rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), row), egui::Sense::hover());
        ui.painter().text(
            pos2(rect.left(), rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            font(theme, TextRole::Caption),
            theme.text_muted(),
        );
        ui.painter().text(
            pos2(rect.right(), rect.center().y),
            egui::Align2::RIGHT_CENTER,
            value,
            font(theme, TextRole::Mono),
            theme.text,
        );
    }
    ui.add_space(theme.space(Space::S6));
    ui.label(
        egui::RichText::new(tr!("inspector-pick-a-clip"))
            .font(font(theme, TextRole::Body))
            .color(theme.text_muted()),
    );
}
