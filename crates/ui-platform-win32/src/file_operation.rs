//! The Win32 half of file operations: volume identity, the recycle bin, symbolic links and
//! the name rules the filesystem enforces.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use ui_platform::file_operation::{FileOperator, LinkKind, OpError};
use windows::Win32::Foundation::{
    CloseHandle, E_ABORT, ERROR_SUCCESS, HANDLE, LUID, RPC_E_CHANGED_MODE,
};
use windows::Win32::Security::{
    GetTokenInformation, LookupPrivilegeValueW, TOKEN_PRIVILEGES, TOKEN_QUERY, TokenPrivileges,
};
use windows::Win32::Storage::FileSystem::{
    CreateSymbolicLinkW, GetVolumeInformationW, GetVolumePathNameW, SYMBOLIC_LINK_FLAGS,
    SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE, SYMBOLIC_LINK_FLAG_DIRECTORY,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize,
};
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::{
    FOFX_ADDUNDORECORD, FOFX_RECYCLEONDELETE, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT,
    FileOperation, IFileOperation, IShellItem, SHCreateItemFromParsingName,
};
use windows::core::{PCWSTR, w};

pub(crate) struct WindowsOperator;

impl FileOperator for WindowsOperator {
    fn same_volume(&self, a: &Path, b: &Path) -> bool {
        match (volume(a), volume(b)) {
            // The serial names one volume however many mount points it has.
            (Some((_, a)), Some((_, b))) if a != 0 && b != 0 => a == b,
            (Some((a, _)), Some((b, _))) => a == b,
            // A volume that cannot be named is not known to be the same one; a copy is the
            // safe answer, because it does not delete the source.
            _ => false,
        }
    }

    fn trash(&self, path: &Path) -> Result<(), OpError> {
        let wide = wide(path);
        // SAFETY: every COM object below is released before `Apartment` leaves, and the
        // pointers stay valid for the duration of the call that reads them.
        unsafe {
            let apartment = Apartment::enter()?;
            let result = (|| -> windows::core::Result<()> {
                let operation: IFileOperation =
                    CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)?;
                operation.SetOperationFlags(
                    FOFX_RECYCLEONDELETE
                        | FOF_NOCONFIRMATION
                        | FOF_SILENT
                        | FOF_NOERRORUI
                        | FOFX_ADDUNDORECORD,
                )?;
                let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)?;
                operation.DeleteItem(&item, None)?;
                operation.PerformOperations()?;
                if operation.GetAnyOperationsAborted()?.as_bool() {
                    return Err(windows::core::Error::from(E_ABORT));
                }
                Ok(())
            })();
            drop(apartment);
            result.map_err(|error| OpError::Io(format!("the recycle bin refused it: {error}")))
        }
    }

    fn symlink(&self, target: &Path, link: &Path, directory: bool) -> Result<(), OpError> {
        let target = wide(target);
        let link = wide(link);
        let mut flags = SYMBOLIC_LINK_FLAG_ALLOW_UNPRIVILEGED_CREATE.0;
        if directory {
            flags |= SYMBOLIC_LINK_FLAG_DIRECTORY.0;
        }
        // SAFETY: both pointers name NUL-terminated buffers owned by this call.
        let created = unsafe {
            CreateSymbolicLinkW(
                PCWSTR(link.as_ptr()),
                PCWSTR(target.as_ptr()),
                SYMBOLIC_LINK_FLAGS(flags),
            )
        };
        if created {
            Ok(())
        } else {
            Err(OpError::Io(format!(
                "the link was refused: {}",
                std::io::Error::last_os_error()
            )))
        }
    }

    fn supports_links(&self, kind: LinkKind) -> bool {
        match kind {
            LinkKind::Hard => true,
            LinkKind::Symbolic => symbolic_link_allowed(),
        }
    }

    fn validate_name(&self, name: &str) -> Result<(), OpError> {
        if name.is_empty() {
            return Err(OpError::Unsupported);
        }
        if name.chars().any(|character| {
            matches!(character, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
                || (character as u32) < 0x20
        }) {
            return Err(OpError::Unsupported);
        }
        if name.ends_with(' ') || name.ends_with('.') {
            return Err(OpError::Unsupported);
        }
        // A device name is reserved with or without an extension: `NUL.txt` is still `NUL`.
        let stem = name
            .split('.')
            .next()
            .unwrap_or(name)
            .trim_end_matches(' ')
            .to_ascii_uppercase();
        const RESERVED: [&str; 22] = [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];
        if RESERVED.contains(&stem.as_str()) {
            return Err(OpError::Unsupported);
        }
        Ok(())
    }
}

/// An apartment on the worker thread, released only when this call made it.
struct Apartment {
    uninitialize: bool,
}

impl Apartment {
    fn enter() -> Result<Self, OpError> {
        // SAFETY: the call takes no pointer this caller owns; the matching release is in
        // `drop`, which knows whether this call started the apartment.
        let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if initialized == RPC_E_CHANGED_MODE {
            // The worker already has another model. IFileOperation may still be created
            // here; tearing the apartment down would be ours to pay for and not to do.
            return Ok(Self {
                uninitialize: false,
            });
        }
        if initialized.is_err() {
            return Err(OpError::Unsupported);
        }
        Ok(Self { uninitialize: true })
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.uninitialize {
            // SAFETY: the matching `CoInitializeEx` above succeeded on this thread.
            unsafe { CoUninitialize() };
        }
    }
}

/// The volume a path sits on, as its mount root and serial number.
fn volume(path: &Path) -> Option<(String, u32)> {
    let wide = wide(path);
    let mut root = [0u16; 260];
    // SAFETY: `root` is a writable buffer whose length is taken from the slice, and `wide`
    // is NUL-terminated.
    let named = unsafe { GetVolumePathNameW(PCWSTR(wide.as_ptr()), &mut root) }.is_ok();
    if !named {
        return None;
    }
    let length = root.iter().position(|unit| *unit == 0).unwrap_or(root.len());
    if length == 0 {
        return None;
    }
    let mut serial = 0u32;
    // SAFETY: the root is the NUL-terminated prefix the call above wrote.
    let named = unsafe {
        GetVolumeInformationW(
            PCWSTR(root.as_ptr()),
            None,
            Some(&mut serial),
            None,
            None,
            None,
        )
    }
    .is_ok();
    let root = String::from_utf16_lossy(&root[..length]).to_ascii_lowercase();
    Some((root, if named { serial } else { 0 }))
}

/// Whether this user may create a symbolic link: the privilege, or Developer Mode.
fn symbolic_link_allowed() -> bool {
    // SAFETY: the token is closed on every path, and the buffer is sized by the API before
    // it is read.
    unsafe {
        if developer_mode() {
            return true;
        }
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut size = 0u32;
        let _ = GetTokenInformation(token, TokenPrivileges, None, 0, &mut size);
        if size == 0 {
            let _ = CloseHandle(token);
            return false;
        }
        let mut buffer = vec![0u8; size as usize];
        let read = GetTokenInformation(
            token,
            TokenPrivileges,
            Some(buffer.as_mut_ptr().cast()),
            size,
            &mut size,
        )
        .is_ok();
        let _ = CloseHandle(token);
        if !read {
            return false;
        }
        let privileges = &*(buffer.as_ptr() as *const TOKEN_PRIVILEGES);
        let entries = std::slice::from_raw_parts(
            privileges.Privileges.as_ptr(),
            privileges.PrivilegeCount as usize,
        );
        let mut wanted = LUID::default();
        if LookupPrivilegeValueW(None, w!("SeCreateSymbolicLinkPrivilege"), &mut wanted).is_err() {
            return false;
        }
        entries.iter().any(|entry| entry.Luid == wanted)
    }
}

/// `AllowDevelopmentWithoutDevLicense` is what makes the unprivileged create flag work.
fn developer_mode() -> bool {
    let subkey = w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\AppModelUnlock");
    let value = w!("AllowDevelopmentWithoutDevLicense");
    let mut enabled = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: the value name and the output buffer are sized and owned by this call.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey,
            value,
            RRF_RT_REG_DWORD,
            None,
            Some((&mut enabled as *mut u32).cast()),
            Some(&mut size),
        )
    };
    status == ERROR_SUCCESS && enabled == 1
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
