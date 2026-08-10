//! Server capability probing: Redis version + installed modules (RedisJSON / RediSearch).
//!
//! The capability profile is probed once per connection and cached in-process.
//! Frontend pages use it to adapt the UI (hide unsupported panels, show
//! guidance cards), while backend commands verify support before executing.

use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

use crate::core::cluster::per_master_values;

/// Parsed semantic version (major, minor). Patch level is ignored for
/// feature-gating purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SemVer(u32, u32);

impl SemVer {
    fn at_least(self, major: u32, minor: u32) -> bool {
        self >= SemVer(major, minor)
    }
}

/// Parse a Redis version string like `7.4.1` or `255.255.255` (dev builds).
fn parse_version(raw: &str) -> Option<SemVer> {
    let mut parts = raw.trim().split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    Some(SemVer(major, minor))
}

/// Decode a Redis module version integer (`major*10000 + minor*100 + patch`).
fn decode_module_version(v: u64) -> String {
    format!("{}.{}.{}", v / 10000, (v / 100) % 100, v % 100)
}

/// Feature capability profile of a connected Redis server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapability {
    pub redis_version: String,
    /// `standalone` or `cluster` (from INFO server).
    pub redis_mode: String,
    /// Streams available (Redis >= 5.0).
    pub streams_supported: bool,
    /// XINFO STREAM FULL available (Redis >= 6.0).
    pub stream_full_supported: bool,
    /// XINFO STREAM FULL COUNT option available (Redis >= 7.0). Without
    /// COUNT the FULL form returns every entry and the complete PEL.
    pub stream_full_count_supported: bool,
    /// Extended stream info: consumer-group lag, max-deleted-entry-id,
    /// XAUTOCLAIM, XINFO STREAM ENTRIES (Redis >= 6.2).
    pub stream_extended_supported: bool,
    pub json_supported: bool,
    pub json_version: Option<String>,
    pub search_supported: bool,
    pub search_version: Option<String>,
    /// Vector search (KNN queries) requires RediSearch >= 2.4.
    pub vector_search_supported: bool,
}

/// Extract `key:value` lines from an INFO section response.
fn parse_info(info: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in info.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

/// Parse one MODULE LIST entry. Handles both the classic array form
/// `[name, ver, path, args...]` and the RESP3 map form.
fn parse_module_entry(entry: &redis::Value) -> Option<(String, Option<u64>)> {
    match entry {
        redis::Value::Array(items) => {
            let name: String = redis::from_redis_value(items.first()?).ok()?;
            let ver: Option<u64> = items
                .get(1)
                .and_then(|v| redis::from_redis_value::<u64>(v).ok());
            Some((name, ver))
        }
        redis::Value::Map(pairs) => {
            let mut name = None;
            let mut ver = None;
            for (k, v) in pairs {
                let key: String = redis::from_redis_value(k).ok()?;
                match key.as_str() {
                    "name" => name = redis::from_redis_value(v).ok(),
                    "ver" => ver = redis::from_redis_value(v).ok(),
                    _ => {}
                }
            }
            name.map(|n| (n, ver))
        }
        _ => None,
    }
}

/// Build the capability profile from raw INFO + MODULE LIST responses.
fn build_capability(info: &str, modules_raw: &redis::Value) -> ServerCapability {
    let info_map = parse_info(info);
    let redis_version = info_map
        .get("redis_version")
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());
    let redis_mode = info_map
        .get("redis_mode")
        .cloned()
        .unwrap_or_else(|| "standalone".to_string());

    let version = parse_version(&redis_version);
    let streams_supported = version.map_or(false, |v| v.at_least(5, 0));
    let stream_full_supported = version.map_or(false, |v| v.at_least(6, 0));
    let stream_full_count_supported = version.map_or(false, |v| v.at_least(7, 0));
    let stream_extended_supported = version.map_or(false, |v| v.at_least(6, 2));

    // Scan MODULE LIST for RedisJSON (module name "ReJSON"/"json") and
    // RediSearch (module name "search"/"ft").
    let mut json_supported = false;
    let mut json_version: Option<String> = None;
    let mut search_supported = false;
    let mut search_version: Option<String> = None;

    let entries: &[redis::Value] = match modules_raw {
        redis::Value::Array(items) => items,
        _ => &[],
    };
    for entry in entries {
        let Some((name, ver)) = parse_module_entry(entry) else {
            continue;
        };
        let lower = name.to_lowercase();
        let decoded = ver.map(decode_module_version);
        match lower.as_str() {
            "rejson" | "json" => {
                json_supported = true;
                json_version = decoded;
            }
            "search" | "ft" => {
                search_supported = true;
                search_version = decoded;
            }
            _ => {}
        }
    }

    // Vector search requires RediSearch >= 2.4.
    let vector_search_supported = search_supported
        && search_version
            .as_deref()
            .and_then(parse_version)
            .map_or(false, |v| v.at_least(2, 4));

    ServerCapability {
        redis_version,
        redis_mode,
        streams_supported,
        stream_full_supported,
        stream_full_count_supported,
        stream_extended_supported,
        json_supported,
        json_version,
        search_supported,
        search_version,
        vector_search_supported,
    }
}

/// Probe a standalone connection.
pub async fn probe(conn: &mut impl AsyncCommands) -> Result<ServerCapability, String> {
    let info: String = redis::cmd("INFO")
        .arg("server")
        .query_async(conn)
        .await
        .map_err(|e| format!("INFO server error: {}", e))?;
    let modules: redis::Value = redis::cmd("MODULE")
        .arg("LIST")
        .query_async(conn)
        .await
        .unwrap_or(redis::Value::Array(Vec::new()));
    Ok(build_capability(&info, &modules))
}

/// Probe a cluster (first master's response wins; modules and version are
/// expected to be homogeneous across the cluster).
pub async fn probe_cluster(
    conn: &mut redis::cluster_async::ClusterConnection,
) -> Result<ServerCapability, String> {
    let info_values = per_master_values(conn, redis::cmd("INFO").arg("server")).await?;
    let info: String = info_values
        .iter()
        .find_map(|(_, v)| redis::from_redis_value::<String>(v).ok())
        .ok_or_else(|| "Failed to read INFO from cluster nodes".to_string())?;

    let module_values = per_master_values(conn, redis::cmd("MODULE").arg("LIST"))
        .await
        .unwrap_or_default();
    let modules: redis::Value = module_values
        .into_iter()
        .map(|(_, v)| v)
        .next()
        .unwrap_or(redis::Value::Array(Vec::new()));

    Ok(build_capability(&info, &modules))
}

/// Probe either connection variant.
pub async fn probe_any(
    conn: &mut crate::core::pool::AnyConn,
) -> Result<ServerCapability, String> {
    if let Some(cluster) = conn.as_cluster() {
        return probe_cluster(cluster).await;
    }
    probe(conn).await
}

// ---------------------------------------------------------------------------
// In-process cache, keyed by connection id
// ---------------------------------------------------------------------------

static CACHE: Mutex<Option<HashMap<String, ServerCapability>>> = Mutex::new(None);

fn with_cache_mut<T>(f: impl FnOnce(&mut HashMap<String, ServerCapability>) -> T) -> Result<T, String> {
    let mut guard = CACHE.lock().map_err(|e| e.to_string())?;
    let map = guard.get_or_insert_with(HashMap::new);
    Ok(f(map))
}

/// Drop cached capability for a connection (called on disconnect).
pub fn clear_cached(connection_id: &str) {
    let _ = with_cache_mut(|map| map.remove(connection_id));
}

/// Get cached capability, probing on miss or forced refresh.
pub async fn get_or_probe(
    pool: &crate::core::pool::AnyPool,
    connection_id: &str,
    force: bool,
) -> Result<ServerCapability, String> {
    if !force {
        if let Ok(Some(cached)) = with_cache_mut(|map| map.get(connection_id).cloned()) {
            return Ok(cached);
        }
    }
    let mut conn = pool.get().await?;
    let cap = probe_any(&mut conn).await?;
    let _ = with_cache_mut(|map| map.insert(connection_id.to_string(), cap.clone()));
    Ok(cap)
}

/// Friendly error for unsupported features (includes current version + requirement).
pub fn unsupported_err(feature: &str, current: &str, required: &str) -> String {
    format!(
        "{} requires Redis {} but the connected server is {}",
        feature, required, current
    )
}
