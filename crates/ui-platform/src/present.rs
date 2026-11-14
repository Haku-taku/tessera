//! Presentation targets: where rendered pixels go.

use ui_core::Damage;
use ui_core::geom::Rect;
use ui_paint::Pixels;

/// What kind of surface we are painting into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceKind {
    /// A regular window.
    Window,
    /// A full-screen overlay layer.
    LayerSurface,
    /// Offscreen, for tests and export.
    Offscreen,
}

/// A target that accepts rendered pixels.
pub trait PresentTarget {
    fn kind(&self) -> SurfaceKind;
    fn size(&self) -> (u32, u32);
    /// Pixel buffer to render into.
    fn pixels_mut(&mut self) -> &mut Pixels;
    /// Commit `damage` to the screen (or to disk, or nowhere).
    fn present(&mut self, damage: &Damage) -> Result<(), String>;
    /// Apply the pointer shape this frame resolved. The cursor is a seat property that the
    /// client must re-assert on every enter, so a target that never sets it keeps whatever
    /// the previous application left behind.
    fn set_cursor(&mut self, _shape: ui_core::CursorShape) {}

    /// Publish where this frame's tree consumes input, in surface-local logical units.
    ///
    /// The engine derives it from the hit index, so a target that owns an input region
    /// never has to be told by the app what is interactive. A target without one ignores
    /// the list: a plain window's shape belongs to the window manager.
    fn set_input_region(&mut self, _areas: &[Rect]) {}

    /// The pointer is over an area this surface consumes, or a gesture it owns is live.
    ///
    /// A target whose window follows the pointer — a layer surface that covers more than it
    /// claims — puts the keyboard on that fact; every other target ignores it.
    fn set_pointer_inside(&mut self, _inside: bool) {}

    /// Apply this frame's input-method state. A target that never sets it simply never
    /// enables text input, which is the safe default for surfaces with no IME.
    fn set_ime(&mut self, _state: ImeState) {}
}

/// What a presented scene actually did, for the caller's own diagnostics.
#[derive(Clone, Copy, Debug, Default)]
pub struct PresentStats {
    pub draw_calls: u32,
    /// The redrawn band in physical pixels, when the target knows it.
    pub band: Option<[i32; 4]>,
}

/// One canvas as a presenter receives it: its primitives already resolved to glyphs, in the
/// canvas's own pixels. Only a changed canvas carries a list; an unchanged one names the
/// target the presenter already holds.
#[derive(Clone)]
pub struct CanvasFrame {
    pub id: ui_core::draw::CanvasId,
    pub size: (u32, u32),
    pub version: u64,
    /// Whether the target has to be painted before this frame uses it.
    pub changed: bool,
    pub list: ui_core::draw::DrawList,
}

/// A target that takes a whole frame: upload the atlas, draw the scene, commit.
///
/// The engine decides when a frame is worth presenting and hands over only what the frame
/// produced, so an app never drives an upload, a draw or a commit itself.
pub trait SceneTarget {
    /// Surface size in physical pixels.
    fn size(&self) -> (u32, u32);

    /// Draw `scene` with `text`'s atlas and commit it; `damage` is what changed. `canvases`
    /// are this frame's targets: paint the changed ones before `scene`, which samples them.
    fn present_scene(
        &mut self,
        scene: &ui_core::draw::DrawList,
        text: &crate::text_pass::TextPass,
        damage: &Damage,
        canvases: &[CanvasFrame],
    ) -> Result<PresentStats, String>;

    /// Apply the pointer shape this frame resolved.
    fn set_cursor(&mut self, _shape: ui_core::CursorShape) {}

    /// Publish where this frame's tree consumes input, in surface-local logical units.
    ///
    /// Pending surface state: the frame's own commit carries it. Where the frame draws
    /// nothing, a target that applied a change reports it through [`Self::commit_pending`].
    fn set_input_region(&mut self, _areas: &[Rect]) {}

    /// The pointer is over an area this surface consumes, or a gesture it owns is live.
    /// See [`PresentTarget::set_pointer_inside`].
    fn set_pointer_inside(&mut self, _inside: bool) {}

    /// Apply this frame's input-method state.
    fn set_ime(&mut self, _state: ImeState) {}

    /// Whether the surface owes a commit beyond the frame's own damage (input method state,
    /// for one): the engine must not skip a present that would carry it.
    fn commit_pending(&self) -> bool {
        false
    }
}

/// What a compositor needs to place an input method's candidate window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImeState {
    /// Enable text input only when `cursor` came from this frame; see
    /// `docs/ime-text-design.md` §6 for the stale-rectangle bug this prevents.
    pub enabled: bool,
    /// The caret in surface coordinates, logical units.
    pub cursor: Option<ui_core::geom::Rect>,
    /// The buffer is secret: the input method is told so, and its surrounding text withheld.
    pub hidden: bool,
    /// What the input is for; a secret buffer is a password whatever this says.
    pub purpose: ui_core::ime::ContentPurpose,
    /// The text around the caret with the caret and anchor offsets into it; the preedit is
    /// excluded, as the protocol requires, and the window is bounded by `surrounding_window`.
    pub surrounding: Option<(String, usize, usize)>,
}

/// The longest surrounding text the text-input protocol carries in one request, in bytes.
///
/// `zwp_text_input_v3.set_surrounding_text` states the limit: "text can not be longer than 4000
/// bytes", because one Wayland message is capped. Over it libwayland aborts the connection.
const SURROUNDING_LIMIT: usize = 4000;

/// The caret's neighbourhood, with the caret and anchor rebased onto it.
///
/// Follows GTK's rule: the cursor and as much of the selection as fits, with text on both
/// sides where the buffer has it. The window is snapped so it never splits a character, and
/// an anchor outside it clamps to the window's edge. Offsets stay byte indices into the text.
pub fn surrounding_window(value: &str, caret: usize, anchor: usize) -> (String, usize, usize) {
    let caret = caret.min(value.len());
    let anchor = anchor.min(value.len());
    let (low, high) = (caret.min(anchor), caret.max(anchor));
    // Half the budget each way, reaching back for the selection's far end when it fits.
    let centered = caret.saturating_sub(SURROUNDING_LIMIT / 2);
    let mut start = centered.min(low);
    if high - start > SURROUNDING_LIMIT {
        // A selection longer than the whole budget: keep the caret's own surroundings.
        start = centered;
    }
    while start > 0 && !value.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (start + SURROUNDING_LIMIT).min(value.len());
    // Snapping down, not up, is what keeps the window inside the protocol's limit.
    while end > start && !value.is_char_boundary(end) {
        end -= 1;
    }
    let window = value[start..end].to_owned();
    let at = caret - start;
    let anchor = anchor.clamp(start, end) - start;
    (window, at, anchor)
}

#[cfg(test)]
mod tests {
    use super::surrounding_window;

    /// A whole document in one request killed the connection; the window must stay inside the
    /// protocol's limit and still name the caret's own byte.
    #[test]
    fn a_long_buffers_window_stays_one_request_and_keeps_the_caret() {
        let value = "x".repeat(9000);
        let (text, caret, anchor) = surrounding_window(&value, 5000, 5000);
        assert!(
            text.len() <= super::SURROUNDING_LIMIT,
            "the window is a request, not the file: {} bytes",
            text.len()
        );
        assert_eq!(anchor, caret, "a collapsed selection stays collapsed");
        assert_eq!(
            text[..caret],
            value[..5000][value[..5000].len() - caret..],
            "the bytes before the caret are the buffer's own"
        );
        assert!(
            value.ends_with(&text[caret..]) || text[caret..].is_empty(),
            "and so are the bytes after it"
        );
    }

    /// GTK's rule: some text before the cursor and some after it, when the buffer has both.
    #[test]
    fn a_window_keeps_text_on_both_sides_of_the_caret() {
        let value = "z".repeat(9000);
        let (text, caret, _) = surrounding_window(&value, 4500, 4500);
        assert!(
            caret > 0 && caret < text.len(),
            "the caret sits inside the window"
        );
        assert_eq!(text.len(), super::SURROUNDING_LIMIT);
    }

    /// A window that cut a character in half would send invalid text and offsets into it.
    #[test]
    fn a_window_never_splits_a_character() {
        let value = "中".repeat(3000);
        let (text, caret, anchor) = surrounding_window(&value, 4497, 4497);
        assert!(
            text.is_char_boundary(caret),
            "the caret lands on a character"
        );
        assert_eq!(anchor, caret);
        assert!(text.chars().all(|c| c == '中'), "only whole characters");
        assert_eq!(text.len() % 3, 0);
    }

    /// A selection longer than a window still names a selection: the anchor clamps to the
    /// window's edge rather than pointing past its bytes.
    #[test]
    fn an_anchor_outside_the_window_clamps_to_its_edge() {
        let value = "y".repeat(6000);
        let (text, caret, anchor) = surrounding_window(&value, 4000, 0);
        assert_eq!(anchor, 0, "the far anchor sits at the window's start");
        assert!(caret > anchor && caret <= text.len());
    }
}

/// An offscreen target holding pixels in memory.
#[derive(Debug)]
pub struct OffscreenTarget {
    pub pixels: Pixels,
}

impl OffscreenTarget {
    pub fn new(width: u32, height: u32) -> Self {
        let mut pixels = Pixels::new(width, height);
        pixels.fill(ui_core::geom::Color::from_rgba8(28, 30, 34, 255));
        Self { pixels }
    }
}

impl PresentTarget for OffscreenTarget {
    fn kind(&self) -> SurfaceKind {
        SurfaceKind::Offscreen
    }

    fn size(&self) -> (u32, u32) {
        (self.pixels.width, self.pixels.height)
    }

    fn pixels_mut(&mut self) -> &mut Pixels {
        &mut self.pixels
    }

    fn present(&mut self, _damage: &Damage) -> Result<(), String> {
        Ok(())
    }
}

/// Where a target's content currently sits inside the surface.
pub fn full_rect(size: (u32, u32)) -> Rect {
    Rect::new(0.0, 0.0, size.0 as f32, size.1 as f32)
}
