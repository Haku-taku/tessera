//! The semantic specs a style is asked to turn into concrete controls.

use ui_core::Id;

/// One slider request: a value and a label, never geometry.
#[derive(Clone, Copy, Debug)]
pub struct SliderSpec<'a> {
    pub id: Id,
    /// Normalised 0..=1 value; the app owns the range and maps it.
    pub value: f32,
    pub label: &'a str,
}

/// One toggle request: the semantic bit and a label, never geometry.
#[derive(Clone, Copy, Debug)]
pub struct ToggleSpec<'a> {
    pub id: Id,
    pub on: bool,
    pub label: &'a str,
}

impl<'a> SliderSpec<'a> {
    pub fn new(id: Id, value: f32, label: &'a str) -> Self {
        Self { id, value, label }
    }
}

impl<'a> ToggleSpec<'a> {
    pub fn new(id: Id, on: bool, label: &'a str) -> Self {
        Self { id, on, label }
    }
}
