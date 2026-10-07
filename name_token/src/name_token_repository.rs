use crate::{label::Label, name_token::NameToken};
use async_trait::async_trait;
use bdk_chain::BlockId;
use bitcoin::{BlockHash, OutPoint};

/// Port through which the domain reads and persists Name-Tokens.
/// Implemented by the outer layers (e.g. a SQLite adapter).
#[async_trait]
pub trait NameTokenRepository: Send + Sync {
    async fn get_name_token_by_outpoint(&self, outpoint: OutPoint) -> Option<NameToken>;

    async fn get_name_tokens_by_label(&self, label: &Label) -> Vec<NameToken>;

    /// Atomically stores the updated Name-Tokens of a block and adds it to the chain checkpoints.
    async fn save_block_updates(
        &self,
        block_hash: BlockHash,
        height: u32,
        updated_name_tokens: &[NameToken],
    );

    /// Removes all Name-Token events produced by a block and removes the block from chain checkpoints.
    async fn get_name_tokens_by_block(&self, block_id: BlockId) -> Vec<NameToken>;

    /// Atomically un-applies a block's updates by replacing them with `updated_name_tokens` and removes the block from chain checkpoints.
    async fn undo_block_updates(&self, block_id: BlockId, updated_name_tokens: &[NameToken]);

    /// Retrieves the list of chain checkpoints, ordered by height ascending.
    async fn get_chain(&self) -> Vec<BlockId>;
}
