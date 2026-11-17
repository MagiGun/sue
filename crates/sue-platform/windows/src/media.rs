//! MP4 movies through Media Foundation, Windows' own media framework.
//!
//! [`Mp4Writer`] hands NV12 pictures and 16-bit PCM sound to the Sink
//! Writer, which runs Windows' software H.264 encoder (High profile,
//! quality based rate control) and its AAC-LC encoder, and writes the MP4
//! through a temporary file whose bytes it hands back. Sue ships no H.264
//! and no AAC code: the codecs are the operating system's.
//!
//! The MP4 comes out with its index (`moov`) after the picture data.
//! Media Foundation's own "index first" setting
//! (`MF_MPEG4SINK_MOOV_BEFORE_MDAT`) was tried on Windows 11 build 26300
//! and wrote a file whose chunk offsets pointed into zeros (ffprobe:
//! "Invalid NAL unit size"), so the index is moved to the front in safe
//! Rust afterwards (`sue_io::mp4_index_first`).
//! [`read_mp4`] reads a movie back through the Source Reader (Windows'
//! own decoders), so tests can count and compare the frames.
//!
//! Where Windows has no H.264 encoder (Windows N without the Media
//! Feature Pack) [`h264_encoder_present`] says so and [`Mp4Writer::new`]
//! fails with [`MediaError::NoEncoder`], never a panic.
//!
//! A writer and a reader start COM and Media Foundation on the thread
//! that makes them and shut them down when dropped, so they stay on that
//! thread (they are not `Send`).

use std::cell::Cell;
use std::marker::PhantomData;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::Media::MediaFoundation::{
    CODECAPI_AVEncCommonQuality, CODECAPI_AVEncCommonRateControlMode,
    CODECAPI_AVEncMPVDefaultBPictureCount, CODECAPI_AVEncMPVGOPSize, IMFActivate, IMFAttributes,
    IMFMediaType, IMFSinkWriter, IMFSourceReader, MF_ACCESSMODE_READWRITE,
    MF_E_TOPO_CODEC_NOT_FOUND, MF_FILEFLAGS_NONE, MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION,
    MF_MT_AAC_PAYLOAD_TYPE, MF_MT_ALL_SAMPLES_INDEPENDENT, MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
    MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS,
    MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_AVG_BITRATE, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE,
    MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_MINIMUM_DISPLAY_APERTURE,
    MF_MT_MPEG2_PROFILE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_MT_TRANSFER_FUNCTION,
    MF_MT_VIDEO_NOMINAL_RANGE, MF_MT_VIDEO_PRIMARIES, MF_MT_YUV_MATRIX,
    MF_OPENMODE_DELETE_IF_EXIST, MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS,
    MF_SINK_WRITER_DISABLE_THROTTLING, MF_SOURCE_READER_FIRST_AUDIO_STREAM,
    MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_SOURCE_READERF_ENDOFSTREAM, MF_TRANSCODE_CONTAINERTYPE,
    MF_VERSION, MFAudioFormat_AAC, MFAudioFormat_PCM, MFCreateAttributes, MFCreateFile,
    MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFCreateSinkWriterFromURL,
    MFCreateSourceReaderFromURL, MFMediaType_Audio, MFMediaType_Video, MFNominalRange_16_235,
    MFSTARTUP_LITE, MFShutdown, MFStartup, MFT_CATEGORY_AUDIO_ENCODER, MFT_CATEGORY_VIDEO_ENCODER,
    MFT_ENUM_FLAG_LOCALMFT, MFT_ENUM_FLAG_SORTANDFILTER, MFT_ENUM_FLAG_SYNCMFT,
    MFT_REGISTER_TYPE_INFO, MFTEnumEx, MFTranscodeContainerType_MPEG4, MFVideoFormat_H264,
    MFVideoFormat_NV12, MFVideoInterlace_Progressive, MFVideoPrimaries_BT709, MFVideoTransFunc_709,
    MFVideoTransferMatrix_BT709, eAVEncCommonRateControlMode_Quality, eAVEncH264VProfile_High,
};
use windows::Win32::System::Com::{
    COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::core::{GUID, PCWSTR};

/// Why Media Foundation could not do it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaError {
    /// Windows has no encoder for the format here (Windows N without the
    /// Media Feature Pack, or the encoder refused to load).
    NoEncoder,
    /// The input does not fit what the writer was made for.
    BadInput(&'static str),
    /// A Media Foundation or COM call failed: which step, the HRESULT and
    /// the system's own message.
    Os {
        /// What Sue was doing.
        step: &'static str,
        /// The HRESULT, as Windows reports it.
        code: i32,
        /// The system's text for it.
        message: String,
    },
}

impl std::fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoEncoder => f.write_str("no encoder"),
            Self::BadInput(what) => f.write_str(what),
            Self::Os {
                step,
                code,
                message,
            } => write!(f, "{step}: 0x{:08X} {message}", *code as u32),
        }
    }
}

impl std::error::Error for MediaError {}

/// Turns a failed call into a [`MediaError::Os`] naming `step`.
fn os(step: &'static str) -> impl FnOnce(windows::core::Error) -> MediaError {
    move |error| {
        if error.code() == MF_E_TOPO_CODEC_NOT_FOUND {
            MediaError::NoEncoder
        } else {
            MediaError::Os {
                step,
                code: error.code().0,
                message: error.message().trim().to_owned(),
            }
        }
    }
}

thread_local! {
    /// Set by tests (through `sue_platform::movie`) to act as a system
    /// with no encoder, so the fallback can be tried on a machine that has
    /// one. Per thread, so tests running side by side do not see it.
    static PRETEND_NO_ENCODER: Cell<bool> = const { Cell::new(false) };
}

/// Makes [`h264_encoder_present`], [`aac_encoder_present`] and
/// [`Mp4Writer::new`] on this thread act as if Windows had no encoder
/// (`true`), or as it really is (`false`). For tests.
pub fn pretend_no_encoder(pretend: bool) {
    PRETEND_NO_ENCODER.with(|cell| cell.set(pretend));
}

fn pretending() -> bool {
    PRETEND_NO_ENCODER.with(Cell::get)
}

/// COM and Media Foundation, started for as long as this lives, on this
/// thread.
pub(crate) struct Session {
    /// Whether this session's `CoInitializeEx` succeeded and so owes a
    /// `CoUninitialize`. On a thread already in a single threaded
    /// apartment (the window's thread) COM is up and stays as it is.
    com: bool,
    /// Stays on the thread that made it.
    _thread: PhantomData<*const ()>,
}

impl Session {
    #[allow(
        unsafe_code,
        reason = "starting COM and Media Foundation has no safe call (standing approval, rule 13)"
    )]
    pub(crate) fn start() -> Result<Self, MediaError> {
        // SAFETY: CoInitializeEx takes no pointer (the reserved one is
        // None) and may be called on any thread; a success (S_OK or
        // S_FALSE) is paired with CoUninitialize in Drop on this same
        // thread, which `_thread` keeps it on. RPC_E_CHANGED_MODE means
        // the thread already has COM in another apartment, which Media
        // Foundation also works in; then nothing is owed.
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let com = result.is_ok();
        if !com && result != RPC_E_CHANGED_MODE {
            return Err(os("starting COM")(result.into()));
        }
        // SAFETY: MFStartup takes plain numbers. It is reference counted,
        // and paired with MFShutdown in Drop; if it fails, nothing is
        // owed but the COM start, undone right here.
        if let Err(error) = unsafe { MFStartup(MF_VERSION, MFSTARTUP_LITE) } {
            if com {
                // SAFETY: pairs the successful CoInitializeEx above, on
                // the same thread.
                unsafe { CoUninitialize() };
            }
            return Err(os("starting Media Foundation")(error));
        }
        Ok(Self {
            com,
            _thread: PhantomData,
        })
    }
}

impl Drop for Session {
    #[allow(
        unsafe_code,
        reason = "shutting COM and Media Foundation down has no safe call (standing approval, rule 13)"
    )]
    fn drop(&mut self) {
        // SAFETY: pairs the successful MFStartup of `start`. Every Media
        // Foundation object the owner made is a field declared before the
        // session, so it was released first.
        let _ = unsafe { MFShutdown() };
        if self.com {
            // SAFETY: pairs the successful CoInitializeEx of `start`, on
            // the same thread (the session is not Send).
            unsafe { CoUninitialize() };
        }
    }
}

/// Whether a synchronous software encoder from `input` to `output` of
/// `category` is installed. Hardware encoders are not counted: Sue uses
/// the software one, which was found to work where the hardware path
/// failed to start (`research-video-export.md`).
#[allow(
    unsafe_code,
    reason = "MFTEnumEx is a Media Foundation call with no safe route (standing approval, rule 13)"
)]
fn encoder_present(category: GUID, input: GUID, output: GUID, major: GUID) -> bool {
    let Ok(_session) = Session::start() else {
        return false;
    };
    let input = MFT_REGISTER_TYPE_INFO {
        guidMajorType: major,
        guidSubtype: input,
    };
    let output = MFT_REGISTER_TYPE_INFO {
        guidMajorType: major,
        guidSubtype: output,
    };
    let mut activates: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0_u32;
    // SAFETY: the two type infos live on this stack for the whole call;
    // `activates` and `count` are valid places for the out values.
    let found = unsafe {
        MFTEnumEx(
            category,
            MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_LOCALMFT | MFT_ENUM_FLAG_SORTANDFILTER,
            Some(&input),
            Some(&output),
            &mut activates,
            &mut count,
        )
    };
    if found.is_err() || activates.is_null() {
        return false;
    }
    // SAFETY: on success MFTEnumEx gives an array of `count` activation
    // objects, each holding one reference, allocated with CoTaskMemAlloc.
    // Each is taken out (and so released when dropped) exactly once, then
    // the array itself is freed exactly once.
    unsafe {
        for at in 0..count as usize {
            drop(activates.add(at).read());
        }
        CoTaskMemFree(Some(activates.cast_const().cast()));
    }
    count > 0
}

/// Whether Windows has an H.264 encoder here (software, NV12 in).
#[must_use]
pub fn h264_encoder_present() -> bool {
    !pretending()
        && encoder_present(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFVideoFormat_NV12,
            MFVideoFormat_H264,
            MFMediaType_Video,
        )
}

/// Whether Windows has an AAC encoder here (16-bit PCM in).
#[must_use]
pub fn aac_encoder_present() -> bool {
    !pretending()
        && encoder_present(
            MFT_CATEGORY_AUDIO_ENCODER,
            MFAudioFormat_PCM,
            MFAudioFormat_AAC,
            MFMediaType_Audio,
        )
}

/// The picture side of an MP4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoSpec {
    /// Width in pixels: even, [`MIN_SIDE`] to [`MAX_SIDE`], and at most
    /// [`MOST_BLOCKS`] blocks of 16 by 16 with the height.
    pub width: u32,
    /// Height in pixels: as the width.
    pub height: u32,
    /// Frames a second as numerator and denominator.
    pub rate: (u32, u32),
    /// The encoder's quality number, 1 to 100.
    pub quality: u32,
    /// Frames from one key frame to the next.
    pub key_interval: u32,
}

/// The sound side of an MP4: AAC-LC from 16-bit PCM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioSpec {
    /// 44100 or 48000 (what Windows' AAC encoder takes).
    pub rate: u32,
    /// 1 or 2.
    pub channels: u16,
}

/// The smallest side Windows' H.264 encoder takes: 32 is refused, 34 is
/// taken (tried on Windows 11 build 26300, every even size from 32 to 48).
pub const MIN_SIDE: u32 = 34;
/// The largest side taken.
pub const MAX_SIDE: u32 = 4096;
/// The most 16 by 16 blocks of a picture: H.264 level 5.2, 4096 by 2304.
/// The encoder refuses larger pictures unless a higher level is named, and
/// with level 6 named it writes 7680 by 4320, but Windows' own decoder then
/// refuses the file (tried on build 26300), so the Windows player could
/// not play it: Sue stops at what Windows plays.
pub const MOST_BLOCKS: u32 = 36_864;

/// The AAC bit rate: 192 kbit/s, in bytes a second (one of the four the
/// encoder takes: 12000, 16000, 20000, 24000).
const AAC_BYTES_PER_SECOND: u32 = 24_000;

fn pack(high: u32, low: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

/// A media type made from `(key, value)` pairs.
enum Value {
    Guid(GUID),
    U32(u32),
    U64(u64),
}

#[allow(
    unsafe_code,
    reason = "building Media Foundation attribute stores is COM (standing approval, rule 13)"
)]
fn media_type(values: &[(GUID, Value)]) -> Result<IMFMediaType, MediaError> {
    // SAFETY: MFCreateMediaType takes nothing and returns a new object.
    let kind = unsafe { MFCreateMediaType() }.map_err(os("making a media type"))?;
    for (key, value) in values {
        // SAFETY: `key` and the GUID value point at values that live for
        // the call; the attribute store copies them.
        unsafe {
            match value {
                Value::Guid(guid) => kind.SetGUID(key, guid),
                Value::U32(number) => kind.SetUINT32(key, *number),
                Value::U64(number) => kind.SetUINT64(key, *number),
            }
        }
        .map_err(os("describing a media type"))?;
    }
    Ok(kind)
}

#[allow(
    unsafe_code,
    reason = "building Media Foundation attribute stores is COM (standing approval, rule 13)"
)]
fn attributes(values: &[(GUID, u32)]) -> Result<IMFAttributes, MediaError> {
    let mut store = None;
    // SAFETY: `store` is a valid place for the out pointer.
    unsafe { MFCreateAttributes(&mut store, values.len() as u32) }
        .map_err(os("making attributes"))?;
    let store = store.ok_or(MediaError::BadInput("no attribute store"))?;
    for (key, value) in values {
        // SAFETY: `key` lives for the call; the store copies it.
        unsafe { store.SetUINT32(key, *value) }.map_err(os("setting an attribute"))?;
    }
    Ok(store)
}

/// A file name in the system's temporary folder no other writer uses,
/// removed when this is dropped.
struct TempFile(PathBuf);

impl TempFile {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let name = format!(
            "sue-movie-{}-{}.mp4",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        Self(std::env::temp_dir().join(name))
    }

    /// The path as a NUL terminated wide string.
    fn wide(&self) -> Vec<u16> {
        self.0
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        // Best effort: a file that is not there is fine.
        let _ = std::fs::remove_file(&self.0);
    }
}

/// An MP4 being written by Windows' Sink Writer.
///
/// The MPEG-4 sink refuses a COM memory stream (BeginWriting fails with
/// E_INVALIDARG; tried on Windows 11 build 26300), so the file is written
/// to a temporary file of its own, read back whole by [`Mp4Writer::finish`]
/// and removed, whatever happens.
pub struct Mp4Writer {
    writer: IMFSinkWriter,
    /// Declared after the writer, so the file is closed before it goes.
    file: TempFile,
    video: u32,
    audio: Option<u32>,
    spec: VideoSpec,
    sound: Option<AudioSpec>,
    /// Declared last, so the objects above are released before Media
    /// Foundation and COM shut down.
    _session: Session,
}

impl std::fmt::Debug for Mp4Writer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mp4Writer")
            .field("spec", &self.spec)
            .field("sound", &self.sound)
            .finish_non_exhaustive()
    }
}

impl Mp4Writer {
    /// Begins an MP4 of H.264 High video, with AAC-LC sound when `audio`
    /// is given and no sound track otherwise.
    ///
    /// # Errors
    /// [`MediaError::NoEncoder`] where Windows has no H.264 encoder (or no
    /// AAC one, when sound is asked for); [`MediaError::BadInput`] for an
    /// odd or out of range size, a zero rate or quality, or sound Windows'
    /// AAC encoder does not take; [`MediaError::Os`] when Media Foundation
    /// refuses.
    #[allow(
        unsafe_code,
        reason = "the Sink Writer is COM, with no safe call (standing approval, rule 13)"
    )]
    pub fn new(spec: VideoSpec, audio: Option<AudioSpec>) -> Result<Self, MediaError> {
        if spec.width % 2 == 1 || spec.height % 2 == 1 {
            return Err(MediaError::BadInput("odd width or height"));
        }
        let blocks = spec.width.div_ceil(16) * spec.height.div_ceil(16);
        if !(MIN_SIDE..=MAX_SIDE).contains(&spec.width)
            || !(MIN_SIDE..=MAX_SIDE).contains(&spec.height)
            || blocks > MOST_BLOCKS
        {
            return Err(MediaError::BadInput("size out of range"));
        }
        if spec.rate.0 == 0 || spec.rate.1 == 0 || !(1..=100).contains(&spec.quality) {
            return Err(MediaError::BadInput("rate or quality out of range"));
        }
        if let Some(sound) = audio
            && (![44_100, 48_000].contains(&sound.rate) || !(1..=2).contains(&sound.channels))
        {
            return Err(MediaError::BadInput("sound rate or channels"));
        }
        if !h264_encoder_present() || (audio.is_some() && !aac_encoder_present()) {
            return Err(MediaError::NoEncoder);
        }
        let session = Session::start()?;
        let file = TempFile::new();
        let url = file.wide();
        let settings = attributes(&[
            (MF_TRANSCODE_CONTAINERTYPE, 0),
            (MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 0),
            (MF_SINK_WRITER_DISABLE_THROTTLING, 1),
        ])?;
        // SAFETY: the keys and value live for the call; the store copies
        // them. This replaces the placeholder number set just above.
        unsafe { settings.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4) }
            .map_err(os("choosing MP4"))?;
        // SAFETY: `url` is a NUL terminated wide path that lives for the
        // call. Read and write access: with the index before the data the
        // MPEG-4 sink reads back what it wrote (write only access fails
        // with "Access is denied" in Finalize).
        let target = unsafe {
            MFCreateFile(
                MF_ACCESSMODE_READWRITE,
                MF_OPENMODE_DELETE_IF_EXIST,
                MF_FILEFLAGS_NONE,
                PCWSTR(url.as_ptr()),
            )
        }
        .map_err(os("making the movie file"))?;
        // SAFETY: the byte stream and `settings` are live; the URL may be
        // null when a byte stream is given, the container being named in
        // `settings`.
        let writer = unsafe { MFCreateSinkWriterFromURL(PCWSTR::null(), &target, &settings) }
            .map_err(os("making the sink writer"))?;
        let size = pack(spec.width, spec.height);
        let rate = pack(spec.rate.0, spec.rate.1);
        let colour = || {
            [
                (MF_MT_FRAME_SIZE, Value::U64(size)),
                (MF_MT_FRAME_RATE, Value::U64(rate)),
                (MF_MT_PIXEL_ASPECT_RATIO, Value::U64(pack(1, 1))),
                (
                    MF_MT_INTERLACE_MODE,
                    Value::U32(MFVideoInterlace_Progressive.0 as u32),
                ),
                (
                    MF_MT_VIDEO_PRIMARIES,
                    Value::U32(MFVideoPrimaries_BT709.0 as u32),
                ),
                (
                    MF_MT_TRANSFER_FUNCTION,
                    Value::U32(MFVideoTransFunc_709.0 as u32),
                ),
                (
                    MF_MT_YUV_MATRIX,
                    Value::U32(MFVideoTransferMatrix_BT709.0 as u32),
                ),
                (
                    MF_MT_VIDEO_NOMINAL_RANGE,
                    Value::U32(MFNominalRange_16_235.0 as u32),
                ),
            ]
        };
        // The average bit rate the output type declares (the encoder
        // wants one): a quarter bit per pixel of each frame, at least
        // 1 Mbit/s.
        // Quality based rate control does not hold to it: grainy pictures
        // at "high" came out several times larger (docs/movie-export.md).
        let pixels_a_second =
            u64::from(spec.width) * u64::from(spec.height) * u64::from(spec.rate.0)
                / u64::from(spec.rate.1);
        let bitrate = u32::try_from((pixels_a_second / 4).max(1_000_000)).unwrap_or(u32::MAX);
        let mut out = vec![
            (MF_MT_MAJOR_TYPE, Value::Guid(MFMediaType_Video)),
            (MF_MT_SUBTYPE, Value::Guid(MFVideoFormat_H264)),
            (MF_MT_AVG_BITRATE, Value::U32(bitrate)),
            (
                MF_MT_MPEG2_PROFILE,
                Value::U32(eAVEncH264VProfile_High.0 as u32),
            ),
        ];
        out.extend(colour());
        let mut input = vec![
            (MF_MT_MAJOR_TYPE, Value::Guid(MFMediaType_Video)),
            (MF_MT_SUBTYPE, Value::Guid(MFVideoFormat_NV12)),
            (MF_MT_DEFAULT_STRIDE, Value::U32(spec.width)),
            (MF_MT_ALL_SAMPLES_INDEPENDENT, Value::U32(1)),
        ];
        input.extend(colour());
        let encoding = attributes(&[
            (
                CODECAPI_AVEncCommonRateControlMode,
                eAVEncCommonRateControlMode_Quality.0 as u32,
            ),
            (CODECAPI_AVEncCommonQuality, spec.quality),
            (CODECAPI_AVEncMPVGOPSize, spec.key_interval.max(1)),
            // No B frames. With them the encoder shows the first picture
            // one frame after it is decoded, and Windows' own player (the
            // Source Reader) ignores the edit list that would correct it,
            // so pictures ran one frame behind the sound (tried on build
            // 26300). Without B frames every player starts at zero.
            (CODECAPI_AVEncMPVDefaultBPictureCount, 0),
        ])?;
        let target = media_type(&out)?;
        // SAFETY: `writer` and `target` are live COM objects.
        let video = unsafe { writer.AddStream(&target) }.map_err(os("adding the video"))?;
        let source = media_type(&input)?;
        // SAFETY: as above; `encoding` configures the encoder it loads.
        unsafe { writer.SetInputMediaType(video, &source, &encoding) }
            .map_err(os("starting the H.264 encoder"))?;
        let audio_stream = match audio {
            None => None,
            Some(sound) => {
                let channels = u32::from(sound.channels);
                let target = media_type(&[
                    (MF_MT_MAJOR_TYPE, Value::Guid(MFMediaType_Audio)),
                    (MF_MT_SUBTYPE, Value::Guid(MFAudioFormat_AAC)),
                    (MF_MT_AUDIO_BITS_PER_SAMPLE, Value::U32(16)),
                    (MF_MT_AUDIO_SAMPLES_PER_SECOND, Value::U32(sound.rate)),
                    (MF_MT_AUDIO_NUM_CHANNELS, Value::U32(channels)),
                    (
                        MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
                        Value::U32(AAC_BYTES_PER_SECOND),
                    ),
                    // Raw AAC frames, as MP4 stores them; AAC-LC level 2.
                    (MF_MT_AAC_PAYLOAD_TYPE, Value::U32(0)),
                    (MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, Value::U32(0x29)),
                ])?;
                // SAFETY: `writer` and `target` are live COM objects.
                let index = unsafe { writer.AddStream(&target) }.map_err(os("adding the sound"))?;
                let source = media_type(&[
                    (MF_MT_MAJOR_TYPE, Value::Guid(MFMediaType_Audio)),
                    (MF_MT_SUBTYPE, Value::Guid(MFAudioFormat_PCM)),
                    (MF_MT_AUDIO_BITS_PER_SAMPLE, Value::U32(16)),
                    (MF_MT_AUDIO_SAMPLES_PER_SECOND, Value::U32(sound.rate)),
                    (MF_MT_AUDIO_NUM_CHANNELS, Value::U32(channels)),
                    (MF_MT_AUDIO_BLOCK_ALIGNMENT, Value::U32(channels * 2)),
                    (
                        MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
                        Value::U32(channels * 2 * sound.rate),
                    ),
                ])?;
                // SAFETY: as above; no encoder settings.
                unsafe { writer.SetInputMediaType(index, &source, None) }
                    .map_err(os("starting the AAC encoder"))?;
                Some(index)
            }
        };
        // SAFETY: every stream has its types; the writer is live.
        unsafe { writer.BeginWriting() }.map_err(os("beginning the movie"))?;
        Ok(Self {
            writer,
            file,
            video,
            audio: audio_stream,
            spec,
            sound: audio,
            _session: session,
        })
    }

    /// The picture side it was begun with.
    #[must_use]
    pub fn spec(&self) -> VideoSpec {
        self.spec
    }

    /// Hands `bytes` to stream `index` as one sample at `time` for
    /// `duration`, both in 100 nanosecond units.
    #[allow(
        unsafe_code,
        reason = "Media Foundation buffers and samples are COM (standing approval, rule 13)"
    )]
    fn write(&self, index: u32, bytes: &[u8], time: i64, duration: i64) -> Result<(), MediaError> {
        let length =
            u32::try_from(bytes.len()).map_err(|_| MediaError::BadInput("sample too large"))?;
        // SAFETY: makes a new buffer of `length` bytes.
        let buffer = unsafe { MFCreateMemoryBuffer(length) }.map_err(os("making a buffer"))?;
        let mut data: *mut u8 = std::ptr::null_mut();
        let mut room = 0_u32;
        // SAFETY: `data` and `room` are valid places for the out values.
        unsafe { buffer.Lock(&mut data, Some(&mut room), None) }.map_err(os("locking a buffer"))?;
        if data.is_null() || room < length {
            // SAFETY: pairs the successful Lock above.
            let _ = unsafe { buffer.Unlock() };
            return Err(MediaError::BadInput("buffer smaller than asked"));
        }
        // SAFETY: Lock gave `data`, writable for `room >= length` bytes
        // until Unlock; `bytes` is a separate Rust slice of `length`
        // bytes, so the two do not overlap. Unlock pairs the Lock.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
            buffer.Unlock()
        }
        .map_err(os("unlocking a buffer"))?;
        // SAFETY: `length` is within the buffer's size.
        unsafe { buffer.SetCurrentLength(length) }.map_err(os("filling a buffer"))?;
        // SAFETY: makes a new, empty sample.
        let sample = unsafe { MFCreateSample() }.map_err(os("making a sample"))?;
        let timed = || -> windows::core::Result<()> {
            // SAFETY: `sample` and `buffer` are live; the sample takes its
            // own reference to the buffer; times are plain numbers.
            unsafe {
                sample.AddBuffer(&buffer)?;
                sample.SetSampleTime(time)?;
                sample.SetSampleDuration(duration)
            }
        };
        timed().map_err(os("timing a sample"))?;
        // SAFETY: `index` is a stream of this writer; `sample` is live.
        unsafe { self.writer.WriteSample(index, &sample) }.map_err(os("encoding a sample"))
    }

    /// Adds a picture: NV12 of the writer's size, at `time` for
    /// `duration` (100 nanosecond units).
    ///
    /// # Errors
    /// A picture of the wrong length, or a failing encoder.
    pub fn write_video(&mut self, nv12: &[u8], time: i64, duration: i64) -> Result<(), MediaError> {
        let expected = self.spec.width as usize * self.spec.height as usize * 3 / 2;
        if nv12.len() != expected {
            return Err(MediaError::BadInput("picture of the wrong size"));
        }
        self.write(self.video, nv12, time, duration)
    }

    /// Adds sound: interleaved 16-bit samples, at `time` for `duration`.
    ///
    /// # Errors
    /// A writer begun without sound, a sample count that does not fill
    /// whole frames, or a failing encoder.
    pub fn write_audio(&mut self, pcm: &[i16], time: i64, duration: i64) -> Result<(), MediaError> {
        let (Some(index), Some(sound)) = (self.audio, self.sound) else {
            return Err(MediaError::BadInput("no sound track"));
        };
        if !pcm.len().is_multiple_of(usize::from(sound.channels)) {
            return Err(MediaError::BadInput("sound cut inside a frame"));
        }
        let bytes: Vec<u8> = pcm.iter().flat_map(|sample| sample.to_le_bytes()).collect();
        self.write(index, &bytes, time, duration)
    }

    /// Ends the movie and gives the MP4's bytes.
    ///
    /// # Errors
    /// When the encoders or the MP4 sink fail to finish.
    #[allow(
        unsafe_code,
        reason = "finalizing the Sink Writer is COM (standing approval, rule 13)"
    )]
    pub fn finish(self) -> Result<Vec<u8>, MediaError> {
        // SAFETY: the writer is live and began writing; Finalize drains the
        // encoders and writes the index.
        unsafe { self.writer.Finalize() }.map_err(os("finishing the movie"))?;
        let file = &self.file.0;
        std::fs::read(file).map_err(|error| MediaError::Os {
            step: "reading the movie back",
            code: error.raw_os_error().unwrap_or(0),
            message: error.to_string(),
        })
    }
}

/// What [`read_mp4`] found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mp4Contents {
    /// The video's display width and height.
    pub width: u32,
    /// See `width`.
    pub height: u32,
    /// The H.264 profile the file declares (100 is High), 0 if unknown.
    pub profile: u32,
    /// How many pictures decoded.
    pub frames: usize,
    /// Each picture's start, in 100 nanosecond units.
    pub times: Vec<i64>,
    /// The first pictures (as many as asked for), NV12 at display size.
    pub pictures: Vec<Vec<u8>>,
    /// The sound's rate and channels, if it has a sound track.
    pub sound: Option<(u32, u16)>,
    /// How many sound frames (one sample per channel) decoded.
    pub sound_frames: usize,
    /// The loudest decoded sample, 0 to 32767.
    pub sound_peak: u16,
}

/// The display area of a decoded type: (left, top, width, height).
#[allow(
    unsafe_code,
    reason = "reading Media Foundation attributes is COM (standing approval, rule 13)"
)]
fn aperture(kind: &IMFMediaType, size: (u32, u32)) -> (u32, u32, u32, u32) {
    let mut blob = [0_u8; 16];
    // SAFETY: `blob` is a valid 16 byte buffer, the size of an
    // MFVideoArea; GetBlob fails rather than write past it.
    let read = unsafe { kind.GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, &mut blob, None) };
    if read.is_err() {
        return (0, 0, size.0, size.1);
    }
    let int = |at: usize| i32::from_le_bytes([blob[at], blob[at + 1], blob[at + 2], blob[at + 3]]);
    // MFVideoArea: OffsetX and OffsetY are a fraction (u16) then a whole
    // part (i16); then the width and height as i32.
    let left = i16::from_le_bytes([blob[2], blob[3]]).max(0) as u32;
    let top = i16::from_le_bytes([blob[6], blob[7]]).max(0) as u32;
    (left, top, int(8).max(0) as u32, int(12).max(0) as u32)
}

/// Reads an MP4 back through Windows' own decoders: the video as NV12
/// (the first `keep` pictures kept whole), the sound as 16-bit PCM.
///
/// # Errors
/// When Media Foundation cannot open or decode the file.
#[allow(
    unsafe_code,
    reason = "the Source Reader is COM, with no safe call (standing approval, rule 13)"
)]
pub fn read_mp4(bytes: &[u8], keep: usize) -> Result<Mp4Contents, MediaError> {
    let session = Session::start()?;
    let file = TempFile::new();
    std::fs::write(&file.0, bytes).map_err(|error| MediaError::Os {
        step: "loading the movie",
        code: error.raw_os_error().unwrap_or(0),
        message: error.to_string(),
    })?;
    let url = file.wide();
    // SAFETY: `url` is a NUL terminated wide path that lives for the
    // call; no attributes.
    let reader: IMFSourceReader =
        unsafe { MFCreateSourceReaderFromURL(PCWSTR(url.as_ptr()), None) }
            .map_err(os("opening the movie"))?;
    let video = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    let audio = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
    let mut out = Mp4Contents::default();
    // SAFETY: reads the stream's first native type, a new object.
    if let Ok(native) = unsafe { reader.GetNativeMediaType(video, 0) } {
        // SAFETY: the key lives for the call.
        out.profile = unsafe { native.GetUINT32(&MF_MT_MPEG2_PROFILE) }.unwrap_or(0);
    }
    let nv12 = media_type(&[
        (MF_MT_MAJOR_TYPE, Value::Guid(MFMediaType_Video)),
        (MF_MT_SUBTYPE, Value::Guid(MFVideoFormat_NV12)),
    ])?;
    // SAFETY: `nv12` is live; the reserved pointer is None.
    unsafe { reader.SetCurrentMediaType(video, None, &nv12) }
        .map_err(os("asking for NV12 pictures"))?;
    let pcm = media_type(&[
        (MF_MT_MAJOR_TYPE, Value::Guid(MFMediaType_Audio)),
        (MF_MT_SUBTYPE, Value::Guid(MFAudioFormat_PCM)),
        (MF_MT_AUDIO_BITS_PER_SAMPLE, Value::U32(16)),
    ])?;
    // SAFETY: as above. A movie without sound has no audio stream; then
    // this fails and the sound is left out.
    let has_sound = unsafe { reader.SetCurrentMediaType(audio, None, &pcm) }.is_ok();
    // The pictures.
    loop {
        let mut flags = 0_u32;
        let mut time = 0_i64;
        let mut sample = None;
        // SAFETY: the out values point at live locals.
        unsafe {
            reader.ReadSample(
                video,
                0,
                None,
                Some(&mut flags),
                Some(&mut time),
                Some(&mut sample),
            )
        }
        .map_err(os("decoding a picture"))?;
        if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
            break;
        }
        let Some(sample) = sample else { continue };
        out.frames += 1;
        out.times.push(time);
        // SAFETY: reads the reader's current type, a new object.
        let kind = unsafe { reader.GetCurrentMediaType(video) }.map_err(os("reading the type"))?;
        // SAFETY: the key lives for the call.
        let size = unsafe { kind.GetUINT64(&MF_MT_FRAME_SIZE) }.map_err(os("reading the size"))?;
        let size = ((size >> 32) as u32, size as u32);
        let (left, top, width, height) = aperture(&kind, size);
        out.width = width;
        out.height = height;
        if out.pictures.len() >= keep {
            continue;
        }
        // SAFETY: `sample` is live; the result is a new reference.
        let buffer =
            unsafe { sample.ConvertToContiguousBuffer() }.map_err(os("joining a picture"))?;
        let mut data: *mut u8 = std::ptr::null_mut();
        let mut filled = 0_u32;
        // SAFETY: the out values point at live locals.
        unsafe { buffer.Lock(&mut data, None, Some(&mut filled)) }
            .map_err(os("locking a picture"))?;
        // SAFETY: Lock gave `data`, readable for `filled` bytes until the
        // Unlock below; the slice is copied out before then.
        let all = unsafe { std::slice::from_raw_parts(data, filled as usize) }.to_vec();
        // SAFETY: pairs the Lock above.
        let _ = unsafe { buffer.Unlock() };
        // Rows may be wider than the picture and the buffer taller (a
        // decoder works in 16 pixel blocks): the stride follows from the
        // length, the luma rows from the frame size.
        let rows = size.1 as usize;
        let stride = (all.len() * 2 / 3).checked_div(rows).unwrap_or(0);
        let mut picture = Vec::with_capacity(width as usize * height as usize * 3 / 2);
        let (left, top, w, h) = (left as usize, top as usize, width as usize, height as usize);
        for y in 0..h {
            let at = (top + y) * stride + left;
            picture.extend_from_slice(all.get(at..at + w).unwrap_or(&[]));
        }
        for y in 0..h / 2 {
            let at = rows * stride + (top / 2 + y) * stride + left;
            picture.extend_from_slice(all.get(at..at + w).unwrap_or(&[]));
        }
        out.pictures.push(picture);
    }
    // The sound.
    if has_sound {
        // SAFETY: reads the reader's current type, a new object.
        let kind = unsafe { reader.GetCurrentMediaType(audio) }.map_err(os("reading the type"))?;
        // SAFETY: the keys live for the calls.
        let (rate, channels) = unsafe {
            (
                kind.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).unwrap_or(0),
                kind.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).unwrap_or(1),
            )
        };
        out.sound = Some((rate, channels as u16));
        let mut samples = 0_usize;
        loop {
            let mut flags = 0_u32;
            let mut sample = None;
            // SAFETY: the out values point at live locals.
            unsafe { reader.ReadSample(audio, 0, None, Some(&mut flags), None, Some(&mut sample)) }
                .map_err(os("decoding sound"))?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
            let Some(sample) = sample else { continue };
            // SAFETY: `sample` is live; the result is a new reference.
            let buffer =
                unsafe { sample.ConvertToContiguousBuffer() }.map_err(os("joining sound"))?;
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut filled = 0_u32;
            // SAFETY: the out values point at live locals.
            unsafe { buffer.Lock(&mut data, None, Some(&mut filled)) }
                .map_err(os("locking sound"))?;
            // SAFETY: Lock gave `data`, readable for `filled` bytes until
            // the Unlock below; the slice is read before then.
            let all = unsafe { std::slice::from_raw_parts(data, filled as usize) };
            for pair in all.as_chunks::<2>().0 {
                let value = i16::from_le_bytes(*pair).unsigned_abs();
                out.sound_peak = out.sound_peak.max(value.min(32_767));
            }
            samples += all.len() / 2;
            // SAFETY: pairs the Lock above.
            let _ = unsafe { buffer.Unlock() };
        }
        out.sound_frames = samples / usize::from(channels.max(1) as u16);
    }
    // The reader closes the file before it is removed, and both go before
    // Media Foundation shuts down.
    drop(reader);
    drop(file);
    drop(session);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat NV12 picture.
    fn flat(width: u32, height: u32, luma: u8, cb: u8, cr: u8) -> Vec<u8> {
        let mut out = vec![luma; (width * height) as usize];
        for _ in 0..(width * height / 4) {
            out.extend_from_slice(&[cb, cr]);
        }
        out
    }

    fn spec(width: u32, height: u32) -> VideoSpec {
        VideoSpec {
            width,
            height,
            rate: (24, 1),
            quality: 75,
            key_interval: 48,
        }
    }

    #[test]
    fn the_encoders_are_found_on_this_machine() {
        // The development machine runs Windows 11 Home, which has both.
        assert!(h264_encoder_present());
        assert!(aac_encoder_present());
    }

    #[test]
    fn a_silent_movie_is_written_and_reads_back_frame_by_frame() {
        let mut writer = Mp4Writer::new(spec(64, 48), None).unwrap();
        let tick = 10_000_000 / 24;
        for frame in 0..12_i64 {
            let shade = if frame < 6 { 40 } else { 200 };
            writer
                .write_video(&flat(64, 48, shade, 128, 128), frame * tick, tick)
                .unwrap();
        }
        let bytes = writer.finish().unwrap();
        // An MP4: ftyp first, the index before the data.
        assert_eq!(&bytes[4..8], b"ftyp");
        let at = |tag: &[u8]| bytes.windows(4).position(|window| window == tag);
        assert!(at(b"moov").is_some() && at(b"mdat").is_some());
        let read = read_mp4(&bytes, 12).unwrap();
        assert_eq!(read.frames, 12);
        assert_eq!((read.width, read.height), (64, 48));
        assert_eq!(read.profile, 100, "High profile");
        assert_eq!(read.sound, None);
        assert!(read.times.windows(2).all(|pair| pair[0] < pair[1]));
        // Each picture is shown when it was written: no B frame delay.
        for (index, shown) in read.times.iter().enumerate() {
            let exact = index as i64 * 10_000_000 / 24;
            assert!((shown - exact).abs() <= 8, "{:?}", read.times);
        }
        for (index, picture) in read.pictures.iter().enumerate() {
            let want = if index < 6 { 40 } else { 200 };
            let mean = picture[..64 * 48]
                .iter()
                .map(|v| u32::from(*v))
                .sum::<u32>()
                / (64 * 48);
            assert!(mean.abs_diff(want) <= 3, "frame {index}: {mean} for {want}");
        }
    }

    #[test]
    fn sound_goes_in_as_aac_and_comes_back() {
        let mut writer = Mp4Writer::new(
            spec(64, 64),
            Some(AudioSpec {
                rate: 48_000,
                channels: 2,
            }),
        )
        .unwrap();
        let tick = 10_000_000 / 24;
        // One second: a 440 Hz tone at half scale, in 2000 frame pieces.
        let tone: Vec<i16> = (0..48_000)
            .flat_map(|at| {
                let value = (f64::from(at) * 440.0 * std::f64::consts::TAU / 48_000.0).sin();
                let value = (value * 16_000.0) as i16;
                [value, value]
            })
            .collect();
        for (piece, chunk) in tone.chunks(4000).enumerate() {
            let time = piece as i64 * 2000 * 10_000_000 / 48_000;
            writer
                .write_audio(chunk, time, 2000 * 10_000_000 / 48_000)
                .unwrap();
        }
        for frame in 0..24_i64 {
            writer
                .write_video(&flat(64, 64, 128, 128, 128), frame * tick, tick)
                .unwrap();
        }
        let read = read_mp4(&writer.finish().unwrap(), 0).unwrap();
        assert_eq!(read.frames, 24);
        assert_eq!(read.sound, Some((48_000, 2)));
        // AAC adds priming and pads the last packet: about a second.
        assert!(
            (46_000..=51_000).contains(&read.sound_frames),
            "{}",
            read.sound_frames
        );
        assert!(
            read.sound_peak > 10_000,
            "the tone is there: {}",
            read.sound_peak
        );
    }

    #[test]
    fn bad_input_and_a_missing_encoder_are_errors_not_panics() {
        assert_eq!(
            Mp4Writer::new(spec(63, 48), None).err(),
            Some(MediaError::BadInput("odd width or height"))
        );
        assert!(Mp4Writer::new(spec(16, 16), None).is_err());
        assert!(Mp4Writer::new(spec(32, 48), None).is_err());
        assert!(Mp4Writer::new(spec(4096, 4096), None).is_err());
        assert!(Mp4Writer::new(spec(7680, 4320), None).is_err());
        // The smallest and the largest sizes write and read back.
        for (width, height) in [(MIN_SIDE, MIN_SIDE), (4096, 2304), (2304, 4096)] {
            let mut movie = Mp4Writer::new(spec(width, height), None).unwrap();
            movie
                .write_video(&flat(width, height, 100, 128, 128), 0, 400_000)
                .unwrap();
            let read = read_mp4(&movie.finish().unwrap(), 0).unwrap();
            assert_eq!((read.frames, read.width, read.height), (1, width, height));
        }
        let bad_sound = AudioSpec {
            rate: 22_050,
            channels: 2,
        };
        assert!(Mp4Writer::new(spec(64, 48), Some(bad_sound)).is_err());
        let mut writer = Mp4Writer::new(spec(64, 48), None).unwrap();
        assert!(writer.write_video(&[0; 10], 0, 1).is_err());
        assert!(writer.write_audio(&[0; 2], 0, 1).is_err(), "no sound track");
        assert!(read_mp4(b"not a movie at all", 1).is_err());
        // A system without the encoder: refused cleanly.
        pretend_no_encoder(true);
        let refused = Mp4Writer::new(spec(64, 48), None).err();
        let present = h264_encoder_present();
        pretend_no_encoder(false);
        assert_eq!(refused, Some(MediaError::NoEncoder));
        assert!(!present);
        assert!(h264_encoder_present());
    }
}
