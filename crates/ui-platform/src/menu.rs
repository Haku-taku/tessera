//! The engine's fallback context menu: what a secondary click collects and how it is drawn.
//!
//! A design system replaces the look through [`MenuStyle`]; the engine still owns the
//! collection, the placement, the hit test and the report.

use ui_core::Id;
use ui_core::geom::{Color, Corners, Point, Rect, Size};
use ui_core::menu::MenuItem;
use ui_core::text::{TextBlock, TextStyle, Weight};

/// How the fallback menu looks.
#[derive(Clone, Debug)]
pub struct MenuStyle {
    pub surface: Color,
    pub ink: Color,
    /// Rows that cannot be chosen.
    pub muted: Color,
    pub hover: Color,
    /// `None` paints no border; MD3's menu is bounded by its shadow alone.
    pub border: Option<(Color, f32)>,
    pub font_size: f32,
    /// The ink, size and weight of the chord shown at a row's trailing edge.
    pub shortcut_ink: Color,
    pub shortcut_size: f32,
    pub shortcut_weight: Weight,
    /// Space between a label and its shortcut, and the inset from the trailing edge.
    pub shortcut_gap: f32,
    pub trailing_space: f32,
    pub row_height: f32,
    /// A label's inset from the container's edge; a chord mirrors it on the trailing side.
    pub pad_x: f32,
    pub pad_y: f32,
    /// The horizontal inset of a row's own shape: its hover fill and hit box shrink by this
    /// much on each side, so a row reads as a shape inside the container.
    pub row_inset: f32,
    pub radius: f32,
    pub min_width: f32,
    /// The widest the menu gets before its labels wrap; MD3 caps it.
    pub max_width: f32,
    pub shadow: f32,
    /// A frost behind the surface, which then needs a translucent fill to show it.
    pub frost: Option<ui_core::BackdropBlur>,
}

impl Default for MenuStyle {
    fn default() -> Self {
        let ink = Color::from_rgba8(232, 232, 236, 255);
        Self {
            surface: Color::from_rgba8(44, 46, 54, 255),
            ink,
            muted: Color::from_rgba8(140, 142, 150, 255),
            hover: Color::from_rgba8(255, 255, 255, 18),
            border: Some((Color::from_rgba8(255, 255, 255, 30), 1.0)),
            font_size: 14.0,
            // The chord is the row's own ink at reduced emphasis, so it follows whatever
            // theme supplied that ink instead of carrying a colour of its own.
            shortcut_ink: Color {
                a: ink.a * 0.7,
                ..ink
            },
            shortcut_size: 12.0,
            shortcut_weight: Weight::MEDIUM,
            shortcut_gap: 16.0,
            trailing_space: 12.0,
            row_height: 34.0,
            pad_x: 14.0,
            pad_y: 6.0,
            row_inset: 4.0,
            radius: 8.0,
            min_width: 180.0,
            max_width: f32::INFINITY,
            shadow: 10.0,
            frost: None,
        }
    }
}

impl MenuStyle {
    /// The text block one row's label is shaped as.
    pub fn block(&self, label: &str) -> TextBlock {
        TextBlock::plain(label, TextStyle::new(self.font_size))
    }

    /// The text block one row's shortcut is shaped as.
    pub fn shortcut_block(&self, shortcut: &str) -> TextBlock {
        TextBlock::plain(
            shortcut,
            TextStyle::new(self.shortcut_size).weight(self.shortcut_weight),
        )
    }
}

/// One built-in text row's words: what it says and, if it has one, the chord it runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuRowText {
    pub label: String,
    /// Already in the app's words; the engine only right-aligns it.
    pub shortcut: Option<String>,
}

/// The app's words for the engine's built-in text rows.
///
/// The engine owns *which* rows exist and whether each is enabled; an app owns every word.
/// Until an app supplies these, the engine contributes no text rows at all — it never
/// invents text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextMenuLabels {
    pub select_all: MenuRowText,
    pub copy: MenuRowText,
    pub cut: MenuRowText,
    pub paste: MenuRowText,
}

/// One open menu, owned by the engine until it closes.
#[derive(Clone, Debug)]
pub(crate) struct OpenMenu {
    /// The node whose menu this is; the pick is reported to it.
    pub owner: Id,
    pub items: Vec<MenuItem>,
    pub rect: Rect,
    /// One rect per item, top to bottom.
    pub rows: Vec<Rect>,
    /// Where each row's shortcut sits, right-aligned; `None` for a row without one.
    pub shortcuts: Vec<Option<Rect>>,
    pub hovered: Option<usize>,
    /// The point that opened it; the entrance travels toward it.
    pub at: Point,
    /// The engine clock when it opened: the entrance's zero.
    pub opened_at: f64,
}

/// WinUI's `ControlFastAnimationDuration`, decelerating on (0,0,0,1).
const ENTRANCE: ui_core::anim::AnimSpec =
    ui_core::anim::AnimSpec::new(0.167, ui_core::anim::Easing::Cubic(0.0, 0.0, 0.0, 1.0));
/// The surface starts half its own height toward the pointer, then travels to rest.
const TRAVEL: f32 = 0.5;

impl OpenMenu {
    /// Place the menu at `at`, kept inside the surface.
    pub fn open(
        style: &MenuStyle,
        owner: Id,
        items: Vec<MenuItem>,
        at: Point,
        opened_at: f64,
        surface: Size,
        mut width_of: impl FnMut(&TextBlock) -> f32,
    ) -> Self {
        let widest = items
            .iter()
            .map(|item| {
                let shortcut = item.shortcut.as_deref().map_or(0.0, |s| {
                    style.shortcut_gap + width_of(&style.shortcut_block(s))
                });
                width_of(&style.block(&item.label))
                    + shortcut
                    + style.pad_x
                    + style.pad_x.max(style.trailing_space)
            })
            .fold(style.min_width, f32::max);
        let width = widest
            .min(style.max_width)
            .min((surface.w - 8.0).max(style.min_width));
        let height = items.len() as f32 * style.row_height + 2.0 * style.pad_y;
        // A menu near the edge opens inward rather than off-screen.
        let x = at.x.min(surface.w - width - 4.0).max(4.0);
        let y = at.y.min(surface.h - height - 4.0).max(4.0);
        let rect = Rect::new(x, y, width, height);
        // The row is inset so its own rounded shape reads inside the container's.
        let rows = (0..items.len())
            .map(|i| {
                Rect::new(
                    x + style.row_inset,
                    y + style.pad_y + i as f32 * style.row_height,
                    width - 2.0 * style.row_inset,
                    style.row_height,
                )
            })
            .collect();
        // Shortcuts are right-aligned against the container's own trailing inset.
        let shortcuts = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let shortcut = item.shortcut.as_deref()?;
                let w = width_of(&style.shortcut_block(shortcut));
                Some(Rect::new(
                    x + width - style.trailing_space - w,
                    y + style.pad_y + i as f32 * style.row_height,
                    w,
                    style.row_height,
                ))
            })
            .collect();
        Self {
            owner,
            items,
            rect,
            rows,
            shortcuts,
            hovered: None,
            at,
            opened_at,
        }
    }

    /// How far the entrance has come: presence `0..=1` and the vertical shift to draw with.
    pub fn entrance(&self, now: f64) -> (f32, f32) {
        let presence = ENTRANCE.eased_at(self.opened_at, now);
        // A menu below the pointer unfolds downward out of its top edge, and vice versa.
        let toward_pointer = match self.rect.center().y < self.at.y {
            true => 1.0,
            false => -1.0,
        };
        (
            presence,
            (1.0 - presence) * self.rect.size.h * TRAVEL * toward_pointer,
        )
    }

    /// The row under `p`, if the menu is even hit, as it is drawn at `now`.
    pub fn row_at(&self, p: Point, now: f64) -> Option<usize> {
        let (_, shift) = self.entrance(now);
        if !self.rect.translate(Point::new(0.0, shift)).contains(p) {
            return None;
        }
        self.rows
            .iter()
            .position(|row| row.translate(Point::new(0.0, shift)).contains(p))
    }

    /// The enabled row under `p`.
    pub fn enabled_row_at(&self, p: Point, now: f64) -> Option<usize> {
        self.row_at(p, now)
            .filter(|i| self.items.get(*i).is_some_and(|item| item.enabled))
    }

    /// The enabled row whose chord was pressed, as the menu was opened.
    ///
    /// A row that names every modifier it needs outranks one that leaves a modifier free,
    /// as the binding tables resolve the same press.
    pub fn accelerator_at(&self, key: ui_core::Key, mods: ui_core::Modifiers) -> Option<usize> {
        let mut best: Option<(u32, usize)> = None;
        for (index, item) in self.items.iter().enumerate() {
            if !item.enabled {
                continue;
            }
            let Some(chord) = item.accelerator.filter(|chord| chord.matches(key, mods)) else {
                continue;
            };
            if best.is_none_or(|(seen, _)| chord.freedom() < seen) {
                best = Some((chord.freedom(), index));
            }
        }
        best.map(|(_, index)| index)
    }

    pub fn corners(style: &MenuStyle) -> Corners {
        Corners::all(style.radius)
    }
}
