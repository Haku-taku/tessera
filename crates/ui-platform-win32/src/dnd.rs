//! Drag and drop on Windows: OLE's drop target, and the source a drag leaves the app with.
//!
//! Both directions go through OLE, on the window's own thread, because that is the thread
//! whose window holds the mouse: `DoDragDrop` is modal on the thread that started it. A
//! drag this app receives arrives as `IDropTarget` callbacks, which post engine events to
//! the window's queue; a drag this app starts runs inside the turn that began it and shows
//! the element's own pixels under the pointer.

use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::rc::Rc;

use ui_core::Event;
use ui_core::dnd::{
    DndEvent, DragData, DragItem, DropEffect, DropEffects, DropOutcome, Performer,
};
use ui_core::geom::{Point, Rect};
use ui_platform::view::{DragImage, NativeDrag};
use windows::Win32::Foundation::{
    COLORREF, DV_E_FORMATETC, E_FAIL, E_NOTIMPL, E_OUTOFMEMORY, HGLOBAL, HWND, POINT, POINTL,
    S_FALSE, S_OK, SIZE,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, ClientToScreen, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HBITMAP, HDC, HGDIOBJ,
    ReleaseDC, SRCCOPY, ScreenToClient, SelectObject,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, DATADIR_GET, DVASPECT_CONTENT, FORMATETC, IAdviseSink,
    IDataObject, IDataObject_Impl, IEnumFORMATETC, IEnumFORMATETC_Impl, STGMEDIUM, STGMEDIUM_0,
    STREAM_SEEK_SET, TYMED_HGLOBAL, TYMED_ISTREAM,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::{
    DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE, DROPEFFECT_NONE, DoDragDrop,
    IDropSource, IDropSource_Impl, IDropTarget, IDropTarget_Impl, OleInitialize, RegisterDragDrop,
    ReleaseStgMedium, RevokeDragDrop,
};
use windows::Win32::System::SystemServices::{
    MK_CONTROL, MK_LBUTTON, MK_MBUTTON, MK_RBUTTON, MK_SHIFT, MODIFIERKEYS_FLAGS,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{
    CLSID_DragDropHelper, IDragSourceHelper, IDropTargetHelper, SHCreateMemStream, SHDRAGIMAGE,
};
use windows::core::{BOOL, Error, HRESULT, Ref, Result, implement};

use crate::clipboard;

/// OLE objects shared with the window's own thread: the events a drag produced, and the
/// effect the last frame negotiated for the pointer.
pub(crate) struct DragQueue {
    events: RefCell<Vec<Event>>,
    /// The effect of the drop under the pointer, in `DROPEFFECT` bits; 0 for none.
    effect: Cell<u32>,
    /// The button the drag is being carried with, read from the keys OLE reports: the
    /// release callback arrives before the button-up, so the drag's own state is the
    /// only place that still knows which button started it.
    button: Cell<ui_core::PointerButton>,
    /// The modifiers last reported, so a drag does not flood the queue with unchanged ones.
    mods: Cell<ui_core::Modifiers>,
    hwnd: HWND,
}

impl DragQueue {
    pub(crate) fn new(hwnd: HWND) -> Self {
        Self {
            events: RefCell::new(Vec::new()),
            effect: Cell::new(0),
            button: Cell::new(ui_core::PointerButton::Primary),
            mods: Cell::new(ui_core::Modifiers::NONE),
            hwnd,
        }
    }

    /// Report the keyboard state OLE carried, if it changed since the last callback.
    ///
    /// The app's own key events do not arrive while another application's drag is modal,
    /// so the drop callback's `grfKeyState` is the only source of the modifiers held.
    fn keys(&self, keys: MODIFIERKEYS_FLAGS) {
        self.keys_with(keys, false);
    }

    /// Report the keyboard state OLE carried, pushing it even when unchanged if `force`.
    ///
    /// A new drag starts from whatever the app last heard; the source's own state is the
    /// truth for the whole gesture, so the entry always announces it.
    fn keys_with(&self, keys: MODIFIERKEYS_FLAGS, force: bool) {
        let modifiers = carried_modifiers(keys);
        if force || modifiers != self.mods.get() {
            self.mods.set(modifiers);
            self.push(Event::Modifiers(modifiers));
        }
        // An absent button bit does not mean "no button": the release callback can run
        // with the button already up, and the last held button is the one that started it.
        if keys.contains(MK_LBUTTON) || keys.contains(MK_RBUTTON) || keys.contains(MK_MBUTTON) {
            self.button.set(carried_button(keys));
        }
    }

    /// Events a drag posted since the last turn.
    pub(crate) fn take(&self) -> Vec<Event> {
        std::mem::take(&mut self.events.borrow_mut())
    }

    /// Whether anything is queued; a modal pump asks before building a frame.
    pub(crate) fn is_empty(&self) -> bool {
        self.events.borrow().is_empty()
    }

    /// Publish what dropping here would do, so the OS pointer shows the negotiated effect.
    pub(crate) fn set_effect(&self, effect: Option<DropEffect>) {
        let bits = match effect {
            Some(DropEffect::Copy) => DROPEFFECT_COPY,
            Some(DropEffect::Move) => DROPEFFECT_MOVE,
            Some(DropEffect::Link) => DROPEFFECT_LINK,
            None => DROPEFFECT_NONE,
        };
        self.effect.set(bits.0);
    }

    pub(crate) fn push(&self, event: Event) {
        self.events.borrow_mut().push(event);
    }

    /// The OLE point as the surface's own logical point.
    fn client(&self, at: &POINTL) -> Option<Point> {
        let mut point = POINT { x: at.x, y: at.y };
        // SAFETY: the window belongs to this thread, and the point outlives the call.
        if !unsafe { ScreenToClient(self.hwnd, &mut point) }.as_bool() {
            return None;
        }
        let scale = unsafe { GetDpiForWindow(self.hwnd) }.max(96) as f32 / 96.0;
        Some(Point::new(point.x as f32 / scale, point.y as f32 / scale))
    }
}

/// Register `hwnd` as a drop target, keeping the interface alive with the returned handle.
pub(crate) fn register(hwnd: HWND, queue: Rc<DragQueue>) -> Option<IDropTarget> {
    if !ensure_ole() {
        log::warn!("ui-platform-win32: OLE could not start, so the window takes no drops");
        return None;
    }
    // OLE's own helper is what paints a source's image and drop description on this
    // window: a drag image is only ever drawn by the target that asks for it.
    let helper: Option<IDropTargetHelper> =
        unsafe { CoCreateInstance(&CLSID_DragDropHelper, None, CLSCTX_INPROC_SERVER) }.ok();
    let target: IDropTarget = DropTarget { queue, helper }.into();
    // SAFETY: the window is this thread's and the interface outlives the registration.
    match unsafe { RegisterDragDrop(hwnd, &target) } {
        Ok(()) => Some(target),
        Err(error) => {
            log::warn!("ui-platform-win32: RegisterDragDrop failed: {error}");
            None
        }
    }
}

/// Withdraw the drop target; the interface itself is dropped by its owner.
pub(crate) fn unregister(hwnd: HWND) {
    // SAFETY: called before the window is destroyed, on the thread that registered it.
    let _ = unsafe { RevokeDragDrop(hwnd) };
}

/// OLE is per-thread; the first call on a thread starts it and every later call reuses it.
fn ensure_ole() -> bool {
    thread_local! {
        static STARTED: Cell<Option<bool>> = const { Cell::new(None) };
    }
    STARTED.with(|cell| match cell.get() {
        Some(started) => started,
        None => {
            // SAFETY: OLE is process-wide state every thread may join; the call is the
            // documented way to join it from one.
            let started = unsafe { OleInitialize(None) }.is_ok();
            cell.set(Some(started));
            started
        }
    })
}

/// The timer a modal gesture drives the app's frames with, in milliseconds.
///
/// Start the drag this app owns, and stay in it until the gesture ends.
///
/// `DoDragDrop` runs a modal loop on the calling thread, and that loop wakes on the mouse
/// input the thread's window captured. A worker thread has no window the OS feeds, so its
/// loop never observes the release: measured, `QueryContinueDrag` was never called and the
/// call never returned. The drag therefore runs on the window's thread, and `pump` is the
/// caller's frame, run from a timer inside OLE's own loop so the app keeps painting and
/// keeps seeing the drag it is carrying. The outcome is queued as `DndEvent::Finished`
/// behind OLE's callbacks.
pub(crate) fn begin(
    hwnd: HWND,
    drag: NativeDrag,
    queue: &DragQueue,
    pump: Option<&mut dyn FnMut()>,
) -> bool {
    let formats = payload_formats(&drag.data);
    if formats.is_empty() {
        return false;
    }
    let allowed = DROPEFFECT(ole_effects(drag.data.effects));
    if allowed.0 == 0 {
        return false;
    }
    // The window's drop target started OLE when it registered; this only covers a window
    // that takes drags out but never any in.
    if !ensure_ole() {
        return false;
    }
    // A target that performs an optimized move signals it by setting our data to a null
    // medium; without that signal the source still owns the originals and must remove
    // them itself when OLE reports a move.
    let moved = Rc::new(Cell::new(false));
    let object: IDataObject = DataObject {
        formats: RefCell::new(formats),
        moved: Rc::clone(&moved),
    }
    .into();
    attach_image(hwnd, &drag, &object);
    let source: IDropSource = DropSource.into();
    let mut effect = DROPEFFECT_NONE;
    if let Some(pump) = pump {
        let timer = crate::modal::arm(hwnd, pump);
        // SAFETY: both interfaces are local to this call and outlive it.
        let result = unsafe { DoDragDrop(&object, &source, allowed, &mut effect) };
        crate::modal::disarm(hwnd, timer);
        queue.push(Event::Dnd(DndEvent::Finished {
            outcome: ole_outcome(result.is_ok(), effect, moved.get()),
        }));
        return true;
    }
    // SAFETY: both interfaces are local to this call and outlive it.
    let result = unsafe { DoDragDrop(&object, &source, allowed, &mut effect) };
    // Queued behind the callbacks OLE just made: any drop this window took is processed
    // before the source hears the gesture is over.
    queue.push(Event::Dnd(DndEvent::Finished {
        outcome: ole_outcome(result.is_ok(), effect, moved.get()),
    }));
    true
}

/// The `DROPEFFECT_*` bits for a payload's permitted effects.
fn ole_effects(effects: DropEffects) -> u32 {
    [DropEffect::Copy, DropEffect::Move, DropEffect::Link]
        .into_iter()
        .filter(|effect| effects.contains(*effect))
        .fold(0, |bits, effect| bits | ole_effect(effect).0)
}

/// One effect as its `DROPEFFECT_*` value.
fn ole_effect(effect: DropEffect) -> DROPEFFECT {
    match effect {
        DropEffect::Copy => DROPEFFECT_COPY,
        DropEffect::Move => DROPEFFECT_MOVE,
        DropEffect::Link => DROPEFFECT_LINK,
    }
}

/// The engine's effects for the `DROPEFFECT_*` bits OLE reports.
fn drop_effects(bits: DROPEFFECT) -> DropEffects {
    let mut effects = DropEffects::NONE;
    if bits.0 & DROPEFFECT_COPY.0 != 0 {
        effects |= DropEffects::COPY;
    }
    if bits.0 & DROPEFFECT_MOVE.0 != 0 {
        effects |= DropEffects::MOVE;
    }
    if bits.0 & DROPEFFECT_LINK.0 != 0 {
        effects |= DropEffects::LINK;
    }
    effects
}

/// How a finished `DoDragDrop` ended, normalized for the engine.
///
/// An optimized move is the target's; any other move is still the source's to finish,
/// because the target copied the data and left the originals in place.
fn ole_outcome(succeeded: bool, effect: DROPEFFECT, moved: bool) -> Option<DropOutcome> {
    if !succeeded || effect == DROPEFFECT_NONE {
        return None;
    }
    let action = if effect == DROPEFFECT_MOVE {
        DropEffect::Move
    } else if effect == DROPEFFECT_LINK {
        DropEffect::Link
    } else {
        DropEffect::Copy
    };
    Some(DropOutcome {
        action,
        performed_by: if action == DropEffect::Move && !moved {
            Performer::Source
        } else {
            Performer::Target
        },
    })
}

/// The keyboard modifiers OLE reports with a drag callback.
fn carried_modifiers(keys: MODIFIERKEYS_FLAGS) -> ui_core::Modifiers {
    /// `MK_ALT`; the windows metadata keeps every other `MK_*` in this module but this one.
    const MK_ALT: u32 = 0x0020;
    ui_core::Modifiers {
        shift: keys.contains(MK_SHIFT),
        ctrl: keys.contains(MK_CONTROL),
        alt: keys.0 & MK_ALT != 0,
        logo: false,
    }
}

/// The button a drag is being carried with, from the mouse bits OLE reports.
fn carried_button(keys: MODIFIERKEYS_FLAGS) -> ui_core::PointerButton {
    if keys.contains(MK_RBUTTON) {
        ui_core::PointerButton::Secondary
    } else if keys.contains(MK_MBUTTON) {
        ui_core::PointerButton::Middle
    } else {
        ui_core::PointerButton::Primary
    }
}

/// Carry the dragged element's own pixels under the pointer for the whole gesture.
///
/// The engine renders the source's subtree into a buffer of its own, so the image is the
/// element and nothing that shared its screen area. A host that did not get one (an older
/// engine, or a build that could not paint the node) falls back to the screen under the
/// element, which at least shows the right pixels when nothing overlaps. OLE copies the
/// bitmap and owns it from the moment the helper takes it.
fn attach_image(hwnd: HWND, drag: &NativeDrag, object: &IDataObject) {
    let rendered = drag
        .image
        .as_ref()
        .and_then(image_bitmap)
        .or_else(|| drag.bounds.and_then(|bounds| capture(hwnd, bounds, drag.origin)));
    let Some((bitmap, width, height, hotspot)) = rendered else {
        return;
    };
    let helper: Result<IDragSourceHelper> =
        unsafe { CoCreateInstance(&CLSID_DragDropHelper, None, CLSCTX_INPROC_SERVER) };
    let helper = match helper {
        Ok(helper) => helper,
        Err(error) => {
            log::warn!("ui-platform-win32: the drag image helper is unavailable: {error}");
            // SAFETY: the bitmap was created here and OLE never saw it.
            unsafe {
                let _ = DeleteObject(HGDIOBJ(bitmap.0));
            }
            return;
        }
    };
    let image = SHDRAGIMAGE {
        sizeDragImage: SIZE {
            cx: width,
            cy: height,
        },
        ptOffset: POINT {
            x: hotspot.0,
            y: hotspot.1,
        },
        hbmpDragImage: bitmap,
        crColorKey: COLORREF(u32::MAX),
    };
    // SAFETY: the data object outlives the call, and the bitmap is handed over with it.
    if let Err(error) = unsafe { helper.InitializeFromBitmap(&image, object) } {
        log::warn!("ui-platform-win32: the drag image was refused: {error}");
    }
}

/// Copy the screen under `bounds` into a 32-bit bitmap, hotspot included.
///
/// `bounds` and `origin` are surface units, and `origin` is the press point: the image's
/// offset is how far inside the element the pointer was, so the picture hangs where the
/// user grabbed it.
fn capture(
    hwnd: HWND,
    bounds: Rect,
    origin: Point,
) -> Option<(windows::Win32::Graphics::Gdi::HBITMAP, i32, i32, (i32, i32))> {
    let scale = unsafe { GetDpiForWindow(hwnd) }.max(96) as f32 / 96.0;
    let width = (bounds.size.w * scale).round() as i32;
    let height = (bounds.size.h * scale).round() as i32;
    if width <= 0 || height <= 0 {
        return None;
    }
    let mut at = POINT {
        x: (bounds.origin.x * scale).round() as i32,
        y: (bounds.origin.y * scale).round() as i32,
    };
    // SAFETY: the window is this thread's and the point outlives the call.
    if !unsafe { ClientToScreen(hwnd, &mut at) }.as_bool() {
        return None;
    }
    let screen = unsafe { GetDC(None) };
    if screen.0.is_null() {
        return None;
    }
    let memory = unsafe { CreateCompatibleDC(Some(screen)) };
    if memory.0.is_null() {
        unsafe { ReleaseDC(None, screen) };
        return None;
    }
    let (bitmap, bits) = match section(memory, width, height) {
        Some(section) => section,
        None => {
            unsafe {
                let _ = DeleteDC(memory);
                ReleaseDC(None, screen);
            }
            return None;
        }
    };
    // SAFETY: both DCs and the bitmap are valid for the call.
    let previous = unsafe { SelectObject(memory, HGDIOBJ(bitmap.0)) };
    let copied = unsafe {
        BitBlt(
            memory,
            0,
            0,
            width,
            height,
            Some(screen),
            at.x,
            at.y,
            SRCCOPY,
        )
    }
    .is_ok();
    unsafe {
        SelectObject(memory, previous);
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
    }
    if !copied || bits.is_null() {
        // SAFETY: the bitmap was created here and never handed to OLE.
        unsafe {
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
        }
        return None;
    }
    // A `BitBlt` leaves the alpha channel at zero, and OLE reads exactly that channel: the
    // element would drag as nothing at all.
    // SAFETY: the section is `width * height` pixels of four bytes, top down.
    let pixels =
        unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), (width * height * 4) as usize) };
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[3] = 0xFF;
    }
    let hotspot = (
        (((origin.x - bounds.origin.x) * scale).round() as i32).clamp(0, width),
        (((origin.y - bounds.origin.y) * scale).round() as i32).clamp(0, height),
    );
    Some((bitmap, width, height, hotspot))
}

/// A 32-bit top-down DIB section sized `width` by `height`, and the pixels behind it.
fn section(dc: HDC, width: i32, height: i32) -> Option<(HBITMAP, *mut std::ffi::c_void)> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative height: rows top down, the way both the screen and the engine read.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    };
    let mut bits = std::ptr::null_mut();
    // SAFETY: the header describes a 32-bit DIB and the out-pointer is writable.
    let bitmap = unsafe {
        CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
    }
    .ok()?;
    if bits.is_null() {
        // SAFETY: the bitmap was created here and has not been handed to anyone.
        unsafe {
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
        }
        return None;
    }
    Some((bitmap, bits))
}

/// The engine's rendered pixels as a drag-image bitmap.
///
/// OLE draws the image with premultiplied alpha; the engine's buffer is straight sRGB, so
/// the colour channels are scaled by alpha here rather than in the renderer, where straight
/// is what every other consumer (a window, a PPM) wants.
fn image_bitmap(image: &DragImage) -> Option<(HBITMAP, i32, i32, (i32, i32))> {
    let (width, height) = (image.size.0 as i32, image.size.1 as i32);
    let length = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
    if width <= 0 || height <= 0 || image.bgra.len() < length {
        return None;
    }
    // A memory DC is only needed to create the section; the pixels outlive it.
    let dc = unsafe { CreateCompatibleDC(None) };
    if dc.0.is_null() {
        return None;
    }
    let section = section(dc, width, height);
    unsafe {
        let _ = DeleteDC(dc);
    }
    let (bitmap, bits) = section?;
    // SAFETY: the section is `width * height` pixels of four bytes, top down.
    let pixels =
        unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), length) };
    for (source, pixel) in image.bgra[..length].chunks_exact(4).zip(pixels.chunks_exact_mut(4)) {
        let alpha = u32::from(source[3]);
        pixel[0] = (u32::from(source[0]) * alpha / 255) as u8;
        pixel[1] = (u32::from(source[1]) * alpha / 255) as u8;
        pixel[2] = (u32::from(source[2]) * alpha / 255) as u8;
        pixel[3] = source[3];
    }
    Some((bitmap, width, height, image.hotspot))
}

/// The Windows formats a payload offers; empty when no MIME type has a Windows form.
fn payload_formats(data: &DragData) -> Vec<(u32, Vec<u8>)> {
    let mut formats: Vec<(u32, Vec<u8>)> = Vec::new();
    for item in &data.items {
        let Some(offered) = clipboard::item_formats(&item.mime, &item.data) else {
            continue;
        };
        for (format, bytes) in offered {
            if !formats.iter().any(|(known, _)| *known == format) {
                formats.push((format, bytes));
            }
        }
    }
    // The source's own preference travels with the payload: OLE itself only knows the set
    // it permits, and the target would otherwise read "no modifier" as copy.
    if !formats.is_empty()
        && let Some(format) = clipboard::preferred_effect_format()
    {
        formats.push((format, ole_effect(data.preferred).0.to_le_bytes().to_vec()));
    }
    formats
}

/// The engine's MIME types an incoming data object offers, best first.
fn offered_types(data: &IDataObject) -> Vec<String> {
    let mut types: Vec<String> = Vec::new();
    for format in clipboard::drop_formats() {
        // SAFETY: the interface is the source's and the format description outlives the call.
        if unsafe { data.QueryGetData(&etc(format)) }.is_err() {
            continue;
        }
        let Some(mime) = clipboard::mime_of(format) else {
            continue;
        };
        if !types.iter().any(|known| known == mime) {
            types.push(mime.to_owned());
        }
    }
    types
}

/// Read every format the data object carries that the engine understands.
fn read_payload(data: &IDataObject) -> Option<DragData> {
    let mut items: Vec<DragItem> = Vec::new();
    for format in clipboard::drop_formats() {
        // SAFETY: the interface is the source's and the format description outlives the call.
        if unsafe { data.QueryGetData(&etc(format)) }.is_err() {
            continue;
        }
        let Ok(medium) = (unsafe { data.GetData(&etc(format)) }) else {
            continue;
        };
        let bytes = medium_bytes(&medium);
        // The block is the source's until this call; copying is what lets it go.
        let mut medium = medium;
        // SAFETY: `medium` came from `GetData` and has not been released.
        unsafe { ReleaseStgMedium(&mut medium) };
        let Some((mime, bytes)) = bytes.and_then(|bytes| clipboard::decode(format, &bytes)) else {
            continue;
        };
        if !items.iter().any(|item| item.mime == mime) {
            items.push(DragItem { mime, data: bytes });
        }
    }
    if items.is_empty() {
        return None;
    }
    let mut payload = DragData::new(items);
    if let Some(effect) = stated_effect(data) {
        payload.preferred = effect;
        payload.effects = payload.effects.union(DropEffects::from_effect(effect));
    }
    Some(payload)
}

/// The effect the source states for the payload, when it names one.
///
/// The same `Preferred DropEffect` block a clipboard carries: Explorer writes it for a cut
/// paste, and this app writes it so a drag coming home keeps its meaning.
fn stated_effect(data: &IDataObject) -> Option<DropEffect> {
    let format = clipboard::preferred_effect_format()?;
    // SAFETY: the interface belongs to the source and outlives the call.
    if unsafe { data.QueryGetData(&etc(format)) }.is_err() {
        return None;
    }
    let mut medium = (unsafe { data.GetData(&etc(format)) }).ok()?;
    let bits = medium_bytes(&medium);
    // SAFETY: `medium` came from `GetData` and has not been released.
    unsafe { ReleaseStgMedium(&mut medium) };
    let bits = u32::from_le_bytes(bits?.get(..4)?.try_into().ok()?);
    Some(
        drop_effects(DROPEFFECT(bits))
            .iter()
            .next()
            .unwrap_or(DropEffect::Copy),
    )
}

/// A `FORMATETC` for one format, globally backed and content-addressed.
fn etc(format: u32) -> FORMATETC {
    FORMATETC {
        cfFormat: format as u16,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

/// The bytes of a `TYMED_HGLOBAL` medium, copied while the block is locked.
fn medium_bytes(medium: &STGMEDIUM) -> Option<Vec<u8>> {
    if medium.tymed != TYMED_HGLOBAL.0 as u32 {
        return None;
    }
    // SAFETY: the union member is the one `tymed` names, and the handle stays valid until
    // `ReleaseStgMedium`.
    let handle = unsafe { medium.u.hGlobal };
    lock_bytes(handle)
}

/// Most a medium read back can hold; a drag description never comes near it.
const STREAM_LIMIT: usize = 16 << 20;

/// The bytes of a `TYMED_ISTREAM` medium, read from its start.
fn stream_bytes(medium: &STGMEDIUM) -> Option<Vec<u8>> {
    if medium.tymed != TYMED_ISTREAM.0 as u32 {
        return None;
    }
    // SAFETY: the union member is the one `tymed` names, and the stream outlives the call.
    let stream = unsafe { (*medium.u.pstm).as_ref() }?;
    // SAFETY: the stream belongs to the medium and is only read here.
    if unsafe { stream.Seek(0, STREAM_SEEK_SET, None) }.is_err() {
        return None;
    }
    let mut bytes: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let mut read = 0;
        // SAFETY: the buffer is writable for `chunk.len()` bytes and `read` is an out-param.
        let status = unsafe {
            stream.Read(
                chunk.as_mut_ptr().cast(),
                chunk.len() as u32,
                Some(&mut read),
            )
        };
        if status.is_err() {
            return None;
        }
        if read == 0 {
            return Some(bytes);
        }
        bytes.extend_from_slice(&chunk[..read as usize]);
        if bytes.len() > STREAM_LIMIT {
            return None;
        }
    }
}

/// Whether the reader asked for the bytes as a stream rather than as a global block.
fn bytes_wanted_as_stream(pformatetc: *const FORMATETC) -> bool {
    // SAFETY: OLE passes a valid pointer for the duration of the call.
    unsafe { pformatetc.as_ref() }.is_some_and(|etc| {
        etc.tymed & TYMED_ISTREAM.0 as u32 != 0 && etc.tymed & TYMED_HGLOBAL.0 as u32 == 0
    })
}

/// The bytes of an `HGLOBAL`, copied out.
fn lock_bytes(handle: HGLOBAL) -> Option<Vec<u8>> {
    // SAFETY: the handle was allocated globally and is locked only for the copy.
    unsafe {
        let pointer = GlobalLock(handle);
        if pointer.is_null() {
            return None;
        }
        let length = windows::Win32::System::Memory::GlobalSize(handle);
        let bytes = std::slice::from_raw_parts(pointer.cast::<u8>(), length).to_vec();
        let _ = GlobalUnlock(handle);
        Some(bytes)
    }
}

/// The window's drop target: every callback posts an event and mirrors the effect.
#[implement(IDropTarget)]
struct DropTarget {
    queue: Rc<DragQueue>,
    /// Paints the source's drag image and drop description over this window.
    helper: Option<IDropTargetHelper>,
}

impl DropTarget {
    /// Hand the same phase to OLE's helper, which owns what the user sees.
    fn show(&self, phase: Phase<'_>) {
        let Some(helper) = &self.helper else {
            return;
        };
        let effect = DROPEFFECT(self.queue.effect.get());
        // SAFETY: the window is this thread's and every pointer lives for the call.
        let _ = unsafe {
            match phase {
                Phase::Enter { data, at } => helper.DragEnter(self.queue.hwnd, data, at, effect),
                Phase::Over { at } => helper.DragOver(at, effect),
                Phase::Leave => helper.DragLeave(),
                Phase::Drop { data, at } => helper.Drop(data, at, effect),
            }
        };
    }
}

/// One callback of a drag, as the helper understands it.
enum Phase<'a> {
    Enter {
        data: &'a IDataObject,
        at: &'a POINT,
    },
    Over {
        at: &'a POINT,
    },
    Leave,
    Drop {
        data: &'a IDataObject,
        at: &'a POINT,
    },
}

impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(
        &self,
        pdataobj: Ref<IDataObject>,
        keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> Result<()> {
        let data = pdataobj.ok()?;
        let types = offered_types(data);
        // SAFETY: OLE passes a readable in-out parameter, which is the documented contract.
        let offered = unsafe { pdweffect.as_ref() }.copied().unwrap_or(DROPEFFECT_NONE);
        let effects = match drop_effects(offered) {
            effects if effects.is_empty() => DropEffects::COPY,
            effects => effects,
        };
        self.queue.keys_with(keys, true);
        if let Some(pos) = self.queue.client(pt) {
            self.queue.push(Event::Dnd(DndEvent::Entered {
                pos,
                types: types.clone(),
                effects,
            }));
        }
        // OLE asks for an answer before this loop has built a frame; copying is the
        // permissive answer, and the next `DragOver` carries the engine's own choice.
        if self.queue.effect.get() == 0 && !types.is_empty() {
            let answer = if effects.contains(DropEffect::Copy) {
                DROPEFFECT_COPY
            } else {
                effects
                    .iter()
                    .next()
                    .map_or(DROPEFFECT_NONE, ole_effect)
            };
            self.queue.effect.set(answer.0);
        }
        write_effect(pdweffect, self.queue.effect.get());
        let at = POINT { x: pt.x, y: pt.y };
        self.show(Phase::Enter { data, at: &at });
        Ok(())
    }

    fn DragOver(
        &self,
        keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> Result<()> {
        self.queue.keys(keys);
        if let Some(pos) = self.queue.client(pt) {
            self.queue.push(Event::Dnd(DndEvent::Over { pos }));
        }
        write_effect(pdweffect, self.queue.effect.get());
        let at = POINT { x: pt.x, y: pt.y };
        self.show(Phase::Over { at: &at });
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        self.queue.push(Event::Dnd(DndEvent::Left));
        self.show(Phase::Leave);
        self.queue.effect.set(0);
        Ok(())
    }

    fn Drop(
        &self,
        pdataobj: Ref<IDataObject>,
        keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> Result<()> {
        let data = pdataobj.ok()?;
        self.queue.keys(keys);
        let effect = self.queue.effect.get();
        if effect != 0
            && let Some(payload) = read_payload(data)
            && let Some(pos) = self.queue.client(pt)
        {
            // An optimized move is the target's to perform. Clearing the source's path
            // list tells it so; without this signal the source still owns the originals
            // and removes them after `DoDragDrop` returns, deleting what was just moved.
            if effect == DROPEFFECT_MOVE.0 {
                let format = etc(clipboard::hdrop_format());
                // SAFETY: a null medium is the documented "data taken" signal, and the
                // format outlives the call.
                let _ = unsafe { data.SetData(&format, std::ptr::null(), false) };
            }
            self.queue.push(Event::Dnd(DndEvent::Dropped {
                pos,
                data: payload,
                button: self.queue.button.get(),
                performer: Performer::Target,
            }));
        }
        write_effect(pdweffect, effect);
        let at = POINT { x: pt.x, y: pt.y };
        self.show(Phase::Drop { data, at: &at });
        self.queue.effect.set(0);
        Ok(())
    }
}

fn write_effect(target: *mut DROPEFFECT, bits: u32) {
    // SAFETY: OLE passes a writable out-parameter, which is the documented contract.
    unsafe {
        if let Some(target) = target.as_mut() {
            *target = DROPEFFECT(bits);
        }
    }
}

/// The payload as an `IDataObject`, with every Windows format it can offer.
///
/// The list grows: OLE's drag-image helper adds its own formats through `SetData`, and the
/// picture under the pointer is one of them.
#[implement(IDataObject)]
struct DataObject {
    formats: RefCell<Vec<(u32, Vec<u8>)>>,
    /// Set when a target moves the originals itself and says so by clearing the data.
    moved: Rc<Cell<bool>>,
}

impl DataObject {
    fn bytes(&self, format: u32) -> Option<Vec<u8>> {
        self.formats
            .borrow()
            .iter()
            .find(|(known, _)| *known == format)
            .map(|(_, bytes)| bytes.clone())
    }

    fn insert(&self, format: u32, bytes: Vec<u8>) {
        let mut formats = self.formats.borrow_mut();
        match formats.iter_mut().find(|(known, _)| *known == format) {
            Some(slot) => slot.1 = bytes,
            None => formats.push((format, bytes)),
        }
    }
}

impl IDataObject_Impl for DataObject_Impl {
    fn GetData(&self, pformatetcin: *const FORMATETC) -> Result<STGMEDIUM> {
        // SAFETY: OLE passes a valid pointer for the duration of the call.
        let format = unsafe { pformatetcin.as_ref() }
            .ok_or_else(|| Error::from_hresult(E_FAIL))?
            .cfFormat as u32;
        let bytes = self
            .bytes(format)
            .ok_or_else(|| Error::from_hresult(DV_E_FORMATETC))?;
        // Anything added through `SetData` came in one medium or the other, and a reader
        // asks for the one it prefers; the bytes are what it actually wants.
        if bytes_wanted_as_stream(pformatetcin) {
            let stream = unsafe { SHCreateMemStream(Some(&bytes)) }
                .ok_or_else(|| Error::from_hresult(E_OUTOFMEMORY))?;
            return Ok(STGMEDIUM {
                tymed: TYMED_ISTREAM.0 as u32,
                u: STGMEDIUM_0 {
                    pstm: ManuallyDrop::new(Some(stream)),
                },
                pUnkForRelease: ManuallyDrop::new(None),
            });
        }
        global_medium(&bytes)
    }

    fn GetDataHere(&self, _pformatetc: *const FORMATETC, _pmedium: *mut STGMEDIUM) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> HRESULT {
        // SAFETY: OLE passes a valid pointer for the duration of the call.
        let Some(etc) = (unsafe { pformatetc.as_ref() }) else {
            return E_FAIL;
        };
        let wanted = etc.cfFormat as u32;
        let present = self.bytes(wanted).is_some();
        let content = etc.dwAspect == DVASPECT_CONTENT.0;
        let carryable = etc.tymed == 0
            || etc.tymed & TYMED_HGLOBAL.0 as u32 != 0
            || etc.tymed & TYMED_ISTREAM.0 as u32 != 0;
        if present && content && carryable {
            S_OK
        } else {
            DV_E_FORMATETC
        }
    }

    fn GetCanonicalFormatEtc(
        &self,
        _pformatectin: *const FORMATETC,
        _pformatetcout: *mut FORMATETC,
    ) -> HRESULT {
        E_NOTIMPL
    }

    fn SetData(
        &self,
        pformatetc: *const FORMATETC,
        pmedium: *const STGMEDIUM,
        frelease: BOOL,
    ) -> Result<()> {
        // SAFETY: OLE passes both pointers valid for the duration of the call.
        let format = unsafe { pformatetc.as_ref() }
            .ok_or_else(|| Error::from_hresult(E_FAIL))?
            .cfFormat as u32;
        // A null medium is the documented "the data is gone" signal: an optimized move
        // performed by the target, so the source must not delete the originals again.
        if pmedium.is_null() {
            self.moved.set(true);
            return Ok(());
        }
        // A medium offered with release is this call's to free, so it is taken by value;
        // without it, only the bytes are read and the caller keeps ownership.
        let medium = if frelease.as_bool() {
            unsafe { std::ptr::read(pmedium) }
        } else {
            unsafe { (*pmedium).clone() }
        };
        // The helper offers its image as a global block and its context as a stream; both
        // are bytes here, and what a reader asks for is decided again in `GetData`.
        let bytes = if medium.tymed == TYMED_HGLOBAL.0 as u32 {
            medium_bytes(&medium)
        } else if medium.tymed == TYMED_ISTREAM.0 as u32 {
            stream_bytes(&medium)
        } else {
            None
        };
        if frelease.as_bool() {
            let mut medium = medium;
            // SAFETY: the medium was moved in above and is released exactly once.
            unsafe { ReleaseStgMedium(&mut medium) };
        }
        let Some(bytes) = bytes else {
            return Err(Error::from_hresult(E_NOTIMPL));
        };
        self.insert(format, bytes);
        Ok(())
    }

    fn EnumFormatEtc(&self, dwdirection: u32) -> Result<IEnumFORMATETC> {
        if dwdirection != DATADIR_GET.0 as u32 {
            return Err(Error::from_hresult(E_NOTIMPL));
        }
        let formats = self
            .formats
            .borrow()
            .iter()
            .map(|(format, _)| etc(*format))
            .collect();
        Ok(FormatEtcEnum::new(formats).into())
    }

    fn DAdvise(
        &self,
        _pformatetc: *const FORMATETC,
        _advf: u32,
        _padvsink: Ref<IAdviseSink>,
    ) -> Result<u32> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn DUnadvise(&self, _dwconnection: u32) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }

    fn EnumDAdvise(&self) -> Result<windows::Win32::System::Com::IEnumSTATDATA> {
        Err(Error::from_hresult(E_NOTIMPL))
    }
}

/// One block a target may take: a globally allocated copy, owned by the caller.
fn global_medium(bytes: &[u8]) -> Result<STGMEDIUM> {
    // SAFETY: a moveable global block is the medium OLE expects for `TYMED_HGLOBAL`.
    unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1))
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        let target = GlobalLock(handle);
        if target.is_null() {
            let _ = windows::Win32::Foundation::GlobalFree(Some(handle));
            return Err(Error::from_hresult(E_OUTOFMEMORY));
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), target.cast::<u8>(), bytes.len());
        let _ = GlobalUnlock(handle);
        Ok(STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: handle },
            pUnkForRelease: ManuallyDrop::new(None),
        })
    }
}

/// The formats a source offers, as an enumerator targets walk.
#[implement(IEnumFORMATETC)]
struct FormatEtcEnum {
    formats: Vec<FORMATETC>,
    index: Cell<usize>,
}

impl FormatEtcEnum {
    fn new(formats: Vec<FORMATETC>) -> Self {
        Self {
            formats,
            index: Cell::new(0),
        }
    }
}

impl IEnumFORMATETC_Impl for FormatEtcEnum_Impl {
    fn Next(&self, celt: u32, rgelt: *mut FORMATETC, pceltfetched: *mut u32) -> HRESULT {
        let start = self.index.get();
        let take = self.formats.len().saturating_sub(start).min(celt as usize);
        for index in 0..take {
            let etc = &self.formats[start + index];
            // SAFETY: the caller offers room for `celt` entries.
            unsafe {
                rgelt.add(index).write(FORMATETC {
                    cfFormat: etc.cfFormat,
                    ptd: etc.ptd,
                    dwAspect: etc.dwAspect,
                    lindex: etc.lindex,
                    tymed: etc.tymed,
                })
            };
        }
        self.index.set(start + take);
        // SAFETY: the out-parameter is optional in COM, and writable when present.
        unsafe {
            if let Some(fetched) = pceltfetched.as_mut() {
                *fetched = take as u32;
            }
        }
        if take == celt as usize { S_OK } else { S_FALSE }
    }

    fn Skip(&self, celt: u32) -> Result<()> {
        let index = self.index.get().saturating_add(celt as usize);
        self.index.set(index.min(self.formats.len()));
        Ok(())
    }

    fn Reset(&self) -> Result<()> {
        self.index.set(0);
        Ok(())
    }

    fn Clone(&self) -> Result<IEnumFORMATETC> {
        Ok(FormatEtcEnum {
            formats: self
                .formats
                .iter()
                .map(|etc| FORMATETC {
                    cfFormat: etc.cfFormat,
                    ptd: etc.ptd,
                    dwAspect: etc.dwAspect,
                    lindex: etc.lindex,
                    tymed: etc.tymed,
                })
                .collect(),
            index: Cell::new(self.index.get()),
        }
        .into())
    }
}

/// What ends a drag this app started: Escape cancels, releasing the button drops.
#[implement(IDropSource)]
struct DropSource;

impl IDropSource_Impl for DropSource_Impl {
    fn QueryContinueDrag(&self, fescapepressed: BOOL, grfkeystate: MODIFIERKEYS_FLAGS) -> HRESULT {
        use windows::Win32::Foundation::{DRAGDROP_S_CANCEL, DRAGDROP_S_DROP};
        if fescapepressed.as_bool() {
            DRAGDROP_S_CANCEL
        } else if !grfkeystate.contains(MK_LBUTTON)
            && !grfkeystate.contains(MK_RBUTTON)
            && !grfkeystate.contains(MK_MBUTTON)
        {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> HRESULT {
        windows::Win32::Foundation::DRAGDROP_S_USEDEFAULTCURSORS
    }
}
