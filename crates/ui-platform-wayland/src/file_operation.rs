//! The unix half of file operations: device identity, symlinks, and the freedesktop trash.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use ui_platform::file_operation::{FileOperator, LinkKind, OpError};

pub(crate) struct UnixOperator;

impl FileOperator for UnixOperator {
    fn same_volume(&self, a: &Path, b: &Path) -> bool {
        match (device(a), device(b)) {
            (Some(a), Some(b)) => a == b,
            // A path that cannot be named is not known to be on another device; a copy is
            // the safe answer because it never deletes the source.
            _ => false,
        }
    }

    fn trash(&self, path: &Path) -> Result<(), OpError> {
        let absolute = std::path::absolute(path).map_err(|error| OpError::from_io(&error))?;
        let trash = trash_dir()?;
        let files = trash.join("files");
        let info = trash.join("info");
        for directory in [&files, &info] {
            std::fs::create_dir_all(directory).map_err(|error| OpError::from_io(&error))?;
        }
        let Some(name) = absolute.file_name() else {
            return Err(OpError::Unsupported);
        };
        let name = name.to_string_lossy().into_owned();
        let (name, info_path) = free_name(&files, &info, &name)?;
        let record = format!(
            "[Trash Info]\nPath={}\nDeletionDate={}\n",
            encode_path(&absolute),
            now(),
        );
        std::fs::write(&info_path, record).map_err(|error| OpError::from_io(&error))?;
        match std::fs::rename(&absolute, files.join(&name)) {
            Ok(()) => Ok(()),
            // The home trash can be on another device; the specification lets the trashing
            // copy instead of renaming, and the copy is only removed after it is complete.
            Err(error)
                if error.raw_os_error() == Some(rustix::io::Errno::XDEV.raw_os_error()) =>
            {
                if let Err(error) = copy_tree(&absolute, &files.join(&name))
                    .and_then(|()| remove_any(&absolute))
                {
                    let _ = std::fs::remove_file(&info_path);
                    return Err(error);
                }
                Ok(())
            }
            Err(error) => {
                let _ = std::fs::remove_file(&info_path);
                Err(OpError::from_io(&error))
            }
        }
    }

    fn symlink(&self, target: &Path, link: &Path, _directory: bool) -> Result<(), OpError> {
        std::os::unix::fs::symlink(target, link).map_err(|error| OpError::from_io(&error))
    }

    fn supports_links(&self, _kind: LinkKind) -> bool {
        true
    }

    fn validate_name(&self, name: &str) -> Result<(), OpError> {
        if name.is_empty() || name.contains('/') || name.contains('\0') {
            Err(OpError::Unsupported)
        } else {
            Ok(())
        }
    }
}

/// The device of `path`, or of the nearest ancestor that exists.
fn device(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    let mut current = Some(path);
    while let Some(candidate) = current {
        if let Ok(metadata) = std::fs::metadata(candidate) {
            return Some(metadata.dev());
        }
        current = candidate.parent();
    }
    None
}

fn trash_dir() -> Result<PathBuf, OpError> {
    if let Some(data) = std::env::var_os("XDG_DATA_HOME") {
        let data = PathBuf::from(data);
        if data.is_absolute() {
            return Ok(data.join("Trash"));
        }
    }
    let home = std::env::var_os("HOME").ok_or(OpError::Unsupported)?;
    Ok(PathBuf::from(home).join(".local/share/Trash"))
}

/// A name neither `files/` nor `info/` holds, as the trash specification names a collision.
fn free_name(files: &Path, info: &Path, name: &str) -> Result<(String, PathBuf), OpError> {
    for number in 1.. {
        let candidate = if number == 1 {
            name.to_owned()
        } else {
            format!("{name}.{number}")
        };
        let info_path = info.join(format!("{candidate}.trashinfo"));
        if !files.join(&candidate).exists() && !info_path.exists() {
            return Ok((candidate, info_path));
        }
    }
    Err(OpError::Unsupported)
}

/// Percent-escape a path for the trash record: `/` stays, everything a URL must escape does
/// not. Non-ASCII bytes are UTF-8 and escaped by the encoder.
fn encode_path(path: &Path) -> String {
    const UNSAFE: &AsciiSet = &CONTROLS
        .add(b' ')
        .add(b'"')
        .add(b'#')
        .add(b'%')
        .add(b'<')
        .add(b'>')
        .add(b'?')
        .add(b'[')
        .add(b'\\')
        .add(b']')
        .add(b'^')
        .add(b'`')
        .add(b'{')
        .add(b'|')
        .add(b'}');
    let path = path.to_string_lossy();
    utf8_percent_encode(&path, UNSAFE).to_string()
}

/// The trash record's local time, `YYYY-MM-DDTHH:MM:SS`.
fn now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}",
        rest / 3600,
        (rest / 60) % 60,
        rest % 60
    )
}

/// Days since 1970-01-01 to a calendar date, by Howard Hinnant's civil algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (
        year + i64::from(month <= 2),
        month as u32,
        day as u32,
    )
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), OpError> {
    let metadata = std::fs::symlink_metadata(source).map_err(|error| OpError::from_io(&error))?;
    let kind = metadata.file_type();
    if kind.is_symlink() {
        let target = std::fs::read_link(source).map_err(|error| OpError::from_io(&error))?;
        return std::os::unix::fs::symlink(target, destination)
            .map_err(|error| OpError::from_io(&error));
    }
    if kind.is_dir() {
        std::fs::create_dir_all(destination).map_err(|error| OpError::from_io(&error))?;
        let entries = std::fs::read_dir(source).map_err(|error| OpError::from_io(&error))?;
        for entry in entries {
            let entry = entry.map_err(|error| OpError::from_io(&error))?;
            copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
        }
        let _ = std::fs::set_permissions(destination, metadata.permissions());
        return Ok(());
    }
    if kind.is_file() {
        std::fs::copy(source, destination).map_err(|error| OpError::from_io(&error))?;
        return Ok(());
    }
    Err(OpError::Unsupported)
}

fn remove_any(path: &Path) -> Result<(), OpError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| OpError::from_io(&error))?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        std::fs::remove_dir_all(path).map_err(|error| OpError::from_io(&error))
    } else {
        std::fs::remove_file(path).map_err(|error| OpError::from_io(&error))
    }
}
