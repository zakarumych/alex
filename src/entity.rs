use {
    crate::{
        generation::{Generation, GenerationCounter},
        util::{Shared, SyncPop, SyncPush},
    },
    std::{cell::UnsafeCell, fmt::Debug},
};

/// Location of entity's components in storage.
#[derive(Copy, Clone, Debug, Default)]
pub struct Location {
    /// ArchetypeInfo storage index.
    pub archetype: u32,

    /// Chunk index in archetype.
    pub chunk: u16,

    /// Index in chunk.
    pub entity: u16,
}

/// Entity index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entity {
    /// Generation id.
    pub generation: Generation,

    /// Index in entities array.
    pub index: usize,
}

/// Collection of all entities.
/// Entity contains nothing more than indices to its components.
pub struct Entities {
    /// Array of entity entries contains current generation for the index
    /// and indices to find entity's components.
    entries: Vec<Entry>,

    /// List of free entity indices.
    /// This list shoud be big enough for systems to spawn entities,
    /// refreshed each frame.
    /// Initial size is set on `Entities` creation.
    free: SyncPop<usize>,

    /// Shared growing array of entity entries.
    /// It is used only when free entries list is exhausted
    /// which should never happen,
    /// but may and we prefer worse performance to panicing.
    slow_entries: Shared<Vec<Entry>>,

    /// List of dropped entity indices.
    /// This list shoud be big enough for systems to drop entities,
    /// refreshed each frame.
    /// Initial size is set on `Entities` creation.
    drop: SyncPush<Entity>,
}

unsafe impl Send for Entities {}
unsafe impl Sync for Entities {}

impl Debug for Entities {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            fmt,
            "Entities {{ entries: [Entry; {}] }}",
            self.entries.len()
        )
    }
}

impl Entities {
    pub fn new(initial_free: usize, initial_drop: usize) -> Self {
        Entities {
            entries: std::iter::repeat_with(|| Entry::new())
                .take(initial_free)
                .collect(),
            free: SyncPop::from_iter(0..initial_free),
            slow_entries: Shared::new(Vec::new()),
            drop: SyncPush::new(initial_drop),
        }
    }

    /// Spawns new entity with specified location.
    /// FIXME: Multispawn.
    pub fn spawn(&self, location: Location) -> Entity {
        self.spawn_with(|_| location)
    }

    /// Spawns new entity with specified location.
    /// FIXME: Multispawn.
    pub fn spawn_with(&self, location: impl FnOnce(usize) -> Location) -> Entity {
        if let Some(index) = self.free.pop() {
            // free index left. Use it.
            let entry = &self.entries[index];
            unsafe {
                // Entry was free and now acquired by this `spawn` invocation.
                // Access through previously returned `Entity` with same index (if it was)
                // will be prevented by bumped `generation` id.
                *entry.location.get() = location(index);
            }

            Entity {
                generation: entry.generation.get(),
                index,
            }
        } else {
            // No free entries left.
            // Put into mutex.
            // Reserve more entries at next maintain.
            let mut lock = self.slow_entries.lock();
            let index = self.entries.len() + lock.len();
            lock.push(Entry::with_location(location(index)));
            drop(lock);

            Entity {
                generation: Generation::new(),
                index,
            }
        }
    }

    /// Spawns new entity with specified location.
    pub fn spawn_mut(&mut self, location: Location) -> Entity {
        self.spawn_with_mut(|_| location)
    }

    /// Spawns new entity with location returnd by function.
    pub fn spawn_with_mut(&mut self, location: impl FnOnce(usize) -> Location) -> Entity {
        if let Some(index) = self.free.pop_mut() {
            // free index left. Use it.
            let entry = &mut self.entries[index];
            unsafe {
                // Entry was free and now acquired by this `spawn` invocation.
                // Access through previously returned `Entity` with same index (if it was)
                // will be prevented by bumped `generation` id.
                *entry.location.get() = location(index);
            }

            Entity {
                generation: entry.generation.get(),
                index,
            }
        } else {
            let index = self.entries.len();
            let entry = Entry::with_location(location(index));
            let generation = entry.generation.get();
            self.entries.push(entry);

            Entity { generation, index }
        }
    }

    /// Drops entity if it is alive.
    pub fn drop(&self, entity: Entity) {
        // Push into drop list.
        self.drop.push(entity);
    }

    /// Perform periodic maintanance.
    pub fn maintenance(&mut self, mut drop_fn: impl FnMut(Location)) {
        let mut excess = self.slow_entries.get_mut().len();
        self.entries.append(self.slow_entries.get_mut());

        let dropped = self.drop.drain();
        let (dropped_lower, _) = dropped.size_hint();
        self.free.reserve(std::cmp::max(dropped_lower, excess));
        for entity in dropped {
            excess = excess.saturating_sub(1);
            let entry = &mut self.entries[entity.index];
            entry.generation.bump();
            drop_fn(unsafe {
                // Mutable access.
                *entry.location.get()
            });
            self.free.push(entity.index);
        }

        if excess > 0 {
            let base = self.entries.len();
            self.entries
                .extend(std::iter::repeat_with(|| Entry::new()).take(excess));
            self.free.extend(base..base + excess);
        }
    }

    pub fn get(&self, entity: &Entity) -> Option<Location> {
        let entry = self.entries.get(entity.index)?;
        if entry.generation.get() == entity.generation {
            unsafe { *entry.location.get() }.into()
        } else {
            assert!(entry.generation.get() > entity.generation);
            None
        }
    }

    pub fn get_mut(&mut self, entity: &Entity) -> Option<&mut Location> {
        let entry = self.entries.get(entity.index)?;
        if entry.generation.get() == entity.generation {
            unsafe { &mut *entry.location.get() }.into()
        } else {
            assert!(entry.generation.get() > entity.generation);
            None
        }
    }

    pub fn get_raw_mut(&mut self, index: usize) -> Option<&mut Location> {
        let entry = self.entries.get(index)?;
        unsafe { &mut *entry.location.get() }.into()
    }
}

struct Entry {
    generation: GenerationCounter,
    location: UnsafeCell<Location>,
}

impl Entry {
    fn new() -> Self {
        Entry {
            generation: GenerationCounter::new(),
            location: UnsafeCell::new(Default::default()),
        }
    }

    fn with_location(location: Location) -> Self {
        Entry {
            generation: GenerationCounter::new(),
            location: UnsafeCell::new(location),
        }
    }
}
