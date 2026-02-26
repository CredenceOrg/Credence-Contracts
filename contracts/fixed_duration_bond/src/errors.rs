/// All panic messages used by the fixed_duration_bond contract.
///
/// Using string constants avoids typos in `#[should_panic(expected = "...")]` tests.
pub const ERR_ALREADY_INITIALIZED: &str = "already initialized";
pub const ERR_NOT_INITIALIZED: &str = "not initialized";
pub const ERR_UNAUTHORIZED: &str = "unauthorized";
pub const ERR_INVALID_AMOUNT: &str = "amount must be positive";
pub const ERR_INVALID_DURATION: &str = "duration must be positive";
pub const ERR_DURATION_OVERFLOW: &str = "bond expiry timestamp would overflow";
pub const ERR_BOND_ACTIVE: &str = "bond already active for this owner";
pub const ERR_NO_BOND: &str = "no active bond found";
pub const ERR_LOCK_PERIOD_NOT_ELAPSED: &str = "lock period has not elapsed yet";
pub const ERR_INSUFFICIENT_BALANCE: &str = "insufficient bond balance";
pub const ERR_TOKEN_NOT_SET: &str = "token not set";
pub const ERR_NO_FEES: &str = "no fees to collect";
pub const ERR_PENALTY_NOT_CONFIGURED: &str = "early-exit penalty not configured";
