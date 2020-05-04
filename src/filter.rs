use {
    crate::{
        access::{Accessor, ArchetypeAccess},
        archetype::ArchetypeInfo,
        component::{Component, ComponentId},
        util::MaybeIter,
        view::View,
    },
    std::fmt::{self, Debug},
};

/// Simple filter for achetypes.
pub trait Filter {
    fn filter_archetype(&self, archetype: &ArchetypeInfo) -> bool;

    /// Add another filter to the stack.
    fn filtered<F>(self, filter: F) -> Filtered<Self, F>
    where
        Self: Sized,
        F: Filter,
    {
        Filtered::new(self, filter)
    }

    /// Additionally filter only archetypes with specified component.
    fn with<T: Component>(self) -> Filtered<Self, With>
    where
        Self: Sized,
    {
        self.with_id(T::component_id())
    }

    /// Additionally filter only archetypes with specified component.
    fn with_id(self, component: ComponentId) -> Filtered<Self, With>
    where
        Self: Sized,
    {
        self.filtered(With::new(component))
    }

    /// Additionally filter only archetypes without specified component.
    fn without<T: Component>(self) -> Filtered<Self, Without>
    where
        Self: Sized,
    {
        self.without_id(T::component_id())
    }

    /// Additionally filter only archetypes without specified component.
    fn without_id(self, component: ComponentId) -> Filtered<Self, Without>
    where
        Self: Sized,
    {
        self.filtered(Without::new(component))
    }
}

macro_rules! tuple_filters {
    ($($a:ident),*) => {
        impl<$($a),*> Filter for ($($a,)*)
        where
            $($a: Filter,)*
        {
            fn filter_archetype(&self, archetype: &ArchetypeInfo) -> bool {
                #![allow(non_snake_case)]
                #![allow(unused_variables)]

                let ($($a,)*) = self;
                true $(&& $a.filter_archetype(archetype))*
            }
        }
    };
}

for_sequences!(tuple_filters);

impl<F> Filter for &'_ F
where
    F: Filter,
{
    fn filter_archetype(&self, archetype: &ArchetypeInfo) -> bool {
        F::filter_archetype(*self, archetype)
    }
}

impl<F> Filter for &'_ mut F
where
    F: Filter,
{
    fn filter_archetype(&self, archetype: &ArchetypeInfo) -> bool {
        F::filter_archetype(*self, archetype)
    }
}

/// Inner accessor|filter|view with additional filter.
pub struct Filtered<T, F> {
    inner: T,
    filter: F,
}

impl<T, F> Filtered<T, F> {
    pub fn new(inner: T, filter: F) -> Self {
        Filtered { inner, filter }
    }
}

impl<'a, A, F> Accessor<'a> for Filtered<A, F>
where
    A: Accessor<'a>,
    F: Filter,
{
    type AccessTypes = MaybeIter<A::AccessTypes>;
    fn access_types(&'a self, archetype: &ArchetypeInfo) -> Self::AccessTypes {
        if self.filter.filter_archetype(archetype) {
            MaybeIter::Just(self.inner.access_types(archetype))
        } else {
            MaybeIter::Nothing
        }
    }
}

impl<'a, T, F> Filter for Filtered<T, F>
where
    T: Filter,
    F: Filter,
{
    fn filter_archetype(&self, archetype: &ArchetypeInfo) -> bool {
        self.inner.filter_archetype(archetype) && self.filter.filter_archetype(archetype)
    }
}

impl<'a, V, F> View<'a> for Filtered<V, F>
where
    V: View<'a>,
    F: Filter,
{
    type EntityView = V::EntityView;
    type ChunkView = V::ChunkView;
    type ArchetypeView = V::ArchetypeView;

    fn view(&self, archetype: &mut ArchetypeAccess<'a>) -> Self::ArchetypeView {
        self.inner.view(archetype)
    }
}

/// Filters archetypes with specifeid component type.
pub struct With(ComponentId);

impl Clone for With {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for With {}

impl Debug for With {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "With{:?}", self.0)
    }
}

impl With {
    /// Returns new instance of query `With`.
    pub fn new(component: ComponentId) -> Self {
        With(component)
    }
}

impl Filter for With {
    fn filter_archetype(&self, archetype: &ArchetypeInfo) -> bool {
        archetype.has(self.0)
    }
}

/// Filters archetypes without specifeid component type.
pub struct Without(ComponentId);

impl Clone for Without {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for Without {}

impl Debug for Without {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "Without{:?}", self.0)
    }
}

impl Without {
    /// Returns new instance of query `Without`.
    pub fn new(component: ComponentId) -> Self {
        Without(component)
    }
}

impl Filter for Without {
    fn filter_archetype(&self, archetype: &ArchetypeInfo) -> bool {
        !archetype.has(self.0)
    }
}
