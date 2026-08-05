//! Redis Streams data access: browse stream metadata, entries, consumer
//! groups and pending entries. Read commands adapt to the server version
//! (XINFO STREAM FULL on 6.0+, basic XINFO STREAM on 5.x).

use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::core::cluster::master_addrs;
use redis::cluster_routing::{RoutingInfo, SingleNodeRoutingInfo};

/// A single stream entry (message): id + field/value pairs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamEntry {
    pub id: String,
    pub fields: Vec<(String, String)>,
}

/// Summary metadata for a stream key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamInfo {
    pub key: String,
    pub length: u64,
    pub last_generated_id: String,
    pub groups: u64,
    pub first_entry: Option<StreamEntry>,
    pub last_entry: Option<StreamEntry>,
    /// Redis >= 6.2 only.
    pub max_deleted_entry_id: Option<String>,
    /// Redis >= 7.0 only.
    pub entries_added: Option<u64>,
    pub radix_tree_keys: Option<u64>,
    pub radix_tree_nodes: Option<u64>,
}

/// A consumer group of a stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumerGroup {
    pub name: String,
    pub consumers: u64,
    pub pending: u64,
    pub last_delivered_id: String,
    /// Redis >= 6.2 only. `None` when the lag is undeterminable.
    pub lag: Option<u64>,
    /// Redis >= 7.0 only.
    pub entries_read: Option<u64>,
}

/// A consumer inside a consumer group.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumerInfo {
    pub name: String,
    pub pending: u64,
    /// Idle time in milliseconds.
    pub idle_ms: u64,
    /// Redis >= 7.2 only.
    pub inactive_ms: Option<u64>,
}

/// A pending entry (message delivered but not yet acknowledged).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingEntry {
    pub id: String,
    pub consumer: String,
    /// Idle time in milliseconds since last delivery.
    pub idle_ms: u64,
    pub delivered_count: u64,
}

/// Parse a raw XRANGE entry: `[id, [f1, v1, f2, v2, ...]]`.
pub fn parse_entry(raw: &redis::Value) -> Result<StreamEntry, String> {
    let items = match raw {
        redis::Value::Array(items) => items,
        other => return Err(format!("Unexpected stream entry format: {:?}", other)),
    };
    if items.len() < 2 {
        return Err("Stream entry has fewer than 2 elements".to_string());
    }
    let id: String = redis::from_redis_value(&items[0])
        .map_err(|e| format!("Failed to parse entry id: {}", e))?;
    let flat: Vec<String> = redis::from_redis_value(&items[1])
        .map_err(|e| format!("Failed to parse entry fields: {}", e))?;
    let fields = flat
        .chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| (c[0].clone(), c[1].clone()))
        .collect();
    Ok(StreamEntry { id, fields })
}

/// Parse a flat `[name, value, name, value, ...]` response into a map of
/// string pairs (XINFO STREAM / XINFO GROUPS / XINFO CONSUMERS all use it).
fn parse_flat_pairs(raw: &redis::Value) -> Vec<(String, redis::Value)> {
    let items = match raw {
        redis::Value::Array(items) => items,
        _ => return Vec::new(),
    };
    items
        .chunks(2)
        .filter(|c| c.len() == 2)
        .filter_map(|c| {
            redis::from_redis_value::<String>(&c[0])
                .ok()
                .map(|k| (k, c[1].clone()))
        })
        .collect()
}

fn num(raw: &redis::Value) -> Option<u64> {
    redis::from_redis_value::<u64>(raw).ok()
}

fn str_val(raw: &redis::Value) -> Option<String> {
    redis::from_redis_value::<String>(raw).ok()
}

fn opt_entry(raw: &redis::Value) -> Option<StreamEntry> {
    match raw {
        redis::Value::Nil => None,
        other => parse_entry(other).ok(),
    }
}

/// Build a `StreamInfo` from an XINFO STREAM response (basic or FULL form —
/// both are flat key/value arrays; FULL simply contains more fields).
fn build_stream_info(key: &str, raw: &redis::Value) -> Result<StreamInfo, String> {
    let pairs = parse_flat_pairs(raw);
    if pairs.is_empty() {
        return Err("Unexpected XINFO STREAM response".to_string());
    }
    let get = |name: &str| pairs.iter().find(|(k, _)| k == name).map(|(_, v)| v);

    Ok(StreamInfo {
        key: key.to_string(),
        length: get("length").and_then(num).unwrap_or(0),
        last_generated_id: get("last-generated-id")
            .and_then(str_val)
            .unwrap_or_default(),
        groups: get("groups").and_then(num).unwrap_or(0),
        first_entry: get("first-entry").and_then(opt_entry),
        last_entry: get("last-entry").and_then(opt_entry),
        max_deleted_entry_id: get("max-deleted-entry-id").and_then(str_val),
        entries_added: get("entries-added").and_then(num),
        radix_tree_keys: get("radix-tree-keys").and_then(num),
        radix_tree_nodes: get("radix-tree-nodes").and_then(num),
    })
}

fn build_group(raw: &redis::Value) -> Option<ConsumerGroup> {
    let pairs = parse_flat_pairs(raw);
    let get = |name: &str| pairs.iter().find(|(k, _)| k == name).map(|(_, v)| v);
    Some(ConsumerGroup {
        name: get("name").and_then(str_val)?,
        consumers: get("consumers").and_then(num).unwrap_or(0),
        pending: get("pending").and_then(num).unwrap_or(0),
        last_delivered_id: get("last-delivered-id")
            .and_then(str_val)
            .unwrap_or_default(),
        lag: get("lag").and_then(num),
        entries_read: get("entries-read").and_then(num),
    })
}

fn build_consumer(raw: &redis::Value) -> Option<ConsumerInfo> {
    let pairs = parse_flat_pairs(raw);
    let get = |name: &str| pairs.iter().find(|(k, _)| k == name).map(|(_, v)| v);
    Some(ConsumerInfo {
        name: get("name").and_then(str_val)?,
        pending: get("pending").and_then(num).unwrap_or(0),
        idle_ms: get("idle").and_then(num).unwrap_or(0),
        inactive_ms: get("inactive").and_then(num),
    })
}

/// Parse the summary form of XPENDING: `[[id, consumer, idleMs, count], ...]`.
fn build_pending_entries(raw: &redis::Value) -> Vec<PendingEntry> {
    let items = match raw {
        redis::Value::Array(items) => items,
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter_map(|item| {
            let parts = match item {
                redis::Value::Array(parts) if parts.len() >= 4 => parts,
                _ => return None,
            };
            Some(PendingEntry {
                id: redis::from_redis_value(&parts[0]).ok()?,
                consumer: redis::from_redis_value(&parts[1]).ok()?,
                idle_ms: redis::from_redis_value(&parts[2]).ok()?,
                delivered_count: redis::from_redis_value(&parts[3]).ok()?,
            })
        })
        .collect()
}

/// Collects stream data from a Redis instance.
pub struct StreamsCollector;

/// Fallback for Redis < 6.0 (SCAN lacks the TYPE option): run a pipelined
/// TYPE check over the scanned batch and keep only stream keys.
async fn filter_stream_keys(
    conn: &mut impl AsyncCommands,
    batch: Vec<String>,
) -> Result<Vec<String>, String> {
    if batch.is_empty() {
        return Ok(Vec::new());
    }
    let mut pipe = redis::pipe();
    for k in &batch {
        pipe.cmd("TYPE").arg(k);
    }
    let types: Vec<String> = pipe
        .query_async(conn)
        .await
        .map_err(|e| format!("TYPE error: {}", e))?;
    Ok(batch
        .into_iter()
        .zip(types)
        .filter(|(_, t)| t == "stream")
        .map(|(k, _)| k)
        .collect())
}

impl StreamsCollector {
    pub fn new() -> Self {
        Self
    }

    /// List stream keys via `SCAN ... TYPE stream` (standalone).
    /// `type_filter_supported` must be true only on Redis >= 6.0; on 5.x the
    /// TYPE option is rejected, so we fall back to a plain SCAN plus a
    /// batched TYPE check per candidate key.
    pub async fn list_streams(
        &self,
        conn: &mut impl AsyncCommands,
        pattern: &str,
        limit: usize,
        type_filter_supported: bool,
    ) -> Result<Vec<String>, String> {
        let mut keys = Vec::new();
        let mut cursor: u64 = 0;
        loop {
            let mut cmd = redis::cmd("SCAN");
            cmd.arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(1000);
            if type_filter_supported {
                cmd.arg("TYPE").arg("stream");
            }
            let (next, batch): (u64, Vec<String>) = cmd
                .query_async(conn)
                .await
                .map_err(|e| format!("SCAN error: {}", e))?;
            let candidates = if type_filter_supported {
                batch
            } else {
                filter_stream_keys(conn, batch).await?
            };
            for k in candidates {
                if !keys.contains(&k) {
                    keys.push(k);
                }
                if keys.len() >= limit {
                    keys.sort();
                    return Ok(keys);
                }
            }
            cursor = next;
            if cursor == 0 {
                break;
            }
        }
        keys.sort();
        Ok(keys)
    }

    /// List stream keys across all cluster masters.
    pub async fn list_streams_cluster(
        &self,
        conn: &mut redis::cluster_async::ClusterConnection,
        pattern: &str,
        limit: usize,
        type_filter_supported: bool,
    ) -> Result<Vec<String>, String> {
        let addrs = master_addrs(conn).await?;
        let mut keys = Vec::new();
        for (host, port) in addrs {
            let mut cursor: u64 = 0;
            loop {
                let mut cmd = redis::cmd("SCAN");
                cmd.arg(cursor)
                    .arg("MATCH")
                    .arg(pattern)
                    .arg("COUNT")
                    .arg(1000);
                if type_filter_supported {
                    cmd.arg("TYPE").arg("stream");
                }
                let val = conn
                    .route_command(
                        &cmd,
                        RoutingInfo::SingleNode(SingleNodeRoutingInfo::ByAddress {
                            host: host.clone(),
                            port,
                        }),
                    )
                    .await
                    .map_err(|e| format!("SCAN error: {}", e))?;
                let elements = match val {
                    redis::Value::Array(items) if items.len() == 2 => items,
                    _ => break,
                };
                let next: u64 =
                    redis::from_redis_value(&elements[0]).unwrap_or(0);
                let batch: Vec<String> =
                    redis::from_redis_value(&elements[1]).unwrap_or_default();
                let candidates = if type_filter_supported {
                    batch
                } else {
                    filter_stream_keys(conn, batch).await?
                };
                for k in candidates {
                    if !keys.contains(&k) {
                        keys.push(k);
                    }
                    if keys.len() >= limit {
                        keys.sort();
                        return Ok(keys);
                    }
                }
                cursor = next;
                if cursor == 0 {
                    break;
                }
            }
        }
        keys.sort();
        Ok(keys)
    }

    /// Fetch stream metadata. Uses `XINFO STREAM FULL` when supported
    /// (6.0+), otherwise falls back to the basic form.
    pub async fn get_stream_info(
        &self,
        conn: &mut impl AsyncCommands,
        key: &str,
        full_supported: bool,
    ) -> Result<StreamInfo, String> {
        let mut cmd = redis::cmd("XINFO");
        cmd.arg("STREAM").arg(key);
        if full_supported {
            cmd.arg("FULL");
        }
        let raw: redis::Value = cmd
            .query_async(conn)
            .await
            .map_err(|e| format!("XINFO STREAM error: {}", e))?;
        build_stream_info(key, &raw)
    }

    /// Browse entries with XRANGE (`start`/`end` default to `-`/`+`).
    pub async fn get_entries(
        &self,
        conn: &mut impl AsyncCommands,
        key: &str,
        start: &str,
        end: &str,
        count: u64,
    ) -> Result<Vec<StreamEntry>, String> {
        let raw: redis::Value = redis::cmd("XRANGE")
            .arg(key)
            .arg(if start.is_empty() { "-" } else { start })
            .arg(if end.is_empty() { "+" } else { end })
            .arg("COUNT")
            .arg(count)
            .query_async(conn)
            .await
            .map_err(|e| format!("XRANGE error: {}", e))?;
        let items = match raw {
            redis::Value::Array(items) => items,
            _ => Vec::new(),
        };
        Ok(items
            .iter()
            .filter_map(|item| parse_entry(item).ok())
            .collect())
    }

    /// List consumer groups of a stream.
    pub async fn get_groups(
        &self,
        conn: &mut impl AsyncCommands,
        key: &str,
    ) -> Result<Vec<ConsumerGroup>, String> {
        let raw: redis::Value = redis::cmd("XINFO")
            .arg("GROUPS")
            .arg(key)
            .query_async(conn)
            .await
            .map_err(|e| format!("XINFO GROUPS error: {}", e))?;
        let items = match raw {
            redis::Value::Array(items) => items,
            _ => Vec::new(),
        };
        Ok(items.iter().filter_map(build_group).collect())
    }

    /// List consumers of a consumer group.
    pub async fn get_consumers(
        &self,
        conn: &mut impl AsyncCommands,
        key: &str,
        group: &str,
    ) -> Result<Vec<ConsumerInfo>, String> {
        let raw: redis::Value = redis::cmd("XINFO")
            .arg("CONSUMERS")
            .arg(key)
            .arg(group)
            .query_async(conn)
            .await
            .map_err(|e| format!("XINFO CONSUMERS error: {}", e))?;
        let items = match raw {
            redis::Value::Array(items) => items,
            _ => Vec::new(),
        };
        Ok(items.iter().filter_map(build_consumer).collect())
    }

    /// List pending entries (summary form of XPENDING).
    pub async fn get_pending(
        &self,
        conn: &mut impl AsyncCommands,
        key: &str,
        group: &str,
        count: u64,
    ) -> Result<Vec<PendingEntry>, String> {
        let raw: redis::Value = redis::cmd("XPENDING")
            .arg(key)
            .arg(group)
            .arg("-")
            .arg("+")
            .arg(count)
            .query_async(conn)
            .await
            .map_err(|e| format!("XPENDING error: {}", e))?;
        Ok(build_pending_entries(&raw))
    }

    /// XADD a message; returns the generated entry id.
    pub async fn add_message(
        &self,
        conn: &mut impl AsyncCommands,
        key: &str,
        id: &str,
        fields: &[(String, String)],
    ) -> Result<String, String> {
        let mut cmd = redis::cmd("XADD");
        cmd.arg(key).arg(if id.is_empty() { "*" } else { id });
        for (f, v) in fields {
            cmd.arg(f).arg(v);
        }
        cmd.query_async(conn)
            .await
            .map_err(|e| format!("XADD error: {}", e))
    }

    /// XTRIM MAXLEN (approximate with `~` when requested); returns removed count.
    pub async fn trim(
        &self,
        conn: &mut impl AsyncCommands,
        key: &str,
        max_len: u64,
        approximate: bool,
    ) -> Result<u64, String> {
        let mut cmd = redis::cmd("XTRIM");
        cmd.arg(key).arg("MAXLEN");
        if approximate {
            cmd.arg("~");
        }
        cmd.arg(max_len);
        cmd.query_async(conn)
            .await
            .map_err(|e| format!("XTRIM error: {}", e))
    }

    /// XDEL one or more entries; returns removed count.
    pub async fn delete_entries(
        &self,
        conn: &mut impl AsyncCommands,
        key: &str,
        ids: &[String],
    ) -> Result<u64, String> {
        let mut cmd = redis::cmd("XDEL");
        cmd.arg(key);
        for id in ids {
            cmd.arg(id);
        }
        cmd.query_async(conn)
            .await
            .map_err(|e| format!("XDEL error: {}", e))
    }

    /// XACK pending entries; returns acknowledged count.
    pub async fn ack(
        &self,
        conn: &mut impl AsyncCommands,
        key: &str,
        group: &str,
        ids: &[String],
    ) -> Result<u64, String> {
        let mut cmd = redis::cmd("XACK");
        cmd.arg(key).arg(group);
        for id in ids {
            cmd.arg(id);
        }
        cmd.query_async(conn)
            .await
            .map_err(|e| format!("XACK error: {}", e))
    }

    /// XGROUP DELCONSUMER; returns the number of removed pending entries.
    pub async fn delete_consumer(
        &self,
        conn: &mut impl AsyncCommands,
        key: &str,
        group: &str,
        consumer: &str,
    ) -> Result<u64, String> {
        redis::cmd("XGROUP")
            .arg("DELCONSUMER")
            .arg(key)
            .arg(group)
            .arg(consumer)
            .query_async(conn)
            .await
            .map_err(|e| format!("XGROUP DELCONSUMER error: {}", e))
    }

    /// XGROUP DESTROY a consumer group.
    pub async fn delete_group(
        &self,
        conn: &mut impl AsyncCommands,
        key: &str,
        group: &str,
    ) -> Result<(), String> {
        redis::cmd("XGROUP")
            .arg("DESTROY")
            .arg(key)
            .arg(group)
            .query_async(conn)
            .await
            .map_err(|e| format!("XGROUP DESTROY error: {}", e))
    }

    /// XCLAIM pending entries to another consumer; returns claimed entries.
    pub async fn claim(
        &self,
        conn: &mut impl AsyncCommands,
        key: &str,
        group: &str,
        consumer: &str,
        min_idle_ms: u64,
        ids: &[String],
    ) -> Result<Vec<StreamEntry>, String> {
        let mut cmd = redis::cmd("XCLAIM");
        cmd.arg(key).arg(group).arg(consumer).arg(min_idle_ms);
        for id in ids {
            cmd.arg(id);
        }
        let raw: redis::Value = cmd
            .query_async(conn)
            .await
            .map_err(|e| format!("XCLAIM error: {}", e))?;
        let items = match raw {
            redis::Value::Array(items) => items,
            _ => Vec::new(),
        };
        Ok(items
            .iter()
            .filter_map(|item| parse_entry(item).ok())
            .collect())
    }
}

impl Default for StreamsCollector {
    fn default() -> Self {
        Self::new()
    }
}
