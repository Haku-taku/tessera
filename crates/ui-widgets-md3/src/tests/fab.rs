//! The MD3 FAB: the size boxes, the shape scale's corners and the colour styles.

use crate::fab::fab_corner;
use crate::spec::{FabColor, FabFlags, FabLowering, FabSize, fab_spec};

#[test]
fn the_box_follows_its_size_and_the_shape_scale() {
    for (size, box_side, icon, corner) in [
        // Literal corners: the shape scale's medium, large, large-increased, extra-large.
        (FabSize::Small, 40.0, 24.0, 12.0),
        (FabSize::Base, 56.0, 24.0, 16.0),
        (FabSize::Medium, 80.0, 28.0, 20.0),
        (FabSize::Large, 96.0, 36.0, 28.0),
    ] {
        let spec = fab_spec(
            FabFlags::NONE,
            FabColor::PrimaryContainer,
            size,
            FabLowering::Base,
        );
        assert_eq!(spec.container.width, box_side, "{size:?}");
        assert_eq!(spec.container.height, box_side, "{size:?}");
        assert_eq!(spec.icon.size, icon, "{size:?}");
        assert_eq!(fab_corner(size), corner, "{size:?}");
    }
}

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
