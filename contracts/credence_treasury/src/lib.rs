#![no_std]

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
