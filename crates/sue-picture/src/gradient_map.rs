//! Gradient maps: a picture's brightness picks a color from a gradient
//! of several stops (Edit, Tonal Correction, Gradient Map, and the
//! Gradient Map correction layer), and the gradient sets people keep and
//! share as files (`docs/gradient-sets.md`).
//!
//! A [`GradientMap`] is a bar of two to [`MAX_STOPS`] stops, each at a
//! place from 0 (black) to 1 (white) with a color, an opacity and where
//! the color comes from (its own, or the main or sub color when the map
//! is used), and a mixing curve that bends brightness before it is
//! looked up. It is `Copy`, so it travels inside a [`crate::filters::Filter`].

use sue_model::Response;

/// The most stops a gradient keeps.
pub const MAX_STOPS: usize = 16;

/// Where a stop's color comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum StopColor {
    /// Its own color.
    #[default]
    Own,
    /// The main drawing color, when the map is used.
    Main,
    /// The sub drawing color, when the map is used.
    Sub,
    /// The color brushes draw with when the gradient is used: the main
    /// color, or the sub color while that one is in front (written `d`;
    /// a Sue from before 0.0.8 skips a gradient that has it).
    Drawing,
}

impl StopColor {
    /// Every kind, in the order a dialog offers them.
    pub const ALL: [Self; 4] = [Self::Own, Self::Main, Self::Sub, Self::Drawing];

    const fn letter(self) -> char {
        match self {
            Self::Own => 'c',
            Self::Main => 'm',
            Self::Sub => 's',
            Self::Drawing => 'd',
        }
    }

    fn of(letter: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| letter.len() == 1 && letter.starts_with(kind.letter()))
    }
}

/// One stop of a gradient.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    /// Its place, 0 (black) to 1 (white).
    pub at: f32,
    /// Its color, sRGB.
    pub rgb: [u8; 3],
    /// How opaque, 0 to 255.
    pub alpha: u8,
    /// Where its color comes from.
    pub color: StopColor,
}

impl Stop {
    /// An opaque stop of its own color.
    #[must_use]
    pub const fn new(at: f32, rgb: [u8; 3]) -> Self {
        Self {
            at,
            rgb,
            alpha: 255,
            color: StopColor::Own,
        }
    }

    const EMPTY: Self = Self::new(0.0, [0, 0, 0]);
}

/// A gradient of stops and its mixing curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientMap {
    stops: [Stop; MAX_STOPS],
    count: u8,
    /// Bends brightness before the stops are looked up (linear leaves
    /// it).
    pub mix: Response,
}

impl Default for GradientMap {
    fn default() -> Self {
        Self::two([0, 0, 0], [255, 255, 255])
    }
}

impl GradientMap {
    /// Black becomes `from`, white becomes `to`: what Sue's gradient maps
    /// were before stops.
    #[must_use]
    pub const fn two(from: [u8; 3], to: [u8; 3]) -> Self {
        let mut stops = [Stop::EMPTY; MAX_STOPS];
        stops[0] = Stop::new(0.0, from);
        stops[1] = Stop::new(1.0, to);
        Self {
            stops,
            count: 2,
            mix: Response::LINEAR,
        }
    }

    /// A gradient of `stops` (two at least, at most [`MAX_STOPS`]; extra
    /// ones are left out), kept in order of place.
    #[must_use]
    pub fn of(stops: &[Stop]) -> Option<Self> {
        if stops.len() < 2 {
            return None;
        }
        let mut map = Self::two([0; 3], [0; 3]);
        map.count = 0;
        for stop in stops.iter().take(MAX_STOPS) {
            map.stops[usize::from(map.count)] = Stop {
                at: stop.at.clamp(0.0, 1.0),
                ..*stop
            };
            map.count += 1;
        }
        map.sort();
        Some(map)
    }

    /// Its stops, in order of place.
    #[must_use]
    pub fn stops(&self) -> &[Stop] {
        &self.stops[..usize::from(self.count)]
    }

    /// Stop `index` to change; call [`Self::sort`] after moving it.
    pub fn stop_mut(&mut self, index: usize) -> Option<&mut Stop> {
        self.stops[..usize::from(self.count)].get_mut(index)
    }

    /// Puts the stops in order of place; answers where the stop that was
    /// at `index` went.
    pub fn sort_keeping(&mut self, index: usize) -> usize {
        let count = usize::from(self.count);
        let mut order: Vec<usize> = (0..count).collect();
        order.sort_by(|a, b| self.stops[*a].at.total_cmp(&self.stops[*b].at));
        let sorted: Vec<Stop> = order.iter().map(|at| self.stops[*at]).collect();
        self.stops[..count].copy_from_slice(&sorted);
        order.iter().position(|at| *at == index).unwrap_or(index)
    }

    /// Puts the stops in order of place.
    pub fn sort(&mut self) {
        self.sort_keeping(0);
    }

    /// Adds a stop at `at` with the color the gradient has there; answers
    /// its index, or `None` when the gradient is full.
    pub fn add_at(&mut self, at: f32) -> Option<usize> {
        if usize::from(self.count) >= MAX_STOPS {
            return None;
        }
        let at = at.clamp(0.0, 1.0);
        let [r, g, b, a] = self.raw_at(at);
        let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        let index = usize::from(self.count);
        self.stops[index] = Stop {
            at,
            rgb: [byte(r), byte(g), byte(b)],
            alpha: byte(a),
            color: StopColor::Own,
        };
        self.count += 1;
        Some(self.sort_keeping(index))
    }

    /// Takes stop `index` away; a gradient keeps two at least.
    pub fn remove(&mut self, index: usize) -> bool {
        let count = usize::from(self.count);
        if count <= 2 || index >= count {
            return false;
        }
        self.stops.copy_within(index + 1..count, index);
        self.count -= 1;
        true
    }

    /// The same gradient with the main and sub colors filled in where its
    /// stops take them.
    #[must_use]
    pub fn with_colors(&self, main: [u8; 3], sub: [u8; 3]) -> Self {
        self.with_drawing(main, sub, main)
    }

    /// [`Self::with_colors`], `drawing` filled in where a stop takes the
    /// color brushes draw with.
    #[must_use]
    pub fn with_drawing(&self, main: [u8; 3], sub: [u8; 3], drawing: [u8; 3]) -> Self {
        let mut out = *self;
        for stop in &mut out.stops[..usize::from(self.count)] {
            match stop.color {
                StopColor::Own => {}
                StopColor::Main => stop.rgb = main,
                StopColor::Sub => stop.rgb = sub,
                StopColor::Drawing => stop.rgb = drawing,
            }
        }
        out
    }

    /// A gradient of two stops from `from` to `to` (each a color source
    /// and an opacity), the colors themselves filled in when it is used.
    #[must_use]
    pub fn sourced(from: (StopColor, u8), to: (StopColor, u8)) -> Self {
        let mut map = Self::two([0; 3], [255; 3]);
        for (index, (color, alpha)) in [from, to].into_iter().enumerate() {
            if let Some(stop) = map.stop_mut(index) {
                stop.color = color;
                stop.alpha = alpha;
            }
        }
        map
    }

    /// The color and opacity at `t` (0 to 1) along the bar, before the
    /// mixing curve: straight, 0 to 1.
    #[must_use]
    pub fn raw_at(&self, t: f32) -> [f32; 4] {
        let stops = self.stops();
        let unit = |stop: &Stop| {
            [
                f32::from(stop.rgb[0]) / 255.0,
                f32::from(stop.rgb[1]) / 255.0,
                f32::from(stop.rgb[2]) / 255.0,
                f32::from(stop.alpha) / 255.0,
            ]
        };
        let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
            return [0.0; 4];
        };
        if t <= first.at {
            return unit(first);
        }
        if t >= last.at {
            return unit(last);
        }
        for pair in stops.windows(2) {
            let (low, high) = (&pair[0], &pair[1]);
            if t <= high.at {
                let span = (high.at - low.at).max(1e-6);
                let k = ((t - low.at) / span).clamp(0.0, 1.0);
                let (a, b) = (unit(low), unit(high));
                return [0, 1, 2, 3].map(|channel| a[channel] + (b[channel] - a[channel]) * k);
            }
        }
        unit(last)
    }

    /// The color and opacity a pixel of brightness `light` (0 to 1) takes:
    /// through the mixing curve, then along the bar.
    #[must_use]
    pub fn at(&self, light: f32) -> [f32; 4] {
        self.raw_at(self.mix.eval(light.clamp(0.0, 1.0)).clamp(0.0, 1.0))
    }

    /// `true` for the two-color form Sue's gradient maps had before stops:
    /// written as the old text, which older Sue reads.
    #[must_use]
    pub fn is_two(&self) -> bool {
        let stops = self.stops();
        stops.len() == 2
            && stops[0].at == 0.0
            && stops[1].at == 1.0
            && stops
                .iter()
                .all(|stop| stop.alpha == 255 && stop.color == StopColor::Own)
            && self.mix == Response::LINEAR
    }

    /// As text: the mixing curve, then each stop as
    /// `place@rrggbbaa@kind`, `;` between them; `|` after the curve.
    #[must_use]
    pub fn to_text(&self) -> String {
        let stops: Vec<String> = self
            .stops()
            .iter()
            .map(|stop| {
                let [r, g, b] = stop.rgb;
                format!(
                    "{}@{r:02x}{g:02x}{b:02x}{:02x}@{}",
                    (stop.at * 10_000.0).round() / 10_000.0,
                    stop.alpha,
                    stop.color.letter()
                )
            })
            .collect();
        format!("{}|{}", self.mix, stops.join(";"))
    }

    /// Reads [`Self::to_text`].
    #[must_use]
    pub fn from_text(text: &str) -> Option<Self> {
        let (mix, stops) = text.split_once('|')?;
        let mix: Response = mix.parse().ok()?;
        let mut read = Vec::new();
        for stop in stops.split(';').filter(|stop| !stop.trim().is_empty()) {
            let mut parts = stop.trim().split('@');
            let at: f32 = parts.next()?.parse().ok()?;
            let hex = parts.next()?;
            let byte = |at: usize| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok();
            let color = StopColor::of(parts.next().unwrap_or("c"))?;
            read.push(Stop {
                at: if at.is_finite() { at } else { 0.0 },
                rgb: [byte(0)?, byte(2)?, byte(4)?],
                alpha: byte(6).unwrap_or(255),
                color,
            });
        }
        let mut map = Self::of(&read)?;
        map.mix = mix;
        Some(map)
    }
}

/// A named gradient of a set.
#[derive(Clone, Debug, PartialEq)]
pub struct NamedGradient {
    /// What people call it.
    pub name: String,
    /// The gradient.
    pub map: GradientMap,
}

/// A gradient set: a named list of gradients, kept as one file people
/// can share (`docs/gradient-sets.md`).
#[derive(Clone, Debug, PartialEq)]
pub struct GradientSet {
    /// What people call the set.
    pub name: String,
    /// Its gradients, in order.
    pub gradients: Vec<NamedGradient>,
}

/// The first line of a gradient set file.
pub const SET_HEADER: &str = "sue-gradient-set 1";

/// The file name ending of a gradient set.
pub const SET_EXTENSION: &str = "suegradients";

impl GradientSet {
    /// The set as its file's text.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = format!("{SET_HEADER}\nname = {}\n", one_line(&self.name));
        for gradient in &self.gradients {
            out.push_str(&format!(
                "gradient = {} = {}\n",
                one_line(&gradient.name).replace(" = ", " - "),
                gradient.map.to_text()
            ));
        }
        out
    }

    /// Reads a set file; lines it does not know are skipped, so a newer
    /// file still opens. `None` when it is not a gradient set at all.
    #[must_use]
    pub fn from_text(text: &str) -> Option<Self> {
        let mut lines = text.lines().map(str::trim);
        let header = lines.find(|line| !line.is_empty() && !line.starts_with('#'))?;
        if !header.starts_with("sue-gradient-set") {
            return None;
        }
        let mut set = Self {
            name: String::new(),
            gradients: Vec::new(),
        };
        for line in lines {
            if let Some(name) = line.strip_prefix("name = ") {
                set.name = name.trim().to_owned();
            } else if let Some(rest) = line.strip_prefix("gradient = ")
                && let Some((name, map)) = rest.split_once(" = ")
                && let Some(map) = GradientMap::from_text(map.trim())
            {
                set.gradients.push(NamedGradient {
                    name: name.trim().to_owned(),
                    map,
                });
            }
        }
        Some(set)
    }

    /// The set Sue ships: a few gradients of its own making.
    #[must_use]
    pub fn builtin() -> Self {
        let named = |name: &str, stops: &[Stop]| NamedGradient {
            name: name.to_owned(),
            map: GradientMap::of(stops).unwrap_or_default(),
        };
        Self {
            name: tr!("gradient-set-sue"),
            gradients: vec![
                NamedGradient {
                    name: tr!("gradient-drawing-colors"),
                    map: {
                        let mut map = GradientMap::two([0; 3], [255; 3]);
                        if let Some(stop) = map.stop_mut(0) {
                            stop.color = StopColor::Main;
                        }
                        if let Some(stop) = map.stop_mut(1) {
                            stop.color = StopColor::Sub;
                        }
                        map
                    },
                },
                named(
                    &tr!("gradient-ink-and-paper"),
                    &[
                        Stop::new(0.0, [28, 30, 38]),
                        Stop::new(1.0, [246, 242, 232]),
                    ],
                ),
                named(
                    &tr!("gradient-dusk"),
                    &[
                        Stop::new(0.0, [24, 20, 60]),
                        Stop::new(0.45, [150, 60, 110]),
                        Stop::new(0.8, [240, 150, 90]),
                        Stop::new(1.0, [255, 236, 190]),
                    ],
                ),
                named(
                    &tr!("gradient-forest"),
                    &[
                        Stop::new(0.0, [10, 28, 20]),
                        Stop::new(0.5, [60, 120, 70]),
                        Stop::new(1.0, [220, 236, 180]),
                    ],
                ),
                named(
                    &tr!("gradient-sea"),
                    &[
                        Stop::new(0.0, [6, 20, 48]),
                        Stop::new(0.55, [30, 110, 160]),
                        Stop::new(1.0, [210, 245, 250]),
                    ],
                ),
                named(
                    &tr!("gradient-sepia"),
                    &[
                        Stop::new(0.0, [40, 26, 14]),
                        Stop::new(0.6, [160, 120, 80]),
                        Stop::new(1.0, [250, 238, 214]),
                    ],
                ),
            ],
        }
    }
}

/// `text` on one line, as a set file keeps a name.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gradient_reads_back_from_its_text() {
        let mut map = GradientMap::of(&[
            Stop::new(0.0, [10, 20, 30]),
            Stop {
                at: 0.4,
                rgb: [200, 100, 0],
                alpha: 128,
                color: StopColor::Main,
            },
            Stop::new(1.0, [250, 250, 240]),
        ])
        .unwrap();
        map.mix = Response::new(&[[0.0, 0.0], [0.5, 0.7], [1.0, 1.0]]);
        let text = map.to_text();
        assert_eq!(GradientMap::from_text(&text), Some(map), "{text}");
        // The drawing color as a source reads back, and is filled in.
        let drawing = GradientMap::sourced((StopColor::Drawing, 255), (StopColor::Drawing, 0));
        let text = drawing.to_text();
        assert!(text.contains("@d"), "{text}");
        assert_eq!(GradientMap::from_text(&text), Some(drawing));
        let used = drawing.with_drawing([1; 3], [2; 3], [3; 3]);
        assert_eq!(used.stops()[0].rgb, [3; 3]);
        assert_eq!((used.stops()[1].rgb, used.stops()[1].alpha), ([3; 3], 0));
        assert!(!map.is_two());
        assert!(GradientMap::two([1, 2, 3], [4, 5, 6]).is_two());
        // Not a gradient.
        assert_eq!(GradientMap::from_text("nonsense"), None);
        assert_eq!(GradientMap::from_text("0,0 1,1|0@000000ff@c"), None);
    }

    #[test]
    fn stops_are_looked_up_in_order_and_mixed_through_the_curve() {
        let map = GradientMap::of(&[
            Stop::new(1.0, [255, 255, 255]),
            Stop::new(0.0, [0, 0, 0]),
            Stop::new(0.5, [255, 0, 0]),
        ])
        .unwrap();
        assert_eq!(map.stops()[1].rgb, [255, 0, 0], "sorted by place");
        assert_eq!(map.at(0.0), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(map.at(0.5), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(map.at(1.0), [1.0, 1.0, 1.0, 1.0]);
        let quarter = map.at(0.25);
        assert!((quarter[0] - 0.5).abs() < 1e-4 && quarter[1].abs() < 1e-4);
        // A curve that holds everything at the middle.
        let mut flat = map;
        flat.mix = Response::new(&[[0.0, 0.5], [1.0, 0.5]]);
        assert_eq!(flat.at(0.1), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn stops_are_added_removed_and_take_the_drawing_colors() {
        let mut map = GradientMap::two([0, 0, 0], [255, 255, 255]);
        let index = map.add_at(0.5).unwrap();
        assert_eq!(index, 1);
        assert_eq!(
            map.stops()[1].rgb,
            [128, 128, 128],
            "the color it had there"
        );
        for _ in 3..MAX_STOPS {
            assert!(map.add_at(0.7).is_some());
        }
        assert_eq!(map.add_at(0.2), None, "full");
        while map.remove(0) {}
        assert_eq!(map.stops().len(), 2, "two stay");
        let mut drawing = GradientMap::two([9, 9, 9], [8, 8, 8]);
        drawing.stop_mut(0).unwrap().color = StopColor::Main;
        drawing.stop_mut(1).unwrap().color = StopColor::Sub;
        let used = drawing.with_colors([1, 2, 3], [4, 5, 6]);
        assert_eq!(used.stops()[0].rgb, [1, 2, 3]);
        assert_eq!(used.stops()[1].rgb, [4, 5, 6]);
        // Moving a stop past another keeps track of it.
        let mut moved = GradientMap::of(&[
            Stop::new(0.0, [0; 3]),
            Stop::new(0.5, [1; 3]),
            Stop::new(1.0, [2; 3]),
        ])
        .unwrap();
        moved.stop_mut(0).unwrap().at = 0.75;
        assert_eq!(moved.sort_keeping(0), 1);
        assert_eq!(moved.stops()[1].rgb, [0; 3]);
    }

    #[test]
    fn a_set_reads_back_from_its_file_and_skips_what_it_does_not_know() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let set = GradientSet::builtin();
        assert!(set.gradients.len() >= 4);
        let text = set.to_text();
        assert!(text.starts_with(SET_HEADER));
        assert_eq!(GradientSet::from_text(&text), Some(set));
        let newer = format!(
            "{SET_HEADER}\nname = Mine\nshine = yes\ngradient = A = 0,0 1,1|0@000000ff@c;1@ffffffff@c\n"
        );
        let read = GradientSet::from_text(&newer).unwrap();
        assert_eq!(read.name, "Mine");
        assert_eq!(read.gradients.len(), 1);
        assert_eq!(GradientSet::from_text("hello\n"), None);
    }
}
