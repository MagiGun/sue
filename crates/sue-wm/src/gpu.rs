//! The `wgpu` batch renderer: turns a [`Canvas`] into instanced quads (SDF
//! rounded boxes, atlas glyphs, arena images) with per-instance clipping.

use std::collections::HashMap;

use sue_core::{Arena, AssetHandle, ComponentKind, PixelFormat, PixelStorage, RasterTile2D};
use sue_platform::wgpu;

use crate::text::{FontId, Fonts};
use crate::{Canvas, Color, DrawCommand, Rect};

const SHADER: &str = include_str!("ui.wgsl");
const FLOATS_PER_INSTANCE: usize = 24;
const INSTANCE_BYTES: u64 = (FLOATS_PER_INSTANCE * size_of::<f32>()) as u64;
const ATLAS_SIZE: u32 = 2048;
const KIND_BOX: f32 = 0.0;
const KIND_GLYPH: f32 = 1.0;
const KIND_RGBA: f32 = 2.0;
const KIND_GRAY: f32 = 3.0;

/// Size of the frame being drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameTarget {
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
    /// Physical pixels per logical pixel.
    pub scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct GlyphKey {
    font: FontId,
    glyph: u16,
    quarter_px: u32,
}

#[derive(Clone, Copy, Debug)]
struct AtlasGlyph {
    uv: [f32; 4],
    width: f32,
    height: f32,
    xmin: f32,
    ymin: f32,
}

/// Packs rectangles into rows ("shelves") of a square texture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ShelfPacker {
    x: u32,
    y: u32,
    shelf_height: u32,
}

impl ShelfPacker {
    /// Reserves `width × height` (plus a 1 px gutter); `None` when full.
    fn allocate(&mut self, width: u32, height: u32, size: u32) -> Option<(u32, u32)> {
        let (padded_width, padded_height) = (width + 1, height + 1);
        if padded_width > size {
            return None;
        }
        if self.x + padded_width > size {
            self.y += self.shelf_height;
            self.x = 0;
            self.shelf_height = 0;
        }
        if self.y + padded_height > size {
            return None;
        }
        let origin = (self.x, self.y);
        self.x += padded_width;
        self.shelf_height = self.shelf_height.max(padded_height);
        Some(origin)
    }
}

struct Atlas {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    glyphs: HashMap<GlyphKey, AtlasGlyph>,
    packer: ShelfPacker,
}

impl Atlas {
    fn clear(&mut self) {
        self.glyphs.clear();
        self.packer = ShelfPacker::default();
    }
}

struct GpuImage {
    view: wgpu::TextureView,
    gray: bool,
    bind_groups: [Option<wgpu::BindGroup>; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Binding {
    Atlas,
    Image(AssetHandle, bool),
}

struct Batch {
    binding: Binding,
    start: u32,
    end: u32,
}

struct PendingGlyph {
    origin: (u32, u32),
    width: u32,
    height: u32,
    coverage: Vec<u8>,
}

/// Draws [`Canvas`] frames with one instanced pipeline.
pub struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    samplers: [wgpu::Sampler; 2],
    atlas: Atlas,
    images: HashMap<AssetHandle, GpuImage>,
    instances: wgpu::Buffer,
    instance_capacity: u64,
    encode_srgb: bool,
    max_texture: u32,
}

impl std::fmt::Debug for UiRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiRenderer")
            .field("glyphs", &self.atlas.glyphs.len())
            .field("images", &self.images.len())
            .finish_non_exhaustive()
    }
}

impl UiRenderer {
    /// A renderer drawing into targets of `format`.
    #[must_use]
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sue-ui"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sue-ui-globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sue-ui-texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sue-ui"),
            bind_group_layouts: &[Some(&globals_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sue-ui"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: INSTANCE_BYTES,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sue-ui-globals"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sue-ui-globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let sampler = |filter| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("sue-ui"),
                mag_filter: filter,
                min_filter: filter,
                ..wgpu::SamplerDescriptor::default()
            })
        };
        let samplers = [
            sampler(wgpu::FilterMode::Nearest),
            sampler(wgpu::FilterMode::Linear),
        ];
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sue-glyph-atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas_bind_group =
            texture_bind_group(device, &texture_layout, &atlas_view, &samplers[0]);
        let instance_capacity = 1024;
        Self {
            pipeline,
            globals,
            globals_bind_group,
            texture_layout,
            atlas: Atlas {
                texture: atlas_texture,
                bind_group: atlas_bind_group,
                glyphs: HashMap::new(),
                packer: ShelfPacker::default(),
            },
            images: HashMap::new(),
            instances: instance_buffer(device, instance_capacity),
            instance_capacity,
            encode_srgb: !format.is_srgb(),
            max_texture: device.limits().max_texture_dimension_2d,
            samplers,
        }
    }

    /// Draws `canvas` into `target`, clearing it to `clear` first.
    #[allow(
        clippy::too_many_arguments,
        reason = "a frame needs the GPU, its target, and every source"
    )]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        frame: FrameTarget,
        canvas: &Canvas,
        fonts: &Fonts,
        arena: &Arena,
        clear: Color,
    ) {
        self.images.retain(|handle, _| arena.contains(*handle));
        let mut built = self.build(device, queue, frame, canvas, fonts, arena);
        if built.is_none() {
            // The glyph atlas filled up: start it over and lay the frame out again.
            self.atlas.clear();
            built = self.build(device, queue, frame, canvas, fonts, arena);
        }
        let (data, batches, pending) = built.unwrap_or_default();
        for glyph in pending {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.atlas.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: glyph.origin.0,
                        y: glyph.origin.1,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &glyph.coverage,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(glyph.width),
                    rows_per_image: Some(glyph.height),
                },
                wgpu::Extent3d {
                    width: glyph.width,
                    height: glyph.height,
                    depth_or_array_layers: 1,
                },
            );
        }
        let needed = (data.len() / FLOATS_PER_INSTANCE) as u64;
        if needed > self.instance_capacity {
            self.instance_capacity = needed.next_power_of_two();
            self.instances = instance_buffer(device, self.instance_capacity);
        }
        if !data.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&data));
        }
        let globals = [
            frame.width as f32,
            frame.height as f32,
            if self.encode_srgb { 1.0 } else { 0.0 },
            0.0,
        ];
        queue.write_buffer(&self.globals, 0, bytemuck::cast_slice(&globals));

        let clear = if self.encode_srgb {
            to_srgb(clear)
        } else {
            clear
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("sue-ui"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sue-ui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(clear.r),
                            g: f64::from(clear.g),
                            b: f64::from(clear.b),
                            a: f64::from(clear.a),
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.globals_bind_group, &[]);
            pass.set_vertex_buffer(0, self.instances.slice(..));
            for batch in &batches {
                let bind_group = match batch.binding {
                    Binding::Atlas => Some(&self.atlas.bind_group),
                    Binding::Image(handle, smooth) => self
                        .images
                        .get(&handle)
                        .and_then(|image| image.bind_groups[usize::from(smooth)].as_ref()),
                };
                if let Some(bind_group) = bind_group {
                    pass.set_bind_group(1, bind_group, &[]);
                    pass.draw(0..4, batch.start..batch.end);
                }
            }
        }
        queue.submit([encoder.finish()]);
    }

    /// Lays out every command into instance data. `None` if the glyph atlas ran
    /// out of room.
    fn build(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: FrameTarget,
        canvas: &Canvas,
        fonts: &Fonts,
        arena: &Arena,
    ) -> Option<(Vec<f32>, Vec<Batch>, Vec<PendingGlyph>)> {
        let scale = frame.scale;
        let mut data: Vec<f32> = Vec::with_capacity(canvas.len() * FLOATS_PER_INSTANCE);
        let mut batches: Vec<Batch> = Vec::new();
        let mut pending = Vec::new();
        let push = |data: &mut Vec<f32>,
                    batches: &mut Vec<Batch>,
                    binding: Option<Binding>,
                    instance: [f32; FLOATS_PER_INSTANCE]| {
            let index = (data.len() / FLOATS_PER_INSTANCE) as u32;
            data.extend_from_slice(&instance);
            match (batches.last_mut(), binding) {
                (Some(batch), None) => batch.end = index + 1,
                (Some(batch), Some(binding)) if batch.binding == binding => batch.end = index + 1,
                (_, binding) => batches.push(Batch {
                    binding: binding.unwrap_or(Binding::Atlas),
                    start: index,
                    end: index + 1,
                }),
            }
        };
        for command in canvas.commands() {
            match command {
                DrawCommand::Box { rect, style, clip } => {
                    let instance = instance(
                        scaled(*rect, scale),
                        [0.0; 4],
                        style.fill,
                        style.border,
                        [
                            style.radius * scale,
                            style.border_width * scale,
                            KIND_BOX,
                            0.0,
                        ],
                        scaled(*clip, scale),
                    );
                    push(&mut data, &mut batches, None, instance);
                }
                DrawCommand::Text {
                    position,
                    text,
                    face,
                    size,
                    color,
                    clip,
                } => {
                    let px = (size * scale).max(1.0);
                    let baseline = (position.y * scale + fonts.ascent(*face, px)).round();
                    let origin_x = position.x * scale;
                    let clip = scaled(*clip, scale);
                    for placed in fonts.layout(*face, px, text) {
                        let key = GlyphKey {
                            font: placed.font,
                            glyph: placed.glyph,
                            quarter_px: (px * 4.0).round() as u32,
                        };
                        let glyph = match self.atlas.glyphs.get(&key) {
                            Some(glyph) => *glyph,
                            None => {
                                let (metrics, coverage) =
                                    fonts.rasterize(placed.font, placed.glyph, px);
                                let (width, height) = (metrics.width as u32, metrics.height as u32);
                                let origin = if width == 0 || height == 0 {
                                    (0, 0)
                                } else {
                                    self.atlas.packer.allocate(width, height, ATLAS_SIZE)?
                                };
                                let atlas_size = ATLAS_SIZE as f32;
                                let glyph = AtlasGlyph {
                                    uv: [
                                        origin.0 as f32 / atlas_size,
                                        origin.1 as f32 / atlas_size,
                                        (origin.0 + width) as f32 / atlas_size,
                                        (origin.1 + height) as f32 / atlas_size,
                                    ],
                                    width: width as f32,
                                    height: height as f32,
                                    xmin: metrics.xmin as f32,
                                    ymin: metrics.ymin as f32,
                                };
                                if width > 0 && height > 0 {
                                    pending.push(PendingGlyph {
                                        origin,
                                        width,
                                        height,
                                        coverage,
                                    });
                                }
                                self.atlas.glyphs.insert(key, glyph);
                                glyph
                            }
                        };
                        if glyph.width <= 0.0 {
                            continue;
                        }
                        let x = (origin_x + placed.x + glyph.xmin).round();
                        let y = baseline - glyph.ymin - glyph.height;
                        let instance = instance(
                            Rect::new(x, y, glyph.width, glyph.height),
                            glyph.uv,
                            *color,
                            Color::TRANSPARENT,
                            [0.0, 0.0, KIND_GLYPH, 0.0],
                            clip,
                        );
                        push(&mut data, &mut batches, Some(Binding::Atlas), instance);
                    }
                }
                DrawCommand::Image {
                    rect,
                    image,
                    smooth,
                    clip,
                } => {
                    let Some(gray) = self.ensure_image(device, queue, arena, *image, *smooth)
                    else {
                        continue;
                    };
                    let kind = if gray { KIND_GRAY } else { KIND_RGBA };
                    let white = Color {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: 1.0,
                    };
                    let instance = instance(
                        scaled(*rect, scale),
                        [0.0, 0.0, 1.0, 1.0],
                        white,
                        Color::TRANSPARENT,
                        [0.0, 0.0, kind, 0.0],
                        scaled(*clip, scale),
                    );
                    push(
                        &mut data,
                        &mut batches,
                        Some(Binding::Image(*image, *smooth)),
                        instance,
                    );
                }
            }
        }
        Some((data, batches, pending))
    }

    /// Uploads the raster of `handle` once. Returns `Some(gray)` when drawable.
    fn ensure_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        arena: &Arena,
        handle: AssetHandle,
        smooth: bool,
    ) -> Option<bool> {
        if !self.images.contains_key(&handle) {
            let tile = arena
                .get(handle)
                .ok()?
                .component(ComponentKind::RasterTile2D)?
                .as_raster()?;
            let (width, height, gray, pixels) = texture_pixels(tile, self.max_texture)?;
            let format = if gray {
                wgpu::TextureFormat::R8Unorm
            } else {
                wgpu::TextureFormat::Rgba8UnormSrgb
            };
            let size = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("sue-image"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * if gray { 1 } else { 4 }),
                    rows_per_image: Some(height),
                },
                size,
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.images.insert(
                handle,
                GpuImage {
                    view,
                    gray,
                    bind_groups: [None, None],
                },
            );
        }
        let image = self.images.get_mut(&handle)?;
        let slot = usize::from(smooth);
        if image.bind_groups[slot].is_none() {
            image.bind_groups[slot] = Some(texture_bind_group(
                device,
                &self.texture_layout,
                &image.view,
                &self.samplers[slot],
            ));
        }
        Some(image.gray)
    }
}

fn texture_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sue-ui-texture"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn instance_buffer(device: &wgpu::Device, capacity: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sue-ui-instances"),
        size: capacity * INSTANCE_BYTES,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn scaled(rect: Rect, scale: f32) -> Rect {
    Rect::new(
        rect.x * scale,
        rect.y * scale,
        rect.width * scale,
        rect.height * scale,
    )
}

fn instance(
    rect: Rect,
    uv: [f32; 4],
    color: Color,
    border: Color,
    params: [f32; 4],
    clip: Rect,
) -> [f32; FLOATS_PER_INSTANCE] {
    [
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        uv[0],
        uv[1],
        uv[2],
        uv[3],
        color.r,
        color.g,
        color.b,
        color.a,
        border.r,
        border.g,
        border.b,
        border.a,
        params[0],
        params[1],
        params[2],
        params[3],
        clip.x,
        clip.y,
        clip.right(),
        clip.bottom(),
    ]
}

fn to_srgb(color: Color) -> Color {
    let encode = |c: f32| {
        if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    };
    Color {
        r: encode(color.r),
        g: encode(color.g),
        b: encode(color.b),
        a: color.a,
    }
}

/// Pixels ready for upload, downscaled to fit `max_dimension`:
/// `(width, height, gray, bytes)`.
fn texture_pixels(tile: &RasterTile2D, max_dimension: u32) -> Option<(u32, u32, bool, Vec<u8>)> {
    let factor = tile
        .width()
        .max(tile.height())
        .div_ceil(max_dimension.max(1))
        .max(1);
    let (width, height) = (
        tile.width().div_ceil(factor),
        tile.height().div_ceil(factor),
    );
    let channels = match tile.format() {
        PixelFormat::Gray8 => 1,
        PixelFormat::Rgba8 | PixelFormat::RgbaF32 => 4,
    };
    let source: std::borrow::Cow<'_, [u8]> = match tile.pixels() {
        PixelStorage::U8(bytes) => bytes.as_slice().into(),
        PixelStorage::F32(samples) => samples
            .iter()
            .map(|&sample| (sample.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect::<Vec<u8>>()
            .into(),
    };
    if factor == 1 {
        return Some((width, height, channels == 1, source.into_owned()));
    }
    let mut out = Vec::with_capacity(width as usize * height as usize * channels);
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0_u32; 4];
            let mut count = 0_u32;
            for sy in (y * factor)..((y + 1) * factor).min(tile.height()) {
                for sx in (x * factor)..((x + 1) * factor).min(tile.width()) {
                    let offset = (sy as usize * tile.width() as usize + sx as usize) * channels;
                    for (total, &value) in
                        sum.iter_mut().zip(source.get(offset..offset + channels)?)
                    {
                        *total += u32::from(value);
                    }
                    count += 1;
                }
            }
            out.extend(
                sum[..channels]
                    .iter()
                    .map(|total| (total / count.max(1)) as u8),
            );
        }
    }
    Some((width, height, channels == 1, out))
}

/// Renders `canvas` offscreen at `frame` size and returns tightly packed
/// sRGB RGBA8 pixels (for snapshots and tests).
///
/// # Errors
/// If the GPU readback fails.
pub fn render_offscreen(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    frame: FrameTarget,
    canvas: &Canvas,
    fonts: &Fonts,
    arena: &Arena,
    clear: Color,
) -> Result<Vec<u8>, String> {
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = UiRenderer::new(device, format);
    let size = wgpu::Extent3d {
        width: frame.width,
        height: frame.height,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sue-offscreen"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    renderer.render(device, queue, &view, frame, canvas, fonts, arena, clear);

    let unpadded = frame.width * 4;
    let padded =
        unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sue-readback"),
        size: u64::from(padded) * u64::from(frame.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("sue-readback"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(frame.height),
            },
        },
        size,
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| error.to_string())?;
    receiver
        .recv()
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    let mapped = readback
        .get_mapped_range(..)
        .map_err(|error| error.to_string())?;
    let mut pixels = Vec::with_capacity((unpadded * frame.height) as usize);
    for row in mapped.chunks(padded as usize) {
        pixels.extend_from_slice(&row[..unpadded as usize]);
    }
    drop(mapped);
    readback.unmap();
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BoxStyle, FontFace};
    use sue_platform::{GpuContext, Point};
    use sue_rice::Rgba;

    #[test]
    fn oversized_rasters_are_downscaled_for_upload() {
        let tile = RasterTile2D::from_bytes(10, 4, PixelFormat::Gray8, (0..40).collect()).unwrap();
        let (width, height, gray, pixels) = texture_pixels(&tile, 5).unwrap();
        assert_eq!((width, height, gray), (5, 2, true));
        assert_eq!(pixels.len(), 10);
        let (w, h, _, same) = texture_pixels(&tile, 4096).unwrap();
        assert_eq!((w, h), (10, 4));
        assert_eq!(same, (0..40).collect::<Vec<u8>>());
    }

    #[test]
    fn atlas_packs_shelves_and_reports_when_full() {
        let mut packer = ShelfPacker::default();
        assert_eq!(packer.allocate(10, 10, 32), Some((0, 0)));
        assert_eq!(packer.allocate(10, 12, 32), Some((11, 0)));
        assert_eq!(
            packer.allocate(20, 5, 32),
            Some((0, 13)),
            "a new shelf starts below the tallest"
        );
        assert_eq!(packer.allocate(40, 1, 32), None, "wider than the atlas");
        assert_eq!(packer.allocate(10, 20, 32), None, "no rows left");
    }

    #[test]
    fn images_render_texel_for_texel() {
        let Ok(gpu) = GpuContext::headless() else {
            eprintln!("no GPU adapter available; skipping the image fidelity test");
            return;
        };
        let fonts = Fonts::new().unwrap();
        let mut arena = Arena::new();
        let ramp: Vec<u8> = (0..=255_u8)
            .flat_map(|v| [v, 255 - v, v / 2, 255])
            .collect();
        let image = arena.spawn(
            "ramp",
            RasterTile2D::from_bytes(256, 1, PixelFormat::Rgba8, ramp).unwrap(),
        );
        let mut canvas = Canvas::new(Rect::new(0.0, 0.0, 256.0, 4.0));
        canvas.draw_image(Rect::new(0.0, 0.0, 256.0, 4.0), image, false);
        let frame = FrameTarget {
            width: 256,
            height: 4,
            scale: 1.0,
        };
        let pixels = render_offscreen(
            &gpu.device,
            &gpu.queue,
            frame,
            &canvas,
            &fonts,
            &arena,
            Color::TRANSPARENT,
        )
        .unwrap();
        for x in 0..256_usize {
            let pixel = &pixels[(256 + x) * 4..(256 + x) * 4 + 4];
            let expected = [x as u8, 255 - x as u8, (x / 2) as u8, 255];
            let close = pixel
                .iter()
                .zip(expected)
                .all(|(&got, want)| got.abs_diff(want) <= 1);
            assert!(
                close,
                "texel {x}: rendered {pixel:?}, expected {expected:?}"
            );
        }
    }

    #[test]
    fn renders_boxes_text_and_images_offscreen() {
        let Ok(gpu) = GpuContext::headless() else {
            eprintln!("no GPU adapter available; skipping the offscreen render test");
            return;
        };
        let fonts = Fonts::new().unwrap();
        let mut arena = Arena::new();
        let image = arena.spawn(
            "red",
            RasterTile2D::from_bytes(2, 2, PixelFormat::Rgba8, [255, 0, 0, 255].repeat(4)).unwrap(),
        );
        let mut canvas = Canvas::new(Rect::new(0.0, 0.0, 64.0, 32.0));
        let green = Color::from_rgba(Rgba::new(0, 255, 0, 255));
        canvas.draw_box(
            Rect::new(0.0, 0.0, 16.0, 16.0),
            BoxStyle::filled(green, 0.0),
        );
        canvas.draw_image(Rect::new(16.0, 0.0, 16.0, 16.0), image, false);
        canvas.draw_text(Point::new(34.0, 2.0), "Sue", FontFace::Ui, 14.0, green);
        let frame = FrameTarget {
            width: 64,
            height: 32,
            scale: 1.0,
        };
        let black = Color::from_rgba(Rgba::new(0, 0, 0, 255));
        let pixels = render_offscreen(
            &gpu.device,
            &gpu.queue,
            frame,
            &canvas,
            &fonts,
            &arena,
            black,
        )
        .unwrap();
        let pixel = |x: usize, y: usize| &pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        assert_eq!(pixel(8, 8), [0, 255, 0, 255], "box");
        assert_eq!(pixel(24, 8), [255, 0, 0, 255], "image");
        assert_eq!(pixel(8, 28), [0, 0, 0, 255], "clear color");
        let text_pixels = (34..64)
            .flat_map(|x| (0..20).map(move |y| (x, y)))
            .filter(|&(x, y)| pixel(x, y)[1] > 128)
            .count();
        assert!(
            text_pixels > 10,
            "text was rasterized ({text_pixels} lit pixels)"
        );
    }
}
