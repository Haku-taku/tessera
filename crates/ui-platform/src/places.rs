//! The places and volumes a picker offers: the user's home, the directories the desktop
//! names, and the mounted filesystems it can browse.
//!
//! A place is a fact about the desktop, not about one locale: the provider answers which
//! kind of place exists and where it points, and the picker draws the label from its own
//! catalogue. A device carries a name of its own instead, because only the platform knows
//! what the user calls that disk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Which kind of place this is. The picker maps it to a label and an icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaceKind {
    Home,
    Desktop,
    Documents,
    Downloads,
    Music,
    Pictures,
    Videos,
    Templates,
    PublicShare,
}

/// One place: its kind, and the directory it opens.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Place {
    pub kind: PlaceKind,
    pub path: PathBuf,
}

/// One mounted volume, as a picker's sidebar offers it beside the places.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Device {
    /// The directory the volume is mounted at.
    pub path: PathBuf,
    /// The name the desktop knows the volume by. `None` is a volume with no name of its own,
    /// which the picker names itself; the bare root is the case that exists.
    pub label: Option<String>,
}

/// The mounted local filesystems in `/proc/self/mountinfo`'s text, in mount-point order.
///
/// The selection follows Dolphin rather than `mount`: a volume is a block device the user
/// can browse, not every mount point the kernel holds. A pseudo filesystem or a network
/// share has no `/dev/` source, a loop device is a package or an image rather than a volume,
/// and a bind mount, a container's view or a filesystem mounted twice repeats a device that
/// is already listed — one row per source, at its shallowest mount point. `label_of` names a
/// block device the way the desktop does; the mount point names a volume without a label.
///
/// The text is Linux's, and the one backend with mount points hands it over; the parse is
/// pure, so the selection is testable without a machine that mounts anything.
pub fn linux_mounts(text: &str, label_of: &dyn Fn(&Path) -> Option<String>) -> Vec<Device> {
    let mut by_device: HashMap<&str, Device> = HashMap::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split(' ').collect();
        let Some(separator) = fields.iter().position(|field| *field == "-") else {
            continue;
        };
        // id, parent, major:minor, root, mount point, options, optional fields…, "-",
        // fstype, source, super options.
        if separator < 6 || separator + 2 >= fields.len() {
            continue;
        }
        let source = fields[separator + 2];
        if !source.starts_with("/dev/") {
            continue;
        }
        let Some(device_name) = Path::new(source).file_name() else {
            continue;
        };
        // A loop device is a snap package or a mounted image; the desktop does not offer
        // them as volumes.
        if device_name.to_string_lossy().starts_with("loop") {
            continue;
        }
        let mount_point = PathBuf::from(unescape(fields[4]));
        match by_device.get(&source) {
            Some(kept) if depth(&kept.path) <= depth(&mount_point) => continue,
            _ => {}
        }
        let label = label_of(Path::new(source)).or_else(|| {
            mount_point
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        });
        by_device.insert(
            source,
            Device {
                path: mount_point,
                label,
            },
        );
    }
    let mut devices: Vec<Device> = by_device.into_values().collect();
    devices.sort_by(|left, right| left.path.cmp(&right.path));
    devices
}

fn depth(path: &Path) -> usize {
    path.components().count()
}

/// Undo the kernel's escaping of a mount table field: `\040` is a space, `\011` a tab,
/// `\012` a newline and `\134` a backslash. Any other byte is the name's own.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\'
            && let Some([first, second, third]) = bytes.get(index + 1..index + 4)
            && let (Some(first), Some(second), Some(third)) =
                (octal(*first), octal(*second), octal(*third))
        {
            out.push(first * 64 + second * 8 + third);
            index += 4;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn octal(byte: u8) -> Option<u8> {
    (b'0'..=b'7').contains(&byte).then_some(byte - b'0')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture is one line per case: the root filesystem, a separate home partition, a
    /// btrfs subvolume and a bind mount of the same device, a snap's loop mount, tmpfs, a
    /// CIFS share, and a mount point whose name the kernel escaped.
    const MOUNTINFO: &str = "\
22 1 259:2 / / rw,relatime - ext4 /dev/nvme0n1p2 rw
23 22 259:3 / /home rw,relatime - btrfs /dev/nvme0n1p3 rw
24 22 259:3 /@snapshots /snapshots rw,relatime - btrfs /dev/nvme0n1p3 rw
25 22 259:2 /var/lib/docker /var/lib/docker rw,relatime - ext4 /dev/nvme0n1p2 rw
26 22 7:0 / /snap/core/123 rw,relatime - squashfs /dev/loop0 ro
27 22 0:40 / /run/user/1000 rw,nosuid - tmpfs tmpfs rw
28 22 0:41 / /mnt/share rw,relatime - cifs //nas/share rw
29 22 259:4 / /media/My\\040Disk rw,relatime - exfat /dev/sda1 rw
";

    fn label_of(source: &Path) -> Option<String> {
        (source == Path::new("/dev/sda1")).then(|| "USB".to_string())
    }

    /// The sidebar lists a volume once, at the mount point a user would guess, and leaves
    /// out what is not a volume: the loop mount, the pseudo filesystem, the network share,
    /// and the repeats of a device already listed.
    #[test]
    fn a_mount_table_yields_one_row_per_volume() {
        let devices = linux_mounts(MOUNTINFO, &label_of);
        let paths: Vec<&Path> = devices.iter().map(|device| device.path.as_path()).collect();
        assert_eq!(
            paths,
            [
                Path::new("/"),
                Path::new("/home"),
                Path::new("/media/My Disk"),
            ]
        );
        assert_eq!(devices[2].label.as_deref(), Some("USB"));
        assert_eq!(devices[1].label.as_deref(), Some("home"));
        assert_eq!(devices[0].label, None, "the root has no name of its own");
    }
}
