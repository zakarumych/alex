use {
    super::access::ArchetypeAccess,
    core::{marker::PhantomData, ptr::NonNull},
};

/// View components of entities in archetype.
pub trait View<'a> {
    /// View of one entity.
    type EntityView: 'a;

    /// View of one archetype.
    type AccessRefs: AccessRefs;

    /// Returns `ArchetypeView` for specified `ArchetypeAccess`.
    ///
    /// # Panics
    ///
    /// This function may panic if archetype does not match `View`'s requirements.
    fn acquire(&self, archetype: ArchetypeAccess<'a>) -> Self::AccessRefs;

    fn release(&self, refs: Self::AccessRefs);
}

struct ChunkEntityIter<P, R> {
    pointers: P,
    len: usize,
    marker: PhantomData<fn() -> R>,
}

impl<'a, T> Iterator for ChunkEntityIter<NonNull<u8>, &'a T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        if self.len > 0 {
            let result = unsafe { &*self.pointers };

            self.pointers = unsafe { self.pointers.offset(1) };
            self.len -= 1;

            Some(result)
        } else {
            None
        }
    }

    fn nth(&mut self, n: usize) -> Option<&'a T> {
        if self.len > n {
            let result = unsafe { &*self.pointers.add(n) };
            self.pointers = unsafe { self.pointers.offset(1) };
            self.len -= n + 1;

            Some(result)
        } else {
            None
        }
    }
}

impl<'a, T> Iterator for ChunkEntityIter<NonNull<u8>, &'a mut T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<&'a mut T> {
        if self.len > 0 {
            let result = unsafe { &mut *self.pointers };

            self.pointers = unsafe { self.pointers.offset(1) };
            self.len -= 1;

            Some(result)
        } else {
            None
        }
    }

    fn nth(&mut self, n: usize) -> Option<&'a mut T> {
        if self.len > n {
            let result = unsafe { &mut *self.pointers.add(n) };
            self.pointers = unsafe { self.pointers.offset(1) };
            self.len -= n + 1;

            Some(result)
        } else {
            None
        }
    }
}

pub struct ArchetypeEntityIter {}
