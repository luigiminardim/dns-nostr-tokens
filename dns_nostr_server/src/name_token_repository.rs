use crate::name_token::{Bytes, Inscription, InscriptionMetadata, NameToken};
use bitcoin::{
    hex::{Case, DisplayHex, FromHex},
    Block, OutPoint, Transaction, TxIn, TxOut, Txid,
};
use bitcoincore_rpc::RpcApi;
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

const MIN_CONFIRMATIONS: u64 = 6;

#[derive(Clone)]
pub struct NameTokenRepository {
    database: NameTokensDatabase,
}

impl NameTokenRepository {
    pub async fn create() -> Self {
        let database = NameTokensDatabase::create().await;
        let this = Self { database };
        let this_clone = this.clone();
        tokio::spawn(async move {
            this_clone.watch_blockchain().await;
        });
        this
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
            tokio::time::sleep(Duration::from_secs(
                600, // 10 minutes
            ))
            .await;
        }
    }

    // This function would typically sync the repository state with the current state of the blockchain.
    async fn sync_blocks(&self) {
        println!("Syncing blocks...");
        loop {
            let state_next_blockheight = self.database.get_next_block_height().await;
            let blockchain_num_blocks = self
                .bitcoin_client()
                .get_blockchain_info()
                .expect("Failed to get blockchain info")
                .blocks;
            if state_next_blockheight >= blockchain_num_blocks - MIN_CONFIRMATIONS {
                break;
            }
            self.sync_next_block(state_next_blockheight).await;
        }
    }

    async fn sync_next_block(&self, next_blockheight: u64) {
        let block_hash = self
            .bitcoin_client()
            .get_block_hash(next_blockheight)
            .expect("Failed to get block hash");
        let block = self
            .bitcoin_client()
            .get_block(&block_hash)
            .expect("Failed to get block");
        self.sync_block(next_blockheight, &block).await;
    }

    async fn sync_block(&self, blockheight: u64, block: &Block) {
        let mut pending_block_updates = HashMap::new();
        for (blockindex, transaction) in block.txdata.iter().enumerate() {
            self.sync_transaction(
                transaction,
                blockindex,
                blockheight,
                &mut pending_block_updates,
            )
            .await;
        }
        let updates: Vec<NameToken> = pending_block_updates.values().cloned().collect();
        self.database
            .save_block_updates(blockheight, &updates)
            .await;
        println!(
            "Synced block at height {} with {} updates",
            blockheight,
            updates.len()
        );
    }

    async fn sync_transaction(
        &self,
        transaction: &Transaction,
        blockindex: usize,
        blockheight: u64,
        mut pending_block_updates: &mut HashMap<OutPoint, NameToken>,
    ) {
        let num_positional_correlation =
            usize::max(transaction.input.len(), transaction.output.len());
        for positional_correlation in 0..num_positional_correlation {
            let txin = transaction.input.get(positional_correlation);
            let txout = transaction.output.get(positional_correlation);
            let metadata = InscriptionMetadata {
                txid: transaction.compute_txid(),
                vout: positional_correlation as u32,
                blockheight,
                blockindex,
            };
            self.sync_txin_txout_positional_correlation(
                txin,
                txout,
                metadata,
                &mut pending_block_updates,
            )
            .await;
        }
    }

    async fn sync_txin_txout_positional_correlation(
        &self,
        txin: Option<&TxIn>,
        txout: Option<&TxOut>,
        metadata: InscriptionMetadata,
        pending_block_updates: &mut HashMap<OutPoint, NameToken>,
    ) {
        let input_name_token = match txin {
            None => None,
            Some(txin) => {
                self.get_name_token_by_outpoint(txin.previous_output, &pending_block_updates)
                    .await
            }
        };
        let output_inscription = match txout {
            None => None,
            Some(txout) => Inscription::from_txout(txout),
        };
        let updated_name_tokens = NameToken::generate_name_token_updates(
            input_name_token.as_ref(),
            output_inscription.as_ref(),
            metadata,
        );
        for updated_name_token in updated_name_tokens {
            pending_block_updates.insert(
                updated_name_token.last_outpoint(),
                updated_name_token.clone(),
            );
        }
    }

    async fn get_name_token_by_outpoint(
        &self,
        outpoint: OutPoint,
        pending_block_updates: &HashMap<OutPoint, NameToken>,
    ) -> Option<NameToken> {
        match pending_block_updates.get(&outpoint) {
            Some(name_token) => Some(name_token.clone()),
            None => self.database.get_name_token_by_outpoint(outpoint).await,
        }
    }

    pub async fn get_name_token(&self, label: &Bytes) -> Option<NameToken> {
        let name_tokens_with_label = self.database.get_name_tokens_by_label(label).await;
        let valid_name_token = NameToken::select_valid_name_token(label, &name_tokens_with_label);
        valid_name_token.cloned()
    }
}

#[derive(Clone)]
struct NameTokensDatabase {
    connection: Arc<Mutex<rusqlite::Connection>>,
}

impl NameTokensDatabase {
    pub async fn create() -> Self {
        let sqlite =
            rusqlite::Connection::open("./data/name-tokens.sqlite").expect("Failed to open SQLite database");
        let this = Self {
            connection: Arc::new(Mutex::new(sqlite)),
        };
        this.create_tables().await;
        this
    }

    async fn create_tables(&self) {
        let mut connection = self.connection.lock().unwrap();
        let transaction = connection.transaction().unwrap();
        transaction
            .execute(
                "CREATE TABLE IF NOT EXISTS state (
                next_block_height UNSIGNED INTEGER NOT NULL
            )",
                [],
            )
            .unwrap();
        transaction
            .execute(
                "CREATE TABLE IF NOT EXISTS name_tokens (
                label_hex TEXT NOT NULL,
                first_blockheight UNSIGNED INTEGER NOT NULL,
                first_blockindex UNSIGNED INTEGER NOT NULL,
                first_vout UNSIGNED INTEGER NOT NULL,
                first_txid CHAR(64) NOT NULL,
                last_blockheight UNSIGNED INTEGER NOT NULL,
                last_blockindex UNSIGNED INTEGER NOT NULL,
                last_vout UNSIGNED INTEGER NOT NULL,
                last_txid CHAR(64) NOT NULL,
                inscription_json TEXT NOT NULL
            )",
                [],
            )
            .unwrap();
        transaction.commit().expect("Failed to create state table");
    }

    pub async fn get_next_block_height(&self) -> u64 {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection
            .prepare("SELECT next_block_height FROM state")
            .unwrap();
        let mut rows = statement.query([]).unwrap();
        rows.next().unwrap().map_or(0, |row| {
            row.get(0).expect("Failed to get next block height")
        })
    }

    pub async fn get_name_token_by_outpoint(&self, outpoint: OutPoint) -> Option<NameToken> {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection
            .prepare(
                "SELECT
                    label_hex,
                    first_blockheight,
                    first_blockindex,
                    first_vout,
                    first_txid,
                    last_blockheight,
                    last_blockindex,
                    last_vout,
                    last_txid,
                    inscription_json
                FROM name_tokens
                WHERE last_txid = ?1 AND last_vout = ?2",
            )
            .unwrap();
        let params = rusqlite::params![outpoint.txid.to_string(), outpoint.vout];
        let mut rows = statement.query(params).unwrap();
        let first_row = rows.next().unwrap();
        match first_row {
            None => None,
            Some(first_row) => {
                let label_hex: String = first_row.get(0).unwrap();
                let first_blockheight: u64 = first_row.get(1).unwrap();
                let first_blockindex: usize = first_row.get(2).unwrap();
                let first_vout: u32 = first_row.get(3).unwrap();
                let first_txid: String = first_row.get(4).unwrap();
                let last_blockheight: u64 = first_row.get(5).unwrap();
                let last_blockindex: usize = first_row.get(6).unwrap();
                let last_vout: u32 = first_row.get(7).unwrap();
                let last_txid: String = first_row.get(8).unwrap();
                let inscription_json: String = first_row.get(9).unwrap();
                Some(NameToken {
                    first_inscription_metadata: InscriptionMetadata {
                        txid: Txid::from_str(&first_txid).expect("Invalid Txid"),
                        vout: first_vout,
                        blockheight: first_blockheight,
                        blockindex: first_blockindex,
                    },
                    last_inscription_metadata: InscriptionMetadata {
                        txid: bitcoin::Txid::from_str(&last_txid).expect("Invalid Txid"),
                        vout: last_vout,
                        blockheight: last_blockheight,
                        blockindex: last_blockindex,
                    },
                    label: Bytes::from_hex(&label_hex).expect("Invalid label hex"),
                    inscription: Some(
                        serde_json::from_str::<Inscription>(&inscription_json)
                            .expect("Failed to parse inscription JSON"),
                    ),
                })
            }
        }
    }

    pub async fn save_block_updates<'a>(
        &self,
        blockheight: u64,
        updated_name_tokens: impl IntoIterator<Item = &'a NameToken>,
    ) {
        let next_block_height = blockheight + 1;
        let mut connection = self.connection.lock().unwrap();
        let transaction = connection.transaction().unwrap();
        // remove old block height
        transaction
            .execute("DELETE FROM state", [])
            .expect("Failed to delete old state");
        // insert new block height
        transaction
            .execute(
                "INSERT INTO state (next_block_height) VALUES (?1)",
                &[&next_block_height],
            )
            .expect("Failed to insert new block height");
        for updated_token in updated_name_tokens.into_iter() {
            transaction
                .execute(
                    "DELETE FROM name_tokens
                    WHERE first_blockheight = ?1 
                        AND first_blockindex = ?2
                        AND first_vout = ?3",
                    rusqlite::params![
                        &updated_token.first_inscription_metadata.blockheight,
                        &updated_token.first_inscription_metadata.blockindex,
                        &updated_token.first_inscription_metadata.vout,
                    ],
                )
                .expect("Failed to delete old name token");
            if updated_token.is_revoked() {
                continue; // Just remove revoked name tokens
            }
            transaction
                .execute(
                    "INSERT INTO name_tokens (
                        label_hex,
                        first_blockheight,
                        first_blockindex,
                        first_vout,
                        first_txid,
                        last_blockheight,
                        last_blockindex,
                        last_vout,
                        last_txid,
                        inscription_json
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    rusqlite::params![
                        updated_token.label.to_hex_string(Case::Lower),
                        updated_token.first_inscription_metadata.blockheight,
                        updated_token.first_inscription_metadata.blockindex,
                        updated_token.first_inscription_metadata.vout,
                        updated_token.first_inscription_metadata.txid.to_string(),
                        updated_token.last_inscription_metadata.blockheight,
                        updated_token.last_inscription_metadata.blockindex,
                        updated_token.last_inscription_metadata.vout,
                        updated_token.last_inscription_metadata.txid.to_string(),
                        serde_json::to_string(&updated_token.inscription)
                            .expect("Failed to serialize inscription"),
                    ],
                )
                .expect("Failed to insert new name token");
        }
        transaction.commit().expect("Failed to commit transaction");
    }

    pub async fn get_name_tokens_by_label(&self, _label: &Bytes) -> Vec<NameToken> {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection
            .prepare(
                "SELECT
                label_hex,
                first_blockheight,
                first_blockindex,
                first_vout,
                first_txid,
                last_blockheight,
                last_blockindex,
                last_vout,
                last_txid,
                inscription_json
            FROM name_tokens WHERE label_hex = ?1",
            )
            .unwrap();
        let params = rusqlite::params![&_label.to_hex_string(Case::Lower)];
        let name_tokens = statement
            .query_map(params, |row| {
                let label_hex: String = row.get(0).unwrap();
                let first_blockheight: u64 = row.get(1).unwrap();
                let first_blockindex: usize = row.get(2).unwrap();
                let first_vout: u32 = row.get(3).unwrap();
                let first_txid: String = row.get(4).unwrap();
                let last_blockheight: u64 = row.get(5).unwrap();
                let last_blockindex: usize = row.get(6).unwrap();
                let last_vout: u32 = row.get(7).unwrap();
                let last_txid: String = row.get(8).unwrap();
                let inscription_json: String = row.get(9).unwrap();
                Ok(NameToken {
                    first_inscription_metadata: InscriptionMetadata {
                        txid: Txid::from_str(&first_txid).expect("Invalid Txid"),
                        vout: first_vout,
                        blockheight: first_blockheight,
                        blockindex: first_blockindex,
                    },
                    last_inscription_metadata: InscriptionMetadata {
                        txid: bitcoin::Txid::from_str(&last_txid).expect("Invalid Txid"),
                        vout: last_vout,
                        blockheight: last_blockheight,
                        blockindex: last_blockindex,
                    },
                    label: Bytes::from_hex(&label_hex).expect("Invalid label hex"),
                    inscription: Some(
                        serde_json::from_str::<Inscription>(&inscription_json)
                            .expect("Failed to parse inscription JSON"),
                    ),
                })
            })
            .expect("Failed to query name tokens");
        name_tokens.filter_map(Result::ok).collect()
    }
}
