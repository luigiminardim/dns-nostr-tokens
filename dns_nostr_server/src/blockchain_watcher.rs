use bdk_bitcoind_rpc::{
    bitcoincore_rpc::{self, Auth, Client, RpcApi},
    Emitter,
};
use bdk_chain::local_chain::LocalChain;
use bitcoin::BlockHash;
use name_token::NameTokenService;
use std::{fmt, sync::Arc, time::Duration};

#[derive(Clone, PartialEq, Eq)]
pub struct BlockchainWatcherConfig {
    pub rpc_url: String,
    pub rpc_user: String,
    pub rpc_password: String,
    pub sync_interval: Duration,
}

impl fmt::Debug for BlockchainWatcherConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BlockchainWatcherConfig")
            .field("rpc_url", &self.rpc_url)
            .field("rpc_user", &self.rpc_user)
            .field("rpc_password", &"[REDACTED]")
            .field("sync_interval", &self.sync_interval)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatcherFatalError {
    GenesisMismatch {
        stored: BlockHash,
        node: BlockHash,
    },
}

impl fmt::Display for WatcherFatalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GenesisMismatch { stored, node } => write!(
                formatter,
                "SQLite genesis block ({stored}) does not match Bitcoin node genesis block ({node})"
            ),
        }
    }
}

impl std::error::Error for WatcherFatalError {}

enum SyncError {
    TransientRpc(bitcoincore_rpc::Error),
    Fatal(WatcherFatalError),
}

impl From<bitcoincore_rpc::Error> for SyncError {
    fn from(error: bitcoincore_rpc::Error) -> Self {
        Self::TransientRpc(error)
    }
}

/// Feeds connected blocks from Bitcoin Core into the `NameTokenService`.
pub struct BlockchainWatcher {
    name_token_service: Arc<NameTokenService>,
    config: BlockchainWatcherConfig,
}

impl BlockchainWatcher {
    pub fn new(
        name_token_service: Arc<NameTokenService>,
        config: BlockchainWatcherConfig,
    ) -> Self {
        Self {
            name_token_service,
            config,
        }
    }

    pub fn spawn(self) -> tokio::task::JoinHandle<Result<(), WatcherFatalError>> {
        tokio::spawn(async move { self.watch_blockchain().await })
    }

    fn bitcoin_client(&self) -> Result<Client, bitcoincore_rpc::Error> {
        Client::new(
            &self.config.rpc_url,
            Auth::UserPass(
                self.config.rpc_user.clone(),
                self.config.rpc_password.clone(),
            ),
        )
    }

    fn verify_genesis(
        stored: Option<BlockHash>,
        node: BlockHash,
    ) -> Result<(), WatcherFatalError> {
        if let Some(stored_hash) = stored {
            if stored_hash != node {
                return Err(WatcherFatalError::GenesisMismatch {
                    stored: stored_hash,
                    node,
                });
            }
        }
        Ok(())
    }

    async fn watch_blockchain(self) -> Result<(), WatcherFatalError> {
        println!("Starting blockchain watcher...");
        loop {
            match self.sync_blocks().await {
                Ok(()) => {}
                Err(SyncError::TransientRpc(error)) => {
                    eprintln!("Blockchain watcher transient RPC error: {error}");
                }
                Err(SyncError::Fatal(fatal)) => return Err(fatal),
            }
            tokio::time::sleep(self.config.sync_interval).await;
        }
    }

    async fn sync_blocks(&self) -> Result<(), SyncError> {
        let client = self.bitcoin_client()?;
        let node_genesis = client.get_block_hash(0)?;
        Self::verify_genesis(self.name_token_service.genesis_hash(), node_genesis)
            .map_err(SyncError::Fatal)?;

        let last_cp = match self.name_token_service.tip() {
            Some(checkpoint) => checkpoint,
            None => {
                let (new_chain, _) = LocalChain::from_genesis_hash(node_genesis);
                new_chain.tip()
            }
        };

        let mut emitter = Emitter::new(
            &client,
            last_cp,
            0,
            bdk_bitcoind_rpc::NO_EXPECTED_MEMPOOL_TXS,
        );

        while let Some(emission) = emitter.next_block()? {
            self.name_token_service
                .apply_block_connected_to(
                    &emission.block,
                    emission.block_height(),
                    emission.connected_to(),
                )
                .await;
            println!("Synced block at height {}", emission.block_height());
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_blockchain_watcher {
    use super::*;
    use bitcoin::hashes::Hash;

    #[test]
    fn verify_genesis_accepts_none_or_matching_hash_and_rejects_mismatch() {
        let genesis_a = BlockHash::from_byte_array([1; 32]);
        let genesis_b = BlockHash::from_byte_array([2; 32]);

        assert_eq!(BlockchainWatcher::verify_genesis(None, genesis_a), Ok(()));
        assert_eq!(
            BlockchainWatcher::verify_genesis(Some(genesis_a), genesis_a),
            Ok(())
        );
        assert_eq!(
            BlockchainWatcher::verify_genesis(Some(genesis_a), genesis_b),
            Err(WatcherFatalError::GenesisMismatch {
                stored: genesis_a,
                node: genesis_b,
            })
        );
    }

    #[test]
    fn config_debug_redacts_rpc_password() {
        let config = BlockchainWatcherConfig {
            rpc_url: "http://127.0.0.1:18443".to_string(),
            rpc_user: "rpcuser".to_string(),
            rpc_password: "secret-rpc-password".to_string(),
            sync_interval: Duration::from_secs(1),
        };
        let formatted = format!("{config:?}");
        assert!(formatted.contains("[REDACTED]"), "{formatted}");
        assert!(!formatted.contains("secret-rpc-password"), "{formatted}");
    }
}
