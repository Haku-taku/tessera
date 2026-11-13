//! Real glyphs end to end: font stack -> cmap -> outline -> atlas -> pixels.
//!
//! Requires the system font feature:
//! `cargo run -p ui-platform --example text_real --features ui-text-fontique/system`

use std::io::Write;

use ui_core::draw::{DrawList, GlyphPrim, RectPrim};
use ui_core::geom::{Color, Corners, Rect};
use ui_paint::{CoverageAtlas, CpuRenderer, Pixels};
use ui_text::{FontProvider, FontRequest, GenericFamily, Script};
use ui_text_fontique::glyph::GlyphCache;
use ui_text_fontique::system_or_isolated;

const W: u32 = 520;
const H: u32 = 220;
const SIZE: f32 = 34.0;

fn write_ppm(path: &str, pixels: &Pixels) -> std::io::Result<()> {
    let mut f = std::fs::File::create(path)?;
    write!(f, "P6\n{} {}\n255\n", pixels.width, pixels.height)?;
    let mut rgb = Vec::with_capacity((pixels.width * pixels.height * 3) as usize);
    for px in pixels.as_bytes().as_chunks::<4>().0 {
        rgb.push(px[2]);
        rgb.push(px[1]);
        rgb.push(px[0]);
    }
    f.write_all(&rgb)
}

fn main() {
    let provider = system_or_isolated();
    if !provider.has_system_fonts() {
        eprintln!("no system fonts; run with --features ui-text-fontique/system");
        return;
    }

    // Resolve a face for each run through the stack, then register its bytes.
    let mut cache = GlyphCache::new(1024, 1024);
    let mut face_by_run: Vec<(FaceIdWrapper, String)> = Vec::new();

    let runs = ["Hello, crab", "中文字体测试", "Sans 混排"];
    for text in runs {
        let req = FontRequest::new(GenericFamily::SansSerif).script(Script::dominant(text));
        let face = provider
            .resolve(&req)
            .or_else(|| {
                provider
                    .fallback_chain(Script::dominant(text))
                    .first()
                    .copied()
            })
            .expect("a face");
        if cache.face_metrics(ui_text::FaceId(face.0)).is_none() {
            let data = provider.face_data(face).expect("font bytes");
            cache.add_face(ui_text::FaceId(face.0), data);
        }
        face_by_run.push((FaceIdWrapper(face.0), text.to_string()));
    }

    // Build the draw list: a background, then one glyph prim per character.
    let owner = ui_core::NodeId::from_raw(0, 0);
    let mut list = DrawList::new(Rect::new(0.0, 0.0, W as f32, H as f32));
    list.push_rect(
        owner,
        RectPrim {
            rect: Rect::new(0.0, 0.0, W as f32, H as f32),
            radius: Corners::ZERO,
            fill: Color::from_rgba8(24, 26, 30, 255),
            border: None,
            border_gradient: None,
            gradient: None,
            notch: None,
        },
    );

    let mut y = 20.0_f32;
    let mut rendered = 0usize;
    for (FaceIdWrapper(face_raw), text) in &face_by_run {
        let face = ui_text::FaceId(*face_raw);
        println!("\nrun {:?} on {:?}", text, provider.face_family_name(face));
        if let Some(m) = cache.face_metrics(face) {
            println!(
                "  metrics: upem={} ascent={:.3} descent={:.3} line_gap={:.3}",
                m.units_per_em, m.ascent, m.descent, m.line_gap
            );
        }
        // Baseline and line height come from the font's own tables, not a guess.
        let (ascent, line_height) = match cache.face_metrics(face) {
            Some(m) => (m.ascent, (m.ascent - m.descent + m.line_gap).max(1.0)),
            None => (0.8, 1.3),
        };
        let baseline = y + ascent * SIZE;
        let mut pen_x = 24.0_f32;
        for c in text.chars() {
            let Some(gid) = cache.glyph_id(face, c) else {
                continue;
            };
            let advance = cache.advance_px(face, gid, SIZE).unwrap_or(SIZE * 0.6);
            let Some((slot, _cov)) = cache.glyph(face, c, SIZE) else {
                pen_x += advance;
                continue;
            };
            let dst = Rect::new(
                pen_x + slot.left as f32,
                baseline - slot.top as f32,
                slot.width as f32,
                slot.height as f32,
            );
            list.push_glyph(
                owner,
                GlyphPrim {
                    rect: dst,
                    atlas: Rect::new(
                        slot.x as f32,
                        slot.y as f32,
                        slot.width as f32,
                        slot.height as f32,
                    ),
                    alpha: 1.0,
                    color: Color::from_rgba8(232, 236, 244, 255),
                    kind: ui_core::draw::GlyphKind::Coverage,
                },
            );
            pen_x += advance;
            rendered += 1;
        }
        y += line_height * SIZE;
    }

    let (aw, ah) = cache.dimensions();
    let mut renderer = CpuRenderer::new();
    renderer.set_atlas(CoverageAtlas {
        width: aw,
        height: ah,
        data: cache.atlas().to_vec(),
    });

    let mut target = Pixels::new(W, H);
    renderer.render(&list, &mut target, &ui_core::Damage::Full);

    println!(
        "\nglyphs rendered: {rendered}  atlas: {}x{}  cache: {:?}",
        aw, ah, cache.stats
    );
    match write_ppm("text_real.ppm", &target) {
        Ok(()) => println!("wrote text_real.ppm"),
        Err(e) => eprintln!("write failed: {e}"),
    }
}

/// Small wrapper so the FaceId stays visible in the debug output.
#[derive(Clone, Copy, Debug)]
struct FaceIdWrapper(u32);
