//! Layered Photoshop documents: `.psd` (version 1) and `.psb` (version 2,
//! "large document"), RGB with transparency, written and read with their
//! layers, folders, blend modes, opacity, visibility, layer masks and
//! resolution.
//!
//! Channel data is written PackBits compressed (RLE), as Photoshop writes
//! it, 8 bits a channel. Reading takes 8 and 16 bits a channel (16-bit
//! channels come in rounded to 8 bits), raw, RLE and zip channels with or
//! without prediction; other depths and color modes are refused with a
//! reason.

/// What a layer record is: pixels, or one end of a folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PsdKind {
    /// A layer of pixels.
    Pixels,
    /// The start (top) of a folder: the record with its name and style.
    FolderOpen {
        /// Shown open in the Layers panel.
        open: bool,
    },
    /// The end (bottom) of a folder, written before its layers.
    FolderEnd,
}

/// One layer record of a Photoshop document.
#[derive(Clone, Debug, PartialEq)]
pub struct PsdLayer {
    /// Its name.
    pub name: String,
    /// Pixels, or a folder end.
    pub kind: PsdKind,
    /// Where its pixels start in the document.
    pub left: i32,
    /// Where its pixels start in the document.
    pub top: i32,
    /// Its pixels' width.
    pub width: u32,
    /// Its pixels' height.
    pub height: u32,
    /// Straight RGBA8, `width * height * 4` bytes (empty for folders).
    pub pixels: Vec<u8>,
    /// 0 to 255.
    pub opacity: u8,
    /// Drawn at all.
    pub visible: bool,
    /// Clipped to the layer below.
    pub clip: bool,
    /// The blend mode key, such as `norm` or `mul ` (see [`blend_key`]).
    pub blend: [u8; 4],
    /// Its layer mask.
    pub mask: Option<PsdMask>,
}

/// A layer mask: one channel, 255 shows and 0 hides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PsdMask {
    /// Where its pixels start in the document.
    pub left: i32,
    /// Where its pixels start in the document.
    pub top: i32,
    /// Its pixels' width.
    pub width: u32,
    /// Its pixels' height.
    pub height: u32,
    /// `width * height` bytes.
    pub pixels: Vec<u8>,
    /// What the mask is outside its pixels: 0 or 255.
    pub outside: u8,
    /// It is on.
    pub enabled: bool,
    /// It moves with the layer.
    pub linked: bool,
}

impl PsdLayer {
    /// A visible, opaque, normal layer of `pixels`.
    #[must_use]
    pub fn pixels(
        name: &str,
        left: i32,
        top: i32,
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            kind: PsdKind::Pixels,
            left,
            top,
            width,
            height,
            pixels,
            opacity: 255,
            visible: true,
            clip: false,
            blend: *b"norm",
            mask: None,
        }
    }

    /// One end of a folder.
    #[must_use]
    pub fn folder(name: &str, kind: PsdKind) -> Self {
        Self {
            kind,
            blend: if kind == PsdKind::FolderEnd {
                *b"norm"
            } else {
                *b"pass"
            },
            ..Self::pixels(name, 0, 0, 0, 0, Vec::new())
        }
    }
}

/// A Photoshop document: its size, layers (bottom first, folders as
/// [`PsdKind::FolderEnd`] then their layers then [`PsdKind::FolderOpen`])
/// and the merged picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Psd {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The layers, bottom first.
    pub layers: Vec<PsdLayer>,
    /// The whole picture merged, straight RGBA8.
    pub merged: Vec<u8>,
}

/// What a Photoshop document says about itself beyond its layers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PsdInfo {
    /// Bits a channel: 8 or 16 are read.
    pub depth: u16,
    /// Pixels per inch across, when the document says.
    pub dpi: Option<f64>,
}

/// The depth and resolution of a Photoshop document, from its header and
/// image resources alone.
///
/// # Errors
/// If the bytes are not a Photoshop document or end early.
pub fn psd_info(bytes: &[u8]) -> Result<PsdInfo, String> {
    let mut r = Reader {
        bytes,
        at: 0,
        big: false,
    };
    if r.take(4)? != b"8BPS" {
        return Err("not a Photoshop document".into());
    }
    r.take(2 + 6 + 2 + 4 + 4)?;
    let depth = r.u16()?;
    r.u16()?;
    let skip = r.u32()? as usize;
    r.take(skip)?;
    let resources = r.u32()? as usize;
    let dpi = resolution(r.take(resources)?);
    Ok(PsdInfo { depth, dpi })
}

/// The horizontal resolution in pixels per inch from the image resources
/// (`ResolutionInfo`, id 1005), when they hold it.
fn resolution(mut data: &[u8]) -> Option<f64> {
    while data.len() >= 12 && &data[..4] == b"8BIM" {
        let id = u16::from_be_bytes([data[4], data[5]]);
        let name = usize::from(data[6]);
        let name_size = (name + 1).next_multiple_of(2);
        let at = 6 + name_size;
        let size = u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?) as usize;
        let body = data.get(at + 4..at + 4 + size)?;
        if id == 1005 && size >= 16 {
            let fixed = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
            let unit = u16::from_be_bytes([body[4], body[5]]);
            let value = f64::from(fixed) / 65536.0;
            // Unit 2 is pixels per centimeter.
            let dpi = if unit == 2 { value * 2.54 } else { value };
            return (dpi > 0.0 && dpi.is_finite()).then_some(dpi);
        }
        data = data.get(at + 4 + size.next_multiple_of(2)..)?;
    }
    None
}

/// The image resource block that says the resolution is `dpi`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn resolution_block(dpi: f64) -> Vec<u8> {
    let fixed = (dpi.clamp(1.0, 65_535.0) * 65536.0).round() as u32;
    let mut out = Vec::with_capacity(28);
    out.extend_from_slice(b"8BIM");
    out.extend_from_slice(&1005_u16.to_be_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(&16_u32.to_be_bytes());
    for _ in 0..2 {
        out.extend_from_slice(&fixed.to_be_bytes());
        // Pixels per inch, shown in inches.
        out.extend_from_slice(&1_u16.to_be_bytes());
        out.extend_from_slice(&1_u16.to_be_bytes());
    }
    out
}

/// The largest side of a `.psd`; `.psb` goes to 300,000.
pub const PSD_MAX_SIDE: u32 = 30_000;
/// The largest side of a `.psb`.
pub const PSB_MAX_SIDE: u32 = 300_000;

/// The Photoshop blend key for a blend mode name as `sue_model` spells it
/// (`Normal`, `Multiply`, `ColorDodge` and so on); `norm` for unknown names.
#[must_use]
pub fn blend_key(name: &str) -> [u8; 4] {
    let key: &[u8; 4] = match name {
        "Darken" => b"dark",
        "Multiply" => b"mul ",
        "ColorBurn" => b"idiv",
        "LinearBurn" => b"lbrn",
        "Subtract" => b"fsub",
        "DarkerColor" => b"dkCl",
        "Lighten" => b"lite",
        "Screen" => b"scrn",
        // Photoshop has no glow modes: Clip Studio's Glow Dodge and Add
        // (Glow) go out as the dodge and add they glow beside.
        "ColorDodge" | "GlowDodge" => b"div ",
        "Add" | "AddGlow" => b"lddg",
        "LighterColor" => b"lgCl",
        "Overlay" => b"over",
        "SoftLight" => b"sLit",
        "HardLight" => b"hLit",
        "VividLight" => b"vLit",
        "LinearLight" => b"lLit",
        "PinLight" => b"pLit",
        "HardMix" => b"hMix",
        "Difference" => b"diff",
        "Exclusion" => b"smud",
        "Divide" => b"fdiv",
        "Hue" => b"hue ",
        "Saturation" => b"sat ",
        "Color" => b"colr",
        "Luminosity" => b"lum ",
        "PassThrough" => b"pass",
        _ => b"norm",
    };
    *key
}

/// The blend mode name for a Photoshop blend key; `Normal` for keys Sue
/// has no mode for (Dissolve among them).
#[must_use]
pub fn blend_name(key: [u8; 4]) -> &'static str {
    match &key {
        b"dark" => "Darken",
        b"mul " => "Multiply",
        b"idiv" => "ColorBurn",
        b"lbrn" => "LinearBurn",
        b"fsub" => "Subtract",
        b"dkCl" => "DarkerColor",
        b"lite" => "Lighten",
        b"scrn" => "Screen",
        b"div " => "ColorDodge",
        b"lddg" => "Add",
        b"lgCl" => "LighterColor",
        b"over" => "Overlay",
        b"sLit" => "SoftLight",
        b"hLit" => "HardLight",
        b"vLit" => "VividLight",
        b"lLit" => "LinearLight",
        b"pLit" => "PinLight",
        b"hMix" => "HardMix",
        b"diff" => "Difference",
        b"smud" => "Exclusion",
        b"fdiv" => "Divide",
        b"hue " => "Hue",
        b"sat " => "Saturation",
        b"colr" => "Color",
        b"lum " => "Luminosity",
        b"pass" => "PassThrough",
        _ => "Normal",
    }
}

/// Writes `psd` as a `.psd`, or a `.psb` when `big`.
///
/// # Errors
/// If the document is larger than the version allows, or a layer's pixels
/// do not fit its size.
pub fn encode_psd(psd: &Psd, big: bool) -> Result<Vec<u8>, String> {
    encode_psd_at(psd, big, None)
}

/// Writes `psd` as a `.psd`, or a `.psb` when `big`, saying its
/// resolution is `dpi` when given.
///
/// # Errors
/// If the document is larger than the version allows, or a layer's or
/// mask's pixels do not fit its size.
#[expect(
    clippy::too_many_lines,
    reason = "one pass from top to bottom; split up, its steps would be read out of order"
)]
pub fn encode_psd_at(psd: &Psd, big: bool, dpi: Option<f64>) -> Result<Vec<u8>, String> {
    let max = if big { PSB_MAX_SIDE } else { PSD_MAX_SIDE };
    if psd.width == 0 || psd.height == 0 || psd.width.max(psd.height) > max {
        return Err(format!(
            "A .{} is 1 to {max} pixels a side",
            if big { "psb" } else { "psd" }
        ));
    }
    if psd.merged.len() != (psd.width * psd.height * 4) as usize {
        return Err("the merged picture does not fit the size".into());
    }
    let mut out = Writer {
        bytes: Vec::new(),
        big,
    };
    out.bytes.extend_from_slice(b"8BPS");
    out.u16(if big { 2 } else { 1 });
    out.bytes.extend_from_slice(&[0; 6]);
    out.u16(4);
    out.u32(psd.height);
    out.u32(psd.width);
    out.u16(8);
    out.u16(3);
    // No color mode data; the resolution when known.
    out.u32(0);
    let resources = dpi.map(resolution_block).unwrap_or_default();
    out.u32(resources.len() as u32);
    out.bytes.extend_from_slice(&resources);

    // Layer and mask information.
    let section = out.reserve_long();
    let info = out.reserve_long();
    let count = i16::try_from(psd.layers.len()).map_err(|_| "too many layers")?;
    // Negative: the merged picture's first extra channel is its alpha.
    out.u16((-count) as u16);
    let mut channels = Vec::with_capacity(psd.layers.len());
    for layer in &psd.layers {
        let planes = layer_planes(layer)?;
        let mut encoded: Vec<Vec<u8>> = planes
            .iter()
            .map(|plane| rle_channel(plane, layer.width, layer.height, big))
            .collect();
        let mut ids = vec![-1_i16, 0, 1, 2];
        if let Some(mask) = &layer.mask {
            if mask.pixels.len() != mask.width as usize * mask.height as usize {
                return Err(format!("the mask of {} does not fit its size", layer.name));
            }
            encoded.push(rle_channel(&mask.pixels, mask.width, mask.height, big));
            ids.push(-2);
        }
        out.i32(layer.top);
        out.i32(layer.left);
        out.i32(layer.top + layer.height as i32);
        out.i32(layer.left + layer.width as i32);
        out.u16(ids.len() as u16);
        for (id, data) in ids.iter().zip(&encoded) {
            out.u16(*id as u16);
            out.long(data.len() as u64);
        }
        out.bytes.extend_from_slice(b"8BIM");
        out.bytes.extend_from_slice(&layer.blend);
        out.bytes.push(layer.opacity);
        out.bytes.push(u8::from(layer.clip));
        // Bit 1 hides; bit 3 says bit 4 is meaningful (as Photoshop writes).
        out.bytes.push(if layer.visible { 0x08 } else { 0x0A });
        out.bytes.push(0);
        let extra = out.reserve_u32();
        match &layer.mask {
            Some(mask) => {
                out.u32(20);
                out.i32(mask.top);
                out.i32(mask.left);
                out.i32(mask.top + mask.height as i32);
                out.i32(mask.left + mask.width as i32);
                out.bytes.push(mask.outside);
                // Bit 1 turns the mask off; bit 0 says its place does not
                // follow the layer (unlinked).
                out.bytes
                    .push(u8::from(!mask.linked) | u8::from(!mask.enabled) << 1);
                out.bytes.extend_from_slice(&[0, 0]);
            }
            None => out.u32(0),
        }
        out.u32(0); // No blending ranges.
        pascal_name(&mut out.bytes, &layer.name);
        unicode_name(&mut out.bytes, &layer.name);
        if layer.kind != PsdKind::Pixels {
            let divider = match layer.kind {
                PsdKind::FolderOpen { open: true } => 1,
                PsdKind::FolderOpen { open: false } => 2,
                PsdKind::FolderEnd | PsdKind::Pixels => 3,
            };
            out.bytes.extend_from_slice(b"8BIMlsct");
            out.u32(12);
            out.u32(divider);
            out.bytes.extend_from_slice(b"8BIM");
            out.bytes.extend_from_slice(&layer.blend);
        }
        out.fill_u32(extra);
        channels.push(encoded);
    }
    for encoded in channels {
        for data in encoded {
            out.bytes.extend_from_slice(&data);
        }
    }
    // The layer info's length is even.
    if (out.bytes.len() - info - if big { 8 } else { 4 }) % 2 == 1 {
        out.bytes.push(0);
    }
    out.fill_long(info);
    out.u32(0); // No global layer mask.
    out.fill_long(section);

    // The merged picture, RLE, planar: red, green, blue, alpha.
    let planes = planar(&psd.merged, psd.width, psd.height);
    out.u16(1);
    let mut counts = Vec::new();
    let mut rows = Vec::new();
    for plane in &planes {
        for row in plane.chunks(psd.width as usize) {
            let packed = packbits(row);
            counts.push(packed.len());
            rows.push(packed);
        }
    }
    for count in counts {
        if big {
            out.u32(count as u32);
        } else {
            out.u16(count as u16);
        }
    }
    for row in rows {
        out.bytes.extend_from_slice(&row);
    }
    Ok(out.bytes)
}

/// Reads a `.psd` or `.psb`; 16-bit channels come in rounded to 8 bits.
///
/// # Errors
/// If the bytes are not a Photoshop document Sue can read: 8 or 16-bit
/// RGB with raw, RLE or zip channels.
pub fn decode_psd(bytes: &[u8]) -> Result<Psd, String> {
    let mut r = Reader {
        bytes,
        at: 0,
        big: false,
    };
    if r.take(4)? != b"8BPS" {
        return Err("not a Photoshop document".into());
    }
    r.big = match r.u16()? {
        1 => false,
        2 => true,
        other => return Err(format!("Photoshop version {other} is not known")),
    };
    r.take(6)?;
    let channels = r.u16()?;
    let height = r.u32()?;
    let width = r.u32()?;
    let depth = r.u16()?;
    let mode = r.u16()?;
    if depth != 8 && depth != 16 {
        return Err(format!("{depth}-bit Photoshop documents are not read yet"));
    }
    if mode != 3 {
        return Err("only RGB Photoshop documents are read".into());
    }
    if width == 0 || height == 0 || width > PSB_MAX_SIDE || height > PSB_MAX_SIDE {
        return Err("the document has no size".into());
    }
    let skip = r.u32()? as usize;
    r.take(skip)?;
    let skip = r.u32()? as usize;
    r.take(skip)?;
    let section = r.long()? as usize;
    let section_end = r.at + section;
    let mut layers = Vec::new();
    if section > 0 {
        let info = r.long()? as usize;
        let info_end = r.at + info;
        if info > 0 {
            layers = read_layers(&mut r, depth)?;
        }
        r.at = info_end;
        if layers.is_empty() && depth == 16 {
            layers = tagged_layers(&mut r, section_end, depth)?;
        }
    }
    r.at = section_end;
    let merged = read_merged(&mut r, width, height, channels, depth)?;
    Ok(Psd {
        width,
        height,
        layers,
        merged,
    })
}

/// The layers of a 16-bit document as Photoshop writes them: its layer
/// info is empty and the layers are in an `Lr16` block among the tagged
/// blocks after the global mask info, which end at `end`.
fn tagged_layers(r: &mut Reader<'_>, end: usize, depth: u16) -> Result<Vec<PsdLayer>, String> {
    // Blocks whose length is 8 bytes in a `.psb`.
    const LONG: [&[u8; 4]; 13] = [
        b"LMsk", b"Lr16", b"Lr32", b"Layr", b"Mt16", b"Mt32", b"Mtrn", b"Alph", b"FMsk", b"lnk2",
        b"FEid", b"FXid", b"PxSD",
    ];
    if r.at.saturating_add(4) > end {
        return Ok(Vec::new());
    }
    let global = r.u32()? as usize;
    r.take(global)?;
    loop {
        // Some writers pad a block to a multiple of four.
        while r.at < end && r.bytes.get(r.at) == Some(&0) {
            r.at += 1;
        }
        if r.at.saturating_add(12) > end {
            return Ok(Vec::new());
        }
        let signature = r.take(4)?;
        if signature != b"8BIM" && signature != b"8B64" {
            return Ok(Vec::new());
        }
        let key = r.take(4)?;
        let size = if r.big && LONG.iter().any(|long| long.as_slice() == key) {
            usize::try_from(r.u64()?).map_err(|_| "the document is damaged")?
        } else {
            r.u32()? as usize
        };
        let start = r.at;
        let block_end = start
            .checked_add(size)
            .filter(|block_end| *block_end <= end)
            .ok_or("the document is cut short")?;
        if key == b"Lr16" {
            return if size == 0 {
                Ok(Vec::new())
            } else {
                read_layers(r, depth)
            };
        }
        r.at = block_end;
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one pass from top to bottom; split up, its steps would be read out of order"
)]
fn read_layers(r: &mut Reader<'_>, depth: u16) -> Result<Vec<PsdLayer>, String> {
    let count = (r.u16()? as i16).unsigned_abs() as usize;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let top = r.i32()?;
        let left = r.i32()?;
        let bottom = r.i32()?;
        let right = r.i32()?;
        let channel_count = r.u16()? as usize;
        let mut channels = Vec::with_capacity(channel_count);
        for _ in 0..channel_count {
            let id = r.u16()? as i16;
            let length = r.long()? as usize;
            channels.push((id, length));
        }
        if r.take(4)? != b"8BIM" {
            return Err("a layer record is damaged".into());
        }
        let mut blend = [0; 4];
        blend.copy_from_slice(r.take(4)?);
        let opacity = r.u8()?;
        let clip = r.u8()? != 0;
        let flags = r.u8()?;
        r.u8()?;
        let extra = r.u32()? as usize;
        let extra_end = r.at + extra;
        let mask_size = r.u32()? as usize;
        let mask_data = r.take(mask_size)?;
        let mask = (mask_size >= 18).then(|| {
            let int = |at: usize| {
                i32::from_be_bytes([
                    mask_data[at],
                    mask_data[at + 1],
                    mask_data[at + 2],
                    mask_data[at + 3],
                ])
            };
            let (top, left, bottom, right) = (int(0), int(4), int(8), int(12));
            let flags = mask_data[17];
            PsdMask {
                left,
                top,
                width: u32::try_from(i64::from(right) - i64::from(left)).unwrap_or(0),
                height: u32::try_from(i64::from(bottom) - i64::from(top)).unwrap_or(0),
                pixels: Vec::new(),
                outside: if mask_data[16] >= 128 { 255 } else { 0 },
                enabled: flags & 2 == 0,
                linked: flags & 1 == 0,
            }
        });
        // A mask larger than any document is damage, not a mask.
        let mask = mask.filter(|mask| mask.width <= PSB_MAX_SIDE && mask.height <= PSB_MAX_SIDE);
        let ranges = r.u32()? as usize;
        r.take(ranges)?;
        let length = r.u8()? as usize;
        let mut name = String::from_utf8_lossy(r.take(length)?).into_owned();
        let padded = (length + 1).div_ceil(4) * 4;
        r.take(padded - 1 - length)?;
        let mut kind = PsdKind::Pixels;
        while r.at + 12 <= extra_end {
            let signature = r.take(4)?;
            if signature != b"8BIM" && signature != b"8B64" {
                break;
            }
            let mut key = [0; 4];
            key.copy_from_slice(r.take(4)?);
            let long_keys: [&[u8; 4]; 13] = [
                b"LMsk", b"Lr16", b"Lr32", b"Layr", b"Mt16", b"Mt32", b"Mtrn", b"Alph", b"FMsk",
                b"lnk2", b"FEid", b"FXid", b"PxSD",
            ];
            let size = if r.big && long_keys.contains(&&key) {
                r.u64()? as usize
            } else {
                r.u32()? as usize
            };
            let data = r.take(size)?;
            match &key {
                b"luni" if data.len() >= 4 => {
                    let chars = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
                    let units: Vec<u16> = data[4..]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .take(chars)
                        .map(|pair| u16::from_be_bytes(*pair))
                        .collect();
                    name = String::from_utf16_lossy(&units)
                        .trim_end_matches('\0')
                        .to_owned();
                }
                b"lsct" | b"lsdk" if data.len() >= 4 => {
                    kind = match u32::from_be_bytes([data[0], data[1], data[2], data[3]]) {
                        1 => PsdKind::FolderOpen { open: true },
                        2 => PsdKind::FolderOpen { open: false },
                        3 => PsdKind::FolderEnd,
                        _ => kind,
                    };
                    if data.len() >= 12 && &data[4..8] == b"8BIM" {
                        blend.copy_from_slice(&data[8..12]);
                    }
                }
                _ => {}
            }
        }
        r.at = extra_end;
        let width = u32::try_from(right - left).unwrap_or(0);
        let height = u32::try_from(bottom - top).unwrap_or(0);
        records.push((
            PsdLayer {
                name,
                kind,
                left,
                top,
                width,
                height,
                pixels: Vec::new(),
                opacity,
                visible: flags & 0x02 == 0,
                clip,
                blend,
                mask,
            },
            channels,
        ));
    }
    let mut layers = Vec::with_capacity(count);
    for (mut layer, channels) in records {
        let area = layer.width as usize * layer.height as usize;
        let mut pixels = vec![0_u8; area * 4];
        if area > 0 {
            // Opaque unless an alpha channel says otherwise.
            for pixel in pixels.chunks_mut(4) {
                pixel[3] = 255;
            }
        }
        let mut mask_plane = None;
        for (id, length) in channels {
            let start = r.at;
            let data = r.take(length)?;
            r.at = start + length;
            if id == -2 {
                mask_plane = Some(data);
                continue;
            }
            let slot = match id {
                0..=2 => Some(id as usize),
                -1 => Some(3),
                _ => None,
            };
            let Some(slot) = slot else {
                continue;
            };
            if area == 0 || data.len() < 2 {
                continue;
            }
            let plane = channel_plane(data, layer.width, layer.height, r.big, depth)?;
            for (index, value) in plane.into_iter().enumerate().take(area) {
                pixels[index * 4 + slot] = value;
            }
        }
        layer.pixels = pixels;
        if let Some(mask) = &mut layer.mask {
            let area = mask.width as usize * mask.height as usize;
            mask.pixels = match mask_plane {
                Some(data) if area > 0 && data.len() >= 2 => {
                    let mut plane = channel_plane(data, mask.width, mask.height, r.big, depth)?;
                    plane.resize(area, mask.outside);
                    plane
                }
                _ => vec![mask.outside; area],
            };
        }
        layers.push(layer);
    }
    Ok(layers)
}

fn read_merged(
    r: &mut Reader<'_>,
    width: u32,
    height: u32,
    channels: u16,
    depth: u16,
) -> Result<Vec<u8>, String> {
    let area = width as usize * height as usize;
    let mut merged = vec![255_u8; area * 4];
    let compression = r.u16()?;
    let used = usize::from(channels.min(4));
    let bytes = usize::from(depth / 8);
    let row = width as usize * bytes;
    let planes: Vec<Vec<u8>> = match compression {
        0 => (0..used)
            .map(|_| r.take(area * bytes).map(|raw| eight_bits(raw, depth)))
            .collect::<Result<_, _>>()?,
        2 | 3 => {
            let packed = &r.bytes[r.at..];
            let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(
                packed,
                area * bytes * usize::from(channels),
            )
            .map_err(|_| "the merged picture does not inflate")?;
            (0..used)
                .map(|channel| {
                    let mut plane = raw
                        .get(channel * area * bytes..(channel + 1) * area * bytes)
                        .ok_or("the merged picture is cut short")?
                        .to_vec();
                    if compression == 3 {
                        unpredict(&mut plane, row, bytes);
                    }
                    Ok(eight_bits(&plane, depth))
                })
                .collect::<Result<_, String>>()?
        }
        1 => {
            let rows = height as usize * usize::from(channels);
            let mut counts = Vec::with_capacity(rows);
            for _ in 0..rows {
                counts.push(if r.big {
                    r.u32()? as usize
                } else {
                    r.u16()? as usize
                });
            }
            let mut planes = Vec::with_capacity(used);
            for channel in 0..used {
                let mut plane = Vec::with_capacity(area);
                for line in 0..height as usize {
                    let packed = r.take(counts[channel * height as usize + line])?;
                    plane.extend(unpackbits(packed, row)?);
                }
                planes.push(eight_bits(&plane, depth));
            }
            planes
        }
        other => return Err(format!("Photoshop compression {other} is not read yet")),
    };
    for (slot, plane) in planes.iter().enumerate() {
        for (index, value) in plane.iter().enumerate().take(area) {
            merged[index * 4 + slot] = *value;
        }
    }
    Ok(merged)
}

/// Samples of `depth` bits as 8-bit ones (16-bit samples rounded).
fn eight_bits(raw: &[u8], depth: u16) -> Vec<u8> {
    if depth == 16 {
        raw.as_chunks::<2>()
            .0
            .iter()
            .map(|pair| {
                let value = u32::from(u16::from_be_bytes(*pair));
                ((value * 255 + 32_767) / 65_535) as u8
            })
            .collect()
    } else {
        raw.to_vec()
    }
}

/// Undoes zip prediction: each sample of a row of `row` bytes was stored
/// as its difference from the one before (samples of `bytes` bytes).
fn unpredict(plane: &mut [u8], row: usize, bytes: usize) {
    if row == 0 {
        return;
    }
    for line in plane.chunks_mut(row) {
        if bytes == 2 {
            let mut last = 0_u16;
            for pair in line.as_chunks_mut::<2>().0 {
                last = last.wrapping_add(u16::from_be_bytes(*pair));
                *pair = last.to_be_bytes();
            }
        } else {
            for i in 1..line.len() {
                line[i] = line[i].wrapping_add(line[i - 1]);
            }
        }
    }
}

/// One channel's data (its compression, then its pixels) as an 8-bit
/// plane.
fn channel_plane(
    data: &[u8],
    width: u32,
    height: u32,
    big: bool,
    depth: u16,
) -> Result<Vec<u8>, String> {
    let compression = u16::from_be_bytes([data[0], data[1]]);
    let body = &data[2..];
    let bytes = usize::from(depth / 8);
    let row = width as usize * bytes;
    let area = width as usize * height as usize * bytes;
    match compression {
        0 => Ok(eight_bits(
            body.get(..area).ok_or("a channel is cut short")?,
            depth,
        )),
        2 | 3 => {
            let mut plane =
                miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(body, area.max(1))
                    .map_err(|_| "a channel does not inflate")?;
            if plane.len() < area {
                return Err("a channel is cut short".into());
            }
            if compression == 3 {
                unpredict(&mut plane, row, bytes);
            }
            Ok(eight_bits(&plane, depth))
        }
        1 => {
            let size = if big { 4 } else { 2 };
            let table = height as usize * size;
            let counts = body.get(..table).ok_or("a channel is cut short")?;
            let mut at = table;
            let mut plane = Vec::with_capacity(area);
            for count in counts.chunks_exact(size) {
                let count = if big {
                    u32::from_be_bytes([count[0], count[1], count[2], count[3]]) as usize
                } else {
                    u16::from_be_bytes([count[0], count[1]]) as usize
                };
                let packed = body.get(at..at + count).ok_or("a channel is cut short")?;
                plane.extend(unpackbits(packed, row)?);
                at += count;
            }
            Ok(eight_bits(&plane, depth))
        }
        other => Err(format!("Photoshop compression {other} is not read yet")),
    }
}

/// A layer's alpha, red, green and blue planes.
fn layer_planes(layer: &PsdLayer) -> Result<[Vec<u8>; 4], String> {
    if layer.pixels.len() != layer.width as usize * layer.height as usize * 4 {
        return Err(format!("the pixels of {} do not fit its size", layer.name));
    }
    let [r, g, b, a] = planar(&layer.pixels, layer.width, layer.height);
    Ok([a, r, g, b])
}

/// RGBA8 split into red, green, blue and alpha planes.
fn planar(pixels: &[u8], width: u32, height: u32) -> [Vec<u8>; 4] {
    let area = width as usize * height as usize;
    let mut planes: [Vec<u8>; 4] = std::array::from_fn(|_| Vec::with_capacity(area));
    for pixel in pixels.as_chunks::<4>().0 {
        for (plane, value) in planes.iter_mut().zip(pixel) {
            plane.push(*value);
        }
    }
    planes
}

/// A channel as Photoshop stores it: RLE with a table of row lengths.
fn rle_channel(plane: &[u8], width: u32, height: u32, big: bool) -> Vec<u8> {
    let mut out = vec![0, 1];
    if width == 0 || height == 0 {
        return vec![0, 0];
    }
    let rows: Vec<Vec<u8>> = plane.chunks(width as usize).map(packbits).collect();
    for row in &rows {
        if big {
            out.extend_from_slice(&(row.len() as u32).to_be_bytes());
        } else {
            out.extend_from_slice(&(row.len() as u16).to_be_bytes());
        }
    }
    for row in rows {
        out.extend_from_slice(&row);
    }
    out
}

/// PackBits: runs of 2 to 128 repeats as `257 - n` and the byte, the rest
/// as `n - 1` and up to 128 literal bytes.
fn packbits(row: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(row.len() / 2 + 2);
    let mut at = 0;
    while at < row.len() {
        let mut run = 1;
        while at + run < row.len() && run < 128 && row[at + run] == row[at] {
            run += 1;
        }
        if run >= 2 {
            out.push((257 - run) as u8);
            out.push(row[at]);
            at += run;
            continue;
        }
        let start = at;
        while at < row.len() && at - start < 128 {
            if at + 1 < row.len() && row[at] == row[at + 1] {
                break;
            }
            at += 1;
        }
        out.push((at - start - 1) as u8);
        out.extend_from_slice(&row[start..at]);
    }
    out
}

/// PackBits undone, to `width` bytes.
fn unpackbits(packed: &[u8], width: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(width);
    let mut at = 0;
    while at < packed.len() && out.len() < width {
        let header = packed[at] as i8;
        at += 1;
        if header >= 0 {
            let count = header as usize + 1;
            out.extend_from_slice(packed.get(at..at + count).ok_or("a row is cut short")?);
            at += count;
        } else if header != -128 {
            let count = 1 - header as isize;
            let value = *packed.get(at).ok_or("a row is cut short")?;
            out.extend(std::iter::repeat_n(value, count as usize));
            at += 1;
        }
    }
    out.resize(width, 0);
    Ok(out)
}

/// The layer name as a Pascal string padded to four bytes (at most 255
/// bytes; the Unicode name keeps the rest).
fn pascal_name(out: &mut Vec<u8>, name: &str) {
    let mut bytes: Vec<u8> = name
        .chars()
        .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
        .collect();
    bytes.truncate(255);
    out.push(bytes.len() as u8);
    out.extend_from_slice(&bytes);
    let padded = (bytes.len() + 1).div_ceil(4) * 4;
    out.extend(std::iter::repeat_n(0, padded - bytes.len() - 1));
}

/// The layer name in full, as UTF-16 (`luni`).
fn unicode_name(out: &mut Vec<u8>, name: &str) {
    let units: Vec<u16> = name.encode_utf16().collect();
    let mut data = Vec::with_capacity(4 + units.len() * 2);
    data.extend_from_slice(&(units.len() as u32).to_be_bytes());
    for unit in units {
        data.extend_from_slice(&unit.to_be_bytes());
    }
    while data.len() % 4 != 0 {
        data.push(0);
    }
    out.extend_from_slice(b"8BIMluni");
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&data);
}

struct Writer {
    bytes: Vec<u8>,
    big: bool,
}

impl Writer {
    fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    /// A length that is 8 bytes in a `.psb`.
    fn long(&mut self, value: u64) {
        if self.big {
            self.bytes.extend_from_slice(&value.to_be_bytes());
        } else {
            self.u32(value as u32);
        }
    }

    fn reserve_u32(&mut self) -> usize {
        let at = self.bytes.len();
        self.u32(0);
        at
    }

    fn reserve_long(&mut self) -> usize {
        let at = self.bytes.len();
        self.long(0);
        at
    }

    /// The length of everything after the 4-byte field at `at`.
    fn fill_u32(&mut self, at: usize) {
        let length = (self.bytes.len() - at - 4) as u32;
        self.bytes[at..at + 4].copy_from_slice(&length.to_be_bytes());
    }

    fn fill_long(&mut self, at: usize) {
        if self.big {
            let length = (self.bytes.len() - at - 8) as u64;
            self.bytes[at..at + 8].copy_from_slice(&length.to_be_bytes());
        } else {
            self.fill_u32(at);
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    big: bool,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self
            .at
            .checked_add(count)
            .ok_or("the document is damaged")?;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or("the document is cut short")?;
        self.at = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }

    fn u64(&mut self) -> Result<u64, String> {
        let b = self.take(8)?;
        let mut eight = [0; 8];
        eight.copy_from_slice(b);
        Ok(u64::from_be_bytes(eight))
    }

    fn long(&mut self) -> Result<u64, String> {
        if self.big {
            self.u64()
        } else {
            Ok(u64::from(self.u32()?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Psd {
        let (width, height) = (6_u32, 4_u32);
        let mut merged = vec![0_u8; (width * height * 4) as usize];
        for (index, pixel) in merged.chunks_mut(4).enumerate() {
            pixel.copy_from_slice(&[index as u8 * 10, 200, 7, 255]);
        }
        // A gradient layer, offset, half transparent in places.
        let mut ink = Vec::new();
        for index in 0..(3 * 2) {
            ink.extend_from_slice(&[
                255,
                index as u8 * 40,
                0,
                if index % 2 == 0 { 255 } else { 128 },
            ]);
        }
        let mut inside = PsdLayer::pixels("Inked \u{3042}", 2, 1, 3, 2, ink);
        inside.blend = blend_key("Multiply");
        inside.opacity = 200;
        inside.clip = true;
        let mut hidden = PsdLayer::pixels("Hidden", 0, 0, 6, 4, vec![9; 6 * 4 * 4]);
        hidden.visible = false;
        Psd {
            width,
            height,
            layers: vec![
                PsdLayer::pixels("Paper", 0, 0, 6, 4, vec![255; 6 * 4 * 4]),
                PsdLayer::folder("</Layer group>", PsdKind::FolderEnd),
                hidden,
                inside,
                PsdLayer::folder("Folder", PsdKind::FolderOpen { open: false }),
            ],
            merged,
        }
    }

    #[test]
    fn psd_and_psb_round_trip_layers_folders_and_styles() {
        for big in [false, true] {
            let psd = sample();
            let bytes = encode_psd(&psd, big).unwrap();
            assert_eq!(&bytes[..4], b"8BPS");
            assert_eq!(bytes[5], if big { 2 } else { 1 });
            let back = decode_psd(&bytes).unwrap();
            assert_eq!(back, psd, "big {big}");
        }
    }

    #[test]
    fn packbits_round_trips_runs_and_literals() {
        let mut row: Vec<u8> = (0..300).map(|i| (i % 7) as u8).collect();
        row.extend(std::iter::repeat_n(42, 500));
        row.extend([1, 2, 1, 1, 3]);
        let packed = packbits(&row);
        assert!(packed.len() < row.len());
        assert_eq!(unpackbits(&packed, row.len()).unwrap(), row);
        assert_eq!(unpackbits(&packbits(&[]), 0).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn blend_keys_go_both_ways() {
        for name in [
            "Normal",
            "Multiply",
            "Screen",
            "Overlay",
            "ColorDodge",
            "Luminosity",
            "PassThrough",
        ] {
            assert_eq!(blend_name(blend_key(name)), name);
        }
        assert_eq!(blend_name(*b"diss"), "Normal");
    }

    #[test]
    fn damaged_or_unknown_documents_are_refused() {
        assert!(decode_psd(b"nope").is_err());
        let mut bytes = encode_psd(&sample(), false).unwrap();
        bytes.truncate(bytes.len() / 2);
        assert!(decode_psd(&bytes).is_err());
        let too_big = Psd {
            width: PSD_MAX_SIDE + 1,
            height: 1,
            layers: Vec::new(),
            merged: vec![0; (PSD_MAX_SIDE as usize + 1) * 4],
        };
        assert!(encode_psd(&too_big, false).is_err());
    }

    #[test]
    fn layer_masks_round_trip_on_off_and_unlinked() {
        for big in [false, true] {
            let mut psd = sample();
            let ramp = |w: u32, h: u32| (0..w * h).map(|i| (i * 37 % 256) as u8).collect();
            psd.layers[2].mask = Some(PsdMask {
                left: -2,
                top: 1,
                width: 4,
                height: 3,
                pixels: ramp(4, 3),
                outside: 255,
                enabled: true,
                linked: true,
            });
            psd.layers[3].mask = Some(PsdMask {
                left: 3,
                top: 0,
                width: 2,
                height: 2,
                pixels: ramp(2, 2),
                outside: 0,
                enabled: false,
                linked: false,
            });
            // A folder carries a mask too.
            psd.layers[4].mask = Some(PsdMask {
                left: 0,
                top: 0,
                width: 0,
                height: 0,
                pixels: Vec::new(),
                outside: 0,
                enabled: true,
                linked: true,
            });
            let bytes = encode_psd(&psd, big).unwrap();
            assert_eq!(decode_psd(&bytes).unwrap(), psd, "big {big}");
        }
        let mut wrong = sample();
        wrong.layers[0].mask = Some(PsdMask {
            left: 0,
            top: 0,
            width: 2,
            height: 2,
            pixels: vec![1],
            outside: 0,
            enabled: true,
            linked: true,
        });
        assert!(encode_psd(&wrong, false).is_err());
    }

    #[test]
    fn the_resolution_is_written_and_read() {
        let psd = sample();
        let plain = encode_psd(&psd, false).unwrap();
        assert_eq!(
            psd_info(&plain).unwrap(),
            PsdInfo {
                depth: 8,
                dpi: None
            }
        );
        for dpi in [72.0, 350.0, 600.5] {
            let bytes = encode_psd_at(&psd, false, Some(dpi)).unwrap();
            let info = psd_info(&bytes).unwrap();
            assert!((info.dpi.unwrap() - dpi).abs() < 1e-3, "{dpi}");
            assert_eq!(decode_psd(&bytes).unwrap(), psd);
        }
        // Pixels per centimeter read as per inch.
        let mut block = resolution_block(100.0);
        block[16..18].copy_from_slice(&2_u16.to_be_bytes());
        assert!((resolution(&block).unwrap() - 254.0).abs() < 1e-3);
        // A resource before it, with a name of odd length, is stepped over.
        let mut both = b"8BIM\x03\xed\x03abc\x00\x00\x00\x03xyz\x00".to_vec();
        both.extend(resolution_block(300.0));
        assert!((resolution(&both).unwrap() - 300.0).abs() < 1e-3);
        assert_eq!(resolution(b"8BIM\x03\xed\x00\x00\xff\xff\xff\xff"), None);
    }

    /// A 16-bit document of `width` by `height` with one layer whose four
    /// channels are stored raw, RLE, zip and zip with prediction, and a
    /// raw merged picture; each sample is `value(x, y, channel)`.
    fn sixteen_bit(width: u32, height: u32, value: impl Fn(u32, u32, usize) -> u16) -> Vec<u8> {
        let plane = |channel: usize| -> Vec<u8> {
            let mut out = Vec::new();
            for y in 0..height {
                for x in 0..width {
                    out.extend_from_slice(&value(x, y, channel).to_be_bytes());
                }
            }
            out
        };
        let row = width as usize * 2;
        let channel_data = |channel: usize, compression: u16| -> Vec<u8> {
            let raw = plane(channel);
            let mut out = compression.to_be_bytes().to_vec();
            match compression {
                0 => out.extend(raw),
                1 => {
                    let rows: Vec<Vec<u8>> = raw.chunks(row).map(packbits).collect();
                    for packed in &rows {
                        out.extend_from_slice(&(packed.len() as u16).to_be_bytes());
                    }
                    for packed in rows {
                        out.extend(packed);
                    }
                }
                2 => out.extend(miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6)),
                _ => {
                    let mut predicted = raw.clone();
                    for line in predicted.chunks_mut(row) {
                        let samples: Vec<u16> = line
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|pair| u16::from_be_bytes(*pair))
                            .collect();
                        for (i, pair) in line.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                            let before = if i == 0 { 0 } else { samples[i - 1] };
                            *pair = samples[i].wrapping_sub(before).to_be_bytes();
                        }
                    }
                    out.extend(miniz_oxide::deflate::compress_to_vec_zlib(&predicted, 6));
                }
            }
            out
        };
        let mut w = Writer {
            bytes: Vec::new(),
            big: false,
        };
        w.bytes.extend_from_slice(b"8BPS");
        w.u16(1);
        w.bytes.extend_from_slice(&[0; 6]);
        w.u16(4);
        w.u32(height);
        w.u32(width);
        w.u16(16);
        w.u16(3);
        w.u32(0);
        w.u32(0);
        let section = w.reserve_long();
        let info = w.reserve_long();
        w.u16((-1_i16) as u16);
        // Alpha raw, red RLE, green zip, blue zip with prediction.
        let channels: Vec<(i16, Vec<u8>)> = vec![
            (-1, channel_data(3, 0)),
            (0, channel_data(0, 1)),
            (1, channel_data(1, 2)),
            (2, channel_data(2, 3)),
        ];
        w.i32(0);
        w.i32(0);
        w.i32(height as i32);
        w.i32(width as i32);
        w.u16(4);
        for (id, data) in &channels {
            w.u16(*id as u16);
            w.u32(data.len() as u32);
        }
        w.bytes.extend_from_slice(b"8BIMnorm");
        w.bytes.extend_from_slice(&[255, 0, 8, 0]);
        let extra = w.reserve_u32();
        w.u32(0);
        w.u32(0);
        pascal_name(&mut w.bytes, "Deep");
        w.fill_u32(extra);
        for (_, data) in channels {
            w.bytes.extend(data);
        }
        if (w.bytes.len() - info - 4) % 2 == 1 {
            w.bytes.push(0);
        }
        w.fill_long(info);
        w.u32(0);
        w.fill_long(section);
        w.u16(0);
        for channel in [0, 1, 2, 3] {
            w.bytes.extend(plane(channel));
        }
        w.bytes
    }

    #[test]
    fn sixteen_bit_documents_read_rounded_to_eight_bits() {
        let (width, height) = (37_u32, 5_u32);
        let value = |x: u32, y: u32, channel: usize| -> u16 {
            ((x * 1777 + y * 9001 + channel as u32 * 12_345) % 65_536) as u16
        };
        let bytes = sixteen_bit(width, height, value);
        assert_eq!(psd_info(&bytes).unwrap().depth, 16);
        let psd = decode_psd(&bytes).unwrap();
        let eight = |v: u16| ((u32::from(v) * 255 + 32_767) / 65_535) as u8;
        let layer = &psd.layers[0];
        assert_eq!(
            (layer.name.as_str(), layer.width, layer.height),
            ("Deep", width, height)
        );
        for y in 0..height {
            for x in 0..width {
                let at = ((y * width + x) * 4) as usize;
                let want: Vec<u8> = (0..4).map(|c| eight(value(x, y, c))).collect();
                assert_eq!(&layer.pixels[at..at + 4], &want[..], "layer at {x},{y}");
                assert_eq!(&psd.merged[at..at + 4], &want[..], "merged at {x},{y}");
            }
        }
        // The ends of the range land on the ends.
        assert_eq!((eight(0), eight(65_535), eight(32_896)), (0, 255, 128));
        // Cut anywhere, it fails cleanly.
        for cut in (0..bytes.len()).step_by(13) {
            let _ = decode_psd(&bytes[..cut]);
        }
        let mut deeper = bytes.clone();
        deeper[22..24].copy_from_slice(&32_u16.to_be_bytes());
        assert!(decode_psd(&deeper).unwrap_err().contains("32-bit"));
    }

    /// Photoshop keeps the layers of a 16-bit document in an `Lr16`
    /// block after the global mask info, with the layer info left empty.
    #[test]
    fn sixteen_bit_layers_kept_in_an_lr16_block_are_read() {
        let (width, height) = (9_u32, 4_u32);
        let value = |x: u32, y: u32, channel: usize| -> u16 {
            ((x * 7001 + y * 333 + channel as u32 * 20_000) % 65_536) as u16
        };
        let plain = sixteen_bit(width, height, value);
        let want = decode_psd(&plain).unwrap();
        // Header, color mode and resources take 34 bytes; then the
        // section's length, the layer info's length and the layer info.
        let length = |at: usize| u32::from_be_bytes(plain[at..at + 4].try_into().unwrap());
        let info = length(38) as usize;
        let body = &plain[42..42 + info];
        let rest = &plain[42 + info + 4..];
        for (signature, padding) in [(b"8BIM", 0), (b"8B64", 3)] {
            let mut block = Vec::new();
            // An unknown block first, which is stepped over.
            block.extend_from_slice(b"8BIMPatt");
            block.extend_from_slice(&3_u32.to_be_bytes());
            block.extend_from_slice(&[1, 2, 3]);
            block.extend(std::iter::repeat_n(0, padding));
            block.extend_from_slice(signature);
            block.extend_from_slice(b"Lr16");
            block.extend_from_slice(&(info as u32).to_be_bytes());
            block.extend_from_slice(body);
            let mut bytes = plain[..34].to_vec();
            bytes.extend_from_slice(&((4 + 4 + block.len()) as u32).to_be_bytes());
            bytes.extend_from_slice(&0_u32.to_be_bytes());
            bytes.extend_from_slice(&0_u32.to_be_bytes());
            bytes.extend(block);
            bytes.extend_from_slice(rest);
            assert_eq!(decode_psd(&bytes).unwrap(), want, "padding {padding}");
            for cut in 34..bytes.len() {
                let _ = decode_psd(&bytes[..cut]);
            }
        }
    }

    #[test]
    fn eight_bit_zip_channels_with_prediction_are_read() {
        let mut row = vec![10_u8, 12, 15, 15, 200, 3];
        let original = row.clone();
        for i in (1..row.len()).rev() {
            row[i] = row[i].wrapping_sub(row[i - 1]);
        }
        unpredict(&mut row, 6, 1);
        assert_eq!(row, original);
    }
}
