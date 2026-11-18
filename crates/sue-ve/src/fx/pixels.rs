//! The pixels of effects.
//!
//! An [`Image`] is straight alpha RGBA, 8 bits a channel, as decoded
//! frames are. Sue's own filters from `sue-picture` do the blurs,
//! sharpening, tone and color corrections, the alpha drop shadow and the
//! mosaic; [`run_filter`] runs one in horizontal strips on every core,
//! each strip read with enough rows around it (its halo) that the result
//! is the same as one run over the whole picture, to the rounding of the
//! running sums (a test holds it to one step in 255).
//! The video effects Sue had no filter for are here: a glow from the
//! bright parts, a stroke round the alpha, a vignette, film grain,
//! chroma and luma keys with spill suppression, crop and mirror.

use sue_picture::filters::Filter;

use super::Params;

/// A picture an effect works on: straight alpha RGBA, row by row.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The pixels.
    pub rgba: Vec<u8>,
}

impl Image {
    /// A picture of `rgba`, `width` by `height`.
    #[must_use]
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            width,
            height,
            rgba,
        }
    }

    /// A clear picture.
    #[must_use]
    pub fn clear(width: u32, height: u32) -> Self {
        Self::new(width, height, vec![0; width as usize * height as usize * 4])
    }

    /// The pixel at `x`, `y` (clamped to the picture).
    #[must_use]
    pub fn at(&self, x: i64, y: i64) -> [u8; 4] {
        let x = x.clamp(0, i64::from(self.width) - 1) as usize;
        let y = y.clamp(0, i64::from(self.height) - 1) as usize;
        let index = (y * self.width as usize + x) * 4;
        self.rgba
            .get(index..index + 4)
            .map_or([0; 4], |p| [p[0], p[1], p[2], p[3]])
    }
}

/// What an effect knows besides its parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ctx {
    /// Picture pixels per source pixel: parameters in pixels (a blur's
    /// radius) are in the source's pixels and scale with the picture, so
    /// a smaller preview looks as the export does.
    pub scale: f32,
    /// The sequence frame (grain that moves changes with it).
    pub frame: i64,
}

impl Default for Ctx {
    fn default() -> Self {
        Self {
            scale: 1.0,
            frame: 0,
        }
    }
}

/// How many threads the strips take.
fn threads() -> usize {
    std::thread::available_parallelism()
        .map_or(4, std::num::NonZero::get)
        .clamp(1, 16)
}

/// Runs `work` over horizontal strips of `rows` rows of `row_bytes`
/// bytes each, in parallel. `work` gets the strip's first row and its
/// bytes.
pub fn strips(
    bytes: &mut [u8],
    row_bytes: usize,
    rows: usize,
    work: &(dyn Fn(usize, &mut [u8]) + Sync),
) {
    if rows == 0 || row_bytes == 0 {
        return;
    }
    let parts = threads().min(rows.div_ceil(16)).max(1);
    let each = rows.div_ceil(parts);
    if parts == 1 {
        work(0, bytes);
        return;
    }
    std::thread::scope(|scope| {
        for (part, chunk) in bytes.chunks_mut(each * row_bytes).enumerate() {
            scope.spawn(move || work(part * each, chunk));
        }
    });
}

/// How many rows around a strip `filter` reads: enough that a strip with
/// them comes out as the whole picture would. `None` for a filter that
/// needs the whole picture at once.
#[must_use]
pub fn halo(filter: &Filter) -> Option<usize> {
    let blur = |radius: f32| {
        let boxed = ((radius * radius * 4.0 / 3.0 + 1.0).sqrt() / 2.0)
            .round()
            .max(1.0) as usize;
        boxed * 3 + 2
    };
    Some(match *filter {
        Filter::Invert
        | Filter::BrightnessContrast { .. }
        | Filter::HueSaturation { .. }
        | Filter::Posterize { .. }
        | Filter::Binarize { .. }
        | Filter::Levels { .. }
        | Filter::ToneCurve { .. }
        | Filter::ColorBalance { .. } => 0,
        Filter::GaussianBlur { radius } => blur(radius),
        Filter::Sharpen { .. } => blur(1.0),
        Filter::Unsharp { radius, .. } => blur(radius.max(0.5)),
        Filter::MotionBlur { angle, distance } => {
            (angle.to_radians().sin().abs() * distance / 2.0).ceil() as usize + 3
        }
        Filter::DropShadow { dy, blur: soft, .. } => {
            dy.abs().ceil() as usize + if soft > 0.25 { blur(soft) } else { 0 } + 1
        }
        _ => return None,
    })
}

/// Runs Sue's `filter` over `image`, in strips on every core when it can
/// (the result is one run's over the whole picture, to a step in 255).
pub fn run_filter(filter: &Filter, image: &mut Image) {
    let (w, h) = (image.width as usize, image.height as usize);
    if w == 0 || h == 0 {
        return;
    }
    let Some(halo) = halo(filter) else {
        filter.apply(&mut image.rgba, image.width, image.height, None, [0, 0]);
        return;
    };
    let row = w * 4;
    if halo == 0 {
        strips(&mut image.rgba, row, h, &|first, part| {
            let rows = part.len() / row;
            filter.apply(part, image_width(w), rows as u32, None, [0, first as i64]);
        });
        return;
    }
    let source = image.rgba.clone();
    strips(&mut image.rgba, row, h, &|first, part| {
        let rows = part.len() / row;
        let top = first.saturating_sub(halo);
        let bottom = (first + rows + halo).min(h);
        let mut strip = source[top * row..bottom * row].to_vec();
        filter.apply(
            &mut strip,
            image_width(w),
            (bottom - top) as u32,
            None,
            [0, top as i64],
        );
        let skip = (first - top) * row;
        part.copy_from_slice(&strip[skip..skip + rows * row]);
    });
}

fn image_width(w: usize) -> u32 {
    u32::try_from(w).unwrap_or(u32::MAX)
}

/// A gaussian blur of `radius` pixels (Sue's).
pub fn blur(image: &mut Image, radius: f32) {
    if radius >= 0.25 {
        run_filter(&Filter::GaussianBlur { radius }, image);
    }
}

/// A plane of numbers (a mask, an alpha) blurred by `radius` pixels with
/// three box passes, as Sue's gaussian is.
#[must_use]
pub fn blur_plane(values: &[f32], width: usize, height: usize, radius: f32) -> Vec<f32> {
    if radius < 0.25 || width == 0 || height == 0 {
        return values.to_vec();
    }
    let boxed = ((radius * radius * 4.0 / 3.0 + 1.0).sqrt() / 2.0)
        .round()
        .max(1.0) as usize;
    let mut a = values.to_vec();
    let mut b = vec![0.0_f32; a.len()];
    for _ in 0..3 {
        box_lines(&a, &mut b, width, height, boxed, true);
        box_lines(&b, &mut a, width, height, boxed, false);
    }
    a
}

fn box_lines(
    source: &[f32],
    target: &mut [f32],
    width: usize,
    height: usize,
    radius: usize,
    rows: bool,
) {
    let span = (radius * 2 + 1) as f32;
    if rows {
        strips_f32(target, width, height, &|first, part| {
            for (row, line) in part.chunks_exact_mut(width).enumerate() {
                let input = &source[(first + row) * width..(first + row + 1) * width];
                box_line(input, line, radius, span, |slice, i| slice[i]);
            }
        });
    } else {
        // Columns: done as rows of a transposed walk, one column at a time
        // per thread share.
        let columns: Vec<usize> = (0..width).collect();
        let parts = threads().min(width.div_ceil(32)).max(1);
        let each = width.div_ceil(parts);
        let results: Vec<Vec<(usize, Vec<f32>)>> = std::thread::scope(|scope| {
            let handles: Vec<_> = columns
                .chunks(each)
                .map(|chunk| {
                    scope.spawn(move || {
                        chunk
                            .iter()
                            .map(|x| {
                                let column: Vec<f32> =
                                    (0..height).map(|y| source[y * width + x]).collect();
                                let mut out = vec![0.0; height];
                                box_line(&column, &mut out, radius, span, |slice, i| slice[i]);
                                (*x, out)
                            })
                            .collect()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap_or_default())
                .collect()
        });
        for (x, column) in results.into_iter().flatten() {
            for (y, value) in column.into_iter().enumerate() {
                target[y * width + x] = value;
            }
        }
    }
}

fn box_line(
    input: &[f32],
    out: &mut [f32],
    radius: usize,
    span: f32,
    get: impl Fn(&[f32], usize) -> f32,
) {
    let length = input.len();
    if length == 0 {
        return;
    }
    let mut sum = 0.0_f32;
    for offset in 0..=radius * 2 {
        sum += get(input, offset.saturating_sub(radius).min(length - 1));
    }
    for at in 0..length {
        out[at] = sum / span;
        let leaving = at.saturating_sub(radius);
        let entering = (at + radius + 1).min(length - 1);
        sum += get(input, entering) - get(input, leaving);
    }
}

fn strips_f32(
    values: &mut [f32],
    width: usize,
    height: usize,
    work: &(dyn Fn(usize, &mut [f32]) + Sync),
) {
    let parts = threads().min(height.div_ceil(16)).max(1);
    let each = height.div_ceil(parts);
    std::thread::scope(|scope| {
        for (part, chunk) in values.chunks_mut(each * width.max(1)).enumerate() {
            scope.spawn(move || work(part * each, chunk));
        }
    });
}

/// The exact distance (in pixels) from every pixel to the nearest pixel
/// where `inside` is true (0 there), by Felzenszwalb and Huttenlocher's
/// two pass squared distance transform.
#[must_use]
pub fn distance(inside: &[bool], width: usize, height: usize) -> Vec<f32> {
    const FAR: f32 = 1e12;
    let mut grid: Vec<f32> = inside
        .iter()
        .map(|on| if *on { 0.0 } else { FAR })
        .collect();
    let mut line = Vec::new();
    let mut out = Vec::new();
    // Columns, then rows.
    for x in 0..width {
        line.clear();
        line.extend((0..height).map(|y| grid[y * width + x]));
        distance_1d(&line, &mut out);
        for y in 0..height {
            grid[y * width + x] = out[y];
        }
    }
    for y in 0..height {
        line.clear();
        line.extend_from_slice(&grid[y * width..(y + 1) * width]);
        distance_1d(&line, &mut out);
        grid[y * width..(y + 1) * width].copy_from_slice(&out);
    }
    grid.iter().map(|squared| squared.sqrt()).collect()
}

fn distance_1d(f: &[f32], d: &mut Vec<f32>) {
    let n = f.len();
    d.clear();
    d.resize(n, 0.0);
    if n == 0 {
        return;
    }
    let mut v = vec![0_usize; n];
    let mut z = vec![0.0_f32; n + 1];
    let mut k = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    for q in 1..n {
        loop {
            let p = v[k];
            let s = ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * (q - p) as f32);
            if s <= z[k] && k > 0 {
                k -= 1;
                continue;
            }
            if s <= z[k] {
                // k == 0 and the new parabola wins everywhere.
                v[0] = q;
                z[0] = f32::NEG_INFINITY;
                z[1] = f32::INFINITY;
                break;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f32::INFINITY;
            break;
        }
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate() {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let p = v[k];
        let delta = q as f32 - p as f32;
        *out = delta * delta + f[p];
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        return if x >= edge0 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn luma(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

/// A small fast hash of a cell and a seed, 0 to 1.
fn hash(x: i64, y: i64, seed: u64) -> f32 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ seed.wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    (h >> 40) as f32 / (1_u64 << 24) as f32
}

/// Runs effect `kind` with `params` over `image`. Kinds this build does
/// not know, and the track matte (the compositor does it), leave it.
pub fn apply(kind: &str, params: &Params, image: &mut Image, ctx: &Ctx) {
    let px = |key: &str| params.f(key) * ctx.scale;
    match kind {
        "blur.gaussian" => blur(image, px("radius")),
        "blur.directional" => {
            let length = px("length");
            if length >= 1.0 {
                run_filter(
                    &Filter::MotionBlur {
                        angle: params.f("angle"),
                        distance: length,
                    },
                    image,
                );
            }
        }
        "blur.sharpen" => run_filter(
            &Filter::Unsharp {
                radius: px("radius").max(0.5),
                amount: params.f("amount") / 100.0,
                threshold: 0.0,
            },
            image,
        ),
        "color.brightness_contrast" => run_filter(
            &Filter::BrightnessContrast {
                brightness: params.f("brightness") / 100.0,
                contrast: params.f("contrast") / 100.0,
            },
            image,
        ),
        "color.hue_saturation" => run_filter(
            &Filter::HueSaturation {
                hue: params.f("hue"),
                saturation: params.f("saturation") / 100.0,
                luminosity: params.f("lightness") / 100.0,
            },
            image,
        ),
        "color.levels" => run_filter(
            &Filter::Levels {
                black: params.f("in_black") / 255.0,
                white: params.f("in_white") / 255.0,
                gamma: params.f("gamma").max(0.1),
                out_black: params.f("out_black") / 255.0,
                out_white: params.f("out_white") / 255.0,
            },
            image,
        ),
        "color.curves" => {
            let response = |key: &str| sue_model::Response::new(&params.points(key));
            run_filter(
                &Filter::ToneCurve {
                    rgb: response("rgb"),
                    red: response("red"),
                    green: response("green"),
                    blue: response("blue"),
                    channel: 0,
                },
                image,
            );
        }
        "color.balance" => {
            let shift = |range: &str| {
                ["red", "green", "blue"]
                    .map(|channel| params.f(&format!("{range}_{channel}")) / 100.0)
            };
            run_filter(
                &Filter::ColorBalance {
                    shifts: [shift("shadow"), shift("mid"), shift("high")],
                    range: 0,
                },
                image,
            );
        }
        "stylize.glow" => glow(image, params, ctx),
        "stylize.drop_shadow" => {
            let (sin, cos) = params.f("angle").to_radians().sin_cos();
            let distance = px("distance");
            run_filter(
                &Filter::DropShadow {
                    dx: cos * distance,
                    dy: sin * distance,
                    blur: px("softness"),
                    opacity: params.f("opacity") / 100.0,
                    rgb: params.rgb("color"),
                },
                image,
            );
        }
        "stylize.stroke" => stroke(
            image,
            px("width"),
            params.f("opacity") / 100.0,
            params.rgb("color"),
        ),
        "stylize.vignette" => vignette(image, params),
        "stylize.grain" => grain(image, params, ctx),
        "stylize.mosaic" => mosaic(image, px("size").round().max(1.0) as usize),
        "key.chroma" => chroma_key(image, params),
        "key.luma" => luma_key(image, params),
        "transform.crop" => crop(image, params, ctx),
        "transform.mirror" => mirror(image, params.pick("mode")),
        _ => {}
    }
}

/// A glow from the bright parts: what is brighter than a threshold,
/// tinted, blurred and screened back over the picture.
fn glow(image: &mut Image, params: &Params, ctx: &Ctx) {
    let threshold = (params.f("threshold") / 100.0).clamp(0.0, 0.99);
    let strength = params.f("strength") / 100.0;
    let tint = params
        .rgb("color")
        .map(|channel| f32::from(channel) / 255.0);
    let mut bright = Image::new(image.width, image.height, image.rgba.clone());
    for pixel in bright.rgba.as_chunks_mut::<4>().0 {
        let rgb = [pixel[0], pixel[1], pixel[2]].map(|c| f32::from(c) / 255.0);
        let alpha = f32::from(pixel[3]) / 255.0;
        let over = ((luma(rgb) - threshold) / (1.0 - threshold)).max(0.0) * alpha;
        for channel in 0..3 {
            pixel[channel] = (rgb[channel] * tint[channel] * over * 255.0).round() as u8;
        }
        pixel[3] = 255;
    }
    blur(&mut bright, params.f("radius") * ctx.scale);
    let row = image.width as usize * 4;
    let rows = image.height as usize;
    let light = &bright.rgba;
    strips(&mut image.rgba, row, rows, &|first, part| {
        let offset = first * row;
        for (index, pixel) in part.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let at = offset + index * 4;
            let mut most = 0.0_f32;
            for channel in 0..3 {
                let a = f32::from(pixel[channel]) / 255.0;
                let g = (f32::from(light[at + channel]) / 255.0 * strength).min(1.0);
                most = most.max(g);
                pixel[channel] = ((1.0 - (1.0 - a) * (1.0 - g)) * 255.0).round() as u8;
            }
            // Light spreads past the edges of what is drawn.
            let alpha = f32::from(pixel[3]) / 255.0;
            pixel[3] = (alpha.max(most) * 255.0).round() as u8;
        }
    });
}

/// A line of `rgb` `width` pixels wide round what is opaque, under it.
fn stroke(image: &mut Image, width: f32, opacity: f32, rgb: [u8; 3]) {
    if width <= 0.0 || opacity <= 0.0 {
        return;
    }
    let (w, h) = (image.width as usize, image.height as usize);
    let inside: Vec<bool> = image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| p[3] >= 128)
        .collect();
    let far = distance(&inside, w, h);
    for (pixel, d) in image.rgba.as_chunks_mut::<4>().0.iter_mut().zip(far) {
        let cover = (width - d + 0.5).clamp(0.0, 1.0) * opacity;
        if cover <= 0.0 {
            continue;
        }
        // The picture over the stroke.
        let alpha = f32::from(pixel[3]) / 255.0;
        let out = alpha + cover * (1.0 - alpha);
        if out <= 0.0 {
            continue;
        }
        for channel in 0..3 {
            let value = (f32::from(pixel[channel]) * alpha
                + f32::from(rgb[channel]) * cover * (1.0 - alpha))
                / out;
            pixel[channel] = value.round().clamp(0.0, 255.0) as u8;
        }
        pixel[3] = (out * 255.0).round() as u8;
    }
}

/// Darkens (or lightens, below 0) toward the edges.
fn vignette(image: &mut Image, params: &Params) {
    let amount = params.f("amount") / 100.0;
    if amount == 0.0 {
        return;
    }
    let size = (params.f("size") / 100.0).clamp(0.0, 1.0);
    let round = (params.f("roundness") / 100.0).clamp(0.0, 1.0);
    let feather = (params.f("feather") / 100.0).clamp(0.01, 1.0);
    let (w, h) = (image.width as f32, image.height as f32);
    // Round: a circle; else an ellipse the frame's shape.
    let (rx, ry) = {
        let circle = w.min(h) / 2.0;
        (
            circle + (w / 2.0 - circle) * (1.0 - round),
            circle + (h / 2.0 - circle) * (1.0 - round),
        )
    };
    let width = image.width as usize;
    let target = if amount > 0.0 { 0.0 } else { 1.0 };
    let amount = amount.abs();
    strips(
        &mut image.rgba,
        width * 4,
        image.height as usize,
        &|first, part| {
            for (index, pixel) in part.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let (x, y) = (
                    (index % width) as f32 + 0.5,
                    (first + index / width) as f32 + 0.5,
                );
                let d = (((x - w / 2.0) / rx).powi(2) + ((y - h / 2.0) / ry).powi(2)).sqrt();
                let edge = smoothstep(size, size + feather, d) * amount;
                for channel in pixel.iter_mut().take(3) {
                    let value = f32::from(*channel) / 255.0;
                    *channel = ((value + (target - value) * edge) * 255.0).round() as u8;
                }
            }
        },
    );
}

/// Film grain: noise in cells `size` source pixels across, the same
/// pattern at any picture size, moving each frame when animated.
fn grain(image: &mut Image, params: &Params, ctx: &Ctx) {
    let amount = params.f("amount") / 100.0;
    if amount <= 0.0 {
        return;
    }
    let cell = (params.f("size").max(1.0) * ctx.scale).max(0.25);
    let colored = params.on("color");
    let seed = if params.on("animated") {
        ctx.frame as u64
    } else {
        0
    };
    let width = image.width as usize;
    strips(
        &mut image.rgba,
        width * 4,
        image.height as usize,
        &|first, part| {
            for (index, pixel) in part.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let x = ((index % width) as f32 / cell).floor() as i64;
                let y = ((first + index / width) as f32 / cell).floor() as i64;
                let noise = |salt: u64| {
                    (hash(x, y, seed * 4 + salt) + hash(x + 7919, y - 104_729, seed * 4 + salt)
                        - 1.0)
                        * amount
                        * 0.5
                };
                let gray = noise(0);
                for (channel, value) in pixel.iter_mut().take(3).enumerate() {
                    let n = if colored {
                        noise(channel as u64 + 1)
                    } else {
                        gray
                    };
                    *value = (f32::from(*value) + n * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
        },
    );
}

/// Square blocks of their average, aligned to the picture's corner.
fn mosaic(image: &mut Image, size: usize) {
    if size <= 1 {
        return;
    }
    let (w, h) = (image.width as usize, image.height as usize);
    let row = w * 4;
    let bands = h.div_ceil(size);
    let parts = threads().min(bands).max(1);
    let per = bands.div_ceil(parts);
    std::thread::scope(|scope| {
        for (part, chunk) in image.rgba.chunks_mut(per * size * row).enumerate() {
            scope.spawn(move || {
                let rows = chunk.len() / row;
                let _ = part;
                for by in (0..rows).step_by(size) {
                    for bx in (0..w).step_by(size) {
                        let (x1, y1) = ((bx + size).min(w), (by + size).min(rows));
                        let mut sum = [0_u64; 4];
                        for y in by..y1 {
                            for x in bx..x1 {
                                for (channel, total) in sum.iter_mut().enumerate() {
                                    *total += u64::from(chunk[y * row + x * 4 + channel]);
                                }
                            }
                        }
                        let count = ((x1 - bx) * (y1 - by)).max(1) as u64;
                        let mean = sum.map(|total| (total / count) as u8);
                        for y in by..y1 {
                            for x in bx..x1 {
                                chunk[y * row + x * 4..y * row + x * 4 + 4].copy_from_slice(&mean);
                            }
                        }
                    }
                }
            });
        }
    });
}

/// Chroma (blue, red) of a color, from -0.5 to 0.5.
fn chroma(rgb: [f32; 3]) -> [f32; 2] {
    let y = luma(rgb);
    [(rgb[2] - y) / 1.8556, (rgb[0] - y) / 1.5748]
}

/// Keys out the colors near the key color (their chroma within the
/// tolerance), softly, and takes the key color's spill off what stays.
fn chroma_key(image: &mut Image, params: &Params) {
    let key = params.rgb("key_color").map(|c| f32::from(c) / 255.0);
    let key_chroma = chroma(key);
    let reach = key_chroma[0].hypot(key_chroma[1]).max(0.05);
    let tolerance = params.f("tolerance") / 100.0 * reach;
    let softness = params.f("softness") / 100.0 * reach;
    let spill = params.f("spill") / 100.0;
    // The key's strongest channel is the one its spill lifts.
    let dominant = (0..3)
        .max_by(|a, b| key[*a].total_cmp(&key[*b]))
        .unwrap_or(1);
    let width = image.width as usize;
    strips(
        &mut image.rgba,
        width * 4,
        image.height as usize,
        &|_, part| {
            for pixel in part.as_chunks_mut::<4>().0 {
                let mut rgb = [pixel[0], pixel[1], pixel[2]].map(|c| f32::from(c) / 255.0);
                let c = chroma(rgb);
                let distance = (c[0] - key_chroma[0]).hypot(c[1] - key_chroma[1]);
                let keep = smoothstep(tolerance, tolerance + softness.max(1e-4), distance);
                if spill > 0.0 {
                    let others = (0..3)
                        .filter(|channel| *channel != dominant)
                        .map(|channel| rgb[channel])
                        .fold(0.0_f32, f32::max);
                    let excess = (rgb[dominant] - others).max(0.0);
                    rgb[dominant] -= excess * spill;
                }
                for channel in 0..3 {
                    pixel[channel] = (rgb[channel] * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                pixel[3] = (f32::from(pixel[3]) * keep).round() as u8;
            }
        },
    );
}

/// Keys out the dark (or bright) parts by their brightness, softly.
fn luma_key(image: &mut Image, params: &Params) {
    let threshold = params.f("threshold") / 100.0;
    let softness = (params.f("softness") / 100.0).max(1e-4);
    let bright = params.pick("side") == 1;
    let width = image.width as usize;
    strips(
        &mut image.rgba,
        width * 4,
        image.height as usize,
        &|_, part| {
            for pixel in part.as_chunks_mut::<4>().0 {
                let y = luma([pixel[0], pixel[1], pixel[2]].map(|c| f32::from(c) / 255.0));
                let keep = if bright {
                    1.0 - smoothstep(1.0 - threshold - softness, 1.0 - threshold, y)
                } else {
                    smoothstep(threshold, threshold + softness, y)
                };
                pixel[3] = (f32::from(pixel[3]) * keep).round() as u8;
            }
        },
    );
}

/// Clears the edges, each a share of the picture, with a soft edge.
fn crop(image: &mut Image, params: &Params, ctx: &Ctx) {
    let (w, h) = (image.width as f32, image.height as f32);
    let left = params.f("left") / 100.0 * w;
    let right = w - params.f("right") / 100.0 * w;
    let top = params.f("top") / 100.0 * h;
    let bottom = h - params.f("bottom") / 100.0 * h;
    let feather = (params.f("feather") * ctx.scale).max(0.0);
    let width = image.width as usize;
    let ramp = |inside: f32| {
        if feather <= 0.0 {
            (inside + 0.5).clamp(0.0, 1.0)
        } else {
            (inside / feather).clamp(0.0, 1.0)
        }
    };
    strips(
        &mut image.rgba,
        width * 4,
        image.height as usize,
        &|first, part| {
            for (index, pixel) in part.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let (x, y) = (
                    (index % width) as f32 + 0.5,
                    (first + index / width) as f32 + 0.5,
                );
                let inside = (x - left).min(right - x).min(y - top).min(bottom - y);
                pixel[3] = (f32::from(pixel[3]) * ramp(inside)).round() as u8;
            }
        },
    );
}

/// Mirrors one half onto the other, or flips the whole picture.
fn mirror(image: &mut Image, mode: usize) {
    let (w, h) = (image.width as usize, image.height as usize);
    let source = image.rgba.clone();
    let get = |x: usize, y: usize| {
        let at = (y * w + x) * 4;
        [source[at], source[at + 1], source[at + 2], source[at + 3]]
    };
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = match MIRROR_MODES.get(mode).copied().unwrap_or("left") {
                "left" if x >= w / 2 => (w - 1 - x, y),
                "right" if x < w / 2 => (w - 1 - x, y),
                "top" if y >= h / 2 => (x, h - 1 - y),
                "bottom" if y < h / 2 => (x, h - 1 - y),
                "flip-x" => (w - 1 - x, y),
                "flip-y" => (x, h - 1 - y),
                _ => (x, y),
            };
            let at = (y * w + x) * 4;
            image.rgba[at..at + 4].copy_from_slice(&get(sx, sy));
        }
    }
}

const MIRROR_MODES: &[&str] = super::MIRROR;

/// A test card for the effects' tests.
#[cfg(test)]
pub(crate) fn tests_card() -> Image {
    tests::card(80, 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test card: a gradient with a bright square and a soft edge.
    pub(crate) fn card(width: u32, height: u32) -> Image {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let bright = (width / 3..2 * width / 3).contains(&x)
                    && (height / 3..2 * height / 3).contains(&y);
                let r = (x * 255 / width.max(1)) as u8;
                let g = (y * 255 / height.max(1)) as u8;
                let b = if bright {
                    250
                } else {
                    ((x + y) % 64) as u8 * 3
                };
                rgba.extend_from_slice(&[r, g, b, if x < 3 { 128 } else { 255 }]);
            }
        }
        Image::new(width, height, rgba)
    }

    #[test]
    fn filters_in_strips_match_one_run_over_the_whole_picture() {
        let picture = card(97, 211);
        for filter in [
            Filter::GaussianBlur { radius: 6.0 },
            Filter::GaussianBlur { radius: 0.8 },
            Filter::MotionBlur {
                angle: 60.0,
                distance: 15.0,
            },
            Filter::Unsharp {
                radius: 2.0,
                amount: 1.0,
                threshold: 0.0,
            },
            Filter::Levels {
                black: 0.1,
                white: 0.9,
                gamma: 1.4,
                out_black: 0.0,
                out_white: 1.0,
            },
            Filter::DropShadow {
                dx: 4.0,
                dy: -6.0,
                blur: 3.0,
                opacity: 0.7,
                rgb: [0, 0, 0],
            },
        ] {
            let mut whole = picture.clone();
            filter.apply(&mut whole.rgba, whole.width, whole.height, None, [0, 0]);
            let mut parts = picture.clone();
            run_filter(&filter, &mut parts);
            let worst = parts
                .rgba
                .iter()
                .zip(&whole.rgba)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0);
            assert!(worst <= 1, "{filter:?}: {worst}");
        }
    }

    #[test]
    fn the_distance_transform_is_exact() {
        let (w, h) = (9, 7);
        let mut inside = vec![false; w * h];
        inside[3 * w + 4] = true;
        let d = distance(&inside, w, h);
        assert_eq!(d[3 * w + 4], 0.0);
        assert!((d[3 * w + 7] - 3.0).abs() < 1e-5);
        assert!((d[0] - (16.0_f32 + 9.0).sqrt()).abs() < 1e-5);
    }

    #[test]
    fn the_plane_blur_keeps_flat_areas_and_spreads_an_edge() {
        let (w, h) = (40, 30);
        let plane: Vec<f32> = (0..w * h)
            .map(|i| if i % w < 20 { 1.0 } else { 0.0 })
            .collect();
        let soft = blur_plane(&plane, w, h, 4.0);
        assert!((soft[15 * w + 2] - 1.0).abs() < 1e-5);
        assert!(soft[15 * w + 37] < 1e-5);
        assert!((soft[15 * w + 20] - 0.5).abs() < 0.2);
    }
}
