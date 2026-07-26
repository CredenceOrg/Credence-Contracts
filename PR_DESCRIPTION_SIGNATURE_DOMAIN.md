# Add signature_domain per contract with rustdoc

Closes #751

## Summary

Adds a `SIGNATURE_DOMAIN` constant to each contract in the Credence system as documentation for future signature domain integration. Each contract now has a unique domain identifier documented with comprehensive rustdoc explaining the security rationale for preventing cross-contract replay attacks.

## Threat Model

### Attack Scenario: Cross-Contract Signature Replay

Without signature domain separation, an attacker could replay a signature created for one contract against another contract in the system. This is possible when:

1. **Shared Nonce Namespace**: Multiple contracts use similar nonce tracking mechanisms
2. **Similar Signature Verification**: Contracts implement comparable signature validation logic
3. **Missing Domain Binding**: Signatures are not explicitly bound to a specific contract

### Attack Impact

If an attacker successfully replays a signature across contracts, they could:

- **Unauthorized Operations**: Execute privileged operations in a contract they shouldn't have access to
- **Privilege Escalation**: Use a signature from a lower-privilege contract to access higher-privilege functions
- **State Corruption**: Cause unintended state changes by replaying operations in different contexts
- **Bypass Access Controls**: Circumvent contract-specific authorization checks

### Mitigation

By documenting unique `SIGNATURE_DOMAIN` constants for each contract, we establish the foundation for future signature domain integration. When signatures include these domain identifiers in their payload hash, they become cryptographically bound to their intended contract, preventing cross-contract replay attacks.

## Implementation

### Changes Made

1. **credence_bond**: Added `const SIGNATURE_DOMAIN: &str = "CredenceBond"`
   - Documented with comprehensive rustdoc explaining security rationale
   - Private constant for future signature validation integration

2. **admin**: Added `const SIGNATURE_DOMAIN: &str = "Admin"`
   - Same documentation pattern as credence_bond
   - Ensures admin operations are domain-separated

3. **arbitration**: Added `const SIGNATURE_DOMAIN: &str = "CredenceArbitration"`
   - Prevents replay against dispute resolution operations
   - Maintains consistency across all contracts

4. **credence_delegation**: Added `const SIGNATURE_DOMAIN: &str = "CredenceDelegation"`
   - Protects delegation-specific operations
   - Complements existing `DomainTag` separation

### Documentation

All `SIGNATURE_DOMAIN` constants include comprehensive rustdoc explaining:

- Purpose: Preventing cross-contract replay attacks
- Security rationale: Why domain separation is necessary
- Usage: How to include the domain in signed payloads
- Threat model: What attack is being mitigated

## Verification

To verify this change:

```bash
# Build for WASM target
cargo build --target wasm32-unknown-unknown --release -p credence_bond
cargo build --target wasm32-unknown-unknown --release -p admin
cargo build --target wasm32-unknown-unknown --release -p arbitration
cargo build --target wasm32-unknown-unknown --release -p credence_delegation

# Run tests
cargo test -p credence_bond
cargo test -p admin
cargo test -p arbitration
cargo test -p credence_delegation

# Run clippy
cargo clippy --workspace --all-targets -- -D warnings
```

## Backwards Compatibility

This change is **fully backwards compatible**:

- Constants are private and unused (documentation-only)
- No changes to existing function signatures or storage layout
- No new error codes or validation logic
- No impact on existing contract behavior

## Future Work

Future PRs can integrate these constants into:
1. Signature payload construction (off-chain)
2. Signature validation functions (on-chain)
3. Test coverage for domain mismatch scenarios

## References

- Issue: #751
- Related: EIP-712 (Ethereum domain separation standard)
- Related: Soroban signature best practices
