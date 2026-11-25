// Off-chain CLI binary — issue #713 silences dynamic-string macros in
// ON-CHAIN contract code only. Production contract wasm cannot depend on
// `format!` for event topics or reverts, but an admin CLI printing JSON
// status text is allowed and uses format!() for diagnostics.

mod rpc;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use rpc::{LiveRpc, SorobanRpc};
use serde_json::json;
use soroban_client::{
    account::{Account, AccountBehavior},
    transaction::TransactionBehavior,
    transaction_builder::{TransactionBuilder, TransactionBuilderBehavior},
};
use std::str::FromStr;
use stellar_baselib::{
    address::{Address, AddressTrait},
    contract::{ContractBehavior, Contracts},
    keypair::{Keypair, KeypairBehavior},
    xdr::{Limits, ScBytes, ScString, ScSymbol, ScVal, ScVec, WriteXdr},
};
use stellar_strkey::ed25519::PrivateKey;

/// Admin CLI for Credence protocol contracts.
///
/// Builds real `InvokeHostFunction` (invoke_contract) transactions for each
/// admin operation. Without --submit the XDR envelope is printed as a
/// structured JSON dry-run. With --submit the transaction is signed with the
/// key from --signer (or the CREDENCE_SIGNER env-var) and sent to the RPC.
#[derive(Debug, Parser)]
#[command(
    name = "credence-admin",
    author,
    version,
    about = "Admin CLI for Credence protocol"
)]
struct Cli {
    /// Soroban RPC endpoint.
    #[arg(
        long,
        env = "CREDENCE_RPC_URL",
        default_value = "https://soroban-testnet.stellar.org"
    )]
    rpc_url: String,

    /// Network passphrase. Defaults to testnet.
    #[arg(
        long,
        env = "CREDENCE_NETWORK",
        default_value = "Test SDF Network ; September 2015"
    )]
    network: String,

    /// Contract address (C…) to invoke.
    #[arg(long, env = "CREDENCE_CONTRACT")]
    contract: Option<String>,

    /// Signer secret key (S…). Required for --submit. Can also be set via
    /// the CREDENCE_SIGNER environment variable.
    #[arg(long, env = "CREDENCE_SIGNER")]
    signer: Option<String>,

    /// Submit the transaction to the network instead of a dry-run.
    #[arg(long, action = clap::ArgAction::SetTrue, default_value = "false")]
    submit: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
/// Maximum basis-points value accepted on-chain (100%).
const MAX_BPS: i64 = 10_000;
/// Maximum attestation weight cap accepted on-chain (`MAX_ATTESTATION_WEIGHT`).
const MAX_WEIGHT: i64 = 1_000_000;

#[derive(Subcommand)]
enum Commands {
    /// Set early-exit penalty configuration on a credence_bond contract.
    ///
    /// Maps to: set_early_exit_config(admin, treasury, penalty_bps)
    BondSetEarlyExitConfig {
        /// Admin Stellar address (G…).
        #[arg(long)]
        admin: String,
        /// Treasury Stellar address (G…) that receives penalty funds.
        #[arg(long)]
        treasury: String,
        /// Penalty in basis points (0–10 000).
        #[arg(long, value_parser = clap::value_parser!(u32).range(0..=MAX_BPS))]
        bps: u32,
    },

    /// Set weight configuration on a credence_bond contract.
    ///
    /// Maps to: set_weight_config(admin, multiplier_bps, max_weight)
    BondSetWeights {
        /// Admin Stellar address (G…).
        #[arg(long)]
        admin: String,
        /// Multiplier in basis points (0–10 000).
        #[arg(long, value_parser = clap::value_parser!(u32).range(0..=MAX_BPS))]
        multiplier_bps: u32,
        /// Maximum attestation weight cap (0–1 000 000).
        #[arg(long, value_parser = clap::value_parser!(u32).range(0..=MAX_WEIGHT))]
        max_weight: u32,
    },

    /// Set pause signer on a credence_delegation contract.
    ///
    /// Maps to: set_pause_signer(admin, signer, enabled)
    DelegationSetPauseSigner {
        /// Admin Stellar address (G…).
        #[arg(long)]
        admin: String,
        /// Pause-signer Stellar address (G…).
        #[arg(long)]
        pause_signer: String,
        /// Whether to enable (true) or disable (false) the signer (default: true).
        #[arg(long, default_value = "true", num_args = 0..=1, default_missing_value = "true")]
        enabled: bool,
    },

    /// Queue a call-bound operation on a timelock contract.
    ///
    /// Maps to: queue_call(proposer, target, func, args, salt, delay)
    TimelockQueueCall {
        /// Proposer (admin) Stellar address (G…).
        #[arg(long)]
        proposer: String,
        /// Target contract address (C…) that execute_operation will invoke.
        #[arg(long)]
        target: String,
        /// Target function name (≤32 characters).
        #[arg(long)]
        func: String,
        /// Typed call arguments, e.g. --arg address:G… --arg u32:7.
        /// Repeatable; order is preserved.
        #[arg(long = "arg")]
        args: Vec<String>,
        /// 32-byte salt as 64 hex characters.
        #[arg(long)]
        salt: String,
        /// Minimum delay in seconds before execution.
        #[arg(long)]
        delay: u64,
    },

    /// Execute a queued timelock operation (optionally after its ETA).
    ///
    /// Maps to: execute_operation(op_id)
    TimelockExecute {
        /// Operation id returned by queue_call.
        #[arg(long)]
        op_id: u64,
    },

    /// Cancel a pending timelock operation.
    ///
    /// Maps to: cancel_operation(admin, op_id)
    TimelockCancel {
        /// Admin Stellar address (G…).
        #[arg(long)]
        admin: String,
        /// Operation id to cancel.
        #[arg(long)]
        op_id: u64,
    },

    /// Register or update an arbitrator and its voting weight.
    ///
    /// Maps to: register_arbitrator(arbitrator, weight)
    ArbitrationRegisterArbitrator {
        /// Arbitrator Stellar address (G…).
        #[arg(long)]
        arbitrator: String,
        /// Positive voting weight (i128).
        #[arg(long, allow_hyphen_values = true)]
        weight: i128,
    },

    /// Remove an arbitrator from the registry.
    ///
    /// Maps to: unregister_arbitrator(arbitrator)
    ArbitrationUnregisterArbitrator {
        /// Arbitrator Stellar address (G…).
        #[arg(long)]
        arbitrator: String,
    },

    /// Set dispute resolution quorum requirements.
    ///
    /// Maps to: set_quorum(admin, min_total_weight, min_voters)
    ArbitrationSetQuorum {
        /// Admin Stellar address (G…).
        #[arg(long)]
        admin: String,
        /// Minimum total vote weight (i128; 0 disables the weight gate).
        #[arg(long, allow_hyphen_values = true)]
        min_total_weight: i128,
        /// Minimum number of distinct voters.
        #[arg(long)]
        min_voters: u32,
    },

    /// Pause an arbitration contract (admin or pause signer).
    ///
    /// Maps to: pause(caller)
    ArbitrationPause {
        /// Authorized caller Stellar address (G…).
        #[arg(long)]
        caller: String,
    },
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn resolve_contract_id(cli: &Cli) -> Result<&str> {
    if let Some(contract) = cli.contract.as_deref() {
        Ok(contract)
    } else if cli.submit {
        Err(anyhow!(
            "--contract / CREDENCE_CONTRACT is required with --submit"
        ))
    } else {
        eprintln!("warning: --contract not set; using zero-address placeholder");
        Ok("CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4")
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let contract_id = resolve_contract_id(&cli)?;

    match &cli.command {
        Commands::BondSetEarlyExitConfig {
            admin,
            treasury,
            bps,
        } => {
            let args = build_early_exit_args(admin, treasury, *bps)?;
            run(&cli, contract_id, "set_early_exit_config", args)
        }
        Commands::BondSetWeights {
            admin,
            multiplier_bps,
            max_weight,
        } => {
            let args = build_weight_args(admin, *multiplier_bps, *max_weight)?;
            run(&cli, contract_id, "set_weight_config", args)
        }
        Commands::DelegationSetPauseSigner {
            admin,
            pause_signer,
            enabled,
        } => {
            let args = build_pause_signer_args(admin, pause_signer, *enabled)?;
            run(&cli, contract_id, "set_pause_signer", args)
        }
        Commands::TimelockQueueCall {
            proposer,
            target,
            func,
            args: arg_specs,
            salt,
            delay,
        } => {
            let args = build_queue_call_args(proposer, target, func, arg_specs, salt, *delay)?;
            run(&cli, contract_id, "queue_call", args)
        }
        Commands::TimelockExecute { op_id } => {
            let args = build_op_id_args(*op_id)?;
            run(&cli, contract_id, "execute_operation", args)
        }
        Commands::TimelockCancel { admin, op_id } => {
            let args = build_cancel_operation_args(admin, *op_id)?;
            run(&cli, contract_id, "cancel_operation", args)
        }
        Commands::ArbitrationRegisterArbitrator { arbitrator, weight } => {
            let args = build_register_arbitrator_args(arbitrator, *weight)?;
            run(&cli, contract_id, "register_arbitrator", args)
        }
        Commands::ArbitrationUnregisterArbitrator { arbitrator } => {
            let args = build_unregister_arbitrator_args(arbitrator)?;
            run(&cli, contract_id, "unregister_arbitrator", args)
        }
        Commands::ArbitrationSetQuorum {
            admin,
            min_total_weight,
            min_voters,
        } => {
            let args = build_set_quorum_args(admin, *min_total_weight, *min_voters)?;
            run(&cli, contract_id, "set_quorum", args)
        }
        Commands::ArbitrationPause { caller } => {
            let args = build_caller_args(caller)?;
            run(&cli, contract_id, "pause", args)
        }
    }
}

// ---------------------------------------------------------------------------
// Argument builders
// ---------------------------------------------------------------------------

/// Encode args for `set_early_exit_config(admin: Address, treasury: Address, penalty_bps: u32)`.
fn build_early_exit_args(admin: &str, treasury: &str, bps: u32) -> Result<Vec<ScVal>> {
    Ok(vec![
        addr_to_sc_val(admin)?,
        addr_to_sc_val(treasury)?,
        ScVal::U32(bps),
    ])
}

/// Encode args for `set_weight_config(admin: Address, multiplier_bps: u32, max_weight: u32)`.
fn build_weight_args(admin: &str, multiplier_bps: u32, max_weight: u32) -> Result<Vec<ScVal>> {
    Ok(vec![
        addr_to_sc_val(admin)?,
        ScVal::U32(multiplier_bps),
        ScVal::U32(max_weight),
    ])
}

/// Encode args for `set_pause_signer(admin: Address, signer: Address, enabled: bool)`.
fn build_pause_signer_args(admin: &str, signer: &str, enabled: bool) -> Result<Vec<ScVal>> {
    Ok(vec![
        addr_to_sc_val(admin)?,
        addr_to_sc_val(signer)?,
        ScVal::Bool(enabled),
    ])
}

/// Encode args for `queue_call(proposer, target, func, args, salt, delay)`.
///
/// `args` is encoded as a Soroban `Vec<Val>` and `salt` as `BytesN<32>`.
fn build_queue_call_args(
    proposer: &str,
    target: &str,
    func: &str,
    arg_specs: &[String],
    salt: &str,
    delay: u64,
) -> Result<Vec<ScVal>> {
    let mut encoded_args = Vec::with_capacity(arg_specs.len());
    for spec in arg_specs {
        encoded_args.push(parse_arg_spec(spec)?);
    }

    Ok(vec![
        addr_to_sc_val(proposer)?,
        addr_to_sc_val(target)?,
        symbol_sc_val(func, "function name")?,
        ScVal::Vec(Some(ScVec::try_from(encoded_args)?)),
        bytes32_sc_val(salt, "salt")?,
        ScVal::U64(delay),
    ])
}

/// Encode args for `execute_operation(op_id: u64)`.
fn build_op_id_args(op_id: u64) -> Result<Vec<ScVal>> {
    Ok(vec![ScVal::U64(op_id)])
}

/// Encode args for `cancel_operation(admin: Address, op_id: u64)`.
fn build_cancel_operation_args(admin: &str, op_id: u64) -> Result<Vec<ScVal>> {
    Ok(vec![addr_to_sc_val(admin)?, ScVal::U64(op_id)])
}

/// Encode args for `register_arbitrator(arbitrator: Address, weight: i128)`.
fn build_register_arbitrator_args(arbitrator: &str, weight: i128) -> Result<Vec<ScVal>> {
    Ok(vec![addr_to_sc_val(arbitrator)?, ScVal::from(weight)])
}

/// Encode args for `unregister_arbitrator(arbitrator: Address)`.
fn build_unregister_arbitrator_args(arbitrator: &str) -> Result<Vec<ScVal>> {
    Ok(vec![addr_to_sc_val(arbitrator)?])
}

/// Encode args for `set_quorum(admin: Address, min_total_weight: i128, min_voters: u32)`.
fn build_set_quorum_args(
    admin: &str,
    min_total_weight: i128,
    min_voters: u32,
) -> Result<Vec<ScVal>> {
    Ok(vec![
        addr_to_sc_val(admin)?,
        ScVal::from(min_total_weight),
        ScVal::U32(min_voters),
    ])
}

/// Encode args for `pause(caller: Address)`.
fn build_caller_args(caller: &str) -> Result<Vec<ScVal>> {
    Ok(vec![addr_to_sc_val(caller)?])
}

/// Parse a typed `--arg TYPE:VALUE` specification into an `ScVal`.
///
/// Supported types: `address`, `bool`, `bytes`, `bytes32`, `i32`, `i64`,
/// `i128`, `string`, `symbol`, `u32`, `u64`, `void`, and `vec:[spec,...]`.
fn parse_arg_spec(spec: &str) -> Result<ScVal> {
    let (kind, value) = spec
        .split_once(':')
        .ok_or_else(|| anyhow!("invalid --arg {spec:?}: expected TYPE:VALUE (e.g. address:G…)"))?;

    match kind {
        "address" => addr_to_sc_val(value),
        "bool" => match value {
            "true" => Ok(ScVal::Bool(true)),
            "false" => Ok(ScVal::Bool(false)),
            _ => Err(anyhow!("invalid --arg bool value {value:?}: expected true/false")),
        },
        "bytes" => Ok(ScVal::Bytes(ScBytes::try_from(hex::decode(value)?)?)),
        "bytes32" => bytes32_sc_val(value, "--arg bytes32"),
        "i32" => Ok(ScVal::I32(value.parse()?)),
        "i64" => Ok(ScVal::I64(value.parse()?)),
        "i128" => Ok(ScVal::from(value.parse::<i128>()?)),
        "string" => Ok(ScVal::String(ScString::try_from(value.as_bytes().to_vec())?)),
        "symbol" => symbol_sc_val(value, "--arg symbol"),
        "u32" => Ok(ScVal::U32(value.parse()?)),
        "u64" => Ok(ScVal::U64(value.parse()?)),
        "void" => Ok(ScVal::Void),
        "vec" => {
            let inner = value
                .strip_prefix('[')
                .and_then(|v| v.strip_suffix(']'))
                .ok_or_else(|| anyhow!("invalid --arg vec {spec:?}: expected vec:[spec,...]"))?;
            let values = if inner.trim().is_empty() {
                Vec::new()
            } else {
                inner.split(',').map(parse_arg_spec).collect::<Result<Vec<_>>>()?
            };
            Ok(ScVal::Vec(Some(ScVec::try_from(values)?)))
        }
        _ => Err(anyhow!(
            "unsupported --arg type {kind:?}; expected address, bool, bytes, bytes32, i32, i64, i128, string, symbol, u32, u64, void or vec"
        )),
    }
}

/// Encode a Soroban symbol, rejecting names the host cannot decode.
fn symbol_sc_val(name: &str, label: &str) -> Result<ScVal> {
    let bytes = name.as_bytes();
    if bytes.is_empty()
        || bytes.len() > 32
        || !bytes[0].is_ascii_alphabetic() && bytes[0] != b'_'
        || !bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        return Err(anyhow!(
            "invalid {label} {name:?}: expected 1–32 characters matching [A-Za-z_][A-Za-z0-9_]*"
        ));
    }
    Ok(ScVal::Symbol(ScSymbol::try_from(bytes.to_vec())?))
}

/// Encode a 32-byte value as `ScVal::Bytes` (`BytesN<32>` on-chain).
fn bytes32_sc_val(value: &str, label: &str) -> Result<ScVal> {
    let bytes = hex::decode(value)
        .map_err(|e| anyhow!("invalid {label} {value:?}: expected hex bytes: {e}"))?;
    if bytes.len() != 32 {
        return Err(anyhow!(
            "invalid {label}: expected 32 bytes (64 hex chars), got {}",
            bytes.len()
        ));
    }
    Ok(ScVal::Bytes(ScBytes::try_from(bytes)?))
}

/// Convert a Stellar address string (G… or C…) to an `ScVal::Address`.
fn addr_to_sc_val(addr: &str) -> Result<ScVal> {
    let address = Address::new(addr).map_err(|e| anyhow!("invalid address {addr:?}: {e}"))?;
    address
        .to_sc_val()
        .map_err(|e| anyhow!("failed to convert address {addr:?} to ScVal: {e}"))
}

/// Decode a Stellar secret key without relying on helpers that panic on bad input.
fn signer_keypair(secret: &str) -> Result<Keypair> {
    let private = PrivateKey::from_str(secret).map_err(|e| anyhow!("invalid signer key: {e}"))?;
    Keypair::from_raw_ed25519_seed(&private.0).map_err(|e| anyhow!("invalid signer key: {e}"))
}

// ---------------------------------------------------------------------------
// Core transaction builder / runner
// ---------------------------------------------------------------------------

/// Build an `InvokeHostFunction` transaction, then either print a dry-run
/// JSON report or sign-and-submit it to the network.
fn run(cli: &Cli, contract_id: &str, function: &str, args: Vec<ScVal>) -> Result<()> {
    let output = if cli.submit {
        if cli.contract.is_none() {
            return Err(anyhow!(
                "--contract / CREDENCE_CONTRACT is required with --submit"
            ));
        }
        // Validate contract address and signer key before opening an RPC connection.
        let _ = Contracts::new(contract_id)
            .map_err(|e| anyhow!("invalid contract address {contract_id:?}: {e}"))?;
        let secret = cli
            .signer
            .as_deref()
            .ok_or_else(|| anyhow!("--signer / CREDENCE_SIGNER is required with --submit"))?;
        let _ = signer_keypair(secret)?;

        let rpc = LiveRpc::new(&cli.rpc_url)?;
        run_with_rpc(cli, contract_id, function, args, Some(&rpc))?
    } else {
        run_with_rpc(cli, contract_id, function, args, None)?
    };
    println!("{output}");
    Ok(())
}

/// Core transaction builder / runner with an injectable [`SorobanRpc`] client
/// so `--submit` can be tested offline without network access.
fn run_with_rpc(
    cli: &Cli,
    contract_id: &str,
    function: &str,
    args: Vec<ScVal>,
    rpc: Option<&dyn SorobanRpc>,
) -> Result<String> {
    if cli.submit && cli.contract.is_none() {
        return Err(anyhow!(
            "--contract / CREDENCE_CONTRACT is required with --submit"
        ));
    }

    // Build the XDR operation via stellar-baselib's Contracts helper.
    let contract = Contracts::new(contract_id)
        .map_err(|e| anyhow!("invalid contract address {contract_id:?}: {e}"))?;
    let operation = contract.call(function, Some(args));

    // Resolve the signer key (required only when submitting).
    let keypair: Option<Keypair> = if cli.submit {
        let secret = cli
            .signer
            .as_deref()
            .ok_or_else(|| anyhow!("--signer / CREDENCE_SIGNER is required with --submit"))?;
        Some(signer_keypair(secret)?)
    } else {
        None
    };

    // Use the signer public key as the source account, or a dummy for dry-runs.
    let source_pub = keypair
        .as_ref()
        .map(|kp| kp.public_key())
        .unwrap_or_else(|| "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF".to_string());

    if cli.submit {
        // --- Live path: fetch account, build, prepare, sign, submit ---------
        let rpc = rpc.ok_or_else(|| anyhow!("RPC client is required with --submit"))?;

        let mut source_account = rpc.get_account(&source_pub)?;

        let mut builder = TransactionBuilder::new(&mut source_account, &cli.network, None);
        builder
            .fee(1_000_000_u32)
            .set_timeout(30)
            .map_err(|e| anyhow!(e))?;
        builder.add_operation(operation);
        let tx = builder.build();

        // Prepare (simulate + assemble footprint + resource fee).
        let tx = rpc.prepare_transaction(&tx)?;

        // Sign.
        let kp = keypair.unwrap();
        let mut signed_tx = tx;
        signed_tx.sign(&[kp]);

        // Submit.
        let resp = rpc.send_transaction(signed_tx)?;
        if resp.status.eq_ignore_ascii_case("ERROR") {
            return Err(anyhow!(
                "transaction rejected by RPC (status: {}, hash: {})",
                resp.status,
                resp.hash
            ));
        }

        let out = json!({
            "status": resp.status,
            "hash": resp.hash,
        });
        Ok(serde_json::to_string_pretty(&out)?)
    } else {
        // --- Dry-run path: build with dummy sequence, emit XDR JSON ---------
        let mut dummy_account =
            Account::new(&source_pub, "0").map_err(|e| anyhow!("account error: {e:?}"))?;

        let mut builder = TransactionBuilder::new(&mut dummy_account, &cli.network, None);
        builder
            .fee(1_000_000_u32)
            .set_timeout(30)
            .map_err(|e| anyhow!(e))?;
        builder.add_operation(operation);
        let tx = builder.build();

        let envelope_xdr = tx
            .to_envelope()
            .map_err(|e| anyhow!("envelope serialization failed: {e}"))?
            .to_xdr_base64(Limits::none())
            .map_err(|e| anyhow!("XDR base64 failed: {e}"))?;

        let tx_hash = hex::encode(tx.hash());

        let out = json!({
            "status": "dry_run",
            "contract": contract_id,
            "function": function,
            "network": cli.network,
            "source": source_pub,
            "envelope_xdr": envelope_xdr,
            "tx_hash": tx_hash,
        });
        Ok(serde_json::to_string_pretty(&out)?)
    }
}

// ---------------------------------------------------------------------------
// Boundary and recovery test coverage
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::{env, ffi::OsString, sync::Mutex};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// A well-formed G-address (Stellar ed25519 public key) used across tests.
    const VALID_G: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
    /// A well-formed C-address (Soroban contract id) used across tests.
    const VALID_C: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";

    struct EnvVarGuard(Vec<(&'static str, Option<OsString>)>);

    impl EnvVarGuard {
        fn clear() -> Self {
            let vars = [
                "CREDENCE_RPC_URL",
                "CREDENCE_NETWORK",
                "CREDENCE_CONTRACT",
                "CREDENCE_SIGNER",
            ]
            .into_iter()
            .map(|name| (name, env::var_os(name)))
            .collect::<Vec<_>>();

            for (name, _) in &vars {
                env::remove_var(name);
            }

            Self(vars)
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            for (name, value) in self.0.drain(..) {
                if let Some(value) = value {
                    env::set_var(name, value);
                } else {
                    env::remove_var(name);
                }
            }
        }
    }

    fn parse_cli(args: &[&str]) -> Result<Cli, clap::Error> {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let _env = EnvVarGuard::clear();
        Cli::try_parse_from(args)
    }

    // -----------------------------------------------------------------------
    // Argument builder: success paths
    // -----------------------------------------------------------------------

    #[test]
    fn early_exit_args_valid() {
        let args = build_early_exit_args(VALID_G, VALID_G, 100).expect("valid args");
        assert_eq!(args.len(), 3);
        assert!(matches!(args[2], ScVal::U32(100)));
    }

    #[test]
    fn weight_args_valid() {
        let args = build_weight_args(VALID_G, 500, 10_000).expect("valid args");
        assert_eq!(args.len(), 3);
        assert!(matches!(args[1], ScVal::U32(500)));
        assert!(matches!(args[2], ScVal::U32(10_000)));
    }

    #[test]
    fn pause_signer_args_valid() {
        let args = build_pause_signer_args(VALID_G, VALID_G, true).expect("valid args");
        assert_eq!(args.len(), 3);
        assert!(matches!(args[2], ScVal::Bool(true)));
    }

    // -----------------------------------------------------------------------
    // Argument builder: boundary values
    // -----------------------------------------------------------------------

    #[test]
    fn early_exit_bps_boundary_zero() {
        let args = build_early_exit_args(VALID_G, VALID_G, 0).expect("zero bps is valid");
        assert!(matches!(args[2], ScVal::U32(0)));
    }

    #[test]
    fn early_exit_bps_boundary_max() {
        let args = build_early_exit_args(VALID_G, VALID_G, 10_000).expect("max bps is valid");
        assert!(matches!(args[2], ScVal::U32(10_000)));
    }

    #[test]
    fn weight_bps_boundary_zero() {
        let args = build_weight_args(VALID_G, 0, 0).expect("zero values are valid");
        assert!(matches!(args[1], ScVal::U32(0)));
        assert!(matches!(args[2], ScVal::U32(0)));
    }

    #[test]
    fn weight_bps_boundary_u32_max() {
        let args = build_weight_args(VALID_G, u32::MAX, u32::MAX).expect("u32::MAX is valid");
        assert!(matches!(args[1], ScVal::U32(u32::MAX)));
        assert!(matches!(args[2], ScVal::U32(u32::MAX)));
    }

    #[test]
    fn pause_signer_args_disabled() {
        let args = build_pause_signer_args(VALID_G, VALID_G, false).expect("valid args");
        assert!(matches!(args[2], ScVal::Bool(false)));
    }

    // -----------------------------------------------------------------------
    // Argument builder: rejection paths
    // -----------------------------------------------------------------------

    #[test]
    fn early_exit_args_reject_empty_admin() {
        let err = build_early_exit_args("", VALID_G, 100).unwrap_err();
        assert!(err.to_string().contains("invalid address"));
    }

    #[test]
    fn early_exit_args_reject_empty_treasury() {
        let err = build_early_exit_args(VALID_G, "", 100).unwrap_err();
        assert!(err.to_string().contains("invalid address"));
    }

    #[test]
    fn early_exit_args_reject_malformed_admin() {
        let err = build_early_exit_args("not-an-address", VALID_G, 100).unwrap_err();
        assert!(err.to_string().contains("invalid address"));
    }

    #[test]
    fn weight_args_reject_malformed_admin() {
        let err = build_weight_args("G123", 500, 10_000).unwrap_err();
        assert!(err.to_string().contains("invalid address"));
    }

    #[test]
    fn pause_signer_args_reject_malformed_signer() {
        let err = build_pause_signer_args(VALID_G, "C-short", true).unwrap_err();
        assert!(err.to_string().contains("invalid address"));
    }

    #[test]
    fn addr_to_sc_val_rejects_garbage() {
        let err = addr_to_sc_val("!!!").unwrap_err();
        assert!(err.to_string().contains("invalid address"));
    }

    #[test]
    fn addr_to_sc_val_accepts_g_address() {
        let val = addr_to_sc_val(VALID_G).expect("valid G address");
        assert!(matches!(val, ScVal::Address(_)));
    }

    #[test]
    fn addr_to_sc_val_accepts_c_address() {
        let val = addr_to_sc_val(VALID_C).expect("valid C address");
        assert!(matches!(val, ScVal::Address(_)));
    }

    // -----------------------------------------------------------------------
    // Timelock and arbitration argument builders
    // -----------------------------------------------------------------------

    const VALID_SALT: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    #[test]
    fn queue_call_args_are_encoded_in_contract_order() {
        let specs = vec![
            "address:GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF".to_string(),
            "u32:7".to_string(),
        ];
        let args = build_queue_call_args(
            VALID_G,
            VALID_C,
            "set_early_exit_config",
            &specs,
            VALID_SALT,
            300,
        )
        .expect("valid queue_call args");
        assert_eq!(args.len(), 6);
        assert!(matches!(args[0], ScVal::Address(_)));
        assert!(matches!(args[1], ScVal::Address(_)));
        assert!(matches!(args[2], ScVal::Symbol(_)));
        let ScVal::Vec(Some(call_args)) = &args[3] else {
            panic!("args must be encoded as ScVal::Vec");
        };
        assert_eq!(call_args.len(), 2);
        assert!(matches!(call_args[0], ScVal::Address(_)));
        assert!(matches!(call_args[1], ScVal::U32(7)));
        let ScVal::Bytes(salt) = &args[4] else {
            panic!("salt must be encoded as ScVal::Bytes");
        };
        let raw_salt: &[u8] = salt.as_ref();
        assert_eq!(raw_salt.len(), 32);
        assert!(matches!(args[5], ScVal::U64(300)));
    }

    #[test]
    fn queue_call_args_allow_no_call_arguments() {
        let args = build_queue_call_args(VALID_G, VALID_C, "pause", &[], VALID_SALT, 0)
            .expect("valid queue_call args");
        assert!(matches!(&args[3], ScVal::Vec(Some(values)) if values.is_empty()));
        assert!(matches!(args[5], ScVal::U64(0)));
    }

    #[test]
    fn op_id_and_cancel_args_are_encoded() {
        assert!(matches!(
            build_op_id_args(42).unwrap().as_slice(),
            [ScVal::U64(42)]
        ));
        let args = build_cancel_operation_args(VALID_G, 42).unwrap();
        assert_eq!(args.len(), 2);
        assert!(matches!(args[0], ScVal::Address(_)));
        assert!(matches!(args[1], ScVal::U64(42)));
    }

    #[test]
    fn arbitration_args_are_encoded() {
        let registered = build_register_arbitrator_args(VALID_G, -7).unwrap();
        assert_eq!(registered.len(), 2);
        assert!(matches!(registered[0], ScVal::Address(_)));
        assert_eq!(i128::try_from(registered[1].clone()).unwrap(), -7);

        assert_eq!(build_unregister_arbitrator_args(VALID_G).unwrap().len(), 1);

        let quorum = build_set_quorum_args(VALID_G, 9, 3).unwrap();
        assert_eq!(quorum.len(), 3);
        assert_eq!(i128::try_from(quorum[1].clone()).unwrap(), 9);
        assert!(matches!(quorum[2], ScVal::U32(3)));

        let paused = build_caller_args(VALID_G).unwrap();
        assert_eq!(paused.len(), 1);
        assert!(matches!(paused[0], ScVal::Address(_)));
    }

    #[test]
    fn typed_arg_specs_encode_address_bytes_symbol_and_vec() {
        assert!(matches!(
            parse_arg_spec("address:GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF")
                .unwrap(),
            ScVal::Address(_)
        ));
        assert!(matches!(
            parse_arg_spec(
                "bytes32:0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"
            )
            .unwrap(),
            ScVal::Bytes(_)
        ));
        assert!(matches!(
            parse_arg_spec("symbol:set_quorum").unwrap(),
            ScVal::Symbol(_)
        ));
        assert!(matches!(
            parse_arg_spec("string:hello").unwrap(),
            ScVal::String(_)
        ));
        assert!(matches!(parse_arg_spec("i128:-5").unwrap(), ScVal::I128(_)));
        assert!(matches!(parse_arg_spec("u64:9").unwrap(), ScVal::U64(9)));

        let ScVal::Vec(Some(values)) = parse_arg_spec(
            "vec:[u32:1,address:GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF]",
        )
        .unwrap() else {
            panic!("nested vec must encode as ScVal::Vec");
        };
        assert_eq!(values.len(), 2);
        assert!(matches!(values[0], ScVal::U32(1)));
        assert!(matches!(values[1], ScVal::Address(_)));
    }

    #[test]
    fn queue_call_args_reject_bad_salt_function_and_arg_types() {
        let err = build_queue_call_args(VALID_G, VALID_C, "pause", &[], "00", 1).unwrap_err();
        assert!(err.to_string().contains("32 bytes"));

        let err =
            build_queue_call_args(VALID_G, VALID_C, "bad-func", &[], VALID_SALT, 1).unwrap_err();
        assert!(err.to_string().contains("function name"));

        let err = build_queue_call_args(
            VALID_G,
            VALID_C,
            "pause",
            &["not-a-spec".to_string()],
            VALID_SALT,
            1,
        )
        .unwrap_err();
        assert!(err.to_string().contains("TYPE:VALUE"));

        let err = parse_arg_spec("float:1.5").unwrap_err();
        assert!(err.to_string().contains("unsupported --arg type"));
    }

    // -----------------------------------------------------------------------
    // CLI parsing: defaults and env fallbacks
    // -----------------------------------------------------------------------

    #[test]
    fn cli_defaults_are_applied() {
        let cli = parse_cli(&[
            "credence-admin",
            "bond-set-early-exit-config",
            "--admin",
            VALID_G,
            "--treasury",
            VALID_G,
            "--bps",
            "100",
        ])
        .expect("parse ok");
        assert_eq!(cli.rpc_url, "https://soroban-testnet.stellar.org");
        assert_eq!(cli.network, "Test SDF Network ; September 2015");
        assert!(!cli.submit);
        assert!(cli.contract.is_none());
        assert!(cli.signer.is_none());
    }

    #[test]
    fn cli_parses_bond_set_early_exit_config() {
        let cli = parse_cli(&[
            "credence-admin",
            "bond-set-early-exit-config",
            "--admin",
            VALID_G,
            "--treasury",
            VALID_C,
            "--bps",
            "250",
        ])
        .expect("parse ok");

        assert!(matches!(
            cli.command,
            Commands::BondSetEarlyExitConfig {
                admin,
                treasury,
                bps: 250,
            } if admin == VALID_G && treasury == VALID_C
        ));
    }

    #[test]
    fn cli_parses_bond_set_weights() {
        let cli = parse_cli(&[
            "credence-admin",
            "bond-set-weights",
            "--admin",
            VALID_G,
            "--multiplier-bps",
            "500",
            "--max-weight",
            "10000",
        ])
        .expect("parse ok");

        assert!(matches!(
            cli.command,
            Commands::BondSetWeights {
                admin,
                multiplier_bps: 500,
                max_weight: 10000,
            } if admin == VALID_G
        ));
    }

    #[test]
    fn cli_parses_delegation_set_pause_signer() {
        let cli = parse_cli(&[
            "credence-admin",
            "delegation-set-pause-signer",
            "--admin",
            VALID_G,
            "--pause-signer",
            VALID_C,
            "--enabled",
            "false",
        ])
        .expect("parse ok");

        assert!(matches!(
            cli.command,
            Commands::DelegationSetPauseSigner {
                admin,
                pause_signer,
                enabled: false,
            } if admin == VALID_G && pause_signer == VALID_C
        ));
    }

    #[test]
    fn cli_uses_environment_fallbacks() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
        let _env = EnvVarGuard::clear();
        env::set_var("CREDENCE_RPC_URL", "https://rpc.example.test");
        env::set_var("CREDENCE_NETWORK", "Example Network");
        env::set_var("CREDENCE_CONTRACT", VALID_C);
        env::set_var("CREDENCE_SIGNER", "S-example-secret");

        let cli = Cli::try_parse_from([
            "credence-admin",
            "bond-set-weights",
            "--admin",
            VALID_G,
            "--multiplier-bps",
            "500",
            "--max-weight",
            "10000",
        ])
        .expect("parse ok");

        assert_eq!(cli.rpc_url, "https://rpc.example.test");
        assert_eq!(cli.network, "Example Network");
        assert_eq!(cli.contract.as_deref(), Some(VALID_C));
        assert_eq!(cli.signer.as_deref(), Some("S-example-secret"));
    }

    #[test]
    fn cli_submit_flag_parses() {
        let cli = parse_cli(&[
            "credence-admin",
            "--submit",
            "bond-set-weights",
            "--admin",
            VALID_G,
            "--multiplier-bps",
            "500",
            "--max-weight",
            "10000",
        ])
        .expect("parse ok");
        assert!(cli.submit);
    }

    #[test]
    fn cli_pause_signer_enabled_defaults_true() {
        let cli = parse_cli(&[
            "credence-admin",
            "delegation-set-pause-signer",
            "--admin",
            VALID_G,
            "--pause-signer",
            VALID_G,
        ])
        .expect("parse ok");
        match cli.command {
            Commands::DelegationSetPauseSigner { enabled, .. } => assert!(enabled),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn cli_pause_signer_enabled_explicit_false() {
        let cli = parse_cli(&[
            "credence-admin",
            "delegation-set-pause-signer",
            "--admin",
            VALID_G,
            "--pause-signer",
            VALID_G,
            "--enabled",
            "false",
        ])
        .expect("parse ok");
        match cli.command {
            Commands::DelegationSetPauseSigner { enabled, .. } => assert!(!enabled),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn cli_rejects_missing_required_flags() {
        let missing_required_flags = [
            vec![
                "credence-admin",
                "bond-set-early-exit-config",
                "--treasury",
                VALID_G,
                "--bps",
                "100",
            ],
            vec![
                "credence-admin",
                "bond-set-early-exit-config",
                "--admin",
                VALID_G,
                "--bps",
                "100",
            ],
            vec![
                "credence-admin",
                "bond-set-early-exit-config",
                "--admin",
                VALID_G,
                "--treasury",
                VALID_G,
            ],
            vec![
                "credence-admin",
                "bond-set-weights",
                "--admin",
                VALID_G,
                "--max-weight",
                "10000",
            ],
            vec![
                "credence-admin",
                "bond-set-weights",
                "--admin",
                VALID_G,
                "--multiplier-bps",
                "500",
            ],
            vec![
                "credence-admin",
                "bond-set-weights",
                "--multiplier-bps",
                "500",
                "--max-weight",
                "10000",
            ],
            vec![
                "credence-admin",
                "delegation-set-pause-signer",
                "--pause-signer",
                VALID_G,
            ],
            vec![
                "credence-admin",
                "delegation-set-pause-signer",
                "--admin",
                VALID_G,
            ],
        ];

        for args in missing_required_flags {
            let err = parse_cli(&args).unwrap_err();
            assert_eq!(
                err.kind(),
                clap::error::ErrorKind::MissingRequiredArgument,
                "arguments should be rejected: {args:?}"
            );
        }
    }

    #[test]
    fn cli_rejects_non_numeric_bps() {
        let err = parse_cli(&[
            "credence-admin",
            "bond-set-early-exit-config",
            "--admin",
            VALID_G,
            "--treasury",
            VALID_G,
            "--bps",
            "not-a-number",
        ])
        .unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
    }

    #[test]
    fn cli_rejects_bps_overflow() {
        // u32 overflow must be rejected at parse time, not silently truncated.
        let err = parse_cli(&[
            "credence-admin",
            "bond-set-early-exit-config",
            "--admin",
            VALID_G,
            "--treasury",
            VALID_G,
            "--bps",
            "4294967296",
        ])
        .unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
    }

    #[test]
    fn cli_parses_timelock_and_arbitration_commands() {
        let cli = parse_cli(&[
            "credence-admin",
            "timelock-queue-call",
            "--proposer",
            VALID_G,
            "--target",
            VALID_C,
            "--func",
            "set_quorum",
            "--arg",
            "u32:1",
            "--arg",
            "address:GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            "--salt",
            VALID_SALT,
            "--delay",
            "300",
        ])
        .expect("parse ok");
        match cli.command {
            Commands::TimelockQueueCall {
                args, delay, func, ..
            } => {
                assert_eq!(args.len(), 2);
                assert_eq!(func, "set_quorum");
                assert_eq!(delay, 300);
            }
            _ => panic!("wrong command"),
        }

        let cli =
            parse_cli(&["credence-admin", "timelock-execute", "--op-id", "7"]).expect("parse ok");
        assert!(matches!(
            cli.command,
            Commands::TimelockExecute { op_id: 7 }
        ));

        let cli = parse_cli(&[
            "credence-admin",
            "timelock-cancel",
            "--admin",
            VALID_G,
            "--op-id",
            "7",
        ])
        .expect("parse ok");
        assert!(matches!(
            cli.command,
            Commands::TimelockCancel { op_id: 7, .. }
        ));

        let cli = parse_cli(&[
            "credence-admin",
            "arbitration-register-arbitrator",
            "--arbitrator",
            VALID_G,
            "--weight",
            "-5",
        ])
        .expect("parse ok");
        assert!(matches!(
            cli.command,
            Commands::ArbitrationRegisterArbitrator { weight: -5, .. }
        ));

        let cli = parse_cli(&[
            "credence-admin",
            "arbitration-unregister-arbitrator",
            "--arbitrator",
            VALID_G,
        ])
        .expect("parse ok");
        assert!(matches!(
            cli.command,
            Commands::ArbitrationUnregisterArbitrator { .. }
        ));

        let cli = parse_cli(&[
            "credence-admin",
            "arbitration-set-quorum",
            "--admin",
            VALID_G,
            "--min-total-weight",
            "100",
            "--min-voters",
            "3",
        ])
        .expect("parse ok");
        assert!(matches!(
            cli.command,
            Commands::ArbitrationSetQuorum { min_voters: 3, .. }
        ));

        let cli = parse_cli(&["credence-admin", "arbitration-pause", "--caller", VALID_G])
            .expect("parse ok");
        assert!(matches!(cli.command, Commands::ArbitrationPause { .. }));
    fn cli_accepts_bps_boundary_values() {
        for bps in ["0", "10000"] {
            let cli = parse_cli(&[
                "credence-admin",
                "bond-set-early-exit-config",
                "--admin",
                VALID_G,
                "--treasury",
                VALID_G,
                "--bps",
                bps,
            ])
            .expect("boundary bps should be accepted");
            match cli.command {
                Commands::BondSetEarlyExitConfig { bps: parsed, .. } => {
                    assert_eq!(parsed, bps.parse::<u32>().unwrap());
                }
                _ => panic!("wrong command"),
            }
        }
    }

    #[test]
    fn cli_rejects_out_of_range_bps() {
        for bps in ["10001", "70000"] {
            let err = parse_cli(&[
                "credence-admin",
                "bond-set-early-exit-config",
                "--admin",
                VALID_G,
                "--treasury",
                VALID_G,
                "--bps",
                bps,
            ])
            .unwrap_err();
            assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
            assert!(
                err.to_string().contains("0..=10000"),
                "expected range message for bps={bps}, got: {err}"
            );
        }
    }

    #[test]
    fn cli_accepts_weight_boundary_values() {
        for (mult, max_w) in [("0", "0"), ("10000", "1000000")] {
            let cli = parse_cli(&[
                "credence-admin",
                "bond-set-weights",
                "--admin",
                VALID_G,
                "--multiplier-bps",
                mult,
                "--max-weight",
                max_w,
            ])
            .expect("boundary weight config should be accepted");
            match cli.command {
                Commands::BondSetWeights {
                    multiplier_bps,
                    max_weight,
                    ..
                } => {
                    assert_eq!(multiplier_bps, mult.parse::<u32>().unwrap());
                    assert_eq!(max_weight, max_w.parse::<u32>().unwrap());
                }
                _ => panic!("wrong command"),
            }
        }
    }

    #[test]
    fn cli_rejects_out_of_range_multiplier_bps() {
        for mult in ["10001", "70000"] {
            let err = parse_cli(&[
                "credence-admin",
                "bond-set-weights",
                "--admin",
                VALID_G,
                "--multiplier-bps",
                mult,
                "--max-weight",
                "1000",
            ])
            .unwrap_err();
            assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
            assert!(
                err.to_string().contains("0..=10000"),
                "expected range message for multiplier_bps={mult}, got: {err}"
            );
        }
    }

    #[test]
    fn cli_rejects_out_of_range_max_weight() {
        let err = parse_cli(&[
            "credence-admin",
            "bond-set-weights",
            "--admin",
            VALID_G,
            "--multiplier-bps",
            "500",
            "--max-weight",
            "1000001",
        ])
        .unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
        assert!(
            err.to_string().contains("0..=1000000"),
            "expected range message for max_weight=1000001, got: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // Recovery: --submit without signer must fail fast with a clear message
    // -----------------------------------------------------------------------

    #[test]
    fn submit_without_signer_is_rejected() {
        let cli = parse_cli(&[
            "credence-admin",
            "--submit",
            "--contract",
            VALID_C,
            "bond-set-weights",
            "--admin",
            VALID_G,
            "--multiplier-bps",
            "500",
            "--max-weight",
            "10000",
        ])
        .expect("parse ok");

        let args = build_weight_args(VALID_G, 500, 10_000).expect("valid args");
        let err = run(&cli, VALID_C, "set_weight_config", args).unwrap_err();
        assert!(
            err.to_string().contains("--signer"),
            "expected signer-required error, got: {err}"
        );
    }

    #[test]
    fn submit_with_invalid_signer_is_rejected() {
        let cli = parse_cli(&[
            "credence-admin",
            "--submit",
            "--signer",
            "not-a-secret",
            "--contract",
            VALID_C,
            "bond-set-weights",
            "--admin",
            VALID_G,
            "--multiplier-bps",
            "500",
            "--max-weight",
            "10000",
        ])
        .expect("parse ok");

        let args = build_weight_args(VALID_G, 500, 10_000).expect("valid args");
        let err = run(&cli, VALID_C, "set_weight_config", args).unwrap_err();
        assert!(
            err.to_string().contains("invalid signer key"),
            "expected invalid-signer error, got: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // Recovery: invalid contract address must fail before any network I/O
    // -----------------------------------------------------------------------

    #[test]
    fn run_rejects_invalid_contract_address() {
        let cli = parse_cli(&[
            "credence-admin",
            "bond-set-weights",
            "--admin",
            VALID_G,
            "--multiplier-bps",
            "500",
            "--max-weight",
            "10000",
        ])
        .expect("parse ok");

        let args = build_weight_args(VALID_G, 500, 10_000).expect("valid args");
        let err = run(&cli, "not-a-contract", "set_weight_config", args).unwrap_err();
        assert!(
            err.to_string().contains("invalid contract address"),
            "expected contract-address error, got: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // Regression: dry-run path is deterministic and does not require signer
    // -----------------------------------------------------------------------

    #[test]
    fn dry_run_does_not_require_signer() {
        // A dry-run (no --submit) must succeed without a signer. We only
        // exercise the argument-building half here so the test stays offline;
        // the full dry-run path is covered by integration tests that capture
        // stdout. This guards against regressions that would make a signer
        // mandatory for read-only inspection.
        let cli = parse_cli(&[
            "credence-admin",
            "--contract",
            VALID_C,
            "bond-set-early-exit-config",
            "--admin",
            VALID_G,
            "--treasury",
            VALID_G,
            "--bps",
            "100",
        ])
        .expect("parse ok");
        assert!(!cli.submit);
        assert!(cli.signer.is_none());
        let _ = build_early_exit_args(VALID_G, VALID_G, 100).expect("valid args");
    }

    // -----------------------------------------------------------------------
    // Offline --submit tests with an in-process mock SorobanRpc
    // -----------------------------------------------------------------------

    use rpc::SubmitResult;
    use soroban_client::transaction::Transaction;
    use std::cell::{Cell, RefCell};
    use stellar_baselib::xdr::{SequenceNumber, TransactionEnvelope};

    struct MockRpc {
        account_seq: Result<String, String>,
        prepare_err: Option<String>,
        send_result: Result<SubmitResult, String>,
        get_account_calls: Cell<usize>,
        prepare_calls: Cell<usize>,
        send_calls: Cell<usize>,
        seen_sequence: Cell<Option<i64>>,
        seen_signature_count: Cell<usize>,
        seen_source: RefCell<Option<String>>,
    }

    impl MockRpc {
        fn succeeding(seq: &str, hash: &str) -> Self {
            Self {
                account_seq: Ok(seq.to_string()),
                prepare_err: None,
                send_result: Ok(SubmitResult {
                    status: "PENDING".to_string(),
                    hash: hash.to_string(),
                }),
                get_account_calls: Cell::new(0),
                prepare_calls: Cell::new(0),
                send_calls: Cell::new(0),
                seen_sequence: Cell::new(None),
                seen_signature_count: Cell::new(0),
                seen_source: RefCell::new(None),
            }
        }
    }

    impl SorobanRpc for MockRpc {
        fn get_account(&self, public_key: &str) -> Result<Account> {
            self.get_account_calls.set(self.get_account_calls.get() + 1);
            *self.seen_source.borrow_mut() = Some(public_key.to_string());
            match &self.account_seq {
                Ok(seq) => Account::new(public_key, seq)
                    .map_err(|e| anyhow!("mock account error: {e:?}")),
                Err(msg) => Err(anyhow!("{msg}")),
            }
        }

        fn prepare_transaction(&self, tx: &Transaction) -> Result<Transaction> {
            self.prepare_calls.set(self.prepare_calls.get() + 1);
            let env = tx
                .to_envelope()
                .map_err(|e| anyhow!("mock envelope error: {e}"))?;
            if let TransactionEnvelope::Tx(v1) = env {
                let SequenceNumber(seq) = v1.tx.seq_num;
                self.seen_sequence.set(Some(seq));
            }
            if let Some(err) = &self.prepare_err {
                return Err(anyhow!("{err}"));
            }
            Ok(tx.clone())
        }

        fn send_transaction(&self, tx: Transaction) -> Result<SubmitResult> {
            self.send_calls.set(self.send_calls.get() + 1);
            let env = tx
                .to_envelope()
                .map_err(|e| anyhow!("mock envelope error: {e}"))?;
            if let TransactionEnvelope::Tx(v1) = env {
                self.seen_signature_count.set(v1.signatures.len());
            }
            match &self.send_result {
                Ok(res) => Ok(res.clone()),
                Err(msg) => Err(anyhow!("{msg}")),
            }
        }
    }

    fn test_signer_secret() -> (String, String) {
        let kp = Keypair::from_raw_ed25519_seed(&[7u8; 32]).expect("valid seed");
        (kp.secret_key().expect("secret key"), kp.public_key())
    }

    #[test]
    fn submit_success_increments_sequence_signs_tx_and_redacts_secret() {
        let (secret, public_key) = test_signer_secret();
        let cli = parse_cli(&[
            "credence-admin",
            "--submit",
            "--contract",
            VALID_C,
            "--signer",
            &secret,
            "bond-set-weights",
            "--admin",
            VALID_G,
            "--multiplier-bps",
            "500",
            "--max-weight",
            "10000",
        ])
        .expect("parse ok");

        let args = build_weight_args(VALID_G, 500, 10_000).expect("valid args");
        let mock = MockRpc::succeeding("41", "abcdef0123456789");

        let out = run_with_rpc(&cli, VALID_C, "set_weight_config", args, Some(&mock))
            .expect("submit should succeed");

        assert_eq!(mock.get_account_calls.get(), 1);
        assert_eq!(mock.prepare_calls.get(), 1);
        assert_eq!(mock.send_calls.get(), 1);
        assert_eq!(mock.seen_source.borrow().as_deref(), Some(public_key.as_str()));
        assert_eq!(mock.seen_sequence.get(), Some(42));
        assert_eq!(mock.seen_signature_count.get(), 1);

        let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(parsed["status"], "PENDING");
        assert_eq!(parsed["hash"], "abcdef0123456789");
        assert!(
            !out.contains(&secret),
            "secret key must never appear in submit output"
        );
    }

    #[test]
    fn submit_account_lookup_error_short_circuits_and_redacts_secret() {
        let (secret, _) = test_signer_secret();
        let cli = parse_cli(&[
            "credence-admin",
            "--submit",
            "--contract",
            VALID_C,
            "--signer",
            &secret,
            "bond-set-weights",
            "--admin",
            VALID_G,
            "--multiplier-bps",
            "500",
            "--max-weight",
            "10000",
        ])
        .expect("parse ok");

        let args = build_weight_args(VALID_G, 500, 10_000).expect("valid args");
        let mut mock = MockRpc::succeeding("0", "unused");
        mock.account_seq = Err("failed to load source account: RPC timeout".to_string());

        let err = run_with_rpc(&cli, VALID_C, "set_weight_config", args, Some(&mock)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("RPC timeout"), "unexpected error: {msg}");
        assert!(!msg.contains(&secret), "secret leaked in error: {msg}");
        assert_eq!(mock.get_account_calls.get(), 1);
        assert_eq!(mock.prepare_calls.get(), 0);
        assert_eq!(mock.send_calls.get(), 0);
    }

    #[test]
    fn submit_simulation_error_short_circuits_and_redacts_secret() {
        let (secret, _) = test_signer_secret();
        let cli = parse_cli(&[
            "credence-admin",
            "--submit",
            "--contract",
            VALID_C,
            "--signer",
            &secret,
            "bond-set-weights",
            "--admin",
            VALID_G,
            "--multiplier-bps",
            "500",
            "--max-weight",
            "10000",
        ])
        .expect("parse ok");

        let args = build_weight_args(VALID_G, 500, 10_000).expect("valid args");
        let mut mock = MockRpc::succeeding("10", "unused");
        mock.prepare_err = Some("simulation failed: contract trap".to_string());

        let err = run_with_rpc(&cli, VALID_C, "set_weight_config", args, Some(&mock)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("simulation failed"), "unexpected error: {msg}");
        assert!(!msg.contains(&secret), "secret leaked in error: {msg}");
        assert_eq!(mock.get_account_calls.get(), 1);
        assert_eq!(mock.prepare_calls.get(), 1);
        assert_eq!(mock.send_calls.get(), 0);
    }

    #[test]
    fn submit_rejected_submission_rpc_error_and_error_status_fail_without_secret() {
        let (secret, _) = test_signer_secret();
        let cli = parse_cli(&[
            "credence-admin",
            "--submit",
            "--contract",
            VALID_C,
            "--signer",
            &secret,
            "bond-set-weights",
            "--admin",
            VALID_G,
            "--multiplier-bps",
            "500",
            "--max-weight",
            "10000",
        ])
        .expect("parse ok");

        // Case 1: send_transaction returns an RPC error.
        let args = build_weight_args(VALID_G, 500, 10_000).expect("valid args");
        let mut mock_err = MockRpc::succeeding("10", "unused");
        mock_err.send_result = Err("send_transaction failed: tx_bad_seq".to_string());
        let err = run_with_rpc(&cli, VALID_C, "set_weight_config", args, Some(&mock_err))
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("tx_bad_seq"), "unexpected error: {msg}");
        assert!(!msg.contains(&secret), "secret leaked in error: {msg}");
        assert_eq!(mock_err.send_calls.get(), 1);

        // Case 2: send_transaction returns status ERROR.
        let args = build_weight_args(VALID_G, 500, 10_000).expect("valid args");
        let mut mock_rejected = MockRpc::succeeding("10", "badhash123");
        mock_rejected.send_result = Ok(SubmitResult {
            status: "ERROR".to_string(),
            hash: "badhash123".to_string(),
        });
        let err = run_with_rpc(
            &cli,
            VALID_C,
            "set_weight_config",
            args,
            Some(&mock_rejected),
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("rejected"), "unexpected error: {msg}");
        assert!(!msg.contains(&secret), "secret leaked in error: {msg}");
        assert_eq!(mock_rejected.send_calls.get(), 1);
    }

    #[test]
    fn submit_bad_signer_key_fails_before_rpc_without_leaking_key() {
        let bad_secret = "SNOTAVALIDSECRETKEYMATERIAL1234567890";
        let cli = parse_cli(&[
            "credence-admin",
            "--submit",
            "--contract",
            VALID_C,
            "--signer",
            bad_secret,
            "bond-set-weights",
            "--admin",
            VALID_G,
            "--multiplier-bps",
            "500",
            "--max-weight",
            "10000",
        ])
        .expect("parse ok");

        let args = build_weight_args(VALID_G, 500, 10_000).expect("valid args");
        let mock = MockRpc::succeeding("0", "unused");
        let err = run_with_rpc(&cli, VALID_C, "set_weight_config", args, Some(&mock)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("invalid signer key"), "unexpected error: {msg}");
        assert!(
            !msg.contains(bad_secret),
            "bad signer key must not be echoed in error: {msg}"
        );
        assert_eq!(mock.get_account_calls.get(), 0);
        assert_eq!(mock.prepare_calls.get(), 0);
        assert_eq!(mock.send_calls.get(), 0);
    }

    #[test]
    fn submit_missing_contract_fails_before_rpc_without_leaking_secret() {
        let (secret, _) = test_signer_secret();
        let cli = parse_cli(&[
            "credence-admin",
            "--submit",
            "--signer",
            &secret,
            "bond-set-weights",
            "--admin",
            VALID_G,
            "--multiplier-bps",
            "500",
            "--max-weight",
            "10000",
        ])
        .expect("parse ok");

        let resolve_err = resolve_contract_id(&cli).unwrap_err();
        assert!(
            resolve_err.to_string().contains("--contract"),
            "unexpected resolve error: {resolve_err}"
        );
        assert!(!resolve_err.to_string().contains(&secret));

        let args = build_weight_args(VALID_G, 500, 10_000).expect("valid args");
        let mock = MockRpc::succeeding("0", "unused");
        let err = run_with_rpc(&cli, VALID_C, "set_weight_config", args, Some(&mock)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("--contract"), "unexpected error: {msg}");
        assert!(!msg.contains(&secret), "secret leaked in error: {msg}");
        assert_eq!(mock.get_account_calls.get(), 0);
        assert_eq!(mock.prepare_calls.get(), 0);
        assert_eq!(mock.send_calls.get(), 0);
    }
}
