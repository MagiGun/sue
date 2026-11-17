//! Editing the timeline: what the palette's clicks, drags and menus ask
//! for ([`Action`]), and the timeline commands that do the same through
//! the registry (cut, copy, paste and delete of picked frames, clips,
//! labels, the playback range, keyframes, New Timeline and Change
//! Settings). Every change to the picture is one undo step.

use sue_core::txn::SetProperty;
use sue_core::{EntityId, Time};
use sue_model::Sequence;
use sue_wm::AppletContext;

use super::Edit;
use super::forms::{InsertForm, LabelForm, TimelineForm};
use crate::anim;
use crate::anim::editing::{self, Selection, TrackLabel};
use crate::anim::motion::Ease;
use crate::dialogs::{AnimDialog, Dialog};
use crate::studio::SuePaint;

/// What the Timeline palette asked for in a frame, done once it is drawn.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Action {
    /// Go to `frame` on track `track` (a click on a cell).
    Go { track: usize, frame: u32 },
    /// Show `cel` (or a blank, `None`) on `track` from `frame`.
    Assign {
        track: usize,
        frame: u32,
        cel: Option<EntityId>,
    },
    /// Pick frames on tracks.
    Select(Selection),
    /// Move what is picked to start at `frame`.
    MoveTo(u32),
    /// Move the key of `track` that starts on `from` to start on `to`
    /// (a drag of a picked cel's first frame): the holds around it change.
    MoveKey { track: usize, from: u32, to: u32 },
    /// Move the keyframe of `layer` (a layer, folder or camera folder) on
    /// frame `from` to frame `to` (a drag of its mark).
    MoveKeyframe { layer: EntityId, from: u32, to: u32 },
    /// Show or hide a track's layer or folder.
    Eye(usize, bool),
    /// Move an audio track's clip to start at a frame.
    AudioTo(usize, i64),
    /// Mute or unmute an audio track.
    AudioMute(usize),
    /// Take an audio track away.
    AudioDelete(usize),
    /// Audio Clip Settings of an audio track.
    AudioSettings(usize),
    /// Make the timeline kept aside in `Some(entity)` the one shown.
    Switch(EntityId),
    /// Open (`true`) or fold a folder's rows, as the Layer palette's fold
    /// arrow does (one undo step).
    Fold(EntityId, bool),
}

impl SuePaint {
    /// What is picked on the timeline now: the selection, else the frame
    /// shown on the current track.
    pub(crate) fn timeline_selection(&self, sequence: &Sequence) -> Selection {
        self.anim_ui
            .selection
            .clone()
            .filter(|selection| {
                !selection.tracks.is_empty()
                    && selection
                        .tracks
                        .iter()
                        .all(|track| *track < sequence.tracks.len())
            })
            .unwrap_or_else(|| {
                let (frame, track) = self.doc().map_or((0, 0), |doc| (doc.frame, doc.track));
                Selection::dragged(vec![track], frame, frame)
            })
    }

    /// Does what the palette asked for.
    pub(crate) fn timeline_action(&mut self, cx: &mut AppletContext<'_>, action: Action) {
        let Some((picture, sequence, length)) = self.timeline_here(&cx.world.arena) else {
            return;
        };
        match action {
            Action::Go { track, frame } => {
                if let Some(doc) = self.doc_mut() {
                    doc.track = track;
                }
                self.go_to_frame(&cx.world.arena, i64::from(frame));
                // A blank there picks the folder itself, as the Layer
                // palette does, so the two stay in step; a layer track
                // picks its layer.
                let target = sequence.tracks.get(track).and_then(|row| {
                    anim::cel_at(row, sequence.rate, frame)
                        .filter(|_| row.kind == anim::CELS)
                        .or_else(|| anim::track_folder(row))
                        .or_else(|| anim::setup::track_layer(row))
                });
                if let (Some(target), Some(doc)) = (target, self.doc_mut()) {
                    doc.layer = Some(target);
                }
            }
            Action::Assign { track, frame, cel } => {
                if let Some(doc) = self.doc_mut() {
                    doc.track = track;
                }
                if cel.is_some_and(|cel| !anim::may_show(&cx.world.arena, &sequence, track, cel)) {
                    cx.notify(tr!("only-cels-of-this-animation-folder"));
                } else {
                    let changed = anim::assign(&sequence, track, frame, cel, picture, length);
                    Self::apply(cx, anim::set(picture, changed, &tr!("specify-cels")));
                }
                self.go_to_frame(&cx.world.arena, i64::from(frame));
            }
            Action::Select(selection) => {
                if let Some(track) = selection.tracks.first().copied()
                    && let Some(doc) = self.doc_mut()
                {
                    doc.track = track;
                }
                self.anim_ui.selection = Some(selection);
            }
            Action::MoveTo(to) => {
                let selection = self.timeline_selection(&sequence);
                if to == selection.start {
                    return;
                }
                let (moved, new_length) =
                    editing::move_range(&sequence, &selection, to, length, picture);
                let mut edit = anim::set(picture, moved, &tr!("move-cels"));
                if new_length != length {
                    edit = edit.then(SetProperty::new(
                        picture,
                        anim::LENGTH,
                        i64::from(new_length),
                    ));
                }
                if Self::apply(cx, edit) {
                    self.anim_ui.selection = Some(Selection {
                        tracks: selection.tracks.clone(),
                        start: to,
                        end: to + selection.span(),
                    });
                    self.go_to_frame(&cx.world.arena, i64::from(to));
                }
            }
            Action::MoveKey { track, from, to } => {
                // Kept between the keys around it, as the drag showed.
                let Some(range) = editing::key_moves(&sequence, track, from, length) else {
                    return;
                };
                let to = to.clamp(*range.start(), *range.end());
                let Some(moved) = editing::move_key(&sequence, track, from, to, length) else {
                    return;
                };
                if Self::apply(cx, anim::set(picture, moved, &tr!("move-cels"))) {
                    self.anim_ui.selection = Some(Selection::dragged(vec![track], to, to));
                    self.go_to_frame(&cx.world.arena, i64::from(to));
                }
            }
            Action::MoveKeyframe { layer, from, to } => {
                let time = |frame: u32| Time::from_frames(i64::from(frame), sequence.rate);
                if let Some(edit) =
                    anim::motion::move_key(&cx.world.arena, layer, time(from), time(to))
                    && Self::apply(cx, edit)
                {
                    self.go_to_frame(&cx.world.arena, i64::from(to));
                }
            }
            Action::Eye(track, visible) => {
                match sequence.tracks.get(track).and_then(|row| {
                    anim::track_folder(row).or_else(|| anim::setup::track_layer(row))
                }) {
                    Some(layer) => {
                        Self::apply(
                            cx,
                            crate::doc::set(layer, sue_model::picture::VISIBLE, visible),
                        );
                    }
                    None => {
                        let mut toggled = sequence.clone();
                        toggled.tracks[track].muted = !visible;
                        Self::apply(
                            cx,
                            anim::set(picture, toggled, &tr!("show-or-hide-the-animation-folder")),
                        );
                    }
                }
            }
            Action::AudioTo(track, frame) => {
                Self::apply(
                    cx,
                    anim::set(
                        picture,
                        anim::audio_moved(&sequence, track, frame),
                        &tr!("move-audio"),
                    ),
                );
            }
            Action::AudioMute(track) => {
                let mut toggled = sequence.clone();
                let muted = toggled.tracks[track].muted;
                toggled.tracks[track].muted = !muted;
                let label = if muted {
                    tr!("unmute-audio")
                } else {
                    tr!("mute-audio")
                };
                Self::apply(cx, anim::set(picture, toggled, &label));
            }
            Action::AudioDelete(track) => {
                Self::apply(
                    cx,
                    anim::set(
                        picture,
                        anim::without_track(&sequence, track),
                        &tr!("delete-audio-track"),
                    ),
                );
            }
            Action::AudioSettings(track) => {
                self.anim_ui.audio_track = Some(track);
                self.ask_audio_clip(cx, track);
            }
            Action::Fold(folder, open) => {
                Self::apply(cx, crate::doc::set(folder, crate::doc::OPEN, open));
            }
            Action::Switch(aside) => {
                if let Some(edit) = anim::setup::switch_timeline(&cx.world.arena, picture, aside) {
                    Self::apply(cx, edit);
                    self.anim_ui.selection = None;
                    self.go_to_frame(&cx.world.arena, 0);
                }
            }
        }
    }

    /// Edit, Cut, Copy, Paste and Delete on the timeline (`what`): the
    /// frames picked, or the frame shown on the current track.
    pub(crate) fn timeline_clipboard(&mut self, cx: &mut AppletContext<'_>, what: &str) {
        let Some((picture, sequence, length)) = self.timeline_here(&cx.world.arena) else {
            cx.fail(tr!("make-timeline-first-animation-timeline-new"));
            return;
        };
        let selection = self.timeline_selection(&sequence);
        // The keyframes of what each track picked stands for go with its
        // frames.
        let target = |index: usize| {
            sequence.tracks.get(index).and_then(|track| {
                anim::track_folder(track).or_else(|| anim::setup::track_layer(track))
            })
        };
        let keys: Vec<Vec<anim::motion::CopiedKey>> = selection
            .tracks
            .iter()
            .map(|index| {
                target(*index).map_or_else(Vec::new, |layer| {
                    anim::motion::copy_keys(
                        &cx.world.arena,
                        layer,
                        sequence.rate,
                        selection.start,
                        selection.end,
                    )
                })
            })
            .collect();
        match what {
            "copy" => {
                self.anim_ui.clipboard = editing::copy(&sequence, &selection);
                self.anim_ui.key_clipboard = keys;
            }
            "cut" => {
                let (changed, clipboard) = editing::cut(&sequence, &selection, length, picture);
                if Self::apply(cx, anim::set(picture, changed, &tr!("cut-frames"))) {
                    self.anim_ui.clipboard = clipboard;
                    self.anim_ui.key_clipboard = keys;
                }
            }
            "delete" => {
                let changed = editing::clear(&sequence, &selection, length, picture);
                Self::apply(cx, anim::set(picture, changed, &tr!("delete-frames")));
            }
            _ => {
                let keyed = self.anim_ui.key_clipboard.iter().any(|row| !row.is_empty());
                if self.anim_ui.clipboard.is_empty() && !keyed {
                    return;
                }
                // Paste lands at the frame shown, on the tracks picked.
                let at = self.doc().map_or(selection.start, |doc| doc.frame);
                let arena = &cx.world.arena;
                let (changed, new_length) = editing::paste(
                    &sequence,
                    &self.anim_ui.clipboard,
                    &selection.tracks,
                    at,
                    length,
                    picture,
                    |track, cel| anim::may_show(arena, &sequence, track, cel),
                );
                let mut edit = anim::set(picture, changed, &tr!("paste-frames"));
                if new_length != length {
                    edit = edit.then(SetProperty::new(
                        picture,
                        anim::LENGTH,
                        i64::from(new_length),
                    ));
                }
                for (row, index) in self.anim_ui.key_clipboard.iter().zip(&selection.tracks) {
                    if let Some(layer) = target(*index)
                        && let Some(keys) =
                            anim::motion::paste_keys(arena, layer, sequence.rate, at, row)
                    {
                        edit = edit.then(keys);
                    }
                }
                Self::apply(cx, edit);
            }
        }
    }

    /// Split Clip, Merge Clips, Set as First and Last Displayed Frame
    /// (`what`) on the current track at the frame shown (Merge: over the
    /// frames picked).
    pub(crate) fn timeline_clip(&mut self, cx: &mut AppletContext<'_>, what: &str) {
        let Some((picture, sequence, length)) = self.timeline_here(&cx.world.arena) else {
            cx.fail(tr!("make-timeline-first-animation-timeline-new"));
            return;
        };
        let (frame, track) = self.doc().map_or((0, 0), |doc| (doc.frame, doc.track));
        let (changed, label) = match what {
            "split" => (
                editing::split_clip(&sequence, track, frame, length),
                tr!("split-clip"),
            ),
            "merge" => {
                let selection = self.timeline_selection(&sequence);
                (
                    editing::merge_clips(
                        &sequence,
                        track,
                        selection.start,
                        selection.end,
                        length,
                        picture,
                    ),
                    tr!("merge-clips"),
                )
            }
            "first" => (
                editing::set_first_displayed(&sequence, track, frame, length, picture),
                tr!("set-as-first-displayed-frame"),
            ),
            _ => (
                editing::set_last_displayed(&sequence, track, frame, length, picture),
                tr!("set-as-last-displayed-frame"),
            ),
        };
        match changed {
            Some(changed) => {
                Self::apply(cx, anim::set(picture, changed, &label));
            }
            None => cx.notify(tr!("nothing-to-change-here")),
        }
    }

    /// A track label (`kind`: `inbetween`, `inbetween-done`,
    /// `reverse-sheet`, or `none` to take it away) on the current track at
    /// the frame shown.
    pub(crate) fn timeline_track_label(&mut self, cx: &mut AppletContext<'_>, kind: &str) {
        let Some((picture, sequence, _)) = self.timeline_here(&cx.world.arena) else {
            return;
        };
        let (frame, track) = self.doc().map_or((0, 0), |doc| (doc.frame, doc.track));
        let label = TrackLabel::from_name(kind);
        let changed = editing::with_track_label(&sequence, track, frame, label);
        if changed != sequence {
            Self::apply(cx, anim::set(picture, changed, &tr!("track-label")));
        }
    }

    /// Opens a timeline label's name at the frame shown.
    pub(crate) fn ask_label(&mut self, cx: &mut AppletContext<'_>) {
        let Some(picture) = self.picture() else {
            return;
        };
        let frame = self.doc().map_or(0, |doc| doc.frame);
        let name = anim::setup::labels(&cx.world.arena, picture)
            .into_iter()
            .find(|label| label.frame == frame)
            .map(|label| label.name)
            .unwrap_or_default();
        self.dialog = Some(Dialog::Anim(Box::new(AnimDialog::Label(LabelForm {
            frame,
            name,
        }))));
    }

    /// Insert Frame asks how many frames, on which tracks and how.
    pub(crate) fn ask_insert_frames(&mut self, cx: &mut AppletContext<'_>) {
        if self.timeline_here(&cx.world.arena).is_none() {
            cx.fail(tr!("make-timeline-first-animation-timeline-new"));
            return;
        }
        let frame = self.doc().map_or(0, |doc| doc.frame);
        self.dialog = Some(Dialog::Anim(Box::new(AnimDialog::Insert(InsertForm {
            frame,
            count: 1,
            selected_only: false,
            split: false,
            preview: true,
        }))));
    }

    /// Inserts the frames `form` asks for, one undo step: the keys after
    /// its frame move on, on every track or the picked ones; with Split
    /// Clip the new frames are blank and the cel showing at the frame
    /// shows again after them.
    pub(crate) fn insert_frames(&mut self, cx: &mut AppletContext<'_>, form: &InsertForm) {
        let Some((picture, out, new_length)) = self.inserted(&cx.world.arena, form) else {
            return;
        };
        let edit = Edit::new(tr!("insert-frame"))
            .then(sue_core::txn::SetExtension::set(picture, out))
            .then(SetProperty::new(
                picture,
                anim::LENGTH,
                i64::from(new_length),
            ));
        Self::apply(cx, edit);
    }

    /// The timeline as Insert Frame with `form` leaves it: its picture,
    /// the new timeline and its length (what the dialog's Preview shows).
    pub(crate) fn inserted(
        &self,
        arena: &sue_core::Arena,
        form: &InsertForm,
    ) -> Option<(EntityId, Sequence, u32)> {
        let (picture, sequence, length) = self.timeline_here(arena)?;
        let count = form.count.max(1);
        let new_length = length + count;
        let picked = form
            .selected_only
            .then(|| self.timeline_selection(&sequence).tracks);
        let mut out = editing::shift_tracks(
            &sequence,
            form.frame,
            i64::from(count),
            new_length,
            picked.as_deref(),
        );
        if form.split {
            let tracks: Vec<usize> = picked.clone().unwrap_or_else(|| anim::folders(&sequence));
            for track in tracks {
                let Some(row) = sequence
                    .tracks
                    .get(track)
                    .filter(|row| row.kind == anim::CELS)
                else {
                    continue;
                };
                let Some(cel) = anim::cel_at(row, sequence.rate, form.frame) else {
                    continue;
                };
                let after = form.frame + 1;
                let next_key = anim::keys(&out.tracks[track], out.rate)
                    .into_iter()
                    .any(|(at, _)| at == after + count);
                out = anim::assign(&out, track, after, None, picture, new_length);
                if !next_key && after + count < new_length {
                    out = anim::assign(&out, track, after + count, Some(cel), picture, new_length);
                }
            }
        }
        Some((picture, out, new_length))
    }

    /// The timeline the palette shows: the one there, or while the Insert
    /// Frame dialog previews, the one it would leave.
    pub(crate) fn timeline_shown(
        &self,
        arena: &sue_core::Arena,
    ) -> Option<(EntityId, Sequence, u32, bool)> {
        if let Some(crate::dialogs::Dialog::Anim(dialog)) = &self.dialog
            && let crate::dialogs::AnimDialog::Insert(form) = dialog.as_ref()
            && form.preview
            && let Some((picture, sequence, length)) = self.inserted(arena, form)
        {
            return Some((picture, sequence, length, true));
        }
        self.timeline_here(arena)
            .map(|(picture, sequence, length)| (picture, sequence, length, false))
    }

    /// The frames Insert Frame's Preview adds: the first, and how many.
    pub(crate) fn insert_preview(&self) -> Option<(u32, u32)> {
        match &self.dialog {
            Some(crate::dialogs::Dialog::Anim(dialog)) => match dialog.as_ref() {
                crate::dialogs::AnimDialog::Insert(form) if form.preview => {
                    Some((form.frame + 1, form.count.max(1)))
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Keeps a timeline label's name; an empty one takes it away.
    pub(crate) fn label_ok(&mut self, cx: &mut AppletContext<'_>, form: &LabelForm) {
        let Some(picture) = self.picture() else {
            return;
        };
        let name = form.name.trim();
        if let Some(edit) = anim::setup::set_label(
            &cx.world.arena,
            picture,
            form.frame,
            (!name.is_empty()).then_some(name),
        ) {
            Self::apply(cx, edit);
        }
    }

    /// The playback range starts (`start`) or ends here, or covers every
    /// frame again (`clear`).
    pub(crate) fn timeline_play_range(&mut self, cx: &mut AppletContext<'_>, what: &str) {
        let Some(picture) = self.picture() else {
            return;
        };
        let Some((_, _, length)) = self.timeline_here(&cx.world.arena) else {
            cx.fail(tr!("make-timeline-first-animation-timeline-new"));
            return;
        };
        let frame = self.doc().map_or(0, |doc| doc.frame);
        let mut settings = anim::setup::Settings::of(&cx.world.arena, picture);
        let (start, end) = settings.range(length);
        match what {
            "start" => {
                settings.start = frame;
                settings.end = (end.max(frame) < length - 1).then_some(end.max(frame));
            }
            "end" => {
                settings.start = start.min(frame);
                settings.end = (frame < length - 1).then_some(frame);
            }
            _ => {
                settings.start = 0;
                settings.end = None;
            }
        }
        if settings != anim::setup::Settings::of(&cx.world.arena, picture) {
            Self::apply(cx, settings.apply(picture));
        }
    }

    /// Moves a playback marker by a drag: the start (`true`) or the end to
    /// `frame`.
    pub(crate) fn drag_play_marker(&mut self, cx: &mut AppletContext<'_>, start: bool, frame: u32) {
        let Some((picture, _, length)) = self.timeline_here(&cx.world.arena) else {
            return;
        };
        let mut settings = anim::setup::Settings::of(&cx.world.arena, picture);
        let (first, last) = settings.range(length);
        let frame = frame.min(length.saturating_sub(1));
        if start {
            settings.start = frame.min(last);
        } else {
            let end = frame.max(first);
            settings.end = (end < length - 1).then_some(end);
        }
        let now = anim::setup::Settings::of(&cx.world.arena, picture);
        if settings != now {
            // Shown at once while the drag goes on; kept as one step when it
            // ends (end_play_marker).
            let mut edit = settings.apply(picture);
            if sue_core::ReversibleTransaction::apply(&mut edit, &mut cx.world.arena).is_ok() {
                self.anim_ui.marker_live.push(edit);
            }
        }
    }

    /// Keeps where a playback marker was dragged to as one undo step.
    pub(crate) fn end_play_marker(&mut self, cx: &mut AppletContext<'_>) {
        let live = std::mem::take(&mut self.anim_ui.marker_live);
        let Some(picture) = self.picture().filter(|_| !live.is_empty()) else {
            return;
        };
        let to = anim::setup::Settings::of(&cx.world.arena, picture);
        for mut edit in live.into_iter().rev() {
            let _ = sue_core::ReversibleTransaction::revert(&mut edit, &mut cx.world.arena);
        }
        if to != anim::setup::Settings::of(&cx.world.arena, picture) {
            Self::apply(cx, to.apply(picture));
        }
    }

    /// New Timeline (a dialog) or Change Settings (`new` false).
    pub(crate) fn ask_timeline_form(&mut self, cx: &mut AppletContext<'_>, new: bool) {
        let Some(picture) = self.picture() else {
            return;
        };
        let form = if new {
            Some(TimelineForm::new_timeline(&cx.world.arena, picture))
        } else {
            TimelineForm::of(&cx.world.arena, picture)
        };
        match form {
            Some(form) => {
                self.dialog = Some(Dialog::Anim(Box::new(AnimDialog::Timeline(form))));
            }
            None => cx.fail(tr!("make-timeline-first-animation-timeline-new")),
        }
    }

    /// Keeps New Timeline or Change Settings, as one undo step. A new
    /// timeline gives every layer a track and makes no folder; one there
    /// already stays one pick away in the timeline list.
    pub(crate) fn timeline_form_ok(&mut self, cx: &mut AppletContext<'_>, form: &TimelineForm) {
        let Some(picture) = self.picture() else {
            return;
        };
        let settings = form.settings();
        let edit = if form.new {
            anim::setup::create_timeline(&cx.world.arena, picture, form.fps, form.length, &settings)
        } else {
            let mut edit = Edit::new(tr!("change-settings")).then(settings.apply(picture));
            if let Some(retimed) =
                anim::edit_timeline(&cx.world.arena, picture, form.fps, form.length)
            {
                edit = edit.then(retimed);
            }
            edit
        };
        if Self::apply(cx, edit) {
            if form.new {
                self.panels.timeline = true;
                self.timeline_on = true;
                self.anim_ui.selection = None;
                if let Some(doc) = self.doc_mut() {
                    doc.track = 0;
                    doc.frame = 0;
                }
            }
            self.go_to_frame(&cx.world.arena, 0);
        }
    }

    /// Keys the current layer at the frame shown: its pose there, Smooth.
    /// Keyframes are switched on for it first when they were off.
    pub(crate) fn key_layer_here(&mut self, cx: &mut AppletContext<'_>) {
        let (Some(picture), Some(layer)) = (self.picture(), self.layer()) else {
            return;
        };
        let Some((_, sequence, _)) = self.timeline_here(&cx.world.arena) else {
            cx.fail(tr!("make-timeline-first-animation-timeline-new"));
            return;
        };
        // The 2D camera picked: its key, of the camera as it stands.
        if crate::camera::is_camera(&cx.world.arena, layer) {
            let camera = self.camera_now(&cx.world.arena, layer);
            let (_, time) = self.frame_time(&cx.world.arena);
            Self::apply(cx, crate::camera::key(&cx.world.arena, layer, time, camera));
            self.camera_pending = None;
            return;
        }
        let arena = &cx.world.arena;
        let size = arena
            .get_by_id(picture)
            .ok()
            .and_then(sue_model::picture::size)
            .unwrap_or([0, 0]);
        let frame = self.doc().map_or(0, |doc| doc.frame);
        let time = Time::from_frames(i64::from(frame), sequence.rate);
        let pose = anim::motion::pose(arena, layer, time, size)
            .unwrap_or_else(|| anim::motion::Pose::still(size));
        let on = arena.get_by_id(layer).is_ok_and(anim::motion::keyframes_on);
        let mut edit = Edit::new(tr!("add-keyframe"));
        if !on {
            edit = edit.then(anim::motion::enable_keyframes(layer, true));
        }
        Self::apply(
            cx,
            edit.then(anim::motion::key_pose(
                arena,
                layer,
                time,
                &pose,
                Ease::Smooth,
            )),
        );
    }

    /// How the current layer's key at the frame shown moves on.
    pub(crate) fn ease_here(&mut self, cx: &mut AppletContext<'_>, ease: &str) {
        let (Some(layer), Some(ease)) = (self.layer(), Ease::from_name(ease)) else {
            return;
        };
        let Some((_, sequence, _)) = self.timeline_here(&cx.world.arena) else {
            return;
        };
        let frame = self.doc().map_or(0, |doc| doc.frame);
        let time = Time::from_frames(i64::from(frame), sequence.rate);
        let edit = if crate::camera::is_camera(&cx.world.arena, layer) {
            crate::camera::set_ease(&cx.world.arena, layer, time, ease)
        } else {
            anim::motion::set_ease(&cx.world.arena, layer, &[time], ease)
        };
        match edit {
            Some(edit) => {
                Self::apply(cx, edit);
            }
            None => cx.notify(tr!("no-keyframe-on-this-frame")),
        }
    }

    /// Takes the current layer's key (or the 2D camera's) at the frame
    /// shown away.
    pub(crate) fn delete_key_here(&mut self, cx: &mut AppletContext<'_>) {
        let Some(layer) = self.layer() else {
            return;
        };
        let Some((_, sequence, _)) = self.timeline_here(&cx.world.arena) else {
            return;
        };
        let frame = self.doc().map_or(0, |doc| doc.frame);
        let time = Time::from_frames(i64::from(frame), sequence.rate);
        let arena = &cx.world.arena;
        let edit = if crate::camera::is_camera(arena, layer) {
            crate::camera::delete_key(arena, layer, time)
        } else {
            anim::motion::delete_key(arena, layer, time, None)
        };
        match edit {
            Some(edit) => {
                Self::apply(cx, edit);
                self.camera_pending = None;
            }
            None => cx.notify(tr!("no-keyframe-on-this-frame")),
        }
    }

    /// Switches keyframes on or off for the current layer.
    pub(crate) fn toggle_keyframes(&mut self, cx: &mut AppletContext<'_>) {
        let Some(layer) = self.layer() else {
            return;
        };
        let on = cx
            .world
            .arena
            .get_by_id(layer)
            .is_ok_and(anim::motion::keyframes_on);
        Self::apply(cx, anim::motion::enable_keyframes(layer, !on));
    }
}
