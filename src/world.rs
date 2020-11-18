use {
    crate::{
        archetype::{Archetype, ArchetypeError, ArchetypeStorage},
        bundle::Bundle,
        component::ComponentInfo,
        entity::{Entity, EntityLocations, Location},
        util::{TypeIdListMap, TypeIdMap},
    },
    alloc::{boxed::Box, vec::Vec},
    core::any::TypeId,
    hashbrown::hash_map::RawEntryMut,
};

struct ArchetypeData {
    storage: ArchetypeStorage,
    with: TypeIdListMap<usize>,
}

impl ArchetypeData {
    fn new(components: Box<[ComponentInfo]>) -> Result<Self, ArchetypeError> {
        let archetype = Archetype::new(components)?;
        let storage = ArchetypeStorage::new(archetype);

        Ok(ArchetypeData {
            storage,
            with: TypeIdListMap::default(),
        })
    }
}

/// Error occuring when referenced entity does not exist.
/// It may be either already despawned or even never spawned.
#[derive(Clone, Copy, Debug)]
pub struct NoSuchEntity;

/// World is container for entities.
pub struct World {
    archetypes: Vec<ArchetypeData>,
    archetype_map: TypeIdListMap<usize>,
    entities: EntityLocations,
}

impl World {
    /// Create new empty `World`.
    pub fn new() -> Self {
        World {
            archetypes: Vec::new(),
            archetype_map: TypeIdListMap::default(),
            entities: EntityLocations::new(),
        }
    }

    /// Spawn new entity with components from `Bundle`.
    pub fn spawn(&mut self, bundle: impl Bundle + 'static) -> Entity {
        let archetype =
            bundle.with_ids(
                |ids| match self.archetype_map.raw_entry_mut().from_key(ids) {
                    RawEntryMut::Occupied(entry) => *entry.get(),
                    RawEntryMut::Vacant(entry) => {
                        let archetypes = &mut self.archetypes;
                        bundle.with_components(move |components| {
                            let archetype =
                                ArchetypeData::new(components.into()).expect("Too large bundle");
                            archetypes.push(archetype);

                            let (_, v) = entry.insert(ids.into(), archetypes.len() - 1);
                            *v
                        })
                    }
                },
            );

        let entity = self.entities.spawn_mut();

        let index = self.archetypes[archetype]
            .storage
            .insert(bundle, entity.index());

        self.entities
            .relocate(entity, Location { archetype, index });

        entity
    }

    /// Returns component of specified entity.
    pub fn get_ref<T: 'static>(&self, entity: Entity) -> Result<Option<&T>, NoSuchEntity> {
        let location = self.entities.locate(entity).ok_or(NoSuchEntity)?;

        if location.archetype == usize::MAX {
            Ok(None)
        } else {
            let storage = &self.archetypes[location.archetype].storage;
            Ok(storage.get_component_ref(entity.index()))
        }
    }

    /// Returns component of specified entity.
    pub fn get_mut<T: 'static>(&mut self, entity: Entity) -> Result<Option<&mut T>, NoSuchEntity> {
        let location = self.entities.locate(entity).ok_or(NoSuchEntity)?;

        if location.archetype == usize::MAX {
            Ok(None)
        } else {
            let storage = &mut self.archetypes[location.archetype].storage;
            Ok(storage.get_component_mut(entity.index()))
        }
    }

    /// Despawn an entity dropping all its commponents.
    pub fn despawn(&self, entity: Entity) -> Result<(), NoSuchEntity> {
        if self.entities.despawn(entity) {
            Ok(())
        } else {
            Err(NoSuchEntity)
        }
    }
}
