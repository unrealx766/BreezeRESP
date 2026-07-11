//! System Keychain-backed encryption key management.
//!
//! On first launch a cryptographically random 32-byte AES-256 key is generated
//! and persisted in the OS credential manager (Windows Credential Manager /
//! macOS Keychain / Linux Secret Service).  Subsequent launches read the key
//! directly from the credential manager.

use rand::RngCore;

const SERVICE: &str = "BreezeRESP";
const ACCOUNT: &str = "enc-key-v1";
const KEY_LEN: usize = 32;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Obtain the 32-byte encryption key.
///
/// - If the key already exists in the system Keychain -> read and return it.
/// - Otherwise -> generate a fresh random key and store it in the Keychain.
pub fn get_or_create_key() -> Result<[u8; 32], String> {
    let entry = keyring::Entry::new(SERVICE, ACCOUNT)
        .map_err(|e| format!("Keychain entry error: {}", e))?;

    match entry.get_password() {
        Ok(hex_str) => decode_key(&hex_str),
        Err(keyring::Error::NoEntry) => generate_and_store(&entry),
        Err(e) => Err(format!("Keychain read error: {}", e)),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Decode a hex-encoded 32-byte key.
fn decode_key(hex_str: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("Hex decode error: {}", e))?;
    if bytes.len() != KEY_LEN {
        return Err(format!(
            "Invalid key length in Keychain: expected {} bytes, got {}",
            KEY_LEN,
            bytes.len()
        ));
    }
    let mut key = [0u8; KEY_LEN];
    key.copy_from_slice(&bytes);
    Ok(key)
}

/// Generate a new random key and persist it to the system Keychain.
fn generate_and_store(entry: &keyring::Entry) -> Result<[u8; 32], String> {
    let mut key = [0u8; KEY_LEN];
    rand::thread_rng().fill_bytes(&mut key);

    let hex_str = hex::encode(key);
    entry
        .set_password(&hex_str)
        .map_err(|e| format!("Keychain write error: {}", e))?;

    log::info!("New encryption key generated and stored in system Keychain");
    Ok(key)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_key_valid_hex() {
        let hex_str = hex::encode([0xABu8; 32]);
        let key = decode_key(&hex_str).unwrap();
        assert_eq!(key, [0xABu8; 32]);
    }

    #[test]
    fn decode_key_wrong_length() {
        let hex_str = hex::encode([0u8; 16]);
        assert!(decode_key(&hex_str).is_err());
    }

    #[test]
    fn decode_key_invalid_hex() {
        assert!(decode_key("not_valid_hex!").is_err());
    }
}
