use crate::core::crypto;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Encrypted local storage for connection configurations.
pub struct ConfigStore {
    data_dir: PathBuf,
    encryption_key: [u8; 32],
}

#[derive(Clone, Serialize, Deserialize)]
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

impl std::fmt::Debug for StoredConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredConnection")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("password", &"[REDACTED]")
            .field("db", &self.db)
            .field("ssl", &self.ssl)
            .field("pinned", &self.pinned)
            .finish()
    }
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
    ///
    /// If the file exists but cannot be decrypted (e.g. encrypted with an
    /// older incompatible format), the file is treated as empty and a warning
    /// is logged so the application can continue gracefully.
    pub fn load(&self) -> Result<Vec<StoredConnection>, String> {
        let path = self.config_path();
        if !path.exists() {
            return Ok(vec![]);
        }

        let data = std::fs::read(&path).map_err(|e| e.to_string())?;
        let plaintext = match crypto::decrypt(&self.encryption_key, &data) {
            Ok(pt) => pt,
            Err(e) => {
                log::warn!("Failed to decrypt connections.enc, starting fresh: {}", e);
                return Ok(vec![]);
            }
        };

        let json = String::from_utf8(plaintext).map_err(|e| e.to_string())?;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }
}
