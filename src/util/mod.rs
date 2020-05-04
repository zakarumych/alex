mod iterator;
mod sync;

use std::{
    convert::{TryFrom, TryInto as _},
    fmt::{self, Debug, Display},
};

pub use self::{iterator::*, sync::*};

lazy_static::lazy_static! {
    pub static ref CACHE_LINE_SIZE_HINT: usize = get_cache_line_size().unwrap_or(512);
}

fn get_cache_line_size() -> Option<usize> {
    use raw_cpuid::*;

    let cpuid = CpuId::new();
    let size = cpuid
        .get_cache_parameters()?
        .filter(|p| p.level() == 1 && p.cache_type() == CacheType::Data)
        .map(|p| p.coherency_line_size())
        .min();

    match size {
        Some(size) => Some(size),
        None => cpuid
            .get_cache_parameters()?
            .filter(|p| p.level() == 1 && p.cache_type() == CacheType::Unified)
            .map(|p| p.coherency_line_size())
            .min(),
    }
}

/// Value that fits both `u32` and `usize`
#[cfg(target_pointer_width = "16")]
#[repr(transparent)]
#[derive(Clone, Copy, Default)]
pub struct U32Size(u16);

/// Value that fits both `u32` and `usize`
#[cfg(not(target_pointer_width = "16"))]
#[repr(transparent)]
#[derive(Clone, Copy, Default)]
pub struct U32Size(u32);

impl TryFrom<u32> for U32Size {
    type Error = std::num::TryFromIntError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        #[cfg(target_pointer_width = "16")]
        {
            u16::try_from(value).map(U32Size)
        }
        #[cfg(not(target_pointer_width = "16"))]
        {
            Ok(U32Size(value))
        }
    }
}

impl TryFrom<usize> for U32Size {
    type Error = std::num::TryFromIntError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        #[cfg(target_pointer_width = "16")]
        {
            Ok(U32Size(value as u32))
        }
        #[cfg(target_pointer_width = "32")]
        {
            Ok(U32Size(value as u32))
        }
        #[cfg(not(any(target_pointer_width = "16", target_pointer_width = "32")))]
        {
            u32::try_from(value).map(U32Size)
        }
    }
}

impl TryFrom<u64> for U32Size {
    type Error = std::num::TryFromIntError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        value.try_into().map(U32Size)
    }
}

impl From<U32Size> for u32 {
    fn from(value: U32Size) -> u32 {
        value.0.into()
    }
}

impl From<U32Size> for usize {
    fn from(value: U32Size) -> usize {
        value.0 as usize
    }
}

impl From<u8> for U32Size {
    fn from(value: u8) -> Self {
        U32Size(value.into())
    }
}

impl From<u16> for U32Size {
    fn from(value: u16) -> Self {
        U32Size(value.into())
    }
}

impl U32Size {
    pub fn checked_inc(self) -> Option<Self> {
        self.as_usize().checked_add(1)?;
        self.as_u32().checked_add(1)?;
        Some(U32Size(self.0 + 1))
    }

    pub fn zero() -> Self {
        U32Size(0)
    }

    pub fn new(value: u16) -> Self {
        U32Size(value.into())
    }

    pub fn as_u32(self) -> u32 {
        self.into()
    }

    pub fn as_usize(self) -> usize {
        self.into()
    }
}

impl Debug for U32Size {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "{:?}", self.0)
    }
}

impl Display for U32Size {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "{}", self.0)
    }
}
