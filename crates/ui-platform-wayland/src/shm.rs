//! Shared-memory buffer pool for `wl_shm` presentation.

use std::os::unix::io::AsFd;

use memmap2::MmapMut;
use wayland_client::protocol::wl_shm::{Format, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::{Connection, QueueHandle};

/// One mapped buffer plus the pool that owns it.
pub struct ShmBuffer {
    pub pool: WlShmPool,
    pub map: MmapMut,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    /// Set by the compositor's `release` event.
    busy: std::cell::Cell<bool>,
}

impl std::fmt::Debug for ShmBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShmBuffer")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

impl ShmBuffer {
    /// Allocate a zeroed buffer of `width * height * 4` bytes.
    pub fn new(
        shm: &WlShm,
        qh: &QueueHandle<impl wayland_client::Dispatch<WlShmPool, ()> + 'static>,
        width: u32,
        height: u32,
    ) -> Option<Self> {
        Self::new_with_stride(shm, qh, width, height, width.checked_mul(4)?)
    }

    /// Allocate a buffer whose rows are `stride` bytes apart.
    ///
    /// A capture protocol states the stride it expects in the buffer, so the rows it writes
    /// may be padded; presentation always uses tight rows and goes through [`new`](Self::new).
    pub fn new_with_stride(
        shm: &WlShm,
        qh: &QueueHandle<impl wayland_client::Dispatch<WlShmPool, ()> + 'static>,
        width: u32,
        height: u32,
        stride: u32,
    ) -> Option<Self> {
        if stride < width.checked_mul(4)? {
            return None;
        }
        let size = (stride as usize).checked_mul(height as usize)?;
        if size == 0 {
            return None;
        }
        let file = tempfile_in_memory(size)?;
        let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
        let map = unsafe { MmapMut::map_mut(&file).ok()? };
        Some(Self {
            pool,
            map,
            width,
            height,
            stride,
            busy: std::cell::Cell::new(false),
        })
    }

    pub fn set_busy(&self, busy: bool) {
        self.busy.set(busy);
    }

    pub fn is_busy(&self) -> bool {
        self.busy.get()
    }

    /// Bytes for `wl_buffer` to interpret, as ARGB8888 (little-endian BGRA).
    pub fn bytes(&self) -> &[u8] {
        &self.map
    }

    /// Copy premultiplied BGRA rows from `src` into the mapping.
    pub fn copy_from(&mut self, src: &[u8]) {
        let n = self.map.len().min(src.len());
        self.map[..n].copy_from_slice(&src[..n]);
    }

    pub fn format() -> Format {
        // wl_shm ARGB8888 is little-endian BGRA in memory, matching `ui_paint`.
        Format::Argb8888
    }
}

/// An anonymous in-memory file suitable for `wl_shm`.
fn tempfile_in_memory(size: usize) -> Option<std::fs::File> {
    use std::io::{Seek, SeekFrom, Write};
    let mut f = tempfile::tempfile().ok()?;
    f.set_len(size as u64).ok()?;
    f.seek(SeekFrom::Start(0)).ok()?;
    // Touch the last byte so the file is fully backed before mapping.
    f.write_all(&[0u8]).ok()?;
    f.seek(SeekFrom::Start(0)).ok()?;
    Some(f)
}

/// Keep the connection type reachable for callers that build queues.
pub type ShmConnection = Connection;
