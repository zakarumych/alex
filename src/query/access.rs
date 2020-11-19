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
#[derive(Clone, Copy)]
pub struct AccessComponent {
    /// Id of the component.
    pub id: TypeId,

    /// Access kind requested.
    pub kind: AccessKind,
}

pub trait AccessOne {
    fn access(&self, archetype: &Archetype) -> AccessComponent;
}

/// Declare components and access kind.
pub trait Access {
    /// Calls closure providing list of accesses required.
    /// Request may depend on archetype.
    fn with_accesses<T>(&self, archetype: &Archetype, f: impl FnOnce(&[AccessComponent]) -> T)
        -> T;
}

impl<A> Access for A
where
    A: AccessOne,
{
    fn with_accesses<T>(
        &self,
        archetype: &Archetype,
        f: impl FnOnce(&[AccessComponent]) -> T,
    ) -> T {
        f(core::slice::from_ref(&self.access(archetype)))
    }
}

pub struct ArchetypeRef<'a, T> {
    offset: usize,
    marker: PhantomData<fn() -> &'a T>,
    unlock: &'a Cell<usize>,
}

impl<'a, T> Drop for ArchetypeRef<'a, T> {
    fn drop(&mut self) {
        self.unlock.set(self.unlock.get() + 1);
    }
}

impl<'a, T> ArchetypeRef<'a, T> {
    pub unsafe fn get(&self, raw: NonNull<u8>) -> NonNull<T> {
        NonNull::new_unchecked(raw.as_ptr().add(self.offset) as *mut T)
    }
}

pub struct ArchetypeRefMut<'a, T> {
    offset: usize,
    marker: PhantomData<fn() -> &'a mut T>,
    unlock: &'a Cell<usize>,
}

impl<'a, T> Drop for ArchetypeRefMut<'a, T> {
    fn drop(&mut self) {
        debug_assert_eq!(self.unlock.get(), 0);
        self.unlock.set(usize::MAX);
    }
}

impl<'a, T> ArchetypeRefMut<'a, T> {
    pub unsafe fn get(&self, raw: NonNull<u8>) -> NonNull<T> {
        NonNull::new_unchecked(raw.as_ptr().add(self.offset) as *mut T)
    }
}

pub struct AccessDyn<'a> {
    offset: usize,
    id: TypeId,
    kind: AccessKind,
    unlock: &'a Cell<usize>,
}

impl<'a> Drop for AccessDyn<'a> {
    fn drop(&mut self) {
        match self.kind {
            AccessKind::Shared => self.unlock.set(self.unlock.get() + 1),
            AccessKind::Mutable => {
                debug_assert_eq!(self.unlock.get(), 0);
                self.unlock.set(usize::MAX);
            }
        }
    }
}

impl<'a> AccessDyn<'a> {
    pub unsafe fn get<T: 'static>(&self, raw: NonNull<u8>) -> NonNull<T> {
        debug_assert_eq!(self.id, TypeId::of::<T>());
        NonNull::new_unchecked(raw.as_ptr().add(self.offset) as *mut T)
    }
}

/// Accesses granted to an archetype.
#[derive(Clone, Copy)]
pub struct ArchetypeAccess<'a> {
    granted: &'a [Cell<usize>],
    storage: &'a ArchetypeStorage,
}

impl<'a> ArchetypeAccess<'a> {
    pub(crate) fn new(granted: &'a [Cell<usize>], storage: &'a ArchetypeStorage) -> Self {
        ArchetypeAccess { granted, storage }
    }

    pub fn len(&self) -> usize {
        self.storage.len()
    }

    pub fn chunk_capacity(&self) -> usize {
        self.storage.chunk_capacity()
    }

    /// Borrow shared access to the components of type `T`.
    pub fn borrow_ref<T: 'static>(&self) -> Option<ArchetypeRef<'a, T>> {
        let id = TypeId::of::<T>();
        let index = self.storage.component_index(id)?;
        debug_assert!(self.granted.len() > index);
        let granted = unsafe { self.granted.get_unchecked(index) };
        match granted.get() {
            0 => None,
            left => {
                granted.set(left - 1);

                Some(ArchetypeRef {
                    unlock: granted,
                    offset: unsafe { self.storage.component_offset_by_index_unchecked(index) },
                    marker: PhantomData,
                })
            }
        }
    }

    /// Borrow shared access to the components of type `T`.
    pub fn borrow_mut<T: 'static>(&self) -> Option<ArchetypeRefMut<'a, T>> {
        let id = TypeId::of::<T>();
        let index = self.storage.component_index(id)?;
        debug_assert!(self.granted.len() > index);
        let granted = unsafe { self.granted.get_unchecked(index) };
        match granted.get() {
            usize::MAX => {
                granted.set(0);

                Some(ArchetypeRefMut {
                    unlock: granted,
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
        debug_assert!(self.granted.len() > index);
        let granted = unsafe { self.granted.get_unchecked(index) };

        let offset = match (granted.get(), kind) {
            (usize::MAX, AccessKind::Mutable) => {
                granted.set(0);
                unsafe { self.storage.component_offset_by_index_unchecked(index) }
            }
            (0, _) => return None,
            (left, AccessKind::Shared) => {
                granted.set(left - 1);
                unsafe { self.storage.component_offset_by_index_unchecked(index) }
            }
            _ => return None,
        };

        Some(AccessDyn {
            unlock: granted,
            offset,
            id,
            kind,
        })
    }
}
