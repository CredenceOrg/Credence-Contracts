# Credence Contracts Threat Model

## Overview
This document provides a STRIDE-style threat model for Credence smart contracts, organized by entrypoint and contract.

Audience: **Contributors** (security-focused developers maintaining/auditing the contracts).

## STRIDE Categories
- **Spoofing**: Pretending to be someone else
- **Tampering**: Modifying data/state without authorization
- **Repudiation**: Denying having performed an action
- **Information disclosure**: Exposing private information
- **Denial of Service**: Making the system unavailable
- **Elevation of privilege**: Gaining unauthorized access/rights

---

## Credence Bond Contract (`credence_bond`)

### Identity Bond Entrypoints

#### `initialize(admin, governance_config)`
| STRIDE Category | Threat | Mitigation |
|-----------------|--------|------------|
| Tampering       | Calling `initialize` multiple times | `initialize` checks `DataKey::Admin` existence and panics if already set |
| Elevation of Privilege | Anyone initializing the contract | The first caller of `initialize` becomes admin; no default admin |

#### `create_bond(identity, amount, duration, is_rolling)`
| STRIDE Category | Threat | Mitigation |
|-----------------|--------|------------|
| Tampering       | Creating bonds with negative `amount` | Validates `amount > 0` |
| Tampering       | Creating bonds with invalid durations | Validates `duration` fits within protocol limits |
| DoS             | Creating many tiny bonds to bloat storage | Inherent Soroban storage limits; gas costs discourage abuse |

#### `top_up(identity, amount)`
| STRIDE Category | Threat | Mitigation |
|-----------------|--------|------------|
| Spoofing        | Topping up someone else's bond | Identity must be authorized (`require_auth`) |
| Tampering       | Overflowing `bonded_amount` | Uses `checked_add()`; panics on overflow |

#### `withdraw_bond(identity, amount)`
| STRIDE Category | Threat | Mitigation |
|-----------------|--------|------------|
| Spoofing        | Withdrawing from someone else's bond | Identity must be authorized (`require_auth`) |
| Tampering       | Withdrawing more than available balance | Validates `amount <= (bonded_amount - slashed_amount)`; uses `checked_sub()` |
| Tampering       | Early withdrawal from locked non-rolling bond | Checks `ledger.timestamp() >= bond_start + bond_duration` for non-rolling bonds |

#### `slash(identity, amount, reason)`
| STRIDE Category | Threat | Mitigation |
|-----------------|--------|------------|
| Spoofing        | Unauthorized users slashing bonds | Restricted to admins/attesters with slashing privileges |
| Tampering       | Slashing more than bonded amount | Automatically caps `slashed_amount` at `bonded_amount` |
| Repudiation     | Denying a slash was performed | Emits `bond_slashed`/`bond_slashed_v2` event with admin, reason, and timestamps |

---

## Credence Delegation Contract (`credence_delegation`)

### Delegation Entrypoints

#### `register_verifier(scheme, verifier_id)`
| STRIDE Category | Threat | Mitigation |
|-----------------|--------|------------|
| Spoofing        | Unauthorized users registering verifiers | Restricted to admin |
| Tampering       | Registering invalid/malicious verifiers | Admin-controlled; verifiers are invoked but only if pre-registered |

---

## Cross-Contract Interactions

### `CredenceDelegation` → `CredenceBond` Calls
See [auth-tree-threats.md](./auth-tree-threats.md) for detailed analysis of Soroban auth tree threats and mitigations!

---

## Additional Resources
- [SECURITY_ANALYSIS.md](../SECURITY_ANALYSIS.md) - Arithmetic security analysis
- [access-control.md](./access-control.md) - Access control model documentation
- [reentrancy.md](./reentrancy.md) - Reentrancy threat analysis
