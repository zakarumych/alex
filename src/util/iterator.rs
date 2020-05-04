use std::{cmp::Reverse, marker::PhantomData, ops::Add};

/// An actual iterator or empty.
pub enum MaybeIter<I> {
    /// Empty iterator variant.
    Nothing,

    /// Inner iterator.
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
            MaybeIter::Just(iter) => match iter.next() {
                Some(next) => Some(next),
                None => {
                    *self = MaybeIter::Nothing;
                    None
                }
            },
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
            MaybeIter::Just(iter) => match iter.nth(n) {
                Some(next) => Some(next),
                None => {
                    *self = MaybeIter::Nothing;
                    None
                }
            },
        }
    }
}

impl<I> DoubleEndedIterator for MaybeIter<I>
where
    I: DoubleEndedIterator,
{
    fn next_back(&mut self) -> Option<I::Item> {
        match self {
            MaybeIter::Nothing => None,
            MaybeIter::Just(iter) => match iter.next_back() {
                Some(next) => Some(next),
                None => {
                    *self = MaybeIter::Nothing;
                    None
                }
            },
        }
    }

    fn nth_back(&mut self, n: usize) -> Option<I::Item> {
        match self {
            MaybeIter::Nothing => None,
            MaybeIter::Just(iter) => match iter.nth_back(n) {
                Some(next) => Some(next),
                None => {
                    *self = MaybeIter::Nothing;
                    None
                }
            },
        }
    }
}

impl<I> ExactSizeIterator for MaybeIter<I>
where
    I: ExactSizeIterator,
{
    fn len(&self) -> usize {
        match self {
            MaybeIter::Nothing => 0,
            MaybeIter::Just(iter) => iter.len(),
        }
    }
}

impl<I> std::iter::FusedIterator for MaybeIter<I> where I: Iterator {}

/// Iterator over multiple `None`s or `Some`s
pub enum TryIter<I> {
    Just(I),
    Nothing(usize),
    Repeat,
}

impl<I> Iterator for TryIter<I>
where
    I: Iterator,
{
    type Item = Option<I::Item>;

    fn next(&mut self) -> Option<Option<I::Item>> {
        match self {
            TryIter::Just(iter) => match iter.next() {
                Some(next) => Some(Some(next)),
                None => {
                    *self = TryIter::Nothing(0);
                    None
                }
            },
            TryIter::Nothing(0) => None,
            TryIter::Nothing(count) => {
                *count -= 1;
                Some(None)
            }
            TryIter::Repeat => Some(None),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            TryIter::Just(iter) => iter.size_hint(),
            TryIter::Nothing(count) => (*count, Some(*count)),
            TryIter::Repeat => (usize::max_value(), None),
        }
    }

    fn count(self) -> usize {
        match self {
            TryIter::Nothing(count) => count,
            TryIter::Just(iter) => iter.count(),
            TryIter::Repeat => panic!("`count()` called for infinite operator"),
        }
    }

    fn last(self) -> Option<Option<I::Item>> {
        match self {
            TryIter::Nothing(0) => None,
            TryIter::Nothing(_) => Some(None),
            TryIter::Just(iter) => iter.last().map(Some),
            TryIter::Repeat => panic!("`last()` called for infinite operator"),
        }
    }

    fn nth(&mut self, n: usize) -> Option<Option<I::Item>> {
        match self {
            TryIter::Just(iter) => match iter.nth(n) {
                Some(next) => Some(Some(next)),
                None => {
                    *self = TryIter::Nothing(0);
                    None
                }
            },
            TryIter::Nothing(count) if *count <= n => {
                *count = 0;
                None
            }
            TryIter::Nothing(count) => {
                *count -= n + 1;
                Some(None)
            }
            TryIter::Repeat => Some(None),
        }
    }
}

impl<I> DoubleEndedIterator for TryIter<I>
where
    I: DoubleEndedIterator,
{
    fn next_back(&mut self) -> Option<Option<I::Item>> {
        match self {
            TryIter::Just(iter) => match iter.next_back() {
                Some(next) => Some(Some(next)),
                None => {
                    *self = TryIter::Nothing(0);
                    None
                }
            },
            TryIter::Nothing(0) => None,
            TryIter::Nothing(count) => {
                *count -= 1;
                Some(None)
            }
            TryIter::Repeat => Some(None),
        }
    }

    fn nth_back(&mut self, n: usize) -> Option<Option<I::Item>> {
        match self {
            TryIter::Just(iter) => match iter.nth_back(n) {
                Some(next) => Some(Some(next)),
                None => {
                    *self = TryIter::Nothing(0);
                    None
                }
            },
            TryIter::Nothing(count) if *count <= n => {
                *count = 0;
                None
            }
            TryIter::Nothing(count) => {
                *count -= n + 1;
                Some(None)
            }
            TryIter::Repeat => Some(None),
        }
    }
}

impl<I> std::iter::FusedIterator for TryIter<I> where I: Iterator {}

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
        impl<Item> Iterator for ChainIter<Item, ()> {
            type Item = Item;
            fn next(&mut self) -> Option<Item> {
                None
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                (0, Some(0))
            }

            fn count(self) -> usize {
                0
            }

            fn last(self) -> Option<Item> {
                None
            }

            fn nth(&mut self, _: usize) -> Option<Item> {
                None
            }
        }
    };

    ($($a:ident),+) => {
        impl<Item $(, $a)+> Iterator for ChainIter<Item, ($($a,)+)>
        where
            $($a: Iterator<Item = Item>,)+
        {
            type Item = Item;

            fn next(&mut self) -> Option<Item> {
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

            fn last(self) -> Option<Item> {
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

/// Tuple wrapper that may implement `Iterator` over zipped items if all elements of the tuple are iterators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Zip<T>(pub T);

macro_rules! zip_iter {
    () => {};

    ($($a:ident),+) => {
        impl<$($a),+> Zip<($($a,)+)> {
            pub fn new($($a: $a,)+) -> Self {
                #![allow(non_snake_case)]
                Zip(($($a,)+))
            }
        }

        impl<$($a),+> Iterator for Zip<($($a,)+)>
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
    };
}

for_sequences!(zip_iter);
