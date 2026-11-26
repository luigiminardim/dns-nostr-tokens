use crate::domain::blockchain_port::{BlockchainError, BlockchainPort};
use crate::domain::dns_nostr_wallet::{DnsNostrWallet, WalletError};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncServiceError {
    Blockchain(BlockchainError),
    Wallet(String),
}

impl From<BlockchainError> for SyncServiceError {
    fn from(error: BlockchainError) -> Self {
        Self::Blockchain(error)
    }
}

impl From<WalletError> for SyncServiceError {
    fn from(error: WalletError) -> Self {
        Self::Wallet(error.to_string())
    }
}

impl fmt::Display for SyncServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blockchain(error) => write!(formatter, "{error}"),
            Self::Wallet(reason) => write!(formatter, "{reason}"),
        }
    }
}

impl std::error::Error for SyncServiceError {}

/// Domain service coordinating wallet blockchain synchronization and tip anchoring.
pub struct SyncService;

impl SyncService {
    /// Anchors a newly created wallet at the node's active tip without scanning historical blocks.
    pub fn tip_anchor(
        &self,
        wallet: &mut DnsNostrWallet,
        blockchain: &dyn BlockchainPort,
    ) -> Result<(), SyncServiceError> {
        let block_ids = blockchain.fetch_tip_anchor(wallet.genesis_hash())?;
        wallet.apply_tip_anchor(block_ids)?;
        Ok(())
    }

    /// Synchronizes confirmed blocks from the wallet's persisted checkpoint and reconciles
    /// evicted wallet-originated unconfirmed transactions.
    pub fn sync(
        &self,
        wallet: &mut DnsNostrWallet,
        blockchain: &dyn BlockchainPort,
    ) -> Result<(), SyncServiceError> {
        blockchain.sync(wallet)?;
        Ok(())
    }
}

#[cfg(test)]
mod test_sync_service {
    use super::*;
    use crate::domain::blockchain_port::FakeBlockchain;
    use bdk_wallet::chain::BlockId;
    use bip39::Mnemonic;
    use bitcoin::hashes::Hash;
    use bitcoin::{BlockHash, Network};

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon \
                            abandon abandon abandon abandon abandon about";

    fn new_regtest_wallet() -> DnsNostrWallet {
        let mnemonic = Mnemonic::parse(MNEMONIC).unwrap();
        DnsNostrWallet::new(&mnemonic, Network::Regtest).unwrap()
    }

    #[test]
    fn tip_anchor_applies_fetched_block_ids() {
        let mut wallet = new_regtest_wallet();
        let blockchain = FakeBlockchain::new();
        let tip = BlockId {
            height: 5,
            hash: BlockHash::from_byte_array([0x05; 32]),
        };
        blockchain.set_tip_anchor_result(Ok(vec![
            BlockId {
                height: 0,
                hash: wallet.genesis_hash(),
            },
            tip,
        ]));

        SyncService.tip_anchor(&mut wallet, &blockchain).unwrap();
        assert_eq!(blockchain.tip_anchor_calls(), 1);
        assert_eq!(wallet.latest_checkpoint().block_id(), tip);
    }

    #[test]
    fn tip_anchor_propagates_blockchain_and_wallet_errors() {
        let mut wallet = new_regtest_wallet();
        let blockchain = FakeBlockchain::new();
        blockchain.set_tip_anchor_result(Err(BlockchainError::Rpc("timeout".into())));

        let err = SyncService.tip_anchor(&mut wallet, &blockchain).unwrap_err();
        assert_eq!(
            err,
            SyncServiceError::Blockchain(BlockchainError::Rpc("timeout".into()))
        );

        let wrong_genesis = BlockHash::from_byte_array([0x99; 32]);
        blockchain.set_tip_anchor_result(Ok(vec![BlockId {
            height: 0,
            hash: wrong_genesis,
        }]));
        let err = SyncService.tip_anchor(&mut wallet, &blockchain).unwrap_err();
        assert!(matches!(err, SyncServiceError::Wallet(_)));
    }

    #[test]
    fn sync_delegates_to_blockchain_and_propagates_error() {
        let mut wallet = new_regtest_wallet();
        let blockchain = FakeBlockchain::new();
        SyncService.sync(&mut wallet, &blockchain).unwrap();
        assert_eq!(blockchain.sync_calls(), 1);

        let expected = wallet.genesis_hash();
        let actual = BlockHash::from_byte_array([0xAA; 32]);
        blockchain.set_sync_error(BlockchainError::GenesisMismatch { expected, actual });
        let err = SyncService.sync(&mut wallet, &blockchain).unwrap_err();
        assert_eq!(
            err,
            SyncServiceError::Blockchain(BlockchainError::GenesisMismatch { expected, actual })
        );
    }
}
