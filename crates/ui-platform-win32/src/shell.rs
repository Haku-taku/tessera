//! The Windows shell behind the picker: its places, directory listings, thumbnails and icons.
//!
//! Places and thumbnails are shell queries; listings are `std::fs` with the attribute rule
//! Windows adds to the dotfile one. A thumbnail may load an in-process shell extension, so it
//! runs on the service's worker; an icon is a shell query about one item and runs where it is
//! asked for.

use std::ffi::c_void;
use std::mem::size_of;
use std::path::PathBuf;

use ui_platform::FileKind;
use ui_platform::file_icons::{
    BadgeCorner, FileBadge, FileIcon, FileIconProvider, FileIconRequest,
};
use ui_platform::listing::FsDirectoryLister;
use ui_platform::places::{Device, Place, PlaceKind};
use ui_platform::search::FsDirectorySearcher;
use ui_platform::thumbnails::{Thumbnailer, ThumbnailPicture, ThumbnailRequest};
use windows::Win32::Foundation::{RPC_E_CHANGED_MODE, SIZE};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW, HBITMAP, HGDIOBJ,
};
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_SYSTEM,
    FILE_FLAGS_AND_ATTRIBUTES, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree};
use windows::Win32::UI::Controls::{ILD_TRANSPARENT, IImageList};
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Music, FOLDERID_Pictures,
    FOLDERID_Profile, FOLDERID_Public, FOLDERID_Templates, FOLDERID_Videos, FOLDERID_Windows,
    IShellItemImageFactory, KF_FLAG_DEFAULT, SHCreateItemFromParsingName, SHFILEINFOW,
    SHGFI_ICON, SHGFI_LARGEICON, SHGFI_OVERLAYINDEX, SHGFI_SMALLICON, SHGFI_SYSICONINDEX,
    SHGFI_USEFILEATTRIBUTES, SHGFI_FLAGS, SHGetFileInfoW, SHGetImageList, SHGetKnownFolderPath,
    SHIL_EXTRALARGE, SHIL_JUMBO, SHIL_LARGE, SHIL_SMALL, SIIGBF, SIIGBF_BIGGERSIZEOK,
    SIIGBF_ICONONLY, SIIGBF_RESIZETOFIT, SIIGBF_THUMBNAILONLY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GetIconInfo, HICON, ICONINFO,
};
use windows::core::{GUID, PCWSTR};

/// Whether an entry is hidden by Windows' rules: a dotfile, or an attribute the shell hides.
///
/// Explorer's "show hidden items" reveals both the hidden and the system attribute, so a
/// listing that hides one and shows the other would disagree with the folder the user sees.
pub(crate) fn hidden(name: &str, metadata: Option<&std::fs::Metadata>) -> bool {
    use std::os::windows::fs::MetadataExt;

    if name.starts_with('.') {
        return true;
    }
    let attributes = metadata
        .map(|metadata| metadata.file_attributes())
        .unwrap_or(0);
    attributes & (FILE_ATTRIBUTE_HIDDEN.0 | FILE_ATTRIBUTE_SYSTEM.0) != 0
}

/// The lister this desktop uses: the portable reader with Windows' hidden rule.
pub(crate) fn lister() -> FsDirectoryLister {
    FsDirectoryLister::new(hidden)
}

/// The search this desktop uses: the same walk and the same hidden rule.
pub(crate) fn searcher() -> FsDirectorySearcher {
    FsDirectorySearcher::new(hidden)
}

/// The user's profile and the shell's folder for each place.
///
/// A folder the profile does not have — "Videos" in a stripped-down profile — is left out:
/// the shell hides an uncreated folder too, and a picker row that fails to open is worse.
pub(crate) fn places() -> Vec<Place> {
    let home = known_folder(&FOLDERID_Profile);
    let mut places = Vec::new();
    if let Some(path) = &home
        && path.is_dir()
    {
        places.push(Place {
            kind: PlaceKind::Home,
            path: path.clone(),
        });
    }
    for (id, kind) in [
        (&FOLDERID_Desktop, PlaceKind::Desktop),
        (&FOLDERID_Documents, PlaceKind::Documents),
        (&FOLDERID_Downloads, PlaceKind::Downloads),
        (&FOLDERID_Music, PlaceKind::Music),
        (&FOLDERID_Pictures, PlaceKind::Pictures),
        (&FOLDERID_Videos, PlaceKind::Videos),
        (&FOLDERID_Templates, PlaceKind::Templates),
        (&FOLDERID_Public, PlaceKind::PublicShare),
    ] {
        let Some(path) = known_folder(id) else {
            continue;
        };
        if home.as_deref() == Some(path.as_path()) || !path.is_dir() {
            continue;
        }
        places.push(Place { kind, path });
    }
    places
}

/// The volumes Explorer offers under This PC: the fixed, removable, network and optical
/// drives that hold media right now.
///
/// A drive letter with no media — an empty optical drive, a disconnected share — fails the
/// volume query, and a row that could not open is worse than no row.
pub(crate) fn devices() -> Vec<Device> {
    use windows::Win32::System::WindowsProgramming::{
        DRIVE_CDROM, DRIVE_FIXED, DRIVE_REMOTE, DRIVE_REMOVABLE,
    };

    // SAFETY: both calls take a NUL-terminated root path that outlives them, and the
    // volume-name buffer is the size the API documents.
    let mask = unsafe { GetLogicalDrives() };
    let mut devices = Vec::new();
    for index in 0..26u8 {
        if mask & (1 << index) == 0 {
            continue;
        }
        let letter = char::from(b'A' + index);
        let root = format!("{letter}:\\");
        let wide = crate::window::wide(&root);
        // The four kept types are the ones Explorer browses; a RAM disk, a letter whose root
        // no longer exists and an unknown type are not.
        let drive_type = unsafe { GetDriveTypeW(PCWSTR::from_raw(wide.as_ptr())) };
        if !matches!(
            drive_type,
            DRIVE_FIXED | DRIVE_REMOVABLE | DRIVE_REMOTE | DRIVE_CDROM
        ) {
            continue;
        }
        let mut volume = [0u16; 261];
        let queried = unsafe {
            GetVolumeInformationW(
                PCWSTR::from_raw(wide.as_ptr()),
                Some(&mut volume),
                None,
                None,
                None,
                None,
            )
        };
        if queried.is_err() {
            continue;
        }
        let length = volume
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(volume.len());
        let name = String::from_utf16_lossy(&volume[..length]);
        // The letter stays on the row even when the volume has a label: two disks may share
        // a label, and "C:" is what every Windows user navigates by.
        let label = if name.is_empty() {
            format!("{letter}:")
        } else {
            format!("{name} ({letter}:)")
        };
        devices.push(Device {
            path: PathBuf::from(root),
            label: Some(label),
        });
    }
    devices
}

/// The current location of one known folder, or `None` when the profile has none.
fn known_folder(id: &GUID) -> Option<PathBuf> {
    // SAFETY: the id is a GUID constant that outlives the call, and the returned string is
    // allocated by the shell for the caller.
    let path = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }.ok()?;
    // SAFETY: the pointer was returned as NUL-terminated UTF-16 and has not been freed.
    let text = unsafe { path.to_string() }.ok();
    // SAFETY: the whole point of the API is that the caller frees this string.
    unsafe { CoTaskMemFree(Some(path.0 as *const c_void)) };
    text.map(PathBuf::from)
}

/// Thumbnails from the shell's image factory.
pub(crate) struct ShellThumbnailer;

impl Thumbnailer for ShellThumbnailer {
    fn thumbnail(&self, request: &ThumbnailRequest) -> Option<ThumbnailPicture> {
        let path = crate::window::wide(&request.path.to_string_lossy());
        // THUMBNAILONLY is what makes a file without a thumbnail source report `None` here
        // instead of the file icon; BIGGERSIZEOK lets the shell reuse a larger cached bitmap
        // rather than resample one down.
        shell_image(
            PCWSTR::from_raw(path.as_ptr()),
            request.size,
            SIIGBF_THUMBNAILONLY | SIIGBF_RESIZETOFIT | SIIGBF_BIGGERSIZEOK,
        )
    }
}

/// The shell's bitmap for one item at `size`, as `flags` name it: its thumbnail, or its icon.
fn shell_image(path: PCWSTR, requested: u32, flags: SIIGBF) -> Option<ThumbnailPicture> {
    ensure_apartment();
    // SAFETY: the path is NUL-terminated and outlives the call; the item is a COM object
    // this thread owns for the duration of the request.
    let factory: IShellItemImageFactory = unsafe { SHCreateItemFromParsingName(path, None) }.ok()?;
    let size = requested.max(1) as i32;
    let bitmap = unsafe {
        factory.GetImage(
            SIZE {
                cx: size,
                cy: size,
            },
            flags,
        )
    }
    .ok()?;
    // SAFETY: the bitmap is owned by this call; it is deleted below whatever the answer.
    let picture = unsafe { bitmap_picture(bitmap, None) };
    // SAFETY: the shell handed the bitmap over and nothing else will free it.
    let _ = unsafe { DeleteObject(HGDIOBJ(bitmap.0)) };
    picture
}

thread_local! {
    /// Whether this thread has asked COM for an apartment, set on its first shell image.
    static APARTMENT: std::sync::Once = const { std::sync::Once::new() };
}

/// Give this thread an apartment before it creates a shell COM object.
///
/// A registered thumbnail or icon handler is an in-process extension and states
/// `ThreadingModel=Apartment`; a thread starts with no apartment, and the factory refuses to
/// be created without one. The apartment lives as long as the thread, as a worker's does.
fn ensure_apartment() {
    APARTMENT.with(|once| {
        once.call_once(|| {
            let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            // `RPC_E_CHANGED_MODE` means the thread already has the other apartment model;
            // the shell works from either, so that answer is not a failure to report.
            if initialized.is_err() && initialized != RPC_E_CHANGED_MODE {
                log::debug!("ui-platform-win32: thread has no apartment: {initialized:?}");
            }
        });
    });
}

/// Type icons from the system image list.
pub(crate) struct ShellFileIcons;

/// Identity tags keep a shell image-list index from ever colliding with a kind's fallback.
const ID_ITEM: u64 = 1 << 63;
const ID_KIND: u64 = 1 << 62;

impl FileIconProvider for ShellFileIcons {
    fn icon(&self, request: &FileIconRequest) -> Option<FileIcon> {
        self.for_item(request).or_else(|| self.for_kind(request))
    }
}

impl ShellFileIcons {
    /// The shell's own answer for the item: an executable's embedded icon, a shortcut's
    /// resolved target, a folder's `desktop.ini`, and the overlay slot the shell badges it
    /// with — the shortcut arrow, a sync state, a link mark.
    fn for_item(&self, request: &FileIconRequest) -> Option<FileIcon> {
        let path = crate::window::wide(&request.path.to_string_lossy());
        let mut info = SHFILEINFOW::default();
        // `SHGFI_OVERLAYINDEX` packs the overlay slot into the high byte of `iIcon`, and the
        // shell computes it only when it also resolves the icon, so `SHGFI_ICON` rides along.
        let listed = unsafe {
            SHGetFileInfoW(
                PCWSTR::from_raw(path.as_ptr()),
                FILE_ATTRIBUTE_NORMAL,
                Some(&mut info),
                size_of::<SHFILEINFOW>() as u32,
                SHGFI_SYSICONINDEX | SHGFI_SMALLICON | SHGFI_ICON | SHGFI_OVERLAYINDEX,
            )
        };
        if !info.hIcon.is_invalid() {
            // The overlay slot is all this lookup reads; the icon is ours to free.
            let _ = unsafe { DestroyIcon(info.hIcon) };
        }
        if listed == 0 {
            return None;
        }
        // The pixels come from the item's own icon, at the size asked for: an image list's
        // index only names a picture inside the list it was read from, and no `SHGetFileInfoW`
        // flag reads the larger lists the grid asks for.
        let base = shell_image(
            PCWSTR::from_raw(path.as_ptr()),
            request.size,
            SIIGBF_ICONONLY | SIIGBF_RESIZETOFIT | SIIGBF_BIGGERSIZEOK,
        )?;
        let overlay = overlay_slot(info.iIcon);
        let badge = overlay_picture(overlay, request.size).map(|picture| FileBadge {
            picture,
            corner: BadgeCorner::BottomLeft,
        });
        // The slot and the pixels come from separate shell queries, and the shell's answer can
        // change between them: a folder whose `desktop.ini` names an icon reports the plain
        // folder index until it has read the file. Naming the picture by its bytes keeps the
        // share key equal to the picture it names, so two rows cannot trade icons.
        Some(FileIcon {
            identity: ID_ITEM | (picture_hash(&base, overlay) & !(ID_ITEM | ID_KIND)),
            base,
            badge,
        })
    }

    /// The extension's icon, asked of a stand-in name. This is the answer when the item
    /// itself cannot be asked about — a disconnected share, a file that vanished between
    /// the listing and the paint.
    fn for_kind(&self, request: &FileIconRequest) -> Option<FileIcon> {
        let (name, attributes, use_attributes) = icon_source(request.kind)?;
        let path = crate::window::wide(&name);
        let mut info = SHFILEINFOW::default();
        // In the frame thread's apartment: the shell resolves the extension's icon without
        // touching the file, because the attributes are the made-up ones it is given.
        let listed = unsafe {
            SHGetFileInfoW(
                PCWSTR::from_raw(path.as_ptr()),
                attributes,
                Some(&mut info),
                size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON
                    | SHGFI_LARGEICON
                    | if use_attributes {
                        SHGFI_USEFILEATTRIBUTES
                    } else {
                        SHGFI_FLAGS(0)
                    },
            )
        };
        if listed == 0 || info.hIcon.is_invalid() {
            return None;
        }
        // The icon is decoded where it was handed over: a system index names an entry in one
        // list only, and this path has no reason to visit another.
        // SAFETY: the shell allocated the icon for this call and nothing else will free it.
        let base = unsafe { icon_picture(info.hIcon) };
        // SAFETY: the icon belongs to this call.
        let _ = unsafe { DestroyIcon(info.hIcon) };
        Some(FileIcon {
            identity: ID_KIND | request.kind as u64,
            base: base?,
            badge: None,
        })
    }
}

/// The overlay slot `SHGFI_OVERLAYINDEX` leaves in the high byte of `iIcon`; a slot of 0
/// means the shell badges the item with nothing. The rest of `iIcon` is an image-list index.
fn overlay_slot(combined: i32) -> i32 {
    ((combined as u32) >> 24) as i32
}

/// Name one picture by its bytes and the overlay drawn on it, so two items with the same
/// icon upload it once.
fn picture_hash(picture: &ThumbnailPicture, overlay: i32) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    picture.width.hash(&mut hasher);
    picture.height.hash(&mut hasher);
    picture.rgba.hash(&mut hasher);
    overlay.hash(&mut hasher);
    hasher.finish()
}

/// The name the shell looks up for a kind, its attributes, and whether the name is a stand-in.
///
/// A file kind is answered by a name with the right extension, which is what
/// `SHGFI_USEFILEATTRIBUTES` exists for. A folder is answered by the attribute alone. A drive
/// has no extension to stand for it and no generic folder answer would be right, so the system
/// volume is asked about as itself.
fn icon_source(kind: FileKind) -> Option<(String, FILE_FLAGS_AND_ATTRIBUTES, bool)> {
    let (name, attributes) = match kind {
        FileKind::Drive => {
            let root = known_folder(&FOLDERID_Windows)?.parent()?.to_path_buf();
            return Some((
                root.to_string_lossy().into_owned(),
                FILE_ATTRIBUTE_NORMAL,
                false,
            ));
        }
        FileKind::Folder => (String::from("folder"), FILE_ATTRIBUTE_DIRECTORY),
        FileKind::Document => (String::from("file.docx"), FILE_ATTRIBUTE_NORMAL),
        FileKind::Spreadsheet => (String::from("file.xlsx"), FILE_ATTRIBUTE_NORMAL),
        FileKind::Presentation => (String::from("file.pptx"), FILE_ATTRIBUTE_NORMAL),
        FileKind::Pdf => (String::from("file.pdf"), FILE_ATTRIBUTE_NORMAL),
        FileKind::Text => (String::from("file.txt"), FILE_ATTRIBUTE_NORMAL),
        FileKind::Code => (String::from("file.rs"), FILE_ATTRIBUTE_NORMAL),
        FileKind::Image => (String::from("file.png"), FILE_ATTRIBUTE_NORMAL),
        FileKind::Video => (String::from("file.mp4"), FILE_ATTRIBUTE_NORMAL),
        FileKind::Audio => (String::from("file.mp3"), FILE_ATTRIBUTE_NORMAL),
        FileKind::Archive => (String::from("file.zip"), FILE_ATTRIBUTE_NORMAL),
        FileKind::Generic => (String::from("file"), FILE_ATTRIBUTE_NORMAL),
    };
    Some((name, attributes, true))
}

/// The shell's overlay for one slot, in the list whose pictures best fit the base.
///
/// The slot is the value `SHGFI_OVERLAYINDEX` reported; `GetOverlayImage` turns it into the
/// index of the picture inside that list's overlay images, which reads back like any other
/// entry.
fn overlay_picture(overlay: i32, requested: u32) -> Option<ThumbnailPicture> {
    if overlay == 0 {
        return None;
    }
    for images in overlay_lists(requested) {
        // SAFETY: the slot came from this thread's system image list.
        let Ok(index) = (unsafe { images.GetOverlayImage(overlay) }) else {
            continue;
        };
        // SAFETY: the index came from the list's own overlay table.
        let Ok(icon) = (unsafe { images.GetIcon(index, ILD_TRANSPARENT.0) }) else {
            continue;
        };
        // SAFETY: the icon is ours and stays alive until it has been read.
        let picture = unsafe { icon_picture(icon) };
        // SAFETY: the icon was created for this caller; the image list keeps its own copy.
        let _ = unsafe { DestroyIcon(icon) };
        if picture.is_some() {
            return picture;
        }
    }
    None
}

/// The image lists, nearest the requested size first, with one at or below it preferred.
///
/// The sizes are read from each list rather than assumed: the system lists are DPI-scaled, so
/// the small list holds 20-pixel pictures at 125%. A badge is drawn at the size its list gave
/// it, and one larger than the base is cropped, so a larger list only answers when no smaller
/// one does. The jumbo list implements only `IImageList2`, so a refused query falls back.
fn overlay_lists(requested: u32) -> Vec<IImageList> {
    let mut lists = Vec::new();
    for list in [SHIL_SMALL, SHIL_LARGE, SHIL_EXTRALARGE, SHIL_JUMBO] {
        // SAFETY: the image list is owned by the shell.
        let Ok(images) = (unsafe { SHGetImageList::<IImageList>(list as i32) }) else {
            continue;
        };
        let (mut width, mut height) = (0i32, 0i32);
        // SAFETY: both values are writable for the call.
        if unsafe { images.GetIconSize(&mut width, &mut height) }.is_err() {
            continue;
        }
        let size = width.max(height).max(1) as u32;
        let rank = if size <= requested {
            (0u8, requested - size)
        } else {
            (1u8, size - requested)
        };
        lists.push((rank, images));
    }
    lists.sort_by_key(|(rank, _)| *rank);
    lists.into_iter().map(|(_, images)| images).collect()
}

/// The pixels of an icon, reading both the colour bitmap and the mask beneath it.
///
/// SAFETY: `icon` must be a live `HICON` for the duration of the call.
unsafe fn icon_picture(icon: HICON) -> Option<ThumbnailPicture> {
    let mut info = ICONINFO::default();
    // SAFETY: the icon is live and `info` is writable for the whole call.
    unsafe { GetIconInfo(icon, &mut info) }.ok()?;
    let picture = if info.hbmColor.is_invalid() {
        // A monochrome icon keeps both its AND and XOR pictures in the mask, a layout this
        // reader does not take apart; the picker's own mark is better than a wrong colour.
        None
    } else {
        // SAFETY: both bitmaps belong to `info`, which the shell just filled in.
        unsafe { bitmap_picture(info.hbmColor, Some(info.hbmMask)) }
    };
    if !info.hbmColor.is_invalid() {
        // SAFETY: `GetIconInfo` created these bitmaps for the caller.
        let _ = unsafe { DeleteObject(HGDIOBJ(info.hbmColor.0)) };
    }
    if !info.hbmMask.is_invalid() {
        // SAFETY: as above.
        let _ = unsafe { DeleteObject(HGDIOBJ(info.hbmMask.0)) };
    }
    picture
}

/// Convert a bitmap into straight-alpha RGBA8.
///
/// The shell answers in whichever layout the handler chose — 24-bit RGB or 32-bit BGRA — so
/// the depth is read from the bitmap instead of assumed. Both carry straight alpha. `mask` is
/// the icon's AND mask, used only when the colour bitmap has no alpha of its own.
///
/// SAFETY: `bitmap` must be live for the whole call.
unsafe fn bitmap_picture(bitmap: HBITMAP, mask: Option<HBITMAP>) -> Option<ThumbnailPicture> {
    let mut details = BITMAP::default();
    // SAFETY: `details` is writable and the bitmap is live.
    let read = unsafe {
        GetObjectW(
            HGDIOBJ(bitmap.0),
            size_of::<BITMAP>() as i32,
            Some(&mut details as *mut BITMAP as *mut c_void),
        )
    };
    if read == 0 {
        return None;
    }
    let width = details.bmWidth.max(0) as u32;
    // A top-down DIB reports a negative height; the rows below are read top-down either way.
    let height = details.bmHeight.unsigned_abs();
    if width == 0 || height == 0 {
        return None;
    }
    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    };
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    // SAFETY: a memory DC is all `GetDIBits` needs; it is deleted below whatever happens.
    let dc = unsafe { CreateCompatibleDC(None) };
    if dc.is_invalid() {
        return None;
    }
    // SAFETY: `pixels` holds exactly one top-down 32-bit row per requested line.
    let lines = unsafe {
        GetDIBits(
            dc,
            bitmap,
            0,
            height,
            Some(pixels.as_mut_ptr() as *mut c_void),
            &mut header,
            DIB_RGB_COLORS,
        )
    };
    // SAFETY: the DC was created above and nothing else uses it.
    let _ = unsafe { DeleteDC(dc) };
    if lines != height as i32 {
        return None;
    }
    let has_alpha =
        details.bmBitsPixel == 32 && pixels.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0);
    let mask_bits = if has_alpha {
        None
    } else {
        // SAFETY: the mask is live and has the colour bitmap's dimensions when one exists.
        mask.and_then(|mask| unsafe { mask_rows(mask, width, height) })
    };
    straighten(&mut pixels, has_alpha, mask_bits.as_deref(), width as usize);
    Some(ThumbnailPicture {
        width,
        height,
        rgba: pixels,
    })
}

/// The AND mask as one bit per pixel, row-padded to four bytes, MSB first.
///
/// SAFETY: `mask` must be a live monochrome bitmap of `width` by `height`.
unsafe fn mask_rows(mask: HBITMAP, width: u32, height: u32) -> Option<Vec<u8>> {
    let stride = (width as usize).div_ceil(32) * 4;
    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 1,
            biCompression: BI_RGB.0,
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    };
    let mut bits = vec![0u8; stride * height as usize];
    // SAFETY: a memory DC is enough to ask a bitmap for its bits.
    let dc = unsafe { CreateCompatibleDC(None) };
    if dc.is_invalid() {
        return None;
    }
    // SAFETY: `bits` holds one padded 1-bit row per requested line.
    let lines = unsafe {
        GetDIBits(
            dc,
            mask,
            0,
            height,
            Some(bits.as_mut_ptr() as *mut c_void),
            &mut header,
            DIB_RGB_COLORS,
        )
    };
    // SAFETY: the DC was created above.
    let _ = unsafe { DeleteDC(dc) };
    (lines == height as i32).then_some(bits)
}

/// Turn BGRA pixels into straight-alpha RGBA8 in place.
///
/// Windows hands colour over as straight alpha: GDI reads a 32-bit icon's colour as straight
/// and multiplies by its alpha itself, and the image factory's DIB follows the icon format.
/// Only the byte order changes here. `alpha` says the fourth byte is a real channel; `mask` is
/// the AND mask consulted when it is not. A mask bit of one means the pixel is transparent, as
/// the icon format defines.
fn straighten(pixels: &mut [u8], alpha: bool, mask: Option<&[u8]>, width: usize) {
    if alpha {
        for pixel in pixels.as_chunks_mut::<4>().0 {
            if pixel[3] == 0 {
                // A transparent pixel carries no colour; zeroed so bilinear sampling does not
                // pull whatever is under it into its neighbours.
                pixel.fill(0);
                continue;
            }
            pixel.swap(0, 2);
        }
        return;
    }
    let stride = width.div_ceil(32) * 4;
    for (index, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        pixel.swap(0, 2);
        let covered = match mask {
            Some(bits) => {
                let row = index / width;
                let column = index % width;
                let bit = bits[row * stride + column / 8] & (0x80 >> (column % 8));
                bit == 0
            }
            None => true,
        };
        pixel[3] = if covered { 255 } else { 0 };
    }
}

#[cfg(test)]
mod tests {
    use super::{overlay_slot, straighten};

    /// `SHGFI_OVERLAYINDEX` puts the overlay slot in the high byte, where it makes `iIcon`
    /// negative; the slot is still read as unsigned. A slot of 0 is "no overlay".
    #[test]
    fn the_overlay_slot_is_the_high_byte() {
        assert_eq!(overlay_slot(3 << 24 | 0x0012_3456), 3);
        assert_eq!(overlay_slot(0x0012_3456), 0);
        assert_eq!(overlay_slot(0xff << 24 | 0x7), 0xff);
    }

    /// BGRA in, straight-alpha RGBA out: a 24-bit answer gets an opaque alpha, a 32-bit one
    /// keeps the alpha it was given and only swaps red and blue, and a fully transparent
    /// pixel is zeroed rather than left showing its hidden colour.
    #[test]
    fn colour_is_swapped_and_a_transparent_pixel_is_zeroed() {
        let mut opaque = [10, 20, 30, 0];
        straighten(&mut opaque, false, None, 1);
        assert_eq!(opaque, [30, 20, 10, 255]);

        let mut alpha = [10, 20, 30, 255, 32, 64, 128, 128, 1, 2, 3, 0];
        straighten(&mut alpha, true, None, 3);
        assert_eq!(&alpha[0..4], &[30, 20, 10, 255]);
        assert_eq!(&alpha[4..8], &[128, 64, 32, 128]);
        assert_eq!(&alpha[8..12], &[0, 0, 0, 0]);
    }

    /// Legacy icons carry transparency in the AND mask instead of an alpha channel; a set
    /// bit there means the pixel is not drawn.
    #[test]
    fn a_mask_supplies_the_alpha_a_legacy_icon_lacks() {
        let mut pixels = [10, 20, 30, 0, 40, 50, 60, 0];
        // One row of two pixels, padded to four bytes: only the second pixel is masked out.
        let mask = [0b0100_0000, 0, 0, 0];
        straighten(&mut pixels, false, Some(&mask), 2);
        assert_eq!(&pixels[0..4], &[30, 20, 10, 255]);
        assert_eq!(&pixels[4..8], &[60, 50, 40, 0]);
    }
}
