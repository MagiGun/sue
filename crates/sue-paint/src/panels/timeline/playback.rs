//! The timeline in the studio: the cel being drawn, the frame shown,
//! playing with its sound, and the light table's file.

use std::path::Path;

use sue_core::EntityId;
use sue_core::txn::{Rename, SetProperty};
use sue_model::Sequence;
use sue_wm::AppletContext;

use super::Edit;
use crate::anim;
use crate::doc::{self, NewPicture};
use crate::studio::SuePaint;

impl SuePaint {
    /// The current picture's timeline, cloned, and its length.
    /// The cel being drawn: the one the current folder shows at the
    /// current frame.
    pub(crate) fn editing_cel(&self, arena: &sue_core::Arena) -> Option<EntityId> {
        // A locked cel stays the one being drawn on every frame.
        if let Some(cel) = self.cel_lock.filter(|cel| arena.get_by_id(*cel).is_ok()) {
            return Some(cel);
        }
        let (_, sequence, _) = self.timeline_here(arena)?;
        let doc = self.doc()?;
        anim::folders(&sequence)
            .contains(&doc.track)
            .then(|| anim::cel_at(&sequence.tracks[doc.track], sequence.rate, doc.frame))
            .flatten()
    }

    /// The animation folder a new cel goes into: the one picked in the
    /// Layer palette (or the one the picked cel is in), else the one of
    /// the timeline's current track `track`, else the first.
    pub(crate) fn cel_target(
        &self,
        arena: &sue_core::Arena,
        sequence: &Sequence,
        track: usize,
    ) -> Option<EntityId> {
        let of_track = |index: usize| {
            sequence
                .tracks
                .get(index)
                .and_then(anim::track_folder)
                .filter(|folder| anim::is_animation_folder(arena, *folder))
        };
        self.layer()
            .and_then(|layer| {
                if anim::is_animation_folder(arena, layer) {
                    Some(layer)
                } else {
                    anim::cel_folder(arena, layer)
                }
            })
            .filter(|folder| anim::track_of(sequence, *folder).is_some())
            .or_else(|| of_track(track))
            .or_else(|| anim::folders(sequence).into_iter().find_map(of_track))
    }

    pub(crate) fn timeline_here(
        &self,
        arena: &sue_core::Arena,
    ) -> Option<(EntityId, Sequence, u32)> {
        let picture = self.picture()?;
        let sequence = anim::timeline(arena, picture)?.clone();
        Some((picture, sequence, anim::length(arena, picture)))
    }

    /// Makes a new animation of `frames` frames at `fps`: a picture with a
    /// timeline whose first folder shows its first layer, renamed `1`.
    pub(crate) fn make_animation(
        &mut self,
        cx: &mut AppletContext<'_>,
        spec: &NewPicture,
        frames: u32,
        fps: u32,
    ) {
        let (edit, picture) = match doc::create(spec) {
            Ok(made) => made,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        if !Self::apply(cx, edit) {
            return;
        }
        let timeline = anim::new_timeline(&cx.world.arena, picture, fps, frames);
        if Self::apply(cx, timeline) {
            self.activate(&cx.world.arena, picture);
            // The first layer becomes cel 1, in the animation folder.
            if let Some(layer) = doc::layers(&cx.world.arena, picture)
                .into_iter()
                .rev()
                .find(|info| info.kind == sue_model::picture::LayerKind::Raster)
                && let Some((_, sequence, length)) = self.timeline_here(&cx.world.arena)
            {
                let mut edit = Edit::new(tr!("name-cel")).then(Rename::new(layer.id, "1"));
                if let Some(folder) = sequence.tracks.first().and_then(anim::track_folder)
                    && let Some(into) = anim::set_folder(&cx.world.arena, layer.id, Some(folder))
                {
                    edit = edit.then(into);
                }
                let sequence = anim::assign(&sequence, 0, 0, Some(layer.id), picture, length);
                Self::apply(
                    cx,
                    edit.then(sue_core::txn::SetExtension::set(picture, sequence)),
                );
                if let Some(doc) = self.doc_mut() {
                    doc.layer = Some(layer.id);
                    doc.track = 0;
                }
            }
            // Every other layer (the paper) has a track too, as Clip
            // Studio Paint's timeline lists every layer.
            if let Some(tracks) = anim::setup::give_every_layer_a_track(&cx.world.arena, picture) {
                Self::apply(cx, tracks);
            }
        }
    }

    /// Moves to `frame` (wrapped into the timeline) and makes the cel it
    /// shows in the current folder the current layer (in a folder cel, a
    /// layer of it: see [`drawn_in`]).
    pub(crate) fn go_to_frame(&mut self, arena: &sue_core::Arena, frame: i64) {
        let Some((picture, sequence, length)) = self.timeline_here(arena) else {
            return;
        };
        let frame = frame.rem_euclid(i64::from(length)) as u32;
        let Some(doc) = self.doc_mut() else {
            return;
        };
        doc.frame = frame;
        let folders = anim::folders(&sequence);
        doc.track = doc.track.min(sequence.tracks.len().saturating_sub(1));
        // A camera, or a layer or folder moved by its keyframes, stays
        // picked from frame to frame, so the Object tool keys it on each
        // (as in Clip Studio Paint). Else the cel shown becomes the
        // current layer while the current layer is in that track's folder
        // (or none is): a layer picked outside it, a 2D camera folder
        // say, stays picked as the frame moves.
        let keyed = doc.layer.is_some_and(|layer| {
            crate::camera::is_camera(arena, layer)
                || arena.get_by_id(layer).is_ok_and(anim::motion::keyframes_on)
        });
        let on_track = |layer: EntityId| {
            anim::track_folder(&sequence.tracks[doc.track]).is_some_and(|folder| {
                layer == folder || doc::members(arena, picture, folder).contains(&layer)
            })
        };
        if !keyed
            && let Some(cel) = folders
                .contains(&doc.track)
                .then(|| anim::cel_at(&sequence.tracks[doc.track], sequence.rate, frame))
                .flatten()
            && doc.layer.is_none_or(on_track)
        {
            doc.layer = Some(drawn_in(arena, picture, cel, doc.layer));
        }
    }

    /// Keeps the shown frame up to date: plays, and tells the compositor
    /// which cels this frame shows.
    pub(crate) fn animate(&mut self, cx: &mut AppletContext<'_>, ctx: &egui::Context) {
        let now = cx.view.now;
        self.anim_ui.cels.next_frame();
        self.sound_export_ui(cx, ctx);
        // An older project's cels go into their animation folders once a
        // session (undo takes that back and it stays back).
        if let Some(picture) = self.picture()
            && self.gathered.insert(picture)
            && let Some(edit) = anim::gather(&cx.world.arena, picture)
        {
            Self::apply(cx, edit);
        }
        // The timeline's current track follows the layer picked: an
        // animation folder or one of its cels.
        if let Some((_, sequence, _)) = self.timeline_here(&cx.world.arena)
            && let Some(layer) = self.layer()
            && let Some(track) = anim::track_for_layer(&cx.world.arena, &sequence, layer)
            && let Some(doc) = self.doc_mut()
        {
            doc.track = track;
        }
        let timeline = self
            .timeline_here(&cx.world.arena)
            .filter(|_| self.timeline_on);
        let Some((_, sequence, length)) = timeline else {
            if let Some(doc) = self.doc_mut() {
                doc.playing = None;
                doc.compositor.set_view(None);
            }
            return;
        };
        let looping = self.looping;
        let onion = self.onion;
        // Playback runs over the playback range (its markers on the
        // ruler), from the frame it started on.
        let (first, last) = self
            .picture()
            .map_or((0, length.saturating_sub(1)), |picture| {
                anim::setup::Settings::of(&cx.world.arena, picture).range(length)
            });
        let span = i64::from(last - first + 1);
        let mut moved = None;
        let mut lap = None;
        if let Some(doc) = self.doc_mut()
            && let Some((started, from)) = doc.playing
        {
            let elapsed = now.saturating_duration_since(started).as_secs_f64();
            let begin = i64::from(from.clamp(first, last) - first);
            let gone = begin + (elapsed * anim::fps(&sequence)) as i64;
            if !looping && gone >= span {
                doc.playing = None;
                moved = Some(i64::from(last));
            } else {
                moved = Some(i64::from(first) + gone.rem_euclid(span));
                lap = Some(gone.div_euclid(span));
            }
            ctx.request_repaint();
        }
        if let Some(frame) = moved {
            self.go_to_frame(&cx.world.arena, frame);
            self.measure_playback(now, frame as u32);
        } else {
            self.anim_ui.clock = None;
            self.anim_ui.measured = 0.0;
        }
        self.sync_audio(cx, &sequence, lap.zip(moved));
        if let Some(frame) = self
            .doc()
            .filter(|doc| doc.playing.is_none())
            .map(|doc| doc.frame)
        {
            self.scrub_audio(cx, &sequence, frame);
        } else {
            // Playback moves the frames: no scrubbing until it stops.
            self.anim_ui.scrubbed = None;
        }
        let settings = self
            .picture()
            .map(|picture| anim::Onion::of(&cx.world.arena, picture))
            .unwrap_or_default();
        // The light table of the cel being drawn, and the general one.
        let size_now = self
            .picture()
            .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
            .and_then(sue_model::picture::size)
            .unwrap_or([0, 0]);
        let traced = if self.light_table && self.light_cel {
            self.editing_cel(&cx.world.arena)
                .map(|cel| anim::traced_on(&cx.world.arena, cel, anim::LIGHT_TABLE, size_now))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let general = match self.picture() {
            Some(picture) if self.light_table && self.light_general => anim::traced_on(
                &cx.world.arena,
                picture,
                anim::GENERAL_LIGHT_TABLE,
                size_now,
            ),
            _ => Vec::new(),
        };
        // An entry's opacity shows at once while its slider is held.
        let held = |table: crate::panels::cels::Table, mut traced: Vec<anim::Traced>| {
            for one in &mut traced {
                one.opacity = self.anim_ui.cels.opacity(table, one.layer, one.opacity);
            }
            traced
        };
        let traced = match self.editing_cel(&cx.world.arena) {
            Some(cel) => held(
                crate::panels::cels::Table {
                    from: cel,
                    general: false,
                },
                traced,
            ),
            None => traced,
        };
        let general = match self.picture() {
            Some(picture) => held(
                crate::panels::cels::Table {
                    from: picture,
                    general: true,
                },
                general,
            ),
            None => general,
        };
        let general_opacity = self.general_opacity;
        let opacity = self.light_opacity;
        let light_color = self
            .light_color
            .then_some(self.light_tint.map(|channel| f32::from(channel) / 255.0));
        // The camera folders at the frame shown.
        let (arena, picture) = (&cx.world.arena, self.picture());
        let size = picture
            .and_then(|picture| arena.get_by_id(picture).ok())
            .and_then(sue_model::picture::size)
            .unwrap_or([0, 0]);
        let rate = sequence.rate;
        // The camera being set up shows its layers as they are, with its
        // frame over them, as in Clip Studio Paint.
        let editing = (self.toolbox.tool() == crate::tools::Tool::Operation)
            .then(|| self.camera_folder(arena))
            .flatten();
        // Playback Settings, Render 2D Camera off: the whole canvas, the
        // cameras' frames over it.
        let render_camera =
            picture.is_some_and(|picture| anim::setup::Playback::of(arena, picture).render_camera);
        let cameras_at = |frame: u32| {
            picture
                .filter(|_| render_camera)
                .map_or_else(Vec::new, |picture| {
                    let mut views = crate::camera::views(
                        arena,
                        picture,
                        Some(crate::camera::time_of(frame, rate)),
                        size,
                    );
                    views.retain(|(folder, _)| Some(*folder) != editing);
                    views
                })
        };
        // Check Cel Motion and Check Surrounding Cels by Key Input:
        // another frame shows a moment, the playhead staying where it is.
        let peeked = self.peek_frame(now, &sequence, ctx);
        // Show Edited Cel Only: the cel being drawn shows in its folder
        // in place of the frame's.
        let edited_only = (self.edited_cel_only && peeked.is_none())
            .then(|| self.editing_cel(arena))
            .flatten()
            .and_then(|cel| Some((cel, anim::cel_folder(arena, cel)?)))
            .zip(picture)
            .map(|((cel, folder), picture)| (cel, anim::cels(arena, picture, folder)));
        if let Some(doc) = self.doc_mut() {
            let playing = doc.playing.is_some();
            let shown = peeked.unwrap_or(doc.frame);
            let onion = (onion && !playing && peeked.is_none()).then_some(&settings);
            // The frame as it plays: layer keyframes, clips and folder
            // cels, with onion skin while it stands still.
            let mut view = anim::frame_view(arena, doc.picture, shown, onion, Some(false))
                .unwrap_or_else(|| anim::view_in(arena, doc.picture, &sequence, shown, onion));
            view.cameras = cameras_at(shown);
            if !playing && let Some((cel, cels)) = &edited_only {
                for other in cels {
                    if other == cel {
                        view.hidden.remove(other);
                    } else {
                        view.hidden.insert(*other);
                    }
                }
            }
            if !playing && peeked.is_none() {
                view = view
                    .with_light_table(&traced, opacity, light_color)
                    .with_light_table(&general, general_opacity, light_color);
            }
            crate::studio::saw_view(crate::studio::ViewSeen {
                frame: shown,
                hidden: view.hidden.clone(),
            });
            doc.compositor.set_view(Some(view));
        }
    }

    /// The frame Check Cel Motion or Check Surrounding Cels by Key Input
    /// shows now in place of the playhead's, while it lasts; the look
    /// ends when the playhead moves or playback starts.
    fn peek_frame(
        &mut self,
        now: sue_platform::time::Instant,
        sequence: &Sequence,
        ctx: &egui::Context,
    ) -> Option<u32> {
        let (frame, playing) = self
            .doc()
            .map_or((0, true), |doc| (doc.frame, doc.playing.is_some()));
        let peek = self.cel_peek.as_ref()?;
        if playing || peek.from != frame {
            self.cel_peek = None;
            return None;
        }
        let shown = match peek.turn {
            Some(at) => peek.frames.get(at).copied(),
            None => {
                let gone = now.saturating_duration_since(peek.started).as_secs_f64();
                let step = (gone * anim::fps(sequence)) as usize;
                let next = peek.frames.get(step).copied();
                if next.is_some() {
                    ctx.request_repaint();
                }
                next
            }
        };
        if shown.is_none() {
            self.cel_peek = None;
            ctx.request_repaint();
        }
        shown
    }

    /// Keeps the timeline's sound in step with playback: started from the
    /// frame shown when playback starts and at each loop, stopped with it.
    fn sync_audio(
        &mut self,
        cx: &mut AppletContext<'_>,
        sequence: &sue_model::Sequence,
        playing_at: Option<(i64, i64)>,
    ) {
        const OWNER: &str = "sue-paint";
        let Some((lap, at)) =
            playing_at.filter(|_| self.doc().is_some_and(|doc| doc.playing.is_some()))
        else {
            if self.audio_lap.take().is_some() && cx.transport.is_playing_for(OWNER) {
                cx.transport.stop(cx.audio.as_deref_mut());
            }
            return;
        };
        if self.audio_lap == Some(lap) {
            return;
        }
        self.audio_lap = Some(lap);
        let Some(mixed) = self.mixed_sound(cx, sequence) else {
            return;
        };
        let seconds = at.max(0) as f64 / anim::fps(sequence).max(1.0);
        let start = (seconds * f64::from(anim::MIX_RATE)) as usize;
        let end = mixed.frames();
        if start >= end {
            if cx.transport.is_playing_for(OWNER) {
                cx.transport.stop(cx.audio.as_deref_mut());
            }
            return;
        }
        cx.transport.load(
            sue_wm::Clip {
                source: sue_wm::ClipSource::Stream(mixed),
                start,
                end,
                gain: 1.0,
                owner: OWNER.to_owned(),
            },
            cx.audio.as_deref_mut(),
        );
        cx.transport.set_playing(true, cx.audio.as_deref_mut());
    }

    /// The timeline's sound mixed into one stream, mixed again only when
    /// the sound changed.
    fn mixed_sound(
        &mut self,
        cx: &AppletContext<'_>,
        sequence: &sue_model::Sequence,
    ) -> Option<sue_core::Shared<sue_core::PCMFloat1D>> {
        let signature = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for index in anim::audio_tracks(sequence) {
                let track = &sequence.tracks[index];
                track.muted.hash(&mut hasher);
                for clip in track.clips() {
                    clip.source.hash(&mut hasher);
                    clip.start.flicks().hash(&mut hasher);
                    // A sound changed in Audio Clip Settings, or its
                    // volume, mixes again.
                    clip.duration.flicks().hash(&mut hasher);
                    anim::volume_of(clip).to_bits().hash(&mut hasher);
                    cx.world
                        .arena
                        .get_by_id(clip.source)
                        .map_or(0, |entity| entity.revision())
                        .hash(&mut hasher);
                }
            }
            hasher.finish()
        };
        if self
            .audio_mix
            .as_ref()
            .is_none_or(|(seen, _)| *seen != signature)
        {
            self.audio_mix = anim::mix(&cx.world.arena, sequence)
                .map(|pcm| (signature, sue_core::Shared::new(pcm)));
        }
        self.audio_mix.clone().map(|(_, mixed)| mixed)
    }

    /// Audio Scrubbing: the frame shown moved by hand (not by playback)
    /// to `frame`, so the sound of that frame plays, one frame long (at
    /// least a twelfth of a second, so it is heard).
    fn scrub_audio(
        &mut self,
        cx: &mut AppletContext<'_>,
        sequence: &sue_model::Sequence,
        frame: u32,
    ) {
        const OWNER: &str = "sue-paint";
        let before = self.anim_ui.scrubbed.replace(frame);
        if !self.anim_ui.scrub || before.is_none_or(|before| before == frame) {
            return;
        }
        let Some(mixed) = self.mixed_sound(cx, sequence) else {
            return;
        };
        let fps = anim::fps(sequence).max(1.0);
        let rate = f64::from(anim::MIX_RATE);
        let start = (f64::from(frame) / fps * rate) as usize;
        let long = ((rate / fps).max(rate / 12.0)) as usize;
        let end = (start + long).min(mixed.frames());
        if start >= end {
            return;
        }
        cx.transport.load(
            sue_wm::Clip {
                source: sue_wm::ClipSource::Stream(mixed),
                start,
                end,
                gain: 1.0,
                owner: OWNER.to_owned(),
            },
            cx.audio.as_deref_mut(),
        );
        cx.transport.set_playing(true, cx.audio.as_deref_mut());
    }

    /// Select and register file: the picture at `path` on the cel's light
    /// table only, a layer that never shows in the picture itself.
    pub(crate) fn light_table_file(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some(cel) = self.editing_cel(&cx.world.arena) else {
            return;
        };
        let before = self.layer();
        self.open_picked(cx, path, true);
        let Some(layer) = self.layer().filter(|layer| Some(*layer) != before) else {
            return;
        };
        let edit = anim::register(cel, layer)
            .then(SetProperty::new(layer, anim::ONLY, true))
            .then(SetProperty::new(layer, sue_model::picture::VISIBLE, false));
        if Self::apply(cx, edit) {
            self.light_table = true;
            self.light_cel = true;
        }
    }

    /// Measures how many frames a second playback reaches, for the bar's
    /// readout.
    fn measure_playback(&mut self, now: std::time::Instant, frame: u32) {
        match self.anim_ui.clock {
            Some((since, seen)) if seen != frame => {
                let seconds = now.saturating_duration_since(since).as_secs_f64();
                if seconds > 0.0 {
                    // Frames passed (one at a wrap to the range's start).
                    let passed = if frame > seen { frame - seen } else { 1 };
                    let fps = f64::from(passed) / seconds;
                    // Smoothed, so the number does not flicker.
                    self.anim_ui.measured = if self.anim_ui.measured > 0.0 {
                        self.anim_ui.measured * 0.8 + fps * 0.2
                    } else {
                        fps
                    };
                }
                self.anim_ui.clock = Some((now, frame));
            }
            Some(_) => {}
            None => self.anim_ui.clock = Some((now, frame)),
        }
    }
}

/// A look at other frames while the playhead stays (Check Cel Motion by
/// Key Input, Check Surrounding Cels by Key Input).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CelPeek {
    /// The frame the playhead stands on: the look ends when it moves.
    pub(crate) from: u32,
    /// The frames shown in turn.
    pub(crate) frames: Vec<u32>,
    /// When the look began: Check Cel Motion plays `frames` at the
    /// timeline's rate from then, once.
    pub(crate) started: sue_platform::time::Instant,
    /// Check Surrounding Cels: which of `frames` shows, until the next
    /// press moves on (past the last, the playhead's own frame again).
    pub(crate) turn: Option<usize>,
}

impl CelPeek {
    /// Check Cel Motion by Key Input from `frame` of `track`: the frames
    /// from the cel two cels before the one shown up to the frame before
    /// the playhead, played at the timeline's rate, so the drawing is
    /// seen moving into the one being drawn, as flipping paper does.
    /// `None` when no cel comes before.
    pub(crate) fn motion(
        track: &sue_model::Track,
        rate: sue_core::Rate,
        frame: u32,
        now: sue_platform::time::Instant,
    ) -> Option<Self> {
        let before: Vec<u32> = anim::keys(track, rate)
            .into_iter()
            .map(|(at, _)| at)
            .filter(|at| *at <= frame)
            .collect();
        // The key of the cel shown, and two before it.
        let start = *before.iter().rev().nth(2).or_else(|| before.first())?;
        let frames: Vec<u32> = (start..frame).collect();
        (!frames.is_empty()).then_some(Self {
            from: frame,
            frames,
            started: now,
            turn: None,
        })
    }

    /// Check Surrounding Cels by Key Input from `frame` of `track`: the
    /// keys of the cels before and after the one shown, one a press.
    /// `None` when there is neither.
    pub(crate) fn surrounding(
        track: &sue_model::Track,
        rate: sue_core::Rate,
        frame: u32,
        now: sue_platform::time::Instant,
    ) -> Option<Self> {
        let keys: Vec<u32> = anim::keys(track, rate)
            .into_iter()
            .map(|(at, _)| at)
            .collect();
        let here = keys.iter().rposition(|at| *at <= frame);
        let previous = here.and_then(|at| at.checked_sub(1)).map(|at| keys[at]);
        let next = keys.get(here.map_or(0, |at| at + 1)).copied();
        let frames: Vec<u32> = previous.into_iter().chain(next).collect();
        (!frames.is_empty()).then_some(Self {
            from: frame,
            frames,
            started: now,
            turn: Some(0),
        })
    }
}

/// Where drawing goes while `cel` shows: the cel itself, or, when the cel
/// is a folder (TR-1), the layer of it that was current already, else its
/// topmost layer that is no folder, so a folder cel is drawn into on its
/// own layers.
pub(crate) fn drawn_in(
    arena: &sue_core::Arena,
    picture: EntityId,
    cel: EntityId,
    current: Option<EntityId>,
) -> EntityId {
    if !doc::is_folder(arena, cel) {
        return cel;
    }
    let members = doc::members(arena, picture, cel);
    current
        .filter(|layer| members.contains(layer))
        .or_else(|| {
            members
                .iter()
                .rev()
                .copied()
                .find(|layer| !doc::is_folder(arena, *layer))
        })
        .unwrap_or(cel)
}
