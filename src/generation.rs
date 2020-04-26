use std::num::NonZeroU64;

/// Opaque generation index.
/// Has niche for enums.
#[derive(Copy, Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Generation(NonZeroU64);

impl Generation {
    /// Creates new opaque generation value.
    ///
    /// This function has two constraints:
    ///
    /// * `Generation::new() == Generation::new()`
    /// * `Generation::new() == GenerationCounter::new().get()`
    pub fn new() -> Self {
        Generation(unsafe {
            // It is safe to create from literal `1`.
            NonZeroU64::new_unchecked(1)
        })
    }
}

/// Opaque generation counter.
#[derive(Debug)]
#[repr(transparent)]
pub struct GenerationCounter(u64);

impl GenerationCounter {
    /// Creates new generation counter.
    pub fn new() -> Self {
        GenerationCounter(1)
    }

    /// Advances counter.
    pub fn bump(&mut self) {
        self.0 += 1;
    }

    /// Returns current generation.
    pub fn get(&self) -> Generation {
        debug_assert_ne!(
            self.0, 0,
            "Generation counter must start from literal `1` and only ever be incremented"
        );
        Generation(unsafe {
            // Can never be `0` thus safe.
            NonZeroU64::new_unchecked(self.0)
        })
    }
}
