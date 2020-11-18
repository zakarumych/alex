use {
    crate::component::ComponentInfo,
    alloc::boxed::Box,
    core::{alloc::Layout, any::TypeId, cmp::min, mem::size_of, ptr::NonNull},
};

const MINIMAL_CHUNK_SIZE: usize = 4096;
const CHUNK_SIZE_UPPER_BOUND: usize = MINIMAL_CHUNK_SIZE * 2;

#[repr(transparent)]
pub struct EntityIndex(pub usize);

pub struct Component {
    pub id: TypeId,
    pub offset: usize,
    pub size: usize,
    pub name: &'static str,
    pub drop_in_place: unsafe fn(NonNull<u8>),
}

pub struct Archetype {
    components: Box<[Component]>,
    entity_align: usize,
    entity_size: usize,
    chunk_capacity: usize,
    chunk_layout: Layout,
}

#[derive(Clone, Copy, Debug)]
pub enum ArchetypeError {
    EntityIsTooLarge,
}

use ArchetypeError::*;

impl Archetype {
    /// Returns `Archetype` instance for specified components.
    /// If chunk layout cannot be instantiated - returns `LayoutErr`.
    pub fn new(mut components: Box<[ComponentInfo]>) -> Result<Self, ArchetypeError> {
        components.iter().try_fold(0usize, |acc, c| {
            acc.checked_add(c.layout().size()).ok_or(EntityIsTooLarge)
        })?;

        components.sort_unstable_by_key(|c| (!0 - c.layout().align(), c.id()));

        let entity_align = components.get(0).map_or(1, |c| c.layout().align());

        let mut acc = size_of::<EntityIndex>();

        let mut components = components
            .iter()
            .map(|c| {
                debug_assert_eq!(acc % c.layout().align(), 0, "Sorting ensures that");
                acc += c.layout().size();

                Component {
                    id: c.id(),
                    offset: acc - c.layout().size(),
                    size: c.layout().size(),
                    name: c.name(),
                    drop_in_place: c.drop_in_place(),
                }
            })
            .collect::<Box<[_]>>();

        let entity_size = acc;

        if entity_size > isize::MAX as usize {
            return Err(EntityIsTooLarge);
        }

        let mut chunk_capacity = usize::MAX;

        if entity_size != 0 {
            chunk_capacity = (MINIMAL_CHUNK_SIZE - 1) / entity_size + 1;
            for c in &mut *components {
                c.offset *= chunk_capacity;
            }
        }

        let chunk_layout = Layout::from_size_align(chunk_capacity * entity_size, entity_align)
            .map_err(|_| EntityIsTooLarge)?;

        debug_assert!(chunk_layout.size() < min(CHUNK_SIZE_UPPER_BOUND, entity_size));

        Ok(Archetype {
            components,
            entity_size,
            entity_align,
            chunk_capacity,
            chunk_layout,
        })
    }

    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    pub fn components(&self) -> &[Component] {
        &self.components
    }

    pub fn ids(&self) -> impl Iterator<Item = TypeId> + Clone + '_ {
        self.components.iter().map(|c| c.id)
    }

    pub fn names(&self) -> impl Iterator<Item = &'static str> + Clone + '_ {
        self.components.iter().map(|c| c.name)
    }

    pub fn chunk_capacity(&self) -> usize {
        self.chunk_capacity
    }

    pub fn chunk_layout(&self) -> Layout {
        self.chunk_layout
    }
}
