//! The egui painter: draws Sue's interface with `egui-wgpu` into the window
//! swap chain or an offscreen texture, and uploads the images it shows.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::GpuContext;

/// Hands borrowed pixels to the GPU so egui can draw them, without copying
/// media buffers between modules: the arena's bytes go straight into a
/// texture upload (backbone rule 5).
pub trait TextureUpload {
    /// A texture showing `pixels`: `size[0]` by `size[1]` pixels of
    /// `channels` bytes each (1 for gray, 4 for sRGB RGBA), row-major. `key`
    /// names the image; it is uploaded again only when `version` changes.
    /// `nearest` keeps pixels crisp when magnified. `None` without a GPU.
    fn texture(
        &mut self,
        key: u64,
        version: u64,
        size: [u32; 2],
        channels: u8,
        pixels: &[u8],
        nearest: bool,
    ) -> Option<egui::TextureId>;

    /// [`Self::texture`] for an image that changes a little at a time (a
    /// canvas being painted): when the texture already holds an older
    /// version, `changed` is asked for the rectangles `[x0, y0, x1, y1]`
    /// that differ since that version and only they are sent again
    /// (`None` from it sends everything). Hosts without partial uploads
    /// send everything.
    #[allow(clippy::too_many_arguments)]
    fn texture_changed(
        &mut self,
        key: u64,
        version: u64,
        size: [u32; 2],
        channels: u8,
        pixels: &[u8],
        nearest: bool,
        changed: &dyn Fn(u64) -> Option<Vec<[u32; 4]>>,
    ) -> Option<egui::TextureId> {
        let _ = changed;
        self.texture(key, version, size, channels, pixels, nearest)
    }
}

/// For hosts without a GPU: images draw as placeholders.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoTextures;

impl TextureUpload for NoTextures {
    fn texture(
        &mut self,
        _key: u64,
        _version: u64,
        _size: [u32; 2],
        _channels: u8,
        _pixels: &[u8],
        _nearest: bool,
    ) -> Option<egui::TextureId> {
        None
    }
}

/// The format the images are held in. Not an sRGB one: egui's shader takes
/// what it samples as sRGB and converts it itself, so a picture in an sRGB
/// texture came out too dark in the middle tones.
const IMAGE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// [`IMAGE_FORMAT`] seen as sRGB, for the mip blits only. They then average
/// in light instead of in the encoded bytes, which is what a zoomed out
/// picture wants: averaged in bytes, fine black lines over white come out
/// much darker than the eye reads them.
///
/// A view in another format needs the device to allow view formats, which the
/// GL backends (and so WebGL) do not: there the levels are blitted as
/// [`IMAGE_FORMAT`] and average in bytes instead. The chain, which is what
/// keeps a zoomed out picture smooth, is the same either way.
const IMAGE_FORMAT_SRGB: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// How many levels a texture of `size` takes, the base one included.
fn mip_levels(size: [u32; 2]) -> u32 {
    32 - size[0].max(size[1]).max(1).leading_zeros()
}

/// Halves a texture one level at a time.
///
/// The GPU samples an image through the sampler we hand egui, so a picture
/// drawn smaller than it is would take a plain bilinear tap that skips most
/// of the pixels it covers: thin lines came out ragged, of a weight that
/// wandered along them. With a chain to sample instead, the GPU picks the
/// level that fits the screen and the picture stays smooth at every zoom.
const MIP_SHADER: &str = r#"
struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> VertexOut {
    // One triangle over the whole target, its corners carrying the uv.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VertexOut;
    out.position = vec4<f32>(corner * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = corner;
    return out;
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    return textureSampleLevel(source, source_sampler, in.uv, 0.0);
}
"#;

/// The pipeline that blits each mip level out of the one above it.
struct Mips {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    /// Whether the levels can be blitted as sRGB views, and so average in
    /// light. False where the device has no view formats (GL, WebGL).
    srgb_views: bool,
}

impl Mips {
    fn new(device: &wgpu::Device, srgb_views: bool) -> Self {
        let target = if srgb_views {
            IMAGE_FORMAT_SRGB
        } else {
            IMAGE_FORMAT
        };
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sue-mips"),
            source: wgpu::ShaderSource::Wgsl(MIP_SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sue-mips"),
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sue-mips"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sue-mips"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sue-mips"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        Self {
            layout,
            pipeline,
            sampler,
            srgb_views,
        }
    }

    /// The commands that fill every level of `texture` below the base one.
    ///
    /// The whole chain is blitted again on every upload, so a stroke on a
    /// huge canvas pays for levels it is not showing. Blitting only the
    /// levels a change reaches is the upgrade if that ever measures.
    fn blit(
        &self,
        device: &wgpu::Device,
        texture: &wgpu::Texture,
        levels: u32,
    ) -> wgpu::CommandBuffer {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("sue-mips"),
        });
        for level in 1..levels {
            let view = |label: &'static str, level: u32| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some(label),
                    format: self.srgb_views.then_some(IMAGE_FORMAT_SRGB),
                    base_mip_level: level,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            };
            let from = view("sue-mip-from", level - 1);
            let to = view("sue-mip-to", level);
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sue-mips"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&from),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sue-mips"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &to,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.finish()
    }
}

struct NativeImage {
    version: u64,
    size: [u32; 2],
    nearest: bool,
    levels: u32,
    texture: wgpu::Texture,
    id: egui::TextureId,
    used: bool,
}

/// RGBA pixels for an upload of at most `limit` pixels a side: the borrowed
/// slice itself when it already fits, else a gray expansion or a nearest
/// downscale made only for the GPU.
fn upload_pixels(
    size: [u32; 2],
    channels: u8,
    pixels: &[u8],
    limit: u32,
) -> Option<([u32; 2], Cow<'_, [u8]>)> {
    let [width, height] = size;
    let stride = usize::from(channels);
    if width == 0
        || height == 0
        || !matches!(channels, 1 | 4)
        || pixels.len() < width as usize * height as usize * stride
    {
        return None;
    }
    let step = width.max(height).div_ceil(limit.max(1)).max(1);
    if step == 1 && channels == 4 {
        let len = width as usize * height as usize * 4;
        return Some((size, Cow::Borrowed(&pixels[..len])));
    }
    let (out_w, out_h) = (width.div_ceil(step), height.div_ceil(step));
    let mut out = Vec::with_capacity(out_w as usize * out_h as usize * 4);
    for y in 0..out_h {
        for x in 0..out_w {
            let at = ((y * step) as usize * width as usize + (x * step) as usize) * stride;
            if channels == 1 {
                let value = pixels[at];
                out.extend_from_slice(&[value, value, value, 255]);
            } else {
                out.extend_from_slice(&pixels[at..at + 4]);
            }
        }
    }
    Some(([out_w, out_h], Cow::Owned(out)))
}

/// Paints egui frames with `egui-wgpu`, and keeps the textures of the images
/// the interface shows.
pub struct EguiPainter {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: egui_wgpu::Renderer,
    images: HashMap<u64, NativeImage>,
    mips: Mips,
    max_side: u32,
}

impl std::fmt::Debug for EguiPainter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EguiPainter")
            .field("images", &self.images.len())
            .finish_non_exhaustive()
    }
}

impl EguiPainter {
    /// A painter for targets of `format`.
    #[must_use]
    pub fn new(gpu: &GpuContext, format: wgpu::TextureFormat) -> Self {
        Self {
            device: gpu.device.clone(),
            queue: gpu.queue.clone(),
            renderer: egui_wgpu::Renderer::new(
                &gpu.device,
                format,
                egui_wgpu::RendererOptions::default(),
            ),
            images: HashMap::new(),
            mips: Mips::new(
                &gpu.device,
                gpu.adapter
                    .get_downlevel_capabilities()
                    .flags
                    .contains(wgpu::DownlevelFlags::VIEW_FORMATS),
            ),
            max_side: gpu.device.limits().max_texture_dimension_2d,
        }
    }

    /// The largest texture side the device takes.
    #[must_use]
    pub fn max_texture_side(&self) -> usize {
        self.max_side as usize
    }

    /// Draws one egui frame into `target` (`size` physical pixels), cleared
    /// to `clear` first, then frees the images the frame did not use.
    #[allow(
        clippy::too_many_arguments,
        reason = "one frame's parts, as egui hands them over"
    )]
    pub fn paint(
        &mut self,
        ctx: &egui::Context,
        target: &wgpu::TextureView,
        size: [u32; 2],
        clear: egui::Color32,
        shapes: Vec<egui::epaint::ClippedShape>,
        mut textures: egui::TexturesDelta,
        pixels_per_point: f32,
    ) {
        let primitives = ctx.tessellate(shapes, pixels_per_point);
        for (id, deltas) in &textures.set {
            for delta in deltas {
                self.renderer
                    .update_texture(&self.device, &self.queue, *id, delta);
            }
        }
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: size,
            pixels_per_point,
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("sue") });
        let mut commands = self.renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &primitives,
            &screen,
        );
        {
            let linear = egui::Rgba::from(clear);
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("sue"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: f64::from(linear.r()),
                                g: f64::from(linear.g()),
                                b: f64::from(linear.b()),
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &primitives, &screen);
        }
        commands.push(encoder.finish());
        self.queue.submit(commands);
        for id in &textures.free {
            self.renderer.free_texture(id);
        }
        textures.clear();
        let renderer = &mut self.renderer;
        self.images.retain(|_, image| {
            let keep = image.used;
            image.used = false;
            if !keep {
                renderer.free_texture(&image.id);
            }
            keep
        });
    }

    /// Applies egui's texture changes without drawing (a skipped frame).
    pub fn apply_textures(&mut self, mut textures: egui::TexturesDelta) {
        for (id, deltas) in &textures.set {
            for delta in deltas {
                self.renderer
                    .update_texture(&self.device, &self.queue, *id, delta);
            }
        }
        for id in &textures.free {
            self.renderer.free_texture(id);
        }
        textures.clear();
    }

    /// Renders one egui frame offscreen and reads it back as sRGB RGBA bytes.
    ///
    /// # Errors
    /// GPU mapping failures.
    pub fn paint_offscreen(
        &mut self,
        ctx: &egui::Context,
        size: [u32; 2],
        clear: egui::Color32,
        shapes: Vec<egui::epaint::ClippedShape>,
        textures: egui::TexturesDelta,
        pixels_per_point: f32,
    ) -> Result<Vec<u8>, String> {
        let [width, height] = size;
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sue-offscreen"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.paint(ctx, &view, size, clear, shapes, textures, pixels_per_point);
        let unpadded = width * 4;
        let padded = unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sue-readback"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                    rows_per_image: Some(height),
                },
            },
            extent,
        );
        self.queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| error.to_string())?;
        receiver
            .recv()
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        let mapped = readback
            .get_mapped_range(..)
            .map_err(|error| error.to_string())?;
        let mut pixels = Vec::with_capacity((unpadded * height) as usize);
        for row in mapped.chunks(padded as usize) {
            pixels.extend_from_slice(&row[..unpadded as usize]);
        }
        drop(mapped);
        readback.unmap();
        Ok(pixels)
    }
}

impl TextureUpload for EguiPainter {
    fn texture_changed(
        &mut self,
        key: u64,
        version: u64,
        size: [u32; 2],
        channels: u8,
        pixels: &[u8],
        nearest: bool,
        changed: &dyn Fn(u64) -> Option<Vec<[u32; 4]>>,
    ) -> Option<egui::TextureId> {
        // Only the changed rectangles go up when the texture holds this
        // picture at full size already: a stroke sends what it painted,
        // not the whole canvas every frame.
        let [width, height] = size;
        let partial = self.images.get(&key).and_then(|image| {
            let fits = image.size == size
                && image.nearest == nearest
                && channels == 4
                && image.version != version
                && pixels.len() >= width as usize * height as usize * 4;
            fits.then(|| changed(image.version)).flatten()
        });
        let Some(rects) = partial else {
            return self.texture(key, version, size, channels, pixels, nearest);
        };
        let image = self.images.get_mut(&key)?;
        for [x0, y0, x1, y1] in rects {
            let (x1, y1) = (x1.min(width), y1.min(height));
            if x0 >= x1 || y0 >= y1 {
                continue;
            }
            let offset = (y0 as usize * width as usize + x0 as usize) * 4;
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &image.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: x0, y: y0, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels[offset..],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(y1 - y0),
                },
                wgpu::Extent3d {
                    width: x1 - x0,
                    height: y1 - y0,
                    depth_or_array_layers: 1,
                },
            );
        }
        // A stroke changes the base level, so the chain below it is stale.
        self.queue.submit(Some(self.mips.blit(
            &self.device,
            &image.texture,
            image.levels,
        )));
        image.version = version;
        image.used = true;
        Some(image.id)
    }

    fn texture(
        &mut self,
        key: u64,
        version: u64,
        size: [u32; 2],
        channels: u8,
        pixels: &[u8],
        nearest: bool,
    ) -> Option<egui::TextureId> {
        if let Some(image) = self.images.get_mut(&key)
            && image.version == version
            && image.nearest == nearest
        {
            image.used = true;
            return Some(image.id);
        }
        let (upload_size, rgba) = upload_pixels(size, channels, pixels, self.max_side)?;
        let extent = wgpu::Extent3d {
            width: upload_size[0],
            height: upload_size[1],
            depth_or_array_layers: 1,
        };
        let reuse = self
            .images
            .get(&key)
            .is_some_and(|image| image.size == upload_size && image.nearest == nearest);
        if !reuse {
            if let Some(old) = self.images.remove(&key) {
                self.renderer.free_texture(&old.id);
            }
            let levels = mip_levels(upload_size);
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("sue-image"),
                size: extent,
                mip_level_count: levels,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                // As egui's own textures: egui's shader takes what it samples
                // as sRGB and converts it itself, so an sRGB texture (which
                // the GPU turns linear first) showed pictures too dark in the
                // middle tones, too contrasty.
                format: IMAGE_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: if self.mips.srgb_views {
                    &[IMAGE_FORMAT_SRGB]
                } else {
                    &[]
                },
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            // Nearest keeps pixels crisp when magnified, the way someone
            // painting at 800% wants them. Below that, minifying asks for the
            // mip chain, which a plain bilinear tap cannot replace.
            let id = self.renderer.register_native_texture_with_sampler_options(
                &self.device,
                &view,
                wgpu::SamplerDescriptor {
                    label: Some("sue-image"),
                    mag_filter: if nearest {
                        wgpu::FilterMode::Nearest
                    } else {
                        wgpu::FilterMode::Linear
                    },
                    min_filter: wgpu::FilterMode::Linear,
                    mipmap_filter: wgpu::MipmapFilterMode::Linear,
                    ..Default::default()
                },
            );
            self.images.insert(
                key,
                NativeImage {
                    version,
                    size: upload_size,
                    nearest,
                    levels,
                    texture,
                    id,
                    used: true,
                },
            );
        }
        let image = self.images.get_mut(&key)?;
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &image.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(upload_size[0] * 4),
                rows_per_image: Some(upload_size[1]),
            },
            extent,
        );
        // The levels below the base one come from what was just written.
        self.queue.submit(Some(self.mips.blit(
            &self.device,
            &image.texture,
            image.levels,
        )));
        image.version = version;
        image.used = true;
        Some(image.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uploads_borrow_rgba_and_expand_or_shrink_only_when_needed() {
        let rgba = vec![7_u8; 4 * 4 * 4];
        let (size, pixels) = upload_pixels([4, 4], 4, &rgba, 4096).unwrap();
        assert_eq!(size, [4, 4]);
        assert!(matches!(pixels, Cow::Borrowed(_)));
        let gray = vec![9_u8; 10 * 4];
        let (size, pixels) = upload_pixels([10, 4], 1, &gray, 5).unwrap();
        assert_eq!(size, [5, 2]);
        assert_eq!(&pixels[..4], &[9, 9, 9, 255]);
        assert!(upload_pixels([4, 4], 4, &rgba[..8], 4096).is_none());
    }

    /// A picture shows on the screen in its own colors: a middle gray
    /// stays a middle gray (it came out much darker when its texture made
    /// the GPU turn it linear before egui's shader did the same again).
    #[test]
    fn a_picture_shows_in_its_own_colors() {
        let Ok(gpu) = crate::gpu::GpuContext::headless() else {
            return;
        };
        let mut painter = EguiPainter::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb);
        let pixels: Vec<u8> = [128_u8, 64, 200, 255].repeat(16 * 16);
        let id = TextureUpload::texture(&mut painter, 1, 1, [16, 16], 4, &pixels, true)
            .expect("an uploaded picture");
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.painter().image(
                id,
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(16.0, 16.0)),
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        });
        let shot = painter
            .paint_offscreen(
                &ctx,
                [16, 16],
                egui::Color32::BLACK,
                output.shapes,
                output.textures_delta,
                1.0,
            )
            .unwrap();
        let middle = &shot[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 3];
        for (got, want) in middle.iter().zip([128_u8, 64, 200]) {
            assert!(got.abs_diff(want) <= 2, "{middle:?}");
        }
    }

    #[test]
    fn a_texture_takes_a_level_for_every_halving() {
        assert_eq!(mip_levels([1600, 1200]), 11);
        assert_eq!(mip_levels([1024, 1024]), 11);
        assert_eq!(mip_levels([300, 200]), 9);
        assert_eq!(mip_levels([1, 1]), 1);
    }

    /// A picture drawn smaller than it is goes through its mip chain: what a
    /// screen pixel covers is averaged, not sampled. A line every four
    /// pixels, drawn a quarter size, comes out an even gray a quarter black,
    /// instead of the white that a bilinear tap of the base level finds
    /// between the lines (which is how a zoomed out picture lost its fine
    /// detail and wandered in weight).
    #[test]
    fn a_picture_zoomed_out_averages_what_it_covers() {
        let Ok(gpu) = crate::gpu::GpuContext::headless() else {
            return;
        };
        let mut painter = EguiPainter::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb);
        let pixels: Vec<u8> = (0..64 * 64)
            .flat_map(|at| {
                let value = if at % 4 == 0 { 0 } else { 255 };
                [value, value, value, 255]
            })
            .collect();
        let id = TextureUpload::texture(&mut painter, 1, 1, [64, 64], 4, &pixels, false)
            .expect("an uploaded picture");
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.painter().image(
                id,
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(16.0, 16.0)),
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        });
        let shot = painter
            .paint_offscreen(
                &ctx,
                [16, 16],
                egui::Color32::BLACK,
                output.shapes,
                output.textures_delta,
                1.0,
            )
            .unwrap();
        let gray: Vec<u8> = shot
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| pixel[0])
            .collect();
        let low = gray.iter().copied().min().unwrap_or_default();
        let high = gray.iter().copied().max().unwrap_or_default();
        // Averaged in light, a quarter black over white is the sRGB encoding
        // of three quarters, about 225. A device with no view formats (the GL
        // and WebGL backends) averages the encoded bytes instead, about 191.
        // Either is the average of what the pixel covers; a bilinear tap of
        // the base level reads 255, since it lands between the lines.
        let want = if painter.mips.srgb_views { 225 } else { 191 };
        assert!(
            low.abs_diff(want) <= 12 && high.abs_diff(want) <= 12,
            "the lines were sampled rather than averaged: {low} to {high}"
        );
    }
}
