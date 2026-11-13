//! The Liquid Glass showcase: the demo app the Flutter reference ships, rebuilt on this
//! engine's glass controls.
//!
//! Four bottom tabs, as the reference has: Explore, Widgets, Demos and Examples.

mod app;
mod pages;

use app::LiquidGlassApp;
use ui_demo::Demo;

/// Faces in the order a glyph is looked up: Latin first, then the Chinese face behind it.
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
