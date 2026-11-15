//! The Timeline palette, and Sue Paint's side of animation (Clip Studio
//! Paint's Animation menu and Timeline palette): making timelines,
//! folders and cels, stepping and playing frames, editing the frames and
//! clips, onion skin, sound, and exporting a movie, an animated picture,
//! a sequence of pictures, the cels or the sound.

mod audio;
mod bar;
mod edit;
mod export;
mod exposure;
pub(crate) mod forms;
pub(crate) mod menu;
mod playback;
mod tracks;
mod view;

pub(crate) use bar::icon;
pub(crate) use edit::Action;
pub(crate) use view::TimelineView;

use std::collections::HashMap;

use egui::{Rect, Ui, pos2};
use sue_core::EntityId;
use sue_wm::export::{ExportDialog, SoundSettings};
use sue_wm::ui::Density;
use sue_wm::{AppletContext, Theme};

use crate::anim::editing::{Clipboard, Selection};
use crate::icons::Icon;
use crate::panels::header;
use crate::studio::SuePaint;

pub(crate) type Edit = sue_core::txn::Batch<sue_core::Arena>;

/// The Timeline palette's own commands, in the order its menu reads them
/// ([`menu::TREE`], which lays them out in submenus; a test keeps the
/// two the same). `Palette::menu_commands` reads it.
pub(crate) const MENU: &[&str] = &[
    "anim.layer.folder",
    "anim.layer.camera",
    "anim.cel.new",
    "anim.cel.assign",
    "anim.cel.rename_timeline",
    "anim.cel.rename_layer",
    "anim.cel.prev",
    "anim.cel.next",
    "anim.key.add",
    "anim.key.delete",
    "anim.key.layer",
    "anim.keyframes",
    "anim.frame.insert",
    "anim.frame.delete",
    "anim.play",
    "anim.loop",
    "anim.audio.scrub",
    "anim.play_range.start",
    "anim.play_range.end",
    "anim.play_range.clear",
    "anim.first",
    "anim.prev",
    "anim.next",
    "anim.last",
    "anim.label",
    "anim.track_label.inbetween",
    "anim.track_label.none",
    "anim.timeline.enable",
    "anim.timeline.new",
    "anim.timeline.prev",
    "anim.timeline.next",
    "anim.timeline.settings",
    "anim.timeline.edit",
    "anim.exposure_sheet",
    "anim.onion",
    "anim.onion_settings",
    "anim.light_table",
    "anim.light_table.register",
    "anim.light_table.register_general",
    "anim.light_table.clear",
    "anim.thumbnails.none",
    "anim.thumbnails.smallest",
    "anim.thumbnails.small",
    "anim.thumbnails.medium",
    "anim.thumbnails.large",
    "anim.thumbnails.largest",
    "anim.time_unit.frames",
    "anim.time_unit.seconds",
    "anim.time_unit.timecode",
    "anim.audio.settings",
    "anim.export.audio",
];

/// How big the cels' pictures on the timeline are, as Clip Studio Paint's
/// Thumbnail Size: None to Largest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Thumbs {
    None,
    Smallest,
    Small,
    #[default]
    Medium,
    Large,
    Largest,
}

impl Thumbs {
    /// Every size, smallest first, with its stable name.
    pub(crate) const ALL: [(Self, &'static str); 6] = [
        (Self::None, "none"),
        (Self::Smallest, "smallest"),
        (Self::Small, "small"),
        (Self::Medium, "medium"),
        (Self::Large, "large"),
        (Self::Largest, "largest"),
    ];

    /// The size of a stable name.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, known)| *known == name)
            .map(|(size, _)| *size)
    }

    /// How tall a cel's picture is, in rows of the density (0 for None).
    pub(crate) fn rows(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Smallest => 0.75,
            Self::Small => 1.0,
            Self::Medium => 1.5,
            Self::Large => 2.0,
            Self::Largest => 3.0,
        }
    }
}

/// How times read on the timeline: frames, seconds and frames, or time
/// code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TimeUnit {
    #[default]
    Frames,
    Seconds,
    Timecode,
}

impl TimeUnit {
    /// The unit of a stable name.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "frames" => Some(Self::Frames),
            "seconds" => Some(Self::Seconds),
            "timecode" => Some(Self::Timecode),
            _ => None,
        }
    }

    /// Frame `frame` (from 0) at `fps` as it reads in this unit: `25`,
    /// `1+01` (one second and one frame) or `00:00:01:01`.
    pub(crate) fn show(self, frame: u32, fps: u32) -> String {
        let fps = fps.max(1);
        match self {
            Self::Frames => (frame + 1).to_string(),
            Self::Seconds => format!("{}+{:02}", frame / fps, frame % fps + 1),
            Self::Timecode => {
                let seconds = frame / fps;
                format!(
                    "{:02}:{:02}:{:02}:{:02}",
                    seconds / 3600,
                    seconds / 60 % 60,
                    seconds % 60,
                    frame % fps
                )
            }
        }
    }
}

/// A press on the timeline's cells being dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Press {
    /// The track and frame pressed.
    pub track: usize,
    pub frame: u32,
    /// It started on a picked frame: the drag moves what is picked.
    pub moving: bool,
    /// It started on the first frame of a cel picked alone: the drag
    /// moves where that cel starts, between the keys around it (the
    /// frames it may start on), so the holds around it change.
    pub key: Option<(u32, u32)>,
}

/// What the Timeline palette keeps between frames besides its view.
#[derive(Default)]
pub(crate) struct AnimUi {
    /// The frames picked, on their tracks.
    pub selection: Option<Selection>,
    /// What Cut or Copy took.
    pub clipboard: Clipboard,
    /// The settings an export's dialog kept, until its file is picked.
    pub pending: export::Pending,
    /// The sound export dialog, while it is open.
    pub sound_export: Option<ExportDialog>,
    /// The sound page's settings, as last kept.
    pub sound_settings: Option<SoundSettings>,
    /// The audio track last picked.
    pub audio_track: Option<usize>,
    /// How big the cels' pictures are.
    pub thumbs: Thumbs,
    /// How times read.
    pub unit: TimeUnit,
    /// A press on the cells being dragged.
    pub press: Option<Press>,
    /// The layer picked before the last click on a cell (the click picks
    /// the cel shown there), so a double click shows that layer there.
    pub picked_before: Option<EntityId>,
    /// A playback marker being dragged: the start (`true`) or the end.
    pub marker: Option<bool>,
    /// Each sound's waveform: its revision, and its peaks.
    pub peaks: HashMap<EntityId, (u64, Vec<f32>)>,
    /// Playback measured: when the frame last changed, and the frames a
    /// second seen over the last second.
    pub clock: Option<(std::time::Instant, u32)>,
    /// The frames a second playback reached.
    pub measured: f64,
    /// The Animation Cels palette's picked entry, held slider and drop
    /// places.
    pub cels: crate::panels::cels::CelsUi,
    /// Audio Scrubbing: moving through the frames by hand plays the sound
    /// of each frame reached.
    pub scrub: bool,
    /// The frame shown when the frames were last looked at, so a move by
    /// hand is told from standing still.
    pub scrubbed: Option<u32>,
}

/// The keys that run `command` now, as people read them (`Ctrl+K`), or
/// nothing.
pub(crate) fn command_keys(rice: &sue_rice::RiceSchema, command: &str) -> String {
    let registry = crate::commands::registry();
    let Some((entry, _)) = registry.find(command) else {
        return String::new();
    };
    let chords = sue_wm::shortcuts::chords_among(
        rice,
        crate::studio::KEYS_SECTION,
        command,
        &entry.default_chords(),
        &sue_wm::shortcuts::SparseKeys::read(rice, crate::studio::KEYS_SECTION),
    );
    chords.to_string()
}

impl SuePaint {
    /// The Timeline palette: its header, the bar of buttons, then the
    /// tracks.
    pub(crate) fn timeline_panel(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::ZERO, theme.panel);
        ui.painter()
            .hline(rect.x_range(), rect.top(), theme.hairline());
        let (body, _) = header(
            ui,
            rect,
            &[(tr!("timeline"), Icon::Timeline, true)],
            0,
            theme,
        );
        let tall = Density::of(ui).icon_button() + theme.space(sue_wm::ui::Space::S2);
        let bar = Rect::from_min_max(body.min, pos2(body.right(), body.top() + tall));
        ui.painter()
            .hline(body.x_range(), bar.bottom(), theme.hairline());
        let tracks = Rect::from_min_max(pos2(body.left(), bar.bottom()), body.max);
        self.timeline_bar(ui, bar, cx, theme);
        self.timeline_ui(ui, tracks, cx, theme);
    }
}

#[cfg(test)]
mod tests {
    use super::{Thumbs, TimeUnit};

    #[test]
    fn times_read_in_frames_seconds_and_time_code() {
        assert_eq!(TimeUnit::Frames.show(24, 24), "25");
        assert_eq!(TimeUnit::Seconds.show(24, 24), "1+01");
        assert_eq!(TimeUnit::Seconds.show(5, 24), "0+06");
        assert_eq!(TimeUnit::Timecode.show(24 * 61 + 3, 24), "00:01:01:03");
    }

    #[test]
    fn every_thumbnail_size_has_a_stable_name_and_none_is_flat() {
        for (size, name) in Thumbs::ALL {
            assert_eq!(Thumbs::from_name(name), Some(size));
        }
        assert_eq!(Thumbs::None.rows(), 0.0);
        assert!(Thumbs::Largest.rows() > Thumbs::Medium.rows());
    }
}
