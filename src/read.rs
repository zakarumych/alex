use {
    crate::{
        access::{Accessor, ArchetypeAccess, ComponentAccess},
        archetype::{ArchetypeComponentIter, ArchetypeInfo, TryArchetypeComponentIter},
        component::Component,
        filter::Filter,
        util::TryIter,
        view::View,
    },
    std::{
        fmt::{self, Debug},
        marker::PhantomData,
    },
};

/// Query to read component of type `T`.
/// Declared immutably access to the component,\
/// filters out archetypes without that component\
/// and yields immutable references to the component.
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
    /// Returns new instance of query `Read`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<'a, T> Accessor<'a> for Read<T>
where
    T: Component,
{
    type AccessTypes = std::option::IntoIter<ComponentAccess>;

    fn access_types(&self, archetype: &ArchetypeInfo) -> std::option::IntoIter<ComponentAccess> {
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
    type ChunkView = std::slice::Iter<'a, T>;
    type ArchetypeView = ArchetypeComponentIter<'a, T>;

    fn view(&self, archetype: &mut ArchetypeAccess<'a>) -> ArchetypeComponentIter<'a, T> {
        debug_assert!(self.filter_archetype(archetype.info()));

        match archetype.read_component::<T>() {
            Some(iter) => iter,
            None => panic!(
                "Failed to fetch read access to component {}",
                T::type_name()
            ),
        }
    }
}

/// Returns new query to read component `T`.
pub fn read<T: Component>() -> Read<T> {
    Read::default()
}

crate::impl_and!(<T: Component> for Read<T>);
crate::impl_or!(<T: Component> for Read<T>);

/// Query to read component of type `T`.
/// Declared immutably access to the component,\
/// unlike `Read` does not filter out archetypes without component `T`
/// but yields `None`s for each entity.
/// Otherwise yields immutable reference to `T`.
pub struct TryRead<T>(PhantomData<fn() -> T>);

impl<T> Clone for TryRead<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for TryRead<T> {}

impl<T> Debug for TryRead<T>
where
    T: Component,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "TryRead<{}>", T::type_name())
    }
}

impl<T> Default for TryRead<T> {
    fn default() -> Self {
        TryRead(PhantomData)
    }
}

impl<T> TryRead<T> {
    /// Returns new instance of query `TryRead`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<'a, T> Accessor<'a> for TryRead<T>
where
    T: Component,
{
    type AccessTypes = std::option::IntoIter<ComponentAccess>;

    fn access_types(&self, archetype: &ArchetypeInfo) -> std::option::IntoIter<ComponentAccess> {
        ComponentAccess::read::<T>(archetype).into_iter()
    }
}

impl<T> Filter for TryRead<T>
where
    T: Component,
{
    fn filter_archetype(&self, _: &ArchetypeInfo) -> bool {
        true
    }
}

impl<'a, T> View<'a> for TryRead<T>
where
    T: Component,
{
    type EntityView = Option<&'a T>;
    type ChunkView = TryIter<std::slice::Iter<'a, T>>;
    type ArchetypeView = TryArchetypeComponentIter<'a, T>;

    fn view(&self, archetype: &mut ArchetypeAccess<'a>) -> TryArchetypeComponentIter<'a, T> {
        archetype.try_read_component::<T>()
    }
}

/// Returns new query to read component `T`.
pub fn try_read<T: Component>() -> TryRead<T> {
    TryRead::default()
}

crate::impl_and!(<T: Component> for TryRead<T>);
crate::impl_or!(<T: Component> for TryRead<T>);
