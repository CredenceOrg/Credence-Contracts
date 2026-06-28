//! # Credence Treasury Contract
//!
//! Manages protocol fees and slashed funds with multi-signature withdrawal support.
//! Tracks fund sources (protocol fees vs slashed funds) and emits treasury events.

use credence_errors::ContractError;
use soroban_sdk::String;
use ethnum::U256;
use soroban_sdk::{contract, contractimpl, contracttype, panic_with_error, Address, Env, Symbol};

use crate::pausable;

const CUMULATIVE_SEGMENT: u128 = (i128::MAX as u128) + 1;

/// Default proposal time-to-live in ledger seconds (7 days).
const DEFAULT_PROPOSAL_TTL: u64 = 7 * 24 * 60 * 60;

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
    /// Proposal expiry timestamp (proposed_at + ttl). Approvals and execution are
    /// rejected once now >= expires_at.
    pub expires_at: u64,
    /// Proposer (signer who created the proposal).
    pub proposer: Address,
    /// True once executed.
    pub executed: bool,
}

/// Lifetime cumulative amount using rollover-safe accounting.
///
/// The represented total is:
/// `rollovers * (i128::MAX + 1) + remainder`
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CumulativeAmount {
    pub rollovers: u64,
    pub remainder: i128,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Paused,
    PauseSigner(Address),
    PauseSignerCount,
    PauseThreshold,
    PauseProposalCounter,
    PauseProposal(u64),
    PauseApproval(u64, Address),
    PauseApprovalCount(u64),
    /// Total balance (sum of all sources).
    TotalBalance,
    /// Available balance per source: ProtocolFee, SlashedFunds.
    BalanceBySource(FundSource),
    /// Lifetime cumulative amount received across all sources.
    CumulativeReceived,
    /// Lifetime cumulative amount received per source.
    CumulativeReceivedBySource(FundSource),
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
    /// Minimum liquidity that must remain in the treasury after a withdrawal.
    MinLiquidity,
    /// The token address managed by the treasury.
    Token,
    /// Proposal TTL in seconds (default 7 days). Configurable by admin.
    ProposalTtl,
}

#[contract]
pub struct CredenceTreasury;

fn zero_cumulative_amount() -> CumulativeAmount {
    CumulativeAmount {
        rollovers: 0,
        remainder: 0,
    }
}

fn add_to_cumulative(e: &Env, current: &CumulativeAmount, amount: i128) -> CumulativeAmount {
    let current_remainder = u128::try_from(current.remainder)
        .unwrap_or_else(|_| panic_with_error!(e, ContractError::Underflow));
    let addend = u128::try_from(amount)
        .unwrap_or_else(|_| panic_with_error!(e, ContractError::AmountMustBePositive));
    let sum = current_remainder + addend;
    let rollover_increment = if sum >= CUMULATIVE_SEGMENT {
        1_u64
    } else {
        0_u64
    };
    let remainder = if rollover_increment == 0 {
        sum
    } else {
        sum - CUMULATIVE_SEGMENT
    };

    CumulativeAmount {
        rollovers: current
            .rollovers
            .checked_add(rollover_increment)
            .unwrap_or_else(|| panic_with_error!(e, ContractError::Overflow)),
        remainder: i128::try_from(remainder)
            .unwrap_or_else(|_| panic_with_error!(e, ContractError::Overflow)),
    }
}

pub(crate) fn proportional_deduction(
    e: &Env,
    source_balance: i128,
    amount: i128,
    total: i128,
) -> i128 {
    if source_balance == 0 || amount == 0 {
        return 0;
    }
    if amount == total {
        return source_balance;
    }

    let source = U256::new(
        u128::try_from(source_balance)
            .unwrap_or_else(|_| panic_with_error!(e, ContractError::AmountMustBePositive)),
    );
    let withdrawal = U256::new(
        u128::try_from(amount)
            .unwrap_or_else(|_| panic_with_error!(e, ContractError::AmountMustBePositive)),
    );
    let available = U256::new(
        u128::try_from(total)
            .unwrap_or_else(|_| panic_with_error!(e, ContractError::AmountMustBePositive)),
    );
    let deduction = (source * withdrawal) / available;

    i128::try_from(deduction.as_u128())
        .unwrap_or_else(|_| panic_with_error!(e, ContractError::Overflow))
}

const STORAGE_TTL_EXTEND_TO: u32 = 31_536_000;

fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(STORAGE_TTL_EXTEND_TO / 2, STORAGE_TTL_EXTEND_TO);
}

#[contractimpl]
impl CredenceTreasury {
    /// Return the contract version.
    pub fn version(e: Env) -> String {
        String::from_str(&e, credence_errors::VERSION)
    }

    /// Initialize the treasury. Sets the admin; only admin can configure signers and depositors.
    ///
    /// @param e The contract environment
    /// @param admin Address that can add/remove signers, set threshold, and manage depositors
    pub fn initialize(e: Env, admin: Address, token: Address) {
        bump_instance_ttl(&e);
        admin.require_auth();
        e.storage().instance().set(&DataKey::Admin, &admin);
        e.storage().instance().set(&DataKey::Token, &token);
        e.storage().instance().set(&DataKey::Paused, &false);
        e.storage()
            .instance()
            .set(&DataKey::PauseSignerCount, &0_u32);
        e.storage().instance().set(&DataKey::PauseThreshold, &0_u32);
        e.storage()
            .instance()
            .set(&DataKey::PauseProposalCounter, &0_u64);
        e.storage().instance().set(&DataKey::TotalBalance, &0_i128);
        e.storage()
            .instance()
            .set(&DataKey::BalanceBySource(FundSource::ProtocolFee), &0_i128);
        e.storage()
            .instance()
            .set(&DataKey::BalanceBySource(FundSource::SlashedFunds), &0_i128);
        e.storage()
            .instance()
            .set(&DataKey::CumulativeReceived, &zero_cumulative_amount());
        e.storage().instance().set(
            &DataKey::CumulativeReceivedBySource(FundSource::ProtocolFee),
            &zero_cumulative_amount(),
        );
        e.storage().instance().set(
            &DataKey::CumulativeReceivedBySource(FundSource::SlashedFunds),
            &zero_cumulative_amount(),
        );
        e.storage().instance().set(&DataKey::SignerCount, &0_u32);
        e.storage().instance().set(&DataKey::Threshold, &0_u32);
        e.storage()
            .instance()
            .set(&DataKey::ProposalCounter, &0_u64);
        e.storage().instance().set(&DataKey::MinLiquidity, &0_i128);
        e.storage()
            .instance()
            .set(&DataKey::ProposalTtl, &DEFAULT_PROPOSAL_TTL);
        e.events()
            .publish((Symbol::new(&e, "treasury_initialized"),), admin);
    }

    /// Receive protocol fee or slashed funds report. Caller must be admin or an authorized depositor.
    ///
    /// # Important Design Notes
    /// This function records fee amounts reported by other contracts (e.g., credence_bond).
    /// The treasury itself does NOT hold tokens — it is purely an accounting system.  
    /// Actual token transfers occur at the bond contract level, where fee-on-transfer tokens
    /// are rejected via balance-delta verification.
    ///
    /// # Arguments
    /// * `from` - Caller (must be auth'd; typically admin or an authorized fee-collecting contract)
    /// * `amount` - Amount to credit (must be > 0)
    /// * `source` - Fund source classification (Protocol fee or slashed funds)
    ///
    /// # Panics
    /// * `AmountMustBePositive` if amount <= 0
    /// * `UnauthorizedDepositor` if caller is neither admin nor an authorized depositor
    /// * `Overflow` if adding the amount would overflow the balance
    pub fn receive_fee(e: Env, from: Address, amount: i128, source: FundSource) {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        from.require_auth();
        if amount <= 0 {
            panic_with_error!(&e, ContractError::AmountMustBePositive);
        }
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));
        let is_depositor = e
            .storage()
            .instance()
            .get(&DataKey::Depositor(from.clone()))
            .unwrap_or(false);
        if from != admin && !is_depositor {
            panic_with_error!(&e, ContractError::UnauthorizedDepositor);
        }
        let total: i128 = e
            .storage()
            .instance()
            .get(&DataKey::TotalBalance)
            .unwrap_or(0);
        let new_total = total
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
        let key_source = DataKey::BalanceBySource(source);
        let source_balance: i128 = e.storage().instance().get(&key_source).unwrap_or(0);
        let new_source = source_balance
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
        let cumulative_total: CumulativeAmount = e
            .storage()
            .instance()
            .get(&DataKey::CumulativeReceived)
            .unwrap_or_else(zero_cumulative_amount);
        let new_cumulative_total = add_to_cumulative(&e, &cumulative_total, amount);
        let cumulative_source: CumulativeAmount = e
            .storage()
            .instance()
            .get(&DataKey::CumulativeReceivedBySource(source))
            .unwrap_or_else(zero_cumulative_amount);
        let new_cumulative_source = add_to_cumulative(&e, &cumulative_source, amount);
        e.storage()
            .instance()
            .set(&DataKey::TotalBalance, &new_total);
        e.storage().instance().set(&key_source, &new_source);
        e.storage()
            .instance()
            .set(&DataKey::CumulativeReceived, &new_cumulative_total);
        e.storage().instance().set(
            &DataKey::CumulativeReceivedBySource(source),
            &new_cumulative_source,
        );

        // Perform actual token transfer into the treasury.
        let token_addr = Self::get_token(e.clone());
        let token_client = soroban_sdk::token::TokenClient::new(&e, &token_addr);
        let contract_addr = e.current_contract_address();
        token_client.transfer(&from, &contract_addr, &amount);

        e.events().publish(
            (Symbol::new(&e, "treasury_deposit"), from),
            (amount, source),
        );
    }

    /// Add an address that can deposit funds via receive_fee (e.g. bond contract).
    /// @param e The contract environment
    /// @param depositor Address to allow as depositor
    pub fn add_depositor(e: Env, depositor: Address) {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));
        admin.require_auth();
        e.storage()
            .instance()
            .set(&DataKey::Depositor(depositor.clone()), &true);
        e.events()
            .publish((Symbol::new(&e, "depositor_added"),), depositor);
    }

    /// Remove a depositor.
    pub fn remove_depositor(e: Env, depositor: Address) {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));
        admin.require_auth();
        e.storage()
            .instance()
            .remove(&DataKey::Depositor(depositor.clone()));
        e.events()
            .publish((Symbol::new(&e, "depositor_removed"),), depositor);
    }

    /// Add a signer for multi-sig withdrawals. Threshold must be <= signer count after add.
    pub fn add_signer(e: Env, signer: Address) {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));
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
        let new_count = count
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
        e.storage()
            .instance()
            .set(&DataKey::SignerCount, &new_count);
        e.events()
            .publish((Symbol::new(&e, "signer_added"),), signer);
    }

    /// Remove a signer. Threshold is auto-capped to new signer count if needed.
    pub fn remove_signer(e: Env, signer: Address) {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));
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
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        let admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));
        admin.require_auth();
        let count: u32 = e
            .storage()
            .instance()
            .get(&DataKey::SignerCount)
            .unwrap_or(0);
        if threshold > count {
            panic_with_error!(&e, ContractError::ThresholdExceedsSigners);
        }
        let old_threshold: u32 = e.storage().instance().get(&DataKey::Threshold).unwrap_or(0);
        e.storage().instance().set(&DataKey::Threshold, &threshold);
        // Emit old and new values for auditability
        e.events().publish(
            (Symbol::new(&e, "threshold_updated"),),
            (old_threshold, threshold),
        );
    }

    /// Propose a withdrawal. Only a signer can propose. Creates a proposal that can be approved and executed.
    /// @return proposal_id The id of the new proposal
    pub fn propose_withdrawal(e: Env, proposer: Address, recipient: Address, amount: i128) -> u64 {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        proposer.require_auth();
        let is_signer = e
            .storage()
            .instance()
            .get(&DataKey::Signer(proposer.clone()))
            .unwrap_or(false);
        if !is_signer {
            panic_with_error!(&e, ContractError::NotSigner);
        }
        if amount <= 0 {
            panic_with_error!(&e, ContractError::AmountMustBePositive);
        }
        let total: i128 = e
            .storage()
            .instance()
            .get(&DataKey::TotalBalance)
            .unwrap_or(0);
        if amount > total {
            panic_with_error!(&e, ContractError::InsufficientTreasuryBalance);
        }
        let id: u64 = e
            .storage()
            .instance()
            .get(&DataKey::ProposalCounter)
            .unwrap_or(0);
        let next_id = id
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
        e.storage()
            .instance()
            .set(&DataKey::ProposalCounter, &next_id);
        let proposed_at = e.ledger().timestamp();
        let ttl: u64 = e
            .storage()
            .instance()
            .get(&DataKey::ProposalTtl)
            .unwrap_or(DEFAULT_PROPOSAL_TTL);
        let expires_at = if ttl == 0 {
            u64::MAX
        } else {
            proposed_at
                .checked_add(ttl)
                .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow))
        };
        let proposal = WithdrawalProposal {
            recipient: recipient.clone(),
            amount,
            proposed_at,
            expires_at,
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
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        approver.require_auth();
        let is_signer = e
            .storage()
            .instance()
            .get(&DataKey::Signer(approver.clone()))
            .unwrap_or(false);
        if !is_signer {
            panic_with_error!(&e, ContractError::NotSigner);
        }
        let proposal: WithdrawalProposal = e
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::ProposalNotFound));
        if proposal.executed {
            panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
        }
        if e.ledger().timestamp() >= proposal.expires_at {
            e.events().publish(
                (Symbol::new(&e, "treasury_proposal_expired"), proposal_id),
                (),
            );
            panic_with_error!(&e, ContractError::ProposalExpired);
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
        let new_count = count
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Overflow));
        e.storage()
            .instance()
            .set(&DataKey::ApprovalCount(proposal_id), &new_count);
        e.events().publish(
            (Symbol::new(&e, "treasury_withdrawal_approved"), proposal_id),
            (approver,),
        );
    }

    /// Execute a withdrawal proposal. Callable by anyone once approval count >= threshold.
    ///
    /// This function marks a proposal as executed and updates the internal balance tracking.
    /// The actual token transfer is caller's responsibility (use the proposal details to arrange
    /// transfer externally or via callback contract).
    ///
    /// # Arguments
    /// * `proposal_id`   - ID of the approved withdrawal proposal.
    /// * `min_amount_out` - Caller-provided minimum acceptable settlement amount.
    ///                      Reverts with `SlippageExceeded` when the realized
    ///                      `actual_amount` is less than this value, protecting the
    ///                      caller against unfavorable price movement between proposal
    ///                      creation and execution.  Pass `0` to skip the check.
    ///
    /// # Failure modes
    /// This path distinguishes two failures that previously shared one code:
    /// * `InsufficientTreasuryBalance` - the treasury lacks funds or the
    ///   withdrawal would breach the `MinLiquidity` floor (operator must top up).
    /// * `SlippageExceeded` - the treasury had funds but the settled amount fell
    ///   below `min_amount_out` (caller should retry with a looser bound).
    ///
    /// # Events
    /// Emits `treasury_withdrawal_executed` with `(recipient, expected, actual)` so
    /// off-chain observers can detect any discrepancy between the proposed and settled
    /// amounts.
    pub fn execute_withdrawal(e: Env, proposal_id: u64, min_amount_out: i128) {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        let mut proposal: WithdrawalProposal = e
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::ProposalNotFound));
        if e.ledger().timestamp() >= proposal.expires_at {
            e.events().publish(
                (Symbol::new(&e, "treasury_proposal_expired"), proposal_id),
                (),
            );
            panic_with_error!(&e, ContractError::ProposalExpired);
        }
        if proposal.executed {
            panic_with_error!(&e, ContractError::ProposalAlreadyExecuted);
        }
        let threshold: u32 = e.storage().instance().get(&DataKey::Threshold).unwrap_or(0);
        let approvals: u32 = e
            .storage()
            .instance()
            .get(&DataKey::ApprovalCount(proposal_id))
            .unwrap_or(0);
        if approvals < threshold {
            panic_with_error!(&e, ContractError::InsufficientApprovals);
        }
        let total: i128 = e
            .storage()
            .instance()
            .get(&DataKey::TotalBalance)
            .unwrap_or(0);
        if total < proposal.amount {
            panic_with_error!(&e, ContractError::InsufficientTreasuryBalance);
        }

        // Liquidity guard: Ensure remaining balance doesn't breach the minimum floor.
        let min_liquidity: i128 = e
            .storage()
            .instance()
            .get(&DataKey::MinLiquidity)
            .unwrap_or(0);
        let remaining = total
            .checked_sub(proposal.amount)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Underflow));
        if remaining < min_liquidity {
            panic_with_error!(&e, ContractError::InsufficientTreasuryBalance);
        }

        // Perform actual token transfer.
        let token_addr = Self::get_token(e.clone());
        let token_client = soroban_sdk::token::TokenClient::new(&e, &token_addr);
        let contract_addr = e.current_contract_address();

        let recipient_balance_before = token_client.balance(&proposal.recipient);
        token_client.transfer(&contract_addr, &proposal.recipient, &proposal.amount);
        let recipient_balance_after = token_client.balance(&proposal.recipient);

        let actual_amount = recipient_balance_after
            .checked_sub(recipient_balance_before)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Underflow));

        // Slippage guard: revert if the settled amount falls below the caller's
        // threshold. This is a distinct failure mode from a balance/liquidity
        // shortfall (which raises `InsufficientTreasuryBalance` above): here the
        // treasury had funds but the realized amount tripped the caller's bound,
        // so callers and indexers must be able to tell the two apart.
        if actual_amount < min_amount_out {
            panic_with_error!(&e, ContractError::SlippageExceeded);
        }

        let new_total = total
            .checked_sub(actual_amount)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Underflow));
        let protocol_balance: i128 = e
            .storage()
            .instance()
            .get(&DataKey::BalanceBySource(FundSource::ProtocolFee))
            .unwrap_or(0);
        let slashed_balance: i128 = e
            .storage()
            .instance()
            .get(&DataKey::BalanceBySource(FundSource::SlashedFunds))
            .unwrap_or(0);
        let protocol_deduction = proportional_deduction(&e, protocol_balance, actual_amount, total);
        let slashed_deduction = actual_amount
            .checked_sub(protocol_deduction)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Underflow));
        let new_protocol_balance = protocol_balance
            .checked_sub(protocol_deduction)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Underflow));
        let new_slashed_balance = slashed_balance
            .checked_sub(slashed_deduction)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Underflow));
        e.storage()
            .instance()
            .set(&DataKey::TotalBalance, &new_total);
        e.storage().instance().set(
            &DataKey::BalanceBySource(FundSource::ProtocolFee),
            &new_protocol_balance,
        );
        e.storage().instance().set(
            &DataKey::BalanceBySource(FundSource::SlashedFunds),
            &new_slashed_balance,
        );
        proposal.executed = true;
        e.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        // Emit (recipient, min_amount_out, actual_amount) so observers can verify settlement.
        e.events().publish(
            (Symbol::new(&e, "treasury_withdrawal_executed"), proposal_id),
            (proposal.recipient.clone(), min_amount_out, actual_amount),
        );
    }

    /// Returns the configured token address.
    pub fn get_token(e: Env) -> Address {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized))
    }

    /// Update the token address. Only admin can call.
    pub fn set_token(e: Env, admin: Address, token: Address) {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        let stored_admin = Self::get_admin(e.clone());
        if admin != stored_admin {
            panic_with_error!(&e, ContractError::NotAdmin);
        }
        admin.require_auth();
        e.storage().instance().set(&DataKey::Token, &token);
        e.events()
            .publish((Symbol::new(&e, "token_updated"),), token);
    }

    /// Set the minimum liquidity floor. Only admin can call.
    pub fn set_min_liquidity(e: Env, admin: Address, min_liquidity: i128) {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        let stored_admin = Self::get_admin(e.clone());
        if admin != stored_admin {
            panic_with_error!(&e, ContractError::NotAdmin);
        }
        admin.require_auth();

        e.storage()
            .instance()
            .set(&DataKey::MinLiquidity, &min_liquidity);
        e.events()
            .publish((Symbol::new(&e, "min_liquidity_updated"),), min_liquidity);
    }

    /// Set the proposal TTL (time-to-live) in ledger seconds. Only admin can call.
    /// Proposals expire `ttl` seconds after they are proposed, after which
    /// approvals and execution are rejected.
    /// Pass `0` for no expiry (legacy behaviour).
    pub fn set_proposal_ttl(e: Env, admin: Address, ttl: u64) {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        let stored_admin = Self::get_admin(e.clone());
        if admin != stored_admin {
            panic_with_error!(&e, ContractError::NotAdmin);
        }
        admin.require_auth();
        e.storage().instance().set(&DataKey::ProposalTtl, &ttl);
        e.events()
            .publish((Symbol::new(&e, "proposal_ttl_updated"),), ttl);
    }

    /// Get the current proposal TTL in ledger seconds.
    pub fn get_proposal_ttl(e: Env) -> u64 {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::ProposalTtl)
            .unwrap_or(DEFAULT_PROPOSAL_TTL)
    }

    /// Get current minimum liquidity floor.
    pub fn get_min_liquidity(e: Env) -> i128 {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::MinLiquidity)
            .unwrap_or(0)
    }

    /// Get total treasury balance.
    pub fn get_balance(e: Env) -> i128 {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::TotalBalance)
            .unwrap_or(0)
    }

    /// Get the currently available balance attributed to a fund source.
    pub fn get_balance_by_source(e: Env, source: FundSource) -> i128 {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::BalanceBySource(source))
            .unwrap_or(0)
    }

    /// Get the lifetime cumulative amount received across all sources.
    pub fn get_cumulative_received(e: Env) -> CumulativeAmount {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::CumulativeReceived)
            .unwrap_or_else(zero_cumulative_amount)
    }

    /// Get the lifetime cumulative amount received for a specific source.
    pub fn get_cumulative_by_source(e: Env, source: FundSource) -> CumulativeAmount {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::CumulativeReceivedBySource(source))
            .unwrap_or_else(zero_cumulative_amount)
    }

    /// Get admin address.
    pub fn get_admin(e: Env) -> Address {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized))
    }

    /// Check if an address is an authorized depositor.
    pub fn is_depositor(e: Env, address: Address) -> bool {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::Depositor(address))
            .unwrap_or(false)
    }

    /// Check if an address is a signer.
    pub fn is_signer(e: Env, address: Address) -> bool {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::Signer(address))
            .unwrap_or(false)
    }

    /// Get current approval threshold.
    pub fn get_threshold(e: Env) -> u32 {
        bump_instance_ttl(&e);
        e.storage().instance().get(&DataKey::Threshold).unwrap_or(0)
    }

    /// Get a withdrawal proposal by id.
    pub fn get_proposal(e: Env, proposal_id: u64) -> WithdrawalProposal {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::ProposalNotFound))
    }

    /// Get approval count for a proposal.
    pub fn get_approval_count(e: Env, proposal_id: u64) -> u32 {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::ApprovalCount(proposal_id))
            .unwrap_or(0)
    }

    /// Check if a signer has approved a proposal.
    pub fn has_approved(e: Env, proposal_id: u64, signer: Address) -> bool {
        bump_instance_ttl(&e);
        e.storage()
            .instance()
            .get(&DataKey::Approval(proposal_id, signer))
            .unwrap_or(false)
    }

    pub fn pause(e: Env, caller: Address) -> Option<u64> {
        bump_instance_ttl(&e);
        pausable::pause(&e, &caller)
    }

    pub fn unpause(e: Env, caller: Address) -> Option<u64> {
        bump_instance_ttl(&e);
        pausable::unpause(&e, &caller)
    }

    pub fn is_paused(e: Env) -> bool {
        bump_instance_ttl(&e);
        pausable::is_paused(&e)
    }

    pub fn set_pause_signer(e: Env, admin: Address, signer: Address, enabled: bool) {
        bump_instance_ttl(&e);
        pausable::set_pause_signer(&e, &admin, &signer, enabled)
    }

    pub fn set_pause_threshold(e: Env, admin: Address, threshold: u32) {
        bump_instance_ttl(&e);
        pausable::set_pause_threshold(&e, &admin, threshold)
    }

    pub fn approve_pause_proposal(e: Env, signer: Address, proposal_id: u64) {
        bump_instance_ttl(&e);
        pausable::approve_pause_proposal(&e, &signer, proposal_id)
    }

    /// Execute a pause proposal.
    pub fn execute_pause_proposal(e: Env, proposal_id: u64) {
        bump_instance_ttl(&e);
        pausable::execute_pause_proposal(&e, proposal_id)
    }

    /// Rescue excess native tokens from the contract.
    ///
    /// Only callable by admin. Transfers only the *excess* balance — the difference
    /// between the contract's actual token balance and the internally accounted
    /// `TotalBalance` — so user/protocol funds cannot be drained.
    ///
    /// # Excess-only bound
    /// ```text
    /// excess = token_client.balance(contract) - TotalBalance
    /// ```
    /// `amount` must satisfy `0 < amount <= excess`. Any attempt to rescue more than
    /// the excess reverts with `InsufficientTreasuryBalance`.
    pub fn rescue_native(e: Env, admin: Address, to: Address, amount: i128) {
        bump_instance_ttl(&e);
        pausable::require_not_paused(&e);
        admin.require_auth();

        let stored_admin: Address = e
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::NotInitialized));
        if stored_admin != admin {
            panic_with_error!(&e, ContractError::NotAdmin);
        }

        if amount <= 0 {
            panic_with_error!(&e, ContractError::AmountMustBePositive);
        }

        let token_addr = Self::get_token(e.clone());
        let token_client = soroban_sdk::token::TokenClient::new(&e, &token_addr);
        let contract_addr = e.current_contract_address();

        let actual_balance = token_client.balance(&contract_addr);
        let total_accounted: i128 = e
            .storage()
            .instance()
            .get(&DataKey::TotalBalance)
            .unwrap_or(0);

        // Excess = tokens held by the contract beyond what is accounted for.
        let excess = actual_balance
            .checked_sub(total_accounted)
            .unwrap_or_else(|| panic_with_error!(&e, ContractError::Underflow));

        if amount > excess {
            panic_with_error!(&e, ContractError::InsufficientTreasuryBalance);
        }

        token_client.transfer(&contract_addr, &to, &amount);

        e.events()
            .publish((Symbol::new(&e, "native_rescued"),), (to, amount, admin));
    }
}
