//! `text/uri-list` for file paths.
//!
//! The clipboard and every drag hand a path list over as URI lines: CRLF-separated,
//! percent-encoded `file:` URIs, `#` comments skipped. Win32 converts the same bytes to and
//! from `CF_HDROP`; nothing else should reimplement the parser.

use std::path::PathBuf;

/// The MIME type a file path list travels under.
pub const MIME: &str = "text/uri-list";

/// A line that is not a usable `file:` URI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UriListError {
    /// One-based line number in the list.
    pub line: usize,
    pub reason: &'static str,
}

/// A path list as `text/uri-list` bytes.
///
/// The paths are made absolute first, because a clipboard entry outlives the working
/// directory it was copied in. A path that is not valid UTF-8 is written through
/// `to_string_lossy`; `text/uri-list` has no byte-preserving form.
pub fn encode(paths: &[PathBuf]) -> Vec<u8> {
    let mut list = String::new();
    for path in paths {
        let absolute = std::path::absolute(path).unwrap_or_else(|_| path.clone());
        let raw = absolute.to_string_lossy();
        let (authority, path) = match raw.strip_prefix(r"\\") {
            Some(unc) => match unc.split_once('\\') {
                // The host is the authority, which `file://` already introduces.
                Some((host, rest)) => (host.to_owned(), rest.to_owned()),
                None => (unc.to_owned(), String::new()),
            },
            None => (String::new(), raw.into_owned()),
        };
        let slash = if path.is_empty() { "" } else { "/" };
        list.push_str("file://");
        list.push_str(&authority);
        list.push_str(slash);
        list.push_str(&encode_path(&path));
        list.push_str("\r\n");
    }
    list.into_bytes()
}

/// The paths a `text/uri-list` names, in order.
///
/// Comments and empty lines are skipped; a line that is not a `file:` URI refuses the list
/// instead of being dropped, because a silently shortened list is a paste of the wrong files.
pub fn decode(data: &[u8]) -> Result<Vec<PathBuf>, UriListError> {
    let text = std::str::from_utf8(data).map_err(|_| UriListError {
        line: 0,
        reason: "the list is not UTF-8",
    })?;
    let mut paths = Vec::new();
    for (index, raw) in text.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw).trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let number = index + 1;
        let path = parse_line(line).map_err(|reason| UriListError {
            line: number,
            reason,
        })?;
        paths.push(path);
    }
    Ok(paths)
}

fn parse_line(line: &str) -> Result<PathBuf, &'static str> {
    let rest = line.strip_prefix("file:").ok_or("not a file: URI")?;
    let (authority, path) = if let Some(after) = rest.strip_prefix("//") {
        match after.find('/') {
            Some(at) => (&after[..at], &after[at..]),
            None => (after, ""),
        }
    } else if rest.starts_with('/') {
        ("", rest)
    } else {
        return Err("not a file: URI");
    };
    let path = percent_decode(path).ok_or("a percent escape is not valid UTF-8")?;
    if path.contains('\0') {
        return Err("the path contains NUL");
    }
    let path = if !authority.is_empty() && authority != "localhost" {
        format!("//{authority}{path}")
    } else if drive_path(&path) {
        // `file:///C:/x` names the drive on its own, not a path below the root.
        path[1..].to_owned()
    } else {
        path
    };
    if path.is_empty() {
        return Err("the path is empty");
    }
    Ok(PathBuf::from(path))
}

/// Whether a decoded local path is `/<drive>:/...`, the form a Windows file URI takes.
fn drive_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 4
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
        && bytes[3] == b'/'
}

/// The path's separators made URI ones and every byte a URI does not carry escaped.
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

/// RFC 2483 percent escapes, decoded as the UTF-8 they encode.
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

/// The largest path list a single paste accepts, so a hostile clipboard cannot exhaust memory.
pub const MAX_BYTES: usize = 16 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(raw: &str) {
        let expected = std::path::absolute(PathBuf::from(raw)).expect("absolute path");
        let list = encode(std::slice::from_ref(&expected));
        let paths = decode(&list).expect("a list we wrote decodes");
        assert_eq!(paths, vec![expected]);
    }

    #[test]
    fn a_path_list_round_trips_spaces_percent_and_non_ascii() {
        round_trip("/tmp/Ann Lee/100% done/报告.txt");
        round_trip("/tmp/a b.txt");
    }

    #[test]
    fn comments_and_empty_lines_are_skipped() {
        let list = b"# a comment\r\n\r\nfile:///tmp/a.txt\r\n# another\r\nfile:///tmp/b.txt\r\n";
        let paths = decode(list).expect("the list is valid");
        assert_eq!(paths, vec![PathBuf::from("/tmp/a.txt"), PathBuf::from("/tmp/b.txt")]);
    }

    #[test]
    fn a_line_that_is_not_a_file_uri_is_refused() {
        let error = decode(b"file:///tmp/a.txt\r\nhttps://example.com/\r\n")
            .expect_err("the second line is not a file URI");
        assert_eq!(error.line, 2);
    }

    #[test]
    fn a_windows_drive_uri_loses_its_leading_slash() {
        let paths = decode(b"file:///C:/Users/Ann/a.txt").expect("valid");
        assert_eq!(paths, vec![PathBuf::from("C:/Users/Ann/a.txt")]);
    }

    #[test]
    fn a_remote_authority_becomes_a_unc_or_network_path() {
        let paths = decode(b"file://server/share/a.txt").expect("valid");
        assert_eq!(paths, vec![PathBuf::from("//server/share/a.txt")]);
    }

    #[test]
    fn an_invalid_percent_escape_is_refused() {
        assert!(decode(b"file:///tmp/%zz").is_err());
    }
}
