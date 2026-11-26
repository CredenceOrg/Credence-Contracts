#![no_std]
extern crate alloc;

pub mod addresses;
pub mod cost_harness;
pub mod vec;

pub use addresses::{admin, attacker, distinct_pair, generate_many, user};
pub use vec::{deduplicate_stable, pair_iter};
