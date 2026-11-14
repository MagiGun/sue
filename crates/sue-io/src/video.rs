//! Movies. The neutral part of movie export lives here: the formats Sue
//! offers ([`MovieFormat`]), what a person picks ([`MovieSettings`]), the
//! picture conversion every system encoder wants (RGBA to [`Nv12`], in
//! safe Rust) and the small [`MovieSink`] trait every writer implements.
//!
//! One sink is Sue's own: Motion JPEG in an AVI container ([`MjpegAvi`]),
//! which the players people already have open. Each frame is a whole JPEG
//! picture, so frames need no encoder of their own and every one is a key
//! frame. The MP4 sink stands on the operating system's own H.264 and AAC
//! encoders, so it lives in `sue-platform` (`sue_platform::movie`); this
//! crate holds nothing of the system.
//!
//! The texts stay English here; the interface reads them in the language in
//! use through `sue_lang::translate("io", ...)`.

use sue_core::{PixelFormat, RasterTile2D};

/// "MP4 (H.264)", the name a dialog shows for [`MovieFormat::Mp4H264`].
const MP4_NAME: &str = "MP4 (H.264)";
/// "AVI (Motion JPEG)", the name for [`MovieFormat::MjpegAvi`].
const AVI_NAME: &str = "AVI (Motion JPEG)";
/// The names of the three qualities.
const SMALL_NAME: &str = "Small file";
const NORMAL_NAME: &str = "Normal";
const HIGH_NAME: &str = "High quality";
/// The errors, in the words a person reads.
const NO_ENCODER: &str = "this system has no MP4 (H.264) encoder";
const NO_SOUND_TRACK: &str = "this movie format carries no sound";
const WRONG_SIZE: &str = "every frame must be the movie's size";
const BAD_SIZE: &str = "the movie size is outside what the format allows";
const BAD_RATE: &str = "the frame rate is not usable";
const BAD_SOUND: &str = "the sound's rate or channel count does not fit the format";
const NO_PIXELS: &str = "the frame has no 8-bit RGBA pixels";
const NO_FRAMES: &str = "the movie has no frames";
const ENCODER_FAILED: &str = "the system encoder failed";
/// What a movie dialog says when it pads an odd size by one pixel.
pub const PADDED_NOTE: &str = "MP4 needs an even width and height: one pixel is added";

/// The most 16 by 16 blocks an MP4 picture may have: H.264 level 5.2, for
/// example 4096 by 2304 or 3840 by 2160. Windows' encoder writes larger
/// pictures only at level 6, which Windows' own decoder refuses.
pub const MP4_MOST_BLOCKS: u32 = 36_864;

/// The movie formats Sue writes, in the order a dialog offers them. Their
/// [`MovieFormat::id`]s are stable names (saved in settings and presets).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MovieFormat {
    /// H.264 High video and AAC-LC sound in MP4, encoded by the operating
    /// system (`sue_platform::movie` says whether this system has it).
    Mp4H264,
    /// Motion JPEG in AVI, Sue's own writer: everywhere, no sound.
    MjpegAvi,
}

impl MovieFormat {
    /// Every format, MP4 first.
    pub const ALL: [Self; 2] = [Self::Mp4H264, Self::MjpegAvi];

    /// The stable name, as presets and settings store it.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Mp4H264 => "mp4-h264",
            Self::MjpegAvi => "avi-mjpeg",
        }
    }

    /// The format of a stable name.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|format| format.id() == id)
    }

    /// The file extension, without the dot.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4H264 => "mp4",
            Self::MjpegAvi => "avi",
        }
    }

    /// The name a dialog shows, in English (`io.ftl` has it).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Mp4H264 => MP4_NAME,
            Self::MjpegAvi => AVI_NAME,
        }
    }

    /// Whether the format carries a sound track.
    #[must_use]
    pub fn carries_sound(self) -> bool {
        matches!(self, Self::Mp4H264)
    }

    /// The sound rates the format takes: Windows' AAC encoder takes 44100
    /// and 48000 (the timeline mixes at 48000).
    #[must_use]
    pub fn sound_rates(self) -> &'static [u32] {
        match self {
            Self::Mp4H264 => &[44_100, 48_000],
            Self::MjpegAvi => &[],
        }
    }

    /// Whether width and height must be even (4:2:0 video). An odd size
    /// is padded by one pixel, repeating the last column or row.
    #[must_use]
    pub fn needs_even_size(self) -> bool {
        matches!(self, Self::Mp4H264)
    }

    /// The smallest and largest width and height the format takes. The
    /// Windows H.264 encoder refuses a side of 32 and takes 34; up to 4096
    /// it takes any picture of at most [`MP4_MOST_BLOCKS`] blocks.
    #[must_use]
    pub fn size_limits(self) -> (u32, u32) {
        match self {
            Self::Mp4H264 => (34, 4096),
            Self::MjpegAvi => (1, 0x7FFF),
        }
    }
}

/// How much a movie is squeezed. The stable names are `small`, `normal`
/// and `high`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MovieQuality {
    /// The smallest file that still looks clean.
    Small,
    /// The default.
    #[default]
    Normal,
    /// Close to the picture, larger files.
    High,
}

impl MovieQuality {
    /// Every quality, smallest file first.
    pub const ALL: [Self; 3] = [Self::Small, Self::Normal, Self::High];

    /// The stable name.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Normal => "normal",
            Self::High => "high",
        }
    }

    /// The quality of a stable name.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|quality| quality.id() == id)
    }

    /// The name a dialog shows, in English (`io.ftl` has it).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Small => SMALL_NAME,
            Self::Normal => NORMAL_NAME,
            Self::High => HIGH_NAME,
        }
    }

    /// The JPEG quality of a Motion JPEG frame.
    #[must_use]
    pub fn jpeg_quality(self) -> u8 {
        match self {
            Self::Small => 75,
            Self::Normal => 90,
            Self::High => 97,
        }
    }

    /// The quality number (1 to 100) of a quality based H.264 encoder.
    /// Measured on flat colour animation and on a painted screen with the
    /// Windows encoder (`docs/movie-export.md`).
    #[must_use]
    pub fn h264_quality(self) -> u32 {
        match self {
            Self::Small => 60,
            Self::Normal => 75,
            Self::High => 90,
        }
    }
}

/// The sound track of a movie: interleaved 32-bit float samples at `rate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MovieSound {
    /// Samples a second (the timeline mixes at 48000).
    pub rate: u32,
    /// 1 or 2.
    pub channels: u16,
}

/// What a movie is: its picture size, frame rate, quality and sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MovieSettings {
    /// The width of the frames pushed in, in pixels.
    pub width: u32,
    /// The height of the frames pushed in, in pixels.
    pub height: u32,
    /// Frames a second, as `numerator / denominator` (24/1, 30000/1001).
    pub rate: (u32, u32),
    /// How much it is squeezed.
    pub quality: MovieQuality,
    /// The sound track, or none.
    pub sound: Option<MovieSound>,
}

/// Media time counts in 100 nanosecond units, as Media Foundation and
/// most containers do.
pub const TICKS_PER_SECOND: u64 = 10_000_000;

impl MovieSettings {
    /// A silent movie of `width` by `height` at `fps` frames a second.
    #[must_use]
    pub fn new(width: u32, height: u32, fps: u32) -> Self {
        Self {
            width,
            height,
            rate: (fps, 1),
            quality: MovieQuality::Normal,
            sound: None,
        }
    }

    /// The size the encoder is given: odd sides gain one pixel where the
    /// format needs even ones.
    #[must_use]
    pub fn encoded_size(&self, format: MovieFormat) -> (u32, u32) {
        if format.needs_even_size() {
            (
                self.width.saturating_add(self.width % 2),
                self.height.saturating_add(self.height % 2),
            )
        } else {
            (self.width, self.height)
        }
    }

    /// Whether the size is padded for `format` (a dialog says so, with
    /// [`PADDED_NOTE`]).
    #[must_use]
    pub fn is_padded(&self, format: MovieFormat) -> bool {
        self.encoded_size(format) != (self.width, self.height)
    }

    /// When frame `index` starts, in [`TICKS_PER_SECOND`] units. Exact on
    /// whole ticks, so no drift builds up over a long movie.
    #[must_use]
    pub fn frame_time(&self, index: u64) -> u64 {
        let (numerator, denominator) = self.rate;
        let ticks =
            u128::from(index) * u128::from(TICKS_PER_SECOND) * u128::from(denominator.max(1));
        u64::try_from(ticks / u128::from(numerator.max(1))).unwrap_or(u64::MAX)
    }

    /// Frames a second as a number.
    #[must_use]
    pub fn fps(&self) -> f64 {
        f64::from(self.rate.0) / f64::from(self.rate.1.max(1))
    }

    /// Checks the settings against `format`.
    ///
    /// # Errors
    /// A size outside [`MovieFormat::size_limits`], a frame rate outside
    /// 1 to 240 a second, or a sound track the format cannot carry or
    /// with an unusable rate or channel count.
    pub fn check(&self, format: MovieFormat) -> Result<(), MovieError> {
        let (least, most) = format.size_limits();
        let (width, height) = self.encoded_size(format);
        if !(least..=most).contains(&width) || !(least..=most).contains(&height) {
            return Err(MovieError::BadSize);
        }
        if format == MovieFormat::Mp4H264
            && width.div_ceil(16) * height.div_ceil(16) > MP4_MOST_BLOCKS
        {
            return Err(MovieError::BadSize);
        }
        let fps = self.fps();
        if self.rate.0 == 0 || self.rate.1 == 0 || !(1.0..=240.0).contains(&fps) {
            return Err(MovieError::BadRate);
        }
        if let Some(sound) = self.sound {
            if !format.carries_sound() {
                return Err(MovieError::NoSoundTrack);
            }
            if !(1..=2).contains(&sound.channels) || !format.sound_rates().contains(&sound.rate) {
                return Err(MovieError::BadSound);
            }
        }
        Ok(())
    }
}

/// Why a movie could not be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MovieError {
    /// The system has no encoder for the format (Windows N without the
    /// Media Feature Pack, or a system Sue has no MP4 route on yet).
    NoEncoder,
    /// Sound was given to a format without a sound track.
    NoSoundTrack,
    /// A frame is not the size the movie was begun with.
    WrongSize,
    /// The movie's size is outside what the format allows.
    BadSize,
    /// The frame rate is zero or out of range.
    BadRate,
    /// The sound's rate or channel count is unusable.
    BadSound,
    /// A frame is not straight 8-bit RGBA.
    NoPixels,
    /// No frame was pushed.
    NoFrames,
    /// The system encoder refused; the detail is the system's own text.
    Encoder(String),
}

impl MovieError {
    /// The English text a person reads (`io.ftl` has it), without the
    /// system's detail.
    #[must_use]
    pub fn text(&self) -> &'static str {
        match self {
            Self::NoEncoder => NO_ENCODER,
            Self::NoSoundTrack => NO_SOUND_TRACK,
            Self::WrongSize => WRONG_SIZE,
            Self::BadSize => BAD_SIZE,
            Self::BadRate => BAD_RATE,
            Self::BadSound => BAD_SOUND,
            Self::NoPixels => NO_PIXELS,
            Self::NoFrames => NO_FRAMES,
            Self::Encoder(_) => ENCODER_FAILED,
        }
    }
}

impl std::fmt::Display for MovieError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Encoder(detail) => write!(f, "{} ({detail})", self.text()),
            _ => f.write_str(self.text()),
        }
    }
}

impl std::error::Error for MovieError {}

/// A movie writer: begun with its settings (each sink's constructor),
/// fed frames and sound, then finished into the file's bytes.
///
/// Sound may come before, between or after the frames: a sink that needs
/// the two interleaved keeps what is ahead and writes it in time order.
/// Sound past the last frame's end is cut, since the frames make the
/// movie's length.
pub trait MovieSink {
    /// The format being written.
    fn format(&self) -> MovieFormat;

    /// The settings it was begun with.
    fn settings(&self) -> &MovieSettings;

    /// Adds the next frame: straight 8-bit RGBA of the settings' size.
    /// Transparent pixels are laid over white.
    ///
    /// # Errors
    /// A frame of another size or kind, or a failing encoder.
    fn push_frame(&mut self, frame: &RasterTile2D) -> Result<(), MovieError>;

    /// Adds sound: interleaved float samples in -1 to 1, at the settings'
    /// rate and channel count, following what came before.
    ///
    /// # Errors
    /// [`MovieError::NoSoundTrack`] when the movie was begun without sound
    /// or the format has none; a failing encoder.
    fn push_sound(&mut self, samples: &[f32]) -> Result<(), MovieError>;

    /// How many frames it has.
    fn frames(&self) -> usize;

    /// Ends the movie and gives the file's bytes.
    ///
    /// # Errors
    /// No frames, or a failing encoder.
    fn finish(self: Box<Self>) -> Result<Vec<u8>, MovieError>;
}

/// A picture in NV12, the layout system H.264 encoders take: the full
/// size luma plane, then one plane of interleaved Cb and Cr samples at
/// half width and half height. BT.709 colours, limited range (luma 16 to
/// 235, chroma 16 to 240), as the movie's colour tags say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nv12 {
    /// Width in pixels, even.
    pub width: u32,
    /// Height in pixels, even.
    pub height: u32,
    /// `width * height` luma bytes, then `width * height / 2` chroma bytes.
    pub bytes: Vec<u8>,
}

/// Fixed point: coefficients scaled by 2^16.
const SHIFT: u32 = 16;
const HALF: i32 = 1 << (SHIFT - 1);

/// `value * 2^16`, rounded, at compile time.
const fn fixed(value: f64) -> i32 {
    (value * 65_536.0 + 0.5) as i32
}

// BT.709: Kr = 0.2126, Kb = 0.0722, Kg = 1 - Kr - Kb. Limited range: luma
// spans 219 steps and chroma 224 steps of the 255 an RGB value spans.
const KR: f64 = 0.2126;
const KB: f64 = 0.0722;
const KG: f64 = 1.0 - KR - KB;
const Y_R: i32 = fixed(219.0 / 255.0 * KR);
const Y_G: i32 = fixed(219.0 / 255.0 * KG);
const Y_B: i32 = fixed(219.0 / 255.0 * KB);
const CB_R: i32 = fixed(-224.0 / 255.0 * KR / (2.0 * (1.0 - KB)));
const CB_G: i32 = fixed(-224.0 / 255.0 * KG / (2.0 * (1.0 - KB)));
const CB_B: i32 = fixed(224.0 / 255.0 / 2.0);
const CR_R: i32 = fixed(224.0 / 255.0 / 2.0);
const CR_G: i32 = fixed(-224.0 / 255.0 * KG / (2.0 * (1.0 - KR)));
const CR_B: i32 = fixed(-224.0 / 255.0 * KB / (2.0 * (1.0 - KR)));

/// An 8-bit value from a fixed point sum plus its offset.
fn to_byte(sum: i32, offset: i32) -> u8 {
    ((sum + HALF) >> SHIFT).saturating_add(offset).clamp(0, 255) as u8
}

/// The colour of a straight RGBA pixel laid over white, as RGB.
fn over_white(pixel: &[u8]) -> [i32; 3] {
    let alpha = i32::from(pixel[3]);
    let lay = |channel: u8| (i32::from(channel) * alpha + 255 * (255 - alpha) + 127) / 255;
    if alpha == 255 {
        [
            i32::from(pixel[0]),
            i32::from(pixel[1]),
            i32::from(pixel[2]),
        ]
    } else {
        [lay(pixel[0]), lay(pixel[1]), lay(pixel[2])]
    }
}

/// Converts straight RGBA8 pixels of `width` by `height` into NV12 of
/// `out_width` by `out_height` (even, at least the source's size, at most
/// one larger), repeating the last column and row into the padding.
/// Transparent pixels are laid over white. Each 2 by 2 block's chroma is
/// taken from the block's average colour.
///
/// # Errors
/// [`MovieError::NoPixels`] when `rgba` is not `width * height * 4` bytes;
/// [`MovieError::BadSize`] when the output size is odd, smaller than the
/// source, more than one larger, or zero.
pub fn rgba_to_nv12_into(
    rgba: &[u8],
    width: u32,
    height: u32,
    out_width: u32,
    out_height: u32,
    out: &mut Vec<u8>,
) -> Result<(), MovieError> {
    let (w, h) = (width as usize, height as usize);
    if rgba.len() != w * h * 4 {
        return Err(MovieError::NoPixels);
    }
    if width == 0
        || height == 0
        || out_width % 2 == 1
        || out_height % 2 == 1
        || !(width..=width + 1).contains(&out_width)
        || !(height..=height + 1).contains(&out_height)
    {
        return Err(MovieError::BadSize);
    }
    let (ow, oh) = (out_width as usize, out_height as usize);
    out.clear();
    out.resize(ow * oh * 3 / 2, 0);
    let pixel = |x: usize, y: usize| -> [i32; 3] {
        let at = (y.min(h - 1) * w + x.min(w - 1)) * 4;
        over_white(&rgba[at..at + 4])
    };
    let (luma, chroma) = out.split_at_mut(ow * oh);
    for (y, row) in luma.chunks_exact_mut(ow).enumerate() {
        for (x, value) in row.iter_mut().enumerate() {
            let [r, g, b] = pixel(x, y);
            *value = to_byte(Y_R * r + Y_G * g + Y_B * b, 16);
        }
    }
    for (by, row) in chroma.chunks_exact_mut(ow).enumerate() {
        for (bx, pair) in row.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            let (x, y) = (bx * 2, by * 2);
            let mut sum = [0_i32; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let rgb = pixel(x + dx, y + dy);
                for channel in 0..3 {
                    sum[channel] += rgb[channel];
                }
            }
            // The average, still times 4: the sums are divided by 4 in
            // the shift below, which keeps the rounding in one place.
            let [r, g, b] = sum;
            let quarter = |total: i32| (total + 2) >> 2;
            pair[0] = to_byte(quarter(CB_R * r + CB_G * g + CB_B * b), 128);
            pair[1] = to_byte(quarter(CR_R * r + CR_G * g + CR_B * b), 128);
        }
    }
    Ok(())
}

/// [`rgba_to_nv12_into`] for a whole tile, padded to the even size.
///
/// # Errors
/// As [`rgba_to_nv12_into`]; a tile that is not 8-bit RGBA.
pub fn rgba_to_nv12(tile: &RasterTile2D) -> Result<Nv12, MovieError> {
    if tile.format() != PixelFormat::Rgba8 {
        return Err(MovieError::NoPixels);
    }
    let bytes = tile.bytes().ok_or(MovieError::NoPixels)?;
    let (width, height) = (tile.width(), tile.height());
    let (out_width, out_height) = (width + width % 2, height + height % 2);
    let mut out = Vec::new();
    rgba_to_nv12_into(bytes, width, height, out_width, out_height, &mut out)?;
    Ok(Nv12 {
        width: out_width,
        height: out_height,
        bytes: out,
    })
}

/// A Motion JPEG AVI being written, frame by frame. It is also a
/// [`MovieSink`] (no sound track), begun with [`MjpegAvi::sink`].
#[derive(Debug)]
pub struct MjpegAvi {
    width: u32,
    height: u32,
    /// Frames a second as numerator and denominator.
    rate: (u32, u32),
    /// The JPEG quality frames are pushed at as a [`MovieSink`].
    quality: u8,
    /// The settings, when begun as a [`MovieSink`].
    settings: MovieSettings,
    /// The `movi` list's chunks so far.
    movi: Vec<u8>,
    /// Each frame's offset in `movi` (from the `movi` tag) and length.
    index: Vec<(u32, u32)>,
    largest: u32,
}

fn put(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(tag);
    put(out, body.len() as u32);
    out.extend_from_slice(body);
    if body.len() % 2 == 1 {
        out.push(0);
    }
}

fn list(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(b"LIST");
    put(out, body.len() as u32 + 4);
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
}

impl MjpegAvi {
    /// A video of `width` by `height` pictures, `fps` a second.
    #[must_use]
    pub fn new(width: u32, height: u32, fps: u32) -> Self {
        let settings = MovieSettings::new(width, height, fps.max(1));
        Self {
            width,
            height,
            rate: (fps.max(1), 1),
            quality: MovieQuality::Normal.jpeg_quality(),
            settings,
            movi: Vec::new(),
            index: Vec::new(),
            largest: 0,
        }
    }

    /// Begins a Motion JPEG AVI as a [`MovieSink`].
    ///
    /// # Errors
    /// Settings [`MovieSettings::check`] refuses for this format; a sound
    /// track, which an AVI of Sue's does not carry.
    pub fn sink(settings: MovieSettings) -> Result<Self, MovieError> {
        settings.check(MovieFormat::MjpegAvi)?;
        Ok(Self {
            rate: settings.rate,
            quality: settings.quality.jpeg_quality(),
            settings,
            ..Self::new(settings.width, settings.height, 1)
        })
    }

    /// Adds a frame already encoded as a JPEG of the video's size.
    pub fn push_jpeg(&mut self, jpeg: &[u8]) {
        // Offsets count from the `movi` tag, which is 4 bytes before.
        let offset = self.movi.len() as u32 + 4;
        chunk(&mut self.movi, b"00dc", jpeg);
        self.index.push((offset, jpeg.len() as u32));
        self.largest = self.largest.max(jpeg.len() as u32);
    }

    /// Adds `tile` as a frame, a JPEG at `quality` over white.
    ///
    /// # Errors
    /// If the tile is not the video's size or will not encode.
    pub fn push_frame(&mut self, tile: &RasterTile2D, quality: u8) -> Result<(), String> {
        if tile.width() != self.width || tile.height() != self.height {
            return Err(format!(
                "every frame is {} by {} pixels",
                self.width, self.height
            ));
        }
        let jpeg = crate::encode_as(tile, "jpg", quality, 1)?;
        self.push_jpeg(&jpeg);
        Ok(())
    }

    /// How many frames it has.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.index.len()
    }

    /// The finished file.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        let frames = self.index.len() as u32;
        let (numerator, denominator) = (self.rate.0.max(1), self.rate.1.max(1));
        // Frames a second, rounded, where the header wants a whole number.
        let fps = numerator.div_ceil(denominator).max(1);
        let micros = u64::from(denominator) * 1_000_000 / u64::from(numerator);
        let mut avih = Vec::with_capacity(56);
        put(&mut avih, u32::try_from(micros).unwrap_or(u32::MAX));
        put(&mut avih, self.largest.saturating_mul(fps));
        put(&mut avih, 0);
        put(&mut avih, 0x10); // It has an index.
        put(&mut avih, frames);
        put(&mut avih, 0);
        put(&mut avih, 1);
        put(&mut avih, self.largest);
        put(&mut avih, self.width);
        put(&mut avih, self.height);
        for _ in 0..4 {
            put(&mut avih, 0);
        }
        let mut strh = Vec::with_capacity(56);
        strh.extend_from_slice(b"vids");
        strh.extend_from_slice(b"MJPG");
        put(&mut strh, 0);
        put(&mut strh, 0); // Priority and language.
        put(&mut strh, 0);
        put(&mut strh, denominator); // Scale over rate is the frame time.
        put(&mut strh, numerator);
        put(&mut strh, 0);
        put(&mut strh, frames);
        put(&mut strh, self.largest);
        put(&mut strh, u32::MAX);
        put(&mut strh, 0);
        let (w, h) = (
            self.width.min(0x7FFF) as u16,
            self.height.min(0x7FFF) as u16,
        );
        for value in [0_u16, 0, w, h] {
            strh.extend_from_slice(&value.to_le_bytes());
        }
        let mut strf = Vec::with_capacity(40);
        put(&mut strf, 40);
        put(&mut strf, self.width);
        put(&mut strf, self.height);
        strf.extend_from_slice(&1_u16.to_le_bytes());
        strf.extend_from_slice(&24_u16.to_le_bytes());
        strf.extend_from_slice(b"MJPG");
        put(
            &mut strf,
            self.width.saturating_mul(self.height).saturating_mul(3),
        );
        for _ in 0..4 {
            put(&mut strf, 0);
        }
        let mut strl = Vec::new();
        chunk(&mut strl, b"strh", &strh);
        chunk(&mut strl, b"strf", &strf);
        let mut hdrl = Vec::new();
        chunk(&mut hdrl, b"avih", &avih);
        list(&mut hdrl, b"strl", &strl);
        let mut idx1 = Vec::with_capacity(self.index.len() * 16);
        for (offset, length) in &self.index {
            idx1.extend_from_slice(b"00dc");
            put(&mut idx1, 0x10); // A key frame.
            put(&mut idx1, *offset);
            put(&mut idx1, *length);
        }
        let mut body = Vec::with_capacity(self.movi.len() + idx1.len() + 512);
        body.extend_from_slice(b"AVI ");
        list(&mut body, b"hdrl", &hdrl);
        list(&mut body, b"movi", &self.movi);
        chunk(&mut body, b"idx1", &idx1);
        let mut out = Vec::with_capacity(body.len() + 8);
        out.extend_from_slice(b"RIFF");
        put(&mut out, body.len() as u32);
        out.extend_from_slice(&body);
        out
    }
}

impl MovieSink for MjpegAvi {
    fn format(&self) -> MovieFormat {
        MovieFormat::MjpegAvi
    }

    fn settings(&self) -> &MovieSettings {
        &self.settings
    }

    fn push_frame(&mut self, frame: &RasterTile2D) -> Result<(), MovieError> {
        if frame.width() != self.width || frame.height() != self.height {
            return Err(MovieError::WrongSize);
        }
        if frame.format() != PixelFormat::Rgba8 || frame.bytes().is_none() {
            return Err(MovieError::NoPixels);
        }
        MjpegAvi::push_frame(self, frame, self.quality).map_err(MovieError::Encoder)
    }

    fn push_sound(&mut self, _samples: &[f32]) -> Result<(), MovieError> {
        Err(MovieError::NoSoundTrack)
    }

    fn frames(&self) -> usize {
        self.index.len()
    }

    fn finish(self: Box<Self>) -> Result<Vec<u8>, MovieError> {
        if self.index.is_empty() {
            return Err(MovieError::NoFrames);
        }
        Ok(MjpegAvi::finish(*self))
    }
}

/// One top-level box of an MP4: where it starts and how long it is.
struct Mp4Box {
    kind: [u8; 4],
    start: usize,
    len: usize,
}

/// The boxes laid one after another in `bytes`, or `None` if they do not
/// tile it exactly.
fn mp4_boxes(bytes: &[u8]) -> Option<Vec<Mp4Box>> {
    let mut boxes = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let head = bytes.get(at..at + 8)?;
        let size = u32::from_be_bytes([head[0], head[1], head[2], head[3]]);
        let kind = [head[4], head[5], head[6], head[7]];
        let len = match size {
            0 => bytes.len() - at,
            1 => {
                let large = bytes.get(at + 8..at + 16)?;
                usize::try_from(u64::from_be_bytes(large.try_into().ok()?)).ok()?
            }
            size => size as usize,
        };
        if len < 8 || at.checked_add(len)? > bytes.len() {
            return None;
        }
        boxes.push(Mp4Box {
            kind,
            start: at,
            len,
        });
        at += len;
    }
    Some(boxes)
}

/// Moves every chunk offset (`stco`, `co64`) found under the container
/// boxes of `body` through `moved`.
fn mp4_shift_offsets(
    body: &mut [u8],
    moved: &dyn Fn(u64) -> Option<u64>,
) -> Result<(), MovieError> {
    let broken = || MovieError::Encoder("the MP4 index cannot be read".to_owned());
    let children = mp4_boxes(body).ok_or_else(broken)?;
    for child in children {
        let inner = child.start + 8..child.start + child.len;
        match &child.kind {
            b"trak" | b"mdia" | b"minf" | b"stbl" => {
                mp4_shift_offsets(&mut body[inner], moved)?;
            }
            b"stco" | b"co64" => {
                let wide = &child.kind == b"co64";
                let table = &mut body[inner];
                let count = table
                    .get(4..8)
                    .map(|count| u32::from_be_bytes([count[0], count[1], count[2], count[3]]))
                    .ok_or_else(broken)? as usize;
                let size = if wide { 8 } else { 4 };
                let entries = table.get_mut(8..8 + count * size).ok_or_else(broken)?;
                for entry in entries.chunks_exact_mut(size) {
                    let old = if wide {
                        u64::from_be_bytes(entry.try_into().map_err(|_| broken())?)
                    } else {
                        u64::from(u32::from_be_bytes(entry.try_into().map_err(|_| broken())?))
                    };
                    let new = moved(old).ok_or_else(broken)?;
                    if wide {
                        entry.copy_from_slice(&new.to_be_bytes());
                    } else {
                        let new = u32::try_from(new).map_err(|_| {
                            MovieError::Encoder(
                                "the MP4 is too large to put its index first".to_owned(),
                            )
                        })?;
                        entry.copy_from_slice(&new.to_be_bytes());
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// The MP4 `bytes` with its index (`moov`) moved in front of the picture
/// and sound data (`mdat`), so the file starts playing while it loads and
/// sites take it as it is ("fast start"). Every chunk offset is moved
/// with the data it points at. A file whose index is already first comes
/// back unchanged.
///
/// # Errors
/// When the boxes cannot be read, an offset points outside every box, or
/// a 32-bit offset table would overflow (a file of nearly 4 GB); the file
/// as given is still a valid MP4 then.
pub fn mp4_index_first(bytes: &[u8]) -> Result<Vec<u8>, MovieError> {
    let broken = || MovieError::Encoder("the MP4 boxes cannot be read".to_owned());
    let boxes = mp4_boxes(bytes).ok_or_else(broken)?;
    let moov = boxes
        .iter()
        .position(|found| &found.kind == b"moov")
        .ok_or_else(broken)?;
    let Some(first_data) = boxes.iter().position(|found| &found.kind == b"mdat") else {
        return Ok(bytes.to_vec());
    };
    if moov < first_data {
        return Ok(bytes.to_vec());
    }
    // The new order: what came before the data, the index, then the rest.
    let mut order: Vec<usize> = (0..first_data).collect();
    order.push(moov);
    order.extend((first_data..boxes.len()).filter(|index| *index != moov));
    // Where each box lands.
    let mut landing = vec![0_u64; boxes.len()];
    let mut at = 0_u64;
    for index in &order {
        landing[*index] = at;
        at += boxes[*index].len as u64;
    }
    let moved = |offset: u64| -> Option<u64> {
        let (index, found) = boxes.iter().enumerate().find(|(_, found)| {
            (found.start as u64..(found.start + found.len) as u64).contains(&offset)
        })?;
        Some(landing[index] + (offset - found.start as u64))
    };
    let index = &boxes[moov];
    let mut index_bytes = bytes[index.start..index.start + index.len].to_vec();
    let header = if u32::from_be_bytes([
        index_bytes[0],
        index_bytes[1],
        index_bytes[2],
        index_bytes[3],
    ]) == 1
    {
        16
    } else {
        8
    };
    mp4_shift_offsets(&mut index_bytes[header..], &moved)?;
    let mut out = Vec::with_capacity(bytes.len());
    for position in order {
        if position == moov {
            out.extend_from_slice(&index_bytes);
        } else {
            let found = &boxes[position];
            out.extend_from_slice(&bytes[found.start..found.start + found.len]);
        }
    }
    Ok(out)
}

/// The most frames [`decode_frames`] reads from one file.
pub const MAX_FRAMES: usize = 3_000;

/// The frames of a movie or an animated picture, each a straight RGBA8
/// tile, and its frames per second: animated GIF, animated PNG, animated
/// WebP and Motion JPEG AVI (as Sue writes it). At most [`MAX_FRAMES`].
///
/// # Errors
/// If the file is none of these or cannot be read.
pub fn decode_frames(bytes: &[u8]) -> Result<(Vec<RasterTile2D>, f64), String> {
    use image::AnimationDecoder;
    use std::io::Cursor;
    let collect = |frames: image::Frames<'_>| -> Result<(Vec<RasterTile2D>, f64), String> {
        let mut tiles = Vec::new();
        let mut total_ms = 0.0;
        for frame in frames.take(MAX_FRAMES) {
            let frame = frame.map_err(|error| error.to_string())?;
            let (numer, denom) = frame.delay().numer_denom_ms();
            total_ms += f64::from(numer) / f64::from(denom.max(1));
            let buffer = frame.into_buffer();
            let (width, height) = buffer.dimensions();
            tiles.push(
                RasterTile2D::from_bytes(
                    width,
                    height,
                    sue_core::PixelFormat::Rgba8,
                    buffer.into_raw(),
                )
                .map_err(|error| error.to_string())?,
            );
        }
        if tiles.is_empty() {
            return Err("the file has no frames".to_owned());
        }
        let each = total_ms / tiles.len() as f64;
        let fps = if each > 0.0 {
            (1000.0 / each).clamp(1.0, 120.0)
        } else {
            12.0
        };
        Ok((tiles, fps))
    };
    if bytes.starts_with(b"GIF8") {
        let decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
            .map_err(|error| error.to_string())?;
        return collect(decoder.into_frames());
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        let decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes))
            .map_err(|error| error.to_string())?;
        if decoder.is_apng().map_err(|error| error.to_string())? {
            let frames = decoder
                .apng()
                .map_err(|error| error.to_string())?
                .into_frames();
            return collect(frames);
        }
        let tile = crate::decode_png(bytes)?.tile;
        return Ok((vec![tile], 12.0));
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        let decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))
            .map_err(|error| error.to_string())?;
        return collect(decoder.into_frames());
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"AVI ") {
        return avi_frames(bytes);
    }
    Err("Sue reads movies as animated GIF, PNG or WebP, or Motion JPEG AVI".to_owned())
}

/// The JPEG frames of a Motion JPEG AVI and its frame rate.
fn avi_frames(bytes: &[u8]) -> Result<(Vec<RasterTile2D>, f64), String> {
    let read = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let mut fps = 12.0;
    if let Some(avih) = bytes.windows(4).position(|tag| tag == b"avih")
        && let Some(micros) = read(avih + 8).filter(|micros| *micros > 0)
    {
        fps = (1_000_000.0 / f64::from(micros)).clamp(1.0, 120.0);
    }
    let movi = bytes
        .windows(4)
        .position(|tag| tag == b"movi")
        .ok_or("the AVI has no frames")?;
    let mut at = movi + 4;
    let mut tiles = Vec::new();
    while at + 8 <= bytes.len() && tiles.len() < MAX_FRAMES {
        let tag = &bytes[at..at + 4];
        if tag == b"idx1" {
            break;
        }
        let size = read(at + 4).ok_or("the AVI is cut short")? as usize;
        let body = bytes
            .get(at + 8..at + 8 + size)
            .ok_or("the AVI is cut short")?;
        if tag == b"LIST" {
            // A list of chunks: step inside it.
            at += 12;
            continue;
        }
        if tag[2..] == *b"dc" || tag[2..] == *b"db" {
            if body.starts_with(&[0xFF, 0xD8]) {
                tiles.push(crate::decode_jpeg(body)?.tile);
            } else {
                return Err("only Motion JPEG AVI movies are read".to_owned());
            }
        }
        at += 8 + size + size % 2;
    }
    if tiles.is_empty() {
        return Err("the AVI has no frames".to_owned());
    }
    Ok((tiles, fps))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }

    #[test]
    fn movies_and_animations_read_back_frame_by_frame() {
        let frame = |shade: u8| {
            RasterTile2D::from_bytes(
                8,
                6,
                PixelFormat::Rgba8,
                [shade, shade, shade, 255].repeat(48),
            )
            .unwrap()
        };
        let mut video = MjpegAvi::new(8, 6, 10);
        for shade in [0_u8, 255] {
            video.push_frame(&frame(shade), 95).unwrap();
        }
        let (frames, fps) = decode_frames(&video.finish()).unwrap();
        assert_eq!(frames.len(), 2);
        assert!((fps - 10.0).abs() < 0.01);
        assert!(frames[1].bytes().unwrap()[0] > 200, "the white frame");
        let gif = crate::encode_gif_animation(&[frame(0), frame(255), frame(0)], 50).unwrap();
        let (frames, fps) = decode_frames(&gif).unwrap();
        assert_eq!(frames.len(), 3);
        assert!((fps - 20.0).abs() < 0.5);
        assert!(decode_frames(b"not a movie").is_err());
    }

    #[test]
    fn frames_become_an_indexed_motion_jpeg_avi() {
        let mut video = MjpegAvi::new(32, 16, 12);
        for shade in [0_u8, 128, 255] {
            let tile = RasterTile2D::from_bytes(
                32,
                16,
                PixelFormat::Rgba8,
                [shade, shade, shade, 255].repeat(32 * 16),
            )
            .unwrap();
            video.push_frame(&tile, 80).unwrap();
        }
        let wrong = RasterTile2D::new(8, 8, PixelFormat::Rgba8).unwrap();
        assert!(
            video.push_frame(&wrong, 80).is_err(),
            "every frame the same size"
        );
        assert_eq!(video.frames(), 3);
        let bytes = video.finish();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(read(&bytes, 4) as usize, bytes.len() - 8);
        assert_eq!(&bytes[8..12], b"AVI ");
        // The main header: frame time, frame count, size.
        let avih = bytes.windows(4).position(|tag| tag == b"avih").unwrap() + 8;
        assert_eq!(read(&bytes, avih), 1_000_000 / 12);
        assert_eq!(read(&bytes, avih + 16), 3);
        assert_eq!((read(&bytes, avih + 32), read(&bytes, avih + 36)), (32, 16));
        // The index points at each frame, a JPEG.
        let movi = bytes.windows(4).position(|tag| tag == b"movi").unwrap();
        let idx1 = bytes.windows(4).rposition(|tag| tag == b"idx1").unwrap();
        assert_eq!(read(&bytes, idx1 + 4), 3 * 16);
        for entry in 0..3 {
            let at = idx1 + 8 + entry * 16;
            assert_eq!(&bytes[at..at + 4], b"00dc");
            let frame = movi + read(&bytes, at + 8) as usize;
            assert_eq!(&bytes[frame..frame + 4], b"00dc");
            assert_eq!(
                &bytes[frame + 8..frame + 10],
                [0xFF, 0xD8],
                "a JPEG starts there"
            );
            let decoded =
                crate::decode_jpeg(&bytes[frame + 8..frame + 8 + read(&bytes, at + 12) as usize])
                    .unwrap();
            assert_eq!((decoded.tile.width(), decoded.tile.height()), (32, 16));
        }
    }

    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> RasterTile2D {
        RasterTile2D::from_bytes(
            width,
            height,
            PixelFormat::Rgba8,
            rgba.repeat((width * height) as usize),
        )
        .unwrap()
    }

    /// The luma, Cb and Cr of a solid picture.
    fn yuv(rgba: [u8; 4]) -> (u8, u8, u8) {
        let nv12 = rgba_to_nv12(&solid(2, 2, rgba)).unwrap();
        assert_eq!(nv12.bytes.len(), 6);
        assert!(nv12.bytes[..4].iter().all(|luma| *luma == nv12.bytes[0]));
        (nv12.bytes[0], nv12.bytes[4], nv12.bytes[5])
    }

    #[test]
    fn nv12_matches_bt709_limited_range_computed_by_hand() {
        // Y = 16 + 219 (0.2126 R + 0.7152 G + 0.0722 B) / 255,
        // Cb = 128 + 224 (B - Y') / (1.8556 * 255),
        // Cr = 128 + 224 (R - Y') / (1.5748 * 255), Y' the unscaled sum.
        assert_eq!(yuv([255, 255, 255, 255]), (235, 128, 128), "white");
        assert_eq!(yuv([0, 0, 0, 255]), (16, 128, 128), "black");
        // Red: 16 + 46.56; 128 - 25.66; 128 + 112.
        assert_eq!(yuv([255, 0, 0, 255]), (63, 102, 240), "red");
        // Green: 16 + 156.63; 128 - 86.34; 128 - 101.73.
        assert_eq!(yuv([0, 255, 0, 255]), (173, 42, 26), "green");
        // Blue: 16 + 15.81; 128 + 112; 128 - 10.27.
        assert_eq!(yuv([0, 0, 255, 255]), (32, 240, 118), "blue");
        // Mid grey 128: 16 + 109.93.
        assert_eq!(yuv([128, 128, 128, 255]), (126, 128, 128), "grey");
        // Half transparent black over white is grey 128 (127.5 rounds up).
        assert_eq!(yuv([0, 0, 0, 128]), yuv([127, 127, 127, 255]), "over white");
        assert_eq!(yuv([9, 99, 199, 0]), (235, 128, 128), "clear is white");
    }

    #[test]
    fn nv12_planes_are_laid_out_and_odd_sizes_padded_with_the_edge() {
        // 3 by 3: a red column, a green column, a blue column; the padded
        // fourth column repeats the blue and the fourth row the third.
        let mut pixels = Vec::new();
        for _ in 0..3 {
            for color in [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]] {
                pixels.extend_from_slice(&color);
            }
        }
        let tile = RasterTile2D::from_bytes(3, 3, PixelFormat::Rgba8, pixels).unwrap();
        let nv12 = rgba_to_nv12(&tile).unwrap();
        assert_eq!((nv12.width, nv12.height), (4, 4));
        assert_eq!(nv12.bytes.len(), 16 + 8);
        for row in nv12.bytes[..16].chunks(4) {
            assert_eq!(row, [63, 173, 32, 32], "luma per column");
        }
        // The left block averages red and green (127.5, 127.5, 0); the
        // right block is all blue.
        let chroma = &nv12.bytes[16..];
        assert_eq!(chroma[..4], chroma[4..], "both chroma rows alike");
        assert_eq!(&chroma[2..4], [240, 118], "blue block");
        // (128,128,0) by hand: Cb 128 - 224 * 0.9278 * 128 / (1.8556 * 255)
        // = 71.8; Cr 128 + 224 * 0.0722 * 128 / (1.5748 * 255) = 133.2.
        let (cb, cr) = (chroma[0], chroma[1]);
        assert!((71..=72).contains(&cb), "{cb}");
        assert!((133..=134).contains(&cr), "{cr}");
        // Wrong lengths and sizes are refused, never a panic.
        let mut out = Vec::new();
        assert_eq!(
            rgba_to_nv12_into(&[0; 15], 2, 2, 2, 2, &mut out),
            Err(MovieError::NoPixels)
        );
        assert_eq!(
            rgba_to_nv12_into(&[0; 16], 2, 2, 3, 2, &mut out),
            Err(MovieError::BadSize)
        );
        assert_eq!(
            rgba_to_nv12_into(&[0; 16], 2, 2, 4, 2, &mut out),
            Err(MovieError::BadSize)
        );
        assert_eq!(
            rgba_to_nv12_into(&[], 0, 0, 0, 0, &mut out),
            Err(MovieError::BadSize)
        );
        let float = RasterTile2D::new(2, 2, PixelFormat::RgbaF32).unwrap();
        assert_eq!(rgba_to_nv12(&float), Err(MovieError::NoPixels));
    }

    #[test]
    fn movie_settings_pad_time_and_check() {
        let mut settings = MovieSettings::new(1921, 1081, 24);
        assert_eq!(settings.encoded_size(MovieFormat::Mp4H264), (1922, 1082));
        assert_eq!(settings.encoded_size(MovieFormat::MjpegAvi), (1921, 1081));
        assert!(settings.is_padded(MovieFormat::Mp4H264));
        assert!(!settings.is_padded(MovieFormat::MjpegAvi));
        assert_eq!(settings.frame_time(24), TICKS_PER_SECOND);
        assert_eq!(settings.frame_time(1), 416_666);
        settings.rate = (30_000, 1001);
        assert_eq!(settings.frame_time(1), 333_666);
        assert_eq!(settings.frame_time(30_000), 1001 * TICKS_PER_SECOND);
        assert!(settings.check(MovieFormat::Mp4H264).is_ok());
        settings.rate = (0, 1);
        assert_eq!(
            settings.check(MovieFormat::Mp4H264),
            Err(MovieError::BadRate)
        );
        settings.rate = (24, 1);
        settings.sound = Some(MovieSound {
            rate: 48_000,
            channels: 2,
        });
        assert!(settings.check(MovieFormat::Mp4H264).is_ok());
        assert_eq!(
            settings.check(MovieFormat::MjpegAvi),
            Err(MovieError::NoSoundTrack)
        );
        settings.sound = Some(MovieSound {
            rate: 48_000,
            channels: 6,
        });
        assert_eq!(
            settings.check(MovieFormat::Mp4H264),
            Err(MovieError::BadSound)
        );
        settings.sound = Some(MovieSound {
            rate: 22_050,
            channels: 2,
        });
        assert_eq!(
            settings.check(MovieFormat::Mp4H264),
            Err(MovieError::BadSound)
        );
        for (width, height) in [(4096, 4096), (7680, 4320), (4098, 1000)] {
            let huge = MovieSettings::new(width, height, 24);
            assert_eq!(huge.check(MovieFormat::Mp4H264), Err(MovieError::BadSize));
        }
        for (width, height) in [(3840, 2160), (4096, 2304), (2304, 4096), (34, 34)] {
            let fits = MovieSettings::new(width, height, 24);
            assert!(
                fits.check(MovieFormat::Mp4H264).is_ok(),
                "{width} by {height}"
            );
        }
        let tiny = MovieSettings::new(16, 16, 24);
        assert_eq!(tiny.check(MovieFormat::Mp4H264), Err(MovieError::BadSize));
        assert!(tiny.check(MovieFormat::MjpegAvi).is_ok());
    }

    #[test]
    fn format_and_quality_names_are_stable() {
        // Saved in settings and presets: never renamed (rule 4).
        let ids: Vec<_> = MovieFormat::ALL.iter().map(|format| format.id()).collect();
        assert_eq!(ids, ["mp4-h264", "avi-mjpeg"]);
        let ids: Vec<_> = MovieQuality::ALL
            .iter()
            .map(|quality| quality.id())
            .collect();
        assert_eq!(ids, ["small", "normal", "high"]);
        for format in MovieFormat::ALL {
            assert_eq!(MovieFormat::from_id(format.id()), Some(format));
        }
        for quality in MovieQuality::ALL {
            assert_eq!(MovieQuality::from_id(quality.id()), Some(quality));
        }
        assert_eq!(MovieFormat::from_id("mov"), None);
        assert_eq!(MovieFormat::Mp4H264.extension(), "mp4");
        assert!(MovieFormat::Mp4H264.carries_sound());
        assert!(!MovieFormat::MjpegAvi.carries_sound());
        // Higher quality never squeezes harder.
        for pair in MovieQuality::ALL.windows(2) {
            assert!(pair[0].h264_quality() < pair[1].h264_quality());
            assert!(pair[0].jpeg_quality() < pair[1].jpeg_quality());
        }
    }

    #[test]
    fn motion_jpeg_avi_is_one_movie_sink() {
        let mut settings = MovieSettings::new(16, 8, 12);
        settings.rate = (25, 2);
        let mut sink: Box<dyn MovieSink> = Box::new(MjpegAvi::sink(settings).unwrap());
        assert_eq!(sink.format(), MovieFormat::MjpegAvi);
        assert_eq!(sink.settings(), &settings);
        assert_eq!(
            sink.push_frame(&solid(8, 8, [0, 0, 0, 255])),
            Err(MovieError::WrongSize)
        );
        assert_eq!(sink.push_sound(&[0.0; 4]), Err(MovieError::NoSoundTrack));
        for shade in [0_u8, 255] {
            sink.push_frame(&solid(16, 8, [shade, shade, shade, 255]))
                .unwrap();
        }
        assert_eq!(sink.frames(), 2);
        let bytes = sink.finish().unwrap();
        let (frames, fps) = decode_frames(&bytes).unwrap();
        assert_eq!(frames.len(), 2);
        assert!((fps - 12.5).abs() < 0.01, "{fps}");
        let empty: Box<dyn MovieSink> = Box::new(MjpegAvi::sink(settings).unwrap());
        assert_eq!(empty.finish(), Err(MovieError::NoFrames));
        settings.sound = Some(MovieSound {
            rate: 48_000,
            channels: 2,
        });
        assert_eq!(
            MjpegAvi::sink(settings).err(),
            Some(MovieError::NoSoundTrack)
        );
        // The sink writes what the plain writer writes.
        let mut plain = MjpegAvi::new(16, 8, 12);
        plain
            .push_frame(
                &solid(16, 8, [9, 9, 9, 255]),
                MovieQuality::Normal.jpeg_quality(),
            )
            .unwrap();
        let mut sink: Box<dyn MovieSink> =
            Box::new(MjpegAvi::sink(MovieSettings::new(16, 8, 12)).unwrap());
        sink.push_frame(&solid(16, 8, [9, 9, 9, 255])).unwrap();
        assert_eq!(sink.finish().unwrap(), plain.finish());
    }

    fn mp4_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    /// An MP4 shaped like Media Foundation's: ftyp, mdat, then moov whose
    /// one track's chunk offsets point at the two chunks in mdat.
    fn index_last() -> (Vec<u8>, [&'static [u8]; 2]) {
        let chunks: [&[u8]; 2] = [b"FIRST-CHUNK", b"SECOND"];
        let ftyp = mp4_box(b"ftyp", b"isom\0\0\0\0isomavc1");
        let mdat_start = ftyp.len() + 8;
        let offsets = [mdat_start, mdat_start + chunks[0].len()];
        let mut stco = vec![0, 0, 0, 0, 0, 0, 0, 2];
        for offset in offsets {
            stco.extend_from_slice(&(offset as u32).to_be_bytes());
        }
        let stbl = mp4_box(b"stbl", &mp4_box(b"stco", &stco));
        let trak = mp4_box(b"trak", &mp4_box(b"mdia", &mp4_box(b"minf", &stbl)));
        let moov = mp4_box(b"moov", &[mp4_box(b"mvhd", &[0; 12]), trak].concat());
        let mdat = mp4_box(b"mdat", &chunks.concat());
        ([ftyp, mdat, moov].concat(), chunks)
    }

    #[test]
    fn the_mp4_index_moves_first_with_its_offsets() {
        let (file, chunks) = index_last();
        let moved = mp4_index_first(&file).unwrap();
        assert_eq!(moved.len(), file.len());
        let tags: Vec<_> = mp4_boxes(&moved)
            .unwrap()
            .iter()
            .map(|found| found.kind)
            .collect();
        assert_eq!(tags, [*b"ftyp", *b"moov", *b"mdat"]);
        // Each offset still points at its chunk.
        let stco = moved.windows(4).position(|tag| tag == b"stco").unwrap() + 4;
        for (entry, chunk) in chunks.iter().enumerate() {
            let at = stco + 8 + entry * 4;
            let offset = u32::from_be_bytes(moved[at..at + 4].try_into().unwrap()) as usize;
            assert_eq!(&moved[offset..offset + chunk.len()], *chunk);
        }
        // Already first: unchanged. Moving twice changes nothing more.
        assert_eq!(mp4_index_first(&moved).unwrap(), moved);
        // Not an MP4, or cut short: an error, never a panic.
        assert!(mp4_index_first(b"not an mp4 file").is_err());
        assert!(mp4_index_first(&file[..file.len() - 3]).is_err());
        let no_index = [mp4_box(b"ftyp", b"isom"), mp4_box(b"mdat", b"x")].concat();
        assert!(mp4_index_first(&no_index).is_err());
    }

    #[test]
    fn every_movie_text_is_in_the_language_database() {
        let mut texts = vec![PADDED_NOTE];
        texts.extend(MovieFormat::ALL.iter().map(|format| format.name()));
        texts.extend(MovieQuality::ALL.iter().map(|quality| quality.name()));
        for error in [
            MovieError::NoEncoder,
            MovieError::NoSoundTrack,
            MovieError::WrongSize,
            MovieError::BadSize,
            MovieError::BadRate,
            MovieError::BadSound,
            MovieError::NoPixels,
            MovieError::NoFrames,
            MovieError::Encoder(String::new()),
        ] {
            texts.push(error.text());
        }
        for text in texts {
            assert!(sue_lang::has_english("io", text), "{text}");
        }
    }
}
