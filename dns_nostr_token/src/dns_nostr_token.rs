use crate::label::{InvalidLabel, Label};
use name_token::NameToken;
use nostr_sdk::PublicKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsNostrToken {
    pub label: Label,
    pub nostr_pubkey: PublicKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsNostrTokenFromNameTokenError {
    InvalidLabel(InvalidLabel),
    MissingProtocolArgs,
    InvalidPublicKey,
    InvalidUtf8Label,
}

impl TryFrom<NameToken> for DnsNostrToken {
    type Error = DnsNostrTokenFromNameTokenError;

    fn try_from(value: NameToken) -> Result<Self, Self::Error> {
        let label_str = std::str::from_utf8(value.label().as_ref())
            .map_err(|_| DnsNostrTokenFromNameTokenError::InvalidUtf8Label)?;
            
        let label = Label::new(label_str)
            .map_err(DnsNostrTokenFromNameTokenError::InvalidLabel)?;

        let protocol_args = match value.protocol_args(&b"dns-nostr".into()) {
            None => Err(DnsNostrTokenFromNameTokenError::MissingProtocolArgs),
            Some(args) => Ok(args),
        }?;
        let nostr_pubkey = match protocol_args.first() {
            None => Err(DnsNostrTokenFromNameTokenError::InvalidPublicKey),
            Some(arg) => match PublicKey::from_slice(arg) {
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
    use bitcoin::{hashes::Hash, Txid, BlockHash};
    use bdk_chain::BlockId;

    use super::*;
    use name_token::{Inscription, InscriptionSection, NameToken, Position};

    #[test]
    fn test_try_from_name_token() {
        let nostr_pubkey = PublicKey::from_slice(&[0; 32]).unwrap();
        let position = Position::new(
            BlockId { height: 0, hash: BlockHash::all_zeros() },
            0,
            bitcoin::OutPoint { txid: Txid::all_zeros(), vout: 0 },
        );
        let name_token = NameToken::new(
            name_token::Label::from(&b"domain"[..]),
            vec![name_token::NameTokenEvent::Minted {
                position: position.clone(),
                inscription: Inscription {
                    label: name_token::Label::from(&b"domain"[..]),
                    sections: vec![InscriptionSection {
                        protocol: b"dns-nostr".into(),
                        arguments: vec![nostr_pubkey.to_bytes().into()],
                    }],
                },
            }],
        );
        let dns_nostr_token = DnsNostrToken::try_from(name_token).unwrap();
        assert_eq!(dns_nostr_token.label.as_str(), "domain");
    }
    
    #[test]
    fn test_try_from_invalid_label() {
        let nostr_pubkey = PublicKey::from_slice(&[0; 32]).unwrap();
        let position = Position::new(
            BlockId { height: 0, hash: BlockHash::all_zeros() },
            0,
            bitcoin::OutPoint { txid: Txid::all_zeros(), vout: 0 },
        );
        let name_token = NameToken::new(
            name_token::Label::from(&b"1domain"[..]),
            vec![name_token::NameTokenEvent::Minted {
                position: position.clone(),
                inscription: Inscription {
                    label: name_token::Label::from(&b"1domain"[..]),
                    sections: vec![InscriptionSection {
                        protocol: b"dns-nostr".into(),
                        arguments: vec![nostr_pubkey.to_bytes().into()],
                    }],
                },
            }],
        );
        let err = DnsNostrToken::try_from(name_token).unwrap_err();
        assert_eq!(err, DnsNostrTokenFromNameTokenError::InvalidLabel(InvalidLabel::InvalidStartCharacter));
    }
}
