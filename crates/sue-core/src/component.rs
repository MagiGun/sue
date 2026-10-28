//! The seven typed buffer components every entity is built from.

use std::fmt;
use std::ops::Range;

use crate::codec::{CodecError, Reader, Writer};
use crate::{CoreError, EntityId};

/// Discriminant of the seven typed buffer components.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComponentKind {
    /// UTF-8 text.
    StringBuffer,
    /// Interleaved 32-bit float audio.
    PCMFloat1D,
    /// Cubic Bézier chain in 2D.
    VectorSpline2D,
    /// A tile of pixels.
    RasterTile2D,
    /// Indexed triangle mesh.
    MeshBuffer3D,
    /// Interval on the global flick clock.
    TimeSpan,
    /// Syntax or scene-graph node.
    ASTNode,
}

impl ComponentKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 7] = [
        Self::StringBuffer,
        Self::PCMFloat1D,
        Self::VectorSpline2D,
        Self::RasterTile2D,
        Self::MeshBuffer3D,
        Self::TimeSpan,
        Self::ASTNode,
    ];

    /// The kind's canonical name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::StringBuffer => "StringBuffer",
            Self::PCMFloat1D => "PCMFloat1D",
            Self::VectorSpline2D => "VectorSpline2D",
            Self::RasterTile2D => "RasterTile2D",
            Self::MeshBuffer3D => "MeshBuffer3D",
            Self::TimeSpan => "TimeSpan",
            Self::ASTNode => "ASTNode",
        }
    }

    /// The kind with [`Self::name`] `name`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// The class an entity gets when this kind is its base.
    #[must_use]
    pub const fn default_class(self) -> &'static str {
        match self {
            Self::StringBuffer => "sue.text",
            Self::PCMFloat1D => "sue.audio",
            Self::VectorSpline2D => "sue.vector",
            Self::RasterTile2D => "sue.image",
            Self::MeshBuffer3D => "sue.mesh",
            Self::TimeSpan => "sue.span",
            Self::ASTNode => "sue.node",
        }
    }
}

impl fmt::Display for ComponentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

// ─── StringBuffer ───────────────────────────────────────────────────────────

/// UTF-8 text addressed by byte offsets that must fall on character boundaries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringBuffer {
    text: String,
}

impl StringBuffer {
    /// An empty buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// `true` if the buffer holds no text.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Inserts `text` at byte `offset`.
    ///
    /// # Errors
    /// [`CoreError::OutOfBounds`] or [`CoreError::NotCharBoundary`] for a bad offset.
    pub fn insert(&mut self, offset: usize, text: &str) -> Result<(), CoreError> {
        self.check_offset(offset)?;
        self.text.insert_str(offset, text);
        Ok(())
    }

    /// Removes the byte `range` and returns the removed text.
    ///
    /// # Errors
    /// [`CoreError::OutOfBounds`] or [`CoreError::NotCharBoundary`] for a bad range.
    pub fn remove(&mut self, range: Range<usize>) -> Result<String, CoreError> {
        if range.start > range.end {
            return Err(CoreError::OutOfBounds {
                offset: range.start,
                len: self.text.len(),
            });
        }
        self.check_offset(range.start)?;
        self.check_offset(range.end)?;
        Ok(self.text.drain(range).collect())
    }

    fn check_offset(&self, offset: usize) -> Result<(), CoreError> {
        if offset > self.text.len() {
            return Err(CoreError::OutOfBounds {
                offset,
                len: self.text.len(),
            });
        }
        if !self.text.is_char_boundary(offset) {
            return Err(CoreError::NotCharBoundary { offset });
        }
        Ok(())
    }
}

impl From<&str> for StringBuffer {
    fn from(text: &str) -> Self {
        Self { text: text.into() }
    }
}

impl From<String> for StringBuffer {
    fn from(text: String) -> Self {
        Self { text }
    }
}

// ─── PCMFloat1D ─────────────────────────────────────────────────────────────

/// Interleaved 32-bit float PCM audio.
#[derive(Clone, Debug, PartialEq)]
pub struct PCMFloat1D {
    sample_rate: u32,
    channels: u16,
    samples: Vec<f32>,
}

impl PCMFloat1D {
    /// Wraps interleaved `samples`.
    ///
    /// # Errors
    /// [`CoreError::InvalidBuffer`] if the rate or channel count is zero, or the
    /// sample count is not a multiple of the channel count.
    pub fn new(sample_rate: u32, channels: u16, samples: Vec<f32>) -> Result<Self, CoreError> {
        if sample_rate == 0 {
            return Err(CoreError::InvalidBuffer("sample rate must be non-zero"));
        }
        if channels == 0 {
            return Err(CoreError::InvalidBuffer("channel count must be non-zero"));
        }
        if !samples.len().is_multiple_of(usize::from(channels)) {
            return Err(CoreError::InvalidBuffer(
                "sample count must be a multiple of the channel count",
            ));
        }
        Ok(Self {
            sample_rate,
            channels,
            samples,
        })
    }

    /// `frames` frames of silence.
    ///
    /// # Errors
    /// Same conditions as [`PCMFloat1D::new`].
    pub fn silence(sample_rate: u32, channels: u16, frames: usize) -> Result<Self, CoreError> {
        let len = frames
            .checked_mul(usize::from(channels))
            .ok_or(CoreError::InvalidBuffer("sample count overflows usize"))?;
        Self::new(sample_rate, channels, vec![0.0; len])
    }

    /// Samples per second, per channel.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Number of interleaved channels.
    #[must_use]
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Number of frames (samples per channel).
    #[must_use]
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }

    /// Duration in seconds.
    #[must_use]
    pub fn duration_seconds(&self) -> f64 {
        self.frames() as f64 / f64::from(self.sample_rate)
    }

    /// The interleaved samples.
    #[must_use]
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// The interleaved samples, mutably. The length is fixed.
    pub fn samples_mut(&mut self) -> &mut [f32] {
        &mut self.samples
    }
}

// ─── VectorSpline2D ─────────────────────────────────────────────────────────

/// A chain of cubic Bézier segments stored as `3n + 1` knots:
/// `p0, c0a, c0b, p1, c1a, c1b, p2, …`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VectorSpline2D {
    knots: Vec<[f32; 2]>,
    closed: bool,
}

impl VectorSpline2D {
    /// Wraps a knot list.
    ///
    /// # Errors
    /// [`CoreError::InvalidBuffer`] unless the list is empty or has `3n + 1` knots.
    pub fn new(knots: Vec<[f32; 2]>, closed: bool) -> Result<Self, CoreError> {
        if !knots.is_empty() && !(knots.len() - 1).is_multiple_of(3) {
            return Err(CoreError::InvalidBuffer(
                "a cubic Bézier chain needs 3n + 1 knots",
            ));
        }
        Ok(Self { knots, closed })
    }

    /// The knots.
    #[must_use]
    pub fn knots(&self) -> &[[f32; 2]] {
        &self.knots
    }

    /// `true` if the last point connects back to the first.
    #[must_use]
    pub fn closed(&self) -> bool {
        self.closed
    }

    /// Number of cubic segments.
    #[must_use]
    pub fn segment_count(&self) -> usize {
        self.knots.len().saturating_sub(1) / 3
    }

    /// Evaluates `segment` at `t ∈ [0, 1]` (clamped) with the Bernstein form.
    #[must_use]
    pub fn point_at(&self, segment: usize, t: f32) -> Option<[f32; 2]> {
        let start = segment.checked_mul(3)?;
        let p = self.knots.get(start..start.checked_add(4)?)?;
        let t = t.clamp(0.0, 1.0);
        let u = 1.0 - t;
        let weights = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        let mut point = [0.0_f32; 2];
        for (knot, weight) in p.iter().zip(weights) {
            point[0] += knot[0] * weight;
            point[1] += knot[1] * weight;
        }
        Some(point)
    }
}

// ─── RasterTile2D ───────────────────────────────────────────────────────────

/// Pixel layout of a [`RasterTile2D`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// One 8-bit channel.
    Gray8,
    /// Four 8-bit channels.
    Rgba8,
    /// Four 32-bit float channels (HDR / linear light).
    RgbaF32,
}

impl PixelFormat {
    /// Channels per pixel.
    #[must_use]
    pub const fn channels(self) -> usize {
        match self {
            Self::Gray8 => 1,
            Self::Rgba8 | Self::RgbaF32 => 4,
        }
    }

    /// Bytes per pixel.
    #[must_use]
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Gray8 => 1,
            Self::Rgba8 => 4,
            Self::RgbaF32 => 16,
        }
    }
}

/// Backing storage of a [`RasterTile2D`].
#[derive(Clone, Debug, PartialEq)]
pub enum PixelStorage {
    /// 8-bit channels (`Gray8`, `Rgba8`).
    U8(Vec<u8>),
    /// 32-bit float channels (`RgbaF32`).
    F32(Vec<f32>),
}

/// A rectangular tile of pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct RasterTile2D {
    width: u32,
    height: u32,
    format: PixelFormat,
    pixels: PixelStorage,
}

impl RasterTile2D {
    /// A zero-filled tile.
    ///
    /// # Errors
    /// [`CoreError::InvalidBuffer`] for a zero or overflowing size.
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Result<Self, CoreError> {
        let len = Self::sample_count(width, height, format)?;
        let pixels = match format {
            PixelFormat::Gray8 | PixelFormat::Rgba8 => PixelStorage::U8(vec![0; len]),
            PixelFormat::RgbaF32 => PixelStorage::F32(vec![0.0; len]),
        };
        Ok(Self {
            width,
            height,
            format,
            pixels,
        })
    }

    /// Wraps 8-bit pixel data without copying it.
    ///
    /// # Errors
    /// [`CoreError::InvalidBuffer`] if `format` is not 8-bit or the length is wrong.
    pub fn from_bytes(
        width: u32,
        height: u32,
        format: PixelFormat,
        bytes: Vec<u8>,
    ) -> Result<Self, CoreError> {
        if format == PixelFormat::RgbaF32 {
            return Err(CoreError::InvalidBuffer("RgbaF32 tiles need f32 samples"));
        }
        if bytes.len() != Self::sample_count(width, height, format)? {
            return Err(CoreError::InvalidBuffer(
                "pixel byte count does not match size",
            ));
        }
        Ok(Self {
            width,
            height,
            format,
            pixels: PixelStorage::U8(bytes),
        })
    }

    /// Wraps `RgbaF32` samples without copying them.
    ///
    /// # Errors
    /// [`CoreError::InvalidBuffer`] if the sample count does not match the size.
    pub fn from_f32(width: u32, height: u32, samples: Vec<f32>) -> Result<Self, CoreError> {
        if samples.len() != Self::sample_count(width, height, PixelFormat::RgbaF32)? {
            return Err(CoreError::InvalidBuffer("sample count does not match size"));
        }
        Ok(Self {
            width,
            height,
            format: PixelFormat::RgbaF32,
            pixels: PixelStorage::F32(samples),
        })
    }

    fn sample_count(width: u32, height: u32, format: PixelFormat) -> Result<usize, CoreError> {
        if width == 0 || height == 0 {
            return Err(CoreError::InvalidBuffer("tile dimensions must be non-zero"));
        }
        (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(format.channels()))
            .ok_or(CoreError::InvalidBuffer("tile size overflows usize"))
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Pixel layout.
    #[must_use]
    pub fn format(&self) -> PixelFormat {
        self.format
    }

    /// The backing storage.
    #[must_use]
    pub fn pixels(&self) -> &PixelStorage {
        &self.pixels
    }

    /// 8-bit channel data, if the tile is 8-bit.
    #[must_use]
    pub fn bytes(&self) -> Option<&[u8]> {
        match &self.pixels {
            PixelStorage::U8(bytes) => Some(bytes),
            PixelStorage::F32(_) => None,
        }
    }

    /// 8-bit channel data, mutably. The length is fixed.
    pub fn bytes_mut(&mut self) -> Option<&mut [u8]> {
        match &mut self.pixels {
            PixelStorage::U8(bytes) => Some(bytes),
            PixelStorage::F32(_) => None,
        }
    }

    /// Float channel data, if the tile is `RgbaF32`.
    #[must_use]
    pub fn f32_samples(&self) -> Option<&[f32]> {
        match &self.pixels {
            PixelStorage::F32(samples) => Some(samples),
            PixelStorage::U8(_) => None,
        }
    }

    /// Float channel data, mutably. The length is fixed.
    pub fn f32_samples_mut(&mut self) -> Option<&mut [f32]> {
        match &mut self.pixels {
            PixelStorage::F32(samples) => Some(samples),
            PixelStorage::U8(_) => None,
        }
    }

    /// Size of the pixel data in bytes.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        match &self.pixels {
            PixelStorage::U8(bytes) => bytes.len(),
            PixelStorage::F32(samples) => samples.len() * size_of::<f32>(),
        }
    }
}

// ─── MeshBuffer3D ───────────────────────────────────────────────────────────

/// An indexed triangle mesh.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshBuffer3D {
    positions: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

impl MeshBuffer3D {
    /// Wraps vertex positions and triangle indices.
    ///
    /// # Errors
    /// [`CoreError::InvalidBuffer`] if the index count is not a multiple of three
    /// or an index points past the last vertex.
    pub fn new(positions: Vec<[f32; 3]>, indices: Vec<u32>) -> Result<Self, CoreError> {
        if !indices.len().is_multiple_of(3) {
            return Err(CoreError::InvalidBuffer(
                "index count must be a multiple of three",
            ));
        }
        if indices
            .iter()
            .any(|&index| index as usize >= positions.len())
        {
            return Err(CoreError::InvalidBuffer("triangle index out of bounds"));
        }
        Ok(Self { positions, indices })
    }

    /// A `width × height` quad in the XY plane: the 3D projection of a 2D tile.
    #[must_use]
    pub fn quad(width: f32, height: f32) -> Self {
        Self {
            positions: vec![
                [0.0, 0.0, 0.0],
                [width, 0.0, 0.0],
                [width, height, 0.0],
                [0.0, height, 0.0],
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    }

    /// Vertex positions.
    #[must_use]
    pub fn positions(&self) -> &[[f32; 3]] {
        &self.positions
    }

    /// Triangle indices.
    #[must_use]
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// Number of vertices.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Number of triangles.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

// ─── TimeSpan ───────────────────────────────────────────────────────────────

/// Ticks per second of the global clock. A flick divides evenly by every common
/// frame rate (24 to 120 fps) and audio sample rate (8 to 192 kHz).
pub const FLICKS_PER_SECOND: i64 = 705_600_000;

/// A half-open interval `[start, start + duration)` in flicks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimeSpan {
    start: i64,
    duration: i64,
}

impl TimeSpan {
    /// A span starting at `start` flicks and lasting `duration` flicks.
    ///
    /// # Errors
    /// [`CoreError::InvalidBuffer`] for a negative duration or an end that overflows.
    pub fn new(start: i64, duration: i64) -> Result<Self, CoreError> {
        if duration < 0 {
            return Err(CoreError::InvalidBuffer("duration must be non-negative"));
        }
        if start.checked_add(duration).is_none() {
            return Err(CoreError::InvalidBuffer("span end overflows the clock"));
        }
        Ok(Self { start, duration })
    }

    /// A span given in seconds, rounded to the nearest flick.
    ///
    /// # Errors
    /// [`CoreError::InvalidBuffer`] for non-finite input, or as for [`TimeSpan::new`].
    pub fn from_seconds(start: f64, duration: f64) -> Result<Self, CoreError> {
        if !start.is_finite() || !duration.is_finite() {
            return Err(CoreError::InvalidBuffer("time must be finite"));
        }
        let to_flicks = |seconds: f64| (seconds * FLICKS_PER_SECOND as f64).round() as i64;
        Self::new(to_flicks(start), to_flicks(duration))
    }

    /// Start, in flicks.
    #[must_use]
    pub const fn start(self) -> i64 {
        self.start
    }

    /// Duration, in flicks.
    #[must_use]
    pub const fn duration(self) -> i64 {
        self.duration
    }

    /// Exclusive end, in flicks.
    #[must_use]
    pub const fn end(self) -> i64 {
        self.start + self.duration
    }

    /// `true` if instant `t` lies inside the span.
    #[must_use]
    pub fn contains(self, t: i64) -> bool {
        (self.start..self.end()).contains(&t)
    }

    /// Duration in seconds.
    #[must_use]
    pub fn duration_seconds(self) -> f64 {
        self.duration as f64 / FLICKS_PER_SECOND as f64
    }
}

// ─── ASTNode ────────────────────────────────────────────────────────────────

/// A node of a syntax tree or scene graph. `span` is a byte range into the
/// entity's `StringBuffer`; children are other entities (by stable id),
/// forming a DAG.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ASTNode {
    kind: String,
    span: Range<usize>,
    children: Vec<EntityId>,
}

impl ASTNode {
    /// A node of `kind` covering `span`.
    ///
    /// # Errors
    /// [`CoreError::InvalidBuffer`] if the span is reversed.
    pub fn new(kind: impl Into<String>, span: Range<usize>) -> Result<Self, CoreError> {
        if span.start > span.end {
            return Err(CoreError::InvalidBuffer("AST span start exceeds its end"));
        }
        Ok(Self {
            kind: kind.into(),
            span,
            children: Vec::new(),
        })
    }

    /// Adds child entities.
    #[must_use]
    pub fn with_children(mut self, children: impl IntoIterator<Item = EntityId>) -> Self {
        self.children.extend(children);
        self
    }

    /// The node kind, e.g. `"fn_item"` or `"mesh_instance"`.
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Byte span in the source text.
    #[must_use]
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }

    /// Child entities.
    #[must_use]
    pub fn children(&self) -> &[EntityId] {
        &self.children
    }

    /// Points every child link through `map`.
    pub fn remap_children(&mut self, map: &mut dyn FnMut(EntityId) -> EntityId) {
        for child in &mut self.children {
            *child = map(*child);
        }
    }
}

// ─── ComponentData ──────────────────────────────────────────────────────────

/// One typed buffer component.
#[derive(Clone, Debug, PartialEq)]
pub enum ComponentData {
    /// UTF-8 text.
    StringBuffer(StringBuffer),
    /// Interleaved float audio.
    PCMFloat1D(PCMFloat1D),
    /// Cubic Bézier chain.
    VectorSpline2D(VectorSpline2D),
    /// Pixel tile.
    RasterTile2D(RasterTile2D),
    /// Triangle mesh.
    MeshBuffer3D(MeshBuffer3D),
    /// Clock interval.
    TimeSpan(TimeSpan),
    /// Syntax / scene node.
    ASTNode(ASTNode),
}

impl ComponentData {
    /// The component's kind.
    #[must_use]
    pub const fn kind(&self) -> ComponentKind {
        match self {
            Self::StringBuffer(_) => ComponentKind::StringBuffer,
            Self::PCMFloat1D(_) => ComponentKind::PCMFloat1D,
            Self::VectorSpline2D(_) => ComponentKind::VectorSpline2D,
            Self::RasterTile2D(_) => ComponentKind::RasterTile2D,
            Self::MeshBuffer3D(_) => ComponentKind::MeshBuffer3D,
            Self::TimeSpan(_) => ComponentKind::TimeSpan,
            Self::ASTNode(_) => ComponentKind::ASTNode,
        }
    }

    /// Approximate heap footprint of the payload, in bytes.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        match self {
            Self::StringBuffer(text) => text.len(),
            Self::PCMFloat1D(pcm) => size_of_val(pcm.samples()),
            Self::VectorSpline2D(spline) => size_of_val(spline.knots()),
            Self::RasterTile2D(tile) => tile.byte_len(),
            Self::MeshBuffer3D(mesh) => size_of_val(mesh.positions()) + size_of_val(mesh.indices()),
            Self::TimeSpan(_) => 0,
            Self::ASTNode(node) => node.kind.len() + size_of_val(node.children()),
        }
    }

    /// A borrowed `f32` tensor view of the numeric payload, if it has one.
    /// This is the zero-copy interface used by AI kernels.
    #[must_use]
    pub fn f32_view(&self) -> Option<&[f32]> {
        match self {
            Self::PCMFloat1D(pcm) => Some(pcm.samples()),
            Self::VectorSpline2D(spline) => Some(spline.knots.as_flattened()),
            Self::RasterTile2D(tile) => tile.f32_samples(),
            Self::MeshBuffer3D(mesh) => Some(mesh.positions.as_flattened()),
            Self::StringBuffer(_) | Self::TimeSpan(_) | Self::ASTNode(_) => None,
        }
    }

    /// A mutable `f32` tensor view. Lengths are fixed, so invariants hold.
    pub fn f32_view_mut(&mut self) -> Option<&mut [f32]> {
        match self {
            Self::PCMFloat1D(pcm) => Some(pcm.samples_mut()),
            Self::VectorSpline2D(spline) => Some(spline.knots.as_flattened_mut()),
            Self::RasterTile2D(tile) => tile.f32_samples_mut(),
            Self::MeshBuffer3D(mesh) => Some(mesh.positions.as_flattened_mut()),
            Self::StringBuffer(_) | Self::TimeSpan(_) | Self::ASTNode(_) => None,
        }
    }

    /// The text payload, if this is a `StringBuffer`.
    #[must_use]
    pub fn as_string_buffer(&self) -> Option<&StringBuffer> {
        match self {
            Self::StringBuffer(text) => Some(text),
            _ => None,
        }
    }

    /// The text payload, mutably.
    pub fn as_string_buffer_mut(&mut self) -> Option<&mut StringBuffer> {
        match self {
            Self::StringBuffer(text) => Some(text),
            _ => None,
        }
    }

    /// The audio payload, if this is a `PCMFloat1D`.
    #[must_use]
    pub fn as_pcm(&self) -> Option<&PCMFloat1D> {
        match self {
            Self::PCMFloat1D(pcm) => Some(pcm),
            _ => None,
        }
    }

    /// The audio payload, mutably.
    pub fn as_pcm_mut(&mut self) -> Option<&mut PCMFloat1D> {
        match self {
            Self::PCMFloat1D(pcm) => Some(pcm),
            _ => None,
        }
    }

    /// The raster payload, if this is a `RasterTile2D`.
    #[must_use]
    pub fn as_raster(&self) -> Option<&RasterTile2D> {
        match self {
            Self::RasterTile2D(tile) => Some(tile),
            _ => None,
        }
    }

    /// The mesh payload, if this is a `MeshBuffer3D`.
    #[must_use]
    pub fn as_mesh(&self) -> Option<&MeshBuffer3D> {
        match self {
            Self::MeshBuffer3D(mesh) => Some(mesh),
            _ => None,
        }
    }

    /// The interval, if this is a `TimeSpan`.
    #[must_use]
    pub fn as_time_span(&self) -> Option<TimeSpan> {
        match self {
            Self::TimeSpan(span) => Some(*span),
            _ => None,
        }
    }
}

fn invalid(input: &Reader<'_>, error: &CoreError) -> CodecError {
    input.error(error.to_string())
}

impl ComponentData {
    /// Writes the payload. The seven built-in encodings are frozen: new data
    /// types are extension components, never new built-in kinds.
    pub fn encode(&self, out: &mut Writer) {
        match self {
            Self::StringBuffer(text) => out.str(text.as_str()),
            Self::PCMFloat1D(pcm) => {
                out.u32(pcm.sample_rate);
                out.varint(u64::from(pcm.channels));
                out.f32s(&pcm.samples);
            }
            Self::VectorSpline2D(spline) => {
                out.bool(spline.closed);
                out.f32s(spline.knots.as_flattened());
            }
            Self::RasterTile2D(tile) => {
                out.u32(tile.width);
                out.u32(tile.height);
                out.u8(match tile.format {
                    PixelFormat::Gray8 => 0,
                    PixelFormat::Rgba8 => 1,
                    PixelFormat::RgbaF32 => 2,
                });
                match &tile.pixels {
                    PixelStorage::U8(bytes) => out.bytes(bytes),
                    PixelStorage::F32(samples) => out.f32s(samples),
                }
            }
            Self::MeshBuffer3D(mesh) => {
                out.f32s(mesh.positions.as_flattened());
                out.len(mesh.indices.len());
                mesh.indices.iter().for_each(|index| out.u32(*index));
            }
            Self::TimeSpan(span) => {
                out.varint_signed(span.start);
                out.varint_signed(span.duration);
            }
            Self::ASTNode(node) => {
                out.str(&node.kind);
                out.varint(node.span.start as u64);
                out.varint(node.span.end as u64);
                out.len(node.children.len());
                node.children
                    .iter()
                    .for_each(|child| out.raw(&child.to_bytes()));
            }
        }
    }

    /// Reads a payload of `kind` written by [`Self::encode`], checking every
    /// invariant the constructors check.
    ///
    /// # Errors
    /// If the bytes are malformed or break an invariant.
    pub fn decode(kind: ComponentKind, input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let usize_of = |input: &mut Reader<'_>| -> Result<usize, CodecError> {
            let value = input.varint()?;
            usize::try_from(value).map_err(|_| input.error("offset overflows"))
        };
        Ok(match kind {
            ComponentKind::StringBuffer => Self::StringBuffer(StringBuffer::from(input.str()?)),
            ComponentKind::PCMFloat1D => {
                let rate = input.u32()?;
                let channels =
                    u16::try_from(input.varint()?).map_err(|_| input.error("too many channels"))?;
                let samples = input.f32s()?;
                let pcm = PCMFloat1D::new(rate, channels, samples);
                Self::PCMFloat1D(pcm.map_err(|error| invalid(input, &error))?)
            }
            ComponentKind::VectorSpline2D => {
                let closed = input.bool()?;
                let flat = input.f32s()?;
                if flat.len() % 2 != 0 {
                    return Err(input.error("odd spline coordinate count"));
                }
                let knots = flat.as_chunks::<2>().0.to_vec();
                let spline = VectorSpline2D::new(knots, closed);
                Self::VectorSpline2D(spline.map_err(|error| invalid(input, &error))?)
            }
            ComponentKind::RasterTile2D => {
                let (width, height) = (input.u32()?, input.u32()?);
                let tile = match input.u8()? {
                    0 => RasterTile2D::from_bytes(
                        width,
                        height,
                        PixelFormat::Gray8,
                        input.bytes()?.to_vec(),
                    ),
                    1 => RasterTile2D::from_bytes(
                        width,
                        height,
                        PixelFormat::Rgba8,
                        input.bytes()?.to_vec(),
                    ),
                    2 => RasterTile2D::from_f32(width, height, input.f32s()?),
                    other => return Err(input.error(format!("unknown pixel format {other}"))),
                };
                Self::RasterTile2D(tile.map_err(|error| invalid(input, &error))?)
            }
            ComponentKind::MeshBuffer3D => {
                let flat = input.f32s()?;
                if flat.len() % 3 != 0 {
                    return Err(input.error("mesh coordinate count is not a multiple of three"));
                }
                let positions = flat.as_chunks::<3>().0.to_vec();
                let count = input.len(4)?;
                let indices = (0..count).map(|_| input.u32()).collect::<Result<_, _>>()?;
                let mesh = MeshBuffer3D::new(positions, indices);
                Self::MeshBuffer3D(mesh.map_err(|error| invalid(input, &error))?)
            }
            ComponentKind::TimeSpan => {
                let (start, duration) = (input.varint_signed()?, input.varint_signed()?);
                let span = TimeSpan::new(start, duration);
                Self::TimeSpan(span.map_err(|error| invalid(input, &error))?)
            }
            ComponentKind::ASTNode => {
                let kind = input.str()?;
                let (start, end) = (usize_of(input)?, usize_of(input)?);
                let count = input.len(16)?;
                let mut children = Vec::with_capacity(count);
                for _ in 0..count {
                    let mut bytes = [0; 16];
                    bytes.copy_from_slice(input.raw(16)?);
                    children.push(EntityId::from_bytes(bytes));
                }
                let node =
                    ASTNode::new(kind, start..end).map_err(|error| invalid(input, &error))?;
                Self::ASTNode(node.with_children(children))
            }
        })
    }
}

macro_rules! component_from {
    ($($variant:ident),* $(,)?) => {
        $(
            impl From<$variant> for ComponentData {
                fn from(value: $variant) -> Self {
                    Self::$variant(value)
                }
            }
        )*
    };
}

component_from!(
    StringBuffer,
    PCMFloat1D,
    VectorSpline2D,
    RasterTile2D,
    MeshBuffer3D,
    TimeSpan,
    ASTNode,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_buffer_respects_char_boundaries() {
        let mut text = StringBuffer::from("héllo");
        assert_eq!(
            text.insert(2, "x"),
            Err(CoreError::NotCharBoundary { offset: 2 })
        );
        text.insert(0, "¡").unwrap();
        assert_eq!(text.as_str(), "¡héllo");
        assert_eq!(text.remove(0.."¡".len()).unwrap(), "¡");
        assert_eq!(
            text.remove(4..99),
            Err(CoreError::OutOfBounds { offset: 99, len: 6 })
        );
    }

    #[test]
    fn pcm_validates_layout() {
        assert!(PCMFloat1D::new(48_000, 2, vec![0.0; 3]).is_err());
        assert!(PCMFloat1D::new(0, 2, vec![]).is_err());
        let pcm = PCMFloat1D::silence(48_000, 2, 480).unwrap();
        assert_eq!(pcm.frames(), 480);
        assert!((pcm.duration_seconds() - 0.01).abs() < 1e-12);
    }

    #[test]
    fn spline_needs_3n_plus_1_knots_and_hits_endpoints() {
        assert!(VectorSpline2D::new(vec![[0.0, 0.0]; 3], false).is_err());
        let spline =
            VectorSpline2D::new(vec![[0.0, 0.0], [1.0, 2.0], [3.0, 2.0], [4.0, 0.0]], false)
                .unwrap();
        assert_eq!(spline.segment_count(), 1);
        assert_eq!(spline.point_at(0, 0.0), Some([0.0, 0.0]));
        assert_eq!(spline.point_at(0, 1.0), Some([4.0, 0.0]));
        assert_eq!(spline.point_at(0, 0.5), Some([2.0, 1.5]));
        assert_eq!(spline.point_at(1, 0.5), None);
    }

    #[test]
    fn raster_sizes_are_checked() {
        assert!(RasterTile2D::new(0, 4, PixelFormat::Gray8).is_err());
        assert!(RasterTile2D::from_bytes(2, 2, PixelFormat::Rgba8, vec![0; 15]).is_err());
        let tile = RasterTile2D::from_f32(2, 1, vec![0.5; 8]).unwrap();
        assert_eq!(tile.byte_len(), 32);
        assert_eq!(
            ComponentData::from(tile).f32_view().map(<[f32]>::len),
            Some(8)
        );
    }

    #[test]
    fn mesh_indices_are_checked() {
        assert!(MeshBuffer3D::new(vec![[0.0; 3]; 3], vec![0, 1]).is_err());
        assert!(MeshBuffer3D::new(vec![[0.0; 3]; 3], vec![0, 1, 3]).is_err());
        let quad = MeshBuffer3D::quad(2.0, 1.0);
        assert_eq!((quad.vertex_count(), quad.triangle_count()), (4, 2));
        let data = ComponentData::from(quad);
        assert_eq!(data.f32_view().map(<[f32]>::len), Some(12));
    }

    #[test]
    fn flicks_divide_common_rates() {
        for rate in [24, 25, 30, 48, 50, 60, 90, 100, 120] {
            assert_eq!(FLICKS_PER_SECOND % rate, 0, "{rate} fps");
        }
        for rate in [
            8_000, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 192_000,
        ] {
            assert_eq!(FLICKS_PER_SECOND % rate, 0, "{rate} Hz");
        }
    }

    #[test]
    fn time_span_bounds() {
        let span = TimeSpan::from_seconds(1.0, 0.5).unwrap();
        assert_eq!(span.start(), FLICKS_PER_SECOND);
        assert!(span.contains(span.start()));
        assert!(!span.contains(span.end()));
        assert!(TimeSpan::new(0, -1).is_err());
        assert!(TimeSpan::new(i64::MAX, 1).is_err());
    }

    #[test]
    fn ast_nodes_link_children() {
        let node = ASTNode::new("fn_item", 0..12)
            .unwrap()
            .with_children([EntityId::from_u128(3)]);
        assert_eq!(node.children(), &[EntityId::from_u128(3)]);
        let (start, end) = (5, 1);
        assert!(ASTNode::new("bad", start..end).is_err());
    }

    #[test]
    fn every_built_in_component_round_trips() {
        let components: Vec<ComponentData> = vec![
            StringBuffer::from("héllo").into(),
            PCMFloat1D::new(44_100, 2, vec![0.5, -0.5, 0.25, 1.0])
                .unwrap()
                .into(),
            VectorSpline2D::new(vec![[0.0, 0.0], [1.0, 2.0], [3.0, 2.0], [4.0, 0.0]], true)
                .unwrap()
                .into(),
            RasterTile2D::from_bytes(2, 1, PixelFormat::Rgba8, vec![1, 2, 3, 4, 5, 6, 7, 8])
                .unwrap()
                .into(),
            RasterTile2D::from_f32(1, 1, vec![0.1, 0.2, 0.3, 1.0])
                .unwrap()
                .into(),
            MeshBuffer3D::quad(2.0, 1.0).into(),
            TimeSpan::new(-5, 10).unwrap().into(),
            ASTNode::new("fn_item", 2..9)
                .unwrap()
                .with_children([EntityId::new()])
                .into(),
        ];
        for component in components {
            let mut out = Writer::new();
            component.encode(&mut out);
            let bytes = out.into_bytes();
            let mut input = Reader::new(&bytes);
            assert_eq!(
                ComponentData::decode(component.kind(), &mut input).unwrap(),
                component
            );
            assert!(input.is_empty());
        }
        let mut out = Writer::new();
        out.u32(0);
        out.varint(1);
        out.f32s(&[]);
        let bytes = out.into_bytes();
        assert!(
            ComponentData::decode(ComponentKind::PCMFloat1D, &mut Reader::new(&bytes)).is_err()
        );
        assert_eq!(
            ComponentKind::from_name("RasterTile2D"),
            Some(ComponentKind::RasterTile2D)
        );
    }

    #[test]
    fn kinds_match_variants() {
        let data = ComponentData::from(StringBuffer::from("x"));
        assert_eq!(data.kind(), ComponentKind::StringBuffer);
        assert!(data.f32_view().is_none());
        assert_eq!(ComponentKind::ALL.len(), 7);
    }
}
