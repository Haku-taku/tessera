//! The trait a platform must implement, plus the types it exchanges.

use std::sync::Arc;

/// Which generic family a request maps to, plus the types a style carries, live in
/// `ui-core` so `TextStyle` can name them without a cycle.
pub use ui_core::text::GenericFamily;
pub use ui_core::{Features, Variations, Weight};

/// ISO 15924 script tag in its 4-byte form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Script(pub [u8; 4]);

impl Script {
    pub const LATIN: Self = Self(*b"Latn");
    pub const HAN: Self = Self(*b"Hani");
    pub const ARABIC: Self = Self(*b"Arab");
    pub const HEBREW: Self = Self(*b"Hebr");
    pub const CYRILLIC: Self = Self(*b"Cyrl");
    pub const GREEK: Self = Self(*b"Grek");
    pub const HIRAGANA: Self = Self(*b"Hira");
    pub const HANGUL: Self = Self(*b"Hang");
    pub const THAI: Self = Self(*b"Thai");
    pub const DEVANAGARI: Self = Self(*b"Deva");

    pub fn tag(self) -> String {
        String::from_utf8_lossy(&self.0).into_owned()
    }

    /// The script a code point belongs to, or `None` when it has none of its own: digits,
    /// punctuation, marks and symbols take the script of the text around them.
    pub fn of_char(c: char) -> Option<Self> {
        let script = match c as u32 {
            0x0041..=0x005A | 0x0061..=0x007A => Self::LATIN,
            0x00C0..=0x024F | 0x1E00..=0x1EFF => Self::LATIN,
            0x0370..=0x03FF | 0x1F00..=0x1FFF => Self::GREEK,
            0x0400..=0x052F => Self::CYRILLIC,
            0x0590..=0x05FF => Self::HEBREW,
            0x0600..=0x06FF | 0x0750..=0x077F | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => Self::ARABIC,
            0x0900..=0x097F => Self::DEVANAGARI,
            0x0E00..=0x0E7F => Self::THAI,
            0x3040..=0x309F => Self::HIRAGANA,
            0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF => Self::HAN,
            0x1100..=0x11FF | 0xAC00..=0xD7AF => Self::HANGUL,
            _ => return None,
        };
        Some(script)
    }

    /// Cheap script guess from one character; unknown ranges read as Latin.
    pub fn for_char(c: char) -> Self {
        Self::of_char(c).unwrap_or(Self::LATIN)
    }

    /// Dominant script of a run: the first non-ASCII character decides.
    pub fn dominant(text: &str) -> Self {
        text.chars()
            .find(|c| !c.is_ascii())
            .map(Self::for_char)
            .unwrap_or(Self::LATIN)
    }
}

/// What the caller wants, before a font has been chosen.
#[derive(Clone, Debug, PartialEq)]
pub struct FontRequest {
    /// Named families in priority order; empty means "use the generic family".
    pub families: Vec<String>,
    pub generic: GenericFamily,
    pub weight: Weight,
    pub italic: bool,
    /// Used to pick a fallback chain when no named family covers the text.
    pub script: Script,
}

impl FontRequest {
    pub fn new(generic: GenericFamily) -> Self {
        Self {
            families: Vec::new(),
            generic,
            weight: Weight::REGULAR,
            italic: false,
            script: Script::LATIN,
        }
    }

    pub fn family(mut self, name: impl Into<String>) -> Self {
        self.families.push(name.into());
        self
    }

    pub fn weight(mut self, w: Weight) -> Self {
        self.weight = w;
        self
    }

    pub fn script(mut self, s: Script) -> Self {
        self.script = s;
        self
    }
}

/// Identifies one face inside a provider's namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FaceId(pub u32);

/// Metrics needed by layout, already scaled so `units_per_em` maps to `size`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
}

impl FaceMetrics {
    pub fn line_height(&self) -> f32 {
        (self.ascent - self.descent + self.line_gap).max(0.0)
    }
}

/// What a platform must supply. One implementation per platform.
pub trait FontProvider {
    /// Stable name for diagnostics and tests.
    fn name(&self) -> &'static str;

    /// Every family name the platform exposes, for font pickers.
    fn family_names(&self) -> Vec<String>;

    /// Resolve a request to a face, or `None` when nothing matches.
    fn resolve(&self, request: &FontRequest) -> Option<FaceId>;

    /// Faces that may cover `script`, best first.
    fn fallback_chain(&self, script: Script) -> Vec<FaceId>;

    /// Whether `face` has a glyph for `c`.
    fn has_glyph(&self, face: FaceId, c: char) -> bool;

    /// Metrics of `face` at size `1.0`.
    fn metrics(&self, face: FaceId) -> FaceMetrics;

    /// Single-line advance width of `text` in `face` at `size`.
    ///
    /// The width is the face's own: kerning, ligatures and a variable font's weight
    /// instance are included, so a bold face measures as its designer widened it. A
    /// provider with no shaper can only approximate, and says so.
    fn advance(&self, face: FaceId, text: &str, size: f32) -> f32;

    /// Weight instance to apply, for a face taken from a variable font.
    ///
    /// `None` for a static face: the bytes already are that weight.
    fn face_variation(&self, face: FaceId) -> Option<f32> {
        let _ = face;
        None
    }

    /// Index of this face within its font file.
    ///
    /// A collection or variable font holds several faces in one file; parsing byte 0
    /// always yields the first face, so the index must travel with the bytes.
    fn face_index(&self, face: FaceId) -> usize {
        let _ = face;
        0
    }

    /// Extra variable-font axes of a face instance; `NONE` for a static face.
    fn face_axes(&self, face: FaceId) -> Variations {
        let _ = face;
        Variations::NONE
    }

    /// A handle to `face` at `axes`. A variable-font provider interns the instance, so
    /// every downstream cache keyed on `FaceId` stays correct with no new dimension.
    fn instance(&mut self, face: FaceId, axes: Variations) -> FaceId {
        let _ = axes;
        face
    }

    /// Register font bytes under `name`; false when the provider takes no fonts or the
    /// bytes do not parse. Registered families win over system ones of the same name.
    fn register_family(&mut self, name: &str, data: Arc<[u8]>) -> bool {
        let _ = (name, data);
        false
    }

    /// Raw font bytes, for a rasterizer to parse. `None` if the provider has none.
    fn face_data(&self, face: FaceId) -> Option<FontBlob> {
        let _ = face;
        None
    }
}

/// One font file's bytes behind a shared handle: a mapped file or a heap copy.
///
/// Keeping the source's own blob is what avoids copying a whole font into the heap; a
/// mapped CJK face costs only the pages actually read, a copied one costs its full size.
#[derive(Clone)]
pub struct FontBlob(Arc<dyn AsRef<[u8]> + Send + Sync>);

impl FontBlob {
    /// Wrap any owner that derefs to the file's bytes.
    pub fn new<T: AsRef<[u8]> + Send + Sync + 'static>(owner: T) -> Self {
        Self(Arc::new(owner))
    }

    pub fn bytes(&self) -> &[u8] {
        self.0.as_ref().as_ref()
    }
}

impl AsRef<[u8]> for FontBlob {
    fn as_ref(&self) -> &[u8] {
        self.bytes()
    }
}

impl std::ops::Deref for FontBlob {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.bytes()
    }
}

impl From<Arc<[u8]>> for FontBlob {
    fn from(data: Arc<[u8]>) -> Self {
        Self(Arc::new(data))
    }
}

/// A provider backed by a fixed face list: tests, and bundling one font.
#[derive(Clone, Debug)]
pub struct StaticProvider {
    name: &'static str,
    faces: Vec<StaticFace>,
}

/// One face of a `StaticProvider`.
#[derive(Clone, Debug)]
pub struct StaticFace {
    pub family: String,
    pub advance_ratio: f32,
    pub metrics: FaceMetrics,
    /// Characters this face covers; `None` means "everything".
    pub coverage: Option<Vec<char>>,
}

impl StaticProvider {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            faces: Vec::new(),
        }
    }

    pub fn with_face(mut self, family: &str, advance_ratio: f32) -> Self {
        self.faces.push(StaticFace {
            family: family.to_string(),
            advance_ratio,
            metrics: FaceMetrics {
                ascent: 0.8,
                descent: -0.2,
                line_gap: 0.0,
            },
            coverage: None,
        });
        self
    }

    /// Add a face that covers only `coverage`, so fallback can be exercised.
    pub fn with_partial_face(
        mut self,
        family: &str,
        advance_ratio: f32,
        coverage: &[char],
    ) -> Self {
        self.faces.push(StaticFace {
            family: family.to_string(),
            advance_ratio,
            metrics: FaceMetrics {
                ascent: 0.8,
                descent: -0.2,
                line_gap: 0.0,
            },
            coverage: Some(coverage.to_vec()),
        });
        self
    }

    pub fn face_count(&self) -> usize {
        self.faces.len()
    }
}

impl FontProvider for StaticProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    fn family_names(&self) -> Vec<String> {
        self.faces.iter().map(|f| f.family.clone()).collect()
    }

    fn resolve(&self, request: &FontRequest) -> Option<FaceId> {
        for wanted in &request.families {
            if let Some(i) = self.faces.iter().position(|f| &f.family == wanted) {
                return Some(FaceId(i as u32));
            }
        }
        if self.faces.is_empty() {
            None
        } else {
            Some(FaceId(0))
        }
    }

    fn fallback_chain(&self, _script: Script) -> Vec<FaceId> {
        (0..self.faces.len() as u32).map(FaceId).collect()
    }

    fn has_glyph(&self, face: FaceId, c: char) -> bool {
        match self.faces.get(face.0 as usize) {
            Some(f) => match &f.coverage {
                Some(list) => list.contains(&c),
                None => true,
            },
            None => false,
        }
    }

    fn metrics(&self, face: FaceId) -> FaceMetrics {
        self.faces
            .get(face.0 as usize)
            .map(|f| f.metrics)
            .unwrap_or(FaceMetrics {
                ascent: 0.8,
                descent: -0.2,
                line_gap: 0.0,
            })
    }

    fn advance(&self, face: FaceId, text: &str, size: f32) -> f32 {
        // A test double has no outlines to shape: every face advances at its own ratio.
        let ratio = self
            .faces
            .get(face.0 as usize)
            .map(|f| f.advance_ratio)
            .unwrap_or(0.6);
        ratio * size * text.chars().count() as f32
    }
}
