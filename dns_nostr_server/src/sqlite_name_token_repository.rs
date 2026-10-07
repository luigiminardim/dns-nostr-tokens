use async_trait::async_trait;
use bdk_chain::BlockId;
use bitcoin::{
    hex::{Case, DisplayHex, FromHex},
    BlockHash, OutPoint,
};
use name_token::{Bytes, Label, NameToken, NameTokenEvent, NameTokenRepository};
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct SqliteNameTokenRepository {
    connection: Arc<Mutex<rusqlite::Connection>>,
}

impl SqliteNameTokenRepository {
    pub async fn create() -> Self {
        let sqlite = rusqlite::Connection::open("./data/name-tokens.sqlite")
            .expect("Failed to open SQLite database");
        let this = Self {
            connection: Arc::new(Mutex::new(sqlite)),
        };
        this.create_tables();
        this
    }

    fn create_tables(&self) {
        let mut connection = self.connection.lock().unwrap();
        let transaction = connection.transaction().unwrap();
        transaction
            .execute("DROP TABLE IF EXISTS state", [])
            .unwrap();
        transaction
            .execute("DROP TABLE IF EXISTS name_token_versions", [])
            .unwrap();

        transaction
            .execute(
                "CREATE TABLE IF NOT EXISTS chain_checkpoints (
                height UNSIGNED INTEGER NOT NULL,
                blockhash CHAR(64) NOT NULL
            )",
                [],
            )
            .unwrap();
        transaction
            .execute(
                "CREATE TABLE IF NOT EXISTS name_tokens (
                first_txid CHAR(64) NOT NULL,
                first_vout UNSIGNED INTEGER NOT NULL,
                label_hex TEXT NOT NULL,
                events_json TEXT NOT NULL,
                last_txid CHAR(64) NOT NULL,
                last_vout UNSIGNED INTEGER NOT NULL,
                PRIMARY KEY (first_txid, first_vout)
            )",
                [],
            )
            .unwrap();
        transaction.commit().expect("Failed to create tables");
    }
}

fn name_token_from_row(row: &rusqlite::Row) -> NameToken {
    let label_hex: String = row.get(0).unwrap();
    let events_json: String = row.get(1).unwrap();
    let events: Vec<NameTokenEvent> =
        serde_json::from_str(&events_json).expect("Failed to parse events JSON");
    NameToken::new(
        Label::from(Bytes::from_hex(&label_hex).expect("Invalid label hex")),
        events,
    )
}

#[async_trait]
impl NameTokenRepository for SqliteNameTokenRepository {
    async fn get_name_token_by_outpoint(&self, outpoint: OutPoint) -> Option<NameToken> {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection
            .prepare(
                "SELECT label_hex, events_json FROM name_tokens WHERE last_txid = ?1 AND last_vout = ?2",
            )
            .unwrap();
        let params = rusqlite::params![outpoint.txid.to_string(), outpoint.vout];
        let mut rows = statement.query(params).unwrap();
        rows.next().unwrap().map(|r| name_token_from_row(r))
    }

    async fn get_name_tokens_by_label(&self, label: &Label) -> Vec<NameToken> {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection
            .prepare("SELECT label_hex, events_json FROM name_tokens WHERE label_hex = ?1")
            .unwrap();
        let params = rusqlite::params![&label.as_ref().to_vec().to_hex_string(Case::Lower)];
        let name_tokens = statement
            .query_map(params, |row| Ok(name_token_from_row(row)))
            .expect("Failed to query name tokens");
        name_tokens.filter_map(Result::ok).collect()
    }

    async fn save_block_updates(
        &self,
        block_hash: BlockHash,
        height: u32,
        updated_name_tokens: &[NameToken],
    ) {
        let mut connection = self.connection.lock().unwrap();
        let transaction = connection.transaction().unwrap();

        transaction
            .execute(
                "INSERT INTO chain_checkpoints (height, blockhash) VALUES (?1, ?2)",
                rusqlite::params![height, block_hash.to_string()],
            )
            .expect("Failed to insert chain checkpoint");

        for updated_token in updated_name_tokens {
            let events_json = serde_json::to_string(&updated_token.events)
                .expect("Failed to serialize events");

            transaction
                .execute(
                    "INSERT INTO name_tokens (
                        first_txid,
                        first_vout,
                        label_hex,
                        events_json,
                        last_txid,
                        last_vout
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                    ON CONFLICT(first_txid, first_vout) DO UPDATE SET
                        events_json=excluded.events_json,
                        last_txid=excluded.last_txid,
                        last_vout=excluded.last_vout",
                    rusqlite::params![
                        updated_token.first_position().txid().to_string(),
                        updated_token.first_position().vout(),
                        updated_token.label.as_ref().to_vec().to_hex_string(Case::Lower),
                        events_json,
                        updated_token.last_position().txid().to_string(),
                        updated_token.last_position().vout(),
                    ],
                )
                .expect("Failed to upsert name token");
        }
        transaction.commit().expect("Failed to commit transaction");
    }

    async fn get_name_tokens_by_block(&self, block_id: BlockId) -> Vec<NameToken> {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection
            .prepare("SELECT label_hex, events_json FROM name_tokens")
            .unwrap();
        let mut rows = statement.query([]).unwrap();
        
        let mut affected = Vec::new();
        while let Some(row) = rows.next().unwrap() {
            let token = name_token_from_row(row);
            if token.last_position().block_id() == block_id {
                affected.push(token);
            }
        }
        affected
    }

    async fn undo_block_updates(&self, block_id: BlockId, updated_name_tokens: &[NameToken]) {
        let mut connection = self.connection.lock().unwrap();
        let transaction = connection.transaction().unwrap();

        let mut tokens_to_delete = Vec::new();
        {
            let mut statement = transaction
                .prepare("SELECT first_txid, first_vout, events_json FROM name_tokens")
                .unwrap();
            let mut rows = statement.query([]).unwrap();

            while let Some(row) = rows.next().unwrap() {
                let first_txid: String = row.get(0).unwrap();
                let first_vout: u32 = row.get(1).unwrap();
                let events_json: String = row.get(2).unwrap();
                let events: Vec<NameTokenEvent> = serde_json::from_str(&events_json).unwrap();
                
                if let Some(last_event) = events.last() {
                    if last_event.position().block_id() == block_id {
                        tokens_to_delete.push((first_txid, first_vout));
                    }
                }
            }
        }

        for (txid, vout) in tokens_to_delete {
            transaction
                .execute(
                    "DELETE FROM name_tokens WHERE first_txid = ?1 AND first_vout = ?2",
                    rusqlite::params![txid, vout],
                )
                .unwrap();
        }

        for updated_token in updated_name_tokens {
            let events_json = serde_json::to_string(&updated_token.events)
                .expect("Failed to serialize events");

            transaction
                .execute(
                    "INSERT INTO name_tokens (
                        first_txid,
                        first_vout,
                        label_hex,
                        events_json,
                        last_txid,
                        last_vout
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                    ON CONFLICT(first_txid, first_vout) DO UPDATE SET
                        events_json=excluded.events_json,
                        last_txid=excluded.last_txid,
                        last_vout=excluded.last_vout",
                    rusqlite::params![
                        updated_token.first_position().txid().to_string(),
                        updated_token.first_position().vout(),
                        updated_token.label.as_ref().to_vec().to_hex_string(Case::Lower),
                        events_json,
                        updated_token.last_position().txid().to_string(),
                        updated_token.last_position().vout(),
                    ],
                )
                .expect("Failed to upsert name token");
        }

        let hash_str = block_id.hash.to_string();
        transaction
            .execute(
                "DELETE FROM chain_checkpoints WHERE height = ?1 AND blockhash = ?2",
                rusqlite::params![block_id.height, hash_str],
            )
            .expect("Failed to undo chain checkpoint");

        transaction.commit().expect("Failed to commit undo");
    }

    async fn get_chain(&self) -> Vec<BlockId> {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection
            .prepare("SELECT height, blockhash FROM chain_checkpoints ORDER BY height ASC")
            .unwrap();
        let mut rows = statement.query([]).unwrap();
        let mut chain = Vec::new();
        while let Some(row) = rows.next().unwrap() {
            let height: u32 = row.get(0).unwrap();
            let blockhash_str: String = row.get(1).unwrap();
            let hash = BlockHash::from_str(&blockhash_str).unwrap();
            chain.push(BlockId { height, hash });
        }
        chain
    }
}
