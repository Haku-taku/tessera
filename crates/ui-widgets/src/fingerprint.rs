//! Fingerprints for `Widget::cache_key`: one `u64` over every input `decl` reads.
//!
//! Floating point is the reason this exists: `f32` is not `Hash`, and a control is mostly
//! floats and colours. Everything is hashed by bit pattern, so `0.1 + 0.2` and `0.3` are
//! different fingerprints — which is what a cache needs, not what arithmetic needs.

use core::hash::Hasher;

use ui_core::Id;
use ui_core::collections::FastHasher;
use ui_core::geom::Color;
use ui_core::text::{Family, GenericFamily, Weight};

/// An input a control can fingerprint.
pub trait Fingerprint {
    fn fingerprint(&self, h: &mut FastHasher);
}

impl Fingerprint for f32 {
    fn fingerprint(&self, h: &mut FastHasher) {
        h.write_u32(self.to_bits());
    }
}

impl Fingerprint for f64 {
    fn fingerprint(&self, h: &mut FastHasher) {
        h.write_u64(self.to_bits());
    }
}

macro_rules! integer_fingerprint {
    ($($t:ty),* $(,)?) => {
        $(impl Fingerprint for $t {
            fn fingerprint(&self, h: &mut FastHasher) {
                h.write_u64(*self as u64);
            }
        })*
    };
}

integer_fingerprint!(u8, u16, u32, u64, usize, i32, i64);

impl Fingerprint for bool {
    fn fingerprint(&self, h: &mut FastHasher) {
        h.write_u8(*self as u8);
    }
}

impl Fingerprint for char {
    fn fingerprint(&self, h: &mut FastHasher) {
        h.write_u32(*self as u32);
    }
}

impl Fingerprint for Id {
    fn fingerprint(&self, h: &mut FastHasher) {
        h.write_u64(self.as_u64());
    }
}

impl Fingerprint for Color {
    fn fingerprint(&self, h: &mut FastHasher) {
        self.r.fingerprint(h);
        self.g.fingerprint(h);
        self.b.fingerprint(h);
        self.a.fingerprint(h);
    }
}

impl Fingerprint for str {
    fn fingerprint(&self, h: &mut FastHasher) {
        h.write_usize(self.len());
        h.write(self.as_bytes());
    }
}

impl Fingerprint for String {
    fn fingerprint(&self, h: &mut FastHasher) {
        self.as_str().fingerprint(h);
    }
}

impl<T: Fingerprint> Fingerprint for Option<T> {
    fn fingerprint(&self, h: &mut FastHasher) {
        match self {
            Some(v) => {
                h.write_u8(1);
                v.fingerprint(h);
            }
            None => h.write_u8(0),
        }
    }
}

impl<T: Fingerprint> Fingerprint for [T] {
    fn fingerprint(&self, h: &mut FastHasher) {
        h.write_usize(self.len());
        for v in self {
            v.fingerprint(h);
        }
    }
}

impl<T: Fingerprint> Fingerprint for Vec<T> {
    fn fingerprint(&self, h: &mut FastHasher) {
        self.as_slice().fingerprint(h);
    }
}

impl Fingerprint for Weight {
    fn fingerprint(&self, h: &mut FastHasher) {
        h.write_u16(self.0);
    }
}

impl Fingerprint for GenericFamily {
    fn fingerprint(&self, h: &mut FastHasher) {
        h.write_u8(match self {
            Self::SansSerif => 0,
            Self::Serif => 1,
            Self::Monospace => 2,
            Self::SystemUi => 3,
        });
    }
}

impl Fingerprint for Family {
    fn fingerprint(&self, h: &mut FastHasher) {
        match self {
            Self::Generic(generic) => {
                h.write_u8(0);
                generic.fingerprint(h);
            }
            Self::Named(name) => {
                h.write_u8(1);
                name.fingerprint(h);
            }
        }
    }
}

impl Fingerprint for crate::icon::IconSource {
    fn fingerprint(&self, h: &mut FastHasher) {
        match self {
            Self::Glyph {
                family,
                glyph,
                fill,
                weight,
                grade,
            } => {
                h.write_u8(0);
                family.fingerprint(h);
                glyph.fingerprint(h);
                fill.fingerprint(h);
                weight.fingerprint(h);
                grade.fingerprint(h);
            }
            Self::Custom(draw) => {
                h.write_u8(1);
                h.write_usize(*draw as usize);
            }
        }
    }
}

impl<A: Fingerprint, B: Fingerprint> Fingerprint for (A, B) {
    fn fingerprint(&self, h: &mut FastHasher) {
        self.0.fingerprint(h);
        self.1.fingerprint(h);
    }
}

impl<A: Fingerprint, B: Fingerprint, C: Fingerprint> Fingerprint for (A, B, C) {
    fn fingerprint(&self, h: &mut FastHasher) {
        self.0.fingerprint(h);
        self.1.fingerprint(h);
        self.2.fingerprint(h);
    }
}

impl<A: Fingerprint, B: Fingerprint, C: Fingerprint, D: Fingerprint> Fingerprint for (A, B, C, D) {
    fn fingerprint(&self, h: &mut FastHasher) {
        self.0.fingerprint(h);
        self.1.fingerprint(h);
        self.2.fingerprint(h);
        self.3.fingerprint(h);
    }
}

/// Hash the listed expressions into one fingerprint.
///
/// The list is the contract: anything `decl` reads that is missing here is a change no
/// frame will ever show.
#[macro_export]
macro_rules! fingerprint {
    ($($e:expr),* $(,)?) => {{
        use $crate::fingerprint::Fingerprint as _;
        use core::hash::Hasher as _;
        let mut h = ui_core::collections::FastHasher::default();
        $( $e.fingerprint(&mut h); )*
        h.finish()
    }};
}
