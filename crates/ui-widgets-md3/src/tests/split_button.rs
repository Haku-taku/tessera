//! The MD3 split button: the size table and the two halves' inner corners.

use super::*;

use crate::spec::{
    ButtonFlags, ButtonSize, ButtonStyle, ButtonToggle, SplitButtonFlags, SplitButtonSize,
    button_spec, split_button_spec,
};
use crate::split_button::{Md3SplitButton, SplitButtonParams, half_corners};
use ui_core::geom::Corners;
use ui_core::layout::AxisSize;

const SPLIT: Id = Id::str("split/test");

/// Build the control and return the tree it declares.
fn build(params: SplitButtonParams) -> Decl {
    let mut store = AnimStore::new();
    let mut core = ui_core::cx::Cx::new(&mut store, 0, 800, 600, 0.0, 0.0, 1.0, None);
    core.route(ui_core::cx::Routing::default());
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let control = Md3SplitButton::new_with(params, &mut cx, crate::pack::pack());
    cx.with_cx(|cx| control.decl(cx))
}

fn filled() -> crate::spec::ButtonSpec {
    button_spec(
        ButtonFlags::NONE,
        ButtonStyle::Filled,
        ButtonSize::Medium,
        ButtonToggle::Base,
    )
}

#[test]
fn the_halves_follow_the_size_table() {
    for (size, height, lead, trail, arrow, rest, wide) in [
        (
            SplitButtonSize::XSmall,
            32.0,
            (12.0, 10.0),
            13.0,
            22.0,
            4.0,
            8.0,
        ),
        (
            SplitButtonSize::Small,
            40.0,
            (16.0, 12.0),
            13.0,
            22.0,
            4.0,
            12.0,
        ),
        (
            SplitButtonSize::Medium,
            56.0,
            (24.0, 24.0),
            15.0,
            26.0,
            4.0,
            12.0,
        ),
        (
            SplitButtonSize::Large,
            96.0,
            (48.0, 48.0),
            29.0,
            38.0,
            8.0,
            20.0,
        ),
        (
            SplitButtonSize::XLarge,
            136.0,
            (64.0, 64.0),
            43.0,
            50.0,
            12.0,
            20.0,
        ),
    ] {
        let spec = split_button_spec(SplitButtonFlags::NONE, size);
        assert_eq!(spec.container.height, height, "{size:?}");
        assert_eq!(spec.container.shape, "circular", "{size:?}");
        assert_eq!(spec.root.between_space, 2.0, "{size:?}");
        assert_eq!(
            (
                spec.leading_button.leading_space,
                spec.leading_button.trailing_space
            ),
            lead,
            "{size:?}"
        );
        assert_eq!(spec.trailing_button.leading_space, trail, "{size:?}");
        assert_eq!(spec.trailing_button.trailing_space, trail, "{size:?}");
        assert_eq!(spec.trailing_button_icon.size, arrow, "{size:?}");
        assert_eq!(spec.inner_corner.corner_size, rest, "{size:?}");
        // Hover and press carry the same widened corner; both widen both halves.
        let hovered = split_button_spec(
            SplitButtonFlags {
                hovered: true,
                ..SplitButtonFlags::NONE
            },
            size,
        );
        let pressed = split_button_spec(
            SplitButtonFlags {
                pressed: true,
                ..SplitButtonFlags::NONE
            },
            size,
        );
        assert_eq!(hovered.inner_corner.corner_size, wide, "{size:?}");
        assert_eq!(pressed.inner_corner.corner_size, wide, "{size:?}");
    }
}

#[test]
fn the_open_menu_rounds_only_the_trailing_half() {
    // The selected corner is a percentage of the box, so it is full at every size and lives
    // on the trailing half's own field.
    for size in [
        SplitButtonSize::XSmall,
        SplitButtonSize::Small,
        SplitButtonSize::Medium,
        SplitButtonSize::Large,
        SplitButtonSize::XLarge,
    ] {
        let open = split_button_spec(
            SplitButtonFlags {
                selected: true,
                ..SplitButtonFlags::NONE
            },
            size,
        );
        assert_eq!(
            open.trailing_button_inner_corner.corner_size, 50.0,
            "{size:?}"
        );
        // The shared corner keeps its resting value: an open menu must not round the label half.
        let rest = split_button_spec(SplitButtonFlags::NONE, size)
            .inner_corner
            .corner_size;
        assert_eq!(open.inner_corner.corner_size, rest, "{size:?}");
    }

    let (leading, trailing) = half_corners(56.0, 4.0, false);
    assert_eq!(
        leading,
        Corners {
            tl: 28.0,
            tr: 4.0,
            br: 4.0,
            bl: 28.0,
            ..Corners::ZERO
        }
    );
    assert_eq!(
        trailing,
        Corners {
            tl: 4.0,
            tr: 28.0,
            br: 28.0,
            bl: 4.0,
            ..Corners::ZERO
        }
    );

    let (leading, trailing) = half_corners(56.0, 20.0, true);
    assert_eq!(
        leading,
        Corners {
            tl: 28.0,
            tr: 20.0,
            br: 20.0,
            bl: 28.0,
            ..Corners::ZERO
        }
    );
    // Only the trailing half's inner corners reach the full radius while the menu is open.
    assert_eq!(trailing, Corners::all(28.0));
}

#[test]
fn the_leading_icon_takes_its_box_from_the_button_table() {
    // The split export has no icon size or icon-label gap; both come from the filled
    // button's table, the same one that supplies the halves' colours and type.
    let decl = build(SplitButtonParams::new(SPLIT, "Share").leading(
        ui_widgets::IconSource::glyph("share", ui_widgets::IconSet::Outlined),
    ));
    let content = &decl.children[0].children[0];
    assert_eq!(content.data.params().expect("a row").gap, 8.0);
    let icon = &content.children[0];
    assert_eq!(
        icon.size_class.width,
        AxisSize::fixed(filled().icon.size),
        "the mark is the button table's icon size"
    );
    assert_eq!(icon.size_class.height, AxisSize::fixed(filled().icon.size));
    assert_eq!(content.children.len(), 2, "the mark and the label");
}

#[test]
fn a_split_button_without_a_leading_icon_holds_its_label_directly() {
    let decl = build(SplitButtonParams::new(SPLIT, "Share"));
    let half = &decl.children[0];
    assert_eq!(half.children.len(), 1);
    assert!(
        half.children[0].data.params().is_none(),
        "the label, not a row"
    );
}
