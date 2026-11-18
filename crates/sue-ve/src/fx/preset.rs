//! Effect presets: a clip's effects stack, with its keyframes, kept
//! under a name and shared as a small file (`.sveffect`), as After
//! Effects' animation presets and Premiere Pro's effect presets are.
//!
//! A preset is Sue's own [`Preset`] (target [`TARGET`]): its values hold
//! the effects as stored on a clip and their keyframes as an encoded
//! [`Animation`], with key times counted from the clip's in point so they
//! land the same way on any clip. The file is a short header and the
//! preset's encoding.

use sue_core::codec::{Reader, Writer};
use sue_core::{ComponentType, Preset, Properties, Time, Value};
use sue_model::{Animation, Clip, Curve, Key};

use super::{Effect, effects, next_id, parse_channel, set_effects};

/// The preset target of effect stacks.
pub const TARGET: &str = "sue-ve.effects";
/// A preset file's extension.
pub const EXTENSION: &str = "sveffect";
/// What a preset file starts with.
const MAGIC: &[u8; 8] = b"SUEVEFX1";

/// `clip`'s effects (and their keys) as a preset named `name`.
#[must_use]
pub fn capture(clip: &Clip, name: &str) -> Preset {
    let list = effects(clip);
    let mut keys = Animation::new();
    for (channel, curve) in clip.animation.channels() {
        if parse_channel(channel).is_none() {
            continue;
        }
        let target = keys.channel_mut(channel);
        for key in curve.keys() {
            target.insert(Key {
                time: key.time - clip.source_in,
                ..*key
            });
        }
    }
    let mut out = Writer::new();
    keys.encode(&mut out);
    let mut values = Properties::new();
    values.set(
        "effects",
        Value::List(list.iter().map(Effect::to_value).collect()),
    );
    values.set("keys", Value::Bytes(out.into_bytes()));
    Preset::new(name, TARGET, values)
}

/// Adds `preset`'s effects to the end of `clip`'s stack, with new ids and
/// their keys from the clip's in point. How many it added.
pub fn apply(preset: &Preset, clip: &mut Clip) -> usize {
    if preset.target != TARGET {
        return 0;
    }
    let incoming: Vec<Effect> = match preset.values.get("effects") {
        Some(Value::List(list)) => list.iter().filter_map(Effect::from_value).collect(),
        _ => return 0,
    };
    let keys = match preset.values.get("keys") {
        Some(Value::Bytes(bytes)) => {
            Animation::decode(Animation::VERSION, &mut Reader::new(bytes)).unwrap_or_default()
        }
        _ => Animation::new(),
    };
    let mut list = effects(clip);
    let mut id = next_id(clip);
    let added = incoming.len();
    for mut effect in incoming {
        let old = effect.id;
        effect.id = id;
        for (channel, curve) in keys.channels() {
            let Some((from, param)) = parse_channel(channel) else {
                continue;
            };
            if from != old {
                continue;
            }
            let mut moved = Curve::new();
            for key in curve.keys() {
                moved.insert(Key {
                    time: key.time + clip.source_in,
                    ..*key
                });
            }
            *clip.animation.channel_mut(&super::channel(id, param)) = moved;
        }
        list.push(effect);
        id += 1;
    }
    set_effects(clip, &list);
    added
}

/// A preset as a file's bytes.
#[must_use]
pub fn to_file(preset: &Preset) -> Vec<u8> {
    let mut out = Writer::new();
    preset.encode(&mut out);
    let mut bytes = MAGIC.to_vec();
    bytes.extend(out.into_bytes());
    bytes
}

/// A preset read from a file's bytes.
///
/// # Errors
/// When the bytes are not an effect preset.
pub fn from_file(bytes: &[u8]) -> Result<Preset, String> {
    let body = bytes
        .strip_prefix(MAGIC.as_slice())
        .ok_or_else(|| tr!("preset-not-a-preset"))?;
    let preset = Preset::decode(&mut Reader::new(body)).map_err(|error| error.to_string())?;
    if preset.target != TARGET {
        return Err(tr!("preset-not-a-preset"));
    }
    Ok(preset)
}

/// Where Sue VE keeps the presets saved here.
#[must_use]
pub fn folder() -> std::path::PathBuf {
    crate::media::import::cache_dir().join("presets")
}

/// The presets saved here, by name.
#[must_use]
pub fn saved() -> Vec<(String, Preset)> {
    let Ok(entries) = std::fs::read_dir(folder()) else {
        return Vec::new();
    };
    let mut out: Vec<(String, Preset)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == EXTENSION))
        .filter_map(|path| {
            let bytes = sue_platform::fs::read(&path).ok()?;
            let preset = from_file(&bytes).ok()?;
            Some((preset.name.clone(), preset))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Saves `preset` here, under its name.
///
/// # Errors
/// What the file system said.
pub fn save(preset: &Preset) -> Result<std::path::PathBuf, String> {
    let dir = folder();
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let safe: String = preset
        .name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
        .collect();
    let path = dir.join(format!("{}.{EXTENSION}", safe.trim()));
    sue_platform::fs::write(&path, to_file(preset)).map_err(|error| error.to_string())?;
    Ok(path)
}

/// The key times of a preset relative to its clip's in point (tests).
#[must_use]
pub fn key_times(preset: &Preset) -> Vec<Time> {
    match preset.values.get("keys") {
        Some(Value::Bytes(bytes)) => Animation::decode(Animation::VERSION, &mut Reader::new(bytes))
            .map(|keys| {
                keys.channels()
                    .flat_map(|(_, curve)| curve.keys().iter().map(|key| key.time))
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}
