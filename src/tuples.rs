use {
    crate::{
        access::{Accessor, ArchetypeAccess, ComponentAccess},
        archetype::ArchetypeInfo,
        filter::Filter,
        view::View,
    },
    std::{cmp::Reverse, iter::IntoIterator, marker::PhantomData, ops::Add},
};

macro_rules! for_sequences {
    ($action:ident) => {
        for_sequences!([POP $action] [A, B, C, D]);
        // for_sequences!([POP $action] [A]);
    };

    ([POP $action:ident] []) => {
        for_sequences!([$action] []);
    };

    ([POP $action:ident] [$head:ident $(,$tail:ident)*]) => {
        for_sequences!([$action] [$head $(,$tail)*]);
        for_sequences!([POP $action] [$($tail),*]);
    };

    ([$action:ident] [$($a:ident),*]) => {
        $action!($($a),*);
    };
}

/// Iterator wrapper that merges tuple of iterators.
pub struct ChainIter<I, T> {
    iters: T,
    marker: PhantomData<fn() -> I>,
}

impl<I, T> ChainIter<I, T> {
    pub fn new(iters: T) -> Self {
        ChainIter {
            iters,
            marker: PhantomData,
        }
    }
}

macro_rules! chain_iter {
    () => {
        impl<I> Iterator for ChainIter<I, ()> {
            type Item = I;
            fn next(&mut self) -> Option<I> {
                None
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                (0, Some(0))
            }

            fn count(self) -> usize {
                0
            }

            fn last(self) -> Option<I> {
                None
            }

            fn nth(&mut self, _: usize) -> Option<I> {
                None
            }
        }
    };

    ($($a:ident),+) => {
        impl<I $(, $a)+> Iterator for ChainIter<I, ($($a,)+)>
        where
            $($a: Iterator<Item = I>,)+
        {
            type Item = I;

            fn next(&mut self) -> Option<I> {
                #![allow(non_snake_case)]

                let ($($a,)+) = &mut self.iters;
                $(
                    if let Some(next) = $a.next() {
                        return Some(next)
                    }
                )+
                None
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                #![allow(non_snake_case)]

                let mut lower = 0;
                let mut upper = Some(0);

                let ($($a,)+) = &self.iters;
                $(
                    let $a = $a.size_hint();
                    lower += $a.0;
                    upper = upper.and_then(|upper| $a.1.map(|u| upper + u));
                )+

                (lower, upper)
            }

            fn count(self) -> usize {
                #![allow(non_snake_case)]
                let ($($a,)+) = self.iters;
                0usize
                $(
                    .add($a.count())
                )+
            }

            fn last(self) -> Option<I> {
                #![allow(non_snake_case)]

                let mut last = None;
                let ($($a,)+) = self.iters;
                $(
                    if let Some(next) = $a.last() {
                        last = Some(next);
                    }
                )+
                last
            }
        }
    };
}

for_sequences!(chain_iter);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Zip<T>(pub T);

/// Iterator wrapper that merges tuple of iterators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ZipIter<T>(pub T);

macro_rules! zip_iter {
    () => {};

    ($($a:ident),+) => {
        impl<$($a),+> Zip<($($a,)+)> {
            pub fn new($($a: $a,)+) -> Self {
                #![allow(non_snake_case)]
                Zip(($($a,)+))
            }
        }

        impl<$($a),+> ZipIter<($($a,)+)> {
            pub fn new($($a: $a,)+) -> Self {
                #![allow(non_snake_case)]
                ZipIter(($($a,)+))
            }
        }

        impl<$($a),+> Iterator for ZipIter<($($a,)+)>
        where
            $($a: Iterator,)+
        {
            type Item = Zip<($($a::Item,)+)>;

            fn next(&mut self) -> Option<Zip<($($a::Item,)+)>> {
                #![allow(non_snake_case)]

                let ($($a,)+) = &mut self.0;
                Zip(($($a.next()?,)+)).into()
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                #![allow(non_snake_case)]

                let mut lower = usize::max_value();
                let mut upper = Reverse(None);

                let ($($a,)+) = &self.0;
                $(
                    let $a = $a.size_hint();
                    lower = lower.min($a.0);
                    upper = upper.min(Reverse($a.1.map(Reverse)));
                )+

                (lower, upper.0.map(|Reverse(upper)| upper))
            }

            fn count(self) -> usize {
                #![allow(non_snake_case)]
                let ($($a,)+) = self.0;
                usize::max_value()
                $(
                    .min($a.count())
                )+
            }

            fn last(self) -> Option<Zip<($($a::Item,)+)>> {
                #![allow(non_snake_case)]

                let ($($a,)+) = self.0;
                Zip(($($a.last()?,)+)).into()
            }

            fn nth(&mut self, n: usize) -> Option<Zip<($($a::Item,)+)>> {
                #![allow(non_snake_case)]

                let ($($a,)+) = &mut self.0;
                Zip(($($a.nth(n)?,)+)).into()
            }
        }

        impl<$($a),+> IntoIterator for Zip<($($a,)+)>
        where
            $($a: IntoIterator,)+
        {
            type Item = Zip<($($a::Item,)+)>;
            type IntoIter = ZipIter<($($a::IntoIter,)+)>;

            fn into_iter(self) -> ZipIter<($($a::IntoIter,)+)> {
                #![allow(non_snake_case)]
                let ($($a,)+) = self.0;
                ZipIter(($($a.into_iter(),)+))
            }
        }
    };
}

for_sequences!(zip_iter);

macro_rules! views {
    () => {};

    ($($a:ident),+) => {
        impl<'a $(, $a)+> Accessor<'a> for ($($a,)+)
        where
            $($a: Accessor<'a>,)+
        {
            type AccessTypes = ChainIter<ComponentAccess, ($($a::AccessTypes,)+)>;

            fn access_types(&'a self, archetype: &ArchetypeInfo) -> Self::AccessTypes {
                #![allow(non_snake_case)]

                let ($($a,)+) = self;
                ChainIter::new(($($a.access_types(archetype),)+))
            }
        }

        impl<$($a),+> Filter for ($($a,)+)
        where
            $($a: Filter,)+
        {
            fn filter_archetype(&self, archetype: &ArchetypeInfo) -> bool {
                #![allow(non_snake_case)]

                let ($($a,)+) = self;
                true $(&& $a.filter_archetype(archetype))+
            }
        }

        impl<'a $(, $a)+> View<'a> for ($($a,)+)
        where
            $($a: View<'a>,)+
        {
            type EntityView = Zip<($($a::EntityView,)+)>;
            type ChunkView = Zip<($($a::ChunkView,)+)>;
            type ArchetypeView = Zip<($($a::ArchetypeView,)+)>;

            fn view(&'a self, archetype: &mut ArchetypeAccess<'a>) -> Zip<($($a::ArchetypeView,)+)> {
                #![allow(non_snake_case)]
                #![allow(unused_assignments)]

                let ($($a,)+) = self;
                Zip(($($a.view(archetype),)+))
            }
        }
    };
}

for_sequences!(views);
