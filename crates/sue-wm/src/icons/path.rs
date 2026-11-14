//! Path data, the way SVG writes it: the subset icons are drawn in.
//!
//! The commands are `M L H V C Q A Z` and their relative forms in lower
//! case. [`parse`] reads a string into [`Subpath`]s and
//! [`Subpath::flatten`] turns one into the straight pieces the stroker and
//! the rasterizer work on.

/// A point: x to the right, y down.
pub type Point = [f32; 2];

/// What is wrong with a piece of path data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathFault {
    /// A letter that is not one of the commands.
    UnknownCommand,
    /// A drawing command before any `M`.
    NoMoveTo,
    /// A command ran out of numbers.
    MissingNumber,
    /// A number that cannot be read, or is not finite.
    BadNumber,
    /// An arc's flag that is neither `0` nor `1`.
    BadFlag,
}

/// Path data that cannot be read, and where.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathError {
    /// What is wrong.
    pub fault: PathFault,
    /// The byte of the string where it went wrong.
    pub at: usize,
}

/// One piece of a subpath, from where the pen is to its last point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    /// A straight line.
    Line(Point),
    /// A quadratic curve: its control point, then its end.
    Quad(Point, Point),
    /// A cubic curve: its two control points, then its end.
    Cubic(Point, Point, Point),
    /// Part of an ellipse, as SVG's `A` gives it.
    Arc {
        /// The two radii.
        radii: Point,
        /// The turn of the ellipse, in degrees.
        rotation: f32,
        /// The longer of the two ways round.
        large: bool,
        /// Clockwise on the screen.
        sweep: bool,
        /// Where it ends.
        to: Point,
    },
}

impl Segment {
    /// Where the piece ends.
    #[must_use]
    pub fn end(&self) -> Point {
        match *self {
            Self::Line(to) | Self::Quad(_, to) | Self::Cubic(_, _, to) | Self::Arc { to, .. } => to,
        }
    }
}

/// One stroke of the pen: from an `M` to the next, or to a `Z`.
#[derive(Clone, Debug, PartialEq)]
pub struct Subpath {
    /// Where it starts.
    pub start: Point,
    /// Its pieces, in order.
    pub segments: Vec<Segment>,
    /// Whether it ends where it started (`Z`).
    pub closed: bool,
}

/// A subpath in straight pieces.
#[derive(Clone, Debug, PartialEq)]
pub struct Polyline {
    /// The corners. A closed one does not repeat its first point.
    pub points: Vec<Point>,
    /// Whether the last point joins the first.
    pub closed: bool,
}

impl Polyline {
    /// The pieces, as pairs of indices into `points`: the closing piece
    /// too when it is closed.
    pub fn edges(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        let count = self.points.len();
        let pieces = if self.closed && count > 1 {
            count
        } else {
            count.saturating_sub(1)
        };
        (0..pieces).map(move |index| (index, (index + 1) % count))
    }
}

/// Reads numbers and letters out of path data.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    /// Steps over spaces and commas.
    fn skip(&mut self) {
        while self
            .peek()
            .is_some_and(|byte| byte.is_ascii_whitespace() || byte == b',')
        {
            self.at += 1;
        }
    }

    fn digits(&mut self) -> usize {
        let from = self.at;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.at += 1;
        }
        self.at - from
    }

    fn number(&mut self) -> Result<f32, PathError> {
        self.skip();
        let from = self.at;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.at += 1;
        }
        let mut digits = self.digits();
        if self.peek() == Some(b'.') {
            self.at += 1;
            digits += self.digits();
        }
        if digits == 0 {
            return Err(PathError {
                fault: PathFault::MissingNumber,
                at: from,
            });
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let mark = self.at;
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if self.digits() == 0 {
                self.at = mark;
            }
        }
        std::str::from_utf8(&self.bytes[from..self.at])
            .ok()
            .and_then(|text| text.parse::<f32>().ok())
            .filter(|number| number.is_finite())
            .ok_or(PathError {
                fault: PathFault::BadNumber,
                at: from,
            })
    }

    fn point(&mut self, base: Point) -> Result<Point, PathError> {
        Ok([base[0] + self.number()?, base[1] + self.number()?])
    }

    fn flag(&mut self) -> Result<bool, PathError> {
        self.skip();
        let flag = match self.peek() {
            Some(b'0') => false,
            Some(b'1') => true,
            _ => {
                return Err(PathError {
                    fault: PathFault::BadFlag,
                    at: self.at,
                });
            }
        };
        self.at += 1;
        Ok(flag)
    }
}

/// Reads path data into subpaths. A subpath is kept when it draws
/// something: at least one piece, or a lone point that is closed (`M x y
/// Z`, which the stroker draws as a dot).
///
/// # Errors
/// A letter that is no command, a drawing command before the first `M`, a
/// command that runs out of numbers, a number or an arc flag that cannot
/// be read.
pub fn parse(data: &str) -> Result<Vec<Subpath>, PathError> {
    let mut reader = Reader {
        bytes: data.as_bytes(),
        at: 0,
    };
    let mut done: Vec<Subpath> = Vec::new();
    let mut open: Option<Subpath> = None;
    let mut pen = [0.0_f32; 2];
    let mut start = [0.0_f32; 2];
    let mut started = false;
    let mut last: Option<u8> = None;
    loop {
        reader.skip();
        let Some(byte) = reader.peek() else { break };
        let at = reader.at;
        let letter = if byte.is_ascii_alphabetic() {
            reader.at += 1;
            byte
        } else {
            // More numbers repeat the command; after a move they are lines.
            match last {
                Some(b'M') => b'L',
                Some(b'm') => b'l',
                Some(b'Z' | b'z') | None => {
                    return Err(PathError {
                        fault: PathFault::UnknownCommand,
                        at,
                    });
                }
                Some(letter) => letter,
            }
        };
        last = Some(letter);
        let base = if letter.is_ascii_lowercase() {
            pen
        } else {
            [0.0, 0.0]
        };
        let segment = match letter.to_ascii_uppercase() {
            b'M' => {
                let to = reader.point(base)?;
                done.extend(open.take().filter(|path| !path.segments.is_empty()));
                open = Some(Subpath {
                    start: to,
                    segments: Vec::new(),
                    closed: false,
                });
                pen = to;
                start = to;
                started = true;
                continue;
            }
            b'Z' => {
                if !started {
                    return Err(PathError {
                        fault: PathFault::NoMoveTo,
                        at,
                    });
                }
                if let Some(mut path) = open.take() {
                    path.closed = true;
                    done.push(path);
                }
                pen = start;
                continue;
            }
            b'L' => Segment::Line(reader.point(base)?),
            b'H' => Segment::Line([base[0] + reader.number()?, pen[1]]),
            b'V' => Segment::Line([pen[0], base[1] + reader.number()?]),
            b'Q' => Segment::Quad(reader.point(base)?, reader.point(base)?),
            b'C' => Segment::Cubic(
                reader.point(base)?,
                reader.point(base)?,
                reader.point(base)?,
            ),
            b'A' => Segment::Arc {
                radii: [reader.number()?, reader.number()?],
                rotation: reader.number()?,
                large: reader.flag()?,
                sweep: reader.flag()?,
                to: reader.point(base)?,
            },
            _ => {
                return Err(PathError {
                    fault: PathFault::UnknownCommand,
                    at,
                });
            }
        };
        if !started {
            return Err(PathError {
                fault: PathFault::NoMoveTo,
                at,
            });
        }
        // After a `Z` the pen is back at the start and draws on from there.
        open.get_or_insert_with(|| Subpath {
            start,
            segments: Vec::new(),
            closed: false,
        })
        .segments
        .push(segment);
        pen = segment.end();
    }
    done.extend(open.filter(|path| !path.segments.is_empty()));
    Ok(done)
}

fn length(a: Point) -> f32 {
    a[0].hypot(a[1])
}

/// How far two points count as the same one.
const SAME: f32 = 1e-5;

fn same(a: Point, b: Point) -> bool {
    (a[0] - b[0]).abs() <= SAME && (a[1] - b[1]).abs() <= SAME
}

/// The most pieces one curve is cut into.
const MOST_PIECES: f32 = 256.0;

/// The corners of an arc from `from`, after SVG's implementation notes
/// (F.6.5: from the two ends to the middle of the ellipse).
fn arc(
    from: Point,
    radii: Point,
    rotation: f32,
    (large, sweep): (bool, bool),
    to: Point,
    tolerance: f32,
    points: &mut Vec<Point>,
) {
    let (mut rx, mut ry) = (radii[0].abs(), radii[1].abs());
    if same(from, to) {
        return;
    }
    if rx <= SAME || ry <= SAME {
        points.push(to);
        return;
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
    let step = 2.0 * (1.0 - (tolerance / rx.max(ry)).min(1.0)).acos();
    // An even count puts a corner on the middle of the arc, so that the
    // top of a half circle is a corner and not a flat piece.
    let pieces = ((turn.abs() / step.max(1e-3)).ceil() / 2.0)
        .ceil()
        .clamp(1.0, MOST_PIECES / 2.0)
        * 2.0;
    let pieces = pieces as usize;
    for piece in 1..pieces {
        let (s, c) = (first + turn * piece as f32 / pieces as f32).sin_cos();
        points.push([
            middle[0] + rx * c * cos - ry * s * sin,
            middle[1] + rx * c * sin + ry * s * cos,
        ]);
    }
    points.push(to);
}

impl Subpath {
    /// The subpath in straight pieces that stay within `tolerance` of it.
    #[must_use]
    pub fn flatten(&self, tolerance: f32) -> Polyline {
        let tolerance = tolerance.max(1e-4);
        let mut points = vec![self.start];
        let mut pen = self.start;
        for segment in &self.segments {
            match *segment {
                Segment::Line(to) => points.push(to),
                Segment::Quad(control, to) => {
                    let bend = length([
                        pen[0] - 2.0 * control[0] + to[0],
                        pen[1] - 2.0 * control[1] + to[1],
                    ]);
                    let pieces = (bend / (4.0 * tolerance))
                        .sqrt()
                        .ceil()
                        .clamp(1.0, MOST_PIECES) as usize;
                    for piece in 1..=pieces {
                        let t = piece as f32 / pieces as f32;
                        let u = 1.0 - t;
                        let (a, b, c) = (u * u, 2.0 * u * t, t * t);
                        points.push([
                            a * pen[0] + b * control[0] + c * to[0],
                            a * pen[1] + b * control[1] + c * to[1],
                        ]);
                    }
                }
                Segment::Cubic(first, second, to) => {
                    let bend = length([
                        pen[0] - 2.0 * first[0] + second[0],
                        pen[1] - 2.0 * first[1] + second[1],
                    ])
                    .max(length([
                        first[0] - 2.0 * second[0] + to[0],
                        first[1] - 2.0 * second[1] + to[1],
                    ]));
                    let pieces = (3.0 * bend / (4.0 * tolerance))
                        .sqrt()
                        .ceil()
                        .clamp(1.0, MOST_PIECES) as usize;
                    for piece in 1..=pieces {
                        let t = piece as f32 / pieces as f32;
                        let u = 1.0 - t;
                        let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                        points.push([
                            a * pen[0] + b * first[0] + c * second[0] + d * to[0],
                            a * pen[1] + b * first[1] + c * second[1] + d * to[1],
                        ]);
                    }
                }
                Segment::Arc {
                    radii,
                    rotation,
                    large,
                    sweep,
                    to,
                } => arc(
                    pen,
                    radii,
                    rotation,
                    (large, sweep),
                    to,
                    tolerance,
                    &mut points,
                ),
            }
            pen = segment.end();
            // The end of every piece is exactly the point that was written.
            if let Some(end) = points.last_mut() {
                *end = pen;
            }
        }
        points.dedup_by(|a, b| same(*a, *b));
        if self.closed && points.len() > 1 && same(points[0], points[points.len() - 1]) {
            points.pop();
        }
        Polyline {
            points,
            closed: self.closed,
        }
    }
}

/// The box around everything `data` draws with a pen `weight` wide:
/// `[left, top, right, bottom]`, or `None` when it draws nothing.
///
/// # Errors
/// What [`parse`] refuses.
pub fn bounds(data: &str, weight: f32) -> Result<Option<[f32; 4]>, PathError> {
    let mut found: Option<[f32; 4]> = None;
    for path in parse(data)? {
        for point in path.flatten(0.005).points {
            let around = [
                point[0] - weight / 2.0,
                point[1] - weight / 2.0,
                point[0] + weight / 2.0,
                point[1] + weight / 2.0,
            ];
            found = Some(found.map_or(around, |b| {
                [
                    b[0].min(around[0]),
                    b[1].min(around[1]),
                    b[2].max(around[2]),
                    b[3].max(around[3]),
                ]
            }));
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Point, b: Point) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn every_command_is_read() {
        let paths = parse("M1 2 L3 4 H5 V6 C1 1 2 2 3 3 Q4 4 5 5 A2 3 30 0 1 9 9 Z").unwrap();
        assert_eq!(paths.len(), 1);
        let path = &paths[0];
        assert!(path.closed);
        assert_eq!(path.start, [1.0, 2.0]);
        assert_eq!(
            path.segments,
            vec![
                Segment::Line([3.0, 4.0]),
                Segment::Line([5.0, 4.0]),
                Segment::Line([5.0, 6.0]),
                Segment::Cubic([1.0, 1.0], [2.0, 2.0], [3.0, 3.0]),
                Segment::Quad([4.0, 4.0], [5.0, 5.0]),
                Segment::Arc {
                    radii: [2.0, 3.0],
                    rotation: 30.0,
                    large: false,
                    sweep: true,
                    to: [9.0, 9.0],
                },
            ]
        );
    }

    #[test]
    fn lower_case_is_relative_to_the_pen() {
        let relative = parse("m1 2 l2 2 h2 v2 c1 0 1 1 2 2 q1 0 1 1 a2 2 0 0 1 2 2 z").unwrap();
        let absolute = parse("M1 2 L3 4 H5 V6 C6 6 6 7 7 8 Q8 8 8 9 A2 2 0 0 1 10 11 Z").unwrap();
        assert_eq!(relative, absolute);
    }

    #[test]
    fn numbers_repeat_the_command_and_follow_a_move_as_lines() {
        let paths = parse("M0 0 4 0 4 4 M10,10 l1 0 0 1").unwrap();
        assert_eq!(paths.len(), 2);
        assert_eq!(
            paths[0].segments,
            vec![Segment::Line([4.0, 0.0]), Segment::Line([4.0, 4.0])]
        );
        assert_eq!(
            paths[1].segments,
            vec![Segment::Line([11.0, 10.0]), Segment::Line([11.0, 11.0])]
        );
    }

    #[test]
    fn numbers_may_touch_and_flags_may_too() {
        let paths = parse("M1.5.5-2-3A1 1 0 0110 10").unwrap();
        assert_eq!(paths[0].start, [1.5, 0.5]);
        assert_eq!(paths[0].segments[0], Segment::Line([-2.0, -3.0]));
        assert_eq!(
            paths[0].segments[1],
            Segment::Arc {
                radii: [1.0, 1.0],
                rotation: 0.0,
                large: false,
                sweep: true,
                to: [10.0, 10.0],
            }
        );
        assert_eq!(parse("M1e1 2.5e-1 L0 0").unwrap()[0].start, [10.0, 0.25]);
    }

    #[test]
    fn the_pen_draws_on_from_the_start_after_a_close() {
        let paths = parse("M2 2 H6 V6 Z L0 0").unwrap();
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[1].start, [2.0, 2.0]);
        assert_eq!(paths[1].segments, vec![Segment::Line([0.0, 0.0])]);
        assert!(!paths[1].closed);
    }

    #[test]
    fn a_closed_point_is_kept_and_an_open_one_is_not() {
        let paths = parse("M3 3 Z M9 9").unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].closed && paths[0].segments.is_empty());
        let dot = paths[0].flatten(0.01);
        assert_eq!(dot.points, vec![[3.0, 3.0]]);
    }

    #[test]
    fn what_cannot_be_read_says_where() {
        let fault = |data: &str| parse(data).unwrap_err();
        assert_eq!(
            fault("L1 1"),
            PathError {
                fault: PathFault::NoMoveTo,
                at: 0
            }
        );
        assert_eq!(fault("Z").fault, PathFault::NoMoveTo);
        assert_eq!(
            fault("M1 1 X2 2"),
            PathError {
                fault: PathFault::UnknownCommand,
                at: 5
            }
        );
        assert_eq!(fault("M1 1 S2 2 3 3").fault, PathFault::UnknownCommand);
        assert_eq!(
            fault("M1 1 L2"),
            PathError {
                fault: PathFault::MissingNumber,
                at: 7
            }
        );
        assert_eq!(fault("M1 1 A1 1 0 2 0 3 3").fault, PathFault::BadFlag);
        assert_eq!(fault("M1 1 L1e99 1").fault, PathFault::BadNumber);
        assert_eq!(fault("7 7").fault, PathFault::UnknownCommand);
        assert_eq!(fault("M1 1 Z 2 2").fault, PathFault::UnknownCommand);
        assert!(parse("").unwrap().is_empty());
    }

    #[test]
    fn a_line_flattens_to_itself() {
        let line = parse("M1 1 L5 1 L5 4").unwrap()[0].flatten(0.01);
        assert_eq!(line.points, vec![[1.0, 1.0], [5.0, 1.0], [5.0, 4.0]]);
        assert!(!line.closed);
        assert_eq!(line.edges().collect::<Vec<_>>(), vec![(0, 1), (1, 2)]);
    }

    #[test]
    fn a_closed_path_does_not_repeat_its_start() {
        let square = parse("M0 0 H4 V4 H0 V0 Z").unwrap()[0].flatten(0.01);
        assert_eq!(square.points.len(), 4);
        assert_eq!(square.edges().count(), 4);
        assert_eq!(square.edges().last(), Some((3, 0)));
    }

    #[test]
    fn a_circle_of_two_arcs_stays_on_its_radius() {
        let circle =
            parse("M2 12 A10 10 0 0 1 22 12 A10 10 0 0 1 2 12 Z").unwrap()[0].flatten(0.01);
        assert!(circle.points.len() > 16);
        for point in &circle.points {
            let radius = (point[0] - 12.0).hypot(point[1] - 12.0);
            assert!((radius - 10.0).abs() < 1e-3, "{point:?} is {radius} out");
        }
        // The sweep flag set goes clockwise on the screen: over the top.
        assert!(circle.points[1][1] < 12.0);
        // The top and the bottom are corners.
        assert!(circle.points.iter().any(|p| near(*p, [12.0, 2.0])));
        assert!(circle.points.iter().any(|p| near(*p, [12.0, 22.0])));
        // The pieces stay within the tolerance: the middle of each is no
        // more than that inside the circle.
        for (a, b) in circle.edges() {
            let (a, b) = (circle.points[a], circle.points[b]);
            let middle = ((a[0] + b[0]) / 2.0 - 12.0).hypot((a[1] + b[1]) / 2.0 - 12.0);
            assert!(10.0 - middle <= 0.011);
        }
    }

    #[test]
    fn the_arc_flags_pick_the_way_round() {
        let ends = |flags: &str| {
            let data = format!("M0 0 A5 5 0 {flags} 5 5");
            let line = parse(&data).unwrap()[0].flatten(0.01);
            let middle = line.points[line.points.len() / 2];
            (line.points.len(), middle)
        };
        // Small and clockwise: a quarter round the middle (0, 5).
        let (count, middle) = ends("0 1");
        assert!(near(middle, [3.5355, 1.4645]), "{middle:?}");
        // Small and the other way: a quarter round (5, 0).
        let (_, middle) = ends("0 0");
        assert!(near(middle, [1.4645, 3.5355]), "{middle:?}");
        // Large: three quarters, so about three times the corners.
        let (large, middle) = ends("1 1");
        assert!(large > 2 * count);
        assert!(near(middle, [8.5355, -3.5355]), "{middle:?}");
    }

    #[test]
    fn radii_too_small_grow_to_reach_and_none_is_a_line() {
        let half = parse("M0 0 A1 1 0 0 1 10 0").unwrap()[0].flatten(0.01);
        let top = half
            .points
            .iter()
            .map(|point| point[1])
            .fold(f32::MAX, f32::min);
        assert!((top + 5.0).abs() < 1e-3, "a half circle of radius 5: {top}");
        let line = parse("M0 0 A0 3 0 0 1 10 0").unwrap()[0].flatten(0.01);
        assert_eq!(line.points, vec![[0.0, 0.0], [10.0, 0.0]]);
    }

    #[test]
    fn curves_stay_within_the_tolerance() {
        // A quarter circle as a cubic: every corner is near radius 10.
        let quarter = parse("M10 0 C10 5.523 5.523 10 0 10").unwrap()[0].flatten(0.01);
        assert!(quarter.points.len() > 8);
        for point in &quarter.points {
            assert!((point[0].hypot(point[1]) - 10.0).abs() < 0.01);
        }
        assert_eq!(quarter.points.last(), Some(&[0.0, 10.0]));
        let bow = parse("M0 0 Q5 10 10 0").unwrap()[0].flatten(0.01);
        let lowest = bow.points.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        assert!((lowest - 5.0).abs() < 0.02, "{lowest}");
        // A finer tolerance cuts more pieces.
        let fine = parse("M0 0 Q5 10 10 0").unwrap()[0].flatten(0.001);
        assert!(fine.points.len() > bow.points.len());
    }

    #[test]
    fn the_box_takes_the_pen_into_account() {
        let found = bounds("M4 4 H20 M12 6 A6 6 0 0 1 12 18", 1.5)
            .unwrap()
            .unwrap();
        assert!((found[0] - 3.25).abs() < 1e-3);
        assert!((found[1] - 3.25).abs() < 1e-3);
        assert!((found[2] - 20.75).abs() < 1e-3);
        assert!((found[3] - 18.75).abs() < 1e-3);
        assert_eq!(bounds("", 1.5).unwrap(), None);
    }
}
