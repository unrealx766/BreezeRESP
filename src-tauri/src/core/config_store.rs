use crate::core::crypto;
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
    #[serde(default)]
    pub pinned: bool,
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
        let encrypted = crypto::encrypt(&self.encryption_key, json.as_bytes())?;

        std::fs::create_dir_all(&self.data_dir).map_err(|e| e.to_string())?;
        std::fs::write(self.config_path(), encrypted).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Load and decrypt connections from disk.
    pub fn load(&self) -> Result<Vec<StoredConnection>, String> {
        let path = self.config_path();
        if !path.exists() {
            return Ok(vec![]);
        }

        let data = std::fs::read(&path).map_err(|e| e.to_string())?;
        let plaintext = crypto::decrypt(&self.encryption_key, &data)?;

        let json = String::from_utf8(plaintext).map_err(|e| e.to_string())?;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }
}
