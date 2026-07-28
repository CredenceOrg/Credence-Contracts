# Single-Admin Multi‑Signature, Timelocked & Delegated Overview

## Summary

This document describes the **single‑admin** governance model that combines **multi‑signature** approval, **timelocked** execution, and **delegated** authority for the Credence contracts.

- **Single‑admin** – One privileged address (the admin) can configure signers, thresholds, and timelock parameters.
- **Multi‑signature** – Proposals require a configurable number of signer approvals before execution.
- **Timelock** – Approved proposals can only be executed after a configurable delay, protecting against rushed changes.
- **Delegated** – Certain actions can be delegated to a separate contract or address, enabling modular governance.

## Key Components

| Component | Description |
|-----------|-------------|
| `Admin` | Authority that can add/remove signers, adjust the threshold, and set timelock duration. |
| `Signer` | Addresses that can submit and sign proposals. |
| `Threshold` | Minimum number of signatures required for a proposal to become executable. |
| `Timelock` | Minimum time (in seconds) that must elapse after a proposal reaches the threshold before it can be executed. |
| `Delegate` | Optional address/contract that can perform specific actions on behalf of the admin (e.g., fund withdrawals). |

## Proposal Lifecycle

1. **Submit** – A signer calls `submit_proposal` with the desired action, optional expiration, and metadata.
2. **Sign** – Additional signers call `sign_proposal`. Once the signature count meets `Threshold`, the proposal status becomes `Ready`.
3. **Timelock Wait** – The system records the earliest `execute_after` timestamp (`now + timelock`). No execution is possible before this time.
4. **Execute** – Any address can call `execute_proposal` after the timelock has passed. The contract performs the requested action or forwards it to the delegated contract.
5. **Reject / Expire** – Admin can `reject_proposal` at any time before execution. Proposals also expire automatically if an `expires_at` timestamp is set.

## Example Workflow

```rust
// 1. Admin sets up the contract
admin.initialize(
    admin_address,
    vec![signer1, signer2, signer3],
    2,               // 2‑of‑3 threshold
    86400,           // 24‑hour timelock
    Some(delegate_address) // optional delegate
);

// 2. Signer submits a proposal to update a config parameter
let proposal_id = client.submit_proposal(
    &signer1,
    ActionType::ConfigChange,
    None,
    None,
    None,
    String::from_str(&e, "Set fee to 0.5%"),
    0, // no expiration
    None,
);

// 3. Another signer signs
client.sign_proposal(&signer2, proposal_id);

// 4. Wait 24 h (timelock) then execute
client.execute_proposal(proposal_id);
```

## Security Considerations

- **Threshold Safety** – The admin must ensure `1 ≤ threshold ≤ signer_count`. The contract auto‑adjusts the threshold to `1` if the last signer is removed.
- **Replay Protection** – Each proposal ID is unique and immutable; signatures are stored per `(proposal_id, signer)`.
- **Delegation Limits** – Delegated actions are limited to a whitelist defined by the admin to avoid privilege escalation.
- **Expiration** – Optional `expires_at` prevents stale proposals from being executed after long delays.

## Linking

The document is linked from the top‑level `README.md` under the **Docs** section.
