# Admin CLI (`credence-admin`)

This document describes the `credence-admin` command-line tool — the
operator's interface for administrative actions on Credence protocol contracts.

> **What changed in this release?**  
> All three handlers (`bond-set-early-exit-config`, `bond-set-weights`,
> `delegation-set-pause-signer`) now build **real** Soroban
> `InvokeHostFunction` transactions instead of empty stubs.  
> `--network`, `--contract`, and `--signer` flags have been added.

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
| `--bps <u32>` | 0–10 000 | Early-exit penalty in basis points |

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
| `--multiplier-bps <u32>` | basis points | Attestation weight multiplier |
| `--max-weight <u32>` | integer | Maximum attestation weight cap |

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
- Different subcommands produce different XDR (each encodes distinct args).
- `--submit` without `--signer` exits non-zero with a clear message.

---

## Contributing

Add new subcommands by:
1. Adding a variant to the `Commands` enum in `src/main.rs`.
2. Writing a `build_*_args() -> Result<Vec<ScVal>>` function that encodes the
   on-chain function arguments.
3. Wiring it into `main()` with a `run(&cli, contract_id, "fn_name", args)` call.
4. Adding integration tests in `tests/integration_test.rs`.
