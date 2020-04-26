use {
    crate::{
        archetype::{
            Archetype, ArchetypeComponentIter, ArchetypeComponentIterMut, ArchetypeInfo,
            RawArchetypeComponentIter,
        },
        component::{Component, ComponentId},
        view::{ArchetypeView, ArchetypeViewIter, ChunkView, ChunkViewIter, EntityView, View},
    },
    bumpalo::{collections::Vec as BVec, Bump},
    std::marker::PhantomData,
};

/// Kind of access to the particular component type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Allows only immutable access.
    /// Multiple read access can be performed simultaneously.
    Read,

    /// Allows mutable access.
    /// Only one write access and no reads can be performed at a time.
    Write,
}

/// Describes access to the component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentAccess {
    /// Id of the component.
    pub component: ComponentId,

    /// Granted access.
    pub access: Access,

    /// Offset of component in the archetype.
    pub offset: usize,
}

impl ComponentAccess {
    pub fn read<T: Component>(archetype: &ArchetypeInfo) -> Option<Self> {
        ComponentAccess {
            component: T::component_id(),
            access: Access::Read,
            offset: archetype.component_offset(T::component_id())?,
        }
        .into()
    }
    pub fn write<T: Component>(archetype: &ArchetypeInfo) -> Option<Self> {
        ComponentAccess {
            component: T::component_id(),
            access: Access::Write,
            offset: archetype.component_offset(T::component_id())?,
        }
        .into()
    }
}

/// Declares set of access types to components.
pub trait Accessor<'a> {
    /// Iterator over component and access pairs for this accessor.
    type AccessTypes: Iterator<Item = ComponentAccess> + 'a;

    /// Returns an iterator over component and access pairs
    /// this accessor wants to perform.
    fn access_types(&'a self, archetype: &ArchetypeInfo) -> Self::AccessTypes;
}

struct ComponentAccessOffset {
    component: ComponentId,
    access: Access,
    offset: usize,
}

/// Restricted access to an archetype.
/// Instances of this type contain particular set of granted access types and do runtime checks.
pub struct ArchetypeAccess<'a> {
    /// Shared archetype.
    archetype: &'a Archetype,

    /// Array of access types this instance grants to its owner.
    access_types: BVec<'a, ComponentAccessOffset>,

    /// Deny clone derive.
    marker: PhantomData<&'a mut u8>,
}

impl<'a> ArchetypeAccess<'a> {
    /// Create arhetype access.
    ///
    /// # Safety
    ///
    /// This function must be used only when archetype is externally
    /// synchronized for specified access types.
    pub unsafe fn new(
        archetype: &'a Archetype,
        access_types: impl Iterator<Item = ComponentAccess>,
        bump: &'a Bump,
    ) -> Self {
        ArchetypeAccess {
            access_types: BVec::from_iter_in(
                access_types.filter_map(|item| {
                    ComponentAccessOffset {
                        component: item.component,
                        access: item.access,
                        offset: archetype.info().component_offset(item.component)?,
                    }
                    .into()
                }),
                bump,
            ),
            archetype,
            marker: PhantomData,
        }
    }

    pub fn info(&self) -> &ArchetypeInfo {
        self.archetype.info()
    }

    /// Fetches specified access to the component.
    /// Returns `None` if that access to specified component cannot be granted.
    pub fn access_component(
        &mut self,
        access: Access,
        component: ComponentId,
    ) -> Option<RawArchetypeComponentIter<'a>> {
        let index = self
            .access_types
            .iter()
            .position(|item| item.component == component)?;

        // Downgrade to `Read`.
        match (access, &mut self.access_types[index].access) {
            (Access::Read, access) => {
                *access = Access::Read;
            }
            (Access::Write, Access::Write) => {
                self.access_types.swap_remove(index);
            }
            (Access::Write, Access::Read) => return None,
        }

        unsafe {
            self.archetype
                .access_component(access, component, self.access_types[index].offset)
        }
        .into()
    }

    /// Fetches `Read` access to the component.
    /// Returns `None` if no access to specified component wasn't granted.
    /// If `Write` access was granted it is downgraded to `Read`.
    pub fn read_component<T: Component>(&mut self) -> Option<ArchetypeComponentIter<'a, T>> {
        let index = self
            .access_types
            .iter()
            .position(|item| item.component == T::component_id())?;

        // Downgrade to `Read`.
        self.access_types[index].access = Access::Read;

        unsafe {
            self.archetype
                .read_component::<T>(self.access_types[index].offset)
        }
        .into()
    }

    /// Fetches `Write` access to the component.
    /// Returns `None` if `Write` access to specified component wasn't granted.
    pub fn write_component<T: Component>(&mut self) -> Option<ArchetypeComponentIterMut<'a, T>> {
        let index = self
            .access_types
            .iter()
            .position(|item| item.component == T::component_id())?;

        // Check it has`Write` access.
        if self.access_types[index].access != Access::Write {
            return None;
        }

        let access_type = self.access_types.swap_remove(index);

        unsafe { self.archetype.write_component::<T>(access_type.offset) }.into()
    }
}

/// Iterator over `ArchetypeAccess` instances.
pub struct WorldAccess<'a> {
    archetypes: &'a [Archetype],
    access_types: &'a [BVec<'a, ComponentAccess>],
    bump: &'a Bump,
    marker: PhantomData<&'a mut u8>,
}

impl<'a> WorldAccess<'a> {
    /// Create arhetype access.
    ///
    /// # Safety
    ///
    /// This function must be used only for archetypes that are
    /// externally synchronized for specified access types.
    pub unsafe fn new(
        archetypes: &'a [Archetype],
        access_types: &'a [BVec<'a, ComponentAccess>],
        bump: &'a Bump,
    ) -> Self {
        WorldAccess {
            archetypes,
            access_types,
            bump,
            marker: PhantomData,
        }
    }

    /// Reborrow to iterate more than once.
    /// This is safe as original iterator is inaccessible until created iterator is dropped.
    pub fn by_ref<'b>(&'b mut self) -> WorldAccess<'b> {
        WorldAccess {
            archetypes: self.archetypes,
            access_types: self.access_types,
            bump: self.bump,
            marker: PhantomData,
        }
    }

    pub fn iter_archetypes<'b, V>(&'b mut self, view: &'b V) -> WorldAccessArchetypeIter<'b, V>
    where
        V: View<'b>,
    {
        WorldAccessArchetypeIter {
            view,
            archetypes: self.archetypes.iter(),
            access_types: self.access_types.iter(),
            bump: self.bump,
            marker: PhantomData,
        }
    }

    pub fn iter_chunks<'b, V>(&'b mut self, view: &'b V) -> WorldAccessChunkIter<'b, V>
    where
        V: View<'b>,
    {
        WorldAccessChunkIter {
            current: None,
            iter: self.iter_archetypes(view),
        }
    }

    pub fn iter_entities<'b, V>(&'b mut self, view: &'b V) -> WorldAccessEntityIter<'b, V>
    where
        V: View<'b>,
    {
        WorldAccessEntityIter {
            current: None,
            iter: self.iter_chunks(view),
        }
    }
}

/// Iterator over `ArchetypeView`s for the `View`
pub struct WorldAccessArchetypeIter<'a, V: View<'a>> {
    view: &'a V,
    archetypes: std::slice::Iter<'a, Archetype>,
    access_types: std::slice::Iter<'a, BVec<'a, ComponentAccess>>,
    bump: &'a Bump,
    marker: PhantomData<&'a mut u8>,
}

impl<'a, V> Iterator for WorldAccessArchetypeIter<'a, V>
where
    V: View<'a>,
{
    type Item = ArchetypeView<'a, V>;

    fn next(&mut self) -> Option<ArchetypeView<'a, V>> {
        loop {
            let archetype = self.archetypes.next()?;
            let access_types = self
                .access_types
                .next()
                .expect("Length must match archetypes iter");
            if self.view.filter_archetype(archetype.info()) {
                let mut next = unsafe {
                    ArchetypeAccess::new(archetype, access_types.iter().copied(), self.bump)
                };
                return self.view.view(&mut next).into();
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, self.archetypes.size_hint().1)
    }

    fn last(self) -> Option<ArchetypeView<'a, V>> {
        for (archetype, access_types) in self.archetypes.zip(self.access_types).rev() {
            if self.view.filter_archetype(archetype.info()) {
                let mut next = unsafe {
                    ArchetypeAccess::new(archetype, access_types.iter().copied(), self.bump)
                };
                return self.view.view(&mut next).into();
            }
        }
        None
    }
}

/// Iterator over `ChunkView`s for the `View`
pub struct WorldAccessChunkIter<'a, V: View<'a>> {
    current: Option<ArchetypeViewIter<'a, V>>,
    iter: WorldAccessArchetypeIter<'a, V>,
}

impl<'a, V> Iterator for WorldAccessChunkIter<'a, V>
where
    V: View<'a>,
{
    type Item = ChunkView<'a, V>;

    fn next(&mut self) -> Option<ChunkView<'a, V>> {
        loop {
            if let Some(current) = &mut self.current {
                if let Some(next) = current.next() {
                    return Some(next);
                }
            }

            self.current = Some(self.iter.next()?.into_iter());
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if let Some(current) = &self.current {
            (current.size_hint().0, None)
        } else {
            (0, None)
        }
    }

    fn last(self) -> Option<ChunkView<'a, V>> {
        let last = match self.iter.last() {
            Some(last) => Some(last.into_iter()),
            None => self.current,
        };
        last?.last()
    }
}

pub struct WorldAccessEntityIter<'a, V: View<'a>> {
    current: Option<ChunkViewIter<'a, V>>,
    iter: WorldAccessChunkIter<'a, V>,
}

impl<'a, V> Iterator for WorldAccessEntityIter<'a, V>
where
    V: View<'a>,
{
    type Item = EntityView<'a, V>;

    fn next(&mut self) -> Option<EntityView<'a, V>> {
        loop {
            if let Some(current) = &mut self.current {
                if let Some(next) = current.next() {
                    return Some(next);
                }
            }

            self.current = Some(self.iter.next()?.into_iter());
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if let Some(current) = &self.current {
            (current.size_hint().0, None)
        } else {
            (0, None)
        }
    }

    fn last(self) -> Option<EntityView<'a, V>> {
        let last = match self.iter.last() {
            Some(last) => Some(last.into_iter()),
            None => self.current,
        };
        last?.last()
    }
}
