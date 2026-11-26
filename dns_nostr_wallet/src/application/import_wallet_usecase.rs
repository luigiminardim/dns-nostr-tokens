use crate::domain::blockchain_port::{BlockchainError, BlockchainPort};
use crate::domain::dns_nostr_wallet::{DnsNostrWallet, WalletError};
use crate::domain::name_token_wallet_repository::{
    NameTokenWalletRepository, WalletRepositoryError,
};
use crate::domain::sync_service::{SyncService, SyncServiceError};
use bip39::Mnemonic;
use bitcoin::Network;
use std::fmt;
use std::sync::Arc;

/// Use case: validate a user-supplied BIP39 mnemonic, scan confirmed blocks from height `0`,
/// and persist the wallet atomically via the repository.
pub struct ImportWalletUsecase {
    network: Network,
    repository: Arc<dyn NameTokenWalletRepository>,
    blockchain: Arc<dyn BlockchainPort>,
    sync_service: Arc<SyncService>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportWalletUsecaseError {
    Blockchain(BlockchainError),
    Sync(SyncServiceError),
    InvalidMnemonic(String),
    WalletAlreadyExists,
    Wallet(String),
}

impl ImportWalletUsecase {
    pub fn new(
        network: Network,
        repository: Arc<dyn NameTokenWalletRepository>,
        blockchain: Arc<dyn BlockchainPort>,
        sync_service: Arc<SyncService>,
    ) -> Self {
        Self {
            network,
            repository,
            blockchain,
            sync_service,
        }
    }

    pub fn execute(&self, phrase: &str) -> Result<(), ImportWalletUsecaseError> {
        let mnemonic = Self::parse_mnemonic(phrase)?;
        if self.repository.load()?.is_some() {
            return Err(ImportWalletUsecaseError::WalletAlreadyExists);
        }

        let mut wallet = DnsNostrWallet::new(&mnemonic, self.network)?;
        self.sync_service.sync(&mut wallet, &*self.blockchain)?;
        self.repository.create(&mnemonic, &mut wallet)?;
        Ok(())
    }

    fn parse_mnemonic(phrase: &str) -> Result<Mnemonic, ImportWalletUsecaseError> {
        let normalized = phrase
            .split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
            .join(" ");
        Mnemonic::parse(normalized)
            .map_err(|error| ImportWalletUsecaseError::InvalidMnemonic(error.to_string()))
    }
}

impl From<BlockchainError> for ImportWalletUsecaseError {
    fn from(error: BlockchainError) -> Self {
        Self::Blockchain(error)
    }
}

impl From<SyncServiceError> for ImportWalletUsecaseError {
    fn from(error: SyncServiceError) -> Self {
        Self::Sync(error)
    }
}

impl From<WalletError> for ImportWalletUsecaseError {
    fn from(error: WalletError) -> Self {
        Self::Wallet(error.to_string())
    }
}

impl From<WalletRepositoryError> for ImportWalletUsecaseError {
    fn from(error: WalletRepositoryError) -> Self {
        match error {
            WalletRepositoryError::WalletAlreadyExists => Self::WalletAlreadyExists,
            other => Self::Wallet(other.to_string()),
        }
    }
}

impl fmt::Display for ImportWalletUsecaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blockchain(error) => write!(formatter, "{error}"),
            Self::Sync(error) => write!(formatter, "{error}"),
            Self::InvalidMnemonic(error) => write!(formatter, "invalid seed phrase: {error}"),
            Self::WalletAlreadyExists => write!(formatter, "a wallet already exists"),
            Self::Wallet(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for ImportWalletUsecaseError {}

#[cfg(test)]
mod test_import_wallet_usecase {
    use super::*;
    use crate::adapters::sqlite_name_token_wallet_repository::SqliteNameTokenWalletRepository;
    use crate::domain::blockchain_port::FakeBlockchain;
    use bitcoin::absolute::LockTime;
    use bitcoin::block::{Header, Version as BlockVersion};
    use bitcoin::hashes::Hash;
    use bitcoin::transaction::Version as TxVersion;
    use bitcoin::{
        Amount, Block, CompactTarget, OutPoint, ScriptBuf, Sequence, Transaction, TxIn,
        TxMerkleNode, TxOut, Txid, Witness,
    };

    const VALID: &str = "abandon abandon abandon abandon abandon abandon \
                         abandon abandon abandon abandon abandon about";

    fn in_memory_repo() -> Arc<dyn NameTokenWalletRepository> {
        Arc::new(SqliteNameTokenWalletRepository::in_memory().unwrap())
    }

    fn import(
        repository: Arc<dyn NameTokenWalletRepository>,
        blockchain: Arc<FakeBlockchain>,
        phrase: &str,
    ) -> Result<(), ImportWalletUsecaseError> {
        ImportWalletUsecase::new(
            Network::Regtest,
            repository,
            blockchain,
            Arc::new(SyncService),
        )
        .execute(phrase)
    }

    fn assert_invalid(phrase: &str) {
        let repository = in_memory_repo();
        let blockchain = Arc::new(FakeBlockchain::new());
        let result = import(repository, Arc::clone(&blockchain), phrase);
        assert!(
            matches!(result, Err(ImportWalletUsecaseError::InvalidMnemonic(_))),
            "{result:?}"
        );
        assert_eq!(blockchain.sync_calls(), 0);
    }

    #[test]
    fn valid_mnemonic_is_stored() {
        let repository = in_memory_repo();
        let blockchain = Arc::new(FakeBlockchain::new());
        import(Arc::clone(&repository), Arc::clone(&blockchain), VALID).unwrap();
        assert_eq!(blockchain.sync_calls(), 1);
        assert!(repository.load().unwrap().is_some());
    }

    #[test]
    fn normalises_whitespace_and_case() {
        let repository = in_memory_repo();
        let blockchain = Arc::new(FakeBlockchain::new());
        let messy = format!("  {}\n", VALID.to_uppercase().replace(' ', "   "));
        import(Arc::clone(&repository), blockchain, &messy).unwrap();
        assert!(repository.load().unwrap().is_some());
    }

    #[test]
    fn rejects_invalid_mnemonics_with_zero_rpc_calls() {
        assert_invalid(&["abandon"; 12].join(" "));
        assert_invalid(&VALID.replace("about", "notaword"));
        assert_invalid(&["abandon"; 11].join(" "));
        assert_invalid("");
        assert_invalid("   \n ");
    }

    #[test]
    fn fails_when_wallet_exists_with_zero_rpc_calls() {
        let repository = in_memory_repo();
        let blockchain = Arc::new(FakeBlockchain::new());
        import(Arc::clone(&repository), Arc::clone(&blockchain), VALID).unwrap();
        assert_eq!(blockchain.sync_calls(), 1);

        let result = import(repository, Arc::clone(&blockchain), VALID);
        assert_eq!(result, Err(ImportWalletUsecaseError::WalletAlreadyExists));
        assert_eq!(blockchain.sync_calls(), 1);
    }

    #[test]
    fn sync_failure_does_not_persist_wallet() {
        let repository = in_memory_repo();
        let blockchain = Arc::new(FakeBlockchain::new());
        blockchain.set_sync_error(BlockchainError::Rpc("rpc down".into()));

        let err = import(Arc::clone(&repository), blockchain, VALID).unwrap_err();
        assert_eq!(
            err,
            ImportWalletUsecaseError::Sync(SyncServiceError::Blockchain(BlockchainError::Rpc(
                "rpc down".into()
            )))
        );
        assert!(repository.load().unwrap().is_none());
    }

    #[test]
    fn historical_confirmed_utxos_synced_during_import_persist_atomically() {
        let repository = in_memory_repo();
        let blockchain = Arc::new(FakeBlockchain::new());
        blockchain.set_sync_hook(|wallet| {
            let addr = wallet.reveal_next_bitcoin_address().address;
            let genesis = wallet.latest_checkpoint().block_id();
            let fund_tx = Transaction {
                version: TxVersion::TWO,
                lock_time: LockTime::ZERO,
                input: vec![TxIn {
                    previous_output: OutPoint {
                        txid: Txid::from_byte_array([0x33; 32]),
                        vout: 0,
                    },
                    script_sig: ScriptBuf::new(),
                    sequence: Sequence::MAX,
                    witness: Witness::new(),
                }],
                output: vec![TxOut {
                    value: Amount::from_btc(0.5).unwrap(),
                    script_pubkey: addr.script_pubkey(),
                }],
            };
            let block1 = Block {
                header: Header {
                    version: BlockVersion::TWO,
                    prev_blockhash: genesis.hash,
                    merkle_root: TxMerkleNode::all_zeros(),
                    time: 1_700_000_001,
                    bits: CompactTarget::from_consensus(0x207f_ffff),
                    nonce: 1,
                },
                txdata: vec![fund_tx],
            };
            wallet
                .apply_block_connected_to(&block1, 1, genesis)
                .map_err(|error| BlockchainError::ChainUpdate(error.to_string()))
        });

        import(Arc::clone(&repository), blockchain, VALID).unwrap();
        let loaded = repository.load().unwrap().expect("wallet should persist");
        assert_eq!(loaded.spendable_balance(), Amount::from_btc(0.5).unwrap());
    }
}
