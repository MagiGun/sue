//! Reading one SVG file of an icon pack into a [`Drawing`]: the subset of
//! SVG that Sue's icon engine draws.
//!
//! What is read: `path` (its `d` with `M L H V C S Q T A Z`, absolute
//! and relative), `line`, `polyline`, `polygon`, `rect` (with round
//! corners), `circle` and `ellipse`; groups (`g`, `a`, `switch`, a nested
//! `svg`); `fill`, `stroke`, `stroke-width`, `fill-rule`, `display` and
//! `visibility`, as attributes or in a `style` attribute; `transform`;
//! the `viewBox` (or `width` and `height`), scaled into the 24 unit
//! canvas and centered; and masks in black and white (`mask`), which is
//! how Sue's own icons write a gap.
//!
//! What is not: colors (an icon is one ink, tinted by the theme as Sue's
//! own are), opacity, caps and joins (always round), dashes, text,
//! pictures, `use`, filters, clip paths and style sheets. Each element
//! or attribute that changes the picture and is not read is named in the
//! file's [`Problem`]s; nothing in a file stops the rest of it from being
//! read, and nothing panics.

use std::collections::HashMap;

use sue_io::publish::xml::{XmlEvent, parse as parse_xml};

use super::super::fit::CANVAS;
use super::super::path::{PathFault, Point, Segment, Subpath};
use super::data;

/// How a shape is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Paint {
    /// Along its outline with a round pen this many units wide (units of
    /// the 24 unit canvas).
    Stroke(f32),
    /// Filled.
    Fill {
        /// `fill-rule="evenodd"`: every other nested outline is a hole,
        /// whichever way round it runs.
        even_odd: bool,
    },
}

/// Outlines in units of the 24 unit canvas, and how they are drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    /// How.
    pub paint: Paint,
    /// What.
    pub paths: Vec<Subpath>,
}

/// One shape of a mask: light keeps what is under it, dark cuts it away.
#[derive(Clone, Debug, PartialEq)]
pub struct Cut {
    /// The shape.
    pub shape: Shape,
    /// Light (keeps) or dark (cuts).
    pub keep: bool,
}

/// One thing a drawing draws, over what is before it.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A shape.
    Shape(Shape),
    /// Items seen through a mask.
    Masked {
        /// What is drawn.
        items: Vec<Item>,
        /// The mask, shape over shape; where nothing light lies it hides.
        mask: Vec<Cut>,
    },
}

/// An icon read from an SVG file, on the 24 unit canvas.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawing {
    /// What it draws, item over item.
    pub items: Vec<Item>,
}

impl Drawing {
    /// Whether it draws nothing at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        fn empty(items: &[Item]) -> bool {
            items.iter().all(|item| match item {
                Item::Shape(shape) => shape.paths.is_empty(),
                Item::Masked { items, .. } => empty(items),
            })
        }
        empty(&self.items)
    }
}

/// Something in a file that Sue could not read, or read only in part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The file is not text (UTF-8).
    NotText,
    /// The file is larger than an icon can be.
    TooLarge {
        /// Its size in bytes.
        bytes: u64,
    },
    /// The file has no `svg` element.
    NoSvg,
    /// The file draws nothing Sue can read.
    Empty,
    /// The path data of an element stops being readable at a byte: what
    /// is before it is drawn.
    PathData {
        /// The element.
        element: String,
        /// What is wrong.
        fault: PathFault,
        /// The byte of the data where it is wrong.
        at: usize,
    },
    /// An attribute whose value cannot be read; the element is drawn as
    /// if it were not there.
    BadValue {
        /// The element.
        element: String,
        /// The attribute.
        attribute: String,
    },
    /// An element Sue does not draw (`text`, `image`, `use` and the
    /// like). It is left out.
    NotDrawn {
        /// The element.
        element: String,
    },
    /// An attribute that changes the picture and that Sue does not
    /// read (`clip-path`, `filter`, `stroke-dasharray`): the element is
    /// drawn without it.
    Ignored {
        /// The attribute.
        attribute: String,
    },
    /// A style sheet (`style` element or `class`): its rules are not
    /// read. Write the same as attributes.
    StyleSheet,
    /// A gradient or a pattern: drawn as plain ink.
    PlainInk,
    /// A mask that is named and not found, or a mask inside a mask: what
    /// it would hide is drawn.
    NoMask {
        /// Its name.
        id: String,
    },
}

/// One element of the document.
#[derive(Debug, Default)]
struct Node {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
}

impl Node {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }
}

/// The tag name without an `svg:` prefix.
fn local(name: &str) -> &str {
    name.strip_prefix("svg:").unwrap_or(name)
}

/// The document as a tree under a root that has no name.
fn tree(text: &str) -> Node {
    let mut stack: Vec<Node> = vec![Node::default()];
    for event in parse_xml(text) {
        match event {
            XmlEvent::Open { name, attrs, empty } => {
                let node = Node {
                    name: local(&name).to_owned(),
                    attrs,
                    children: Vec::new(),
                };
                if empty {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    }
                } else {
                    stack.push(node);
                }
            }
            XmlEvent::Close(name) => {
                let name = local(&name);
                // Close up to the element that is named, if it is open.
                if let Some(depth) = stack.iter().rposition(|node| node.name == name)
                    && depth > 0
                {
                    while stack.len() > depth {
                        let node = stack.pop().unwrap_or_default();
                        if let Some(parent) = stack.last_mut() {
                            parent.children.push(node);
                        }
                    }
                }
            }
            XmlEvent::Text(_) => {}
        }
    }
    while stack.len() > 1 {
        let node = stack.pop().unwrap_or_default();
        if let Some(parent) = stack.last_mut() {
            parent.children.push(node);
        }
    }
    stack.pop().unwrap_or_default()
}

/// An affine map: `x' = a x + c y + e`, `y' = b x + d y + f`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Matrix([f32; 6]);

impl Matrix {
    const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// `self` after `inner`: a point is mapped by `inner` first.
    fn then(self, inner: Self) -> Self {
        let [a, b, c, d, e, f] = self.0;
        let [p, q, r, s, t, u] = inner.0;
        Self([
            a * p + c * q,
            b * p + d * q,
            a * r + c * s,
            b * r + d * s,
            a * t + c * u + e,
            b * t + d * u + f,
        ])
    }

    fn map(self, [x, y]: Point) -> Point {
        let [a, b, c, d, e, f] = self.0;
        [a * x + c * y + e, b * x + d * y + f]
    }

    /// How much it scales a length, on average.
    fn scale(self) -> f32 {
        let [a, b, c, d, ..] = self.0;
        (a * d - b * c).abs().sqrt()
    }

    /// Its scale and turn in degrees when it only turns, scales evenly
    /// and moves.
    fn similarity(self) -> Option<(f32, f32)> {
        let [a, b, c, d, ..] = self.0;
        let near = 1e-5 * (a.abs() + b.abs()).max(1e-6);
        ((a - d).abs() <= near && (b + c).abs() <= near && a * d - b * c > 0.0)
            .then(|| (a.hypot(b), b.atan2(a).to_degrees()))
    }

    fn is_finite(self) -> bool {
        self.0.iter().all(|number| number.is_finite())
    }
}

/// Reads a `transform` attribute.
fn transform(text: &str) -> Option<Matrix> {
    let mut matrix = Matrix::IDENTITY;
    let mut rest = text.trim();
    while !rest.is_empty() {
        let open = rest.find('(')?;
        let close = rest.find(')')?;
        if close < open {
            return None;
        }
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let numbers = numbers(&rest[open + 1..close])?;
        let step = match (name, numbers.as_slice()) {
            ("matrix", [a, b, c, d, e, f]) => Matrix([*a, *b, *c, *d, *e, *f]),
            ("translate", [x]) => Matrix([1.0, 0.0, 0.0, 1.0, *x, 0.0]),
            ("translate", [x, y]) => Matrix([1.0, 0.0, 0.0, 1.0, *x, *y]),
            ("scale", [s]) => Matrix([*s, 0.0, 0.0, *s, 0.0, 0.0]),
            ("scale", [x, y]) => Matrix([*x, 0.0, 0.0, *y, 0.0, 0.0]),
            ("rotate", [angle, around @ ..]) if around.is_empty() || around.len() == 2 => {
                let (sin, cos) = angle.to_radians().sin_cos();
                let turn = Matrix([cos, sin, -sin, cos, 0.0, 0.0]);
                if let [x, y] = around {
                    Matrix([1.0, 0.0, 0.0, 1.0, *x, *y])
                        .then(turn)
                        .then(Matrix([1.0, 0.0, 0.0, 1.0, -x, -y]))
                } else {
                    turn
                }
            }
            ("skewX", [angle]) => Matrix([1.0, 0.0, angle.to_radians().tan(), 1.0, 0.0, 0.0]),
            ("skewY", [angle]) => Matrix([1.0, angle.to_radians().tan(), 0.0, 1.0, 0.0, 0.0]),
            _ => return None,
        };
        matrix = matrix.then(step);
        rest = rest[close + 1..].trim_start();
    }
    matrix.is_finite().then_some(matrix)
}

/// Numbers apart by spaces or commas, as attributes write them.
fn numbers(text: &str) -> Option<Vec<f32>> {
    // Path data reads numbers the way SVG writes them (`1-2` is two).
    let (paths, fault) = data::parse(&format!("M0 0 L{text}"));
    let list: Vec<f32> = text
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter(|piece| !piece.is_empty())
        .map(str::parse::<f32>)
        .collect::<Result<_, _>>()
        .ok()
        .or_else(|| {
            // Written close together: read pairs from the path reader.
            if fault.is_some() {
                return None;
            }
            Some(
                paths
                    .first()?
                    .segments
                    .iter()
                    .flat_map(|segment| segment.end())
                    .collect(),
            )
        })?;
    list.iter().all(|number| number.is_finite()).then_some(list)
}

/// A length: a number, or a number of `px`.
fn length(text: &str) -> Option<f32> {
    let text = text.trim();
    let text = text.strip_suffix("px").unwrap_or(text).trim_end();
    text.parse::<f32>().ok().filter(|number| number.is_finite())
}

/// What a fill or a stroke is painted with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ink {
    None,
    /// Ink of a color: light or dark, which only a mask asks.
    Color {
        light: bool,
    },
}

/// Reads a paint. `None` when it cannot be read.
fn ink(text: &str, problems: &mut Vec<Problem>) -> Option<Ink> {
    let text = text.trim();
    let lower = text.to_ascii_lowercase();
    let light = |r: f32, g: f32, b: f32| Ink::Color {
        light: 0.2126 * r + 0.7152 * g + 0.0722 * b >= 0.5,
    };
    Some(match lower.as_str() {
        "none" | "transparent" => Ink::None,
        "currentcolor" | "inherit" | "context-fill" | "context-stroke" => {
            Ink::Color { light: true }
        }
        "black" => Ink::Color { light: false },
        "white" => Ink::Color { light: true },
        _ if lower.starts_with("url(") => {
            if !problems.contains(&Problem::PlainInk) {
                problems.push(Problem::PlainInk);
            }
            Ink::Color { light: true }
        }
        _ if lower.starts_with('#') => {
            let hex = &lower[1..];
            let digit = |at: usize, wide: usize| {
                u8::from_str_radix(hex.get(at..at + wide)?, 16)
                    .ok()
                    .map(|value| {
                        if wide == 1 {
                            f32::from(value) / 15.0
                        } else {
                            f32::from(value) / 255.0
                        }
                    })
            };
            let (r, g, b) = match hex.len() {
                3 | 4 => (digit(0, 1)?, digit(1, 1)?, digit(2, 1)?),
                6 | 8 => (digit(0, 2)?, digit(2, 2)?, digit(4, 2)?),
                _ => return None,
            };
            light(r, g, b)
        }
        _ if lower.starts_with("rgb") => {
            let inner = lower.split_once('(')?.1.trim_end_matches(')');
            let mut channels = inner
                .split(|ch: char| ch == ',' || ch == '/' || ch.is_whitespace())
                .filter(|piece| !piece.is_empty())
                .map(|piece| {
                    piece.strip_suffix('%').map_or_else(
                        || piece.parse::<f32>().ok().map(|value| value / 255.0),
                        |percent| percent.parse::<f32>().ok().map(|value| value / 100.0),
                    )
                });
            let (r, g, b) = (channels.next()??, channels.next()??, channels.next()??);
            light(r, g, b)
        }
        // Any other color name: ink, light or dark by a guess that only a
        // mask would ask for.
        _ if lower.chars().all(|ch| ch.is_ascii_alphabetic()) && !lower.is_empty() => Ink::Color {
            light: !matches!(
                lower.as_str(),
                "navy" | "darkblue" | "maroon" | "darkgreen" | "indigo" | "purple"
            ),
        },
        _ => return None,
    })
}

/// What an element inherits and what it says itself.
#[derive(Clone, Copy, Debug)]
struct Style {
    fill: Ink,
    stroke: Ink,
    /// In the element's own units.
    width: f32,
    even_odd: bool,
    hidden: bool,
    matrix: Matrix,
}

impl Style {
    /// SVG's starting point: filled in black, no stroke, a pen of 1.
    const ROOT: Self = Self {
        fill: Ink::Color { light: false },
        stroke: Ink::None,
        width: 1.0,
        even_odd: false,
        hidden: false,
        matrix: Matrix::IDENTITY,
    };
}

/// The attributes that change the picture and are not read.
const IGNORED: [&str; 6] = [
    "clip-path",
    "filter",
    "stroke-dasharray",
    "marker-start",
    "marker-mid",
    "marker-end",
];

/// Elements that hold no drawing of their own, or only one used by name.
const SILENT: [&str; 12] = [
    "title",
    "desc",
    "metadata",
    "defs",
    "mask",
    "clipPath",
    "symbol",
    "linearGradient",
    "radialGradient",
    "pattern",
    "marker",
    "filter",
];

/// Everything that reads one file.
struct Reader<'a> {
    masks: HashMap<String, &'a Node>,
    problems: Vec<Problem>,
    /// The style of the root element, which a mask's shapes inherit.
    root: Style,
}

impl<'a> Reader<'a> {
    fn note(&mut self, problem: Problem) {
        if !self.problems.contains(&problem) {
            self.problems.push(problem);
        }
    }

    /// Finds every mask by its id.
    fn find_masks(&mut self, node: &'a Node) {
        for child in &node.children {
            if child.name == "mask"
                && let Some(id) = child.attr("id")
            {
                self.masks.insert(id.to_owned(), child);
            }
            self.find_masks(child);
        }
    }

    /// `node`'s style, from `outer`'s.
    fn style(&mut self, node: &Node, outer: Style) -> Style {
        let mut style = outer;
        let mut properties: Vec<(String, String)> = node.attrs.clone();
        // A `style` attribute says the same as attributes, and wins.
        if let Some(declarations) = node.attr("style") {
            for declaration in declarations.split(';') {
                if let Some((key, value)) = declaration.split_once(':') {
                    properties.push((key.trim().to_owned(), value.trim().to_owned()));
                }
            }
        }
        for (key, value) in &properties {
            let bad = |reader: &mut Self| {
                reader.note(Problem::BadValue {
                    element: node.name.clone(),
                    attribute: key.clone(),
                });
            };
            match key.as_str() {
                "fill" => match ink(value, &mut self.problems) {
                    Some(found) => style.fill = found,
                    None => bad(self),
                },
                "stroke" => match ink(value, &mut self.problems) {
                    Some(found) => style.stroke = found,
                    None => bad(self),
                },
                "stroke-width" => match length(value).filter(|width| *width >= 0.0) {
                    Some(width) => style.width = width,
                    None => bad(self),
                },
                "fill-rule" => style.even_odd = value.trim() == "evenodd",
                "display" if value.trim() == "none" => style.hidden = true,
                "visibility" => style.hidden = matches!(value.trim(), "hidden" | "collapse"),
                "class" => self.note(Problem::StyleSheet),
                "stroke-dasharray" if value.trim() == "none" => {}
                "filter" | "clip-path" if value.trim() == "none" => {}
                _ if IGNORED.contains(&key.as_str()) => self.note(Problem::Ignored {
                    attribute: key.clone(),
                }),
                _ => {}
            }
        }
        if let Some(text) = node.attr("transform") {
            match transform(text) {
                Some(matrix) => style.matrix = outer.matrix.then(matrix),
                None => {
                    self.note(Problem::BadValue {
                        element: node.name.clone(),
                        attribute: "transform".to_owned(),
                    });
                }
            }
        }
        style
    }

    /// The outlines of a shape element, in its own units.
    fn outlines(&mut self, node: &Node) -> Option<Vec<Subpath>> {
        let number = |reader: &mut Self, key: &str, default: f32| -> Option<f32> {
            match node.attr(key) {
                None => Some(default),
                Some(text) => {
                    let found = length(text);
                    if found.is_none() {
                        reader.note(Problem::BadValue {
                            element: node.name.clone(),
                            attribute: key.to_owned(),
                        });
                    }
                    found
                }
            }
        };
        let read = |reader: &mut Self, data: &str| {
            let (paths, fault) = data::parse(data);
            if let Some(fault) = fault {
                reader.note(Problem::PathData {
                    element: node.name.clone(),
                    fault: fault.fault,
                    at: fault.at,
                });
            }
            paths
        };
        Some(match node.name.as_str() {
            "path" => read(self, node.attr("d").unwrap_or_default()),
            "line" => {
                let [x1, y1, x2, y2] = [
                    number(self, "x1", 0.0)?,
                    number(self, "y1", 0.0)?,
                    number(self, "x2", 0.0)?,
                    number(self, "y2", 0.0)?,
                ];
                vec![Subpath {
                    start: [x1, y1],
                    segments: vec![Segment::Line([x2, y2])],
                    closed: false,
                }]
            }
            "polyline" | "polygon" => {
                let points = node.attr("points").unwrap_or_default();
                if points.trim().is_empty() {
                    return Some(Vec::new());
                }
                let close = if node.name == "polygon" { " Z" } else { "" };
                read(self, &format!("M{points}{close}"))
            }
            "rect" => {
                let [x, y, width, height] = [
                    number(self, "x", 0.0)?,
                    number(self, "y", 0.0)?,
                    number(self, "width", 0.0)?,
                    number(self, "height", 0.0)?,
                ];
                if width <= 0.0 || height <= 0.0 {
                    return Some(Vec::new());
                }
                let rx = node.attr("rx").map(|_| number(self, "rx", 0.0));
                let ry = node.attr("ry").map(|_| number(self, "ry", 0.0));
                let (rx, ry) = match (rx.flatten(), ry.flatten()) {
                    (Some(rx), Some(ry)) => (rx, ry),
                    (Some(r), None) | (None, Some(r)) => (r, r),
                    (None, None) => (0.0, 0.0),
                };
                let (rx, ry) = (rx.clamp(0.0, width / 2.0), ry.clamp(0.0, height / 2.0));
                vec![rect([x, y, width, height], [rx, ry])]
            }
            "circle" => {
                let [cx, cy, r] = [
                    number(self, "cx", 0.0)?,
                    number(self, "cy", 0.0)?,
                    number(self, "r", 0.0)?,
                ];
                if r <= 0.0 {
                    return Some(Vec::new());
                }
                vec![ellipse([cx, cy], [r, r])]
            }
            "ellipse" => {
                let [cx, cy, rx, ry] = [
                    number(self, "cx", 0.0)?,
                    number(self, "cy", 0.0)?,
                    number(self, "rx", 0.0)?,
                    number(self, "ry", 0.0)?,
                ];
                if rx <= 0.0 || ry <= 0.0 {
                    return Some(Vec::new());
                }
                vec![ellipse([cx, cy], [rx, ry])]
            }
            _ => return None,
        })
    }

    /// The shapes `node` draws with `style`: a fill, then a stroke.
    fn shapes(&mut self, node: &Node, style: Style) -> Vec<(Shape, bool)> {
        let Some(paths) = self.outlines(node) else {
            return Vec::new();
        };
        let paths = place(&paths, style.matrix);
        if paths.is_empty() {
            return Vec::new();
        }
        let mut found = Vec::new();
        // A line has no inside to fill.
        if let Ink::Color { light } = style.fill
            && node.name != "line"
        {
            found.push((
                Shape {
                    paint: Paint::Fill {
                        even_odd: style.even_odd,
                    },
                    paths: paths.clone(),
                },
                light,
            ));
        }
        if let Ink::Color { light } = style.stroke {
            let width = style.width * style.matrix.scale();
            if width > 0.0 && width.is_finite() {
                found.push((
                    Shape {
                        paint: Paint::Stroke(width),
                        paths,
                    },
                    light,
                ));
            }
        }
        found
    }

    /// The items under `node`, drawn with `outer` style.
    fn items(&mut self, node: &'a Node, outer: Style, depth: usize) -> Vec<Item> {
        let mut items = Vec::new();
        for child in &node.children {
            items.extend(self.item(child, outer, depth));
        }
        items
    }

    /// What one element draws.
    fn item(&mut self, node: &'a Node, outer: Style, depth: usize) -> Vec<Item> {
        let name = node.name.as_str();
        if SILENT.contains(&name) || name.contains(':') || depth > 64 {
            return Vec::new();
        }
        let style = self.style(node, outer);
        if style.hidden {
            return Vec::new();
        }
        let drawn = match name {
            "g" | "a" | "switch" | "svg" => self.items(node, style, depth + 1),
            "path" | "line" | "polyline" | "polygon" | "rect" | "circle" | "ellipse" => self
                .shapes(node, style)
                .into_iter()
                .map(|(shape, _)| Item::Shape(shape))
                .collect(),
            "style" => {
                self.note(Problem::StyleSheet);
                Vec::new()
            }
            _ => {
                self.note(Problem::NotDrawn {
                    element: name.to_owned(),
                });
                Vec::new()
            }
        };
        let Some(reference) = node.attr("mask") else {
            return drawn;
        };
        let id = reference
            .trim()
            .strip_prefix("url(")
            .and_then(|rest| rest.strip_suffix(')'))
            .map(|inner| inner.trim().trim_matches(['"', '\'']))
            .and_then(|inner| inner.strip_prefix('#'))
            .unwrap_or_default();
        match self.masks.get(id).copied() {
            Some(mask) => {
                // A mask's shapes lie in the units of the element that
                // names it and inherit their paint from the root.
                let base = self.style(
                    mask,
                    Style {
                        matrix: style.matrix,
                        ..self.root
                    },
                );
                let mut cuts = Vec::new();
                self.cuts(mask, base, &mut cuts, depth);
                vec![Item::Masked {
                    items: drawn,
                    mask: cuts,
                }]
            }
            None if reference.trim() == "none" => drawn,
            None => {
                self.note(Problem::NoMask { id: id.to_owned() });
                drawn
            }
        }
    }

    /// The shapes of a mask, light and dark.
    fn cuts(&mut self, node: &'a Node, outer: Style, cuts: &mut Vec<Cut>, depth: usize) {
        if depth > 64 {
            return;
        }
        for child in &node.children {
            let name = child.name.as_str();
            if SILENT.contains(&name) || name.contains(':') {
                continue;
            }
            let style = self.style(child, outer);
            if style.hidden {
                continue;
            }
            if child.attr("mask").is_some() {
                self.note(Problem::NoMask {
                    id: child.attr("mask").unwrap_or_default().to_owned(),
                });
            }
            match name {
                "g" | "a" | "switch" => self.cuts(child, style, cuts, depth + 1),
                "path" | "line" | "polyline" | "polygon" | "rect" | "circle" | "ellipse" => {
                    for (shape, keep) in self.shapes(child, style) {
                        cuts.push(Cut { shape, keep });
                    }
                }
                _ => self.note(Problem::NotDrawn {
                    element: name.to_owned(),
                }),
            }
        }
    }
}

/// A box with corners rounded by `radii`, clockwise from the top left.
fn rect([x, y, width, height]: [f32; 4], [rx, ry]: [f32; 2]) -> Subpath {
    let (right, bottom) = (x + width, y + height);
    if rx <= 0.0 || ry <= 0.0 {
        return Subpath {
            start: [x, y],
            segments: vec![
                Segment::Line([right, y]),
                Segment::Line([right, bottom]),
                Segment::Line([x, bottom]),
            ],
            closed: true,
        };
    }
    let arc = |to: Point| Segment::Arc {
        radii: [rx, ry],
        rotation: 0.0,
        large: false,
        sweep: true,
        to,
    };
    Subpath {
        start: [x + rx, y],
        segments: vec![
            Segment::Line([right - rx, y]),
            arc([right, y + ry]),
            Segment::Line([right, bottom - ry]),
            arc([right - rx, bottom]),
            Segment::Line([x + rx, bottom]),
            arc([x, bottom - ry]),
            Segment::Line([x, y + ry]),
            arc([x + rx, y]),
        ],
        closed: true,
    }
}

/// An ellipse around `middle`, clockwise from its right.
fn ellipse(middle: Point, radii: Point) -> Subpath {
    let arc = |to: Point| Segment::Arc {
        radii,
        rotation: 0.0,
        large: false,
        sweep: true,
        to,
    };
    Subpath {
        start: [middle[0] + radii[0], middle[1]],
        segments: vec![
            arc([middle[0] - radii[0], middle[1]]),
            arc([middle[0] + radii[0], middle[1]]),
        ],
        closed: true,
    }
}

/// The arc from `from` as cubic curves, each a quarter turn or less: how
/// an arc is mapped by a transform that does not keep its shape.
fn arc_cubics(from: Point, segment: Segment) -> Vec<Segment> {
    let Segment::Arc {
        radii,
        rotation,
        large,
        sweep,
        to,
    } = segment
    else {
        return vec![segment];
    };
    let (mut rx, mut ry) = (radii[0].abs(), radii[1].abs());
    if (from[0] - to[0]).abs() <= 1e-6 && (from[1] - to[1]).abs() <= 1e-6 {
        return Vec::new();
    }
    if rx <= 1e-6 || ry <= 1e-6 {
        return vec![Segment::Line(to)];
    }
    let (sin, cos) = rotation.to_radians().sin_cos();
    let half = [(from[0] - to[0]) / 2.0, (from[1] - to[1]) / 2.0];
    let x1 = cos * half[0] + sin * half[1];
    let y1 = -sin * half[0] + cos * half[1];
    let lambda = (x1 / rx).powi(2) + (y1 / ry).powi(2);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let above = (rx * ry).powi(2) - (rx * y1).powi(2) - (ry * x1).powi(2);
    let below = (rx * y1).powi(2) + (ry * x1).powi(2);
    let mut factor = (above / below).max(0.0).sqrt();
    if large == sweep {
        factor = -factor;
    }
    let (cxp, cyp) = (factor * rx * y1 / ry, -factor * ry * x1 / rx);
    let middle = [
        cos * cxp - sin * cyp + (from[0] + to[0]) / 2.0,
        sin * cxp + cos * cyp + (from[1] + to[1]) / 2.0,
    ];
    let first = ((y1 - cyp) / ry).atan2((x1 - cxp) / rx);
    let end = ((-y1 - cyp) / ry).atan2((-x1 - cxp) / rx);
    let mut turn = end - first;
    if sweep && turn < 0.0 {
        turn += std::f32::consts::TAU;
    } else if !sweep && turn > 0.0 {
        turn -= std::f32::consts::TAU;
    }
    let pieces = (turn.abs() / std::f32::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = turn / pieces as f32;
    let handle = 4.0 / 3.0 * (step / 4.0).tan();
    let at = |angle: f32| {
        let (s, c) = angle.sin_cos();
        (
            [
                middle[0] + rx * c * cos - ry * s * sin,
                middle[1] + rx * c * sin + ry * s * cos,
            ],
            [-rx * s * cos - ry * c * sin, -rx * s * sin + ry * c * cos],
        )
    };
    let mut out = Vec::with_capacity(pieces);
    for piece in 0..pieces {
        let (a, b) = (
            first + step * piece as f32,
            first + step * (piece + 1) as f32,
        );
        let ((p0, d0), (p3, d3)) = (at(a), at(b));
        let p3 = if piece + 1 == pieces { to } else { p3 };
        out.push(Segment::Cubic(
            [p0[0] + handle * d0[0], p0[1] + handle * d0[1]],
            [p3[0] - handle * d3[0], p3[1] - handle * d3[1]],
            p3,
        ));
    }
    out
}

/// `paths` mapped by `matrix`. Arcs stay arcs where the map keeps their
/// shape and become cubic curves where it does not. Subpaths with a
/// number that is not finite after the map are dropped.
fn place(paths: &[Subpath], matrix: Matrix) -> Vec<Subpath> {
    if matrix == Matrix::IDENTITY {
        return paths.to_vec();
    }
    let similar = matrix.similarity();
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        let mut segments = Vec::with_capacity(path.segments.len());
        let mut pen = path.start;
        for segment in &path.segments {
            let pieces = match (*segment, similar) {
                (Segment::Arc { .. }, Some(_)) => vec![*segment],
                (Segment::Arc { .. }, None) => arc_cubics(pen, *segment),
                _ => vec![*segment],
            };
            for piece in pieces {
                segments.push(match piece {
                    Segment::Line(to) => Segment::Line(matrix.map(to)),
                    Segment::Quad(control, to) => {
                        Segment::Quad(matrix.map(control), matrix.map(to))
                    }
                    Segment::Cubic(a, b, to) => {
                        Segment::Cubic(matrix.map(a), matrix.map(b), matrix.map(to))
                    }
                    Segment::Arc {
                        radii,
                        rotation,
                        large,
                        sweep,
                        to,
                    } => {
                        let (scale, turn) = similar.unwrap_or((1.0, 0.0));
                        Segment::Arc {
                            radii: [radii[0] * scale, radii[1] * scale],
                            rotation: rotation + turn,
                            large,
                            sweep,
                            to: matrix.map(to),
                        }
                    }
                });
            }
            pen = segment.end();
        }
        let placed = Subpath {
            start: matrix.map(path.start),
            segments,
            closed: path.closed,
        };
        let finite = placed.start.iter().all(|n| n.is_finite())
            && placed.segments.iter().all(|segment| match segment {
                Segment::Line(a) => a.iter().all(|n| n.is_finite()),
                Segment::Quad(a, b) => a.iter().chain(b).all(|n| n.is_finite()),
                Segment::Cubic(a, b, c) => a.iter().chain(b).chain(c).all(|n| n.is_finite()),
                Segment::Arc {
                    radii,
                    rotation,
                    to,
                    ..
                } => radii.iter().chain(to).all(|n| n.is_finite()) && rotation.is_finite(),
            });
        if finite {
            out.push(placed);
        }
    }
    out
}

/// The map from the root's `viewBox` (or its `width` and `height`) to
/// the 24 unit canvas: scaled evenly and centered.
fn viewport(root: &Node, problems: &mut Vec<Problem>) -> Matrix {
    let mut bad = |attribute: &str| {
        problems.push(Problem::BadValue {
            element: "svg".to_owned(),
            attribute: attribute.to_owned(),
        });
    };
    let boxed = match root.attr("viewBox") {
        Some(text) => match numbers(text).as_deref() {
            Some(&[x, y, width, height]) if width > 0.0 && height > 0.0 => {
                Some([x, y, width, height])
            }
            _ => {
                bad("viewBox");
                None
            }
        },
        None => None,
    };
    let [x, y, width, height] = boxed.unwrap_or_else(|| {
        let side = |key: &str| root.attr(key).and_then(length).filter(|side| *side > 0.0);
        match (side("width"), side("height")) {
            (Some(width), Some(height)) => [0.0, 0.0, width, height],
            (Some(side), None) | (None, Some(side)) => [0.0, 0.0, side, side],
            (None, None) => [0.0, 0.0, CANVAS, CANVAS],
        }
    });
    let scale = (CANVAS / width).min(CANVAS / height);
    let left = (CANVAS - width * scale) / 2.0 - x * scale;
    let top = (CANVAS - height * scale) / 2.0 - y * scale;
    let matrix = Matrix([scale, 0.0, 0.0, scale, left, top]);
    if matrix.is_finite() {
        matrix
    } else {
        Matrix::IDENTITY
    }
}

/// Reads an SVG document into a drawing on the 24 unit canvas, with what
/// could not be read in it. Never fails: a file that cannot be read is
/// an empty drawing and says why.
#[must_use]
pub fn read(text: &str) -> (Drawing, Vec<Problem>) {
    let document = tree(text);
    fn find<'n>(node: &'n Node) -> Option<&'n Node> {
        if node.name == "svg" {
            return Some(node);
        }
        node.children.iter().find_map(find)
    }
    let Some(root) = find(&document) else {
        return (Drawing::default(), vec![Problem::NoSvg]);
    };
    let mut reader = Reader {
        masks: HashMap::new(),
        problems: Vec::new(),
        root: Style::ROOT,
    };
    let matrix = viewport(root, &mut reader.problems);
    reader.find_masks(root);
    let style = reader.style(
        root,
        Style {
            matrix,
            ..Style::ROOT
        },
    );
    reader.root = style;
    let items = if style.hidden {
        Vec::new()
    } else {
        reader.items(root, style, 0)
    };
    let mut problems = reader.problems;
    let drawing = Drawing { items };
    if drawing.is_empty() {
        problems.push(Problem::Empty);
    }
    (drawing, problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shapes(drawing: &Drawing) -> Vec<&Shape> {
        drawing
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Shape(shape) => Some(shape),
                Item::Masked { .. } => None,
            })
            .collect()
    }

    #[test]
    fn the_view_box_is_scaled_into_the_canvas_and_centered() {
        let (drawing, problems) = read(
            r##"<svg viewBox="0 0 48 24" xmlns="http://www.w3.org/2000/svg"><path d="M0 0 L48 24" stroke="#000" fill="none" stroke-width="4"/></svg>"##,
        );
        assert!(problems.is_empty(), "{problems:?}");
        let found = shapes(&drawing);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].paint, Paint::Stroke(2.0));
        assert_eq!(found[0].paths[0].start, [0.0, 6.0]);
        assert_eq!(
            found[0].paths[0].segments,
            vec![Segment::Line([24.0, 18.0])]
        );
    }

    #[test]
    fn shapes_become_outlines() {
        let (drawing, problems) = read(
            r##"<svg viewBox="0 0 24 24">
                <line x1="2" y1="2" x2="22" y2="2" stroke="black"/>
                <polyline points="2,4 6,8 10,4" fill="none" stroke="black"/>
                <polygon points="2 10 6 14 10 10"/>
                <rect x="2" y="16" width="6" height="4"/>
                <rect x="12" y="16" width="8" height="6" rx="2"/>
                <circle cx="17" cy="7" r="3"/>
                <ellipse cx="17" cy="12" rx="3" ry="1"/>
            </svg>"##,
        );
        assert!(problems.is_empty(), "{problems:?}");
        let found = shapes(&drawing);
        assert_eq!(found.len(), 7);
        assert_eq!(found[1].paths[0].segments.len(), 2);
        assert!(found[2].paths[0].closed);
        assert_eq!(found[3].paths[0].segments.len(), 3);
        assert_eq!(found[4].paths[0].segments.len(), 8);
        assert!(matches!(found[5].paths[0].segments[0], Segment::Arc { .. }));
    }

    #[test]
    fn styles_inherit_and_the_style_attribute_wins() {
        let (drawing, problems) = read(
            r##"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <g stroke-width="3"><path d="M2 2 H20" style="stroke-width: 1px; fill: #fff"/></g>
                <path d="M2 6 H20" stroke="none"/>
                <path d="M2 8 H20" display="none"/>
            </svg>"##,
        );
        assert!(!problems.contains(&Problem::Empty), "{problems:?}");
        let found = shapes(&drawing);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].paint, Paint::Fill { even_odd: false });
        assert_eq!(found[1].paint, Paint::Stroke(1.0));
    }

    #[test]
    fn transforms_map_points_and_arcs() {
        let (drawing, _) = read(
            r##"<svg viewBox="0 0 24 24"><g transform="translate(2 3) scale(2)"><circle cx="2" cy="2" r="1"/></g>
               <ellipse transform="scale(2 1)" cx="4" cy="10" rx="1" ry="1"/>
               <path transform="rotate(90 12 12)" d="M12 2 V6" stroke="#000"/></svg>"##,
        );
        let found = shapes(&drawing);
        assert_eq!(found[0].paths[0].start, [8.0, 7.0]);
        match found[0].paths[0].segments[0] {
            Segment::Arc { radii, to, .. } => {
                assert_eq!(radii, [2.0, 2.0]);
                assert_eq!(to, [4.0, 7.0]);
            }
            other => panic!("{other:?}"),
        }
        // Scaled unevenly, the circle is an ellipse of cubic curves.
        assert!(
            found[1].paths[0]
                .segments
                .iter()
                .all(|segment| matches!(segment, Segment::Cubic(..)))
        );
        let end = found[1].paths[0].segments.last().unwrap().end();
        assert!((end[0] - 10.0).abs() < 1e-4 && (end[1] - 10.0).abs() < 1e-4);
        let line = &found[3].paths[0];
        assert!((line.start[0] - 22.0).abs() < 1e-4 && (line.start[1] - 12.0).abs() < 1e-4);
    }

    #[test]
    fn what_cannot_be_read_is_named_and_the_rest_is_drawn() {
        let (drawing, problems) = read(
            r##"<svg viewBox="0 0 24 24"><style>.a{fill:red}</style>
                <text x="2" y="2">A</text><image href="a.png"/>
                <path d="M2 2 L10 10 Q" stroke="#000" clip-path="url(#c)"/>
                <rect width="abc" height="4"/>
                <circle r="4" cx="12" cy="12" fill="url(#gradient)" mask="url(#missing)"/>
            </svg>"##,
        );
        assert!(problems.contains(&Problem::StyleSheet));
        assert!(problems.contains(&Problem::NotDrawn {
            element: "text".to_owned()
        }));
        assert!(problems.contains(&Problem::NotDrawn {
            element: "image".to_owned()
        }));
        assert!(problems.contains(&Problem::PathData {
            element: "path".to_owned(),
            fault: PathFault::MissingNumber,
            at: 13,
        }));
        assert!(problems.contains(&Problem::Ignored {
            attribute: "clip-path".to_owned()
        }));
        assert!(problems.contains(&Problem::BadValue {
            element: "rect".to_owned(),
            attribute: "width".to_owned()
        }));
        assert!(problems.contains(&Problem::PlainInk));
        assert!(problems.contains(&Problem::NoMask {
            id: "missing".to_owned()
        }));
        // The path up to its fault and the circle are drawn.
        assert_eq!(shapes(&drawing).len(), 3);
    }

    #[test]
    fn nothing_readable_is_said_plainly() {
        assert_eq!(read("not svg at all").1, vec![Problem::NoSvg]);
        assert_eq!(read("<svg/>").1, vec![Problem::Empty]);
        assert_eq!(read("").1, vec![Problem::NoSvg]);
        let (_, problems) =
            read(r##"<svg viewBox="0 0 0 5"><path d="M1 1 H4" stroke="#000"/></svg>"##);
        assert_eq!(
            problems,
            vec![Problem::BadValue {
                element: "svg".to_owned(),
                attribute: "viewBox".to_owned()
            }]
        );
    }

    #[test]
    fn masks_keep_light_and_cut_dark() {
        let (drawing, problems) = read(
            r##"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor">
                <mask id="m"><rect width="24" height="24" fill="#fff" stroke="none"/><path d="M12 2 V22" stroke="#000" stroke-width="4.5"/></mask>
                <g mask="url(#m)"><path d="M2 12 H22"/></g>
            </svg>"##,
        );
        assert!(problems.is_empty(), "{problems:?}");
        let [Item::Masked { items, mask }] = drawing.items.as_slice() else {
            panic!("{:?}", drawing.items);
        };
        assert_eq!(items.len(), 1);
        assert_eq!(mask.len(), 2);
        assert!(mask[0].keep && !mask[1].keep);
        assert_eq!(mask[1].shape.paint, Paint::Stroke(4.5));
    }

    #[test]
    fn transform_lists_read() {
        assert_eq!(
            transform("translate(1,2)"),
            Some(Matrix([1.0, 0.0, 0.0, 1.0, 1.0, 2.0]))
        );
        assert_eq!(
            transform("scale(2) translate(1 1)").map(|m| m.map([0.0, 0.0])),
            Some([2.0, 2.0])
        );
        assert!(transform("wobble(3)").is_none());
        assert!(transform("matrix(1 2 3)").is_none());
        assert_eq!(numbers("1-2 3e1,4"), Some(vec![1.0, -2.0, 30.0, 4.0]));
    }
}
