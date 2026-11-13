//! CAM16 viewing conditions, derived from the surround/luminance inputs.

use crate::lab::{WHITE_D65, y_from_lstar};

/// The parameters CAM16 needs beyond the stimulus itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewing {
    pub adapting_luminance: f64,
    pub background_lstar: f64,
    pub surround: f64,
    pub discounting_illuminant: bool,
    /// Background Y relative to the white point Y.
    pub n: f64,
    pub aw: f64,
    pub nbb: f64,
    pub ncb: f64,
    pub c: f64,
    pub n_c: f64,
    pub fl: f64,
    pub fl_root: f64,
    pub z: f64,
    pub white: [f64; 3],
    pub rgb_d: [f64; 3],
}

/// Colour math stays f64 end to end, as the reference model is: these values feed CAM16 terms
/// whose contrast outcomes the tone checks compare, where f32 noise can flip a step.
fn lerp(start: f64, stop: f64, amount: f64) -> f64 {
    (1.0 - amount) * start + amount * stop
}

/// CAT02-like LMS transform of the white point, used for the adaptation factors.
fn cone_response(xyz: [f64; 3]) -> [f64; 3] {
    [
        0.401288 * xyz[0] + 0.650173 * xyz[1] - 0.051461 * xyz[2],
        -0.250268 * xyz[0] + 1.204414 * xyz[1] + 0.045854 * xyz[2],
        -0.002079 * xyz[0] + 0.048952 * xyz[1] + 0.953127 * xyz[2],
    ]
}

impl Viewing {
    /// Derive every constant; a background below L\* 30 is lifted to 30, as the model requires.
    pub fn create(
        white: [f64; 3],
        adapting_luminance: f64,
        background_lstar: f64,
        surround: f64,
        discounting_illuminant: bool,
    ) -> Self {
        let bg = background_lstar.max(30.0);
        let rgb_w = cone_response(white);
        let f = 0.8 + surround / 10.0;
        let c = if f >= 0.9 {
            lerp(0.59, 0.69, (f - 0.9) * 10.0)
        } else {
            lerp(0.525, 0.59, (f - 0.8) * 10.0)
        };
        let d = if discounting_illuminant {
            1.0
        } else {
            f * (1.0 - (1.0 / 3.6) * ((-adapting_luminance - 42.0) / 92.0).exp())
        }
        .clamp(0.0, 1.0);
        let rgb_d = [
            d * (100.0 / rgb_w[0]) + 1.0 - d,
            d * (100.0 / rgb_w[1]) + 1.0 - d,
            d * (100.0 / rgb_w[2]) + 1.0 - d,
        ];
        let k = 1.0 / (5.0 * adapting_luminance + 1.0);
        let k4 = k * k * k * k;
        let k4f = 1.0 - k4;
        let fl =
            k4 * adapting_luminance + 0.1 * k4f * k4f * (5.0 * adapting_luminance).powf(1.0 / 3.0);
        let fl_root = fl.powf(0.25);
        let n = y_from_lstar(bg) / white[1];
        let z = 1.48 + n.sqrt();
        let nbb = 0.725 / n.powf(0.2);
        let factors = [
            (fl * rgb_d[0] * rgb_w[0] / 100.0).powf(0.42),
            (fl * rgb_d[1] * rgb_w[1] / 100.0).powf(0.42),
            (fl * rgb_d[2] * rgb_w[2] / 100.0).powf(0.42),
        ];
        let rgb_a = [
            400.0 * factors[0] / (factors[0] + 27.13),
            400.0 * factors[1] / (factors[1] + 27.13),
            400.0 * factors[2] / (factors[2] + 27.13),
        ];
        let aw = (40.0 * rgb_a[0] + 20.0 * rgb_a[1] + rgb_a[2]) / 20.0 * nbb;
        Self {
            adapting_luminance,
            background_lstar: bg,
            surround,
            discounting_illuminant,
            n,
            aw,
            nbb,
            ncb: nbb,
            c,
            n_c: f,
            fl,
            fl_root,
            z,
            white,
            rgb_d,
        }
    }

    /// sRGB on D65, average surround, dim adapting field: the conditions HCT defines itself under.
    pub fn srgb() -> Self {
        Self::create(WHITE_D65, 11.725676537, 50.0, 2.0, false)
    }
}
