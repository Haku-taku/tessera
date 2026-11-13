//! The outputs the compositor reports: what the app needs to place a surface or name a file.
//!
//! Sizes are logical: device pixels divided by the output's scale, with rotation applied, so
//! a canvas or a pin uses the numbers layout already uses.

/// One output.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OutputInfo {
    /// The connector's name, such as `DP-1`; empty when the compositor never said.
    pub name: String,
    /// The make and model, when the compositor names them.
    pub description: String,
    /// Top-left in the compositor's global layout, logical pixels.
    pub x: i32,
    pub y: i32,
    /// Logical size after rotation.
    pub width: u32,
    pub height: u32,
    /// Device pixels per logical pixel.
    pub scale: f32,
    /// Refresh rate in millihertz; zero when the compositor never said.
    pub refresh_mhz: u32,
    /// Rotated by a quarter turn: `width` and `height` are already swapped.
    pub rotated: bool,
}

impl OutputInfo {
    /// What a file name or a menu row calls this output.
    pub fn label(&self) -> String {
        if !self.name.is_empty() {
            self.name.clone()
        } else if !self.description.is_empty() {
            self.description.clone()
        } else {
            format!("{}x{}", self.width, self.height)
        }
    }
}
