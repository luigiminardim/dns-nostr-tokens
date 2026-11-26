use crate::domain::dns_nostr_wallet::DnsNostrWallet;
use crate::domain::key_derivation_policy::KeyDerivationPolicy;
use crate::domain::name_token_wallet_repository::{
    NameTokenWalletRepository, WalletRepositoryError,
};
use bdk_wallet::chain::Merge;
use bdk_wallet::rusqlite::{
    params, Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
};
use bdk_wallet::{ChangeSet, KeychainKind};
use bip39::Mnemonic;
use bitcoin::bip32::Xpriv;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

enum Storage {
    Path(PathBuf),
    Connection(Mutex<Connection>),
}

/// SQLite-backed implementation of `NameTokenWalletRepository`.
///
/// Supports lazy path-backed operation (`open`) and injected-connection testing (`new`, `in_memory`).
/// Executes all writes across the `dns_nostr_wallet` mnemonic table and BDK's `bdk_*`
/// `ChangeSet` tables inside a single `rusqlite::Transaction`, and clears the wallet's
/// staged `ChangeSet` only after `Transaction::commit()` succeeds.
pub struct SqliteNameTokenWalletRepository {
    storage: Storage,
}

impl SqliteNameTokenWalletRepository {
    pub fn open(db_path: PathBuf) -> Self {
        Self {
            storage: Storage::Path(db_path),
        }
    }

    pub fn new(connection: Connection) -> Self {
        Self {
            storage: Storage::Connection(Mutex::new(connection)),
        }
    }

    pub fn in_memory() -> Result<Self, bdk_wallet::rusqlite::Error> {
        let mut conn = Connection::open_in_memory()?;
        {
            let tx = conn.transaction()?;
            Self::init_tables_raw(&tx)?;
            tx.commit()?;
        }
        Ok(Self::new(conn))
    }

    fn init_tables_raw(tx: &Transaction<'_>) -> Result<(), bdk_wallet::rusqlite::Error> {
        tx.execute(
            "CREATE TABLE IF NOT EXISTS dns_nostr_wallet (
                id INTEGER PRIMARY KEY CHECK (id = 0),
                mnemonic TEXT NOT NULL
            )",
            [],
        )?;
        ChangeSet::init_sqlite_tables(tx)?;
        Ok(())
    }

    fn init_tables(tx: &Transaction<'_>) -> Result<(), WalletRepositoryError> {
        Self::init_tables_raw(tx)
            .map_err(|error| WalletRepositoryError::Storage(error.to_string()))
    }

    fn map_read_error(error: bdk_wallet::rusqlite::Error) -> WalletRepositoryError {
        let message = error.to_string();
        if message.contains("no such table") {
            WalletRepositoryError::CorruptedState(message)
        } else {
            WalletRepositoryError::Storage(message)
        }
    }

    fn select_mnemonic(tx: &Transaction<'_>) -> Result<Option<String>, WalletRepositoryError> {
        tx.query_row(
            "SELECT mnemonic FROM dns_nostr_wallet WHERE id = 0",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(Self::map_read_error)
    }

    fn create_db_file(db_path: &Path) -> Result<Connection, WalletRepositoryError> {
        if let Some(parent) = db_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|error| {
                WalletRepositoryError::Storage(format!(
                    "could not create wallet directory `{}`: {error}",
                    parent.display()
                ))
            })?;
        }

        let mut options = OpenOptions::new();
        options.create(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(db_path).map_err(|error| {
            WalletRepositoryError::Storage(format!(
                "could not create wallet database `{}`: {error}",
                db_path.display()
            ))
        })?;

        Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_WRITE).map_err(|error| {
            WalletRepositoryError::Storage(format!(
                "could not open wallet database `{}`: {error}",
                db_path.display()
            ))
        })
    }

    fn open_existing_rw(db_path: &Path) -> Result<Connection, WalletRepositoryError> {
        Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_WRITE).map_err(|error| {
            WalletRepositoryError::Storage(format!(
                "could not open wallet database `{}`: {error}",
                db_path.display()
            ))
        })
    }

    fn create_in_conn(
        conn: &mut Connection,
        mnemonic: &Mnemonic,
        wallet: &mut DnsNostrWallet,
    ) -> Result<(), WalletRepositoryError> {
        let tx = conn
            .transaction()
            .map_err(|error| WalletRepositoryError::Storage(error.to_string()))?;

        Self::init_tables(&tx)?;

        let existing_mnemonic = Self::select_mnemonic(&tx)?;
        let existing_changeset = ChangeSet::from_sqlite(&tx)
            .map_err(|error| WalletRepositoryError::Storage(error.to_string()))?;
        if existing_mnemonic.is_some() || !existing_changeset.is_empty() {
            return Err(WalletRepositoryError::WalletAlreadyExists);
        }

        tx.execute(
            "INSERT INTO dns_nostr_wallet (id, mnemonic) VALUES (0, ?1)",
            params![mnemonic.to_string()],
        )
        .map_err(|error| WalletRepositoryError::Storage(error.to_string()))?;

        if let Some(changeset) = wallet.staged() {
            changeset
                .persist_to_sqlite(&tx)
                .map_err(|error| WalletRepositoryError::Storage(error.to_string()))?;
        }

        tx.commit()
            .map_err(|error| WalletRepositoryError::Storage(error.to_string()))?;
        wallet.take_staged();
        Ok(())
    }

    fn save_in_conn(
        conn: &mut Connection,
        wallet: &mut DnsNostrWallet,
    ) -> Result<(), WalletRepositoryError> {
        let Some(changeset) = wallet.staged() else {
            return Ok(());
        };
        if changeset.is_empty() {
            return Ok(());
        }

        let tx = conn
            .transaction()
            .map_err(|error| WalletRepositoryError::Storage(error.to_string()))?;

        if Self::select_mnemonic(&tx)?.is_none() {
            return Err(WalletRepositoryError::CorruptedState(
                "cannot save changeset: wallet mnemonic row is missing".into(),
            ));
        }

        changeset
            .persist_to_sqlite(&tx)
            .map_err(|error| WalletRepositoryError::Storage(error.to_string()))?;
        tx.commit()
            .map_err(|error| WalletRepositoryError::Storage(error.to_string()))?;
        wallet.take_staged();
        Ok(())
    }

    fn load_from_conn(conn: &mut Connection) -> Result<Option<DnsNostrWallet>, WalletRepositoryError> {
        const NO_PASSPHRASE: &str = "";
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| WalletRepositoryError::Storage(error.to_string()))?;

        let mnemonic_row = Self::select_mnemonic(&tx)?;
        let changeset = ChangeSet::from_sqlite(&tx).map_err(Self::map_read_error)?;

        match (mnemonic_row, changeset.is_empty()) {
            (None, true) => Ok(None),
            (Some(_), true) => Err(WalletRepositoryError::CorruptedState(
                "mnemonic row exists without BDK changeset".into(),
            )),
            (None, false) => Err(WalletRepositoryError::CorruptedState(
                "BDK changeset exists without mnemonic row".into(),
            )),
            (Some(mnemonic_str), false) => {
                let mnemonic = Mnemonic::parse(&mnemonic_str).map_err(|error| {
                    WalletRepositoryError::CorruptedState(format!(
                        "invalid stored mnemonic: {error}"
                    ))
                })?;

                let network = changeset.network.ok_or_else(|| {
                    WalletRepositoryError::CorruptedState("BDK changeset is missing network".into())
                })?;

                let seed = mnemonic.to_seed(NO_PASSPHRASE);
                let master_key = Xpriv::new_master(network, &seed).map_err(|error| {
                    WalletRepositoryError::CorruptedState(format!(
                        "failed to derive master key: {error}"
                    ))
                })?;

                let bdk_wallet = bdk_wallet::Wallet::load()
                    .descriptor(
                        KeychainKind::External,
                        Some(KeyDerivationPolicy::bitcoin_external_descriptor(
                            &master_key,
                        )),
                    )
                    .descriptor(
                        KeychainKind::Internal,
                        Some(KeyDerivationPolicy::bitcoin_internal_descriptor(
                            &master_key,
                        )),
                    )
                    .extract_keys()
                    .load_wallet_no_persist(changeset)
                    .map_err(|error| {
                        WalletRepositoryError::CorruptedState(format!(
                            "failed to load BDK wallet: {error}"
                        ))
                    })?
                    .ok_or_else(|| {
                        WalletRepositoryError::CorruptedState(
                            "BDK changeset did not produce a wallet".into(),
                        )
                    })?;

                Ok(Some(DnsNostrWallet::from_loaded(bdk_wallet, master_key)))
            }
        }
    }
}

impl NameTokenWalletRepository for SqliteNameTokenWalletRepository {
    fn create(
        &self,
        mnemonic: &Mnemonic,
        wallet: &mut DnsNostrWallet,
    ) -> Result<(), WalletRepositoryError> {
        match &self.storage {
            Storage::Path(db_path) => {
                let mut conn = Self::create_db_file(db_path)?;
                Self::create_in_conn(&mut conn, mnemonic, wallet)
            }
            Storage::Connection(mutex) => {
                let mut conn = mutex
                    .lock()
                    .map_err(|_| WalletRepositoryError::Storage("connection mutex poisoned".into()))?;
                Self::create_in_conn(&mut conn, mnemonic, wallet)
            }
        }
    }

    fn save(&self, wallet: &mut DnsNostrWallet) -> Result<(), WalletRepositoryError> {
        let Some(changeset) = wallet.staged() else {
            return Ok(());
        };
        if changeset.is_empty() {
            return Ok(());
        }

        match &self.storage {
            Storage::Path(db_path) => {
                if !db_path.exists() {
                    return Err(WalletRepositoryError::CorruptedState(format!(
                        "cannot save changeset: wallet database `{}` does not exist",
                        db_path.display()
                    )));
                }
                let mut conn = Self::open_existing_rw(db_path)?;
                Self::save_in_conn(&mut conn, wallet)
            }
            Storage::Connection(mutex) => {
                let mut conn = mutex
                    .lock()
                    .map_err(|_| WalletRepositoryError::Storage("connection mutex poisoned".into()))?;
                Self::save_in_conn(&mut conn, wallet)
            }
        }
    }

    fn load(&self) -> Result<Option<DnsNostrWallet>, WalletRepositoryError> {
        match &self.storage {
            Storage::Path(db_path) => {
                if !db_path.exists() {
                    return Ok(None);
                }
                let mut conn = Self::open_existing_rw(db_path)?;
                Self::load_from_conn(&mut conn)
            }
            Storage::Connection(mutex) => {
                let mut conn = mutex
                    .lock()
                    .map_err(|_| WalletRepositoryError::Storage("connection mutex poisoned".into()))?;
                Self::load_from_conn(&mut conn)
            }
        }
    }
}

#[cfg(test)]
mod test_sqlite_name_token_wallet_repository {
    use super::*;
    use bitcoin::Network;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon \
                            abandon abandon abandon abandon abandon about";
    const MAINNET_EXTERNAL_0: &str = "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu";
    const MAINNET_EXTERNAL_1: &str = "bc1qnjg0jd8228aq7egyzacy8cys3knf9xvrerkf9g";
    const MAINNET_EXTERNAL_2: &str = "bc1qp59yckz4ae5c4efgw2s5wfyvrz0ala7rgvuz8z";

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "dns_nostr_wallet_repo_test_{label}_{}_{}_{}",
                std::process::id(),
                nanos,
                seq
            ));
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sample_mnemonic() -> Mnemonic {
        Mnemonic::parse(MNEMONIC).unwrap()
    }

    #[test]
    fn load_returns_none_when_empty() {
        let repo = SqliteNameTokenWalletRepository::in_memory().unwrap();
        assert!(repo.load().unwrap().is_none());
    }

    #[test]
    fn create_persists_wallet_clears_staged_and_rejects_duplicate() {
        let repo = SqliteNameTokenWalletRepository::in_memory().unwrap();
        let mnemonic = sample_mnemonic();
        let mut wallet = DnsNostrWallet::new(&mnemonic, Network::Bitcoin).unwrap();
        assert!(wallet.staged().is_some());

        repo.create(&mnemonic, &mut wallet).unwrap();
        assert!(wallet.staged().is_none());

        let mut duplicate = DnsNostrWallet::new(&mnemonic, Network::Bitcoin).unwrap();
        let err = repo.create(&mnemonic, &mut duplicate).unwrap_err();
        assert_eq!(err, WalletRepositoryError::WalletAlreadyExists);
        // Failed transaction must not clear the duplicate wallet's staged changeset
        assert!(duplicate.staged().is_some());
    }

    #[test]
    fn save_persists_revealed_addresses_across_loads_and_clears_staged() {
        let repo = SqliteNameTokenWalletRepository::in_memory().unwrap();
        let mnemonic = sample_mnemonic();
        let mut wallet = DnsNostrWallet::new(&mnemonic, Network::Bitcoin).unwrap();
        repo.create(&mnemonic, &mut wallet).unwrap();

        let addr0 = wallet.reveal_next_bitcoin_address();
        assert_eq!(addr0.index, 0);
        assert_eq!(addr0.address.to_string(), MAINNET_EXTERNAL_0);
        assert!(wallet.staged().is_some());
        repo.save(&mut wallet).unwrap();
        assert!(wallet.staged().is_none());

        let addr1 = wallet.reveal_next_bitcoin_address();
        assert_eq!(addr1.index, 1);
        assert_eq!(addr1.address.to_string(), MAINNET_EXTERNAL_1);
        repo.save(&mut wallet).unwrap();

        let mut loaded = repo.load().unwrap().expect("wallet should load");
        let addr2 = loaded.reveal_next_bitcoin_address();
        assert_eq!(addr2.index, 2);
        assert_eq!(addr2.address.to_string(), MAINNET_EXTERNAL_2);
    }

    #[test]
    fn save_fails_when_uninitialized_and_preserves_staged_changeset() {
        let repo = SqliteNameTokenWalletRepository::in_memory().unwrap();
        let mnemonic = sample_mnemonic();
        let mut wallet = DnsNostrWallet::new(&mnemonic, Network::Bitcoin).unwrap();

        let err = repo.save(&mut wallet).unwrap_err();
        assert!(matches!(err, WalletRepositoryError::CorruptedState(_)));
        assert!(wallet.staged().is_some());
        assert!(repo.load().unwrap().is_none());
    }

    #[test]
    fn load_detects_corrupted_state_when_mnemonic_exists_without_bdk_changeset() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE dns_nostr_wallet (
                id INTEGER PRIMARY KEY CHECK (id = 0),
                mnemonic TEXT NOT NULL
            )",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO dns_nostr_wallet (id, mnemonic) VALUES (0, ?1)",
            params![MNEMONIC],
        )
        .unwrap();

        let repo = SqliteNameTokenWalletRepository::new(conn);
        let err = repo.load().unwrap_err();
        assert!(matches!(err, WalletRepositoryError::CorruptedState(_)));
    }

    #[test]
    fn load_detects_corrupted_state_when_bdk_changeset_exists_without_mnemonic() {
        let mnemonic = sample_mnemonic();
        let wallet = DnsNostrWallet::new(&mnemonic, Network::Bitcoin).unwrap();
        let mut conn = Connection::open_in_memory().unwrap();
        {
            let tx = conn.transaction().unwrap();
            ChangeSet::init_sqlite_tables(&tx).unwrap();
            wallet.staged().unwrap().persist_to_sqlite(&tx).unwrap();
            tx.commit().unwrap();
        }

        let repo = SqliteNameTokenWalletRepository::new(conn);
        let err = repo.load().unwrap_err();
        assert!(matches!(err, WalletRepositoryError::CorruptedState(_)));
    }

    #[test]
    fn load_detects_corrupted_state_when_stored_mnemonic_is_invalid() {
        let repo = SqliteNameTokenWalletRepository::in_memory().unwrap();
        let mnemonic = sample_mnemonic();
        let mut wallet = DnsNostrWallet::new(&mnemonic, Network::Bitcoin).unwrap();
        repo.create(&mnemonic, &mut wallet).unwrap();

        if let Storage::Connection(ref mutex) = repo.storage {
            let conn = mutex.lock().unwrap();
            conn.execute(
                "UPDATE dns_nostr_wallet SET mnemonic = 'not a valid mnemonic' WHERE id = 0",
                [],
            )
            .unwrap();
        } else {
            panic!("expected in-memory connection storage");
        }

        let err = repo.load().unwrap_err();
        assert!(matches!(err, WalletRepositoryError::CorruptedState(_)));
    }

    #[test]
    fn path_backed_load_on_missing_path_returns_none_without_creating_files() {
        let dir = TempDir::new("missing_load");
        let db_path = dir.path().join("wallet.sqlite");
        let repo = SqliteNameTokenWalletRepository::open(db_path.clone());

        assert!(repo.load().unwrap().is_none());
        assert!(!dir.path().exists(), "wallet_dir must not be created on load");
        assert!(!db_path.exists(), "wallet.sqlite must not be created on load");
    }

    #[test]
    fn path_backed_load_on_clean_wallet_leaves_file_unchanged() {
        let dir = TempDir::new("clean_load");
        let db_path = dir.path().join("wallet.sqlite");
        let repo = SqliteNameTokenWalletRepository::open(db_path.clone());
        let mnemonic = sample_mnemonic();
        let mut wallet = DnsNostrWallet::new(&mnemonic, Network::Bitcoin).unwrap();
        repo.create(&mnemonic, &mut wallet).unwrap();

        let bytes_before = fs::read(&db_path).unwrap();
        let loaded = repo.load().unwrap();
        assert!(loaded.is_some());
        let bytes_after = fs::read(&db_path).unwrap();
        assert_eq!(bytes_before, bytes_after);
    }

    #[test]
    fn path_backed_create_sets_0600_and_rejects_duplicate_without_altering_file() {
        let dir = TempDir::new("create_0600");
        let db_path = dir.path().join("wallet.sqlite");
        let repo = SqliteNameTokenWalletRepository::open(db_path.clone());
        let mnemonic = sample_mnemonic();
        let mut wallet = DnsNostrWallet::new(&mnemonic, Network::Bitcoin).unwrap();

        repo.create(&mnemonic, &mut wallet).unwrap();
        assert!(db_path.is_file());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&db_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let bytes_before = fs::read(&db_path).unwrap();
        let mut duplicate = DnsNostrWallet::new(&mnemonic, Network::Bitcoin).unwrap();
        let err = repo.create(&mnemonic, &mut duplicate).unwrap_err();
        assert_eq!(err, WalletRepositoryError::WalletAlreadyExists);
        let bytes_after = fs::read(&db_path).unwrap();
        assert_eq!(bytes_before, bytes_after);
    }

    #[test]
    fn reloaded_wallet_restores_signers_and_signs_transaction() {
        use bitcoin::absolute::LockTime;
        use bitcoin::block::{Header, Version as BlockVersion};
        use bitcoin::hashes::Hash;
        use bitcoin::transaction::Version as TxVersion;
        use bitcoin::{
            Amount, Block, CompactTarget, FeeRate, OutPoint, ScriptBuf, Sequence, Transaction,
            TxIn, TxMerkleNode, TxOut, Txid, Witness,
        };

        let repo = SqliteNameTokenWalletRepository::in_memory().unwrap();
        let mnemonic = sample_mnemonic();
        let mut wallet = DnsNostrWallet::new(&mnemonic, Network::Regtest).unwrap();
        repo.create(&mnemonic, &mut wallet).unwrap();

        let addr = wallet.reveal_next_bitcoin_address().address;
        let genesis = wallet.latest_checkpoint().block_id();
        let fund_tx = Transaction {
            version: TxVersion::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint {
                    txid: Txid::from_byte_array([0x11; 32]),
                    vout: 0,
                },
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: Amount::from_btc(1.0).unwrap(),
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
        wallet.apply_block_connected_to(&block1, 1, genesis).unwrap();
        repo.save(&mut wallet).unwrap();

        let mut reloaded = repo.load().unwrap().expect("wallet should reload");
        let recipient = reloaded.reveal_next_bitcoin_address().address;
        let signed = reloaded
            .build_and_sign_transaction(
                recipient,
                Amount::from_btc(0.25).unwrap(),
                FeeRate::from_sat_per_kwu(250),
            )
            .unwrap();
        assert!(!signed.input[0].witness.is_empty());
    }

    #[test]
    fn load_detects_corrupted_state_when_descriptors_do_not_match_mnemonic() {
        let repo = SqliteNameTokenWalletRepository::in_memory().unwrap();
        let mnemonic = sample_mnemonic();
        let mut wallet = DnsNostrWallet::new(&mnemonic, Network::Bitcoin).unwrap();
        repo.create(&mnemonic, &mut wallet).unwrap();

        let other_mnemonic =
            "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong";
        if let Storage::Connection(ref mutex) = repo.storage {
            let conn = mutex.lock().unwrap();
            conn.execute(
                "UPDATE dns_nostr_wallet SET mnemonic = ?1 WHERE id = 0",
                params![other_mnemonic],
            )
            .unwrap();
        } else {
            panic!("expected in-memory connection storage");
        }

        let err = repo.load().unwrap_err();
        assert!(matches!(err, WalletRepositoryError::CorruptedState(_)));
    }
}
