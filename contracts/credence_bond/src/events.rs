use soroban_sdk::{Address, Env, Symbol};

/// Emitted when a new bond is created.
///
/// # Topics
/// * `Symbol` - "bond_created"
/// * `Address` - The identity owning the bond
///
/// # Data
/// * `i128` - The initial bonded amount
/// * `u64` - The duration of the bond in seconds
/// * `bool` - Whether the bond is rolling
pub fn emit_bond_created(
    e: &Env,
    identity: &Address,
    amount: i128,
    duration: u64,
    is_rolling: bool,
) {
    let topics = (Symbol::new(e, "bond_created"), identity.clone());
    let data = (amount, duration, is_rolling);
    e.events().publish(topics, data);
}

/// Emitted when an existing bond is increased (topped up).
///
/// # Topics
/// * `Symbol` - "bond_increased"
/// * `Address` - The identity owning the bond
///
/// # Data
/// * `i128` - The additional amount added
/// * `i128` - The new total bonded amount
pub fn emit_bond_increased(e: &Env, identity: &Address, added_amount: i128, new_total: i128) {
    let topics = (Symbol::new(e, "bond_increased"), identity.clone());
    let data = (added_amount, new_total);
    e.events().publish(topics, data);
}

/// Emitted when funds are successfully withdrawn from a bond.
///
/// # Topics
/// * `Symbol` - "bond_withdrawn"
/// * `Address` - The identity owning the bond
///
/// # Data
/// * `i128` - The amount withdrawn
/// * `i128` - The remaining bonded amount
pub fn emit_bond_withdrawn(e: &Env, identity: &Address, amount_withdrawn: i128, remaining: i128) {
    let topics = (Symbol::new(e, "bond_withdrawn"), identity.clone());
    let data = (amount_withdrawn, remaining);
    e.events().publish(topics, data);
}

/// Emitted when a bond is slashed by an admin.
///
/// # Topics
/// * `Symbol` - "bond_slashed"
/// * `Address` - The identity owning the bond
///
/// # Data
/// * `i128` - The amount slashed in this event
/// * `i128` - The new total slashed amount for this bond
pub fn emit_bond_slashed(e: &Env, identity: &Address, slash_amount: i128, total_slashed: i128) {
    let topics = (Symbol::new(e, "bond_slashed"), identity.clone());
    let data = (slash_amount, total_slashed);
    e.events().publish(topics, data);
}
