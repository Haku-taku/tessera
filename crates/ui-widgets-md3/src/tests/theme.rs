//! The theme transition blends real packs between appearances.

use super::*;
use ui_design::Appearance;

fn avg(c: Color) -> f32 {
    (c.r + c.g + c.b) / 3.0
}

#[test]
fn a_halfway_theme_blend_sits_between_light_and_dark() {
    let light = crate::pack::pack_of(Appearance::Light);
    let dark = crate::pack::pack_of(Appearance::Dark);
    let mid = ui_style::Style::resolve(
        ui_style::StyleScope {
            appearance: Appearance::Dark,
            theme_from: Some(Appearance::Light),
            theme_t: 0.5,
        },
        crate::pack::pack_owned,
    );
    let l = light.ink("primary").expect("light primary");
    let d = dark.ink("primary").expect("dark primary");
    let m = mid.ink("primary").expect("mid primary");
    assert_ne!(m, l, "mid must not equal the source");
    assert_ne!(m, d, "mid must not equal the target");
    let (al, ad, am) = (avg(l), avg(d), avg(m));
    assert!(
        am > al.min(ad) && am < al.max(ad),
        "mid {am} not between {al} and {ad}"
    );
}

#[test]
fn the_theme_transition_endpoints_match_the_plain_packs() {
    let dark = crate::pack::pack_of(Appearance::Dark);
    let at = |from: Option<Appearance>, t: f32| {
        ui_style::Style::resolve(
            ui_style::StyleScope {
                appearance: Appearance::Dark,
                theme_from: from,
                theme_t: t,
            },
            crate::pack::pack_owned,
        )
    };
    assert_eq!(
        at(Some(Appearance::Light), 0.0).ink("primary"),
        crate::pack::pack_of(Appearance::Light).ink("primary")
    );
    assert_eq!(
        at(Some(Appearance::Light), 1.0).ink("primary"),
        dark.ink("primary")
    );
    // With no transition, the target is returned untouched even at t<1.
    assert_eq!(at(None, 0.3).ink("primary"), dark.ink("primary"));
}

use ui_widgets::StyleKit;

#[test]
fn an_md3_toggle_reads_the_blended_pack() {
    let mut store = AnimStore::new();
    let mut core = ui_core::cx::Cx::new(&mut store, 0, 400, 200, 0.0, 0.0, 1.0, None);
    core.route(ui_core::cx::Routing::default());
    let env = ui_style::Env::with_scope(
        None,
        ui_style::StyleScope {
            appearance: Appearance::Dark,
            theme_from: Some(Appearance::Light),
            theme_t: 0.5,
        },
        Md3Options::default(),
    );
    let mut cx = ui_widgets::Cx::new(&mut core, env);
    let spec = ui_style::ToggleSpec::new(Id::str("t"), false, "");
    let control = Md3Kit::new().toggle(&spec, &mut cx);
    let decl = cx.with_cx(|c| control.decl(c));
    let got = shape_fill(track_shape(&decl));
    let l = crate::pack::pack_of(Appearance::Light)
        .ink("surfaceContainerHighest")
        .unwrap();
    let d = crate::pack::pack_of(Appearance::Dark)
        .ink("surfaceContainerHighest")
        .unwrap();
    let mid = l.lerp(d, 0.5);
    assert!(
        (got.r - mid.r).abs() < 0.02
            && (got.g - mid.g).abs() < 0.02
            && (got.b - mid.b).abs() < 0.02,
        "md3 track should be the blended mid: {got:?} vs {mid:?}"
    );
}
