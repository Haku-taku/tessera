//! The platform's picture for a row: the base layer a thumbnail may replace.
//!
//! A thumbnail decodes one file and may take seconds, so it runs on a worker. An icon is a
//! theme file read or a shell query about the item, which on Windows costs 2–26 ms per item —
//! a list scrolling fast reveals dozens per frame — so it runs on a worker too, through the
//! same [`Pictures`](crate::pictures) queue. Both hand over `ImageEntry`, so an icon uploads
//! once and draws through one pipeline.
//!
//! The request names the item itself, not its category. Windows answers an executable with
//! the icon embedded in it and a folder with the one `desktop.ini` names; a freedesktop
//! desktop answers the icon its theme maps the exact MIME type to. A provider that cannot
//! answer for the item falls back to the kind, which every platform can draw.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use ui_core::colour::ContentColour;
use ui_core::draw::{ImageEntry, ImageSource, Sampling};

use crate::files::FileKind;
use crate::jobs::Waker;
use crate::keep_alive::KeepAlive;
use crate::IDLE_WINDOW;
use crate::pictures::{Limits, Pictures, Produced, State};
use crate::thumbnails::ThumbnailPicture;

/// Answers kept in memory. A directory may name thousands of items, and each visible row
/// asks once per frame; this is large enough that scrolling back over a folder is free.
const MAX_REQUESTS: usize = 4096;
/// Distinct pictures kept: kinds times overlays times sizes, so this is generous.
const MAX_PICTURES: usize = 256;
const MAX_BYTES: usize = 16 << 20;

/// Icon ids share the surface's texture table with app pictures and thumbnails, so they
/// carry their own prefix. This one spells "icon" in ASCII.
const ID_BASE: u64 = 0x6963_6f6e_0000_0000;

/// One item at one size, with what the platform may use to tell it from its neighbours.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileIconRequest {
    /// The item itself: an embedded icon, a `desktop.ini` and a link's target are all facts
    /// about this path, not about its extension.
    pub path: PathBuf,
    /// The category the name implies. It is the fallback when the platform cannot answer
    /// for the item, and the whole answer on platforms whose icons are per type.
    pub kind: FileKind,
    /// The item is a symbolic link, which a desktop that badges links should mark.
    pub link: bool,
    /// The longer side in physical pixels, as for a thumbnail.
    pub size: u32,
}

impl FileIconRequest {
    pub fn new(path: impl Into<PathBuf>, kind: FileKind, link: bool, size: u32) -> Self {
        Self {
            path: path.into(),
            kind,
            link,
            size,
        }
    }
}

/// The corner a badge occupies on the base picture. The convention is the platform's: the
/// shell puts its overlays where its own views put them, a themed desktop where its file
/// manager does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// A mark drawn over the base picture: a shell overlay (a shortcut arrow, a sync state) or a
/// desktop emblem (a link mark), already decoded.
pub struct FileBadge {
    pub picture: ThumbnailPicture,
    pub corner: BadgeCorner,
}

/// The provider's answer.
pub struct FileIcon {
    /// Names this picture, not the item: two answers that share an identity and dimensions
    /// share one upload, so a folder of one type costs one icon. A provider derives it from
    /// whatever it knows names the pixels — a theme icon's path, or the pixels' own bytes
    /// when the platform's bookkeeping may disagree with them.
    pub identity: u64,
    pub base: ThumbnailPicture,
    pub badge: Option<FileBadge>,
}

/// The platform half. Called on the module's worker thread, where blocking is expected and a
/// slow query delays only the icons behind it.
pub trait FileIconProvider: Send + Sync {
    /// The picture for this item, or `None` when the platform has none.
    fn icon(&self, request: &FileIconRequest) -> Option<FileIcon>;
}

/// What a [`FileIcons::get`] call answers.
#[derive(Clone, Debug)]
pub enum FileIconState {
    /// The worker is on it; ask again after the frame its completion forces.
    Loading,
    Ready(ImageEntry),
    /// The platform has no icon for this item; the caller draws its own mark.
    Absent,
}

/// The app's handle to icons. Cheap to clone; every clone shares one worker and cache.
#[derive(Clone)]
pub struct FileIcons {
    pictures: Pictures<FileIconRequest>,
}

impl FileIcons {
    /// Start the worker. `waker` is what forces a frame when an icon arrives.
    ///
    /// Construction never fails: a worker that could not start answers `Absent` for
    /// everything, and the rows then draw the framework's glyph.
    pub fn new(provider: Box<dyn FileIconProvider>, waker: Arc<dyn Waker>) -> Self {
        Self {
            pictures: Pictures::new(
                "file icons",
                waker,
                Limits {
                    requests: MAX_REQUESTS,
                    pictures: MAX_PICTURES,
                    bytes: MAX_BYTES,
                    idle: IDLE_WINDOW,
                },
                move |request| {
                    let icon = provider.icon(request)?;
                    let picture = compose(&icon.base, icon.badge.as_ref());
                    let entry = picture_entry(icon.identity, picture)?;
                    Some(Produced {
                        identity: icon.identity,
                        entry,
                    })
                },
            ),
        }
    }

    /// The picture for this item: from memory, or a queued request that answers `Loading`.
    /// `Absent` means the caller draws its own mark; that answer is cached like a picture.
    pub fn get(&self, request: &FileIconRequest) -> FileIconState {
        match self.pictures.get(request) {
            State::Loading => FileIconState::Loading,
            State::Ready(entry) => FileIconState::Ready(entry),
            State::Absent => FileIconState::Absent,
        }
    }

    /// Keep the icons through the idle window while the returned claim lives: a window that
    /// is open but has not drawn for a minute still wants them.
    pub fn keep_alive(&self) -> KeepAlive {
        self.pictures.keep_alive()
    }

    /// How many requests have finished. The loop forces one frame when this moves.
    pub fn completed(&self) -> u64 {
        self.pictures.completed()
    }

    /// Drop every icon; the next `get` asks the provider again.
    pub fn clear(&self) {
        self.pictures.clear();
    }
}

/// The id names the picture, so a picture made again replaces the engine's copy instead of
/// adding a second: the engine keeps every registered id it is handed for the run.
fn picture_entry(identity: u64, picture: ThumbnailPicture) -> Option<ImageEntry> {
    let expected = picture.width as usize * picture.height as usize * 4;
    if picture.width == 0 || picture.height == 0 || picture.rgba.len() != expected {
        log::warn!(
            "ui-platform: an icon came back {}×{} with {} bytes, not {expected}",
            picture.width,
            picture.height,
            picture.rgba.len()
        );
        return None;
    }
    let mut hasher = DefaultHasher::new();
    identity.hash(&mut hasher);
    picture.width.hash(&mut hasher);
    picture.height.hash(&mut hasher);
    Some(ImageEntry {
        id: ID_BASE | hasher.finish(),
        width: picture.width,
        height: picture.height,
        source: ImageSource::Pixels(Arc::new(picture.rgba)),
        colour: ContentColour::SDR,
        sampling: Sampling::Smooth,
    })
}

/// Draw a badge over the base picture, at the badge's own pixel size and flush with its
/// corner: the platform rendered the mark at the size its own views use, and scaling it
/// here would blur the only pixels it has. A badge larger than the base is cropped.
fn compose(base: &ThumbnailPicture, badge: Option<&FileBadge>) -> ThumbnailPicture {
    let Some(badge) = badge else {
        return base.clone();
    };
    let mut picture = base.clone();
    let width = picture.width as usize;
    let height = picture.height as usize;
    let badge_width = badge.picture.width as usize;
    let badge_height = badge.picture.height as usize;
    if width == 0 || height == 0 || badge_width == 0 || badge_height == 0 {
        return picture;
    }
    let left = match badge.corner {
        BadgeCorner::TopRight | BadgeCorner::BottomRight => width.saturating_sub(badge_width),
        BadgeCorner::TopLeft | BadgeCorner::BottomLeft => 0,
    };
    let top = match badge.corner {
        BadgeCorner::BottomLeft | BadgeCorner::BottomRight => height.saturating_sub(badge_height),
        BadgeCorner::TopLeft | BadgeCorner::TopRight => 0,
    };
    for row in 0..badge_height.min(height - top) {
        for column in 0..badge_width.min(width - left) {
            let source = (row * badge_width + column) * 4;
            let destination = ((top + row) * width + left + column) * 4;
            blend_over(
                &mut picture.rgba[destination..destination + 4],
                &badge.picture.rgba[source..source + 4],
            );
        }
    }
    picture
}

/// Source-over with straight channels, the blend the painter applies: the destination
/// keeps showing through the badge's translucent pixels.
fn blend_over(destination: &mut [u8], source: &[u8]) {
    let source_alpha = source[3] as u32;
    if source_alpha == 0 {
        return;
    }
    if source_alpha == 255 {
        destination.copy_from_slice(source);
        return;
    }
    let inverse = 255 - source_alpha;
    let destination_alpha = destination[3] as u32;
    // Both sides scaled by 255: `alpha` is the output's coverage, `destination`'s share is
    // `destination_alpha * inverse`.
    let destination_share = destination_alpha * inverse;
    let alpha = source_alpha * 255 + destination_share;
    if alpha == 0 {
        destination.fill(0);
        return;
    }
    for channel in 0..3 {
        let mixed = source[channel] as u32 * source_alpha * 255
            + destination[channel] as u32 * destination_share;
        destination[channel] = ((mixed + alpha / 2) / alpha).min(255) as u8;
    }
    destination[3] = ((alpha + 127) / 255) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    struct CountingWaker(AtomicUsize);

    impl Waker for CountingWaker {
        fn wake(&self) {
            self.0.fetch_add(1, Ordering::Release);
        }
    }

    struct Counting {
        calls: Arc<AtomicUsize>,
        answer: fn(&FileIconRequest) -> Option<FileIcon>,
    }

    impl FileIconProvider for Counting {
        fn icon(&self, request: &FileIconRequest) -> Option<FileIcon> {
            self.calls.fetch_add(1, Ordering::Release);
            (self.answer)(request)
        }
    }

    fn waker() -> Arc<dyn Waker> {
        Arc::new(CountingWaker(AtomicUsize::new(0)))
    }

    fn counting(
        answer: fn(&FileIconRequest) -> Option<FileIcon>,
    ) -> (Box<Counting>, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = Box::new(Counting {
            calls: Arc::clone(&calls),
            answer,
        });
        (provider, calls)
    }

    fn request(path: &str) -> FileIconRequest {
        FileIconRequest::new(Path::new(path), FileKind::Text, false, 32)
    }

    fn square(request: &FileIconRequest) -> Option<FileIcon> {
        Some(FileIcon {
            identity: 7,
            base: ThumbnailPicture {
                width: request.size,
                height: request.size,
                rgba: vec![9; request.size as usize * request.size as usize * 4],
            },
            badge: None,
        })
    }

    fn nothing(_request: &FileIconRequest) -> Option<FileIcon> {
        None
    }

    fn wait_for(icons: &FileIcons, done: u64) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while icons.completed() < done && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(icons.completed() >= done, "the worker never finished");
    }

    /// Two items the provider identifies as one picture share a single upload, and the
    /// provider is still asked once per item: identity saves pixels, not the query.
    #[test]
    fn answers_with_one_identity_share_one_upload() {
        let (provider, calls) = counting(square);
        let icons = FileIcons::new(provider, waker());
        for path in ["/a.txt", "/b.txt"] {
            assert!(matches!(
                icons.get(&request(path)),
                FileIconState::Loading
            ));
        }
        wait_for(&icons, 2);
        assert_eq!(calls.load(Ordering::Acquire), 2);
        let first = icons.get(&request("/a.txt"));
        let second = icons.get(&request("/b.txt"));
        let (FileIconState::Ready(first), FileIconState::Ready(second)) = (first, second) else {
            panic!("both items answer a picture");
        };
        assert_eq!(first.id, second.id, "one upload, two rows");
        let (Some(first_pixels), Some(second_pixels)) = (first.pixels(), second.pixels()) else {
            panic!("an icon is pixels");
        };
        assert!(Arc::ptr_eq(first_pixels, second_pixels), "one upload");
        assert!(first.id >= ID_BASE);
    }

    /// One item is asked about once, and "no icon" is an answer the cache keeps.
    #[test]
    fn one_item_is_asked_once_even_when_it_has_no_icon() {
        let (provider, calls) = counting(nothing);
        let icons = FileIcons::new(provider, waker());
        assert!(matches!(
            icons.get(&request("/missing")),
            FileIconState::Loading
        ));
        wait_for(&icons, 1);
        for _ in 0..3 {
            assert!(matches!(
                icons.get(&request("/missing")),
                FileIconState::Absent
            ));
        }
        assert_eq!(calls.load(Ordering::Acquire), 1);
    }

    /// One identity at two sizes is two pictures: a provider names a kind's icon the same
    /// way at 20 and 80 pixels, and the grid must not draw the row's smaller picture.
    #[test]
    fn one_identity_at_two_sizes_keeps_two_pictures() {
        let (provider, _calls) = counting(square);
        let icons = FileIcons::new(provider, waker());
        let small = FileIconRequest::new("/a.txt", FileKind::Text, false, 20);
        let large = FileIconRequest::new("/a.txt", FileKind::Text, false, 80);
        icons.get(&small);
        icons.get(&large);
        wait_for(&icons, 2);
        let FileIconState::Ready(small) = icons.get(&small) else {
            panic!("the small picture is ready");
        };
        let FileIconState::Ready(large) = icons.get(&large) else {
            panic!("the large picture is ready");
        };
        assert_eq!((small.width, small.height), (20, 20));
        assert_eq!((large.width, large.height), (80, 80));
        assert_ne!(small.id, large.id, "two sizes are two uploads");
    }

    /// A picture keeps its id when it is made again. The engine keeps every registered id
    /// for the run, so minting a fresh one per production leaks the pixels it names.
    #[test]
    fn a_remade_picture_keeps_its_id() {
        let (provider, _calls) = counting(square);
        let icons = FileIcons::new(provider, waker());
        let request = request("/a.txt");
        assert!(matches!(
            icons.get(&request),
            FileIconState::Loading
        ));
        wait_for(&icons, 1);
        let FileIconState::Ready(first) = icons.get(&request) else {
            panic!("the first picture is ready");
        };
        icons.clear();
        assert!(matches!(
            icons.get(&request),
            FileIconState::Loading
        ));
        wait_for(&icons, 2);
        let FileIconState::Ready(second) = icons.get(&request) else {
            panic!("the re-made picture is ready");
        };
        assert_eq!(first.id, second.id, "one picture, one id");
    }

    /// An item's icon is asked for on the worker, never between two frames: a shell query
    /// costs milliseconds, and a list scrolling fast reveals dozens of items per frame.
    #[test]
    fn an_icon_is_asked_for_off_the_frame_thread() {
        struct Where(Arc<std::sync::Mutex<Option<std::thread::ThreadId>>>);

        impl FileIconProvider for Where {
            fn icon(&self, request: &FileIconRequest) -> Option<FileIcon> {
                if let Ok(mut seen) = self.0.lock() {
                    *seen = Some(std::thread::current().id());
                }
                square(request)
            }
        }

        let seen = Arc::new(std::sync::Mutex::new(None));
        let icons = FileIcons::new(Box::new(Where(Arc::clone(&seen))), waker());
        assert!(matches!(
            icons.get(&request("/a.txt")),
            FileIconState::Loading
        ));
        wait_for(&icons, 1);
        let asked_on = seen.lock().expect("not poisoned").to_owned();
        assert!(
            matches!(asked_on, Some(asked_on) if asked_on != std::thread::current().id()),
            "the provider ran on the frame's thread, or never ran"
        );
    }

    /// A badge lands in the corner it names, at its own size, and its translucent pixels
    /// let the base show through.
    #[test]
    fn a_badge_covers_its_corner_and_blends_what_it_covers() {
        let base = ThumbnailPicture {
            width: 2,
            height: 2,
            rgba: [10, 10, 10, 255].repeat(4),
        };
        let badge = FileBadge {
            picture: ThumbnailPicture {
                width: 1,
                height: 1,
                rgba: vec![200, 0, 0, 128],
            },
            corner: BadgeCorner::BottomRight,
        };
        let picture = compose(&base, Some(&badge));
        assert_eq!(picture.width, 2);
        assert_eq!(picture.height, 2);
        assert_eq!(&picture.rgba[0..4], &[10, 10, 10, 255], "top-left is base");
        assert_eq!(&picture.rgba[8..12], &[10, 10, 10, 255], "top-right is base");
        // Half-covered red over the grey: each channel is the coverage-weighted mix.
        let corner = &picture.rgba[12..16];
        assert_eq!(corner[0], 105, "red, blended");
        assert_eq!((corner[1], corner[2], corner[3]), (5, 5, 255));
    }
}
