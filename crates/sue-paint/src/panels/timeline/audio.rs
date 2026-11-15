//! The timeline's sound: Audio Clip Settings (volume, cuts and fades
//! through `sue_io::transform_sound`), the waveform drawn on an audio
//! clip, and File, Export Animation, Audio (the mixed sound through the
//! export dialog's sound page).

use std::path::Path;

use sue_core::txn::ReplaceComponent;
use sue_core::{EntityId, PCMFloat1D, Time};
use sue_platform::write_atomic;
use sue_wm::export::ExportDialog;
use sue_wm::{AppletContext, FilePick, FileType, ShellRequest};

use super::Edit;
use super::forms::AudioClipForm;
use crate::anim;
use crate::dialogs::{AnimDialog, Dialog};
use crate::studio::SuePaint;

/// The loudest sample in each of `columns` equal stretches of `pcm`, all
/// channels together: what a clip's waveform draws.
#[must_use]
pub(crate) fn peaks(pcm: &PCMFloat1D, columns: usize) -> Vec<f32> {
    let channels = usize::from(pcm.channels().max(1));
    let frames = pcm.frames();
    let samples = pcm.samples();
    (0..columns)
        .map(|column| {
            let from = frames * column / columns.max(1);
            let to = (frames * (column + 1) / columns.max(1))
                .max(from + 1)
                .min(frames);
            samples[from * channels..to * channels]
                .iter()
                .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
        })
        .collect()
}

/// The sound of an audio clip: its entity's samples.
fn clip_sound(arena: &sue_core::Arena, source: EntityId) -> Option<&PCMFloat1D> {
    arena
        .get_by_id(source)
        .ok()?
        .component(sue_core::ComponentKind::PCMFloat1D)?
        .as_pcm()
}

impl SuePaint {
    /// Opens Audio Clip Settings for audio track `track` of the timeline.
    pub(crate) fn ask_audio_clip(&mut self, cx: &mut AppletContext<'_>, track: usize) {
        let Some((_, sequence, _)) = self.timeline_here(&cx.world.arena) else {
            return;
        };
        let Some(clip) = sequence
            .tracks
            .get(track)
            .filter(|track| track.kind == anim::AUDIO)
            .and_then(|track| track.clips().first())
        else {
            cx.fail(tr!("pick-an-audio-clip-first"));
            return;
        };
        let seconds = clip_sound(&cx.world.arena, clip.source).map_or_else(
            || clip.duration.seconds(),
            |pcm| pcm.frames() as f64 / f64::from(pcm.sample_rate().max(1)),
        );
        self.dialog = Some(Dialog::Anim(Box::new(AnimDialog::AudioClip(
            AudioClipForm {
                track,
                volume_db: AudioClipForm::decibels(anim::volume_of(clip)),
                cut_start: 0.0,
                cut_end: 0.0,
                fade_in: 0.0,
                fade_out: 0.0,
                seconds,
            },
        ))));
    }

    /// Opens Audio Clip Settings for the audio clip the timeline points
    /// at: the current track when it is an audio track, else the first.
    pub(crate) fn ask_audio_clip_here(&mut self, cx: &mut AppletContext<'_>) {
        let Some((_, sequence, _)) = self.timeline_here(&cx.world.arena) else {
            cx.fail(tr!("make-timeline-first-animation-timeline-new"));
            return;
        };
        let audio = anim::audio_tracks(&sequence);
        let current = self
            .anim_ui
            .audio_track
            .filter(|track| audio.contains(track));
        match current.or_else(|| audio.first().copied()) {
            Some(track) => self.ask_audio_clip(cx, track),
            None => cx.fail(tr!("pick-an-audio-clip-first")),
        }
    }

    /// Keeps Audio Clip Settings: the volume on the clip, and the cuts and
    /// fades into its sound, all as one undo step.
    pub(crate) fn audio_clip_ok(&mut self, cx: &mut AppletContext<'_>, form: &AudioClipForm) {
        let Some((picture, sequence, _)) = self.timeline_here(&cx.world.arena) else {
            return;
        };
        let Some(clip) = sequence
            .tracks
            .get(form.track)
            .and_then(|track| track.clips().first())
            .cloned()
        else {
            return;
        };
        let label = tr!("audio-clip-settings");
        let mut changed =
            anim::with_volume(&sequence, form.track, AudioClipForm::factor(form.volume_db));
        let mut edit = Edit::new(label.clone());
        if form.changes_sound() {
            let Some(pcm) = clip_sound(&cx.world.arena, clip.source) else {
                return;
            };
            let sound = match sue_io::transform_sound(pcm, &form.options()) {
                Ok(sound) => sound,
                Err(error) => {
                    cx.fail(sue_lang::translate("io", &error));
                    return;
                }
            };
            // The clip lasts as long as its sound now.
            let duration = Time::from_samples(sound.frames() as i64, sound.sample_rate());
            if let Some(track) = changed.tracks.get_mut(form.track) {
                let clips: Vec<_> = track.clips().to_vec();
                while track.remove(0).is_some() {}
                for mut each in clips {
                    if each.source == clip.source {
                        each.duration = duration;
                    }
                    track.insert(each);
                }
            }
            edit = edit.then(ReplaceComponent::new(clip.source, sound, label.clone()));
        }
        if changed == sequence && !form.changes_sound() {
            return;
        }
        Self::apply(
            cx,
            edit.then(sue_core::txn::SetExtension::set(picture, changed)),
        );
    }

    /// File, Export Animation, Audio: the timeline's sound mixed, through
    /// the export dialog's sound page.
    pub(crate) fn ask_export_audio(&mut self, cx: &mut AppletContext<'_>) {
        let Some((_, sequence, _)) = self.timeline_here(&cx.world.arena) else {
            cx.fail(tr!("make-timeline-first-animation-timeline-new"));
            return;
        };
        if anim::mix(&cx.world.arena, &sequence).is_none() {
            cx.fail(tr!("the-timeline-has-no-sound"));
            return;
        }
        let settings = self.anim_ui.sound_settings.clone().unwrap_or_default();
        self.anim_ui.sound_export = Some(ExportDialog::for_sound(
            tr!("export-animation-audio"),
            settings,
        ));
    }

    /// Shows the sound export dialog while it is open; Export asks where
    /// the file goes.
    pub(crate) fn sound_export_ui(&mut self, cx: &mut AppletContext<'_>, ctx: &egui::Context) {
        let Some(dialog) = self.anim_ui.sound_export.as_mut() else {
            return;
        };
        let theme = cx.theme.clone();
        let Some(answer) = dialog.show(ctx, &theme) else {
            return;
        };
        let Some(dialog) = self.anim_ui.sound_export.take() else {
            return;
        };
        if answer != sue_wm::ui::Answer::Accept {
            return;
        }
        let extension = dialog.extension();
        self.anim_ui.sound_settings = Some(dialog.sound);
        let name = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .map_or_else(|| tr!("animation"), |entity| entity.name().to_owned());
        cx.request(ShellRequest::PickFile(
            FilePick::save(
                Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
                "anim-sound",
                &tr!("export-animation-audio"),
                &format!("{name}.{extension}"),
            )
            .types(vec![FileType::new(
                &extension.to_ascii_uppercase(),
                &[extension],
            )])
            .starting_in(self.last_dir.clone()),
        ));
    }

    /// Writes the timeline's sound to `path` with the sound page's
    /// settings.
    pub(crate) fn write_audio(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some((_, sequence, _)) = self.timeline_here(&cx.world.arena) else {
            return;
        };
        let Some(pcm) = anim::mix(&cx.world.arena, &sequence) else {
            cx.fail(tr!("the-timeline-has-no-sound"));
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let settings = self.anim_ui.sound_settings.clone().unwrap_or_default();
        match settings.export(&pcm).and_then(|bytes| {
            write_atomic(path, &bytes)
                .map_err(|error| error.to_string())
                .map(|()| bytes.len())
        }) {
            Ok(size) => cx.saved(path, tr!("kib", value = size.div_ceil(1024))),
            Err(error) => cx.fail(tr!(
                "cannot-export",
                error = sue_lang::translate("io", &error)
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::peaks;
    use sue_core::PCMFloat1D;

    #[test]
    fn peaks_take_the_loudest_sample_of_each_stretch() {
        let pcm = PCMFloat1D::new(8, 2, vec![0.1, -0.5, 0.2, 0.0, -0.9, 0.3, 0.0, 0.0]).unwrap();
        assert_eq!(peaks(&pcm, 2), vec![0.5, 0.9]);
        assert_eq!(peaks(&pcm, 4).len(), 4);
    }
}
