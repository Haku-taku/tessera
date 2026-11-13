//! One `timed!` for every crate that profiles frames.

/// Times a block with hotpath under the caller's `profile` feature, and is the block otherwise.
///
/// `cfg` is evaluated where the macro expands, so the label costs nothing unless the crate
/// calling it was built with `profile`.
#[macro_export]
macro_rules! timed {
    ($label:literal, $body:expr) => {{
        #[cfg(feature = "profile")]
        {
            hotpath::measure_block!($label, $body)
        }
        #[cfg(not(feature = "profile"))]
        {
            $body
        }
    }};
}
