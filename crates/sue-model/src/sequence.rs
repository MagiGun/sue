//! Sequences. A video edit, a song in a DAW, a slideshow with timings, a
//! podcast, an animation's dope sheet and a subtitle track are all tracks of
//! clips on the one clock. A [`Clip`] points at any entity by id (a video
//! file, a recording, a picture, a text, another sequence), so every feature
//! can be placed on every timeline. [`Notes`] hold note events (MIDI, a
//! piano roll, singing synthesis with lyrics and phonemes).

use sue_core::codec::{CodecError, Reader, Writer};
use sue_core::{ComponentType, EntityId, Properties, Rate, Time};

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
}

impl Clip {
    /// A clip of `source` at `start` for `duration`, untrimmed.
    #[must_use]
    pub fn new(source: EntityId, start: Time, duration: Time) -> Self {
        Self {
            source,
            start,
            source_in: Time::ZERO,
            duration,
            speed: 1.0,
            properties: Properties::new(),
        }
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

/// Tracks of clips at a frame rate and a sample rate.
#[derive(Clone, Debug, PartialEq)]
pub struct Sequence {
    /// The edit (frame) rate.
    pub rate: Rate,
    /// The audio sample rate.
    pub sample_rate: u32,
    /// Top track first.
    pub tracks: Vec<Track>,
}

impl Default for Sequence {
    fn default() -> Self {
        Self {
            rate: Rate::FPS_30,
            sample_rate: 48_000,
            tracks: Vec::new(),
        }
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
        }
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
}

fn decode_clip(input: &mut Reader<'_>) -> Result<Clip, CodecError> {
    Ok(Clip {
        source: read_id(input)?,
        start: Time(input.varint_signed()?),
        source_in: Time(input.varint_signed()?),
        duration: Time(input.varint_signed()?),
        speed: input.f64()?,
        properties: Properties::decode(input)?,
    })
}

impl ComponentType for Sequence {
    const NAME: &'static str = "sue.sequence";
    const VERSION: u32 = 1;

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
        }
    }

    fn decode(_version: u32, input: &mut Reader<'_>) -> Result<Self, CodecError> {
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
                track.insert(decode_clip(input)?);
            }
            sequence.tracks.push(track);
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
        for track in &mut self.tracks {
            for clip in &mut track.clips {
                clip.source = map(clip.source);
                clip.properties
                    .values_mut()
                    .for_each(|value| value.remap_entities(map));
            }
        }
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
        let (film, song) = (EntityId::new(), EntityId::new());
        let mut sequence = Sequence::new(Rate::FPS_24, 48_000);
        let mut video = Track::new("V1", "video");
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
        let back = Sequence::decode(1, &mut Reader::new(&bytes)).unwrap();
        assert_eq!(back, sequence);

        let other = EntityId::new();
        let mut moved = back;
        ComponentType::remap_entities(&mut moved, &mut |id| if id == film { other } else { id });
        assert_eq!(moved.tracks[0].clips()[0].source, other);
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
