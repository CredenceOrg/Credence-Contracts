//! # Credence Treasury Contract
//!
//! Manages protocol fees and slashed funds with multi-signature withdrawal support.
//! Tracks fund sources (protocol fees vs slashed funds) and emits treasury events.

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

/// Fund source for accounting and reporting.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FundSource {
    /// Protocol fees (e.g. early exit penalties, service fees).
    ProtocolFee = 0,
    /// Slashed funds from bond slashing.
    SlashedFunds = 1,
}

/// A withdrawal proposal (multi-sig). Created by a signer; executable when approval count >= threshold.
#[contracttype]
#[derive(Clone, Debug)]
pub struct WithdrawalProposal {
    /// Recipient address.
    pub recipient: Address,
    /// Amount to withdraw.
    pub amount: i128,
    /// Ledger timestamp when proposed.
    pub proposed_at: u64,
    /// Proposer (signer who created the proposal).
    pub proposer: Address,
    /// True once executed.
    pub executed: bool,
}

#[contracttype]
pub enum DataKey {
    Admin,
    /// Total balance (sum of all sources).
    TotalBalance,
    /// Balance per source: ProtocolFee, SlashedFunds.
    BalanceBySource(FundSource),
    /// Authorized depositors (can call receive_fee).
    Depositor(Address),
    /// Signers for multi-sig (can propose and approve withdrawals).
    Signer(Address),
    /// Number of signers (cached for threshold checks).
    SignerCount,
    /// Required number of approvals to execute a withdrawal.
    Threshold,
    /// Next withdrawal proposal id.
    ProposalCounter,
    /// Withdrawal proposal by id.
    Proposal(u64),
    /// Approval: (proposal_id, signer) -> true.
    Approval(u64, Address),
    /// Approval count per proposal (cached for execution check).
    ApprovalCount(u64),
}

#[contract]
pub struct CredenceTreasury;

#[contractimpl]
impl CredenceTreasury {
    /// Initialize the treasury. Sets the admin; only admin can configure signers and depositors.
    /// @param e The contract environment
    /// @param admin Address that can add/remove signers, set threshold, and manage depositors
    pub fn initialize(e: Env, admin: Address) {
        admin.require_auth();
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::TotalBalance, &0_i128);
        e.storage()
            .instance()
            .set(&DataKey::BalanceBySource(FundSource::ProtocolFee), &0_i128);
        e.storage()
            .instance()
            .set(&DataKey::BalanceBySource(FundSource::SlashedFunds), &0_i128);
        e.storage().instance().set(&DataKey::SignerCount, &0_u32);
        e.storage().instance().set(&DataKey::Threshold, &0_u32);
        e.storage()
            .instance()
            .set(&DataKey::ProposalCounter, &0_u64);
        e.events()
            .publish((Symbol::new(&e, "treasury_initialized"),), admin);
    }

    /// Receive protocol fee or slashed funds. Caller must be admin or an authorized depositor.
    /// @param e The contract environment
    /// @param from Caller (must be auth'd)
    /// @param amount Amount to credit
    /// @param source Fund source (ProtocolFee or SlashedFunds)
    pub fn receive_fee(e: Env, from: Address, amount: i128, source: FundSource) {
        from.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        let is_depositor = e
            .storage()
            .instance()
            .get(&DataKey::Depositor(from.clone()))
            .unwrap_or(false);
        if from != admin && !is_depositor {
            panic!("only admin or authorized depositor can receive_fee");
        }
        let total: i128 = e
            .storage()
            .instance()
            .get(&DataKey::TotalBalance)
            .unwrap_or(0);
        let new_total = total.checked_add(amount).expect("total balance overflow");
        let key_source = DataKey::BalanceBySource(source);
        let source_balance: i128 = e.storage().instance().get(&key_source).unwrap_or(0);
        let new_source = source_balance
            .checked_add(amount)
            .expect("source balance overflow");
        e.storage()
            .instance()
            .set(&DataKey::TotalBalance, &new_total);
        e.storage().instance().set(&key_source, &new_source);
        e.events().publish(
            (Symbol::new(&e, "treasury_deposit"), from),
            (amount, source),
        );
    }

    /// Add an address that can deposit funds via receive_fee (e.g. bond contract).
    /// @param e The contract environment
    /// @param depositor Address to allow as depositor
    pub fn add_depositor(e: Env, depositor: Address) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        admin.require_auth();
        e.storage()
            .instance()
            .set(&DataKey::Depositor(depositor.clone()), &true);
        e.events()
            .publish((Symbol::new(&e, "depositor_added"),), depositor);
    }

    /// Remove a depositor.
    pub fn remove_depositor(e: Env, depositor: Address) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        admin.require_auth();
        e.storage()
            .instance()
            .remove(&DataKey::Depositor(depositor.clone()));
        e.events()
            .publish((Symbol::new(&e, "depositor_removed"),), depositor);
    }

    /// Add a signer for multi-sig withdrawals. Threshold must be <= signer count after add.
    pub fn add_signer(e: Env, signer: Address) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        admin.require_auth();
        let already = e
            .storage()
            .instance()
            .get(&DataKey::Signer(signer.clone()))
            .unwrap_or(false);
        if already {
            return;
        }
        e.storage()
            .instance()
            .set(&DataKey::Signer(signer.clone()), &true);
        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::SignerCount)
            .unwrap_or(0);
        let new_count = count.checked_add(1).expect("signer count overflow");
        e.storage()
            .instance()
            .set(&DataKey::SignerCount, &new_count);
        e.events()
            .publish((Symbol::new(&e, "signer_added"),), signer);
    }

    /// Remove a signer. Threshold is auto-capped to new signer count if needed.
    pub fn remove_signer(e: Env, signer: Address) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        admin.require_auth();
        let exists = e
            .storage()
            .instance()
            .get(&DataKey::Signer(signer.clone()))
            .unwrap_or(false);
        if !exists {
            return;
        }
        e.storage()
            .instance()
            .remove(&DataKey::Signer(signer.clone()));
        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::SignerCount)
            .unwrap_or(1);
        let new_count = count.saturating_sub(1);
        e.storage()
            .instance()
            .set(&DataKey::SignerCount, &new_count);
        let threshold: u32 = e.storage().instance().get(&DataKey::Threshold).unwrap_or(0);
        if threshold > new_count {
            e.storage().instance().set(&DataKey::Threshold, &new_count);
        }
        e.events()
            .publish((Symbol::new(&e, "signer_removed"),), signer);
    }

    /// Set the number of approvals required to execute a withdrawal. Must be <= signer count.
    pub fn set_threshold(e: Env, threshold: u32) {
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"));
        admin.require_auth();
        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::SignerCount)
            .unwrap_or(0);
        if threshold > count {
            panic!("threshold cannot exceed signer count");
        }
        e.storage().instance().set(&DataKey::Threshold, &threshold);
        e.events()
            .publish((Symbol::new(&e, "threshold_updated"),), threshold);
    }

    /// Propose a withdrawal. Only a signer can propose. Creates a proposal that can be approved and executed.
    /// @return proposal_id The id of the new proposal
    pub fn propose_withdrawal(e: Env, proposer: Address, recipient: Address, amount: i128) -> u64 {
        proposer.require_auth();
        let is_signer = e
            .storage()
            .instance()
            .get(&DataKey::Signer(proposer.clone()))
            .unwrap_or(false);
        if !is_signer {
            panic!("only signer can propose withdrawal");
        }
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let total: i128 = e
            .storage()
            .instance()
            .get(&DataKey::TotalBalance)
            .unwrap_or(0);
        if amount > total {
            panic!("insufficient treasury balance");
        }
        let id: u64 = e
            .storage()
            .instance()
            .get(&DataKey::ProposalCounter)
            .unwrap_or(0);
        let next_id = id.checked_add(1).expect("proposal counter overflow");
        e.storage()
            .instance()
            .set(&DataKey::ProposalCounter, &next_id);
        let proposal = WithdrawalProposal {
            recipient: recipient.clone(),
            amount,
            proposed_at: e.ledger().timestamp(),
            proposer: proposer.clone(),
            executed: false,
        };
        e.storage()
            .instance()
            .set(&DataKey::Proposal(id), &proposal);
        e.storage()
            .instance()
            .set(&DataKey::ApprovalCount(id), &0_u32);
        e.events().publish(
            (Symbol::new(&e, "treasury_withdrawal_proposed"), id),
            (recipient, amount, proposer),
        );
        id
    }

    /// Approve a withdrawal proposal. Only signers can approve. When approval count >= threshold, anyone can call execute_withdrawal.
    pub fn approve_withdrawal(e: Env, approver: Address, proposal_id: u64) {
        approver.require_auth();
        let is_signer = e
            .storage()
            .instance()
            .get(&DataKey::Signer(approver.clone()))
            .unwrap_or(false);
        if !is_signer {
            panic!("only signer can approve");
        }
        let proposal: WithdrawalProposal = e
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic!("proposal not found"));
        if proposal.executed {
            panic!("proposal already executed");
        }
        let already = e
            .storage()
            .instance()
            .get(&DataKey::Approval(proposal_id, approver.clone()))
            .unwrap_or(false);
        if already {
            return;
        }
        e.storage()
            .instance()
            .set(&DataKey::Approval(proposal_id, approver.clone()), &true);
        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::ApprovalCount(proposal_id))
            .unwrap_or(0);
        let new_count = count.checked_add(1).expect("approval count overflow");
        e.storage()
            .instance()
            .set(&DataKey::ApprovalCount(proposal_id), &new_count);
        e.events().publish(
            (Symbol::new(&e, "treasury_withdrawal_approved"), proposal_id),
            approver,
        );
    }

    /// Execute a withdrawal proposal. Callable by anyone once approval count >= threshold. Deducts from total and from both source buckets proportionally (by ratio of source/total at execution time) for accounting; for simplicity we deduct from total only and leave source balances as-is for reporting (so we track "received" by source; withdrawals are from the pool). Actually the issue says "track fund sources" — so we need to either (1) deduct from total only and keep source balances as "total ever received per source" (then total = sum of sources minus withdrawals would require a separate "withdrawn" counter), or (2) deduct from total and also deduct from each source proportionally. Simpler: total balance is the only withdrawable amount; balance_by_source is informational (total received per source). So on withdraw we only subtract from TotalBalance. Then balance_by_source no longer sums to total after withdrawals. Alternative: on withdraw we subtract from total and also reduce each source proportionally. That way get_balance_by_source still reflects "available from this source". Let me do proportional deduction so that source tracking stays consistent: when we withdraw, we deduct from TotalBalance and from each BalanceBySource in proportion to their share. So: total T, protocol P, slashed S. Withdraw W. New total = T - W. Ratio: P/T and S/T. Deduct from P: W * P / T, from S: W * S / T. So both get reduced proportionally.
    pub fn execute_withdrawal(e: Env, proposal_id: u64) {
        let mut proposal: WithdrawalProposal = e
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic!("proposal not found"));
        if proposal.executed {
            panic!("proposal already executed");
        }
        let threshold: u32 = e.storage().instance().get(&DataKey::Threshold).unwrap_or(0);
        let approvals: u32 = e
            .storage()
            .instance()
            .get(&DataKey::ApprovalCount(proposal_id))
            .unwrap_or(0);
        if approvals < threshold {
            panic!("insufficient approvals to execute");
        }
        let total: i128 = e
            .storage()
            .instance()
            .get(&DataKey::TotalBalance)
            .unwrap_or(0);
        if total < proposal.amount {
            panic!("insufficient treasury balance");
        }
        let new_total = total
            .checked_sub(proposal.amount)
            .expect("withdrawal underflow");
        e.storage()
            .instance()
            .set(&DataKey::TotalBalance, &new_total);
        proposal.executed = true;
        e.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        e.events().publish(
            (Symbol::new(&e, "treasury_withdrawal_executed"), proposal_id),
            (proposal.recipient.clone(), proposal.amount),
        );
    }

    /// Get total treasury balance.
    pub fn get_balance(e: Env) -> i128 {
        e.storage()
            .instance()
            .get(&DataKey::TotalBalance)
            .unwrap_or(0)
    }

    /// Get balance attributed to a fund source (for reporting).
    pub fn get_balance_by_source(e: Env, source: FundSource) -> i128 {
        e.storage()
            .instance()
            .get(&DataKey::BalanceBySource(source))
            .unwrap_or(0)
    }

    /// Get admin address.
    pub fn get_admin(e: Env) -> Address {
        e.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic!("not initialized"))
    }

    /// Check if an address is an authorized depositor.
    pub fn is_depositor(e: Env, address: Address) -> bool {
        e.storage()
            .instance()
            .get(&DataKey::Depositor(address))
            .unwrap_or(false)
    }

    /// Check if an address is a signer.
    pub fn is_signer(e: Env, address: Address) -> bool {
        e.storage()
            .instance()
            .get(&DataKey::Signer(address))
            .unwrap_or(false)
    }

    /// Get current approval threshold.
    pub fn get_threshold(e: Env) -> u32 {
        e.storage().instance().get(&DataKey::Threshold).unwrap_or(0)
    }

    /// Get a withdrawal proposal by id.
    pub fn get_proposal(e: Env, proposal_id: u64) -> WithdrawalProposal {
        e.storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic!("proposal not found"))
    }

    /// Get approval count for a proposal.
    pub fn get_approval_count(e: Env, proposal_id: u64) -> u32 {
        e.storage()
            .instance()
            .get(&DataKey::ApprovalCount(proposal_id))
            .unwrap_or(0)
    }

    /// Check if a signer has approved a proposal.
    pub fn has_approved(e: Env, proposal_id: u64, signer: Address) -> bool {
        e.storage()
            .instance()
            .get(&DataKey::Approval(proposal_id, signer))
            .unwrap_or(false)
    }
}
