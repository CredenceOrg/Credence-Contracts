#![no_std]

pub mod timelock;

pub use timelock::*;

#[cfg(test)]
mod test_timelock;
