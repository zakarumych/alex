use {
    super::{
        access::{AccessRef, ArchetypeAccess},
        view::View,
    },
    core::{any::TypeId, marker::PhantomData},
};

pub struct Read<T> {
    marker: PhantomData<fn() -> T>,
}

pub fn read<T>() -> Read<T> {
    Read {
        marker: PhantomData,
    }
}

impl<'a, T> View<'a> for Read<T> {
    type EntityView = &'a T;
    type AccessRefs = AccessRef<'a, T>;

    fn acquire(&self, archetype: ArchetypeAccess<'a>) -> AccessRef<'a, T> {
        archetype.borrow()
    }

    fn release(access: AccessRef<'a, T>, archetype: ArchetypeAccess<'a>) {
        archetype.release_ref(access);
    }
}
