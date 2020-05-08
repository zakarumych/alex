//! Alex - A lite entity component system.
//!
//! Alex is aimed to be flexible, data-driven and fast.
//!
//! For better CPU cores utilization systems may be executed in parallel on a thread-pool.
//! Parallel execution requires scheduling to ward off data races.
//! Alex schedules systems execution based on requested access types
//! to particular archetypes which are not tied to views the systems will fetch.
//! Instead views borrows data from granted access types to ensure that borrow is valid.
//! This allows parallel execution of systems that mutably borrow same component in disjoing sets of archetypes.
//! And providing ability to compose `View`s inside systems arbitrary within granted access types.
//!
//! Scheduled system execution is mostly deterministic.
//! Conflicting systems are always executed in the same oreder as they were added.
//! If component uses interior mutability (like `Mutex`) then two or more systems may modify component
//! while borrowing it immutably and thus they can executed in any order or even in parallel.

/// Macro to invoke another macro for sequences of identifiers.
macro_rules! for_sequences {
    ($action:ident) => {
        for_sequences!([POP $action] [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P]);
        // for_sequences!([POP $action] [A]);
    };

    ([POP $action:ident] []) => {
        for_sequences!([$action] []);
    };

    ([POP $action:ident] [$head:ident $(,$tail:ident)*]) => {
        for_sequences!([$action] [$head $(,$tail)*]);
        for_sequences!([POP $action] [$($tail),*]);
    };

    ([$action:ident] [$($a:ident),*]) => {
        $action!($($a),*);
    };
}

mod access;
mod and;
mod archetype;
mod component;
mod entity;
mod filter;
mod generation;
mod or;
mod read;
mod schedule;
mod util;
mod view;
mod world;
mod write;

pub use self::{
    access::*, and::*, archetype::*, component::*, entity::*, filter::*, generation::*, or::*,
    read::*, schedule::*, util::*, view::*, world::*, write::*,
};

pub use bumpalo;
