//! Audio: decoding MP3, FLAC, WAV, Ogg Vorbis and AAC / M4A with `symphonia`
//! into interleaved `f32` (`PCMFloat1D`), and writing 16-bit PCM WAV files.

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;

use sue_core::PCMFloat1D;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTagKey};
use symphonia::core::probe::Hint;

/// Facts about a decoded track.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioInfo {
    /// Codec name, e.g. `MP3`.
    pub codec: String,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Interleaved channels.
    pub channels: u16,
    /// Frames (samples per channel).
    pub frames: usize,
    /// Track title tag.
    pub title: Option<String>,
    /// Artist tag.
    pub artist: Option<String>,
    /// Album tag.
    pub album: Option<String>,
    /// Track number tag (`3`, or the `3` of `3/12`).
    pub track: Option<u32>,
    /// Album artist tag.
    pub album_artist: Option<String>,
    /// Genre tag.
    pub genre: Option<String>,
}

/// A decoded track.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedAudio {
    /// Interleaved samples.
    pub pcm: PCMFloat1D,
    /// What the file said.
    pub info: AudioInfo,
}

/// Shares the encoded bytes with the decoder without copying them, so they
/// can be handed back to the caller afterwards.
struct SharedBytes {
    bytes: Arc<Vec<u8>>,
    position: u64,
}

impl Read for SharedBytes {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let start = usize::try_from(self.position)
            .unwrap_or(usize::MAX)
            .min(self.bytes.len());
        let count = buf.len().min(self.bytes.len() - start);
        buf[..count].copy_from_slice(&self.bytes[start..start + count]);
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for SharedBytes {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let length = self.bytes.len() as i128;
        let target = match to {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::End(offset) => length + i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.position) + i128::from(offset),
        };
        if target < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before the start",
            ));
        }
        self.position = u64::try_from(target).unwrap_or(u64::MAX);
        Ok(self.position)
    }
}

impl MediaSource for SharedBytes {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.bytes.len() as u64)
    }
}

fn read_tags(revision: &MetadataRevision, info: &mut AudioInfo) {
    for tag in revision.tags() {
        if tag.std_key == Some(StandardTagKey::TrackNumber) && info.track.is_none() {
            let text = tag.value.to_string();
            info.track = text
                .split('/')
                .next()
                .and_then(|number| number.trim().parse().ok());
            continue;
        }
        let slot = match tag.std_key {
            Some(StandardTagKey::TrackTitle) => &mut info.title,
            Some(StandardTagKey::Artist) => &mut info.artist,
            Some(StandardTagKey::Album) => &mut info.album,
            Some(StandardTagKey::AlbumArtist) => &mut info.album_artist,
            Some(StandardTagKey::Genre) => &mut info.genre,
            _ => continue,
        };
        let value = tag.value.to_string();
        if slot.is_none() && !value.trim().is_empty() {
            *slot = Some(value.trim().to_owned());
        }
    }
}

/// Reads a track's tags, format and length from the file at `path`
/// without decoding its sound, so a whole music folder can be listed
/// quickly. `frames` is 0 when the container does not say.
///
/// # Errors
/// If the file cannot be read or holds no audio.
pub fn probe_audio(path: &std::path::Path) -> Result<AudioInfo, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
        hint.with_extension(extension);
    }
    let mut probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|error| error.to_string())?;
    let mut info = AudioInfo::default();
    if let Some(revision) = probed.metadata.get().as_ref().and_then(|log| log.current()) {
        read_tags(revision, &mut info);
    }
    let format = &mut probed.format;
    if let Some(revision) = format.metadata().current() {
        read_tags(revision, &mut info);
    }
    let track = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| "the file has no audio track".to_owned())?;
    let params = &track.codec_params;
    info.sample_rate = params.sample_rate.unwrap_or(0);
    info.channels = params.channels.map_or(0, |set| set.count()) as u16;
    info.frames = params.n_frames.map_or(0, |frames| frames as usize);
    info.codec = symphonia::default::get_codecs()
        .get_codec(params.codec)
        .map_or_else(
            || "Audio".to_owned(),
            |codec| match codec.short_name {
                name if name.starts_with("pcm") => "PCM".to_owned(),
                "vorbis" => "Vorbis".to_owned(),
                name => name.to_uppercase(),
            },
        );
    Ok(info)
}

/// Decodes a whole track. `extension` (e.g. `mp3`) helps the prober; the
/// bytes decide. The bytes are returned untouched on failure so the caller
/// can still show them raw.
///
/// # Errors
/// The decoder's message, plus the original bytes.
pub fn decode_audio(
    bytes: Vec<u8>,
    extension: Option<&str>,
) -> Result<(DecodedAudio, Vec<u8>), (String, Vec<u8>)> {
    let shared = Arc::new(bytes);
    let result = decode_shared(&shared, extension);
    // Every reader the decoder held is dropped by now.
    let bytes = Arc::try_unwrap(shared).unwrap_or_else(|shared| shared.as_ref().clone());
    match result {
        Ok(decoded) => Ok((decoded, bytes)),
        Err(message) => Err((message, bytes)),
    }
}

fn decode_shared(bytes: &Arc<Vec<u8>>, extension: Option<&str>) -> Result<DecodedAudio, String> {
    let source = SharedBytes {
        bytes: Arc::clone(bytes),
        position: 0,
    };
    let stream = MediaSourceStream::new(Box::new(source), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = extension {
        hint.with_extension(extension);
    }
    let mut probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|error| error.to_string())?;
    let mut info = AudioInfo::default();
    if let Some(revision) = probed.metadata.get().as_ref().and_then(|log| log.current()) {
        read_tags(revision, &mut info);
    }
    let format = &mut probed.format;
    if let Some(revision) = format.metadata().current() {
        read_tags(revision, &mut info);
    }
    let track = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| "the file has no audio track".to_owned())?;
    let track_id = track.id;
    info.codec = symphonia::default::get_codecs()
        .get_codec(track.codec_params.codec)
        .map_or_else(
            || "Audio".to_owned(),
            |codec| match codec.short_name {
                name if name.starts_with("pcm") => "PCM".to_owned(),
                "vorbis" => "Vorbis".to_owned(),
                name => name.to_uppercase(),
            },
        );
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|error| error.to_string())?;
    let mut samples: Vec<f32> = Vec::new();
    let mut buffer: Option<SampleBuffer<f32>> = None;
    let (mut rate, mut channels) = (
        track.codec_params.sample_rate.unwrap_or(0),
        track.codec_params.channels.map_or(0, |set| set.count()),
    );
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error)) if error.kind() == io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(error) => return Err(error.to_string()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            // A damaged frame: skip it, like every player does.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(error) => return Err(error.to_string()),
        };
        let spec = *decoded.spec();
        rate = spec.rate;
        channels = spec.channels.count();
        let needed = decoded.capacity() as u64;
        let reuse = buffer.as_ref().is_some_and(|buffer| {
            buffer.capacity() as u64 >= needed * spec.channels.count() as u64
        });
        if !reuse {
            buffer = Some(SampleBuffer::new(needed, spec));
        }
        if let Some(buffer) = buffer.as_mut() {
            buffer.copy_interleaved_ref(decoded);
            samples.extend_from_slice(buffer.samples());
        }
    }
    let channels = u16::try_from(channels.max(1)).unwrap_or(u16::MAX);
    if rate == 0 || samples.is_empty() {
        return Err("the track decoded to silence: no samples".to_owned());
    }
    samples.truncate(samples.len() / usize::from(channels) * usize::from(channels));
    let pcm = PCMFloat1D::new(rate, channels, samples).map_err(|error| error.to_string())?;
    info.sample_rate = rate;
    info.channels = channels;
    info.frames = pcm.frames();
    Ok(DecodedAudio { pcm, info })
}

/// Encodes audio as a 16-bit PCM WAV file (samples are clamped to ±1).
#[must_use]
pub fn encode_wav(pcm: &PCMFloat1D) -> Vec<u8> {
    let channels = pcm.channels();
    let data_len = u32::try_from(pcm.samples().len() * 2).unwrap_or(u32::MAX - 36);
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&pcm.sample_rate().to_le_bytes());
    out.extend_from_slice(&(pcm.sample_rate() * u32::from(channels) * 2).to_le_bytes());
    out.extend_from_slice(&(channels * 2).to_le_bytes());
    out.extend_from_slice(&16_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &sample in pcm.samples() {
        let value = (sample.clamp(-1.0, 1.0) * 32_767.0).round() as i16;
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// How many bits each sample of a written file has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SampleDepth {
    /// 16-bit integers, as on a CD.
    Int16,
    /// 24-bit integers, as studios use.
    Int24,
    /// 32-bit floats, exactly as Sue holds them (WAV only).
    Float32,
}

impl SampleDepth {
    fn bits(self) -> u16 {
        match self {
            Self::Int16 => 16,
            Self::Int24 => 24,
            Self::Float32 => 32,
        }
    }

    fn integer(self, sample: f32) -> i32 {
        let sample = f64::from(sample.clamp(-1.0, 1.0));
        match self {
            Self::Int16 => (sample * 32_767.0).round() as i32,
            Self::Int24 | Self::Float32 => (sample * 8_388_607.0).round() as i32,
        }
    }
}

/// A WAV file of `pcm` at `depth`.
#[must_use]
pub fn encode_wav_as(pcm: &PCMFloat1D, depth: SampleDepth) -> Vec<u8> {
    if depth == SampleDepth::Int16 {
        return encode_wav(pcm);
    }
    let channels = pcm.channels();
    let bytes_per_sample = u32::from(depth.bits() / 8);
    let data_len = u32::try_from(pcm.samples().len() as u64 * u64::from(bytes_per_sample))
        .unwrap_or(u32::MAX - 36);
    let format: u16 = if depth == SampleDepth::Float32 { 3 } else { 1 };
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&format.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&pcm.sample_rate().to_le_bytes());
    out.extend_from_slice(
        &(pcm.sample_rate() * u32::from(channels) * bytes_per_sample).to_le_bytes(),
    );
    out.extend_from_slice(&(channels * depth.bits() / 8).to_le_bytes());
    out.extend_from_slice(&depth.bits().to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &sample in pcm.samples() {
        match depth {
            SampleDepth::Float32 => out.extend_from_slice(&sample.clamp(-1.0, 1.0).to_le_bytes()),
            _ => out.extend_from_slice(&depth.integer(sample).to_le_bytes()[..3]),
        }
    }
    out
}

/// An 80-bit extended float, as AIFF writes its sample rate.
fn extended(value: f64) -> [u8; 10] {
    let mut out = [0_u8; 10];
    if value <= 0.0 {
        return out;
    }
    let exponent = value.log2().floor() as i32;
    let mantissa = (value / 2_f64.powi(exponent - 63)) as u64;
    out[..2].copy_from_slice(&((exponent + 16_383) as u16).to_be_bytes());
    out[2..].copy_from_slice(&mantissa.to_be_bytes());
    out
}

/// An AIFF file of `pcm` with 16 or 24-bit samples (32-bit floats are
/// written as 24-bit).
#[must_use]
pub fn encode_aiff(pcm: &PCMFloat1D, depth: SampleDepth) -> Vec<u8> {
    let depth = if depth == SampleDepth::Float32 {
        SampleDepth::Int24
    } else {
        depth
    };
    let bytes = usize::from(depth.bits() / 8);
    let channels = pcm.channels();
    let frames = u32::try_from(pcm.frames()).unwrap_or(u32::MAX);
    let data = pcm.samples().len() * bytes;
    let ssnd = 8 + data as u32;
    let mut out = Vec::with_capacity(54 + data);
    out.extend_from_slice(b"FORM");
    out.extend_from_slice(&(4 + 26 + 8 + ssnd).to_be_bytes());
    out.extend_from_slice(b"AIFFCOMM");
    out.extend_from_slice(&18_u32.to_be_bytes());
    out.extend_from_slice(&channels.to_be_bytes());
    out.extend_from_slice(&frames.to_be_bytes());
    out.extend_from_slice(&depth.bits().to_be_bytes());
    out.extend_from_slice(&extended(f64::from(pcm.sample_rate())));
    out.extend_from_slice(b"SSND");
    out.extend_from_slice(&ssnd.to_be_bytes());
    out.extend_from_slice(&[0; 8]);
    for &sample in pcm.samples() {
        let value = depth.integer(sample).to_be_bytes();
        out.extend_from_slice(&value[4 - bytes..]);
    }
    out
}

/// A FLAC file of `pcm` with 16 or 24-bit samples (32-bit floats are
/// written as 24-bit), compressed losslessly.
///
/// # Errors
/// If the encoder refuses the stream (too many channels, say).
pub fn encode_flac(pcm: &PCMFloat1D, depth: SampleDepth) -> Result<Vec<u8>, String> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;
    let depth = if depth == SampleDepth::Float32 {
        SampleDepth::Int24
    } else {
        depth
    };
    let samples: Vec<i32> = pcm
        .samples()
        .iter()
        .map(|&sample| depth.integer(sample))
        .collect();
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|(_, error)| format!("{error:?}"))?;
    let source = flacenc::source::MemSource::from_samples(
        &samples,
        usize::from(pcm.channels()),
        usize::from(depth.bits()),
        pcm.sample_rate() as usize,
    );
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|error| format!("{error:?}"))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|error| format!("{error:?}"))?;
    let mut bytes = sink.as_slice().to_vec();
    // The stream's minimum block size leaves out the shorter last block
    // (the FLAC format says so; decoders such as symphonia rely on it), so
    // with fixed blocks it equals the maximum.
    if bytes.len() >= 12 && bytes.starts_with(b"fLaC") {
        bytes.copy_within(10..12, 8);
    }
    Ok(bytes)
}

/// Trimming cut the whole sound away.
const NO_SOUND_LEFT: &str = "trimming left no sound";

/// Everything an export or a clip's settings can change about a sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundOptions {
    /// Samples per second, or `None` to keep them.
    pub sample_rate: Option<u32>,
    /// Channels, or `None` to keep them.
    pub channels: Option<u16>,
    /// Bits per sample.
    pub depth: SampleDepth,
    /// Gain in decibels.
    pub gain_db: f32,
    /// Bring the loudest peak to -1 dB (instead of the gain).
    pub normalize: bool,
    /// Seconds cut from the start.
    pub trim_start: f32,
    /// Seconds cut from the end.
    pub trim_end: f32,
    /// Seconds of fade in.
    pub fade_in: f32,
    /// Seconds of fade out.
    pub fade_out: f32,
}

impl Default for SoundOptions {
    fn default() -> Self {
        Self {
            sample_rate: None,
            channels: None,
            depth: SampleDepth::Int16,
            gain_db: 0.0,
            normalize: false,
            trim_start: 0.0,
            trim_end: 0.0,
            fade_in: 0.0,
            fade_out: 0.0,
        }
    }
}

/// Resamples interleaved `samples` of `channels` by `ratio` (new rate over
/// old rate) with a Lanczos window.
fn resample(samples: &[f32], channels: usize, ratio: f64) -> Vec<f32> {
    let channels = channels.max(1);
    let frames = samples.len() / channels;
    if frames == 0 || (ratio - 1.0).abs() < 1e-9 {
        return samples.to_vec();
    }
    let out_frames = ((frames as f64) * ratio).round().max(1.0) as usize;
    let cutoff = ratio.min(1.0);
    let taps = 8.0 / cutoff;
    let mut out = Vec::with_capacity(out_frames * channels);
    for frame in 0..out_frames {
        let center = frame as f64 / ratio;
        let first = (center - taps).floor().max(0.0) as usize;
        let last = ((center + taps).ceil() as usize).min(frames - 1);
        let mut sums = vec![0.0_f64; channels];
        let mut total = 0.0;
        for index in first..=last {
            let x = (index as f64 - center) * cutoff;
            let weight = if x.abs() < 1e-9 {
                1.0
            } else if x.abs() < 8.0 {
                let pi = std::f64::consts::PI * x;
                8.0 * pi.sin() * (pi / 8.0).sin() / (pi * pi)
            } else {
                0.0
            };
            total += weight;
            for (channel, sum) in sums.iter_mut().enumerate() {
                *sum += f64::from(samples[index * channels + channel]) * weight;
            }
        }
        for sum in sums {
            out.push(if total.abs() > 1e-9 {
                (sum / total) as f32
            } else {
                0.0
            });
        }
    }
    out
}

/// Changes a sound as the options say: cuts, channels, sample rate,
/// loudness and fades, in that order.
///
/// # Errors
/// If the result would hold no sound.
pub fn transform_sound(pcm: &PCMFloat1D, options: &SoundOptions) -> Result<PCMFloat1D, String> {
    let rate = pcm.sample_rate().max(1);
    let channels = usize::from(pcm.channels().max(1));
    let frames = pcm.frames();
    // Trim.
    let start = ((options.trim_start.max(0.0) * rate as f32) as usize).min(frames);
    let end = frames
        .saturating_sub((options.trim_end.max(0.0) * rate as f32) as usize)
        .max(start);
    let mut samples = pcm.samples()[start * channels..end * channels].to_vec();
    // Channels.
    let out_channels = usize::from(options.channels.unwrap_or(pcm.channels()).clamp(1, 8));
    if out_channels != channels {
        samples = samples
            .chunks_exact(channels)
            .flat_map(|frame| {
                let mono = frame.iter().sum::<f32>() / channels as f32;
                (0..out_channels)
                    .map(|channel| match (channels, out_channels) {
                        (_, 1) => mono,
                        (1, _) => frame[0],
                        _ => frame.get(channel).copied().unwrap_or(mono),
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
    }
    // The sample rate.
    let out_rate = options.sample_rate.unwrap_or(rate).clamp(1_000, 384_000);
    let ratio = f64::from(out_rate) / f64::from(rate);
    samples = resample(&samples, out_channels, ratio);
    // Loudness.
    let gain = if options.normalize {
        let peak = samples
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        if peak > 1e-6 { 0.891 / peak } else { 1.0 }
    } else {
        10_f32.powf(options.gain_db / 20.0)
    };
    // Fades.
    let out_frames = samples.len() / out_channels;
    let fade_in = (options.fade_in.max(0.0) * out_rate as f32) as usize;
    let fade_out = (options.fade_out.max(0.0) * out_rate as f32) as usize;
    for (frame, chunk) in samples.chunks_exact_mut(out_channels).enumerate() {
        let mut level = gain;
        if fade_in > 0 && frame < fade_in {
            level *= frame as f32 / fade_in as f32;
        }
        if fade_out > 0 && frame + fade_out > out_frames {
            level *= (out_frames - frame) as f32 / fade_out as f32;
        }
        for sample in chunk {
            *sample = (*sample * level).clamp(-1.0, 1.0);
        }
    }
    if samples.is_empty() {
        return Err(NO_SOUND_LEFT.to_owned());
    }
    PCMFloat1D::new(out_rate, out_channels as u16, samples).map_err(|error| error.to_string())
}

/// Writes a sound as a file of `extension`: `wav`, `flac` or `aiff`.
///
/// # Errors
/// The encoder's message, or that Sue writes no such sounds.
pub fn write_sound(
    pcm: &PCMFloat1D,
    extension: &str,
    depth: SampleDepth,
) -> Result<Vec<u8>, String> {
    let extension = extension.trim_start_matches('.').to_ascii_lowercase();
    match extension.as_str() {
        "wav" | "wave" => Ok(encode_wav_as(pcm, depth)),
        "flac" => encode_flac(pcm, depth),
        "aiff" | "aif" => Ok(encode_aiff(pcm, depth)),
        other => Err(format!("Sue cannot write .{other} sounds")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, frames: usize) -> PCMFloat1D {
        let samples = (0..frames)
            .flat_map(|frame| {
                let t = frame as f32 / rate as f32;
                let left = (t * 440.0 * std::f32::consts::TAU).sin() * 0.5;
                [left, -left]
            })
            .collect();
        PCMFloat1D::new(rate, 2, samples).unwrap()
    }

    #[test]
    fn every_depth_and_container_decodes_back_to_the_same_sound() {
        let source = tone(44_100, 2_205);
        let files = [
            ("wav", encode_wav_as(&source, SampleDepth::Int24), 1e-5),
            ("wav", encode_wav_as(&source, SampleDepth::Float32), 1e-7),
            ("aiff", encode_aiff(&source, SampleDepth::Int16), 1e-4),
            ("aiff", encode_aiff(&source, SampleDepth::Int24), 1e-5),
            (
                "flac",
                encode_flac(&source, SampleDepth::Int16).unwrap(),
                1e-4,
            ),
            (
                "flac",
                encode_flac(&source, SampleDepth::Int24).unwrap(),
                1e-5,
            ),
        ];
        for (extension, bytes, tolerance) in files {
            let (decoded, _) = decode_audio(bytes, Some(extension))
                .unwrap_or_else(|(error, _)| panic!("{extension}: {error}"));
            assert_eq!(decoded.info.sample_rate, 44_100, "{extension}");
            assert_eq!(decoded.info.channels, 2, "{extension}");
            assert_eq!(decoded.pcm.frames(), 2_205, "{extension}");
            let worst = decoded
                .pcm
                .samples()
                .iter()
                .zip(source.samples())
                .map(|(a, b)| (a - b).abs())
                .fold(0.0_f32, f32::max);
            assert!(worst <= tolerance, "{extension} drifted by {worst}");
        }
        assert_eq!(extended(44_100.0)[..2], [0x40, 0x0E]);
    }

    #[test]
    fn probing_reads_the_length_without_decoding() {
        let wav = encode_wav(&tone(48_000, 24_000));
        let path = std::env::temp_dir().join(format!("sue-probe-{}.wav", std::process::id()));
        std::fs::write(&path, &wav).unwrap();
        let info = probe_audio(&path).unwrap();
        assert_eq!(
            (info.sample_rate, info.channels, info.frames),
            (48_000, 2, 24_000)
        );
        assert_eq!(info.codec, "PCM");
        let _ = std::fs::remove_file(&path);
        assert!(probe_audio(&path).is_err(), "a missing file is an error");
    }

    #[test]
    fn wav_files_round_trip_through_the_decoder() {
        let source = tone(44_100, 4_410);
        let wav = encode_wav(&source);
        assert!(wav.starts_with(b"RIFF") && wav[8..12] == *b"WAVE");
        let (decoded, bytes) = decode_audio(wav.clone(), Some("wav")).unwrap();
        assert_eq!(bytes, wav, "the encoded bytes come back untouched");
        assert_eq!(decoded.info.sample_rate, 44_100);
        assert_eq!(decoded.info.channels, 2);
        assert_eq!(decoded.info.frames, 4_410);
        assert_eq!(decoded.info.codec, "PCM");
        let error = decoded
            .pcm
            .samples()
            .iter()
            .zip(source.samples())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(error < 1e-4, "16-bit rounding error {error}");
    }

    #[test]
    fn garbage_is_an_error_and_keeps_its_bytes() {
        let junk = b"RIFF\0\0\0\0WAVEjunkjunk".to_vec();
        let (message, bytes) = decode_audio(junk.clone(), None).unwrap_err();
        assert!(!message.is_empty());
        assert_eq!(bytes, junk);
    }

    #[test]
    fn sounds_trim_fade_resample_and_change_channels() {
        let rate = 8_000;
        let samples: Vec<f32> = (0..rate * 2).flat_map(|_| [0.5_f32, -0.5]).collect();
        let pcm = PCMFloat1D::new(rate, 2, samples).unwrap();
        let options = SoundOptions {
            sample_rate: Some(16_000),
            channels: Some(1),
            trim_start: 0.5,
            fade_out: 0.25,
            gain_db: -6.0,
            ..SoundOptions::default()
        };
        let out = transform_sound(&pcm, &options).unwrap();
        assert_eq!((out.sample_rate(), out.channels()), (16_000, 1));
        assert!((out.duration_seconds() - 1.5).abs() < 0.01);
        // Left and right cancel in mono.
        assert!(out.samples()[100].abs() < 1e-3);
        let loud = transform_sound(
            &PCMFloat1D::new(rate, 1, vec![0.25; rate as usize]).unwrap(),
            &SoundOptions {
                normalize: true,
                fade_out: 0.1,
                ..SoundOptions::default()
            },
        )
        .unwrap();
        assert!(
            (loud.samples()[10] - 0.891).abs() < 1e-3,
            "{}",
            loud.samples()[10]
        );
        assert!(loud.samples().last().unwrap().abs() < 0.02, "faded out");
        for extension in ["wav", "flac", "aiff"] {
            let bytes = write_sound(&out, extension, SampleDepth::Int16).unwrap();
            let (back, _) = decode_audio(bytes, Some(extension)).unwrap();
            assert_eq!(back.pcm.frames(), out.frames(), "{extension}");
        }
        assert!(write_sound(&out, "mp3", SampleDepth::Int16).is_err());
        // Nothing asked for changes nothing; everything cut away is an
        // error the interface can translate.
        let same = transform_sound(&pcm, &SoundOptions::default()).unwrap();
        assert_eq!(same, pcm);
        let gone = transform_sound(
            &pcm,
            &SoundOptions {
                trim_start: 3.0,
                ..SoundOptions::default()
            },
        );
        assert_eq!(gone.unwrap_err(), NO_SOUND_LEFT);
        assert!(sue_lang::has_english("io", NO_SOUND_LEFT));
    }
}
