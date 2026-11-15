//! The stored stroke and its bytes.

use sue_core::codec::{CodecError, Reader, Writer};
use sue_model::Response;
use sue_model::brush::ContentHash;

use crate::input::{Device, Kind, Sample, Settings};
use crate::paint::Colors;

/// The version the bytes of a [`StoredStroke`] lead with.
pub const FORMAT: u16 = 1;

/// One stroke as it is kept, to be painted again. See the module.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredStroke {
    /// The engine version that painted it ([`super::ENGINE`]).
    pub engine: u32,
    /// The hash of its brush's bytes ([`sue_model::BrushPreset::to_bytes`]),
    /// which the document keeps beside its strokes, once for all of them.
    /// The brush holds the finish (post correction, flick) too.
    pub brush: ContentHash,
    /// The seed of its chances.
    pub seed: u64,
    /// The device that drew it.
    pub device: Device,
    /// The person's own pressure curve when it was drawn.
    pub curve: Response,
    /// Screen points per layer pixel when it was drawn (the zoom).
    pub scale: f32,
    /// The arc length between two points, when it was set.
    pub step: Option<f32>,
    /// Its colors.
    pub colors: Colors,
    /// The pen's samples in layer pixels, time counted in seconds from
    /// the landing: a [`Kind::Down`], [`Kind::Move`]s, and a [`Kind::Up`]
    /// once the pen lifted.
    pub samples: Vec<Sample>,
    /// The copies a symmetry ruler made, each `[a, b, c, d, e, f]`
    /// mapping `(x, y)` to `(a x + b y + e, c x + d y + f)`; empty for a
    /// stroke drawn once.
    pub copies: Vec<[f32; 6]>,
    /// Fields this build does not know, kept as they were read and
    /// written back unchanged.
    pub extra: Vec<(String, Vec<u8>)>,
}

impl StoredStroke {
    /// An empty stroke of engine `engine` with `brush`, `seed` and
    /// `settings` (whose `to_layer` is not kept: samples are kept in
    /// layer space), in `colors`.
    #[must_use]
    pub fn new(
        engine: u32,
        brush: ContentHash,
        settings: &Settings,
        colors: Colors,
        copies: Vec<[f32; 6]>,
    ) -> Self {
        Self {
            engine,
            brush,
            seed: settings.seed,
            device: settings.device,
            curve: settings.curve,
            scale: settings.scale,
            step: settings.step,
            colors,
            samples: Vec::new(),
            copies,
            extra: Vec::new(),
        }
    }

    /// The pipeline's settings to paint it again with: its own, in
    /// layer space.
    #[must_use]
    pub fn settings(&self) -> Settings {
        Settings {
            device: self.device,
            curve: self.curve,
            to_layer: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            scale: self.scale,
            seed: self.seed,
            step: self.step,
        }
    }

    /// `true` once the pen has lifted (the last sample is the lift).
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.samples.last().is_some_and(|s| s.kind == Kind::Up)
    }

    /// The stroke as bytes: [`FORMAT`], then each field as a name and its
    /// bytes, the fields this build does not know last. The same stroke
    /// always gives the same bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Writer::new();
        out.varint(u64::from(FORMAT));
        let mut fields: Vec<(&str, Vec<u8>)> = Vec::new();
        let mut field = |name: &'static str, write: &dyn Fn(&mut Writer)| {
            let mut bytes = Writer::new();
            write(&mut bytes);
            fields.push((name, bytes.into_bytes()));
        };
        field("engine", &|w| w.varint(u64::from(self.engine)));
        field("brush", &|w| w.raw(&self.brush));
        field("seed", &|w| w.u64(self.seed));
        field("device", &|w| match self.device {
            Device::Pen => w.u8(0),
            Device::Constant(pressure) => {
                w.u8(1);
                w.f32(pressure);
            }
        });
        field("curve", &|w| {
            let points = self.curve.points();
            w.len(points.len());
            for [x, y] in points {
                w.f32(*x);
                w.f32(*y);
            }
        });
        field("scale", &|w| w.f32(self.scale));
        if let Some(step) = self.step {
            field("step", &|w| w.f32(step));
        }
        field("colors", &|w| {
            w.f32s(&[
                self.colors.main[0],
                self.colors.main[1],
                self.colors.main[2],
                self.colors.sub[0],
                self.colors.sub[1],
                self.colors.sub[2],
            ]);
        });
        field("samples", &|w| {
            w.len(self.samples.len());
            for sample in &self.samples {
                write_sample(w, sample);
            }
        });
        if !self.copies.is_empty() {
            field("copies", &|w| {
                let flat: Vec<f32> = self.copies.iter().flatten().copied().collect();
                w.f32s(&flat);
            });
        }
        out.len(fields.len() + self.extra.len());
        for (name, bytes) in &fields {
            out.str(name);
            out.bytes(bytes);
        }
        for (name, bytes) in &self.extra {
            out.str(name);
            out.bytes(bytes);
        }
        out.into_bytes()
    }

    /// A stroke read from [`Self::to_bytes`].
    ///
    /// # Errors
    /// If the bytes are malformed, written by a newer format than
    /// [`FORMAT`], or lack a field every stroke has.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut input = Reader::new(bytes);
        let format = input.varint()?;
        if format == 0 || format > u64::from(FORMAT) {
            return Err(input.error(format!(
                "stored stroke format {format} is not one this build reads (1 to {FORMAT})"
            )));
        }
        let mut stroke = Self {
            engine: u32::MAX,
            brush: [0; 32],
            seed: 0,
            device: Device::Pen,
            curve: Response::LINEAR,
            scale: 1.0,
            step: None,
            colors: Colors {
                main: [0.0; 3],
                sub: [1.0; 3],
            },
            samples: Vec::new(),
            copies: Vec::new(),
            extra: Vec::new(),
        };
        let mut seen = (false, false, false);
        let count = input.len(1)?;
        for _ in 0..count {
            let name = input.str()?;
            let bytes = input.bytes()?;
            let mut r = Reader::new(bytes);
            match name.as_str() {
                "engine" => {
                    stroke.engine = u32::try_from(r.varint()?)
                        .map_err(|_| r.error("an engine version past u32"))?;
                    seen.0 = true;
                }
                "brush" => {
                    stroke.brush.copy_from_slice(r.raw(32)?);
                    seen.1 = true;
                }
                "seed" => stroke.seed = r.u64()?,
                "device" => {
                    stroke.device = match r.u8()? {
                        0 => Device::Pen,
                        1 => Device::Constant(r.f32()?),
                        other => return Err(r.error(format!("device kind {other}"))),
                    };
                }
                "curve" => {
                    let n = r.len(8)?;
                    let mut points = Vec::with_capacity(n);
                    for _ in 0..n {
                        points.push([r.f32()?, r.f32()?]);
                    }
                    stroke.curve = Response::new(&points);
                }
                "scale" => stroke.scale = r.f32()?,
                "step" => stroke.step = Some(r.f32()?),
                "colors" => {
                    let c = r.f32s()?;
                    if c.len() != 6 {
                        return Err(r.error("colors are six numbers"));
                    }
                    stroke.colors = Colors {
                        main: [c[0], c[1], c[2]],
                        sub: [c[3], c[4], c[5]],
                    };
                }
                "samples" => {
                    let n = r.len(9)?;
                    let mut samples = Vec::with_capacity(n);
                    for _ in 0..n {
                        samples.push(read_sample(&mut r)?);
                    }
                    stroke.samples = samples;
                    seen.2 = true;
                }
                "copies" => {
                    let flat = r.f32s()?;
                    if flat.len() % 6 != 0 {
                        return Err(r.error("a copy is six numbers"));
                    }
                    stroke.copies = flat.as_chunks::<6>().0.to_vec();
                }
                _ => {
                    stroke.extra.push((name, bytes.to_vec()));
                    continue;
                }
            }
            if !r.is_empty() {
                return Err(r.error(format!("{} bytes left in field {name}", r.remaining())));
            }
        }
        if !input.is_empty() {
            return Err(input.error("bytes after the last field"));
        }
        match seen {
            (false, _, _) => Err(input.error("a stored stroke names its engine")),
            (_, false, _) => Err(input.error("a stored stroke names its brush")),
            (_, _, false) => Err(input.error("a stored stroke has its samples")),
            _ => Ok(stroke),
        }
    }
}

/// Which of a sample's optional parts it has.
const PRESSURE: u8 = 1;
const TILT: u8 = 2;
const TWIST: u8 = 4;

fn write_sample(w: &mut Writer, sample: &Sample) {
    w.u8(match sample.kind {
        Kind::Down => 0,
        Kind::Move => 1,
        Kind::Up => 2,
    });
    let flags = (u8::from(sample.pressure.is_some()) * PRESSURE)
        | (u8::from(sample.tilt.is_some()) * TILT)
        | (u8::from(sample.twist.is_some()) * TWIST);
    w.u8(flags);
    w.f32(sample.pos[0]);
    w.f32(sample.pos[1]);
    w.f64(sample.time);
    if let Some(pressure) = sample.pressure {
        w.f32(pressure);
    }
    if let Some([x, y]) = sample.tilt {
        w.f32(x);
        w.f32(y);
    }
    if let Some(twist) = sample.twist {
        w.f32(twist);
    }
}

fn read_sample(r: &mut Reader<'_>) -> Result<Sample, CodecError> {
    let kind = match r.u8()? {
        0 => Kind::Down,
        1 => Kind::Move,
        2 => Kind::Up,
        other => return Err(r.error(format!("sample kind {other}"))),
    };
    let flags = r.u8()?;
    if flags & !(PRESSURE | TILT | TWIST) != 0 {
        return Err(r.error(format!("sample flags {flags}")));
    }
    let pos = [r.f32()?, r.f32()?];
    let time = r.f64()?;
    let pressure = if flags & PRESSURE != 0 {
        Some(r.f32()?)
    } else {
        None
    };
    let tilt = if flags & TILT != 0 {
        Some([r.f32()?, r.f32()?])
    } else {
        None
    };
    let twist = if flags & TWIST != 0 {
        Some(r.f32()?)
    } else {
        None
    };
    Ok(Sample {
        pos,
        time,
        pressure,
        tilt,
        twist,
        kind,
    })
}
