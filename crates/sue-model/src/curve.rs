//! Animation curves. Keyframed motion graphics, 2D and 3D animation, rig
//! poses, DAW automation lanes, color grading keyframes and game tweens are
//! all a value that changes over [`Time`]. A [`Curve`] holds the keys; an
//! [`Animation`] maps property keys (the same keys as entity properties,
//! e.g. `transform.x` or `audio.gain`) to curves, so anything that has a
//! property can be animated the same way.

use std::collections::BTreeMap;

use sue_core::codec::{CodecError, Reader, Writer};
use sue_core::{ComponentType, Properties, Time};

/// How a curve moves from one key to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Interpolation {
    /// Holds the value until the next key.
    Constant,
    /// A straight line.
    Linear,
    /// A cubic Bezier. Handles are offsets from the key in (seconds, value):
    /// `out` leaves this key, `in` arrives at the next one.
    Bezier {
        /// Outgoing handle of this key.
        out: [f64; 2],
        /// Incoming handle of the next key.
        incoming: [f64; 2],
    },
}

/// One key.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    /// When.
    pub time: Time,
    /// The value there.
    pub value: f64,
    /// How to reach the next key.
    pub interpolation: Interpolation,
}

/// Keys sorted by time.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Curve {
    keys: Vec<Key>,
}

fn bezier(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let u = 1.0 - t;
    u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * p3
}

impl Curve {
    /// An empty curve.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The keys, by time.
    #[must_use]
    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    /// Adds a key, replacing one at the same time. Returns the replaced key.
    pub fn insert(&mut self, key: Key) -> Option<Key> {
        match self.keys.binary_search_by_key(&key.time, |key| key.time) {
            Ok(index) => Some(std::mem::replace(&mut self.keys[index], key)),
            Err(index) => {
                self.keys.insert(index, key);
                None
            }
        }
    }

    /// Removes the key at `time`.
    pub fn remove(&mut self, time: Time) -> Option<Key> {
        let index = self.keys.binary_search_by_key(&time, |key| key.time).ok()?;
        Some(self.keys.remove(index))
    }

    /// The value at `time`: held before the first key and after the last.
    #[must_use]
    pub fn sample(&self, time: Time) -> Option<f64> {
        let first = self.keys.first()?;
        if time <= first.time {
            return Some(first.value);
        }
        let index = self.keys.partition_point(|key| key.time <= time);
        let Some(next) = self.keys.get(index) else {
            return self.keys.last().map(|key| key.value);
        };
        let key = self.keys[index - 1];
        let span = (next.time - key.time).seconds();
        let x = (time - key.time).seconds();
        Some(match key.interpolation {
            Interpolation::Constant => key.value,
            Interpolation::Linear => key.value + (next.value - key.value) * (x / span),
            Interpolation::Bezier { out, incoming } => {
                // Clamp handle times inside the segment so time stays monotonic.
                let x1 = out[0].clamp(0.0, span);
                let x2 = (span + incoming[0]).clamp(0.0, span);
                let (y1, y2) = (key.value + out[1], next.value + incoming[1]);
                // Solve x(t) = x by bisection, then evaluate y(t).
                let (mut low, mut high) = (0.0, 1.0);
                for _ in 0..48 {
                    let middle = (low + high) / 2.0;
                    if bezier(0.0, x1, x2, span, middle) < x {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                bezier(key.value, y1, y2, next.value, (low + high) / 2.0)
            }
        })
    }

    fn encode(&self, out: &mut Writer) {
        out.len(self.keys.len());
        for key in &self.keys {
            out.varint_signed(key.time.flicks());
            out.f64(key.value);
            match key.interpolation {
                Interpolation::Constant => out.u8(0),
                Interpolation::Linear => out.u8(1),
                Interpolation::Bezier {
                    out: handle,
                    incoming,
                } => {
                    out.u8(2);
                    handle.iter().chain(&incoming).for_each(|x| out.f64(*x));
                }
            }
        }
    }

    fn decode(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let count = input.len(10)?;
        let mut curve = Self::new();
        for _ in 0..count {
            let time = Time(input.varint_signed()?);
            let value = input.f64()?;
            let interpolation = match input.u8()? {
                0 => Interpolation::Constant,
                1 => Interpolation::Linear,
                2 => Interpolation::Bezier {
                    out: [input.f64()?, input.f64()?],
                    incoming: [input.f64()?, input.f64()?],
                },
                other => return Err(input.error(format!("unknown interpolation {other}"))),
            };
            curve.insert(Key {
                time,
                value,
                interpolation,
            });
        }
        Ok(curve)
    }
}

/// Curves by property key.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Animation {
    channels: BTreeMap<String, Curve>,
}

impl Animation {
    /// No animated properties.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The curve of `key`.
    #[must_use]
    pub fn channel(&self, key: &str) -> Option<&Curve> {
        self.channels.get(key)
    }

    /// The curve of `key`, created if missing.
    pub fn channel_mut(&mut self, key: &str) -> &mut Curve {
        self.channels.entry(key.to_owned()).or_default()
    }

    /// Removes a channel.
    pub fn remove_channel(&mut self, key: &str) -> Option<Curve> {
        self.channels.remove(key)
    }

    /// Every channel.
    pub fn channels(&self) -> impl Iterator<Item = (&str, &Curve)> {
        self.channels
            .iter()
            .map(|(key, curve)| (key.as_str(), curve))
    }

    /// The animated values at `time`, as properties to lay over the base ones.
    #[must_use]
    pub fn sample(&self, time: Time) -> Properties {
        self.channels
            .iter()
            .filter_map(|(key, curve)| Some((key.clone(), curve.sample(time)?.into())))
            .collect()
    }
}

impl ComponentType for Animation {
    const NAME: &'static str = "sue.animation";
    const VERSION: u32 = 1;

    fn encode(&self, out: &mut Writer) {
        out.len(self.channels.len());
        for (key, curve) in &self.channels {
            out.str(key);
            curve.encode(out);
        }
    }

    fn decode(_version: u32, input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let count = input.len(2)?;
        let mut channels = BTreeMap::new();
        for _ in 0..count {
            let key = input.str()?;
            channels.insert(key, Curve::decode(input)?);
        }
        Ok(Self { channels })
    }

    fn heap_bytes(&self) -> usize {
        self.channels
            .values()
            .map(|curve| curve.keys.len() * size_of::<Key>())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(seconds: f64, value: f64, interpolation: Interpolation) -> Key {
        Key {
            time: Time::from_seconds(seconds),
            value,
            interpolation,
        }
    }

    #[test]
    fn curves_interpolate_every_way() {
        let mut curve = Curve::new();
        curve.insert(key(2.0, 10.0, Interpolation::Constant));
        curve.insert(key(0.0, 0.0, Interpolation::Linear));
        curve.insert(key(3.0, 20.0, Interpolation::Linear));
        let at = |s: f64| curve.sample(Time::from_seconds(s)).unwrap();
        assert_eq!(at(-1.0), 0.0);
        assert!((at(1.0) - 5.0).abs() < 1e-9);
        assert_eq!(at(2.5), 10.0, "constant holds");
        assert_eq!(at(9.0), 20.0);
        // An ease with flat handles: symmetric, slow at both ends.
        let mut ease = Curve::new();
        let flat = Interpolation::Bezier {
            out: [0.5, 0.0],
            incoming: [-0.5, 0.0],
        };
        ease.insert(key(0.0, 0.0, flat));
        ease.insert(key(1.0, 1.0, Interpolation::Linear));
        let middle = ease.sample(Time::from_seconds(0.5)).unwrap();
        assert!((middle - 0.5).abs() < 1e-6, "{middle}");
        let early = ease.sample(Time::from_seconds(0.1)).unwrap();
        assert!(early < 0.1, "slow start {early}");
        assert!(Curve::new().sample(Time::ZERO).is_none());
        assert!(curve.remove(Time::from_seconds(2.0)).is_some());
    }

    #[test]
    fn animations_sample_properties_and_round_trip() {
        let mut animation = Animation::new();
        animation
            .channel_mut("transform.x")
            .insert(key(0.0, 0.0, Interpolation::Linear));
        animation
            .channel_mut("transform.x")
            .insert(key(1.0, 100.0, Interpolation::Linear));
        animation.channel_mut("audio.gain").insert(key(
            0.0,
            -6.0,
            Interpolation::Bezier {
                out: [0.1, 1.0],
                incoming: [-0.1, 0.0],
            },
        ));
        let values = animation.sample(Time::from_seconds(0.5));
        assert_eq!(values.f64("transform.x"), Some(50.0));
        assert_eq!(values.f64("audio.gain"), Some(-6.0));
        let mut out = Writer::new();
        ComponentType::encode(&animation, &mut out);
        let bytes = out.into_bytes();
        assert_eq!(
            Animation::decode(1, &mut Reader::new(&bytes)).unwrap(),
            animation
        );
    }
}
