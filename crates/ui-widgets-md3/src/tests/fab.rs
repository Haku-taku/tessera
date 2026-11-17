//! The MD3 FAB: the size boxes, the shape scale's corners and the colour styles.

use crate::spec::{FabColor, FabFlags, FabLowering, FabSize, fab_spec};

#[test]
fn a_surface_fab_keeps_its_size_and_the_lowered_form_drops_its_shadow() {
    // The `surface` family re-declares the base 56 dp box, so a table that applied colour
    // after size would shrink a large surface FAB back to 56.
    let large = fab_spec(
        FabFlags::NONE,
        FabColor::Surface,
        FabSize::Large,
        FabLowering::Base,
    );
    assert_eq!((large.container.width, large.icon.size), (96.0, 36.0));
    assert_eq!(large.container.color, "surfaceContainerHigh");
    assert_eq!(large.icon.color, "primary");
    // The ring is exported only under `surface`; the widget falls back to it elsewhere.
    assert_eq!(large.focus_indicator.color, "secondary");
    assert_eq!(large.focus_indicator.thickness, 3.0);

    let lowered = fab_spec(
        FabFlags::NONE,
        FabColor::Surface,
        FabSize::Base,
        FabLowering::Lowered,
    );
    assert_eq!(lowered.container.color, "surfaceContainerLow");
    assert_eq!(lowered.container.elevation, 1.0);
    let hovered = fab_spec(
        FabFlags {
            hovered: true,
            ..FabFlags::NONE
        },
        FabColor::Surface,
        FabSize::Base,
        FabLowering::Lowered,
    );
    assert_eq!(hovered.container.elevation, 3.0);

    // A solid style paints its own role and keeps an on-colour mark at every size.
    let primary = fab_spec(
        FabFlags::NONE,
        FabColor::Primary,
        FabSize::Large,
        FabLowering::Base,
    );
    assert_eq!(primary.container.color, "primary");
    assert_eq!(primary.icon.color, "onPrimary");
    assert_eq!(primary.container.elevation, 6.0);
}
