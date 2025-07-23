use bitcoin::{
    opcodes::all::{OP_ENDIF, OP_IF, OP_NOP},
    script::{Instruction, Instructions},
    OutPoint, TxOut,
};
use std::cmp::Ordering;

pub type Bytes = Vec<u8>;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InscriptionSection {
    pub protocol: Bytes,
    pub arguments: Vec<Bytes>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct InscriptionMetadata {
    /// The block height containing the transaction.
    pub blockheight: u64,

    /// The index of the transaction in the block.
    pub blockindex: usize,

    /// output index of the transaction.
    pub vout: u32,

    /// The transaction ID.
    pub txid: bitcoin::Txid,
}

#[cfg(test)]
mod test_inscription_metadata {
    use super::*;
    use bitcoin::{hashes::Hash, Txid};

    #[test]
    fn test_inscription_ordering() {
        let older = InscriptionMetadata {
            blockheight: 0,
            blockindex: 0,
            vout: 0,
            txid: Txid::all_zeros(),
        };
        let sorted_vec = vec![
            older.clone(),
            InscriptionMetadata {
                vout: 1,
                ..older.clone()
            },
            InscriptionMetadata {
                blockindex: 1,
                ..older.clone()
            },
            InscriptionMetadata {
                blockheight: 1,
                ..older.clone()
            },
        ];
        let mut vec = sorted_vec.clone();
        vec.sort();
        assert_eq!(vec, sorted_vec);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Inscription {
    pub label: Bytes,
    pub sections: Vec<InscriptionSection>,
}

impl Inscription {
    pub fn from_txout(txout: &TxOut) -> Option<Inscription> {
        let script_buffer = &txout.script_pubkey;
        let (label, sections) = parse_inscription(&mut script_buffer.instructions())?;
        Some(Inscription { label, sections })
    }
}

fn parse_inscription(instructions: &mut Instructions) -> Option<(Bytes, Vec<InscriptionSection>)> {
    let (label, has_more) = parse_header(instructions)?;
    let mut sections: Vec<InscriptionSection> = Vec::new();
    if !has_more {
        return Some((label, sections));
    }
    loop {
        let (section, has_more) = parse_section(instructions)?;
        sections.push(section);
        if !has_more {
            break;
        }
    }
    Some((label, sections))
}

/// Parse the inscription header.
/// If the header is not valid, return `None`.
/// If the header is valid, return the label and a boolean indicating if there is more to parse.
fn parse_header(mut instructions: &mut Instructions) -> Option<(Bytes, bool)> {
    match instructions.next()? {
        Ok(Instruction::PushBytes(push_bytes)) if push_bytes.is_empty() => {}
        _ => return None,
    }
    match instructions.next()? {
        Ok(Instruction::Op(OP_IF)) => {}
        _ => return None,
    }
    let (header_section, has_more) = parse_section(&mut instructions)?;
    let magic_bytes = &header_section.protocol;
    if *magic_bytes != b"name" {
        return None;
    }
    let label = header_section.arguments.get(0)?;
    return Some((label.clone(), has_more));
}

/// Parse a section from the instructions.
/// If the section is not valid, return `None`.
/// If the section is valid, return the section and a boolean indicating if there is more to parse.
fn parse_section(instructions: &mut Instructions) -> Option<(InscriptionSection, bool)> {
    let protocol: Bytes = match instructions.next()? {
        Ok(Instruction::PushBytes(push_bytes)) => push_bytes.as_bytes().into(),
        _ => return None,
    };
    let mut arguments: Vec<Bytes> = Vec::new();
    for instruction in instructions {
        match instruction {
            Ok(Instruction::PushBytes(push_bytes)) => {
                arguments.push(push_bytes.as_bytes().into());
            }
            Ok(Instruction::Op(op)) if op == OP_NOP || op == OP_ENDIF => {
                let has_more = op == OP_NOP;
                return Some((
                    InscriptionSection {
                        protocol: protocol,
                        arguments,
                    },
                    has_more,
                ));
            }
            _ => {
                return None;
            }
        }
    }
    None
}

#[cfg(test)]
mod test_inscription {
    use super::*;
    use bitcoin::{
        hashes::{hash160, Hash},
        opcodes::{
            all::{OP_ENDIF, OP_IF},
            OP_FALSE,
        },
        script::{Builder, ScriptBuf},
        Amount, PubkeyHash,
    };

    #[test]
    fn test_from_txout() {
        let p2pkh_script =
            ScriptBuf::new_p2pkh(&PubkeyHash::from_raw_hash(hash160::Hash::all_zeros()));
        let nostr_pubkey = nostr_sdk::PublicKey::from_byte_array([
            0xff, 0xf7, 0x9a, 0xc1, 0xea, 0x96, 0x51, 0x30, 0x4e, 0xd4, 0xb7, 0x1a, 0x05, 0x43,
            0x72, 0xad, 0x5a, 0x26, 0x2b, 0xe6, 0x12, 0xdd, 0xa0, 0x58, 0x8a, 0xeb, 0x84, 0x61,
            0x77, 0xbd, 0x29, 0xe2,
        ]);
        let mut script_pubkey = Builder::default()
            .push_opcode(OP_FALSE)
            .push_opcode(OP_IF)
            .push_slice(b"name")
            .push_slice(b"label")
            .push_opcode(OP_NOP)
            .push_slice(b"protocol-0")
            .push_slice(b"arg1")
            .push_slice(b"arg2")
            .push_opcode(OP_NOP)
            .push_slice(b"dns-nostr")
            .push_slice(nostr_pubkey.as_bytes())
            .push_opcode(OP_ENDIF)
            .into_script();
        script_pubkey.extend(p2pkh_script.instructions().flatten());
        println!("Script: {}", script_pubkey.to_asm_string());
        let txout = bitcoin::TxOut {
            value: Amount::from_sat(0), // Placeholder value,
            script_pubkey,
        };
        let inscription = Inscription::from_txout(&txout).unwrap();
        assert_eq!(inscription.label, b"label");
        assert_eq!(inscription.sections.len(), 2);
        assert_eq!(inscription.sections[0].protocol, b"protocol-0");
        assert_eq!(inscription.sections[0].arguments.len(), 2);
        assert_eq!(inscription.sections[0].arguments[0], b"arg1");
        assert_eq!(inscription.sections[0].arguments[1], b"arg2");
        assert_eq!(inscription.sections[1].protocol, b"dns-nostr");
        assert_eq!(inscription.sections[1].arguments.len(), 1);
        assert_eq!(
            inscription.sections[1].arguments[0],
            nostr_pubkey.as_bytes()
        );
    }
}

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
