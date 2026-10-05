use crate::{inscription::Bytes, name_token::NameToken};
use async_trait::async_trait;
use bitcoin::OutPoint;

/// Port through which the domain reads and persists Name-Tokens.
/// Implemented by the outer layers (e.g. a SQLite adapter).
#[async_trait]
pub trait NameTokenRepository: Send + Sync {
    async fn get_name_token_by_outpoint(&self, outpoint: OutPoint) -> Option<NameToken>;

    async fn get_name_tokens_by_label(&self, label: &Bytes) -> Vec<NameToken>;

    /// Atomically stores the updated Name-Tokens of a block and advances the
    /// next block height to `blockheight + 1`.
    async fn save_block_updates(&self, blockheight: u64, updated_name_tokens: &[NameToken]);

    /// Height of the next block to be applied; `0` when nothing was applied yet.
    async fn get_next_block_height(&self) -> u64;
}
