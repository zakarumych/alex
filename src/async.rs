use {
    crate::{
        archetype::{Archetype, ArchetypeError, ArchetypeStorage},
        bundle::Bundle,
        component::ComponentInfo,
        entity::{Entity, EntityLocations, Location},
        query::{
            iter::ArchetypeEntityIter, Access, AccessComponent, AccessKind, ArchetypeAccess, View,
        },
        util::{unreachable_unchecked, MutableGuard, SharedGuard, TypeIdListMap, TypeIdMap},
        world::World,
    },
    alloc::{boxed::Box, vec::Vec},
    core::{
        any::TypeId,
        cell::Cell,
        cmp::{Ord, Ordering},
        future::Future,
        mem::replace,
        pin::Pin,
        task::{Context, Poll, Waker},
    },
    hashbrown::hash_map::RawEntryMut,
    spin::Mutex,
};

enum Guard<'a> {
    Shared(SharedGuard<'a>),
    Mutable(MutableGuard<'a>),
}

pub struct AsyncArchetypeAccess<'a> {
    guards: Vec<Guard<'a>>,
    granted: Vec<Cell<usize>>,
    storage: &'a ArchetypeStorage,
}

impl AsyncArchetypeAccess<'_> {
    pub fn get(&mut self) -> ArchetypeAccess<'_> {
        ArchetypeAccess::new(&mut self.granted, self.storage)
    }
}

pub struct AsyncWorldAccess<'a> {
    archetypes: Vec<AsyncArchetypeAccess<'a>>,
}

impl<'a> AsyncWorldAccess<'a> {
    pub fn iter_view<V: View<'a> + 'a>(
        &'a mut self,
        view: &'a V,
    ) -> impl Iterator<Item = <V as View<'a>>::EntityView> + 'a {
        self.archetypes.iter_mut().flat_map(move |archetype| {
            ArchetypeEntityIter {
                raw_chunks: archetype.storage.raw_chunks().iter(),
                len: archetype.storage.len(),
                chunk_capacity: archetype.storage.chunk_capacity(),
                refs: view.acquire(archetype.get()),
            }
            .flatten()
        })
    }
}

impl World {
    async fn lock(&self, access: impl Access) -> AsyncWorldAccess<'_> {
        if self.archetypes().len() == 0 {
            AsyncWorldAccess {
                archetypes: Vec::new(),
            }
        } else {
            let mut result: Vec<AsyncArchetypeAccess<'_>> = Vec::new();

            for archetype in self.archetypes() {
                let storage = archetype.storage();
                let archetype_components = storage.archetype().components();

                let components: Box<[AccessComponent]> =
                    access.with_accesses(storage.archetype(), |slice| slice.into());

                let has_all = (|| {
                    let mut archetype_components = archetype_components.iter();
                    for component in &*components {
                        for archetype_component in &mut archetype_components {
                            match Ord::cmp(&component.id, &archetype_component.id) {
                                Ordering::Equal => break,
                                Ordering::Less => return false, // Component not found.
                                Ordering::Greater => continue,
                            }
                        }
                    }
                    true
                })();

                if has_all {
                    let mut guards = Vec::new();
                    let mut granted = Vec::new();
                    let mut archetype_components = archetype_components.iter();
                    let mut locks = archetype.locks().iter();

                    for component in &*components {
                        for (archetype_component, lock) in
                            Iterator::zip(&mut archetype_components, &mut locks)
                        {
                            match Ord::cmp(&component.id, &archetype_component.id) {
                                Ordering::Equal => {
                                    match component.kind {
                                        AccessKind::Mutable => {
                                            guards.push(Guard::Mutable(lock.lock_mutable().await));
                                            granted.push(Cell::new(usize::MAX));
                                        }
                                        AccessKind::Shared => {
                                            guards.push(Guard::Shared(lock.lock_shared().await));
                                            granted.push(Cell::new(usize::MAX - 1));
                                        }
                                    }
                                    break;
                                }
                                Ordering::Less => unsafe { unreachable_unchecked() },
                                Ordering::Greater => continue,
                            }
                        }
                    }

                    result.push(AsyncArchetypeAccess {
                        guards,
                        granted,
                        storage,
                    })
                }
            }

            AsyncWorldAccess { archetypes: result }
        }
    }
}

// struct AsyncWorldLock<'a, A> {
//     world: &'a World,
//     archetype_checked: bool,
//     archetype_offset: usize,
//     component_offset: usize,
//     archetype_access_cache: Vec<Cell<usize>>,
//     world_access_cache: Vec<AsyncArchetypeAccess<'a>>,
//     access: A,
// }

// impl<A> Unpin for AsyncWorldLock<'_, A> {}

// impl<'a, A> Future for AsyncWorldLock<'a, A>
// where
//     A: Access,
// {
//     type Output = AsyncWorldAccess<'a>;
//     fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<AsyncWorldAccess<'a>> {
//         let me = self.get_mut();

//         loop {
//             debug_assert!(me.world.archetypes().len() > me.archetype_offset);
//             let archetype = unsafe { me.world.archetypes().get_unchecked(me.archetype_offset) };
//             let storage = archetype.storage();

//             debug_assert_eq!(
//                 storage.archetype().components().len(),
//                 archetype.accesses().len()
//             );

//             let archetype_checked = &mut me.archetype_checked;
//             let component_offset = &mut me.component_offset;
//             let archetype_access_cache = &mut me.archetype_access_cache;

//             let ready = me.access.with_accesses(storage.archetype(), |components| {
//                 if archetype.accesses().is_empty() || components.is_empty() {
//                     return Poll::Ready(Some({
//                         AsyncArchetypeAccess {
//                             granted: Vec::new(),
//                             storage,
//                         }
//                     }));
//                 }

//                 let archetype_components = storage.archetype().components();

//                 if !*archetype_checked {
//                     let mut archetype_components = archetype_components.iter();
//                     for component in components {
//                         for archetype_component in &mut archetype_components {
//                             match Ord::cmp(&component.id, &archetype_component.id) {
//                                 Ordering::Equal => break,
//                                 Ordering::Less => return Poll::Ready(None), // Component not found.
//                                 Ordering::Greater => continue,
//                             }
//                         }
//                     }
//                     *archetype_checked = true;
//                 }

//                 debug_assert!(components.len() >= archetype_access_cache.len());
//                 debug_assert!(archetype.accesses().len() >= *component_offset);

//                 loop {
//                     let component =
//                         unsafe { archetype_components.get_unchecked(*component_offset) };

//                     let access_component =
//                         unsafe { components.get_unchecked(archetype_access_cache.len()) };

//                     match Ord::cmp(&component.id, &access_component.id) {
//                         Ordering::Equal => {
//                             let access =
//                                 unsafe { archetype.accesses().get_unchecked(*component_offset) };
//                             let mut guard = access.lock();

//                             if !guard.borrow_dyn(ctx, access_component.kind) {
//                                 return Poll::Pending;
//                             }

//                             archetype_access_cache.push(Cell::new(match access_component.kind {
//                                 AccessKind::Shared => usize::MAX - 1,
//                                 AccessKind::Mutable => usize::MAX,
//                             }));

//                             if components.len() == archetype_access_cache.len() + 1 {
//                                 return Poll::Ready(Some(AsyncArchetypeAccess {
//                                     granted: replace(archetype_access_cache, Vec::new()),
//                                     storage: archetype.storage(),
//                                 }));
//                             }
//                             *component_offset += 1;
//                             debug_assert!(archetype.accesses().len() > *component_offset);
//                         }
//                         Ordering::Less => unreachable_unchecked(),
//                         Ordering::Greater => {
//                             *component_offset += 1;
//                             debug_assert!(archetype.accesses().len() > *component_offset);
//                         }
//                     }
//                 }
//             });

//             match ready {
//                 Poll::Pending => return Poll::Pending,
//                 Poll::Ready(archetype_access) => {
//                     if let Some(archetype_access) = archetype_access {
//                         me.world_access_cache.push(archetype_access);
//                     }

//                     if me.world.archetypes().len() == me.archetype_offset + 1 {
//                         return Poll::Ready(AsyncWorldAccess {
//                             archetypes: replace(&mut me.world_access_cache, Vec::new()),
//                         });
//                     }

//                     me.archetype_checked = false;
//                     me.archetype_offset += 1;
//                 }
//             }
//         }
//     }
// }
