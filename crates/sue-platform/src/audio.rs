//! Audio output (HUMANZ §3.5): a `cpal` device stream fed from a lock-free
//! single-producer, single-consumer ring.
//!
//! The UI thread pushes interleaved `f32` frames; the device callback only
//! pops them. The callback never locks, allocates or blocks: the ring slots
//! are atomics, so the whole path is safe Rust. A paused stream is stopped
//! at the device, so an idle player costs no CPU.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};

use crate::PlatformError;

/// What the user asked for (from rice).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioConfig {
    /// Device name, or `None` for the system default.
    pub device: Option<String>,
    /// Requested sample rate in Hz.
    pub sample_rate: u32,
    /// Requested frames per callback.
    pub buffer_frames: u32,
    /// Ask the OS to schedule the callback thread in real time.
    pub realtime: bool,
    /// Ring capacity, in milliseconds of audio.
    pub queue_ms: u32,
}

/// Whether the callback thread runs at real-time priority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealtimeStatus {
    /// Not requested.
    Off,
    /// Requested; the callback has not run yet.
    Pending,
    /// The OS accepted the request.
    Granted,
    /// The OS refused it; the backend's default priority applies.
    Refused,
}

impl RealtimeStatus {
    fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Pending,
            2 => Self::Granted,
            3 => Self::Refused,
            _ => Self::Off,
        }
    }
}

/// What the device actually runs at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioStreamInfo {
    /// Device name.
    pub device: String,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Interleaved channels.
    pub channels: u16,
    /// Frames per callback, if the device accepted a fixed size.
    pub buffer_frames: Option<u32>,
}

/// Where a player sends audio. Implemented by [`AudioOutput`] (a device) and
/// [`NullAudio`] (a clock without a device, for tests and headless runs).
pub trait AudioSink {
    /// What the stream runs at.
    fn info(&self) -> &AudioStreamInfo;
    /// Frames that fit into the queue now.
    fn free_frames(&self) -> usize;
    /// Queued frames the device has not played yet.
    fn queued_frames(&self) -> usize;
    /// Queues interleaved samples (`info().channels` per frame). Returns the
    /// number of whole frames accepted.
    fn push(&mut self, samples: &[f32]) -> usize;
    /// Drops everything queued, e.g. after a seek.
    fn clear(&mut self);
    /// Starts or stops the device. A stopped stream uses no CPU.
    fn set_playing(&mut self, playing: bool);
    /// Output gain, `0.0..=1.0`, applied in the callback so it takes effect
    /// at once, not after the queued audio.
    fn set_volume(&mut self, volume: f32);
    /// Frames the device has played since the stream opened.
    fn played_frames(&self) -> u64;
    /// Callbacks that found the queue empty while playing.
    fn underruns(&self) -> u64;
    /// Real-time scheduling of the callback thread.
    fn realtime(&self) -> RealtimeStatus;
}

/// A lock-free SPSC ring of `f32` samples stored as atomic bit patterns.
struct Ring {
    slots: Box<[AtomicU32]>,
    /// Total samples ever written (producer-owned).
    head: AtomicUsize,
    /// Total samples ever read (consumer-owned).
    tail: AtomicUsize,
    /// The consumer skips to this index when `flush` is set.
    flush_to: AtomicUsize,
    flush: AtomicBool,
}

impl Ring {
    fn new(capacity: usize) -> Self {
        Self {
            slots: (0..capacity.max(1)).map(|_| AtomicU32::new(0)).collect(),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            flush_to: AtomicUsize::new(0),
            flush: AtomicBool::new(false),
        }
    }

    fn capacity(&self) -> usize {
        self.slots.len()
    }

    fn len(&self) -> usize {
        self.head
            .load(Ordering::Acquire)
            .wrapping_sub(self.tail.load(Ordering::Acquire))
            .min(self.capacity())
    }

    /// Producer side: writes as many samples as fit.
    fn push(&self, samples: &[f32]) -> usize {
        let head = self.head.load(Ordering::Relaxed);
        let free = self.capacity() - self.len();
        let count = samples.len().min(free);
        for (offset, sample) in samples[..count].iter().enumerate() {
            self.slots[head.wrapping_add(offset) % self.capacity()]
                .store(sample.to_bits(), Ordering::Relaxed);
        }
        self.head.store(head.wrapping_add(count), Ordering::Release);
        count
    }

    /// Producer side: drops what is queued now; later pushes are kept.
    fn flush(&self) {
        self.flush_to
            .store(self.head.load(Ordering::Relaxed), Ordering::Release);
        self.flush.store(true, Ordering::Release);
    }

    /// Consumer side: fills `out` from the queue. Returns samples written.
    fn pop(&self, out: &mut [f32]) -> usize {
        let mut tail = self.tail.load(Ordering::Relaxed);
        if self.flush.swap(false, Ordering::AcqRel) {
            let target = self.flush_to.load(Ordering::Acquire);
            if target.wrapping_sub(tail) <= self.capacity() {
                tail = target;
            }
        }
        let available = self.head.load(Ordering::Acquire).wrapping_sub(tail);
        let count = out.len().min(available);
        for (offset, slot) in out[..count].iter_mut().enumerate() {
            *slot = f32::from_bits(
                self.slots[tail.wrapping_add(offset) % self.capacity()].load(Ordering::Relaxed),
            );
        }
        self.tail.store(tail.wrapping_add(count), Ordering::Release);
        count
    }
}

/// Counters the callback updates.
struct Shared {
    played_frames: AtomicU64,
    underruns: AtomicU64,
    playing: AtomicBool,
    realtime: AtomicU8,
    /// Gain as `f32` bits.
    volume: AtomicU32,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            played_frames: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            realtime: AtomicU8::new(0),
            volume: AtomicU32::new(1.0_f32.to_bits()),
        }
    }
}

/// A live device stream.
pub struct AudioOutput {
    stream: cpal::Stream,
    ring: Arc<Ring>,
    shared: Arc<Shared>,
    info: AudioStreamInfo,
    playing: bool,
}

impl std::fmt::Debug for AudioOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioOutput")
            .field("info", &self.info)
            .field("playing", &self.playing)
            .finish_non_exhaustive()
    }
}

/// Names of the output devices, the system default first.
#[must_use]
pub fn audio_output_devices() -> Vec<String> {
    let host = cpal::default_host();
    let default = host
        .default_output_device()
        .and_then(|device| device.name().ok());
    let mut names: Vec<String> = host
        .output_devices()
        .map(|devices| devices.filter_map(|device| device.name().ok()).collect())
        .unwrap_or_default();
    names.sort();
    names.dedup();
    if let Some(default) = default {
        names.retain(|name| name != &default);
        names.insert(0, default);
    }
    names
}

fn audio_error(what: &str, error: impl std::fmt::Display) -> PlatformError {
    PlatformError::Audio(format!("{what}: {error}"))
}

impl AudioOutput {
    /// Opens the device named in `config` (or the default), stopped.
    ///
    /// # Errors
    /// [`PlatformError::Audio`] when there is no device or it refuses every
    /// configuration.
    pub fn open(config: &AudioConfig) -> Result<Self, PlatformError> {
        let host = cpal::default_host();
        let device = match &config.device {
            Some(wanted) => host
                .output_devices()
                .map_err(|error| audio_error("cannot list devices", error))?
                .find(|device| device.name().is_ok_and(|name| &name == wanted))
                .ok_or_else(|| PlatformError::Audio(format!("no output device named {wanted}")))?,
            None => host
                .default_output_device()
                .ok_or_else(|| PlatformError::Audio("no output device".to_owned()))?,
        };
        let name = device.name().unwrap_or_else(|_| "Output".to_owned());
        let rate = cpal::SampleRate(config.sample_rate);
        // Prefer f32 at the requested rate, then any format at that rate, then
        // whatever the device likes best.
        let ranges: Vec<cpal::SupportedStreamConfigRange> = device
            .supported_output_configs()
            .map(Iterator::collect)
            .unwrap_or_default();
        let at_rate = |format: Option<cpal::SampleFormat>| {
            ranges
                .iter()
                .filter(|range| range.min_sample_rate() <= rate && rate <= range.max_sample_rate())
                .filter(|range| format.is_none_or(|format| range.sample_format() == format))
                .max_by_key(|range| u16::from(range.channels() == 2))
                .map(|range| range.with_sample_rate(rate))
        };
        let supported = match at_rate(Some(cpal::SampleFormat::F32)).or_else(|| at_rate(None)) {
            Some(supported) => supported,
            None => device
                .default_output_config()
                .map_err(|error| audio_error("no usable configuration", error))?,
        };
        let mut stream_config = supported.config();
        let buffer_frames = match supported.buffer_size() {
            cpal::SupportedBufferSize::Range { min, max }
                if (*min..=*max).contains(&config.buffer_frames) =>
            {
                stream_config.buffer_size = cpal::BufferSize::Fixed(config.buffer_frames);
                Some(config.buffer_frames)
            }
            _ => None,
        };
        let channels = stream_config.channels.max(1);
        let sample_rate = stream_config.sample_rate.0;
        let capacity = (sample_rate as usize * config.queue_ms.max(20) as usize / 1000)
            * usize::from(channels);
        let ring = Arc::new(Ring::new(capacity));
        let shared = Arc::new(Shared::default());
        shared
            .realtime
            .store(if config.realtime { 1 } else { 0 }, Ordering::Relaxed);
        let promote = (buffer_frames.unwrap_or(0), sample_rate);
        let stream = match supported.sample_format() {
            cpal::SampleFormat::I16 => {
                build::<i16>(&device, &stream_config, &ring, &shared, channels, promote)
            }
            cpal::SampleFormat::U16 => {
                build::<u16>(&device, &stream_config, &ring, &shared, channels, promote)
            }
            _ => build::<f32>(&device, &stream_config, &ring, &shared, channels, promote),
        }?;
        // Streams may start running on creation; stay quiet until asked.
        let _ = stream.pause();
        Ok(Self {
            stream,
            ring,
            shared,
            info: AudioStreamInfo {
                device: name,
                sample_rate,
                channels,
                buffer_frames,
            },
            playing: false,
        })
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    ring: &Arc<Ring>,
    shared: &Arc<Shared>,
    channels: u16,
    (buffer_frames, sample_rate): (u32, u32),
) -> Result<cpal::Stream, PlatformError>
where
    T: SizedSample + FromSample<f32>,
{
    let (ring, shared) = (Arc::clone(ring), Arc::clone(shared));
    // Scratch space is allocated once, here, never in the callback.
    let mut scratch = vec![0.0_f32; 16_384];
    let data = move |out: &mut [T], _: &cpal::OutputCallbackInfo| {
        if shared.realtime.load(Ordering::Relaxed) == 1 {
            let granted = audio_thread_priority::promote_current_thread_to_real_time(
                buffer_frames,
                sample_rate,
            )
            .is_ok();
            shared
                .realtime
                .store(if granted { 2 } else { 3 }, Ordering::Relaxed);
        }
        let gain = f32::from_bits(shared.volume.load(Ordering::Relaxed));
        let mut written = 0;
        for chunk in out.chunks_mut(scratch.len()) {
            let scratch = &mut scratch[..chunk.len()];
            let got = ring.pop(scratch);
            scratch[got..].fill(0.0);
            for (out, sample) in chunk.iter_mut().zip(scratch.iter()) {
                *out = T::from_sample(*sample * gain);
            }
            written += got;
        }
        if written < out.len() && shared.playing.load(Ordering::Relaxed) {
            shared.underruns.fetch_add(1, Ordering::Relaxed);
        }
        shared.played_frames.fetch_add(
            (written / usize::from(channels.max(1))) as u64,
            Ordering::Relaxed,
        );
    };
    device
        .build_output_stream::<T, _, _>(
            config,
            data,
            |error| eprintln!("sue: audio stream error: {error}"),
            None,
        )
        .map_err(|error| audio_error("cannot open the stream", error))
}

impl AudioSink for AudioOutput {
    fn info(&self) -> &AudioStreamInfo {
        &self.info
    }

    fn free_frames(&self) -> usize {
        (self.ring.capacity() - self.ring.len()) / usize::from(self.info.channels)
    }

    fn queued_frames(&self) -> usize {
        self.ring.len() / usize::from(self.info.channels)
    }

    fn push(&mut self, samples: &[f32]) -> usize {
        let channels = usize::from(self.info.channels);
        let whole = samples.len() / channels * channels;
        let fit = (self.free_frames() * channels).min(whole);
        self.ring.push(&samples[..fit]) / channels
    }

    fn clear(&mut self) {
        self.ring.flush();
    }

    fn set_playing(&mut self, playing: bool) {
        if playing == self.playing {
            return;
        }
        self.playing = playing;
        self.shared.playing.store(playing, Ordering::Relaxed);
        let result = if playing {
            self.stream.play().map_err(|error| error.to_string())
        } else {
            self.stream.pause().map_err(|error| error.to_string())
        };
        if let Err(error) = result {
            eprintln!("sue: audio stream: {error}");
        }
    }

    fn set_volume(&mut self, volume: f32) {
        self.shared
            .volume
            .store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    fn played_frames(&self) -> u64 {
        self.shared.played_frames.load(Ordering::Relaxed)
    }

    fn underruns(&self) -> u64 {
        self.shared.underruns.load(Ordering::Relaxed)
    }

    fn realtime(&self) -> RealtimeStatus {
        RealtimeStatus::from_code(self.shared.realtime.load(Ordering::Relaxed))
    }
}

/// A sink without a device: it "plays" queued audio when told to by
/// [`NullAudio::advance`], so players can be tested headlessly.
#[derive(Debug)]
pub struct NullAudio {
    info: AudioStreamInfo,
    capacity_frames: usize,
    queued: usize,
    played: u64,
    playing: bool,
    volume: f32,
}

impl NullAudio {
    /// A null stream at `sample_rate` with `channels`, queueing up to
    /// `capacity_frames`.
    #[must_use]
    pub fn new(sample_rate: u32, channels: u16, capacity_frames: usize) -> Self {
        Self {
            info: AudioStreamInfo {
                device: "No device".to_owned(),
                sample_rate,
                channels: channels.max(1),
                buffer_frames: None,
            },
            capacity_frames,
            queued: 0,
            played: 0,
            playing: false,
            volume: 1.0,
        }
    }

    /// The gain last set.
    #[must_use]
    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Plays `frames` of the queue if the stream is running.
    pub fn advance(&mut self, frames: usize) {
        if self.playing {
            let played = frames.min(self.queued);
            self.queued -= played;
            self.played += played as u64;
        }
    }

    /// `true` while started.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing
    }
}

impl AudioSink for NullAudio {
    fn info(&self) -> &AudioStreamInfo {
        &self.info
    }

    fn free_frames(&self) -> usize {
        self.capacity_frames - self.queued
    }

    fn queued_frames(&self) -> usize {
        self.queued
    }

    fn push(&mut self, samples: &[f32]) -> usize {
        let frames = (samples.len() / usize::from(self.info.channels)).min(self.free_frames());
        self.queued += frames;
        frames
    }

    fn clear(&mut self) {
        self.queued = 0;
    }

    fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
    }

    fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    fn played_frames(&self) -> u64 {
        self.played
    }

    fn underruns(&self) -> u64 {
        0
    }

    fn realtime(&self) -> RealtimeStatus {
        RealtimeStatus::Off
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_is_fifo_bounded_and_flushable() {
        let ring = Ring::new(8);
        assert_eq!(ring.push(&[1.0, 2.0, 3.0]), 3);
        let mut out = [0.0; 2];
        assert_eq!(ring.pop(&mut out), 2);
        assert_eq!(out, [1.0, 2.0]);
        assert_eq!(ring.push(&[4.0; 10]), 7, "only free slots are written");
        assert_eq!(ring.len(), 8);
        ring.flush();
        assert_eq!(
            ring.push(&[9.0]),
            0,
            "still full until the consumer flushes"
        );
        let mut out = [0.0; 8];
        assert_eq!(ring.pop(&mut out), 0, "the flushed audio is dropped");
        assert_eq!(ring.push(&[9.0, 8.0]), 2);
        assert_eq!(ring.pop(&mut out), 2);
        assert_eq!(&out[..2], &[9.0, 8.0]);
    }

    #[test]
    fn the_ring_survives_concurrent_use() {
        let ring = Arc::new(Ring::new(64));
        let producer = Arc::clone(&ring);
        let writer = std::thread::spawn(move || {
            let mut next = 0.0_f32;
            while next < 10_000.0 {
                if producer.push(&[next]) == 1 {
                    next += 1.0;
                }
            }
        });
        let mut expected = 0.0_f32;
        let mut out = [0.0_f32; 16];
        while expected < 10_000.0 {
            let got = ring.pop(&mut out);
            for sample in &out[..got] {
                assert_eq!(*sample, expected);
                expected += 1.0;
            }
        }
        writer.join().unwrap();
    }

    #[test]
    #[ignore = "needs an audio output device; run with --ignored"]
    fn the_default_device_consumes_queued_audio() {
        let config = AudioConfig {
            device: None,
            sample_rate: 48_000,
            buffer_frames: 512,
            realtime: true,
            queue_ms: 500,
        };
        let mut output = AudioOutput::open(&config).expect("an output device");
        let channels = usize::from(output.info().channels);
        let quarter_second = output.info().sample_rate as usize / 4 * channels;
        // Silence: the pipeline is exercised without making a sound.
        assert!(output.push(&vec![0.0; quarter_second]) > 0);
        output.set_playing(true);
        std::thread::sleep(std::time::Duration::from_millis(400));
        output.set_playing(false);
        assert!(output.played_frames() > 0, "the device consumed the queue");
        assert_ne!(output.realtime(), RealtimeStatus::Pending);
        eprintln!(
            "{:?}, played {} frames, real-time {:?}, underruns {}",
            output.info(),
            output.played_frames(),
            output.realtime(),
            output.underruns()
        );
    }

    #[test]
    fn null_audio_plays_only_while_started() {
        let mut sink = NullAudio::new(48_000, 2, 100);
        assert_eq!(sink.push(&[0.0; 300]), 100);
        sink.advance(40);
        assert_eq!(sink.played_frames(), 0);
        sink.set_playing(true);
        sink.advance(40);
        assert_eq!((sink.played_frames(), sink.queued_frames()), (40, 60));
        sink.clear();
        assert_eq!(sink.free_frames(), 100);
    }
}
