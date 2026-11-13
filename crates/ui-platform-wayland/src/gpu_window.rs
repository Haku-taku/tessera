//! Zero-copy presenter: EGL context, double-buffered dma-buf targets, `present_dmabuf`.
//!
//! The Wayland surface and the input loop stay with the caller; this owns only the GPU side.

use std::cell::RefCell;
use std::io::Write;
use std::path::Path;
use std::rc::Rc;

use ui_core::colour::BufferFormat;
use ui_core::damage::Damage;
use ui_core::draw::DrawList;
use ui_core::timed;
use ui_gpu_egl::{
    DmaBufTarget as GpuTarget, EglContext, GpuError, SyncTimeline, output_render_node,
};
use ui_paint_gpu::{AtlasUpload, CanvasUpload, GlDevice, GpuRenderer};
use ui_platform::TextPass;

use crate::dmabuf::DmaBufImage;
use crate::window::{HdrSurface, WaylandWindow, WindowError, device_size};

/// How many dma-buf targets to cycle: the compositor holds one while we draw the other.
const TARGETS: usize = 2;
static PREV_FRAME: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);

thread_local! {
    /// One EGL context per render node. An EGL context, its GBM device and its display
    /// belong to the GPU, not to a window: creating them costs 57 ms (release, warm), which
    /// every pin canvas would otherwise pay again. The renderer is not shared with it: its
    /// destination and uploaded atlases follow one surface's colour.
    static CONTEXTS: RefCell<Vec<(String, Rc<EglContext>)>> = const { RefCell::new(Vec::new()) };
}

/// The context for `node`, created on first use; `cnode` is the same path as a C string.
fn context_for(node: &str, cnode: &std::ffi::CStr) -> Result<Rc<EglContext>, GpuError> {
    CONTEXTS.with(|contexts| {
        let mut contexts = contexts.borrow_mut();
        if let Some((_, context)) = contexts.iter().find(|(name, _)| name == node) {
            return Ok(Rc::clone(context));
        }
        let context = Rc::new(EglContext::new(cnode)?);
        contexts.push((node.to_string(), Rc::clone(&context)));
        Ok(context)
    })
}

/// How long a frame waits for the compositor's release point before drawing anyway.
const RELEASE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(50);

/// The destination one declared surface gives the renderer.
fn destination(surface: HdrSurface) -> ui_core::colour::Destination {
    ui_core::colour::Destination {
        domain: ui_core::colour::Domain::Linear {
            reference_nits: surface.reference_nits as f32,
            min_nits: surface.min_nits,
            peak_nits: surface.max_nits,
        },
        tone: ui_core::colour::ToneMapping::Bt2390,
    }
}

/// What [`GpuWindow::open`] needs to know.
#[derive(Clone, Debug)]
pub struct GpuWindowConfig {
    /// DRM render node; `None` asks the driver which GPU drives a connected output.
    pub render_node: Option<String>,
    /// Format of the presented buffers while the surface is SDR.
    pub format: BufferFormat,
    /// What the caller wants the surface's colour to follow.
    pub colour: ColourPolicy,
}

/// What a caller asks for; the surface's actual colour is resolved at open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColourPolicy {
    /// Take the output's own description: HDR where it is HDR, SDR otherwise.
    Output,
    /// Present SDR even on an HDR output, leaving the compositor to map it.
    Sdr,
}

/// What a surface actually presents, as resolved from its output.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SurfaceColour {
    Sdr,
    Hdr(HdrSurface),
}

impl SurfaceColour {
    /// The HDR encoding in force, `None` for an SDR surface.
    pub fn hdr(self) -> Option<HdrSurface> {
        match self {
            Self::Sdr => None,
            Self::Hdr(surface) => Some(surface),
        }
    }
}

impl Default for GpuWindowConfig {
    fn default() -> Self {
        Self {
            render_node: None,
            format: BufferFormat::Bgra8Srgb,
            colour: ColourPolicy::Output,
        }
    }
}

/// Why a [`GpuWindow`] could not open or present a frame.
#[derive(Debug)]
pub enum GpuWindowError {
    /// The compositor never advertised `zwp_linux_dmabuf_v1`: there is no zero-copy path.
    NoDmaBuf,
    /// The render node path is not a valid C string.
    InvalidNode(String),
    /// Nothing has been presented yet, so there is no frame to read back.
    NotPresented,
    /// EGL, the target pool, or a GL call failed.
    Gpu(GpuError),
    /// The scene painter could not allocate or draw.
    Draw(ui_paint_gpu::GpuError),
    /// The Wayland side refused the buffer or the commit.
    Window(WindowError),
    /// Writing a read-back image failed.
    Io(std::io::Error),
}

impl std::fmt::Display for GpuWindowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDmaBuf => write!(
                f,
                "compositor has no zwp_linux_dmabuf_v1; zero-copy presentation is unavailable"
            ),
            Self::InvalidNode(path) => write!(f, "render node {path:?} is not a valid path"),
            Self::NotPresented => write!(f, "no frame has been presented yet"),
            Self::Gpu(e) => write!(f, "GPU: {e}"),
            Self::Draw(e) => write!(f, "draw: {e}"),
            Self::Window(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "write failed: {e}"),
        }
    }
}

impl std::error::Error for GpuWindowError {}

impl From<GpuError> for GpuWindowError {
    fn from(e: GpuError) -> Self {
        Self::Gpu(e)
    }
}

impl From<ui_paint_gpu::GpuError> for GpuWindowError {
    fn from(e: ui_paint_gpu::GpuError) -> Self {
        Self::Draw(e)
    }
}

impl From<WindowError> for GpuWindowError {
    fn from(e: WindowError) -> Self {
        Self::Window(e)
    }
}

impl From<std::io::Error> for GpuWindowError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// What one [`GpuWindow::present`] call did, so the caller can log it.
#[derive(Clone, Copy, Debug)]
pub struct PresentOutcome {
    /// The new device size when the target pool was rebuilt for this frame.
    pub resized: Option<(u32, u32)>,
    /// Device pixels per logical unit used for this frame.
    pub scale: f32,
    /// GPU draw calls issued for this frame: the cost driver overhead tracks.
    pub draw_calls: u32,
    /// Device-space redraw band, `None` when the whole surface was redrawn.
    pub band: Option<[i32; 4]>,
}

/// A GPU presenter bound to one window.
pub struct GpuWindow {
    /// Shared with every other window on the same GPU.
    ctx: Rc<EglContext>,
    device: GlDevice,
    renderer: GpuRenderer,
    /// The DRM timelines carrying this window's acquire/release points.
    sync_timeline: Option<SyncTimeline>,
    /// The compositor signals this timeline once it has stopped reading a buffer.
    sync_release_timeline: Option<SyncTimeline>,
    /// Last acquire point handed to the compositor; points must strictly increase.
    acquire_point: u64,
    /// Release point each target was last presented with, so reuse can wait for it.
    release_points: [u64; TARGETS],
    /// Last release point handed out; points must strictly increase.
    release_point: u64,
    targets: Vec<GpuTarget>,
    /// Device size of the current pool; `(0, 0)` before the first present.
    size: (u32, u32),
    format: BufferFormat,
    /// Index of the target to render into next.
    next: usize,
    /// Index of the target handed to the compositor last.
    last: usize,
    /// Bands drawn since the oldest target in the pool was last written, newest last.
    recent_bands: Vec<[i32; 4]>,
    atlas_version: u64,
    color_atlas_version: u64,
    node: String,
    /// What this surface presents, re-resolved when the compositor says it changed.
    colour: SurfaceColour,
    /// The caller's wish, kept so a re-resolution applies the same policy.
    policy: ColourPolicy,
    /// The format to present while the surface is SDR.
    sdr_format: BufferFormat,
    /// Set by a colour change: the target pool must be allocated in the new format.
    rebuild_needed: bool,
}

impl GpuWindow {
    /// Create the EGL context and renderer for `win`, or say why it is impossible.
    pub fn open(win: &mut WaylandWindow, config: GpuWindowConfig) -> Result<Self, GpuWindowError> {
        if !win.has_dmabuf() {
            return Err(GpuWindowError::NoDmaBuf);
        }
        let node = config
            .render_node
            .or_else(output_render_node)
            .unwrap_or_else(|| "/dev/dri/renderD128".to_string());
        let cnode = std::ffi::CString::new(node.clone())
            .map_err(|_| GpuWindowError::InvalidNode(node.clone()))?;
        let ctx = context_for(&node, cnode.as_c_str())?;
        let mut device = GlDevice::new(ctx.gl_handle(), ctx.is_gles());
        let mut renderer = GpuRenderer::new(&mut device)?;
        // The surface takes its output's description, so its pixels mean what the display
        // expects; on an HDR output that also means encoding to PQ in a wide-enough buffer.
        let colour = match config.colour {
            ColourPolicy::Output => win
                .declare_hdr_surface()
                .map_or(SurfaceColour::Sdr, SurfaceColour::Hdr),
            ColourPolicy::Sdr => SurfaceColour::Sdr,
        };
        // This is the answer for now; the first present must not resolve it again.
        win.take_colour_change();
        let format = colour.hdr().map_or(config.format, |surface| surface.format);
        if let Some(surface) = colour.hdr() {
            renderer.set_destination(destination(surface));
            renderer.set_gamut(surface.gamut);
            eprintln!(
                "colour: HDR surface, reference {} nits, black {} nits, peak {} nits, {:?} buffers",
                surface.reference_nits, surface.min_nits, surface.max_nits, surface.format
            );
        }
        // Explicit synchronization: a DRM timeline whose fd the compositor imports, so each
        // frame carries an acquire point instead of relying on implicit fences.
        let (sync_timeline, sync_release_timeline) = match (
            SyncTimeline::new(ctx.drm_fd()),
            SyncTimeline::new(ctx.drm_fd()),
        ) {
            (Ok(acquire), Ok(release)) => match (acquire.dup_fd(), release.dup_fd()) {
                (Ok(afd), Ok(rfd)) => {
                    if win.attach_sync_timelines(afd, rfd) {
                        eprintln!("explicit sync: acquire/release timelines attached");
                        (Some(acquire), Some(release))
                    } else {
                        eprintln!("explicit sync unavailable: no wp_linux_drm_syncobj_manager_v1");
                        (None, None)
                    }
                }
                _ => (None, None),
            },
            (Err(e), _) | (_, Err(e)) => {
                eprintln!("explicit sync unavailable: {e}");
                (None, None)
            }
        };
        Ok(Self {
            ctx,
            device,
            renderer,
            sync_timeline,
            sync_release_timeline,
            acquire_point: 0,
            release_points: [0; TARGETS],
            release_point: 0,
            targets: Vec::new(),
            size: (0, 0),
            format,
            next: 0,
            last: 0,
            recent_bands: Vec::new(),
            atlas_version: u64::MAX,
            color_atlas_version: 0,
            node,
            colour,
            policy: config.colour,
            sdr_format: config.format,
            rebuild_needed: false,
        })
    }

    /// The GPU's own renderer string, for reporting which device is in use.
    pub fn renderer_name(&self) -> String {
        self.ctx.renderer()
    }

    /// The render node this presenter opened.
    pub fn node(&self) -> &str {
        &self.node
    }

    /// Wait until the GPU has executed every command submitted so far.
    ///
    /// A queue still full at exit drains while the compositor waits behind it, which is
    /// what a frozen screen after closing looks like.
    pub fn finish(&mut self) {
        if let Err(error) = self.ctx.make_current() {
            eprintln!("gpu: no current context to finish the queue: {error}");
            return;
        }
        self.renderer.finish(&mut self.device);
    }

    /// Format of the presented buffers.
    pub fn format(&self) -> BufferFormat {
        self.format
    }

    /// The surface's resolved colour: SDR, or the HDR description it declared.
    pub fn colour(&self) -> SurfaceColour {
        self.colour
    }

    /// DRM fourcc of the presented buffers.
    pub fn fourcc(&self) -> u32 {
        self.format.drm_fourcc()
    }

    /// The renderer, for pipeline choices such as HDR or internal precision.
    pub fn renderer_mut(&mut self) -> &mut GpuRenderer {
        &mut self.renderer
    }

    /// Upload the glyph atlases if the text pass changed them; returns whether it did.
    pub fn sync_atlas(&mut self, pass: &TextPass) -> Result<bool, GpuWindowError> {
        self.ctx.make_current()?;
        let mut uploaded = false;
        if pass.version() != self.atlas_version {
            self.atlas_version = pass.version();
            let (width, height) = pass.dimensions();
            self.renderer.set_atlas(
                &mut self.device,
                AtlasUpload {
                    width,
                    height,
                    data: pass.atlas().to_vec(),
                },
            )?;
            uploaded = true;
        }
        // The colour page is tracked separately: a new text glyph must not re-upload it.
        if pass.color_version() != self.color_atlas_version {
            self.color_atlas_version = pass.color_version();
            if !pass.color_atlas().is_empty() {
                let (width, height) = pass.color_dimensions();
                self.renderer.set_color_atlas(
                    &mut self.device,
                    AtlasUpload {
                        width,
                        height,
                        data: pass.color_atlas().to_vec(),
                    },
                )?;
                uploaded = true;
            }
        }
        Ok(uploaded)
    }

    /// Render `list` at `logical` size into the back target and hand it to the compositor.
    pub fn present(
        &mut self,
        win: &mut WaylandWindow,
        logical: (u32, u32),
        list: &DrawList,
        damage: &Damage,
        canvases: &[CanvasUpload<'_>],
    ) -> Result<PresentOutcome, GpuWindowError> {
        self.ctx.make_current()?;
        self.sync_colour(win);
        let scale = win.scale();
        let device = device_size(logical, scale);
        let resized = if device == self.size && !self.rebuild_needed {
            None
        } else {
            self.rebuild_needed = false;
            match self.rebuild(device) {
                Ok(()) => {}
                // An HDR-only format the driver or compositor refuses must not end the
                // session: go back to SDR and keep drawing.
                Err(error) if self.colour.hdr().is_some() => {
                    eprintln!("colour: {error}; falling back to SDR");
                    self.apply_colour(SurfaceColour::Sdr);
                    self.rebuild_needed = false;
                    self.rebuild(device)?;
                }
                Err(error) => return Err(error),
            }
            // The old targets' dma-bufs are gone; their cached buffers must not survive to
            // be matched by a reused inode.
            win.forget_dmabuf_cache();
            Some(device)
        };

        self.renderer.set_scale(scale);
        // A rebuilt pool holds no previous frame, so the whole surface has to be redrawn.
        let damage = if resized.is_some() {
            &Damage::Full
        } else {
            damage
        };
        // The compositor may still be scanning out this target's previous frame; writing
        // now would let it sample a half-drawn one. Its release point says when it is done.
        if let Some(timeline) = self.sync_release_timeline.as_ref() {
            let point = self.release_points[self.next];
            if point != 0 && !timeline.wait(point, RELEASE_TIMEOUT) {
                eprintln!("present: release point {point} timed out; drawing anyway");
            }
        }
        let target = &self.targets[self.next];
        // A target only holds the bands drawn on the frames it was handed, so it also needs
        // everything drawn since its last turn. Without that the compositor shows an old
        // frame for one buffer's worth of time — the residue and flicker this pool caused.
        let stats = timed!(
            "gpu_draw",
            self.renderer.render_frame_with_canvases(
                &mut self.device,
                target,
                list,
                damage,
                &self.recent_bands,
                canvases,
            )
        )?;
        let full = [0, 0, self.size.0 as i32, self.size.1 as i32];
        self.recent_bands.push(stats.band.unwrap_or(full));
        while self.recent_bands.len() >= TARGETS {
            self.recent_bands.remove(0);
        }
        // Explicit sync exports a fence the compositor waits on; without it, block here so
        // the compositor never samples writes that have not landed yet.
        let acquire_point = timed!(
            "gpu_fence",
            match self.sync_timeline.as_ref() {
                Some(timeline) => {
                    let fence = self.ctx.export_fence()?;
                    self.acquire_point += 1;
                    let point = self.acquire_point;
                    timeline.import_fence_at(&fence.fd, point)?;
                    Some(point)
                }
                None => {
                    // Without explicit sync the compositor cannot tell when these writes land, so
                    // wait here. Measured: this blocks (200–300µs) but burns no CPU.
                    self.renderer.finish(&mut self.device);
                    None
                }
            }
        );

        let desc = target.desc();
        let image = DmaBufImage {
            fd: target.dup_fd()?,
            width: desc.width,
            height: desc.height,
            stride: desc.stride,
            offset: desc.offset,
            format: self.fourcc(),
            modifier: desc.modifier,
        };
        // The release point rides this commit: the compositor signals it once done reading.
        self.release_point += 1;
        let release_point = self.release_point;
        self.release_points[self.next] = release_point;
        // Report what this frame actually redrew. Everything outside the band is already the
        // same in the handed-over buffer, so the compositor may keep its copy there.
        let reported = match stats.band {
            Some([x, y, w, h]) => {
                let s = scale.max(f32::EPSILON);
                Damage::Rects(vec![ui_core::geom::Rect::new(
                    x as f32 / s,
                    y as f32 / s,
                    w as f32 / s,
                    h as f32 / s,
                )])
            }
            None => Damage::Full,
        };
        // Probe: a handed-over buffer promises the whole surface, so it must match the frame.
        if std::env::var_os("CRABSNAP_TARGET_CHECK").is_some() {
            if let Some((n, bbox)) = self.renderer.diff_frame(&mut self.device, target) {
                eprintln!(
                    "[target-check] differing={n} bbox={bbox:?} band={:?} reported={reported:?}",
                    stats.band
                );
            }
            // A change outside the band would never reach the compositor: it is not reported.
            if let Some((w, h, px)) = self.renderer.frame_pixels(&mut self.device) {
                let mut prev = PREV_FRAME.lock().unwrap();
                if let Some(p) = prev.as_ref().filter(|p| p.len() == px.len()) {
                    let band = stats.band.unwrap_or([0, 0, w as i32, h as i32]);
                    let (mut c, mut x0, mut y0, mut x1, mut y1) =
                        (0u32, u32::MAX, u32::MAX, 0u32, 0u32);
                    for (i, (a, b)) in px
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .zip(p.as_chunks::<4>().0)
                        .enumerate()
                    {
                        if a != b {
                            let (x, y) = (i as u32 % w, i as u32 / w);
                            let outside = (x as i32) < band[0]
                                || (y as i32) < band[1]
                                || (x as i32) >= band[0] + band[2]
                                || (y as i32) >= band[1] + band[3];
                            if outside {
                                c += 1;
                                x0 = x0.min(x);
                                y0 = y0.min(y);
                                x1 = x1.max(x);
                                y1 = y1.max(y);
                            }
                        }
                    }
                    if c > 0 {
                        eprintln!(
                            "[frame-outside] {c} px at [{x0},{y0},{x1},{y1}] band={:?}",
                            stats.band
                        );
                    }
                }
                *prev = Some(px);
            }
        }
        timed!(
            "commit",
            win.present_dmabuf(&image, &reported, acquire_point, release_point)
        )?;

        self.last = self.next;
        self.next = (self.next + 1) % self.targets.len();
        Ok(PresentOutcome {
            resized,
            scale,
            draw_calls: stats.draw_calls,
            band: stats.band,
        })
    }

    /// Write the interface and its layer 0 as PPM files.
    ///
    /// The picture is captured before the present pass, so an HDR session writes the same
    /// SDR image an SDR session does rather than the PQ codes its buffer holds.
    pub fn dump_ppm(
        &mut self,
        frame: &Path,
        layer: &Path,
    ) -> Result<Option<(u32, u32)>, GpuWindowError> {
        self.ctx.make_current()?;
        let (w, h, pixels) = self
            .renderer
            .capture(&mut self.device)
            .ok_or(GpuWindowError::NotPresented)?;
        write_ppm(frame, w, h, &pixels, |p| [p[0], p[1], p[2]])?;
        // Alpha as grey, so transparency is visible instead of guessed.
        // Layer 0 is absent when the frame painted no group; that is not an error.
        match self.renderer.read_layer(&mut self.device, 0) {
            Some((w, h, buf)) => {
                write_ppm(layer, w, h, &buf, |p| [p[3], p[3], p[3]])?;
                Ok(Some((w, h)))
            }
            None => Ok(None),
        }
    }

    /// Follow a change of the surface's colour, if the compositor reported one.
    ///
    /// Returns whether the target pool has to be rebuilt, which happens when the buffer
    /// format changed.
    fn sync_colour(&mut self, win: &mut WaylandWindow) -> bool {
        if !win.take_colour_change() {
            return false;
        }
        let colour = match self.policy {
            ColourPolicy::Output => win
                .refresh_surface_colour(true)
                .map_or(SurfaceColour::Sdr, SurfaceColour::Hdr),
            ColourPolicy::Sdr => {
                win.refresh_surface_colour(false);
                SurfaceColour::Sdr
            }
        };
        if colour == self.colour {
            return false;
        }
        eprintln!("colour: surface is now {colour:?}");
        self.apply_colour(colour);
        true
    }

    /// Put the renderer and the presented format on `colour`.
    fn apply_colour(&mut self, colour: SurfaceColour) {
        self.colour = colour;
        self.format = colour
            .hdr()
            .map_or(self.sdr_format, |surface| surface.format);
        match colour.hdr() {
            Some(surface) => {
                self.renderer.set_destination(destination(surface));
                self.renderer.set_gamut(surface.gamut);
            }
            None => {
                self.renderer
                    .set_destination(ui_core::colour::Destination::SDR);
                self.renderer
                    .set_gamut([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
            }
        }
        // Atlases and image textures carry the domain in their channels; forcing their
        // versions makes the next frame upload them again.
        self.atlas_version = u64::MAX;
        self.color_atlas_version = u64::MAX;
        self.rebuild_needed = true;
    }

    /// Allocate a fresh pool of `device`-sized targets; the old one is released only on success.
    fn rebuild(&mut self, device: (u32, u32)) -> Result<(), GpuWindowError> {
        let fourcc = self.fourcc();
        let mut fresh = Vec::with_capacity(TARGETS);
        for _ in 0..TARGETS {
            match GpuTarget::new_with_fourcc(&self.ctx, device.0, device.1, fourcc) {
                Ok(t) => fresh.push(t),
                Err(e) => {
                    for t in &fresh {
                        t.release(&self.ctx);
                    }
                    return Err(GpuWindowError::Gpu(e));
                }
            }
        }
        self.release_targets();
        self.targets = fresh;
        self.size = device;
        self.next = 0;
        self.last = 0;
        // Fresh dma-bufs carry no earlier point; the old points name buffers that are gone.
        self.release_points = [0; TARGETS];
        // Fresh targets hold nothing, and the full frame they get next covers them.
        self.recent_bands.clear();
        Ok(())
    }

    fn release_targets(&mut self) {
        for t in &self.targets {
            t.release(&self.ctx);
        }
        self.targets.clear();
    }
}

impl Drop for GpuWindow {
    /// Release GL objects while the EGL context is still alive: fields drop after this body.
    fn drop(&mut self) {
        // EGL current is thread state, and another window's context may own the thread
        // right now. An id only names an object inside the context that created it, so
        // deleting here under that other context deletes *its* objects with the same ids.
        if let Err(error) = self.ctx.make_current() {
            eprintln!("gpu: teardown without a current context: {error}");
            self.device.forget();
            return;
        }
        ui_platform::exit_probe::stage("gpu: targets");
        self.release_targets();
        ui_platform::exit_probe::stage("gpu: renderer");
        self.device.release();
        // EGL teardown is the driver's: `eglTerminate` waits for its worker threads.
        ui_platform::exit_probe::stage("gpu: egl");
    }
}

/// Write top-down RGBA8 pixels as binary PPM, taking three channels per pixel.
fn write_ppm(
    path: &Path,
    width: u32,
    height: u32,
    rgba: &[u8],
    pick: impl Fn(&[u8]) -> [u8; 3],
) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    write!(file, "P6\n{width} {height}\n255\n")?;
    for p in rgba.as_chunks::<4>().0 {
        file.write_all(&pick(p))?;
    }
    Ok(())
}
