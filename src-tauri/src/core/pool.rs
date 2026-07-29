use deadpool_redis::cluster::{
    Config as ClusterConfig, Connection as ClusterPoolConnection, Pool as ClusterPool,
};
use deadpool_redis::{Config, Connection as SinglePoolConnection, Pool, Runtime, Timeouts};
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
