use {
    crate::{
        access::{Accessor, ArchetypeAccess, ComponentAccess},
        archetype::{ArchetypeComponentIterMut, ArchetypeInfo, TryArchetypeComponentIterMut},
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
/// Declared mutably access to the component,\
/// filters out archetypes without that component\
/// and yields mutable references to the component.
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
    /// Returns new instance of query `Write`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<'a, T> Accessor<'a> for Write<T>
where
    T: Component,
{
    type AccessTypes = std::option::IntoIter<ComponentAccess>;

    fn access_types(&self, archetype: &ArchetypeInfo) -> std::option::IntoIter<ComponentAccess> {
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
    type ChunkView = std::slice::IterMut<'a, T>;
    type ArchetypeView = ArchetypeComponentIterMut<'a, T>;

    fn view(&self, archetype: &mut ArchetypeAccess<'a>) -> ArchetypeComponentIterMut<'a, T> {
        debug_assert!(self.filter_archetype(archetype.info()));

        match archetype.write_component::<T>() {
            Some(iter) => iter,
            None => panic!(
                "Failed to fetch write access to component {}",
                T::type_name()
            ),
        }
    }
}

/// Returns new query to write component `T`.
pub fn write<T: Component>() -> Write<T> {
    Write::default()
}

crate::impl_and!(<T: Component> for Write<T>);
crate::impl_or!(<T: Component> for Write<T>);

/// Query to read component of type `T`.
/// Declared immutably access to the component,\
/// unlike `Read` does not filter out archetypes without component `T`
/// but yields `None`s for each entity.
/// Otherwise yields immutable reference to `T`.
pub struct TryWrite<T>(PhantomData<fn() -> T>);

impl<T> Clone for TryWrite<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for TryWrite<T> {}

impl<T> Debug for TryWrite<T>
where
    T: Component,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "TryWrite<{}>", T::type_name())
    }
}

impl<T> Default for TryWrite<T> {
    fn default() -> Self {
        TryWrite(PhantomData)
    }
}

impl<T> TryWrite<T> {
    /// Returns new instance of query `TryWrite`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<'a, T> Accessor<'a> for TryWrite<T>
where
    T: Component,
{
    type AccessTypes = std::option::IntoIter<ComponentAccess>;

    fn access_types(&self, archetype: &ArchetypeInfo) -> std::option::IntoIter<ComponentAccess> {
        ComponentAccess::write::<T>(archetype).into_iter()
    }
}

impl<T> Filter for TryWrite<T>
where
    T: Component,
{
    fn filter_archetype(&self, _: &ArchetypeInfo) -> bool {
        true
    }
}

impl<'a, T> View<'a> for TryWrite<T>
where
    T: Component,
{
    type EntityView = Option<&'a mut T>;
    type ChunkView = TryIter<std::slice::IterMut<'a, T>>;
    type ArchetypeView = TryArchetypeComponentIterMut<'a, T>;

    fn view(&self, archetype: &mut ArchetypeAccess<'a>) -> TryArchetypeComponentIterMut<'a, T> {
        archetype.try_write_component::<T>()
    }
}

/// Returns new query to write component `T`.
pub fn try_write<T: Component>() -> TryWrite<T> {
    TryWrite::default()
}

crate::impl_and!(<T: Component> for TryWrite<T>);
crate::impl_or!(<T: Component> for TryWrite<T>);
