//! D3D11 paint backend: the [`Device`] the neutral scene draws through on Windows.
//!
//! The crate is empty off Windows. A D3D11 device cannot exist without COM and DXGI, and
//! there is nothing for a non-Windows build to fall back to, so the workspace builds an
//! empty library there rather than failing on a member it can never use.
//!
//! [`Device`]: ui_gpu_scene::Device

#![cfg(windows)]

mod device;
pub mod shader;

pub use device::D3d11Device;
