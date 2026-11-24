use dns_nostr_token::label::Label;
use rand::RngCore;
use std::time::{SystemTime, UNIX_EPOCH};

/// Generates a unique, RFC-1034-valid lowercase label prefixed with `e2e-`.
pub fn unique_label(prefix: &str) -> Label {
    let sanitized: String = prefix
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch == '_' { '-' } else { ch })
        .filter(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || *ch == '-')
        .collect();
    let trimmed = sanitized.trim_matches('-');
    let truncated = trimmed
        .get(..trimmed.len().min(20))
        .unwrap_or(trimmed)
        .trim_end_matches('-');

    let unix_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before UNIX_EPOCH")
        .as_secs();

    let mut random_bytes = [0u8; 4];
    rand::thread_rng().fill_bytes(&mut random_bytes);
    let rand_hex = format!(
        "{:02x}{:02x}{:02x}{:02x}",
        random_bytes[0], random_bytes[1], random_bytes[2], random_bytes[3]
    );

    let candidate = if truncated.is_empty() {
        format!("e2e-{unix_ts}-{rand_hex}")
    } else {
        format!("e2e-{truncated}-{unix_ts}-{rand_hex}")
    };

    Label::new(&candidate).expect("generated e2e label must satisfy RFC-1034 rules")
}

#[cfg(test)]
mod test_labels {
    use super::*;

    #[test]
    fn generates_valid_sanitized_labels_under_63_bytes() {
        let label = unique_label("First_Is_Root_With_A_Very_Long_Prefix_Name");
        assert!(label.as_str().starts_with("e2e-first-is-root-with-a-"));
        assert!(label.as_str().len() <= 63);
        assert!(Label::new(label.as_str()).is_ok());
    }

    #[test]
    fn handles_empty_or_symbol_only_prefix() {
        let label = unique_label("___");
        assert!(label.as_str().starts_with("e2e-"));
        assert!(Label::new(label.as_str()).is_ok());
    }

    #[test]
    fn consecutive_labels_are_unique() {
        let first = unique_label("mint");
        let second = unique_label("mint");
        assert_ne!(first, second);
    }
}
