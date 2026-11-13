//! The WinUI 3 Gallery, rebuilt on this engine's Fluent controls.
//!
//! Navigation, page titles and sample captions follow Microsoft's own gallery; the samples
//! are the controls this crate actually draws.

// The shared bootstrap opens a Wayland window; on a platform without it this example is a
// message rather than a compile error.
#[cfg(unix)]
mod app;
#[cfg(unix)]
mod pages;

#[cfg(unix)]
use app::WinUiGallery;
#[cfg(unix)]
use ui_demo::Demo;

/// Faces in the order a glyph is looked up: Latin first, then the Chinese face behind it.
#[cfg(unix)]
const STACK: &[&str] = &[
    "SF Pro Text",
    "SF Pro Display",
    "Inter",
    "PingFang SC",
    "Noto Sans SC",
    "Noto Sans CJK SC",
    "MiSans VF",
    "HarmonyOS Sans SC",
];

#[cfg(unix)]
fn main() {
    Demo::new(
        "crabsnap-ui   WinUI Gallery",
        "dev.crabsnap.ui.winui_gallery",
        1440,
        960,
    )
    .stack(STACK)
    .font("Inter", ui_demo::INTER)
    .font(
        ui_widgets_fluent::glyphs::ICON_FAMILY,
        ui_demo::FLUENT_ICONS,
    )
    .run(WinUiGallery::new());
}

#[cfg(not(unix))]
fn main() {
    eprintln!("the WinUI Gallery demo boots through ui-demo, which opens a Wayland window");
}
