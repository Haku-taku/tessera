//! The parameter bag controls pass to a message, without naming Fluent in their signatures.

use fluent_bundle::FluentArgs;

/// A value a message argument accepts: numbers and strings.
pub use fluent_bundle::FluentValue as Value;

/// Anything Fluent can read as a value; implemented for the types callers actually pass.
pub trait IntoArg<'a> {
    /// Convert into Fluent's own value type.
    #[doc(hidden)]
    fn into_value(self) -> Value<'a>;
}

impl<'a, V: Into<Value<'a>>> IntoArg<'a> for V {
    fn into_value(self) -> Value<'a> {
        self.into()
    }
}

/// Named arguments for one parameterized message.
pub struct Args<'a> {
    inner: FluentArgs<'a>,
}

impl<'a> Args<'a> {
    pub fn new() -> Self {
        Self {
            inner: FluentArgs::new(),
        }
    }

    /// Set `name` to `value`; numbers select plural categories, strings feed `select`.
    pub fn set<V: IntoArg<'a>>(&mut self, name: &'a str, value: V) {
        self.inner.set(name, value.into_value());
    }

    /// Read an argument back, if it was set.
    pub fn get(&self, name: &'a str) -> Option<&Value<'a>> {
        self.inner.get(name)
    }

    pub(crate) fn inner(&self) -> &FluentArgs<'a> {
        &self.inner
    }
}

impl Default for Args<'_> {
    fn default() -> Self {
        Self::new()
    }
}
