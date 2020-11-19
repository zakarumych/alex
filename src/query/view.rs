use {
    super::access::{ArchetypeAccess, ArchetypeRef, ArchetypeRefMut},
    core::{marker::PhantomData, ptr::NonNull},
};

pub trait ChunkRefs {
    type Item;
    unsafe fn next(&mut self) -> Self::Item;
}

#[repr(transparent)]
pub struct ChunkRef<'a, T> {
    ptr: NonNull<T>,
    marker: PhantomData<&'a [T]>,
}

impl<'a, T> ChunkRefs for ChunkRef<'a, T> {
    type Item = &'a T;
    unsafe fn next(&mut self) -> &'a T {
        let result = &*self.ptr.as_ptr();
        self.ptr = NonNull::new_unchecked(self.ptr.as_ptr().add(1));
        result
    }
}

#[repr(transparent)]
pub struct ChunkRefMut<'a, T> {
    ptr: NonNull<T>,
    marker: PhantomData<&'a mut [T]>,
}

impl<'a, T> ChunkRefs for ChunkRefMut<'a, T> {
    type Item = &'a mut T;
    unsafe fn next(&mut self) -> &'a mut T {
        let result = &mut *self.ptr.as_ptr();
        self.ptr = NonNull::new_unchecked(self.ptr.as_ptr().add(1));
        result
    }
}

pub trait ArchetypeRefs {
    type Item: ChunkRefs;
    unsafe fn get(&self, base: NonNull<u8>) -> Self::Item;
}

impl<'a, T> ArchetypeRefs for ArchetypeRef<'a, T> {
    type Item = ChunkRef<'a, T>;
    unsafe fn get(&self, base: NonNull<u8>) -> ChunkRef<'a, T> {
        ChunkRef {
            ptr: self.get(base),
            marker: PhantomData,
        }
    }
}

impl<'a, T> ArchetypeRefs for ArchetypeRefMut<'a, T> {
    type Item = ChunkRefMut<'a, T>;
    unsafe fn get(&self, base: NonNull<u8>) -> ChunkRefMut<'a, T> {
        ChunkRefMut {
            ptr: self.get(base),
            marker: PhantomData,
        }
    }
}

/// View components of entities in archetype.
pub trait View<'a> {
    /// View of one entity.
    type EntityView: 'a;

    type ChunkRefs: ChunkRefs<Item = Self::EntityView>;

    /// View of one archetype.
    type ArchetypeRefs: ArchetypeRefs<Item = Self::ChunkRefs>;

    /// Returns `ArchetypeRefs` for specified `ArchetypeAccess`.
    ///
    /// # Panics
    ///
    /// This function may panic if archetype does not match `View`'s requirements.
    fn acquire(&self, archetype: ArchetypeAccess<'a>) -> Self::ArchetypeRefs;
}
