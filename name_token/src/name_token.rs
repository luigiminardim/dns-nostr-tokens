use crate::inscription::{Bytes, Inscription, InscriptionMetadata};
use bitcoin::OutPoint;
use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameToken {
    pub label: Bytes,
    pub first_inscription_metadata: InscriptionMetadata,
    pub last_inscription_metadata: InscriptionMetadata,
    pub inscription: Option<Inscription>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateNameTokenError {
    /// The new inscription has a different label than the last inscription.
    LabelMismatch,

    /// The NameToken is already revoked.
    Revoked,

    /// The new inscription has an older metadata than the last inscription.
    StaleInscription,
}

impl NameToken {
    pub fn new(
        label: Bytes,
        first_inscription_metadata: InscriptionMetadata,
        last_inscription_metadata: InscriptionMetadata,
        inscription: Inscription,
    ) -> NameToken {
        NameToken {
            label,
            first_inscription_metadata,
            last_inscription_metadata,
            inscription: Some(inscription),
        }
    }

    pub fn create(inscription: Inscription, metadata: InscriptionMetadata) -> NameToken {
        NameToken {
            label: inscription.label.clone(),
            first_inscription_metadata: metadata.clone(),
            last_inscription_metadata: metadata,
            inscription: Some(inscription),
        }
    }

    pub fn is_revoked(&self) -> bool {
        self.inscription.is_none()
    }

    pub fn last_outpoint(&self) -> OutPoint {
        OutPoint {
            txid: self.last_inscription_metadata.txid,
            vout: self.last_inscription_metadata.vout,
        }
    }

    pub fn protocol_args(&self, protocol: &Bytes) -> Option<Vec<Bytes>> {
        self.inscription.as_ref().and_then(|inscription| {
            inscription.sections.iter().find_map(|section| {
                if &section.protocol == protocol {
                    Some(section.arguments.clone())
                } else {
                    None
                }
            })
        })
    }

    pub fn update(
        &self,
        inscription: Inscription,
        metadata: InscriptionMetadata,
    ) -> Result<NameToken, UpdateNameTokenError> {
        if inscription.label != self.label {
            return Err(UpdateNameTokenError::LabelMismatch);
        }
        if self.is_revoked() {
            return Err(UpdateNameTokenError::Revoked);
        }
        if self.last_inscription_metadata.cmp(&metadata) != Ordering::Less {
            return Err(UpdateNameTokenError::StaleInscription);
        }
        Ok(NameToken {
            label: self.label.clone(),
            first_inscription_metadata: self.first_inscription_metadata.clone(),
            last_inscription_metadata: metadata,
            inscription: Some(inscription),
        })
    }

    pub fn revoke(&self) -> NameToken {
        NameToken {
            label: self.label.clone(),
            first_inscription_metadata: self.first_inscription_metadata.clone(),
            last_inscription_metadata: self.last_inscription_metadata.clone(),
            inscription: None,
        }
    }

    pub fn generate_name_token_updates(
        input_name_token: Option<&NameToken>,
        output_inscription: Option<&Inscription>,
        metadata: InscriptionMetadata,
    ) -> Vec<NameToken> {
        match (input_name_token, output_inscription) {
            (None, None) => vec![],
            (Some(input_name_token), None) => {
                let revoked_name_token = input_name_token.clone();
                vec![revoked_name_token]
            }
            (None, Some(output_inscription)) => {
                let created_name_token = NameToken::create(output_inscription.clone(), metadata);
                vec![created_name_token]
            }
            (Some(input_name_token), Some(output_inscription)) => {
                match input_name_token.update(output_inscription.clone(), metadata.clone()) {
                    Ok(updated_name_token) => vec![updated_name_token],
                    Err(UpdateNameTokenError::LabelMismatch) => {
                        let revoked_name_token = input_name_token.revoke();
                        let created_name_token =
                            NameToken::create(output_inscription.clone(), metadata);
                        vec![revoked_name_token, created_name_token]
                    }
                    Err(UpdateNameTokenError::Revoked) => {
                        let created_name_token =
                            NameToken::create(output_inscription.clone(), metadata);
                        vec![created_name_token]
                    }
                    Err(UpdateNameTokenError::StaleInscription) => {
                        vec![]
                    }
                }
            }
        }
    }

    pub fn select_valid_name_token<'a>(
        label: &Bytes,
        name_tokens: impl IntoIterator<Item = &'a NameToken>,
    ) -> Option<&'a NameToken> {
        name_tokens
            .into_iter()
            .filter(|nt| &nt.label == label)
            .filter(|nt| !nt.is_revoked())
            .min_by(|a, b| {
                InscriptionMetadata::cmp(
                    &a.first_inscription_metadata,
                    &b.first_inscription_metadata,
                )
            })
    }
}

#[cfg(test)]
mod test_name_token {
    use super::*;
    use crate::inscription::InscriptionSection;
    use bitcoin::{hashes::Hash, Txid};

    #[test]
    fn test_lifetime() {
        let label = b"label".to_vec();

        // Token creation
        let metadata = InscriptionMetadata {
            blockheight: 1,
            blockindex: 0,
            vout: 0,
            txid: Txid::all_zeros(),
        };
        let section_0 = InscriptionSection {
            protocol: b"section-0".to_vec(),
            arguments: vec![b"arg1".to_vec(), b"arg2".to_vec()],
        };
        let name_token = NameToken::create(
            Inscription {
                label: label.clone(),
                sections: vec![section_0],
            },
            metadata.clone(),
        );

        // Check initial state
        assert_eq!(name_token.label, b"label");
        assert_eq!(name_token.first_inscription_metadata.blockheight, 1);
        assert_eq!(name_token.last_inscription_metadata.blockheight, 1);
        assert!(!name_token.is_revoked());
        assert!(name_token.protocol_args(&b"section-0".into()).is_some());
        assert!(name_token.protocol_args(&b"nonexistent".into()).is_none());

        // Update with a new inscription
        let metadata = InscriptionMetadata {
            blockheight: 2,
            ..metadata.clone()
        };
        let section_1 = InscriptionSection {
            protocol: b"section-1".into(),
            arguments: vec![b"arg3".into(), b"arg4".into()],
        };
        let updated_token = name_token
            .update(
                Inscription {
                    label: label.clone(),
                    sections: vec![section_1],
                },
                metadata.clone(),
            )
            .unwrap();

        // Check updated state
        assert_eq!(updated_token.label, b"label");
        assert_eq!(updated_token.first_inscription_metadata.blockheight, 1);
        assert_eq!(updated_token.last_inscription_metadata.blockheight, 2);
        assert!(!updated_token.is_revoked());
        assert!(updated_token.protocol_args(&b"section-0".into()).is_none());
        assert!(updated_token.protocol_args(&b"section-1".into()).is_some());

        // Revoke the token
        let revoked_token = updated_token.revoke();
        assert_eq!(revoked_token.label, b"label");
        assert_eq!(revoked_token.first_inscription_metadata.blockheight, 1);
        assert_eq!(revoked_token.last_inscription_metadata.blockheight, 2);
        assert!(revoked_token.is_revoked());
        assert!(revoked_token.protocol_args(&b"section-0".into()).is_none());
        assert!(revoked_token.protocol_args(&b"section-1".into()).is_none());
    }

    #[test]
    fn test_select_valid_name_token() {
        let label = Bytes::from(b"label");
        let inscription = Inscription {
            label: label.clone(),
            sections: vec![InscriptionSection {
                protocol: b"section-0".into(),
                arguments: vec![b"arg1".into(), b"arg2".into()],
            }],
        };
        let first_name_token = NameToken::create(
            inscription.clone(),
            InscriptionMetadata {
                blockheight: 1,
                blockindex: 0,
                vout: 0,
                txid: Txid::all_zeros(),
            },
        );
        let second_name_token = NameToken::create(
            inscription.clone(),
            InscriptionMetadata {
                blockheight: 1,
                blockindex: 1,
                vout: 0,
                txid: Txid::all_zeros(),
            },
        );
        let third_name_token = NameToken::create(
            inscription.clone(),
            InscriptionMetadata {
                blockheight: 1,
                blockindex: 1,
                vout: 1,
                txid: Txid::all_zeros(),
            },
        );
        let fourth_name_token = NameToken::create(
            inscription.clone(),
            InscriptionMetadata {
                blockheight: 2,
                blockindex: 0,
                vout: 0,
                txid: Txid::all_zeros(),
            },
        );
        assert_eq!(
            NameToken::select_valid_name_token(
                &label,
                vec![
                    &fourth_name_token,
                    &third_name_token,
                    &second_name_token,
                    &first_name_token
                ]
            ),
            Some(&first_name_token)
        );

        let first_name_token = first_name_token
            .update(
                inscription,
                InscriptionMetadata {
                    blockheight: 2,
                    blockindex: 0,
                    vout: 1,
                    txid: Txid::all_zeros(),
                },
            )
            .unwrap();
        assert_eq!(
            NameToken::select_valid_name_token(
                &label,
                vec![
                    &fourth_name_token,
                    &third_name_token,
                    &second_name_token,
                    &first_name_token
                ]
            ),
            Some(&first_name_token)
        );
    }
}
