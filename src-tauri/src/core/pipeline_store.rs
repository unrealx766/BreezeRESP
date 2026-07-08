use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPipelineCommand {
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredPipeline {
    pub id: String,
    pub name: String,
    pub commands: Vec<StoredPipelineCommand>,
    #[serde(alias = "created_at")]
    pub created_at: u64,
}

/// Encrypted local storage for saved pipeline configurations.
pub struct PipelineStore {
    data_dir: PathBuf,
    encryption_key: [u8; 32],
}

impl PipelineStore {
    pub fn new(data_dir: PathBuf, encryption_key: [u8; 32]) -> Self {
        Self {
            data_dir,
            encryption_key,
        }
    }

    fn storage_path(&self) -> PathBuf {
        self.data_dir.join("pipelines.enc")
    }

    fn cipher(&self) -> Result<Aes256Gcm, String> {
        Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| format!("Cipher init error: {}", e))
    }

    /// Load all saved pipelines from encrypted storage.
    pub fn load_all(&self) -> Result<Vec<StoredPipeline>, String> {
        let path = self.storage_path();
        if !path.exists() {
            return Ok(vec![]);
        }
        let ciphertext = std::fs::read(&path).map_err(|e| e.to_string())?;
        let cipher = self.cipher()?;
        let nonce = Nonce::from_slice(&[0u8; 12]);
        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|e| format!("Decryption error: {}", e))?;
        let json = String::from_utf8(plaintext).map_err(|e| e.to_string())?;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }

    /// Save all pipelines (full overwrite).
    fn save_all(&self, pipelines: &[StoredPipeline]) -> Result<(), String> {
        let json = serde_json::to_string(pipelines).map_err(|e| e.to_string())?;
        let cipher = self.cipher()?;
        let nonce = Nonce::from_slice(&[0u8; 12]);
        let ciphertext = cipher
            .encrypt(nonce, json.as_bytes())
            .map_err(|e| format!("Encryption error: {}", e))?;
        std::fs::create_dir_all(&self.data_dir).map_err(|e| e.to_string())?;
        std::fs::write(self.storage_path(), ciphertext).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Save (upsert) a single pipeline by id.
    pub fn save(&self, pipeline: StoredPipeline) -> Result<(), String> {
        let mut pipelines = self.load_all()?;
        if let Some(existing) = pipelines.iter_mut().find(|p| p.id == pipeline.id) {
            *existing = pipeline;
        } else {
            pipelines.push(pipeline);
        }
        self.save_all(&pipelines)
    }

    /// Delete a pipeline by id.
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let mut pipelines = self.load_all()?;
        pipelines.retain(|p| p.id != id);
        self.save_all(&pipelines)
    }
}
