//! Font bytes with the `FontRef` over them parsed once.

use swash::FontRef;
use ui_text::{FontBlob, Variations};

/// The variation settings an instance sets: weight first, then the extra axes.
///
/// Shaping, advance correction and rasterization all read this one list, so an instance
/// cannot mean different things to the pen and to the outline.
pub(crate) fn variation_settings(weight: Option<f32>, axes: Variations) -> Vec<([u8; 4], f32)> {
    let mut out = Vec::with_capacity(4);
    if let Some(weight) = weight {
        out.push((*b"wght", weight));
    }
    for (tag, value) in [
        (*b"opsz", axes.optical_size),
        (*b"FILL", axes.fill),
        (*b"GRAD", axes.grade),
    ] {
        if let Some(value) = value {
            out.push((tag, value));
        }
    }
    out
}

/// The normalized coordinates `font` sits at for `weight` and `axes`.
///
/// Empty when the face is static or nothing varies: a default location, not a missing one.
pub(crate) fn instance_coords(
    font: &skrifa::FontRef<'_>,
    weight: Option<f32>,
    axes: Variations,
) -> Vec<skrifa::instance::NormalizedCoord> {
    use skrifa::MetadataProvider as _;
    let settings = variation_settings(weight, axes);
    if settings.is_empty() {
        return Vec::new();
    }
    let axes = font.axes();
    if axes.is_empty() {
        return Vec::new();
    }
    axes.location(
        settings
            .into_iter()
            .map(|(tag, value)| (skrifa::Tag::new(&tag), value)),
    )
    .coords()
    .to_vec()
}

// swash keys its shaping and scaling plan caches on a per-`FontRef` id minted from a
// global counter, so parsing the font again on every call misses every cache entry and
// rebuilds the plan each time. This cell keeps one `FontRef` alive for the face.
self_cell::self_cell!(
    /// One font file, parsed once by both shaping and outline libraries.
    pub struct FaceData {
        owner: FontBlob,
        #[covariant]
        dependent: Faces,
    }
);

/// The two parsed views of one font file.
pub struct Faces<'a> {
    pub swash: FontRef<'a>,
    pub skrifa: skrifa::FontRef<'a>,
}

impl FaceData {
    /// Parse `data` at `index`, once per face.
    pub fn parse(data: impl Into<FontBlob>, index: usize) -> Option<Self> {
        Self::try_new(data.into(), |bytes| {
            let swash = FontRef::from_index(bytes, index).ok_or(())?;
            let skrifa = skrifa::FontRef::from_index(bytes, index as u32).map_err(|_| ())?;
            Ok::<_, ()>(Faces { swash, skrifa })
        })
        .ok()
    }

    /// The swash view; cheap, because nothing is re-parsed.
    pub fn font(&self) -> FontRef<'_> {
        self.borrow_dependent().swash
    }

    /// The skrifa view, for outlines and metrics.
    pub fn outline_font(&self) -> &skrifa::FontRef<'_> {
        &self.borrow_dependent().skrifa
    }

    /// The bytes the fonts borrow from.
    pub fn bytes(&self) -> &FontBlob {
        self.borrow_owner()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_face_keeps_one_font_key_so_the_plan_cache_holds() {
        // swash keys its plan caches on the `FontRef` key, minted per parse: a second parse
        // misses them all and rebuilds the plan, which made shaping ~100x slower.
        let Some(data) = crate::tests::tests_support::sans_face() else {
            return;
        };
        let face = FaceData::parse(data, 0).expect("a system face parses");
        assert_eq!(
            face.font().key.value(),
            face.font().key.value(),
            "every call must hand swash the same key"
        );
        assert_eq!(face.font().data.as_ptr(), face.bytes().as_ptr());
    }
}
