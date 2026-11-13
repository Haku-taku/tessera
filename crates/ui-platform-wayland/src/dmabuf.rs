//! dma-buf backed `wl_buffer` creation through `zwp_linux_dmabuf_v1`.

use std::collections::BTreeSet;
use std::os::fd::{AsFd, OwnedFd};

use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::{Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::{
    self, ZwpLinuxBufferParamsV1,
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1;

/// `DRM_FORMAT_ARGB8888`, matching the shm path's little-endian BGRA bytes.
pub const FORMAT_ARGB8888: u32 = 0x3432_5241;

/// `DRM_FORMAT_MOD_LINEAR`: stride-major rows, the fallback layout.
pub const MODIFIER_LINEAR: u64 = 0;

/// `DRM_FORMAT_MOD_INVALID`: let the compositor derive the layout from the dma-buf.
pub const MODIFIER_INVALID: u64 = 0x00ff_ffff_ffff_ffff;

/// One single-plane dma-buf image ready to become a `wl_buffer`.
#[derive(Debug)]
pub struct DmaBufImage {
    pub fd: OwnedFd,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub offset: u32,
    /// DRM fourcc, e.g. [`FORMAT_ARGB8888`].
    pub format: u32,
    /// Real layout modifier from the allocator; never assume [`MODIFIER_LINEAR`].
    pub modifier: u64,
}

/// How the compositor's advertisement covers a format/modifier pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormatSupport {
    /// The exact pair was advertised and may be sent verbatim.
    Explicit,
    /// Only implicit layout is offered: send [`MODIFIER_INVALID`].
    ImplicitOnly,
    /// The compositor will reject this pair.
    Unsupported,
}

/// Format/modifier pairs advertised by `zwp_linux_dmabuf_v1`.
#[derive(Clone, Debug, Default)]
pub struct DmaBufFormats {
    pairs: BTreeSet<(u32, u64)>,
    formats: BTreeSet<u32>,
}

impl DmaBufFormats {
    /// Record a legacy `format` event (v1/v2 carry no modifier).
    pub fn record_format(&mut self, format: u32) {
        self.formats.insert(format);
    }

    /// Record a `modifier` event (v3+ only).
    pub fn record_modifier(&mut self, format: u32, modifier: u64) {
        self.formats.insert(format);
        self.pairs.insert((format, modifier));
    }

    pub fn is_empty(&self) -> bool {
        self.formats.is_empty()
    }

    /// Number of distinct advertised format codes.
    pub fn format_count(&self) -> usize {
        self.formats.len()
    }

    /// Number of distinct advertised format/modifier pairs.
    pub fn pair_count(&self) -> usize {
        self.pairs.len()
    }

    /// Whether `format` appeared in any advertisement.
    pub fn has_format(&self, format: u32) -> bool {
        self.formats.contains(&format) || self.pairs.iter().any(|(f, _)| *f == format)
    }

    /// Classify a pair against the advertisement.
    pub fn support(&self, format: u32, modifier: u64) -> FormatSupport {
        if self.pairs.contains(&(format, modifier)) {
            return FormatSupport::Explicit;
        }
        if !self.has_format(format) {
            return FormatSupport::Unsupported;
        }
        // Implicit layout is valid when INVALID was advertised or only codes were.
        if modifier == MODIFIER_INVALID || self.pairs.contains(&(format, MODIFIER_INVALID)) {
            return FormatSupport::ImplicitOnly;
        }
        let explicit_only = self
            .pairs
            .iter()
            .any(|(f, m)| *f == format && *m != MODIFIER_INVALID);
        if explicit_only {
            FormatSupport::Unsupported
        } else {
            FormatSupport::ImplicitOnly
        }
    }

    /// Modifier to put on the wire, or `None` when the pair cannot work.
    pub fn effective_modifier(&self, format: u32, modifier: u64) -> Option<u64> {
        match self.support(format, modifier) {
            FormatSupport::Explicit => Some(modifier),
            FormatSupport::ImplicitOnly => Some(MODIFIER_INVALID),
            FormatSupport::Unsupported => None,
        }
    }
}

/// Why a dma-buf `wl_buffer` could not be created.
#[derive(Debug)]
pub enum DmaBufError {
    /// The compositor does not expose `zwp_linux_dmabuf_v1`.
    Unavailable,
    /// `create_immed` needs version 2; the bound global is older.
    TooOld(u32),
    /// The format/modifier pair was never advertised.
    NotAdvertised { format: u32, modifier: u64 },
    /// Zero width, height, stride or format.
    InvalidImage,
    /// Duplicating the dma-buf fd failed.
    Fd,
}

impl std::fmt::Display for DmaBufError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DmaBufError::Unavailable => write!(f, "zwp_linux_dmabuf_v1 is not available"),
            DmaBufError::TooOld(v) => write!(f, "zwp_linux_dmabuf_v1 v{v} lacks create_immed"),
            DmaBufError::NotAdvertised { format, modifier } => write!(
                f,
                "compositor never advertised format {format:#x} modifier {modifier:#x}"
            ),
            DmaBufError::InvalidImage => write!(f, "invalid dma-buf image geometry"),
            DmaBufError::Fd => write!(f, "could not duplicate the dma-buf fd"),
        }
    }
}

impl std::error::Error for DmaBufError {}

/// A `wl_buffer` plus the dma-buf fd that must outlive it.
#[derive(Debug)]
pub struct DmaBufBuffer {
    buffer: WlBuffer,
    /// The compositor may sample the dma-buf until it sends `release`.
    _keepalive: OwnedFd,
    /// Params stays alive so a late `failed` event still reaches dispatch.
    _params: ZwpLinuxBufferParamsV1,
}

impl DmaBufBuffer {
    pub fn buffer(&self) -> &WlBuffer {
        &self.buffer
    }

    /// Destroy the `wl_buffer` now; the compositor releases the dma-buf with it.
    pub fn destroy(&self) {
        self.buffer.destroy();
    }
}

/// Combine the protocol's split modifier back into one 64-bit token.
pub fn modifier_from_parts(hi: u32, lo: u32) -> u64 {
    ((hi as u64) << 32) | lo as u64
}

/// Split a 64-bit modifier into the protocol's `(modifier_hi, modifier_lo)`.
pub fn modifier_parts(modifier: u64) -> (u32, u32) {
    ((modifier >> 32) as u32, modifier as u32)
}

/// The `(device, inode)` a dma-buf descriptor points at: a dup shares both, so the pair
/// identifies "the same buffer" across frames and lets its `wl_buffer` be reused.
pub fn image_identity(image: &DmaBufImage) -> Option<(u64, u64)> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(format!("/proc/self/fd/{}", image.fd.as_raw_fd())).ok()?;
    Some((meta.dev(), meta.ino()))
}

/// Import `image` and create a `wl_buffer` through `create_immed`.
pub fn create_dmabuf_buffer<D>(
    dmabuf: &ZwpLinuxDmabufV1,
    formats: &DmaBufFormats,
    image: &DmaBufImage,
    qh: &QueueHandle<D>,
) -> Result<DmaBufBuffer, DmaBufError>
where
    D: Dispatch<ZwpLinuxBufferParamsV1, ()> + Dispatch<WlBuffer, ()> + 'static,
{
    let version = dmabuf.version();
    if version < 2 {
        return Err(DmaBufError::TooOld(version));
    }
    if image.width == 0 || image.height == 0 || image.stride == 0 || image.format == 0 {
        return Err(DmaBufError::InvalidImage);
    }
    let Some(modifier) = formats.effective_modifier(image.format, image.modifier) else {
        return Err(DmaBufError::NotAdvertised {
            format: image.format,
            modifier: image.modifier,
        });
    };
    let keepalive = image.fd.try_clone().map_err(|_| DmaBufError::Fd)?;
    let (hi, lo) = modifier_parts(modifier);

    let params = dmabuf.create_params(qh, ());
    params.add(image.fd.as_fd(), 0, image.offset, image.stride, hi, lo);
    // create_immed is synchronous; a rejected import arrives as `failed` later.
    let buffer = params.create_immed(
        image.width as i32,
        image.height as i32,
        image.format,
        zwp_linux_buffer_params_v1::Flags::empty(),
        qh,
        (),
    );
    Ok(DmaBufBuffer {
        buffer,
        _keepalive: keepalive,
        _params: params,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_parts_round_trip_preserves_nvidia_tiled_value() {
        let tiled = 0x0300_0000_00e0_8012u64;
        let (hi, lo) = modifier_parts(tiled);
        assert_eq!(hi, 0x0300_0000);
        assert_eq!(lo, 0x00e0_8012);
        assert_eq!(modifier_from_parts(hi, lo), tiled);
    }

    #[test]
    fn unlisted_modifier_is_unsupported_once_explicit_modifiers_are_advertised() {
        let mut formats = DmaBufFormats::default();
        formats.record_modifier(FORMAT_ARGB8888, MODIFIER_LINEAR);
        assert_eq!(
            formats.support(FORMAT_ARGB8888, 0x0300_0000_00e0_8012),
            FormatSupport::Unsupported
        );
    }

    #[test]
    fn implicit_fallback_sends_modifier_invalid() {
        let mut formats = DmaBufFormats::default();
        formats.record_modifier(FORMAT_ARGB8888, MODIFIER_INVALID);
        assert_eq!(
            formats.effective_modifier(FORMAT_ARGB8888, 0x0300_0000_00e0_8012),
            Some(MODIFIER_INVALID)
        );
    }
}
