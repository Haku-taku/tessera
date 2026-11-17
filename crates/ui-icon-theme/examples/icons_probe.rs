//! What the desktop's icon theme answers for each kind and for named paths, and how long it
//! takes, so a PNG icon and a rendered SVG are both visible:
//!
//!     cargo run --release -p ui-icon-theme --example icons_probe -- [PATH...]

use std::path::Path;
use std::time::Instant;

use ui_icon_theme::IconTheme;
use ui_platform::{FileIconProvider, FileIconRequest, FileKind};

fn main() {
    let theme = IconTheme::new();
    let kinds = [
        FileKind::Folder,
        FileKind::Drive,
        FileKind::Document,
        FileKind::Spreadsheet,
        FileKind::Presentation,
        FileKind::Pdf,
        FileKind::Text,
        FileKind::Code,
        FileKind::Image,
        FileKind::Video,
        FileKind::Audio,
        FileKind::Archive,
        FileKind::Generic,
    ];
    for size in [16u32, 32, 128] {
        for kind in kinds {
            let started = Instant::now();
            let request = FileIconRequest::new(Path::new("stand-in"), kind, false, size);
            let answered = theme.icon(&request);
            let took = started.elapsed();
            match answered {
                Some(icon) => println!(
                    "{kind:?} @{size}: {}×{}, identity={:#x}, badge={} in {took:?}",
                    icon.base.width,
                    icon.base.height,
                    icon.identity,
                    icon.badge.is_some()
                ),
                None => println!("{kind:?} @{size}: none in {took:?}"),
            }
        }
    }

    for argument in std::env::args().skip(1) {
        let path = Path::new(&argument);
        let started = Instant::now();
        let request = FileIconRequest::new(path, FileKind::Generic, false, 32);
        match theme.icon(&request) {
            Some(icon) => {
                let set = icon
                    .base
                    .rgba
                    .chunks_exact(4)
                    .filter(|pixel| pixel[3] != 0)
                    .count();
                println!(
                    "{argument}: {}×{}, {set} set pixels, identity={:#x}, badge={} in {:?}",
                    icon.base.width,
                    icon.base.height,
                    icon.identity,
                    icon.badge.is_some(),
                    started.elapsed()
                );
            }
            None => println!("{argument}: none in {:?}", started.elapsed()),
        }
    }
}
