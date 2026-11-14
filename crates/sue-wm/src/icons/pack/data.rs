//! Path data as icon packs write it: everything [`super::super::path`]
//! reads (`M L H V C Q A Z`), and the smooth curves `S` and `T` that
//! drawing programs write, each in its absolute and relative form.
//!
//! The commands Sue's own icons use are read exactly as
//! [`super::super::path::parse`] reads them, so that a pack made from
//! Sue's own set draws the same pixels; a test holds the two readers to
//! that. A smooth curve becomes the plain curve it stands for, its first
//! control point the mirror of the one before. Unlike the reader of Sue's
//! own art, this one keeps what it read before a fault, as SVG viewers
//! do, and says where the fault is.

use super::super::path::{PathError, PathFault, Point, Segment, Subpath};

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

/// The mirror of `control` through `pen`.
fn mirror(control: Point, pen: Point) -> Point {
    [2.0 * pen[0] - control[0], 2.0 * pen[1] - control[1]]
}

/// Reads path data into subpaths, as [`super::super::path::parse`] does,
/// with `S` and `T` besides. What was read before a fault is kept, and
/// the fault is handed back with it.
#[must_use]
pub fn parse(data: &str) -> (Vec<Subpath>, Option<PathError>) {
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
    // The second control point of the last cubic and the control point
    // of the last quadratic, for `S` and `T` to mirror.
    let mut cubic: Option<Point> = None;
    let mut quad: Option<Point> = None;
    let fault = loop {
        reader.skip();
        let Some(byte) = reader.peek() else {
            break None;
        };
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
                    break Some(PathError {
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
        let segment: Result<Segment, PathError> = match letter.to_ascii_uppercase() {
            b'M' => match reader.point(base) {
                Ok(to) => {
                    done.extend(open.take().filter(|path| !path.segments.is_empty()));
                    open = Some(Subpath {
                        start: to,
                        segments: Vec::new(),
                        closed: false,
                    });
                    pen = to;
                    start = to;
                    started = true;
                    cubic = None;
                    quad = None;
                    continue;
                }
                Err(error) => Err(error),
            },
            b'Z' => {
                if !started {
                    break Some(PathError {
                        fault: PathFault::NoMoveTo,
                        at,
                    });
                }
                if let Some(mut path) = open.take() {
                    path.closed = true;
                    done.push(path);
                }
                pen = start;
                cubic = None;
                quad = None;
                continue;
            }
            b'L' => reader.point(base).map(Segment::Line),
            b'H' => reader
                .number()
                .map(|x| Segment::Line([base[0] + x, pen[1]])),
            b'V' => reader
                .number()
                .map(|y| Segment::Line([pen[0], base[1] + y])),
            b'Q' => (|| Ok(Segment::Quad(reader.point(base)?, reader.point(base)?)))(),
            b'T' => reader.point(base).map(|to| {
                let control = quad.map_or(pen, |control| mirror(control, pen));
                Segment::Quad(control, to)
            }),
            b'C' => (|| {
                Ok(Segment::Cubic(
                    reader.point(base)?,
                    reader.point(base)?,
                    reader.point(base)?,
                ))
            })(),
            b'S' => (|| {
                let first = cubic.map_or(pen, |control| mirror(control, pen));
                Ok(Segment::Cubic(
                    first,
                    reader.point(base)?,
                    reader.point(base)?,
                ))
            })(),
            b'A' => (|| {
                Ok(Segment::Arc {
                    radii: [reader.number()?, reader.number()?],
                    rotation: reader.number()?,
                    large: reader.flag()?,
                    sweep: reader.flag()?,
                    to: reader.point(base)?,
                })
            })(),
            _ => Err(PathError {
                fault: PathFault::UnknownCommand,
                at,
            }),
        };
        let segment = match segment {
            Ok(segment) => segment,
            Err(error) => break Some(error),
        };
        if !started {
            break Some(PathError {
                fault: PathFault::NoMoveTo,
                at,
            });
        }
        (cubic, quad) = match segment {
            Segment::Cubic(_, second, _) => (Some(second), None),
            Segment::Quad(control, _) => (None, Some(control)),
            _ => (None, None),
        };
        // After a `Z` the pen is back at the start and draws on from there.
        open.get_or_insert_with(|| Subpath {
            start,
            segments: Vec::new(),
            closed: false,
        })
        .segments
        .push(segment);
        pen = segment.end();
    };
    done.extend(open.filter(|path| !path.segments.is_empty()));
    (done, fault)
}

#[cfg(test)]
mod tests {
    use super::super::super::{Icon, path};
    use super::*;

    #[test]
    fn sue_s_own_art_reads_exactly_as_the_icon_reader_reads_it() {
        let mut parts = 0;
        for icon in Icon::ALL {
            let art = icon.art();
            for part in art.line.iter().chain(art.solid) {
                let (read, fault) = parse(part.data());
                assert_eq!(fault, None, "{}", icon.name());
                assert_eq!(read, path::parse(part.data()).unwrap(), "{}", icon.name());
                parts += 1;
            }
        }
        assert!(parts > Icon::ALL.len());
    }

    #[test]
    fn smooth_curves_mirror_the_control_point_before_them() {
        let (smooth, fault) = parse("M2 12 C4 4 8 4 10 12 S16 20 18 12");
        assert_eq!(fault, None);
        let (plain, _) = parse("M2 12 C4 4 8 4 10 12 C12 20 16 20 18 12");
        assert_eq!(smooth, plain);
        let (smooth, _) = parse("M2 12 Q6 4 10 12 T18 12 t4 0");
        let (plain, _) = parse("M2 12 Q6 4 10 12 Q14 20 18 12 Q22 4 22 12");
        assert_eq!(smooth, plain);
        // With no curve before it, the first control point is the pen.
        let (alone, _) = parse("M2 2 S6 6 10 2");
        let (plain, _) = parse("M2 2 C2 2 6 6 10 2");
        assert_eq!(alone, plain);
        let (alone, _) = parse("M2 2 L4 4 T10 2");
        let (plain, _) = parse("M2 2 L4 4 Q4 4 10 2");
        assert_eq!(alone, plain);
        // Relative forms.
        let (relative, _) = parse("m2 12 c2 -8 6 -8 8 0 s6 8 8 0");
        let (absolute, _) = parse("M2 12 C4 4 8 4 10 12 S16 20 18 12");
        assert_eq!(relative, absolute);
    }

    #[test]
    fn what_was_read_before_a_fault_is_kept() {
        let (read, fault) = parse("M2 2 L10 10 M4 4 L6 X");
        let fault = fault.unwrap();
        assert_eq!(fault.fault, PathFault::MissingNumber);
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].segments, vec![Segment::Line([10.0, 10.0])]);
        let (read, fault) = parse("L4 4");
        assert!(read.is_empty());
        assert_eq!(fault.unwrap().fault, PathFault::NoMoveTo);
        let (_, fault) = parse("M1 1 A2 2 0 2 0 4 4");
        assert_eq!(fault.unwrap().fault, PathFault::BadFlag);
        let (read, fault) = parse("");
        assert!(read.is_empty() && fault.is_none());
        let (_, fault) = parse("M1 1 1e999 2");
        assert_eq!(fault.unwrap().fault, PathFault::BadNumber);
    }
}
