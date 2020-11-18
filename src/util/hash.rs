use {
    ahash::RandomState,
    core::hash::{BuildHasher as _, Hasher},
};

#[derive(Default)]
pub struct NoOpHasher {
    hash: u64,
}

impl Hasher for NoOpHasher {
    fn finish(&self) -> u64 {
        self.hash
    }

    #[cfg(target_pointer_width = "64")]
    fn write_usize(&mut self, i: usize) {
        self.hash = i as u64;
    }

    fn write_u128(&mut self, i: u128) {
        self.hash = i as u64;
    }

    fn write_u64(&mut self, i: u64) {
        self.hash = i;
    }

    fn write(&mut self, bytes: &[u8]) {
        match *bytes {
            [a, b, c, d, e, f, g, h, ..] => {
                self.hash = u64::from_ne_bytes([a, b, c, d, e, f, g, h]);
            }
            _ => {
                let mut hasher = RandomState::new().build_hasher();
                hasher.write(bytes);
                self.hash = hasher.finish();
            }
        }
    }
}

#[derive(Default)]
pub struct XorHasher {
    hash: u64,
}

impl Hasher for XorHasher {
    fn finish(&self) -> u64 {
        self.hash
    }

    #[cfg(target_pointer_width = "64")]
    fn write_usize(&mut self, i: usize) {
        self.hash ^= i as u64;
    }

    fn write_u128(&mut self, i: u128) {
        self.hash ^= i as u64;
    }

    fn write_u64(&mut self, i: u64) {
        self.hash ^= i;
    }

    fn write(&mut self, bytes: &[u8]) {
        match *bytes {
            [a, b, c, d, e, f, g, h, ..] => {
                self.hash ^= u64::from_ne_bytes([a, b, c, d, e, f, g, h]);
            }
            _ => {
                let mut hasher = RandomState::new().build_hasher();
                hasher.write(bytes);
                self.hash ^= hasher.finish();
            }
        }
    }
}
