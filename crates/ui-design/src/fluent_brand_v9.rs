//! Fluent v9 brand ramp: a faithful port of FluentUI's `getBrandTokensFromPalette`.
//!
//! Sources (microsoft/fluentui, MIT): `theme-designer/src/colors/{palettes,csswg,geometry,hueMap}.ts`.
//! Formulas are reproduced as written, including D50 adaptation and `floor(x*256)` channel packing.

use crate::fluent_hue_map::HUE_TO_SNAPPING_POINTS;
use crate::value::Rgba;

pub use crate::fluent_brand::{KEY_TONE, TONES};

/// FluentUI `defaultLinearity`: 0 = logarithmic distribution, 1 = linear.
pub const LINEARITY: f64 = 0.75;
/// Dark/light Bezier control fractions (`darkCp` / `lightCp`).
pub const DARK_CP: f64 = 2.0 / 3.0;
pub const LIGHT_CP: f64 = 1.0 / 3.0;
/// `hueTorsion`: 0 keeps the hue fixed along the curve.
pub const HUE_TORSION: f64 = 0.0;
/// `curveDepth`, which sets the sampling divisions per Bezier.
pub const CURVE_DEPTH: f64 = 24.0;

/// Which lightness spacing to use when distributing the 16 shades.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LogDistribution {
    /// FluentUI's own path (log term collapses to 1..2); reproduced for fidelity.
    #[default]
    Faithful,
    /// Log term spans the real lightness band, so the light end reaches the table's max.
    Corrected,
}

type Vec3 = [f64; 3];

fn mul3(m: &[[f64; 3]; 3], v: Vec3) -> Vec3 {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn lin_srgb(v: Vec3) -> Vec3 {
    v.map(|val| {
        let sign = if val < 0.0 { -1.0 } else { 1.0 };
        let a = val.abs();
        if a < 0.040_45 {
            val / 12.92
        } else {
            sign * ((a + 0.055) / 1.055).powf(2.4)
        }
    })
}

fn gam_srgb(v: Vec3) -> Vec3 {
    v.map(|val| {
        let sign = if val < 0.0 { -1.0 } else { 1.0 };
        let a = val.abs();
        if a > 0.003_130_8 {
            sign * (1.055 * a.powf(1.0 / 2.4) - 0.055)
        } else {
            12.92 * val
        }
    })
}

const LIN_TO_XYZ: [[f64; 3]; 3] = [
    [
        0.412_390_799_265_959_34,
        0.357_584_339_383_878,
        0.180_480_788_401_834_3,
    ],
    [
        0.212_639_005_871_510_27,
        0.715_168_678_767_756,
        0.072_192_315_360_733_71,
    ],
    [
        0.019_330_818_715_591_82,
        0.119_194_779_794_625_98,
        0.950_532_152_249_660_7,
    ],
];

const XYZ_TO_LIN: [[f64; 3]; 3] = [
    [
        3.240_969_941_904_522_6,
        -1.537_383_177_570_094,
        -0.498_610_760_293_003_4,
    ],
    [
        -0.969_243_636_280_879_6,
        1.875_967_501_507_720_2,
        0.041_555_057_407_175_59,
    ],
    [
        0.055_630_079_696_993_66,
        -0.203_976_958_888_976_52,
        1.056_971_514_242_878_6,
    ],
];

const D65_TO_D50: [[f64; 3]; 3] = [
    [
        1.047_929_820_840_548_8,
        0.022_946_793_341_019_088,
        -0.050_192_229_543_135_57,
    ],
    [
        0.029_627_815_688_159_344,
        0.990_434_484_573_249,
        -0.017_073_825_029_385_14,
    ],
    [
        -0.009_243_058_152_591_178,
        0.015_055_144_896_577_895,
        0.751_874_289_958_000_8,
    ],
];

const D50_TO_D65: [[f64; 3]; 3] = [
    [
        0.955_473_452_704_218_2,
        -0.023_098_536_874_261_423,
        0.063_259_308_661_021_7,
    ],
    [
        -0.028_369_706_963_208_136,
        1.009_995_458_005_822_6,
        0.021_041_398_966_943_008,
    ],
    [
        0.012_314_001_688_319_899,
        -0.020_507_696_433_477_912,
        1.330_365_936_608_075_3,
    ],
];

/// D50 reference white, the white the Lab conversion is anchored to.
const WHITE_D50: Vec3 = [0.964_22, 1.0, 0.825_21];

fn xyz_to_lab(xyz: Vec3) -> Vec3 {
    const EPS: f64 = 216.0 / 24389.0;
    const KAPPA: f64 = 24389.0 / 27.0;
    let f = |i: usize| {
        let v = xyz[i] / WHITE_D50[i];
        if v > EPS {
            v.cbrt()
        } else {
            (KAPPA * v + 16.0) / 116.0
        }
    };
    let (fx, fy, fz) = (f(0), f(1), f(2));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn lab_to_xyz(lab: Vec3) -> Vec3 {
    const EPS: f64 = 216.0 / 24389.0;
    const KAPPA: f64 = 24389.0 / 27.0;
    let fy = (lab[0] + 16.0) / 116.0;
    let fx = lab[1] / 500.0 + fy;
    let fz = fy - lab[2] / 200.0;
    let x = if fx.powi(3) > EPS {
        fx.powi(3)
    } else {
        (116.0 * fx - 16.0) / KAPPA
    };
    let y = if lab[0] > KAPPA * EPS {
        ((lab[0] + 16.0) / 116.0).powi(3)
    } else {
        lab[0] / KAPPA
    };
    let z = if fz.powi(3) > EPS {
        fz.powi(3)
    } else {
        (116.0 * fz - 16.0) / KAPPA
    };
    [x * WHITE_D50[0], y * WHITE_D50[1], z * WHITE_D50[2]]
}

fn lab_to_lch(lab: Vec3) -> Vec3 {
    let h = lab[2].atan2(lab[1]).to_degrees();
    [
        lab[0],
        (lab[1] * lab[1] + lab[2] * lab[2]).sqrt(),
        if h >= 0.0 { h } else { h + 360.0 },
    ]
}

fn lch_to_lab(lch: Vec3) -> Vec3 {
    let r = lch[2].to_radians();
    [lch[0], lch[1] * r.cos(), lch[1] * r.sin()]
}

fn srgb_to_lch(rgb: Vec3) -> Vec3 {
    lab_to_lch(xyz_to_lab(mul3(
        &D65_TO_D50,
        mul3(&LIN_TO_XYZ, lin_srgb(rgb)),
    )))
}

fn lch_to_srgb(lch: Vec3) -> Vec3 {
    gam_srgb(mul3(
        &XYZ_TO_LIN,
        mul3(&D50_TO_D65, lab_to_xyz(lch_to_lab(lch))),
    ))
}

fn lab_to_srgb(lab: Vec3) -> Vec3 {
    gam_srgb(mul3(&XYZ_TO_LIN, mul3(&D50_TO_D65, lab_to_xyz(lab))))
}

/// `hex_to_sRGB`: an 8-bit triple scaled to `0..=1`.
fn argb_to_srgb(argb: u32) -> Vec3 {
    [
        ((argb >> 16) & 0xff) as f64 / 255.0,
        ((argb >> 8) & 0xff) as f64 / 255.0,
        (argb & 0xff) as f64 / 255.0,
    ]
}

/// `sRGB_to_hex`: `floor(x * 256)` channel packing, exactly as FluentUI writes it.
fn srgb_to_rgb8(rgb: Vec3) -> (u8, u8, u8) {
    let ch = |x: f64| {
        if x < 0.0 {
            0u8
        } else if x >= 1.0 {
            255u8
        } else {
            (x * 256.0).floor() as u8
        }
    };
    (ch(rgb[0]), ch(rgb[1]), ch(rgb[2]))
}

fn lab_to_rgba(lab: Vec3) -> Rgba {
    let (r, g, b) = srgb_to_rgb8(lab_to_srgb(lab));
    Rgba::from_rgb(r, g, b)
}

fn is_lch_inside_srgb(l: f64, c: f64, h: f64) -> bool {
    const EPS: f64 = 0.000_005;
    lch_to_srgb([l, c, h])
        .iter()
        .all(|v| (-EPS..=1.0 + EPS).contains(v))
}

/// `snap_into_gamut`: hold L and h, binary-search chroma onto the sRGB boundary.
fn snap_into_gamut(lab: Vec3) -> Vec3 {
    const EPS: f64 = 0.000_1;
    let lch = lab_to_lch(lab);
    let (l, h) = (lch[0], lch[2]);
    if is_lch_inside_srgb(l, lch[1], h) {
        return lab;
    }
    let mut hi_c = lch[1];
    let mut lo_c = 0.0;
    let mut c = lch[1] / 2.0;
    while hi_c - lo_c > EPS {
        if is_lch_inside_srgb(l, c, h) {
            lo_c = c;
        } else {
            hi_c = c;
        }
        c = (hi_c + lo_c) / 2.0;
    }
    lch_to_lab([l, c, h])
}

/// `hexToHue`: the integer **HSL** hue used to index the snapping-point table.
fn hsl_hue(argb: u32) -> usize {
    let r = ((argb >> 16) & 0xff) as f64 / 255.0;
    let g = ((argb >> 8) & 0xff) as f64 / 255.0;
    let b = (argb & 0xff) as f64 / 255.0;
    let cmax = r.max(g).max(b);
    let cmin = r.min(g).min(b);
    let d = cmax - cmin;
    let mut hue = if d == 0.0 {
        0.0
    } else if cmax == r {
        ((g - b) / d) % 6.0
    } else if cmax == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    hue = (hue * 60.0).round();
    if hue < 0.0 {
        hue += 360.0;
    }
    (hue as usize).min(359)
}

/// `snappingPointsForKeyColor`: `[min, center, max]` Lab lightness for the seed's hue.
fn snapping_points(argb: u32) -> Vec3 {
    let row = HUE_TO_SNAPPING_POINTS[hsl_hue(argb)];
    [
        row[0] as f64 * 100.0,
        row[1] as f64 * 100.0,
        row[2] as f64 * 100.0,
    ]
}

/// `linearInterpolationThroughPoint`: `n` samples from `start` to `end` forced through `in_between`.
fn linear_through_point(start: f64, end: f64, in_between: f64, n: usize) -> Vec<f64> {
    let ratio = (in_between - start) / (end - start);
    let idx = (((n - 1) as f64 * ratio).floor() as usize).clamp(1, n - 2);
    let mut out = vec![0.0; n];
    out[0] = start;
    out[idx] = in_between;
    out[n - 1] = end;
    let step_before = (in_between - start) / idx as f64;
    let step_after = (end - in_between) / (n - 1 - idx) as f64;
    for i in 1..idx {
        out[i] = start + i as f64 * step_before;
    }
    for i in (idx + 1)..(n - 1) {
        out[i] = in_between + (i - idx) as f64 * step_after;
    }
    out
}

/// `getLogSpace` under the chosen distribution; the faithful arm keeps FluentUI's quirk.
fn log_space(range: [f64; 2], n: usize, dist: LogDistribution) -> Vec<f64> {
    match dist {
        // FluentUI passes log10(0)/log10(100) and treats a non-positive min as `a = 0`,
        // so the values collapse to 1..2 and the ramp never reaches the light end.
        LogDistribution::Faithful => {
            let a = 0.0_f64;
            let b = 100.0_f64.log10().ln();
            let delta = (b - a) / n as f64;
            let mut out = vec![a.exp()];
            for i in 1..n {
                out.push((a + delta * i as f64).exp());
            }
            out.push(b.exp());
            out
        }
        // Corrected: geometric spacing across the real band `[range0, range1]` in Lab L.
        LogDistribution::Corrected => {
            let lo = range[0].max(0.5);
            let ratio = range[1] / lo;
            (0..n)
                .map(|i| lo * ratio.powf(i as f64 / (n - 1) as f64))
                .collect()
        }
    }
}

fn bezier(t: f64, p0: f64, p1: f64, p2: f64) -> f64 {
    let k = 1.0 - t;
    k * k * p0 + 2.0 * k * t * p1 + t * t * p2
}

fn point_on_curve(curve: [Vec3; 3], t: f64) -> Vec3 {
    let [v0, v1, v2] = curve;
    [
        bezier(t, v0[0], v1[0], v2[0]),
        bezier(t, v0[1], v1[1], v2[1]),
        bezier(t, v0[2], v1[2], v2[2]),
    ]
}

/// `getPointsOnCurvePath`: uniform-t samples per curve, dropping consecutive duplicates.
fn points_on_curve_path(curves: [[Vec3; 3]; 2], divisions: usize) -> Vec<Vec3> {
    let mut points = Vec::new();
    for curve in curves {
        for d in 0..=divisions {
            let p = point_on_curve(curve, d as f64 / divisions as f64);
            if points.last() == Some(&p) {
                continue;
            }
            points.push(p);
        }
    }
    points
}

/// `getPointOnHelix`: rotate the hue by `torsion * (L - torsionT0)`.
fn point_on_helix(point: Vec3, torsion: f64, torsion_t0: f64) -> Vec3 {
    let t = point[0];
    let lch = lab_to_lch(point);
    lch_to_lab([lch[0], lch[1], lch[2] + torsion * (t - torsion_t0)])
}

/// `curvePathFromPalette`: black -> key -> white as two quadratic Beziers in Lab.
fn curve_path_from_palette(key_lch: Vec3, dark_cp: f64, light_cp: f64) -> ([[Vec3; 3]; 2], f64) {
    let key = lch_to_lab(key_lch);
    let l = key[0];
    let dark_ctrl = [l * (1.0 - dark_cp), key[1], key[2]];
    let light_ctrl = [l + (100.0 - l) * light_cp, key[1], key[2]];
    (
        [
            [[0.0, 0.0, 0.0], dark_ctrl, key],
            [key, light_ctrl, [100.0, 0.0, 0.0]],
        ],
        l,
    )
}

/// `paletteShadesFromCurvePoints`: blend log- and linear-spaced lightness, then read a/b off the curve.
fn shades_from_curve_points(
    points: &[Vec3],
    n: usize,
    linearity: f64,
    argb: u32,
    dist: LogDistribution,
) -> Vec<Vec3> {
    if points.len() <= 2 {
        return Vec::new();
    }
    let sp = snapping_points(argb);
    let range = [sp[0], sp[2]];
    let log_l = log_space(range, n, dist);
    let lin_l = linear_through_point(range[0], range[1], sp[1], 16);

    let mut c = 0usize;
    let mut shades = Vec::with_capacity(n);
    for i in 0..n {
        let l = range[1].min(range[0].max(log_l[i] * (1.0 - linearity) + lin_l[i] * linearity));
        while c + 1 < points.len() && l > points[c + 1][0] {
            c += 1;
        }
        let (p1, p2) = (points[c], points[c + 1]);
        let u = if p2[0] != p1[0] {
            (l - p1[0]) / (p2[0] - p1[0])
        } else {
            0.0
        };
        shades.push([
            p1[0] + (p2[0] - p1[0]) * u,
            p1[1] + (p2[1] - p1[1]) * u,
            p1[2] + (p2[2] - p1[2]) * u,
        ]);
    }
    shades.into_iter().map(snap_into_gamut).collect()
}

/// Generate the 16-tone ramp using FluentUI's faithful lightness distribution.
pub fn brand_ramp_from_seed(seed: Rgba) -> [(u16, Rgba); 16] {
    brand_ramp_from_seed_with(seed, LogDistribution::Faithful)
}

/// Generate the 16-tone ramp with a chosen lightness distribution.
pub fn brand_ramp_from_seed_with(seed: Rgba, dist: LogDistribution) -> [(u16, Rgba); 16] {
    let argb = seed.argb();
    let key_lch = srgb_to_lch(argb_to_srgb(argb));
    let (curves, torsion_t0) = curve_path_from_palette(key_lch, DARK_CP, LIGHT_CP);
    let divisions = ((CURVE_DEPTH * (1.0 + HUE_TORSION.abs().max(1.0))) / 2.0).ceil() as usize;
    let points: Vec<Vec3> = points_on_curve_path(curves, divisions)
        .into_iter()
        .map(|p| point_on_helix(p, HUE_TORSION, torsion_t0))
        .collect();
    let shades = shades_from_curve_points(&points, TONES.len(), LINEARITY, argb, dist);

    let mut out = [(0u16, Rgba::TRANSPARENT); 16];
    for (i, tone) in TONES.iter().enumerate() {
        out[i] = (*tone, lab_to_rgba(shades[i]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The faithful port must reproduce the reference implementation's output byte for byte.
    #[test]
    fn the_port_reproduces_the_reference_implementation_for_the_default_blue() {
        let ramp = brand_ramp_from_seed(Rgba::from_argb(0xff0f_6cbd));
        let expected: [u32; 16] = [
            0xff00030a, 0xff01142b, 0xff001f3f, 0xff00284e, 0xff00315d, 0xff003b6c, 0xff00447c,
            0xff004e8d, 0xff00589e, 0xff0062af, 0xff106cbe, 0xff2a77c8, 0xff3f81d1, 0xff518bd9,
            0xff6396e0, 0xff74a0e6,
        ];
        for ((tone, got), want) in ramp.iter().zip(expected) {
            assert_eq!(got.argb(), want, "tone {tone} diverged from the reference");
        }
    }
}
