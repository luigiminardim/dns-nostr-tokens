use crate::inscription::{Bytes, Inscription};
use crate::label::Label;
use crate::position::Position;
use bdk_chain::BlockId;
use bitcoin::{OutPoint, TxOut};
use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NameTokenEvent {
    Minted {
        position: Position,
        inscription: Inscription,
    },
    Updated {
        position: Position,
        inscription: Inscription,
    },
    Revoked {
        position: Position,
    },
}

impl NameTokenEvent {
    pub fn position(&self) -> &Position {
        match self {
            Self::Minted { position, .. } => position,
            Self::Updated { position, .. } => position,
            Self::Revoked { position } => position,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameToken {
    pub label: Label,
    pub events: Vec<NameTokenEvent>,
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
    pub fn new(label: Label, events: Vec<NameTokenEvent>) -> NameToken {
        NameToken { label, events }
    }

    fn mint(position: Position, inscription: Inscription) -> NameToken {
        NameToken {
            label: inscription.label.clone(),
            events: vec![NameTokenEvent::Minted {
                position,
                inscription,
            }],
        }
    }

    pub fn first_position(&self) -> &Position {
        self.events.first().expect("NameToken has no events").position()
    }

    pub fn last_position(&self) -> &Position {
        self.events.last().expect("NameToken has no events").position()
    }

    pub fn inscription(&self) -> Option<&Inscription> {
        match self.events.last() {
            Some(NameTokenEvent::Minted { inscription, .. }) => Some(inscription),
            Some(NameTokenEvent::Updated { inscription, .. }) => Some(inscription),
            _ => None,
        }
    }

    pub fn is_revoked(&self) -> bool {
        matches!(self.events.last(), Some(NameTokenEvent::Revoked { .. }))
    }

    pub fn has_minted(&self) -> bool {
        !self.events.is_empty()
    }

    pub fn last_outpoint(&self) -> OutPoint {
        let pos = self.last_position();
        OutPoint {
            txid: pos.txid(),
            vout: pos.vout(),
        }
    }

    pub fn protocol_args(&self, protocol: &Bytes) -> Option<Vec<Bytes>> {
        self.inscription().and_then(|inscription| {
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
        position: Position,
    ) -> Result<NameToken, UpdateNameTokenError> {
        if inscription.label != self.label {
            return Err(UpdateNameTokenError::LabelMismatch);
        }
        if self.is_revoked() {
            return Err(UpdateNameTokenError::Revoked);
        }
        if self.last_position().cmp(&position) != Ordering::Less {
            return Err(UpdateNameTokenError::StaleInscription);
        }
        let mut updated = self.clone();
        updated.events.push(NameTokenEvent::Updated {
            position,
            inscription,
        });
        Ok(updated)
    }

    pub fn revoke(&self, position: Position) -> NameToken {
        let mut revoked = self.clone();
        revoked.events.push(NameTokenEvent::Revoked { position });
        revoked
    }

    pub fn process_same_index_chain(
        input_name_token: Option<&NameToken>,
        output: Option<&TxOut>,
        position: &Position,
    ) -> Vec<NameToken> {
        let output_inscription = output.and_then(Inscription::from_txout);
        match (input_name_token, output_inscription) {
            (None, None) => vec![],
            (Some(input_name_token), None) => {
                let revoked_name_token = input_name_token.revoke(position.clone());
                vec![revoked_name_token]
            }
            (None, Some(output_inscription)) => {
                let minted_name_token = NameToken::mint(position.clone(), output_inscription);
                vec![minted_name_token]
            }
            (Some(input_name_token), Some(output_inscription)) => {
                match input_name_token.update(output_inscription.clone(), position.clone()) {
                    Ok(updated_name_token) => vec![updated_name_token],
                    Err(UpdateNameTokenError::LabelMismatch) => {
                        let revoked_name_token = input_name_token.revoke(position.clone());
                        let minted_name_token =
                            NameToken::mint(position.clone(), output_inscription);
                        vec![revoked_name_token, minted_name_token]
                    }
                    Err(UpdateNameTokenError::Revoked) => {
                        let minted_name_token =
                            NameToken::mint(position.clone(), output_inscription);
                        vec![minted_name_token]
                    }
                    Err(UpdateNameTokenError::StaleInscription) => {
                        vec![]
                    }
                }
            }
        }
    }

    pub fn select_root_token<'a>(
        label: &Label,
        name_tokens: impl IntoIterator<Item = &'a NameToken>,
    ) -> Option<&'a NameToken> {
        name_tokens
            .into_iter()
            .filter(|nt| &nt.label == label)
            .filter(|nt| !nt.is_revoked())
            .min_by(|a, b| Position::cmp(a.first_position(), b.first_position()))
    }

    pub fn undo_block_events(&mut self, block_id: BlockId) {
        self.events.retain(|event| event.position().block_id() != block_id);
    }
}

#[cfg(test)]
mod test_name_token {
    use super::*;
    use crate::inscription::InscriptionSection;
    use bitcoin::{hashes::Hash, BlockHash, Txid};

    fn position(height: u32, index: usize, vout: u32) -> Position {
        Position::new(
            BlockId { height, hash: BlockHash::all_zeros() },
            index,
            bitcoin::OutPoint { txid: Txid::all_zeros(), vout },
        )
    }

    fn inscription(label: &[u8], protocols: &[&[u8]]) -> Inscription {
        let sections = protocols
            .iter()
            .map(|&p| InscriptionSection {
                protocol: p.to_vec(),
                arguments: vec![b"arg1".into(), b"arg2".into()],
            })
            .collect();
        Inscription {
            label: Label::from(label),
            sections,
        }
    }

    fn inscribed_output(label: &[u8]) -> bitcoin::TxOut {
        use bitcoin::opcodes::{all::*, OP_FALSE};
        use bitcoin::script::{Builder, PushBytesBuf};
        let label_bytes = PushBytesBuf::try_from(label.to_vec()).unwrap();
        bitcoin::TxOut {
            value: bitcoin::Amount::from_sat(546),
            script_pubkey: Builder::default()
                .push_opcode(OP_FALSE)
                .push_opcode(OP_IF)
                .push_slice(b"name")
                .push_slice(&label_bytes)
                .push_opcode(OP_ENDIF)
                .into_script(),
        }
    }
    
    fn plain_output() -> bitcoin::TxOut {
        bitcoin::TxOut {
            value: bitcoin::Amount::from_sat(546),
            script_pubkey: bitcoin::ScriptBuf::new(),
        }
    }

    #[test]
    fn should_initialize_name_token_with_valid_data() {
        let token = NameToken::mint(
            position(1, 0, 0),
            inscription(b"label", &[b"section-0"]),
        );

        assert_eq!(token.label.as_ref(), b"label");
        assert_eq!(token.first_position(), &position(1, 0, 0));
        assert_eq!(token.last_position(), &position(1, 0, 0));
        assert!(!token.is_revoked());
        assert!(token.protocol_args(&b"section-0".into()).is_some());
        assert!(token.protocol_args(&b"nonexistent".into()).is_none());
    }

    #[test]
    fn should_update_name_token_and_retain_first_position() {
        let token = NameToken::mint(
            position(1, 0, 0),
            inscription(b"label", &[b"section-0"]),
        );

        let updated = token
            .update(inscription(b"label", &[b"section-1"]), position(2, 0, 0))
            .unwrap();

        assert_eq!(updated.label.as_ref(), b"label");
        assert_eq!(updated.first_position(), &position(1, 0, 0));
        assert_eq!(updated.last_position(), &position(2, 0, 0));
        assert!(!updated.is_revoked());
        assert!(updated.protocol_args(&b"section-0".into()).is_none());
        assert!(updated.protocol_args(&b"section-1".into()).is_some());
    }

    #[test]
    fn should_revoke_name_token_and_clear_inscription() {
        let token = NameToken::mint(
            position(1, 0, 0),
            inscription(b"label", &[b"section-0"]),
        );

        let revoked = token.revoke(position(2, 0, 0));

        assert_eq!(revoked.label.as_ref(), b"label");
        assert_eq!(revoked.first_position(), &position(1, 0, 0));
        assert_eq!(revoked.last_position(), &position(2, 0, 0));
        assert!(revoked.is_revoked());
        assert!(revoked.protocol_args(&b"section-0".into()).is_none());
    }

    #[test]
    fn should_return_true_for_has_minted_if_events_are_present() {
        let mut token = NameToken::new(Label::from(b"label".as_ref()), vec![]);
        assert!(!token.has_minted());

        token = NameToken::mint(position(1, 0, 0), inscription(b"label", &[]));
        assert!(token.has_minted());
    }

    #[test]
    fn should_fail_to_update_when_labels_mismatch() {
        let token = NameToken::mint(
            position(1, 0, 0),
            inscription(b"label", &[]),
        );

        let result = token.update(inscription(b"other", &[]), position(2, 0, 0));

        assert_eq!(result, Err(UpdateNameTokenError::LabelMismatch));
    }

    #[test]
    fn should_fail_to_update_when_already_revoked() {
        let token = NameToken::mint(
            position(1, 0, 0),
            inscription(b"label", &[]),
        )
        .revoke(position(2, 0, 0));

        let result = token.update(inscription(b"label", &[]), position(3, 0, 0));

        assert_eq!(result, Err(UpdateNameTokenError::Revoked));
    }

    #[test]
    fn should_fail_to_update_when_new_position_is_older() {
        let token = NameToken::mint(
            position(2, 0, 0),
            inscription(b"label", &[]),
        );

        let result = token.update(
            inscription(b"label", &[]),
            position(1, 0, 0), // Older position
        );

        assert_eq!(result, Err(UpdateNameTokenError::StaleInscription));
    }

    #[test]
    fn should_select_earliest_inscription_as_root() {
        let token1 = NameToken::mint(
            position(2, 0, 0),
            inscription(b"label", &[]),
        );
        let token2 = NameToken::mint(
            position(1, 1, 0),
            inscription(b"label", &[]),
        );
        let token3 = NameToken::mint(
            position(1, 0, 0),
            inscription(b"label", &[]),
        ); // Earliest

        let tokens = vec![&token1, &token2, &token3];
        let root = NameToken::select_root_token(&Label::from(&b"label"[..]), tokens);

        assert_eq!(root, Some(&token3));
    }

    #[test]
    fn should_ignore_revoked_tokens_when_selecting_root() {
        let revoked_earliest = NameToken::mint(
            position(1, 0, 0),
            inscription(b"label", &[]),
        )
        .revoke(position(1, 1, 0));
        let valid_later = NameToken::mint(
            position(2, 0, 0),
            inscription(b"label", &[]),
        );

        let tokens = vec![&revoked_earliest, &valid_later];
        let root = NameToken::select_root_token(&Label::from(&b"label"[..]), tokens);

        assert_eq!(root, Some(&valid_later));
    }

    #[test]
    fn should_ignore_tokens_with_different_labels_when_selecting_root() {
        let other_label = NameToken::mint(
            position(1, 0, 0),
            inscription(b"other", &[]),
        );
        let correct_label = NameToken::mint(
            position(2, 0, 0),
            inscription(b"label", &[]),
        );

        let tokens = vec![&other_label, &correct_label];
        let root = NameToken::select_root_token(&Label::from(&b"label"[..]), tokens);

        assert_eq!(root, Some(&correct_label));
    }

    #[test]
    fn should_return_empty_when_processing_chain_with_no_input_and_no_output() {
        let result = NameToken::process_same_index_chain(None, None, &position(1, 0, 0));
        assert!(result.is_empty());
    }

    #[test]
    fn should_revoke_token_when_processing_chain_with_input_but_no_output_inscription() {
        let input = NameToken::mint(
            position(1, 0, 0),
            inscription(b"label", &[]),
        );

        let result = NameToken::process_same_index_chain(
            Some(&input),
            Some(&plain_output()),
            &position(2, 0, 0),
        );

        assert_eq!(result.len(), 1);
        assert!(result[0].is_revoked());
        assert_eq!(result[0].last_position(), &position(2, 0, 0));
    }

    #[test]
    fn should_mint_token_when_processing_chain_with_no_input_but_output_is_inscribed() {
        let result = NameToken::process_same_index_chain(
            None,
            Some(&inscribed_output(b"label")),
            &position(2, 0, 0),
        );

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].label.as_ref(), b"label");
        assert_eq!(result[0].first_position(), &position(2, 0, 0));
        assert!(!result[0].is_revoked());
    }

    #[test]
    fn should_update_token_when_processing_chain_with_valid_reinscription() {
        let input = NameToken::mint(
            position(1, 0, 0),
            inscription(b"label", &[]),
        );

        let result = NameToken::process_same_index_chain(
            Some(&input),
            Some(&inscribed_output(b"label")),
            &position(2, 0, 0),
        );

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].label.as_ref(), b"label");
        assert_eq!(result[0].first_position(), &position(1, 0, 0));
        assert_eq!(result[0].last_position(), &position(2, 0, 0));
        assert!(!result[0].is_revoked());
    }

    #[test]
    fn should_revoke_old_and_mint_new_when_processing_chain_with_label_mismatch() {
        let input = NameToken::mint(
            position(1, 0, 0),
            inscription(b"label", &[]),
        );

        let result = NameToken::process_same_index_chain(
            Some(&input),
            Some(&inscribed_output(b"other")),
            &position(2, 0, 0),
        );

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].label.as_ref(), b"label");
        assert!(result[0].is_revoked());
        
        assert_eq!(result[1].label.as_ref(), b"other");
        assert!(!result[1].is_revoked());
        assert_eq!(result[1].first_position(), &position(2, 0, 0));
    }

    #[test]
    fn should_mint_new_token_when_processing_chain_with_already_revoked_input() {
        let input = NameToken::mint(
            position(1, 0, 0),
            inscription(b"label", &[]),
        )
        .revoke(position(2, 0, 0));

        let result = NameToken::process_same_index_chain(
            Some(&input),
            Some(&inscribed_output(b"label")),
            &position(3, 0, 0),
        );

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].label.as_ref(), b"label");
        assert_eq!(result[0].first_position(), &position(3, 0, 0));
        assert!(!result[0].is_revoked());
    }

    #[test]
    fn should_undo_block_events() {
        let mut token = NameToken::mint(
            position(1, 0, 0),
            inscription(b"label", &[]),
        )
        .revoke(position(2, 0, 0));

        token.undo_block_events(position(2, 0, 0).block_id());

        assert!(!token.is_revoked());
        assert_eq!(token.last_position(), &position(1, 0, 0));
    }
}
