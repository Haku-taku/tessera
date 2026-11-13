//! What one clipboard copy puts on Windows for each engine content kind, read back through
//! the shell rather than through this crate's own encoder.
//!
//! Run: `cargo run -p ui-platform-win32 --example clipboard`
//!
//! The probe restores the clipboard's text afterwards, because it replaces whatever the user
//! had copied; a non-text clipboard cannot be restored and is reported.

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_main() {
        eprintln!("clipboard: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the Win32 backend is Windows only");
}

#[cfg(windows)]
use windows::Win32::Foundation::HWND;

#[cfg(windows)]
fn windows_main() -> Result<(), String> {
    use std::sync::Arc;

    use image::ImageEncoder;
    use ui_core::clipboard::ClipboardContent;
    use ui_platform::window::SurfaceSpec;
    use ui_platform_win32::Win32Window;
    use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
    use windows::Win32::System::Ole::{CF_DIB, CF_DIBV5, CF_UNICODETEXT};
    use windows::core::PCWSTR;

    let expect = |pattern: &str, found: bool| {
        println!("{pattern:64} {}", if found { "OK" } else { "FAIL" });
        found
    };
    let mut ok = true;

    let mut window = Win32Window::new(SurfaceSpec::window(
        "crabsnap-win32-clipboard",
        "crabsnap — clipboard probe",
        (320, 200),
    ))
    .map_err(|error| error.to_string())?;
    let hwnd = window.hwnd();
    let kept = block(hwnd, CF_UNICODETEXT.0 as u32).and_then(|bytes| utf16(&bytes));

    let format = |name: &str| -> u32 {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe { RegisterClipboardFormatW(PCWSTR::from_raw(wide.as_ptr())) }
    };
    let png_format = format("PNG");
    let html_format = format("HTML Format");

    // Text: the one format the backend carried before, kept as the regression.
    let text = "clipboard probe — 文本";
    ok &= expect(
        "a text copy is written",
        window.write_clipboard(ClipboardContent::Text(text.to_owned())),
    );
    ok &= expect(
        "the text reads back through CF_UNICODETEXT",
        block(hwnd, CF_UNICODETEXT.0 as u32)
            .and_then(|bytes| utf16(&bytes))
            .is_some_and(|read| read == text),
    );

    // HTML: the fragment is marked inside a document, and the offsets point at it.
    let fragment = "<b>frag</b> — 片段";
    ok &= expect(
        "an HTML copy is written",
        window.write_clipboard(ClipboardContent::Bytes {
            mime: String::from("text/html"),
            data: Arc::new(fragment.as_bytes().to_vec()),
        }),
    );
    ok &= expect(
        "HTML Format carries the fragment between its own offsets",
        block(hwnd, html_format).is_some_and(|bytes| {
            let Ok(document) = String::from_utf8(bytes) else {
                return false;
            };
            let start = offset(&document, "StartFragment:");
            let end = offset(&document, "EndFragment:");
            let html_end = offset(&document, "EndHTML:");
            start
                .zip(end)
                .is_some_and(|(start, end)| document.get(start..end) == Some(fragment))
                && html_end == Some(document.len())
        }),
    );

    // A file list: two file URIs, one escaped, plus a comment and a URL that cannot be a path.
    let list = "file:///C:/Windows/win.ini\r\n# a comment\r\nfile:///C:/a%20b.txt\r\nhttps://example.com/x\r\n";
    ok &= expect(
        "a uri-list copy is written",
        window.write_clipboard(ClipboardContent::Bytes {
            mime: String::from("text/uri-list"),
            data: Arc::new(list.as_bytes().to_vec()),
        }),
    );
    let paths = hdrop_paths(hwnd);
    println!("  CF_HDROP: {paths:?}");
    ok &= expect(
        "CF_HDROP holds the two file paths, decoded and unescaped",
        paths
            == [
                String::from("C:\\Windows\\win.ini"),
                String::from("C:\\a b.txt"),
            ],
    );

    // A picture: four known pixels, so the DIB's channel order and row order are checkable.
    let mut picture = image::RgbaImage::new(2, 2);
    picture.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
    picture.put_pixel(1, 0, image::Rgba([0, 255, 0, 255]));
    picture.put_pixel(0, 1, image::Rgba([0, 0, 255, 255]));
    picture.put_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(picture.as_raw(), 2, 2, image::ExtendedColorType::Rgba8)
        .map_err(|error| error.to_string())?;
    let png = Arc::new(png);
    ok &= expect(
        "a PNG copy is written",
        window.write_clipboard(ClipboardContent::Bytes {
            mime: String::from("image/png"),
            data: Arc::clone(&png),
        }),
    );
    ok &= expect(
        "the PNG format carries the bytes unchanged",
        block(hwnd, png_format).is_some_and(|bytes| bytes == *png),
    );

    // A DIB stores its rows bottom-up, so the first pixel in the block is the bottom-left
    // one — blue here — and a byte swap of the channels would show as red instead.
    // Bottom row first, each pixel blue, green, red, alpha: the blue pixel the picture has at
    // its lower left, then white; above them red, then green.
    let pixels = [
        255u8, 0, 0, 255, 255, 255, 255, 255, 0, 0, 255, 255, 0, 255, 0, 255,
    ];
    let v5 = block(hwnd, CF_DIBV5.0 as u32);
    ok &= expect(
        "CF_DIBV5 is a 124-byte V5 header, 32-bit and bitfield-masked",
        v5.as_ref().is_some_and(|bytes| {
            word(bytes, 0) == Some(124)
                && word(bytes, 4) == Some(2)
                && word(bytes, 8) == Some(2)
                && half(bytes, 12) == Some(1)
                && half(bytes, 14) == Some(32)
                && word(bytes, 16) == Some(3)
                && word(bytes, 40) == Some(0x00ff_0000)
                && word(bytes, 52) == Some(0xff00_0000)
        }),
    );
    ok &= expect(
        "CF_DIBV5 carries the picture's pixels bottom row first, blue before red",
        v5.as_ref()
            .and_then(|bytes| bytes.get(124..140))
            .is_some_and(|block| block == pixels),
    );
    let dib = block(hwnd, CF_DIB.0 as u32);
    ok &= expect(
        "CF_DIB is the older 40-byte header with the same pixels",
        dib.as_ref().is_some_and(|bytes| {
            word(bytes, 0) == Some(40)
                && word(bytes, 4) == Some(2)
                && word(bytes, 8) == Some(2)
                && half(bytes, 12) == Some(1)
                && half(bytes, 14) == Some(32)
                && word(bytes, 16) == Some(0)
                && bytes.get(40..56).is_some_and(|block| block == pixels)
        }),
    );

    if let Some(kept) = kept {
        let restored = window.write_clipboard(ClipboardContent::Text(kept));
        println!("the earlier clipboard text was {}", if restored { "restored" } else { "lost" });
    }
    if !ok {
        return Err(String::from("one or more probes failed"));
    }
    println!("PASS: text, HTML, file list and picture formats");
    Ok(())
}

/// The clipboard's bytes for one format, while the probe owns nothing else.
#[cfg(windows)]
fn block(hwnd: HWND, format: u32) -> Option<Vec<u8>> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    unsafe {
        OpenClipboard(Some(hwnd)).ok()?;
        let block = (|| {
            let handle = GetClipboardData(format).ok()?;
            let memory = HGLOBAL(handle.0);
            let pointer = GlobalLock(memory);
            if pointer.is_null() {
                return None;
            }
            let length = GlobalSize(memory);
            let bytes = std::slice::from_raw_parts(pointer.cast::<u8>(), length).to_vec();
            let _ = GlobalUnlock(memory);
            Some(bytes)
        })();
        let _ = CloseClipboard();
        block
    }
}

/// A `CF_UNICODETEXT` block as text.
#[cfg(windows)]
fn utf16(bytes: &[u8]) -> Option<String> {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .take_while(|unit| *unit != 0)
        .collect();
    String::from_utf16(&units).ok()
}

/// The number after `key` in the `CF_HTML` header.
#[cfg(windows)]
fn offset(header: &str, key: &str) -> Option<usize> {
    let start = header.find(key)? + key.len();
    header.get(start..start + 10)?.trim().parse().ok()
}

/// A little-endian 16-bit field at `offset`; a DIB header mixes these in beside the words.
#[cfg(windows)]
fn half(bytes: &[u8], offset: usize) -> Option<u16> {
    let field = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([field[0], field[1]]))
}

/// A little-endian 32-bit field at `offset`, which is the only way a DIB header is read.
#[cfg(windows)]
fn word(bytes: &[u8], offset: usize) -> Option<u32> {
    let field = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([field[0], field[1], field[2], field[3]]))
}

/// The paths the shell itself reads out of a `CF_HDROP` block.
#[cfg(windows)]
fn hdrop_paths(hwnd: HWND) -> Vec<String> {
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows::Win32::System::Ole::CF_HDROP;
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

    unsafe {
        if OpenClipboard(Some(hwnd)).is_err() {
            return Vec::new();
        }
        let mut paths = Vec::new();
        if let Ok(handle) = GetClipboardData(CF_HDROP.0 as u32) {
            let drop = HDROP(handle.0);
            let count = DragQueryFileW(drop, u32::MAX, None);
            for index in 0..count {
                let length = DragQueryFileW(drop, index, None) as usize;
                let mut buffer = vec![0u16; length + 1];
                DragQueryFileW(drop, index, Some(&mut buffer));
                paths.push(String::from_utf16_lossy(&buffer[..length]));
            }
        }
        let _ = CloseClipboard();
        paths
    }
}
