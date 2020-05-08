use {
    crate::{
        access::{Access, Accessor, ComponentAccess, WorldAccess},
        archetype::ArchetypeInfo,
        world::World,
    },
    bumpalo::{collections::Vec as BVec, Bump},
};

#[cfg(all(feature = "rayon", feature = "parallel"))]
mod parallel {
    use {
        super::*,
        crate::{archetype::Archetype, entity::Entities},
        rayon::ThreadPool,
        std::{
            cell::UnsafeCell,
            sync::atomic::{AtomicUsize, Ordering},
        },
    };

    struct ArchetypeNodeAccess<'a> {
        /// ArchetypeInfo description.
        archetype: &'a Archetype,

        /// Nodes in the last group.
        nodes: BVec<'a, usize>,

        /// Union of all access types in this group of `nodes`.
        access_types: BVec<'a, ComponentAccess>,
    }

    struct Node<'a> {
        /// Number of nodes this node waits for.
        waits: AtomicUsize,

        /// Node indices that are waiting for this one.
        signals: Vec<usize>,

        /// Closure to run for this node.
        runnable: UnsafeCell<&'a mut dyn AnySystem>,

        access_types: BVec<'a, BVec<'a, ComponentAccess>>,
    }

    impl<'a> Node<'a> {
        /// # Sefety
        ///
        /// Node access to filtered archetypes must be synchronzied.
        /// This function must be called at most once.
        unsafe fn execute(&self, entities: &Entities, archetypes: &[Archetype], bump: &Bump) {
            (&mut **self.runnable.get()).run(WorldAccess::new(
                entities,
                archetypes,
                &*self.access_types,
                bump,
            ));
        }
    }

    /// Is not Sync only because of `UnsafeCell` which is used in synchronized manner.
    unsafe impl<'a> Sync for Node<'a> {}

    /// Signal to node. Subtracts 1 from waiting counter.
    /// Nodes has initial waiting counter equal number of nodes they wait + 1
    /// This + 1 is signalled initially for all nodes when execution starts.
    /// This starts nodes with no dependencies and then they signal to nodes that wait for them.
    fn signal_to_node<'s>(
        nodes: &'s [Node],
        node_index: usize,
        entities: &'s Entities,
        archetypes: &'s [Archetype],
        scope: &rayon::Scope<'s>,
        bumps: &'s SyncBump<'_>,
    ) {
        let node = &nodes[node_index];

        let left = node.waits.fetch_sub(1, Ordering::AcqRel) - 1;
        debug_assert!(left < nodes.len(), "Waiting counter underflow");
        if left == 0 {
            scope.spawn(move |scope| {
                unsafe {
                    node.execute(entities, archetypes, bumps.bump());
                }

                for signal in &node.signals {
                    debug_assert!(*signal > node_index);
                    signal_to_node(nodes, *signal, entities, archetypes, scope, bumps);
                }
            })
        }
    }

    /// Returns union of two sets of access types unless their writes overlap.
    /// Otherwise returns `None`.
    fn merge_access_types(
        existing: &mut BVec<'_, ComponentAccess>,
        merge: &[ComponentAccess],
    ) -> bool {
        debug_assert_eq!(
            &*{
                let mut copy = merge.to_vec();
                copy.sort_by_key(|item| item.component);
                copy
            },
            merge
        );

        let mut offset = 0;
        for next in merge {
            if let Some(index) = existing[offset..]
                .iter()
                .position(|existing| existing.component >= next.component)
            {
                if existing[index + offset].component == next.component {
                    match (existing[index + offset].access, next.access) {
                        (Access::Read, Access::Read) => {
                            // merge.
                            offset += 1;
                        }
                        _ => {
                            existing.clear();
                            existing.extend(merge.iter().copied());
                            return false;
                        }
                    }
                } else {
                    existing.insert(offset + index, *next);
                    offset += index + 1;
                }
            } else {
                existing.push(*next);
                offset = existing.len();
            }
        }

        true
    }

    /// bumpalo::Bump adapter for rayon workers.
    struct SyncBump<'a> {
        bumps: BVec<'a, Bump>,
    }

    unsafe impl<'a> Sync for SyncBump<'a> {}

    impl<'a> SyncBump<'a> {
        fn new(pool: &rayon::ThreadPool, bump: &'a Bump) -> Self {
            SyncBump {
                bumps: BVec::from_iter_in(
                    std::iter::repeat_with(|| Bump::new()).take(pool.current_num_threads()),
                    bump,
                ),
            }
        }

        fn bump(&self) -> &Bump {
            &self.bumps[rayon::current_thread_index().unwrap()]
        }
    }

    impl Schedule {
        /// Execute all nodes on rayon thread pool.
        pub fn execute_rayon(&mut self, pool: &ThreadPool, world: &mut World, bump: &Bump) {
            use self::parallel::*;
            let mut archetypes = BVec::from_iter_in(
                world
                    .archetypes()
                    .iter()
                    .map(|archetype| ArchetypeNodeAccess {
                        archetype,
                        nodes: BVec::with_capacity_in(8, bump),
                        access_types: BVec::with_capacity_in(8, bump),
                    }),
                bump,
            );

            let mut nodes = BVec::<Node<'_>>::new_in(bump);

            for system in &mut self.systems {
                let next_node = nodes.len();
                let mut waits = BVec::new_in(bump);

                let access_types = archetypes.iter_mut().map(|archetype| {
                    let access_types = system.access_types(archetype.archetype.info(), bump);

                    // Try to add into last group.
                    if merge_access_types(&mut archetype.access_types, &access_types) {
                        archetype.nodes.push(next_node);
                    } else {
                        // New node in turn waits for all of them.
                        waits.extend(archetype.nodes.iter().copied());

                        archetype.nodes.clear();
                        archetype.nodes.push(next_node);
                    }

                    access_types
                });

                let access_types = BVec::from_iter_in(access_types, bump);

                waits.sort();
                waits.dedup();

                let waits = waits
                    .into_iter()
                    .map(|wait| {
                        nodes[wait].signals.push(next_node);
                    })
                    .count();

                nodes.push(Node {
                    waits: AtomicUsize::new(waits + 1), // One extra will be subtracted by execution loop.
                    signals: Vec::new(),
                    runnable: UnsafeCell::new(&mut **system),
                    access_types,
                });
            }

            log::trace!("Executing schedule on rayon thread pool");
            let nodes = &nodes[..];
            let archetypes = world.archetypes();
            let entities = world.entities();
            let bumps = SyncBump::new(pool, bump);
            pool.scope(|scope| {
                for node_index in 0..nodes.len() {
                    signal_to_node(nodes, node_index, entities, archetypes, scope, &bumps);
                }
            })
        }
    }
}

trait AnySystem: Send + 'static {
    fn access_types<'a>(
        &self,
        archetype: &ArchetypeInfo,
        bump: &'a Bump,
    ) -> BVec<'a, ComponentAccess>;

    fn run(&mut self, world: WorldAccess<'_>);
}

impl<A, F> AnySystem for (A, F)
where
    A: for<'a> Accessor<'a> + Send + 'static,
    F: for<'a> FnMut(WorldAccess<'a>) + Send + 'static,
{
    fn access_types<'a>(
        &self,
        archetype: &ArchetypeInfo,
        bump: &'a Bump,
    ) -> BVec<'a, ComponentAccess> {
        sort_dedup_access_types(self.0.access_types(archetype), bump)
    }

    fn run(&mut self, world: WorldAccess<'_>) {
        let (_, f) = self;
        f(world)
    }
}

/// System execution schedule.
///
/// Schedules systems in conflict-free fasion.
/// That is, no two conflicting system may be executed in parallel.
/// Two systems conflict if they access same components on same archetypes
/// and at least one of them do so mutably.
///
/// Scheduler will schedule a system after all conflicting systems that were added before.
pub struct Schedule {
    systems: Vec<Box<dyn AnySystem>>,
}

impl Schedule {
    /// Build new schedule.
    pub fn new() -> Self {
        Schedule {
            systems: Vec::new(),
        }
    }

    /// Schedule the system for execution.
    ///
    /// `accessor` declares access types for the components.
    /// Which SHOULD be a union of access types performed by all views in the system closure.
    /// The system will be executed with `WorldAccess` synchronized for those access types.
    /// If system would try to access unsynchronized components then acquired `WorldAccess` would panic.
    ///
    /// `f` is the system closure.
    pub fn add_system<A>(
        &mut self,
        accessor: A,
        f: impl FnMut(WorldAccess<'_>) + Send + 'static,
    ) -> &mut Self
    where
        A: for<'a> Accessor<'a> + Send + 'static,
    {
        self.systems.push(Box::new((accessor, f)));
        self
    }

    /// Schedule the system for execution.
    ///
    /// `accessor` declares access types for the components.
    /// Which SHOULD be a union of access types performed by all views in the system closure.
    /// The system will be executed with `WorldAccess` synchronized for those access types.
    /// If system would try to access unsynchronized components then acquired `WorldAccess` would panic.
    ///
    /// `f` is the system closure.
    pub fn with_system<A>(
        mut self,
        accessor: A,
        f: impl FnMut(WorldAccess<'_>) + Send + 'static,
    ) -> Self
    where
        A: for<'a> Accessor<'a> + Send + 'static,
    {
        self.systems.push(Box::new((accessor, f)));
        self
    }

    /// Execute all nodes on this thread.
    /// Doesn't require synchronization as all system are executed in sequence.
    pub fn execute(&mut self, world: &mut World, bump: &Bump) {
        let archetypes = world.archetypes();
        for system in &mut self.systems {
            let access_types = BVec::from_iter_in(
                archetypes
                    .iter()
                    .map(|a| system.access_types(a.info(), bump)),
                bump,
            );
            let world_access = unsafe {
                // Unique borrow of `World` gives all required access.
                WorldAccess::new(world.entities(), archetypes, &*access_types, bump)
            };
            system.run(world_access);
        }
    }
}

/// Sorts access types by components.
/// Dedups multiple reads and writes,
/// leaving single `Write` if the was at least one `Write`, otherwise leaves single `Read`.
fn sort_dedup_access_types<'a>(
    iter: impl Iterator<Item = ComponentAccess>,
    bump: &'a Bump,
) -> BVec<'a, ComponentAccess> {
    let access_ord = |access: Access| match access {
        Access::Read => 1,
        Access::Write => 0,
    };

    let mut result = BVec::from_iter_in(iter, bump);
    result.sort_by_key(|item| (item.component, access_ord(item.access))); // Ensure that `Write` comes first.

    let mut last = None;
    result.retain(|item| match &last {
        Some(ComponentAccess { component, .. }) if *component == item.component => false,
        _ => {
            last = Some(*item);
            true
        }
    });

    result
}
