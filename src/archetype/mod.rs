mod iterator;

pub use self::iterator::*;

use {
    crate::{
        access::Access,
        component::{Component, ComponentId, ComponentInfo},
        entity::Entity,
        util::{U32Size, CACHE_LINE_SIZE_HINT},
    },
    smallvec::SmallVec,
    std::{
        alloc::{alloc, handle_alloc_error, Layout},
        convert::TryFrom as _,
        mem::{align_of, size_of},
        ptr::NonNull,
    },
};

fn chunk_upper_limit() -> usize {
    std::option_env!("ALEX_CHUNK_UPPER_LIMIT")
        .and_then(|s| s.parse().ok())
        .unwrap_or(65536)
}

fn chunk_lower_limit() -> usize {
    std::option_env!("ALEX_CHUNK_LOWER_LIMIT")
        .and_then(|s| s.parse().ok())
        .unwrap_or(512)
}

/// Location of entity's components in storage.
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct Location {
    /// ArchetypeInfo storage index.
    pub archetype: U32Size,

    /// Index in the archetype.
    pub entity: U32Size,
}

#[derive(Clone, Copy, Debug)]
struct ComponentData {
    info: ComponentInfo,
    offset: usize,
}

/// Defines set components and layout in the data chunks for all components and entity indices.
#[derive(Debug)]
pub struct ArchetypeInfo {
    /// Layout of chunks for this archetype.
    chunk_layout: Layout,

    /// Maximum number of entities fit in one chunk.
    chunk_capacity: U32Size,

    /// Information about each component in the archetype.
    /// Sorted by type_id.
    components: Vec<ComponentData>,
}

impl ArchetypeInfo {
    /// Create archetype info for specified set of components.
    ///
    /// # Panics
    ///
    /// This function panics if array of components is not sorted.
    fn new(components: &[ComponentInfo]) -> Self {
        debug_assert_eq!(
            {
                let mut copy = components.to_vec();
                copy.sort();
                copy
            },
            components,
            "components must be sorted",
        );

        let chunk_lower_limit = chunk_lower_limit();
        let chunk_upper_limit = chunk_upper_limit();

        // Find total entity size.
        let entity_size: usize = components
            .iter()
            .map(|info| info.layout().size())
            .chain(Some(size_of::<Entity>()))
            .sum();

        assert!(
            entity_size <= chunk_upper_limit,
            "Too many large components. Consider boxing them or storing elsewhere",
        );

        // Chunk alignment ensures that all components are properly aligned.
        let min_align = components
            .iter()
            .map(|info| info.layout().align() - 1)
            .chain(Some(align_of::<Entity>() - 1))
            .fold(0, std::ops::BitOr::bitor)
            + 1;

        debug_assert!(min_align.is_power_of_two());
        assert!(u32::try_from(min_align).is_ok());

        // Hint for chunk size and alingment.
        // Expected to be equal to cache line size,
        // but at least 1,
        // but not greater than CACHE_LINE_SIZE_HINT,
        // but not less than minimal alignment requirements
        // and is power of two.
        let chunk_hint = CACHE_LINE_SIZE_HINT
            .max(1)
            .min(chunk_upper_limit)
            .max(min_align)
            .next_power_of_two();

        // Calculate greatest common divisor for chunk hint and component sizes.
        // Chunk hint is power of two thus to find gcd it is possible to simply count trailing zeros and find
        // smallest one.
        let size_gcd = 1usize
            << components
                .iter()
                .map(|info| info.layout().size().trailing_zeros())
                .chain(Some(size_of::<Entity>().trailing_zeros()))
                .chain(Some(chunk_hint.trailing_zeros()))
                .min()
                .unwrap();

        debug_assert!(size_gcd.is_power_of_two());

        // Capacity * <any component size> should be multiple of chunk hints.
        let chunk_capacity_hint = (chunk_hint / size_gcd).max(1);
        let chunk_capacity_min = chunk_lower_limit.next_power_of_two();

        let chunk_capacity = std::cmp::max(chunk_capacity_min, chunk_capacity_hint);

        // Ensure chunk size fits `usize`.
        let chunk_size = chunk_capacity
            .checked_mul(entity_size)
            .expect("Chunk size overflows. Try to specify smaller ALEX_CHUNK_UPPER_LIMIT environment variable");

        let chunk_layout = Layout::from_size_align(chunk_size, chunk_hint).expect(
            "Layout overflows. Try to specify smaller ALEX_CHUNK_UPPER_LIMIT environment variable",
        );

        let mut offset = size_of::<usize>();
        ArchetypeInfo {
            chunk_layout,
            components: components
                .iter()
                .copied()
                .map(|info| {
                    debug_assert_eq!(
                        offset % info.layout().align(),
                        0,
                        "Offset must be properly aligned",
                    );
                    offset += info.layout().size();
                    ComponentData {
                        offset: chunk_capacity * (offset - info.layout().size()),
                        info,
                    }
                })
                .collect(),
            chunk_capacity: U32Size::try_from(chunk_capacity).unwrap(),
        }
    }

    /// Checks if archetype matches specifeid components set exactly.
    /// `components` iterator SHOULD yield components in ascending order.
    pub fn is(&self, mut components: impl Iterator<Item = ComponentId>) -> bool {
        self.components
            .iter()
            .all(|c| Some(c.info.id()) == components.next())
            && components.count() == 0
    }

    /// Returns true if archetype has specified component.
    /// Returns false otherwise.
    pub fn has(&self, component: ComponentId) -> bool {
        self.components
            .iter()
            .position(|c| c.info.id() == component)
            .is_some()
    }

    /// Returns offset of the component in archetype's layout.
    /// Returns `None` if archetype doesn't have specified component.
    pub fn component_offset(&self, component: ComponentId) -> Option<usize> {
        self.components
            .iter()
            .find(|c| c.info.id() == component)
            .map(|c| c.offset)
    }

    pub(crate) fn split_entity_index(&self, index: U32Size) -> (U32Size, U32Size) {
        let chunk = index.as_u32() / self.chunk_capacity.as_u32();
        let entity = index.as_u32() % self.chunk_capacity.as_u32();

        (
            U32Size::try_from(chunk).unwrap(),
            U32Size::try_from(entity).unwrap(),
        )
    }
}

/// Archetype stores all entities with same set of components.
/// They are created on demand when first entity with particular components set is built.
#[cfg_attr(debug_assertions, derive(Debug))]
pub(crate) struct Archetype {
    info: ArchetypeInfo,

    /// Array of chunks storing entities of this archetype.
    chunks: Vec<NonNull<u8>>,

    /// Number of entities in this archetype.
    len: U32Size,
}

impl Archetype {
    /// Create archetype for specifeid set of components.
    ///
    /// # Panics
    ///
    /// This function panics if array of components is not sorted.
    pub fn new(components: &[ComponentInfo]) -> Self {
        let info = ArchetypeInfo::new(components);
        Archetype {
            info,
            chunks: Vec::new(),
            len: U32Size::zero(),
        }
    }

    /// Returns archetype info.
    pub fn info(&self) -> &ArchetypeInfo {
        &self.info
    }

    /// Returns an iterator over sizes of chunks in the archetype.
    pub fn chunk_sizes(&self) -> ChunkSizes {
        ChunkSizes::new(self.info.chunk_capacity.into(), self.len.into())
    }

    /// Returns an iterator over ptr-size pairs of chunks in the archetype.
    pub fn chunks(&self) -> Chunks<'_> {
        Chunks::new(
            &self.chunks,
            self.info.chunk_capacity.into(),
            self.len.into(),
        )
    }

    /// Returns iterator of bytes slices to components at specifeid offsets.
    ///
    /// # Safety
    ///
    /// Requested write access must not overlap with reads and writes to the same component.
    pub unsafe fn access_component(
        &self,
        access: Access,
        component: ComponentId,
        offset: usize,
    ) -> RawArchetypeComponentIter<'_> {
        debug_assert_eq!(offset, self.info.component_offset(component).unwrap());

        RawArchetypeComponentIter::new(component, access, offset, self.chunks())
    }

    /// Returns iterator of immutable slices to components at specifeid offsets.
    ///
    /// # Safety
    ///
    /// Requested write access must not overlap with reads and writes to the same component.
    pub unsafe fn read_component<T: Component>(
        &self,
        offset: usize,
    ) -> ArchetypeComponentIter<'_, T> {
        debug_assert_eq!(
            offset,
            self.info.component_offset(T::component_id()).unwrap()
        );

        ArchetypeComponentIter::new(offset, self.chunks())
    }

    /// Returns iterator of mutable slices to components at specifeid offsets.
    ///
    /// # Safety
    ///
    /// Requested write access must not overlap with reads and writes to the same component.
    pub unsafe fn write_component<T: Component>(
        &self,
        offset: usize,
    ) -> ArchetypeComponentIterMut<'_, T> {
        debug_assert_eq!(
            offset,
            self.info.component_offset(T::component_id()).unwrap()
        );

        ArchetypeComponentIterMut::new(offset, self.chunks())
    }

    /// Inserts components from set to the new entity in archetype.
    /// Returns location of the entity.
    ///
    /// # Panics
    ///
    /// This function will panic if `set` does not exactly match archetype's set of components.
    pub fn insert<'a, S>(
        &'a mut self,
        mut spawn_entity: impl FnMut(U32Size) -> Entity + 'a,
        sets: impl Iterator<Item = S> + 'a,
    ) -> impl Iterator<Item = Entity> + 'a
    where
        S: ComponentSet,
    {
        let components = S::components();
        let components = components.as_ref();
        assert_eq!(
            self.info.components.len(),
            components.len(),
            "Wrong archetype picked",
        );

        let offsets = components
            .iter()
            .map(|c| {
                (
                    c.id(),
                    self.info
                        .component_offset(c.id())
                        .expect("Component missing"),
                )
            })
            .collect::<SmallVec<[_; 16]>>();

        sets.map(move |set| {
            let newlen = match self.len.checked_inc() {
                Some(len) => len,
                None => handle_alloc_error(Layout::new::<Entity>()),
            };

            if self.len.as_usize() == self.chunks.len() * self.info.chunk_capacity.as_usize() {
                self.alloc_chunk();
            }

            let chunk_index = self.len.as_usize() / self.info.chunk_capacity.as_usize();
            let entity_index = self.len.as_usize() % self.info.chunk_capacity.as_usize();

            let ptr = self.chunks[usize::from(chunk_index)];
            let mut inserter = EntityInserter {
                base: ptr.as_ptr(),
                index: entity_index,
                offsets: offsets.iter(),
            };

            set.insert(&mut inserter);
            let entity = spawn_entity(self.len);
            inserter.finish(entity.index);

            self.len = newlen;

            entity
        })
    }

    /// Allocates new chunk.
    fn alloc_chunk(&mut self) {
        debug_assert_eq!(
            self.len.as_usize(),
            self.chunks.len() * self.info.chunk_capacity.as_usize(),
            "New chunk must be allocated only when all existing chunks are exhausted"
        );

        if self.chunks.len() == u32::max_value() as usize {
            handle_alloc_error(self.info.chunk_layout);
        }
        let ptr = if self.info.chunk_layout.size() > 0 {
            unsafe {
                // Layout is not zero-sized.
                alloc(self.info.chunk_layout)
            }
        } else {
            self.info.chunk_layout.align() as *mut u8
        };
        if let Some(ptr) = NonNull::new(ptr) {
            self.chunks.push(ptr);
        } else {
            handle_alloc_error(self.info.chunk_layout)
        }
    }
}

/// Archetype is not `Send` only due to `NonNull` which
/// can contain only `Send + Sync` values.
unsafe impl Send for Archetype {}

/// Archetype is not `Sync` only due to `NonNull` which
/// can contain only `Send + Sync` values.
unsafe impl Sync for Archetype {}

/// Object that is used to insert all components of the entity one by one.
#[derive(Debug)]
pub struct EntityInserter<'a> {
    base: *mut u8,
    index: usize,
    offsets: std::slice::Iter<'a, (ComponentId, usize)>,
}

impl<'a> EntityInserter<'a> {
    /// Write next component.
    ///
    /// # Panics
    ///
    /// This function panics if wrong component is written.
    /// All components must be written in the asceding order of `ComponentId`s.
    pub fn write<T: Component>(&mut self, value: T) {
        if let Some((id, offset)) = self.offsets.next() {
            if *id == T::component_id() {
                unsafe {
                    std::ptr::write(self.base.add(*offset).cast::<T>().add(self.index), value);
                }
            } else {
                panic!("Invalid components writing order");
            }
        }
    }

    pub fn finish(self, entity: usize) {
        assert_eq!(self.offsets.count(), 0);
        unsafe {
            *self.base.cast::<usize>().add(self.index) = entity;
        }
    }
}

/// Trait that is implemented for instances of component sets.
/// For example tuples of up to 32 component types.
/// This trait is used for inserting new instances with set of components.
pub trait ComponentSet: Send + Sync + 'static {
    type Components: AsRef<[ComponentInfo]>;

    /// Array of component infos.
    fn components() -> Self::Components;

    /// Consumes components set and inserts it into chunk.
    fn insert(self, inserter: &mut EntityInserter<'_>);
}

macro_rules! count_indices {
    () => { 0 };
    ($head:ident $(, $tail:ident)*) => { 1 + count_indices!($($tail),*) };
}

macro_rules! tuple_sets {
    ($($a:ident),*) => {
        impl<$($a),*> ComponentSet for ($($a,)*)
        where
            $($a: Component,)*
        {
            type Components = [ComponentInfo; count_indices!($($a),*)];

            fn components() -> Self::Components {
                [$(ComponentInfo::of::<$a>()),*]
            }

            fn insert(self, inserter: &mut EntityInserter<'_>) {
                #![allow(bad_style)]
                #![allow(unused_variables)]
                let ($($a,)*) = self;
                $(
                    inserter.write($a);
                )*
            }
        }
    };
}

for_sequences!(tuple_sets);
