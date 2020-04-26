use {
    crate::{
        archetype::{Archetype, ComponentSet},
        component::Component,
        entity::{Entities, Entity, Location},
    },
    std::{
        any::TypeId,
        collections::hash_map::{Entry, HashMap},
        convert::TryFrom,
    },
};

struct Archetypes {
    array: Vec<Archetype>,
    set_to_archetype: HashMap<TypeId, u32>,
}

impl Archetypes {
    fn archetype_for_set<S>(&mut self) -> u32
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
                    .position(|archetype| archetype.info().is(components.iter().map(|c| c.id)));

                match result {
                    Some(archetype) => u32::try_from(archetype).unwrap(),
                    None => match u32::try_from(self.array.len()) {
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

    fn get(&self, index: u32) -> &Archetype {
        &self.array[usize::try_from(index).unwrap()]
    }

    fn get_mut(&mut self, index: u32) -> &mut Archetype {
        &mut self.array[usize::try_from(index).unwrap()]
    }
}

pub struct World {
    entities: Entities,
    archetypes: Archetypes,
}

impl World {
    pub fn new() -> Self {
        World {
            entities: Entities::new(1024, 1024),
            archetypes: Archetypes {
                array: vec![Archetype::new(&[])],
                set_to_archetype: HashMap::new(),
            },
        }
    }

    ///
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
            move |chunk, entity| {
                entities.spawn_mut(Location {
                    archetype,
                    chunk,
                    entity,
                })
            },
            sets.into_iter(),
        )
    }

    pub fn get_component<T: Component>(&self, entity: &Entity) -> Option<&T> {
        let entry = self.entities.get(entity)?;
        let archetype = self.archetypes.get(entry.archetype);
        let offset = archetype.info().component_offset(T::component_id())?;
        let mut chunks = unsafe {
            // Immutable reference to the `World` guarantees that
            // `get_component_mut` cannot be called
            // or schedule runned.
            // Which leaves only immutable access via `get_component`
            archetype.read_component::<T>(offset)
        };
        chunks
            .nth(entry.chunk.into())?
            .iter()
            .nth(entry.entity.into())
    }

    pub fn get_component_mut<T: Component>(&mut self, entity: &Entity) -> Option<&T> {
        let entry = self.entities.get(entity)?;
        let archetype = self.archetypes.get(entry.archetype);
        let offset = archetype.info().component_offset(T::component_id())?;
        let mut chunks = unsafe {
            // Mutable reference to the `World` guarantees that
            // no other access_types can be performed.
            archetype.write_component::<T>(offset)
        };
        chunks
            .nth(entry.chunk.into())?
            .iter()
            .nth(entry.entity.into())
    }

    pub fn archetypes(&self) -> &[Archetype] {
        &self.archetypes.array
    }
}
