use soroban_sdk::{contracttype, Address, Env, Symbol, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlashRecord {
    pub identity: Address,
    pub slash_amount: i128,
    pub reason: Symbol,
    pub timestamp: u64,
    pub total_slashed_after: i128,
}

// Use a proper contracttype enum for storage keys
#[contracttype]
#[derive(Clone)]
pub enum SlashStorageKey {
    SlashCount(Address),
    SlashRecord(Address, u32),
}

pub fn append_slash_history(
    e: &Env,
    identity: &Address,
    slash_amount: i128,
    reason: Symbol,
    total_slashed_after: i128,
) {
    let count_key = SlashStorageKey::SlashCount(identity.clone());

    let mut count: u32 = e
        .storage()
        .persistent()
        .get(&count_key)
        .unwrap_or(0);

    let record = SlashRecord {
        identity: identity.clone(),
        slash_amount,
        reason,
        timestamp: e.ledger().timestamp(),
        total_slashed_after,
    };

    let history_key = SlashStorageKey::SlashRecord(identity.clone(), count);
    e.storage().persistent().set(&history_key, &record);

    count += 1;
    e.storage().persistent().set(&count_key, &count);
}

#[must_use]
pub fn get_slash_count(e: &Env, identity: &Address) -> u32 {
    let key = SlashStorageKey::SlashCount(identity.clone());
    e.storage().persistent().get(&key).unwrap_or(0)
}

#[must_use]
pub fn get_slash_history(e: &Env, identity: &Address) -> Vec<SlashRecord> {
    let count = get_slash_count(e, identity);
    let mut history = Vec::new(e);

    for i in 0..count {
        let key = SlashStorageKey::SlashRecord(identity.clone(), i);
        if let Some(record) = e.storage().persistent().get(&key) {
            history.push_back(record);
        }
    }

    history
}

#[must_use]
pub fn get_slash_record(e: &Env, identity: &Address, index: u32) -> SlashRecord {
    let key = SlashStorageKey::SlashRecord(identity.clone(), index);
    e.storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic!("slash record not found"))
}

#[must_use]
pub fn get_total_slashed_from_history(e: &Env, identity: &Address) -> i128 {
    let history = get_slash_history(e, identity);
    let mut total: i128 = 0;
    for record in history.iter() {
        total += record.slash_amount;
    }
    total
}
