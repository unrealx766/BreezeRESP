use deadpool_redis::{Config, Pool, Runtime};
use std::collections::HashMap;
use std::sync::Mutex;

/// Manages multiple Redis connection pools, keyed by connection ID.
pub struct ConnectionPoolManager {
    pools: Mutex<HashMap<String, Pool>>,
}

impl ConnectionPoolManager {
    pub fn new() -> Self {
        Self {
            pools: Mutex::new(HashMap::new()),
        }
    }

    /// Create or retrieve a connection pool for the given connection config.
    pub fn get_or_create(
        &self,
        id: &str,
        host: &str,
        port: u16,
        password: Option<&str>,
        db: u8,
    ) -> Result<Pool, String> {
        let mut pools = self.pools.lock().map_err(|e| e.to_string())?;

        if let Some(pool) = pools.get(id) {
            return Ok(pool.clone());
        }

        let url = match password {
            Some(pw) if !pw.is_empty() => {
                let encoded_pw = urlencoding::encode(pw);
                format!("redis://:{}@{}:{}/{}", encoded_pw, host, port, db)
            }
            _ => format!("redis://{}:{}/{}", host, port, db),
        };

        let cfg = Config::from_url(url);
        let pool = cfg
            .create_pool(Some(Runtime::Tokio1))
            .map_err(|e| format!("Failed to create pool: {}", e))?;

        pools.insert(id.to_string(), pool.clone());
        Ok(pool)
    }

    /// Remove a pool (on disconnect).
    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut pools = self.pools.lock().map_err(|e| e.to_string())?;
        pools.remove(id);
        Ok(())
    }

    /// Retrieve an existing connection pool by ID.
    pub fn get_pool(&self, id: &str) -> Result<Pool, String> {
        let pools = self.pools.lock().map_err(|e| e.to_string())?;
        pools
            .get(id)
            .cloned()
            .ok_or_else(|| format!("No connection pool for id: {}", id))
    }

    /// Check if a connection pool exists.
    pub fn has(&self, id: &str) -> bool {
        self.pools
            .lock()
            .map(|p| p.contains_key(id))
            .unwrap_or(false)
    }
}

impl Default for ConnectionPoolManager {
    fn default() -> Self {
        Self::new()
    }
}
