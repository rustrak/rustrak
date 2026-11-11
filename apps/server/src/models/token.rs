use rand::RngExt;

pub fn generate_token() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; 20] = rng.random();
    hex::encode(bytes)
}

/// 40 lowercase hex chars.
pub fn is_valid_token_format(token: &str) -> bool {
    token.len() == 40
        && token
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_token() {
        let token = generate_token();
        assert_eq!(token.len(), 40);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_is_valid_token_format() {
        assert!(is_valid_token_format(
            "0123456789abcdef0123456789abcdef01234567"
        ));
        assert!(!is_valid_token_format(
            "0123456789ABCDEF0123456789abcdef01234567"
        )); // uppercase
        assert!(!is_valid_token_format("short")); // too short
        assert!(!is_valid_token_format(
            "0123456789abcdef0123456789abcdef0123456789"
        )); // too long
    }
}
