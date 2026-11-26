use crate::label::Label;
use crate::name_token::NameToken;
use crate::name_token_repository::NameTokenRepository;
use async_trait::async_trait;
use bdk_chain::BlockId;
use bitcoin::{BlockHash, OutPoint};
use std::sync::Mutex;

#[derive(Default)]
pub struct InMemoryNameTokenRepository {
    state: Mutex<InMemoryState>,
}

#[derive(Default)]
struct InMemoryState {
    chain: Vec<BlockId>,
    name_tokens: Vec<NameToken>,
}

#[async_trait]
impl NameTokenRepository for InMemoryNameTokenRepository {
    async fn get_name_token_by_outpoint(&self, outpoint: OutPoint) -> Option<NameToken> {
        let state = self.state.lock().unwrap();
        state
            .name_tokens
            .iter()
            .find(|name_token| name_token.last_outpoint() == outpoint)
            .cloned()
    }

    async fn get_name_tokens_by_label(&self, label: &Label) -> Vec<NameToken> {
        let state = self.state.lock().unwrap();
        state
            .name_tokens
            .iter()
            .filter(|nt| nt.label() == label)
            .cloned()
            .collect()
    }

    async fn save_block_updates(
        &self,
        block_hash: BlockHash,
        height: u32,
        updated_name_tokens: &[NameToken],
    ) {
        let mut state = self.state.lock().unwrap();
        for updated in updated_name_tokens {
            let outpoint = updated.first_position().outpoint();
            if let Some(pos) = state
                .name_tokens
                .iter()
                .position(|nt| nt.first_position().outpoint() == outpoint)
            {
                state.name_tokens[pos] = updated.clone();
            } else {
                state.name_tokens.push(updated.clone());
            }
        }
        state.chain.push(BlockId { height, hash: block_hash });
    }

    async fn get_name_tokens_by_block(&self, block_id: BlockId) -> Vec<NameToken> {
        let state = self.state.lock().unwrap();
        state
            .name_tokens
            .iter()
            .filter(|nt| nt.last_position().block_id() == block_id)
            .cloned()
            .collect()
    }

    async fn undo_block_updates(&self, block_id: BlockId, updated_name_tokens: &[NameToken]) {
        let mut state = self.state.lock().unwrap();
        
        state.chain.retain(|&id| id != block_id);
        state.name_tokens.retain(|nt| nt.last_position().block_id() != block_id);

        for updated in updated_name_tokens {
            let outpoint = updated.first_position().outpoint();
            if let Some(pos) = state
                .name_tokens
                .iter()
                .position(|nt| nt.first_position().outpoint() == outpoint)
            {
                state.name_tokens[pos] = updated.clone();
            } else {
                state.name_tokens.push(updated.clone());
            }
        }
    }

    async fn get_chain(&self) -> Vec<BlockId> {
        let state = self.state.lock().unwrap();
        state.chain.clone()
    }
}
