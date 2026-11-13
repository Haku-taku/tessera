//! HCT: CAM16 hue and chroma at a CIELAB L\* tone, written from published colour science.
//!
//! Tone is deliberately CIELAB L\*, not CAM16 lightness: L\* maps to relative luminance, so
//! tone differences translate directly into contrast ratios.

pub mod cam;
pub mod contrast;
pub mod lab;
pub mod palette;
pub mod quantize;
pub mod scheme;
pub mod solve;
pub mod viewing;

pub use cam::{Cam, argb_from_cam_with, cam_from_argb, cam_from_argb_with, linrgb_from_jch};
pub use contrast::{
    darker, darker_unsafe, enable_light_foreground, lighter, lighter_unsafe, ratio_of_tones,
    ratio_of_ys, tone_allows_light_foreground, tone_prefers_light_foreground,
};
pub use lab::{
    argb_from_linrgb, argb_from_tone, delinearized, linrgb_from_xyz, lstar_from_y, rgb, tone_of,
    xyz_from_argb, y_from_lstar,
};
pub use palette::{KeyColor, TonalPalette};
pub use quantize::{Cluster, quantize};
pub use scheme::{Role, Scheme};
pub use solve::{argb_from_hcl, argb_from_hcl_in};
pub use viewing::Viewing;
