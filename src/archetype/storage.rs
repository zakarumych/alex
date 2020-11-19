use {
    super::{Archetype, Component, EntityIndex},
    crate::{
        bundle::Bundle,
        util::{capacity_overflow, DisplayPunctuated as _},
    },
    alloc::{
        alloc::{alloc, handle_alloc_error},
        boxed::Box,
        vec::Vec,
    },
    core::{
        any::{type_name, TypeId},
        cell::Cell,
        mem::{forget, size_of},
        ptr::{write, NonNull},
    },
};

#[derive(Clone, Copy)]
struct Place {
    ptr: NonNull<u8>,
    init: bool,
}

impl Place {
    fn new() -> Self {
        Place {
            ptr: NonNull::dangling(),
            init: false,
        }
    }
}
pub struct ArchetypeStorage {
    archetype: Archetype,
    chunks: Vec<NonNull<u8>>,
    len: usize,
    places_cache: Box<[Place]>,
}

impl ArchetypeStorage {
    /// Returns storage for specified archetype.
    pub fn new(archetype: Archetype) -> Self {
        ArchetypeStorage {
            places_cache: alloc::vec![Place::new(); archetype.components().len()]
                .into_boxed_slice(), //TODO: switch to `Box::new_zeroed_slice()` when stable
            archetype,
            chunks: Vec::new(),
            len: 0,
        }
    }

    pub fn archetype(&self) -> &Archetype {
        &self.archetype
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn chunk_capacity(&self) -> usize {
        self.archetype.chunk_capacity()
    }

    pub fn capacity(&self) -> usize {
        self.chunks.len() * self.archetype.chunk_capacity()
    }

    pub fn insert<B>(&mut self, bundle: B, entity: usize) -> usize
    where
        B: Bundle + 'static,
    {
        #[cfg(debug_assertions)]
        {
            // Validate that correct archetype is chosen.
            if bundle.with_ids(|ids| ids.iter().copied().eq(self.archetype.ids())) {
                bundle.with_type_names(|names| {
                    panic!("Incorrect `Archetype` for `Bundle`.\n  Archetype components: [{}]\n  Souce components: [{}]", self.archetype.names().display_punctuated(), names.display_punctuated())
                })
            }
        }

        debug_assert!(self.capacity() >= self.len);
        if self.capacity() == self.len {
            self.alloc_chunk();
        }
        debug_assert!(self.capacity() > self.len);

        let chunk = self.len / self.archetype.chunk_capacity();
        let index = self.len % self.archetype.chunk_capacity();

        let chunk_ptr = self.chunks[chunk];

        for (p, c) in Iterator::zip(
            self.places_cache.iter_mut(),
            self.archetype.components().iter(),
        ) {
            let offset = c.offset + index * c.size;
            debug_assert!(offset <= self.archetype.chunk_layout().size());

            p.ptr = unsafe {
                // SAFETY: `chunk_ptr` points to the begining of chunk with layout `self.archetype.chunk_layout`.
                // Check above guarentees that offset is not out of bound of allocation, so adding it may not overflow pointer value.
                // `offset` may overflow only due to bug in this `Archetype` or this module.
                NonNull::new_unchecked(chunk_ptr.as_ptr().add(offset))
            };
        }

        // Prepare to share `places`.
        let places = Cell::from_mut(&mut *self.places_cache).as_slice_of_cells();

        let uninit = UninitComponents {
            components: self.archetype.components(),
            places,
        };

        let drop_initialized = DropInitialized {
            components: self.archetype.components(),
            places,
        };

        bundle.init_components(uninit);

        if places.iter().all(|p| p.get().init) {
            // All components are initialized.
            let offset = index * size_of::<EntityIndex>();
            debug_assert!(offset <= self.archetype.chunk_layout().size());

            let ptr = unsafe {
                // SAFETY: `chunk_ptr` points to the begining of chunk with layout `self.archetype.chunk_layout`.
                // Check above guarentees that offset is not out of bound of allocation, so adding it may not overflow pointer value.
                // `offset` may overflow only due to bug in this `Archetype` or this module.
                chunk_ptr.as_ptr().add(offset)
            };

            unsafe { write(ptr as *mut _, EntityIndex(entity)) }

            forget(drop_initialized);
            self.len += 1;
            self.len - 1
        } else {
            // Drop initialized components and panic.
            drop(drop_initialized);
            panic!(
                "Not all components were initialized by `<{} as Bundle>::init_components`",
                type_name::<B>(),
            )
        }
    }

    pub fn get_component_ref<T: 'static>(&self, index: usize) -> Option<&T> {
        let ptr = self.get_component_ptr(index)?;
        Some(unsafe { &*ptr.as_ptr() })
    }

    pub fn get_component_mut<T: 'static>(&mut self, index: usize) -> Option<&mut T> {
        let ptr = self.get_component_ptr(index)?;
        Some(unsafe { &mut *ptr.as_ptr() })
    }

    pub fn component_index(&self, id: TypeId) -> Option<usize> {
        self.archetype
            .components()
            .iter()
            .position(move |c| c.id == id)
    }

    pub unsafe fn component_offset_by_index_unchecked(&self, index: usize) -> usize {
        debug_assert!(self.archetype.components().len() > index);
        self.archetype.components().get_unchecked(index).offset
    }

    pub fn raw_chunks(&self) -> &[NonNull<u8>] {
        &self.chunks
    }

    #[cfg(debug_assertions)]
    pub fn is_correct_index_offset(&self, id: TypeId, index: usize, offset: usize) -> bool {
        let component = &self.archetype.components()[index];
        component.id == id && component.offset == offset
    }

    fn alloc_chunk(&mut self) {
        let chunk_size = self.archetype.chunk_layout().size();

        debug_assert!(chunk_size <= isize::MAX as usize);
        let chunk_size_i = chunk_size as isize;

        debug_assert!(self.chunks.len() <= isize::MAX as usize);
        let chunks_len_i = self.chunks.len() as isize;

        debug_assert_ne!(chunk_size, 0);
        debug_assert!(
            chunks_len_i.checked_mul(chunk_size_i).is_some(),
            "Combined chunks size cannot overflow `isize`"
        );

        let total_bytes = self.chunks.len() * chunk_size;
        debug_assert!(total_bytes <= isize::MAX as usize);
        let total_bytes_i = total_bytes as isize;

        if total_bytes_i.checked_add(chunk_size_i).is_none() {
            // Prevent bytes count to overflow `isize`.
            // Allocation would probably fail anyway on 64bit system.
            // But this is not `std` to make such bold assumptions.
            // So explicit panic is required.
            capacity_overflow();
        }

        let ptr = unsafe { alloc(self.archetype.chunk_layout()) };
        let ptr =
            NonNull::new(ptr).unwrap_or_else(|| handle_alloc_error(self.archetype.chunk_layout()));

        self.chunks.push(ptr);
    }

    fn get_component_ptr<T: 'static>(&self, index: usize) -> Option<NonNull<T>> {
        let id = TypeId::of::<T>();
        self.get_component_ptr_erased(id, index)
            .map(NonNull::cast::<T>)
    }

    fn get_component_ptr_erased(&self, id: TypeId, index: usize) -> Option<NonNull<u8>> {
        let component = self.component_index(id)?;
        let component = &self.archetype.components()[component];

        let chunk = index / self.archetype.chunk_capacity();
        let index = index % self.archetype.chunk_capacity();

        let offset = component.offset + index * component.size;
        debug_assert!(offset <= self.archetype.chunk_layout().size());

        let chunk_ptr = self.chunks.get(chunk)?;

        Some(unsafe { NonNull::new_unchecked(chunk_ptr.as_ptr().add(offset)) })
    }
}

/// Contains pointers to unitialized components.
/// User should call `UninitComponents::init_some` function
/// to initialized all components with correct type in arbitrary order.
///
/// `Bundle` implementation receive instance of this type.
/// All components must be initialized with `UninitComponents::init_some` function
/// in arbitrary order.
/// If `Bundle` leaves some components unitialized then all initialized components
/// will be dropped and components insertion will be aborted.
pub struct UninitComponents<'a> {
    components: &'a [Component],
    places: &'a [Cell<Place>],
}

struct DropInitialized<'a> {
    components: &'a [Component],
    places: &'a [Cell<Place>],
}

impl Drop for DropInitialized<'_> {
    fn drop(&mut self) {
        for (p, c) in Iterator::zip(self.places.iter(), self.components.iter()) {
            let p = p.get();
            if p.init {
                unsafe { (c.drop_in_place)(p.ptr) }
            }
        }
    }
}

impl UninitComponents<'_> {
    /// Initialize one of the component.
    /// Receiver of `UninitComponents` instance must call this function for all components.
    ///
    /// # Panics
    ///
    /// This instance must expect component of type `T`.
    /// Component of type `T` must not have been yet initialized.
    pub fn init_some<T: 'static>(&mut self, value: T) {
        let pos = match self
            .components
            .iter()
            .position(|c| c.id == TypeId::of::<T>())
        {
            None => panic!(
                "Failed to insert component of type `{}`. Expected one of `{}`",
                type_name::<T>(),
                self.components.iter().map(|c| c.name).display_punctuated(),
            ),
            Some(pos) => pos,
        };

        let mut place = self.places[pos].get();

        if place.init {
            panic!(
                "Failed to insert component of type `{}` twice",
                type_name::<T>()
            )
        } else {
            unsafe { write(place.ptr.as_ptr() as *mut T, value) };
            place.init = true;
            self.places[pos].set(place);
        }
    }
}
