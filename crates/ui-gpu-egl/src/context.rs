//! EGL display and OpenGL ES context bound to a DRM render node.

use std::ffi::CStr;

use glow::HasContext;
use khronos_egl as egl;

use crate::GpuError;
use crate::ffi::{
    self, CreateImage, DestroyImage, EGL_PLATFORM_GBM_KHR, GbmDevice, ImageTargetTexture,
};

const EGL_NONE: egl::Int = 0x3038;

/// A sync_file exported from the GL queue: it signals when the queued work lands.
pub struct ExportedFence {
    /// The sync_file descriptor to import into a DRM timeline.
    pub fd: std::os::fd::OwnedFd,
}

/// Image functions a dma-buf backed target needs.
struct ImageFns {
    create: CreateImage,
    destroy: DestroyImage,
    target_texture: ImageTargetTexture,
}

/// A live EGL display + GL context on one GPU.
pub struct EglContext {
    egl: egl::DynamicInstance<egl::EGL1_5>,
    display: egl::Display,
    context: egl::Context,
    gl: std::rc::Rc<glow::Context>,
    images: ImageFns,
    gbm: GbmDevice,
    is_gles: bool,
    /// Whether the driver can create a native fence; `None` until the first export tries.
    native_fence: std::cell::Cell<Option<bool>>,
}

impl EglContext {
    /// Create a context on `node`, e.g. `/dev/dri/renderD128`.
    pub fn new(node: &CStr) -> Result<Self, GpuError> {
        let egl = unsafe { egl::DynamicInstance::<egl::EGL1_5>::load_required() }
            .map_err(|e| GpuError::Egl(format!("cannot load libEGL: {e:?}")))?;
        let gbm = GbmDevice::open(node)?;

        // The GBM platform, not `EGL_EXT_platform_device`: GLVND loads every EGL vendor at
        // display creation, so the device platform saves no memory (design-errors, 2026-11-04).
        let display = unsafe {
            egl.get_platform_display(EGL_PLATFORM_GBM_KHR, gbm.raw(), &[egl::ATTRIB_NONE])
        }
        .map_err(|e| GpuError::Egl(format!("no GBM display: {e:?}")))?;
        egl.initialize(display)
            .map_err(|e| GpuError::Egl(format!("eglInitialize: {e:?}")))?;

        let extensions = egl
            .query_string(Some(display), egl::EXTENSIONS)
            .map(|c| c.to_string_lossy().into_owned())
            .unwrap_or_default();
        for required in ["EGL_EXT_image_dma_buf_import", "EGL_KHR_image_base"] {
            if !extensions.contains(required) {
                return Err(GpuError::MissingExtension(required_message(required)));
            }
        }

        // No SURFACE_TYPE: GBM displays need not expose pbuffer configs.
        let config = egl
            .choose_first_config(
                display,
                &[
                    egl::RENDERABLE_TYPE,
                    egl::OPENGL_ES3_BIT,
                    egl::RED_SIZE,
                    8,
                    egl::GREEN_SIZE,
                    8,
                    egl::BLUE_SIZE,
                    8,
                    egl::ALPHA_SIZE,
                    8,
                    EGL_NONE,
                ],
            )
            .map_err(|e| GpuError::Egl(format!("choose_config: {e:?}")))?
            .ok_or_else(|| GpuError::Egl("no GLES3 config".into()))?;

        let context = egl
            .create_context(
                display,
                config,
                None,
                &[
                    egl::CONTEXT_MAJOR_VERSION,
                    3,
                    egl::CONTEXT_MINOR_VERSION,
                    0,
                    EGL_NONE,
                ],
            )
            .map_err(|e| GpuError::Egl(format!("create_context: {e:?}")))?;
        egl.make_current(display, None, None, Some(context))
            .map_err(|e| GpuError::Egl(format!("make_current: {e:?}")))?;

        let gl = unsafe {
            glow::Context::from_loader_function(|s| {
                egl.get_proc_address(s)
                    .map_or(std::ptr::null(), |f| f as *const _)
            })
        };
        let version = unsafe { gl.get_parameter_string(glow::VERSION) };
        let is_gles = version.contains("OpenGL ES");

        let create = egl
            .get_proc_address("eglCreateImageKHR")
            .ok_or(GpuError::MissingExtension("eglCreateImageKHR"))?;
        let destroy = egl
            .get_proc_address("eglDestroyImageKHR")
            .ok_or(GpuError::MissingExtension("eglDestroyImageKHR"))?;
        let target_texture = egl
            .get_proc_address("glEGLImageTargetTexture2DOES")
            .ok_or(GpuError::MissingExtension("glEGLImageTargetTexture2DOES"))?;
        let images = ImageFns {
            create: unsafe { std::mem::transmute(create) },
            destroy: unsafe { std::mem::transmute(destroy) },
            target_texture: unsafe { std::mem::transmute(target_texture) },
        };

        Ok(Self {
            egl,
            display,
            context,
            gl: std::rc::Rc::new(gl),
            images,
            gbm,
            is_gles,
            native_fence: std::cell::Cell::new(None),
        })
    }

    /// Make this context the thread's current one.
    ///
    /// EGL's current context is thread state, not per-object: with two windows on one thread
    /// whichever context was created or released last is current, so every piece of GL work
    /// states which context it belongs to first.
    pub fn make_current(&self) -> Result<(), GpuError> {
        self.egl
            .make_current(self.display, None, None, Some(self.context))
            .map_err(|e| GpuError::Egl(format!("make_current: {e:?}")))
    }

    /// The GL context.
    pub fn gl(&self) -> &glow::Context {
        &self.gl
    }

    /// The same context as a shared handle, for a device that outlives the borrow.
    pub fn gl_handle(&self) -> std::rc::Rc<glow::Context> {
        std::rc::Rc::clone(&self.gl)
    }

    /// Human-readable renderer string, for reporting which GPU is in use.
    pub fn renderer(&self) -> String {
        unsafe { self.gl.get_parameter_string(glow::RENDERER) }
    }

    /// Version string, useful when picking a shader dialect.
    pub fn version(&self) -> String {
        unsafe { self.gl.get_parameter_string(glow::VERSION) }
    }

    /// Whether the context is OpenGL ES (affects the GLSL header).
    pub fn is_gles(&self) -> bool {
        self.is_gles
    }

    pub(crate) fn gbm(&self) -> &GbmDevice {
        &self.gbm
    }

    /// The DRM node descriptor, for syncobj timelines.
    pub fn drm_fd(&self) -> i32 {
        self.gbm.fd()
    }

    /// Export a native fence for the GL work submitted so far, without blocking.
    ///
    /// `eglDupNativeFenceFDANDROID` flushes the GL queue and returns a sync_file, which is
    /// what explicit synchronization imports into a timeline; it replaces `glFinish`.
    ///
    /// Drivers that refuse a native-fence sync object (NVIDIA) still hand a fence-sync
    /// marker's fd over, which signals with the same work; both paths avoid `glFinish`.
    pub fn export_fence(&self) -> Result<ExportedFence, GpuError> {
        use std::ffi::c_void;
        use std::os::fd::FromRawFd;
        const EGL_SYNC_NATIVE_FENCE_ANDROID: egl::Enum = 0x3140;
        const EGL_SYNC_FENCE_KHR: egl::Enum = 0x30F9;
        type CreateSync =
            unsafe extern "system" fn(*mut c_void, egl::Enum, *const egl::Int) -> *mut c_void;
        type DupFence = unsafe extern "system" fn(*mut c_void, *mut c_void) -> egl::Int;
        type DestroySync = unsafe extern "system" fn(*mut c_void, *mut c_void) -> egl::Boolean;
        let create: CreateSync = unsafe {
            std::mem::transmute(
                self.egl
                    .get_proc_address("eglCreateSyncKHR")
                    .ok_or(GpuError::MissingExtension("eglCreateSyncKHR"))?,
            )
        };
        let dup: DupFence = unsafe {
            std::mem::transmute(
                self.egl
                    .get_proc_address("eglDupNativeFenceFDANDROID")
                    .ok_or(GpuError::MissingExtension("eglDupNativeFenceFDANDROID"))?,
            )
        };
        let destroy: DestroySync = unsafe {
            std::mem::transmute(
                self.egl
                    .get_proc_address("eglDestroySyncKHR")
                    .ok_or(GpuError::MissingExtension("eglDestroySyncKHR"))?,
            )
        };
        let dpy = self.display.as_ptr();
        let attribs: [egl::Int; 1] = [EGL_NONE];
        // A real GPU fence, when the driver can create one (Mesa on AMD/Intel).
        if self.native_fence.get() != Some(false) {
            let native = unsafe { create(dpy, EGL_SYNC_NATIVE_FENCE_ANDROID, attribs.as_ptr()) };
            if !native.is_null() {
                let fd = unsafe { dup(dpy, native) };
                unsafe {
                    let _ = destroy(dpy, native);
                }
                if fd >= 0 {
                    if self.native_fence.get().is_none() {
                        eprintln!("explicit sync: native fence");
                    }
                    self.native_fence.set(Some(true));
                    return Ok(ExportedFence {
                        fd: unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) },
                    });
                }
            }
            // Clear the failure so it is not misread as a later call's error.
            let _ = self.egl.get_error();
            eprintln!("explicit sync: fence-sync marker instead of a native fence");
            self.native_fence.set(Some(false));
        }
        // NVIDIA refuses a native-fence sync object; a fence-sync marker is still a real
        // GPU fence, and the fd below signals with the same work.
        let marker = unsafe { create(dpy, EGL_SYNC_FENCE_KHR, attribs.as_ptr()) };
        if marker.is_null() {
            let err = self.egl.get_error();
            return Err(GpuError::Egl(format!("eglCreateSyncKHR: {err:?}")));
        }
        let fd = unsafe { dup(dpy, marker) };
        unsafe {
            let _ = destroy(dpy, marker);
        }
        if fd < 0 {
            return Err(GpuError::Egl("eglDupNativeFenceFDANDROID".into()));
        }
        Ok(ExportedFence {
            fd: unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) },
        })
    }

    /// Wrap a dma-buf as an EGLImage. Both Mesa and NVIDIA support this, while
    /// `EGL_KHR_image_pixmap` is absent on NVIDIA.
    pub(crate) fn import_dmabuf(
        &self,
        image: &DmaBufDesc,
    ) -> Result<*mut std::ffi::c_void, GpuError> {
        let _ = &self.images;
        // Width and height are mandatory; omitting them yields EGL_BAD_PARAMETER
        // on both Mesa and NVIDIA.
        let attribs: [egl::Int; 17] = [
            egl::WIDTH,
            image.width as egl::Int,
            egl::HEIGHT,
            image.height as egl::Int,
            ffi::EGL_LINUX_DRM_FOURCC_EXT,
            image.format as egl::Int,
            ffi::EGL_DMA_BUF_PLANE0_FD_EXT,
            image.fd,
            ffi::EGL_DMA_BUF_PLANE0_OFFSET_EXT,
            image.offset as egl::Int,
            ffi::EGL_DMA_BUF_PLANE0_PITCH_EXT,
            image.stride as egl::Int,
            ffi::EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT,
            (image.modifier & 0xffff_ffff) as egl::Int,
            ffi::EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT,
            (image.modifier >> 32) as egl::Int,
            EGL_NONE,
        ];
        let img = unsafe {
            (self.images.create)(
                self.display.as_ptr(),
                std::ptr::null_mut(),
                ffi::EGL_LINUX_DMA_BUF_EXT,
                std::ptr::null_mut(),
                attribs.as_ptr(),
            )
        };
        if img.is_null() {
            return Err(GpuError::Egl(format!(
                "eglCreateImageKHR(dma_buf) failed: {:?}",
                self.egl.get_error()
            )));
        }
        Ok(img)
    }

    /// Bind an EGLImage as the current `GL_TEXTURE_2D`.
    pub(crate) fn bind_image_texture(&self, image: *mut std::ffi::c_void) {
        unsafe { (self.images.target_texture)(glow::TEXTURE_2D, image) };
    }

    pub(crate) fn destroy_image(&self, image: *mut std::ffi::c_void) {
        unsafe { (self.images.destroy)(self.display.as_ptr(), image) };
    }
}

impl Drop for EglContext {
    fn drop(&mut self) {
        // Two exit-test knobs: `EXIT_KEEP_GL` leaves the whole context to process exit,
        // `EXIT_KEEP_EGL` only the display teardown, so a stall can be pinned to one call.
        if std::env::var_os("EXIT_KEEP_GL").is_some() {
            return;
        }
        // Only release the thread's binding when it is this context: a sibling window's
        // context may own the thread, and unbinding it here would break its next draw.
        if self.egl.get_current_context() == Some(self.context) {
            let _ = self.egl.make_current(self.display, None, None, None);
        }
        let _ = self.egl.destroy_context(self.display, self.context);
        if std::env::var_os("EXIT_KEEP_EGL").is_none() {
            let _ = self.egl.terminate(self.display);
        }
    }
}

/// The subset of a dma-buf description needed to import it.
impl EglContext {
    /// Turn a dma-buf into a renderable texture + framebuffer.
    ///
    /// The EGLImage must outlive the texture, so it is returned too; the caller destroys
    /// it after deleting the texture.
    #[allow(clippy::type_complexity)]
    pub fn create_dmabuf_texture(
        &self,
        desc: &DmaBufDesc,
    ) -> Result<(glow::Texture, glow::Framebuffer, *mut std::ffi::c_void), crate::GpuError> {
        let image = self.import_dmabuf(desc)?;
        let gl = self.gl();
        let texture = unsafe {
            let tex = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            self.bind_image_texture(image);
            let err = gl.get_error();
            if err != 0 {
                self.destroy_image(image);
                return Err(crate::GpuError::Gl(format!(
                    "glEGLImageTargetTexture2DOES: {err:#x}"
                )));
            }
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST as i32,
            );
            tex
        };
        let fbo = unsafe {
            let fbo = gl.create_framebuffer()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            if status != glow::FRAMEBUFFER_COMPLETE {
                return Err(crate::GpuError::Gl(format!("FBO incomplete: {status:#x}")));
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            fbo
        };
        Ok((texture, fbo, image))
    }
}

pub struct DmaBufDesc {
    pub fd: i32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub offset: u32,
    pub format: u32,
    pub modifier: u64,
}

fn required_message(name: &'static str) -> &'static str {
    name
}
