use {
    crate::{
        access::Access,
        component::{Component, ComponentId, ComponentInfo},
        entity::Entity,
    },
    smallvec::SmallVec,
    std::{
        alloc::{alloc, handle_alloc_error, Layout},
        marker::PhantomData,
        mem::{align_of, size_of},
        ptr::NonNull,
    },
};

/// Number of entities one chunk can store.
const CHUNK_CAPACITY: u16 = 32_768;

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
            components
        );

        let chunk_align = components
            .iter()
            .map(|info| info.layout.align() - 1)
            .chain(Some(align_of::<usize>() - 1))
            .fold(0, |acc, align| acc | align)
            + 1;

        let total_size: usize = components
            .iter()
            .map(|info| info.layout.size())
            .chain(Some(size_of::<usize>()))
            .sum();

        let chunk_layout =
            Layout::from_size_align(total_size * usize::from(CHUNK_CAPACITY), chunk_align)
                .expect("Too many components");

        let mut offset = size_of::<usize>();
        ArchetypeInfo {
            chunk_layout,
            components: components
                .iter()
                .copied()
                .map(|info| {
                    debug_assert_eq!(
                        offset % info.layout.align(),
                        0,
                        "Offset must be properly aligned",
                    );
                    offset += info.layout.size();
                    ComponentData {
                        offset: usize::from(CHUNK_CAPACITY) * (offset - info.layout.size()),
                        info,
                    }
                })
                .collect(),
        }
    }

    /// Checks if archetype matches specifeid components set exactly.
    /// `components` iterator SHOULD yield components in ascending order.
    pub fn is(&self, mut components: impl Iterator<Item = ComponentId>) -> bool {
        self.components
            .iter()
            .all(|c| Some(c.info.id) == components.next())
            && components.count() == 0
    }

    /// Returns true if archetype has specified component.
    /// Returns false otherwise.
    pub fn has(&self, component: ComponentId) -> bool {
        self.components
            .iter()
            .position(|c| c.info.id == component)
            .is_some()
    }

    /// Returns offset of the component in archetype's layout.
    /// Returns `None` if archetype doesn't have specified component.
    pub fn component_offset(&self, component: ComponentId) -> Option<usize> {
        self.components
            .iter()
            .find(|c| c.info.id == component)
            .map(|c| c.offset)
    }
}

/// Chunk of components.
/// Layout is defined by owning `Archetype`.
struct Chunk {
    /// Raw components storage pointer.
    ptr: NonNull<u8>,
    /// Number of entities in this chunk.
    len: u16,
}

/// Chunk contains only types that implment `Component` which requires `Send` and `Sync`.
unsafe impl Send for Chunk {}
unsafe impl Sync for Chunk {}

impl Chunk {
    fn base_ptr(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }
}

/// Archetype stores all entities with same set of components.
/// They are created on demand when first entity with particular components set is built.
pub struct Archetype {
    info: ArchetypeInfo,

    /// Array of chunks storing entities of this archetype.
    chunks: Vec<Chunk>,

    /// Array of chunks with unused space.
    unexhausted: Vec<u16>,
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
            unexhausted: Vec::new(),
        }
    }

    /// Get archetype info.
    pub fn info(&self) -> &ArchetypeInfo {
        &self.info
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

        RawArchetypeComponentIter {
            component,
            access,
            offset,
            iter: self.chunks.iter(),
            marker: PhantomData,
        }
    }

    /// Returns iterator of bytes slices to components at specifeid offsets.
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

        ArchetypeComponentIter {
            offset,
            iter: self.chunks.iter(),
            marker: PhantomData,
        }
    }

    /// Returns iterator of bytes slices to components at specifeid offsets.
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

        ArchetypeComponentIterMut {
            offset,
            iter: self.chunks.iter(),
            marker: PhantomData,
        }
    }

    /// Inserts components from set to the new entity in archetype.
    /// Returns location of the entity.
    ///
    /// # Panics
    ///
    /// This function will panic if `set` does not exactly match archetype's set of components.
    pub fn insert<'a, S>(
        &'a mut self,
        mut spawn_entity: impl FnMut(u16, u16) -> Entity + 'a,
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
                    c.id,
                    self.info.component_offset(c.id).expect("Component missing"),
                )
            })
            .collect::<SmallVec<[_; 16]>>();

        sets.map(move |set| {
            let chunk_index = self.nonexhausted_chunk();
            let chunk = &mut self.chunks[usize::from(chunk_index)];
            let mut inserter = EntityInserter {
                base: chunk.base_ptr(),
                index: chunk.len,
                offsets: offsets.iter(),
            };
            set.insert(&mut inserter);
            let entity = spawn_entity(chunk_index, chunk.len);
            inserter.finish(entity.index);

            // Now entity is initialized.
            chunk.len += 1;

            if chunk.len == CHUNK_CAPACITY {
                debug_assert_eq!(self.unexhausted.last(), Some(&chunk_index));
                self.unexhausted.pop();
            }

            entity
        })
    }

    /// Returns chunk and entity indices that are not yet used.
    /// This position is not yet initialized and if entity insertion fails it can be reused.
    fn nonexhausted_chunk(&mut self) -> u16 {
        let index = match self.unexhausted.last() {
            Some(index) => *index,
            None => self.alloc_chunk(),
        };
        debug_assert!(self.chunks[usize::from(index)].len < CHUNK_CAPACITY);
        index
    }

    /// Allocates new chunk.
    fn alloc_chunk(&mut self) -> u16 {
        if self.chunks.len() == u16::max_value() as usize {
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
            let index = self.chunks.len() as u16;
            self.chunks.push(Chunk { ptr, len: 0 });
            self.unexhausted.push(index);
            index
        } else {
            handle_alloc_error(self.info.chunk_layout)
        }
    }
}

/// Slice of components.
pub struct ComponentSlice<'a> {
    component: ComponentId,
    access: Access,
    ptr: NonNull<u8>,
    len: u16,
    marker: PhantomData<&'a mut u8>,
}

impl<'a> ComponentSlice<'a> {
    /// Returns slice to component instances.
    pub fn read<T: Component>(self) -> &'a [T] {
        assert_eq!(T::component_id(), self.component);
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr() as *const T, self.len.into()) }
    }

    /// Returns slice to component instances.
    pub fn write<T: Component>(self) -> &'a mut [T] {
        assert_eq!(T::component_id(), self.component);
        assert_eq!(self.access, Access::Write);
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr() as *mut T, self.len.into()) }
    }

    pub fn raw(&self) -> NonNull<u8> {
        self.ptr
    }

    pub fn len(&self) -> u16 {
        self.len
    }
}

/// Iterator for component slices.
pub struct RawArchetypeComponentIter<'a> {
    component: ComponentId,
    access: Access,
    offset: usize,
    iter: std::slice::Iter<'a, Chunk>,
    marker: PhantomData<&'a mut u8>,
}

impl<'a> RawArchetypeComponentIter<'a> {
    fn item_from_chunk(&self, chunk: &'a Chunk) -> ComponentSlice<'a> {
        let ptr = unsafe {
            // Cannot overflow because offset is smaller then allocation that starts with `chunk.ptr`.
            NonNull::new_unchecked(chunk.ptr.as_ptr().add(self.offset))
        };
        ComponentSlice {
            component: self.component,
            access: self.access,
            ptr,
            len: chunk.len,
            marker: PhantomData,
        }
    }
}

impl<'a> Iterator for RawArchetypeComponentIter<'a> {
    type Item = ComponentSlice<'a>;

    fn next(&mut self) -> Option<ComponentSlice<'a>> {
        let chunk = self.iter.next()?;
        self.item_from_chunk(chunk).into()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    fn count(self) -> usize {
        self.iter.count()
    }

    fn last(self) -> Option<ComponentSlice<'a>> {
        let chunk = self.iter.last()?;
        let ptr = unsafe {
            // Cannot overflow because offset is smaller then allocation that starts with `chunk.ptr`.
            NonNull::new_unchecked(chunk.ptr.as_ptr().add(self.offset))
        };
        ComponentSlice {
            component: self.component,
            access: self.access,
            ptr,
            len: chunk.len,
            marker: PhantomData,
        }
        .into()
    }

    fn nth(&mut self, n: usize) -> Option<ComponentSlice<'a>> {
        let chunk = self.iter.nth(n)?;
        self.item_from_chunk(chunk).into()
    }
}

/// Iterator for component slices.
pub struct ArchetypeComponentIter<'a, T> {
    offset: usize,
    iter: std::slice::Iter<'a, Chunk>,
    marker: PhantomData<&'a T>,
}

impl<'a, T> ArchetypeComponentIter<'a, T> {
    fn item_from_chunk(&self, chunk: &'a Chunk) -> &'a [T] {
        unsafe {
            // Cannot overflow because offset is smaller then allocation that starts with `chunk.ptr`.
            let ptr = NonNull::new_unchecked(chunk.ptr.as_ptr().add(self.offset));
            std::slice::from_raw_parts(ptr.as_ptr() as *const T, chunk.len.into())
        }
    }
}

impl<'a, T: 'a> Iterator for ArchetypeComponentIter<'a, T> {
    type Item = &'a [T];

    fn next(&mut self) -> Option<&'a [T]> {
        let chunk = self.iter.next()?;
        self.item_from_chunk(chunk).into()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    fn count(self) -> usize {
        self.iter.count()
    }

    fn last(self) -> Option<&'a [T]> {
        let chunk = self.iter.last()?;
        unsafe {
            // Cannot overflow because offset is smaller then allocation that starts with `chunk.ptr`.
            let ptr = NonNull::new_unchecked(chunk.ptr.as_ptr().add(self.offset));
            std::slice::from_raw_parts(ptr.as_ptr() as *const T, chunk.len.into()).into()
        }
    }

    fn nth(&mut self, n: usize) -> Option<&'a [T]> {
        let chunk = self.iter.nth(n)?;
        self.item_from_chunk(chunk).into()
    }
}

/// Iterator for component slices.
pub struct ArchetypeComponentIterMut<'a, T> {
    offset: usize,
    iter: std::slice::Iter<'a, Chunk>,
    marker: PhantomData<&'a mut T>,
}

impl<'a, T> ArchetypeComponentIterMut<'a, T> {
    fn item_from_chunk(&self, chunk: &'a Chunk) -> &'a mut [T] {
        unsafe {
            // Cannot overflow because offset is smaller then allocation that starts with `chunk.ptr`.
            let ptr = NonNull::new_unchecked(chunk.ptr.as_ptr().add(self.offset));
            std::slice::from_raw_parts_mut(ptr.as_ptr() as *mut T, chunk.len.into())
        }
    }
}

impl<'a, T: 'a> Iterator for ArchetypeComponentIterMut<'a, T> {
    type Item = &'a mut [T];

    fn next(&mut self) -> Option<&'a mut [T]> {
        let chunk = self.iter.next()?;
        self.item_from_chunk(chunk).into()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }

    fn count(self) -> usize {
        self.iter.count()
    }

    fn last(self) -> Option<&'a mut [T]> {
        let chunk = self.iter.last()?;
        unsafe {
            // Cannot overflow because offset is smaller then allocation that starts with `chunk.ptr`.
            let ptr = NonNull::new_unchecked(chunk.ptr.as_ptr().add(self.offset));
            std::slice::from_raw_parts_mut(ptr.as_ptr() as *mut T, chunk.len.into())
        }
        .into()
    }

    fn nth(&mut self, n: usize) -> Option<&'a mut [T]> {
        let chunk = self.iter.nth(n)?;
        self.item_from_chunk(chunk).into()
    }
}

/// Object that is used to insert all components of the entity one by one.
#[derive(Debug)]
pub struct EntityInserter<'a> {
    base: *mut u8,
    index: u16,
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
                    std::ptr::write(
                        self.base.add(*offset).cast::<T>().add(self.index.into()),
                        value,
                    );
                }
            } else {
                panic!("Invalid components writing order");
            }
        }
    }

    pub fn finish(self, entity: usize) {
        assert_eq!(self.offsets.count(), 0);
        unsafe {
            *self.base.cast::<usize>().add(self.index.into()) = entity;
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

macro_rules! impl_set_for_tuple {
    ($($a:ident),* $(,)?) => {
        impl_set_for_tuple!(POP [$($a),*]);

        impl<$($a),*> ComponentSet for ($($a,)*)
        where
            $($a: Component,)*
        {
            type Components = [ComponentInfo; impl_set_for_tuple!(COUNT [$($a),*])];

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

    (POP [$head:ident $(,$tail:ident)* $(,)?]) => {
        impl_set_for_tuple!($($tail),*);
    };

    (POP [$(,)?]) => {};

    (COUNT [$head:ident $(,$tail:ident)* $(,)?]) => {
        impl_set_for_tuple!(COUNT [$($tail),*]) + 1
    };

    (COUNT [$(,)?]) => {
        0
    };
}

impl_set_for_tuple!(A, B, C, D, E, F, G, H);
