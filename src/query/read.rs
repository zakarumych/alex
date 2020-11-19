use {
    super::{
        access::{ArchetypeAccess, ArchetypeRef},
        view::{ChunkRef, View},
    },
    core::{any::type_name, marker::PhantomData},
};

pub struct Read<T> {
    marker: PhantomData<fn() -> T>,
}

pub fn read<T>() -> Read<T> {
    Read {
        marker: PhantomData,
    }
}

impl<'a, T: 'static> View<'a> for Read<T> {
    type EntityView = &'a T;
    type ChunkRefs = ChunkRef<'a, T>;
    type ArchetypeRefs = ArchetypeRef<'a, T>;

    fn acquire(&self, archetype: ArchetypeAccess<'a>) -> ArchetypeRef<'a, T> {
        match archetype.borrow_ref() {
            Some(access) => access,
            None => panic!(
                "Archetype missing components of type `{}`",
                type_name::<T>(),
            ),
        }
    }
}
