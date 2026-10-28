//! GPU device creation and the window swap chain.

use crate::{GpuTier, GpuVendor, GraphicsApi, HardwareCapabilities, PlatformError};

/// Which GPU draws a window (`display.gpu_preference`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GpuPreference {
    /// The GPU that drives the display, so frames never cross adapters.
    #[default]
    Auto,
    /// The power-saving integrated GPU.
    Integrated,
    /// The high-performance discrete GPU.
    Discrete,
}

impl GpuPreference {
    /// Reads a setting: `integrated`, `discrete`, and anything else is `auto`.
    #[must_use]
    pub fn from_setting(value: &str) -> Self {
        match value {
            "integrated" => Self::Integrated,
            "discrete" => Self::Discrete,
            _ => Self::Auto,
        }
    }
}

/// The GPU objects every renderer needs.
pub struct GpuContext {
    /// The wgpu instance.
    pub instance: wgpu::Instance,
    /// The chosen adapter.
    pub adapter: wgpu::Adapter,
    /// The logical device.
    pub device: wgpu::Device,
    /// The submission queue.
    pub queue: wgpu::Queue,
}

impl std::fmt::Debug for GpuContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let info = self.adapter.get_info();
        f.debug_struct("GpuContext")
            .field("adapter", &info.name)
            .field("backend", &info.backend)
            .finish_non_exhaustive()
    }
}

impl GpuContext {
    /// A device for offscreen rendering (snapshots, tests). Tries a hardware
    /// adapter first, then the software fallback adapter.
    ///
    /// # Errors
    /// [`PlatformError::Gpu`] if no adapter or device is available.
    pub fn headless() -> Result<Self, PlatformError> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        // Offscreen frames are never shown, so the power-saving GPU is enough.
        pollster::block_on(Self::with_instance(
            instance,
            None,
            GpuPreference::Integrated,
        ))
    }

    /// Picks an adapter compatible with `surface` (if any) by `preference`
    /// and opens a device.
    pub(crate) async fn with_instance(
        instance: wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
        preference: GpuPreference,
    ) -> Result<Self, PlatformError> {
        let mut adapter = match preference {
            GpuPreference::Auto => display_adapter(&instance, surface).await,
            GpuPreference::Integrated | GpuPreference::Discrete => None,
        };
        let power_preference = match preference {
            GpuPreference::Discrete => wgpu::PowerPreference::HighPerformance,
            GpuPreference::Integrated => wgpu::PowerPreference::LowPower,
            // Windows lists the display GPU first; elsewhere the integrated
            // GPU usually drives the display.
            GpuPreference::Auto if cfg!(windows) => wgpu::PowerPreference::None,
            GpuPreference::Auto => wgpu::PowerPreference::LowPower,
        };
        for force_fallback_adapter in [false, true] {
            if adapter.is_some() {
                break;
            }
            let options = wgpu::RequestAdapterOptions {
                power_preference,
                force_fallback_adapter,
                compatible_surface: surface,
                ..Default::default()
            };
            adapter = instance.request_adapter(&options).await.ok();
        }
        let adapter =
            adapter.ok_or_else(|| PlatformError::Gpu("no compatible GPU adapter".to_owned()))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("sue"),
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .map_err(|error| PlatformError::Gpu(error.to_string()))?;
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
        })
    }

    /// `name (backend)` of the adapter, for diagnostics.
    #[must_use]
    pub fn adapter_label(&self) -> String {
        let info = self.adapter.get_info();
        format!("{} ({:?})", info.name, info.backend)
    }

    /// `base` with the graphics API, vendor and tier filled in from the adapter.
    #[must_use]
    pub fn describe(&self, base: HardwareCapabilities) -> HardwareCapabilities {
        describe_adapter(&self.adapter.get_info(), base)
    }
}

/// The adapter for the GPU that drives the primary display, on Windows.
///
/// DXGI lists that GPU first, after applying the per-app choice in the system
/// graphics settings. Drawing on any other GPU makes Windows copy every frame
/// across adapters, and that copy keeps a time-critical driver thread busy
/// even while the window sleeps (measured: a full core, with the integrated
/// GPU drawing for a monitor on the discrete one).
async fn display_adapter(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
) -> Option<wgpu::Adapter> {
    if !cfg!(windows) {
        return None;
    }
    let adapters = instance.enumerate_adapters(wgpu::Backends::all()).await;
    let display = adapters
        .iter()
        .find(|adapter| adapter.get_info().backend == wgpu::Backend::Dx12)?
        .get_info();
    // The same GPU through the first backend wgpu itself would choose.
    adapters.into_iter().find(|adapter| {
        let info = adapter.get_info();
        info.vendor == display.vendor
            && info.device == display.device
            && info.device_type != wgpu::DeviceType::Cpu
            && surface.is_none_or(|target| adapter.is_surface_supported(target))
    })
}

/// Fills GPU facts into `base` from adapter information.
#[must_use]
pub fn describe_adapter(
    info: &wgpu::AdapterInfo,
    base: HardwareCapabilities,
) -> HardwareCapabilities {
    let graphics_api = match info.backend {
        wgpu::Backend::Vulkan => GraphicsApi::Vulkan,
        wgpu::Backend::Dx12 => GraphicsApi::Dx12,
        wgpu::Backend::Metal => GraphicsApi::Metal,
        wgpu::Backend::Gl => GraphicsApi::OpenGl,
        _ => GraphicsApi::None,
    };
    let gpu_vendor = match info.vendor {
        0x10DE => GpuVendor::Nvidia,
        0x1002 | 0x1022 => GpuVendor::Amd,
        0x8086 => GpuVendor::Intel,
        0x106B => GpuVendor::Apple,
        0x5143 => GpuVendor::Qualcomm,
        0x13B5 => GpuVendor::Arm,
        0 => GpuVendor::Unknown,
        _ => GpuVendor::Other,
    };
    let tier = match info.device_type {
        wgpu::DeviceType::DiscreteGpu => GpuTier::HighEnd,
        wgpu::DeviceType::IntegratedGpu | wgpu::DeviceType::VirtualGpu => GpuTier::Mainstream,
        wgpu::DeviceType::Cpu => GpuTier::Software,
        wgpu::DeviceType::Other => GpuTier::Unknown,
    };
    // A GL-only driver is treated as legacy hardware, however fast the chip.
    let gpu_tier = if graphics_api == GraphicsApi::OpenGl {
        tier.min(GpuTier::Legacy)
    } else {
        tier
    };
    HardwareCapabilities {
        graphics_api,
        gpu_vendor,
        gpu_tier,
        ..base
    }
}

/// The swap chain of a window.
pub struct WindowSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

impl WindowSurface {
    pub(crate) fn new(
        surface: wgpu::Surface<'static>,
        gpu: &GpuContext,
        width: u32,
        height: u32,
    ) -> Result<Self, PlatformError> {
        let mut config = surface
            .get_default_config(&gpu.adapter, width.max(1), height.max(1))
            .ok_or_else(|| {
                PlatformError::Gpu("the surface is not supported by the adapter".to_owned())
            })?;
        let capabilities = surface.get_capabilities(&gpu.adapter);
        if let Some(format) = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
        {
            config.format = format;
        }
        config.present_mode = wgpu::PresentMode::AutoVsync;
        config.desired_maximum_frame_latency = 2;
        surface.configure(&gpu.device, &config);
        Ok(Self { surface, config })
    }

    /// The swap-chain texture format.
    #[must_use]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Current size in physical pixels.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub(crate) fn resize(&mut self, gpu: &GpuContext, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&gpu.device, &self.config);
    }

    /// The next frame to draw into, or `None` to skip this frame (the window is
    /// minimized, occluded, or the surface had to be reconfigured).
    pub(crate) fn acquire(&mut self, gpu: &GpuContext) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => Some(frame),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                drop(frame);
                self.surface.configure(&gpu.device, &self.config);
                None
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&gpu.device, &self.config);
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(
        backend: wgpu::Backend,
        vendor: u32,
        device_type: wgpu::DeviceType,
    ) -> wgpu::AdapterInfo {
        wgpu::AdapterInfo {
            name: "test".to_owned(),
            vendor,
            device: 0,
            device_type,
            device_pci_bus_id: String::new(),
            driver: String::new(),
            driver_info: String::new(),
            backend,
            subgroup_min_size: 4,
            subgroup_max_size: 128,
            transient_saves_memory: None,
            limit_bucket: None,
        }
    }

    #[test]
    fn gpu_preference_reads_its_setting() {
        assert_eq!(
            GpuPreference::from_setting("integrated"),
            GpuPreference::Integrated
        );
        assert_eq!(
            GpuPreference::from_setting("discrete"),
            GpuPreference::Discrete
        );
        assert_eq!(GpuPreference::from_setting("auto"), GpuPreference::Auto);
        assert_eq!(GpuPreference::from_setting("bogus"), GpuPreference::Auto);
    }

    #[test]
    #[ignore = "needs a real GPU; run with --ignored --nocapture"]
    fn auto_draws_on_the_display_gpu() {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let gpu = pollster::block_on(GpuContext::with_instance(
            instance,
            None,
            GpuPreference::Auto,
        ))
        .unwrap();
        println!("auto draws on {}", gpu.adapter_label());
        if cfg!(windows) {
            let display = pollster::block_on(gpu.instance.enumerate_adapters(wgpu::Backends::DX12))
                .into_iter()
                .next()
                .unwrap()
                .get_info();
            let chosen = gpu.adapter.get_info();
            assert_eq!(
                (chosen.vendor, chosen.device),
                (display.vendor, display.device)
            );
        }
    }

    #[test]
    fn adapters_map_to_capabilities() {
        let base = HardwareCapabilities::probe_portable();
        let discrete = describe_adapter(
            &info(wgpu::Backend::Vulkan, 0x10DE, wgpu::DeviceType::DiscreteGpu),
            base.clone(),
        );
        assert_eq!(
            (
                discrete.graphics_api,
                discrete.gpu_vendor,
                discrete.gpu_tier
            ),
            (GraphicsApi::Vulkan, GpuVendor::Nvidia, GpuTier::HighEnd)
        );
        let gl = describe_adapter(
            &info(wgpu::Backend::Gl, 0x8086, wgpu::DeviceType::IntegratedGpu),
            base.clone(),
        );
        assert_eq!(
            (gl.graphics_api, gl.gpu_tier),
            (GraphicsApi::OpenGl, GpuTier::Legacy)
        );
        let software = describe_adapter(
            &info(wgpu::Backend::Dx12, 0x1414, wgpu::DeviceType::Cpu),
            base,
        );
        assert_eq!(
            (software.gpu_vendor, software.gpu_tier),
            (GpuVendor::Other, GpuTier::Software)
        );
    }
}
