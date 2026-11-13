// Page modules and the page dispatcher.

use crate::app::Gallery;
use crate::shell::*;

pub(crate) mod action_sheet;
pub(crate) mod app_bar;
pub(crate) mod buttons;
pub(crate) mod cards;
pub(crate) mod carousel;
pub(crate) mod checkbox;
pub(crate) mod chips;
pub(crate) mod combo;
pub(crate) mod date_picker;
pub(crate) mod dialogs;
pub(crate) mod dialogs_fluent;
pub(crate) mod fab;
pub(crate) mod generic;
pub(crate) mod glyphs;
pub(crate) mod grid;
pub(crate) mod icon_button;
pub(crate) mod icons;
pub(crate) mod infobar;
pub(crate) mod inline;
pub(crate) mod layouts;
pub(crate) mod lists;
pub(crate) mod markdown;
pub(crate) mod materials;
pub(crate) mod math;
pub(crate) mod menu_flyout;
pub(crate) mod modal_sheet;
pub(crate) mod nav_bar;
pub(crate) mod nav_drawer;
pub(crate) mod nav_rail;
pub(crate) mod nav_view;
pub(crate) mod positioning;
pub(crate) mod progress;
pub(crate) mod radio;
pub(crate) mod segmented;
pub(crate) mod selection;
pub(crate) mod slider;
pub(crate) mod snackbar;
pub(crate) mod stepper;
pub(crate) mod switch;
pub(crate) mod system;
pub(crate) mod tabs;
pub(crate) mod text_field;
pub(crate) mod time_picker;
pub(crate) mod toast;
pub(crate) mod toolbar;
pub(crate) mod tooltip;

pub(crate) use action_sheet::*;
pub(crate) use app_bar::*;
pub(crate) use buttons::*;
pub(crate) use cards::*;
pub(crate) use carousel::*;
pub(crate) use checkbox::*;
pub(crate) use chips::*;
pub(crate) use combo::*;
pub(crate) use date_picker::*;
pub(crate) use dialogs::*;
pub(crate) use fab::*;
pub(crate) use generic::*;
pub(crate) use grid::*;
pub(crate) use icon_button::*;
pub(crate) use icons::*;
pub(crate) use infobar::*;
pub(crate) use inline::*;
pub(crate) use layouts::*;
pub(crate) use lists::*;
pub(crate) use markdown::*;
pub(crate) use materials::*;
pub(crate) use math::*;
pub(crate) use menu_flyout::*;
pub(crate) use modal_sheet::*;
pub(crate) use nav_bar::*;
pub(crate) use nav_drawer::*;
pub(crate) use nav_rail::*;
pub(crate) use nav_view::*;
pub(crate) use positioning::*;
pub(crate) use progress::*;
pub(crate) use radio::*;
pub(crate) use segmented::*;
pub(crate) use selection::*;
pub(crate) use slider::*;
pub(crate) use snackbar::*;
pub(crate) use stepper::*;
pub(crate) use switch::*;
pub(crate) use system::*;
pub(crate) use tabs::*;
pub(crate) use text_field::*;
pub(crate) use time_picker::*;
pub(crate) use toast::*;
pub(crate) use toolbar::*;
pub(crate) use tooltip::*;

pub(crate) fn page_body(page: Page, g: &Gallery, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    // One place unpacks the gallery state, so adding a page never widens a parameter list.
    let styles: [(&dyn StyleKit, &'static str, Msg); 3] = [
        (&g.md3, "md3", STYLES[0].1),
        (&g.fluent, "fluent", STYLES[1].1),
        (&g.glass, "glass", STYLES[2].1),
    ];
    match page {
        Page::Switch => switch_page(&styles, g.on, g.variant_on, cx),
        Page::Checkbox => checkbox_page(g.checked, g.fchecked, g.ftri, cx),
        Page::Radio => radio_page(g.radio_sel, g.fradio_sel, cx),
        Page::Slider => slider_page(&styles, g.value, g.step_value, g.range, cx),
        Page::Tabs => tabs_page(g.tab_sel, &g.fluent_tabs, g.fluent_tab_sel, cx),
        Page::ToolTip => tooltip_page(g, cx),
        Page::InfoBar => info_bar_page(g, cx),
        Page::Buttons => buttons_page(g.btn_sel, g.split_open, g.split_presses, g.glass_press, cx),
        Page::IconButton => icon_button_page(g.icon_button_on, cx),
        Page::FabMenu => fab_menu_page(g.fab_open, g.fab_picked, g.fab_presses, cx),
        Page::MenuFlyout => menu_flyout_page(g, cx),
        Page::Combo => combo_page(g, cx),
        Page::NavView => nav_view_page(g, cx),
        Page::Chips => chips_page(g.chip_sel, g.chip_glass, g.chip_glass_gone, cx),
        Page::Cards => cards_page(g.card_hits, cx),
        Page::Lists => lists_page(g.list_sel, g.glass_list_hits, cx),
        Page::Icons => icons_page(g.icon_filled, cx),
        Page::Dialogs => dialogs_page(
            g.dialog_picked,
            g.dialog_dismissed,
            g.dialog_open,
            g.dialog_target,
            cx,
        ),
        Page::ActionSheet => action_sheet_page(g.sheet_picked, g.sheet_dismissed, cx),
        Page::Toast => toast_page(g.toast_picked, g.toast_dismissed, cx),
        Page::ModalSheet => modal_sheet_page(
            g.modal_detent,
            g.modal_dismissed,
            g.md3_side_sheet_dismissed,
            cx,
        ),
        Page::Stepper => stepper_page(g.stepper_value, g.stepper_wrap, g.stepper_presses, cx),
        Page::DatePicker => date_picker_page(
            g.dp_value,
            g.dp_shown,
            g.dp_view,
            &g.dp_range,
            &DateInputState {
                text: &g.di_text,
                action: g.di_action,
            },
            cx,
        ),
        Page::TimePicker => time_picker_page(&g.tp_hour, &g.tp_minute, g.tp_period, cx),
        Page::AppBar => app_bar_page(g.app_bar_scrolled, g.app_bar_collapsed, g.app_bar_fired, cx),
        Page::Carousel => carousel_page(g.carousel_fired, cx),
        Page::TextField => text_field_page(
            &g.tf_value,
            &g.tf_fluent,
            &g.tf_glass,
            &g.tf_secure,
            g.shortcut,
            cx,
        ),
        Page::Search => search_page(
            &g.search_value,
            g.search_acted.as_deref(),
            g.search_open,
            &g.search_view_value,
            &g.glass_search,
            g.glass_search_acted.as_deref(),
            cx,
        ),
        Page::Selection => selection_page(&styles, cx),
        Page::Generic => generic_page(&g.list_heights, g.banner_open, g.note_open, cx),
        Page::Progress => progress_page(cx),
        Page::Segmented => segmented_page(g.seg_single, g.seg_multi, g.seg_glass, cx),
        Page::NavBar => nav_bar_page(g.nav_sel, cx),
        Page::NavRail => nav_rail_page(g.nav_sel, g.rail_expanded, cx),
        Page::NavDrawer => nav_drawer_page(g.drawer_sel, g.drawer_open, cx),
        Page::Toolbar => toolbar_page(g.toolbar_sel, cx),
        Page::Snackbar => snackbar_page(g.snack_hits, g.snack_overlay_visible, cx),
        Page::Layouts => layouts_page(&styles, cx),
        Page::Grid => grid_page(cx),
        Page::Inline => inline_page(cx),
        // The parsed trees and their frames live in the app: the control redraws every
        // frame, and neither a parse nor a projection belongs in one.
        Page::Markdown => markdown_page(
            g.markdown_code,
            &g.md3,
            g.markdown_sample
                .as_ref()
                .map(|(_, document)| document.clone()),
            g.markdown_sample_prepared.clone(),
            g.markdown_doc
                .as_ref()
                .map(|(_, document)| document.clone()),
            g.markdown_prepared.clone(),
            cx,
        ),
        Page::Math => math_page(&g.math_edit_src, cx),
        Page::Positioning => positioning_page(cx),
        Page::Materials => materials_page(&styles, &g.materials, cx),
        Page::Md3System => system_page(ui_design::SystemId::Md3, Page::Md3System, cx),
        Page::FluentSystem => system_page(ui_design::SystemId::Fluent, Page::FluentSystem, cx),
        Page::HigSystem => system_page(ui_design::SystemId::Hig, Page::HigSystem, cx),
    }
}
