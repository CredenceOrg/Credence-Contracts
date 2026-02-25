#![no_std]

pub mod pausable;
pub mod treasury;

pub use treasury::*;

#[cfg(test)]
mod test_treasury;

#[cfg(test)]
mod test_pausable;
