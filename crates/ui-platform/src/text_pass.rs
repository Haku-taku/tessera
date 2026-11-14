//! Turns `Text` primitives into positioned `Glyph` primitives plus an atlas.
//!
//! Both backends must see the same glyph stream: the CPU renderer takes coverage
//! from the atlas, the GPU renderer uploads it as a texture.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use ui_core::collections::{FastHasher, FastMap, FastSet};

use ui_core::draw::{
    DrawList, GlyphPrim, LinePrim, MathPrim, Op, PrimKind, RectPrim, TextPrim, Transform,
};
use ui_core::geom::Rect;
use ui_core::math::MathItem;
use ui_core::timed;
use ui_core::{Family, TextStyle};
use ui_text::{FaceId, FontProvider, FontRequest, GenericFamily, Script};
use ui_text_fontique::{FontiqueProvider, GlyphCache, ShapedRun, Shaper, place_run_scaled};

/// Initial atlas dimensions; `GlyphCache` grows it as needed.
const ATLAS_W: u32 = 1024;
const ATLAS_H: u32 = 1024;

/// Cap on cached measurements: a page never needs more, and dynamic text must not grow it.
const MEASURE_CACHE_MAX: usize = 16_384;

/// Cap on cached shaped spans: one per styled run on screen, and a page has far fewer.
const SHAPE_CACHE_MAX: usize = 4_096;

/// Cap on cached resolved primitives: one per text on screen, and a page has far fewer.
const RESOLVE_CACHE_MAX: usize = 4_096;

/// Cap on cached formulas: a page shows a handful, and a scroll of math is still far below it.
const MATH_FORMULA_CACHE_MAX: usize = 256;

/// One shaped piece of a span: the face that shaped it, the run, its byte range and script.
type ShapedPiece = (FaceId, ShapedRun, usize, usize, Script);

/// A text primitive's resolved primitives, reusable while nothing that shapes it changed.
struct CachedText {
    /// The primitive this was resolved from, compared exactly on every look-up.
    prim: TextPrim,
    /// Atlas generation when it was resolved; the slot rects below only hold for that stamp.
    stamp: u64,
    rects: Vec<RectPrim>,
    glyphs: Vec<GlyphPrim>,
    lines: Vec<LinePrim>,
    caret: Option<RectPrim>,
}

/// A math face the pass laid a formula out with, so painting uses the same ids.
struct MathFaceEntry {
    face: FaceId,
    math: ui_math::MathFace<'static>,
    /// The text stack the face's formulas measure their `\text` with.
    shaper: Arc<FormulaShaper>,
}

/// A structural editor the pass keeps for a control: its face, its parameters and the
/// source the app last handed over. The editor owns the model, caret and history.
struct MathEditorEntry {
    family: String,
    size: f32,
    display: bool,
    /// The source the app last supplied, so an edit the editor made itself is not mistaken
    /// for an external change when the app's copy catches up a frame later.
    source: String,
    /// The frame this entry was last attached in; an entry not attached for a frame is gone.
    seen: u64,
    /// The text input and byte range this formula is part of, when a document embeds it.
    owner: Option<(ui_core::Id, usize, usize)>,
    editor: ui_math::MathEditor,
}

/// Text inside a formula, shaped by the pass's font stack.
///
/// A formula's own face rarely covers CJK or emoji, and `\text` may hold either. The
/// primary face is tried first; the script's fallback faces follow. Each glyph carries the
/// slot of the face that produced it, because a glyph id means nothing in another font.
struct FormulaShaper {
    provider: std::rc::Weak<std::cell::RefCell<FontiqueProvider>>,
    /// The formula's face: slot 0, and the first tried for every run.
    primary: FaceId,
    /// Colour and symbol faces with no script chain of their own, tried last.
    emoji: Vec<FaceId>,
    shapers: std::sync::Mutex<FastMap<FaceId, Shaper>>,
    /// The face each glyph slot names; slot 0 is `primary`.
    slots: std::sync::Mutex<Vec<FaceId>>,
    /// `(ascent, descent)` in em, descent positive downward.
    metrics: std::sync::Mutex<FastMap<FaceId, (f32, f32)>>,
}

/// Em size the shaper measures at; advances are divided by it, so `\text` scales with `em`.
const SHAPE_EM: f32 = 64.0;

impl FormulaShaper {
    fn new(
        provider: std::rc::Weak<std::cell::RefCell<FontiqueProvider>>,
        primary: FaceId,
        emoji: Vec<FaceId>,
    ) -> Self {
        Self {
            provider,
            primary,
            emoji,
            shapers: std::sync::Mutex::new(FastMap::default()),
            slots: std::sync::Mutex::new(vec![primary]),
            metrics: std::sync::Mutex::new(FastMap::default()),
        }
    }

    /// The slot a face is painted through, adding it when the shaper had not used it.
    fn slot_of(&self, face: FaceId) -> u16 {
        let Ok(mut slots) = self.slots.lock() else {
            return 0;
        };
        if let Some(slot) = slots.iter().position(|known| *known == face) {
            return slot as u16;
        }
        slots.push(face);
        (slots.len() - 1) as u16
    }

    /// The face a glyph slot names.
    fn face_of(&self, slot: u16) -> Option<FaceId> {
        self.slots.lock().ok()?.get(slot as usize).copied()
    }

    /// The faces to try for one run, in order.
    fn candidates(&self, script: Script) -> Vec<FaceId> {
        let mut out = vec![self.primary];
        if let Some(provider) = self.provider.upgrade()
            && let Ok(provider) = provider.try_borrow()
        {
            for face in provider.fallback_chain(script) {
                if !out.contains(&face) {
                    out.push(face);
                }
            }
        }
        for face in &self.emoji {
            if !out.contains(face) {
                out.push(*face);
            }
        }
        out
    }

    /// Shape one range with one face, reusing the face's swash shaper across calls.
    fn shape_with(
        &self,
        face: FaceId,
        text: &str,
        range: std::ops::Range<usize>,
        script: Script,
        direction: ui_text::Direction,
    ) -> Option<ShapedRun> {
        let mut shapers = self.shapers.lock().ok()?;
        if let std::collections::hash_map::Entry::Vacant(slot) = shapers.entry(face) {
            let cell = self.provider.upgrade()?;
            let provider = cell.try_borrow().ok()?;
            let data = provider.face_data(face)?;
            let index = provider.face_index(face);
            slot.insert(Shaper::new(data, index)?);
        }
        Some(shapers.get_mut(&face)?.shape_run(
            text,
            range,
            SHAPE_EM,
            script,
            direction,
            ui_text::Features::NONE,
        ))
    }

    /// A face's ascent and descent in em, read from its own metrics.
    fn metrics_em(&self, face: FaceId) -> (f32, f32) {
        if let Ok(metrics) = self.metrics.lock()
            && let Some(found) = metrics.get(&face)
        {
            return *found;
        }
        let provider = self.provider.upgrade().and_then(|cell| {
            let borrowed = cell.try_borrow().ok()?;
            borrowed.face_data(face)
        });
        let measured = provider
            .and_then(ui_text_fontique::ParsedFace::new)
            .map(|parsed| {
                // The face's metrics are already em ratios; the descent is stored negative
                // and math-core counts it downward.
                let metrics = parsed.metrics();
                (metrics.ascent, (-metrics.descent).max(0.0))
            })
            .unwrap_or((0.8, 0.2));
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.insert(face, measured);
        }
        measured
    }

    /// Append a shaped range's glyphs, in em, to the run the layout reads.
    fn append(
        &self,
        out: &mut math_core::ShapedRun,
        face: FaceId,
        shaped: &ShapedRun,
        base: usize,
        pen: &mut f32,
    ) {
        let slot = self.slot_of(face);
        let (ascent, descent) = self.metrics_em(face);
        out.ascent = out.ascent.max(ascent);
        out.descent = out.descent.max(descent);
        for glyph in &shaped.glyphs {
            out.glyphs.push(math_core::ShapedGlyph {
                glyph: math_core::GlyphId(glyph.glyph_id),
                x: (*pen + glyph.x) / SHAPE_EM,
                // The shaper's y points down; math-core's points up.
                y: -glyph.y / SHAPE_EM,
                size: 1.0,
                cluster: base + glyph.cluster as usize,
                face: slot,
            });
            *pen += glyph.advance;
        }
        out.width = *pen / SHAPE_EM;
    }

    /// Shape a run no single face covers, one grapheme cluster at a time.
    ///
    /// Splitting on clusters never breaks a mark from its base; clusters no face covers are
    /// reported to the layout, which marks the construct instead of drawing `.notdef`.
    fn append_clusters(
        &self,
        out: &mut math_core::ShapedRun,
        text: &str,
        level: &ui_text::LevelRun,
        candidates: &[FaceId],
        pen: &mut f32,
    ) {
        let body = text.get(level.range.clone()).unwrap_or("");
        for (offset, cluster) in ui_core::grapheme::clusters(body) {
            let start = level.range.start + offset;
            let end = start + cluster.len();
            let covered = candidates.iter().find_map(|face| {
                let shaped =
                    self.shape_with(*face, text, start..end, level.script, level.direction())?;
                (!shaped.is_empty() && shaped.glyphs.iter().all(|glyph| glyph.glyph_id != 0))
                    .then_some((*face, shaped))
            });
            match covered {
                Some((face, shaped)) => self.append(out, face, &shaped, start, pen),
                None => out.missing.extend(cluster.chars()),
            }
        }
    }
}

impl math_core::TextShaper for FormulaShaper {
    fn shape(&self, text: &str, _style: math_core::TextStyle) -> math_core::ShapedRun {
        let mut out = math_core::ShapedRun::default();
        if text.is_empty() {
            return out;
        }
        let mut pen = 0.0;
        for level in ui_text::itemize(text, None) {
            let candidates = self.candidates(level.script);
            let covered = candidates.iter().find_map(|face| {
                let shaped = self.shape_with(
                    *face,
                    text,
                    level.range.clone(),
                    level.script,
                    level.direction(),
                )?;
                (!shaped.is_empty() && shaped.glyphs.iter().all(|glyph| glyph.glyph_id != 0))
                    .then_some((*face, shaped))
            });
            match covered {
                Some((face, shaped)) => {
                    self.append(&mut out, face, &shaped, level.range.start, &mut pen)
                }
                None => self.append_clusters(&mut out, text, &level, &candidates, &mut pen),
            }
        }
        out
    }
}

/// MD3's caret: two logical units wide, centred on the insertion point.
const CARET_WIDTH: f32 = 2.0;

/// Underline thickness for an input method's composing run, in logical units.
const COMPOSING_UNDERLINE: f32 = 1.0;

/// The side of the block that marks a selected line break, as a share of the line box.
const BREAK_MARK: f32 = 0.35;

/// Opacity and corner radius of the block standing in for an atom (P4b placeholder).
const ATOM_ALPHA: f32 = 0.25;
const ATOM_RADIUS: f32 = 4.0;

pub struct TextPass {
    /// Shared with the formula text shaper, which measures `\text` from inside the
    /// editor: one provider, one byte cache, and no call site has to own it.
    provider: std::rc::Rc<std::cell::RefCell<FontiqueProvider>>,
    cache: GlyphCache,
    /// Faces in preference order: the explicit stack first, then the platform generic.
    stack: Vec<FaceId>,
    /// Installed emoji faces, tried after the script fallback: no text face covers
    /// U+1F600, and the Latin fallback chain does not list them.
    emoji: Vec<FaceId>,
    /// Family names the pass was built with, kept so a styled run can re-resolve them.
    families: Vec<String>,
    /// Resolved stacks per (family, weight, italic): resolving walks the collection, and
    /// doing it per run per frame would repeat that walk needlessly.
    styled: FastMap<(Family, u16, bool), Vec<FaceId>>,
    /// One shaper per face, because a run must be shaped by the face that covers it.
    shapers: FastMap<FaceId, Shaper>,
    /// Script to the face found to cover it, within one family.
    /// Best face per (family, script, weight, italic): keying on the script alone let a bold
    /// run reuse a regular face, and let an icon face answer for unrelated Latin text.
    script_face: FastMap<(Family, Script, u16, bool), FaceId>,
    /// Atlas generation: `(rasterized, grows)` when the atlas last changed.
    atlas_mark: (u64, u64),
    version: u64,
    /// Text primitives this pass could not resolve, reported instead of dropped.
    unresolved: usize,
    /// Wrapped blocks, keyed by a hash of the block, the wrap width and the font set.
    measured: FastMap<u64, std::sync::Arc<ui_core::InlineLayout>>,
    /// Min-content widths, keyed like `measured` but without a width of their own.
    min_widths: FastMap<u64, f32>,
    /// Slice widths, keyed like `shaped_spans`: a page's labels repeat, and every line
    /// breaks over the same words several times before it settles.
    slice_widths: FastMap<u64, f32>,
    /// Font files already admitted, by data pointer and face index.
    font_files: FastSet<(usize, u32)>,
    /// Shaped spans, keyed by a hash of the text, the style and the font set: shaping is the
    /// costliest step of a frame, and a still page reshapes the same runs every frame.
    shaped_spans: FastMap<u64, std::rc::Rc<Vec<Vec<ShapedPiece>>>>,
    /// Resolved text primitives: resolving one re-runs shaping, fallback and atlas look-ups
    /// for every glyph, so a still page must not redo it. Keyed by `text_key`.
    resolved_text: FastMap<u64, std::rc::Rc<CachedText>>,
    /// Math faces by family. A formula's glyph ids only mean something in the face that laid
    /// it out, so the pass hands that face out and paints with it.
    math_faces: FastMap<String, MathFaceEntry>,
    /// The family a formula asks for when its caller names none, and whether it was looked
    /// for: "no math face installed" is an answer worth keeping.
    math_default: Option<String>,
    math_default_scanned: bool,
    /// Laid-out formulas, keyed by family, source, size and style: a still page rebuilds
    /// its formula nodes every frame, and parsing the MathML again is the whole cost.
    math_formulas: FastMap<u64, Result<ui_core::math::Formula, ui_core::math::MathError>>,
    /// Structural editors by node id, kept across frames while their controls are mounted.
    math_editors: FastMap<ui_core::Id, MathEditorEntry>,
    /// The frame the editors above were last pruned in.
    math_edit_frame: u64,
    /// Bumped when the set of loaded faces changes, so a stale size is never served.
    fonts_version: u64,
    /// Measurements that missed the cache; a layout that repeats itself must not grow this.
    measure_misses: u64,
    /// Text primitives that had to be resolved; repeats are served from `resolved_text`.
    resolve_misses: u64,
}

/// Whether the block draws symbols rather than text: one run, in a symbol family.
fn is_symbol_block(block: &ui_core::TextBlock) -> bool {
    match block.runs.as_slice() {
        [run] => run.style.family.is_symbol(),
        _ => false,
    }
}

/// The engine's editing key as the math editor spells it.
fn edit_key(key: ui_core::EditKey) -> ui_math::MathKey {
    match key {
        ui_core::EditKey::Character(character) => ui_math::MathKey::Character(character),
        ui_core::EditKey::Backspace => ui_math::MathKey::Backspace,
        ui_core::EditKey::Delete => ui_math::MathKey::Delete,
        ui_core::EditKey::Left => ui_math::MathKey::Left,
        ui_core::EditKey::Right => ui_math::MathKey::Right,
        ui_core::EditKey::Up => ui_math::MathKey::Up,
        ui_core::EditKey::Down => ui_math::MathKey::Down,
        ui_core::EditKey::Home => ui_math::MathKey::Home,
        ui_core::EditKey::End => ui_math::MathKey::End,
        ui_core::EditKey::Escape => ui_math::MathKey::Escape,
    }
}

/// A math editor's rect as the engine's vocabulary spells it.
fn edit_rect(rect: ui_math::EditRect) -> ui_core::MathEditRect {
    ui_core::MathEditRect {
        x: rect.x,
        y: rect.y,
        w: rect.w,
        h: rect.h,
    }
}

/// A plain filled rect: a formula's rule or background.
/// A display-list segment placed at the formula's origin and baseline.
fn math_line(
    origin: ui_core::geom::Point,
    baseline: f32,
    [x1, y1, x2, y2]: [f32; 4],
    thickness: f32,
    colour: ui_core::Color,
) -> LinePrim {
    LinePrim {
        from: ui_core::geom::Point::new(origin.x + x1, baseline + y1),
        to: ui_core::geom::Point::new(origin.x + x2, baseline + y2),
        thickness,
        colour,
    }
}

fn math_rect(x: f32, y: f32, w: f32, h: f32, fill: ui_core::Color) -> RectPrim {
    RectPrim {
        rect: ui_core::geom::Rect::new(x, y, w, h),
        radius: ui_core::Corners::ZERO,
        fill,
        gradient: None,
        border: None,
        border_gradient: None,
        notch: None,
    }
}

/// A block prepared once, so paint and the geometry queries agree on every fact.
struct Prepared {
    flat: String,
    spans: Vec<ui_core::Span>,
    /// The lines layout already sized; paint draws these instead of wrapping again.
    layout: std::sync::Arc<ui_core::InlineLayout>,
}

/// One char boundary on a line, and where it sits on the surface.
///
/// Built from the shaped glyphs, so a click, a caret and a selection all agree about
/// where byte `n` is — prefix-shaping each of them separately would let them drift.
#[derive(Clone, Copy, Debug)]
struct Boundary {
    byte: usize,
    x: f32,
    rtl: bool,
}

impl Default for TextPass {
    fn default() -> Self {
        Self::new()
    }
}

impl TextPass {
    /// Stack backed by the platform's generic sans face.
    pub fn new() -> Self {
        Self::shared(&[])
    }

    /// Family names tried in order; unknown or unloadable names are skipped.
    pub fn with_stack(families: &[&str]) -> Self {
        Self::shared(families)
    }

    /// Pass with no font at all: text stays unresolved and is reported as such.
    pub fn isolated() -> Self {
        Self::build(Rc::new(RefCell::new(FontiqueProvider::new_isolated())), &[])
    }

    /// A pass over the process's scanned font database.
    ///
    /// Only the database is shared: the glyph atlas, the shaped spans and every metric
    /// belong to one pass, because layout and paint must agree on one face choice.
    fn shared(families: &[&str]) -> Self {
        Self::build(ui_text_fontique::shared_system(), families)
    }

    /// Register font bytes under `name`; runs naming that family resolve to these bytes.
    /// Registered families win over system ones, so a bundled icon font cannot be shadowed.
    pub fn register(&mut self, name: &str, data: Arc<[u8]>) -> bool {
        let ok = self.provider.borrow_mut().register_family(name, data);
        if ok {
            self.refresh_faces();
        }
        ok
    }

    /// The families this pass prefers, in order; empty leaves the platform's own choice.
    pub fn families(&self) -> &[String] {
        &self.families
    }

    /// Replace the preferred families, the way a settings page changes the app's font.
    ///
    /// Setting a family is not registering it: the bytes must already be installed or
    /// registered. Every answer derived from the old choice is dropped, so a family set
    /// after the first frame reaches layout like one passed to `with_stack`.
    pub fn set_families(&mut self, families: &[&str]) {
        let next: Vec<String> = families.iter().map(|family| family.to_string()).collect();
        if next == self.families {
            return;
        }
        self.families = next;
        self.refresh_faces();
    }

    /// Every family a picker can offer: installed ones plus those registered here.
    pub fn family_names(&self) -> Vec<String> {
        self.provider.borrow().family_names()
    }

    /// Drop everything derived from a face choice, then resolve the stack again.
    fn refresh_faces(&mut self) {
        // A stack built before the family existed would keep answering "no such face", and
        // a metric cached under the old stack would keep the old advance.
        self.styled.clear();
        self.script_face.clear();
        self.rebuild_stack();
        self.forget_measurements();
    }

    fn build(provider: Rc<RefCell<FontiqueProvider>>, families: &[&str]) -> Self {
        let mut pass = Self {
            families: families.iter().map(|f| f.to_string()).collect(),
            styled: FastMap::default(),
            provider,
            cache: GlyphCache::new(ATLAS_W, ATLAS_H),
            stack: Vec::new(),
            emoji: Vec::new(),
            shapers: FastMap::default(),
            script_face: FastMap::default(),
            atlas_mark: (0, 0),
            version: 0,
            unresolved: 0,
            measured: FastMap::default(),
            min_widths: FastMap::default(),
            slice_widths: FastMap::default(),
            font_files: FastSet::default(),
            shaped_spans: FastMap::default(),
            resolved_text: FastMap::default(),
            math_faces: FastMap::default(),
            math_default: None,
            math_default_scanned: false,
            math_formulas: FastMap::default(),
            math_editors: FastMap::default(),
            math_edit_frame: 0,
            fonts_version: 0,
            measure_misses: 0,
            resolve_misses: 0,
        };
        pass.rebuild_stack();
        pass
    }

    /// Resolve the base stack from the configured families, in priority order.
    ///
    /// This runs again after every registration: a family added later has to reach the
    /// stack too, and it belongs ahead of whatever the platform chose before it existed.
    fn rebuild_stack(&mut self) {
        let names = self.families.clone();
        let mut stack: Vec<FaceId> = Vec::new();
        // Resolving is cheap; admitting is not: a CJK face's file is tens of megabytes, and
        // most of the stack never shapes anything, so the bytes wait for `shaper_for`.
        let take = |pass: &mut Self, request: &FontRequest, stack: &mut Vec<FaceId>| {
            if let Some(face) = pass.provider.borrow().resolve(request)
                && !stack.contains(&face)
            {
                stack.push(face);
            }
        };
        // Named families first: an explicit stack is what makes coverage predictable.
        for name in &names {
            take(
                self,
                &FontRequest::new(GenericFamily::SansSerif).family(name.as_str()),
                &mut stack,
            );
        }
        // Then the platform's own choice for the generic family, the UI face ahead of the
        // plain sans: the default style asks for `SystemUi`, and this is the stack it gets.
        take(
            self,
            &FontRequest::new(GenericFamily::SystemUi),
            &mut stack,
        );
        take(
            self,
            &FontRequest::new(GenericFamily::SansSerif),
            &mut stack,
        );
        self.stack = stack;

        // Emoji faces are found by name: the generic families have no Emoji variant, and
        // the script fallback for a Common character answers with a text face.
        let mut emoji: Vec<FaceId> = Vec::new();
        for name in self.provider.borrow().family_names() {
            if !name.to_ascii_lowercase().contains("emoji") {
                continue;
            }
            let request = FontRequest::new(GenericFamily::SansSerif).family(name);
            if let Some(face) = self.provider.borrow().resolve(&request)
                && !emoji.contains(&face)
            {
                emoji.push(face);
            }
        }
        self.emoji = emoji;
    }

    /// True when at least one loadable face exists, i.e. text can reach the atlas.
    pub fn has_face(&self) -> bool {
        !self.stack.is_empty()
    }

    pub fn atlas(&self) -> &[u8] {
        self.cache.atlas()
    }

    /// Premultiplied RGBA page for colour glyphs; empty until one is rasterized.
    pub fn color_atlas(&self) -> &[u8] {
        self.cache.color_atlas()
    }

    pub fn dimensions(&self) -> (u32, u32) {
        self.cache.dimensions()
    }

    pub fn color_dimensions(&self) -> (u32, u32) {
        self.cache.color_dimensions()
    }

    /// Changes whenever the colour page changes; tracked separately so a new text
    /// glyph does not re-upload four megabytes of emoji.
    pub fn color_version(&self) -> u64 {
        self.cache.color_version
    }

    /// Changes whenever new coverage was rasterized into the atlas.
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn unresolved(&self) -> usize {
        self.unresolved
    }

    /// Measurements that actually ran; repeats are served from the cache.
    pub fn measure_misses(&self) -> u64 {
        self.measure_misses
    }

    /// Text primitives that had to be resolved; an unchanged one is reused.
    pub fn resolve_misses(&self) -> u64 {
        self.resolve_misses
    }

    /// Rebuild `list`, replacing every `Text` primitive with placed glyphs.
    ///
    /// Unresolvable text stays in the list so a CPU fallback still runs; it is
    /// counted in `unresolved()` rather than vanishing.
    pub fn resolve(&mut self, list: &DrawList, scale: f32) -> DrawList {
        let mut out = DrawList::new(list.canvas);
        self.resolve_into(list, scale, &mut out);
        out
    }

    /// Same pass, writing into a caller-owned list so its capacity survives the frame.
    pub fn resolve_into(&mut self, list: &DrawList, scale: f32, out: &mut DrawList) {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        self.unresolved = 0;

        out.clear();
        out.canvas = list.canvas;
        out.clips.clone_from(&list.clips);
        out.layers.clone_from(&list.layers);
        // Backdrop entries are referenced by index from the ops, so they must travel too.
        out.backdrops.clone_from(&list.backdrops);
        // Image entries are indexed by the image prims, so they travel too; the pixels sit
        // behind an Arc, so this copies handles rather than megabytes.
        out.images.clone_from(&list.images);

        // The clips in force, intersected down the stack: text that cannot reach the frame
        // must not shape, or a document pays for every block it holds on every frame.
        let mut clips: Vec<Rect> = Vec::new();
        for op in &list.ops {
            let Op::Prim(prim) = op else {
                match op {
                    Op::PushClip(index) => {
                        let clip = list
                            .clips
                            .get(*index as usize)
                            .map_or(Rect::ZERO, |c| c.rect);
                        let clip = match clips.last() {
                            Some(outer) => outer.intersection(clip).unwrap_or(Rect::ZERO),
                            None => clip,
                        };
                        clips.push(clip);
                    }
                    Op::PopClip => {
                        clips.pop();
                    }
                    _ => {}
                }
                out.ops.push(*op);
                continue;
            };
            match prim.kind {
                // Images carry no text, so the resolved list keeps them only for the painter.
                PrimKind::Shadow => {
                    let owner = list.owners.shadows[prim.index as usize];
                    out.push_shadow(owner, list.prims.shadows[prim.index as usize]);
                }
                PrimKind::Image => {
                    let owner = list.owners.images[prim.index as usize];
                    out.push_image_prim(owner, list.prims.images[prim.index as usize]);
                }
                // Arcs and ellipses carry no text either; they only need to survive the pass.
                PrimKind::Sector => {
                    let owner = list.owners.sectors[prim.index as usize];
                    out.push_sector(owner, list.prims.sectors[prim.index as usize]);
                }
                PrimKind::Ellipse => {
                    let owner = list.owners.ellipses[prim.index as usize];
                    out.push_ellipse(owner, list.prims.ellipses[prim.index as usize]);
                }
                PrimKind::Line => {
                    let owner = list.owners.lines[prim.index as usize];
                    out.push_line(owner, list.prims.lines[prim.index as usize]);
                }
                PrimKind::Rect => {
                    let owner = list.owners.rects[prim.index as usize];
                    out.push_rect(owner, list.prims.rects[prim.index as usize]);
                }
                PrimKind::Glyph => {
                    let owner = list.owners.glyphs[prim.index as usize];
                    out.push_glyph(owner, list.prims.glyphs[prim.index as usize]);
                }
                PrimKind::Text => {
                    let owner = list.owners.texts[prim.index as usize];
                    let text = &list.prims.texts[prim.index as usize];
                    if !Self::text_can_reach(text, clips.last().copied()) {
                        continue;
                    }
                    // A scaled pin shapes at its drawn size instead of stretching a picture
                    // of the glyphs: the atlas texel and the destination differ by `scale`
                    // alone, so the result is as sharp as text laid out that large.
                    let own = text.shape_scale;
                    let done = self.shape_cached(text, scale * own);
                    // Spaces ink nothing and a line range trims them away, so a blank window
                    // still shapes to no glyph; its caret and selection are all it paints.
                    let inkless = text.block.atoms.is_empty()
                        && text
                            .block
                            .runs
                            .iter()
                            .all(|run| !run.visibility.painted() || run.text.trim().is_empty());
                    if done.glyphs.is_empty() && !inkless {
                        self.unresolved += 1;
                        // The prim carries its own zoom; the list must not stack a second one.
                        out.transform = None;
                        out.push_text(owner, text.clone());
                        continue;
                    }
                    // Shaped unscaled, scaled on the way out; the highlight goes under the
                    // glyphs and the caret over them.
                    out.transform = text.zoom.filter(|t| !t.is_identity());
                    for rect in &done.rects {
                        out.push_rect(owner, *rect);
                    }
                    for glyph in &done.glyphs {
                        out.push_glyph(owner, *glyph);
                    }
                    for line in &done.lines {
                        out.push_line(owner, *line);
                    }
                    // After the glyphs: the caret belongs on top of the text it splits.
                    if let Some(rect) = done.caret {
                        out.push_rect(owner, rect);
                    }
                    out.transform = None;
                }
                PrimKind::Math => {
                    let owner = list.owners.maths[prim.index as usize];
                    let math = &list.prims.maths[prim.index as usize];
                    if !Self::math_can_reach(math, clips.last().copied()) {
                        continue;
                    }
                    let own = math.shape_scale;
                    let Some((rects, glyphs, lines)) = self.math_items(
                        &math.family,
                        &math.formula,
                        math.ink,
                        math.origin,
                        scale * own,
                    ) else {
                        self.unresolved += 1;
                        out.transform = None;
                        out.push_math(owner, math.clone());
                        continue;
                    };
                    out.transform = math.zoom.filter(|t| !t.is_identity());
                    // Rules and backgrounds first: a fraction bar sits under its glyphs.
                    for rect in &rects {
                        out.push_rect(owner, *rect);
                    }
                    for glyph in &glyphs {
                        out.push_glyph(owner, *glyph);
                    }
                    // Cancel strokes sit over the body they cross.
                    for line in &lines {
                        out.push_line(owner, *line);
                    }
                    out.transform = None;
                }
            }
        }
        self.note_atlas_change();
    }

    /// Whether any part of this box can land inside `clip`, after its own scale.
    fn reaches(bounds: Option<Rect>, zoom: Option<Transform>, clip: Option<Rect>) -> bool {
        let (Some(bounds), Some(clip)) = (bounds, clip) else {
            return true;
        };
        let bounds = match zoom.filter(|zoom| !zoom.is_identity()) {
            Some(zoom) => zoom.apply_rect(bounds),
            None => bounds,
        };
        bounds.intersects(clip)
    }

    /// Whether any part of this text can land inside `clip`.
    ///
    /// The proof is the block's box with one em of slack around it, which covers bearings,
    /// italics and symbols centred in their box; text with no laid-out box is never dropped.
    fn text_can_reach(text: &TextPrim, clip: Option<Rect>) -> bool {
        let Some(layout) = text.layout.as_deref() else {
            return true;
        };
        let size = layout.size();
        let em = text
            .block
            .runs
            .first()
            .map_or(0.0, |run| run.style.size)
            .max(text.height);
        let bounds = Rect::new(
            text.origin.x,
            text.origin.y,
            size.w,
            size.h.max(text.height),
        )
        .inflate(em);
        Self::reaches(Some(bounds), text.zoom, clip)
    }

    /// The same proof for a formula: its box, with half its height of slack for the bars,
    /// radicals and italic correction that reach past it.
    fn math_can_reach(math: &MathPrim, clip: Option<Rect>) -> bool {
        let height = math.formula.height();
        let bounds = Rect::new(math.origin.x, math.origin.y, math.formula.width, height)
            .inflate(height * 0.5);
        Self::reaches(Some(bounds), math.zoom, clip)
    }

    /// Lay `source` out with the math face `family` resolves to, or `None` when that family is
    /// missing or has no MATH table.
    ///
    /// The formula's glyph ids belong to this pass's face, so this is the only correct
    /// producer of a formula the pass will paint.
    pub fn layout_math(
        &mut self,
        family: &str,
        source: &str,
        size: f32,
        display: bool,
        style: Option<ui_core::math::MathErrorStyle>,
    ) -> Result<ui_core::math::Formula, ui_core::math::MathError> {
        use std::hash::{Hash, Hasher};
        let mut h = FastHasher::default();
        self.fonts_version.hash(&mut h);
        family.hash(&mut h);
        source.hash(&mut h);
        size.to_bits().hash(&mut h);
        display.hash(&mut h);
        // The error box is painted from the formula, so its colours belong to the key.
        style
            .map(|style| (style.border, style.panel, style.panel_ink))
            .hash(&mut h);
        let key = h.finish();
        if let Some(hit) = self.math_formulas.get(&key) {
            return hit.clone();
        }
        // A failure is cached too: an editor's invalid source must not re-parse every frame.
        let laid_out = match self.math_entry(family) {
            Some(entry) => entry
                .math
                .layout_recovering_with(source, size, display, style, Some(&*entry.shaper))
                .map_err(|error| ui_core::math::MathError::Failed(error.to_string())),
            None => Err(ui_core::math::MathError::NoFace),
        };
        if self.math_formulas.len() >= MATH_FORMULA_CACHE_MAX {
            self.math_formulas.clear();
        }
        self.math_formulas.insert(key, laid_out.clone());
        laid_out
    }

    /// The family a formula asks for when its caller names none: the first installed family
    /// that says math in its name and whose font carries the table.
    pub fn default_math_family(&mut self) -> Option<String> {
        if !self.math_default_scanned {
            self.math_default_scanned = true;
            for name in self.provider.borrow().family_names() {
                if !name.to_lowercase().contains("math") {
                    continue;
                }
                let request = FontRequest::new(GenericFamily::SystemUi).family(name.as_str());
                let Some(face) = self.provider.borrow().resolve(&request) else {
                    continue;
                };
                let Some(data) = self.provider.borrow().face_data(face) else {
                    continue;
                };
                if ui_math::probe(data.bytes(), self.provider.borrow().face_index(face) as u32) {
                    self.math_default = Some(name);
                    break;
                }
            }
        }
        self.math_default.clone()
    }

    /// The pass's math face for `family`, parsed once and kept for the process.
    fn math_entry(&mut self, family: &str) -> Option<&MathFaceEntry> {
        if !self.math_faces.contains_key(family) {
            let request = FontRequest::new(GenericFamily::SystemUi).family(family);
            let face = self.provider.borrow().resolve(&request)?;
            // The rasterizer needs the outlines, not just the MATH table the layout read.
            if !self.admit_face(face) {
                return None;
            }
            let data = self.provider.borrow().face_data(face)?;
            let index = self.provider.borrow().face_index(face) as u32;
            // `MathFace` borrows its bytes, so they must outlive the pass. One face per
            // family leaks once and then lives as long as the process does.
            let bytes: &'static [u8] = Box::leak(data.bytes().to_vec().into_boxed_slice());
            let math = ui_math::MathFace::new(bytes, index).ok()?;
            // The pass is a UI-thread value: `Arc` is the editor's container for the
            // shaper, not a claim that shaping may cross threads.
            #[allow(clippy::arc_with_non_send_sync)]
            let shaper = Arc::new(FormulaShaper::new(
                std::rc::Rc::downgrade(&self.provider),
                face,
                self.emoji.clone(),
            ));
            self.math_faces
                .insert(family.to_owned(), MathFaceEntry { face, math, shaper });
        }
        self.math_faces.get(family)
    }

    /// Keep an editor for `id` alive this frame, starting one from `source` when there is
    /// none. An editor not attached for a frame is dropped, so a page swap frees its
    /// history instead of keeping every formula ever focused.
    pub fn math_edit_attach(&mut self, request: ui_core::MathAttach<'_>) -> bool {
        let ui_core::MathAttach {
            id,
            family,
            source,
            size,
            display,
            frame,
            owner,
        } = request;
        if self.math_edit_frame != frame {
            self.math_edit_frame = frame;
            let previous = frame.saturating_sub(1);
            self.math_editors.retain(|_, entry| entry.seen >= previous);
        }
        if !self.math_faces.contains_key(family) && self.math_entry(family).is_none() {
            return false;
        }
        // A different face measures the same model differently; start again from what the
        // app holds rather than keep a caret whose geometry belonged to the old face.
        if self
            .math_editors
            .get(&id)
            .is_some_and(|entry| entry.family != family)
        {
            self.math_editors.remove(&id);
        }
        let face = &self.math_faces[family].math;
        if let Some(entry) = self.math_editors.get_mut(&id) {
            entry.seen = frame;
            if entry.source != source {
                // Only a source the editor did not write itself is an external change.
                if entry.editor.source() != source {
                    entry.editor.set_source(face, source);
                }
                entry.source = source.to_owned();
            }
            entry.owner = owner;
            if entry.size != size {
                entry.editor.set_size(face, size);
                entry.size = size;
            }
            if entry.display != display {
                entry.editor.set_display(face, display);
                entry.display = display;
            }
            return true;
        }
        let mut editor = face.editor(source, size, display);
        editor.set_shaper(
            face,
            Arc::clone(&self.math_faces[family].shaper) as Arc<dyn math_core::TextShaper>,
        );
        self.math_editors.insert(
            id,
            MathEditorEntry {
                family: family.to_owned(),
                size,
                display,
                source: source.to_owned(),
                seen: frame,
                owner,
                editor,
            },
        );
        true
    }

    /// The editor for `id` and the face that paints it, so an edit and its layout agree.
    fn math_editor(
        &mut self,
        id: ui_core::Id,
    ) -> Option<(&mut ui_math::MathEditor, &ui_math::MathFace<'static>)> {
        let family = self.math_editors.get(&id)?.family.clone();
        let face = &self.math_faces.get(&family)?.math;
        let editor = &mut self.math_editors.get_mut(&id)?.editor;
        Some((editor, face))
    }

    /// The text input whose buffer `id`'s formula is part of, with its byte range.
    pub fn math_edit_owner(&mut self, id: ui_core::Id) -> Option<(ui_core::Id, usize, usize)> {
        self.math_editors.get(&id)?.owner
    }

    pub fn math_edit_is_editing(&mut self, id: ui_core::Id) -> bool {
        self.math_editors.contains_key(&id)
    }

    pub fn math_edit_key(&mut self, id: ui_core::Id, key: ui_core::EditKey, shift: bool) -> bool {
        self.math_editor(id)
            .is_some_and(|(editor, face)| editor.key(face, edit_key(key), shift))
    }

    pub fn math_edit_insert(&mut self, id: ui_core::Id, latex: &str) -> bool {
        self.math_editor(id)
            .is_some_and(|(editor, face)| editor.insert_latex(face, latex))
    }

    pub fn math_edit_click(&mut self, id: ui_core::Id, x: f32, y: f32, extend: bool) -> bool {
        self.math_editors
            .get_mut(&id)
            .is_some_and(|entry| entry.editor.click(x, y, extend))
    }

    pub fn math_edit_formula(
        &mut self,
        id: ui_core::Id,
        style: Option<ui_core::math::MathErrorStyle>,
    ) -> Option<ui_core::math::Formula> {
        self.math_editors
            .get_mut(&id)
            .map(|entry| entry.editor.formula(style))
    }

    pub fn math_edit_source(&mut self, id: ui_core::Id) -> Option<String> {
        self.math_editors
            .get(&id)
            .map(|entry| entry.editor.source())
    }

    pub fn math_edit_caret(&mut self, id: ui_core::Id) -> Option<ui_core::MathCaret> {
        self.math_editors
            .get(&id)
            .and_then(|entry| entry.editor.caret())
            .map(|caret| ui_core::MathCaret {
                x: caret.x,
                top: caret.top,
                bottom: caret.bottom,
            })
    }

    pub fn math_edit_selection(&mut self, id: ui_core::Id) -> Vec<ui_core::MathEditRect> {
        self.math_editors
            .get(&id)
            .map(|entry| {
                entry
                    .editor
                    .selection()
                    .into_iter()
                    .map(edit_rect)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn math_edit_placeholders(&mut self, id: ui_core::Id) -> Vec<ui_core::MathEditRect> {
        self.math_editors
            .get(&id)
            .map(|entry| {
                entry
                    .editor
                    .placeholders()
                    .into_iter()
                    .map(edit_rect)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The selection as byte offsets into the formula's own source.
    pub fn math_edit_selection_offsets(&mut self, id: ui_core::Id) -> Option<(usize, usize)> {
        self.math_editors.get(&id)?.editor.selection_offsets()
    }

    /// Place the formula's caret and selection from byte offsets of its own source.
    pub fn math_edit_select_offsets(
        &mut self,
        id: ui_core::Id,
        anchor: usize,
        caret: usize,
    ) -> bool {
        self.math_editors
            .get_mut(&id)
            .is_some_and(|entry| entry.editor.set_selection_offsets(anchor, caret))
    }

    /// The offset in the formula's own source nearest a point in formula space.
    pub fn math_edit_offset_at(&mut self, id: ui_core::Id, x: f32, y: f32) -> Option<usize> {
        self.math_editors.get(&id)?.editor.offset_at(x, y)
    }

    /// The formula embedded in `owner` whose byte range holds `offset`.
    ///
    /// A caret in a document's buffer lands inside the formula's bytes there, and the
    /// structural editor is what has to act on it. Ranges are half-open with the end
    /// counted, so a caret just past the payload still belongs to the formula whose
    /// closing fence it precedes.
    pub fn math_edit_at(&mut self, owner: ui_core::Id, offset: usize) -> Option<ui_core::Id> {
        let mut best: Option<(usize, ui_core::Id)> = None;
        for (id, entry) in &self.math_editors {
            let Some((input, start, end)) = entry.owner else {
                continue;
            };
            if input != owner || offset < start || offset > end {
                continue;
            }
            let width = end - start;
            if best.is_none_or(|(narrowest, _)| width < narrowest) {
                best = Some((width, *id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// Point every formula `owner` embeds at the input's selection, so the structural
    /// caret and the document's caret name the same place.
    ///
    /// Offsets outside a formula clamp to its ends: a selection that starts in the prose
    /// and runs through the formula selects the formula whole.
    pub fn math_edit_follow(&mut self, owner: ui_core::Id, anchor: usize, caret: usize) {
        for entry in self.math_editors.values_mut() {
            let Some((input, start, end)) = entry.owner else {
                continue;
            };
            if input != owner || start > end {
                continue;
            }
            let local = |at: usize| at.clamp(start, end) - start;
            entry
                .editor
                .set_selection_offsets(local(anchor), local(caret));
        }
    }

    pub fn math_edit_message(&mut self, id: ui_core::Id) -> Option<String> {
        self.math_editors
            .get(&id)
            .and_then(|entry| entry.editor.message())
            .map(str::to_owned)
    }

    pub fn math_edit_set_preedit(&mut self, id: ui_core::Id, text: &str) -> bool {
        self.math_editor(id)
            .is_some_and(|(editor, face)| editor.set_preedit(face, text))
    }

    pub fn math_edit_commit_preedit(&mut self, id: ui_core::Id, text: &str) -> bool {
        self.math_editor(id)
            .is_some_and(|(editor, face)| editor.commit_preedit(face, text))
    }

    pub fn math_edit_cancel_preedit(&mut self, id: ui_core::Id) -> bool {
        self.math_editor(id)
            .is_some_and(|(editor, face)| editor.cancel_preedit(face))
    }

    pub fn math_edit_undo(&mut self, id: ui_core::Id) -> bool {
        self.math_editor(id)
            .is_some_and(|(editor, face)| editor.undo(face))
    }

    pub fn math_edit_redo(&mut self, id: ui_core::Id) -> bool {
        self.math_editor(id)
            .is_some_and(|(editor, face)| editor.redo(face))
    }

    pub fn math_edit_select_all(&mut self, id: ui_core::Id) -> bool {
        self.math_editors
            .get_mut(&id)
            .is_some_and(|entry| entry.editor.select_all())
    }

    pub fn math_edit_selection_latex(&mut self, id: ui_core::Id) -> Option<String> {
        self.math_editors
            .get_mut(&id)
            .and_then(|entry| entry.editor.selection_latex())
    }

    pub fn math_edit_cut(&mut self, id: ui_core::Id) -> Option<String> {
        self.math_editor(id)
            .and_then(|(editor, face)| editor.cut_selection(face))
    }

    /// Rasterize a formula's display list: rects in list order, then its glyphs.
    ///
    /// `origin` is the top-left of the formula's box, and `formula.ascent` is how far its
    /// baseline sits below that: a math node and an inline atom differ only in where the
    /// box starts, so both come through here.
    fn math_items(
        &mut self,
        family: &str,
        formula: &ui_core::math::Formula,
        ink: ui_core::Color,
        origin: ui_core::geom::Point,
        scale: f32,
    ) -> Option<(Vec<RectPrim>, Vec<GlyphPrim>, Vec<LinePrim>)> {
        let (primary, shaper) = {
            let entry = self.math_entry(family)?;
            (entry.face, Arc::clone(&entry.shaper))
        };
        let baseline = origin.y + formula.ascent;
        let mut rects = Vec::new();
        let mut glyphs = Vec::new();
        let mut lines = Vec::new();
        for item in &formula.items {
            match item {
                MathItem::Glyph {
                    id,
                    face,
                    x,
                    y,
                    size,
                    color,
                    mirrored,
                    ..
                } => {
                    // A mirror needs a flipped blit; no LaTeX input asks for one, and drawing
                    // an RTL radical unmirrored is wrong, so the formula stays unresolved.
                    if *mirrored {
                        return None;
                    }
                    let origin_x = ((origin.x + x) * scale).round() as i32;
                    let baseline_y = (baseline + y) * scale;
                    let (_, y_bin, _) = ui_text::VerticalBin::split(baseline_y);
                    // A text run may have fallen back to another face; its glyph ids are
                    // that face's, so it is that face that rasterizes them.
                    let face = if *face == 0 {
                        primary
                    } else {
                        shaper.face_of(*face).unwrap_or(primary)
                    };
                    if face != primary {
                        self.admit_face(face);
                    }
                    let Some(blit) = self.cache.blit_glyph(
                        face,
                        *id,
                        origin_x,
                        baseline_y,
                        size * scale,
                        ui_text::Phases::new(ui_text::SubpixelBin::Zero, y_bin),
                        0.0,
                    ) else {
                        continue;
                    };
                    glyphs.push(GlyphPrim {
                        rect: blit.dest_logical(scale),
                        atlas: blit.atlas,
                        alpha: 1.0,
                        color: color.unwrap_or(ink),
                        kind: blit.kind,
                    });
                }
                MathItem::Rule { x, y, w, h, color } => {
                    rects.push(math_rect(
                        origin.x + x,
                        baseline + y,
                        *w,
                        *h,
                        color.unwrap_or(ink),
                    ));
                }
                MathItem::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    thickness,
                    color,
                } => {
                    lines.push(math_line(
                        origin,
                        baseline,
                        [*x1, *y1, *x2, *y2],
                        *thickness,
                        color.unwrap_or(ink),
                    ));
                }
                MathItem::Background { x, y, w, h, color } => {
                    rects.push(math_rect(origin.x + x, baseline + y, *w, *h, *color));
                }
            }
        }
        Some((rects, glyphs, lines))
    }

    /// Resolve one text primitive, reusing the previous frame's answer when nothing that
    /// shapes it changed: shaping is by far the costliest step of a still frame.
    fn shape_cached(&mut self, text: &TextPrim, scale: f32) -> std::rc::Rc<CachedText> {
        let key = text_key(text, scale, self.fonts_version);
        let stamp = self.atlas_stamp();
        if let Some(hit) = self.resolved_text.get(&key)
            && hit.stamp == stamp
            && hit.prim.same_as(text)
        {
            return std::rc::Rc::clone(hit);
        }
        self.resolve_misses += 1;
        let (rects, glyphs, lines, caret) = self.shape(text, scale);
        // Re-keyed after shaping: shaping is what registers a face and bumps `fonts_version`,
        // and the stamp is read after it for the same reason — both must survive the frame
        // that made them.
        let key = text_key(text, scale, self.fonts_version);
        let entry = std::rc::Rc::new(CachedText {
            prim: text.clone(),
            stamp: self.atlas_stamp(),
            rects,
            glyphs,
            lines,
            caret,
        });
        if self.resolved_text.len() >= RESOLVE_CACHE_MAX {
            self.resolved_text.clear();
        }
        self.resolved_text.insert(key, std::rc::Rc::clone(&entry));
        entry
    }

    /// Record that glyphs were rasterized into the atlas, so the host re-uploads it.
    ///
    /// Any primitive kind can rasterize — a formula's glyphs are rasterized by the same
    /// cache — so the check also runs once at the end of a pass: the last primitive may be
    /// a formula, and its coverage would otherwise sit in memory while the renderer keeps
    /// drawing from the old page.
    fn note_atlas_change(&mut self) {
        let mark = (self.cache.stats.rasterized, self.cache.stats.grows);
        if mark != self.atlas_mark {
            self.atlas_mark = mark;
            self.version += 1;
        }
    }

    /// Identity of the slots a resolved primitive points at.
    ///
    /// A cached glyph holds a texel rect, and only a page clear reuses slots; rasterizing a
    /// glyph or widening a page leaves every existing slot where it was, and the renderer
    /// normalizes the rects by the atlas size it samples with. Counting those would make one
    /// newly revealed glyph reshape every text on the page — which is what a selection drag
    /// does, construct after construct, for the whole gesture.
    fn atlas_stamp(&self) -> u64 {
        self.cache.stats.evictions
    }

    /// Shape one text primitive, choosing a covering face per visual run.
    ///
    /// One face is not enough: Latin, CJK and Arabic are rarely covered by a single
    /// font, and shaping a run with a non-covering face yields `.notdef` boxes.
    fn shape(
        &mut self,
        text: &TextPrim,
        scale: f32,
    ) -> (
        Vec<RectPrim>,
        Vec<GlyphPrim>,
        Vec<LinePrim>,
        Option<RectPrim>,
    ) {
        let prep = self.prepare(text);
        // The caret belongs to the line, not to the glyphs: an empty block draws no ink and
        // still shows where the caret stands, which is the whole of a blank markdown row.
        let caret_rect = text.caret.and_then(|caret| {
            let rect = self.caret_rect_with(text, &prep, caret.offset, scale)?;
            Some(RectPrim {
                rect,
                radius: ui_core::Corners::ZERO,
                fill: caret.color,
                gradient: None,
                border: None,
                border_gradient: None,
                notch: None,
            })
        });
        if prep.spans.is_empty() {
            // An empty block paints no ink but still owns a line box, and that line is where
            // its selected break and its composing underline stand, as its caret does.
            let mut rects = self.paint_selection(text, &prep, scale).0;
            rects.extend(self.composing_underline(text, &prep, scale));
            return (rects, Vec::new(), Vec::new(), caret_rect);
        }
        let (selection, ink_override) = self.paint_selection(text, &prep, scale);
        let ink_override = ink_override.as_ref();
        // Backgrounds sit under the selection they surround; the composing underline sits
        // over both, because it is a line of the input rather than a tint.
        let mut rects = self.run_backgrounds(text, &prep, scale);
        rects.extend(selection);
        rects.extend(self.run_decorations(text, &prep, scale, ink_override));
        rects.extend(self.composing_underline(text, &prep, scale));

        let mut out = Vec::new();
        let mut lines = Vec::new();
        let number_style = text.block.runs.first().map(|run| run.style);
        let mut number = text.block.line_numbers.map(|numbers| numbers.start);
        for (index, line) in prep.layout.lines.iter().enumerate() {
            let range = line.range();
            let body = prep.flat.get(range.clone()).unwrap_or("");
            // The ellipsis is not part of the source range, so it is appended here.
            let owned = line
                .ellipsis
                .then(|| format!("{body}{}", ui_text::ELLIPSIS));
            let slice = owned.as_deref().unwrap_or(body);
            // The line's own baseline, from the layout that sized the block.
            let baseline = self.line_baseline(text, &prep, index);
            let x = self.line_x(text, &prep, index);
            if !slice.is_empty() {
                self.shape_line(
                    slice,
                    &prep.spans,
                    range.start,
                    baseline,
                    x,
                    line,
                    x,
                    Self::justify_extra(text, &prep, index),
                    scale,
                    text.ink,
                    ink_override,
                    Some(&mut out),
                    None,
                    prep.layout.rtl,
                );
            }
            // Only a source line carries a number: a line the wrap opened continues under
            // the one above, at the edge the gutter leaves it.
            if let (Some(numbers), Some(style)) = (text.block.line_numbers, number_style) {
                let starts_line =
                    range.start == 0 || prep.flat.as_bytes().get(range.start - 1) == Some(&b'\n');
                if starts_line {
                    let value = number.take().unwrap_or(numbers.start);
                    number = Some(value + 1);
                    let label = value.to_string();
                    let style = ui_core::TextStyle {
                        size: numbers.size,
                        ..style
                    };
                    let before = out.len();
                    let advance = self.shape_span(
                        &label,
                        0,
                        style,
                        baseline,
                        x,
                        scale,
                        numbers.ink,
                        Some(&mut out),
                        None,
                        prep.layout.rtl,
                    );
                    let shift = -(numbers.gap + advance);
                    for glyph in &mut out[before..] {
                        glyph.rect.origin.x += shift;
                    }
                }
            }
            // An atom reserves a box; its object fills it. The faint block stays for a box
            // with no object, because an empty gap would read as a missing glyph.
            let top = Self::line_top(text, &prep) + line.y;
            for atom in &line.atoms {
                // A held node paints itself in tree order; the pass leaves its bytes blank.
                // The layout is shared between nodes that measured alike, so the child is
                // read from this block rather than from the layout.
                if text
                    .block
                    .atom_at(atom.at)
                    .is_some_and(|boxed| boxed.holder.is_some())
                {
                    continue;
                }
                let rect = atom.rect(ui_core::geom::Point::new(x, top));
                let object = text
                    .block
                    .atoms
                    .iter()
                    .find(|boxed| boxed.at == atom.at)
                    .and_then(|boxed| boxed.object.as_ref());
                let painted = match object {
                    Some(ui_core::InlineObject::Math {
                        formula,
                        family,
                        ink,
                    }) => match self.math_items(family, formula, *ink, rect.origin, scale) {
                        Some((math_rects, math_glyphs, math_lines)) => {
                            rects.extend(math_rects);
                            out.extend(math_glyphs);
                            lines.extend(math_lines);
                            true
                        }
                        None => false,
                    },
                    // The object's text is drawn on the line's own baseline, so it meets the
                    // characters around it however the atom box was measured.
                    Some(ui_core::InlineObject::Glyphs {
                        text: glyphs,
                        style,
                        ink,
                        offset,
                        ..
                    }) => {
                        // A stand-in narrower than its atom sits where the atom says, so a
                        // number's dot lines up with the ones above it.
                        let x = rect.origin.x + *offset;
                        self.shape_span(
                            glyphs,
                            0,
                            *style,
                            baseline,
                            x,
                            scale,
                            *ink,
                            Some(&mut out),
                            None,
                            prep.layout.rtl,
                        );
                        true
                    }
                    None => false,
                };
                if !painted {
                    let ink = text.ink;
                    rects.push(RectPrim {
                        rect,
                        radius: ui_core::Corners::all(ATOM_RADIUS),
                        fill: ui_core::Color::rgba(ink.r, ink.g, ink.b, ink.a * ATOM_ALPHA),
                        gradient: None,
                        border: None,
                        border_gradient: None,
                        notch: None,
                    });
                }
            }
        }

        // After the glyphs: the caret belongs on top of the text it splits. It was resolved
        // above, before the span test, which an empty block never passes.
        self.note_atlas_change();
        // A symbol is centred by its ink: its line box carries the font's ascent, and the
        // mark fills the em from the baseline, so the box centre and the ink centre differ.
        if is_symbol_block(&text.block) && text.height > 0.0 {
            let (top, bottom) = out.iter().fold((f32::MAX, f32::MIN), |(lo, hi), g| {
                (lo.min(g.rect.min_y()), hi.max(g.rect.max_y()))
            });
            if top < bottom {
                let dy = text.origin.y + text.height * 0.5 - (top + bottom) * 0.5;
                for glyph in &mut out {
                    glyph.rect.origin.y += dy;
                }
            }
        }
        (rects, out, lines, caret_rect)
    }

    /// One rect per visual line per run that tinted its own text.
    fn run_backgrounds(&mut self, text: &TextPrim, prep: &Prepared, scale: f32) -> Vec<RectPrim> {
        let mut out = Vec::new();
        for span in &prep.spans {
            if !span.visibility.painted() {
                continue;
            }
            let Some(fill) = span.background else {
                continue;
            };
            out.extend(
                self.selection_fragments_with(text, prep, span.range.clone(), scale)
                    .into_iter()
                    .map(|f| RectPrim {
                        rect: f.rect,
                        radius: ui_core::Corners::ZERO,
                        fill,
                        gradient: None,
                        border: None,
                        border_gradient: None,
                        notch: None,
                    }),
            );
        }
        out
    }

    /// Underlines and strikeouts: one line per visual line of each decorated run.
    ///
    /// Placement comes from the face's own `post`/`OS/2` metrics, so a line sits where the
    /// designer put it. The edge and the thickness are snapped to whole device pixels,
    /// because a one-pixel rule at a fractional offset spreads into two faint rows.
    /// One rule per decorated span and line, cut where the selection is: the selected
    /// part takes the selection's ink, so a link under a selection does not keep its own
    /// colour across the boundary.
    fn run_decorations(
        &mut self,
        text: &TextPrim,
        prep: &Prepared,
        scale: f32,
        selection: Option<&(std::ops::Range<usize>, ui_core::Color)>,
    ) -> Vec<RectPrim> {
        let mut out = Vec::new();
        // A line's boundaries are shaped once, however many spans decorate it.
        let mut shaped: Vec<Option<Vec<Boundary>>> = vec![None; prep.layout.lines.len()];
        for span in &prep.spans {
            if !span.visibility.painted() || !(span.underline.drawn() || span.strikethrough.drawn())
            {
                continue;
            }
            let Some(metrics) = self.line_metrics(&span.style) else {
                continue;
            };
            let size = span.style.size.max(1.0);
            let ink = span.ink.unwrap_or(text.ink);
            for (index, line) in prep.layout.lines.iter().enumerate() {
                let line_range = line.range();
                let from = span.range.start.max(line_range.start);
                let to = span.range.end.min(line_range.end);
                if from >= to {
                    continue;
                }
                let bounds =
                    shaped[index].get_or_insert_with(|| self.boundaries(text, prep, index, scale));
                let fallback = self.line_x(text, prep, index);
                let baseline = self.line_baseline(text, prep, index);
                // The pattern starts where the span does, so a piece the selection cut out
                // continues the run's dots and waves instead of restarting them.
                let origin = Self::x_at(bounds, from, fallback);
                for (range, selected) in ui_core::text::decoration_pieces(from..to, selection) {
                    let x0 = Self::x_at(bounds, range.start, fallback);
                    let x1 = Self::x_at(bounds, range.end, fallback);
                    let piece_ink = match (selected, selection) {
                        (true, Some((_, ink))) => *ink,
                        _ => ink,
                    };
                    let (left, width) = (x0.min(x1), (x1 - x0).abs());
                    for (style, decoration) in [
                        (span.underline, metrics.underline),
                        (span.strikethrough, metrics.strikeout),
                    ] {
                        if !style.drawn() {
                            continue;
                        }
                        let thickness =
                            (decoration.thickness * size * scale).round().max(1.0) / scale;
                        // No snapping: a rule that stands on a whole pixel while its letters
                        // ride the subpixel grid would slide against them as the text moves.
                        let top = baseline - decoration.position * size;
                        for rect in ui_core::text::decoration_rects(
                            style, left, width, top, thickness, origin,
                        ) {
                            out.push(RectPrim {
                                rect,
                                radius: ui_core::Corners::ZERO,
                                fill: piece_ink,
                                gradient: None,
                                border: None,
                                border_gradient: None,
                                notch: None,
                            });
                        }
                    }
                }
            }
        }
        out
    }

    /// One thin rect per visual line under the composing run.
    fn composing_underline(
        &mut self,
        text: &TextPrim,
        prep: &Prepared,
        scale: f32,
    ) -> Vec<RectPrim> {
        let Some(range) = text.composing.clone() else {
            return Vec::new();
        };
        self.selection_fragments_with(text, prep, range, scale)
            .into_iter()
            .map(|f| RectPrim {
                rect: ui_core::geom::Rect {
                    origin: ui_core::geom::Point::new(
                        f.rect.origin.x,
                        f.rect.origin.y + f.rect.size.h - COMPOSING_UNDERLINE,
                    ),
                    size: ui_core::geom::Size {
                        w: f.rect.size.w,
                        h: COMPOSING_UNDERLINE,
                    },
                },
                radius: ui_core::Corners::ZERO,
                fill: text.ink,
                gradient: None,
                border: None,
                border_gradient: None,
                notch: None,
            })
            .collect()
    }

    /// Ask the node's painter for the highlight, and for any ink override.
    fn paint_selection(
        &mut self,
        text: &TextPrim,
        prep: &Prepared,
        scale: f32,
    ) -> (
        Vec<RectPrim>,
        Option<(std::ops::Range<usize>, ui_core::Color)>,
    ) {
        let Some(selection) = &text.selection else {
            return (Vec::new(), None);
        };
        let fragments = self.selection_fragments_with(text, prep, selection.range.clone(), scale);
        if fragments.is_empty() {
            return (Vec::new(), None);
        }
        let geometry = ui_core::SelectionGeometry {
            fragments: &fragments,
            fill: selection.fill,
            ink: text.ink,
        };
        let paint = match selection.painter {
            Some(painter) => (painter.0).paint(&geometry),
            None => ui_core::flat_selection(&geometry),
        };
        let range =
            selection.range.start.min(prep.flat.len())..selection.range.end.min(prep.flat.len());
        (paint.rects, paint.ink.map(|ink| (range, ink)))
    }

    /// Flatten and take the wrapped lines; paint and every query start from this.
    fn prepare(&mut self, text: &TextPrim) -> Prepared {
        let (flat, spans) = text.block.flatten();
        // Layout already wrapped this block; a prim built outside layout gets it here once.
        let layout = match &text.layout {
            Some(layout) => std::sync::Arc::clone(layout),
            None => self.layout(&text.block, text.max_width),
        };
        Prepared {
            flat,
            spans,
            layout,
        }
    }

    /// Left edge of a line, honouring the block's alignment and first-line indent.
    fn line_x(&self, text: &TextPrim, prep: &Prepared, index: usize) -> f32 {
        self.line_origin(text, prep, index).x
    }

    /// Top-left corner of a line, from the rule layout placement also uses.
    ///
    /// A widget inside a line is placed from this same call, so its rect and the caret
    /// cannot disagree about where the line starts.
    fn line_origin(&self, text: &TextPrim, prep: &Prepared, index: usize) -> ui_core::geom::Point {
        prep.layout.line_origin(
            index,
            text.origin,
            text.height,
            text.max_width,
            text.block.edges(),
            text.block.align,
        )
    }

    /// Space a justified line still needs to reach its target width.
    ///
    /// Zero for any other alignment, for the block's last line and for a line closed by a
    /// forced break — CSS leaves both flush, so the few words before a newline are never
    /// spread across the row. The target is the same box `line_x` aligns inside, so indent
    /// and alignment agree.
    fn justify_extra(text: &TextPrim, prep: &Prepared, index: usize) -> f32 {
        if text.block.align != ui_core::TextAlign::Justify
            || index + 1 >= prep.layout.lines.len()
            || Self::ends_at_forced_break(&prep.flat, &prep.layout.lines[index])
        {
            return 0.0;
        }
        let indent = if index == 0 {
            text.block.indent
        } else {
            text.block.hanging
        }
        .max(0.0);
        let target = text.max_width - indent;
        if !target.is_finite() {
            return 0.0;
        }
        (target - prep.layout.lines[index].width).max(0.0)
    }

    /// Whether the whitespace at a line's end holds a forced break.
    ///
    /// A wrapped line leaves a space the line hangs; a hard-broken one stops before the
    /// newline, its spaces kept, so the newline is the first byte past the range.
    fn ends_at_forced_break(flat: &str, line: &ui_core::InlineLine) -> bool {
        flat.get(line.range().end..)
            .unwrap_or("")
            .chars()
            .take_while(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{3000}'))
            .any(|c| c == '\n' || c == '\r')
    }

    /// Top of the line block: `origin` is the node's box top, and the block sits centred in
    /// a box taller than it, so an icon is not pinned to the top of its 1.4 em slot.
    fn line_top(text: &TextPrim, prep: &Prepared) -> f32 {
        // Line 0 begins at the block's own top, so the shared origin answers for the block.
        prep.layout
            .line_origin(
                0,
                text.origin,
                text.height,
                text.max_width,
                text.block.edges(),
                text.block.align,
            )
            .y
    }

    /// Baseline of a line, from the block's origin.
    fn line_baseline(&self, text: &TextPrim, prep: &Prepared, index: usize) -> f32 {
        let line = &prep.layout.lines[index];
        Self::line_top(text, prep) + line.y + line.baseline
    }

    /// Every char boundary of one line and where it sits, in visual (left-to-right) order.
    ///
    /// RTL runs contribute the *end* of a cluster at their left edge: the glyph's left
    /// side is where the reader has finished it, not where they started.
    fn boundaries(
        &mut self,
        text: &TextPrim,
        prep: &Prepared,
        index: usize,
        scale: f32,
    ) -> Vec<Boundary> {
        let layout = &prep.layout.lines[index];
        let range = layout.range();
        let body = prep.flat.get(range.clone()).unwrap_or("");
        let x = self.line_x(text, prep, index);
        let baseline = self.line_baseline(text, prep, index);
        let mut out = Vec::new();
        if !body.is_empty() {
            self.shape_line(
                body,
                &prep.spans,
                range.start,
                baseline,
                x,
                layout,
                x,
                Self::justify_extra(text, prep, index),
                scale,
                text.ink,
                None,
                None,
                Some(&mut out),
                prep.layout.rtl,
            );
        }
        // Boundaries come back relative to the line; callers index the whole block.
        let start = range.start;
        for b in &mut out {
            b.byte += start;
        }
        out
    }

    /// x of `byte` among visual boundaries; a byte inside a cluster takes the nearest one.
    fn x_at(bounds: &[Boundary], byte: usize, fallback: f32) -> f32 {
        let Some(first) = bounds.first() else {
            return fallback;
        };
        if let Some(hit) = bounds.iter().find(|b| b.byte == byte) {
            return hit.x;
        }
        let mut best = *first;
        for b in bounds {
            if b.byte.abs_diff(byte) < best.byte.abs_diff(byte) {
                best = *b;
            }
        }
        best.x
    }

    /// Whether the text at `byte` reads right to left.
    fn rtl_at(bounds: &[Boundary], byte: usize) -> bool {
        bounds
            .iter()
            .find(|b| b.byte == byte)
            .or_else(|| bounds.first())
            .is_some_and(|b| b.rtl)
    }

    /// The caret for `index`, or `None` when the block has no lines at all.
    fn caret_rect_with(
        &mut self,
        text: &TextPrim,
        prep: &Prepared,
        index: usize,
        scale: f32,
    ) -> Option<ui_core::geom::Rect> {
        if prep.layout.lines.is_empty() {
            return None;
        }
        let at = ui_core::text_input::clamp(&prep.flat, index);
        // A wrapped boundary belongs to the next line, which is where paint puts it too.
        let lines = &prep.layout.lines;
        let line = lines
            .iter()
            .position(|l| l.range().contains(&at))
            .or_else(|| lines.iter().position(|l| at <= l.range().end))
            .unwrap_or(lines.len() - 1);
        let bounds = self.boundaries(text, prep, line, scale);
        let fallback = self.line_x(text, prep, line);
        let x = Self::x_at(&bounds, at, fallback);
        let box_line = &lines[line];
        Some(ui_core::geom::Rect::new(
            x - CARET_WIDTH * 0.5,
            Self::line_top(text, prep) + box_line.y,
            CARET_WIDTH,
            box_line.height,
        ))
    }

    /// The cluster boundary nearest to a surface point; the click-to-caret primitive.
    pub fn index_at(&mut self, text: &TextPrim, p: ui_core::geom::Point, scale: f32) -> usize {
        let prep = self.prepare(text);
        let lines = &prep.layout.lines;
        if lines.is_empty() {
            return 0;
        }
        // Lines can differ in height, so pick by each one's own box, not by a fixed step.
        let rel = (p.y - Self::line_top(text, &prep)).max(0.0);
        let line = lines
            .iter()
            .position(|l| rel < l.bottom())
            .unwrap_or(lines.len() - 1);
        let bounds = self.boundaries(text, &prep, line, scale);
        let Some(first) = bounds.first() else {
            return lines[line].range().start;
        };
        let mut best = *first;
        for b in &bounds {
            if (b.x - p.x).abs() < (best.x - p.x).abs() {
                best = *b;
            }
        }
        // Shaped clusters are the boundaries, but a face may split inside one; snap back.
        ui_core::grapheme::snap(&prep.flat, best.byte)
    }

    /// The offset one line away from `index`, at the column `x`, or `None` when `index` sits
    /// on the view's last line in that direction.
    ///
    /// A vertical step is a step between lines, and a point on a line's edge is a point
    /// either line can claim: aiming at `caret.y + line height` landed back on the caret's
    /// own line whenever the two floats missed the edge by one ulp, and the step then left
    /// the view instead of moving down it.
    pub fn line_step(
        &mut self,
        text: &TextPrim,
        index: usize,
        delta: isize,
        x: f32,
        scale: f32,
    ) -> Option<usize> {
        let prep = self.prepare(text);
        let lines = &prep.layout.lines;
        if lines.is_empty() {
            return None;
        }
        let index = index.min(prep.flat.len());
        let from = lines
            .iter()
            .position(|line| line.range().contains(&index))
            .or_else(|| lines.iter().position(|line| line.range().end >= index))
            .unwrap_or(lines.len() - 1);
        let to = from.checked_add_signed(delta)?;
        let range = lines.get(to)?.range();
        let bounds = self.boundaries(text, &prep, to, scale);
        let Some(first) = bounds.first() else {
            return Some(range.start);
        };
        let mut best = *first;
        for b in &bounds {
            if (b.x - x).abs() < (best.x - x).abs() {
                best = *b;
            }
        }
        Some(ui_core::grapheme::snap(&prep.flat, best.byte))
    }

    /// Where the caret for `index` sits, in surface coordinates.
    pub fn caret_rect(
        &mut self,
        text: &TextPrim,
        index: usize,
        scale: f32,
    ) -> Option<ui_core::geom::Rect> {
        let prep = self.prepare(text);
        self.caret_rect_with(text, &prep, index, scale)
    }

    /// One rect per visual line piece the selection covers, in surface coordinates.
    pub fn selection_fragments(
        &mut self,
        text: &TextPrim,
        range: std::ops::Range<usize>,
        scale: f32,
    ) -> Vec<ui_core::SelectionFragment> {
        let prep = self.prepare(text);
        self.selection_fragments_with(text, &prep, range, scale)
    }

    /// The same fragments, from a block that is already prepared.
    fn selection_fragments_with(
        &mut self,
        text: &TextPrim,
        prep: &Prepared,
        range: std::ops::Range<usize>,
        scale: f32,
    ) -> Vec<ui_core::SelectionFragment> {
        let mut out = Vec::new();
        let text_end = prep.flat.len();
        let start = range.start.min(text_end);
        let end = range.end.min(text_end);
        // A view may name the break that follows its text, which no glyph of this node
        // draws: a range that reaches past the text marks that break alone.
        let trailing = range.end > text_end;
        if start >= end && !trailing {
            return out;
        }
        for index in 0..prep.layout.lines.len() {
            let line = &prep.layout.lines[index];
            let line_end = line.range().end;
            let from = start.max(line.range().start);
            let to = end.min(line_end);
            // A break belongs to the line it ends, and is selected only whole.
            let gap = ui_core::text::break_len(&prep.flat, line_end);
            let takes_break =
                gap > 0 && range.start <= line_end && range.end >= line_end.saturating_add(gap);
            // Shaping a line is the whole cost of a fragment, so a line the range never
            // reaches must not be shaped: a selected code card would otherwise shape every
            // line once per highlighted span, and that is most of a drag's frame.
            if from >= to && !takes_break {
                continue;
            }
            let bounds = self.boundaries(text, prep, index, scale);
            let fallback = self.line_x(text, prep, index);
            let top = Self::line_top(text, prep) + line.y;
            if from < to {
                let x0 = Self::x_at(&bounds, from, fallback);
                let x1 = Self::x_at(&bounds, to, fallback);
                out.push(ui_core::SelectionFragment {
                    rect: ui_core::geom::Rect::new(x0.min(x1), top, (x1 - x0).abs(), line.height),
                    line: index,
                    first: false,
                    last: false,
                    rtl: Self::rtl_at(&bounds, from),
                    line_break: false,
                });
            }
            if takes_break {
                out.push(Self::break_mark(
                    &bounds, line, index, line_end, fallback, top,
                ));
            }
        }
        if trailing && let Some((index, line)) = prep.layout.lines.iter().enumerate().next_back() {
            let bounds = self.boundaries(text, prep, index, scale);
            let fallback = self.line_x(text, prep, index);
            let top = Self::line_top(text, prep) + line.y;
            out.push(Self::break_mark(
                &bounds,
                line,
                index,
                line.range().end,
                fallback,
                top,
            ));
        }
        if let Some(first) = out.iter_mut().find(|f| !f.line_break) {
            first.first = true;
        }
        if let Some(last) = out.iter_mut().rev().find(|f| !f.line_break) {
            last.last = true;
        }
        out
    }

    /// The block a selected line break draws, since no glyph covers its bytes: a small
    /// square past the line's last cluster, centred in the line box.
    fn break_mark(
        bounds: &[Boundary],
        line: &ui_core::InlineLine,
        index: usize,
        at: usize,
        fallback: f32,
        top: f32,
    ) -> ui_core::SelectionFragment {
        let end_x = Self::x_at(bounds, at, fallback);
        let side = line.height * BREAK_MARK;
        let rtl = Self::rtl_at(bounds, at);
        ui_core::SelectionFragment {
            rect: ui_core::geom::Rect::new(
                if rtl { end_x - side } else { end_x },
                top,
                side,
                line.height,
            ),
            line: index,
            first: false,
            last: false,
            rtl,
            line_break: true,
        }
    }

    /// Measured size for layout: the same breaker and shaping the painter uses.
    pub fn measure(&mut self, block: &ui_core::TextBlock, max_width: f32) -> ui_core::Size {
        self.layout(block, max_width).size()
    }

    /// Smallest and largest size: the widest unbreakable segment, and the unwrapped line.
    pub fn intrinsic(&mut self, block: &ui_core::TextBlock) -> (ui_core::Size, ui_core::Size) {
        let max = self.measure(block, f32::INFINITY);
        // The floor is width-independent, so it caches like the wrap does; without this every
        // layout reshapes the block slice by slice just to re-measure the same floor.
        let key = measure_key(block, f32::INFINITY, self.fonts_version);
        let min_w = match self.min_widths.get(&key) {
            Some(w) => *w,
            None => {
                let (flat, spans) = block.flatten();
                let rtl = ui_text::paragraph_direction(&flat).is_rtl();
                let w = ui_text::min_content_width(&flat, |_slice, range| {
                    self.measure_range(&flat, &spans, &block.atoms, range, rtl)
                });
                if self.min_widths.len() >= MEASURE_CACHE_MAX {
                    self.min_widths.clear();
                }
                self.min_widths.insert(key, w);
                w
            }
        };
        (ui_core::Size::new(min_w, max.h), max)
    }

    /// The cached wrap for this block at this width, computed once.
    pub fn layout(
        &mut self,
        block: &ui_core::TextBlock,
        max_width: f32,
    ) -> std::sync::Arc<ui_core::InlineLayout> {
        let key = measure_key(block, max_width, self.fonts_version);
        if let Some(hit) = self.measured.get(&key) {
            return std::sync::Arc::clone(hit);
        }
        let computed = std::sync::Arc::new(timed!("measure", self.layout_uncached(block, max_width)));
        self.measure_misses += 1;
        if self.measured.len() >= MEASURE_CACHE_MAX {
            self.measured.clear();
        }
        self.measured.insert(key, std::sync::Arc::clone(&computed));
        computed
    }

    fn layout_uncached(
        &mut self,
        block: &ui_core::TextBlock,
        max_width: f32,
    ) -> ui_core::InlineLayout {
        let (flat, spans) = block.flatten();
        // The paragraph's direction, decided once: a line of digits inside an RTL paragraph
        // has no strong character of its own, and reading it alone would flip its neutrals.
        let rtl = ui_text::paragraph_direction(&flat).is_rtl();
        // Face metrics per span: a line's box and its baseline both come from the face.
        let boxes: Vec<(std::ops::Range<usize>, f32, f32, f32)> = spans
            .iter()
            .map(|span| {
                let (ascent, descent, line_h) = self.face_metrics(&span.style);
                (span.range.clone(), ascent, descent, line_h)
            })
            .collect();
        // Every run empty — a blank line, an empty fence — flattens to no span at all, but
        // the line is still a line box. Without a face it would measure one pixel and the
        // source's blank lines would collapse; the runs' own style names the face to use.
        let empty_box = match boxes.is_empty() {
            true => block.runs.first().map(|run| self.face_metrics(&run.style)),
            false => None,
        };
        let lines = self.break_into_lines(
            &flat,
            &spans,
            &block.atoms,
            max_width,
            block.max_lines,
            block.indent,
            block.hanging,
            rtl,
        );
        let mut out: Vec<ui_core::InlineLine> = Vec::with_capacity(lines.len());
        let mut y = 0.0f32;
        for line in lines {
            let on_line: Vec<&ui_core::AtomBox> = block
                .atoms
                .iter()
                .filter(|a| a.at >= line.range.start && a.at < line.range.end)
                .collect();
            let mut ascent = 0.0f32;
            let mut descent = 0.0f32;
            let mut box_h = 0.0f32;
            if let Some((a, d, h)) = empty_box {
                ascent = a;
                descent = d;
                box_h = h;
            }
            for (span, a, d, h) in &boxes {
                // An empty line (the one a hard break opens) overlaps nothing, so it takes
                // the box of the span it sits at; a non-empty line takes what it overlaps.
                let hit = if line.range.start == line.range.end {
                    span.start <= line.range.start && span.end >= line.range.end
                } else {
                    span.start < line.range.end && span.end > line.range.start
                };
                if hit {
                    ascent = ascent.max(*a);
                    descent = descent.max(*d);
                    box_h = box_h.max(*h);
                }
            }
            let (height, baseline) = ui_core::text::line_metrics(ascent, descent, box_h, &on_line);
            let mut atoms = Vec::new();
            for atom in &on_line {
                let x =
                    self.measure_range(&flat, &spans, &block.atoms, line.range.start..atom.at, rtl);
                atoms.push(ui_core::InlineAtom {
                    at: atom.at,
                    len: atom.len,
                    x,
                    y: atom.align.top(atom.size, atom.baseline, baseline, height),
                    size: atom.size,
                    align: atom.align,
                });
            }
            out.push(ui_core::InlineLine {
                range: line.range.clone(),
                y,
                height,
                baseline,
                width: line.width,
                ellipsis: line.ellipsis,
                runs: ui_core::text::runs_in(block, &spans, line.range.clone()),
                atoms,
            });
            // A gap sits between lines only; the last line adds none.
            y += height + block.line_gap;
        }
        // An empty block still owns a caret line: without one, `caret_rect` has no origin
        // and an empty text field could never place the candidate window.
        let height = if out.is_empty() {
            let height = boxes
                .first()
                .map_or(ui_core::TextStyle::default().size * 1.2, |b| b.1 + b.2);
            out.push(ui_core::InlineLine {
                range: 0..0,
                y: 0.0,
                height,
                baseline: height * 0.8,
                width: 0.0,
                ellipsis: false,
                runs: Vec::new(),
                atoms: Vec::new(),
            });
            height
        } else {
            (y - block.line_gap).max(0.0)
        };
        let width = out.iter().enumerate().fold(0.0f32, |acc, (index, line)| {
            let left = if index == 0 {
                block.indent
            } else {
                block.hanging
            }
            .max(0.0);
            acc.max(line.width + left)
        });
        let baseline = out.first().map_or(0.0, |l| l.baseline);
        ui_core::InlineLayout {
            lines: out,
            width,
            height,
            baseline,
            rtl,
        }
    }

    /// One line's box from the face that answers `style`, in pixels: the same numbers a
    /// shaped span gets, so an unshaped line and a shaped one stand on the same baseline.
    fn face_metrics(&mut self, style: &ui_core::TextStyle) -> (f32, f32, f32) {
        let size = style.size.max(1.0);
        let m = self.line_metrics(style);
        let ascent = m.map_or(0.8, |m| m.ascent) * size;
        let descent = m.map_or(0.3, |m| -m.descent) * size;
        let line_h = m.map_or(1.2, |m| (m.ascent - m.descent + m.line_gap).max(0.0)) * size;
        (ascent, descent, line_h)
    }

    /// Metrics of the face that answers `style`'s stack, admitting it on first use.
    fn line_metrics(&mut self, style: &TextStyle) -> Option<ui_text_fontique::RealMetrics> {
        let face = *self
            .stack_for(style.family, style.weight, style.italic)
            .first()?;
        self.admit_face(face);
        self.cache.face_metrics(face)
    }

    fn break_into_lines(
        &mut self,
        text: &str,
        spans: &[ui_core::Span],
        atoms: &[ui_core::AtomBox],
        max_width: f32,
        max_lines: Option<u32>,
        indent: f32,
        hanging: f32,
        rtl: bool,
    ) -> Vec<ui_text::Line> {
        // The ellipsis is drawn in the style of the text it replaces: the last span's.
        let ellipsis = self.measure_slice(
            ui_text::ELLIPSIS,
            ui_core::TextBlock::style_at(spans, text.len()),
            rtl,
        );
        timed!(
            "linebreak",
            ui_text::wrap_indented(
                text,
                max_width,
                max_lines,
                indent,
                hanging,
                ellipsis,
                |_slice, range| { self.measure_range(text, spans, atoms, range, rtl) }
            )
        )
    }

    /// Width of `range`, with every atom counted at its own size and its U+FFFC at none.
    fn measure_range(
        &mut self,
        text: &str,
        spans: &[ui_core::Span],
        atoms: &[ui_core::AtomBox],
        range: std::ops::Range<usize>,
        rtl: bool,
    ) -> f32 {
        let mut total = 0.0;
        let mut from = range.start;
        for atom in atoms
            .iter()
            .filter(|a| a.at >= range.start && a.at < range.end)
        {
            if atom.at > from {
                total += self.measure_styled(text, spans, from, atom.at, rtl);
            }
            total += atom.size.w;
            from = atom.at + atom.len;
        }
        if from < range.end {
            total += self.measure_styled(text, spans, from, range.end, rtl);
        }
        total
    }

    /// Width of `text[from..to]`, cut at every span boundary and measured piece by piece.
    ///
    /// The breaker hands over whole segments, and a segment can cross a style change; one
    /// style for the whole piece would wrap bold or oversized text as if it were the first.
    fn measure_styled(
        &mut self,
        text: &str,
        spans: &[ui_core::Span],
        from: usize,
        to: usize,
        rtl: bool,
    ) -> f32 {
        let mut total = 0.0;
        let mut at = from;
        for span in spans
            .iter()
            .filter(|s| s.range.start < to && s.range.end > from)
        {
            let (start, end) = (at.max(span.range.start), to.min(span.range.end));
            if start >= end {
                continue;
            }
            if start > at {
                total += self.measure_slice(
                    text.get(at..start).unwrap_or(""),
                    ui_core::TextBlock::style_at(spans, at),
                    rtl,
                );
            }
            // A collapsed run keeps its bytes for offsets and contributes no width, so the
            // wrap breaks exactly where the visible text would. A covered run keeps its
            // room: its stand-in paints where the bytes would have. A roomed run takes the
            // width it declares, whole, so the column beside it never depends on its glyphs.
            if span.visibility.takes_room() {
                match span
                    .room
                    .filter(|_| start == span.range.start && end == span.range.end)
                {
                    Some(room) => total += room.width,
                    None => {
                        total +=
                            self.measure_slice(text.get(start..end).unwrap_or(""), span.style, rtl)
                    }
                }
            }
            at = end;
        }
        if at < to {
            total += self.measure_slice(
                text.get(at..to).unwrap_or(""),
                ui_core::TextBlock::style_at(spans, at),
                rtl,
            );
        }
        total
    }

    fn measure_slice(&mut self, line: &str, style: ui_core::TextStyle, rtl: bool) -> f32 {
        let key = shape_key(line, &style, self.fonts_version, rtl);
        if let Some(&width) = self.slice_widths.get(&key) {
            return width;
        }
        let width = timed!("slice", {
            let mut total = 0.0;
            for run in ui_text::itemize(line, base_direction(rtl)) {
                let slice = line.get(run.range.clone()).unwrap_or("");
                if slice.is_empty() {
                    continue;
                }
                let script = run.script;
                // The same split the painter uses, or measurement and paint drift apart.
                total += self
                    .shape_run_parts(line, run.range.clone(), style, script, run.direction())
                    .iter()
                    .map(|(_, shaped, _, _)| shaped.width)
                    .sum::<f32>();
            }
            total
        });
        if self.slice_widths.len() >= SHAPE_CACHE_MAX {
            self.slice_widths.clear();
        }
        self.slice_widths.insert(key, width);
        width
    }

    /// Lay one line out, switching style at every span boundary, in the order the line's
    /// own bidi runs put its pieces.
    fn shape_line(
        &mut self,
        line: &str,
        spans: &[ui_core::Span],
        base: usize,
        baseline: f32,
        mut pen_x: f32,
        layout: &ui_core::InlineLine,
        line_left: f32,
        justify: f32,
        scale: f32,
        ink: ui_core::Color,
        ink_override: Option<&(std::ops::Range<usize>, ui_core::Color)>,
        mut out: Option<&mut Vec<GlyphPrim>>,
        mut bounds: Option<&mut Vec<Boundary>>,
        paragraph_rtl: bool,
    ) -> f32 {
        let end = base + line.len();
        // Bidi is a property of the whole line, and the line's direction is its paragraph's:
        // cutting at a span boundary or a selection edge must not restart either, or the
        // halves of one Arabic word swap places.
        let runs = ui_text::itemize(line, base_direction(paragraph_rtl));
        // Justification widens the gaps a reader already sees; a line with an RTL run is
        // left flush, because its reversed placement would need the gaps mirrored as well.
        let gaps = if justify > 0.0 && runs.iter().all(|run| !run.is_rtl()) {
            ui_text::justify_gaps(line, justify)
        } else {
            Vec::new()
        };
        let run_at = |offset: usize| {
            runs.iter()
                .position(|run| run.range.contains(&offset.saturating_sub(base)))
                .unwrap_or(0)
        };
        // Pieces in logical order. An atom is a piece with no glyph, so the pen still steps
        // over its box; it is painted from the layout, not from a shape.
        let mut pieces: Vec<(usize, usize, ui_core::TextStyle, ui_core::Color)> = Vec::new();
        let mut cursor = base;
        while cursor < end {
            let style = ui_core::TextBlock::style_at(spans, cursor);
            // Selection beats the run's ink, the run beats the node's: a selected link inverts.
            let ink_of = |at: usize| match ink_override {
                Some((range, colour)) if range.start <= at && at < range.end => *colour,
                _ => ui_core::TextBlock::ink_at(spans, at).unwrap_or(ink),
            };
            if let Some(atom) = layout.atoms.iter().find(|atom| atom.at == cursor) {
                pieces.push((cursor, cursor + atom.len, style, ink));
                cursor += atom.len;
                continue;
            }
            let span_end = spans
                .iter()
                .find(|s| s.range.contains(&cursor))
                .map_or(end, |s| s.range.end.min(end));
            let run_end = runs
                .iter()
                .find(|run| run.range.contains(&(cursor - base)))
                .map_or(end, |run| base + run.range.end);
            // Stop the piece at the next atom so its U+FFFC is never shaped, and at the
            // run's own end so no piece straddles two directions.
            let next_atom = layout
                .atoms
                .iter()
                .filter(|atom| atom.at > cursor)
                .map(|atom| atom.at)
                .min()
                .unwrap_or(end);
            let piece_end = span_end
                .max(cursor + 1)
                .min(end)
                .min(next_atom.max(cursor + 1))
                .min(run_end.max(cursor + 1));
            // Split at the selection boundary so the override ink covers exactly its bytes.
            let mut from = cursor;
            if let Some((range, _)) = ink_override {
                for cut in [range.start, range.end] {
                    // A cut inside a grapheme would shape the mark without its base, which is
                    // UAX #9 L3's detached mark in an RTL run and a lost accent anywhere.
                    let cut = (base + ui_core::grapheme::ceil(line, cut.saturating_sub(base)))
                        .min(piece_end);
                    if cut > from && cut < piece_end && line.is_char_boundary(cut - base) {
                        pieces.push((from, cut, style, ink_of(from)));
                        from = cut;
                    }
                }
            }
            pieces.push((from, piece_end, style, ink_of(from)));
            cursor = piece_end;
        }
        // A gap opens between two shaped pieces, so a piece that spans one would shape its
        // word pair as a single run and could never make room inside it.
        if !gaps.is_empty() {
            let mut split = Vec::with_capacity(pieces.len() + gaps.len());
            for (start, stop, style, piece_ink) in pieces {
                let mut from = start;
                for (at, _) in &gaps {
                    let cut = base + at;
                    let in_atom = layout
                        .atoms
                        .iter()
                        .any(|atom| atom.at < cut && cut < atom.at + atom.len);
                    // A room is one unit: a gap inside it would split it into two widths.
                    let in_room = ui_core::TextBlock::room_at(spans, cut).is_some();
                    if !in_atom
                        && !in_room
                        && cut > from
                        && cut < stop
                        && line.is_char_boundary(*at)
                    {
                        split.push((from, cut, style, piece_ink));
                        from = cut;
                    }
                }
                split.push((from, stop, style, piece_ink));
            }
            pieces = split;
        }
        // Visual order of the runs, and an RTL run's pieces laid out in reverse: their own
        // glyphs are already in visual order.
        let mut order: Vec<usize> = Vec::with_capacity(pieces.len());
        for run_index in ui_text::visual_order(&runs) {
            let mut in_run: Vec<usize> = (0..pieces.len())
                .filter(|index| run_at(pieces[*index].0) == run_index)
                .collect();
            if runs[run_index].is_rtl() {
                in_run.reverse();
            }
            order.extend(in_run);
        }
        for index in 0..pieces.len() {
            if !order.contains(&index) {
                order.push(index);
            }
        }
        for index in order {
            let (start, stop, style, piece_ink) = pieces[index];
            // The gap that opens after this piece, if a break opportunity sits at its end.
            let gap_after = gaps
                .iter()
                .find(|(at, _)| base + *at == stop)
                .map_or(0.0, |(_, delta)| *delta);
            // An atom occupies the bytes it stands for: no glyph, but it paints its box and
            // moves the pen past its width, with both edges still caret positions. It is
            // read before the collapse branch because those bytes are unpainted by design.
            if let Some(atom) = layout.atoms.iter().find(|atom| atom.at == start) {
                // The layout knows nothing of gaps, so the atom's own x is shifted here.
                let shift: f32 = gaps
                    .iter()
                    .filter(|(at, _)| base + *at <= start)
                    .map(|(_, delta)| *delta)
                    .sum();
                let left = line_left + atom.x + shift;
                let right = left + atom.size.w;
                if let Some(bounds) = bounds.as_deref_mut() {
                    bounds.push(Boundary {
                        byte: start,
                        x: left,
                        rtl: false,
                    });
                    bounds.push(Boundary {
                        byte: start + atom.len,
                        x: right,
                        rtl: false,
                    });
                }
                pen_x = right + gap_after;
                continue;
            }
            let visibility = ui_core::TextBlock::visibility_at(spans, start);
            if !visibility.takes_room() {
                // Collapsed syntax keeps its bytes so a caret offset still names the same
                // source byte, but it paints nothing and moves no pen.
                if let Some(bounds) = bounds.as_deref_mut() {
                    let rtl = runs
                        .iter()
                        .find(|run| run.range.contains(&(start - base)))
                        .is_some_and(|run| run.is_rtl());
                    bounds.push(Boundary {
                        byte: start,
                        x: pen_x,
                        rtl,
                    });
                    bounds.push(Boundary {
                        byte: stop,
                        x: pen_x,
                        rtl,
                    });
                }
                pen_x += gap_after;
                continue;
            }
            let slice = line.get(start - base..stop - base).unwrap_or("");
            if slice.is_empty() {
                pen_x += gap_after;
                continue;
            }
            // A roomed run keeps its declared width and lays its glyphs by its alignment,
            // with the room's edges as caret positions whatever the glyphs measure.
            if let Some(room) = ui_core::TextBlock::room_at(spans, start) {
                let shaped = self.measure_slice(slice, style, paragraph_rtl);
                if let Some(bounds) = bounds.as_deref_mut() {
                    bounds.push(Boundary {
                        byte: start,
                        x: pen_x,
                        rtl: false,
                    });
                }
                let glyphs = if visibility.painted() {
                    out.as_deref_mut()
                } else {
                    None
                };
                self.shape_span(
                    slice,
                    start - base,
                    style,
                    baseline,
                    pen_x + room.offset(shaped),
                    scale,
                    piece_ink,
                    glyphs,
                    bounds.as_deref_mut(),
                    paragraph_rtl,
                );
                pen_x += room.width;
                if let Some(bounds) = bounds.as_deref_mut() {
                    bounds.push(Boundary {
                        byte: stop,
                        x: pen_x,
                        rtl: false,
                    });
                }
                pen_x += gap_after;
                continue;
            }
            // A covered run is shaped for its geometry alone: its stand-in paints the room.
            let glyphs = if visibility.painted() {
                out.as_deref_mut()
            } else {
                None
            };
            pen_x += self.shape_span(
                slice,
                start - base,
                style,
                baseline,
                pen_x,
                scale,
                piece_ink,
                glyphs,
                bounds.as_deref_mut(),
                paragraph_rtl,
            );
            pen_x += gap_after;
        }
        pen_x
    }

    /// Shape one styled slice and return the advance it consumed.
    ///
    /// `out` and `bounds` are both optional: paint asks for glyphs, the geometry queries
    /// ask for boundaries, and neither pays for the other's work.
    fn shape_span(
        &mut self,
        slice: &str,
        offset: usize,
        style: ui_core::TextStyle,
        baseline: f32,
        mut pen_x: f32,
        scale: f32,
        ink: ui_core::Color,
        mut out: Option<&mut Vec<GlyphPrim>>,
        mut bounds: Option<&mut Vec<Boundary>>,
        paragraph_rtl: bool,
    ) -> f32 {
        let key = shape_key(slice, &style, self.fonts_version, paragraph_rtl);
        let shaped = match self.shaped_spans.get(&key) {
            Some(hit) => std::rc::Rc::clone(hit),
            None => {
                let built = std::rc::Rc::new(self.build_shaped(slice, style, paragraph_rtl));
                if self.shaped_spans.len() >= SHAPE_CACHE_MAX {
                    self.shaped_spans.clear();
                }
                self.shaped_spans.insert(key, std::rc::Rc::clone(&built));
                built
            }
        };
        let logical = ui_text::itemize(slice, base_direction(paragraph_rtl));
        let start = pen_x;
        for index in ui_text::visual_order(&logical) {
            let Some(parts) = shaped.get(index) else {
                continue;
            };
            let rtl = logical[index].direction().is_rtl();
            // RTL runs were shaped in logical order and must be placed in reverse.
            let ordered: Vec<&(FaceId, ShapedRun, usize, usize, Script)> = if rtl {
                parts.iter().rev().collect()
            } else {
                parts.iter().collect()
            };
            for (face, run, from, to, script) in ordered {
                let base = offset + *from;
                let run_len = to - from;
                let placed = place_run_scaled(
                    run,
                    pen_x,
                    baseline,
                    ui_text::subpixel::should_subpixel(*script),
                    scale,
                );
                if let Some(out) = out.as_deref_mut() {
                    // The outline is its own tiles, pushed under the fill: a stroke and the
                    // contour it came from must be one shaping answer, so both come from
                    // this same placed run.
                    if style.outline.paints() {
                        for blit in self.cache.blit_run_outlined(
                            *face,
                            &placed,
                            style.size,
                            scale,
                            style.outline.width,
                        ) {
                            out.push(GlyphPrim {
                                rect: blit.dest_logical(scale),
                                atlas: blit.atlas,
                                alpha: 1.0,
                                color: style.outline.color,
                                kind: blit.kind,
                            });
                        }
                    }
                    for blit in self.cache.blit_run(*face, &placed, style.size, scale) {
                        out.push(GlyphPrim {
                            rect: blit.dest_logical(scale),
                            atlas: blit.atlas,
                            alpha: 1.0,
                            color: ink,
                            kind: blit.kind,
                        });
                    }
                }
                if let Some(bounds) = bounds.as_deref_mut() {
                    record_boundaries(run, base, run_len, rtl, pen_x, bounds);
                }
                pen_x += run.width;
            }
        }
        pen_x - start
    }

    /// Shape one itemized run: one face when its clusters agree on presentation, else split.
    fn shape_run_parts(
        &mut self,
        whole: &str,
        range: std::ops::Range<usize>,
        style: TextStyle,
        script: Script,
        direction: ui_text::Direction,
    ) -> Vec<(FaceId, ShapedRun, usize, usize)> {
        let piece = whole.get(range.clone()).unwrap_or("");
        if let Some(emoji) = ui_text::emoji::uniform_presentation(piece)
            && let Some((face, shaped)) =
                self.shape_covering(whole, range.clone(), style, script, direction, emoji)
        {
            return vec![(
                face,
                track(shaped, style.letter_spacing),
                range.start,
                range.end,
            )];
        }
        self.shape_covering_split(whole, range, style, script, direction)
            .into_iter()
            .map(|(face, shaped, start, end)| {
                (face, track(shaped, style.letter_spacing), start, end)
            })
            .collect()
    }

    /// Shape one span into logical runs, each possibly split across several faces.
    fn build_shaped(
        &mut self,
        slice: &str,
        style: ui_core::TextStyle,
        paragraph_rtl: bool,
    ) -> Vec<Vec<ShapedPiece>> {
        let logical = ui_text::itemize(slice, base_direction(paragraph_rtl));
        let mut shaped: Vec<Vec<ShapedPiece>> = Vec::with_capacity(logical.len());
        for run in &logical {
            let piece = slice.get(run.range.clone()).unwrap_or("");
            if piece.is_empty() {
                shaped.push(Vec::new());
                continue;
            }
            let script = run.script;
            let parts =
                self.shape_run_parts(slice, run.range.clone(), style, script, run.direction());
            shaped.push(
                parts
                    .into_iter()
                    .map(|(face, shaped_run, start, end)| (face, shaped_run, start, end, script))
                    .collect(),
            );
        }
        shaped
    }

    /// Faces for a style, in priority order: the family the run names, then the configured
    /// families, then the generic choice, then the base stack.
    ///
    /// A plain family lookup lands on the family's default face, which is Regular —
    /// so asking for Bold without this would still rasterise Regular outlines.
    pub fn stack_for(
        &mut self,
        family: Family,
        weight: ui_core::Weight,
        italic: bool,
    ) -> Vec<FaceId> {
        let key = (family, weight.0, italic);
        if let Some(hit) = self.styled.get(&key) {
            return hit.clone();
        }
        let mut out: Vec<FaceId> = Vec::new();
        let unstyled = weight == ui_core::Weight::REGULAR && !italic;
        // The base stack already answers the default family at its default weight.
        if unstyled && family == Family::Generic(GenericFamily::SystemUi) {
            out = self.stack.clone();
        } else {
            let request = |family: Option<&str>, generic: GenericFamily| {
                let mut r = FontRequest::new(generic).weight(weight);
                r.italic = italic;
                if let Some(f) = family {
                    r = r.family(f);
                }
                r
            };
            let push = |face: FaceId, out: &mut Vec<FaceId>| {
                if !out.contains(&face) {
                    out.push(face);
                }
            };
            if let Family::Named(name) = family
                && let Some(face) = self
                    .provider
                    .borrow()
                    .resolve(&request(Some(name), GenericFamily::SansSerif))
            {
                push(face, &mut out);
            }
            if let Family::Generic(generic) = family
                && let Some(face) = self.provider.borrow().resolve(&request(None, generic))
            {
                push(face, &mut out);
            }
            for name in &self.families {
                if let Some(face) = self
                    .provider
                    .borrow()
                    .resolve(&request(Some(name), GenericFamily::SansSerif))
                {
                    push(face, &mut out);
                }
            }
            if let Some(face) = self
                .provider
                .borrow()
                .resolve(&request(None, GenericFamily::SansSerif))
            {
                push(face, &mut out);
            }
            // Coverage beats fidelity: a family with no bold face yields nothing above,
            // and losing the glyph is worse than losing the weight.
            for face in &self.stack {
                push(*face, &mut out);
            }
        }
        self.styled.insert(key, out.clone());
        out
    }

    /// First face whose shaping produces no `.notdef` glyph; `emoji_first` reorders the list.
    fn shape_covering(
        &mut self,
        whole: &str,
        range: std::ops::Range<usize>,
        style: TextStyle,
        script: Script,
        direction: ui_text::Direction,
        emoji_first: bool,
    ) -> Option<(FaceId, ShapedRun)> {
        timed!("shape", {
            let key = (style.family, script, style.weight.0, style.italic);
            let candidates = self.face_candidates(style, script, emoji_first);

            for base in candidates {
                // Coverage does not depend on the axes, so the cache above keys on the base
                // face; the instance is what shapes and rasterises.
                let face = self.provider.borrow_mut().instance(base, style.variations);
                let Some(shaper) = self.shaper_for(face) else {
                    continue;
                };
                let run = shaper.shape_run(
                    whole,
                    range.clone(),
                    style.size,
                    script,
                    direction,
                    style.features,
                );
                if run.is_empty() {
                    continue;
                }
                if run.glyphs.iter().all(|g| g.glyph_id != 0) {
                    self.script_face.insert(key, base);
                    return Some((face, run));
                }
            }
            None
        })
    }

    /// The faces to try for one run, in order; `emoji_first` puts emoji presentation ahead.
    fn face_candidates(
        &mut self,
        style: TextStyle,
        script: Script,
        emoji_first: bool,
    ) -> Vec<FaceId> {
        let key = (style.family, script, style.weight.0, style.italic);
        let mut out: Vec<FaceId> = Vec::new();
        fn push(face: FaceId, out: &mut Vec<FaceId>) {
            if !out.contains(&face) {
                out.push(face);
            }
        }
        // A remembered face answers only runs of its own presentation: an emoji face would
        // otherwise claim the plain clusters beside an emoji and draw them as empty components.
        if let Some(&known) = self.script_face.get(&key)
            && self.emoji.contains(&known) == emoji_first
        {
            out.push(known);
        }
        if emoji_first {
            for face in self.emoji.clone() {
                push(face, &mut out);
            }
        }
        for face in self.stack_for(style.family, style.weight, style.italic) {
            push(face, &mut out);
        }
        for face in self.provider.borrow().fallback_chain(script) {
            push(face, &mut out);
        }
        if !emoji_first {
            for face in self.emoji.clone() {
                push(face, &mut out);
            }
        }
        out
    }

    /// Shape a run no single face covers, splitting it where coverage changes.
    ///
    /// Emoji is the common case: no text face has it, and dropping the whole run made
    /// every character in it disappear. Splits fall on cluster boundaries, never inside one.
    fn shape_covering_split(
        &mut self,
        whole: &str,
        range: std::ops::Range<usize>,
        style: TextStyle,
        script: Script,
        direction: ui_text::Direction,
    ) -> Vec<(FaceId, ShapedRun, usize, usize)> {
        let plain = self.face_candidates(style, script, false);
        let emoji = self.face_candidates(style, script, true);
        let Some(&first) = plain.first() else {
            return Vec::new();
        };
        let mut groups: Vec<(usize, usize, FaceId)> = Vec::new();
        let body = whole.get(range.clone()).unwrap_or("");
        for (i, cluster) in ui_core::grapheme::clusters(body) {
            let start = range.start + i;
            let end = start + cluster.len();
            // Presentation belongs to the cluster, not to the run: `❤` stays text beside `❤️`.
            let list = match ui_text::emoji::asks_emoji(cluster) {
                true => &emoji,
                false => &plain,
            };
            let face = cluster
                .chars()
                .next()
                .and_then(|base| {
                    list.iter()
                        .copied()
                        .find(|&f| self.provider.borrow().has_glyph(f, base))
                })
                .unwrap_or_else(|| groups.last().map_or(first, |g| g.2));
            match groups.last_mut() {
                Some(g) if g.2 == face && g.1 == start => g.1 = end,
                _ => groups.push((start, end, face)),
            }
        }
        let mut out = Vec::new();
        for (start, end, base) in groups {
            let face = self.provider.borrow_mut().instance(base, style.variations);
            let Some(shaper) = self.shaper_for(face) else {
                continue;
            };
            let run = shaper.shape_run(
                whole,
                start..end,
                style.size,
                script,
                direction,
                style.features,
            );
            if run.is_empty() {
                continue;
            }
            out.push((face, run, start, end));
        }
        out
    }

    /// Put a face's metrics and outlines in the cache, once; false when it has no bytes.
    fn admit_face(&mut self, face: FaceId) -> bool {
        if self.cache.face_metrics(face).is_some() {
            return true;
        }
        let Some(data) = self.provider.borrow().face_data(face) else {
            return false;
        };
        if data.is_empty() {
            return false;
        }
        let axes = self.provider.borrow().face_axes(face);
        let index = self.provider.borrow().face_index(face) as u32;
        let source = (data.as_ptr() as usize, index);
        let admitted = self.cache.add_face_axes(
            face,
            data,
            index,
            self.provider.borrow().face_variation(face),
            axes,
        );
        // Only a font file the pass had never loaded can change line heights it cached; a new
        // axis instance of a known file leaves every other run's metrics alone.
        if admitted && self.font_files.insert(source) {
            self.forget_measurements();
        }
        admitted
    }

    /// Drop cached sizes and change the font generation so nothing stale is served.
    fn forget_measurements(&mut self) {
        self.measured.clear();
        self.min_widths.clear();
        self.slice_widths.clear();
        self.shaped_spans.clear();
        self.fonts_version = self.fonts_version.wrapping_add(1);
    }

    fn shaper_for(&mut self, face: FaceId) -> Option<&mut Shaper> {
        if !self.shapers.contains_key(&face) {
            if !self.admit_face(face) {
                return None;
            }
            let data = self.provider.borrow().face_data(face)?;
            let axes = self.provider.borrow().face_axes(face);
            let index = self.provider.borrow().face_index(face);
            let weight = self.provider.borrow().face_variation(face);
            let shaper = Shaper::new_with_axes(data, index, weight, axes)?;
            self.shapers.insert(face, shaper);
        }
        self.shapers.get_mut(&face)
    }
}

/// The base direction an itemizer takes from the paragraph's own.
fn base_direction(rtl: bool) -> Option<ui_text::Direction> {
    Some(if rtl {
        ui_text::Direction::RightToLeft
    } else {
        ui_text::Direction::LeftToRight
    })
}

/// One hash over everything that changes how a span shapes: its text, its style, the faces
/// and the paragraph direction, which reorders the mark and space runs around them.
fn shape_key(slice: &str, style: &TextStyle, fonts_version: u64, rtl: bool) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = FastHasher::default();
    fonts_version.hash(&mut h);
    slice.hash(&mut h);
    rtl.hash(&mut h);
    hash_style(style, &mut h);
    h.finish()
}

/// Bucket key for resolved text. Correctness rests on the exact comparison against the
/// cached primitive, so a collision only costs a recompute; the key just has to separate
/// the primitives a page really has.
fn text_key(text: &TextPrim, scale: f32, fonts_version: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = FastHasher::default();
    fonts_version.hash(&mut h);
    text.block.runs.len().hash(&mut h);
    text.block.atoms.len().hash(&mut h);
    text.max_width.to_bits().hash(&mut h);
    scale.to_bits().hash(&mut h);
    text.origin.x.to_bits().hash(&mut h);
    text.origin.y.to_bits().hash(&mut h);
    text.height.to_bits().hash(&mut h);
    // The layout allocation, kept alive by the cached entry, so the address cannot be reused.
    text.layout
        .as_ref()
        .map_or(0, |l| std::sync::Arc::as_ptr(l) as usize)
        .hash(&mut h);
    h.finish()
}

/// One hash over everything that changes the measured size of a block.
fn measure_key(block: &ui_core::TextBlock, max_width: f32, fonts_version: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = FastHasher::default();
    fonts_version.hash(&mut h);
    max_width.to_bits().hash(&mut h);
    block.max_lines.hash(&mut h);
    block.line_gap.to_bits().hash(&mut h);
    block.indent.to_bits().hash(&mut h);
    block.hanging.to_bits().hash(&mut h);
    for run in &block.runs {
        run.text.hash(&mut h);
        hash_style(&run.style, &mut h);
        // A collapsed run takes no advance, so revealing one changes where the lines break.
        // Without this bit the measured lines of the collapsed state are reused for the
        // shown text, and the longer line draws past the block's width.
        run.visibility.hash(&mut h);
        // A room replaces the run's advance, so two rooms measure differently.
        run.room
            .map(|room| (room.width.to_bits(), room.align as u8))
            .hash(&mut h);
    }
    // An atom's box changes where the line breaks, so it is part of the measured identity.
    for atom in &block.atoms {
        atom.at.hash(&mut h);
        atom.len.hash(&mut h);
        atom.size.w.to_bits().hash(&mut h);
        atom.size.h.to_bits().hash(&mut h);
        atom.baseline.to_bits().hash(&mut h);
        (atom.align as u8).hash(&mut h);
    }
    h.finish()
}

/// The style fields that reach face selection, shaping and metrics.
/// CSS `letter-spacing`: an extra advance after every cluster, the last one included.
///
/// Baking it into the shaped advances is what keeps the wrap, the painted glyphs and the
/// caret boundaries on one width; a separate spacing channel would drift between them.
fn track(mut run: ShapedRun, spacing: f32) -> ShapedRun {
    if spacing == 0.0 || run.glyphs.is_empty() {
        return run;
    }
    let mut clusters = 0.0f32;
    for index in 0..run.glyphs.len() {
        let last = run
            .glyphs
            .get(index + 1)
            .is_none_or(|next| next.cluster != run.glyphs[index].cluster);
        if last {
            run.glyphs[index].advance += spacing;
            clusters += 1.0;
        }
    }
    run.width += spacing * clusters;
    run
}

fn hash_style(style: &TextStyle, h: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    style.size.to_bits().hash(h);
    style.weight.0.hash(h);
    style.italic.hash(h);
    match style.family {
        Family::Generic(generic) => {
            0u8.hash(h);
            (generic as u8).hash(h);
        }
        Family::Named(name) => {
            1u8.hash(h);
            name.hash(h);
        }
    }
    // Quantised exactly as the face instance is: an animated axis then reuses one entry per
    // step instead of minting a key — and a reshape — on every frame.
    style.variations.quantized().hash(h);
    style.letter_spacing.to_bits().hash(h);
    for (tag, value) in style.features.iter() {
        tag.hash(h);
        value.hash(h);
    }
}

/// Record where each glyph's cluster starts, in visual order.
///
/// A glyph's left edge is the boundary *before* it in LTR and the boundary *after* its
/// cluster in RTL, because an RTL run is laid out left to right while reading right to left.
fn record_boundaries(
    run: &ShapedRun,
    base: usize,
    run_len: usize,
    rtl: bool,
    pen_x: f32,
    out: &mut Vec<Boundary>,
) {
    if run.glyphs.is_empty() {
        return;
    }
    let mut clusters: Vec<u32> = run.glyphs.iter().map(|g| g.cluster).collect();
    clusters.sort_unstable();
    let after = |c: u32| {
        clusters
            .iter()
            .copied()
            .find(|s| *s > c)
            .unwrap_or(run_len as u32)
    };
    let mut gx = pen_x;
    for glyph in &run.glyphs {
        let byte = if rtl {
            base + after(glyph.cluster) as usize
        } else {
            base + glyph.cluster as usize
        };
        out.push(Boundary { byte, x: gx, rtl });
        gx += glyph.advance;
    }
    let tail = if rtl { base } else { base + run_len };
    out.push(Boundary {
        byte: tail,
        x: gx,
        rtl,
    });
}

#[cfg(test)]
mod tests;
