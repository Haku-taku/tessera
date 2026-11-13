//! Scope resolution: the nearest declaration wins, and a foreign style is skipped.

use super::*;

#[test]
fn the_nearest_scope_wins() {
    let style = Md3Kit::new();
    let outer = md3_scope(2.0);
    let middle = ui_style::Env::with(
        Some(outer.clone()),
        Md3Options {
            outline_width: 2.0,
            ..Default::default()
        },
    );
    let inner = ui_style::Env::with(
        Some(middle.clone()),
        Md3Options {
            outline_width: 5.0,
            ..Default::default()
        },
    );
    assert_eq!(
        inner.find::<Md3Options>(),
        Some(&Md3Options {
            outline_width: 5.0,
            ..Default::default()
        })
    );
    let expected = toggle_decl(&style, &md3_scope(5.0), true);
    assert_eq!(toggle_decl(&style, &inner, true), expected);
    assert_ne!(toggle_decl(&style, &middle, true), expected);
}

#[test]
fn an_instance_default_applies_when_no_scope_exists() {
    let shoot = Md3Kit::with_scope(
        ui_style::StyleScope::default(),
        Md3Options {
            outline_width: 5.0,
            ..Default::default()
        },
    );
    let root = ui_style::Env::root();
    // The constant default is `true`, so this instance must not match it.
    assert_ne!(
        toggle_decl(&shoot, &root, true),
        toggle_decl(&Md3Kit::new(), &root, true)
    );
    let explicit = md3_scope(5.0);
    assert_eq!(
        toggle_decl(&shoot, &root, true),
        toggle_decl(&shoot, &explicit, true)
    );
}
