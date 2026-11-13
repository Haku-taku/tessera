//! CAM16: forward from sRGB, and the inverse path the solver reuses.

use crate::lab::{argb_from_linrgb, linrgb_from_xyz, xyz_from_argb};
use crate::viewing::Viewing;

/// The CAM16 attributes of a colour. `hue` and `chroma` become HCT's H and C.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cam {
    pub hue: f64,
    pub chroma: f64,
    pub j: f64,
    pub q: f64,
    pub m: f64,
    pub s: f64,
    pub jstar: f64,
    pub astar: f64,
    pub bstar: f64,
}

fn signum(v: f64) -> f64 {
    if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        0.0
    }
}

fn wrap_degrees(mut d: f64) -> f64 {
    d %= 360.0;
    if d < 0.0 {
        d += 360.0;
    }
    d
}

/// Cone response after chromatic adaptation, from XYZ.
fn adapted_cone(xyz: [f64; 3]) -> [f64; 3] {
    [
        0.401288 * xyz[0] + 0.650173 * xyz[1] - 0.051461 * xyz[2],
        -0.250268 * xyz[0] + 1.204414 * xyz[1] + 0.045854 * xyz[2],
        -0.002079 * xyz[0] + 0.048952 * xyz[1] + 0.953127 * xyz[2],
    ]
}

/// Inverse of the adaptation-only step: LMS scaled by `rgb_d` back towards XYZ space.
fn unadapt_cone(rgb_d: [f64; 3], adapted: [f64; 3]) -> [f64; 3] {
    [
        adapted[0] / rgb_d[0],
        adapted[1] / rgb_d[1],
        adapted[2] / rgb_d[2],
    ]
}

const CONE_TO_XYZ: [[f64; 3]; 3] = [
    [1.86206786, -1.01125463, 0.14918677],
    [0.38752654, 0.62144744, -0.00897398],
    [-0.01584150, -0.03412294, 1.04996444],
];

fn mat(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// Post-adaptation opponent components from the adapted cone responses.
fn opponent(r_a: f64, g_a: f64, b_a: f64) -> (f64, f64, f64, f64) {
    let a = (11.0 * r_a - 12.0 * g_a + b_a) / 11.0;
    let b = (r_a + g_a - 2.0 * b_a) / 9.0;
    let u = (20.0 * r_a + 20.0 * g_a + 21.0 * b_a) / 20.0;
    let p2 = (40.0 * r_a + 20.0 * g_a + b_a) / 20.0;
    (a, b, u, p2)
}

fn forward(r_d: [f64; 3], v: &Viewing) -> [f64; 3] {
    let af = [
        (v.fl * r_d[0].abs() / 100.0).powf(0.42),
        (v.fl * r_d[1].abs() / 100.0).powf(0.42),
        (v.fl * r_d[2].abs() / 100.0).powf(0.42),
    ];
    [
        signum(r_d[0]) * 400.0 * af[0] / (af[0] + 27.13),
        signum(r_d[1]) * 400.0 * af[1] / (af[1] + 27.13),
        signum(r_d[2]) * 400.0 * af[2] / (af[2] + 27.13),
    ]
}

/// CAM16 of an opaque sRGB colour under the given viewing conditions.
pub fn cam_from_argb_with(argb: u32, v: &Viewing) -> Cam {
    let r_c = adapted_cone(xyz_from_argb(argb));
    let r_d = [
        r_c[0] * v.rgb_d[0],
        r_c[1] * v.rgb_d[1],
        r_c[2] * v.rgb_d[2],
    ];
    let a_rgb = forward(r_d, v);
    let (a, b, u, p2) = opponent(a_rgb[0], a_rgb[1], a_rgb[2]);
    let hue = wrap_degrees(b.atan2(a).to_degrees());
    let hue_r = hue.to_radians();
    let ac = p2 * v.nbb;
    let j = 100.0 * (ac / v.aw).powf(v.c * v.z);
    let q = 4.0 / v.c * (j / 100.0).sqrt() * (v.aw + 4.0) * v.fl_root;
    let h_prime = if hue < 20.14 { hue + 360.0 } else { hue };
    let e_hue = 0.25 * ((h_prime.to_radians() + 2.0).cos() + 3.8);
    let p1 = 50000.0 / 13.0 * e_hue * v.n_c * v.ncb;
    let t = p1 * (a * a + b * b).sqrt() / (u + 0.305);
    let alpha = t.powf(0.9) * (1.64 - 0.29f64.powf(v.n)).powf(0.73);
    let chroma = alpha * (j / 100.0).sqrt();
    let m = chroma * v.fl_root;
    let s = 50.0 * (alpha * v.c / (v.aw + 4.0)).sqrt();
    let jstar = (1.0 + 100.0 * 0.007) * j / (1.0 + 0.007 * j);
    let mstar = (1.0 + 0.0228 * m).ln() / 0.0228;
    Cam {
        hue,
        chroma,
        j,
        q,
        m,
        s,
        jstar,
        astar: mstar * hue_r.cos(),
        bstar: mstar * hue_r.sin(),
    }
}

pub fn cam_from_argb(argb: u32) -> Cam {
    cam_from_argb_with(argb, &Viewing::srgb())
}

/// Linear sRGB (0..100) for a CAM16 stimulus; channels may leave the gamut box.
pub fn linrgb_from_jch(j: f64, chroma: f64, hue: f64, v: &Viewing) -> [f64; 3] {
    let alpha = if chroma == 0.0 || j == 0.0 {
        0.0
    } else {
        chroma / (j / 100.0).sqrt()
    };
    let t = (alpha / (1.64 - 0.29f64.powf(v.n)).powf(0.73)).powf(1.0 / 0.9);
    let h_rad = hue.to_radians();
    let e_hue = 0.25 * ((h_rad + 2.0).cos() + 3.8);
    let ac = v.aw * (j / 100.0).powf(1.0 / v.c / v.z);
    let p1 = e_hue * (50000.0 / 13.0) * v.n_c * v.ncb;
    let p2 = ac / v.nbb;
    let gamma =
        23.0 * (p2 + 0.305) * t / (23.0 * p1 + 11.0 * t * h_rad.cos() + 108.0 * t * h_rad.sin());
    let a = gamma * h_rad.cos();
    let b = gamma * h_rad.sin();
    let r_a = (460.0 * p2 + 451.0 * a + 288.0 * b) / 1403.0;
    let g_a = (460.0 * p2 - 891.0 * a - 261.0 * b) / 1403.0;
    let b_a = (460.0 * p2 - 220.0 * a - 6300.0 * b) / 1403.0;
    let back = |x: f64| {
        let base = (27.13 * x.abs() / (400.0 - x.abs())).max(0.0);
        signum(x) * (100.0 / v.fl) * base.powf(1.0 / 0.42)
    };
    let cone = [back(r_a), back(g_a), back(b_a)];
    let xyz = mat(CONE_TO_XYZ, unadapt_cone(v.rgb_d, cone));
    linrgb_from_xyz(xyz)
}

/// The sRGB colour of a CAM16 stimulus; out-of-gamut channels are clipped by encoding.
pub fn argb_from_cam_with(cam: Cam, v: &Viewing) -> u32 {
    argb_from_linrgb(linrgb_from_jch(cam.j, cam.chroma, cam.hue, v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lab::tone_of;

    #[test]
    fn a_neutral_stays_near_the_chroma_floor() {
        for tone in [20.0, 50.0, 80.0] {
            let argb = crate::lab::argb_from_tone(tone);
            assert!((tone_of(argb) - tone).abs() < 0.6);
            assert!(cam_from_argb(argb).chroma < 4.0, "tone {tone}");
        }
    }
}
