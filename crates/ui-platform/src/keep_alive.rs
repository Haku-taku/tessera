//! A claim that a consumer is still using a worker's cache.
//!
//! The idle sweep exists for memory a closed window left behind. Between frames an open
//! window also stops asking, and its cache would otherwise be dropped and every icon or
//! directory read again the moment the pointer moves. A [`KeepAlive`] states the difference:
//! the consumer is idle, not gone, so the sweep leaves the cache alone until the claim drops.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Keeps a worker's cache through its idle window for as long as it lives.
///
/// The bounds on entries, pictures and bytes still hold: a claim exempts the cache from the
/// timeout, not from eviction by size.
pub struct KeepAlive {
    held: Arc<AtomicUsize>,
}

impl KeepAlive {
    pub(crate) fn new(held: &Arc<AtomicUsize>) -> Self {
        held.fetch_add(1, Ordering::AcqRel);
        Self {
            held: Arc::clone(held),
        }
    }
}

impl Drop for KeepAlive {
    fn drop(&mut self) {
        self.held.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Whether an idle worker may drop its cache: nobody holds a claim.
pub(crate) fn released(held: &AtomicUsize) -> bool {
    held.load(Ordering::Acquire) == 0
}
