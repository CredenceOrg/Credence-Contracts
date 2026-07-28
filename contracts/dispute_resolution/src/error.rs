use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DisputeError {
    DisputeNotFound = 1,
    AlreadyClosed = 2,
    Unauthorized = 3,
}