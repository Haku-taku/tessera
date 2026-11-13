//! The system clipboard: the Windows formats each engine content kind becomes.
//!
//! Windows pastes a format, not a MIME type, and the applications that read a picture want a
//! DIB while a browser takes the PNG, so one payload is offered as several formats at once.

use ui_core::clipboard::ClipboardContent;
use windows::Win32::Foundation::{HANDLE, HWND, GlobalFree};
use windows::Win32::Graphics::Gdi::{BI_BITFIELDS, BI_RGB, BITMAPINFOHEADER, BITMAPV5HEADER};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::{CF_DIB, CF_DIBV5, CF_HDROP, CF_UNICODETEXT};
use windows::Win32::UI::Shell::DROPFILES;
use windows::core::PCWSTR;

/// Put `content` on the clipboard, replacing what was there.
///
/// False when no Windows format carries the payload, or when the clipboard refused it.
pub(crate) fn write(owner: Option<HWND>, content: &ClipboardContent) -> bool {
    let Some(formats) = formats(content) else {
        return false;
    };
    unsafe {
        if OpenClipboard(owner).is_err() {
            return false;
        }
        let mut stored = false;
        if EmptyClipboard().is_ok() {
            stored = true;
            // Every format is offered: the shell refusing one must not cost the others.
            for (format, bytes) in formats {
                stored &= store(format, &bytes);
            }
        }
        // Closing is the copy's real commit; a failure leaves the old clipboard intact.
        let _ = CloseClipboard();
        stored
    }
}

/// The clipboard's text; `None` when it holds none.
pub(crate) fn read_text(owner: Option<HWND>) -> Option<String> {
    unsafe {
        if OpenClipboard(owner).is_err() {
            return None;
        }
        let text = clipboard_text();
        let _ = CloseClipboard();
        text
    }
}

/// The formats one payload is published under.
fn formats(content: &ClipboardContent) -> Option<Vec<(u32, Vec<u8>)>> {
    match content {
        ClipboardContent::Text(text) => Some(vec![(CF_UNICODETEXT.0 as u32, utf16_bytes(text))]),
        ClipboardContent::Bytes { mime, data } => match mime.as_str() {
            "image/png" => png_formats(data),
            "text/html" => html_formats(data),
            "text/uri-list" => uri_formats(data),
            // No other MIME type has a Windows format this backend can produce.
            _ => None,
        },
    }
}

/// A NUL-terminated UTF-16 string, as `CF_UNICODETEXT` stores it.
fn utf16_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_ne_bytes)
        .collect()
}

/// A PNG is pasted as PNG by browsers and as a DIB by Explorer, Word and Paint; the two DIB
/// headers differ only in what they can say about alpha and colour space.
fn png_formats(data: &[u8]) -> Option<Vec<(u32, Vec<u8>)>> {
    // Bytes that do not decode are not a picture, so they take no DIB with them.
    let image = image::load_from_memory_with_format(data, image::ImageFormat::Png).ok()?;
    let image = image.to_rgba8();
    let width = i32::try_from(image.width()).ok()?;
    let height = i32::try_from(image.height()).ok()?;
    let pixels = bgra_bottom_up(&image);
    Some(vec![
        (registered("PNG")?, data.to_vec()),
        (CF_DIBV5.0 as u32, dib_v5(width, height, &pixels)),
        (CF_DIB.0 as u32, dib(width, height, &pixels)),
    ])
}

/// The pixels in a DIB's order: the bottom row first, each pixel blue, green, red, alpha.
fn bgra_bottom_up(image: &image::RgbaImage) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(image.as_raw().len());
    for row in image.rows().rev() {
        for pixel in row {
            pixels.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    pixels
}

/// `CF_DIBV5`: a `BITMAPV5HEADER`, the pixels, and no file header.
fn dib_v5(width: i32, height: i32, pixels: &[u8]) -> Vec<u8> {
    let header = BITMAPV5HEADER {
        bV5Size: std::mem::size_of::<BITMAPV5HEADER>() as u32,
        bV5Width: width,
        // Positive: a DIB stores its rows bottom-up, which is the order the pixels are in.
        bV5Height: height,
        bV5Planes: 1,
        bV5BitCount: 32,
        bV5Compression: BI_BITFIELDS,
        bV5SizeImage: pixels.len() as u32,
        bV5RedMask: 0x00ff_0000,
        bV5GreenMask: 0x0000_ff00,
        bV5BlueMask: 0x0000_00ff,
        bV5AlphaMask: 0xff00_0000,
        // LCS_sRGB, the FOURCC 'sRGB', which the bindings do not name.
        bV5CSType: 0x7352_4742,
        ..Default::default()
    };
    let mut block = as_bytes(&header).to_vec();
    block.extend_from_slice(pixels);
    block
}

/// `CF_DIB`: the older `BITMAPINFOHEADER` block, which carries no masks and no colour space.
fn dib(width: i32, height: i32, pixels: &[u8]) -> Vec<u8> {
    let header = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        biHeight: height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        biSizeImage: pixels.len() as u32,
        ..Default::default()
    };
    let mut block = as_bytes(&header).to_vec();
    block.extend_from_slice(pixels);
    block
}

/// `HTML Format`: the header of byte offsets and the fragment marked inside a document.
fn html_formats(data: &[u8]) -> Option<Vec<(u32, Vec<u8>)>> {
    let fragment = std::str::from_utf8(data).ok()?;
    Some(vec![(registered("HTML Format")?, cf_html(fragment))])
}

/// The block Office and Word read: offsets counted from its first byte, header included.
fn cf_html(fragment: &str) -> Vec<u8> {
    const OPENING: &str = "<html><body>\n<!--StartFragment-->";
    const CLOSING: &str = "<!--EndFragment-->\n</body></html>";
    // The header's length is part of the offsets it carries, so every offset is written ten
    // wide and the length can be measured before any of them is known.
    let start_html = header(0, 0, 0, 0).len();
    let start_fragment = start_html + OPENING.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + CLOSING.len();
    let mut block = header(start_html, end_html, start_fragment, end_fragment).into_bytes();
    block.extend_from_slice(OPENING.as_bytes());
    block.extend_from_slice(fragment.as_bytes());
    block.extend_from_slice(CLOSING.as_bytes());
    block
}

/// The `CF_HTML` header: four byte offsets, each ten digits so the header's own length is
/// independent of the values it carries.
fn header(start_html: usize, end_html: usize, start_fragment: usize, end_fragment: usize) -> String {
    format!(
        "Version:0.9\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\n\
         StartFragment:{start_fragment:010}\r\nEndFragment:{end_fragment:010}\r\n"
    )
}

/// A uri-list of `file:` URIs as `CF_HDROP`: paths, because the format carries no contents.
fn uri_formats(data: &[u8]) -> Option<Vec<(u32, Vec<u8>)>> {
    let list = std::str::from_utf8(data).ok()?;
    let paths: Vec<String> = list.lines().filter_map(file_path).collect();
    if paths.is_empty() {
        return None;
    }
    Some(vec![(CF_HDROP.0 as u32, hdrop(&paths))])
}

/// One uri-list line as a Windows path; `None` for a comment, an empty line, or anything
/// that is not a `file:` URI.
fn file_path(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let rest = line.strip_prefix("file://")?;
    let (path, unc) = match rest.strip_prefix('/') {
        Some(local) => (local, false),
        // `file://server/share` is another machine, which a UNC path names with two leading
        // backslashes.
        None => (rest, true),
    };
    let path = percent_decode(path)?;
    if path.is_empty() || path.contains('\0') {
        // An empty or NUL-bearing path would end the list early and leave the rest unread.
        return None;
    }
    let path = path.replace('/', "\\");
    Some(if unc { format!("\\\\{path}") } else { path })
}

/// RFC 2483 percent-escapes, decoded as the UTF-8 they encode.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

/// The `DROPFILES` header and its wide, double-NUL-terminated path list.
fn hdrop(paths: &[String]) -> Vec<u8> {
    let header = DROPFILES {
        pFiles: std::mem::size_of::<DROPFILES>() as u32,
        // Wide, so a path with a character outside the ANSI code page arrives whole.
        fWide: true.into(),
        ..Default::default()
    };
    let mut block = as_bytes(&header).to_vec();
    for path in paths {
        for unit in path.encode_utf16().chain(std::iter::once(0)) {
            block.extend_from_slice(&unit.to_ne_bytes());
        }
    }
    // The list ends with a NUL of its own, after the last path's terminator.
    block.extend_from_slice(&[0, 0]);
    block
}

/// A clipboard format registered by name, such as `"PNG"`; `None` when the shell refuses.
fn registered(name: &str) -> Option<u32> {
    let name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let format = unsafe { RegisterClipboardFormatW(PCWSTR::from_raw(name.as_ptr())) };
    (format != 0).then_some(format)
}

/// Hand `bytes` to the clipboard as `format`; the clipboard owns the block on success.
unsafe fn store(format: u32, bytes: &[u8]) -> bool {
    unsafe {
        let Ok(handle) = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) else {
            return false;
        };
        let target = GlobalLock(handle);
        if target.is_null() {
            let _ = GlobalFree(Some(handle));
            return false;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), target.cast::<u8>(), bytes.len());
        let _ = GlobalUnlock(handle);
        match SetClipboardData(format, Some(HANDLE(handle.0))) {
            // On success the clipboard owns the block and frees it itself.
            Ok(_) => true,
            Err(_) => {
                let _ = GlobalFree(Some(handle));
                false
            }
        }
    }
}

/// The clipboard's `CF_UNICODETEXT`, read while the clipboard is open.
unsafe fn clipboard_text() -> Option<String> {
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_err() {
            return None;
        }
        let handle = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
        let block = windows::Win32::Foundation::HGLOBAL(handle.0);
        let pointer = GlobalLock(block);
        if pointer.is_null() {
            return None;
        }
        let text = pointer.cast::<u16>();
        // Bounded by the block the clipboard handed over: nothing guarantees a terminator
        // inside it, and scanning past the end is the bug this bound exists to prevent.
        let capacity = GlobalSize(block) / 2;
        let mut length = 0usize;
        while length < capacity && *text.add(length) != 0 {
            length += 1;
        }
        let slice = std::slice::from_raw_parts(text, length);
        let text = String::from_utf16_lossy(slice);
        let _ = GlobalUnlock(block);
        Some(text)
    }
}

/// The bytes a `repr(C)` header struct begins a DIB with.
fn as_bytes<T>(value: &T) -> &[u8] {
    // SAFETY: the DIB headers are plain `repr(C)` integers, so every byte of one is
    // initialised and reading it as a byte slice cannot observe another type's padding.
    unsafe { std::slice::from_raw_parts((value as *const T).cast::<u8>(), std::mem::size_of::<T>()) }
}
