# Credence Contracts

Soroban smart contracts for the Credence economic trust protocol on Stellar: bonds, attestations, arbitration, dispute resolution, timelocks and treasury.

**Live demo (frontend):** https://credence-frontend-fawn.vercel.app

## Testnet deployment

| Contract | Contract ID (Stellar testnet) |
|---|---|
| `timelock` | [`CAKAQJWZN5O2BTSWKUAJZNJIF6ZPBZ2UEZM6H4IIVTC675JXHICCGM4K`](https://stellar.expert/explorer/testnet/contract/CAKAQJWZN5O2BTSWKUAJZNJIF6ZPBZ2UEZM6H4IIVTC675JXHICCGM4K) |
| `credence_arbitration` | [`CCIED32YR7ZB5MBDKRCT7HYFWAH2AAKFH43HL4IREGSUJ2U3ZHECCL7T`](https://stellar.expert/explorer/testnet/contract/CCIED32YR7ZB5MBDKRCT7HYFWAH2AAKFH43HL4IREGSUJ2U3ZHECCL7T) |
| `dispute_resolution` | [`CCKOGX3BZOH7VSMTIVPITZU6S32NY4PETJIRVQ6SA5WY7PABLMU3KXAH`](https://stellar.expert/explorer/testnet/contract/CCKOGX3BZOH7VSMTIVPITZU6S32NY4PETJIRVQ6SA5WY7PABLMU3KXAH) |

## Layout

- `contracts/` — one crate per contract, plus shared crates (`credence_errors`, `credence_math`, ...)
- `crates/` — shared interfaces, test utilities and the admin CLI
- `docs/` — design notes and templates

## Build

```bash
stellar contract build            # all contracts
stellar contract build --package timelock
```

Deploy a built contract to testnet:

```bash
stellar contract deploy --wasm target/wasm32v1-none/release/<crate>.wasm --source <identity> --network testnet
```
