use {
    super::{
        access::ArchetypeRef,
        view::{ArchetypeRefs, ChunkRefs},
    },
    core::{cmp::min, marker::PhantomData, ptr::NonNull, slice},
};

pub struct ChunkEntityIter<T> {
    ptrs: T,
    len: usize,
}

impl<T> Iterator for ChunkEntityIter<T>
where
    T: ChunkRefs,
{
    type Item = T::Item;

    fn next(&mut self) -> Option<T::Item> {
        if self.len > 0 {
            let result = unsafe { self.ptrs.next() };
            self.len -= 1;
            Some(result)
        } else {
            None
        }
    }
}

pub struct ArchetypeEntityIter<'a, A> {
    pub(crate) raw_chunks: slice::Iter<'a, NonNull<u8>>,
    pub(crate) len: usize,
    pub(crate) chunk_capacity: usize,
    pub(crate) refs: A,
}

impl<'a, A> Iterator for ArchetypeEntityIter<'a, A>
where
    A: ArchetypeRefs,
{
    type Item = ChunkEntityIter<A::Item>;

    fn next(&mut self) -> Option<ChunkEntityIter<A::Item>> {
        let raw_chunk = *self.raw_chunks.next()?;

        let len = min(self.len, self.chunk_capacity);
        self.len -= len;

        let ptrs = unsafe { self.refs.get(raw_chunk) };
        Some(ChunkEntityIter { ptrs, len })
    }
}

macro_rules! impl_for_tuple {
    () => {
        impl ChunkRefs for () {
            type Item = ();
            unsafe fn next(&mut self) -> () {}
        }

        impl ArchetypeRefs for () {
            type Item = ();
            unsafe fn get(&self, _: NonNull<u8>) -> () {}
        }
    };

    ($($a:ident),+ $(,)?) => {
        impl<$($a),+> ChunkRefs for ($($a,)+)
        where
            $($a: ChunkRefs,)+
        {
            type Item = ($($a::Item,)+);
            unsafe fn next(&mut self) -> ($($a::Item,)+) {
                #![allow(non_snake_case)]
                let ($($a,)+) = self;
                ($($a.next(),)+)
            }
        }

        impl<$($a),+> ArchetypeRefs for ($($a,)+)
        where
            $($a: ArchetypeRefs,)+
        {
            type Item = ($($a::Item,)+);
            unsafe fn get(&self, base: NonNull<u8>) -> ($($a::Item,)+) {
                #![allow(non_snake_case)]
                let ($($a,)+) = self;
                ($($a.get(base),)+)
            }
        }
    };
}

impl_for_tuple!();
impl_for_tuple!(A);
impl_for_tuple!(A, B);
impl_for_tuple!(A, B, C);
impl_for_tuple!(A, B, C, D);
