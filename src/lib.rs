//! Alex - A lite entity component
//!
//! This crate allows writing logic that spans multiple frames
//! as async functions (or anything that implements `Future` trait really).
//!

pub mod access;
pub mod archetype;
pub mod component;
pub mod entity;
pub mod filter;
pub mod generation;
// pub mod query;
pub mod schedule;
pub mod tuples;
pub mod view;
pub mod world;

mod util;

// pub use self::{
//     access::*, archetype::*, component::*, entity::*, filter::*, generation::*, query::*,
//     schedule::*, tuples::*, view::*, world::*,
// };
