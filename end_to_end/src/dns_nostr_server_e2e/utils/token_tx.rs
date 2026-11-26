use crate::utils::regtest_bitcoin::RegtestBitcoinClient;
use bitcoin::{
    absolute::LockTime,
    hashes::Hash,
    opcodes::{
        all::{OP_CHECKSIG, OP_DUP, OP_ENDIF, OP_EQUALVERIFY, OP_HASH160, OP_IF, OP_NOP},
        OP_FALSE,
    },
    script::{Builder, PushBytesBuf},
    secp256k1::{Message, Secp256k1, SecretKey},
    sighash::{EcdsaSighashType, SighashCache},
    transaction, Address, Amount, CompressedPublicKey, Network, OutPoint, PubkeyHash, ScriptBuf,
    Sequence, Transaction, TxIn, TxOut, Txid, Witness,
};
use dns_nostr_token::label::Label;
use rand::RngCore;

#[derive(Debug, Clone)]
pub struct TokenUtxo {
    pub outpoint: OutPoint,
    pub amount: Amount,
    pub script_pubkey: ScriptBuf,
    pub secret_key: SecretKey,
    pub block_height: u64,
}

/// Builds a P2PKH-locked Name-Token `scriptPubKey` carrying a `dns-nostr` Nostr public key.
pub fn build_token_script_pubkey(
    label: &Label,
    nostr_pubkey: &nostr_sdk::PublicKey,
    owner_pubkey_hash: &PubkeyHash,
) -> ScriptBuf {
    let label_bytes = PushBytesBuf::try_from(label.as_bytes().to_vec())
        .expect("valid DNS label is at most 63 bytes");
    let pubkey_bytes = PushBytesBuf::try_from(nostr_pubkey.to_bytes().to_vec())
        .expect("Nostr pubkey is 32 bytes");
    let pubkey_hash_bytes = PushBytesBuf::try_from(owner_pubkey_hash.as_byte_array().to_vec())
        .expect("PubkeyHash is 20 bytes");

    Builder::new()
        .push_opcode(OP_FALSE)
        .push_opcode(OP_IF)
        .push_slice(b"name")
        .push_slice(&label_bytes)
        .push_opcode(OP_NOP)
        .push_slice(b"dns-nostr")
        .push_slice(&pubkey_bytes)
        .push_opcode(OP_ENDIF)
        .push_opcode(OP_DUP)
        .push_opcode(OP_HASH160)
        .push_slice(&pubkey_hash_bytes)
        .push_opcode(OP_EQUALVERIFY)
        .push_opcode(OP_CHECKSIG)
        .into_script()
}

pub struct TokenTxClient<'a> {
    bitcoin: &'a RegtestBitcoinClient,
    secp: Secp256k1<bitcoin::secp256k1::All>,
}

impl<'a> TokenTxClient<'a> {
    pub fn new(bitcoin: &'a RegtestBitcoinClient) -> Self {
        Self {
            bitcoin,
            secp: Secp256k1::new(),
        }
    }

    pub fn mint_token(
        &self,
        label: &Label,
        nostr_pubkey: &nostr_sdk::PublicKey,
    ) -> Result<TokenUtxo, String> {
        let funding_amount = Amount::from_sat(100_000);
        let token_amount = Amount::from_sat(99_000);

        let secret_key = self.generate_ephemeral_secret_key();
        let compressed_pubkey = CompressedPublicKey(secret_key.public_key(&self.secp));
        let owner_pubkey_hash = compressed_pubkey.pubkey_hash();
        let ephemeral_p2pkh_script = ScriptBuf::new_p2pkh(&owner_pubkey_hash);
        let ephemeral_address = Address::p2pkh(compressed_pubkey, Network::Regtest);

        let funding_txid = self
            .bitcoin
            .fund_address_unconfirmed(&ephemeral_address, funding_amount)?;
        let (funding_block_hash, _) = self.bitcoin.mine_one_block()?;

        let funding_tx = self
            .bitcoin
            .get_raw_transaction(&funding_txid, &funding_block_hash)?;
        let funding_vout = funding_tx
            .output
            .iter()
            .position(|output| output.script_pubkey == ephemeral_p2pkh_script)
            .ok_or_else(|| {
                format!("funding tx `{funding_txid}` missing output for ephemeral P2PKH address")
            })? as u32;

        let token_script_pubkey =
            build_token_script_pubkey(label, nostr_pubkey, &owner_pubkey_hash);
        let signed_tx = self.build_and_sign_single_io_tx(
            OutPoint {
                txid: funding_txid,
                vout: funding_vout,
            },
            &ephemeral_p2pkh_script,
            &secret_key,
            token_amount,
            token_script_pubkey.clone(),
        )?;

        let mint_txid = self.bitcoin.send_raw_transaction(&signed_tx)?;
        let (_, block_height) = self.bitcoin.mine_one_block()?;

        Ok(TokenUtxo {
            outpoint: OutPoint {
                txid: mint_txid,
                vout: 0,
            },
            amount: token_amount,
            script_pubkey: token_script_pubkey,
            secret_key,
            block_height,
        })
    }

    pub fn update_token(
        &self,
        previous: &TokenUtxo,
        label: &Label,
        new_nostr_pubkey: &nostr_sdk::PublicKey,
    ) -> Result<TokenUtxo, String> {
        let fee = Amount::from_sat(1_000);
        let updated_amount = previous
            .amount
            .checked_sub(fee)
            .ok_or_else(|| "token UTXO balance too low to cover update fee".to_string())?;
        let compressed_pubkey = CompressedPublicKey(previous.secret_key.public_key(&self.secp));
        let owner_pubkey_hash = compressed_pubkey.pubkey_hash();
        let new_script_pubkey =
            build_token_script_pubkey(label, new_nostr_pubkey, &owner_pubkey_hash);

        let signed_tx = self.build_and_sign_single_io_tx(
            previous.outpoint,
            &previous.script_pubkey,
            &previous.secret_key,
            updated_amount,
            new_script_pubkey.clone(),
        )?;

        let update_txid = self.bitcoin.send_raw_transaction(&signed_tx)?;
        let (_, block_height) = self.bitcoin.mine_one_block()?;

        Ok(TokenUtxo {
            outpoint: OutPoint {
                txid: update_txid,
                vout: 0,
            },
            amount: updated_amount,
            script_pubkey: new_script_pubkey,
            secret_key: previous.secret_key,
            block_height,
        })
    }

    pub fn revoke_token(&self, previous: &TokenUtxo) -> Result<(Txid, u64), String> {
        let fee = Amount::from_sat(1_000);
        let revoked_amount = previous
            .amount
            .checked_sub(fee)
            .ok_or_else(|| "token UTXO balance too low to cover revoke fee".to_string())?;
        let compressed_pubkey = CompressedPublicKey(previous.secret_key.public_key(&self.secp));
        let plain_p2pkh_script = ScriptBuf::new_p2pkh(&compressed_pubkey.pubkey_hash());

        let signed_tx = self.build_and_sign_single_io_tx(
            previous.outpoint,
            &previous.script_pubkey,
            &previous.secret_key,
            revoked_amount,
            plain_p2pkh_script,
        )?;

        let revoke_txid = self.bitcoin.send_raw_transaction(&signed_tx)?;
        let (_, block_height) = self.bitcoin.mine_one_block()?;

        Ok((revoke_txid, block_height))
    }

    fn build_and_sign_single_io_tx(
        &self,
        previous_outpoint: OutPoint,
        previous_script_pubkey: &ScriptBuf,
        secret_key: &SecretKey,
        output_amount: Amount,
        output_script_pubkey: ScriptBuf,
    ) -> Result<Transaction, String> {
        let mut tx = Transaction {
            version: transaction::Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: previous_outpoint,
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: output_amount,
                script_pubkey: output_script_pubkey,
            }],
        };

        let sighash = SighashCache::new(&tx)
            .legacy_signature_hash(0, previous_script_pubkey, EcdsaSighashType::All.to_u32())
            .map_err(|error| format!("failed to compute legacy signature hash: {error}"))?;
        let message = Message::from_digest(sighash.to_byte_array());
        let signature = self.secp.sign_ecdsa(&message, secret_key);

        let mut sig_bytes = signature.serialize_der().to_vec();
        sig_bytes.push(EcdsaSighashType::All as u8);
        let sig_push = PushBytesBuf::try_from(sig_bytes)
            .map_err(|error| format!("invalid DER signature push bytes: {error}"))?;

        let compressed_pubkey = CompressedPublicKey(secret_key.public_key(&self.secp));
        let pubkey_push = PushBytesBuf::try_from(compressed_pubkey.to_bytes().to_vec())
            .map_err(|error| format!("invalid compressed pubkey push bytes: {error}"))?;

        tx.input[0].script_sig = Builder::new()
            .push_slice(&sig_push)
            .push_slice(&pubkey_push)
            .into_script();

        Ok(tx)
    }

    fn generate_ephemeral_secret_key(&self) -> SecretKey {
        let mut rng = rand::thread_rng();
        loop {
            let mut bytes = [0u8; 32];
            rng.fill_bytes(&mut bytes);
            if let Ok(key) = SecretKey::from_slice(&bytes) {
                return key;
            }
        }
    }
}

#[cfg(test)]
mod test_token_tx {
    use super::*;
    use bdk_chain::BlockId;
    use bitcoin::BlockHash;
    use dns_nostr_token::dns_nostr_token::DnsNostrToken;
    use name_token::{Inscription, NameToken, NameTokenEvent, Position};

    #[test]
    fn build_token_script_pubkey_round_trips_through_domain_parsers() {
        let label = Label::new("e2e-roundtrip-1").unwrap();
        let nostr_keys = nostr_sdk::Keys::generate();
        let owner_pubkey_hash = PubkeyHash::from_byte_array([0x42; 20]);

        let script_pubkey =
            build_token_script_pubkey(&label, &nostr_keys.public_key(), &owner_pubkey_hash);
        let txout = TxOut {
            value: Amount::from_sat(99_000),
            script_pubkey,
        };

        let inscription = Inscription::from_txout(&txout).expect("inscription should parse");
        let position = Position::new(
            BlockId {
                height: 1,
                hash: BlockHash::all_zeros(),
            },
            0,
            OutPoint {
                txid: Txid::all_zeros(),
                vout: 0,
            },
        );
        let name_token = NameToken::new(
            inscription.label.clone(),
            vec![NameTokenEvent::Minted {
                position,
                inscription,
            }],
        );

        let dns_nostr_token =
            DnsNostrToken::try_from(name_token).expect("DnsNostrToken conversion should succeed");
        assert_eq!(dns_nostr_token.label, label);
        assert_eq!(dns_nostr_token.nostr_pubkey, nostr_keys.public_key());
    }
}
