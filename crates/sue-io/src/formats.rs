//! Format handlers: every codec Sue has, behind the kernel's one
//! [`FormatRegistry`]. Importers turn bytes into entities of a well-known
//! class (`sue.image`, `sue.audio`, `sue.sheet`, `sue.model`, ...), and
//! exporters turn entities back into bytes, so any feature can open and
//! save any of these formats. Formats Sue cannot decode yet (video, PDF,
//! fonts, MIDI, layered and camera files) still open: the original bytes
//! ride along as [`Encoded`] and export unchanged.

use sue_core::{
    Arena, Bundle, ComponentKind, ComponentRegistry, Entity, EntityId, FormatError, FormatHandler,
    FormatInfo, FormatRegistry, PixelFormat, Properties, PropertyDef, StringBuffer, SueFile, Value,
};
use sue_model::{AttributeData, Domain, Encoded, Geometry, PrimKind, Table, class};

use crate::picture_ops::{PictureOptions, transform_picture, write_picture};
use crate::{
    IngestRoute, JxlPipeline, NativeFormat, NativeJxlPipeline, PictureFormat, ProgressiveTarget,
    SampleDepth, decode_audio, decode_jpeg, decode_picture, decode_png, encode_jpeg, encode_png,
    encode_wav, meta, sniff_stream, write_sound,
};

fn native(head: &[u8]) -> Option<NativeFormat> {
    match sniff_stream(head) {
        IngestRoute::Native(format) => Some(format),
        _ => None,
    }
}

fn image_entity(name: &str, tile: sue_core::RasterTile2D, format: &str, bytes: usize) -> Entity {
    let (width, height) = (tile.width(), tile.height());
    Entity::new(name, tile)
        .with_class(class::IMAGE)
        .with_meta(meta::FORMAT, format)
        .with_meta(meta::BYTES, bytes.to_string())
        .with_meta(meta::WIDTH, width.to_string())
        .with_meta(meta::HEIGHT, height.to_string())
}

fn raster_of<'a>(
    arena: &'a Arena,
    ids: &[EntityId],
) -> Result<&'a sue_core::RasterTile2D, FormatError> {
    let id = ids
        .first()
        .ok_or_else(|| FormatError::new("nothing to export"))?;
    arena
        .get_by_id(*id)?
        .component(ComponentKind::RasterTile2D)
        .and_then(|component| component.as_raster())
        .ok_or_else(|| FormatError::new("only pictures export to this format"))
}

fn has_8bit_raster(entity: &Entity) -> bool {
    entity
        .component(ComponentKind::RasterTile2D)
        .and_then(|component| component.as_raster())
        .is_some_and(|tile| tile.format() != PixelFormat::RgbaF32)
}

/// PNG pictures.
#[derive(Clone, Copy, Debug, Default)]
pub struct PngFormat;

impl FormatHandler for PngFormat {
    fn info(&self) -> FormatInfo {
        FormatInfo {
            id: "png",
            name: "PNG image",
            extensions: &["png"],
            mime: "image/png",
            category: "Image",
        }
    }

    fn sniff(&self, head: &[u8]) -> bool {
        native(head) == Some(NativeFormat::Png)
    }

    fn can_import(&self) -> bool {
        true
    }

    fn import(
        &self,
        bytes: &[u8],
        name: &str,
        _options: &Properties,
    ) -> Result<Bundle, FormatError> {
        let decoded = decode_png(bytes).map_err(FormatError::new)?;
        Ok(Bundle::single(image_entity(
            name,
            decoded.tile,
            "PNG",
            bytes.len(),
        )))
    }

    fn can_export(&self, entity: &Entity) -> bool {
        entity.component(ComponentKind::RasterTile2D).is_some()
    }

    fn export(
        &self,
        arena: &Arena,
        ids: &[EntityId],
        _options: &Properties,
    ) -> Result<Vec<u8>, FormatError> {
        encode_png(raster_of(arena, ids)?).map_err(FormatError::new)
    }
}

/// JPEG pictures.
#[derive(Clone, Copy, Debug, Default)]
pub struct JpegFormat;

impl FormatHandler for JpegFormat {
    fn info(&self) -> FormatInfo {
        FormatInfo {
            id: "jpeg",
            name: "JPEG image",
            extensions: &["jpg", "jpeg", "jfif"],
            mime: "image/jpeg",
            category: "Image",
        }
    }

    fn sniff(&self, head: &[u8]) -> bool {
        native(head) == Some(NativeFormat::Jpeg)
    }

    fn can_import(&self) -> bool {
        true
    }

    fn import(
        &self,
        bytes: &[u8],
        name: &str,
        _options: &Properties,
    ) -> Result<Bundle, FormatError> {
        let decoded = decode_jpeg(bytes).map_err(FormatError::new)?;
        Ok(Bundle::single(image_entity(
            name,
            decoded.tile,
            "JPEG",
            bytes.len(),
        )))
    }

    fn can_export(&self, entity: &Entity) -> bool {
        has_8bit_raster(entity)
    }

    fn export(
        &self,
        arena: &Arena,
        ids: &[EntityId],
        options: &Properties,
    ) -> Result<Vec<u8>, FormatError> {
        let quality = options.f64("quality").unwrap_or(90.0).clamp(1.0, 100.0) as u8;
        encode_jpeg(raster_of(arena, ids)?, quality).map_err(FormatError::new)
    }

    fn options(&self) -> Vec<PropertyDef> {
        vec![PropertyDef::new("quality", "Quality", 90).range(1.0, 100.0)]
    }
}

/// JPEG XL pictures, Sue's own image format.
#[derive(Clone, Copy, Debug, Default)]
pub struct JxlFormat;

impl FormatHandler for JxlFormat {
    fn info(&self) -> FormatInfo {
        FormatInfo {
            id: "jxl",
            name: "JPEG XL image",
            extensions: &["jxl"],
            mime: "image/jxl",
            category: "Image",
        }
    }

    fn sniff(&self, head: &[u8]) -> bool {
        matches!(native(head), Some(NativeFormat::JpegXl { .. }))
    }

    fn can_import(&self) -> bool {
        true
    }

    fn import(
        &self,
        bytes: &[u8],
        name: &str,
        _options: &Properties,
    ) -> Result<Bundle, FormatError> {
        let decoded = NativeJxlPipeline
            .decode(bytes, ProgressiveTarget::Full)
            .map_err(|error| FormatError::new(error.to_string()))?;
        Ok(Bundle::single(image_entity(
            name,
            decoded.tile,
            "JPEG XL",
            bytes.len(),
        )))
    }

    fn can_export(&self, entity: &Entity) -> bool {
        has_8bit_raster(entity)
    }

    fn export(
        &self,
        arena: &Arena,
        ids: &[EntityId],
        options: &Properties,
    ) -> Result<Vec<u8>, FormatError> {
        let effort = options.f64("effort").unwrap_or(4.0).clamp(1.0, 9.0) as u8;
        NativeJxlPipeline
            .encode_layers(&[raster_of(arena, ids)?], effort)
            .map_err(|error| FormatError::new(error.to_string()))
    }

    fn options(&self) -> Vec<PropertyDef> {
        vec![PropertyDef::new("effort", "Effort", 4).range(1.0, 9.0)]
    }
}

/// Pictures the shared picture codec reads into pixels and writes from
/// them: Windows icons and Netpbm pixmaps.
#[derive(Clone, Copy, Debug)]
pub struct PictureFileFormat {
    info: FormatInfo,
    format: PictureFormat,
}

impl PictureFileFormat {
    /// Windows icons. A picture larger than 256 pixels is fitted into
    /// that size when it is written.
    #[must_use]
    pub fn ico() -> Self {
        Self {
            info: FormatInfo {
                id: "ico",
                name: "ICO image",
                extensions: &["ico"],
                mime: "image/vnd.microsoft.icon",
                category: "Image",
            },
            format: PictureFormat::Ico,
        }
    }

    /// Netpbm pixmaps. They hold no transparency, so a picture is laid
    /// over white when it is written.
    #[must_use]
    pub fn ppm() -> Self {
        Self {
            info: FormatInfo {
                id: "ppm",
                name: "PPM image",
                extensions: &["ppm", "pgm", "pbm", "pnm", "pam"],
                mime: "image/x-portable-pixmap",
                category: "Image",
            },
            format: PictureFormat::Pnm,
        }
    }
}

impl FormatHandler for PictureFileFormat {
    fn info(&self) -> FormatInfo {
        self.info
    }

    /// Icons are known by their first bytes. A pixmap starts with two
    /// characters of plain text, so only its file extension tells.
    fn sniff(&self, head: &[u8]) -> bool {
        self.format == PictureFormat::Ico && native(head) == Some(NativeFormat::Ico)
    }

    fn can_import(&self) -> bool {
        true
    }

    fn import(
        &self,
        bytes: &[u8],
        name: &str,
        _options: &Properties,
    ) -> Result<Bundle, FormatError> {
        let tile = decode_picture(bytes, Some(self.format)).map_err(FormatError::new)?;
        Ok(Bundle::single(image_entity(
            name,
            tile,
            self.format.name(),
            bytes.len(),
        )))
    }

    fn can_export(&self, entity: &Entity) -> bool {
        has_8bit_raster(entity)
    }

    fn export(
        &self,
        arena: &Arena,
        ids: &[EntityId],
        _options: &Properties,
    ) -> Result<Vec<u8>, FormatError> {
        let options = PictureOptions::default();
        let extension = self.format.extension();
        let fitted = transform_picture(raster_of(arena, ids)?, &options, extension)
            .map_err(FormatError::new)?;
        write_picture(&fitted, extension, &options).map_err(FormatError::new)
    }
}

fn audio_entity(name: &str, decoded: crate::DecodedAudio, format: &str, bytes: usize) -> Entity {
    let info = decoded.info;
    let mut entity = Entity::new(name, decoded.pcm)
        .with_class(class::AUDIO)
        .with_meta(meta::FORMAT, format)
        .with_meta(meta::BYTES, bytes.to_string())
        .with_meta(meta::CODEC, info.codec)
        .with_meta(meta::SAMPLE_RATE, info.sample_rate.to_string())
        .with_meta(meta::CHANNELS, info.channels.to_string());
    for (key, value) in [
        (meta::TITLE, info.title),
        (meta::ARTIST, info.artist),
        (meta::ALBUM, info.album),
    ] {
        if let Some(value) = value {
            entity = entity.with_meta(key, value);
        }
    }
    entity
}

fn decode_track(bytes: &[u8], name: &str, format: &str, hint: &str) -> Result<Bundle, FormatError> {
    let (decoded, _) = decode_audio(bytes.to_vec(), Some(hint))
        .map_err(|(message, _)| FormatError::new(message))?;
    Ok(Bundle::single(audio_entity(
        name,
        decoded,
        format,
        bytes.len(),
    )))
}

/// Writes the sound of the first of `ids` as a file of `extension`, with
/// the sample size of the `bits` option (16 or 24).
fn encode_track(
    arena: &Arena,
    ids: &[EntityId],
    options: &Properties,
    extension: &str,
) -> Result<Vec<u8>, FormatError> {
    let id = ids
        .first()
        .ok_or_else(|| FormatError::new("nothing to export"))?;
    let pcm = arena
        .get_by_id(*id)?
        .component(ComponentKind::PCMFloat1D)
        .and_then(|component| component.as_pcm())
        .ok_or_else(|| FormatError::new("only audio exports to this format"))?;
    let bits = options.f64("bits").unwrap_or(16.0);
    let depth = if bits >= 24.0 {
        SampleDepth::Int24
    } else {
        SampleDepth::Int16
    };
    write_sound(pcm, extension, depth).map_err(FormatError::new)
}

/// The `bits` option of the sound formats that write 16 or 24 bits.
fn bits_option() -> Vec<PropertyDef> {
    vec![PropertyDef::new("bits", "Bits", 16).range(16.0, 24.0)]
}

/// WAV audio.
#[derive(Clone, Copy, Debug, Default)]
pub struct WavFormat;

impl FormatHandler for WavFormat {
    fn info(&self) -> FormatInfo {
        FormatInfo {
            id: "wav",
            name: "WAV audio",
            extensions: &["wav", "wave"],
            mime: "audio/wav",
            category: "Audio",
        }
    }

    fn sniff(&self, head: &[u8]) -> bool {
        native(head) == Some(NativeFormat::Wav)
    }

    fn can_import(&self) -> bool {
        true
    }

    fn import(
        &self,
        bytes: &[u8],
        name: &str,
        _options: &Properties,
    ) -> Result<Bundle, FormatError> {
        decode_track(bytes, name, "WAV", "wav")
    }

    fn can_export(&self, entity: &Entity) -> bool {
        entity.component(ComponentKind::PCMFloat1D).is_some()
    }

    fn export(
        &self,
        arena: &Arena,
        ids: &[EntityId],
        _options: &Properties,
    ) -> Result<Vec<u8>, FormatError> {
        let id = ids
            .first()
            .ok_or_else(|| FormatError::new("nothing to export"))?;
        let pcm = arena
            .get_by_id(*id)?
            .component(ComponentKind::PCMFloat1D)
            .and_then(|component| component.as_pcm())
            .ok_or_else(|| FormatError::new("only audio exports to WAV"))?;
        Ok(encode_wav(pcm))
    }
}

/// Compressed audio Sue decodes (MP3, FLAC, Ogg Vorbis, AAC in M4A).
#[derive(Clone, Copy, Debug)]
pub struct CompressedAudioFormat {
    info: FormatInfo,
    native: NativeFormat,
}

impl CompressedAudioFormat {
    /// Every compressed audio format.
    #[must_use]
    pub fn all() -> [Self; 4] {
        let make = |id, name, extensions, mime, native| Self {
            info: FormatInfo {
                id,
                name,
                extensions,
                mime,
                category: "Audio",
            },
            native,
        };
        [
            make(
                "mp3",
                "MP3 audio",
                &["mp3"],
                "audio/mpeg",
                NativeFormat::Mp3,
            ),
            make(
                "flac",
                "FLAC audio",
                &["flac"],
                "audio/flac",
                NativeFormat::Flac,
            ),
            make(
                "ogg",
                "Ogg Vorbis audio",
                &["ogg", "oga"],
                "audio/ogg",
                NativeFormat::Ogg,
            ),
            make(
                "m4a",
                "AAC audio",
                &["m4a", "aac"],
                "audio/mp4",
                NativeFormat::M4a,
            ),
        ]
    }
}

impl FormatHandler for CompressedAudioFormat {
    fn info(&self) -> FormatInfo {
        self.info
    }

    fn sniff(&self, head: &[u8]) -> bool {
        native(head) == Some(self.native)
    }

    fn can_import(&self) -> bool {
        true
    }

    fn import(
        &self,
        bytes: &[u8],
        name: &str,
        _options: &Properties,
    ) -> Result<Bundle, FormatError> {
        decode_track(bytes, name, self.native.name(), self.info.extensions[0])
    }

    /// Of the compressed formats Sue writes FLAC, which loses nothing.
    fn can_export(&self, entity: &Entity) -> bool {
        self.native == NativeFormat::Flac && entity.component(ComponentKind::PCMFloat1D).is_some()
    }

    fn export(
        &self,
        arena: &Arena,
        ids: &[EntityId],
        options: &Properties,
    ) -> Result<Vec<u8>, FormatError> {
        if self.native != NativeFormat::Flac {
            return Err(FormatError::new(format!(
                "{} files cannot be written yet",
                self.info.name
            )));
        }
        encode_track(arena, ids, options, "flac")
    }

    fn options(&self) -> Vec<PropertyDef> {
        if self.native == NativeFormat::Flac {
            bits_option()
        } else {
            Vec::new()
        }
    }
}

/// AIFF audio.
#[derive(Clone, Copy, Debug, Default)]
pub struct AiffFormat;

impl FormatHandler for AiffFormat {
    fn info(&self) -> FormatInfo {
        FormatInfo {
            id: "aiff",
            name: "AIFF audio",
            extensions: &["aiff", "aif"],
            mime: "audio/aiff",
            category: "Audio",
        }
    }

    fn sniff(&self, head: &[u8]) -> bool {
        head.len() >= 12 && head.starts_with(b"FORM") && matches!(&head[8..12], b"AIFF" | b"AIFC")
    }

    fn can_import(&self) -> bool {
        true
    }

    fn import(
        &self,
        bytes: &[u8],
        name: &str,
        _options: &Properties,
    ) -> Result<Bundle, FormatError> {
        decode_track(bytes, name, "AIFF", "aiff")
    }

    fn can_export(&self, entity: &Entity) -> bool {
        entity.component(ComponentKind::PCMFloat1D).is_some()
    }

    fn export(
        &self,
        arena: &Arena,
        ids: &[EntityId],
        options: &Properties,
    ) -> Result<Vec<u8>, FormatError> {
        encode_track(arena, ids, options, "aiff")
    }

    fn options(&self) -> Vec<PropertyDef> {
        bits_option()
    }
}

/// Plain text and Markdown.
#[derive(Clone, Copy, Debug)]
pub struct TextFormat {
    info: FormatInfo,
}

impl TextFormat {
    /// Plain text.
    #[must_use]
    pub fn plain() -> Self {
        Self {
            info: FormatInfo {
                id: "txt",
                name: "Plain text",
                extensions: &["txt", "text", "log"],
                mime: "text/plain",
                category: "Document",
            },
        }
    }

    /// Markdown.
    #[must_use]
    pub fn markdown() -> Self {
        Self {
            info: FormatInfo {
                id: "md",
                name: "Markdown",
                extensions: &["md", "markdown"],
                mime: "text/markdown",
                category: "Document",
            },
        }
    }
}

impl FormatHandler for TextFormat {
    fn info(&self) -> FormatInfo {
        self.info
    }

    fn can_import(&self) -> bool {
        true
    }

    fn import(
        &self,
        bytes: &[u8],
        name: &str,
        _options: &Properties,
    ) -> Result<Bundle, FormatError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| FormatError::new("the file is not UTF-8 text"))?;
        let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
        Ok(Bundle::single(
            Entity::new(name, StringBuffer::from(text))
                .with_class(class::DOCUMENT)
                .with_meta(meta::FORMAT, self.info.name)
                .with_meta(meta::BYTES, bytes.len().to_string()),
        ))
    }

    fn can_export(&self, entity: &Entity) -> bool {
        entity.string_buffer().is_some()
    }

    fn export(
        &self,
        arena: &Arena,
        ids: &[EntityId],
        _options: &Properties,
    ) -> Result<Vec<u8>, FormatError> {
        let mut out = String::new();
        for id in ids {
            if let Some(text) = arena.get_by_id(*id)?.string_buffer() {
                if !out.is_empty() {
                    out.push_str("\n\n");
                }
                out.push_str(text.as_str());
            }
        }
        Ok(out.into_bytes())
    }
}

/// Comma- or tab-separated tables.
#[derive(Clone, Copy, Debug)]
pub struct DelimitedFormat {
    info: FormatInfo,
    delimiter: char,
}

impl DelimitedFormat {
    /// Comma-separated values.
    #[must_use]
    pub fn csv() -> Self {
        Self {
            info: FormatInfo {
                id: "csv",
                name: "CSV table",
                extensions: &["csv"],
                mime: "text/csv",
                category: "Table",
            },
            delimiter: ',',
        }
    }

    /// Tab-separated values.
    #[must_use]
    pub fn tsv() -> Self {
        Self {
            info: FormatInfo {
                id: "tsv",
                name: "TSV table",
                extensions: &["tsv", "tab"],
                mime: "text/tab-separated-values",
                category: "Table",
            },
            delimiter: '\t',
        }
    }
}

/// Splits delimited text into rows of fields (RFC 4180 quoting).
fn parse_delimited(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            match ch {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => quoted = false,
                _ => field.push(ch),
            }
            continue;
        }
        match ch {
            '"' if field.is_empty() => quoted = true,
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            _ if ch == delimiter => row.push(std::mem::take(&mut field)),
            _ => field.push(ch),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

fn typed(field: &str) -> Option<Value> {
    if field.is_empty() {
        return None;
    }
    let trimmed = field.trim();
    if let Ok(int) = trimmed.parse::<i64>() {
        return Some(Value::Int(int));
    }
    if let Ok(float) = trimmed.parse::<f64>()
        && float.is_finite()
    {
        return Some(Value::Float(float));
    }
    Some(Value::Text(field.to_owned()))
}

impl FormatHandler for DelimitedFormat {
    fn info(&self) -> FormatInfo {
        self.info
    }

    fn can_import(&self) -> bool {
        true
    }

    fn import(
        &self,
        bytes: &[u8],
        name: &str,
        _options: &Properties,
    ) -> Result<Bundle, FormatError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| FormatError::new("the file is not UTF-8 text"))?;
        let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
        let mut table = Table::new();
        for (row, fields) in parse_delimited(text, self.delimiter)
            .into_iter()
            .enumerate()
        {
            for (column, field) in fields.iter().enumerate() {
                if let Some(value) = typed(field) {
                    table.set(
                        row as u32,
                        column as u32,
                        Some(sue_model::Cell::value(value)),
                    );
                }
            }
        }
        Ok(Bundle::single(
            Entity::empty(name, class::SHEET)
                .with_ext(table)
                .with_meta(meta::FORMAT, self.info.name)
                .with_meta(meta::BYTES, bytes.len().to_string()),
        ))
    }

    fn can_export(&self, entity: &Entity) -> bool {
        entity.ext::<Table>().is_some()
    }

    fn export(
        &self,
        arena: &Arena,
        ids: &[EntityId],
        _options: &Properties,
    ) -> Result<Vec<u8>, FormatError> {
        let id = ids
            .first()
            .ok_or_else(|| FormatError::new("nothing to export"))?;
        let table = arena
            .get_by_id(*id)?
            .ext::<Table>()
            .ok_or_else(|| FormatError::new("only tables export to this format"))?;
        let (rows, columns) = table.size();
        let mut out = String::new();
        for row in 0..rows {
            for column in 0..columns {
                if column > 0 {
                    out.push(self.delimiter);
                }
                let text = table
                    .value(row, column)
                    .map(ToString::to_string)
                    .unwrap_or_default();
                if text.contains([self.delimiter, '"', '\n', '\r']) {
                    out.push('"');
                    out.push_str(&text.replace('"', "\"\""));
                    out.push('"');
                } else {
                    out.push_str(&text);
                }
            }
            out.push_str("\r\n");
        }
        Ok(out.into_bytes())
    }
}

/// Wavefront OBJ geometry.
#[derive(Clone, Copy, Debug, Default)]
pub struct ObjFormat;

fn obj_index(token: &str, count: usize) -> Result<u32, FormatError> {
    let first = token.split('/').next().unwrap_or("");
    let index: i64 = first
        .parse()
        .map_err(|_| FormatError::new(format!("`{token}` is not a vertex index")))?;
    let resolved = if index < 0 {
        count as i64 + index
    } else {
        index - 1
    };
    u32::try_from(resolved)
        .ok()
        .filter(|index| (*index as usize) < count)
        .ok_or_else(|| FormatError::new(format!("vertex {index} does not exist")))
}

impl FormatHandler for ObjFormat {
    fn info(&self) -> FormatInfo {
        FormatInfo {
            id: "obj",
            name: "Wavefront OBJ model",
            extensions: &["obj"],
            mime: "model/obj",
            category: "3D",
        }
    }

    fn can_import(&self) -> bool {
        true
    }

    fn import(
        &self,
        bytes: &[u8],
        name: &str,
        _options: &Properties,
    ) -> Result<Bundle, FormatError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| FormatError::new("the file is not UTF-8 text"))?;
        let mut geometry = Geometry::new();
        let mut groups = Vec::new();
        let mut group = String::new();
        for line in text.lines() {
            let mut tokens = line.split_whitespace();
            match tokens.next() {
                Some("v") => {
                    let mut xyz = [0.0; 3];
                    for value in &mut xyz {
                        *value = tokens
                            .next()
                            .and_then(|token| token.parse().ok())
                            .ok_or_else(|| FormatError::new(format!("bad vertex `{line}`")))?;
                    }
                    geometry.add_point(xyz);
                }
                Some(kind @ ("f" | "l")) => {
                    let count = geometry.points().len();
                    let points = tokens
                        .map(|token| obj_index(token, count))
                        .collect::<Result<Vec<_>, _>>()?;
                    let prim = if kind == "f" {
                        PrimKind::Polygon
                    } else {
                        PrimKind::Polyline
                    };
                    geometry.add_prim(prim, &points)?;
                    groups.push(group.clone());
                }
                Some("o" | "g") => group = tokens.collect::<Vec<_>>().join(" "),
                _ => {}
            }
        }
        if groups.iter().any(|group| !group.is_empty()) {
            geometry.set_attribute(Domain::Primitive, "name", AttributeData::Text(groups))?;
        }
        Ok(Bundle::single(
            Entity::empty(name, class::MODEL)
                .with_ext(geometry)
                .with_meta(meta::FORMAT, "OBJ")
                .with_meta(meta::BYTES, bytes.len().to_string()),
        ))
    }

    fn can_export(&self, entity: &Entity) -> bool {
        entity.ext::<Geometry>().is_some()
    }

    fn export(
        &self,
        arena: &Arena,
        ids: &[EntityId],
        _options: &Properties,
    ) -> Result<Vec<u8>, FormatError> {
        use std::fmt::Write as _;
        let mut out = String::from("# Sue\n");
        let mut offset = 0;
        for id in ids {
            let entity = arena.get_by_id(*id)?;
            let Some(geometry) = entity.ext::<Geometry>() else {
                continue;
            };
            let _ = writeln!(out, "o {}", entity.name());
            for [x, y, z] in geometry.points() {
                let _ = writeln!(out, "v {x} {y} {z}");
            }
            for prim in geometry.prims() {
                let tag = match prim.kind {
                    PrimKind::Polygon => "f",
                    PrimKind::Polyline | PrimKind::Bezier { .. } => "l",
                };
                out.push_str(tag);
                for point in geometry.prim_points(prim) {
                    let _ = write!(out, " {}", point + 1 + offset);
                }
                out.push('\n');
            }
            offset += geometry.points().len() as u32;
        }
        Ok(out.into_bytes())
    }
}

/// Sue's own project files.
#[derive(Debug, Default)]
pub struct SueFormat {
    registry: ComponentRegistry,
}

impl SueFormat {
    /// Reads components through `registry` (unknown ones stay opaque).
    #[must_use]
    pub fn new(registry: ComponentRegistry) -> Self {
        Self { registry }
    }
}

impl FormatHandler for SueFormat {
    fn info(&self) -> FormatInfo {
        FormatInfo {
            id: "sue",
            name: "Sue project",
            extensions: &["sue"],
            mime: "application/x-sue",
            category: "Project",
        }
    }

    fn sniff(&self, head: &[u8]) -> bool {
        SueFile::sniff(head)
    }

    fn can_import(&self) -> bool {
        true
    }

    fn import(
        &self,
        bytes: &[u8],
        _name: &str,
        _options: &Properties,
    ) -> Result<Bundle, FormatError> {
        let mut arena = SueFile::from_bytes(bytes)?.arena(&self.registry)?;
        let relations = arena.relations().iter().collect();
        let handles: Vec<_> = arena.iter().map(|(handle, _)| handle).collect();
        // Move the entities out; nothing is copied.
        let entities = handles
            .into_iter()
            .filter_map(|handle| arena.despawn(handle).ok())
            .collect();
        Ok(Bundle {
            entities,
            relations,
        })
    }

    fn can_export(&self, _entity: &Entity) -> bool {
        true
    }

    fn export(
        &self,
        arena: &Arena,
        ids: &[EntityId],
        _options: &Properties,
    ) -> Result<Vec<u8>, FormatError> {
        let bundle = Bundle::copy_from(arena, ids, true)?;
        let mut out = Arena::new();
        for entity in bundle.entities {
            out.insert_keep_id(entity)?;
        }
        for relation in bundle.relations {
            out.relate(relation)?;
        }
        let mut file = SueFile::new();
        file.set_arena(&out);
        Ok(file.to_bytes())
    }
}

/// A format Sue recognizes but cannot decode yet: the bytes are kept as
/// [`Encoded`] on an entity of the right class and export unchanged.
#[derive(Clone, Copy, Debug)]
pub struct KeptFormat {
    info: FormatInfo,
    native: NativeFormat,
    class: &'static str,
}

impl KeptFormat {
    /// Every kept format.
    #[must_use]
    pub fn all() -> Vec<Self> {
        let make = |id, name, extensions, mime, category, native, class| Self {
            info: FormatInfo {
                id,
                name,
                extensions,
                mime,
                category,
            },
            native,
            class,
        };
        vec![
            make(
                "mp4",
                "MPEG-4 video",
                &["mp4", "m4v"],
                "video/mp4",
                "Video",
                NativeFormat::Mp4,
                class::VIDEO,
            ),
            make(
                "mov",
                "QuickTime movie",
                &["mov"],
                "video/quicktime",
                "Video",
                NativeFormat::QuickTime,
                class::VIDEO,
            ),
            make(
                "mkv",
                "Matroska or WebM video",
                &["mkv", "webm"],
                "video/x-matroska",
                "Video",
                NativeFormat::Matroska,
                class::VIDEO,
            ),
            make(
                "avi",
                "AVI video",
                &["avi"],
                "video/x-msvideo",
                "Video",
                NativeFormat::Avi,
                class::VIDEO,
            ),
            make(
                "pdf",
                "PDF document",
                &["pdf"],
                "application/pdf",
                "Document",
                NativeFormat::Pdf,
                class::PUBLICATION,
            ),
            make(
                "midi",
                "MIDI song",
                &["mid", "midi"],
                "audio/midi",
                "Audio",
                NativeFormat::Midi,
                "sue.melody",
            ),
            make(
                "gif",
                "GIF image",
                &["gif"],
                "image/gif",
                "Image",
                NativeFormat::Gif,
                class::IMAGE,
            ),
            make(
                "webp",
                "WebP image",
                &["webp"],
                "image/webp",
                "Image",
                NativeFormat::WebP,
                class::IMAGE,
            ),
            make(
                "tiff",
                "TIFF image",
                &["tif", "tiff"],
                "image/tiff",
                "Image",
                NativeFormat::Tiff,
                class::IMAGE,
            ),
            make(
                "bmp",
                "BMP image",
                &["bmp"],
                "image/bmp",
                "Image",
                NativeFormat::Bmp,
                class::IMAGE,
            ),
            make(
                "avif",
                "AVIF image",
                &["avif"],
                "image/avif",
                "Image",
                NativeFormat::Avif,
                class::IMAGE,
            ),
            make(
                "heif",
                "HEIF image",
                &["heic", "heif"],
                "image/heif",
                "Image",
                NativeFormat::Heif,
                class::IMAGE,
            ),
            make(
                "glb",
                "glTF binary model",
                &["glb"],
                "model/gltf-binary",
                "3D",
                NativeFormat::Glb,
                class::MODEL,
            ),
            make(
                "font",
                "Font",
                &["ttf", "otf", "woff", "woff2"],
                "font/ttf",
                "Font",
                NativeFormat::Font,
                "sue.font",
            ),
        ]
    }
}

impl FormatHandler for KeptFormat {
    fn info(&self) -> FormatInfo {
        self.info
    }

    fn sniff(&self, head: &[u8]) -> bool {
        native(head) == Some(self.native)
    }

    fn can_import(&self) -> bool {
        true
    }

    fn import(
        &self,
        bytes: &[u8],
        name: &str,
        _options: &Properties,
    ) -> Result<Bundle, FormatError> {
        Ok(Bundle::single(
            Entity::empty(name, self.class)
                .with_ext(Encoded::new(self.info.id, bytes.to_vec()))
                .with_meta(meta::FORMAT, self.info.name)
                .with_meta(meta::BYTES, bytes.len().to_string()),
        ))
    }

    fn can_export(&self, entity: &Entity) -> bool {
        entity
            .ext::<Encoded>()
            .is_some_and(|encoded| encoded.format == self.info.id)
    }

    fn export(
        &self,
        arena: &Arena,
        ids: &[EntityId],
        _options: &Properties,
    ) -> Result<Vec<u8>, FormatError> {
        let id = ids
            .first()
            .ok_or_else(|| FormatError::new("nothing to export"))?;
        let entity = arena.get_by_id(*id)?;
        entity
            .ext::<Encoded>()
            .filter(|encoded| encoded.format == self.info.id)
            .map(|encoded| encoded.bytes.to_vec())
            .ok_or_else(|| {
                FormatError::new(format!("{} has no {} data", entity.name(), self.info.name))
            })
    }
}

/// Every format this build reads or writes, with `registry` for `.sue`
/// projects. Content sniffing picks the handler first, the extension second.
#[must_use]
/// Rich text documents through the publishing formats: Word reads and
/// writes, OpenDocument, EPUB and RTF write. A document's text and its
/// [`sue_model::TextMarks`] go out as one section with the default styles.
#[derive(Clone, Copy, Debug)]
pub struct PublishFormat {
    info: FormatInfo,
    write: fn(&crate::publish::Book) -> Vec<u8>,
    read: Option<ReadRich>,
}

/// Reads a document file into its text and marks.
type ReadRich = fn(&[u8]) -> Result<(String, sue_model::TextMarks), String>;

impl PublishFormat {
    /// Every publishing format the registry offers.
    pub fn all() -> [Self; 4] {
        use crate::publish as p;
        let info = |id, name, extensions, mime| FormatInfo {
            id,
            name,
            extensions,
            mime,
            category: "Document",
        };
        [
            Self {
                info: info(
                    "docx",
                    "Word document",
                    &["docx"],
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                ),
                write: p::to_docx,
                read: Some(p::from_docx),
            },
            Self {
                info: info(
                    "odt",
                    "OpenDocument text",
                    &["odt"],
                    "application/vnd.oasis.opendocument.text",
                ),
                write: p::to_odt,
                read: Some(p::from_odt),
            },
            Self {
                info: info("epub", "EPUB e-book", &["epub"], "application/epub+zip"),
                write: p::to_epub,
                read: None,
            },
            Self {
                info: info("rtf", "Rich Text Format", &["rtf"], "application/rtf"),
                write: |book| p::to_rtf(book).into_bytes(),
                read: None,
            },
        ]
    }
}

impl FormatHandler for PublishFormat {
    fn info(&self) -> FormatInfo {
        self.info
    }

    fn can_import(&self) -> bool {
        self.read.is_some()
    }

    fn import(
        &self,
        bytes: &[u8],
        name: &str,
        _options: &Properties,
    ) -> Result<Bundle, FormatError> {
        let read = self.read.ok_or_else(|| {
            FormatError::new(format!("{} files cannot be opened yet", self.info.name))
        })?;
        let (text, marks) = read(bytes).map_err(FormatError::new)?;
        let title = std::path::Path::new(name).file_stem().map_or_else(
            || name.to_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        Ok(Bundle::single(
            Entity::new(title, StringBuffer::from(text))
                .with_class(class::DOCUMENT)
                .with_ext(marks)
                .with_meta(meta::FORMAT, self.info.name)
                .with_meta(meta::BYTES, bytes.len().to_string()),
        ))
    }

    fn can_export(&self, entity: &Entity) -> bool {
        entity.string_buffer().is_some()
    }

    fn export(
        &self,
        arena: &Arena,
        ids: &[EntityId],
        _options: &Properties,
    ) -> Result<Vec<u8>, FormatError> {
        let mut book: Option<crate::publish::Book> = None;
        for id in ids {
            let entity = arena.get_by_id(*id)?;
            let Some(buffer) = entity.string_buffer() else {
                continue;
            };
            let mut text = buffer.as_str().to_owned();
            sue_model::ensure_final_newline(&mut text);
            let marks = entity
                .ext::<sue_model::TextMarks>()
                .cloned()
                .unwrap_or_default();
            match &mut book {
                None => book = Some(crate::publish::Book::single(entity.name(), &text, &marks)),
                Some(book) => book.sections.push(crate::publish::Section {
                    title: entity.name().to_owned(),
                    depth: 0,
                    text,
                    marks,
                    new_page: true,
                }),
            }
        }
        let book = book.ok_or_else(|| FormatError::new("there is no text to write"))?;
        Ok((self.write)(&book))
    }
}

pub fn formats(registry: ComponentRegistry) -> FormatRegistry {
    let mut formats = FormatRegistry::new();
    formats.register(SueFormat::new(registry));
    formats.register(JxlFormat);
    formats.register(PngFormat);
    formats.register(JpegFormat);
    formats.register(PictureFileFormat::ico());
    formats.register(PictureFileFormat::ppm());
    formats.register(WavFormat);
    for format in CompressedAudioFormat::all() {
        formats.register(format);
    }
    formats.register(AiffFormat);
    formats.register(TextFormat::plain());
    formats.register(TextFormat::markdown());
    formats.register(DelimitedFormat::csv());
    formats.register(DelimitedFormat::tsv());
    formats.register(ObjFormat);
    for format in PublishFormat::all() {
        formats.register(format);
    }
    // Layered pictures: real handlers, no longer kept bytes only.
    formats.register(crate::LayeredFormat::clip());
    formats.register(crate::LayeredFormat::psd());
    for format in KeptFormat::all() {
        formats.register(format);
    }
    formats
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::{InsertBundle, PCMFloat1D, RasterTile2D, UndoStack};

    fn registry() -> FormatRegistry {
        formats(sue_model::registry())
    }

    fn import_one(formats: &FormatRegistry, bytes: &[u8], name: &str) -> (Arena, EntityId) {
        let bundle = formats.import(bytes, name, &Properties::new()).unwrap();
        let mut arena = Arena::new();
        let insert = InsertBundle::new("Import", bundle);
        let id = insert.ids()[0];
        UndoStack::new().apply(&mut arena, insert).unwrap();
        (arena, id)
    }

    #[test]
    fn rich_text_goes_through_word_and_the_other_publishing_formats() {
        let formats = registry();
        let text = "Heading\nSome bold words.\n";
        let mut marks = sue_model::TextMarks::new();
        marks.set_paragraphs(
            text,
            0..1,
            sue_model::mark_kind::STYLE,
            Some("heading1".into()),
        );
        marks.set(13..17, sue_model::mark_kind::BOLD, Some(true.into()));
        let mut arena = Arena::new();
        let handle = arena.insert(
            Entity::new("Note", StringBuffer::from(text))
                .with_class(class::DOCUMENT)
                .with_ext(marks.clone()),
        );
        let id = arena.get(handle).unwrap().id();
        for format in ["docx", "odt", "epub", "rtf"] {
            let handler = formats.get(format).unwrap();
            assert!(handler.can_export(arena.get(handle).unwrap()));
            let bytes = handler.export(&arena, &[id], &Properties::new()).unwrap();
            assert!(bytes.len() > 100, "{format}");
        }
        let docx = formats
            .get("docx")
            .unwrap()
            .export(&arena, &[id], &Properties::new())
            .unwrap();
        let (back, back_id) = import_one(&formats, &docx, "Note.docx");
        let entity = back.get_by_id(back_id).unwrap();
        assert_eq!(entity.string_buffer().unwrap().as_str(), text);
        assert_eq!(entity.name(), "Note");
        assert!(
            entity
                .ext::<sue_model::TextMarks>()
                .unwrap()
                .covers(13..17, sue_model::mark_kind::BOLD)
        );
    }

    #[test]
    fn pictures_round_trip_through_every_image_format() {
        let formats = registry();
        let mut tile = RasterTile2D::new(3, 2, PixelFormat::Rgba8).unwrap();
        for (index, byte) in tile.bytes_mut().unwrap().iter_mut().enumerate() {
            *byte = if index % 4 == 3 {
                255
            } else {
                (index * 20) as u8
            };
        }
        let mut arena = Arena::new();
        let handle = arena.insert(Entity::new("pic", tile.clone()));
        let id = arena.get(handle).unwrap().id();
        for format in ["png", "jxl"] {
            let handler = formats.get(format).unwrap();
            assert!(handler.can_export(arena.get(handle).unwrap()));
            let bytes = handler.export(&arena, &[id], &Properties::new()).unwrap();
            let (back, back_id) = import_one(&formats, &bytes, &format!("pic.{format}"));
            let entity = back.get_by_id(back_id).unwrap();
            assert_eq!(entity.class(), class::IMAGE);
            let decoded = entity
                .component(ComponentKind::RasterTile2D)
                .unwrap()
                .as_raster()
                .unwrap();
            assert_eq!(decoded.bytes(), tile.bytes(), "{format} is lossless");
        }
        let jpeg = formats
            .get("jpeg")
            .unwrap()
            .export(&arena, &[id], &Properties::new().with("quality", 95))
            .unwrap();
        let (back, back_id) = import_one(&formats, &jpeg, "unnamed");
        assert_eq!(
            back.get_by_id(back_id).unwrap().meta(meta::FORMAT),
            Some("JPEG")
        );
    }

    #[test]
    fn audio_text_tables_and_models_round_trip() {
        let formats = registry();
        let pcm = PCMFloat1D::new(8000, 1, vec![0.0, 0.5, -0.5, 0.25]).unwrap();
        let mut arena = Arena::new();
        let handle = arena.insert(Entity::new("tone", pcm));
        let id = arena.get(handle).unwrap().id();
        let wav = formats
            .get("wav")
            .unwrap()
            .export(&arena, &[id], &Properties::new())
            .unwrap();
        let (back, back_id) = import_one(&formats, &wav, "tone.wav");
        assert_eq!(back.get_by_id(back_id).unwrap().class(), class::AUDIO);

        let (doc, doc_id) = import_one(&formats, "\u{FEFF}# Title\nBody".as_bytes(), "notes.md");
        assert_eq!(
            doc.get_by_id(doc_id)
                .unwrap()
                .string_buffer()
                .unwrap()
                .as_str(),
            "# Title\nBody"
        );
        assert_eq!(
            doc.get_by_id(doc_id).unwrap().meta(meta::FORMAT),
            Some("Markdown")
        );

        let csv = "Item,Qty,Note\r\nBolt,40,\"M8, zinc\"\r\nDuct,2.5,\"say \"\"hi\"\"\"\r\n";
        let (sheet, sheet_id) = import_one(&formats, csv.as_bytes(), "parts.csv");
        let table = sheet.get_by_id(sheet_id).unwrap().ext::<Table>().unwrap();
        assert_eq!(table.value(1, 1), Some(&Value::Int(40)));
        assert_eq!(table.value(2, 1), Some(&Value::Float(2.5)));
        assert_eq!(table.value(1, 2).and_then(Value::as_str), Some("M8, zinc"));
        let out = formats
            .get("csv")
            .unwrap()
            .export(&sheet, &[sheet_id], &Properties::new())
            .unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), csv);

        let obj = "o Tri\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\nl -3 -2\n";
        let (model, model_id) = import_one(&formats, obj.as_bytes(), "tri.obj");
        let geometry = model
            .get_by_id(model_id)
            .unwrap()
            .ext::<Geometry>()
            .unwrap();
        assert_eq!(geometry.prims().len(), 2);
        assert_eq!(geometry.to_mesh().triangle_count(), 1);
        let again = formats
            .get("obj")
            .unwrap()
            .export(&model, &[model_id], &Properties::new())
            .unwrap();
        let text = String::from_utf8(again).unwrap();
        assert!(text.contains("f 1 2 3") && text.contains("l 1 2"));
        assert!(
            formats
                .import(b"f 9 9 9", "bad.obj", &Properties::new())
                .is_err()
        );
    }

    #[test]
    fn projects_and_kept_formats_lose_nothing() {
        let formats = registry();
        let mp4 = [
            0, 0, 0, 24, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm', 0, 0, 2, 0,
        ];
        let (arena, id) = import_one(&formats, &mp4, "clip.bin");
        let entity = arena.get_by_id(id).unwrap();
        assert_eq!(entity.class(), class::VIDEO);
        let exporter = formats
            .exporters_for(entity)
            .find(|handler| handler.info().id == "mp4")
            .unwrap();
        assert_eq!(
            exporter.export(&arena, &[id], &Properties::new()).unwrap(),
            mp4
        );

        let project = formats
            .get("sue")
            .unwrap()
            .export(&arena, &[id], &Properties::new())
            .unwrap();
        let (back, back_id) = import_one(&formats, &project, "copy.sue");
        assert_eq!(back_id, id, "a project keeps its ids");
        assert_eq!(back.get_by_id(id).unwrap(), entity);
    }

    #[test]
    fn icons_and_pixmaps_round_trip_and_fit_their_format() {
        let formats = registry();
        let mut tile = RasterTile2D::new(3, 2, PixelFormat::Rgba8).unwrap();
        for (index, byte) in tile.bytes_mut().unwrap().iter_mut().enumerate() {
            *byte = if index % 4 == 3 {
                255
            } else {
                (index * 20) as u8
            };
        }
        let mut arena = Arena::new();
        let handle = arena.insert(Entity::new("pic", tile.clone()));
        let id = arena.get(handle).unwrap().id();
        for (format, name) in [("ico", "ICO"), ("ppm", "PPM")] {
            let handler = formats.get(format).unwrap();
            assert!(handler.can_export(arena.get(handle).unwrap()));
            let bytes = handler.export(&arena, &[id], &Properties::new()).unwrap();
            let (back, back_id) = import_one(&formats, &bytes, &format!("pic.{format}"));
            let entity = back.get_by_id(back_id).unwrap();
            assert_eq!(entity.class(), class::IMAGE);
            assert_eq!(entity.meta(meta::FORMAT), Some(name));
            let decoded = entity
                .component(ComponentKind::RasterTile2D)
                .unwrap()
                .as_raster()
                .unwrap();
            assert_eq!(decoded.bytes(), tile.bytes(), "{format} is lossless");
        }
        // An icon is known by its bytes, whatever the file is called.
        let ico = formats
            .get("ico")
            .unwrap()
            .export(&arena, &[id], &Properties::new())
            .unwrap();
        assert_eq!(
            formats.importer_for(&ico, "unnamed").unwrap().info().id,
            "ico"
        );
        // A pixmap only by its name: its first bytes are plain text.
        assert_eq!(formats.for_name("scan.pgm").unwrap().info().id, "ppm");
        assert!(
            formats
                .importer_for(b"P3 is a room", "notes.txt")
                .unwrap()
                .info()
                .id
                != "ppm"
        );

        // A picture too large for an icon is fitted into 256 pixels, and
        // a pixmap, which has no transparency, is laid over white.
        let mut large = RasterTile2D::new(512, 128, PixelFormat::Rgba8).unwrap();
        large.bytes_mut().unwrap().fill(0);
        let handle = arena.insert(Entity::new("large", large));
        let id = arena.get(handle).unwrap().id();
        let ico = formats
            .get("ico")
            .unwrap()
            .export(&arena, &[id], &Properties::new())
            .unwrap();
        let (back, back_id) = import_one(&formats, &ico, "large.ico");
        let icon = back
            .get_by_id(back_id)
            .unwrap()
            .component(ComponentKind::RasterTile2D)
            .unwrap()
            .as_raster()
            .unwrap();
        assert_eq!((icon.width(), icon.height()), (256, 64));
        assert!(icon.bytes().unwrap().iter().all(|&byte| byte == 0));
        let ppm = formats
            .get("ppm")
            .unwrap()
            .export(&arena, &[id], &Properties::new())
            .unwrap();
        let (back, back_id) = import_one(&formats, &ppm, "large.ppm");
        let pixmap = back
            .get_by_id(back_id)
            .unwrap()
            .component(ComponentKind::RasterTile2D)
            .unwrap()
            .as_raster()
            .unwrap();
        assert_eq!((pixmap.width(), pixmap.height()), (512, 128));
        assert!(pixmap.bytes().unwrap().iter().all(|&byte| byte == 255));

        // Float pixels and things that are no picture are not offered.
        let float = arena.insert(Entity::new(
            "float",
            RasterTile2D::new(1, 1, PixelFormat::RgbaF32).unwrap(),
        ));
        let sound = arena.insert(Entity::new(
            "tone",
            PCMFloat1D::new(8000, 1, vec![0.0]).unwrap(),
        ));
        for format in ["ico", "ppm"] {
            let handler = formats.get(format).unwrap();
            assert!(!handler.can_export(arena.get(float).unwrap()));
            assert!(!handler.can_export(arena.get(sound).unwrap()));
        }
        assert!(
            formats
                .get("ico")
                .unwrap()
                .import(b"not an icon", "bad.ico", &Properties::new())
                .is_err()
        );
    }

    #[test]
    fn flac_and_aiff_round_trip_at_16_and_24_bits() {
        let formats = registry();
        let samples: Vec<f32> = (0..800)
            .flat_map(|frame| {
                let left = (frame as f32 * 0.05).sin() * 0.5;
                [left, -left]
            })
            .collect();
        let pcm = PCMFloat1D::new(8000, 2, samples).unwrap();
        let mut arena = Arena::new();
        let handle = arena.insert(Entity::new("tone", pcm.clone()));
        let id = arena.get(handle).unwrap().id();
        let picture = arena.insert(Entity::new(
            "pic",
            RasterTile2D::new(1, 1, PixelFormat::Rgba8).unwrap(),
        ));
        for (format, name) in [("flac", "FLAC"), ("aiff", "AIFF")] {
            let handler = formats.get(format).unwrap();
            assert!(handler.can_export(arena.get(handle).unwrap()), "{format}");
            assert!(!handler.can_export(arena.get(picture).unwrap()), "{format}");
            assert_eq!(handler.options()[0].key, "bits", "{format}");
            let mut sizes = Vec::new();
            for (bits, tolerance) in [(16, 1e-4_f32), (24, 1e-5)] {
                let bytes = handler
                    .export(&arena, &[id], &Properties::new().with("bits", bits))
                    .unwrap();
                sizes.push(bytes.len());
                // The bytes tell the format, not the name.
                let (back, back_id) = import_one(&formats, &bytes, "unnamed");
                let entity = back.get_by_id(back_id).unwrap();
                assert_eq!(entity.class(), class::AUDIO);
                assert_eq!(entity.meta(meta::FORMAT), Some(name));
                let decoded = entity
                    .component(ComponentKind::PCMFloat1D)
                    .unwrap()
                    .as_pcm()
                    .unwrap();
                assert_eq!(
                    (decoded.sample_rate(), decoded.channels(), decoded.frames()),
                    (8000, 2, 800),
                    "{format} {bits}"
                );
                let worst = decoded
                    .samples()
                    .iter()
                    .zip(pcm.samples())
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0_f32, f32::max);
                assert!(worst <= tolerance, "{format} {bits} drifted by {worst}");
            }
            assert!(sizes[1] > sizes[0], "{format}: 24 bits take more room");
            // Without the option the file has 16 bits.
            let plain = handler.export(&arena, &[id], &Properties::new()).unwrap();
            assert_eq!(plain.len(), sizes[0], "{format}");
        }
        // The other compressed formats are still read only.
        for format in ["mp3", "ogg", "m4a"] {
            let handler = formats.get(format).unwrap();
            assert!(!handler.can_export(arena.get(handle).unwrap()), "{format}");
            assert!(handler.options().is_empty(), "{format}");
            assert!(
                handler.export(&arena, &[id], &Properties::new()).is_err(),
                "{format}"
            );
        }
    }

    /// The Converter had its own table writer. What it quoted, this
    /// format quotes too, and where it gave up (a tab or a line end in a
    /// tab-separated cell became a space) this one keeps the cell whole.
    #[test]
    fn delimited_tables_quote_every_cell_that_needs_it() {
        let formats = registry();
        let cells = [
            "plain",
            "Lee, Ada",
            "say \"hi\"",
            "two\nlines",
            "two\r\nlines",
            "a\tb",
            " spaced ",
            "\"",
        ];
        let mut table = Table::new();
        for (column, cell) in cells.iter().enumerate() {
            table.set(0, column as u32, Some(sue_model::Cell::value(*cell)));
            table.set(1, column as u32, Some(sue_model::Cell::value(*cell)));
        }
        let mut arena = Arena::new();
        let handle = arena.insert(Entity::empty("cells", class::SHEET).with_ext(table));
        let id = arena.get(handle).unwrap().id();
        for format in ["csv", "tsv"] {
            let bytes = formats
                .get(format)
                .unwrap()
                .export(&arena, &[id], &Properties::new())
                .unwrap();
            let (back, back_id) = import_one(&formats, &bytes, &format!("cells.{format}"));
            let read = back.get_by_id(back_id).unwrap().ext::<Table>().unwrap();
            assert_eq!(read.size(), (2, cells.len() as u32), "{format}");
            for row in 0..2 {
                for (column, cell) in cells.iter().enumerate() {
                    assert_eq!(
                        read.value(row, column as u32).and_then(Value::as_str),
                        Some(*cell),
                        "{format} row {row} column {column}"
                    );
                }
            }
        }
        let csv = formats
            .get("csv")
            .unwrap()
            .export(&arena, &[id], &Properties::new())
            .unwrap();
        let text = String::from_utf8(csv).unwrap();
        assert!(text.starts_with("plain,\"Lee, Ada\",\"say \"\"hi\"\"\",\"two\nlines\","));
        // In a tab-separated file a comma needs no quotes, a tab does.
        let tsv = formats
            .get("tsv")
            .unwrap()
            .export(&arena, &[id], &Properties::new())
            .unwrap();
        let text = String::from_utf8(tsv).unwrap();
        assert!(text.starts_with("plain\tLee, Ada\t\"say \"\"hi\"\"\"\t"));
        assert!(text.contains("\t\"a\tb\"\t"));
    }
}
