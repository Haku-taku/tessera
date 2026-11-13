//! The Liquid Glass showcase: the demo app the Flutter reference ships, rebuilt on this
//! engine's glass controls.
//!
//! Four bottom tabs, as the reference has: Explore, Widgets, Demos and Examples.

// The shared bootstrap opens a Wayland window; on a platform without it this example is a
// message rather than a compile error.
#[cfg(unix)]
mod app;
#[cfg(unix)]
mod pages;

#[cfg(unix)]
use app::LiquidGlassApp;
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
        "crabsnap-ui   Liquid Glass",
        "dev.crabsnap.ui.liquid_glass",
        1280,
        900,
    )
    .stack(STACK)
    .font("Inter", ui_demo::INTER)
    .font(ui_widgets::SF_SYMBOLS_FAMILY, ui_demo::SF_SYMBOLS)
    .wallpaper("/home/ollie/Pictures/Wallpapers/140959926_p0.png")
    .run(LiquidGlassApp::new());
}

#[cfg(not(unix))]
fn main() {
    eprintln!("the Liquid Glass demo boots through ui-demo, which opens a Wayland window");
}
