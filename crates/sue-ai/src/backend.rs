//! Compute backends and where they are available.

use std::fmt;

use sue_platform::{GpuVendor, GraphicsApi, HardwareCapabilities, OsFamily};

/// Where a tensor kernel executes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComputeBackend {
    /// Auto-vectorized Rust on the CPU. Always available.
    CpuSimd,
    /// Compute shaders through Vulkan or DX12 (wgpu).
    VulkanWgpu,
    /// NVIDIA CUDA.
    Cuda,
    /// Apple Metal.
    Metal,
    /// Android Neural Networks API.
    AndroidNnapi,
}

impl ComputeBackend {
    /// Every backend.
    pub const ALL: [Self; 5] = [
        Self::CpuSimd,
        Self::VulkanWgpu,
        Self::Cuda,
        Self::Metal,
        Self::AndroidNnapi,
    ];

    /// Preference when several are available: higher is better.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Cuda | Self::Metal => 4,
            Self::VulkanWgpu => 3,
            Self::AndroidNnapi => 2,
            Self::CpuSimd => 1,
        }
    }

    /// `true` if this backend can run on `os` with `capabilities`.
    #[must_use]
    pub fn available_on(self, os: OsFamily, capabilities: &HardwareCapabilities) -> bool {
        match self {
            Self::CpuSimd => true,
            Self::VulkanWgpu => matches!(
                capabilities.graphics_api,
                GraphicsApi::Vulkan | GraphicsApi::Dx12
            ),
            Self::Cuda => {
                matches!(os, OsFamily::Windows | OsFamily::Linux)
                    && capabilities.gpu_vendor == GpuVendor::Nvidia
            }
            Self::Metal => matches!(os, OsFamily::MacOs | OsFamily::Ios),
            Self::AndroidNnapi => os == OsFamily::Android,
        }
    }

    /// The available backends, best first.
    #[must_use]
    pub fn detect_available(os: OsFamily, capabilities: &HardwareCapabilities) -> Vec<Self> {
        let mut available: Vec<Self> = Self::ALL
            .into_iter()
            .filter(|backend| backend.available_on(os, capabilities))
            .collect();
        available.sort_by_key(|backend| std::cmp::Reverse(backend.rank()));
        available
    }

    /// Display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CpuSimd => "CPU (SIMD)",
            Self::VulkanWgpu => "Vulkan/wgpu",
            Self::Cuda => "CUDA",
            Self::Metal => "Metal",
            Self::AndroidNnapi => "Android NNAPI",
        }
    }
}

impl fmt::Display for ComputeBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capabilities(api: GraphicsApi, vendor: GpuVendor) -> HardwareCapabilities {
        HardwareCapabilities {
            graphics_api: api,
            gpu_vendor: vendor,
            ..HardwareCapabilities::probe_portable()
        }
    }

    #[test]
    fn detects_backends_per_platform() {
        let windows = capabilities(GraphicsApi::Vulkan, GpuVendor::Nvidia);
        assert_eq!(
            ComputeBackend::detect_available(OsFamily::Windows, &windows),
            [
                ComputeBackend::Cuda,
                ComputeBackend::VulkanWgpu,
                ComputeBackend::CpuSimd
            ]
        );
        let mac = capabilities(GraphicsApi::Metal, GpuVendor::Apple);
        assert_eq!(
            ComputeBackend::detect_available(OsFamily::MacOs, &mac),
            [ComputeBackend::Metal, ComputeBackend::CpuSimd]
        );
        let android = capabilities(GraphicsApi::Vulkan, GpuVendor::Qualcomm);
        assert_eq!(
            ComputeBackend::detect_available(OsFamily::Android, &android),
            [
                ComputeBackend::VulkanWgpu,
                ComputeBackend::AndroidNnapi,
                ComputeBackend::CpuSimd
            ]
        );
        let headless = HardwareCapabilities::probe_portable();
        assert_eq!(
            ComputeBackend::detect_available(OsFamily::Linux, &headless),
            [ComputeBackend::CpuSimd]
        );
    }
}
