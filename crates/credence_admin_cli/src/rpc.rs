use anyhow::{anyhow, Result};
use soroban_client::{account::Account, transaction::Transaction, Options, Server};

/// Outcome of submitting a signed transaction to a Soroban RPC endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitResult {
    /// Status string returned by the RPC (e.g. `"PENDING"`, `"ERROR"`).
    pub status: String,
    /// Transaction hash returned by the RPC.
    pub hash: String,
}

/// Thin abstraction over [`soroban_client::Server`] for the `--submit` path.
///
/// Keeping this surface minimal allows offline tests to exercise the full
/// account-lookup -> simulate/prepare -> sign -> send flow with an in-process
/// mock without touching the network.
pub trait SorobanRpc {
    /// Fetch the source account and its current sequence number.
    fn get_account(&self, public_key: &str) -> Result<Account>;

    /// Simulate and assemble resource fees and footprint for the transaction.
    fn prepare_transaction(&self, tx: &Transaction) -> Result<Transaction>;

    /// Submit a signed transaction to the RPC endpoint.
    fn send_transaction(&self, tx: Transaction) -> Result<SubmitResult>;
}

/// Live Soroban RPC client backed by [`soroban_client::Server`].
pub struct LiveRpc {
    server: Server,
    runtime: tokio::runtime::Runtime,
}

impl LiveRpc {
    /// Connect to a Soroban RPC endpoint over HTTPS.
    pub fn new(rpc_url: &str) -> Result<Self> {
        let runtime = tokio::runtime::Runtime::new()?;
        let server = Server::new(
            rpc_url,
            Options {
                allow_http: false,
                ..Default::default()
            },
        )
        .map_err(|e| anyhow!("RPC connect error: {e:?}"))?;
        Ok(Self { server, runtime })
    }
}

impl SorobanRpc for LiveRpc {
    fn get_account(&self, public_key: &str) -> Result<Account> {
        self.runtime.block_on(async {
            self.server
                .get_account(public_key)
                .await
                .map_err(|e| anyhow!("failed to load source account {public_key}: {e:?}"))
        })
    }

    fn prepare_transaction(&self, tx: &Transaction) -> Result<Transaction> {
        self.runtime.block_on(async {
            self.server
                .prepare_transaction(tx)
                .await
                .map_err(|e| anyhow!("simulation failed: {e:?}"))
        })
    }

    fn send_transaction(&self, tx: Transaction) -> Result<SubmitResult> {
        self.runtime.block_on(async {
            let resp = self
                .server
                .send_transaction(tx)
                .await
                .map_err(|e| anyhow!("send_transaction failed: {e:?}"))?;
            #[allow(clippy::disallowed_macros)]
            let status = format!("{:?}", resp.status);
            Ok(SubmitResult {
                status,
                hash: resp.hash,
            })
        })
    }
}
