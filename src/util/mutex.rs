#[cfg(not(feature = "std"))]
pub use spin::Mutex;

#[cfg(feature = "std")]
pub use parking_lot::Mutex;
