#![no_std]
extern crate alloc;

pub mod addresses;
pub mod vec;

pub use addresses::{admin, attacker, user};
pub use vec::deduplicate_stable;
