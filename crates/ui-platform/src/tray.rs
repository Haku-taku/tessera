//! The system tray: an icon in the desktop's notification area, its tooltip and its menu.
//!
//! Every desktop says this differently — Windows' `Shell_NotifyIcon`, macOS' `NSStatusItem`,
//! Linux's `org.kde.StatusNotifierItem` over D-Bus — and they agree on one shape: a picture,
//! a tooltip, a menu the desktop draws natively, and a report when the user clicks the icon
//! or picks a row. That shape is what this module exposes; the desktop decides how it looks.
//!
//! The abstraction stops where the desktops stop agreeing. It has no balloon notifications
//! (Windows' `NIF_INFO`, Linux's `org.freedesktop.Notifications`, a separate permission on
//! macOS), no title beside the icon (Linux has one, Windows and macOS do not), no attention
//! state, and no hover reports. An app that needs one of those asks a platform type, not this
//! module.

use ui_core::Id;
use ui_core::event::PointerButton;
use ui_core::geom::Point;
use ui_core::menu::MenuItem;

/// A picture for the tray, as the app supplies it.
///
/// Straight (non-premultiplied) sRGB with an alpha channel, top row first. This is what an
/// image file decodes to, which is the form every framework takes: Qt's `QIcon`, Electron's
/// `NativeImage`, tray-icon's `Icon::from_rgba`.
///
/// Supply the size the desktop should draw, 32×32 or larger: Windows scales the picture for
/// the notification area at the display's scale, and a 16×16 source scales badly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Icon {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Icon {
    /// The icon `rgba` draws, four bytes per pixel.
    pub fn from_rgba(rgba: Vec<u8>, width: u32, height: u32) -> Result<Self, String> {
        let expected = width as usize * height as usize * 4;
        if width == 0 || height == 0 {
            return Err("a tray icon needs a width and a height".into());
        }
        if rgba.len() != expected {
            return Err(format!(
                "a {width}×{height} tray icon needs {expected} bytes, not {}",
                rgba.len()
            ));
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
}

/// One open menu, as the app hands it over.
///
/// The desktop draws it, so the rows are its own: a right-click on the icon opens the menu in
/// the shell's own style, beside the shell's own window. This is why the menu is its own model
/// rather than the engine-drawn [`MenuStyle`](crate::menu::MenuStyle) a surface shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Menu {
    pub entries: Vec<MenuEntry>,
}

impl Menu {
    pub fn new(entries: Vec<MenuEntry>) -> Self {
        Self { entries }
    }
}

/// One row of a tray menu.
///
/// A leaf row is a [`MenuItem`], the same row a surface's context menu declares, so an app
/// that builds both uses one vocabulary. The variants add what a desktop menu has and a
/// surface menu does not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    /// A row that reports its id through [`TrayEvent::Menu`] when chosen.
    Item(MenuItem),
    /// A row with a check mark. The app owns the state: after the row is chosen it hands a
    /// new menu over with [`Tray::set_menu`], and the next opening shows the new mark.
    Check { item: MenuItem, checked: bool },
    /// A row that opens `entries` beside it.
    Submenu {
        label: String,
        enabled: bool,
        entries: Vec<MenuEntry>,
    },
    /// A divider between rows.
    Separator,
}

impl MenuEntry {
    /// A row that reports `id` when chosen.
    pub fn item(id: Id, label: impl Into<String>) -> Self {
        Self::Item(MenuItem::new(id, label))
    }

    /// A row with a check mark beside it.
    pub fn check(id: Id, label: impl Into<String>, checked: bool) -> Self {
        Self::Check {
            item: MenuItem::new(id, label),
            checked,
        }
    }

    /// A row that opens `entries`.
    pub fn submenu(label: impl Into<String>, entries: Vec<MenuEntry>) -> Self {
        Self::Submenu {
            label: label.into(),
            enabled: true,
            entries,
        }
    }

    /// A divider between rows.
    pub const fn separator() -> Self {
        Self::Separator
    }
}

/// What the user did to a tray icon.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrayEvent {
    /// A full press and release on the icon.
    Click { button: PointerButton, at: Point },
    /// A second click on the icon, as the desktop counts one. Only the primary button has
    /// this on every desktop, so it carries no button.
    DoubleClick { at: Point },
    /// A row of the icon's menu was chosen.
    ///
    /// A row inside a submenu reports its own id; the rows that open submenus report nothing.
    Menu { id: Id },
}

/// Which tray icon an event or a command is about.
///
/// Stable for the icon's whole life, so an app with two icons can tell them apart without
/// holding the platform's handle inside its own state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TrayId(u64);

impl TrayId {
    pub const fn from_u64(value: u64) -> Self {
        Self(value)
    }

    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// The platform's icon, behind [`Tray`].
///
/// The application never names this trait: `Host::tray` returns a `Tray`, which forwards to
/// it. Dropping the platform object takes the icon out of the tray.
pub trait TrayHandle {
    /// Show `icon` in place of the current picture.
    ///
    /// Returns the platform's reason when it cannot make the picture; the icon keeps the one
    /// it had.
    fn set_icon(&mut self, icon: Icon) -> Result<(), String>;

    /// The tooltip the desktop shows when the pointer rests on the icon. Empty shows none.
    fn set_tooltip(&mut self, tooltip: &str);

    /// The menu a right-click opens; `None` opens nothing and reports the click alone.
    fn set_menu(&mut self, menu: Option<Menu>);

    /// Whether the icon is in the notification area at all. A hidden icon reports nothing.
    fn set_visible(&mut self, visible: bool);
}

/// An icon in the notification area, as the application holds it.
///
/// This is the whole of the application's side of the tray: create it once, then set what
/// changed. What the user does reaches the app through `Host::on_tray`, not through this
/// handle, because the loop owns the wait the report arrives on.
pub struct Tray {
    id: TrayId,
    handle: Box<dyn TrayHandle>,
}

impl Tray {
    pub(crate) fn new(id: TrayId, handle: Box<dyn TrayHandle>) -> Self {
        Self { id, handle }
    }

    /// The id the app's tray events carry.
    pub fn id(&self) -> TrayId {
        self.id
    }

    /// Show `icon` in place of the current picture.
    pub fn set_icon(&mut self, icon: Icon) -> Result<(), String> {
        self.handle.set_icon(icon)
    }

    /// The tooltip the desktop shows when the pointer rests on the icon. Empty shows none.
    pub fn set_tooltip(&mut self, tooltip: &str) {
        self.handle.set_tooltip(tooltip)
    }

    /// The menu a right-click opens; `None` opens nothing and reports the click alone.
    pub fn set_menu(&mut self, menu: Option<Menu>) {
        self.handle.set_menu(menu)
    }

    /// Whether the icon is in the notification area at all.
    pub fn set_visible(&mut self, visible: bool) {
        self.handle.set_visible(visible)
    }
}
