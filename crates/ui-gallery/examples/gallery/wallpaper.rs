//! The window's wallpaper: one photo behind the shell, so the cards and the glass have
//! something to sit on instead of a flat surface.

use ui_core::draw::ImageEntry;
use ui_platform::view::AppLoop;

use crate::app::Gallery;

/// The texture id the root refers to; stable for the run, so the renderer caches one upload.
pub const WALLPAPER: u64 = 0x7761_6c6c_7061_7065;

/// The file this machine's gallery is tuned against; `GALLERY_WALLPAPER` overrides it.
const DEFAULT_PATH: &str = "/home/ollie/Pictures/Wallpapers/140959926_p0.png";

/// Register the wallpaper. A missing or undecodable file leaves the window's flat surface,
/// because the gallery must still run on a machine that does not have this photo.
pub fn install(lp: &mut AppLoop<Gallery>) {
    if let Some(entry) = entry() {
        lp.register_image(entry);
    }
}

/// The photo as an entry a host can open a surface with, if the file can be read.
pub fn entry() -> Option<ImageEntry> {
    let path = std::env::var("GALLERY_WALLPAPER").unwrap_or_else(|_| DEFAULT_PATH.to_string());
    match decode(&path) {
        Some(entry) => Some(entry),
        None => {
            eprintln!("wallpaper: {path} could not be read; the window keeps its flat surface");
            None
        }
    }
}

/// Straight-alpha RGBA8, which is what `ImageEntry` carries and what the shader samples.
fn decode(path: &str) -> Option<ImageEntry> {
    let image = image::open(path).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    Some(ImageEntry {
        id: WALLPAPER,
        width,
        height,
        source: ui_core::draw::ImageSource::Pixels(std::sync::Arc::new(image.into_raw())),
        colour: ui_core::colour::ContentColour::SDR,
        sampling: ui_core::draw::Sampling::Smooth,
    })
}
