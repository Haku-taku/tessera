//! Glyph rasterization and a packed atlas.

use std::collections::{HashMap, HashSet};

use skrifa::MetadataProvider;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::TableProvider;

use ui_core::draw::GlyphKind;
use ui_text::{FaceId, Variations};

/// A rasterized glyph: coverage bytes plus placement relative to the pen origin.
#[derive(Clone, Debug)]
pub struct GlyphBitmap {
    pub width: u32,
    pub height: u32,
    /// Left edge relative to the pen origin, in pixels.
    pub left: i32,
    /// Top edge relative to the baseline, in pixels (positive is up).
    pub top: i32,
    /// One byte of coverage per pixel, row-major, `width * height` long.
    pub coverage: Vec<u8>,
}

impl GlyphBitmap {
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// A rasterized colour glyph: premultiplied RGBA plus placement relative to the pen.
#[derive(Clone, Debug)]
pub struct ColorBitmap {
    pub width: u32,
    pub height: u32,
    /// Left edge relative to the pen origin, in pixels.
    pub left: i32,
    /// Top edge relative to the baseline, in pixels (positive is up).
    pub top: i32,
    /// Four bytes of premultiplied RGBA per pixel, row-major.
    pub rgba: Vec<u8>,
}

/// Where a decoration line sits and how heavy it is, in em with y up.
///
/// `position` is the line's top edge from the baseline, positive above it, so the
/// underline's value is normally negative and the strikeout's positive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecorationMetrics {
    pub position: f32,
    pub thickness: f32,
}

impl DecorationMetrics {
    const UNDERLINE: Self = Self {
        position: -0.1,
        thickness: 0.05,
    };
    const STRIKEOUT: Self = Self {
        position: 0.25,
        thickness: 0.05,
    };
}

/// Metrics read from the font's own tables, not guessed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RealMetrics {
    pub units_per_em: f32,
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub underline: DecorationMetrics,
    pub strikeout: DecorationMetrics,
}

impl Default for RealMetrics {
    /// Metrics of a typical sans-serif, used when no font is loaded.
    fn default() -> Self {
        Self {
            units_per_em: 1000.0,
            ascent: 0.8,
            descent: -0.2,
            line_gap: 0.0,
            underline: DecorationMetrics::UNDERLINE,
            strikeout: DecorationMetrics::STRIKEOUT,
        }
    }
}

/// Parsed font plus a glyph outline collector.
pub struct ParsedFace {
    /// Parsed once: a collection packs several faces into one blob, so the index matters
    /// and re-parsing per glyph would drop every swash scale-cache entry.
    font: crate::face::FaceData,
    /// Weight instance for a variable font; `None` for a static face.
    weight: Option<f32>,
    /// Axes beyond weight this instance sits at.
    axes: Variations,
    metrics: RealMetrics,
    /// Kept for the colour path: its plan cache is keyed by the font and the context.
    scale: swash::scale::ScaleContext,
    /// Whether the face can produce colour glyphs. Those keep the serial path: the
    /// scaler above carries per-face state and cannot be shared across threads.
    color: bool,
}

impl ParsedFace {
    /// Parse `data`, reading `head`/`hhea` for real metrics.
    pub fn new(data: impl Into<ui_text::FontBlob>) -> Option<Self> {
        Self::new_at(data, 0)
    }

    /// Parse `data` at a specific face index inside the file.
    pub fn new_at(data: impl Into<ui_text::FontBlob>, font_index: u32) -> Option<Self> {
        Self::new_instance(data, font_index, None)
    }

    /// Parse a variable-font instance at `weight`.
    pub fn new_instance(
        data: impl Into<ui_text::FontBlob>,
        font_index: u32,
        weight: Option<f32>,
    ) -> Option<Self> {
        Self::new_axes(data, font_index, weight, Variations::NONE)
    }

    /// Parse a variable-font instance at `weight` and the extra `axes`.
    pub fn new_axes(
        data: impl Into<ui_text::FontBlob>,
        font_index: u32,
        weight: Option<f32>,
        axes: Variations,
    ) -> Option<Self> {
        let font = crate::face::FaceData::parse(data, font_index as usize)?;
        let view = font.outline_font();
        let upem = view.head().map(|h| h.units_per_em()).unwrap_or(1000) as f32;
        let hhea = view.hhea().ok()?;
        // A font may omit either table; the fallbacks are the usual sans-serif values so a
        // decoration never disappears for lack of a metric.
        let underline = view
            .post()
            .ok()
            .map_or(DecorationMetrics::UNDERLINE, |post| DecorationMetrics {
                position: post.underline_position().to_i16() as f32 / upem,
                thickness: post.underline_thickness().to_i16() as f32 / upem,
            });
        let strikeout =
            view.os2()
                .ok()
                .map_or(DecorationMetrics::STRIKEOUT, |os2| DecorationMetrics {
                    position: os2.y_strikeout_position() as f32 / upem,
                    thickness: os2.y_strikeout_size() as f32 / upem,
                });
        let metrics = RealMetrics {
            units_per_em: upem,
            ascent: hhea.ascender().to_i16() as f32 / upem,
            descent: hhea.descender().to_i16() as f32 / upem,
            line_gap: hhea.line_gap().to_i16() as f32 / upem,
            underline,
            strikeout,
        };
        let color = view.colr().is_ok() || view.cbdt().is_ok() || view.sbix().is_ok();
        Some(Self {
            font,
            weight,
            axes,
            metrics,
            scale: swash::scale::ScaleContext::new(),
            color,
        })
    }

    /// Metrics normalised so the em maps to 1.0.
    pub fn metrics(&self) -> RealMetrics {
        self.metrics
    }

    /// Advance width of `gid` in em units, from `hmtx`.
    pub fn advance_em(&self, gid: u32) -> f32 {
        let font = self.font.outline_font();
        match font.hmtx() {
            Ok(hmtx) => {
                let upem = self.metrics.units_per_em;
                hmtx.advance(skrifa::GlyphId::new(gid))
                    .map(|a| a as f32 / upem)
                    .unwrap_or(0.6)
            }
            Err(_) => 0.6,
        }
    }

    /// Map a character to a glyph id through `cmap`.
    pub fn glyph_id(&self, c: char) -> Option<u32> {
        let gid = self.font.outline_font().charmap().map(c)?;
        if gid.to_u32() == 0 {
            None
        } else {
            Some(gid.to_u32())
        }
    }

    /// Whether any glyph of this face may come from a colour source.
    pub fn has_color(&self) -> bool {
        self.color
    }

    /// Normalized coordinates for this instance, empty for a static face.
    pub fn instance_coords(&self) -> Vec<skrifa::instance::NormalizedCoord> {
        crate::face::instance_coords(self.font.outline_font(), self.weight, self.axes)
    }

    /// Rasterize `gid` at `px` size into a coverage bitmap.
    pub fn rasterize(&self, gid: u32, px: f32) -> Option<GlyphBitmap> {
        self.rasterize_subpixel(gid, px, 0.0, 0.0)
    }

    /// Rasterize `gid` shifted right by `dx` and down by `dy` pixels, each in `[0, 1)`.
    ///
    /// A phase is a translation of the outline, so the bitmap it produces carries the
    /// subpixel position and the placement stays on whole device pixels; see
    /// `docs/src/engine/text-and-input.md`.
    pub fn rasterize_subpixel(&self, gid: u32, px: f32, dx: f32, dy: f32) -> Option<GlyphBitmap> {
        self.rasterize_with(gid, px, dx, dy, &self.instance_coords(), 0.0)
    }

    /// Rasterize at coordinates computed once for the whole run, stroking the contour when
    /// `outline` is positive.
    fn rasterize_with(
        &self,
        gid: u32,
        px: f32,
        dx: f32,
        dy: f32,
        coords: &[skrifa::instance::NormalizedCoord],
        outline: f32,
    ) -> Option<GlyphBitmap> {
        let font = self.font.outline_font();
        let outlines = font.outline_glyphs();
        let glyph = outlines.get(skrifa::GlyphId::new(gid))?;

        let mut pen = ContourPen::new(self.metrics.units_per_em, px, outline);
        pen.dx = dx;
        pen.dy = dy;
        // The instance's coordinates, not the default one: a variable font's Bold
        // outlines only exist at `wght=700`, so `LocationRef::default()` draws Regular.
        glyph
            .draw(
                DrawSettings::unhinted(Size::new(px), LocationRef::new(coords)),
                &mut pen,
            )
            .ok()?;

        pen.finish()
    }

    /// Rasterize `gid` through the font's colour layers as premultiplied RGBA.
    ///
    /// `None` for monochrome glyphs, so they keep the coverage path even inside a
    /// colour font. Colour sources are COLR v0 outlines and CBDT/sbix bitmaps.
    pub fn rasterize_color(&mut self, gid: u32, px: f32) -> Option<ColorBitmap> {
        let settings = self.swash_settings();
        let mut builder = self.scale.builder(self.font.font()).size(px);
        if !settings.is_empty() {
            builder = builder.variations(settings);
        }
        let mut scaler = builder.build();
        let render = swash::scale::Render::new(&[
            swash::scale::Source::ColorOutline(0),
            swash::scale::Source::ColorBitmap(swash::scale::StrikeWith::BestFit),
        ]);
        let image = render.render(&mut scaler, gid as u16)?;
        if image.content != swash::scale::image::Content::Color {
            return None;
        }
        let width = image.placement.width;
        let height = image.placement.height;
        if width == 0 || height == 0 {
            return None;
        }
        let len = (width * height * 4) as usize;
        let mut rgba = image.data;
        if rgba.len() < len {
            return None;
        }
        rgba.truncate(len);
        // swash composites colour layers with straight alpha; the atlas stores premultiplied.
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let a = pixel[3] as u32;
            pixel[0] = (pixel[0] as u32 * a / 255) as u8;
            pixel[1] = (pixel[1] as u32 * a / 255) as u8;
            pixel[2] = (pixel[2] as u32 * a / 255) as u8;
        }
        Some(ColorBitmap {
            width,
            height,
            left: image.placement.left,
            top: image.placement.top,
            rgba,
        })
    }

    /// Variation settings in swash's form, mirroring `instance_coords`.
    fn swash_settings(&self) -> Vec<(swash::Tag, f32)> {
        let mut settings: Vec<(swash::Tag, f32)> = Vec::with_capacity(4);
        if let Some(weight) = self.weight {
            settings.push((swash::tag_from_bytes(b"wght"), weight));
        }
        for (tag, value) in [
            (b"opsz", self.axes.optical_size),
            (b"FILL", self.axes.fill),
            (b"GRAD", self.axes.grade),
        ] {
            if let Some(value) = value {
                settings.push((swash::tag_from_bytes(tag), value));
            }
        }
        settings
    }
}

/// Collects outline contours and rasterizes them with `zeno`.
struct ContourPen {
    /// Scale from font units to pixels.
    scale: f32,
    /// Horizontal subpixel shift, in pixels.
    dx: f32,
    /// Vertical subpixel shift, in pixels, down the screen.
    dy: f32,
    /// Stroke width around the contour, in pixels; zero fills the contour instead.
    outline: f32,
    /// Display-scale (font units are y-up; screen is y-down).
    path: String,
    bounds: Option<(f32, f32, f32, f32)>,
}

impl ContourPen {
    fn new(_units_per_em: f32, _px: f32, outline: f32) -> Self {
        // skrifa already applies `Size` when it hands coordinates to the pen, so no
        // extra scaling belongs here; the pen receives pixels.
        Self {
            scale: 1.0,
            dx: 0.0,
            dy: 0.0,
            outline: outline.max(0.0),
            path: String::new(),
            bounds: None,
        }
    }

    /// One outline point in screen space, shifted by the phase.
    ///
    /// Every point goes through here, control points included: a phase translates the
    /// shape, and shifting only the anchors would bend each curve toward its old control
    /// point instead.
    fn place(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.scale + self.dx, -y * self.scale + self.dy)
    }

    fn track(&mut self, x: f32, y: f32) {
        self.bounds = Some(match self.bounds {
            None => (x, y, x, y),
            Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
        });
    }

    fn finish(self) -> Option<GlyphBitmap> {
        let (x0, y0, x1, y1) = self.bounds?;
        // `bounds` is in screen space (y grows down, baseline at y = 0), so the
        // glyph's top is the smallest y and its bottom the largest.
        //
        // The outline is painted outside the glyph, and the stroke that makes it is centred
        // on the contour: only its outer half shows, since the fill covers the inner half.
        // So the tile grows by the visible width on each side and the stroke is twice it.
        let pad = 1.0 + self.outline;
        let left = (x0 - pad).floor();
        let right = (x1 + pad).ceil();
        // Distance from the baseline up to the bitmap's top edge; this is what a
        // renderer subtracts from the baseline to place the glyph.
        let top = -((y0 - pad).floor());
        let bottom = (y1 + pad).ceil();
        let w = (right - left).max(0.0) as u32;
        let h = (bottom - (y0 - pad).floor()).max(0.0) as u32;
        if w == 0 || h == 0 {
            return None;
        }
        let mut coverage = vec![0u8; (w * h) as usize];

        // zeno renders in the path's own space; shift so the glyph fills the bitmap.
        let mut mask = zeno::Mask::new(self.path.as_str());
        mask.offset(zeno::Vector::new(-left, -((y0 - pad).floor())))
            .size(w, h);
        if self.outline > 0.0 {
            // A round join keeps the band's reach at exactly half the width, which is what
            // the tile was grown by; a miter would spike past it on sharp corners.
            let mut stroke = zeno::Stroke::new(self.outline * 2.0);
            stroke.join(zeno::Join::Round);
            mask.style(stroke);
        }
        mask.render_into(&mut coverage, None);

        Some(GlyphBitmap {
            width: w,
            height: h,
            left: left as i32,
            top: top as i32,
            coverage,
        })
    }
}

impl OutlinePen for ContourPen {
    fn move_to(&mut self, x: f32, y: f32) {
        let (px, py) = self.place(x, y);
        self.track(px, py);
        self.path.push_str(&format!("M {px},{py} "));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (px, py) = self.place(x, y);
        self.track(px, py);
        self.path.push_str(&format!("L {px},{py} "));
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (qx, qy) = self.place(cx, cy);
        let (px, py) = self.place(x, y);
        self.track(qx, qy);
        self.track(px, py);
        self.path.push_str(&format!("Q {qx},{qy} {px},{py} "));
    }

    fn curve_to(&mut self, cx1: f32, cy1: f32, cx2: f32, cy2: f32, x: f32, y: f32) {
        let (a, b) = self.place(cx1, cy1);
        let (c, d) = self.place(cx2, cy2);
        let (px, py) = self.place(x, y);
        self.track(a, b);
        self.track(c, d);
        self.track(px, py);
        self.path.push_str(&format!("C {a},{b} {c},{d} {px},{py} "));
    }

    fn close(&mut self) {
        self.path.push_str("Z ");
    }
}

/// One glyph ready to composite: atlas tile and destination, both in device pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphBlit {
    /// Tile in atlas texture coordinates, never scaled.
    pub atlas: ui_core::geom::Rect,
    /// Destination in device pixels.
    pub dest: ui_core::geom::Rect,
    /// Which atlas `atlas` refers to.
    pub kind: GlyphKind,
}

impl GlyphBlit {
    /// Destination in logical units, for primitives authored in logical space.
    pub fn dest_logical(&self, scale: f32) -> ui_core::geom::Rect {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        if (scale - 1.0).abs() < f32::EPSILON {
            return self.dest;
        }
        ui_core::geom::Rect::new(
            self.dest.origin.x / scale,
            self.dest.origin.y / scale,
            self.dest.size.w / scale,
            self.dest.size.h / scale,
        )
    }
}

/// A glyph atlas slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub left: i32,
    pub top: i32,
}

/// One atlas page grown by shelf packing, one or four bytes per pixel.
struct Shelf {
    width: u32,
    height: u32,
    bytes_per_pixel: u32,
    pixels: Vec<u8>,
    /// Current shelf cursor and row height.
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
}

impl Shelf {
    fn new(width: u32, height: u32, bytes_per_pixel: u32, eager: bool) -> Self {
        let pixels = if eager {
            vec![0u8; (width * height * bytes_per_pixel) as usize]
        } else {
            Vec::new()
        };
        Self {
            width,
            height,
            bytes_per_pixel,
            pixels,
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
        }
    }

    /// Allocate the page on first use; the colour page stays absent until an emoji arrives.
    fn ensure(&mut self) {
        if self.pixels.is_empty() {
            self.pixels.resize(
                (self.width * self.height * self.bytes_per_pixel) as usize,
                0,
            );
        }
    }

    /// Double the page height (and width once), preserving row contents.
    ///
    /// Rows are left-aligned, so widening never moves existing slots; growing the
    /// height adds blank rows underneath.
    fn grow(&mut self) -> bool {
        const MAX_SIDE: u32 = 4096;
        let new_height = (self.height * 2).min(MAX_SIDE);
        if new_height == self.height && self.width >= MAX_SIDE {
            return false;
        }
        let new_width = (self.width * 2).min(MAX_SIDE);
        if !self.pixels.is_empty() {
            let bpp = self.bytes_per_pixel as usize;
            let mut grown = vec![0u8; (new_width * new_height) as usize * bpp];
            let row_bytes = self.width as usize * bpp;
            for row in 0..self.height as usize {
                let src = row * row_bytes;
                let dst = row * new_width as usize * bpp;
                grown[dst..dst + row_bytes].copy_from_slice(&self.pixels[src..src + row_bytes]);
            }
            self.pixels = grown;
        }
        self.width = new_width;
        self.height = new_height;
        true
    }

    /// Shelf allocator: fill a row, then start a new one. The flag says the page
    /// was cleared, so the caller must drop the matching index.
    fn alloc(&mut self, w: u32, h: u32) -> Option<(Slot, bool)> {
        self.ensure();
        let mut cleared = false;
        loop {
            if w > self.width || h > self.height {
                return None;
            }
            if self.cursor_x + w > self.width {
                self.cursor_x = 0;
                self.cursor_y += self.row_height;
                self.row_height = 0;
            }
            if self.cursor_y + h <= self.height {
                break;
            }
            // Grow once before ever evicting; only a second overflow drops the page.
            if self.grow() {
                continue;
            }
            self.pixels.fill(0);
            self.cursor_x = 0;
            self.cursor_y = 0;
            self.row_height = 0;
            cleared = true;
        }
        let slot = Slot {
            x: self.cursor_x,
            y: self.cursor_y,
            width: w,
            height: h,
            left: 0,
            top: 0,
        };
        self.cursor_x += w;
        self.row_height = self.row_height.max(h);
        Some((slot, cleared))
    }

    fn write(&mut self, slot: Slot, src: &[u8]) {
        self.ensure();
        let bpp = self.bytes_per_pixel as usize;
        let n = slot.width as usize * bpp;
        for row in 0..slot.height {
            let from = (row * slot.width) as usize * bpp;
            let to = ((slot.y + row) * self.width + slot.x) as usize * bpp;
            self.pixels[to..to + n].copy_from_slice(&src[from..from + n]);
        }
    }

    fn region(&self, slot: Slot) -> Vec<u8> {
        let bpp = self.bytes_per_pixel as usize;
        let mut out = Vec::with_capacity((slot.width * slot.height) as usize * bpp);
        for row in 0..slot.height {
            let start = ((slot.y + row) * self.width + slot.x) as usize * bpp;
            out.extend_from_slice(&self.pixels[start..start + slot.width as usize * bpp]);
        }
        out
    }

    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Below this many misses, starting threads costs more than the rasterization saves.
const PARALLEL_RASTER_MINIMUM: usize = 64;
/// A batch takes at most this many threads, leaving the machine to the compositor.
const RASTER_THREADS_MAXIMUM: usize = 8;

/// Rasterize one run's misses, spreading them over threads when there are enough.
fn rasterize_misses(
    parsed: &ParsedFace,
    misses: &[(u32, ui_text::Phases)],
    px: f32,
    coords: &[skrifa::instance::NormalizedCoord],
    outline: f32,
) -> Vec<Option<GlyphBitmap>> {
    let threads = std::thread::available_parallelism()
        .map_or(1, |count| count.get())
        .min(RASTER_THREADS_MAXIMUM);
    if misses.len() < PARALLEL_RASTER_MINIMUM || threads < 2 {
        return misses
            .iter()
            .map(|(gid, phases)| {
                parsed.rasterize_with(
                    *gid,
                    px,
                    phases.x.as_float(),
                    phases.y.as_float(),
                    coords,
                    outline,
                )
            })
            .collect();
    }
    let mut bitmaps: Vec<Option<GlyphBitmap>> = misses.iter().map(|_| None).collect();
    let chunk = misses.len().div_ceil(threads);
    std::thread::scope(|scope| {
        for (part, out) in misses.chunks(chunk).zip(bitmaps.chunks_mut(chunk)) {
            scope.spawn(move || {
                for ((gid, phases), slot) in part.iter().zip(out.iter_mut()) {
                    *slot = parsed.rasterize_with(
                        *gid,
                        px,
                        phases.x.as_float(),
                        phases.y.as_float(),
                        coords,
                        outline,
                    );
                }
            });
        }
    });
    bitmaps
}

/// The cache key of one glyph tile: face, glyph, quarter-pixel size, both phases, and the
/// outline width in quarter pixels. Two outlines of one glyph are different tiles, since
/// the coverage includes the band the stroke made.
type EntryKey = (FaceId, u32, u32, u8, u8, u32);

fn outline_bucket(outline: f32) -> u32 {
    (outline.max(0.0) * 4.0).round() as u32
}

/// Coverage and colour pages, plus the slots indexed into them.
pub struct GlyphCache {
    faces: HashMap<FaceId, ParsedFace>,
    /// Keyed by face, glyph, quarter-pixel size bucket, both subpixel phases and the
    /// quarter-pixel outline width. `None` records a glyph with no pixels at all (spaces,
    /// `.notdef`), so it is not re-rasterized every frame.
    entries: HashMap<EntryKey, Option<Slot>>,
    /// Colour entries ignore the phase: emoji do not need quarter-pixel positioning.
    color_entries: HashMap<(FaceId, u32, u32, u8), Slot>,
    coverage: Shelf,
    color: Shelf,
    pub stats: CacheStats,
    /// Changes whenever a colour entry is added or the colour page moves.
    pub color_version: u64,
}

/// Cache hit/miss counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub rasterized: u64,
    pub evictions: u64,
    /// How many times the atlas widened instead of evicting.
    pub grows: u64,
}

impl GlyphCache {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            faces: HashMap::new(),
            entries: HashMap::new(),
            color_entries: HashMap::new(),
            coverage: Shelf::new(width, height, 1, true),
            color: Shelf::new(width, height, 4, false),
            stats: CacheStats::default(),
            color_version: 0,
        }
    }

    /// Register a face's bytes; returns whether it parsed.
    pub fn add_face(&mut self, face: FaceId, data: impl Into<ui_text::FontBlob>) -> bool {
        self.add_face_at(face, data, 0)
    }

    /// Add a face that lives at `font_index` inside a collection file.
    pub fn add_face_at(
        &mut self,
        face: FaceId,
        data: impl Into<ui_text::FontBlob>,
        font_index: u32,
    ) -> bool {
        self.add_face_instance(face, data, font_index, None)
    }

    /// Add a variable-font instance, so its rasterised glyphs use the instance's axis.
    pub fn add_face_instance(
        &mut self,
        face: FaceId,
        data: impl Into<ui_text::FontBlob>,
        font_index: u32,
        weight: Option<f32>,
    ) -> bool {
        self.add_face_axes(face, data, font_index, weight, Variations::NONE)
    }

    /// Add an instance at `weight` and the extra `axes`.
    pub fn add_face_axes(
        &mut self,
        face: FaceId,
        data: impl Into<ui_text::FontBlob>,
        font_index: u32,
        weight: Option<f32>,
        axes: Variations,
    ) -> bool {
        match ParsedFace::new_axes(data, font_index, weight, axes) {
            Some(parsed) => {
                self.faces.insert(face, parsed);
                true
            }
            None => false,
        }
    }

    pub fn face_metrics(&self, face: FaceId) -> Option<RealMetrics> {
        self.faces.get(&face).map(|f| f.metrics())
    }

    /// Advance width of `gid` at `px`.
    pub fn advance_px(&self, face: FaceId, gid: u32, px: f32) -> Option<f32> {
        self.faces.get(&face).map(|f| f.advance_em(gid) * px)
    }

    pub fn glyph_id(&self, face: FaceId, c: char) -> Option<u32> {
        self.faces.get(&face)?.glyph_id(c)
    }

    /// Coverage bitmap for `c`, rasterizing once. Returns the slot plus the
    /// underlying coverage bytes so the caller can composite.
    pub fn glyph(&mut self, face: FaceId, c: char, px: f32) -> Option<(Slot, Vec<u8>)> {
        let gid = self.glyph_id(face, c)?;
        self.glyph_by_id(face, gid, px)
    }

    /// Coverage bitmap for a shaped glyph id at `px`.
    pub fn glyph_for_id(&mut self, face: FaceId, gid: u16, px: f32) -> Option<(Slot, Vec<u8>)> {
        self.glyph_by_id(face, gid as u32, px)
    }

    /// Rasterize the run's missing glyphs before the per-glyph lookups.
    ///
    /// The first frame of a page misses hundreds of glyphs; rasterizing them one
    /// at a time leaves every other core idle. Colour faces keep the serial path.
    fn prepare_run(&mut self, face: FaceId, placed: &[crate::PlacedGlyph], px: f32, outline: f32) {
        // Deduplication can only lower the miss count, so a shorter run cannot qualify.
        if placed.len() < PARALLEL_RASTER_MINIMUM {
            return;
        }
        let Some(parsed) = self.faces.get(&face) else {
            return;
        };
        // An outlined glyph cannot come from a colour source, so the batch stays serial only
        // for the fill it still paints.
        if parsed.has_color() && outline <= 0.0 {
            return;
        }
        let size_key = (px * 4.0).round() as u32;
        let outline_key = outline_bucket(outline);
        let mut seen = HashSet::new();
        let mut misses = Vec::new();
        for glyph in placed {
            let gid = glyph.glyph_id as u32;
            if !seen.insert((gid, glyph.phases)) {
                continue;
            }
            let key = (
                face,
                gid,
                size_key,
                glyph.phases.x.index(),
                glyph.phases.y.index(),
                outline_key,
            );
            if self.entries.contains_key(&key) {
                continue;
            }
            misses.push((gid, glyph.phases));
        }
        if misses.is_empty() {
            return;
        }
        let coords = parsed.instance_coords();
        let bitmaps = rasterize_misses(parsed, &misses, px, &coords, outline);
        self.stats.misses += misses.len() as u64;
        for ((gid, phases), bitmap) in misses.into_iter().zip(bitmaps) {
            let key = (
                face,
                gid,
                size_key,
                phases.x.index(),
                phases.y.index(),
                outline_key,
            );
            let Some(bitmap) = bitmap else {
                // A space or `.notdef` has no pixels; recording the answer stops the
                // next frame rebuilding a scaler only to learn that again.
                self.entries.insert(key, None);
                continue;
            };
            self.stats.rasterized += 1;
            let Some(slot) = self.alloc_in(GlyphKind::Coverage, bitmap.width, bitmap.height) else {
                continue;
            };
            self.coverage.write(slot, &bitmap.coverage);
            let slot = Slot {
                left: bitmap.left,
                top: bitmap.top,
                ..slot
            };
            self.entries.insert(key, Some(slot));
        }
    }

    /// Rasterize a placed run at device resolution and return 1:1 blits.
    ///
    /// Rasterizing at `size` instead of `size * scale` yields glyphs that are too
    /// small; leaving the destination in logical units makes the renderer rescale
    /// the tile, which is what looks like blur.
    pub fn blit_run(
        &mut self,
        face: FaceId,
        placed: &[crate::PlacedGlyph],
        size: f32,
        scale: f32,
    ) -> Vec<GlyphBlit> {
        self.blit_run_outlined(face, placed, size, scale, 0.0)
    }

    /// The same run, its glyphs stroked `outline` logical units wide instead of filled.
    ///
    /// The outline is its own tile: the coverage holds the band the stroke made, so the
    /// renderer draws it in the outline's ink and then the fill on top of it.
    pub fn blit_run_outlined(
        &mut self,
        face: FaceId,
        placed: &[crate::PlacedGlyph],
        size: f32,
        scale: f32,
        outline: f32,
    ) -> Vec<GlyphBlit> {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let device_px = size * scale;
        let outline_px = outline * scale;
        self.prepare_run(face, placed, device_px, outline_px);
        placed
            .iter()
            .filter_map(|glyph| {
                self.blit_glyph(
                    face,
                    glyph.glyph_id,
                    glyph.origin_x,
                    glyph.baseline_y,
                    device_px,
                    glyph.phases,
                    outline_px,
                )
            })
            .collect()
    }

    /// One glyph: the primitive both shaped runs and formulas draw.
    ///
    /// `baseline_y` is the exact device baseline. The tile already carries the phase it was
    /// rasterized at, so placing it from that baseline leaves the quarter pixel the phase did
    /// not take as a fractional destination, which the renderer samples through.
    pub fn blit_glyph(
        &mut self,
        face: FaceId,
        glyph_id: u16,
        origin_x: i32,
        baseline_y: f32,
        px: f32,
        phases: ui_text::Phases,
        outline: f32,
    ) -> Option<GlyphBlit> {
        let (slot, kind) = self.entry_by_id_phased(face, glyph_id as u32, px, phases, outline)?;
        // The tile is rasterized from a baseline shifted by the phase, so the placement takes
        // that shift back out; a colour tile is phase-independent and carries none of it.
        let phase = match kind {
            GlyphKind::Coverage => phases.y.as_float(),
            GlyphKind::Color => 0.0,
        };
        Some(GlyphBlit {
            atlas: ui_core::geom::Rect::new(
                slot.x as f32,
                slot.y as f32,
                slot.width as f32,
                slot.height as f32,
            ),
            dest: ui_core::geom::Rect::new(
                (origin_x + slot.left) as f32,
                baseline_y - phase - slot.top as f32,
                slot.width as f32,
                slot.height as f32,
            ),
            kind,
        })
    }

    /// Coverage for a shaped glyph id at a subpixel phase.
    pub fn glyph_for_id_phased(
        &mut self,
        face: FaceId,
        gid: u16,
        px: f32,
        phases: ui_text::Phases,
    ) -> Option<(Slot, Vec<u8>)> {
        self.glyph_by_id_phased(face, gid as u32, px, phases)
    }

    /// Coverage bitmap by glyph id.
    pub fn glyph_by_id(&mut self, face: FaceId, gid: u32, px: f32) -> Option<(Slot, Vec<u8>)> {
        self.glyph_by_id_phased(face, gid, px, ui_text::Phases::ZERO)
    }

    /// Pixels for a glyph at one phase per axis: one coverage byte or four premultiplied
    /// RGBA bytes per pixel, depending on which atlas it landed in.
    pub fn glyph_by_id_phased(
        &mut self,
        face: FaceId,
        gid: u32,
        px: f32,
        phases: ui_text::Phases,
    ) -> Option<(Slot, Vec<u8>)> {
        let (slot, kind) = self.entry_by_id_phased(face, gid, px, phases, 0.0)?;
        let region = match kind {
            GlyphKind::Coverage => self.coverage.region(slot),
            GlyphKind::Color => self.color.region(slot),
        };
        Some((slot, region))
    }

    /// Which atlas holds `gid`, rasterizing on a miss.
    pub fn entry_kind(
        &mut self,
        face: FaceId,
        gid: u32,
        px: f32,
        phases: ui_text::Phases,
    ) -> Option<GlyphKind> {
        self.entry_by_id_phased(face, gid, px, phases, 0.0)
            .map(|(_, kind)| kind)
    }

    /// Resolve a glyph to an atlas slot, rasterizing and caching it on a miss.
    fn entry_by_id_phased(
        &mut self,
        face: FaceId,
        gid: u32,
        px: f32,
        phases: ui_text::Phases,
        outline: f32,
    ) -> Option<(Slot, GlyphKind)> {
        let size_key = (px * 4.0).round() as u32; // quarter-pixel buckets
        let outline_key = outline_bucket(outline);
        let key = (
            face,
            gid,
            size_key,
            phases.x.index(),
            phases.y.index(),
            outline_key,
        );
        if let Some(cached) = self.entries.get(&key) {
            self.stats.hits += 1;
            return cached.map(|slot| (slot, GlyphKind::Coverage));
        }
        // Colour entries are phase-independent, so every phase shares the zero bucket.
        let color_key = (face, gid, size_key, 0);
        // An outlined glyph is a coverage tile whatever the face: a colour source carries
        // its own inks and has no contour to stroke.
        if outline <= 0.0
            && let Some(slot) = self.color_entries.get(&color_key)
        {
            self.stats.hits += 1;
            return Some((*slot, GlyphKind::Color));
        }
        self.stats.misses += 1;

        // A colour source wins when the glyph has one; monochrome glyphs fall through.
        let color = {
            if outline > 0.0 {
                None
            } else {
                let parsed = self.faces.get_mut(&face)?;
                parsed.rasterize_color(gid, px)
            }
        };
        if let Some(bitmap) = color {
            self.stats.rasterized += 1;
            let slot = self.alloc_in(GlyphKind::Color, bitmap.width, bitmap.height)?;
            self.color.write(slot, &bitmap.rgba);
            let slot = Slot {
                left: bitmap.left,
                top: bitmap.top,
                ..slot
            };
            self.color_entries.insert(color_key, slot);
            self.color_version += 1;
            return Some((slot, GlyphKind::Color));
        }

        let bitmap = self.faces.get(&face).and_then(|parsed| {
            parsed.rasterize_with(
                gid,
                px,
                phases.x.as_float(),
                phases.y.as_float(),
                &parsed.instance_coords(),
                outline,
            )
        });
        let Some(bitmap) = bitmap else {
            // A space or `.notdef` has no pixels at all. Remember that, or every frame rebuilds
            // a swash scaler just to discover it again.
            self.entries.insert(key, None);
            return None;
        };
        self.stats.rasterized += 1;

        let slot = self.alloc_in(GlyphKind::Coverage, bitmap.width, bitmap.height)?;
        self.coverage.write(slot, &bitmap.coverage);
        let slot = Slot {
            left: bitmap.left,
            top: bitmap.top,
            ..slot
        };
        self.entries.insert(key, Some(slot));
        Some((slot, GlyphKind::Coverage))
    }

    /// Allocate a tile in the requested page, dropping its index on eviction.
    fn alloc_in(&mut self, kind: GlyphKind, w: u32, h: u32) -> Option<Slot> {
        let (before, slot, cleared, after) = {
            let shelf = match kind {
                GlyphKind::Coverage => &mut self.coverage,
                GlyphKind::Color => &mut self.color,
            };
            let before = shelf.dimensions();
            let allocated = shelf.alloc(w, h);
            let after = shelf.dimensions();
            let (slot, cleared) = allocated?;
            (before, slot, cleared, after)
        };
        if after != before {
            self.stats.grows += 1;
            if kind == GlyphKind::Color {
                self.color_version += 1;
            }
        }
        if cleared {
            match kind {
                GlyphKind::Coverage => self.entries.clear(),
                GlyphKind::Color => self.color_entries.clear(),
            }
            self.stats.evictions += 1;
            if kind == GlyphKind::Color {
                self.color_version += 1;
            }
        }
        Some(slot)
    }

    /// Full coverage atlas, for debugging or uploading as a texture.
    pub fn atlas(&self) -> &[u8] {
        &self.coverage.pixels
    }

    /// Full colour atlas: premultiplied RGBA, empty until a colour glyph is rasterized.
    pub fn color_atlas(&self) -> &[u8] {
        &self.color.pixels
    }

    pub fn dimensions(&self) -> (u32, u32) {
        self.coverage.dimensions()
    }

    pub fn color_dimensions(&self) -> (u32, u32) {
        self.color.dimensions()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Any resolvable sans face, via the platform provider rather than `fc-match`,
    /// so the same tests work on fontconfig and DirectWrite.
    fn any_system_font() -> Option<ui_text::FontBlob> {
        crate::tests::tests_support::sans_face()
    }

    #[test]
    fn real_metrics_come_from_the_font_tables() {
        let Some(data) = any_system_font() else {
            return;
        };
        let face = ParsedFace::new(data).expect("parse");
        let m = face.metrics();
        assert!(m.units_per_em >= 16.0, "upem must be a real value");
        assert!(m.ascent > 0.5 && m.ascent < 1.5, "ascent em ratio");
        assert!(m.descent < 0.0, "descent is negative in font units");
    }

    /// A glyph with no pixels is resolved once, then served from the cache like any other.
    ///
    /// Otherwise every frame rebuilds a swash scaler for every space on the page, and the
    /// CFF table parse that comes with it costs more than drawing the text.
    #[test]
    fn a_glyph_with_no_pixels_is_remembered_instead_of_retried() {
        let Some(data) = any_system_font() else {
            return;
        };
        let mut cache = GlyphCache::new(64, 64);
        let face = FaceId(0);
        assert!(cache.add_face(face, data));
        let gid = cache.glyph_id(face, ' ').expect("a space has a glyph id");
        assert!(
            cache.glyph_by_id(face, gid, 12.0).is_none(),
            "a space has no pixels"
        );
        let missed = cache.stats.misses;
        assert!(cache.glyph_by_id(face, gid, 12.0).is_none());
        assert_eq!(
            cache.stats.misses, missed,
            "the second look-up must not reach the font"
        );
    }

    #[test]
    fn a_phase_translates_the_outline_instead_of_bending_it() {
        let Some(data) = any_system_font() else {
            return;
        };
        let face = ParsedFace::new(data).expect("font");
        // A whole pixel of phase is a translation: every point moves by one, control points
        // included, so the tile must hold the same picture one pixel to the right.
        for letter in ['o', 'S', 'e', 'g'] {
            let Some(gid) = face.glyph_id(letter) else {
                continue;
            };
            let flat = face
                .rasterize_subpixel(gid, 24.0, 0.0, 0.0)
                .expect("raster");
            let moved = face
                .rasterize_subpixel(gid, 24.0, 1.0, 0.0)
                .expect("raster");
            assert_eq!(
                (moved.width, moved.height),
                (flat.width, flat.height),
                "{letter}"
            );
            assert_eq!(moved.left, flat.left + 1, "{letter} did not move sideways");
            let worst = flat
                .coverage
                .iter()
                .zip(&moved.coverage)
                .map(|(flat, moved)| u8::abs_diff(*flat, *moved))
                .max()
                .unwrap_or(0);
            assert!(
                worst <= 2,
                "{letter} changed shape by {worst} levels under a phase"
            );
        }
    }

    /// An outline is a band around the contour, in a tile of its own: wider than the fill by
    /// the width it shows, and painted where the fill has no tile at all.
    #[test]
    fn an_outline_is_a_band_outside_the_fill_in_its_own_tile() {
        let Some(data) = any_system_font() else {
            return;
        };
        let face = ParsedFace::new(data).expect("font");
        let Some(gid) = face.glyph_id('l') else {
            return;
        };
        let coords = face.instance_coords();
        let width = 3.0;
        let fill = face
            .rasterize_with(gid, 48.0, 0.0, 0.0, &coords, 0.0)
            .expect("a fill rasterizes");
        let band = face
            .rasterize_with(gid, 48.0, 0.0, 0.0, &coords, width)
            .expect("an outline rasterizes");

        let margin = fill.left - band.left;
        assert!(
            (2..=5).contains(&margin),
            "the tile grew by the outline it draws, left {margin}"
        );
        assert!(
            band.width >= fill.width + 4 && band.height >= fill.height + 4,
            "both axes grew: fill {}x{}, band {}x{}",
            fill.width,
            fill.height,
            band.width,
            band.height
        );

        // The band reaches into the margin the fill does not have: a column just outside the
        // fill's left edge has to carry ink for the outline to be visible at all.
        let x = margin - 2;
        let painted = (0..band.height).any(|y| {
            let at = (y * band.width + x as u32) as usize;
            band.coverage.get(at).is_some_and(|coverage| *coverage > 0)
        });
        assert!(painted, "the outline left the glyph's own edge empty");

        // The band straddles the contour, which is why only half of it shows once the fill is
        // drawn over it: the middle row is painted one pixel inside the glyph's own edge.
        let x = margin + 1;
        let inside = (0..band.height).any(|y| {
            let at = (y * band.width + x as u32) as usize;
            band.coverage.get(at).is_some_and(|coverage| *coverage > 0)
        });
        assert!(inside, "the outline sat wholly outside the contour");
        let filled = fill.coverage.iter().copied().max().unwrap_or(0);
        assert_eq!(filled, 255, "the fill still covers the glyph's own area");
    }

    #[test]
    fn different_phases_rasterize_different_bitmaps() {
        let Some(data) = any_system_font() else {
            return;
        };
        let face = ParsedFace::new(data.clone()).expect("font");
        let gid = face.glyph_id('l').expect("glyph");
        let phases: Vec<Vec<u8>> = (0..4)
            .map(|i| {
                face.rasterize_subpixel(gid, 32.0, i as f32 * 0.25, 0.0)
                    .expect("raster")
                    .coverage
            })
            .collect();
        // Every phase must differ from the next: identical bitmaps would mean the
        // offset never reached the rasterizer.
        for i in 0..3 {
            assert_ne!(
                phases[i],
                phases[i + 1],
                "phase {i} and {} produced identical coverage",
                i + 1
            );
        }
        // The vertical phase reaches it too, and by a whole half pixel.
        let flat = face
            .rasterize_subpixel(gid, 32.0, 0.0, 0.0)
            .expect("raster");
        let lower = face
            .rasterize_subpixel(gid, 32.0, 0.0, 0.5)
            .expect("raster");
        assert_ne!(flat.coverage, lower.coverage);
        let ink = |bitmap: &GlyphBitmap| {
            let mut sum = 0.0;
            let mut weight = 0.0;
            for (index, level) in bitmap.coverage.iter().enumerate() {
                let row = (index as u32 / bitmap.width) as f32;
                sum += row * f32::from(*level);
                weight += f32::from(*level);
            }
            sum / weight
        };
        let moved = (ink(&lower) - ink(&flat)) + (flat.top as f32 - lower.top as f32);
        assert!(
            (moved - 0.5).abs() < 0.06,
            "the ink moved {moved} px, not half of one"
        );

        // Through the cache as well: a vertical phase is its own tile, or the run's text
        // would all come from the zero-phase entry and never move by half a pixel.
        let mut cache = GlyphCache::new(512, 512);
        cache.add_face(FaceId(0), data);
        let zero = cache.glyph_by_id_phased(FaceId(0), gid, 32.0, ui_text::Phases::ZERO);
        let half = cache.glyph_by_id_phased(
            FaceId(0),
            gid,
            32.0,
            ui_text::Phases::new(ui_text::SubpixelBin::Zero, ui_text::VerticalBin::Half),
        );
        let (zero, half) = (zero.expect("tile"), half.expect("tile"));
        assert_ne!(zero.1, half.1, "the two phases share one tile");
    }

    #[test]
    fn a_baseline_between_pixels_reaches_the_destination() {
        let Some(data) = any_system_font() else {
            return;
        };
        let face = FaceId(0);
        let mut cache = GlyphCache::new(1024, 1024);
        cache.add_face(face, data.clone());
        let gid = cache.glyph_id(face, 'l').expect("glyph") as u16;
        let dest = |cache: &mut GlyphCache, baseline: f32| {
            let placed = crate::PlacedGlyph {
                glyph_id: gid,
                origin_x: 0,
                baseline_y: baseline,
                phases: ui_text::Phases::new(
                    ui_text::SubpixelBin::Zero,
                    ui_text::VerticalBin::split(baseline).1,
                ),
            };
            cache.blit_run(face, &[placed], 16.0, 1.0)[0].dest.origin.y
        };
        // An integral baseline stays on the grid: nothing about a still line changes.
        assert_eq!(dest(&mut cache, 20.0).fract(), 0.0);
        // A tenth of a pixel below it reaches the destination, which is what the sampler
        // interpolates; snapping it here is the step the whole change removes. The move is
        // taken inside one phase, so the tile is the same one and only the destination travels.
        assert!((dest(&mut cache, 20.1) - dest(&mut cache, 20.0) - 0.1).abs() < 1e-4);
        assert!((dest(&mut cache, 20.6) - dest(&mut cache, 20.5) - 0.1).abs() < 1e-4);
    }

    #[test]
    fn atlas_grows_instead_of_evicting_when_a_page_fills() {
        let Some(data) = any_system_font() else {
            return;
        };
        // A page far too small for a full alphabet forces growth.
        let mut cache = GlyphCache::new(64, 64);
        let face = FaceId(0);
        cache.add_face(face, data);
        for c in 'a'..='z' {
            cache.glyph(face, c, 32.0);
        }
        assert_eq!(
            cache.stats.evictions, 0,
            "growth must happen before any eviction"
        );
        assert!(cache.stats.grows > 0, "the page must have widened");
        assert!(cache.dimensions().0 > 64);
    }

    #[test]
    fn grow_preserves_existing_glyph_content() {
        let Some(data) = any_system_font() else {
            return;
        };
        let mut cache = GlyphCache::new(64, 64);
        let face = FaceId(0);
        cache.add_face(face, data);
        let (first, before) = cache.glyph(face, 'W', 32.0).expect("W");
        // Force several growths.
        for c in 'a'..='z' {
            cache.glyph(face, c, 32.0);
        }
        assert!(cache.stats.grows > 0);
        // The same slot must still hold the same pixels.
        let (again, after) = cache.glyph(face, 'W', 32.0).expect("W again");
        assert_eq!(first, again, "slot is stable across growth");
        assert_eq!(before, after, "coverage is preserved across growth");
    }

    #[test]
    fn advance_is_read_from_hmtx() {
        let Some(data) = any_system_font() else {
            return;
        };
        let face = ParsedFace::new(data).expect("parse");
        let gid = face.glyph_id('M').expect("glyph");
        let adv = face.advance_em(gid);
        assert!(
            adv > 0.2 && adv < 1.5,
            "advance em must be plausible: {adv}"
        );
    }

    #[test]
    fn a_colour_glyph_lands_in_the_rgba_page_instead_of_coverage() {
        let Some(data) = crate::tests::tests_support::emoji_face() else {
            eprintln!("skipped: no colour emoji face is installed");
            return;
        };
        let mut cache = GlyphCache::new(256, 256);
        let face = FaceId(0);
        assert!(cache.add_face(face, data));
        let Some(gid) = cache.glyph_id(face, '\u{1F600}') else {
            eprintln!("skipped: the emoji face has no U+1F600");
            return;
        };
        let bin = ui_text::SubpixelBin::Zero;
        assert_eq!(
            cache.entry_kind(
                face,
                gid,
                32.0,
                ui_text::Phases::new(bin, ui_text::VerticalBin::Zero)
            ),
            Some(GlyphKind::Color),
            "a glyph with colour layers must not take the coverage path"
        );
        let (slot, pixels) = cache
            .glyph_by_id_phased(
                face,
                gid,
                32.0,
                ui_text::Phases::new(bin, ui_text::VerticalBin::Zero),
            )
            .expect("colour glyph");
        assert!(slot.width > 0 && slot.height > 0);
        assert_eq!(
            pixels.len(),
            (slot.width * slot.height * 4) as usize,
            "the colour page stores four bytes per pixel"
        );
        assert!(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[0] > 0 || p[1] > 0 || p[2] > 0),
            "the colour entry must contain ink"
        );
        assert!(
            !cache.color_atlas().is_empty(),
            "the colour page must have been allocated"
        );
        assert_eq!(cache.color_version, 1, "one colour entry bumps the version");
    }

    /// A batch rasterized over several threads lands exactly where the serial path
    /// would put it: same slots, same pixels.
    #[test]
    fn a_parallel_glyph_batch_fills_the_atlas_like_a_serial_one() {
        if std::thread::available_parallelism().map_or(1, |count| count.get()) < 2 {
            return;
        }
        let Some(data) = any_system_font() else {
            return;
        };
        let face = FaceId(0);
        let count = PARALLEL_RASTER_MINIMUM as u16 + 32;
        let placed: Vec<crate::PlacedGlyph> = (0..count)
            .map(|index| crate::PlacedGlyph {
                glyph_id: 1 + index,
                origin_x: 0,
                baseline_y: 0.0,
                phases: ui_text::Phases::new(
                    ui_text::SubpixelBin::from_index((index % 4) as u8),
                    ui_text::VerticalBin::from_index((index % 2) as u8),
                ),
            })
            .collect();

        let mut batched = GlyphCache::new(1024, 1024);
        batched.add_face(face, data.clone());
        batched.blit_run(face, &placed, 16.0, 1.0);

        let mut serial = GlyphCache::new(1024, 1024);
        serial.add_face(face, data);
        for glyph in &placed {
            serial.blit_glyph(face, glyph.glyph_id, 0, 0.0, 16.0, glyph.phases, 0.0);
        }

        assert!(
            batched.atlas() == serial.atlas(),
            "the batch must fill the atlas exactly as the serial path does"
        );
    }

    /// What a first paint's missing glyphs cost, serial against batched. Ignored because
    /// it measures time: `cargo test --release -p ui-text-fontique --features system --
    /// --ignored --nocapture`.
    #[test]
    #[ignore = "timing measurement"]
    fn a_batch_of_missing_glyphs_costs_less_than_one_at_a_time() {
        use std::time::Instant;

        if std::thread::available_parallelism().map_or(1, |count| count.get()) < 2 {
            return;
        }
        let Some(data) = any_system_font() else {
            return;
        };
        let face = FaceId(0);
        for count in [32usize, 64, 128, 512, 2048] {
            let placed: Vec<crate::PlacedGlyph> = (0..count as u16)
                .map(|index| crate::PlacedGlyph {
                    glyph_id: 1 + index,
                    origin_x: 0,
                    baseline_y: 0.0,
                    phases: ui_text::Phases::new(
                        ui_text::SubpixelBin::from_index((index % 4) as u8),
                        ui_text::VerticalBin::from_index((index % 2) as u8),
                    ),
                })
                .collect();

            let mut serial = GlyphCache::new(2048, 2048);
            serial.add_face(face, data.clone());
            let started = Instant::now();
            for glyph in &placed {
                serial.blit_glyph(face, glyph.glyph_id, 0, 0.0, 16.0, glyph.phases, 0.0);
            }
            let serial_time = started.elapsed();

            let mut batched = GlyphCache::new(2048, 2048);
            batched.add_face(face, data.clone());
            let started = Instant::now();
            batched.blit_run(face, &placed, 16.0, 1.0);
            let batched_time = started.elapsed();

            assert_eq!(serial.stats.rasterized, batched.stats.rasterized);
            println!("{count:4} fresh glyphs: serial {serial_time:?}, batched {batched_time:?}");
        }
    }
}
