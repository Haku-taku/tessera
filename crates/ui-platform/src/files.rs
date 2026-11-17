//! What a path's name says about its type, and the kinds a picker draws.
//!
//! The table is the one vocabulary every layer shares: the Wayland thumbnailer lookup
//! matches it against `.thumbnailer` MimeType lines, and [`FileKind`] folds it into the
//! handful of shapes an icon slot can draw. Names are read, never file contents — a
//! header or magic number would need the codecs this framework leaves to the desktop.

use std::path::Path;

/// The shape an item is drawn as. Deliberately coarser than MIME: an icon slot shows one
/// of these, and a type the vocabulary does not know is `Generic`, not an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FileKind {
    Folder,
    Drive,
    Document,
    Spreadsheet,
    Presentation,
    Pdf,
    Text,
    Code,
    Image,
    Video,
    Audio,
    Archive,
    Generic,
}

impl FileKind {
    /// The kind a name implies. A directory is not decided here: its name says nothing
    /// about it, and only the caller has stat'ed it.
    pub fn of_path(path: &Path) -> Self {
        match mime_type(path) {
            Some(mime) => Self::of_mime(mime),
            None => Self::Generic,
        }
    }

    pub fn of_mime(mime: &str) -> Self {
        match mime {
            "inode/directory" => Self::Folder,
            "application/pdf" => Self::Pdf,
            "application/msword"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            | "application/vnd.oasis.opendocument.text"
            | "application/rtf" => Self::Document,
            "application/vnd.ms-excel"
            | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            | "application/vnd.oasis.opendocument.spreadsheet"
            | "text/csv" => Self::Spreadsheet,
            "application/vnd.ms-powerpoint"
            | "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            | "application/vnd.oasis.opendocument.presentation" => Self::Presentation,
            "application/zip"
            | "application/gzip"
            | "application/x-tar"
            | "application/x-bzip2"
            | "application/x-xz"
            | "application/zstd"
            | "application/x-7z-compressed"
            | "application/x-rar-compressed"
            | "application/x-cd-image"
            | "application/vnd.debian.binary-package"
            | "application/x-rpm"
            | "application/vnd.appimage"
            | "application/java-archive" => Self::Archive,
            "application/json"
            | "application/xml"
            | "application/toml"
            | "application/yaml"
            | "application/javascript"
            | "application/x-sh"
            | "text/html"
            | "text/css" => Self::Code,
            // `text/*` is text unless it names a format, which shared-mime-info spells
            // `text/x-name`; marking an unknown subtype as code would make every note look
            // like source.
            _ if mime.starts_with("text/x-") => Self::Code,
            _ if mime.starts_with("text/") => Self::Text,
            _ if mime.starts_with("image/") => Self::Image,
            _ if mime.starts_with("video/") => Self::Video,
            _ if mime.starts_with("audio/") => Self::Audio,
            _ => Self::Generic,
        }
    }
}

/// The MIME type a name implies, from its extension. This is the shared table: the
/// thumbnailer lookup needs the string, and [`FileKind`] needs the category.
pub fn mime_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let mime = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "tif" | "tiff" => "image/tiff",
        "avif" => "image/avif",
        "heic" | "heif" => "image/heif",
        "jxl" => "image/jxl",
        "ico" => "image/vnd.microsoft.icon",
        "pdf" => "application/pdf",
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "mpg" | "mpeg" => "video/mpeg",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "opus" => "audio/ogg",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        "wav" => "audio/x-wav",
        "odt" => "application/vnd.oasis.opendocument.text",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        "odp" => "application/vnd.oasis.opendocument.presentation",
        "odg" => "application/vnd.oasis.opendocument.graphics",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "dng" => "image/x-adobe-dng",
        "cr2" => "image/x-canon-cr2",
        "nef" => "image/x-nikon-nef",
        "arw" => "image/x-sony-arw",
        "appimage" => "application/vnd.appimage",
        "txt" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "rst" => "text/x-rst",
        "log" => "text/plain",
        "csv" => "text/csv",
        "json" => "application/json",
        "toml" => "application/toml",
        "yaml" | "yml" => "application/yaml",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" | "cjs" => "application/javascript",
        "rs" => "text/x-rust",
        "py" => "text/x-python",
        "c" => "text/x-c",
        "h" => "text/x-c",
        "cpp" | "cc" | "cxx" => "text/x-c++",
        "hpp" | "hh" => "text/x-c++",
        "go" => "text/x-go",
        "java" => "text/x-java",
        "kt" | "kts" => "text/x-kotlin",
        "rb" => "text/x-ruby",
        "php" => "text/x-php",
        "lua" => "text/x-lua",
        "sql" => "text/x-sql",
        "ini" | "conf" | "cfg" => "text/plain",
        "sh" | "bash" => "application/x-sh",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" | "tgz" => "application/gzip",
        "bz2" => "application/x-bzip2",
        "xz" => "application/x-xz",
        "zst" => "application/zstd",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/x-rar-compressed",
        "iso" => "application/x-cd-image",
        "deb" => "application/vnd.debian.binary-package",
        "rpm" => "application/x-rpm",
        _ => return None,
    };
    Some(mime)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table reaches the kind vocabulary at all, folds case, and leaves a name it
    /// does not know as `Generic` rather than guessing.
    #[test]
    fn a_name_yields_its_kind_and_an_unknown_one_is_generic() {
        assert_eq!(FileKind::of_path(Path::new("/a/b/C.PDF")), FileKind::Pdf);
        assert_eq!(
            FileKind::of_path(Path::new("backup.tar.gz")),
            FileKind::Archive
        );
        assert_eq!(FileKind::of_path(Path::new("main.rs")), FileKind::Code);
        assert_eq!(FileKind::of_path(Path::new("notes.markdown")), FileKind::Text);
        assert_eq!(FileKind::of_path(Path::new("README")), FileKind::Generic);
        assert_eq!(FileKind::of_mime("inode/directory"), FileKind::Folder);
    }
}
