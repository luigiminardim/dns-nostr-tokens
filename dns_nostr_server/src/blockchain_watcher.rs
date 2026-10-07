use bdk_bitcoind_rpc::{
    bitcoincore_rpc::{Auth, Client, RpcApi},
    Emitter,
};
use name_token::{NameTokenRepository, NameTokenService};
use std::{sync::Arc, time::Duration};

const SYNC_INTERVAL: Duration = Duration::from_secs(10); // sync every 10 seconds since we can handle reorgs

/// Feeds connected blocks from Bitcoin Core into the `NameTokenService`.
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

    fn bitcoin_client(&self) -> Client {
        Client::new(
            "http://0.0.0.0:18443",
            Auth::UserPass("rpcuser".into(), "rpcpassword".into()),
        )
        .unwrap()
    }

    async fn watch_blockchain(&self) {
        println!("Starting blockchain watcher...");
        loop {
            self.sync_blocks().await;
            tokio::time::sleep(SYNC_INTERVAL).await;
        }
    }

    async fn sync_blocks(&self) {
        let client = self.bitcoin_client();
        let tip = self.name_token_service.tip();

        let last_cp = match tip {
            Some(cp) => cp,
            None => {
                let genesis_hash = client
                    .get_block_hash(0)
                    .expect("Failed to get genesis hash");
                // Create a CheckPoint from the genesis block
                // Wait, how to construct a CheckPoint in bdk_chain 0.23?
                // `bdk_chain::local_chain::LocalChain::from_genesis_hash` creates a chain and returns `(LocalChain, CheckPoint)`.
                let (new_chain, _) =
                    bdk_chain::local_chain::LocalChain::from_genesis_hash(genesis_hash);
                new_chain.tip()
            }
        };

        let mut emitter = Emitter::new(
            &client,
            last_cp,
            0,
            bdk_bitcoind_rpc::NO_EXPECTED_MEMPOOL_TXS,
        );

        while let Some(emission) = emitter.next_block().expect("Failed to get next block") {
            self.name_token_service
                .apply_block_connected_to(
                    &emission.block,
                    emission.block_height(),
                    emission.connected_to(),
                )
                .await;
            println!("Synced block at height {}", emission.block_height());
        }
    }
}
