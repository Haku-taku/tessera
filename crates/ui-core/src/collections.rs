//! The hash containers and digests the engine uses internally.
//!
//! Every key here is produced by the engine itself — node ids, content digests, integer
//! buckets — so SipHash's collision resistance costs a frame real time and buys nothing.
//!
//! One module so the choice has exactly one home: switching hashers is editing this file.

/// A `HashMap` for engine-internal keys.
pub type FastMap<K, V> = std::collections::HashMap<K, V, rustc_hash::FxBuildHasher>;

/// A `HashSet` for engine-internal keys.
pub type FastSet<T> = std::collections::HashSet<T, rustc_hash::FxBuildHasher>;

/// A digest hasher, for the cases that hash content into one `u64` instead of keying a map.
pub type FastHasher = rustc_hash::FxHasher;
