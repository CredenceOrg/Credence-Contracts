# Admin CLI (`credence-admin`)

This document describes the `credence-admin` command-line tool — the
operator's interface for administrative actions on Credence protocol contracts.

> See [OPERATOR_BALANCES.md](OPERATOR_BALANCES.md) for what each operation costs
> and how to keep your signing account and protocol balances healthy.

> **What changed in this release?**  
> All handlers — the original bond/delegation commands plus seven new
> timelock and arbitration commands — build **real** Soroban
> `InvokeHostFunction` transactions. `--network`, `--contract`, and `--signer`
> flags have been added.

---

## Installation

The CLI is part of the workspace:

```sh
cargo build -p credence_admin_cli
# binary at: target/debug/credence-admin
```

---

## Global flags

| Flag | Env var | Default | Description |
|------|---------|---------|-------------|
| `--rpc-url <URL>` | `CREDENCE_RPC_URL` | `https://soroban-testnet.stellar.org` | Soroban RPC endpoint |
| `--network <PASSPHRASE>` | `CREDENCE_NETWORK` | `Test SDF Network ; September 2015` | Network passphrase used to hash the transaction |
| `--contract <C_ADDR>` | `CREDENCE_CONTRACT` | *(required)* | Bech32m contract address (`C…`) to invoke |
| `--signer <SECRET>` | `CREDENCE_SIGNER` | *(required for `--submit`)* | Stellar secret key (`S…`) of the transaction signer / admin |
| `--submit` | — | `false` | Sign and submit the transaction; omit for a dry-run |

---

## Subcommands

### `bond-set-early-exit-config`

Calls `set_early_exit_config(admin, treasury, penalty_bps)` on a
`credence_bond` contract.

```sh
credence-admin \
  --contract C… \
  --signer   S… \
  bond-set-early-exit-config \
  --admin    G… \
  --treasury G… \
  --bps      500
```

| Argument | Type | Description |
|----------|------|-------------|
| `--admin G…` | Stellar address | Admin authority address |
| `--treasury G…` | Stellar address | Penalty recipient address |
| `--bps <u32>` | `0..=10000` | Early-exit penalty in basis points (validated locally before any RPC call) |

---

### `bond-set-weights`

Calls `set_weight_config(admin, multiplier_bps, max_weight)` on a
`credence_bond` contract.

```sh
credence-admin \
  --contract C… \
  --signer   S… \
  bond-set-weights \
  --admin          G… \
  --multiplier-bps 10000 \
  --max-weight     200
```

| Argument | Type | Description |
|----------|------|-------------|
| `--admin G…` | Stellar address | Admin authority address |
| `--multiplier-bps <u32>` | `0..=10000` | Attestation weight multiplier in basis points (validated locally before any RPC call) |
| `--max-weight <u32>` | `0..=1000000` | Maximum attestation weight cap (validated locally before any RPC call) |

---

### `delegation-set-pause-signer`

Calls `set_pause_signer(admin, signer, enabled)` on a
`credence_delegation` contract.

```sh
credence-admin \
  --contract C… \
  --signer   S… \
  delegation-set-pause-signer \
  --admin        G… \
  --pause-signer G… \
  --enabled      true
```

| Argument | Type | Description |
|----------|------|-------------|
| `--admin G…` | Stellar address | Admin authority address |
| `--pause-signer G…` | Stellar address | Address to grant/revoke pause authority |
| `--enabled <bool>` | `true`/`false` | Enable or disable the signer (default: `true`) |

---

## Timelock subcommands

### `timelock-queue-call`

Calls `queue_call(proposer, target, func, args, salt, delay)` on a timelock
contract. The queued operation invokes `func` on `target` after `delay`
seconds. `args` are encoded as a Soroban `Vec<Val>`; the salt is a 32-byte
`BytesN<32>`.

```sh
credence-admin \
  --contract C… \
  timelock-queue-call \
  --proposer G… \
  --target C… \
  --func set_quorum \
  --arg address:G… \
  --arg u32:100 \
  --salt 1111111111111111111111111111111111111111111111111111111111111111 \
  --delay 300
```

| Argument | Type | Description |
|----------|------|-------------|
| `--proposer G…` | Stellar address | Admin/proposer address (must match the timelock admin) |
| `--target C…` | Contract address | Contract the queued call will invoke |
| `--func <name>` | Symbol | Function name, 1–32 characters: `[A-Za-z_][A-Za-z0-9_]*` |
| `--arg <TYPE:VALUE>` | `ScVal` | Repeatable typed call argument (see below); order is preserved |
| `--salt <hex>` | 32-byte hex | 64 hex characters used in the operation hash pre-image |
| `--delay <u64>` | seconds | Delay before the operation becomes executable |

### `timelock-execute`

Calls `execute_operation(op_id)` for a queued operation.

```sh
credence-admin --contract C… timelock-execute --op-id 42
```

### `timelock-cancel`

Calls `cancel_operation(admin, op_id)` to cancel a pending operation.

```sh
credence-admin --contract C… timelock-cancel --admin G… --op-id 42
```

---

## Arbitration subcommands

### `arbitration-register-arbitrator`

Calls `register_arbitrator(arbitrator, weight)`. The weight is an `i128` and
should be positive; zero or negative weights are rejected on-chain.

```sh
credence-admin --contract C… arbitration-register-arbitrator \
  --arbitrator G… --weight 100
```

### `arbitration-unregister-arbitrator`

Calls `unregister_arbitrator(arbitrator)`.

```sh
credence-admin --contract C… arbitration-unregister-arbitrator --arbitrator G…
```

### `arbitration-set-quorum`

Calls `set_quorum(admin, min_total_weight, min_voters)`. Passing `0 0`
preserves the legacy "no quorum" behaviour.

```sh
credence-admin --contract C… arbitration-set-quorum \
  --admin G… --min-total-weight 100 --min-voters 3
```

### `arbitration-pause`

Calls `pause(caller)`; the caller must be an admin or configured pause signer.

```sh
credence-admin --contract C… arbitration-pause --caller G…
```

---

## Typed `--arg` values

`--arg` accepts `TYPE:VALUE` pairs. Negative numbers are supported (use
`--arg i128:-5`, not a separate `-5` token).

| Type | Example | Encoded `ScVal` |
|------|---------|-----------------|
| `address` | `address:G…` / `address:C…` | `Address` |
| `bool` | `bool:true` | `Bool` |
| `bytes` | `bytes:deadbeef` | `Bytes` |
| `bytes32` | `bytes32:0102…20` | `Bytes` (exactly 32 bytes, for `BytesN<32>`) |
| `i32` / `i64` / `i128` | `i128:-7` | `I32` / `I64` / `I128` |
| `string` | `string:hello` | `String` |
| `symbol` | `symbol:set_quorum` | `Symbol` |
| `u32` / `u64` | `u32:100` | `U32` / `U64` |
| `void` | `void:` | `Void` |
| `vec` | `vec:[u32:1,address:G…]` | `Vec` of the inner encoded values |

---

## Dry-run mode (default)

When `--submit` is **omitted** the CLI builds the full transaction offline
(using a dummy sequence number `0`) and prints a structured JSON object —
**no network request is made**:

```json
{
  "status": "dry_run",
  "contract": "C…",
  "function": "set_early_exit_config",
  "network": "Test SDF Network ; September 2015",
  "source": "G…",
  "envelope_xdr": "<base64-encoded TransactionEnvelope XDR>",
  "tx_hash": "<hex sha256 of the signature payload>"
}
```

The `envelope_xdr` field contains the **real** `InvokeHostFunction` operation
encoded in XDR — operators can decode and verify it with `stellar xdr decode`
before submitting manually.

---

## Submit mode (`--submit`)

With `--submit` the CLI:

1. Fetches the source account and current sequence number from the RPC.
2. Calls `simulateTransaction` to obtain the Soroban resource budget and
   footprint (via `server.prepare_transaction`).
3. Signs the assembled transaction with the key from `--signer` /
   `CREDENCE_SIGNER`.
4. Calls `sendTransaction` and prints the response:

```json
{
  "status": "PENDING",
  "hash": "abc123…"
}
```

### Missing signer

If `--submit` is used without `--signer` (and `CREDENCE_SIGNER` is unset),
the CLI exits with a non-zero code and prints:

```
Error: --signer / CREDENCE_SIGNER is required with --submit
```

---

## Examples

```sh
# Dry-run: inspect XDR before touching the network
credence-admin \
  --contract CABC123… \
  bond-set-early-exit-config \
  --admin    GABC… \
  --treasury GXYZ… \
  --bps 300

# Submit on testnet (key from env)
export CREDENCE_SIGNER=SABC…
credence-admin \
  --contract CABC123… \
  --submit \
  bond-set-weights \
  --admin          GABC… \
  --multiplier-bps 9000 \
  --max-weight     100

# Submit on mainnet
credence-admin \
  --rpc-url https://mainnet.stellar.validationcloud.io/v1/<key>/soroban/rpc \
  --network "Public Global Stellar Network ; September 2015" \
  --contract CABC123… \
  --signer   SABC… \
  --submit \
  delegation-set-pause-signer \
  --admin        GABC… \
  --pause-signer GPAUSE… \
  --enabled true

# Dry-run a timelocked arbitration quorum change
credence-admin \
  --contract CTIMELOCK… \
  timelock-queue-call \
  --proposer GADMIN… \
  --target CARBITRATION… \
  --func set_quorum \
  --arg address:GADMIN… \
  --arg i128:100 \
  --arg u32:3 \
  --salt 1111111111111111111111111111111111111111111111111111111111111111 \
  --delay 300
```

---

## Testing

Run the integration test suite (offline — no live network required):

```sh
cargo test -p credence_admin_cli -- --nocapture
```

Tests assert that:
- Each subcommand exits `0` and emits valid JSON with `envelope_xdr` and `tx_hash`.
- The `envelope_xdr` field is non-empty (real XDR, not a stub).
- Dry-run envelopes decode back to `InvokeHostFunction` calls with the expected
  function names and `ScVal` arguments (including `Address`, `BytesN<32>`,
  `Vec`, and `I128`).
- Different subcommands produce different XDR (each encodes distinct args).
- Invalid typed arguments are rejected before any network I/O.
- `--submit` without `--signer` exits non-zero with a clear message.

---

## Contributing

Add new subcommands by:
1. Adding a variant to the `Commands` enum in `src/main.rs`.
2. Writing a `build_*_args() -> Result<Vec<ScVal>>` function that encodes the
   on-chain function arguments.
3. Wiring it into `main()` with a `run(&cli, contract_id, "fn_name", args)` call.
4. Adding integration tests in `tests/integration_test.rs`.
