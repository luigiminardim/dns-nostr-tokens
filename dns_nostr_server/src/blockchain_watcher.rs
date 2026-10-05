use bitcoincore_rpc::RpcApi;
use name_token::{NameTokenRepository, NameTokenService};
use std::{sync::Arc, time::Duration};

const MIN_CONFIRMATIONS: u64 = 6;
const SYNC_INTERVAL: Duration = Duration::from_secs(600); // 10 minutes

/// Feeds confirmed blocks from Bitcoin Core into the `NameTokenService`.
pub struct BlockchainWatcher<R: NameTokenRepository + 'static> {
    name_token_service: Arc<NameTokenService<R>>,
}

impl<R: NameTokenRepository + 'static> BlockchainWatcher<R> {
    pub fn new(name_token_service: Arc<NameTokenService<R>>) -> Self {
        Self { name_token_service }
    }

    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.watch_blockchain().await;
        })
    }

    fn bitcoin_client(&self) -> bitcoincore_rpc::Client {
        bitcoincore_rpc::Client::new(
            "http://0.0.0.0:18443",
            bitcoincore_rpc::Auth::UserPass("rpcuser".into(), "rpcpassword".into()),
        )
        .unwrap()
    }

    async fn watch_blockchain(&self) {
        loop {
            self.sync_blocks().await;
            tokio::time::sleep(SYNC_INTERVAL).await;
        }
    }

    async fn sync_blocks(&self) {
        println!("Syncing blocks...");
        loop {
            let next_blockheight = self.name_token_service.next_block_height().await;
            let blockchain_num_blocks = self
                .bitcoin_client()
                .get_blockchain_info()
                .expect("Failed to get blockchain info")
                .blocks;
            if next_blockheight >= blockchain_num_blocks - MIN_CONFIRMATIONS {
                break;
            }
            self.sync_block(next_blockheight).await;
        }
    }

    async fn sync_block(&self, blockheight: u64) {
        let block_hash = self
            .bitcoin_client()
            .get_block_hash(blockheight)
            .expect("Failed to get block hash");
        let block = self
            .bitcoin_client()
            .get_block(&block_hash)
            .expect("Failed to get block");
        self.name_token_service
            .apply_block(blockheight, &block)
            .await;
        println!("Synced block at height {}", blockheight);
    }
}
