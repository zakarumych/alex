//! Query execution scheduler.
//! Schedules queries in conflict-free fasion.
//! That is, no two conflicting query may be executed in parallel.
//! Two queries conflict if they access same components on same archetypes
//! and at least one of them do so mutably.
//!
//! Although conflict relation is non-transitive
//! scheduler will not schedule a query before conflicting queries that were added before it.
//!
//! That is if queries X and Y conflict then they will be executed in the same order that they were added to `Schedule`.
//!

use {
    crate::{
        access::{Access, Accessor, ComponentAccess, WorldAccess},
        archetype::ArchetypeInfo,
        world::World,
    },
    bumpalo::{collections::Vec as BVec, Bump},
};

#[cfg(feature = "rayon-parallel")]
mod parallel {
    use {
        super::*,
        crate::archetype::Archetype,
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
        runnable: UnsafeCell<Box<dyn AnyQuery>>,

        access_types: BVec<'a, BVec<'a, ComponentAccess>>,
    }

    impl<'a> Node<'a> {
        /// # Sefety
        ///
        /// Node access to filtered archetypes must be synchronzied.
        /// This function must be called at most once.
        unsafe fn execute(&self, bump: &Bump, archetypes: &'a [Archetype]) {
            (&mut *self.runnable.get()).run_once(WorldAccess::new(
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
                    node.execute(bumps.bump(), archetypes);
                }

                for signal in &node.signals {
                    debug_assert!(*signal > node_index);
                    signal_to_node(nodes, *signal, archetypes, scope, bumps);
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
        pub fn execute_rayon(self, pool: &ThreadPool, world: &mut World, bump: &Bump) {
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

            for query in self.queries {
                let next_node = nodes.len();
                let mut waits = 0;

                let access_types = archetypes.iter_mut().map(|archetype| {
                    let access_types = query.access_types(archetype.archetype.info(), bump);

                    // Try to add into last group.
                    if merge_access_types(&mut archetype.access_types, &access_types) {
                        archetype.nodes.push(next_node);
                    } else {
                        // Otherwise nodes in the last group signal to the new node.
                        for node in &archetype.nodes {
                            nodes[*node].signals.push(next_node);
                        }
                        // New node in turn waits for all of them.
                        waits += archetype.nodes.len();

                        archetype.nodes.clear();
                        archetype.nodes.push(next_node);
                    }

                    access_types
                });

                let access_types = BVec::from_iter_in(access_types, bump);

                nodes.push(Node {
                    waits: AtomicUsize::new(waits + 1), // One extra will be subtracted by execution loop.
                    signals: Vec::new(),
                    runnable: UnsafeCell::new(query),
                    access_types,
                });
            }

            log::trace!("Executing schedule on rayon thread pool");
            let nodes = &nodes[..];
            let archetypes = world.archetypes();
            let bumps = SyncBump::new(pool, bump);
            pool.scope(|scope| {
                for node_index in 0..nodes.len() {
                    signal_to_node(nodes, node_index, archetypes, scope, &bumps);
                }
            })
        }
    }
}

trait AnyQuery: Send + 'static {
    fn access_types<'a>(
        &self,
        archetype: &ArchetypeInfo,
        bump: &'a Bump,
    ) -> BVec<'a, ComponentAccess>;

    fn run_once(&mut self, world: WorldAccess<'_>);
}

impl<A, F> AnyQuery for Option<(A, F)>
where
    A: for<'a> Accessor<'a> + Send + 'static,
    F: for<'a> FnOnce(WorldAccess<'a>) + Send + 'static,
{
    fn access_types<'a>(
        &self,
        archetype: &ArchetypeInfo,
        bump: &'a Bump,
    ) -> BVec<'a, ComponentAccess> {
        sort_dedup_access_types(self.as_ref().unwrap().0.access_types(archetype), bump)
    }

    fn run_once(&mut self, world: WorldAccess<'_>) {
        let (_, f) = self.take().unwrap();
        f(world)
    }
}

pub struct Schedule {
    queries: Vec<Box<dyn AnyQuery>>,
}

impl Schedule {
    /// Build new schedule.
    pub fn new() -> Self {
        Schedule {
            queries: Vec::new(),
        }
    }

    /// Schedule the query for execution.
    ///
    /// Query filters archetypes and declares access types for the components.
    /// Which SHOULD be a union of access types performed by all views in the set.
    /// The query will be executed with `WorldAccess` synchronized for those access types.
    /// If queries would try to access unsynchronized components then acquired `WorldAccess` would panic to prevent UB.
    pub fn add_query<A>(
        &mut self,
        accessor: A,
        f: impl FnOnce(WorldAccess<'_>) + Send + 'static,
    ) -> &mut Self
    where
        A: for<'a> Accessor<'a> + Send + 'static,
    {
        self.queries.push(Box::new(Some((accessor, f))));
        self
    }

    /// Schedule the query for execution.
    /// See `add_query` for details.
    pub fn with_query<A>(
        mut self,
        accessor: A,
        f: impl FnOnce(WorldAccess<'_>) + Send + 'static,
    ) -> Self
    where
        A: for<'a> Accessor<'a> + Send + 'static,
    {
        self.queries.push(Box::new(Some((accessor, f))));
        self
    }

    /// Execute all nodes on this thread.
    pub fn execute(self, world: &mut World, bump: &Bump) {
        let archetypes = world.archetypes();
        for mut query in self.queries {
            let access_types = BVec::from_iter_in(
                archetypes
                    .iter()
                    .map(|a| query.access_types(a.info(), bump)),
                bump,
            );
            let world_access = unsafe {
                // Unique borrow of `World` gives all required access.
                WorldAccess::new(archetypes, &*access_types, bump)
            };
            query.run_once(world_access);
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
