//! What the animation dialogs hold while they are open: New Timeline
//! and Change Settings, the four export dialogs (movie, image sequence,
//! animated picture, cels) and Audio Clip Settings. The dialogs draw them
//! (`dialogs/anim.rs`); the timeline acts on them when they are kept.

use sue_core::{Arena, EntityId};
use sue_io::{MovieFormat, MovieQuality};

use crate::anim;

/// The frames an export covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Range {
    /// Every frame of the timeline.
    All,
    /// The frames played (the playback range).
    Played,
    /// Frames `from` to `to`, both included, counted from 1.
    Frames(u32, u32),
}

impl Range {
    /// The frames covered, from 0, the last not included, on a timeline
    /// `length` long whose playback range is `played` (from 0, both
    /// included).
    #[must_use]
    pub(crate) fn frames(self, length: u32, played: (u32, u32)) -> std::ops::Range<u32> {
        let length = length.max(1);
        match self {
            Self::All => 0..length,
            Self::Played => played.0.min(length - 1)..(played.1 + 1).min(length),
            Self::Frames(from, to) => {
                let first = from.clamp(1, length) - 1;
                let last = to.clamp(1, length).max(first + 1);
                first..last
            }
        }
    }

    /// The index of the choice in [`Range::choices`].
    #[must_use]
    pub(crate) fn index(self) -> usize {
        match self {
            Self::All => 0,
            Self::Played => 1,
            Self::Frames(..) => 2,
        }
    }

    /// The choices a dialog offers, in their order.
    #[must_use]
    pub(crate) fn choices() -> [String; 3] {
        [
            tr!("export-range-all-frames"),
            tr!("export-range-played-frames"),
            tr!("export-range-these-frames"),
        ]
    }
}

/// New Timeline and Change Settings: a timeline's name, frame rate,
/// length, playback range, scene, shot and division line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TimelineForm {
    /// `true` for New Timeline, `false` for Change Settings.
    pub new: bool,
    /// Its name.
    pub name: String,
    /// Frames a second.
    pub fps: u32,
    /// Its length in frames.
    pub length: u32,
    /// The first frame played, counted from 1.
    pub start: u32,
    /// The last frame played, counted from 1.
    pub end: u32,
    /// The scene number.
    pub scene: u32,
    /// The shot number.
    pub shot: u32,
    /// A division line every so many frames.
    pub division: u32,
}

impl TimelineForm {
    /// New Timeline's answers before anything is changed: the next name,
    /// 24 frames a second, 24 frames.
    #[must_use]
    pub(crate) fn new_timeline(arena: &Arena, picture: EntityId) -> Self {
        let count = anim::setup::timelines(arena, picture).len();
        Self {
            new: true,
            name: tr!("timeline-number", value = count + 1),
            fps: 24,
            length: 24,
            start: 1,
            end: 24,
            scene: 1,
            shot: 1,
            division: 6,
        }
    }

    /// Change Settings as `picture`'s timeline is now.
    #[must_use]
    pub(crate) fn of(arena: &Arena, picture: EntityId) -> Option<Self> {
        let sequence = anim::timeline(arena, picture)?;
        let length = anim::length(arena, picture);
        let settings = anim::setup::Settings::of(arena, picture);
        let (start, end) = settings.range(length);
        Some(Self {
            new: false,
            name: settings.name,
            fps: anim::fps(sequence).round().max(1.0) as u32,
            length,
            start: start + 1,
            end: end + 1,
            scene: settings.scene,
            shot: settings.shot,
            division: settings.division,
        })
    }

    /// The model's settings these answers give (the playback range from
    /// 0; the last frame left open when it is the timeline's end).
    #[must_use]
    pub(crate) fn settings(&self) -> anim::setup::Settings {
        let length = self.length.max(1);
        let end = self.end.clamp(1, length);
        let start = self.start.clamp(1, end);
        anim::setup::Settings {
            name: self.name.trim().to_owned(),
            scene: self.scene,
            shot: self.shot,
            division: self.division.max(1),
            start: start - 1,
            end: (end < length).then_some(end - 1),
        }
    }
}

/// File, Export Animation, Movie.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MovieForm {
    /// The formats this system writes, MP4 first where it has an encoder.
    pub formats: Vec<MovieFormat>,
    /// The index of the one chosen in `formats`.
    pub format: usize,
    /// The movie's width and height (pixels).
    pub size: [u32; 2],
    /// The picture's own size, for keeping the shape.
    pub source: [u32; 2],
    /// Keep the shape when one side changes.
    pub keep_shape: bool,
    /// Frames a second.
    pub fps: u32,
    /// How much it is squeezed.
    pub quality: MovieQuality,
    /// Seen through the 2D camera folders.
    pub camera: bool,
    /// The timeline's sound goes in (MP4 only).
    pub sound: bool,
    /// Whether the timeline has sound at all.
    pub has_sound: bool,
    /// The frames written.
    pub range: Range,
}

impl MovieForm {
    /// The format chosen.
    #[must_use]
    pub(crate) fn chosen(&self) -> MovieFormat {
        self.formats
            .get(self.format)
            .copied()
            .unwrap_or(MovieFormat::MjpegAvi)
    }

    /// The settings the sink is opened with, for `frames` frames.
    #[must_use]
    pub(crate) fn settings(&self) -> sue_io::MovieSettings {
        let mut settings = sue_io::MovieSettings::new(self.size[0], self.size[1], self.fps.max(1));
        settings.quality = self.quality;
        if self.sound && self.has_sound && self.chosen().carries_sound() {
            settings.sound = Some(sue_io::MovieSound {
                rate: anim::MIX_RATE,
                channels: 2,
            });
        }
        settings
    }

    /// Sets the width, and the height with it while the shape is kept.
    pub(crate) fn set_width(&mut self, width: u32) {
        self.size[0] = width.max(1);
        if self.keep_shape && self.source[0] > 0 {
            self.size[1] = ((u64::from(self.size[0]) * u64::from(self.source[1]))
                / u64::from(self.source[0]))
            .max(1) as u32;
        }
    }

    /// Sets the height, and the width with it while the shape is kept.
    pub(crate) fn set_height(&mut self, height: u32) {
        self.size[1] = height.max(1);
        if self.keep_shape && self.source[1] > 0 {
            self.size[0] = ((u64::from(self.size[1]) * u64::from(self.source[0]))
                / u64::from(self.source[1]))
            .max(1) as u32;
        }
    }
}

/// File, Export Animation, Image Sequence.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SequenceForm {
    /// What each file's name starts with; empty takes the name typed in
    /// the save dialog.
    pub prefix: String,
    /// The number of the first file.
    pub first_number: u32,
    /// How many digits each number has, at least.
    pub digits: u32,
    /// The index of the format in `sue_io::PICTURE_SAVE_TYPES`.
    pub format: usize,
    /// The frames written.
    pub range: Range,
    /// The size each picture is written at, in percent.
    pub scale: u32,
    /// Seen through the 2D camera folders.
    pub camera: bool,
}

impl Default for SequenceForm {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            first_number: 1,
            digits: 4,
            format: 0,
            range: Range::All,
            scale: 100,
            camera: true,
        }
    }
}

impl SequenceForm {
    /// The file name of the picture numbered `number`, with `stem` when
    /// no prefix was given.
    #[must_use]
    pub(crate) fn file_name(&self, stem: &str, number: u32, extension: &str) -> String {
        let prefix = if self.prefix.trim().is_empty() {
            format!("{stem}-")
        } else {
            self.prefix.clone()
        };
        let width = self.digits.clamp(1, 9) as usize;
        format!("{prefix}{number:0width$}.{extension}")
    }

    /// The extension of the format chosen.
    #[must_use]
    pub(crate) fn extension(&self) -> &'static str {
        sue_io::PICTURE_SAVE_TYPES
            .get(self.format)
            .and_then(|(_, extensions)| extensions.first())
            .copied()
            .unwrap_or("png")
    }
}

/// The animated pictures: a GIF, an animated PNG or an animated WebP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Animated {
    Gif,
    Apng,
    WebP,
}

impl Animated {
    /// The kind a File command exports.
    #[must_use]
    pub(crate) fn of_command(command: &str) -> Option<Self> {
        match command {
            "file.export.gif" => Some(Self::Gif),
            "file.export.apng" => Some(Self::Apng),
            "file.export.webp_animation" => Some(Self::WebP),
            _ => None,
        }
    }

    /// The file's extension.
    #[must_use]
    pub(crate) fn extension(self) -> &'static str {
        match self {
            Self::Gif => "gif",
            Self::Apng => "png",
            Self::WebP => "webp",
        }
    }
}

/// Animated GIF, Animated Sticker (APNG) and Animated WebP.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AnimatedForm {
    /// Which picture.
    pub kind: Animated,
    /// Its width and height (pixels).
    pub size: [u32; 2],
    /// The picture's own size, for keeping the shape.
    pub source: [u32; 2],
    /// How many times it plays; 0 for ever (GIF only).
    pub loops: u32,
    /// The frames written.
    pub range: Range,
    /// Frames a second (read from the timeline, shown only).
    pub fps: u32,
    /// Seen through the 2D camera folders.
    pub camera: bool,
}

impl AnimatedForm {
    /// Sets the width, keeping the shape.
    pub(crate) fn set_width(&mut self, width: u32) {
        self.size[0] = width.max(1);
        if self.source[0] > 0 {
            self.size[1] = ((u64::from(self.size[0]) * u64::from(self.source[1]))
                / u64::from(self.source[0]))
            .max(1) as u32;
        }
    }
}

/// Export Animation Cels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CelsForm {
    /// The index of the format in `sue_io::PICTURE_SAVE_TYPES`.
    pub format: usize,
    /// Each animation folder's cels go in a folder of its own.
    pub per_folder: bool,
}

impl Default for CelsForm {
    fn default() -> Self {
        Self {
            format: 0,
            per_folder: true,
        }
    }
}

/// Audio Clip Settings: what changes about one audio clip's sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AudioClipForm {
    /// The audio track.
    pub track: usize,
    /// How loud it plays, in decibels (the clip's volume).
    pub volume_db: f64,
    /// Seconds cut from the start.
    pub cut_start: f64,
    /// Seconds cut from the end.
    pub cut_end: f64,
    /// Seconds of fade in.
    pub fade_in: f64,
    /// Seconds of fade out.
    pub fade_out: f64,
    /// How long the sound is now, in seconds.
    pub seconds: f64,
}

impl AudioClipForm {
    /// The quietest volume a clip is set to, in decibels; a clip plays at
    /// most twice as loud (about +6 dB).
    pub(crate) const QUIETEST_DB: f64 = -60.0;
    /// The loudest volume, in decibels.
    pub(crate) const LOUDEST_DB: f64 = 6.0;

    /// A volume (a factor, 0 to 2) in decibels.
    #[must_use]
    pub(crate) fn decibels(volume: f32) -> f64 {
        if volume <= 0.0 {
            Self::QUIETEST_DB
        } else {
            (20.0 * f64::from(volume).log10()).clamp(Self::QUIETEST_DB, Self::LOUDEST_DB)
        }
    }

    /// A volume in decibels as a factor; the quietest is silence.
    #[must_use]
    pub(crate) fn factor(decibels: f64) -> f32 {
        if decibels <= Self::QUIETEST_DB {
            0.0
        } else {
            10_f64.powf(decibels / 20.0).clamp(0.0, 2.0) as f32
        }
    }

    /// Whether the sound itself changes (a cut or a fade).
    #[must_use]
    pub(crate) fn changes_sound(&self) -> bool {
        self.cut_start > 0.0 || self.cut_end > 0.0 || self.fade_in > 0.0 || self.fade_out > 0.0
    }

    /// The options `sue_io::transform_sound` takes for the cuts and fades.
    #[must_use]
    pub(crate) fn options(&self) -> sue_io::SoundOptions {
        sue_io::SoundOptions {
            trim_start: self.cut_start.max(0.0) as f32,
            trim_end: self.cut_end.max(0.0) as f32,
            fade_in: self.fade_in.max(0.0) as f32,
            fade_out: self.fade_out.max(0.0) as f32,
            ..sue_io::SoundOptions::default()
        }
    }
}

/// A named track label: the track, the frame (from 0), its name and how
/// many frames it covers when Range is ticked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TrackNoteForm {
    /// The track it goes on.
    pub track: usize,
    /// The frame, from 0.
    pub frame: u32,
    /// The name; empty takes the label away.
    pub name: String,
    /// Whether it covers more than its frame.
    pub range: bool,
    /// How many frames it covers when `range` is on.
    pub frames: u32,
}

/// A timeline label: the frame (from 0) and its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LabelForm {
    /// The frame, from 0.
    pub frame: u32,
    /// The name; empty takes the label away.
    pub name: String,
}

/// Insert Frame: how many frames go in after the frame shown, on which
/// tracks, and whether the cel showing there is split around them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InsertForm {
    /// The frame they go in after, from 0.
    pub frame: u32,
    /// How many.
    pub count: u32,
    /// Only on the tracks picked (the frame's track when none is).
    pub selected_only: bool,
    /// The cel showing at the frame shows again after the new frames,
    /// which stay blank, instead of being held through them.
    pub split: bool,
    /// The Timeline palette shows the result while the dialog is open.
    pub preview: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_range_covers_the_frames_it_names_and_stays_on_the_timeline() {
        assert_eq!(Range::All.frames(24, (0, 23)), 0..24);
        assert_eq!(Range::Played.frames(24, (4, 9)), 4..10);
        assert_eq!(Range::Frames(3, 5).frames(24, (0, 23)), 2..5);
        assert_eq!(Range::Frames(30, 40).frames(24, (0, 23)), 23..24);
        assert_eq!(Range::Frames(5, 2).frames(24, (0, 23)), 4..5);
    }

    #[test]
    fn a_sequence_is_named_by_prefix_number_and_digits() {
        let mut form = SequenceForm::default();
        assert_eq!(form.file_name("Walk", 1, "png"), "Walk-0001.png");
        form.prefix = "take_".to_owned();
        form.first_number = 10;
        form.digits = 2;
        assert_eq!(form.file_name("Walk", 10, "jpg"), "take_10.jpg");
        assert_eq!(form.file_name("Walk", 100, "jpg"), "take_100.jpg");
    }

    #[test]
    fn decibels_and_factors_turn_into_each_other() {
        assert!((AudioClipForm::decibels(1.0)).abs() < 1e-9);
        assert!((AudioClipForm::decibels(0.5) + 6.0206).abs() < 1e-3);
        assert_eq!(AudioClipForm::decibels(0.0), AudioClipForm::QUIETEST_DB);
        assert!((AudioClipForm::factor(-6.0206) - 0.5).abs() < 1e-4);
        assert_eq!(AudioClipForm::factor(AudioClipForm::QUIETEST_DB), 0.0);
        assert!(AudioClipForm::factor(12.0) <= 2.0);
    }

    #[test]
    fn a_movie_keeps_its_shape_when_one_side_changes() {
        let mut form = MovieForm {
            formats: vec![MovieFormat::MjpegAvi],
            format: 0,
            size: [1920, 1080],
            source: [1920, 1080],
            keep_shape: true,
            fps: 24,
            quality: MovieQuality::Normal,
            camera: true,
            sound: true,
            has_sound: true,
            range: Range::All,
        };
        form.set_width(1280);
        assert_eq!(form.size, [1280, 720]);
        form.keep_shape = false;
        form.set_height(500);
        assert_eq!(form.size, [1280, 500]);
        // The AVI carries no sound, whatever the box says.
        assert!(form.settings().sound.is_none());
    }
}
