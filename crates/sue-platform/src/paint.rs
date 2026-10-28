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

struct NativeImage {
    version: u64,
    size: [u32; 2],
    nearest: bool,
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
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("sue-image"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let filter = if nearest {
                wgpu::FilterMode::Nearest
            } else {
                wgpu::FilterMode::Linear
            };
            let id = self
                .renderer
                .register_native_texture(&self.device, &view, filter);
            self.images.insert(
                key,
                NativeImage {
                    version,
                    size: upload_size,
                    nearest,
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
}
