use crate::{access::ArchetypeAccess, filter::Filter};

/// Trait to create views into chunks and entities.
pub trait View<'a>: Filter {
    /// Entity for the view.
    /// Typically this type is a set of references of viewed components for particular entity.
    /// Instances of this type are produced by iterating over `ChunkView`.
    type EntityView: 'a;

    /// Chunk for the view.
    /// Typically this type is a set of slices of view components for whole chunk.
    type ChunkView: IntoIterator<Item = Self::EntityView> + 'a;

    /// Chunk for the view.
    /// Typically this type is a set of slices of view components for whole chunk.
    type ArchetypeView: IntoIterator<Item = Self::ChunkView> + 'a;

    /// Returns `ArchetypeView` for specified `ArchetypeAccess`.
    /// `offset` is number of access types declared by earlier sub-views in super-view.
    /// If this is root view then `offset` should be `0`.
    ///
    /// # Panics
    ///
    /// This function is expected to panic if `ArchetypeAccess` doesn't allow access to components this `View` expects.
    fn view(&'a self, archetype: &mut ArchetypeAccess<'a>) -> Self::ArchetypeView;
}

/// `ArchetypeView` alias for `View`
pub type ArchetypeView<'a, V> = <V as View<'a>>::ArchetypeView;

/// `ChunkView` alias for `View`
pub type ChunkView<'a, V> = <V as View<'a>>::ChunkView;

/// `EntityView` alias for `View`
pub type EntityView<'a, V> = <V as View<'a>>::EntityView;

/// `ArchetypeView` alias for `View`
pub type ArchetypeViewIter<'a, V> = <<V as View<'a>>::ArchetypeView as IntoIterator>::IntoIter;

/// `ChunkView` alias for `View`
pub type ChunkViewIter<'a, V> = <<V as View<'a>>::ChunkView as IntoIterator>::IntoIter;

/// `EntityView` alias for `View`
pub type EntityViewIter<'a, V> = <<V as View<'a>>::EntityView as IntoIterator>::IntoIter;

// impl<'a> WorldAccess<'a> {
//     /// Iterate over chunks using the query.
//     ///
//     /// # Panics
//     ///
//     /// Returned iterator will panic if query would attempt to perform access outside of this `WorldAccess` restriction.
//     pub fn iter_chunks<'b, V: View<'b>>(&'b mut self, view: &'b V) -> WorldAccessChunkIter<'b, V> {
//         WorldAccessChunkIter {
//             archetypes: self.archetypes.reborrow(),
//             view,
//             current: None,
//         }
//     }

//     /// Iterate over entities using the query.
//     ///
//     /// # Panics
//     ///
//     /// Returned iterator will panic if query would attemtp to perform access outside of this `WorldAccess` restriction.
//     pub fn iter_entities<'b, V: View<'b>>(
//         &'b mut self,
//         view: &'b V,
//     ) -> WorldAccessEntityIter<'b, V> {
//         WorldAccessEntityIter {
//             archetypes: self.archetypes.reborrow(),
//             view,
//             current_chunk: None,
//             current_archetype: None,
//         }
//     }
// }

pub use self::impls::*;

mod impls {
    use {
        crate::{
            access::{Accessor, ArchetypeAccess, ComponentAccess},
            archetype::{ArchetypeComponentIter, ArchetypeComponentIterMut, ArchetypeInfo},
            component::Component,
            filter::Filter,
            view::View,
        },
        std::{
            fmt::{self, Debug},
            marker::PhantomData,
        },
    };

    pub struct Read<T>(PhantomData<fn() -> T>);

    impl<T> Clone for Read<T> {
        fn clone(&self) -> Self {
            *self
        }
    }

    impl<T> Copy for Read<T> {}

    impl<T> Debug for Read<T>
    where
        T: Component,
    {
        fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(fmt, "Read<{}>", T::type_name())
        }
    }

    impl<T> Default for Read<T> {
        fn default() -> Self {
            Read(PhantomData)
        }
    }

    impl<T> Read<T> {
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl<'a, T> Accessor<'a> for Read<T>
    where
        T: Component,
    {
        type AccessTypes = std::option::IntoIter<ComponentAccess>;

        fn access_types(
            &self,
            archetype: &ArchetypeInfo,
        ) -> std::option::IntoIter<ComponentAccess> {
            ComponentAccess::read::<T>(archetype).into_iter()
        }
    }

    impl<T> Filter for Read<T>
    where
        T: Component,
    {
        fn filter_archetype(&self, archetype: &ArchetypeInfo) -> bool {
            archetype.has(T::component_id())
        }
    }

    impl<'a, T> View<'a> for Read<T>
    where
        T: Component,
    {
        type EntityView = &'a T;
        type ChunkView = &'a [T];
        type ArchetypeView = ArchetypeComponentIter<'a, T>;

        fn view(&'a self, archetype: &mut ArchetypeAccess<'a>) -> ArchetypeComponentIter<'a, T> {
            match archetype.read_component::<T>() {
                Some(iter) => iter,
                None => panic!(
                    "Failed to fetch read access to component {}",
                    T::type_name()
                ),
            }
        }
    }

    pub fn read<T: Component>() -> Read<T> {
        Read::default()
    }

    pub struct Write<T>(PhantomData<fn() -> T>);

    impl<T> Clone for Write<T> {
        fn clone(&self) -> Self {
            *self
        }
    }

    impl<T> Copy for Write<T> {}

    impl<T> Debug for Write<T>
    where
        T: Component,
    {
        fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(fmt, "Write<{}>", T::type_name())
        }
    }

    impl<T> Default for Write<T> {
        fn default() -> Self {
            Write(PhantomData)
        }
    }

    impl<T> Write<T> {
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl<'a, T> Accessor<'a> for Write<T>
    where
        T: Component,
    {
        type AccessTypes = std::option::IntoIter<ComponentAccess>;

        fn access_types(
            &self,
            archetype: &ArchetypeInfo,
        ) -> std::option::IntoIter<ComponentAccess> {
            ComponentAccess::write::<T>(archetype).into_iter()
        }
    }

    impl<T> Filter for Write<T>
    where
        T: Component,
    {
        fn filter_archetype(&self, archetype: &ArchetypeInfo) -> bool {
            archetype.has(T::component_id())
        }
    }

    impl<'a, T> View<'a> for Write<T>
    where
        T: Component,
    {
        type EntityView = &'a mut T;
        type ChunkView = &'a mut [T];
        type ArchetypeView = ArchetypeComponentIterMut<'a, T>;

        fn view(&'a self, archetype: &mut ArchetypeAccess<'a>) -> ArchetypeComponentIterMut<'a, T> {
            match archetype.write_component::<T>() {
                Some(iter) => iter,
                None => panic!(
                    "Failed to fetch write access to component {}",
                    T::type_name()
                ),
            }
        }
    }

    pub fn write<T: Component>() -> Write<T> {
        Write::default()
    }
}
