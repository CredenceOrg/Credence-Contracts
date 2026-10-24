# Pause Signer Invariant

This document describes an important storage invariant for the pause
multisig mechanism used across Credence contracts.

Invariant
---------

The value stored at `DataKey::PauseSignerCount` MUST always equal the number
of `DataKey::PauseSigner(Address)` entries that are set to `true` in contract
storage. If these values digerge (for example, by incrementing the counter
without setting the corresponding `PauseSigner` entry), the pause threshold
checks may be undermined and the contract could become unpauseable or
otherwise misbehave.

Approval invariant
------------------

Per-proposal approval bookkeeping must remain consistent with the current
pause-signer set:

- The cached `PauseApprovalCount(id)` is an upper bound only.
- `execute_pause_proposal` re-derives the effective count from
  `PauseApprovers(id)` and the live `PauseSigner(addr)` flags.
- Approvals from revoked signers are ignored.
- When a signer is disabled and re-enabled, its old approval does not
  count; it must re-approve.
- When a fresh payload is created for a reused id, the approval count and
  approver list are reset.
- After a successful execution, the proposal payload, count, flags, and
  approver list are cleared.

Testing
-------

- The `credence_delegation` crate includes `test_pause_signer_invariant.rs`,
  which exercises idempotent add/remove flows and alternating sequences and
  asserts the invariant after every `set_pause_signer` call.
- The `credence_multisig` crate includes tests covering same-epoch
  repositioning, removed-signer revalidation, re-added-signer required
  re-approval, and clearing of approval state after execution.

Developer note
--------------

Always update both the per-address `PauseSigner(...)` entry and the
`PauseSignerCount` atomically (i.e. only bump the counter when the boolean
state actually changes), and keep tests that enumerate stored signer entries
and compare against the counter.
