//! Manual check for the Wayland provider: what it answers for each path, and how long it
//! takes, so a first run (a thumbnailer program) and a second (the cache) are visible.
//!
//!     cargo run --release -p ui-platform-wayland --example thumbnails_probe -- PATH...

#[cfg(not(unix))]
fn main() {
    eprintln!("the Wayland backend is Unix only");
}

#[cfg(unix)]
use std::time::Instant;

#[cfg(unix)]
use ui_platform::thumbnails::{ThumbnailRequest, Thumbnailer};

#[cfg(unix)]
fn main() {
    let provider = ui_platform_wayland::DesktopThumbnailer::new();
    for path in std::env::args().skip(1) {
        for size in [64u32, 300] {
            let request = ThumbnailRequest::new(&path, size);
            let started = Instant::now();
            let picture = provider.thumbnail(&request);
            let took = started.elapsed();
            match picture {
                Some(picture) => println!(
                    "{path} @{size}: {}×{} in {took:?}",
                    picture.width, picture.height
                ),
                None => println!("{path} @{size}: none in {took:?}"),
            }
        }
    }
}
