// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use klecks_settings::GpuSettings;
use serde::Serialize;

use crate::vendor::Vendor;

#[derive(Debug, Clone, Serialize)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: Vendor,
    pub backend: String,
    pub driver: String,
}

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("no GPU found on the Vulkan/DX12 backends; klecks never runs ML on the CPU")]
    NoGpu,
    #[error(
        "found GPU(s) [{found}] but none from an allowed vendor [{allowed}]; \
         edit `gpu.allowed_vendors` in config/klecks.yaml"
    )]
    VendorNotAllowed { found: String, allowed: String },
}

/// Refuses to start ML work unless a real, allowed GPU is present.
///
/// Software adapters (WARP, llvmpipe, SwiftShader) report `DeviceType::Cpu` and are
/// rejected, so a missing driver can never silently fall back to the CPU.
pub struct GpuGuard {
    allowed: Vec<Vendor>,
    allowed_names: Vec<String>,
    backends: wgpu::Backends,
}

impl GpuGuard {
    pub fn new(settings: &GpuSettings) -> Self {
        Self {
            allowed: settings
                .allowed_vendors
                .iter()
                .filter_map(|name| Vendor::from_config_name(name))
                .collect(),
            allowed_names: settings.allowed_vendors.clone(),
            backends: Self::backends_from(&settings.backends),
        }
    }

    /// Returns the best allowed GPU (discrete before integrated), or why there is none.
    pub fn detect(&self) -> Result<GpuInfo, GpuError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: self.backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapters = pollster::block_on(instance.enumerate_adapters(self.backends));

        let mut real: Vec<wgpu::AdapterInfo> = adapters
            .iter()
            .map(wgpu::Adapter::get_info)
            .filter(|info| {
                matches!(
                    info.device_type,
                    wgpu::DeviceType::DiscreteGpu | wgpu::DeviceType::IntegratedGpu
                )
            })
            .collect();
        if real.is_empty() {
            return Err(GpuError::NoGpu);
        }
        real.sort_by_key(|info| (info.device_type != wgpu::DeviceType::DiscreteGpu) as u8);

        // The same card shows up once per backend/layer; list each name only once.
        let mut names: Vec<String> = real.iter().map(|info| info.name.clone()).collect();
        names.sort();
        names.dedup();
        let found = names.join(", ");
        real.into_iter()
            .find(|info| self.allowed.contains(&Vendor::from_pci_id(info.vendor)))
            .map(|info| GpuInfo {
                vendor: Vendor::from_pci_id(info.vendor),
                backend: format!("{:?}", info.backend),
                driver: info.driver_info,
                name: info.name,
            })
            .ok_or_else(|| GpuError::VendorNotAllowed {
                found,
                allowed: self.allowed_names.join(", "),
            })
    }

    fn backends_from(names: &[String]) -> wgpu::Backends {
        let mut backends = wgpu::Backends::empty();
        for name in names {
            match name.trim().to_ascii_lowercase().as_str() {
                "vulkan" => backends |= wgpu::Backends::VULKAN,
                "dx12" => backends |= wgpu::Backends::DX12,
                _ => {}
            }
        }
        if backends.is_empty() {
            wgpu::Backends::VULKAN | wgpu::Backends::DX12
        } else {
            backends
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_backend_names_fall_back_to_vulkan_and_dx12() {
        let backends = GpuGuard::backends_from(&["metal".to_string()]);
        assert_eq!(backends, wgpu::Backends::VULKAN | wgpu::Backends::DX12);
    }

    #[test]
    fn pci_ids_map_to_vendors() {
        assert_eq!(Vendor::from_pci_id(0x10DE), Vendor::Nvidia);
        assert_eq!(Vendor::from_pci_id(0x1002), Vendor::Amd);
        assert_eq!(Vendor::from_pci_id(0xFFFF), Vendor::Other);
    }
}
