# Credence Contracts — Documentation Index

This directory contains all design docs, API references, and operational guides for the Credence Soroban contracts.

## Core Concepts

| Document | Audience | Summary |
|---|---|---|
| [architecture.md](architecture.md) | Contributor / Operator | System-wide component diagram, data flows, trust boundaries |
| [access-control.md](access-control.md) | Contributor / Integrator | RBAC modifiers, entrypoint authority matrix, event schemas |
| [admin-roles.md](admin-roles.md) | Operator / Integrator | Hierarchical admin system (SuperAdmin / Admin / Operator), assignment, suspension, rotation |
| [HISTORICAL_ROLES.md](HISTORICAL_ROLES.md) | Operator / Auditor | How role assignments are tracked over time, event stream for indexing, audit queries |
| [governance.md](governance.md) | Operator | Multi-sig pause, emergency mode, upgrade flow |
| [upgrade.md](UPGRADE.md) | Operator | Contract upgrade procedure, data migration, verification |

## Bond Contract (`credence_bond`)

| Document | Audience | Summary |
|---|---|---|
| [credence-bond.md](credence-bond.md) | Integrator | High-level overview of the identity bond contract |
| [credence_bond_api.md](credence_bond_api.md) | Integrator | Complete API reference (entrypoints, types, errors) |
| [bond-state-transitions.md](bond-state-transitions.md) | Contributor | State machine for bond lifecycle |
| [tier-system.md](tier-system.md) | Contributor / Operator | Auto-upgrade/downgrade tier logic |
| [rolling-bonds.md](rolling-bonds.md) | Integrator | `request_withdrawal` / `renew_if_rolling` flow |
| [early-exit.md](early-exit.md) | Integrator | Penalty calculation, treasury routing |
| [slashing.md](slashing.md) | Contributor / Operator | Slash entrypoints, available-balance enforcement |
| [slashing-history.md](slashing-history.md) | Integrator / Auditor | Append-only slash record storage |
| [withdrawal.md](withdrawal.md) | Integrator | Normal and early withdrawal flows |
| [bond-invariants.md](bond-invariants.md) | Contributor | Mathematical invariants tested in fuzz suite |
| [bond-drift-detection.md](bond-drift-detection.md) | Operator | Detecting storage drift across deployments |
| [bond-introspection.md](bond-introspection.md) | Integrator | Read-only view functions |
| [bond-crate-layout.md](bond-crate-layout.md) | Contributor | Module map, public re-exports |
| [bond-token-custody.md](bond-token-custody.md) | Operator | Token custody semantics during bond lifecycle |
| [fixed-duration-bond.md](fixed-duration-bond.md) | Integrator | Non-rolling bond variant |
| [multi-identity-bonds.md](multi-identity-bonds.md) | Integrator | Multiple identities per bond |
| [budget-ceilings.md](budget-ceilings.md) | Operator | Protocol fee caps |
| [cooldown.md](cooldown.md) | Integrator | Cooldown periods between operations |
| [expiry-boundaries.md](expiry-boundaries.md) | Contributor | Ledger timestamp edge cases |
| [fees.md](fees.md) | Integrator | Fee calculation and collection |
| [fund-flow.md](fund-flow.md) | Operator | Token flows through the contract |
| [liquidation.md](liquidation.md) | Operator | Liquidation mechanics |
| [treasury.md](treasury.md) | Operator | Treasury configuration and sweeping |
| [weighted-attestations.md](weighted-attestations.md) | Contributor | Attestation weighting system |

## Delegation Contract (`credence_delegation`)

| Document | Audience | Summary |
|---|---|---|
| [delegation.md](delegation.md) | Integrator | Delegation types, expiry, revocation, cleanup |
| [credence_delegation_api.md](credence_delegation_api.md) | Integrator | Full API reference |
| [delegation-failure-modes.md](delegation-failure-modes.md) | Contributor | Error code taxonomy, replay protection |
| [delegation-summary-view.md](delegation-summary-view.md) | Integrator | Aggregated delegation queries |

## Events & Indexing

| Document | Audience | Summary |
|---|---|---|
| [EVENTS.md](EVENTS.md) | Integrator / Indexer | Canonical event catalog with topics and payloads |
| [event-indexing.md](event-indexing.md) | Indexer | Indexer architecture, cursor management, replay |
| [EVENT_INDEXING_MIGRATION.md](EVENT_INDEXING_MIGRATION.md) | Operator | Migration guide for indexer schema changes |
| [indexer-replay-contract.md](indexer-replay-contract.md) | Operator | Replay contract for backfilling |

## Security & Threat Modeling

| Document | Audience | Summary |
|---|---|---|
| [security.md](security.md) | Contributor / Auditor | Security model, trust assumptions |
| [THREAT_MODEL.md](THREAT_MODEL.md) | Auditor | STRIDE analysis, mitigations |
| [auth-tree-threats.md](auth-tree-threats.md) | Auditor | Auth tree specific threats |
| [reentrancy.md](reentrancy.md) | Contributor | Reentrancy guards, patterns |
| [arbitration.md](arbitration.md) | Contributor | Dispute resolution flow |
| [dispute-resolution.md](dispute-resolution.md) | Integrator | Arbitration API |
| [emergency.md](emergency.md) | Operator | Emergency mode, drain, audit log |
| [emergency-drain.md](emergency-drain.md) | Operator | Emergency withdrawal procedure |
| [pause-proposal-view.md](pause-proposal-view.md) | Integrator | Pause multi-sig proposal view |
| [pause-signer-invariant.md](pause-signer-invariant.md) | Contributor | Pause signer guarantees |
| [pause-state-snapshots.md](pause-state-snapshots.md) | Contributor | Pause state serialization |
| [SECURITY_SCANNING.md](SECURITY_SCANNING.md) | Contributor | `cargo audit` workflow, triage |

## Operations & Deployment

| Document | Audience | Summary |
|---|---|---|
| [DEPLOYMENT.md](DEPLOYMENT.md) | Operator | Testnet/mainnet deploy runbook, cross-contract wiring |
| [admin-cli.md](admin-cli.md) | Operator | CLI for admin operations |
| [STORAGE_KEYS.md](STORAGE_KEYS.md) | Contributor / Operator | Storage key enum, TTL policies |
| [storage-ttl.md](storage-ttl.md) | Contributor | TTL extension strategies |
| [wasm-reproducibility.md](wasm-reproducibility.md) | Operator | Reproducible build verification |
| [wasm-size-budget.md](wasm-size-budget.md) | Contributor | Per-contract size ceilings, CI gate |

## Testing & Quality

| Document | Audience | Summary |
|---|---|---|
| [testing.md](testing.md) | Contributor | Test organization, patterns, coverage |
| [doctest-style.md](doctest-style.md) | Contributor | Doc-test conventions |
| [fuzz-testing.md](fuzz-testing.md) | Contributor | Cargo-fuzz targets, invariants |
| [chaos-testing.md](chaos-testing.md) | Contributor | Chaos engineering scenarios |
| [differential-testing.md](differential-testing.md) | Contributor | Cross-implementation diff testing |
| [tier-fuzz.md](tier-fuzz.md) | Contributor | Tier system fuzz invariants |

## Reference & Utilities

| Document | Audience | Summary |
|---|---|---|
| [datakey-fingerprint.md](datakey-fingerprint.md) | Contributor | Storage key fingerprinting for upgrades |
| [decimal-handling.md](decimal-handling.md) | Contributor | Fixed-point arithmetic patterns |
| [error-codes-wire.md](error-codes-wire.md) | Integrator | On-chain error code → off-chain mapping |
| [errors.md](errors.md) | Contributor | Error enum definitions |
| [proposal-id-derivation.md](proposal-id-derivation.md) | Contributor | Deterministic proposal ID scheme |
| [registry.md](registry.md) | Integrator | Contract registry pattern |
| [signature-scheme-upgrade.md](signature-scheme-upgrade.md) | Contributor | Ed25519 → secp256k1 migration plan |
| [status-snapshot.md](status-snapshot.md) | Integrator | On-chain status snapshots |
| [supported-tokens.md](supported-tokens.md) | Operator | Allowlisted token configuration |
| [templates.md](templates.md) | Contributor | Code generation templates |
| [token-integration.md](token-integration.md) | Integrator | Adding new token types |
| [verifiers.md](verifiers.md) | Integrator | Verifier registration and stake |
| [arbitration_api.md](arbitration_api.md) | Integrator | Arbitration contract API |
| [credence-timelock.md](credence-timelock.md) | Integrator | Timelock contract |
| [multisig.md](multisig.md) | Integrator | Multi-sig wallet contract |
| [known-simplifications.md](known-simplifications.md) | Contributor / Auditor | Intentional simplifications and production paths |

## Benchmarks

| Document | Audience | Summary |
|---|---|---|
| [bond_gas_benchmarks.md](bond_gas_benchmarks.md) | Contributor | Bond contract gas costs |
| [dispute_resolution_gas_benchmarks.md](dispute_resolution_gas_benchmarks.md) | Contributor | Arbitration gas costs |

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) for code style, testing requirements, and PR process.

## Quick Links

- [Root README](../README.md) — Workspace overview, build/test commands
- [CHANGELOG.md](../CHANGELOG.md) — Release history