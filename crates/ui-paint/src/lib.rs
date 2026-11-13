//! CPU rasterizer: turns a `DrawList` into straight-alpha sRGB8 pixels.
//!
//! `zeno` supplies coverage only; compositing stays here so both future backends
//! share one blending definition.

use ui_core::Damage;
use ui_core::draw::{Batch, DrawList, GlyphKind, PrimKind, RectPrim, TextPrim};
use ui_core::effect::{ColorMatrix, Effect, Kernel};
use ui_core::geom::{Color, Corners, Rect};

/// A CPU pixel buffer, straight (non-premultiplied) sRGB8 in BGRA byte order.
#[derive(Clone, Debug)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    /// Premultiplied BGRA, `width * height * 4` bytes.
    pub data: Vec<u8>,
}

impl Pixels {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0; (width as usize) * (height as usize) * 4],
        }
    }

    pub fn clear(&mut self) {
        self.data.fill(0);
    }

    pub fn as_bgra(&self) -> &[u8] {
        &self.data
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Read one pixel as straight sRGB.
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y as usize) * (self.width as usize) + x as usize) * 4;
        [
            self.data[i + 2],
            self.data[i + 1],
            self.data[i],
            self.data[i + 3],
        ]
    }

    /// Fill the whole buffer with one straight-alpha colour.
    pub fn fill(&mut self, color: Color) {
        let c = Color::rgba(color.r, color.g, color.b, 1.0);
        for px in self.data.as_chunks_mut::<4>().0 {
            let mut tmp = [0u8; 4];
            blend_pixel(&mut tmp, c, 1.0);
            px.copy_from_slice(&tmp);
        }
    }

    /// Copy the region of `src` covered by `rect` into `self`.
    pub fn restore_region(&mut self, src: &Pixels, rect: Rect) {
        let (x0, y0, x1, y1) = clip_to(rect, self.width, self.height);
        let stride = self.width as usize * 4;
        for y in y0..y1 {
            let row = y as usize * stride;
            let a = row + x0 as usize * 4;
            let b = row + x1 as usize * 4;
            self.data[a..b].copy_from_slice(&src.data[a..b]);
        }
    }
}

/// FNV-1a over the bytes of one region, row by row.
///
/// A pixel chain reads only inside the rect it is given (both sampling passes clamp to
/// it), so these bytes are the whole input: equal hash and equal rect mean equal output.
fn hash_region(pixels: &Pixels, x0: u32, y0: u32, x1: u32, y1: u32) -> u64 {
    let stride = pixels.width as usize;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for y in y0..y1 {
        let from = (y as usize * stride + x0 as usize) * 4;
        let to = (y as usize * stride + x1 as usize) * 4;
        for byte in &pixels.data[from..to] {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

/// The region's bytes, row by row, without the stride between them.
fn region_bytes(pixels: &Pixels, x0: u32, y0: u32, x1: u32, y1: u32) -> Vec<u8> {
    let stride = pixels.width as usize;
    let row = (x1 - x0) as usize * 4;
    let mut out = Vec::with_capacity(row * (y1 - y0) as usize);
    for y in y0..y1 {
        let from = (y as usize * stride + x0 as usize) * 4;
        out.extend_from_slice(&pixels.data[from..from + row]);
    }
    out
}

/// Write `data` back over the same region it was taken from.
fn paste_region(pixels: &mut Pixels, x0: u32, y0: u32, width: u32, height: u32, data: &[u8]) {
    let stride = pixels.width as usize;
    let row = width as usize * 4;
    for y in 0..height {
        let to = ((y0 + y) as usize * stride + x0 as usize) * 4;
        let from = y as usize * row;
        pixels.data[to..to + row].copy_from_slice(&data[from..from + row]);
    }
}

fn clip_to(rect: Rect, w: u32, h: u32) -> (u32, u32, u32, u32) {
    let x0 = rect.min_x().floor().max(0.0) as u32;
    let y0 = rect.min_y().floor().max(0.0) as u32;
    let x1 = rect.max_x().ceil().clamp(0.0, w as f32) as u32;
    let y1 = rect.max_y().ceil().clamp(0.0, h as f32) as u32;
    (x0.min(w), y0.min(h), x1.min(w), y1.min(h))
}

/// Counters for diagnostics and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    pub batches: usize,
    pub rects: usize,
    pub texts: usize,
    pub glyphs: usize,
    pub images: usize,
    pub shadows: usize,
    pub pixels_written: u64,
    /// Layers whose effect chain was reused from an earlier frame.
    pub effect_hits: usize,
    /// Layers whose effect chain had to run again.
    pub effect_misses: usize,
}

/// The CPU renderer.
#[derive(Default)]
pub struct CpuRenderer {
    pub stats: RenderStats,
    /// Device pixels per logical point. Primitives are authored in logical units.
    scale: f32,
    /// Coverage atlas uploaded by the caller, if glyphs are in play.
    atlas: Option<CoverageAtlas>,
    /// Colour atlas uploaded by the caller, if colour glyphs are in play.
    color_atlas: Option<ColorAtlas>,
    /// Scratch targets for offscreen layers, indexed by nesting depth.
    scratch: Vec<Pixels>,
    /// The effect chain's output for the layer at each depth, kept for the next frame that
    /// asks the same question of the same bytes.
    effect_cache: Vec<Option<EffectCache>>,
    /// One convolution pass buffer, reused by every effect of every layer: a separable
    /// kernel needs a copy of the region it reads while it writes the result.
    effect_scratch: Vec<u8>,
}

/// One layer's post-effect pixels, and what they were computed from.
struct EffectCache {
    /// Fingerprint of the chain as it ran on this grid.
    chain: u64,
    /// Fingerprint of the source pixels the chain read.
    source: u64,
    x0: u32,
    y0: u32,
    width: u32,
    height: u32,
    /// The uniform opacity the chain left for the blend below it.
    opacity: f32,
    /// Premultiplied BGRA of the region, row by row.
    pixels: Vec<u8>,
}

/// A coverage atlas the renderer samples for glyph blits.
#[derive(Clone, Debug)]
pub struct CoverageAtlas {
    pub width: u32,
    pub height: u32,
    /// One byte of coverage per pixel.
    pub data: Vec<u8>,
}

/// A colour atlas: premultiplied RGBA, four bytes per pixel.
#[derive(Clone, Debug)]
pub struct ColorAtlas {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl CpuRenderer {
    pub fn new() -> Self {
        Self {
            scale: 1.0,
            ..Self::default()
        }
    }

    /// Set device pixels per logical point (fractional scaling, HiDPI).
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Scale a logical rect into device pixels.
    fn to_device(&self, r: Rect) -> Rect {
        if (self.scale - 1.0).abs() < f32::EPSILON {
            return r;
        }
        Rect::new(
            r.origin.x * self.scale,
            r.origin.y * self.scale,
            r.size.w * self.scale,
            r.size.h * self.scale,
        )
    }

    /// Set the coverage atlas glyph blits sample from.
    pub fn set_atlas(&mut self, atlas: CoverageAtlas) {
        self.atlas = Some(atlas);
    }

    /// Set the colour atlas colour glyphs sample from.
    pub fn set_color_atlas(&mut self, atlas: ColorAtlas) {
        self.color_atlas = Some(atlas);
    }

    /// Render `list` into `target`. When `damage` names rects, untouched pixels come
    /// from `previous`.
    /// Render `list` into `target`, repainting the whole surface.
    ///
    /// `damage` now only decides *whether* to repaint at all: any non-empty damage repaints
    /// everything. Partial repainting by copying last frame's pixels was removed together
    /// with the rest of the incremental machinery (`docs/effect-cache-plan.md` §11) — it is
    /// what let a backdrop see stale pixels, and it also ran each backdrop once per damaged
    /// rect, which double-blurred a frost that two rects happened to cover.
    pub fn render(&mut self, list: &DrawList, target: &mut Pixels, damage: &Damage) {
        self.stats = RenderStats::default();
        if damage.is_none() {
            return;
        }
        target.clear();
        let full = Rect::new(0.0, 0.0, target.width as f32, target.height as f32);
        let batches = list.plan_batches();
        self.render_region(list, &batches, target, full);
    }

    /// Draw one region, honouring offscreen layers.
    fn render_region(
        &mut self,
        list: &DrawList,
        batches: &[Batch],
        target: &mut Pixels,
        region: Rect,
    ) {
        // Active layer chain; depth `d` draws into `self.scratch[d]`.
        let mut stack: Vec<u32> = Vec::new();
        // A layer starts empty, so a frosted surface inside one samples its parent, the
        // buffer the layer is composited back over.
        let backdrops = list.backdrop_order();
        let mut done = 0usize;

        for batch in batches {
            self.sync_layers(list, &mut stack, &batch.layers, target, region);
            while done < backdrops.len() && backdrops[done].0 <= batch.epoch {
                let (_, entry) = backdrops[done];
                done += 1;
                self.frost_entry(&stack, target, entry);
            }
            if stack.is_empty() {
                self.draw_batch(list, batch, target, region);
            } else {
                let depth = stack.len() - 1;
                let mut scratch = self.take_scratch(depth, target.width, target.height);
                self.draw_batch(list, batch, &mut scratch, region);
                self.put_scratch(depth, scratch);
            }
        }
        // A backdrop with no batch after it would otherwise never run: the loop only
        // advances the queue when it sees a later epoch.
        while done < backdrops.len() {
            let (_, entry) = backdrops[done];
            done += 1;
            self.frost_entry(&stack, target, entry);
        }
        while let Some(index) = stack.pop() {
            let depth = stack.len();
            let (effects, bounds, radius) = layer_plan(list, index);
            self.composite(depth, effects, bounds, radius, target, region);
        }
    }

    /// Pop and push layers so the active chain equals `want`.
    fn sync_layers(
        &mut self,
        list: &DrawList,
        stack: &mut Vec<u32>,
        want: &[u32],
        target: &mut Pixels,
        region: Rect,
    ) {
        let mut common = 0;
        while common < stack.len() && common < want.len() && stack[common] == want[common] {
            common += 1;
        }
        while stack.len() > common {
            let index = stack.pop().expect("non-empty");
            let depth = stack.len();
            let (effects, bounds, radius) = layer_plan(list, index);
            self.composite(depth, effects, bounds, radius, target, region);
        }
        for &index in &want[common..] {
            let depth = stack.len();
            let mut scratch = self.take_scratch(depth, target.width, target.height);
            // A layer starts empty; leftovers from an earlier frame would leak through.
            clear_region(&mut scratch, region);
            self.put_scratch(depth, scratch);
            stack.push(index);
        }
    }

    /// Frost one backdrop entry. The area, the blur width and the corner shape all reach
    /// the rasteriser in device pixels: the band it writes is made of them.
    fn frost_entry(
        &mut self,
        stack: &[u32],
        target: &mut Pixels,
        entry: ui_core::draw::BackdropEntry,
    ) {
        let sigma = entry.sigma * self.scale;
        self.frost_backdrop(
            stack,
            target,
            sigma,
            self.to_device(frost_area(entry)),
            scale_corners(entry.corners, self.scale),
        );
    }

    /// Frost `rect` for the layer in force: a layer starts empty, so a surface inside one
    /// frosts its parent — the buffer the layer is composited back over.
    fn frost_backdrop(
        &mut self,
        stack: &[u32],
        target: &mut Pixels,
        sigma: f32,
        rect: Rect,
        corners: ui_core::geom::Corners,
    ) {
        if rect.size.w <= 0.0 || rect.size.h <= 0.0 {
            return;
        }
        // Material shading is GPU-only; the CPU path keeps the plain blur.
        let Some(depth) = stack.len().checked_sub(1) else {
            if let Some(band) = Band::take(target, sigma, rect) {
                band.write_into(target, rect, corners);
            }
            return;
        };
        let mut scratch = self.take_scratch(depth, target.width, target.height);
        let band = if depth == 0 {
            Band::take(target, sigma, rect)
        } else {
            Band::take(&self.scratch[depth - 1], sigma, rect)
        };
        if let Some(band) = band {
            band.write_into(&mut scratch, rect, corners);
        }
        self.put_scratch(depth, scratch);
    }

    fn take_scratch(&mut self, depth: usize, w: u32, h: u32) -> Pixels {
        while self.scratch.len() <= depth {
            self.scratch.push(Pixels::new(0, 0));
        }
        let slot = &mut self.scratch[depth];
        if slot.width != w || slot.height != h {
            *slot = Pixels::new(w, h);
        }
        std::mem::replace(slot, Pixels::new(0, 0))
    }

    fn put_scratch(&mut self, depth: usize, pixels: Pixels) {
        while self.scratch.len() <= depth {
            self.scratch.push(Pixels::new(0, 0));
        }
        self.scratch[depth] = pixels;
    }

    /// Composite scratch level `depth` into its parent after running its effects.
    fn composite(
        &mut self,
        depth: usize,
        effects: &[Effect],
        bounds: Rect,
        radius: ui_core::geom::Corners,
        target: &mut Pixels,
        region: Rect,
    ) {
        let mut src = self.take_scratch(depth, target.width, target.height);
        // `bounds` is logical and `region` is device pixels; convert before intersecting.
        let effect_region = self
            .to_device(bounds)
            .intersection(region)
            .filter(|r| r.size.w > 0.0 && r.size.h > 0.0)
            .unwrap_or(region);
        // Kernels are authored per logical dp; this buffer holds `scale` times more pixels.
        let scaled: Vec<Effect> = effects
            .iter()
            .cloned()
            .map(|e| ui_core::effect::on_grid(e, self.scale))
            .collect();
        let opacity = self.layer_effects(depth, &mut src, &scaled, effect_region);
        // Corner rounding is a device-space property; the layer's bounds are logical.
        let device = self.to_device(bounds);
        let scaled = scale_corners(radius, self.scale);
        if depth == 0 {
            composite_over_rounded(target, &src, opacity, region, device, scaled);
        } else {
            // The parent level has to be borrowed at the same time, so take it out too.
            let mut parent = self.take_scratch(depth - 1, target.width, target.height);
            composite_over_rounded(&mut parent, &src, opacity, region, device, scaled);
            self.put_scratch(depth - 1, parent);
        }
        self.put_scratch(depth, src);
    }

    /// Run `effects` over `region` of `src`, or reuse the last frame's answer when the
    /// bytes it was computed from are the same.
    ///
    /// The cache sits at the chain's output and nowhere else: an intermediate result has
    /// been quantised once, and a reused one must be the very pixels a recompute would
    /// produce. Same chain, same rect, same source bytes leaves nothing else to vary.
    fn layer_effects(
        &mut self,
        depth: usize,
        src: &mut Pixels,
        effects: &[Effect],
        region: Rect,
    ) -> f32 {
        let (x0, y0, x1, y1) = clip_to(region, src.width, src.height);
        // A chain that only rewrites pixels in place is one pass per pixel; hashing its
        // input would cost about as much as running it. A kernel reads neighbours, and
        // that is the work worth keeping.
        let keep = x1 > x0 && y1 > y0 && effects.iter().any(|e| !e.kernel.is_identity());
        if keep {
            let chain = ui_core::effect::chain_fingerprint(effects);
            let source = hash_region(src, x0, y0, x1, y1);
            if self.effect_cache.len() <= depth {
                self.effect_cache.resize_with(depth + 1, || None);
            }
            let hit = self.effect_cache[depth].take().filter(|entry| {
                entry.chain == chain
                    && entry.source == source
                    && (entry.x0, entry.y0, entry.width, entry.height) == (x0, y0, x1 - x0, y1 - y0)
            });
            if let Some(entry) = hit {
                self.stats.effect_hits += 1;
                paste_region(src, x0, y0, entry.width, entry.height, &entry.pixels);
                return entry.opacity;
            }
            let opacity = apply_effects(src, effects, region, &mut self.effect_scratch);
            self.stats.effect_misses += 1;
            self.effect_cache[depth] = Some(EffectCache {
                chain,
                source,
                x0,
                y0,
                width: x1 - x0,
                height: y1 - y0,
                opacity,
                pixels: region_bytes(src, x0, y0, x1, y1),
            });
            return opacity;
        }
        apply_effects(src, effects, region, &mut self.effect_scratch)
    }

    fn draw_batch(&mut self, list: &DrawList, batch: &Batch, target: &mut Pixels, region: Rect) {
        // `batch.clip` is in logical units while `region` is in device pixels; comparing
        // them directly is only correct at scale 1.
        let (clip, rounded) = match batch.clip {
            Some(c) => {
                let device = self.to_device(c.rect);
                let rect = device.intersection(region).unwrap_or(Rect::ZERO);
                // The shape only matters when the radius is non-zero.
                let shape = if c.corners.is_zero() {
                    None
                } else {
                    // The corners follow the uncut box: an outer clip narrows `rect` but the
                    // node's own rounded edges stay where they were.
                    let clamped = c.corners.clamp_to(c.shape.size);
                    let s = self.scale;
                    Some((
                        self.to_device(c.shape),
                        Corners {
                            tl: clamped.tl * s,
                            tr: clamped.tr * s,
                            br: clamped.br * s,
                            bl: clamped.bl * s,
                            shape: clamped.shape,
                        },
                    ))
                };
                (rect, shape)
            }
            None => (region, None),
        };
        if clip.is_empty() {
            return;
        }
        let extent = clip_to(clip, target.width, target.height);
        if extent.0 >= extent.2 || extent.1 >= extent.3 {
            return;
        }
        let clip = ClipRegion { extent, rounded };

        match batch.kind {
            PrimKind::Image => {
                for i in batch.range.clone() {
                    let Some(prim) = list.prims.images.get(i as usize) else {
                        continue;
                    };
                    // A missing texture draws nothing, as it does on the GPU path.
                    let Some(entry) = list.images.get(prim.texture as usize) else {
                        continue;
                    };
                    self.draw_image(prim, entry, target, &clip);
                    self.stats.images += 1;
                }
            }
            PrimKind::Shadow => {
                for i in batch.range.clone() {
                    if let Some(prim) = list.prims.shadows.get(i as usize) {
                        self.draw_shadow(prim, target, &clip);
                        self.stats.shadows += 1;
                    }
                }
            }
            PrimKind::Sector => {
                for i in batch.range.clone() {
                    if let Some(prim) = list.prims.sectors.get(i as usize).copied() {
                        self.draw_sector(&prim, target, &clip);
                    }
                }
            }
            PrimKind::Ellipse => {
                for i in batch.range.clone() {
                    if let Some(prim) = list.prims.ellipses.get(i as usize).copied() {
                        self.draw_ellipse(&prim, target, &clip);
                    }
                }
            }
            PrimKind::Line => {
                for i in batch.range.clone() {
                    if let Some(prim) = list.prims.lines.get(i as usize).copied() {
                        self.draw_line(&prim, target, &clip);
                    }
                }
            }
            PrimKind::Rect => {
                for i in batch.range.clone() {
                    if let Some(prim) = list.prims.rects.get(i as usize).copied() {
                        self.draw_rect(&prim, target, &clip);
                        self.stats.rects += 1;
                    }
                }
            }
            PrimKind::Text => {
                for i in batch.range.clone() {
                    if let Some(prim) = list.prims.texts.get(i as usize) {
                        self.draw_text(prim, target, &clip);
                        self.stats.texts += 1;
                    }
                }
            }
            // A formula without a rasterized atlas has no CPU path: its glyphs are ids, and
            // only the text pass can turn those into coverage.
            PrimKind::Math => {}
            PrimKind::Glyph => {
                for i in batch.range.clone() {
                    if let Some(prim) = list.prims.glyphs.get(i as usize).copied() {
                        self.draw_glyph(&prim, target, &clip);
                        self.stats.glyphs += 1;
                    }
                }
            }
        }
    }

    /// One stroked segment: signed distance to the segment, feathered over a pixel.
    fn draw_line(
        &mut self,
        prim: &ui_core::draw::LinePrim,
        target: &mut Pixels,
        clip: &ClipRegion,
    ) {
        let s = self.scale;
        let a = (prim.from.x * s, prim.from.y * s);
        let b = (prim.to.x * s, prim.to.y * s);
        let half = prim.thickness * s * 0.5;
        if half <= 0.0 || prim.colour.a <= 0.0 {
            return;
        }
        let (cx0, cy0, cx1, cy1) = clip.extent;
        let x0 = (a.0.min(b.0) - half - 1.0).floor().max(cx0 as f32) as u32;
        let y0 = (a.1.min(b.1) - half - 1.0).floor().max(cy0 as f32) as u32;
        let x1 = (a.0.max(b.0) + half + 1.0).ceil().min(cx1 as f32) as u32;
        let y1 = (a.1.max(b.1) + half + 1.0).ceil().min(cy1 as f32) as u32;
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let ba = (b.0 - a.0, b.1 - a.1);
        let length_squared = ba.0 * ba.0 + ba.1 * ba.1;
        let stride_w = target.width as usize * 4;
        for y in y0..y1 {
            let py = y as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let cc = clip.coverage(px, py);
                if cc <= 0.0 {
                    continue;
                }
                let pa = (px - a.0, py - a.1);
                let t = if length_squared > 0.0 {
                    ((pa.0 * ba.0 + pa.1 * ba.1) / length_squared).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let dx = pa.0 - ba.0 * t;
                let dy = pa.1 - ba.1 * t;
                let distance = (dx * dx + dy * dy).sqrt() - half;
                let coverage = (0.5 - distance).clamp(0.0, 1.0);
                let alpha = prim.colour.a * coverage;
                if alpha <= 0.0 {
                    continue;
                }
                let colour = Color::rgba(prim.colour.r, prim.colour.g, prim.colour.b, alpha);
                let idx = y as usize * stride_w + x as usize * 4;
                blend_pixel(&mut target.data[idx..idx + 4], colour, cc);
            }
        }
    }

    /// One ellipse: fill inside the curve, stroke centred on it, feathered over a pixel.
    fn draw_ellipse(
        &mut self,
        prim: &ui_core::draw::EllipsePrim,
        target: &mut Pixels,
        clip: &ClipRegion,
    ) {
        let s = self.scale;
        let (cx0, cy0, cx1, cy1) = clip.extent;
        let c = (prim.centre.x * s, prim.centre.y * s);
        let (rx, ry) = (prim.radii.0 * s, prim.radii.1 * s);
        let half = prim.thickness * s * 0.5;
        if rx <= 0.0 || ry <= 0.0 {
            return;
        }
        let x0 = (c.0 - rx - half - 1.0).floor().max(cx0 as f32) as u32;
        let y0 = (c.1 - ry - half - 1.0).floor().max(cy0 as f32) as u32;
        let x1 = (c.0 + rx + half + 1.0).ceil().min(cx1 as f32) as u32;
        let y1 = (c.1 + ry + half + 1.0).ceil().min(cy1 as f32) as u32;
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let stride_w = target.width as usize * 4;
        for y in y0..y1 {
            let py = y as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let cc = clip.coverage(px, py);
                if cc <= 0.0 {
                    continue;
                }
                let distance = sd_ellipse(px - c.0, py - c.1, rx, ry);
                let fill_cov = (0.5 - distance).clamp(0.0, 1.0);
                let stroke_cov = if half > 0.0 {
                    (0.5 - (distance.abs() - half)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let cov = fill_cov.max(stroke_cov);
                if cov <= 0.0 {
                    continue;
                }
                // Fill and stroke meet only in the feather seam, so one blend covers both.
                let ring = (stroke_cov / cov).clamp(0.0, 1.0);
                let colour = ring_mix(prim.fill, prim.colour, ring);
                let idx = y as usize * stride_w + x as usize * 4;
                blend_pixel(&mut target.data[idx..idx + 4], colour, cov * cc);
                self.stats.pixels_written += 1;
            }
        }
    }

    fn draw_rect(&mut self, prim: &RectPrim, target: &mut Pixels, clip: &ClipRegion) {
        let (cx0, cy0, cx1, cy1) = clip.extent;
        let r = self.to_device(prim.rect);
        if r.is_empty() {
            return;
        }
        let s = self.scale;
        let radius = Corners {
            tl: prim.radius.tl * s,
            tr: prim.radius.tr * s,
            br: prim.radius.br * s,
            bl: prim.radius.bl * s,
            shape: prim.radius.shape,
        }
        .clamp_to(r.size);
        let stride_w = target.width as usize * 4;

        let x0 = r.min_x().floor().max(cx0 as f32) as u32;
        let y0 = r.min_y().floor().max(cy0 as f32) as u32;
        let x1 = r.max_x().ceil().min(cx1 as f32) as u32;
        let y1 = r.max_y().ceil().min(cy1 as f32) as u32;
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let inner_rect = prim.border.map(|(_, bw)| {
            let bw = bw * s;
            Rect::new(
                r.origin.x + bw,
                r.origin.y + bw,
                (r.size.w - bw * 2.0).max(0.0),
                (r.size.h - bw * 2.0).max(0.0),
            )
        });
        let inner_radius = prim.border.map(|(_, bw)| {
            let bw = bw * s;
            Corners {
                tl: (radius.tl - bw).max(0.0),
                tr: (radius.tr - bw).max(0.0),
                br: (radius.br - bw).max(0.0),
                bl: (radius.bl - bw).max(0.0),
                shape: radius.shape,
            }
        });

        for y in y0..y1 {
            let py = y as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                // The rounded clip attenuates coverage instead of just bounding it.
                let cc = clip.coverage(px, py);
                if cc <= 0.0 {
                    continue;
                }
                let cov = coverage_rounded_rect(r, radius, px, py) * cc;
                if cov <= 0.0 {
                    continue;
                }
                let idx = y as usize * stride_w + x as usize * 4;
                // A gradient varies per pixel, so the fill colour is sampled here rather
                // than hoisted out of the loop. It is sampled in device space: the stops
                // and extent are lengths, and lengths scale with the output.
                let fill = match &prim.gradient {
                    Some(g) => {
                        let (px_l, py_l) = (px / s, py / s);
                        let (pos, extent) = match g.axis {
                            ui_core::draw::GradientAxis::Vertical => {
                                (py_l - r.origin.y / s, r.size.h / s)
                            }
                            ui_core::draw::GradientAxis::Horizontal => {
                                (px_l - r.origin.x / s, r.size.w / s)
                            }
                        };
                        g.sample(pos, extent)
                    }
                    None => prim.fill,
                };
                // The ring and the inner area are disjoint parts of one shape: mix them by
                // area and blend once. Two stacked blends count the feather twice.
                let src = match (prim.border, inner_rect, inner_radius) {
                    (Some((_, bw)), Some(ir), Some(irad)) => {
                        // A gradient stroke is sampled per pixel, like the fill above.
                        let bc = match &prim.border_gradient {
                            Some(g) => {
                                let (px_l, py_l) = (px / s, py / s);
                                let (pos, extent) = match g.axis {
                                    ui_core::draw::GradientAxis::Vertical => {
                                        (py_l - r.origin.y / s, r.size.h / s)
                                    }
                                    ui_core::draw::GradientAxis::Horizontal => {
                                        (px_l - r.origin.x / s, r.size.w / s)
                                    }
                                };
                                g.sample(pos, extent)
                            }
                            None => prim.border.map(|(c, _)| c).unwrap_or(Color::TRANSPARENT),
                        };
                        let inner_cov = coverage_rounded_rect(ir, irad, px, py) * cc;
                        let mut ring = ((cov - inner_cov) / cov).clamp(0.0, 1.0);
                        // A notch cuts the stroke where it crosses the top edge; the fill
                        // survives, so a transparent box shows the backdrop through it.
                        // The notch is stated in logical units while this loop runs in device
                        // pixels; scaling only the border width left the gap half as wide at
                        // scale 2, so a hairline of stroke stayed under the floating label.
                        let (lx, ly) = (px - r.origin.x, py - r.origin.y);
                        if prim.in_notch(lx / s, ly / s, bw, 0.0) {
                            ring = 0.0;
                        }
                        ring_mix(fill, bc, ring)
                    }
                    _ => fill,
                };
                blend_pixel(&mut target.data[idx..idx + 4], src, cov);
                self.stats.pixels_written += 1;
            }
        }
    }

    /// One texture: `src` (texture pixels) stretched over the destination box, sampled
    /// bilinearly and feathered by the box's corner shape and the clip, exactly as the GPU
    /// sampler does it.
    fn draw_image(
        &mut self,
        prim: &ui_core::draw::ImagePrim,
        entry: &ui_core::draw::ImageEntry,
        target: &mut Pixels,
        clip: &ClipRegion,
    ) {
        let (cx0, cy0, cx1, cy1) = clip.extent;
        let r = self.to_device(prim.rect);
        if r.is_empty() || entry.width == 0 || entry.height == 0 {
            return;
        }
        let radius = scale_corners(prim.corners, self.scale).clamp_to(r.size);
        let stride_w = target.width as usize * 4;
        // The CPU painter is always an SDR destination, so a picture keeps its own domain
        // here only until the sample; the texture itself is never rewritten.
        let placement = entry.colour.placement(ui_core::colour::Destination::SDR);

        let x0 = r.min_x().floor().max(cx0 as f32) as u32;
        let y0 = r.min_y().floor().max(cy0 as f32) as u32;
        let x1 = r.max_x().ceil().min(cx1 as f32) as u32;
        let y1 = r.max_y().ceil().min(cy1 as f32) as u32;
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        for y in y0..y1 {
            let py = y as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let cc = clip.coverage(px, py);
                if cc <= 0.0 {
                    continue;
                }
                let cov = coverage_rounded_rect(r, radius, px, py) * cc;
                if cov <= 0.0 {
                    continue;
                }
                let (u, v) = ((px - r.origin.x) / r.size.w, (py - r.origin.y) / r.size.h);
                // A sampler maps the box onto the texture's *edges*, so texel centres sit half
                // a texel in; without the shift a stretched texture is offset by half a texel.
                let src = sample_texture(
                    entry,
                    &placement,
                    prim.src.origin.x + u * prim.src.size.w - 0.5,
                    prim.src.origin.y + v * prim.src.size.h - 0.5,
                );
                let idx = y as usize * stride_w + x as usize * 4;
                blend_pixel(&mut target.data[idx..idx + 4], src, cov);
                self.stats.pixels_written += 1;
            }
        }
    }

    /// One analytic shadow: the silhouette convolved with a gaussian, evaluated through the
    /// normal CDF as the shader does it, so neither path needs a blur pass or a target.
    fn draw_shadow(
        &mut self,
        prim: &ui_core::draw::ShadowPrim,
        target: &mut Pixels,
        clip: &ClipRegion,
    ) {
        let s = self.scale;
        let (cx0, cy0, cx1, cy1) = clip.extent;
        let r = self.to_device(prim.rect);
        if r.is_empty() {
            return;
        }
        let radius = scale_corners(prim.corners, s).clamp_to(r.size);
        // `blur` is the gaussian's radius, which CSS and Skia define as two sigma.
        let sigma = (prim.blur * s * 0.5).max(1e-4);
        let dy = prim.dy * s;
        let band = prim.ring * s * 0.5;
        // The caster moved by `dy` is the shape whose distance the falloff reads.
        let caster = Rect::new(r.origin.x, r.origin.y + dy, r.size.w, r.size.h);
        let bound = prim.bound.map(|bound| {
            let rect = self.to_device(bound.rect);
            (rect, radius.clamp_to(rect.size), bound.side)
        });
        // An inset cast runs from the caster boundary out to the surface edge, so the scan
        // must cover both boxes.
        let area = match bound {
            Some((rect, ..)) => r.union(rect),
            None => r,
        };
        // The gradient fits in three sigma plus the offset; the ring's half-band adds its own.
        let pad = prim.blur * s * 1.5 + dy.abs() + band;
        let x0 = (area.min_x() - pad).floor().max(cx0 as f32) as u32;
        let y0 = (area.min_y() - pad).floor().max(cy0 as f32) as u32;
        let x1 = (area.max_x() + pad).ceil().min(cx1 as f32) as u32;
        let y1 = (area.max_y() + pad).ceil().min(cy1 as f32) as u32;
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let stride_w = target.width as usize * 4;

        for y in y0..y1 {
            let py = y as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let cc = clip.coverage(px, py);
                if cc <= 0.0 {
                    continue;
                }
                let d = sd_round_box(caster, radius, px, py);
                // A ring is the band `ring` deep straddling the silhouette; the difference of
                // the two CDFs is that band blurred, and carries its sub-unit alpha on its own.
                // An inset cast is the same falloff mirrored: the wall outside the edge throws
                // inward, so the surface edge is the shoulder on both sides.
                let hist = match bound {
                    Some((_, _, ui_core::draw::ShadowSide::Inset)) => normal_cdf(d / sigma),
                    _ if band > 0.0 => {
                        normal_cdf((d + band) / sigma) - normal_cdf((d - band) / sigma)
                    }
                    _ => normal_cdf(-d / sigma),
                };
                let mut alpha = prim.colour.a * hist;
                if let Some((edge, cut_radius, side)) = bound {
                    // Feather the cut like the surface's own fill, so a translucent surface
                    // shows no seam where its edge meets the cast.
                    let inside = coverage_rounded_rect(edge, cut_radius, px, py);
                    alpha *= match side {
                        ui_core::draw::ShadowSide::Inset => inside,
                        ui_core::draw::ShadowSide::Outer => 1.0 - inside,
                    };
                }
                if alpha <= 0.0 {
                    continue;
                }
                let colour = Color::rgba(prim.colour.r, prim.colour.g, prim.colour.b, alpha);
                let idx = y as usize * stride_w + x as usize * 4;
                blend_pixel(&mut target.data[idx..idx + 4], colour, cc);
                self.stats.pixels_written += 1;
            }
        }
    }

    /// One stroked arc: an annulus clipped to a sweep, with optional round caps.
    fn draw_sector(
        &mut self,
        prim: &ui_core::draw::SectorPrim,
        target: &mut Pixels,
        clip: &ClipRegion,
    ) {
        let s = self.scale;
        let (cx0, cy0, cx1, cy1) = clip.extent;
        let c = (prim.sector.centre.x * s, prim.sector.centre.y * s);
        let r_out = prim.sector.radius * s;
        let w = prim.sector.thickness * s;
        let r_in = (r_out - w).max(0.0);
        let r_mid = (r_out + r_in) * 0.5;
        let sweep = prim.sector.sweep();
        if r_out <= 0.0 || w <= 0.0 || sweep <= 0.0 {
            return;
        }
        let (start, end) = (prim.sector.trim.0, prim.sector.trim.0 + sweep);
        // The caps are circles at the two end points, centred on the stroke.
        let cap = |turns: f32| {
            let a = turns * std::f32::consts::TAU;
            (c.0 + r_mid * a.cos(), c.1 + r_mid * a.sin())
        };
        let caps = prim.sector.round_caps.then(|| [cap(start), cap(end)]);

        let x0 = (c.0 - r_out - w * 0.5).floor().max(cx0 as f32) as u32;
        let y0 = (c.1 - r_out - w * 0.5).floor().max(cy0 as f32) as u32;
        let x1 = (c.0 + r_out + w * 0.5).ceil().min(cx1 as f32) as u32;
        let y1 = (c.1 + r_out + w * 0.5).ceil().min(cy1 as f32) as u32;
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let stride_w = target.width as usize * 4;
        for y in y0..y1 {
            let py = y as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let cc = clip.coverage(px, py);
                if cc <= 0.0 {
                    continue;
                }
                let mut cov = sector_coverage(c, r_in, r_out, start, sweep, px, py);
                if let Some(caps) = caps {
                    for (ccx, ccy) in caps {
                        let d = ((px - ccx).powi(2) + (py - ccy).powi(2)).sqrt();
                        cov = cov.max((w * 0.5 + 0.5 - d).clamp(0.0, 1.0));
                    }
                }
                let cov = cov * cc;
                if cov <= 0.0 {
                    continue;
                }
                let idx = y as usize * stride_w + x as usize * 4;
                blend_pixel(&mut target.data[idx..idx + 4], prim.colour, cov);
                self.stats.pixels_written += 1;
            }
        }
    }

    /// Blit one glyph: coverage is tinted by `color`, colour pixels are used as-is.
    fn draw_glyph(
        &mut self,
        prim: &ui_core::draw::GlyphPrim,
        target: &mut Pixels,
        clip: &ClipRegion,
    ) {
        let (cx0, cy0, cx1, cy1) = clip.extent;
        let dst = self.to_device(prim.rect);
        // Round, not floor: `logical * scale` can land a hair below the integer. The GPU path
        // samples the quarter pixel a glyph's vertical phase leaves over; this one drops it,
        // so its text steps where the GPU's glides.
        let (x0, y0, x1, y1) = (
            dst.min_x().round().max(cx0 as f32) as u32,
            dst.min_y().round().max(cy0 as f32) as u32,
            dst.max_x().round().min(cx1 as f32) as u32,
            dst.max_y().round().min(cy1 as f32) as u32,
        );
        if x0 >= x1 || y0 >= y1 || prim.atlas.is_empty() {
            return;
        }
        let stride = target.width as usize * 4;
        // Glyphs blit 1:1 in practice; integer offsets avoid sampling drift that
        // would make letter spacing uneven.
        let off_x = prim.atlas.min_x().round() as i32 - dst.min_x().round() as i32;
        let off_y = prim.atlas.min_y().round() as i32 - dst.min_y().round() as i32;
        let one_to_one = (prim.atlas.size.w - dst.size.w).abs() < 0.01
            && (prim.atlas.size.h - dst.size.h).abs() < 0.01;
        match prim.kind {
            GlyphKind::Coverage => {
                let Some(atlas) = self.atlas.as_ref() else {
                    return;
                };
                for y in y0..y1 {
                    let sy = if one_to_one {
                        (y as i32 + off_y).max(0) as u32
                    } else {
                        let v = (y as f32 - dst.min_y()) / dst.size.h.max(1.0);
                        (prim.atlas.min_y() + v * prim.atlas.size.h) as u32
                    };
                    if sy >= atlas.height {
                        continue;
                    }
                    for x in x0..x1 {
                        let sx = if one_to_one {
                            (x as i32 + off_x).max(0) as u32
                        } else {
                            let u = (x as f32 - dst.min_x()) / dst.size.w.max(1.0);
                            (prim.atlas.min_x() + u * prim.atlas.size.w) as u32
                        };
                        if sx >= atlas.width {
                            continue;
                        }
                        let cov = atlas.data[(sy * atlas.width + sx) as usize];
                        if cov == 0 {
                            continue;
                        }
                        let alpha = cov as f32 / 255.0
                            * prim.alpha
                            * clip.coverage(x as f32 + 0.5, y as f32 + 0.5);
                        if alpha <= 0.0 {
                            continue;
                        }
                        let idx = y as usize * stride + x as usize * 4;
                        blend_pixel(&mut target.data[idx..idx + 4], prim.color, alpha);
                        self.stats.pixels_written += 1;
                    }
                }
            }
            GlyphKind::Color => {
                let Some(atlas) = self.color_atlas.as_ref() else {
                    return;
                };
                for y in y0..y1 {
                    let sy = if one_to_one {
                        (y as i32 + off_y).max(0) as u32
                    } else {
                        let v = (y as f32 - dst.min_y()) / dst.size.h.max(1.0);
                        (prim.atlas.min_y() + v * prim.atlas.size.h) as u32
                    };
                    if sy >= atlas.height {
                        continue;
                    }
                    for x in x0..x1 {
                        let sx = if one_to_one {
                            (x as i32 + off_x).max(0) as u32
                        } else {
                            let u = (x as f32 - dst.min_x()) / dst.size.w.max(1.0);
                            (prim.atlas.min_x() + u * prim.atlas.size.w) as u32
                        };
                        if sx >= atlas.width {
                            continue;
                        }
                        let base = (sy * atlas.width + sx) as usize * 4;
                        let (r, g, b, a) = (
                            atlas.data[base],
                            atlas.data[base + 1],
                            atlas.data[base + 2],
                            atlas.data[base + 3],
                        );
                        if a == 0 {
                            continue;
                        }
                        let m = prim.alpha
                            * prim.color.a
                            * clip.coverage(x as f32 + 0.5, y as f32 + 0.5);
                        if m <= 0.0 {
                            continue;
                        }
                        let idx = y as usize * stride + x as usize * 4;
                        blend_premul_pixel(&mut target.data[idx..idx + 4], r, g, b, a, m);
                        self.stats.pixels_written += 1;
                    }
                }
            }
        }
    }

    fn draw_text(&mut self, prim: &TextPrim, target: &mut Pixels, clip: &ClipRegion) {
        // Placeholder: one bar per character cell, enough to prove layout wiring.
        // Replaced by a real glyph atlas when `ui-text` lands.
        let (text, spans) = prim.block.flatten();
        let style = prim
            .block
            .runs
            .first()
            .map_or_else(Default::default, |r| r.style);
        let _ = spans;
        let char_w = (style.size * 0.6).max(1.0);
        let line_h = style.size * 1.3;
        // The line block sits centred in a taller node box, as the glyph path places it.
        let spare = prim
            .layout
            .as_ref()
            .map_or(0.0, |l| (prim.height - l.height).max(0.0));
        let bar = RectPrim {
            rect: Rect::new(
                prim.origin.x,
                prim.origin.y + spare * 0.5 + line_h * 0.3,
                char_w * text.chars().count() as f32,
                style.size * 0.55,
            ),
            radius: Corners::ZERO,
            fill: prim.ink,
            border: None,
            border_gradient: None,
            gradient: None,
            notch: None,
        };
        self.draw_rect(&bar, target, clip);
    }
}

/// One sample of a straight-alpha RGBA8 texture at a texel coordinate.
///
/// Interpolation runs on premultiplied channels: a transparent texel next to an opaque one
/// must not drag its colour into the edge, which is what straight-alpha lerp does. A
/// pixelated drawing takes the nearest texel instead, the way `GL_NEAREST` does.
fn sample_texture(
    entry: &ui_core::draw::ImageEntry,
    placement: &ui_core::colour::Placement,
    x: f32,
    y: f32,
) -> Color {
    const ZERO: [u8; 8] = [0; 8];
    let (w, h) = (entry.width as i32, entry.height as i32);
    let texel_bytes = entry.colour.storage.bytes_per_texel();
    // A canvas has no CPU pixels: the engine swaps its entries for the target's bytes before
    // this pass, so an entry that is still a canvas is outside that contract and draws
    // nothing rather than sampling a stale buffer.
    let Some(data) = entry.pixels() else {
        return Color::TRANSPARENT;
    };
    // Edge-clamped, matching the sampler's `CLAMP_TO_EDGE`.
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
    let texel = |tx: i32, ty: i32| -> [f32; 4] {
        let at = ((ty as usize * entry.width as usize) + tx as usize) * texel_bytes;
        let bytes = data
            .get(at..at + texel_bytes)
            .unwrap_or(&ZERO[..texel_bytes]);
        let [r, g, b, a] = placement.texel(bytes);
        [r * a, g * a, b * a, a]
    };
    let mut out = [0.0f32; 4];
    if entry.sampling == ui_core::draw::Sampling::Pixelated {
        out = texel(x.round() as i32, y.round() as i32);
    } else {
        let (x0, y0) = (x.floor() as i32, y.floor() as i32);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let (a, b, c, d) = (texel(x0, y0), texel(x1, y0), texel(x0, y1), texel(x1, y1));
        for k in 0..4 {
            let top = a[k] + (b[k] - a[k]) * fx;
            let bottom = c[k] + (d[k] - c[k]) * fx;
            out[k] = top + (bottom - top) * fy;
        }
    }
    if out[3] <= 0.0 {
        return Color::TRANSPARENT;
    }
    // `blend_pixel` takes straight alpha, so undo the premultiplication.
    Color::rgba(out[0] / out[3], out[1] / out[3], out[2] / out[3], out[3])
}

/// Standard normal CDF, Abramowitz & Stegun 7.1.26: |error| < 1.5e-7 for every x.
///
/// The shader carries the same approximation; a shadow is a pure function of the distance, so
/// the two backends agree on it wherever the shape does.
fn normal_cdf(x: f32) -> f32 {
    let z = x * std::f32::consts::FRAC_1_SQRT_2;
    let t = 1.0 / (1.0 + 0.3275911 * z.abs());
    let poly = t
        * (0.254829592
            + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    let erf = 1.0 - poly * (-z * z).exp();
    if z >= 0.0 {
        0.5 * (1.0 + erf)
    } else {
        0.5 * (1.0 - erf)
    }
}

/// Blend a straight-alpha colour over the straight BGRA pixel, emitting straight.
fn blend_pixel(dst: &mut [u8], color: Color, coverage: f32) {
    let sa = (color.a * coverage).clamp(0.0, 1.0);
    if sa <= 0.0 {
        return;
    }
    let a_src = (sa * 255.0).round() as u32;
    let a_dst = dst[3] as u32;
    let inv = 255 - a_src;
    let a_out = a_src + a_dst * inv / 255;
    if a_out == 0 {
        dst.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }
    let blend = |s: f32, d: u32| -> u8 {
        let sc = (s.clamp(0.0, 1.0) * 255.0) as u32;
        ((sc * a_src + d * inv) / a_out).min(255) as u8
    };
    dst[0] = blend(color.b, dst[0] as u32);
    dst[1] = blend(color.g, dst[1] as u32);
    dst[2] = blend(color.r, dst[2] as u32);
    dst[3] = a_out.min(255) as u8;
}

/// Blend one premultiplied RGBA texel over the straight BGRA destination.
///
/// `scale` folds the primitive alpha and clip coverage into the texel.
fn blend_premul_pixel(dst: &mut [u8], r: u8, g: u8, b: u8, a: u8, scale: f32) {
    let ea = (a as f32 / 255.0 * scale).clamp(0.0, 1.0);
    let sa = (ea * 255.0).round() as u32;
    if sa == 0 {
        return;
    }
    let inv = 255 - sa;
    let da = dst[3] as u32;
    let a_out = sa + da * inv / 255;
    if a_out == 0 {
        dst.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }
    // The source is premultiplied, the destination is straight: scale the source by
    // `scale` and combine it with the destination's premultiplied contribution.
    let channel = |s: u8, d: u8| -> u8 {
        let src = (s as f32 * scale).round() as u32;
        let out_premul = src + (d as u32 * da / 255) * inv / 255;
        ((out_premul * 255 + a_out / 2) / a_out).min(255) as u8
    };
    dst[0] = channel(b, dst[0]);
    dst[1] = channel(g, dst[1]);
    dst[2] = channel(r, dst[2]);
    dst[3] = a_out.min(255) as u8;
}

/// Analytic coverage of a rounded rect at a sample point (0..=1).
/// The clip in force for one batch: a rect extent plus an optional rounded shape.
#[derive(Clone, Copy)]
struct ClipRegion {
    extent: (u32, u32, u32, u32),
    /// Device-space rounded rect, or `None` for a plain rectangle.
    rounded: Option<(Rect, Corners)>,
}

impl ClipRegion {
    fn coverage(&self, px: f32, py: f32) -> f32 {
        match self.rounded {
            Some((rect, corners)) => coverage_rounded_rect(rect, corners, px, py),
            None => 1.0,
        }
    }
}

/// Superellipse norm of a non-negative pair, `(|x|^n + |y|^n)^(1/n)`.
///
/// n = 2 is the circle, n = 1 a straight bevel, n -> inf a square corner.
fn superellipse_norm(x: f32, y: f32, n: f32) -> f32 {
    if n <= 1.0 {
        return x + y;
    }
    if n >= 64.0 {
        return x.max(y);
    }
    (x.powf(n) + y.powf(n)).powf(1.0 / n)
}

/// Signed distance to the rounded box at a device pixel: negative inside, positive outside.
///
/// The same field the shaders evaluate (`sd_round_box`), so the fill feather and a shadow's
/// falloff describe one shape rather than two approximations of it.
fn sd_round_box(r: Rect, radius: Corners, px: f32, py: f32) -> f32 {
    let half = (r.size.w * 0.5, r.size.h * 0.5);
    let dx = px - (r.origin.x + half.0);
    let dy = py - (r.origin.y + half.1);
    let rad = match (dx > 0.0, dy > 0.0) {
        (true, true) => radius.br,
        (true, false) => radius.tr,
        (false, true) => radius.bl,
        (false, false) => radius.tl,
    };
    let qx = dx.abs() - half.0 + rad;
    let qy = dy.abs() - half.1 + rad;
    qx.max(qy).min(0.0) + superellipse_norm(qx.max(0.0), qy.max(0.0), radius.shape.exponent()) - rad
}

/// One pixel that is part ring, part fill: mix them by area in premultiplied space.
fn ring_mix(fill: Color, border: Color, ring: f32) -> Color {
    let f = [fill.r * fill.a, fill.g * fill.a, fill.b * fill.a, fill.a];
    let b = [
        border.r * border.a,
        border.g * border.a,
        border.b * border.a,
        border.a,
    ];
    // Border composited over the fill, then weighted by the ring's share of the pixel.
    let inv = 1.0 - b[3];
    let over = [
        b[0] + f[0] * inv,
        b[1] + f[1] * inv,
        b[2] + f[2] * inv,
        b[3] + f[3] * inv,
    ];
    let m = [
        f[0] + (over[0] - f[0]) * ring,
        f[1] + (over[1] - f[1]) * ring,
        f[2] + (over[2] - f[2]) * ring,
        f[3] + (over[3] - f[3]) * ring,
    ];
    if m[3] <= 0.0 {
        return Color::TRANSPARENT;
    }
    Color {
        r: m[0] / m[3],
        g: m[1] / m[3],
        b: m[2] / m[3],
        a: m[3],
    }
}

/// Coverage of one stroked arc at a device pixel, analytic and one pixel wide.
///
/// Both boundaries are distances in device pixels, so the feather is round everywhere:
/// the ring uses the radial distance, the sweep uses arc length (`angle × radius`).
fn sector_coverage(
    c: (f32, f32),
    r_in: f32,
    r_out: f32,
    start: f32,
    sweep: f32,
    px: f32,
    py: f32,
) -> f32 {
    let (dx, dy) = (px - c.0, py - c.1);
    let d = (dx * dx + dy * dy).sqrt();
    // Negative inside the annulus.
    let ring = (r_in - d).max(d - r_out);
    if sweep >= 1.0 {
        return (0.5 - ring).clamp(0.0, 1.0);
    }
    let mut turns = dy.atan2(dx) / std::f32::consts::TAU;
    if turns < 0.0 {
        turns += 1.0;
    }
    let rel = (turns - start).rem_euclid(1.0);
    let arc = d * std::f32::consts::TAU;
    let angle = if rel <= sweep {
        -rel.min(sweep - rel) * arc
    } else {
        (rel - sweep).min(1.0 - rel) * arc
    };
    (0.5 - ring.max(angle)).clamp(0.0, 1.0)
}

/// Signed distance to an ellipse centred on the origin, in device pixels.
///
/// The exact form costs a cubic; this is the gradient-corrected estimate, which is exact on
/// the axes and within about a pixel elsewhere — the same tolerance a feathered edge has.
fn sd_ellipse(qx: f32, qy: f32, rx: f32, ry: f32) -> f32 {
    let k = ((qx / rx).powi(2) + (qy / ry).powi(2)).sqrt();
    let gradient = ((qx / (rx * rx)).powi(2) + (qy / (ry * ry)).powi(2)).sqrt();
    if gradient <= f32::EPSILON {
        // The centre: the nearest boundary is the shorter semi-axis.
        return -rx.min(ry);
    }
    k * (k - 1.0) / gradient
}

fn coverage_rounded_rect(r: Rect, radius: Corners, px: f32, py: f32) -> f32 {
    (0.5 - sd_round_box(r, radius, px, py)).clamp(0.0, 1.0)
}

/// Encode premultiplied BGRA into the straight-alpha RGBA `PixelStorage::Sdr8` describes,
/// which is the byte order every `ImageSource` picture is sampled in.
pub fn encode_rgba_straight(pixels: &Pixels) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.data.len());
    for px in pixels.data.as_chunks::<4>().0 {
        let a = px[3] as u32;
        let channel = |c: u8| {
            if a == 0 {
                0
            } else {
                ((c as u32 * 255) / a).min(255) as u8
            }
        };
        out.extend_from_slice(&[channel(px[2]), channel(px[1]), channel(px[0]), px[3]]);
    }
    out
}

#[cfg(test)]
mod tests;

/// Effects and bounds recorded for a layer index; empty when absent.
fn layer_plan(list: &DrawList, index: u32) -> (&[Effect], Rect, ui_core::geom::Corners) {
    match list.layers.get(index as usize) {
        Some(layer) => (layer.effects.as_slice(), layer.bounds, layer.radius),
        None => (&[], Rect::ZERO, ui_core::geom::Corners::ZERO),
    }
}

/// Run an effect chain over a layer buffer, returning the residual uniform opacity.
///
/// Kernels are separable, so each costs two 1D passes instead of one 2D pass.
fn apply_effects(
    pixels: &mut Pixels,
    effects: &[Effect],
    region: Rect,
    scratch: &mut Vec<u8>,
) -> f32 {
    let mut opacity = 1.0f32;
    for effect in effects {
        if effect.is_box() {
            box_blur(pixels, effect.kernel.radius, region);
        } else if !effect.kernel.is_identity() {
            convolve(pixels, &effect.kernel, region, scratch);
        }
        if effect.color.is_identity() {
            continue;
        }
        match effect.color.as_opacity() {
            // Keep the cheap case cheap: a pure fade folds into the blend below.
            Some(a) => opacity *= a,
            None => transform(pixels, &effect.color, region),
        }
    }
    opacity
}

/// The blurred band covering `rect`, taken from `src` with the padding the kernel reads.
struct Band {
    x0: u32,
    y0: u32,
    w: u32,
    data: Vec<u8>,
}

impl Band {
    /// Blur `rect` of `src` into a band, or `None` when it falls outside the surface.
    fn take(src: &Pixels, sigma: f32, rect: Rect) -> Option<Band> {
        let radius = ui_core::boxblur::radius_for_sigma(sigma);
        if radius == 0 {
            return None;
        }
        // The chain reads one radius per pass, so the neighbourhood must reach the whole band.
        let band = radius * ui_core::boxblur::BOX_PASSES;
        let padded = ui_core::geom::Insets::all(band as f32).expand(rect);
        let (px0, py0, px1, py1) = clip_to(padded, src.width, src.height);
        if px1 <= px0 || py1 <= py0 {
            return None;
        }
        let (w, h) = ((px1 - px0) as usize, (py1 - py0) as usize);
        let mut temp = Pixels::new(w as u32, h as u32);
        for row in 0..h {
            let from = ((py0 as usize + row) * src.width as usize + px0 as usize) * 4;
            let to = row * w * 4;
            temp.data[to..to + w * 4].copy_from_slice(&src.data[from..from + w * 4]);
        }
        box_blur(&mut temp, radius, Rect::new(0.0, 0.0, w as f32, h as f32));
        Some(Band {
            x0: px0,
            y0: py0,
            w: w as u32,
            data: temp.data,
        })
    }

    /// Keep only `rect`, written into `dst` at the same place it was read from.
    ///
    /// `corners` feathers the write to the box's own shape: the blur must not show square
    /// corners around a rounded panel. Anything outside the shape keeps the pixels the blur
    /// was about to replace, which is what an unwritten pixel means here.
    fn write_into(&self, dst: &mut Pixels, rect: Rect, corners: ui_core::geom::Corners) {
        let (rx0, ry0, rx1, ry1) = clip_to(rect, dst.width, dst.height);
        if rx1 <= rx0 || ry1 <= ry0 {
            return;
        }
        let w = self.w as usize;
        let stride = dst.width as usize * 4;
        let shaped = corners != ui_core::geom::Corners::ZERO;
        for y in ry0..ry1 {
            let from = ((y - self.y0) as usize * w + (rx0 - self.x0) as usize) * 4;
            let to = y as usize * stride + rx0 as usize * 4;
            let n = (rx1 - rx0) as usize * 4;
            if !shaped {
                dst.data[to..to + n].copy_from_slice(&self.data[from..from + n]);
                continue;
            }
            for x in rx0..rx1 {
                let cov = coverage_rounded_rect(rect, corners, x as f32 + 0.5, y as f32 + 0.5);
                if cov <= 0.0 {
                    continue;
                }
                let i = (y as usize * stride / 4 + x as usize) * 4;
                let j = ((y - self.y0) as usize * w + (x - self.x0) as usize) * 4;
                for ch in 0..4 {
                    let kept = dst.data[i + ch] as f32;
                    let blurred = self.data[j + ch] as f32;
                    dst.data[i + ch] = (kept + (blurred - kept) * cov).round() as u8;
                }
            }
        }
    }
}

/// Three box averages over `region`: a Gaussian whose cost does not grow with radius.
fn box_blur(pixels: &mut Pixels, radius: u32, region: Rect) {
    let (x0, y0, x1, y1) = clip_to(region, pixels.width, pixels.height);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let stride = pixels.width as usize;
    let (bx, by) = (x0 as usize, y0 as usize);
    let (bw, bh) = ((x1 - x0) as usize, (y1 - y0) as usize);
    for _ in 0..3 {
        ui_core::boxblur::box_h(&mut pixels.data, stride, bx, by, bw, bh, radius);
        ui_core::boxblur::box_v(&mut pixels.data, stride, bx, by, bw, bh, radius);
    }
}

/// Two 1D passes of `kernel` over `region`, edges clamped to the border pixel.
fn convolve(pixels: &mut Pixels, kernel: &Kernel, region: Rect, scratch: &mut Vec<u8>) {
    let (x0, y0, x1, y1) = clip_to(region, pixels.width, pixels.height);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let stride = pixels.width as usize;
    let (rw, rh) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let r = kernel.radius as i32;
    // The vertical pass reads the horizontal one, so only the region is copied: the pixels
    // around it are never read and are written back exactly as they were.
    scratch.clear();
    scratch.resize(rw * rh * 4, 0);
    for y in 0..rh {
        for x in 0..rw {
            let mut acc = [0.0f32; 4];
            for (i, weight) in kernel.weights.iter().enumerate() {
                let sx = (x as i32 + i as i32 - r).clamp(0, rw as i32 - 1) as usize;
                let o = (((y0 as usize + y) * stride) + x0 as usize + sx) * 4;
                for (c, cell) in acc.iter_mut().enumerate() {
                    *cell += weight * pixels.data[o + c] as f32;
                }
            }
            let o = (y * rw + x) * 4;
            for (c, value) in acc.iter().enumerate() {
                scratch[o + c] = value.clamp(0.0, 255.0) as u8;
            }
        }
    }
    for y in 0..rh {
        for x in 0..rw {
            let mut acc = [0.0f32; 4];
            for (i, weight) in kernel.weights.iter().enumerate() {
                let sy = (y as i32 + i as i32 - r).clamp(0, rh as i32 - 1) as usize;
                let o = (sy * rw + x) * 4;
                for (c, cell) in acc.iter_mut().enumerate() {
                    *cell += weight * scratch[o + c] as f32;
                }
            }
            let o = (((y0 as usize + y) * stride) + x0 as usize + x) * 4;
            for (c, value) in acc.iter().enumerate() {
                pixels.data[o + c] = value.clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Apply a colour matrix to every pixel in `region`.
fn transform(pixels: &mut Pixels, matrix: &ColorMatrix, region: Rect) {
    let (x0, y0, x1, y1) = clip_to(region, pixels.width, pixels.height);
    let w = pixels.width as usize;
    for y in y0..y1 {
        for x in x0..x1 {
            let o = (y as usize * w + x as usize) * 4;
            let px = [
                pixels.data[o] as f32,
                pixels.data[o + 1] as f32,
                pixels.data[o + 2] as f32,
                pixels.data[o + 3] as f32,
            ];
            let out = matrix.apply(px);
            for c in 0..4 {
                pixels.data[o + c] = out[c].clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// The pixels a frost may touch: its box cut by the clip, as its own fill would be.
fn frost_area(entry: ui_core::draw::BackdropEntry) -> Rect {
    entry.rect.intersection(entry.clip).unwrap_or(Rect::ZERO)
}

/// Zero a rectangle of a premultiplied buffer.
fn clear_region(pixels: &mut Pixels, region: Rect) {
    let (x0, y0, x1, y1) = clip_to(region, pixels.width, pixels.height);
    for y in y0..y1 {
        let start = (y as usize * pixels.width as usize + x0 as usize) * 4;
        let end = (y as usize * pixels.width as usize + x1 as usize) * 4;
        pixels.data[start..end].fill(0);
    }
}

/// Source-over composite of a straight-alpha layer scaled by `opacity`.
/// Same, but the layer only shows inside `bounds` with rounded corners.
fn composite_over_rounded(
    dst: &mut Pixels,
    src: &Pixels,
    opacity: f32,
    region: Rect,
    bounds: Rect,
    radius: ui_core::geom::Corners,
) {
    composite_over_in(
        dst,
        src,
        opacity,
        region,
        ui_core::colour::Encoding::SDR_PREMULTIPLIED,
        bounds,
        radius,
    );
}

fn scale_corners(c: ui_core::geom::Corners, scale: f32) -> ui_core::geom::Corners {
    ui_core::geom::Corners {
        tl: c.tl * scale,
        tr: c.tr * scale,
        br: c.br * scale,
        bl: c.bl * scale,
        ..c
    }
}

/// Composite in `encoding`'s domain; a linear domain decodes, blends, re-encodes.
///
/// Premultiplied encoded values cannot be decoded directly, so this un-premultiplies
/// first. That is why a linear domain must be chosen when the buffer is filled, not here.
fn composite_over_in(
    dst: &mut Pixels,
    src: &Pixels,
    opacity: f32,
    region: Rect,
    encoding: ui_core::colour::Encoding,
    bounds: Rect,
    radius: ui_core::geom::Corners,
) {
    if opacity <= 0.0 {
        return;
    }
    if encoding.is_linear() {
        composite_linear(dst, src, opacity, region, encoding.transfer);
        return;
    }
    // The buffer contract is straight alpha end to end (AGENTS 1.5: a premultiplied
    // reading over-brightened every layered partial-alpha pixel), so reuse `blend_pixel`.
    let width = dst.width as usize;
    let (x0, y0, x1, y1) = clip_to(region, dst.width, dst.height);
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y as usize * width + x as usize) * 4;
            let cov = coverage_rounded_rect(bounds, radius, x as f32 + 0.5, y as f32 + 0.5);
            if cov <= 0.0 {
                continue;
            }
            let color = Color::rgba(
                src.data[i + 2] as f32 / 255.0,
                src.data[i + 1] as f32 / 255.0,
                src.data[i] as f32 / 255.0,
                src.data[i + 3] as f32 / 255.0,
            );
            blend_pixel(&mut dst.data[i..i + 4], color, opacity * cov);
        }
    }
}

/// Float blend in linear light on straight buffers: decode, alpha-weight, blend, re-encode.
fn composite_linear(
    dst: &mut Pixels,
    src: &Pixels,
    opacity: f32,
    region: Rect,
    tf: ui_core::colour::TransferFunction,
) {
    let width = dst.width as usize;
    let (x0, y0, x1, y1) = clip_to(region, dst.width, dst.height);
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y as usize * width + x as usize) * 4;
            let sa = src.data[i + 3] as f32 / 255.0;
            if sa <= 0.0 {
                continue;
            }
            let a = sa * opacity;
            let da = dst.data[i + 3] as f32 / 255.0;
            let out_a = a + da * (1.0 - a);
            if out_a <= 0.0 {
                continue;
            }
            for ch in 0..3 {
                let s_lin = tf.decode(src.data[i + ch] as f32 / 255.0) * a;
                let d_lin = if da > 0.0 {
                    tf.decode(dst.data[i + ch] as f32 / 255.0) * da
                } else {
                    0.0
                };
                let out = s_lin + d_lin * (1.0 - a);
                let enc = tf.encode(out / out_a) * out_a;
                dst.data[i + ch] = (enc * 255.0).round().clamp(0.0, 255.0) as u8;
            }
            dst.data[i + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}
