//! Text shaping: turns a run of text into positioned glyphs.

use skrifa::MetadataProvider as _;
use skrifa::instance::{LocationRef, Size};
use skrifa::raw::TableProvider as _;
use swash::shape::{Direction as SwashDirection, ShapeContext};
use swash::text::Script as SwashScript;

use ui_text::{Features, Phases, Script, SubpixelBin, Variations, VerticalBin};

/// One positioned glyph produced by shaping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapedGlyph {
    pub glyph_id: u16,
    /// Horizontal offset from the pen, in pixels (combining marks, mkmk).
    pub x: f32,
    /// Vertical offset from the baseline, in pixels, positive down.
    pub y: f32,
    /// Advance in pixels, after kerning and ligatures.
    pub advance: f32,
    /// Byte offset of the cluster this glyph belongs to.
    pub cluster: u32,
}

/// A shaped run: glyphs plus the total advance.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapedRun {
    pub glyphs: Vec<ShapedGlyph>,
    pub width: f32,
}

impl ShapedRun {
    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }
}

/// Reverse clusters for RTL display, keeping each cluster's glyphs in logical order.
///
/// swash's own contract: reversing glyphs places a mark one base-advance away from where
/// the font positioned it.
fn reverse_clusters(glyphs: &mut Vec<ShapedGlyph>) {
    let mut starts: Vec<usize> = Vec::new();
    for (index, glyph) in glyphs.iter().enumerate() {
        if index == 0 || glyph.cluster != glyphs[index - 1].cluster {
            starts.push(index);
        }
    }
    let mut out = Vec::with_capacity(glyphs.len());
    for (n, &start) in starts.iter().enumerate().rev() {
        let end = starts.get(n + 1).copied().unwrap_or(glyphs.len());
        out.extend_from_slice(&glyphs[start..end]);
    }
    *glyphs = out;
}

/// One directional run, in visual order, with glyphs in visual order.
#[derive(Clone, Debug, PartialEq)]
pub struct VisualRun {
    /// Byte range in the original text.
    pub range: std::ops::Range<usize>,
    /// Whether this run reads right to left.
    pub is_rtl: bool,
    pub run: ShapedRun,
}

impl VisualRun {
    pub fn is_rtl(&self) -> bool {
        self.is_rtl
    }

    pub fn width(&self) -> f32 {
        self.run.width
    }
}

/// A glyph placed at a quantized origin, ready to blit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    pub glyph_id: u16,
    /// Integer horizontal origin, in **device** pixels.
    pub origin_x: i32,
    /// Baseline for this glyph, in device pixels, including the glyph's own vertical offset
    /// (combining marks sit above the baseline).
    ///
    /// Exact rather than integral: [`VerticalBin`] takes the half pixel into the tile, and
    /// what it leaves over is a fractional destination the renderer samples through.
    pub baseline_y: f32,
    /// Which subpixel variant to fetch from the atlas.
    pub phases: Phases,
}

/// Place a shaped run at `pen_x` / `baseline_y`, quantizing to subpixel phases.
///
/// Horizontal positions keep a quarter-pixel phase so a run does not jitter when an earlier
/// character changes width; vertical takes the nearest half pixel, because a block of text
/// moves down a screen far more often than it changes width, and a whole-pixel grid there
/// makes it step while everything around it glides.
pub fn place_run(
    run: &ShapedRun,
    pen_x: f32,
    baseline_y: f32,
    use_subpixel: bool,
) -> Vec<PlacedGlyph> {
    place_run_scaled(run, pen_x, baseline_y, use_subpixel, 1.0)
}

/// Place a run for a device scale of `scale` device pixels per logical point.
///
/// Quantization happens in *device* space: at a fractional scale the same logical
/// origin maps to a different fractional device position, so quantizing in logical
/// space would leave up to one device pixel of jitter.
pub fn place_run_scaled(
    run: &ShapedRun,
    pen_x: f32,
    baseline_y: f32,
    use_subpixel: bool,
    scale: f32,
) -> Vec<PlacedGlyph> {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let mut pen = pen_x;
    let mut out = Vec::with_capacity(run.glyphs.len());
    for glyph in &run.glyphs {
        let x = (pen + glyph.x) * scale;
        let (device_x, bin) = if use_subpixel {
            SubpixelBin::split(x)
        } else {
            (x.round() as i32, SubpixelBin::Zero)
        };
        let y = (baseline_y - glyph.y) * scale;
        let (_, y_bin, _) = VerticalBin::split(y);
        let origin_x = device_x;
        out.push(PlacedGlyph {
            glyph_id: glyph.glyph_id,
            origin_x,
            baseline_y: y,
            phases: Phases::new(bin, y_bin),
        });
        pen += glyph.advance;
    }
    out
}

fn to_swash_script(script: Script) -> SwashScript {
    match &script.0 {
        b"Latn" => SwashScript::Latin,
        b"Hani" => SwashScript::Han,
        b"Arab" => SwashScript::Arabic,
        b"Hebr" => SwashScript::Hebrew,
        b"Cyrl" => SwashScript::Cyrillic,
        b"Grek" => SwashScript::Greek,
        b"Hira" => SwashScript::Hiragana,
        b"Hang" => SwashScript::Hangul,
        b"Thai" => SwashScript::Thai,
        b"Deva" => SwashScript::Devanagari,
        _ => SwashScript::Latin,
    }
}

/// A shaper for one face. `ShapeContext` is reusable; a `Shaper` is not.
pub struct Shaper {
    context: ShapeContext,
    font: crate::face::FaceData,
    /// Variable-font weight instance; `None` for a static face.
    weight: Option<f32>,
    /// Axes beyond weight this instance sits at.
    axes: Variations,
    /// Advance delta per glyph, in font units, for an instance whose advances live in
    /// `gvar` phantom points. Fixed for the face, so it is read once per glyph.
    instance_deltas: std::collections::HashMap<u16, f32>,
}

impl Shaper {
    pub fn new(data: impl Into<ui_text::FontBlob>, font_index: usize) -> Option<Self> {
        Self::new_with_weight(data, font_index, None)
    }

    /// A shaper for a variable-font instance at `weight`.
    pub fn new_with_weight(
        data: impl Into<ui_text::FontBlob>,
        font_index: usize,
        weight: Option<f32>,
    ) -> Option<Self> {
        Self::new_with_axes(data, font_index, weight, Variations::NONE)
    }

    /// A shaper for a variable-font instance at `weight` and the extra `axes`.
    pub fn new_with_axes(
        data: impl Into<ui_text::FontBlob>,
        font_index: usize,
        weight: Option<f32>,
        axes: Variations,
    ) -> Option<Self> {
        // Fail early if the bytes are not a font, and keep the parse for every later call.
        let font = crate::face::FaceData::parse(data, font_index)?;
        Some(Self {
            context: ShapeContext::new(),
            font,
            weight,
            axes,
            instance_deltas: std::collections::HashMap::new(),
        })
    }

    /// Shape `text` at `size`, with kerning and ligatures applied.
    pub fn shape(&mut self, text: &str, size: f32, script: Script) -> ShapedRun {
        self.shape_directed(
            text,
            size,
            script,
            ui_text::Direction::LeftToRight,
            Features::NONE,
        )
    }

    /// Shape `text` with an explicit direction and feature switches.
    ///
    /// Direction selects contextual forms; it does **not** reorder glyphs, so an RTL
    /// run still comes back in logical order and must be reversed by the caller.
    pub fn shape_directed(
        &mut self,
        text: &str,
        size: f32,
        script: Script,
        direction: ui_text::Direction,
        features: Features,
    ) -> ShapedRun {
        // A shaper consumes itself when run, so build a fresh one per call.
        let mut builder = self.context.builder(self.font.font());
        // The axis must be set before shaping, or advances come from the default instance.
        let settings: Vec<(swash::Tag, f32)> =
            crate::face::variation_settings(self.weight, self.axes)
                .into_iter()
                .map(|(tag, value)| (swash::tag_from_bytes(&tag), value))
                .collect();
        if !settings.is_empty() {
            builder = builder.variations(settings);
        }
        let switches: Vec<(swash::Tag, u16)> = features
            .iter()
            .map(|(tag, value)| {
                (
                    swash::tag_from_bytes(&tag),
                    value.min(u16::MAX as u32) as u16,
                )
            })
            .collect();
        if !switches.is_empty() {
            builder = builder.features(switches);
        }
        let mut shaper = builder
            .script(to_swash_script(script))
            .size(size)
            .direction(match direction {
                ui_text::Direction::LeftToRight => SwashDirection::LeftToRight,
                ui_text::Direction::RightToLeft => SwashDirection::RightToLeft,
            })
            .build();
        shaper.add_str(text);

        let mut glyphs = Vec::new();
        shaper.shape_with(|cluster| {
            let start = cluster.source.start;
            for glyph in cluster.glyphs {
                glyphs.push(ShapedGlyph {
                    glyph_id: glyph.id,
                    x: glyph.x,
                    y: glyph.y,
                    advance: glyph.advance,
                    cluster: start,
                });
            }
        });
        self.correct_instance_advances(&mut glyphs, size);
        let width = glyphs.iter().map(|g| g.advance).sum();
        ShapedRun { glyphs, width }
    }

    /// Add the advance variation shoaping cannot see.
    ///
    /// A variable font without `HVAR` carries advance deltas in `gvar` phantom points,
    /// which swash's shaper ignores: a bold instance draws bold outlines on the regular
    /// advance and the glyphs collide. skrifa reads them, so its delta is added to the
    /// positioned advance — kerning and mark offsets stay as shaping produced them.
    fn correct_instance_advances(&mut self, glyphs: &mut [ShapedGlyph], size: f32) {
        if self.weight.is_none() && self.axes == Variations::NONE {
            return;
        }
        let font = self.font.outline_font();
        // HVAR is what swash reads; without gvar there are no phantom points either.
        let table = |tag: &[u8; 4]| font.table_data(skrifa::Tag::new(tag)).is_some();
        if table(b"HVAR") || !table(b"gvar") {
            return;
        }
        let coords = crate::face::instance_coords(font, self.weight, self.axes);
        if coords.is_empty() {
            return;
        }
        let upem = font.head().map(|head| head.units_per_em()).unwrap_or(1000) as f32;
        let instance = font.glyph_metrics(Size::unscaled(), LocationRef::new(&coords));
        let plain = font.glyph_metrics(Size::unscaled(), LocationRef::default());
        for glyph in glyphs {
            let units = match self.instance_deltas.get(&glyph.glyph_id) {
                Some(delta) => *delta,
                None => {
                    let id = skrifa::GlyphId::new(glyph.glyph_id as u32);
                    let delta = instance.advance_width(id).unwrap_or(0.0)
                        - plain.advance_width(id).unwrap_or(0.0);
                    self.instance_deltas.insert(glyph.glyph_id, delta);
                    delta
                }
            };
            glyph.advance += units / upem * size;
        }
    }

    /// Shape a whole paragraph: itemize by direction, shape each run, reorder.
    ///
    /// Returns runs in **visual** order, each with its glyphs already in visual order,
    /// so a caller can lay them out left to right with a single pen.
    /// Shape one logical run, applying RTL reversal and UAX#9 L4 bracketing.
    ///
    /// `TextPass` needs this per face: it picks a covering face per run, so it cannot
    /// use `shape_bidi`, which assumes one face for the whole string.
    pub fn shape_run(
        &mut self,
        whole: &str,
        range: std::ops::Range<usize>,
        size: f32,
        script: Script,
        direction: ui_text::Direction,
        features: Features,
    ) -> ShapedRun {
        let slice = whole.get(range.clone()).unwrap_or("");
        let mut run = self.shape_directed(slice, size, script, direction, features);
        // swash returns logical order even for RTL, so reverse here — by cluster, since a
        // mark must still follow the base it attaches to.
        if direction.is_rtl() {
            reverse_clusters(&mut run.glyphs);
        }
        run
    }

    pub fn shape_bidi(
        &mut self,
        text: &str,
        size: f32,
        base: Option<ui_text::Direction>,
    ) -> Vec<VisualRun> {
        let logical = ui_text::itemize(text, base);
        if logical.is_empty() {
            return Vec::new();
        }

        let shaped: Vec<VisualRun> = logical
            .iter()
            .map(|run| {
                let direction = run.direction();
                let shaped_run = self.shape_run(
                    text,
                    run.range.clone(),
                    size,
                    run.script,
                    direction,
                    Features::NONE,
                );
                VisualRun {
                    range: run.range.clone(),
                    is_rtl: direction.is_rtl(),
                    run: shaped_run,
                }
            })
            .collect();

        let order = ui_text::visual_order(&logical);
        // Moved out of their logical slots, not cloned: each run carries its whole glyph
        // vector, and this runs for every shaped paragraph.
        let mut slots: Vec<Option<VisualRun>> = shaped.into_iter().map(Some).collect();
        let mut reordered = Vec::with_capacity(order.len());
        for index in order {
            if let Some(run) = slots.get_mut(index).and_then(Option::take) {
                reordered.push(run);
            }
        }
        reordered
    }

    /// Glyph id for a character through the font's cmap.
    pub fn glyph_id(&self, c: char) -> Option<u16> {
        let gid = self.font.font().charmap().map(c);
        if gid == 0 { None } else { Some(gid) }
    }

    /// Apply UAX#9 rule L4: swap brackets for their mirror image.
    ///
    /// `unicode-bidi` hands this rule to the engine; without it an RTL run shows
    /// `)text(` instead of `(text)`.
    /// Advance for a single glyph id, without shaping context, at this instance.
    pub fn glyph_advance(&self, glyph_id: u16, size: f32) -> f32 {
        let font = self.font.outline_font();
        let coords = crate::face::instance_coords(font, self.weight, self.axes);
        let metrics = font.glyph_metrics(Size::unscaled(), LocationRef::new(&coords));
        let upem = font.head().map(|head| head.units_per_em()).unwrap_or(1000) as f32;
        metrics
            .advance_width(skrifa::GlyphId::new(glyph_id as u32))
            .unwrap_or(0.0)
            / upem
            * size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_rtl_run_reverses_clusters_without_disturbing_a_cluster() {
        // Clusters 0, 1, 4 with a mark inside cluster 4: the clusters swap places, the
        // base and its mark keep the order the font gave them.
        let glyph = |cluster, advance| ShapedGlyph {
            glyph_id: cluster as u16,
            x: 0.0,
            y: 0.0,
            advance,
            cluster,
        };
        let mut glyphs = vec![glyph(0, 5.0), glyph(1, 6.0), glyph(4, 7.0), glyph(4, 0.0)];
        reverse_clusters(&mut glyphs);
        let order: Vec<u32> = glyphs.iter().map(|g| g.cluster).collect();
        assert_eq!(
            order,
            vec![4, 4, 1, 0],
            "clusters reverse, the pair does not"
        );
    }

    /// A font that actually covers Arabic, for bidi tests.
    fn arabic_font() -> Option<ui_text::FontBlob> {
        crate::tests::tests_support::rtl_face()
    }

    fn system_font() -> Option<ui_text::FontBlob> {
        crate::tests::tests_support::sans_face()
    }

    #[test]
    fn an_instance_advance_follows_the_outlines_it_is_drawn_with() {
        // A variable font without HVAR keeps its advance deltas in gvar phantom points,
        // which swash's shaper never reads: the bold instance would draw bold outlines on
        // the regular advance and its glyphs would collide. skrifa reads them, so the pen
        // and the outline must agree on the same advance.
        use crate::FontProvider;
        let provider = crate::system_or_isolated();
        let Some(bold) = provider.resolve(
            &ui_text::FontRequest::new(ui_text::GenericFamily::SansSerif)
                .weight(ui_text::Weight::BOLD),
        ) else {
            eprintln!("skipped: no system fonts");
            return;
        };
        let (Some(weight), Some(data)) = (provider.face_variation(bold), provider.face_data(bold))
        else {
            eprintln!("skipped: the bold face is static, so no instance advance varies");
            return;
        };
        let index = provider.face_index(bold);
        let axes = provider.face_axes(bold);
        let mut shaper = Shaper::new_with_axes(data.clone(), index, Some(weight), axes).unwrap();
        let shaped = shaper.shape("H", 40.0, Script::LATIN).width;
        let face = crate::face::FaceData::parse(data, index).unwrap();
        let font = face.outline_font();
        let coords = crate::face::instance_coords(font, Some(weight), axes);
        let metrics = font.glyph_metrics(Size::unscaled(), LocationRef::new(&coords));
        let gid = font.charmap().map('H').expect("the face draws H");
        let upem = font.head().map(|head| head.units_per_em()).unwrap_or(1000) as f32;
        let outlined = metrics.advance_width(gid).unwrap_or(0.0) / upem * 40.0;
        assert!(
            (shaped - outlined).abs() < 0.01,
            "the pen must advance by the outline's width: shaped {shaped}, outlines {outlined}"
        );
    }

    #[test]
    fn kerning_makes_av_narrower_than_the_sum_of_advances() {
        let Some(data) = system_font() else { return };
        let mut s = Shaper::new(data, 0).expect("font");
        let run = s.shape("AV", 32.0, Script::LATIN);
        assert_eq!(run.glyphs.len(), 2);
        let unkerned = s.glyph_advance(run.glyphs[0].glyph_id, 32.0)
            + s.glyph_advance(run.glyphs[1].glyph_id, 32.0);
        assert!(
            run.width < unkerned,
            "kerning must shrink AV: shaped {} vs unkerned {}",
            run.width,
            unkerned
        );
    }

    #[test]
    fn place_run_keeps_the_subpixel_phase_on_both_axes() {
        let Some(data) = system_font() else { return };
        let mut s = Shaper::new(data, 0).expect("font");
        let run = s.shape("mmmm", 16.0, Script::LATIN);
        let placed = place_run(&run, 10.3, 20.7, true);
        assert_eq!(placed.len(), run.glyphs.len());
        // The baseline is exact; the tile takes the nearest half pixel of it.
        assert!(placed.iter().all(|p| (p.baseline_y - 20.7).abs() < 1e-5));
        assert!(placed.iter().all(|p| p.phases.y == VerticalBin::Half));
        // At least one glyph must carry a non-zero phase for a fractional origin.
        assert!(
            placed.iter().any(|p| p.phases.x != SubpixelBin::Zero),
            "fractional pen must produce a phase"
        );
        // A quarter-pixel move changes no phase but must still reach the baseline.
        let nudged = place_run(&run, 10.3, 20.9, true);
        assert!(nudged.iter().all(|p| p.phases.y == VerticalBin::Zero));
        assert!(nudged.iter().all(|p| (p.baseline_y - 20.9).abs() < 1e-5));
    }

    #[test]
    fn bidi_rtl_run_has_its_glyphs_reversed() {
        let Some(data) = arabic_font() else { return };
        let mut s = Shaper::new(data, 0).expect("font");
        let text = "مرحبا";
        let runs = s.shape_bidi(text, 32.0, None);
        assert_eq!(runs.len(), 1);
        assert!(runs[0].is_rtl(), "Arabic must be an RTL run");

        // In visual order the first glyph must come from the last logical cluster.
        let glyphs = &runs[0].run.glyphs;
        assert!(glyphs.len() >= 2);
        let first = glyphs.first().expect("glyph").cluster;
        let last = glyphs.last().expect("glyph").cluster;
        assert!(
            first > last,
            "RTL glyphs must be reversed: first cluster {first} should exceed last {last}"
        );
    }

    #[test]
    fn bidi_mixed_text_yields_several_runs_covering_the_text() {
        let Some(data) = arabic_font() else { return };
        let mut s = Shaper::new(data, 0).expect("font");
        let text = "abc مرحبا xyz";
        let runs = s.shape_bidi(text, 16.0, None);
        assert!(
            runs.len() >= 3,
            "expected at least 3 runs, got {}",
            runs.len()
        );
        assert!(runs.iter().any(|r| r.is_rtl()));
        assert!(runs.iter().any(|r| !r.is_rtl()));

        // Every byte of the text must belong to exactly one run.
        let mut covered = vec![false; text.len()];
        for run in &runs {
            for flag in &mut covered[run.range.clone()] {
                assert!(!*flag, "byte covered twice");
                *flag = true;
            }
        }
        assert!(covered.iter().all(|c| *c), "all bytes must be covered");
    }

    #[test]
    fn bidi_total_width_is_the_sum_of_visual_runs() {
        let Some(data) = arabic_font() else { return };
        let mut s = Shaper::new(data, 0).expect("font");
        let runs = s.shape_bidi("abc مرحبا xyz", 16.0, None);
        let total: f32 = runs.iter().map(|r| r.run.width).sum();
        assert!(total > 0.0);
        for run in &runs {
            assert!(run.run.width > 0.0, "every run contributes width");
        }
    }
}
