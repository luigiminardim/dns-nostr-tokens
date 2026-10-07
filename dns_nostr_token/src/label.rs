#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Label(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidLabel {
    Empty,
    TooLong,
    InvalidStartCharacter,
    TrailingHyphen,
    InvalidCharacter,
}

impl Label {
    pub fn new(s: &str) -> Result<Self, InvalidLabel> {
        if s.is_empty() {
            return Err(InvalidLabel::Empty);
        }
        if s.len() > 63 {
            return Err(InvalidLabel::TooLong);
        }
        
        let mut chars = s.chars();
        let first = chars.next().unwrap();
        
        if !first.is_ascii_alphabetic() {
            return Err(InvalidLabel::InvalidStartCharacter);
        }
        if !first.is_ascii_lowercase() {
            return Err(InvalidLabel::InvalidCharacter);
        }
        if s.ends_with('-') {
            return Err(InvalidLabel::TrailingHyphen);
        }
        for c in chars {
            if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' {
                return Err(InvalidLabel::InvalidCharacter);
            }
        }
        Ok(Label(s.to_string()))
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
    
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl std::fmt::Display for Label {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_labels() {
        assert!(Label::new("blog").is_ok());
        assert!(Label::new("blog-123").is_ok());
        assert!(Label::new("a").is_ok());
        assert!(Label::new("a1").is_ok());
    }

    #[test]
    fn test_invalid_labels() {
        assert_eq!(Label::new(""), Err(InvalidLabel::Empty));
        assert_eq!(Label::new("1blog"), Err(InvalidLabel::InvalidStartCharacter));
        assert_eq!(Label::new("-blog"), Err(InvalidLabel::InvalidStartCharacter));
        assert_eq!(Label::new("blog-"), Err(InvalidLabel::TrailingHyphen));
        assert_eq!(Label::new("Blog"), Err(InvalidLabel::InvalidCharacter));
        assert_eq!(Label::new("blOg"), Err(InvalidLabel::InvalidCharacter));
        assert_eq!(Label::new("blog_123"), Err(InvalidLabel::InvalidCharacter));
        
        let long_label = "a".repeat(64);
        assert_eq!(Label::new(&long_label), Err(InvalidLabel::TooLong));
    }
}
