//! Named colour roles held by name, not by struct field, so a style pack can add or
//! deprecate slots without touching every widget's code.

use ui_core::collections::FastMap;

use ui_core::geom::Color;

/// A role slot, interned by its own `&'static str`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RoleName(pub &'static str);

impl RoleName {
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

/// One theme's colour set: values plus the alias chain that migrates old names.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct RoleDict {
    values: FastMap<RoleName, Color>,
    // Aliases resolve at lookup, not at construction: a pack may define a role only
    // through a legacy name.
    aliases: FastMap<RoleName, RoleName>,
}

impl RoleDict {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_pairs(pairs: &[(RoleName, Color)]) -> Self {
        let mut dict = Self::new();
        for (name, color) in pairs {
            dict.set(*name, *color);
        }
        dict
    }

    pub fn set(&mut self, role: RoleName, color: Color) -> &mut Self {
        self.values.insert(role, color);
        self
    }

    /// Point an old role name at a live one, the way MD3 redirects deprecated getters.
    pub fn alias(&mut self, from: RoleName, to: RoleName) -> &mut Self {
        self.aliases.insert(from, to);
        self
    }

    /// Resolve a role, following at most a short alias chain; cycles yield `None`.
    pub fn get(&self, role: RoleName) -> Option<Color> {
        let mut current = role;
        for _ in 0..8 {
            if let Some(color) = self.values.get(&current) {
                return Some(*color);
            }
            current = *self.aliases.get(&current)?;
        }
        None
    }

    /// Overlay `other` on top of `self`: the higher layer wins per key.
    pub fn merged_with(&self, other: &Self) -> Self {
        let mut out = self.clone();
        for (role, color) in &other.values {
            out.values.insert(*role, *color);
        }
        for (from, to) in &other.aliases {
            out.aliases.insert(*from, *to);
        }
        out
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether the pack declares no role at all; every lookup on it misses.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Every concrete role name (alias keys excluded); used to blend two packs.
    pub fn names(&self) -> impl Iterator<Item = RoleName> + '_ {
        self.values.keys().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role(name: &'static str) -> RoleName {
        RoleName(name)
    }

    fn gray(v: u8) -> Color {
        Color::from_rgba8(v, v, v, 255)
    }

    #[test]
    fn an_alias_cycle_resolves_to_nothing_instead_of_hanging() {
        let mut dict = RoleDict::new();
        dict.alias(role("a"), role("b"));
        dict.alias(role("b"), role("a"));
        assert_eq!(dict.get(role("a")), None);
    }

    #[test]
    fn merging_overlays_only_the_keys_the_upper_layer_names() {
        let base =
            RoleDict::with_pairs(&[(role("primary"), gray(10)), (role("surface"), gray(20))]);
        let over = RoleDict::with_pairs(&[(role("primary"), gray(99))]);
        let merged = base.merged_with(&over);
        assert_eq!(merged.get(role("primary")), Some(gray(99)));
        assert_eq!(merged.get(role("surface")), Some(gray(20)));
        assert_eq!(
            merged.len(),
            2,
            "overlaying an existing key must not add one"
        );
    }
}
