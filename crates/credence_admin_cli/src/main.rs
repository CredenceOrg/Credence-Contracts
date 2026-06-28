use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use serde_json::json;
use soroban_client::{
    account::{Account, AccountBehavior},
    transaction::TransactionBehavior,
    transaction_builder::{TransactionBuilder, TransactionBuilderBehavior},
    Options, Server,
};
use stellar_baselib::{
    address::{Address, AddressTrait},
    contract::{ContractBehavior, Contracts},
    keypair::{Keypair, KeypairBehavior},
    xdr::{Limits, ScVal, WriteXdr},
};

/// Admin CLI for Credence protocol contracts.
///
/// Builds real `InvokeHostFunction` (invoke_contract) transactions for each
/// admin operation. Without --submit the XDR envelope is printed as a
/// structured JSON dry-run. With --submit the transaction is signed with the
/// key from --signer (or the CREDENCE_SIGNER env-var) and sent to the RPC.
#[derive(Parser)]
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
        #[arg(long)]
        bps: u32,
    },

    /// Set weight configuration on a credence_bond contract.
    ///
    /// Maps to: set_weight_config(admin, multiplier_bps, max_weight)
    BondSetWeights {
        /// Admin Stellar address (G…).
        #[arg(long)]
        admin: String,
        /// Multiplier in basis points.
        #[arg(long)]
        multiplier_bps: u32,
        /// Maximum attestation weight cap.
        #[arg(long)]
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
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();

    let contract_id = cli.contract.as_deref().unwrap_or_else(|| {
        eprintln!("warning: --contract not set; using zero-address placeholder");
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4"
    });

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

/// Convert a Stellar address string (G… or C…) to an `ScVal::Address`.
fn addr_to_sc_val(addr: &str) -> Result<ScVal> {
    let address = Address::new(addr).map_err(|e| anyhow!("invalid address {addr:?}: {e}"))?;
    address
        .to_sc_val()
        .map_err(|e| anyhow!("failed to convert address {addr:?} to ScVal: {e}"))
}

// ---------------------------------------------------------------------------
// Core transaction builder / runner
// ---------------------------------------------------------------------------

/// Build an `InvokeHostFunction` transaction, then either print a dry-run
/// JSON report or sign-and-submit it to the network.
fn run(cli: &Cli, contract_id: &str, function: &str, args: Vec<ScVal>) -> Result<()> {
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
        Some(Keypair::from_secret(secret).map_err(|e| anyhow!("invalid signer key: {e}"))?)
    } else {
        None
    };

    // Use the signer public key as the source account, or a dummy for dry-runs.
    let source_pub = keypair
        .as_ref()
        .map(|kp| kp.public_key())
        .unwrap_or_else(|| "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF".to_string());

    if cli.submit {
        // --- Live path: fetch account, build, sign, submit ------------------
        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(async {
            let server = Server::new(
                &cli.rpc_url,
                Options {
                    allow_http: false,
                    ..Default::default()
                },
            )
            .map_err(|e| anyhow!("RPC connect error: {e:?}"))?;

            let mut source_account: Account = server
                .get_account(&source_pub)
                .await
                .map_err(|e| anyhow!("failed to load source account {source_pub}: {e:?}"))?;

            let mut builder = TransactionBuilder::new(&mut source_account, &cli.network, None);
            builder
                .fee(1_000_000_u32)
                .set_timeout(30)
                .map_err(|e| anyhow!(e))?;
            builder.add_operation(operation);
            let tx = builder.build();

            // Prepare (simulate + assemble footprint + resource fee).
            let tx = server
                .prepare_transaction(&tx)
                .await
                .map_err(|e| anyhow!("simulation failed: {e:?}"))?;

            // Sign.
            let kp = keypair.unwrap();
            let mut signed_tx = tx;
            signed_tx.sign(&[kp]);

            // Submit.
            let resp = server
                .send_transaction(signed_tx)
                .await
                .map_err(|e| anyhow!("send_transaction failed: {e:?}"))?;

            let out = json!({
                "status": format!("{:?}", resp.status),
                "hash": resp.hash,
            });
            println!("{}", serde_json::to_string_pretty(&out)?);
            Ok(())
        })
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
        println!("{}", serde_json::to_string_pretty(&out)?);
        Ok(())
    }
}
