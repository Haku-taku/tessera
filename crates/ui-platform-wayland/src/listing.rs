//! The desktop's user directories and volumes for the file picker.
//!
//! The places come from `user-dirs.dirs`, the same file GTK, KDE and the XDG utilities read,
//! so the two agree about where "Downloads" is. The volumes come from the mount table with
//! `ui-platform`'s Dolphin-shaped selection. Reading a directory itself is the portable
//! `std::fs` lister in `ui-platform`, with the dotfile rule unix desktops use.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use ui_platform::places::{Device, Place, PlaceKind};

/// The user's home and the XDG user directories that exist.
///
/// A directory the file names but that does not exist is left out: the desktop hides an
/// uncreated "Templates" folder too, and a picker row that fails to open is worse than no row.
pub fn places() -> Vec<Place> {
    let Some(home) = env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let mut places = Vec::new();
    if home.is_dir() {
        places.push(Place {
            kind: PlaceKind::Home,
            path: home.clone(),
        });
    }
    let table = user_dirs(&home);
    for (key, kind) in [
        ("DESKTOP", PlaceKind::Desktop),
        ("DOCUMENTS", PlaceKind::Documents),
        ("DOWNLOAD", PlaceKind::Downloads),
        ("MUSIC", PlaceKind::Music),
        ("PICTURES", PlaceKind::Pictures),
        ("VIDEOS", PlaceKind::Videos),
        ("TEMPLATES", PlaceKind::Templates),
        ("PUBLICSHARE", PlaceKind::PublicShare),
    ] {
        let Some(path) = table.get(key) else {
            continue;
        };
        if path == &home || !path.is_dir() {
            continue;
        }
        places.push(Place {
            kind,
            path: path.clone(),
        });
    }
    places
}

/// The mounted volumes a picker offers beside the places.
///
/// The mount table is `/proc/self/mountinfo` and the selection is the portable
/// [`linux_mounts`](ui_platform::places::linux_mounts) rule; this side only reads the files
/// and names a device by its filesystem label where udev knows one.
pub fn devices() -> Vec<Device> {
    let Ok(table) = fs::read_to_string("/proc/self/mountinfo") else {
        return Vec::new();
    };
    let labels = labels();
    ui_platform::places::linux_mounts(&table, &|source| {
        let target = fs::canonicalize(source).ok()?;
        labels
            .iter()
            .find(|(device, _)| device == &target)
            .map(|(_, label)| label.clone())
    })
}

/// The labelled block devices, from udev's `/dev/disk/by-label` links: the name of the
/// link, and the device it resolves to.
fn labels() -> Vec<(PathBuf, String)> {
    let Ok(entries) = fs::read_dir("/dev/disk/by-label") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let device = fs::canonicalize(entry.path()).ok()?;
            let label = entry.file_name().to_string_lossy().into_owned();
            Some((device, label))
        })
        .collect()
}

/// The `XDG_*_DIR` table from `user-dirs.dirs`.
///
/// `$HOME` inside a value is the only variable the format uses; anything else stays a
/// literal path, which is what `xdg-user-dirs` does with it too.
fn user_dirs(home: &Path) -> HashMap<&'static str, PathBuf> {
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    let Ok(source) = fs::read_to_string(config.join("user-dirs.dirs")) else {
        return HashMap::new();
    };
    let home_text = home.to_string_lossy();
    let mut table = HashMap::new();
    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let Some(key) = key.trim().strip_prefix("XDG_").and_then(|key| key.strip_suffix("_DIR"))
        else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        let value = strip_home(value, &home_text);
        if value.is_empty() {
            continue;
        }
        if let Some(key) = [
            "DESKTOP",
            "DOCUMENTS",
            "DOWNLOAD",
            "MUSIC",
            "PICTURES",
            "VIDEOS",
            "TEMPLATES",
            "PUBLICSHARE",
        ]
        .into_iter()
        .find(|candidate| *candidate == key)
        {
            table.insert(key, PathBuf::from(value));
        }
    }
    table
}

/// Replace a leading `$HOME` or `${HOME}` with the home directory.
fn strip_home(value: &str, home: &str) -> String {
    if let Some(rest) = value.strip_prefix("${HOME}") {
        return format!("{home}{rest}");
    }
    if let Some(rest) = value.strip_prefix("$HOME") {
        return format!("{home}{rest}");
    }
    value.to_string()
}
