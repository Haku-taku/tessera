//! The Windows file services against real paths: places, a listing, thumbnails and icons.
//!
//! Run: `cargo run -p ui-platform-win32 --example files_probe -- [DIR] [ICON_DUMP_DIR]`
//! A dump directory writes the composed icons as PNGs, which is how blur or colour fringes
//! in the provider's own pixels are told apart from the painter's.

#[cfg(not(windows))]
fn main() {
    eprintln!("the Win32 backend is Windows only");
}

#[cfg(windows)]
fn main() {
    use ui_platform::Backend;
    use ui_platform::file_icons::FileIconRequest;
    use ui_platform::listing::ListingRequest;
    use ui_platform::thumbnails::ThumbnailRequest;
    use ui_platform::FileKind;
    use windows::Win32::UI::HiDpi::GetDpiForSystem;

    // The picker asks for physical pixels, so the scale the window will report is what makes
    // a 20-pixel request the 20-pixel box the painter draws.
    let dpi = unsafe { GetDpiForSystem() };
    println!("system DPI: {dpi} ({:.2}x)", dpi as f32 / 96.0);

    let mut backend = ui_platform_win32::Win32Backend::new();
    for place in backend.places() {
        println!("place {:?}: {}", place.kind, place.path.display());
    }

    let directory = std::env::args()
        .nth(1)
        .unwrap_or_else(|| String::from("C:\\Windows\\Web\\Wallpaper\\Windows"));
    let lister = backend.file_listing().expect("a lister");
    let entries = lister
        .entries(&ListingRequest::new(&directory))
        .expect("the directory to read");
    println!("{} entries in {directory}", entries.len());
    for entry in entries.iter().take(8) {
        println!(
            "  {:?} {} hidden={} dir={} len={}",
            entry.kind, entry.name, entry.hidden, entry.directory, entry.len
        );
    }

    let thumbnails = backend.thumbnails().expect("a thumbnailer");
    for entry in entries.iter().filter(|entry| !entry.directory).take(3) {
        let started = std::time::Instant::now();
        let picture = thumbnails.thumbnail(&ThumbnailRequest::new(&entry.path, 128));
        match picture {
            Some(picture) => {
                let opaque = picture
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|pixel| pixel[3] == 255)
                    .count();
                println!(
                    "  thumbnail {}: {}x{} in {:?}, {} of {} pixels opaque",
                    entry.name,
                    picture.width,
                    picture.height,
                    started.elapsed(),
                    opaque,
                    picture.width * picture.height
                );
            }
            None => println!("  thumbnail {}: none in {:?}", entry.name, started.elapsed()),
        }
    }

    let icons = backend.file_icons().expect("an icon provider");
    // Per item: this is what the picker asks, and the shell may read the file itself.
    let mut identities = std::collections::HashMap::new();
    for size in [20u32, 80] {
        for entry in entries.iter().take(64) {
            let request = FileIconRequest::new(&entry.path, entry.kind, entry.link, size);
            let started = std::time::Instant::now();
            match icons.icon(&request) {
                Some(icon) => {
                    let transparent = icon
                        .base
                        .rgba
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .filter(|pixel| pixel[3] == 0)
                        .count();
                    *identities.entry((size, icon.identity)).or_insert(0) += 1;
                    println!(
                        "  icon {} {:?} @{size}: {}x{} identity={:#x} badge={} in {:?}, {transparent} transparent",
                        entry.name,
                        entry.kind,
                        icon.base.width,
                        icon.base.height,
                        icon.identity,
                        icon.badge.is_some(),
                        started.elapsed()
                    );
                }
                None => println!("  icon {} @{size}: none in {:?}", entry.name, started.elapsed()),
            }
        }
    }
    println!(
        "{} items answered {} distinct pictures",
        entries.len().min(64) * 2,
        identities.len()
    );

    // The kind fallback the provider uses when the item itself cannot be asked about.
    for kind in [
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
    ] {
        let started = std::time::Instant::now();
        let request = FileIconRequest::new("missing-name", kind, false, 20);
        match icons.icon(&request) {
            Some(icon) => println!(
                "  fallback {kind:?}: {}x{} identity={:#x} in {:?}",
                icon.base.width,
                icon.base.height,
                icon.identity,
                started.elapsed()
            ),
            None => println!("  fallback {kind:?}: none in {:?}", started.elapsed()),
        }
    }

    if let Some(output) = std::env::args().nth(2) {
        let cache = ui_platform::FileIcons::new(icons, std::sync::Arc::new(NoopWaker));
        std::fs::create_dir_all(&output).expect("an output directory");
        for entry in entries.iter().take(16) {
            let name: String = entry
                .name
                .chars()
                .map(|letter| {
                    if letter.is_alphanumeric() || matches!(letter, '.' | '-' | '_') {
                        letter
                    } else {
                        '_'
                    }
                })
                .collect();
            for size in [20u32, 25, 32, 40, 80, 100] {
                let request = FileIconRequest::new(&entry.path, entry.kind, entry.link, size);
                dump(&cache, &request, &output, &format!("{name}@{size}"));
            }
        }
        // The kind fallback is the image-list path, whose alpha layout differs from the
        // image factory's; the two have to be told apart in the dump.
        for kind in [
            FileKind::Folder,
            FileKind::Drive,
            FileKind::Document,
            FileKind::Pdf,
            FileKind::Image,
            FileKind::Archive,
            FileKind::Generic,
        ] {
            let request = FileIconRequest::new("missing-name", kind, false, 20);
            dump(&cache, &request, &output, &format!("kind-{kind:?}@20"));
        }
    }
}

#[cfg(windows)]
/// The probe runs with no loop to wake: it waits for each icon instead of asking for a frame.
struct NoopWaker;

#[cfg(windows)]
impl ui_platform::jobs::Waker for NoopWaker {
    fn wake(&self) {}
}

#[cfg(windows)]
fn dump(
    cache: &ui_platform::FileIcons,
    request: &ui_platform::FileIconRequest,
    output: &str,
    name: &str,
) {
    use std::time::{Duration, Instant};
    use ui_platform::FileIconState;

    let deadline = Instant::now() + Duration::from_secs(30);
    let image = loop {
        match cache.get(request) {
            FileIconState::Ready(image) => break image,
            FileIconState::Absent => return,
            FileIconState::Loading if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            FileIconState::Loading => {
                eprintln!("  dump {name}: the icon never arrived");
                return;
            }
        }
    };
    let Some(pixels) = image.pixels() else {
        return;
    };
    let Some(buffer) = image::RgbaImage::from_raw(image.width, image.height, pixels.to_vec()) else {
        return;
    };
    let path = std::path::Path::new(output).join(format!("{name}.png"));
    match buffer.save(&path) {
        Ok(()) => println!("  dump {}", path.display()),
        Err(error) => eprintln!("  dump {}: {error}", path.display()),
    }
}
