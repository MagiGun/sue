//! Which codec should a tile of pixels be stored with?
//!
//! The 0.0.8 plan moves pixels out of the arena into a store of 256 pixel
//! tiles: a tile of one value costs nothing, any other tile is one blob
//! with a codec byte. This benchmark gives the numbers the choice of the
//! default codec is made from. It generates pictures, cuts each layer into
//! 256 pixel tiles the way the store will, and encodes every tile that is
//! not of one value with every candidate:
//!
//! - `raw`: the pixels as they are;
//! - `deflate` at levels 1, 3 and 6 (`miniz_oxide`, no zlib frame), on
//!   the pixels as they are; after the `sub` predictor (each byte minus
//!   the same channel of the pixel to its left, as PNG's Sub filter);
//!   after the `med` predictor (each byte minus the median of left, up
//!   and left + up - upleft, as in LOCO-I and JPEG-LS); and as `planes`
//!   (all alpha, then all red, green and blue, each plane with `med`),
//!   which is what the research for the plan proposed;
//! - lossless JPEG XL (`zune-jpegxl`) at four efforts, decoded with
//!   `jxl-oxide` on one thread.
//!
//! Every blob is decoded again and compared with the tile, so a number is
//! only printed for a codec that gives the pixels back.
//!
//! Run it in a release build, the timings of a debug build mean nothing:
//!
//! ```text
//! cargo test --release -p sue-core --test codec_bench -- --ignored --nocapture
//! ```
//!
//! The pictures are generated, never loaded: flat color shapes, line art
//! on a transparent layer, noise, a smooth gradient, a photo-like field
//! with grain, and a painting of 20 layers (paper, sketch and ink lines,
//! flat fills, soft brush work). Sizes: 512, 1024 and 2048 pixels square
//! and A4 at 350 dpi (2894 x 4093).

use std::fmt::Write as _;
use std::io::Cursor;
use std::time::{Duration, Instant};

use jxl_oxide::{JxlImage, JxlThreadPool};
use miniz_oxide::deflate::compress_to_vec;
use miniz_oxide::inflate::decompress_to_vec_with_limit;
use zune_core::bit_depth::BitDepth;
use zune_core::colorspace::ColorSpace;
use zune_core::options::EncoderOptions;
use zune_jpegxl::JxlSimpleEncoder;

/// The side of a storage tile, in pixels.
const TILE: usize = 256;
/// Bytes per pixel: every picture here is 8-bit RGBA.
const BPP: usize = 4;

// Random numbers that are the same on every machine and every run.

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// In `0.0..1.0`.
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1_u64 << 24) as f32
    }

    fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }

    fn color(&mut self) -> [u8; 3] {
        let bits = self.next();
        [(bits >> 8) as u8, (bits >> 24) as u8, (bits >> 40) as u8]
    }
}

// A layer to draw on.

struct Canvas {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

/// A rectangle of a canvas a layer's content stays in: left, top, right,
/// bottom.
type Region = [f32; 4];

impl Canvas {
    /// A transparent layer.
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height * BPP],
        }
    }

    fn whole(&self) -> Region {
        [0.0, 0.0, self.width as f32, self.height as f32]
    }

    /// A random part of the canvas that covers between `low` and `high`
    /// of each side.
    fn part(&self, rng: &mut Rng, low: f32, high: f32) -> Region {
        let (width, height) = (self.width as f32, self.height as f32);
        let (w, h) = (width * rng.range(low, high), height * rng.range(low, high));
        let (x, y) = (rng.range(0.0, width - w), rng.range(0.0, height - h));
        [x, y, x + w, y + h]
    }

    fn fill(&mut self, color: [u8; 3]) {
        for pixel in self.pixels.as_chunks_mut::<BPP>().0 {
            *pixel = [color[0], color[1], color[2], 255];
        }
    }

    /// A hard-edged opaque rectangle.
    fn rect(&mut self, region: Region, color: [u8; 3]) {
        let [x0, y0, x1, y1] = region.map(|edge| edge.max(0.0) as usize);
        for y in y0..y1.min(self.height) {
            let row = y * self.width;
            for x in x0..x1.min(self.width) {
                let at = (row + x) * BPP;
                self.pixels[at..at + BPP].copy_from_slice(&[color[0], color[1], color[2], 255]);
            }
        }
    }

    /// Puts `color` over the pixel at `at` with coverage `alpha`.
    fn over(&mut self, at: usize, color: [u8; 3], alpha: f32) {
        let pixel = &mut self.pixels[at..at + BPP];
        let under = f32::from(pixel[3]) / 255.0;
        let out = alpha + under * (1.0 - alpha);
        if out <= 0.0 {
            return;
        }
        for channel in 0..3 {
            let mixed = (f32::from(color[channel]) * alpha
                + f32::from(pixel[channel]) * under * (1.0 - alpha))
                / out;
            pixel[channel] = mixed.round().clamp(0.0, 255.0) as u8;
        }
        pixel[3] = (out * 255.0).round().clamp(0.0, 255.0) as u8;
    }

    /// One dab of a round brush: `soft` falls off from the middle like an
    /// airbrush, otherwise the edge is one antialiased pixel like a pen.
    fn dab(&mut self, cx: f32, cy: f32, radius: f32, color: [u8; 3], flow: f32, soft: bool) {
        let reach = radius + 1.0;
        let x0 = (cx - reach).floor().max(0.0) as usize;
        let y0 = (cy - reach).floor().max(0.0) as usize;
        let x1 = ((cx + reach).ceil().max(0.0) as usize).min(self.width);
        let y1 = ((cy + reach).ceil().max(0.0) as usize).min(self.height);
        for y in y0..y1 {
            for x in x0..x1 {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let distance = (dx * dx + dy * dy).sqrt();
                let cover = if soft {
                    let t = (distance / radius).min(1.0);
                    (1.0 - t * t) * (1.0 - t * t)
                } else {
                    (radius + 0.5 - distance).clamp(0.0, 1.0)
                };
                if cover > 0.0 {
                    self.over((y * self.width + x) * BPP, color, cover * flow);
                }
            }
        }
    }

    /// A wandering stroke of `length` pixels that stays inside `region`
    /// and swells and thins like a pressed pen.
    #[allow(clippy::too_many_arguments)]
    fn stroke(
        &mut self,
        rng: &mut Rng,
        region: Region,
        radius: f32,
        length: f32,
        color: [u8; 3],
        flow: f32,
        soft: bool,
    ) {
        let [x0, y0, x1, y1] = region;
        let (mut x, mut y) = (rng.range(x0, x1), rng.range(y0, y1));
        let mut angle = rng.range(0.0, std::f32::consts::TAU);
        let mut turn = 0.0_f32;
        let spacing = (radius * if soft { 0.35 } else { 0.5 }).max(0.5);
        let steps = (length / spacing) as usize;
        for step in 0..steps {
            let along = step as f32 / steps.max(1) as f32;
            let pressure = 0.35 + 0.65 * (along * std::f32::consts::PI).sin();
            self.dab(x, y, (radius * pressure).max(0.4), color, flow, soft);
            turn = (turn + rng.range(-0.02, 0.02)).clamp(-0.08, 0.08);
            angle += turn;
            x += angle.cos() * spacing;
            y += angle.sin() * spacing;
            if x < x0 || x > x1 || y < y0 || y > y1 {
                angle += std::f32::consts::FRAC_PI_2;
                x = x.clamp(x0, x1);
                y = y.clamp(y0, y1);
            }
        }
    }
}

/// A smooth random field: `octaves` layers of value noise, the first with
/// cells of `cell` pixels, each next one half as large and half as strong.
/// Values lie in `0.0..1.0`.
fn field(width: usize, height: usize, cell: usize, octaves: u32, seed: u64) -> Vec<f32> {
    let mut out = vec![0.0_f32; width * height];
    let mut strength = 0.5_f32;
    let mut cell = cell.max(2);
    for octave in 0..octaves {
        let (columns, rows) = (width / cell + 2, height / cell + 2);
        let mut rng = Rng::new(seed ^ (u64::from(octave) + 1).wrapping_mul(0xD1B5_4A32_D192_ED03));
        let lattice: Vec<f32> = (0..columns * rows).map(|_| rng.unit()).collect();
        for y in 0..height {
            let (row, fy) = (y / cell, (y % cell) as f32 / cell as f32);
            let fy = fy * fy * (3.0 - 2.0 * fy);
            for x in 0..width {
                let (column, fx) = (x / cell, (x % cell) as f32 / cell as f32);
                let fx = fx * fx * (3.0 - 2.0 * fx);
                let at = row * columns + column;
                let top = lattice[at] + (lattice[at + 1] - lattice[at]) * fx;
                let bottom = lattice[at + columns]
                    + (lattice[at + columns + 1] - lattice[at + columns]) * fx;
                out[y * width + x] += (top + (bottom - top) * fy) * strength;
            }
        }
        strength *= 0.5;
        cell = (cell / 2).max(2);
    }
    out
}

// The pictures.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Picture {
    Flat,
    LineArt,
    Noise,
    Gradient,
    Photo,
    Layers20,
}

impl Picture {
    const ALL: [Self; 6] = [
        Self::Flat,
        Self::LineArt,
        Self::Noise,
        Self::Gradient,
        Self::Photo,
        Self::Layers20,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::LineArt => "line art",
            Self::Noise => "noise",
            Self::Gradient => "gradient",
            Self::Photo => "photo-like",
            Self::Layers20 => "20 layers",
        }
    }

    fn layers(self) -> usize {
        if self == Self::Layers20 { 20 } else { 1 }
    }

    /// Layer `index` of the picture at `width` x `height`. The same
    /// arguments always give the same pixels.
    fn layer(self, index: usize, width: usize, height: usize) -> Canvas {
        // Brush sizes follow the picture: a line of 3 pixels on a 1024
        // pixel sketch is 8 pixels wide on an A4 page at 350 dpi.
        let scale = width.min(height) as f32 / 1024.0;
        let mut rng = Rng::new(self as u64 * 1000 + index as u64 + 1);
        let mut canvas = Canvas::new(width, height);
        match self {
            Self::Flat => flat(&mut canvas, &mut rng),
            Self::LineArt => {
                let region = canvas.whole();
                lines(&mut canvas, &mut rng, region, scale, 40, [20, 20, 24]);
            }
            Self::Noise => {
                for chunk in canvas.pixels.chunks_mut(8) {
                    let bytes = rng.next().to_le_bytes();
                    chunk.copy_from_slice(&bytes[..chunk.len()]);
                }
            }
            Self::Gradient => gradient(&mut canvas),
            Self::Photo => photo(&mut canvas),
            Self::Layers20 => painting_layer(&mut canvas, &mut rng, index, scale),
        }
        canvas
    }
}

/// Flat color shapes with hard edges on an opaque ground, as in cel
/// shading or a comic's flats.
fn flat(canvas: &mut Canvas, rng: &mut Rng) {
    canvas.fill([236, 232, 220]);
    for _ in 0..14 {
        let region = canvas.part(rng, 0.08, 0.45);
        let color = rng.color();
        if rng.unit() < 0.5 {
            canvas.rect(region, color);
        } else {
            // A hard-edged disc in the rectangle.
            let [x0, y0, x1, y1] = region;
            let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            let radius = (x1 - x0).min(y1 - y0) / 2.0;
            for y in y0 as usize..(y1 as usize).min(canvas.height) {
                for x in x0 as usize..(x1 as usize).min(canvas.width) {
                    let (dx, dy) = (x as f32 - cx, y as f32 - cy);
                    if dx * dx + dy * dy < radius * radius {
                        let at = (y * canvas.width + x) * BPP;
                        canvas.pixels[at..at + BPP]
                            .copy_from_slice(&[color[0], color[1], color[2], 255]);
                    }
                }
            }
        }
    }
}

/// Antialiased pen lines.
fn lines(
    canvas: &mut Canvas,
    rng: &mut Rng,
    region: Region,
    scale: f32,
    count: usize,
    ink: [u8; 3],
) {
    let reach = (region[2] - region[0]).max(region[3] - region[1]);
    for _ in 0..count {
        let radius = rng.range(1.0, 2.6) * scale.max(0.5);
        let length = reach * rng.range(0.3, 1.2);
        canvas.stroke(rng, region, radius, length, ink, 1.0, false);
    }
}

/// A smooth opaque blend of three colors, part straight and part round.
fn gradient(canvas: &mut Canvas) {
    let (width, height) = (canvas.width as f32, canvas.height as f32);
    // One row per channel: the three colors that are blended.
    let colors: [[f32; 3]; 3] = [
        [250.0, 60.0, 20.0],
        [180.0, 90.0, 20.0],
        [60.0, 200.0, 40.0],
    ];
    for y in 0..canvas.height {
        for x in 0..canvas.width {
            let (u, v) = (x as f32 / width, y as f32 / height);
            let straight = (u * 0.7 + v * 0.3).clamp(0.0, 1.0);
            let round = (((u - 0.3).powi(2) + (v - 0.6).powi(2)).sqrt() * 1.4).clamp(0.0, 1.0);
            let at = (y * canvas.width + x) * BPP;
            for (channel, [from, to, middle]) in colors.iter().enumerate() {
                let first = from + (to - from) * straight;
                let value = first + (middle - first) * round;
                canvas.pixels[at + channel] = value.round() as u8;
            }
            canvas.pixels[at + 3] = 255;
        }
    }
}

/// What a photograph is to a codec: structure at every scale, colors that
/// move together, and grain in every pixel.
fn photo(canvas: &mut Canvas) {
    let (width, height) = (canvas.width, canvas.height);
    let light = field(width, height, 256, 7, 11);
    let warm = field(width, height, 192, 3, 23);
    let cool = field(width, height, 192, 3, 37);
    let mut grain = Rng::new(51);
    for (index, pixel) in canvas
        .pixels
        .as_chunks_mut::<BPP>()
        .0
        .iter_mut()
        .enumerate()
    {
        let base = light[index] * 255.0;
        let bits = grain.next();
        let noise = |shift: u32| ((bits >> shift) & 7) as f32 - 3.5;
        let red = base + (warm[index] - 0.5) * 90.0 + noise(0);
        let green = base + noise(8);
        let blue = base + (cool[index] - 0.5) * 90.0 + noise(16);
        *pixel = [
            red.round().clamp(0.0, 255.0) as u8,
            green.round().clamp(0.0, 255.0) as u8,
            blue.round().clamp(0.0, 255.0) as u8,
            255,
        ];
    }
}

/// One layer of a painting of 20: paper, two sketch layers, three ink
/// layers, six layers of flat fills, six of soft brush work and two of
/// small highlights. Most layers cover only a part of the page, as the
/// layers of a real painting do.
fn painting_layer(canvas: &mut Canvas, rng: &mut Rng, index: usize, scale: f32) {
    let reach = canvas.width.max(canvas.height) as f32;
    match index {
        0 => canvas.fill([244, 240, 230]),
        1 | 2 => {
            let region = canvas.part(rng, 0.6, 0.95);
            lines(canvas, rng, region, scale * 0.7, 60, [70, 90, 160]);
        }
        3..=5 => {
            let region = canvas.part(rng, 0.4, 0.9);
            lines(canvas, rng, region, scale, 35, [16, 14, 18]);
        }
        6..=11 => {
            let region = canvas.part(rng, 0.25, 0.6);
            for _ in 0..5 {
                let color = rng.color();
                let radius = rng.range(18.0, 46.0) * scale;
                canvas.stroke(rng, region, radius, reach * 0.5, color, 1.0, false);
            }
        }
        12..=17 => {
            let region = canvas.part(rng, 0.3, 0.7);
            for _ in 0..8 {
                let color = rng.color();
                let radius = rng.range(20.0, 60.0) * scale;
                let flow = rng.range(0.05, 0.3);
                canvas.stroke(rng, region, radius, reach * 0.35, color, flow, true);
            }
        }
        _ => {
            for _ in 0..12 {
                let region = canvas.part(rng, 0.03, 0.1);
                let radius = rng.range(3.0, 9.0) * scale;
                canvas.stroke(
                    rng,
                    region,
                    radius,
                    reach * 0.05,
                    [255, 255, 250],
                    0.6,
                    true,
                );
            }
        }
    }
}

// The codecs.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Predictor {
    None,
    Sub,
    Med,
    Planes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Codec {
    Raw,
    Deflate { level: u8, predictor: Predictor },
    Jxl { effort: u8 },
}

impl Codec {
    fn all() -> Vec<Self> {
        let mut all = vec![Self::Raw];
        for predictor in [
            Predictor::None,
            Predictor::Sub,
            Predictor::Med,
            Predictor::Planes,
        ] {
            for level in [1, 3, 6] {
                all.push(Self::Deflate { level, predictor });
            }
        }
        // The encoder's effort is how many rows of a tile it samples to
        // build its code: 4 is its default, 127 samples nearly all.
        for effort in [1, 4, 16, 127] {
            all.push(Self::Jxl { effort });
        }
        all
    }

    fn name(self) -> String {
        match self {
            Self::Raw => "raw".to_owned(),
            Self::Deflate { level, predictor } => match predictor {
                Predictor::None => format!("deflate {level}"),
                Predictor::Sub => format!("sub + deflate {level}"),
                Predictor::Med => format!("med + deflate {level}"),
                Predictor::Planes => format!("planes + deflate {level}"),
            },
            Self::Jxl { effort } => format!("jpeg xl effort {effort}"),
        }
    }

    fn encode(self, tile: &[u8], width: usize, height: usize) -> Vec<u8> {
        match self {
            Self::Raw => tile.to_vec(),
            Self::Deflate { level, predictor } => match predictor {
                Predictor::None => compress_to_vec(tile, level),
                Predictor::Sub => {
                    let mut residue = tile.to_vec();
                    sub_forward(&mut residue, width);
                    compress_to_vec(&residue, level)
                }
                Predictor::Med => {
                    let mut residue = tile.to_vec();
                    med_forward(&mut residue, width * BPP, BPP);
                    compress_to_vec(&residue, level)
                }
                Predictor::Planes => compress_to_vec(&planes_forward(tile, width), level),
            },
            Self::Jxl { effort } => {
                let options = EncoderOptions::new(width, height, ColorSpace::RGBA, BitDepth::Eight)
                    .set_effort(effort)
                    .set_num_threads(1);
                let mut blob = Vec::new();
                JxlSimpleEncoder::new(tile, options)
                    .encode(&mut blob)
                    .expect("JPEG XL encodes an RGBA tile");
                blob
            }
        }
    }

    fn decode(self, blob: &[u8], width: usize, height: usize) -> Vec<u8> {
        let len = width * height * BPP;
        match self {
            Self::Raw => blob.to_vec(),
            Self::Deflate { predictor, .. } => {
                let mut tile = decompress_to_vec_with_limit(blob, len).expect("deflate inflates");
                match predictor {
                    Predictor::None => {}
                    Predictor::Sub => sub_inverse(&mut tile, width),
                    Predictor::Med => med_inverse(&mut tile, width * BPP, BPP),
                    Predictor::Planes => tile = planes_inverse(tile, width),
                }
                tile
            }
            Self::Jxl { .. } => {
                let image = JxlImage::builder()
                    .pool(JxlThreadPool::none())
                    .read(Cursor::new(blob))
                    .expect("JPEG XL reads its own blob");
                let render = image.render_frame(0).expect("JPEG XL renders");
                let mut stream = render.stream();
                assert_eq!(stream.channels() as usize, BPP);
                let mut tile = vec![0_u8; len];
                assert_eq!(stream.write_to_buffer(&mut tile), len);
                tile
            }
        }
    }
}

/// Each byte minus the same channel of the pixel to its left.
fn sub_forward(tile: &mut [u8], width: usize) {
    for row in tile.chunks_exact_mut(width * BPP) {
        for at in (BPP..row.len()).rev() {
            row[at] = row[at].wrapping_sub(row[at - BPP]);
        }
    }
}

fn sub_inverse(tile: &mut [u8], width: usize) {
    for row in tile.chunks_exact_mut(width * BPP) {
        for at in BPP..row.len() {
            row[at] = row[at].wrapping_add(row[at - BPP]);
        }
    }
}

/// The median of left, up and left + up - upleft.
fn median(left: u8, up: u8, upleft: u8) -> u8 {
    let (low, high) = (left.min(up), left.max(up));
    if upleft >= high {
        low
    } else if upleft <= low {
        high
    } else {
        // Between low and high, so the sum stays in a byte.
        (i16::from(left) + i16::from(up) - i16::from(upleft)) as u8
    }
}

/// What the `med` predictor expects at byte `at` of rows of `stride`
/// bytes whose pixels are `step` bytes apart: left only in the first row,
/// up only in the first column, nothing for the first pixel.
fn med_guess(tile: &[u8], at: usize, stride: usize, step: usize) -> u8 {
    let (first_row, first_column) = (at < stride, at % stride < step);
    match (first_row, first_column) {
        (true, true) => 0,
        (true, false) => tile[at - step],
        (false, true) => tile[at - stride],
        (false, false) => median(tile[at - step], tile[at - stride], tile[at - stride - step]),
    }
}

fn med_forward(tile: &mut [u8], stride: usize, step: usize) {
    // From the end, so every neighbor is still a pixel when it is read.
    for at in (0..tile.len()).rev() {
        tile[at] = tile[at].wrapping_sub(med_guess(tile, at, stride, step));
    }
}

fn med_inverse(tile: &mut [u8], stride: usize, step: usize) {
    for at in 0..tile.len() {
        tile[at] = tile[at].wrapping_add(med_guess(tile, at, stride, step));
    }
}

/// The tile as four planes, alpha first, then red, green and blue, each
/// with the `med` predictor inside its own plane.
fn planes_forward(tile: &[u8], width: usize) -> Vec<u8> {
    let (pixels, _) = tile.as_chunks::<BPP>();
    let mut planes = Vec::with_capacity(tile.len());
    for channel in [3, 0, 1, 2] {
        planes.extend(pixels.iter().map(|pixel| pixel[channel]));
    }
    for plane in planes.chunks_exact_mut(pixels.len()) {
        med_forward(plane, width, 1);
    }
    planes
}

fn planes_inverse(mut planes: Vec<u8>, width: usize) -> Vec<u8> {
    let count = planes.len() / BPP;
    for plane in planes.chunks_exact_mut(count) {
        med_inverse(plane, width, 1);
    }
    let mut tile = vec![0; planes.len()];
    for (plane, channel) in planes.chunks_exact(count).zip([3, 0, 1, 2]) {
        for (pixel, value) in tile.as_chunks_mut::<BPP>().0.iter_mut().zip(plane) {
            pixel[channel] = *value;
        }
    }
    tile
}

// Measuring.

#[derive(Clone, Copy, Debug, Default)]
struct Cost {
    bytes: u64,
    encode: Duration,
    decode: Duration,
}

/// What one picture (or several together) cost with every codec.
#[derive(Clone, Debug)]
struct Measured {
    /// Bytes of every layer as a flat raster, which is what 0.0.75 saves.
    raster: u64,
    /// Tiles in all layers, and how many of them are of one value.
    tiles: u64,
    solid: u64,
    /// Bytes of the tiles that are not of one value: what the codecs got.
    coded: u64,
    /// One per codec of [`Codec::all`].
    costs: Vec<Cost>,
}

impl Measured {
    fn empty() -> Self {
        Self {
            raster: 0,
            tiles: 0,
            solid: 0,
            coded: 0,
            costs: vec![Cost::default(); Codec::all().len()],
        }
    }

    fn add(&mut self, other: &Self) {
        self.raster += other.raster;
        self.tiles += other.tiles;
        self.solid += other.solid;
        self.coded += other.coded;
        for (mine, theirs) in self.costs.iter_mut().zip(&other.costs) {
            mine.bytes += theirs.bytes;
            mine.encode += theirs.encode;
            mine.decode += theirs.decode;
        }
    }
}

/// Copies the tile at tile column `tx`, row `ty` out of a layer. Tiles at
/// the right and bottom edge are as small as what is left of the layer.
fn cut_tile(canvas: &Canvas, tx: usize, ty: usize) -> (Vec<u8>, usize, usize) {
    let (x0, y0) = (tx * TILE, ty * TILE);
    let (width, height) = (TILE.min(canvas.width - x0), TILE.min(canvas.height - y0));
    let mut tile = Vec::with_capacity(width * height * BPP);
    for y in y0..y0 + height {
        let start = (y * canvas.width + x0) * BPP;
        tile.extend_from_slice(&canvas.pixels[start..start + width * BPP]);
    }
    (tile, width, height)
}

fn is_solid(tile: &[u8]) -> bool {
    let (pixels, _) = tile.as_chunks::<BPP>();
    pixels.iter().all(|pixel| *pixel == pixels[0])
}

/// Encodes and decodes every tile of every layer of `picture` with every
/// codec, `repeats` times, and keeps the fastest time of each tile.
fn measure(picture: Picture, width: usize, height: usize, repeats: usize) -> Measured {
    let codecs = Codec::all();
    let mut measured = Measured::empty();
    for index in 0..picture.layers() {
        let canvas = picture.layer(index, width, height);
        measured.raster += canvas.pixels.len() as u64;
        for ty in 0..height.div_ceil(TILE) {
            for tx in 0..width.div_ceil(TILE) {
                let (tile, tile_width, tile_height) = cut_tile(&canvas, tx, ty);
                measured.tiles += 1;
                if is_solid(&tile) {
                    measured.solid += 1;
                    continue;
                }
                measured.coded += tile.len() as u64;
                for (codec, cost) in codecs.iter().zip(&mut measured.costs) {
                    let (mut encode, mut decode) = (Duration::MAX, Duration::MAX);
                    let mut size = 0;
                    for _ in 0..repeats.max(1) {
                        let start = Instant::now();
                        let blob = codec.encode(&tile, tile_width, tile_height);
                        encode = encode.min(start.elapsed());
                        let start = Instant::now();
                        let back = codec.decode(&blob, tile_width, tile_height);
                        decode = decode.min(start.elapsed());
                        assert!(
                            back == tile,
                            "{} does not give tile {tx}, {ty} of {} back",
                            codec.name(),
                            picture.name()
                        );
                        size = blob.len();
                    }
                    cost.bytes += size as u64;
                    cost.encode += encode;
                    cost.decode += decode;
                }
            }
        }
    }
    measured
}

// The tables.

fn megabytes(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// Megabytes of tile pixels per second.
fn speed(bytes: u64, time: Duration) -> String {
    let seconds = time.as_secs_f64();
    if bytes == 0 || seconds <= 0.0 {
        return "-".to_owned();
    }
    format!("{:.0}", megabytes(bytes) / seconds)
}

/// Stored bytes as a percentage of the flat rasters.
fn percent(bytes: u64, raster: u64) -> String {
    if raster == 0 {
        return "-".to_owned();
    }
    format!("{:.2}", bytes as f64 * 100.0 / raster as f64)
}

/// A table with one row per codec and one column per entry of `columns`.
fn table(
    out: &mut String,
    title: &str,
    columns: &[(&str, &Measured)],
    cell: impl Fn(&Measured, &Cost) -> String,
) {
    let _ = writeln!(out, "{title}");
    let _ = write!(out, "{:<22}", "codec");
    for (name, _) in columns {
        let _ = write!(out, "{name:>11}");
    }
    let _ = writeln!(out);
    for (index, codec) in Codec::all().iter().enumerate() {
        let _ = write!(out, "{:<22}", codec.name());
        for (_, measured) in columns {
            let _ = write!(out, "{:>11}", cell(measured, &measured.costs[index]));
        }
        let _ = writeln!(out);
    }
    let _ = writeln!(out);
}

/// The three tables of one set of pictures.
fn report(out: &mut String, heading: &str, columns: &[(&str, &Measured)]) {
    let _ = writeln!(out, "== {heading} ==");
    let _ = writeln!(
        out,
        "{:<12}{:>12}{:>8}{:>8}{:>12}",
        "picture", "raster MB", "tiles", "solid", "coded MB"
    );
    for (name, measured) in columns {
        let _ = writeln!(
            out,
            "{:<12}{:>12.1}{:>8}{:>8}{:>12.1}",
            name,
            megabytes(measured.raster),
            measured.tiles,
            measured.solid,
            megabytes(measured.coded)
        );
    }
    let _ = writeln!(out);
    table(
        out,
        "Stored size, percent of the flat rasters (0.0.75 stores 100)",
        columns,
        |measured, cost| percent(cost.bytes, measured.raster),
    );
    table(
        out,
        "Encode, MB of tile pixels per second on one thread",
        columns,
        |measured, cost| speed(measured.coded, cost.encode),
    );
    table(
        out,
        "Decode, MB of tile pixels per second on one thread",
        columns,
        |measured, cost| speed(measured.coded, cost.decode),
    );
}

const SIZES: [(usize, usize, &str); 4] = [
    (512, 512, "512 x 512"),
    (1024, 1024, "1024 x 1024"),
    (2048, 2048, "2048 x 2048"),
    (2894, 4093, "A4 at 350 dpi, 2894 x 4093"),
];

/// The benchmark. It takes some minutes and prints its tables; nothing is
/// asserted but that every codec gives every tile back.
#[test]
#[ignore = "benchmark: run in release with --ignored --nocapture, takes minutes"]
fn tile_codecs_compared() {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Tile codec benchmark: {TILE} px tiles, 8-bit RGBA, tiles of one value cost nothing.\n"
    );
    let mut everything = Measured::empty();
    let mut without_noise = Measured::empty();
    let mut per_picture: Vec<Measured> = Picture::ALL.iter().map(|_| Measured::empty()).collect();
    for (width, height, heading) in SIZES {
        // Small pictures are a handful of tiles: time them more than once.
        let repeats = if width * height <= 1024 * 1024 { 5 } else { 1 };
        let mut all = Measured::empty();
        let results: Vec<Measured> = Picture::ALL
            .iter()
            .map(|picture| measure(*picture, width, height, repeats))
            .collect();
        for ((picture, measured), total) in Picture::ALL.iter().zip(&results).zip(&mut per_picture)
        {
            all.add(measured);
            total.add(measured);
            everything.add(measured);
            if *picture != Picture::Noise {
                without_noise.add(measured);
            }
        }
        let mut columns: Vec<(&str, &Measured)> = Picture::ALL
            .iter()
            .map(|picture| picture.name())
            .zip(&results)
            .collect();
        columns.push(("all", &all));
        report(&mut out, heading, &columns);
        // Show each size as it is done: the whole run takes minutes.
        print!("{out}");
        out.clear();
    }
    let mut columns: Vec<(&str, &Measured)> = Picture::ALL
        .iter()
        .map(|picture| picture.name())
        .zip(&per_picture)
        .collect();
    columns.push(("all", &everything));
    columns.push(("no noise", &without_noise));
    report(&mut out, "All four sizes together", &columns);
    print!("{out}");
}

// The benchmark's own parts are tested on every run, so its numbers can be
// trusted when it is run.

#[test]
fn the_pictures_are_the_same_on_every_run() {
    for picture in Picture::ALL {
        let last = picture.layers() - 1;
        let first = picture.layer(last, 96, 80);
        assert_eq!(first.pixels.len(), 96 * 80 * BPP);
        assert_eq!(first.pixels, picture.layer(last, 96, 80).pixels);
    }
    // The pictures are what their names say.
    let alpha = |picture: Picture, index| -> Vec<u8> {
        let layer = picture.layer(index, 96, 80);
        layer.pixels.iter().skip(3).step_by(BPP).copied().collect()
    };
    for opaque in [Picture::Flat, Picture::Gradient, Picture::Photo] {
        assert!(alpha(opaque, 0).iter().all(|value| *value == 255));
    }
    let ink = alpha(Picture::LineArt, 0);
    assert!(ink.contains(&0) && ink.contains(&255), "lines on nothing");
    assert!(
        is_solid(&Picture::Layers20.layer(0, 96, 80).pixels),
        "paper"
    );
    for index in 1..20 {
        let layer = alpha(Picture::Layers20, index);
        assert!(layer.contains(&0), "layer {index} leaves the page showing");
        assert!(
            layer.iter().any(|value| *value > 0),
            "layer {index} is drawn on"
        );
    }
}

#[test]
fn the_predictors_undo_themselves() {
    let canvas = Picture::Photo.layer(0, 37, 29);
    for width in [37, 1, 29 * 37] {
        let mut tile = canvas.pixels.clone();
        sub_forward(&mut tile, width);
        // A column of single pixels has no pixel to the left.
        assert_eq!(tile != canvas.pixels, width > 1);
        sub_inverse(&mut tile, width);
        assert_eq!(tile, canvas.pixels);
        med_forward(&mut tile, width * BPP, BPP);
        assert_ne!(tile, canvas.pixels);
        med_inverse(&mut tile, width * BPP, BPP);
        assert_eq!(tile, canvas.pixels);
        let planes = planes_forward(&tile, width);
        assert_ne!(planes, canvas.pixels);
        tile = planes_inverse(planes, width);
        assert_eq!(tile, canvas.pixels);
    }
    // A flat tile and a ramp leave almost nothing to store.
    let mut ramp: Vec<u8> = (0..16 * 16)
        .flat_map(|index| {
            let value = (index % 16) as u8;
            [value, value, value, 255]
        })
        .collect();
    med_forward(&mut ramp, 16 * BPP, BPP);
    assert!(ramp.iter().filter(|byte| **byte != 0).count() <= 2 * 4 * 16);
    assert_eq!(median(10, 20, 5), 20);
    assert_eq!(median(10, 20, 30), 10);
    assert_eq!(median(10, 20, 15), 15);
}

#[test]
fn every_codec_gives_the_pixels_back() {
    // `measure` compares every decoded tile with its source. 300 x 280
    // has whole tiles and narrow ones at both edges.
    let codecs = Codec::all();
    assert_eq!(codecs.len(), 17);
    for picture in [
        Picture::Flat,
        Picture::LineArt,
        Picture::Noise,
        Picture::Photo,
    ] {
        let measured = measure(picture, 300, 280, 1);
        assert_eq!(measured.raster, 300 * 280 * BPP as u64);
        assert_eq!(measured.tiles, 4);
        assert!(measured.coded <= measured.raster);
        // Raw stores what it was given.
        assert_eq!(codecs[0], Codec::Raw);
        assert_eq!(measured.costs[0].bytes, measured.coded);
        for (codec, cost) in codecs.iter().zip(&measured.costs) {
            assert!(cost.bytes > 0, "{} stored nothing", codec.name());
            if picture != Picture::Noise && *codec != Codec::Raw {
                assert!(
                    cost.bytes < measured.coded,
                    "{} made {} larger",
                    codec.name(),
                    picture.name()
                );
            }
        }
    }
    // A tile of one value is not coded at all.
    let paper = measure(Picture::Layers20, 96, 80, 1);
    assert_eq!(paper.tiles, 20);
    assert!(paper.solid >= 1);
    let mut both = Measured::empty();
    both.add(&paper);
    both.add(&paper);
    assert_eq!(both.raster, 2 * paper.raster);
    assert_eq!(both.costs[3].bytes, 2 * paper.costs[3].bytes);
    assert_eq!(percent(50, 200), "25.00");
    assert_eq!(speed(0, Duration::ZERO), "-");
}
