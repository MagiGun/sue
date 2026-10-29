//! The shapes of an SVG drawing as polylines, for tools that turn vector
//! drawings into their own lines (File, Import, Vector): paths (every
//! command, curves and arcs flattened), lines, polylines, polygons,
//! rectangles, circles and ellipses, with their stroke, fill and width,
//! through every `transform` and the `viewBox`.
//!
//! Text, gradients, patterns, masks and clips are left out.

/// One shape of the drawing.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgShape {
    /// Its outline, in the drawing's pixels.
    pub points: Vec<[f32; 2]>,
    /// The outline closes on itself.
    pub closed: bool,
    /// Its stroke color (sRGB), if stroked.
    pub stroke: Option<[u8; 3]>,
    /// Its fill color (sRGB), if filled.
    pub fill: Option<[u8; 3]>,
    /// Its stroke width, in the drawing's pixels.
    pub width: f32,
}

/// An SVG drawing: its size and its shapes, in drawing order.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgDrawing {
    /// Width in pixels.
    pub width: f32,
    /// Height in pixels.
    pub height: f32,
    /// The shapes.
    pub shapes: Vec<SvgShape>,
}

/// An affine map `[a, b, c, d, e, f]`: `x' = a x + c y + e`,
/// `y' = b x + d y + f`.
type Affine = [f32; 6];

const IDENTITY: Affine = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn then(outer: Affine, inner: Affine) -> Affine {
    [
        outer[0] * inner[0] + outer[2] * inner[1],
        outer[1] * inner[0] + outer[3] * inner[1],
        outer[0] * inner[2] + outer[2] * inner[3],
        outer[1] * inner[2] + outer[3] * inner[3],
        outer[0] * inner[4] + outer[2] * inner[5] + outer[4],
        outer[1] * inner[4] + outer[3] * inner[5] + outer[5],
    ]
}

fn map(m: Affine, p: [f32; 2]) -> [f32; 2] {
    [
        m[0] * p[0] + m[2] * p[1] + m[4],
        m[1] * p[0] + m[3] * p[1] + m[5],
    ]
}

/// What a group passes to what is inside it.
#[derive(Clone, Copy)]
struct Style {
    transform: Affine,
    stroke: Option<[u8; 3]>,
    fill: Option<[u8; 3]>,
    width: f32,
}

/// Reads the shapes of the SVG `text`.
///
/// # Errors
/// If it has no `<svg>` element.
pub fn parse_svg(text: &str) -> Result<SvgDrawing, String> {
    let mut drawing = SvgDrawing {
        width: 0.0,
        height: 0.0,
        shapes: Vec::new(),
    };
    let mut stack: Vec<Style> = Vec::new();
    let mut seen_svg = false;
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        if rest.starts_with("!--") {
            rest = rest.find("-->").map_or("", |end| &rest[end + 3..]);
            continue;
        }
        if rest.starts_with('?') || rest.starts_with('!') {
            rest = rest.find('>').map_or("", |end| &rest[end + 1..]);
            continue;
        }
        let Some(end) = tag_end(rest) else {
            break;
        };
        let tag = &rest[..end];
        rest = &rest[end + 1..];
        if let Some(closing) = tag.strip_prefix('/') {
            if matches!(closing.trim(), "g" | "svg" | "a") {
                stack.pop();
            }
            continue;
        }
        let self_closing = tag.trim_end().ends_with('/');
        let tag = tag.trim_end().trim_end_matches('/');
        let name_end = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
        let name = &tag[..name_end];
        let attributes = attributes(&tag[name_end..]);
        let get = |key: &str| {
            attributes
                .iter()
                .find(|(found, _)| found == key)
                .map(|(_, value)| value.as_str())
        };
        let parent = stack.last().copied().unwrap_or(Style {
            transform: IDENTITY,
            stroke: None,
            fill: Some([0, 0, 0]),
            width: 1.0,
        });
        let mut style = parent;
        if let Some(transform) = get("transform") {
            style.transform = then(parent.transform, parse_transform(transform));
        }
        let mut paint = |key: &str, value: &str| match key {
            "stroke" => style.stroke = color(value),
            "fill" => style.fill = color(value),
            "stroke-width" => {
                if let Some(width) = number_prefix(value) {
                    style.width = width;
                }
            }
            _ => {}
        };
        for key in ["stroke", "fill", "stroke-width"] {
            if let Some(value) = get(key) {
                paint(key, value);
            }
        }
        if let Some(inline) = get("style") {
            for rule in inline.split(';') {
                if let Some((key, value)) = rule.split_once(':') {
                    paint(key.trim(), value.trim());
                }
            }
        }
        if name == "svg" && !seen_svg {
            seen_svg = true;
            let size = |key: &str| get(key).and_then(number_prefix);
            let view: Vec<f32> = get("viewBox").map(numbers).unwrap_or_default();
            let (width, height) = match (size("width"), size("height"), view.len() == 4) {
                (Some(w), Some(h), _) => (w, h),
                (_, _, true) => (view[2], view[3]),
                _ => (300.0, 150.0),
            };
            drawing.width = width;
            drawing.height = height;
            if view.len() == 4 && view[2] > 0.0 && view[3] > 0.0 {
                let (sx, sy) = (width / view[2], height / view[3]);
                let scale = sx.min(sy);
                style.transform = then(
                    style.transform,
                    [scale, 0.0, 0.0, scale, -view[0] * scale, -view[1] * scale],
                );
            }
        }
        if matches!(name, "g" | "svg" | "a") {
            if !self_closing {
                stack.push(style);
            }
            continue;
        }
        let number = |key: &str| get(key).and_then(number_prefix).unwrap_or(0.0);
        let outlines: Vec<(Vec<[f32; 2]>, bool)> = match name {
            "path" => get("d").map(path).unwrap_or_default(),
            "line" => vec![(
                vec![[number("x1"), number("y1")], [number("x2"), number("y2")]],
                false,
            )],
            "polyline" | "polygon" => {
                let values = get("points").map(numbers).unwrap_or_default();
                let points: Vec<[f32; 2]> = values.as_chunks::<2>().0.to_vec();
                vec![(points, name == "polygon")]
            }
            "rect" => {
                let (x, y, w, h) = (number("x"), number("y"), number("width"), number("height"));
                vec![(vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]], true)]
            }
            "circle" | "ellipse" => {
                let (cx, cy) = (number("cx"), number("cy"));
                let (rx, ry) = if name == "circle" {
                    (number("r"), number("r"))
                } else {
                    (number("rx"), number("ry"))
                };
                let steps = 48;
                let points = (0..steps)
                    .map(|step| {
                        let angle = step as f32 / steps as f32 * std::f32::consts::TAU;
                        [cx + rx * angle.cos(), cy + ry * angle.sin()]
                    })
                    .collect();
                vec![(points, true)]
            }
            _ => Vec::new(),
        };
        let scale = (style.transform[0] * style.transform[3]
            - style.transform[1] * style.transform[2])
            .abs()
            .sqrt();
        for (points, closed) in outlines {
            if points.len() < 2 || (style.stroke.is_none() && style.fill.is_none()) {
                continue;
            }
            drawing.shapes.push(SvgShape {
                points: points
                    .into_iter()
                    .map(|p| map(style.transform, p))
                    .collect(),
                closed,
                stroke: style.stroke,
                fill: style.fill,
                width: style.width * scale,
            });
        }
    }
    if !seen_svg {
        return Err("not an SVG drawing".to_owned());
    }
    Ok(drawing)
}

/// Where the tag that `rest` starts ends (its `>`), past quoted values.
fn tag_end(rest: &str) -> Option<usize> {
    let mut quote = None;
    for (index, c) in rest.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(open), _) if c == open => quote = None,
            (None, '>') => return Some(index),
            _ => {}
        }
    }
    None
}

/// `key="value"` pairs.
fn attributes(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq].trim().to_owned();
        let after = rest[eq + 1..].trim_start();
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            break;
        };
        let body = &after[1..];
        let Some(close) = body.find(quote) else {
            break;
        };
        out.push((key, unescape(&body[..close])));
        rest = &body[close + 1..];
    }
    out
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The number at the start of `text` (`12px` is 12).
fn number_prefix(text: &str) -> Option<f32> {
    let text = text.trim();
    let end = text
        .char_indices()
        .find(|(index, c)| {
            !(c.is_ascii_digit()
                || *c == '.'
                || ((*c == '-' || *c == '+') && *index == 0)
                || *c == 'e')
        })
        .map_or(text.len(), |(index, _)| index);
    text[..end].parse().ok()
}

/// Every number in `text`, however separated (`1,2 3-4` is four).
fn numbers(text: &str) -> Vec<f32> {
    let mut scanner = Scanner::new(text);
    let mut out = Vec::new();
    while let Some(value) = scanner.number() {
        out.push(value);
    }
    out
}

fn parse_transform(text: &str) -> Affine {
    let mut out = IDENTITY;
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let values = numbers(&rest[open + 1..open + close]);
        let v = |index: usize, default: f32| values.get(index).copied().unwrap_or(default);
        let step = match name {
            "matrix" if values.len() == 6 => [
                v(0, 1.0),
                v(1, 0.0),
                v(2, 0.0),
                v(3, 1.0),
                v(4, 0.0),
                v(5, 0.0),
            ],
            "translate" => [1.0, 0.0, 0.0, 1.0, v(0, 0.0), v(1, 0.0)],
            "scale" => [v(0, 1.0), 0.0, 0.0, v(1, v(0, 1.0)), 0.0, 0.0],
            "rotate" => {
                let (sin, cos) = v(0, 0.0).to_radians().sin_cos();
                let (cx, cy) = (v(1, 0.0), v(2, 0.0));
                then(
                    then(
                        [1.0, 0.0, 0.0, 1.0, cx, cy],
                        [cos, sin, -sin, cos, 0.0, 0.0],
                    ),
                    [1.0, 0.0, 0.0, 1.0, -cx, -cy],
                )
            }
            "skewX" => [1.0, 0.0, v(0, 0.0).to_radians().tan(), 1.0, 0.0, 0.0],
            "skewY" => [1.0, v(0, 0.0).to_radians().tan(), 0.0, 1.0, 0.0, 0.0],
            _ => IDENTITY,
        };
        out = then(out, step);
        rest = &rest[open + close + 1..];
    }
    out
}

/// A color, or `None` for `none` and colors this reader does not know.
fn color(text: &str) -> Option<[u8; 3]> {
    let text = text.trim().to_ascii_lowercase();
    if let Some(hex) = text.strip_prefix('#') {
        let digit = |index: usize| u8::from_str_radix(hex.get(index..=index)?, 16).ok();
        return match hex.len() {
            3 | 4 => Some([digit(0)? * 17, digit(1)? * 17, digit(2)? * 17]),
            6 | 8 => Some([
                digit(0)? * 16 + digit(1)?,
                digit(2)? * 16 + digit(3)?,
                digit(4)? * 16 + digit(5)?,
            ]),
            _ => None,
        };
    }
    if let Some(inside) = text
        .strip_prefix("rgb(")
        .or_else(|| text.strip_prefix("rgba("))
    {
        let values: Vec<f32> = inside
            .trim_end_matches(')')
            .split(',')
            .take(3)
            .filter_map(|part| {
                let part = part.trim();
                part.strip_suffix('%').map_or_else(
                    || part.parse().ok(),
                    |percent| percent.parse::<f32>().ok().map(|p| p * 2.55),
                )
            })
            .collect();
        return (values.len() == 3)
            .then(|| [0, 1, 2].map(|index| values[index].round().clamp(0.0, 255.0) as u8));
    }
    match text.as_str() {
        "black" | "currentcolor" => Some([0, 0, 0]),
        "white" => Some([255, 255, 255]),
        "red" => Some([255, 0, 0]),
        "green" => Some([0, 128, 0]),
        "lime" => Some([0, 255, 0]),
        "blue" => Some([0, 0, 255]),
        "yellow" => Some([255, 255, 0]),
        "cyan" | "aqua" => Some([0, 255, 255]),
        "magenta" | "fuchsia" => Some([255, 0, 255]),
        "gray" | "grey" => Some([128, 128, 128]),
        "silver" => Some([192, 192, 192]),
        "orange" => Some([255, 165, 0]),
        "purple" => Some([128, 0, 128]),
        "brown" => Some([165, 42, 42]),
        "navy" => Some([0, 0, 128]),
        "pink" => Some([255, 192, 203]),
        _ => None,
    }
}

/// Reads numbers and flags out of path data.
struct Scanner<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            at: 0,
        }
    }

    fn skip(&mut self) {
        while self.at < self.bytes.len()
            && (self.bytes[self.at].is_ascii_whitespace() || self.bytes[self.at] == b',')
        {
            self.at += 1;
        }
    }

    fn command(&mut self) -> Option<u8> {
        self.skip();
        let c = *self.bytes.get(self.at)?;
        if c.is_ascii_alphabetic() && c != b'e' && c != b'E' {
            self.at += 1;
            Some(c)
        } else {
            None
        }
    }

    fn starts_number(&mut self) -> bool {
        self.skip();
        self.bytes
            .get(self.at)
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.'))
    }

    fn number(&mut self) -> Option<f32> {
        if !self.starts_number() {
            return None;
        }
        let start = self.at;
        let mut seen_dot = false;
        let mut seen_e = false;
        if matches!(self.bytes[self.at], b'-' | b'+') {
            self.at += 1;
        }
        while let Some(&c) = self.bytes.get(self.at) {
            match c {
                b'0'..=b'9' => self.at += 1,
                b'.' if !seen_dot && !seen_e => {
                    seen_dot = true;
                    self.at += 1;
                }
                b'e' | b'E' if !seen_e => {
                    seen_e = true;
                    self.at += 1;
                    if matches!(self.bytes.get(self.at), Some(b'-' | b'+')) {
                        self.at += 1;
                    }
                }
                _ => break,
            }
        }
        std::str::from_utf8(&self.bytes[start..self.at])
            .ok()?
            .parse()
            .ok()
    }

    /// An arc flag: one digit, which may touch the next number.
    fn flag(&mut self) -> Option<bool> {
        self.skip();
        let c = *self.bytes.get(self.at)?;
        self.at += 1;
        match c {
            b'0' => Some(false),
            b'1' => Some(true),
            _ => None,
        }
    }
}

/// Points along each curve piece.
const CURVE_STEPS: usize = 16;

/// The subpaths of path data `d`, each flattened, and whether it closes.
fn path(d: &str) -> Vec<(Vec<[f32; 2]>, bool)> {
    let mut out = Vec::new();
    let mut scanner = Scanner::new(d);
    let mut current: Vec<[f32; 2]> = Vec::new();
    let mut at = [0.0_f32; 2];
    let mut start = [0.0_f32; 2];
    let mut last_control: Option<[f32; 2]> = None;
    let mut last_quad: Option<[f32; 2]> = None;
    let mut command = b'M';
    let finish =
        |current: &mut Vec<[f32; 2]>, out: &mut Vec<(Vec<[f32; 2]>, bool)>, closed: bool| {
            if current.len() >= 2 {
                out.push((std::mem::take(current), closed));
            } else {
                current.clear();
            }
        };
    loop {
        if let Some(next) = scanner.command() {
            command = next;
        } else if !scanner.starts_number() {
            break;
        }
        let relative = command.is_ascii_lowercase();
        let base = if relative { at } else { [0.0, 0.0] };
        let point = |scanner: &mut Scanner<'_>| -> Option<[f32; 2]> {
            Some([scanner.number()? + base[0], scanner.number()? + base[1]])
        };
        let upper = command.to_ascii_uppercase();
        match upper {
            b'M' => {
                let Some(to) = point(&mut scanner) else {
                    break;
                };
                finish(&mut current, &mut out, false);
                at = to;
                start = to;
                current.push(to);
                // Further pairs after a move are lines.
                command = if relative { b'l' } else { b'L' };
            }
            b'L' => {
                let Some(to) = point(&mut scanner) else {
                    break;
                };
                at = to;
                current.push(to);
            }
            b'H' => {
                let Some(x) = scanner.number() else {
                    break;
                };
                at = [x + if relative { at[0] } else { 0.0 }, at[1]];
                current.push(at);
            }
            b'V' => {
                let Some(y) = scanner.number() else {
                    break;
                };
                at = [at[0], y + if relative { at[1] } else { 0.0 }];
                current.push(at);
            }
            b'C' | b'S' => {
                let first = if upper == b'C' {
                    let Some(first) = point(&mut scanner) else {
                        break;
                    };
                    first
                } else {
                    last_control.map_or(at, |control| {
                        [2.0 * at[0] - control[0], 2.0 * at[1] - control[1]]
                    })
                };
                let (Some(second), Some(to)) = (point(&mut scanner), point(&mut scanner)) else {
                    break;
                };
                if current.is_empty() {
                    current.push(at);
                }
                for step in 1..=CURVE_STEPS {
                    let t = step as f32 / CURVE_STEPS as f32;
                    let u = 1.0 - t;
                    let mix = |i: usize| {
                        u * u * u * at[i]
                            + 3.0 * u * u * t * first[i]
                            + 3.0 * u * t * t * second[i]
                            + t * t * t * to[i]
                    };
                    current.push([mix(0), mix(1)]);
                }
                last_control = Some(second);
                at = to;
                last_quad = None;
                continue;
            }
            b'Q' | b'T' => {
                let control = if upper == b'Q' {
                    let Some(control) = point(&mut scanner) else {
                        break;
                    };
                    control
                } else {
                    last_quad.map_or(at, |control| {
                        [2.0 * at[0] - control[0], 2.0 * at[1] - control[1]]
                    })
                };
                let Some(to) = point(&mut scanner) else {
                    break;
                };
                if current.is_empty() {
                    current.push(at);
                }
                for step in 1..=CURVE_STEPS {
                    let t = step as f32 / CURVE_STEPS as f32;
                    let u = 1.0 - t;
                    let mix = |i: usize| u * u * at[i] + 2.0 * u * t * control[i] + t * t * to[i];
                    current.push([mix(0), mix(1)]);
                }
                last_quad = Some(control);
                at = to;
                last_control = None;
                continue;
            }
            b'A' => {
                let (Some(rx), Some(ry), Some(rotation)) =
                    (scanner.number(), scanner.number(), scanner.number())
                else {
                    break;
                };
                let (Some(large), Some(sweep)) = (scanner.flag(), scanner.flag()) else {
                    break;
                };
                let Some(to) = point(&mut scanner) else {
                    break;
                };
                if current.is_empty() {
                    current.push(at);
                }
                current.extend(arc(at, to, rx, ry, rotation, large, sweep));
                at = to;
            }
            b'Z' => {
                if current.len() >= 2 {
                    finish(&mut current, &mut out, true);
                }
                at = start;
                current.clear();
                current.push(start);
            }
            _ => {
                // An unknown command: skip its numbers.
                while scanner.number().is_some() {}
            }
        }
        last_control = None;
        last_quad = None;
    }
    finish(&mut current, &mut out, false);
    out
}

/// An elliptical arc from `from` to `to`, flattened (SVG's endpoint form).
fn arc(
    from: [f32; 2],
    to: [f32; 2],
    rx: f32,
    ry: f32,
    rotation: f32,
    large: bool,
    sweep: bool,
) -> Vec<[f32; 2]> {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 || from == to {
        return vec![to];
    }
    let (sin, cos) = rotation.to_radians().sin_cos();
    let (dx, dy) = ((from[0] - to[0]) / 2.0, (from[1] - to[1]) / 2.0);
    let (x1, y1) = (cos * dx + sin * dy, -sin * dx + cos * dy);
    let grow = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if grow > 1.0 {
        rx *= grow.sqrt();
        ry *= grow.sqrt();
    }
    let numerator = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut factor = (numerator / denominator).max(0.0).sqrt();
    if large == sweep {
        factor = -factor;
    }
    let (cx1, cy1) = (factor * rx * y1 / ry, -factor * ry * x1 / rx);
    let center = [
        cos * cx1 - sin * cy1 + (from[0] + to[0]) / 2.0,
        sin * cx1 + cos * cy1 + (from[1] + to[1]) / 2.0,
    ];
    let angle = |ux: f32, uy: f32| uy.atan2(ux);
    let start = angle((x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut sweep_angle = angle((-x1 - cx1) / rx, (-y1 - cy1) / ry) - start;
    if sweep && sweep_angle < 0.0 {
        sweep_angle += std::f32::consts::TAU;
    } else if !sweep && sweep_angle > 0.0 {
        sweep_angle -= std::f32::consts::TAU;
    }
    let steps = ((sweep_angle.abs() / std::f32::consts::TAU) * 48.0)
        .ceil()
        .max(2.0) as usize;
    (1..=steps)
        .map(|step| {
            let theta = start + sweep_angle * step as f32 / steps as f32;
            let (x, y) = (rx * theta.cos(), ry * theta.sin());
            [center[0] + cos * x - sin * y, center[1] + sin * x + cos * y]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 0.01 && (a[1] - b[1]).abs() < 0.01
    }

    #[test]
    fn shapes_paths_and_transforms_become_polylines() {
        let text = r##"<?xml version="1.0"?>
<!-- a test drawing -->
<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100" viewBox="0 0 100 50">
  <g stroke="#ff0000" stroke-width="2" fill="none" transform="translate(10,0)">
    <path d="M0 0 L10 0 l0 10 H0 v-10 Z"/>
    <line x1="0" y1="20" x2="20" y2="20" style="stroke: rgb(0, 0, 255)"/>
  </g>
  <circle cx="50" cy="25" r="5" fill="black"/>
  <path d="M0,40 C10,30 20,30 30,40 S50,50 60,40 Q70,30 80,40 T90,40 A5 5 0 0 1 95 45" stroke="green" fill="none"/>
</svg>"##;
        let drawing = parse_svg(text).unwrap();
        assert_eq!((drawing.width, drawing.height), (200.0, 100.0));
        assert_eq!(drawing.shapes.len(), 4);
        // The square: moved 10 right, doubled by the view box, closed.
        let square = &drawing.shapes[0];
        assert!(square.closed);
        assert!(close(square.points[0], [20.0, 0.0]));
        assert!(close(square.points[2], [40.0, 20.0]));
        assert_eq!(square.stroke, Some([255, 0, 0]));
        assert!((square.width - 4.0).abs() < 0.01, "the width scales too");
        // The line takes its color from its style, its width from the group.
        let line = &drawing.shapes[1];
        assert_eq!(line.stroke, Some([0, 0, 255]));
        assert!(!line.closed);
        // The circle is filled, not stroked.
        assert_eq!(
            (drawing.shapes[2].stroke, drawing.shapes[2].fill),
            (None, Some([0, 0, 0]))
        );
        // The curves end where they say.
        let curve = &drawing.shapes[3];
        assert!(close(*curve.points.last().unwrap(), [190.0, 90.0]));
        assert!(curve.points.len() > 40, "curves are flattened");
    }

    #[test]
    fn numbers_colors_and_transforms_read_as_svg_means() {
        assert_eq!(numbers("1,2 3-4.5.5e1"), vec![1.0, 2.0, 3.0, -4.5, 5.0]);
        assert_eq!(color("#abc"), Some([0xaa, 0xbb, 0xcc]));
        assert_eq!(color("none"), None);
        let rotated = map(parse_transform("rotate(90)"), [1.0, 0.0]);
        assert!(close(rotated, [0.0, 1.0]));
        let both = map(parse_transform("translate(5 5) scale(2)"), [1.0, 1.0]);
        assert!(close(both, [7.0, 7.0]));
        assert!(parse_svg("<html></html>").is_err());
    }
}
