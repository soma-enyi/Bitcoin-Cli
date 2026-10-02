use super::AppError;

pub fn pubkey_hex(pubkey: &str) -> Result<(), AppError> {
    if pubkey.is_empty() {
        return Err(AppError::Input("public key cannot be empty".into()));
    }

    if pubkey.len() != 66 && pubkey.len() != 130 {
        return Err(AppError::Input(format!(
            "public key must be 66 chars (compressed) or 130 chars (uncompressed), got {}",
            pubkey.len()
        )));
    }

    if !pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::Input("public key must be valid hex".into()));
    }

    if pubkey.len() == 66 {
        let first = &pubkey[..2];
        if first != "02" && first != "03" {
            return Err(AppError::Input(
                "compressed public key must start with 02 or 03".into(),
            ));
        }
    } else if !pubkey.starts_with("04") {
        return Err(AppError::Input(
            "uncompressed public key must start with 04".into(),
        ));
    }

    Ok(())
}

pub fn transaction_hex(hex: &str) -> Result<(), AppError> {
    if hex.is_empty() {
        return Err(AppError::Input("transaction hex cannot be empty".into()));
    }

    if hex.len() % 2 != 0 {
        return Err(AppError::Input(
            "transaction hex must have even number of characters".into(),
        ));
    }

    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::Input("transaction hex must contain only 0-9, a-f".into()));
    }

    if hex.len() < 20 {
        return Err(AppError::Input(format!(
            "transaction hex too short ({} chars, minimum 20)",
            hex.len()
        )));
    }

    Ok(())
}

pub fn block_height(height_str: &str) -> Result<u32, AppError> {
    if height_str.is_empty() {
        return Err(AppError::Input("block height cannot be empty".into()));
    }

    height_str.parse::<u32>().map_err(|_| {
        AppError::Input(format!(
            "block height must be a number between 0 and {}",
            u32::MAX
        ))
    })
}

pub fn block_hash(hash: &str) -> Result<(), AppError> {
    if hash.is_empty() {
        return Err(AppError::Input("block hash cannot be empty".into()));
    }

    if hash.len() != 64 {
        return Err(AppError::Input(format!(
            "block hash must be 64 hex chars, got {}",
            hash.len()
        )));
    }

    if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::Input("block hash must be valid hex (0-9, a-f)".into()));
    }

    Ok(())
}

pub fn fee_target(target: u16) -> Result<(), AppError> {
    if !(1..=1008).contains(&target) {
        return Err(AppError::Input(format!(
            "fee target blocks must be 1-1008, got {}",
            target
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "02c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5";

    #[test]
    fn pubkey_accepts_compressed() {
        assert!(pubkey_hex(KEY).is_ok());
    }

    #[test]
    fn pubkey_rejects_bad_input() {
        assert!(pubkey_hex("").is_err());
        assert!(pubkey_hex("02ab").is_err());
        assert!(pubkey_hex(&format!("05{}", &KEY[2..])).is_err());
        assert!(pubkey_hex(&format!("zz{}", &KEY[2..])).is_err());
    }

    #[test]
    fn tx_hex_rules() {
        assert!(transaction_hex("").is_err());
        assert!(transaction_hex("abc").is_err());
        assert!(transaction_hex("zzzzzzzzzzzzzzzzzzzzzz").is_err());
        assert!(transaction_hex("0100000001000000000000").is_ok());
    }

    #[test]
    fn block_ref_rules() {
        assert_eq!(block_height("101").unwrap(), 101);
        assert!(block_height("-1").is_err());
        assert!(block_height("99999999999").is_err());
        assert!(block_hash(&"a".repeat(64)).is_ok());
        assert!(block_hash("abcd").is_err());
    }

    #[test]
    fn fee_target_bounds() {
        assert!(fee_target(0).is_err());
        assert!(fee_target(1).is_ok());
        assert!(fee_target(1008).is_ok());
        assert!(fee_target(1009).is_err());
    }
}
