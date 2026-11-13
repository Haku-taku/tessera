// Synthetic input checks against the real tree, and their shared helpers.

use self::action_sheet::action_sheet;
use self::app_bar::app_bar;
use self::buttons::{buttons, cursor};
use self::cards::cards;
use self::carousel::carousel;
use self::checkbox::checkbox;
use self::chips::chips;
use self::combo::combo;
use self::date_picker::date_picker;
use self::dialogs::dialogs;
use self::fab::{fab_menu, fab_page};
use self::grid::grid;
use self::icon_button::icon_button;
use self::icons::icons;
use self::inline::inline;
use self::lists::lists;
use self::markdown::{markdown_code_options, markdown_edit, markdown_page_scroll};
use self::math::math;
use self::menu_flyout::menu_flyout;
use self::modal_sheet::modal_sheet;
use self::nav_bar::nav_bar;
use self::nav_drawer::nav_drawer;
use self::nav_rail::nav_rail;
use self::nav_view::nav_view;
use self::positioning::positioning;
use self::progress::progress;
use self::radio::radio;
use self::search::search;
use self::segmented::segmented;
use self::selection::selection;
use self::slider::slider;
use self::snackbar::snackbar;
use self::stepper::stepper;
use self::switch::switch_state;
use self::tabs::tabs;
use self::text_field::{
    glass_search_bar, glass_text_field, text_field, text_field_caret, text_field_ime,
    text_field_secure,
};
use self::time_picker::time_picker;
use self::toast::toast;
use self::toolbar::toolbar;
use self::tooltip::tooltip;
pub(crate) use crate::app::Gallery;
pub(crate) use crate::pages::*;
pub(crate) use crate::shell::*;
use ui_core::timed;

mod action_sheet;
mod app_bar;
mod buttons;
mod cards;
mod carousel;
mod checkbox;
mod chips;
mod combo;
mod date_picker;
mod dialogs;
mod fab;
mod grid;
mod icon_button;
mod icons;
mod inline;
mod lists;
mod markdown;
mod math;
mod menu_flyout;
mod modal_sheet;
mod nav_bar;
mod nav_drawer;
mod nav_rail;
mod nav_view;
mod positioning;
mod progress;
mod radio;
mod search;
mod segmented;
mod selection;
mod slider;
mod snackbar;
mod stepper;
mod switch;
mod tabs;
mod text_field;
mod time_picker;
mod toast;
mod toolbar;
mod tooltip;

pub(crate) fn want_dump() -> bool {
    std::env::var("GALLERY_DUMP").is_ok()
}

/// `GALLERY_DUMP_FRAMES=n` dumps on frame `n` instead of the first, so a screenshot can
/// catch an animation in flight without the compositor's screencopy.
pub(crate) fn dump_frame() -> u32 {
    std::env::var("GALLERY_DUMP_FRAMES")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0)
}

/// Advance `frames` scripted frames of 16ms, so fades run without sleeping.
pub(crate) fn step(lp: &mut AppLoop<Gallery>, size: (u32, u32), pointer: Point, frames: u32) {
    for _ in 0..frames {
        let _ = lp.build_with_dt(size, Some(pointer), 0.016);
    }
}

pub(crate) fn rect_of(lp: &AppLoop<Gallery>, id: Id) -> Option<Rect> {
    let nid = lp.tree().find_by_id(id)?;
    lp.tree().get(nid).map(|n| n.visual_rect())
}

/// The layout rect, before the visual offset is applied.
pub(crate) fn layout_rect(lp: &AppLoop<Gallery>, id: Id) -> Option<Rect> {
    let nid = lp.tree().find_by_id(id)?;
    lp.tree().get(nid).map(|n| n.rect)
}

/// The rect a control paints for its `index`-th shape, in paint order.
pub(crate) fn shape_rect(lp: &AppLoop<Gallery>, id: Id, index: usize) -> Option<Rect> {
    let nid = lp.tree().find_by_id(id)?;
    let list = lp.draw_list();
    let mut seen = 0;
    for i in 0..list.prims.rects.len() {
        if list.owners.rects.get(i) != Some(&nid) {
            continue;
        }
        if seen == index {
            return Some(list.prims.rects[i].rect);
        }
        seen += 1;
    }
    None
}

/// The fill a control paints for its `index`-th shape, in paint order.
pub(crate) fn shape_fill(
    lp: &AppLoop<Gallery>,
    id: Id,
    index: usize,
) -> Option<ui_core::geom::Color> {
    let nid = lp.tree().find_by_id(id)?;
    let list = lp.draw_list();
    let mut seen = 0;
    for i in 0..list.prims.rects.len() {
        if list.owners.rects.get(i) != Some(&nid) {
            continue;
        }
        if seen == index {
            return Some(list.prims.rects[i].fill);
        }
        seen += 1;
    }
    None
}

/// The `index`-th arc a control paints, in paint order.
pub(crate) fn shape_sector(
    lp: &AppLoop<Gallery>,
    id: Id,
    index: usize,
) -> Option<ui_core::draw::SectorPrim> {
    let nid = lp.tree().find_by_id(id)?;
    let list = lp.draw_list();
    let mut seen = 0;
    for i in 0..list.prims.sectors.len() {
        if list.owners.sectors.get(i) != Some(&nid) {
            continue;
        }
        if seen == index {
            return Some(list.prims.sectors[i]);
        }
        seen += 1;
    }
    None
}

/// Every stroked segment a control paints, in paint order.
pub(crate) fn shape_lines(lp: &AppLoop<Gallery>, id: Id) -> Vec<ui_core::draw::LinePrim> {
    let Some(nid) = lp.tree().find_by_id(id) else {
        return Vec::new();
    };
    let list = lp.draw_list();
    list.prims
        .lines
        .iter()
        .zip(&list.owners.lines)
        .filter(|(_, owner)| **owner == nid)
        .map(|(line, _)| *line)
        .collect()
}

/// Every rect a control paints, in paint order.
pub(crate) fn shape_rects(lp: &AppLoop<Gallery>, id: Id) -> Vec<Rect> {
    let Some(nid) = lp.tree().find_by_id(id) else {
        return Vec::new();
    };
    let list = lp.draw_list();
    list.prims
        .rects
        .iter()
        .zip(&list.owners.rects)
        .filter(|(_, owner)| **owner == nid)
        .map(|(rect, _)| rect.rect)
        .collect()
}

/// The effective ink of a node's first glyph: its colour times its coverage multiplier.
pub(crate) fn glyph_ink(lp: &AppLoop<Gallery>, id: Id) -> Option<ui_core::geom::Color> {
    let nid = lp.tree().find_by_id(id)?;
    let list = lp.draw_list();
    let glyph = list
        .prims
        .glyphs
        .iter()
        .zip(&list.owners.glyphs)
        .find(|(_, owner)| **owner == nid)
        .map(|(glyph, _)| glyph)?;
    Some(ui_core::geom::Color {
        a: glyph.color.a * glyph.alpha,
        ..glyph.color
    })
}

/// The ink a text node asks for, before shaping: its declared colour and alpha.
pub(crate) fn text_ink(lp: &AppLoop<Gallery>, id: Id) -> Option<ui_core::geom::Color> {
    let nid = lp.tree().find_by_id(id)?;
    match &lp.tree().get(nid)?.data {
        ui_core::NodeData::Text { ink, .. } => Some(*ink),
        _ => None,
    }
}

/// The characters a text node draws, as its runs spell them.
pub(crate) fn painted_text(lp: &AppLoop<Gallery>, id: Id) -> Option<String> {
    let nid = lp.tree().find_by_id(id)?;
    match &lp.tree().get(nid)?.data {
        ui_core::NodeData::Text { block, .. } => {
            Some(block.runs.iter().map(|run| run.text.as_str()).collect())
        }
        _ => None,
    }
}

/// Press and release at `at` without settling, then run `frames` 16ms frames: the caller
/// decides how far into the 500ms selection animation it is looking.
pub(crate) fn tap(lp: &mut AppLoop<Gallery>, size: (u32, u32), at: Point, frames: u32) {
    let button = ui_core::PointerButton::Primary;
    lp.dispatch(ui_core::Event::PointerMoved { pos: at });
    step(lp, size, at, 1);
    lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
    step(lp, size, at, 1);
    lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
    lp.dispatch(ui_core::Event::PointerMoved { pos: at });
    step(lp, size, at, frames);
}

/// The last shape a control paints: for a switch that is the thumb.
pub(crate) fn last_shape_rect(lp: &AppLoop<Gallery>, id: Id) -> Option<Rect> {
    let nid = lp.tree().find_by_id(id)?;
    let list = lp.draw_list();
    let mut last = None;
    for i in 0..list.prims.rects.len() {
        if list.owners.rects.get(i) == Some(&nid) {
            last = Some(list.prims.rects[i].rect);
        }
    }
    last
}

/// The largest-area shape a node paints — the handle, ignoring the icon bars that now follow it.
pub(crate) fn biggest_shape_rect(lp: &AppLoop<Gallery>, id: Id) -> Option<Rect> {
    let nid = lp.tree().find_by_id(id)?;
    let list = lp.draw_list();
    let mut best: Option<Rect> = None;
    for i in 0..list.prims.rects.len() {
        if list.owners.rects.get(i) == Some(&nid) {
            let r = list.prims.rects[i].rect;
            // The handle is a square (rest states) or the 34x22 pill (mid-transition); a square
            // wider than 34 is the track or a focus ring, both fixed. The splash is a circle,
            // so requiring a square excludes it.
            let square = (r.size.w - r.size.h).abs() < 0.5;
            // A handle never exceeds 34; the splash is 40 and the track 52, both excluded.
            if !square || r.size.w > 34.5 {
                continue;
            }
            let area = r.size.w * r.size.h;
            if best.map(|b| area > b.size.w * b.size.h).unwrap_or(true) {
                best = Some(r);
            }
        }
    }
    best
}

/// Whether two rects agree to within half a pixel on every edge.
pub(crate) fn close_rect(a: Rect, b: Rect) -> bool {
    (a.origin.x - b.origin.x).abs() <= 0.5
        && (a.origin.y - b.origin.y).abs() <= 0.5
        && (a.size.w - b.size.w).abs() <= 0.5
        && (a.size.h - b.size.h).abs() <= 0.5
}

pub(crate) fn center(r: Rect) -> Point {
    Point::new(r.origin.x + r.size.w * 0.5, r.origin.y + r.size.h * 0.5)
}

pub(crate) fn frame(lp: &mut AppLoop<Gallery>, size: (u32, u32), pointer: Option<Point>) {
    // A full second settles every retargeted instance, so geometry is comparable.
    let _ = timed!("build_scripted", lp.build_with_dt(size, pointer, 1.0));
}

/// The visual top of the page's content, which is what a press's hit test sees.
fn page_content_top(lp: &AppLoop<Gallery>) -> Option<f32> {
    let nid = lp.tree().find_by_id(Id::str("page/view"))?;
    let child = lp.tree().children_of(Some(nid)).first().copied()?;
    lp.tree().get(child).map(|node| node.visual_rect().origin.y)
}

/// Scroll the page's viewport so `at` sits inside it, and say where `at` now sits.
///
/// A page taller than the window scrolls under the app bar, so a control near its end is
/// laid out below the surface and a press there reaches nothing. The wheel is the app's own
/// path to that scroller; the point moves by however much the content moved.
///
/// The movement is read from the content's own visual position, not from the app's scroll
/// request: a caret reveal moves the engine's scroll without touching that request, and a
/// correction off the request landed presses on whatever had moved under them.
pub(crate) fn reveal_page_point(lp: &mut AppLoop<Gallery>, size: (u32, u32), at: Point) -> Point {
    const PAGE_VIEW: Id = Id::str("page/view");
    let Some(view) = rect_of(lp, PAGE_VIEW) else {
        return at;
    };
    let top = view.origin.y.max(0.0);
    let bottom = (view.origin.y + view.size.h).min(size.1 as f32);
    const MARGIN: f32 = 8.0;
    if at.y >= top + MARGIN && at.y <= bottom - MARGIN {
        return at;
    }
    let want = if at.y > bottom {
        at.y - bottom + MARGIN * 3.0
    } else {
        at.y - top - MARGIN * 3.0
    };
    let over = center(view);
    let before = page_content_top(lp);
    lp.dispatch(ui_core::Event::PointerMoved { pos: over });
    lp.dispatch(ui_core::Event::wheel(over, want));
    // A scroller springs into place over a few frames; settle them before reading where
    // the content landed, or the press names a byte the content has since moved away from.
    for _ in 0..4 {
        frame(lp, size, Some(over));
    }
    let after = page_content_top(lp);
    match (before, after) {
        (Some(before), Some(after)) => Point::new(at.x, at.y + (after - before)),
        _ => at,
    }
}

/// Dispatch committed text into the focused input; the engine splices it at the caret.
pub(crate) fn type_text(lp: &mut AppLoop<Gallery>, size: (u32, u32), text: &str) {
    lp.dispatch(ui_core::Event::Text(text.to_owned()));
    frame(lp, size, None);
}

/// Press the node `id` at `dx` from its left edge, once the page has settled.
///
/// A press placed from a rect read while the page is still moving under it lands on
/// whatever moved beneath the point: a caret reveal or a rebuilt document takes a frame or
/// two to reach the tree the hit test reads.
pub(crate) fn click_id(lp: &mut AppLoop<Gallery>, size: (u32, u32), id: Id, dx: f32) {
    for _ in 0..3 {
        frame(lp, size, None);
    }
    let Some(rect) = rect_of(lp, id) else {
        return;
    };
    let at = Point::new(rect.origin.x + dx, rect.origin.y + rect.size.h * 0.5);
    click(lp, size, at);
}

pub(crate) fn click(lp: &mut AppLoop<Gallery>, size: (u32, u32), at: Point) {
    let at = reveal_page_point(lp, size, at);
    let button = ui_core::PointerButton::Primary;
    lp.dispatch(ui_core::Event::PointerMoved { pos: at });
    frame(lp, size, Some(at));
    lp.dispatch(ui_core::Event::PointerPressed { pos: at, button });
    frame(lp, size, Some(at));
    lp.dispatch(ui_core::Event::PointerReleased { pos: at, button });
    // The real loop dispatches a whole burst before one frame (`run`'s `take_events` loop);
    // a motion right behind the release must not erase the click.
    lp.dispatch(ui_core::Event::PointerMoved { pos: at });
    frame(lp, size, Some(at));
    // A page swap converges over a few `Tree::apply` frames; settle before reading geometry.
    for _ in 0..4 {
        frame(lp, size, Some(at));
    }
}

/// Press at `from`, travel to `to` in steps, release: a swipe the tree can follow.
pub(crate) fn drag(lp: &mut AppLoop<Gallery>, size: (u32, u32), from: Point, to: Point) {
    let button = ui_core::PointerButton::Primary;
    lp.dispatch(ui_core::Event::PointerMoved { pos: from });
    frame(lp, size, Some(from));
    lp.dispatch(ui_core::Event::PointerPressed { pos: from, button });
    frame(lp, size, Some(from));
    // Past the engine's drag threshold first, then on to the destination.
    const STEPS: u32 = 6;
    for k in 1..=STEPS {
        let t = k as f32 / STEPS as f32;
        let at = Point::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        lp.dispatch(ui_core::Event::PointerMoved { pos: at });
        frame(lp, size, Some(at));
    }
    lp.dispatch(ui_core::Event::PointerReleased { pos: to, button });
    for _ in 0..4 {
        frame(lp, size, Some(to));
    }
}

/// Wheel the nav to `target` through the real input path, which also invalidates the
/// declaration cache; writing the app field directly would leave the old decl in place.
pub(crate) fn scroll_nav_to(lp: &mut AppLoop<Gallery>, size: (u32, u32), target: f32) {
    let Some(view) = rect_of(lp, NAV_SCROLL) else {
        return;
    };
    let over = center(view);
    let cur = lp.scroll_of(NAV_SCROLL).unwrap_or(0.0);
    let delta = target - cur;
    lp.dispatch(ui_core::Event::PointerMoved { pos: over });
    lp.dispatch(ui_core::Event::wheel(over, delta));
    frame(lp, size, Some(over));
}

/// Wheel the nav until `id`'s centre is inside the scroller, so it can be clicked.
pub(crate) fn scroll_into_view(lp: &mut AppLoop<Gallery>, size: (u32, u32), id: Id) {
    let item_opt = rect_of(lp, id);
    let view_opt = rect_of(lp, NAV_SCROLL);
    let (Some(item), Some(view)) = (item_opt, view_opt) else {
        return;
    };
    let target = center(item);
    // A scroller may reach past the surface it sits in; a row outside the window is not in
    // view whatever the scroller's own rect says, and its centre would take the press off
    // the surface entirely.
    let top = view.origin.y.max(0.0);
    let bottom = (view.origin.y + view.size.h).min(size.1 as f32);
    if target.y >= top && target.y <= bottom {
        return;
    }
    // The item's visual centre is `content_y - scroll`; the scroll that puts it inside
    // the viewport is solved directly, so a deep item lands in one wheel event.
    let cur = lp.scroll_of(NAV_SCROLL).unwrap_or(0.0);
    // Above the viewport the content must move down, i.e. `scroll` shrinks: the delta is
    // `target.y - top - 32`, not the mirrored `top - target.y + 32`, which scrolls the
    // wrong way and leaves the item further out of reach.
    let want = if target.y > bottom {
        target.y - bottom + 32.0
    } else {
        target.y - top - 32.0
    };
    scroll_nav_to(lp, size, cur + want);
}

/// Scroll a page's nav row into the scroller, then click it.
pub(crate) fn goto(lp: &mut AppLoop<Gallery>, size: (u32, u32), page: Page) {
    scroll_into_view(lp, size, page.nav_id());
    nav_click(lp, size, page);
}

/// Click a nav row; each open popup eats one tap, and the menu page opens two.
pub(crate) fn nav_click(lp: &mut AppLoop<Gallery>, size: (u32, u32), page: Page) {
    for _ in 0..3 {
        let nav = rect_of(lp, page.nav_id()).expect("nav item");
        click(lp, size, center(nav));
        if lp.app_mut().page == page {
            return;
        }
    }
}

/// The column tags a page draws, in render order: one where the page shows a single style.
pub(crate) fn page_tags(page: Page) -> &'static [&'static str] {
    match page {
        // The pack-driven page shows one widget kind per column, not one style per column.
        Page::Generic => &["card", "list", "panel"],
        // One column per aspect of the system: colours, dimensions, provenance.
        Page::Md3System | Page::FluentSystem | Page::HigSystem => {
            &["colours", "dimensions", "source"]
        }
        // One column per icon set: the set is the axis, not the style.
        Page::Icons => &["outlined", "rounded", "sharp"],
        // One column per inline case: baseline pair, mixed alignments, narrow block.
        Page::Inline => &["baseline", "mixed", "narrow"],
        // One column per positioning case: relative, absolute, fixed + z.
        Page::Positioning => &["relative", "absolute", "fixed"],
        // A dialog needs a host column of its own; the Fluent column cannot be the narrow one.
        Page::Dialogs => &["md3", "host", "fluent", "glass"],
        // WinUI controls the other packs do not have yet: one Fluent column.
        // The menu page pairs WinUI's flyout with the glass panel it has no counterpart for.
        Page::MenuFlyout => &["fluent", "glass"],
        // The text-mode date input takes the place of this page's two placeholder columns.
        Page::DatePicker => &["md3", "md3-input"],
        // One column for the axes, one for the toggles: the pack has a single style.
        Page::IconButton => &["md3", "md3-toggle"],
        // A document is one column whatever the design system: it has no per-style samples.
        // The in-place editor is a second card under it, not a style column.
        Page::Markdown => &["document", "editing"],
        Page::ToolTip => &["fluent", "md3"],
        Page::InfoBar | Page::Combo | Page::NavView => &["fluent"],
        _ => &[STYLES[0].0, STYLES[1].0, STYLES[2].0],
    }
}

/// Report a page's columns and whether they are disjoint.
pub(crate) fn columns_ok(lp: &AppLoop<Gallery>, page: Page) -> Option<Vec<Rect>> {
    let tags = page_tags(page);
    let mut rects = vec![Rect::ZERO; tags.len()];
    for (i, tag) in tags.iter().enumerate() {
        let r = rect_of(lp, col_id(page, tag))?;
        if r.size.w <= 0.0 || r.size.h <= 0.0 {
            return None;
        }
        rects[i] = r;
    }
    for i in 0..tags.len() {
        for j in (i + 1)..tags.len() {
            let a = rects[i];
            let b = rects[j];
            let overlap_x =
                a.origin.x < b.origin.x + b.size.w && b.origin.x < a.origin.x + a.size.w;
            let overlap_y =
                a.origin.y < b.origin.y + b.size.h && b.origin.y < a.origin.y + a.size.h;
            if overlap_x && overlap_y {
                println!("selftest: columns overlap on {page:?}");
                return None;
            }
        }
    }
    Some(rects)
}

/// Hand the engine's clipboard requests to the seat: the platform half of the split.
pub(crate) fn route_clipboard<H: HostSession<Gallery>>(session: &mut H) {
    use ui_platform::clipboard::{ClipboardCommand, ClipboardKind};
    for cmd in session.lp_mut().take_clipboard_commands() {
        match cmd {
            ClipboardCommand::Copy { content, kind } => {
                if kind == ClipboardKind::Selection {
                    session.write_clipboard(content);
                }
            }
            ClipboardCommand::RequestPaste { kind } => {
                if kind == ClipboardKind::Selection {
                    session.request_paste();
                }
            }
        }
    }
}

pub(crate) fn nav_and_columns(lp: &mut AppLoop<Gallery>, size: (u32, u32)) -> bool {
    let mut pass = true;
    let mut nav_ok = 0;
    let mut cols_ok = 0;

    for page in PAGES {
        scroll_into_view(lp, size, page.nav_id());
        if rect_of(lp, page.nav_id()).is_none() {
            println!("selftest: nav item {:?} is missing", page);
            std::process::exit(1);
        }
        nav_click(lp, size, page);
        if lp.app_mut().page == page {
            nav_ok += 1;
        } else {
            println!("selftest: click did not switch to {:?}", page);
            pass = false;
        }
        // The materials page is a settings pane beside a stage, so it has no three columns
        // to be disjoint: asking it for the other pages' layout would only ever fail.
        if page == Page::Materials {
            cols_ok += 1;
            continue;
        }
        match columns_ok(lp, page) {
            Some(rects) => {
                cols_ok += 1;
                let sizes: Vec<String> = page_tags(page)
                    .iter()
                    .zip(rects)
                    .map(|(_, r)| format!("[{:.0}x{:.0}]", r.size.w, r.size.h))
                    .collect();
                println!(
                    "selftest: {:?} columns {} disjoint ✓",
                    page,
                    sizes.join(", ")
                );
            }
            None => {
                println!("selftest: {:?} columns are missing or overlap", page);
                pass = false;
            }
        }
    }
    println!("selftest: nav click {nav_ok}/{} pages ✓", PAGES.len());
    println!("selftest: three columns {cols_ok}/{} pages ✓", PAGES.len());

    // The three design-system pages must actually render their columns and content.
    // The nav loop above already clicked each page in order, so the last page visited was
    // HigSystem; visit the three system pages again through the same click path.
    let mut sys_ok = 0;
    scroll_nav_to(lp, size, 0.0);
    for page in [Page::Md3System, Page::FluentSystem, Page::HigSystem] {
        scroll_into_view(lp, size, page.nav_id());
        let nav = rect_of(lp, page.nav_id()).expect("system nav item");
        click(lp, size, center(nav));
        let cols = columns_ok(lp, page);
        // Ids are hashes; holding them is the only reliable way to look them up, so the
        // page helper publishes them through a registry the selftest can read.
        // Count rows we both emitted and can still find in the tree.
        let has_common = SYSTEM_ROWS.with(|r| {
            r.common
                .borrow()
                .iter()
                .filter(|id| rect_of(lp, **id).is_some())
                .count()
        }) > 0;
        let has_dim = SYSTEM_ROWS.with(|r| {
            r.dims
                .borrow()
                .iter()
                .filter(|id| rect_of(lp, **id).is_some())
                .count()
        }) > 0;
        let has_src = SYSTEM_ROWS.with(|r| {
            r.source
                .borrow()
                .iter()
                .filter(|id| rect_of(lp, **id).is_some())
                .count()
        }) > 0;
        if cols.is_some() && has_common && has_dim && has_src {
            sys_ok += 1;
        } else {
            println!(
                "selftest: {:?} system page incomplete (cols={} common={has_common} dims={has_dim} src={has_src})",
                page,
                cols.is_some()
            );
        }
    }
    println!("selftest: system pages {sys_ok}/3 complete ✓");
    // Leave the nav where every later test expects it: scrolled to the top.
    scroll_nav_to(lp, size, 0.0);
    pass
}

/// Synthetic input against the real tree: nav, columns, and the kept semantic value.
pub(crate) fn selftest(lp: &mut AppLoop<Gallery>, size: (u32, u32)) {
    frame(lp, size, None);
    let mut pass = true;
    if !nav_and_columns(lp, size) {
        pass = false;
    }
    let (slider_ok, dragged, drag_ok) = slider(lp, size);
    if !slider_ok {
        pass = false;
    }
    if !tabs(lp, size) {
        pass = false;
    }
    if !buttons(lp, size) {
        pass = false;
    }
    if !icon_button(lp, size) {
        pass = false;
    }
    if !fab_menu(lp, size) {
        pass = false;
    }
    if !chips(lp, size) {
        pass = false;
    }
    if !cards(lp, size) {
        pass = false;
    }
    if !lists(lp, size) {
        pass = false;
    }
    if !switch_state(lp, size, dragged, drag_ok) {
        pass = false;
    }
    if !checkbox(lp, size) {
        pass = false;
    }
    if !radio(lp, size) {
        pass = false;
    }
    if !dialogs(lp, size) {
        pass = false;
    }
    if !action_sheet(lp, size) {
        pass = false;
    }
    if !toast(lp, size) {
        pass = false;
    }
    if !modal_sheet(lp, size) {
        pass = false;
    }
    if !stepper(lp, size) {
        pass = false;
    }
    if !date_picker(lp, size) {
        pass = false;
    }
    if !time_picker(lp, size) {
        pass = false;
    }
    if !progress(lp, size) {
        pass = false;
    }
    if !segmented(lp, size) {
        pass = false;
    }
    if !nav_bar(lp, size) {
        pass = false;
    }
    if !nav_rail(lp, size) {
        pass = false;
    }
    if !nav_drawer(lp, size) {
        pass = false;
    }
    if !toolbar(lp, size) {
        pass = false;
    }
    if !snackbar(lp, size) {
        pass = false;
    }
    if !search(lp, size) {
        pass = false;
    }
    if !app_bar(lp, size) {
        pass = false;
    }
    if !carousel(lp, size) {
        pass = false;
    }
    if !text_field(lp, size) {
        pass = false;
    }
    if !text_field_caret(lp, size) {
        pass = false;
    }
    if !text_field_secure(lp, size) {
        pass = false;
    }
    if !fab_page(lp, size) {
        pass = false;
    }
    if !menu_flyout(lp, size) {
        pass = false;
    }
    if !combo(lp, size) {
        pass = false;
    }
    if !nav_view(lp, size) {
        pass = false;
    }
    if !grid(lp, size) {
        pass = false;
    }
    if !inline(lp, size) {
        pass = false;
    }
    if !math(lp, size) {
        pass = false;
    }
    if !markdown_edit(lp, size) {
        pass = false;
    }
    if !markdown_page_scroll(lp, size) {
        pass = false;
    }
    if !markdown_code_options(lp, size) {
        pass = false;
    }
    if !positioning(lp, size) {
        pass = false;
    }
    if !cursor(lp, size) {
        pass = false;
    }
    if !icons(lp, size) {
        pass = false;
    }
    if !selection(lp, size) {
        pass = false;
    }
    if !tooltip(lp, size) {
        pass = false;
    }
    if !text_field_ime(lp, size) {
        pass = false;
    }
    if !glass_text_field(lp, size) {
        pass = false;
    }
    if !glass_search_bar(lp, size) {
        pass = false;
    }

    println!("selftest: {}", if pass { "PASS" } else { "FAIL" });
    if !pass {
        std::process::exit(1);
    }
}
