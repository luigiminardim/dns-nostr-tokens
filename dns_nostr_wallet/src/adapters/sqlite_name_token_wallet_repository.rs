use crate::domain::dns_nostr_wallet::DnsNostrWallet;
use crate::domain::name_token_wallet_repository::{
    NameTokenWalletRepository, WalletRepositoryError,
};
use bdk_wallet::chain::Merge;
use bdk_wallet::rusqlite::{params, Connection, OptionalExtension, Transaction};
use bdk_wallet::ChangeSet;
use bip39::Mnemonic;
use bitcoin::bip32::Xpriv;
use std::sync::Mutex;

/// SQLite-backed implementation of `NameTokenWalletRepository`.
///
/// Executes all writes across the `dns_nostr_wallet` mnemonic table and BDK's `bdk_*`
/// `ChangeSet` tables inside a single `rusqlite::Transaction`, and clears the wallet's
/// staged `ChangeSet` only after `Transaction::commit()` succeeds.
pub struct SqliteNameTokenWalletRepository {
    connection: Mutex<Connection>,
}

impl SqliteNameTokenWalletRepository {
    pub fn new(connection: Connection) -> Self {
        Self {
            connection: Mutex::new(connection),
        }
    }

    pub fn in_memory() -> Result<Self, bdk_wallet::rusqlite::Error> {
        Ok(Self::new(Connection::open_in_memory()?))
    }

    fn init_tables(tx: &Transaction<'_>) -> Result<(), WalletRepositoryError> {
        tx.execute(
            "CREATE TABLE IF NOT EXISTS dns_nostr_wallet (
                id INTEGER PRIMARY KEY CHECK (id = 0),
                mnemonic TEXT NOT NULL
            )",
            [],
        )
        .map_err(|error| WalletRepositoryError::Storage(error.to_string()))?;
        ChangeSet::init_sqlite_tables(tx)
            .map_err(|error| WalletRepositoryError::Storage(error.to_string()))?;
        Ok(())
    }

    fn select_mnemonic(tx: &Transaction<'_>) -> Result<Option<String>, WalletRepositoryError> {
        tx.query_row(
            "SELECT mnemonic FROM dns_nostr_wallet WHERE id = 0",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| WalletRepositoryError::Storage(error.to_string()))
    }
}

impl NameTokenWalletRepository for SqliteNameTokenWalletRepository {
    fn create(
        &self,
        mnemonic: &Mnemonic,
        wallet: &mut DnsNostrWallet,
    ) -> Result<(), WalletRepositoryError> {
        let mut conn = self
            .connection
            .lock()
            .map_err(|_| WalletRepositoryError::Storage("connection mutex poisoned".into()))?;
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

    fn save(&self, wallet: &mut DnsNostrWallet) -> Result<(), WalletRepositoryError> {
        let Some(changeset) = wallet.staged() else {
            return Ok(());
        };
        if changeset.is_empty() {
            return Ok(());
        }

        let mut conn = self
            .connection
            .lock()
            .map_err(|_| WalletRepositoryError::Storage("connection mutex poisoned".into()))?;
        let tx = conn
            .transaction()
            .map_err(|error| WalletRepositoryError::Storage(error.to_string()))?;

        Self::init_tables(&tx)?;

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

    fn load(&self) -> Result<Option<DnsNostrWallet>, WalletRepositoryError> {
        const NO_PASSPHRASE: &str = "";
        let mut conn = self
            .connection
            .lock()
            .map_err(|_| WalletRepositoryError::Storage("connection mutex poisoned".into()))?;
        let tx = conn
            .transaction()
            .map_err(|error| WalletRepositoryError::Storage(error.to_string()))?;

        Self::init_tables(&tx)?;

        let mnemonic_row = Self::select_mnemonic(&tx)?;
        let changeset = ChangeSet::from_sqlite(&tx)
            .map_err(|error| WalletRepositoryError::Storage(error.to_string()))?;

        match (mnemonic_row, changeset.is_empty()) {
            (None, true) => {
                tx.commit()
                    .map_err(|error| WalletRepositoryError::Storage(error.to_string()))?;
                Ok(None)
            }
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

                let bdk_wallet = bdk_wallet::Wallet::load()
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

                let seed = mnemonic.to_seed(NO_PASSPHRASE);
                let master_key = Xpriv::new_master(network, &seed).map_err(|error| {
                    WalletRepositoryError::CorruptedState(format!(
                        "failed to derive master key: {error}"
                    ))
                })?;

                tx.commit()
                    .map_err(|error| WalletRepositoryError::Storage(error.to_string()))?;
                Ok(Some(DnsNostrWallet::from_loaded(bdk_wallet, master_key)))
            }
        }
    }
}

#[cfg(test)]
mod test_sqlite_name_token_wallet_repository {
    use super::*;
    use bitcoin::Network;

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon \
                            abandon abandon abandon abandon abandon about";
    const MAINNET_EXTERNAL_0: &str = "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu";
    const MAINNET_EXTERNAL_1: &str = "bc1qnjg0jd8228aq7egyzacy8cys3knf9xvrerkf9g";
    const MAINNET_EXTERNAL_2: &str = "bc1qp59yckz4ae5c4efgw2s5wfyvrz0ala7rgvuz8z";

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

        {
            let conn = repo.connection.lock().unwrap();
            conn.execute(
                "UPDATE dns_nostr_wallet SET mnemonic = 'not a valid mnemonic' WHERE id = 0",
                [],
            )
            .unwrap();
        }

        let err = repo.load().unwrap_err();
        assert!(matches!(err, WalletRepositoryError::CorruptedState(_)));
    }
}
