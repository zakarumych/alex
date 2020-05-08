use {
    crate::{
        archetype::{
            Archetype, ArchetypeComponentIter, ArchetypeComponentIterMut, ArchetypeInfo,
            ChunkSizes, RawArchetypeComponentIter, TryArchetypeComponentIter,
            TryArchetypeComponentIterMut,
        },
        component::{Component, ComponentId},
        entity::{Entities, Entity},
        util::ChainIter,
        view::{ArchetypeView, ChunkView, EntityView, View},
    },
    bumpalo::{collections::Vec as BVec, Bump},
    std::marker::PhantomData,
};

/// Kind of the access. Immutable read or mutable write.
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
    /// Returns read component access for the archetype info.
    /// Returns `None` if archetype doesn't have this component.
    pub fn read<T: Component>(archetype: &ArchetypeInfo) -> Option<Self> {
        ComponentAccess {
            component: T::component_id(),
            access: Access::Read,
            offset: archetype.component_offset(T::component_id())?,
        }
        .into()
    }

    /// Returns write component access for the archetype info.
    /// Returns `None` if archetype doesn't have this component.
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
/// Used by `Schedule` for systems synchronization.
pub trait Accessor<'a> {
    /// Iterator over component-access values for this accessor.
    type AccessTypes: Iterator<Item = ComponentAccess> + 'a;

    /// Returns an iterator over component-access values
    /// this accessor wants to perform.
    fn access_types(&'a self, archetype: &ArchetypeInfo) -> Self::AccessTypes;
}

impl<'a, A> Accessor<'a> for &'_ A
where
    A: Accessor<'a>,
{
    type AccessTypes = A::AccessTypes;

    fn access_types(&'a self, archetype: &ArchetypeInfo) -> Self::AccessTypes {
        A::access_types(*self, archetype)
    }
}

impl<'a, A> Accessor<'a> for &'_ mut A
where
    A: Accessor<'a>,
{
    type AccessTypes = A::AccessTypes;

    fn access_types(&'a self, archetype: &ArchetypeInfo) -> Self::AccessTypes {
        A::access_types(*self, archetype)
    }
}

macro_rules! tuple_accessor {
    ($($a:ident),*) => {
        impl<'a $(, $a)*> Accessor<'a> for ($($a,)*)
        where
            $($a: Accessor<'a>,)*
        {
            type AccessTypes = ChainIter<ComponentAccess, ($($a::AccessTypes,)*)>;

            fn access_types(&'a self, archetype: &ArchetypeInfo) -> Self::AccessTypes {
                #![allow(non_snake_case)]
                #![allow(unused_variables)]

                let ($($a,)*) = self;
                ChainIter::new(($($a.access_types(archetype),)*))
            }
        }
    };
}

for_sequences!(tuple_accessor);

/// Restricted access to an archetype.
/// Instances of this type contain particular set of granted access types
/// and check them in runtime.
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct ArchetypeAccess<'a> {
    /// Shared archetype.
    archetype: &'a Archetype,
    /// Array of access types this instance grants to its owner.
    access_types: BVec<'a, ComponentAccess>,
    /// Bump allocator for reborrows.
    bump: &'a Bump,
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
    pub(crate) unsafe fn new(
        archetype: &'a Archetype,
        access_types: &'a [ComponentAccess],
        bump: &'a Bump,
    ) -> Self {
        ArchetypeAccess {
            archetype,
            access_types: BVec::from_iter_in(access_types.iter().copied(), bump),
            bump,
            marker: PhantomData,
        }
    }

    pub fn take<A>(&mut self, accessor: A) -> Self
    where
        A: for<'b> Accessor<'b>,
    {
        let access_types = &mut self.access_types;

        let taken = BVec::from_iter_in(
            accessor
                .access_types(self.archetype.info())
                .filter_map(|requested| {
                    let index = access_types
                        .iter()
                        .position(|item| item.component == requested.component)?;

                    match (requested.access, &mut access_types[index].access) {
                        (Access::Read, access) => {
                            *access = Access::Read;
                        }
                        (Access::Write, Access::Write) => {
                            access_types.swap_remove(index);
                        }
                        (Access::Write, Access::Read) => return None,
                    }

                    Some(requested)
                }),
            self.bump,
        );

        ArchetypeAccess {
            archetype: self.archetype,
            access_types: taken,
            bump: self.bump,
            marker: PhantomData,
        }
    }

    pub fn reborrow<'b>(&'b mut self) -> ArchetypeAccess<'b> {
        ArchetypeAccess {
            archetype: self.archetype,
            access_types: self.access_types.clone(),
            bump: self.bump,
            marker: PhantomData,
        }
    }

    pub fn info(&self) -> &ArchetypeInfo {
        self.archetype.info()
    }

    pub fn chunk_sizes(&self) -> ChunkSizes {
        self.archetype.chunk_sizes()
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
    pub fn borrow_read_component<T: Component>(&self) -> Option<ArchetypeComponentIter<'_, T>> {
        let index = self
            .access_types
            .iter()
            .position(|item| item.component == T::component_id())?;

        unsafe {
            self.archetype
                .read_component::<T>(self.access_types[index].offset)
        }
        .into()
    }

    /// Fetches `Write` access to the component.
    /// Returns `None` if `Write` access to specified component wasn't granted.
    pub fn borrow_write_component<T: Component>(
        &mut self,
    ) -> Option<ArchetypeComponentIterMut<'_, T>> {
        let index = self
            .access_types
            .iter()
            .position(|item| item.component == T::component_id())?;

        // Check it has`Write` access.
        if self.access_types[index].access != Access::Write {
            return None;
        }

        unsafe {
            self.archetype
                .write_component::<T>(self.access_types[index].offset)
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
    /// This function removes `Write` component so it can't be fetched twice.
    /// To fetch multiple times see `borrow_write_component`.
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

    /// Fetches `Read` access to the component.
    /// Returns `None` if no access to specified component wasn't granted.
    /// If `Write` access was granted it is downgraded to `Read`.
    pub fn try_read_component<T: Component>(&mut self) -> TryArchetypeComponentIter<'a, T> {
        match self
            .access_types
            .iter()
            .position(|item| item.component == T::component_id())
        {
            Some(index) => {
                // Downgrade to `Read`.
                self.access_types[index].access = Access::Read;

                unsafe {
                    self.archetype
                        .read_component::<T>(self.access_types[index].offset)
                }
                .into()
            }
            None => self.archetype.chunk_sizes().into(),
        }
    }

    /// Fetches `Write` access to the component.
    /// Returns `None` if no access to specified component wasn't granted.
    /// If `Write` access was granted it is downgraded to `Write`.
    pub fn try_write_component<T: Component>(&mut self) -> TryArchetypeComponentIterMut<'a, T> {
        match self
            .access_types
            .iter()
            .position(|item| item.component == T::component_id())
        {
            Some(index) => {
                // Check it has`Write` access.
                if self.access_types[index].access != Access::Write {
                    return self.archetype.chunk_sizes().into();
                }

                let access_type = self.access_types.swap_remove(index);

                unsafe { self.archetype.write_component::<T>(access_type.offset) }.into()
            }
            None => self.archetype.chunk_sizes().into(),
        }
    }
}

// /// Restricted access to `World` for use in scheduled systems.
// #[cfg_attr(debug_assertions, derive(Debug))]
// pub struct WorldAccess<'a> {
//     archetypes: &'a [Archetype],
//     access_types: &'a [BVec<'a, ComponentAccess>],
//     entities: &'a Entities,
//     bump: &'a Bump,

//     /// Deny clone derive.
//     marker: PhantomData<&'a mut u8>,
// }

// impl<'a> WorldAccess<'a> {
//     /// Create arhetype access.
//     ///
//     /// # Safety
//     ///
//     /// This function must be used only for archetypes that are
//     /// externally synchronized for specified access types.
//     pub unsafe fn new(
//         entities: &'a Entities,
//         archetypes: &'a [Archetype],
//         access_types: &'a [BVec<'a, ComponentAccess>],
//         bump: &'a Bump,
//     ) -> Self {
//         WorldAccess {
//             entities,
//             archetypes,
//             access_types,
//             bump,
//             marker: PhantomData,
//         }
//     }

//     pub fn read<T: Component>(&self, entity: &Entity) -> Option<&T> {
//         let location = self.entities.get(entity)?;
//         let archetype_index = usize::try_from(location.archetype).unwrap();
//         let ca = self.access_types[archetype_index]
//             .iter()
//             .find(|item| item.component == T::component_id())
//             .unwrap();

//         let (chunk_index, entity_index) = self.archetypes[archetype_index]
//             .info()
//             .split_entity_index(location.entity);

//         let mut archetype = unsafe {
//             // Access checked above
//             self.archetypes[archetype_index].read_component(ca.offset)
//         };
//         archetype
//             .nth(chunk_index.as_usize())?
//             .nth(entity_index.as_usize())
//     }

//     pub fn write<T: Component>(&mut self, entity: &Entity) -> Option<&mut T> {
//         let location = self.entities.get(entity)?;
//         let archetype_index = usize::try_from(location.archetype).unwrap();
//         let ca = self.access_types[archetype_index]
//             .iter()
//             .find(|item| item.component == T::component_id())
//             .unwrap();

//         assert_eq!(ca.access, Access::Write);

//         let (chunk_index, entity_index) = self.archetypes[archetype_index]
//             .info()
//             .split_entity_index(location.entity);

//         let mut archetype = unsafe {
//             // Access checked above
//             self.archetypes[archetype_index].write_component(ca.offset)
//         };
//         archetype
//             .nth(chunk_index.as_usize())?
//             .nth(entity_index.as_usize())
//     }

//     /// Returns query constructed from view.
//     /// View will borrow required access types for whole query lifetime which makes it impossible to create multiple
//     /// queries with overlapping scope. But same query can be created repeatedly in non-overlapping scopes.
//     /// Use `split` method to trade this limitation for reusability.
//     ///
//     /// # Panics
//     ///
//     /// Returns iterator may panic if required access is not granted by this `WorldAccess` instance.
//     pub fn query<'b, V>(&'b mut self, view: V) -> Query<'b, V>
//     where
//         V: View<'b>,
//     {
//         Query {
//             archetypes: BVec::from_iter_in(
//                 self.archetypes.iter().zip(self.access_types).filter_map(
//                     |(archetype, access_types)| {
//                         if view.filter_archetype(archetype.info()) {
//                             let mut access = unsafe {
//                                 // Access types are granted as guaranteed by `WorldAccess::new` contract.
//                                 // Mutable borrow of `WorldAccess` guarantees that
//                                 // resuling archetype access can't be created twice in overlapping scope
//                                 ArchetypeAccess::new(
//                                     archetype,
//                                     access_types.iter().copied(),
//                                     self.bump,
//                                 )
//                             };

//                             Some(view.view(&mut access))
//                         } else {
//                             None
//                         }
//                     },
//                 ),
//                 self.bump,
//             ),
//         }
//     }

//     /// Returns query constructed from view.
//     /// Similar to `query` but substitutes `Write` access with `Read` access for the view.
//     /// This allows to create multiple read-only queries.
//     ///
//     /// # Panics
//     ///
//     /// Returns iterator may panic if required access is not granted by this `WorldAccess` instance.
//     pub fn query_immutable<'b, V>(&'b self, view: V) -> Query<'b, V>
//     where
//         V: View<'b>,
//     {
//         Query {
//             archetypes: BVec::from_iter_in(
//                 self.archetypes.iter().zip(self.access_types).filter_map(
//                     |(archetype, access_types)| {
//                         if view.filter_archetype(archetype.info()) {
//                             let mut access = unsafe {
//                                 // Access types are granted as guaranteed by `WorldAccess::new` contract.
//                                 // Mutable borrow of `WorldAccess` guarantees that
//                                 // resuling archetype access can't be created twice in overlapping scope
//                                 ArchetypeAccess::new(
//                                     archetype,
//                                     access_types.iter().map(|&ca| ComponentAccess {
//                                         access: Access::Read,
//                                         ..ca
//                                     }),
//                                     self.bump,
//                                 )
//                             };

//                             Some(view.view(&mut access))
//                         } else {
//                             None
//                         }
//                     },
//                 ),
//                 self.bump,
//             ),
//         }
//     }

//     /// Returns iterator over archetype views for specified view instance.
//     /// View will borrow required access types for whole iterator lifetime which makes it impossible to create multiple
//     /// queries with overlapping scope. But same iterator can be created repeatedly in non-overlapping scopes.
//     /// Use `split` method to trade this limitation for reusability.
//     ///
//     /// # Panics
//     ///
//     /// Returns iterator may panic if required access is not granted by this `WorldAccess` instance.
//     pub fn archetypes_iter<'b, V>(&'b mut self, view: V) -> WorldAccessArchetypeIter<'b, V>
//     where
//         V: View<'b>,
//     {
//         self.query(view).archetypes_iter()
//     }

//     /// Returns iterator over chunk views for specified view instance.
//     /// View will borrow required access types for whole iterator lifetime which makes it impossible to create multiple
//     /// queries with overlapping scope. But same iterator can be created repeatedly in non-overlapping scopes.
//     /// Use `split` method to trade this limitation for reusability.
//     ///
//     /// # Panics
//     ///
//     /// Returns iterator may panic if required access is not granted by this `WorldAccess` instance.
//     pub fn chunks_iter<'b, V>(&'b mut self, view: V) -> WorldAccessChunkIter<'b, V>
//     where
//         V: View<'b>,
//     {
//         self.query(view).chunks_iter()
//     }

//     /// Returns iterator over entity views for specified view instance.
//     /// View will borrow required access types for whole iterator lifetime which makes it impossible to create multiple
//     /// queries with overlapping scope. But same iterator can be created repeatedly in non-overlapping scopes.
//     /// Use `split` method to trade this limitation for reusability.
//     ///
//     /// # Panics
//     ///
//     /// Returns iterator may panic if required access is not granted by this `WorldAccess` instance.
//     pub fn entities_iter<'b, V>(&'b mut self, view: V) -> WorldAccessEntityIter<'b, V>
//     where
//         V: View<'b>,
//     {
//         self.query(view).entities_iter()
//     }

//     /// Returns splitting world access instance that can be used to create multiple writing queries or iterators with overlapping scopes.
//     pub fn split<'b>(&'b mut self) -> SplitWorldAccess<'b> {
//         SplitWorldAccess {
//             archetypes: BVec::from_iter_in(
//                 self.archetypes
//                     .iter()
//                     .zip(self.access_types)
//                     .map(|(archetype, access_types)| {
//                         unsafe {
//                             // Access types are granted as guaranteed by `WorldAccess::new` contract.
//                             // Mutable borrow of `WorldAccess` guarantees that
//                             // resuling archetype access can't be created twice in overlapping scope
//                             ArchetypeAccess::new(archetype, access_types.iter().copied(), self.bump)
//                         }
//                     }),
//                 self.bump,
//             ),
//             entities: self.entities,
//             bump: self.bump,
//         }
//     }
// }

/// Restricted access to `World` for use in scheduled systems.
/// Unlike `WorldAccess` it is not reusable, but instead can create multiple queries and iterators.
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct WorldAccess<'a> {
    archetypes: BVec<'a, ArchetypeAccess<'a>>,
    entities: &'a Entities,
    bump: &'a Bump,
}

impl<'a> WorldAccess<'a> {
    /// Create arhetype access.
    ///
    /// # Safety
    ///
    /// This function must be used only for archetypes that are
    /// externally synchronized for specified access types.
    pub(crate) unsafe fn new(
        entities: &'a Entities,
        archetypes: &'a [Archetype],
        access_types: &'a [BVec<'a, ComponentAccess>],
        bump: &'a Bump,
    ) -> Self {
        WorldAccess {
            archetypes: BVec::from_iter_in(
                archetypes
                    .iter()
                    .zip(access_types)
                    .map(|(archetype, access_types)| {
                        ArchetypeAccess::new(archetype, access_types, bump)
                    }),
                bump,
            ),
            entities,
            bump,
        }
    }

    pub fn reborrow(&mut self) -> WorldAccess<'_> {
        WorldAccess {
            archetypes: BVec::from_iter_in(
                self.archetypes.iter_mut().map(ArchetypeAccess::reborrow),
                self.bump,
            ),
            entities: self.entities,
            bump: self.bump,
        }
    }

    /// Splits world access instance.
    /// Returns world access with all access types that `accessor` tries to fetch.
    /// Requested `Read`s are shared betweeen resulting `WorldAccess` instaces (`Write` is downgraded to `Read`).
    pub fn take<A>(&mut self, accessor: A) -> WorldAccess<'a>
    where
        A: for<'b> Accessor<'b>,
    {
        WorldAccess {
            archetypes: BVec::from_iter_in(
                self.archetypes
                    .iter_mut()
                    .map(|archetype| archetype.take(&accessor)),
                self.bump,
            ),
            entities: self.entities,
            bump: self.bump,
        }
    }

    pub fn read<T: Component>(&self, entity: &Entity) -> Option<&T> {
        let location = self.entities.get(entity)?;
        let archetype_index = location.archetype.as_usize();

        let (chunk_index, entity_index) = self.archetypes[archetype_index]
            .info()
            .split_entity_index(location.entity);

        if let Some(mut archetype) = self.archetypes[archetype_index].borrow_read_component() {
            archetype
                .nth(chunk_index.as_usize())?
                .nth(entity_index.as_usize())
        } else {
            panic!("Access to `{}` was not granted", T::type_name());
        }
    }

    pub fn write<T: Component>(&mut self, entity: &Entity) -> Option<&mut T> {
        let location = self.entities.get(entity)?;
        let archetype_index = location.archetype.as_usize();

        let (chunk_index, entity_index) = self.archetypes[archetype_index]
            .info()
            .split_entity_index(location.entity);

        if let Some(mut archetype) = self.archetypes[archetype_index].borrow_write_component() {
            archetype
                .nth(chunk_index.as_usize())?
                .nth(entity_index.as_usize())
        } else {
            panic!("Access to `{}` was not granted", T::type_name());
        }
    }

    /// Returns query constructed from view.
    /// Unlike `WorldAccess::take_query` view will just borrow access types and not take them out of this instance.
    ///
    /// # Panics
    ///
    /// Returns iterator may panic if required access is not granted by this `WorldAccess` instance.
    pub fn query<'b, V>(&'b mut self, view: V) -> Query<'b, V>
    where
        V: View<'b>,
    {
        Query {
            archetypes: BVec::from_iter_in(
                self.archetypes.iter_mut().filter_map(|archetype| {
                    if view.filter_archetype(archetype.info()) {
                        let mut archetype = archetype.reborrow();
                        Some(view.view(&mut archetype))
                    } else {
                        None
                    }
                }),
                self.bump,
            ),
        }
    }

    /// Returns query constructed from view.
    /// Unlike `WorldAccess::query` view will not borrow access types, but take them out of this instance.
    ///
    /// # Panics
    ///
    /// Returns iterator may panic if required access is not granted by this `WorldAccess` instance.
    pub fn take_query<V>(&mut self, view: V) -> Query<'a, V>
    where
        V: View<'a>,
    {
        Query {
            archetypes: BVec::from_iter_in(
                self.archetypes.iter_mut().filter_map(|archetype| {
                    if view.filter_archetype(archetype.info()) {
                        Some(view.view(archetype))
                    } else {
                        None
                    }
                }),
                self.bump,
            ),
        }
    }

    /// Returns iterator over archetype views for specified view instance.
    /// Note that unlike `WorldAccess` view will not borrow access types, but take them out of this instance.
    ///
    /// # Panics
    ///
    /// Returns iterator may panic if required access is not granted by this `WorldAccess` instance.
    pub fn archetypes_iter<'b, V>(&'b mut self, view: V) -> WorldAccessArchetypeIter<'b, V>
    where
        V: View<'b>,
    {
        self.query(view).archetypes_iter()
    }

    /// Returns iterator over chunk views for specified view instance.
    /// Note that unlike `WorldAccess` view will not borrow access types, but take them out of this instance.
    ///
    /// # Panics
    ///
    /// Returns iterator may panic if required access is not granted by this `WorldAccess` instance.
    pub fn chunks_iter<'b, V>(&'b mut self, view: V) -> WorldAccessChunkIter<'b, V>
    where
        V: View<'b>,
    {
        self.query(view).chunks_iter()
    }

    /// Returns iterator over entity views for specified view instance.
    /// Note that unlike `WorldAccess` view will not borrow access types, but take them out of this instance.
    ///
    /// # Panics
    ///
    /// Returns iterator may panic if required access is not granted by this `WorldAccess` instance.
    pub fn entities_iter<'b, V>(&'b mut self, view: V) -> WorldAccessEntityIter<'b, V>
    where
        V: View<'b>,
    {
        self.query(view).entities_iter()
    }

    /// Returns iterator over archetype views for specified view instance.
    /// Note that unlike `WorldAccess` view will not borrow access types, but take them out of this instance.
    ///
    /// # Panics
    ///
    /// Returns iterator may panic if required access is not granted by this `WorldAccess` instance.
    pub fn take_archetypes_iter<V>(&mut self, view: V) -> WorldAccessArchetypeIter<'a, V>
    where
        V: View<'a>,
    {
        self.take_query(view).archetypes_iter()
    }

    /// Returns iterator over chunk views for specified view instance.
    /// Note that unlike `WorldAccess` view will not borrow access types, but take them out of this instance.
    ///
    /// # Panics
    ///
    /// Returns iterator may panic if required access is not granted by this `WorldAccess` instance.
    pub fn take_chunks_iter<V>(&mut self, view: V) -> WorldAccessChunkIter<'a, V>
    where
        V: View<'a>,
    {
        self.take_query(view).chunks_iter()
    }

    /// Returns iterator over entity views for specified view instance.
    /// Note that unlike `WorldAccess` view will not borrow access types, but take them out of this instance.
    ///
    /// # Panics
    ///
    /// Returns iterator may panic if required access is not granted by this `WorldAccess` instance.
    pub fn take_entities_iter<V>(&mut self, view: V) -> WorldAccessEntityIter<'a, V>
    where
        V: View<'a>,
    {
        self.take_query(view).entities_iter()
    }
}

/// Query over `WorldAccess`.
pub struct Query<'a, V: View<'a>> {
    archetypes: BVec<'a, ArchetypeView<'a, V>>,
}

impl<'a, V> Query<'a, V>
where
    V: View<'a>,
{
    pub fn archetypes_iter(self) -> WorldAccessArchetypeIter<'a, V>
    where
        V: View<'a>,
    {
        WorldAccessArchetypeIter {
            iter: self.archetypes.into_iter(),
        }
    }

    pub fn chunks_iter(self) -> WorldAccessChunkIter<'a, V>
    where
        V: View<'a>,
    {
        WorldAccessChunkIter {
            current: None,
            iter: self.archetypes_iter(),
        }
    }

    pub fn entities_iter(self) -> WorldAccessEntityIter<'a, V>
    where
        V: View<'a>,
    {
        WorldAccessEntityIter {
            current: None,
            iter: self.chunks_iter(),
        }
    }
}

/// Iterator over `ArchetypeView`s for the `View`
pub struct WorldAccessArchetypeIter<'a, V: View<'a>> {
    iter: <BVec<'a, ArchetypeView<'a, V>> as IntoIterator>::IntoIter,
}

impl<'a, V> Iterator for WorldAccessArchetypeIter<'a, V>
where
    V: View<'a>,
{
    type Item = ArchetypeView<'a, V>;

    fn next(&mut self) -> Option<ArchetypeView<'a, V>> {
        self.iter.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    fn last(self) -> Option<ArchetypeView<'a, V>> {
        self.iter.last()
    }

    fn nth(&mut self, n: usize) -> Option<ArchetypeView<'a, V>> {
        self.iter.nth(n)
    }
}

/// Iterator over `ChunkView`s for the `View`
pub struct WorldAccessChunkIter<'a, V: View<'a>> {
    current: Option<ArchetypeView<'a, V>>,
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

/// Iterator over `EntityView`s for the `View`
pub struct WorldAccessEntityIter<'a, V: View<'a>> {
    current: Option<ChunkView<'a, V>>,
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
