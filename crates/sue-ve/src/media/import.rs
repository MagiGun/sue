//! Bringing files into the media pool.
//!
//! A movie or a sound file becomes a kept encoded file: an entity that
//! links to the file by its path, with what probing found as properties,
//! and its bytes inside when the project embeds it. A still picture is
//! decoded into a raster entity (it is small and Sue's other studios work
//! on it as it is). A numbered run of pictures becomes an image sequence.

use std::path::{Path, PathBuf};

use sue_core::{Entity, RasterTile2D, Rate, Time};
use sue_model::{Encoded, class as model_class};

use crate::model::{class, key};

/// What a file is, by its extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// A movie.
    Movie,
    /// Sound only.
    Sound,
    /// A still picture.
    Still,
    /// Something else.
    Other,
}

/// Movie extensions the system decoders may read.
pub const MOVIE_EXTENSIONS: [&str; 9] = [
    "mp4", "m4v", "mov", "mkv", "avi", "wmv", "mts", "m2ts", "3gp",
];
/// Sound extensions.
pub const SOUND_EXTENSIONS: [&str; 9] = [
    "wav", "flac", "mp3", "m4a", "aac", "ogg", "oga", "aif", "aiff",
];
/// Still picture extensions.
pub const STILL_EXTENSIONS: [&str; 10] = [
    "png", "jpg", "jpeg", "bmp", "gif", "tif", "tiff", "webp", "tga", "qoi",
];

/// The kind of `path`, by its extension.
#[must_use]
pub fn kind_of(path: &Path) -> FileKind {
    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let is = |list: &[&str]| list.contains(&extension.as_str());
    if is(&MOVIE_EXTENSIONS) {
        FileKind::Movie
    } else if is(&SOUND_EXTENSIONS) {
        FileKind::Sound
    } else if is(&STILL_EXTENSIONS) {
        FileKind::Still
    } else {
        FileKind::Other
    }
}

/// Every extension the pool takes, for the open dialog.
#[must_use]
pub fn extensions() -> Vec<&'static str> {
    MOVIE_EXTENSIONS
        .iter()
        .chain(&SOUND_EXTENSIONS)
        .chain(&STILL_EXTENSIONS)
        .copied()
        .collect()
}

fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// A time in 100 nanosecond units on Sue's clock.
#[must_use]
pub fn from_ticks(ticks: i64) -> Time {
    Time(ticks.saturating_mul(sue_core::FLICKS_PER_SECOND / 10_000_000))
}

/// A picture file's pixels.
///
/// # Errors
/// When it is no picture Sue reads.
pub fn read_picture(bytes: &[u8]) -> Result<RasterTile2D, String> {
    if let Ok(png) = sue_io::decode_png(bytes) {
        return Ok(png.tile);
    }
    if let Ok(jpeg) = sue_io::decode_jpeg(bytes) {
        return Ok(jpeg.tile);
    }
    sue_io::decode_picture(bytes, None)
}

/// The entity for the file at `path`, ready to spawn: probed (a movie,
/// a sound) or decoded (a still). With `embed` a movie or sound keeps its
/// bytes inside the project too.
///
/// # Errors
/// When the file cannot be read, or is no media Sue takes.
pub fn entity_for(path: &Path, embed: bool) -> Result<Entity, String> {
    let name = name_of(path);
    let source = path.to_string_lossy().into_owned();
    let mut entity = match kind_of(path) {
        FileKind::Still => {
            let bytes = sue_platform::fs::read(path).map_err(|error| error.to_string())?;
            let tile = read_picture(&bytes)?;
            return Ok(Entity::new(name, tile)
                .with_class(model_class::IMAGE)
                .with_meta(sue_io::meta::SOURCE, source));
        }
        FileKind::Movie => {
            let mut entity = Entity::empty(&name, model_class::VIDEO);
            match sue_platform::video::probe(path) {
                Ok(facts) => {
                    entity = entity
                        .with_property(key::WIDTH, i64::from(facts.width))
                        .with_property(key::HEIGHT, i64::from(facts.height))
                        .with_property(key::RATE_NUM, i64::from(facts.rate.0))
                        .with_property(key::RATE_DEN, i64::from(facts.rate.1))
                        .with_property(key::DURATION, from_ticks(facts.duration))
                        .with_property(key::VIDEO_CODEC, facts.video_codec.clone());
                    if let Some((rate, channels)) = facts.audio {
                        entity = entity
                            .with_property(key::AUDIO_RATE, i64::from(rate))
                            .with_property(key::AUDIO_CHANNELS, i64::from(channels));
                    }
                }
                Err(sue_platform::video::VideoError::NotAvailable) => {}
                Err(error) => return Err(error.to_string()),
            }
            entity
        }
        FileKind::Sound => {
            let mut entity = Entity::empty(&name, model_class::AUDIO);
            let info = sue_io::probe_audio(path);
            let facts = sue_platform::video::probe(path).ok();
            let (rate, channels, length) = match (&info, &facts) {
                (Ok(info), _) if info.frames > 0 && info.sample_rate > 0 => (
                    info.sample_rate,
                    info.channels,
                    Time::from_seconds(info.frames as f64 / f64::from(info.sample_rate)),
                ),
                (_, Some(facts)) if facts.audio.is_some() => {
                    let (rate, channels) = facts.audio.unwrap_or((48_000, 2));
                    (rate, channels, from_ticks(facts.duration))
                }
                (Ok(info), _) => (info.sample_rate, info.channels, Time::ZERO),
                (Err(why), None) => return Err(why.clone()),
                (Err(why), Some(_)) => return Err(why.clone()),
            };
            entity = entity
                .with_property(key::AUDIO_RATE, i64::from(rate))
                .with_property(key::AUDIO_CHANNELS, i64::from(channels))
                .with_property(key::DURATION, length);
            entity
        }
        FileKind::Other => return Err(tr!("not-media", name = name)),
    };
    entity = entity
        .with_property(key::PATH, source.clone())
        .with_meta(sue_io::meta::SOURCE, source);
    if embed {
        let bytes = sue_platform::fs::read(path).map_err(|error| error.to_string())?;
        entity = entity.with_ext(Encoded::new(extension_of(path), bytes));
    }
    Ok(entity)
}

/// A numbered run of pictures found from one of them: the file name
/// pattern (`{n}` for the number), the first number, how many follow on
/// without a gap, and the digits.
#[must_use]
pub fn image_run(path: &Path) -> Option<(String, u32, u32, usize)> {
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let extension = path.extension()?.to_string_lossy().into_owned();
    let digits = stem.chars().rev().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let prefix = &stem[..stem.len() - digits];
    let pattern = format!("{prefix}{{n}}.{extension}");
    let dir = path.parent().unwrap_or(Path::new("."));
    let numbered = |n: u32| dir.join(format!("{prefix}{n:0digits$}.{extension}"));
    let mut first: u32 = stem[stem.len() - digits..].parse().ok()?;
    while first > 0 && numbered(first - 1).exists() {
        first -= 1;
    }
    let mut count = 0_u32;
    while count < 100_000 && numbered(first + count).exists() {
        count += 1;
    }
    (count > 1).then_some((pattern, first, count, digits))
}

/// The file of frame `index` of an image sequence entity.
#[must_use]
pub fn sequence_frame(entity: &Entity, index: i64) -> Option<PathBuf> {
    let folder = crate::model::text(entity, key::PATH)?;
    let pattern = crate::model::text(entity, key::PATTERN)?;
    let first = crate::model::number(entity, key::FIRST)? as i64;
    let count = crate::model::number(entity, key::COUNT)? as i64;
    let digits = crate::model::number(entity, key::DIGITS).unwrap_or(4.0) as usize;
    let n = first + index.clamp(0, count.max(1) - 1);
    let name = pattern.replace("{n}", &format!("{n:0digits$}"));
    Some(Path::new(folder).join(name))
}

/// An image sequence entity for the run `path` is part of, at `rate`.
///
/// # Errors
/// When `path` is not part of a numbered run, or its first picture does
/// not read.
pub fn image_sequence_for(path: &Path, rate: Rate) -> Result<Entity, String> {
    let (pattern, first, count, digits) =
        image_run(path).ok_or_else(|| tr!("not-a-numbered-run", name = name_of(path)))?;
    let folder = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let first_file = folder.join(pattern.replace("{n}", &format!("{first:0digits$}")));
    let bytes = sue_platform::fs::read(&first_file).map_err(|error| error.to_string())?;
    let tile = read_picture(&bytes)?;
    let name = pattern.replace("{n}", &"#".repeat(digits));
    Ok(Entity::empty(name, class::IMAGE_SEQUENCE)
        .with_property(key::PATH, folder.to_string_lossy().into_owned())
        .with_property(key::PATTERN, pattern)
        .with_property(key::FIRST, i64::from(first))
        .with_property(key::COUNT, i64::from(count))
        .with_property(key::DIGITS, digits as i64)
        .with_property(key::WIDTH, i64::from(tile.width()))
        .with_property(key::HEIGHT, i64::from(tile.height()))
        .with_property(key::RATE_NUM, i64::from(rate.numerator()))
        .with_property(key::RATE_DEN, i64::from(rate.denominator()))
        .with_property(key::DURATION, Time::from_frames(i64::from(count), rate)))
}

/// Where Sue VE keeps what it makes for itself: proxies and embedded
/// media unpacked for the decoders.
#[must_use]
pub fn cache_dir() -> PathBuf {
    sue_platform::PlatformPaths::detect()
        .map(|paths| paths.cache_dir)
        .unwrap_or_else(|_| std::env::temp_dir().join("sue"))
        .join("ve")
}

/// The file the decoders read for source `id`: its own file when it is
/// there, else its embedded bytes unpacked into the cache once.
#[must_use]
pub fn file_of(entity: &Entity) -> Option<PathBuf> {
    if let Some(path) = crate::model::text(entity, key::PATH).map(PathBuf::from)
        && path.is_file()
    {
        return Some(path);
    }
    let encoded = entity.ext::<Encoded>()?;
    let dir = cache_dir().join("media");
    let path = dir.join(format!("{}.{}", entity.id(), encoded.format));
    if !path.is_file() {
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::write(&path, encoded.bytes.as_slice()).ok()?;
    }
    Some(path)
}

/// A solid's or a title's color as RGBA.
#[must_use]
pub fn color_of(entity: &Entity, default: [u8; 4]) -> [u8; 4] {
    match entity.property(key::COLOR) {
        Some(sue_core::Value::Color(color)) => {
            let srgb = color.to_space(sue_core::ColorSpace::Srgb);
            let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            [
                byte(srgb.rgb[0]),
                byte(srgb.rgb[1]),
                byte(srgb.rgb[2]),
                byte(srgb.alpha),
            ]
        }
        _ => default,
    }
}

/// A new title saying `text`.
#[must_use]
pub fn title(name: &str, text: &str) -> Entity {
    Entity::new(name, sue_core::StringBuffer::from(text))
        .with_class(class::TITLE)
        .with_property(key::SIZE, 96.0)
        .with_property(key::COLOR, sue_core::Color::srgb8(255, 255, 255))
}

/// A new solid of `rgb`.
#[must_use]
pub fn solid(name: &str, rgb: [u8; 3]) -> Entity {
    Entity::empty(name, class::SOLID)
        .with_property(key::COLOR, sue_core::Color::srgb8(rgb[0], rgb[1], rgb[2]))
}

/// A new adjustment clip.
#[must_use]
pub fn adjustment(name: &str) -> Entity {
    Entity::empty(name, class::ADJUSTMENT)
}

/// A summary of what probing a movie found, for the pool's
/// list view: `1920 x 1080, 29.97 fps, H264`.
#[must_use]
pub fn summary(entity: &Entity) -> String {
    let get = |name: &str| crate::model::number(entity, name);
    let mut parts = Vec::new();
    if let (Some(width), Some(height)) = (get(key::WIDTH), get(key::HEIGHT)) {
        parts.push(format!("{width} x {height}"));
    }
    if let Some(rate) = crate::model::rate_of(entity) {
        parts.push(format!("{:.3} fps", rate.as_f64()).replace(".000", ""));
    }
    if let Some(codec) =
        crate::model::text(entity, key::VIDEO_CODEC).filter(|codec| !codec.is_empty())
    {
        parts.push(codec.to_owned());
    }
    if let Some(rate) = get(key::AUDIO_RATE) {
        parts.push(format!("{} kHz", rate / 1000.0));
    }
    parts.join(", ")
}
