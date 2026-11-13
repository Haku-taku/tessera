//! Colour management: transfer functions, colour spaces, output description.
//!
//! Storage and working domain are separate concerns. SDR stays in 8-bit sRGB and
//! linearises through a LUT; only HDR needs a wide domain.

/// How encoded values map to linear light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransferFunction {
    /// sRGB piecewise curve, the default for SDR.
    Srgb,
    /// No curve; the buffer already holds linear light.
    Linear,
    /// SMPTE ST 2084, absolute: 1.0 means 10000 nits.
    Pq,
    /// BT.2100 HLG, scene-referred.
    Hlg,
    /// A plain power curve, which is what a compositor describes with an exponent.
    Gamma(f32),
}

impl TransferFunction {
    /// Encoded value in `[0, 1]` to linear light in `[0, 1]`.
    pub fn decode(self, encoded: f32) -> f32 {
        let e = encoded.clamp(0.0, 1.0);
        match self {
            Self::Srgb => {
                if e <= 0.040_449_237 {
                    e / 12.92
                } else {
                    ((e + 0.055) / 1.055).powf(2.4)
                }
            }
            Self::Linear => e,
            Self::Pq => {
                const M1: f32 = 2610.0 / 16384.0;
                const M2: f32 = 2523.0 / 32.0;
                const C1: f32 = 3424.0 / 4096.0;
                const C2: f32 = 2413.0 / 128.0;
                const C3: f32 = 2392.0 / 128.0;
                let p = e.powf(1.0 / M2);
                let num = (p - C1).max(0.0);
                let den = C2 - C3 * p;
                if den <= 0.0 {
                    return 1.0;
                }
                (num / den).powf(1.0 / M1)
            }
            Self::Hlg => {
                let a: f32 = 0.178_832_77;
                let b = 1.0 - 4.0 * a;
                let c = 0.5 - a * (4.0 * a).ln();
                if e <= 0.5 {
                    e * e / 3.0
                } else {
                    (((e - c) / a).exp() + b) / 12.0
                }
            }
            Self::Gamma(gamma) => e.powf(gamma),
        }
    }

    /// Linear light in `[0, 1]` to an encoded value in `[0, 1]`.
    pub fn encode(self, linear: f32) -> f32 {
        let l = linear.clamp(0.0, 1.0);
        match self {
            Self::Srgb => {
                if l <= 0.003_130_8 {
                    l * 12.92
                } else {
                    1.055 * l.powf(1.0 / 2.4) - 0.055
                }
            }
            Self::Linear => l,
            Self::Pq => {
                const M1: f32 = 2610.0 / 16384.0;
                const M2: f32 = 2523.0 / 32.0;
                const C1: f32 = 3424.0 / 4096.0;
                const C2: f32 = 2413.0 / 128.0;
                const C3: f32 = 2392.0 / 128.0;
                let p = l.powf(M1);
                ((C1 + C2 * p) / (1.0 + C3 * p)).powf(M2)
            }
            Self::Hlg => {
                let a: f32 = 0.178_832_77;
                let b = 1.0 - 4.0 * a;
                let c = 0.5 - a * (4.0 * a).ln();
                if l <= 1.0 / 12.0 {
                    (3.0 * l).sqrt()
                } else {
                    a * (12.0 * l - b).ln() + c
                }
            }
            Self::Gamma(gamma) => l.powf(1.0 / gamma),
        }
    }

    /// Whether this function can express more than SDR's ~100 nits.
    pub fn is_hdr(self) -> bool {
        matches!(self, Self::Pq | Self::Hlg)
    }
}

/// What a buffer's bytes mean, as opposed to how they are laid out.
///
/// Premultiplication is not inferable from bytes, so it lives in the type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Encoding {
    pub transfer: TransferFunction,
    pub premultiplied: bool,
}

impl Default for Encoding {
    /// SDR premultiplied: the only thing this crate produced before HDR existed.
    fn default() -> Self {
        Self::SDR_PREMULTIPLIED
    }
}

impl Encoding {
    /// What every SDR path in this crate produces today.
    pub const SDR_PREMULTIPLIED: Self = Self {
        transfer: TransferFunction::Srgb,
        premultiplied: true,
    };

    /// The working domain an HDR pipeline blends in.
    pub const LINEAR_PREMULTIPLIED: Self = Self {
        transfer: TransferFunction::Linear,
        premultiplied: true,
    };

    /// Whether blending in this encoding is physically meaningful.
    pub fn is_linear(&self) -> bool {
        matches!(self.transfer, TransferFunction::Linear)
    }
}

/// Map linear light to the code a PQ output expects.
///
/// `reference_nits` is the display's SDR white and `min_nits` its black level; both come
/// from the compositor, never from a constant. The shader transcribes this function.
/// Scale that puts the display's SDR white at the right place in PQ's range.
pub fn reference_scale(reference_nits: f32) -> f32 {
    reference_nits / 10_000.0
}

/// Offset contributed by the display's black level.
pub fn min_offset(min_nits: f32) -> f32 {
    min_nits / 10_000.0
}

pub fn encode_for_pq_output(linear: f32, reference_nits: f32, min_nits: f32) -> f32 {
    let scale = reference_scale(reference_nits);
    let offset = min_offset(min_nits);
    TransferFunction::Pq.encode((scale * linear + offset).clamp(0.0, 1.0))
}

/// Chromaticity coordinates of a set of primaries, as the protocol reports them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chromaticities {
    pub red: (f32, f32),
    pub green: (f32, f32),
    pub blue: (f32, f32),
    pub white: (f32, f32),
}

impl Chromaticities {
    /// BT.709 / sRGB, the space our own content is authored in.
    pub const SRGB: Self = Self {
        red: (0.64, 0.33),
        green: (0.30, 0.60),
        blue: (0.15, 0.06),
        white: (0.3127, 0.3290),
    };
    /// BT.2020, what an HDR output usually describes.
    pub const BT2020: Self = Self {
        red: (0.708, 0.292),
        green: (0.170, 0.797),
        blue: (0.131, 0.046),
        white: (0.3127, 0.3290),
    };
    /// Display P3.
    pub const DISPLAY_P3: Self = Self {
        red: (0.680, 0.320),
        green: (0.265, 0.690),
        blue: (0.150, 0.060),
        white: (0.3127, 0.3290),
    };

    fn xyz_of(self) -> [[f32; 3]; 3] {
        let col = |(x, y): (f32, f32)| [x / y, 1.0, (1.0 - x - y) / y];
        let r = col(self.red);
        let g = col(self.green);
        let b = col(self.blue);
        [[r[0], g[0], b[0]], [r[1], g[1], b[1]], [r[2], g[2], b[2]]]
    }

    /// Matrix taking linear RGB in this space to CIE XYZ.
    fn to_xyz(self) -> [[f32; 3]; 3] {
        let m = self.xyz_of();
        let (x, y) = self.white;
        let w = [x / y, 1.0, (1.0 - x - y) / y];
        let inv = invert(m).expect("primaries matrix is invertible");
        let s = mul_vec(inv, w);
        [
            [m[0][0] * s[0], m[0][1] * s[1], m[0][2] * s[2]],
            [m[1][0] * s[0], m[1][1] * s[1], m[1][2] * s[2]],
            [m[2][0] * s[0], m[2][1] * s[1], m[2][2] * s[2]],
        ]
    }
}

/// How the white points are reconciled when converting between gamuts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhiteAdaptation {
    /// Convert primaries only; a differing white point leaves a neutral cast.
    None,
    /// Bradford cone response; neutrals stay neutral across white points.
    Bradford,
}

/// Matrix converting linear RGB from `from` into `to`, both row-major.
pub fn gamut_matrix(from: Chromaticities, to: Chromaticities) -> [[f32; 3]; 3] {
    gamut_matrix_adapted(from, to, WhiteAdaptation::None)
}

/// The same, with an explicit white point strategy.
pub fn gamut_matrix_adapted(
    from: Chromaticities,
    to: Chromaticities,
    adaptation: WhiteAdaptation,
) -> [[f32; 3]; 3] {
    let inv = invert(to.to_xyz()).expect("primaries matrix is invertible");
    let src = from.to_xyz();
    match adaptation {
        WhiteAdaptation::None => mul(inv, src),
        WhiteAdaptation::Bradford => {
            // Adapt the source white to the destination white in cone space.
            let b = [
                [0.8951, 0.2664, -0.1614],
                [-0.7502, 1.7135, 0.0367],
                [0.0389, -0.0685, 1.0296],
            ];
            let binv = invert(b).expect("bradford matrix is invertible");
            let ws = mul_vec(b, white_xyz(from.white));
            let wd = mul_vec(b, white_xyz(to.white));
            let mut d = [[0.0f32; 3]; 3];
            for i in 0..3 {
                d[i][i] = wd[i] / ws[i];
            }
            let adapt = mul(binv, mul(d, b));
            mul(inv, mul(adapt, src))
        }
    }
}

fn white_xyz((x, y): (f32, f32)) -> [f32; 3] {
    [x / y, 1.0, (1.0 - x - y) / y]
}

fn mul_vec(m: [[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0f32; 3];
    for (i, row) in m.iter().enumerate() {
        out[i] = row[0] * v[0] + row[1] * v[1] + row[2] * v[2];
    }
    out
}

fn mul(a: [[f32; 3]; 3], b: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut out = [[0.0f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

fn invert(m: [[f32; 3]; 3]) -> Option<[[f32; 3]; 3]> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    Some([
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv,
        ],
    ])
}

/// The luminance BT.2408 puts a reference SDR white at.
///
/// An SDR destination has no description of its own, so this is the white content is placed
/// against: sRGB 1.0 and HDR content's 203 nits both land here.
pub const SDR_WHITE_NITS: f32 = 203.0;

/// How a picture's texels are packed.
///
/// The packing is not inferable from the bytes: the same four bytes are one sRGB texel or
/// one 10-bit word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelStorage {
    /// RGBA8, straight alpha: three 8-bit codes, then 8 bits of alpha.
    Sdr8,
    /// One little-endian 32-bit word: red in bits 0–9, green 10–19, blue 20–29, alpha 30–31.
    Packed10,
    /// Four little-endian binary16 channels, straight alpha. scRGB: linear light whose 1.0
    /// is [`SCRGB_WHITE_NITS`], which is the surface a desktop compositing in floats hands
    /// over, and the only packing that can carry light above SDR white.
    Float16,
}

impl PixelStorage {
    /// Bytes one texel occupies.
    pub fn bytes_per_texel(self) -> usize {
        match self {
            Self::Sdr8 | Self::Packed10 => 4,
            Self::Float16 => 8,
        }
    }

    /// Whether a texel is integer codes, which a whole-buffer transfer table can index.
    pub fn is_integer(self) -> bool {
        !matches!(self, Self::Float16)
    }

    /// The integer codes one texel holds, and its alpha as an 8-bit fraction.
    ///
    /// Codes rather than fractions, because a whole-frame conversion tabulates the transfer
    /// function over exactly these values. A float packing has no codes.
    fn codes(self, bytes: &[u8]) -> Option<([u16; 3], u8)> {
        let byte = |index: usize| bytes.get(index).copied().unwrap_or(0) as u16;
        Some(match self {
            Self::Sdr8 => ([byte(0), byte(1), byte(2)], byte(3) as u8),
            Self::Packed10 => {
                // Every word in a frame is four bytes; a short slice is a caller bug the
                // zero-filled tail turns into black rather than a panic.
                let mut word = [0u8; 4];
                for (slot, byte) in word.iter_mut().zip(bytes) {
                    *slot = *byte;
                }
                let word = u32::from_le_bytes(word);
                let code = |shift: u32| ((word >> shift) & 0x3ff) as u16;
                // Two bits of alpha: 3 means opaque, which is every code scaled to 255.
                let alpha = (((word >> 30) & 0x3) * 85) as u8;
                ([code(0), code(10), code(20)], alpha)
            }
            Self::Float16 => return None,
        })
    }

    /// The largest code an integer packing can hold.
    fn levels(self) -> u16 {
        match self {
            Self::Sdr8 => 255,
            Self::Packed10 => 1023,
            Self::Float16 => 1,
        }
    }

    /// One texel in the units its transfer function decodes, and its alpha in `[0, 1]`.
    ///
    /// An integer packing answers codes scaled into `[0, 1]`; a float packing answers the
    /// values themselves, which may exceed 1, because that is what carrying light above the
    /// reference white means.
    #[inline]
    pub fn unpack(self, bytes: &[u8]) -> ([f32; 3], f32) {
        if let Some((codes, alpha)) = self.codes(bytes) {
            let levels = f32::from(self.levels());
            return (
                [
                    f32::from(codes[0]) / levels,
                    f32::from(codes[1]) / levels,
                    f32::from(codes[2]) / levels,
                ],
                f32::from(alpha) / 255.0,
            );
        }
        let channel = |at: usize| {
            let low = bytes.get(at).copied().unwrap_or(0);
            let high = bytes.get(at + 1).copied().unwrap_or(0);
            half_to_f32(u16::from_le_bytes([low, high]))
        };
        (
            [channel(0), channel(2), channel(4)],
            channel(6).clamp(0.0, 1.0),
        )
    }
}

/// The luminance scRGB's 1.0 stands for: the format's own unit, not the display's white.
pub const SCRGB_WHITE_NITS: f32 = 80.0;

/// A binary16 value as a float.
pub fn half_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = (bits >> 10) & 0x1f;
    let mantissa = (bits & 0x3ff) as f32;
    match exponent {
        // Subnormals scale from 2^-14 with no implicit leading bit.
        0 => sign * mantissa * (1.0 / 16_777_216.0),
        0x1f if mantissa == 0.0 => sign * f32::INFINITY,
        0x1f => f32::NAN,
        _ => sign * (1.0 + mantissa / 1024.0) * f32::exp2(exponent as f32 - 15.0),
    }
}

/// What a picture's pixels mean: how they are packed and what they were authored against.
///
/// This travels with the pixels and is never rewritten for a display: a conversion is a
/// texture for one destination, and the original stays what it was.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentColour {
    pub storage: PixelStorage,
    pub primaries: Chromaticities,
    pub transfer: TransferFunction,
    /// Peak luminance the content can carry, in nits; zero when nobody stated one.
    pub max_nits: f32,
}

impl ContentColour {
    /// What the engine's own content is authored as: sRGB codes, no luminance claim.
    pub const SDR: Self = Self {
        storage: PixelStorage::Sdr8,
        primaries: Chromaticities::SRGB,
        transfer: TransferFunction::Srgb,
        max_nits: 0.0,
    };

    /// A desktop's float surface: scRGB light up to `peak_nits`, nothing converted.
    ///
    /// This is what a compositor hands over when it composites in floats — the Windows
    /// duplication and capture-session surfaces — and it is absolute light in a fixed unit,
    /// so it survives being pinned and drawn on either kind of surface.
    pub fn scrgb(peak_nits: f32) -> Self {
        Self {
            storage: PixelStorage::Float16,
            primaries: Chromaticities::SRGB,
            transfer: TransferFunction::Linear,
            max_nits: peak_nits,
        }
    }

    /// The transform that draws this picture on `destination`, derived once per texture.
    pub fn placement(self, destination: Destination) -> Placement {
        let encoded = matches!(destination.domain, Domain::Encoded);
        let reference_nits = match destination.domain {
            Domain::Encoded => SDR_WHITE_NITS,
            Domain::Linear { reference_nits, .. } => reference_nits,
        };
        let peak_nits = match destination.domain {
            Domain::Encoded => reference_nits,
            Domain::Linear { peak_nits, .. } => peak_nits,
        };
        // PQ decodes to a fraction of 10000 nits, an absolute luminance; sRGB and gamma
        // codes mean "this display's white", and the destination is the same desktop, so
        // 1.0 already lands where it belongs. The destination's own white is the anchor
        // either way, which is why content carries no white of its own. HLG is scene
        // referred and needs an OOTF this engine has no peak for; nothing produces it yet,
        // so it is placed as relative light.
        // scRGB is absolute light in a fixed unit, so it needs the same treatment as PQ: its
        // 1.0 is 80 nits whatever the destination's white is.
        let scrgb = self.storage == PixelStorage::Float16;
        let absolute = scrgb || matches!(self.transfer, TransferFunction::Pq);
        let gain = if scrgb {
            SCRGB_WHITE_NITS / reference_nits.max(1.0)
        } else if absolute {
            10_000.0 / reference_nits.max(1.0)
        } else {
            1.0
        };
        let identity = encoded
            && self.storage == PixelStorage::Sdr8
            && self.transfer == TransferFunction::Srgb
            && self.primaries == Chromaticities::SRGB;
        let tone = match destination.tone {
            // Clip needs no curve: the final clamp at the destination peak is the same thing.
            ToneMapping::Clip => None,
            ToneMapping::Bt2390 if absolute && self.max_nits > peak_nits => {
                Some((ToneMapping::Bt2390, self.max_nits, peak_nits))
            }
            ToneMapping::Bt2390 => None,
        };
        let matrix = if identity || self.primaries == Chromaticities::SRGB {
            IDENTITY_MATRIX
        } else {
            gamut_matrix_adapted(
                self.primaries,
                Chromaticities::SRGB,
                WhiteAdaptation::Bradford,
            )
        };
        Placement {
            storage: self.storage,
            transfer: self.transfer,
            matrix,
            gain,
            reference_nits,
            tone,
            identity,
            encoded,
            decode: None,
            encode: None,
        }
    }
}

/// Where content is being drawn, as opposed to what it holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Destination {
    pub domain: Domain,
    pub tone: ToneMapping,
}

impl Destination {
    /// An SDR surface: 8-bit sRGB, blended encoded.
    pub const SDR: Self = Self {
        domain: Domain::Encoded,
        tone: ToneMapping::Bt2390,
    };
}

/// The domain a destination blends in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Domain {
    /// 8-bit sRGB codes, blended in the encoded domain.
    Encoded,
    /// Linear light whose 1.0 is the surface's reference white.
    Linear {
        reference_nits: f32,
        /// Black level, in nits; the output pass places it in PQ.
        min_nits: f32,
        peak_nits: f32,
    },
}

/// What brings content above the destination's peak into its range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToneMapping {
    /// Hard clip; pixel-exact and the control every other curve is compared against.
    Clip,
    /// ITU-R BT.2390's EETF: 1:1 below the knee, a Hermite roll-off above it.
    Bt2390,
}

impl ToneMapping {
    /// One luminance in absolute nits, mapped into `[0, target_peak]`.
    pub fn apply(self, nits: f32, source_peak: f32, target_peak: f32) -> f32 {
        match self {
            Self::Clip => nits.clamp(0.0, target_peak),
            Self::Bt2390 => bt2390(nits, source_peak, target_peak),
        }
    }
}

/// The BT.2390 EETF, evaluated in PQ code space where the standard defines it.
///
/// Everything is normalised to the source peak, so the destination peak lands at
/// `target/source` and the knee at `ks`; below the knee the value passes through, above it a
/// cubic Hermite runs from the knee to the destination peak with the same slope at the knee
/// (so the curve does not kink) and zero slope at the top (so highlights do not plateau).
fn bt2390(nits: f32, source_peak: f32, target_peak: f32) -> f32 {
    let code = |nits: f32| TransferFunction::Pq.encode(nits / 10_000.0);
    let (source, target) = (code(source_peak), code(target_peak));
    if source <= 0.0 || target >= source {
        return nits.clamp(0.0, target_peak);
    }
    let ratio = target / source;
    // ITU-R BT.2390-9 §5.4.3 states ks = 1.5 * Ld - 0.5 in the source-normalised scale;
    // libplacebo makes the 1.5 a parameter and defaults it to 2.0, a harder knee.
    let ks = (1.5 * ratio - 0.5).max(0.0);
    let e = (code(nits) / source).clamp(0.0, 1.0);
    let mapped = if e <= ks {
        e
    } else {
        let t = (e - ks) / (1.0 - ks);
        let (t2, t3) = (t * t, t * t * t);
        (2.0 * t3 - 3.0 * t2 + 1.0) * ks
            + (t3 - 2.0 * t2 + t) * (1.0 - ks)
            + (-2.0 * t3 + 3.0 * t2) * ratio
    };
    TransferFunction::Pq.decode((mapped * source).clamp(0.0, 1.0)) * 10_000.0
}

const IDENTITY_MATRIX: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// One picture's transform onto one destination, derived once and then applied per texel.
#[derive(Clone, Debug)]
pub struct Placement {
    storage: PixelStorage,
    transfer: TransferFunction,
    /// Content primaries to the working domain's sRGB, row-major.
    matrix: [[f32; 3]; 3],
    /// Destination-white units per unit of content linear light.
    gain: f32,
    /// Nits one destination-white unit stands for.
    reference_nits: f32,
    tone: Option<(ToneMapping, f32, f32)>,
    /// The codes are already what the destination wants; texels need no transform.
    identity: bool,
    /// The destination takes encoded codes rather than linear light.
    encoded: bool,
    /// Decoded linear light per code, when this placement is tabulated for a whole buffer.
    decode: Option<Box<[f32; MAX_CODES]>>,
    /// The encoded value at each step of destination linear light, likewise.
    encode: Option<Box<[f32; ENCODE_STEPS]>>,
}

/// Codes a 10-bit packing can hold, the most any storage uses.
const MAX_CODES: usize = 1024;
/// Steps of destination linear light in the tabulated encode; one step is 1/65535.
const ENCODE_STEPS: usize = 65_536;

impl Placement {
    /// Whether a texel's bytes can be handed over as they are.
    ///
    /// Only true for 8-bit sRGB content on an SDR destination: every other pairing changes
    /// the numbers, even when the net effect is a round trip.
    pub fn is_identity(&self) -> bool {
        self.identity
    }

    /// Whether the result is encoded codes (`true`) rather than linear light.
    pub fn is_encoded(&self) -> bool {
        self.encoded
    }

    /// The same transform with its transfer functions tabulated, for a whole buffer.
    ///
    /// A frame is millions of texels and each transfer function is a `powf`: measured at
    /// 348 ms against 10 ms for a 2560x1440 PQ rendition on this machine. A table is only
    /// built where it is exact: the decode covers every code the packing holds, the encode
    /// every step of an encoded destination. A linear destination keeps the direct encode,
    /// and a tone curve is a luminance ratio rather than a per-channel map, so neither is
    /// tabulated.
    pub fn tabulated(mut self) -> Self {
        // A float packing has no code space to index; its texels decode directly.
        if self.storage.is_integer() {
            let mut decode = Box::new([0.0f32; MAX_CODES]);
            let levels = self.storage.levels();
            for (code, slot) in decode.iter_mut().enumerate() {
                *slot = self.transfer.decode(code as f32 / f32::from(levels));
            }
            self.decode = Some(decode);
        }
        if self.encoded {
            let mut encode = Box::new([0.0f32; ENCODE_STEPS]);
            for (step, slot) in encode.iter_mut().enumerate() {
                *slot = TransferFunction::Srgb.encode(step as f32 / (ENCODE_STEPS - 1) as f32);
            }
            self.encode = Some(encode);
        }
        self
    }

    /// One texel as straight-alpha values in the destination's domain.
    ///
    /// `inline` because the caller is a loop over millions of texels, and the crate boundary
    /// would otherwise keep this a call per texel.
    #[inline]
    pub fn texel(&self, bytes: &[u8]) -> [f32; 4] {
        let Some((codes, alpha)) = self.storage.codes(bytes) else {
            let (values, alpha) = self.storage.unpack(bytes);
            if self.identity {
                return [values[0], values[1], values[2], alpha];
            }
            // A float texel *is* the content's linear light; a transfer decode would clamp it
            // at the reference white and throw away every highlight the packing exists for.
            return self.place(values, alpha);
        };
        let alpha = f32::from(alpha) / 255.0;
        let levels = f32::from(self.storage.levels());
        let code = |code: u16| f32::from(code) / levels;
        if self.identity {
            return [code(codes[0]), code(codes[1]), code(codes[2]), alpha];
        }
        let linear = match &self.decode {
            Some(table) => [
                table[codes[0] as usize],
                table[codes[1] as usize],
                table[codes[2] as usize],
            ],
            None => [
                self.transfer.decode(code(codes[0])),
                self.transfer.decode(code(codes[1])),
                self.transfer.decode(code(codes[2])),
            ],
        };
        self.place(linear, alpha)
    }

    /// Linear light into the destination's domain: primaries, gain, curve, encoding.
    fn place(&self, linear: [f32; 3], alpha: f32) -> [f32; 4] {
        let m = &self.matrix;
        let mut rgb = [
            (m[0][0] * linear[0] + m[0][1] * linear[1] + m[0][2] * linear[2]) * self.gain,
            (m[1][0] * linear[0] + m[1][1] * linear[1] + m[1][2] * linear[2]) * self.gain,
            (m[2][0] * linear[0] + m[2][1] * linear[1] + m[2][2] * linear[2]) * self.gain,
        ];
        if let Some((mapping, source_peak, target_peak)) = self.tone {
            // The curve acts on luminance and keeps the channel ratios, so hue survives
            // where a per-channel curve would desaturate a bright colour.
            let luma = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
            if luma > 0.0 {
                let mapped = mapping.apply(luma * self.reference_nits, source_peak, target_peak);
                let ratio = (mapped / self.reference_nits / luma).max(0.0);
                rgb = [rgb[0] * ratio, rgb[1] * ratio, rgb[2] * ratio];
            }
        }
        if self.encoded {
            match &self.encode {
                Some(table) => {
                    let encoded = |value: f32| {
                        let step = (value.clamp(0.0, 1.0) * (ENCODE_STEPS - 1) as f32) as usize;
                        table[step]
                    };
                    [encoded(rgb[0]), encoded(rgb[1]), encoded(rgb[2]), alpha]
                }
                None => [
                    TransferFunction::Srgb.encode(rgb[0]),
                    TransferFunction::Srgb.encode(rgb[1]),
                    TransferFunction::Srgb.encode(rgb[2]),
                    alpha,
                ],
            }
        } else {
            [rgb[0].max(0.0), rgb[1].max(0.0), rgb[2].max(0.0), alpha]
        }
    }
}

/// The primaries a buffer's values are relative to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSpace {
    /// sRGB / BT.709 primaries, the SDR default.
    Srgb,
    /// Display P3, wider gamut, same transfer options as sRGB.
    DisplayP3,
    /// BT.2020, the HDR wide gamut.
    Bt2020,
}

/// What an output says it wants. Everything here comes from the compositor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputDescription {
    pub color_space: ColorSpace,
    pub transfer: TransferFunction,
    /// Peak luminance in nits; SDR is conventionally 80.
    pub max_luminance: f32,
    /// Black level in nits; never assume exactly zero.
    pub min_luminance: f32,
}

impl OutputDescription {
    /// The plain SDR description, used when the compositor says nothing.
    pub const SDR: Self = Self {
        color_space: ColorSpace::Srgb,
        transfer: TransferFunction::Srgb,
        max_luminance: 80.0,
        min_luminance: 0.0,
    };

    pub fn is_hdr(&self) -> bool {
        self.transfer.is_hdr() || self.max_luminance > 100.0
    }
}

/// A buffer layout the compositor can import.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferFormat {
    /// 8-bit sRGB, premultiplied BGRA. The only SDR format we present.
    Bgra8Srgb,
    /// 10-bit unsigned normalised, premultiplied. Minimum for HDR.
    Rgb10a2,
    /// 16-bit float, premultiplied. Needed above 10-bit precision.
    Rgba16Float,
}

impl BufferFormat {
    /// Bits per channel available for colour.
    pub fn bits_per_channel(self) -> u8 {
        match self {
            Self::Bgra8Srgb => 8,
            Self::Rgb10a2 => 10,
            Self::Rgba16Float => 16,
        }
    }

    /// Pick a format that can carry `out`, preferring the cheapest that suffices.
    ///
    /// HDR needs 16-bit float: a 10-bit RGB buffer has only 2 bits of alpha, and interface
    /// content carries its antialiasing and shadow ramps in alpha, which then collapses to
    /// four steps. Colour depth is not the only requirement.
    pub fn choose(out: &OutputDescription) -> Self {
        if !out.transfer.is_hdr() {
            return Self::Bgra8Srgb;
        }
        Self::Rgba16Float
    }
}

impl BufferFormat {
    /// DRM fourcc for a little-endian premultiplied buffer.
    ///
    /// Alpha matters: a compositor blends the result, so the 10-bit format is ABGR2101010,
    /// not XRGB2101010. The f16 one is ABGR16161616F.
    pub fn drm_fourcc(self) -> u32 {
        match self {
            Self::Bgra8Srgb => 0x3432_5241,   // 'AR24'
            Self::Rgb10a2 => 0x3033_4241,     // 'AB30'
            Self::Rgba16Float => 0x4834_4241, // 'AB48'
        }
    }

    /// Bytes per pixel, used for stride and buffer sizing.
    pub fn bytes_per_pixel(self) -> u32 {
        match self {
            Self::Bgra8Srgb | Self::Rgb10a2 => 4,
            Self::Rgba16Float => 8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_transfer_function_round_trips() {
        // Encode/decode must be inverses, or a colour drifts every time it is copied.
        for tf in [
            TransferFunction::Srgb,
            TransferFunction::Linear,
            TransferFunction::Pq,
            TransferFunction::Hlg,
        ] {
            for i in 0..=20 {
                let v = i as f32 / 20.0;
                let back = tf.encode(tf.decode(v));
                assert!(
                    (back - v).abs() < 1e-4,
                    "{tf:?} at {v}: round trip gave {back}"
                );
            }
        }
    }

    #[test]
    fn hdr_picks_float_buffers_not_ten_bit() {
        // 10-bit RGB carries 2 bits of alpha, which cannot hold a shadow ramp.
        let hdr = OutputDescription {
            transfer: TransferFunction::Pq,
            max_luminance: 200.0,
            ..OutputDescription::SDR
        };
        assert_eq!(BufferFormat::choose(&hdr), BufferFormat::Rgba16Float);
        assert_eq!(
            BufferFormat::choose(&OutputDescription::SDR),
            BufferFormat::Bgra8Srgb
        );
    }

    #[test]
    fn ten_bit_pq_separates_dark_steps_that_eight_bit_srgb_collapses() {
        // This is the whole reason HDR needs 10-bit: in the shadows, 8-bit sRGB runs out
        // of codes first. Counting distinct codes over a dark ramp measures exactly that.
        let ramp: Vec<f32> = (0..64).map(|i| 0.0002 + i as f32 * 0.00002).collect();
        let count = |tf: TransferFunction, bits: u8| {
            let mut codes: Vec<u16> = ramp
                .iter()
                .map(|&l| {
                    let levels = ((1u32 << bits) - 1) as f32;
                    (tf.encode(l) * levels).round() as u16
                })
                .collect();
            codes.sort_unstable();
            codes.dedup();
            codes.len()
        };

        let srgb8 = count(TransferFunction::Srgb, 8);
        let pq10 = count(TransferFunction::Pq, 10);
        assert!(
            srgb8 < 24,
            "8-bit sRGB should collapse most of a dark ramp, got {srgb8} of 64"
        );
        assert!(
            pq10 > srgb8 * 2,
            "10-bit PQ should resolve far more dark steps: {pq10} vs {srgb8}"
        );
    }

    /// One 10-bit word, the way a compositor writes it: red in the low bits, then G and B.
    fn packed10(code: f32) -> [u8; 4] {
        let code = (code * 1023.0).round().clamp(0.0, 1023.0) as u32;
        (code | (code << 10) | (code << 20) | (3 << 30)).to_le_bytes()
    }

    fn pq_code(nits: f32) -> f32 {
        TransferFunction::Pq.encode(nits / 10_000.0)
    }

    #[test]
    fn a_pq_picture_on_an_sdr_surface_keeps_its_white_and_mid_grey() {
        // The direction that matters for a pin: an HDR capture shown where there is no HDR.
        let content = ContentColour {
            storage: PixelStorage::Packed10,
            primaries: Chromaticities::SRGB,
            transfer: TransferFunction::Pq,
            max_nits: 1000.0,
        };
        let placement = content.placement(Destination {
            domain: Domain::Encoded,
            tone: ToneMapping::Clip,
        });
        let white = placement.texel(&packed10(pq_code(203.0)));
        assert!(
            white[..3]
                .iter()
                .all(|channel| (channel - 1.0).abs() < 0.01),
            "a 203-nit white is the SDR peak: {white:?}"
        );
        // 0.5 sRGB is 0.214 linear, 43 nits at a 203-nit reference; it must come back as 128.
        let grey = placement.texel(&packed10(pq_code(
            TransferFunction::Srgb.decode(0.5) * 203.0,
        )));
        for channel in &grey[..3] {
            let byte = (channel.clamp(0.0, 1.0) * 255.0).round();
            assert!((byte - 128.0).abs() <= 1.0, "mid grey became {byte}");
        }
        // On an HDR surface the same picture is a picture of light: every code keeps its
        // luminance, and 203 nits is exactly one unit of the working domain.
        let hdr = Destination {
            domain: Domain::Linear {
                reference_nits: 203.0,
                min_nits: 0.0,
                peak_nits: 417.0,
            },
            tone: ToneMapping::Bt2390,
        };
        let fits = ContentColour {
            max_nits: 417.0,
            ..content
        };
        let placement = fits.placement(hdr);
        for (nits, want) in [(203.0, 1.0), (406.0, 2.0)] {
            let placed = placement.texel(&packed10(pq_code(nits)));
            assert!(
                (placed[0] - want).abs() < 0.01,
                "{nits} nits became {} of the working domain",
                placed[0]
            );
        }
        // Content that claims more than the surface holds is compressed, not clipped: the
        // 1000-nit picture puts 406 nits below the 2.0 the fitted one reached.
        let compressed = content.placement(hdr).texel(&packed10(pq_code(406.0)));
        assert!(
            compressed[0] < 1.95,
            "the tone curve did not run: {}",
            compressed[0]
        );
    }

    /// One grey scRGB texel as binary16, with opaque alpha. Every value the test uses is exact
    /// in binary16, so the bits are arithmetic; the renderer owns the general encoder.
    fn scrgb(value: f32) -> [u8; 8] {
        let exponent = value.log2().floor() as i32;
        let mantissa = ((value / f32::exp2(exponent as f32) - 1.0) * 1024.0).round() as u16;
        let bits = (((exponent + 15) as u16) << 10) | mantissa;
        let mut bytes = [0u8; 8];
        for (slot, half) in bytes
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip([bits; 3].into_iter().chain([0x3c00]))
        {
            slot.copy_from_slice(&half.to_le_bytes());
        }
        bytes
    }

    /// A float capture keeps the light above SDR white: the packing exists so a highlight can
    /// be pinned and exported as light, not as the 255 a clipping ingest would leave.
    #[test]
    fn a_float_picture_keeps_its_light_above_the_reference_white() {
        let content = ContentColour::scrgb(1000.0);
        let hdr = Destination {
            domain: Domain::Linear {
                reference_nits: 203.0,
                min_nits: 0.0,
                peak_nits: 1000.0,
            },
            tone: ToneMapping::Bt2390,
        };
        // scRGB's 2.5 is 200 nits, just under the destination's white; 8.0 is 640 nits.
        let white = content.placement(hdr).texel(&scrgb(2.5));
        assert!(
            (white[0] - 200.0 / 203.0).abs() < 1e-3,
            "SDR white landed at {white:?}"
        );
        let highlight = content.placement(hdr).texel(&scrgb(8.0));
        assert!(
            (highlight[0] - 640.0 / 203.0).abs() < 1e-2,
            "light above white was reduced on the way in: {highlight:?}"
        );
        // A file is SDR, so the same texel lands on the peak while white stays white.
        let file = content.placement(Destination {
            domain: Domain::Encoded,
            tone: ToneMapping::Clip,
        });
        let clipped = file.texel(&scrgb(8.0));
        assert!(
            clipped[..3]
                .iter()
                .all(|channel| (channel - 1.0).abs() < 1e-3),
            "a highlight in a file is the peak: {clipped:?}"
        );
        let encoded_white = file.texel(&scrgb(2.5));
        assert!(
            encoded_white[0] > 0.98,
            "the SDR white dimmed in the file: {encoded_white:?}"
        );
    }

    #[test]
    fn only_srgb_codes_on_an_sdr_surface_are_handed_over_untouched() {
        // The identity branch is what keeps every existing SDR picture byte-exact; widening
        // it would round trip every texture through floats, and missing it would slow the
        // common case to a per-texel conversion.
        assert!(ContentColour::SDR.placement(Destination::SDR).is_identity());
        let p3 = ContentColour {
            primaries: Chromaticities::DISPLAY_P3,
            ..ContentColour::SDR
        };
        assert!(
            !p3.placement(Destination::SDR).is_identity(),
            "a wider gamut needs a conversion even into eight bits"
        );
        let hdr = Destination {
            domain: Domain::Linear {
                reference_nits: 203.0,
                min_nits: 0.0,
                peak_nits: 417.0,
            },
            tone: ToneMapping::Bt2390,
        };
        assert!(
            !ContentColour::SDR.placement(hdr).is_identity(),
            "the same codes mean different light in a linear domain"
        );
    }

    #[test]
    fn the_bt2390_curve_reaches_both_peaks_and_never_folds_back() {
        let curve = |nits: f32| ToneMapping::Bt2390.apply(nits, 1000.0, 203.0);
        assert!(curve(0.0).abs() < 1e-6, "black stays black");
        assert!(
            (curve(1000.0) - 203.0).abs() < 0.5,
            "the source peak lands on the destination peak: {}",
            curve(1000.0)
        );
        assert!(
            (curve(50.0) - 50.0).abs() < 0.5,
            "below the knee the luminance passes through: {}",
            curve(50.0)
        );
        let mut previous = -1.0;
        for step in 0..=400 {
            let mapped = curve(step as f32 * 2.5);
            assert!(mapped >= previous - 1e-3, "folded back at step {step}");
            previous = mapped;
        }
    }

    #[test]
    fn srgb_to_bt2020_matches_the_published_matrix() {
        // Rows sum to 1: a neutral grey must stay neutral in any gamut.
        let m = gamut_matrix(Chromaticities::SRGB, Chromaticities::BT2020);
        for row in m {
            let sum: f32 = row.iter().sum();
            assert!((sum - 1.0).abs() < 1e-4, "row sums to {sum}");
        }
        assert!((m[0][0] - 0.6274).abs() < 0.01, "got {}", m[0][0]);
        assert!((m[2][2] - 0.8956).abs() < 0.01, "got {}", m[2][2]);
    }
}
