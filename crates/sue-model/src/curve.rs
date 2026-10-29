//! Animation curves. Keyframed motion graphics, 2D and 3D animation, rig
//! poses, DAW automation lanes, color grading keyframes and game tweens are
//! all a value that changes over [`Time`]. A [`Curve`] holds the keys; an
//! [`Animation`] maps property keys (the same keys as entity properties,
//! e.g. `transform.x` or `audio.gain`) to curves, so anything that has a
//! property can be animated the same way.
//!
//! A [`Response`] is the other kind of curve: not over time but from one
//! value to another, 0 to 1 in and out. Pen pressure curves, brush
//! dynamics, tone curves and level curves are all one.

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

/// A response curve: how an input from 0 to 1 (pen pressure, a tone, a
/// level, a speed) becomes an output from 0 to 1, shaped by a few control
/// points. The curve is a smooth B-spline pulled toward the points (it
/// passes through the first and the last), so a handful of points bend it
/// gently without overshoot, the way paint programs draw pressure graphs.
/// The first point sits at input 0 and the last at input 1; between them
/// the inputs rise.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Response {
    points: [[f32; 2]; Response::MAX_POINTS],
    len: u8,
}

impl Default for Response {
    fn default() -> Self {
        Self::LINEAR
    }
}

impl Response {
    /// The most control points a curve keeps.
    pub const MAX_POINTS: usize = 8;

    /// Output equals input.
    pub const LINEAR: Self = Self {
        points: [
            [0.0, 0.0],
            [1.0, 1.0],
            [0.0; 2],
            [0.0; 2],
            [0.0; 2],
            [0.0; 2],
            [0.0; 2],
            [0.0; 2],
        ],
        len: 2,
    };

    /// Inputs closer than this count as one point.
    const MIN_GAP: f32 = 0.01;

    /// A curve through `points` (sorted by input, clamped to 0 to 1, near
    /// duplicates merged, at most [`Self::MAX_POINTS`]). The ends are moved
    /// to inputs 0 and 1; fewer than two points give [`Self::LINEAR`].
    #[must_use]
    pub fn new(points: &[[f32; 2]]) -> Self {
        let mut sorted: Vec<[f32; 2]> = points
            .iter()
            .filter(|point| point.iter().all(|value| value.is_finite()))
            .map(|&[x, y]| [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)])
            .collect();
        sorted.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut kept: Vec<[f32; 2]> = Vec::with_capacity(sorted.len());
        for point in sorted {
            match kept.last_mut() {
                Some(last) if point[0] - last[0] < Self::MIN_GAP => *last = point,
                _ => kept.push(point),
            }
        }
        if kept.len() < 2 {
            return Self::LINEAR;
        }
        if kept.len() > Self::MAX_POINTS {
            // Keep the ends and the points spread evenly between them.
            let last = kept.len() - 1;
            kept = (0..Self::MAX_POINTS)
                .map(|i| kept[i * last / (Self::MAX_POINTS - 1)])
                .collect();
        }
        let mut out = Self {
            points: [[0.0; 2]; Self::MAX_POINTS],
            len: kept.len() as u8,
        };
        out.points[..kept.len()].copy_from_slice(&kept);
        out.points[0][0] = 0.0;
        out.points[kept.len() - 1][0] = 1.0;
        out
    }

    /// The control points, by input.
    #[must_use]
    pub fn points(&self) -> &[[f32; 2]] {
        &self.points[..usize::from(self.len)]
    }

    /// `true` if output equals input everywhere.
    #[must_use]
    pub fn is_linear(&self) -> bool {
        self.points().iter().all(|[x, y]| (x - y).abs() < 1e-4)
    }

    /// Adds a control point. Returns its index, or `None` when the curve
    /// is full or a point already sits at that input.
    pub fn insert(&mut self, point: [f32; 2]) -> Option<usize> {
        let [x, y] = [point[0].clamp(0.0, 1.0), point[1].clamp(0.0, 1.0)];
        let points = self.points();
        if points.len() >= Self::MAX_POINTS
            || points
                .iter()
                .any(|known| (known[0] - x).abs() < Self::MIN_GAP)
        {
            return None;
        }
        let index = points.partition_point(|known| known[0] < x);
        let len = usize::from(self.len);
        self.points.copy_within(index..len, index + 1);
        self.points[index] = [x, y];
        self.len += 1;
        Some(index)
    }

    /// Removes control point `index`. The ends stay; returns `false` for
    /// them.
    pub fn remove(&mut self, index: usize) -> bool {
        let len = usize::from(self.len);
        if index == 0 || index + 1 >= len {
            return false;
        }
        self.points.copy_within(index + 1..len, index);
        self.len -= 1;
        true
    }

    /// Moves control point `index` to `point`: between its neighbors'
    /// inputs, and only up and down for the ends.
    pub fn set(&mut self, index: usize, point: [f32; 2]) {
        let len = usize::from(self.len);
        if index >= len {
            return;
        }
        let y = point[1].clamp(0.0, 1.0);
        let x = if index == 0 {
            0.0
        } else if index + 1 == len {
            1.0
        } else {
            let low = self.points[index - 1][0] + Self::MIN_GAP;
            let high = self.points[index + 1][0] - Self::MIN_GAP;
            point[0].clamp(low, high.max(low))
        };
        self.points[index] = [x, y];
    }

    /// The same curve with every output put through `map` (for example a
    /// power to make it stronger or lighter).
    #[must_use]
    pub fn map_outputs(&self, map: impl Fn(f32) -> f32) -> Self {
        let mut out = *self;
        for point in &mut out.points[..usize::from(self.len)] {
            point[1] = map(point[1]).clamp(0.0, 1.0);
        }
        out
    }

    /// The output for `input` (clamped to 0 to 1).
    #[must_use]
    pub fn eval(&self, input: f32) -> f32 {
        let x = if input.is_nan() {
            0.0
        } else {
            input.clamp(0.0, 1.0)
        };
        if self.len == 2 {
            let [[x0, y0], [x1, y1]] = [self.points[0], self.points[1]];
            return y0 + (y1 - y0) * ((x - x0) / (x1 - x0).max(1e-6));
        }
        // The spline's input rises with its parameter, so find the
        // parameter at `x` by bisection.
        let (mut low, mut high) = (0.0_f32, 1.0_f32);
        for _ in 0..24 {
            let middle = 0.5 * (low + high);
            if self.at(middle)[0] < x {
                low = middle;
            } else {
                high = middle;
            }
        }
        self.at(0.5 * (low + high))[1].clamp(0.0, 1.0)
    }

    /// The point of the clamped uniform B-spline (degree up to 3) at
    /// parameter `t`, by de Boor's algorithm.
    #[must_use]
    pub fn at(&self, t: f32) -> [f32; 2] {
        let n = usize::from(self.len);
        let degree = (n - 1).min(3);
        let knot = |i: usize| -> f32 {
            if i <= degree {
                0.0
            } else if i >= n {
                1.0
            } else {
                (i - degree) as f32 / (n - degree) as f32
            }
        };
        let t = t.clamp(0.0, 1.0);
        let mut span = degree;
        while span + 1 < n && t >= knot(span + 1) {
            span += 1;
        }
        let mut d = [[0.0_f32; 2]; 4];
        for (j, point) in d.iter_mut().enumerate().take(degree + 1) {
            *point = self.points[span - degree + j];
        }
        for r in 1..=degree {
            for j in (r..=degree).rev() {
                let i = span - degree + j;
                let width = knot(i + degree + 1 - r) - knot(i);
                let alpha = if width > 0.0 {
                    (t - knot(i)) / width
                } else {
                    0.0
                };
                d[j] = [
                    d[j - 1][0] + (d[j][0] - d[j - 1][0]) * alpha,
                    d[j - 1][1] + (d[j][1] - d[j - 1][1]) * alpha,
                ];
            }
        }
        d[degree]
    }
}

impl std::fmt::Display for Response {
    /// `0,0 0.5,0.35 1,1`: each point as input,output.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, [x, y]) in self.points().iter().enumerate() {
            if index > 0 {
                f.write_str(" ")?;
            }
            let round = |value: f32| (value * 1000.0).round() / 1000.0;
            write!(f, "{},{}", round(*x), round(*y))?;
        }
        Ok(())
    }
}

impl std::str::FromStr for Response {
    type Err = String;

    /// Reads `0,0 0.5,0.35 1,1`; empty text is [`Response::LINEAR`].
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut points = Vec::new();
        for pair in text.split_whitespace() {
            let (x, y) = pair
                .split_once(',')
                .ok_or_else(|| format!("{pair} is not input,output"))?;
            let number = |value: &str| {
                value
                    .trim()
                    .parse::<f32>()
                    .map_err(|_| format!("{value} is not a number"))
            };
            points.push([number(x)?, number(y)?]);
        }
        Ok(Self::new(&points))
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

    #[test]
    fn a_linear_response_gives_back_its_input() {
        let linear = Response::LINEAR;
        assert!(linear.is_linear());
        for step in 0..=10 {
            let x = step as f32 / 10.0;
            assert!((linear.eval(x) - x).abs() < 1e-5);
        }
        assert_eq!(linear.eval(-1.0), 0.0);
        assert_eq!(linear.eval(2.0), 1.0);
    }

    #[test]
    fn a_response_is_smooth_through_its_ends_and_bends_toward_its_points() {
        // Clip Studio Paint's soft pen: light presses stay light.
        let soft = Response::new(&[[0.0, 0.0], [0.8, 0.3], [1.0, 1.0]]);
        assert!((soft.eval(0.0)).abs() < 1e-4);
        assert!((soft.eval(1.0) - 1.0).abs() < 1e-4);
        let middle = soft.eval(0.5);
        assert!(middle < 0.4, "pulled under the diagonal: {middle}");
        assert!(middle > soft.eval(0.3));
        // It rises with the input everywhere, with no overshoot.
        let mut last = -1.0;
        for step in 0..=100 {
            let y = soft.eval(step as f32 / 100.0);
            assert!(y >= last - 1e-4 && (0.0..=1.0).contains(&y));
            last = y;
        }
        // Many points: still smooth and still near them.
        let wavy = Response::new(&[
            [0.0, 0.1],
            [0.2, 0.15],
            [0.4, 0.5],
            [0.6, 0.55],
            [0.8, 0.9],
            [1.0, 1.0],
        ]);
        assert!((wavy.eval(0.0) - 0.1).abs() < 1e-4);
        assert!((wavy.eval(0.5) - 0.52).abs() < 0.08);
    }

    #[test]
    fn response_points_are_kept_in_order_and_ends_stay_put() {
        let mut curve = Response::LINEAR;
        assert_eq!(curve.insert([0.5, 0.2]), Some(1));
        assert_eq!(curve.insert([0.5, 0.9]), None, "one point per input");
        assert_eq!(curve.insert([0.25, 0.1]), Some(1));
        assert_eq!(curve.points().len(), 4);
        // Dragging past a neighbor stops at it; the ends only go up and down.
        curve.set(1, [0.9, 0.4]);
        assert!(curve.points()[1][0] < 0.5);
        curve.set(0, [0.3, 0.2]);
        assert_eq!(curve.points()[0], [0.0, 0.2]);
        assert!(!curve.remove(0), "the ends stay");
        assert!(curve.remove(1));
        assert_eq!(curve.points().len(), 3);
        for _ in 0..20 {
            let x = curve.points()[1][0] * 0.5 + 0.01;
            curve.insert([x, 0.5]);
        }
        assert_eq!(curve.points().len(), Response::MAX_POINTS);
        let stronger = curve.map_outputs(|y| y.powf(0.5));
        assert!(stronger.eval(0.5) > curve.eval(0.5));
    }

    #[test]
    fn a_response_reads_back_from_its_text() {
        let curve = Response::new(&[[0.0, 0.05], [0.4, 0.2], [1.0, 1.0]]);
        let text = curve.to_string();
        assert_eq!(text, "0,0.05 0.4,0.2 1,1");
        assert_eq!(text.parse::<Response>().unwrap(), curve);
        assert_eq!("".parse::<Response>().unwrap(), Response::LINEAR);
        assert!("0,0 nope".parse::<Response>().is_err());
        // Out of order, out of range and too many points come out tidy.
        let messy: Response = "1,2 0.5,0.5 -1,0".parse().unwrap();
        assert_eq!(messy.points(), &[[0.0, 0.0], [0.5, 0.5], [1.0, 1.0]]);
    }
}
