use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::core::cluster::per_master_values;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

/// Parse a raw INFO payload into `ServerMetrics`.
fn parse_info(info: &str) -> ServerMetrics {
    let get_val = |key: &str| -> String {
        for line in info.lines() {
            if let Some(val) = line.strip_prefix(&format!("{}:", key)) {
                return val.trim().to_string();
            }
        }
        "0".to_string()
    };

    ServerMetrics {
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
    }
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
        Ok(parse_info(&info))
    }

    /// Fetch INFO from every cluster master and aggregate into a single view.
    /// Counters are summed; version comes from the first node and uptime is
    /// the minimum across nodes (youngest member).
    pub async fn collect_cluster(
        &self,
        conn: &mut redis::cluster_async::ClusterConnection,
    ) -> Result<ServerMetrics, String> {
        let values = per_master_values(conn, &redis::cmd("INFO")).await?;

        let mut aggregated: Option<ServerMetrics> = None;
        for (_addr, value) in values {
            let info: String = redis::from_redis_value(&value)
                .map_err(|e| format!("INFO parse error: {}", e))?;
            let m = parse_info(&info);
            aggregated = Some(match aggregated {
                None => m,
                Some(mut acc) => {
                    acc.used_memory += m.used_memory;
                    acc.total_memory += m.total_memory;
                    acc.connected_clients += m.connected_clients;
                    acc.uptime_seconds = acc.uptime_seconds.min(m.uptime_seconds);
                    acc.used_cpu_sys += m.used_cpu_sys;
                    acc.used_cpu_user += m.used_cpu_user;
                    acc.keyspace_hits += m.keyspace_hits;
                    acc.keyspace_misses += m.keyspace_misses;
                    acc.instantaneous_ops_per_sec += m.instantaneous_ops_per_sec;
                    acc
                }
            });
        }
        aggregated.ok_or_else(|| "No cluster nodes responded to INFO".to_string())
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}
