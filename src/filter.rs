use crate::{access::Accessor, archetype::ArchetypeInfo};

/// Simple filter for achetypes.
pub trait Filter {
    fn filter_archetype(&self, archetype: &ArchetypeInfo) -> bool;
}

pub struct FilteredAccessor<A, F> {
    filter: F,
    accessor: A,
}

impl<A, F> FilteredAccessor<A, F> {
    pub fn new(filter: F, accessor: A) -> Self {
        FilteredAccessor { filter, accessor }
    }
}

impl<'a, A, F> Accessor<'a> for FilteredAccessor<A, F>
where
    F: Filter,
    A: Accessor<'a>,
{
    type AccessTypes = MaybeIter<A::AccessTypes>;
    fn access_types(&'a self, archetype: &ArchetypeInfo) -> Self::AccessTypes {
        if self.filter.filter_archetype(archetype) {
            MaybeIter::Just(self.accessor.access_types(archetype))
        } else {
            MaybeIter::Nothing
        }
    }
}

pub enum MaybeIter<I> {
    Nothing,
    Just(I),
}

impl<I> Iterator for MaybeIter<I>
where
    I: Iterator,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        match self {
            MaybeIter::Nothing => None,
            MaybeIter::Just(iter) => iter.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            MaybeIter::Nothing => (0, Some(0)),
            MaybeIter::Just(iter) => iter.size_hint(),
        }
    }

    fn count(self) -> usize {
        match self {
            MaybeIter::Nothing => 0,
            MaybeIter::Just(iter) => iter.count(),
        }
    }

    fn last(self) -> Option<I::Item> {
        match self {
            MaybeIter::Nothing => None,
            MaybeIter::Just(iter) => iter.last(),
        }
    }

    fn nth(&mut self, n: usize) -> Option<I::Item> {
        match self {
            MaybeIter::Nothing => None,
            MaybeIter::Just(iter) => iter.nth(n),
        }
    }
}
