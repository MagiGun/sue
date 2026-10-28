//! The audio transport shared by the Player, audio entities and the
//! timeline. A clip plays an arena entity, or a decoded track held in a
//! [`Shared`] buffer (the Player's library, which is not project data);
//! each tick feeds the platform sink's ring with the next slice of the
//! clip, resampled to the device rate (the ring is the device boundary,
//! like a texture upload for images).

use sue_core::{AssetHandle, ComponentKind, PCMFloat1D, Shared};
use sue_platform::AudioSink;

use crate::World;

/// Most device frames queued in one tick.
const FEED_CHUNK: usize = 16_384;

/// The decoded audio of `handle`.
#[must_use]
pub fn pcm_of(world: &World, handle: AssetHandle) -> Option<&PCMFloat1D> {
    world
        .arena
        .get(handle)
        .ok()?
        .component(ComponentKind::PCMFloat1D)?
        .as_pcm()
}

/// Up to `free` device frames from `pcm` starting at track frame `start`,
/// stopping at `end`, resampled linearly with channels mapped and `gain`
/// applied.
fn resample(
    pcm: &PCMFloat1D,
    start: f64,
    end: usize,
    ratio: f64,
    free: usize,
    channels_out: usize,
    gain: f32,
) -> Vec<f32> {
    let channels_in = usize::from(pcm.channels());
    let samples = pcm.samples();
    let last = end.min(pcm.frames()).saturating_sub(1);
    let mut out = Vec::with_capacity(free * channels_out);
    for frame in 0..free {
        let t = start + frame as f64 * ratio;
        if t >= last as f64 {
            break;
        }
        let i0 = t.floor() as usize;
        let mix = (t - i0 as f64) as f32;
        let sample = |channel: usize| {
            let a = samples[i0 * channels_in + channel];
            let b = samples[(i0 + 1) * channels_in + channel];
            (a + (b - a) * mix) * gain
        };
        for channel in 0..channels_out {
            out.push(match (channels_in, channels_out) {
                (1, _) if channel < 2 => sample(0),
                (_, 1) => (sample(0) + sample(1)) / 2.0,
                (inputs, _) if channel < inputs => sample(channel),
                _ => 0.0,
            });
        }
    }
    out
}

/// Where a clip's sound comes from.
#[derive(Clone, PartialEq)]
pub enum ClipSource {
    /// An audio entity in the arena.
    Entity(AssetHandle),
    /// A decoded track outside the arena, shared without copying.
    Stream(Shared<PCMFloat1D>),
}

impl std::fmt::Debug for ClipSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Entity(handle) => f.debug_tuple("Entity").field(handle).finish(),
            Self::Stream(pcm) => write!(f, "Stream({} frames)", pcm.frames()),
        }
    }
}

impl ClipSource {
    /// The entity, if the sound comes from the arena.
    #[must_use]
    pub fn entity(&self) -> Option<AssetHandle> {
        match self {
            Self::Entity(handle) => Some(*handle),
            Self::Stream(_) => None,
        }
    }

    fn pcm<'w>(&'w self, world: &'w World) -> Option<&'w PCMFloat1D> {
        match self {
            Self::Entity(handle) => pcm_of(world, *handle),
            Self::Stream(pcm) => Some(pcm),
        }
    }
}

/// What is loaded into the transport.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    /// Where the sound comes from.
    pub source: ClipSource,
    /// First frame to play.
    pub start: usize,
    /// Frame to stop at.
    pub end: usize,
    /// Linear gain.
    pub gain: f32,
    /// Who started it: `"player"`, `"timeline"` or a node.
    pub owner: String,
}

/// Plays one clip at a time through the platform sink.
#[derive(Debug, Default)]
pub struct Transport {
    clip: Option<Clip>,
    playing: bool,
    anchor_frame: f64,
    anchor_played: u64,
    feed: f64,
    position: f64,
    last_played: u64,
}

impl Transport {
    /// Loads `clip` at its start, paused. What was queued is dropped.
    pub fn load(&mut self, clip: Clip, sink: Option<&mut (dyn AudioSink + 'static)>) {
        let start = clip.start as f64;
        self.clip = Some(clip);
        self.seek(start, sink);
    }

    /// The loaded clip.
    #[must_use]
    pub fn clip(&self) -> Option<&Clip> {
        self.clip.as_ref()
    }

    /// `true` while playing.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// `true` if `owner` loaded the clip and it plays.
    #[must_use]
    pub fn is_playing_for(&self, owner: &str) -> bool {
        self.playing && self.clip.as_ref().is_some_and(|clip| clip.owner == owner)
    }

    /// The track frame being heard.
    #[must_use]
    pub fn position(&self) -> f64 {
        self.position
    }

    /// Starts or pauses.
    pub fn set_playing(&mut self, playing: bool, sink: Option<&mut (dyn AudioSink + 'static)>) {
        self.playing = playing && self.clip.is_some();
        if let Some(sink) = sink {
            sink.set_playing(self.playing);
        }
    }

    /// Changes the gain of the loaded clip.
    pub fn set_gain(&mut self, gain: f32) {
        if let Some(clip) = self.clip.as_mut() {
            clip.gain = gain;
        }
    }

    /// Jumps to track frame `frame`, dropping what is queued.
    pub fn seek(&mut self, frame: f64, sink: Option<&mut (dyn AudioSink + 'static)>) {
        self.anchor_frame = frame;
        self.feed = frame;
        self.position = frame;
        if let Some(sink) = sink {
            sink.clear();
            self.anchor_played = sink.played_frames();
            self.last_played = self.anchor_played;
        } else {
            self.anchor_played = 0;
            self.last_played = 0;
        }
    }

    /// Stops and unloads.
    pub fn stop(&mut self, sink: Option<&mut (dyn AudioSink + 'static)>) {
        self.playing = false;
        self.clip = None;
        if let Some(sink) = sink {
            sink.clear();
            sink.set_playing(false);
        }
    }

    /// Feeds the sink and advances the position. Returns `true` when the
    /// clip finished (and stops).
    pub fn pump(&mut self, world: &World, sink: &mut dyn AudioSink, volume: f32) -> bool {
        let Some(clip) = self.clip.clone() else {
            return false;
        };
        if !self.playing {
            return false;
        }
        let Some(pcm) = clip.source.pcm(world) else {
            self.playing = false;
            return true;
        };
        let end = clip.end.min(pcm.frames());
        let played = sink.played_frames();
        if played < self.last_played {
            // A new device: what was queued is gone, carry on from here.
            self.anchor_frame = self.position;
            self.anchor_played = played;
            self.feed = self.position;
        }
        self.last_played = played;
        sink.set_volume(volume);
        sink.set_playing(true);
        let ratio = f64::from(pcm.sample_rate()) / f64::from(sink.info().sample_rate.max(1));
        let free = sink.free_frames().min(FEED_CHUNK);
        if free > 0 && self.feed < end as f64 {
            let channels = usize::from(sink.info().channels.max(1));
            let samples = resample(pcm, self.feed, end, ratio, free, channels, clip.gain);
            let pushed = sink.push(&samples);
            self.feed += pushed as f64 * ratio;
        }
        self.position = (self.anchor_frame
            + (played.saturating_sub(self.anchor_played)) as f64 * ratio)
            .min(end as f64);
        if self.position >= end as f64 - 1.0 && sink.queued_frames() == 0 {
            self.playing = false;
            sink.set_playing(false);
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_platform::NullAudio;

    fn world_with_tone(rate: u32, seconds: usize) -> (World, AssetHandle) {
        let mut world = World::default();
        let samples = (0..rate as usize * seconds)
            .map(|i| (i as f32 * 0.01).sin() * 0.5)
            .collect();
        let pcm = PCMFloat1D::new(rate, 1, samples).unwrap();
        let handle = world.arena.spawn("tone", pcm);
        (world, handle)
    }

    #[test]
    fn a_clip_range_plays_through_the_sink_and_ends() {
        let (world, handle) = world_with_tone(8_000, 1);
        let mut sink = NullAudio::new(8_000, 2, 4_000);
        let mut transport = Transport::default();
        transport.load(
            Clip {
                source: ClipSource::Entity(handle),
                start: 2_000,
                end: 4_000,
                gain: 0.5,
                owner: "test".to_owned(),
            },
            Some(&mut sink),
        );
        transport.set_playing(true, Some(&mut sink));
        assert!(transport.is_playing_for("test"));
        let mut ended = false;
        for _ in 0..64 {
            ended = transport.pump(&world, &mut sink, 1.0);
            sink.advance(1_000);
            if ended {
                break;
            }
        }
        assert!(ended, "the clip never ended");
        assert!(!transport.is_playing());
        assert!(transport.position() >= 3_999.0);
    }

    #[test]
    fn a_shared_track_plays_without_the_arena() {
        let pcm = PCMFloat1D::new(8_000, 1, vec![0.25; 8_000]).unwrap();
        let shared = Shared::new(pcm);
        let mut sink = NullAudio::new(8_000, 2, 4_000);
        let mut transport = Transport::default();
        transport.load(
            Clip {
                source: ClipSource::Stream(shared.clone()),
                start: 0,
                end: 8_000,
                gain: 1.0,
                owner: "player".to_owned(),
            },
            Some(&mut sink),
        );
        transport.set_playing(true, Some(&mut sink));
        let world = World::default();
        let mut ended = false;
        for _ in 0..64 {
            ended = transport.pump(&world, &mut sink, 1.0);
            sink.advance(1_000);
            if ended {
                break;
            }
        }
        assert!(ended);
        assert_eq!(shared.holders(), 2, "played from the one buffer, no copy");
    }
}
