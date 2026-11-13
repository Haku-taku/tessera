//! The system clipboard: the Windows formats each engine content kind becomes, and back.
//!
//! Windows pastes a format, not a MIME type, and the applications that read a picture want a
//! DIB while a browser takes the PNG, so one payload is offered as several formats at once.
//! A read walks the same table backwards: the formats present become MIME types, and a DIB
//! becomes a PNG because the app reads one picture MIME whichever application copied it.

use image::ImageEncoder;
use ui_core::clipboard::ClipboardContent;
use ui_platform::transfer::{Selection, SelectionReader};
use windows::Win32::Foundation::{HANDLE, HWND, GlobalFree};
use windows::Win32::Graphics::Gdi::{BI_BITFIELDS, BI_RGB, BITMAPINFOHEADER, BITMAPV5HEADER};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::{CF_DIB, CF_DIBV5, CF_HDROP, CF_UNICODETEXT};
use windows::Win32::UI::Shell::{DROPFILES, DragQueryFileW, HDROP};
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

/// The system clipboard as the engine's selection reader, for a command that pastes.
///
/// Windows copies its formats in whole rather than holding them for a peer to pull, so a
/// read finishes inside the call that starts it; `poll` only hands over what was copied.
/// The payload is held until `poll` takes it, because the command answers between turns.
#[derive(Default)]
pub(crate) struct ClipboardReader {
    pending: Option<Selection>,
}

impl SelectionReader for ClipboardReader {
    fn offers(&self) -> Vec<String> {
        let formats = clipboard_formats();
        mimes(&formats).into_iter().map(str::to_owned).collect()
    }

    fn read(&mut self, mime: &str) -> bool {
        let Some(data) = read_payload(mime) else {
            return false;
        };
        self.pending = Some(Selection {
            mime: mime.to_owned(),
            data,
        });
        true
    }

    fn poll(&mut self) -> Option<Selection> {
        self.pending.take()
    }
}

/// The clipboard's bytes for one MIME, converted from the Windows format that carries it.
///
/// One open covers the whole conversion: the block belongs to the application that copied
/// it, and a copy arriving between two opens would mix two payloads.
fn read_payload(mime: &str) -> Option<Vec<u8>> {
    unsafe {
        if OpenClipboard(None).is_err() {
            return None;
        }
        let payload = match mime {
            "text/uri-list" => clipboard_paths().map(uri_list),
            "image/png" => clipboard_picture(),
            "text/plain" => clipboard_text().map(String::into_bytes),
            _ => None,
        };
        let _ = CloseClipboard();
        payload
    }
}

/// Every format the clipboard currently holds, in the order it lists them.
fn clipboard_formats() -> Vec<u32> {
    unsafe {
        if OpenClipboard(None).is_err() {
            return Vec::new();
        }
        let mut formats = Vec::new();
        let mut format = EnumClipboardFormats(0);
        while format != 0 {
            formats.push(format);
            format = EnumClipboardFormats(format);
        }
        let _ = CloseClipboard();
        formats
    }
}

/// The MIME types those Windows formats offer, most specific first.
///
/// A picture is one MIME however many formats carry it: an application that writes both
/// `"PNG"` and a DIB copied one picture, and `read` decides which of them to hand over.
fn mimes(formats: &[u32]) -> Vec<&'static str> {
    let has = |format: u32| formats.contains(&format);
    let mut found = Vec::new();
    if has(CF_HDROP.0 as u32) {
        found.push("text/uri-list");
    }
    if registered("PNG").is_some_and(has) || has(CF_DIBV5.0 as u32) || has(CF_DIB.0 as u32) {
        found.push("image/png");
    }
    if has(CF_UNICODETEXT.0 as u32) {
        found.push("text/plain");
    }
    found
}

/// The paths a `CF_HDROP` block names, read by the shell, which owns that format's layout.
fn clipboard_paths() -> Option<Vec<String>> {
    unsafe {
        let handle = GetClipboardData(CF_HDROP.0 as u32).ok()?;
        let drop = HDROP(handle.0);
        let count = DragQueryFileW(drop, u32::MAX, None);
        let mut paths = Vec::with_capacity(count as usize);
        for index in 0..count {
            let length = DragQueryFileW(drop, index, None) as usize;
            let mut buffer = vec![0u16; length + 1];
            DragQueryFileW(drop, index, Some(&mut buffer));
            paths.push(String::from_utf16_lossy(&buffer[..length]));
        }
        // A block that names no path is not a file list, whatever else it holds.
        (!paths.is_empty()).then_some(paths)
    }
}

/// The copied paths as a `text/uri-list`, the Web's file-list form: one `file:` URI a line.
fn uri_list(paths: Vec<String>) -> Vec<u8> {
    let mut list = String::new();
    for path in paths {
        list.push_str(&file_uri(&path));
        list.push_str("\r\n");
    }
    list.into_bytes()
}

/// One Windows path as a `file:` URI.
///
/// `\\server\share\x` is another machine's share, which a URI names as its host; every other
/// path has an empty authority, so its own path starts after the third slash.
fn file_uri(path: &str) -> String {
    let (authority, path) = match path.strip_prefix(r"\\") {
        Some(unc) => match unc.split_once('\\') {
            Some((host, rest)) => (format!("//{host}"), rest.to_owned()),
            // A share with no path in it still names the share, which has a URI form of its own.
            None => (format!("//{unc}"), String::new()),
        },
        None => ("//".to_owned(), path.to_owned()),
    };
    let slash = if path.is_empty() { "" } else { "/" };
    format!("file:{authority}{slash}{}", encode_path(&path))
}

/// The path's separators made URI ones and every byte a URI does not carry escaped.
///
/// The colon after a drive letter is a path character under RFC 3986 and stays as it is; a
/// space, a `%` or any non-ASCII byte is escaped from its UTF-8 encoding.
fn encode_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'\\' => encoded.push('/'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

/// The clipboard's picture as PNG: `"PNG"` already is one, and a DIB is decoded into one.
fn clipboard_picture() -> Option<Vec<u8>> {
    if let Some(png) = registered("PNG").and_then(block) {
        return Some(png);
    }
    let dib = block(CF_DIBV5.0 as u32).or_else(|| block(CF_DIB.0 as u32))?;
    match dib_to_png(&dib) {
        Ok(png) => Some(png),
        Err(error) => {
            // The command asked for a picture and cannot have it; say why it could not.
            log::warn!("ui-platform-win32: {error}");
            None
        }
    }
}

/// Decode a device-independent bitmap, header first and no file header, into PNG bytes.
///
/// The block gets the `BITMAPFILEHEADER` Windows leaves off, because that header is where a
/// BMP reader learns where the pixels start. Nothing is copied twice: the DIB's own header,
/// masks and palette follow the fourteen bytes prepended to them.
fn dib_to_png(dib: &[u8]) -> Result<Vec<u8>, String> {
    let pixels = dib_pixels(dib).ok_or("the copied bitmap does not begin with a bitmap header")?;
    let mut bmp = Vec::with_capacity(14 + dib.len());
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(
        &u32::try_from(14 + dib.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    bmp.extend_from_slice(&[0; 4]);
    let pixels = u32::try_from(14 + pixels).unwrap_or(u32::MAX);
    bmp.extend_from_slice(&pixels.to_le_bytes());
    bmp.extend_from_slice(dib);
    let decoder = image::codecs::bmp::BmpDecoder::new(std::io::Cursor::new(bmp))
        .map_err(|error| format!("the copied bitmap does not decode: {error}"))?;
    let image = image::DynamicImage::from_decoder(decoder)
        .map_err(|error| format!("the copied bitmap does not decode: {error}"))?
        .into_rgba8();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|error| format!("the copied bitmap does not encode as PNG: {error}"))?;
    Ok(png)
}

/// Where the DIB's pixels start: after its header, the colour masks and the palette.
///
/// A 40-byte info header has no room for bit masks, so `BI_BITFIELDS` puts them right after
/// it; the later headers carry them inside themselves, and only a paletted bitmap has a
/// palette at all.
fn dib_pixels(dib: &[u8]) -> Option<usize> {
    let header = field(dib, 0)? as usize;
    if header < 12 {
        return None;
    }
    // The core header states its bit count 16 bits in and has no compression or palette size.
    if header == 12 {
        let bits = u16::from_le_bytes([*dib.get(10)?, *dib.get(11)?]) as usize;
        let palette = if bits <= 8 { 1 << bits } else { 0 };
        return Some(header + palette * 3);
    }
    let bits = u16::from_le_bytes([*dib.get(14)?, *dib.get(15)?]) as usize;
    let compression = field(dib, 16)?;
    let used = field(dib, 32)? as usize;
    let palette = if bits <= 8 {
        if used == 0 { 1 << bits } else { used }
    } else {
        0
    };
    let masks = if compression == 3 && header == 40 {
        12
    } else {
        0
    };
    Some(header + masks + palette * 4)
}

/// A little-endian 32-bit field, the only width a bitmap header's numbers use.
fn field(bytes: &[u8], offset: usize) -> Option<u32> {
    let bytes = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// The clipboard's bytes for one format, copied while the clipboard is open.
///
/// `None` when the format is not there, or when the block cannot be locked or sized.
fn block(format: u32) -> Option<Vec<u8>> {
    unsafe {
        let handle = GetClipboardData(format).ok()?;
        let memory = windows::Win32::Foundation::HGLOBAL(handle.0);
        let pointer = GlobalLock(memory);
        if pointer.is_null() {
            return None;
        }
        let length = GlobalSize(memory);
        let bytes = std::slice::from_raw_parts(pointer.cast::<u8>(), length).to_vec();
        let _ = GlobalUnlock(memory);
        Some(bytes)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2x2 fixture: red and green over blue and white, and one pixel that only `CF_DIBV5`
    /// can carry the alpha of.
    fn picture() -> image::RgbaImage {
        let mut picture = image::RgbaImage::new(2, 2);
        picture.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        picture.put_pixel(1, 0, image::Rgba([0, 255, 0, 255]));
        picture.put_pixel(0, 1, image::Rgba([0, 0, 255, 255]));
        picture.put_pixel(1, 1, image::Rgba([255, 255, 255, 128]));
        picture
    }

    fn png_of(picture: &image::RgbaImage) -> Vec<u8> {
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(
                picture.as_raw(),
                picture.width(),
                picture.height(),
                image::ExtendedColorType::Rgba8,
            )
            .expect("the fixture encodes");
        png
    }

    fn decoded(png: &[u8]) -> image::RgbaImage {
        image::load_from_memory_with_format(png, image::ImageFormat::Png)
            .expect("the read side encodes a PNG")
            .to_rgba8()
    }

    /// A picture copied to a DIB is read back from one, because the clipboard's picture is
    /// whichever format the application chose to write. Row order, channel order and the
    /// alpha a `CF_DIB` cannot carry are what this pins.
    #[test]
    fn a_written_dib_reads_back_as_the_same_picture() {
        let picture = picture();
        let formats = png_formats(&png_of(&picture)).expect("a PNG is a picture");
        let dib = |wanted: u32| {
            formats
                .iter()
                .find(|(format, _)| *format == wanted)
                .map(|(_, bytes)| bytes)
                .expect("the write side offers both DIB headers")
        };

        assert_eq!(
            decoded(&dib_to_png(dib(CF_DIBV5.0 as u32)).expect("the V5 DIB decodes")),
            picture
        );

        // `CF_DIB`'s words have no alpha channel at all, so an application reading one sees
        // the picture, not a hole where the alpha byte was left at zero.
        let mut opaque = picture.clone();
        for pixel in opaque.pixels_mut() {
            pixel[3] = 255;
        }
        assert_eq!(
            decoded(&dib_to_png(dib(CF_DIB.0 as u32)).expect("the DIB decodes")),
            opaque
        );
    }

    /// A copied file is handed over as `CF_HDROP`, whose paths become the Web's file-list
    /// form; a path the URI cannot spell put another file on the paste.
    #[test]
    fn a_copied_path_becomes_a_file_uri() {
        assert_eq!(
            file_uri(r"C:\Users\a b\shot #1.png"),
            "file:///C:/Users/a%20b/shot%20%231.png"
        );
        assert_eq!(
            file_uri(r"\\build\share\one.png"),
            "file://build/share/one.png"
        );
        let list = uri_list(vec![r"C:\one.png".into(), r"\\build\share\two.png".into()]);
        assert_eq!(
            String::from_utf8(list).expect("a URI list is text"),
            "file:///C:/one.png\r\nfile://build/share/two.png\r\n"
        );
    }
}
