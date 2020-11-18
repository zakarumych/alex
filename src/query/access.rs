use {
    crate::archetype::{Archetype, ArchetypeStorage},
    core::{any::TypeId, cell::Cell, marker::PhantomData, ptr::NonNull},
};

/// Kind of access.
/// Either shared or exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessKind {
    /// Access that can be granted to multiple
    /// requests in parallel.
    Shared,

    /// Access that can be granted to one requests at a time.
    Mutable,
}

/// Request access to a component.
pub struct AccessComponent {
    /// Id of the component.
    pub id: TypeId,

    /// Access kind requested.
    pub kind: AccessKind,
}

/// Declare components and access kind.
pub trait Access {
    /// Calls closure providing list of accesses required.
    /// Request may depend on archetype.
    fn with_accesses<T>(&self, archetype: &Archetype, f: impl FnOnce(&[AccessComponent]) -> T)
        -> T;
}

pub struct AccessRef<'a, T> {
    index: usize,
    offset: usize,
    marker: PhantomData<fn() -> &'a T>,
}

pub struct AccessRefMut<'a, T> {
    index: usize,
    offset: usize,
    marker: PhantomData<fn() -> &'a mut T>,
}

pub struct AccessDyn<'a> {
    index: usize,
    offset: usize,
    id: TypeId,
    kind: AccessKind,
    marker: PhantomData<fn() -> &'a mut ()>,
}

/// Accesses granted to an archetype.
#[derive(Clone, Copy)]
pub struct ArchetypeAccess<'a> {
    granted: &'a [Cell<usize>],
    storage: &'a ArchetypeStorage,
}

impl<'a> ArchetypeAccess<'a> {
    pub fn len(&self) -> usize {
        self.storage.len()
    }

    pub fn chunk_capacity(&self) -> usize {
        self.storage.chunk_capacity()
    }

    /// Borrow shared access to the components of type `T`.
    pub fn borrow_ref<T: 'static>(&self) -> Option<AccessRef<'a, T>> {
        let id = TypeId::of::<T>();
        let index = self.storage.component_index(id)?;
        let granted = unsafe { self.granted.get_unchecked(index) };
        match granted.get() {
            0 => None,
            left => {
                granted.set(left - 1);

                Some(AccessRef {
                    index,
                    offset: unsafe { self.storage.component_offset_by_index_unchecked(index) },
                    marker: PhantomData,
                })
            }
        }
    }

    /// Borrow shared access to the components of type `T`.
    pub fn borrow_mut<T: 'static>(&self) -> Option<AccessRefMut<'a, T>> {
        let id = TypeId::of::<T>();
        let index = self.storage.component_index(id)?;
        let granted = unsafe { self.granted.get_unchecked(index) };
        match granted.get() {
            usize::MAX => {
                granted.set(0);

                Some(AccessRefMut {
                    index,
                    offset: unsafe { self.storage.component_offset_by_index_unchecked(index) },
                    marker: PhantomData,
                })
            }
            _ => None,
        }
    }

    /// Borrow shared access to the components of type `T`.
    pub fn borrow_dyn(&self, id: TypeId, kind: AccessKind) -> Option<AccessDyn<'a>> {
        let index = self.storage.component_index(id)?;
        let granted = unsafe { self.granted.get_unchecked(index) };

        let offset = match (granted.get(), kind) {
            (usize::MAX, AccessKind::Exclusive) => {
                granted.set(0);
                unsafe { self.storage.component_offset_by_index_unchecked(index) }
            }
            0 => return None,
            (left, AccessKind::Shared) => {
                granted.set(left - 1);
                unsafe { self.storage.component_offset_by_index_unchecked(index) }
            }
            _ => return None,
        };

        Some(AccessDyn {
            index,
            offset,
            id,
            kind,
            marker: PhantomData,
        })
    }

    pub fn release_ref<T>(&self, access: AccessRef<'a, T>) {
        assert!(self.storage.is_correct_index_offset(
            TypeId::of::<T>(),
            access.index,
            access.offset
        ));

        let granted = &self.granted[access.index];
        granted.set(granted.get() + 1);
    }

    pub fn release_mut<T>(&self, access: AccessRefMut<'a, T>) {
        assert!(self.storage.is_correct_index_offset(
            TypeId::of::<T>(),
            access.index,
            access.offset
        ));

        let granted = &self.granted[access.index];
        debug_assert_eq!(granted.get(), 0);
        granted.set(usize::MAX);
    }

    pub fn release_dyn(&self, access: AccessDyn<'a>) {
        assert!(self
            .storage
            .is_correct_index_offset(access.id, access.index, access.offset));

        let granted = &self.granted[access.index];
        match access.kind {
            AccessKind::Shared => granted.set(granted.get() + 1),
            AccessKind::Exclusive => {
                debug_assert_eq!(granted.get(), 0);
                granted.set(usize::MAX);
            }
        }
    }
}
