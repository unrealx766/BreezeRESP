use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::core::cluster::per_master_values;

/// A single entry from the Redis slow log.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlowlogEntry {
    pub id: u64,
    pub timestamp: i64,
    /// Execution duration in **microseconds**.
    pub duration_us: i64,
    /// The command with its arguments joined into a single readable string.
    pub command: String,
    /// Number of arguments (including the command name).
    pub args_count: u32,
    /// Client address and port (available since Redis 4.0).
    pub client_addr: Option<String>,
    /// Client name set via CLIENT SETNAME (available since Redis 4.0).
    pub client_name: Option<String>,
}

/// Aggregated slow-log information returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlowlogInfo {
    pub entries: Vec<SlowlogEntry>,
    /// Total number of entries in the slow log (SLOWLOG LEN).
    pub total_len: u64,
    /// Current threshold in microseconds (CONFIG GET slowlog-log-slower-than).
    pub slowlog_log_slower_than: i64,
}

/// Parse one raw SLOWLOG GET entry into a `SlowlogEntry`.
///
/// The Redis response for each entry is an array:
/// ```text
/// [id, timestamp, duration_us, [cmd, arg1, arg2, ...], client_addr?, client_name?]
/// ```
fn parse_entry(raw: &redis::Value) -> Result<SlowlogEntry, String> {
    let items = match raw {
        redis::Value::Array(items) => items,
        other => return Err(format!("Unexpected slowlog entry format: {:?}", other)),
    };

    if items.len() < 4 {
        return Err("Slowlog entry has fewer than 4 fields".to_string());
    }

    let id: u64 = redis::from_redis_value(&items[0])
        .map_err(|e| format!("Failed to parse slowlog id: {}", e))?;
    let timestamp: i64 = redis::from_redis_value(&items[1])
        .map_err(|e| format!("Failed to parse slowlog timestamp: {}", e))?;
    let duration_us: i64 = redis::from_redis_value(&items[2])
        .map_err(|e| format!("Failed to parse slowlog duration: {}", e))?;

    // Parse the command arguments array
    let cmd_args: Vec<String> = match &items[3] {
        redis::Value::Array(args) => args
            .iter()
            .map(|a| {
                redis::from_redis_value::<String>(a)
                    .unwrap_or_else(|_| format!("{:?}", a))
            })
            .collect(),
        other => {
            let s: String = redis::from_redis_value(other).unwrap_or_default();
            vec![s]
        }
    };

    let args_count = cmd_args.len() as u32;
    let command = cmd_args.join(" ");

    // Optional fields (Redis 4.0+)
    let client_addr = items.get(4).and_then(|v| {
        redis::from_redis_value::<String>(v).ok().filter(|s| !s.is_empty())
    });
    let client_name = items.get(5).and_then(|v| {
        redis::from_redis_value::<String>(v).ok().filter(|s| !s.is_empty())
    });

    Ok(SlowlogEntry {
        id,
        timestamp,
        duration_us,
        command,
        args_count,
        client_addr,
        client_name,
    })
}

/// Reads the `slowlog-log-slower-than` config value from an INFO-style key-value string.
fn parse_slower_than(config_resp: &str) -> i64 {
    for line in config_resp.lines() {
        if let Some(val) = line.strip_prefix("slowlog-log-slower-than:") {
            return val.trim().parse().unwrap_or(10000);
        }
    }
    10000 // default 10ms
}

/// Collects slow-log data from a Redis instance.
pub struct SlowlogCollector;

impl SlowlogCollector {
    pub fn new() -> Self {
        Self
    }

    /// Fetch slow-log entries from a single Redis instance.
    pub async fn collect(
        &self,
        conn: &mut impl AsyncCommands,
        count: u64,
    ) -> Result<SlowlogInfo, String> {
        // Fetch entries
        let raw_entries: redis::Value = redis::cmd("SLOWLOG")
            .arg("GET")
            .arg(count)
            .query_async(conn)
            .await
            .map_err(|e| format!("SLOWLOG GET error: {}", e))?;

        // Fetch total length
        let total_len: u64 = redis::cmd("SLOWLOG")
            .arg("LEN")
            .query_async(conn)
            .await
            .map_err(|e| format!("SLOWLOG LEN error: {}", e))?;

        // Fetch current threshold
        let config: String = redis::cmd("CONFIG")
            .arg("GET")
            .arg("slowlog-log-slower-than")
            .query_async(conn)
            .await
            .unwrap_or_else(|_| "slowlog-log-slower-than:10000".to_string());
        let slower_than = parse_slower_than(&config);

        // Parse entries
        let entries = match raw_entries {
            redis::Value::Array(items) => items
                .iter()
                .filter_map(|item| parse_entry(item).ok())
                .collect(),
            _ => Vec::new(),
        };

        Ok(SlowlogInfo {
            entries,
            total_len,
            slowlog_log_slower_than: slower_than,
        })
    }

    /// Fetch slow-log from every cluster master and merge into a single view.
    /// Entries are de-duplicated by ID and sorted by ID descending (newest first).
    pub async fn collect_cluster(
        &self,
        conn: &mut redis::cluster_async::ClusterConnection,
        count: u64,
    ) -> Result<SlowlogInfo, String> {
        // SLOWLOG GET from all masters
        let values = per_master_values(
            conn,
            redis::cmd("SLOWLOG").arg("GET").arg(count),
        )
        .await?;

        // SLOWLOG LEN from all masters (sum)
        let len_values = per_master_values(
            conn,
            redis::cmd("SLOWLOG").arg("LEN"),
        )
        .await?;
        let total_len: u64 = len_values
            .iter()
            .filter_map(|(_, v)| redis::from_redis_value::<u64>(v).ok())
            .sum();

        // CONFIG GET slowlog-log-slower-than (take first response)
        let slower_than = {
            let config_values = per_master_values(
                conn,
                redis::cmd("CONFIG").arg("GET").arg("slowlog-log-slower-than"),
            )
            .await
            .unwrap_or_default();
            config_values
                .first()
                .and_then(|(_, v)| {
                    let s: String = redis::from_redis_value(v).ok()?;
                    Some(parse_slower_than(&s))
                })
                .unwrap_or(10000)
        };

        // Parse and de-duplicate entries across nodes
        let mut seen_ids = std::collections::HashSet::new();
        let mut entries = Vec::new();
        for (_addr, value) in values {
            let items = match value {
                redis::Value::Array(items) => items,
                _ => continue,
            };
            for item in items {
                if let Ok(entry) = parse_entry(&item) {
                    if seen_ids.insert(entry.id) {
                        entries.push(entry);
                    }
                }
            }
        }

        // Sort by ID descending (newest first)
        entries.sort_by(|a, b| b.id.cmp(&a.id));

        Ok(SlowlogInfo {
            entries,
            total_len,
            slowlog_log_slower_than: slower_than,
        })
    }
}

impl Default for SlowlogCollector {
    fn default() -> Self {
        Self::new()
    }
}
