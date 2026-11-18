//! Reading movies and sound through Media Foundation's Source Reader,
//! with Windows' own decoders: H.264 and, where the system has its
//! extension, HEVC in MP4 and QuickTime files; AAC, MP3, WAV and FLAC
//! sound. Sue ships no decoder of its own for them.
//!
//! [`MediaReader`] opens a file for its pictures or its sound (one reader
//! each: a reader that read only one of two selected streams would keep
//! the other's samples piling up). Pictures come out as 8-bit RGBA at the
//! size asked for: the Source Reader's video processor turns the decoder's
//! YUV into RGB and scales on the way, on the graphics card when a
//! Direct3D 11 device is handed to it ([`ReaderOptions::hardware`]), so
//! the decoding itself runs on the card's video engine (DXVA) where it has
//! one. Without a card that takes it, the same reader runs in software.
//! Sound comes out as 32-bit float at the rate asked for, interleaved.
//!
//! Seeking ([`MediaReader::seek`]) lands on the key frame at or before the
//! time asked for, which is fast; reading on to the exact frame is the
//! caller's choice ([`MediaReader::frame_at`]).
//!
//! A reader starts COM and Media Foundation on the thread that makes it
//! and shuts them down when dropped, so it stays on that thread.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION,
    D3D11CreateDevice, ID3D11Device, ID3D11Multithread,
};
use windows::Win32::Media::MediaFoundation::{
    IMF2DBuffer, IMFDXGIDeviceManager, IMFMediaBuffer, IMFMediaType, IMFSample, IMFSourceReader,
    MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS,
    MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE,
    MF_MT_SUBTYPE, MF_PD_DURATION, MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS,
    MF_SOURCE_READER_ALL_STREAMS, MF_SOURCE_READER_D3D_MANAGER,
    MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, MF_SOURCE_READER_FIRST_AUDIO_STREAM,
    MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_SOURCE_READER_MEDIASOURCE,
    MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED, MF_SOURCE_READERF_ENDOFSTREAM, MFAudioFormat_Float,
    MFCreateDXGIDeviceManager, MFCreateSourceReaderFromURL, MFMediaType_Audio, MFMediaType_Video,
    MFVideoFormat_RGB32,
};
use windows::Win32::System::Com::StructuredStorage::{
    PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0, PropVariantClear,
    PropVariantToInt64,
};
use windows::Win32::System::Variant::VT_I8;
use windows::core::{GUID, Interface, PCWSTR};

use crate::media::{MediaError, Session, Value, attributes, media_type, os};

/// What a file holds, as Media Foundation reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MediaInfo {
    /// Its pictures' display width and height (0 without pictures).
    pub width: u32,
    /// See `width`.
    pub height: u32,
    /// Frames a second as numerator and denominator ((0, 1) without
    /// pictures).
    pub rate: (u32, u32),
    /// How long it lasts, in 100 nanosecond units.
    pub duration: i64,
    /// The pictures' codec as its four letters (`H264`, `HEVC`), empty
    /// without pictures.
    pub video_codec: String,
    /// The sound's rate and channels as stored, if it has sound.
    pub audio: Option<(u32, u16)>,
}

/// How a [`MediaReader`] reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReaderOptions {
    /// Read the pictures (else the sound).
    pub video: bool,
    /// The size pictures come out at; `None` for the file's own.
    pub size: Option<(u32, u32)>,
    /// Decode and convert on the graphics card when it can.
    pub hardware: bool,
    /// The rate sound comes out at.
    pub sound_rate: u32,
    /// The channels sound comes out in (1 or 2).
    pub sound_channels: u16,
}

impl ReaderOptions {
    /// Pictures at `size` (or the file's own), on the card when it can.
    #[must_use]
    pub fn video(size: Option<(u32, u32)>, hardware: bool) -> Self {
        Self {
            video: true,
            size,
            hardware,
            sound_rate: 48_000,
            sound_channels: 2,
        }
    }

    /// Sound at `rate` in `channels`.
    #[must_use]
    pub fn sound(rate: u32, channels: u16) -> Self {
        Self {
            video: false,
            size: None,
            hardware: false,
            sound_rate: rate,
            sound_channels: channels.clamp(1, 2),
        }
    }
}

/// One picture read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameTime {
    /// When it shows, in 100 nanosecond units from the file's start.
    pub time: i64,
    /// How long it shows (0 when the file does not say).
    pub duration: i64,
}

/// A Direct3D 11 device and the manager that hands it to Media
/// Foundation, for decoding on the graphics card.
struct Card {
    _device: ID3D11Device,
    manager: IMFDXGIDeviceManager,
}

/// Makes a device on the default graphics card with video support, or
/// `None` where there is none (a virtual machine, a server).
#[allow(
    unsafe_code,
    reason = "Direct3D 11 and its device manager are COM, with no safe call (standing approval, rule 13)"
)]
fn card() -> Option<Card> {
    let mut device = None;
    // SAFETY: every pointer is None or a live local; the default adapter
    // (None) and no software module are asked for.
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            windows::Win32::Foundation::HMODULE::default(),
            D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
    }
    .ok()?;
    let device = device?;
    // The decoder and the video processor use the device from Media
    // Foundation's own threads: it must lock itself.
    if let Ok(threads) = device.cast::<ID3D11Multithread>() {
        // SAFETY: `threads` is a live interface of the device just made.
        let _ = unsafe { threads.SetMultithreadProtected(true) };
    }
    let mut token = 0_u32;
    let mut manager = None;
    // SAFETY: `token` and `manager` are live locals for the out values.
    unsafe { MFCreateDXGIDeviceManager(&mut token, &mut manager) }.ok()?;
    let manager = manager?;
    // SAFETY: `device` is live and `token` is the manager's own.
    unsafe { manager.ResetDevice(&device, token) }.ok()?;
    Some(Card {
        _device: device,
        manager,
    })
}

/// A time in 100 nanosecond units as the PROPVARIANT seeking takes.
fn time_variant(time: i64) -> PROPVARIANT {
    PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_I8,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 { hVal: time },
            }),
        },
    }
}

/// The four letters of a codec's subtype (its first 32 bits are its
/// FOURCC).
fn fourcc(guid: GUID) -> String {
    guid.data1
        .to_le_bytes()
        .iter()
        .map(|byte| {
            if byte.is_ascii_graphic() {
                char::from(*byte)
            } else {
                '?'
            }
        })
        .collect()
}

fn high_low(value: u64) -> (u32, u32) {
    ((value >> 32) as u32, value as u32)
}

/// A file read by the Source Reader: its pictures or its sound.
pub struct MediaReader {
    reader: IMFSourceReader,
    stream: u32,
    info: MediaInfo,
    options: ReaderOptions,
    /// The size pictures come out at (after the processor).
    size: (u32, u32),
    /// The sound's rate and channels as they come out.
    sound: (u32, u16),
    /// The card the reader decodes on, if it does.
    card: Option<Card>,
    /// Declared last, so the objects above are released before Media
    /// Foundation and COM shut down.
    _session: Session,
}

impl std::fmt::Debug for MediaReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaReader")
            .field("info", &self.info)
            .field("options", &self.options)
            .field("size", &self.size)
            .field("hardware", &self.card.is_some())
            .finish_non_exhaustive()
    }
}

/// The file at `path` as a NUL terminated wide string.
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

impl MediaReader {
    /// Opens `path` for its pictures or its sound.
    ///
    /// # Errors
    /// When the file cannot be opened, has no stream of the kind asked
    /// for, or Windows has no decoder for it ([`MediaError::NoEncoder`]
    /// stands for a missing codec here too).
    #[allow(
        unsafe_code,
        reason = "the Source Reader is COM, with no safe call (standing approval, rule 13)"
    )]
    pub fn open(path: &Path, options: ReaderOptions) -> Result<Self, MediaError> {
        let session = Session::start()?;
        let card = if options.video && options.hardware {
            card()
        } else {
            None
        };
        let settings = attributes(&[
            (
                MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING,
                u32::from(options.video),
            ),
            (
                MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS,
                u32::from(card.is_some()),
            ),
        ])?;
        if let Some(card) = &card {
            // SAFETY: the key lives for the call; the store keeps its own
            // reference to the manager.
            unsafe { settings.SetUnknown(&MF_SOURCE_READER_D3D_MANAGER, &card.manager) }
                .map_err(os("handing the graphics card over"))?;
        }
        let url = wide(path);
        // SAFETY: `url` is a NUL terminated wide path that lives for the
        // call; `settings` is live.
        let reader: IMFSourceReader =
            unsafe { MFCreateSourceReaderFromURL(PCWSTR(url.as_ptr()), &settings) }
                .map_err(os("opening the file"))?;
        let video = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let audio = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
        let mut info = MediaInfo {
            rate: (0, 1),
            ..MediaInfo::default()
        };
        // SAFETY: plain numbers; a reader takes deselecting every stream.
        unsafe { reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false) }
            .map_err(os("choosing the streams"))?;
        // SAFETY: reads the first native type of each stream, new objects;
        // a file without the stream fails, which is an answer.
        if let Ok(native) = unsafe { reader.GetNativeMediaType(video, 0) } {
            // SAFETY: the keys live for the calls.
            unsafe {
                let size = native.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0);
                (info.width, info.height) = high_low(size);
                let rate = native.GetUINT64(&MF_MT_FRAME_RATE).unwrap_or(0);
                let rate = high_low(rate);
                info.rate = if rate.0 > 0 && rate.1 > 0 {
                    rate
                } else {
                    (0, 1)
                };
                if let Ok(subtype) = native.GetGUID(&MF_MT_SUBTYPE) {
                    info.video_codec = fourcc(subtype);
                }
            }
        }
        // SAFETY: as above.
        if let Ok(native) = unsafe { reader.GetNativeMediaType(audio, 0) } {
            // SAFETY: the keys live for the calls.
            let (rate, channels) = unsafe {
                (
                    native
                        .GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)
                        .unwrap_or(0),
                    native.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).unwrap_or(0),
                )
            };
            if rate > 0 && channels > 0 {
                info.audio = Some((rate, channels.min(u32::from(u16::MAX)) as u16));
            }
        }
        // SAFETY: the key lives for the call; the result is a new
        // PROPVARIANT that is read and then cleared.
        if let Ok(mut duration) = unsafe {
            reader.GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)
        } {
            // SAFETY: `duration` is a live PROPVARIANT (VT_UI8 for a
            // duration); converting reads it, clearing frees what it holds.
            unsafe {
                info.duration = PropVariantToInt64(&duration).unwrap_or(0);
                let _ = PropVariantClear(&mut duration);
            }
        }
        let mut out = Self {
            reader,
            stream: if options.video { video } else { audio },
            options,
            size: (0, 0),
            sound: (0, 0),
            card,
            info,
            _session: session,
        };
        if options.video {
            if out.info.width == 0 {
                return Err(MediaError::BadInput("the file has no pictures"));
            }
            out.choose_video_type()?;
        } else {
            if out.info.audio.is_none() {
                return Err(MediaError::BadInput("the file has no sound"));
            }
            out.choose_sound_type()?;
        }
        Ok(out)
    }

    #[allow(
        unsafe_code,
        reason = "the Source Reader is COM, with no safe call (standing approval, rule 13)"
    )]
    fn choose_video_type(&mut self) -> Result<(), MediaError> {
        let (width, height) = self
            .options
            .size
            .map_or((self.info.width, self.info.height), |(w, h)| {
                (w.max(2) & !1, h.max(2) & !1)
            });
        let kind = media_type(&[
            (MF_MT_MAJOR_TYPE, Value::Guid(MFMediaType_Video)),
            (MF_MT_SUBTYPE, Value::Guid(MFVideoFormat_RGB32)),
            (
                MF_MT_FRAME_SIZE,
                Value::U64((u64::from(width) << 32) | u64::from(height)),
            ),
        ])?;
        // SAFETY: plain numbers.
        unsafe { self.reader.SetStreamSelection(self.stream, true) }
            .map_err(os("choosing the pictures"))?;
        // SAFETY: `kind` is live; the reserved pointer is None.
        unsafe { self.reader.SetCurrentMediaType(self.stream, None, &kind) }
            .map_err(os("asking for RGB pictures"))?;
        self.read_size()
    }

    #[allow(
        unsafe_code,
        reason = "the Source Reader is COM, with no safe call (standing approval, rule 13)"
    )]
    fn read_size(&mut self) -> Result<(), MediaError> {
        // SAFETY: reads the reader's current type, a new object.
        let kind: IMFMediaType = unsafe { self.reader.GetCurrentMediaType(self.stream) }
            .map_err(os("reading the picture type"))?;
        // SAFETY: the key lives for the call.
        let size = unsafe { kind.GetUINT64(&MF_MT_FRAME_SIZE) }.map_err(os("reading the size"))?;
        self.size = high_low(size);
        Ok(())
    }

    #[allow(
        unsafe_code,
        reason = "the Source Reader is COM, with no safe call (standing approval, rule 13)"
    )]
    fn choose_sound_type(&mut self) -> Result<(), MediaError> {
        let (rate, channels) = (self.options.sound_rate, self.options.sound_channels);
        let channels32 = u32::from(channels);
        let kind = media_type(&[
            (MF_MT_MAJOR_TYPE, Value::Guid(MFMediaType_Audio)),
            (MF_MT_SUBTYPE, Value::Guid(MFAudioFormat_Float)),
            (MF_MT_AUDIO_BITS_PER_SAMPLE, Value::U32(32)),
            (MF_MT_AUDIO_SAMPLES_PER_SECOND, Value::U32(rate)),
            (MF_MT_AUDIO_NUM_CHANNELS, Value::U32(channels32)),
            (MF_MT_AUDIO_BLOCK_ALIGNMENT, Value::U32(channels32 * 4)),
        ])?;
        // SAFETY: plain numbers.
        unsafe { self.reader.SetStreamSelection(self.stream, true) }
            .map_err(os("choosing the sound"))?;
        // SAFETY: `kind` is live; the reserved pointer is None. The Source
        // Reader resamples and mixes the channels down to what is asked.
        unsafe { self.reader.SetCurrentMediaType(self.stream, None, &kind) }
            .map_err(os("asking for float sound"))?;
        // SAFETY: reads the reader's current type, a new object.
        let current: IMFMediaType = unsafe { self.reader.GetCurrentMediaType(self.stream) }
            .map_err(os("reading the sound type"))?;
        // SAFETY: the keys live for the calls.
        self.sound = unsafe {
            (
                current
                    .GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)
                    .unwrap_or(rate),
                current
                    .GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)
                    .unwrap_or(channels32) as u16,
            )
        };
        Ok(())
    }

    /// What the file holds.
    #[must_use]
    pub fn info(&self) -> &MediaInfo {
        &self.info
    }

    /// The size pictures come out at.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// The rate and channels sound comes out at.
    #[must_use]
    pub fn sound_format(&self) -> (u32, u16) {
        self.sound
    }

    /// `true` when it decodes on the graphics card.
    #[must_use]
    pub fn hardware(&self) -> bool {
        self.card.is_some()
    }

    /// Goes to the key frame at or before `time` (100 nanosecond units):
    /// the next picture or sound read starts there.
    ///
    /// # Errors
    /// When the file cannot seek.
    #[allow(
        unsafe_code,
        reason = "the Source Reader is COM, with no safe call (standing approval, rule 13)"
    )]
    pub fn seek(&mut self, time: i64) -> Result<(), MediaError> {
        let position = time_variant(time.max(0));
        // SAFETY: GUID_NULL (100 nanosecond units) and a VT_I8 PROPVARIANT
        // that lives for the call and owns nothing to free.
        unsafe { self.reader.SetCurrentPosition(&GUID::zeroed(), &position) }.map_err(os("seeking"))
    }

    /// Reads the next sample of the stream. `None` at the end.
    #[allow(
        unsafe_code,
        reason = "the Source Reader is COM, with no safe call (standing approval, rule 13)"
    )]
    fn next_sample(&mut self) -> Result<Option<(IMFSample, i64)>, MediaError> {
        loop {
            let mut flags = 0_u32;
            let mut time = 0_i64;
            let mut sample = None;
            // SAFETY: the out values point at live locals.
            unsafe {
                self.reader.ReadSample(
                    self.stream,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut time),
                    Some(&mut sample),
                )
            }
            .map_err(os("decoding"))?;
            if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 && self.options.video
            {
                self.read_size()?;
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                return Ok(None);
            }
            if let Some(sample) = sample {
                return Ok(Some((sample, time)));
            }
        }
    }

    /// Reads the next picture into `rgba` (width times height times 4
    /// bytes, opaque) and says when it shows. `None` at the end.
    ///
    /// # Errors
    /// When decoding fails.
    #[allow(
        unsafe_code,
        reason = "Media Foundation buffers are COM (standing approval, rule 13)"
    )]
    pub fn next_frame(&mut self, rgba: &mut Vec<u8>) -> Result<Option<FrameTime>, MediaError> {
        if !self.options.video {
            return Err(MediaError::BadInput("a sound reader has no pictures"));
        }
        let Some((sample, time)) = self.next_sample()? else {
            return Ok(None);
        };
        // SAFETY: `sample` is live.
        let duration = unsafe { sample.GetSampleDuration() }.unwrap_or(0);
        // SAFETY: `sample` is live; the result is a new reference.
        let buffer: IMFMediaBuffer =
            unsafe { sample.ConvertToContiguousBuffer() }.map_err(os("joining a picture"))?;
        let (width, height) = (self.size.0 as usize, self.size.1 as usize);
        rgba.resize(width * height * 4, 0);
        if let Ok(flat) = buffer.cast::<IMF2DBuffer>() {
            let mut start: *mut u8 = std::ptr::null_mut();
            let mut pitch = 0_i32;
            // SAFETY: the out values point at live locals.
            unsafe { flat.Lock2D(&mut start, &mut pitch) }.map_err(os("locking a picture"))?;
            let row_bytes = width * 4;
            if start.is_null() || (pitch.unsigned_abs() as usize) < row_bytes {
                // SAFETY: pairs the Lock2D above.
                let _ = unsafe { flat.Unlock2D() };
                return Err(MediaError::BadInput("a picture smaller than its size"));
            }
            for y in 0..height {
                // SAFETY: Lock2D gave the first row of the picture at
                // `start` and the step to the next as `pitch` (negative for
                // a picture stored bottom up); each of the `height` rows
                // holds at least `width * 4` readable bytes until Unlock2D.
                let row = unsafe {
                    std::slice::from_raw_parts(start.offset(pitch as isize * y as isize), row_bytes)
                };
                bgrx_to_rgba(row, &mut rgba[y * row_bytes..(y + 1) * row_bytes]);
            }
            // SAFETY: pairs the Lock2D above.
            let _ = unsafe { flat.Unlock2D() };
        } else {
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut filled = 0_u32;
            // SAFETY: the out values point at live locals.
            unsafe { buffer.Lock(&mut data, None, Some(&mut filled)) }
                .map_err(os("locking a picture"))?;
            if data.is_null() || (filled as usize) < width * height * 4 {
                // SAFETY: pairs the Lock above.
                let _ = unsafe { buffer.Unlock() };
                return Err(MediaError::BadInput("a picture smaller than its size"));
            }
            // SAFETY: Lock gave `data`, readable for `filled` bytes until
            // the Unlock below, which is at least the picture.
            let all = unsafe { std::slice::from_raw_parts(data, width * height * 4) };
            bgrx_to_rgba(all, rgba);
            // SAFETY: pairs the Lock above.
            let _ = unsafe { buffer.Unlock() };
        }
        Ok(Some(FrameTime { time, duration }))
    }

    /// Reads pictures until the one showing at `time`, leaving it in
    /// `rgba`: the exact frame, after a [`Self::seek`] to before it. The
    /// last picture read when the file ends first; `None` if none was.
    ///
    /// # Errors
    /// When decoding fails.
    pub fn frame_at(
        &mut self,
        time: i64,
        rgba: &mut Vec<u8>,
    ) -> Result<Option<FrameTime>, MediaError> {
        let mut last = None;
        // Half a frame of slack, for timestamps rounded to 100 ns.
        let slack = self.frame_ticks() / 2;
        while let Some(frame) = self.next_frame(rgba)? {
            let length = if frame.duration > 0 {
                frame.duration
            } else {
                self.frame_ticks()
            };
            last = Some(frame);
            if frame.time + length > time + slack.min(length / 2) || frame.time >= time {
                break;
            }
        }
        Ok(last)
    }

    /// One frame's length in 100 nanosecond units (a 30th of a second
    /// when the file does not say).
    #[must_use]
    pub fn frame_ticks(&self) -> i64 {
        let (numerator, denominator) = self.info.rate;
        if numerator == 0 {
            return 333_333;
        }
        (10_000_000 * i64::from(denominator)) / i64::from(numerator)
    }

    /// Reads the next piece of sound, appending it to `samples`
    /// (interleaved float), and says when it starts. `None` at the end.
    ///
    /// # Errors
    /// When decoding fails.
    #[allow(
        unsafe_code,
        reason = "Media Foundation buffers are COM (standing approval, rule 13)"
    )]
    pub fn next_sound(&mut self, samples: &mut Vec<f32>) -> Result<Option<i64>, MediaError> {
        if self.options.video {
            return Err(MediaError::BadInput("a picture reader has no sound"));
        }
        let Some((sample, time)) = self.next_sample()? else {
            return Ok(None);
        };
        // SAFETY: `sample` is live; the result is a new reference.
        let buffer: IMFMediaBuffer =
            unsafe { sample.ConvertToContiguousBuffer() }.map_err(os("joining sound"))?;
        let mut data: *mut u8 = std::ptr::null_mut();
        let mut filled = 0_u32;
        // SAFETY: the out values point at live locals.
        unsafe { buffer.Lock(&mut data, None, Some(&mut filled)) }.map_err(os("locking sound"))?;
        if !data.is_null() {
            // SAFETY: Lock gave `data`, readable for `filled` bytes until
            // the Unlock below; it is read before then.
            let all = unsafe { std::slice::from_raw_parts(data, filled as usize) };
            samples.extend(
                all.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|bytes| f32::from_le_bytes(*bytes)),
            );
        }
        // SAFETY: pairs the Lock above.
        let _ = unsafe { buffer.Unlock() };
        Ok(Some(time))
    }
}

/// Media Foundation's RGB32 (blue, green, red, unused) as opaque RGBA.
fn bgrx_to_rgba(from: &[u8], to: &mut [u8]) {
    for (source, target) in from
        .as_chunks::<4>()
        .0
        .iter()
        .zip(to.as_chunks_mut::<4>().0)
    {
        *target = [source[2], source[1], source[0], 255];
    }
}

/// What `path` holds, without decoding it.
///
/// # Errors
/// When the file cannot be opened.
pub fn probe(path: &Path) -> Result<MediaInfo, MediaError> {
    let video = MediaReader::open(path, ReaderOptions::video(Some((64, 36)), false));
    match video {
        Ok(reader) => Ok(reader.info.clone()),
        Err(_) => MediaReader::open(path, ReaderOptions::sound(48_000, 2)).map(|r| r.info.clone()),
    }
}

/// Whether this system can make a Direct3D 11 device with video support
/// for Media Foundation.
#[must_use]
pub fn hardware_present() -> bool {
    Session::start().is_ok_and(|_session| card().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{AudioSpec, Mp4Writer, VideoSpec};

    /// A movie of `frames` frames at `fps`, `width` by `height`, whose
    /// frame `n` has luma `n * 8` in its left half (so each frame is told
    /// apart) and a second of a 440 Hz tone, written to a file.
    fn movie(name: &str, width: u32, height: u32, fps: u32, frames: u32) -> std::path::PathBuf {
        let mut writer = Mp4Writer::new(
            VideoSpec {
                width,
                height,
                rate: (fps, 1),
                quality: 90,
                key_interval: 12,
            },
            Some(AudioSpec {
                rate: 48_000,
                channels: 2,
            }),
        )
        .unwrap();
        let tick = 10_000_000 / i64::from(fps);
        let tone: Vec<i16> = (0..48_000)
            .flat_map(|at| {
                let v = (f64::from(at) * 440.0 * std::f64::consts::TAU / 48_000.0).sin();
                let v = (v * 12_000.0) as i16;
                [v, v]
            })
            .collect();
        for (piece, chunk) in tone.chunks(9600).enumerate() {
            writer
                .write_audio(chunk, piece as i64 * 1_000_000, 1_000_000)
                .unwrap();
        }
        for frame in 0..frames {
            let (w, h) = (width as usize, height as usize);
            let mut nv12 = vec![128_u8; w * h * 3 / 2];
            let luma = (16 + frame * 8).min(235) as u8;
            for y in 0..h {
                for x in 0..w / 2 {
                    nv12[y * w + x] = luma;
                }
            }
            writer
                .write_video(&nv12, i64::from(frame) * tick, tick)
                .unwrap();
        }
        let bytes = writer.finish().unwrap();
        let dir = std::env::temp_dir().join("sue-decode-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}-{}.mp4", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn left_luma(rgba: &[u8], width: u32, height: u32) -> u32 {
        let (w, h) = (width as usize, height as usize);
        let (x, y) = (w / 4, h / 2);
        u32::from(rgba[(y * w + x) * 4 + 1])
    }

    #[test]
    fn a_movie_reads_its_facts_frames_and_sound() {
        let path = movie("facts", 160, 96, 24, 24);
        let info = probe(&path).unwrap();
        assert_eq!((info.width, info.height), (160, 96));
        assert_eq!(info.rate, (24, 1));
        assert_eq!(info.video_codec, "H264");
        assert_eq!(info.audio, Some((48_000, 2)));
        assert!(
            (info.duration - 10_000_000).abs() < 500_000,
            "{}",
            info.duration
        );
        for hardware in [false, true] {
            let mut reader =
                MediaReader::open(&path, ReaderOptions::video(None, hardware)).unwrap();
            assert_eq!(reader.size(), (160, 96));
            let mut rgba = Vec::new();
            let mut count = 0;
            let mut last = -1;
            while let Some(frame) = reader.next_frame(&mut rgba).unwrap() {
                assert!(frame.time > last);
                last = frame.time;
                assert_eq!(rgba.len(), 160 * 96 * 4);
                assert_eq!(rgba[3], 255, "opaque");
                count += 1;
            }
            assert_eq!(count, 24, "hardware {hardware}");
        }
        let mut sound = MediaReader::open(&path, ReaderOptions::sound(48_000, 2)).unwrap();
        assert_eq!(sound.sound_format(), (48_000, 2));
        let mut samples = Vec::new();
        while sound.next_sound(&mut samples).unwrap().is_some() {}
        let frames = samples.len() / 2;
        assert!((46_000..=51_000).contains(&frames), "{frames}");
        let peak = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.3, "{peak}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn seeking_lands_on_the_exact_frame() {
        let path = movie("seek", 128, 72, 24, 26);
        let mut reader = MediaReader::open(&path, ReaderOptions::video(None, false)).unwrap();
        let tick = reader.frame_ticks();
        assert_eq!(tick, 416_666);
        let mut rgba = Vec::new();
        for target in [17_i64, 3, 25, 12, 0, 13] {
            let time = target * 10_000_000 / 24;
            reader.seek(time).unwrap();
            let frame = reader.frame_at(time, &mut rgba).unwrap().unwrap();
            assert!((frame.time - time).abs() < tick / 2, "{target}: {frame:?}");
            let want = 16 + target as u32 * 8;
            let got = left_luma(&rgba, 128, 72);
            // RGB of a gray: the studio range luma comes back full range.
            let full = (f64::from(want) - 16.0) * 255.0 / 219.0;
            assert!(
                (f64::from(got) - full).abs() < 6.0,
                "frame {target}: {got} for {full:.0}"
            );
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn pictures_come_out_at_the_size_asked_for() {
        let path = movie("size", 320, 180, 30, 3);
        let mut reader =
            MediaReader::open(&path, ReaderOptions::video(Some((80, 45)), false)).unwrap();
        assert_eq!(reader.size(), (80, 44), "even, as the processor wants");
        let mut rgba = Vec::new();
        assert!(reader.next_frame(&mut rgba).unwrap().is_some());
        assert_eq!(rgba.len(), 80 * 44 * 4);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_file_that_is_no_movie_is_an_error() {
        let dir = std::env::temp_dir().join("sue-decode-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("junk-{}.mp4", std::process::id()));
        std::fs::write(&path, b"not a movie at all").unwrap();
        assert!(probe(&path).is_err());
        assert!(MediaReader::open(&path, ReaderOptions::video(None, true)).is_err());
        assert!(probe(&dir.join("missing.mp4")).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
