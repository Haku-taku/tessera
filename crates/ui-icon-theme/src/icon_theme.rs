//! Freedesktop icon themes: the desktop's own picture for a file type.
//!
//! The lookup is the icon theme specification's: `index.theme` lists the directories and
//! their sizes, `Inherits` orders the fallback themes, and a directory holds the icons.
//! Breeze ships no PNG at all and Adwaita only 16px ones, so SVG is rendered here rather
//! than merely read; a PNG-only reader would answer nothing on one and a blurry mark on
//! the other.

use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ui_platform::{
    FileIconProvider, FileIconRequest, FileKind, FileIcon, FileBadge, BadgeCorner, ThumbnailPicture,
};

/// The theme chain this process draws from: the selected theme, then its `Inherits` order,
/// then `hicolor`. Resolved once; a desktop that switches themes mid-run keeps the old
/// pictures until the process restarts.
pub struct IconTheme {
    chain: OnceLock<Vec<Theme>>,
}

impl Default for IconTheme {
    fn default() -> Self {
        Self::new()
    }
}

impl IconTheme {
    pub fn new() -> Self {
        Self {
            chain: OnceLock::new(),
        }
    }

    fn chain(&self) -> &[Theme] {
        self.chain.get_or_init(load_chain)
    }
}

impl FileIconProvider for IconTheme {
    fn icon(&self, request: &FileIconRequest) -> Option<FileIcon> {
        let (base_path, base) = self.picture(base_names(request), request.size)?;
        // A link keeps its target's picture and gains the desktop's own emblem, which is
        // the freedesktop answer to the shell's overlay slot.
        let badge = if request.link {
            self.picture(
                &["emblem-symbolic-link", "emblem-symlink"],
                request.size,
            )
            .map(|(path, picture)| {
                (
                    path,
                    FileBadge {
                        picture,
                        corner: BadgeCorner::BottomRight,
                    },
                )
            })
        } else {
            None
        };
        let identity = identity(&base_path, badge.as_ref().map(|(path, _)| path.as_path()));
        Some(FileIcon {
            identity,
            base,
            badge: badge.map(|(_, badge)| badge),
        })
    }
}

impl IconTheme {
    /// The first named icon the chain carries, with the file it came from so two rows of
    /// one type can share an identity.
    fn picture(&self, names: &[&str], size: u32) -> Option<(PathBuf, ThumbnailPicture)> {
        for name in names {
            for theme in self.chain() {
                let Some(path) = theme.find(name, size) else {
                    continue;
                };
                match load_picture(&path, size) {
                    Some(picture) => return Some((path, picture)),
                    None => log::debug!("icon theme: {} did not decode", path.display()),
                }
            }
        }
        None
    }
}

/// The names the item asks for. An executable is a permission bit rather than a name, so
/// no `FileKind` can say it, and the desktop has a more specific icon than the kind's.
fn base_names(request: &FileIconRequest) -> &'static [&'static str] {
    if is_executable(&request.path) {
        return &[
            "application-x-executable",
            "application-x-executable-script",
            "text-x-generic",
        ];
    }
    icon_names(request.kind)
}

/// Whether the file's own permission bits name it executable. Metadata follows a symlink,
/// so a link to a program draws the program's icon.
#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// A platform without the Unix permission bits has nothing here to read; the kind already
/// named the file.
#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    false
}

/// The share key for a picture: equal keys are one upload. Hashed from the theme files that
/// produced the pixels, not from the pixels themselves.
fn identity(base: &Path, badge: Option<&Path>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    base.hash(&mut hasher);
    badge.hash(&mut hasher);
    hasher.finish()
}

/// The standard names a kind asks for, most specific first. A file manager would resolve
/// the exact MIME type; the kind vocabulary asks for the family's generic icon, which every
/// theme drawn for the desktop has.
fn icon_names(kind: FileKind) -> &'static [&'static str] {
    match kind {
        FileKind::Folder => &["folder", "inode-directory"],
        FileKind::Drive => &["drive-harddisk", "harddisk"],
        FileKind::Document => &["x-office-document", "text-x-generic"],
        FileKind::Spreadsheet => &["x-office-spreadsheet", "text-x-generic"],
        FileKind::Presentation => &["x-office-presentation", "text-x-generic"],
        FileKind::Pdf => &["application-pdf", "x-office-document"],
        FileKind::Text => &["text-x-generic"],
        FileKind::Code => &["text-x-script", "text-x-generic"],
        FileKind::Image => &["image-x-generic"],
        FileKind::Video => &["video-x-generic"],
        FileKind::Audio => &["audio-x-generic"],
        FileKind::Archive => &["package-x-generic"],
        FileKind::Generic => &["text-x-generic", "unknown"],
    }
}

/// One theme's directories, as its `index.theme` lists them.
struct Theme {
    dirs: Vec<Directory>,
    inherits: Vec<String>,
}

struct Directory {
    path: PathBuf,
    /// The nominal size the directory was drawn at; SVG ignores it.
    size: u32,
}

impl Theme {
    /// The best icon file for `name`, or nothing: a theme either has the icon or does not,
    /// and a poor size is still better than falling through to the next theme.
    fn find(&self, name: &str, size: u32) -> Option<PathBuf> {
        let mut best: Option<((u8, u32), PathBuf)> = None;
        for dir in &self.dirs {
            for extension in ["svg", "png"] {
                let path = dir.path.join(format!("{name}.{extension}"));
                if !path.is_file() {
                    continue;
                }
                let rank = rank(extension, dir.size, size);
                if best.as_ref().is_none_or(|(current, _)| rank < *current) {
                    best = Some((rank, path));
                }
            }
        }
        best.map(|(_, path)| path)
    }
}

/// How well a file answers a request: any SVG first (it scales to whatever is asked), then
/// the smallest PNG at least as large as asked, then the largest PNG below it.
fn rank(extension: &str, directory_size: u32, size: u32) -> (u8, u32) {
    if extension == "svg" {
        (0, 0)
    } else if directory_size >= size {
        (1, directory_size - size)
    } else {
        (2, size - directory_size)
    }
}

fn load_picture(path: &Path, size: u32) -> Option<ThumbnailPicture> {
    match path.extension()?.to_str()? {
        "svg" => render_svg(path, size),
        "png" => decode_png(path),
        _ => None,
    }
}

fn render_svg(path: &Path, size: u32) -> Option<ThumbnailPicture> {
    let data = fs::read(path).ok()?;
    let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()).ok()?;
    let source = tree.size();
    let longest = source.width().max(source.height());
    if !longest.is_finite() || longest <= 0.0 {
        return None;
    }
    let scale = size as f32 / longest;
    let width = (source.width() * scale).round().max(1.0) as u32;
    let height = (source.height() * scale).round().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(ThumbnailPicture {
        width,
        height,
        rgba: straight(pixmap.data().to_vec()),
    })
}

/// A pixmap's bytes are premultiplied; `ImageEntry` carries straight alpha.
fn straight(mut rgba: Vec<u8>) -> Vec<u8> {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = pixel[3] as u32;
        if alpha == 0 || alpha == 255 {
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = ((*channel as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
    rgba
}

fn decode_png(path: &Path) -> Option<ThumbnailPicture> {
    let file = fs::File::open(path).ok()?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    // EXPAND turns the palette and tRNS entries indexed icons almost always use into
    // straight RGB(A); STRIP_16 gives the 8 bits per channel the picture type promises.
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    let bytes = &buffer[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => bytes
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect(),
        png::ColorType::Grayscale => bytes.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => bytes
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
            .collect(),
        png::ColorType::Indexed => return None,
    };
    Some(ThumbnailPicture {
        width: info.width,
        height: info.height,
        rgba,
    })
}

fn load_chain() -> Vec<Theme> {
    let mut cache: HashMap<String, Option<Theme>> = HashMap::new();
    let mut load = |name: &str| -> Option<Vec<String>> {
        cache
            .entry(name.to_string())
            .or_insert_with(|| load_theme(name))
            .as_ref()
            .map(|theme| theme.inherits.clone())
    };
    let mut names = chain_names(&selected_theme(), &mut load);
    // `hicolor` is the specification's guaranteed theme; the selected one may not list it.
    if !names.iter().any(|name| name == "hicolor") && load("hicolor").is_some() {
        names.push("hicolor".to_string());
    }
    drop(load);
    names
        .into_iter()
        .filter_map(|name| cache.remove(&name).flatten())
        .collect()
}

/// The themes to try, in order, ending before a name would repeat. A theme that names a
/// cycle in `Inherits` is not an error the desktop reports; the chain simply closes.
fn chain_names(start: &str, load: &mut dyn FnMut(&str) -> Option<Vec<String>>) -> Vec<String> {
    let mut names = Vec::new();
    let mut queue = VecDeque::from([start.to_string()]);
    let mut seen = HashSet::new();
    while let Some(name) = queue.pop_front() {
        if !seen.insert(name.clone()) {
            continue;
        }
        match load(&name) {
            Some(inherits) => {
                queue.extend(inherits);
                names.push(name);
            }
            None => log::debug!("icon theme: {name:?} is not installed"),
        }
    }
    names
}

fn load_theme(name: &str) -> Option<Theme> {
    for root in icon_roots() {
        let root = root.join(name);
        let Ok(text) = fs::read_to_string(root.join("index.theme")) else {
            continue;
        };
        let index = parse_index(&text);
        let dirs = index
            .dirs
            .into_iter()
            .map(|(dir, size)| Directory {
                path: root.join(dir),
                size,
            })
            .collect();
        return Some(Theme {
            dirs,
            inherits: index.inherits,
        });
    }
    None
}

struct Index {
    inherits: Vec<String>,
    /// Directory name and nominal size, in the order the theme lists them.
    dirs: Vec<(String, u32)>,
}

fn parse_index(text: &str) -> Index {
    let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            sections.push((name.to_string(), Vec::new()));
        } else if let Some((key, value)) = line.split_once('=')
            && let Some((_, entries)) = sections.last_mut()
        {
            entries.push((key.trim().to_string(), value.trim().to_string()));
        }
    }

    let mut inherits = Vec::new();
    let mut listing = Vec::new();
    let mut sizes: HashMap<String, u32> = HashMap::new();
    for (section, entries) in sections {
        if section == "Icon Theme" {
            for (key, value) in entries {
                match key.as_str() {
                    "Inherits" => inherits.extend(names_of(&value)),
                    "Directories" | "ScaledDirectories" => listing.extend(names_of(&value)),
                    _ => {}
                }
            }
        } else if let Some(size) = entries
            .iter()
            .find(|(key, _)| key.as_str() == "Size")
            .and_then(|(_, value)| value.parse().ok())
        {
            sizes.insert(section, size);
        }
    }
    let dirs = listing
        .into_iter()
        .filter_map(|dir| sizes.get(&dir).map(|size| (dir.clone(), *size)))
        .collect();
    Index { inherits, dirs }
}

fn names_of(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

/// The theme the desktop says it uses. The desktop's own configuration is the only source
/// that does not need GSettings or D-Bus; a user who sets one in both GTK and KDE gets GTK's
/// answer, which is what a GTK-styled app is expected to show.
fn selected_theme() -> String {
    let kde_desktop = env::var("XDG_CURRENT_DESKTOP")
        .is_ok_and(|desktop| desktop.split(':').any(|part| part.eq_ignore_ascii_case("kde")));
    if kde_desktop && let Some(theme) = kde_theme() {
        return theme;
    }
    if let Some(theme) = gtk_theme() {
        return theme;
    }
    if let Some(theme) = kde_theme() {
        return theme;
    }
    "hicolor".to_string()
}

fn gtk_theme() -> Option<String> {
    let config = config_home()?;
    for version in ["gtk-4.0", "gtk-3.0"] {
        let Ok(text) = fs::read_to_string(config.join(version).join("settings.ini")) else {
            continue;
        };
        if let Some(theme) = ini_value(&text, Some("Settings"), "gtk-icon-theme-name") {
            return Some(theme);
        }
    }
    None
}

fn kde_theme() -> Option<String> {
    let text = fs::read_to_string(config_home()?.join("kdeglobals")).ok()?;
    ini_value(&text, Some("Icons"), "Theme")
}

fn config_home() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

fn ini_value(text: &str, wanted: Option<&str>, key: &str) -> Option<String> {
    let mut section = None::<String>;
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            section = Some(name.to_string());
            continue;
        }
        if wanted.is_some() && section.as_deref() != wanted {
            continue;
        }
        if let Some((name, value)) = line.split_once('=')
            && name.trim() == key
            && !value.trim().is_empty()
        {
            return Some(value.trim().to_string());
        }
    }
    None
}

fn icon_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    match env::var_os("XDG_DATA_HOME") {
        Some(home) => roots.push(PathBuf::from(home).join("icons")),
        None => {
            if let Some(home) = env::var_os("HOME") {
                roots.push(PathBuf::from(home).join(".local/share/icons"));
            }
        }
    }
    let dirs = env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    roots.extend(
        dirs.split(':')
            .filter(|dir| !dir.is_empty())
            .map(|dir| Path::new(dir).join("icons")),
    );
    if let Some(home) = env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".icons"));
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `index.theme` decides where icons live and in what order the themes are tried; the
    /// size of each directory is what the ranking later reads.
    #[test]
    fn an_index_lists_its_directories_and_inherits() {
        let index = parse_index(
            "[Icon Theme]\n\
             Name=Test\n\
             Name[de]=Test\n\
             Inherits=hicolor, breeze\n\
             Directories=16x16/mimetypes,scalable/mimetypes\n\
             ScaledDirectories=16x16/mimetypes@2\n\
             \n\
             [16x16/mimetypes]\n\
             Size=16\n\
             Type=Fixed\n\
             \n\
             [scalable/mimetypes]\n\
             Size=64\n\
             Type=Scalable\n",
        );
        assert_eq!(index.inherits, ["hicolor", "breeze"]);
        assert_eq!(
            index.dirs,
            [
                ("16x16/mimetypes".to_string(), 16),
                ("scalable/mimetypes".to_string(), 64),
            ]
        );
    }

    /// A scalable SVG wins at any size; otherwise the smallest sufficient PNG wins, and
    /// when nothing is large enough the largest is still better than nothing.
    #[test]
    fn a_bigger_picture_wins_then_the_largest() {
        assert!(rank("svg", 16, 512) < rank("png", 512, 512));
        assert!(rank("png", 32, 24) < rank("png", 64, 24));
        assert_eq!(rank("png", 16, 24), (2, 8));
        assert_eq!(rank("png", 48, 24), (1, 24));
    }

    /// A theme whose `Inherits` points back at one already seen ends, rather than looping.
    #[test]
    fn an_inherit_cycle_ends() {
        let names = chain_names("a", &mut |name| match name {
            "a" => Some(vec!["b".to_string()]),
            "b" => Some(vec!["a".to_string(), "c".to_string()]),
            "c" => None,
            _ => None,
        });
        assert_eq!(names, ["a", "b"]);
    }

    /// A renderer hands back premultiplied bytes; the picture type promises straight alpha,
    /// and a half-transparent red is the case where the two differ.
    #[test]
    fn a_premultiplied_pixel_becomes_straight() {
        let pixels = straight(vec![128, 0, 0, 128, 10, 10, 10, 255]);
        assert_eq!(&pixels[..4], [255, 0, 0, 128]);
        assert_eq!(&pixels[4..], [10, 10, 10, 255]);
    }
}
