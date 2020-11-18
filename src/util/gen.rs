use core::num::NonZeroU64;

/// Generation counter
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Generation {
    value: NonZeroU64,
}

impl Generation {
    pub const fn new() -> Self {
        Generation {
            value: unsafe { NonZeroU64::new_unchecked(1) },
        }
    }

    pub fn is_initial(&self) -> bool {
        self.value.get() == 1
    }

    pub fn inc(&mut self) {
        let value = self.value.get();
        let newvalue = NonZeroU64::new(value.wrapping_add(1))
            .expect("Overflow while incrementing 64bit. What year is it?");

        self.value = newvalue;
    }
}
