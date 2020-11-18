//!
//! alex crate.
//!

#![no_std]

extern crate alloc;

mod archetype;
mod bundle;
mod component;
mod entity;
mod query;
mod util;
mod world;

pub use self::{
    archetype::{Archetype, UninitComponents},
    bundle::Bundle,
    entity::Entity,
    query::{read, write, Access, AccessComponent, AccessKind, Read, Write},
    world::World,
};
