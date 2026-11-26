use crate::domain::blockchain_port::{BlockchainError, BlockchainPort};
use crate::domain::dns_nostr_wallet::DnsNostrWallet;
use bdk_bitcoind_rpc::bitcoincore_rpc::{
    self,
    json::EstimateMode,
    jsonrpc, Auth, Client, RpcApi,
};
use bdk_bitcoind_rpc::Emitter;
use bdk_wallet::chain::BlockId;
use bitcoin::{Amount, BlockHash, FeeRate, Transaction, Txid};
use std::fmt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Bitcoin Core JSON-RPC implementation of `BlockchainPort`.
pub struct RpcBlockchainAdapter {
    rpc_url: String,
    rpc_user: String,
    rpc_password: String,
}

impl RpcBlockchainAdapter {
    pub fn new(
        rpc_url: impl Into<String>,
        rpc_user: impl Into<String>,
        rpc_password: impl Into<String>,
    ) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            rpc_user: rpc_user.into(),
            rpc_password: rpc_password.into(),
        }
    }

    fn client(&self) -> Result<Client, BlockchainError> {
        Client::new(
            &self.rpc_url,
            Auth::UserPass(self.rpc_user.clone(), self.rpc_password.clone()),
        )
        .map_err(|error| BlockchainError::Rpc(error.to_string()))
    }

    fn verify_genesis(client: &Client, expected: BlockHash) -> Result<(), BlockchainError> {
        let actual = client
            .get_block_hash(0)
            .map_err(|error| BlockchainError::Rpc(error.to_string()))?;
        Self::check_genesis(expected, actual)
    }

    fn check_genesis(expected: BlockHash, actual: BlockHash) -> Result<(), BlockchainError> {
        if actual != expected {
            return Err(BlockchainError::GenesisMismatch { expected, actual });
        }
        Ok(())
    }

    fn is_not_in_mempool_error(error: &bitcoincore_rpc::Error) -> bool {
        const RPC_INVALID_ADDRESS_OR_KEY: i32 = -5;
        matches!(
            error,
            bitcoincore_rpc::Error::JsonRpc(jsonrpc::Error::Rpc(rpc_error))
                if rpc_error.code == RPC_INVALID_ADDRESS_OR_KEY
        )
    }

    fn btc_per_kvb_to_fee_rate(amount: Amount) -> FeeRate {
        FeeRate::from_sat_per_kwu(amount.to_sat().div_ceil(4))
    }

    fn resolve_fee_rate(
        smart_fee: Option<Amount>,
        mempool_min_fee: Amount,
        min_relay_tx_fee: Amount,
    ) -> FeeRate {
        const MIN_FLOOR_SAT_PER_KWU: u64 = 250; // 1 sat/vB
        let node_min_fee = Self::btc_per_kvb_to_fee_rate(mempool_min_fee)
            .max(Self::btc_per_kvb_to_fee_rate(min_relay_tx_fee));
        let floor = FeeRate::from_sat_per_kwu(MIN_FLOOR_SAT_PER_KWU);
        let smart = smart_fee.map(Self::btc_per_kvb_to_fee_rate);
        smart.unwrap_or(node_min_fee).max(node_min_fee).max(floor)
    }

    fn unix_timestamp_now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

impl fmt::Debug for RpcBlockchainAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RpcBlockchainAdapter")
            .field("rpc_url", &self.rpc_url)
            .field("rpc_user", &self.rpc_user)
            .field("rpc_password", &"[REDACTED]")
            .finish()
    }
}

impl BlockchainPort for RpcBlockchainAdapter {
    fn fetch_tip_anchor(
        &self,
        expected_genesis: BlockHash,
    ) -> Result<Vec<BlockId>, BlockchainError> {
        const MAX_ANCHOR_HEADERS: usize = 10;
        let client = self.client()?;
        Self::verify_genesis(&client, expected_genesis)?;

        let mut hash = client
            .get_best_block_hash()
            .map_err(|error| BlockchainError::Rpc(error.to_string()))?;
        let mut block_ids = Vec::with_capacity(MAX_ANCHOR_HEADERS);

        for _ in 0..MAX_ANCHOR_HEADERS {
            let header = client
                .get_block_header_info(&hash)
                .map_err(|error| BlockchainError::Rpc(error.to_string()))?;
            let height = u32::try_from(header.height).map_err(|error| {
                BlockchainError::Rpc(format!("invalid block height {}: {error}", header.height))
            })?;
            block_ids.push(BlockId {
                height,
                hash: header.hash,
            });
            if height == 0 {
                break;
            }
            hash = header.previous_block_hash.ok_or_else(|| {
                BlockchainError::Rpc(format!(
                    "block header at height {height} is missing previous_block_hash"
                ))
            })?;
        }

        block_ids.reverse();
        Ok(block_ids)
    }

    fn sync(&self, wallet: &mut DnsNostrWallet) -> Result<(), BlockchainError> {
        let client = self.client()?;
        Self::verify_genesis(&client, wallet.genesis_hash())?;

        let mut emitter = Emitter::new(
            &client,
            wallet.latest_checkpoint(),
            0,
            std::iter::empty::<Arc<Transaction>>(),
        );
        while let Some(emission) = emitter
            .next_block()
            .map_err(|error| BlockchainError::Rpc(error.to_string()))?
        {
            wallet
                .apply_block_connected_to(
                    &emission.block,
                    emission.block_height(),
                    emission.connected_to(),
                )
                .map_err(|error| BlockchainError::ChainUpdate(error.to_string()))?;
        }

        let unconfirmed = wallet.unconfirmed_txids();
        if unconfirmed.is_empty() {
            return Ok(());
        }

        let evicted_at = Self::unix_timestamp_now();
        let mut evicted = Vec::new();
        for txid in unconfirmed {
            match client.get_mempool_entry(&txid) {
                Ok(_) => {}
                Err(error) if Self::is_not_in_mempool_error(&error) => {
                    evicted.push((txid, evicted_at));
                }
                Err(error) => return Err(BlockchainError::Rpc(error.to_string())),
            }
        }
        if !evicted.is_empty() {
            wallet.apply_evicted_txs(evicted);
        }
        Ok(())
    }

    fn estimate_fee_rate(&self) -> Result<FeeRate, BlockchainError> {
        const CONFIRMATION_TARGET_BLOCKS: u16 = 2;
        let client = self.client()?;
        let smart_fee = client
            .estimate_smart_fee(CONFIRMATION_TARGET_BLOCKS, Some(EstimateMode::Economical))
            .map_err(|error| BlockchainError::Rpc(error.to_string()))?;
        let mempool_info = client
            .get_mempool_info()
            .map_err(|error| BlockchainError::Rpc(error.to_string()))?;

        Ok(Self::resolve_fee_rate(
            smart_fee.fee_rate,
            mempool_info.mempool_min_fee,
            mempool_info.min_relay_tx_fee,
        ))
    }

    fn broadcast(&self, tx: &Transaction) -> Result<Txid, BlockchainError> {
        let client = self.client()?;
        client
            .send_raw_transaction(tx)
            .map_err(|error| BlockchainError::Broadcast(error.to_string()))
    }
}

#[cfg(test)]
mod test_rpc_blockchain_adapter {
    use super::*;
    use bitcoin::hashes::Hash;

    #[test]
    fn btc_per_kvb_conversion_preserves_fractional_precision_and_rounds_up() {
        // 1.5 sat/vB = 1,500 sat/kvB -> 375 sat/kwu exact
        assert_eq!(
            RpcBlockchainAdapter::btc_per_kvb_to_fee_rate(Amount::from_sat(1_500)),
            FeeRate::from_sat_per_kwu(375)
        );
        // 1,501 sat/kvB -> ceil(1501 / 4) = 376 sat/kwu
        assert_eq!(
            RpcBlockchainAdapter::btc_per_kvb_to_fee_rate(Amount::from_sat(1_501)),
            FeeRate::from_sat_per_kwu(376)
        );
        // 1,001 sat/kvB -> ceil(1001 / 4) = 251 sat/kwu
        assert_eq!(
            RpcBlockchainAdapter::btc_per_kvb_to_fee_rate(Amount::from_sat(1_001)),
            FeeRate::from_sat_per_kwu(251)
        );
    }

    #[test]
    fn resolve_fee_rate_applies_smart_fee_node_min_and_floor_precedence() {
        // Smart fee higher than node minimums and floor wins.
        let rate = RpcBlockchainAdapter::resolve_fee_rate(
            Some(Amount::from_sat(5_000)),
            Amount::from_sat(1_000),
            Amount::from_sat(1_000),
        );
        assert_eq!(rate, FeeRate::from_sat_per_kwu(1_250));

        // Missing smart fee falls back to max(mempool_min_fee, min_relay_tx_fee).
        let rate = RpcBlockchainAdapter::resolve_fee_rate(
            None,
            Amount::from_sat(2_000),
            Amount::from_sat(3_000),
        );
        assert_eq!(rate, FeeRate::from_sat_per_kwu(750));

        // Node min overrides lower smart fee.
        let rate = RpcBlockchainAdapter::resolve_fee_rate(
            Some(Amount::from_sat(1_200)),
            Amount::from_sat(2_400),
            Amount::from_sat(1_000),
        );
        assert_eq!(rate, FeeRate::from_sat_per_kwu(600));

        // 1 sat/vB (250 sat/kwu) floor applies when all inputs are below floor.
        let rate = RpcBlockchainAdapter::resolve_fee_rate(
            None,
            Amount::from_sat(100),
            Amount::from_sat(200),
        );
        assert_eq!(rate, FeeRate::from_sat_per_kwu(250));
    }

    #[test]
    fn classifies_minus_five_rpc_error_as_not_in_mempool_and_propagates_others() {
        let not_in_mempool = bitcoincore_rpc::Error::JsonRpc(jsonrpc::Error::Rpc(
            jsonrpc::error::RpcError {
                code: -5,
                message: "Transaction not in mempool".to_string(),
                data: None,
            },
        ));
        assert!(RpcBlockchainAdapter::is_not_in_mempool_error(
            &not_in_mempool
        ));

        let other_rpc = bitcoincore_rpc::Error::JsonRpc(jsonrpc::Error::Rpc(
            jsonrpc::error::RpcError {
                code: -28,
                message: "Loading block index".to_string(),
                data: None,
            },
        ));
        assert!(!RpcBlockchainAdapter::is_not_in_mempool_error(&other_rpc));

        let transport = bitcoincore_rpc::Error::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "connection refused",
        ));
        assert!(!RpcBlockchainAdapter::is_not_in_mempool_error(&transport));
    }

    #[test]
    fn debug_redacts_rpc_password() {
        let adapter = RpcBlockchainAdapter::new(
            "http://127.0.0.1:18443",
            "rpcuser",
            "super-secret-password",
        );
        let formatted = format!("{adapter:?}");
        assert!(formatted.contains("[REDACTED]"), "{formatted}");
        assert!(!formatted.contains("super-secret-password"), "{formatted}");
    }

    #[test]
    fn check_genesis_rejects_mismatch_and_formats_both_hashes() {
        let expected = BlockHash::from_byte_array([0x11; 32]);
        let actual = BlockHash::from_byte_array([0x22; 32]);
        assert_eq!(RpcBlockchainAdapter::check_genesis(expected, expected), Ok(()));

        let err = RpcBlockchainAdapter::check_genesis(expected, actual).unwrap_err();
        assert_eq!(err, BlockchainError::GenesisMismatch { expected, actual });
        let display = err.to_string();
        assert!(display.contains(&expected.to_string()), "{display}");
        assert!(display.contains(&actual.to_string()), "{display}");
    }
}
