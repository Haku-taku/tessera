//! Text Services Framework: the document an input method reads and edits on Windows.
//!
//! IMM32 gives an input method a composition and a caret and nothing else: it cannot see the
//! text around the caret, and it cannot ask for any of it to be removed. TSF is the interface
//! that carries both, and while it is attached it also owns the input: the IME writes its
//! composition into the document this module keeps and edits that document through a locked
//! session, so this store — not `WM_IME_COMPOSITION` — is what turns an input method into
//! engine events.
//!
//! The engine keeps the document. What it hands the platform is a window of committed text
//! around the caret, `ImeState::surrounding`; the store publishes that window as the document
//! and holds the IME's composition as a splice at the caret. While a composition is open the
//! store is authoritative, because the engine keeps the preedit out of its buffer by design.
//!
//! A request to remove text around the caret arrives as an ordinary edit of this store. The
//! diff against the committed text becomes `Event::DeleteSurrounding`, whose counts are UTF-8
//! bytes on the engine's side and UTF-16 code units on TSF's.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ui_core::Event;
use ui_core::geom::Rect;
use ui_platform::present::ImeState;
use windows::Win32::Foundation::{E_FAIL, E_NOTIMPL, HWND, LPARAM, POINT, RECT, S_OK, WPARAM};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::TextServices::{
    CLSID_TF_InputProcessorProfiles, CLSID_TF_ThreadMgr, GUID_TFCAT_TIP_KEYBOARD, ITextStoreACP,
    ITextStoreACP_Impl, ITextStoreACPSink, ITfCompositionView, ITfContext,
    ITfContextOwnerCompositionSink, ITfContextOwnerCompositionSink_Impl, ITfDocumentMgr,
    ITfInputProcessorProfileMgr, ITfKeystrokeMgr, ITfRange, ITfRangeACP, ITfThreadMgr,
    TEXT_STORE_LOCK_FLAGS, TF_INPUTPROCESSORPROFILE, TF_IPP_FLAG_ACTIVE, TF_IAS_QUERYONLY,
    TS_AE_END, TS_AE_START, TS_E_NOLAYOUT, TS_E_NOLOCK, TS_E_SYNCHRONOUS, TS_LC_CHANGE,
    TS_LC_CREATE, TS_LF_SYNC, TS_RT_PLAIN, TS_RUNINFO, TS_S_ASYNC, TS_SELECTION_ACP,
    TS_SELECTIONSTYLE, TS_STATUS, TS_TEXTCHANGE,
};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::core::{
    BOOL, GUID, HRESULT, IUnknown, Interface, PCWSTR, PWSTR, Ref, Result, implement,
};

/// The one view every ACP offset and rectangle belongs to.
const VIEW: u32 = 1;

/// What the store knows between calls; its ACP calls and its composition notifications share it.
struct StoreState {
    /// The committed text the engine holds once it has applied what this store emitted.
    committed: String,
    /// The engine caret into `committed`, in bytes.
    committed_caret: usize,
    /// The document as TSF sees it: the committed text with the composition spliced in.
    text: String,
    /// Anchor and caret into `text`, in bytes.
    anchor: usize,
    caret: usize,
    /// The composition's range in `text`, in bytes.
    composition: Option<(usize, usize)>,
    /// The preedit last sent, so an unchanged composition sends nothing.
    preedit: Option<(String, usize)>,
    /// The caret in screen physical pixels, from the last published frame.
    caret_rect: Option<RECT>,
    /// The window the view is in.
    hwnd: HWND,
    /// Engine events waiting to be drained by the window.
    pending: Vec<Event>,
    /// A document lock is held, which lasts only inside `OnLockGranted`.
    locked: bool,
    /// The IME's sink, advised by TSF.
    sink: Option<ITextStoreACPSink>,
}

impl StoreState {
    fn new(hwnd: HWND) -> Self {
        Self {
            committed: String::new(),
            committed_caret: 0,
            text: String::new(),
            anchor: 0,
            caret: 0,
            composition: None,
            preedit: None,
            caret_rect: None,
            hwnd,
            pending: Vec::new(),
            locked: false,
            sink: None,
        }
    }

    /// Publish a frame's committed window, its caret and its anchor.
    ///
    /// An open composition owns the document until it ends: the engine's buffer excludes the
    /// preedit by design, so its window would otherwise delete the composition from under the
    /// IME on every frame.
    fn publish(&mut self, ime: &ImeState) -> bool {
        if self.composition.is_some() {
            return false;
        }
        let (text, caret, anchor) = ime.surrounding.clone().unwrap_or_default();
        let caret = caret.min(text.len());
        let anchor = anchor.min(text.len());
        let changed = self.text != text || self.caret != caret || self.anchor != anchor;
        self.committed = text.clone();
        self.committed_caret = caret;
        self.text = text;
        self.caret = caret;
        self.anchor = anchor;
        self.preedit = None;
        changed
    }

    /// The text outside the composition, which is what the engine's buffer holds.
    fn outside(&self) -> String {
        let Some((start, end)) = self.composition else {
            return self.text.clone();
        };
        let mut outside = self.text.get(..start).unwrap_or_default().to_owned();
        outside.push_str(self.text.get(end..).unwrap_or_default());
        outside
    }

    /// Diff the committed side against the engine's model and emit what the engine must do.
    ///
    /// One edit changes one contiguous span, so the diff is a common prefix and a common
    /// suffix. A removal splits at the engine caret into the two counts
    /// `DeleteSurrounding` carries; an addition at the caret is committed text.
    fn derive_committed(&mut self) {
        let outside = self.outside();
        let prefix = common_prefix(&self.committed, &outside);
        let suffix = common_suffix(&self.committed, &outside, prefix);
        let old_end = self.committed.len() - suffix;
        let new_end = outside.len() - suffix;
        let caret = self.committed_caret;
        if prefix < old_end {
            let split = caret.clamp(prefix, old_end);
            if caret < prefix || caret > old_end {
                // The engine only removes a range around its caret; an edit elsewhere cannot
                // be expressed, and the diff would name the wrong bytes.
                log::warn!("tsf: a removal at {prefix}..{old_end} misses the caret at {caret}");
            }
            self.pending.push(Event::DeleteSurrounding {
                before: self.committed[prefix..split].len(),
                after: self.committed[split..old_end].len(),
            });
        }
        if prefix < new_end {
            self.pending
                .push(Event::Text(outside[prefix..new_end].to_owned()));
        }
        // Text written inside an open composition leaves the committed side alone, and with
        // it the engine's caret: only a real diff moves that caret.
        if prefix < old_end || prefix < new_end {
            self.committed_caret = shift(caret, prefix, old_end - prefix, new_end - prefix);
            self.committed = outside;
        }
    }

    /// Close the composition: the preedit goes, then whatever it committed reaches the
    /// engine. Undelivered preedit updates collapse, so a commit does not flicker through
    /// its final character first.
    fn end_composition(&mut self) {
        self.composition = None;
        while matches!(self.pending.last(), Some(Event::Preedit(_))) {
            self.pending.pop();
        }
        if self.preedit.take().is_some() {
            self.pending.push(Event::Preedit(None));
        }
        self.derive_committed();
    }

    /// Send the composition's text and its cursor as the engine's preedit.
    fn derive_preedit(&mut self) {
        let next = self.composition.and_then(|(start, end)| {
            let text = self.text.get(start..end).filter(|text| !text.is_empty())?;
            let cursor = (self.caret.clamp(start, end) - start).min(text.len());
            Some((text.to_owned(), cursor))
        });
        if next == self.preedit {
            return;
        }
        self.preedit = next.clone();
        self.pending.push(Event::Preedit(
            next.map(|(text, cursor)| ui_core::Preedit::new(text, cursor)),
        ));
    }

    /// The text string's length in the code units TSF indexes by.
    fn units(&self) -> i32 {
        units_of(&self.text) as i32
    }

    /// A byte offset as a code-unit offset, which is what the store's callers speak.
    fn acp(&self, byte: usize) -> i32 {
        units_of(self.text.get(..byte).unwrap_or_default()) as i32
    }

    /// Replace a code-unit range, keeping the selection and the composition in place.
    fn replace(&mut self, start: i32, end: i32, with: &str) -> TS_TEXTCHANGE {
        let (start, end) = self.clamp_range(start, end);
        let old_start = self.acp(start);
        let old_end = self.acp(end);
        let removed = end - start;
        let inserted = with.len();
        self.text.replace_range(start..end, with);
        for point in [&mut self.caret, &mut self.anchor] {
            *point = shift(*point, start, removed, inserted);
        }
        if let Some((comp_start, comp_end)) = &mut self.composition {
            let span = shift_span(*comp_start, *comp_end, start, removed, inserted);
            *comp_start = span.0;
            *comp_end = span.1;
        }
        self.derive_committed();
        self.derive_preedit();
        TS_TEXTCHANGE {
            acpStart: old_start,
            acpOldEnd: old_end,
            acpNewEnd: self.acp(start + inserted),
        }
    }

    /// A code-unit range as byte offsets inside the text, ordered and on char boundaries.
    fn clamp_range(&self, start: i32, end: i32) -> (usize, usize) {
        let (low, high) = (start.min(end), start.max(end));
        (acp_to_byte(&self.text, low), acp_to_byte(&self.text, high))
    }

    /// A code-unit range as byte offsets, clamped into the text.
    fn range(&self, start: i32, end: i32) -> (usize, usize) {
        self.clamp_range(start, end)
    }

    fn take_pending(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.pending)
    }
}

/// A composition's range after an edit: text written at a collapsed composition's position
/// joins the composition, which is how an IME's first keystroke arrives.
fn shift_span(
    start: usize,
    end: usize,
    at: usize,
    removed: usize,
    inserted: usize,
) -> (usize, usize) {
    if start == end {
        if start == at && removed == 0 {
            return (start, end + inserted);
        }
        let point = shift(start, at, removed, inserted);
        return (point, point);
    }
    (
        shift(start, at, removed, inserted),
        shift(end, at, removed, inserted),
    )
}

/// `pos` after a replacement that removed `removed` bytes at `start` and inserted `inserted`.
fn shift(pos: usize, start: usize, removed: usize, inserted: usize) -> usize {
    if pos >= start + removed {
        pos - removed + inserted
    } else if pos > start {
        start + inserted
    } else {
        pos
    }
}

fn common_prefix(a: &str, b: &str) -> usize {
    let mut index = 0;
    for (left, right) in a.chars().zip(b.chars()) {
        if left != right {
            break;
        }
        index += left.len_utf8();
    }
    index
}

/// How many bytes the two strings share at their ends, without reaching into the prefix.
fn common_suffix(a: &str, b: &str, prefix: usize) -> usize {
    let mut shared = 0;
    for (left, right) in a.chars().rev().zip(b.chars().rev()) {
        if left != right || prefix + shared + left.len_utf8() > a.len() {
            break;
        }
        shared += left.len_utf8();
    }
    shared
}

fn units_of(text: &str) -> usize {
    text.encode_utf16().count()
}

/// A code-unit offset as a byte offset, snapped back so it never splits a character.
fn acp_to_byte(text: &str, units: i32) -> usize {
    let units = usize::try_from(units).unwrap_or(usize::MAX);
    let mut seen = 0;
    for (index, character) in text.char_indices() {
        if seen + character.len_utf16() > units {
            return index;
        }
        seen += character.len_utf16();
    }
    text.len()
}

/// The screen rectangle a caret occupies, from the engine's logical surface coordinates.
fn screen_rect(caret: Rect, dpi: u32, origin: POINT) -> RECT {
    let scale = dpi.max(96) as f32 / 96.0;
    let left = origin.x + (caret.origin.x * scale).round() as i32;
    let top = origin.y + (caret.origin.y * scale).round() as i32;
    RECT {
        left,
        top,
        right: left + (caret.size.w.max(1.0) * scale).round() as i32,
        bottom: top + (caret.size.h.max(1.0) * scale).round() as i32,
    }
}

/// The value a locked method returns when the document is not locked.
fn lock(state: &StoreState) -> Result<()> {
    if state.locked {
        Ok(())
    } else {
        Err(windows::core::Error::from_hresult(TS_E_NOLOCK))
    }
}

// The manager queries the object handed to `CreateContext` for the composition sink itself and
// installs it, so the owner's notifications live on the store rather than on a separate advisor.
#[implement(ITextStoreACP, ITfContextOwnerCompositionSink)]
struct TextStore {
    state: Rc<RefCell<StoreState>>,
}

impl ITextStoreACP_Impl for TextStore_Impl {
    fn AdviseSink(&self, riid: *const GUID, punk: Ref<IUnknown>, _mask: u32) -> Result<()> {
        let id =
            unsafe { riid.as_ref() }.ok_or_else(|| windows::core::Error::from_hresult(E_FAIL))?;
        if *id != ITextStoreACPSink::IID {
            return Err(windows::core::Error::from_hresult(E_NOTIMPL));
        }
        let sink: ITextStoreACPSink = punk.ok()?.cast()?;
        self.state.borrow_mut().sink = Some(sink);
        Ok(())
    }

    fn UnadviseSink(&self, _punk: Ref<IUnknown>) -> Result<()> {
        self.state.borrow_mut().sink = None;
        Ok(())
    }

    fn RequestLock(&self, flags: u32) -> Result<HRESULT> {
        let sink = {
            let mut state = self.state.borrow_mut();
            if state.locked {
                // Everything here runs on the window's thread, so a request that arrives
                // while a lock is held is only ever refused or answered as asynchronous.
                return Ok(if flags & TS_LF_SYNC != 0 {
                    TS_E_SYNCHRONOUS
                } else {
                    TS_S_ASYNC
                });
            }
            state.locked = true;
            state.sink.clone()
        };
        let result = match sink {
            Some(sink) => unsafe { sink.OnLockGranted(TEXT_STORE_LOCK_FLAGS(flags)) },
            None => Err(windows::core::Error::from_hresult(E_FAIL)),
        };
        self.state.borrow_mut().locked = false;
        result.map(|()| S_OK)
    }

    fn GetStatus(&self) -> Result<TS_STATUS> {
        Ok(TS_STATUS {
            dwDynamicFlags: 0,
            dwStaticFlags: 0,
        })
    }

    fn QueryInsert(
        &self,
        start: i32,
        end: i32,
        _length: u32,
        result_start: *mut i32,
        result_end: *mut i32,
    ) -> Result<()> {
        let state = self.state.borrow();
        lock(&state)?;
        let length = state.units();
        let (low, high) = (
            start.min(end).clamp(0, length),
            start.max(end).clamp(0, length),
        );
        unsafe {
            result_start.write(low);
            result_end.write(high);
        }
        Ok(())
    }

    fn GetSelection(
        &self,
        index: u32,
        count: u32,
        selection: *mut TS_SELECTION_ACP,
        fetched: *mut u32,
    ) -> Result<()> {
        let state = self.state.borrow();
        lock(&state)?;
        if index != 0 || count == 0 || selection.is_null() {
            unsafe { fetched.write(0) };
            return Ok(());
        }
        let (start, end) = (state.anchor.min(state.caret), state.anchor.max(state.caret));
        let active = if state.caret < state.anchor {
            TS_AE_START
        } else {
            TS_AE_END
        };
        unsafe {
            selection.write(TS_SELECTION_ACP {
                acpStart: state.acp(start),
                acpEnd: state.acp(end),
                style: TS_SELECTIONSTYLE {
                    ase: active,
                    fInterimChar: BOOL(0),
                },
            });
            fetched.write(1);
        }
        Ok(())
    }

    fn SetSelection(&self, count: u32, selection: *const TS_SELECTION_ACP) -> Result<()> {
        let mut state = self.state.borrow_mut();
        lock(&state)?;
        if count == 0 || selection.is_null() {
            return Ok(());
        }
        let chosen = unsafe { *selection };
        let (low, high) = state.range(chosen.acpStart, chosen.acpEnd);
        let reversed = chosen.style.ase == TS_AE_START;
        state.anchor = if reversed { high } else { low };
        state.caret = if reversed { low } else { high };
        state.derive_preedit();
        Ok(())
    }

    fn GetText(
        &self,
        start: i32,
        end: i32,
        plain: PWSTR,
        plain_requested: u32,
        plain_returned: *mut u32,
        runs: *mut TS_RUNINFO,
        runs_requested: u32,
        runs_returned: *mut u32,
        next: *mut i32,
    ) -> Result<()> {
        let state = self.state.borrow();
        lock(&state)?;
        let all: Vec<u16> = state.text.encode_utf16().collect();
        // TSF asks for the rest of the document with an end of -1.
        let end = if end < 0 { state.units() } else { end };
        let (low, high) = state.range(start, end);
        let low = state.acp(low).max(0) as usize;
        let high = state.acp(high).max(0) as usize;
        let piece = all
            .get(low.min(high)..high.min(all.len()))
            .unwrap_or_default();
        unsafe {
            if plain.is_null() {
                plain_returned.write(piece.len() as u32);
            } else {
                let copied = piece.len().min(plain_requested as usize);
                for (index, unit) in piece.iter().take(copied).enumerate() {
                    plain.as_ptr().add(index).write(*unit);
                }
                plain_returned.write(copied as u32);
            }
            if !runs.is_null() && runs_requested > 0 {
                runs.write(TS_RUNINFO {
                    uCount: piece.len() as u32,
                    r#type: TS_RT_PLAIN,
                });
                runs_returned.write(1);
            } else if !runs_returned.is_null() {
                runs_returned.write(0);
            }
            if !next.is_null() {
                next.write(low.saturating_add(piece.len()).min(i32::MAX as usize) as i32);
            }
        }
        Ok(())
    }

    fn SetText(
        &self,
        _flags: u32,
        start: i32,
        end: i32,
        text: &PCWSTR,
        length: u32,
    ) -> Result<TS_TEXTCHANGE> {
        let mut state = self.state.borrow_mut();
        lock(&state)?;
        let replaced = wide_string(text.as_ptr(), length);
        Ok(state.replace(start, end, &replaced))
    }

    fn GetFormattedText(
        &self,
        _start: i32,
        _end: i32,
    ) -> Result<windows::Win32::System::Com::IDataObject> {
        Err(windows::core::Error::from_hresult(E_NOTIMPL))
    }

    fn GetEmbedded(
        &self,
        _position: i32,
        _service: *const GUID,
        _riid: *const GUID,
    ) -> Result<IUnknown> {
        Err(windows::core::Error::from_hresult(E_NOTIMPL))
    }

    fn QueryInsertEmbedded(
        &self,
        _service: *const GUID,
        _format: *const windows::Win32::System::Com::FORMATETC,
    ) -> Result<BOOL> {
        Ok(BOOL(0))
    }

    fn InsertEmbedded(
        &self,
        _flags: u32,
        _start: i32,
        _end: i32,
        _data: Ref<windows::Win32::System::Com::IDataObject>,
    ) -> Result<TS_TEXTCHANGE> {
        Err(windows::core::Error::from_hresult(E_NOTIMPL))
    }

    fn InsertTextAtSelection(
        &self,
        flags: u32,
        text: &PCWSTR,
        length: u32,
        start: *mut i32,
        end: *mut i32,
        change: *mut TS_TEXTCHANGE,
    ) -> Result<()> {
        let mut state = self.state.borrow_mut();
        lock(&state)?;
        let inserted = wide_string(text.as_ptr(), length);
        let (low, high) = (state.anchor.min(state.caret), state.anchor.max(state.caret));
        let (from, to) = (state.acp(low), state.acp(high));
        let result = if flags & TF_IAS_QUERYONLY.0 != 0 {
            TS_TEXTCHANGE {
                acpStart: from,
                acpOldEnd: to,
                acpNewEnd: to,
            }
        } else {
            state.replace(from, to, &inserted)
        };
        unsafe {
            if !start.is_null() {
                start.write(result.acpStart);
            }
            if !end.is_null() {
                end.write(result.acpNewEnd);
            }
            if !change.is_null() {
                change.write(result);
            }
        }
        Ok(())
    }

    fn InsertEmbeddedAtSelection(
        &self,
        _flags: u32,
        _data: Ref<windows::Win32::System::Com::IDataObject>,
        _start: *mut i32,
        _end: *mut i32,
        _change: *mut TS_TEXTCHANGE,
    ) -> Result<()> {
        Err(windows::core::Error::from_hresult(E_NOTIMPL))
    }

    fn RequestSupportedAttrs(&self, _flags: u32, _filter: u32, _attrs: *const GUID) -> Result<()> {
        Ok(())
    }

    fn RequestAttrsAtPosition(
        &self,
        _position: i32,
        _filter: u32,
        _attrs: *const GUID,
        _flags: u32,
    ) -> Result<()> {
        Ok(())
    }

    fn RequestAttrsTransitioningAtPosition(
        &self,
        _position: i32,
        _filter: u32,
        _attrs: *const GUID,
        _flags: u32,
    ) -> Result<()> {
        Ok(())
    }

    fn FindNextAttrTransition(
        &self,
        _start: i32,
        limit: i32,
        _filter: u32,
        _attrs: *const GUID,
        _flags: u32,
        next: *mut i32,
        found: *mut BOOL,
        offset: *mut i32,
    ) -> Result<()> {
        unsafe {
            if !next.is_null() {
                next.write(limit);
            }
            if !found.is_null() {
                found.write(BOOL(0));
            }
            if !offset.is_null() {
                offset.write(-1);
            }
        }
        Ok(())
    }

    fn RetrieveRequestedAttrs(
        &self,
        _count: u32,
        _values: *mut windows::Win32::UI::TextServices::TS_ATTRVAL,
        fetched: *mut u32,
    ) -> Result<()> {
        unsafe {
            if !fetched.is_null() {
                fetched.write(0);
            }
        }
        Ok(())
    }

    fn GetEndACP(&self) -> Result<i32> {
        let state = self.state.borrow();
        lock(&state)?;
        Ok(state.units())
    }

    fn GetActiveView(&self) -> Result<u32> {
        Ok(VIEW)
    }

    fn GetACPFromPoint(&self, _view: u32, _point: *const POINT, _flags: u32) -> Result<i32> {
        let state = self.state.borrow();
        lock(&state)?;
        Ok(state.acp(state.caret))
    }

    fn GetTextExt(
        &self,
        _view: u32,
        start: i32,
        end: i32,
        rect: *mut RECT,
        clipped: *mut BOOL,
    ) -> Result<()> {
        let state = self.state.borrow();
        lock(&state)?;
        let Some(caret) = state.caret_rect else {
            return Err(windows::core::Error::from_hresult(TS_E_NOLAYOUT));
        };
        let position = state.acp(state.caret);
        unsafe {
            if !rect.is_null() {
                rect.write(caret);
            }
            if !clipped.is_null() {
                let inside = start.min(end) <= position && position <= start.max(end);
                clipped.write(BOOL(!inside as i32));
            }
        }
        Ok(())
    }

    fn GetScreenExt(&self, _view: u32) -> Result<RECT> {
        let hwnd = self.state.borrow().hwnd;
        let mut client = RECT::default();
        unsafe { GetClientRect(hwnd, &mut client)? };
        let mut origin = POINT {
            x: client.left,
            y: client.top,
        };
        let mut far = POINT {
            x: client.right,
            y: client.bottom,
        };
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut origin);
            let _ = windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut far);
        }
        Ok(RECT {
            left: origin.x,
            top: origin.y,
            right: far.x,
            bottom: far.y,
        })
    }

    fn GetWnd(&self, _view: u32) -> Result<HWND> {
        Ok(self.state.borrow().hwnd)
    }
}

/// A UTF-16 buffer as a Rust string, for the text a store write carries.
fn wide_string(pointer: *const u16, length: u32) -> String {
    if pointer.is_null() || length == 0 {
        return String::new();
    }
    let units = unsafe { std::slice::from_raw_parts(pointer, length as usize) };
    String::from_utf16_lossy(units)
}

impl ITfContextOwnerCompositionSink_Impl for TextStore_Impl {
    fn OnStartComposition(&self, composition: Ref<ITfCompositionView>) -> Result<BOOL> {
        let mut state = self.state.borrow_mut();
        if let Some((start, end)) = composition.as_ref().and_then(composition_acp) {
            let range = state.range(start, end);
            state.composition = Some(range);
        }
        // A store that refuses the composition leaves the IME no way to compose at all.
        Ok(BOOL(1))
    }

    fn OnUpdateComposition(
        &self,
        composition: Ref<ITfCompositionView>,
        _replaced: Ref<ITfRange>,
    ) -> Result<()> {
        let mut state = self.state.borrow_mut();
        if let Some((start, end)) = composition.as_ref().and_then(composition_acp) {
            let range = state.range(start, end);
            state.composition = Some(range);
        }
        state.derive_preedit();
        Ok(())
    }

    fn OnEndComposition(&self, _composition: Ref<ITfCompositionView>) -> Result<()> {
        self.state.borrow_mut().end_composition();
        Ok(())
    }
}

/// The ACP extent a composition view covers.
fn composition_acp(composition: &ITfCompositionView) -> Option<(i32, i32)> {
    let range = unsafe { composition.GetRange() }.ok()?;
    let acp: ITfRangeACP = range.cast().ok()?;
    let (mut start, mut length) = (0i32, 0i32);
    unsafe { acp.GetExtent(&mut start, &mut length) }.ok()?;
    Some((start.max(0), start.max(0).saturating_add(length.max(0))))
}

/// The document a focused input hands an IME, and the sink that turns its edits into events.
pub(crate) struct Session {
    hwnd: HWND,
    manager: ITfThreadMgr,
    document: ITfDocumentMgr,
    store: Rc<RefCell<StoreState>>,
    /// The document is associated with the window, so TSF routes input to this store.
    attached: bool,
    /// The next `WM_CHAR` belongs to a key the IME took and must not reach the engine.
    swallow_char: Cell<bool>,
}

/// Whether this thread's active input method is a service TSF can drive.
///
/// TSF asks a text service about a key synchronously, which only a service loaded into this
/// process can answer. The platform's out-of-process input methods — the Windows 11 IMEs,
/// `ChsIME.exe` and its kin — are instead driven through the window's IMM32 emulation, and
/// `ITfThreadMgr::AssociateFocus` switches that emulation off: a window associated with one
/// of them took no key, composed nothing, and could not even be switched. Which kind a
/// profile is is what the registry says it registers, `InprocServer32` or `LocalServer32`.
pub fn serves_input() -> bool {
    use windows::Win32::System::Registry::{
        HKEY_CLASSES_ROOT, KEY_QUERY_VALUE, RegCloseKey, RegOpenKeyExW,
    };

    // The profile manager is a COM object. A thread some other part of the process already
    // put in an apartment can still create it, so a failed call here means no TSF at all.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let profiles: ITfInputProcessorProfileMgr = match unsafe {
        CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)
    } {
        Ok(profiles) => profiles,
        Err(_) => return false,
    };
    let mut active = TF_INPUTPROCESSORPROFILE::default();
    if unsafe { profiles.GetActiveProfile(&GUID_TFCAT_TIP_KEYBOARD, &mut active) }.is_err() {
        return false;
    }
    if active.dwFlags & TF_IPP_FLAG_ACTIVE == 0 {
        // No service holds the keyboard, so there is nothing TSF could carry.
        return false;
    }

    let path = crate::window::wide(&format!("CLSID\\{{{:?}}}\\InprocServer32", active.clsid));
    let mut key = windows::Win32::System::Registry::HKEY::default();
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_CLASSES_ROOT,
            PCWSTR::from_raw(path.as_ptr()),
            None,
            KEY_QUERY_VALUE,
            &mut key,
        )
    };
    if opened.0 != 0 {
        return false;
    }
    // The key was only opened to exist; a close that fails leaves nothing this call owns.
    let _ = unsafe { RegCloseKey(key) };
    true
}

impl Session {
    /// Bring TSF up for this window: a thread manager, one document, and its text store.
    ///
    /// The document is pushed but not associated: nothing takes the keyboard until a text
    /// field asks for it.
    pub(crate) fn attach(hwnd: HWND) -> windows::core::Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            let manager: ITfThreadMgr =
                CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER)?;
            // The manager hands out this thread's client id; a context is owned by it, and
            // TSF rejects a context created under no client.
            let client = manager.Activate()?;
            let document = manager.CreateDocumentMgr()?;
            let store = Rc::new(RefCell::new(StoreState::new(hwnd)));
            let text_store: ITextStoreACP = TextStore {
                state: store.clone(),
            }
            .into();
            let mut context: Option<ITfContext> = None;
            let mut flags = 0u32;
            document.CreateContext(client, 0, &text_store, &mut context, &mut flags)?;
            let context = context.ok_or_else(|| windows::core::Error::from_hresult(E_FAIL))?;
            document.Push(&context)?;

            Ok(Self {
                hwnd,
                manager,
                document,
                store,
                attached: false,
                swallow_char: Cell::new(false),
            })
        }
    }

    /// Give the document to the window, or take it back; false when TSF refused.
    pub(crate) fn set_attached(&mut self, attached: bool) -> bool {
        if self.attached == attached {
            return true;
        }
        // The document takes the thread's focus first, and the window is associated with it
        // afterwards so that a later focus event on the window does not take it back.
        let focus = attached.then_some(&self.document);
        if let Err(error) = unsafe { self.manager.SetFocus(focus) } {
            log::warn!("tsf: set focus({attached}) failed: {error}");
        }
        let result = unsafe {
            if attached {
                self.manager.AssociateFocus(self.hwnd, Some(&self.document))
            } else {
                self.manager
                    .AssociateFocus(self.hwnd, None::<&ITfDocumentMgr>)
            }
        };
        // The call hands back the document it replaced, which is null for a window that had
        // none; the binding's non-nullable out parameter reports that success as an error.
        if let Some(error) = result.err().filter(|error| error.code().is_err()) {
            log::warn!("tsf: associate({attached}) failed: {error}");
            return false;
        }
        self.attached = attached;
        if attached {
            let sink = self.store.borrow().sink.clone();
            if let Some(sink) = sink
                && let Err(error) = unsafe { sink.OnLayoutChange(TS_LC_CREATE, VIEW) }
            {
                log::warn!("tsf: the store could not report a layout: {error}");
            }
        }
        true
    }

    /// Publish this frame's view: the caret rectangle always, the committed text when no
    /// composition owns the document.
    pub(crate) fn publish(&mut self, ime: &ImeState, dpi: u32, origin: POINT) {
        let rect = ime.cursor.map(|caret| screen_rect(caret, dpi, origin));
        let (sink, changed) = {
            let mut state = self.store.borrow_mut();
            let moved = state.caret_rect != rect;
            state.caret_rect = rect;
            let changed = state.publish(ime) || moved;
            (state.sink.clone(), changed)
        };
        // An input method composes only where it has a layout to put a candidate window; the
        // callback re-enters the store, so the borrow above is released first. A frame that
        // changed neither text nor caret says nothing, or every still frame would repeat it.
        if changed
            && self.attached
            && let Some(sink) = sink
            && let Err(error) = unsafe { sink.OnLayoutChange(TS_LC_CHANGE, VIEW) }
        {
            log::warn!("tsf: the store could not report its layout: {error}");
        }
    }

    /// Forward a key to the input methods; true when one took it.
    pub(crate) fn key(&mut self, down: bool, wparam: usize, lparam: isize) -> bool {
        // An associated document that is not the thread's focus still receives keyboard
        // events through `TestKeyDown`; letting it take them would lose the key entirely.
        if !self.attached || unsafe { self.manager.GetFocus() }.ok().is_none() {
            return false;
        }
        let Ok(keys) = self.manager.cast::<ITfKeystrokeMgr>() else {
            return false;
        };
        let (message, place) = (WPARAM(wparam), LPARAM(lparam));
        let test = unsafe {
            if down {
                keys.TestKeyDown(message, place)
            } else {
                keys.TestKeyUp(message, place)
            }
        };
        let taken = match test {
            Ok(taken) if taken.as_bool() => {
                let handled = unsafe {
                    if down {
                        keys.KeyDown(message, place)
                    } else {
                        keys.KeyUp(message, place)
                    }
                };
                handled
                    .unwrap_or_else(|error| {
                        log::warn!("tsf: the input method could not handle the key: {error}");
                        BOOL(0)
                    })
                    .as_bool()
            }
            Ok(_) => false,
            Err(error) => {
                log::warn!("tsf: the input method could not see the key: {error}");
                false
            }
        };
        if taken && down {
            // `TranslateMessage` still turns the key into `WM_CHAR`, which the IME's own
            // composition must not also receive.
            self.swallow_char.set(true);
        }
        taken
    }

    /// Whether the pending `WM_CHAR` is the character of a key an input method took.
    pub(crate) fn take_swallowed_char(&self) -> bool {
        self.swallow_char.replace(false)
    }

    /// Engine events the store produced since the last call.
    pub(crate) fn take_events(&self) -> Vec<Event> {
        self.store.borrow_mut().take_pending()
    }

    /// Whether the document currently owns the window's input.
    pub(crate) fn is_attached(&self) -> bool {
        self.attached
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            let _ = self
                .manager
                .AssociateFocus(self.hwnd, None::<&ITfDocumentMgr>);
            let _ = self.document.Pop(0);
            let _ = self.manager.Deactivate();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_core::Preedit;

    fn state(text: &str, caret: usize) -> StoreState {
        let mut state = StoreState::new(HWND::default());
        state.publish(&ImeState {
            enabled: true,
            surrounding: Some((text.to_owned(), caret, caret)),
            ..ImeState::default()
        });
        state
    }

    /// A composition becomes the engine's preedit and its text a single commit; the preedit
    /// must clear before the commit lands at the caret.
    #[test]
    fn a_composition_is_one_preedit_and_its_end_one_commit() {
        let mut store = state("ab", 2);
        store.composition = Some((2, 2));
        store.replace(2, 2, "ni");
        store.composition = Some((2, 4));
        store.derive_preedit();
        assert_eq!(
            store.take_pending(),
            vec![Event::Preedit(Some(Preedit::new("ni", 2)))]
        );

        // The commit writes the final text inside the still-open composition, as TSF does,
        // and only then ends it.
        store.replace(2, 4, "\u{4f60}");
        store.end_composition();
        assert_eq!(
            store.take_pending(),
            vec![Event::Preedit(None), Event::Text("\u{4f60}".to_owned())]
        );
        assert_eq!(store.committed, "ab\u{4f60}");
        assert_eq!(store.committed_caret, "ab\u{4f60}".len());
    }

    /// An input method's request to remove text before the caret reaches the engine as byte
    /// counts around the caret, which is what the engine's own edit expects.
    #[test]
    fn a_removal_before_the_caret_asks_the_engine_for_surrounding_text() {
        let mut store = state("hello", 5);
        store.composition = Some((5, 5));
        store.replace(5, 5, "ni");
        store.composition = Some((5, 7));
        store.derive_preedit();
        let _ = store.take_pending();

        // Two characters before the composition: the IME's surrounding-text deletion.
        store.replace(3, 5, "");
        store.composition = Some((3, 5));
        assert_eq!(
            store.take_pending(),
            vec![Event::DeleteSurrounding {
                before: 2,
                after: 0
            }]
        );
        assert_eq!(store.committed, "hel");
        assert_eq!(store.committed_caret, 3);
    }

    /// The engine counts bytes and TSF counts code units; a multibyte removal must not turn
    /// three bytes into one.
    #[test]
    fn a_removal_counts_the_engines_bytes_not_tsfs_units() {
        let mut store = state("\u{4f60}\u{597d}", 6);
        // The composition sits at the engine caret, byte 6; `replace` is the TSF call and
        // speaks code units.
        store.composition = Some((6, 6));
        store.replace(2, 2, "a");
        let _ = store.take_pending();
        // Remove the second character, one code unit before the composition.
        store.replace(1, 2, "");
        assert_eq!(
            store.take_pending(),
            vec![Event::DeleteSurrounding {
                before: 3,
                after: 0
            }]
        );
    }

    /// ACP ranges are code units, and a caller that lands inside a surrogate pair must be
    /// snapped to a character boundary rather than panicking on the slice.
    #[test]
    fn a_range_inside_a_surrogate_pair_snaps_to_a_character_boundary() {
        let store = state("a\u{1f600}b", 0);
        let (low, high) = store.clamp_range(2, 4);
        assert!(store.text.is_char_boundary(low));
        assert!(store.text.is_char_boundary(high));
        // Unit 2 falls inside the pair, which starts at unit 1; the pair is never split.
        assert_eq!(&store.text[low..high], "\u{1f600}b");
        assert_eq!(store.acp(low), 1);
    }
}
