# Pause Signer Invariant

This document describes an important storage invariant for the pause
multisig mechanism used across Credence contracts.

Invariant
---------

The value stored at `DataKey::PauseSignerCount` MUST always equal the number
of `DataKey::PauseSigner(Address)` entries that are set to `true` in contract
storage. If these values diverge (for example, by incrementing the counter
without setting the corresponding `PauseSigner` entry), the pause threshold
checks may be undermined and the contract could become unpauseable or
otherwise misbehave.

Testing
-------

- The `credence_delegation` crate includes `test_pause_signer_invariant.rs`,
  which exercises idempotent add/remove flows and alternating sequences and
  asserts the invariant after every `set_pause_signer` call.

Developer note
--------------

Always update both the per-address `PauseSigner(...)` entry and the
`PauseSignerCount` atomically (i.e. only bump the counter when the boolean
state actually changes), and keep tests that enumerate stored signer entries
and compare against the counter.
