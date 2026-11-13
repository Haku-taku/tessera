//! A DRM syncobj timeline: the kernel object explicit synchronization is built on.
//!
//! `libdrm` is loaded with `dlopen`, like `libgbm`, so a machine without it still builds.

use std::ffi::{CStr, c_void};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

type SyncobjCreate = unsafe extern "system" fn(i32, u32, *mut u32) -> i32;
type SyncobjDestroy = unsafe extern "system" fn(i32, u32) -> i32;
type SyncobjHandleToFd = unsafe extern "system" fn(i32, u32, *mut i32) -> i32;
type SyncobjImportSyncFile = unsafe extern "system" fn(i32, u32, i32) -> i32;
type SyncobjTransfer = unsafe extern "system" fn(i32, u32, u64, u32, u64, u32) -> i32;
type SyncobjTimelineWait =
    unsafe extern "system" fn(i32, *const u32, *const u64, u32, i64, u32, *mut u32) -> i32;
type GetCap = unsafe extern "system" fn(i32, u64, *mut u64) -> i32;

/// `DRM_SYNCOBJ_WAIT_FLAGS_WAIT_FOR_SUBMIT`: wait for a point that is not submitted yet.
const WAIT_FOR_SUBMIT: u32 = 1 << 1;

/// `DRM_CAP_SYNCOBJ_TIMELINE`: the kernel can hold a point timeline at all.
const DRM_CAP_SYNCOBJ_TIMELINE: u64 = 0x14;

/// One timeline syncobj on one DRM device.
pub struct SyncTimeline {
    /// Dup of the device fd, so the timeline outlives a device handle swap.
    fd: OwnedFd,
    /// The syncobj's own fd, which is what `import_timeline` takes.
    syncobj_fd: OwnedFd,
    handle: u32,
    /// Binary syncobj that receives one fence before it is moved onto a point.
    scratch: u32,
    destroy: SyncobjDestroy,
    import_sync_file: SyncobjImportSyncFile,
    transfer: SyncobjTransfer,
    timeline_wait: SyncobjTimelineWait,
}

impl SyncTimeline {
    /// Create a timeline on `device_fd`, e.g. the GBM device's DRM node.
    pub fn new(device_fd: i32) -> Result<Self, crate::GpuError> {
        // SAFETY: dlopen/dlsym with a NUL-terminated name; symbols are transmuted to the
        // signatures libdrm documents.
        unsafe {
            let lib = libc::dlopen(c"libdrm.so.2".as_ptr(), libc::RTLD_NOW);
            if lib.is_null() {
                return Err(crate::GpuError::SyncObj("cannot dlopen libdrm.so.2"));
            }
            let sym = |name: &CStr| -> Result<*mut c_void, crate::GpuError> {
                let p = libc::dlsym(lib, name.as_ptr());
                if p.is_null() {
                    Err(crate::GpuError::SyncObj("libdrm symbol missing"))
                } else {
                    Ok(p)
                }
            };
            let create: SyncobjCreate = std::mem::transmute(sym(c"drmSyncobjCreate")?);
            let destroy: SyncobjDestroy = std::mem::transmute(sym(c"drmSyncobjDestroy")?);
            let handle_to_fd: SyncobjHandleToFd =
                std::mem::transmute(sym(c"drmSyncobjHandleToFD")?);
            let import_sync_file: SyncobjImportSyncFile =
                std::mem::transmute(sym(c"drmSyncobjImportSyncFile")?);
            let transfer: SyncobjTransfer = std::mem::transmute(sym(c"drmSyncobjTransfer")?);
            let timeline_wait: SyncobjTimelineWait =
                std::mem::transmute(sym(c"drmSyncobjTimelineWait")?);
            let get_cap: GetCap = std::mem::transmute(sym(c"drmGetCap")?);
            // Without the capability every transfer fails, so fall back before attaching.
            let mut timeline: u64 = 0;
            if get_cap(device_fd, DRM_CAP_SYNCOBJ_TIMELINE, &mut timeline) != 0 || timeline == 0 {
                return Err(crate::GpuError::SyncObj("no timeline syncobj support"));
            }

            let dup = libc::dup(device_fd);
            if dup < 0 {
                return Err(crate::GpuError::SyncObj("dup device fd"));
            }
            let fd = OwnedFd::from_raw_fd(dup);
            let mut handle: u32 = 0;
            if create(fd.as_raw_fd(), 0, &mut handle) != 0 {
                return Err(crate::GpuError::SyncObj("drmSyncobjCreate"));
            }
            let mut scratch: u32 = 0;
            if create(fd.as_raw_fd(), 0, &mut scratch) != 0 {
                let _ = destroy(fd.as_raw_fd(), handle);
                return Err(crate::GpuError::SyncObj("drmSyncobjCreate"));
            }
            let mut obj_fd: i32 = -1;
            if handle_to_fd(fd.as_raw_fd(), handle, &mut obj_fd) != 0 || obj_fd < 0 {
                let _ = destroy(fd.as_raw_fd(), handle);
                let _ = destroy(fd.as_raw_fd(), scratch);
                return Err(crate::GpuError::SyncObj("drmSyncobjHandleToFD"));
            }
            Ok(Self {
                fd,
                syncobj_fd: OwnedFd::from_raw_fd(obj_fd),
                handle,
                scratch,
                destroy,
                import_sync_file,
                transfer,
                timeline_wait,
            })
        }
    }

    /// Duplicate the syncobj fd, so it can be handed to another process (the compositor).
    pub fn dup_fd(&self) -> Result<OwnedFd, crate::GpuError> {
        // SAFETY: the syncobj fd is a valid open descriptor; `dup` returns a new one or -1.
        let dup = unsafe { libc::dup(self.syncobj_fd.as_raw_fd()) };
        if dup < 0 {
            return Err(crate::GpuError::SyncObj("dup timeline fd"));
        }
        Ok(unsafe { OwnedFd::from_raw_fd(dup) })
    }

    /// Import `fence` as timeline point `point`, which must exceed every earlier point.
    ///
    /// `drmSyncobjImportSyncFile` alone only replaces a binary fence; a timeline point
    /// comes only from `drmSyncobjTransfer`, so the fence is staged in a scratch syncobj.
    pub fn import_fence_at(&self, fence: &OwnedFd, point: u64) -> Result<(), crate::GpuError> {
        // SAFETY: both handles belong to this device and `fence` is a valid sync_file.
        unsafe {
            let fd = self.fd.as_raw_fd();
            if (self.import_sync_file)(fd, self.scratch, fence.as_raw_fd()) != 0 {
                return Err(crate::GpuError::SyncObj("drmSyncobjImportSyncFile"));
            }
            if (self.transfer)(fd, self.handle, point, self.scratch, 0, 0) != 0 {
                return Err(crate::GpuError::SyncObj("drmSyncobjTransfer"));
            }
            Ok(())
        }
    }

    /// Whether the timeline has reached `point` within `timeout`.
    ///
    /// The compositor drives this timeline: it signals a point once it has stopped reading
    /// the buffer that carried it. An unsubmitted point waits instead of failing, and a
    /// timeout means "not yet", never an error the caller must handle.
    pub fn wait(&self, point: u64, timeout: std::time::Duration) -> bool {
        // The kernel takes an absolute CLOCK_MONOTONIC deadline, not a duration.
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `ts` is a valid timespec and CLOCK_MONOTONIC always exists.
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
        let now = ts.tv_sec * 1_000_000_000 + ts.tv_nsec;
        let deadline = now.saturating_add(timeout.as_nanos().min(i64::MAX as u128) as i64);
        // SAFETY: the handle belongs to this timeline, and both arrays outlive the call.
        unsafe {
            let mut signaled: u32 = 0;
            let rc = (self.timeline_wait)(
                self.fd.as_raw_fd(),
                &self.handle,
                &point,
                1,
                deadline,
                WAIT_FOR_SUBMIT,
                &mut signaled,
            );
            rc == 0
        }
    }
}

impl Drop for SyncTimeline {
    fn drop(&mut self) {
        // SAFETY: both handles were created on this fd and are destroyed exactly once.
        unsafe {
            let _ = (self.destroy)(self.fd.as_raw_fd(), self.handle);
            let _ = (self.destroy)(self.fd.as_raw_fd(), self.scratch);
        }
    }
}
