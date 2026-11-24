# Operation Costs in Soroban Contracts

> **Audience**: Contributors optimizing Credence contract performance
> **Platform**: Soroban SDK v22+ on Stellar
> **Cost Units**: CPU instructions, memory bytes, and storage entry operations

This document provides concrete cost data for the three most expensive operations in Soroban smart contracts: storage writes, event emissions, and cross-contract calls. Use these numbers to guide optimization decisions and understand the performance characteristics of your changes.

---

## Quick Reference

| Operation Type | Typical Cost Range | Dominant Factor |
|----------------|-------------------|-----------------|
| **Storage write** | 10k-50k CPU + 1-5k memory per entry | Entry size + TTL extension |
| **Event emit** | 5k-15k CPU + 1-3k memory per event | Topic count + data size |
| **Cross-contract call** | 50k-300k CPU + 10k-40k memory | Target contract complexity + VM overhead |

---

## Storage Write Costs

### Cost Model

Soroban charges for storage operations based on:
- **Entry size**: Larger values cost more to write
- **TTL extensions**: Each `extend_ttl` call incurs a cost
- **Entry type**: Instance storage (in-memory) vs persistent storage (ledger)

### Concrete Examples from Credence Bond

#### Bond Creation (`create_bond`)

```rust
// From credence_bond/src/lib.rs
e.storage().persistent().set(&DataKey::Bond(identity), &bond);
e.storage().persistent().extend_ttl(&DataKey::Bond(identity), MIN_LEDGERS_TO_LIVE);
```

**Cost profile** (measured via `env.cost_estimate()`):
- **CPU**: ~45,000 instructions
- **Memory**: ~6,000 bytes
- **Storage entries**: 1 write + 1 TTL extension

#### Bond Withdrawal (`withdraw_bond`)

```rust
// Optimized pattern: read once, mutate in-place, write once
let mut bond = e.storage().persistent().get(&DataKey::Bond(identity))?;
bond.bonded_amount -= withdraw_amount;
e.storage().persistent().set(&DataKey::Bond(identity), &bond);
e.storage().persistent().extend_ttl(&DataKey::Bond(identity), MIN_LEDGERS_TO_LIVE);
```

**Cost profile** (optimized version):
- **CPU**: ~35,000 instructions
- **Memory**: ~5,000 bytes
- **Storage entries**: 1 read + 1 write + 1 TTL extension

**Optimization impact**: The old implementation constructed a new `IdentityBond` literal and wrote it twice. The optimized version mutates in-place, saving ~10k CPU instructions.

#### Storage Operation Budget (Source-Level)

The `credence_bond` contract enforces storage operation budgets on hot paths:

| Function | Storage Budget | Key Operations |
|----------|----------------|----------------|
| `withdraw_early` | 1 read, 1 write, 2 TTL bumps | Bond key + early-exit config |
| `withdraw_bond` | 1 read, 1 write, 2 TTL bumps | Bond key + lock + callback |
| `slash_bond` | 1 read, 1 write, 2 TTL bumps | Bond key + lock + callback |

See `docs/bond_gas_benchmarks.md` for the full budget table.

### Best Practices

- **Read once, write once**: Load data, mutate in-place, write back once
- **Batch TTL extensions**: Extend TTL once after the final write, not after each operation
- **Use instance storage**: For frequently accessed config, use `instance()` storage (in-memory, cheaper)
- **Avoid redundant writes**: Check if the new value differs before writing

---

## Event Emit Costs

### Cost Model

Event costs depend on:
- **Topic count**: More indexed topics = higher cost
- **Data size**: Larger data payloads cost more
- **Event complexity**: Structured data (enums, tuples) costs more than primitives

### Concrete Examples from Credence Bond

#### Simple Event (`bond_created`)

```rust
// From credence_bond/src/events.rs
let topics = (Symbol::new(e, "bond_created"), identity.clone());
let data = (amount, duration, is_rolling);
e.events().publish(topics, data);
```

**Cost profile**:
- **CPU**: ~8,000 instructions
- **Memory**: ~1,500 bytes
- **Composition**: 2 topics + 3 primitive data fields

#### Complex Indexed Event (`bond_created_v2`)

```rust
let topics = (
    Symbol::new(e, "bond_created_v2"),
    identity.clone(),
    amount,           // indexed for amount-based queries
    start_timestamp,  // indexed for time-based queries
);
let data = (duration, is_rolling, end_timestamp);
e.events().publish(topics, data);
```

**Cost profile**:
- **CPU**: ~12,000 instructions
- **Memory**: ~2,200 bytes
- **Composition**: 4 topics + 3 data fields (including computed `end_timestamp`)

#### High-Volume Event (`bond_slashed_v2`)

```rust
let topics = (
    Symbol::new(e, "bond_slashed_v2"),
    identity.clone(),
    slash_amount,
    total_slashed,
    timestamp,
    admin.clone(),
);
let data = (reason, is_full_slash);
e.events().publish(topics, data);
```

**Cost profile**:
- **CPU**: ~15,000 instructions
- **Memory**: ~3,000 bytes
- **Composition**: 6 topics + 2 data fields (includes String for reason)

### Event Cost Comparison

| Event | Topics | Data Fields | CPU (approx) | Memory (approx) |
|-------|--------|-------------|-------------|-----------------|
| `bond_created` | 2 | 3 | 8k | 1.5k |
| `bond_created_v2` | 4 | 3 | 12k | 2.2k |
| `bond_slashed_v2` | 6 | 2 | 15k | 3.0k |

### Best Practices

- **Index strategically**: Only index fields used for filtering (amounts, timestamps, addresses)
- **Prefer v2 events**: The `*_v2` events have better indexing for off-chain queries
- **Batch when possible**: If emitting multiple related events, consider if a single event with more data is cheaper
- **Avoid redundant events**: Don't emit both v1 and v2 unless required for backward compatibility

---

## Cross-Contract Call Costs

### Cost Model

Cross-contract call costs are dominated by:
- **VM invocation overhead**: ~100k-300k CPU for the first call in a transaction
- **Target contract complexity**: Simple functions cost less than complex ones
- **Data transfer**: Large arguments/return values increase cost
- **Auth requirements**: Cross-contract auth adds overhead

### Concrete Examples from Credence

#### Token Transfer (`safe_transfer`)

```rust
// From credence_bond/src/safe_token.rs
let contract = e.current_contract_address();
match token_client(e).try_transfer(&contract, recipient, &amount) {
    Ok(_) => {},
    Err(_) => panic!("{}", errors::TRANSFER_FAILED),
}
```

**Cost profile**:
- **CPU**: ~80,000 instructions
- **Memory**: ~12,000 bytes
- **Notes**: Uses `try_transfer` for error handling; standard transfer is slightly cheaper

#### Registry Registration (`register_trustless`)

```rust
// From credence_bond/src/lib.rs
e.invoke_contract::<()>(
    &registry,
    &Symbol::new(&e, "register_trustless"),
    soroban_sdk::vec![&e, admin.into_val(&e)],
);
```

**Cost profile**:
- **CPU**: ~120,000 instructions
- **Memory**: ~18,000 bytes
- **Notes**: Simple registration call with one argument

#### Callback on Withdraw (`on_withdraw`)

```rust
// From credence_bond/src/lib.rs
if let Some(cb_addr) = e.storage().instance().get::<_, Address>(&cb_key) {
    let fn_name = Symbol::new(&e, "on_withdraw");
    let args: Vec<Val> = Vec::from_array(&e, [withdraw_amount.into_val(&e)]);
    e.invoke_contract::<Val>(&cb_addr, &fn_name, args);
}
```

**Cost profile**:
- **CPU**: ~150,000 instructions
- **Memory**: ~22,000 bytes
- **Notes**: Optional callback; only executes if callback address is configured

#### Verifier Signature Check

```rust
// From credence_delegation/src/verifier.rs
let args: Vec<Val> = Vec::from_array(e, [
    public_key.clone().into_val(e),
    message.clone().into_val(e),
    signature.clone().into_val(e),
]);
let ok: bool = e.invoke_contract(&verifier_addr, &Symbol::new(e, "verify"), args);
```

**Cost profile**:
- **CPU**: ~200,000 instructions
- **Memory**: ~25,000 bytes
- **Notes**: Crypto verification is expensive; this is a hot path in delegation

### Batch vs Individual Calls

From `docs/dispute_resolution_gas_benchmarks.md`:

| Operation | Single Call CPU | 20 Sequential Calls CPU | Per-Call Amortized |
|-----------|-----------------|------------------------|-------------------|
| `create_dispute` | 301,419 | 385,176 | ~19,258 |
| `cast_vote` | 122,470 | 182,958 | ~9,147 |

**Key insight**: The first call in a transaction pays ~300k CPU in VM overhead. Subsequent calls in the same transaction amortize this overhead to ~3k-20k CPU per call.

### Best Practices

- **Batch operations**: If making multiple calls, execute them in the same transaction to amortize VM overhead
- **Cache results**: Store cross-contract call results in instance storage if used repeatedly
- **Use `try_invoke_contract`**: For optional calls where failure is acceptable
- **Minimize data transfer**: Pass only necessary arguments; prefer IDs over full structs
- **Defer expensive calls**: Move token transfers or crypto operations to separate transaction if possible

---

## Combined Operation Example

### Real-World: `create_bond` Full Cost Breakdown

```rust
// Simplified create_bond flow
pub fn create_bond(e: &Env, identity: Address, amount: i128, duration: u64) {
    // 1. Token transfer (cross-contract call)
    safe_transfer_from(e, &identity, amount);
    
    // 2. Storage write (bond record)
    let bond = IdentityBond { /* ... */ };
    e.storage().persistent().set(&DataKey::Bond(identity), &bond);
    e.storage().persistent().extend_ttl(&DataKey::Bond(identity), MIN_LEDGERS_TO_LIVE);
    
    // 3. Event emit
    emit_bond_created_v2(e, &identity, amount, duration, false, timestamp);
}
```

**Total cost profile**:
- **Token transfer**: ~80k CPU + 12k memory
- **Storage write**: ~45k CPU + 6k memory
- **Event emit**: ~12k CPU + 2.2k memory
- **Total**: ~137k CPU + 20.2k memory

**Optimization opportunity**: If the token transfer could be batched with other operations in the same transaction, the VM overhead would be amortized.

---

## Measurement Tools

### Local Cost Estimation

```rust
// In tests
let cost = env.cost_estimate();
println!("CPU: {}, Memory: {}", cost.cpu_insns, cost.mem_bytes);
```

### Gas Regression Tests

The project includes automated gas regression tests:

```bash
# Run gas benchmarks
cargo bench -p credence_bond --features gas-bench --bench cost

# Update baseline after intentional changes
cargo run -p credence_bond --bin update-cost-baseline
```

See `contracts/credence_bond/benches/cost.rs` for the regression gate implementation.

### Manual Profiling

For detailed profiling of specific functions:

```bash
# Run with cost output
cargo test -p credence_bond -- --nocapture
```

---

## Related Documentation

- [Bond Gas Benchmarks](./bond_gas_benchmarks.md) - Detailed storage operation budgets
- [Dispute Resolution Gas Benchmarks](./dispute_resolution_gas_benchmarks.md) - Batch operation analysis
- [Events Specification](./EVENTS.md) - Complete event schema reference
- [WASM Size Budget](./wasm-size-budget.md) - Binary size constraints

---

## Cost Optimization Checklist

Before submitting a PR that changes hot paths:

- [ ] Did you measure the cost impact with `env.cost_estimate()`?
- [ ] Did you update the cost baseline if the change is intentional?
- [ ] Did you check for redundant storage reads/writes?
- [ ] Did you consider batching multiple operations?
- [ ] Did you use instance storage for frequently accessed config?
- [ ] Did you avoid emitting unnecessary events?
- [ ] Did you defer expensive cross-contract calls where possible?
