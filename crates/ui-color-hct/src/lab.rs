//! sRGB, linear light and CIELAB L\*, in the 0..100 domain HCT works in.

/// D65 white point on the same 0..100 scale as `linearized`.
pub const WHITE_D65: [f64; 3] = [95.047, 100.0, 108.883];

const K_E: f64 = 8.0;
const KAPPA: f64 = 24389.0 / 27.0;
const EPSILON: f64 = 216.0 / 24389.0;

/// 8-bit sRGB channel to linear light, scaled so white is 100.
pub fn linearized(channel: u8) -> f64 {
    let v = f64::from(channel) / 255.0;
    let lin = if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    };
    lin * 100.0
}

/// Linear light in the 0..100 domain back to an 8-bit channel.
pub fn delinearized(v: f64) -> u8 {
    let n = v / 100.0;
    let s = if n <= 0.0031308 {
        n * 12.92
    } else {
        1.055 * n.powf(1.0 / 2.4) - 0.055
    };
    (s.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// CIE L\* to relative luminance Y (0..100).
pub fn y_from_lstar(lstar: f64) -> f64 {
    if lstar > K_E {
        let c = (lstar + 16.0) / 116.0;
        c * c * c * 100.0
    } else {
        lstar / KAPPA * 100.0
    }
}

/// Relative luminance Y (0..100) to CIE L\*: this is HCT's tone.
pub fn lstar_from_y(y: f64) -> f64 {
    let n = y / 100.0;
    if n <= EPSILON {
        KAPPA * n
    } else {
        116.0 * n.cbrt() - 16.0
    }
}

pub fn red_of(argb: u32) -> u8 {
    ((argb >> 16) & 0xff) as u8
}

pub fn green_of(argb: u32) -> u8 {
    ((argb >> 8) & 0xff) as u8
}

pub fn blue_of(argb: u32) -> u8 {
    (argb & 0xff) as u8
}

pub fn rgb(red: u8, green: u8, blue: u8) -> u32 {
    0xff00_0000 | (u32::from(red) << 16) | (u32::from(green) << 8) | u32::from(blue)
}

/// XYZ (0..100, D65) of an opaque sRGB colour.
pub fn xyz_from_argb(argb: u32) -> [f64; 3] {
    let r = linearized(red_of(argb));
    let g = linearized(green_of(argb));
    let b = linearized(blue_of(argb));
    [
        0.41233895 * r + 0.35762064 * g + 0.18051042 * b,
        0.2126 * r + 0.7152 * g + 0.0722 * b,
        0.01932141 * r + 0.11916382 * g + 0.95034478 * b,
    ]
}

/// Linear sRGB (0..100) from XYZ; channels may fall outside 0..100 when out of gamut.
pub fn linrgb_from_xyz(xyz: [f64; 3]) -> [f64; 3] {
    let [x, y, z] = xyz;
    [
        3.2406 * x - 1.5372 * y - 0.4986 * z,
        -0.9689 * x + 1.8758 * y + 0.0415 * z,
        0.0557 * x - 0.2040 * y + 1.0570 * z,
    ]
}

pub fn argb_from_linrgb(lin: [f64; 3]) -> u32 {
    rgb(
        delinearized(lin[0]),
        delinearized(lin[1]),
        delinearized(lin[2]),
    )
}

/// The tone (CIE L\*) of an opaque sRGB colour.
pub fn tone_of(argb: u32) -> f64 {
    lstar_from_y(xyz_from_argb(argb)[1])
}

/// A neutral colour at the requested tone.
pub fn argb_from_tone(tone: f64) -> u32 {
    let v = delinearized(y_from_lstar(tone));
    rgb(v, v, v)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Both the hex and the tone come from MD3's own baseline docs: primary is primary-tone-40.
    #[test]
    fn the_md3_baseline_primary_sits_at_tone_40() {
        let got = tone_of(0xff67_50a4);
        assert!((got - 40.0).abs() < 0.6, "got {got}");
    }
}
