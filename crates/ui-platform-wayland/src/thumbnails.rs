//! Pictures from the freedesktop thumbnail system: its cache first, then the `.thumbnailer`
//! program the desktop ships for the file's type. Nothing here decodes a *file* — the codecs
//! belong to the desktop's thumbnailers, so this crate bundles none of them.
//!
//! The cache is shared with every file manager on the machine, so an entry is trusted only
//! while its `Thumb::MTime` matches the file and its `Thumb::Size` (when present) matches the
//! length; what this module writes carries the same chunks and is renamed into place.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::BufWriter;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant, UNIX_EPOCH};

use md5::{Digest, Md5};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_encode};
use ui_platform::thumbnails::{ThumbnailPicture, ThumbnailRequest, Thumbnailer};

/// The standard's four tiers, by directory name and the size of the longer side.
const TIERS: [(&str, u32); 4] = [
    ("normal", 128),
    ("large", 256),
    ("x-large", 512),
    ("xx-large", 1024),
];

/// A thumbnailer that has not answered by now is killed: the loop is not waiting, but the
/// worker is, and a stuck converter would starve every picture behind it.
const DEADLINE: Duration = Duration::from_secs(10);

/// Every byte a URI may carry unescaped: the unreserved set plus the separators a path needs.
const URI: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b'/');

/// The desktop's thumbnail system, as much of it as one machine can have.
pub struct DesktopThumbnailer {
    cache: Option<PathBuf>,
    /// `TryExec`/`Exec`/`MimeType` entries, read once on the worker.
    programs: OnceLock<Vec<Program>>,
}

impl Default for DesktopThumbnailer {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopThumbnailer {
    pub fn new() -> Self {
        Self {
            cache: cache_home(),
            programs: OnceLock::new(),
        }
    }

    fn programs(&self) -> &[Program] {
        self.programs.get_or_init(discover)
    }

    /// Make the picture with the first thumbnailer that claims the file's type.
    fn generate(
        &self,
        path: &Path,
        uri: &str,
        mime: Option<&str>,
        side: u32,
    ) -> Option<ThumbnailPicture> {
        if let Some(mime) = mime
            && let Some(program) = self
                .programs()
                .iter()
                .find(|program| program.mime_types.iter().any(|own| own == mime))
        {
            if let Some(picture) = run_program(program, path, uri, side) {
                return Some(picture);
            }
            log::debug!("wayland: {} produced no thumbnail for {path:?}", program.exec);
        }
        None
    }
}

impl Thumbnailer for DesktopThumbnailer {
    fn thumbnail(&self, request: &ThumbnailRequest) -> Option<ThumbnailPicture> {
        let cache = self.cache.as_deref()?;
        let (tier, side) = tier_for(request.size)?;
        let meta = fs::metadata(&request.path).ok()?;
        if !meta.is_file() {
            return None;
        }
        let modified = modified_seconds(&meta)?;
        let uri = file_uri(&request.path)?;
        let name = md5_name(&uri);

        if let Some(picture) = cached(cache, tier, &name, &uri, modified, meta.len()) {
            return Some(picture);
        }
        if failed_before(cache, &name, modified) {
            return None;
        }
        let mime = ui_platform::files::mime_type(&request.path);
        let picture = self.generate(&request.path, &uri, mime, side)?;
        write_cache(
            &cache.join(tier),
            &name,
            &picture,
            &uri,
            modified,
            meta.len(),
            mime,
        );
        Some(picture)
    }
}

/// The tier that holds a picture at least `size` long, and its size.
fn tier_for(size: u32) -> Option<(&'static str, u32)> {
    if size == 0 {
        return None;
    }
    Some(
        TIERS
            .iter()
            .find(|(_, longest)| size <= *longest)
            .copied()
            .unwrap_or(TIERS[TIERS.len() - 1]),
    )
}

fn cache_home() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from)
        && dir.is_absolute()
    {
        return Some(dir.join("thumbnails"));
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".cache").join("thumbnails"))
}

fn modified_seconds(meta: &fs::Metadata) -> Option<i64> {
    let since = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(since.as_secs() as i64)
}

/// The `file:` URI a thumbnail is named after, escaped byte by byte so a path that is not
/// UTF-8 still has one.
fn file_uri(path: &Path) -> Option<String> {
    let absolute = std::path::absolute(path).ok()?;
    Some(format!(
        "file://{}",
        percent_encode(absolute.as_os_str().as_bytes(), URI)
    ))
}

fn md5_name(uri: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(uri.as_bytes());
    let digest = hasher.finalize();
    let mut name = String::with_capacity(32);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(name, "{byte:02x}");
    }
    name
}

/// The picture in the tier's cache entry, if that entry is this file's.
fn cached(
    cache: &Path,
    tier: &str,
    name: &str,
    uri: &str,
    modified: i64,
    len: u64,
) -> Option<ThumbnailPicture> {
    let path = cache.join(tier).join(format!("{name}.png"));
    let chunks = text_chunks(&path)?;
    if !accepts(&chunks, uri, modified, len) {
        return None;
    }
    read_png(&path)
}

/// A record in `fail/` at this file's current mtime means some other application already
/// concluded the file has no thumbnail.
fn failed_before(cache: &Path, name: &str, modified: i64) -> bool {
    let path = cache.join("fail").join(format!("{name}.png"));
    text_chunks(&path)
        .and_then(|chunks| value(&chunks, "Thumb::MTime").and_then(|text| text.parse().ok()))
        .is_some_and(|mtime: i64| mtime == modified)
}

/// The standard requires `Thumb::MTime`, but most writers on a real machine omit
/// `Thumb::URI`, so it is checked only when present: the file name is that URI's MD5.
fn accepts(chunks: &[(String, String)], uri: &str, modified: i64, len: u64) -> bool {
    let Some(stamp) = value(chunks, "Thumb::MTime").and_then(|text| text.parse::<i64>().ok())
    else {
        return false;
    };
    if stamp != modified {
        return false;
    }
    if let Some(size) = value(chunks, "Thumb::Size").and_then(|text| text.parse::<u64>().ok())
        && size != len
    {
        return false;
    }
    match value(chunks, "Thumb::URI") {
        Some(recorded) => recorded == uri,
        None => true,
    }
}

fn value<'a>(chunks: &'a [(String, String)], key: &str) -> Option<&'a str> {
    chunks
        .iter()
        .find(|(keyword, _)| keyword == key)
        .map(|(_, text)| text.as_str())
}

/// The PNG's `tEXt` chunks, read without decoding the picture.
fn text_chunks(path: &Path) -> Option<Vec<(String, String)>> {
    let file = fs::File::open(path).ok()?;
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let reader = decoder.read_info().ok()?;
    Some(
        reader
            .info()
            .uncompressed_latin1_text
            .iter()
            .map(|chunk| (chunk.keyword.clone(), chunk.text.clone()))
            .collect(),
    )
}

/// Decode a cached thumbnail into straight-alpha RGBA8. The cache holds PNGs, and a PNG's
/// alpha is already straight, so only the colour type has to be widened here.
fn read_png(path: &Path) -> Option<ThumbnailPicture> {
    let file = fs::File::open(path).ok()?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    let pixels = &buffer[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect(),
        png::ColorType::Grayscale => pixels
            .iter()
            .flat_map(|grey| [*grey, *grey, *grey, 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
            .collect(),
        png::ColorType::Indexed => {
            log::debug!("wayland: a cached thumbnail stayed indexed after expansion");
            return None;
        }
    };
    Some(ThumbnailPicture {
        width: info.width,
        height: info.height,
        rgba,
    })
}

/// Put the picture in the shared cache, atomically: another application must never read a
/// half-written entry, and the metadata is what this process measured itself.
fn write_cache(
    dir: &Path,
    name: &str,
    picture: &ThumbnailPicture,
    uri: &str,
    modified: i64,
    len: u64,
    mime: Option<&str>,
) {
    if let Err(error) = cache_write(dir, name, picture, uri, modified, len, mime) {
        log::debug!("wayland: the thumbnail cache was not written: {error}");
    }
}

fn cache_write(
    dir: &Path,
    name: &str,
    picture: &ThumbnailPicture,
    uri: &str,
    modified: i64,
    len: u64,
    mime: Option<&str>,
) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    let temp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    if let Err(error) = write_png(&temp, picture, uri, modified, len, mime) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    fs::rename(&temp, dir.join(format!("{name}.png"))).map_err(|error| error.to_string())
}

fn write_png(
    path: &Path,
    picture: &ThumbnailPicture,
    uri: &str,
    modified: i64,
    len: u64,
    mime: Option<&str>,
) -> Result<(), String> {
    let file = fs::File::create(path).map_err(|error| error.to_string())?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), picture.width, picture.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .add_text_chunk("Thumb::URI".to_string(), uri.to_string())
        .map_err(|error| error.to_string())?;
    encoder
        .add_text_chunk("Thumb::MTime".to_string(), modified.to_string())
        .map_err(|error| error.to_string())?;
    encoder
        .add_text_chunk("Thumb::Size".to_string(), len.to_string())
        .map_err(|error| error.to_string())?;
    if let Some(mime) = mime {
        encoder
            .add_text_chunk("Thumb::Mimetype".to_string(), mime.to_string())
            .map_err(|error| error.to_string())?;
    }
    let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
    writer
        .write_image_data(&picture.rgba)
        .map_err(|error| error.to_string())?;
    writer.finish().map_err(|error| error.to_string())
}

/// One `.thumbnailer` desktop entry, as much of it as thumbnailing needs.
#[derive(Debug, Clone)]
struct Program {
    exec: String,
    try_exec: Option<String>,
    mime_types: Vec<String>,
}

fn discover() -> Vec<Program> {
    let mut dirs = Vec::new();
    if let Some(data) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
        dirs.push(data.join("thumbnailers"));
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".local/share/thumbnailers"));
    }
    let data_dirs = std::env::var_os("XDG_DATA_DIRS")
        .map(|dirs| dirs.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    for dir in data_dirs.split(':').filter(|dir| !dir.is_empty()) {
        dirs.push(Path::new(dir).join("thumbnailers"));
    }
    let mut programs = Vec::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension() == Some(OsStr::new("thumbnailer")))
            .collect();
        paths.sort();
        for path in paths {
            let Some(text) = fs::read_to_string(&path).ok() else {
                continue;
            };
            if let Some(program) = parse_entry(&text) {
                programs.push(program);
            }
        }
    }
    programs
}

fn parse_entry(text: &str) -> Option<Program> {
    let mut in_entry = false;
    let (mut exec, mut try_exec, mut mime_types) = (None, None, Vec::new());
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Thumbnailer Entry]";
            continue;
        }
        if !in_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Exec" => exec = Some(value.trim().to_string()),
            "TryExec" => try_exec = Some(value.trim().to_string()),
            "MimeType" => mime_types.extend(
                value
                    .split(';')
                    .map(str::trim)
                    .filter(|mime| !mime.is_empty())
                    .map(str::to_string),
            ),
            _ => {}
        }
    }
    Some(Program {
        exec: exec?,
        try_exec,
        mime_types,
    })
}

/// Run a thumbnailer, read its PNG, and answer the picture; `None` if it failed or ran out
/// of time.
fn run_program(program: &Program, path: &Path, uri: &str, side: u32) -> Option<ThumbnailPicture> {
    let temp = tempfile::Builder::new().prefix("ui-thumbnail").tempdir().ok()?;
    let output = temp.path().join("thumbnail.png");
    let words = split_words(&program.exec);
    let executable = resolve(program.try_exec.as_deref(), words.first()?)?;
    let size = side.to_string();
    let output_text = output.as_os_str();
    let values: [(&str, &OsStr); 4] = [
        ("%i", path.as_os_str()),
        ("%u", OsStr::new(uri)),
        ("%o", output_text),
        ("%s", OsStr::new(size.as_str())),
    ];
    let args: Vec<OsString> = words[1..]
        .iter()
        .map(|word| fill(word, &values))
        .collect();
    let mut child = match Command::new(&executable)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            log::debug!("wayland: {} did not start: {error}", program.exec);
            return None;
        }
    };
    let deadline = Instant::now() + DEADLINE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(status)) => {
                log::debug!("wayland: {} exited with {status}", program.exec);
                return None;
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                log::debug!("wayland: {} ran past its deadline", program.exec);
                return None;
            }
            Err(error) => {
                log::debug!("wayland: {} could not be followed: {error}", program.exec);
                return None;
            }
        }
    }
    read_png(&output)
}

/// The executable to run: `TryExec` when it names one, else the `Exec` line's own program,
/// resolved through `PATH` when it is a bare name.
fn resolve(try_exec: Option<&str>, program: &str) -> Option<OsString> {
    let named = try_exec.map(OsStr::new).unwrap_or_else(|| OsStr::new(program));
    let candidate = Path::new(named);
    if candidate.is_absolute() {
        return candidate.is_file().then(|| named.to_os_string());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(named))
        .find(|joined| joined.is_file())
        .map(PathBuf::into_os_string)
}

/// Replace the placeholders in an already-split word. Splitting first is what keeps a path
/// with a space one argument: the value never reaches the word splitter.
fn fill(word: &str, values: &[(&str, &OsStr)]) -> OsString {
    let mut out = OsString::new();
    let mut literal = String::new();
    let mut chars = word.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            literal.push(ch);
            continue;
        }
        let Some(&next) = chars.peek() else {
            literal.push('%');
            break;
        };
        if next == '%' {
            chars.next();
            literal.push('%');
            continue;
        }
        let place = format!("%{next}");
        match values.iter().find(|(key, _)| *key == place) {
            Some((_, value)) => {
                chars.next();
                out.push(std::mem::take(&mut literal));
                out.push(*value);
            }
            None => literal.push('%'),
        }
    }
    out.push(literal);
    out
}

/// Split a desktop entry's `Exec` into words with the quoting a shell would apply, without
/// being a shell: no pipes, no variables, no globs.
fn split_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        match quote {
            Some('\'') => {
                if ch == '\'' {
                    quote = None;
                } else {
                    word.push(ch);
                }
            }
            Some(_) => match ch {
                '"' => quote = None,
                '\\' => {
                    if let Some(next) = chars.next() {
                        word.push(next);
                    }
                }
                _ => word.push(ch),
            },
            None => match ch {
                '\'' | '"' => {
                    quote = Some(ch);
                    started = true;
                }
                '\\' => {
                    if let Some(next) = chars.next() {
                        word.push(next);
                        started = true;
                    }
                }
                ch if ch.is_whitespace() => {
                    if started {
                        words.push(std::mem::take(&mut word));
                        started = false;
                    }
                }
                ch => {
                    word.push(ch);
                    started = true;
                }
            },
        }
    }
    if started {
        words.push(word);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cache entry counts only for the file it was made from.
    #[test]
    fn a_cache_entry_counts_only_for_the_file_it_was_made_from() {
        let chunks = vec![
            ("Thumb::MTime".to_string(), "1700000000".to_string()),
            ("Thumb::Size".to_string(), "4096".to_string()),
            ("Thumb::URI".to_string(), "file:///tmp/a.png".to_string()),
        ];
        assert!(accepts(&chunks, "file:///tmp/a.png", 1_700_000_000, 4096));
        assert!(
            accepts(&chunks[..2], "file:///tmp/a.png", 1_700_000_000, 4096),
            "entries without Thumb::URI are what most writers produce"
        );
        assert!(!accepts(&chunks, "file:///tmp/a.png", 1_700_000_001, 4096));
        assert!(!accepts(&chunks, "file:///tmp/b.png", 1_700_000_000, 4096));
        assert!(!accepts(&chunks, "file:///tmp/a.png", 1_700_000_000, 4097));
        assert!(!accepts(&chunks[2..], "file:///tmp/a.png", 1_700_000_000, 4096));
    }

    /// A value with a space or an apostrophe stays inside the argument it was written in.
    #[test]
    fn a_substituted_path_stays_one_argument() {
        let values: [(&str, &OsStr); 4] = [
            ("%i", OsStr::new("/tmp/a b's.png")),
            ("%u", OsStr::new("file:///tmp/a%20b%27s.png")),
            ("%o", OsStr::new("/tmp/out.png")),
            ("%s", OsStr::new("128")),
        ];
        let line = "raw-thumbnailer -s %s %i -o %o";
        let words: Vec<OsString> = split_words(line)
            .iter()
            .map(|word| fill(word, &values))
            .collect();
        assert_eq!(
            words,
            [
                OsString::from("raw-thumbnailer"),
                OsString::from("-s"),
                OsString::from("128"),
                OsString::from("/tmp/a b's.png"),
                OsString::from("-o"),
                OsString::from("/tmp/out.png"),
            ]
        );
        assert_eq!(
            fill("--input=%u", &values),
            OsString::from("--input=file:///tmp/a%20b%27s.png")
        );
        assert_eq!(fill("100%%", &values), OsString::from("100%"));
    }

    /// A thumbnailer entry yields its `Exec`, its optional `TryExec` and every type it
    /// claims.
    #[test]
    fn an_entry_yields_its_program_and_types() {
        let text = "[Thumbnailer Entry]\nTryExec=/usr/bin/glycin-thumbnailer\n\
                    Exec=/usr/bin/glycin-thumbnailer --input %u --output %o --size %s\n\
                    MimeType=image/png;image/jpeg;\n";
        let program = parse_entry(text).expect("a complete entry");
        assert_eq!(
            program.try_exec.as_deref(),
            Some("/usr/bin/glycin-thumbnailer")
        );
        assert_eq!(program.mime_types, ["image/png", "image/jpeg"]);
        let bare = parse_entry("[Thumbnailer Entry]\nExec=x %i %o\nMimeType=image/png\n")
            .expect("an entry without TryExec");
        assert_eq!(bare.try_exec, None);
        assert_eq!(bare.exec, "x %i %o");
    }

    /// The tier a request maps to is the first one that is large enough.
    #[test]
    fn a_size_maps_to_the_first_tier_that_holds_it() {
        assert_eq!(tier_for(64), Some(("normal", 128)));
        assert_eq!(tier_for(128), Some(("normal", 128)));
        assert_eq!(tier_for(129), Some(("large", 256)));
        assert_eq!(tier_for(300), Some(("x-large", 512)));
        assert_eq!(tier_for(600), Some(("xx-large", 1024)));
        assert_eq!(tier_for(4096), Some(("xx-large", 1024)));
        assert_eq!(tier_for(0), None);
    }

    /// The cache name is the MD5 of the escaped `file:` URI, which is the only thing that
    /// makes an entry findable by another application.
    #[test]
    fn a_path_becomes_an_escaped_uri_and_its_md5() {
        let uri = file_uri(Path::new("/tmp/a b.png")).expect("an absolute path");
        assert_eq!(uri, "file:///tmp/a%20b.png");
        assert_eq!(md5_name("file:///tmp/a.png"), "a04bfd79b77efaccf5f6adb271b86f1e");
        assert_eq!(
            md5_name("file:///tmp/a b.png"),
            "00f489498344fac9d291b97296847d1e"
        );
    }
}
