# Governance Approval for Slashing

## Overview

Slash requests require multi-signature (multi-sig) verification before execution. A proposal is created, governors vote (approve or reject), and the slash is applied only when quorum and approval requirements are met. Vote delegation is supported.

## Components

- **Slash proposal**: Amount to slash, proposer, status (Open / Executed / Rejected).
- **Governors**: Set of addresses that can vote; configured at initialization.
- **Quorum**: Minimum share of governors that must vote (basis points), and/or minimum count.
- **Delegation**: A governor may delegate their vote to another address.

## Flow

1. **Initialize** (admin only): `initialize_governance(admin, governors, quorum_bps, min_governors)`.
2. **Propose**: Admin or any governor calls `propose_slash(proposer, amount)` → returns proposal id.
3. **Vote**: Each governor (or their delegate) calls `governance_vote(voter, proposal_id, approve)`.
4. **Execute**: When quorum is met and majority approve, the proposer calls `execute_slash_with_governance(proposer, proposal_id)` to apply the slash.

## API

| Function | Auth | Description |
|----------|------|-------------|
| `initialize_governance(admin, governors, quorum_bps, min_governors)` | Admin | Set governors and quorum. |
| `propose_slash(proposer, amount)` | Proposer (admin or governor) | Create slash proposal. |
| `governance_vote(voter, proposal_id, approve)` | Voter (governor or delegate) | Cast vote. |
| `governance_delegate(governor, to)` | Governor | Delegate vote to `to`. |
| `execute_slash_with_governance(proposer, proposal_id)` | Proposer | Execute approved slash. |
| `get_slash_proposal(proposal_id)` | — | Get proposal. |
| `get_governance_vote(proposal_id, voter)` | — | Get vote. |
| `get_governors()` | — | List governors. |
| `get_governance_delegate(governor)` | — | Get delegate. |
| `get_quorum_config()` | — | (quorum_bps, min_governors). |

## Events

- `slash_proposed`: (proposal_id, proposer, amount)
- `governance_vote`: (proposal_id, voter, 1=approve / 0=reject)
- `governance_delegate`: (proposal_id=0, governor, 0)
- `slash_proposal_executed`: (proposal_id, proposer, amount)
- `slash_proposal_rejected`: (proposal_id, proposer, amount)

## Quorum and Approval

- **Quorum**: `voted_count >= max(total_governors * quorum_bps / 10000, min_governors)`.
- **Approval**: Majority of votes that were cast must be approve (`approve_count > voted_count / 2`).
- Execution is only allowed when both quorum and approval are satisfied; only the proposer may call `execute_slash_with_governance`.

## Security

- Only the proposer can execute an approved proposal.
- Double voting is rejected.
- Non-governors and non-delegates cannot vote.
- Governance is initialized once by admin; governors and quorum are then fixed for the contract instance.
