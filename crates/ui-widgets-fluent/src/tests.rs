//! One test module for the pack: geometry, slider, toggle, scaffold and scope together.

use crate::*;

#[test]
fn a_quick_drag_to_the_far_end_commits_on_before_the_knob_has_glided_there() {
    // One drag frame, then a release: the knob is still on the off side because the
    // 83 ms follow has barely started, but the finger is not, and the finger decides.
    let pack = pack();
    let id = ui_core::Id::str("sw-quick");
    let mut anim = ui_core::AnimStore::new();
    let mut build = |state: ui_widgets::WidgetState, now: f64| {
        let mut node = ui_widgets::NodeCx::new(id, state, 1.0, now, &mut anim);
        FluentToggleSwitch::new(id, false, "On", &mut node, &pack)
    };
    let at = |x: f32| Some(ui_core::geom::Point { x, y: 10.0 });
    build(
        ui_widgets::WidgetState {
            pressed: true,
            local: at(2.0),
            ..Default::default()
        },
        0.0,
    );
    build(
        ui_widgets::WidgetState {
            pressed: true,
            dragging: true,
            drag_started: true,
            local: at(30.0),
            ..Default::default()
        },
        0.017,
    );
    let release = ui_widgets::WidgetState {
        dropped: true,
        local: at(30.0),
        ..Default::default()
    };
    let switch = build(release, 0.034);
    assert!(
        matches!(
            ui_widgets::Widget::change(&switch, &release, ui_core::geom::Rect::ZERO),
            Some(ui_core::Change::Toggled { on: Some(true) })
        ),
        "a flick to the far end after one frame must still commit on"
    );
}

#[test]
fn a_three_state_box_reaches_indeterminate_only_when_it_is_asked_to() {
    // `ToggleButton.OnToggle`: checked steps to indeterminate, and only then to off.
    let pack = pack();
    let click = ui_widgets::WidgetState {
        clicked: true,
        ..Default::default()
    };
    let next = |state: CheckState, three: bool| {
        let id = ui_core::Id::str("cb-tri");
        let mut anim = ui_core::AnimStore::new();
        let mut node = ui_widgets::NodeCx::new(id, click, 1.0, 0.0, &mut anim);
        let mut params = CheckBoxParams::new(id, "All", state);
        if three {
            params = params.three_state();
        }
        let boxed = FluentCheckBox::new(params, &mut node, &pack);
        match ui_widgets::Widget::change(&boxed, &click, ui_core::geom::Rect::ZERO) {
            Some(ui_core::Change::Toggled { on }) => on,
            other => panic!("unexpected change {other:?}"),
        }
    };
    assert_eq!(next(CheckState::Unchecked, true), Some(true));
    assert_eq!(next(CheckState::Checked, true), None);
    assert_eq!(next(CheckState::Indeterminate, true), Some(false));
    // A two-state box never reports the third value; a mixed one settles to checked.
    assert_eq!(next(CheckState::Checked, false), Some(false));
    assert_eq!(next(CheckState::Indeterminate, false), Some(true));
}

/// Build one button from a state, with its own animation id so states cannot mix.
fn button(
    name: &'static str,
    variant: ButtonVariant,
    state: ui_widgets::WidgetState,
    pack: &ui_style::Style,
) -> FluentButton {
    let id = ui_core::Id::str(name);
    let mut anim = ui_core::AnimStore::new();
    let mut node = ui_widgets::NodeCx::new(id, state, 1.0, 0.0, &mut anim);
    FluentButton::new(
        ButtonParams::new(id, "Save").variant(variant),
        &mut node,
        pack,
    )
}

#[test]
fn each_button_variant_reads_its_own_brush_family() {
    // The three styles name `Button*`, `AccentButton*` and `SubtleButton*`; reading one
    // family for all three paints a white accent button on a dark theme.
    let pack = pack();
    let rest = ui_widgets::WidgetState::default();
    let accent = pack.ink("AccentButtonBackground").unwrap();
    let plain = pack.ink("ButtonBackground").unwrap();
    let subtle = pack.ink("SubtleButtonBackground").unwrap();
    assert_ne!(accent, plain, "the theme must tell the two apart");
    assert_eq!(
        button("a", ButtonVariant::Accent, rest, &pack).fill(),
        accent
    );
    assert_eq!(
        button("b", ButtonVariant::Default, rest, &pack).fill(),
        plain
    );
    assert_eq!(
        button("c", ButtonVariant::Subtle, rest, &pack).fill(),
        subtle
    );
}

#[test]
fn a_disabled_button_wins_over_a_pressed_one_and_press_over_hover() {
    // `CommonStates` is one group: Disabled replaces Pressed, not blends with it.
    let pack = pack();
    let pressed = ui_widgets::WidgetState {
        pressed: true,
        hovered: true,
        ..Default::default()
    };
    let disabled = ui_widgets::WidgetState {
        disabled: true,
        ..pressed
    };
    assert_eq!(
        button("a", ButtonVariant::Default, pressed, &pack).fill(),
        pack.ink("ButtonBackgroundPressed").unwrap()
    );
    assert_eq!(
        button("b", ButtonVariant::Default, disabled, &pack).fill(),
        pack.ink("ButtonBackgroundDisabled").unwrap()
    );
}

#[test]
fn a_tooltip_fades_over_167_ms_and_paints_nothing_while_closed() {
    // `OpenStates` runs `FadeInThemeAnimation`/`FadeOutThemeAnimation` at the 167 ms
    // `ControlFastAnimationDuration`; a closed panel returns no content at all.
    let id = ui_core::Id::str("tip");
    let mut anim = ui_core::AnimStore::new();
    let alpha = |anim: &mut ui_core::AnimStore, now: f64, open: bool| {
        let params = ToolTipParams::new(
            id,
            "Save the file",
            ui_core::layout::Anchor::Node(ui_core::Id::str("save")),
        )
        .open(open);
        let mut node = ui_widgets::NodeCx::new(id, Default::default(), 1.0, now, anim);
        FluentToolTip::new(params, &mut node, &pack()).alpha()
    };
    assert_eq!(alpha(&mut anim, 0.0, false), 0.0, "mounted closed");
    assert_eq!(
        alpha(&mut anim, 0.1, true),
        0.0,
        "opening starts from nothing"
    );
    assert!(
        (alpha(&mut anim, 0.1835, true) - 0.5).abs() < 0.02,
        "halfway at 83.5 ms"
    );
    assert_eq!(alpha(&mut anim, 0.4, true), 1.0, "and settles after 167 ms");
    assert_eq!(
        alpha(&mut anim, 0.45, false),
        1.0,
        "closing starts from full"
    );
    assert!(
        (alpha(&mut anim, 0.5335, false) - 0.5).abs() < 0.02,
        "closing mirrors it"
    );
    assert_eq!(
        alpha(&mut anim, 0.7, false),
        0.0,
        "and is gone after 167 ms"
    );
}

/// Build one info bar from a severity, with its own id so no two share a slot.
fn infobar(name: &'static str, severity: InfoBarSeverity, open: bool) -> FluentInfoBar {
    let id = ui_core::Id::str(name);
    let mut anim = ui_core::AnimStore::new();
    let mut node = ui_widgets::NodeCx::new(id, Default::default(), 1.0, 0.0, &mut anim);
    FluentInfoBar::new(
        InfoBarParams::new(id, "Update available.", "Restart to apply.")
            .severity(severity)
            .open(open),
        &mut node,
        Default::default(),
        &pack(),
    )
}

#[test]
fn each_severity_reads_its_own_background_and_icon_brushes() {
    // WinUI's dictionary keys three brushes per severity; one shared family paints
    // all four bars the informational tint.
    let pack = pack();
    let cases = [
        (
            InfoBarSeverity::Informational,
            "InfoBarInformationalSeverityBackgroundBrush",
            "InfoBarInformationalSeverityIconBackground",
            "InfoBarInformationalSeverityIconForeground",
        ),
        (
            InfoBarSeverity::Success,
            "InfoBarSuccessSeverityBackgroundBrush",
            "InfoBarSuccessSeverityIconBackground",
            "InfoBarSuccessSeverityIconForeground",
        ),
        (
            InfoBarSeverity::Warning,
            "InfoBarWarningSeverityBackgroundBrush",
            "InfoBarWarningSeverityIconBackground",
            "InfoBarWarningSeverityIconForeground",
        ),
        (
            InfoBarSeverity::Error,
            "InfoBarErrorSeverityBackgroundBrush",
            "InfoBarErrorSeverityIconBackground",
            "InfoBarErrorSeverityIconForeground",
        ),
    ];
    for (severity, fill_key, disc_key, ink_key) in cases {
        let (fill, icon_fill, icon_ink) = infobar("bar", severity, true).severity_colours();
        assert_eq!(fill, pack.ink(fill_key).unwrap(), "{fill_key}");
        assert_eq!(icon_fill, pack.ink(disc_key).unwrap(), "{disc_key}");
        assert_eq!(icon_ink, pack.ink(ink_key).unwrap(), "{ink_key}");
    }
}

/// Lay a dialog out in an 800×600 window, with the measurer the engine ships.
fn dialog_tree(params: ContentDialogParams<'_>) -> ui_core::Tree {
    use ui_core::layout::LayoutEngine;
    use ui_widgets::Widget;
    let decl =
        ui_widgets::Cx::for_test_mut(|cx| FluentContentDialog::new(params, cx, &pack()).decl(cx));
    let window = ui_core::Decl::container(ui_core::Id::str("window"), ui_core::NodeKind::Column)
        .size(ui_core::layout::SizeClass::fixed(800.0, 600.0))
        .child(decl);
    let mut tree = ui_core::Tree::new();
    tree.apply(&window);
    let mut engine = ui_core::layout_engine::BuiltinLayout::new(
        ui_core::layout_engine::MonospaceMeasure::default(),
    );
    let root = tree.root().expect("root");
    engine.solve(
        &mut tree,
        root,
        ui_core::layout::Constraints::loose(ui_core::geom::Size::new(800.0, 600.0)),
    );
    tree
}

/// The box the node `id` landed on.
fn box_of(tree: &ui_core::Tree, id: ui_core::Id) -> ui_core::geom::Rect {
    tree.get(tree.find_by_id(id).expect("node"))
        .expect("node")
        .rect
}

#[test]
fn a_lone_action_still_sits_in_the_right_half() {
    // `PrimaryVisible` moves the button to the last `*` column and collapses the rest;
    // the empty primary column keeps its share, so the button is not centred.
    let id = ui_core::Id::str("dlg");
    let tree = dialog_tree(ContentDialogParams::new(id, "Title", "Body").close("OK"));
    let commands = box_of(&tree, id.with("commands"));
    let close = box_of(&tree, id.with("close"));
    let half = (commands.size.w - 2.0 * DIALOG_PAD - DIALOG_BUTTON_SPACING) * 0.5;
    assert_eq!(close.size.w, half);
    assert_eq!(
        close.origin.x,
        commands.origin.x + DIALOG_PAD + half + DIALOG_BUTTON_SPACING
    );
}

/// The fills a dialog resolves for its actions under one default-button choice.
fn dialog_fills(button: ContentDialogButton) -> Vec<(&'static str, ui_core::geom::Color)> {
    let pack = pack();
    let id = ui_core::Id::str("dlg");
    ui_widgets::Cx::for_test_mut(|cx| {
        FluentContentDialog::new(
            ContentDialogParams::new(id, "T", "B")
                .primary("Save")
                .secondary("Cancel")
                .default_button(button),
            cx,
            &pack,
        )
        .action_fills()
    })
}

#[test]
fn only_the_default_button_takes_the_accent_fill() {
    // `DefaultButtonStates` swaps in `AccentButtonStyle` for the chosen button alone,
    // and `ContentDialogButton.None` is the property's own default.
    let pack = pack();
    let accent = pack.ink("AccentButtonBackground").unwrap();
    let plain = pack.ink("ButtonBackground").unwrap();
    for (button, accented) in [
        (ContentDialogButton::None, None),
        (ContentDialogButton::Primary, Some("primary")),
        (ContentDialogButton::Secondary, Some("secondary")),
    ] {
        let fills = dialog_fills(button);
        assert_eq!(fills.len(), 2, "both actions are there");
        for (slot, fill) in fills {
            let want = if Some(slot) == accented {
                accent
            } else {
                plain
            };
            assert_eq!(fill, want, "{button:?} / {slot}");
        }
    }
}

/// What one dialog reports when the engine says this chord fired on it.
fn dialog_shortcut_reports(
    button: ContentDialogButton,
    chord: ui_core::ShortcutId,
) -> Option<ui_core::Id> {
    let pack = pack();
    let id = ui_core::Id::str("dlg");
    ui_widgets::Cx::for_test_mut(|cx| {
        let dialog = FluentContentDialog::new(
            ContentDialogParams::new(id, "T", "B")
                .primary("Save")
                .secondary("Later")
                .close("Cancel")
                .default_button(button),
            cx,
            &pack,
        );
        use ui_widgets::Widget;
        let state = ui_widgets::WidgetState {
            shortcut: Some(chord),
            ..Default::default()
        };
        match dialog.change(&state, ui_core::geom::Rect::ZERO) {
            Some(ui_core::Change::Selected(Some(picked))) => Some(picked),
            _ => None,
        }
    })
}

#[test]
fn enter_presses_the_default_button_and_escape_the_close_one() {
    // WinUI's `ContentDialog` binds Enter to `DefaultButton` and Escape to the close
    // button, wherever focus sits; with no default, Enter reports nothing.
    use ui_core::ShortcutId;
    let base = ui_core::Id::str("dlg");
    assert_eq!(
        dialog_shortcut_reports(ContentDialogButton::Primary, DIALOG_DEFAULT_SHORTCUT),
        Some(base.with("primary"))
    );
    assert_eq!(
        dialog_shortcut_reports(ContentDialogButton::Primary, DIALOG_CLOSE_SHORTCUT),
        Some(base.with("close"))
    );
    assert_eq!(
        dialog_shortcut_reports(ContentDialogButton::None, DIALOG_DEFAULT_SHORTCUT),
        None,
        "no default button, no Enter action"
    );
    assert_eq!(
        dialog_shortcut_reports(ContentDialogButton::Primary, ShortcutId("other")),
        None,
        "a chord it did not bind is not its business"
    );
}

#[test]
fn a_dialog_declares_its_way_in_and_out_instead_of_arriving_at_rest() {
    // A widget-side tween on a node that did not exist last frame has no start value,
    // so the dialog used to appear at full opacity; the entrance is declared instead,
    // with the fade and the scale on the two clocks `DialogShowingStates` gives them.
    let id = ui_core::Id::str("dlg");
    let tree = dialog_tree(ContentDialogParams::new(id, "Title", "Body").close("OK"));
    let root = tree
        .get(tree.find_by_id(id).expect("the dialog root"))
        .expect("the dialog root");
    let enter = root
        .data
        .params()
        .and_then(|p| p.enter)
        .expect("the smoke layer declares its way in");
    assert_eq!(enter.opacity, 0.0, "it must start invisible");
    assert_eq!(enter.spec, DIALOG_FADE, "the 83 ms linear fade");
    let exit = root
        .data
        .params()
        .and_then(|p| p.exit)
        .expect("and its way out, so the engine holds it");
    assert_eq!(exit.spec, DIALOG_FADE);
    let surface = tree
        .get(tree.find_by_id(id.with("surface")).expect("the surface"))
        .expect("the surface");
    let scale = surface
        .data
        .params()
        .and_then(|p| p.enter)
        .expect("the surface scales");
    assert_eq!(
        scale.scale, DIALOG_SCALE_FROM,
        "the surface starts at ScaleTransform 1.05, got {scale:?}"
    );
    assert_eq!(
        scale.spec, DIALOG_SCALE_IN,
        "its scale takes 250 ms, not the 83 the fade gets"
    );
    assert_eq!(scale.opacity, 1.0, "the surface does not fade on its own");
}

#[test]
fn the_arrow_walk_skips_disabled_rows_and_wraps_both_ways() {
    // `ComboBox` moves the selection with Up/Down and wraps at either end; a disabled
    // row is stepped over rather than selected.
    let items = [
        ComboBoxItem::new(ui_core::Id::str("a"), "A"),
        ComboBoxItem::new(ui_core::Id::str("b"), "B").disabled(),
        ComboBoxItem::new(ui_core::Id::str("c"), "C"),
    ];
    assert_eq!(
        neighbour(&items, None, true),
        Some(0),
        "nothing selected goes to the first"
    );
    assert_eq!(neighbour(&items, Some(0), true), Some(2), "b is disabled");
    assert_eq!(neighbour(&items, Some(2), true), Some(0), "forward wraps");
    assert_eq!(neighbour(&items, Some(0), false), Some(2), "backward wraps");
    let all_off = [ComboBoxItem::new(ui_core::Id::str("x"), "X").disabled()];
    assert_eq!(
        neighbour(&all_off, None, true),
        None,
        "nothing to step onto"
    );
}

/// A shell that opens a menu under its anchor, with the flags the test drives.
struct OpeningMenu {
    open: bool,
    /// Room above the anchor, which pushes the menu toward the bottom edge.
    pad: f32,
}

impl ui_platform::App for OpeningMenu {
    fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn ui_platform::Element> {
        let items = [
            MenuFlyoutEntry::Item(MenuFlyoutItem::new(ui_core::Id::str("one"), "One")),
            MenuFlyoutEntry::Item(MenuFlyoutItem::new(ui_core::Id::str("two"), "Two")),
        ];
        let params = MenuFlyoutParams::new(
            ui_core::Id::str("menu"),
            ui_core::layout::Anchor::Node(ui_core::Id::str("anchor")),
            &items,
        )
        .open(self.open);
        let menu = {
            let mut wx = ui_widgets::Cx::new(cx, ui_style::Env::root());
            let menu = FluentMenuFlyout::new(params, &mut wx, &pack());
            wx.element(Box::new(menu))
        };
        let root = ui_platform::Container::new(ui_core::Id::str("root"), ui_core::NodeKind::Column)
            .size(ui_core::layout::SizeClass::fixed(240.0, 240.0))
            .child(ui_platform::from_decl(
                ui_core::Decl::container(ui_core::Id::str("pad"), ui_core::NodeKind::Box)
                    .size(ui_core::layout::SizeClass::fixed(240.0, self.pad)),
            ))
            .child(ui_platform::from_decl(
                ui_core::Decl::container(ui_core::Id::str("anchor"), ui_core::NodeKind::Box)
                    .size(ui_core::layout::SizeClass::fixed(120.0, 32.0)),
            ))
            .child(menu);
        Box::new(root)
    }
}

/// A node's drawn rect and the rect it is clipped to.
fn node_rects(
    app: &ui_platform::AppLoop<OpeningMenu>,
    id: ui_core::Id,
) -> (ui_core::geom::Rect, ui_core::geom::Rect) {
    let tree = app.tree();
    let node = tree.get(tree.find_by_id(id).expect("node")).expect("data");
    (node.visual_rect(), node.mask)
}

#[test]
fn an_opening_menu_travels_inside_a_window_that_never_moves() {
    // `MenuPopupThemeTransition` moves the whole surface half its height toward the
    // trigger while the popup window stays put and clips it.
    let mut target = ui_platform::present::OffscreenTarget::new(240, 240);
    let mut app = ui_platform::AppLoop::new(OpeningMenu {
        open: false,
        pad: 0.0,
    });
    app.frame_with_dt(&mut target, None, 0.0);
    app.app_mut().open = true;
    app.frame_with_dt(&mut target, None, 0.0);
    let window = ui_core::Id::str("menu");
    let panel_id = window.with("panel");
    let (panel, mask) = node_rects(&app, panel_id);
    let (home, _) = node_rects(&app, window);
    assert!(
        (home.origin.y - panel.origin.y - home.size.h * POPUP_TRAVEL / 100.0).abs() < 0.5,
        "the surface starts half its height toward the trigger: {:?} in {:?}",
        panel.origin,
        home.origin
    );
    assert_eq!(
        mask, home,
        "the window, not the travelling panel, is the clip"
    );
    app.frame_with_dt(&mut target, None, 0.30);
    let (settled, mask) = node_rects(&app, panel_id);
    assert_eq!(
        settled.origin.y, home.origin.y,
        "it comes to rest on its window"
    );
    assert_eq!(mask, home, "and the clip never followed it");
}

#[test]
fn a_menu_flipped_above_its_anchor_unfolds_from_the_bottom() {
    // Near the bottom edge the menu sits above the trigger, so the travel points down:
    // the surface's bottom shows first and the rest unfolds upward out of it.
    let mut target = ui_platform::present::OffscreenTarget::new(240, 240);
    let mut app = ui_platform::AppLoop::new(OpeningMenu {
        open: false,
        pad: 200.0,
    });
    app.frame_with_dt(&mut target, None, 0.0);
    app.app_mut().open = true;
    app.frame_with_dt(&mut target, None, 0.0);
    // The flip is only known once the window has been laid out, and the surface is still
    // fully transparent on that first frame, so the direction is read from the next one.
    app.frame_with_dt(&mut target, None, 0.0);
    let window = ui_core::Id::str("menu");
    let panel_id = window.with("panel");
    let (panel, mask) = node_rects(&app, panel_id);
    let (home, _) = node_rects(&app, window);
    let (anchor, _) = node_rects(&app, ui_core::Id::str("anchor"));
    assert!(
        home.origin.y + home.size.h <= anchor.origin.y,
        "the menu is placed above its trigger: {:?} over {:?}",
        home,
        anchor
    );
    assert!(
        (panel.origin.y - home.origin.y - home.size.h * POPUP_TRAVEL / 100.0).abs() < 0.5,
        "the surface starts half its height below its window: {:?} in {:?}",
        panel.origin,
        home.origin
    );
    assert_eq!(
        mask, home,
        "the window, not the travelling panel, is the clip"
    );
    app.frame_with_dt(&mut target, None, 0.30);
    let (settled, _) = node_rects(&app, panel_id);
    assert_eq!(
        settled.origin.y, home.origin.y,
        "it comes to rest on its window"
    );
}

#[test]
fn a_closing_menu_stays_on_stage_until_its_fade_is_spent() {
    // The fade-out is a state of the widget, not of the app: the app has already said
    // "closed", and the surface must still be there for the frames that spend it.
    let mut target = ui_platform::present::OffscreenTarget::new(240, 240);
    let mut app = ui_platform::AppLoop::new(OpeningMenu {
        open: true,
        pad: 0.0,
    });
    app.frame_with_dt(&mut target, None, 0.0);
    app.frame_with_dt(&mut target, None, 0.30);
    // The window stays as a 0x0 placeholder once closed; the surface is the panel.
    let menu = ui_core::Id::str("menu").with("panel");
    assert!(app.tree().find_by_id(menu).is_some(), "open and settled");
    app.app_mut().open = false;
    let out = app.frame_with_dt(&mut target, None, 0.05);
    assert!(out.animating, "the fade keeps the loop awake");
    assert!(app.tree().find_by_id(menu).is_some(), "still on stage");
    app.frame_with_dt(&mut target, None, 0.30);
    assert!(
        app.tree().find_by_id(menu).is_none(),
        "once the fade is spent the surface is gone"
    );
}

#[test]
fn a_fluent_formula_takes_the_packs_body_ink_and_size() {
    // A host with no math face draws the source text; the ink and size it draws with are the
    // pack's, so a brush name the pack does not know shows up here rather than on a screen.
    let pack = pack();
    let mut store = ui_core::AnimStore::new();
    let mut core = ui_core::cx::Cx::new(&mut store, 0, 400, 200, 0.0, 0.0, 1.0, None);
    let mut cx = ui_widgets::Cx::new(&mut core, ui_style::Env::root());
    let decl = cx.with_cx(|cx| FluentMath::new(ui_core::Id::str("m"), "x^2", pack).resolve_in(cx));
    let ui_core::NodeData::Text { ink, block, .. } = &decl.data else {
        panic!("a host with no math face draws the source text");
    };
    assert_eq!(
        *ink,
        pack.ink("TextFillColorPrimaryBrush")
            .expect("the pack knows its primary text brush")
    );
    assert_eq!(
        block.runs[0].style.size,
        pack.type_table.get(ui_style::TextRole::Body).size
    );
}
