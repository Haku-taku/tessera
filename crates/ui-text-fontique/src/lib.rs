//! Platform-native font provider backed by `fontique`.
//!
//! `fontique` owns the per-OS calls (fontconfig on Linux, DirectWrite on Windows,
//! CoreText on macOS); this crate only translates its shapes into our
//! `FontProvider` trait, so replacing `fontique` touches one file.

pub mod face;
pub mod glyph;
pub mod shape;

pub use face::FaceData;
pub use glyph::{
    ColorBitmap, DecorationMetrics, GlyphBitmap, GlyphBlit, GlyphCache, ParsedFace, RealMetrics,
    Slot,
};
pub use shape::{
    PlacedGlyph, ShapedGlyph, ShapedRun, Shaper, VisualRun, place_run, place_run_scaled,
};

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use fontique::{Collection, CollectionOptions, FallbackKey, FamilyId, Script as FqScript};
use ui_text::{
    FaceId, FaceMetrics, FontBlob, FontProvider, FontRequest, GenericFamily, Script, Variations,
};

/// A provider that enumerates the OS font stack through `fontique`.
pub struct FontiqueProvider {
    inner: Mutex<Collection>,
    /// The faces of the families asked for so far.
    admitted: Mutex<Admitted>,
    /// Family name -> id. Names come with the scan; faces do not, so this is the whole
    /// list a picker shows without parsing a single font.
    names: Mutex<HashMap<String, FamilyId>>,
    /// Cached raw font bytes; loading hits the disk otherwise.
    blobs: Mutex<HashMap<FaceId, FontBlob>>,
    /// (face, character) -> whether the face's cmap covers it. The probe parses the table
    /// every time otherwise, and fallback asks once per character per candidate face.
    glyph_probes: Mutex<HashMap<(FaceId, char), bool>>,
    /// Shapers `advance` alone uses. Layout keeps its own per face; this map makes a
    /// convenience measurement cost one plan build per face, not one per call.
    advance_shapers: Mutex<HashMap<FaceId, crate::shape::Shaper>>,
    /// The bytes each registered name was admitted with. The provider is shared by every
    /// surface on its thread and fontique never gives a family back, so the same bundled
    /// font handed over again must not be admitted again: a picker opened ten times would
    /// otherwise hold ten copies of its icon font.
    registered: Mutex<HashMap<String, FontBlob>>,
    /// Counts internal family names for bytes that replace a registered one; fontique can
    /// only append a family, never drop and re-add one.
    next_serial: u64,
    /// Whether the OS font stack was actually available.
    system: bool,
}

/// The face tables, filled one family at a time.
///
/// A system holds hundreds of families and a program uses a handful. Parsing every face's
/// metadata up front costs 211 ms (release, 180 families, 592 faces on this machine), while
/// the scan that yields the names costs 15 ms. A family is parsed the first time something
/// asks for it, so an app that draws with three families pays for three.
#[derive(Default)]
struct Admitted {
    faces: Vec<FaceSlot>,
    /// Family -> the slot of its own default face, which is what a fallback chain wants.
    first: HashMap<FamilyId, u32>,
    /// (family, index within the family) -> the face's default slot.
    by_family_index: HashMap<(FamilyId, usize), u32>,
    /// (family, index, weight) -> the variable-font instance slot for that weight.
    by_variation: HashMap<(FamilyId, usize, u16), u32>,
    /// (family, index, weight, extra axes at 1/64) -> the instance slot for that position.
    by_axes: HashMap<(FamilyId, usize, u16, [Option<i32>; 3]), u32>,
    /// Families already admitted, or found to hold no face.
    families: HashSet<FamilyId>,
}

/// What a [`FaceId`] says about its face without the family name beside it.
#[derive(Clone, Copy, Debug)]
struct FacePosition {
    family: FamilyId,
    font_index: usize,
    variation: Option<f32>,
    axes: Variations,
}

#[derive(Clone, Debug)]
struct FaceSlot {
    family: FamilyId,
    family_name: String,
    /// Which font of the family this face is. A family holds Regular, Bold, Italic...
    font_index: usize,
    /// Weight instance for a variable font; `None` for a static face.
    ///
    /// A variable font is one face with a continuous axis, so "Bold" is not a different
    /// font — it is the same file at a different `wght`. Encoding the instance in
    /// `FaceId` keeps every downstream cache correct without new key dimensions.
    variation: Option<f32>,
    /// The axes beyond weight this instance sits at; `NONE` for a face's own defaults.
    axes: Variations,
}

/// `wght` axis tag, the only axis a weight request can drive.
const VARIATION_WEIGHT_TAG: skrifa::Tag = skrifa::Tag::new(b"wght");

/// Weight instances to register for a variable font, clamped to the axis range.
///
/// A ladder rather than a single value: a request for 600 must land on something
/// between Regular and Bold instead of snapping to one end.
fn weight_ladder(min: f32, max: f32) -> Vec<u16> {
    let mut out = Vec::new();
    for w in (100..=900).step_by(100) {
        let w = w as f32;
        if w >= min && w <= max {
            out.push(w as u16);
        }
    }
    // Always include the axis ends so an extreme request still has a target.
    for w in [min, max] {
        let w = w.round();
        if (100.0..=900.0).contains(&w) && !out.contains(&(w as u16)) {
            out.push(w as u16);
        }
    }
    out.sort_unstable();
    out
}

/// Generic family mapping, kept separate so tests can pin it.
/// A sized wrapper, because `Arc<[u8]>` cannot coerce straight to a trait object.
struct FontBytes(Arc<[u8]>);

impl AsRef<[u8]> for FontBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Snapshot one family's faces, plus a weight-instance ladder for each variable font.
///
/// A variable font is one face with a continuous weight axis. Registering a ladder of
/// instances makes "Bold" a distinct `FaceId`, which keeps the shaper, rasterizer and
/// atlas keyed correctly with no new key dimension.
fn admit_faces(
    faces: &mut Vec<FaceSlot>,
    by_family_index: &mut HashMap<(FamilyId, usize), u32>,
    by_variation: &mut HashMap<(FamilyId, usize, u16), u32>,
    family: FamilyId,
    name: &str,
    fonts: &[fontique::FontInfo],
    default: usize,
) -> u32 {
    let mut default_slot = 0;
    for (index, font) in fonts.iter().enumerate() {
        let slot = faces.len() as u32;
        by_family_index.insert((family, index), slot);
        if index == default {
            default_slot = slot;
        }
        faces.push(FaceSlot {
            family,
            family_name: name.to_string(),
            font_index: index,
            variation: None,
            axes: Variations::NONE,
        });
        if !font.has_weight_axis() {
            continue;
        }
        let Some(axis) = font.axes().iter().find(|a| a.tag == VARIATION_WEIGHT_TAG) else {
            continue;
        };
        for weight in weight_ladder(axis.min, axis.max) {
            let slot = faces.len() as u32;
            by_variation.insert((family, index, weight), slot);
            faces.push(FaceSlot {
                family,
                family_name: name.to_string(),
                font_index: index,
                variation: Some(weight as f32),
                axes: Variations::NONE,
            });
        }
    }
    default_slot
}

fn generic_of(g: GenericFamily) -> fontique::GenericFamily {
    match g {
        // `SystemUi` is the desktop's own face — Segoe UI, the system font, the configured
        // default — and folding it into the sans slot served Arial to every app on Windows.
        GenericFamily::SystemUi => fontique::GenericFamily::SystemUi,
        GenericFamily::SansSerif => fontique::GenericFamily::SansSerif,
        GenericFamily::Serif => fontique::GenericFamily::Serif,
        GenericFamily::Monospace => fontique::GenericFamily::Monospace,
    }
}

fn fq_script(s: Script) -> FqScript {
    FqScript::from_bytes(s.0)
}

impl FontiqueProvider {
    /// Build a provider over the OS font stack.
    pub fn new_system() -> Self {
        let collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: true,
        });
        Self::from_collection(collection)
    }

    /// Build a provider with no system fonts (for deterministic tests).
    pub fn new_isolated() -> Self {
        let collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        Self::from_collection(collection)
    }

    fn from_collection(mut collection: Collection) -> Self {
        // Names only. `family()` parses a family's font files, and the scan that answers
        // `family_ids()` does not: everything else waits until a family is asked for.
        let mut names = HashMap::new();
        let ids: Vec<FamilyId> = collection.family_ids().collect();
        for id in ids {
            if let Some(name) = collection.family_name(id) {
                names.entry(name.to_string()).or_insert(id);
            }
        }
        let system = !names.is_empty();
        Self {
            inner: Mutex::new(collection),
            admitted: Mutex::new(Admitted::default()),
            names: Mutex::new(names),
            blobs: Mutex::new(HashMap::new()),
            glyph_probes: Mutex::new(HashMap::new()),
            advance_shapers: Mutex::new(HashMap::new()),
            registered: Mutex::new(HashMap::new()),
            next_serial: 0,
            system,
        }
    }

    /// Whether any family was found at all.
    pub fn has_system_fonts(&self) -> bool {
        self.system
    }

    /// Parse `family`'s faces, once. Every lookup path calls this before reading a slot.
    fn admit(&self, family: FamilyId) {
        let Ok(mut admitted) = self.admitted.lock() else {
            return;
        };
        if admitted.families.contains(&family) {
            return;
        }
        // Parsing under the admitted lock keeps one family from being parsed twice; the
        // collection lock is taken second, and never held while the admitted lock is free.
        let parsed = {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };
            let Some(name) = inner.family_name(family).map(str::to_string) else {
                admitted.families.insert(family);
                return;
            };
            // `family()` is the parse: this is the 0.36 ms per face the eager build paid
            // for every installed family.
            let Some(info) = inner.family(family) else {
                admitted.families.insert(family);
                return;
            };
            let fonts: Vec<fontique::FontInfo> = info.fonts().to_vec();
            let default = info.default_font_index();
            (name, fonts, default)
        };
        let (name, fonts, default) = parsed;
        let Admitted {
            faces,
            by_family_index,
            by_variation,
            first,
            families,
            ..
        } = &mut *admitted;
        let slot = admit_faces(
            faces,
            by_family_index,
            by_variation,
            family,
            &name,
            &fonts,
            default,
        );
        first.entry(family).or_insert(slot);
        families.insert(family);
    }

    /// Display name of the face, for diagnostics.
    pub fn face_family_name(&self, face: FaceId) -> Option<String> {
        self.admitted
            .lock()
            .ok()?
            .faces
            .get(face.0 as usize)
            .map(|slot| slot.family_name.clone())
    }

    fn position(&self, face: FaceId) -> Option<FacePosition> {
        let admitted = self.admitted.lock().ok()?;
        admitted
            .faces
            .get(face.0 as usize)
            .map(|slot| FacePosition {
                family: slot.family,
                font_index: slot.font_index,
                variation: slot.variation,
                axes: slot.axes,
            })
    }

    /// A handle to `face` at `axes`, interning a slot the first time that position is asked
    /// for. The axes are quantised to 1/64 first, so an animated `FILL` yields at most 65
    /// instances rather than one per frame.
    pub fn instance_axes(&mut self, face: FaceId, axes: Variations) -> FaceId {
        if axes == Variations::NONE {
            return face;
        }
        let Ok(mut admitted) = self.admitted.lock() else {
            return face;
        };
        let Some(slot) = admitted.faces.get(face.0 as usize).cloned() else {
            return face;
        };
        let weight = slot.variation.map(|w| w as u16).unwrap_or(0);
        let quantized = axes.quantized();
        let key = (slot.family, slot.font_index, weight, quantized);
        if let Some(&existing) = admitted.by_axes.get(&key) {
            return FaceId(existing);
        }
        let merged = Variations {
            optical_size: quantized[0].map(|q| q as f32 / 64.0),
            fill: quantized[1].map(|q| q as f32 / 64.0),
            grade: quantized[2].map(|q| q as f32 / 64.0),
        };
        let id = FaceId(admitted.faces.len() as u32);
        admitted.faces.push(FaceSlot {
            family: slot.family,
            family_name: slot.family_name,
            font_index: slot.font_index,
            variation: slot.variation,
            axes: merged,
        });
        admitted.by_axes.insert(key, id.0);
        drop(admitted);
        // The instance is the base face's own file, so prime the byte cache: a registered
        // family has no fontique family entry to load from later.
        if let Some(data) = self.face_data(face)
            && let Ok(mut blobs) = self.blobs.lock()
        {
            blobs.insert(id, data);
        }
        id
    }

    /// Register font bytes under `name`; the family then resolves like a system one.
    ///
    /// Registering the same bytes under a name that already has them is a no-op: the shared
    /// provider lives as long as its thread, and fontique holds every admitted family for
    /// good. Different bytes for a known name get an internal family name of their own, so
    /// the new face takes effect while the old one is simply never asked for again.
    pub fn register(&mut self, name: &str, data: Arc<[u8]>) -> bool {
        let replaced = {
            let registered = match self.registered.lock() {
                Ok(registered) => registered,
                Err(_) => return false,
            };
            match registered.get(name) {
                Some(existing) if existing.bytes() == &data[..] => return true,
                Some(_) => true,
                None => false,
            }
        };
        let family_name = if replaced {
            self.next_serial += 1;
            format!("{name}#{}", self.next_serial)
        } else {
            name.to_string()
        };
        if let Ok(mut registered) = self.registered.lock() {
            registered.insert(name.to_string(), FontBlob::from(Arc::clone(&data)));
        }
        let blob: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(FontBytes(Arc::clone(&data)));
        let added = {
            let Ok(mut inner) = self.inner.lock() else {
                return false;
            };
            inner.register_fonts(
                fontique::Blob::new(blob),
                Some(fontique::FontInfoOverride {
                    family_name: Some(&family_name),
                    ..Default::default()
                }),
            )
        };
        let Some((family, fonts)) = added.into_iter().next() else {
            return false;
        };
        let Ok(mut admitted) = self.admitted.lock() else {
            return false;
        };
        let before = admitted.faces.len();
        let Admitted {
            faces,
            by_family_index,
            by_variation,
            first,
            families,
            ..
        } = &mut *admitted;
        let slot = admit_faces(
            faces,
            by_family_index,
            by_variation,
            family,
            name,
            &fonts,
            0,
        );
        first.entry(family).or_insert(slot);
        families.insert(family);
        let after = admitted.faces.len();
        drop(admitted);
        // A registered family must win over a system family of the same name.
        if let Ok(mut names) = self.names.lock() {
            names.insert(name.to_string(), family);
        }
        if let Ok(mut blobs) = self.blobs.lock() {
            let data = FontBlob::from(data);
            for slot in before..after {
                blobs.insert(FaceId(slot as u32), data.clone());
            }
        }
        self.system = true;
        true
    }

    /// Exact family lookup, with no alias substitution.
    fn exact_family(&self, name: &str) -> Option<FamilyId> {
        let names = self.names.lock().ok()?;
        names.get(name).copied()
    }

    /// The face of a family closest to the request, as a stable `FaceId`.
    ///
    /// Matching is fontique's, not ours: it scores width, style and weight across the
    /// family's faces, which is what a hand-written "first face" lookup silently dropped.
    fn pick_face(&self, family: FamilyId, request: &FontRequest) -> Option<FaceId> {
        let index = {
            let mut inner = self.inner.lock().ok()?;
            let info = inner.family(family)?;
            let weight = fontique::FontWeight::new(request.weight.0.clamp(1, 1000) as f32);
            let style = if request.italic {
                fontique::FontStyle::Italic
            } else {
                fontique::FontStyle::Normal
            };
            // `FontRequest` carries no width, so match on the normal width.
            info.match_index(fontique::FontWidth::NORMAL, style, weight, false)?
        };
        self.admit(family);
        let admitted = self.admitted.lock().ok()?;
        // A variable font answers with the same face for every weight; the instance
        // ladder is what actually makes Bold different.
        let requested = (request.weight.0).clamp(1, 1000);
        if let Some(slot) = admitted.by_variation.get(&(family, index, requested)) {
            return Some(FaceId(*slot));
        }
        // Nearest ladder rung, so 550 still lands on 500 or 600 instead of Regular.
        let mut best: Option<(u16, u32)> = None;
        for ((fam, idx, w), slot) in admitted.by_variation.iter() {
            if *fam != family || *idx != index {
                continue;
            }
            let distance = w.abs_diff(requested);
            if best
                .map(|(bw, _)| distance < bw.abs_diff(requested))
                .unwrap_or(true)
            {
                best = Some((*w, *slot));
            }
        }
        if let Some((_, slot)) = best {
            return Some(FaceId(slot));
        }
        admitted
            .by_family_index
            .get(&(family, index))
            .map(|i| FaceId(*i))
    }

    /// The platform's face for a generic family, matched against the request.
    ///
    /// This path carries the weight too: a request with no named family is the common
    /// case, so ignoring attributes here would make bold text impossible by default.
    fn generic_face(&self, generic: GenericFamily, request: &FontRequest) -> Option<FaceId> {
        let family = {
            let Ok(mut inner) = self.inner.lock() else {
                return None;
            };
            // The block ends the borrow: the iterator borrows `inner` mutably.
            inner.generic_families(generic_of(generic)).next()
        };
        if let Some(id) = family
            && let Some(face) = self.pick_face(id, request)
        {
            return Some(face);
        }
        self.fallback_for_script(Script::LATIN).first().copied()
    }

    fn fallback_for_script(&self, script: Script) -> Vec<FaceId> {
        let ids: Vec<FamilyId> = {
            let Ok(mut inner) = self.inner.lock() else {
                return Vec::new();
            };
            inner
                .fallback_families(FallbackKey::new(fq_script(script), None))
                .take(16)
                .collect()
        };
        // Only the script's own candidates are parsed, not the whole installation.
        let mut chain = Vec::with_capacity(ids.len());
        for id in ids {
            self.admit(id);
            let Ok(admitted) = self.admitted.lock() else {
                return chain;
            };
            if let Some(&slot) = admitted.first.get(&id) {
                chain.push(FaceId(slot));
            }
        }
        chain
    }
}

impl FontProvider for FontiqueProvider {
    fn name(&self) -> &'static str {
        if self.system {
            "fontique-system"
        } else {
            "fontique-isolated"
        }
    }

    fn family_names(&self) -> Vec<String> {
        // The scan's own names: a picker listing families must not parse a single font.
        let Ok(names) = self.names.lock() else {
            return Vec::new();
        };
        let mut v: Vec<String> = names.keys().cloned().collect();
        v.sort();
        v
    }

    fn resolve(&self, request: &FontRequest) -> Option<FaceId> {
        // Named families are an exact query: if none match, report absence rather
        // than silently substituting the generic face, which would hide typos.
        if !request.families.is_empty() {
            for wanted in &request.families {
                if let Some(id) = self.exact_family(wanted)
                    && let Some(face) = self.pick_face(id, request)
                {
                    return Some(face);
                }
            }
            return None;
        }
        // The platform's own choice for the generic family.
        self.generic_face(request.generic, request)
    }

    fn fallback_chain(&self, script: Script) -> Vec<FaceId> {
        self.fallback_for_script(script)
    }

    fn has_glyph(&self, face: FaceId, c: char) -> bool {
        let key = (face, c);
        if let Ok(cache) = self.glyph_probes.lock()
            && let Some(hit) = cache.get(&key)
        {
            return *hit;
        }
        // Probe the face's actual font bytes; a family can hold several faces and
        // only some cover the character.
        let covered = match self.face_data(face) {
            Some(data) => charmap_has_glyph(&data, c),
            None => false,
        };
        if let Ok(mut cache) = self.glyph_probes.lock() {
            if cache.len() >= 8192 {
                cache.clear();
            }
            cache.insert(key, covered);
        }
        covered
    }

    fn metrics(&self, face: FaceId) -> FaceMetrics {
        // Until a rasterizer parses the real font, report representative metrics.
        let _ = self.position(face);
        FaceMetrics {
            ascent: 0.8,
            descent: -0.2,
            line_gap: 0.0,
        }
    }

    fn advance(&self, face: FaceId, text: &str, size: f32) -> f32 {
        // Shaped, not counted: kerning, ligatures and the face's own weight instance are
        // what make a bold run wider than a regular one. The shaper is kept per face
        // because a fresh one misses swash's plan cache and rebuilds it on every call.
        let Ok(mut shapers) = self.advance_shapers.lock() else {
            return 0.0;
        };
        if !shapers.contains_key(&face) {
            let Some(data) = self.face_data(face) else {
                return 0.0;
            };
            let Some(shaper) = crate::shape::Shaper::new_with_axes(
                data,
                self.face_index(face),
                self.face_variation(face),
                self.face_axes(face),
            ) else {
                return 0.0;
            };
            shapers.insert(face, shaper);
        }
        let Some(shaper) = shapers.get_mut(&face) else {
            return 0.0;
        };
        shaper.shape(text, size, Script::dominant(text)).width
    }

    fn face_variation(&self, face: FaceId) -> Option<f32> {
        self.position(face).and_then(|slot| slot.variation)
    }

    fn face_axes(&self, face: FaceId) -> Variations {
        self.position(face)
            .map_or(Variations::NONE, |slot| slot.axes)
    }

    fn instance(&mut self, face: FaceId, axes: Variations) -> FaceId {
        self.instance_axes(face, axes)
    }

    fn register_family(&mut self, name: &str, data: Arc<[u8]>) -> bool {
        self.register(name, data)
    }

    fn face_index(&self, face: FaceId) -> usize {
        // Two different indices, and they are not interchangeable: the slot holds the
        // face's position *within its family*, while a parser needs its position *within
        // the file* (a TTC packs several faces into one blob). Confusing them silently
        // loads the wrong face.
        let Some(slot) = self.position(face) else {
            return 0;
        };
        let Ok(mut inner) = self.inner.lock() else {
            return 0;
        };
        let Some(info) = inner.family(slot.family) else {
            return 0;
        };
        info.fonts()
            .get(slot.font_index)
            .map_or(0, |font| font.index() as usize)
    }

    fn face_data(&self, face: FaceId) -> Option<FontBlob> {
        if let Ok(cache) = self.blobs.lock()
            && let Some(hit) = cache.get(&face)
        {
            return Some(hit.clone());
        }
        let slot = self.position(face)?;
        let blob = {
            let mut inner = self.inner.lock().ok()?;
            let info = inner.family(slot.family)?;
            // The face's own index: `first()` would hand back Regular for every id,
            // so a resolved Bold would still rasterise Regular outlines.
            let font = info.fonts().get(slot.font_index)?;
            let mut cache = fontique::SourceCache::new_shared();
            // fontique maps a file source; keeping its blob avoids copying the whole file.
            font.load(Some(&mut cache))?
        };
        let blob = FontBlob::new(blob);
        if let Ok(mut cache) = self.blobs.lock() {
            cache.insert(face, blob.clone());
        }
        Some(blob)
    }
}

/// Minimal `cmap` probe: does this font cover `c`?
///
/// Scans the table directory for `cmap` and checks format 4 / 12 subtables.
fn charmap_has_glyph(data: &[u8], c: char) -> bool {
    let Some(cmap) = find_table(data, b"cmap") else {
        return false;
    };
    let num = get_u16(cmap, 2).unwrap_or(0) as usize;
    let mut best: Option<(u8, usize)> = None;
    for i in 0..num.min(64) {
        let base = 4 + i * 8;
        let (Some(pid), Some(eid), Some(off)) = (
            get_u16(cmap, base),
            get_u16(cmap, base + 2),
            get_u32(cmap, base + 4),
        ) else {
            continue;
        };
        // Prefer Unicode subtables: platform 0, or Windows (3) with UCS-4/UCS-2.
        if pid != 0 && !(pid == 3 && (eid == 1 || eid == 10)) {
            continue;
        }
        let offset = off as usize;
        // A font may list several Unicode subtables; keep the one that can answer for the
        // most code points, not whichever record happened to come last. A format 4 table
        // cannot report a character above the BMP at all.
        let Some(format) = get_u16(cmap, offset) else {
            continue;
        };
        let rank = match format {
            12 => 2,
            4 => 1,
            _ => continue,
        };
        if best.is_none_or(|(seen, _)| rank > seen) {
            best = Some((rank, offset));
        }
    }
    let Some((rank, offset)) = best else {
        return false;
    };
    let cp = c as u32;
    match rank {
        1 => {
            if cp > 0xFFFF {
                return false;
            }
            let Some(seg_count) = get_u16(cmap, offset + 6).map(|v| (v / 2) as usize) else {
                return false;
            };
            let end_base = offset + 14;
            let start_base = end_base + seg_count * 2 + 2;
            for i in 0..seg_count {
                let (Some(end), Some(start)) = (
                    get_u16(cmap, end_base + i * 2),
                    get_u16(cmap, start_base + i * 2),
                ) else {
                    continue;
                };
                if cp >= start as u32 && cp <= end as u32 {
                    return true;
                }
            }
            false
        }
        _ => {
            let Some(ngroups) = get_u32(cmap, offset + 12) else {
                return false;
            };
            let groups_start = offset + 16;
            for i in 0..(ngroups.min(4096) as usize) {
                let base = groups_start + i * 12;
                let (Some(start), Some(end)) = (get_u32(cmap, base), get_u32(cmap, base + 4))
                else {
                    break;
                };
                if cp >= start && cp <= end {
                    return true;
                }
            }
            false
        }
    }
}

fn get_u16(data: &[u8], at: usize) -> Option<u16> {
    let b = data.get(at..at + 2)?;
    Some(u16::from_be_bytes([b[0], b[1]]))
}

fn get_u32(data: &[u8], at: usize) -> Option<u32> {
    let b = data.get(at..at + 4)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Locate an OpenType table by tag.
fn find_table<'a>(data: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let num_tables = get_u16(data, 4)? as usize;
    for i in 0..num_tables.min(64) {
        let rec = 12 + i * 16;
        if data.get(rec..rec + 4)? != tag {
            continue;
        }
        let offset = get_u32(data, rec + 8)? as usize;
        let len = get_u32(data, rec + 12)? as usize;
        return data.get(offset..offset.checked_add(len)?);
    }
    None
}

thread_local! {
    /// One scanned system font database per thread: the scan takes 420–480 ms (release, a
    /// machine with 160 families), and every text pass on this thread asks it the same
    /// questions.
    static SHARED_SYSTEM: RefCell<Option<Rc<RefCell<FontiqueProvider>>>> =
        const { RefCell::new(None) };
}

/// The thread's system font database, scanned on first use and shared after that.
///
/// Which fonts exist is a fact about the machine, not about one surface: a process with
/// several surfaces (a pin canvas beside the overlay) must scan once, not once per surface.
/// Registrations are shared with everything else on the thread, which is what a bundled
/// font is — a process asset, not a per-surface one.
pub fn shared_system() -> Rc<RefCell<FontiqueProvider>> {
    SHARED_SYSTEM.with(|slot| {
        Rc::clone(
            slot.borrow_mut()
                .get_or_insert_with(|| Rc::new(RefCell::new(system_or_isolated()))),
        )
    })
}

/// Convenience: a provider over the OS stack, falling back to isolated mode.
pub fn system_or_isolated() -> FontiqueProvider {
    let p = FontiqueProvider::new_system();
    if p.has_system_fonts() {
        p
    } else {
        FontiqueProvider::new_isolated()
    }
}

/// Report which generic family maps to which name, for diagnostics.
pub fn describe_generics(p: &FontiqueProvider) -> Vec<(GenericFamily, Option<String>)> {
    [
        GenericFamily::SansSerif,
        GenericFamily::Serif,
        GenericFamily::Monospace,
        GenericFamily::SystemUi,
    ]
    .into_iter()
    .map(|g| {
        let name = p
            .generic_face(g, &FontRequest::new(g))
            .and_then(|face| p.face_family_name(face));
        (g, name)
    })
    .collect()
}

#[cfg(test)]
mod tests;
