use {
    super::{
        access::{ArchetypeAccess, ArchetypeRefMut},
        view::{ChunkRefMut, View},
    },
    core::{any::type_name, marker::PhantomData},
};

pub struct Write<T> {
    marker: PhantomData<fn() -> T>,
}

pub fn write<T>() -> Write<T> {
    Write {
        marker: PhantomData,
    }
}

impl<'a, T: 'static> View<'a> for Write<T> {
    type EntityView = &'a mut T;
    type ChunkRefs = ChunkRefMut<'a, T>;
    type ArchetypeRefs = ArchetypeRefMut<'a, T>;

    fn acquire(&self, archetype: ArchetypeAccess<'a>) -> ArchetypeRefMut<'a, T> {
        match archetype.borrow_mut() {
            Some(access) => access,
            None => panic!(
                "Archetype missing components of type `{}`",
                type_name::<T>(),
            ),
        }
    }
}
