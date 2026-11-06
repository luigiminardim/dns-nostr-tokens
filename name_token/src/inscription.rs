use bitcoin::{
    opcodes::all::{OP_ENDIF, OP_IF, OP_NOP},
    script::{Instruction, Instructions},
    TxOut,
};

pub type Bytes = Vec<u8>;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InscriptionSection {
    pub protocol: Bytes,
    pub arguments: Vec<Bytes>,
}

use crate::Label;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Inscription {
    pub label: Label,
    pub sections: Vec<InscriptionSection>,
}

impl Inscription {
    pub fn from_txout(txout: &TxOut) -> Option<Inscription> {
        let script_buffer = &txout.script_pubkey;
        let (label, sections) = parse_inscription(&mut script_buffer.instructions())?;
        Some(Inscription { label, sections })
    }
}

fn parse_inscription(instructions: &mut Instructions) -> Option<(Label, Vec<InscriptionSection>)> {
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
fn parse_header(instructions: &mut Instructions) -> Option<(Label, bool)> {
    match instructions.next()? {
        Ok(Instruction::PushBytes(push_bytes)) if push_bytes.is_empty() => {}
        _ => return None,
    }
    match instructions.next()? {
        Ok(Instruction::Op(OP_IF)) => {}
        _ => return None,
    }
    let (header_section, has_more) = parse_section(instructions)?;
    let magic_bytes = &header_section.protocol;
    if *magic_bytes != b"name" {
        return None;
    }
    let label = header_section.arguments.first()?;
    Some((label.clone().into(), has_more))
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
                        protocol,
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
        let pubkey_bytes: [u8; 32] = [
            0xff, 0xf7, 0x9a, 0xc1, 0xea, 0x96, 0x51, 0x30, 0x4e, 0xd4, 0xb7, 0x1a, 0x05, 0x43,
            0x72, 0xad, 0x5a, 0x26, 0x2b, 0xe6, 0x12, 0xdd, 0xa0, 0x58, 0x8a, 0xeb, 0x84, 0x61,
            0x77, 0xbd, 0x29, 0xe2,
        ];
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
            .push_slice(pubkey_bytes)
            .push_opcode(OP_ENDIF)
            .into_script();
        script_pubkey.extend(p2pkh_script.instructions().flatten());
        println!("Script: {}", script_pubkey.to_asm_string());
        let txout = bitcoin::TxOut {
            value: Amount::from_sat(0), // Placeholder value,
            script_pubkey,
        };
        let inscription = Inscription::from_txout(&txout).unwrap();
        assert_eq!(inscription.label.as_ref(), b"label");
        assert_eq!(inscription.sections.len(), 2);
        assert_eq!(inscription.sections[0].protocol, b"protocol-0");
        assert_eq!(inscription.sections[0].arguments.len(), 2);
        assert_eq!(inscription.sections[0].arguments[0], b"arg1");
        assert_eq!(inscription.sections[0].arguments[1], b"arg2");
        assert_eq!(inscription.sections[1].protocol, b"dns-nostr");
        assert_eq!(inscription.sections[1].arguments.len(), 1);
        assert_eq!(inscription.sections[1].arguments[0], pubkey_bytes);
    }
}
