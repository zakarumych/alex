use crate::{access::ArchetypeAccess, archetype::ChunkSizes, filter::Filter, util::Zip};

/// Trait to create views into chunks and entities.
pub trait View<'a>: Filter {
    /// Entity for the view.
    /// Typically this type is a set of references of viewed components for particular entity.
    /// Instances of this type are produced by iterating over `ChunkView`.
    type EntityView: 'a;

    /// Chunk for the view.
    /// Typically this type is a set of slices of view components for whole chunk.
    type ChunkView: Iterator<Item = Self::EntityView> + 'a;

    /// Chunk for the view.
    /// Typically this type is a set of slices of view components for whole chunk.
    type ArchetypeView: Iterator<Item = Self::ChunkView> + 'a;

    /// Returns `ArchetypeView` for specified `ArchetypeAccess`.
    ///
    /// # Panics
    ///
    /// This function is expected to panic if `ArchetypeAccess` doesn't allow access to components this `View` expects.
    fn view(&self, archetype: &mut ArchetypeAccess<'a>) -> Self::ArchetypeView;
}

impl<'a, V> View<'a> for &'_ V
where
    V: View<'a>,
{
    type EntityView = V::EntityView;
    type ChunkView = V::ChunkView;
    type ArchetypeView = V::ArchetypeView;

    fn view(&self, archetype: &mut ArchetypeAccess<'a>) -> Self::ArchetypeView {
        V::view(*self, archetype)
    }
}

impl<'a, V> View<'a> for &'_ mut V
where
    V: View<'a>,
{
    type EntityView = V::EntityView;
    type ChunkView = V::ChunkView;
    type ArchetypeView = V::ArchetypeView;

    fn view(&self, archetype: &mut ArchetypeAccess<'a>) -> Self::ArchetypeView {
        V::view(*self, archetype)
    }
}

/// `ArchetypeView` alias for `View`
pub type ArchetypeView<'a, V> = <V as View<'a>>::ArchetypeView;

/// `ChunkView` alias for `View`
pub type ChunkView<'a, V> = <V as View<'a>>::ChunkView;

/// `EntityView` alias for `View`
pub type EntityView<'a, V> = <V as View<'a>>::EntityView;

pub type EmptyChunkView = std::iter::Take<std::iter::Repeat<()>>;

pub struct EmptyArchetypeView {
    sizes: ChunkSizes,
}

impl EmptyArchetypeView {
    pub fn from_access(archetype: &ArchetypeAccess<'_>) -> Self {
        EmptyArchetypeView {
            sizes: archetype.chunk_sizes(),
        }
    }
}

impl Iterator for EmptyArchetypeView {
    type Item = EmptyChunkView;

    fn next(&mut self) -> Option<EmptyChunkView> {
        let size = self.sizes.next()?;
        std::iter::repeat(()).take(size).into()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.sizes.size_hint()
    }

    fn count(self) -> usize {
        self.sizes.count()
    }

    fn last(self) -> Option<EmptyChunkView> {
        let size = self.sizes.last()?;
        std::iter::repeat(()).take(size).into()
    }

    fn nth(&mut self, n: usize) -> Option<EmptyChunkView> {
        let size = self.sizes.nth(n)?;
        std::iter::repeat(()).take(size).into()
    }
}

impl DoubleEndedIterator for EmptyArchetypeView {
    fn next_back(&mut self) -> Option<EmptyChunkView> {
        let size = self.sizes.next_back()?;
        std::iter::repeat(()).take(size).into()
    }

    fn nth_back(&mut self, n: usize) -> Option<EmptyChunkView> {
        let size = self.sizes.nth_back(n)?;
        std::iter::repeat(()).take(size).into()
    }
}

impl ExactSizeIterator for EmptyArchetypeView {
    fn len(&self) -> usize {
        self.sizes.len()
    }
}

impl std::iter::FusedIterator for EmptyArchetypeView {}

macro_rules! tuple_views {
    () => {
        impl<'a> View<'a> for () {
            type EntityView = ();
            type ChunkView = EmptyChunkView;
            type ArchetypeView = EmptyArchetypeView;
            fn view(&self, archetype: &mut ArchetypeAccess<'a>) -> Self::ArchetypeView {
                EmptyArchetypeView::from_access(archetype)
            }
        }
    };

    ($($a:ident),+) => {
        impl<'a $(, $a)+> View<'a> for ($($a,)+)
        where
            $($a: View<'a>,)+
        {
            type EntityView = Zip<($($a::EntityView,)+)>;
            type ChunkView = Zip<($($a::ChunkView,)+)>;
            type ArchetypeView = Zip<($($a::ArchetypeView,)+)>;

            fn view(&self, archetype: &mut ArchetypeAccess<'a>) -> Zip<($($a::ArchetypeView,)+)> {
                #![allow(non_snake_case)]

                let ($($a,)+) = self;
                Zip(($($a.view(archetype),)+))
            }
        }
    };
}

for_sequences!(tuple_views);
