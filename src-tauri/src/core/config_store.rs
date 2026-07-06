use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Encrypted local storage for connection configurations.
pub struct ConfigStore {
    data_dir: PathBuf,
    encryption_key: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredConnection {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub password: String,
    pub db: u8,
    pub ssl: bool,
}

impl ConfigStore {
    pub fn new(data_dir: PathBuf, encryption_key: [u8; 32]) -> Self {
        Self {
            data_dir,
            encryption_key,
        }
    }

    fn config_path(&self) -> PathBuf {
        self.data_dir.join("connections.enc")
    }

    /// Encrypt and save connections to disk.
    pub fn save(&self, connections: &[StoredConnection]) -> Result<(), String> {
        let json = serde_json::to_string(connections).map_err(|e| e.to_string())?;
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| format!("Cipher init error: {}", e))?;

        let nonce_bytes = [0u8; 12]; // In production, use a random nonce
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, json.as_bytes())
            .map_err(|e| format!("Encryption error: {}", e))?;

        std::fs::create_dir_all(&self.data_dir).map_err(|e| e.to_string())?;
        std::fs::write(self.config_path(), ciphertext).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Load and decrypt connections from disk.
    pub fn load(&self) -> Result<Vec<StoredConnection>, String> {
        let path = self.config_path();
        if !path.exists() {
            return Ok(vec![]);
        }

        let ciphertext = std::fs::read(&path).map_err(|e| e.to_string())?;
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| format!("Cipher init error: {}", e))?;

        let nonce_bytes = [0u8; 12];
        let nonce = Nonce::from_slice(&nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|e| format!("Decryption error: {}", e))?;

        let json = String::from_utf8(plaintext).map_err(|e| e.to_string())?;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }
}
