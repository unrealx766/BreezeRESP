use redis::cluster_async::ClusterConnection;
use redis::cluster_routing::{
    MultipleNodeRoutingInfo, ResponsePolicy, RoutingInfo, SingleNodeRoutingInfo,
};
use std::collections::HashMap;
use std::sync::Mutex;

use crate::core::pool::parse_node_addr;

/// Per-connection state for a cluster-wide SCAN that iterates every master
/// node sequentially. The frontend keeps its opaque u64 cursor protocol:
/// 0 starts a fresh scan, a synthetic non-zero cursor continues it.
pub struct ClusterScanState {
    nodes: Vec<(String, u16)>,
    node_idx: usize,
    cursor: u64,
}

/// Tracks in-flight cluster scans, keyed by connection ID.
pub struct ClusterScanManager {
    states: Mutex<HashMap<String, ClusterScanState>>,
}

impl ClusterScanManager {
    pub fn new() -> Self {
        Self {
            states: Mutex::new(HashMap::new()),
        }
    }

    fn take(&self, connection_id: &str) -> Result<Option<ClusterScanState>, String> {
        let mut states = self.states.lock().map_err(|e| e.to_string())?;
        Ok(states.remove(connection_id))
    }

    fn store(&self, connection_id: &str, state: ClusterScanState) -> Result<(), String> {
        let mut states = self.states.lock().map_err(|e| e.to_string())?;
        states.insert(connection_id.to_string(), state);
        Ok(())
    }

    /// Drop any scan state for a connection (on disconnect/delete).
    pub fn clear(&self, connection_id: &str) {
        if let Ok(mut states) = self.states.lock() {
            states.remove(connection_id);
        }
    }

    /// Perform one SCAN step across the cluster. Returns `(next_cursor, keys)`
    /// where `next_cursor` is 0 when all master nodes are exhausted, or a
    /// synthetic 1 while the scan is still in progress.
    pub async fn scan_step(
        &self,
        connection_id: &str,
        conn: &mut ClusterConnection,
        cursor: u64,
        pattern: &str,
        count: u64,
    ) -> Result<(u64, Vec<String>), String> {
        // cursor == 0 starts a fresh scan; otherwise resume saved state.
        let mut state = if cursor == 0 {
            self.clear(connection_id);
            ClusterScanState {
                nodes: master_addrs(conn).await?,
                node_idx: 0,
                cursor: 0,
            }
        } else {
            self.take(connection_id)?
                .ok_or_else(|| "Cluster scan state expired; restart the scan".to_string())?
        };

        let Some((host, port)) = state.nodes.get(state.node_idx).cloned() else {
            return Ok((0, Vec::new()));
        };

        let mut cmd = redis::cmd("SCAN");
        cmd.arg(state.cursor)
            .arg("MATCH")
            .arg(pattern)
            .arg("COUNT")
            .arg(count);
        let scan_val = conn
            .route_command(
                &cmd,
                RoutingInfo::SingleNode(SingleNodeRoutingInfo::ByAddress { host, port }),
            )
            .await
            .map_err(|e| format!("SCAN error: {}", e))?;

        // Parse SCAN response: [cursor, [keys...]]
        let elements = match scan_val {
            redis::Value::Array(items) if items.len() == 2 => items,
            other => return Err(format!("Unexpected SCAN response format: {:?}", other)),
        };
        let node_cursor: u64 = redis::from_redis_value(&elements[0])
            .map_err(|e| format!("Failed to parse SCAN cursor: {}", e))?;
        let keys: Vec<String> = redis::from_redis_value(&elements[1])
            .map_err(|e| format!("Failed to parse SCAN keys: {}", e))?;

        // Advance to the next node once the current one is exhausted.
        if node_cursor == 0 {
            state.node_idx += 1;
            state.cursor = 0;
        } else {
            state.cursor = node_cursor;
        }

        if state.node_idx >= state.nodes.len() {
            self.clear(connection_id);
            Ok((0, keys))
        } else {
            self.store(connection_id, state)?;
            Ok((1, keys))
        }
    }
}

impl Default for ClusterScanManager {
    fn default() -> Self {
        Self::new()
    }
}

/// List the addresses of all master nodes by fanning out a PING and reading
/// the per-node response map keys.
pub async fn master_addrs(conn: &mut ClusterConnection) -> Result<Vec<(String, u16)>, String> {
    let val = conn
        .route_command(
            &redis::cmd("PING"),
            RoutingInfo::MultiNode((MultipleNodeRoutingInfo::AllMasters, None)),
        )
        .await
        .map_err(|e| format!("Cluster discovery error: {}", e))?;

    let entries = match val {
        redis::Value::Map(entries) => entries,
        other => return Err(format!("Unexpected cluster response format: {:?}", other)),
    };

    let mut nodes = Vec::with_capacity(entries.len());
    for (addr, _) in &entries {
        let addr: String = redis::from_redis_value(addr)
            .map_err(|e| format!("Failed to parse node address: {}", e))?;
        // Defensive: strip a scheme prefix if present.
        let addr = addr
            .rsplit_once("://")
            .map(|(_, rest)| rest.to_string())
            .unwrap_or(addr);
        if let Some(node) = parse_node_addr(&addr) {
            nodes.push(node);
        }
    }
    if nodes.is_empty() {
        return Err("No cluster master nodes found".to_string());
    }
    nodes.sort();
    Ok(nodes)
}

/// Route a command to all masters and sum the integer responses (e.g. DBSIZE).
pub async fn sum_on_all_masters(
    conn: &mut ClusterConnection,
    cmd: &redis::Cmd,
) -> Result<u64, String> {
    let val = conn
        .route_command(
            cmd,
            RoutingInfo::MultiNode((
                MultipleNodeRoutingInfo::AllMasters,
                Some(ResponsePolicy::Aggregate(redis::cluster_routing::AggregateOp::Sum)),
            )),
        )
        .await
        .map_err(|e| format!("Cluster command error: {}", e))?;
    redis::from_redis_value(&val).map_err(|e| format!("Failed to parse aggregate result: {}", e))
}

/// Route a command to all masters without aggregation, returning the raw
/// per-node responses as `(address, value)` pairs.
pub async fn per_master_values(
    conn: &mut ClusterConnection,
    cmd: &redis::Cmd,
) -> Result<Vec<(String, redis::Value)>, String> {
    let val = conn
        .route_command(
            cmd,
            RoutingInfo::MultiNode((MultipleNodeRoutingInfo::AllMasters, None)),
        )
        .await
        .map_err(|e| format!("Cluster command error: {}", e))?;

    let entries = match val {
        redis::Value::Map(entries) => entries,
        other => return Err(format!("Unexpected cluster response format: {:?}", other)),
    };
    entries
        .into_iter()
        .map(|(addr, value)| {
            let addr: String = redis::from_redis_value(&addr)
                .map_err(|e| format!("Failed to parse node address: {}", e))?;
            Ok((addr, value))
        })
        .collect()
}

/// Route a command to all nodes (masters + replicas) without aggregation,
/// returning the raw per-node responses.
pub async fn per_node_values(
    conn: &mut ClusterConnection,
    cmd: &redis::Cmd,
) -> Result<Vec<(String, redis::Value)>, String> {
    let val = conn
        .route_command(
            cmd,
            RoutingInfo::MultiNode((MultipleNodeRoutingInfo::AllNodes, None)),
        )
        .await
        .map_err(|e| format!("Cluster command error: {}", e))?;

    let entries = match val {
        redis::Value::Map(entries) => entries,
        other => return Err(format!("Unexpected cluster response format: {:?}", other)),
    };
    entries
        .into_iter()
        .map(|(addr, value)| {
            let addr: String = redis::from_redis_value(&addr)
                .map_err(|e| format!("Failed to parse node address: {}", e))?;
            Ok((addr, value))
        })
        .collect()
}
