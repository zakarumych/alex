use {
    crate::component::ComponentInfo,
    alloc::boxed::Box,
    core::{alloc::Layout, any::TypeId, mem::size_of, ptr::NonNull},
};

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

        components.sort_unstable();

        let entity_align = components
            .iter()
            .map(|c| c.layout().align())
            .max()
            .unwrap_or(1);

        let mut acc = size_of::<EntityIndex>();

        let mut components = components
            .iter()
            .map(|c| {
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

        let chunk_capacity = chunk_capacity(entity_size, entity_align).ok_or(EntityIsTooLarge)?;

        for c in &mut *components {
            c.offset *= chunk_capacity;
        }

        let chunk_layout = Layout::from_size_align(
            chunk_capacity
                .checked_mul(entity_size)
                .ok_or(EntityIsTooLarge)?,
            entity_align,
        )
        .map_err(|_| EntityIsTooLarge)?;

        Ok(Archetype {
            components,
            entity_size,
            entity_align,
            chunk_capacity,
            chunk_layout,
        })
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

fn chunk_capacity(entity_size: usize, entity_align: usize) -> Option<usize> {
    debug_assert!(entity_align.is_power_of_two());

    if entity_size == 0 {
        Some(usize::MAX & !(entity_align))
    } else {
        const BASE: usize = 4095;
        Some(((BASE / entity_size).checked_add(entity_align)?) & !(entity_align - 1))
    }
}
