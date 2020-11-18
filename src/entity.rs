use {
    crate::util::{Generation, Queue, SyncPop, SyncPush},
    alloc::vec::Vec,
    core::{
        convert::TryFrom as _,
        sync::atomic::{AtomicI64, AtomicUsize, Ordering::*},
    },
    spin::Mutex,
};

/// Entity handle value.
/// Most operations concerning an entity use this handle
/// an entity in the `World`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Entity {
    index: usize,
    gen: Generation,
}

impl Entity {
    pub fn index(&self) -> usize {
        self.index
    }
}

/// Entity location indices.
#[derive(Clone, Copy)]
pub struct Location {
    pub archetype: usize,
    pub index: usize,
}

impl Location {
    const EMPTY: Self = Location {
        archetype: usize::MAX,
        index: 0,
    };
}

pub struct TooManyEntities;

struct Entry {
    location: Location,
    gen: Generation,
}

/// Maps entity to location.
pub struct EntityLocations {
    entries: Vec<Entry>,
    ready_counter: AtomicI64,
    ready_entries: Vec<usize>,

    drop: Queue<usize, SyncPush>,
    drop_slow: Mutex<Vec<usize>>,
}

impl Default for EntityLocations {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityLocations {
    pub fn new() -> Self {
        Self::with_capacity(8)
    }

    pub fn with_capacity(drop_cap: usize) -> Self {
        EntityLocations {
            entries: Vec::new(),
            ready_entries: Vec::new(),
            ready_counter: AtomicI64::new(0),

            drop: Queue::with_capacity(drop_cap),
            drop_slow: Mutex::new(Vec::new()),
        }
    }

    pub fn spawn(&self) -> Result<Entity, TooManyEntities> {
        let ready_counter = self.ready_counter.fetch_sub(1, Acquire);

        if ready_counter > 0 {
            debug_assert!(
                usize::try_from(ready_counter).is_ok(),
                "Never stores value greater than `ready_counter.len()`"
            );
            let index = self.ready_entries[ready_counter as usize - 1];
            Ok(Entity {
                index,
                gen: self.entries[index].gen,
            })
        } else if ready_counter < underflow_treshold() {
            Err(TooManyEntities)
        } else {
            let index = (-ready_counter) as usize;
            Ok(Entity {
                index,
                gen: Generation::new(),
            })
        }
    }

    pub fn spawn_mut(&mut self) -> Entity {
        self.flush_spawns();

        let ready_counter = *self.ready_counter.get_mut();

        if ready_counter > 0 {
            *self.ready_counter.get_mut() -= 1;

            debug_assert!(
                usize::try_from(ready_counter).is_ok(),
                "Never stores value greater than `ready_counter.len()`"
            );
            let index = self.ready_entries[ready_counter as usize];
            Entity {
                index,
                gen: self.entries[index].gen,
            }
        } else {
            let gen = Generation::new();
            self.entries.push(Entry {
                location: Location::EMPTY,
                gen,
            });
            Entity {
                index: self.entries.len() - 1,
                gen,
            }
        }
    }

    /// Returns location of an entity.
    pub fn locate(&self, entity: Entity) -> Option<Location> {
        match self.entries.get(entity.index) {
            Some(entry) if entity.gen == entry.gen => Some(entry.location),
            None if entity.gen.is_initial() => Some(Location::EMPTY),
            _ => None,
        }
    }

    /// Changes location of an entity.
    pub fn relocate(&mut self, entity: Entity, location: Location) {
        self.flush_spawns();

        let entry = &mut self.entries[entity.index];
        assert_eq!(entry.gen, entity.gen);
        entry.location = location;
    }

    pub fn despawn(&self, entity: Entity) -> bool {
        if entity.gen == self.get_generation(entity.index) {
            // Schedule entity dropping.
            if self.drop.sync_push(entity.index).is_err() {
                self.drop_slow.lock().push(entity.index);
            }
            true
        } else {
            false
        }
    }

    pub fn despawn_mut(&mut self, entity: Entity) -> bool {
        if entity.gen == self.get_generation(entity.index) {
            // Schedule entity dropping.
            self.drop.push(entity.index);
            true
        } else {
            false
        }
    }

    /// Must be called before any mutable operation.
    pub fn flush_spawns(&mut self) {
        let counter = *self.ready_counter.get_mut();

        if counter >= 0 {
            debug_assert!(usize::try_from(counter).is_ok());
            self.ready_entries.truncate(counter as usize);
        } else {
            self.ready_entries.clear();
            *self.ready_counter.get_mut() = 0;
            debug_assert!(usize::try_from(-counter).is_ok());
            let excess = (-counter) as usize;

            self.entries.extend((0..excess).map(|i| Entry {
                location: Location::EMPTY,
                gen: Generation::new(),
            }));
        }
    }

    /// Must be called after each systems dispatch.
    pub fn flush(&mut self, mut drop_fn: impl FnMut(Location)) {
        self.flush_spawns();

        let drop = &mut self.drop;

        let slow_drop_len = self.drop_slow.get_mut().len();

        let todrop = self
            .drop_slow
            .get_mut()
            .drain(..)
            .chain(core::iter::from_fn(|| drop.pop()));

        for index in todrop {
            drop_fn(self.entries[index].location);
            self.entries[index].gen.inc();

            self.ready_entries.push(index);
            let count = i64::try_from(self.ready_entries.len()).unwrap_or(i64::MAX);
            *self.ready_counter.get_mut() = count;
        }

        self.drop.reserve(slow_drop_len);
    }

    fn get_generation(&self, index: usize) -> Generation {
        self.entries.get(index).map_or(Generation::new(), |e| e.gen)
    }
}

fn underflow_treshold() -> i64 {
    i64::try_from(isize::MIN).unwrap_or(i64::MIN)
}

fn saturating_cast(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}
