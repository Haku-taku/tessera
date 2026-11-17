//! Tests for this module, split from the source file; one module per contract area.

mod contracts {
    use crate::*;
    use ui_core::id::NodeId;

    fn nid(i: u32) -> NodeId {
        NodeId::from_raw(i, 0)
    }

    fn prim(x: f32, y: f32, w: f32, h: f32, color: Color) -> RectPrim {
        RectPrim {
            rect: Rect::new(x, y, w, h),
            radius: Corners::ZERO,
            fill: color,
            border: None,
            border_gradient: None,
            gradient: None,
            notch: None,
        }
    }

    /// A backdrop must soften what is already painted under it, on the CPU path too.
    /// Every effect's visible influence stops inside the reach it reports: one lit pixel
    /// spreads no further than `guaranteed_reach`, and it does spread. The declaration is
    /// checked against the pixels, so a pass chain reading wider than its kernel claims
    /// shows up here rather than as a clipped tail at a layer edge.
    #[test]
    fn an_effect_reaches_no_further_than_it_reports() {
        let side = 101u32;
        let centre = side / 2;
        let effects = [
            ui_core::effect::Effect::blur(4, 1.5),
            ui_core::effect::Effect::blur(16, 1.5),
            ui_core::effect::Effect::blur(2, 3.0),
            ui_core::effect::Effect::box_blur(0.6),
            ui_core::effect::Effect::box_blur(2.5),
            ui_core::effect::Effect::box_blur(6.0),
        ];
        // A solid block, not one pixel: a wide box pass averaging a single lit pixel
        // rounds its own tail to nothing, and then the test measures no spread at all.
        let half = 10u32;
        let (x0, x1) = (centre - half, centre + half);
        for effect in effects {
            let mut pixels = Pixels::new(side, side);
            for y in x0..x1 {
                for x in x0..x1 {
                    let at = ((y * side + x) as usize) * 4;
                    pixels.data[at..at + 4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
            apply_effects(
                &mut pixels,
                std::slice::from_ref(&effect),
                Rect::new(0.0, 0.0, side as f32, side as f32),
                &mut Vec::new(),
            );
            let mut furthest = 0u32;
            for y in 0..side {
                for x in 0..side {
                    if pixels.data[((y * side + x) as usize) * 4 + 3] == 0 {
                        continue;
                    }
                    let out = x
                        .abs_diff(centre)
                        .saturating_sub(half)
                        .max(y.abs_diff(centre).saturating_sub(half));
                    furthest = furthest.max(out);
                }
            }
            let claimed =
                ui_core::effect::EffectReach::of(std::slice::from_ref(&effect)).guaranteed;
            assert!(
                furthest > 0,
                "{effect:?} must spread past the block it was given"
            );
            assert!(
                furthest <= claimed,
                "{effect:?} reaches {furthest} pixels out, past its claimed {claimed}"
            );
            assert!(
                furthest <= effect.band(),
                "{effect:?} reads past the band its own passes need"
            );
        }
    }

    #[test]
    fn a_backdrop_blurs_what_was_painted_before_it() {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 120.0, 60.0));
        // A hard-edged white block on black, then a backdrop over it.
        list.push_rect(
            nid(0),
            prim(40.0, 0.0, 20.0, 60.0, Color::from_rgba8(255, 255, 255, 255)),
        );
        list.push_backdrop(ui_core::draw::BackdropEntry {
            rect: Rect::new(10.0, 10.0, 100.0, 40.0),
            clip: Rect::new(10.0, 10.0, 100.0, 40.0),
            corners: Corners::ZERO,
            radius: 12,
            sigma: 6.0,
            material: ui_core::draw::BackdropMaterial::PLAIN,
        });
        // A full-frame black rect supplies the dark surround the blur mixes in.
        list.push_rect(
            nid(1),
            prim(0.0, 0.0, 0.0, 0.0, Color::from_rgba8(0, 0, 0, 0)),
        );

        let mut target = Pixels::new(120, 60);
        CpuRenderer::new().render(&list, &mut target, &Damage::Full);

        // The block's interior must no longer be pure white: the blur pulled the black
        // surround inward, which a skipped backdrop would not do.
        let inside = target.get(50, 30);
        assert!(
            inside[0] < 250,
            "the backdrop must soften the block's interior, got {inside:?}"
        );
        // Far outside the backdrop rect nothing may change.
        assert_eq!(
            target.get(115, 55),
            [0, 0, 0, 0],
            "outside the backdrop must stay untouched"
        );
    }

    /// A rounded frost writes its blur through the box's own shape: a square write leaves
    /// blurred pixels outside the corner, which reads as a grey square around the panel.
    #[test]
    fn a_rounded_frost_keeps_the_pixels_outside_its_corners() {
        let scene = |corners: Corners| {
            let mut list = DrawList::new(Rect::new(0.0, 0.0, 60.0, 60.0));
            // A hard edge at x = 6, close enough that a square write would blur the corner.
            list.push_rect(
                nid(0),
                prim(0.0, 0.0, 60.0, 60.0, Color::from_rgba8(0, 0, 0, 255)),
            );
            list.push_rect(
                nid(1),
                prim(6.0, 0.0, 54.0, 60.0, Color::from_rgba8(255, 255, 255, 255)),
            );
            list.push_backdrop(ui_core::draw::BackdropEntry {
                rect: Rect::new(0.0, 0.0, 40.0, 40.0),
                clip: Rect::new(0.0, 0.0, 40.0, 40.0),
                corners,
                radius: 9,
                sigma: 3.0,
                material: ui_core::draw::BackdropMaterial::PLAIN,
            });
            let mut target = Pixels::new(60, 60);
            CpuRenderer::new().render(&list, &mut target, &Damage::Full);
            target
        };
        let rounded = scene(Corners::all(12.0));
        // (1, 1) is outside the 12 dp arc, so the black block must still be black there.
        assert_eq!(
            rounded.get(1, 1),
            [0, 0, 0, 255],
            "a rounded frost must not paint outside its corner"
        );
        // Under the frost and clear of every corner, the edge is mixed: the blur ran.
        let inside = rounded.get(10, 20);
        assert!(
            inside[0] > 10 && inside[0] < 245,
            "the frost must blur the edge it covers, got {inside:?}"
        );
        // The same scene without corners blurs that pixel, which is what the mask prevents.
        let square = scene(Corners::ZERO);
        assert!(
            square.get(1, 1)[0] > 10,
            "without corners the blur reaches (1, 1), so the check above is not vacuous"
        );
    }

    /// A frost is geometry, so at 2x it must cover the box's device pixels, not its logical
    /// ones: a logical rect on a device buffer frosts the top-left quarter of the panel.
    #[test]
    fn a_scaled_frost_covers_the_boxes_device_pixels() {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 60.0, 60.0));
        // White for logical x in 10..20, black elsewhere: a block the blur can pull in.
        list.push_rect(
            nid(0),
            prim(0.0, 0.0, 60.0, 60.0, Color::from_rgba8(0, 0, 0, 255)),
        );
        list.push_rect(
            nid(1),
            prim(10.0, 0.0, 10.0, 60.0, Color::from_rgba8(255, 255, 255, 255)),
        );
        // Logical (0, 0, 30, 30) is device (0, 0, 60, 60) at scale 2.
        list.push_backdrop(ui_core::draw::BackdropEntry {
            rect: Rect::new(0.0, 0.0, 30.0, 30.0),
            clip: Rect::new(0.0, 0.0, 30.0, 30.0),
            corners: Corners::ZERO,
            radius: 9,
            sigma: 3.0,
            material: ui_core::draw::BackdropMaterial::PLAIN,
        });
        let mut target = Pixels::new(120, 120);
        let mut renderer = CpuRenderer::new();
        renderer.set_scale(2.0);
        renderer.render(&list, &mut target, &Damage::Full);
        // Device (45, 45) lies past the logical box but inside the scaled one, and its blur
        // reaches the white block: reading the rect as device pixels leaves it black.
        let inside = target.get(45, 45);
        assert!(
            inside[0] > 10,
            "the frost must reach the whole scaled box, got {inside:?}"
        );
        // Device (70, 70) is outside the box and must keep the plain fill.
        assert_eq!(target.get(70, 70), [0, 0, 0, 255], "outside the box");
    }

    /// A frosted surface inside a group layer frosts its parent: the layer starts empty,
    /// so without that the surface would land on nothing.
    #[test]
    fn a_backdrop_inside_a_group_layer_frosts_its_parent() {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 120.0, 60.0));
        // The white block sits beside the frost rect, so only the blur can bring it in.
        list.push_rect(
            nid(0),
            prim(45.0, 0.0, 15.0, 60.0, Color::from_rgba8(255, 255, 255, 255)),
        );
        list.push_layer(
            Rect::new(60.0, 10.0, 50.0, 40.0),
            vec![ui_core::Effect::opacity(0.5)],
            Corners::ZERO,
        );
        list.push_backdrop(ui_core::draw::BackdropEntry {
            rect: Rect::new(60.0, 10.0, 50.0, 40.0),
            clip: Rect::new(60.0, 10.0, 50.0, 40.0),
            corners: Corners::ZERO,
            radius: 12,
            sigma: 6.0,
            material: ui_core::draw::BackdropMaterial::PLAIN,
        });
        // The frosted surface's own clear box keeps the layer open past the backdrop.
        list.push_rect(nid(2), prim(60.0, 10.0, 50.0, 40.0, Color::TRANSPARENT));
        list.pop_layer();
        list.push_rect(
            nid(1),
            prim(0.0, 0.0, 0.0, 0.0, Color::from_rgba8(0, 0, 0, 0)),
        );

        let mut target = Pixels::new(120, 60);
        CpuRenderer::new().render(&list, &mut target, &Damage::Full);

        // The frost pulls the block's white into the layer, which then composites at 50%;
        // an empty layer would leave the black frame showing through instead.
        let inside = target.get(63, 30);
        assert!(
            inside[0] > 5,
            "the layer must carry the frosted parent, got {inside:?}"
        );
    }

    #[test]
    fn a_translucent_layer_composites_by_coverage_not_by_raw_channel() {
        // A layer holds one straight-alpha pixel (red at ~50%); the destination below is
        // opaque blue. Straight-over halves both channels; a premultiplied reading kept
        // the red whole and let no blue through.
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 4.0, 4.0));
        // The blue must be painted in this frame: the renderer repaints the whole surface,
        // so a colour pre-filled into the target would be cleared before the layer runs.
        list.push_rect(
            NodeId::from_raw(0, 0),
            RectPrim {
                rect: Rect::new(0.0, 0.0, 4.0, 4.0),
                radius: Corners::ZERO,
                fill: Color::from_rgba8(0, 0, 255, 255),
                border: None,
                border_gradient: None,
                gradient: None,
                notch: None,
            },
        );
        list.push_layer(Rect::new(0.0, 0.0, 4.0, 4.0), Vec::new(), Corners::ZERO);
        list.push_rect(
            NodeId::from_raw(1, 0),
            RectPrim {
                rect: Rect::new(1.0, 1.0, 2.0, 2.0),
                radius: Corners::ZERO,
                fill: Color::from_rgba8(255, 0, 0, 128),
                border: None,
                border_gradient: None,
                gradient: None,
                notch: None,
            },
        );
        list.pop_layer();
        let mut px = Pixels::new(4, 4);
        CpuRenderer::new().render(
            &list,
            &mut px,
            &ui_core::damage::Damage::Rects(vec![Rect::new(0.0, 0.0, 4.0, 4.0)]),
        );
        let got = px.get(2, 2);
        assert!(
            got[0].abs_diff(128) <= 2 && got[2].abs_diff(127) <= 2,
            "expected red and blue halved, got {got:?}"
        );
    }

    #[test]
    fn half_alpha_white_over_black_lands_mid_grey() {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 4.0, 4.0));
        list.push_rect(
            nid(0),
            prim(0.0, 0.0, 4.0, 4.0, Color::from_rgba8(0, 0, 0, 255)),
        );
        list.push_rect(
            nid(1),
            prim(0.0, 0.0, 4.0, 4.0, Color::from_rgba8(255, 255, 255, 128)),
        );

        let mut target = Pixels::new(4, 4);
        CpuRenderer::new().render(&list, &mut target, &Damage::Full);
        let px = target.get(2, 2);
        assert!(
            px[0] > 100 && px[0] < 160,
            "50% white over black should be mid grey, got {px:?}"
        );
    }

    #[test]
    fn a_stroked_line_covers_its_segment_only() {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 20.0, 20.0));
        list.push_line(
            nid(0),
            ui_core::draw::LinePrim {
                from: ui_core::geom::Point::new(2.0, 2.0),
                to: ui_core::geom::Point::new(18.0, 18.0),
                thickness: 2.0,
                colour: Color::WHITE,
            },
        );

        let mut target = Pixels::new(20, 20);
        CpuRenderer::new().render(&list, &mut target, &Damage::Full);
        let on = target.get(10, 10);
        assert!(on[3] > 200, "the segment's middle is painted, got {on:?}");
        let off = target.get(10, 16);
        assert_eq!(off[3], 0, "a pixel away from the segment stays clear");
    }

    #[test]
    fn clip_excludes_pixels_outside_the_clip_rect() {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 20.0, 20.0));
        list.push_clip(Rect::new(0.0, 0.0, 10.0, 20.0), Corners::ZERO);
        list.push_rect(nid(0), prim(0.0, 0.0, 20.0, 20.0, Color::WHITE));
        list.pop_clip();

        let mut target = Pixels::new(20, 20);
        CpuRenderer::new().render(&list, &mut target, &Damage::Full);

        assert_eq!(target.get(5, 10), [255, 255, 255, 255], "inside clip");
        assert_eq!(
            target.get(15, 10),
            [0, 0, 0, 0],
            "outside clip must be untouched"
        );
    }

    /// A texture is stretched over the box by the sampler's rules: bilinear between texels,
    /// premultiplied so a transparent neighbour contributes no colour, and masked by the
    /// box's corners. A nearest or straight-alpha CPU path drifts from the GPU picture.
    #[test]
    fn a_texture_is_sampled_the_way_the_sampler_does() {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 20.0, 20.0));
        // Left texel is transparent *red*, right is opaque blue: straight-alpha
        // interpolation would tint the mix red.
        let texture = list.push_image(ui_core::draw::ImageEntry {
            id: 7,
            width: 2,
            height: 1,
            source: ui_core::draw::ImageSource::Pixels(std::sync::Arc::new(vec![
                255, 0, 0, 0, 0, 0, 255, 255,
            ])),
            colour: ui_core::colour::ContentColour::SDR,
            sampling: ui_core::draw::Sampling::Smooth,
        });
        list.push_image_prim(
            nid(0),
            ui_core::draw::ImagePrim {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                corners: Corners::all(8.0),
                texture,
                src: Rect::new(0.0, 0.0, 2.0, 1.0),
            },
        );

        let mut renderer = CpuRenderer::new();
        let mut target = Pixels::new(20, 20);
        renderer.render(&list, &mut target, &Damage::Full);

        // Each texel takes half the box, so the seam is at device x 10: one pixel past it the
        // alpha must be about half. A sampling offset of half a texel moves that seam.
        let mixed = target.get(10, 10);
        assert_eq!(
            (mixed[0], mixed[2]),
            (0, 255),
            "the mix must be the opaque texel's colour, got {mixed:?}"
        );
        assert!(
            (100..200).contains(&mixed[3]),
            "just past the seam, the alpha must be about half, got {mixed:?}"
        );
        // A quarter of the way in the transparent texel owns the pixel, bar the seam's feather.
        let near_edge = target.get(5, 10);
        assert!(
            near_edge[3] < 40,
            "the transparent texel must keep its half of the box, got {near_edge:?}"
        );
        assert_eq!(target.get(17, 10), [0, 0, 255, 255], "the right texel wins");
        assert_eq!(
            target.get(1, 1),
            [0, 0, 0, 0],
            "a rounded corner must mask the texture"
        );
        assert_eq!(renderer.stats.images, 1, "the primitive must be drawn");
    }

    /// A shadow is the silhouette convolved with a gaussian: half the peak on the edge, gone
    /// about `blur` out, and with a ring the band outside the caster rather than under it.
    #[test]
    fn a_shadow_fades_from_its_edge_like_a_gaussian_cast() {
        let scene = |ring: f32| {
            let mut list = DrawList::new(Rect::new(0.0, 0.0, 80.0, 80.0));
            list.push_shadow(
                nid(0),
                ui_core::draw::ShadowPrim {
                    rect: Rect::new(20.0, 20.0, 40.0, 40.0),
                    corners: Corners::ZERO,
                    colour: Color::from_rgba8(0, 0, 0, 255),
                    blur: 4.0,
                    dy: 0.0,
                    ring,
                    bound: None,
                },
            );
            let mut target = Pixels::new(80, 80);
            CpuRenderer::new().render(&list, &mut target, &Damage::Full);
            target
        };
        let solid = scene(0.0);
        // The edge pixel is half a pixel outside a silhouette of peak alpha: about half.
        let edge = solid.get(19, 40)[3];
        assert!(
            (95..110).contains(&edge),
            "the falloff must be about half at the edge, got {edge}"
        );
        assert_eq!(solid.get(40, 40)[3], 255, "under the caster it is solid");
        // blur 4 is two sigma, so three sigma out it must be gone.
        let near = solid.get(17, 40)[3];
        assert!(
            (20..35).contains(&near),
            "one sigma out the falloff is its tail, got {near}"
        );
        assert_eq!(solid.get(13, 40)[3], 0, "and stop about blur past the edge");

        // A ring casts only the band around the silhouette, not the area beneath it.
        let ring = scene(6.0);
        assert_eq!(ring.get(40, 40)[3], 0, "a ring leaves the interior clear");
        assert!(
            (205..225).contains(&ring.get(19, 40)[3]),
            "and darkens the edge more than a solid cast, got {}",
            ring.get(19, 40)[3]
        );

        // `dy` moves the caster, so the cast is stronger below it than above it.
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 80.0, 80.0));
        list.push_shadow(
            nid(0),
            ui_core::draw::ShadowPrim {
                rect: Rect::new(20.0, 20.0, 40.0, 40.0),
                corners: Corners::ZERO,
                colour: Color::from_rgba8(0, 0, 0, 255),
                blur: 4.0,
                dy: 6.0,
                ring: 0.0,
                bound: None,
            },
        );
        let mut target = Pixels::new(80, 80);
        CpuRenderer::new().render(&list, &mut target, &Damage::Full);
        assert!(
            target.get(40, 62)[3] > target.get(40, 18)[3],
            "an offset shadow must fall below its caster"
        );
    }

    /// A shadow stops at the edge of the surface it belongs to, as CSS clips a box-shadow at
    /// the border box: an outer cast paints only beyond that edge, an inset one only within
    /// it, so neither can show through a translucent fill.
    #[test]
    fn a_shadow_stops_at_its_surfaces_edge() {
        let scene = |side: ui_core::draw::ShadowSide| {
            let surface = Rect::new(20.0, 20.0, 40.0, 40.0);
            let mut list = DrawList::new(Rect::new(0.0, 0.0, 80.0, 80.0));
            list.push_shadow(
                nid(0),
                ui_core::draw::ShadowPrim {
                    rect: surface,
                    corners: Corners::ZERO,
                    colour: Color::from_rgba8(0, 0, 0, 255),
                    blur: 4.0,
                    dy: 0.0,
                    ring: 0.0,
                    bound: Some(ui_core::draw::ShadowBound {
                        rect: surface,
                        side,
                    }),
                },
            );
            let mut target = Pixels::new(80, 80);
            CpuRenderer::new().render(&list, &mut target, &Damage::Full);
            target
        };
        let outer = scene(ui_core::draw::ShadowSide::Outer);
        assert_eq!(
            (outer.get(20, 40)[3], outer.get(40, 40)[3]),
            (0, 0),
            "an outer cast must not reach inside its surface"
        );
        assert!(
            (95..110).contains(&outer.get(19, 40)[3]),
            "and must still land half its peak just outside, got {}",
            outer.get(19, 40)[3]
        );

        let inset = scene(ui_core::draw::ShadowSide::Inset);
        assert_eq!(
            inset.get(19, 40)[3],
            0,
            "an inset cast must not spill outside its surface"
        );
        assert!(
            (45..75).contains(&inset.get(21, 40)[3]),
            "but must darken the inner edge, got {}",
            inset.get(21, 40)[3]
        );
        assert_eq!(
            inset.get(40, 40)[3],
            0,
            "and fade out before the surface's middle"
        );
    }

    /// The geometry is in device pixels: at 2x the same shadow covers twice the distance, and
    /// its edge sits on the caster's device edge rather than its logical one.
    #[test]
    fn a_shadow_covers_the_device_pixels_of_its_caster() {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 80.0, 80.0));
        list.push_shadow(
            nid(0),
            ui_core::draw::ShadowPrim {
                rect: Rect::new(20.0, 20.0, 40.0, 40.0),
                corners: Corners::ZERO,
                colour: Color::from_rgba8(0, 0, 0, 255),
                blur: 4.0,
                dy: 0.0,
                ring: 0.0,
                bound: None,
            },
        );
        let mut target = Pixels::new(160, 160);
        let mut renderer = CpuRenderer::new();
        renderer.set_scale(2.0);
        renderer.render(&list, &mut target, &Damage::Full);

        // Sigma is four device pixels here, so the edge pixel is a quarter sigma out.
        let edge = target.get(39, 80)[3];
        assert!(
            (108..122).contains(&edge),
            "the logical edge is device x 40, got {edge} at 39"
        );
        assert_eq!(target.get(100, 80)[3], 255, "the caster's middle is solid");
        assert_eq!(target.get(26, 80)[3], 0, "three sigma out in device pixels");
    }
}

mod blend_domain_tests {
    use crate::*;
    use ui_core::colour::TransferFunction;
    use ui_core::geom::Rect;

    /// Half-transparent white over opaque black, in one 1x1 pixel.
    fn white_half_over_black(decode_first: bool) -> u8 {
        let mut dst = Pixels::new(1, 1);
        dst.data[0..4].copy_from_slice(&[0, 0, 0, 255]);
        let mut src = Pixels::new(1, 1);
        // Straight contract: full-white channel at half alpha, not premultiplied halves.
        src.data[0..4].copy_from_slice(&[255, 255, 255, 128]);
        let region = Rect::new(0.0, 0.0, 1.0, 1.0);
        if decode_first {
            composite_linear(&mut dst, &src, 1.0, region, TransferFunction::Srgb);
        } else {
            composite_over_rounded(
                &mut dst,
                &src,
                1.0,
                region,
                region,
                ui_core::geom::Corners::all(0.0),
            );
        }
        dst.data[0]
    }

    #[test]
    fn decoding_before_blending_brightens_a_mid_tone_by_tens_of_levels() {
        // 50% white over black: encoded blend gives 128, decode-blend-encode gives ~187.
        // The gap is ~59 levels, so the earlier "within one LSB" note was simply wrong.
        let encoded = white_half_over_black(false);
        let decoded = white_half_over_black(true);
        assert!((encoded as i32 - 128).abs() <= 2, "encoded gave {encoded}");
        assert!((decoded as i32 - 187).abs() <= 4, "decoded gave {decoded}");
        assert!(
            decoded > encoded + 40,
            "gap {encoded} -> {decoded} is too small to be a real domain difference"
        );
    }
}

mod layer_tests {
    use crate::*;
    use ui_core::NodeId;
    use ui_core::draw::{DrawList, RectPrim};
    use ui_core::geom::{Color, Corners};

    fn opaque_white(x: f32, y: f32) -> RectPrim {
        RectPrim {
            rect: Rect::new(x, y, 20.0, 20.0),
            radius: Corners::ZERO,
            fill: Color::WHITE,
            gradient: None,
            border: None,
            border_gradient: None,
            notch: None,
        }
    }

    /// Two opaque squares that overlap, inside a layer at `opacity`.
    fn overlapping_in_layer(opacity: f32) -> DrawList {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        list.push_layer(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            vec![ui_core::Effect::opacity(opacity)],
            Corners::ZERO,
        );
        list.push_rect(NodeId::from_raw(1, 0), opaque_white(10.0, 10.0));
        list.push_rect(NodeId::from_raw(2, 0), opaque_white(20.0, 10.0));
        list.pop_layer();
        list
    }

    fn render(list: &DrawList) -> Pixels {
        let mut target = Pixels::new(100, 100);
        let mut renderer = CpuRenderer::new();
        renderer.render(list, &mut target, &Damage::Full);
        target
    }

    fn alpha_at(pixels: &Pixels, x: u32, y: u32) -> u8 {
        pixels.data[((y * pixels.width + x) * 4 + 3) as usize]
    }

    #[test]
    fn group_opacity_does_not_double_darken_overlaps() {
        // The defining property of a layer: the group composites once, so two
        // overlapping opaque children stay at exactly the layer's opacity.
        let pixels = render(&overlapping_in_layer(0.5));
        let single = alpha_at(&pixels, 15, 20);
        let overlap = alpha_at(&pixels, 25, 20);
        assert_eq!(
            single, overlap,
            "overlap must not be darker than a single child: {single} vs {overlap}"
        );
        assert!(
            (120..=136).contains(&single),
            "50% layer should land near 128, got {single}"
        );
    }

    #[test]
    fn nested_layers_multiply_their_opacities() {
        // Inner 0.5 inside outer 0.5 composites to 0.25.
        let mut list = DrawList::new(Rect::new(0.0, 0.0, 100.0, 100.0));
        list.push_layer(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            vec![ui_core::Effect::opacity(0.5)],
            Corners::ZERO,
        );
        list.push_layer(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            vec![ui_core::Effect::opacity(0.5)],
            Corners::ZERO,
        );
        list.push_rect(NodeId::from_raw(1, 0), opaque_white(10.0, 10.0));
        list.pop_layer();
        list.pop_layer();

        let pixels = render(&list);
        let a = alpha_at(&pixels, 15, 20);
        assert!(
            (56..=72).contains(&a),
            "nested 0.5 * 0.5 should land near 64, got {a}"
        );
    }
}

mod effect_tests {
    use crate::*;
    use ui_core::NodeId;
    use ui_core::effect::Effect;
    use ui_core::geom::{Color, Rect};

    const SIZE: u32 = 96;

    /// One white 24x24 square at (36,36) inside a layer, everything else transparent.
    fn scene(effects: Vec<Effect>) -> Pixels {
        scene_with_bounds(effects, Rect::new(0.0, 0.0, SIZE as f32, SIZE as f32))
    }

    fn scene_with_bounds(effects: Vec<Effect>, bounds: Rect) -> Pixels {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, SIZE as f32, SIZE as f32));
        list.push_layer(bounds, effects, Corners::ZERO);
        list.push_rect(
            NodeId::from_raw(1, 0),
            RectPrim {
                rect: Rect::new(36.0, 36.0, 24.0, 24.0),
                radius: Corners::ZERO,
                fill: Color::WHITE,
                border: None,
                border_gradient: None,
                gradient: None,
                notch: None,
            },
        );
        list.pop_layer();
        let mut target = Pixels::new(SIZE, SIZE);
        CpuRenderer::new().render(&list, &mut target, &Damage::Full);
        target
    }

    fn at(px: &Pixels, x: u32, y: u32) -> [u8; 4] {
        let o = ((y * px.width + x) * 4) as usize;
        [px.data[o], px.data[o + 1], px.data[o + 2], px.data[o + 3]]
    }

    /// The square inside a full-size layer, filled with `fill`, as a list to render twice.
    fn layered_scene(effects: Vec<Effect>, fill: Color) -> DrawList {
        let mut list = DrawList::new(Rect::new(0.0, 0.0, SIZE as f32, SIZE as f32));
        list.push_layer(
            Rect::new(0.0, 0.0, SIZE as f32, SIZE as f32),
            effects,
            Corners::ZERO,
        );
        list.push_rect(
            NodeId::from_raw(1, 0),
            RectPrim {
                rect: Rect::new(36.0, 36.0, 24.0, 24.0),
                radius: Corners::ZERO,
                fill,
                border: None,
                border_gradient: None,
                gradient: None,
                notch: None,
            },
        );
        list.pop_layer();
        list
    }

    /// A layer whose bytes and chain are unchanged reuses its effect result, and the reuse
    /// is bit-identical to a recompute: a cached pass that drifted would be invisible.
    #[test]
    fn an_effect_result_is_reused_while_its_source_is_unchanged() {
        let list = layered_scene(vec![Effect::blur(4, 1.5)], Color::WHITE);
        let mut renderer = CpuRenderer::new();
        let mut target = Pixels::new(SIZE, SIZE);
        renderer.render(&list, &mut target, &Damage::Full);
        assert_eq!(
            renderer.stats.effect_hits, 0,
            "the first frame has nothing to reuse"
        );
        assert_eq!(renderer.stats.effect_misses, 1);
        let first = target.clone();

        // Damage that misses the layer still repaints the surface, and the bytes the chain
        // reads are the same: the result is reused rather than recomputed.
        let elsewhere = Damage::Rects(vec![Rect::new(0.0, 0.0, 2.0, 2.0)]);
        renderer.render(&list, &mut target, &elsewhere);
        assert_eq!(
            renderer.stats.effect_hits, 1,
            "same chain, rect and source bytes"
        );
        assert_eq!(renderer.stats.effect_misses, 0);
        assert_eq!(
            target.data, first.data,
            "the reused result is the recomputed pixels"
        );
    }

    /// A changed layer runs its chain again, and the answer matches a cold render.
    #[test]
    fn a_changed_layer_reruns_its_effect() {
        let mut renderer = CpuRenderer::new();
        let mut target = Pixels::new(SIZE, SIZE);
        renderer.render(
            &layered_scene(vec![Effect::blur(4, 1.5)], Color::WHITE),
            &mut target,
            &Damage::Full,
        );
        let changed = layered_scene(
            vec![Effect::blur(4, 1.5)],
            Color::from_rgba8(200, 60, 60, 255),
        );
        renderer.render(&changed, &mut target, &Damage::Full);
        // The counters are the frame's own: the renderer resets them before each one.
        assert_eq!(renderer.stats.effect_hits, 0, "the source bytes changed");
        assert_eq!(renderer.stats.effect_misses, 1);

        let mut cold = Pixels::new(SIZE, SIZE);
        CpuRenderer::new().render(&changed, &mut cold, &Damage::Full);
        assert_eq!(
            target.data, cold.data,
            "the recomputed frame matches a cold one"
        );
    }

    /// A different chain over the same pixels is not the same result.
    #[test]
    fn a_changed_chain_reruns_the_effect() {
        let mut renderer = CpuRenderer::new();
        let mut target = Pixels::new(SIZE, SIZE);
        renderer.render(
            &layered_scene(vec![Effect::blur(4, 1.5)], Color::WHITE),
            &mut target,
            &Damage::Full,
        );
        renderer.render(
            &layered_scene(vec![Effect::blur(6, 1.5)], Color::WHITE),
            &mut target,
            &Damage::Full,
        );
        assert_eq!(
            renderer.stats.effect_hits, 0,
            "a wider kernel is another chain"
        );
        assert_eq!(renderer.stats.effect_misses, 1);
    }

    /// A chain with no kernel is one in-place pass: hashing its input would cost as much as
    /// running it, so neither path may touch the cache.
    #[test]
    fn a_colour_only_chain_is_not_cached() {
        let list = layered_scene(vec![Effect::opacity(0.5)], Color::WHITE);
        let mut renderer = CpuRenderer::new();
        let mut target = Pixels::new(SIZE, SIZE);
        renderer.render(&list, &mut target, &Damage::Full);
        renderer.render(&list, &mut target, &Damage::Full);
        assert_eq!(
            (renderer.stats.effect_hits, renderer.stats.effect_misses),
            (0, 0),
            "a pure colour chain runs every frame and is not recorded"
        );
    }

    /// The same square rendered on a grid `scale` times finer.
    fn scaled_scene(effects: Vec<Effect>, scale: f32) -> Pixels {
        let logical = SIZE as f32;
        let mut list = DrawList::new(Rect::new(0.0, 0.0, logical, logical));
        list.push_layer(
            Rect::new(0.0, 0.0, logical, logical),
            effects,
            Corners::ZERO,
        );
        list.push_rect(
            NodeId::from_raw(1, 0),
            RectPrim {
                rect: Rect::new(36.0, 36.0, 24.0, 24.0),
                radius: Corners::ZERO,
                fill: Color::WHITE,
                border: None,
                border_gradient: None,
                gradient: None,
                notch: None,
            },
        );
        list.pop_layer();
        let device = (logical * scale) as u32;
        let mut target = Pixels::new(device, device);
        let mut renderer = CpuRenderer::new();
        renderer.set_scale(scale);
        renderer.render(&list, &mut target, &Damage::Full);
        target
    }

    #[test]
    fn a_group_blur_widens_with_the_render_grid() {
        // A radius is authored in logical dp. On a 2x grid the square spans device 72..120,
        // so a 4 dp blur must reach 8 device pixels past it instead of 4.
        let twice = scaled_scene(vec![Effect::blur(4, 1.5)], 2.0);
        assert!(
            at(&twice, 126, 96)[3] > 0,
            "the blur must reach past the unscaled radius"
        );
        assert_eq!(
            at(&twice, 130, 96)[3],
            0,
            "and still stop at the scaled one"
        );
    }

    #[test]
    fn bounds_that_ignore_the_reach_clip_the_blur() {
        // The negative control: without the dilation the ink cannot leave the box.
        let dot = Rect::new(36.0, 36.0, 24.0, 24.0);
        let full = scene(vec![Effect::blur(4, 1.5)]);
        let clipped = scene_with_bounds(vec![Effect::blur(4, 1.5)], dot);
        assert!(at(&full, 62, 48)[3] > 0, "the dilated layer spreads here");
        assert_eq!(at(&clipped, 62, 48)[3], 0, "the tight layer cannot");
    }
}
