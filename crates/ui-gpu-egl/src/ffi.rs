//! Raw GBM bindings and EGL extension entry points.
//!
//! GBM is loaded with `dlopen` so the crate builds on machines without a DRM
//! device; the failure is reported at runtime instead of link time.

use std::ffi::{CStr, c_void};

use khronos_egl as egl;

/// `GBM_FORMAT_ARGB8888`, the only format this backend presents today.
pub const GBM_FORMAT_ARGB8888: u32 = 0x3432_5241;
/// `GBM_BO_USE_RENDERING`.
pub const GBM_BO_USE_RENDERING: u32 = 1 << 2;

pub const EGL_PLATFORM_GBM_KHR: egl::Enum = 0x31D7;
pub const EGL_LINUX_DMA_BUF_EXT: egl::Enum = 0x3270;
pub const EGL_LINUX_DRM_FOURCC_EXT: egl::Int = 0x3271;
pub const EGL_DMA_BUF_PLANE0_FD_EXT: egl::Int = 0x3272;
pub const EGL_DMA_BUF_PLANE0_OFFSET_EXT: egl::Int = 0x3273;
pub const EGL_DMA_BUF_PLANE0_PITCH_EXT: egl::Int = 0x3274;
pub const EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT: egl::Int = 0x3443;
pub const EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT: egl::Int = 0x3444;

type BoCreate = unsafe extern "system" fn(*mut c_void, u32, u32, u32, u32) -> *mut c_void;
type BoGetFd = unsafe extern "system" fn(*mut c_void) -> i32;
type BoGetStride = unsafe extern "system" fn(*mut c_void) -> u32;
type BoGetOffset = unsafe extern "system" fn(*mut c_void, i32) -> u32;
type BoGetModifier = unsafe extern "system" fn(*mut c_void) -> u64;
type BoDestroy = unsafe extern "system" fn(*mut c_void);
type DeviceDestroy = unsafe extern "system" fn(*mut c_void);

/// `egl::Display` is not `repr(transparent)`, so raw pointers keep these FFI-safe.
pub type CreateImage = unsafe extern "system" fn(
    *mut c_void,
    *mut c_void,
    egl::Enum,
    *mut c_void,
    *const egl::Int,
) -> *mut c_void;
pub type DestroyImage = unsafe extern "system" fn(*mut c_void, *mut c_void) -> egl::Boolean;
pub type ImageTargetTexture = unsafe extern "system" fn(u32, *mut c_void);

/// A loaded `libgbm` device and the entry points this backend needs.
pub struct GbmDevice {
    dev: *mut c_void,
    /// The DRM node descriptor; `gbm_create_device` does not take ownership of it.
    fd: i32,
    /// Kept alive for the lifetime of `dev`.
    _lib: *mut c_void,
    bo_create: BoCreate,
    bo_get_fd: BoGetFd,
    bo_get_stride: BoGetStride,
    bo_get_offset: BoGetOffset,
    bo_get_modifier: BoGetModifier,
    bo_destroy: BoDestroy,
    device_destroy: DeviceDestroy,
}

// The GBM device is only ever used from the thread owning the GL context.
unsafe impl Send for GbmDevice {}

impl GbmDevice {
    /// Open `node` and create a GBM device on it.
    pub fn open(node: &CStr) -> Result<Self, crate::GpuError> {
        let fd = unsafe { libc::open(node.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
        if fd < 0 {
            return Err(crate::GpuError::OpenNode {
                path: node.to_string_lossy().into_owned(),
                errno: std::io::Error::last_os_error().raw_os_error().unwrap_or(0),
            });
        }
        let lib = unsafe { libc::dlopen(c"libgbm.so.1".as_ptr(), libc::RTLD_NOW) };
        if lib.is_null() {
            return Err(crate::GpuError::GbmUnavailable("cannot dlopen libgbm.so.1"));
        }
        unsafe fn sym(lib: *mut c_void, name: &CStr) -> Result<*mut c_void, crate::GpuError> {
            let p = unsafe { libc::dlsym(lib, name.as_ptr()) };
            if p.is_null() {
                // Leak the name to keep the error message `'static`.
                let leaked: &'static str =
                    Box::leak(name.to_string_lossy().into_owned().into_boxed_str());
                Err(crate::GpuError::GbmUnavailable(leaked))
            } else {
                Ok(p)
            }
        }
        unsafe {
            let create: unsafe extern "system" fn(i32) -> *mut c_void =
                std::mem::transmute(sym(lib, c"gbm_create_device")?);
            let dev = create(fd);
            if dev.is_null() {
                return Err(crate::GpuError::Allocation("gbm_create_device"));
            }
            Ok(Self {
                dev,
                fd,
                _lib: lib,
                bo_create: std::mem::transmute(sym(lib, c"gbm_bo_create")?),
                bo_get_fd: std::mem::transmute(sym(lib, c"gbm_bo_get_fd")?),
                bo_get_stride: std::mem::transmute(sym(lib, c"gbm_bo_get_stride")?),
                bo_get_offset: std::mem::transmute(sym(lib, c"gbm_bo_get_offset")?),
                bo_get_modifier: std::mem::transmute(sym(lib, c"gbm_bo_get_modifier")?),
                bo_destroy: std::mem::transmute(sym(lib, c"gbm_bo_destroy")?),
                device_destroy: std::mem::transmute(sym(lib, c"gbm_device_destroy")?),
            })
        }
    }

    /// Raw `gbm_device*` for `eglGetPlatformDisplay`.
    pub fn raw(&self) -> *mut c_void {
        self.dev
    }

    /// The DRM node descriptor, for syncobj ioctls.
    pub fn fd(&self) -> i32 {
        self.fd
    }

    /// Allocate a renderable buffer object.
    pub fn create_buffer(
        &self,
        w: u32,
        h: u32,
        format: u32,
        flags: u32,
    ) -> Result<GbmBuffer, crate::GpuError> {
        let bo = unsafe { (self.bo_create)(self.dev, w, h, format, flags) };
        if bo.is_null() {
            return Err(crate::GpuError::Allocation("gbm_bo_create"));
        }
        let fd = unsafe { (self.bo_get_fd)(bo) };
        if fd < 0 {
            unsafe { (self.bo_destroy)(bo) };
            return Err(crate::GpuError::Allocation("gbm_bo_get_fd"));
        }
        Ok(GbmBuffer {
            bo,
            fd,
            width: w,
            height: h,
            stride: unsafe { (self.bo_get_stride)(bo) },
            offset: unsafe { (self.bo_get_offset)(bo, 0) },
            modifier: unsafe { (self.bo_get_modifier)(bo) },
            bo_destroy: self.bo_destroy,
        })
    }
}

impl Drop for GbmDevice {
    fn drop(&mut self) {
        unsafe {
            (self.device_destroy)(self.dev);
            libc::close(self.fd);
        }
    }
}

/// One GBM buffer plus the dma-buf description the compositor needs.
pub struct GbmBuffer {
    bo: *mut c_void,
    /// Owned dma-buf file descriptor.
    pub fd: i32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub offset: u32,
    /// DRM format modifier; must be forwarded to the compositor verbatim.
    pub modifier: u64,
    bo_destroy: BoDestroy,
}

// The buffer is described by a plain fd; the raw `bo` only frees it.
unsafe impl Send for GbmBuffer {}

impl GbmBuffer {
    /// Raw `gbm_bo*`, needed to build the EGLImage.
    pub fn raw(&self) -> *mut c_void {
        self.bo
    }

    /// Take ownership of the fd, leaving the buffer unable to close it.
    pub fn take_fd(&mut self) -> i32 {
        let fd = self.fd;
        self.fd = -1;
        fd
    }
}

impl Drop for GbmBuffer {
    fn drop(&mut self) {
        if self.fd >= 0 {
            unsafe { libc::close(self.fd) };
        }
        unsafe { (self.bo_destroy)(self.bo) };
    }
}
