//! Stable widget identity.

use core::fmt;
use core::hash::{Hash, Hasher};

/// A user-supplied, stable identifier.
///
/// Users only ever hand the engine an `Id`; the engine owns `NodeId`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Id(u64);

impl Id {
    pub const fn from_u64(v: u64) -> Self {
        Self(v)
    }

    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Derive a child id from a parent id and a salt.
    ///
    /// Uses FNV-1a over the two halves, so it is stable across runs and platforms.
    pub fn with(self, salt: impl Hash) -> Self {
        let mut h = Fnv1a::new();
        self.0.hash(&mut h);
        salt.hash(&mut h);
        Self(h.finish())
    }

    /// Derive an id from any hashable tuple.
    pub fn of(v: impl Hash) -> Self {
        let mut h = Fnv1a::new();
        v.hash(&mut h);
        Self(h.finish())
    }

    /// Mnemonic id for `&str`, handy in examples.
    pub const fn str(s: &'static str) -> Self {
        // Compile-time FNV-1a over the bytes.
        let bytes = s.as_bytes();
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut i = 0;
        while i < bytes.len() {
            hash ^= bytes[i] as u64;
            hash = hash.wrapping_mul(0x1000_0000_01b3);
            i += 1;
        }
        Self(hash)
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({:#018x})", self.0)
    }
}

/// Minimal FNV-1a hasher; no external dependency and no per-run seed.
struct Fnv1a(u64);

impl Fnv1a {
    const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    const fn finish(self) -> u64 {
        self.0
    }
}

impl Hasher for Fnv1a {
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= *b as u64;
            self.0 = self.0.wrapping_mul(0x1000_0000_01b3);
        }
    }

    fn write_u64(&mut self, v: u64) {
        self.write(&v.to_le_bytes());
    }

    fn write_u32(&mut self, v: u32) {
        self.write(&v.to_le_bytes());
    }

    fn write_usize(&mut self, v: usize) {
        self.write(&(v as u64).to_le_bytes());
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// Engine-internal node handle with a generation counter.
///
/// A recycled slot bumps its generation, so a stale `NodeId` reads as absent
/// instead of silently pointing at a new node.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NodeId {
    pub(crate) index: u32,
    pub(crate) generation: u32,
}

impl NodeId {
    /// Construct a handle from raw parts. Test-only; production code gets ids from
    /// the tree.
    pub fn from_raw(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// Slot index, for arena access.
    pub const fn index(self) -> usize {
        self.index as usize
    }

    /// Generation counter, exposed for tests and diagnostics.
    pub const fn generation(self) -> u32 {
        self.generation
    }
}
