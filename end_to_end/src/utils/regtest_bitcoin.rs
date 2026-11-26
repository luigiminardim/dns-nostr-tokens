use super::just_command::JustCommand;
use bitcoin::{
    consensus::encode::{deserialize_hex, serialize_hex},
    Address, Amount, BlockHash, Denomination, Network, Transaction, Txid,
};
use std::str::FromStr;

pub struct RegtestBitcoinClient {
    just: JustCommand,
}

impl RegtestBitcoinClient {
    pub fn new(rpc_user: &str, rpc_password: &str) -> Result<Self, String> {
        let just = JustCommand::new(rpc_user, rpc_password)?;

        let raw_address = just.run(&["miner-address"])?;
        Self::parse_regtest_address(&raw_address)?;

        let raw_balance = just.run(&["miner-balance"])?;
        Self::parse_btc_amount(&raw_balance)?;

        Ok(Self { just })
    }

    pub fn fund_address(&self, address: &Address, amount: Amount) -> Result<Txid, String> {
        let btc = Self::format_btc_amount(amount);
        let addr_str = address.to_string();
        let raw_txid = self.just.run(&["miner-fund", &addr_str, &btc])?;
        Self::parse_txid(&raw_txid)
    }

    pub fn fund_address_unconfirmed(
        &self,
        address: &Address,
        amount: Amount,
    ) -> Result<Txid, String> {
        let btc = Self::format_btc_amount(amount);
        let addr_str = address.to_string();
        let raw_txid = self
            .just
            .run(&["miner-fund", &addr_str, &btc, "--no-mine"])?;
        Self::parse_txid(&raw_txid)
    }

    pub fn mine_one_block(&self) -> Result<(BlockHash, u64), String> {
        let raw_hash = self.just.run(&["miner-mineblock"])?;
        let block_hash = Self::parse_block_hash(&raw_hash)?;
        let raw_height = self.just.run(&["bitcoin-cli", "getblockcount"])?;
        let height = Self::parse_block_height(&raw_height)?;
        Ok((block_hash, height))
    }

    pub fn get_raw_transaction(
        &self,
        txid: &Txid,
        block_hash: &BlockHash,
    ) -> Result<Transaction, String> {
        let txid_str = txid.to_string();
        let block_hash_str = block_hash.to_string();
        let raw_hex = self.just.run(&[
            "bitcoin-cli",
            "getrawtransaction",
            &txid_str,
            "false",
            &block_hash_str,
        ])?;
        deserialize_hex(&raw_hex)
            .map_err(|error| format!("failed to decode raw transaction `{txid}`: {error}"))
    }

    pub fn send_raw_transaction(&self, tx: &Transaction) -> Result<Txid, String> {
        let raw_hex = serialize_hex(tx);
        let raw_txid = self
            .just
            .run(&["bitcoin-cli", "sendrawtransaction", &raw_hex])?;
        Self::parse_txid(&raw_txid)
    }

    pub fn parse_regtest_address(raw: &str) -> Result<Address, String> {
        Address::from_str(raw.trim())
            .map_err(|error| format!("invalid regtest address `{raw}`: {error}"))?
            .require_network(Network::Regtest)
            .map_err(|error| format!("address `{raw}` is not on regtest: {error}"))
    }

    pub fn parse_btc_amount(raw: &str) -> Result<Amount, String> {
        Amount::from_str_in(raw.trim(), Denomination::Bitcoin)
            .map_err(|error| format!("invalid BTC amount `{raw}`: {error}"))
    }

    fn format_btc_amount(amount: Amount) -> String {
        amount.to_string_in(Denomination::Bitcoin)
    }

    fn parse_txid(raw: &str) -> Result<Txid, String> {
        Txid::from_str(raw.trim()).map_err(|error| format!("invalid txid `{raw}`: {error}"))
    }

    fn parse_block_hash(raw: &str) -> Result<BlockHash, String> {
        BlockHash::from_str(raw.trim())
            .map_err(|error| format!("invalid block hash `{raw}`: {error}"))
    }

    fn parse_block_height(raw: &str) -> Result<u64, String> {
        raw.trim()
            .parse::<u64>()
            .map_err(|error| format!("invalid block height `{raw}`: {error}"))
    }
}

#[cfg(test)]
mod test_regtest_bitcoin {
    use super::*;
    use bitcoin::hashes::Hash;

    #[test]
    fn format_btc_amount_produces_plain_decimal() {
        assert_eq!(
            RegtestBitcoinClient::format_btc_amount(Amount::from_sat(100_000)),
            "0.001"
        );
        assert_eq!(
            RegtestBitcoinClient::format_btc_amount(Amount::from_btc(1.5).unwrap()),
            "1.5"
        );
    }

    #[test]
    fn parses_just_command_outputs_and_rejects_invalid_values() {
        assert!(
            RegtestBitcoinClient::parse_regtest_address(
                "bcrt1q6rz28mcfaxtmd6v789l9rrlrusdprr9pz3cppk"
            )
            .is_ok()
        );
        assert!(RegtestBitcoinClient::parse_regtest_address("not-an-address").is_err());

        assert_eq!(
            RegtestBitcoinClient::parse_btc_amount("50.00000000").unwrap(),
            Amount::from_btc(50.0).unwrap()
        );
        assert!(RegtestBitcoinClient::parse_btc_amount("abc").is_err());

        let zero_txid = Txid::all_zeros();
        assert_eq!(
            RegtestBitcoinClient::parse_txid(&zero_txid.to_string()).unwrap(),
            zero_txid
        );
        assert!(RegtestBitcoinClient::parse_txid("short").is_err());

        let zero_hash = BlockHash::all_zeros();
        assert_eq!(
            RegtestBitcoinClient::parse_block_hash(&zero_hash.to_string()).unwrap(),
            zero_hash
        );
        assert!(RegtestBitcoinClient::parse_block_hash("short").is_err());

        assert_eq!(
            RegtestBitcoinClient::parse_block_height("101\n").unwrap(),
            101
        );
        assert!(RegtestBitcoinClient::parse_block_height("-1").is_err());
    }
}
