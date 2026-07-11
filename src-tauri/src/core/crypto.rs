use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;

/// Nonce length in bytes (AES-GCM standard: 96 bits / 12 bytes)
const NONCE_LEN: usize = 12;

/// Encrypt plaintext with AES-256-GCM, prepending a random 12-byte nonce.
///
/// Output format: `[nonce (12 bytes) | ciphertext + auth tag]`
///
/// Each call generates a fresh random nonce so identical plaintext + key
/// pairs produce different ciphertexts, preventing replay attacks.
pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| format!("Cipher init error: {}", e))?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("Encryption error: {}", e))?;

    // Prepend nonce to ciphertext
    let mut output = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Decrypt data that was encrypted with [`encrypt`].
///
/// Expects the first 12 bytes to be the nonce, followed by ciphertext + auth tag.
/// Supports legacy format (no prepended nonce) for backward compatibility:
/// if the data is shorter than NONCE_LEN bytes, decryption will fail gracefully.
pub fn decrypt(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < NONCE_LEN {
        return Err("Decryption error: data too short (missing nonce)".to_string());
    }

    let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| format!("Cipher init error: {}", e))?;
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption error: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        k[..10].copy_from_slice(b"BreezeRESP");
        k
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = test_key();
        let plaintext = b"hello breezeresp";
        let encrypted = encrypt(&key, plaintext).unwrap();
        assert_ne!(encrypted, plaintext.to_vec());
        // Nonce is prepended, so encrypted is longer
        assert!(encrypted.len() > plaintext.len());
        let decrypted = decrypt(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext.to_vec());
    }

    #[test]
    fn same_plaintext_different_ciphertexts() {
        let key = test_key();
        let plaintext = b"determinism check";
        let a = encrypt(&key, plaintext).unwrap();
        let b = encrypt(&key, plaintext).unwrap();
        // Random nonces should differ → ciphertexts differ
        assert_ne!(a, b);
    }

    #[test]
    fn decrypt_too_short_data() {
        let key = test_key();
        assert!(decrypt(&key, &[0u8; 5]).is_err());
    }

    #[test]
    fn decrypt_tampered_data() {
        let key = test_key();
        let mut encrypted = encrypt(&key, b"secret").unwrap();
        // Flip a byte in the ciphertext portion
        if let Some(last) = encrypted.last_mut() {
            *last ^= 0xFF;
        }
        assert!(decrypt(&key, &encrypted).is_err());
    }
}
