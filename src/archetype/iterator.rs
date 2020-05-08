use {
    crate::{
        access::Access,
        component::{Component, ComponentId},
        util::TryIter,
    },
    std::{marker::PhantomData, ptr::NonNull},
};

pub struct ChunkSizes {
    chunk: usize,
    len: usize,
}

impl ChunkSizes {
    pub fn new(chunk: usize, len: usize) -> Self {
        ChunkSizes { chunk, len }
    }
}

impl Iterator for ChunkSizes {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        if self.len == 0 {
            None
        } else {
            let next = std::cmp::min(self.len, self.chunk);
            self.len -= next;
            Some(next)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }

    fn count(self) -> usize {
        self.len()
    }

    fn last(self) -> Option<usize> {
        if self.len == 0 {
            None
        } else {
            Some((self.len - 1) % self.chunk + 1)
        }
    }

    fn nth(&mut self, n: usize) -> Option<usize> {
        let (offset, overflow) = n.overflowing_mul(self.chunk);
        if self.len <= offset || overflow {
            self.len = 0;
            None
        } else {
            self.len -= offset;
            let next = std::cmp::min(self.len, self.chunk);
            self.len -= next;
            Some(next)
        }
    }
}

impl DoubleEndedIterator for ChunkSizes {
    fn next_back(&mut self) -> Option<usize> {
        if self.len == 0 {
            None
        } else {
            let next = (self.len - 1) % self.chunk + 1;
            self.len -= next;
            Some(next)
        }
    }

    fn nth_back(&mut self, n: usize) -> Option<usize> {
        let (offset, overflow) = n.overflowing_mul(self.chunk);
        if self.len <= offset || overflow {
            self.len = 0;
            None
        } else {
            self.len -= offset;
            let next = (self.len - 1) % self.chunk + 1;
            self.len -= next;
            Some(next)
        }
    }
}

impl ExactSizeIterator for ChunkSizes {
    fn len(&self) -> usize {
        (self.len + self.chunk - 1) / self.chunk
    }
}

impl std::iter::FusedIterator for ChunkSizes {}

pub struct Chunks<'a> {
    ptrs: std::slice::Iter<'a, NonNull<u8>>,
    sizes: ChunkSizes,
}

impl<'a> Chunks<'a> {
    pub fn new(ptrs: &'a [NonNull<u8>], chunk: usize, len: usize) -> Self {
        let chunks_count = if len == 0 { 0 } else { (len - 1) / chunk + 1 };
        assert_eq!(ptrs.len(), chunks_count);
        Chunks {
            ptrs: ptrs.iter(),
            sizes: ChunkSizes { len, chunk },
        }
    }
}

impl Iterator for Chunks<'_> {
    type Item = (NonNull<u8>, usize);

    fn next(&mut self) -> Option<(NonNull<u8>, usize)> {
        Some((*self.ptrs.next()?, self.sizes.next()?))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }

    fn count(self) -> usize {
        self.len()
    }

    fn last(self) -> Option<(NonNull<u8>, usize)> {
        Some((*self.ptrs.last()?, self.sizes.last()?))
    }

    fn nth(&mut self, n: usize) -> Option<(NonNull<u8>, usize)> {
        Some((*self.ptrs.nth(n)?, self.sizes.nth(n)?))
    }
}

impl DoubleEndedIterator for Chunks<'_> {
    fn next_back(&mut self) -> Option<(NonNull<u8>, usize)> {
        Some((*self.ptrs.next_back()?, self.sizes.next_back()?))
    }

    fn nth_back(&mut self, n: usize) -> Option<(NonNull<u8>, usize)> {
        Some((*self.ptrs.nth_back(n)?, self.sizes.nth_back(n)?))
    }
}

impl ExactSizeIterator for Chunks<'_> {
    fn len(&self) -> usize {
        self.ptrs.len()
    }
}

impl std::iter::FusedIterator for Chunks<'_> {}

/// Slice of components.
pub struct ComponentSlice<'a> {
    component: ComponentId,
    access: Access,
    ptr: NonNull<u8>,
    len: usize,
    marker: PhantomData<&'a mut u8>,
}

impl<'a> ComponentSlice<'a> {
    /// Returns slice to component instances.
    pub fn read<T: Component>(self) -> &'a [T] {
        assert_eq!(T::component_id(), self.component);
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr() as *const T, self.len) }
    }

    /// Returns slice to component instances.
    pub fn write<T: Component>(self) -> std::slice::IterMut<'a, T> {
        assert_eq!(T::component_id(), self.component);
        assert_eq!(self.access, Access::Write);
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr() as *mut T, self.len) }.iter_mut()
    }

    pub fn raw(&self) -> NonNull<u8> {
        self.ptr
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

/// Iterator for component slices.
pub struct RawArchetypeComponentIter<'a> {
    component: ComponentId,
    access: Access,
    offset: usize,
    chunks: Chunks<'a>,
    marker: PhantomData<&'a mut u8>,
}

impl<'a> RawArchetypeComponentIter<'a> {
    /// Create new raw component data iterator.
    ///
    /// # Safety
    ///
    /// Iterator yields possibly mutable references to component bytes at specified offset.
    /// Access must be externally synchronized.
    /// `offset` must be the offset in chunks where specified components are stored.
    pub(super) unsafe fn new(
        component: ComponentId,
        access: Access,
        offset: usize,
        chunks: Chunks<'a>,
    ) -> Self {
        RawArchetypeComponentIter {
            component,
            access,
            offset,
            chunks,
            marker: PhantomData,
        }
    }

    fn item_from_chunk(&self, ptr: NonNull<u8>, len: usize) -> ComponentSlice<'a> {
        let ptr = unsafe {
            // Cannot overflow because offset is smaller then allocation that starts with `ptr`.
            NonNull::new_unchecked(ptr.as_ptr().add(self.offset))
        };

        ComponentSlice {
            component: self.component,
            access: self.access,
            ptr,
            len,
            marker: PhantomData,
        }
    }
}

impl<'a> Iterator for RawArchetypeComponentIter<'a> {
    type Item = ComponentSlice<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let (ptr, len) = self.chunks.next()?;
        self.item_from_chunk(ptr, len).into()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chunks.size_hint()
    }

    fn count(self) -> usize {
        self.chunks.count()
    }

    fn last(self) -> Option<Self::Item> {
        let (ptr, len) = self.chunks.last()?;

        let ptr = unsafe {
            // Cannot overflow because offset is smaller then allocation that starts with `ptr`.
            NonNull::new_unchecked(ptr.as_ptr().add(self.offset))
        };

        ComponentSlice {
            component: self.component,
            access: self.access,
            ptr,
            len,
            marker: PhantomData,
        }
        .into()
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let (ptr, len) = self.chunks.nth(n)?;
        self.item_from_chunk(ptr, len).into()
    }
}

impl<'a> DoubleEndedIterator for RawArchetypeComponentIter<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let (ptr, len) = self.chunks.next_back()?;
        self.item_from_chunk(ptr, len).into()
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        let (ptr, len) = self.chunks.nth_back(n)?;
        self.item_from_chunk(ptr, len).into()
    }
}

impl<'a> ExactSizeIterator for RawArchetypeComponentIter<'a> {
    fn len(&self) -> usize {
        self.chunks.len()
    }
}

impl<'a> std::iter::FusedIterator for RawArchetypeComponentIter<'a> {}

/// Iterator for component slices.
pub struct ArchetypeComponentIter<'a, T> {
    offset: usize,
    chunks: Chunks<'a>,
    marker: PhantomData<&'a T>,
}

impl<'a, T: 'a> ArchetypeComponentIter<'a, T> {
    /// Create new immutable archetype component iterator.
    ///
    /// # Safety
    ///
    /// Iterator yields immutable references to components stored in chunks.
    /// Immutable access must be externally synchronized.
    pub(super) unsafe fn new(offset: usize, chunks: Chunks<'a>) -> Self {
        ArchetypeComponentIter {
            offset,
            chunks,
            marker: PhantomData,
        }
    }
}

impl<'a, T> ArchetypeComponentIter<'a, T> {
    fn item_from_chunk(&mut self, ptr: NonNull<u8>, len: usize) -> std::slice::Iter<'a, T> {
        unsafe {
            // Cannot overflow because offset is smaller then allocation that starts with `ptr`.
            std::slice::from_raw_parts(ptr.as_ptr().add(self.offset) as *const T, len)
        }
        .iter()
    }
}

impl<'a, T: 'a> Iterator for ArchetypeComponentIter<'a, T> {
    type Item = std::slice::Iter<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        let (ptr, len) = self.chunks.next()?;
        self.item_from_chunk(ptr, len).into()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chunks.size_hint()
    }

    fn count(self) -> usize {
        self.chunks.count()
    }

    fn last(self) -> Option<Self::Item> {
        let (ptr, len) = self.chunks.last()?;
        unsafe {
            // Cannot overflow because offset is smaller then allocation that starts with `ptr`.
            std::slice::from_raw_parts(ptr.as_ptr().add(self.offset) as *const T, len)
        }
        .iter()
        .into()
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let (ptr, len) = self.chunks.nth(n)?;
        self.item_from_chunk(ptr, len).into()
    }
}

impl<'a, T: 'a> DoubleEndedIterator for ArchetypeComponentIter<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let (ptr, len) = self.chunks.next_back()?;
        self.item_from_chunk(ptr, len).into()
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        let (ptr, len) = self.chunks.nth_back(n)?;
        self.item_from_chunk(ptr, len).into()
    }
}

impl<'a, T> ExactSizeIterator for ArchetypeComponentIter<'a, T> {
    fn len(&self) -> usize {
        self.chunks.len()
    }
}

impl<'a, T: 'a> std::iter::FusedIterator for ArchetypeComponentIter<'a, T> {}

/// Iterator for component slices.
pub struct ArchetypeComponentIterMut<'a, T> {
    offset: usize,
    chunks: Chunks<'a>,
    marker: PhantomData<&'a mut T>,
}

impl<'a, T: 'a> ArchetypeComponentIterMut<'a, T> {
    /// Create new mutable archetype component iterator.
    ///
    /// # Safety
    ///
    /// Iterator yields mutable references to components stored in chunks.
    /// Mutable access must be externally synchronized.
    pub(super) unsafe fn new(offset: usize, chunks: Chunks<'a>) -> Self {
        ArchetypeComponentIterMut {
            offset,
            chunks,
            marker: PhantomData,
        }
    }
}

impl<'a, T> ArchetypeComponentIterMut<'a, T> {
    fn item_from_chunk(&mut self, ptr: NonNull<u8>, len: usize) -> std::slice::IterMut<'a, T> {
        unsafe {
            // Cannot overflow because offset is smaller then allocation that starts with `chunk.ptr`.
            std::slice::from_raw_parts_mut(ptr.as_ptr().add(self.offset) as *mut T, len)
        }
        .iter_mut()
    }
}

impl<'a, T: 'a> Iterator for ArchetypeComponentIterMut<'a, T> {
    type Item = std::slice::IterMut<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        let (ptr, len) = self.chunks.next()?;
        self.item_from_chunk(ptr, len).into()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chunks.size_hint()
    }

    fn count(self) -> usize {
        self.chunks.count()
    }

    fn last(self) -> Option<Self::Item> {
        let (ptr, len) = self.chunks.last()?;
        unsafe {
            // Cannot overflow because offset is smaller then allocation that starts with `chunk.ptr`.
            std::slice::from_raw_parts_mut(ptr.as_ptr().add(self.offset) as *mut T, len)
        }
        .iter_mut()
        .into()
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let (ptr, len) = self.chunks.nth(n)?;
        self.item_from_chunk(ptr, len).into()
    }
}

impl<'a, T: 'a> DoubleEndedIterator for ArchetypeComponentIterMut<'a, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let (ptr, len) = self.chunks.next_back()?;
        self.item_from_chunk(ptr, len).into()
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        let (ptr, len) = self.chunks.nth_back(n)?;
        self.item_from_chunk(ptr, len).into()
    }
}

impl<'a, T> ExactSizeIterator for ArchetypeComponentIterMut<'a, T> {
    fn len(&self) -> usize {
        self.chunks.len()
    }
}

impl<'a, T: 'a> std::iter::FusedIterator for ArchetypeComponentIterMut<'a, T> {}

/// Iterator for component slices.
pub enum TryArchetypeIter<I> {
    Just(I),
    Nothing(ChunkSizes),
    Repeat,
}

impl<'a, T: 'a> From<ArchetypeComponentIter<'a, T>>
    for TryArchetypeIter<ArchetypeComponentIter<'a, T>>
{
    fn from(iter: ArchetypeComponentIter<'a, T>) -> Self {
        Self::Just(iter)
    }
}

impl<'a, T: 'a> From<ArchetypeComponentIterMut<'a, T>>
    for TryArchetypeIter<ArchetypeComponentIterMut<'a, T>>
{
    fn from(iter: ArchetypeComponentIterMut<'a, T>) -> Self {
        Self::Just(iter)
    }
}

impl<I> From<ChunkSizes> for TryArchetypeIter<I> {
    fn from(iter: ChunkSizes) -> Self {
        Self::Nothing(iter)
    }
}

impl<I> Iterator for TryArchetypeIter<I>
where
    I: Iterator,
    I::Item: Iterator,
{
    type Item = TryIter<I::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Just(iter) => iter.next().map(TryIter::Just),
            Self::Nothing(iter) => iter.next().map(TryIter::Nothing),
            Self::Repeat => Some(TryIter::Repeat),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Just(iter) => iter.size_hint(),
            Self::Nothing(iter) => iter.size_hint(),
            Self::Repeat => (usize::max_value(), None),
        }
    }

    fn count(self) -> usize {
        match self {
            Self::Just(iter) => iter.count(),
            Self::Nothing(iter) => iter.count(),
            Self::Repeat => panic!("`count()` called for infinite operator"),
        }
    }

    fn last(self) -> Option<Self::Item> {
        match self {
            Self::Just(iter) => iter.last().map(TryIter::Just),
            Self::Nothing(iter) => iter.last().map(TryIter::Nothing),
            Self::Repeat => panic!("`last()` called for infinite operator"),
        }
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        match self {
            Self::Just(iter) => iter.nth(n).map(TryIter::Just),
            Self::Nothing(iter) => iter.nth(n).map(TryIter::Nothing),
            Self::Repeat => Some(TryIter::Repeat),
        }
    }
}

impl<I> DoubleEndedIterator for TryArchetypeIter<I>
where
    I: DoubleEndedIterator,
    I::Item: Iterator,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            Self::Just(iter) => iter.next_back().map(TryIter::Just),
            Self::Nothing(iter) => iter.next_back().map(TryIter::Nothing),
            Self::Repeat => Some(TryIter::Repeat),
        }
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        match self {
            Self::Just(iter) => iter.nth_back(n).map(TryIter::Just),
            Self::Nothing(iter) => iter.nth_back(n).map(TryIter::Nothing),
            Self::Repeat => Some(TryIter::Repeat),
        }
    }
}

impl<I> std::iter::FusedIterator for TryArchetypeIter<I>
where
    I: Iterator,
    I::Item: Iterator,
{
}

pub type TryArchetypeComponentIter<'a, T> = TryArchetypeIter<ArchetypeComponentIter<'a, T>>;
pub type TryArchetypeComponentIterMut<'a, T> = TryArchetypeIter<ArchetypeComponentIterMut<'a, T>>;
