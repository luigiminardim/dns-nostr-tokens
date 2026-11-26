use crate::domain::dns_nostr_wallet::DnsNostrWallet;
use bdk_wallet::chain::BlockId;
use bitcoin::{BlockHash, FeeRate, Transaction, Txid};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockchainError {
    GenesisMismatch {
        expected: BlockHash,
        actual: BlockHash,
    },
    Rpc(String),
    ChainUpdate(String),
    Broadcast(String),
}

impl fmt::Display for BlockchainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GenesisMismatch { expected, actual } => write!(
                formatter,
                "Bitcoin node genesis block ({actual}) does not match configured network genesis ({expected})"
            ),
            Self::Rpc(reason) => write!(formatter, "Bitcoin RPC error: {reason}"),
            Self::ChainUpdate(reason) => write!(formatter, "wallet chain update failed: {reason}"),
            Self::Broadcast(reason) => write!(formatter, "transaction broadcast failed: {reason}"),
        }
    }
}

impl std::error::Error for BlockchainError {}

/// Synchronous blockchain port abstracting Bitcoin Core JSON-RPC operations.
pub trait BlockchainPort {
    /// Verifies the node's genesis hash and returns up to 10 linked headers ending at the active
    /// tip in ascending height order, without downloading full blocks.
    fn fetch_tip_anchor(&self, expected_genesis: BlockHash)
        -> Result<Vec<BlockId>, BlockchainError>;

    /// Verifies the node's genesis hash, streams confirmed blocks from the wallet's persisted
    /// checkpoint, and reconciles evicted wallet-originated unconfirmed transactions.
    fn sync(&self, wallet: &mut DnsNostrWallet) -> Result<(), BlockchainError>;

    /// Estimates a transaction fee rate with automatic fallbacks to node mempool minimums.
    fn estimate_fee_rate(&self) -> Result<FeeRate, BlockchainError>;

    /// Broadcasts a signed transaction and returns its `Txid`.
    fn broadcast(&self, tx: &Transaction) -> Result<Txid, BlockchainError>;
}

#[cfg(test)]
pub struct FakeBlockchain {
    state: std::sync::Mutex<FakeBlockchainState>,
}

#[cfg(test)]
type SyncHook = Box<dyn FnMut(&mut DnsNostrWallet) -> Result<(), BlockchainError> + Send>;

#[cfg(test)]
struct FakeBlockchainState {
    tip_anchor_calls: usize,
    sync_calls: usize,
    estimate_fee_rate_calls: usize,
    broadcast_calls: usize,
    tip_anchor_result: Result<Vec<BlockId>, BlockchainError>,
    sync_hook: Option<SyncHook>,
    fee_rate_result: Result<FeeRate, BlockchainError>,
    broadcast_error: Option<BlockchainError>,
    broadcasted_txs: Vec<Transaction>,
}

#[cfg(test)]
impl FakeBlockchain {
    pub fn new() -> Self {
        Self {
            state: std::sync::Mutex::new(FakeBlockchainState {
                tip_anchor_calls: 0,
                sync_calls: 0,
                estimate_fee_rate_calls: 0,
                broadcast_calls: 0,
                tip_anchor_result: Ok(Vec::new()),
                sync_hook: None,
                fee_rate_result: Ok(FeeRate::from_sat_per_kwu(250)),
                broadcast_error: None,
                broadcasted_txs: Vec::new(),
            }),
        }
    }

    pub fn set_tip_anchor_result(&self, result: Result<Vec<BlockId>, BlockchainError>) {
        self.state.lock().unwrap().tip_anchor_result = result;
    }

    pub fn set_sync_hook<F>(&self, hook: F)
    where
        F: FnMut(&mut DnsNostrWallet) -> Result<(), BlockchainError> + Send + 'static,
    {
        self.state.lock().unwrap().sync_hook = Some(Box::new(hook));
    }

    pub fn set_sync_error(&self, error: BlockchainError) {
        self.set_sync_hook(move |_| Err(error.clone()));
    }

    pub fn set_fee_rate_result(&self, result: Result<FeeRate, BlockchainError>) {
        self.state.lock().unwrap().fee_rate_result = result;
    }

    pub fn set_broadcast_error(&self, error: Option<BlockchainError>) {
        self.state.lock().unwrap().broadcast_error = error;
    }

    pub fn tip_anchor_calls(&self) -> usize {
        self.state.lock().unwrap().tip_anchor_calls
    }

    pub fn sync_calls(&self) -> usize {
        self.state.lock().unwrap().sync_calls
    }

    pub fn estimate_fee_rate_calls(&self) -> usize {
        self.state.lock().unwrap().estimate_fee_rate_calls
    }

    pub fn broadcast_calls(&self) -> usize {
        self.state.lock().unwrap().broadcast_calls
    }

    pub fn broadcasted_txs(&self) -> Vec<Transaction> {
        self.state.lock().unwrap().broadcasted_txs.clone()
    }
}

#[cfg(test)]
impl Default for FakeBlockchain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl BlockchainPort for FakeBlockchain {
    fn fetch_tip_anchor(
        &self,
        _expected_genesis: BlockHash,
    ) -> Result<Vec<BlockId>, BlockchainError> {
        let mut state = self.state.lock().unwrap();
        state.tip_anchor_calls += 1;
        state.tip_anchor_result.clone()
    }

    fn sync(&self, wallet: &mut DnsNostrWallet) -> Result<(), BlockchainError> {
        let mut state = self.state.lock().unwrap();
        state.sync_calls += 1;
        if let Some(hook) = state.sync_hook.as_mut() {
            hook(wallet)
        } else {
            Ok(())
        }
    }

    fn estimate_fee_rate(&self) -> Result<FeeRate, BlockchainError> {
        let mut state = self.state.lock().unwrap();
        state.estimate_fee_rate_calls += 1;
        state.fee_rate_result.clone()
    }

    fn broadcast(&self, tx: &Transaction) -> Result<Txid, BlockchainError> {
        let mut state = self.state.lock().unwrap();
        state.broadcast_calls += 1;
        if let Some(ref error) = state.broadcast_error {
            return Err(error.clone());
        }
        state.broadcasted_txs.push(tx.clone());
        Ok(tx.compute_txid())
    }
}
