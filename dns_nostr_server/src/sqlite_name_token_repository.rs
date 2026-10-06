use async_trait::async_trait;
use bitcoin::{
    hex::{Case, DisplayHex, FromHex},
    OutPoint, Txid,
};
use name_token::{Bytes, Inscription, NameTokenPosition, NameToken, NameTokenRepository};
use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};

const NAME_TOKEN_COLUMNS: &str = "label_hex,
    first_blockheight,
    first_blockindex,
    first_vout,
    first_txid,
    last_blockheight,
    last_blockindex,
    last_vout,
    last_txid,
    inscription_json";

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
}

fn name_token_from_row(row: &rusqlite::Row) -> NameToken {
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
    NameToken {
        first_position: NameTokenPosition {
            txid: Txid::from_str(&first_txid).expect("Invalid Txid"),
            vout: first_vout,
            blockheight: first_blockheight,
            blockindex: first_blockindex,
        },
        last_position: NameTokenPosition {
            txid: Txid::from_str(&last_txid).expect("Invalid Txid"),
            vout: last_vout,
            blockheight: last_blockheight,
            blockindex: last_blockindex,
        },
        label: Bytes::from_hex(&label_hex).expect("Invalid label hex"),
        inscription: Some(
            serde_json::from_str::<Inscription>(&inscription_json)
                .expect("Failed to parse inscription JSON"),
        ),
    }
}

#[async_trait]
impl NameTokenRepository for SqliteNameTokenRepository {
    async fn get_name_token_by_outpoint(&self, outpoint: OutPoint) -> Option<NameToken> {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection
            .prepare(&format!(
                "SELECT {NAME_TOKEN_COLUMNS}
                FROM name_tokens
                WHERE last_txid = ?1 AND last_vout = ?2"
            ))
            .unwrap();
        let params = rusqlite::params![outpoint.txid.to_string(), outpoint.vout];
        let mut rows = statement.query(params).unwrap();
        rows.next().unwrap().map(name_token_from_row)
    }

    async fn get_name_tokens_by_label(&self, label: &Bytes) -> Vec<NameToken> {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection
            .prepare(&format!(
                "SELECT {NAME_TOKEN_COLUMNS} FROM name_tokens WHERE label_hex = ?1"
            ))
            .unwrap();
        let params = rusqlite::params![&label.to_hex_string(Case::Lower)];
        let name_tokens = statement
            .query_map(params, |row| Ok(name_token_from_row(row)))
            .expect("Failed to query name tokens");
        name_tokens.filter_map(Result::ok).collect()
    }

    async fn save_block_updates(&self, blockheight: u64, updated_name_tokens: &[NameToken]) {
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
                [&next_block_height],
            )
            .expect("Failed to insert new block height");
        for updated_token in updated_name_tokens {
            transaction
                .execute(
                    "DELETE FROM name_tokens
                    WHERE first_blockheight = ?1 
                        AND first_blockindex = ?2
                        AND first_vout = ?3",
                    rusqlite::params![
                        &updated_token.first_position.blockheight,
                        &updated_token.first_position.blockindex,
                        &updated_token.first_position.vout,
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
                        updated_token.first_position.blockheight,
                        updated_token.first_position.blockindex,
                        updated_token.first_position.vout,
                        updated_token.first_position.txid.to_string(),
                        updated_token.last_position.blockheight,
                        updated_token.last_position.blockindex,
                        updated_token.last_position.vout,
                        updated_token.last_position.txid.to_string(),
                        serde_json::to_string(&updated_token.inscription)
                            .expect("Failed to serialize inscription"),
                    ],
                )
                .expect("Failed to insert new name token");
        }
        transaction.commit().expect("Failed to commit transaction");
    }

    async fn get_next_block_height(&self) -> u64 {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection
            .prepare("SELECT next_block_height FROM state")
            .unwrap();
        let mut rows = statement.query([]).unwrap();
        rows.next().unwrap().map_or(0, |row| {
            row.get(0).expect("Failed to get next block height")
        })
    }
}
