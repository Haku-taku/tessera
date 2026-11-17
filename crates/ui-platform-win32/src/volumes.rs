//! Unmounted removable volumes on Windows, and mounting one on a free drive letter.
//!
//! Explorer only shows volumes that have a drive letter, but the system knows about media
//! with none: a card reader's slot before Windows assigned it one, a disk whose letter was
//! removed. The volume GUID is the identity; mounting means claiming a letter for it, which
//! is what `SetVolumeMountPointW` does.

use std::path::PathBuf;

use ui_platform::volumes::{Mountable, VolumeMounter};
use windows::Win32::Foundation::ERROR_MORE_DATA;
use windows::Win32::Storage::FileSystem::{
    FindFirstVolumeW, FindNextVolumeW, FindVolumeClose, GetDriveTypeW, GetLogicalDrives,
    GetVolumeInformationW, GetVolumePathNamesForVolumeNameW, SetVolumeMountPointW,
};
use windows::Win32::System::WindowsProgramming::DRIVE_REMOVABLE;
use windows::core::PCWSTR;

/// `MAX_PATH` plus the terminator: what every volume-name call requires.
const NAME_LENGTH: usize = 261;
/// A volume's mount points fit well inside this; the buffer only has to be large enough to
/// tell "any path" from "none".
const PATH_LENGTH: usize = 4096;

pub(crate) struct Win32Volumes;

impl VolumeMounter for Win32Volumes {
    fn mountable(&self) -> Result<Vec<Mountable>, String> {
        let mut name = [0u16; NAME_LENGTH];
        let handle =
            unsafe { FindFirstVolumeW(&mut name) }.map_err(|error| format!("volumes: {error}"))?;
        let mut found = Vec::new();
        loop {
            // A fixed disk with no letter is a partition, not media the user inserted and
            // may expect in the list; only removable volumes are offered.
            if unsafe { GetDriveTypeW(PCWSTR(name.as_ptr())) } == DRIVE_REMOVABLE
                && !has_mount_point(&name)
            {
                let id = text(&name);
                if !id.is_empty() {
                    found.push(Mountable {
                        label: volume_label(&name),
                        id,
                    });
                }
            }
            if unsafe { FindNextVolumeW(handle, &mut name) }.is_err() {
                break;
            }
        }
        let _ = unsafe { FindVolumeClose(handle) };
        Ok(found)
    }

    fn mount(&self, id: &str) -> Result<PathBuf, String> {
        let volume: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
        let used = unsafe { GetLogicalDrives() };
        // A and B are the floppy letters and are left alone; C is the system's own.
        for letter in b'D'..=b'Z' {
            if used & (1u32 << (letter - b'A')) != 0 {
                continue;
            }
            let drive = format!("{}:\\", letter as char);
            let wide: Vec<u16> = drive.encode_utf16().chain(std::iter::once(0)).collect();
            if unsafe {
                SetVolumeMountPointW(PCWSTR(wide.as_ptr()), PCWSTR(volume.as_ptr()))
            }
            .is_ok()
            {
                return Ok(PathBuf::from(drive));
            }
        }
        Err(format!("{id}: no free drive letter"))
    }
}

/// Whether the volume already has a drive letter or a folder mount point. A volume with one
/// is a [`Device`](ui_platform::places::Device) instead, and offering it twice would let a
/// click mount what is already mounted.
fn has_mount_point(volume: &[u16]) -> bool {
    let mut paths = [0u16; PATH_LENGTH];
    let mut length = 0u32;
    match unsafe {
        GetVolumePathNamesForVolumeNameW(PCWSTR(volume.as_ptr()), Some(&mut paths), &mut length)
    } {
        Ok(()) => paths.iter().any(|unit| *unit != 0),
        // A short buffer is reported as an error together with the size needed, which still
        // means a path exists.
        Err(error) => (error.code().0 as u32 & 0xFFFF) == ERROR_MORE_DATA.0,
    }
}

fn volume_label(volume: &[u16]) -> Option<String> {
    let mut buffer = [0u16; NAME_LENGTH];
    unsafe {
        GetVolumeInformationW(
            PCWSTR(volume.as_ptr()),
            Some(&mut buffer),
            None,
            None,
            None,
            None,
        )
    }
    .ok()?;
    let label = text(&buffer);
    (!label.is_empty()).then_some(label)
}

/// The NUL-terminated contents of a wide buffer.
fn text(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}
