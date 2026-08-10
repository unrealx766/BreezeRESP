use deadpool_redis::cluster::{
    Config as ClusterConfig, Connection as ClusterPoolConnection, Pool as ClusterPool,
};
use deadpool_redis::{Config, Connection as SinglePoolConnection, Pool, Runtime, Timeouts};
use futures::stream::{self, StreamExt};
use redis::aio::ConnectionLike;
use redis::cluster_async::ClusterConnection;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

const POOL_TIMEOUTS: Timeouts = Timeouts {
    wait: Some(Duration::from_secs(10)),
    create: Some(Duration::from_secs(10)),
    recycle: Some(Duration::from_secs(5)),
};

/// A pool that is either a standalone Redis pool or a Redis Cluster pool.
#[derive(Clone)]
pub enum AnyPool {
    Single(Pool),
    Cluster(ClusterPool),
}

impl AnyPool {
    /// Whether this pool targets a Redis Cluster.
    pub fn is_cluster(&self) -> bool {
        matches!(self, AnyPool::Cluster(_))
    }

    /// Get a connection from the pool (standalone or cluster).
    pub async fn get(&self) -> Result<AnyConn, String> {
        match self {
            AnyPool::Single(pool) => pool
                .get()
                .await
                .map(AnyConn::Single)
                .map_err(|e| e.to_string()),
            AnyPool::Cluster(pool) => pool
                .get()
                .await
                .map(AnyConn::Cluster)
                .map_err(|e| e.to_string()),
        }
    }
}

/// A pooled connection that is either standalone or cluster-aware.
/// Implements `ConnectionLike`, so `redis::cmd(...).query_async(&mut conn)`
/// and the `AsyncCommands` trait work transparently for both variants.
pub enum AnyConn {
    Single(SinglePoolConnection),
    Cluster(ClusterPoolConnection),
}

impl AnyConn {
    /// Access the underlying cluster connection for cluster-specific routing
    /// (e.g. `route_command`). Returns `None` for standalone connections.
    pub fn as_cluster(&mut self) -> Option<&mut ClusterConnection> {
        match self {
            AnyConn::Single(_) => None,
            AnyConn::Cluster(conn) => Some(&mut **conn),
        }
    }
}

impl ConnectionLike for AnyConn {
    fn req_packed_command<'a>(
        &'a mut self,
        cmd: &'a redis::Cmd,
    ) -> redis::RedisFuture<'a, redis::Value> {
        match self {
            AnyConn::Single(conn) => conn.req_packed_command(cmd),
            AnyConn::Cluster(conn) => conn.req_packed_command(cmd),
        }
    }

    fn req_packed_commands<'a>(
        &'a mut self,
        cmd: &'a redis::Pipeline,
        offset: usize,
        count: usize,
    ) -> redis::RedisFuture<'a, Vec<redis::Value>> {
        match self {
            AnyConn::Single(conn) => conn.req_packed_commands(cmd, offset, count),
            AnyConn::Cluster(conn) => conn.req_packed_commands(cmd, offset, count),
        }
    }

    fn get_db(&self) -> i64 {
        match self {
            AnyConn::Single(conn) => conn.get_db(),
            AnyConn::Cluster(conn) => conn.get_db(),
        }
    }
}

/// Build a single-node Redis URL (`redis[s]://[:pw@]host:port[/db]`).
fn build_url(host: &str, port: u16, password: Option<&str>, db: Option<u8>, ssl: bool) -> String {
    let scheme = if ssl { "rediss" } else { "redis" };
    let auth = match password {
        Some(pw) if !pw.is_empty() => format!(":{}@", urlencoding::encode(pw)),
        _ => String::new(),
    };
    match db {
        Some(db) => format!("{}://{}{}:{}/{}", scheme, auth, host, port, db),
        None => format!("{}://{}{}:{}", scheme, auth, host, port),
    }
}

/// Manages multiple Redis connection pools, keyed by connection ID.
pub struct ConnectionPoolManager {
    pools: Mutex<HashMap<String, AnyPool>>,
}

impl ConnectionPoolManager {
    pub fn new() -> Self {
        Self {
            pools: Mutex::new(HashMap::new()),
        }
    }

    /// Create or retrieve a connection pool for the given connection config.
    /// For cluster mode, `nodes` holds extra seed nodes (`host:port`) beyond
    /// the primary `host`/`port`, and `db` is ignored (cluster is always db 0).
    #[allow(clippy::too_many_arguments)]
    pub fn get_or_create(
        &self,
        id: &str,
        host: &str,
        port: u16,
        password: Option<&str>,
        db: u8,
        ssl: bool,
        cluster: bool,
        nodes: &[String],
    ) -> Result<AnyPool, String> {
        let mut pools = self.pools.lock().map_err(|e| e.to_string())?;

        if let Some(pool) = pools.get(id) {
            return Ok(pool.clone());
        }

        let pool = if cluster {
            // Seed URLs: primary host:port first, then extra nodes (no db path).
            let mut urls = vec![build_url(host, port, password, None, ssl)];
            for node in nodes {
                if let Some((n_host, n_port)) = parse_node_addr(node) {
                    urls.push(build_url(&n_host, n_port, password, None, ssl));
                }
            }

            let cfg = ClusterConfig::from_urls(urls);
            let pool = cfg
                .builder()
                .map_err(|e| format!("Failed to build cluster pool: {}", e))?
                .runtime(Runtime::Tokio1)
                .timeouts(POOL_TIMEOUTS)
                .build()
                .map_err(|e| format!("Failed to create cluster pool: {}", e))?;
            AnyPool::Cluster(pool)
        } else {
            let cfg = Config::from_url(build_url(host, port, password, Some(db), ssl));
            let pool = cfg
                .builder()
                .map_err(|e| format!("Failed to build pool: {}", e))?
                .runtime(Runtime::Tokio1)
                .timeouts(POOL_TIMEOUTS)
                .build()
                .map_err(|e| format!("Failed to create pool: {}", e))?;
            AnyPool::Single(pool)
        };

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
    pub fn get_pool(&self, id: &str) -> Result<AnyPool, String> {
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

/// Parse a `host:port` node address. Returns `None` when malformed.
pub fn parse_node_addr(addr: &str) -> Option<(String, u16)> {
    let (host, port) = addr.trim().rsplit_once(':')?;
    if host.is_empty() {
        return None;
    }
    let port: u16 = port.parse().ok()?;
    Some((host.to_string(), port))
}

const SEED_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum number of topology-advertised nodes to probe.
const MAX_TOPOLOGY_PROBES: usize = 32;

/// Probe one node with a standalone connection attempt (TCP + AUTH + PING)
/// and return a human-readable result.
async fn probe_node(host: &str, port: u16, password: Option<&str>, ssl: bool) -> String {
    let url = build_url(host, port, password, None, ssl);
    let probe = async {
        let client = redis::Client::open(url).map_err(|e| e.to_string())?;
        let mut conn: redis::aio::MultiplexedConnection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| e.to_string())?;
        let _: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    };
    match tokio::time::timeout(SEED_PROBE_TIMEOUT, probe).await {
        Ok(Ok(())) => "ok".to_string(),
        Ok(Err(err)) => err.lines().next().unwrap_or("unknown error").to_string(),
        Err(_) => format!("timeout after {}s", SEED_PROBE_TIMEOUT.as_secs()),
    }
}

/// Fetch the node addresses the cluster advertises via `CLUSTER NODES`
/// (lines look like `<id> <ip:port@cport> <flags> ...`). Returns `None`
/// when the seed cannot be queried.
async fn fetch_advertised_nodes(
    host: &str,
    port: u16,
    password: Option<&str>,
    ssl: bool,
) -> Option<Vec<String>> {
    let url = build_url(host, port, password, None, ssl);
    let query = async {
        let client = redis::Client::open(url).ok()?;
        let mut conn: redis::aio::MultiplexedConnection = client
            .get_multiplexed_async_connection()
            .await
            .ok()?;
        let raw: String = redis::cmd("CLUSTER")
            .arg("NODES")
            .query_async(&mut conn)
            .await
            .ok()?;
        Some(raw)
    };
    let raw = tokio::time::timeout(SEED_PROBE_TIMEOUT, query)
        .await
        .ok()
        .flatten()?;
    Some(
        raw.lines()
            .filter_map(|line| line.split_whitespace().nth(1))
            .map(|field| field.split('@').next().unwrap_or(field).to_string())
            .collect(),
    )
}

/// Diagnose cluster connection failures by probing each seed node and every
/// node address advertised by the cluster topology, returning a
/// human-readable summary. redis-rs swallows the per-node errors behind a
/// generic "Failed to create initial connections" IoError, and silently
/// drops unreachable topology nodes (making later commands hang), so this
/// surfaces the real cause (DNS failure, unreachable node, auth error,
/// TLS failure, ...).
pub async fn diagnose_cluster_seeds(
    host: &str,
    port: u16,
    password: Option<&str>,
    ssl: bool,
    nodes: &[String],
) -> String {
    let mut seeds = vec![(host.to_string(), port)];
    for node in nodes {
        if let Some(parsed) = parse_node_addr(node) {
            seeds.push(parsed);
        }
    }

    let mut parts = Vec::with_capacity(seeds.len());
    for (seed_host, seed_port) in &seeds {
        let label = format!("{}:{}", seed_host, seed_port);
        let detail = probe_node(seed_host, *seed_port, password, ssl).await;
        parts.push(format!("{} => {}", label, detail));
    }

    // The cluster client drops the seed connection map once topology is
    // discovered and only talks to the advertised nodes; if those are
    // unreachable from the client, commands hang. Query CLUSTER NODES from
    // the first reachable seed and probe every advertised address too.
    let mut topo_parts: Vec<String> = Vec::new();
    for (seed_host, seed_port) in &seeds {
        let Some(advertised) = fetch_advertised_nodes(seed_host, *seed_port, password, ssl).await
        else {
            continue;
        };
        let mut addrs: Vec<(String, u16)> =
            advertised.iter().filter_map(|a| parse_node_addr(a)).collect();
        addrs.sort();
        addrs.dedup();
        topo_parts = stream::iter(addrs.into_iter().take(MAX_TOPOLOGY_PROBES))
            .map(|(addr_host, addr_port)| async move {
                let detail = probe_node(&addr_host, addr_port, password, ssl).await;
                format!("{}:{} => {}", addr_host, addr_port, detail)
            })
            .buffer_unordered(8)
            .collect()
            .await;
        topo_parts.sort();
        break;
    }

    if topo_parts.is_empty() {
        format!("seed node check: {}", parts.join("; "))
    } else {
        format!(
            "seed node check: {} | advertised nodes: {}",
            parts.join("; "),
            topo_parts.join("; ")
        )
    }
}
