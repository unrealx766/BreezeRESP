use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerMetrics {
    pub used_memory: u64,
    pub total_memory: u64,
    pub version: String,
    pub connected_clients: u32,
    pub uptime_seconds: u64,
    pub used_cpu_sys: f64,
    pub used_cpu_user: f64,
    pub keyspace_hits: u64,
    pub keyspace_misses: u64,
    pub instantaneous_ops_per_sec: u64,
}

/// Collects real-time metrics from a Redis instance.
pub struct MetricsCollector;

impl MetricsCollector {
    pub fn new() -> Self {
        Self
    }

    /// Fetch INFO metrics from the Redis server.
    pub async fn collect(&self, conn: &mut impl AsyncCommands) -> Result<ServerMetrics, String> {
        let info: String = redis::cmd("INFO")
            .query_async(conn)
            .await
            .map_err(|e| format!("INFO error: {}", e))?;

        let get_val = |key: &str| -> String {
            for line in info.lines() {
                if let Some(val) = line.strip_prefix(&format!("{}:", key)) {
                    return val.trim().to_string();
                }
            }
            "0".to_string()
        };

        Ok(ServerMetrics {
            used_memory: get_val("used_memory").parse().unwrap_or(0),
            total_memory: get_val("total_system_memory").parse().unwrap_or(0),
            version: get_val("redis_version"),
            connected_clients: get_val("connected_clients").parse().unwrap_or(0),
            uptime_seconds: get_val("uptime_in_seconds").parse().unwrap_or(0),
            used_cpu_sys: get_val("used_cpu_sys").parse().unwrap_or(0.0),
            used_cpu_user: get_val("used_cpu_user").parse().unwrap_or(0.0),
            keyspace_hits: get_val("keyspace_hits").parse().unwrap_or(0),
            keyspace_misses: get_val("keyspace_misses").parse().unwrap_or(0),
            instantaneous_ops_per_sec: get_val("instantaneous_ops_per_sec").parse().unwrap_or(0),
        })
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}
