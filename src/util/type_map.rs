use {
    crate::util::{NoOpHasher, XorHasher},
    alloc::boxed::Box,
    core::{any::TypeId, hash::BuildHasherDefault},
    hashbrown::HashMap,
};

/// HashMap that utilize the fact that `TypeId`s are already hashed.
pub type TypeIdMap<T> = HashMap<TypeId, T, BuildHasherDefault<NoOpHasher>>;

/// HashMap that utilize the fact that `TypeId`s are already hashed.
pub type TypeIdListMap<T> = HashMap<Box<[TypeId]>, T, BuildHasherDefault<XorHasher>>;
