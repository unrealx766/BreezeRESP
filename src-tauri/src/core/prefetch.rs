use redis::AsyncCommands;

/// Efficiently scan and prefetch keys using SCAN command.
/// Designed for large keyspaces to avoid blocking with KEYS command.
#[allow(dead_code)]
pub struct KeyPrefetcher {
    batch_size: u64,
}

#[allow(dead_code)]
impl KeyPrefetcher {
    pub fn new(batch_size: u64) -> Self {
        Self { batch_size }
    }

    /// Perform a single SCAN iteration.
    /// Returns (next_cursor, keys_found).
    pub async fn scan_batch(
        &self,
        conn: &mut impl AsyncCommands,
        cursor: u64,
        pattern: &str,
    ) -> Result<(u64, Vec<String>), String> {
        let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(pattern)
            .arg("COUNT")
            .arg(self.batch_size)
            .query_async(conn)
            .await
            .map_err(|e| format!("SCAN error: {}", e))?;

        Ok((next_cursor, keys))
    }

    /// Get key type using TYPE command.
    pub async fn get_key_type(
        conn: &mut impl AsyncCommands,
        key: &str,
    ) -> Result<String, String> {
        let type_str: String = redis::cmd("TYPE")
            .arg(key)
            .query_async(conn)
            .await
            .map_err(|e| format!("TYPE error for '{}': {}", key, e))?;
        Ok(type_str)
    }

    /// Get key TTL using TTL command.
    pub async fn get_key_ttl(
        conn: &mut impl AsyncCommands,
        key: &str,
    ) -> Result<i64, String> {
        conn.ttl(key)
            .await
            .map_err(|e| format!("TTL error for '{}': {}", key, e))
    }
}

impl Default for KeyPrefetcher {
    fn default() -> Self {
        Self::new(100)
    }
}
