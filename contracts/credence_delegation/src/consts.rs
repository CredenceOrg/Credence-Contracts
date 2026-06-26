/// Ledger TTL to extend instance storage to on every write (~1 year at 5 s/ledger).
pub const INSTANCE_TTL_EXTEND_TO: u32 = 31_536_000;

/// Threshold below which a TTL bump is triggered.
pub const INSTANCE_TTL_THRESHOLD: u32 = INSTANCE_TTL_EXTEND_TO / 2;

/// Safety buffer added on top of the delegation's `expires_at` TTL (~1 day at 5 s/ledger).
pub const LEDGER_BUMP_BUFFER: u32 = 17_280;

/// Minimum TTL for a Nonce entry regardless of delegation expiry (~30 days at 5 s/ledger).
pub const MIN_NONCE_TTL: u32 = 518_400;

/// Maximum persistent TTL allowed by the Soroban network (~6 months at 5 s/ledger).
pub const MAX_TTL: u32 = 3_110_400;
