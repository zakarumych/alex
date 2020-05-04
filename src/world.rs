use {
    crate::{
        archetype::{Archetype, ComponentSet, Location},
        component::Component,
        entity::{Entities, Entity},
        util::U32Size,
    },
    std::{
        any::TypeId,
        collections::hash_map::{Entry, HashMap},
        convert::TryFrom,
    },
};

struct Archetypes {
    array: Vec<Archetype>,
    set_to_archetype: HashMap<TypeId, U32Size>,
}

impl Archetypes {
    fn archetype_for_set<S>(&mut self) -> U32Size
    where
        S: ComponentSet,
    {
        match self.set_to_archetype.entry(TypeId::of::<S>()) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let mut components = S::components().as_ref().to_vec();
                components.sort();

                let result = self
                    .array
                    .iter_mut()
                    .position(|archetype| archetype.info().is(components.iter().map(|c| c.id())));

                match result {
                    Some(archetype) => U32Size::try_from(archetype).unwrap(),
                    None => match U32Size::try_from(self.array.len()) {
                        Err(_) => panic!("Too many archetypes"),
                        Ok(len) => {
                            let archetype = Archetype::new(&components);
                            self.array.push(archetype);
                            entry.insert(len);
                            len
                        }
                    },
                }
            }
        }
    }

    fn get(&self, index: U32Size) -> &Archetype {
        &self.array[index.as_usize()]
    }

    fn get_mut(&mut self, index: U32Size) -> &mut Archetype {
        &mut self.array[index.as_usize()]
    }
}

/// Container for entities and their components.
pub struct World {
    entities: Entities,
    archetypes: Archetypes,
}

impl World {
    /// Returns newly created `World`.
    pub fn new() -> Self {
        World {
            entities: Entities::new(1024, 1024),
            archetypes: Archetypes {
                array: vec![Archetype::new(&[])],
                set_to_archetype: HashMap::new(),
            },
        }
    }

    /// For each set yield by iterator create an entity with all components from the set.
    /// Returns iterator of created entities.
    ///
    ///
    /// # Example
    ///
    /// ```
    /// # use alex::*;
    ///
    /// let mut world = World::new();
    ///
    /// #[derive(Debug, PartialEq, Eq)]
    /// struct Foo(u32);
    ///
    /// let entities = world.insert(std::iter::once((Foo(23),))).collect::<Vec<_>>();
    /// assert_eq!(entities.len(), 1);
    /// assert_eq!(world.get_component::<Foo>(&entities[0]), Some(&Foo(23)));
    /// ```
    ///
    pub fn insert<'a, S>(
        &'a mut self,
        sets: impl IntoIterator<Item = S> + 'a,
    ) -> impl Iterator<Item = Entity> + 'a
    where
        S: ComponentSet,
    {
        let archetype = self.archetypes.archetype_for_set::<S>();
        let entities = &mut self.entities;

        self.archetypes.get_mut(archetype).insert(
            move |entity| entities.spawn_mut(Location { archetype, entity }),
            sets.into_iter(),
        )
    }

    /// Returns immutable borrow for component of specified entity.
    /// Returns `None` if entities does not have component with required type
    /// or entitiy was destroyed
    pub fn get_component<T: Component>(&self, entity: &Entity) -> Option<&T> {
        let entry = self.entities.get(entity)?;
        let archetype = self.archetypes.get(entry.archetype);
        let offset = archetype.info().component_offset(T::component_id())?;

        let (chunk_index, entity_index) = archetype.info().split_entity_index(entry.entity);

        let mut chunks = unsafe {
            // Immutable reference to the `World` guarantees that
            // `get_component_mut` cannot be called
            // or schedule runned.
            // Which leaves only immutable access via `get_component`
            archetype.read_component::<T>(offset)
        };
        chunks
            .nth(chunk_index.as_usize())?
            .nth(entity_index.as_usize())
    }

    /// Returns mutable borrow for component of specified entity.
    /// Returns `None` if entities does not have component with required type
    /// or entitiy was destroyed
    pub fn get_component_mut<T: Component>(&mut self, entity: &Entity) -> Option<&mut T> {
        let entry = self.entities.get(entity)?;
        let archetype = self.archetypes.get(entry.archetype);
        let offset = archetype.info().component_offset(T::component_id())?;

        let (chunk_index, entity_index) = archetype.info().split_entity_index(entry.entity);

        let mut chunks = unsafe {
            // Mutable reference to the `World` guarantees that
            // no other access_types can be performed.
            archetype.write_component::<T>(offset)
        };
        chunks
            .nth(chunk_index.as_usize())?
            .nth(entity_index.as_usize())
    }

    /// Returns slice of archetypes.
    pub(crate) fn archetypes(&self) -> &[Archetype] {
        &self.archetypes.array
    }

    /// Borrows entities
    pub fn entities(&self) -> &Entities {
        &self.entities
    }
}
