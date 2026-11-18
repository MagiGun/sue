//! Sequences. A video edit, a song in a DAW, a slideshow with timings, a
//! podcast, an animation's dope sheet and a subtitle track are all tracks of
//! clips on the one clock. A [`Clip`] points at any entity by id (a video
//! file, a recording, a picture, a text, another sequence), so every feature
//! can be placed on every timeline. [`Notes`] hold note events (MIDI, a
//! piano roll, singing synthesis with lyrics and phonemes).
//!
//! Version 2 of `sue.sequence` (Sue VE, 0.0.83) adds what an edit needs
//! beyond clips on tracks, so that Sue Paint's animation timeline, Sue
//! Writer and Sue VE share one model:
//!
//! - every clip has an id that is unique in its sequence ([`Clip::id`],
//!   given by [`Sequence::ensure_ids`]), a switch ([`Clip::enabled`]), a
//!   link group ([`Clip::link`]: a picture and its sound move together)
//!   and keyframes ([`Clip::animation`], the shared [`Animation`] curves,
//!   keyed in the source's time so that trims never move them);
//! - every track has transitions between or at the ends of its clips
//!   ([`Transition`]);
//! - the sequence has markers ([`Marker`]) and properties of its own
//!   (a video's frame size lives there).
//!
//! [`SequenceEdit`] changes a sequence as one undo step that stores only
//! the tracks it changed, before and after.

use sue_core::codec::{CodecError, Reader, Writer};
use sue_core::{
    Arena, ComponentType, CoreError, EntityId, Properties, Rate, ReversibleTransaction, Time,
    TxnError,
};

use crate::Animation;

fn write_id(out: &mut Writer, id: EntityId) {
    out.raw(&id.to_bytes());
}

fn read_id(input: &mut Reader<'_>) -> Result<EntityId, CodecError> {
    let mut bytes = [0; 16];
    bytes.copy_from_slice(input.raw(16)?);
    Ok(EntityId::from_bytes(bytes))
}

/// A use of an entity on a track.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    /// Its id in the sequence, 0 until [`Sequence::ensure_ids`] gives it
    /// one. Copies get new ids; a split's right half gets a new one.
    pub id: u64,
    /// What plays.
    pub source: EntityId,
    /// Where it starts on the sequence.
    pub start: Time,
    /// Where it starts inside the source (the trim).
    pub source_in: Time,
    /// How long it lasts on the sequence.
    pub duration: Time,
    /// Playback speed (1 is normal, negative plays backwards).
    pub speed: f64,
    /// Per-clip settings (gain, opacity, transition, effects chain).
    pub properties: Properties,
    /// `false` for a clip that stays on its track but neither shows nor
    /// sounds.
    pub enabled: bool,
    /// Its link group (0: none): clips of one group (a picture and its
    /// sound) are selected, moved and trimmed together.
    pub link: u64,
    /// Keyframed properties, keyed in the source's time (what
    /// [`Clip::source_time`] gives), so trims and splits never move them.
    pub animation: Animation,
}

impl Clip {
    /// A clip of `source` at `start` for `duration`, untrimmed.
    #[must_use]
    pub fn new(source: EntityId, start: Time, duration: Time) -> Self {
        Self {
            id: 0,
            source,
            start,
            source_in: Time::ZERO,
            duration,
            speed: 1.0,
            properties: Properties::new(),
            enabled: true,
            link: 0,
            animation: Animation::new(),
        }
    }

    /// The source time at the clip's out point (just past the last
    /// that plays).
    #[must_use]
    pub fn source_out(&self) -> Time {
        self.source_time(self.end())
    }

    /// The value of `key` at sequence time `time`: its keyframes when it
    /// has any, else the clip's property, else `default`.
    #[must_use]
    pub fn value_at(&self, key: &str, time: Time, default: f64) -> f64 {
        if let Some(value) = self
            .animation
            .channel(key)
            .and_then(|curve| curve.sample(self.source_time(time)))
        {
            return value;
        }
        self.properties
            .get(key)
            .and_then(sue_core::Value::as_f64)
            .unwrap_or(default)
    }

    /// Where it ends on the sequence.
    #[must_use]
    pub fn end(&self) -> Time {
        self.start + self.duration
    }

    /// The source time that plays at sequence time `time`.
    #[must_use]
    pub fn source_time(&self, time: Time) -> Time {
        let offset = (time - self.start).seconds() * self.speed;
        self.source_in + Time::from_seconds(offset)
    }
}

/// A lane of clips.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    /// Name, e.g. `V1` or `Vocals`.
    pub name: String,
    /// `video`, `audio`, `midi`, `subtitle`, `text`, `automation` or a
    /// feature's own kind.
    pub kind: String,
    /// Silenced or hidden.
    pub muted: bool,
    /// Protected from edits.
    pub locked: bool,
    /// Track settings (volume, pan, blend mode, output bus).
    pub properties: Properties,
    clips: Vec<Clip>,
    /// Transitions between its clips and at their ends, by start.
    pub transitions: Vec<Transition>,
}

impl Track {
    /// An empty track.
    pub fn new(name: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: kind.into(),
            ..Self::default()
        }
    }

    /// The clips, by start time.
    #[must_use]
    pub fn clips(&self) -> &[Clip] {
        &self.clips
    }

    /// Adds a clip in time order; returns its index.
    pub fn insert(&mut self, clip: Clip) -> usize {
        let index = self.clips.partition_point(|old| old.start <= clip.start);
        self.clips.insert(index, clip);
        index
    }

    /// Removes the clip at `index`.
    pub fn remove(&mut self, index: usize) -> Option<Clip> {
        (index < self.clips.len()).then(|| self.clips.remove(index))
    }

    /// The clips, to change in place. Whoever moves a clip's start calls
    /// [`Track::sort`] afterwards.
    pub fn clips_mut(&mut self) -> &mut Vec<Clip> {
        &mut self.clips
    }

    /// Puts the clips and the transitions back in order of start.
    pub fn sort(&mut self) {
        self.clips.sort_by_key(|clip| clip.start);
        self.transitions.sort_by_key(|transition| transition.start);
    }

    /// The clip with id `id`.
    #[must_use]
    pub fn clip(&self, id: u64) -> Option<&Clip> {
        self.clips.iter().find(|clip| clip.id == id)
    }

    /// The clip with id `id`, to change.
    pub fn clip_mut(&mut self, id: u64) -> Option<&mut Clip> {
        self.clips.iter_mut().find(|clip| clip.id == id)
    }

    /// The clips playing at `time`.
    pub fn at(&self, time: Time) -> impl Iterator<Item = &Clip> {
        self.clips
            .iter()
            .filter(move |clip| clip.start <= time && time < clip.end())
    }

    /// Cuts the clip at `index` in two at sequence time `time`. Returns the
    /// index of the right half, or `None` if `time` is not inside the clip.
    pub fn split(&mut self, index: usize, time: Time) -> Option<usize> {
        let clip = self.clips.get_mut(index)?;
        if time <= clip.start || time >= clip.end() {
            return None;
        }
        let mut right = clip.clone();
        // The right half is a clip of its own: it gets an id from
        // `Sequence::ensure_ids`.
        right.id = 0;
        right.source_in = clip.source_time(time);
        right.start = time;
        right.duration = clip.end() - time;
        clip.duration = time - clip.start;
        Some(self.insert(right))
    }

    /// When the last clip ends.
    #[must_use]
    pub fn end(&self) -> Time {
        self.clips.iter().map(Clip::end).max().unwrap_or(Time::ZERO)
    }
}

/// A change from one clip to the next on a track (a dissolve, a dip), or
/// from or to nothing at a clip's end (a fade). It covers the time from
/// `start` for `duration`; the clips it joins are named by their ids.
#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    /// What it does, by a registered name: `dissolve`, `dip-black`,
    /// `dip-white`, or a feature's own (`ve.wipe`).
    pub kind: String,
    /// Where it begins on the sequence.
    pub start: Time,
    /// How long it lasts.
    pub duration: Time,
    /// The clip it leaves (0: it fades in from nothing).
    pub from: u64,
    /// The clip it reaches (0: it fades out to nothing).
    pub to: u64,
    /// Its settings (the dip's color, a wipe's angle).
    pub properties: Properties,
}

impl Transition {
    /// A transition of `kind` over `start` for `duration`, from clip
    /// `from` to clip `to`.
    #[must_use]
    pub fn new(kind: impl Into<String>, start: Time, duration: Time, from: u64, to: u64) -> Self {
        Self {
            kind: kind.into(),
            start,
            duration,
            from,
            to,
            properties: Properties::new(),
        }
    }

    /// Where it ends.
    #[must_use]
    pub fn end(&self) -> Time {
        self.start + self.duration
    }

    /// How far through it `time` is, 0 to 1 (clamped).
    #[must_use]
    pub fn progress(&self, time: Time) -> f64 {
        if self.duration.flicks() <= 0 {
            return 1.0;
        }
        ((time - self.start).seconds() / self.duration.seconds()).clamp(0.0, 1.0)
    }
}

/// A named place (or stretch) on a sequence.
#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    /// Where.
    pub at: Time,
    /// How long (zero for a point).
    pub duration: Time,
    /// What people read.
    pub name: String,
    /// Its color, a note, a chapter flag.
    pub properties: Properties,
}

impl Marker {
    /// A point marker at `at`.
    #[must_use]
    pub fn new(at: Time, name: impl Into<String>) -> Self {
        Self {
            at,
            duration: Time::ZERO,
            name: name.into(),
            properties: Properties::new(),
        }
    }
}

/// Tracks of clips at a frame rate and a sample rate.
#[derive(Clone, Debug, PartialEq)]
pub struct Sequence {
    /// The edit (frame) rate.
    pub rate: Rate,
    /// The audio sample rate.
    pub sample_rate: u32,
    /// Top track first.
    pub tracks: Vec<Track>,
    /// Markers, by time.
    pub markers: Vec<Marker>,
    /// The sequence's own settings (a video's `ve.width` and `ve.height`).
    pub properties: Properties,
    /// The id the next clip gets.
    pub next_id: u64,
}

impl Default for Sequence {
    fn default() -> Self {
        Self::new(Rate::FPS_30, 48_000)
    }
}

impl Sequence {
    /// An empty sequence at `rate` and `sample_rate`.
    #[must_use]
    pub fn new(rate: Rate, sample_rate: u32) -> Self {
        Self {
            rate,
            sample_rate,
            tracks: Vec::new(),
            markers: Vec::new(),
            properties: Properties::new(),
            next_id: 1,
        }
    }

    /// A new clip id.
    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        id
    }

    /// Gives every clip without an id (and every clip whose id an earlier
    /// clip has too) one of its own, and keeps `next_id` past them all.
    pub fn ensure_ids(&mut self) {
        let top = self
            .tracks
            .iter()
            .flat_map(|track| &track.clips)
            .map(|clip| clip.id)
            .max()
            .unwrap_or(0);
        self.next_id = self.next_id.max(top + 1);
        let mut seen = std::collections::HashSet::new();
        let mut next = self.next_id;
        for clip in self.tracks.iter_mut().flat_map(|track| &mut track.clips) {
            if clip.id == 0 || !seen.insert(clip.id) {
                clip.id = next;
                seen.insert(next);
                next += 1;
            }
        }
        self.next_id = next;
    }

    /// The track index and clip of id `id`.
    #[must_use]
    pub fn find(&self, id: u64) -> Option<(usize, &Clip)> {
        self.tracks
            .iter()
            .enumerate()
            .find_map(|(index, track)| track.clip(id).map(|clip| (index, clip)))
    }

    /// The track index and clip of id `id`, to change.
    pub fn find_mut(&mut self, id: u64) -> Option<(usize, &mut Clip)> {
        self.tracks
            .iter_mut()
            .enumerate()
            .find_map(|(index, track)| track.clip_mut(id).map(|clip| (index, clip)))
    }

    /// The ids of the clips linked with `id`, itself included.
    #[must_use]
    pub fn linked(&self, id: u64) -> Vec<u64> {
        let Some((_, clip)) = self.find(id) else {
            return Vec::new();
        };
        if clip.link == 0 {
            return vec![id];
        }
        let link = clip.link;
        self.tracks
            .iter()
            .flat_map(|track| &track.clips)
            .filter(|other| other.link == link)
            .map(|other| other.id)
            .collect()
    }

    /// When the last clip ends.
    #[must_use]
    pub fn duration(&self) -> Time {
        self.tracks
            .iter()
            .map(Track::end)
            .max()
            .unwrap_or(Time::ZERO)
    }

    /// Every clip playing at `time`, with its track index, top track first,
    /// skipping muted tracks.
    #[must_use]
    pub fn at(&self, time: Time) -> Vec<(usize, &Clip)> {
        self.tracks
            .iter()
            .enumerate()
            .filter(|(_, track)| !track.muted)
            .flat_map(|(index, track)| track.at(time).map(move |clip| (index, clip)))
            .collect()
    }
}

fn encode_clip(out: &mut Writer, clip: &Clip) {
    write_id(out, clip.source);
    out.varint_signed(clip.start.flicks());
    out.varint_signed(clip.source_in.flicks());
    out.varint_signed(clip.duration.flicks());
    out.f64(clip.speed);
    clip.properties.encode(out);
    // Version 2.
    out.varint(clip.id);
    out.bool(clip.enabled);
    out.varint(clip.link);
    out.u32(Animation::VERSION);
    out.nested(|out| ComponentType::encode(&clip.animation, out));
}

fn decode_clip(version: u32, input: &mut Reader<'_>) -> Result<Clip, CodecError> {
    let mut clip = Clip::new(read_id(input)?, Time::ZERO, Time::ZERO);
    clip.start = Time(input.varint_signed()?);
    clip.source_in = Time(input.varint_signed()?);
    clip.duration = Time(input.varint_signed()?);
    clip.speed = input.f64()?;
    clip.properties = Properties::decode(input)?;
    if version >= 2 {
        clip.id = input.varint()?;
        clip.enabled = input.bool()?;
        clip.link = input.varint()?;
        let animation_version = input.u32()?;
        let mut nested = input.nested()?;
        clip.animation = Animation::decode(animation_version, &mut nested)?;
    }
    Ok(clip)
}

fn encode_transition(out: &mut Writer, transition: &Transition) {
    out.str(&transition.kind);
    out.varint_signed(transition.start.flicks());
    out.varint_signed(transition.duration.flicks());
    out.varint(transition.from);
    out.varint(transition.to);
    transition.properties.encode(out);
}

fn decode_transition(input: &mut Reader<'_>) -> Result<Transition, CodecError> {
    let kind = input.str()?;
    let start = Time(input.varint_signed()?);
    let duration = Time(input.varint_signed()?);
    let from = input.varint()?;
    let mut transition = Transition::new(kind, start, duration, from, input.varint()?);
    transition.properties = Properties::decode(input)?;
    Ok(transition)
}

impl ComponentType for Sequence {
    const NAME: &'static str = "sue.sequence";
    const VERSION: u32 = 2;

    fn encode(&self, out: &mut Writer) {
        out.u32(self.rate.numerator());
        out.u32(self.rate.denominator());
        out.u32(self.sample_rate);
        out.len(self.tracks.len());
        for track in &self.tracks {
            out.str(&track.name);
            out.str(&track.kind);
            out.bool(track.muted);
            out.bool(track.locked);
            track.properties.encode(out);
            out.len(track.clips.len());
            for clip in &track.clips {
                encode_clip(out, clip);
            }
            // Version 2.
            out.len(track.transitions.len());
            for transition in &track.transitions {
                encode_transition(out, transition);
            }
        }
        // Version 2.
        out.len(self.markers.len());
        for marker in &self.markers {
            out.varint_signed(marker.at.flicks());
            out.varint_signed(marker.duration.flicks());
            out.str(&marker.name);
            marker.properties.encode(out);
        }
        self.properties.encode(out);
        out.varint(self.next_id);
    }

    fn decode(version: u32, input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let (numerator, denominator) = (input.u32()?, input.u32()?);
        if numerator == 0 || denominator == 0 {
            return Err(input.error("a rate must be positive"));
        }
        let mut sequence = Self::new(Rate::new(numerator, denominator), input.u32()?);
        for _ in 0..input.len(5)? {
            let mut track = Track::new(input.str()?, input.str()?);
            track.muted = input.bool()?;
            track.locked = input.bool()?;
            track.properties = Properties::decode(input)?;
            for _ in 0..input.len(28)? {
                track.insert(decode_clip(version, input)?);
            }
            if version >= 2 {
                for _ in 0..input.len(5)? {
                    track.transitions.push(decode_transition(input)?);
                }
            }
            sequence.tracks.push(track);
        }
        if version >= 2 {
            for _ in 0..input.len(4)? {
                let at = Time(input.varint_signed()?);
                let duration = Time(input.varint_signed()?);
                let mut marker = Marker::new(at, input.str()?);
                marker.duration = duration;
                marker.properties = Properties::decode(input)?;
                sequence.markers.push(marker);
            }
            sequence.properties = Properties::decode(input)?;
            sequence.next_id = input.varint()?.max(1);
        }
        Ok(sequence)
    }

    fn heap_bytes(&self) -> usize {
        self.tracks
            .iter()
            .map(|track| track.clips.len() * size_of::<Clip>())
            .sum()
    }

    fn remap_entities(&mut self, map: &mut dyn FnMut(EntityId) -> EntityId) {
        self.properties
            .values_mut()
            .for_each(|value| value.remap_entities(map));
        for track in &mut self.tracks {
            // A track may name entities too (the folder it stands for).
            track
                .properties
                .values_mut()
                .for_each(|value| value.remap_entities(map));
            for clip in &mut track.clips {
                clip.source = map(clip.source);
                clip.properties
                    .values_mut()
                    .for_each(|value| value.remap_entities(map));
            }
        }
    }
}

/// What of a sequence is not its tracks: small, kept whole by an edit
/// that changes it.
#[derive(Clone, Debug, PartialEq)]
struct Head {
    rate: Rate,
    sample_rate: u32,
    markers: Vec<Marker>,
    properties: Properties,
    next_id: u64,
}

impl Head {
    fn of(sequence: &Sequence) -> Self {
        Self {
            rate: sequence.rate,
            sample_rate: sequence.sample_rate,
            markers: sequence.markers.clone(),
            properties: sequence.properties.clone(),
            next_id: sequence.next_id,
        }
    }

    fn put(&self, sequence: &mut Sequence) {
        sequence.rate = self.rate;
        sequence.sample_rate = self.sample_rate;
        sequence.markers.clone_from(&self.markers);
        sequence.properties = self.properties.clone();
        sequence.next_id = self.next_id;
    }
}

/// A change to the [`Sequence`] of entity `id` as one undo step. It
/// holds the tracks the change touched, as they were and as they became,
/// and the rest of the sequence only when that changed too: a trim on a
/// timeline of forty tracks stores one track twice, not the edit.
#[derive(Clone, Debug)]
pub struct SequenceEdit {
    id: EntityId,
    label: String,
    lengths: (usize, usize),
    tracks: Vec<(usize, Option<Track>, Option<Track>)>,
    head: Option<(Head, Head)>,
}

impl SequenceEdit {
    /// The change from `before` to `after` on entity `id`, named `label`;
    /// `None` when the two are the same.
    #[must_use]
    pub fn between(
        id: EntityId,
        label: impl Into<String>,
        before: &Sequence,
        after: &Sequence,
    ) -> Option<Self> {
        let lengths = (before.tracks.len(), after.tracks.len());
        let tracks: Vec<(usize, Option<Track>, Option<Track>)> = (0..lengths.0.max(lengths.1))
            .filter_map(|index| {
                let (old, new) = (before.tracks.get(index), after.tracks.get(index));
                (old != new).then(|| (index, old.cloned(), new.cloned()))
            })
            .collect();
        let (old, new) = (Head::of(before), Head::of(after));
        let head = (old != new).then_some((old, new));
        (!tracks.is_empty() || head.is_some()).then(|| Self {
            id,
            label: label.into(),
            lengths,
            tracks,
            head,
        })
    }

    /// How many tracks it holds (for tests of its size).
    #[must_use]
    pub fn tracks_held(&self) -> usize {
        self.tracks.len()
    }

    fn put(&self, arena: &mut Arena, forward: bool) -> Result<(), TxnError> {
        let sequence = arena
            .get_by_id_mut(self.id)?
            .ext_mut::<Sequence>()
            .ok_or_else(|| TxnError::from(CoreError::UnknownId(self.id)))?;
        let (from, to) = if forward {
            self.lengths
        } else {
            (self.lengths.1, self.lengths.0)
        };
        if sequence.tracks.len() != from {
            return Err(TxnError::new(
                "the sequence is not as this edit found it",
            ));
        }
        sequence.tracks.resize_with(to, Track::default);
        for (index, old, new) in &self.tracks {
            let track = if forward { new } else { old };
            if let (Some(track), Some(slot)) = (track, sequence.tracks.get_mut(*index)) {
                slot.clone_from(track);
            }
        }
        if let Some((old, new)) = &self.head {
            if forward { new } else { old }.put(sequence);
        }
        Ok(())
    }
}

impl ReversibleTransaction<Arena> for SequenceEdit {
    fn label(&self) -> String {
        self.label.clone()
    }

    fn apply(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        self.put(arena, true)
    }

    fn revert(&mut self, arena: &mut Arena) -> Result<(), TxnError> {
        self.put(arena, false)
    }
}

/// One note event.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    /// When it starts.
    pub start: Time,
    /// How long it lasts.
    pub duration: Time,
    /// MIDI pitch (60 is middle C); fractions are microtonal.
    pub pitch: f32,
    /// Loudness, 0 to 1.
    pub velocity: f32,
    /// MIDI channel or voice.
    pub channel: u8,
    /// A sung syllable, for singing synthesis.
    pub lyric: Option<String>,
    /// Phonemes, vibrato, expression, articulation.
    pub properties: Properties,
}

impl Note {
    /// A note without lyric.
    #[must_use]
    pub fn new(start: Time, duration: Time, pitch: f32, velocity: f32) -> Self {
        Self {
            start,
            duration,
            pitch,
            velocity,
            channel: 0,
            lyric: None,
            properties: Properties::new(),
        }
    }

    /// The frequency in hertz (A4 = 440 Hz).
    #[must_use]
    pub fn frequency(&self) -> f64 {
        440.0 * 2_f64.powf((f64::from(self.pitch) - 69.0) / 12.0)
    }
}

/// Note events, by start time.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notes {
    notes: Vec<Note>,
}

impl Notes {
    /// No notes.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The notes, by start.
    #[must_use]
    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    /// Adds a note in time order; returns its index.
    pub fn insert(&mut self, note: Note) -> usize {
        let index = self.notes.partition_point(|old| old.start <= note.start);
        self.notes.insert(index, note);
        index
    }

    /// Removes the note at `index`.
    pub fn remove(&mut self, index: usize) -> Option<Note> {
        (index < self.notes.len()).then(|| self.notes.remove(index))
    }

    /// The notes sounding at `time`.
    pub fn at(&self, time: Time) -> impl Iterator<Item = &Note> {
        self.notes
            .iter()
            .filter(move |note| note.start <= time && time < note.start + note.duration)
    }

    /// The lyrics in order, joined by spaces.
    #[must_use]
    pub fn lyrics(&self) -> String {
        self.notes
            .iter()
            .filter_map(|note| note.lyric.as_deref())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl ComponentType for Notes {
    const NAME: &'static str = "sue.notes";
    const VERSION: u32 = 1;

    fn encode(&self, out: &mut Writer) {
        out.len(self.notes.len());
        for note in &self.notes {
            out.varint_signed(note.start.flicks());
            out.varint_signed(note.duration.flicks());
            out.f32(note.pitch);
            out.f32(note.velocity);
            out.u8(note.channel);
            out.str(note.lyric.as_deref().unwrap_or(""));
            note.properties.encode(out);
        }
    }

    fn decode(_version: u32, input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let mut notes = Self::new();
        for _ in 0..input.len(13)? {
            let mut note = Note::new(
                Time(input.varint_signed()?),
                Time(input.varint_signed()?),
                input.f32()?,
                input.f32()?,
            );
            note.channel = input.u8()?;
            note.lyric = Some(input.str()?).filter(|lyric| !lyric.is_empty());
            note.properties = Properties::decode(input)?;
            notes.insert(note);
        }
        Ok(notes)
    }

    fn heap_bytes(&self) -> usize {
        self.notes.len() * size_of::<Note>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_trim_split_and_stack() {
        let (film, song, folder) = (EntityId::new(), EntityId::new(), EntityId::new());
        let mut sequence = Sequence::new(Rate::FPS_24, 48_000);
        let mut video = Track::new("V1", "video");
        // A track naming an entity (the folder it stands for).
        video.properties.set("anim.folder", folder);
        let mut clip = Clip::new(film, Time::from_seconds(1.0), Time::from_seconds(4.0));
        clip.source_in = Time::from_seconds(10.0);
        video.insert(clip);
        let mut audio = Track::new("A1", "audio");
        audio.insert(Clip::new(song, Time::ZERO, Time::from_seconds(8.0)));
        sequence.tracks.extend([video, audio]);
        assert_eq!(sequence.duration(), Time::from_seconds(8.0));
        assert_eq!(sequence.at(Time::from_seconds(2.0)).len(), 2);
        assert_eq!(sequence.at(Time::from_seconds(6.0)).len(), 1);

        let right = sequence.tracks[0]
            .split(0, Time::from_seconds(3.0))
            .unwrap();
        let halves = sequence.tracks[0].clips();
        assert_eq!(halves[0].duration, Time::from_seconds(2.0));
        assert_eq!(halves[right].start, Time::from_seconds(3.0));
        assert_eq!(halves[right].source_in, Time::from_seconds(12.0));
        assert_eq!(halves[right].end(), Time::from_seconds(5.0));
        assert!(
            sequence.tracks[0]
                .split(0, Time::from_seconds(9.0))
                .is_none()
        );

        sequence.tracks[1].muted = true;
        assert_eq!(sequence.at(Time::from_seconds(6.0)).len(), 0);

        let mut out = Writer::new();
        ComponentType::encode(&sequence, &mut out);
        let bytes = out.into_bytes();
        let back = Sequence::decode(Sequence::VERSION, &mut Reader::new(&bytes)).unwrap();
        assert_eq!(back, sequence);

        let (other, other_folder) = (EntityId::new(), EntityId::new());
        let mut moved = back;
        ComponentType::remap_entities(&mut moved, &mut |id| {
            if id == film {
                other
            } else if id == folder {
                other_folder
            } else {
                id
            }
        });
        assert_eq!(moved.tracks[0].clips()[0].source, other);
        assert_eq!(
            moved.tracks[0]
                .properties
                .get("anim.folder")
                .and_then(sue_core::Value::as_entity),
            Some(other_folder),
            "a track's entity properties follow a remap"
        );
    }

    /// A sequence as version 1 (0.0.82 and before) wrote it.
    fn version_1_bytes(sequence: &Sequence) -> Vec<u8> {
        let mut out = Writer::new();
        out.u32(sequence.rate.numerator());
        out.u32(sequence.rate.denominator());
        out.u32(sequence.sample_rate);
        out.len(sequence.tracks.len());
        for track in &sequence.tracks {
            out.str(&track.name);
            out.str(&track.kind);
            out.bool(track.muted);
            out.bool(track.locked);
            track.properties.encode(&mut out);
            out.len(track.clips().len());
            for clip in track.clips() {
                write_id(&mut out, clip.source);
                out.varint_signed(clip.start.flicks());
                out.varint_signed(clip.source_in.flicks());
                out.varint_signed(clip.duration.flicks());
                out.f64(clip.speed);
                clip.properties.encode(&mut out);
            }
        }
        out.into_bytes()
    }

    fn edit() -> Sequence {
        let (film, song) = (EntityId::new(), EntityId::new());
        let mut sequence = Sequence::new(Rate::FPS_29_97, 48_000);
        let mut video = Track::new("V1", "video");
        let mut clip = Clip::new(film, Time::from_seconds(1.0), Time::from_seconds(4.0));
        clip.properties.set("ve.opacity", 0.5);
        video.insert(clip);
        video.insert(Clip::new(
            film,
            Time::from_seconds(5.0),
            Time::from_seconds(2.0),
        ));
        let mut audio = Track::new("A1", "audio");
        audio.insert(Clip::new(song, Time::ZERO, Time::from_seconds(8.0)));
        sequence.tracks.extend([video, audio]);
        sequence
    }

    #[test]
    fn a_version_1_sequence_reads_with_the_new_fields_at_their_defaults() {
        let old = edit();
        let bytes = version_1_bytes(&old);
        let mut read = Sequence::decode(1, &mut Reader::new(&bytes)).unwrap();
        assert_eq!(read, old, "the same clips, ids 0, enabled, unlinked");
        assert!(read.tracks[0].clips().iter().all(|clip| clip.enabled));
        assert!(read.markers.is_empty() && read.tracks[0].transitions.is_empty());
        read.ensure_ids();
        let ids: Vec<u64> = read
            .tracks
            .iter()
            .flat_map(|track| track.clips().iter().map(|clip| clip.id))
            .collect();
        assert_eq!(ids, [1, 2, 3]);
        assert_eq!(read.next_id, 4);
        // Saved again it is version 2 and reads back the same.
        let mut out = Writer::new();
        ComponentType::encode(&read, &mut out);
        let bytes = out.into_bytes();
        assert_eq!(
            Sequence::decode(2, &mut Reader::new(&bytes)).unwrap(),
            read
        );
    }

    #[test]
    fn version_2_keeps_ids_links_keys_transitions_and_markers() {
        let mut sequence = edit();
        sequence.ensure_ids();
        sequence.properties.set("ve.width", 1920_i64);
        let clip = sequence.tracks[0].clip_mut(1).unwrap();
        clip.link = 7;
        clip.enabled = false;
        clip.animation.channel_mut("ve.opacity").insert(crate::Key {
            time: Time::from_seconds(0.5),
            value: 0.25,
            interpolation: crate::Interpolation::Linear,
        });
        sequence.tracks[0].transitions.push(Transition::new(
            "dissolve",
            Time::from_seconds(4.5),
            Time::from_seconds(1.0),
            1,
            2,
        ));
        let mut marker = Marker::new(Time::from_seconds(2.0), "Chorus");
        marker.duration = Time::from_seconds(1.0);
        sequence.markers.push(marker);
        let mut out = Writer::new();
        ComponentType::encode(&sequence, &mut out);
        let bytes = out.into_bytes();
        let back = Sequence::decode(2, &mut Reader::new(&bytes)).unwrap();
        assert_eq!(back, sequence);
        assert_eq!(back.linked(1), [1]);
        // A broken or cut file is an error, never a panic.
        for cut in 0..bytes.len() {
            let _ = Sequence::decode(2, &mut Reader::new(&bytes[..cut]));
        }
    }

    #[test]
    fn keyed_values_follow_the_source_so_trims_keep_them() {
        let mut clip = Clip::new(EntityId::new(), Time::from_seconds(10.0), Time::SECOND);
        clip.source_in = Time::from_seconds(3.0);
        clip.properties.set("ve.scale", 2.0);
        assert_eq!(clip.value_at("ve.scale", Time::from_seconds(10.5), 1.0), 2.0);
        assert_eq!(clip.value_at("ve.rotation", Time::from_seconds(10.5), 0.0), 0.0);
        let curve = clip.animation.channel_mut("ve.scale");
        for (at, value) in [(3.0, 1.0), (4.0, 3.0)] {
            curve.insert(crate::Key {
                time: Time::from_seconds(at),
                value,
                interpolation: crate::Interpolation::Linear,
            });
        }
        assert!((clip.value_at("ve.scale", Time::from_seconds(10.5), 1.0) - 2.0).abs() < 1e-9);
        // Moved and trimmed, the key stays on the same source frame.
        clip.start = Time::from_seconds(20.0);
        clip.source_in = Time::from_seconds(3.5);
        assert!((clip.value_at("ve.scale", Time::from_seconds(20.0), 1.0) - 2.0).abs() < 1e-9);
        assert_eq!(clip.source_out(), Time::from_seconds(4.5));
    }

    #[test]
    fn a_sequence_edit_stores_only_the_tracks_it_changed_and_undoes_exactly() {
        use sue_core::{Entity, UndoStack};
        let mut arena = Arena::new();
        let mut sequence = edit();
        for index in 0..10 {
            sequence.tracks.push(Track::new(format!("V{index}"), "video"));
        }
        sequence.ensure_ids();
        let entity = Entity::empty("Edit", crate::class::SEQUENCE).with_ext(sequence.clone());
        let id = entity.id();
        arena.insert_keep_id(entity).unwrap();
        let mut after = sequence.clone();
        after.tracks[1].clip_mut(3).unwrap().duration = Time::from_seconds(2.0);
        let edit = SequenceEdit::between(id, "Trim", &sequence, &after).unwrap();
        assert_eq!(edit.tracks_held(), 1, "one track of twelve");
        assert!(SequenceEdit::between(id, "Nothing", &sequence, &sequence).is_none());
        let mut history = UndoStack::new();
        history.apply(&mut arena, edit).unwrap();
        let now = |arena: &Arena| arena.get_by_id(id).unwrap().ext::<Sequence>().unwrap().clone();
        assert_eq!(now(&arena), after);
        // A track added, a marker and a size changed: one step.
        let mut more = after.clone();
        more.tracks.insert(0, Track::new("V0", "video"));
        more.markers.push(Marker::new(Time::SECOND, "A"));
        more.properties.set("ve.height", 1080_i64);
        history
            .apply(
                &mut arena,
                SequenceEdit::between(id, "Add Track", &after, &more).unwrap(),
            )
            .unwrap();
        assert_eq!(now(&arena), more);
        assert!(history.undo(&mut arena).unwrap());
        assert_eq!(now(&arena), after);
        assert!(history.undo(&mut arena).unwrap());
        assert_eq!(now(&arena), sequence);
        assert!(history.redo(&mut arena).unwrap());
        assert!(history.redo(&mut arena).unwrap());
        assert_eq!(now(&arena), more);
        // An edit made for another state refuses rather than corrupts.
        let mut stale = SequenceEdit::between(id, "Stale", &sequence, &after).unwrap();
        stale.lengths = (1, 1);
        assert!(stale.apply(&mut arena).is_err());
        assert_eq!(now(&arena), more);
    }

    #[test]
    fn notes_carry_pitch_and_lyrics() {
        let mut notes = Notes::new();
        let beat = Time::from_seconds(0.5);
        for (index, (pitch, lyric)) in [(60.0, "Hap"), (60.0, "py"), (62.0, "birth")]
            .into_iter()
            .enumerate()
        {
            let mut note = Note::new(Time(beat.flicks() * index as i64), beat, pitch, 0.8);
            note.lyric = Some(lyric.to_owned());
            notes.insert(note);
        }
        assert_eq!(notes.lyrics(), "Hap py birth");
        assert_eq!(notes.at(Time::from_seconds(0.75)).count(), 1);
        assert!((notes.notes()[0].frequency() - 261.625_565).abs() < 1e-3);
        let mut out = Writer::new();
        ComponentType::encode(&notes, &mut out);
        let bytes = out.into_bytes();
        assert_eq!(Notes::decode(1, &mut Reader::new(&bytes)).unwrap(), notes);
    }
}
