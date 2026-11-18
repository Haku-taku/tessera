//! The picker's tree: a toolbar over a places sidebar and a column of rows, and a footer.
//!
//! Every size here is decided from the dialog's own logical size, because a picker is one
//! window of a fixed purpose: the sidebar disappears under 720 units and the row window follows
//! the scroll container's own geometry. The styles come from the pack; this module names roles,
//! never colours.

use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use ui_core::change::Change;
use ui_core::dnd::{DragData, DropEffect, DropEffects};
use ui_core::geom::{Color, Corners, Insets, Point, Rect};
use ui_core::layout::{
    Anchor, AxisSize, Clip, Grow, Overflow, OverlayAlign, OverlayPlacement, Presentation, Side,
    SizeClass,
};
use ui_core::draw::Shape;
use ui_core::tree::{ImageFit, ModalTarget};
use ui_core::{
    AnimSpec, CursorShape, Decl, Id, Key, KeyChord, MenuItem, MenuRow, Modifiers, NodeKind, Sense,
    ShortcutId, TextAlign, Weight,
};
use ui_design::{CommonColor, CommonMetric};
use ui_i18n::{Args, Msg};
use ui_platform::chooser::{ChoiceKind, ChooserPlace};
use ui_platform::file_icons::{FileIconRequest, FileIconState};
use ui_platform::listing::Entry;
use ui_platform::places::{Device, Place};
use ui_platform::shell_menu::ShellMenuRow;
use ui_platform::thumbnails::ThumbnailState;
use ui_platform::uri_list;
use ui_platform::volumes::VolumesState;
use ui_platform::{Container, Element, FileKind, WidgetState, from_decl};
use ui_style::{MotionName, Style};
use ui_widgets::scroll::ScrollbarConf;
use ui_widgets::{Button, Cx, IconSet, IconSource, Pressable, RowWindow, Scroll, Text};

use crate::icons;
use crate::RenameError;
use crate::model::{Load, Mode, Subfolders};
use crate::PickerState;
use crate::{RowGesture, RowTrigger};

/// One row's height, and the height the row window measures against.
pub(crate) const ROW_HEIGHT: f32 = 32.0;

/// How long the pointer must rest on a cut name before its tooltip appears, in seconds.
const NAME_TOOLTIP_DELAY: f32 = 0.6;
/// The gap above a name's tooltip, and the width at which a very long name wraps in it.
const NAME_TOOLTIP_GAP: f32 = 4.0;
const NAME_TOOLTIP_WIDTH: f32 = 360.0;

const TOOLBAR_HEIGHT: f32 = 46.0;
const FOOTER_HEIGHT: f32 = 60.0;
/// The band the caller's extra controls sit in, above the footer.
const CHOICES_HEIGHT: f32 = 44.0;
const CHOICE_BUTTON_HEIGHT: f32 = 32.0;
/// The insertion caret's width, matching the engine's own editor.
const CARET_WIDTH: f32 = 2.0;
/// The editable line's inset from its field, and the room the caret keeps past the edge
/// before the line scrolls.
const FIELD_INSET: f32 = 8.0;
const FIELD_SCROLL_SLACK: f32 = 2.0;
const HEADER_HEIGHT: f32 = 28.0;
const PADDING: f32 = 12.0;
const GAP: f32 = 8.0;
/// A rule between two regions: one pixel, the pack's `Divider` role.
const RULE: f32 = 1.0;
/// The columns' own horizontal inset, on each side.
const COLUMN_INSET: f32 = 8.0;
const SIDEBAR_WIDTH: f32 = 180.0;
/// Below this window width the sidebar is not declared: a picker that keeps it would leave no
/// room for the names it exists to show.
const SIDEBAR_MIN_WIDTH: f32 = 720.0;
const MARK_SIZE: f32 = 20.0;
/// How far a hidden entry is dimmed. Dimming is the whole signal: a name that is excluded by
/// default still reads as an ordinary entry when the setting shows it, so the list says so.
const HIDDEN_OPACITY: f32 = 0.55;
/// How much of itself a cut row keeps drawing: the file is still there, and the paste that
/// will move it has not run yet.
const CUT_OPACITY: f32 = 0.45;

/// The stroke around the row the keyboard's cursor stands on, independently of the marks.
const FOCUS_RING: f32 = 1.5;
/// The grid's cell: a square picture over a two-line name, and the gap that separates them.
const GRID_IMAGE: f32 = 80.0;
const GRID_CELL_WIDTH: f32 = 116.0;
pub(crate) const GRID_CELL_HEIGHT: f32 = 136.0;
const GRID_GAP: f32 = 8.0;
/// One grid row's height, including the gap under it: what a vertical arrow steps by.
pub(crate) const GRID_ROW_PITCH: f32 = GRID_CELL_HEIGHT + GRID_GAP;
/// The name column's floor. A narrower panel scrolls sideways instead of squeezing a name to
/// nothing; the floor is the width at which a truncated name still reads.
const NAME_MIN_COLUMN: f32 = 60.0;
const SIZE_COLUMN: f32 = 84.0;
const MODIFIED_COLUMN: f32 = 150.0;
/// A crumb capsule's height: the trail is a compact field, not a row of buttons.
const CRUMB_HEIGHT: f32 = 24.0;
/// The strip's padding around the capsules, which keeps them clear of its rounded corners.
const CRUMB_INSET: f32 = 2.0;
/// The arrow beside a crumb: its hit strip, and the glyph inside it.
const CRUMB_ARROW: f32 = 20.0;
const ARROW_SIZE: f32 = 14.0;
/// The subfolder menu: its width, one row's height, and the tallest it may grow.
const MENU_WIDTH: f32 = 240.0;
const MENU_PADDING: f32 = 4.0;
const MENU_ROW_HEIGHT: f32 = 28.0;
const MENU_MAX_HEIGHT: f32 = 320.0;
/// The Replace question's panel: wider than a menu, because its sentence wraps.
const ASK_WIDTH: f32 = 340.0;
/// The band above the footer that names the files a job or a save touches: a card's height,
/// the gap between cards, the band's own inset, and how many cards it shows before the list
/// scrolls inside it.
const JOB_CARD_HEIGHT: f32 = 28.0;
const JOB_CARD_GAP: f32 = 4.0;
const JOB_BAND_PADDING: f32 = 8.0;
const JOB_VISIBLE_CARDS: usize = 4;
/// One card's pitch, which the band's row window steps by.
const JOB_CARD_PITCH: f32 = JOB_CARD_HEIGHT + JOB_CARD_GAP;
/// A card's own inset for its path.
const JOB_CARD_INSET: f32 = 10.0;

const ROOT: &str = "picker/root";
const TOOLBAR: &str = "picker/toolbar";
const SIDEBAR: &str = "picker/sidebar";
const SIDEBAR_SCROLL: &str = "picker/sidebar/scroll";
const SIDEBAR_LIST: &str = "picker/sidebar/list";
const SIDEBAR_DEVICES: &str = "picker/sidebar/devices";
const BODY: &str = "picker/body";
const PANEL: &str = "picker/panel";
const HEADER: &str = "picker/header";
const FOOTER: &str = "picker/footer";
const STATE: &str = "picker/state";
/// The vertical scroller's horizontal offset, which the header keeps instead.
const HEADER_SCROLL: &str = "picker/header/scroll";

pub(crate) fn up_id() -> Id {
    Id::str("picker/up")
}

pub(crate) fn hidden_id() -> Id {
    Id::str("picker/hidden")
}

/// The search field, and the toolbar button that opens or closes it.
pub(crate) fn search_id() -> Id {
    Id::str("picker/search")
}

pub(crate) fn search_button_id() -> Id {
    Id::str("picker/search/button")
}

/// The address field a typed path goes into.
pub(crate) fn path_entry_id() -> Id {
    Id::str("picker/path")
}

pub(crate) fn open_id() -> Id {
    Id::str("picker/open")
}

pub(crate) fn cancel_id() -> Id {
    Id::str("picker/cancel")
}

pub(crate) fn retry_id() -> Id {
    Id::str("picker/retry")
}

/// The field a single mode's answer is typed into: a save's name, or a path.
pub(crate) fn answer_id() -> Id {
    Id::str("picker/answer")
}

/// The sidebar's own scroller, which a drag near its edge moves.
pub(crate) fn sidebar_scroll_id() -> Id {
    Id::str(SIDEBAR_SCROLL)
}

/// The footer toggle that unfolds the caller's extra controls.
pub(crate) fn options_id() -> Id {
    Id::str("picker/options")
}

/// The toolbar toggle that switches between the list and the grid.
pub(crate) fn view_id() -> Id {
    Id::str("picker/view")
}

/// The grid's scroller, which is per directory so each folder keeps its own offset.
pub(crate) fn grid_id(path: &Path) -> Id {
    Id::of(("picker/grid", path))
}

/// One row of cells in the grid; the scroll window measures its height by this id.
fn grid_row_id(row: usize) -> Id {
    Id::of(("picker/grid/row", row))
}

/// One cell in the grid.
///
/// A cell never reuses the list's [`row_id`]: the list measures each row by reading the rect
/// its node had last frame, and the rect a cell left behind is a cell, not a row.
pub(crate) fn grid_cell_id(entry: &Entry) -> Id {
    Id::of(("picker/grid/cell", &entry.path))
}

fn options_band_id() -> Id {
    Id::str("picker/choices/band")
}

/// The band above the footer that lists the files a job or a save names, and one card in it.
fn job_band_id() -> Id {
    Id::str("picker/job/band")
}

fn job_card_id(index: usize) -> Id {
    Id::of(("picker/job/card", index))
}

/// The questions that list their own files — the permanent delete and the undo — and one
/// card in such a list.
fn asking_card_id(id: Id, index: usize) -> Id {
    id.with("card").with(index)
}

fn header_scroll_id() -> Id {
    Id::str(HEADER_SCROLL)
}

/// The type menu's button, its panel, and one filter in it.
pub(crate) fn filter_id() -> Id {
    Id::str("picker/filter")
}

pub(crate) fn filter_menu_id() -> Id {
    Id::str("picker/filter/menu")
}

pub(crate) fn filter_item_id(index: usize) -> Id {
    Id::of(("picker/filter/item", index))
}

/// One of the caller's extra places in the sidebar.
pub(crate) fn extra_place_id(place: &ChooserPlace) -> Id {
    Id::of(("picker/place/extra", &place.path))
}

/// A choice's button, its menu, and one option in it.
pub(crate) fn choice_button_id(index: usize) -> Id {
    Id::of(("picker/choice", index))
}

pub(crate) fn choice_menu_id(index: usize) -> Id {
    Id::of(("picker/choice/menu", index))
}

pub(crate) fn choice_item_id(index: usize, value: &str) -> Id {
    Id::of(("picker/choice/item", index, value))
}

/// The Replace question: its panel and its two answers.
pub(crate) fn overwrite_id() -> Id {
    Id::str("picker/overwrite")
}

pub(crate) fn overwrite_replace_id() -> Id {
    Id::str("picker/overwrite/replace")
}

pub(crate) fn overwrite_cancel_id() -> Id {
    Id::str("picker/overwrite/cancel")
}

/// The in-place rename field, wherever the selected row's name is drawn.
pub(crate) fn rename_id() -> Id {
    Id::str("picker/rename")
}

/// The footer's progress: its cancel button and the failure notice beside it.
pub(crate) fn operation_cancel_id() -> Id {
    Id::str("picker/operation/cancel")
}

pub(crate) fn failures_id() -> Id {
    Id::str("picker/failures")
}

pub(crate) fn failures_close_id() -> Id {
    Id::str("picker/failures/close")
}

/// The conflict panel: its answers, their Apply-to-all toggle, and its Rename field.
pub(crate) fn conflict_id() -> Id {
    Id::str("picker/conflict")
}

pub(crate) fn conflict_replace_id() -> Id {
    Id::str("picker/conflict/replace")
}

pub(crate) fn conflict_skip_id() -> Id {
    Id::str("picker/conflict/skip")
}

pub(crate) fn conflict_keep_both_id() -> Id {
    Id::str("picker/conflict/keep-both")
}

pub(crate) fn conflict_rename_id() -> Id {
    Id::str("picker/conflict/rename")
}

pub(crate) fn conflict_rename_ok_id() -> Id {
    Id::str("picker/conflict/rename-ok")
}

pub(crate) fn conflict_name_id() -> Id {
    Id::str("picker/conflict/name")
}

pub(crate) fn conflict_apply_all_id() -> Id {
    Id::str("picker/conflict/apply-all")
}

pub(crate) fn conflict_cancel_id() -> Id {
    Id::str("picker/conflict/cancel")
}

/// The permanent delete's confirmation.
pub(crate) fn delete_ask_id() -> Id {
    Id::str("picker/delete")
}

pub(crate) fn delete_confirm_id() -> Id {
    Id::str("picker/delete/confirm")
}

pub(crate) fn delete_cancel_id() -> Id {
    Id::str("picker/delete/cancel")
}

/// The question an Undo asks before it removes what a copy made.
pub(crate) fn undo_ask_id() -> Id {
    Id::str("picker/undo")
}

pub(crate) fn undo_confirm_id() -> Id {
    Id::str("picker/undo/confirm")
}

pub(crate) fn undo_cancel_id() -> Id {
    Id::str("picker/undo/cancel")
}

/// The panel that reports the selection's facts.
pub(crate) fn properties_id() -> Id {
    Id::str("picker/properties")
}

pub(crate) fn properties_close_id() -> Id {
    Id::str("picker/properties/close")
}

/// The folder chooser a Move to / Copy to command opens.
pub(crate) fn transfer_id() -> Id {
    Id::str("picker/transfer")
}

pub(crate) fn transfer_accept_id() -> Id {
    Id::str("picker/transfer/accept")
}

pub(crate) fn transfer_cancel_id() -> Id {
    Id::str("picker/transfer/cancel")
}

pub(crate) fn transfer_up_id() -> Id {
    Id::str("picker/transfer/up")
}

/// A shortcut row, a volume row and a subfolder row each carry their own namespace, because
/// a path may be both a place and a folder of the one being browsed.
pub(crate) fn transfer_place_id(path: &Path) -> Id {
    Id::of(("picker/transfer/place", path))
}

/// The caller's own shortcuts keep their own namespace, as the sidebar's do: a caller may
/// name a folder the platform already offers as a place.
pub(crate) fn transfer_extra_place_id(path: &Path) -> Id {
    Id::of(("picker/transfer/extra-place", path))
}

pub(crate) fn transfer_device_id(path: &Path) -> Id {
    Id::of(("picker/transfer/device", path))
}

pub(crate) fn transfer_folder_id(path: &Path) -> Id {
    Id::of(("picker/transfer/folder", path))
}

/// The right-drag decision menu: its panel, one row per effect, and its cancel row.
pub(crate) fn drop_menu_id() -> Id {
    Id::str("picker/drop/menu")
}

pub(crate) fn drop_menu_item_id(effect: DropEffect) -> Id {
    Id::of(("picker/drop/item", effect.bit()))
}

pub(crate) fn drop_menu_cancel_id() -> Id {
    Id::str("picker/drop/cancel")
}

/// The effect a decision-menu row stands for, if `id` is one of its rows.
pub(crate) fn drop_menu_effect(id: Id) -> Option<DropEffect> {
    [DropEffect::Move, DropEffect::Copy, DropEffect::Link]
        .into_iter()
        .find(|effect| drop_menu_item_id(*effect) == id)
}

pub(crate) fn place_id(place: &Place) -> Id {
    Id::of(("picker/place", &place.path))
}

pub(crate) fn device_id(device: &Device) -> Id {
    Id::of(("picker/device", &device.path))
}

/// One removable volume's row, keyed by the platform's volume name.
pub(crate) fn mountable_id(id: &str) -> Id {
    Id::of(("picker/mountable", id))
}

pub(crate) fn row_id(entry: &Entry) -> Id {
    Id::of(("picker/row", &entry.path))
}

pub(crate) fn list_id(path: &Path) -> Id {
    Id::of(("picker/list", path))
}

pub(crate) fn crumb_id(path: &Path) -> Id {
    Id::of(("picker/crumb", path))
}

/// The arrow half of a crumb's capsule, which opens the folder's subfolders.
pub(crate) fn crumb_arrow_id(path: &Path) -> Id {
    Id::of(("picker/crumb/arrow", path))
}

/// One subfolder of the open menu.
pub(crate) fn crumb_item_id(path: &Path) -> Id {
    Id::of(("picker/crumb/item", path))
}

/// The address field's empty space after the last crumb; a click there starts typing.
pub(crate) fn crumb_space_id() -> Id {
    Id::str("picker/crumbs/space")
}

/// The menu's window: the id the engine attributes a press outside it to.
pub(crate) fn crumb_menu_id() -> Id {
    Id::str("picker/crumb/menu")
}

/// Escape while the menu has the keyboard: the chord is the menu's, not the dialog's.
const CRUMB_MENU_DISMISS: ShortcutId = ShortcutId("picker.crumbs.dismiss");
/// Escape while a right-drag menu has the keyboard.
const DROP_MENU_DISMISS: ShortcutId = ShortcutId("picker.drop.dismiss");
/// Escape while a footer menu — the type menu or a choice — has the keyboard.
const FOOTER_MENU_DISMISS: ShortcutId = ShortcutId("picker.footer.dismiss");

/// The text one ancestor shows: its own name, or for a root the path prefix it stands for.
///
/// A root has no file name. Unix shows `/`; Windows shows the drive (`C:`) or the share, so
/// the first crumb of a `C:\...` path is the drive rather than a nameless slash.
pub(crate) fn crumb_label(segment: &Path) -> String {
    if let Some(name) = segment.file_name() {
        return name.to_string_lossy().into_owned();
    }
    match segment.components().next() {
        Some(Component::Prefix(prefix)) => prefix.as_os_str().to_string_lossy().into_owned(),
        _ => segment.as_os_str().to_string_lossy().into_owned(),
    }
}

pub(crate) fn build(
    state: &mut PickerState,
    cx: &mut Cx<'_, '_>,
    width: f32,
    height: f32,
) -> Box<dyn Element> {
    let pack = state.pack;
    let width = width.max(360.0);
    let height = height.max(240.0);
    // The default folder name is a message, and only a build holds the catalog.
    if std::mem::take(&mut state.new_folder) {
        let name = cx.t(Msg::PICKER_NEW_FOLDER_NAME).to_string();
        state.create_folder(&name);
    }
    // A right press with no declared menu: the app owns the rows, so it opens theirs here.
    // When the platform answers for the selection, the shell's read goes out first, and the
    // menu opens once its rows are in or the wait is over.
    if let Some((at, target)) = state.context_menu.take() {
        if !state.begin_menu_read(cx, at, target.clone()) {
            open_context_menu(state, cx, at, target);
        }
    }
    match state.poll_menu_read(cx) {
        Some(crate::MenuReadStep::Open { at, target }) => {
            open_context_menu(state, cx, at, target)
        }
        Some(crate::MenuReadStep::Refresh) => refresh_context_menu(state, cx),
        None => {}
    }
    state.poll_shell_invocations(cx);
    // Every frame's rows rebuild this: the row the pointer is on is the only one that sets it.
    state.name_tooltip = None;
    let surface = pack.opaque_or(
        CommonColor::SurfaceRaised,
        Color::from_rgba8(28, 28, 32, 255),
    );
    let divider = pack.colour_or(CommonColor::Divider, Color::from_rgba8(255, 255, 255, 30));
    // The caller's controls unfold upward from the footer; the band's height is the
    // animation's value, so the body gives back exactly what the band takes as it grows.
    let unfold = if state.choices.is_empty() {
        0.0
    } else {
        let spec = pack.motion.get(MotionName::Standard).spec;
        cx.at(options_band_id()).animate(
            options_band_id().with("open"),
            if state.options_open { 1.0 } else { 0.0 },
            spec,
        )
    };
    let band_height = CHOICES_HEIGHT * unfold;
    // The files a running job or a SaveFiles request names, listed above the footer. It is
    // there for as long as that set is, so it takes its own height out of the body.
    let (job_paths, job_current) = job_items(state);
    let job_height = job_band_height(job_paths.len());
    // Both rules take their own pixel out of the body, so the column's fixed children add up
    // to the dialog's height instead of overflowing it by one.
    let body_height =
        (height - TOOLBAR_HEIGHT - FOOTER_HEIGHT - band_height - job_height - 2.0 * RULE).max(0.0);
    let sidebar_width = if state.places.is_empty() || width < SIDEBAR_MIN_WIDTH {
        0.0
    } else {
        SIDEBAR_WIDTH
    };

    let rule = |id: &'static str, rule_width: f32, rule_height: f32| {
        Decl::container(Id::str(id), NodeKind::Box)
            .size(SizeClass::fixed(rule_width, rule_height))
            .background(divider, Corners::ZERO)
    };
    let menu = state.open_crumb_menu();
    let filter_open = state.filter_menu;
    let choice_open = state.choice_menu;
    let overwrite_open = state.overwrite_confirm.is_some();
    let mut children = vec![
        toolbar(state, cx, width, menu.as_deref()),
        from_decl(rule("picker/toolbar/rule", width, RULE)),
        body(state, cx, width, sidebar_width, body_height),
        from_decl(rule("picker/divider", width, RULE)),
    ];
    // Declared even at rest height zero so the band's scroller keeps its offset across a
    // close and the controls keep their ids; the clip is what hides them until it opens.
    if !state.choices.is_empty() {
        children.push(choices_band(state, cx, width, band_height));
    }
    // Between the caller's controls and the footer's own progress line, so the list sits
    // beside the sentence that sums it up.
    if job_height > 0.0 {
        children.push(job_band(
            state,
            cx,
            &job_paths,
            job_current.as_deref(),
            width,
            job_height,
        ));
    }
    children.push(footer(state, cx, width));
    // The name tooltip is declared after the body that asked for it, and before the menus
    // so a menu always draws over it.
    if let Some((anchor, name)) = state.name_tooltip.clone() {
        children.push(name_tooltip_panel(pack, anchor, name));
    }
    // The menu floats over everything, so it is the root's last child and an overlay: its
    // anchor is a crumb inside the toolbar's own scrolling strip.
    if let Some(folder) = &menu {
        children.push(crumb_menu(state, cx, folder));
    }
    if filter_open {
        children.push(filter_menu(state, cx));
    }
    if let Some(index) = choice_open {
        children.push(choice_menu(state, cx, index));
    }
    if let Some(menu) = &state.drop_menu {
        let at = menu.at;
        children.push(drop_menu(state, cx, at));
    }
    if overwrite_open {
        children.push(overwrite_panel(state, cx));
    }
    if state.conflict.is_some() {
        children.push(conflict_panel(state, cx));
    }
    if state.failures.is_some() {
        children.push(failures_panel(state, cx));
    }
    if state.delete_ask.is_some() {
        children.push(delete_panel(state, cx));
    }
    if state.undo_ask.is_some() {
        children.push(undo_panel(state, cx));
    }
    if state.properties.is_some() {
        children.push(properties_panel(state, cx));
    }
    if state.transfer.is_some() {
        children.push(transfer_panel(state, cx));
    }
    element(
        Decl::container(Id::str(ROOT), NodeKind::Column)
            .size(SizeClass::fixed(width, height))
            .background(surface, Corners::ZERO),
        children,
    )
}

/// A drawn node with live children, so a control can sit inside it.
fn element(decl: Decl, children: Vec<Box<dyn Element>>) -> Box<dyn Element> {
    Box::new(Container::from_decl(decl).children(children))
}

/// A region this module paints and the user clicks: one control, one activation.
///
/// A `Decl` alone has nowhere to report a click, so every clickable region is placed as a
/// [`Pressable`] element rather than as a declaration.
fn control(cx: &Cx<'_, '_>, decl: Decl) -> Box<dyn Element> {
    cx.element(Box::new(Pressable::new(decl)))
}

fn toolbar(
    state: &mut PickerState,
    cx: &mut Cx<'_, '_>,
    width: f32,
    menu: Option<&Path>,
) -> Box<dyn Element> {
    let pack = state.pack;
    let ink = pack.colour_or(CommonColor::OnSurfaceRaised, Color::WHITE);
    let crumb_width = (width - 2.0 * PADDING - 4.0 * (MARK_SIZE + 12.0) - 4.0 * GAP).max(80.0);
    let up = icon_button(state, cx, up_id(), "arrow_upward", false);
    // The glyph names the view the click reaches, not the one on screen: a grid button
    // beside a grid says nothing about what pressing it does.
    let view = icon_button(
        state,
        cx,
        view_id(),
        match state.layout {
            crate::Layout::Rows => "grid_view",
            crate::Layout::Grid => "view_list",
        },
        false,
    );
    let hidden = icon_button(
        state,
        cx,
        hidden_id(),
        if state.model.show_hidden() {
            "visibility"
        } else {
            "visibility_off"
        },
        state.model.show_hidden(),
    );
    let search = icon_button(
        state,
        cx,
        search_button_id(),
        if state.search.is_some() {
            "close"
        } else {
            "search"
        },
        state.search.is_some(),
    );
    // The address and search fields take the breadcrumb's slot: the trail names the folder
    // a search is under, and the field says what the user is typing instead.
    let address = if let Some(text) = state.path_entry.as_ref().map(|entry| entry.text.clone()) {
        toolbar_field(state, cx, path_entry_id(), &text, crumb_width, true)
    } else if let Some(text) = state.search.as_ref().map(|search| search.query.clone()) {
        toolbar_field(state, cx, search_id(), &text, crumb_width, true)
    } else {
        breadcrumb(state, cx, crumb_width, ink, menu)
    };
    element(
        Decl::container(Id::str(TOOLBAR), NodeKind::Row)
            .size(SizeClass::fixed(width, TOOLBAR_HEIGHT))
            .padding(Insets::new(PADDING, 0.0, PADDING, 0.0))
            .gap(GAP)
            .cross_align(0.0)
            .cross_pos(0.5),
        vec![up, address, search, view, hidden],
    )
}

/// The path as one capsule per ancestor, oldest first, scrolling sideways.
///
/// A capsule is the label's jump and, beside it, the arrow that opens the folder's subfolders:
/// Windows Explorer's trail. The two halves are two controls in one capsule, so clicking the
/// name never opens a menu and clicking the arrow never navigates. The whole trail sits in one
/// rounded field, which is the toolbar's only filled region.
fn breadcrumb(
    state: &mut PickerState,
    cx: &mut Cx<'_, '_>,
    width: f32,
    ink: Color,
    menu: Option<&Path>,
) -> Box<dyn Element> {
    let pack = state.pack;
    let path = state.model.path().to_path_buf();
    let segments: Vec<&Path> = path.ancestors().collect();
    let field = pack.colour_or(CommonColor::Surface, Color::from_rgba8(20, 20, 24, 255));
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    let field_height = CRUMB_HEIGHT + 2.0 * CRUMB_INSET;
    let scroll = Scroll::horizontal(Id::str("picker/crumbs"), width, field_height)
        .with_scrollbar(ScrollbarConf::never());
    let spec = pack.motion.get(MotionName::StateChange).spec;
    let hover = pack.colour_or(CommonColor::SurfaceHover, Color::from_rgba8(255, 255, 255, 18));
    // A capsule is a pill whose halves round their outer ends only, so the half the pointer
    // is on and the one beside it still read as one crumb.
    let corner = CRUMB_HEIGHT / 2.0;
    let left = Corners {
        tl: corner,
        bl: corner,
        ..Corners::ZERO
    };
    let right = Corners {
        tr: corner,
        br: corner,
        ..Corners::ZERO
    };
    let mut crumbs: Vec<Box<dyn Element>> = Vec::new();
    for (index, segment) in segments.iter().rev().enumerate() {
        let segment = *segment;
        let id = crumb_id(segment);
        let last = index + 1 == segments.len();
        // The last crumb shows an arrow only when its folder has somewhere to go; every crumb
        // before it is proved to have a child by the next one.
        let arrow = !last || state.crumb_has_children(segment);
        let fill = hover_fade(cx, id, hover, spec);
        let mut label_decl = Decl::container(id, NodeKind::Row)
            // Hug the segment's own text: a finite width hint would cap it and let the
            // neighbouring segment's text run into this one.
            .size(SizeClass {
                width: AxisSize::hug(0.0, f32::INFINITY),
                height: AxisSize::fixed(CRUMB_HEIGHT),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets::new(6.0, 0.0, 6.0, 0.0))
            .cursor(CursorShape::PointingHand)
            .sense(Sense::HOVER | Sense::CLICK)
            .cross_align(0.0)
            .cross_pos(0.5)
            .background(
                fill,
                if arrow { left } else { Corners::all(corner) },
            )
            .child(
                Text::new(id.with("label"), crumb_label(segment), caption_size(pack), ink)
                    .nowrap()
                    .resolve(),
            );
        // A crumb names a folder, so a drag can land in it and rest on it, as on a row;
        // the arrow beside it stays the menu's, so it takes no drops. The drop rule judges
        // the release, not the entry: a crumb the release would refuse still opens.
        label_decl = label_decl.accept_drop([uri_list::MIME]);
        let label = control(cx, label_decl);
        let mut halves = vec![label];
        if arrow {
            let arrow_id = crumb_arrow_id(segment);
            let fill = hover_fade(cx, arrow_id, hover, spec);
            halves.push(control(
                cx,
                Decl::container(arrow_id, NodeKind::Row)
                    .size(SizeClass::fixed(CRUMB_ARROW, CRUMB_HEIGHT))
                    .padding(Insets::new(3.0, 0.0, 3.0, 0.0))
                    .cursor(CursorShape::PointingHand)
                    .sense(Sense::HOVER | Sense::CLICK)
                    .cross_align(0.0)
                    .cross_pos(0.5)
                    .background(fill, right)
                    .child(
                        icons::crumb_arrow(menu == Some(segment)).resolve_with_size(
                            id.with("arrow"),
                            ink,
                            ARROW_SIZE,
                        ),
                    ),
            ));
        }
        let capsule = id.with("capsule");
        crumbs.push(element(
            Decl::container(capsule, NodeKind::Row)
                .size(SizeClass {
                    width: AxisSize::hug(0.0, f32::INFINITY),
                    height: AxisSize::fixed(CRUMB_HEIGHT),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
                .margin(Insets::new(1.0, 0.0, 1.0, 0.0))
                .sense(Sense::HOVER)
                .cross_align(0.0)
                .cross_pos(0.5)
                // Fainter than either half's: it says which crumb the pointer is in, and the
                // half under the pointer says which target.
                .background(
                    hover_fade(cx, capsule, hover.faded(0.4), spec),
                    Corners::all(corner),
                ),
            halves,
        ));
    }
    // The rest of the field is the address bar's empty space: a click there turns the trail
    // into the path field. It is a sibling of the caps, not their ancestor, so a click on a
    // crumb reaches only the crumb.
    let rest = control(
        cx,
        Decl::container(crumb_space_id(), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, f32::INFINITY),
                height: AxisSize::fixed(CRUMB_HEIGHT),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            })
            .cursor(CursorShape::Text)
            .sense(Sense::CLICK),
    );
    let trail = element(
        Decl::container(Id::str("picker/crumbs/trail"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::hug(0.0, f32::INFINITY),
                height: AxisSize::hug(0.0, f32::INFINITY),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .cross_align(0.0)
            .cross_pos(0.5),
        crumbs,
    );
    let contents = element(
        Decl::container(Id::str("picker/crumbs/content"), NodeKind::Row)
            .size(SizeClass {
                // At least the field is wide, so the empty space after the last crumb holds
                // the press target; a longer trail grows past it.
                width: AxisSize::hug(width, f32::INFINITY),
                height: AxisSize::hug(0.0, f32::INFINITY),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            // The inset is content padding inside the scroller, so the field's rounded
            // region owns the clip and the crumbs do not stop at a margin.
            .padding(Insets::all(CRUMB_INSET))
            .cross_align(0.0)
            .cross_pos(0.5),
        vec![trail, rest],
    );
    element(
        Decl::container(Id::str("picker/crumbs/field"), NodeKind::Row)
            .size(SizeClass::fixed(width, field_height))
            .cross_align(0.0)
            .cross_pos(0.5)
            .background(field, Corners::all(radius)),
        vec![Box::new(scroll.element_in(cx).children(vec![contents]))],
    )
}

/// A control's hover tint this frame, faded in by its own state change.
fn hover_fade(cx: &mut Cx<'_, '_>, id: Id, colour: Color, spec: AnimSpec) -> Color {
    let state = cx.state(id);
    let lift = cx.at(id).animate(
        id.with("hover"),
        if state.hovered || state.pressed { 1.0 } else { 0.0 },
        spec,
    );
    // The tint keeps its colour and fades in by alpha: lerping from transparent would drag
    // it towards black on the way.
    colour.faded(lift)
}

/// The menu under an open crumb arrow: that folder's subfolders, oldest first.
///
/// The panel is an anchored overlay rather than a child of the toolbar, so it escapes the
/// breadcrumb's own clip and floats over the rows it covers. It asks for a surface of its
/// own where the window has one — a dialog placed near the top of the output would otherwise
/// cut the list at its own edge — and the engine keeps it in the window when there is none.
fn crumb_menu(state: &mut PickerState, cx: &mut Cx<'_, '_>, folder: &Path) -> Box<dyn Element> {
    let pack = state.pack;
    let id = crumb_menu_id();
    let surface = pack.opaque_or(
        CommonColor::SurfaceRaised,
        Color::from_rgba8(28, 28, 32, 255),
    );
    let divider = pack.colour_or(CommonColor::Divider, Color::from_rgba8(255, 255, 255, 30));
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    let mut rows: Vec<Box<dyn Element>> = Vec::new();
    match state.model.subfolders(folder) {
        Subfolders::Loading => rows.push(menu_message(state, cx, Msg::PICKER_LOADING)),
        Subfolders::Failed => rows.push(menu_message(state, cx, Msg::PICKER_FAILED)),
        Subfolders::Ready(folders) if folders.is_empty() => {
            rows.push(menu_message(state, cx, Msg::PICKER_EMPTY));
        }        Subfolders::Ready(folders) => {
            for entry in folders {
                rows.push(menu_row(state, cx, &entry));
            }
        }
    }
    let scroll = Scroll::vertical(id.with("scroll"), MENU_MAX_HEIGHT)
        .with_scrollbar(ScrollbarConf::on(pack));
    let contents = element(
        Decl::container(id.with("content"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(MENU_WIDTH),
                height: AxisSize::hug(0.0, f32::INFINITY),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            // Padding inside the scroller: the bar sits over the panel's inset rather than
            // across the last letter of a folder name.
            .padding(Insets::all(MENU_PADDING)),
        rows,
    );
    let panel = element(
        Decl::container(id.with("panel"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(MENU_WIDTH),
                height: AxisSize::hug(0.0, f32::INFINITY),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .background(surface, Corners::all(radius))
            .border(divider, 1.0)
            // The panel covers the rows it hangs over; without the cast shadow it reads as
            // part of the list it is hiding.
            .elevation(2),
        vec![Box::new(scroll.element_in(cx).children(vec![contents]))],
    );
    let window = Decl::container(id, NodeKind::Box)
        .overlay(OverlayPlacement {
            anchor: Anchor::Node(crumb_arrow_id(folder)),
            side: Side::Below,
            align: OverlayAlign::Start,
            offset: 2.0,
            flip: true,
            match_anchor_width: false,
            match_anchor_height: false,
            min_width: 0.0,
            presentation: Presentation::Platform,
        })
        // Focus here is what makes the menu the keyboard scope while it is open: Escape
        // reaches its own chord, and a press outside arrives as the picker's dismissal.
        .sense(Sense::FOCUSABLE | Sense::HOVER)
        .shortcut(
            KeyChord::new(Key::Escape, Modifiers::NONE),
            CRUMB_MENU_DISMISS,
        );
    Box::new(Dismissible {
        inner: Container::from_decl(window).children(vec![panel]),
        chord: CRUMB_MENU_DISMISS,
    })
}

/// One subfolder in the menu, a jump with the same hover the rows have.
fn menu_row(state: &PickerState, cx: &mut Cx<'_, '_>, entry: &Entry) -> Box<dyn Element> {
    jump_row(
        state,
        cx,
        crumb_item_id(&entry.path),
        icons::file_mark(FileKind::Folder),
        &entry.name,
        MENU_WIDTH - 2.0 * MENU_PADDING,
        MENU_ROW_HEIGHT,
    )
}

/// One folder the pointer can jump to: a menu's subfolder, a chooser's shortcut or one of
/// its own rows. The mark names what the path is; the row itself only browses.
fn jump_row(
    state: &PickerState,
    cx: &mut Cx<'_, '_>,
    id: Id,
    mark: IconSource,
    label: &str,
    width: f32,
    height: f32,
) -> Box<dyn Element> {
    let pack = state.pack;
    let ink = pack.colour_or(CommonColor::OnSurfaceRaised, Color::WHITE);
    let hover = pack.colour_or(CommonColor::SurfaceHover, Color::from_rgba8(255, 255, 255, 18));
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    let spec = pack.motion.get(MotionName::StateChange).spec;
    let st = cx.state(id);
    let lift = cx.at(id).animate(
        id.with("hover"),
        if st.hovered || st.pressed { 1.0 } else { 0.0 },
        spec,
    );
    let fill = hover.faded(lift);
    let inset = 8.0;
    control(
        cx,
        Decl::container(id, NodeKind::Row)
            .size(SizeClass::fixed(width, height))
            .padding(Insets::new(inset, 0.0, inset, 0.0))
            .gap(GAP)
            .cursor(CursorShape::PointingHand)
            .sense(Sense::HOVER | Sense::CLICK)
            .cross_align(0.0)
            .cross_pos(0.5)
            .background(fill, Corners::all(radius))
            .child(mark.resolve_with_size(id.with("mark"), ink, MARK_SIZE))
            .child(
                Text::new(id.with("label"), label.to_string(), caption_size(pack), ink)
                    .max_lines(1)
                    .max_width(width - 2.0 * inset - MARK_SIZE - GAP)
                    .resolve(),
            ),
    )
}

/// One muted line a panel shows when it has no rows to offer: reading, empty or failed.
fn message_row(
    state: &PickerState,
    cx: &mut Cx<'_, '_>,
    id: Id,
    message: Msg,
    width: f32,
    height: f32,
) -> Box<dyn Element> {
    let pack = state.pack;
    let muted = pack.colour_or(CommonColor::TextMuted, Color::from_rgba8(170, 170, 180, 255));
    from_decl(
        Decl::container(id, NodeKind::Row)
            .size(SizeClass::fixed(width, height))
            .padding(Insets::new(8.0, 0.0, 8.0, 0.0))
            .cross_align(0.0)
            .cross_pos(0.5)
            .child(
                Text::new(
                    id.with("label"),
                    cx.t(message).to_string(),
                    caption_size(pack),
                    muted,
                )
                .max_lines(1)
                .max_width(width - 16.0)
                .resolve(),
            ),
    )
}

/// The menu's one-line answer when it has no folder to offer: reading, empty or failed.
fn menu_message(state: &PickerState, cx: &mut Cx<'_, '_>, message: Msg) -> Box<dyn Element> {
    message_row(
        state,
        cx,
        crumb_menu_id().with("message"),
        message,
        MENU_WIDTH - 2.0 * MENU_PADDING,
        MENU_ROW_HEIGHT,
    )
}

/// The decision a right drag asks for, drawn where the release landed.
///
/// A plain right drag cannot answer through modifiers — the button means "ask" — so the
/// effect is a choice among rows. The panel is anchored to the drop point rather than to a
/// node, because the row it landed on belongs to a list that may scroll underneath it.
fn drop_menu(
    state: &PickerState,
    cx: &mut Cx<'_, '_>,
    at: ui_core::geom::Point,
) -> Box<dyn Element> {
    let pack = state.pack;
    let id = drop_menu_id();
    let surface = pack.opaque_or(
        CommonColor::SurfaceRaised,
        Color::from_rgba8(28, 28, 32, 255),
    );
    let divider = pack.colour_or(CommonColor::Divider, Color::from_rgba8(255, 255, 255, 30));
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    let rows = vec![
        menu_label_row(
            state,
            cx,
            drop_menu_item_id(DropEffect::Move),
            Msg::PICKER_MOVE_HERE,
        ),
        menu_label_row(
            state,
            cx,
            drop_menu_item_id(DropEffect::Copy),
            Msg::PICKER_COPY_HERE,
        ),
        menu_label_row(
            state,
            cx,
            drop_menu_item_id(DropEffect::Link),
            Msg::PICKER_LINK_HERE,
        ),
        menu_label_row(state, cx, drop_menu_cancel_id(), Msg::PICKER_CANCEL),
    ];
    let panel = element(
        Decl::container(id.with("panel"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(MENU_WIDTH),
                height: AxisSize::hug(0.0, f32::INFINITY),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets::all(MENU_PADDING))
            .gap(2.0)
            .background(surface, Corners::all(radius))
            .border(divider, 1.0)
            .elevation(2),
        rows,
    );
    let window = Decl::container(id, NodeKind::Box)
        .overlay(OverlayPlacement {
            anchor: Anchor::Point(at),
            side: Side::Below,
            align: OverlayAlign::Start,
            offset: 2.0,
            flip: true,
            match_anchor_width: false,
            match_anchor_height: false,
            min_width: 0.0,
            presentation: Presentation::Platform,
        })
        .sense(Sense::FOCUSABLE | Sense::HOVER)
        .shortcut(
            KeyChord::new(Key::Escape, Modifiers::NONE),
            DROP_MENU_DISMISS,
        );
    Box::new(Dismissible {
        inner: Container::from_decl(window).children(vec![panel]),
        chord: DROP_MENU_DISMISS,
    })
}

/// A row of the picker's own context menu, worded and bound for the command it runs.
fn command_item(
    cx: &mut Cx<'_, '_>,
    command: crate::MenuCommand,
    msg: Msg,
    chord: Option<(KeyChord, &str)>,
) -> MenuItem {
    let mut item = MenuItem::new(command.id(), cx.t(msg).to_string());
    if let Some((chord, label)) = chord {
        item = item.with_chord(chord, label);
    }
    item
}

/// A quick action: a command row that the menu draws as an icon cell in its top strip.
fn quick_item(
    cx: &mut Cx<'_, '_>,
    command: crate::MenuCommand,
    msg: Msg,
    glyph: &str,
    chord: Option<(KeyChord, &str)>,
) -> MenuItem {
    command_item(cx, command, msg, chord).compact(icons::menu_mark(glyph))
}

/// The Undo row: worded by what it would put back, and disabled while the journal is empty.
fn undo_item(state: &PickerState, cx: &mut Cx<'_, '_>, ctrl: Modifiers) -> MenuItem {
    let cause = state.undo.last().map(ui_platform::UndoAction::cause);
    let msg = match cause {
        Some(ui_platform::UndoCause::Move) => Msg::PICKER_UNDO_MOVE,
        Some(ui_platform::UndoCause::Rename) => Msg::PICKER_UNDO_RENAME,
        Some(ui_platform::UndoCause::Copy) => Msg::PICKER_UNDO_COPY,
        None => Msg::PICKER_UNDO,
    };
    let item = MenuItem::new(crate::MenuCommand::Undo.id(), cx.t(msg).to_string())
        .with_chord(KeyChord::new(Key::Char('z'), ctrl), "Ctrl+Z");
    if cause.is_some() && state.file_operations.is_some() {
        item
    } else {
        item.disabled()
    }
}

/// Open the picker's own menu for a press that landed at `at`.
///
/// Show the row menu the press asked for, with whatever shell rows `state` already holds.
///
/// A read still running leaves the menu to open without them; the answer rebuilds the same
/// rows through [`refresh_context_menu`] instead of waiting for another press.
fn open_context_menu(
    state: &mut PickerState,
    cx: &mut Cx<'_, '_>,
    at: Point,
    target: crate::MenuTarget,
) {
    state.menu_point = at;
    let (owner, destination) = match &target {
        crate::MenuTarget::Row(index) => {
            let entry = state.model.rows().get(*index).cloned();
            // Paste and the link commands land where the row says when it is a folder, and
            // in the folder on screen otherwise, which is also what the row stands for.
            let destination = entry
                .as_ref()
                .filter(|entry| entry.directory)
                .map(|entry| entry.path.clone())
                .unwrap_or_else(|| state.model.path().to_path_buf());
            let owner = match &entry {
                Some(entry) if state.layout == crate::Layout::Grid => grid_cell_id(entry),
                Some(entry) => row_id(entry),
                None => list_id(state.model.path()),
            };
            (owner, destination)
        }
        // The background and a sidebar row stand for their folder, and their commands land
        // in it: Explorer's background menu writes to the folder it was pressed on.
        crate::MenuTarget::Folder => (
            list_id(state.model.path()),
            state.model.path().to_path_buf(),
        ),
        crate::MenuTarget::Place { owner, path } => (*owner, path.clone()),
    };
    state.menu_destination = destination;
    let items = context_menu_items(state, cx, &target);
    state.menu_open = Some(crate::OpenMenu { owner, target });
    cx.core().open_menu(owner, at, items);
}

/// Give the open menu rows that arrived after it went up.
///
/// The rows are built the same way as at the press, so a menu opened without the shell's
/// rows gains them where it stands rather than only on the next press.
fn refresh_context_menu(state: &mut PickerState, cx: &mut Cx<'_, '_>) {
    let Some((owner, target)) = state
        .menu_open
        .as_ref()
        .map(|open| (open.owner, open.target.clone()))
    else {
        return;
    };
    let items = context_menu_items(state, cx, &target);
    cx.core().update_menu(owner, items);
}

/// The menu a right press opens: the row's verbs, the folder's when the press missed every
/// row, or the folder a sidebar row stands for.
///
/// A row's menu is about the selection, which the press has already made its own; the
/// background's and a sidebar row's are about their folder, which is also where Paste lands.
fn context_menu_items(
    state: &mut PickerState,
    cx: &mut Cx<'_, '_>,
    target: &crate::MenuTarget,
) -> Vec<MenuItem> {
    let ctrl = Modifiers {
        ctrl: true,
        ..Modifiers::NONE
    };
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    let ctrl_shift = Modifiers {
        ctrl: true,
        shift: true,
        ..Modifiers::NONE
    };
    let alt = Modifiers {
        alt: true,
        ..Modifiers::NONE
    };
    let row = match target {
        crate::MenuTarget::Row(index) => Some(*index),
        _ => None,
    };
    let selection = state.model.selection_paths();
    let has = !selection.is_empty();
    // What the shell's rows stand for: the selection a row's menu was opened on, or the
    // folder itself for the background and a sidebar row, as Explorer's menus are.
    let shell_paths = state.shell_paths(target);
    let single = selection.len() == 1;
    let operations = state.file_operations.clone();
    let destination = state.menu_destination.clone();
    let mut items: Vec<MenuItem> = Vec::new();
    if let Some(index) = row {
        let directory = state
            .model
            .rows()
            .get(index)
            .is_some_and(|entry| entry.directory);
        items.push(
            command_item(
                cx,
                crate::MenuCommand::Open,
                Msg::PICKER_OPEN,
                Some((KeyChord::new(Key::Enter, Modifiers::NONE), "Enter")),
            )
            .default_row(),
        );
        items.push(MenuItem::separator());
        // The verbs the row menu is opened for stand in one strip of icon cells, as Win11's
        // menu does; their chords still pick them while the menu is open.
        let cut = quick_item(
            cx,
            crate::MenuCommand::Cut,
            Msg::PICKER_CUT,
            "content_cut",
            Some((KeyChord::new(Key::Char('x'), ctrl), "Ctrl+X")),
        );
        items.push(if has { cut } else { cut.disabled() });
        let copy = quick_item(
            cx,
            crate::MenuCommand::Copy,
            Msg::PICKER_COPY,
            "content_copy",
            Some((KeyChord::new(Key::Char('c'), ctrl), "Ctrl+C")),
        );
        items.push(if has { copy } else { copy.disabled() });
        if directory {
            let paste = quick_item(
                cx,
                crate::MenuCommand::Paste,
                Msg::PICKER_PASTE,
                "content_paste",
                Some((KeyChord::new(Key::Char('v'), ctrl), "Ctrl+V")),
            );
            items.push(if operations.is_some() {
                paste
            } else {
                paste.disabled()
            });
        }
        let rename = quick_item(
            cx,
            crate::MenuCommand::Rename,
            Msg::PICKER_RENAME,
            "drive_file_rename_outline",
            Some((KeyChord::new(Key::Function(2), Modifiers::NONE), "F2")),
        );
        items.push(if single { rename } else { rename.disabled() });
        let trash = quick_item(
            cx,
            crate::MenuCommand::Trash,
            Msg::PICKER_TRASH,
            "delete",
            Some((KeyChord::new(Key::Delete, Modifiers::NONE), "Delete")),
        );
        items.push(if has && operations.is_some() {
            trash
        } else {
            trash.disabled()
        });
        items.push(MenuItem::separator());
        // The two arrows a clipboard cannot express: the destination is chosen in a small
        // folder chooser rather than carried from another folder's paste.
        let move_to = command_item(cx, crate::MenuCommand::MoveTo, Msg::PICKER_MOVE_TO, None);
        items.push(if has && operations.is_some() {
            move_to
        } else {
            move_to.disabled()
        });
        let copy_to = command_item(cx, crate::MenuCommand::CopyTo, Msg::PICKER_COPY_TO, None);
        items.push(if has && operations.is_some() {
            copy_to
        } else {
            copy_to.disabled()
        });
        items.push(MenuItem::separator());
        let delete = command_item(
            cx,
            crate::MenuCommand::Delete,
            Msg::PICKER_DELETE,
            Some((KeyChord::new(Key::Delete, shift), "Shift+Delete")),
        );
        items.push(if has && operations.is_some() {
            delete
        } else {
            delete.disabled()
        });
        items.push(MenuItem::separator());
        // A hard link stays on its volume and names files, and the platform says whether it
        // can make one at all; each refusal is the row's own second line.
        let hard_reason = if operations
            .as_ref()
            .is_some_and(|operations| !operations.supports_links(ui_platform::file_operation::LinkKind::Hard))
        {
            Some(Msg::PICKER_LINK_UNSUPPORTED)
        } else if !state
            .model
            .selection_entries()
            .iter()
            .all(|entry| !entry.directory)
        {
            Some(Msg::PICKER_LINK_FILES_ONLY)
        } else if !operations.as_ref().is_some_and(|operations| {
            selection
                .iter()
                .all(|path| operations.same_volume(path.parent().unwrap_or(path), &destination))
        }) {
            Some(Msg::PICKER_LINK_ACROSS_VOLUMES)
        } else {
            None
        };
        let link = command_item(
            cx,
            crate::MenuCommand::CreateLink,
            Msg::PICKER_CREATE_LINK,
            None,
        );
        let link = if operations
            .as_ref()
            .is_some_and(|operations| !operations.supports_links(ui_platform::file_operation::LinkKind::Symbolic))
        {
            link.disabled_because(cx.t(Msg::PICKER_SYMLINK_UNSUPPORTED).to_string())
        } else if has && operations.is_some() {
            link
        } else {
            link.disabled()
        };
        items.push(link);
        let hard = command_item(
            cx,
            crate::MenuCommand::CreateHardLink,
            Msg::PICKER_CREATE_HARD_LINK,
            None,
        );
        items.push(match hard_reason {
            Some(msg) => hard.disabled_because(cx.t(msg).to_string()),
            None if has && operations.is_some() => hard,
            None => hard.disabled(),
        });
        let paths = command_item(
            cx,
            crate::MenuCommand::CopyPaths,
            Msg::PICKER_COPY_PATH,
            None,
        );
        items.push(if has { paths } else { paths.disabled() });
        let properties = command_item(
            cx,
            crate::MenuCommand::Properties,
            Msg::PICKER_PROPERTIES,
            Some((KeyChord::new(Key::Enter, alt), "Alt+Enter")),
        );
        items.push(if has {
            properties
        } else {
            properties.disabled()
        });
        // The platform's rows join this menu behind one row of ours when the host answered
        // faithfully; the native popup stays the door for a menu it could not carry.
        shell_menu_rows(state, cx, &shell_paths, &mut items);
        items.push(MenuItem::separator());
        items.push(command_item(
            cx,
            crate::MenuCommand::NewFolder,
            Msg::PICKER_NEW_FOLDER,
            Some((KeyChord::new(Key::Char('n'), ctrl_shift), "Ctrl+Shift+N")),
        ));
        items.push(MenuItem::separator());
        let select_all = command_item(
            cx,
            crate::MenuCommand::SelectAll,
            Msg::PICKER_SELECT_ALL,
            Some((KeyChord::new(Key::Char('a'), ctrl), "Ctrl+A")),
        );
        items.push(select_all);
        let invert = command_item(
            cx,
            crate::MenuCommand::Invert,
            Msg::PICKER_INVERT_SELECTION,
            None,
        );
        items.push(invert);
    } else if matches!(target, crate::MenuTarget::Place { .. }) {
        // A sidebar row offers what acts on its folder without the list's own verbs: the
        // folder is not on screen, so Select All or Refresh would speak for another one.
        items.push(command_item(
            cx,
            crate::MenuCommand::Open,
            Msg::PICKER_OPEN,
            Some((KeyChord::new(Key::Enter, Modifiers::NONE), "Enter")),
        ));
        items.push(MenuItem::separator());
        if shell_menu_rows(state, cx, &shell_paths, &mut items) {
            items.push(MenuItem::separator());
        }
        items.push(command_item(
            cx,
            crate::MenuCommand::Properties,
            Msg::PICKER_PROPERTIES,
            Some((KeyChord::new(Key::Enter, alt), "Alt+Enter")),
        ));
    } else {
        items.push(undo_item(state, cx, ctrl));
        items.push(MenuItem::separator());
        let paste = command_item(
            cx,
            crate::MenuCommand::Paste,
            Msg::PICKER_PASTE,
            Some((KeyChord::new(Key::Char('v'), ctrl), "Ctrl+V")),
        );
        items.push(if operations.is_some() {
            paste
        } else {
            paste.disabled()
        });
        items.push(MenuItem::separator());
        items.push(command_item(
            cx,
            crate::MenuCommand::NewFolder,
            Msg::PICKER_NEW_FOLDER,
            Some((KeyChord::new(Key::Char('n'), ctrl_shift), "Ctrl+Shift+N")),
        ));
        items.push(MenuItem::separator());
        let select_all = command_item(
            cx,
            crate::MenuCommand::SelectAll,
            Msg::PICKER_SELECT_ALL,
            Some((KeyChord::new(Key::Char('a'), ctrl), "Ctrl+A")),
        );
        items.push(select_all);
        let invert = command_item(
            cx,
            crate::MenuCommand::Invert,
            Msg::PICKER_INVERT_SELECTION,
            None,
        );
        items.push(invert);
        items.push(MenuItem::separator());
        items.push(
            command_item(
                cx,
                crate::MenuCommand::ShowHidden,
                Msg::PICKER_SHOW_HIDDEN,
                None,
            )
            .checked(state.model.show_hidden()),
        );
        items.push(command_item(
            cx,
            crate::MenuCommand::Refresh,
            Msg::PICKER_REFRESH,
            None,
        ));
        let properties = command_item(
            cx,
            crate::MenuCommand::Properties,
            Msg::PICKER_PROPERTIES,
            Some((KeyChord::new(Key::Enter, alt), "Alt+Enter")),
        );
        // The background stands for the folder on screen, so its Properties always has a
        // subject, whatever the list behind the menu has marked. The shell's rows speak for
        // the same folder, which is what makes this menu the folder's own.
        items.push(properties);
        items.push(MenuItem::separator());
        shell_menu_rows(state, cx, &shell_paths, &mut items);
    }
    items
}

/// The platform's rows behind one submenu row, plus the native menu's door, for the paths
/// a menu speaks for; every target merges them the same way.
///
/// Returns whether anything was added: without a host the menu keeps the rows it has.
fn shell_menu_rows(
    state: &mut PickerState,
    cx: &mut Cx<'_, '_>,
    shell_paths: &[PathBuf],
    items: &mut Vec<MenuItem>,
) -> bool {
    let Some(host) = &state.shell_menu else {
        return false;
    };
    if shell_paths.is_empty() || !host.available(shell_paths) {
        return false;
    }
    let merged = state
        .shell_rows
        .as_ref()
        .filter(|shell| shell.paths == shell_paths)
        .map(|shell| (shell.rows.clone(), shell.omitted));
    if let Some((rows, omitted)) = merged {
        let mut next = 0;
        let mut menu = shell_menu_items(state, &rows, &mut next);
        if menu.iter().any(|item| !matches!(item.kind, MenuRow::Separator)) {
            if omitted > 0 {
                // The native menu beside this one still holds the rest; saying so
                // is better than a list that quietly looks complete.
                menu.push(MenuItem::separator());
                menu.push(
                    MenuItem::new(
                        Id::str("picker/menu/shell-note"),
                        cx.t(Msg::PICKER_SHELL_PARTIAL).to_string(),
                    )
                    .disabled(),
                );
            }
            items.push(MenuItem::submenu(
                Id::of(("picker/menu/shell", "root")),
                cx.t(Msg::PICKER_SHELL_MENU).to_string(),
                menu,
            ));
        }
    }
    items.push(command_item(
        cx,
        crate::MenuCommand::ShellMenu,
        Msg::PICKER_MORE_OPTIONS,
        None,
    ));
    true
}

/// The platform's rows as this menu's rows, recording the command each one runs.
///
/// A row the shell draws itself never reaches here: the read refuses the whole menu when one
/// cannot be carried, so every label and state is the shell's own answer.
fn shell_menu_items(
    state: &mut PickerState,
    rows: &[ShellMenuRow],
    next: &mut u64,
) -> Vec<MenuItem> {
    let mut items: Vec<MenuItem> = Vec::with_capacity(rows.len());
    for row in rows {
        if row.separator {
            items.push(MenuItem::separator());
            continue;
        }
        match &row.submenu {
            Some(children) => {
                let children = shell_menu_items(state, children, next);
                if !children
                    .iter()
                    .any(|item| !matches!(item.kind, MenuRow::Separator))
                {
                    continue;
                }
                let id = Id::of(("picker/menu/shell-submenu", *next));
                *next += 1;
                items.push(MenuItem::submenu(
                    id,
                    row.label.clone(),
                    trim_shell_dividers(children),
                ));
            }
            None => {
                let id = Id::of(("picker/menu/shell-row", row.token));
                state.shell_tokens.insert(id, row.token);
                let mut item = MenuItem::new(id, row.label.clone());
                if row.checked {
                    item = item.checked(true);
                }
                if !row.enabled {
                    item = item.disabled();
                }
                items.push(item);
            }
        }
    }
    trim_shell_dividers(items)
}

/// A divider that starts or ends the rows says nothing, and a doubled one even less.
fn trim_shell_dividers(items: Vec<MenuItem>) -> Vec<MenuItem> {
    let mut out: Vec<MenuItem> = Vec::with_capacity(items.len());
    for item in items {
        let divider = matches!(item.kind, MenuRow::Separator);
        let after_divider = out
            .last()
            .is_some_and(|last| matches!(last.kind, MenuRow::Separator));
        if divider && (out.is_empty() || after_divider) {
            continue;
        }
        out.push(item);
    }
    while out
        .last()
        .is_some_and(|last| matches!(last.kind, MenuRow::Separator))
    {
        out.pop();
    }
    out
}

/// A menu row that is a label and nothing else: the effect rows and the way out.
fn menu_label_row(
    state: &PickerState,
    cx: &mut Cx<'_, '_>,
    id: Id,
    message: Msg,
) -> Box<dyn Element> {
    let pack = state.pack;
    let ink = pack.colour_or(CommonColor::OnSurfaceRaised, Color::WHITE);
    let hover = pack.colour_or(CommonColor::SurfaceHover, Color::from_rgba8(255, 255, 255, 18));
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    let spec = pack.motion.get(MotionName::StateChange).spec;
    let st = cx.state(id);
    let lift = cx.at(id).animate(
        id.with("hover"),
        if st.hovered || st.pressed { 1.0 } else { 0.0 },
        spec,
    );
    let fill = hover.faded(lift);
    let inner = MENU_WIDTH - 2.0 * MENU_PADDING;
    let label = cx.t(message).to_string();
    control(
        cx,
        Decl::container(id, NodeKind::Row)
            .size(SizeClass::fixed(inner, MENU_ROW_HEIGHT))
            .padding(Insets::new(8.0, 0.0, 8.0, 0.0))
            .cursor(CursorShape::PointingHand)
            .sense(Sense::HOVER | Sense::CLICK)
            .cross_align(0.0)
            .cross_pos(0.5)
            .background(fill, Corners::all(radius))
            .child(
                Text::new(id.with("label"), label, caption_size(pack), ink)
                    .max_lines(1)
                    .max_width(inner - 16.0)
                    .resolve(),
            ),
    )
}

/// A popup window that reports the engine's light dismiss and its own Escape chord as one
/// [`Change::Dismissed`].
///
/// The engine attributes both to the popup root's id, which is the node this element builds.
/// A `Container` already carries live children but reports no change, and `Pressable` is a
/// leaf, so a popup whose rows are controls needs both halves in one element.
struct Dismissible {
    inner: Container,
    chord: ShortcutId,
}

impl Element for Dismissible {
    fn decl(&self, cx: &mut ui_core::cx::Cx<'_>) -> Decl {
        self.inner.decl(cx)
    }

    fn children(&self) -> &[Box<dyn Element>] {
        Element::children(&self.inner)
    }

    fn change(&self, state: &WidgetState, _bounds: Rect) -> Option<Change> {
        (state.dismissed || state.shortcut == Some(self.chord)).then_some(Change::Dismissed)
    }
}

fn body(
    state: &mut PickerState,
    cx: &mut Cx<'_, '_>,
    width: f32,
    sidebar_width: f32,
    height: f32,
) -> Box<dyn Element> {
    let pack = state.pack;
    let mut children: Vec<Box<dyn Element>> = Vec::new();
    let mut panel_width = width;
    if sidebar_width > 0.0 {
        children.push(sidebar(state, cx, sidebar_width, height));
        // The rule belongs to the body's row, not to either region: the sidebar keeps one
        // fill and the panel its own, so a border on either would paint around all four sides.
        children.push(from_decl(
            Decl::container(Id::str("picker/sidebar/rule"), NodeKind::Box)
                .size(SizeClass::fixed(RULE, height))
                .background(
                    pack.colour_or(CommonColor::Divider, Color::from_rgba8(255, 255, 255, 30)),
                    Corners::ZERO,
                ),
        ));
        panel_width = (width - sidebar_width - RULE).max(0.0);
    }
    children.push(panel(state, cx, panel_width, height));
    element(
        Decl::container(Id::str(BODY), NodeKind::Row)
            .size(SizeClass::fixed(width, height))
            .cross_align(0.0),
        children,
    )
}

fn sidebar(
    state: &mut PickerState,
    cx: &mut Cx<'_, '_>,
    width: f32,
    height: f32,
) -> Box<dyn Element> {
    let pack = state.pack;
    let surface = pack.colour_or(
        CommonColor::Surface,
        Color::from_rgba8(22, 22, 26, 255),
    );
    let places = state.places.clone();
    let extra_places = state.extra_places.clone();
    let devices = state.devices.clone();
    let current = state.model.path().to_path_buf();

    let mut rows: Vec<Box<dyn Element>> = Vec::new();
    for place in places {
        let label = cx.t(icons::place_label(place.kind)).to_string();
        let selected = current == place.path;
        rows.push(side_row(
            state,
            cx,
            place_id(&place),
            icons::place_mark(place.kind),
            label,
            selected,
            Some(place.path.as_path()),
            width,
        ));
    }
    // The caller's own places follow the platform's: they are extra destinations, not a
    // second vocabulary the sidebar has to tell apart.
    for place in extra_places {
        let selected = current == place.path;
        rows.push(side_row(
            state,
            cx,
            extra_place_id(&place),
            icons::file_mark(FileKind::Folder),
            place.label.clone(),
            selected,
            Some(place.path.as_path()),
            width,
        ));
    }
    if !devices.is_empty() {
        rows.push(side_header(state, cx, width, Msg::PICKER_DEVICES));
        for device in devices {
            let label = device
                .label
                .clone()
                .unwrap_or_else(|| cx.t(Msg::PICKER_DEVICE_ROOT).to_string());
            let selected = current == device.path;
            rows.push(side_row(
                state,
                cx,
                device_id(&device),
                icons::device_mark(),
                label,
                selected,
                Some(device.path.as_path()),
                width,
            ));
        }
    }
    // Media the desktop can mount but has not: a click mounts it and opens what the
    // platform says its mount point is.
    if let Some(volumes) = &state.volumes
        && let VolumesState::Ready(removable) = volumes.state()
        && !removable.is_empty()
    {
        rows.push(side_header(state, cx, width, Msg::PICKER_REMOVABLE));
        for volume in removable.iter() {
            let label = volume
                .label
                .clone()
                .unwrap_or_else(|| cx.t(Msg::PICKER_REMOVABLE_UNNAMED).to_string());
            rows.push(side_row(
                state,
                cx,
                mountable_id(&volume.id),
                icons::device_mark(),
                label,
                false,
                // A volume that is not mounted names no folder yet.
                None,
                width,
            ));
        }
    }
    let list = element(
        Decl::container(Id::str(SIDEBAR_LIST), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, f32::INFINITY),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            // The inset is content padding, not a margin around the scroller: the bar and
            // the clip stay at the region's edge, over the padding rather than the rows.
            .padding(Insets::all(PADDING))
            .gap(2.0),
        rows,
    );
    // A machine with many mounts outgrows the panel; the list scrolls rather than clipping
    // the volumes a user may be looking for.
    let scroller = Scroll::fill(sidebar_scroll_id()).with_scrollbar(ScrollbarConf::on(&pack));
    element(
        Decl::container(Id::str(SIDEBAR), NodeKind::Column)
            .size(SizeClass::fixed(width, height))
            .background(surface, Corners::ZERO),
        vec![Box::new(scroller.element_in(cx).children(vec![list]))],
    )
}

/// One sidebar row: a place or a mounted volume, with the same hover, selection and click.
///
/// A row that names a folder is also a drop target, so a drag can land in it or rest on it
/// and spring into it, as a folder row of the list can.
fn side_row(
    state: &PickerState,
    cx: &mut Cx<'_, '_>,
    id: Id,
    mark: IconSource,
    label: String,
    selected: bool,
    folder: Option<&Path>,
    width: f32,
) -> Box<dyn Element> {
    let pack = state.pack;
    let ink = pack.colour_or(CommonColor::OnSurface, Color::WHITE);
    let hover = pack.colour_or(CommonColor::SurfaceHover, Color::from_rgba8(255, 255, 255, 18));
    let selected_fill = pack.colour_or(
        CommonColor::SurfaceSelected,
        Color::from_rgba8(255, 255, 255, 36),
    );
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    let spec = pack.motion.get(MotionName::StateChange).spec;
    let st = cx.state(id);
    let lift = cx.at(id).animate(
        id.with("hover"),
        if st.hovered || st.pressed { 1.0 } else { 0.0 },
        spec,
    );
    // The pointer's wash sits over the selected fill rather than replacing it.
    let base = if selected {
        selected_fill
    } else {
        Color::TRANSPARENT
    };
    let fill = hover.faded(lift).over(base);
    let fill = if st.drag_over && folder.is_some_and(|path| state.drop_accepts(path)) {
        pack.colour_or(CommonColor::Accent, ink).faded(0.28).over(fill)
    } else {
        fill
    };
    let mut decl = Decl::container(id, NodeKind::Row)
        .size(SizeClass {
            width: AxisSize::fill(0.0, 0.0),
            height: AxisSize::fixed(ROW_HEIGHT),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .padding(Insets::new(8.0, 0.0, 8.0, 0.0))
        .gap(GAP)
        .cross_align(0.0)
        .cross_pos(0.5)
        .sense(Sense::HOVER | Sense::CLICK)
        .cursor(CursorShape::PointingHand)
        .background(fill, Corners::all(radius))
        .child(mark.resolve_with_size(id.with("icon"), ink, MARK_SIZE))
        .child(
            Text::new(id.with("label"), label, caption_size(pack), ink)
                .nowrap()
                // The sidebar's padding, the row's own, the mark and the gap between
                // them: what is left is the room the label may take.
                .max_width(width - 2.0 * PADDING - 2.0 * 8.0 - MARK_SIZE - GAP)
                .resolve(),
        );
    if folder.is_some() {
        // A row that names a folder takes the drag so a hold springs into it; whether the
        // release would change anything is the wash's and the release's question.
        decl = decl.accept_drop([uri_list::MIME]);
    }
    control(cx, decl)
}

/// The heading above the volumes, which are not places and are worth telling apart.
fn side_header(
    state: &PickerState,
    cx: &mut Cx<'_, '_>,
    width: f32,
    message: Msg,
) -> Box<dyn Element> {
    let pack = state.pack;
    let muted = pack.colour_or(CommonColor::TextMuted, Color::from_rgba8(170, 170, 180, 255));
    let id = Id::str(SIDEBAR_DEVICES);
    element(
        Decl::container(id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, f32::INFINITY),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets::new(8.0, 12.0, 8.0, 4.0))
            .cross_align(0.0)
            .cross_pos(0.5),
        vec![from_decl(
            Text::new(
                id.with("label"),
                cx.t(message).to_string(),
                caption_size(pack),
                muted,
            )
            .nowrap()
            .max_width(width - 2.0 * PADDING - 16.0)
            .resolve(),
        )],
    )
}

/// The panel as a node: it stands for the folder on screen, so a file drag over the space
/// between rows — or over an empty folder's message — lands in that folder. While such a
/// drag hovers, a wash and an edge say which folder that is.
fn panel_decl(state: &PickerState, cx: &mut Cx<'_, '_>, width: f32, height: f32) -> Decl {
    let mut decl = Decl::container(Id::str(PANEL), NodeKind::Column)
        .size(SizeClass::fixed(width, height));
    // A drag from inside this folder has nowhere to land in it, so the background stops
    // offering itself as a target rather than taking the drop and doing nothing.
    if state.drop_accepts(&state.model.path()) {
        decl = decl.accept_drop([uri_list::MIME]);
    }
    if !cx.state(Id::str(PANEL)).drag_over {
        return decl;
    }
    let accent = state.pack.colour_or(
        CommonColor::Accent,
        state.pack.colour_or(CommonColor::OnSurface, Color::WHITE),
    );
    decl.background(accent.faded(0.10), Corners::ZERO)
        .border(accent.faded(0.55), 1.0)
}

fn panel(
    state: &mut PickerState,
    cx: &mut Cx<'_, '_>,
    width: f32,
    height: f32,
) -> Box<dyn Element> {
    let inner = (width - 2.0 * PADDING).max(0.0);
    if state.layout == crate::Layout::Grid {
        // A grid has no columns to name, so the header's band belongs to the list alone and
        // the cells take the whole panel.
        let columns = grid_columns(inner);
        state.columns = columns;
        let cell = grid_cell_width(inner, columns);
        return element(
            panel_decl(state, cx, width, height),
            vec![grid(state, cx, width, inner, height, columns, cell)],
        );
    }
    // The columns keep their floors and the header scrolls sideways to reach them: a
    // narrower panel must not squeeze a name out of existence.
    let content = inner.max(list_min_width());
    // The header owns the horizontal offset. The rows follow it rather than scrolling
    // themselves: dragging the list sideways would move a column one row-height at a time,
    // while the header is the one strip whose whole purpose is its columns.
    let shift = cx
        .core()
        .scroll_of(header_scroll_id())
        .map_or(0.0, |geom| geom.offset_x);
    let list_height = (height - HEADER_HEIGHT - RULE).max(0.0);
    element(
        panel_decl(state, cx, width, height),
        vec![
            header(state, cx, width, content),
            from_decl(header_rule(state, width)),
            list(state, cx, width, content, shift, list_height),
        ],
    )
}

/// How many cells a panel `inner` wide holds: as many as fit a cell and its gap, at least one.
fn grid_columns(inner: f32) -> usize {
    let pitch = GRID_CELL_WIDTH + GRID_GAP;
    (((inner + GRID_GAP) / pitch).floor() as usize).max(1)
}

/// The cell's width once the row's gaps are taken out, so the columns reach both edges.
fn grid_cell_width(inner: f32, columns: usize) -> f32 {
    let gaps = GRID_GAP * (columns.saturating_sub(1)) as f32;
    ((inner - gaps) / columns as f32).max(GRID_CELL_WIDTH)
}

/// The header as its own horizontal scroller: its labels stay over their columns while the
/// list below moves with the same offset.
fn header(
    state: &PickerState,
    cx: &mut Cx<'_, '_>,
    viewport: f32,
    content: f32,
) -> Box<dyn Element> {
    let pack = state.pack;
    let muted = pack.colour_or(CommonColor::TextMuted, Color::from_rgba8(170, 170, 180, 255));
    let divider = pack.colour_or(CommonColor::Divider, Color::from_rgba8(255, 255, 255, 30));
    let width = content + 2.0 * PADDING;
    let name_width = column_width(content, SIZE_COLUMN, MODIFIED_COLUMN);
    let mut row = Decl::container(Id::str(HEADER).with("row"), NodeKind::Row)
        .size(SizeClass::fixed(width, HEADER_HEIGHT))
        .padding(Insets::new(PADDING + COLUMN_INSET, 0.0, PADDING + COLUMN_INSET, 0.0))
        .gap(GAP)
        .cross_align(0.0)
        .cross_pos(0.5)
        // The rows start with a mark; the header starts with its slot, so "Name" sits over
        // the names rather than over the icons.
        .child(
            Decl::container(Id::str(HEADER).with("mark"), NodeKind::Box)
                .size(SizeClass::fixed(MARK_SIZE, 1.0)),
        );
    for (id, msg, column, align) in [
        ("name", Msg::PICKER_NAME, name_width, TextAlign::Start),
        ("size", Msg::PICKER_SIZE, SIZE_COLUMN, TextAlign::End),
        (
            "modified",
            Msg::PICKER_MODIFIED,
            MODIFIED_COLUMN,
            TextAlign::End,
        ),
    ] {
        row = row.child(cell(
            Text::new(
                Id::str(HEADER).with(id),
                cx.t(msg).to_string(),
                caption_size(pack),
                muted,
            )
            .weight(Weight::MEDIUM)
            .nowrap(),
            column,
            align,
        ));
    }
    // The column rules are shorter than the header and do not meet its edges, so the
    // header reads as one strip rather than as three boxes.
    let rule_height = 14.0;
    let left = PADDING + COLUMN_INSET;
    let first = left + MARK_SIZE + GAP + name_width + GAP * 0.5;
    let second = first + GAP * 0.5 + SIZE_COLUMN + GAP * 0.5;
    let mut stack = Decl::container(Id::str(HEADER), NodeKind::Stack)
        .size(SizeClass::fixed(width, HEADER_HEIGHT))
        .child(row);
    for (index, x) in [first, second].into_iter().enumerate() {
        stack = stack.child(
            Decl::container(Id::str(HEADER).with("rule").with(index), NodeKind::Box)
                .size(SizeClass::fixed(RULE, rule_height))
                .background(divider.faded(0.55), Corners::ZERO)
                .offset(x - RULE * 0.5, (HEADER_HEIGHT - rule_height) * 0.5),
        );
    }
    let scroll = Scroll::horizontal(header_scroll_id(), viewport, HEADER_HEIGHT)
        .with_scrollbar(ScrollbarConf::never());
    Box::new(scroll.element_in(cx).children(vec![from_decl(stack)]))
}

/// The header's underline: fainter than the toolbar's rule and inset from both ends, so the
/// labels sit in their own band instead of on the dialog's own divider.
fn header_rule(state: &PickerState, width: f32) -> Decl {
    let divider = state
        .pack
        .colour_or(CommonColor::Divider, Color::from_rgba8(255, 255, 255, 30));
    Decl::container(Id::str(HEADER).with("underline"), NodeKind::Column)
        .size(SizeClass::fixed(width, RULE))
        .padding(Insets::new(PADDING + COLUMN_INSET, 0.0, PADDING + COLUMN_INSET, 0.0))
        .child(
            Decl::container(Id::str(HEADER).with("underline/line"), NodeKind::Box)
                .size(SizeClass {
                    width: AxisSize::fill(0.0, 0.0),
                    height: AxisSize::fixed(RULE),
                    grow_x: Grow::No,
                    grow_y: Grow::No,
                })
                .background(divider.faded(0.7), Corners::ZERO),
        )
}

/// One details cell: the text takes exactly `width`, so every row's next column starts at the
/// same place. A hugging text would let the figures ride up against a short name.
fn cell(text: Text, width: f32, align: TextAlign) -> Decl {
    let mut decl = text.max_width(width).align(align).resolve();
    decl.size_class.width = AxisSize::fixed(width);
    decl
}

fn list(
    state: &mut PickerState,
    cx: &mut Cx<'_, '_>,
    viewport: f32,
    content: f32,
    shift: f32,
    height: f32,
) -> Box<dyn Element> {
    let pack = state.pack;
    let scroller = list_id(state.model.path());
    if state.model.rows().is_empty() {
        return empty_message(state, cx, viewport, height);
    }

    let count = state.model.rows().len();
    let window = state.heights.window(cx.core(), scroller, count, ROW_HEIGHT * 3.0, |index| {
        row_id(&state.model.rows()[index])
    });
    // Only vertically: the header above owns the horizontal offset, and a list that also
    // scrolled sideways would move a row under the pointer without the header moving.
    // The inset is still content padding, which keeps the upright bar and the clip over the
    // padding instead of directly on a column.
    let scroll = Scroll::fill(scroller).with_scrollbar(ScrollbarConf::on(pack));
    let mut children: Vec<Box<dyn Element>> = Vec::new();
    if window.above > 0.0 {
        children.push(from_decl(spacer(
            scroller.with("above"),
            content,
            window.above,
        )));
    }
    for index in window.first..window.last {
        let entry = state.model.rows()[index].clone();
        children.push(row(state, cx, &entry, index, content, shift));
    }
    if window.below > 0.0 {
        children.push(from_decl(spacer(
            scroller.with("below"),
            content,
            window.below,
        )));
    }
    let contents = element(
        Decl::container(scroller.with("content"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(content + 2.0 * PADDING),
                height: AxisSize::hug(0.0, f32::INFINITY),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets::new(PADDING, 0.0, PADDING, 0.0)),
        children,
    );
    Box::new(scroll.element_in(cx).children(vec![contents]))
}

/// The one line a panel shows when it has no rows: reading, empty or failed.
fn empty_message(
    state: &PickerState,
    cx: &mut Cx<'_, '_>,
    width: f32,
    height: f32,
) -> Box<dyn Element> {
    // The platform's own reason goes under the localized line: it is diagnostic text,
    // not a string this crate authors, and dropping it would hide why the read failed.
    let (message, reason) = match &state.model.load {
        Load::Loading if state.model.searching() => (Msg::PICKER_SEARCHING, None),
        Load::Failed(reason) => (Msg::PICKER_FAILED, Some(reason.to_string())),
        Load::Ready if state.model.searching() => (Msg::PICKER_NO_MATCHES, None),
        Load::Loading => (Msg::PICKER_LOADING, None),
        Load::Ready => (Msg::PICKER_EMPTY, None),
    };
    state_message(state, cx, message, reason, width, height)
}

/// The grid: one scroller over rows of cells, each cell a picture over a two-line name.
///
/// The window is measured in grid rows rather than entries, so a thousand files cost the
/// same handful of rows to build that a short folder does.
fn grid(
    state: &mut PickerState,
    cx: &mut Cx<'_, '_>,
    viewport: f32,
    content: f32,
    height: f32,
    columns: usize,
    cell_width: f32,
) -> Box<dyn Element> {
    let pack = state.pack;
    let scroller = grid_id(state.model.path());
    if state.model.rows().is_empty() {
        return empty_message(state, cx, viewport, height);
    }
    let count = state.model.rows().len();
    let rows = count.div_ceil(columns);
    let window = state.grid_heights.window(
        cx.core(),
        scroller,
        rows,
        GRID_ROW_PITCH * 2.0,
        grid_row_id,
    );
    let scroll = Scroll::fill(scroller).with_scrollbar(ScrollbarConf::on(pack));
    let mut children: Vec<Box<dyn Element>> = Vec::new();
    if window.above > 0.0 {
        children.push(from_decl(spacer(scroller.with("above"), content, window.above)));
    }
    for row in window.first..window.last {
        children.push(grid_row(state, cx, row, columns, cell_width, content));
    }
    if window.below > 0.0 {
        children.push(from_decl(spacer(scroller.with("below"), content, window.below)));
    }
    let contents = element(
        Decl::container(scroller.with("content"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(viewport),
                height: AxisSize::hug(0.0, f32::INFINITY),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets::new(PADDING, 0.0, PADDING, 0.0)),
        children,
    );
    Box::new(scroll.element_in(cx).children(vec![contents]))
}

fn grid_row(
    state: &mut PickerState,
    cx: &mut Cx<'_, '_>,
    row: usize,
    columns: usize,
    cell_width: f32,
    width: f32,
) -> Box<dyn Element> {
    let first = row * columns;
    let last = (first + columns).min(state.model.rows().len());
    let mut cells: Vec<Box<dyn Element>> = Vec::new();
    for index in first..last {
        let entry = state.model.rows()[index].clone();
        cells.push(grid_cell(state, cx, &entry, index, columns, cell_width));
    }
    element(
        Decl::container(grid_row_id(row), NodeKind::Row)
            .size(SizeClass::fixed(width, GRID_ROW_PITCH))
            // The pitch includes the gap, so the bottom padding is what a measured row reports.
            .padding(Insets::new(0.0, 0.0, 0.0, GRID_GAP))
            .gap(GRID_GAP)
            .cross_align(0.0),
        cells,
    )
}

fn grid_cell(
    state: &mut PickerState,
    cx: &mut Cx<'_, '_>,
    entry: &Entry,
    index: usize,
    columns: usize,
    width: f32,
) -> Box<dyn Element> {
    let pack = state.pack;
    let id = grid_cell_id(entry);
    let ink = pack.colour_or(CommonColor::OnSurface, Color::WHITE);
    let hover = pack.colour_or(CommonColor::SurfaceHover, Color::from_rgba8(255, 255, 255, 18));
    let selected = pack.colour_or(
        CommonColor::SurfaceSelected,
        Color::from_rgba8(255, 255, 255, 36),
    );
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    let spec = pack.motion.get(MotionName::StateChange).spec;
    let is_selected = state.model.is_selected(index);
    let focused = state.model.is_cursor(index);
    let st = cx.state(id);
    let lift = cx.at(id).animate(
        id.with("hover"),
        if st.hovered || st.pressed { 1.0 } else { 0.0 },
        spec,
    );
    let base = if is_selected {
        selected
    } else {
        Color::TRANSPARENT
    };
    let fill = hover.faded(lift).over(base);
    // The release refuses a target that would change nothing; its wash must refuse too,
    // while it still takes the drag so a hold can spring into it.
    let drop_over = st.drag_over && state.drop_accepts(&entry.path);
    let fill = if drop_over {
        pack.colour_or(CommonColor::Accent, ink).faded(0.28).over(fill)
    } else {
        fill
    };
    let ring = pack.colour_or(CommonColor::FocusRing, pack.colour_or(CommonColor::Accent, ink));
    let dim = if entry.hidden { HIDDEN_OPACITY } else { 1.0 }
        * if state.is_cut(&entry.path) {
            CUT_OPACITY
        } else {
            1.0
        };
    let renaming = state
        .rename
        .as_ref()
        .is_some_and(|field| field.path == entry.path);
    let mark = mark(state, cx, entry, id.with("mark"), ink, GRID_IMAGE, dim);
    let caption = caption_size(pack);
    let payload = drag_payload(state, index);
    let mut decl = Decl::container(id, NodeKind::Column)
        .size(SizeClass::fixed(width, GRID_CELL_HEIGHT))
        .padding(Insets::all(6.0))
        .gap(4.0)
        // The picture and the two name lines are centred; the name keeps the same cell width
        // on every row so a wrapped name cannot make its column narrower.
        .cross_align(0.0)
        .cross_pos(0.5)
        .sense(Sense::HOVER | Sense::CLICK)
        .drag_data(payload)
        .drag_out()
        .cursor(CursorShape::PointingHand)
        .background(fill, Corners::all(radius))
        .child(mark);
    if renaming {
        decl = decl
            .child(rename_cell(state, cx, width - 12.0))
            .cursor(CursorShape::Text);
    } else {
        let name = Text::new(
            id.with("name"),
            shown_name(state, entry),
            caption,
            ink.faded(dim),
        )
        .max_lines(2)
        .max_width(width - 12.0);
        name_tooltip(state, cx, id, &name);
        decl = decl.child(cell(name, width - 12.0, TextAlign::Center));
    }
    if entry.directory {
        // Every folder takes the drag, so a hold can spring into it; the release and the
        // wash decide with `moves_into` whether anything would change.
        decl = decl.accept_drop([uri_list::MIME]);
    }
    if focused {
        decl = decl.border(ring, FOCUS_RING);
    }
    if drop_over {
        decl = decl.border(pack.colour_or(CommonColor::Accent, ring), FOCUS_RING);
    }
    Box::new(GridControl {
        decl,
        index,
        columns,
        cell_width: width,
    })
}

/// A cell that reports its own gesture, mapping the pointer's own offsets to the cell it is
/// over: the drag began inside this cell, so one row down and two columns right is here plus
/// the pitches travelled.
struct GridControl {
    decl: Decl,
    index: usize,
    columns: usize,
    cell_width: f32,
}

impl Element for GridControl {
    fn decl(&self, _cx: &mut ui_core::cx::Cx<'_>) -> Decl {
        self.decl.clone()
    }

    fn change(&self, state: &WidgetState, _bounds: Rect) -> Option<Change> {
        // A move, not the threshold frame: a native drag never reports one, so a row that
        // carries a payload to other windows does not sweep this selection as it leaves.
        if state.drag_step.is_some() {
            let local = state.local?;
            let columns = self.columns.max(1);
            let row = (self.index / columns) as f32 + (local.y / GRID_ROW_PITCH).floor();
            let column =
                (self.index % columns) as f32 + (local.x / (self.cell_width + GRID_GAP)).floor();
            let row = row.max(0.0) as usize;
            let column = column.clamp(0.0, (columns - 1) as f32) as usize;
            return Some(Change::Data(Box::new(RowGesture {
                index: self.index,
                drag_row: Some(row * columns + column),
                modifiers: state.modifiers,
                trigger: None,
            })));
        }
        state.activation().then(|| {
            Change::Data(Box::new(RowGesture {
                index: self.index,
                drag_row: None,
                modifiers: state.modifiers,
                trigger: Some(if state.clicked {
                    RowTrigger::Click {
                        clicks: state.clicks,
                    }
                } else {
                    RowTrigger::Key
                }),
            }))
        })
    }
}

fn spacer(id: Id, width: f32, height: f32) -> Decl {
    Decl::container(id, NodeKind::Box).size(SizeClass::fixed(width, height))
}

fn state_message(
    state: &PickerState,
    cx: &mut Cx<'_, '_>,
    message: Msg,
    reason: Option<String>,
    width: f32,
    height: f32,
) -> Box<dyn Element> {
    let pack = state.pack;
    let muted = pack.colour_or(CommonColor::TextMuted, Color::from_rgba8(170, 170, 180, 255));
    let mut children: Vec<Box<dyn Element>> = vec![from_decl(
        Text::new(
            Id::str(STATE).with("label"),
            cx.t(message).to_string(),
            caption_size(pack),
            muted,
        )
        .max_width(width - 2.0 * PADDING - 16.0)
        .resolve(),
    )];
    if let Some(reason) = reason {
        children.push(from_decl(
            Text::new(
                Id::str(STATE).with("reason"),
                reason,
                caption_size(pack),
                muted,
            )
            .max_width(width - 2.0 * PADDING - 16.0)
            .resolve(),
        ));
    }    // A failed read is the one state a user can act on here; the toolbar still navigates,
    // but retrying is the action that does not make them leave and come back.
    if matches!(state.model.load, Load::Failed(_)) {
        let retry = button(
            cx,
            pack,
            retry_id(),
            cx.t(Msg::PICKER_RETRY).to_string(),
            false,
        );
        children.push(retry);
    }
    element(
        Decl::container(Id::str(STATE), NodeKind::Column)
            .size(SizeClass::fixed(width, height))
            // The panel no longer pads the scroll region; the message keeps the rows' inset
            // so a state line starts where a row's text would.
            .padding(Insets::new(PADDING + 8.0, 16.0, PADDING + 8.0, 0.0))
            .gap(4.0)
            // Start-aligned children: the retry button hugs its label instead of spanning
            // the panel, and the lines keep their natural width.
            .cross_align(0.0),
        children,
    )
}

/// The name a row shows. In search results the folder is part of what tells two matches
/// apart, so the row carries the path from the search root.
fn shown_name(state: &PickerState, entry: &Entry) -> String {
    if state.model.searching()
        && let Ok(relative) = entry.path.strip_prefix(state.model.path())
    {
        let shown = relative.to_string_lossy();
        if !shown.is_empty() {
            return shown.into_owned();
        }
    }
    entry.name.clone()
}

/// Show the full name of the row the pointer rests on, when its label is cut.
///
/// The delay is application state, not an engine fact: the row's own age counts while the
/// pointer rests on a cut name, and leaving forgets it, so a second visit waits again.
fn name_tooltip(state: &mut PickerState, cx: &mut Cx<'_, '_>, row: Id, name: &Text) {
    let age_id = row.with("tooltip-age");
    let st = cx.state(row);
    if covered(state)
        || cx.core().drag_step().is_some()
        || !st.hovered
        || st.pressed
        || !name.truncated_in(cx).unwrap_or(false)
    {
        cx.at(row).forget_age(age_id);
        return;
    }
    if cx.at(row).age(age_id) >= NAME_TOOLTIP_DELAY {
        state.name_tooltip = Some((name.id, name.body.clone()));
    }
}

/// Whether something of the dialog's own stands over the list, where a tooltip would
/// talk about a row the user cannot see.
fn covered(state: &PickerState) -> bool {
    state.menu_open.is_some()
        || state.drop_menu.is_some()
        || state.filter_menu
        || state.choice_menu.is_some()
        || state.crumb_menu.is_some()
        || state.conflict.is_some()
        || state.delete_ask.is_some()
        || state.undo_ask.is_some()
        || state.overwrite_confirm.is_some()
        || state.transfer.is_some()
        || state.rename.is_some()
        || state.failures.is_some()
        || state.properties.is_some()
}

/// The full name of a row whose column cut it: a small panel above the label.
///
/// It is painted in the window, not given a surface: a popup would take the pointer capture
/// from the row it is describing and end the hover that keeps it up.
fn name_tooltip_panel(pack: &Style, anchor: Id, name: String) -> Box<dyn Element> {
    let surface = pack.opaque_or(
        CommonColor::SurfaceRaised,
        Color::from_rgba8(28, 28, 32, 255),
    );
    let divider = pack.colour_or(CommonColor::Divider, Color::from_rgba8(255, 255, 255, 30));
    let ink = pack.colour_or(CommonColor::OnSurface, Color::WHITE);
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    from_decl(
        Decl::container(Id::str("picker/name-tooltip"), NodeKind::Box)
            .overlay(OverlayPlacement {
                anchor: Anchor::Node(anchor),
                side: Side::Above,
                align: OverlayAlign::Start,
                offset: NAME_TOOLTIP_GAP,
                flip: true,
                match_anchor_width: false,
                match_anchor_height: false,
                min_width: 0.0,
                presentation: Presentation::Parent,
            })
            // The pointer belongs to the row underneath: the tooltip is never a target.
            .sense(Sense::NONE)
            .padding(Insets::new(8.0, 4.0, 8.0, 4.0))
            .background(surface, Corners::all(radius))
            .border(divider, 1.0)
            .elevation(2)
            .child(
                Text::new(
                    Id::str("picker/name-tooltip/text"),
                    name,
                    caption_size(pack),
                    ink,
                )
                .max_width(NAME_TOOLTIP_WIDTH)
                .resolve(),
            ),
    )
}

/// What a drag that starts on `index` carries.
///
/// The marked set when the row is part of it, the row itself otherwise. Every effect is
/// permitted because the drop's modifiers pick between them, and moving is the preference a
/// file manager's drag starts from.
fn drag_payload(state: &PickerState, index: usize) -> Rc<DragData> {
    let paths = state.model.drag_paths(index);
    Rc::new(
        DragData::bytes(uri_list::MIME, uri_list::encode(&paths))
            .with_effects(DropEffects::ALL, DropEffect::Move),
    )
}

fn row(
    state: &mut PickerState,
    cx: &mut Cx<'_, '_>,
    entry: &Entry,
    index: usize,
    width: f32,
    shift: f32,
) -> Box<dyn Element> {
    let pack = state.pack;
    let id = row_id(entry);
    let ink = pack.colour_or(CommonColor::OnSurface, Color::WHITE);
    let muted = pack.colour_or(CommonColor::TextMuted, Color::from_rgba8(170, 170, 180, 255));
    let hover = pack.colour_or(CommonColor::SurfaceHover, Color::from_rgba8(255, 255, 255, 18));
    let selected = pack.colour_or(
        CommonColor::SurfaceSelected,
        Color::from_rgba8(255, 255, 255, 36),
    );
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    let spec = pack.motion.get(MotionName::StateChange).spec;
    let is_selected = state.model.is_selected(index);
    let focused = state.model.is_cursor(index);
    let st = cx.state(id);
    let lift = cx.at(id).animate(
        id.with("hover"),
        if st.hovered || st.pressed { 1.0 } else { 0.0 },
        spec,
    );
    // Three independent layers: the marks are the answer, the wash is the pointer, and the
    // ring is where the keyboard's cursor stands. Layering the wash over the selection keeps
    // hover visible on a marked row, and the ring keeps the cursor visible on one.
    let base = if is_selected {
        selected
    } else {
        Color::TRANSPARENT
    };
    let fill = hover.faded(lift).over(base);
    // The row a file drag would land on says so before the release: the same tint the
    // selection uses, at a strength that reads under a wash. A target the release would
    // refuse keeps its wash to itself while still taking the drag.
    let drop_over = st.drag_over && state.drop_accepts(&entry.path);
    let fill = if drop_over {
        pack.colour_or(CommonColor::Accent, ink).faded(0.28).over(fill)
    } else {
        fill
    };
    let ring = pack.colour_or(CommonColor::FocusRing, pack.colour_or(CommonColor::Accent, ink));
    let name_width = column_width(width, SIZE_COLUMN, MODIFIED_COLUMN);
    // A cut row waits for its paste: dimmed like a hidden entry, so it is clearly still
    // there but no longer simply part of the folder.
    let dim = if entry.hidden { HIDDEN_OPACITY } else { 1.0 }
        * if state.is_cut(&entry.path) {
            CUT_OPACITY
        } else {
            1.0
        };
    let renaming = state
        .rename
        .as_ref()
        .is_some_and(|field| field.path == entry.path);
    let mark = mark(state, cx, entry, id.with("mark"), ink, MARK_SIZE, dim);
    let body_size = pack.type_table.get(ui_style::TextRole::Body).size;
    let caption = caption_size(pack);
    let size_text = if entry.directory {
        String::new()
    } else {
        format_size(entry.len)
    };
    let modified_text = modified_label(cx, entry.modified);

    let payload = drag_payload(state, index);
    let mut decl = Decl::container(id, NodeKind::Row)
        .size(SizeClass::fixed(width, ROW_HEIGHT))
        .padding(Insets::new(COLUMN_INSET, 0.0, COLUMN_INSET, 0.0))
        .gap(GAP)
        .cross_align(0.0)
        .cross_pos(0.5)
        .sense(Sense::HOVER | Sense::CLICK)
        // A row drag leaves the dialog: another window may take the files, so the gesture
        // is the platform's from the threshold, not this loop's.
        .drag_data(payload)
        .drag_out()
        .cursor(CursorShape::PointingHand)
        .background(fill, Corners::all(radius))
        .child(mark);
    if renaming {
        // The field is the row's only gesture while it is open; a drag owner above it would
        // turn a selection inside the name into a file drag.
        decl = decl
            .child(rename_cell(state, cx, name_width))
            .cursor(CursorShape::Text);
    } else {
        let name = Text::new(
            id.with("name"),
            shown_name(state, entry),
            body_size,
            ink.faded(dim),
        )
        .max_lines(1)
        .max_width(name_width);
        name_tooltip(state, cx, id, &name);
        decl = decl.child(cell(name, name_width, TextAlign::Start));
    }
    decl = decl
        .child(cell(
            Text::new(id.with("size"), size_text, caption, muted).nowrap(),
            SIZE_COLUMN,
            TextAlign::End,
        ))
        .child(cell(
            Text::new(id.with("modified"), modified_text, caption, muted).nowrap(),
            MODIFIED_COLUMN,
            TextAlign::End,
        ));
    if entry.directory {
        // A folder row takes the drag so a hold can spring into it; a file row lets the
        // drop fall through to the list behind it, which stands for the folder on screen.
        decl = decl.accept_drop([uri_list::MIME]);
    }
    if focused {
        decl = decl.border(ring, FOCUS_RING);
    }
    // The transient state outdraws the cursor's ring: the row a release would land on is the
    // one the user is pointing at, and a focus ring underneath it would read as the target.
    if drop_over {
        decl = decl.border(pack.colour_or(CommonColor::Accent, ring), FOCUS_RING);
    }
    Box::new(RowControl {
        decl: decl.offset(-shift, 0.0),
        index,
    })
}

/// A row that reports its own gesture, because a selection click carries the modifiers that
/// decide replace, toggle or extend, and `Change::Activated` has none.
struct RowControl {
    decl: Decl,
    index: usize,
}

impl Element for RowControl {
    fn decl(&self, _cx: &mut ui_core::cx::Cx<'_>) -> Decl {
        self.decl.clone()
    }

    fn change(&self, state: &WidgetState, _bounds: Rect) -> Option<Change> {
        // A move, not the threshold frame: a native drag never reports one, so a row that
        // carries a payload to other windows does not sweep this selection as it leaves.
        if state.drag_step.is_some() {
            // The drag's owner reports every position in its own space; the row under the
            // pointer is this row plus the rows the pointer travelled.
            let local = state.local?;
            let row = (self.index as f32 + (local.y / ROW_HEIGHT).floor()).max(0.0) as usize;
            return Some(Change::Data(Box::new(RowGesture {
                index: self.index,
                drag_row: Some(row),
                modifiers: state.modifiers,
                trigger: None,
            })));
        }
        state.activation().then(|| {
            Change::Data(Box::new(RowGesture {
                index: self.index,
                drag_row: None,
                modifiers: state.modifiers,
                trigger: Some(if state.clicked {
                    RowTrigger::Click {
                        clicks: state.clicks,
                    }
                } else {
                    RowTrigger::Key
                }),
            }))
        })
    }
}

/// The row's leading mark: the thumbnail when the policy asked and the platform has one, then
/// the platform's type icon, then the framework's own glyph. `dim` fades the mark for a hidden
/// entry; a glyph fades through its ink, which costs no offscreen layer.
fn mark(
    state: &mut PickerState,
    cx: &mut Cx<'_, '_>,
    entry: &Entry,
    id: Id,
    ink: Color,
    size: f32,
    dim: f32,
) -> Decl {
    // The slot is measured in logical units and a texture in physical pixels. Asking for the
    // logical size hands the painter a picture smaller than the box it fills, and the stretch
    // is what shows as soft, broken edges at a fractional scale.
    let pixels = (size * cx.scale()).round().max(1.0) as u32;
    let thumbnail = state.thumbnails.as_ref().and_then(|thumbnails| {
        let request = state.model.thumbnail_request(entry, state.policy, pixels)?;
        match thumbnails.get(&request) {
            ThumbnailState::Ready(image) => Some(image),
            // A thumbnailer that answers `Unavailable` costs nothing after the first look;
            // the fallbacks below then draw the type icon, so a folder of text files is not
            // a column of blanks.
            ThumbnailState::Loading | ThumbnailState::Unavailable => None,
        }
    });
    if let Some(image) = thumbnail {
        // Handed over every frame: a picture's id names it, so the engine replaces its copy
        // instead of holding a second one, and a re-made picture's new pixels still land.
        cx.core().register_image(image.clone());
        // The picture is decoration: a press or a menu request on it belongs to the row it
        // fills, not to the mark (an image is a hit consumer, unlike a glyph's text).
        return Decl::container(id, NodeKind::Box)
            .size(SizeClass::fixed(size, size))
            .background_image(image.id, ImageFit::Contain)
            .opacity(dim)
            .sense(Sense::PASS_THROUGH);
    }
    // The item, not just its kind: an executable carries its own icon and a folder may name
    // one in `desktop.ini`, so the platform is asked about the path. A queued icon leaves the
    // framework's glyph in the slot until its completion forces the frame that shows it.
    let request = FileIconRequest::new(entry.path.as_path(), entry.kind, entry.link, pixels);
    let icon = state
        .file_icons
        .as_ref()
        .map(|icons| icons.get(&request));
    if let Some(FileIconState::Ready(image)) = icon {
        cx.core().register_image(image.clone());
        return Decl::container(id, NodeKind::Box)
            .size(SizeClass::fixed(size, size))
            .background_image(image.id, ImageFit::Contain)
            .opacity(dim)
            .sense(Sense::PASS_THROUGH);
    }
    icons::file_mark(entry.kind).resolve_with_size(id, ink.faded(dim), size)
}

/// The share of a job's work that is done: bytes when the worker knows them, items
/// otherwise, because a copy of one huge file must not read as one item of one.
fn progress_percent(running: &crate::Running) -> u32 {
    let (done, total) = if running.bytes_total > 0 {
        (running.bytes_done, running.bytes_total)
    } else {
        (running.done, running.total)
    };
    if total == 0 {
        return 0;
    }
    ((done.min(total) * 100) / total) as u32
}

fn footer(state: &mut PickerState, cx: &mut Cx<'_, '_>, width: f32) -> Box<dyn Element> {
    let pack = state.pack;
    let muted = pack.colour_or(CommonColor::TextMuted, Color::from_rgba8(170, 170, 180, 255));
    let ink = pack.colour_or(CommonColor::OnSurface, Color::WHITE);
    let accept = state.model.accept_enabled();
    let mode = state.model.mode();
    let cancel_label = state
        .cancel_label
        .clone()
        .unwrap_or_else(|| cx.t(Msg::PICKER_CANCEL).to_string());
    let accept_label = state.accept_label.clone().unwrap_or_else(|| match mode {
        Mode::SaveFile | Mode::SaveFiles => cx.t(Msg::PICKER_SAVE).to_string(),
        Mode::SelectFolder => cx.t(Msg::PICKER_SELECT_FOLDER).to_string(),
        Mode::SelectFolders => cx.t(Msg::PICKER_SELECT_FOLDERS).to_string(),
        _ => cx.t(Msg::PICKER_OPEN).to_string(),
    });
    // The look already dims a disabled button; the node must say so too, or a press on it
    // would still arrive as an activation.
    let open = button(cx, pack, open_id(), accept_label, !accept);
    let cancel = button(cx, pack, cancel_id(), cancel_label, false);
    // Two groups: the left is the caller's text — a save's name field or the every-name
    // status — and the right is the filter menu and both buttons, which move as one block so
    // a narrow dialog never lets a control drift away from the group it belongs to.
    let mut left: Vec<Box<dyn Element>> = Vec::new();
    // The in-place field's refusal sits beside it, in the footer's own words.
    if let Some(error) = state.rename.as_ref().and_then(|field| field.error.clone()) {
        let message = match error {
            RenameError::Invalid => cx.t(Msg::PICKER_RENAME_INVALID).to_string(),
            RenameError::Taken(name) => {
                let mut args = Args::new();
                args.set("name", name.as_str());
                cx.t_args(Msg::PICKER_RENAME_TAKEN, &args).to_string()
            }
        };
        left.push(from_decl(
            Text::new(Id::str("picker/rename/error"), message, caption_size(pack), muted)
                .nowrap()
                .resolve(),
        ));
    }
    // A running job owns the left side of the footer: what it is doing, how far it has
    // gone, and the one item it is on.
    if let Some(running) = state.running.clone() {
        let percent = progress_percent(&running);
        let mut args = Args::new();
        args.set("done", running.done as i64);
        args.set("total", running.total as i64);
        args.set("percent", percent as i64);
        let message = match running.kind {
            ui_platform::OperationKind::Copy => cx.t_args(Msg::PICKER_COPYING, &args).to_string(),
            ui_platform::OperationKind::Move => cx.t_args(Msg::PICKER_MOVING, &args).to_string(),
            ui_platform::OperationKind::Link(_) => {
                cx.t_args(Msg::PICKER_LINKING, &args).to_string()
            }
            ui_platform::OperationKind::Delete(_) => {
                cx.t_args(Msg::PICKER_DELETING, &args).to_string()
            }
            ui_platform::OperationKind::Rename => cx.t(Msg::PICKER_RENAMING).to_string(),
        };
        left.push(from_decl(
            Text::new(
                Id::str("picker/progress/label"),
                message,
                caption_size(pack),
                ink,
            )
            .nowrap()
            .resolve(),
        ));
        if let Some(current) = running.current.as_deref() {
            let name = current
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| current.display().to_string());
            left.push(from_decl(
                Text::new(
                    Id::str("picker/progress/name"),
                    name,
                    caption_size(pack),
                    muted,
                )
                .nowrap()
                .resolve(),
            ));
        }
    }
    let path_error = state.path_entry.as_ref().is_some_and(|entry| entry.error);
    if path_error {
        left.push(from_decl(
            Text::new(
                Id::str("picker/path/error"),
                cx.t(Msg::PICKER_PATH_FAILED).to_string(),
                caption_size(pack),
                muted,
            )
            .nowrap()
            .resolve(),
        ));
    }
    if state.mount_failed {
        left.push(from_decl(
            Text::new(
                Id::str("picker/mount/error"),
                cx.t(Msg::PICKER_MOUNT_FAILED).to_string(),
                caption_size(pack),
                muted,
            )
            .nowrap()
            .resolve(),
        ));
    }
    match mode {
        Mode::SaveFile | Mode::OpenFile | Mode::SelectFolder => left.push(answer_field(state, cx)),
        Mode::SaveFiles => left.push(save_files_caption(state, cx)),
        _ => {}
    }
    if state.model.filters().is_empty() && !path_error {
        // A save with no filters has nothing to offer the type menu; its footer is the name
        // field and the buttons. Everything else says plainly that every name is shown.
        if !mode.is_save() {
            let label = cx.t(Msg::PICKER_FILTER_ALL_FILES).to_string();
            left.push(from_decl(
                Text::new(Id::str("picker/filter"), label, caption_size(pack), muted)
                    .nowrap()
                    .resolve(),
            ));
        }
    }
    let left_slot = element(
        Decl::container(Id::str("picker/footer/text"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::fixed(34.0),
                grow_x: Grow::Yes,
                grow_y: Grow::No,
            })
            .gap(GAP)
            .cross_align(0.0)
            .cross_pos(0.5),
        left,
    );
    let mut actions: Vec<Box<dyn Element>> = Vec::new();
    if state.running.is_some() {
        // Stopping the job is the one action the band adds; it does not replace the
        // dialog's own Cancel button beside it.
        actions.push(icon_button(state, cx, operation_cancel_id(), "close", false));
    }
    if !state.choices.is_empty() {
        // The toggle opens the band above the footer; its pressed state is the band's.
        let active = state.options_open;
        actions.push(icon_button(state, cx, options_id(), "tune", active));
    }
    if !state.model.filters().is_empty() {
        actions.push(filter_chip(state, cx));
    }
    actions.push(cancel);
    actions.push(open);
    let actions_slot = element(
        Decl::container(Id::str("picker/footer/actions"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::hug(0.0, f32::INFINITY),
                height: AxisSize::hug(0.0, f32::INFINITY),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .gap(GAP)
            .cross_align(0.0)
            .cross_pos(0.5),
        actions,
    );
    element(
        Decl::container(Id::str(FOOTER), NodeKind::Row)
            .size(SizeClass::fixed(width, FOOTER_HEIGHT))
            .padding(Insets::new(PADDING, 0.0, PADDING, 0.0))
            .gap(GAP)
            .cross_align(0.0)
            .cross_pos(0.5),
        vec![left_slot, actions_slot],
    )
}

/// The active filter as the type menu's button: its label and the chevron that opens it.
fn filter_chip(state: &PickerState, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let pack = state.pack;
    let id = filter_id();
    let ink = pack.colour_or(CommonColor::OnSurface, Color::WHITE);
    let hover = pack.colour_or(CommonColor::SurfaceHover, Color::from_rgba8(255, 255, 255, 18));
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    let spec = pack.motion.get(MotionName::StateChange).spec;
    let st = cx.state(id);
    let lift = cx.at(id).animate(
        id.with("hover"),
        if st.hovered || st.pressed { 1.0 } else { 0.0 },
        spec,
    );
    let label = state
        .model
        .active_filter()
        .map(|filter| filter.label.clone())
        .unwrap_or_else(|| cx.t(Msg::PICKER_FILTER_ALL_FILES).to_string());
    control(
        cx,
        Decl::container(id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::hug(0.0, f32::INFINITY),
                height: AxisSize::fixed(34.0),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets::new(10.0, 0.0, 8.0, 0.0))
            .gap(6.0)
            .cross_align(0.0)
            .cross_pos(0.5)
            .sense(Sense::HOVER | Sense::CLICK)
            .cursor(CursorShape::PointingHand)
            .background(
                hover.faded(lift),
                Corners::all(radius),
            )
            .child(
                Text::new(id.with("label"), label, caption_size(pack), ink)
                    .max_lines(1)
                    .resolve(),
            )
            .child(
                IconSource::glyph("expand_more", IconSet::Outlined).resolve_with_size(
                    id.with("arrow"),
                    ink,
                    ARROW_SIZE,
                ),
            ),
    )
}

/// The names a `SaveFiles` request writes, as the count the user is choosing a folder for.
fn save_files_caption(state: &PickerState, cx: &Cx<'_, '_>) -> Box<dyn Element> {
    let pack = state.pack;
    let muted = pack.colour_or(CommonColor::TextMuted, Color::from_rgba8(170, 170, 180, 255));
    let count = state.model.save_names().len().to_string();
    let mut args = Args::new();
    args.set("count", count.as_str());
    let text = cx.t_args(Msg::PICKER_SAVE_FILES, &args).to_string();
    from_decl(
        Text::new(Id::str("picker/save-files"), text, caption_size(pack), muted)
            .nowrap()
            .resolve(),
    )
}

/// The files the band above the footer lists, and the one being worked on.
///
/// A running job names its own batch; a `SaveFiles` request names the paths it will write
/// under the folder on screen. Both are sorted the same way, so one batch reads in one order
/// wherever the dialog shows it.
fn job_items(state: &PickerState) -> (Vec<PathBuf>, Option<PathBuf>) {
    if let Some(running) = &state.running {
        if running.items.is_empty() {
            // A job the dialog did not start names only its current file.
            return match &running.current {
                Some(current) => (vec![current.clone()], Some(current.clone())),
                None => (Vec::new(), None),
            };
        }
        return (running.items.clone(), running.current.clone());
    }
    if state.model.mode() == Mode::SaveFiles {
        let folder = state.model.path();
        let mut paths: Vec<PathBuf> = state
            .model
            .save_names()
            .iter()
            .map(|name| folder.join(name))
            .collect();
        crate::sort_paths(&mut paths);
        return (paths, None);
    }
    (Vec::new(), None)
}

/// The band's height for its list: its inset and the cards it shows, capped so a long batch
/// scrolls instead of taking the list's room.
fn job_band_height(count: usize) -> f32 {
    if count == 0 {
        return 0.0;
    }
    let visible = count.min(JOB_VISIBLE_CARDS);
    2.0 * JOB_BAND_PADDING
        + visible as f32 * JOB_CARD_HEIGHT
        + visible.saturating_sub(1) as f32 * JOB_CARD_GAP
}

/// The height a confirmation's file list takes: a few cards, then it scrolls.
fn ask_files_height(count: usize) -> f32 {
    let visible = count.min(JOB_VISIBLE_CARDS);
    visible as f32 * JOB_CARD_HEIGHT + visible.saturating_sub(1) as f32 * JOB_CARD_GAP
}

/// The cards above the footer: one per file, the current one lifted, scrolling inside the
/// band's own height.
fn job_band(
    state: &PickerState,
    cx: &mut Cx<'_, '_>,
    paths: &[PathBuf],
    current: Option<&Path>,
    width: f32,
    height: f32,
) -> Box<dyn Element> {
    path_list(
        state,
        cx,
        job_band_id(),
        paths,
        current,
        width,
        height,
        JOB_BAND_PADDING,
        job_card_id,
    )
}

/// One card per path over a scroller of a fixed height: the band's, or a question's files.
///
/// The rows are one height, so the window steps by their pitch and a thousand names cost
/// the handful of cards the viewport shows.
#[allow(clippy::too_many_arguments)]
fn path_list(
    state: &PickerState,
    cx: &mut Cx<'_, '_>,
    id: Id,
    paths: &[PathBuf],
    current: Option<&Path>,
    width: f32,
    height: f32,
    inset: f32,
    card_id: impl Fn(usize) -> Id,
) -> Box<dyn Element> {
    let window = RowWindow::of(cx.core(), id, paths.len(), JOB_CARD_PITCH * 2.0, |_| {
        JOB_CARD_PITCH
    });
    // The bar floats over the trailing 4 dp, so the card stops short of it.
    let card_width = (width - 2.0 * inset - 12.0).max(40.0);
    let label_width = (card_width - 2.0 * JOB_CARD_INSET).max(20.0);
    let mut cards: Vec<Box<dyn Element>> = Vec::new();
    if window.above > 0.0 {
        cards.push(from_decl(spacer(id.with("above"), card_width, window.above)));
    }
    for index in window.first..window.last {
        let path = &paths[index];
        cards.push(job_card(
            state,
            cx,
            card_id(index),
            path,
            current == Some(path.as_path()),
            card_width,
            label_width,
        ));
    }
    if window.below > 0.0 {
        cards.push(from_decl(spacer(id.with("below"), card_width, window.below)));
    }
    let list = element(
        Decl::container(id.with("list"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fill(0.0, 0.0),
                height: AxisSize::hug(0.0, f32::INFINITY),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            // The inset is content padding, so the bar and the clip stay at the region's
            // edge over the padding rather than on a card.
            .padding(Insets::new(inset, inset, inset, inset))
            .gap(JOB_CARD_GAP),
        cards,
    );
    let scroll = Scroll::fixed(id, width, height).with_scrollbar(ScrollbarConf::on(&state.pack));
    Box::new(scroll.element_in(cx).children(vec![list]))
}

/// One file in a list: its whole path on a card, lifted while the job works on it.
fn job_card(
    state: &PickerState,
    cx: &Cx<'_, '_>,
    id: Id,
    path: &Path,
    current: bool,
    card_width: f32,
    label_width: f32,
) -> Box<dyn Element> {
    let pack = state.pack;
    let ink = pack.colour_or(CommonColor::OnSurface, Color::WHITE);
    let fill = if current {
        pack.colour_or(
            CommonColor::SurfaceSelected,
            Color::from_rgba8(255, 255, 255, 36),
        )
    } else {
        pack.colour_or(
            CommonColor::SurfaceHover,
            Color::from_rgba8(255, 255, 255, 18),
        )
    };
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    from_decl(
        Decl::container(id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::fixed(card_width),
                height: AxisSize::fixed(JOB_CARD_HEIGHT),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets::new(JOB_CARD_INSET, 0.0, JOB_CARD_INSET, 0.0))
            .cross_align(0.0)
            .cross_pos(0.5)
            .background(fill, Corners::all(radius))
            .child(
                Text::new(
                    id.with("path"),
                    path_line(path, cx, pack, label_width),
                    caption_size(pack),
                    ink,
                )
                .max_lines(1)
                .max_width(label_width)
                .resolve(),
            ),
    )
}

/// A path on one card line.
///
/// One too wide for the card loses its middle: the name at the end identifies the item and
/// the deepest folders are how a reader recognises it, so neither end is the one to cut. No
/// measurer means the painter's own end ellipsis is what stands.
fn path_line(path: &Path, cx: &Cx<'_, '_>, pack: &Style, max_width: f32) -> String {
    let full = path.display().to_string();
    let width_of = |body: &str| {
        Text::new(
            Id::str("picker/job/measure"),
            body.to_string(),
            caption_size(pack),
            Color::WHITE,
        )
        .width_in(cx)
    };
    let Some(width) = width_of(&full) else {
        return full;
    };
    if width <= max_width {
        return full;
    }
    let chars: Vec<char> = full.chars().collect();
    let mut tail = 0usize;
    let mut low = 0usize;
    let mut high = chars.len();
    while low < high {
        let mid = (low + high).div_ceil(2);
        let candidate = ellipsized(&chars[chars.len() - mid..]);
        match width_of(&candidate) {
            Some(width) if width <= max_width => {
                low = mid;
                tail = mid;
            }
            _ => high = mid - 1,
        }
    }
    if tail == 0 {
        return full;
    }
    // The tail starts at a separator where one follows, so the ellipsis replaces whole
    // folders instead of the front of a name; cutting only shortens what fits.
    let start = chars.len() - tail;
    let cut = (start..chars.len())
        .find(|&index| is_separator(chars[index]))
        .unwrap_or(start);
    ellipsized(&chars[cut..])
}

fn ellipsized(tail: &[char]) -> String {
    std::iter::once('…').chain(tail.iter().copied()).collect()
}

fn is_separator(glyph: char) -> bool {
    glyph == '\\' || glyph == '/'
}

/// The band above the footer: one control per caller choice, scrolling sideways when the
/// caller brings more than a narrow dialog holds.
///
/// It unfolds upward from the footer rather than appearing at once: `height` is the
/// animation's value, the content keeps its full height and is aligned to the band's bottom,
/// so the band's top edge is what moves while the clip hides the part that has not arrived.
fn choices_band(
    state: &PickerState,
    cx: &mut Cx<'_, '_>,
    width: f32,
    height: f32,
) -> Box<dyn Element> {
    let mut buttons: Vec<Box<dyn Element>> = Vec::new();
    for index in 0..state.choices.len() {
        buttons.push(choice_button(state, cx, index));
    }
    let id = Id::str("picker/choices");
    let scroll = Scroll::horizontal(id, width, CHOICE_BUTTON_HEIGHT)
        .with_scrollbar(ScrollbarConf::never());
    let contents = element(
        Decl::container(id.with("content"), NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::hug(0.0, f32::INFINITY),
                height: AxisSize::fixed(CHOICE_BUTTON_HEIGHT),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            // The inset belongs inside the scroller, so the clip cuts at the band's edge
            // rather than right beside the first and last control.
            .padding(Insets::new(PADDING, 0.0, PADDING, 0.0))
            .cross_align(0.0)
            .cross_pos(0.5),
        buttons,
    );
    let content = element(
        Decl::container(options_band_id().with("content"), NodeKind::Row)
            .size(SizeClass::fixed(width, CHOICES_HEIGHT))
            .cross_align(0.0)
            .cross_pos(0.5),
        vec![Box::new(scroll.element_in(cx).children(vec![contents]))],
    );
    element(
        Decl::container(options_band_id(), NodeKind::Stack)
            .size(SizeClass::fixed(width, height))
            .clip(Clip {
                x: Overflow::Hidden,
                y: Overflow::Hidden,
            })
            .stack_align_axes(OverlayAlign::Start, OverlayAlign::End)
            .opacity(height / CHOICES_HEIGHT),
        vec![content],
    )
}

/// One choice: a combo box's button, or a checkbox that is its own control.
fn choice_button(state: &PickerState, cx: &mut Cx<'_, '_>, index: usize) -> Box<dyn Element> {
    let pack = state.pack;
    let choice = &state.choices[index];
    let value = state
        .choice_values
        .get(index)
        .map(String::as_str)
        .unwrap_or_default();
    let id = choice_button_id(index);
    let ink = pack.colour_or(CommonColor::OnSurface, Color::WHITE);
    let hover = pack.colour_or(CommonColor::SurfaceHover, Color::from_rgba8(255, 255, 255, 18));
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    let spec = pack.motion.get(MotionName::StateChange).spec;
    let st = cx.state(id);
    let lift = cx.at(id).animate(
        id.with("hover"),
        if st.hovered || st.pressed { 1.0 } else { 0.0 },
        spec,
    );
    let (label, glyph) = match &choice.kind {
        ChoiceKind::Select(options) => {
            let current = options
                .iter()
                .find(|option| option.value == value)
                .map(|option| option.label.as_str())
                .unwrap_or(value);
            (format!("{}: {current}", choice.label), "expand_more")
        }
        ChoiceKind::Boolean => (
            choice.label.clone(),
            if value == "true" {
                "check_box"
            } else {
                "check_box_outline_blank"
            },
        ),
    };
    control(
        cx,
        Decl::container(id, NodeKind::Row)
            .size(SizeClass {
                width: AxisSize::hug(0.0, f32::INFINITY),
                height: AxisSize::fixed(CHOICE_BUTTON_HEIGHT),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets::new(10.0, 0.0, 8.0, 0.0))
            .gap(6.0)
            .cross_align(0.0)
            .cross_pos(0.5)
            .sense(Sense::HOVER | Sense::CLICK)
            .cursor(CursorShape::PointingHand)
            .background(
                hover.faded(lift),
                Corners::all(radius),
            )
            .child(
                Text::new(id.with("label"), label, caption_size(pack), ink)
                    .max_lines(1)
                    .resolve(),
            )
            .child(IconSource::glyph(glyph, IconSet::Outlined).resolve_with_size(
                id.with("icon"),
                ink,
                ARROW_SIZE,
            )),
    )
}

/// The type menu: one row per filter, opened from the footer's chip.
fn filter_menu(state: &PickerState, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let active = state.model.active_filter_index();
    let mut rows: Vec<Box<dyn Element>> = Vec::new();
    for (index, filter) in state.model.filters().iter().enumerate() {
        rows.push(menu_option_row(
            state,
            cx,
            filter_item_id(index),
            filter.label.clone(),
            active == Some(index),
            MENU_WIDTH - 2.0 * MENU_PADDING,
        ));
    }
    footer_menu(state.pack, filter_menu_id(), filter_id(), rows, cx)
}

/// The menu under a combo choice: one row per option.
fn choice_menu(state: &PickerState, cx: &mut Cx<'_, '_>, index: usize) -> Box<dyn Element> {
    let value = state
        .choice_values
        .get(index)
        .map(String::as_str)
        .unwrap_or_default();
    let mut rows: Vec<Box<dyn Element>> = Vec::new();
    if let Some(choice) = state.choices.get(index)
        && let ChoiceKind::Select(options) = &choice.kind
    {
        for option in options {
            rows.push(menu_option_row(
                state,
                cx,
                choice_item_id(index, &option.value),
                option.label.clone(),
                option.value == value,
                MENU_WIDTH - 2.0 * MENU_PADDING,
            ));
        }
    }
    footer_menu(state.pack, choice_menu_id(index), choice_button_id(index), rows, cx)
}

/// A menu anchored over its footer button, closed by Escape or a press outside it.
fn footer_menu(
    pack: &Style,
    id: Id,
    anchor: Id,
    rows: Vec<Box<dyn Element>>,
    cx: &mut Cx<'_, '_>,
) -> Box<dyn Element> {
    let surface = pack.opaque_or(
        CommonColor::SurfaceRaised,
        Color::from_rgba8(28, 28, 32, 255),
    );
    let divider = pack.colour_or(CommonColor::Divider, Color::from_rgba8(255, 255, 255, 30));
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    let scroll = Scroll::vertical(id.with("scroll"), MENU_MAX_HEIGHT)
        .with_scrollbar(ScrollbarConf::on(pack));
    let contents = element(
        Decl::container(id.with("content"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(MENU_WIDTH),
                height: AxisSize::hug(0.0, f32::INFINITY),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            // Padding inside the scroller, as in the crumb menu: the bar and the clip stay
            // on the inset, away from the deepest label.
            .padding(Insets::all(MENU_PADDING)),
        rows,
    );
    let panel = element(
        Decl::container(id.with("panel"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(MENU_WIDTH),
                height: AxisSize::hug(0.0, f32::INFINITY),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .background(surface, Corners::all(radius))
            .border(divider, 1.0)
            .elevation(2),
        vec![Box::new(scroll.element_in(cx).children(vec![contents]))],
    );
    let window = Decl::container(id, NodeKind::Box)
        .overlay(OverlayPlacement {
            anchor: Anchor::Node(anchor),
            side: Side::Above,
            align: OverlayAlign::Start,
            offset: 2.0,
            flip: true,
            match_anchor_width: false,
            match_anchor_height: false,
            min_width: 0.0,
            presentation: Presentation::Platform,
        })
        // Focus here is what makes the menu the keyboard scope while it is open: Escape
        // reaches its own chord, and a press outside arrives as the dialog's dismissal.
        .sense(Sense::FOCUSABLE | Sense::HOVER)
        .shortcut(
            KeyChord::new(Key::Escape, Modifiers::NONE),
            FOOTER_MENU_DISMISS,
        );
    Box::new(Dismissible {
        inner: Container::from_decl(window).children(vec![panel]),
        chord: FOOTER_MENU_DISMISS,
    })
}

/// One row of a footer menu: its label and, for the active entry, a check.
fn menu_option_row(
    state: &PickerState,
    cx: &mut Cx<'_, '_>,
    id: Id,
    label: String,
    checked: bool,
    width: f32,
) -> Box<dyn Element> {
    let pack = state.pack;
    let ink = pack.colour_or(CommonColor::OnSurfaceRaised, Color::WHITE);
    let hover = pack.colour_or(CommonColor::SurfaceHover, Color::from_rgba8(255, 255, 255, 18));
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    let spec = pack.motion.get(MotionName::StateChange).spec;
    let st = cx.state(id);
    let lift = cx.at(id).animate(
        id.with("hover"),
        if st.hovered || st.pressed { 1.0 } else { 0.0 },
        spec,
    );
    let fill = hover.faded(lift);
    let inner = width;
    let mut row = Decl::container(id, NodeKind::Row)
        .size(SizeClass::fixed(inner, MENU_ROW_HEIGHT))
        .padding(Insets::new(8.0, 0.0, 8.0, 0.0))
        .gap(GAP)
        .cursor(CursorShape::PointingHand)
        .sense(Sense::HOVER | Sense::CLICK)
        .cross_align(0.0)
        .cross_pos(0.5)
        .background(fill, Corners::all(radius))
        .child(
            Text::new(id.with("label"), label, caption_size(pack), ink)
                .max_lines(1)
                .max_width(inner - 16.0 - ARROW_SIZE - GAP)
                .resolve(),
        );
    if checked {
        row = row.child(
            IconSource::glyph("check", IconSet::Outlined).resolve_with_size(
                id.with("check"),
                ink,
                ARROW_SIZE,
            ),
        );
    }
    control(cx, row)
}

/// The Replace question: one sentence naming the file, and its two answers.
fn overwrite_panel(state: &mut PickerState, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let pack = state.pack;
    let id = overwrite_id();
    let surface = pack.opaque_or(
        CommonColor::SurfaceRaised,
        Color::from_rgba8(28, 28, 32, 255),
    );
    let divider = pack.colour_or(CommonColor::Divider, Color::from_rgba8(255, 255, 255, 30));
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    let ink = pack.colour_or(CommonColor::OnSurfaceRaised, Color::WHITE);
    let name = state
        .overwrite_confirm
        .as_ref()
        .map(|(_, name)| name.clone())
        .unwrap_or_default();
    let mut args = Args::new();
    args.set("name", name.as_str());
    let message = cx.t_args(Msg::PICKER_OVERWRITE, &args).to_string();
    let keep_label = cx.t(Msg::PICKER_CANCEL).to_string();
    let keep = button(cx, pack, overwrite_cancel_id(), keep_label, false);
    let replace_label = cx.t(Msg::PICKER_REPLACE).to_string();
    let replace = button(cx, pack, overwrite_replace_id(), replace_label, false);
    let answers = element(
        Decl::container(id.with("answers"), NodeKind::Row)
            .gap(GAP)
            .cross_align(0.0)
            .cross_pos(0.5),
        vec![keep, replace],
    );
    let question = from_decl(
        Text::new(id.with("label"), message, caption_size(pack), ink)
            .max_lines(3)
            .max_width(ASK_WIDTH - 24.0)
            .resolve(),
    );
    let panel = element(
        Decl::container(id.with("panel"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(ASK_WIDTH),
                height: AxisSize::hug(0.0, f32::INFINITY),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .padding(Insets::all(12.0))
            .gap(10.0)
            .background(surface, Corners::all(radius))
            .border(divider, 1.0)
            .elevation(3),
        vec![question, answers],
    );
    question_modal(pack, id, panel)
}

/// A question over the dialog: the scrim takes every press and two zero-basis growers
/// centre the surface, since a column has no main-axis position of its own.
fn question_modal(pack: &Style, id: Id, panel: Box<dyn Element>) -> Box<dyn Element> {
    let scrim = pack.colour_or(CommonColor::Scrim, Color::from_rgba8(0, 0, 0, 128));
    let root = Decl::container(id, NodeKind::Column)
        .size(SizeClass::hug(0.0, 0.0))
        .modal(ModalTarget::Global)
        .cross_align(0.0)
        .cross_pos(0.5)
        .sense(Sense::CLICK)
        .cursor(CursorShape::Arrow)
        .shapes(vec![Shape::Box {
            corners: Corners::ZERO,
            fill: scrim,
            gradient: None,
            border: None,
            shadow: None,
            image: None,
        }]);
    element(
        root,
        vec![
            from_decl(grower(id.with("pad-top"))),
            panel,
            from_decl(grower(id.with("pad-bottom"))),
        ],
    )
}

/// A zero-basis grower: it takes a share of the spare height and draws nothing.
fn grower(id: Id) -> Decl {
    Decl::container(id, NodeKind::Column).size(SizeClass {
        width: AxisSize::hug(0.0, 0.0),
        height: AxisSize::hug(0.0, 0.0),
        grow_x: Grow::No,
        grow_y: Grow::Yes,
    })
}

/// A read-only window of the platform's own beside the dialog: its own frame, under the
/// toolbar where a fallback popup stands, dismissed by Escape, Enter, its Close button or
/// the window's own close box. It reports, so it does not block the list.
fn info_window(id: Id, title: String, panel: Box<dyn Element>) -> Box<dyn Element> {
    let window = Decl::container(id, NodeKind::Box)
        .overlay(OverlayPlacement {
            anchor: Anchor::Node(Id::str(TOOLBAR)),
            side: Side::Below,
            align: OverlayAlign::Center,
            offset: RULE + 6.0,
            flip: true,
            match_anchor_width: false,
            match_anchor_height: false,
            min_width: 0.0,
            presentation: Presentation::Window,
        })
        .window_title(title)
        .sense(Sense::FOCUSABLE | Sense::HOVER)
        .shortcut(
            KeyChord::new(Key::Escape, Modifiers::NONE),
            FOOTER_MENU_DISMISS,
        )
        // Every fact is already read, so Enter has nothing to confirm: it closes, as the
        // panel's own Dismiss button does.
        .shortcut(
            KeyChord::new(Key::Enter, Modifiers::NONE),
            FOOTER_MENU_DISMISS,
        );
    Box::new(Dismissible {
        inner: Container::from_decl(window).children(vec![panel]),
        chord: FOOTER_MENU_DISMISS,
    })
}

/// The raised body every question draws its rows in, over the dialog's own surface.
fn question_panel(pack: &Style, id: Id, rows: Vec<Box<dyn Element>>) -> Box<dyn Element> {
    panel_body(pack, id, rows, true)
}

/// A reading window's body: the same rows on the window's background, with no card of its
/// own, because the platform has already drawn the frame the card would double.
fn reading_panel(pack: &Style, id: Id, rows: Vec<Box<dyn Element>>) -> Box<dyn Element> {
    panel_body(pack, id, rows, false)
}

fn panel_body(
    pack: &Style,
    id: Id,
    rows: Vec<Box<dyn Element>>,
    card: bool,
) -> Box<dyn Element> {
    let surface = pack.opaque_or(
        CommonColor::SurfaceRaised,
        Color::from_rgba8(28, 28, 32, 255),
    );
    let divider = pack.colour_or(CommonColor::Divider, Color::from_rgba8(255, 255, 255, 30));
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    let mut body = Decl::container(id, NodeKind::Column)
        .size(SizeClass {
            width: AxisSize::fixed(ASK_WIDTH),
            height: AxisSize::hug(0.0, f32::INFINITY),
            grow_x: Grow::No,
            grow_y: Grow::No,
        })
        .padding(Insets::all(12.0))
        .gap(10.0)
        .background(
            surface,
            if card { Corners::all(radius) } else { Corners::ZERO },
        );
    if card {
        body = body.border(divider, 1.0).elevation(3);
    }
    element(body, rows)
}

/// One answer row's wording for the platform's error.
fn operation_error_text(cx: &mut Cx<'_, '_>, error: &ui_platform::OpError) -> String {
    use ui_platform::OpError;
    let msg = match error {
        OpError::NotFound => Msg::PICKER_ERROR_NOT_FOUND,
        OpError::PermissionDenied => Msg::PICKER_ERROR_DENIED,
        OpError::ReadOnly => Msg::PICKER_ERROR_READ_ONLY,
        OpError::AlreadyExists => Msg::PICKER_ERROR_EXISTS,
        OpError::AlreadyThere => Msg::PICKER_ERROR_ALREADY_THERE,
        OpError::CrossDevice => Msg::PICKER_ERROR_CROSS_DEVICE,
        OpError::Recursive => Msg::PICKER_ERROR_RECURSIVE,
        OpError::Unsupported => Msg::PICKER_ERROR_UNSUPPORTED,
        OpError::Io(_) => Msg::PICKER_ERROR_OTHER,
    };
    cx.t(msg).to_string()
}

/// The collision question: what landed on what, one answer per policy, and the field the
/// Rename answer opens.
fn conflict_panel(state: &mut PickerState, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let pack = state.pack;
    let id = conflict_id();
    let ink = pack.colour_or(CommonColor::OnSurfaceRaised, Color::WHITE);
    let Some(ask) = state.conflict.clone() else {
        return element(Decl::container(id, NodeKind::Box), Vec::new());
    };
    let name = ask
        .conflict
        .destination
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| ask.conflict.destination.display().to_string());
    let mut args = Args::new();
    args.set("name", name.as_str());
    let message = cx.t_args(Msg::PICKER_CONFLICT, &args).to_string();
    let question = from_decl(
        Text::new(id.with("label"), message, caption_size(pack), ink)
            .max_lines(3)
            .max_width(ASK_WIDTH - 24.0)
            .resolve(),
    );
    let mut rows: Vec<Box<dyn Element>> = vec![question];
    if let Some(rename) = &ask.rename {
        let field = input_field(
            cx,
            pack,
            conflict_name_id(),
            rename,
            SizeClass::fixed(ASK_WIDTH - 24.0 - 72.0 - GAP, 32.0),
            false,
        );
        let ok = button(
            cx,
            pack,
            conflict_rename_ok_id(),
            cx.t(Msg::PICKER_RENAME).to_string(),
            false,
        );
        rows.push(element(
            Decl::container(id.with("name-row"), NodeKind::Row)
                .gap(GAP)
                .cross_align(0.0)
                .cross_pos(0.5),
            vec![field, ok],
        ));
    } else {
        rows.push(menu_option_row(
            state,
            cx,
            conflict_apply_all_id(),
            cx.t(Msg::PICKER_CONFLICT_APPLY_ALL).to_string(),
            ask.apply_all,
            ASK_WIDTH - 24.0,
        ));
    }
    let answer = |id: Id, msg: Msg, cx: &mut Cx<'_, '_>| {
        button(cx, pack, id, cx.t(msg).to_string(), false)
    };
    let first_row = element(
        Decl::container(id.with("answers"), NodeKind::Row)
            .gap(GAP)
            .cross_align(0.0)
            .cross_pos(0.5),
        vec![
            answer(conflict_replace_id(), Msg::PICKER_CONFLICT_REPLACE, cx),
            answer(conflict_skip_id(), Msg::PICKER_CONFLICT_SKIP, cx),
            answer(conflict_keep_both_id(), Msg::PICKER_CONFLICT_KEEP_BOTH, cx),
        ],
    );
    let second_row = element(
        Decl::container(id.with("answers-2"), NodeKind::Row)
            .gap(GAP)
            .cross_align(0.0)
            .cross_pos(0.5),
        vec![
            answer(conflict_rename_id(), Msg::PICKER_CONFLICT_RENAME, cx),
            answer(conflict_cancel_id(), Msg::PICKER_CANCEL, cx),
        ],
    );
    rows.push(first_row);
    rows.push(second_row);
    question_modal(pack, id, question_panel(pack, id.with("panel"), rows))
}

/// The failures a finished job left, one row per name and reason.
fn failures_panel(state: &mut PickerState, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let pack = state.pack;
    let id = failures_id();
    let ink = pack.colour_or(CommonColor::OnSurfaceRaised, Color::WHITE);
    let Some(failures) = &state.failures else {
        return element(Decl::container(id, NodeKind::Box), Vec::new());
    };
    let mut args = Args::new();
    args.set("count", failures.items.len() as i64);
    let title = cx.t_args(Msg::PICKER_OPERATION_FAILED, &args).to_string();
    let header = from_decl(
        Text::new(id.with("title"), title.clone(), caption_size(pack), ink)
            .max_lines(2)
            .max_width(ASK_WIDTH - 24.0)
            .resolve(),
    );
    let mut items: Vec<Box<dyn Element>> = Vec::new();
    for (index, (path, error)) in failures.items.iter().enumerate() {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let reason = operation_error_text(cx, error);
        let mut args = Args::new();
        args.set("name", name.as_str());
        args.set("reason", reason.as_str());
        items.push(from_decl(
            Text::new(
                id.with("item").with(index),
                cx.t_args(Msg::PICKER_FAILURE_ITEM, &args).to_string(),
                caption_size(pack),
                ink,
            )
            .max_lines(2)
            .max_width(ASK_WIDTH - 24.0)
            .resolve(),
        ));
    }
    // A job can fail on many names: the window stays small and the list scrolls, as the
    // folder chooser's does.
    let list = element(
        Decl::container(id.with("list"), NodeKind::Column)
            .size(SizeClass::hug(ASK_WIDTH - 24.0, 0.0))
            .gap(2.0),
        items,
    );
    let scroll = Scroll::vertical(id.with("scroll"), MENU_MAX_HEIGHT)
        .with_scrollbar(ScrollbarConf::on(pack));
    let rows = vec![
        header,
        Box::new(scroll.element_in(cx).children(vec![list])),
        button(
            cx,
            pack,
            failures_close_id(),
            cx.t(Msg::PICKER_DISMISS).to_string(),
            false,
        ),
    ];
    info_window(id, title, reading_panel(pack, id.with("panel"), rows))
}

/// The permanent delete's confirmation: the one question a delete asks.
fn delete_panel(state: &mut PickerState, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let pack = state.pack;
    let id = delete_ask_id();
    let ink = pack.colour_or(CommonColor::OnSurfaceRaised, Color::WHITE);
    let Some(paths) = &state.delete_ask else {
        return element(Decl::container(id, NodeKind::Box), Vec::new());
    };
    let mut args = Args::new();
    args.set("count", paths.len() as i64);
    let message = cx.t_args(Msg::PICKER_DELETE_CONFIRM, &args).to_string();
    let question = from_decl(
        Text::new(id.with("label"), message, caption_size(pack), ink)
            .max_lines(3)
            .max_width(ASK_WIDTH - 24.0)
            .resolve(),
    );
    // The sentence counts them; the cards say which ones, so a delete nothing can undo is
    // read before it is answered.
    let mut sorted = paths.clone();
    crate::sort_paths(&mut sorted);
    let files = path_list(
        state,
        cx,
        id.with("files"),
        &sorted,
        None,
        ASK_WIDTH - 24.0,
        ask_files_height(sorted.len()),
        0.0,
        |index| asking_card_id(id, index),
    );
    let answers = element(
        Decl::container(id.with("answers"), NodeKind::Row)
            .gap(GAP)
            .cross_align(0.0)
            .cross_pos(0.5),
        vec![
            button(
                cx,
                pack,
                delete_cancel_id(),
                cx.t(Msg::PICKER_CANCEL).to_string(),
                false,
            ),
            button(
                cx,
                pack,
                delete_confirm_id(),
                cx.t(Msg::PICKER_DELETE).to_string(),
                false,
            ),
        ],
    );
    question_modal(
        pack,
        id,
        question_panel(pack, id.with("panel"), vec![question, files, answers]),
    )
}

/// The question an Undo asks before it removes what a copy made.
fn undo_panel(state: &mut PickerState, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let pack = state.pack;
    let id = undo_ask_id();
    let ink = pack.colour_or(CommonColor::OnSurfaceRaised, Color::WHITE);
    let Some(paths) = &state.undo_ask else {
        return element(Decl::container(id, NodeKind::Box), Vec::new());
    };
    let mut args = Args::new();
    args.set("count", paths.len() as i64);
    let message = cx.t_args(Msg::PICKER_UNDO_COPY_CONFIRM, &args).to_string();
    let question = from_decl(
        Text::new(id.with("label"), message, caption_size(pack), ink)
            .max_lines(3)
            .max_width(ASK_WIDTH - 24.0)
            .resolve(),
    );
    let mut sorted = paths.clone();
    crate::sort_paths(&mut sorted);
    let files = path_list(
        state,
        cx,
        id.with("files"),
        &sorted,
        None,
        ASK_WIDTH - 24.0,
        ask_files_height(sorted.len()),
        0.0,
        |index| asking_card_id(id, index),
    );
    let answers = element(
        Decl::container(id.with("answers"), NodeKind::Row)
            .gap(GAP)
            .cross_align(0.0)
            .cross_pos(0.5),
        vec![
            button(
                cx,
                pack,
                undo_cancel_id(),
                cx.t(Msg::PICKER_CANCEL).to_string(),
                false,
            ),
            button(
                cx,
                pack,
                undo_confirm_id(),
                cx.t(Msg::PICKER_UNDO).to_string(),
                false,
            ),
        ],
    );
    question_modal(
        pack,
        id,
        question_panel(pack, id.with("panel"), vec![question, files, answers]),
    )
}

/// The selection's facts: what it is, where it is, how big, and when it changed.
fn properties_panel(state: &mut PickerState, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let pack = state.pack;
    let id = properties_id();
    let ink = pack.colour_or(CommonColor::OnSurfaceRaised, Color::WHITE);
    let Some(properties) = &state.properties else {
        return element(Decl::container(id, NodeKind::Box), Vec::new());
    };
    let mut rows: Vec<Box<dyn Element>> = Vec::new();
    let heading = match &properties.name {
        Some(name) => name.clone(),
        None => {
            let mut args = Args::new();
            args.set("count", properties.count as i64);
            cx.t_args(Msg::PICKER_PROPERTIES_ITEMS, &args).to_string()
        }
    };
    rows.push(from_decl(
        Text::new(id.with("name"), heading, caption_size(pack), ink)
            .max_lines(2)
            .max_width(ASK_WIDTH - 24.0)
            .resolve(),
    ));
    // A mixed selection has no one type; only a single item states one.
    if properties.name.is_some() {
        let kind = if properties.folder {
            cx.t(Msg::PICKER_PROPERTIES_TYPE_FOLDER).to_string()
        } else if let Some(extension) = &properties.extension {
            let mut args = Args::new();
            args.set("ext", extension.as_str());
            cx.t_args(Msg::PICKER_PROPERTIES_TYPE_EXTENSION, &args)
                .to_string()
        } else {
            cx.t(Msg::PICKER_PROPERTIES_TYPE_FILE).to_string()
        };
        rows.push(fact_row(
            pack,
            id.with("type"),
            cx.t(Msg::PICKER_PROPERTIES_TYPE).to_string(),
            kind,
        ));
    }
    rows.push(fact_row(
        pack,
        id.with("location"),
        cx.t(Msg::PICKER_PROPERTIES_LOCATION).to_string(),
        properties.location.display().to_string(),
    ));
    if let Some(size) = properties.size {
        rows.push(fact_row(
            pack,
            id.with("size"),
            cx.t(Msg::PICKER_PROPERTIES_SIZE).to_string(),
            format_size(size),
        ));
    }
    if properties.modified.is_some() {
        rows.push(fact_row(
            pack,
            id.with("modified"),
            cx.t(Msg::PICKER_PROPERTIES_MODIFIED).to_string(),
            modified_label(cx, properties.modified),
        ));
    }
    rows.push(button(
        cx,
        pack,
        properties_close_id(),
        cx.t(Msg::PICKER_DISMISS).to_string(),
        false,
    ));
    let title = cx.t(Msg::PICKER_PROPERTIES).to_string();
    info_window(id, title, reading_panel(pack, id.with("panel"), rows))
}

/// The folder chooser a Move to / Copy to command opened.
///
/// It browses with the dialog's own listing service rather than a second picker: the rows
/// are the same subfolder read the crumbs use. The shortcuts and volumes outside the folder
/// being browsed are its way to another root, which Up alone cannot reach.
fn transfer_panel(state: &mut PickerState, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let pack = state.pack;
    let id = transfer_id();
    let muted = pack.colour_or(CommonColor::TextMuted, Color::from_rgba8(160, 160, 170, 255));
    let Some(ask) = &state.transfer else {
        return element(Decl::container(id, NodeKind::Box), Vec::new());
    };
    let folder = ask.folder.clone();
    let effect = ask.effect;
    let inner = ASK_WIDTH - 24.0;
    // The path is the question's subject: "Here" in the answer row is this folder.
    let location = from_decl(
        Text::new(
            id.with("folder"),
            folder.display().to_string(),
            caption_size(pack),
            muted,
        )
        .max_lines(2)
        .max_width(inner)
        .resolve(),
    );
    let mut rows: Vec<Box<dyn Element>> = Vec::new();
    if folder.parent().is_some() {
        let up = cx.t(Msg::PICKER_UP).to_string();
        rows.push(jump_row(
            state,
            cx,
            transfer_up_id(),
            icons::file_mark(FileKind::Folder),
            &up,
            inner,
            MENU_ROW_HEIGHT,
        ));
    }
    // A shortcut that names a folder inside this one is already a row of the listing, so
    // only the ones leading elsewhere are drawn.
    let outside = |path: &Path| path != folder && !path.starts_with(&folder);
    let mut jumps: Vec<(Id, IconSource, String)> = Vec::new();
    for place in &state.places {
        if outside(&place.path) {
            let label = cx.t(icons::place_label(place.kind)).to_string();
            jumps.push((
                transfer_place_id(&place.path),
                icons::place_mark(place.kind),
                label,
            ));
        }
    }
    for place in &state.extra_places {
        if outside(&place.path) {
            jumps.push((
                transfer_extra_place_id(&place.path),
                icons::file_mark(FileKind::Folder),
                place.label.clone(),
            ));
        }
    }
    for device in &state.devices {
        if outside(&device.path) {
            let label = device
                .label
                .clone()
                .unwrap_or_else(|| cx.t(Msg::PICKER_DEVICE_ROOT).to_string());
            jumps.push((transfer_device_id(&device.path), icons::device_mark(), label));
        }
    }
    for (jump, mark, label) in jumps {
        rows.push(jump_row(state, cx, jump, mark, &label, inner, MENU_ROW_HEIGHT));
    }
    match state.model.subfolders(&folder) {
        Subfolders::Loading => rows.push(message_row(
            state,
            cx,
            id.with("message"),
            Msg::PICKER_LOADING,
            inner,
            MENU_ROW_HEIGHT,
        )),
        Subfolders::Failed => rows.push(message_row(
            state,
            cx,
            id.with("message"),
            Msg::PICKER_FAILED,
            inner,
            MENU_ROW_HEIGHT,
        )),
        Subfolders::Ready(folders) if folders.is_empty() => rows.push(message_row(
            state,
            cx,
            id.with("message"),
            Msg::PICKER_EMPTY,
            inner,
            MENU_ROW_HEIGHT,
        )),
        Subfolders::Ready(folders) => {
            for entry in folders {
                rows.push(jump_row(
                    state,
                    cx,
                    transfer_folder_id(&entry.path),
                    icons::file_mark(FileKind::Folder),
                    &entry.name,
                    inner,
                    MENU_ROW_HEIGHT,
                ));
            }
        }
    }
    let list = element(
        Decl::container(id.with("list"), NodeKind::Column)
            .size(SizeClass {
                width: AxisSize::fixed(inner),
                height: AxisSize::hug(0.0, f32::INFINITY),
                grow_x: Grow::No,
                grow_y: Grow::No,
            })
            .gap(2.0),
        rows,
    );
    let scroll = Scroll::vertical(id.with("scroll"), MENU_MAX_HEIGHT)
        .with_scrollbar(ScrollbarConf::on(pack));
    let accept = match effect {
        DropEffect::Move => Msg::PICKER_MOVE_HERE,
        DropEffect::Copy => Msg::PICKER_COPY_HERE,
        // The chooser is only opened for a move or a copy; a link names its own row.
        DropEffect::Link => Msg::PICKER_LINK_HERE,
    };
    let answers = element(
        Decl::container(id.with("answers"), NodeKind::Row)
            .gap(GAP)
            .cross_align(0.0)
            .cross_pos(0.5),
        vec![
            button(
                cx,
                pack,
                transfer_cancel_id(),
                cx.t(Msg::PICKER_CANCEL).to_string(),
                false,
            ),
            button(
                cx,
                pack,
                transfer_accept_id(),
                cx.t(accept).to_string(),
                false,
            ),
        ],
    );
    question_modal(
        pack,
        id,
        question_panel(
            pack,
            id.with("panel"),
            vec![
                location,
                Box::new(scroll.element_in(cx).children(vec![list])),
                answers,
            ],
        ),
    )
}

/// One fact: a muted name and the value beside it.
fn fact_row(pack: &Style, id: Id, label: String, value: String) -> Box<dyn Element> {
    const LABEL_WIDTH: f32 = 84.0;
    let ink = pack.colour_or(CommonColor::OnSurfaceRaised, Color::WHITE);
    let muted = pack.colour_or(CommonColor::TextMuted, Color::from_rgba8(160, 160, 170, 255));
    let size = caption_size(pack);
    element(
        Decl::container(id, NodeKind::Row).gap(GAP).cross_align(0.0),
        vec![
            from_decl(
                Text::new(id.with("label"), label, size, muted)
                    .max_width(LABEL_WIDTH)
                    .resolve(),
            ),
            from_decl(
                Text::new(id.with("value"), value, size, ink)
                    .max_lines(3)
                    .max_width(ASK_WIDTH - 24.0 - LABEL_WIDTH - GAP)
                    .resolve(),
            ),
        ],
    )
}

/// The answer a single mode would return, editable: a name for a save, a path for the modes
/// that answer with one. The selection writes it, and the user may type or paste over it.
///
/// The buffer lives on the container, so the field is the container and the text is one of
/// its views; the engine reports every edit through [`Change::Text`](ui_core::change::Change)
/// on the field's id, and the model keeps what it reports.
fn answer_field(state: &mut PickerState, cx: &mut Cx<'_, '_>) -> Box<dyn Element> {
    let value = state.model.answer().to_string();
    let size = SizeClass {
        width: AxisSize::fill(0.0, 0.0),
        height: AxisSize::fixed(34.0),
        grow_x: Grow::Yes,
        grow_y: Grow::No,
    };
    input_field(cx, state.pack, answer_id(), &value, size, false)
}

/// The address or search field, in the toolbar's one filled slot.
fn toolbar_field(
    state: &mut PickerState,
    cx: &mut Cx<'_, '_>,
    id: Id,
    value: &str,
    width: f32,
    dismisses: bool,
) -> Box<dyn Element> {
    let height = CRUMB_HEIGHT + 2.0 * CRUMB_INSET;
    input_field(
        cx,
        state.pack,
        id,
        value,
        SizeClass::fixed(width, height),
        dismisses,
    )
}

/// The name slot of the row being renamed: an editor where the label was.
fn rename_cell(state: &mut PickerState, cx: &mut Cx<'_, '_>, width: f32) -> Decl {
    let value = state
        .rename
        .as_ref()
        .map(|field| field.buffer.clone())
        .unwrap_or_default();
    let height = (ROW_HEIGHT - 6.0).max(18.0);
    input_decl(
        cx,
        state.pack,
        rename_id(),
        &value,
        SizeClass::fixed(width.max(80.0), height),
        true,
    )
}

/// One editable line. The engine owns the caret, the selection and the composition; the
/// line is `ui_widgets::text::LineEdit`, which also scrolls a long value under the caret.
///
/// Enter is a node shortcut because a focused caret owns the plain key; Escape joins it for
/// the fields that are a scope of their own, so the dialog is not closed behind them.
fn input_decl(
    cx: &mut Cx<'_, '_>,
    pack: &'static Style,
    id: Id,
    value: &str,
    size: SizeClass,
    dismisses: bool,
) -> Decl {
    let ink = pack.colour_or(CommonColor::OnSurface, Color::WHITE);
    let field = pack.colour_or(CommonColor::Surface, Color::from_rgba8(20, 20, 24, 255));
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    let caret = cx.core().caret_for(id);
    let preedit = cx.core().preedit_for(id);
    let focused = cx.state(id).focused;
    let painted = cx
        .core()
        .selection_for(id)
        .filter(|selection| !selection.is_collapsed())
        .map(|selection| ui_core::SelectionPrim {
            range: selection.range(),
            fill: pack
                .ink(ui_widgets::text::roles::SELECTION)
                .unwrap_or(Color::TRANSPARENT),
            painter: pack.selection,
        });
    // A field that fills its parent has no width before layout; the rect its last frame left
    // is the field's own width, and the surface bounds it until there is one.
    let total = if size.width.hint > 0.0 {
        size.width.hint
    } else {
        cx.rect_of(id)
            .map_or(cx.core().width() as f32, |rect| rect.size.w)
    };
    let insets = FIELD_INSET;
    let line = ui_widgets::text::LineEdit {
        id,
        text: value,
        caret,
        preedit: preedit.as_ref(),
        painted,
        placeholder: "",
        obscure: false,
        hint_always: false,
        focused,
        size: caption_size(pack),
        weight: Weight::REGULAR,
        ink,
        placeholder_ink: ink,
        caret_ink: ink,
        caret_width: CARET_WIDTH,
        slack: FIELD_SCROLL_SLACK,
        width: (total - 2.0 * insets).max(0.0),
        left_inset: insets,
        right_inset: insets,
    }
    .resolve(cx);
    let mut decl = Decl::container(id, NodeKind::Row)
        .size(size)
        .cursor(CursorShape::Text)
        .sense(Sense::FOCUSABLE | Sense::CLICK)
        .cross_align(0.0)
        .cross_pos(0.5)
        .background(field, Corners::all(radius))
        .clip(Clip {
            x: Overflow::Hidden,
            y: Overflow::Visible,
        })
        .text_input(value.to_string())
        .shortcut(KeyChord::new(Key::Enter, Modifiers::NONE), crate::OPEN)
        .child(line);
    if dismisses {
        decl = decl.shortcut(KeyChord::new(Key::Escape, Modifiers::NONE), crate::CANCEL);
    }
    decl
}

/// An input as an element: the same declaration, with the press handling a standalone
/// control has.
fn input_field(
    cx: &mut Cx<'_, '_>,
    pack: &'static Style,
    id: Id,
    value: &str,
    size: SizeClass,
    dismisses: bool,
) -> Box<dyn Element> {
    let decl = input_decl(cx, pack, id, value, size, dismisses);
    control(cx, decl)
}

/// One pack button, resolved with the hover fade its own node records.
///
/// `disabled` is this build's answer, not the node's bit from last frame: a selection landing
/// in the same turn enables the accept button, and resolving from the stale bit would keep it
/// disabled-coloured until an unrelated frame. The resolved declaration paints the disabled
/// look itself, so it tells the adapter it must not be faded a second time.
fn button(
    cx: &mut Cx<'_, '_>,
    pack: &Style,
    id: Id,
    label: String,
    disabled: bool,
) -> Box<dyn Element> {
    let mut st = cx.state(id);
    st.disabled = disabled;
    let spec = pack.motion.get(MotionName::StateChange).spec;
    let lift = cx.at(id).animate(
        id.with("hover"),
        if st.hovered || st.pressed { 1.0 } else { 0.0 },
        spec,
    );
    let decl = Button::new(id, label)
        .resolve(st, pack, lift)
        .disabled(disabled);
    cx.element(Box::new(Pressable::new(decl).painted_disabled()))
}

/// A square icon button: padding around a glyph, so the clickable area is the whole square.
fn icon_button(
    state: &mut PickerState,
    cx: &mut Cx<'_, '_>,
    id: Id,
    glyph: &str,
    active: bool,
) -> Box<dyn Element> {
    let pack = state.pack;
    let ink = pack.colour_or(CommonColor::OnSurfaceRaised, Color::WHITE);
    let radius = pack.metric_or(CommonMetric::Radius, 6.0);
    let hover = pack.colour_or(CommonColor::SurfaceHover, Color::from_rgba8(255, 255, 255, 18));
    let selected = pack.colour_or(
        CommonColor::SurfaceSelected,
        Color::from_rgba8(255, 255, 255, 36),
    );
    let spec = pack.motion.get(MotionName::StateChange).spec;
    let st = cx.state(id);
    let lift = cx.at(id).animate(
        id.with("hover"),
        if st.hovered || st.pressed { 1.0 } else { 0.0 },
        spec,
    );
    let base = if active { selected } else { Color::TRANSPARENT };
    control(
        cx,
        Decl::container(id, NodeKind::Row)
            .padding(Insets::all(6.0))
            .sense(Sense::HOVER | Sense::CLICK)
            .cursor(CursorShape::PointingHand)
            // The hover is a layer over the active fill, not a replacement for it: a state
            // layer keeps the button's own colour visible underneath.
            .background(hover.faded(lift).over(base), Corners::all(radius))
            .child(
                ui_widgets::IconSource::glyph(glyph, ui_widgets::IconSet::Outlined).resolve_with_size(
                    id.with("icon"),
                    ink,
                    MARK_SIZE,
                ),
            ),
    )
}

/// The width the name column takes beside the two fixed ones.
fn column_width(width: f32, size: f32, modified: f32) -> f32 {
    (width - 2.0 * COLUMN_INSET - MARK_SIZE - 3.0 * GAP - size - modified).max(NAME_MIN_COLUMN)
}

/// The narrowest the list's content may be: the columns at their floors.
fn list_min_width() -> f32 {
    2.0 * COLUMN_INSET + MARK_SIZE + 3.0 * GAP + NAME_MIN_COLUMN + SIZE_COLUMN + MODIFIED_COLUMN
}

fn caption_size(pack: &Style) -> f32 {
    pack.type_table.get(ui_style::TextRole::Caption).size
}

/// A byte count in the binary units a file manager shows.
fn format_size(len: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if len < 1024 {
        return format!("{len} B");
    }
    let mut value = len as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// A file's modification date in the catalogue's locale, at whole days.
///
/// The engine's `Duration` is since the Unix epoch; a picker shows the day, not the second,
/// and the catalogue is the only place that knows how this language writes a date.
fn modified_label(cx: &Cx<'_, '_>, modified: Option<Duration>) -> String {
    let Some(modified) = modified else {
        return String::new();
    };
    let days = (modified.as_secs() / 86_400) as i64;
    let (year, month, day) = civil_from_days(days);
    match cx.date(year, month, day) {
        Some(text) => text.to_string(),
        None => format!("{year:04}-{month:02}-{day:02}"),
    }
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i32, u8, u8) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    (year as i32, month as u8, day as u8)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use ui_core::change::Change;
    use ui_core::geom::Rect;
    use ui_platform::FileKind;
    use ui_platform::chooser::{ChoiceOption, ChooserChoice, FileTypeFilter};
    use ui_platform::listing::{ListingRequest, ListingState};
    use ui_platform::places::{Place, PlaceKind};
    use ui_platform::{App, AppLoop, OffscreenTarget};
    use ui_widgets::WidgetState;

    use super::*;
    use crate::{ListingSource, PickerOptions, PickerOutcome};

    /// One directory with one file, answered as soon as it is asked.
    struct OneFile(PathBuf);

    impl ListingSource for OneFile {
        fn get(&self, _request: &ListingRequest) -> ListingState {
            ListingState::Ready(Arc::new(vec![Entry {
                name: "photo.png".to_string(),
                path: self.0.join("photo.png"),
                kind: FileKind::Generic,
                directory: false,
                link: false,
                len: 2048,
                modified: None,
                hidden: false,
            }]))
        }

        fn invalidate(&self, _path: &Path) {}
    }

    /// A folder of files, answered as soon as it is asked.
    struct Many(Vec<Entry>);

    impl ListingSource for Many {
        fn get(&self, _request: &ListingRequest) -> ListingState {
            ListingState::Ready(Arc::new(self.0.clone()))
        }

        fn invalidate(&self, _path: &Path) {}
    }

    /// A trail of folders three deep, answered as soon as it is asked.
    struct Folders;

    impl Folders {
        fn folder(name: &str, path: &str) -> Entry {
            Entry {
                name: name.to_string(),
                path: PathBuf::from(path),
                kind: FileKind::Folder,
                directory: true,
                link: false,
                len: 0,
                modified: None,
                hidden: false,
            }
        }
    }

    impl ListingSource for Folders {
        fn get(&self, request: &ListingRequest) -> ListingState {
            let entries = match request.path.to_str() {
                Some("/") => vec![Folders::folder("a", "/a")],
                Some("/a") => vec![Folders::folder("b", "/a/b")],
                Some("/a/b") => vec![Folders::folder("c", "/a/b/c")],
                _ => Vec::new(),
            };
            ListingState::Ready(Arc::new(entries))
        }

        fn invalidate(&self, _path: &Path) {}
    }

    /// The picker's element that owns `id`, with the change it reports for a click.
    fn click(element: &dyn Element, decl: &Decl, id: Id) -> Option<Option<Change>> {
        if decl.id == id {
            let clicked = WidgetState {
                clicked: true,
                ..WidgetState::default()
            };
            return Some(element.change(&clicked, Rect::ZERO));
        }
        for (child, child_decl) in element.children().iter().zip(&decl.children) {
            if let Some(report) = click(child.as_ref(), child_decl, id) {
                return Some(report);
            }
        }
        None
    }

    /// Click `id` in a freshly built tree and hand the reported change to the state.
    fn press(
        state: &mut PickerState,
        cx: &mut ui_core::cx::Cx<'_>,
        id: Id,
    ) -> Option<PickerOutcome> {
        let root = state.view(cx, 900.0, 600.0);
        let decl = root.decl(cx);
        let change = click(root.as_ref(), &decl, id)
            .unwrap_or_else(|| panic!("the picker declares {id:?}"))
            .unwrap_or_else(|| panic!("{id:?} reports a change on a click"));
        state.changed(id, change)
    }

    #[test]
    fn every_clickable_region_the_picker_builds_reports_its_own_click() {
        // The bug this pins: the whole tree was declarations, so pointer clicks reached no
        // `change()` and the dialog answered the keyboard's shortcuts only. The regions are
        // spread over the toolbar, the sidebar, the list and the footer, because one path
        // being rebuilt as bare declarations is exactly how the bug comes back.
        let start = PathBuf::from("/picker-fixture");
        let places = vec![Place {
            kind: PlaceKind::Home,
            path: start.clone(),
        }];
        let device = Device {
            path: PathBuf::from("/picker-fixture/volume"),
            label: Some("Volume".to_string()),
        };
        let pack = ui_widgets_md3::pack_of;
        let options = PickerOptions::new(
            OneFile(start.clone()),
            start,
            pack,
            ui_widgets_md3::menu_style_of,
            ui_style::Env::root(),
        )
        .places(places.clone())
        .devices(vec![device.clone()])
        .filters(vec![FileTypeFilter::all_files()])
        .choices(vec![
            ChooserChoice::boolean("hidden", "Hidden", false),
            ChooserChoice::select(
                "depth",
                "Depth",
                vec![ChoiceOption::new("one", "One")],
                "one",
            ),
        ]);
        let mut state = PickerState::new(options);
        let mut store = ui_core::AnimStore::new();
        let mut core = ui_core::cx::Cx::new(&mut store, 0, 900, 600, 0.0, 0.0, 1.0, None);
        let root = state.view(&mut core, 900.0, 600.0);
        let decl = root.decl(&mut core);

        let entry = state.model.rows()[0].clone();
        let targets = [
            ("the view toggle", view_id()),
            ("the hidden-files toggle", hidden_id()),
            ("the up button", up_id()),
            ("a breadcrumb", crumb_id(Path::new("/"))),
            ("a breadcrumb arrow", crumb_arrow_id(Path::new("/"))),
            ("a place", place_id(&places[0])),
            ("a device", device_id(&device)),
            ("the type menu button", filter_id()),
            ("the options toggle", options_id()),
            ("a checkbox choice", choice_button_id(0)),
            ("a select choice", choice_button_id(1)),
            ("cancel", cancel_id()),
        ];
        for (what, id) in targets {
            let report = click(root.as_ref(), &decl, id)
                .unwrap_or_else(|| panic!("{what}: the picker declares this control"));
            assert!(
                matches!(report, Some(Change::Activated)),
                "{what}: a click must reach the app as an activation, not silence"
            );
        }
        // A row is the one region whose click is not an activation: its answer carries the
        // modifiers the click was made with, so replace, toggle and extend stay distinct.
        let report = click(root.as_ref(), &decl, row_id(&entry))
            .expect("a row: the picker declares this control")
            .expect("a row: a click must reach the app, not silence");
        assert!(
            matches!(report, Change::Data(_)),
            "a row reports the gesture that carries its click's modifiers"
        );
    }

    /// The toolbar's view button swaps the rows for a grid. The cells are still the same
    /// clickable rows, and a vertical arrow moves a whole row of cells: the view changed what
    /// one screen row is, not what an entry is.
    #[test]
    fn the_view_toggle_builds_a_grid_and_arrows_step_by_its_columns() {
        let start = PathBuf::from("/picker-fixture");
        let entries: Vec<Entry> = (0..20)
            .map(|index| Entry {
                name: format!("{index}.png"),
                path: start.join(format!("{index}.png")),
                kind: FileKind::Generic,
                directory: false,
                link: false,
                len: 1,
                modified: None,
                hidden: false,
            })
            .collect();
        let pack = ui_widgets_md3::pack_of;
        let options = PickerOptions::new(
            Many(entries.clone()),
            start,
            pack,
            ui_widgets_md3::menu_style_of,
            ui_style::Env::root(),
        );
        let mut state = PickerState::new(options);
        let mut store = ui_core::AnimStore::new();
        let mut core = ui_core::cx::Cx::new(&mut store, 0, 900, 600, 0.0, 0.0, 1.0, None);

        press(&mut state, &mut core, view_id());
        assert_eq!(state.layout, crate::Layout::Grid);

        let root = state.view(&mut core, 900.0, 600.0);
        let columns = state.columns;
        assert!(columns > 1, "a 900-unit dialog holds more than one cell");
        let decl = root.decl(&mut core);
        let report = click(root.as_ref(), &decl, grid_cell_id(&entries[0]))
            .expect("the grid declares every cell")
            .expect("a cell must answer its click");
        assert!(
            matches!(report, Change::Data(_)),
            "a cell reports the gesture that carries its click's modifiers"
        );
        // A cell that reused the row's id corrupted the list's height table: the table
        // learns a row's height by reading the rect the id left in the previous frame, and
        // switching back to the list read a 136-tall cell as a row and left most of the
        // list unbuilt.
        assert!(
            click(root.as_ref(), &decl, row_id(&entries[0])).is_none(),
            "a grid cell must not stand on the list row's node id"
        );

        // One row down from the first cell is the cell a whole screen row away.
        state.model.row_clicked(
            0,
            Modifiers::NONE,
            crate::model::RangeShape::Column,
            false,
        );
        state.shortcut(crate::NEXT, Modifiers::NONE);
        assert_eq!(state.model.cursor, Some(columns));
    }

    /// The type menu and a choice's option are not just painted: their clicks reach the
    /// state, and the answer names both. The bug this pins is a band whose ids are built but
    /// never handled — the menu opens and a row in it answers nothing.
    #[test]
    fn the_type_menu_and_every_choice_reach_the_answer() {
        let start = PathBuf::from("/picker-fixture");
        let pack = ui_widgets_md3::pack_of;
        let options = PickerOptions::new(
            OneFile(start.clone()),
            start.clone(),
            pack,
            ui_widgets_md3::menu_style_of,
            ui_style::Env::root(),
        )
        .filters(vec![
            FileTypeFilter::all_files(),
            FileTypeFilter::new("Images", vec!["*.png".to_string()]),
        ])
        .choices(vec![
            ChooserChoice::boolean("hidden", "Hidden", false),
            ChooserChoice::select(
                "depth",
                "Depth",
                vec![
                    ChoiceOption::new("one", "One"),
                    ChoiceOption::new("all", "All"),
                ],
                "one",
            ),
        ]);
        let mut state = PickerState::new(options);
        let mut store = ui_core::AnimStore::new();
        let mut core = ui_core::cx::Cx::new(&mut store, 0, 900, 600, 0.0, 0.0, 1.0, None);
        state.view(&mut core, 900.0, 600.0);
        let entry = state.model.rows()[0].clone();

        press(&mut state, &mut core, row_id(&entry));
        press(&mut state, &mut core, filter_id());
        press(&mut state, &mut core, filter_item_id(1));
        press(&mut state, &mut core, choice_button_id(0));
        press(&mut state, &mut core, choice_button_id(1));
        press(&mut state, &mut core, choice_item_id(1, "all"));

        let outcome = press(&mut state, &mut core, open_id()).expect("Open answers the dialog");
        let PickerOutcome::Selected(selection) = outcome else {
            panic!("Open must select the row, not cancel");
        };
        assert_eq!(selection.paths, vec![start.join("photo.png")]);
        assert_eq!(selection.filter, Some(1), "the menu's pick is the answer's filter");
        assert_eq!(
            selection.choices,
            vec![
                ("hidden".to_string(), "true".to_string()),
                ("depth".to_string(), "all".to_string()),
            ],
            "a checkbox's toggle and a menu's pick are both in the answer"
        );
    }

    /// An open names an existing file at once: the overwrite policy is the save dialog's
    /// question, and asking it here would make every open a two-step replace.
    #[test]
    fn an_open_answers_a_taken_name_without_asking_to_replace_it() {
        let dir = std::env::temp_dir().join("crabsnap-picker-open-fixture");
        // The test owns this name, so a crashed earlier run's leftover is cleared, not merged.
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("clear a stale fixture directory");
        }
        std::fs::create_dir_all(&dir).expect("a temporary directory for the fixture");
        std::fs::write(dir.join("photo.png"), b"x").expect("a file to open");
        let pack = ui_widgets_md3::pack_of;
        let options = PickerOptions::new(
            OneFile(dir.clone()),
            dir.clone(),
            pack,
            ui_widgets_md3::menu_style_of,
            ui_style::Env::root(),
        );
        let mut state = PickerState::new(options);
        let mut store = ui_core::AnimStore::new();
        let mut core = ui_core::cx::Cx::new(&mut store, 0, 900, 600, 0.0, 0.0, 1.0, None);
        state.view(&mut core, 900.0, 600.0);
        let entry = state.model.rows()[0].clone();

        press(&mut state, &mut core, row_id(&entry));
        let outcome = press(&mut state, &mut core, open_id());
        std::fs::remove_dir_all(&dir).expect("the fixture is removable");
        assert!(
            matches!(outcome, Some(PickerOutcome::Selected(_))),
            "an open must answer the existing file, not open a Replace question"
        );
    }

    /// A trail segment is a jump: the click's id carries the ancestor, and the model follows
    /// it. The bug this pins: the crumbs were clickable regions with no handler, so the
    /// trail painted and answered nothing.
    #[test]
    fn a_breadcrumb_click_jumps_to_its_ancestor() {
        let start = PathBuf::from("/picker-fixture");
        let pack = ui_widgets_md3::pack_of;
        let options = PickerOptions::new(
            OneFile(start.clone()),
            start,
            pack,
            ui_widgets_md3::menu_style_of,
            ui_style::Env::root(),
        );
        let mut state = PickerState::new(options);
        let outcome = state.changed(crumb_id(Path::new("/")), Change::Activated);
        assert!(outcome.is_none(), "navigation is not an answer to the dialog");
        assert_eq!(state.model.path(), Path::new("/"));
    }

    /// A volume row opens the directory it is mounted at, the same way a place does.
    #[test]
    fn a_device_click_opens_its_mount_point() {
        let start = PathBuf::from("/picker-fixture");
        let device = Device {
            path: PathBuf::from("/picker-fixture/volume"),
            label: Some("Volume".to_string()),
        };
        let pack = ui_widgets_md3::pack_of;
        let options = PickerOptions::new(
            OneFile(start.clone()),
            start,
            pack,
            ui_widgets_md3::menu_style_of,
            ui_style::Env::root(),
        )
        .devices(vec![device.clone()]);
        let mut state = PickerState::new(options);
        let outcome = state.changed(device_id(&device), Change::Activated);
        assert!(outcome.is_none(), "navigation is not an answer to the dialog");
        assert_eq!(state.model.path(), device.path);
    }

    /// The arrow beside a crumb opens that folder's subfolders, and a row in the menu is a
    /// jump. The bug this would catch: an arrow that only navigates like its label, or a menu
    /// whose rows are declarations with nowhere to report a click.
    #[test]
    fn a_crumb_arrow_opens_its_subfolders_and_a_choice_navigates() {
        let pack = ui_widgets_md3::pack_of;
        let options = PickerOptions::new(
            Folders,
            PathBuf::from("/a/b"),
            pack,
            ui_widgets_md3::menu_style_of,
            ui_style::Env::root(),
        );
        let mut state = PickerState::new(options);
        let mut store = ui_core::AnimStore::new();
        let mut core = ui_core::cx::Cx::new(&mut store, 0, 900, 600, 0.0, 0.0, 1.0, None);
        let root = state.view(&mut core, 900.0, 600.0);
        let decl = root.decl(&mut core);

        let arrow = crumb_arrow_id(Path::new("/"));
        assert!(
            click(root.as_ref(), &decl, arrow).is_some(),
            "every crumb before the last shows its arrow"
        );
        state.changed(arrow, Change::Activated);
        assert_eq!(state.crumb_menu.as_deref(), Some(Path::new("/")));

        // The next frame builds the menu the click asked for.
        let root = state.view(&mut core, 900.0, 600.0);
        let decl = root.decl(&mut core);
        let item = crumb_item_id(Path::new("/a"));
        assert!(
            click(root.as_ref(), &decl, item).is_some(),
            "the menu declares one control per subfolder"
        );
        state.changed(item, Change::Activated);
        assert_eq!(state.model.path(), Path::new("/a"));
        assert_eq!(
            state.crumb_menu, None,
            "a jump closes the menu it was made from"
        );
    }

    #[test]
    fn a_civil_date_survives_the_leap_years() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(59), (1970, 3, 1));
        // 2000-02-29: a leap day inside an era year divisible by 400.
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(19_999), (2024, 10, 3));
    }

    #[test]
    fn a_size_reads_in_binary_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1.0 KiB");
        assert_eq!(format_size(1024 * 1024), "1.0 MiB");
        assert_eq!(format_size(1536 * 1024), "1.5 MiB");
        assert_eq!(format_size(700 * 1024 * 1024), "700 MiB");
    }

    /// The list's viewport is the panel's, not its content's. A one-row folder in a 600-unit
    /// dialog still scrolls (and clips) down to the dialog's edge: a row dragged below the
    /// last one must not be cut where the content happens to end.
    #[test]
    fn a_short_list_fills_the_panel_below_its_header() {
        struct Panel(PickerState);

        impl App for Panel {
            fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
                let (width, height) = (cx.width() as f32, cx.height() as f32);
                self.0.view(cx, width, height)
            }
        }

        let start = PathBuf::from("/picker-fixture");
        let options = PickerOptions::new(
            OneFile(start.clone()),
            start.clone(),
            ui_widgets_md3::pack_of,
            ui_widgets_md3::menu_style_of,
            ui_style::Env::root(),
        );
        let mut app = AppLoop::new(Panel(PickerState::new(options)));
        let mut target = OffscreenTarget::new(900, 600);
        app.frame(&mut target, None);

        let panel = app
            .tree()
            .find_by_id(Id::str(PANEL))
            .and_then(|node| app.tree().get(node))
            .expect("the panel is laid out")
            .visual_rect()
            .size
            .h;
        let node = app
            .tree()
            .find_by_id(list_id(&start))
            .and_then(|node| app.tree().get(node))
            .expect("the list is laid out");
        let expected = panel - HEADER_HEIGHT - RULE;
        assert!(
            expected > ROW_HEIGHT * 2.0,
            "the premise: one row in a tall panel ({expected} available)"
        );
        assert!(
            (node.visual_rect().size.h - expected).abs() < 0.5,
            "the list fills the panel below its header: {} against {expected}",
            node.visual_rect().size.h
        );
    }

    /// The panel is the folder on screen, so a live drag over it must paint the wash and edge.
    #[test]
    fn an_incoming_drag_lights_the_panel() {
        struct Panel(PickerState);

        impl App for Panel {
            fn view(&mut self, cx: &mut ui_core::cx::Cx<'_>) -> Box<dyn Element> {
                let (width, height) = (cx.width() as f32, cx.height() as f32);
                self.0.view(cx, width, height)
            }
        }

        let start = PathBuf::from("/picker-fixture");
        let options = PickerOptions::new(
            OneFile(start.clone()),
            start.clone(),
            ui_widgets_md3::pack_of,
            ui_widgets_md3::menu_style_of,
            ui_style::Env::root(),
        );
        let mut app = AppLoop::new(Panel(PickerState::new(options)));
        let mut target = OffscreenTarget::new(900, 600);
        app.frame(&mut target, None);
        let rect = app
            .tree()
            .find_by_id(Id::str(PANEL))
            .and_then(|node| app.tree().get(node))
            .expect("the panel is laid out")
            .visual_rect();
        let probe = ui_core::geom::Point::new(
            rect.origin.x + rect.size.w / 2.0,
            rect.origin.y + rect.size.h - 20.0,
        );
        let before = target.pixels.get(probe.x as u32, probe.y as u32);
        app.dispatch(ui_core::Event::Dnd(ui_core::DndEvent::Entered {
            pos: probe,
            types: vec![ui_platform::uri_list::MIME.to_owned()],
            effects: ui_core::DropEffects::COPY,
        }));
        app.frame(&mut target, None);
        let after = target.pixels.get(probe.x as u32, probe.y as u32);
        assert_ne!(
            before, after,
            "the panel paints nothing for a live drag: {before:?}"
        );
    }
}
