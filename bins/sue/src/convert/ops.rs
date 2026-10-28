//! What the Converter does to a file: reads it as a picture, a sound, text
//! or plain bytes, changes it as the options say, and writes it in the
//! chosen format. Pure functions of their inputs, so they run on a
//! background thread and are tested on their own.

use std::fmt::Write as _;
use std::path::Path;

use sue_core::{PCMFloat1D, PixelFormat, RasterTile2D};
use sue_io::{
    JxlError, JxlPipeline, NativeJxlPipeline, PictureFormat, ProgressiveTarget, SampleDepth,
    decode_audio, decode_jpeg, decode_picture, decode_png, encode_aiff, encode_flac, encode_jpeg,
    encode_picture, encode_png, encode_wav_as,
};

/// What a file is, for the Converter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// A picture.
    Picture,
    /// A sound.
    Sound,
    /// Text.
    Text,
    /// Anything else, as bytes.
    Data,
}

impl Kind {
    /// The kinds in the order the Converter shows them.
    pub const ALL: [Self; 4] = [Self::Picture, Self::Sound, Self::Text, Self::Data];

    /// How the Converter names files of this kind.
    #[must_use]
    pub fn plural(self) -> &'static str {
        match self {
            Self::Picture => "Pictures",
            Self::Sound => "Sounds",
            Self::Text => "Text files",
            Self::Data => "Other files",
        }
    }
}

const TEXT: [&str; 24] = [
    "txt", "md", "markdown", "csv", "tsv", "json", "html", "htm", "xml", "rs", "py", "js", "ts",
    "css", "toml", "yaml", "yml", "ini", "log", "suerice", "sh", "c", "cpp", "h",
];

fn extension(path: &Path) -> String {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// What `path` (with its first `bytes`) is.
#[must_use]
pub fn kind_of(path: &Path, bytes: &[u8]) -> Kind {
    let extension = extension(path);
    if matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "jxl")
        || PictureFormat::from_extension(&extension).is_some()
    {
        return Kind::Picture;
    }
    if crate::player::library::is_audio(path) || extension == "aif" {
        return Kind::Sound;
    }
    if TEXT.contains(&extension.as_str())
        || (!bytes.is_empty() && !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok())
    {
        return Kind::Text;
    }
    Kind::Data
}

/// A format the Converter writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    /// PNG.
    Png,
    /// JPEG.
    Jpeg,
    /// Lossless JPEG XL.
    Jxl,
    /// One of the `image` formats.
    Picture(PictureFormat),
    /// WAV.
    Wav,
    /// FLAC.
    Flac,
    /// AIFF.
    Aiff,
    /// The same text, tidied.
    Text,
    /// Markdown as an HTML page.
    Html,
    /// A table as comma-separated values.
    Csv,
    /// A table as tab-separated values.
    Tsv,
    /// A table (or JSON) as JSON.
    Json,
    /// The bytes as Base64 text.
    Base64,
    /// The bytes as a hex dump.
    Hex,
    /// The bytes as a gray picture, one pixel per byte.
    ByteMap,
    /// The bytes as they are.
    Copy,
}

impl Target {
    /// What a target is called in the dropdown.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Png => "PNG picture".to_owned(),
            Self::Jpeg => "JPEG picture".to_owned(),
            Self::Jxl => "JPEG XL picture (lossless)".to_owned(),
            Self::Picture(format) => format!("{} picture", format.name()),
            Self::Wav => "WAV sound".to_owned(),
            Self::Flac => "FLAC sound (lossless)".to_owned(),
            Self::Aiff => "AIFF sound".to_owned(),
            Self::Text => "Text (tidied)".to_owned(),
            Self::Html => "HTML page (from Markdown)".to_owned(),
            Self::Csv => "CSV table".to_owned(),
            Self::Tsv => "TSV table".to_owned(),
            Self::Json => "JSON".to_owned(),
            Self::Base64 => "Base64 text".to_owned(),
            Self::Hex => "Hex dump".to_owned(),
            Self::ByteMap => "Byte map picture (PNG)".to_owned(),
            Self::Copy => "A copy, unchanged".to_owned(),
        }
    }

    /// The written file's extension.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png | Self::ByteMap => "png",
            Self::Jpeg => "jpg",
            Self::Jxl => "jxl",
            Self::Picture(format) => format.extension(),
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Aiff => "aiff",
            Self::Text | Self::Base64 | Self::Hex => "txt",
            Self::Html => "html",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Json => "json",
            Self::Copy => "",
        }
    }

    /// Every target files of `kind` can become, the usual one first.
    #[must_use]
    pub fn for_kind(kind: Kind) -> Vec<Self> {
        let data = [Self::Base64, Self::Hex, Self::ByteMap, Self::Copy];
        let mut targets = match kind {
            Kind::Picture => {
                let mut targets = vec![Self::Png, Self::Jpeg, Self::Jxl];
                targets.extend(PictureFormat::ALL.map(Self::Picture));
                targets
            }
            Kind::Sound => vec![Self::Flac, Self::Wav, Self::Aiff],
            Kind::Text => vec![Self::Text, Self::Html, Self::Json, Self::Csv, Self::Tsv],
            Kind::Data => Vec::new(),
        };
        targets.extend(data);
        targets
    }

    /// `true` for picture outputs that drop transparency.
    #[must_use]
    pub fn flattens(self) -> bool {
        matches!(self, Self::Jpeg | Self::Picture(PictureFormat::Pnm))
    }
}

/// How a picture is resized.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Resize {
    /// Not at all.
    Keep,
    /// By a percentage.
    Scale(f32),
    /// To a width, keeping the shape.
    Width(u32),
    /// To a height, keeping the shape.
    Height(u32),
    /// To fit inside a box, keeping the shape.
    Fit(u32, u32),
    /// To exactly this size.
    Exact(u32, u32),
}

/// How pixels are resampled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    /// The nearest pixel: crisp pixel art.
    Nearest,
    /// A straight blend of neighbors.
    Bilinear,
    /// Catmull-Rom: sharper.
    Bicubic,
    /// Lanczos: sharpest, for photos.
    Lanczos,
}

impl Filter {
    /// Every filter, with its name.
    pub const ALL: [(Self, &'static str); 4] = [
        (Self::Lanczos, "Lanczos (sharpest)"),
        (Self::Bicubic, "Bicubic"),
        (Self::Bilinear, "Bilinear (soft)"),
        (Self::Nearest, "Nearest (pixel art)"),
    ];

    fn support(self) -> f32 {
        match self {
            Self::Nearest => 0.5,
            Self::Bilinear => 1.0,
            Self::Bicubic => 2.0,
            Self::Lanczos => 3.0,
        }
    }

    fn weight(self, x: f32) -> f32 {
        let x = x.abs();
        match self {
            Self::Nearest => f32::from(x < 0.5),
            Self::Bilinear => (1.0 - x).max(0.0),
            Self::Bicubic => {
                if x < 1.0 {
                    1.5 * x * x * x - 2.5 * x * x + 1.0
                } else if x < 2.0 {
                    -0.5 * x * x * x + 2.5 * x * x - 4.0 * x + 2.0
                } else {
                    0.0
                }
            }
            Self::Lanczos => {
                if x < 1e-6 {
                    1.0
                } else if x < 3.0 {
                    let pi = std::f32::consts::PI * x;
                    3.0 * pi.sin() * (pi / 3.0).sin() / (pi * pi)
                } else {
                    0.0
                }
            }
        }
    }
}

/// Everything the Converter can change about a picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PictureOptions {
    /// Resizing.
    pub resize: Resize,
    /// Resampling.
    pub filter: Filter,
    /// Clockwise quarter turns.
    pub quarter_turns: u8,
    /// Mirror left to right.
    pub flip_h: bool,
    /// Mirror top to bottom.
    pub flip_v: bool,
    /// Grays only.
    pub grayscale: bool,
    /// -1 to 1.
    pub brightness: f32,
    /// -1 to 1.
    pub contrast: f32,
    /// -1 to 1.
    pub saturation: f32,
    /// Fill transparency with the background.
    pub flatten: bool,
    /// The background for flattening.
    pub background: [u8; 3],
    /// JPEG quality, 1 to 100.
    pub jpeg_quality: u8,
    /// JPEG XL effort, 1 to 9.
    pub jxl_effort: u8,
}

impl Default for PictureOptions {
    fn default() -> Self {
        Self {
            resize: Resize::Keep,
            filter: Filter::Lanczos,
            quarter_turns: 0,
            flip_h: false,
            flip_v: false,
            grayscale: false,
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            flatten: false,
            background: [255, 255, 255],
            jpeg_quality: 90,
            jxl_effort: 7,
        }
    }
}

/// Everything the Converter can change about a sound.
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
    /// Playback speed (changes the pitch too).
    pub speed: f32,
    /// Backwards.
    pub reverse: bool,
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
            speed: 1.0,
            reverse: false,
        }
    }
}

/// Line endings for written text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endings {
    /// As they are.
    Keep,
    /// `\n`, as on Linux and macOS.
    Lf,
    /// `\r\n`, as on Windows.
    CrLf,
}

/// Everything the Converter can change about text and bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct TextOptions {
    /// Line endings.
    pub endings: Endings,
    /// Drop spaces at the ends of lines.
    pub trim_trailing: bool,
    /// Replace tabs with this many spaces (0 keeps them).
    pub tab_spaces: u8,
    /// Spaces per level of JSON (0 writes it on one line).
    pub json_indent: u8,
    /// The first row of a table names its columns.
    pub header_row: bool,
    /// The HTML page's title (empty uses the file name).
    pub html_title: String,
    /// Bytes per line of a hex dump.
    pub hex_width: u8,
    /// Wrap Base64 at 76 characters.
    pub base64_wrap: bool,
    /// Pixels per row of a byte map.
    pub map_width: u32,
}

impl Default for TextOptions {
    fn default() -> Self {
        Self {
            endings: Endings::Keep,
            trim_trailing: false,
            tab_spaces: 0,
            json_indent: 2,
            header_row: true,
            html_title: String::new(),
            hex_width: 16,
            base64_wrap: true,
            map_width: 256,
        }
    }
}

/// Every option, for one conversion.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Options {
    /// For pictures.
    pub picture: PictureOptions,
    /// For sounds.
    pub sound: SoundOptions,
    /// For text and bytes.
    pub text: TextOptions,
}

// ----- Pictures ----------------------------------------------------------------

/// Reads a picture file's bytes into RGBA8 pixels.
///
/// # Errors
/// If no codec reads it.
pub fn read_picture(path: &Path, bytes: &[u8]) -> Result<RasterTile2D, String> {
    let extension = extension(path);
    let tile = match extension.as_str() {
        "png" => decode_png(bytes)?.tile,
        "jpg" | "jpeg" => decode_jpeg(bytes)?.tile,
        "jxl" => {
            NativeJxlPipeline
                .decode(bytes, ProgressiveTarget::Full)
                .map_err(|error| error.to_string())?
                .tile
        }
        _ => decode_picture(bytes, PictureFormat::from_extension(&extension))?,
    };
    rgba(&tile)
}

/// `tile` as RGBA8.
///
/// # Errors
/// If it holds float pixels.
pub fn rgba(tile: &RasterTile2D) -> Result<RasterTile2D, String> {
    let bytes = tile.bytes().ok_or("the picture has no pixels")?;
    let pixels = match tile.format() {
        PixelFormat::Rgba8 => return Ok(tile.clone()),
        PixelFormat::Gray8 => bytes
            .iter()
            .flat_map(|&gray| [gray, gray, gray, 255])
            .collect(),
        PixelFormat::RgbaF32 => return Err("float pictures are not converted yet".to_owned()),
    };
    RasterTile2D::from_bytes(tile.width(), tile.height(), PixelFormat::Rgba8, pixels)
        .map_err(|error| error.to_string())
}

/// The size `resize` makes of `width` by `height`.
#[must_use]
pub fn resized(width: u32, height: u32, resize: Resize) -> (u32, u32) {
    let (w, h) = (width.max(1) as f32, height.max(1) as f32);
    let (nw, nh) = match resize {
        Resize::Keep => (w, h),
        Resize::Scale(percent) => (w * percent / 100.0, h * percent / 100.0),
        Resize::Width(target) => (target as f32, h * target as f32 / w),
        Resize::Height(target) => (w * target as f32 / h, target as f32),
        Resize::Fit(bw, bh) => {
            let scale = (bw as f32 / w).min(bh as f32 / h);
            (w * scale, h * scale)
        }
        Resize::Exact(bw, bh) => (bw as f32, bh as f32),
    };
    (
        (nw.round() as u32).clamp(1, 32_768),
        (nh.round() as u32).clamp(1, 32_768),
    )
}

/// Resamples premultiplied RGBA rows along one axis.
fn resample_axis(
    source: &[f32],
    (width, height): (usize, usize),
    out_len: usize,
    horizontal: bool,
    filter: Filter,
) -> Vec<f32> {
    let in_len = if horizontal { width } else { height };
    let lines = if horizontal { height } else { width };
    let scale = out_len as f32 / in_len as f32;
    let stretch = if scale < 1.0 { 1.0 / scale } else { 1.0 };
    let support = filter.support() * stretch;
    let (out_w, out_h) = if horizontal {
        (out_len, height)
    } else {
        (width, out_len)
    };
    let mut out = vec![0.0_f32; out_w * out_h * 4];
    // The taps of each output position, shared by every line.
    let taps: Vec<(usize, Vec<f32>)> = (0..out_len)
        .map(|at| {
            let center = (at as f32 + 0.5) / scale;
            let first = (center - support).floor().max(0.0) as usize;
            let last = ((center + support).ceil() as usize).min(in_len);
            let mut weights: Vec<f32> = (first..last)
                .map(|index| filter.weight((index as f32 + 0.5 - center) / stretch))
                .collect();
            let total: f32 = weights.iter().sum();
            if total.abs() > 1e-6 {
                weights.iter_mut().for_each(|weight| *weight /= total);
            } else if let Some(nearest) = weights.first_mut() {
                *nearest = 1.0;
            }
            (first, weights)
        })
        .collect();
    for line in 0..lines {
        for (at, (first, weights)) in taps.iter().enumerate() {
            let mut sum = [0.0_f32; 4];
            for (offset, weight) in weights.iter().enumerate() {
                let index = first + offset;
                let pixel = if horizontal {
                    line * width + index
                } else {
                    index * width + line
                } * 4;
                for channel in 0..4 {
                    sum[channel] += source[pixel + channel] * weight;
                }
            }
            let pixel = if horizontal {
                line * out_w + at
            } else {
                at * out_w + line
            } * 4;
            out[pixel..pixel + 4].copy_from_slice(&sum);
        }
    }
    out
}

/// Resizes RGBA8 pixels with `filter`, blending in premultiplied alpha so
/// transparent edges never turn dark.
#[must_use]
pub fn resize(
    pixels: &[u8],
    (width, height): (u32, u32),
    (nw, nh): (u32, u32),
    filter: Filter,
) -> Vec<u8> {
    if (width, height) == (nw, nh) {
        return pixels.to_vec();
    }
    let premultiplied: Vec<f32> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| {
            let alpha = f32::from(pixel[3]) / 255.0;
            [
                f32::from(pixel[0]) * alpha,
                f32::from(pixel[1]) * alpha,
                f32::from(pixel[2]) * alpha,
                alpha * 255.0,
            ]
        })
        .collect();
    let (w, h) = (width as usize, height as usize);
    let across = resample_axis(&premultiplied, (w, h), nw as usize, true, filter);
    let done = resample_axis(&across, (nw as usize, h), nh as usize, false, filter);
    done.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| {
            let alpha = pixel[3].clamp(0.0, 255.0);
            let unpremultiply = |value: f32| {
                if alpha <= 0.0 {
                    0
                } else {
                    (value * 255.0 / alpha).round().clamp(0.0, 255.0) as u8
                }
            };
            [
                unpremultiply(pixel[0]),
                unpremultiply(pixel[1]),
                unpremultiply(pixel[2]),
                alpha.round() as u8,
            ]
        })
        .collect()
}

/// Turns RGBA8 pixels a quarter turn clockwise, `turns` times.
#[must_use]
pub fn turn(pixels: &[u8], (width, height): (u32, u32), turns: u8) -> (Vec<u8>, (u32, u32)) {
    let (w, h) = (width as usize, height as usize);
    match turns % 4 {
        0 => (pixels.to_vec(), (width, height)),
        2 => {
            let out: Vec<u8> = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .rev()
                .flatten()
                .copied()
                .collect();
            (out, (width, height))
        }
        quarter => {
            let mut out = vec![0_u8; pixels.len()];
            for y in 0..h {
                for x in 0..w {
                    let (nx, ny) = if quarter == 1 {
                        (h - 1 - y, x)
                    } else {
                        (y, w - 1 - x)
                    };
                    let from = (y * w + x) * 4;
                    let to = (ny * h + nx) * 4;
                    out[to..to + 4].copy_from_slice(&pixels[from..from + 4]);
                }
            }
            (out, (height, width))
        }
    }
}

/// Mirrors RGBA8 pixels.
pub fn flip(pixels: &mut [u8], (width, height): (u32, u32), horizontal: bool) {
    let (w, h) = (width as usize, height as usize);
    if horizontal {
        for row in pixels.chunks_exact_mut(w * 4) {
            for x in 0..w / 2 {
                for channel in 0..4 {
                    row.swap(x * 4 + channel, (w - 1 - x) * 4 + channel);
                }
            }
        }
    } else {
        for y in 0..h / 2 {
            let (top, bottom) = pixels.split_at_mut((h - 1 - y) * w * 4);
            top[y * w * 4..(y + 1) * w * 4].swap_with_slice(&mut bottom[..w * 4]);
        }
    }
}

/// Brightness, contrast, saturation, grays and flattening, pixel by pixel.
pub fn adjust(pixels: &mut [u8], options: &PictureOptions, flatten: bool) {
    let contrast = (1.0 + options.contrast).max(0.0).powf(2.0);
    for pixel in pixels.as_chunks_mut::<4>().0.iter_mut() {
        let mut rgb = [pixel[0], pixel[1], pixel[2]].map(|value| f32::from(value) / 255.0);
        for value in &mut rgb {
            *value = ((*value - 0.5) * contrast + 0.5 + options.brightness * 0.5).clamp(0.0, 1.0);
        }
        let luma = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
        let saturation = if options.grayscale {
            0.0
        } else {
            1.0 + options.saturation
        };
        for value in &mut rgb {
            *value = (luma + (*value - luma) * saturation).clamp(0.0, 1.0);
        }
        if flatten {
            let alpha = f32::from(pixel[3]) / 255.0;
            for (value, background) in rgb.iter_mut().zip(options.background) {
                *value = *value * alpha + f32::from(background) / 255.0 * (1.0 - alpha);
            }
            pixel[3] = 255;
        }
        for (channel, value) in pixel.iter_mut().zip(rgb) {
            *channel = (value * 255.0).round() as u8;
        }
    }
}

/// Changes a picture as the options say.
///
/// # Errors
/// If it holds float pixels.
pub fn transform_picture(
    tile: &RasterTile2D,
    options: &PictureOptions,
    target: Target,
) -> Result<RasterTile2D, String> {
    let tile = rgba(tile)?;
    let mut size = (tile.width(), tile.height());
    let mut pixels = tile.bytes().ok_or("the picture has no pixels")?.to_vec();
    let mut new_size = resized(size.0, size.1, options.resize);
    // Icons and the like have a largest size: fit into it.
    if let Target::Picture(format) = target
        && let Some(max) = format.max_side()
        && new_size.0.max(new_size.1) > max
    {
        new_size = resized(new_size.0, new_size.1, Resize::Fit(max, max));
    }
    if new_size != size {
        pixels = resize(&pixels, size, new_size, options.filter);
        size = new_size;
    }
    if !options.quarter_turns.is_multiple_of(4) {
        (pixels, size) = turn(&pixels, size, options.quarter_turns);
    }
    if options.flip_h {
        flip(&mut pixels, size, true);
    }
    if options.flip_v {
        flip(&mut pixels, size, false);
    }
    let touched = options.grayscale
        || options.brightness != 0.0
        || options.contrast != 0.0
        || options.saturation != 0.0
        || options.flatten
        || target.flattens();
    if touched {
        adjust(&mut pixels, options, options.flatten || target.flattens());
    }
    RasterTile2D::from_bytes(size.0, size.1, PixelFormat::Rgba8, pixels)
        .map_err(|error| error.to_string())
}

/// Writes a picture as `target`.
///
/// # Errors
/// The encoder's message.
pub fn write_picture(
    tile: &RasterTile2D,
    target: Target,
    options: &PictureOptions,
) -> Result<Vec<u8>, String> {
    match target {
        Target::Png => encode_png(tile),
        Target::Jpeg => encode_jpeg(tile, options.jpeg_quality.clamp(1, 100)),
        Target::Jxl => NativeJxlPipeline
            .encode_layers(&[tile], options.jxl_effort.clamp(1, 9))
            .map_err(|error| match error {
                JxlError::CodecUnavailable { .. } => {
                    "JPEG XL writing is not in this build".to_owned()
                }
                other => other.to_string(),
            }),
        Target::Picture(format) => encode_picture(tile, format),
        other => Err(format!("a picture does not become {}", other.label())),
    }
}

// ----- Sounds ------------------------------------------------------------------

/// Resamples interleaved `samples` of `channels` from `from` to `to` per
/// second with a Lanczos window.
#[must_use]
pub fn resample(samples: &[f32], channels: usize, ratio: f64) -> Vec<f32> {
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

/// Changes a sound as the options say.
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
    // Speed, then the sample rate.
    let out_rate = options.sample_rate.unwrap_or(rate).clamp(1_000, 384_000);
    let ratio = f64::from(out_rate) / f64::from(rate) / f64::from(options.speed.clamp(0.25, 4.0));
    samples = resample(&samples, out_channels, ratio);
    if options.reverse {
        samples = samples
            .chunks_exact(out_channels)
            .rev()
            .flatten()
            .copied()
            .collect();
    }
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
        return Err("trimming left no sound".to_owned());
    }
    PCMFloat1D::new(out_rate, out_channels as u16, samples).map_err(|error| error.to_string())
}

/// Writes a sound as `target`.
///
/// # Errors
/// The encoder's message.
pub fn write_sound(
    pcm: &PCMFloat1D,
    target: Target,
    depth: SampleDepth,
) -> Result<Vec<u8>, String> {
    match target {
        Target::Wav => Ok(encode_wav_as(pcm, depth)),
        Target::Flac => encode_flac(pcm, depth),
        Target::Aiff => Ok(encode_aiff(pcm, depth)),
        other => Err(format!("a sound does not become {}", other.label())),
    }
}

// ----- Text and bytes ----------------------------------------------------------

/// The text tidied: line endings, trailing spaces, tabs.
#[must_use]
pub fn tidy(text: &str, options: &TextOptions) -> String {
    let crlf = match options.endings {
        Endings::Keep => text.contains("\r\n"),
        Endings::Lf => false,
        Endings::CrLf => true,
    };
    let ends = text.ends_with('\n');
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            let mut line = if options.tab_spaces > 0 {
                line.replace('\t', &" ".repeat(usize::from(options.tab_spaces)))
            } else {
                line.to_owned()
            };
            if options.trim_trailing {
                line.truncate(line.trim_end().len());
            }
            line
        })
        .collect();
    let mut out = lines.join(if crlf { "\r\n" } else { "\n" });
    if ends {
        out.push_str(if crlf { "\r\n" } else { "\n" });
    }
    out
}

/// Splits a table line by `separator`, honoring double quotes.
fn split_row(line: &str, separator: char) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            c if c == separator && !quoted => cells.push(std::mem::take(&mut cell)),
            c => cell.push(c),
        }
    }
    cells.push(cell);
    cells
}

/// The rows of a CSV or TSV table.
#[must_use]
pub fn table(text: &str, separator: char) -> Vec<Vec<String>> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| split_row(line, separator))
        .collect()
}

/// Writes rows as CSV (quoting where needed) or TSV.
#[must_use]
pub fn write_table(rows: &[Vec<String>], separator: char) -> String {
    let mut out = String::new();
    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| {
                if separator == ','
                    && (cell.contains(',') || cell.contains('"') || cell.contains('\n'))
                {
                    format!("\"{}\"", cell.replace('"', "\"\""))
                } else {
                    cell.replace(['\t', '\n'], " ")
                }
            })
            .collect();
        out.push_str(&cells.join(&separator.to_string()));
        out.push('\n');
    }
    out
}

fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A table as JSON: objects named by the header row, or arrays.
#[must_use]
pub fn table_json(rows: &[Vec<String>], header: bool) -> String {
    let cell = |text: &str| -> String {
        let trimmed = text.trim();
        if trimmed.parse::<f64>().is_ok() && !trimmed.is_empty() && !trimmed.starts_with('+') {
            trimmed.to_owned()
        } else {
            json_string(text)
        }
    };
    let mut items = Vec::new();
    if header && let Some((names, rest)) = rows.split_first() {
        for row in rest {
            let fields: Vec<String> = names
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    format!(
                        "{}:{}",
                        json_string(name),
                        cell(row.get(index).map_or("", String::as_str))
                    )
                })
                .collect();
            items.push(format!("{{{}}}", fields.join(",")));
        }
    } else {
        for row in rows {
            let cells: Vec<String> = row.iter().map(|text| cell(text)).collect();
            items.push(format!("[{}]", cells.join(",")));
        }
    }
    format!("[{}]", items.join(","))
}

/// Re-indents JSON by `indent` spaces a level (0 writes one line),
/// leaving strings as they are.
#[must_use]
pub fn reindent_json(text: &str, indent: u8) -> String {
    let mut out = String::new();
    let mut depth = 0_usize;
    let mut in_string = false;
    let mut escaped = false;
    let pad = |out: &mut String, depth: usize| {
        if indent > 0 {
            out.push('\n');
            out.push_str(&" ".repeat(depth * usize::from(indent)));
        }
    };
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '{' | '[' => {
                out.push(c);
                // Keep empty objects and arrays on one line.
                let next = chars[index + 1..].iter().find(|c| !c.is_whitespace());
                if matches!(next, Some('}' | ']')) {
                    let close = chars[index + 1..]
                        .iter()
                        .position(|c| !c.is_whitespace())
                        .unwrap_or(0);
                    out.push(chars[index + 1 + close]);
                    index += close + 2;
                    continue;
                }
                depth += 1;
                pad(&mut out, depth);
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
                pad(&mut out, depth);
                out.push(c);
            }
            ',' => {
                out.push(c);
                pad(&mut out, depth);
            }
            ':' => {
                out.push(':');
                if indent > 0 {
                    out.push(' ');
                }
            }
            c if c.is_whitespace() => {}
            c => out.push(c),
        }
        index += 1;
    }
    if indent > 0 {
        out.push('\n');
    }
    out
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Bold, italics, code and links inside one line of Markdown.
fn inline(text: &str) -> String {
    let mut out = String::new();
    let mut rest = escape_html(text);
    // Code first, so its insides stay as they are.
    while let Some(start) = rest.find('`') {
        let Some(len) = rest[start + 1..].find('`') else {
            break;
        };
        out.push_str(&emphasis(&rest[..start]));
        out.push_str(&format!(
            "<code>{}</code>",
            &rest[start + 1..start + 1 + len]
        ));
        rest = rest[start + len + 2..].to_owned();
    }
    out.push_str(&emphasis(&rest));
    out
}

fn emphasis(text: &str) -> String {
    let mut out = text.to_owned();
    for (mark, tag) in [("**", "strong"), ("*", "em"), ("_", "em")] {
        let mut parts = out.split(mark).collect::<Vec<_>>();
        if parts.len() >= 3 {
            let mut joined = String::new();
            let last = parts.len() - 1;
            for (index, part) in parts.drain(..).enumerate() {
                joined.push_str(part);
                if index < last {
                    joined.push_str(&if index % 2 == 0 {
                        format!("<{tag}>")
                    } else {
                        format!("</{tag}>")
                    });
                }
            }
            if last % 2 == 1 {
                joined.push_str(&format!("</{tag}>"));
            }
            out = joined;
        }
    }
    // Links: [text](address).
    let mut linked = String::new();
    let mut rest = out.as_str();
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|at| open + at) else {
            break;
        };
        let Some(end) = rest[close..].find(')').map(|at| close + at) else {
            break;
        };
        linked.push_str(&rest[..open]);
        linked.push_str(&format!(
            "<a href=\"{}\">{}</a>",
            &rest[close + 2..end],
            &rest[open + 1..close]
        ));
        rest = &rest[end + 1..];
    }
    linked.push_str(rest);
    linked
}

/// Markdown as an HTML page titled `title`.
#[must_use]
pub fn markdown_html(text: &str, title: &str) -> String {
    let mut body = String::new();
    let mut paragraph: Vec<String> = Vec::new();
    let mut list: Option<&str> = None;
    let mut code: Option<Vec<String>> = None;
    let flush = |body: &mut String, paragraph: &mut Vec<String>| {
        if !paragraph.is_empty() {
            body.push_str(&format!("<p>{}</p>\n", inline(&paragraph.join(" "))));
            paragraph.clear();
        }
    };
    for line in text.lines() {
        if let Some(lines) = code.as_mut() {
            if line.trim_start().starts_with("```") {
                body.push_str(&format!(
                    "<pre><code>{}</code></pre>\n",
                    escape_html(&lines.join("\n"))
                ));
                code = None;
            } else {
                lines.push(line.to_owned());
            }
            continue;
        }
        let trimmed = line.trim();
        let item = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .map(|item| ("ul", item))
            .or_else(|| {
                let (number, item) = trimmed.split_once(". ")?;
                number
                    .chars()
                    .all(|c| c.is_ascii_digit())
                    .then_some(("ol", item))
            });
        if list.is_some() && item.map(|(kind, _)| kind) != list {
            body.push_str(&format!("</{}>\n", list.unwrap_or("ul")));
            list = None;
        }
        if trimmed.starts_with("```") {
            flush(&mut body, &mut paragraph);
            code = Some(Vec::new());
        } else if trimmed.is_empty() {
            flush(&mut body, &mut paragraph);
        } else if let Some((kind, item)) = item {
            flush(&mut body, &mut paragraph);
            if list.is_none() {
                body.push_str(&format!("<{kind}>\n"));
                list = Some(kind);
            }
            body.push_str(&format!("<li>{}</li>\n", inline(item)));
        } else if trimmed.starts_with('#') {
            flush(&mut body, &mut paragraph);
            let level = trimmed.chars().take_while(|c| *c == '#').count().min(6);
            body.push_str(&format!(
                "<h{level}>{}</h{level}>\n",
                inline(trimmed[level..].trim())
            ));
        } else if let Some(quote) = trimmed.strip_prefix('>') {
            flush(&mut body, &mut paragraph);
            body.push_str(&format!(
                "<blockquote>{}</blockquote>\n",
                inline(quote.trim())
            ));
        } else if trimmed == "---" || trimmed == "***" {
            flush(&mut body, &mut paragraph);
            body.push_str("<hr>\n");
        } else {
            paragraph.push(trimmed.to_owned());
        }
    }
    flush(&mut body, &mut paragraph);
    if let Some(kind) = list {
        body.push_str(&format!("</{kind}>\n"));
    }
    if let Some(lines) = code {
        body.push_str(&format!(
            "<pre><code>{}</code></pre>\n",
            escape_html(&lines.join("\n"))
        ));
    }
    format!(
        "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n<style>body{{font-family:system-ui,sans-serif;max-width:46em;margin:3em auto;padding:0 1em;line-height:1.55}}pre{{background:#f2f2f2;padding:1em;overflow:auto}}code{{font-family:monospace}}</style>\n</head>\n<body>\n{body}</body>\n</html>\n",
        escape_html(title)
    )
}

/// Bytes as Base64 text.
#[must_use]
pub fn base64(bytes: &[u8], wrap: bool) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4 + bytes.len() / 57 + 2);
    let mut column = 0;
    for chunk in bytes.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for index in 0..4 {
            let c = if index <= chunk.len() {
                ALPHABET[((value >> (18 - index * 6)) & 63) as usize] as char
            } else {
                '='
            };
            out.push(c);
            column += 1;
            if wrap && column == 76 {
                out.push('\n');
                column = 0;
            }
        }
    }
    if wrap && column > 0 {
        out.push('\n');
    }
    out
}

/// Bytes as a hex dump: offset, bytes, and the printable characters.
#[must_use]
pub fn hex_dump(bytes: &[u8], width: u8) -> String {
    let width = usize::from(width.clamp(4, 64));
    let mut out = String::new();
    for (row, chunk) in bytes.chunks(width).enumerate() {
        let _ = write!(out, "{:08x}  ", row * width);
        for index in 0..width {
            match chunk.get(index) {
                Some(byte) => {
                    let _ = write!(out, "{byte:02x} ");
                }
                None => out.push_str("   "),
            }
        }
        out.push(' ');
        out.push('|');
        out.extend(chunk.iter().map(|&byte| {
            if byte.is_ascii_graphic() || byte == b' ' {
                byte as char
            } else {
                '.'
            }
        }));
        out.push_str("|\n");
    }
    out
}

// ----- The whole conversion ----------------------------------------------------

/// Converts a file's `bytes` (read from `path`, of `kind`) to `target`.
/// Returns the written bytes and a note on what it became.
///
/// # Errors
/// A message for the Converter's list.
pub fn convert(
    path: &Path,
    bytes: Vec<u8>,
    kind: Kind,
    target: Target,
    options: &Options,
) -> Result<(Vec<u8>, String), String> {
    let name = path
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    match target {
        Target::Copy => Ok((bytes, "copied".to_owned())),
        Target::Base64 => Ok((
            base64(&bytes, options.text.base64_wrap).into_bytes(),
            "Base64".to_owned(),
        )),
        Target::Hex => Ok((
            hex_dump(&bytes, options.text.hex_width).into_bytes(),
            "hex dump".to_owned(),
        )),
        Target::ByteMap => {
            let tile = sue_io::byte_map_tile(bytes, options.text.map_width.max(1))
                .map_err(|error| error.to_string())?;
            let note = format!("{} by {} byte map", tile.width(), tile.height());
            Ok((encode_png(&tile)?, note))
        }
        _ => match kind {
            Kind::Picture => {
                let tile = read_picture(path, &bytes)?;
                let changed = transform_picture(&tile, &options.picture, target)?;
                let note = format!("{} by {} pixels", changed.width(), changed.height());
                Ok((write_picture(&changed, target, &options.picture)?, note))
            }
            Kind::Sound => {
                let extension = extension(path);
                let (decoded, _) =
                    decode_audio(bytes, Some(&extension)).map_err(|(error, _)| error)?;
                let changed = transform_sound(&decoded.pcm, &options.sound)?;
                let note = format!(
                    "{:.1} s, {} Hz, {} channels",
                    changed.duration_seconds(),
                    changed.sample_rate(),
                    changed.channels()
                );
                Ok((write_sound(&changed, target, options.sound.depth)?, note))
            }
            Kind::Text | Kind::Data => {
                let text =
                    String::from_utf8(bytes).map_err(|_| "this file is not text".to_owned())?;
                let source = extension(path);
                let separator = if source == "tsv" { '\t' } else { ',' };
                let out = match target {
                    Target::Text => tidy(&text, &options.text),
                    Target::Html => {
                        let title = if options.text.html_title.trim().is_empty() {
                            name
                        } else {
                            options.text.html_title.clone()
                        };
                        markdown_html(&text, &title)
                    }
                    Target::Json if source == "json" => {
                        reindent_json(&text, options.text.json_indent)
                    }
                    Target::Json => reindent_json(
                        &table_json(&table(&text, separator), options.text.header_row),
                        options.text.json_indent,
                    ),
                    Target::Csv => write_table(&table(&text, separator), ','),
                    Target::Tsv => write_table(&table(&text, separator), '\t'),
                    other => return Err(format!("text does not become {}", other.label())),
                };
                Ok((
                    tidy(
                        &out,
                        &TextOptions {
                            endings: options.text.endings,
                            ..TextOptions::default()
                        },
                    )
                    .into_bytes(),
                    "text".to_owned(),
                ))
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(width: u32, height: u32) -> RasterTile2D {
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                pixels.extend([(x * 40) as u8, (y * 40) as u8, 128, 255]);
            }
        }
        RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels).unwrap()
    }

    #[test]
    fn sizes_follow_the_resize_choice() {
        assert_eq!(resized(400, 200, Resize::Scale(50.0)), (200, 100));
        assert_eq!(resized(400, 200, Resize::Width(100)), (100, 50));
        assert_eq!(resized(400, 200, Resize::Height(100)), (200, 100));
        assert_eq!(resized(400, 200, Resize::Fit(100, 100)), (100, 50));
        assert_eq!(resized(400, 200, Resize::Exact(30, 70)), (30, 70));
        assert_eq!(resized(400, 200, Resize::Keep), (400, 200));
    }

    #[test]
    fn resizing_keeps_flat_colors_and_turning_moves_pixels() {
        let flat = [10_u8, 200, 30, 255].repeat(16 * 16);
        for (filter, _) in Filter::ALL {
            let small = resize(&flat, (16, 16), (5, 7), filter);
            assert_eq!(small.len(), 5 * 7 * 4);
            assert!(
                small
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| *pixel == [10, 200, 30, 255]),
                "{filter:?}"
            );
        }
        // A see-through edge stays its own color, not darker.
        let mut edge = [255_u8, 0, 0, 255].repeat(4);
        edge.extend([0, 0, 0, 0].repeat(4));
        let blended = resize(&edge, (8, 1), (4, 1), Filter::Bilinear);
        assert!(
            blended
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[3] > 0)
                .all(|pixel| pixel[0] == 255)
        );
        let tile = picture(3, 2);
        let pixels = tile.bytes().unwrap();
        let (turned, size) = turn(pixels, (3, 2), 1);
        assert_eq!(size, (2, 3));
        // The top left pixel moves to the top right.
        assert_eq!(&turned[4..8], &pixels[..4]);
        let (back, _) = turn(&turned, size, 3);
        assert_eq!(back, pixels);
        let mut mirrored = pixels.to_vec();
        flip(&mut mirrored, (3, 2), true);
        assert_eq!(&mirrored[8..12], &pixels[..4]);
        flip(&mut mirrored, (3, 2), true);
        flip(&mut mirrored, (3, 2), false);
        assert_eq!(&mirrored[..12], &pixels[12..]);
    }

    #[test]
    fn pictures_convert_to_every_picture_format() {
        let dir = std::env::temp_dir().join(format!("sue-convert-ops-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = encode_png(&picture(20, 10)).unwrap();
        let path = dir.join("card.png");
        let options = Options {
            picture: PictureOptions {
                resize: Resize::Width(10),
                grayscale: true,
                ..PictureOptions::default()
            },
            ..Options::default()
        };
        for target in Target::for_kind(Kind::Picture) {
            let (bytes, note) = convert(&path, png.clone(), Kind::Picture, target, &options)
                .unwrap_or_else(|error| panic!("{target:?}: {error}"));
            assert!(!bytes.is_empty(), "{target:?}");
            if matches!(
                target,
                Target::Png | Target::Jpeg | Target::Jxl | Target::Picture(_)
            ) {
                assert_eq!(note, "10 by 5 pixels", "{target:?}");
                let out = dir.join(format!("card.{}", target.extension()));
                let back = read_picture(&out, &bytes).unwrap();
                assert_eq!((back.width(), back.height()), (10, 5), "{target:?}");
                let pixel = &back.bytes().unwrap()[..3];
                let spread = pixel.iter().max().unwrap() - pixel.iter().min().unwrap();
                assert!(spread <= 12, "{target:?} kept color {pixel:?}");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
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
        let fast = transform_sound(
            &pcm,
            &SoundOptions {
                speed: 2.0,
                ..SoundOptions::default()
            },
        )
        .unwrap();
        assert!((fast.duration_seconds() - 1.0).abs() < 0.01);
        for target in [Target::Wav, Target::Flac, Target::Aiff] {
            let bytes = write_sound(&out, target, SampleDepth::Int16).unwrap();
            let (back, _) = decode_audio(bytes, Some(target.extension())).unwrap();
            assert_eq!(back.pcm.frames(), out.frames(), "{target:?}");
        }
    }

    #[test]
    fn text_tables_json_and_markdown_convert() {
        let options = TextOptions {
            endings: Endings::CrLf,
            trim_trailing: true,
            tab_spaces: 2,
            ..TextOptions::default()
        };
        assert_eq!(tidy("a  \n\tb\n", &options), "a\r\n  b\r\n");
        let rows = table("name,age\n\"Lee, Ada\",36\n", ',');
        assert_eq!(rows[1], ["Lee, Ada", "36"]);
        assert_eq!(write_table(&rows, ','), "name,age\n\"Lee, Ada\",36\n");
        assert_eq!(write_table(&rows, '\t'), "name\tage\nLee, Ada\t36\n");
        let json = table_json(&rows, true);
        assert_eq!(json, "[{\"name\":\"Lee, Ada\",\"age\":36}]");
        assert_eq!(
            reindent_json(&json, 2),
            "[\n  {\n    \"name\": \"Lee, Ada\",\n    \"age\": 36\n  }\n]\n"
        );
        assert_eq!(reindent_json("{ \"a\" : [ ] }", 0), "{\"a\":[]}");
        let html = markdown_html(
            "# Title\n\nSome **bold** and `code`.\n\n- one\n- two\n",
            "Doc",
        );
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<p>Some <strong>bold</strong> and <code>code</code>.</p>"));
        assert!(html.contains("<ul>\n<li>one</li>\n<li>two</li>\n</ul>"));
        assert!(html.contains("<title>Doc</title>"));
    }

    #[test]
    fn any_bytes_become_base64_hex_and_byte_maps() {
        assert_eq!(base64(b"Man", false), "TWFu");
        assert_eq!(base64(b"Ma", false), "TWE=");
        assert_eq!(base64(b"M", false), "TQ==");
        let dump = hex_dump(b"Hi\x00", 4);
        assert_eq!(dump, "00000000  48 69 00     |Hi.|\n");
        let (png, note) = convert(
            Path::new("x.bin"),
            vec![7; 300],
            Kind::Data,
            Target::ByteMap,
            &Options {
                text: TextOptions {
                    map_width: 100,
                    ..TextOptions::default()
                },
                ..Options::default()
            },
        )
        .unwrap();
        assert_eq!(note, "100 by 3 byte map");
        assert!(png.starts_with(b"\x89PNG"));
        assert_eq!(kind_of(Path::new("a.flac"), b""), Kind::Sound);
        assert_eq!(kind_of(Path::new("a.webp"), b""), Kind::Picture);
        assert_eq!(kind_of(Path::new("a.unknown"), b"plain words"), Kind::Text);
        assert_eq!(kind_of(Path::new("a.bin"), b"\x00\x01"), Kind::Data);
    }
}
