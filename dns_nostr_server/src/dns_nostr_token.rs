use crate::name_token::NameToken;
use hickory_server::proto::rr::domain::Label;
use nostr_sdk::PublicKey;

#[derive(Debug, Clone)]
pub struct DnsNostrToken {
    pub label: Label,
    pub nostr_pubkey: PublicKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsNostrTokenFromNameTokenError {
    InvalidLabel,
    MissingProtocolArgs,
    InvalidPublicKey,
}

impl TryFrom<NameToken> for DnsNostrToken {
    type Error = DnsNostrTokenFromNameTokenError;

    fn try_from(value: NameToken) -> Result<Self, Self::Error> {
        let label = match Label::from_raw_bytes(&value.label) {
            Err(_) => Err(DnsNostrTokenFromNameTokenError::InvalidLabel),
            Ok(label) => {
                let lower_name_label = label.to_lowercase();
                if lower_name_label != label {
                    Err(DnsNostrTokenFromNameTokenError::InvalidLabel)
                } else {
                    Ok(label)
                }
            }
        }?;
        let protocol_args = match value.protocol_args(&b"dns-nostr".into()) {
            None => Err(DnsNostrTokenFromNameTokenError::MissingProtocolArgs),
            Some(args) => Ok(args),
        }?;
        let nostr_pubkey = match protocol_args.get(0) {
            None => Err(DnsNostrTokenFromNameTokenError::InvalidPublicKey),
            Some(arg) => match PublicKey::from_slice(&arg) {
                Err(_) => Err(DnsNostrTokenFromNameTokenError::InvalidPublicKey),
                Ok(pubkey) => Ok(pubkey),
            },
        }?;
        Ok(DnsNostrToken {
            label,
            nostr_pubkey,
        })
    }
}

#[cfg(test)]
mod tests {
    use bitcoin::{hashes::Hash, Txid};

    use super::*;
    use crate::name_token::{Inscription, InscriptionMetadata, InscriptionSection, NameToken};

    #[test]
    fn test_try_from_name_token() {
        let nostr_pubkey = PublicKey::from_slice(&[0; 32]).unwrap();
        let name_token = NameToken::create(
            Inscription {
                label: b"domain".into(),
                sections: vec![InscriptionSection {
                    protocol: b"dns-nostr".into(),
                    arguments: vec![
                        nostr_pubkey.to_bytes().into(),
                    ],
                }],
            },
            InscriptionMetadata {
                blockheight: 0,
                blockindex: 0,
                vout: 0,
                txid: Txid::all_zeros(),
            },
        );
        let dns_nostr_token = DnsNostrToken::try_from(name_token).unwrap();
        assert_eq!(dns_nostr_token.label.to_string(), "domain");
    }
}
