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
        }
    }

    /// Moves to `frame` (wrapped into the timeline) and makes the cel it
    /// shows in the current folder the current layer.
    pub(crate) fn go_to_frame(&mut self, arena: &sue_core::Arena, frame: i64) {
        let Some((_, sequence, length)) = self.timeline_here(arena) else {
            return;
        };
        let frame = frame.rem_euclid(i64::from(length)) as u32;
        let Some(doc) = self.doc_mut() else {
            return;
        };
        doc.frame = frame;
        let folders = anim::folders(&sequence);
        doc.track = doc.track.min(sequence.tracks.len().saturating_sub(1));
        if let Some(cel) = folders
            .contains(&doc.track)
            .then(|| anim::cel_at(&sequence.tracks[doc.track], sequence.rate, frame))
            .flatten()
        {
            doc.layer = Some(cel);
        }
    }

    /// Keeps the shown frame up to date: plays, and tells the compositor
    /// which cels this frame shows.
    pub(crate) fn animate(&mut self, cx: &mut AppletContext<'_>, ctx: &egui::Context) {
        let now = cx.view.now;
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
        let mut moved = None;
        if let Some(doc) = self.doc_mut()
            && let Some((started, from)) = doc.playing
        {
            let elapsed = now.saturating_duration_since(started).as_secs_f64();
            let frame = i64::from(from) + (elapsed * anim::fps(&sequence)) as i64;
            if !looping && frame >= i64::from(length) {
                doc.playing = None;
                moved = Some(i64::from(length) - 1);
            } else {
                moved = Some(frame);
            }
            ctx.request_repaint();
        }
        if let Some(frame) = moved {
            self.go_to_frame(&cx.world.arena, frame);
        }
        self.sync_audio(cx, &sequence, length, moved);
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
                .map(|cel| {
                    anim::traced(
                        &cx.world.arena,
                        &anim::light_table(&cx.world.arena, cel),
                        size_now,
                    )
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let general = match self.picture() {
            Some(picture) if self.light_table && self.light_general => anim::traced(
                &cx.world.arena,
                &anim::general_light_table(&cx.world.arena, picture),
                size_now,
            ),
            _ => Vec::new(),
        };
        let general_opacity = self.general_opacity;
        let opacity = self.light_opacity;
        let light_color = self.light_color.then_some([0.25, 0.45, 1.0]);
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
        let cameras_at = |frame: u32| {
            picture.map_or_else(Vec::new, |picture| {
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
        if let Some(doc) = self.doc_mut() {
            let playing = doc.playing.is_some();
            let mut view = anim::view_in(
                arena,
                doc.picture,
                &sequence,
                doc.frame,
                (onion && !playing).then_some(&settings),
            );
            view.cameras = cameras_at(doc.frame);
            if !playing {
                view = view
                    .with_light_table(&traced, opacity, light_color)
                    .with_light_table(&general, general_opacity, light_color);
            }
            doc.compositor.set_view(Some(view));
        }
    }

    /// Keeps the timeline's sound in step with playback: started from the
    /// frame shown when playback starts and at each loop, stopped with it.
    fn sync_audio(
        &mut self,
        cx: &mut AppletContext<'_>,
        sequence: &sue_model::Sequence,
        length: u32,
        playing_at: Option<i64>,
    ) {
        const OWNER: &str = "sue-paint";
        let Some(at) = playing_at.filter(|_| self.doc().is_some_and(|doc| doc.playing.is_some()))
        else {
            if self.audio_lap.take().is_some() && cx.transport.is_playing_for(OWNER) {
                cx.transport.stop(cx.audio.as_deref_mut());
            }
            return;
        };
        let length = i64::from(length.max(1));
        let lap = at.div_euclid(length);
        if self.audio_lap == Some(lap) {
            return;
        }
        self.audio_lap = Some(lap);
        // Mixed again only when the sound changed.
        let signature = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for index in anim::audio_tracks(sequence) {
                let track = &sequence.tracks[index];
                track.muted.hash(&mut hasher);
                for clip in track.clips() {
                    clip.source.hash(&mut hasher);
                    clip.start.flicks().hash(&mut hasher);
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
        let Some((_, mixed)) = self.audio_mix.clone() else {
            return;
        };
        let seconds = at.rem_euclid(length) as f64 / anim::fps(sequence).max(1.0);
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
}
