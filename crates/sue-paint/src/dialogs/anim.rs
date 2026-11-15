//! The animation dialogs: Onion Skin Settings, Edit Timeline, Export
//! Timelapse, and those of the timeline: New Timeline, Change Settings,
//! the movie, image sequence, animated picture and cels exports, Audio
//! Clip Settings and a timeline label.

use egui::{RichText, Ui};
use sue_wm::{AppletContext, Theme};

use super::{Outcome, ok_cancel};
use crate::doc;
use crate::studio::SuePaint;

/// Onion Skin Settings, as Clip Studio Paint lays them out: how many cels
/// show before and after, their colors, and how faint they are.
pub(super) fn onion_ui(ui: &mut Ui, onion: &mut crate::anim::Onion, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let modes: Vec<String> = crate::anim::Onion::MODES
        .iter()
        .map(|name| crate::localize(name))
        .collect();
    wm_ui::form_grid(ui, "anim-onion-form", theme, |grid| {
        // No section titles: the labels say it, and the form fits the
        // dialog with room for the opacity rows.
        for (salt, name, count) in [
            (
                "anim-onion-before",
                tr!("onion-cels-before"),
                &mut onion.before,
            ),
            (
                "anim-onion-after",
                tr!("onion-cels-after"),
                &mut onion.after,
            ),
        ] {
            grid.row(&name, |ui| {
                whole(ui, salt, count, 0..=10, "", theme);
            });
        }
        grid.row(&tr!("color-mode"), |ui| {
            let mut chosen = usize::from(onion.mode.min(2));
            if wm_ui::dropdown("anim-onion-mode", &mut chosen, &modes)
                .show(ui, theme)
                .changed()
            {
                onion.mode = u8::try_from(chosen).unwrap_or(0);
            }
        });
        for (name, color) in [
            (tr!("onion-color-before"), &mut onion.before_color),
            (tr!("onion-color-after"), &mut onion.after_color),
        ] {
            grid.row(&name, |ui| {
                let mut rgba = [color[0], color[1], color[2], 255];
                if sue_wm::color_button(ui, &name, &mut rgba, false, theme).changed {
                    *color = [rgba[0], rgba[1], rgba[2]];
                }
            });
        }
        let mut specify = onion.opacity.is_some();
        grid.row("", |ui| {
            tick(
                ui,
                "anim-onion-specify",
                &tr!("specify-opacity-level"),
                &mut specify,
                theme,
            );
        });
        if specify != onion.opacity.is_some() {
            onion.opacity = specify.then_some((0.8, 0.1));
        }
        if let Some((start, step)) = onion.opacity.as_mut() {
            for (salt, name, value) in [
                ("anim-onion-start", tr!("onion-first-opacity"), start),
                ("anim-onion-step", tr!("onion-opacity-step"), step),
            ] {
                grid.row(&name, |ui| {
                    let mut percent = (f64::from(*value) * 100.0).round();
                    if wm_ui::number_field(salt, &mut percent)
                        .range(0.0..=100.0)
                        .decimals(0)
                        .suffix(" %")
                        .label(&name)
                        .show(ui, theme)
                        .changed()
                    {
                        *value = (percent / 100.0) as f32;
                    }
                });
            }
        }
    });
    ok_cancel(ui, &mut outcome);
    outcome
}

/// Change Frame Rate (Edit Timeline): frames a second and length in
/// frames. The rest of a timeline's settings are in Change Settings.
pub(super) fn timeline_ui(ui: &mut Ui, fps: &mut u32, length: &mut u32, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    wm_ui::form_grid(ui, "anim-edit-timeline", theme, |grid| {
        grid.row(&tr!("form-frame-rate"), |ui| {
            whole(ui, "anim-edit-fps", fps, 1..=120, &tr!("fps"), theme);
        });
        grid.row(&tr!("form-playback-time"), |ui| {
            whole(
                ui,
                "anim-edit-length",
                length,
                1..=100_000,
                &tr!("frames"),
                theme,
            );
        });
    });
    ok_cancel(ui, &mut outcome);
    outcome
}

impl SuePaint {
    /// Changes the timeline's frame rate and length.
    pub(super) fn timeline_ok(&mut self, cx: &mut AppletContext<'_>, fps: u32, length: u32) {
        if let Some(picture) = self.picture()
            && let Some(edit) = crate::anim::edit_timeline(&cx.world.arena, picture, fps, length)
        {
            Self::apply(cx, edit);
        }
    }

    /// Keeps the onion skin settings on the picture and shows it.
    pub(super) fn onion_ok(&mut self, cx: &mut AppletContext<'_>, onion: &crate::anim::Onion) {
        if let Some(picture) = self.picture() {
            let edit = doc::Edit::new(tr!("onion-skin-settings")).then(
                sue_core::txn::SetProperty::new(picture, crate::anim::ONION, onion.to_text()),
            );
            if Self::apply(cx, edit) {
                // The settings are for seeing it.
                self.onion = true;
            }
        }
    }
}

// ----- The timeline's dialogs -------------------------------------------------

use crate::panels::timeline::forms::{
    Animated, AnimatedForm, AudioClipForm, CelsForm, InsertForm, LabelForm, MovieForm, Range,
    SequenceForm, TimelineForm,
};
use sue_wm::ui::{self as wm_ui, TextRole};

/// A dialog of the timeline, kept while it is open.
#[derive(Clone, Debug)]
pub(crate) enum AnimDialog {
    /// New Timeline, or Change Settings of the one shown.
    Timeline(TimelineForm),
    /// File, Export Animation, Movie.
    Movie(MovieForm),
    /// File, Export Animation, Image Sequence.
    Sequence(SequenceForm),
    /// Animated GIF, Animated Sticker (APNG), Animated WebP.
    Animated(AnimatedForm),
    /// Export Animation Cels.
    Cels(CelsForm),
    /// Audio Clip Settings.
    AudioClip(AudioClipForm),
    /// A timeline label's name.
    Label(LabelForm),
    /// Stopping Record Timelapse: keep what was recorded, or delete it
    /// (`true`). It holds the frames kept so far.
    StopTimelapse(bool, usize),
    /// Insert Frame: how many, where and how.
    Insert(InsertForm),
}

impl AnimDialog {
    /// The dialog's title.
    pub(super) fn title(&self) -> String {
        match self {
            Self::Timeline(form) if form.new => tr!("new-timeline-2"),
            Self::Timeline(_) => tr!("change-settings"),
            Self::Movie(_) => tr!("movie-export-settings"),
            Self::Sequence(_) => tr!("image-sequence-export-settings"),
            Self::Animated(form) => match form.kind {
                Animated::Gif => tr!("animated-gif-export-settings"),
                Animated::Apng => tr!("animated-png-export-settings"),
                Animated::WebP => tr!("animated-webp-export-settings"),
            },
            Self::Cels(_) => tr!("export-animation-cels"),
            Self::AudioClip(_) => tr!("audio-clip-settings"),
            Self::Label(_) => tr!("timeline-label"),
            Self::StopTimelapse(..) => tr!("stop-recording-timelapse"),
            Self::Insert(_) => tr!("insert-frame"),
        }
    }

    /// Its size before the screen clamps it: as many rows as it has, each
    /// a regular row and its gap.
    pub(super) fn size(&self) -> egui::Vec2 {
        let rows: f32 = match self {
            Self::Timeline(_) => 11.5,
            Self::Movie(_) => 10.5,
            Self::Sequence(_) => 11.0,
            Self::Animated(_) => 9.0,
            Self::Cels(_) => 5.0,
            Self::AudioClip(_) => 10.0,
            Self::Label(_) | Self::StopTimelapse(..) => 4.0,
            Self::Insert(_) => 5.5,
        };
        let row = wm_ui::Density::Regular.row();
        egui::Vec2::new(row * 15.0, row * (rows + 2.0) * 1.2)
    }

    /// Draws it; the answer says whether it stays open.
    pub(super) fn ui(&mut self, ui: &mut Ui, theme: &Theme) -> Outcome {
        match self {
            Self::Timeline(form) => timeline_form_ui(ui, form, theme),
            Self::Movie(form) => movie_ui(ui, form, theme),
            Self::Sequence(form) => sequence_ui(ui, form, theme),
            Self::Animated(form) => animated_ui(ui, form, theme),
            Self::Cels(form) => cels_ui(ui, form, theme),
            Self::AudioClip(form) => audio_clip_ui(ui, form, theme),
            Self::Label(form) => label_ui(ui, form, theme),
            Self::StopTimelapse(delete, frames) => stop_timelapse_ui(ui, delete, *frames, theme),
            Self::Insert(form) => insert_ui(ui, form, theme),
        }
    }
}

impl SuePaint {
    /// What OK does in a dialog of the timeline.
    pub(super) fn anim_dialog_ok(&mut self, cx: &mut AppletContext<'_>, dialog: AnimDialog) {
        match dialog {
            AnimDialog::Timeline(form) => self.timeline_form_ok(cx, &form),
            AnimDialog::Movie(form) => self.ask_movie_file(cx, form),
            AnimDialog::Sequence(form) => self.ask_sequence_file(cx, form),
            AnimDialog::Animated(form) => self.ask_animated_file(cx, form),
            AnimDialog::Cels(form) => self.ask_cels_file(cx, form),
            AnimDialog::AudioClip(form) => self.audio_clip_ok(cx, &form),
            AnimDialog::Label(form) => self.label_ok(cx, &form),
            AnimDialog::StopTimelapse(delete, _) => self.stop_timelapse(cx, delete),
            AnimDialog::Insert(form) => self.insert_frames(cx, &form),
        }
    }
}

/// A line of muted help under a form.
fn help(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.label(
        RichText::new(text)
            .font(theme.text_font(TextRole::Caption))
            .color(theme.text_muted()),
    );
}

/// A whole number in a field, kept inside `range`. `true` when changed.
fn whole(
    ui: &mut Ui,
    salt: &str,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
    suffix: &str,
    theme: &Theme,
) -> bool {
    let mut number = f64::from(*value);
    let changed = wm_ui::number_field(salt, &mut number)
        .range(f64::from(*range.start())..=f64::from(*range.end()))
        .decimals(0)
        .suffix(suffix)
        .show(ui, theme)
        .changed();
    if changed {
        *value = number.round().max(0.0) as u32;
    }
    changed
}

/// A number of seconds in a field, 0 to `most`.
fn seconds(ui: &mut Ui, salt: &str, value: &mut f64, most: f64, theme: &Theme) {
    wm_ui::number_field(salt, value)
        .range(0.0..=most.max(0.0))
        .decimals(2)
        .step(0.1)
        .suffix(" s")
        .show(ui, theme);
}

/// A box that is ticked or not.
fn tick(ui: &mut Ui, salt: &str, label: &str, on: &mut bool, theme: &Theme) {
    if wm_ui::checkbox(salt, label, *on).show(ui, theme).clicked() {
        *on = !*on;
    }
}

/// The range of frames an export covers: all, the played ones or some.
fn range_rows(grid: &mut wm_ui::FormGrid<'_, '_>, range: &mut Range, theme: &Theme) {
    let choices = Range::choices();
    grid.row(&tr!("export-range"), |ui| {
        let mut chosen = range.index();
        if wm_ui::dropdown("anim-export-range", &mut chosen, &choices)
            .show(ui, theme)
            .changed()
        {
            *range = match chosen {
                0 => Range::All,
                1 => Range::Played,
                _ => Range::Frames(1, 24),
            };
        }
    });
    if let Range::Frames(from, to) = range {
        grid.row(&tr!("export-range-from"), |ui| {
            whole(ui, "anim-export-from", from, 1..=100_000, "", theme);
        });
        grid.row(&tr!("export-range-to"), |ui| {
            whole(ui, "anim-export-to", to, 1..=100_000, "", theme);
        });
    }
}

/// The names of the picture formats, in `sue_io::PICTURE_SAVE_TYPES`'s
/// order.
fn picture_formats() -> Vec<String> {
    sue_io::PICTURE_SAVE_TYPES
        .iter()
        .map(|(name, extensions)| format!("{name} (.{})", extensions.first().unwrap_or(&"")))
        .collect()
}

/// New Timeline and Change Settings.
fn timeline_form_ui(ui: &mut Ui, form: &mut TimelineForm, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    wm_ui::form_grid(ui, "anim-timeline-form", theme, |grid| {
        grid.row(&tr!("timeline-name"), |ui| {
            wm_ui::text_field("anim-timeline-name", &mut form.name).show(ui, theme);
        });
        grid.row(&tr!("form-frame-rate"), |ui| {
            whole(
                ui,
                "anim-timeline-fps",
                &mut form.fps,
                1..=120,
                &tr!("fps"),
                theme,
            );
        });
        grid.row(&tr!("form-playback-time"), |ui| {
            let was = form.length;
            if whole(
                ui,
                "anim-timeline-length",
                &mut form.length,
                1..=100_000,
                &tr!("frames"),
                theme,
            ) && form.end == was
            {
                // A range that ran to the end keeps running to it.
                form.end = form.length;
            }
        });
        grid.section(&tr!("playback-range"));
        grid.row(&tr!("first-frame-played"), |ui| {
            whole(
                ui,
                "anim-timeline-start",
                &mut form.start,
                1..=100_000,
                "",
                theme,
            );
        });
        grid.row(&tr!("last-frame-played"), |ui| {
            whole(
                ui,
                "anim-timeline-end",
                &mut form.end,
                1..=100_000,
                "",
                theme,
            );
        });
        grid.section(&tr!("timeline-sheet"));
        grid.row(&tr!("scene-number"), |ui| {
            whole(
                ui,
                "anim-timeline-scene",
                &mut form.scene,
                0..=9999,
                "",
                theme,
            );
        });
        grid.row(&tr!("shot-number"), |ui| {
            whole(
                ui,
                "anim-timeline-shot",
                &mut form.shot,
                0..=9999,
                "",
                theme,
            );
        });
        grid.row(&tr!("division-line-every"), |ui| {
            whole(
                ui,
                "anim-timeline-division",
                &mut form.division,
                1..=1000,
                &tr!("frames"),
                theme,
            );
        });
    });
    form.end = form.end.clamp(1, form.length.max(1));
    form.start = form.start.clamp(1, form.end);
    if form.name.trim().is_empty() {
        help(ui, &tr!("a-timeline-needs-a-name"), theme);
    }
    ok_cancel(ui, &mut outcome);
    if outcome == Outcome::Ok && form.name.trim().is_empty() {
        outcome = Outcome::Open;
    }
    outcome
}

/// The movie export settings of D22: the format (MP4 first where the
/// system has an encoder), size, frame rate, quality, 2D camera, sound and
/// range.
fn movie_ui(ui: &mut Ui, form: &mut MovieForm, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let names: Vec<String> = form
        .formats
        .iter()
        .map(|format| sue_lang::translate("io", format.name()))
        .collect();
    let qualities: Vec<String> = sue_io::MovieQuality::ALL
        .iter()
        .map(|quality| sue_lang::translate("io", quality.name()))
        .collect();
    wm_ui::form_grid(ui, "anim-movie-form", theme, |grid| {
        grid.row(&tr!("movie-format"), |ui| {
            wm_ui::dropdown("anim-movie-format", &mut form.format, &names).show(ui, theme);
        });
        grid.row(&tr!("form-width"), |ui| {
            let mut width = form.size[0];
            if whole(ui, "anim-movie-width", &mut width, 1..=8192, " px", theme) {
                form.set_width(width);
            }
        });
        grid.row(&tr!("form-height"), |ui| {
            let mut height = form.size[1];
            if whole(ui, "anim-movie-height", &mut height, 1..=8192, " px", theme) {
                form.set_height(height);
            }
        });
        grid.row("", |ui| {
            tick(
                ui,
                "anim-movie-shape",
                &tr!("keep-the-shape"),
                &mut form.keep_shape,
                theme,
            );
        });
        grid.row(&tr!("form-frame-rate"), |ui| {
            whole(
                ui,
                "anim-movie-fps",
                &mut form.fps,
                1..=240,
                &tr!("fps"),
                theme,
            );
        });
        grid.row(&tr!("movie-quality"), |ui| {
            let mut chosen = sue_io::MovieQuality::ALL
                .iter()
                .position(|quality| *quality == form.quality)
                .unwrap_or(1);
            if wm_ui::dropdown("anim-movie-quality", &mut chosen, &qualities)
                .show(ui, theme)
                .changed()
            {
                form.quality = sue_io::MovieQuality::ALL[chosen];
            }
        });
        grid.row("", |ui| {
            tick(
                ui,
                "anim-movie-camera",
                &tr!("apply-2d-camera-effects"),
                &mut form.camera,
                theme,
            );
        });
        if form.has_sound {
            let carries = form.chosen().carries_sound();
            grid.row("", |ui| {
                ui.add_enabled_ui(carries, |ui| {
                    tick(
                        ui,
                        "anim-movie-sound",
                        &tr!("with-the-timeline-sound"),
                        &mut form.sound,
                        theme,
                    );
                });
            });
        }
        range_rows(grid, &mut form.range, theme);
    });
    let format = form.chosen();
    let settings = form.settings();
    if let Err(error) = settings.check(format) {
        help(ui, &sue_lang::translate("io", error.text()), theme);
    } else if settings.is_padded(format) {
        help(ui, &sue_lang::translate("io", sue_io::PADDED_NOTE), theme);
    }
    if !format.carries_sound() && form.has_sound {
        help(ui, &tr!("avi-carries-no-sound"), theme);
    }
    if !form.formats.contains(&sue_io::MovieFormat::Mp4H264) {
        help(ui, &tr!("mp4-needs-the-media-feature-pack"), theme);
    }
    ok_cancel(ui, &mut outcome);
    if outcome == Outcome::Ok && settings.check(format).is_err() {
        outcome = Outcome::Open;
    }
    outcome
}

/// The image sequence settings: file names, format, size and range.
fn sequence_ui(ui: &mut Ui, form: &mut SequenceForm, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let formats = picture_formats();
    let hint = tr!("the-name-you-save-as");
    wm_ui::form_grid(ui, "anim-sequence-form", theme, |grid| {
        grid.row(&tr!("file-name-prefix"), |ui| {
            wm_ui::text_field("anim-sequence-prefix", &mut form.prefix)
                .hint(&hint)
                .show(ui, theme);
        });
        grid.row(&tr!("first-number"), |ui| {
            whole(
                ui,
                "anim-sequence-first",
                &mut form.first_number,
                0..=999_999,
                "",
                theme,
            );
        });
        grid.row(&tr!("digits"), |ui| {
            whole(
                ui,
                "anim-sequence-digits",
                &mut form.digits,
                1..=9,
                "",
                theme,
            );
        });
        grid.row(&tr!("file-format"), |ui| {
            wm_ui::dropdown("anim-sequence-format", &mut form.format, &formats).show(ui, theme);
        });
        grid.row(&tr!("scale"), |ui| {
            whole(
                ui,
                "anim-sequence-scale",
                &mut form.scale,
                1..=400,
                " %",
                theme,
            );
        });
        grid.row("", |ui| {
            tick(
                ui,
                "anim-sequence-camera",
                &tr!("apply-2d-camera-effects"),
                &mut form.camera,
                theme,
            );
        });
        range_rows(grid, &mut form.range, theme);
    });
    help(
        ui,
        &tr!(
            "first-file-will-be",
            name = form.file_name(&tr!("animation"), form.first_number, form.extension())
        ),
        theme,
    );
    ok_cancel(ui, &mut outcome);
    outcome
}

/// The animated picture settings: size, loops (GIF) and range.
fn animated_ui(ui: &mut Ui, form: &mut AnimatedForm, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    wm_ui::form_grid(ui, "anim-animated-form", theme, |grid| {
        grid.row(&tr!("form-width"), |ui| {
            let mut width = form.size[0];
            if whole(
                ui,
                "anim-animated-width",
                &mut width,
                1..=8192,
                " px",
                theme,
            ) {
                form.set_width(width);
            }
        });
        grid.row(&tr!("form-height"), |ui| {
            help(
                ui,
                &format!("{} px", sue_lang::number(f64::from(form.size[1]), 0)),
                theme,
            );
        });
        if form.kind == Animated::Gif {
            grid.row(&tr!("loop-count"), |ui| {
                whole(
                    ui,
                    "anim-animated-loops",
                    &mut form.loops,
                    0..=65_535,
                    "",
                    theme,
                );
            });
        }
        grid.row(&tr!("form-frame-rate"), |ui| {
            help(ui, &tr!("fps-of-the-timeline", value = form.fps), theme);
        });
        grid.row("", |ui| {
            tick(
                ui,
                "anim-animated-camera",
                &tr!("apply-2d-camera-effects"),
                &mut form.camera,
                theme,
            );
        });
        range_rows(grid, &mut form.range, theme);
    });
    if form.kind == Animated::Gif {
        help(ui, &tr!("loop-count-zero-plays-for-ever"), theme);
    }
    ok_cancel(ui, &mut outcome);
    outcome
}

/// Export Animation Cels: the format, and a folder per animation folder.
fn cels_ui(ui: &mut Ui, form: &mut CelsForm, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let formats = picture_formats();
    wm_ui::form_grid(ui, "anim-cels-form", theme, |grid| {
        grid.row(&tr!("file-format"), |ui| {
            wm_ui::dropdown("anim-cels-format", &mut form.format, &formats).show(ui, theme);
        });
        grid.row("", |ui| {
            tick(
                ui,
                "anim-cels-folders",
                &tr!("a-folder-for-each-animation-folder"),
                &mut form.per_folder,
                theme,
            );
        });
    });
    help(ui, &tr!("cels-export-flattened"), theme);
    ok_cancel(ui, &mut outcome);
    outcome
}

/// Audio Clip Settings: volume, cuts and fades.
fn audio_clip_ui(ui: &mut Ui, form: &mut AudioClipForm, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let length = form.seconds;
    wm_ui::form_grid(ui, "anim-audio-form", theme, |grid| {
        grid.row(&tr!("volume"), |ui| {
            wm_ui::number_field("anim-audio-volume", &mut form.volume_db)
                .range(AudioClipForm::QUIETEST_DB..=AudioClipForm::LOUDEST_DB)
                .decimals(1)
                .step(0.5)
                .suffix(" dB")
                .show(ui, theme);
        });
        grid.section(&tr!("cut"));
        grid.row(&tr!("cut-from-the-start"), |ui| {
            seconds(
                ui,
                "anim-audio-cut-start",
                &mut form.cut_start,
                length,
                theme,
            );
        });
        grid.row(&tr!("cut-from-the-end"), |ui| {
            seconds(ui, "anim-audio-cut-end", &mut form.cut_end, length, theme);
        });
        grid.section(&tr!("fade"));
        grid.row(&tr!("fade-in"), |ui| {
            seconds(ui, "anim-audio-fade-in", &mut form.fade_in, length, theme);
        });
        grid.row(&tr!("fade-out"), |ui| {
            seconds(ui, "anim-audio-fade-out", &mut form.fade_out, length, theme);
        });
    });
    let left = form.seconds - form.cut_start - form.cut_end;
    help(
        ui,
        &tr!(
            "audio-clip-length",
            value = format!("{:.2}", left.max(0.0)),
            whole = format!("{:.2}", form.seconds)
        ),
        theme,
    );
    ok_cancel(ui, &mut outcome);
    if outcome == Outcome::Ok && left <= 0.0 {
        outcome = Outcome::Open;
    }
    outcome
}

/// A timeline label's name; empty takes it away.
fn label_ui(ui: &mut Ui, form: &mut LabelForm, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    wm_ui::form_grid(ui, "anim-label-form", theme, |grid| {
        grid.row(&tr!("label-at-frame", value = form.frame + 1), |ui| {
            wm_ui::text_field("anim-label-name", &mut form.name).show(ui, theme);
        });
    });
    help(ui, &tr!("an-empty-label-is-removed"), theme);
    ok_cancel(ui, &mut outcome);
    outcome
}

/// Stopping Record Timelapse asks what becomes of the frames kept so far:
/// Keep them to export later, or Delete them (that cannot be undone).
fn stop_timelapse_ui(ui: &mut Ui, delete: &mut bool, frames: usize, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    ui.label(
        RichText::new(tr!("stop-timelapse-keep-or-delete", count = frames))
            .font(theme.text_font(TextRole::Body))
            .color(theme.text),
    );
    help(ui, &tr!("a-kept-timelapse-can-be-exported"), theme);
    ui.add_space(theme.space(wm_ui::Space::S5));
    crate::dialogs::button_row(ui, |ui| {
        if wm_ui::button("anim-timelapse-keep", &tr!("keep-the-recording"))
            .primary()
            .show(ui, theme)
            .clicked()
        {
            *delete = false;
            outcome = Outcome::Ok;
        }
        if wm_ui::button("anim-timelapse-delete", &tr!("delete-the-recording"))
            .danger()
            .show(ui, theme)
            .clicked()
        {
            *delete = true;
            outcome = Outcome::Ok;
        }
        if wm_ui::button("anim-timelapse-cancel", &tr!("cancel"))
            .ghost()
            .show(ui, theme)
            .clicked()
        {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

/// Insert Frame: the number of frames, Selected layers only, Split clip.
fn insert_ui(ui: &mut Ui, form: &mut InsertForm, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    wm_ui::form_grid(ui, "anim-insert-form", theme, |grid| {
        grid.row(&tr!("number-of-frames"), |ui| {
            whole(
                ui,
                "anim-insert-count",
                &mut form.count,
                1..=10_000,
                "",
                theme,
            );
        });
        grid.row("", |ui| {
            tick(
                ui,
                "anim-insert-selected",
                &tr!("selected-layers-only"),
                &mut form.selected_only,
                theme,
            );
        });
        grid.row("", |ui| {
            tick(
                ui,
                "anim-insert-split",
                &tr!("split-clip-option"),
                &mut form.split,
                theme,
            );
        });
        grid.row("", |ui| {
            tick(
                ui,
                "anim-insert-preview",
                &tr!("preview-on-the-timeline"),
                &mut form.preview,
                theme,
            );
        });
    });
    help(
        ui,
        &tr!("frames-go-in-after-frame", value = form.frame + 1),
        theme,
    );
    ok_cancel(ui, &mut outcome);
    outcome
}
