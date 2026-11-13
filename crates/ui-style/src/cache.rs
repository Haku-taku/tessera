//! Memoizes resolved styles by the context they resolve for.
//!
//! Building a pack runs the system's whole pipeline (tonal palettes, 49 roles, 22 common
//! roles); a page with thirty controls would otherwise pay that thirty times per frame.

use std::rc::Rc;
use std::sync::Mutex;

use ui_design::Appearance;

use crate::env::StyleScope;
use crate::pack::Style;

/// The context a style was resolved for: appearance, what it fades from, and the step of
/// that fade. Quantized to 1/256 so a floating-point wobble cannot miss the cache.
type Key = (Appearance, Option<Appearance>, u8);

/// A bounded memo table. Not keyed by frame: the key is every input that can change the
/// result, so an entry never goes stale.
pub struct StyleCache {
    entries: Mutex<Vec<(Key, Rc<Style>)>>,
}

/// A transition sweeps `t` across its range; past this many entries the table is dropped
/// rather than grown, because the entries are cheap to rebuild.
const MAX_ENTRIES: usize = 64;

impl StyleCache {
    pub const fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }

    /// The style for this context, resolved at most once per quantized step.
    pub fn get(&self, scope: StyleScope, resolve: fn(Appearance) -> Style) -> Rc<Style> {
        let scope = scope.settled();
        let key: Key = (
            scope.appearance,
            scope.theme_from,
            (scope.theme_t * 255.0).round() as u8,
        );
        if let Some(hit) = self.lookup(key) {
            return hit;
        }
        let style = Rc::new(Style::resolve(scope, resolve));
        self.store(key, &style);
        style
    }

    fn lookup(&self, key: Key) -> Option<Rc<Style>> {
        let entries = self.entries.lock().ok()?;
        entries
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, s)| Rc::clone(s))
    }

    fn store(&self, key: Key, style: &Rc<Style>) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        if entries.len() >= MAX_ENTRIES {
            entries.clear();
        }
        entries.push((key, Rc::clone(style)));
    }
}

impl Default for StyleCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::stub_type_table;
    use crate::roles::{RoleDict, RoleName};
    use crate::types::MetricsSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use ui_core::geom::Color;

    /// A pack distinguishable by the colour of one role, so a cache hit is a value, not a proxy.
    fn named(appearance: Appearance) -> Style {
        let ink = match appearance {
            Appearance::Dark => Color::from_rgba8(0, 0, 0, 255),
            Appearance::Light => Color::from_rgba8(255, 255, 255, 255),
        };
        let mut roles = RoleDict::new();
        roles.set(RoleName("which"), ink);
        Style {
            common: None,
            roles,
            motion: crate::motion::MotionTable::legacy(),
            metrics: MetricsSet::NONE,
            controls: crate::ControlMetricsTable::EMPTY,
            type_table: stub_type_table(),
            material: None,
            device: crate::capability::Capability::GPU_BASELINE,
            palette: None,
            selection: None,
        }
    }

    // The whole point: thirty controls in one frame must not build thirty packs.
    #[test]
    fn a_repeated_context_resolves_only_once() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        fn resolve(a: Appearance) -> Style {
            CALLS.fetch_add(1, Ordering::SeqCst);
            named(a)
        }
        let cache = StyleCache::new();
        let first = cache.get(StyleScope::new(Appearance::Light), resolve);
        let second = cache.get(StyleScope::new(Appearance::Light), resolve);
        assert_eq!(
            first.ink("which"),
            Some(Color::from_rgba8(255, 255, 255, 255))
        );
        assert!(
            std::rc::Rc::ptr_eq(&first, &second),
            "one context hands out one pack, not a copy of it"
        );
        assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    }

    // Two appearances must not collide on one entry, or a theme switch shows the old colours.
    #[test]
    fn another_appearance_is_a_different_entry() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        fn resolve(a: Appearance) -> Style {
            CALLS.fetch_add(1, Ordering::SeqCst);
            named(a)
        }
        let cache = StyleCache::new();
        let light = cache.get(StyleScope::new(Appearance::Light), resolve);
        let dark = cache.get(StyleScope::new(Appearance::Dark), resolve);
        assert_eq!(
            light.ink("which"),
            Some(Color::from_rgba8(255, 255, 255, 255))
        );
        assert_eq!(dark.ink("which"), Some(Color::from_rgba8(0, 0, 0, 255)));
        assert_eq!(CALLS.load(Ordering::SeqCst), 2);
    }
}
