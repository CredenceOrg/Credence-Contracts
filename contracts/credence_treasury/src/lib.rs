#![no_std]
#![allow(
    deprecated,
    unused_imports,
    unused_variables,
    dead_code,
    unused_assignments,
    unused_mut,
    mismatched_lifetime_syntaxes,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::restriction
)]

pub mod pausable;
pub mod receiver;
pub mod treasury;

pub use treasury::*;

#[cfg(test)]
mod test_treasury;

#[cfg(test)]
mod test_pausable;

// Flash loan tests are currently incomplete
// #[cfg(test)]
// mod test_flash_loan;

#[cfg(test)]
mod test_withdrawal_guardrails;

#[cfg(test)]
mod test_slippage_adversarial;

#[cfg(test)]
mod test_proportional_deduction;

#[cfg(test)]
mod test_pause_withdrawal_lifecycle;

#[cfg(test)]
mod test_per_source_reconciliation;
