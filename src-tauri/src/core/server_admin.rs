use serde::{Deserialize, Serialize};

use crate::core::cluster::{master_addrs, per_master_values, per_node_values};
use crate::core::pool::AnyConn;
use redis::cluster_routing::{MultipleNodeRoutingInfo, RoutingInfo, SingleNodeRoutingInfo};

/// Total number of hash slots in a Redis Cluster.
pub const TOTAL_SLOTS: u64 = 16384;

/// CONFIG SET parameters that could be abused for host-level damage
/// (classic RCE vectors via CONFIG SET dir / dbfilename).
const BLOCKED_CONFIG_PARAMS: &[&str] = &["dir", "dbfilename", "logfile"];

// ---------------------------------------------------------------------------
// INFO
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoNode {
    /// Node address; "server" for standalone connections.
    pub addr: String,
    pub info: String,
}

/// Fetch raw INFO text (optionally one section). Cluster connections return
/// one entry per node.
pub async fn fetch_info(
    conn: &mut AnyConn,
    section: Option<&str>,
) -> Result<Vec<InfoNode>, String> {
    let mut cmd = redis::cmd("INFO");
    if let Some(section) = section {
        cmd.arg(section);
    }

    if let Some(cluster) = conn.as_cluster() {
        let values = per_node_values(cluster, &cmd).await?;
        let mut nodes = Vec::with_capacity(values.len());
        for (addr, value) in values {
            let info: String = redis::from_redis_value(&value)
                .map_err(|e| format!("INFO parse error: {}", e))?;
            nodes.push(InfoNode { addr, info });
        }
        nodes.sort_by(|a, b| a.addr.cmp(&b.addr));
        return Ok(nodes);
    }

    let info: String = cmd
        .query_async(conn)
        .await
        .map_err(|e| format!("INFO error: {}", e))?;
    Ok(vec![InfoNode {
        addr: "server".to_string(),
        info,
    }])
}

// ---------------------------------------------------------------------------
// CONFIG
// ---------------------------------------------------------------------------

/// CONFIG GET as (name, value) pairs. Cluster: merged across all masters
/// (first responder wins per parameter).
pub async fn config_get(conn: &mut AnyConn, pattern: &str) -> Result<Vec<(String, String)>, String> {
    let mut cmd = redis::cmd("CONFIG");
    cmd.arg("GET").arg(pattern);

    let pairs: Vec<(String, String)> = if let Some(cluster) = conn.as_cluster() {
        let values = per_master_values(cluster, &cmd).await?;
        let mut merged: Vec<(String, String)> = Vec::new();
        for (_addr, value) in values {
            for (name, val) in parse_config_pairs(&value) {
                if !merged.iter().any(|(n, _)| n == &name) {
                    merged.push((name, val));
                }
            }
        }
        merged
    } else {
        let value: redis::Value = cmd
            .query_async(conn)
            .await
            .map_err(|e| format!("CONFIG GET error: {}", e))?;
        parse_config_pairs(&value)
    };

    Ok(pairs)
}

fn parse_config_pairs(value: &redis::Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut push_pair = |k: &redis::Value, v: &redis::Value| {
        if let (Ok(name), Ok(val)) = (
            redis::from_redis_value::<String>(k),
            redis::from_redis_value::<String>(v),
        ) {
            out.push((name, val));
        }
    };
    match value {
        redis::Value::Map(entries) => {
            for (k, v) in entries {
                push_pair(k, v);
            }
        }
        redis::Value::Array(items) => {
            let mut i = 0;
            while i + 1 < items.len() {
                push_pair(&items[i], &items[i + 1]);
                i += 2;
            }
        }
        _ => {}
    }
    out
}

/// CONFIG SET on the whole deployment. Cluster: applied to every node.
/// Returns an error listing per-node failures when any node rejects.
pub async fn config_set(conn: &mut AnyConn, param: &str, value: &str) -> Result<(), String> {
    let lower = param.to_ascii_lowercase();
    if BLOCKED_CONFIG_PARAMS.contains(&lower.as_str()) {
        return Err(format!(
            "CONFIG SET '{}' is blocked for security reasons",
            param
        ));
    }

    let mut cmd = redis::cmd("CONFIG");
    cmd.arg("SET").arg(param).arg(value);

    if let Some(cluster) = conn.as_cluster() {
        let val = cluster
            .route_command(
                &cmd,
                RoutingInfo::MultiNode((MultipleNodeRoutingInfo::AllNodes, None)),
            )
            .await
            .map_err(|e| format!("CONFIG SET error: {}", e))?;
        // Per-node results arrive as a map; surface individual failures.
        if let redis::Value::Map(entries) = val {
            let mut errors = Vec::new();
            for (addr, result) in entries {
                let addr: String = redis::from_redis_value(&addr).unwrap_or_default();
                if redis::from_redis_value::<()>(&result).is_err() {
                    errors.push(format!(
                        "{}: {}",
                        addr,
                        redis::from_redis_value::<String>(&result).unwrap_or_else(|_| "failed".into())
                    ));
                }
            }
            if !errors.is_empty() {
                return Err(errors.join("; "));
            }
        }
        return Ok(());
    }

    let _: () = cmd
        .query_async(conn)
        .await
        .map_err(|e| format!("CONFIG SET error: {}", e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// CLIENT LIST / KILL
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub id: u64,
    pub addr: String,
    pub name: String,
    pub age: u64,
    pub idle: u64,
    pub flags: String,
    pub db: u32,
    pub cmd: String,
    pub user: String,
    /// Node address the client is connected to (cluster only).
    pub node: String,
}

/// CLIENT LIST parsed into structured rows. Cluster: merged from every node.
pub async fn client_list(conn: &mut AnyConn) -> Result<Vec<ClientInfo>, String> {
    let mut rows = Vec::new();

    if let Some(cluster) = conn.as_cluster() {
        let values = per_node_values(cluster, &redis::cmd("CLIENT").arg("LIST")).await?;
        for (addr, value) in values {
            let text: String = match redis::from_redis_value(&value) {
                Ok(t) => t,
                Err(_) => continue,
            };
            for line in text.lines() {
                if let Some(info) = parse_client_line(line) {
                    rows.push(ClientInfo {
                        node: addr.clone(),
                        ..info
                    });
                }
            }
        }
        return Ok(rows);
    }

    let text: String = redis::cmd("CLIENT")
        .arg("LIST")
        .query_async(conn)
        .await
        .map_err(|e| format!("CLIENT LIST error: {}", e))?;
    for line in text.lines() {
        if let Some(mut info) = parse_client_line(line) {
            info.node = "server".to_string();
            rows.push(info);
        }
    }
    Ok(rows)
}

fn parse_client_line(line: &str) -> Option<ClientInfo> {
    let mut info = ClientInfo {
        id: 0,
        addr: String::new(),
        name: String::new(),
        age: 0,
        idle: 0,
        flags: String::new(),
        db: 0,
        cmd: String::new(),
        user: String::new(),
        node: String::new(),
    };
    let mut seen_id = false;
    for part in line.split_whitespace() {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        match k {
            "id" => {
                info.id = v.parse().ok()?;
                seen_id = true;
            }
            "addr" => info.addr = v.to_string(),
            "name" => info.name = v.to_string(),
            "age" => info.age = v.parse().unwrap_or(0),
            "idle" => info.idle = v.parse().unwrap_or(0),
            "flags" => info.flags = v.to_string(),
            "db" => info.db = v.parse().unwrap_or(0),
            "cmd" => info.cmd = v.to_string(),
            "user" => info.user = v.to_string(),
            _ => {}
        }
    }
    seen_id.then_some(info)
}

/// CLIENT KILL ID <id>. Cluster: broadcast to every node; succeeds when at
/// least one node killed the client.
pub async fn client_kill(conn: &mut AnyConn, client_id: u64) -> Result<bool, String> {
    let mut cmd = redis::cmd("CLIENT");
    cmd.arg("KILL").arg("ID").arg(client_id);

    if let Some(cluster) = conn.as_cluster() {
        let val = cluster
            .route_command(
                &cmd,
                RoutingInfo::MultiNode((MultipleNodeRoutingInfo::AllNodes, None)),
            )
            .await
            .map_err(|e| format!("CLIENT KILL error: {}", e))?;
        // The cluster multi-node response may arrive as a Map (addr→result)
        // or an Array of per-node results, depending on the redis crate
        // version and protocol (RESP2 vs RESP3).
        match val {
            redis::Value::Map(entries) => {
                for (_addr, result) in entries {
                    if value_to_kill_count(&result) > 0 {
                        return Ok(true);
                    }
                }
            }
            redis::Value::Array(items) => {
                for result in items {
                    if value_to_kill_count(&result) > 0 {
                        return Ok(true);
                    }
                }
            }
            other => return Ok(value_to_kill_count(&other) > 0),
        }
        return Ok(false);
    }

    // Standalone: parse as generic Value to tolerate different response shapes.
    let val: redis::Value = cmd
        .query_async(conn)
        .await
        .map_err(|e| format!("CLIENT KILL error: {}", e))?;
    Ok(value_to_kill_count(&val) > 0)
}

/// Extract the number of killed clients from a CLIENT KILL response.
/// Redis returns an integer, but we tolerate non-integer shapes (e.g. OK
/// means the legacy single-client form succeeded).
fn value_to_kill_count(val: &redis::Value) -> u64 {
    match val {
        redis::Value::Int(n) => (*n).max(0) as u64,
        redis::Value::Okay => 1,
        _ => redis::from_redis_value::<u64>(val).unwrap_or(0),
    }
}

// ---------------------------------------------------------------------------
// OBJECT FREQ (hot keys under an LFU eviction policy)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyFreq {
    pub key: String,
    /// None when the server rejects OBJECT FREQ (non-LFU policy / old version).
    pub freq: Option<u64>,
}

/// Batched OBJECT FREQ lookup. Errors are propagated per key as `freq: None`.
pub async fn object_freq(conn: &mut AnyConn, keys: &[String]) -> Result<Vec<KeyFreq>, String> {
    let mut out = Vec::with_capacity(keys.len());

    if matches!(conn, AnyConn::Cluster(_)) {
        for key in keys {
            let freq: Result<u64, _> =
                redis::cmd("OBJECT").arg("FREQ").arg(key).query_async(conn).await;
            out.push(KeyFreq {
                key: key.clone(),
                freq: freq.ok(),
            });
        }
        return Ok(out);
    }

    let mut pipe = redis::pipe();
    for key in keys {
        pipe.cmd("OBJECT").arg("FREQ").arg(key);
    }
    let values: Vec<redis::Value> = pipe.query_async(conn).await.unwrap_or_default();
    for (i, key) in keys.iter().enumerate() {
        let freq = values
            .get(i)
            .and_then(|v| redis::from_redis_value::<u64>(v).ok());
        out.push(KeyFreq {
            key: key.clone(),
            freq,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Command statistics (INFO commandstats)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CmdStat {
    /// Command name without the "cmdstat_" prefix, e.g. "get".
    pub cmd: String,
    pub calls: u64,
    /// Cumulative execution time in microseconds.
    pub total_usec: u64,
    /// Rejected calls (Redis 6.2+; 0 on older servers).
    pub rejected_calls: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CmdStatNode {
    /// Node address; "server" for standalone connections.
    pub addr: String,
    pub stats: Vec<CmdStat>,
}

/// Parse one INFO commandstats payload. Handles both the classic microsecond
/// fields (usec / usec_per_call, ≤ Redis 7) and the millisecond fields Redis
/// 8 introduced (msec / msec_per_call).
fn parse_commandstats(info: &str) -> Vec<CmdStat> {
    let mut stats = Vec::new();
    for line in info.lines() {
        let Some(rest) = line.strip_prefix("cmdstat_") else {
            continue;
        };
        let Some((cmd, fields)) = rest.split_once(':') else {
            continue;
        };
        let mut stat = CmdStat {
            cmd: cmd.to_string(),
            calls: 0,
            total_usec: 0,
            rejected_calls: 0,
        };
        for part in fields.split(',') {
            if let Some((k, v)) = part.split_once('=') {
                match k {
                    "calls" => stat.calls = v.parse().unwrap_or(0),
                    "rejected_calls" => stat.rejected_calls = v.parse().unwrap_or(0),
                    "usec" => stat.total_usec = v.parse().unwrap_or(0),
                    // Redis 8 reports milliseconds; normalize to microseconds.
                    "msec" => stat.total_usec = v.parse::<u64>().unwrap_or(0).saturating_mul(1000),
                    _ => {}
                }
            }
        }
        stats.push(stat);
    }
    stats.sort_by(|a, b| b.total_usec.cmp(&a.total_usec));
    stats
}

/// Per-node command statistics. Cluster connections merge every node into a
/// single aggregated entry labeled "cluster"; standalone returns "server".
pub async fn command_stats(conn: &mut AnyConn) -> Result<Vec<CmdStatNode>, String> {
    let mut cmd = redis::cmd("INFO");
    cmd.arg("commandstats");

    if let Some(cluster) = conn.as_cluster() {
        let values = per_node_values(cluster, &cmd).await?;
        let mut merged: Vec<CmdStat> = Vec::new();
        for (_addr, value) in values {
            let Ok(info) = redis::from_redis_value::<String>(&value) else {
                continue;
            };
            for stat in parse_commandstats(&info) {
                if let Some(existing) = merged.iter_mut().find(|s| s.cmd == stat.cmd) {
                    existing.calls += stat.calls;
                    existing.total_usec += stat.total_usec;
                    existing.rejected_calls += stat.rejected_calls;
                } else {
                    merged.push(stat);
                }
            }
        }
        merged.sort_by(|a, b| b.total_usec.cmp(&a.total_usec));
        return Ok(vec![CmdStatNode {
            addr: "cluster".to_string(),
            stats: merged,
        }]);
    }

    let info: String = cmd
        .query_async(conn)
        .await
        .map_err(|e| format!("INFO commandstats error: {}", e))?;
    Ok(vec![CmdStatNode {
        addr: "server".to_string(),
        stats: parse_commandstats(&info),
    }])
}

// ---------------------------------------------------------------------------
// Cluster topology
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterNodeInfo {
    pub id: String,
    pub addr: String,
    /// "master" | "replica"
    pub role: String,
    /// Empty string for masters.
    pub master_id: String,
    /// Normalized health flags: "connected" | "disconnected" | "fail" | "noaddr".
    pub flags: String,
    /// Inclusive slot ranges owned by this node (masters only).
    pub slots: Vec<(u16, u16)>,
    pub used_memory: u64,
    pub connected_clients: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterTopology {
    pub cluster_enabled: bool,
    pub slots_assigned: u64,
    pub total_slots: u64,
    pub nodes: Vec<ClusterNodeInfo>,
}

/// Build the cluster topology: prefers CLUSTER SHARDS (Redis 7+), falls back
/// to parsing CLUSTER NODES text. Node-level metrics come from per-node INFO.
pub async fn cluster_topology(conn: &mut AnyConn) -> Result<ClusterTopology, String> {
    let Some(cluster) = conn.as_cluster() else {
        return Ok(ClusterTopology {
            cluster_enabled: false,
            slots_assigned: 0,
            total_slots: TOTAL_SLOTS,
            nodes: Vec::new(),
        });
    };

    // Route the introspection command to the first known master.
    let masters = master_addrs(cluster).await?;
    let (host, port) = masters[0].clone();
    let routing = RoutingInfo::SingleNode(SingleNodeRoutingInfo::ByAddress { host, port });

    let mut nodes: Vec<ClusterNodeInfo> = Vec::new();

    // Attempt CLUSTER SHARDS first.
    let shards_val = cluster
        .route_command(&redis::cmd("CLUSTER").arg("SHARDS"), routing.clone())
        .await;
    match shards_val {
        Ok(value) => {
            nodes = parse_cluster_shards(&value);
        }
        Err(_) => {}
    }

    // Fallback: CLUSTER NODES text.
    if nodes.is_empty() {
        let text: String = cluster
            .route_command(&redis::cmd("CLUSTER").arg("NODES"), routing)
            .await
            .map_err(|e| format!("CLUSTER NODES error: {}", e))
            .and_then(|v| {
                redis::from_redis_value::<String>(&v)
                    .map_err(|e| format!("CLUSTER NODES parse error: {}", e))
            })?;
        nodes = parse_cluster_nodes_text(&text);
    }

    if nodes.is_empty() {
        return Err("Unable to read cluster topology".to_string());
    }

    // Per-node metrics from INFO (best-effort; matched by address).
    if let Ok(values) = per_node_values(cluster, &redis::cmd("INFO")).await {
        let mut metrics: Vec<(String, u64, u64)> = Vec::new();
        for (addr, value) in values {
            if let Ok(info) = redis::from_redis_value::<String>(&value) {
                metrics.push((
                    addr,
                    extract_info_u64(&info, "used_memory"),
                    extract_info_u64(&info, "connected_clients"),
                ));
            }
        }
        for node in &mut nodes {
            if let Some((_, mem, clients)) = metrics
                .iter()
                .find(|(addr, _, _)| addrs_match(addr, &node.addr))
            {
                node.used_memory = *mem;
                node.connected_clients = *clients;
            }
        }
    }

    let slots_assigned: u64 = nodes
        .iter()
        .map(|n| n.slots.iter().map(|(a, b)| (*b as u64) - (*a as u64) + 1).sum::<u64>())
        .sum();

    // Masters first, then by address for stable rendering.
    nodes.sort_by(|a, b| {
        b.role
            .cmp(&a.role)
            .then_with(|| a.addr.cmp(&b.addr))
    });

    Ok(ClusterTopology {
        cluster_enabled: true,
        slots_assigned,
        total_slots: TOTAL_SLOTS,
        nodes,
    })
}

/// Loose address matching: "ip:port" may differ in representation
/// (IPv6 brackets, announce addresses), so compare the port-suffix too.
fn addrs_match(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let strip = |s: &str| s.trim_start_matches('[').trim_end_matches(']').to_string();
    strip(a) == strip(b)
}

fn extract_info_u64(info: &str, key: &str) -> u64 {
    let prefix = format!("{}:", key);
    for line in info.lines() {
        if let Some(val) = line.strip_prefix(&prefix) {
            return val.trim().parse().unwrap_or(0);
        }
    }
    0
}

/// Fetch a named field from a map-like redis::Value (RESP3 Map or RESP2 flat
/// pair array).
fn value_field<'a>(entry: &'a redis::Value, name: &str) -> Option<&'a redis::Value> {
    match entry {
        redis::Value::Map(entries) => entries
            .iter()
            .find(|(k, _)| redis::from_redis_value::<String>(k).ok().as_deref() == Some(name))
            .map(|(_, v)| v),
        redis::Value::Array(items) => {
            let mut i = 0;
            while i + 1 < items.len() {
                if redis::from_redis_value::<String>(&items[i]).ok().as_deref() == Some(name) {
                    return Some(&items[i + 1]);
                }
                i += 2;
            }
            None
        }
        _ => None,
    }
}

fn value_str(v: &redis::Value) -> String {
    redis::from_redis_value::<String>(v).unwrap_or_default()
}

/// Parse the CLUSTER SHARDS response into node descriptors.
fn parse_cluster_shards(value: &redis::Value) -> Vec<ClusterNodeInfo> {
    let mut nodes = Vec::new();
    let shards = match value {
        redis::Value::Array(shards) => shards,
        _ => return nodes,
    };

    for shard in shards {
        let slots: Vec<(u16, u16)> = match value_field(shard, "slots") {
            Some(redis::Value::Array(ranges)) => {
                let mut out = Vec::new();
                let mut i = 0;
                while i + 1 < ranges.len() {
                    let start: u16 = redis::from_redis_value(&ranges[i]).unwrap_or(0);
                    let end: u16 = redis::from_redis_value(&ranges[i + 1]).unwrap_or(0);
                    out.push((start, end));
                    i += 2;
                }
                out
            }
            _ => Vec::new(),
        };

        let shard_nodes = match value_field(shard, "nodes") {
            Some(redis::Value::Array(list)) => list,
            _ => continue,
        };

        for node in shard_nodes {
            let role = value_field(node, "role").map(value_str).unwrap_or_default();
            let health = value_field(node, "health").map(value_str).unwrap_or_default();
            let link_state = value_field(node, "link-state").map(value_str).unwrap_or_default();

            let flags = if health == "failed" {
                "fail".to_string()
            } else if health == "loading" {
                "loading".to_string()
            } else if link_state == "disconnected" {
                "disconnected".to_string()
            } else {
                "connected".to_string()
            };

            // Prefer "endpoint", fall back to ip + port.
            let addr = match value_field(node, "endpoint").map(value_str) {
                Some(endpoint) if !endpoint.is_empty() => endpoint,
                _ => {
                    let ip = value_field(node, "ip").map(value_str).unwrap_or_default();
                    let port = value_field(node, "port")
                        .and_then(|v| redis::from_redis_value::<u16>(v).ok())
                        .unwrap_or(0);
                    format!("{}:{}", ip, port)
                }
            };

            nodes.push(ClusterNodeInfo {
                id: value_field(node, "id").map(value_str).unwrap_or_default(),
                addr,
                role: if role == "replica" { "replica".to_string() } else { "master".to_string() },
                master_id: String::new(), // resolved below from shard grouping
                flags,
                slots: if role == "master" { slots.clone() } else { Vec::new() },
                used_memory: 0,
                connected_clients: 0,
            });
        }

        // Replicas in this shard replicate the shard's master.
        let master_id = nodes
            .iter()
            .rev()
            .find(|n| n.role == "master" && shard_node_belongs(shard, &n.id))
            .map(|n| n.id.clone())
            .unwrap_or_default();
        for node in nodes.iter_mut().rev() {
            if node.role == "replica" && shard_node_belongs(shard, &node.id) && node.master_id.is_empty() {
                node.master_id = master_id.clone();
            }
        }
    }

    nodes
}

fn shard_node_belongs(shard: &redis::Value, id: &str) -> bool {
    match value_field(shard, "nodes") {
        Some(redis::Value::Array(list)) => list.iter().any(|n| {
            value_field(n, "id").map(value_str).as_deref() == Some(id)
        }),
        _ => false,
    }
}

/// Parse the legacy CLUSTER NODES text format.
/// Line: `<id> <ip:port@cport> <flags> <master|-> <ping> <pong> <epoch> <link-state> [slots...]`
fn parse_cluster_nodes_text(text: &str) -> Vec<ClusterNodeInfo> {
    let mut nodes = Vec::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 {
            continue;
        }
        let id = parts[0].to_string();
        // Strip the cluster bus suffix: 10.0.0.1:7000@17000
        let addr = parts[1]
            .split('@')
            .next()
            .unwrap_or(parts[1])
            .to_string();
        let raw_flags = parts[2];
        let master_field = parts[3];
        let link_state = parts[7];

        let role = if raw_flags.contains("master") {
            "master"
        } else {
            "replica"
        };
        let flags = if raw_flags.contains("fail") && !raw_flags.contains("fail?") {
            "fail".to_string()
        } else if raw_flags.contains("noaddr") {
            "noaddr".to_string()
        } else if link_state == "disconnected" {
            "disconnected".to_string()
        } else {
            "connected".to_string()
        };

        let mut slots = Vec::new();
        for range in &parts[8..] {
            // Skip importing/migrating markers like [13->-...] / [13-<-...].
            if range.starts_with('[') {
                continue;
            }
            if let Some((a, b)) = range.split_once('-') {
                if let (Ok(start), Ok(end)) = (a.parse::<u16>(), b.parse::<u16>()) {
                    slots.push((start, end));
                }
            } else if let Ok(single) = range.parse::<u16>() {
                slots.push((single, single));
            }
        }

        nodes.push(ClusterNodeInfo {
            id,
            addr,
            role: role.to_string(),
            master_id: if master_field == "-" {
                String::new()
            } else {
                master_field.to_string()
            },
            flags,
            slots,
            used_memory: 0,
            connected_clients: 0,
        });
    }
    nodes
}
