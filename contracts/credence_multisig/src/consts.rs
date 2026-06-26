/// Ledger TTL to extend instance storage to on every write (~1 year at 5 s/ledger).
pub const INSTANCE_TTL_EXTEND_TO: u32 = 31_536_000;

/// Threshold below which a TTL bump is triggered.
pub const INSTANCE_TTL_THRESHOLD: u32 = INSTANCE_TTL_EXTEND_TO / 2;
