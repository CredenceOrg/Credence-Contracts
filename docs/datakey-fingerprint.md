# DataKey Fingerprint Test

`DataKey` enum variants are the literal ledger keys under which a contract stores
its state. If a key's encoding changes between two deployed versions, the new
code reads/writes a *different* slot and the old entry is **orphaned** — funds,
flags, and counters silently disappear from the contract's view.

`tests/datakey_fingerprint.rs` pins the XDR encoding of every
`credence_delegation::DataKey` variant so any change that would move a key fails
CI instead of shipping.

## How a `#[contracttype]` enum is keyed

This is the crucial detail, and it differs from a `bincode` or `#[repr(u32)]`
enum:

> A Soroban `#[contracttype]` enum is encoded as an `ScVal::Vec` whose **first
> element is a `Symbol` of the variant name**, followed by its fields. The key is
> derived from the **variant name and field shape — not its declaration order.**

You can see this in the pinned bytes: the fingerprint for `Admin` contains
`41646d696e` ("Admin") and `PauseSigner` contains `50617573655369676e6572`
("PauseSigner"). The leading structural bytes are identical across unit variants.

Consequences for upgrades:

| Change                                   | Key moves? | Orphans entries? |
|------------------------------------------|------------|------------------|
| **Rename** a variant                     | Yes        | **Yes** |
| **Change** a variant's field count/types | Yes        | **Yes** |
| **Reorder** variants                     | No         | No |
| **Append** a new variant at the end      | No (new key) | No |

So the real hazard is **renaming or re-typing**, not reordering. The fingerprint
test catches exactly those.

> Note: the issue framing ("reordering changes the byte tag") describes
> order-indexed encodings like `bincode`. It does not apply to Soroban
> `#[contracttype]` enums, which are name-indexed — verified empirically by the
> byte layout above. The test therefore pins names + field shapes, which are the
> properties whose change actually orphans ledger state.

## The variant-ordering / stability rule

1. **Never rename** a `DataKey` variant on a deployed contract.
2. **Never change** a variant's field count or field types.
3. **Append** new variants at the end (keep the file readable; reordering is
   encoding-safe but pointless and noisy).
4. Any intentional change to the above must be paired with a reviewed update to
   the `EXPECTED` snapshot in `tests/datakey_fingerprint.rs`.

## What the test does

For each variant (built with fixed, deterministic placeholder field values), it:

1. Encodes the key to XDR via `soroban_sdk::xdr::ToXdr` → `Bytes`.
2. Renders `name = hex(bytes)` lines in declaration order.
3. Asserts the rendered block equals the pinned `EXPECTED` constant.

A second test asserts all fingerprints are mutually distinct, so no two variants
can ever alias the same storage slot.

## Refreshing the snapshot (intentional changes only)

```sh
cargo test -p credence_delegation --test datakey_fingerprint -- --nocapture
```

Copy the printed `---- DataKey fingerprints ----` block into `EXPECTED`, and
**review the diff**: every changed line is a ledger key that moved.

## Validating the guard

Rename or change the fields of any `DataKey` variant (e.g. `Nonce` → `Nonces`)
and re-run the test: `datakey_fingerprints_are_pinned` fails with the moved key
shown in the diff. Revert to restore green.

## Bond contract

The same rule applies to `credence_bond`'s `DataKey`; its declaration carries the
same stability doc-comment. A parallel fingerprint test there is blocked only by
unrelated build breakage in the bond crate, and should be added with the same
pattern once that crate compiles.
