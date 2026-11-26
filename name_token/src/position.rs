use bdk_chain::BlockId;
use bitcoin::{BlockHash, OutPoint, Txid};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Position {
    block_id: BlockId,
    blockindex: usize,
    outpoint: OutPoint,
}

impl Position {
    pub fn new(block_id: BlockId, blockindex: usize, outpoint: OutPoint) -> Self {
        Self {
            block_id,
            blockindex,
            outpoint,
        }
    }

    pub fn block_id(&self) -> BlockId {
        self.block_id
    }

    pub fn block_height(&self) -> u32 {
        self.block_id.height
    }

    pub fn block_hash(&self) -> BlockHash {
        self.block_id.hash
    }

    pub fn blockindex(&self) -> usize {
        self.blockindex
    }

    pub fn outpoint(&self) -> OutPoint {
        self.outpoint
    }

    pub fn txid(&self) -> Txid {
        self.outpoint.txid
    }

    pub fn vout(&self) -> u32 {
        self.outpoint.vout
    }
}

#[cfg(test)]
mod test_position {
    use super::*;
    use bitcoin::{hashes::Hash, Txid};

    #[test]
    fn test_position_ordering() {
        let older = Position::new(
            BlockId { height: 0, hash: BlockHash::all_zeros() },
            0,
            OutPoint { txid: Txid::all_zeros(), vout: 0 },
        );
        let sorted_vec = vec![
            older.clone(),
            Position::new(
                older.block_id(),
                older.blockindex(),
                OutPoint { txid: older.txid(), vout: 1 },
            ),
            Position::new(
                older.block_id(),
                1,
                older.outpoint(),
            ),
            Position::new(
                BlockId { height: 1, hash: BlockHash::all_zeros() },
                older.blockindex(),
                older.outpoint(),
            ),
        ];
        let mut vec = sorted_vec.clone();
        vec.sort();
        assert_eq!(vec, sorted_vec);
    }
}
